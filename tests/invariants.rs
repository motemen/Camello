//! The acceptance bar: what every checked-in fixture must satisfy.
//!
//! Two different kinds of check live here, and the difference between them is
//! the whole design.
//!
//! **The expected output.** A fixture under `src/fmt/fixtures/regressions/` is
//! an A→B pair: the `.pl` file is A, and B is its `.expected.pl` sibling, or A
//! itself when there is none — a fixture that must come back unchanged is not a
//! special case, it is one whose B happens to equal its A. Where the answer is
//! written down, checking the answer is the whole of the job.
//!
//! **The invariants** ([`camello::check`], ADR 0006 §6 and ADR 0008 §6). These
//! are what can be asked of code whose answer *nobody has written down*: that
//! the string content survives, that the comments survive, that a second pass
//! changes nothing. Their real home is `camello check`, over a corpus; running
//! them over the fixtures as well is cheap, and catches an expected output that
//! is itself wrong.
//!
//! A defect found before it is fixed goes in the ledger — see
//! `src/fmt/fixtures/regressions/known-broken.txt`.

use std::fs;
use std::path::{Path, PathBuf};

use camello::check::{check, Invariant};
use camello::format_perl;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Collect every fixture `.pl` below `dir`, sorted for deterministic reporting.
///
/// `*.expected.pl` is the B of an A→B pair, not a fixture in its own right.
fn collect_fixtures(dir: &Path) -> Vec<PathBuf> {
    let mut acc = Vec::new();
    fn walk(dir: &Path, acc: &mut Vec<PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) => panic!("failed to read {}: {err}", dir.display()),
        };
        for entry in entries {
            let path = entry.expect("failed to read directory entry").path();
            if path.is_dir() {
                walk(&path, acc);
            } else if path.extension().is_some_and(|ext| ext == "pl")
                && !path.to_string_lossy().ends_with(".expected.pl")
            {
                acc.push(path);
            }
        }
    }
    walk(dir, &mut acc);
    acc.sort();
    acc
}

/// Every fixture the invariants are asked of.
///
/// The `errors/` fixtures are excluded on purpose: they exist to pin down what a
/// malformed file reports, and do not parse cleanly by construction.
fn all_fixture_files() -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    for directory in ["src/fmt/fixtures", "src/parse/fixtures/success"] {
        for path in collect_fixtures(&root().join(directory)) {
            let label = path
                .strip_prefix(root())
                .unwrap_or(&path)
                .display()
                .to_string();
            files.push((label, path));
        }
    }
    assert!(!files.is_empty(), "no fixtures found");
    files
}

/// The A→B pairs: (label, A, B).
fn regression_cases() -> Vec<(String, String, String)> {
    collect_fixtures(&root().join("src/fmt/fixtures/regressions"))
        .into_iter()
        .map(|path| {
            let label = path
                .strip_prefix(root())
                .unwrap_or(&path)
                .display()
                .to_string();
            let input = fs::read_to_string(&path).expect("failed to read fixture");
            let expected_path = path.with_extension("expected.pl");
            let expected = fs::read_to_string(&expected_path).unwrap_or_else(|_| input.clone());
            (label, input, expected)
        })
        .collect()
}

/// Fixtures whose expected output the formatter does not yet produce.
///
/// The ledger is monotone: an entry may be removed and never added, and a listed
/// fixture that starts producing its expected output fails the test just as
/// loudly as an unlisted one that stops. So it cannot silence a regression in
/// code that works today — that code has no entry to hide behind — and it cannot
/// go stale, because the fix that lands is required to delete its line.
///
/// A listed fixture is skipped by the invariant sweeps too. Its output is
/// already known to be wrong; asking eight more questions about it spreads one
/// fact across eight lists, and the fix would then have to remember all eight.
fn known_broken() -> Vec<String> {
    let path = root().join("src/fmt/fixtures/regressions/known-broken.txt");
    let text = fs::read_to_string(&path).expect("failed to read the ledger");
    let entries: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToString::to_string)
        .collect();
    for entry in &entries {
        assert!(
            root().join(entry).is_file(),
            "the ledger lists {entry}, which does not exist"
        );
    }
    entries
}

struct Failure {
    fixture: String,
    detail: String,
}

fn report(kind: &str, failures: Vec<Failure>, total: usize) {
    assert!(
        failures.is_empty(),
        "{}",
        failures.iter().fold(
            format!("{} of {total} fixtures violate {kind}:\n", failures.len()),
            |mut message, failure| {
                message.push_str(&format!(
                    "\n=== {} ===\n{}\n",
                    failure.fixture, failure.detail
                ));
                message
            }
        )
    );
}

