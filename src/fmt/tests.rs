//! Formatter tests, including the six reproduced formatting bugs from
//! `notes/2026-07-28-redesign-assessment.md` appendix A (F1-F6), and the
//! invariants of ADR 0008 §6.

use std::time::Instant;

use super::{format_source, FormatterOptions};
use crate::lang::TokenKind;

fn format(source: &str) -> String {
    format_source(source, &FormatterOptions::default())
}

/// `format(format(x)) == format(x)` (ADR 0008 §6).
#[track_caller]
fn assert_idempotent(source: &str) {
    let once = format(source);
    let twice = format(&once);
    assert_eq!(
        once, twice,
        "formatting is not idempotent\n--- pass 1 ---\n{once}--- pass 2 ---\n{twice}"
    );
}

/// The non-trivia token sequence must survive formatting (ADR 0008 §6).
fn tokens(source: &str) -> Vec<(TokenKind, String)> {
    use crate::lang::TokenExt;
    crate::parse::parse(source)
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.token_kind().is_trivia())
        .map(|token| (token.token_kind(), token.text().to_string()))
        .collect()
}

#[track_caller]
fn assert_preserves_semantics(source: &str) {
    let formatted = format(source);
    assert_eq!(
        tokens(source),
        tokens(&formatted),
        "formatting changed the token stream\n--- input ---\n{source}--- output ---\n{formatted}"
    );
}

#[track_caller]
fn assert_formats_to(source: &str, expected: &str) {
    assert_eq!(format(source), expected);
    assert_idempotent(source);
    assert_preserves_semantics(source);
}

// ===== F1: no indentation is injected into a string literal =====

#[test]
fn multiline_string_literals_are_left_alone() {
    // The old formatter indented the *contents* of a string that began a line,
    // and did it again on every pass, so the string grew without bound. Here a
    // string literal is a `Raw` atom and the renderer has no way to write inside
    // one (ADR 0008 §2).
    let source = "sub f {\nmy $s = \"line1\nline2\";\n}\n";
    let formatted = format(source);
    assert!(
        formatted.contains("\"line1\nline2\""),
        "the literal must survive byte for byte:\n{formatted}"
    );
    assert_idempotent(source);
    assert_preserves_semantics(source);

    // And it stays put however many passes are applied.
    let mut text = source.to_string();
    for _ in 0..5 {
        text = format(&text);
    }
    assert!(text.contains("\"line1\nline2\""), "{text}");
}

// ===== F2: a crammed subroutine formats to a fixed point in one pass =====

#[test]
fn crammed_subroutine_formats_in_one_pass() {
    assert_formats_to(
        "sub f{my $x=shift;return $x+1}\n",
        "sub f {\n    my $x = shift;\n    return $x + 1\n}\n",
    );
}

// ===== F3: alignment happens on the first pass =====

#[test]
fn assignments_align_on_the_first_pass() {
    // The old alignment pass required a NEWLINE in the *source* to start a
    // group, so a one-line input only aligned once the first pass had inserted
    // the newlines. The align pass now sees rendered columns and nothing else.
    assert_formats_to(
        "my $x=1;my $yy=2;my $zzz=3;\n",
        "my $x   = 1;\nmy $yy  = 2;\nmy $zzz = 3;\n",
    );
}

#[test]
fn alignment_groups_break_on_shape_and_blank_lines() {
    assert_formats_to(
        "my $x = 1;\nmy $yy = 2;\n\n$z = 3;\n$wwww = 4;\n",
        "my $x  = 1;\nmy $yy = 2;\n\n$z    = 3;\n$wwww = 4;\n",
    );
}

#[test]
fn alignment_is_its_own_fixed_point() {
    // I3 of ADR 0008 §6: padding is spaces, so it creates no new anchor.
    let aligned = "my $x   = 1;\nmy $yy  = 2;\nmy $zzz = 3;\n";
    assert_eq!(format(aligned), aligned);
}

