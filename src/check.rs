//! The invariants of ADR 0006 §6 and ADR 0008 §6, asked of arbitrary input.
//!
//! These are the properties that can be asserted about formatting *code nobody
//! has written the answer down for*. A fixture has an expected output and is
//! checked against it; a file taken off someone's disk has no expected output,
//! and these eight questions are what is left to ask of it.
//!
//! They live here rather than in the test so that both callers can use them:
//! `tests/invariants.rs` runs them over the checked-in fixtures, and
//! `camello dev check` runs them over anything at all — which is how a defect gets
//! found in the first place, before it is minimised into a fixture.

use crate::lang::{TokenExt, TokenKind};
use crate::{format_perl, parse_perl};

/// One property of the formatter, asked of one input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Invariant {
    /// The input parses without a diagnostic.
    CleanParse,
    /// The tree's tokens reproduce the source byte for byte (ADR 0006 §6).
    Losslessness,
    /// No node's range begins or ends on trivia (ADR 0006 §4).
    TriviaPlacement,
    /// `format(format(x)) == format(x)` (ADR 0008 §6).
    Idempotency,
    /// Re-lexing input and output yields the same non-trivia token sequence.
    SemanticPreservation,
    /// Input and output hold the same comment texts, in the same order.
    CommentPreservation,
    /// Verbatim content is reproduced byte for byte (ADR 0008 §6, I1).
    VerbatimPreservation,
    /// A broken group's own output re-reads as broken (ADR 0008 §6, I2).
    SeedStability,
}

impl Invariant {
    /// Every invariant, in the order they are reported.
    pub const ALL: &'static [Invariant] = &[
        Invariant::CleanParse,
        Invariant::Losslessness,
        Invariant::TriviaPlacement,
        Invariant::Idempotency,
        Invariant::SemanticPreservation,
        Invariant::CommentPreservation,
        Invariant::VerbatimPreservation,
        Invariant::SeedStability,
    ];

    /// The name used in reports, with the ADR clause it comes from.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Invariant::CleanParse => "a clean parse",
            Invariant::Losslessness => "losslessness (ADR 0006 §6)",
            Invariant::TriviaPlacement => "trivia placement (ADR 0006 §4)",
            Invariant::Idempotency => "idempotency (ADR 0008 §6)",
            Invariant::SemanticPreservation => "semantic preservation (ADR 0008 §6)",
            Invariant::CommentPreservation => "comment preservation",
            Invariant::VerbatimPreservation => "verbatim preservation (ADR 0008 §6, I1)",
            Invariant::SeedStability => "seed stability (ADR 0008 §6, I2)",
        }
    }

    /// A short slug, for filtering on a command line.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Invariant::CleanParse => "clean-parse",
            Invariant::Losslessness => "lossless",
            Invariant::TriviaPlacement => "trivia-placement",
            Invariant::Idempotency => "idempotency",
            Invariant::SemanticPreservation => "semantics",
            Invariant::CommentPreservation => "comments",
            Invariant::VerbatimPreservation => "verbatim",
            Invariant::SeedStability => "seed-stability",
        }
    }
}

/// One invariant, violated, with enough detail to act on.
#[derive(Debug, Clone)]
pub struct Violation {
    pub invariant: Invariant,
    /// A one-line summary, suitable for a table.
    pub summary: String,
    /// The evidence: streams, passes, offending nodes.
    pub detail: String,
}

/// Ask every invariant of `source`.
///
/// A file that does not parse cleanly is reported as that and nothing else: the
/// other seven questions are about what the formatter did with a tree, and a
/// tree built by error recovery is not one the formatter was asked to handle.
#[must_use]
pub fn check(source: &str) -> Vec<Violation> {
    check_only(source, Invariant::ALL)
}

