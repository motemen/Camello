use crate::format;
use crate::formatter::spacing::{self, SpacingContext};
use crate::parse_perl;
use crate::SyntaxKind;

/// Helper to parse, format, and assert no parse errors.
fn format_and_assert(input: &str) -> String {
    let (syntax, err) = parse_perl(input);
    assert!(err.is_empty(), "Parse errors for '{}': {:?}", input, err);
    format(&syntax)
}

/// Helper function to reduce code duplication in formatting tests
pub fn check_formatting_cases(cases: &[(&str, &str)]) {
    for (input, expected) in cases {
        let formatted = format_and_assert(input);
        assert_eq!(
            formatted, *expected,
            "Formatting failed for input: '{}'",
            input
        );
    }
}

#[test]
fn test_all_var_decl_types_formatting() {
    let cases = [
        ("my $x = 1;", "my $x = 1;\n"),
        ("our $x = 2;", "our $x = 2;\n"),
        ("state $x = 3;", "state $x = 3;\n"),
        ("local $x = 4;", "local $x = 4;\n"),
        ("my@arr=(1,2,3);", "my @arr = (1, 2, 3);\n"),
        ("our%hash=(a=>1);", "our %hash = (a => 1);\n"),
        ("state($x,$y)=(1,2);", "state ($x, $y) = (1, 2);\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_for_stmt_formatting() {
    let input = "for my$var(@list){my$x=1;print$x;}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        for my $var (@list) {
            my $x = 1;
            print $x;
        }
        ");
}

#[test]
fn test_nested_loop_with_complex_conditions() {
    let input = "while($a+$b*$c){for(@array){print;}}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        while ($a + $b * $c) {
            for (@array) {
                print;
            }
        }
        ");
}

#[test]
fn test_comment_formatting() {
    let input = r#" 
sub test {
    my $x = 1;
# a comment
    my $y = 2;
}
"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        sub test {
            my $x = 1;
            # a comment
            my $y = 2;
        }
        ");
}

#[test]
fn test_if_else_stmt_formatting() {
    let input = "if($condition){do_something();}else{do_something_else();}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        if ($condition) {
            do_something();
        } else {
            do_something_else();
        }
        ");
}

#[test]
fn test_unless_stmt_formatting() {
    let input = "unless($condition){do_something();}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        unless ($condition) {
            do_something();
        }
        ");
}

#[test]
fn test_postfix_conditional_formatting() {
    let cases = [
        // Postfix if tests
        ("return $x if $x > $y;", "return $x if $x > $y;\n"),
        ("print \"hello\" if $debug;", "print \"hello\" if $debug;\n"),
        (
            "my $result = calculate() if $do_calc;",
            "my $result = calculate() if $do_calc;\n",
        ),
        // Postfix unless tests
        ("return $x unless $x > $y;", "return $x unless $x > $y;\n"),
        (
            "print \"hello\" unless $quiet;",
            "print \"hello\" unless $quiet;\n",
        ),
        (
            "die \"Error\" unless defined $result;",
            "die \"Error\" unless defined $result;\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_inline_comment_preservation() {
    let cases = [
        // Inline comments should stay on the same line
        (
            "my $x = 1; # inline comment",
            "my $x = 1; # inline comment\n",
        ),
        ("print $var; # debug output", "print $var; # debug output\n"),
        (
            "return 42; # return the answer",
            "return 42; # return the answer\n",
        ),
        // Block comments should remain on their own line
        (
            "my $x = 1;\n# block comment",
            "my $x = 1;\n# block comment\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_nested_eval_in_sub() {
    let input = "sub f{eval{print$x;};return 1;}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        sub f {
            eval {
                print $x;
            };
            return 1;
        }
        ");
}

#[test]
fn test_version_use_statements_formatting() {
    check_formatting_cases(&[
        // v-prefixed versions (current support)
        ("use v5.24.1;", "use v5.24.1;\n"),
        ("use v5.008_001;", "use v5.008_001;\n"),
        ("use v5.36;", "use v5.36;\n"),
        // Bare version formats (new support)
        ("use 5.24.1;", "use 5.24.1;\n"),
        ("use 5.008_001;", "use 5.008_001;\n"),
        ("use 5.36.0;", "use 5.36.0;\n"),
        // Simple version numbers
        ("use 5;", "use 5;\n"),
        ("use 5.24;", "use 5.24;\n"),
        // With spacing variations
        ("use  v5.24.1 ;", "use v5.24.1;\n"),
        ("use  5.24.1 ;", "use 5.24.1;\n"),
        ("use\tv5.24.1\t;", "use v5.24.1;\n"),
        ("use\t5.24.1\t;", "use 5.24.1;\n"),
    ]);
}

#[test]
fn test_method_call_formatting() {
    let cases = [
        ("$obj->method($a,$b);", "$obj->method($a, $b);\n"),
        (
            "my$result=$obj->calculate();",
            "my $result = $obj->calculate();\n",
        ),
        (
            "$obj->get()->set($value)->save();",
            "$obj->get()->set($value)->save();\n",
        ),
        ("func()->method();", "func()->method();\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_multiline_function_call_with_complex_args_formatting() {
    let input = r#"complex_func(
    $var1 + $var2,
    "string argument",
    42,
    $obj->method()
);"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r#"
        complex_func(
            $var1 + $var2,
            "string argument",
            42,
            $obj->method()
        );
        "#);
}

#[test]
fn test_nested_multiline_function_calls_formatting() {
    let input = r#"outer_func(
    inner_func(
        nested_arg1,
        nested_arg2
    ),
    other_arg
);"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        outer_func(
            inner_func(
                nested_arg1,
                nested_arg2
            ),
            other_arg
        );
        ");
}

#[test]
fn test_subscription_vs_ref_access() {
    // Test that both direct subscription and ref access work correctly
    let input =
        "my $a = $hash{key}; my $b = $hashref->{key}; my $c = $array[0]; my $d = $arrayref->[0];";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @r"
        my $a = $hash{key};
        my $b = $hashref->{key};
        my $c = $array[0];
        my $d = $arrayref->[0];
        ");
}

#[test]
fn test_complex_subscription_expressions() {
    let input = "my $val = $hash{$prefix . $suffix}[$array[$index]];";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @"my $val = $hash{$prefix . $suffix}[$array[$index]];");
}

#[test]
fn test_subscription_assignment() {
    let input = "$hash{$key} = $value;";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @"$hash{$key} = $value;");
}

