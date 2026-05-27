use std::sync::Arc;

use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString};

use prosemirror::dynamic::types::{
    Dyn, DynamicMark, DynamicMarkType, DynamicNode, DynamicNodeType,
};
use prosemirror::dynamic::DynamicSchema;
use prosemirror::model::{Fragment, MarkSet, Node, NodeType, ResolvedPos, Slice};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn py_to_json(obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = obj.cast::<pyo3::types::PyBool>() {
        Ok(serde_json::Value::Bool(b.is_true()))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(serde_json::Value::Number(i.into()))
    } else if let Ok(f) = obj.extract::<f64>() {
        serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .ok_or_else(|| PyValueError::new_err("Invalid float for JSON"))
    } else if let Ok(s) = obj.cast::<PyString>() {
        Ok(serde_json::Value::String(s.to_str()?.to_string()))
    } else if let Ok(list) = obj.cast::<PyList>() {
        let mut arr = Vec::new();
        for item in list.iter() {
            arr.push(py_to_json(&item)?);
        }
        Ok(serde_json::Value::Array(arr))
    } else if let Ok(dict) = obj.cast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key = k.cast::<PyString>()?.to_str()?.to_string();
            map.insert(key, py_to_json(&v)?);
        }
        Ok(serde_json::Value::Object(map))
    } else {
        Err(PyValueError::new_err(format!(
            "Cannot convert {} to JSON",
            obj.get_type().name()?
        )))
    }
}

pub fn json_to_py<'py>(py: Python<'py>, val: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    match val {
        serde_json::Value::Null => Ok(py.None().into_bound(py)),
        serde_json::Value::Bool(b) => Ok(pyo3::types::PyBool::new(py, *b).to_owned().into_any()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_pyobject(py)?.to_owned().into_any())
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_pyobject(py)?.to_owned().into_any())
            } else {
                Err(PyValueError::new_err("Unsupported JSON number"))
            }
        }
        serde_json::Value::String(s) => Ok(s.clone().into_pyobject(py)?.to_owned().into_any()),
        serde_json::Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(json_to_py(py, item)?)?;
            }
            Ok(list.into_any())
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map.iter() {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            Ok(dict.into_any())
        }
    }
}

fn wrap_marks(name: &str, marks: &MarkSet<Dyn>) -> String {
    let mut result = name.to_string();
    for m in marks.iter().rev() {
        result = format!("{}({})", m.type_name, result);
    }
    result
}

pub fn extract_fragment(obj: &Bound<'_, PyAny>, schema: &DynamicSchema) -> PyResult<Fragment<Dyn>> {
    if obj.is_none() {
        return Ok(Fragment::new());
    }
    if let Ok(frag) = obj.cast::<PyFragment>() {
        return Ok(frag.borrow().inner.clone());
    }
    if let Ok(list) = obj.cast::<PyList>() {
        let mut nodes = Vec::new();
        for item in list.iter() {
            let node = item.cast::<PyNode>()?.borrow().inner.clone();
            nodes.push(node);
        }
        return Ok(schema.with_types(|| Fragment::from(nodes)));
    }
    if let Ok(node) = obj.cast::<PyNode>() {
        return Ok(schema.with_types(|| Fragment::from(vec![node.borrow().inner.clone()])));
    }
    Err(PyValueError::new_err(
        "Expected Fragment, list of Node, or Node",
    ))
}

