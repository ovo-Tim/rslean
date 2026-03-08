use rslean_lexer::{tokenize, Keyword, Token, TokenKind};
use rslean_name::Name;
use rslean_syntax::*;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Errors accumulated during parsing.
#[derive(Clone, Debug)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

/// Result of parsing a source file.
#[derive(Debug)]
pub struct ParseResult {
    pub syntax: Syntax,
    pub errors: Vec<ParseError>,
}

/// Parse a Lean 4 source string into a `Syntax` tree.
pub fn parse(source: &str) -> ParseResult {
    let mut p = Parser::new(source);
    let syntax = p.parse_module();
    ParseResult {
        syntax,
        errors: p.errors,
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Recursive-descent parser for Lean 4 syntax.
pub struct Parser<'a> {
    pub(crate) source: &'a str,
    pub(crate) tokens: Vec<Token>,
    pub(crate) pos: usize,
    pub(crate) errors: Vec<ParseError>,
    /// Stack of expected indentation columns for block-level constructs.
    pub(crate) indent_stack: Vec<u32>,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        let tokens = tokenize(source);
        Self {
            source,
            tokens,
            pos: 0,
            errors: Vec::new(),
            indent_stack: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Token navigation
    // -----------------------------------------------------------------------

    /// Current token (never panics – returns Eof sentinel).
    pub(crate) fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or_else(|| {
            self.tokens
                .last()
                .expect("tokenize always emits at least Eof")
        })
    }

    /// Reference to current `TokenKind`.
    pub(crate) fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    /// Look-ahead by `n` tokens (skipping newlines in between).
    pub(crate) fn peek_nth_non_newline(&self, n: usize) -> &Token {
        let mut seen = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            if !matches!(self.tokens[i].kind, TokenKind::Newline) {
                if seen == n {
                    return &self.tokens[i];
                }
                seen += 1;
            }
            i += 1;
        }
        self.tokens
            .last()
            .expect("tokenize always emits at least Eof")
    }

    /// Consume the current token and advance.
    pub(crate) fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or_else(|| {
            let eof_span = self.eof_span();
            Token::new(TokenKind::Eof, eof_span)
        });
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    /// True when the parser has reached the end of input.
    pub(crate) fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    /// Skip over any `Newline` tokens.
    pub(crate) fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline) {
            self.pos += 1;
        }
    }

    /// Span of the previous token (used for end-of-node spans).
    #[allow(dead_code)]
    pub(crate) fn prev_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            Span::dummy()
        }
    }

    /// Span for an Eof position.
    fn eof_span(&self) -> Span {
        let end = self.source.len() as u32;
        Span::new(end, end)
    }

    // -----------------------------------------------------------------------
    // Matching helpers
    // -----------------------------------------------------------------------

    /// Check if the current token is a specific keyword.
    pub(crate) fn at_keyword(&self, kw: Keyword) -> bool {
        matches!(self.peek_kind(), TokenKind::Keyword(k) if *k == kw)
    }

    /// Check if the current token is an identifier.
    pub(crate) fn at_ident(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(_))
    }

    /// Check if the current token matches a `TokenKind` variant
    /// (for variants without payload, uses `==`; for variants with payload,
    /// uses discriminant comparison).
    pub(crate) fn at_token(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(kind)
    }

    /// Check if the current token is exactly `kind` (using `PartialEq`).
    pub(crate) fn at_exact(&self, kind: &TokenKind) -> bool {
        self.peek_kind() == kind
    }

    /// Consume the current token if it matches `kind` (by discriminant).
    pub(crate) fn eat(&mut self, kind: &TokenKind) -> Option<Token> {
        if self.at_token(kind) {
            Some(self.advance())
        } else {
            None
        }
    }

    /// Consume the current token if it is the given keyword.
    pub(crate) fn eat_keyword(&mut self, kw: Keyword) -> Option<Token> {
        if self.at_keyword(kw) {
            Some(self.advance())
        } else {
            None
        }
    }

    /// Consume the current token if it matches exactly (by `PartialEq`).
    pub(crate) fn eat_exact(&mut self, kind: &TokenKind) -> Option<Token> {
        if self.at_exact(kind) {
            Some(self.advance())
        } else {
            None
        }
    }

    /// Consume the current token, or emit an error and return a synthetic
    /// `Missing` node. This is the main error-recovery primitive.
    pub(crate) fn expect(&mut self, kind: &TokenKind) -> Token {
        if let Some(tok) = self.eat(kind) {
            tok
        } else {
            self.error(format!("expected {:?}, found {:?}", kind, self.peek_kind()));
            // Return a synthetic token at the current position.
            Token::new(kind.clone(), self.peek().span)
        }
    }

    /// Expect a specific keyword.
    pub(crate) fn expect_keyword(&mut self, kw: Keyword) -> Token {
        if let Some(tok) = self.eat_keyword(kw) {
            tok
        } else {
            self.error(format!("expected `{}`, found {:?}", kw, self.peek_kind()));
            Token::new(TokenKind::Keyword(kw), self.peek().span)
        }
    }

    // -----------------------------------------------------------------------
    // Syntax construction
    // -----------------------------------------------------------------------

    /// Create a `SourceInfo` for a given span (leading/trailing left empty).
    pub(crate) fn mk_info(&self, span: Span) -> SourceInfo {
        SourceInfo::new(span)
    }

    /// Create a `Syntax::Missing` at the current position.
    pub(crate) fn mk_missing(&self) -> Syntax {
        Syntax::missing(self.peek().span)
    }

    /// Build a `Syntax::Atom` from a consumed token.
    pub(crate) fn mk_atom(&self, tok: &Token, val: AtomVal) -> Syntax {
        Syntax::atom(self.mk_info(tok.span), val)
    }

    /// Build a keyword atom from a keyword token.
    pub(crate) fn mk_keyword_atom(&self, tok: &Token) -> Syntax {
        let text = tok.span.text(self.source).to_string();
        self.mk_atom(tok, AtomVal::Keyword(text))
    }

    /// Build a punctuation atom.
    pub(crate) fn mk_punct(&self, tok: &Token) -> Syntax {
        let text = tok.span.text(self.source).to_string();
        self.mk_atom(tok, AtomVal::Punct(text))
    }

    /// Build a `Syntax::Ident` from an identifier token.
    pub(crate) fn mk_ident_from_token(&self, tok: &Token) -> Syntax {
        let raw = tok.span.text(self.source).to_string();
        let name = name_from_str(&raw);
        Syntax::ident(self.mk_info(tok.span), name, raw)
    }

    /// Build a node wrapping children with a computed span.
    pub(crate) fn mk_node(&self, kind: SyntaxNodeKind, children: Vec<Syntax>) -> Syntax {
        let span = node_span(&children);
        Syntax::node(self.mk_info(span), kind, children)
    }

    /// Build a node with an explicit span.
    #[allow(dead_code)]
    pub(crate) fn mk_node_span(
        &self,
        span: Span,
        kind: SyntaxNodeKind,
        children: Vec<Syntax>,
    ) -> Syntax {
        Syntax::node(self.mk_info(span), kind, children)
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    /// Record an error at the current token position.
    pub(crate) fn error(&mut self, msg: impl Into<String>) {
        let span = self.peek().span;
        self.errors.push(ParseError {
            span,
            message: msg.into(),
        });
    }

    /// Record an error at a specific span.
    #[allow(dead_code)]
    pub(crate) fn error_at(&mut self, span: Span, msg: impl Into<String>) {
        self.errors.push(ParseError {
            span,
            message: msg.into(),
        });
    }

    /// Synchronize: skip tokens until we reach a command-level keyword or EOF.
    /// Used for error recovery after a parse failure.
    #[allow(dead_code)]
    pub(crate) fn synchronize(&mut self) {
        loop {
            match self.peek_kind() {
                TokenKind::Eof => break,
                TokenKind::Keyword(kw) if is_command_keyword(kw) => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Indentation
    // -----------------------------------------------------------------------

    /// Compute the column (0-based) of a byte position in the source.
    pub(crate) fn column_of(&self, byte_pos: u32) -> u32 {
        let pos = byte_pos as usize;
        let start = self.source[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        (pos - start) as u32
    }

    /// Column of the current token.
    pub(crate) fn current_column(&self) -> u32 {
        self.column_of(self.peek().span.start)
    }

    /// Push the current column onto the indent stack.
    pub(crate) fn push_indent(&mut self) {
        self.indent_stack.push(self.current_column());
    }

    /// Pop the indent stack.
    pub(crate) fn pop_indent(&mut self) {
        self.indent_stack.pop();
    }

    /// Check if the current column is indented deeper than the top of the
    /// indent stack. Returns `true` if the stack is empty (top-level).
    #[allow(dead_code)]
    pub(crate) fn is_indented(&self) -> bool {
        match self.indent_stack.last() {
            None => true,
            Some(&col) => self.current_column() > col,
        }
    }

    // -----------------------------------------------------------------------
    // Module / header
    // -----------------------------------------------------------------------

    /// Parse a full Lean module:
    ///   `header command*`
    pub(crate) fn parse_module(&mut self) -> Syntax {
        self.skip_newlines();
        let header = self.parse_header();
        let mut children = vec![header];
        self.skip_newlines();
        while !self.is_at_end() {
            let saved = self.pos;
            let cmd = self.parse_command();
            children.push(cmd);
            if self.pos == saved {
                self.advance();
            }
            self.skip_newlines();
        }
        self.mk_node(SyntaxNodeKind::Module, children)
    }

    /// Parse the module header (prelude? import*).
    pub(crate) fn parse_header(&mut self) -> Syntax {
        let mut children = Vec::new();

        if self.at_keyword(Keyword::Module) {
            let module_kw = self.advance();
            let mut module_children = vec![self.mk_keyword_atom(&module_kw)];
            self.skip_newlines();
            if self.at_ident() {
                let name = self.advance();
                module_children.push(self.mk_ident_from_token(&name));
                self.skip_newlines();
            }
            children.push(self.mk_node(SyntaxNodeKind::Command, module_children));
        }

        // optional `prelude`
        if self.at_keyword(Keyword::Prelude) {
            let tok = self.advance();
            children.push(self.mk_keyword_atom(&tok));
            self.skip_newlines();
        }

        // imports
        while self.at_keyword(Keyword::Import) {
            let imp = self.parse_import();
            children.push(imp);
            self.skip_newlines();
        }

        self.mk_node(SyntaxNodeKind::Header, children)
    }

    /// Parse `import <modulePath>`.
    pub(crate) fn parse_import(&mut self) -> Syntax {
        let kw = self.advance(); // `import`
        let kw_syn = self.mk_keyword_atom(&kw);
        self.skip_newlines();

        let path = if self.at_ident() {
            let tok = self.advance();
            self.mk_ident_from_token(&tok)
        } else {
            self.error("expected module path after `import`");
            self.mk_missing()
        };

        self.mk_node(SyntaxNodeKind::Command, vec![kw_syn, path])
    }

    // -----------------------------------------------------------------------
    // Command dispatch
    // -----------------------------------------------------------------------

    /// Parse a single top-level command.
    pub(crate) fn parse_command(&mut self) -> Syntax {
        self.skip_newlines();

        // Collect doc comments
        let mut doc_comment = None;
        if let TokenKind::DocComment(_) = self.peek_kind() {
            let tok = self.advance();
            if let TokenKind::DocComment(text) = &tok.kind {
                doc_comment = Some(self.mk_atom(&tok, AtomVal::DocComment(text.clone())));
            }
            self.skip_newlines();
        }

        // Collect module-level doc comments
        if let TokenKind::ModuleDoc(_) = self.peek_kind() {
            let tok = self.advance();
            if let TokenKind::ModuleDoc(text) = &tok.kind {
                let syn = self.mk_atom(&tok, AtomVal::DocComment(text.clone()));
                return self.mk_node(SyntaxNodeKind::ModuleDoc, vec![syn]);
            }
        }

        // Collect attributes `@[...]`
        let attrs = self.parse_attributes_opt();

        // Collect declaration modifiers (private/protected/noncomputable/unsafe/partial/nonrec)
        let modifiers = self.parse_decl_modifiers();

        // Main dispatch on keyword
        let cmd = match self.peek_kind() {
            TokenKind::Keyword(Keyword::Def) => self.parse_declaration(SyntaxNodeKind::Definition),
            TokenKind::Keyword(Keyword::Theorem) => self.parse_declaration(SyntaxNodeKind::Theorem),
            TokenKind::Keyword(Keyword::Lemma) => self.parse_declaration(SyntaxNodeKind::Theorem),
            TokenKind::Keyword(Keyword::Abbrev) => self.parse_declaration(SyntaxNodeKind::Abbrev),
            TokenKind::Keyword(Keyword::Opaque) => self.parse_declaration(SyntaxNodeKind::Opaque),
            TokenKind::Keyword(Keyword::Instance) => {
                self.parse_declaration(SyntaxNodeKind::Instance)
            }
            TokenKind::Keyword(Keyword::Example) => self.parse_declaration(SyntaxNodeKind::Example),
            TokenKind::Keyword(Keyword::Axiom) => self.parse_declaration(SyntaxNodeKind::Axiom),
            TokenKind::Keyword(Keyword::Structure) => self.parse_structure(),
            TokenKind::Keyword(Keyword::Class) => self.parse_class(),
            TokenKind::Keyword(Keyword::Inductive) => self.parse_inductive(),

            TokenKind::Keyword(Keyword::Namespace) => self.parse_namespace(),
            TokenKind::Keyword(Keyword::Module) => {
                let kw = self.advance();
                let kw_syn = self.mk_keyword_atom(&kw);
                let mut children = vec![kw_syn];
                self.skip_newlines();
                if self.at_ident() {
                    let name = self.advance();
                    children.push(self.mk_ident_from_token(&name));
                }
                self.mk_node(SyntaxNodeKind::Command, children)
            }
            TokenKind::Keyword(Keyword::Prelude) => {
                let kw = self.advance();
                self.mk_node(SyntaxNodeKind::Command, vec![self.mk_keyword_atom(&kw)])
            }
            TokenKind::Keyword(Keyword::Section) => self.parse_section(),
            TokenKind::Keyword(Keyword::End) => self.parse_end(),
            TokenKind::Keyword(Keyword::Mutual) => self.parse_mutual(),

            TokenKind::Keyword(Keyword::Open) => self.parse_open(),
            TokenKind::Keyword(Keyword::Export) => self.parse_export(),
            TokenKind::Keyword(Keyword::Variable) => self.parse_variable(),
            TokenKind::Keyword(Keyword::Universe) => self.parse_universe_decl(),
            TokenKind::Keyword(Keyword::SetOption) => self.parse_set_option(),
            TokenKind::Keyword(Keyword::Attribute) => self.parse_attribute_cmd(),

            TokenKind::Keyword(Keyword::Initialize) => self.parse_initialize(),
            TokenKind::Keyword(Keyword::BuiltinInitialize) => self.parse_initialize(),

            TokenKind::Keyword(Keyword::Notation) => self.parse_notation(),
            TokenKind::Keyword(Keyword::Prefix) => self.parse_notation(),
            TokenKind::Keyword(Keyword::Infix) => self.parse_notation(),
            TokenKind::Keyword(Keyword::Infixl) => self.parse_notation(),
            TokenKind::Keyword(Keyword::Infixr) => self.parse_notation(),
            TokenKind::Keyword(Keyword::Postfix) => self.parse_notation(),
            TokenKind::Keyword(Keyword::Macro) => self.parse_notation(),
            TokenKind::Keyword(Keyword::Syntax) => self.parse_notation(),
            TokenKind::Keyword(Keyword::Elab) => self.parse_notation(),

            TokenKind::Keyword(Keyword::Check) => self.parse_hash_command(),
            TokenKind::Keyword(Keyword::Eval) => self.parse_hash_command(),
            TokenKind::Keyword(Keyword::Print) => self.parse_hash_command(),
            TokenKind::Keyword(Keyword::Reduce) => self.parse_hash_command(),
            TokenKind::Keyword(Keyword::Synth) => self.parse_hash_command(),
            TokenKind::Keyword(Keyword::Exit) => self.parse_hash_command(),

            _ => {
                if let TokenKind::Ident(s) = self.peek_kind() {
                    if s == "public"
                        && self.peek_nth_non_newline(1).kind == TokenKind::Keyword(Keyword::Section)
                    {
                        let public_tok = self.advance();
                        self.skip_newlines();
                        let section = self.parse_section();
                        return self.mk_node(
                            SyntaxNodeKind::Declaration,
                            vec![
                                self.mk_node(
                                    SyntaxNodeKind::DeclModifiers,
                                    vec![self.mk_node(
                                        SyntaxNodeKind::Command,
                                        vec![self.mk_ident_from_token(&public_tok)],
                                    )],
                                ),
                                section,
                            ],
                        );
                    }
                    return self.parse_ident_command();
                }
                self.error(format!(
                    "unexpected token {:?} at command level",
                    self.peek_kind()
                ));
                let tok = self.advance();
                self.mk_punct(&tok)
            }
        };

        // Wrap with doc comment, attributes, and modifiers if present
        self.wrap_with_decl_info(doc_comment, attrs, modifiers, cmd)
    }

    // -----------------------------------------------------------------------
    // Attributes and modifiers
    // -----------------------------------------------------------------------

    /// Optionally parse `@[attr1, attr2, ...]`.
    pub(crate) fn parse_attributes_opt(&mut self) -> Option<Syntax> {
        // Check for `@[`
        if !self.at_exact(&TokenKind::At) {
            return None;
        }
        // Look ahead: if next non-newline token is `[`, this is an attribute
        let next = self.peek_nth_non_newline(1);
        if next.kind != TokenKind::LBracket {
            return None;
        }

        let at_tok = self.advance();
        let at_syn = self.mk_punct(&at_tok);
        let lb = self.advance();
        let lb_syn = self.mk_punct(&lb);

        let mut children = vec![at_syn, lb_syn];
        self.skip_newlines();

        // Parse comma-separated attributes
        loop {
            if self.at_exact(&TokenKind::RBracket) || self.is_at_end() {
                break;
            }
            // An attribute is an identifier, possibly with arguments
            let attr = self.parse_attr_entry();
            children.push(attr);
            self.skip_newlines();
            if self.eat_exact(&TokenKind::Comma).is_some() {
                let comma = &self.tokens[self.pos - 1];
                children.push(self.mk_punct(comma));
                self.skip_newlines();
            } else {
                break;
            }
        }

        let rb = self.expect(&TokenKind::RBracket);
        children.push(self.mk_punct(&rb));
        self.skip_newlines();

        Some(self.mk_node(SyntaxNodeKind::Attribute, children))
    }

    /// Parse a single attribute entry (e.g. `simp`, `extern "name"`, `inline`).
    pub(crate) fn parse_attr_entry(&mut self) -> Syntax {
        // Identifier or keyword (some attributes are keywords like `simp`, `inline`)
        let mut children = Vec::new();

        // Parse the attribute name - could be an ident or keyword
        let name = if self.at_ident() {
            let tok = self.advance();
            self.mk_ident_from_token(&tok)
        } else if matches!(self.peek_kind(), TokenKind::Keyword(_)) {
            let tok = self.advance();
            self.mk_keyword_atom(&tok)
        } else {
            self.error("expected attribute name");
            return self.mk_missing();
        };
        children.push(name);

        // Optional arguments: string literal, number, or identifiers until `,` or `]`
        loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokenKind::Comma | TokenKind::RBracket | TokenKind::Eof => break,
                TokenKind::StrLit(s) => {
                    let s = s.clone();
                    let tok = self.advance();
                    children.push(self.mk_atom(&tok, AtomVal::StrLit(s)));
                }
                TokenKind::NatLit(n) => {
                    let n = n.clone();
                    let tok = self.advance();
                    children.push(self.mk_atom(&tok, AtomVal::NumLit(n)));
                }
                TokenKind::Ident(_) => {
                    let tok = self.advance();
                    children.push(self.mk_ident_from_token(&tok));
                }
                _ => break,
            }
        }

        self.mk_node(SyntaxNodeKind::Attribute, children)
    }

    /// Parse declaration modifiers: `private`/`protected`/`noncomputable`/
    /// `unsafe`/`partial`/`nonrec`.
    pub(crate) fn parse_decl_modifiers(&mut self) -> Vec<Syntax> {
        let mut mods = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokenKind::Keyword(Keyword::Private) => {
                    let tok = self.advance();
                    mods.push(
                        self.mk_node(SyntaxNodeKind::Private, vec![self.mk_keyword_atom(&tok)]),
                    );
                }
                TokenKind::Keyword(Keyword::Protected) => {
                    let tok = self.advance();
                    mods.push(
                        self.mk_node(SyntaxNodeKind::Protected, vec![self.mk_keyword_atom(&tok)]),
                    );
                }
                TokenKind::Keyword(Keyword::Noncomputable) => {
                    let tok = self.advance();
                    mods.push(self.mk_node(
                        SyntaxNodeKind::Noncomputable,
                        vec![self.mk_keyword_atom(&tok)],
                    ));
                }
                TokenKind::Keyword(Keyword::Unsafe) => {
                    let tok = self.advance();
                    mods.push(
                        self.mk_node(SyntaxNodeKind::Unsafe, vec![self.mk_keyword_atom(&tok)]),
                    );
                }
                TokenKind::Keyword(Keyword::Partial) => {
                    let tok = self.advance();
                    mods.push(
                        self.mk_node(SyntaxNodeKind::Partial, vec![self.mk_keyword_atom(&tok)]),
                    );
                }
                TokenKind::Keyword(Keyword::Nonrec) => {
                    let tok = self.advance();
                    mods.push(
                        self.mk_node(SyntaxNodeKind::Nonrec, vec![self.mk_keyword_atom(&tok)]),
                    );
                }
                _ => break,
            }
            self.skip_newlines();
        }
        mods
    }

    /// Wrap a command node with doc comment, attributes, and modifiers
    /// (if any were parsed).
    fn wrap_with_decl_info(
        &self,
        doc: Option<Syntax>,
        attrs: Option<Syntax>,
        mods: Vec<Syntax>,
        cmd: Syntax,
    ) -> Syntax {
        if doc.is_none() && attrs.is_none() && mods.is_empty() {
            return cmd;
        }
        let mut children = Vec::new();
        if let Some(d) = doc {
            children.push(self.mk_node(SyntaxNodeKind::DocComment, vec![d]));
        }
        if let Some(a) = attrs {
            children.push(a);
        }
        if !mods.is_empty() {
            children.push(self.mk_node(SyntaxNodeKind::DeclModifiers, mods));
        }
        children.push(cmd);
        self.mk_node(SyntaxNodeKind::Declaration, children)
    }

    // -----------------------------------------------------------------------
    // Simple commands
    // -----------------------------------------------------------------------

    /// `universe u v w ...`
    pub(crate) fn parse_universe_decl(&mut self) -> Syntax {
        let kw = self.advance(); // `universe`
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];

        // Parse universe names
        while self.at_ident() {
            let tok = self.advance();
            children.push(self.mk_ident_from_token(&tok));
        }

        self.mk_node(SyntaxNodeKind::Universe, children)
    }

    /// `namespace <name>`
    pub(crate) fn parse_namespace(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let name = self.parse_ident_or_missing("expected namespace name");
        self.mk_node(SyntaxNodeKind::Namespace, vec![kw_syn, name])
    }

    /// `section <name>?`
    pub(crate) fn parse_section(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        if self.at_ident() {
            let tok = self.advance();
            children.push(self.mk_ident_from_token(&tok));
        }
        self.mk_node(SyntaxNodeKind::Section, children)
    }

    /// `end <name>?`
    pub(crate) fn parse_end(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        if self.at_ident() {
            let tok = self.advance();
            children.push(self.mk_ident_from_token(&tok));
        }
        self.mk_node(SyntaxNodeKind::End, children)
    }

    /// `mutual ... end`
    pub(crate) fn parse_mutual(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        while !self.at_keyword(Keyword::End) && !self.is_at_end() {
            let cmd = self.parse_command();
            children.push(cmd);
            self.skip_newlines();
        }

        if self.at_keyword(Keyword::End) {
            let end = self.advance();
            children.push(self.mk_keyword_atom(&end));
        } else {
            self.error("expected `end` to close `mutual`");
            children.push(self.mk_missing());
        }

        self.mk_node(SyntaxNodeKind::Mutual, children)
    }

    /// `open <name> (in <name>)?` or `open <name> hiding ...` etc.
    pub(crate) fn parse_open(&mut self) -> Syntax {
        let kw = self.advance(); // `open`
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];

        // Parse name(s) and clauses until next command
        self.parse_open_body(&mut children);

        // optional `in` clause
        if self.at_keyword(Keyword::In) {
            let in_kw = self.advance();
            children.push(self.mk_keyword_atom(&in_kw));
            self.skip_newlines();
            let cmd = self.parse_command();
            children.push(cmd);
            return self.mk_node(SyntaxNodeKind::OpenSimple, children);
        }

        self.mk_node(SyntaxNodeKind::OpenSimple, children)
    }

    /// Parse the body of an `open` command (names, renaming, hiding).
    fn parse_open_body(&mut self, children: &mut Vec<Syntax>) {
        self.skip_newlines();
        // Collect identifiers
        while self.at_ident() {
            let tok = self.advance();
            children.push(self.mk_ident_from_token(&tok));
            self.skip_newlines();

            // Check for `hiding`, `renaming`, parenthesized list
            if self.at_exact(&TokenKind::LParen) {
                let group = self.parse_paren_names();
                children.push(group);
                self.skip_newlines();
            }
        }
    }

    /// Parse `(name1 name2 ...)` — a parenthesized list of names.
    pub(crate) fn parse_paren_names(&mut self) -> Syntax {
        let lp = self.advance();
        let lp_syn = self.mk_punct(&lp);
        let mut children = vec![lp_syn];
        self.skip_newlines();

        while !self.at_exact(&TokenKind::RParen) && !self.is_at_end() {
            if self.at_ident() {
                let tok = self.advance();
                children.push(self.mk_ident_from_token(&tok));
            } else if matches!(self.peek_kind(), TokenKind::Keyword(_)) {
                // Allow keywords like `hiding`, `renaming` inside parens
                let tok = self.advance();
                children.push(self.mk_keyword_atom(&tok));
            } else {
                self.error("expected name in parenthesized list");
                break;
            }
            self.skip_newlines();
        }

        let rp = self.expect(&TokenKind::RParen);
        children.push(self.mk_punct(&rp));
        self.mk_node(SyntaxNodeKind::Group, children)
    }

    /// `export <name> (<names>)`
    pub(crate) fn parse_export(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.parse_open_body(&mut children);
        self.mk_node(SyntaxNodeKind::Export, children)
    }

    /// `variable <binders>`
    pub(crate) fn parse_variable(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Parse binders
        let binders = self.parse_binders();
        children.extend(binders);

        self.mk_node(SyntaxNodeKind::Variable, children)
    }

    /// `set_option <name> <val>? in <command>?`
    pub(crate) fn parse_set_option(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Option name (dotted identifier)
        let name = self.parse_ident_or_missing("expected option name");
        children.push(name);
        self.skip_newlines();

        // Optional value
        match self.peek_kind() {
            TokenKind::Keyword(Keyword::In) => {}
            TokenKind::Eof | TokenKind::Newline => {}
            _ => {
                // Parse value (could be true/false, number, string, ident)
                let val = self.parse_option_value();
                children.push(val);
                self.skip_newlines();
            }
        }

        // Optional `in` clause
        if self.at_keyword(Keyword::In) {
            let in_kw = self.advance();
            children.push(self.mk_keyword_atom(&in_kw));
            self.skip_newlines();
            let cmd = self.parse_command();
            children.push(cmd);
        }

        self.mk_node(SyntaxNodeKind::SetOption, children)
    }

    /// Parse an option value (identifier, number, string, or `true`/`false`).
    fn parse_option_value(&mut self) -> Syntax {
        match self.peek_kind().clone() {
            TokenKind::Ident(_) => {
                let tok = self.advance();
                self.mk_ident_from_token(&tok)
            }
            TokenKind::NatLit(n) => {
                let tok = self.advance();
                self.mk_atom(&tok, AtomVal::NumLit(n))
            }
            TokenKind::StrLit(s) => {
                let tok = self.advance();
                self.mk_atom(&tok, AtomVal::StrLit(s))
            }
            TokenKind::Keyword(Keyword::True) | TokenKind::Keyword(Keyword::False) => {
                let tok = self.advance();
                self.mk_keyword_atom(&tok)
            }
            _ => {
                self.error("expected option value");
                self.mk_missing()
            }
        }
    }

    /// `attribute [attr1, ...] <name>` command
    pub(crate) fn parse_attribute_cmd(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Parse `[attr, ...]`
        if self.at_exact(&TokenKind::LBracket) {
            let lb = self.advance();
            children.push(self.mk_punct(&lb));
            self.skip_newlines();

            loop {
                if self.at_exact(&TokenKind::RBracket) || self.is_at_end() {
                    break;
                }
                let attr = self.parse_attr_entry();
                children.push(attr);
                self.skip_newlines();
                if self.eat_exact(&TokenKind::Comma).is_some() {
                    let comma = &self.tokens[self.pos - 1];
                    children.push(self.mk_punct(comma));
                    self.skip_newlines();
                } else {
                    break;
                }
            }

            let rb = self.expect(&TokenKind::RBracket);
            children.push(self.mk_punct(&rb));
            self.skip_newlines();
        }

        // Rest of line is names/expressions
        while self.at_ident() {
            let tok = self.advance();
            children.push(self.mk_ident_from_token(&tok));
            self.skip_newlines();
        }

        self.mk_node(SyntaxNodeKind::Attribute, children)
    }

    /// `#check`, `#eval`, `#print`, `#reduce`, `#synth`, `#exit`
    pub(crate) fn parse_hash_command(&mut self) -> Syntax {
        let kw = self.advance();
        let kind = match &kw.kind {
            TokenKind::Keyword(Keyword::Check) => SyntaxNodeKind::Check,
            TokenKind::Keyword(Keyword::Eval) => SyntaxNodeKind::Eval,
            TokenKind::Keyword(Keyword::Print) => SyntaxNodeKind::Print,
            TokenKind::Keyword(Keyword::Reduce) => SyntaxNodeKind::Reduce,
            TokenKind::Keyword(Keyword::Synth) => SyntaxNodeKind::Synth,
            _ => SyntaxNodeKind::HashCommand,
        };
        let kw_syn = self.mk_keyword_atom(&kw);
        self.skip_newlines();

        if kind == SyntaxNodeKind::HashCommand {
            // `#exit` has no argument
            return self.mk_node(kind, vec![kw_syn]);
        }

        // Parse the expression argument
        let expr = self.parse_expr();
        self.mk_node(kind, vec![kw_syn, expr])
    }

    pub(crate) fn parse_ident_command(&mut self) -> Syntax {
        let first = self.advance();
        let mut children = vec![self.mk_ident_from_token(&first)];

        loop {
            match self.peek_kind() {
                TokenKind::Newline | TokenKind::Eof => break,
                _ => {
                    let tok = self.advance();
                    let syn = match tok.kind.clone() {
                        TokenKind::Ident(_) => self.mk_ident_from_token(&tok),
                        TokenKind::Keyword(_) => self.mk_keyword_atom(&tok),
                        TokenKind::NatLit(n) => self.mk_atom(&tok, AtomVal::NumLit(n)),
                        TokenKind::StrLit(s) => self.mk_atom(&tok, AtomVal::StrLit(s)),
                        TokenKind::CharLit(c) => {
                            self.mk_atom(&tok, AtomVal::CharLit(c.to_string()))
                        }
                        TokenKind::ScientificLit(s) => {
                            self.mk_atom(&tok, AtomVal::ScientificLit(s))
                        }
                        _ => self.mk_punct(&tok),
                    };
                    children.push(syn);
                }
            }
        }

        self.mk_node(SyntaxNodeKind::Command, children)
    }

    /// `initialize` / `builtin_initialize` declarations.
    pub(crate) fn parse_initialize(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Optional name
        if self.at_ident() {
            let tok = self.advance();
            children.push(self.mk_ident_from_token(&tok));
            self.skip_newlines();
        }

        // Optional `:` type
        if self.at_exact(&TokenKind::Colon) {
            let colon = self.advance();
            children.push(self.mk_punct(&colon));
            self.skip_newlines();
            let ty = self.parse_expr();
            children.push(ty);
            self.skip_newlines();
        }

        // `:=` or `←` value
        if self.at_exact(&TokenKind::ColonEq) || self.at_exact(&TokenKind::LArrow) {
            let sep = self.advance();
            children.push(self.mk_punct(&sep));
            self.skip_newlines();
            let val = self.parse_expr();
            children.push(val);
        }

        self.mk_node(SyntaxNodeKind::Initialize, children)
    }

    /// Parse notation / macro / syntax / elab — just consume tokens until
    /// the next command-level keyword (full parsing is not needed for
    /// Phase 2 minimal viability).
    pub(crate) fn parse_notation(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Consume everything until next command keyword at col 0 or EOF
        while !self.is_at_end() {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }
            // If we see a command keyword at column 0, stop
            if let TokenKind::Keyword(kw) = self.peek_kind() {
                if is_command_keyword(kw) && self.current_column() == 0 {
                    break;
                }
            }
            // Also stop at doc comments at column 0
            if matches!(
                self.peek_kind(),
                TokenKind::DocComment(_) | TokenKind::ModuleDoc(_)
            ) && self.current_column() == 0
            {
                break;
            }
            let tok = self.advance();
            children.push(self.mk_punct(&tok));
        }

        self.mk_node(SyntaxNodeKind::Command, children)
    }

    // -----------------------------------------------------------------------
    // Shared helpers (used by expr.rs and command.rs)
    // -----------------------------------------------------------------------

    /// Parse an identifier or emit an error and return Missing.
    pub(crate) fn parse_ident_or_missing(&mut self, msg: &str) -> Syntax {
        self.skip_newlines();
        if self.at_ident() {
            let tok = self.advance();
            self.mk_ident_from_token(&tok)
        } else {
            self.error(msg);
            self.mk_missing()
        }
    }

    /// Parse an optional `: type` annotation.
    pub(crate) fn parse_opt_type_annot(&mut self) -> Option<Syntax> {
        self.skip_newlines();
        if self.at_exact(&TokenKind::Colon) {
            let colon = self.advance();
            let colon_syn = self.mk_punct(&colon);
            self.skip_newlines();
            let ty = self.parse_expr();
            Some(self.mk_node(SyntaxNodeKind::TypeAscription, vec![colon_syn, ty]))
        } else {
            None
        }
    }

    /// Parse a declaration id: `<ident>` possibly with `.{u1, u2, ...}`.
    pub(crate) fn parse_decl_id(&mut self) -> Syntax {
        self.skip_newlines();
        let mut children = Vec::new();
        if self.at_ident() {
            let tok = self.advance();
            children.push(self.mk_ident_from_token(&tok));
        } else if matches!(self.peek_kind(), TokenKind::Keyword(_)) {
            let tok = self.advance();
            children.push(self.mk_keyword_atom(&tok));
        } else {
            self.error("expected declaration name");
            children.push(self.mk_missing());
            return self.mk_node(SyntaxNodeKind::DeclId, children);
        }

        loop {
            if !self.at_exact(&TokenKind::Dot) {
                break;
            }
            let dot_pos = self.pos;
            let dot = self.advance();
            if self.at_exact(&TokenKind::LBrace) {
                self.pos = dot_pos;
                break;
            }
            self.skip_newlines();
            if self.at_ident() {
                let name = self.advance();
                children.push(self.mk_punct(&dot));
                children.push(self.mk_ident_from_token(&name));
            } else if matches!(self.peek_kind(), TokenKind::Keyword(_)) {
                let name = self.advance();
                children.push(self.mk_punct(&dot));
                children.push(self.mk_keyword_atom(&name));
            } else {
                self.pos = dot_pos;
                break;
            }
        }

        // Optional universe parameters `.{u, v, ...}`
        if self.at_exact(&TokenKind::Dot) {
            let dot_pos = self.pos;
            let dot = self.advance();
            if self.at_exact(&TokenKind::LBrace) {
                children.push(self.mk_punct(&dot));
                let lb = self.advance();
                children.push(self.mk_punct(&lb));
                self.skip_newlines();

                loop {
                    if self.at_exact(&TokenKind::RBrace) || self.is_at_end() {
                        break;
                    }
                    let uname = self.parse_ident_or_missing("expected universe variable name");
                    children.push(uname);
                    self.skip_newlines();
                    if self.eat_exact(&TokenKind::Comma).is_some() {
                        let comma = &self.tokens[self.pos - 1];
                        children.push(self.mk_punct(comma));
                        self.skip_newlines();
                    } else {
                        break;
                    }
                }

                let rb = self.expect(&TokenKind::RBrace);
                children.push(self.mk_punct(&rb));
            } else {
                self.pos = dot_pos;
            }
        }

        self.mk_node(SyntaxNodeKind::DeclId, children)
    }

    /// Check if the current position looks like the start of a binder.
    pub(crate) fn at_binder_start(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::LParen | TokenKind::LBrace | TokenKind::LBracket | TokenKind::LDblAngle
        )
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Convert a string like `"Lean.Meta.whnf"` into a `Name`.
pub(crate) fn name_from_str(s: &str) -> Name {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 1 {
        Name::mk_simple(s)
    } else {
        let mut name = Name::anonymous();
        for part in parts {
            name = Name::mk_str(name, part);
        }
        name
    }
}

