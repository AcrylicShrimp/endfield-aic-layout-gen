//! Shared physical catalogs, geometry, routed layouts and boundary validation.
//! No solver, learner, game constants or runtime catalog I/O in hot operations.
pub mod facilities;
pub mod geometry;
pub mod logistics;
pub mod physical;
pub mod physical_identity;
pub mod projection;
pub mod rate;
pub mod rng;
pub mod stable_id;
pub mod validation;