fn extract_markset(obj: &Bound<'_, PyAny>) -> PyResult<MarkSet<Dyn>> {
    if let Ok(set) = obj.cast::<PyMarkSet>() {
        return Ok(set.borrow().inner.clone());
    }
    if let Ok(list) = obj.cast::<PyList>() {
        let mut marks = MarkSet::new();
        for item in list.iter() {
            let mark = item.cast::<PyMark>()?.borrow().inner.clone();
            marks.add(&mark);
        }
        return Ok(marks);
    }
    Err(PyValueError::new_err("Expected MarkSet or list of Mark"))
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

#[pyclass(name = "Schema")]
pub struct PySchema {
    pub(crate) inner: Arc<DynamicSchema>,
    spec: serde_json::Value,
}

impl PySchema {
    pub(crate) fn from_arc(arc: Arc<DynamicSchema>) -> Self {
        Self {
            inner: arc,
            spec: serde_json::Value::Null,
        }
    }
}

#[pymethods]
impl PySchema {
    #[new]
    fn new(spec: &Bound<'_, PyAny>) -> PyResult<Self> {
        let json = py_to_json(spec)?;
        let schema = DynamicSchema::from_json(&json)
            .map_err(|e| PyValueError::new_err(format!("Invalid schema: {e}")))?;
        Ok(PySchema {
            inner: Arc::new(schema),
            spec: json,
        })
    }

    #[getter]
    fn nodes(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        self.inner.with_types(|| {
            for (name, idx) in &self.inner.node_type_map {
                let nt = PyNodeType {
                    schema: self.inner.clone(),
                    inner: DynamicNodeType { idx: *idx },
                    name: name.clone(),
                };
                dict.set_item(name, nt)?;
            }
            Ok::<_, PyErr>(())
        });
        Ok(dict.unbind())
    }

    #[getter]
    fn marks(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        self.inner.with_types(|| {
            for (name, idx) in &self.inner.mark_type_map {
                let mt = PyMarkType {
                    schema: self.inner.clone(),
                    inner: DynamicMarkType { idx: *idx },
                    name: name.clone(),
                };
                dict.set_item(name, mt)?;
            }
            Ok::<_, PyErr>(())
        });
        Ok(dict.unbind())
    }

    fn node_from_json(&self, json: &Bound<'_, PyAny>) -> PyResult<PyNode> {
        let val = py_to_json(json)?;
        let node = self
            .inner
            .node_from_json(&val)
            .map_err(|e| PyValueError::new_err(format!("Invalid node JSON: {e}")))?;
        Ok(PyNode {
            schema: self.inner.clone(),
            inner: node,
        })
    }

    fn mark_from_json(&self, json: &Bound<'_, PyAny>) -> PyResult<PyMark> {
        let val = py_to_json(json)?;
        let mark = self
            .inner
            .mark_from_json(&val)
            .map_err(|e| PyValueError::new_err(format!("Invalid mark JSON: {e}")))?;
        Ok(PyMark {
            schema: self.inner.clone(),
            inner: mark,
        })
    }

    #[pyo3(signature = (text, marks=None))]
    fn text(&self, text: &str, marks: Option<&Bound<'_, PyAny>>) -> PyResult<PyNode> {
        let mut node = self.inner.with_types(|| DynamicNode::text(text));
        if let Some(marks) = marks {
            let marks = extract_markset(marks)?;
            node = self.inner.with_types(|| node.mark(marks));
        }
        Ok(PyNode {
            schema: self.inner.clone(),
            inner: node,
        })
    }

    #[pyo3(signature = (type_name, attrs=None, content=None, marks=None))]
    fn node(
        &self,
        type_name: &str,
        attrs: Option<&Bound<'_, PyAny>>,
        content: Option<&Bound<'_, PyAny>>,
        marks: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyNode> {
        let attrs = attrs
            .map(py_to_json)
            .unwrap_or(Ok(serde_json::Value::Null))?;
        let content = content
            .map(|c| extract_fragment(c, &self.inner))
            .unwrap_or(Ok(Fragment::new()))?;
        let marks = marks.map(extract_markset).unwrap_or(Ok(MarkSet::new()))?;
        let node = self
            .inner
            .node(type_name, attrs, content, marks)
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(PyNode {
            schema: self.inner.clone(),
            inner: node,
        })
    }

    #[getter]
    fn spec(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &self.spec).map(|b| b.unbind())
    }

    #[pyo3(signature = (type_name, attrs=None))]
    fn mark(&self, type_name: &str, attrs: Option<&Bound<'_, PyAny>>) -> PyResult<PyMark> {
        let attrs = attrs
            .map(py_to_json)
            .unwrap_or(Ok(serde_json::Value::Null))?;
        Ok(PyMark {
            schema: self.inner.clone(),
            inner: DynamicMark {
                type_name: type_name.to_string(),
                attrs,
            },
        })
    }
}

// ---------------------------------------------------------------------------
// NodeType
// ---------------------------------------------------------------------------

#[pyclass(name = "NodeType")]
pub struct PyNodeType {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: DynamicNodeType,
    pub(crate) name: String,
}

#[pymethods]
impl PyNodeType {
    #[getter]
    fn schema(&self) -> PySchema {
        PySchema::from_arc(self.schema.clone())
    }

    #[getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[getter]
    fn is_block(&self) -> bool {
        self.schema.with_types(|| self.inner.is_block())
    }

    #[getter]
    fn is_inline(&self) -> bool {
        self.schema.with_types(|| self.inner.is_inline())
    }

    #[getter]
    fn is_atom(&self) -> bool {
        self.schema.with_types(|| self.inner.is_atom())
    }

    #[getter]
    fn is_textblock(&self) -> bool {
        self.schema.with_types(|| self.inner.is_textblock())
    }

    #[getter]
    fn inline_content(&self) -> bool {
        self.schema.with_types(|| self.inner.inline_content())
    }

    #[getter]
    fn is_leaf(&self) -> bool {
        self.is_atom()
    }

    #[pyo3(signature = (attrs=None, content=None, marks=None))]
    fn create(
        &self,
        attrs: Option<&Bound<'_, PyAny>>,
        content: Option<&Bound<'_, PyAny>>,
        marks: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyNode> {
        let attrs = attrs
            .map(py_to_json)
            .unwrap_or(Ok(serde_json::Value::Null))?;
        let content = content
            .map(|c| extract_fragment(c, &self.schema))
            .unwrap_or(Ok(Fragment::new()))?;
        let marks = marks.map(extract_markset).unwrap_or(Ok(MarkSet::new()))?;
        let node = self
            .schema
            .node(&self.name, attrs, content, marks)
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(PyNode {
            schema: self.schema.clone(),
            inner: node,
        })
    }

    fn valid_content(&self, fragment: &Bound<'_, PyAny>) -> PyResult<bool> {
        let frag = extract_fragment(fragment, &self.schema)?;
        let valid = self.schema.with_types(|| self.inner.valid_content(&frag));
        Ok(valid)
    }

    fn __str__(&self) -> String {
        self.name.clone()
    }

    fn __repr__(&self) -> String {
        format!("<NodeType {}>", self.name)
    }
}

// ---------------------------------------------------------------------------
// MarkType
// ---------------------------------------------------------------------------

#[pyclass(name = "MarkType")]
pub struct PyMarkType {
    schema: Arc<DynamicSchema>,
    inner: DynamicMarkType,
    name: String,
}

#[pymethods]
impl PyMarkType {
    #[getter]
    fn schema(&self) -> PySchema {
        PySchema::from_arc(self.schema.clone())
    }

    #[getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[pyo3(signature = (attrs=None))]
    fn create(&self, attrs: Option<&Bound<'_, PyAny>>) -> PyResult<PyMark> {
        let attrs = attrs
            .map(py_to_json)
            .unwrap_or(Ok(serde_json::Value::Null))?;
        Ok(PyMark {
            schema: self.schema.clone(),
            inner: DynamicMark {
                type_name: self.name.clone(),
                attrs,
            },
        })
    }

    fn __str__(&self) -> String {
        self.name.clone()
    }

    fn __repr__(&self) -> String {
        format!("<MarkType {}>", self.name)
    }
}

// ---------------------------------------------------------------------------
// Mark
// ---------------------------------------------------------------------------

#[pyclass(name = "Mark")]
pub struct PyMark {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: DynamicMark,
}

#[pymethods]
impl PyMark {
    #[getter]
    fn type_(&self) -> PyResult<PyMarkType> {
        let name = self.inner.type_name.clone();
        let idx = self
            .schema
            .with_types(|| self.schema.mark_type_map.get(&name).copied().unwrap_or(0));
        Ok(PyMarkType {
            schema: self.schema.clone(),
            inner: DynamicMarkType { idx },
            name,
        })
    }

    #[getter]
    #[pyo3(name = "type")]
    fn py_type(&self) -> PyResult<PyMarkType> {
        self.type_()
    }

    #[getter]
    fn attrs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &self.inner.attrs).map(|b| b.unbind())
    }

    fn add_to_set(&self, set: &Bound<'_, PyAny>) -> PyResult<Vec<PyMark>> {
        let set = extract_markset(set)?;
        let mut new_set = set;
        self.schema.with_types(|| {
            new_set.add(&self.inner);
        });
        Ok(new_set
            .iter()
            .map(|m| PyMark {
                schema: self.schema.clone(),
                inner: m.clone(),
            })
            .collect())
    }

    fn remove_from_set(&self, set: &Bound<'_, PyAny>) -> PyResult<Vec<PyMark>> {
        let set = extract_markset(set)?;
        let mut new_set = set;
        self.schema.with_types(|| {
            new_set.remove(&self.inner);
        });
        Ok(new_set
            .iter()
            .map(|m| PyMark {
                schema: self.schema.clone(),
                inner: m.clone(),
            })
            .collect())
    }

    fn is_in_set(&self, set: &Bound<'_, PyAny>) -> PyResult<bool> {
        let set = extract_markset(set)?;
        let present = self.schema.with_types(|| set.contains(&self.inner));
        Ok(present)
    }

    fn eq(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other = other.cast::<PyMark>()?.borrow();
        Ok(self.inner == other.inner)
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        if let Ok(other) = other.cast::<PyMark>() {
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

    fn __str__(&self) -> String {
        format!("{}(...)", self.inner.type_name)
    }

    fn __repr__(&self) -> String {
        format!("<Mark {}>", self.inner.type_name)
    }
}

// ---------------------------------------------------------------------------
// MarkSet
// ---------------------------------------------------------------------------

#[pyclass(name = "MarkSet")]
pub struct PyMarkSet {
    schema: Arc<DynamicSchema>,
    inner: MarkSet<Dyn>,
}

#[pymethods]
impl PyMarkSet {
    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyIterator>> {
        let mut result: Vec<PyMark> = Vec::new();
        self.schema.with_types(|| {
            for m in self.inner.iter() {
                result.push(PyMark {
                    schema: self.schema.clone(),
                    inner: m.clone(),
                });
            }
        });
        let list = PyList::new(py, result)?;
        pyo3::types::PyIterator::from_object(&list)
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        if let Ok(other) = other.cast::<PyMarkSet>() {
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
// Fragment
// ---------------------------------------------------------------------------

#[pyclass(name = "Fragment")]
pub struct PyFragment {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: Fragment<Dyn>,
}

#[pymethods]
impl PyFragment {
    #[new]
    fn new() -> Self {
        PyFragment {
            schema: Arc::new(DynamicSchema::default()),
            inner: Fragment::new(),
        }
    }

    #[staticmethod]
    fn from_array(nodes: &Bound<'_, PyList>) -> PyResult<Self> {
        let mut inner_nodes = Vec::new();
        let mut schema: Option<Arc<DynamicSchema>> = None;
        for item in nodes.iter() {
            let py_node = item.cast::<PyNode>()?;
            let borrowed = py_node.borrow();
            if schema.is_none() {
                schema = Some(borrowed.schema.clone());
            }
            inner_nodes.push(borrowed.inner.clone());
        }
        let schema = schema.unwrap_or_else(|| Arc::new(DynamicSchema::default()));
        let frag = schema.with_types(|| Fragment::from(inner_nodes));
        Ok(PyFragment {
            schema,
            inner: frag,
        })
    }

    #[classattr]
    fn empty<'py>(_py: Python<'py>) -> PyFragment {
        PyFragment::new()
    }

    #[getter]
    fn size(&self) -> usize {
        self.inner.size()
    }

    #[getter]
    fn child_count(&self) -> usize {
        self.inner.child_count()
    }

    fn child(&self, index: usize) -> PyResult<PyNode> {
        let node = self.schema.with_types(|| {
            let child = self.inner.child(index);
            (*child).clone()
        });
        Ok(PyNode {
            schema: self.schema.clone(),
            inner: node,
        })
    }

    fn maybe_child(&self, index: usize) -> PyResult<Option<PyNode>> {
        let node = self
            .schema
            .with_types(|| self.inner.maybe_child(index).map(|n| (*n).clone()));
        Ok(node.map(|n| PyNode {
            schema: self.schema.clone(),
            inner: n,
        }))
    }

    fn eq(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other = other.cast::<PyFragment>()?.borrow();
        Ok(self.inner == other.inner)
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        if let Ok(other) = other.cast::<PyFragment>() {
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

    fn __str__(&self) -> String {
        let inner = self.schema.with_types(|| {
            self.inner
                .children()
                .iter()
                .map(|n| node_to_debug_str(n))
                .collect::<Vec<_>>()
                .join(", ")
        });
        format!("<{inner}>")
    }

    fn __repr__(&self) -> String {
        format!("<Fragment {}>", self.__str__())
    }

    fn find_diff_start(&self, other: &PyFragment) -> PyResult<Option<usize>> {
        Ok(self
            .schema
            .with_types(|| self.inner.find_diff_start(&other.inner, 0)))
    }

    fn find_diff_end<'py>(
        &self,
        other: &PyFragment,
        py: Python<'py>,
    ) -> PyResult<Option<Bound<'py, PyDict>>> {
        Ok(self.schema.with_types(|| {
            self.inner
                .find_diff_end(&other.inner, self.inner.size(), other.inner.size())
                .map(|(a, b)| {
                    let dict = PyDict::new(py);
                    dict.set_item("a", a).unwrap();
                    dict.set_item("b", b).unwrap();
                    dict
                })
        }))
    }
}

// ---------------------------------------------------------------------------
// Slice
// ---------------------------------------------------------------------------

#[pyclass(name = "Slice")]
pub struct PySlice {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: Slice<Dyn>,
}

#[pymethods]
impl PySlice {
    #[new]
    fn new(content: &PyFragment, open_start: usize, open_end: usize) -> Self {
        PySlice {
            schema: content.schema.clone(),
            inner: Slice::new(content.inner.clone(), open_start, open_end),
        }
    }

    #[getter]
    fn content(&self) -> PyResult<PyFragment> {
        Ok(PyFragment {
            schema: self.schema.clone(),
            inner: self.inner.content.clone(),
        })
    }

    #[getter]
    fn open_start(&self) -> usize {
        self.inner.open_start
    }

    #[getter]
    fn open_end(&self) -> usize {
        self.inner.open_end
    }

    fn eq(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other = other.cast::<PySlice>()?.borrow();
        Ok(self.inner == other.inner)
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        if let Ok(other) = other.cast::<PySlice>() {
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

    fn __str__(&self) -> String {
        let content_str = PyFragment {
            schema: self.schema.clone(),
            inner: self.inner.content.clone(),
        }
        .__str__();
        format!(
            "{content_str}({},{})",
            self.inner.open_start, self.inner.open_end
        )
    }

    fn __repr__(&self) -> String {
        format!("<Slice {}>", self.__str__())
    }
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

fn node_to_debug_str(node: &DynamicNode) -> String {
    let name = &node.type_name;
    if let Some(tn) = node.text_node() {
        let text = tn.text.as_str().to_string();
        return wrap_marks(&format!("\"{text}\""), &node.marks);
    }
    if let Some(content) = node.content() {
        let inner = content
            .children()
            .iter()
            .map(node_to_debug_str)
            .collect::<Vec<_>>()
            .join(", ");
        wrap_marks(&format!("{name}({inner})"), &node.marks)
    } else {
        wrap_marks(name, &node.marks)
    }
}

#[pyclass(name = "Node", dict)]
pub struct PyNode {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: DynamicNode,
}

#[pymethods]
impl PyNode {
    #[staticmethod]
    fn from_json(schema: &PySchema, json: &Bound<'_, PyAny>) -> PyResult<PyNode> {
        let val = py_to_json(json)?;
        let node = schema
            .inner
            .node_from_json(&val)
            .map_err(|e| PyValueError::new_err(format!("Invalid node JSON: {e}")))?;
        Ok(PyNode {
            schema: schema.inner.clone(),
            inner: node,
        })
    }

    #[getter]
    fn type_(&self) -> PyResult<PyNodeType> {
        let name = self.inner.type_name.clone();
        let idx = self.inner.type_idx;
        Ok(PyNodeType {
            schema: self.schema.clone(),
            inner: DynamicNodeType { idx },
            name,
        })
    }

    #[getter]
    #[pyo3(name = "type")]
    fn py_type(&self) -> PyResult<PyNodeType> {
        self.type_()
    }

    #[getter]
    fn content(&self) -> PyResult<Option<PyFragment>> {
        let frag = self.schema.with_types(|| {
            if let Some(n) = self.inner.content() {
                Some((*n).clone())
            } else {
                None
            }
        });
        Ok(frag.map(|f| PyFragment {
            schema: self.schema.clone(),
            inner: f,
        }))
    }

    #[getter]
    fn marks(&self) -> PyResult<PyMarkSet> {
        let set = self
            .schema
            .with_types(|| self.inner.marks().cloned().unwrap_or_else(MarkSet::new));
        Ok(PyMarkSet {
            schema: self.schema.clone(),
            inner: set,
        })
    }

    #[getter]
    fn attrs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &self.inner.attrs_json()).map(|b| b.unbind())
    }

    #[getter]
    fn text_content(&self) -> String {
        self.schema.with_types(|| self.inner.text_content())
    }

    #[getter]
    fn node_size(&self) -> usize {
        self.schema.with_types(|| self.inner.node_size())
    }

    #[getter]
    fn child_count(&self) -> usize {
        self.schema.with_types(|| self.inner.child_count())
    }

    #[getter]
    fn is_text(&self) -> bool {
        self.schema.with_types(|| self.inner.is_text())
    }

    #[getter]
    fn is_block(&self) -> bool {
        self.schema.with_types(|| self.inner.is_block())
    }

    #[getter]
    fn is_leaf(&self) -> bool {
        self.schema.with_types(|| self.inner.is_leaf())
    }

    fn child(&self, index: usize) -> PyResult<PyNode> {
        let node = self
            .schema
            .with_types(|| (*self.inner.child(index).unwrap()).clone());
        Ok(PyNode {
            schema: self.schema.clone(),
            inner: node,
        })
    }

    #[pyo3(signature = (from_, to=None, include_parents=false))]
    fn slice(&self, from_: usize, to: Option<usize>, include_parents: bool) -> PyResult<PySlice> {
        let to = to.unwrap_or_else(|| self.schema.with_types(|| self.inner.content_size()));
        let slice = self
            .schema
            .with_types(|| self.inner.slice(from_..to, include_parents))
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(PySlice {
            schema: self.schema.clone(),
            inner: slice,
        })
    }

    fn cut(&self, from: usize, to: usize) -> PyResult<PyNode> {
        let node = self.schema.with_types(|| self.inner.cut(from..to));
        Ok(PyNode {
            schema: self.schema.clone(),
            inner: node.into_owned(),
        })
    }

    fn replace(&self, from: usize, to: usize, slice: &PySlice) -> PyResult<PyNode> {
        let node = self
            .schema
            .with_types(|| self.inner.replace(from..to, &slice.inner))
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(PyNode {
            schema: self.schema.clone(),
            inner: node,
        })
    }

    fn mark(&self, marks: &Bound<'_, PyAny>) -> PyResult<PyNode> {
        let marks = extract_markset(marks)?;
        let node = self.schema.with_types(|| self.inner.mark(marks));
        Ok(PyNode {
            schema: self.schema.clone(),
            inner: node,
        })
    }

    fn resolve(&self, pos: usize) -> PyResult<PyResolvedPos> {
        Ok(PyResolvedPos {
            schema: self.schema.clone(),
            doc: self.inner.clone(),
            pos,
        })
    }

    fn eq(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other = other.cast::<PyNode>()?.borrow();
        Ok(self.inner == other.inner)
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        if let Ok(other) = other.cast::<PyNode>() {
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

    fn __str__(&self) -> String {
        self.schema.with_types(|| node_to_debug_str(&self.inner))
    }

    fn __repr__(&self) -> String {
        format!("<Node {}>", self.__str__())
    }

    fn __getattr__<'py>(&self, name: &str, py: Python<'py>) -> PyResult<Py<PyAny>> {
        if name == "tag" {
            Ok(PyDict::new(py).into())
        } else {
            Err(PyAttributeError::new_err(format!(
                "'Node' object has no attribute '{}'",
                name
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// ResolvedPos
// ---------------------------------------------------------------------------

#[pyclass(name = "ResolvedPos")]
pub struct PyResolvedPos {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) doc: DynamicNode,
    pub(crate) pos: usize,
}

#[pymethods]
impl PyResolvedPos {
    #[getter]
    fn pos(&self) -> usize {
        self.pos
    }

    #[getter]
    fn depth(&self) -> usize {
        self.schema.with_types(|| {
            ResolvedPos::<Dyn>::resolve(&self.doc, self.pos)
                .map(|r| r.depth)
                .unwrap_or(0)
        })
    }

    #[getter]
    fn parent_offset(&self) -> usize {
        self.schema.with_types(|| {
            ResolvedPos::<Dyn>::resolve(&self.doc, self.pos)
                .map(|r| r.parent_offset)
                .unwrap_or(0)
        })
    }

    fn node(&self, depth: usize) -> PyResult<PyNode> {
        let n = self
            .schema
            .with_types(|| {
                ResolvedPos::<Dyn>::resolve(&self.doc, self.pos).map(|r| r.node(depth).clone())
            })
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(PyNode {
            schema: self.schema.clone(),
            inner: n,
        })
    }

    fn start(&self, depth: usize) -> PyResult<usize> {
        let v = self
            .schema
            .with_types(|| ResolvedPos::<Dyn>::resolve(&self.doc, self.pos).map(|r| r.start(depth)))
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(v)
    }

    fn end(&self, depth: usize) -> PyResult<usize> {
        let v = self
            .schema
            .with_types(|| ResolvedPos::<Dyn>::resolve(&self.doc, self.pos).map(|r| r.end(depth)))
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(v)
    }

    fn before(&self, depth: usize) -> PyResult<Option<usize>> {
        let v = self
            .schema
            .with_types(|| {
                ResolvedPos::<Dyn>::resolve(&self.doc, self.pos).map(|r| r.before(depth))
            })
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(v)
    }

    fn after(&self, depth: usize) -> PyResult<Option<usize>> {
        let v = self
            .schema
            .with_types(|| ResolvedPos::<Dyn>::resolve(&self.doc, self.pos).map(|r| r.after(depth)))
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(v)
    }

    #[getter]
    fn node_before(&self) -> PyResult<Option<PyNode>> {
        let n = self.schema.with_types(|| {
            ResolvedPos::<Dyn>::resolve(&self.doc, self.pos)
                .ok()
                .and_then(|r| r.node_before().map(|cow| cow.into_owned()))
        });
        Ok(n.map(|inner| PyNode {
            schema: self.schema.clone(),
            inner,
        }))
    }

    #[getter]
    fn node_after(&self) -> PyResult<Option<PyNode>> {
        let n = self.schema.with_types(|| {
            ResolvedPos::<Dyn>::resolve(&self.doc, self.pos)
                .ok()
                .and_then(|r| r.node_after().map(|cow| cow.into_owned()))
        });
        Ok(n.map(|inner| PyNode {
            schema: self.schema.clone(),
            inner,
        }))
    }

    fn pos_at_index(&self, index: usize, depth: Option<usize>) -> PyResult<usize> {
        let v = self
            .schema
            .with_types(|| {
                ResolvedPos::<Dyn>::resolve(&self.doc, self.pos)
                    .map(|r| r.pos_at_index(index, depth))
            })
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(v)
    }

    fn marks(&self) -> PyResult<Vec<PyMark>> {
        let mut result = Vec::new();
        self.schema.with_types(|| {
            if let Ok(r) = ResolvedPos::<Dyn>::resolve(&self.doc, self.pos) {
                for m in r.marks() {
                    result.push(PyMark {
                        schema: self.schema.clone(),
                        inner: m,
                    });
                }
            }
        });
        Ok(result)
    }

    fn __str__(&self) -> String {
        self.schema.with_types(|| {
            if let Ok(r) = ResolvedPos::<Dyn>::resolve(&self.doc, self.pos) {
                let path: Vec<String> = (1..=r.depth)
                    .map(|i| format!("{}_{}", r.node(i).type_name, r.index(i - 1)))
                    .collect();
                format!("{}:{}", path.join("/"), r.parent_offset)
            } else {
                format!("invalid:{}", self.pos)
            }
        })
    }

    #[pyo3(signature = (other=None))]
    fn block_range(&self, other: Option<&PyResolvedPos>) -> PyResult<Option<PyNodeRange>> {
        let range = self.schema.with_types(|| {
            let from = ResolvedPos::<Dyn>::resolve(&self.doc, self.pos).ok()?;
            let other_ref = other
                .map(|o| ResolvedPos::<Dyn>::resolve(&o.doc, o.pos).ok())
                .unwrap_or(None);
            let other_ref_borrowed = other_ref.as_ref();
            let nr = from.block_range(other_ref_borrowed, None)?;
            Some((self.doc.clone(), nr.start(), nr.end(), nr.depth))
        });
        Ok(range.map(|(doc, start, end, depth)| PyNodeRange {
            schema: self.schema.clone(),
            from_doc: doc,
            from_pos: start,
            to_pos: end,
            depth,
        }))
    }

    fn __repr__(&self) -> String {
        format!("<ResolvedPos {}>", self.__str__())
    }
}

#[pyclass(name = "NodeRange")]
pub struct PyNodeRange {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) from_doc: DynamicNode,
    pub(crate) from_pos: usize,
    pub(crate) to_pos: usize,
    pub(crate) depth: usize,
}

#[pymethods]
impl PyNodeRange {
    #[new]
    fn new(from: &PyResolvedPos, to: &PyResolvedPos, depth: Option<usize>) -> PyResult<Self> {
        let depth = depth.unwrap_or_else(|| {
            from.schema.with_types(|| {
                prosemirror::model::ResolvedPos::<Dyn>::resolve(&from.doc, from.pos)
                    .map(|r| r.shared_depth(to.pos))
                    .unwrap_or(0)
            })
        });
        Ok(PyNodeRange {
            schema: from.schema.clone(),
            from_doc: from.doc.clone(),
            from_pos: from.pos,
            to_pos: to.pos,
            depth,
        })
    }

    #[getter]
    fn depth(&self) -> usize {
        self.depth
    }

    #[getter]
    fn start(&self) -> usize {
        self.from_pos
    }

    #[getter]
    fn end(&self) -> usize {
        self.to_pos
    }
}
