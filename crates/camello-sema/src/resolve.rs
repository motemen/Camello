//! Finding the file behind a `use` (`docs/typecheck.md`, "Dependencies").
//!
//! Four places, in order, and the order is the whole of the policy:
//!
//! 1. **the roots** — the paths the command was pointed at. Analysed in full.
//! 2. **the stub roots** — `--stubs stubs/`. Declarations, shadowing
//!    everything below: a stub is how a project types the corner of a
//!    dependency that no recogniser can read.
//! 3. **`PERL5LIB` and the `@INC` of the perl on `PATH`** — declarations only.
//!    Asked once per run, by reading a list rather than running the project;
//!    this is the one perl invocation the checker makes, and `--inc` replaces
//!    it.
//! 4. **nowhere** — the package is `Unknown` and every use of it is silent.
//!
//! The declaration pass over a dependency is cached on disk, keyed by the
//! file's path, size, mtime and content hash, so a run over a project with a
//! large `@INC` costs the scan once.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where a module was found, which is what decides how much of it is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Under a project root: analysed in full, and reported against.
    Root,
    /// Under a stub root: declarations, shadowing the real module wholesale.
    Stub,
    /// Somewhere in `@INC`: declarations only.
    Inc,
}

impl Origin {
    #[must_use]
    pub const fn in_roots(self) -> bool {
        matches!(self, Origin::Root)
    }
}

/// The search path a run resolves against.
#[derive(Debug, Default, Clone)]
pub struct Resolver {
    roots: Vec<PathBuf>,
    stubs: Vec<PathBuf>,
    inc: Vec<PathBuf>,
}

impl Resolver {
    #[must_use]
    pub fn new(roots: Vec<PathBuf>, stubs: Vec<PathBuf>, inc: Vec<PathBuf>) -> Self {
        Resolver { roots, stubs, inc }
    }

    /// Where `Foo::Bar` lives, as `Foo/Bar.pm` under each root in turn.
    #[must_use]
    pub fn locate(&self, module: &str) -> Option<(PathBuf, Origin)> {
        let relative = module_path(module)?;
        for (directories, origin) in [
            (&self.roots, Origin::Root),
            (&self.stubs, Origin::Stub),
            (&self.inc, Origin::Inc),
        ] {
            for directory in directories {
                let candidate = directory.join(&relative);
                if candidate.is_file() {
                    return Some((candidate, origin));
                }
            }
        }
        None
    }

    /// Whether a `use` is worth resolving at all.
    ///
    /// A pragma is spelled in lower case by a convention perl itself follows,
    /// it declares no symbols the checker could use, and resolving every
    /// `use strict` would read `strict.pm` once per file in the run.
    #[must_use]
    pub fn worth_resolving(module: &str) -> bool {
        !module.is_empty()
            && module
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase())
    }
}

/// `Foo::Bar` → `Foo/Bar.pm`, or `None` when the name is not one.
#[must_use]
pub fn module_path(module: &str) -> Option<PathBuf> {
    if module.is_empty()
        || !module
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == ':')
    {
        return None;
    }
    let mut path = PathBuf::new();
    for part in module.split("::") {
        if part.is_empty() {
            return None;
        }
        path.push(part);
    }
    Some(path.with_extension("pm"))
}

/// The `@INC` of the perl on `PATH`, plus `PERL5LIB`.
///
/// Reading a list, not running the project. It is asked once per run, and
/// `--inc` is how to ask for a different one — or for none, which is what a
/// hermetic build wants.
#[must_use]
pub fn perl_inc() -> Vec<PathBuf> {
    let Ok(output) = Command::new("perl")
        .arg("-e")
        .arg("print join qq{\\n}, grep { !ref && m{^/} } @INC")
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(PathBuf::from)
        .filter(|path| path.is_dir() && seen.insert(path.clone()))
        .collect()
}

