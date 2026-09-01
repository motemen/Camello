//! A parser and formatter for modern Perl.
//!
//! The layers are described in `docs/architecture.md`, and since the workspace
//! split (`docs/typecheck.md`, "Where it lives") they are crates:
//!
//! * [`camello_syntax`] — vocabulary, scanner, event-recording parser, AST views.
//! * [`camello_fmt`] — a document IR, so layout is decided once and rendered once.
//! * [`camello_sema`] — declarations, types, and the two-phase checker.
//! * [`camello_lsp`] — the editor front end over all three (`docs/lsp.md`).
//!
//! This crate is the command line and the invariants that compare the two.

pub mod check;
pub mod cli;
pub mod config;
pub mod report;

pub use camello_fmt as fmt;
pub use camello_lsp as lsp;
pub use camello_sema as sema;
pub use camello_syntax::{diagnostic, lang, lex, parse};

pub use camello_fmt::{DelimiterSpacing, FormatterOptions};
pub use camello_syntax::{
    parse_perl, parse_perl_with_trivia, NodeKind, ParseError, PerlNode, SyntaxKind, SyntaxNode,
    SyntaxToken, TokenKind, TriviaMap,
};

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
    (camello_fmt::format(&node, &trivia, options), errors)
}
