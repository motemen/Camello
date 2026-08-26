//! Formatter tests, including layout regressions and formatter invariants.

use std::time::Instant;

use super::{format_source, FormatterOptions};
use crate::lang::TokenKind;

fn format(source: &str) -> String {
    format_source(source, &FormatterOptions::default())
}

/// `format(format(x)) == format(x)` (the formatter contract).
#[track_caller]
fn assert_idempotent(source: &str) {
    let once = format(source);
    let twice = format(&once);
    assert_eq!(
        once, twice,
        "formatting is not idempotent\n--- pass 1 ---\n{once}--- pass 2 ---\n{twice}"
    );
}

/// The non-trivia token sequence must survive formatting (the formatter contract).
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
    // one (the formatter contract).
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
    // I3 of the formatter contract: padding is spaces, so it creates no new anchor.
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
    assert_eq!(first, "if ($x) {    # why", "{formatted}");
    assert!(
        !formatted.contains("\n\n"),
        "and no blank line is introduced:\n{formatted}"
    );
    assert_idempotent(source);
}

// ===== Layout rules (docs/formatting.md) =====

#[test]
fn blocks_of_control_structures_always_break() {
    assert_formats_to(
        "if ($x) { print 1; } else { print 2; }\n",
        "if ($x) {\n    print 1;\n} else {\n    print 2;\n}\n",
    );
}

