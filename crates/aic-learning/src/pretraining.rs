//! Bounded multi-head learning from observed common-editor outcomes.

mod config;
mod evaluate;
mod model;
mod online;
mod run;

pub use evaluate::run as evaluate;
pub use online::run as run_online;
pub use run::run;

#[cfg(test)]
mod tests;