#[test]
fn test_array_subscription_assignment() {
    let input = "$array[$index] = $value;";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @"$array[$index] = $value;");
}

#[test]
fn test_function_call_formatting() {
    let cases = [
        ("push@array,$value;", "push @array, $value;\n"),
        ("print$var,\"hello\",123;", "print $var, \"hello\", 123;\n"),
        ("shift@array;", "shift @array;\n"),
        ("delete$hash{key};", "delete $hash{key};\n"),
        ("my_func$a,$b,$c;", "my_func $a, $b, $c;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_function_call_in_sub() {
    let input = "sub test{push@array,$value;return$result;}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        sub test {
            push @array, $value;
            return $result;
        }
        ");
}

#[test]
fn test_function_call_with_variable_declaration_formatting() {
    let cases = [
        // Basic variable declaration as function argument
        ("foo my $x;", "foo my $x;\n"),
        ("foo my $x, my $y;", "foo my $x, my $y;\n"),
        ("bar our $a;", "bar our $a;\n"),
        ("baz state $s;", "baz state $s;\n"),
        ("qux local $l;", "qux local $l;\n"),
        // Mixed arguments
        (
            "args my $x, my $y => 'Type';",
            "args my $x, my $y => 'Type';\n",
        ),
        ("func my $a, $b, my $c;", "func my $a, $b, my $c;\n"),
        (
            "test my $x, 123, \"string\";",
            "test my $x, 123, \"string\";\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_eval_block_function_formatting() {
    let input = "eval{my$x=1;print$x;};";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        eval {
            my $x = 1;
            print $x;
        };
        ");
}

#[test]
fn test_parenthesized_eval_block_formatting() {
    let input = "(eval {})";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"(eval {})");
}

#[test]
fn test_map_with_parentheses_formatting() {
    let input = "map{$_*2}(1,2,3); sort{$a+$b}@values;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        map { $_ * 2 } (1, 2, 3);
        sort { $a + $b } @values;
        ");
}

#[test]
fn test_single_line_function_call_formatting() {
    let input = "func(arg1, arg2, arg3);";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"func(arg1, arg2, arg3);");
}

#[test]
fn test_ternary_in_data_structures_formatting() {
    let input =
        "my $config = { timeout => $is_production ? 30 : 5, retries => $is_critical ? 3 : 1 };";
    let output = format_and_assert(input);
    insta::assert_snapshot!(output, @"my $config = {timeout => $is_production ? 30 : 5, retries => $is_critical ? 3 : 1};");
}

#[test]
fn test_io_operator_formatting() {
    let cases = [
        // Basic I/O operators
        ("$line = <$fh>;", "$line = <$fh>;\n"),
        ("$data=<FILE>;", "$data = <FILE>;\n"),
        ("my $input = <STDIN>;", "my $input = <STDIN>;\n"),
        ("while (<>) { print; }", "while (<>) {\n    print;\n}\n"),
        (
            "while (<DATA>) { chomp; print; }",
            "while (<DATA>) {\n    chomp;\n    print;\n}\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_original_io_examples() {
    // Test the three examples from the original issue
    let input1 = "while (defined($_ = <STDIN>)) { print; }";
    let formatted1 = format_and_assert(input1);

    let input2 = "while (<>) {\n    print;\n}";
    let formatted2 = format_and_assert(input2);

    let input3 = "$line = <$fh>;";
    let formatted3 = format_and_assert(input3);

    // Just verify they format without errors and contain the I/O operators
    assert!(
        formatted1.contains("<STDIN>"),
        "Example 1 should contain <STDIN>"
    );
    assert!(formatted2.contains("<>"), "Example 2 should contain <>");
    assert!(
        formatted3.contains("<$fh>"),
        "Example 3 should contain <$fh>"
    );

    // Snapshot the results
    insta::assert_snapshot!(formatted1, @r"
        while (defined($_ = <STDIN>)) {
            print;
        }
        ");
    insta::assert_snapshot!(formatted2, @r"
        while (<>) {
            print;
        }
        ");
    insta::assert_snapshot!(formatted3, @"$line = <$fh>;");
}

#[test]
fn test_single_line_hash_ref_formatting() {
    let input = "my $hash = { a => 1, b => 2 };";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"my $hash = {a => 1, b => 2};");
}

#[test]
fn test_multiline_hash_ref_formatting() {
    let input = r#"my $hash = {
    a => 1,
    b => 2
};"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my $hash = {
            a => 1,
            b => 2
        };
        ");
}

#[test]
fn test_single_line_array_ref_formatting() {
    let input = "my $array = [1, 2, 3];";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"my $array = [1, 2, 3];");
}

#[test]
fn test_multiline_array_ref_formatting() {
    let input = r#"my $array = [
    1,
    2, 3
];"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my $array = [
            1,
            2,
            3
        ];
        ");
}

