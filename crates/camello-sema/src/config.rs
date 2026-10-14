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

use crate::diag::{Code, Severity};

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

/// A `[check]` table read into camello's vocabulary: codes and severities
/// rather than their spellings, and paths that no longer depend on where the
/// reader stands.
///
/// What a field falls back to when the file says nothing is left to the
/// reader — the command line prints from `warning` up, the language server
/// from `info` — and so is what to do with a problem: the command line stops,
/// the server logs it and carries on without that field.
#[derive(Debug, Default)]
pub struct Resolved {
    pub lib: Vec<PathBuf>,
    pub stubs: Vec<PathBuf>,
    pub disabled: Vec<Code>,
    pub error_on: Option<Severity>,
    pub min_severity: Option<Severity>,
    pub guard_classes: Vec<String>,
    pub strict_annotations: bool,
    pub read_as: BTreeMap<String, String>,
}

impl Check {
    /// Read the table, with `lib` and `stubs` relative to `base` — the
    /// directory the file was read from — and every value it could not read
    /// said as a sentence naming the field.
    #[must_use]
    pub fn resolve(&self, base: &Path) -> (Resolved, Vec<String>) {
        let mut problems = Vec::new();
        let mut disabled = Vec::new();
        for name in &self.disable {
            match Code::parse(name) {
                Some(code) => disabled.push(code),
                None => problems.push(format!("unknown diagnostic code `{name}` in {FILE_NAME}")),
            }
        }
        let mut severity = |field: &str, value: &Option<String>| {
            let name = value.as_deref()?;
            let parsed = Severity::parse(name);
            if parsed.is_none() {
                problems.push(format!(
                    "`{field}` in {FILE_NAME} takes `error`, `warning` or `info`, not `{name}`"
                ));
            }
            parsed
        };
        let error_on = severity("error-on", &self.error_on);
        let min_severity = severity("min-severity", &self.min_severity);
        let resolved = Resolved {
            lib: self
                .lib
                .iter()
                .map(|path| relative_to(base, path))
                .collect(),
            stubs: self
                .stubs
                .iter()
                .map(|path| relative_to(base, path))
                .collect(),
            disabled,
            error_on,
            min_severity,
            guard_classes: self.guard_classes.clone(),
            strict_annotations: self.strict_annotations,
            read_as: self.read_as.clone(),
        };
        (resolved, problems)
    }
}

/// `path` read from a file in `base`. A file in the working directory
/// leaves it as written, so that what is reported under it reads the way the
/// user wrote it.
fn relative_to(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() || base.as_os_str().is_empty() || base == Path::new(".") {
        path.to_path_buf()
    } else {
        base.join(path)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn check(text: &str) -> Check {
        toml::from_str::<Config>(text).expect("a table").check
    }

    #[test]
    fn paths_are_read_from_where_the_file_is() {
        let table = check("[check]\nlib = [\"lib\", \"/abs\"]\nstubs = [\"stubs\"]\n");
        let (resolved, problems) = table.resolve(Path::new("proj"));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            resolved.lib,
            [PathBuf::from("proj/lib"), PathBuf::from("/abs")]
        );
        assert_eq!(resolved.stubs, [PathBuf::from("proj/stubs")]);
        let (here, _) = table.resolve(Path::new("."));
        assert_eq!(here.lib[0], PathBuf::from("lib"), "as the user wrote it");
    }

    #[test]
    fn every_value_it_cannot_read_is_named() {
        let table = check(
            "[check]\ndisable = [\"no-such-code\", \"unused-variable\"]\nmin-severity = \"loud\"\n",
        );
        let (resolved, problems) = table.resolve(Path::new("."));
        assert_eq!(resolved.disabled, [Code::UnusedVariable]);
        assert_eq!(resolved.min_severity, None);
        assert_eq!(
            problems,
            [
                "unknown diagnostic code `no-such-code` in camello.toml",
                "`min-severity` in camello.toml takes `error`, `warning` or `info`, not `loud`",
            ],
        );
    }
}
