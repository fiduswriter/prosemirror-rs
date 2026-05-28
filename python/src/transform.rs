use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;
use std::sync::Arc;

use crate::model::{
    json_to_py, py_to_json, PyFragment, PyMark, PyNode, PyNodeRange, PyNodeType, PyResolvedPos,
    PySchema,
};
use prosemirror::dynamic::types::{Dyn, DynamicNode, DynamicNodeType};
use prosemirror::dynamic::DynamicSchema;
use prosemirror::model::{MarkSet, NodeType as _, Schema, Slice};
use prosemirror::transform::{
    map::{MapResult, Mappable, Mapping, StepMap},
    structure::{
        can_join as rs_can_join, can_split as rs_can_split, drop_point as rs_drop_point,
        find_wrapping as rs_find_wrapping, insert_point as rs_insert_point,
        join_point as rs_join_point, lift_target as rs_lift_target, NodeRange,
    },
    AddMarkStep, RemoveMarkStep, ReplaceStep, Step, Transform,
};

// ---------------------------------------------------------------------------
// StepMap
// ---------------------------------------------------------------------------

#[pyclass(name = "StepMap")]
pub struct PyStepMap {
    inner: StepMap,
}

#[pymethods]
impl PyStepMap {
    #[new]
    fn new(ranges: Vec<usize>) -> Self {
        PyStepMap {
            inner: StepMap::new(ranges),
        }
    }

    #[getter]
    fn ranges(&self) -> Vec<usize> {
        self.inner.ranges.clone()
    }

    fn map(&self, pos: usize, assoc: i32) -> usize {
        self.inner.map(pos, assoc)
    }

    fn map_result(&self, pos: usize, assoc: i32) -> PyMapResult {
        PyMapResult {
            inner: self.inner.map_result(pos, assoc),
        }
    }

    fn recover(&self, value: usize) -> Option<usize> {
        self.inner.recover(value)
    }

