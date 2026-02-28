#![allow(clippy::arc_with_non_send_sync)]

pub mod builtins;
pub mod env;
pub mod error;
pub mod eval;
pub mod iota;
pub mod loader;
pub mod value;

#[cfg(test)]
mod tests;

pub use env::LocalEnv;
pub use error::{InterpError, InterpResult};
pub use eval::Interpreter;
pub use value::{FuncRef, Value};
