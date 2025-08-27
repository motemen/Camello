use rowan::{NodeOrToken, SyntaxElementChildren};

use crate::{PerlLanguage, PerlNode, SyntaxKind};

use super::Formatter;

impl Formatter {
    pub fn format_function_call(&mut self, node: &PerlNode) {
        // Format function call: function_name arg1, arg2, arg3
        // Ensure proper spacing: space after function name, space after commas
        // Handle multiline parentheses for function parameters
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_function_call(node);
        } else {
            self.format_single_line_function_call(node);
        }
    }

    pub fn format_parenthesized_expr(&mut self, node: &PerlNode) {
        // Format any parenthesized expression with proper multiline indentation
        self.format_multiline_delimited(node, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    fn format_single_line_function_call(&mut self, node: &PerlNode) {
        // Format function call on a single line
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => {
                    self.format_token(&token);
                }
            }
        }
    }

    fn format_multiline_function_call(&mut self, node: &PerlNode) {
        // Format function call with multiline parentheses
        self.format_multiline_delimited(node, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    pub fn format_block_function_call(&mut self, node: &PerlNode) {
        // Format block function call: function_name { ... } additional_args
        // Use single-line for simple blocks (single statement, no semicolon)
        // Use multi-line for complex blocks

        // Pre-calculate which blocks are simple to avoid repeated checks
        let simple_block_ranges: std::collections::HashSet<_> = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::BLOCK_STMT && self.is_simple_block(child))
            .map(|child| child.text_range())
            .collect();

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    if child_node.kind() == SyntaxKind::BLOCK_STMT {
                        if simple_block_ranges.contains(&child_node.text_range()) {
                            self.format_simple_block(&child_node);
                        } else {
                            // Consistently use multiline formatting for complex blocks
                            self.format_multiline_delimited(
                                &child_node,
                                SyntaxKind::L_BRACE,
                                SyntaxKind::R_BRACE,
                            );
                        }
                    } else {
                        self.format_node(&child_node);
                    }
                }
                NodeOrToken::Token(token) => {
                    self.format_token(&token);
                }
            }
        }
    }

    pub fn format_method_call(&mut self, node: &PerlNode) {
        // Check if this method call should be formatted multiline
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_method_call(node);
        } else {
            self.format_single_line_method_call(node);
        }
    }

    fn format_single_line_method_call(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        for child in children.by_ref() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::WHITESPACE {
                        // Skip whitespace in method calls
                        continue;
                    }
                    self.format_token(&token);
                }
            }
            break;
        }
        self.format_subscription_iter(children, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    fn format_multiline_method_call(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        for child in children.by_ref() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::WHITESPACE {
                        // Skip whitespace in method calls
                        continue;
                    }
                    self.format_token(&token);
                }
            }
            break;
        }
        // Use multiline formatting for the parenthesized arguments
        self.format_multiline_delimited_iter(children, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    fn format_until_arrow_iter(&mut self, iter: &mut SyntaxElementChildren<PerlLanguage>) {
        for child in iter {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) if token.kind() == SyntaxKind::WHITESPACE => {}
                NodeOrToken::Token(token) => {
                    // Use format_token to ensure proper spacing is applied
                    self.format_token(&token);

                    if token.kind() == SyntaxKind::ARROW {
                        break;
                    }
                }
            }
        }
    }

    /// formats @array, %hash or its ref's [ ... ] or { ... } part
    fn format_subscription_iter(
        &mut self,
        iter: SyntaxElementChildren<PerlLanguage>,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) {
        if self.has_newline_before_first_value_iter(iter.clone()) {
            self.format_multiline_delimited_iter(iter, opening, closing);
        } else {
            for child in iter {
                match child {
                    NodeOrToken::Node(node) => self.format_node(&node),
                    NodeOrToken::Token(token) => {
                        let kind = token.kind();
                        let text = token.text();

                        match kind {
                            _ if kind == opening || kind == closing => {
                                self.output.push_str(text);
                                self.prev_token_kind = Some(kind);
                            }
                            _ => {
                                self.format_token(&token);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn format_hash_ref_access(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        self.format_subscription_iter(children, SyntaxKind::L_BRACE, SyntaxKind::R_BRACE);
    }

    pub fn format_array_ref_access(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        self.format_subscription_iter(children, SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET);
    }

    pub fn format_code_ref_call(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        self.format_subscription_iter(children, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    pub fn format_hash_subscription(&mut self, node: &PerlNode) {
        let children = node.children_with_tokens();
        self.format_subscription_iter(children, SyntaxKind::L_BRACE, SyntaxKind::R_BRACE);
    }

    pub fn format_array_subscription(&mut self, node: &PerlNode) {
        let children = node.children_with_tokens();
        self.format_subscription_iter(children, SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET);
    }

    pub fn format_deref_expr(&mut self, node: &PerlNode) {
        // Format dereference expressions (e.g., @$var, %$var, $$var)
        // Output all child elements consecutively without spaces
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    // Do not add spaces in dereference expressions
                    match kind {
                        SyntaxKind::WHITESPACE => {}
                        _ => {
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                    }
                }
            }
        }
    }

    pub fn format_reference_expr(&mut self, node: &PerlNode) {
        // Format reference expressions (e.g., \$scalar, \@array, \%hash, \&func)
        // Output all child elements consecutively without spaces between the backslash and the operand
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    // Handle spacing normally for the backslash, but no spaces within the reference expression
                    match kind {
                        SyntaxKind::BACKSLASH => {
                            // Apply normal spacing before the backslash
                            self.handle_spacing_before(kind);
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace inside reference expressions to keep them compact
                        }
                        _ => {
                            // For other tokens (sigils, identifiers, etc.), output directly without spacing
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                    }
                }
            }
        }
    }

    pub fn format_io_expr(&mut self, node: &PerlNode) {
        // Format I/O expressions (e.g., <STDIN>, <>, <$fh>)
        // Output all child elements consecutively without spaces
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    // Apply normal spacing before the I/O operator
                    match kind {
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace inside I/O expressions
                        }
                        _ => {
                            // For the opening <, apply normal spacing rules
                            if text.starts_with('<') {
                                self.handle_spacing_before(kind);
                                if self.at_line_start {
                                    self.add_indent();
                                    self.at_line_start = false;
                                }
                            }
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                    }
                }
            }
        }
    }

    pub fn format_ternary_expr(&mut self, node: &PerlNode) {
        // Format ternary expressions (e.g., condition ? true_expr : false_expr)
        // Add spaces around ? and : for readability
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        SyntaxKind::QUESTION_MARK => {
                            // Add space before ? and after ?
                            self.handle_spacing_before(kind);
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.output.push_str(text);
                            self.output.push(' ');
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::COLON => {
                            // Add space before : and after :
                            self.handle_spacing_before(kind);
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.output.push_str(text);
                            self.output.push(' ');
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Skip original whitespace, we manage spacing manually
                        }
                        _ => {
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::format;
    use crate::{formatter::tests::check_formatting_cases, parse_perl};

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
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

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
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

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
        let input = "my $a = $hash{key}; my $b = $hashref->{key}; my $c = $array[0]; my $d = $arrayref->[0];";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
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
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @"my $val = $hash{$prefix . $suffix}[$array[$index]];");
    }

    #[test]
    fn test_subscription_assignment() {
        let input = "$hash{$key} = $value;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @"$hash{$key} = $value;");
    }

    #[test]
    fn test_array_subscription_assignment() {
        let input = "$array[$index] = $value;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @"$array[$index] = $value;");
    }

    #[test]
    fn test_function_call_formatting() {
        let cases = [
            ("push@array,$value;", "push @array, $value;\n"),
            ("print$var,\"hello\",123;", "print $var, \"hello\", 123;\n"),
            ("shift@array;", "shift @array;\n"),
            // TODO: ハッシュインデックス構文をサポートする必要がある: ("delete$hash{key};", "delete $hash{key};\n"),
            ("my_func$a,$b,$c;", "my_func $a, $b, $c;\n"),
        ];
        crate::formatter::tests::check_formatting_cases(&cases);
    }

    #[test]
    fn test_function_call_in_sub() {
        let input = "sub test{push@array,$value;return$result;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

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
        crate::formatter::tests::check_formatting_cases(&cases);
    }

    #[test]
    fn test_eval_block_function_formatting() {
        let input = "eval{my$x=1;print$x;};";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        eval {
            my $x = 1;
            print $x;
        };
        ");
    }

    #[test]
    fn test_map_with_parentheses_formatting() {
        let input = "map{$_*2}(1,2,3); sort{$a+$b}@values;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        map { $_ * 2 } (1, 2, 3);
        sort { $a + $b } @values;
        ");
    }

    #[test]
    fn test_single_line_function_call_formatting() {
        let input = "func(arg1, arg2, arg3);";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"func(arg1, arg2, arg3);");
    }

    #[test]
    fn test_ternary_in_data_structures_formatting() {
        let input =
            "my $config = { timeout => $is_production ? 30 : 5, retries => $is_critical ? 3 : 1 };";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let output = format(&syntax);
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
        crate::formatter::tests::check_formatting_cases(&cases);
    }

    #[test]
    fn test_original_io_examples() {
        // Test the three examples from the original issue
        let input1 = "while (defined($_ = <STDIN>)) { print; }";
        let (syntax, err) = parse_perl(input1);
        assert!(err.is_empty(), "Parse errors for example 1: {:?}", err);
        let formatted1 = format(&syntax);

        let input2 = "while (<>) {\n    print;\n}";
        let (syntax, err) = parse_perl(input2);
        assert!(err.is_empty(), "Parse errors for example 2: {:?}", err);
        let formatted2 = format(&syntax);

        let input3 = "$line = <$fh>;";
        let (syntax, err) = parse_perl(input3);
        assert!(err.is_empty(), "Parse errors for example 3: {:?}", err);
        let formatted3 = format(&syntax);

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
}
