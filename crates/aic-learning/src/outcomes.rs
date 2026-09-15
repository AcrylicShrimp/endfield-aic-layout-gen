//! Bounded procedural physical-action outcome datasets.

pub mod config;
#[cfg(feature = "libtorch")]
pub(crate) mod live;
pub mod run;
pub mod sample;
pub mod storage;

pub use run::{collect, validate};

#[cfg(test)]
mod tests;
