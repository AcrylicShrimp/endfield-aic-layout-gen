//! Seeded construction of complete routed physical layouts.
mod config;
mod generate;
mod packing;
pub use config::Config;
pub use generate::{Attempt, Failure, Generated, Generator};
pub mod batch;
pub mod io;
pub mod render;
pub mod survey;
#[cfg(test)]
mod tests;
