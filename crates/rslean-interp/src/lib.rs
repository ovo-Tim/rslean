pub mod builtins;
pub mod env;
pub mod error;
pub mod eval;
pub mod iota;
pub mod value;

#[cfg(test)]
mod tests;

pub use env::LocalEnv;
pub use error::{InterpError, InterpResult};
pub use eval::Interpreter;
pub use value::{FuncRef, Value};
