//! `camello.toml`, as the command line reads it.
//!
//! The table and its fields live in [`camello_sema::config`], because
//! `camello lsp` reads the same file under the same rules and a second reader
//! would be a second dialect (`docs/lsp.md`, "Diagnostics"). What is left here
//! is the one thing that is the command line's own: a file that does not parse
//! is a `miette` report, printed and fatal, rather than something to carry on
//! past.

use std::path::Path;

use miette::Result;

pub use camello_sema::config::{Check, Config, FILE_NAME};

/// Read `camello.toml` from a directory, or the default when there is none.
///
/// A file that does not parse is an error rather than a shrug: a config
/// silently ignored is a project checked under rules nobody asked for.
pub fn read(directory: &Path) -> Result<Config> {
    Config::read(directory).map_err(|error| miette::miette!("{error}"))
}
