//! Retained complete-factory environment. The physical authority remains aic-data.

pub(crate) mod blueprint;
mod candidates;
mod catalog;
mod chains;
mod changes;
mod engine;
mod groups;
mod identity;
mod movement;
mod observation;
mod path;
mod poses;
pub(crate) mod python;
mod routing;
mod selection;
mod spatial;
mod strips;
mod types;

#[cfg(test)]
mod tests;