/// The first line at which two texts differ, rendered with its neighbours.
fn describe_line_divergence(expected: &str, actual: &str) -> String {
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();
    let position = (0..expected.len().max(actual.len()))
        .find(|&index| expected.get(index) != actual.get(index))
        .unwrap_or(0);
    let context = position.saturating_sub(2);
    let render = |lines: &[&str]| {
        lines
            .iter()
            .enumerate()
            .skip(context)
            .take(position - context + 3)
            .map(|(index, line)| {
                let marker = if index == position { ">>" } else { "  " };
                format!("{marker} {line}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "first difference at line {}\n--- expected ---\n{}\n--- actual ---\n{}",
        position + 1,
        render(&expected),
        render(&actual)
    )
}

/// Every A→B pair produces its B.
///
/// This is the one test the ledger answers to. A fixture that reproduces a
/// defect is listed there, and the fix that lands is what deletes its line.
#[test]
fn fixtures_produce_their_expected_output() {
    let known = known_broken();
    let cases = regression_cases();
    let total = cases.len();
    assert!(total > 0, "no regression fixtures found");

    let mut failures = Vec::new();
    let mut tolerated = Vec::new();
    let mut fixed = Vec::new();

    for (label, input, expected) in cases {
        let (actual, _) = format_perl(&input);
        let listed = known.contains(&label);
        match (actual == expected, listed) {
            (true, false) => {}
            (true, true) => fixed.push(label),
            (false, true) => tolerated.push(label),
            (false, false) => failures.push(Failure {
                fixture: label,
                detail: describe_line_divergence(&expected, &actual),
            }),
        }
    }

    // Visible under `cargo test -- --nocapture`; a green run should still be
    // able to say what it is not checking.
    if !tolerated.is_empty() {
        println!("{} known-broken fixture(s) tolerated", tolerated.len());
        for label in &tolerated {
            println!("  - {label}");
        }
    }

    assert!(
        fixed.is_empty(),
        "fixture(s) now produce their expected output but are still listed in \
         known-broken.txt; remove them:\n{}",
        fixed
            .iter()
            .map(|label| format!("  - {label}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    report("their expected output", failures, total);
}

/// Ask one invariant of every fixture that is not already known to be broken.
fn sweep(invariant: Invariant) {
    let known = known_broken();
    let files = all_fixture_files();
    let total = files.len();
    let mut failures = Vec::new();

    for (label, path) in files {
        if known.contains(&label) {
            continue;
        }
        let source = fs::read_to_string(&path).expect("failed to read fixture");
        for violation in check(&source) {
            if violation.invariant == invariant {
                failures.push(Failure {
                    fixture: label.clone(),
                    detail: format!("{}\n{}", violation.summary, violation.detail),
                });
            }
        }
    }

    report(invariant.name(), failures, total);
}

#[test]
fn every_fixture_parses_without_diagnostics() {
    sweep(Invariant::CleanParse);
}

#[test]
fn parsing_is_lossless() {
    sweep(Invariant::Losslessness);
}

#[test]
fn no_node_range_includes_trivia() {
    sweep(Invariant::TriviaPlacement);
}

#[test]
fn formatting_is_idempotent() {
    sweep(Invariant::Idempotency);
}

#[test]
fn formatting_preserves_semantics() {
    sweep(Invariant::SemanticPreservation);
}

#[test]
fn formatting_preserves_comments() {
    sweep(Invariant::CommentPreservation);
}

#[test]
fn formatting_preserves_verbatim_content() {
    sweep(Invariant::VerbatimPreservation);
}

#[test]
fn layout_decisions_are_stable() {
    sweep(Invariant::SeedStability);
}

/// The expected outputs are themselves Perl the formatter is happy with.
///
/// A B that is not a fixed point of the formatter is not an answer: applying the
/// formatter to it would produce something else, and the fixture would be asking
/// for output the formatter can never settle on.
#[test]
fn expected_outputs_are_fixed_points() {
    let known = known_broken();
    let cases = regression_cases();
    let total = cases.len();
    let mut failures = Vec::new();

    for (label, _, expected) in cases {
        // A known-broken fixture whose B equals its A is exactly the case where
        // the formatter does not settle on B yet; that is what the ledger says.
        if known.contains(&label) {
            continue;
        }
        let (once, _) = format_perl(&expected);
        if once != expected {
            failures.push(Failure {
                fixture: label,
                detail: describe_line_divergence(&expected, &once),
            });
        }
    }

    report("their expected output being a fixed point", failures, total);
}
