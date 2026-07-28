//! Operator binding powers (ADR 0007 §4).
//!
//! Ordered as `perlop` orders them. Two corrections against the table this
//! replaces are noted inline; a third — the direction the ADR asked bitwise
//! operators to move — is recorded in
//! `notes/2026-07-28-redesign-deviation-log.md` (L-003), because moving them
//! would have contradicted perlop rather than matched it.

use crate::lang::{TokenKind, T};

/// Higher binds tighter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Precedence(pub u8);

impl Precedence {
    pub const LOWEST: Precedence = Precedence(0);
    pub const OR_KW: Precedence = Precedence(4);
    pub const AND_KW: Precedence = Precedence(6);
    pub const NOT_KW: Precedence = Precedence(8);
    pub const COMMA: Precedence = Precedence(10);
    pub const ASSIGNMENT: Precedence = Precedence(15);
    pub const TERNARY: Precedence = Precedence(20);
    pub const RANGE: Precedence = Precedence(25);
    pub const LOGICAL_OR: Precedence = Precedence(30);
    pub const LOGICAL_AND: Precedence = Precedence(35);
    pub const BITWISE_OR: Precedence = Precedence(40);
    pub const BITWISE_AND: Precedence = Precedence(45);
    pub const EQUALITY: Precedence = Precedence(50);
    pub const RELATIONAL: Precedence = Precedence(55);
    /// File tests and named unary functions. The old table reused the prefix
    /// level, which binds tighter than `.`, so `-f $x . "y"` grouped as
    /// `(-f $x) . "y"` instead of perl's `-f ($x . "y")`.
    pub const NAMED_UNARY: Precedence = Precedence(60);
    pub const SHIFT: Precedence = Precedence(65);
    pub const ADDITIVE: Precedence = Precedence(70);
    pub const MULTIPLICATIVE: Precedence = Precedence(75);
    pub const REGEX_BIND: Precedence = Precedence(80);
    pub const PREFIX: Precedence = Precedence(85);
    pub const EXPONENT: Precedence = Precedence(90);
    pub const INCREMENT: Precedence = Precedence(95);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Associativity {
    Left,
    Right,
    /// `..`, `<=>`: chaining them is a syntax error in perl, and treating them
    /// as left-associative silently accepts nonsense.
    None,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct InfixOp {
    pub precedence: Precedence,
    pub associativity: Associativity,
    /// The node this operator builds, so the formatter and future lints can
    /// branch on operator class rather than re-inspecting the token
    /// (ADR 0007 §2).
    pub node: crate::lang::NodeKind,
}

use crate::lang::NodeKind;

/// The infix operator at this token, if there is one.
pub(crate) fn infix_op(kind: TokenKind) -> Option<InfixOp> {
    use Associativity::{Left, None as NonAssoc, Right};

    let (precedence, associativity, node) = match kind {
        T!["or"] | T!["xor"] => (Precedence::OR_KW, Left, NodeKind::BINARY_EXPR),
        T!["and"] => (Precedence::AND_KW, Left, NodeKind::BINARY_EXPR),

        kind if kind.is_assignment_op() => (Precedence::ASSIGNMENT, Right, NodeKind::ASSIGN_EXPR),

        T![".."] | T!["..."] => (Precedence::RANGE, NonAssoc, NodeKind::RANGE_EXPR),

        T!["||"] | T!["//"] => (Precedence::LOGICAL_OR, Left, NodeKind::BINARY_EXPR),
        T!["&&"] => (Precedence::LOGICAL_AND, Left, NodeKind::BINARY_EXPR),

        T!["|"] | T!["^"] => (Precedence::BITWISE_OR, Left, NodeKind::BINARY_EXPR),
        TokenKind::BITWISE_AND => (Precedence::BITWISE_AND, Left, NodeKind::BINARY_EXPR),

        // perlop calls these non-associative, but perl itself accepts
        // `$a < $b > 0` and groups it to the left, so that is what we build.
        T!["=="] | T!["!="] | T!["<=>"] | T!["eq"] | T!["ne"] | T!["cmp"] | T!["~~"] => {
            (Precedence::EQUALITY, Left, NodeKind::BINARY_EXPR)
        }
        T!["<"] | T![">"] | T!["<="] | T![">="] | T!["lt"] | T!["gt"] | T!["le"] | T!["ge"] => {
            (Precedence::RELATIONAL, Left, NodeKind::BINARY_EXPR)
        }

        T!["<<"] | T![">>"] => (Precedence::SHIFT, Left, NodeKind::BINARY_EXPR),
        T!["+"] | T!["-"] | T!["."] => (Precedence::ADDITIVE, Left, NodeKind::BINARY_EXPR),
        T!["/"] | T!["x"] | TokenKind::STAR | TokenKind::MODULO => {
            (Precedence::MULTIPLICATIVE, Left, NodeKind::BINARY_EXPR)
        }
        T!["=~"] | T!["!~"] => (Precedence::REGEX_BIND, Left, NodeKind::MATCH_EXPR),
        T!["**"] => (Precedence::EXPONENT, Right, NodeKind::BINARY_EXPR),

        _ => return None,
    };

    Some(InfixOp {
        precedence,
        associativity,
        node,
    })
}

/// The binding power an operand is parsed at, for a left-associative operator of
/// the given precedence.
pub(crate) fn right_binding_power(op: InfixOp) -> Precedence {
    match op.associativity {
        Associativity::Left | Associativity::None => Precedence(op.precedence.0 + 1),
        Associativity::Right => op.precedence,
    }
}
