//! The fixture harness (`docs/lsp.md`, "Testing").
//!
//! One level up from `camello-sema`'s, and the same bargain: a fixture is
//! Perl, its expectations are comments inside it, and the two sets must be
//! *equal* — an unexpected answer fails as loudly as a missing one, which is
//! what makes a fixture with no markers the way to write down "the server
//! stays silent here".
//!
//! A fixture is a **directory** under `src/fixtures/`. Everything in it is the
//! workspace, indexed the way a real one is; every Perl file in it is opened
//! and asked about.
//!
//! Two marker shapes:
//!
//! ```perl
//! my $unused = 1;              #~ warning unused-variable: `$unused`
//! my $dog = My::Dog->new;
//! #   ^ hover $dog : InstanceOf['My::Dog']
//! ```
//!
//! * `#~ <severity> <code>[: text]` — a *published* diagnostic on this line,
//!   after the blast radius has been applied. This is the same grammar the
//!   checker's own fixtures use, on purpose: what an editor shows and what
//!   `camello check` prints are the same diagnostics.
//! * `#<spaces>^ <feature> <expected>` — a request at the position the caret
//!   points at *in the line above*, and the answer it must give. The features
//!   are `hover`, `complete`, `complete-own` and `definition`; `-` is how "no
//!   answer" is written, which is the answer the checker's silence discipline
//!   produces most often and the one worth being able to assert.
//!
//! A file named `X.pl.edit` beside `X.pl` is the buffer *after* an edit: the
//! harness opens `X.pl`, then sends the edited text as version 2 and asks the
//! `.edit` file's markers of it. That is the only way to write down a
//! mid-edit state — a half-typed `->` whose receiver is only in the previous
//! version's table — and it is what the broken-buffer suite is built out of.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use camello_sema::{Code, Severity};
use rowan::TextSize;
use tower_lsp_server::ls_types::Uri;

use crate::analysis;
use crate::document::Document;
use crate::handlers;
use crate::position::Encoding;
use crate::settings::Settings;
use crate::state::GlobalState;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures")
}

/// The server, without a transport.
///
/// `tower-lsp-server`'s `LanguageServer` is a trait on a plain struct, and
/// what the protocol layer adds above this is JSON and a socket. So a test
/// holds the state, opens a document, edits it and asks the handlers — which
/// is the request path with the transport taken out rather than a
/// reimplementation of it.
struct Harness {
    state: GlobalState,
    root: PathBuf,
}

impl Harness {
    fn open_workspace(root: &Path) -> Self {
        let (mut settings, problems) = Settings::load(root, &[root.to_path_buf()]);
        assert!(problems.is_empty(), "{}: {problems:?}", root.display());
        // A test does not write a cache into the source tree; it also must not
        // read one, or a stale entry would decide what a fixture proves.
        settings.cache_dir = None;
        let index = crate::index::build(&settings);
        let mut state = GlobalState::new(settings);
        state.index = Arc::new(RwLock::new(index));
        Harness {
            state,
            root: root.to_path_buf(),
        }
    }

    fn uri(&self, path: &Path) -> Uri {
        Uri::from_file_path(path).expect("a fixture path is absolute")
    }

    /// Put a buffer in the store and analyse it, as `didOpen`/`didChange` do.
    fn edit(&mut self, path: &Path, text: &str, version: i32) -> Answers {
        let uri = self.uri(path);
        let document = Arc::new(Document::new(
            Some(path.to_path_buf()),
            text,
            version,
            Encoding::Utf16,
        ));
        self.state
            .documents
            .insert(uri.clone(), Arc::clone(&document));

        let index = Arc::clone(&self.state.index);
        let settings = Arc::clone(&self.state.settings);
        let held = index.read().expect("no writer holds this in a test");
        let context = analysis::context(&document, &held, &settings);
        let found = analysis::analyse(&document, &context, &settings, true);
        let tables = Arc::clone(&found.tables);
        drop(held);
        self.state.remember(&uri, Arc::clone(&tables));
        Answers {
            document,
            tables,
            diagnostics: found.diagnostics,
        }
    }

