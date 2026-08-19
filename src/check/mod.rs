//! The parser and formatter invariants, asked of arbitrary input.
//!
//! These are the properties that can be asserted about formatting *code nobody
//! has written the answer down for*. A fixture has an expected output and is
//! checked against it; a file taken off someone's disk has no expected output,
//! and these six questions are what is left to ask of it.
//!
//! They come in two groups, and the group is the first thing a report should
//! say: [`Subject::Parser`] asks about the input alone, so a failure there is
//! the parser's; [`Subject::Formatter`] compares an input against its output, so
//! a failure there is the formatter's. An invariant that mixed the two would
//! answer "something is wrong" without answering "with what".
//!
//! They live here rather than in the test so that both callers can use them:
//! `tests/invariants.rs` runs them over the checked-in fixtures, and
//! `camello dev check` runs them over anything at all — which is how a defect gets
//! found in the first place, before it is minimised into a fixture.

pub mod deparse;

use crate::lang::{TokenExt, TokenKind};
use crate::{format_perl, parse_perl};

/// Whose defect a violation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Subject {
    /// Asked of the input alone: a failure is in the parser.
    Parser,
    /// Asked of an input against its output: a failure is in the formatter.
    Formatter,
}

impl Subject {
    /// The heading this group is reported under.
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            Subject::Parser => "the parser, asked of the input",
            Subject::Formatter => "the formatter, asked of input against output",
        }
    }
}

/// One property of the parser or the formatter, asked of one input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Invariant {
    /// The input parses without a diagnostic.
    CleanParse,
    /// The tree is a faithful copy of the input: its tokens reproduce the
    /// source byte for byte, and no node's range begins or ends on trivia.
    NormalForm,
    /// Re-lexing input and output yields the same non-trivia token sequence.
    SemanticPreservation,
    /// Input and output hold the same comment texts, in the same order.
    CommentPreservation,
    /// Verbatim content is reproduced byte for byte.
    VerbatimPreservation,
    /// The output is a fixed point: `format(format(x)) == format(x)`, and the
    /// layout decisions read back out of it are the ones the input gave.
    Idempotency,
    /// perl reads the output as the program the input was. Opt-in: it runs
    /// perl, and `perl -c` runs the `BEGIN` blocks of the file being checked.
    Deparse,
}

