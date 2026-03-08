// Pratt parser binding power table.
// The actual Pratt parsing loop is in expr.rs (parse_infix method).
// This module provides the binding power lookup and operator helpers.

use rslean_lexer::TokenKind;

/// Binding power pair: (left, right).
/// For left-associative ops: right > left.
/// For right-associative ops: left > right.
#[allow(dead_code)]
pub type BindingPower = (u32, u32);

#[allow(dead_code)]
pub fn infix_binding_power(op: &TokenKind) -> Option<BindingPower> {
    let bp = match op {
        // Pipe / dollar (low precedence)
        TokenKind::PipeRight => (10, 11),
        TokenKind::PipeLeft | TokenKind::Dollar => (10, 9),
        TokenKind::OrElse => (20, 21),
        TokenKind::Iff => (20, 20),

        // Arrow (right-assoc)
        TokenKind::Arrow => (25, 24),

        // Logical
        TokenKind::OrOr | TokenKind::Disj | TokenKind::BitOr => (30, 31),
        TokenKind::BitXor => (32, 33),
        TokenKind::AndAnd | TokenKind::Conj | TokenKind::BitAnd | TokenKind::Ampersand => (35, 36),

        // Bit shifts
        TokenKind::ShiftLeft | TokenKind::ShiftRight => (45, 46),

        // Comparison
        TokenKind::Eq
        | TokenKind::Ne
        | TokenKind::Le
        | TokenKind::Ge
        | TokenKind::Lt
        | TokenKind::Gt
        | TokenKind::Mem => (50, 51),

        // Monadic
        TokenKind::Bind => (55, 56),
        TokenKind::Seq | TokenKind::SeqLeft | TokenKind::SeqRight | TokenKind::SeqComp => (60, 61),

        // Additive
        TokenKind::Plus | TokenKind::Minus | TokenKind::Append => (65, 66),
        TokenKind::Cons => (67, 66), // right-assoc

        // Multiplicative
        TokenKind::Star
        | TokenKind::Slash
        | TokenKind::Percent
        | TokenKind::Times
        | TokenKind::Cdot
        | TokenKind::SMul
        | TokenKind::Dvd => (70, 71),

        // Subst
        TokenKind::Subst => (75, 76),

        // Power (right-assoc)
        TokenKind::Caret => (80, 79),

        // Composition
        TokenKind::Compose => (90, 91),

        // Functor map
        TokenKind::Map => (100, 101),

        _ => return None,
    };
    Some(bp)
}

#[allow(dead_code)]
pub fn prefix_binding_power(op: &TokenKind) -> Option<u32> {
    match op {
        TokenKind::Neg | TokenKind::Bang | TokenKind::Complement | TokenKind::Tilde => Some(200),
        TokenKind::Minus => Some(200),
        _ => None,
    }
}