// ===== F4: alignment is linear, not quadratic =====

#[test]
fn alignment_does_not_reformat_to_measure() {
    // 800 lines took 30 seconds under the old formatter, which measured widths
    // by re-running itself. This is a shape check, not a benchmark: quadratic
    // behaviour shows up as the ratio, and a generous bound keeps it from being
    // flaky on a loaded machine.
    let build = |count: usize| {
        (0..count)
            .map(|index| format!("my $var{index} = {index};\n"))
            .collect::<String>()
    };

    let time = |source: &str| {
        let start = Instant::now();
        let _ = format(source);
        start.elapsed().as_secs_f64()
    };

    let small = time(&build(200)).max(1e-4);
    let large = time(&build(800));

    assert!(
        large < small * 40.0,
        "formatting 800 lines took {large:.3}s against {small:.3}s for 200; \
         that ratio looks super-linear"
    );
}

// ===== F5: a comment inside a multi-line list adds no blank line =====

#[test]
fn comment_at_the_start_of_a_broken_list_adds_no_blank_line() {
    let source = "foo(\n    # first\n    1,\n    2,\n);\n";
    let formatted = format(source);
    assert!(
        !formatted.contains("\n\n"),
        "no blank line should appear:\n{formatted}"
    );
    assert_idempotent(source);
    assert_preserves_semantics(source);
}

// ===== F6: a comment before the brace does not push it to its own line =====

#[test]
fn comment_before_a_block_keeps_the_brace_on_its_line() {
    let source = "if ($x)    # why\n{\n    print 1;\n}\n";
    let formatted = format(source);
    // K&R: the brace stays on the `if` line, and the comment that sat before it
    // moves after it, because the brace is the formatter's to place.
    let first = formatted.lines().next().unwrap_or_default();
    assert_eq!(first, "if ($x) { # why", "{formatted}");
    assert!(
        !formatted.contains("\n\n"),
        "and no blank line is introduced:\n{formatted}"
    );
    assert_idempotent(source);
}

// ===== Layout rules (formatting.md) =====

#[test]
fn blocks_of_control_structures_always_break() {
    assert_formats_to(
        "if ($x) { print 1; } else { print 2; }\n",
        "if ($x) {\n    print 1;\n} else {\n    print 2;\n}\n",
    );
}

#[test]
fn map_and_sub_blocks_may_stay_on_one_line() {
    assert_formats_to(
        "my @xs = map { $_ * 2 } @ys;\n",
        "my @xs = map { $_ * 2 } @ys;\n",
    );
}

#[test]
fn a_bracket_group_breaks_when_the_source_broke_it() {
    // The seed rule of ADR 0008 §3: a newline straight after the opening
    // bracket. Broken output has that newline itself, so re-formatting reaches
    // the same decision (I2).
    assert_formats_to("foo(1, 2);\n", "foo(1, 2);\n");
    assert_formats_to("foo(\n1,\n2,\n);\n", "foo(\n    1,\n    2,\n);\n");
}

#[test]
fn user_newlines_inside_an_expression_are_kept_and_indented() {
    assert_idempotent("my $x = 1\n+ 2;\n");
    assert_preserves_semantics("my $x = 1\n+ 2;\n");
}

#[test]
fn nested_fat_commas_align_per_depth() {
    let source = "my %h = (\n    a => 1,\n    bbb => { x => 1, yy => 2 },\n);\n";
    assert_idempotent(source);
    assert_preserves_semantics(source);
}

// ===== Verbatim regions (formatting.md VERBATIM) =====

#[test]
fn verbatim_regions_survive_untouched() {
    for source in [
        "print <<EOF;\n  body   with spacing\nEOF\nmy $x = 1;\n",
        "=head1 NAME\n\n   indented pod\n\n=cut\n\nmy $x = 1;\n",
        "my $x = 1;\n__DATA__\n   raw ; stuff\n",
        "$x =~ s{  a  }{  b  }g;\n",
        "my @w = qw(  a   b  );\n",
    ] {
        assert_idempotent(source);
        assert_preserves_semantics(source);
    }
}

