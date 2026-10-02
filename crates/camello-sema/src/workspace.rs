//! Walking a tree of Perl files, and running a pass over it on every core.
//!
//! Both consumers of the declaration pass do this: `camello check` walks the
//! paths it was pointed at, and `camello lsp` walks the workspace in the
//! background at startup (`docs/lsp.md`, "The index"). It lives here rather
//! than in the command line because the language server cannot reach the
//! command line — the binary depends on the server crate and not the other way
//! round — and two copies of a worker pool is two places for a walk to start
//! following symlinks.

use std::fmt;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::{Match, WalkBuilder};

/// How many workers a run of `items` gets.
#[must_use]
pub fn worker_count(jobs: Option<usize>, items: usize) -> usize {
    jobs.filter(|&jobs| jobs > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
        })
        .min(items)
        .max(1)
}

/// Run `job` over every item, on `jobs` threads, and answer in input order.
///
/// Scoped threads and an atomic cursor: the items are borrowed rather than
/// moved, and the results land in per-item slots so that the order a reader
/// sees is the order they were asked in, whatever order they finished in.
pub fn in_parallel<T, R>(items: &[T], jobs: Option<usize>, job: impl Fn(&T) -> R + Sync) -> Vec<R>
where
    T: Sync,
    R: Send,
{
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let workers = worker_count(jobs, items.len());

    if workers == 1 {
        return items.iter().map(job).collect();
    }

    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    let job = &job;
    let slots = &slots;
    let next = &next;

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else { return };
                let result = job(item);
                *slots[index]
                    .lock()
                    .expect("no worker panics while holding this") = Some(result);
            });
        }
    });

    slots
        .iter()
        .map(|slot| {
            slot.lock()
                .expect("the workers are finished")
                .take()
                .expect("every slot was filled")
        })
        .collect()
}

/// The extensions a directory is walked for when nobody named others: what
/// `camello check` defaults `--extensions` to, and what the language server
/// indexes.
pub const EXTENSIONS: &[&str] = &["pl", "pm", "t", "psgi"];

/// The file a project lists what `camello` is to leave alone in, gitignore
/// syntax, relative to the directory it is in.
pub const IGNORE_FILE: &str = ".camelloignore";

/// Directories that are never source, whatever any ignore file says.
const VCS_DIRS: &[&str] = &[".git", ".hg", ".svn", ".jj"];

/// What a walk leaves out.
///
/// Two kinds of rule, told apart by what happens to a path somebody named.
/// `.gitignore` says what is not the project's to track, which is mostly what
/// a walk should not descend into, but a file named on the command line is a
/// file somebody asked for — so it applies beneath a named path and never to
/// it. `.camelloignore` and `--exclude` say what is not camello's to touch,
/// and naming a file does not change that, so they apply to the named path
/// too and the caller is told why it was left out.
#[derive(Debug, Clone)]
pub struct Ignore {
    /// `--exclude`, relative to the directory the run is in.
    excludes: Gitignore,
    /// Whether `.gitignore` and `.camelloignore` are read at all.
    files: bool,
}

/// Why a path somebody named was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ignored {
    /// A line of a `.camelloignore`.
    File { file: PathBuf, pattern: String },
    /// An `--exclude`.
    Exclude { pattern: String },
}

impl fmt::Display for Ignored {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ignored::File { file, pattern } => {
                write!(f, "`{pattern}` in {}", file.display())
            }
            Ignored::Exclude { pattern } => write!(f, "--exclude `{pattern}`"),
        }
    }
}

impl Default for Ignore {
    /// The ignore files, and nothing excluded: what the language server walks
    /// with.
    fn default() -> Self {
        Ignore {
            excludes: Gitignore::empty(),
            files: true,
        }
    }
}

impl Ignore {
    /// `excludes` are gitignore patterns relative to `base`; `files` is false
    /// for `--no-ignore`, which leaves only them.
    pub fn new(base: &Path, excludes: &[String], files: bool) -> std::io::Result<Self> {
        let mut builder = GitignoreBuilder::new(base);
        for pattern in excludes {
            builder
                .add_line(None, pattern)
                .map_err(|error| std::io::Error::other(format!("--exclude: {error}")))?;
        }
        let excludes = builder
            .build()
            .map_err(|error| std::io::Error::other(format!("--exclude: {error}")))?;
        Ok(Ignore { excludes, files })
    }

