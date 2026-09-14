//! LibTorch primitives missing from tch, without application semantics.
//! Enable `libtorch` explicitly; ordinary workspace builds need no Torch SDK.

#[cfg(feature = "libtorch")]
#[allow(unsafe_code)]
mod bridge;
#[cfg(feature = "libtorch")]
mod optimizer;
#[cfg(feature = "libtorch")]
mod rng;

#[cfg(feature = "libtorch")]
pub use optimizer::{Adam, AdamBackend, AdamState};
#[cfg(feature = "libtorch")]
pub use rng::{Generator, deterministic};
#[cfg(feature = "libtorch")]
pub use tch;