/// Compute a combined span from a list of children.
pub(crate) fn node_span(children: &[Syntax]) -> Span {
    if children.is_empty() {
        return Span::dummy();
    }
    let start = children
        .iter()
        .map(|c| c.span().start)
        .filter(|s| *s != u32::MAX) // skip dummies
        .min()
        .unwrap_or(0);
    let end = children.iter().map(|c| c.span().end).max().unwrap_or(0);
    Span::new(start, end)
}

/// True if `kw` is a keyword that starts a command.
pub(crate) fn is_command_keyword(kw: &Keyword) -> bool {
    matches!(
        kw,
        Keyword::Def
            | Keyword::Theorem
            | Keyword::Lemma
            | Keyword::Abbrev
            | Keyword::Opaque
            | Keyword::Instance
            | Keyword::Example
            | Keyword::Axiom
            | Keyword::Structure
            | Keyword::Class
            | Keyword::Inductive
            | Keyword::Namespace
            | Keyword::Module
            | Keyword::Section
            | Keyword::End
            | Keyword::Mutual
            | Keyword::Open
            | Keyword::Export
            | Keyword::Variable
            | Keyword::Universe
            | Keyword::SetOption
            | Keyword::Attribute
            | Keyword::Import
            | Keyword::Prelude
            | Keyword::Private
            | Keyword::Protected
            | Keyword::Noncomputable
            | Keyword::Unsafe
            | Keyword::Partial
            | Keyword::Nonrec
            | Keyword::Notation
            | Keyword::Prefix
            | Keyword::Infix
            | Keyword::Infixl
            | Keyword::Infixr
            | Keyword::Postfix
            | Keyword::Macro
            | Keyword::Syntax
            | Keyword::Elab
            | Keyword::Initialize
            | Keyword::BuiltinInitialize
            | Keyword::Check
            | Keyword::Eval
            | Keyword::Print
            | Keyword::Reduce
            | Keyword::Synth
            | Keyword::Exit
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let res = parse("");
        assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
        assert!(res.syntax.node_kind_matches(SyntaxNodeKind::Module));
    }

    #[test]
    fn test_parse_prelude_header() {
        let res = parse("prelude");
        assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
        let module = &res.syntax;
        assert!(module.node_kind_matches(SyntaxNodeKind::Module));
        let header = module.child(0).unwrap();
        assert!(header.node_kind_matches(SyntaxNodeKind::Header));
    }

    #[test]
    fn test_parse_imports() {
        let src = "import Init\nimport Lean.Meta\n";
        let res = parse(src);
        assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
        let header = res.syntax.child(0).unwrap();
        assert!(header.node_kind_matches(SyntaxNodeKind::Header));
        // header should have 2 import children
        assert_eq!(header.num_children(), 2, "header children: {:?}", header);
    }

    #[test]
    fn test_parse_universe_decl() {
        let src = "universe u v w\n";
        let res = parse(src);
        assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
        // module > header, universe
        assert_eq!(res.syntax.num_children(), 2);
        let cmd = res.syntax.child(1).unwrap();
        assert!(cmd.node_kind_matches(SyntaxNodeKind::Universe));
        // `universe` keyword + 3 ident children
        assert_eq!(cmd.num_children(), 4);
    }

    #[test]
    fn test_parse_namespace_section_end() {
        let src = "namespace Foo\nsection Bar\nend Bar\nend Foo\n";
        let res = parse(src);
        assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
        // module > header, namespace, section, end, end
        assert_eq!(res.syntax.num_children(), 5);
    }

    #[test]
    fn test_parse_set_option() {
        let src = "set_option maxRecDepth 1000\n";
        let res = parse(src);
        assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
        let cmd = res.syntax.child(1).unwrap();
        assert!(cmd.node_kind_matches(SyntaxNodeKind::SetOption));
    }

    #[test]
    fn test_parse_hash_check() {
        let src = "#check Nat\n";
        let res = parse(src);
        // Will have an error because parse_expr is not yet implemented
        // but the structure should still be created
        let cmd = res.syntax.child(1).unwrap();
        assert!(cmd.node_kind_matches(SyntaxNodeKind::Check));
    }

    #[test]
    fn test_name_from_str() {
        let n = name_from_str("Lean.Meta.whnf");
        assert_eq!(n.to_string(), "Lean.Meta.whnf");

        let n = name_from_str("Nat");
        assert_eq!(n.to_string(), "Nat");
    }

    #[test]
    fn test_node_span() {
        let s1 = Syntax::missing(Span::new(0, 5));
        let s2 = Syntax::missing(Span::new(10, 15));
        let sp = node_span(&[s1, s2]);
        assert_eq!(sp.start, 0);
        assert_eq!(sp.end, 15);
    }

    #[test]
    fn test_parse_attributes() {
        let src = "@[simp, inline] def foo := 1\n";
        let res = parse(src);
        // Should parse without panicking. The def body will need expr parser.
        let cmd = res.syntax.child(1).unwrap();
        assert!(
            cmd.node_kind_matches(SyntaxNodeKind::Declaration)
                || cmd.node_kind_matches(SyntaxNodeKind::Definition)
        );
    }
}
