use crate::error::{KernelError, KernelResult};
use rslean_expr::ConstantInfo;
use rslean_name::Name;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// The Lean 4 environment: an immutable map from names to constant declarations.
///
/// Environments are persistent — `add` returns a new environment.
#[derive(Clone)]
pub struct Environment {
    inner: Arc<EnvironmentInner>,
}

#[derive(Clone)]
struct EnvironmentInner {
    constants: FxHashMap<Name, ConstantInfo>,
    quot_initialized: bool,
}

impl Environment {
    /// Create an empty environment.
    pub fn new() -> Self {
        Environment {
            inner: Arc::new(EnvironmentInner {
                constants: FxHashMap::default(),
                quot_initialized: false,
            }),
        }
    }

    /// Create an environment from a pre-built constants map.
    pub fn from_constants(constants: FxHashMap<Name, ConstantInfo>) -> Self {
        Environment {
            inner: Arc::new(EnvironmentInner {
                constants,
                quot_initialized: false,
            }),
        }
    }

    /// Look up a constant by name.
    pub fn find(&self, name: &Name) -> Option<&ConstantInfo> {
        self.inner.constants.get(name)
    }

    /// Look up a constant, returning an error if not found.
    pub fn get(&self, name: &Name) -> KernelResult<&ConstantInfo> {
        self.find(name)
            .ok_or_else(|| KernelError::UnknownConstant(name.clone()))
    }

    /// Add a constant to the environment. Returns a new environment.
    pub fn add_constant(&self, info: ConstantInfo) -> KernelResult<Self> {
        let name = info.name().clone();
        if self.inner.constants.contains_key(&name) {
            return Err(KernelError::AlreadyDeclared(name));
        }
        let mut new_constants = self.inner.constants.clone();
        new_constants.insert(name, info);
        Ok(Environment {
            inner: Arc::new(EnvironmentInner {
                constants: new_constants,
                quot_initialized: self.inner.quot_initialized,
            }),
        })
    }

    /// Add a constant without checking for duplicates.
    /// Uses Arc::make_mut for copy-on-write — cheap when there's only one reference.
    pub fn add_constant_unchecked(&mut self, info: ConstantInfo) {
        let inner = Arc::make_mut(&mut self.inner);
        inner.constants.insert(info.name().clone(), info);
    }

    /// Mark quotient types as initialized.
    pub fn set_quot_initialized(&self) -> Self {
        Environment {
            inner: Arc::new(EnvironmentInner {
                constants: self.inner.constants.clone(),
                quot_initialized: true,
            }),
        }
    }

    pub fn is_quot_initialized(&self) -> bool {
        self.inner.quot_initialized
    }

    /// Number of constants in the environment.
    pub fn num_constants(&self) -> usize {
        self.inner.constants.len()
    }

    /// Iterate over all constants.
    pub fn for_each_constant(&self, mut f: impl FnMut(&ConstantInfo)) {
        for info in self.inner.constants.values() {
            f(info);
        }
    }

    /// Check if a name is an inductive type.
    pub fn is_inductive(&self, name: &Name) -> bool {
        matches!(self.find(name), Some(ConstantInfo::Inductive { .. }))
    }

    /// Check if a name is a constructor.
    pub fn is_constructor(&self, name: &Name) -> bool {
        matches!(self.find(name), Some(ConstantInfo::Constructor { .. }))
    }

    /// Check if a name is a recursor.
    pub fn is_recursor(&self, name: &Name) -> bool {
        matches!(self.find(name), Some(ConstantInfo::Recursor { .. }))
    }
}

impl Default for Environment {
    fn default() -> Self {
        Environment::new()
    }
}

impl std::fmt::Debug for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Environment({} constants)", self.num_constants())
    }
}

#[derive(Serialize, Deserialize)]
struct EnvironmentData {
    constants: Vec<ConstantInfo>,
    quot_initialized: bool,
}

impl Serialize for Environment {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let data = EnvironmentData {
            constants: self.inner.constants.values().cloned().collect(),
            quot_initialized: self.inner.quot_initialized,
        };
        data.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Environment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let data = EnvironmentData::deserialize(deserializer)?;
        let mut constants = FxHashMap::default();
        constants.reserve(data.constants.len());
        for ci in data.constants {
            constants.insert(ci.name().clone(), ci);
        }
        Ok(Environment {
            inner: Arc::new(EnvironmentInner {
                constants,
                quot_initialized: data.quot_initialized,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rslean_expr::Expr;

    #[test]
    fn test_empty_env() {
        let env = Environment::new();
        assert_eq!(env.num_constants(), 0);
        assert!(env.find(&Name::mk_simple("Nat")).is_none());
    }

    #[test]
    fn test_add_and_find() {
        let env = Environment::new();
        let nat = ConstantInfo::Axiom {
            name: Name::mk_simple("Nat"),
            level_params: vec![],
            type_: Expr::type_(),
            is_unsafe: false,
        };
        let env2 = env.add_constant(nat).unwrap();
        assert_eq!(env2.num_constants(), 1);
        assert!(env2.find(&Name::mk_simple("Nat")).is_some());
        // Original env unchanged
        assert_eq!(env.num_constants(), 0);
    }

    #[test]
    fn test_duplicate_error() {
        let env = Environment::new();
        let nat = ConstantInfo::Axiom {
            name: Name::mk_simple("Nat"),
            level_params: vec![],
            type_: Expr::type_(),
            is_unsafe: false,
        };
        let env2 = env.add_constant(nat.clone()).unwrap();
        let result = env2.add_constant(nat);
        assert!(result.is_err());
    }

    #[test]
    fn test_quot_initialized() {
        let env = Environment::new();
        assert!(!env.is_quot_initialized());
        let env2 = env.set_quot_initialized();
        assert!(env2.is_quot_initialized());
    }
}
