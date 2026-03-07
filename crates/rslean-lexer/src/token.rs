use rslean_syntax::Span;

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn is_keyword(&self, kw: super::Keyword) -> bool {
        self.kind == TokenKind::Keyword(kw)
    }

    pub fn is_eof(&self) -> bool {
        self.kind == TokenKind::Eof
    }

    pub fn is_ident(&self) -> bool {
        matches!(self.kind, TokenKind::Ident(_))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Ident(String),
    NatLit(String),
    StrLit(String),
    CharLit(char),
    ScientificLit(String),
    Keyword(super::Keyword),

    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    LAngle,
    RAngle,
    LDblAngle,
    RDblAngle,

    Comma,
    Semicolon,
    Colon,
    ColonColon,
    ColonEq,
    Dot,
    DotDot,
    Arrow,
    FatArrow,
    LArrow,
    At,
    Hash,
    Question,
    Bang,
    Underscore,
    Pipe,
    Dollar,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Ampersand,
    Tilde,
    Eq,
    Ne,
    Le,
    Ge,
    Lt,
    Gt,
    AndAnd,
    OrOr,
    Append,
    Cons,

    Bind,
    Seq,
    Map,
    SeqComp,
    SeqLeft,
    SeqRight,
    OrElse,
    PipeRight,
    PipeLeft,

    Forall,
    Exists,
    Lambda,
    Neg,
    Conj,
    Disj,
    Iff,
    Mem,
    Compose,
    Times,
    Cdot,
    Subst,
    SMul,
    Dvd,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    Complement,

    InterpolBegin,
    InterpolEnd,

    Newline,
    Eof,

    DocComment(String),
    ModuleDoc(String),

    Error(String),
}

impl TokenKind {
    pub fn is_operator(&self) -> bool {
        matches!(
            self,
            Self::Plus
                | Self::Minus
                | Self::Star
                | Self::Slash
                | Self::Percent
                | Self::Caret
                | Self::Ampersand
                | Self::Tilde
                | Self::Eq
                | Self::Ne
                | Self::Le
                | Self::Ge
                | Self::Lt
                | Self::Gt
                | Self::AndAnd
                | Self::OrOr
                | Self::Append
                | Self::Cons
                | Self::Bind
                | Self::Seq
                | Self::Map
                | Self::SeqComp
                | Self::SeqLeft
                | Self::SeqRight
                | Self::OrElse
                | Self::PipeRight
                | Self::PipeLeft
                | Self::Arrow
                | Self::FatArrow
                | Self::LArrow
                | Self::Forall
                | Self::Exists
                | Self::Lambda
                | Self::Neg
                | Self::Conj
                | Self::Disj
                | Self::Iff
                | Self::Mem
                | Self::Compose
                | Self::Times
                | Self::Cdot
                | Self::Subst
                | Self::SMul
                | Self::Dvd
                | Self::BitAnd
                | Self::BitOr
                | Self::BitXor
                | Self::ShiftLeft
                | Self::ShiftRight
                | Self::Complement
                | Self::Pipe
                | Self::Dollar
        )
    }

    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            Self::NatLit(_) | Self::StrLit(_) | Self::CharLit(_) | Self::ScientificLit(_)
        )
    }
}
