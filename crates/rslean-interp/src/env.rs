use crate::error::{InterpError, InterpResult};
use crate::value::Value;

/// A local variable environment indexed by de Bruijn indices.
///
/// Index 0 is the most recently bound variable (front of the list).
#[derive(Clone, Debug)]
pub struct LocalEnv {
    bindings: Vec<Value>,
}

impl LocalEnv {
    pub fn new() -> Self {
        LocalEnv {
            bindings: Vec::new(),
        }
    }

    /// Push a value onto the front of the environment (becomes index 0).
    pub fn push(&self, val: Value) -> Self {
        let mut bindings = Vec::with_capacity(self.bindings.len() + 1);
        bindings.push(val);
        bindings.extend_from_slice(&self.bindings);
        LocalEnv { bindings }
    }

    /// Look up a value by de Bruijn index.
    pub fn lookup(&self, idx: u64) -> InterpResult<&Value> {
        self.bindings
            .get(idx as usize)
            .ok_or(InterpError::UnboundVar(idx))
    }

    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

impl Default for LocalEnv {
    fn default() -> Self {
        Self::new()
    }
}
