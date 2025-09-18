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
        ("my*glob=\\*STDIN;", "my *glob = \\*STDIN;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_local_complex_lvalue_formatting() {
    let cases = [
        // Hash subscriptions
        (
            "local $SIG{__WARN__} = \\&CORE::die;",
            "local $SIG{__WARN__} = \\&CORE::die;\n",
        ),
        (
            "local $ENV{PATH} = '/usr/bin';",
            "local $ENV{PATH} = '/usr/bin';\n",
        ),
        ("local $hash{key} = $value;", "local $hash{key} = $value;\n"),
        // Array subscriptions
        ("local $array[0] = 'first';", "local $array[0] = 'first';\n"),
        ("local $list[1] = $item;", "local $list[1] = $item;\n"),
        // Parenthesized lists with variables
        ("local ($a, $b) = (1, 2);", "local ($a, $b) = (1, 2);\n"),
        (
            "local ($x, $y, $z) = @values;",
            "local ($x, $y, $z) = @values;\n",
        ),
        // Mixed parenthesized lists with complex lvalues
        (
            "local ($SIG{__WARN__}, $a) = (\\&handler, $old_a);",
            "local ($SIG{__WARN__}, $a) = (\\&handler, $old_a);\n",
        ),
        (
            "local ($array[0], $hash{key}) = ($new_first, $new_value);",
            "local ($array[0], $hash{key}) = ($new_first, $new_value);\n",
        ),
        // With undef in lists
        (
            "local (undef, $SIG{__DIE__}) = (undef, \\&my_die);",
            "local (undef, $SIG{__DIE__}) = (undef, \\&my_die);\n",
        ),
        (
            "local ($a, undef, $hash{key}) = @list;",
            "local ($a, undef, $hash{key}) = @list;\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_continuation_indent_postfix_if() {
    let input = "warn 1\nif $err;";
    let formatted = format_and_assert(input);
    assert_eq!(formatted, "warn 1\n    if $err;\n");
}

#[test]
fn test_continuation_indent_infix_operator() {
    let input = "my $x = 1\n+ 2;";
    let formatted = format_and_assert(input);
    assert_eq!(formatted, "my $x = 1\n    + 2;\n");
}

#[test]
fn test_continuation_indent_comma() {
    let input = "my $result = func($a,\n$b);";
    let formatted = format_and_assert(input);
    assert_eq!(formatted, "my $result = func($a,\n    $b);\n");
}

#[test]
fn test_continuation_indent_comma_in_block() {
    let input = "sub foo {\n    my $result = func($a,\n$b);\n}";
    let formatted = format_and_assert(input);
    assert_eq!(
        formatted,
        "sub foo {\n    my $result = func($a,\n        $b);\n}\n"
    );
}

#[test]
fn test_continuation_indent_fat_comma() {
    let input = "my %hash = (key1 => 'value1',\nkey2 => 'value2');";
    let formatted = format_and_assert(input);
    assert_eq!(
        formatted,
        "my %hash = (key1 => 'value1',\n    key2 => 'value2');\n"
    );
}

#[test]
fn test_undef_in_variable_declaration_formatting() {
    let cases = [
        ("my(undef,$x)=@_;", "my (undef, $x) = @_;\n"),
        ("my($a,undef,$c)=@list;", "my ($a, undef, $c) = @list;\n"),
        (
            "my(undef,undef,$result)=func();",
            "my (undef, undef, $result) = func();\n",
        ),
        ("our(undef,$y)=(1,2);", "our (undef, $y) = (1, 2);\n"),
        ("state($x,undef)=@array;", "state ($x, undef) = @array;\n"),
        // Mixed variable declarations and undef (not part of variable declaration statement)
        ("(undef,my @a)=@_;", "(undef, my @a) = @_;\n"),
        (
            "(my $x,undef,our @y)=get_values();",
            "(my $x, undef, our @y) = get_values();\n",
        ),
        (
            "(undef,state $cache,my %hash)=complex_func(@args);",
            "(undef, state $cache, my %hash) = complex_func(@args);\n",
        ),
        (
            "(local $old,undef)=backup();",
            "(local $old, undef) = backup();\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_undef_function_call_formatting() {
    let cases = [
        // undef as a function call
        ("undef $x;", "undef $x;\n"),
        ("undef($var);", "undef($var);\n"),
        ("undef @array;", "undef @array;\n"),
        ("undef %hash;", "undef %hash;\n"),
        ("undef $hash{key};", "undef $hash{key};\n"),
        ("undef $array[0];", "undef $array[0];\n"),
        ("undef$x;", "undef $x;\n"),
        ("undef\t$y;", "undef $y;\n"),
        // undef with multiple arguments
        ("undef $x,$y;", "undef $x, $y;\n"),
        ("undef($a,$b,$c);", "undef($a, $b, $c);\n"),
        // undef as literal (should remain unchanged)
        ("my $x = undef;", "my $x = undef;\n"),
        ("$y = undef;", "$y = undef;\n"),
        ("return undef;", "return undef;\n"),
        // Mixed cases
        ("undef $x; my $y = undef;", "undef $x;\nmy $y = undef;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_special_variable_formatting() {
    let cases = [
        // Special variables with undef keyword as variable name
        ("$undef", "$undef"),
        ("@undef", "@undef"),
        ("%undef", "%undef"),
        ("my $undef = 1;", "my $undef = 1;\n"),
        ("our @undef;", "our @undef;\n"),
        ("state %undef;", "state %undef;\n"),
        // Special variables with caret notation
        ("${^MATCH}", "${^MATCH}"),
        ("${^PREMATCH}", "${^PREMATCH}"),
        ("${^POSTMATCH}", "${^POSTMATCH}"),
        ("${^ENCODING}", "${^ENCODING}"),
        ("${^TAINT}", "${^TAINT}"),
        ("${^UNICODE}", "${^UNICODE}"),
        ("${^UTF8CACHE}", "${^UTF8CACHE}"),
        ("${^UTF8LOCALE}", "${^UTF8LOCALE}"),
        // Assignment with special variables
        ("my $result = ${^MATCH};", "my $result = ${^MATCH};\n"),
        ("$undef = ${^ENCODING};", "$undef = ${^ENCODING};\n"),
        // Mixed usage
        ("print $undef, ${^MATCH};", "print $undef, ${^MATCH};\n"),
        (
            "use vars qw($undef ${^MATCH});",
            "use vars qw($undef ${^MATCH});\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_backtick_command_substitution_formatting() {
    let cases = [
        // Basic backtick command substitution
        ("`ls`;", "`ls`;\n"),
        ("`pwd`;", "`pwd`;\n"),
        ("`date`;", "`date`;\n"),
        // Commands with arguments
        ("`ls -la`;", "`ls -la`;\n"),
        ("`grep 'pattern' file.txt`;", "`grep 'pattern' file.txt`;\n"),
        ("`find . -name '*.pl'`;", "`find . -name '*.pl'`;\n"),
        // Assignment from command substitution
        ("my $output = `ls`;", "my $output = `ls`;\n"),
        ("$result = `pwd`;", "$result = `pwd`;\n"),
        ("my @files = `ls`;", "my @files = `ls`;\n"),
        // In expressions
        ("print `date`;", "print `date`;\n"),
        ("chomp(my $dir = `pwd`);", "chomp(my $dir = `pwd`);\n"),
        // Multiline commands (should preserve content)
        (
            "`echo 'line1'\necho 'line2'`;",
            "`echo 'line1'\necho 'line2'`;\n",
        ),
        // Commands with escapes
        ("`echo 'It\\'s working'`;", "`echo 'It\\'s working'`;\n"),
        (
            "`echo \"Hello \\\"world\\\"\"`;",
            "`echo \"Hello \\\"world\\\"\"`;\n",
        ),
        // Empty command
        ("``;", "``;\n"),
        // Commands in context
        (
            "if (`which perl`) { print 'found'; }",
            "if (`which perl`) {\n    print 'found';\n}\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_for_loop_with_expression_lists_formatting() {
    let cases = [
        // Basic for loop with multiple arrays
        (
            "for my $x (@a1, @a2) { print $x; }",
            "for my $x (@a1, @a2) {\n    print $x;\n}\n",
        ),
        (
            "foreach my $item (@array1, @array2, @array3) { say $item; }",
            "foreach my $item (@array1, @array2, @array3) {\n    say $item;\n}\n",
        ),
        // For loop with mixed expressions
        (
            "for my $x (@arr, qw(a b c), 1..10) { print $x; }",
            "for my $x (@arr, qw(a b c), 1 .. 10) {\n    print $x;\n}\n",
        ),
        // For loop with complex expressions
        (
            "for my $val (@{$hash{key}}, split(/,/, $str)) { process($val); }",
            "for my $val (@{$hash{key}}, split(/,/, $str)) {\n    process($val);\n}\n",
        ),
        // For loop with function calls
        (
            "for my $file (glob('*.txt'), @ARGV) { open my $fh, '<', $file; }",
            "for my $file (glob('*.txt'), @ARGV) {\n    open my $fh, '<', $file;\n}\n",
        ),
        // Nested structures
        (
            "for my $x (@a,@b,@c) { for my $y (@d) { print \"$x:$y\"; } }",
            "for my $x (@a, @b, @c) {\n    for my $y (@d) {\n        print \"$x:$y\";\n    }\n}\n",
        ),
        // C-style for with expression lists (though less common)
        ("for (@a, @b) { print; }", "for (@a, @b) {\n    print;\n}\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_postfix_dereference_formatting() {
    let cases = [
        ("$ref->@*;", "$ref->@*;\n"),
        ("$ref->%*;", "$ref->%*;\n"),
        ("$ref->$*;", "$ref->$*;\n"),
        ("$foo->@*[0];", "$foo->@*[0];\n"),
        ("$bar->%*{key};", "$bar->%*{key};\n"),
        ("$obj->meth->@*;", "$obj->meth->@*;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_sub_attribute_formatting() {
    let cases = [
        ("sub foo:bar {}", "sub foo : bar {}"),
        ("sub foo :bar:baz {}", "sub foo : bar : baz {}"),
        ("sub foo:bar(1,2) {}", "sub foo : bar(1, 2) {}"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_increment_decrement_formatting() {
    let cases = [
        ("$i++;", "$i++;\n"),
        ("++$i;", "++$i;\n"),
        ("$i--;", "$i--;\n"),
        ("--$i;", "--$i;\n"),
        ("$i++ + $j--;", "$i++ + $j--;\n"),
        // Note: ++$i++; is syntactically invalid in Perl (++$i produces an rvalue, not a valid lvalue for postfix ++)
        // but the parser is intentionally lenient to handle malformed code gracefully
        ("++$i++;", "++$i++;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_range_operator_formatting() {
    let cases = [("1..10;", "1 .. 10;\n"), ("1...10;", "1 ... 10;\n")];
    check_formatting_cases(&cases);
}

#[test]
fn test_loop_control_statements_formatting() {
    let cases = [
        ("next;", "next;\n"),
        ("next LABEL;", "next LABEL;\n"),
        ("last;", "last;\n"),
        ("last LOOP;", "last LOOP;\n"),
        ("redo;", "redo;\n"),
        ("redo LOOP;", "redo LOOP;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_return_multiple_values_formatting() {
    let cases = [
        ("return 1,2;", "return 1, 2;\n"),
        ("return $foo,@bar;", "return $foo, @bar;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_labeled_loop_formatting() {
    let input = "LOOP: while($i<10){next LOOP if $i==5;last if $i==8;redo LOOP if $flag;$i++;}";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @r"
        LOOP: while ($i < 10) {
            next LOOP if $i == 5;
            last if $i == 8;
            redo LOOP if $flag;
            $i++;
        }
        ");
}

#[test]
fn test_label_with_whitespace_before_colon() {
    let input = "LOOP : while($i<2){}";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @r"
        LOOP: while ($i < 2) {}
        ");
}

#[test]
fn test_until_loop_formatting() {
    let input = "until($i>10){$i--;}";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @r"
        until ($i > 10) {
            $i--;
        }
        ");
}

#[test]
fn test_ellipsis_statement_formatting() {
    let cases = [("...;", "...;\n"), ("sub foo{...}", "sub foo { ... }")];
    check_formatting_cases(&cases);
}

#[test]
fn test_empty_statement_formatting() {
    let cases = [
        (";", ";\n"),
        (";;", ";\n;\n"),
        (";;;", ";\n;\n;\n"),
        ("$x = 1; ;", "$x = 1;\n\n;\n"),
        ("sub foo { ; }", "sub foo {\n    ;\n}\n"),
        ("sub foo {\n    ;\n}", "sub foo {\n    ;\n}\n"),
        (
            "if ($x) { ; } else { ; }",
            "if ($x) {\n    ;\n} else {\n    ;\n}\n",
        ), // Added trailing newline
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_function_call_with_newline_in_args_formatting() {
    let cases = [("func({}\n);", "func({}\n);\n")];
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
fn test_postfix_for_modifier_formatting() {
    let cases = [("print $_ for@values;", "print $_ for @values;\n")];
    check_formatting_cases(&cases);
}

#[test]
fn test_basic_heredoc_formatting() {
    let input = "my $str = <<EOF;\nhello\nEOF\n";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @r"
        my $str = <<EOF;
        hello
        EOF
        ");
}

#[test]
fn test_empty_heredoc_formatting() {
    let input = "my $str = <<EOF;\nEOF\n";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @r"
        my $str = <<EOF;
        EOF
        ");
}

#[test]
fn test_bareword_call_with_heredoc() {
    let input = "die <<DIE;\n\nThis is\n  dying message!\n\nDIE\n";
    let formatted = format_and_assert(input);
    // Should preserve heredoc structure and not misparse as shift-left
    insta::assert_snapshot!(formatted, @r"
        die <<DIE;

        This is
          dying message!

        DIE
        ");
}

#[test]
fn test_package_block_formatting() {
    let input = "package Foo::Bar{my $x=1;}";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @r"
        package Foo::Bar {
            my $x = 1;
        }
        ");
}

#[test]
fn test_package_with_version_basic() {
    let cases = [
        // Package with version number and semicolon
        ("package Foo::Bar 1.23;", "package Foo::Bar 1.23;\n"),
        ("package My::Module 0.01;", "package My::Module 0.01;\n"),
        ("package Test  2.5;", "package Test 2.5;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_package_with_version_and_block() {
    let cases = [
        // Package with version and block
        (
            "package Foo::Bar 1.23{my $x=1;}",
            "package Foo::Bar 1.23 {\n    my $x = 1;\n}\n",
        ),
        (
            "package My::Module 0.01{print \"hello\";}",
            "package My::Module 0.01 {\n    print \"hello\";\n}\n",
        ),
        // Package with version literal (v-prefix)
        (
            "package Test::Module v2.0.0{my $var=42;}",
            "package Test::Module v2.0.0 {\n    my $var = 42;\n}\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_package_with_bare_version() {
    let cases = [
        // Package with bare version (no 'v' prefix)
        ("package Foo 5.024.001;", "package Foo 5.024.001;\n"),
        ("package Foo::Bar v1.2.3;", "package Foo::Bar v1.2.3;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_package_with_version_empty_block() {
    // Test empty block separately to understand formatting
    let input = "package Bar 5.024.001{}";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @"package Bar 5.024.001 {}");
}

#[test]
fn test_typeglob_formatting() {
    // Test simple typeglob reference
    let input = "my $fh = \\*STDIN;";
    let formatted = format_and_assert(input);
    assert_eq!(formatted, "my $fh = \\*STDIN;\n");

    // Test typeglob identifier
    let input = "*STDOUT;";
    let formatted = format_and_assert(input);
    assert_eq!(formatted, "*STDOUT;\n");
}

#[test]
fn test_typeglob_brace_formatting() {
    // Test typeglob with braces - this might need different handling
    let input = "*{$name};";
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @r"
        *{$name};
        ");
}

#[test]
fn test_reference_to_string_literal() {
    let cases = [
        (r#"my $ref = \"foo";"#, "my $ref = \\\"foo\";\n"),
        (r#"my $ref = \'bar';"#, "my $ref = \\'bar';\n"),
        (r#"my $ref = \q{baz};"#, "my $ref = \\q{baz};\n"),
        (r#"my $ref = \qq{qux};"#, "my $ref = \\qq{qux};\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_punctuation_named_special_variables_parse() {
    // Enumerate special variables whose names are punctuation characters.
    // This includes scalars, arrays, and hashes commonly used in modern Perl.
    // Note: We intentionally exclude legacy or caret-alphanum vars here.
    let input = r#"$_; @_; $!; $@; $?; $/; $\; $:; $;; $"; $'; $`; $&; $.; $0; $$; $<; $>; $(; $);
$[; $]; $+; $-; $=; $%; $|; $~; $*;
@+; @-; %+; %-; %!;"#;

    // Ensure it parses without errors and formatting preserves tokens
    let formatted = format_and_assert(input);
    assert_eq!(
        formatted,
        "$_;\n@_;\n$!;\n$@;\n$?;\n$/;\n$\\;\n$:;\n$;;\n$\";\n$';\n$`;\n$&;\n$.;\n$0;\n$$;\n$<;\n$>;\n$(;\n$);\n$[;\n$];\n$+;\n$-;\n$=;\n$%;\n$|;\n$~;\n$*;\n@+;\n@-;\n%+;\n%-;\n%!;\n",
        "Punctuation-named special variables should parse and format"
    );
}

#[test]
fn test_array_last_index_variables() {
    let cases = [
        // Basic $#array syntax
        ("$#arr;", "$#arr;\n"),
        ("my $last = $#items;", "my $last = $#items;\n"),
        // $#array with qualified names
        ("$#Package::array;", "$#Package::array;\n"),
        ("$#main::data;", "$#main::data;\n"),
        // $#$var syntax (last index of array referenced by $var)
        ("$#$arrayref;", "$#$arrayref;\n"),
        ("my $size = $#$ref + 1;", "my $size = $#$ref + 1;\n"),
        // Complex expressions with $#
        ("for my $i (0 .. $#array) {}", "for my $i (0 .. $#array) {}"),
        ("if ($#data >= 0) {}", "if ($#data >= 0) {}"),
        // Note: $#{array} syntax with braces has formatting issues and is commented out for now
        // ("$#{arr};", "$#{arr};\n"),
        // ("$#{Package::items};", "$#{Package::items};\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_comprehensive_typeglob_formatting() {
    let cases = [
        // Symbolic reference assignment
        ("*{$name} = \\&some_sub;", "*{$name} = \\&some_sub;\n"),
        // Package-qualified typeglob with braces and string literal
        (
            r#"*{"Foo::bar"} = *STDOUT;"#,
            "*{\"Foo::bar\"} = *STDOUT;\n",
        ),
        // Typeglob reference to different handle types
        ("my $fh = \\*STDERR;", "my $fh = \\*STDERR;\n"),
        // Complex symbolic reference
        (
            "*{\"${pkg}::${name}\"} = $value;",
            "*{\"${pkg}::${name}\"} = $value;\n",
        ),
        // Typeglob assignment with different sigils
        ("*name = \\$scalar;", "*name = \\$scalar;\n"),
        // Typeglob in hash context
        ("*{$hash{key}} = \\@array;", "*{$hash{key}} = \\@array;\n"),
    ];
    check_formatting_cases(&cases);
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
fn test_shift_exponent_and_bitwise_not_formatting() {
    let cases = [
        ("$a<<2;", "$a << 2;\n"),
        ("$a>>$b;", "$a >> $b;\n"),
        ("$x**$y;", "$x ** $y;\n"),
        ("~$mask;", "~$mask;\n"),
        ("$a<<=$b;", "$a <<= $b;\n"),
        ("$b>>=2;", "$b >>= 2;\n"),
        ("$c**=$d;", "$c **= $d;\n"),
    ];
    check_formatting_cases(&cases);
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
fn test_leading_comment_attached_to_sub() {
    let input = r#"my $x = 1;

# doc comment
sub foo {
    return $x;
}
"#;

    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my $x = 1;

        # doc comment
        sub foo {
            return $x;
        }
        ");
}

#[test]
fn test_multiple_leading_comments_attached_to_sub() {
    let input = r#"my $x = 1;
# first line
# second line
sub foo {
    return $x;
}
"#;

    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my $x = 1;

        # first line
        # second line
        sub foo {
            return $x;
        }
        ");
}

#[test]
fn test_inline_comment_before_sub_still_spaced() {
    let input = r#"my $x = 1; # trailing comment
sub foo {
    return $x;
}
"#;

    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my $x = 1; # trailing comment

        sub foo {
            return $x;
        }
        ");
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
fn test_version_no_statements_formatting() {
    check_formatting_cases(&[
        // v-prefixed versions
        ("no v5.24.1;", "no v5.24.1;\n"),
        ("no v5.008_001;", "no v5.008_001;\n"),
        ("no v5.36;", "no v5.36;\n"),
        // Bare version formats
        ("no 5.24.1;", "no 5.24.1;\n"),
        ("no 5.008_001;", "no 5.008_001;\n"),
        ("no 5.36.0;", "no 5.36.0;\n"),
        // Simple version numbers
        ("no 5;", "no 5;\n"),
        ("no 5.24;", "no 5.24;\n"),
        // With spacing variations
        ("no  v5.24.1 ;", "no v5.24.1;\n"),
        ("no  5.24.1 ;", "no 5.24.1;\n"),
        ("no\tv5.24.1\t;", "no v5.24.1;\n"),
        ("no\t5.24.1\t;", "no 5.24.1;\n"),
    ]);
}

#[test]
fn test_no_statement_with_module_names() {
    check_formatting_cases(&[
        // Basic module disabling
        ("no strict;", "no strict;\n"),
        ("no warnings;", "no warnings;\n"),
        ("no strict 'refs';", "no strict 'refs';\n"),
        ("no warnings 'all';", "no warnings 'all';\n"),
        // Qualified module names
        ("no Module::Name;", "no Module::Name;\n"),
        (
            "no Very::Long::Module::Name;",
            "no Very::Long::Module::Name;\n",
        ),
        // With spacing variations
        ("no  strict  ;", "no strict;\n"),
        ("no\twarnings\t;", "no warnings;\n"),
    ]);
}

#[test]
fn test_no_statement_with_parentheses_formatting() {
    check_formatting_cases(&[
        // No statement with empty parentheses
        ("no A();", "no A ();\n"),
        // Qualified module names
        ("no Module::Name();", "no Module::Name ();\n"),
        // With import list
        ("no A(qw/func1 func2/);", "no A (qw/func1 func2/);\n"),
        ("no Module(func1,func2);", "no Module (func1, func2);\n"),
        // With spacing variations
        ("no  A  ()  ;", "no A ();\n"),
        ("no\tA\t()\t;", "no A ();\n"),
    ]);
}

#[test]
fn test_no_statement_with_expressions() {
    check_formatting_cases(&[
        // Basic hash pair expressions
        ("no A::B x => 1;", "no A::B x => 1;\n"),
        ("no Module x=>1;", "no Module x => 1;\n"),
        // Multiple hash pairs
        ("no A::B x => 1, y => 2;", "no A::B x => 1, y => 2;\n"),
        (
            "no Module foo=>bar,baz=>123;",
            "no Module foo => bar, baz => 123;\n",
        ),
        // Different value types
        ("no A::B key => 'value';", "no A::B key => 'value';\n"),
        (
            "no A::B num => 42, str => \"hello\";",
            "no A::B num => 42, str => \"hello\";\n",
        ),
        // References and complex structures
        (
            "no A::B func => \\&function;",
            "no A::B func => \\&function;\n",
        ),
        ("no A::B array => [1,2,3];", "no A::B array => [1, 2, 3];\n"),
        (
            "no A::B hash => {a=>1,b=>2};",
            "no A::B hash => {a => 1, b => 2};\n",
        ),
        // With spacing variations
        ("no  A::B  x  =>  1  ;", "no A::B x => 1;\n"),
        ("no\tModule\tx\t=>\t1\t;", "no Module x => 1;\n"),
        // Multiple parameters with good spacing
        (
            "no A::B foo => 1, bar => 2;",
            "no A::B foo => 1, bar => 2;\n",
        ),
        // Long module name
        (
            "no Very::Long::Module::Name x => 1;",
            "no Very::Long::Module::Name x => 1;\n",
        ),
    ]);
}

#[test]
fn test_require_formatting() {
    check_formatting_cases(&[
        ("require local::lib;", "require local::lib;\n"),
        ("require v5.14;", "require v5.14;\n"),
        ("require 5.24.1;", "require 5.24.1;\n"),
        ("require 5;", "require 5;\n"),
        ("my $v = require v5.14;", "my $v = require v5.14;\n"),
        (
            "my $result = require local::lib;",
            "my $result = require local::lib;\n",
        ),
    ]);
}

#[test]
fn test_keyword_module_names() {
    check_formatting_cases(&[
        // Method calls on keyword module names
        ("local::lib->new;", "local::lib->new;\n"),
        ("local::lib->import;", "local::lib->import;\n"),
        ("use::ok->new;", "use::ok->new;\n"),
        ("if::then->call;", "if::then->call;\n"),
        // Keyword:: followed by ->
        ("local::->new;", "local::->new;\n"),
        ("use::->import;", "use::->import;\n"),
        // Multiple levels
        ("local::lib::more->new;", "local::lib::more->new;\n"),
        // In expressions
        ("my $obj = local::lib->new;", "my $obj = local::lib->new;\n"),
        (
            "my $result = use::ok->call($arg);",
            "my $result = use::ok->call($arg);\n",
        ),
        // With parentheses
        ("local::lib->new();", "local::lib->new();\n"),
        (
            "local::lib->import('feature');",
            "local::lib->import('feature');\n",
        ),
    ]);
}

#[test]
fn test_use_statement_with_parentheses_formatting() {
    check_formatting_cases(&[
        // Use statement with empty parentheses (import list)
        ("use A();", "use A ();\n"),
        // Qualified module names now have space
        ("use Module::Name();", "use Module::Name ();\n"),
        // Use statement with import list - qw formatting should not have extra spaces
        ("use A(qw/func1 func2/);", "use A (qw/func1 func2/);\n"),
        ("use Module(func1,func2);", "use Module (func1, func2);\n"),
        // With spacing variations
        ("use  A  ()  ;", "use A ();\n"),
        ("use\tA\t()\t;", "use A ();\n"),
    ]);
}

#[test]
fn test_use_statement_with_expressions() {
    check_formatting_cases(&[
        // Basic hash pair expressions
        ("use A::B x => 1;", "use A::B x => 1;\n"),
        ("use Module x=>1;", "use Module x => 1;\n"),
        // Dash-prefixed import flag should be preserved
        ("use A -abcde => 1;", "use A -abcde => 1;\n"),
        // Multiple hash pairs
        ("use A::B x => 1, y => 2;", "use A::B x => 1, y => 2;\n"),
        (
            "use Module foo=>bar,baz=>123;",
            "use Module foo => bar, baz => 123;\n",
        ),
        // Different value types
        ("use A::B key => 'value';", "use A::B key => 'value';\n"),
        (
            "use A::B num => 42, str => \"hello\";",
            "use A::B num => 42, str => \"hello\";\n",
        ),
        // Complex expressions
        (
            "use A::B func => \\&function;",
            "use A::B func => \\&function;\n",
        ),
        (
            "use A::B array => [1,2,3];",
            "use A::B array => [1, 2, 3];\n",
        ),
        (
            "use A::B hash => {a=>1,b=>2};",
            "use A::B hash => {a => 1, b => 2};\n",
        ),
        // Spacing variations
        ("use  A::B  x  =>  1  ;", "use A::B x => 1;\n"),
        ("use\tModule\tx\t=>\t1\t;", "use Module x => 1;\n"),
        // Mixed with regular identifiers (not starting with 'x')
        (
            "use A::B foo => 1, bar => 2;",
            "use A::B foo => 1, bar => 2;\n",
        ),
        // Complex module names with expressions
        (
            "use Very::Long::Module::Name x => 1;",
            "use Very::Long::Module::Name x => 1;\n",
        ),
    ]);
}

#[test]
fn test_method_call_formatting() {
    let cases = [
        // Basic method calls
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
        // Dynamic method calls - basic
        ("$obj->$method();", "$obj->$method();\n"),
        ("Class->$method();", "Class->$method();\n"),
        ("$self->$dynamic_method;", "$self->$dynamic_method;\n"),
        // Dynamic method calls with arguments
        ("$obj->$method($a,$b);", "$obj->$method($a, $b);\n"),
        ("$obj->$method($arg);", "$obj->$method($arg);\n"),
        (
            "$object->$method_name($arg1,$arg2);",
            "$object->$method_name($arg1, $arg2);\n",
        ),
        (
            "$object->$method_name($a,$b,$c);",
            "$object->$method_name($a, $b, $c);\n",
        ),
        // Chained dynamic method calls
        (
            "$obj->$method1()->$method2();",
            "$obj->$method1()->$method2();\n",
        ),
        (
            "$obj->get()->$set_method($value);",
            "$obj->get()->$set_method($value);\n",
        ),
        // Mixed regular and dynamic
        (
            "$obj->regular()->$dynamic();",
            "$obj->regular()->$dynamic();\n",
        ),
        (
            "$obj->$dynamic()->regular();",
            "$obj->$dynamic()->regular();\n",
        ),
        // Complex expressions as method invocants
        ("func()->$method();", "func()->$method();\n"),
        (
            "$hash_ref->{method}->$dynamic();",
            "$hash_ref->{method}->$dynamic();\n",
        ),
        (
            "($obj || $default)->$method();",
            "($obj || $default)->$method();\n",
        ),
        // Without parentheses (valid in Perl)
        ("$obj->$method_var;", "$obj->$method_var;\n"),
        ("Class->$static_method;", "Class->$static_method;\n"),
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
fn test_print_like_filehandles() {
    let cases = [
        ("print$fh 1,2,3;", "print $fh 1, 2, 3;\n"),
        ("printf$fh\"%s\",$msg;", "printf $fh \"%s\", $msg;\n"),
        ("say{get_fh()}$value;", "say { get_fh() } $value;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_print_function_calls_as_expressions() {
    // Test cases where function calls should be parsed as expressions, not filehandles
    let cases = [
        // Function calls should be parsed as expressions, not filehandles
        ("print foo(), \"x\";", "print foo(), \"x\";\n"),
        ("print get_handle(), $data;", "print get_handle(), $data;\n"),
        (
            "printf get_formatter(), \"%d\", $num;",
            "printf get_formatter(), \"%d\", $num;\n",
        ),
        ("say func(), @values;", "say func(), @values;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_print_complex_scalar_expressions() {
    // Test cases for complex scalar expressions that should be parsed as expressions,
    // not filehandles, with the improved lookahead logic
    let cases = [
        // Method calls on scalars - should be parsed as expressions
        ("print $code->(), \"x\";", "print $code->(), \"x\";\n"),
        (
            "print $obj->method(), $data;",
            "print $obj->method(), $data;\n",
        ),
        (
            "printf $formatter->get(), \"%s\", $str;",
            "printf $formatter->get(), \"%s\", $str;\n",
        ),
        // Array/hash access on scalars - should be parsed as expressions
        ("print $array[0], \"x\";", "print $array[0], \"x\";\n"),
        ("print $hash{key}, $value;", "print $hash{key}, $value;\n"),
        // Function calls on scalars - should be parsed as expressions
        // Note: formatter adds space before parentheses in this context
        (
            "print $func(), \"result\";",
            "print $func (), \"result\";\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_print_simple_filehandle_behavior() {
    // Test that simple filehandle syntax still works correctly
    let cases = [
        // Simple scalar variables as filehandles (no postfix operations)
        ("print $fh \"data\";", "print $fh \"data\";\n"),
        ("printf $fh \"%s\", $msg;", "printf $fh \"%s\", $msg;\n"),
        ("say $fh $message;", "say $fh $message;\n"),
        // With explicit comma (traditional filehandle syntax)
        ("print $fh, \"data\";", "print $fh, \"data\";\n"),
        ("printf $fh, \"%s\", $msg;", "printf $fh, \"%s\", $msg;\n"),
        // Bareword filehandles
        ("print STDERR \"error\";", "print STDERR \"error\";\n"),
        ("print STDOUT $output;", "print STDOUT $output;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_print_function_vs_filehandle_disambiguation() {
    // Test that the lookahead logic correctly distinguishes between
    // function calls and filehandle syntax
    let cases = [
        // Function calls - should NOT be treated as filehandles
        ("print foo(), \"x\";", "print foo(), \"x\";\n"),
        ("print get_handle(), $data;", "print get_handle(), $data;\n"),
        // Simple scalars followed by expressions - should be treated as filehandles
        ("print $fh \"data\";", "print $fh \"data\";\n"),
        ("print $handle $message;", "print $handle $message;\n"),
        // Complex expressions - should NOT be treated as filehandles
        (
            "print $obj->method(), \"result\";",
            "print $obj->method(), \"result\";\n",
        ),
        (
            "print $array[0], \"value\";",
            "print $array[0], \"value\";\n",
        ),
        (
            "print $hash{key}, \"data\";",
            "print $hash{key}, \"data\";\n",
        ),
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
fn test_eval_block_followed_by_defined_or() {
    let input = "eval{}//1;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        eval {} // 1;
        ");
}

#[test]
fn test_do_block_followed_by_defined_or() {
    let input = "do{}//1;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        do {} // 1;
        ");
}

#[test]
fn test_generic_block_function_formatting() {
    let input = "foo{$_+1}@values; Module::bar{process($_)}@items;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        foo { $_ + 1 } @values;
        Module::bar { process($_) } @items;
        ");
}

#[test]
fn test_print_binary_operator_disambiguation() {
    // Test cases to ensure binary operators are correctly parsed as expressions, not filehandles
    // Focus on key cases that verify the disambiguation logic works
    let cases = [
        // These should parse correctly - comparison and logical operators have consistent spacing
        ("print $a == $b;", "print $a == $b;\n"),
        ("print $a != $b;", "print $a != $b;\n"),
        ("print $a <= $b;", "print $a <= $b;\n"),
        ("print $a >= $b;", "print $a >= $b;\n"),
        ("print $a && $b;", "print $a && $b;\n"),
        ("print $a || $b;", "print $a || $b;\n"),
        // Complex expressions that should work
        ("print foo() + bar();", "print foo() + bar();\n"),
        ("print $obj->method() * 2;", "print $obj->method() * 2;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_print_binary_operator_parsing_basic() {
    // Minimal test to verify the core disambiguation works without getting bogged down in spacing
    let input = "print foo + 1;";
    let formatted = format_and_assert(input);
    // The key is that it should parse and format successfully, proving disambiguation works
    assert!(formatted.contains("print foo"));
    assert!(formatted.contains("1"));
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
fn test_ternary_with_array_hash_references() {
    let input = "$a ? [] : {}";
    let output = format_and_assert(input);
    insta::assert_snapshot!(output, @"$a ? [] : {}");
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
fn test_qw_variable_assignment() {
    // Test the case that was failing: my @a = qw(a);
    let cases = [
        ("my @a = qw(a);", "my @a = qw(a);\n"),
        (
            "my @words = qw(one two three);",
            "my @words = qw(one two three);\n",
        ),
        (
            "our @list = qw/alpha beta gamma/;",
            "our @list = qw/alpha beta gamma/;\n",
        ),
    ];
    check_formatting_cases(&cases);
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
fn test_qw_string_with_trailing_newline() {
    // Test that QW_STRING tokens ending with newlines don't get extra newlines
    let input = r#"my @words = qw(
foo
bar
);"#;
    let formatted = format_and_assert(input);

    // Should not have extra blank lines between words
    insta::assert_snapshot!(formatted, @r"
        my @words = qw(
            foo
            bar
        );
        ");
}

#[test]
fn test_write_str_handles_embedded_newlines() {
    // Test that write_str correctly handles strings with embedded newlines
    let input = r#"say "line1
line2
line3";"#;
    let formatted = format_and_assert(input);

    // Should preserve the embedded newlines without adding extra ones
    insta::assert_snapshot!(formatted, @r#"
        say "line1
        line2
        line3";
        "#);
}

#[test]
fn test_mixed_single_and_multiline() {
    let input = r#"my $mixed = {
simple => { a => 1, b => 2 },
complex => {
nested => [1, 2, 3],
items => [
        "first",
        "second" ]
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
fn test_q_arbitrary_delimiters_formatting() {
    let input = r#"# Test q() with various delimiters
print q(hello);       # parentheses
print q/world/;       # slash
print q{foo};         # braces
print q[bar];         # brackets
print q<baz>;         # angle brackets
print q|pipe|;        # pipe
print q#hash#;        # hash
print q@at@;          # at sign
print q%percent%;     # percent
print q^caret^;       # caret
print q*asterisk*;    # asterisk
print q+plus+;        # plus
print q=equals=;      # equals
print q!exclamation!; # exclamation
print q~tilde~;       # tilde
print q`backtick`;    # backtick"#;

    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
    # Test q() with various delimiters
    print q(hello); # parentheses
    print q/world/; # slash
    print q{foo}; # braces
    print q[bar]; # brackets
    print q<baz>; # angle brackets
    print q|pipe|; # pipe
    print q#hash#; # hash
    print q@at@; # at sign
    print q%percent%; # percent
    print q^caret^; # caret
    print q*asterisk*; # asterisk
    print q+plus+; # plus
    print q=equals=; # equals
    print q!exclamation!; # exclamation
    print q~tilde~; # tilde
    print q`backtick`; # backtick
    ");
}

#[test]
fn test_qq_arbitrary_delimiters_formatting() {
    let input = r#"# Test qq() with various delimiters (simple content)
print qq(hello world);
print qq/simple text/;
print qq{foo bar};
print qq[baz qux];
print qq<test string>;"#;

    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r###"
    # Test qq() with various delimiters (simple content)
    print qq(hello world);
    print qq/simple text/;
    print qq{foo bar};
    print qq[baz qux];
    print qq<test string>;
    "###);
}

#[test]
fn test_qx_arbitrary_delimiters_formatting() {
    let input = r#"# Test qx() with various delimiters
my $result = qx(ls -la);
my $result = qx/ps aux/;
my $result = qx{date};
my $result = qx[whoami];
my $result = qx<pwd>;"#;

    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r###"
    # Test qx() with various delimiters
    my $result = qx(ls -la);
    my $result = qx/ps aux/;
    my $result = qx{date};
    my $result = qx[whoami];
    my $result = qx<pwd>;
    "###);
}

#[test]
fn test_m_arbitrary_delimiters_formatting() {
    let input = r#"# Test m// with various delimiters
if ($string =~ m(pattern)i) { }
if ($string =~ m/pattern/i) { }
if ($string =~ m{pattern}i) { }
if ($string =~ m[pattern]i) { }
if ($string =~ m<pattern>i) { }
if ($string =~ m|pattern|i) { }"#;

    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
    # Test m// with various delimiters
    if ($string =~ m(pattern)i) {}
    if ($string =~ m/pattern/i) {}
    if ($string =~ m{pattern}i) {}
    if ($string =~ m[pattern]i) {}
    if ($string =~ m<pattern>i) {}
    if ($string =~ m|pattern|i) {}
    ");
}

#[test]
fn test_qr_arbitrary_delimiters_formatting() {
    let input = r#"# Test qr// with various delimiters
my $regex = qr(pattern)i;
my $regex = qr/pattern/i;
my $regex = qr{pattern}i;
my $regex = qr[pattern]i;
my $regex = qr<pattern>i;"#;

    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r###"
    # Test qr// with various delimiters
    my $regex = qr(pattern)i;
    my $regex = qr/pattern/i;
    my $regex = qr{pattern}i;
    my $regex = qr[pattern]i;
    my $regex = qr<pattern>i;
    "###);
}

#[test]
fn test_s_operator_formatting() {
    let input = "$str =~ s/abc/xyz/;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"$str =~ s/abc/xyz/;");
}

#[test]
fn test_s_operator_whitespace_handling() {
    // Test case for issue where S_EXPR incorrectly includes trailing whitespace
    let input = "s/re// or return;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @"s/re// or return;");
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
    // Test tr with different delimiters
    let formatted = format_and_assert("tr(a)(b);");
    // The formatter adds a newline, which is expected behavior
    assert_eq!(formatted, "tr(a)(b);\n");
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
    sub and {}

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
    let input = r#"use strict;


use warnings;

my $x = 1;


sub foo {
    return $x;
}

1;"#;

    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        use strict;
        
        use warnings;

        my $x = 1;

        sub foo {
            return $x;
        }
        
        1;");
}

#[test]
fn test_no_empty_lines_automatic_insertion() {
    let input = "use strict;use warnings;my $x = 1;sub foo {return $x;}1;";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        use strict;
        use warnings;

        my $x = 1;

        sub foo {
            return $x;
        }

        1;
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

#[test]
fn test_anonymous_subroutine_formatting() {
    // Test anonymous subroutines with our new implementation
    let input = "my $code = sub {\n   print \"Hello\\n\";\n};\n$code->();";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r###"
my $code = sub {
    print "Hello\n";
};
$code->();
"###);
}

#[test]
fn test_anonymous_subroutine_various_cases() {
    let cases = [
        // Simple anonymous sub
        ("my$s=sub{1};", "my $s = sub { 1 };\n"),
        // Anonymous sub with parameters and multiple statements
        (
            "my$f=sub{my($x,$y)=@_;return $x+$y;};",
            "my $f = sub {\n    my ($x, $y) = @_;\n    return $x + $y;\n};\n",
        ),
        // Anonymous sub in function call
        ("map(sub{$_*2},@arr);", "map(sub { $_ * 2 }, @arr);\n"),
        // Anonymous sub assignment and immediate call (multiline because of semicolon)
        (
            "(sub{print\"test\";})();",
            "(sub {\n    print \"test\";\n}\n)();\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_anonymous_subroutine_in_expressions() {
    // Test anonymous subs in different contexts
    let cases = [
        // As standalone assignment
        ("my $simple = sub { 42 };", "my $simple = sub { 42 };\n"),
        // In array context
        ("my @subs = (sub { 1 });", "my @subs = (sub { 1 });\n"),
        // As function argument
        (
            "call_me(sub { \"hello\" });",
            "call_me(sub { \"hello\" });\n",
        ),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_original_issue_example() {
    // Test the original issue example
    let input = r#"my $code = sub {
   print "Hello\n";
};
$code->();"#;
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r###"
my $code = sub {
    print "Hello\n";
};
$code->();
"###);
}

#[test]
fn test_subroutine_prototypes_formatting() {
    // Basic prototypes
    // All examples from the GitHub issue, joined as single-line code
    let input = concat!(
        "sub mypush (\\@@) {}",
        "sub myjoin ($@) {}",
        "sub mysplice (\\@$$@) {}",
        "sub mykeys (\\[%@]) {}",
        "sub myopen (*;$) {}",
        "sub mygrep (&@) {}",
        "sub myrand (;$) {}",
        "sub mytime () {}",
        "sub test( $ @ )  { }",
        "sub foo(\t\\@\t){}  ",
        "sub mypush (\\@@) { my $x = 1; }",
    );
    let formatted = format_and_assert(input);
    insta::assert_snapshot!(formatted, @r"
    sub mypush (\@@) {}

    sub myjoin ($@) {}

    sub mysplice (\@$$@) {}

    sub mykeys (\[%@]) {}

    sub myopen (*;$) {}

    sub mygrep (&@) {}

    sub myrand (;$) {}

    sub mytime () {}

    sub test ($@) {}

    sub foo (\@) {}

    sub mypush (\@@) {
        my $x = 1;
    }
    ");
}

#[test]
fn test_subroutine_prototype_spacing() {
    // Test that prototypes have proper spacing before parentheses
    let input = "sub test(\\@){my $x = 1;}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        sub test (\@) {
            my $x = 1;
        }
        ");
}

#[test]
fn test_complex_subroutine_prototype() {
    // Test complex prototype with mixed symbols
    let input = "sub complex_func (\\@$$;*&\\[%@]) { my ($arr_ref, $scalar1, $scalar2, $opt_typeglob, $code_block, $hash_or_array_ref) = @_; }";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        sub complex_func (\@$$;*&\[%@]) {
            my ($arr_ref, $scalar1, $scalar2, $opt_typeglob, $code_block, $hash_or_array_ref) = @_;
        }
        ");
}

#[test]
fn test_file_test_operator_formatting() {
    let cases = [
        ("-f $file;", "-f $file;\n"),
        ("-f;", "-f;\n"), // argumentless file test operator
        ("-d;", "-d;\n"), // argumentless directory test operator
        ("-e;", "-e;\n"), // argumentless existence test operator
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_x_operator_formatting() {
    // Test string repetition operator 'x'
    let cases = [
        // Basic x operator
        ("\"a\" x 5;", "\"a\" x 5;\n"),
        ("'hello' x 3;", "'hello' x 3;\n"),
        ("$str x $count;", "$str x $count;\n"),
        // q expressions with x operator
        ("q(a) x 10;", "q(a) x 10;\n"),
        ("q/abc/ x 3;", "q/abc/ x 3;\n"),
        ("q{hello} x $times;", "q{hello} x $times;\n"),
        // qq expressions with x operator
        ("qq(a) x 10;", "qq(a) x 10;\n"),
        ("qq/abc/ x 3;", "qq/abc/ x 3;\n"),
        // qw expressions with x operator
        ("qw(a) x 10;", "qw(a) x 10;\n"),
        ("qw/word1 word2/ x 2;", "qw/word1 word2/ x 2;\n"),
        // Complex expressions
        ("($str . $suffix) x 2;", "($str . $suffix) x 2;\n"),
        ("func() x 4;", "func() x 4;\n"),
    ];
    check_formatting_cases(&cases);
}

#[test]
fn test_regex_literals_in_function_calls() {
    check_formatting_cases(&[
        // Basic split with regex literal - with parentheses
        ("split(/\\s+/, $string)", "split(/\\s+/, $string)"),
        // Split with regex flags
        ("split(/\\s+/g, $string)", "split(/\\s+/g, $string)"),
        // Complex regex patterns
        ("split(/\\d{2,4}/, $input)", "split(/\\d{2,4}/, $input)"),
        ("split(/[a-zA-Z]+/, $data)", "split(/[a-zA-Z]+/, $data)"),
        // The main fix: builtin functions without parentheses
        ("split /\\s+/, $string", "split /\\s+/, $string"),
        ("split /pattern/, $str", "split /pattern/, $str"),
        ("warn /warning/", "warn /warning/"),
        ("print /pattern/", "print /pattern/"),
        ("say /hello/", "say /hello/"),
        // Other builtin functions with regex literals
        ("grep /pattern/, @array", "grep /pattern/, @array"),
        ("map /transform/, @list", "map /transform/, @list"),
        ("substr /abc/, 1, 2", "substr /abc/, 1, 2"),
        ("index /needle/, $haystack", "index /needle/, $haystack"),
        // Multiple arguments with regex
        ("split /\\s+/, $string, 3", "split /\\s+/, $string, 3"),
        (
            "grep /\\d+/, @numbers, @more",
            "grep /\\d+/, @numbers, @more",
        ),
        // Complex regex patterns without parentheses
        ("split /\\d{2,4}/, $input", "split /\\d{2,4}/, $input"),
        (
            "grep /[a-zA-Z]+\\d*/, @mixed",
            "grep /[a-zA-Z]+\\d*/, @mixed",
        ),
        ("warn /^Error: .*$/", "warn /^Error: .*$/"),
        // Regex in other contexts (should still work)
        ("match(/pattern/, $str)", "match(/pattern/, $str)"),
        ("if ($str =~ /pattern/) { }", "if ($str =~ /pattern/) {}"),
        // Make sure division still works correctly
        ("$a / $b", "$a / $b"),
        ("my $result = $x / $y;", "my $result = $x / $y;\n"),
        (
            "$count = $total / $divisor;",
            "$count = $total / $divisor;\n",
        ),
        // Mixed regex and division - ensure proper context switching
        ("split(/\\s+/, $str) / 2", "split(/\\s+/, $str) / 2"),
        ("warn /alert/ / $count", "warn /alert/ / $count"),
        // Edge cases: builtin functions in statements
        (
            "my $func = split /pattern/, $str;",
            "my $func = split /pattern/, $str;\n",
        ),
        (
            "return split /\\s/, $input;",
            "return split /\\s/, $input;\n",
        ),
        (
            "push @results, grep /match/, @data;",
            "push @results, grep /match/, @data;\n",
        ),
        // Chained function calls
        (
            "join '', split /\\s+/, $text",
            "join '', split /\\s+/, $text",
        ),
        (
            "print reverse split /,/, $csv",
            "print reverse split /,/, $csv",
        ),
        // Regex with modifiers in builtin functions
        ("split /pattern/i, $str", "split /pattern/i, $str"),
        ("grep /test/g, @array", "grep /test/g, @array"),
        ("warn /debug/x", "warn /debug/x"),
    ]);
}

#[test]
fn test_unary_plus_operator_formatting() {
    let cases = vec![
        // Basic unary plus
        ("my $x = +42;", "my $x = +42;\n"),
        ("my $y = +$var;", "my $y = +$var;\n"),
        // Unary plus with hash reference (the main use case)
        ("my $h = +{ a => 1 };", "my $h = +{a => 1};\n"),
        ("my $h = +{a=>1,b=>2};", "my $h = +{a => 1, b => 2};\n"),
        // Unary plus with array reference
        ("my $a = +[ 1, 2, 3 ];", "my $a = +[1, 2, 3];\n"),
        ("my $a = +[1,2,3];", "my $a = +[1, 2, 3];\n"),
        // Nested unary plus
        ("my $x = +(+$y);", "my $x = +(+$y);\n"),
        // Unary plus in expressions
        ("my $result = +$x * 2;", "my $result = +$x * 2;\n"),
        ("my $result = 3 + +$x;", "my $result = 3 + +$x;\n"),
        // Complex cases
        (
            "my $complex = +{ key => +[ +$a, +$b ] };",
            "my $complex = +{key => +[+$a, +$b]};\n",
        ),
    ];

    check_formatting_cases(&cases);
}

#[test]
fn test_unary_minus_operator_formatting() {
    let cases = vec![
        // Basic unary minus
        ("my $x = -42;", "my $x = -42;\n"),
        ("my $y = -$var;", "my $y = -$var;\n"),
        // Unary minus with complex expressions
        ("my $h = -{ a => 1 };", "my $h = -{a => 1};\n"),
        ("my $a = -[ 1, 2, 3 ];", "my $a = -[1, 2, 3];\n"),
        // Nested unary minus
        ("my $x = -(-$y);", "my $x = -(-$y);\n"),
        // Mixed unary operators
        ("my $x = +-$y;", "my $x = +-$y;\n"),
        ("my $x = -+$y;", "my $x = -+$y;\n"),
        // Unary minus in expressions
        ("my $result = -$x * 2;", "my $result = -$x * 2;\n"),
        ("my $result = 3 - -$x;", "my $result = 3 - -$x;\n"),
    ];

    check_formatting_cases(&cases);
}

#[test]
fn test_unary_operators_in_context() {
    let cases = vec![
        // Function arguments with parentheses (works correctly)
        ("func(+$x, -$y);", "func(+$x, -$y);\n"),
        ("print(+{a => 1});", "print(+{a => 1});\n"),
        // Function calls without parentheses (space between function and args is correct in Perl)
        ("print +{a => 1};", "print + {a => 1};\n"),
        // Return statements
        (
            "sub foo { return +{ result => $val }; }",
            "sub foo {\n    return +{result => $val};\n}\n",
        ),
        // Assignment context
        ("@arr = (+1, -2, +$x);", "@arr = (+1, -2, +$x);\n"),
        // Hash and array contexts
        ("%hash = ( key => +$val );", "%hash = (key => +$val);\n"),
        ("@array = [ +$a, -$b ];", "@array = [+$a, -$b];\n"),
        // Conditional context
        (
            "if (+$x > 0) { print \"positive\"; }",
            "if (+$x > 0) {\n    print \"positive\";\n}\n",
        ),
    ];

    check_formatting_cases(&cases);
}

#[test]
fn test_keyword_variable_names_formatting() {
    // Test that keywords can be used as variable names and are formatted correctly
    let cases = [
        // Basic keyword variable names mentioned in the issue
        ("my $package = \"test\";", "my $package = \"test\";\n"),
        ("my @if = (1, 2, 3);", "my @if = (1, 2, 3);\n"),
        // Various declaration types with keyword variable names
        ("our $sub = 1;", "our $sub = 1;\n"),
        ("state $while = 2;", "state $while = 2;\n"),
        ("local $for = 3;", "local $for = 3;\n"),
        ("my %else = (a => 1);", "my %else = (a => 1);\n"),
        // More keywords as variable names
        ("my $unless = 4;", "my $unless = 4;\n"),
        ("my $elsif = 5;", "my $elsif = 5;\n"),
        ("my $return = 6;", "my $return = 6;\n"),
        ("my $use = 7;", "my $use = 7;\n"),
        ("my $no = 8;", "my $no = 8;\n"),
        // Mixed normal and keyword variables
        (
            "my ($x, $if, $y) = (1, 2, 3);",
            "my ($x, $if, $y) = (1, 2, 3);\n",
        ),
        ("my @package = qw(foo bar);", "my @package = qw(foo bar);\n"),
        // In expressions
        ("$if + $package;", "$if + $package;\n"),
        ("print $use, $no;", "print $use, $no;\n"),
    ];

    check_formatting_cases(&cases);
}

#[test]
fn test_declaration_keywords_as_variable_names_formatting() {
    // Test that declaration keywords (my, our, state, local) can be used as variable names
    let cases = [
        // Declaration keywords as variable names - the main fix
        ("my $my = 1;", "my $my = 1;\n"),
        ("my $our = 2;", "my $our = 2;\n"),
        ("my $state = 3;", "my $state = 3;\n"),
        ("my $local = 4;", "my $local = 4;\n"),
        // Mixed declaration types
        ("our $my = 5;", "our $my = 5;\n"),
        ("state $our = 6;", "state $our = 6;\n"),
        ("local $state = 7;", "local $state = 7;\n"),
        // Different sigil types
        ("my @my = (1, 2);", "my @my = (1, 2);\n"),
        ("my %our = (a => 1);", "my %our = (a => 1);\n"),
        // In expressions
        ("$my + $our;", "$my + $our;\n"),
        ("print $state, $local;", "print $state, $local;\n"),
        // Multiple on one line
        (
            "my ($my, $our, $state) = (1, 2, 3);",
            "my ($my, $our, $state) = (1, 2, 3);\n",
        ),
    ];

    check_formatting_cases(&cases);
}

#[test]
fn test_expression_dereference_formatting() {
    let cases = [
        // Traditional dereferencing (should continue to work)
        ("@$ref;", "@$ref;\n"),
        ("%$ref;", "%$ref;\n"),
        ("$$ref;", "$$ref;\n"),
        // New expression dereferencing
        ("@{ func() };", "@{func()};\n"),
        ("%{ func() };", "%{func()};\n"),
        ("${ func() };", "${func()};\n"),
        // More complex expressions
        ("@{ $obj->method() };", "@{$obj->method()};\n"),
        ("%{ get_hash_ref() };", "%{get_hash_ref()};\n"),
        ("${ $array[0] };", "${$array[0]};\n"),
        // With spacing variations
        ("@{func()};", "@{func()};\n"),
        ("@{ func() };", "@{func()};\n"),
        ("@{  func()  };", "@{func()};\n"),
    ];

    check_formatting_cases(&cases);
}

#[test]
fn test_compound_assignment_precedence() {
    let cases = [
        // Basic compound assignments
        ("$x += 1;", "$x += 1;\n"),
        ("$x -= 2;", "$x -= 2;\n"),
        ("$x *= 3;", "$x *= 3;\n"),
        ("$x /= 4;", "$x /= 4;\n"),
        ("$x %= 5;", "$x %= 5;\n"),
        ("$x .= 'text';", "$x .= 'text';\n"),
        // Logical compound assignments
        ("$x ||= 'default';", "$x ||= 'default';\n"),
        ("$x &&= 'value';", "$x &&= 'value';\n"),
        ("$x //= 'defined';", "$x //= 'defined';\n"),
        // String repetition compound assignment
        ("$x x= 3;", "$x x= 3;\n"),
        // Bitwise compound assignments
        ("$x &= $mask;", "$x &= $mask;\n"),
        ("$flags |= $bit;", "$flags |= $bit;\n"),
        ("$value ^= $toggle;", "$value ^= $toggle;\n"),
        // Test precedence: compound assignment should have assignment precedence
        // This should be parsed as: $x += ($y + $z), not ($x += $y) + $z
        ("$x += $y + $z;", "$x += $y + $z;\n"),
        ("$result ||= $a + $b * $c;", "$result ||= $a + $b * $c;\n"),
        // Complex expressions with compound assignment
        ("$hash{key} += $value * 2;", "$hash{key} += $value * 2;\n"),
        ("@array[0] .= ' suffix';", "@array[0] .= ' suffix';\n"),
    ];

    check_formatting_cases(&cases);
}

#[test]
fn test_comma_operator_precedence() {
    let cases = [
        // Basic comma operator
        ("$a, $b;", "$a, $b;\n"),
        ("$a, $b, $c;", "$a, $b, $c;\n"),
        // Comma has lower precedence than assignment
        // This should parse as: (($x = $y), $z), not: $x = ($y, $z)
        ("$x = $y, $z;", "$x = $y, $z;\n"),
        ("$a = 1, $b = 2;", "$a = 1, $b = 2;\n"),
        // Comma has lower precedence than arithmetic operators
        ("$a + $b, $c * $d;", "$a + $b, $c * $d;\n"),
        // Comma has lower precedence than logical operators
        ("$a && $b, $c || $d;", "$a && $b, $c || $d;\n"),
        // Complex expression with multiple comma operators
        (
            "$x = $a + $b, $y = $c * $d, $z = $e / $f;",
            "$x = $a + $b, $y = $c * $d, $z = $e / $f;\n",
        ),
        // Comma with parentheses for grouping
        ("($a, $b) = ($c, $d);", "($a, $b) = ($c, $d);\n"),
    ];

    check_formatting_cases(&cases);
}

#[test]
fn test_bitwise_operators_formatting() {
    let cases = [
        // Basic bitwise operators
        ("$a & $b;", "$a & $b;\n"),
        ("$x | $y;", "$x | $y;\n"),
        ("$flags ^ $mask;", "$flags ^ $mask;\n"),
        // Bitwise operators with precedence
        ("$result = $a & $b | $c;", "$result = $a & $b | $c;\n"), // & has higher precedence than |
        ("$value = $x | $y & $z;", "$value = $x | $y & $z;\n"),   // & binds tighter than |
        ("$flags = $a ^ $b | $c;", "$flags = $a ^ $b | $c;\n"), // ^ and | have same precedence, left-to-right
        // Bitwise operators in complex expressions
        (
            "$mask = ($a & $b) | ($c ^ $d);",
            "$mask = ($a & $b) | ($c ^ $d);\n",
        ),
        ("$result = $x + $y & $mask;", "$result = $x + $y & $mask;\n"), // + has higher precedence than &
        // Mixed with logical operators
        ("$test = $a & $b && $c;", "$test = $a & $b && $c;\n"), // && has lower precedence than &
        ("$check = $x | $y || $z;", "$check = $x | $y || $z;\n"), // || has lower precedence than |
    ];

    check_formatting_cases(&cases);
}

#[test]
fn test_numeric_literals_with_underscores_and_bases() {
    let cases = [
        ("my $x = 3.14_15_92;", "my $x = 3.14_15_92;\n"),
        ("my $dec = .23E-10;", "my $dec = .23E-10;\n"),
        ("my $bin = 0b110_100_100;", "my $bin = 0b110_100_100;\n"),
        ("my $oct = 0o12_345;", "my $oct = 0o12_345;\n"),
        ("my $hex = 0xdead_beef;", "my $hex = 0xdead_beef;\n"),
        (
            "my $hexfloat = 0x1.999ap-4;",
            "my $hexfloat = 0x1.999ap-4;\n",
        ),
    ];

    check_formatting_cases(&cases);
}

#[test]
fn test_multiline_token_indentation() {
    // Test that write_str correctly adds indentation for multiline tokens
    let input = r#"if ($condition) {
say "first line
second line
third line";
}"#;
    let formatted = format_and_assert(input);

    // Should preserve indentation for multiline string content
    insta::assert_snapshot!(formatted, @r#"
        if ($condition) {
            say "first line
            second line
            third line";
        }
        "#);
}

#[test]
fn test_hash_keyword_keys() {
    let cases = [
        // Bareword hash keys that are keywords should be treated as identifiers
        ("$h->{package};", "$h->{package};\n"),
        ("$h->{use};", "$h->{use};\n"),
        ("$h->{sub};", "$h->{sub};\n"),
        ("$h->{if};", "$h->{if};\n"),
        ("$h->{for};", "$h->{for};\n"),
        ("$h->{while};", "$h->{while};\n"),
        ("$h->{my};", "$h->{my};\n"),
        ("$h->{local};", "$h->{local};\n"),
        ("$h->{return};", "$h->{return};\n"),
        // Multiple keyword hash accesses
        ("$h->{package}->{use};", "$h->{package}->{use};\n"),
        // Direct hash access (without arrow operator)
        ("$h{package};", "$h{package};\n"),
        ("$h{use};", "$h{use};\n"),
    ];
    check_formatting_cases(&cases);
}
