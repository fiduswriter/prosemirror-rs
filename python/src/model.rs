use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

// Global registry mapping DynamicSchema pointer addresses to their raw
// Python spec dicts (with callables intact).  This allows any PySchema
// wrapper — including those created via from_arc — to access the original
// spec values needed for toDebugString / leafText.
static SCHEMA_RAW_SPECS: std::sync::LazyLock<Mutex<HashMap<usize, Py<PyAny>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

use prosemirror::binding::model::{
    b_fragment_from, b_schema_top_node_type, BContentMatch, BFragment, BMark, BMarkType, BNode,
    BNodeRange, BNodeType, BResolvedPos, BSlice, FragmentFromInput,
};
use prosemirror::dynamic::types::{
    Dyn, DynamicMark, DynamicMarkType, DynamicNode, DynamicNodeType,
};
use prosemirror::dynamic::DynamicSchema;
use prosemirror::model::{Fragment, MarkSet, Node, ResolvedPos};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Recursively strip Python callables from a dict/list structure,
/// replacing them with `None`. This allows schema specs that contain
/// callbacks (e.g. `leafText`, `toDebugString`) to be passed through
/// JSON deserialization.
pub fn strip_callables<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if obj.is_callable() {
        return Ok(py.None().into_bound(py));
    }
    if obj.is_none() {
        return Ok(py.None().into_bound(py));
    }
    if let Ok(list) = obj.cast::<PyList>() {
        let new_list = PyList::new(py, [] as [Bound<'py, PyAny>; 0])?;
        for item in list.iter() {
            new_list.append(strip_callables(py, &item)?)?;
        }
        return Ok(new_list.into_any());
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        let new_dict = PyDict::new(py);
        for (k, v) in dict.iter() {
            new_dict.set_item(k, strip_callables(py, &v)?)?;
        }
        return Ok(new_dict.into_any());
    }
    // Primitives, strings, etc. pass through unchanged
    Ok(obj.clone())
}

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
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(serde_json::Value::String(s))
    } else if let Ok(list) = obj.cast::<PyList>() {
        let mut arr = Vec::new();
        for item in list.iter() {
            arr.push(py_to_json(&item)?);
        }
        Ok(serde_json::Value::Array(arr))
    } else if let Ok(dict) = obj.cast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key = k.extract::<String>()?;
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

