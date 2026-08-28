//! The checker: `camello check` over the CST that `camello-syntax` produces
//! (`docs/typecheck.md`).
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

pub mod annotate;
pub mod arity;
pub mod config;
pub mod decl;
pub mod diag;
pub mod flow;
pub mod interp;
pub mod program;
pub mod resolve;
pub mod scope;
pub mod suppress;
pub mod types;
pub mod workspace;

use std::path::Path;

use camello_syntax::lang::SyntaxNode;

pub use program::Program;

pub use diag::{Code, Diagnostic, LineIndex, Position, Severity};

/// A sub whose written `Returns:` and whose body do not say the same thing.
#[derive(Debug, Clone)]
pub struct Drift {
    pub file: usize,
    pub package: String,
    pub name: String,
    pub range: rowan::TextRange,
    /// What the comment says.
    pub written: annotate::Returns,
    /// What the return walk read off the body.
    pub body: annotate::Returns,
}

/// Whether a written `Returns:` and a body's own answer disagree.
///
/// Only a half the annotation actually claims is compared: a scalar-only
/// `Returns:` says nothing about list context, and a list the body inferred
/// beside it is an addition rather than a disagreement. A half the *body*
/// could not read is not a disagreement either — `Unknown` is the walk saying
/// it has nothing, which is never evidence against something written down.
fn drifts(written: &annotate::Returns, body: &annotate::Returns) -> bool {
    let scalar =
        !written.scalar.is_unknown() && !body.scalar.is_unknown() && written.scalar != body.scalar;
    let list = written.list != annotate::ListShape::Unknown
        && body.list != annotate::ListShape::Unknown
        && written.list != body.list;
    scalar || list
}

/// What a run asks for.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Report a public sub with no annotation.
    pub strict_annotations: bool,
    /// Codes this project has turned off (`camello.toml`).
    pub disabled: Vec<Code>,
    /// Classes a value is held of for its destructor, beyond the ones
    /// [`scope::GUARD_NAMES`] knows (`camello.toml`).
    pub guard_classes: Vec<String>,
}

impl Options {
    /// What a fixture asks for, read from where it lives, so that the one
    /// setting a fixture can ask for needs no marker inside the file that the
    /// formatter would have to preserve.
    #[must_use]
    pub fn for_fixture(path: &Path) -> Self {
        Options {
            strict_annotations: path.to_string_lossy().contains("strict-annotations"),
            ..Options::default()
        }
    }
}

