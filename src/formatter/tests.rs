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
fn test_postfix_dereference_formatting() {
    let cases = [
        ("$ref->@*;", "$ref->@*;\n"),
        ("$ref->%*;", "$ref->%*;\n"),
        ("$ref->$*;", "$ref->$*;\n"),
        ("$foo->@*[0];", "$foo->@*[0];\n"),
        ("$bar->%*{key};", "$bar->%*{key};\n"),
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
fn test_comment_before_subroutine() {
    let input = "my$x=1;\n# before foo\nsub foo{my$y=2;}";
    let formatted = format_and_assert(input);

    insta::assert_snapshot!(formatted, @r"
        my $x = 1;
        # before foo
        sub foo {
            my $y = 2;
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
    let cases = [("-f $file;", "-f $file;\n")];
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
