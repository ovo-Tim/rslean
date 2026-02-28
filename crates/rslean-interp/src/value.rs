use num_bigint::BigUint;
use rslean_expr::Expr;
use rslean_level::Level;
use rslean_name::Name;
use std::sync::Arc;

use crate::env::LocalEnv;

/// A runtime value produced by the interpreter.
#[derive(Clone, Debug)]
pub enum Value {
    /// Natural number (arbitrary precision).
    Nat(Arc<BigUint>),
    /// String value.
    String(Arc<str>),
    /// Constructor application: `Ctor { tag, name, fields }`.
    /// `tag` is the constructor index (0 for first ctor, 1 for second, etc.).
    Ctor {
        tag: u32,
        name: Name,
        fields: Vec<Value>,
    },
    /// A closure: a function waiting for arguments.
    Closure {
        func: FuncRef,
        captured: Vec<Value>,
        remaining_arity: u32,
    },
    /// Array of values.
    Array(Arc<Vec<Value>>),
    /// Erased term (type, proof — computationally irrelevant).
    Erased,
    /// An opaque kernel expression passed through (for elaborator bridge).
    KernelExpr(Expr),
}

/// What a closure refers to.
#[derive(Clone, Debug)]
pub enum FuncRef {
    /// A global definition, with instantiated universe levels.
    Definition(Name, Vec<Level>),
    /// A lambda body captured with its environment.
    Lambda(Expr, LocalEnv),
    /// A native Rust builtin function.
    Builtin(Name),
    /// A constructor-building function: when fully applied, produces a Ctor.
    CtorFn {
        name: Name,
        tag: u32,
        num_params: u32,
        num_fields: u32,
    },
    /// A recursor function: when fully applied, performs iota reduction.
    RecursorFn(Name, Vec<Level>),
}

impl Value {
    pub fn nat_small(n: u64) -> Self {
        Value::Nat(Arc::new(BigUint::from(n)))
    }

    pub fn nat(n: BigUint) -> Self {
        Value::Nat(Arc::new(n))
    }

    pub fn string(s: impl Into<Arc<str>>) -> Self {
        Value::String(s.into())
    }

    pub fn bool_(b: bool) -> Self {
        // Bool.false = tag 0, Bool.true = tag 1
        Value::Ctor {
            tag: if b { 1 } else { 0 },
            name: Name::from_str_parts(if b { "Bool.true" } else { "Bool.false" }),
            fields: vec![],
        }
    }

    pub fn unit() -> Self {
        Value::Ctor {
            tag: 0,
            name: Name::from_str_parts("Unit.unit"),
            fields: vec![],
        }
    }

    /// Try to interpret this value as a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Ctor { tag: 0, .. } => Some(false),
            Value::Ctor { tag: 1, .. } => Some(true),
            _ => None,
        }
    }

    /// Try to interpret this value as a Nat.
    pub fn as_nat(&self) -> Option<&BigUint> {
        match self {
            Value::Nat(n) => Some(n),
            _ => None,
        }
    }

    /// Try to interpret this value as a String.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Check if this is a constructor with the given name.
    pub fn is_ctor_named(&self, name: &str) -> bool {
        matches!(self, Value::Ctor { name: n, .. } if n == &Name::from_str_parts(name))
    }
}