    /// Ask one marker, and render the answer the way the marker spells it.
    fn ask(&self, answers: &Answers, marker: &Marker) -> String {
        let index = self.state.index.read().expect("no writer holds this");
        let context = analysis::context(&answers.document, &index, &self.state.settings);
        let offset = answers.document.positions.offset(marker.position);
        match marker.feature {
            Feature::Hover => {
                handlers::hover::hover(&answers.document, &answers.tables, &context, offset)
                    .map_or_else(|| "-".to_string(), |hover| render_hover(&hover))
            }
            Feature::Complete | Feature::CompleteOwn => {
                let uri = self.uri(
                    answers
                        .document
                        .path
                        .as_ref()
                        .expect("a fixture has a path"),
                );
                let fallback = self.state.clean_tables.get(&uri).cloned();
                let items = handlers::completion::completion(
                    &answers.document,
                    &answers.tables,
                    fallback.as_deref(),
                    &context,
                    offset,
                );
                let labels: Vec<String> = items
                    .into_iter()
                    .filter(|item| {
                        marker.feature == Feature::Complete
                            || item
                                .label_details
                                .as_ref()
                                .and_then(|details| details.description.as_deref())
                                != Some("UNIVERSAL")
                    })
                    .map(|item| item.label)
                    .collect();
                if labels.is_empty() {
                    "-".to_string()
                } else {
                    labels.join(", ")
                }
            }
            Feature::Definition => {
                let uri = self.uri(
                    answers
                        .document
                        .path
                        .as_ref()
                        .expect("a fixture has a path"),
                );
                handlers::definition::definition(
                    &answers.document,
                    &uri,
                    &answers.tables,
                    &context,
                    offset,
                    Encoding::Utf16,
                )
                .map_or_else(
                    || "-".to_string(),
                    |location| {
                        let path = location
                            .uri
                            .to_file_path()
                            .map(|path| path.into_owned())
                            .unwrap_or_default();
                        let name = path
                            .strip_prefix(&self.root)
                            .unwrap_or(&path)
                            .display()
                            .to_string();
                        format!(
                            "{name}:{}:{}",
                            location.range.start.line + 1,
                            location.range.start.character + 1
                        )
                    },
                )
            }
        }
    }
}

/// What one analysis of one buffer produced.
struct Answers {
    document: Arc<Document>,
    tables: Arc<analysis::Tables>,
    diagnostics: Vec<camello_sema::Diagnostic>,
}

fn render_hover(hover: &tower_lsp_server::ls_types::Hover) -> String {
    let tower_lsp_server::ls_types::HoverContents::Markup(markup) = &hover.contents else {
        panic!("the server only ever answers with markup");
    };
    markup
        .value
        .trim_start_matches("```perl")
        .trim_end_matches("```")
        .trim()
        .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Feature {
    Hover,
    Complete,
    /// Completion without what `UNIVERSAL` gives every class, which is the
    /// same seven names in every fixture and says nothing about any of them.
    CompleteOwn,
    Definition,
}

#[derive(Debug)]
struct Marker {
    position: tower_lsp_server::ls_types::Position,
    feature: Feature,
    expected: String,
    /// The line the marker itself is written on, for the failure message.
    line: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct Expectation {
    line: usize,
    severity: Severity,
    code: Code,
    contains: Option<String>,
}

/// `#<spaces>^ <feature> <expected>` — a request at the caret's column in the
/// line above.
fn parse_markers(source: &str) -> Vec<Marker> {
    let mut markers = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }
        let Some(caret) = line.find('^') else {
            continue;
        };
        let rest = line[caret + 1..].trim();
        let (word, expected) = rest.split_once(' ').unwrap_or((rest, ""));
        let feature = match word {
            "hover" => Feature::Hover,
            "complete" => Feature::Complete,
            "complete-own" => Feature::CompleteOwn,
            "definition" => Feature::Definition,
            _ => continue,
        };
        // The nearest line above that is not itself a marker, so several
        // questions can be asked of one line of Perl — hover *and*
        // definition, which is the usual pair.
        let mut target = index;
        while target > 0 {
            target -= 1;
            let candidate = source.lines().nth(target).unwrap_or_default();
            if !is_marker_line(candidate) {
                break;
            }
        }
        assert!(
            !is_marker_line(source.lines().nth(target).unwrap_or_default()),
            "a marker points at a line of Perl"
        );
        // The caret's column is a UTF-16 offset into that line, which is
        // exactly what a client would send.
        let above = source.lines().nth(target).unwrap_or_default();
        let character: usize = above
            .char_indices()
            .take_while(|(offset, _)| *offset < caret)
            .map(|(_, ch)| ch.len_utf16())
            .sum();
        markers.push(Marker {
            position: tower_lsp_server::ls_types::Position {
                line: u32::try_from(target).expect("a fixture is short"),
                character: u32::try_from(character).expect("a fixture line is short"),
            },
            feature,
            expected: expected.trim().to_string(),
            line: index + 1,
        });
    }
    markers
}

/// Whether a line is one of the `#^` markers rather than Perl.
fn is_marker_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return false;
    }
    let Some(caret) = line.find('^') else {
        return false;
    };
    let rest = line[caret + 1..].trim();
    let word = rest.split_whitespace().next().unwrap_or_default();
    matches!(word, "hover" | "complete" | "complete-own" | "definition")
}

