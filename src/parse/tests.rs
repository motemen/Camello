//! Parser tests: the CST normal form (ADR 0007 §2), error recovery
//! (ADR 0007 §3) and the trivia model (ADR 0006).

use crate::lang::{NodeKind, SyntaxNode};

use super::parse;

/// An indented S-expression view of the tree, trivia elided.
fn tree(source: &str) -> String {
    fn render(node: &SyntaxNode, depth: usize, out: &mut String) {
        out.push_str(&"  ".repeat(depth));
        out.push_str(&format!("{}\n", node.kind()));
        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(node) => render(&node, depth + 1, out),
                rowan::NodeOrToken::Token(token) if !token.kind().is_trivia() => {
                    out.push_str(&"  ".repeat(depth + 1));
                    out.push_str(&format!("{} {:?}\n", token.kind(), token.text()));
                }
                rowan::NodeOrToken::Token(_) => {}
            }
        }
    }

    let parsed = parse(source);
    let mut out = String::new();
    render(&parsed.syntax(), 0, &mut out);
    out
}

fn errors(source: &str) -> Vec<String> {
    parse(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Concatenating the tree's tokens must reproduce the source (ADR 0006 §6).
#[track_caller]
fn assert_lossless(source: &str) {
    let parsed = parse(source);
    let rebuilt: String = parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .map(|token| token.text().to_string())
        .collect();
    assert_eq!(rebuilt, source, "tree is not lossless");
}

fn nodes_of_kind(source: &str, kind: NodeKind) -> usize {
    parse(source)
        .syntax()
        .descendants()
        .filter(|node| node.kind() == crate::lang::SyntaxKind::from(kind))
        .count()
}

// ===== CST normal form (ADR 0007 §2) =====

#[test]
fn expression_statements_are_always_wrapped() {
    // No bare `STMT`, and no "wrapped sometimes" — every statement is a closed
    // node of a known kind.
    insta::assert_snapshot!(tree("$x = 1;\nfoo();\n"));
}

#[test]
fn declarations_get_their_own_statement_kind() {
    insta::assert_snapshot!(tree("my ($a, $b) = @_;\n"));
}

#[test]
fn list_expr_is_generated_even_for_one_element() {
    // The old parser produced the wrapper only when a comma was present, so
    // every consumer downstream had to handle both shapes.
    assert_eq!(nodes_of_kind("foo(1);", NodeKind::LIST_EXPR), 2);
    assert_eq!(nodes_of_kind("foo(1, 2);", NodeKind::LIST_EXPR), 2);
    assert_eq!(nodes_of_kind("foo();", NodeKind::LIST_EXPR), 2);
}

#[test]
fn calls_are_split_by_shape() {
    insta::assert_snapshot!(tree(
        "foo(1);\nprint STDERR \"x\";\nmap { $_ } @xs;\n$obj->method(2);\n"
    ));
}

#[test]
fn operator_classes_are_distinguished() {
    // `BINARY_EXPR` vs `ASSIGN_EXPR` vs `RANGE_EXPR`, rather than everything
    // being an INFIX_EXPR.
    insta::assert_snapshot!(tree("$a = $b + $c * $d;\n$e //= 1 .. 5;\n"));
}

#[test]
fn compound_assignment_has_no_wrapper_node() {
    // ADR 0004 §4: the operator is one token, so there is nothing to wrap.
    let rendered = tree("$x += 1;");
    assert!(rendered.contains("`+=`"), "{rendered}");
    assert!(rendered.contains("ASSIGN_EXPR"));
}

// ===== Precedence (ADR 0007 §4) =====

#[test]
fn file_test_binds_at_named_unary_precedence() {
    // perl groups this as `-f ($x . "y")`. The old table reused the prefix
    // level and grouped it the other way.
    insta::assert_snapshot!(tree("-f $x . \"y\";\n"));
}

#[test]
fn bitwise_binds_looser_than_comparison() {
    // As perlop orders it. See L-003 in the deviation log for why this differs
    // from the parenthetical in ADR 0007 §4.
    insta::assert_snapshot!(tree("$a & $b == $c;\n"));
}

// ===== Speculative parsing (ADR 0007 §1) =====

#[test]
fn anon_hash_wins_over_block_when_it_parses() {
    // `{ $k => 1 }` was a block under the old first-token heuristic.
    let rendered = tree("my $h = { $k => 1 };");
    assert!(
        rendered.contains("ANON_HASH"),
        "expected an anonymous hash, got:\n{rendered}"
    );
    let quote_like = tree("my $h = { qq{x} => 1 };");
    assert!(
        quote_like.contains("ANON_HASH"),
        "lookahead must see through the quote-like operator, got:\n{quote_like}"
    );
}

#[test]
fn signature_and_prototype_are_told_apart_by_trying() {
    let signature = tree("sub f($x, $y = 1) { }");
    assert!(signature.contains("SUB_SIGNATURE"), "{signature}");

    for prototype in ["($$)", "(_)", "(+)", "(\\[$@])"] {
        let rendered = tree(&format!("sub f{prototype} {{ }}"));
        assert!(
            rendered.contains("SUB_PROTOTYPE"),
            "{prototype} should be a prototype, got:\n{rendered}"
        );
        assert!(
            errors(&format!("sub f{prototype} {{ }}")).is_empty(),
            "{prototype} is legal perl and must not produce diagnostics"
        );
    }
}

// ===== Error recovery: the acceptance criteria of ADR 0007 §3 =====

#[test]
fn direct_subscription_after_call_reports_two_errors() {
    let source = "f(){k};\nf()[0];\n";
    let errors = errors(source);
    assert_eq!(
        errors.len(),
        2,
        "two mistakes must produce two diagnostics, not six: {errors:#?}"
    );
    assert_lossless(source);
}

#[test]
fn invalid_signature_reports_one_error_per_bad_parameter() {
    let source = "\
sub bad_number ($1) { }
sub bad_digits ($123) { }
sub bad_negative ($-foo) { }
sub bad_plus_equals ($value += 1) { }
sub bad_low_precedence_or ($x = 1 or 2) { }
sub bad_low_precedence_and ($x = 1 and 2) { }
";
    let errors = errors(source);
    assert_eq!(
        errors.len(),
        6,
        "six mistakes must produce six diagnostics, not eleven: {errors:#?}"
    );
    assert_lossless(source);
}

#[test]
fn missing_semicolon_does_not_swallow_the_next_statement() {
    // The old parser reported once and then ate the second `use` as an ERROR
    // token, so the file silently lost a statement.
    let source = "use A use X;\n";
    let errors = errors(source);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(
        errors[0].contains(';'),
        "the diagnostic should point at the missing `;`: {errors:#?}"
    );

    let rendered = tree(source);
    assert_eq!(
        rendered.matches("USE_STMT").count(),
        2,
        "both `use` statements must survive:\n{rendered}"
    );
    assert_lossless(source);
}

#[test]
fn diagnostics_do_not_leak_internal_names() {
    for source in ["sub f {", "my $x = ;", "foo(", "if ($x {"] {
        for message in errors(source) {
            assert!(
                !message.contains("R_BRACE") && !message.contains("SyntaxKind"),
                "{source:?} produced an internal name: {message}"
            );
        }
    }
}

// ===== Losslessness and trivia (ADR 0006) =====

#[test]
fn trees_are_lossless() {
    for source in [
        "my $x = 1;\n",
        "# leading comment\nsub f {\n    return 1;    # trailing\n}\n\n",
        "print <<EOF;\nbody\nEOF\n",
        "=head1 NAME\n\ntext\n\n=cut\n\nmy $x = 1;\n",
        "my $x = 1;\n__DATA__\nraw\n",
        "$x =~ s{a}{b}g;\n",
        "q{unterminated\n",
    ] {
        assert_lossless(source);
    }
}

#[test]
fn node_ranges_never_include_trivia() {
    // The placement rule of ADR 0006 §4. Without it, "does this node span more
    // than one line" depends on where the trivia happened to land.
    let source = "sub f {\n\n    my $x = 1;   # note\n\n}\n";
    let parsed = parse(source);
    for node in parsed.syntax().descendants() {
        // ROOT covers the file, trailing newline and all.
        if node.kind() == crate::lang::SyntaxKind::from(NodeKind::ROOT) {
            continue;
        }
        let text = node.text().to_string();
        if text.is_empty() {
            continue;
        }
        assert_eq!(
            text.trim_start(),
            text,
            "{} starts with trivia: {text:?}",
            node.kind()
        );
        assert_eq!(
            text.trim_end(),
            text,
            "{} ends with trivia: {text:?}",
            node.kind()
        );
    }
}

#[test]
fn trivia_is_attributed_by_line() {
    // ADR 0006 §3: a comment on the same line as the previous token belongs to
    // it; an own-line comment belongs to what follows.
    let source = "my $x = 1;  # trailing\n# own line\nmy $y = 2;\n";
    let parsed = parse(source);

    let tokens: Vec<_> = parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();

    let semicolon = tokens
        .iter()
        .find(|token| token.text() == ";")
        .expect("first semicolon");
    assert!(
        parsed
            .trivia
            .at(semicolon.text_range().start())
            .has_comment(),
        "the trailing comment belongs to the token it shares a line with"
    );

    let second_my = tokens
        .iter()
        .filter(|token| token.text() == "my")
        .nth(1)
        .expect("second declaration");
    let trivia = parsed.trivia.at(second_my.text_range().start());
    assert!(
        trivia
            .leading
            .iter()
            .any(|item| item.kind == crate::lang::TokenKind::COMMENT),
        "the own-line comment belongs to the statement below it"
    );
}

#[test]
fn blank_lines_are_counted_from_trivia() {
    let source = "my $x = 1;\n\n\nmy $y = 2;\n";
    let parsed = parse(source);
    let second_my = parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.text() == "my")
        .nth(1)
        .expect("second declaration");
    assert_eq!(
        parsed
            .trivia
            .at(second_my.text_range().start())
            .blank_lines_before(),
        2
    );
}

// ===== A broad smoke test =====

#[test]
fn parses_a_representative_program() {
    let source = "\
package Foo::Bar;

use strict;
use warnings;

sub greet ($name, $greeting = 'hello') {
    my %seen;
    for my $word (split /\\s+/, $name) {
        next if $seen{$word}++;
        print STDERR \"$greeting, $word\\n\";
    }
    return wantarray ? keys %seen : scalar keys %seen;
}

1;
";
    let diagnostics = errors(source);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_lossless(source);
    insta::assert_snapshot!(tree(source));
}
