//! Adapter for unmodified upstream Rustel's versioned stdio interface.
mod adapter;
#[cfg(not(target_arch = "wasm32"))]
mod host;
pub use adapter::{Adapter, Capabilities, Output};
#[cfg(not(target_arch = "wasm32"))]
pub use host::{Config, Hosted, ScoreValidator};
