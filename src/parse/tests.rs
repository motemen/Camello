//! Parser tests: the CST normal form, error recovery, and the trivia model.

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

/// Concatenating the tree's tokens must reproduce the source (the trivia model).
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

// ===== CST normal form (the parser contract) =====

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
    // the language model: the operator is one token, so there is nothing to wrap.
    let rendered = tree("$x += 1;");
    assert!(rendered.contains("`+=`"), "{rendered}");
    assert!(rendered.contains("ASSIGN_EXPR"));
}

// ===== Precedence (the parser contract) =====

#[test]
fn file_test_binds_at_named_unary_precedence() {
    // perl groups this as `-f ($x . "y")`. The old table reused the prefix
    // level and grouped it the other way.
    insta::assert_snapshot!(tree("-f $x . \"y\";\n"));
}

#[test]
fn bitwise_binds_looser_than_comparison() {
    // As perlop orders it; the current binding powers intentionally follow Perl.
    insta::assert_snapshot!(tree("$a & $b == $c;\n"));
}

// ===== Speculative parsing (the parser contract) =====

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

// ===== Error recovery: the acceptance criteria of the parser contract =====

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

// ===== Losslessness and trivia =====

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
    // The placement rule of the trivia model. Without it, "does this node span more
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
    // the trivia model: a comment on the same line as the previous token belongs to
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

// ===== Fixture snapshots =====

/// Parse every fixture and snapshot the tree, or the diagnostics for the
/// `errors/` ones.
#[test]
fn fixture_snapshots() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/parse/fixtures");
    let mut files = Vec::new();
    collect(&directory, &mut files);
    files.sort();
    assert!(!files.is_empty(), "no fixtures found in {directory:?}");

    for path in files {
        let relative = path
            .strip_prefix(&directory)
            .expect("fixture is under the fixture directory");
        let name = relative
            .iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("__");
        let source = std::fs::read_to_string(&path).expect("failed to read fixture");

        // For an error fixture the diagnostics are the point; for the rest the
        // tree is.
        let is_error = relative
            .components()
            .any(|part| part.as_os_str() == "errors");
        let parsed = parse(&source);
        let rendered = if is_error {
            let diagnostics = parsed.diagnostics;
            assert!(
                !diagnostics.is_empty(),
                "error fixture {} produced no diagnostic",
                relative.display()
            );
            diagnostics
                .iter()
                .map(|diagnostic| {
                    let line = source[..usize::from(diagnostic.range.start())]
                        .lines()
                        .count();
                    format!("line {line}: {}", diagnostic.message)
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            assert!(
                parsed.diagnostics.is_empty(),
                "success fixture {} produced diagnostics: {:#?}",
                relative.display(),
                parsed.diagnostics
            );
            tree(&source)
        };

        insta::assert_snapshot!(name, rendered);
    }
}

#[test]
fn the_builtin_table_knows_every_name_perl_does() {
    // The table is generated from perl's own prototypes
    // (`scripts/generate-builtins`). Written from memory it had holes, and the
    // holes were not harmless: with `eval` missing, the parser fell through to
    // "unknown name, expect an operator", `eval <<EOT` lexed as a left shift,
    // and the heredoc body became code.
    use super::grammar::builtins::{lookup, Shape};

    for name in [
        "eval",
        "wantarray",
        "chomp",
        "select",
        "readline",
        "prototype",
    ] {
        assert!(lookup(name).is_some(), "`{name}` is missing from the table");
    }
    assert_eq!(lookup("wantarray").map(|b| b.shape), Some(Shape::Nullary));
    assert_eq!(lookup("eval").map(|b| b.shape), Some(Shape::BlockOrTerm));
    assert_eq!(lookup("defined").map(|b| b.shape), Some(Shape::NamedUnary));
    assert_eq!(lookup("push").map(|b| b.shape), Some(Shape::List));
    assert!(lookup("frobnicate").is_none());
}

#[test]
fn eval_takes_a_heredoc_and_a_block() {
    let rendered = tree("my $d = eval <<EOT;\nhello\nEOT\n");
    assert!(
        rendered.contains("HEREDOC_EXPR"),
        "`eval <<EOT` must read the heredoc marker, not a left shift:\n{rendered}"
    );
    assert!(
        rendered.contains(r#""<<EOT""#),
        "the marker must be one token, not a shift and a bareword:\n{rendered}"
    );

    let rendered = tree("my $r = eval { 1 };\n");
    assert!(
        rendered.contains("BLOCK") && !rendered.contains("ANON_HASH"),
        "`eval BLOCK` is a block, never an anonymous hash:\n{rendered}"
    );
}

#[test]
fn a_limit_is_a_diagnostic_and_not_an_abort() {
    // P1-2 and P1-3. The step limit was a `panic!` — in release builds too — so
    // a thousand open parentheses aborted the process, and a little deeper the
    // formatter's own recursive walk ran out of stack. Neither is a bug in the
    // input: generated code and fuzzers produce it, and a formatter's answer is
    // a diagnostic.
    for source in [
        format!("{}1{};\n", "(".repeat(2000), ")".repeat(2000)),
        format!("{}{};\n", "[".repeat(2000), "]".repeat(2000)),
        format!("{}1{};\n", "sub {{ ".repeat(2000), " }}".repeat(2000)),
    ] {
        let parsed = parse(&source);
        assert!(
            !parsed.diagnostics.is_empty(),
            "reaching a limit must be reported"
        );
        // And nothing is lost: what could not be parsed is still in the tree.
        let rebuilt: String = parsed
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .map(|token| token.text().to_string())
            .collect();
        assert_eq!(rebuilt, source, "the tail of the input was dropped");
    }
}

#[test]
fn a_speculative_parse_leaves_no_stale_lookahead() {
    // P1-4. `anon_hash_or_block` tries the hash reading, scans the `}` in
    // operator position, rolls back, and re-parses as a block — reaching that
    // same `}` in term position without ever changing `expect`, so nothing
    // invalidated it. Debug builds asserted; release builds consumed a token
    // scanned under the wrong state.
    for source in ["foo{sub}", "sub(@y^", "t{,**t", "foo{sub}; bar{s}"] {
        let parsed = parse(source);
        let rebuilt: String = parsed
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .map(|token| token.text().to_string())
            .collect();
        assert_eq!(rebuilt, source);
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