impl Invariant {
    /// Every invariant, in the order they are reported: by group, and within a
    /// group, prerequisites first.
    pub const ALL: &'static [Invariant] = &[
        Invariant::CleanParse,
        Invariant::NormalForm,
        Invariant::SemanticPreservation,
        Invariant::CommentPreservation,
        Invariant::VerbatimPreservation,
        Invariant::Idempotency,
    ];

    /// The invariants that are not asked unless they are asked for by name.
    ///
    /// Everything in [`Invariant::ALL`] is a question camello can answer about
    /// itself. These need something else — here, a perl — and running a perl
    /// over a file means running the file's `BEGIN` blocks, which is not
    /// something a checker may do to somebody's corpus without being told to.
    pub const OPT_IN: &'static [Invariant] = &[Invariant::Deparse];

    /// Every invariant there is, asked-by-default or not.
    pub fn every() -> impl Iterator<Item = Invariant> {
        Invariant::ALL
            .iter()
            .chain(Invariant::OPT_IN.iter())
            .copied()
    }

    /// Does asking this invariant run perl?
    #[must_use]
    pub fn needs_perl(self) -> bool {
        matches!(self, Invariant::Deparse)
    }

    /// Whose defect a violation of this invariant is.
    #[must_use]
    pub fn subject(self) -> Subject {
        match self {
            Invariant::CleanParse | Invariant::NormalForm => Subject::Parser,
            Invariant::SemanticPreservation
            | Invariant::CommentPreservation
            | Invariant::VerbatimPreservation
            | Invariant::Idempotency
            | Invariant::Deparse => Subject::Formatter,
        }
    }

    /// The name used in reports.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Invariant::CleanParse => "a clean parse",
            Invariant::NormalForm => "the tree's normal form",
            Invariant::SemanticPreservation => "semantic preservation",
            Invariant::CommentPreservation => "comment preservation",
            Invariant::VerbatimPreservation => "verbatim preservation",
            Invariant::Idempotency => "idempotency",
            Invariant::Deparse => "the deparse oracle",
        }
    }

    /// A short slug, for filtering on a command line.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Invariant::CleanParse => "clean-parse",
            Invariant::NormalForm => "normal-form",
            Invariant::SemanticPreservation => "semantics",
            Invariant::CommentPreservation => "comments",
            Invariant::VerbatimPreservation => "verbatim",
            Invariant::Idempotency => "idempotency",
            Invariant::Deparse => "deparse",
        }
    }

    /// What the invariant asks, and why a reader should care that it failed.
    ///
    /// `--list-invariants` is where someone who just watched a slug scroll past
    /// goes to find out what it meant, so the answer lives here rather than in
    /// a doc comment they would have to read the source to see.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Invariant::CleanParse => {
                "Parsing the input reports no diagnostic. Every other invariant is about \
                 what the formatter did with a tree, and a tree built by error recovery is \
                 not one it was asked to handle — so a file that fails this is reported as \
                 that and nothing else, and the rest are left unanswered."
            }
            Invariant::NormalForm => {
                "The tree is a faithful copy of the input, in two respects. Its tokens, \
                 concatenated, reproduce the source byte for byte — a byte the parser \
                 dropped is one the formatter cannot print back. And no node's range begins \
                 or ends on trivia: trivia belongs between nodes, and a node that swallows a \
                 comment or a blank line makes every rule keyed on that node's extent — is \
                 this multi-line, is there a newline after the bracket — depend on where the \
                 trivia happened to land."
            }
            Invariant::SemanticPreservation => {
                "Re-lexing the output yields the same non-trivia tokens as the input — same \
                 kinds, same texts, same order. This is the one that says the formatter \
                 moved whitespace and nothing else; a failure here is code that changed \
                 meaning."
            }
            Invariant::CommentPreservation => {
                "The output holds the same comment texts as the input, in the same order. \
                 Comments are trivia to the parser and attached by hand to the layout, \
                 which makes them the easiest thing in a file to lose or reorder."
            }
            Invariant::VerbatimPreservation => {
                "Content that is not the formatter's to touch — string and regex bodies, \
                 heredoc bodies, POD, __DATA__ — comes out byte for byte, and every such \
                 token in the output is still found in the input. The second half catches a \
                 literal that absorbed something the formatter inserted beside it."
            }
            Invariant::Idempotency => {
                "The output is where formatting stops, in two respects. Formatting it again \
                 changes nothing: format(format(x)) == format(x). And the layout seeds read \
                 back out of it are the ones the input gave — camello breaks a group because \
                 the input put a newline after the opening bracket, never because a line got \
                 long, so a group the input had broken must re-read as broken. The seeds are \
                 the cause and the text is the symptom: seeds that differ while the text \
                 holds still are a shape that will move on some later edit."
            }
            Invariant::Deparse => {
                "perl reads the output as the program the input was: both compile, and \
                 B::Deparse renders them the same. This is the only check that asks anything \
                 outside camello, and it is the only one that can see what a token stream \
                 cannot — ${^MATCH} against ${^ MATCH} is one token sequence and two \
                 different variables, and a comment that migrated into a replacement string \
                 is code that still lexes. Opt-in with --deparse, and note what that opts \
                 into: `perl -c` runs the BEGIN blocks of every file it is pointed at."
            }
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

/// What asking a set of invariants of one source came to.
///
/// Three answers and not two: an invariant can also go unanswered, and a report
/// that folds that into "passed" is a report that says a corpus is clean when
/// most of it was never looked at.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// The invariants that were asked and found wanting.
    pub violations: Vec<Violation>,
    /// The invariants that were selected but could not be answered here, and
    /// why. A file that does not parse cleanly leaves the formatter questions
    /// unanswered; a file perl will not load leaves the oracle's.
    pub unanswered: Vec<Unanswered>,
}

/// One invariant that could not be answered for one input.
#[derive(Debug, Clone)]
pub struct Unanswered {
    pub invariant: Invariant,
    /// The class, which is what a run's counts are grouped by.
    pub why: &'static str,
    /// What the thing that declined actually said. Empty where there is
    /// nothing to add — a parse failure is already reported on its own row.
    pub detail: String,
}

/// Ask every invariant of `source` that camello can answer by itself.
///
/// A file that does not parse cleanly is reported as that and nothing else: the
/// other five questions are about what the formatter did with a tree, and a
/// tree built by error recovery is not one the formatter was asked to handle.
#[must_use]
pub fn check(source: &str) -> Vec<Violation> {
    check_only(source, Invariant::ALL)
}

