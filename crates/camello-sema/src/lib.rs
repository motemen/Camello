//! The checker: `camello lint` and `camello typecheck` over the CST that
//! `camello-syntax` produces (`docs/typecheck.md`).
//!
//! Perl has no static types, so almost every answer here is a derivation from
//! evidence — a sigil, a literal, a constructor call, an annotation on the
//! callee — and the rule that keeps it useful is that it is *silent when it
//! does not know*. A program with no annotations and no recognisable
//! constructors gets no type diagnostics at all, and that is correct behaviour
//! rather than a gap.
//!
//! The scope diagnostics are the exception and are sound within a file: `my`
//! is a declaration and `use strict` makes an undeclared name an error.
//!
//! Two subcommands, one analysis: `lint` is what needs no type lattice, and
//! `typecheck` is everything `lint` reports plus what the lattice adds.
//! [`Code::needs_types`] is the line between them.

pub mod arity;
pub mod decl;
pub mod diag;
pub mod interp;
pub mod program;
pub mod scope;

use std::path::Path;

use camello_syntax::lang::SyntaxNode;

pub use program::Program;

pub use diag::{Code, Diagnostic, LineIndex, Position, Severity};

/// The codes that are implemented and therefore owe a fixture.
///
/// It grows a milestone at a time, and `every_code_has_a_fixture` is what
/// keeps it honest: a code added to [`Code`] and to this list without a
/// fixture is a failing test.
pub const COVERED_CODES: &[Code] = &[
    Code::UndeclaredVariable,
    Code::UnusedVariable,
    Code::ShadowedVariable,
    Code::Arity,
];

/// What a run asks for.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Whether the type lattice runs. `lint` says no, `typecheck` says yes.
    pub types: bool,
    /// Report a public sub with no annotation.
    pub strict_annotations: bool,
}

impl Options {
    #[must_use]
    pub fn lint() -> Self {
        Options {
            types: false,
            strict_annotations: false,
        }
    }

    #[must_use]
    pub fn typecheck() -> Self {
        Options {
            types: true,
            strict_annotations: false,
        }
    }

    /// What a fixture asks for, read from where it lives: a fixture under
    /// `fixtures/typecheck/` is a `typecheck` fixture and one under
    /// `fixtures/lint/` is a `lint` fixture, so the two commands are covered
    /// without a marker inside the file that the formatter would have to
    /// preserve.
    #[must_use]
    pub fn for_fixture(path: &Path) -> Self {
        let text = path.to_string_lossy();
        if text.contains("typecheck") {
            Options::typecheck()
        } else {
            Options::lint()
        }
    }
}

/// A run: the program graph, and the files it was built from.
///
/// Two phases, and the split is the design's (`docs/typecheck.md`, "Data
/// flow"): [`Analysis::declare`] over every file first, so that a call site in
/// the first file can see a sub declared in the last, and [`Analysis::check`]
/// per file afterwards.
#[derive(Default)]
pub struct Analysis {
    program: Program,
}

impl Analysis {
    #[must_use]
    pub fn new() -> Self {
        Analysis::default()
    }

    /// Fold one file's declarations into the graph.
    ///
    /// `in_roots` says whether a diagnostic may be reported against it: a
    /// dependency contributes declarations and is never reported on.
    pub fn declare(&mut self, path: &Path, root: &SyntaxNode, in_roots: bool) -> usize {
        self.program.add_file(path, root, in_roots)
    }

    /// The same, for declarations another thread read.
    pub fn add(&mut self, path: &Path, decls: decl::FileDecls, in_roots: bool) -> usize {
        self.program.add(path, decls, in_roots)
    }

    #[must_use]
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Everything the checker has to say about one file.
    ///
    /// Parsing is the caller's, because a caller that formats and checks the
    /// same file should not parse it twice.
    #[must_use]
    pub fn check(
        &self,
        path: &Path,
        root: &SyntaxNode,
        source: &str,
        options: &Options,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = scope::analyse(root, source).diagnostics;
        if let Some(file) = self.program.index_of(path) {
            diagnostics.extend(arity::analyse(root, file, &self.program));
        }
        if !options.types {
            diagnostics.retain(|diagnostic| !diagnostic.code.needs_types());
        }
        diagnostics.sort_by_key(|diagnostic| (diagnostic.range.start(), diagnostic.code));
        diagnostics
    }
}

/// Parse and check one file on its own, for a caller that has only the text.
#[must_use]
pub fn check_source(source: &str, options: &Options) -> Vec<Diagnostic> {
    check_file(Path::new("<source>"), source, options)
}

/// Parse and check one file, naming it so that its declarations are its own.
#[must_use]
pub fn check_file(path: &Path, source: &str, options: &Options) -> Vec<Diagnostic> {
    let parsed = camello_syntax::parse::parse(source);
    let mut analysis = Analysis::new();
    analysis.declare(path, &parsed.syntax(), true);
    analysis.check(path, &parsed.syntax(), source, options)
}

#[cfg(test)]
mod tests;