    /// Why a path somebody named is left out, or `None` when it is not.
    ///
    /// The `.camelloignore` nearest the path decides, the way a nested
    /// `.gitignore` overrides the one above it; `--exclude` overrides them
    /// all.
    pub fn named(&self, path: &Path) -> std::io::Result<Option<Ignored>> {
        let is_dir = path.is_dir();
        if let Some(relative) = under(self.excludes.path(), path) {
            if let Match::Ignore(glob) =
                self.excludes.matched_path_or_any_parents(&relative, is_dir)
            {
                return Ok(Some(Ignored::Exclude {
                    pattern: glob.original().to_string(),
                }));
            }
        }
        if !self.files {
            return Ok(None);
        }
        let absolute = std::path::absolute(path)?;
        for directory in absolute.ancestors().skip(1) {
            let file = directory.join(IGNORE_FILE);
            if !file.is_file() {
                continue;
            }
            let mut builder = GitignoreBuilder::new(directory);
            if let Some(error) = builder.add(&file) {
                return Err(std::io::Error::other(error.to_string()));
            }
            let rules = builder
                .build()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            match rules.matched_path_or_any_parents(&absolute, is_dir) {
                Match::Ignore(glob) => {
                    // Named the way the path was, as far as it can be.
                    let file = std::env::current_dir()
                        .ok()
                        .and_then(|here| file.strip_prefix(here).ok().map(Path::to_path_buf))
                        .unwrap_or(file);
                    return Ok(Some(Ignored::File {
                        file,
                        pattern: glob.original().to_string(),
                    }));
                }
                Match::Whitelist(_) => return Ok(None),
                Match::None => {}
            }
        }
        Ok(None)
    }
}

/// `path` relative to `base`, when it is under it or already relative.
fn under(base: &Path, path: &Path) -> Option<PathBuf> {
    if path.is_relative() {
        return Some(path.to_path_buf());
    }
    path.strip_prefix(base).ok().map(Path::to_path_buf)
}

/// The Perl files under a path, or the path itself when it names a file.
///
/// Recursive, sorted, and it does not follow a symlink found below a
/// requested root: besides escaping the root, a link to an ancestor would
/// recurse forever. A link the caller named itself is still followed, because
/// naming it is asking for it.
///
/// What `ignore` leaves out is left out (see [`Ignore`]); when that is the
/// named path itself, nothing is collected and the answer says why.
pub fn collect_files(
    path: &Path,
    extensions: &[&str],
    ignore: &Ignore,
    into: &mut Vec<PathBuf>,
) -> std::io::Result<Option<Ignored>> {
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no such file or directory: {}", path.display()),
        ));
    }
    if let Some(why) = ignore.named(path)? {
        return Ok(Some(why));
    }
    if path.is_file() {
        into.push(path.to_path_buf());
        return Ok(None);
    }

    let mut walk = WalkBuilder::new(path);
    walk.standard_filters(false)
        .follow_links(false)
        .sort_by_file_name(Ord::cmp);
    if ignore.files {
        walk.git_ignore(true)
            .git_exclude(true)
            .parents(true)
            .require_git(false)
            .add_custom_ignore_filename(IGNORE_FILE);
    }
    let excludes = ignore.excludes.clone();
    walk.filter_entry(move |entry| {
        let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
        if is_dir && entry.depth() > 0 && VCS_DIRS.iter().any(|name| entry.file_name() == *name) {
            return false;
        }
        !excludes.matched(entry.path(), is_dir).is_ignore()
    });

    for entry in walk.build() {
        let entry = entry.map_err(into_io)?;
        if let Some(error) = entry.error() {
            // A `.gitignore` line git itself would shrug at is not this
            // run's business; a `.camelloignore` one is a rule somebody wrote
            // for camello, and one silently dropped is a file touched that
            // was asked to be left alone.
            if mentions(error, IGNORE_FILE) {
                return Err(std::io::Error::other(error.to_string()));
            }
        }
        let is_file = entry.file_type().is_some_and(|kind| kind.is_file());
        if is_file
            && entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extensions.contains(&extension))
        {
            into.push(entry.into_path());
        }
    }
    Ok(None)
}

fn into_io(error: ignore::Error) -> std::io::Error {
    let message = error.to_string();
    error
        .into_io_error()
        .unwrap_or_else(|| std::io::Error::other(message))
}

/// Whether an ignore-file error is about a file of this name.
fn mentions(error: &ignore::Error, name: &str) -> bool {
    match error {
        ignore::Error::WithPath { path, err } => {
            path.file_name().is_some_and(|file| file == name) || mentions(err, name)
        }
        ignore::Error::Partial(errors) => errors.iter().any(|error| mentions(error, name)),
        ignore::Error::WithLineNumber { err, .. } | ignore::Error::WithDepth { err, .. } => {
            mentions(err, name)
        }
        _ => false,
    }
}
