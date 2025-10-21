//! Type definitions for the lexer state machine.

use crate::{SyntaxKind, T};

/// External lexical context hint provided by the parser to disambiguate
/// only operator/value sensitive tokens in default contexts.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LexContext {
    /// Expecting a value, identifier, or sigil (after keywords, operators, sigils)
    #[default]
    Value,
    /// Expecting an operator (after identifiers, numbers, variables)
    Operator,
    /// Ambiguous value lookahead used by the parser when probing for possible arguments.
    ///
    /// This behaves like `Value` for most tokens but avoids implicit regex/io/filetest
    /// handling and performs additional disambiguation for sigils so the parser can
    /// inspect upcoming tokens without forcing value-context side effects.
    AmbiguousValueLookahead,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DelimiterPhase {
    First,
    Second,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DelimiterType {
    Open,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuoteLikeMode {
    Q,  // q, qq, qx
    QW, // qw
    M,  // m
    QR, // qr
    S,  // s
    TR, // tr, y
}

impl QuoteLikeMode {
    /// Get QuoteLikeMode from keyword SyntaxKind
    pub fn from_keyword(kind: SyntaxKind) -> Self {
        match kind {
            T![q] | T![qq] | T![qx] => Self::Q,
            T![qw] => Self::QW,
            T![m] => Self::M,
            T![qr] => Self::QR,
            T![s] => Self::S,
            T![tr] | T![y] => Self::TR,
            _ => panic!("Invalid quote-like keyword: {:?}", kind),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuoteLikeState {
    Delimiter {
        phase: DelimiterPhase,
        kind: DelimiterType,
    },
    Content {
        phase: DelimiterPhase,
    },
    Flags,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum LexerMode {
    #[default]
    Normal,
    QuoteLike {
        prefix: SyntaxKind,
        mode: QuoteLikeMode,
        state: QuoteLikeState,
        delimiter: char,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct HeredocMarker<'a> {
    pub(super) marker: &'a str,
    pub(super) strip_indent: bool,
}