/// [`check_report`], keeping only what was violated.
#[must_use]
pub fn check_only(source: &str, wanted: &[Invariant]) -> Vec<Violation> {
    check_report(source, wanted).violations
}

/// Ask only these invariants, and do only their work.
///
/// The filtering used to happen after the fact, so `--only clean-parse` parsed
/// the file, formatted it twice, re-lexed both, and then threw the other answers
/// away — asking one question of a corpus cost several times what it should.
///
/// Naming an invariant from [`Invariant::OPT_IN`] here is what opting in means:
/// nothing reaches for a perl unless the caller put one of those in `wanted`.
#[must_use]
pub fn check_report(source: &str, wanted: &[Invariant]) -> Outcome {
    let asked = |invariant: Invariant| wanted.contains(&invariant);
    let mut outcome = Outcome::default();

    // Not a question anyone can opt out of: every answer below is about what the
    // formatter did with a tree, and a tree built by error recovery is not one
    // it was asked to handle. Even when the caller selected another invariant,
    // report this prerequisite instead of calling the file clean.
    if let Some(violation) = clean_parse(source) {
        outcome.violations.push(violation);
        outcome.unanswered = wanted
            .iter()
            .filter(|invariant| **invariant != Invariant::CleanParse)
            .map(|invariant| Unanswered {
                invariant: *invariant,
                why: "no clean parse",
                detail: String::new(),
            })
            .collect();
        return outcome;
    }

    if asked(Invariant::NormalForm) {
        outcome.violations.extend(normal_form(source));
    }

    let formats = [
        Invariant::SemanticPreservation,
        Invariant::CommentPreservation,
        Invariant::VerbatimPreservation,
        Invariant::Idempotency,
        Invariant::Deparse,
    ];
    if !formats.iter().copied().any(asked) {
        return outcome;
    }

    let (formatted, _) = format_perl(source);

    if asked(Invariant::SemanticPreservation) {
        outcome
            .violations
            .extend(semantic_preservation(source, &formatted));
    }
    if asked(Invariant::CommentPreservation) {
        outcome
            .violations
            .extend(comment_preservation(source, &formatted));
    }
    if asked(Invariant::VerbatimPreservation) {
        outcome
            .violations
            .extend(verbatim_preservation(source, &formatted));
    }
    if asked(Invariant::Idempotency) {
        outcome.violations.extend(idempotency(source, &formatted));
    }
    if asked(Invariant::Deparse) {
        match deparse::meaning(source, &formatted) {
            deparse::Verdict::Same => {}
            deparse::Verdict::NotLoadable { why, detail } => outcome.unanswered.push(Unanswered {
                invariant: Invariant::Deparse,
                why,
                detail,
            }),
            deparse::Verdict::Differs { summary, detail } => {
                outcome
                    .violations
                    .push(violation(Invariant::Deparse, summary, detail))
            }
        }
    }

    outcome
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

/// The tree is a faithful copy of the input: it holds every byte, and it holds
/// the trivia between nodes rather than inside them.
///
/// One invariant and not two, because the two failures have one address — a
/// parser that built the wrong tree — and a reader who has to be told which of
/// the eight slugs belongs to the parser has been given a taxonomy to memorise
/// instead of an answer.
fn normal_form(source: &str) -> Option<Violation> {
    let root = parse_perl(source).0;
    losslessness(source, &root).or_else(|| trivia_placement(&root))
}

fn losslessness(source: &str, root: &crate::lang::SyntaxNode) -> Option<Violation> {
    let rebuilt: String = root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .map(|token| token.text().to_string())
        .collect();
    if rebuilt == source {
        return None;
    }

    // The byte counts alone say a byte went missing and leave the reader to
    // find it; the file is on their disk and the position is not.
    let at = source
        .bytes()
        .zip(rebuilt.bytes())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| source.len().min(rebuilt.len()));
    let line = source[..at.min(source.len())].lines().count().max(1);
    Some(violation(
        Invariant::NormalForm,
        format!(
            "the tree's tokens do not reproduce the source ({} bytes in, {} out)",
            source.len(),
            rebuilt.len()
        ),
        format!(
            "first divergence at byte {at}, line {line}\n  input:  {}\n  tree:   {}",
            elide(window(source, at)),
            elide(window(&rebuilt, at))
        ),
    ))
}

