//! Rust-owned factory edit environment with a narrow Python learning boundary.

#[cfg(feature = "python")]
mod acceptance;
#[cfg(feature = "python")]
mod cache;
#[cfg(feature = "python")]
mod candidates;
#[cfg(feature = "python")]
mod catalog;
#[cfg(feature = "python")]
mod command;
#[cfg(feature = "python")]
mod diagnostic;
#[cfg(feature = "python")]
mod edits;
#[cfg(feature = "python")]
mod engine;
// Legacy factory/blueprint APIs in this shared engine are reached through
// Python only; the declarative Rust facade uses a strict subset.
#[cfg_attr(not(feature = "python"), allow(dead_code, unused_imports))]
mod full_factory;
pub mod input;
pub mod learning;
#[cfg(feature = "python")]
mod observation;
#[cfg(feature = "python")]
mod python_engine;
#[cfg(feature = "python")]
mod python_factory;
#[cfg(feature = "python")]
mod state;
#[cfg(feature = "python")]
mod transition;

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
#[pymodule]
fn aic_pilot_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<python_engine::PythonHandle>()?;
    module.add_class::<python_engine::PythonEngine>()?;
    module.add_class::<python_factory::PythonFactory>()?;
    module.add_class::<full_factory::python::PythonRuntime>()?;
    module.add_class::<full_factory::python::PythonEpisode>()?;
    module.add_class::<full_factory::python::PythonHandle>()?;
    module.add_class::<full_factory::blueprint::python::PythonSession>()?;
    module.add_class::<full_factory::blueprint::declarative::python::PythonSession>()?;
    Ok(())
}
