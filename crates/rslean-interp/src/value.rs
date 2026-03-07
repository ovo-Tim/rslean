use num_bigint::{BigInt, BigUint};
use rslean_expr::Expr;
use rslean_kernel::Environment;
use rslean_level::Level;
use rslean_name::Name;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::sync::Arc;

/// HashMap bucket storage type used by `Value::HashMap`.
pub type HashMapBuckets = FxHashMap<u64, Vec<(Value, Value)>>;

use crate::env::LocalEnv;

/// A runtime value produced by the interpreter.
#[derive(Clone, Debug)]
pub enum Value {
    /// Natural number (arbitrary precision).
    Nat(Arc<BigUint>),
    /// Signed integer (arbitrary precision).
    Int(Arc<BigInt>),
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
    /// Byte array.
    ByteArray(Arc<Vec<u8>>),
    /// Mutable reference (for ST/Ref monad).
    Ref(Arc<RefCell<Value>>),
    /// Hash map (opaque, for Lean.HashMap).
    HashMap(Arc<RefCell<HashMapBuckets>>),
    /// An opaque kernel Environment (for elaborator bridge).
    Environment(Arc<Environment>),
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
    /// A forwarding function: when fully applied, calls `apply(args[func_idx], args[arg_idx])`.
    /// Used for axioms like `BaseIO.toEIO` that forward one argument to another.
    ForwardApply {
        name: Name,
        arity: u32,
        func_idx: u32,
        arg_idx: u32,
    },
    /// Run an IO action synchronously and wrap the result as a Task (Ref).
    /// Used for `BaseIO.asTask`: executes the action, then wraps the ok-value in a Ref.
    RunAction {
        name: Name,
        arity: u32,
        action_idx: u32,
        world_idx: u32,
    },
    /// Chain a Task with a continuation: get Task value, apply function, run resulting IO, wrap as new Task.
    /// Used for `BaseIO.chainTask`: arity 6 [α, β, task, f, prio, world].
    ChainTask,
    /// Loop continuation for `Lean.Loop.forIn`.
    /// When applied to a `ForInStep` value, either returns `pure b` (done)
    /// or recursively calls `bind(f () b)(cont)` (yield).
    LoopContinuation {
        f: Box<Value>,
        bind_fn: Box<Value>,
        pure_fn: Box<Value>,
    },
    /// Initial `Lean.Loop.forIn` invocation (arity 6).
    /// Handled in `apply_fully` with interpreter access.
    LoopForIn,
    /// StateRefT'.get: arity 6 [ω, σ, m, inst, ref, world] → io_ok(ref.get(), world)
    StateRefGet,
    /// StateRefT'.set: arity 7 [ω, σ, m, inst, s, ref, world] → ref.set(s); io_ok(unit, world)
    StateRefSet,
    StateRefModifyGet,
    StateRefRun,
    StateRefLift,
    IgnoreArgThenReturn {
        val: Box<Value>,
    },
    EioToIoPrime,
    /// EIO.toBaseIO: arity 4 [ε, α, act, world] → run act, wrap result in Except
    EioToBaseIO,
    /// IO.FS.withIsolatedStreams: arity 8 [m, α, monad, finally, lift, action, isolateStderr, world]
    WithIsolatedStreams,
    /// EIO.catchExceptions: arity 4 [α, act, defVal, world] → run act, on error return defVal
    EioCatchExceptions,
    /// Array.foldlM.loop: arity 11 [m, β, α, Monad, f, arr, stop, H, i, j, b]
    ArrayFoldlMLoop,
    /// Continuation for Array.foldlM.loop iteration
    ArrayFoldlMCont {
        f: Box<Value>,
        arr: Arc<Vec<Value>>,
        stop: u64,
        current_idx: u64,
        bind_fn: Box<Value>,
        pure_fn: Box<Value>,
    },
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

    /// Try to interpret this value as a signed integer.
    pub fn as_int(&self) -> Option<&BigInt> {
        match self {
            Value::Int(n) => Some(n),
            // Nat can be viewed as non-negative Int
            Value::Nat(_) => None,
            _ => None,
        }
    }

    /// Convert to BigInt, handling both Nat and Int representations.
    pub fn to_bigint(&self) -> Option<BigInt> {
        match self {
            Value::Int(n) => Some(n.as_ref().clone()),
            Value::Nat(n) => Some(BigInt::from(n.as_ref().clone())),
            _ => None,
        }
    }

    /// Check if this is a constructor with the given name.
    pub fn is_ctor_named(&self, name: &str) -> bool {
        matches!(self, Value::Ctor { name: n, .. } if n == &Name::from_str_parts(name))
    }

    /// Create an Option.some value.
    pub fn some(val: Value) -> Self {
        Value::Ctor {
            tag: 1,
            name: Name::from_str_parts("Option.some"),
            fields: vec![val],
        }
    }

    /// Create an Option.none value.
    pub fn none() -> Self {
        Value::Ctor {
            tag: 0,
            name: Name::from_str_parts("Option.none"),
            fields: vec![],
        }
    }
}
