//! Bounded procedural physical-action outcome datasets.

pub mod config;
pub mod run;
pub mod sample;
pub mod storage;

pub use run::{collect, validate};

#[cfg(test)]
mod tests;
