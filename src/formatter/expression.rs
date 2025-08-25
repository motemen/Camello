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
        // Keep short blocks on same line, longer blocks with proper indentation

        let children = node.children_with_tokens().peekable();

        for child in children {
            match child {
                NodeOrToken::Node(child_node) => {
                    match child_node.kind() {
                        SyntaxKind::BLOCK_STMT => {
                            // Check if this is a simple, short block
                            if self.is_simple_block(&child_node) {
                                self.format_simple_block(&child_node);
                            } else {
                                self.format_node(&child_node);
                            }
                        }
                        _ => {
                            self.format_node(&child_node);
                        }
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
                        self.handle_whitespace(&token);
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
                    let kind = token.kind();
                    let text = token.text();
                    self.output.push_str(text);

                    if kind == SyntaxKind::ARROW {
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
                            SyntaxKind::WHITESPACE => {
                                self.handle_whitespace(&token);
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
                        SyntaxKind::WHITESPACE => {
                            self.handle_whitespace(&token);
                        }
                        _ => {
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
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
}
