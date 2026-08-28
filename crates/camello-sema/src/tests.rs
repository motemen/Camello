//! The fixture harness (`docs/typecheck.md`, "Testing").
//!
//! A fixture is a Perl file whose expected diagnostics are written as comments
//! on the line they belong to:
//!
//! ```perl
//! $nope = 1;   #~ error undeclared-variable: `$nope`
//! ```
//!
//! The set of `#~` expectations must *equal* the set of diagnostics, line
//! numbers included — an unexpected diagnostic fails the fixture as loudly as
//! a missing one, which is what makes a fixture with no `#~` at all the way to
//! say "the checker stays silent here".
//!
//! Marker grammar: `#~ <severity> <code>` and, optionally, `: <text>` that the
//! message must contain. Several markers may share a comment, each opened by
//! its own `#~`.
//!
//! A marker that is the only thing on its line belongs to the line *above*.
//! That is for the diagnostics whose span is itself a comment — a `Returns:`
//! that does not parse — where a trailing marker would become part of the
//! annotation it is about.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::diag::{Code, LineIndex, Severity};

#[derive(Debug, PartialEq, Eq)]
struct Expectation {
    line: usize,
    severity: Severity,
    code: Code,
    contains: Option<String>,
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures")
}

/// One fixture: a file on its own, or a directory with a `roots` marker.
///
/// A multi-file fixture is a directory holding a `roots` file, whose lines are
/// `root: <dir>`, `stub: <dir>` and `read-as: <module> = <module>`. Everything
/// under a root is checked; everything under a stub contributes declarations
/// and is never reported on, which is what makes the stub mechanism something
/// a fixture can ask about, and `read-as` is the project setting of the same
/// name.
#[derive(Debug)]
struct Fixture {
    /// The files whose `#~` markers are the expectation.
    checked: Vec<PathBuf>,
    /// The files that contribute declarations only.
    stubs: Vec<PathBuf>,
    /// What this fixture's own modules stand in for.
    dialect: BTreeMap<String, String>,
    label: PathBuf,
}

fn perl_files(dir: &Path, acc: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("the directory exists") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            perl_files(&path, acc);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "pl" || ext == "pm")
        {
            acc.push(path);
        }
    }
    acc.sort();
}

fn collect(dir: &Path, acc: &mut Vec<Fixture>) {
    let marker = dir.join("roots");
    if marker.is_file() {
        let mut fixture = Fixture {
            checked: Vec::new(),
            stubs: Vec::new(),
            dialect: BTreeMap::new(),
            label: dir.to_path_buf(),
        };
        for line in fs::read_to_string(&marker)
            .expect("a readable marker")
            .lines()
        {
            let Some((kind, name)) = line.split_once(':') else {
                continue;
            };
            if kind.trim() == "read-as" {
                let (from, to) = name
                    .split_once('=')
                    .unwrap_or_else(|| panic!("`read-as` wants `A = B` in {}", marker.display()));
                fixture
                    .dialect
                    .insert(from.trim().to_string(), to.trim().to_string());
                continue;
            }
            let mut files = Vec::new();
            perl_files(&dir.join(name.trim()), &mut files);
            match kind.trim() {
                "root" => fixture.checked.extend(files),
                "stub" => fixture.stubs.extend(files),
                other => panic!("unknown marker `{other}` in {}", marker.display()),
            }
        }
        acc.push(fixture);
        return;
    }
    for entry in fs::read_dir(dir).expect("the fixture directory exists") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            collect(&path, acc);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "pl" || ext == "pm")
        {
            acc.push(Fixture {
                checked: vec![path.clone()],
                stubs: Vec::new(),
                dialect: BTreeMap::new(),
                label: path,
            });
        }
    }
}

fn fixtures() -> Vec<Fixture> {
    let mut acc = Vec::new();
    collect(&fixtures_dir(), &mut acc);
    acc.sort_by(|left, right| left.label.cmp(&right.label));
    acc
}

fn parse_expectations(source: &str) -> Vec<Expectation> {
    let mut acc = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let Some(position) = line.find("#~") else {
            continue;
        };
        // An own-line marker is about the line above it.
        let line_number = if line[..position].trim().is_empty() {
            index
        } else {
            index + 1
        };
        for marker in line[position..].split("#~").skip(1) {
            let marker = marker.trim();
            if marker.is_empty() {
                continue;
            }
            let (head, contains) = match marker.split_once(':') {
                Some((head, rest)) => (head.trim(), Some(rest.trim().to_string())),
                None => (marker, None),
            };
            let mut words = head.split_whitespace();
            let severity = words
                .next()
                .and_then(Severity::parse)
                .unwrap_or_else(|| panic!("bad severity in `#~{marker}`"));
            let code = words
                .next()
                .and_then(Code::parse)
                .unwrap_or_else(|| panic!("bad code in `#~{marker}`"));
            acc.push(Expectation {
                line: line_number,
                severity,
                code,
                contains,
            });
        }
    }
    acc
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn parse_one(path: &Path, source: &str) -> camello_syntax::lang::SyntaxNode {
    let parsed = camello_syntax::parse::parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "{} does not parse: {:?}",
        path.display(),
        parsed.diagnostics
    );
    parsed.syntax()
}

