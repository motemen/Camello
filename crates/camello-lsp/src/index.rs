//! The workspace index (`docs/lsp.md`, "The index").
//!
//! At `initialize` the server walks the workspace roots — plus whatever
//! `[check] lib` and `[check] stubs` name — and runs the **declaration pass
//! only** over every Perl file it finds, through the same on-disk cache
//! `camello check` uses. A repository that has ever been checked therefore
//! warm-starts.
//!
//! What is retained is `FileDecls` and nothing else: packages, subs with their
//! name ranges, imports, facts. Serde-sized data, never a tree and never the
//! source text. Trees exist for open files, in the document store, and
//! transiently inside a body pass. That is what makes thousands of files a
//! memory bill somebody can state.
//!
//! Open files never queue behind the walk. Before it finishes, requests are
//! answered from single-file analysis — lexical diagnostics are exact,
//! cross-file answers are absent — and when it finishes the graph answers
//! instead. The flag that says which is [`Index::ready`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use camello_sema::decl::FileDecls;
use camello_sema::Analysis;

use crate::settings::{Settings, EXTENSIONS};

/// The program graph, and whether it is the whole of one yet.
pub struct Index {
    pub analysis: Analysis,
    /// The declaration fingerprint each file was last *installed* under.
    ///
    /// A memo of what the graph was told, and it has to be a memo: [`link`]
    /// rewrites the stored declarations in place — a named type that resolved
    /// is no longer the name it was written as — so the graph cannot be asked
    /// afterwards what it was given, and the answer it would give differs from
    /// what the next edit computes.
    ///
    /// It lives here, beside the graph it describes, rather than beside the
    /// documents: a memo that outlives its graph is one that says "unchanged"
    /// about a file that changed under it, and the background walk swaps a
    /// whole new graph in. Held together, they cannot come apart.
    ///
    /// [`link`]: camello_sema::Analysis::link
    fingerprints: HashMap<PathBuf, String>,
    /// Whether the background walk has finished. Before it has, the graph
    /// holds whatever single files have been opened and nothing else, and a
    /// cross-file question has no answer rather than a wrong one.
    pub ready: bool,
    /// How many files the walk read, for the log line that says so.
    pub files: usize,
}

impl Index {
    #[must_use]
    pub fn empty(settings: &Settings) -> Self {
        Index {
            analysis: settings.empty_analysis(),
            fingerprints: HashMap::new(),
            ready: false,
            files: 0,
        }
    }

    /// Whether the graph can answer about this path at all.
    #[must_use]
    pub fn holds(&self, path: &Path) -> bool {
        self.analysis.program().index_of(path).is_some()
    }

    /// Install one file's declarations, and say whether they were news.
    ///
    /// The only way declarations enter the graph after the walk, so that the
    /// memo cannot be updated by some paths and not others — which is the
    /// whole of the bug this replaced (`docs/lsp.md`, "Incremental
    /// reanalysis", step 3).
    ///
    /// Unchanged declarations are not installed at all, and that is not only
    /// an economy: [`Program::replace`] rebuilds the name indexes over every
    /// file, and it would put back the *unlinked* declarations that
    /// [`Analysis::link`] had already resolved.
    ///
    /// A caller that gets `true` owes the graph a `link` — batched, because
    /// linking walks every file and a watched-file event may carry many.
    ///
    /// [`Program::replace`]: camello_sema::program::Program::replace
    /// [`Analysis::link`]: camello_sema::Analysis::link
    pub fn install(&mut self, path: &Path, decls: FileDecls) -> bool {
        let fingerprint = fingerprint(&decls);
        if self
            .fingerprints
            .get(path)
            .is_some_and(|held| held == &fingerprint)
        {
            return false;
        }
        self.analysis.replace(path, decls, true);
        self.fingerprints.insert(path.to_path_buf(), fingerprint);
        true
    }
}