/// Ask only these invariants, and do only their work.
///
/// The filtering used to happen after the fact, so `--only clean-parse` parsed
/// the file, formatted it twice, re-lexed both, and then threw seven answers
/// away — asking one question of a corpus cost eight times what it should.
#[must_use]
pub fn check_only(source: &str, wanted: &[Invariant]) -> Vec<Violation> {
    let asked = |invariant: Invariant| wanted.contains(&invariant);
    let mut violations = Vec::new();

    // Not a question anyone can opt out of: every answer below is about what the
    // formatter did with a tree, and a tree built by error recovery is not one
    // it was asked to handle.
    if let Some(violation) = clean_parse(source) {
        return asked(Invariant::CleanParse)
            .then_some(vec![violation])
            .unwrap_or_default();
    }

    if asked(Invariant::Losslessness) {
        violations.extend(losslessness(source));
    }
    if asked(Invariant::TriviaPlacement) {
        violations.extend(trivia_placement(source));
    }

    let formats = [
        Invariant::Idempotency,
        Invariant::SemanticPreservation,
        Invariant::CommentPreservation,
        Invariant::VerbatimPreservation,
        Invariant::SeedStability,
    ];
    if !formats.iter().copied().any(asked) {
        return violations;
    }

    let (formatted, _) = format_perl(source);

    if asked(Invariant::Idempotency) {
        violations.extend(idempotency(&formatted));
    }
    if asked(Invariant::SemanticPreservation) {
        violations.extend(semantic_preservation(source, &formatted));
    }
    if asked(Invariant::CommentPreservation) {
        violations.extend(comment_preservation(source, &formatted));
    }
    if asked(Invariant::VerbatimPreservation) {
        violations.extend(verbatim_preservation(source, &formatted));
    }
    if asked(Invariant::SeedStability) {
        violations.extend(seed_stability(source, &formatted));
    }

    violations
}

fn violation(invariant: Invariant, summary: String, detail: String) -> Violation {
    Violation {
        invariant,
        summary,
        detail,
    }
}

