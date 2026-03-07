use std::fmt;
use std::sync::Arc;

use num_bigint::BigUint;
use rslean_name::Name;

use crate::{SourceInfo, Span, SyntaxNodeKind};

#[derive(Clone, Debug)]
pub enum Syntax {
    Missing {
        span: Span,
    },
    Atom {
        info: SourceInfo,
        val: AtomVal,
    },
    Ident {
        info: SourceInfo,
        val: Name,
        raw_val: String,
    },
    Node {
        info: SourceInfo,
        kind: SyntaxNodeKind,
        children: Vec<Syntax>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum AtomVal {
    Keyword(String),
    Punct(String),
    NumLit(String),
    StrLit(String),
    CharLit(String),
    ScientificLit(String),
    DocComment(String),
}

impl Syntax {
    pub fn missing(span: Span) -> Self {
        Self::Missing { span }
    }

    pub fn atom(info: SourceInfo, val: AtomVal) -> Self {
        Self::Atom { info, val }
    }

    pub fn ident(info: SourceInfo, val: Name, raw_val: impl Into<String>) -> Self {
        Self::Ident {
            info,
            val,
            raw_val: raw_val.into(),
        }
    }

    pub fn node(info: SourceInfo, kind: SyntaxNodeKind, children: Vec<Syntax>) -> Self {
        Self::Node {
            info,
            kind,
            children,
        }
    }

    pub fn ident_arc(info: SourceInfo, val: Arc<Name>, raw_val: impl Into<String>) -> Self {
        Self::ident(info, (*val).clone(), raw_val)
    }

    pub fn num_lit_biguint(info: SourceInfo, val: BigUint) -> Self {
        Self::atom(info, AtomVal::NumLit(val.to_string()))
    }

    pub fn span(&self) -> Span {
        match self {
            Syntax::Missing { span } => *span,
            Syntax::Atom { info, .. } | Syntax::Ident { info, .. } | Syntax::Node { info, .. } => {
                info.full_span()
            }
        }
    }

    pub fn source_info(&self) -> SourceInfo {
        match self {
            Syntax::Missing { span } => SourceInfo::new(*span),
            Syntax::Atom { info, .. } | Syntax::Ident { info, .. } | Syntax::Node { info, .. } => {
                info.clone()
            }
        }
    }

    pub fn kind(&self) -> Option<SyntaxNodeKind> {
        match self {
            Syntax::Node { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    pub fn is_missing(&self) -> bool {
        matches!(self, Syntax::Missing { .. })
    }

    pub fn is_atom(&self) -> bool {
        matches!(self, Syntax::Atom { .. })
    }

    pub fn is_ident(&self) -> bool {
        matches!(self, Syntax::Ident { .. })
    }

    pub fn is_node(&self) -> bool {
        matches!(self, Syntax::Node { .. })
    }

    pub fn children(&self) -> &[Syntax] {
        match self {
            Syntax::Node { children, .. } => children,
            _ => &[],
        }
    }

    pub fn num_children(&self) -> usize {
        self.children().len()
    }

    pub fn child(&self, idx: usize) -> Option<&Syntax> {
        self.children().get(idx)
    }

    pub fn ident_name(&self) -> Option<&Name> {
        match self {
            Syntax::Ident { val, .. } => Some(val),
            _ => None,
        }
    }

    pub fn atom_val(&self) -> Option<&AtomVal> {
        match self {
            Syntax::Atom { val, .. } => Some(val),
            _ => None,
        }
    }

    pub fn node_kind_matches(&self, kind: SyntaxNodeKind) -> bool {
        matches!(self, Syntax::Node { kind: this_kind, .. } if *this_kind == kind)
    }

    pub fn for_each<F: FnMut(&Syntax)>(&self, f: &mut F) {
        f(self);
        if let Syntax::Node { children, .. } = self {
            for child in children {
                child.for_each(f);
            }
        }
    }

    pub fn find<F: Fn(&Syntax) -> bool>(&self, pred: &F) -> Option<&Syntax> {
        if pred(self) {
            return Some(self);
        }

        if let Syntax::Node { children, .. } = self {
            for child in children {
                if let Some(found) = child.find(pred) {
                    return Some(found);
                }
            }
        }

        None
    }

    pub fn has_missing(&self) -> bool {
        self.find(&Syntax::is_missing).is_some()
    }

    fn write_to(&self, out: &mut String) {
        match self {
            Syntax::Missing { .. } => {}
            Syntax::Atom { val, .. } => out.push_str(&val.to_string()),
            Syntax::Ident { raw_val, .. } => out.push_str(raw_val),
            Syntax::Node { children, .. } => {
                for child in children {
                    let mut chunk = String::new();
                    child.write_to(&mut chunk);
                    if chunk.is_empty() {
                        continue;
                    }
                    if needs_space_between(out, &chunk) {
                        out.push(' ');
                    }
                    out.push_str(&chunk);
                }
            }
        }
    }
}

fn needs_space_between(left: &str, right: &str) -> bool {
    let Some(left_last) = left.chars().last() else {
        return false;
    };
    let Some(right_first) = right.chars().next() else {
        return false;
    };
    is_word_char(left_last) && is_word_char(right_first)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '`'
}

impl fmt::Display for Syntax {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        self.write_to(&mut out);
        write!(f, "{out}")
    }
}

impl fmt::Display for AtomVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AtomVal::Keyword(s)
            | AtomVal::Punct(s)
            | AtomVal::NumLit(s)
            | AtomVal::StrLit(s)
            | AtomVal::CharLit(s)
            | AtomVal::ScientificLit(s)
            | AtomVal::DocComment(s) => write!(f, "{s}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use num_bigint::BigUint;
    use rslean_name::Name;

    use crate::{SourceInfo, Span, SyntaxNodeKind};

    use super::{AtomVal, Syntax};

    fn info(start: u32, end: u32) -> SourceInfo {
        SourceInfo::new(Span::new(start, end))
    }

    #[test]
    fn constructs_missing() {
        let s = Syntax::missing(Span::new(2, 2));
        assert!(s.is_missing());
        assert_eq!(s.span(), Span::new(2, 2));
        assert_eq!(s.num_children(), 0);
    }

    #[test]
    fn constructs_atom() {
        let s = Syntax::atom(info(1, 4), AtomVal::Keyword("def".into()));
        assert!(s.is_atom());
        assert_eq!(s.atom_val(), Some(&AtomVal::Keyword("def".into())));
        assert_eq!(s.span(), Span::new(1, 4));
    }

    #[test]
    fn constructs_ident() {
        let name = Name::from_str_parts("Lean.Meta.whnf");
        let s = Syntax::ident(info(5, 19), name.clone(), "Lean.Meta.whnf");
        assert!(s.is_ident());
        assert_eq!(s.ident_name(), Some(&name));
        assert_eq!(s.to_string(), "Lean.Meta.whnf");
    }

    #[test]
    fn constructs_node() {
        let child = Syntax::atom(info(0, 3), AtomVal::Keyword("def".into()));
        let node = Syntax::node(info(0, 3), SyntaxNodeKind::Command, vec![child]);
        assert!(node.is_node());
        assert_eq!(node.kind(), Some(SyntaxNodeKind::Command));
        assert_eq!(node.num_children(), 1);
    }

    #[test]
    fn source_info_and_full_span_work() {
        let base = SourceInfo::new(Span::new(10, 13))
            .with_leading(Span::new(7, 10))
            .with_trailing(Span::new(13, 15));
        let atom = Syntax::atom(base.clone(), AtomVal::Keyword("theorem".into()));
        assert_eq!(atom.source_info(), base);
        assert_eq!(atom.span(), Span::new(7, 15));
    }

    #[test]
    fn children_and_child_access() {
        let n = Syntax::node(
            info(0, 7),
            SyntaxNodeKind::App,
            vec![
                Syntax::ident(info(0, 1), Name::mk_simple("f"), "f"),
                Syntax::ident(info(2, 3), Name::mk_simple("x"), "x"),
            ],
        );
        assert_eq!(n.children().len(), 2);
        assert!(n.child(0).unwrap().is_ident());
        assert!(n.child(1).unwrap().is_ident());
        assert!(n.child(2).is_none());
    }

    #[test]
    fn node_kind_matching() {
        let n = Syntax::node(info(0, 1), SyntaxNodeKind::Type, vec![]);
        assert!(n.node_kind_matches(SyntaxNodeKind::Type));
        assert!(!n.node_kind_matches(SyntaxNodeKind::Prop));

        let a = Syntax::atom(info(0, 1), AtomVal::Punct("(".into()));
        assert!(!a.node_kind_matches(SyntaxNodeKind::Type));
    }

    #[test]
    fn for_each_preorder_traversal() {
        let tree = Syntax::node(
            info(0, 8),
            SyntaxNodeKind::Command,
            vec![
                Syntax::atom(info(0, 3), AtomVal::Keyword("def".into())),
                Syntax::ident(info(4, 5), Name::mk_simple("f"), "f"),
                Syntax::node(
                    info(6, 8),
                    SyntaxNodeKind::App,
                    vec![Syntax::ident(info(6, 7), Name::mk_simple("x"), "x")],
                ),
            ],
        );

        let mut tags = Vec::new();
        tree.for_each(&mut |s| {
            if s.is_node() {
                tags.push("node");
            } else if s.is_atom() {
                tags.push("atom");
            } else if s.is_ident() {
                tags.push("ident");
            }
        });

        assert_eq!(tags, vec!["node", "atom", "ident", "node", "ident"]);
    }

    #[test]
    fn find_returns_first_match() {
        let tree = Syntax::node(
            info(0, 5),
            SyntaxNodeKind::App,
            vec![
                Syntax::ident(info(0, 1), Name::mk_simple("a"), "a"),
                Syntax::ident(info(2, 3), Name::mk_simple("b"), "b"),
            ],
        );
        let found = tree.find(&|s| s.ident_name().is_some_and(|n| n.to_string() == "b"));
        assert!(found.is_some());
        assert_eq!(found.unwrap().to_string(), "b");
    }

    #[test]
    fn has_missing_detects_missing_anywhere() {
        let tree = Syntax::node(
            info(0, 5),
            SyntaxNodeKind::App,
            vec![
                Syntax::ident(info(0, 1), Name::mk_simple("f"), "f"),
                Syntax::node(
                    info(2, 5),
                    SyntaxNodeKind::Argument,
                    vec![Syntax::missing(Span::new(3, 3))],
                ),
            ],
        );
        assert!(tree.has_missing());
        assert!(!Syntax::ident(info(0, 1), Name::mk_simple("x"), "x").has_missing());
    }

    #[test]
    fn display_atom_and_ident() {
        let atom = Syntax::atom(info(0, 2), AtomVal::Keyword("if".into()));
        let ident = Syntax::ident(info(3, 7), Name::mk_simple("then"), "`then`");
        assert_eq!(atom.to_string(), "if");
        assert_eq!(ident.to_string(), "`then`");
    }

    #[test]
    fn display_node_reconstructs_source_like_text() {
        let node = Syntax::node(
            info(0, 12),
            SyntaxNodeKind::Command,
            vec![
                Syntax::atom(info(0, 3), AtomVal::Keyword("def".into())),
                Syntax::ident(info(4, 5), Name::mk_simple("x"), "x"),
                Syntax::atom(info(6, 7), AtomVal::Punct(":".into())),
                Syntax::ident(info(8, 11), Name::mk_simple("Nat"), "Nat"),
            ],
        );
        assert_eq!(node.to_string(), "def x:Nat");
    }

    #[test]
    fn display_atom_val_variants() {
        assert_eq!(AtomVal::Keyword("def".into()).to_string(), "def");
        assert_eq!(AtomVal::Punct("=>".into()).to_string(), "=>");
        assert_eq!(AtomVal::NumLit("42".into()).to_string(), "42");
        assert_eq!(AtomVal::StrLit("\"hi\"".into()).to_string(), "\"hi\"");
        assert_eq!(AtomVal::CharLit("'x'".into()).to_string(), "'x'");
        assert_eq!(AtomVal::ScientificLit("1.2e3".into()).to_string(), "1.2e3");
        assert_eq!(
            AtomVal::DocComment("/-! docs -/".into()).to_string(),
            "/-! docs -/"
        );
    }

    #[test]
    fn biguint_constructor_and_ident_arc_work() {
        let num = BigUint::parse_bytes(b"12345678901234567890", 10).unwrap();
        let lit = Syntax::num_lit_biguint(info(0, 20), num);
        assert_eq!(lit.to_string(), "12345678901234567890");

        let n = Arc::new(Name::from_str_parts("Lean.Parser"));
        let i = Syntax::ident_arc(info(21, 32), n.clone(), "Lean.Parser");
        assert_eq!(i.ident_name(), Some(n.as_ref()));
    }
}
