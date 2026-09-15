//! Bounded multi-head learning from observed common-editor outcomes.

mod config;
mod model;
mod run;

pub use run::run;

#[cfg(test)]
mod tests;
