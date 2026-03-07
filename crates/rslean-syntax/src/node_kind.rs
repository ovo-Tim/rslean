use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxNodeKind {
    Module,
    Header,
    ImportDecl,

    Command,
    Declaration,
    DeclModifiers,
    DeclId,
    DeclSig,
    OptDeclSig,
    DeclVal,
    DeclValSimple,
    DeclValEqns,
    WhereStructInst,
    Definition,
    Theorem,
    Abbrev,
    Opaque,
    Instance,
    Example,
    Axiom,
    Structure,
    StructCtor,
    StructField,
    StructFields,
    StructExplicitBinder,
    Extends,
    ClassDecl,
    Inductive,
    Ctor,
    OptDeriving,
    Deriving,
    Namespace,
    Section,
    End,
    Mutual,
    Variable,
    Universe,
    OpenDecl,
    OpenSimple,
    OpenOnly,
    OpenHiding,
    OpenRenaming,
    SetOption,
    Attribute,
    AttributeList,
    AttrInstance,
    Export,
    HashCommand,
    Check,
    Eval,
    Print,
    Reduce,
    Synth,
    ModuleDoc,
    DocComment,
    Init,
    Initialize,

    Private,
    Protected,
    Noncomputable,
    Unsafe,
    Partial,
    Nonrec,

    App,
    Fun,
    FunBinder,
    BasicFun,
    MatchAlts,
    MatchAlt,
    Forall,
    DepArrow,
    Let,
    LetDecl,
    LetIdDecl,
    LetPatDecl,
    LetEqnsDecl,
    LetRec,
    Have,
    HaveDecl,
    HaveIdDecl,
    LetFun,
    Match,
    MatchDiscr,
    NoMatch,
    NoFun,
    If,
    IfThenElse,
    Do,
    DoSeq,
    DoSeqItem,
    DoLet,
    DoLetRec,
    DoLetElse,
    DoReassign,
    DoExpr,
    DoReturn,
    DoFor,
    DoUnless,
    ByTactic,
    TacticSeq,
    Tactic,
    TacticSemicolon,
    Show,
    Suffices,
    TypeAscription,
    Tuple,
    Paren,
    AnonymousCtor,
    StructInst,
    StructInstField,
    StructInstFieldAbbrev,
    Explicit,
    Inaccessible,
    NamedArgument,
    Ellipsis,
    Argument,
    CDot,
    Hole,
    Sorry,
    Absurd,
    Ident,
    Num,
    Scientific,
    Str,
    Char,
    NameLit,
    Sort,
    Type,
    Prop,
    Arrow,
    BinOp,
    UnaryOp,
    PrefixOp,
    Projection,
    DotNotation,
    WhereDecls,
    FunImplicitBinder,
    FunStrictImplicitBinder,
    FunInstBinder,
    ExplicitBinder,
    ImplicitBinder,
    StrictImplicitBinder,
    InstBinder,
    TerminationBy,
    DecreasingBy,

    LevelExpr,

    Token,
    Missing,
    Null,
    Group,
    Sep,
    Semicolon,
}

impl fmt::Display for SyntaxNodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let debug_name = format!("{self:?}");
        let mut chars = debug_name.chars();
        if let Some(first) = chars.next() {
            write!(f, "{}{}", first.to_ascii_lowercase(), chars.as_str())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SyntaxNodeKind;

    #[test]
    fn display_is_lower_camel_case() {
        assert_eq!(SyntaxNodeKind::Module.to_string(), "module");
        assert_eq!(SyntaxNodeKind::DeclId.to_string(), "declId");
        assert_eq!(SyntaxNodeKind::IfThenElse.to_string(), "ifThenElse");
        assert_eq!(SyntaxNodeKind::CDot.to_string(), "cDot");
        assert_eq!(SyntaxNodeKind::LevelExpr.to_string(), "levelExpr");
    }

    #[test]
    fn enum_variants_compare_and_hash() {
        assert_eq!(SyntaxNodeKind::Let, SyntaxNodeKind::Let);
        assert_ne!(SyntaxNodeKind::Let, SyntaxNodeKind::Have);
    }
}
