//! Declarative factory geometry, topology and bounded routing.
pub(crate) mod blueprint;
mod catalog;
mod chains;
pub mod generation;
pub(crate) mod identity;
use aic_routing as path;
#[cfg(test)]
mod tests;
mod types;
