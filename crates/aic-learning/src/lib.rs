//! Native numerical learner. Domain logic and state live in explicit modules;
//! CLI and diagnostic data export are separate from this crate.
#[cfg(feature = "libtorch")]
pub mod input;
#[cfg(feature = "libtorch")]
pub mod model;
pub mod schema;
#[cfg(feature = "libtorch")]
pub mod update;

#[cfg(feature = "libtorch")]
pub mod collection;
