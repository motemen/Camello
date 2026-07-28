//! Formatter invariants required by ADR 0008 §6.
//!
//! * **Idempotency**: `format(format(x)) == format(x)` for every fixture.
//! * **Semantic preservation**: re-lexing the input and the output yields the
//!   same non-trivia token sequence.
//!
//! These run over every checked-in fixture so that the redesign (ADR 0004-0008)
//! has a fixed acceptance bar that exists *before* the rewrite lands.

use std::fs;
use std::path::{Path, PathBuf};

use camello::{format_perl, parse_perl, SyntaxKind};

/// Collect every `.pl` file below `dir`, sorted for deterministic reporting.
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
            } else if path.extension().is_some_and(|ext| ext == "pl") {
                acc.push(path);
            }
        }
    }
    walk(dir, &mut acc);
    acc.sort();
    acc
}

fn fixture_root(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// The non-trivia token sequence of `source`, as `(kind, text)` pairs.
///
/// Lexing in this project is parser-driven, so the only faithful way to re-lex
/// is to run the parser and read the leaves of the resulting CST.
fn token_stream(source: &str) -> Vec<(SyntaxKind, String)> {
    let (syntax, _errors) = parse_perl(source);
    syntax
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| (token.kind(), token.text().to_string()))
        .collect()
}