/// How many rounds tier 2 gives a program (`docs/return-inference.md`,
/// "Tier 2").
///
/// Termination is the number of subs, and the rounds a program needs are the
/// depth of its longest cross-file chain of unannotated subs. The cap is what
/// bounds the cost when that chain is deeper than anyone expected: what it
/// cuts off stays `Unknown`, which is silent.
const PROGRAM_ROUNDS: usize = 6;

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

    /// Install a file's declarations over the ones already in the graph
    /// (`docs/lsp.md`, "Incremental reanalysis").
    pub fn replace(&mut self, path: &Path, decls: decl::FileDecls, in_roots: bool) -> usize {
        self.program.replace(path, decls, in_roots)
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

    /// Fill in the returns no single file could see
    /// (`docs/return-inference.md`, "Tier 2").
    ///
    /// Tier 1 ran inside the declaration pass, where a call into another file
    /// is `Unknown` because the other file is not in yet. This is the same
    /// walk against the whole graph, in rounds over the files that still have
    /// something unresolved, and it is monotone: a sub becomes known at most
    /// once, and its type is final when it does, because it was computed from
    /// callees that were themselves final.
    ///
    /// A recursive or mutually recursive sub whose every path goes through
    /// the recursion stays `Unknown` in every round, which is the cut the
    /// design asked for without a call graph having to be built to find it.
    ///
    /// `read` rather than the text itself: a round parses only the files with
    /// something left to resolve, and holding a workspace's sources in memory
    /// to save the re-reads would cost more than the re-reads do.
    pub fn infer_returns(
        &mut self,
        sources: &[std::path::PathBuf],
        jobs: Option<usize>,
        read: impl Fn(&Path) -> Option<String> + Sync,
    ) {
        let files: Vec<usize> = sources
            .iter()
            .filter_map(|path| self.program.index_of(path))
            .collect();
        for _ in 0..PROGRAM_ROUNDS {
            let pending: Vec<usize> = files
                .iter()
                .copied()
                .filter(|file| !self.program.unresolved_returns(*file).is_empty())
                .collect();
            if pending.is_empty() {
                return;
            }
            let program = &self.program;
            let found = workspace::in_parallel(&pending, jobs, |file| {
                let only = program.unresolved_returns(*file);
                let path = program.file(*file).map(|entry| entry.path.clone())?;
                let source = read(&path)?;
                let parsed = camello_syntax::parse::parse(&source);
                Some(flow::infer_returns(&parsed.syntax(), *file, program, &only))
            });
            let mut installed = false;
            for (file, results) in pending.iter().zip(found) {
                for (index, returns) in results.into_iter().flatten() {
                    self.program.set_returns(*file, index, returns);
                    installed = true;
                }
            }
            if !installed {
                return;
            }
        }
    }

    /// Where a written `Returns:` and the body it sits on do not agree
    /// (`docs/return-inference.md`, "Drift").
    ///
    /// An annotation wins at every call site (ANNOT-7a), so the only thing
    /// that ever compares it against the code is `return-mismatch`, which
    /// looks at one `return` at a time. That misses the drift a file collects:
    /// an annotation that was right when it was written and has been widened,
    /// narrowed or contradicted by the body since. This asks the walk what the
    /// body says and puts the two side by side.
    ///
    /// Nothing is installed and the program is not changed, so the answer for
    /// one sub is computed against the annotations every *other* sub still
    /// carries — which is what makes it a report on the annotation rather
    /// than on the whole file at once.
    #[must_use]
    pub fn returns_drift(
        &self,
        sources: &[std::path::PathBuf],
        jobs: Option<usize>,
        read: impl Fn(&Path) -> Option<String> + Sync,
    ) -> Vec<Drift> {
        let files: Vec<usize> = sources
            .iter()
            .filter_map(|path| self.program.index_of(path))
            .collect();
        let wanted: Vec<usize> = files
            .iter()
            .copied()
            .filter(|file| !self.program.written_returns(*file).is_empty())
            .collect();
        let program = &self.program;
        let found = workspace::in_parallel(&wanted, jobs, |file| {
            let only = program.written_returns(*file);
            let path = program.file(*file).map(|entry| entry.path.clone())?;
            let source = read(&path)?;
            let parsed = camello_syntax::parse::parse(&source);
            Some(flow::infer_returns(&parsed.syntax(), *file, program, &only))
        });
        let mut drifted = Vec::new();
        for (file, results) in wanted.iter().zip(found) {
            for (index, body) in results.into_iter().flatten() {
                let Some(symbol) = self
                    .program
                    .file(*file)
                    .and_then(|entry| entry.decls.subs.get(index))
                else {
                    continue;
                };
                if !drifts(&symbol.returns, &body) {
                    continue;
                }
                drifted.push(Drift {
                    file: *file,
                    package: symbol.package.clone(),
                    name: symbol.name.clone(),
                    range: symbol.range,
                    written: symbol.returns.clone(),
                    body,
                });
            }
        }
        drifted
    }

    /// Step 4′ (`docs/return-inference.md`, "What changes for the incremental
    /// loop"): re-derive one file's inferred returns, and say whether the
    /// graph now holds anything different.
    ///
    /// A body edit that keeps tier 1's answer but changes what tier 2 would
    /// say — `return $self->load` edited to `return $self->parse`, both
    /// cross-file — is invisible to the declaration fingerprint, and the
    /// callers in other open files would go on seeing the old type.
    ///
    /// Every inferable sub is put back to `Unknown` first, because a monotone
    /// round only ever looks at what is still unresolved and the stale answer
    /// is not. Nothing is lost by that: what tier 1 could see inside the file,
    /// the whole program can see too.
    pub fn reinfer_returns(&mut self, path: &Path, source: &str) -> bool {
        let Some(file) = self.program.index_of(path) else {
            return false;
        };
        let candidates = self.program.inferable_returns(file);
        if candidates.is_empty() {
            return false;
        }
        let before: Vec<annotate::Returns> = candidates
            .iter()
            .map(|index| self.program.returns_at(file, *index))
            .collect();
        for index in &candidates {
            self.program
                .set_returns(file, *index, annotate::Returns::default());
        }
        self.infer_returns(&[path.to_path_buf()], Some(1), |_| Some(source.to_string()));
        let after: Vec<annotate::Returns> = candidates
            .iter()
            .map(|index| self.program.returns_at(file, *index))
            .collect();
        before != after
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
        self.analyse_file(path, root, source, options, false)
            .diagnostics
    }

    /// The same, keeping the tables the passes built on the way
    /// (`docs/lsp.md`, "What sema must newly expose").
    ///
    /// `record` is what an editor asks for and the CLI does not: the scope
    /// resolution comes out either way — the pass computes it and the only
    /// question was whether anything kept it — while the type side-table
    /// costs a clone per typed expression and is built only when asked.
    #[must_use]
    pub fn analyse_file(
        &self,
        path: &Path,
        root: &SyntaxNode,
        source: &str,
        options: &Options,
        record: bool,
    ) -> FileAnalysis {
        let mut scope = scope::analyse(root, source, &options.guard_classes);
        let mut types = flow::TypeTable::default();
        // Moved rather than copied: what comes back is the merged, filtered
        // list, and a second copy inside the scope report would be the same
        // diagnostics under different rules.
        let mut diagnostics = std::mem::take(&mut scope.diagnostics);
        if let Some(file) = self.program.index_of(path) {
            diagnostics.extend(arity::analyse(root, file, &self.program));
            let guards = if record {
                let (found, table, guards) = flow::analyse_recording(root, file, &self.program);
                diagnostics.extend(found);
                types = table;
                guards
            } else {
                let (found, guards) = flow::analyse(root, file, &self.program);
                diagnostics.extend(found);
                guards
            };
            // A value held for its destructor is bound so that the destructor
            // runs, and never reading it is the point (`docs/types.md`,
            // DIAG-12d). The scope pass names what is never read and has no
            // types to decide this with, so the body pass decides and its
            // answer is applied here.
            if !guards.is_empty() {
                diagnostics.retain(|diagnostic| {
                    diagnostic.code != Code::UnusedVariable || !guards.contains(&diagnostic.range)
                });
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
        FileAnalysis {
            diagnostics,
            types,
            scope,
            file: self.program.index_of(path),
        }
    }
}

/// One file's answers, and the tables behind them.
///
/// `camello check` reads the diagnostics and drops the rest; an editor reads
/// all three, because hover and completion are questions about what the
/// checker knew rather than about what it complained of.
pub struct FileAnalysis {
    pub diagnostics: Vec<Diagnostic>,
    /// Empty unless the caller asked for the recording pass.
    pub types: flow::TypeTable,
    pub scope: scope::ScopeReport,
    /// Where the file sits in the program graph, or `None` when the graph
    /// does not hold it — which is what single-file mode looks like before
    /// the workspace index has finished.
    pub file: Option<usize>,
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
