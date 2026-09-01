//! The Perl front end: vocabulary, scanner, parser, and typed views.
//!
//! The layers are described in `docs/architecture.md`:
//!
//! * [`lang`] — the language vocabulary, generated from a single definition.
//! * [`lex`] — a scanner whose `expect` state lives in one place.
//! * [`parse`] — an event-recording parser, so speculative parsing is possible.
//! * [`ast`] — typed views over the CST, for callers that ask about structure.
//!
//! This crate knows nothing about formatting or checking. `camello-fmt` and the
//! checker both sit on top of it, and neither can see the other
//! (`docs/typecheck.md`, "Where it lives").

pub mod ast;
pub mod diagnostic;
pub mod hash;
pub mod lang;
pub mod lex;
pub mod parse;

use std::sync::Arc;

pub use ast::AstNode;
pub use diagnostic::ParseError;
pub use lang::{NodeKind, SyntaxKind, SyntaxNode, SyntaxToken, TokenKind};
pub use parse::{is_builtin, TriviaMap};

/// The root of a parsed file.
pub type PerlNode = SyntaxNode;

/// Parse Perl source into a lossless CST.
#[must_use]
pub fn parse_perl(input: &str) -> (PerlNode, Vec<ParseError>) {
    let (node, _trivia, errors) = parse_perl_with_trivia(input);
    (node, errors)
}

/// Parse, keeping the trivia map the formatter needs.
#[must_use]
pub fn parse_perl_with_trivia(input: &str) -> (PerlNode, TriviaMap, Vec<ParseError>) {
    let parsed = parse::parse(input);
    // The source is copied for the diagnostics to point into, and only then: a
    // file that parses cleanly is the common case, and copying it whole to hand
    // an empty iterator something to borrow was a copy of every byte of every
    // file in a run.
    let errors = if parsed.diagnostics.is_empty() {
        Vec::new()
    } else {
        let source: Arc<str> = Arc::from(input);
        parsed
            .diagnostics
            .into_iter()
            .map(|diagnostic| ParseError::from_parse(diagnostic, &source))
            .collect()
    };
    (SyntaxNode::new_root(parsed.green), parsed.trivia, errors)
}