pub fn extract_fragment(obj: &Bound<'_, PyAny>, schema: &DynamicSchema) -> PyResult<Fragment<Dyn>> {
    if obj.is_none() {
        return Ok(Fragment::new());
    }
    if let Ok(frag) = obj.cast::<PyFragment>() {
        return Ok(frag.borrow().inner.inner.clone());
    }
    if let Ok(list) = obj.cast::<PyList>() {
        let mut nodes = Vec::new();
        for item in list.iter() {
            let node = item.cast::<PyNode>()?.borrow().inner.inner.clone();
            nodes.push(node);
        }
        return Ok(schema.with_types(|| Fragment::from(nodes)));
    }
    if let Ok(node) = obj.cast::<PyNode>() {
        return Ok(schema.with_types(|| Fragment::from(vec![node.borrow().inner.inner.clone()])));
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
        let mut marks = Vec::new();
        for item in list.iter() {
            let py_mark = item.cast::<PyMark>()?;
            let mark = py_mark.borrow().inner.inner.clone();
            marks.push(mark);
        }
        return Ok(MarkSet::from_vec(marks));
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
    /// The original Python spec dict, preserved so that callable values
    /// (e.g. `leafText`, `toDebugString`) can be accessed by Python-side code.
    raw_spec: Option<Py<PyAny>>,
}

impl PySchema {
    pub(crate) fn from_arc(arc: Arc<DynamicSchema>) -> Self {
        let ptr = Arc::as_ptr(&arc) as usize;
        let raw_spec = Python::attach(|py| {
            SCHEMA_RAW_SPECS
                .lock()
                .unwrap()
                .get(&ptr)
                .map(|r| r.clone_ref(py))
        });
        Self {
            inner: arc,
            spec: serde_json::Value::Null,
            raw_spec,
        }
    }
}

#[pymethods]
impl PySchema {
    #[new]
    fn new(spec: &Bound<'_, PyAny>) -> PyResult<Self> {
        let py = spec.py();
        let stripped = strip_callables(py, spec)?;
        let json = py_to_json(&stripped)?;
        let schema = DynamicSchema::from_json(&json)
            .map_err(|e| PyValueError::new_err(format!("Invalid schema: {e}")))?;
        let arc = Arc::new(schema);
        let ptr = Arc::as_ptr(&arc) as usize;
        SCHEMA_RAW_SPECS
            .lock()
            .unwrap()
            .insert(ptr, spec.clone().unbind());
        Ok(PySchema {
            inner: arc,
            spec: json,
            raw_spec: Some(spec.clone().unbind()),
        })
    }

    #[getter]
    fn raw_spec(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.raw_spec.as_ref().map(|r| r.clone_ref(py))
    }

    #[getter]
    fn nodes(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        self.inner.with_types(|| {
            for (name, idx) in &self.inner.node_type_map {
                let nt = PyNodeType {
                    inner: BNodeType::new(
                        self.inner.clone(),
                        DynamicNodeType { idx: *idx },
                        name.clone(),
                    ),
                };
                dict.set_item(name, nt)?;
            }
            Ok::<_, PyErr>(())
        })?;
        Ok(dict.unbind())
    }

    #[getter]
    fn marks(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        self.inner.with_types(|| {
            for (name, idx) in &self.inner.mark_type_map {
                let mt = PyMarkType {
                    inner: BMarkType {
                        schema: self.inner.clone(),
                        inner: DynamicMarkType { idx: *idx },
                        name: name.clone(),
                    },
                };
                dict.set_item(name, mt)?;
            }
            Ok::<_, PyErr>(())
        })?;
        Ok(dict.unbind())
    }

    fn node_from_json(&self, json: &Bound<'_, PyAny>) -> PyResult<PyNode> {
        let val = py_to_json(json)?;
        let bnode = BNode::from_json(&self.inner, &val)
            .map_err(|e| PyValueError::new_err(format!("Invalid node JSON: {e}")))?;
        Ok(PyNode { inner: bnode })
    }

    fn mark_from_json(&self, json: &Bound<'_, PyAny>) -> PyResult<PyMark> {
        let val = py_to_json(json)?;
        let mark = self
            .inner
            .mark_from_json(&val)
            .map_err(|e| PyValueError::new_err(format!("Invalid mark JSON: {e}")))?;
        Ok(PyMark {
            inner: BMark {
                schema: self.inner.clone(),
                inner: mark,
            },
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
            inner: BNode {
                schema: self.inner.clone(),
                inner: node,
            },
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
            inner: BNode {
                schema: self.inner.clone(),
                inner: node,
            },
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
            .unwrap_or(Ok(serde_json::Value::Object(serde_json::Map::new())))?;
        Ok(PyMark {
            inner: BMark {
                schema: self.inner.clone(),
                inner: DynamicMark {
                    type_name: type_name.to_string(),
                    attrs,
                },
            },
        })
    }

    #[getter]
    fn top_node_type(&self) -> PyResult<PyNodeType> {
        b_schema_top_node_type(&self.inner)
            .map(|inner| PyNodeType { inner })
            .ok_or_else(|| PyValueError::new_err("Unknown top node type"))
    }
}

// ---------------------------------------------------------------------------
// NodeType
// ---------------------------------------------------------------------------

#[pyclass(name = "NodeType")]
pub struct PyNodeType {
    pub(crate) inner: BNodeType,
}

#[pymethods]
impl PyNodeType {
    #[getter]
    fn schema(&self) -> PySchema {
        PySchema::from_arc(self.inner.schema.clone())
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn is_block(&self) -> bool {
        self.inner.is_block()
    }

    #[getter]
    fn is_inline(&self) -> bool {
        self.inner.is_inline()
    }

    #[getter]
    fn is_atom(&self) -> bool {
        self.inner.is_atom()
    }

    #[getter]
    fn is_textblock(&self) -> bool {
        self.inner.is_textblock()
    }

    #[getter]
    fn inline_content(&self) -> bool {
        self.inner.inline_content()
    }

    #[getter]
    fn is_leaf(&self) -> bool {
        self.inner.is_leaf()
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
            .map(|c| extract_fragment(c, &self.inner.schema))
            .unwrap_or(Ok(Fragment::new()))?;
        let marks = marks.map(extract_markset).unwrap_or(Ok(MarkSet::new()))?;
        let bnode = self.inner.create(attrs, content, marks);
        Ok(PyNode { inner: bnode })
    }

    fn valid_content(&self, fragment: &Bound<'_, PyAny>) -> PyResult<bool> {
        let frag = extract_fragment(fragment, &self.inner.schema)?;
        Ok(self.inner.valid_content(&frag))
    }

    fn allows_mark_type(&self, mark_type: &PyMarkType) -> PyResult<bool> {
        Ok(self.inner.allows_mark_type(&mark_type.inner))
    }

    #[pyo3(signature = (attrs=None, content=None, marks=None))]
    fn create_and_fill(
        &self,
        attrs: Option<&Bound<'_, PyAny>>,
        content: Option<&Bound<'_, PyAny>>,
        marks: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Option<PyNode>> {
        let attrs = attrs
            .map(py_to_json)
            .unwrap_or(Ok(serde_json::Value::Null))?;
        let content = content
            .map(|c| extract_fragment(c, &self.inner.schema))
            .unwrap_or(Ok(Fragment::new()))?;
        let marks = marks.map(extract_markset).unwrap_or(Ok(MarkSet::new()))?;
        Ok(self
            .inner
            .create_and_fill(attrs, Some(content), marks)
            .map(|bnode| PyNode { inner: bnode }))
    }

    #[pyo3(signature = (attrs=None, content=None, marks=None))]
    fn create_checked(
        &self,
        attrs: Option<&Bound<'_, PyAny>>,
        content: Option<&Bound<'_, PyAny>>,
        marks: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyNode> {
        let attrs = attrs
            .map(py_to_json)
            .unwrap_or(Ok(serde_json::Value::Null))?;
        let content = content
            .map(|c| extract_fragment(c, &self.inner.schema))
            .unwrap_or(Ok(Fragment::new()))?;
        let marks = marks.map(extract_markset).unwrap_or(Ok(MarkSet::new()))?;
        let bnode = self
            .inner
            .create_checked(attrs, content, marks)
            .map_err(|e| PyValueError::new_err(e))?;
        Ok(PyNode { inner: bnode })
    }

    #[getter]
    fn is_text(&self) -> bool {
        self.inner.is_text()
    }

    #[getter]
    fn whitespace(&self) -> String {
        self.inner.whitespace()
    }

    #[getter]
    fn is_code(&self) -> bool {
        self.inner.is_code()
    }

    #[getter]
    fn has_required_attrs(&self) -> bool {
        self.inner.has_required_attrs()
    }

    fn compatible_content(&self, other: &PyNodeType) -> bool {
        self.inner.compatible_content(&other.inner)
    }

    #[getter]
    fn content_match(&self) -> Option<PyContentMatch> {
        self.inner
            .content_match()
            .map(|cm| PyContentMatch { inner: cm })
    }

    fn allows_marks(&self, marks: &Bound<'_, PyAny>) -> PyResult<bool> {
        let ms = extract_markset(marks)?;
        Ok(self.inner.allows_marks(&ms))
    }

    fn is_in_group(&self, group: &str) -> bool {
        self.inner.is_in_group(group)
    }

    #[getter]
    fn attrs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &self.inner.attrs_defaults()).map(|b| b.unbind())
    }

    #[getter]
    fn mark_set(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match self.inner.mark_set() {
            None => Ok(None),
            Some(marks) => {
                let list = PyList::new(py, marks.into_iter().map(|bmt| PyMarkType { inner: bmt }))?;
                Ok(Some(list.into_any().unbind()))
            }
        }
    }

    fn allowed_marks(&self, marks: &Bound<'_, PyAny>) -> PyResult<Vec<PyMark>> {
        let ms = extract_markset(marks)?;
        let schema = self.inner.schema.clone();
        Ok(self
            .inner
            .allowed_marks_filtered(ms.iter().cloned().collect())
            .into_iter()
            .map(|m| PyMark {
                inner: BMark {
                    schema: schema.clone(),
                    inner: m,
                },
            })
            .collect())
    }

    #[getter]
    fn spec(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &self.inner.spec_json()).map(|b| b.unbind())
    }

    fn __str__(&self) -> String {
        self.inner.name.clone()
    }

    fn __repr__(&self) -> String {
        format!("<NodeType {}>", self.inner.name)
    }
}

// ---------------------------------------------------------------------------
// MarkType
// ---------------------------------------------------------------------------

#[pyclass(name = "MarkType")]
pub struct PyMarkType {
    pub(crate) inner: BMarkType,
}

#[pymethods]
impl PyMarkType {
    #[getter]
    fn schema(&self) -> PySchema {
        PySchema::from_arc(self.inner.schema.clone())
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[pyo3(signature = (attrs=None))]
    fn create(&self, attrs: Option<&Bound<'_, PyAny>>) -> PyResult<PyMark> {
        let attrs = attrs
            .map(py_to_json)
            .unwrap_or(Ok(serde_json::Value::Null))?;
        Ok(PyMark {
            inner: self.inner.create(attrs),
        })
    }

    fn remove_from_set(&self, set: &Bound<'_, PyAny>) -> PyResult<Vec<PyMark>> {
        let ms = extract_markset(set)?;
        let result = self.inner.remove_from_set(ms.iter().cloned().collect());
        Ok(result
            .into_iter()
            .map(|m| PyMark {
                inner: BMark {
                    schema: self.inner.schema.clone(),
                    inner: m,
                },
            })
            .collect())
    }

    fn is_in_set(&self, set: &Bound<'_, PyAny>) -> PyResult<Option<PyMark>> {
        let ms = extract_markset(set)?;
        Ok(self
            .inner
            .is_in_set(&ms.iter().cloned().collect::<Vec<_>>())
            .map(|bm| PyMark { inner: bm }))
    }

    fn excludes(&self, other: &PyMarkType) -> bool {
        self.inner.excludes(&other.inner)
    }

    #[getter]
    fn spec(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &self.inner.spec_json()).map(|b| b.unbind())
    }

    fn __str__(&self) -> String {
        self.inner.name.clone()
    }

    fn __repr__(&self) -> String {
        format!("<MarkType {}>", self.inner.name)
    }
}

// ---------------------------------------------------------------------------
// Mark
// ---------------------------------------------------------------------------

#[pyclass(name = "Mark")]
pub struct PyMark {
    pub(crate) inner: BMark,
}

#[pymethods]
impl PyMark {
    #[getter]
    fn type_(&self) -> PyResult<PyMarkType> {
        Ok(PyMarkType {
            inner: self.inner.type_(),
        })
    }

    #[getter]
    #[pyo3(name = "type")]
    fn py_type(&self) -> PyResult<PyMarkType> {
        self.type_()
    }

    #[getter]
    fn attrs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &self.inner.attrs_json()).map(|b| b.unbind())
    }

    fn add_to_set(&self, set: &Bound<'_, PyAny>) -> PyResult<Vec<PyMark>> {
        let set = extract_markset(set)?;
        let result = self.inner.add_to_set(set.iter().cloned().collect());
        Ok(result
            .into_iter()
            .map(|m| PyMark {
                inner: BMark {
                    schema: self.inner.schema.clone(),
                    inner: m,
                },
            })
            .collect())
    }

    fn remove_from_set(&self, set: &Bound<'_, PyAny>) -> PyResult<Vec<PyMark>> {
        let set = extract_markset(set)?;
        let result = self.inner.remove_from_set(set.iter().cloned().collect());
        Ok(result
            .into_iter()
            .map(|m| PyMark {
                inner: BMark {
                    schema: self.inner.schema.clone(),
                    inner: m,
                },
            })
            .collect())
    }

    fn is_in_set(&self, set: &Bound<'_, PyAny>) -> PyResult<bool> {
        let set = extract_markset(set)?;
        Ok(self
            .inner
            .is_in_set(&set.iter().cloned().collect::<Vec<_>>()))
    }

    fn to_json(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &self.inner.to_json()).map(|b| b.unbind())
    }

    fn eq(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other = other.cast::<PyMark>()?.borrow();
        Ok(self.inner.eq(&other.inner))
    }

    #[staticmethod]
    fn same_set(a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> PyResult<bool> {
        let av: Vec<_> = if let Ok(list) = a.cast::<pyo3::types::PyList>() {
            list.iter()
                .map(|i| Ok(i.cast::<PyMark>()?.borrow().inner.inner.clone()))
                .collect::<PyResult<Vec<_>>>()?
        } else {
            vec![]
        };
        let bv: Vec<_> = if let Ok(list) = b.cast::<pyo3::types::PyList>() {
            list.iter()
                .map(|i| Ok(i.cast::<PyMark>()?.borrow().inner.inner.clone()))
                .collect::<PyResult<Vec<_>>>()?
        } else {
            vec![]
        };
        Ok(BMark::same_set(&av, &bv))
    }

    #[staticmethod]
    fn set_from(schema: &PySchema, marks: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<PyMark>> {
        let raw: Vec<_> = if let Some(marks) = marks {
            if let Ok(list) = marks.cast::<pyo3::types::PyList>() {
                list.iter()
                    .map(|i| Ok(i.cast::<PyMark>()?.borrow().inner.inner.clone()))
                    .collect::<PyResult<Vec<_>>>()?
            } else {
                vec![]
            }
        } else {
            vec![]
        };
        let s = schema.inner.clone();
        Ok(BMark::set_from(&s, raw)
            .into_iter()
            .map(|m| PyMark {
                inner: BMark {
                    schema: s.clone(),
                    inner: m,
                },
            })
            .collect())
    }

    #[classattr]
    fn none() -> Vec<PyMark> {
        Vec::new()
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        if let Ok(other) = other.cast::<PyMark>() {
            let other = other.borrow();
            match op {
                pyo3::basic::CompareOp::Eq => Ok(self.inner.inner == other.inner.inner),
                pyo3::basic::CompareOp::Ne => Ok(self.inner.inner != other.inner.inner),
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    fn __str__(&self) -> String {
        format!("{}(...)", self.inner.inner.type_name)
    }

    fn __repr__(&self) -> String {
        format!("<Mark {}>", self.inner.inner.type_name)
    }
}

// ---------------------------------------------------------------------------
// MarkSet
// ---------------------------------------------------------------------------

#[pyclass(name = "MarkSet")]
pub struct PyMarkSet {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: MarkSet<Dyn>,
}

#[pymethods]
impl PyMarkSet {
    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyIterator>> {
        let mut result: Vec<PyMark> = Vec::new();
        self.schema.with_types(|| {
            for m in self.inner.iter() {
                result.push(PyMark {
                    inner: BMark {
                        schema: self.schema.clone(),
                        inner: m.clone(),
                    },
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
    pub(crate) inner: BFragment,
}

#[pymethods]
impl PyFragment {
    #[new]
    fn new() -> Self {
        PyFragment {
            inner: BFragment::empty(Arc::new(DynamicSchema::default())),
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
                schema = Some(borrowed.inner.schema.clone());
            }
            inner_nodes.push(borrowed.inner.inner.clone());
        }
        let schema = schema.unwrap_or_else(|| Arc::new(DynamicSchema::default()));
        let frag = schema.with_types(|| Fragment::from_array(inner_nodes));
        Ok(PyFragment {
            inner: BFragment {
                schema,
                inner: frag,
            },
        })
    }

    /// Polymorphic `Fragment.from(input)` — accepts null/None, a Node, a list
    /// of Nodes, or an existing Fragment.
    #[staticmethod]
    #[pyo3(name = "from_")]
    fn from_input(input: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let schema;
        let finput = match input {
            None => {
                return Ok(PyFragment::new());
            }
            Some(obj) if obj.is_none() => {
                return Ok(PyFragment::new());
            }
            Some(obj) => {
                if let Ok(f) = obj.cast::<PyFragment>() {
                    return Ok(PyFragment {
                        inner: f.borrow().inner.clone(),
                    });
                }
                if let Ok(n) = obj.cast::<PyNode>() {
                    schema = n.borrow().inner.schema.clone();
                    let node = n.borrow().inner.clone();
                    FragmentFromInput::SingleNode(node)
                } else if let Ok(list) = obj.cast::<PyList>() {
                    let mut nodes = Vec::new();
                    let mut s: Option<Arc<DynamicSchema>> = None;
                    for item in list.iter() {
                        let n = item.cast::<PyNode>()?;
                        let nb = n.borrow();
                        if s.is_none() {
                            s = Some(nb.inner.schema.clone());
                        }
                        nodes.push(nb.inner.inner.clone());
                    }
                    schema = s.unwrap_or_else(|| Arc::new(DynamicSchema::default()));
                    FragmentFromInput::NodeArray(nodes)
                } else {
                    return Err(PyValueError::new_err(
                        "Fragment.from_: expected Node, list of Nodes, Fragment, or None",
                    ));
                }
            }
        };
        Ok(PyFragment {
            inner: b_fragment_from(schema, finput),
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
        self.inner
            .child(index)
            .map(|bn| PyNode { inner: bn })
            .ok_or_else(|| PyValueError::new_err(format!("child index {index} out of bounds")))
    }

    fn maybe_child(&self, index: usize) -> PyResult<Option<PyNode>> {
        Ok(self.inner.maybe_child(index).map(|bn| PyNode { inner: bn }))
    }

    fn eq(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other = other.cast::<PyFragment>()?.borrow();
        Ok(self.inner.eq(&other.inner))
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        if let Ok(other) = other.cast::<PyFragment>() {
            let other = other.borrow();
            match op {
                pyo3::basic::CompareOp::Eq => Ok(self.inner.inner == other.inner.inner),
                pyo3::basic::CompareOp::Ne => Ok(self.inner.inner != other.inner.inner),
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    fn __str__(&self) -> String {
        let inner_str = self.inner.schema.with_types(|| {
            self.inner
                .inner
                .children()
                .iter()
                .map(|n| n.to_debug_string())
                .collect::<Vec<_>>()
                .join(", ")
        });
        format!("<{inner_str}>")
    }

    fn __repr__(&self) -> String {
        format!("<Fragment {}>", self.__str__())
    }

    fn append(&self, other: &PyFragment) -> PyResult<PyFragment> {
        Ok(PyFragment {
            inner: self.inner.append(&other.inner),
        })
    }

    #[getter]
    fn first_child(&self) -> Option<PyNode> {
        self.inner.first_child().map(|bn| PyNode { inner: bn })
    }

    #[getter]
    fn last_child(&self) -> Option<PyNode> {
        self.inner.last_child().map(|bn| PyNode { inner: bn })
    }

    fn replace_child(&self, index: usize, node: &PyNode) -> PyFragment {
        PyFragment {
            inner: self.inner.replace_child(index, node.inner.inner.clone()),
        }
    }

    fn add_to_start(&self, node: &PyNode) -> PyFragment {
        PyFragment {
            inner: self.inner.add_to_start(node.inner.inner.clone()),
        }
    }

    fn add_to_end(&self, node: &PyNode) -> PyFragment {
        PyFragment {
            inner: self.inner.add_to_end(node.inner.inner.clone()),
        }
    }

    #[pyo3(signature = (from_, to, block_separator=None, leaf_text=None))]
    fn text_between(
        &self,
        from_: usize,
        to: usize,
        block_separator: Option<&str>,
        leaf_text: Option<&str>,
    ) -> String {
        self.inner
            .text_between(from_, to, block_separator, leaf_text)
    }

    fn for_each(&self, py: Python<'_>, f: Py<PyAny>) -> PyResult<()> {
        let mut items: Vec<(DynamicNode, usize, usize)> = Vec::new();
        self.inner.for_each(|n, o, i| items.push((n.clone(), o, i)));
        for (node, offset, index) in items {
            f.call1(
                py,
                (
                    PyNode {
                        inner: BNode {
                            schema: self.inner.schema.clone(),
                            inner: node,
                        },
                    },
                    offset,
                    index,
                ),
            )?;
        }
        Ok(())
    }

    #[pyo3(signature = (from_, to, f, node_start=0))]
    fn nodes_between(
        &self,
        py: Python<'_>,
        from_: usize,
        to: usize,
        f: Py<PyAny>,
        node_start: usize,
    ) -> PyResult<()> {
        let schema = self.inner.schema.clone();
        let mut err: Option<pyo3::PyErr> = None;
        schema.with_types(|| {
            self.inner.inner.nodes_between(
                from_,
                to,
                &mut |n, p, parent, index| {
                    if err.is_some() {
                        return false;
                    }
                    let result = (|| -> PyResult<bool> {
                        let py_node = PyNode {
                            inner: BNode {
                                schema: schema.clone(),
                                inner: n.clone(),
                            },
                        };
                        let py_parent = parent.map(|par| PyNode {
                            inner: BNode {
                                schema: schema.clone(),
                                inner: par.clone(),
                            },
                        });
                        let ret = f.call1(py, (py_node, p, py_parent, index))?;
                        // Only strict Python `False` suppresses recursion.
                        if let Ok(b) = ret.extract::<bool>(py) {
                            if !b {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    })();
                    match result {
                        Ok(v) => v,
                        Err(e) => {
                            err = Some(e);
                            false
                        }
                    }
                },
                node_start,
                None,
            );
        });
        if let Some(e) = err {
            return Err(e);
        }
        Ok(())
    }

    fn descendants(&self, py: Python<'_>, f: Py<PyAny>) -> PyResult<()> {
        let schema = self.inner.schema.clone();
        let mut err: Option<pyo3::PyErr> = None;
        let size = self.inner.inner.size();
        schema.with_types(|| {
            self.inner.inner.nodes_between(
                0,
                size,
                &mut |n, p, parent, index| {
                    if err.is_some() {
                        return false;
                    }
                    let result = (|| -> PyResult<bool> {
                        let py_node = PyNode {
                            inner: BNode {
                                schema: schema.clone(),
                                inner: n.clone(),
                            },
                        };
                        let py_parent = parent.map(|par| PyNode {
                            inner: BNode {
                                schema: schema.clone(),
                                inner: par.clone(),
                            },
                        });
                        let ret = f.call1(py, (py_node, p, py_parent, index))?;
                        if let Ok(b) = ret.extract::<bool>(py) {
                            if !b {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    })();
                    match result {
                        Ok(v) => v,
                        Err(e) => {
                            err = Some(e);
                            false
                        }
                    }
                },
                0,
                None,
            );
        });
        if let Some(e) = err {
            return Err(e);
        }
        Ok(())
    }

    #[pyo3(signature = (other, pos=0))]
    fn find_diff_start(&self, other: &PyFragment, pos: Option<usize>) -> PyResult<Option<usize>> {
        Ok(self.inner.find_diff_start(&other.inner, pos.unwrap_or(0)))
    }

    #[pyo3(signature = (other, pos_a=0, pos_b=0))]
    fn find_diff_end<'py>(
        &self,
        other: &PyFragment,
        pos_a: Option<usize>,
        pos_b: Option<usize>,
        py: Python<'py>,
    ) -> PyResult<Option<Bound<'py, PyDict>>> {
        Ok(self
            .inner
            .find_diff_end(&other.inner, pos_a.unwrap_or(0), pos_b.unwrap_or(0))
            .map(|(a, b)| {
                let dict = PyDict::new(py);
                dict.set_item("a", a).unwrap();
                dict.set_item("b", b).unwrap();
                dict
            }))
    }
}

// ---------------------------------------------------------------------------
// Slice
// ---------------------------------------------------------------------------

#[pyclass(name = "Slice")]
pub struct PySlice {
    pub(crate) inner: BSlice,
}

#[pymethods]
impl PySlice {
    #[new]
    fn new(content: &PyFragment, open_start: usize, open_end: usize) -> Self {
        PySlice {
            inner: BSlice::new(&content.inner, open_start, open_end),
        }
    }

    #[getter]
    fn content(&self) -> PyResult<PyFragment> {
        Ok(PyFragment {
            inner: self.inner.content(),
        })
    }

    #[getter]
    fn open_start(&self) -> usize {
        self.inner.open_start()
    }

    #[getter]
    fn open_end(&self) -> usize {
        self.inner.open_end()
    }

    #[getter]
    fn size(&self) -> usize {
        self.inner.size()
    }

    fn eq(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other = other.cast::<PySlice>()?.borrow();
        Ok(self.inner.eq(&other.inner))
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        if let Ok(other) = other.cast::<PySlice>() {
            let other = other.borrow();
            match op {
                pyo3::basic::CompareOp::Eq => Ok(self.inner.inner == other.inner.inner),
                pyo3::basic::CompareOp::Ne => Ok(self.inner.inner != other.inner.inner),
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    fn __str__(&self) -> String {
        let content_str = PyFragment {
            inner: self.inner.content(),
        }
        .__str__();
        format!(
            "{content_str}({},{})",
            self.inner.open_start(),
            self.inner.open_end()
        )
    }

    fn __repr__(&self) -> String {
        format!("<Slice {}>", self.__str__())
    }
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

#[pyclass(name = "Node", dict)]
pub struct PyNode {
    pub(crate) inner: BNode,
}

#[pymethods]
impl PyNode {
    #[staticmethod]
    fn from_json(schema: &PySchema, json: &Bound<'_, PyAny>) -> PyResult<PyNode> {
        let val = if let Ok(s) = json.extract::<String>() {
            serde_json::from_str(&s)
                .map_err(|e| PyValueError::new_err(format!("Invalid JSON string: {e}")))?
        } else {
            py_to_json(json)?
        };
        let bnode = BNode::from_json(&schema.inner, &val)
            .map_err(|e| PyValueError::new_err(format!("Invalid node JSON: {e}")))?;
        Ok(PyNode { inner: bnode })
    }

    #[getter]
    fn type_(&self) -> PyResult<PyNodeType> {
        Ok(PyNodeType {
            inner: self.inner.type_(),
        })
    }

    #[getter]
    #[pyo3(name = "type")]
    fn py_type(&self) -> PyResult<PyNodeType> {
        self.type_()
    }

    #[getter]
    fn content(&self) -> PyResult<Option<PyFragment>> {
        Ok(self.inner.content().map(|bf| PyFragment { inner: bf }))
    }

    #[getter]
    fn marks(&self) -> PyResult<PyMarkSet> {
        let marks_vec = self.inner.marks_vec();
        Ok(PyMarkSet {
            schema: self.inner.schema.clone(),
            inner: MarkSet::from_vec(marks_vec),
        })
    }

    #[getter]
    fn attrs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &self.inner.attrs_json()).map(|b| b.unbind())
    }

    #[getter]
    fn text_content(&self) -> String {
        self.inner.text_content()
    }

    #[getter]
    fn text(&self) -> Option<String> {
        self.inner.text()
    }

    #[getter]
    fn node_size(&self) -> usize {
        self.inner.node_size()
    }

    #[getter]
    fn child_count(&self) -> usize {
        self.inner.child_count()
    }

    #[getter]
    fn first_child(&self) -> PyResult<Option<PyNode>> {
        Ok(self.inner.first_child().map(|bn| PyNode { inner: bn }))
    }

    #[getter]
    fn last_child(&self) -> PyResult<Option<PyNode>> {
        Ok(self.inner.last_child().map(|bn| PyNode { inner: bn }))
    }

    #[getter]
    fn is_text(&self) -> bool {
        self.inner.is_text()
    }

    #[getter]
    fn is_block(&self) -> bool {
        self.inner.is_block()
    }

    #[getter]
    fn is_leaf(&self) -> bool {
        self.inner.is_leaf()
    }

    fn child(&self, index: usize) -> PyResult<PyNode> {
        self.inner
            .child(index)
            .map(|bn| PyNode { inner: bn })
            .ok_or_else(|| PyValueError::new_err(format!("child index {index} out of bounds")))
    }

    #[pyo3(signature = (from_, to=None, include_parents=false))]
    fn slice(&self, from_: usize, to: Option<usize>, include_parents: bool) -> PyResult<PySlice> {
        let to = to.unwrap_or_else(|| self.inner.content_size());
        Ok(PySlice {
            inner: self.inner.slice(from_, to, include_parents),
        })
    }

    #[pyo3(signature = (from_=0, to=None))]
    fn cut(&self, from_: usize, to: Option<usize>) -> PyResult<PyNode> {
        let to = to.unwrap_or_else(|| self.inner.content_size());
        Ok(PyNode {
            inner: self.inner.cut(from_, to),
        })
    }

    fn replace(&self, from: usize, to: usize, slice: &PySlice) -> PyResult<PyNode> {
        self.inner
            .replace(from, to, &slice.inner)
            .map(|bn| PyNode { inner: bn })
            .map_err(|e| PyValueError::new_err(e))
    }

    fn mark(&self, marks: &Bound<'_, PyAny>) -> PyResult<PyNode> {
        let marks = extract_markset(marks)?;
        Ok(PyNode {
            inner: self.inner.mark(marks),
        })
    }

    fn copy(&self, content: &PyFragment) -> PyResult<PyNode> {
        Ok(PyNode {
            inner: self.inner.copy(content.inner.inner.clone()),
        })
    }

    fn check(&self) -> PyResult<()> {
        self.inner.check().map_err(|e| PyValueError::new_err(e))
    }

    fn node_at(&self, pos: usize) -> PyResult<Option<PyNode>> {
        Ok(self.inner.node_at(pos).map(|bn| PyNode { inner: bn }))
    }

    #[pyo3(signature = (type_, attrs=None, marks=None))]
    fn has_markup(
        &self,
        type_: &PyNodeType,
        attrs: Option<&Bound<'_, PyAny>>,
        marks: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        let attrs_val = attrs.map(py_to_json).transpose()?;
        let raw_marks: Option<Vec<_>> = marks
            .map(|m| {
                let set = extract_markset(m)?;
                Ok::<_, PyErr>(set.iter().cloned().collect::<Vec<_>>())
            })
            .transpose()?;
        Ok(self
            .inner
            .has_markup(&type_.inner, attrs_val.as_ref(), raw_marks.as_deref()))
    }

    #[pyo3(signature = (from_, to, replacement=None, start=0, end=None))]
    fn can_replace(
        &self,
        from_: usize,
        to: usize,
        replacement: Option<&PyFragment>,
        start: usize,
        end: Option<usize>,
    ) -> bool {
        self.inner
            .can_replace(from_, to, replacement.map(|f| &f.inner), start, end)
    }

    fn can_replace_with(&self, from_: usize, to: usize, type_: &PyNodeType) -> bool {
        self.inner.can_replace_with(from_, to, &type_.inner)
    }

    fn child_after(&self, pos: usize, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match self.inner.child_after(pos) {
            None => Ok(None),
            Some((node, index, offset)) => {
                let d = PyDict::new(py);
                d.set_item("node", PyNode { inner: node })?;
                d.set_item("index", index)?;
                d.set_item("offset", offset)?;
                Ok(Some(d.into_any().unbind()))
            }
        }
    }

    fn child_before(&self, pos: usize, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match self.inner.child_before(pos) {
            None => Ok(None),
            Some((node, index, offset)) => {
                let d = PyDict::new(py);
                d.set_item("node", PyNode { inner: node })?;
                d.set_item("index", index)?;
                d.set_item("offset", offset)?;
                Ok(Some(d.into_any().unbind()))
            }
        }
    }

    fn maybe_child(&self, index: usize) -> Option<PyNode> {
        self.inner.maybe_child(index).map(|bn| PyNode { inner: bn })
    }

    #[getter]
    fn is_inline(&self) -> bool {
        self.inner.is_inline()
    }

    #[getter]
    fn is_textblock(&self) -> bool {
        self.inner.is_textblock()
    }

    #[getter]
    fn is_atom(&self) -> bool {
        self.inner.is_atom()
    }

    #[getter]
    fn inline_content(&self) -> bool {
        self.inner.inline_content()
    }

    fn same_markup(&self, other: &PyNode) -> bool {
        self.inner.same_markup(&other.inner)
    }

    fn range_has_mark(&self, from: usize, to: usize, mark_type: &PyMarkType) -> bool {
        self.inner.range_has_mark(from, to, mark_type.inner.inner)
    }

    fn can_append(&self, other: &PyNode) -> bool {
        self.inner.can_append(&other.inner)
    }

    fn content_match_at(&self, index: usize) -> PyResult<PyContentMatch> {
        self.inner
            .content_match_at(index)
            .map(|cm| PyContentMatch { inner: cm })
            .map_err(|e| PyValueError::new_err(e))
    }

    fn for_each(&self, py: Python<'_>, f: Py<PyAny>) -> PyResult<()> {
        let mut items: Vec<(DynamicNode, usize, usize)> = Vec::new();
        self.inner
            .for_each(&mut |n, o, i| items.push((n.clone(), o, i)));
        for (node, offset, index) in items {
            f.call1(
                py,
                (
                    PyNode {
                        inner: BNode {
                            schema: self.inner.schema.clone(),
                            inner: node,
                        },
                    },
                    offset,
                    index,
                ),
            )?;
        }
        Ok(())
    }

    #[pyo3(signature = (from_, to, f, start_pos=0))]
    fn nodes_between(
        &self,
        py: Python<'_>,
        from_: usize,
        to: usize,
        f: Py<PyAny>,
        start_pos: usize,
    ) -> PyResult<()> {
        let schema = self.inner.schema.clone();
        let mut err: Option<pyo3::PyErr> = None;
        schema.with_types(|| {
            <DynamicNode as Node<Dyn>>::nodes_between(
                &self.inner.inner,
                from_,
                to,
                &mut |n, p, parent, index| {
                    if err.is_some() {
                        return false;
                    }
                    let result = (|| -> PyResult<bool> {
                        let py_node = PyNode {
                            inner: BNode {
                                schema: schema.clone(),
                                inner: n.clone(),
                            },
                        };
                        let py_parent = parent.map(|par| PyNode {
                            inner: BNode {
                                schema: schema.clone(),
                                inner: par.clone(),
                            },
                        });
                        let ret = f.call1(py, (py_node, p, py_parent, index))?;
                        if let Ok(b) = ret.extract::<bool>(py) {
                            if !b {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    })();
                    match result {
                        Ok(v) => v,
                        Err(e) => {
                            err = Some(e);
                            false
                        }
                    }
                },
                start_pos,
            );
        });
        if let Some(e) = err {
            return Err(e);
        }
        Ok(())
    }

    fn descendants(&self, py: Python<'_>, f: Py<PyAny>) -> PyResult<()> {
        let schema = self.inner.schema.clone();
        let mut err: Option<pyo3::PyErr> = None;
        schema.with_types(|| {
            <DynamicNode as Node<Dyn>>::descendants(
                &self.inner.inner,
                &mut |n, p, parent, index| {
                    if err.is_some() {
                        return false;
                    }
                    let result = (|| -> PyResult<bool> {
                        let py_node = PyNode {
                            inner: BNode {
                                schema: schema.clone(),
                                inner: n.clone(),
                            },
                        };
                        let py_parent = parent.map(|par| PyNode {
                            inner: BNode {
                                schema: schema.clone(),
                                inner: par.clone(),
                            },
                        });
                        let ret = f.call1(py, (py_node, p, py_parent, index))?;
                        if let Ok(b) = ret.extract::<bool>(py) {
                            if !b {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    })();
                    match result {
                        Ok(v) => v,
                        Err(e) => {
                            err = Some(e);
                            false
                        }
                    }
                },
            );
        });
        if let Some(e) = err {
            return Err(e);
        }
        Ok(())
    }

    #[pyo3(signature = (from_, to, block_separator=None, leaf_text=None))]
    fn text_between(
        &self,
        from_: usize,
        to: usize,
        block_separator: Option<&str>,
        leaf_text: Option<&str>,
    ) -> PyResult<String> {
        Ok(self
            .inner
            .text_between(from_, to, block_separator, leaf_text))
    }

    fn resolve(&self, pos: usize) -> PyResult<PyResolvedPos> {
        Ok(PyResolvedPos {
            inner: self.inner.resolve(pos),
        })
    }

    fn eq(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other = other.cast::<PyNode>()?.borrow();
        Ok(self.inner.eq(&other.inner))
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        if let Ok(other) = other.cast::<PyNode>() {
            let other = other.borrow();
            match op {
                pyo3::basic::CompareOp::Eq => Ok(self.inner.eq(&other.inner)),
                pyo3::basic::CompareOp::Ne => Ok(!self.inner.eq(&other.inner)),
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    fn to_json(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let val = self.inner.to_json(false);
        json_to_py(py, &val).map(|b| b.unbind())
    }

    fn __str__(&self) -> String {
        self.inner.to_debug_string()
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
    pub(crate) inner: BResolvedPos,
}

#[pymethods]
impl PyResolvedPos {
    #[getter]
    fn pos(&self) -> usize {
        self.inner.pos
    }

    #[getter]
    fn depth(&self) -> usize {
        self.inner.depth()
    }

    #[getter]
    fn parent_offset(&self) -> usize {
        self.inner.parent_offset()
    }

    fn node(&self, depth: usize) -> PyResult<PyNode> {
        Ok(PyNode {
            inner: self.inner.node(Some(depth)),
        })
    }

    fn start(&self, depth: usize) -> PyResult<usize> {
        Ok(self.inner.start(Some(depth)))
    }

    fn end(&self, depth: usize) -> PyResult<usize> {
        Ok(self.inner.end(Some(depth)))
    }

    fn before(&self, depth: usize) -> PyResult<Option<usize>> {
        Ok(self.inner.before(Some(depth)))
    }

    fn after(&self, depth: usize) -> PyResult<Option<usize>> {
        Ok(self.inner.after(Some(depth)))
    }

    #[getter]
    fn node_before(&self) -> PyResult<Option<PyNode>> {
        Ok(self.inner.node_before().map(|bn| PyNode { inner: bn }))
    }

    #[getter]
    fn node_after(&self) -> PyResult<Option<PyNode>> {
        Ok(self.inner.node_after().map(|bn| PyNode { inner: bn }))
    }

    fn pos_at_index(&self, index: usize, depth: Option<usize>) -> PyResult<usize> {
        Ok(self.inner.pos_at_index(index, depth))
    }

    fn marks(&self) -> PyResult<Vec<PyMark>> {
        Ok(self
            .inner
            .marks()
            .into_iter()
            .map(|m| PyMark {
                inner: BMark {
                    schema: self.inner.schema.clone(),
                    inner: m,
                },
            })
            .collect())
    }

    #[getter]
    fn parent(&self) -> PyNode {
        PyNode {
            inner: self.inner.parent(),
        }
    }

    #[getter]
    fn doc(&self) -> PyNode {
        PyNode {
            inner: self.inner.doc_node(),
        }
    }

    #[getter]
    fn text_offset(&self) -> usize {
        self.inner.text_offset()
    }

    #[pyo3(signature = (depth=None))]
    fn index(&self, depth: Option<usize>) -> usize {
        self.inner.index(depth)
    }

    #[pyo3(signature = (depth=None))]
    fn index_after(&self, depth: Option<usize>) -> usize {
        self.inner.index_after(depth)
    }

    fn shared_depth(&self, pos: usize) -> usize {
        self.inner.shared_depth(pos)
    }

    fn marks_across(&self, end: &PyResolvedPos) -> PyResult<Option<Vec<PyMark>>> {
        Ok(self.inner.marks_across(&end.inner).map(|ms| {
            ms.into_iter()
                .map(|m| PyMark {
                    inner: BMark {
                        schema: self.inner.schema.clone(),
                        inner: m,
                    },
                })
                .collect()
        }))
    }

    fn same_parent(&self, other: &PyResolvedPos) -> bool {
        self.inner.same_parent(&other.inner)
    }

    fn max(&self, other: &PyResolvedPos) -> PyResolvedPos {
        PyResolvedPos {
            inner: self.inner.max(&other.inner),
        }
    }

    fn min(&self, other: &PyResolvedPos) -> PyResolvedPos {
        PyResolvedPos {
            inner: self.inner.min(&other.inner),
        }
    }

    fn __str__(&self) -> String {
        self.inner.schema.with_types(|| {
            if let Ok(r) = ResolvedPos::<Dyn>::resolve(&self.inner.doc, self.inner.pos) {
                let path: Vec<String> = (1..=r.depth)
                    .map(|i| format!("{}_{}", r.node(i).type_name, r.index(i - 1)))
                    .collect();
                format!("{}:{}", path.join("/"), r.parent_offset)
            } else {
                format!("invalid:{}", self.inner.pos)
            }
        })
    }

    #[pyo3(signature = (other=None))]
    fn block_range(&self, other: Option<&PyResolvedPos>) -> PyResult<Option<PyNodeRange>> {
        Ok(self
            .inner
            .block_range(other.map(|o| &o.inner))
            .map(|bnr| PyNodeRange { inner: bnr }))
    }

    fn __repr__(&self) -> String {
        format!("<ResolvedPos {}>", self.__str__())
    }
}

// ---------------------------------------------------------------------------
// NodeRange
// ---------------------------------------------------------------------------

#[pyclass(name = "NodeRange")]
pub struct PyNodeRange {
    pub(crate) inner: BNodeRange,
}

#[pymethods]
impl PyNodeRange {
    #[new]
    fn new(from: &PyResolvedPos, to: &PyResolvedPos, depth: Option<usize>) -> PyResult<Self> {
        let depth = depth.unwrap_or_else(|| from.inner.shared_depth(to.inner.pos));
        Ok(PyNodeRange {
            inner: BNodeRange {
                schema: from.inner.schema.clone(),
                doc: from.inner.doc.clone(),
                from_pos: from.inner.pos,
                to_pos: to.inner.pos,
                depth,
            },
        })
    }

    #[getter]
    fn depth(&self) -> usize {
        self.inner.depth()
    }

    #[getter]
    fn start(&self) -> usize {
        self.inner.start()
    }

    #[getter]
    fn end(&self) -> usize {
        self.inner.end()
    }

    #[getter]
    fn parent(&self) -> PyNode {
        PyNode {
            inner: self.inner.parent(),
        }
    }

    #[getter]
    fn start_index(&self) -> usize {
        self.inner.start_index()
    }

    #[getter]
    fn end_index(&self) -> usize {
        self.inner.end_index()
    }

    /// The start position of this range.
    #[getter]
    fn from_pos(&self) -> usize {
        self.inner.from_pos
    }

    /// The end position of this range.
    #[getter]
    fn to_pos(&self) -> usize {
        self.inner.to_pos
    }

    /// The resolved start position of this range (JS: `$from`,
    /// Python keyword-safe: `from_`).
    #[getter]
    fn from_(&self) -> PyResolvedPos {
        PyResolvedPos {
            inner: self.inner.from_resolved_pos(),
        }
    }

    /// The resolved end position of this range (JS: `$to`).
    #[getter]
    fn to(&self) -> PyResolvedPos {
        PyResolvedPos {
            inner: self.inner.to_resolved_pos(),
        }
    }
}

// ---------------------------------------------------------------------------
// ContentMatch
// ---------------------------------------------------------------------------

#[pyclass(name = "ContentMatch")]
pub struct PyContentMatch {
    pub(crate) inner: BContentMatch,
}

#[pymethods]
impl PyContentMatch {
    #[staticmethod]
    fn parse(expr: &str, node_types: &Bound<'_, PyDict>) -> PyResult<Self> {
        let mut schema: Option<Arc<DynamicSchema>> = None;
        for item in node_types.iter() {
            let (_, value) = item;
            if let Ok(py_node_type) = value.cast::<PyNodeType>() {
                let nt = py_node_type.borrow();
                schema = Some(nt.inner.schema.clone());
                break;
            }
        }
        let schema = schema.ok_or_else(|| PyValueError::new_err("No valid node types provided"))?;
        let inner = BContentMatch::parse(expr, &schema)
            .map_err(|e| PyValueError::new_err(format!("Content expression parse error: {e}")))?;
        Ok(PyContentMatch { inner })
    }

    #[getter]
    fn valid_end(&self) -> bool {
        self.inner.valid_end()
    }

    fn match_type(&self, node_type: &PyNodeType) -> Option<PyContentMatch> {
        self.inner
            .match_type(&node_type.inner)
            .map(|cm| PyContentMatch { inner: cm })
    }

    fn match_fragment(&self, fragment: &PyFragment) -> Option<PyContentMatch> {
        self.inner
            .match_fragment(&fragment.inner, 0, None)
            .map(|cm| PyContentMatch { inner: cm })
    }

    #[pyo3(signature = (fragment, to_end=false, start_index=0))]
    fn fill_before(
        &self,
        fragment: &PyFragment,
        to_end: bool,
        start_index: usize,
    ) -> Option<PyFragment> {
        self.inner
            .fill_before(&fragment.inner, to_end, start_index)
            .map(|bf| PyFragment { inner: bf })
    }

    #[getter]
    fn default_type(&self) -> Option<PyNodeType> {
        self.inner.default_type().map(|nt| PyNodeType { inner: nt })
    }

    fn find_wrapping(&self, target: &PyNodeType) -> Option<Vec<PyNodeType>> {
        self.inner.find_wrapping(&target.inner).map(|types| {
            types
                .into_iter()
                .map(|nt| PyNodeType { inner: nt })
                .collect()
        })
    }

    #[getter]
    fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    fn edge_type(&self, n: usize) -> Option<PyNodeType> {
        self.inner.edge(n).map(|(nt, _)| PyNodeType { inner: nt })
    }

    fn edge_match(&self, n: usize) -> Option<PyContentMatch> {
        self.inner
            .edge(n)
            .map(|(_, cm)| PyContentMatch { inner: cm })
    }
}
