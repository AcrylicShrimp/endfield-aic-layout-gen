//! Owned numerical exports; command domains remain behind native handles.

use numpy::{IntoPyArray, ndarray::Array2};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::*;

fn matrix<'py, T: numpy::Element, const N: usize>(
    py: Python<'py>,
    rows: Vec<[T; N]>,
) -> Bound<'py, numpy::PyArray2<T>> {
    let count = rows.len();
    Array2::from_shape_vec((count, N), rows.into_flattened())
        .unwrap()
        .into_pyarray(py)
}

pub(crate) fn export<'py>(
    py: Python<'py>,
    value: Input,
    learning: &Learning,
    session: &Session,
    metadata: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("schema", "declarative-learning-graph-v1")?;
    result.set_item("native_domain", "declarative-native-domain-v1")?;
    result.set_item("native_handle", learning.handle)?;
    result.set_item("blueprint_sha256", &session.blueprint_sha256)?;
    result.set_item("revision", session.revision)?;
    result.set_item("attempts", session.attempts)?;
    result.set_item("done", session.done())?;
    result.set_item("pending", value.pending)?;
    result.set_item("conflicts", value.conflicts)?;
    result.set_item("elapsed_ms", session.elapsed().as_secs_f64() * 1000.)?;
    result.set_item("node_static", matrix(py, value.node_static))?;
    result.set_item("node_state", matrix(py, value.node_state))?;
    result.set_item("edge_index_rows", matrix(py, value.edge_index))?;
    result.set_item("edge_static", matrix(py, value.edge_static))?;
    result.set_item("edge_state", matrix(py, value.edge_state))?;
    result.set_item("path_cells", matrix(py, value.path_cells))?;
    result.set_item("path_edges", value.path_edges.into_pyarray(py))?;
    result.set_item("global_features", value.global_features.into_pyarray(py))?;
    result.set_item("action_features", matrix(py, value.action_features))?;
    result.set_item("action_references", matrix(py, value.action_references))?;
    result.set_item("root_meta", matrix(py, value.root_meta))?;
    result.set_item("kind_log_counts", value.kind_log_counts.into_pyarray(py))?;
    result.set_item("port_features", matrix(py, value.port_features))?;
    result.set_item("port_metadata", metadata)?;
    if learning.options.masking.is_some() {
        result.set_item("root_mask_schema", "declarative-root-mask-v1")?;
        result.set_item("root_invalid", value.root_invalid.into_pyarray(py))?;
    }
    Ok(result)
}
