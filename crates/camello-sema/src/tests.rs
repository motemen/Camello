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

use std::fs;
use std::path::{Path, PathBuf};

use crate::diag::{Code, Diagnostic, LineIndex, Severity};

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

fn collect(dir: &Path, acc: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("the fixture directory exists") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            collect(&path, acc);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "pl" || ext == "pm")
        {
            acc.push(path);
        }
    }
}

fn fixtures() -> Vec<PathBuf> {
    let mut acc = Vec::new();
    collect(&fixtures_dir(), &mut acc);
    acc.sort();
    acc
}

fn parse_expectations(source: &str) -> Vec<Expectation> {
    let mut acc = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let Some(position) = line.find("#~") else {
            continue;
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
                line: index + 1,
                severity,
                code,
                contains,
            });
        }
    }
    acc
}

fn actual(source: &str, path: &Path) -> Vec<Diagnostic> {
    let parsed = camello_syntax::parse::parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "{} does not parse: {:?}",
        path.display(),
        parsed.diagnostics
    );
    let mut analysis = crate::Analysis::new();
    analysis.declare(path, &parsed.syntax(), true);
    analysis.check(
        path,
        &parsed.syntax(),
        source,
        &crate::Options::for_fixture(path),
    )
}

#[test]
fn fixtures_report_exactly_what_they_say() {
    let files = fixtures();
    assert!(!files.is_empty(), "no fixtures found");
    let mut failures = Vec::new();

    for path in &files {
        let source = fs::read_to_string(path).expect("a readable fixture");
        let index = LineIndex::new(&source);
        let expected = parse_expectations(&source);
        let found = actual(&source, path);

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

    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

#[test]
fn every_code_has_a_fixture() {
    // A code with no coverage is a failing test rather than a gap nobody
    // notices (`docs/typecheck.md`, "Testing").
    let mut seen = Vec::new();
    for path in fixtures() {
        let source = fs::read_to_string(&path).expect("a readable fixture");
        for expectation in parse_expectations(&source) {
            if !seen.contains(&expectation.code) {
                seen.push(expectation.code);
            }
        }
    }
    let missing: Vec<_> = Code::ALL
        .iter()
        .filter(|code| !seen.contains(code) && crate::COVERED_CODES.contains(code))
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
