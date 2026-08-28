//! The corpus bars (`docs/lsp.md`, "Testing").
//!
//! Fixtures exercise written-down cases; a bar is a number taken over real
//! code, printed so that a regression is visible rather than argued about. Two
//! of them:
//!
//! * **the index** — walk a corpus, run the declaration pass over it, and say
//!   how many files, how long, and how much memory the residency cost. The
//!   design *assumes* `FileDecls`-only residency fits a large repository
//!   comfortably; this is what turns the assumption into a number.
//! * **the edit loop** — N edits to one file, decl-diff clean, timed. That is
//!   the common case of "Incremental reanalysis" step 4, and it is what the
//!   300 ms debounce should be compared against.
//!
//! The questions are the binary's, the corpus is the script's — the same split
//! `scripts/corpus-check` already makes.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::index::{self, Index};
use crate::settings::Settings;

/// What indexing a corpus cost.
pub struct IndexBar {
    pub files: usize,
    pub packages: usize,
    pub subs: usize,
    pub elapsed: Duration,
    /// Peak resident size in kilobytes, where the platform will say.
    pub peak_rss_kb: Option<u64>,
}

/// Walk these roots and build the graph, timing it.
#[must_use]
pub fn index_bar(roots: Vec<PathBuf>, cache: bool) -> (Index, IndexBar) {
    let root = roots.first().cloned().unwrap_or_default();
    let (mut settings, _) = Settings::load(&root, &roots);
    settings.roots = roots;
    if !cache {
        // A cold start is the measurement worth having first: a warm cache
        // measures the disk, not the pass.
        settings.cache_dir = None;
    }

    let started = Instant::now();
    let built = index::build(&settings);
    let elapsed = started.elapsed();

    let program = built.analysis.program();
    let bar = IndexBar {
        files: program.files().count(),
        packages: program
            .files()
            .map(|entry| entry.decls.packages.len())
            .sum(),
        subs: program.files().map(|entry| entry.decls.subs.len()).sum(),
        elapsed,
        peak_rss_kb: peak_rss_kb(),
    };
    (built, bar)
}

/// What N edits to one file cost.
pub struct EditBar {
    pub edits: usize,
    pub elapsed: Duration,
    /// How many of them the decl-diff called a declaration change. A trailing
    /// comment changes none, so anything but zero is the fingerprint claiming
    /// a change that is not there — which would make every keystroke sweep
    /// every open file.
    pub declaration_changes: usize,
}

/// Retype one file N times and time the loop.
///
/// The edit is a trailing comment. That is not the most realistic keystroke,
/// but it is the one that is *definitionally* decl-diff clean on any file in
/// any corpus, which is what makes the number comparable across runs — and the
/// work it measures is the whole of the loop either way: reparse, declaration
/// pass, fingerprint, body pass.
#[must_use]
pub fn edit_bar(index: &Index, path: &Path, edits: usize) -> Option<EditBar> {
    let source = std::fs::read_to_string(path).ok()?;
    let settings = {
        let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let (mut settings, _) = Settings::load(&root, std::slice::from_ref(&root));
        settings.cache_dir = None;
        settings
    };
    let cache = camello_sema::resolve::Cache::disabled();

    let mut declaration_changes = 0;
    let mut previous: Option<String> = None;
    let started = Instant::now();
    for edit in 0..edits {
        let text = format!("{source}\n# camello edit {edit}\n");
        let document = crate::document::Document::new(
            Some(path.to_path_buf()),
            &text,
            i32::try_from(edit).unwrap_or(i32::MAX),
            crate::position::Encoding::Utf16,
        );
        let decls = index::declarations(path, &text, &settings.dialect, &cache);
        let fingerprint = index::fingerprint(&decls);
        if previous.as_deref().is_some_and(|held| held != fingerprint) {
            declaration_changes += 1;
        }
        previous = Some(fingerprint);
        let context = crate::analysis::context(&document, index, &settings);
        let _ = crate::analysis::analyse(&document, &context, &settings, true);
    }
    Some(EditBar {
        edits,
        elapsed: started.elapsed(),
        declaration_changes,
    })
}

/// The high-water resident size, where the platform keeps one.
///
/// Linux does, in `/proc/self/status`. Elsewhere the bar prints the files and
/// the time and says nothing about memory, which is better than printing a
/// number nobody can compare.
fn peak_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.split_whitespace().next()?.parse().ok())
}
