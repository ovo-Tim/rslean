use rslean_syntax::Span;

use crate::{Keyword, Token, TokenKind};

pub fn tokenize(source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(source);
    lexer.tokenize();
    lexer.tokens
}

struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            tokens: Vec::new(),
        }
    }

    fn tokenize(&mut self) {
        while !self.is_eof() {
            if self.starts_with(" ") || self.starts_with("\t") || self.starts_with("\r") {
                self.bump_char();
                continue;
            }

            if self.starts_with("\n") {
                let start = self.pos;
                self.bump_char();
                self.push_token(TokenKind::Newline, start, self.pos);
                continue;
            }

            if self.starts_with("--") {
                self.skip_line_comment();
                continue;
            }

            if self.starts_with("/-!") {
                self.lex_doc_comment(true);
                continue;
            }

            if self.starts_with("/--") {
                self.lex_doc_comment(false);
                continue;
            }

            if self.starts_with("/-") {
                self.skip_block_comment();
                continue;
            }

            if self.is_interpol_prefix() {
                self.lex_interpolated_string();
                continue;
            }

            if self.starts_with("\"") {
                self.lex_string();
                continue;
            }

            if self.starts_with("'") {
                self.lex_char();
                continue;
            }

            if self.starts_with("«") {
                self.lex_guillemet_ident();
                continue;
            }

            if self.peek_char().is_some_and(Self::is_dec_digit) {
                self.lex_number();
                continue;
            }

            if self.starts_with("#") && self.lex_hash_keyword() {
                continue;
            }

            if self.peek_char().is_some_and(Self::is_ident_start) {
                self.lex_ident_or_keyword();
                continue;
            }

            if self.lex_operator_or_punct() {
                continue;
            }

            let start = self.pos;
            let ch = self.bump_char().unwrap_or('\0');
            self.push_token(TokenKind::Error(ch.to_string()), start, self.pos);
        }

        self.push_token(TokenKind::Eof, self.pos, self.pos);
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn peek_nth_char(&self, n: usize) -> Option<char> {
        self.source[self.pos..].chars().nth(n)
    }

    fn starts_with(&self, s: &str) -> bool {
        self.source[self.pos..].starts_with(s)
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn push_token(&mut self, kind: TokenKind, start: usize, end: usize) {
        let span = Span::new(start as u32, end as u32);
        self.tokens.push(Token::new(kind, span));
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch == '\n' {
                break;
            }
            self.bump_char();
        }
    }

    fn skip_block_comment(&mut self) {
        let mut depth = 0usize;
        while !self.is_eof() {
            if self.starts_with("/-") {
                depth += 1;
                self.pos += 2;
                continue;
            }
            if self.starts_with("-/") {
                self.pos += 2;
                depth -= 1;
                if depth == 0 {
                    break;
                }
                continue;
            }
            self.bump_char();
        }
    }

    fn lex_doc_comment(&mut self, module: bool) {
        let start = self.pos;
        self.pos += 3;

        let content_start = self.pos;
        let mut depth = 1usize;
        while !self.is_eof() {
            if self.starts_with("/-") {
                depth += 1;
                self.pos += 2;
                continue;
            }
            if self.starts_with("-/") {
                depth -= 1;
                if depth == 0 {
                    let content = self.source[content_start..self.pos].to_string();
                    self.pos += 2;
                    let kind = if module {
                        TokenKind::ModuleDoc(content)
                    } else {
                        TokenKind::DocComment(content)
                    };
                    self.push_token(kind, start, self.pos);
                    return;
                }
                self.pos += 2;
                continue;
            }
            self.bump_char();
        }

        self.push_token(
            TokenKind::Error("unterminated doc comment".to_string()),
            start,
            self.pos,
        );
    }

    fn is_interpol_prefix(&self) -> bool {
        (self.starts_with("s!\"") || self.starts_with("f!\""))
            && self.peek_nth_char(0).is_some()
            && self.peek_nth_char(1) == Some('!')
            && self.peek_nth_char(2) == Some('"')
    }

    fn lex_interpolated_string(&mut self) {
        let begin_start = self.pos;
        self.pos += 2;
        self.push_token(TokenKind::InterpolBegin, begin_start, self.pos);
        self.lex_string();
        let end = self.pos;
        self.push_token(TokenKind::InterpolEnd, end, end);
    }

    fn lex_string(&mut self) {
        let start = self.pos;
        self.bump_char();
        let mut out = String::new();

        while let Some(ch) = self.peek_char() {
            if ch == '"' {
                self.bump_char();
                self.push_token(TokenKind::StrLit(out), start, self.pos);
                return;
            }
            if ch == '\\' {
                self.bump_char();
                match self.read_escape() {
                    Ok(c) => out.push(c),
                    Err(msg) => {
                        self.push_token(TokenKind::Error(msg), start, self.pos);
                        self.consume_until_quote();
                        return;
                    }
                }
                continue;
            }
            out.push(ch);
            self.bump_char();
        }

        self.push_token(
            TokenKind::Error("unterminated string literal".to_string()),
            start,
            self.pos,
        );
    }

    fn consume_until_quote(&mut self) {
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            if ch == '"' {
                break;
            }
        }
    }

    fn read_escape(&mut self) -> Result<char, String> {
        let esc = self
            .bump_char()
            .ok_or_else(|| "invalid escape sequence".to_string())?;
        match esc {
            'n' => Ok('\n'),
            't' => Ok('\t'),
            'r' => Ok('\r'),
            '\\' => Ok('\\'),
            '"' => Ok('"'),
            '\'' => Ok('\''),
            '0' => Ok('\0'),
            'x' => self.read_hex_escape(),
            'u' => self.read_unicode_escape(),
            _ => Err(format!("unknown escape: \\{esc}")),
        }
    }

    fn read_hex_escape(&mut self) -> Result<char, String> {
        let h1 = self
            .bump_char()
            .ok_or_else(|| "incomplete hex escape".to_string())?;
        let h2 = self
            .bump_char()
            .ok_or_else(|| "incomplete hex escape".to_string())?;
        let value = Self::hex_val(h1)
            .and_then(|a| Self::hex_val(h2).map(|b| a * 16 + b))
            .ok_or_else(|| "invalid hex escape".to_string())?;
        char::from_u32(value).ok_or_else(|| "invalid hex scalar".to_string())
    }

    fn read_unicode_escape(&mut self) -> Result<char, String> {
        if self.bump_char() != Some('{') {
            return Err("invalid unicode escape".to_string());
        }

        let mut value: u32 = 0;
        let mut digits = 0usize;

        while let Some(ch) = self.peek_char() {
            if ch == '}' {
                self.bump_char();
                if digits == 0 {
                    return Err("empty unicode escape".to_string());
                }
                return char::from_u32(value).ok_or_else(|| "invalid unicode scalar".to_string());
            }
            let d = Self::hex_val(ch).ok_or_else(|| "invalid unicode escape".to_string())?;
            self.bump_char();
            value = value.saturating_mul(16).saturating_add(d);
            digits += 1;
            if digits > 6 {
                return Err("unicode escape too long".to_string());
            }
        }

        Err("unterminated unicode escape".to_string())
    }

    fn lex_char(&mut self) {
        let start = self.pos;
        self.bump_char();

        let ch = match self.peek_char() {
            Some('\\') => {
                self.bump_char();
                match self.read_escape() {
                    Ok(c) => c,
                    Err(msg) => {
                        self.push_token(TokenKind::Error(msg), start, self.pos);
                        return;
                    }
                }
            }
            Some(c) => {
                self.bump_char();
                c
            }
            None => {
                self.push_token(
                    TokenKind::Error("unterminated char literal".to_string()),
                    start,
                    self.pos,
                );
                return;
            }
        };

        if self.bump_char() != Some('\'') {
            self.push_token(
                TokenKind::Error("unterminated char literal".to_string()),
                start,
                self.pos,
            );
            return;
        }

        self.push_token(TokenKind::CharLit(ch), start, self.pos);
    }

    fn lex_guillemet_ident(&mut self) {
        let start = self.pos;
        self.bump_char();
        let content_start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch == '»' {
                let content = self.source[content_start..self.pos].to_string();
                self.bump_char();
                self.push_token(TokenKind::Ident(content), start, self.pos);
                return;
            }
            self.bump_char();
        }
        self.push_token(
            TokenKind::Error("unterminated quoted identifier".to_string()),
            start,
            self.pos,
        );
    }

    fn is_ident_start(ch: char) -> bool {
        (ch == '_' || ch.is_alphabetic()) && ch != 'λ'
    }

    fn is_ident_continue(ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '\'' || ch == '?' || ch == '!'
    }

    fn lex_ident_or_keyword(&mut self) {
        let start = self.pos;
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            if Self::is_ident_continue(ch) {
                self.bump_char();
            } else {
                break;
            }
        }

        loop {
            if !self.starts_with(".") {
                break;
            }
            let after_dot = self.pos + 1;
            if after_dot >= self.source.len() {
                break;
            }
            let next = self.source[after_dot..].chars().next();
            if !next.is_some_and(Self::is_ident_start) {
                break;
            }
            self.pos += 1;
            self.bump_char();
            while let Some(ch) = self.peek_char() {
                if Self::is_ident_continue(ch) {
                    self.bump_char();
                } else {
                    break;
                }
            }
        }

        let text = &self.source[start..self.pos];
        if text == "_" {
            self.push_token(TokenKind::Underscore, start, self.pos);
            return;
        }

        if let Some(kw) = Keyword::parse(text) {
            self.push_token(TokenKind::Keyword(kw), start, self.pos);
            return;
        }

        self.push_token(TokenKind::Ident(text.to_string()), start, self.pos);
    }

    fn is_dec_digit(ch: char) -> bool {
        ch.is_ascii_digit()
    }

    fn lex_number(&mut self) {
        let start = self.pos;

        if self.starts_with("0x") || self.starts_with("0X") {
            self.pos += 2;
            self.consume_digits_with_underscore(|c| c.is_ascii_hexdigit());
            self.push_token(
                TokenKind::NatLit(self.source[start..self.pos].to_string()),
                start,
                self.pos,
            );
            return;
        }

        if self.starts_with("0b") || self.starts_with("0B") {
            self.pos += 2;
            self.consume_digits_with_underscore(|c| c == '0' || c == '1');
            self.push_token(
                TokenKind::NatLit(self.source[start..self.pos].to_string()),
                start,
                self.pos,
            );
            return;
        }

        if self.starts_with("0o") || self.starts_with("0O") {
            self.pos += 2;
            self.consume_digits_with_underscore(|c| matches!(c, '0'..='7'));
            self.push_token(
                TokenKind::NatLit(self.source[start..self.pos].to_string()),
                start,
                self.pos,
            );
            return;
        }

        self.consume_digits_with_underscore(|c| c.is_ascii_digit());

        let mut has_frac = false;
        let mut has_exp = false;

        if self.starts_with(".")
            && !self.starts_with("..")
            && self.peek_nth_char(1).is_some_and(|c| c.is_ascii_digit())
        {
            has_frac = true;
            self.pos += 1;
            self.consume_digits_with_underscore(|c| c.is_ascii_digit());
        }

        if matches!(self.peek_char(), Some('e' | 'E')) {
            let save = self.pos;
            self.bump_char();
            if matches!(self.peek_char(), Some('+' | '-')) {
                self.bump_char();
            }
            let exp_start = self.pos;
            self.consume_digits_with_underscore(|c| c.is_ascii_digit());
            if self.pos > exp_start {
                has_exp = true;
            } else {
                self.pos = save;
            }
        }

        if has_frac || has_exp {
            self.push_token(
                TokenKind::ScientificLit(self.source[start..self.pos].to_string()),
                start,
                self.pos,
            );
        } else {
            self.push_token(
                TokenKind::NatLit(self.source[start..self.pos].to_string()),
                start,
                self.pos,
            );
        }
    }

    fn consume_digits_with_underscore<F>(&mut self, mut valid: F)
    where
        F: FnMut(char) -> bool,
    {
        while let Some(ch) = self.peek_char() {
            if ch == '_' || valid(ch) {
                self.bump_char();
            } else {
                break;
            }
        }
    }

    fn lex_hash_keyword(&mut self) -> bool {
        let start = self.pos;
        self.bump_char();
        let ident_start = self.pos;
        while let Some(ch) = self.peek_char() {
            if Self::is_ident_continue(ch) {
                self.bump_char();
            } else {
                break;
            }
        }

        if self.pos == ident_start {
            self.push_token(TokenKind::Hash, start, self.pos);
            return true;
        }

        let text = &self.source[start..self.pos];
        if let Some(kw) = Keyword::parse(text) {
            self.push_token(TokenKind::Keyword(kw), start, self.pos);
        } else {
            self.push_token(TokenKind::Hash, start, start + 1);
            self.push_token(
                TokenKind::Ident(self.source[ident_start..self.pos].to_string()),
                ident_start,
                self.pos,
            );
        }
        true
    }

    fn lex_operator_or_punct(&mut self) -> bool {
        #[allow(clippy::type_complexity)]
        const OPS: [(&str, fn() -> TokenKind); 36] = [
            ("<<<", || TokenKind::ShiftLeft),
            (">>>", || TokenKind::ShiftRight),
            ("&&&", || TokenKind::BitAnd),
            ("|||", || TokenKind::BitOr),
            ("^^^", || TokenKind::BitXor),
            ("~~~", || TokenKind::Complement),
            (">>=", || TokenKind::Bind),
            ("<|>", || TokenKind::OrElse),
            ("<$>", || TokenKind::Map),
            ("<*>", || TokenKind::SeqComp),
            ("<*", || TokenKind::SeqLeft),
            ("*>", || TokenKind::SeqRight),
            ("|>", || TokenKind::PipeRight),
            ("<|", || TokenKind::PipeLeft),
            (">>", || TokenKind::Seq),
            ("++", || TokenKind::Append),
            ("::", || TokenKind::ColonColon),
            (":=", || TokenKind::ColonEq),
            ("..", || TokenKind::DotDot),
            ("=>", || TokenKind::FatArrow),
            ("->", || TokenKind::Arrow),
            ("←", || TokenKind::LArrow),
            ("<-", || TokenKind::LArrow),
            ("→", || TokenKind::Arrow),
            ("&&", || TokenKind::AndAnd),
            ("||", || TokenKind::OrOr),
            ("<=", || TokenKind::Le),
            ("≥", || TokenKind::Ge),
            (">=", || TokenKind::Ge),
            ("≤", || TokenKind::Le),
            ("!=", || TokenKind::Ne),
            ("≠", || TokenKind::Ne),
            ("↔", || TokenKind::Iff),
            ("⦃", || TokenKind::LDblAngle),
            ("⦄", || TokenKind::RDblAngle),
            ("∀", || TokenKind::Forall),
        ];

        for (op, kind_fn) in OPS {
            if self.starts_with(op) {
                let start = self.pos;
                self.pos += op.len();
                self.push_token(kind_fn(), start, self.pos);
                return true;
            }
        }

        let Some(ch) = self.peek_char() else {
            return false;
        };
        let start = self.pos;
        self.bump_char();

        let kind = match ch {
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '⟨' => TokenKind::LAngle,
            '⟩' => TokenKind::RAngle,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            ':' => TokenKind::Colon,
            '.' => TokenKind::Dot,
            '@' => TokenKind::At,
            '?' => TokenKind::Question,
            '!' => TokenKind::Bang,
            '|' => TokenKind::Pipe,
            '$' => TokenKind::Dollar,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '^' => TokenKind::Caret,
            '&' => TokenKind::Ampersand,
            '~' => TokenKind::Tilde,
            '=' => TokenKind::Eq,
            '<' => TokenKind::Lt,
            '>' => TokenKind::Gt,
            'λ' => TokenKind::Lambda,
            '∃' => TokenKind::Exists,
            '¬' => TokenKind::Neg,
            '∧' => TokenKind::Conj,
            '∨' => TokenKind::Disj,
            '∈' => TokenKind::Mem,
            '∘' => TokenKind::Compose,
            '×' => TokenKind::Times,
            '·' => TokenKind::Cdot,
            '▸' => TokenKind::Subst,
            '•' => TokenKind::SMul,
            '∣' => TokenKind::Dvd,
            '#' => TokenKind::Hash,
            '_' => TokenKind::Underscore,
            _ => {
                self.pos = start;
                return false;
            }
        };

        self.push_token(kind, start, self.pos);
        true
    }

    fn hex_val(ch: char) -> Option<u32> {
        ch.to_digit(16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn tokenizes_basic_keywords() {
        assert_eq!(
            kinds("def theorem where"),
            vec![
                TokenKind::Keyword(Keyword::Def),
                TokenKind::Keyword(Keyword::Theorem),
                TokenKind::Keyword(Keyword::Where),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_identifiers_and_dotted_names() {
        assert_eq!(
            kinds("Nat.add αβγ foo'?"),
            vec![
                TokenKind::Ident("Nat.add".to_string()),
                TokenKind::Ident("αβγ".to_string()),
                TokenKind::Ident("foo'?".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_underscore() {
        assert_eq!(kinds("_"), vec![TokenKind::Underscore, TokenKind::Eof]);
    }

    #[test]
    fn tokenizes_guillemet_identifier() {
        assert_eq!(
            kinds("«hello world»"),
            vec![TokenKind::Ident("hello world".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn tokenizes_decimal_nat() {
        assert_eq!(
            kinds("1_000"),
            vec![TokenKind::NatLit("1_000".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn tokenizes_hex_binary_octal() {
        assert_eq!(
            kinds("0x1F 0b1010 0o17"),
            vec![
                TokenKind::NatLit("0x1F".to_string()),
                TokenKind::NatLit("0b1010".to_string()),
                TokenKind::NatLit("0o17".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_scientific_literals() {
        assert_eq!(
            kinds("1.5e10 2e-3 42.0"),
            vec![
                TokenKind::ScientificLit("1.5e10".to_string()),
                TokenKind::ScientificLit("2e-3".to_string()),
                TokenKind::ScientificLit("42.0".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn range_does_not_merge_with_number() {
        assert_eq!(
            kinds("1..2"),
            vec![
                TokenKind::NatLit("1".to_string()),
                TokenKind::DotDot,
                TokenKind::NatLit("2".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_string_literal_with_escapes() {
        assert_eq!(
            kinds("\"a\\n\\t\\\\\\\"\""),
            vec![TokenKind::StrLit("a\n\t\\\"".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn tokenizes_string_hex_and_unicode_escapes() {
        assert_eq!(
            kinds("\"\\x41\\u{1F600}\""),
            vec![TokenKind::StrLit("A😀".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn tokenizes_interpolated_string_markers() {
        assert_eq!(
            kinds("s!\"hello {x}\""),
            vec![
                TokenKind::InterpolBegin,
                TokenKind::StrLit("hello {x}".to_string()),
                TokenKind::InterpolEnd,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_char_literal_simple() {
        assert_eq!(kinds("'a'"), vec![TokenKind::CharLit('a'), TokenKind::Eof]);
    }

    #[test]
    fn tokenizes_char_literal_escape() {
        assert_eq!(
            kinds("'\\n'"),
            vec![TokenKind::CharLit('\n'), TokenKind::Eof]
        );
    }

    #[test]
    fn tokenizes_char_literal_unicode_escape() {
        assert_eq!(
            kinds("'\\u{1F600}'"),
            vec![TokenKind::CharLit('😀'), TokenKind::Eof]
        );
    }

    #[test]
    fn emits_newline_tokens() {
        assert_eq!(
            kinds("a\n\nb"),
            vec![
                TokenKind::Ident("a".to_string()),
                TokenKind::Newline,
                TokenKind::Newline,
                TokenKind::Ident("b".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn skips_line_comment() {
        assert_eq!(
            kinds("a -- comment\nb"),
            vec![
                TokenKind::Ident("a".to_string()),
                TokenKind::Newline,
                TokenKind::Ident("b".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn skips_nested_block_comments() {
        assert_eq!(
            kinds("a /- x /- y -/ z -/ b"),
            vec![
                TokenKind::Ident("a".to_string()),
                TokenKind::Ident("b".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_doc_comment() {
        let toks = kinds("/-- doc -/");
        assert_eq!(
            toks,
            vec![TokenKind::DocComment(" doc ".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn tokenizes_module_doc_comment() {
        let toks = kinds("/-! module -/");
        assert_eq!(
            toks,
            vec![TokenKind::ModuleDoc(" module ".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn tokenizes_delimiters() {
        assert_eq!(
            kinds("()[]{}⟨⟩⦃⦄"),
            vec![
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LAngle,
                TokenKind::RAngle,
                TokenKind::LDblAngle,
                TokenKind::RDblAngle,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_punctuation() {
        assert_eq!(
            kinds(", ; : :: := . .. @ # ? ! | $"),
            vec![
                TokenKind::Comma,
                TokenKind::Semicolon,
                TokenKind::Colon,
                TokenKind::ColonColon,
                TokenKind::ColonEq,
                TokenKind::Dot,
                TokenKind::DotDot,
                TokenKind::At,
                TokenKind::Hash,
                TokenKind::Question,
                TokenKind::Bang,
                TokenKind::Pipe,
                TokenKind::Dollar,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_ascii_operators() {
        assert_eq!(
            kinds("+ - * / % ^ & ~ = != <= >= < > && || ++ >> >>= ::"),
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::Caret,
                TokenKind::Ampersand,
                TokenKind::Tilde,
                TokenKind::Eq,
                TokenKind::Ne,
                TokenKind::Le,
                TokenKind::Ge,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::AndAnd,
                TokenKind::OrOr,
                TokenKind::Append,
                TokenKind::Seq,
                TokenKind::Bind,
                TokenKind::ColonColon,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_arrows_and_pipes() {
        assert_eq!(
            kinds("-> => <- → ← |> <| <|> <$> <*> <* *>"),
            vec![
                TokenKind::Arrow,
                TokenKind::FatArrow,
                TokenKind::LArrow,
                TokenKind::Arrow,
                TokenKind::LArrow,
                TokenKind::PipeRight,
                TokenKind::PipeLeft,
                TokenKind::OrElse,
                TokenKind::Map,
                TokenKind::SeqComp,
                TokenKind::SeqLeft,
                TokenKind::SeqRight,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_unicode_operators() {
        assert_eq!(
            kinds("∀ ∃ λ ¬ ∧ ∨ ↔ ∈ ∘ × · ▸ • ∣"),
            vec![
                TokenKind::Forall,
                TokenKind::Exists,
                TokenKind::Lambda,
                TokenKind::Neg,
                TokenKind::Conj,
                TokenKind::Disj,
                TokenKind::Iff,
                TokenKind::Mem,
                TokenKind::Compose,
                TokenKind::Times,
                TokenKind::Cdot,
                TokenKind::Subst,
                TokenKind::SMul,
                TokenKind::Dvd,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_bitwise_operators() {
        assert_eq!(
            kinds("&&& ||| ^^^ <<< >>> ~~~"),
            vec![
                TokenKind::BitAnd,
                TokenKind::BitOr,
                TokenKind::BitXor,
                TokenKind::ShiftLeft,
                TokenKind::ShiftRight,
                TokenKind::Complement,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_hash_keywords() {
        assert_eq!(
            kinds("#check #eval #print #reduce #synth #exit"),
            vec![
                TokenKind::Keyword(Keyword::Check),
                TokenKind::Keyword(Keyword::Eval),
                TokenKind::Keyword(Keyword::Print),
                TokenKind::Keyword(Keyword::Reduce),
                TokenKind::Keyword(Keyword::Synth),
                TokenKind::Keyword(Keyword::Exit),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn hash_then_identifier_when_not_command_keyword() {
        assert_eq!(
            kinds("#foo"),
            vec![
                TokenKind::Hash,
                TokenKind::Ident("foo".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn unknown_char_emits_error_and_recovers() {
        assert_eq!(
            kinds("a § b"),
            vec![
                TokenKind::Ident("a".to_string()),
                TokenKind::Error("§".to_string()),
                TokenKind::Ident("b".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn unterminated_string_emits_error() {
        let ks = kinds("\"abc");
        assert!(matches!(ks[0], TokenKind::Error(_)));
        assert_eq!(ks[1], TokenKind::Eof);
    }

    #[test]
    fn unterminated_char_emits_error() {
        let ks = kinds("'a");
        assert!(matches!(ks[0], TokenKind::Error(_)));
        assert_eq!(ks[1], TokenKind::Eof);
    }

    #[test]
    fn token_spans_are_byte_offsets() {
        let toks = tokenize("α + β");
        assert_eq!(toks[0].span.start, 0);
        assert_eq!(toks[0].span.end, 2);
        assert_eq!(toks[1].span.start, 3);
        assert_eq!(toks[1].span.end, 4);
        assert_eq!(toks[2].span.start, 5);
        assert_eq!(toks[2].span.end, 7);
    }

    #[test]
    fn token_helper_methods_work() {
        let tok = Token::new(TokenKind::Keyword(Keyword::Def), Span::new(0, 3));
        assert!(tok.is_keyword(Keyword::Def));
        assert!(!tok.is_ident());

        let eof = Token::new(TokenKind::Eof, Span::new(0, 0));
        assert!(eof.is_eof());

        assert!(TokenKind::Plus.is_operator());
        assert!(TokenKind::NatLit("1".to_string()).is_literal());
    }

    #[test]
    fn keyword_roundtrip_display() {
        assert_eq!(Keyword::parse("def"), Some(Keyword::Def));
        assert_eq!(Keyword::Def.as_str(), "def");
        assert_eq!(Keyword::Def.to_string(), "def");
    }
}
