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

pub mod annotate;
pub mod arity;
pub mod decl;
pub mod diag;
pub mod flow;
pub mod interp;
pub mod program;
pub mod resolve;
pub mod scope;
pub mod suppress;
pub mod types;

use std::path::Path;

use camello_syntax::lang::SyntaxNode;

pub use program::Program;

pub use diag::{Code, Diagnostic, LineIndex, Position, Severity};

/// What a run asks for.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Whether the type lattice runs. `lint` says no, `typecheck` says yes.
    pub types: bool,
    /// Report a public sub with no annotation.
    pub strict_annotations: bool,
    /// Codes this project has turned off (`camello.toml`).
    pub disabled: Vec<Code>,
    /// Classes a value is held of for its destructor, beyond the ones
    /// [`scope::GUARD_NAMES`] knows (`camello.toml`).
    pub guard_classes: Vec<String>,
}

impl Options {
    #[must_use]
    pub fn lint() -> Self {
        Options {
            types: false,
            ..Options::default()
        }
    }

    #[must_use]
    pub fn typecheck() -> Self {
        Options {
            types: true,
            ..Options::default()
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
        let mut options = if text.contains("typecheck") {
            Options::typecheck()
        } else {
            Options::lint()
        };
        options.strict_annotations = text.contains("strict-annotations");
        options
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
    resolver: resolve::Resolver,
    cache: Option<resolve::Cache>,
}

impl Analysis {
    #[must_use]
    pub fn new() -> Self {
        Analysis::default()
    }

    /// The search path and the cache a run resolves dependencies against.
    #[must_use]
    pub fn with_resolver(mut self, resolver: resolve::Resolver, cache: resolve::Cache) -> Self {
        self.resolver = resolver;
        self.cache = Some(cache);
        self
    }

    /// What this project's own modules stand in for (`camello.toml`,
    /// `read-as`). Every file the run reads, its dependencies included, is
    /// read under it.
    #[must_use]
    pub fn with_dialect(mut self, dialect: annotate::Dialect) -> Self {
        self.program.set_dialect(dialect);
        self
    }

    /// Follow every `use` out of the files already added, and fold what it
    /// finds into the graph as declarations.
    ///
    /// Transitive, because an ancestor two modules up is what decides whether
    /// "no such method" may be said at all. Bounded by the fact that a module
    /// is read once: the visited set is the program's own `by_path`.
    ///
    /// A dependency is added with `in_roots = false` whatever it resolved to.
    /// No diagnostic is ever reported against a file the command was not
    /// pointed at, and a file that *was* pointed at is already in the graph.
    pub fn resolve_dependencies(&mut self) {
        let mut pending: Vec<String> = self
            .program
            .files()
            .flat_map(|entry| entry.decls.uses.clone())
            .collect();
        let mut seen: std::collections::HashSet<String> = pending.iter().cloned().collect();

        while let Some(module) = pending.pop() {
            if !resolve::Resolver::worth_resolving(&module) {
                continue;
            }
            let Some((path, _origin)) = self.resolver.locate(&module) else {
                continue;
            };
            if self.program.index_of(&path).is_some() {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let decls = self.read_declarations(&path, &source);
            for used in &decls.uses {
                if seen.insert(used.clone()) {
                    pending.push(used.clone());
                }
            }
            self.program.add(&path, decls, false);
        }
    }

    /// A dependency's declarations, off the cache when the file has not
    /// changed since they were written.
    fn read_declarations(&self, path: &std::path::Path, source: &str) -> decl::FileDecls {
        let key = self
            .cache
            .as_ref()
            .filter(|cache| cache.is_enabled())
            .map(|_| resolve::Cache::key(path, source, &self.program.dialect().fingerprint()));
        if let (Some(cache), Some(key)) = (&self.cache, &key) {
            if let Some(text) = cache.read(key) {
                if let Ok(decls) = serde_json::from_str(&text) {
                    return decls;
                }
            }
        }
        let parsed = camello_syntax::parse::parse(source);
        let decls = decl::declare_in(&parsed.syntax(), self.program.dialect());
        if let (Some(cache), Some(key)) = (&self.cache, &key) {
            if let Ok(text) = serde_json::to_string(&decls) {
                cache.write(key, &text);
            }
        }
        decls
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

    /// Close the declaration phase: every file is in, so what one file's type
    /// library declares can be substituted into what another file annotated.
    ///
    /// Called once, between the two phases. Calling it twice is harmless — the
    /// substitution is idempotent, because a name that resolved is no longer a
    /// name.
    pub fn link(&mut self) {
        self.program.link_named_types();
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
        let mut diagnostics = scope::analyse(root, source, &options.guard_classes).diagnostics;
        if let Some(file) = self.program.index_of(path) {
            diagnostics.extend(arity::analyse(root, file, &self.program));
            if options.types {
                diagnostics.extend(flow::analyse(root, file, &self.program));
            }
            // What the declaration pass had to say about this file's
            // annotations. It ran once, over every file; a dependency's
            // diagnostics are read and dropped, because no diagnostic is ever
            // reported against a file outside the roots.
            if let Some(entry) = self.program.file(file) {
                if entry.in_roots {
                    diagnostics.extend(entry.decls.diagnostics.iter().cloned());
                    if options.strict_annotations {
                        diagnostics.extend(unannotated(&entry.decls));
                    }
                }
            }
        }
        if !options.types {
            diagnostics.retain(|diagnostic| !diagnostic.code.needs_types());
        }
        if !options.disabled.is_empty() {
            diagnostics.retain(|diagnostic| !options.disabled.contains(&diagnostic.code));
        }
        // Per-line suppression is read last, so that a marker names a code the
        // run would otherwise have reported.
        let suppressions = suppress::Suppressions::of(root, source);
        if !suppressions.is_empty() {
            let index = LineIndex::new(source);
            diagnostics.retain(|diagnostic| !suppressions.silences(diagnostic, source, &index));
        }
        diagnostics.sort_by_key(|diagnostic| (diagnostic.range.start(), diagnostic.code));
        diagnostics
    }
}

/// Every public sub in a file that says nothing about its own shape.
///
/// "Public" is the language-wide reading of a leading underscore, and
/// "annotated" is a signature, an `args` list or a `Returns:` — anything the
/// checker could have used. Reported at `info`, because it is a thing a user
/// asked to be told rather than a contradiction between two declared things.
fn unannotated(decls: &decl::FileDecls) -> Vec<Diagnostic> {
    decls
        .subs
        .iter()
        .filter(|symbol| {
            symbol.source != decl::SymbolSource::Annotated
                && !symbol.name.starts_with('_')
                && !symbol
                    .name
                    .chars()
                    .all(|ch| ch.is_ascii_uppercase() || ch == '_')
        })
        .map(|symbol| {
            Diagnostic::new(
                Code::MissingAnnotation,
                symbol.range,
                format!(
                    "`{}` is public and says nothing about what it takes or returns",
                    symbol.name
                ),
            )
        })
        .collect()
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
    analysis.link();
    analysis.check(path, &parsed.syntax(), source, options)
}

/// A `TextRange` as two numbers, so that a declaration can be cached.
///
/// rowan's own types know nothing about serde, and the cache is the only
/// place a range crosses a process boundary.
pub(crate) mod serde_range {
    use rowan::{TextRange, TextSize};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(range: &TextRange, into: S) -> Result<S::Ok, S::Error> {
        (u32::from(range.start()), u32::from(range.end())).serialize(into)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(from: D) -> Result<TextRange, D::Error> {
        let (start, end) = <(u32, u32)>::deserialize(from)?;
        Ok(TextRange::new(TextSize::from(start), TextSize::from(end)))
    }
}

#[cfg(test)]
mod tests;
