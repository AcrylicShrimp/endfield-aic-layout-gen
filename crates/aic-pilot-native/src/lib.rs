//! Rust-owned factory edit environment with a narrow Python learning boundary.

mod acceptance;
mod cache;
mod candidates;
mod catalog;
mod command;
mod diagnostic;
mod edits;
mod engine;
mod observation;
mod python_engine;
mod python_factory;
mod state;
mod transition;

use pyo3::prelude::*;

#[pymodule]
fn aic_pilot_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<python_engine::PythonHandle>()?;
    module.add_class::<python_engine::PythonEngine>()?;
    module.add_class::<python_factory::PythonFactory>()?;
    Ok(())
}
