//! Formatter and parser invariants required by ADR 0006 §6 and ADR 0008 §6.
//!
//! * **Clean parse**: every checked-in fixture parses without a diagnostic.
//! * **Losslessness**: the tree's tokens reproduce the source byte for byte.
//! * **Idempotency**: `format(format(x)) == format(x)`.
//! * **Semantic preservation**: re-lexing input and output yields the same
//!   non-trivia token sequence.
//! * **Comment preservation**: input and output hold the same comment texts, in
//!   the same order.
//! * **Verbatim preservation**: every `Raw` token of the output appears
//!   unchanged in the input.
//! * **Seed stability** (ADR 0008 §6 I2): a broken group's own output re-reads
//!   as broken, so a second pass makes the same layout choices as the first.
//! * **Trivia placement** (ADR 0006 §4): no node's range begins or ends on
//!   trivia.
//!
//! These ran throughout the redesign — first against the old stack, then against
//! the new one — with a registry of known violations that was only ever allowed
//! to shrink. It reached empty and was removed; [`known_violations`] brings it
//! back, so that a defect can enter the tree as a fixture on the day it is found
//! rather than on the day it is fixed.

use std::fs;
use std::path::{Path, PathBuf};

use camello::lang::{TokenExt, TokenKind};
use camello::{format_perl, parse_perl};

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

/// Every fixture that must satisfy the invariants.
///
/// The `errors/` fixtures are excluded on purpose: they exist to pin down what a
/// malformed file reports, and do not parse cleanly by construction.
fn all_fixture_files() -> Vec<(String, PathBuf)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for directory in ["src/fmt/fixtures", "src/parse/fixtures/success"] {
        for path in collect_fixtures(&root.join(directory)) {
            let label = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            files.push((label, path));
        }
    }
    assert!(!files.is_empty(), "no fixtures found");
    files
}

