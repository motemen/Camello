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
pub mod source;

pub use camello_fmt as fmt;
pub use camello_lsp as lsp;
pub use camello_sema as sema;
pub use camello_syntax::{diagnostic, lang, lex, parse};

pub use camello_fmt::{DelimiterSpacing, FormatterOptions};
pub use camello_syntax::{
    parse_perl, parse_perl_with_trivia, NodeKind, ParseError, PerlNode, SyntaxKind, SyntaxNode,
    SyntaxToken, TokenKind, TriviaMap,
};

/// How a command came out, as the status the process exits with.
///
/// Returned rather than exited with, so that `main` is the one place the
/// process ends and a run that fails can be asked about by whoever called it.
/// A command that could not do what it was asked at all — a flag it cannot
/// read, a file it cannot open — returns an `Err` instead, and exits 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum Exit {
    /// 0: nothing to report.
    Success,
    /// 1: what the run was asked to fail on was found — a file that is not
    /// formatted under `--check`, one that does not parse, a diagnostic at or
    /// above `--error-on`, a violated invariant.
    Failure,
    /// 2: a source that does not parse, under `dev dump`.
    ParseError,
}

impl From<Exit> for std::process::ExitCode {
    fn from(exit: Exit) -> Self {
        match exit {
            Exit::Success => std::process::ExitCode::SUCCESS,
            Exit::Failure => std::process::ExitCode::from(1),
            Exit::ParseError => std::process::ExitCode::from(2),
        }
    }
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
    (camello_fmt::format(&node, &trivia, options), errors)
}