    fn invert(&self) -> PyStepMap {
        PyStepMap {
            inner: self.inner.invert(),
        }
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        if let Ok(other) = other.cast::<PyStepMap>() {
            let other = other.borrow();
            match op {
                pyo3::basic::CompareOp::Eq => Ok(self.inner == other.inner),
                pyo3::basic::CompareOp::Ne => Ok(self.inner != other.inner),
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }
}

// ---------------------------------------------------------------------------
// MapResult
// ---------------------------------------------------------------------------

#[pyclass(name = "MapResult")]
pub struct PyMapResult {
    inner: MapResult,
}

#[pymethods]
impl PyMapResult {
    #[getter]
    fn pos(&self) -> usize {
        self.inner.pos
    }

    #[getter]
    fn deleted(&self) -> bool {
        self.inner.deleted()
    }

    #[getter]
    fn deleted_before(&self) -> bool {
        self.inner.deleted_before()
    }

    #[getter]
    fn deleted_after(&self) -> bool {
        self.inner.deleted_after()
    }

    #[getter]
    fn deleted_across(&self) -> bool {
        self.inner.deleted_across()
    }

    #[getter]
    fn recover(&self) -> Option<usize> {
        self.inner.recover
    }
}

// ---------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------

#[pyclass(name = "Mapping")]
pub struct PyMapping {
    inner: Mapping,
}

#[pymethods]
impl PyMapping {
    #[new]
    #[pyo3(signature = (maps = None))]
    fn new(maps: Option<&Bound<'_, PyList>>) -> PyResult<Self> {
        let mut mapping = Mapping::new();
        if let Some(list) = maps {
            for item in list.iter() {
                let map = item.cast::<PyStepMap>()?.borrow();
                mapping.append_map(map.inner.clone(), None);
            }
        }
        Ok(PyMapping { inner: mapping })
    }

    #[pyo3(signature = (map, mirrors=None))]
    fn append_map(&mut self, map: &PyStepMap, mirrors: Option<usize>) {
        self.inner.append_map(map.inner.clone(), mirrors);
    }

    fn set_mirror(&mut self, n: usize, m: usize) {
        self.inner.set_mirror(n, m);
    }

    fn get_mirror(&self, n: usize) -> Option<usize> {
        self.inner.get_mirror(n)
    }

    fn invert(&self) -> PyMapping {
        PyMapping {
            inner: self.inner.invert(),
        }
    }

    fn map(&self, pos: usize, assoc: i32) -> usize {
        self.inner.map(pos, assoc)
    }

    fn map_result(&self, pos: usize, assoc: i32) -> PyMapResult {
        PyMapResult {
            inner: self.inner.map_result(pos, assoc),
        }
    }

    fn slice(&self, from: usize, to: Option<usize>) -> PyMapping {
        PyMapping {
            inner: self.inner.slice(from, to),
        }
    }

    #[getter]
    fn maps(&self) -> Vec<PyStepMap> {
        self.inner
            .maps
            .iter()
            .map(|m| PyStepMap { inner: m.clone() })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// StepResult
// ---------------------------------------------------------------------------

#[pyclass(name = "StepResult")]
pub struct PyStepResult {
    doc: Option<PyNode>,
    failed: Option<String>,
}

#[pymethods]
impl PyStepResult {
    #[getter]
    fn doc(&self) -> Option<PyNode> {
        self.doc.as_ref().map(|n| PyNode {
            schema: n.schema.clone(),
            inner: n.inner.clone(),
        })
    }

    #[getter]
    fn failed(&self) -> Option<String> {
        self.failed.clone()
    }
}

fn wrap_step_result(
    schema: Arc<DynamicSchema>,
    result: Result<
        prosemirror::dynamic::types::DynamicNode,
        prosemirror::transform::StepError<Dyn>,
    >,
) -> PyResult<PyStepResult> {
    match result {
        Ok(doc) => Ok(PyStepResult {
            doc: Some(PyNode {
                schema: schema.clone(),
                inner: doc,
            }),
            failed: None,
        }),
        Err(e) => Ok(PyStepResult {
            doc: None,
            failed: Some(format!("{e:?}")),
        }),
    }
}

// ---------------------------------------------------------------------------
// Step
// ---------------------------------------------------------------------------

#[pyclass(name = "Step")]
pub struct PyStep {
    inner: Step<Dyn>,
}

#[pymethods]
impl PyStep {
    #[staticmethod]
    fn from_json(schema: &PySchema, json: &Bound<'_, PyAny>) -> PyResult<PyStep> {
        let val = py_to_json(json)?;
        let step = schema
            .inner
            .with_types(|| serde_json::from_value::<Step<Dyn>>(val))
            .map_err(|e| PyValueError::new_err(format!("Invalid step JSON: {e}")))?;
        Ok(PyStep { inner: step })
    }

    fn to_json(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let val =
            serde_json::to_value(&self.inner).map_err(|e| PyValueError::new_err(format!("{e}")))?;
        json_to_py(py, &val).map(|b| b.unbind())
    }

    fn apply(&self, doc: &PyNode) -> PyResult<PyStepResult> {
        let result = doc.schema.with_types(|| self.inner.apply(&doc.inner));
        wrap_step_result(doc.schema.clone(), result)
    }

    fn get_map(&self) -> PyStepMap {
        PyStepMap {
            inner: self.inner.get_map(),
        }
    }

    fn invert(&self, doc: &PyNode) -> PyResult<PyStep> {
        let step = doc.schema.with_types(|| self.inner.invert(&doc.inner));
        Ok(PyStep { inner: step })
    }

    fn map(&self, mapping: &PyMapping) -> PyResult<Option<PyStep>> {
        let step = self.inner.map(&mapping.inner);
        Ok(step.map(|s| PyStep { inner: s }))
    }

    fn merge(&self, other: &PyStep) -> PyResult<Option<PyStep>> {
        let step = self.inner.merge(&other.inner);
        Ok(step.map(|s| PyStep { inner: s }))
    }

    #[staticmethod]
    fn replace(
        from: usize,
        to: usize,
        slice: Option<&crate::model::PySlice>,
        structure: Option<bool>,
    ) -> PyResult<PyStep> {
        let slice = slice.map(|s| s.inner.clone()).unwrap_or_default();
        Ok(PyStep {
            inner: Step::Replace(ReplaceStep {
                span: prosemirror::transform::Span { from, to },
                slice,
                structure: structure.unwrap_or(false),
            }),
        })
    }

    #[staticmethod]
    fn add_mark(from: usize, to: usize, mark: &PyMark) -> PyResult<PyStep> {
        Ok(PyStep {
            inner: Step::AddMark(AddMarkStep {
                span: prosemirror::transform::Span { from, to },
                mark: mark.inner.clone(),
            }),
        })
    }

    #[staticmethod]
    fn remove_mark(from: usize, to: usize, mark: &PyMark) -> PyResult<PyStep> {
        Ok(PyStep {
            inner: Step::RemoveMark(RemoveMarkStep {
                span: prosemirror::transform::Span { from, to },
                mark: mark.inner.clone(),
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// Transform
// ---------------------------------------------------------------------------

#[pyclass(name = "Transform")]
pub struct PyTransform {
    schema: Arc<DynamicSchema>,
    inner: Transform<Dyn>,
    before: Py<PyNode>,
}

#[pymethods]
impl PyTransform {
    #[new]
    fn new(doc: &Bound<'_, PyNode>) -> PyResult<Self> {
        let node = doc.borrow();
        Ok(PyTransform {
            schema: node.schema.clone(),
            inner: Transform::new(node.inner.clone()),
            before: doc.clone().unbind(),
        })
    }

    #[getter]
    fn doc(&self) -> PyResult<PyNode> {
        Ok(PyNode {
            schema: self.schema.clone(),
            inner: self.inner.doc.clone(),
        })
    }

    #[getter]
    fn before<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyNode>> {
        Ok(self.before.bind(py).clone())
    }

    #[getter]
    fn steps(&self) -> PyResult<Vec<PyStep>> {
        Ok(self
            .inner
            .steps
            .iter()
            .map(|s| PyStep { inner: s.clone() })
            .collect())
    }

    #[getter]
    fn docs(&self) -> PyResult<Vec<PyNode>> {
        Ok(self
            .inner
            .docs
            .iter()
            .map(|d| PyNode {
                schema: self.schema.clone(),
                inner: d.clone(),
            })
            .collect())
    }

    #[getter]
    fn mapping(&self) -> PyMapping {
        PyMapping {
            inner: self.inner.mapping.clone(),
        }
    }

    fn doc_changed(&self) -> bool {
        self.inner.doc_changed()
    }

    fn step(slf: &Bound<'_, Self>, step: &PyStep) -> PyResult<Py<Self>> {
        let result = {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| this.inner.step(step.inner.clone()).map(|_| ()))
        };
        match result {
            Ok(_) => Ok(slf.clone().unbind()),
            Err(e) => Err(PyValueError::new_err(format!("{e:?}"))),
        }
    }

    fn replace(
        slf: &Bound<'_, Self>,
        from: usize,
        to: Option<usize>,
        slice: Option<&crate::model::PySlice>,
    ) -> PyResult<Py<Self>> {
        let slice = slice.map(|s| s.inner.clone());
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.replace(from, to, slice);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn replace_with(
        slf: &Bound<'_, Self>,
        from: usize,
        to: usize,
        content: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            let fragment = crate::model::extract_fragment(content, &schema)?;
            schema.with_types(|| {
                this.inner.replace_with(from, to, fragment);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn delete(slf: &Bound<'_, Self>, from: usize, to: usize) -> PyResult<Py<Self>> {
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.delete(from, to);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn insert(slf: &Bound<'_, Self>, pos: usize, content: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            let fragment = crate::model::extract_fragment(content, &schema)?;
            schema.with_types(|| {
                this.inner.insert(pos, fragment);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn add_mark(
        slf: &Bound<'_, Self>,
        from: usize,
        to: usize,
        mark: &PyMark,
    ) -> PyResult<Py<Self>> {
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.add_mark(from, to, mark.inner.clone());
            });
        }
        Ok(slf.clone().unbind())
    }

    fn remove_mark(
        slf: &Bound<'_, Self>,
        from: usize,
        to: usize,
        mark: Option<&PyMark>,
    ) -> PyResult<Py<Self>> {
        let mark = mark.map(|m| m.inner.clone());
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.remove_mark(from, to, mark);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn add_node_mark(slf: &Bound<'_, Self>, pos: usize, mark: &PyMark) -> PyResult<Py<Self>> {
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.add_node_mark(pos, mark.inner.clone());
            });
        }
        Ok(slf.clone().unbind())
    }

    fn remove_node_mark(slf: &Bound<'_, Self>, pos: usize, mark: &PyMark) -> PyResult<Py<Self>> {
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.remove_node_mark(pos, mark.inner.clone());
            });
        }
        Ok(slf.clone().unbind())
    }

    fn set_node_attribute(
        slf: &Bound<'_, Self>,
        pos: usize,
        attr: &str,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let value = py_to_json(value)?;
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.set_node_attribute(pos, attr, value);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn set_doc_attribute(
        slf: &Bound<'_, Self>,
        attr: &str,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let value = py_to_json(value)?;
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.set_doc_attribute(attr, value);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn maybe_step(slf: &Bound<'_, Self>, step: &PyStep) -> PyResult<Option<String>> {
        let result = {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| this.inner.maybe_step(step.inner.clone()))
        };
        Ok(result)
    }

    #[pyo3(signature = (pos, depth = 1, types_after = None))]
    fn split(
        slf: &Bound<'_, Self>,
        pos: usize,
        depth: Option<usize>,
        types_after: Option<&Bound<'_, PyList>>,
    ) -> PyResult<Py<Self>> {
        let types_after = types_after.map(|list| {
            list.iter()
                .map(|item: Bound<'_, PyAny>| {
                    let nt = item.cast::<PyNodeType>().unwrap().borrow();
                    DynamicNodeType { idx: nt.inner.idx }
                })
                .collect::<Vec<_>>()
        });
        let types_after_ref = types_after.as_deref();
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.split(pos, depth, types_after_ref);
            });
        }
        Ok(slf.clone().unbind())
    }

    #[pyo3(signature = (pos, depth = 1))]
    fn join(slf: &Bound<'_, Self>, pos: usize, depth: Option<usize>) -> PyResult<Py<Self>> {
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.join(pos, depth);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn lift(slf: &Bound<'_, Self>, range: &PyNodeRange, target: usize) -> PyResult<Py<Self>> {
        let nr = to_node_range(range)?;
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.lift(&nr, target);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn wrap(
        slf: &Bound<'_, Self>,
        range: &PyNodeRange,
        wrappers: &Bound<'_, PyList>,
    ) -> PyResult<Py<Self>> {
        let mut wrapper_types = Vec::new();
        for item in wrappers.iter() {
            let nt = item.cast::<PyNodeType>()?.borrow();
            wrapper_types.push((DynamicNodeType { idx: nt.inner.idx }, None));
        }
        let nr = to_node_range(range)?;
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.wrap(&nr, &wrapper_types);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn set_block_type(
        slf: &Bound<'_, Self>,
        from: usize,
        to: usize,
        node_type: &PyNodeType,
    ) -> PyResult<Py<Self>> {
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.set_block_type(
                    from,
                    to,
                    DynamicNodeType {
                        idx: node_type.inner.idx,
                    },
                );
            });
        }
        Ok(slf.clone().unbind())
    }

    #[pyo3(signature = (pos, node_type = None, marks = None))]
    fn set_node_markup(
        slf: &Bound<'_, Self>,
        pos: usize,
        node_type: Option<&PyNodeType>,
        marks: Option<&Bound<'_, PyList>>,
    ) -> PyResult<Py<Self>> {
        let node_type = node_type.map(|nt| DynamicNodeType { idx: nt.inner.idx });
        let marks = marks
            .map(|list| {
                let mut mark_set = MarkSet::new();
                for item in list.iter() {
                    let mark = item.cast::<PyMark>()?.borrow();
                    mark_set.add(&mark.inner);
                }
                Ok::<_, PyErr>(mark_set)
            })
            .transpose()?;
        {
            let mut this = slf.borrow_mut();
            let schema = this.schema.clone();
            schema.with_types(|| {
                this.inner.set_node_markup(pos, node_type, marks);
            });
        }
        Ok(slf.clone().unbind())
    }

    fn changed_range(&self) -> PyResult<Option<(usize, usize)>> {
        Ok(self.inner.changed_range())
    }
}

// ---------------------------------------------------------------------------
// Structure functions
// ---------------------------------------------------------------------------

#[pyfunction(name = "lift_target")]
pub fn py_lift_target(range: &PyNodeRange) -> PyResult<Option<usize>> {
    let nr = to_node_range(range)?;
    Ok(range.schema.with_types(|| rs_lift_target(&nr)))
}

#[pyfunction(name = "can_split")]
pub fn py_can_split(
    doc: &PyNode,
    pos: usize,
    depth: Option<usize>,
    types_after: Option<&Bound<'_, PyList>>,
) -> PyResult<bool> {
    let types_after = types_after.map(|list| {
        list.iter()
            .map(|item: Bound<'_, PyAny>| {
                let nt = item.cast::<PyNodeType>().unwrap().borrow();
                DynamicNodeType { idx: nt.inner.idx }
            })
            .collect::<Vec<_>>()
    });
    let types_after_ref = types_after.as_deref();
    Ok(doc
        .schema
        .with_types(|| rs_can_split::<Dyn>(&doc.inner, pos, depth, types_after_ref)))
}

#[pyfunction(name = "find_wrapping")]
pub fn py_find_wrapping(
    range: &PyNodeRange,
    node_type: &PyNodeType,
) -> PyResult<Option<Vec<PyNodeType>>> {
    let range = to_node_range(range)?;
    let result = node_type.schema.with_types(|| {
        rs_find_wrapping(
            &range,
            DynamicNodeType {
                idx: node_type.inner.idx,
            },
            |_nt| true,
        )
    });
    Ok(result.map(|types| {
        types
            .iter()
            .map(|t| PyNodeType {
                schema: node_type.schema.clone(),
                inner: *t,
                name: String::new(),
            })
            .collect()
    }))
}

#[pyfunction(name = "can_join")]
pub fn py_can_join(doc: &PyNode, pos: usize) -> PyResult<bool> {
    Ok(doc
        .schema
        .with_types(|| rs_can_join::<Dyn>(&doc.inner, pos).unwrap_or(false)))
}

#[pyfunction(name = "join_point")]
pub fn py_join_point(doc: &PyNode, pos: usize, dir: Option<i32>) -> PyResult<Option<usize>> {
    Ok(doc
        .schema
        .with_types(|| rs_join_point::<Dyn>(&doc.inner, pos, dir)))
}

#[pyfunction(name = "insert_point")]
pub fn py_insert_point(
    doc: &PyNode,
    pos: usize,
    node_type: &PyNodeType,
) -> PyResult<Option<usize>> {
    Ok(doc.schema.with_types(|| {
        rs_insert_point::<Dyn>(
            &doc.inner,
            pos,
            DynamicNodeType {
                idx: node_type.inner.idx,
            },
        )
    }))
}

#[pyfunction(name = "drop_point")]
pub fn py_drop_point(
    doc: &PyNode,
    pos: usize,
    slice: &crate::model::PySlice,
) -> PyResult<Option<usize>> {
    Ok(doc
        .schema
        .with_types(|| rs_drop_point::<Dyn>(&doc.inner, pos, &slice.inner)))
}

// ---------------------------------------------------------------------------
// NodeRange wrapper
// ---------------------------------------------------------------------------

pub(crate) fn to_node_range(nr: &crate::model::PyNodeRange) -> PyResult<NodeRange<'_, Dyn>> {
    nr.schema.with_types(|| {
        let from = prosemirror::model::ResolvedPos::resolve(&nr.from_doc, nr.from_pos)
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        let to = prosemirror::model::ResolvedPos::resolve(&nr.from_doc, nr.to_pos)
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(NodeRange::new(from, to, nr.depth))
    })
}
