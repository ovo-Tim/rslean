use rslean_name::Name;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InterpError {
    #[error("unbound variable index {0}")]
    UnboundVar(u64),

    #[error("unknown constant '{0}'")]
    UnknownConstant(Name),

    #[error("stack overflow: eval depth exceeded {0}")]
    StackOverflow(u32),

    #[error("type error: {0}")]
    TypeError(String),

    #[error("arity mismatch: expected {expected} args, got {got}")]
    ArityMismatch { expected: u32, got: u32 },

    #[error("projection index {idx} out of range for {struct_name} with {num_fields} fields")]
    ProjOutOfRange {
        struct_name: Name,
        idx: u64,
        num_fields: usize,
    },

    #[error("recursor error: {0}")]
    RecursorError(String),

    #[error("builtin error: {0}")]
    BuiltinError(String),

    #[error("kernel error: {0}")]
    KernelError(#[from] rslean_kernel::KernelError),
}

pub type InterpResult<T> = Result<T, InterpError>;
