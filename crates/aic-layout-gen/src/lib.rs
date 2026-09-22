//! Seeded construction of complete routed physical layouts.
mod config;
mod generate;
mod intent;
mod packing;
pub mod placement;
pub use config::Config as MechanismConfig;
pub use generate::{Attempt, Failure, Generated, Generator};
pub use intent::{
    Config, Crossings, Effort, Intent, IntentFailure, IntentGenerated, IntentGenerator,
    PackingProfile, Request, ResolvedPolicy, TopologyProfile,
};
pub mod batch;
pub mod io;
pub mod render;
pub mod survey;
#[cfg(test)]
mod tests;