#[test]
fn fixtures_report_exactly_what_they_say() {
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no fixtures found");
    let mut failures = Vec::new();

    for fixture in &fixtures {
        // Every file's declarations first, the way a run does it, so that a
        // call in one file can see a sub declared in another.
        let mut analysis = crate::Analysis::new()
            .with_dialect(crate::annotate::Dialect::new(fixture.dialect.clone()));
        for path in fixture.checked.iter().chain(&fixture.stubs) {
            let source = read(path);
            let root = parse_one(path, &source);
            analysis.declare(path, &root, fixture.checked.contains(path));
        }
        analysis.link();

        for path in &fixture.checked {
            let source = read(path);
            let root = parse_one(path, &source);
            let index = LineIndex::new(&source);
            let expected = parse_expectations(&source);
            let found = analysis.check(path, &root, &source, &crate::Options::for_fixture(path));

            let mut remaining: Vec<&Expectation> = expected.iter().collect();
            let mut unexpected = Vec::new();

            for diagnostic in &found {
                let line = index
                    .position(&source, usize::from(diagnostic.range.start()))
                    .line;
                let matched = remaining.iter().position(|expectation| {
                    expectation.line == line
                        && expectation.severity == diagnostic.severity
                        && expectation.code == diagnostic.code
                        && expectation
                            .contains
                            .as_ref()
                            .is_none_or(|text| diagnostic.message.contains(text.as_str()))
                });
                match matched {
                    Some(position) => {
                        remaining.remove(position);
                    }
                    None => unexpected.push(format!(
                        "  line {line}: unexpected {} {}: {}",
                        diagnostic.severity, diagnostic.code, diagnostic.message
                    )),
                }
            }

            if !remaining.is_empty() || !unexpected.is_empty() {
                let mut report = format!("{}:\n", path.display());
                report.push_str(&unexpected.join("\n"));
                for expectation in remaining {
                    report.push_str(&format!(
                        "\n  line {}: expected {} {} and it was not reported",
                        expectation.line, expectation.severity, expectation.code
                    ));
                }
                failures.push(report);
            }
        }
    }

    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

#[test]
fn every_code_has_a_fixture() {
    // A code with no coverage is a failing test rather than a gap nobody
    // notices (`docs/typecheck.md`, "Testing").
    let mut seen = Vec::new();
    for fixture in fixtures() {
        for path in &fixture.checked {
            for expectation in parse_expectations(&read(path)) {
                if !seen.contains(&expectation.code) {
                    seen.push(expectation.code);
                }
            }
        }
    }
    let missing: Vec<_> = Code::ALL
        .iter()
        .filter(|code| !seen.contains(code))
        .map(|code| code.as_str())
        .collect();
    assert!(missing.is_empty(), "no fixture reports {missing:?}");
}

#[test]
fn the_harness_notices_a_missing_diagnostic() {
    // The fixtures above pass; this is what says that passing means something.
    let source = "use strict;\nprint $nope;\n";
    let found = crate::check_source(source, &crate::Options::lint());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, Code::UndeclaredVariable);
    assert_eq!(found[0].severity, Severity::Error);

    let expectations = parse_expectations("print $nope;  #~ error undeclared-variable: `$nope`\n");
    assert_eq!(expectations.len(), 1);
    assert_eq!(expectations[0].line, 1);
    assert_eq!(expectations[0].code, Code::UndeclaredVariable);
    assert_eq!(expectations[0].contains.as_deref(), Some("`$nope`"));
}

#[test]
fn a_list_operator_chain_is_walked_once_per_argument() {
    // `header nullable Str, 'k1' => header nullable Str, ...` is not a flat
    // list of entries: a Perl list operator swallows everything to its right,
    // so thirty entries are thirty nested calls. Typing each argument twice —
    // once on the way in and once against the callee's parameters — therefore
    // cost 2^30 walks, and a hundred-key `Dict` in a real `Type::Library` was
    // not a slow file but a run that never ended.
    let mut source = String::from(
        "package Rows;\n\
         use strict;\n\
         use warnings;\n\
         sub Str { return 'Str' }\n\
         sub nullable { my ($ty) = @_; return $ty }\n\
         sub header { my ($ty) = @_; return $ty }\n\
         sub dict { my ($shape) = @_; return $shape }\n\
         my $row = dict [\n",
    );
    for index in 0..30 {
        source.push_str(&format!("    'k{index}' => header nullable Str,\n"));
    }
    source.push_str("];\n1;\n");

    // Walked once this is milliseconds. The budget is not a performance bar —
    // it is what makes the exponent coming back a failing test rather than a
    // suite that hangs.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let found = crate::check_source(&source, &crate::Options::typecheck());
            let _ = sender.send(found);
        })
        .expect("the thread starts");
    receiver
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("the walk finished inside the budget");
}
