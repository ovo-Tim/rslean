use crate::Expr;
use rslean_name::Name;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DefinitionSafety {
    Safe,
    Unsafe,
    Partial,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReducibilityHints {
    Opaque,
    Abbreviation,
    Regular(u32),
}

impl ReducibilityHints {
    pub fn is_abbrev(&self) -> bool {
        matches!(self, ReducibilityHints::Abbreviation)
    }

    pub fn is_opaque(&self) -> bool {
        matches!(self, ReducibilityHints::Opaque)
    }

    pub fn get_height(&self) -> u32 {
        match self {
            ReducibilityHints::Regular(h) => *h,
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuotKind {
    Type,
    Mk,
    Lift,
    Ind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecursorRule {
    pub ctor_name: Name,
    pub num_fields: u32,
    pub rhs: Expr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConstantInfo {
    Axiom {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        is_unsafe: bool,
    },
    Definition {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        value: Expr,
        hints: ReducibilityHints,
        safety: DefinitionSafety,
    },
    Theorem {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        value: Expr,
    },
    Opaque {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        value: Expr,
        is_unsafe: bool,
    },
    Quot {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        kind: QuotKind,
    },
    Inductive {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        num_params: u32,
        num_indices: u32,
        all: Vec<Name>,
        ctors: Vec<Name>,
        num_nested: u32,
        is_rec: bool,
        is_unsafe: bool,
        is_reflexive: bool,
    },
    Constructor {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        induct_name: Name,
        ctor_idx: u32,
        num_params: u32,
        num_fields: u32,
        is_unsafe: bool,
    },
    Recursor {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        all: Vec<Name>,
        num_params: u32,
        num_indices: u32,
        num_motives: u32,
        num_minors: u32,
        rules: Vec<RecursorRule>,
        is_k: bool,
        is_unsafe: bool,
    },
}

impl ConstantInfo {
    pub fn name(&self) -> &Name {
        match self {
            ConstantInfo::Axiom { name, .. }
            | ConstantInfo::Definition { name, .. }
            | ConstantInfo::Theorem { name, .. }
            | ConstantInfo::Opaque { name, .. }
            | ConstantInfo::Quot { name, .. }
            | ConstantInfo::Inductive { name, .. }
            | ConstantInfo::Constructor { name, .. }
            | ConstantInfo::Recursor { name, .. } => name,
        }
    }

    pub fn level_params(&self) -> &[Name] {
        match self {
            ConstantInfo::Axiom { level_params, .. }
            | ConstantInfo::Definition { level_params, .. }
            | ConstantInfo::Theorem { level_params, .. }
            | ConstantInfo::Opaque { level_params, .. }
            | ConstantInfo::Quot { level_params, .. }
            | ConstantInfo::Inductive { level_params, .. }
            | ConstantInfo::Constructor { level_params, .. }
            | ConstantInfo::Recursor { level_params, .. } => level_params,
        }
    }

    pub fn type_(&self) -> &Expr {
        match self {
            ConstantInfo::Axiom { type_, .. }
            | ConstantInfo::Definition { type_, .. }
            | ConstantInfo::Theorem { type_, .. }
            | ConstantInfo::Opaque { type_, .. }
            | ConstantInfo::Quot { type_, .. }
            | ConstantInfo::Inductive { type_, .. }
            | ConstantInfo::Constructor { type_, .. }
            | ConstantInfo::Recursor { type_, .. } => type_,
        }
    }

    pub fn value(&self) -> Option<&Expr> {
        match self {
            ConstantInfo::Definition { value, .. }
            | ConstantInfo::Theorem { value, .. }
            | ConstantInfo::Opaque { value, .. } => Some(value),
            _ => None,
        }
    }

    pub fn is_definition(&self) -> bool {
        matches!(self, ConstantInfo::Definition { .. })
    }

    pub fn is_axiom(&self) -> bool {
        matches!(self, ConstantInfo::Axiom { .. })
    }

    pub fn is_theorem(&self) -> bool {
        matches!(self, ConstantInfo::Theorem { .. })
    }

    pub fn is_opaque(&self) -> bool {
        matches!(self, ConstantInfo::Opaque { .. })
    }

    pub fn is_quot(&self) -> bool {
        matches!(self, ConstantInfo::Quot { .. })
    }

    pub fn is_inductive(&self) -> bool {
        matches!(self, ConstantInfo::Inductive { .. })
    }

    pub fn is_constructor(&self) -> bool {
        matches!(self, ConstantInfo::Constructor { .. })
    }

    pub fn is_recursor(&self) -> bool {
        matches!(self, ConstantInfo::Recursor { .. })
    }

    pub fn is_unsafe(&self) -> bool {
        match self {
            ConstantInfo::Axiom { is_unsafe, .. } => *is_unsafe,
            ConstantInfo::Definition { safety, .. } => *safety == DefinitionSafety::Unsafe,
            ConstantInfo::Opaque { is_unsafe, .. } => *is_unsafe,
            ConstantInfo::Inductive { is_unsafe, .. } => *is_unsafe,
            ConstantInfo::Constructor { is_unsafe, .. } => *is_unsafe,
            ConstantInfo::Recursor { is_unsafe, .. } => *is_unsafe,
            ConstantInfo::Theorem { .. } | ConstantInfo::Quot { .. } => false,
        }
    }

    pub fn hints(&self) -> Option<&ReducibilityHints> {
        match self {
            ConstantInfo::Definition { hints, .. } => Some(hints),
            _ => None,
        }
    }
}

impl std::fmt::Display for ConstantInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            ConstantInfo::Axiom { .. } => "axiom",
            ConstantInfo::Definition { .. } => "def",
            ConstantInfo::Theorem { .. } => "theorem",
            ConstantInfo::Opaque { .. } => "opaque",
            ConstantInfo::Quot { .. } => "quot",
            ConstantInfo::Inductive { .. } => "inductive",
            ConstantInfo::Constructor { .. } => "ctor",
            ConstantInfo::Recursor { .. } => "rec",
        };
        write!(f, "{} {} : {}", kind, self.name(), self.type_())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Declaration {
    Axiom {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        is_unsafe: bool,
    },
    Definition {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        value: Expr,
        hints: ReducibilityHints,
        safety: DefinitionSafety,
    },
    Theorem {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        value: Expr,
    },
    Opaque {
        name: Name,
        level_params: Vec<Name>,
        type_: Expr,
        value: Expr,
        is_unsafe: bool,
    },
    Quot,
    InductiveDecl {
        level_params: Vec<Name>,
        num_params: u32,
        types: Vec<InductiveType>,
        is_unsafe: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InductiveType {
    pub name: Name,
    pub type_: Expr,
    pub ctors: Vec<ConstructorDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstructorDecl {
    pub name: Name,
    pub type_: Expr,
}
