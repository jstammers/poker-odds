pub mod cards;
pub mod eval;
pub mod game;
pub mod sim;

#[cfg(not(target_arch = "wasm32"))]
pub mod tui;

#[cfg(target_arch = "wasm32")]
pub mod wasm;