#[test]
fn test_single_line_qw_formatting() {
    let input = "my @words = qw(hello world test);";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"my @words = qw(hello world test);");
}

#[test]
fn test_multiline_qw_formatting() {
    let input = r#"my @words = qw(
    hello
    world
    test
);"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my @words = qw(
            hello
            world
            test
        );
        ");
}

#[test]
fn test_mixed_single_and_multiline() {
    let input = r#"my $mixed = {
    simple => { a => 1, b => 2 },
    complex => {
        nested => [1, 2, 3],
        items => [
            "first",
            "second"
        ]
    }
};"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r#"
        my $mixed = {
            simple => {a => 1, b => 2},
            complex => {
                nested => [1, 2, 3],
                items => [
                    "first",
                    "second"
                ]
            }
        };
        "#);
}

#[test]
fn test_tr_operator_formatting() {
    let input = "$str =~ tr/abc/xyz/;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"$str =~ tr/abc/xyz/;");
}

#[test]
fn test_y_operator_formatting() {
    let input = "$str =~ y/abc/xyz/;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"$str =~ y/abc/xyz/;");
}

#[test]
fn test_tr_with_different_delimiters() {
    let cases = [
        ("$str =~ tr/abc/xyz/;", "$str =~ tr/abc/xyz/;"),
        ("$str =~ tr(abc)(xyz);", "$str =~ tr(abc)(xyz);"),
        ("$str =~ tr[abc][xyz];", "$str =~ tr[abc][xyz];"),
        ("$str =~ tr{abc}{xyz};", "$str =~ tr{abc}{xyz};"),
    ];

    for (input, expected) in cases {
        let formatted = format_and_assert(input);
        assert_eq!(formatted.trim(), expected, "Failed for input: '{}'", input);
    }
}

#[test]
fn test_y_with_different_delimiters() {
    let cases = [
        ("$str =~ y/abc/xyz/;", "$str =~ y/abc/xyz/;"),
        ("$str =~ y(abc)(xyz);", "$str =~ y(abc)(xyz);"),
        ("$str =~ y[abc][xyz];", "$str =~ y[abc][xyz];"),
        ("$str =~ y{abc}{xyz};", "$str =~ y{abc}{xyz};"),
    ];

    for (input, expected) in cases {
        let formatted = format_and_assert(input);
        assert_eq!(formatted.trim(), expected, "Failed for input: '{}'", input);
    }
}