fn trivia_placement(root: &crate::lang::SyntaxNode) -> Option<Violation> {
    let mut offenders = Vec::new();
    for node in root.descendants() {
        // ROOT covers the file, trailing newline and all.
        if &node == root {
            continue;
        }
        // `first_token`/`last_token` walk the node's own edge; asking a
        // `descendants_with_tokens` iterator for its last item walks the whole
        // subtree, once per node, for a token that is always at the edge.
        let Some(first) = node.first_token() else {
            continue;
        };
        let last = node.last_token().unwrap_or_else(|| first.clone());

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
            Invariant::NormalForm,
            format!("{} node(s) span trivia", offenders.len()),
            offenders.join("\n"),
        )
    })
}

/// The output is where formatting stops: reformatting it changes nothing, and
/// the layout decisions read back out of it are the ones the input gave.
///
/// The seeds are the cause and the text is the symptom, so they are one
/// question. Camello breaks a group because the input put a newline after the
/// opening bracket, never because a line got long, and the seeds are that
/// reading — so seeds that disagree are a second pass that will lay the file out
/// differently, whether or not this particular file's text moved.
fn idempotency(source: &str, formatted: &str) -> Option<Violation> {
    let (twice, _) = format_perl(formatted);
    let seeds_in = crate::fmt::layout_seeds(source);
    let seeds_out = crate::fmt::layout_seeds(formatted);

    if twice == *formatted && seeds_in == seeds_out {
        return None;
    }

    let seeds = (seeds_in != seeds_out).then(|| {
        let render = |broken: &bool| {
            (
                String::new(),
                if *broken { "broken" } else { "flat" }.to_string(),
            )
        };
        format!(
            "the layout decisions differ: {} group(s) in, {} out\n{}",
            seeds_in.len(),
            seeds_out.len(),
            describe_divergence(&seeds_in, &seeds_out, &Report::stream("group"), render)
        )
    });

    let text = (twice != *formatted).then(|| {
        let pass1: Vec<&str> = formatted.lines().collect();
        let pass2: Vec<&str> = twice.lines().collect();
        let report = Report {
            unit: "line",
            sides: ("pass 1", "pass 2"),
            base: 1,
        };
        describe_divergence(&pass1, &pass2, &report, |line| {
            (String::new(), (*line).to_string())
        })
    });

    let summary = match (&text, &seeds) {
        (Some(_), Some(_)) => "format(format(x)) != format(x), and the layout seeds moved",
        (Some(_), None) => "format(format(x)) != format(x)",
        // Worth reporting on its own: the text held still because nothing in
        // this file rendered the changed decision differently. The next edit to
        // it need not be so lucky.
        (None, _) => "the text is a fixed point, the layout decisions are not",
    };

    Some(violation(
        Invariant::Idempotency,
        summary.to_string(),
        [seeds, text]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n"),
    ))
}

/// The bytes around `at`, for a report that has to point at a position.
fn window(text: &str, at: usize) -> &str {
    let start = (0..=at.min(text.len()))
        .rev()
        .find(|&index| text.is_char_boundary(index) && at - index >= 20)
        .unwrap_or(0);
    let end = (at.min(text.len())..=text.len())
        .find(|&index| text.is_char_boundary(index) && index - at >= 40)
        .unwrap_or(text.len());
    &text[start..end]
}