/// The same `#~` grammar `camello-sema`'s fixtures use.
fn parse_expectations(source: &str) -> Vec<Expectation> {
    let mut acc = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let mut rest = line;
        let mut line_number = index + 1;
        if line.trim_start().starts_with("#~") {
            line_number = index;
        }
        while let Some(position) = rest.find("#~") {
            let body = &rest[position + 2..];
            let end = body.find("#~").unwrap_or(body.len());
            let marker = body[..end].trim();
            rest = &body[end..];
            let (head, contains) = match marker.split_once(':') {
                Some((head, text)) => (head.trim(), Some(text.trim().to_string())),
                None => (marker, None),
            };
            let mut words = head.split_whitespace();
            let (Some(severity), Some(code)) = (words.next(), words.next()) else {
                panic!("a `#~` marker wants `<severity> <code>`, got `{marker}`");
            };
            acc.push(Expectation {
                line: line_number,
                severity: Severity::parse(severity)
                    .unwrap_or_else(|| panic!("unknown severity `{severity}`")),
                code: Code::parse(code).unwrap_or_else(|| panic!("unknown code `{code}`")),
                contains,
            });
        }
    }
    acc
}

fn line_of(source: &str, offset: TextSize) -> usize {
    source[..usize::from(offset).min(source.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

/// Every Perl file in a fixture directory, and the edited buffer beside it.
fn fixture_files(dir: &Path) -> Vec<(PathBuf, Option<PathBuf>)> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .expect("the fixture directory exists")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    files.sort();
    files
        .iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "pl" || extension == "pm")
        })
        .map(|path| {
            let edited = path.with_extension(format!(
                "{}.edit",
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_default()
            ));
            (path.clone(), edited.is_file().then_some(edited))
        })
        .collect()
}

fn fixture_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(fixtures_dir())
        .expect("the fixtures directory exists")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Check one buffer's markers and diagnostics, and gather what went wrong.
fn check_buffer(
    harness: &mut Harness,
    path: &Path,
    text: &str,
    version: i32,
    label: &str,
    failures: &mut Vec<String>,
) {
    let answers = harness.edit(path, text, version);

    for marker in parse_markers(text) {
        let actual = harness.ask(&answers, &marker);
        if actual != marker.expected {
            failures.push(format!(
                "{label}:{}: expected `{}`, got `{}`",
                marker.line, marker.expected, actual
            ));
        }
    }

    let mut expected: BTreeMap<(usize, Code), Vec<&Expectation>> = BTreeMap::new();
    let parsed = parse_expectations(text);
    for expectation in &parsed {
        expected
            .entry((expectation.line, expectation.code))
            .or_default()
            .push(expectation);
    }
    let mut matched = vec![false; parsed.len()];
    for diagnostic in &answers.diagnostics {
        let line = line_of(text, diagnostic.range.start());
        let found = parsed.iter().enumerate().position(|(index, expectation)| {
            !matched[index]
                && expectation.line == line
                && expectation.code == diagnostic.code
                && expectation.severity == diagnostic.severity
                && expectation
                    .contains
                    .as_ref()
                    .is_none_or(|text| diagnostic.message.contains(text))
        });
        match found {
            Some(index) => matched[index] = true,
            None => failures.push(format!(
                "{label}:{line}: unexpected {} {} — {}",
                diagnostic.severity, diagnostic.code, diagnostic.message
            )),
        }
    }
    for (index, expectation) in parsed.iter().enumerate() {
        if !matched[index] {
            failures.push(format!(
                "{label}:{}: expected {} {} and got nothing",
                expectation.line, expectation.severity, expectation.code
            ));
        }
    }
}

#[test]
fn fixtures_answer_what_their_markers_say() {
    let mut failures: Vec<String> = Vec::new();
    let dirs = fixture_dirs();
    assert!(!dirs.is_empty(), "there are fixtures to run");
    for dir in dirs {
        let mut harness = Harness::open_workspace(&dir);
        for (path, edited) in fixture_files(&dir) {
            let text = fs::read_to_string(&path).expect("a readable fixture");
            let label = path
                .strip_prefix(fixtures_dir())
                .unwrap_or(&path)
                .display()
                .to_string();
            check_buffer(&mut harness, &path, &text, 1, &label, &mut failures);
            if let Some(edited) = edited {
                let text = fs::read_to_string(&edited).expect("a readable fixture");
                let label = edited
                    .strip_prefix(fixtures_dir())
                    .unwrap_or(&edited)
                    .display()
                    .to_string();
                check_buffer(&mut harness, &path, &text, 2, &label, &mut failures);
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