/// The declaration cache (`docs/typecheck.md`, "Dependencies").
///
/// One file per cached module, named by a key over the path, the size, the
/// mtime and a hash of the contents — so a file that was touched and not
/// changed still hits, and one that was changed never does.
pub struct Cache {
    directory: Option<PathBuf>,
}

impl Cache {
    /// A cache under `directory`, or a cache that stores nothing.
    #[must_use]
    pub fn new(directory: Option<PathBuf>) -> Self {
        if let Some(directory) = &directory {
            let _ = std::fs::create_dir_all(directory);
        }
        Cache { directory }
    }

    #[must_use]
    pub fn disabled() -> Self {
        Cache { directory: None }
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.directory.is_some()
    }

    /// The key for a file, from what a stat and a hash can say about it.
    ///
    /// `salt` is what the run reads it *under* — a declaration read under one
    /// dialect is not the one the same bytes give under another.
    /// What the cached shape of `FileDecls` is called.
    ///
    /// Bumped whenever an entry written by an older camello would be read as
    /// a *complete* answer by a newer one. Return inference is the case that
    /// needed it: the serde defaults make an old entry parse, and what it
    /// parses to is a file whose subs are all `Unknown` — which the tiers
    /// would then never revisit, because a cached entry is not walked again.
    ///
    /// **A new recogniser is the same case.** The bytes of a dependency do
    /// not change when camello learns to read them, so the key over them does
    /// not either, and the cached "this package has no framework, no
    /// attributes and no `new`" would outlive the release that fixed it —
    /// `Carton::Dist`, a `Class::Tiny` class, kept reporting `unknown-method`
    /// on its own constructor. Bump this with the recogniser.
    const FORMAT: &'static str = "class-tiny-1";

    #[must_use]
    pub fn key(path: &Path, source: &str, salt: &str) -> String {
        let metadata = std::fs::metadata(path).ok();
        let size = metadata.as_ref().map_or(0, std::fs::Metadata::len);
        let mtime = metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_secs());
        format!(
            "{:016x}-{size}-{mtime}",
            fnv(path.to_string_lossy().as_bytes())
                ^ fnv(source.as_bytes())
                ^ fnv(salt.as_bytes())
                ^ fnv(Cache::FORMAT.as_bytes())
        )
    }

    #[must_use]
    pub fn read(&self, key: &str) -> Option<String> {
        std::fs::read_to_string(self.directory.as_ref()?.join(format!("{key}.json"))).ok()
    }

    pub fn write(&self, key: &str, contents: &str) {
        if let Some(directory) = &self.directory {
            let _ = std::fs::write(directory.join(format!("{key}.json")), contents);
        }
    }
}

/// FNV-1a, which is all this needs: the hash guards against a file changing
/// under a cache entry, not against anyone choosing one to collide.
fn fnv(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_name_becomes_a_path() {
        assert_eq!(
            module_path("Foo::Bar"),
            Some(PathBuf::from("Foo").join("Bar.pm"))
        );
        assert_eq!(module_path("Foo"), Some(PathBuf::from("Foo.pm")));
        assert_eq!(module_path("Foo::"), None);
        assert_eq!(module_path(""), None);
        assert_eq!(module_path("../etc/passwd"), None);
    }

    #[test]
    fn a_pragma_is_not_worth_resolving() {
        assert!(!Resolver::worth_resolving("strict"));
        assert!(!Resolver::worth_resolving("parent"));
        assert!(Resolver::worth_resolving("Foo::Bar"));
    }

    #[test]
    fn the_key_changes_with_the_contents() {
        let path = Path::new("nowhere.pm");
        assert_ne!(Cache::key(path, "a", ""), Cache::key(path, "b", ""));
        assert_eq!(Cache::key(path, "a", ""), Cache::key(path, "a", ""));
        // And by what the file is read under, not only by the file.
        assert_ne!(Cache::key(path, "a", ""), Cache::key(path, "a", "A=B"));
    }
}