/// The non-trivia token sequence of `source`, as `(kind, text)` pairs.
///
/// Lexing is parser-driven — `expect` comes from the grammar (ADR 0005 §2) — so
/// the faithful way to re-lex is to parse and read the leaves.
fn token_stream(source: &str) -> Vec<(String, String)> {
    parse_perl(source)
        .0
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

/// Render the first divergence between two token streams for a readable failure.
fn describe_divergence(before: &[(String, String)], after: &[(String, String)]) -> String {
    let Some(position) =
        (0..before.len().max(after.len())).find(|&index| before.get(index) != after.get(index))
    else {
        return "streams are equal".to_string();
    };

    let context = position.saturating_sub(3);
    let render = |stream: &[(String, String)]| {
        stream
            .iter()
            .enumerate()
            .skip(context)
            .take(position - context + 4)
            .map(|(index, (kind, text))| {
                let marker = if index == position { ">>" } else { "  " };
                format!("{marker} [{index}] {kind} {text:?}")
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

struct Failure {
    fixture: String,
    detail: String,
}

/// Fixtures known to violate an invariant, one list per invariant.
///
/// A known defect is worth a fixture straight away, but a fixture that
/// reproduces one fails the invariants by construction — so without somewhere to
/// record it, the reproduction cannot be checked in until the fix is written,
/// and in the meantime the only copy of it lives outside the tree.
///
/// The ledger is monotone: an entry may be removed and never added, and a listed
/// fixture that starts passing fails the test just as loudly as an unlisted one
/// that starts failing. So it cannot be used to silence a regression — a
/// regression in already-passing code has no entry to hide behind — and it
/// cannot go stale, because the fix that lands is required to delete its line.
///
/// Each entry says which case it is and links its reproduction.
mod known_violations {
    pub const CLEAN_PARSE: &[&str] = &[];
    pub const LOSSLESSNESS: &[&str] = &[];
    pub const TRIVIA_PLACEMENT: &[&str] = &[];

    /// Downstream of the dropped-body defect: the first pass drops a body, so
    /// the second pass reads a marker that never gets one.
    pub const IDEMPOTENCY: &[&str] = &[
        "src/fmt/fixtures/regressions/heredoc_in_a_list_element.pl",
        "src/fmt/fixtures/regressions/heredoc_marker_without_a_separator.pl",
        "src/fmt/fixtures/regressions/two_heredocs_in_one_list.pl",
    ];

    pub const COMMENT_PRESERVATION: &[&str] = &[];

    /// Verbatim content that owns whole lines is written at the current column
    /// instead of at column 0, or is not written at all.
    ///
    /// * `heredoc_inside_a_broken_group`: written as `Raw` rather than
    ///   `VerbatimLines`, so it lands at the current column instead of at
    ///   column 0. The first body line and the terminator are both indented,
    ///   which changes the string and leaves the terminator no longer at the
    ///   start of its line.
    /// * `heredoc_marker_without_a_separator`: the body and its terminator
    ///   disappear from the output altogether.
    /// * `heredoc_in_a_list_element`: the same as the first, with the marker in
    ///   a bracketed list element rather than a hash value.
    /// * `multiline_q_string_closing_delimiter`: not a heredoc — a multi-line
    ///   `q{...}`, whose closing delimiter is indented, appending the
    ///   indentation to the string's value. perl reads the output's value as
    ///   `"\nalpha\n        "` where the input's is `"\nalpha\n"`.
    /// * `two_heredocs_in_one_list`: both at once — the first body is indented
    ///   further, and with `<<~` that changes the string's value because the
    ///   terminator's indentation is what gets stripped; the second body is
    ///   dropped.
    pub const VERBATIM_PRESERVATION: &[&str] = &[
        "src/fmt/fixtures/regressions/heredoc_in_a_list_element.pl",
        "src/fmt/fixtures/regressions/multiline_q_string_closing_delimiter.pl",
        "src/fmt/fixtures/regressions/heredoc_inside_a_broken_group.pl",
        "src/fmt/fixtures/regressions/heredoc_marker_without_a_separator.pl",
        "src/fmt/fixtures/regressions/two_heredocs_in_one_list.pl",
    ];

    /// Downstream of the same defects: an indented terminator no longer closes
    /// the heredoc and a dropped body is gone, so re-lexing the output yields a
    /// different token sequence — and perl cannot read it at all.
    pub const SEMANTIC_PRESERVATION: &[&str] = &[
        "src/fmt/fixtures/regressions/heredoc_in_a_list_element.pl",
        "src/fmt/fixtures/regressions/multiline_q_string_closing_delimiter.pl",
        "src/fmt/fixtures/regressions/heredoc_inside_a_broken_group.pl",
        "src/fmt/fixtures/regressions/heredoc_marker_without_a_separator.pl",
        "src/fmt/fixtures/regressions/two_heredocs_in_one_list.pl",
    ];
}

fn report(kind: &str, failures: Vec<Failure>, total: usize, known: &[&str]) {
    let (expected, unexpected): (Vec<Failure>, Vec<Failure>) = failures
        .into_iter()
        .partition(|failure| known.contains(&failure.fixture.as_str()));

    let fixed: Vec<&&str> = known
        .iter()
        .filter(|fixture| !expected.iter().any(|failure| failure.fixture == **fixture))
        .collect();

    let mut message = String::new();

    if !unexpected.is_empty() {
        message.push_str(&format!(
            "{} of {total} fixtures violate {kind}:\n",
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

#[test]
fn every_fixture_parses_without_diagnostics() {
    let files = all_fixture_files();
    let total = files.len();
    let mut failures = Vec::new();

    for (label, path) in files {
        let source = fs::read_to_string(&path).expect("failed to read fixture");
        let (_, errors) = parse_perl(&source);
        if errors.is_empty() {
            continue;
        }
        let detail = errors
            .iter()
            .take(3)
            .map(|error| {
                let line = source[..usize::from(error.range.start())].lines().count();
                format!("  line {line}: {}", error.message)
            })
            .collect::<Vec<_>>()
            .join("\n");
        failures.push(Failure {
            fixture: label,
            detail,
        });
    }

    report(
        "a clean parse",
        failures,
        total,
        known_violations::CLEAN_PARSE,
    );
}

#[test]
fn parsing_is_lossless() {
    let files = all_fixture_files();
    let total = files.len();
    let mut failures = Vec::new();

    for (label, path) in files {
        let source = fs::read_to_string(&path).expect("failed to read fixture");
        let rebuilt: String = parse_perl(&source)
            .0
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .map(|token| token.text().to_string())
            .collect();
        if rebuilt != source {
            failures.push(Failure {
                fixture: label,
                detail: "the tree's tokens do not reproduce the source".to_string(),
            });
        }
    }

    report(
        "losslessness (ADR 0006 §6)",
        failures,
        total,
        known_violations::LOSSLESSNESS,
    );
}

/// No node's range begins or ends on trivia (ADR 0006 §4).
///
/// The ADR promised this as a property test and it shipped as one assertion
/// about one hard-coded string. It is what makes "does this node span more than
/// one line" have an exact answer instead of depending on where the whitespace
/// happened to land, so it is worth asking of every fixture in the tree.
#[test]
fn no_node_range_includes_trivia() {
    let files = all_fixture_files();
    let total = files.len();
    let mut failures = Vec::new();

    for (label, path) in files {
        let source = fs::read_to_string(&path).expect("failed to read fixture");
        let root = parse_perl(&source).0;
        let mut offenders = Vec::new();
        for node in root.descendants() {
            // ROOT covers the file, trailing newline and all.
            if node == root {
                continue;
            }
            let mut tokens = node
                .descendants_with_tokens()
                .filter_map(|element| element.into_token());
            let Some(first) = tokens.next() else { continue };
            let last = node
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .last()
                .unwrap_or_else(|| first.clone());

            // Asked of the tokens, not of the text: POD and `__DATA__` hold
            // their own line terminators, and those are content.
            if first.token_kind().is_trivia() {
                offenders.push(format!(
                    "  {} starts on trivia: {:?}",
                    node.kind(),
                    first.text()
                ));
            } else if last.token_kind().is_trivia() {
                offenders.push(format!(
                    "  {} ends on trivia: {:?}",
                    node.kind(),
                    last.text()
                ));
            }
            if offenders.len() >= 3 {
                break;
            }
        }
        if !offenders.is_empty() {
            failures.push(Failure {
                fixture: label,
                detail: offenders.join("\n"),
            });
        }
    }

    report(
        "trivia placement (ADR 0006 §4)",
        failures,
        total,
        known_violations::TRIVIA_PLACEMENT,
    );
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
                    "format(format(x)) != format(x)\n--- pass 1 ---\n{once}--- pass 2 ---\n{twice}"
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

/// The comment texts of `source`, in order.
fn comment_stream(source: &str) -> Vec<String> {
    parse_perl(source)
        .0
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.token_kind() == TokenKind::COMMENT)
        .map(|token| token.text().trim_end().to_string())
        .collect()
}

/// The texts of every token the formatter must reproduce byte for byte.
fn verbatim_stream(source: &str) -> Vec<String> {
    parse_perl(source)
        .0
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.token_kind().is_verbatim())
        .map(|token| token.text().to_string())
        .collect()
}

/// Comments survive formatting unchanged and in order.
///
/// The other invariants compare non-trivia tokens, so a comment that is dropped,
/// duplicated, or absorbed into a replacement string changes nothing they look
/// at. Every one of P0-1 and P0-3 in the 2026-07-28 review lived in that blind
/// spot.
#[test]
fn formatting_preserves_comments() {
    let files = all_fixture_files();
    let total = files.len();
    let mut failures = Vec::new();

    for (label, path) in files {
        let source = fs::read_to_string(&path).expect("failed to read fixture");
        let (formatted, _) = format_perl(&source);
        let before = comment_stream(&source);
        let after = comment_stream(&formatted);
        if before != after {
            failures.push(Failure {
                fixture: label,
                detail: format!(
                    "{} comments in, {} out\n--- input ---\n{}\n--- output ---\n{}",
                    before.len(),
                    after.len(),
                    before.join("\n"),
                    after.join("\n")
                ),
            });
        }
    }

    report(
        "comment preservation",
        failures,
        total,
        known_violations::COMMENT_PRESERVATION,
    );
}

/// Verbatim content is reproduced byte for byte (ADR 0008 §6, I1).
///
/// Two checks, because they fail in different ways. The sequence comparison
/// catches a literal that changed; the substring test catches a literal that
/// grew something the formatter inserted next to it, which is how the same byte
/// sequence can survive re-lexing as a *different* token boundary.
#[test]
fn formatting_preserves_verbatim_content() {
    let files = all_fixture_files();
    let total = files.len();
    let mut failures = Vec::new();

    for (label, path) in files {
        let source = fs::read_to_string(&path).expect("failed to read fixture");
        let (formatted, _) = format_perl(&source);
        let before = verbatim_stream(&source);
        let after = verbatim_stream(&formatted);

        if before != after {
            let position = (0..before.len().max(after.len()))
                .find(|&index| before.get(index) != after.get(index))
                .unwrap_or(0);
            failures.push(Failure {
                fixture: label,
                detail: format!(
                    "verbatim token #{position} changed\n  input:  {:?}\n  output: {:?}",
                    before.get(position),
                    after.get(position)
                ),
            });
            continue;
        }

        // A heredoc terminator carries the newline that ends its line, and the
        // file's final newline may have been absent from the input.
        if let Some(text) = after
            .iter()
            .find(|text| !source.contains(text.trim_end_matches('\n')))
        {
            failures.push(Failure {
                fixture: label,
                detail: format!("verbatim text not present in the input: {text:?}"),
            });
        }
    }

    report(
        "verbatim preservation (ADR 0008 §6, I1)",
        failures,
        total,
        known_violations::VERBATIM_PRESERVATION,
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