#[test]
fn test_tr_with_flags() {
    let cases = [
        ("$str =~ tr/abc/xyz/d;", "$str =~ tr/abc/xyz/d;"),
        ("$str =~ tr/abc/xyz/c;", "$str =~ tr/abc/xyz/c;"),
        ("$str =~ tr/abc/xyz/s;", "$str =~ tr/abc/xyz/s;"),
        ("$str =~ tr/abc/xyz/cs;", "$str =~ tr/abc/xyz/cs;"),
        ("$str =~ tr/abc/xyz/ds;", "$str =~ tr/abc/xyz/ds;"),
    ];

    for (input, expected) in cases {
        let formatted = format_and_assert(input);
        assert_eq!(formatted.trim(), expected, "Failed for input: '{}'", input);
    }
}

#[test]
fn test_y_with_flags() {
    let cases = [
        ("$str =~ y/abc/xyz/d;", "$str =~ y/abc/xyz/d;"),
        ("$str =~ y/abc/xyz/cs;", "$str =~ y/abc/xyz/cs;"),
    ];

    for (input, expected) in cases {
        let formatted = format_and_assert(input);
        assert_eq!(formatted.trim(), expected, "Failed for input: '{}'", input);
    }
}

#[test]
fn test_tr_complex_patterns() {
    let cases = [
        ("$str =~ tr/a-z/A-Z/;", "$str =~ tr/a-z/A-Z/;"),
        (
            "$str =~ tr/\\x41-\\x5A/a-z/;",
            "$str =~ tr/\\x41-\\x5A/a-z/;",
        ),
        ("$str =~ tr/0-9/*/;", "$str =~ tr/0-9/*/;"),
    ];

    for (input, expected) in cases {
        let formatted = format_and_assert(input);
        assert_eq!(formatted.trim(), expected, "Failed for input: '{}'", input);
    }
}

#[test]
fn test_tr_y_in_context() {
    let input = r#"
        sub process_text {
            my $text = shift;
            $text =~ tr/a-z/A-Z/;
            $text =~ y/0-9/*/d;
            return $text;
        }
        "#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r#"
        sub process_text {
            my $text = shift;
            $text =~ tr/a-z/A-Z/;
            $text =~ y/0-9/*/d;
            return $text;
        }
        "#);
}

#[test]
fn test_binary_operators_need_spaces() {
    let cases = [
        (Some(SyntaxKind::IDENT), SyntaxKind::PLUS),
        (Some(SyntaxKind::PLUS), SyntaxKind::IDENT),
        (Some(SyntaxKind::IDENT), SyntaxKind::EQ),
        (Some(SyntaxKind::EQ), SyntaxKind::IDENT),
    ];

    for (prev, current) in cases {
        let context = SpacingContext {
            prev_token: prev,
            current_token: current,
            at_line_start: false,
        };
        assert!(
            spacing::needs_space_before(&context),
            "Should need space for {:?} -> {:?}",
            prev,
            current
        );
    }
}

#[test]
fn test_arrow_operator_no_spaces() {
    let cases = [
        (Some(SyntaxKind::IDENT), SyntaxKind::ARROW),
        (Some(SyntaxKind::ARROW), SyntaxKind::IDENT),
    ];

    for (prev, current) in cases {
        let context = SpacingContext {
            prev_token: prev,
            current_token: current,
            at_line_start: false,
        };
        assert!(
            !spacing::needs_space_before(&context),
            "Arrow should never have spaces: {:?} -> {:?}",
            prev,
            current
        );
    }
}

#[test]
fn test_logical_not_special_handling() {
    // No space after (
    let context = SpacingContext {
        prev_token: Some(SyntaxKind::L_PAREN),
        current_token: SyntaxKind::LOGICAL_NOT,
        at_line_start: false,
    };
    assert!(!spacing::needs_space_before(&context));

    // Space before ! in other cases
    let context = SpacingContext {
        prev_token: Some(SyntaxKind::IDENT),
        current_token: SyntaxKind::LOGICAL_NOT,
        at_line_start: false,
    };
    assert!(spacing::needs_space_before(&context));

    // No space after !
    let context = SpacingContext {
        prev_token: Some(SyntaxKind::LOGICAL_NOT),
        current_token: SyntaxKind::IDENT,
        at_line_start: false,
    };
    assert!(!spacing::needs_space_before(&context));
}

