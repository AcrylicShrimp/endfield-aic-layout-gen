//! Native numerical learner. Domain logic and state live in explicit modules;
//! CLI and diagnostic data export are separate from this crate.
#[cfg(feature = "libtorch")]
pub mod input;
#[cfg(feature = "libtorch")]
pub mod model;
#[cfg(feature = "outcomes")]
pub mod outcomes;
#[cfg(feature = "libtorch")]
pub mod pretraining;
pub mod schema;
#[cfg(feature = "libtorch")]
pub mod update;

#[cfg(feature = "libtorch")]
pub mod collection;
#[cfg(feature = "libtorch")]
pub mod constructive;
#[cfg(feature = "libtorch")]
pub mod training;

// LibTorch's cold initializer uses a process-global seed. Serialize tests that
// initialize multiple comparison models so unrelated tests cannot reseed them.
#[cfg(all(test, feature = "libtorch"))]
pub(crate) static TORCH_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
