//! What `camello.toml` says, as the server reads it (`docs/lsp.md`,
//! "Diagnostics").
//!
//! The `[check]` table is the same table `camello check` reads, read by the
//! same code — [`camello_sema::config`] — because the language server is
//! another consumer of the project's configuration and not a new dialect of
//! it. What is added here is only what the command line supplies from flags
//! and the server has to supply from somewhere: where the workspace root is,
//! and what to do when the file does not parse (log it and carry on, rather
//! than exit).

use std::path::{Path, PathBuf};

use camello_fmt::FormatterOptions;
use camello_sema::annotate::Dialect;
use camello_sema::resolve::CACHE_DIR;
use camello_sema::{Options, Severity};

#[derive(Debug, Clone)]
pub struct Settings {
    /// The directory `camello.toml` was looked for in, and what a relative
    /// `lib` or `stubs` is relative to.
    pub root: PathBuf,
    /// What the index walks: the workspace roots, plus `[check] lib`.
    pub roots: Vec<PathBuf>,
    /// `[check] stubs`, which shadow the real modules.
    pub stubs: Vec<PathBuf>,
    pub cache_dir: Option<PathBuf>,
    pub dialect: Dialect,
    pub options: Options,
    /// The quietest severity worth publishing. What it drops is dropped
    /// whole, the way the CLI drops it.
    ///
    /// The default is `info` where the CLI's is `warning`, and that is the
    /// second deliberate divergence (`docs/lsp.md`): what a batch tool has to
    /// weigh is a wall of lines against the few worth reading, and an editor
    /// has no wall — an `info` is a squiggle under the name it is about, in
    /// the file already on screen. A `min-severity` the project wrote down
    /// applies here exactly as it does there.
    pub min_severity: Severity,
    pub formatter: FormatterOptions,
}

impl Settings {
    /// Read the configuration for a workspace, and say what went wrong.
    ///
    /// A `camello.toml` that does not parse is reported to the client and
    /// then ignored: the command line exits on one because a project checked
    /// under rules nobody asked for is worse than no check, and an editor
    /// cannot exit — so it says so in the log and offers the defaults, which
    /// is the same bargain a client makes with any other unreadable setting.
    #[must_use]
    pub fn load(root: &Path, folders: &[PathBuf]) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let config = match camello_sema::config::Config::read(root) {
            Ok(config) => config,
            Err(error) => {
                problems.push(error.to_string());
                camello_sema::config::Config::default()
            }
        };

        let (check, found) = config.check.resolve(root);
        problems.extend(found);
        let options = Options {
            strict_annotations: check.strict_annotations,
            disabled: check.disabled,
            guard_classes: check.guard_classes,
        };

        // The workspace folders first — they are what the editor is open on —
        // and then whatever `[check] lib` names, which is how a project says
        // "this subtree is the program" the way it does for the CLI.
        let mut roots: Vec<PathBuf> = folders.to_vec();
        if roots.is_empty() {
            roots.push(root.to_path_buf());
        }
        roots.extend(check.lib);
        roots.dedup();

        (
            Settings {
                root: root.to_path_buf(),
                roots,
                stubs: check.stubs,
                cache_dir: Some(root.join(CACHE_DIR)),
                dialect: Dialect::new(check.read_as),
                options,
                min_severity: check.min_severity.unwrap_or(Severity::Info),
                // There is no `[format]` table yet, and the layout flags on
                // `camello format` are hidden because their names and
                // defaults may still move (`docs/architecture.md`). So the
                // server formats the way `camello format` with no flags
                // formats, which is the only answer that cannot drift from
                // it.
                formatter: FormatterOptions::default(),
            },
            problems,
        )
    }

    /// A fresh, linked analysis over nothing — what answers requests before
    /// the workspace walk has finished.
    #[must_use]
    pub fn empty_analysis(&self) -> camello_sema::Analysis {
        camello_sema::Analysis::new()
            .with_resolver(
                camello_sema::resolve::Resolver::new(
                    self.roots.clone(),
                    self.stubs.clone(),
                    Vec::new(),
                ),
                cache(self.cache_dir.as_deref()),
            )
            .with_dialect(self.dialect.clone())
    }
}

pub fn cache(directory: Option<&Path>) -> camello_sema::resolve::Cache {
    match directory {
        Some(directory) => camello_sema::resolve::Cache::new(Some(directory.to_path_buf())),
        None => camello_sema::resolve::Cache::disabled(),
    }
}
