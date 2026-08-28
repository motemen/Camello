//! `camello.toml` (`docs/typecheck.md`, "Open questions").
//!
//! At the root the command is run from, and shared with the formatter's
//! options when those become configurable — which is why the table is
//! `[check]`, after the subcommand it configures.
//!
//! It lives here rather than in the command line because the command line is
//! no longer its only reader: `camello lsp` applies the same table under the
//! same rules, and it is another consumer of the configuration rather than a
//! new dialect of it (`docs/lsp.md`, "Diagnostics"). The codes and severities
//! the file names are this crate's vocabulary, so this is where a file that
//! names them can be read once.
//!
//! ```toml
//! [check]
//! lib = ["lib", "t"]
//! stubs = ["stubs"]
//! disable = ["unused-variable"]
//! error-on = "warning"
//! min-severity = "warning"
//! guard-classes = ["My::Lock"]
//! strict-annotations = true
//!
//! [check.read-as]
//! "My::Accessors" = "Class::Accessor::Typed"
//! ```
//!
//! Not `.perlcriticrc`: the codes are camello's and the file is camello's.
//! Every field is optional, and a flag on the command line wins over it —
//! the file says what the project is, and the flag says what this run is.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const FILE_NAME: &str = "camello.toml";

/// A `camello.toml` that does not parse, named with the path it was read
/// from.
///
/// Its own type rather than a `miette::Report`, because this crate is a
/// library and two callers render an error differently: the command line
/// prints it and stops, the language server logs it and carries on with the
/// defaults.
#[derive(Debug)]
pub struct Error {
    pub path: PathBuf,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub check: Check,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Check {
    /// Directories to add to the roots, so that `camello check` with no
    /// paths knows what the project is.
    #[serde(default)]
    pub lib: Vec<PathBuf>,
    /// Directories of stub modules.
    #[serde(default)]
    pub stubs: Vec<PathBuf>,
    /// Codes this project has turned off.
    #[serde(default)]
    pub disable: Vec<String>,
    /// The severity that makes a run fail.
    pub error_on: Option<String>,
    /// The quietest severity worth printing.
    pub min_severity: Option<String>,
    /// Classes this project holds a value of for its destructor, on top of the
    /// ones the checker already knows.
    #[serde(default)]
    pub guard_classes: Vec<String>,
    /// Report a public sub with no annotation.
    #[serde(default)]
    pub strict_annotations: bool,
    /// A module of this project's own, and the module whose interface it
    /// re-exports. `use My::Accessors` is then read the way `use
    /// Class::Accessor::Typed` is: recognition is by an import that could
    /// have provided the name, and a wrapper is what took that import away.
    #[serde(default)]
    pub read_as: BTreeMap<String, String>,
}

impl Config {
    /// Read `camello.toml` from a directory, or the default when there is none.
    ///
    /// A file that does not parse is an error rather than a shrug: a config
    /// silently ignored is a project checked under rules nobody asked for.
    pub fn read(directory: &Path) -> Result<Self, Error> {
        let path = directory.join(FILE_NAME);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(Config::default());
        };
        toml::from_str(&text).map_err(|error| Error {
            path,
            message: error.to_string(),
        })
    }
}