#[test]
fn test_logical_operators_formatting() {
    let cases = [
        // Logical NOT prefix operator (no space after !)
        ("!$x;", "!$x;\n"),
        ("$a||!$b;", "$a || !$b;\n"),
        ("(!$a&&$b);", "(!$a && $b);\n"),
        // Low-precedence logical operators (space around)
        ("$a and $b;", "$a and $b;\n"),
        ("$x or $y;", "$x or $y;\n"),
        ("$a xor $b;", "$a xor $b;\n"),
        ("not $x;", "not $x;\n"),
        // Defined-or operator
        ("$a//$b;", "$a // $b;\n"),
        ("$x//$y//$z;", "$x // $y // $z;\n"),
        // Spaceship operator
        ("$a<=>$b;", "$a <=> $b;\n"),
        ("$x<=>$y;", "$x <=> $y;\n"),
        // Mixed precedence expressions
        ("$a&&$b||$c;", "$a && $b || $c;\n"),
        ("$a||$b//$c;", "$a || $b // $c;\n"),
        ("$a and $b or $c;", "$a and $b or $c;\n"),
        ("$a&&$b and $c;", "$a && $b and $c;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_complex_logical_expressions_formatting() {
    let input = "$a&&$b||$c and $d or $e xor $f;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"$a && $b || $c and $d or $e xor $f;");
}

#[test]
fn test_logical_operators_with_parentheses() {
    let input = "(!$a&&($b||$c))and($x//$y);";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"(!$a && ($b || $c)) and ($x // $y);");
}

#[test]
fn test_spaceship_in_expressions() {
    let input = "$result=$a<=>$b;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"$result = $a <=> $b;");
}

#[test]
fn test_contextual_logical_keywords() {
    // Test that and, or, etc. are treated as identifiers in non-operator contexts
    let input = "sub and { } my $or = 1;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        sub and {
        }

        my $or = 1;
        ");
}

#[test]
fn test_end_data_section_basic() {
    let input = r#"
my $x = 1;
__DATA__
This is data after __DATA__ $#&!
  Raw string here~
        "#;

    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my $x = 1;
        __DATA__
        This is data after __DATA__ $#&!
          Raw string here~
        ");
}

#[test]
fn test_pod_with_code_before_and_after() {
    let input = r#"my $var = 1;

=head1 DESCRIPTION

This is a POD section with detailed description.
It preserves all formatting exactly.

=cut

my $other = 2;
"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my $var = 1;
        =head1 DESCRIPTION

        This is a POD section with detailed description.
        It preserves all formatting exactly.

        =cut
        my $other = 2;
        ");
}

#[test]
fn test_pod_at_eof_without_cut() {
    let input = r#"my $var = 1;

=pod

This POD block goes to EOF without =cut.
Everything after =pod should be treated as POD content.
"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my $var = 1;
        =pod

        This POD block goes to EOF without =cut.
        Everything after =pod should be treated as POD content.
        ");
}

#[test]
fn test_empty_lines_preservation() {
    let input = "use strict;\n\n\nuse warnings;\n\nmy $x = 1;\n\n\nsub foo {\n    return $x;\n}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        use strict;
        use warnings;

        my $x = 1;

        sub foo {
            return $x;
        }
        ");
}

#[test]
fn test_no_empty_lines_automatic_insertion() {
    let input = "use strict;use warnings;my $x = 1;sub foo {return $x;}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        use strict;
        use warnings;

        my $x = 1;

        sub foo {
            return $x;
        }
        ");
}

#[test]
fn test_block_stmt_empty_line_preservation() {
    // Test that user-written empty lines inside BLOCK_STMT are preserved
    let input = r#"sub f {
bar();

return 1;
}"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        sub f {
            bar();

            return 1;
        }
        ");
}

#[test]
fn test_multiple_empty_lines_in_block_stmt() {
    // Test that multiple consecutive empty lines are collapsed to one
    let input = r#"sub f {
bar();



return 1;
}"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        sub f {
            bar();

            return 1;
        }
        ");
}

#[test]
fn test_empty_lines_in_various_block_contexts() {
    // Test empty line preservation in different block contexts
    let input = r#"if ($condition) {
    1;

    2;


    3;

    # space ⬆️
    4;
    # space ⬇️

    5;

    # space ↕️

    6;

}"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        if ($condition) {
            1;

            2;

            3;

            # space ⬆️
            4;
            # space ⬇️

            5;

            # space ↕️

            6;

        }
        ");
}

#[test]
fn test_empty_lines_before_after_subs() {
    let input = "my$x=1;sub foo{my$y=2;}my$z=3;sub bar{return 42;}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my $x = 1;

        sub foo {
            my $y = 2;
        }

        my $z = 3;

        sub bar {
            return 42;
        }
        ");
}