fn clean_parse(source: &str) -> Option<Violation> {
    let (_, errors) = parse_perl(source);
    if errors.is_empty() {
        return None;
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
    Some(violation(
        Invariant::CleanParse,
        format!("{} diagnostic(s)", errors.len()),
        detail,
    ))
}

fn losslessness(source: &str) -> Option<Violation> {
    let rebuilt: String = parse_perl(source)
        .0
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .map(|token| token.text().to_string())
        .collect();
    (rebuilt != source).then(|| {
        violation(
            Invariant::Losslessness,
            "the tree's tokens do not reproduce the source".to_string(),
            format!("{} bytes in, {} bytes out", source.len(), rebuilt.len()),
        )
    })
}

fn trivia_placement(source: &str) -> Option<Violation> {
    let root = parse_perl(source).0;
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

        // Asked of the tokens, not of the text: POD and `__DATA__` hold their
        // own line terminators, and those are content.
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
    (!offenders.is_empty()).then(|| {
        violation(
            Invariant::TriviaPlacement,
            format!("{} node(s) span trivia", offenders.len()),
            offenders.join("\n"),
        )
    })
}

fn idempotency(formatted: &str) -> Option<Violation> {
    let (twice, _) = format_perl(formatted);
    (twice != *formatted).then(|| {
        violation(
            Invariant::Idempotency,
            "format(format(x)) != format(x)".to_string(),
            {
                let pass1: Vec<&str> = formatted.lines().collect();
                let pass2: Vec<&str> = twice.lines().collect();
                let report = Report {
                    unit: "line",
                    sides: ("pass 1", "pass 2"),
                    base: 1,
                };
                describe_divergence(&pass1, &pass2, &report, |line| elide(line))
            },
        )
    })
}

fn semantic_preservation(source: &str, formatted: &str) -> Option<Violation> {
    let before = token_stream(source);
    let after = token_stream(formatted);
    (before != after).then(|| {
        violation(
            Invariant::SemanticPreservation,
            format!("{} tokens in, {} out", before.len(), after.len()),
            describe_divergence(&before, &after, &Report::stream("token"), |(kind, text)| {
                format!("{kind} {}", elide(text))
            }),
        )
    })
}

fn comment_preservation(source: &str, formatted: &str) -> Option<Violation> {
    let before = comment_stream(source);
    let after = comment_stream(formatted);
    (before != after).then(|| {
        violation(
            Invariant::CommentPreservation,
            format!("{} comments in, {} out", before.len(), after.len()),
            describe_divergence(&before, &after, &Report::stream("comment"), |text| {
                elide(text)
            }),
        )
    })
}

/// Two checks, because they fail in different ways. The sequence comparison
/// catches a literal that changed; the substring test catches a literal that
/// grew something the formatter inserted next to it, which is how the same byte
/// sequence can survive re-lexing as a *different* token boundary.
fn verbatim_preservation(source: &str, formatted: &str) -> Option<Violation> {
    let before = verbatim_stream(source);
    let after = verbatim_stream(formatted);

    if before != after {
        let position = (0..before.len().max(after.len()))
            .find(|&index| before.get(index) != after.get(index))
            .unwrap_or(0);
        let show = |text: Option<&String>| text.map_or("(none)".to_string(), |text| elide(text));
        return Some(violation(
            Invariant::VerbatimPreservation,
            format!("verbatim token #{position} changed"),
            format!(
                "  input:  {}\n  output: {}",
                show(before.get(position)),
                show(after.get(position))
            ),
        ));
    }

    // A heredoc terminator carries the newline that ends its line, and the
    // file's final newline may have been absent from the input.
    after
        .iter()
        .find(|text| !source.contains(text.trim_end_matches('\n')))
        .map(|text| {
            violation(
                Invariant::VerbatimPreservation,
                "verbatim text not present in the input".to_string(),
                format!("  {}", elide(text)),
            )
        })
}

fn seed_stability(source: &str, formatted: &str) -> Option<Violation> {
    let before = crate::fmt::layout_seeds(source);
    let after = crate::fmt::layout_seeds(formatted);
    (before != after).then(|| {
        violation(
            Invariant::SeedStability,
            format!("{} groups in, {} out", before.len(), after.len()),
            format!("--- input ---\n{before:?}\n--- output ---\n{after:?}"),
        )
    })
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

/// As much of a text as a report can carry: a `__DATA__` section or a heredoc
/// body is one item of a stream and thousands of lines of a terminal.
const ELIDE_AT: usize = 120;

/// `text`, quoted, with anything past [`ELIDE_AT`] replaced by a count.
fn elide(text: &str) -> String {
    if text.len() <= ELIDE_AT {
        return format!("{text:?}");
    }
    let mut end = ELIDE_AT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{:?} … {} more bytes", &text[..end], text.len() - end)
}

/// How to caption a divergence report.
struct Report<'a> {
    /// What the stream is made of: `token`, `comment`, `line`.
    unit: &'a str,
    /// What the two sides are called.
    sides: (&'a str, &'a str),
    /// The number the first item is given: 0 for a stream, 1 for a file's lines.
    base: usize,
}

impl<'a> Report<'a> {
    /// The usual case: a stream taken from the input and from the output.
    fn stream(unit: &'a str) -> Self {
        Report {
            unit,
            sides: ("input", "output"),
            base: 0,
        }
    }
}

/// Render the first divergence between two streams, with a little context
/// either side.
///
/// Every one of these comparisons is between two long sequences that agree
/// almost everywhere, so what a report owes its reader is the place they stop
/// agreeing — printing both streams in full is how one mislaid comment fills a
/// screen and says nothing.
fn describe_divergence<T: PartialEq>(
    before: &[T],
    after: &[T],
    report: &Report<'_>,
    render_item: impl Fn(&T) -> String,
) -> String {
    let Report { unit, sides, base } = *report;
    let Some(position) =
        (0..before.len().max(after.len())).find(|&index| before.get(index) != after.get(index))
    else {
        return "streams are equal".to_string();
    };

    let context = position.saturating_sub(3);
    let render = |stream: &[T]| {
        stream
            .iter()
            .enumerate()
            .skip(context)
            .take(position - context + 4)
            .map(|(index, item)| {
                let marker = if index == position { ">>" } else { "  " };
                format!("{marker} [{}] {}", index + base, render_item(item))
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "first divergence at {unit} #{}\n--- {} ---\n{}\n--- {} ---\n{}",
        position + base,
        sides.0,
        render(before),
        sides.1,
        render(after)
    )
}

#[cfg(test)]
mod tests {
    use super::{describe_divergence, elide, Report, ELIDE_AT};

    #[test]
    fn a_report_carries_the_divergence_and_not_the_stream() {
        let before: Vec<String> = (0..500).map(|index| format!("# {index}")).collect();
        let mut after = before.clone();
        after.remove(200);

        let detail = describe_divergence(&before, &after, &Report::stream("comment"), |text| {
            elide(text)
        });

        assert!(detail.contains("first divergence at comment #200"));
        assert!(detail.contains("\"# 200\""));
        assert!(!detail.contains("\"# 300\""));
        assert!(detail.lines().count() < 20);
    }

    #[test]
    fn a_long_text_is_elided_on_a_character_boundary() {
        let data = format!("__DATA__\n{}", "らくだ\n".repeat(200));
        let elided = elide(&data);

        assert!(elided.len() < data.len());
        assert!(elided.contains("more bytes"));
        assert!(elide("# short").ends_with("\"# short\""));
        assert_eq!(
            elide(&"x".repeat(ELIDE_AT)),
            format!("{:?}", "x".repeat(ELIDE_AT))
        );
    }
}