fn semantic_preservation(source: &str, formatted: &str) -> Option<Violation> {
    let before = token_stream(source);
    let after = token_stream(formatted);
    (before != after).then(|| {
        violation(
            Invariant::SemanticPreservation,
            format!("{} tokens in, {} out", before.len(), after.len()),
            describe_divergence(&before, &after, &Report::stream("token"), |(kind, text)| {
                (kind.clone(), text.clone())
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
                (String::new(), text.clone())
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

/// The non-trivia token sequence of `source`, as `(kind, text)` pairs.
///
/// Lexing is parser-driven — `expect` comes from the grammar (the lexer contract) — so
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
///
/// `render_item` gives back the item in two parts: whatever labels it, which is
/// always printed, and its text, which is not — a deparsed line or a `__DATA__`
/// token is thousands of characters of which the report can carry a hundred and
/// twenty, and it is this function that knows which hundred and twenty.
fn describe_divergence<T: PartialEq>(
    before: &[T],
    after: &[T],
    report: &Report<'_>,
    render_item: impl Fn(&T) -> (String, String),
) -> String {
    let Report { unit, sides, base } = *report;
    let Some(position) =
        (0..before.len().max(after.len())).find(|&index| before.get(index) != after.get(index))
    else {
        return "streams are equal".to_string();
    };

    // The two items that disagree may be long and disagree late — two deparsed
    // lines differing in a line number four hundred bytes in. A window taken
    // from the front of those shows the reader four hundred bytes of the part
    // that matched. Both sides are cut at the same place, so what is on the
    // screen is the same stretch of two texts.
    let focus = match (
        before.get(position).map(&render_item),
        after.get(position).map(&render_item),
    ) {
        (Some((_, left)), Some((_, right))) => left
            .bytes()
            .zip(right.bytes())
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| left.len().min(right.len())),
        _ => 0,
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
                let (label, text) = render_item(item);
                // Context items are cut from the front: they agree, so there is
                // nothing in them to point at.
                let at = if index == position { focus } else { 0 };
                let space = if label.is_empty() { "" } else { " " };
                format!(
                    "{marker} [{}] {label}{space}{}",
                    index + base,
                    window_on(&text, at)
                )
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

/// `text`, quoted, as [`ELIDE_AT`] bytes of it around `at`.
///
/// The window sits a third of the way in rather than in the middle: the place
/// two texts stop agreeing is where reading starts, so most of the room goes to
/// what follows it. Where anything was left out the report says which bytes
/// these are, because a reader who wants the rest has the file.
fn window_on(text: &str, at: usize) -> String {
    if text.len() <= ELIDE_AT {
        return format!("{text:?}");
    }

    let mut start = at.saturating_sub(ELIDE_AT / 3).min(text.len() - ELIDE_AT);
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + ELIDE_AT).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }

    format!(
        "{:?} (bytes {start}..{end} of {})",
        &text[start..end],
        text.len()
    )
}

#[cfg(test)]
mod tests {
    use super::{check_only, describe_divergence, elide, idempotency, Invariant, Report, ELIDE_AT};

    #[test]
    fn a_parse_error_is_reported_even_when_another_invariant_was_selected() {
        let violations = check_only("my $x = ;\n", &[Invariant::CommentPreservation]);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].invariant, Invariant::CleanParse);
    }

    /// The half of idempotency that a text comparison cannot see.
    ///
    /// Handed an output whose layout decisions are not the input's, the check
    /// has to report it even though that output is a perfectly good fixed point
    /// of the formatter — that is the whole reason the seeds are consulted
    /// rather than inferred from the text.
    #[test]
    fn a_stable_text_with_moving_seeds_is_still_a_violation() {
        let violation = idempotency("f(\n    1\n);\n", "f(1);\n").expect("seeds moved");
        assert_eq!(violation.invariant, Invariant::Idempotency);
        assert!(
            violation.summary.contains("the layout decisions are not"),
            "{}",
            violation.summary
        );
        assert!(
            violation.detail.contains("first divergence at group #"),
            "{}",
            violation.detail
        );
    }

    #[test]
    fn a_report_carries_the_divergence_and_not_the_stream() {
        let before: Vec<String> = (0..500).map(|index| format!("# {index}")).collect();
        let mut after = before.clone();
        after.remove(200);

        let detail = describe_divergence(&before, &after, &Report::stream("comment"), |text| {
            (String::new(), text.clone())
        });

        assert!(detail.contains("first divergence at comment #200"));
        assert!(detail.contains("\"# 200\""));
        assert!(!detail.contains("\"# 300\""));
        assert!(detail.lines().count() < 20);
    }

    /// The point of a window: two long items that agree for longer than a
    /// report is wide are shown where they stop agreeing, not where they start.
    #[test]
    fn a_divergence_late_in_a_long_item_is_the_part_that_is_shown() {
        let before = vec![format!(
            "{}NEEDLE-BEFORE{}",
            "x".repeat(400),
            "y".repeat(400)
        )];
        let after = vec![format!(
            "{}NEEDLE-AFTER{}",
            "x".repeat(400),
            "y".repeat(400)
        )];

        let detail = describe_divergence(&before, &after, &Report::stream("line"), |text| {
            (String::new(), text.clone())
        });

        assert!(detail.contains("NEEDLE-BEFORE"), "{detail}");
        assert!(detail.contains("NEEDLE-AFTER"), "{detail}");
        assert!(detail.contains("bytes 3"), "{detail}");
        // And what it shows is still a window, not the item.
        assert!(detail.len() < before[0].len(), "{detail}");
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