/// Render the first divergence between two token streams for a readable failure.
fn describe_divergence(before: &[(SyntaxKind, String)], after: &[(SyntaxKind, String)]) -> String {
    let position = (0..before.len().max(after.len())).find(|&index| {
        let lhs = before.get(index);
        let rhs = after.get(index);
        lhs != rhs
    });

    let Some(position) = position else {
        return "streams are equal".to_string();
    };

    let context = position.saturating_sub(3);
    let render = |stream: &[(SyntaxKind, String)]| {
        stream
            .iter()
            .enumerate()
            .skip(context)
            .take(position - context + 4)
            .map(|(index, (kind, text))| {
                let marker = if index == position { ">>" } else { "  " };
                format!("{marker} [{index}] {kind:?} {text:?}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "first divergence at token #{position}\n--- input ---\n{}\n--- output ---\n{}",
        render(before),
        render(after)
    )
}

pub struct Failure {
    pub fixture: String,
    pub detail: String,
}

/// Fixtures known to violate an invariant under the *current* (pre-redesign)
/// implementation.
///
/// This registry is the migration ledger for ADR 0008 §6: entries may only be
/// removed, never added, and the redesign is not complete until it is empty.
/// Listing a fixture here still enforces something — if a fixture starts
/// passing, the test fails and demands the entry be dropped.
mod known_violations {
    /// F3 in notes/2026-07-28-redesign-assessment.md: alignment groups require a
    /// NEWLINE in the *source*, so `my $x=1;my $yy=2;` only aligns on the second
    /// pass. Fixed by the independent align pass (ADR 0008 §5).
    pub const IDEMPOTENCY: &[&str] = &["src/formatter/fixtures/control_flow.pl"];

    /// F1 is not currently triggered by any checked-in fixture; the redesign adds
    /// coverage for it (multi-line string literals inside blocks).
    pub const SEMANTIC_PRESERVATION: &[&str] = &[];
}

pub fn report(kind: &str, failures: Vec<Failure>, total: usize, known: &[&str]) {
    let mut unexpected = Vec::new();
    let mut seen = Vec::new();

    for failure in failures {
        if known.contains(&failure.fixture.as_str()) {
            seen.push(failure.fixture);
        } else {
            unexpected.push(failure);
        }
    }

    let fixed: Vec<&&str> = known
        .iter()
        .filter(|fixture| !seen.iter().any(|s| s == *fixture))
        .collect();

    let mut message = String::new();

    if !unexpected.is_empty() {
        message.push_str(&format!(
            "{} of {total} fixtures newly violate {kind}:\n",
            unexpected.len()
        ));
        for failure in &unexpected {
            message.push_str(&format!(
                "\n=== {} ===\n{}\n",
                failure.fixture, failure.detail
            ));
        }
    }

    if !fixed.is_empty() {
        message.push_str(&format!(
            "\n{} fixture(s) now satisfy {kind} but are still listed in \
             known_violations; remove them from the registry:\n",
            fixed.len()
        ));
        for fixture in fixed {
            message.push_str(&format!("  - {fixture}\n"));
        }
    }

    assert!(message.is_empty(), "{message}");
}

/// Every fixture directory whose contents must satisfy the invariants.
pub fn all_fixture_files() -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    for root in ["src/formatter/fixtures", "src/parser/fixtures/success"] {
        let dir = fixture_root(root);
        for path in collect_fixtures(&dir) {
            let label = path
                .strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
                .unwrap_or(&path)
                .display()
                .to_string();
            files.push((label, path));
        }
    }
    assert!(!files.is_empty(), "no fixtures found");
    files
}

#[test]
fn formatting_is_idempotent() {
    let files = all_fixture_files();
    let total = files.len();
    let mut failures = Vec::new();

    for (label, path) in files {
        let source = fs::read_to_string(&path).expect("failed to read fixture");
        let (once, _) = format_perl(&source);
        let (twice, _) = format_perl(&once);
        if once != twice {
            failures.push(Failure {
                fixture: label,
                detail: format!(
                    "format(format(x)) != format(x)\n--- pass 1 ---\n{once}\n--- pass 2 ---\n{twice}"
                ),
            });
        }
    }

    report(
        "idempotency (ADR 0008 §6)",
        failures,
        total,
        known_violations::IDEMPOTENCY,
    );
}

#[test]
fn formatting_preserves_semantics() {
    let files = all_fixture_files();
    let total = files.len();
    let mut failures = Vec::new();

    for (label, path) in files {
        let source = fs::read_to_string(&path).expect("failed to read fixture");
        let (formatted, _) = format_perl(&source);
        let before = token_stream(&source);
        let after = token_stream(&formatted);
        if before != after {
            failures.push(Failure {
                fixture: label,
                detail: describe_divergence(&before, &after),
            });
        }
    }

    report(
        "semantic preservation (ADR 0008 §6)",
        failures,
        total,
        known_violations::SEMANTIC_PRESERVATION,
    );
}

// ============================================================================
// The redesigned stack (ADR 0004-0008)
//
// The new lexer, parser and formatter live alongside the old ones until the
// switch-over. These tests hold the new stack to the same bar, with their own
// ledger of what it does not handle yet. Cutting over means emptying these
// three registries; until then they say precisely what is left.
// ============================================================================

mod redesign {
    use super::{all_fixture_files, Failure};
    use camello::fmt::{format_source, FormatterOptions};
    use camello::lang::TokenExt;
    use std::fs;

    /// Fixtures the new grammar does not parse cleanly yet.
    ///
    /// Every entry is a gap in coverage, not a disagreement about what the
    /// fixture means. Ordered as they appear on disk.
    /// Fixtures the new grammar does not parse cleanly yet.
    ///
    /// Empty: every checked-in fixture parses without a diagnostic.
    const PARSE_GAPS: &[&str] = &[];

    /// Fixtures the new formatter does not yet round-trip.
    ///
    /// Both are heredocs: the body is placed in the tree at the line it starts
    /// on (ADR 0007 §7), and the builder does not yet reproduce that placement
    /// exactly when the marker is nested inside a broken argument list.
    const IDEMPOTENCY_GAPS: &[&str] = &[
        "src/formatter/fixtures/heredoc.pl",
        "src/formatter/fixtures/heredoc_and_package.pl",
    ];

    const SEMANTIC_GAPS: &[&str] = &[
        "src/formatter/fixtures/heredoc.pl",
        "src/formatter/fixtures/heredoc_and_package.pl",
    ];

    fn tokens(source: &str) -> Vec<(String, String)> {
        camello::parse::parse(source)
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !token.token_kind().is_trivia())
            .map(|token| {
                (
                    format!("{:?}", token.token_kind()),
                    token.text().to_string(),
                )
            })
            .collect()
    }

    /// Same monotonic ledger as the old stack's: entries may only be removed.
    fn check(kind: &str, failures: Vec<Failure>, known: &[&str]) {
        super::report(kind, failures, known.len(), known);
    }

    #[test]
    fn every_fixture_parses_without_diagnostics() {
        let mut failures = Vec::new();
        for (label, path) in all_fixture_files() {
            let source = fs::read_to_string(&path).expect("failed to read fixture");
            let parsed = camello::parse::parse(&source);
            if parsed.diagnostics.is_empty() {
                continue;
            }
            let detail = parsed
                .diagnostics
                .iter()
                .take(3)
                .map(|diagnostic| {
                    let line = source[..usize::from(diagnostic.range.start())]
                        .lines()
                        .count();
                    format!("  line {line}: {}", diagnostic.message)
                })
                .collect::<Vec<_>>()
                .join("\n");
            failures.push(Failure {
                fixture: label,
                detail,
            });
        }
        check("the redesigned grammar", failures, PARSE_GAPS);
    }

    #[test]
    fn formatting_is_idempotent() {
        let options = FormatterOptions::default();
        let mut failures = Vec::new();
        for (label, path) in all_fixture_files() {
            let source = fs::read_to_string(&path).expect("failed to read fixture");
            let once = format_source(&source, &options);
            let twice = format_source(&once, &options);
            if once != twice {
                failures.push(Failure {
                    fixture: label,
                    detail: format!("--- pass 1 ---\n{once}--- pass 2 ---\n{twice}"),
                });
            }
        }
        check("redesigned idempotency", failures, IDEMPOTENCY_GAPS);
    }

    #[test]
    fn formatting_preserves_semantics() {
        let options = FormatterOptions::default();
        let mut failures = Vec::new();
        for (label, path) in all_fixture_files() {
            let source = fs::read_to_string(&path).expect("failed to read fixture");
            let formatted = format_source(&source, &options);
            if tokens(&source) != tokens(&formatted) {
                failures.push(Failure {
                    fixture: label,
                    detail: "token stream differs".to_string(),
                });
            }
        }
        check("redesigned semantic preservation", failures, SEMANTIC_GAPS);
    }
}
