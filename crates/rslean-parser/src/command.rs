use rslean_lexer::{Keyword, TokenKind};
use rslean_syntax::*;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    pub(crate) fn parse_declaration(&mut self, kind: SyntaxNodeKind) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        let decl_id = self.parse_decl_id();
        children.push(decl_id);
        self.skip_newlines();

        // Parse binders
        while self.at_binder_start() {
            children.push(self.parse_single_binder());
            self.skip_newlines();
        }

        // Optional `: type` signature
        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(self.mk_node(SyntaxNodeKind::DeclSig, vec![ty]));
        }
        self.skip_newlines();

        // Value: `:=`, `where`, or `|`
        self.parse_decl_value(&mut children);

        // Optional `where` clause (local definitions)
        self.skip_newlines();
        if self.at_keyword(Keyword::Where) && !matches!(kind, SyntaxNodeKind::Axiom) {
            let where_clause = self.parse_where_clause();
            children.push(where_clause);
        }

        // Optional `termination_by` / `decreasing_by`
        self.parse_termination_hints(&mut children);

        self.mk_node(kind, children)
    }

    fn parse_decl_value(&mut self, children: &mut Vec<Syntax>) {
        self.skip_newlines();
        match self.peek_kind() {
            TokenKind::ColonEq => {
                let assign = self.advance();
                children.push(self.mk_punct(&assign));
                self.skip_newlines();
                let val = self.parse_expr();
                children.push(self.mk_node(SyntaxNodeKind::DeclValSimple, vec![val]));
            }
            TokenKind::Pipe => {
                let alts = self.parse_match_alts();
                children.push(self.mk_node(SyntaxNodeKind::DeclValEqns, vec![alts]));
            }
            TokenKind::Keyword(Keyword::Where) => {
                // `where` can be both a value-position (struct-like def) and post-value local defs
                // Here it appears before `:=`, so it's likely `where` as in struct `where` clause
                // We'll handle this in the caller
            }
            _ => {
                // No value (e.g., axiom, opaque without body)
            }
        }
    }

    fn parse_where_clause(&mut self) -> Syntax {
        let kw = self.advance(); // `where`
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        let base_col = self.current_column();

        loop {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }
            let col = self.current_column();
            if col < base_col {
                break;
            }
            if self.at_command_start() {
                if col > 0 {
                    let saved = self.pos;
                    let cmd = self.parse_command();
                    children.push(cmd);
                    if self.pos == saved {
                        self.advance();
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        self.mk_node(SyntaxNodeKind::WhereDecls, children)
    }

    fn parse_termination_hints(&mut self, children: &mut Vec<Syntax>) {
        self.skip_newlines();
        if self.at_ident() {
            if let TokenKind::Ident(ref s) = self.peek_kind().clone() {
                if s == "termination_by" {
                    let kw = self.advance();
                    let kw_syn = self.mk_ident_from_token(&kw);
                    self.skip_newlines();
                    let expr = self.parse_expr();
                    children.push(self.mk_node(SyntaxNodeKind::TerminationBy, vec![kw_syn, expr]));
                    self.skip_newlines();
                }
                if let TokenKind::Ident(ref s) = self.peek_kind().clone() {
                    if s == "decreasing_by" {
                        let kw = self.advance();
                        let kw_syn = self.mk_ident_from_token(&kw);
                        self.skip_newlines();
                        let tac = self.parse_tactic_seq();
                        children
                            .push(self.mk_node(SyntaxNodeKind::DecreasingBy, vec![kw_syn, tac]));
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Structure
    // -----------------------------------------------------------------------

    pub(crate) fn parse_structure(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Name
        let decl_id = self.parse_decl_id();
        children.push(decl_id);
        self.skip_newlines();

        // Binders
        while self.at_binder_start() {
            children.push(self.parse_single_binder());
            self.skip_newlines();
        }

        // Optional `extends`
        if self.at_keyword(Keyword::Extends) {
            let ext = self.parse_extends();
            children.push(ext);
            self.skip_newlines();
        }

        // Optional `: Type`
        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        }
        self.skip_newlines();

        // `where` clause with fields
        if self.at_keyword(Keyword::Where) {
            let where_kw = self.advance();
            children.push(self.mk_keyword_atom(&where_kw));
            self.skip_newlines();

            // Optional constructor name
            if self.at_ident() {
                let next = self.peek_nth_non_newline(1);
                if matches!(next.kind, TokenKind::ColonColon) {
                    let ctor_name = self.advance();
                    children.push(self.mk_ident_from_token(&ctor_name));
                    let cc = self.advance();
                    children.push(self.mk_punct(&cc));
                    self.skip_newlines();
                }
            }

            let fields = self.parse_struct_fields_decl();
            children.push(fields);
        }

        // Optional `deriving`
        if self.at_keyword(Keyword::Deriving) {
            let deriving = self.parse_deriving();
            children.push(deriving);
        }

        self.mk_node(SyntaxNodeKind::Structure, children)
    }

    fn parse_extends(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Comma-separated parent types
        children.push(self.parse_expr());
        while self.eat_exact(&TokenKind::Comma).is_some() {
            let comma = &self.tokens[self.pos - 1];
            children.push(self.mk_punct(comma));
            self.skip_newlines();
            children.push(self.parse_expr());
            self.skip_newlines();
        }

        self.mk_node(SyntaxNodeKind::Extends, children)
    }

    fn parse_struct_fields_decl(&mut self) -> Syntax {
        let mut fields = Vec::new();
        let base_col = self.current_column();

        loop {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }
            let col = self.current_column();
            if col < base_col {
                break;
            }
            if self.at_keyword(Keyword::Deriving) {
                break;
            }
            if self.at_command_start() && col <= base_col {
                break;
            }

            let saved = self.pos;
            let field = self.parse_struct_field_decl();
            fields.push(field);
            if self.pos == saved {
                self.advance();
            }
        }

        self.mk_node(SyntaxNodeKind::StructFields, fields)
    }

    fn parse_struct_field_decl(&mut self) -> Syntax {
        let mut children = Vec::new();

        // Optional doc comment
        if let TokenKind::DocComment(text) = self.peek_kind().clone() {
            let tok = self.advance();
            children.push(self.mk_atom(&tok, AtomVal::DocComment(text)));
            self.skip_newlines();
        }

        // Optional attributes
        if let Some(attrs) = self.parse_attributes_opt() {
            children.push(attrs);
            self.skip_newlines();
        }

        // Field name(s) followed by `: type`
        while self.at_ident() || self.at_exact(&TokenKind::Underscore) {
            let tok = self.advance();
            let syn = if tok.kind == TokenKind::Underscore {
                self.mk_punct(&tok)
            } else {
                self.mk_ident_from_token(&tok)
            };
            children.push(syn);
            self.skip_newlines();
        }

        // `: type`
        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        }
        self.skip_newlines();

        // Optional `:= default`
        if self.at_exact(&TokenKind::ColonEq) {
            let assign = self.advance();
            children.push(self.mk_punct(&assign));
            self.skip_newlines();
            children.push(self.parse_expr());
        }

        self.mk_node(SyntaxNodeKind::StructField, children)
    }

    // -----------------------------------------------------------------------
    // Class
    // -----------------------------------------------------------------------

    pub(crate) fn parse_class(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // `class inductive` is a variant
        if self.at_keyword(Keyword::Inductive) {
            let ind = self.parse_inductive();
            children.push(ind);
            return self.mk_node(SyntaxNodeKind::ClassDecl, children);
        }

        // Parse like structure
        let decl_id = self.parse_decl_id();
        children.push(decl_id);
        self.skip_newlines();

        while self.at_binder_start() {
            children.push(self.parse_single_binder());
            self.skip_newlines();
        }

        if self.at_keyword(Keyword::Extends) {
            let ext = self.parse_extends();
            children.push(ext);
            self.skip_newlines();
        }

        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        }
        self.skip_newlines();

        if self.at_keyword(Keyword::Where) {
            let where_kw = self.advance();
            children.push(self.mk_keyword_atom(&where_kw));
            self.skip_newlines();

            let fields = self.parse_struct_fields_decl();
            children.push(fields);
        }

        if self.at_keyword(Keyword::Deriving) {
            let deriving = self.parse_deriving();
            children.push(deriving);
        }

        self.mk_node(SyntaxNodeKind::ClassDecl, children)
    }

    // -----------------------------------------------------------------------
    // Inductive
    // -----------------------------------------------------------------------

    pub(crate) fn parse_inductive(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        let decl_id = self.parse_decl_id();
        children.push(decl_id);
        self.skip_newlines();

        // Binders
        while self.at_binder_start() {
            children.push(self.parse_single_binder());
            self.skip_newlines();
        }

        // Optional `: Type`
        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        }
        self.skip_newlines();

        // `where` keyword
        if self.at_keyword(Keyword::Where) {
            let where_kw = self.advance();
            children.push(self.mk_keyword_atom(&where_kw));
            self.skip_newlines();
        }

        // Constructors
        let base_col = self.current_column();
        loop {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }
            let documented_ctor = matches!(self.peek_kind(), TokenKind::DocComment(_))
                && matches!(self.peek_nth_non_newline(1).kind, TokenKind::Pipe);
            if !self.at_exact(&TokenKind::Pipe) && !documented_ctor {
                let col = self.current_column();
                if col <= base_col && self.at_command_start() {
                    break;
                }
                if col < base_col {
                    break;
                }
                if self.at_keyword(Keyword::Deriving) {
                    break;
                }
                // If not at pipe and not deeper, might be end of inductive
                if !self.at_ident() {
                    break;
                }
            }

            let ctor = self.parse_constructor();
            children.push(ctor);
        }

        // Optional `deriving`
        if self.at_keyword(Keyword::Deriving) {
            let deriving = self.parse_deriving();
            children.push(deriving);
        }

        self.mk_node(SyntaxNodeKind::Inductive, children)
    }

    fn parse_constructor(&mut self) -> Syntax {
        let mut children = Vec::new();

        if let TokenKind::DocComment(text) = self.peek_kind().clone() {
            let tok = self.advance();
            children.push(self.mk_atom(&tok, AtomVal::DocComment(text)));
            self.skip_newlines();
        }

        // Optional `|`
        if self.eat_exact(&TokenKind::Pipe).is_some() {
            let pipe = &self.tokens[self.pos - 1];
            children.push(self.mk_punct(pipe));
            self.skip_newlines();
        }

        // Optional doc comment
        if let TokenKind::DocComment(text) = self.peek_kind().clone() {
            let tok = self.advance();
            children.push(self.mk_atom(&tok, AtomVal::DocComment(text)));
            self.skip_newlines();
        }

        // Constructor name
        let name = if self.at_ident() {
            let tok = self.advance();
            self.mk_ident_from_token(&tok)
        } else if matches!(self.peek_kind(), TokenKind::Keyword(_)) {
            let tok = self.advance();
            self.mk_keyword_atom(&tok)
        } else {
            self.error("expected constructor name");
            self.mk_missing()
        };
        children.push(name);
        self.skip_newlines();

        // Binders / parameters
        while self.at_binder_start() {
            children.push(self.parse_single_binder());
            self.skip_newlines();
        }

        // Optional `: type`
        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        }

        self.mk_node(SyntaxNodeKind::Ctor, children)
    }

    fn parse_deriving(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Comma-separated handler names
        if self.at_ident() {
            let tok = self.advance();
            children.push(self.mk_ident_from_token(&tok));
            while self.eat_exact(&TokenKind::Comma).is_some() {
                let comma = &self.tokens[self.pos - 1];
                children.push(self.mk_punct(comma));
                self.skip_newlines();
                if self.at_ident() {
                    let tok = self.advance();
                    children.push(self.mk_ident_from_token(&tok));
                }
                self.skip_newlines();
            }
        }

        self.mk_node(SyntaxNodeKind::Deriving, children)
    }
}