// ===== Comments (formatting.md COMMENT) =====

#[test]
fn comments_keep_their_line_and_their_ownership() {
    assert_formats_to(
        "# own line\nmy $x = 1;   # trailing\n",
        "# own line\nmy $x = 1; # trailing\n",
    );
}

#[test]
fn trailing_comments_align_as_a_group() {
    let formatted = format("my $x = 1;  # a\nmy $yy = 2; # b\n");
    let columns: Vec<usize> = formatted
        .lines()
        .filter_map(|line| line.find('#'))
        .collect();
    assert_eq!(
        columns.first(),
        columns.last(),
        "trailing comments in one group share a column: {formatted}"
    );
}

#[test]
fn the_minimum_space_before_a_comment_is_one_option() {
    // The old formatter had two comment paths — one hard-coding four spaces, one
    // copying the source's whitespace — and the option reached only one.
    let options = FormatterOptions {
        min_spaces_before_comment: 3,
        ..FormatterOptions::default()
    };
    let formatted = format_source("my $x = 1; # note\n", &options);
    assert!(formatted.contains("1;   # note"), "{formatted}");
}

// ===== Blank lines (formatting.md BLANK_LINE) =====

#[test]
fn consecutive_blank_lines_collapse_to_one() {
    assert_formats_to(
        "my $x = 1;\n\n\n\nmy $y = 2;\n",
        "my $x = 1;\n\nmy $y = 2;\n",
    );
}

// ===== The invariants, over a wider corpus =====

#[test]
fn invariants_hold_across_a_representative_corpus() {
    for source in [
        "package Foo;\nuse strict;\n\nsub greet ($name) {\n    print \"hi $name\\n\";\n}\n\n1;\n",
        "for my $i (1 .. 10) {\n    next if $i % 2;\n    print $i;\n}\n",
        "my $h = {\n    alpha => 1,\n    beta  => [1, 2, 3],\n};\n",
        "try {\n    risky();\n} catch ($e) {\n    warn $e;\n} finally {\n    cleanup();\n}\n",
        "my @sorted = sort { $a <=> $b } @values;\n",
        "$obj->method(1)->chained->{key}[0];\n",
        "print STDERR \"oops\\n\" unless $ok;\n",
        "sub f($$) { }\n",
        "BEGIN { $x = 1; }\n",
        "LOOP: while ($i < 10) {\n    last LOOP if $done;\n}\n",
    ] {
        assert_idempotent(source);
        assert_preserves_semantics(source);
    }
}

#[test]
fn malformed_input_still_formats_to_a_fixed_point() {
    // Error recovery must not produce output that changes on the next pass.
    for source in [
        "sub f {\n",
        "my $x = ;\n",
        "q{unterminated\n",
        "use A use X;\n",
    ] {
        assert_idempotent(source);
    }
}

// ===== Fixture snapshots =====

/// Format every fixture and snapshot the result.
///
/// These are the spec-by-example: `formatting.md` says what the rules are, and
/// these say what they produce.
#[test]
fn fixture_snapshots() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fmt/fixtures");
    let mut files = Vec::new();
    collect(&directory, &mut files);
    files.sort();
    assert!(!files.is_empty(), "no fixtures found in {directory:?}");

    for path in files {
        let name = path
            .strip_prefix(&directory)
            .expect("fixture is under the fixture directory")
            .iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("__");
        let source = std::fs::read_to_string(&path).expect("failed to read fixture");
        insta::assert_snapshot!(name, format(&source));
    }
}

fn collect(directory: &std::path::Path, into: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("failed to read fixture directory") {
        let path = entry.expect("failed to read fixture entry").path();
        if path.is_dir() {
            collect(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "pl") {
            into.push(path);
        }
    }
}
