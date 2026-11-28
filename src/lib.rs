mod app;
#[cfg(not(target_arch = "wasm32"))]
mod bridge;
#[cfg(target_arch = "wasm32")]
#[path = "web_bridge.rs"]
mod bridge;
mod charts;
mod model;
#[cfg(not(target_arch = "wasm32"))]
mod native;
mod provenance;
mod stage;
mod state;
#[cfg(not(target_arch = "wasm32"))]
pub use native::run as run_native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
mod music_mcp;