/// Walk the workspace and build the graph. Blocking, and meant for a thread of
/// its own.
#[must_use]
pub fn build(settings: &Settings) -> Index {
    let files = walk(settings);
    let dialect = settings.dialect.clone();
    let cache = crate::settings::cache(settings.cache_dir.as_deref());

    // The declaration pass over every file, on every core — the same pass
    // `camello check` runs, through the same cache key, so the two share
    // their warm entries.
    let declared = camello_sema::workspace::in_parallel(&files, None, |path| {
        let source = std::fs::read_to_string(path).ok()?;
        Some(declarations(path, &source, &dialect, &cache))
    });

    let mut analysis = Analysis::new()
        .with_resolver(
            camello_sema::resolve::Resolver::new(
                settings.roots.clone(),
                settings.stubs.clone(),
                camello_sema::resolve::perl_inc(),
            ),
            crate::settings::cache(settings.cache_dir.as_deref()),
        )
        .with_dialect(settings.dialect.clone());
    let mut read = 0usize;
    let mut fingerprints = HashMap::new();
    for (path, decls) in files.iter().zip(declared) {
        if let Some(decls) = decls {
            fingerprints.insert(path.clone(), fingerprint(&decls));
            analysis.add(path, decls, true);
            read += 1;
        }
    }
    // Everything a root file `use`s, transitively, as declarations — an
    // ancestor two modules away is what decides whether "no such method" may
    // be said at all.
    analysis.resolve_dependencies();
    analysis.link();
    // The returns a single file could not see (`docs/return-inference.md`,
    // "Tier 2"). The roots are the workspace files: a dependency contributes
    // whatever tier 1 read off it inside the declaration pass and no more.
    analysis.infer_returns(&files, None, |path| std::fs::read_to_string(path).ok());

    Index {
        analysis,
        fingerprints,
        ready: true,
        files: read,
    }
}

/// One file's declarations, off the cache when the file has not changed.
///
/// The same key `camello check` writes under: path, size, mtime, content hash
/// and the dialect fingerprint. Nothing new is persisted — the design's
/// decision not to grow a project index until a cold start is measured and
/// found wanting (`docs/lsp.md`, non-goals).
pub fn declarations(
    path: &Path,
    source: &str,
    dialect: &camello_sema::annotate::Dialect,
    cache: &camello_sema::resolve::Cache,
) -> FileDecls {
    let key = cache
        .is_enabled()
        .then(|| camello_sema::resolve::Cache::key(path, source, &dialect.fingerprint()));
    if let Some(key) = &key {
        if let Some(text) = cache.read(key) {
            if let Ok(decls) = serde_json::from_str(&text) {
                return decls;
            }
        }
    }
    let parsed = camello_syntax::parse::parse(source);
    let decls = camello_sema::decl::declare_in(&parsed.syntax(), dialect);
    if let Some(key) = &key {
        if let Ok(text) = serde_json::to_string(&decls) {
            cache.write(key, &text);
        }
    }
    decls
}

/// Every Perl file under the roots, each named once.
fn walk(settings: &Settings) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in settings.roots.iter().chain(settings.stubs.iter()) {
        // A root that does not exist is not an error here: `[check] lib` may
        // name a directory this checkout does not have, and an editor that
        // refused to start over it would be answering a question nobody
        // asked.
        let _ = camello_sema::workspace::collect_files(root, EXTENSIONS, &mut files);
    }
    files.sort();
    files.dedup();
    files
}

/// A file's declarations as one comparable string (`docs/lsp.md`,
/// "Incremental reanalysis", step 3).
///
/// The decl-diff asks one question — did this edit change what another file
/// can see — and a fingerprint answers it without `FileDecls` having to grow
/// an equality nobody else wants. What goes in is what crosses a file
/// boundary: the packages, what they inherit and declare, and every sub's
/// name and shape. What stays out is anything positional: moving a sub down a
/// line changes no other file's analysis, and an index that thought otherwise
/// would resweep the open files on every newline.
#[must_use]
pub fn fingerprint(decls: &FileDecls) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (_, name) in &decls.packages {
        parts.push(format!("package {name}"));
    }
    for symbol in &decls.subs {
        parts.push(format!(
            "sub {}::{} {}",
            symbol.package,
            symbol.name,
            camello_sema::decl::signature_of(symbol)
        ));
    }
    for facts in &decls.facts {
        parts.push(format!(
            "facts {} isa=[{}] roles=[{}] ctor={} dynamic={} exports=[{}]",
            facts.name,
            facts.isa.join(","),
            facts.roles.join(","),
            facts.constructor,
            facts.dynamic,
            facts.exports.join(","),
        ));
        for attribute in &facts.attributes {
            parts.push(format!(
                "attr {}::{} {} [{}]",
                facts.name,
                attribute.name,
                attribute.ty,
                attribute
                    .methods
                    .iter()
                    .map(|method| method.name.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        for named in &facts.types {
            parts.push(format!("type {} = {}", named.name, named.ty));
        }
    }
    for (name, from) in &decls.imports {
        parts.push(format!("import {name} from {from}"));
    }
    let mut uses = decls.uses.clone();
    uses.sort();
    parts.push(format!("uses {}", uses.join(",")));
    parts.sort();
    parts.join("\n")
}
