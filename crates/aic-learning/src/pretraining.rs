//! Bounded multi-head learning from observed common-editor outcomes.

mod config;
mod model;
mod online;
mod run;

pub use online::run as run_online;
pub use run::run;

#[cfg(test)]
mod tests;
