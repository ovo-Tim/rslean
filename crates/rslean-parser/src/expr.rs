use rslean_lexer::{Keyword, TokenKind};
use rslean_syntax::*;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    pub(crate) fn parse_expr(&mut self) -> Syntax {
        self.parse_expr_max()
    }

    pub(crate) fn parse_expr_max(&mut self) -> Syntax {
        self.skip_newlines();
        let lhs = self.parse_unary();
        self.parse_infix(lhs, 0)
    }

    #[allow(dead_code)]
    pub(crate) fn parse_expr_arg(&mut self) -> Syntax {
        self.skip_newlines();
        self.parse_atom()
    }

    fn parse_unary(&mut self) -> Syntax {
        self.skip_newlines();
        match self.peek_kind() {
            TokenKind::Keyword(Keyword::Fun) => self.parse_fun(),
            TokenKind::Lambda => self.parse_fun(),
            TokenKind::Keyword(Keyword::Forall) | TokenKind::Forall => self.parse_forall(),
            TokenKind::Keyword(Keyword::Let) => self.parse_let(),
            TokenKind::Keyword(Keyword::Have) => self.parse_have(),
            TokenKind::Keyword(Keyword::Match) => self.parse_match(),
            TokenKind::Keyword(Keyword::If) => self.parse_if(),
            TokenKind::Keyword(Keyword::Do) => self.parse_do(),
            TokenKind::Keyword(Keyword::Return) => self.parse_return(),
            TokenKind::Keyword(Keyword::Show) => self.parse_show(),
            TokenKind::Keyword(Keyword::Suffices) => self.parse_suffices(),
            TokenKind::Keyword(Keyword::Assume) => self.parse_assume(),
            TokenKind::Keyword(Keyword::Nomatch) => self.parse_nomatch(),
            TokenKind::Keyword(Keyword::Nofun) => self.parse_nofun(),
            TokenKind::Keyword(Keyword::By) => self.parse_by(),
            TokenKind::Neg => {
                let tok = self.advance();
                let op_syn = self.mk_punct(&tok);
                let rhs = self.parse_unary();
                self.mk_node(SyntaxNodeKind::UnaryOp, vec![op_syn, rhs])
            }
            TokenKind::Bang => {
                let tok = self.advance();
                let op_syn = self.mk_punct(&tok);
                let rhs = self.parse_unary();
                self.mk_node(SyntaxNodeKind::UnaryOp, vec![op_syn, rhs])
            }
            TokenKind::Complement => {
                let tok = self.advance();
                let op_syn = self.mk_punct(&tok);
                let rhs = self.parse_unary();
                self.mk_node(SyntaxNodeKind::UnaryOp, vec![op_syn, rhs])
            }
            _ => self.parse_app(),
        }
    }

    fn parse_app(&mut self) -> Syntax {
        let mut func = self.parse_atom();
        func = self.parse_postfix(func);
        loop {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }
            if self.at_app_arg_start() {
                let arg = self.parse_app_arg();
                func = self.mk_node(SyntaxNodeKind::App, vec![func, arg]);
                func = self.parse_postfix(func);
            } else {
                break;
            }
        }
        func
    }

    fn at_app_arg_start(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Ident(_)
                | TokenKind::NatLit(_)
                | TokenKind::StrLit(_)
                | TokenKind::CharLit(_)
                | TokenKind::ScientificLit(_)
                | TokenKind::LParen
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::LDblAngle
                | TokenKind::At
                | TokenKind::Underscore
                | TokenKind::Keyword(Keyword::Sort)
                | TokenKind::Keyword(Keyword::Type)
                | TokenKind::Keyword(Keyword::Prop)
                | TokenKind::Keyword(Keyword::True)
                | TokenKind::Keyword(Keyword::False)
                | TokenKind::Keyword(Keyword::Sorry)
                | TokenKind::Question
                | TokenKind::Bang
                | TokenKind::InterpolBegin
                | TokenKind::LAngle
        )
    }

    fn parse_app_arg(&mut self) -> Syntax {
        if self.at_exact(&TokenKind::At) {
            let at_tok = self.advance();
            let at_syn = self.mk_punct(&at_tok);
            let atom = self.parse_atom();
            let arg = self.parse_postfix(atom);
            return self.mk_node(SyntaxNodeKind::Explicit, vec![at_syn, arg]);
        }
        let atom = self.parse_atom();
        self.parse_postfix(atom)
    }

    pub(crate) fn parse_atom(&mut self) -> Syntax {
        self.skip_newlines();
        match self.peek_kind().clone() {
            TokenKind::Ident(_s) => {
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
            TokenKind::CharLit(c) => {
                let tok = self.advance();
                self.mk_atom(&tok, AtomVal::CharLit(c.to_string()))
            }
            TokenKind::ScientificLit(s) => {
                let tok = self.advance();
                self.mk_atom(&tok, AtomVal::ScientificLit(s))
            }
            TokenKind::LParen => self.parse_paren_or_tuple(),
            TokenKind::LBracket => self.parse_list_literal(),
            TokenKind::LBrace => self.parse_brace_expr(),
            TokenKind::LDblAngle => self.parse_anonymous_ctor(),
            TokenKind::LAngle => self.parse_angle_ctor(),
            TokenKind::InterpolBegin => self.parse_interpolated_string(),
            TokenKind::Keyword(Keyword::Sort) => {
                let tok = self.advance();
                let kw = self.mk_keyword_atom(&tok);
                self.skip_newlines();
                let lvl = if self.at_ident()
                    || self.at_exact(&TokenKind::LParen)
                    || matches!(self.peek_kind(), TokenKind::NatLit(_))
                {
                    self.parse_level_expr()
                } else {
                    self.mk_missing()
                };
                self.mk_node(SyntaxNodeKind::Sort, vec![kw, lvl])
            }
            TokenKind::Keyword(Keyword::Type) => {
                let tok = self.advance();
                let kw = self.mk_keyword_atom(&tok);
                self.skip_newlines();
                let lvl = if self.at_ident()
                    || self.at_exact(&TokenKind::LParen)
                    || matches!(self.peek_kind(), TokenKind::NatLit(_))
                {
                    self.parse_level_expr()
                } else {
                    self.mk_missing()
                };
                self.mk_node(SyntaxNodeKind::Type, vec![kw, lvl])
            }
            TokenKind::Keyword(Keyword::Prop) => {
                let tok = self.advance();
                self.mk_keyword_atom(&tok)
            }
            TokenKind::Keyword(Keyword::True) => {
                let tok = self.advance();
                self.mk_keyword_atom(&tok)
            }
            TokenKind::Keyword(Keyword::False) => {
                let tok = self.advance();
                self.mk_keyword_atom(&tok)
            }
            TokenKind::Keyword(Keyword::Sorry) => {
                let tok = self.advance();
                let kw = self.mk_keyword_atom(&tok);
                self.mk_node(SyntaxNodeKind::Sorry, vec![kw])
            }
            TokenKind::Underscore => {
                let tok = self.advance();
                let syn = self.mk_punct(&tok);
                self.mk_node(SyntaxNodeKind::Hole, vec![syn])
            }
            TokenKind::Question => {
                let tok = self.advance();
                let syn = self.mk_punct(&tok);
                self.mk_node(SyntaxNodeKind::Hole, vec![syn])
            }
            TokenKind::Keyword(Keyword::Pure) => {
                let tok = self.advance();
                let kw = self.mk_keyword_atom(&tok);
                self.skip_newlines();
                let arg = self.parse_atom();
                self.mk_node(SyntaxNodeKind::App, vec![kw, arg])
            }
            _ => {
                self.error(format!("expected expression, found {:?}", self.peek_kind()));
                self.advance();
                self.mk_missing()
            }
        }
    }

    fn parse_dot_notation(&mut self, lhs: Syntax) -> Syntax {
        let mut result = lhs;
        while self.at_exact(&TokenKind::Dot) {
            let dot = self.advance();
            let dot_syn = self.mk_punct(&dot);
            self.skip_newlines();
            if self.at_ident() {
                let field = self.advance();
                let field_syn = self.mk_ident_from_token(&field);
                result = self.mk_node(SyntaxNodeKind::Projection, vec![result, dot_syn, field_syn]);
            } else if matches!(self.peek_kind(), TokenKind::Keyword(_)) {
                let field = self.advance();
                let field_syn = self.mk_keyword_atom(&field);
                result = self.mk_node(SyntaxNodeKind::Projection, vec![result, dot_syn, field_syn]);
            } else if let TokenKind::NatLit(n) = self.peek_kind().clone() {
                let tok = self.advance();
                let num = self.mk_atom(&tok, AtomVal::NumLit(n));
                result = self.mk_node(SyntaxNodeKind::Projection, vec![result, dot_syn, num]);
            } else {
                result = self.mk_node(
                    SyntaxNodeKind::DotNotation,
                    vec![result, dot_syn, self.mk_missing()],
                );
                break;
            }
        }
        result
    }

    fn parse_postfix(&mut self, mut base: Syntax) -> Syntax {
        loop {
            if self.at_exact(&TokenKind::Dot)
                && matches!(self.peek_nth_non_newline(1).kind, TokenKind::LBrace)
            {
                base = self.parse_universe_inst(base);
                continue;
            }
            if self.at_exact(&TokenKind::Dot) {
                base = self.parse_dot_notation(base);
                continue;
            }
            break;
        }
        base
    }

    fn parse_universe_inst(&mut self, base: Syntax) -> Syntax {
        let dot = self.advance();
        let dot_syn = self.mk_punct(&dot);
        let lb = self.expect(&TokenKind::LBrace);
        let lb_syn = self.mk_punct(&lb);
        let mut children = vec![base, dot_syn, lb_syn];
        self.skip_newlines();

        loop {
            if self.at_exact(&TokenKind::RBrace) || self.is_at_end() {
                break;
            }
            if self.at_ident() {
                let tok = self.advance();
                children.push(self.mk_ident_from_token(&tok));
            } else if matches!(self.peek_kind(), TokenKind::Keyword(_)) {
                let tok = self.advance();
                children.push(self.mk_keyword_atom(&tok));
            } else {
                self.error("expected universe level name");
                break;
            }
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
        self.mk_node(SyntaxNodeKind::DotNotation, children)
    }

    fn parse_paren_or_tuple(&mut self) -> Syntax {
        let lp = self.advance();
        let lp_syn = self.mk_punct(&lp);
        self.skip_newlines();

        if self.at_exact(&TokenKind::RParen) {
            let rp = self.advance();
            let rp_syn = self.mk_punct(&rp);
            return self.mk_node(SyntaxNodeKind::Tuple, vec![lp_syn, rp_syn]);
        }

        let first = self.parse_expr();
        self.skip_newlines();

        if self.at_exact(&TokenKind::Comma) {
            let mut elems = vec![lp_syn, first];
            while self.eat_exact(&TokenKind::Comma).is_some() {
                let comma = &self.tokens[self.pos - 1];
                elems.push(self.mk_punct(comma));
                self.skip_newlines();
                if self.at_exact(&TokenKind::RParen) {
                    break;
                }
                elems.push(self.parse_expr());
                self.skip_newlines();
            }
            let rp = self.expect(&TokenKind::RParen);
            elems.push(self.mk_punct(&rp));
            self.mk_node(SyntaxNodeKind::Tuple, elems)
        } else {
            let rp = self.expect(&TokenKind::RParen);
            let rp_syn = self.mk_punct(&rp);
            self.mk_node(SyntaxNodeKind::Paren, vec![lp_syn, first, rp_syn])
        }
    }

    fn parse_list_literal(&mut self) -> Syntax {
        let lb = self.advance();
        let lb_syn = self.mk_punct(&lb);
        let mut children = vec![lb_syn];
        self.skip_newlines();

        if !self.at_exact(&TokenKind::RBracket) {
            children.push(self.parse_expr());
            self.skip_newlines();
            while self.eat_exact(&TokenKind::Comma).is_some() {
                let comma = &self.tokens[self.pos - 1];
                children.push(self.mk_punct(comma));
                self.skip_newlines();
                if self.at_exact(&TokenKind::RBracket) {
                    break;
                }
                children.push(self.parse_expr());
                self.skip_newlines();
            }
        }

        let rb = self.expect(&TokenKind::RBracket);
        children.push(self.mk_punct(&rb));
        self.mk_node(SyntaxNodeKind::Group, children)
    }

    fn parse_brace_expr(&mut self) -> Syntax {
        let lb = self.advance();
        let lb_syn = self.mk_punct(&lb);
        self.skip_newlines();

        // Could be: `{ x : T // P }` (subtype), `{ x }` (struct instance),
        // or `{ x : T }` (implicit binder used as term)
        // For now, parse as struct instance or subtype
        let first = self.parse_expr();
        self.skip_newlines();

        if self.at_keyword(Keyword::With) {
            // Struct instance: `{ first with field := val, ... }`
            let with_tok = self.advance();
            let with_syn = self.mk_keyword_atom(&with_tok);
            let mut children = vec![lb_syn, first, with_syn];
            self.skip_newlines();
            self.parse_struct_fields(&mut children);
            let rb = self.expect(&TokenKind::RBrace);
            children.push(self.mk_punct(&rb));
            return self.mk_node(SyntaxNodeKind::StructInst, children);
        }

        if self.at_exact(&TokenKind::ColonEq) {
            // Struct instance field: `{ field := val, ... }`
            let mut children = vec![lb_syn];
            let assign = self.advance();
            let assign_syn = self.mk_punct(&assign);
            self.skip_newlines();
            let val = self.parse_expr();
            let field_node = self.mk_node(
                SyntaxNodeKind::StructInstField,
                vec![first, assign_syn, val],
            );
            children.push(field_node);
            self.skip_newlines();
            self.parse_struct_fields(&mut children);
            let rb = self.expect(&TokenKind::RBrace);
            children.push(self.mk_punct(&rb));
            return self.mk_node(SyntaxNodeKind::StructInst, children);
        }

        let rb = self.expect(&TokenKind::RBrace);
        let rb_syn = self.mk_punct(&rb);
        self.mk_node(SyntaxNodeKind::StructInst, vec![lb_syn, first, rb_syn])
    }

    fn parse_struct_fields(&mut self, children: &mut Vec<Syntax>) {
        loop {
            self.skip_newlines();
            if self.at_exact(&TokenKind::RBrace) || self.is_at_end() {
                break;
            }
            if self.eat_exact(&TokenKind::Comma).is_some() {
                let comma = &self.tokens[self.pos - 1];
                children.push(self.mk_punct(comma));
                self.skip_newlines();
            }
            if self.at_exact(&TokenKind::RBrace) || self.is_at_end() {
                break;
            }
            if self.at_ident() {
                let name_tok = self.advance();
                let name_syn = self.mk_ident_from_token(&name_tok);
                self.skip_newlines();
                if self.at_exact(&TokenKind::ColonEq) {
                    let assign = self.advance();
                    let assign_syn = self.mk_punct(&assign);
                    self.skip_newlines();
                    let val = self.parse_expr();
                    children.push(self.mk_node(
                        SyntaxNodeKind::StructInstField,
                        vec![name_syn, assign_syn, val],
                    ));
                } else {
                    children
                        .push(self.mk_node(SyntaxNodeKind::StructInstFieldAbbrev, vec![name_syn]));
                }
            } else {
                self.error("expected field name in struct instance");
                break;
            }
        }
    }

    fn parse_anonymous_ctor(&mut self) -> Syntax {
        let la = self.advance(); // ⟨
        let la_syn = self.mk_punct(&la);
        let mut children = vec![la_syn];
        self.skip_newlines();

        if !self.at_exact(&TokenKind::RDblAngle) {
            children.push(self.parse_expr());
            self.skip_newlines();
            while self.eat_exact(&TokenKind::Comma).is_some() {
                let comma = &self.tokens[self.pos - 1];
                children.push(self.mk_punct(comma));
                self.skip_newlines();
                if self.at_exact(&TokenKind::RDblAngle) {
                    break;
                }
                children.push(self.parse_expr());
                self.skip_newlines();
            }
        }

        let ra = self.expect(&TokenKind::RDblAngle);
        children.push(self.mk_punct(&ra));
        self.mk_node(SyntaxNodeKind::AnonymousCtor, children)
    }

    fn parse_angle_ctor(&mut self) -> Syntax {
        let la = self.advance();
        let la_syn = self.mk_punct(&la);
        let mut children = vec![la_syn];
        self.skip_newlines();

        if !self.at_exact(&TokenKind::RAngle) {
            children.push(self.parse_expr());
            self.skip_newlines();
            while self.eat_exact(&TokenKind::Comma).is_some() {
                let comma = &self.tokens[self.pos - 1];
                children.push(self.mk_punct(comma));
                self.skip_newlines();
                if self.at_exact(&TokenKind::RAngle) {
                    break;
                }
                children.push(self.parse_expr());
                self.skip_newlines();
            }
        }

        let ra = self.expect(&TokenKind::RAngle);
        children.push(self.mk_punct(&ra));
        self.mk_node(SyntaxNodeKind::AnonymousCtor, children)
    }

    fn parse_interpolated_string(&mut self) -> Syntax {
        let begin = self.advance();
        let begin_syn = self.mk_punct(&begin);
        let mut children = vec![begin_syn];

        loop {
            match self.peek_kind() {
                TokenKind::InterpolEnd | TokenKind::Eof => break,
                TokenKind::StrLit(s) => {
                    let s = s.clone();
                    let tok = self.advance();
                    children.push(self.mk_atom(&tok, AtomVal::StrLit(s)));
                }
                _ => {
                    children.push(self.parse_expr());
                    self.skip_newlines();
                }
            }
        }

        if let Some(end) = self.eat(&TokenKind::InterpolEnd) {
            children.push(self.mk_punct(&end));
        }
        self.mk_node(SyntaxNodeKind::Str, children)
    }

    fn parse_level_expr(&mut self) -> Syntax {
        self.skip_newlines();
        let base = match self.peek_kind().clone() {
            TokenKind::Ident(_) => {
                let tok = self.advance();
                self.mk_ident_from_token(&tok)
            }
            TokenKind::NatLit(n) => {
                let tok = self.advance();
                self.mk_atom(&tok, AtomVal::NumLit(n))
            }
            TokenKind::LParen => {
                let lp = self.advance();
                let lp_syn = self.mk_punct(&lp);
                let inner = self.parse_level_expr();
                let rp = self.expect(&TokenKind::RParen);
                let rp_syn = self.mk_punct(&rp);
                self.mk_node(SyntaxNodeKind::LevelExpr, vec![lp_syn, inner, rp_syn])
            }
            _ => {
                self.error("expected level expression");
                return self.mk_missing();
            }
        };
        // Handle `+` for level addition
        if self.at_exact(&TokenKind::Plus) {
            let plus = self.advance();
            let plus_syn = self.mk_punct(&plus);
            let rhs = self.parse_level_expr();
            self.mk_node(SyntaxNodeKind::LevelExpr, vec![base, plus_syn, rhs])
        } else {
            base
        }
    }

    // -----------------------------------------------------------------------
    // Fun / Lambda
    // -----------------------------------------------------------------------

    fn parse_fun(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Parse binders
        let binders = self.parse_fun_binders();
        children.extend(binders);
        self.skip_newlines();

        // Expect `=>`
        let arrow = self.expect(&TokenKind::FatArrow);
        children.push(self.mk_punct(&arrow));
        self.skip_newlines();

        let body = self.parse_expr();
        children.push(body);

        self.mk_node(SyntaxNodeKind::Fun, children)
    }

    fn parse_fun_binders(&mut self) -> Vec<Syntax> {
        let mut binders = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokenKind::FatArrow | TokenKind::Eof => break,
                TokenKind::Ident(_) | TokenKind::Underscore => {
                    let tok = self.advance();
                    let syn = if tok.kind == TokenKind::Underscore {
                        self.mk_punct(&tok)
                    } else {
                        self.mk_ident_from_token(&tok)
                    };
                    // Check for `: type`
                    if self.at_exact(&TokenKind::Colon) {
                        let colon = self.advance();
                        let colon_syn = self.mk_punct(&colon);
                        self.skip_newlines();
                        let ty = self.parse_expr();
                        binders.push(
                            self.mk_node(SyntaxNodeKind::FunBinder, vec![syn, colon_syn, ty]),
                        );
                    } else {
                        binders.push(syn);
                    }
                }
                TokenKind::LParen => {
                    let binder = self.parse_explicit_binder();
                    binders.push(binder);
                }
                TokenKind::LBrace => {
                    let binder = self.parse_implicit_binder();
                    binders.push(binder);
                }
                TokenKind::LBracket => {
                    let binder = self.parse_inst_binder();
                    binders.push(binder);
                }
                TokenKind::LDblAngle => {
                    let binder = self.parse_strict_implicit_binder();
                    binders.push(binder);
                }
                // pattern match: `| pat => body`
                TokenKind::Pipe => break,
                _ => break,
            }
        }
        // Handle `fun | pat1 => e1 | pat2 => e2` (fun with match arms)
        if self.at_exact(&TokenKind::Pipe) {
            binders.push(self.parse_match_alts());
        }
        binders
    }

    // -----------------------------------------------------------------------
    // Forall
    // -----------------------------------------------------------------------

    fn parse_forall(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        let binders = self.parse_binders();
        children.extend(binders);
        self.skip_newlines();

        let comma = self.expect(&TokenKind::Comma);
        children.push(self.mk_punct(&comma));
        self.skip_newlines();

        let body = self.parse_expr();
        children.push(body);

        self.mk_node(SyntaxNodeKind::Forall, children)
    }

    // -----------------------------------------------------------------------
    // Let / Have / Assume / Show / Suffices
    // -----------------------------------------------------------------------

    fn parse_let(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        self.skip_newlines();

        // `let rec`
        if self.at_keyword(Keyword::Mutual)
            || (self.at_ident() && self.peek_kind() == &TokenKind::Ident("rec".to_string()))
        {
            return self.parse_let_rec(kw_syn);
        }

        let mut children = vec![kw_syn];

        // Let binding: `let x : T := e; body` or `let x := e; body`
        let name = self.parse_ident_or_missing("expected name after `let`");
        let mut decl_children = vec![name];
        self.skip_newlines();

        // Optional binders
        while self.at_binder_start() || self.at_ident() || self.at_exact(&TokenKind::Underscore) {
            if self.at_binder_start() {
                decl_children.push(self.parse_single_binder());
            } else {
                let tok = self.advance();
                let syn = if tok.kind == TokenKind::Underscore {
                    self.mk_punct(&tok)
                } else {
                    self.mk_ident_from_token(&tok)
                };
                decl_children.push(syn);
            }
            self.skip_newlines();
        }

        // Optional `: type`
        if let Some(ty) = self.parse_opt_type_annot() {
            decl_children.push(ty);
        }
        self.skip_newlines();

        // `:=`
        let assign = self.expect(&TokenKind::ColonEq);
        decl_children.push(self.mk_punct(&assign));
        self.skip_newlines();

        let val = self.parse_expr();
        decl_children.push(val);

        children.push(self.mk_node(SyntaxNodeKind::LetIdDecl, decl_children));
        self.skip_newlines();

        // Newline or semicolon separator, then body
        self.eat_exact(&TokenKind::Semicolon);
        self.skip_newlines();

        if !self.is_at_end() && !self.at_command_start() {
            let body = self.parse_expr();
            children.push(body);
        }

        self.mk_node(SyntaxNodeKind::Let, children)
    }

    fn parse_let_rec(&mut self, let_kw: Syntax) -> Syntax {
        let rec_tok = self.advance(); // `rec` or `mutual`
        let rec_syn = self.mk_keyword_atom(&rec_tok);
        let mut children = vec![let_kw, rec_syn];
        self.skip_newlines();

        // Parse one or more let-rec bindings
        let binding = self.parse_let_rec_binding();
        children.push(binding);
        self.skip_newlines();

        self.eat_exact(&TokenKind::Semicolon);
        self.skip_newlines();

        if !self.is_at_end() && !self.at_command_start() {
            let body = self.parse_expr();
            children.push(body);
        }

        self.mk_node(SyntaxNodeKind::LetRec, children)
    }

    fn parse_let_rec_binding(&mut self) -> Syntax {
        let name = self.parse_ident_or_missing("expected name in let rec binding");
        let mut children = vec![name];
        self.skip_newlines();

        while self.at_binder_start() {
            children.push(self.parse_single_binder());
            self.skip_newlines();
        }

        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        }
        self.skip_newlines();

        let assign = self.expect(&TokenKind::ColonEq);
        children.push(self.mk_punct(&assign));
        self.skip_newlines();

        let val = self.parse_expr();
        children.push(val);

        self.mk_node(SyntaxNodeKind::LetIdDecl, children)
    }

    fn parse_have(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Optional name
        let mut decl_children = Vec::new();
        if self.at_ident() {
            let next = self.peek_nth_non_newline(1);
            if matches!(next.kind, TokenKind::Colon | TokenKind::ColonEq) {
                let name = self.advance();
                decl_children.push(self.mk_ident_from_token(&name));
                self.skip_newlines();
            }
        }

        // `: type`
        if let Some(ty) = self.parse_opt_type_annot() {
            decl_children.push(ty);
        }
        self.skip_newlines();

        // `:=` or `from` value
        if self.at_exact(&TokenKind::ColonEq) {
            let assign = self.advance();
            decl_children.push(self.mk_punct(&assign));
            self.skip_newlines();
            let val = self.parse_expr();
            decl_children.push(val);
        } else if self.at_keyword(Keyword::By) {
            let by_expr = self.parse_by();
            decl_children.push(by_expr);
        }

        children.push(self.mk_node(SyntaxNodeKind::HaveIdDecl, decl_children));
        self.skip_newlines();

        self.eat_exact(&TokenKind::Semicolon);
        self.skip_newlines();

        if !self.is_at_end() && !self.at_command_start() {
            let body = self.parse_expr();
            children.push(body);
        }

        self.mk_node(SyntaxNodeKind::Have, children)
    }

    fn parse_assume(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        self.skip_newlines();

        let name = self.parse_ident_or_missing("expected name after `assume`");
        self.skip_newlines();

        let ty = if let Some(ty) = self.parse_opt_type_annot() {
            ty
        } else {
            self.mk_missing()
        };
        self.skip_newlines();

        self.eat_exact(&TokenKind::Semicolon);
        self.skip_newlines();

        let body = if !self.is_at_end() && !self.at_command_start() {
            self.parse_expr()
        } else {
            self.mk_missing()
        };

        self.mk_node(SyntaxNodeKind::Have, vec![kw_syn, name, ty, body])
    }

    fn parse_show(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        self.skip_newlines();
        let expr = self.parse_expr();
        self.mk_node(SyntaxNodeKind::Show, vec![kw_syn, expr])
    }

    fn parse_suffices(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        self.skip_newlines();
        let expr = self.parse_expr();
        self.mk_node(SyntaxNodeKind::Suffices, vec![kw_syn, expr])
    }

    fn parse_return(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        self.skip_newlines();
        if self.is_at_end() || self.at_command_start() {
            return self.mk_node(SyntaxNodeKind::DoReturn, vec![kw_syn]);
        }
        let expr = self.parse_expr();
        self.mk_node(SyntaxNodeKind::DoReturn, vec![kw_syn, expr])
    }

    fn parse_nomatch(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        self.mk_node(SyntaxNodeKind::NoMatch, vec![kw_syn])
    }

    fn parse_nofun(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        self.mk_node(SyntaxNodeKind::NoFun, vec![kw_syn])
    }

    // -----------------------------------------------------------------------
    // Match
    // -----------------------------------------------------------------------

    fn parse_match(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Optional `(generalizing := true/false)`
        // Optional `(motive := fun x => ...)`
        while self.at_exact(&TokenKind::LParen) {
            let next = self.peek_nth_non_newline(1);
            if matches!(next.kind, TokenKind::Ident(ref s) if s == "generalizing" || s == "motive")
            {
                let group = self.parse_match_option();
                children.push(group);
                self.skip_newlines();
            } else {
                break;
            }
        }

        // Parse discriminants (comma-separated expressions)
        let discr = self.parse_expr();
        let mut discrs = vec![discr];
        while self.eat_exact(&TokenKind::Comma).is_some() {
            let comma = &self.tokens[self.pos - 1];
            discrs.push(self.mk_punct(comma));
            self.skip_newlines();
            discrs.push(self.parse_expr());
            self.skip_newlines();
        }
        children.push(self.mk_node(SyntaxNodeKind::MatchDiscr, discrs));
        self.skip_newlines();

        // Optional `: type`
        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        }
        self.skip_newlines();

        // `with`
        let with = self.expect_keyword(Keyword::With);
        children.push(self.mk_keyword_atom(&with));
        self.skip_newlines();

        // Match alternatives
        let alts = self.parse_match_alts();
        children.push(alts);

        self.mk_node(SyntaxNodeKind::Match, children)
    }

    fn parse_match_option(&mut self) -> Syntax {
        let lp = self.advance();
        let lp_syn = self.mk_punct(&lp);
        let mut children = vec![lp_syn];

        let name = self.advance();
        children.push(self.mk_ident_from_token(&name));
        self.skip_newlines();

        let assign = self.expect(&TokenKind::ColonEq);
        children.push(self.mk_punct(&assign));
        self.skip_newlines();

        let val = self.parse_expr();
        children.push(val);

        let rp = self.expect(&TokenKind::RParen);
        children.push(self.mk_punct(&rp));

        self.mk_node(SyntaxNodeKind::Group, children)
    }

    pub(crate) fn parse_match_alts(&mut self) -> Syntax {
        let mut alts = Vec::new();
        while self.at_exact(&TokenKind::Pipe) {
            let alt = self.parse_match_alt();
            alts.push(alt);
            self.skip_newlines();
        }
        self.mk_node(SyntaxNodeKind::MatchAlts, alts)
    }

    fn parse_match_alt(&mut self) -> Syntax {
        let pipe = self.advance();
        let pipe_syn = self.mk_punct(&pipe);
        let mut children = vec![pipe_syn];
        self.skip_newlines();

        // Parse patterns (comma-separated)
        children.push(self.parse_expr());
        self.skip_newlines();
        while self.eat_exact(&TokenKind::Comma).is_some() {
            let comma = &self.tokens[self.pos - 1];
            children.push(self.mk_punct(comma));
            self.skip_newlines();
            children.push(self.parse_expr());
            self.skip_newlines();
        }

        // `=>`
        let arrow = self.expect(&TokenKind::FatArrow);
        children.push(self.mk_punct(&arrow));
        self.skip_newlines();

        let body = self.parse_expr();
        children.push(body);

        self.mk_node(SyntaxNodeKind::MatchAlt, children)
    }

    // -----------------------------------------------------------------------
    // If / Do / By
    // -----------------------------------------------------------------------

    fn parse_if(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        // Optional `let` for `if let`
        if self.at_keyword(Keyword::Let) {
            let let_kw = self.advance();
            children.push(self.mk_keyword_atom(&let_kw));
            self.skip_newlines();
        }

        // Condition
        let cond = self.parse_expr();
        children.push(cond);
        self.skip_newlines();

        // `then`
        let then_kw = self.expect_keyword(Keyword::Then);
        children.push(self.mk_keyword_atom(&then_kw));
        self.skip_newlines();

        let then_body = self.parse_expr();
        children.push(then_body);
        self.skip_newlines();

        // `else`
        let else_kw = self.expect_keyword(Keyword::Else);
        children.push(self.mk_keyword_atom(&else_kw));
        self.skip_newlines();

        let else_body = self.parse_expr();
        children.push(else_body);

        self.mk_node(SyntaxNodeKind::IfThenElse, children)
    }

    fn parse_do(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        self.push_indent();
        let items = self.parse_do_seq();
        children.push(items);
        self.pop_indent();

        self.mk_node(SyntaxNodeKind::Do, children)
    }

    fn parse_do_seq(&mut self) -> Syntax {
        let mut items = Vec::new();
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
            if self.at_command_start() && col <= base_col {
                break;
            }

            let saved = self.pos;
            let item = self.parse_do_item();
            items.push(item);
            if self.pos == saved {
                self.advance();
            }
            self.skip_newlines();

            while self.eat_exact(&TokenKind::Semicolon).is_some() {
                self.skip_newlines();
            }
        }

        self.mk_node(SyntaxNodeKind::DoSeq, items)
    }

    fn parse_do_item(&mut self) -> Syntax {
        // `let` in do
        if self.at_keyword(Keyword::Let) {
            let let_expr = self.parse_let();
            return self.mk_node(SyntaxNodeKind::DoLet, vec![let_expr]);
        }
        // `return`
        if self.at_keyword(Keyword::Return) {
            return self.parse_return();
        }
        // `for ... in ... do`
        if self.at_keyword(Keyword::For) {
            return self.parse_do_for();
        }
        // `unless`
        if self.at_keyword(Keyword::Unless) {
            return self.parse_do_unless();
        }
        // `if`
        if self.at_keyword(Keyword::If) {
            let if_expr = self.parse_if();
            return self.mk_node(SyntaxNodeKind::DoExpr, vec![if_expr]);
        }
        // `match`
        if self.at_keyword(Keyword::Match) {
            let match_expr = self.parse_match();
            return self.mk_node(SyntaxNodeKind::DoExpr, vec![match_expr]);
        }

        // General expression; check for `<-` reassignment
        let expr = self.parse_expr();
        self.skip_newlines();

        if self.at_exact(&TokenKind::LArrow) {
            let arrow = self.advance();
            let arrow_syn = self.mk_punct(&arrow);
            self.skip_newlines();
            let rhs = self.parse_expr();
            return self.mk_node(SyntaxNodeKind::DoReassign, vec![expr, arrow_syn, rhs]);
        }

        self.mk_node(SyntaxNodeKind::DoExpr, vec![expr])
    }

    fn parse_do_for(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        self.skip_newlines();

        let pat = self.parse_expr();
        self.skip_newlines();

        let in_kw = self.expect_keyword(Keyword::In);
        let in_syn = self.mk_keyword_atom(&in_kw);
        self.skip_newlines();

        let iter = self.parse_expr();
        self.skip_newlines();

        let do_kw = self.expect_keyword(Keyword::Do);
        let do_syn = self.mk_keyword_atom(&do_kw);
        self.skip_newlines();

        let body = self.parse_expr();

        self.mk_node(
            SyntaxNodeKind::DoFor,
            vec![kw_syn, pat, in_syn, iter, do_syn, body],
        )
    }

    fn parse_do_unless(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        self.skip_newlines();

        let cond = self.parse_expr();
        self.skip_newlines();

        let do_kw = self.expect_keyword(Keyword::Do);
        let do_syn = self.mk_keyword_atom(&do_kw);
        self.skip_newlines();

        let body = self.parse_expr();

        self.mk_node(SyntaxNodeKind::DoUnless, vec![kw_syn, cond, do_syn, body])
    }

    fn parse_by(&mut self) -> Syntax {
        let kw = self.advance();
        let kw_syn = self.mk_keyword_atom(&kw);
        let mut children = vec![kw_syn];
        self.skip_newlines();

        self.push_indent();
        let tactics = self.parse_tactic_seq();
        children.push(tactics);
        self.pop_indent();

        self.mk_node(SyntaxNodeKind::ByTactic, children)
    }

    pub(crate) fn parse_tactic_seq(&mut self) -> Syntax {
        let mut tactics = Vec::new();
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

            let saved = self.pos;
            let tac = self.parse_tactic();
            tactics.push(tac);
            if self.pos == saved {
                self.advance();
            }
            self.skip_newlines();

            if self.eat_exact(&TokenKind::Semicolon).is_some() {
                let sc = &self.tokens[self.pos - 1];
                tactics.push(self.mk_punct(sc));
            }
        }

        self.mk_node(SyntaxNodeKind::TacticSeq, tactics)
    }

    fn parse_tactic(&mut self) -> Syntax {
        // Tactics: We parse them as a sequence of tokens/expressions until
        // the next tactic (at same indentation) or end of block.
        // For a minimal parser, we treat tactics as expressions.
        let base_col = self.current_column();
        let mut children = Vec::new();

        // First token of tactic
        let first = self.parse_expr();
        children.push(first);
        self.skip_newlines();

        // Consume continuation tokens at deeper indentation
        loop {
            if self.is_at_end() {
                break;
            }
            let col = self.current_column();
            if col <= base_col {
                break;
            }
            let saved = self.pos;
            let arg = self.parse_expr();
            children.push(arg);
            if self.pos == saved {
                self.advance();
            }
            self.skip_newlines();
        }

        if children.len() == 1 {
            return children.pop().unwrap();
        }
        self.mk_node(SyntaxNodeKind::Tactic, children)
    }

    // -----------------------------------------------------------------------
    // Binders (shared by expr and command)
    // -----------------------------------------------------------------------

    pub(crate) fn parse_binders(&mut self) -> Vec<Syntax> {
        let mut binders = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokenKind::LParen => binders.push(self.parse_explicit_binder()),
                TokenKind::LBrace => binders.push(self.parse_implicit_binder()),
                TokenKind::LBracket => binders.push(self.parse_inst_binder()),
                TokenKind::LDblAngle => binders.push(self.parse_strict_implicit_binder()),
                TokenKind::Ident(_) | TokenKind::Underscore => {
                    let tok = self.advance();
                    let syn = if tok.kind == TokenKind::Underscore {
                        self.mk_punct(&tok)
                    } else {
                        self.mk_ident_from_token(&tok)
                    };
                    binders.push(syn);
                }
                _ => break,
            }
        }
        binders
    }

    pub(crate) fn parse_single_binder(&mut self) -> Syntax {
        match self.peek_kind() {
            TokenKind::LParen => self.parse_explicit_binder(),
            TokenKind::LBrace => self.parse_implicit_binder(),
            TokenKind::LBracket => self.parse_inst_binder(),
            TokenKind::LDblAngle => self.parse_strict_implicit_binder(),
            _ => {
                self.error("expected binder");
                self.mk_missing()
            }
        }
    }

    pub(crate) fn parse_explicit_binder(&mut self) -> Syntax {
        let lp = self.advance();
        let lp_syn = self.mk_punct(&lp);
        let mut children = vec![lp_syn];
        self.skip_newlines();

        // Parse names
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

        // Optional `: type`
        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        }

        // Optional `:= default`
        if self.at_exact(&TokenKind::ColonEq) {
            let assign = self.advance();
            children.push(self.mk_punct(&assign));
            self.skip_newlines();
            children.push(self.parse_expr());
        }

        self.skip_newlines();
        let rp = self.expect(&TokenKind::RParen);
        children.push(self.mk_punct(&rp));

        self.mk_node(SyntaxNodeKind::ExplicitBinder, children)
    }

    pub(crate) fn parse_implicit_binder(&mut self) -> Syntax {
        let lb = self.advance();
        let lb_syn = self.mk_punct(&lb);
        let mut children = vec![lb_syn];
        self.skip_newlines();

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

        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        }

        self.skip_newlines();
        let rb = self.expect(&TokenKind::RBrace);
        children.push(self.mk_punct(&rb));

        self.mk_node(SyntaxNodeKind::ImplicitBinder, children)
    }

    pub(crate) fn parse_inst_binder(&mut self) -> Syntax {
        let lb = self.advance();
        let lb_syn = self.mk_punct(&lb);
        let mut children = vec![lb_syn];
        self.skip_newlines();

        // Optional name
        if self.at_ident() {
            let next = self.peek_nth_non_newline(1);
            if matches!(next.kind, TokenKind::Colon) {
                let name = self.advance();
                children.push(self.mk_ident_from_token(&name));
                self.skip_newlines();
            }
        }

        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        } else {
            // Just a type without name prefix
            let ty = self.parse_expr();
            children.push(ty);
        }

        self.skip_newlines();
        let rb = self.expect(&TokenKind::RBracket);
        children.push(self.mk_punct(&rb));

        self.mk_node(SyntaxNodeKind::InstBinder, children)
    }

    pub(crate) fn parse_strict_implicit_binder(&mut self) -> Syntax {
        let la = self.advance();
        let la_syn = self.mk_punct(&la);
        let mut children = vec![la_syn];
        self.skip_newlines();

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

        if let Some(ty) = self.parse_opt_type_annot() {
            children.push(ty);
        }

        self.skip_newlines();
        let ra = self.expect(&TokenKind::RDblAngle);
        children.push(self.mk_punct(&ra));

        self.mk_node(SyntaxNodeKind::StrictImplicitBinder, children)
    }

    // -----------------------------------------------------------------------
    // Pratt infix parsing
    // -----------------------------------------------------------------------

    fn parse_infix(&mut self, lhs: Syntax, min_bp: u32) -> Syntax {
        let mut lhs = lhs;
        loop {
            self.skip_newlines();
            let (op_bp, right_bp, is_arrow) = match self.peek_kind() {
                // Arrows (right-assoc)
                TokenKind::Arrow => (25, 24, true),

                // Logical
                TokenKind::OrOr | TokenKind::Disj => (30, 31, false),
                TokenKind::AndAnd | TokenKind::Conj => (35, 36, false),
                TokenKind::Iff => (20, 20, false),

                // Comparison
                TokenKind::Eq | TokenKind::Ne | TokenKind::Le | TokenKind::Ge => (50, 51, false),
                TokenKind::Lt | TokenKind::Gt => (50, 51, false),
                TokenKind::Mem => (50, 51, false),

                // Bind/Seq
                TokenKind::Bind => (55, 56, false),
                TokenKind::Seq | TokenKind::SeqLeft | TokenKind::SeqRight | TokenKind::SeqComp => {
                    (60, 61, false)
                }

                // Arithmetic
                TokenKind::Plus | TokenKind::Minus | TokenKind::Append => (65, 66, false),
                TokenKind::Cons => (67, 66, false), // right-assoc
                TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::Times
                | TokenKind::Cdot
                | TokenKind::SMul
                | TokenKind::Dvd => (70, 71, false),
                TokenKind::Caret => (80, 79, false), // right-assoc

                // Functor/applicative
                TokenKind::Map => (100, 101, false),
                TokenKind::OrElse => (20, 21, false),

                // Pipe
                TokenKind::PipeRight => (10, 11, false),
                TokenKind::PipeLeft => (10, 9, false), // right-assoc
                TokenKind::Dollar => (10, 9, false),   // right-assoc (`$` is right-assoc, low-prec)

                // Bit operations
                TokenKind::BitAnd | TokenKind::Ampersand => (35, 36, false),
                TokenKind::BitOr => (30, 31, false),
                TokenKind::BitXor => (32, 33, false),
                TokenKind::ShiftLeft | TokenKind::ShiftRight => (45, 46, false),

                // Composition
                TokenKind::Compose => (90, 91, false),
                TokenKind::Subst => (75, 76, false),

                _ => break,
            };

            if op_bp < min_bp {
                break;
            }

            let op_tok = self.advance();
            let op_syn = self.mk_punct(&op_tok);
            self.skip_newlines();

            let rhs = if is_arrow {
                self.parse_expr_max()
            } else {
                let rhs_atom = self.parse_unary();
                self.parse_infix(rhs_atom, right_bp)
            };

            let kind = if is_arrow {
                SyntaxNodeKind::Arrow
            } else {
                SyntaxNodeKind::BinOp
            };
            lhs = self.mk_node(kind, vec![lhs, op_syn, rhs]);
        }

        // Handle `: type` ascription at expression level
        if self.at_exact(&TokenKind::Colon) && min_bp == 0 {
            let colon = self.advance();
            let colon_syn = self.mk_punct(&colon);
            self.skip_newlines();
            let ty = self.parse_expr();
            lhs = self.mk_node(SyntaxNodeKind::TypeAscription, vec![lhs, colon_syn, ty]);
        }

        lhs
    }

    pub(crate) fn at_command_start(&self) -> bool {
        match self.peek_kind() {
            TokenKind::Keyword(kw) => crate::parser::is_command_keyword(kw),
            TokenKind::DocComment(_) | TokenKind::ModuleDoc(_) => true,
            TokenKind::At => {
                let next = self.peek_nth_non_newline(1);
                matches!(next.kind, TokenKind::LBracket)
            }
            _ => false,
        }
    }
}
