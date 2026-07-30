//! A parser and formatter for modern Perl.
//!
//! The layers are described by ADRs 0004-0008 in `dev/adr/`:
//!
//! * [`lang`] — the language vocabulary, generated from a single definition.
//! * [`lex`] — a scanner whose `expect` state lives in one place.
//! * [`parse`] — an event-recording parser, so speculative parsing is possible.
//! * [`fmt`] — a document IR, so layout is decided once and rendered once.

pub mod check;
pub mod cli;
pub mod diagnostic;
pub mod fmt;
pub mod lang;
pub mod lex;
pub mod parse;

use std::sync::Arc;

pub use diagnostic::ParseError;
pub use fmt::{DelimiterSpacing, FormatterOptions};
pub use lang::{NodeKind, SyntaxKind, SyntaxNode, SyntaxToken, TokenKind};
pub use parse::TriviaMap;

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
    let source: Arc<str> = Arc::from(input);
    let parsed = parse::parse(input);
    let errors = parsed
        .diagnostics
        .into_iter()
        .map(|diagnostic| ParseError::from_parse(diagnostic, &source))
        .collect();
    (SyntaxNode::new_root(parsed.green), parsed.trivia, errors)
}

/// Format Perl source with the default options.
#[must_use]
pub fn format_perl(input: &str) -> (String, Vec<ParseError>) {
    format_perl_with_options(input, &FormatterOptions::default())
}

/// Format Perl source.
#[must_use]
pub fn format_perl_with_options(
    input: &str,
    options: &FormatterOptions,
) -> (String, Vec<ParseError>) {
    let (node, trivia, errors) = parse_perl_with_trivia(input);
    (fmt::format(&node, &trivia, options), errors)
}