#[test]
fn catch_with_keeps_its_exception_class() {
    assert_formats_to(
        "try { risky(); } catch ePortal::Exception::Fatal with { my $error = shift; }\n",
        "try {\n    risky();\n} catch ePortal::Exception::Fatal with {\n    my $error = shift;\n}\n",
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
    // The seed rule of the formatter contract: a newline straight after the opening
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
fn a_comment_before_a_deferred_method_call_semicolon_is_a_continuation() {
    assert_formats_to(
        "$obj->method\n# comment\n;\n",
        "$obj->method\n    # comment\n    ;\n",
    );
}

#[test]
fn nested_fat_commas_align_per_depth() {
    let source = "my %h = (\n    a => 1,\n    bbb => { x => 1, yy => 2 },\n);\n";
    assert_idempotent(source);
    assert_preserves_semantics(source);
}

#[test]
fn fat_commas_in_adjacent_flat_nested_hashes_align() {
    assert_formats_to(
        "+{\n    a   => { aaa => 1 },\n    bbb => { b   => 2 },\n};\n",
        "+{\n    a   => { aaa => 1 },\n    bbb => { b   => 2 },\n};\n",
    );
}

#[test]
fn bareword_call_arguments_hang_from_the_first_argument() {
    assert_formats_to(
        "args my $class => 'A',\n     my $arg   => 'B';\n",
        "args my $class => 'A',\n     my $arg   => 'B';\n",
    );
}

// ===== Verbatim regions (docs/formatting.md VERBATIM) =====

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

#[test]
fn caret_variables_are_never_split() {
    // `${^ MATCH}` is a different variable from `${^MATCH}`. The name is one
    // token, so there is nowhere for a space to go — and if that ever changed,
    // the token stream would differ and `assert_preserves_semantics` would say
    // so.
    for source in [
        "${^MATCH};\n",
        "my $phase = ${^GLOBAL_PHASE};\n",
        "my @caps = @{^CAPTURE};\n",
        "my $warning = $^W;\n",
    ] {
        let formatted = format(source);
        assert!(
            !formatted.contains("^ "),
            "the caret must stay attached to its name: {formatted}"
        );
        assert_idempotent(source);
        assert_preserves_semantics(source);
    }
}

#[test]
fn postfix_dereference_binds_tight() {
    // `->@*` is an arrow with its target glued on, so nothing goes between the
    // subject and it. A postfix slice is a subscript and takes a subscript's
    // spacing: it holds a list, so it opens up (SPACING-7), the same as the
    // `@x[ 0, 1 ]` it is another spelling of.
    assert_formats_to("$r->@*;\n", "$r->@*;\n");
    assert_formats_to("$r->%*;\n", "$r->%*;\n");
    assert_formats_to("$r->$#*;\n", "$r->$#*;\n");
    assert_formats_to("$r->@[0,1];\n", "$r->@[ 0, 1 ];\n");
    assert_formats_to("$r->%{a,b};\n", "$r->%{ a, b };\n");
}

// ===== Comments (docs/formatting.md COMMENT) =====

#[test]
fn comments_keep_their_line_and_their_ownership() {
    assert_formats_to(
        "# own line\nmy $x = 1;   # trailing\n",
        "# own line\nmy $x = 1;    # trailing\n",
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
fn alignment_padding_is_capped() {
    // The cap exists so one very long line cannot push a whole group across the
    // screen (issue #273). It shipped with no test at all, which is how a DoS
    // guard stops being one.
    //
    // It is a property of the group: aligned on the widest member, or not
    // aligned at all. Capping each line's own padding instead left a group in
    // three different columns — aligned by no measure, and stable, so nothing
    // ever took it back.
    let source = "my $short = 1;\nmy $this_name_is_very_much_longer_than_the_others = 2;\n";

    let with_cap = |cap: usize| {
        format_source(
            source,
            &FormatterOptions {
                max_alignment_padding: cap,
                ..FormatterOptions::default()
            },
        )
    };
    let generous = with_cap(100);
    let capped = with_cap(4);

    let columns = |text: &str| -> Vec<usize> {
        text.lines()
            .filter_map(|line| line.find('='))
            .collect::<Vec<_>>()
    };
    let long_name = "my $this_name_is_very_much_longer_than_the_others ".len();
    assert_eq!(columns(&generous), vec![long_name, long_name]);
    // Over the cap, so neither line is padded — and in particular they do not
    // end up in two columns.
    assert_eq!(columns(&capped), vec!["my $short ".len(), long_name]);

    // Capping must not cost idempotency: the second pass has to reach the same
    // columns, not add another four spaces.
    let options = FormatterOptions {
        max_alignment_padding: 4,
        ..FormatterOptions::default()
    };
    assert_eq!(format_source(&capped, &options), capped);

    // Zero means "do not align", and stays that way.
    let none = FormatterOptions {
        max_alignment_padding: 0,
        ..FormatterOptions::default()
    };
    let unaligned = format_source(source, &none);
    assert_eq!(columns(&unaligned), vec!["my $short ".len(), long_name]);
    assert_eq!(format_source(&unaligned, &none), unaligned);

    // Within the cap, every member of the group agrees on one column.
    let pair = "my $x = 1;\nmy $longer_name = 2;\n";
    let aligned = format_source(pair, &FormatterOptions::default());
    let expected = "my $longer_name ".len();
    assert_eq!(columns(&aligned), vec![expected, expected]);
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

// ===== Blank lines (docs/formatting.md BLANK_LINE) =====

#[test]
fn consecutive_blank_lines_collapse_to_one() {
    assert_formats_to(
        "my $x = 1;\n\n\n\nmy $y = 2;\n",
        "my $x = 1;\n\nmy $y = 2;\n",
    );
}

#[test]
fn definitions_and_phase_blocks_stand_apart() {
    // docs/formatting.md BLANK_LINE-1. The blank line at the top of the file and the
    // one that would land straight after `{` are dropped, so the rule can be
    // stated without exceptions at the point it is applied.
    assert_formats_to(
        "my $x=1;\nsub foo { return 1; }\nmy $y=2;\n",
        "my $x = 1;\n\nsub foo {\n    return 1;\n}\n\nmy $y = 2;\n",
    );
    assert_formats_to(
        "BEGIN{$a=1;}INIT{$b=2;}\n",
        "BEGIN {\n    $a = 1;\n}\n\nINIT {\n    $b = 2;\n}\n",
    );
    // A blank line before `__DATA__`, and none introduced inside a block.
    assert_formats_to(
        "my $x = 1;\n__DATA__\nraw\n",
        "my $x = 1;\n\n__DATA__\nraw\n",
    );
    // A block that stays on one line gains no blank lines: there is no line for
    // one to go on.
    assert_formats_to("sub f { sub g { 1 } }\n", "sub f { sub g { 1 } }\n");
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

// ===== The 2026-07-28 review: formatting that destroyed code =====

#[test]
fn a_comment_never_swallows_the_code_after_it() {
    // P0-1. The group holding a comment was flat, so everything after the
    // comment was appended to its line — and a line after `#` is not code.
    // `my %h = ( # c\n a => 1,\n);` formatted to `my %h = ( # ca => 1,);`.
    for source in [
        "my %h = ( # c\n  a => 1,\n);\n",
        "foo(1, # c\n  2);\n",
        "foo(1 # c\n);\n",
        "my $x = [ # c\n 1,\n];\n",
        "my $h = { a => 1 # c\n};\n",
        "outer(inner(1 # deep\n), 2);\n",
    ] {
        let formatted = format(source);
        for line in formatted.lines() {
            let Some(hash) = line.find('#') else { continue };
            assert!(
                !line[hash..].contains([';', ',']),
                "code was appended to a comment line:\n{formatted}"
            );
        }
        assert_idempotent(source);
        assert_preserves_semantics(source);
        assert_preserves_comments(source);
    }
}

#[test]
fn a_comment_after_a_substitution_stays_outside_it() {
    // P0-3. The empty replacement list of `s/a//` is zero width, so it starts
    // where the closing delimiter starts and an offset-keyed lookup handed both
    // of them the same trailing comment. The comment was emitted twice, once
    // inside the literal: "delete a" became "replace a with ' # c'".
    let source = "$x =~ s/a//   # c\n  || 1;\n";
    let formatted = format(source);
    assert!(
        formatted.starts_with("$x =~ s/a//    # c\n"),
        "the comment must stay outside the replacement:\n{formatted}"
    );
    assert_preserves_comments(source);
    assert_preserves_semantics(source);
    assert_idempotent(source);

    for source in [
        "$x =~ s{a}{}; # c\n",
        "$x =~ m//; # c\n",
        "my @w = qw(); # c\n",
        "my $q = q(); # c\n",
    ] {
        assert_preserves_comments(source);
        assert_preserves_semantics(source);
        assert_idempotent(source);
    }
}

#[test]
fn a_quote_like_run_is_never_spaced_out() {
    // P0-2. A DELIMITER belongs to a quote-like run wherever it is found. When a
    // misparse left the run inside an ERROR node the run's own node was gone,
    // the tightness rule keyed on it stopped firing, and a space appeared beside
    // content the next pass re-lexed as part of the literal — so the literal
    // grew by two characters per pass.
    for source in [
        "use Exporter 5.57 qw( import );\n",
        "$v =~ s/xx\\z//;\n",
        "my @x = grep { $_ }qw(a b);\n",
        "print \"x\" if any { $_ } qw(a b);\n",
    ] {
        let mut text = source.to_string();
        for _ in 0..4 {
            text = format(&text);
        }
        assert_eq!(
            text,
            format(source),
            "the output kept growing across passes:\n{text}"
        );
        assert!(
            !text.contains("qw ("),
            "the delimiter was spaced away from its keyword:\n{text}"
        );
        assert_preserves_semantics(source);
    }
}

#[test]
fn what_the_parser_could_not_read_is_copied_out_unchanged() {
    // The last resort of P0-2: every layout rule is a rule about a construct the
    // parser recognised, and inside an ERROR node it recognised none of them.
    for source in [
        "use A use X;\n",
        "my $x = 1 + ;\nmy $y = 2;\n",
        "sub f {\n",
        "q{unterminated\n",
    ] {
        assert_idempotent(source);
    }
}

#[test]
fn a_header_comment_and_a_brace_comment_both_survive() {
    // F6 moves a comment written before the brace to after it, because the brace
    // does not move. When the brace already carries one of its own, the two
    // cannot share a line — the second would be inside the first — so the later
    // one takes a line inside the block. Which is later is the order they were
    // written in: `DBI::DBD::SqlEngine` writes one sentence across the two, and
    // had it come back in reverse.
    let formatted =
        format("if ($y)    # about the condition\n{ # about the block\n    print 1;\n}\n");
    assert_eq!(
        formatted,
        "if ($y) {    # about the condition\n    # about the block\n    print 1;\n}\n"
    );
    assert_idempotent(&formatted);
}

#[track_caller]
fn assert_preserves_comments(source: &str) {
    let comments = |text: &str| {
        use crate::lang::TokenExt;
        crate::parse::parse(text)
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.token_kind() == TokenKind::COMMENT)
            .map(|token| token.text().trim_end().to_string())
            .collect::<Vec<_>>()
    };
    let formatted = format(source);
    assert_eq!(
        comments(source),
        comments(&formatted),
        "the comments changed\n--- input ---\n{source}--- output ---\n{formatted}"
    );
}

// ===== The 2026-07-28 review: robustness =====

#[test]
fn nested_blocks_do_not_take_exponential_time() {
    // P1-1. Twenty nested `sub {` — forty characters — took over ninety
    // seconds. Each level re-answered "can this be flat" for every level below
    // it, and each answer allocated the node's whole text to search it for a
    // newline. Two hundred levels is ten times deeper and finishes instantly.
    let source = format!("{}1{};\n", "sub { ".repeat(200), " }".repeat(200));
    let start = Instant::now();
    let _ = format(&source);
    let elapsed = start.elapsed().as_secs_f64();
    assert!(
        elapsed < 5.0,
        "200 nested blocks took {elapsed:.3}s; that is not linear"
    );
}

#[test]
fn input_deeper_than_the_limit_is_reported_and_kept() {
    // P1-2 and P1-3. A thousand open parentheses aborted the process — the step
    // limit was a `panic!` in release builds too — and the formatter's own
    // recursive walk ran out of stack a little deeper. It is a diagnostic now,
    // what could not be parsed is copied out unchanged, and the output is still
    // a fixed point.
    for source in [
        format!("{}1{};\n", "(".repeat(2000), ")".repeat(2000)),
        format!("{}{};\n", "[".repeat(2000), "]".repeat(2000)),
        format!("{}1{};\n", "sub { ".repeat(2000), " }".repeat(2000)),
    ] {
        let (formatted, errors) = crate::format_perl(&source);
        assert!(!errors.is_empty(), "the limit must be reported");
        assert_eq!(
            formatted.chars().filter(|ch| !ch.is_whitespace()).count(),
            source.chars().filter(|ch| !ch.is_whitespace()).count(),
            "nothing may be dropped when the limit is reached"
        );
        assert_idempotent(&source);
    }
}

#[test]
fn verbatim_content_keeps_its_trailing_whitespace() {
    // P1-5. `finish_line` trimmed every line, including the ones that end
    // inside a raw atom, where the whitespace is content. Two corpus modules
    // deparsed differently because of it.
    let source = "my $re = qr/\n  a  \n  b\n/x;\n";
    assert!(
        format(source).contains("  a  \n"),
        "the pattern lost characters: {}",
        format(source)
    );
    assert_idempotent(source);

    // And the blank lines at the end of a `__DATA__` section, which
    // `while (<DATA>)` counts.
    let source = "my $x = 1;\n__DATA__\nline1\n\n\n";
    assert!(
        format(source).ends_with("line1\n\n\n"),
        "{:?}",
        format(source)
    );
    assert_idempotent(source);
}

#[test]
fn verbatim_regions_start_in_column_zero() {
    // P1-6. `__END__` and `=head1` are recognised at a line start and nowhere
    // else (the lexer contract), so a region that begins inside an open block still
    // begins in column 0 — and indenting it produces output that no longer has
    // a data section in it.
    let source = "sub f {\n    print 1;\n__END__\nrest\n";
    let formatted = format(source);
    assert!(
        formatted.contains("\n__END__\nrest\n"),
        "the region was indented out of existence:\n{formatted}"
    );
    assert_idempotent(source);

    let source = "sub f {\n=head1 X\n\ndoc\n\n=cut\n    print 1;\n}\n";
    assert!(format(source).contains("\n=head1 X"), "{}", format(source));
    assert_idempotent(source);

    // The other half of the same rule: indented, the marker is an ordinary
    // word. Moving it to column 0 to match the kind the lexer gave it would
    // turn the rest of the enclosing block into data.
    let source = "sub f {\n    print 1;\n    __END__\n}\ntail\n";
    assert!(
        format(source).contains("    __END__\n}"),
        "an indented marker is a word and stays where it is:\n{}",
        format(source)
    );
    assert_idempotent(source);
    assert_preserves_semantics(source);
}

#[test]
fn a_format_declaration_is_carried_through_untouched() {
    // P1-7. `@<<<<` is a left-justified field five characters wide. Parsed as
    // an expression it came out as `@< << <`, which is four things that mean
    // nothing, and nothing was reported.
    let source = "format STDOUT =\n@<<<<< @>>>>>\n$a,    $b\n.\n\nprint 1;\n";
    let formatted = format(source);
    assert!(
        formatted.contains("@<<<<< @>>>>>\n$a,    $b\n.\n"),
        "the picture lines were rewritten:\n{formatted}"
    );
    assert_idempotent(source);
    assert_preserves_semantics(source);

    // And `format` is only a keyword where a declaration follows it.
    assert_formats_to("my %h = (format => 1);\n", "my %h = (format => 1);\n");
    assert_formats_to("my $x = $o->format($y);\n", "my $x = $o->format($y);\n");
    assert_formats_to("sub format { 1 }\n", "sub format { 1 }\n");
}

// ===== I2: the seed rule reproduces its own output =====

#[track_caller]
fn assert_seed_stable(source: &str) {
    let formatted = format(source);
    assert_eq!(
        super::layout_seeds(source),
        super::layout_seeds(&formatted),
        "the layout decisions differ between passes\n--- input ---\n{source}--- output ---\n{formatted}"
    );
}

#[test]
fn layout_decisions_are_stable_across_passes() {
    for source in [
        "foo(1, 2);\n",
        "foo(\n1,\n2,\n);\n",
        "my %h = (a => 1, b => 2);\n",
        "my %h = (\n a => 1,\n b => 2,\n);\n",
        "sub f { 1 }\n",
        "sub f {\n    my $x = shift;\n    return $x;\n}\n",
        "if ($x) { print 1; } else { print 2; }\n",
        "my @xs = map { $_ * 2 } grep { $_ } @ys;\n",
        "my $r = [\n1,\n[2, 3],\n];\n",
    ] {
        assert_seed_stable(source);
    }
}

// ===== `use` alignment, which is off by default =====

/// A run of `use` lines is a two-column table — the module, then what is taken
/// from it — and under `align_use_imports` it is laid out as one.
#[test]
fn use_imports_align_only_when_asked() {
    let source = "use Foo::Bar qw(f);\nuse Foo::BazBaz qw(g h i);\nuse Foo::Q ();\n";

    // The default leaves one space, which is what a repository adopting camello
    // gets whether or not it wants this.
    assert_eq!(format(source), source);

    let options = FormatterOptions {
        align_use_imports: true,
        ..FormatterOptions::default()
    };
    let aligned = format_source(source, &options);
    assert_eq!(
        aligned,
        "use Foo::Bar    qw(f);\nuse Foo::BazBaz qw(g h i);\nuse Foo::Q      ();\n"
    );
    // And the pass is its own fixed point (the formatter contract, I3).
    assert_eq!(format_source(&aligned, &options), aligned);
}

/// The group ends where any alignment group does: at a blank line, at a line
/// with nothing to align, and at a different kind of statement.
#[test]
fn use_alignment_groups_end_where_alignment_groups_end() {
    let options = FormatterOptions {
        align_use_imports: true,
        ..FormatterOptions::default()
    };
    let source = concat!(
        "use strict;\n",
        "use Foo::A qw(a);\n",
        "use Foo::BBBB qw(b);\n",
        "\n",
        "use Foo::CC qw(c);\n",
        "no Foo::DDDD qw(d);\n",
    );
    assert_eq!(
        format_source(source, &options),
        concat!(
            // `use strict;` imports nothing, so there is no column to agree on
            // and the group starts after it.
            "use strict;\n",
            "use Foo::A    qw(a);\n",
            "use Foo::BBBB qw(b);\n",
            "\n",
            // A blank line ends the group. A `no` does not: it is written in
            // the same block and read as part of the same table.
            "use Foo::CC  qw(c);\n",
            "no Foo::DDDD qw(d);\n",
        )
    );
}

// ===== Fixture snapshots =====

/// Format every fixture and snapshot the result.
///
/// These are the spec-by-example: `docs/formatting.md` says what the rules are, and
/// these say what they produce.
///
/// `regressions/` is excluded. Those fixtures carry their own expected output
/// as an A→B pair (`tests/invariants.rs`), and a snapshot beside it would be a
/// second answer to the same question — one generated from what the formatter
/// does, which is exactly what a regression fixture must not take on trust.
#[test]
fn fixture_snapshots() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fmt/fixtures");
    let mut files = Vec::new();
    collect(&directory, &mut files);
    files.retain(|path| !path.starts_with(directory.join("regressions")));
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
        } else if path.extension().is_some_and(|extension| extension == "pl")
            && !path.to_string_lossy().ends_with(".expected.pl")
        {
            into.push(path);
        }
    }
}
