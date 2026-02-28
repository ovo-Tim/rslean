use rslean_expr::Expr;
use rslean_name::Name;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum KernelError {
    #[error("unknown constant '{0}'")]
    UnknownConstant(Name),

    #[error("already declared '{0}'")]
    AlreadyDeclared(Name),

    #[error("type expected: {0}")]
    TypeExpected(Expr),

    #[error("function expected: {0}")]
    FunctionExpected(Expr),

    #[error("type mismatch: expected {expected}, got {got}")]
    TypeMismatch { expected: Expr, got: Expr },

    #[error("definitional equality failure: {lhs} != {rhs}")]
    DefEqFailure { lhs: Expr, rhs: Expr },

    #[error("universe level not found: {0}")]
    UndefLevelParam(Name),

    #[error("declaration has metavariables: {0}")]
    HasMVar(Name),

    #[error("declaration has free variables: {0}")]
    HasFVar(Name),

    #[error("declaration has local constants: {0}")]
    HasLocal(Name),

    #[error("kernel panic: {0}")]
    Panic(String),

    #[error("inductive error: {0}")]
    InductiveError(String),

    #[error("quotient not initialized")]
    QuotNotInitialized,

    #[error("incorrect number of universe levels for '{name}': expected {expected}, got {got}")]
    IncorrectNumLevels { name: Name, expected: usize, got: usize },

    #[error("unsafe declaration in safe environment: {0}")]
    UnsafeDefinition(Name),
}

pub type KernelResult<T> = Result<T, KernelError>;
