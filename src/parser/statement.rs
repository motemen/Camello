use crate::SyntaxKind;

use super::Parser;

impl Parser<'_> {
    pub fn statement(&mut self) -> bool {
        self.skip_trivia();

        match self.current_kind() {
            Some(
                SyntaxKind::MY_KW
                | SyntaxKind::OUR_KW
                | SyntaxKind::STATE_KW
                | SyntaxKind::LOCAL_KW,
            ) => {
                self.var_decl();
                true
            }
            Some(SyntaxKind::SUB_KW) => {
                self.sub_def();
                true
            }
            Some(SyntaxKind::IF_KW) => {
                self.if_stmt();
                true
            }
            Some(SyntaxKind::UNLESS_KW) => {
                self.unless_stmt();
                true
            }
            Some(SyntaxKind::FOR_KW | SyntaxKind::FOREACH_KW) => {
                self.for_stmt();
                true
            }
            Some(SyntaxKind::WHILE_KW) => {
                self.while_stmt();
                true
            }
            Some(SyntaxKind::PACKAGE_KW) => {
                self.package_stmt();
                true
            }
            Some(SyntaxKind::USE_KW) => {
                self.use_stmt();
                true
            }
            Some(SyntaxKind::NO_KW) => {
                self.no_stmt();
                true
            }
            Some(SyntaxKind::END_KW | SyntaxKind::DATA_KW) => {
                self.data_section();
                true
            }
            Some(SyntaxKind::POD_CONTENT) => {
                self.pod_block();
                true
            }
            Some(SyntaxKind::CUT_KW) => {
                // =cut without a preceding POD block is an error
                self.error("Found =cut without a preceding POD command");
                self.bump(); // Consume the =cut token
                true
            }
            Some(SyntaxKind::R_BRACE) => {
                // End of block, notify the caller.
                false
            }
            Some(_) => {
                // Try to parse as an expression statement
                self.expression_stmt()
            }
            None => false, // EOF
        }
    }

    fn var_decl(&mut self) {
        self.var_decl_common(true);
    }

    // Variable declaration as expression (no semicolon expected)
    pub fn var_decl_expr(&mut self) {
        self.var_decl_common(false);
    }

    // Helper to parse variable based on declaration kind
    fn parse_variable_by_decl_kind(&mut self, decl_kind: SyntaxKind) {
        if matches!(decl_kind, SyntaxKind::OUR_KW | SyntaxKind::LOCAL_KW) {
            self.parse_variable_qualified();
        } else {
            self.parse_variable_simple();
        }
    }

    // Common logic for variable declarations
    fn var_decl_common(&mut self, expect_semicolon: bool) {
        self.builder.start_node(SyntaxKind::DECLARATION_STMT.into());

        // Variable declaration keyword (my, our, state, local)
        let decl_kind = self.current_kind().unwrap();
        self.bump(); // consume the keyword
        self.skip_trivia();

        // my $var or my ($var, ...)
        if self.at(SyntaxKind::L_PAREN) {
            self.bump(); // (
            self.skip_trivia();

            while !self.at(SyntaxKind::R_PAREN) && !self.at_end() {
                if self
                    .current_kind()
                    .is_some_and(super::super::syntax_kind::SyntaxKind::is_sigil)
                {
                    self.parse_variable_by_decl_kind(decl_kind);
                } else if self.at(SyntaxKind::UNDEF_KW) {
                    self.bump(); // consume 'undef'
                } else {
                    self.error("Expected variable in parenthesized list");
                    break; // Break loop when error occurs
                }

                self.skip_trivia();

                if self.at(SyntaxKind::COMMA) {
                    self.bump();
                    self.skip_trivia();
                } else if !self.at(SyntaxKind::R_PAREN) {
                    self.error("Expected ',' or ')' in variable list");
                    break; // Break loop when error occurs
                }
            }

            self.expect(SyntaxKind::R_PAREN);
        } else if self
            .current_kind()
            .is_some_and(super::super::syntax_kind::SyntaxKind::is_sigil)
        {
            self.parse_variable_by_decl_kind(decl_kind);
        } else {
            self.error("Expected variable or parenthesized list of variables after variable declaration keyword");
        }

        self.skip_trivia();

        // Process initializer if present
        if self.at(SyntaxKind::EQ) {
            self.bump(); // =
            self.skip_trivia();
            if !self.expression() {
                self.error("Invalid expression in variable assignment");
            }
        }

        self.skip_trivia();

        // Check for postfix conditionals (if/unless modifiers)
        self.parse_optional_postfix_conditional();

        if expect_semicolon {
            self.expect(SyntaxKind::SEMICOLON);
        }

        self.builder.finish_node();
    }

    fn sub_def(&mut self) {
        self.builder.start_node(SyntaxKind::SUB_DEF.into());

        self.expect(SyntaxKind::SUB_KW);
        self.skip_trivia();

        // Subroutine name (qualified identifier also allowed); keywords accepted as identifiers
        self.parse_identifier_or_qualified();
        self.skip_trivia();

        // Parse optional prototype
        if self.at(SyntaxKind::L_PAREN) {
            self.parse_sub_prototype();
            self.skip_trivia();
        }

        self.block();

        self.builder.finish_node();
    }

    fn package_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::PACKAGE_STMT.into());

        // "package"
        self.expect(SyntaxKind::PACKAGE_KW);
        self.skip_trivia();

        // Package name (qualified identifier); allow keywords as identifiers
        self.parse_identifier_or_qualified();
        self.skip_trivia();

        // Semicolon
        self.expect(SyntaxKind::SEMICOLON);

        self.builder.finish_node();
    }

    fn use_or_no_stmt(&mut self, is_use: bool) {
        let (keyword_kind, stmt_kind) = if is_use {
            (SyntaxKind::USE_KW, SyntaxKind::USE_STMT)
        } else {
            (SyntaxKind::NO_KW, SyntaxKind::NO_STMT)
        };

        self.builder.start_node(stmt_kind.into());

        // "use" or "no"
        self.expect(keyword_kind);
        self.skip_trivia();

        // VERSION literal or module name (qualified identifier)
        if self.at(SyntaxKind::VERSION) {
            // Version literal (e.g., use v5.42; or no v5.42;)
            self.bump();
        } else if self.at(SyntaxKind::BARE_VERSION) {
            // Bare version literal (e.g., use 5.24.1; or no 5.24.1;)
            self.bump();
        } else if self.at(SyntaxKind::NUMBER) {
            // Simple version number (e.g., use 5; or no 5;)
            self.bump();
        } else {
            // Module name (qualified identifier); allow keywords as identifiers
            self.parse_identifier_or_qualified();
        }
        self.skip_trivia();

        // Option: import list (e.g., qw()) or comma-separated expressions (x => 1, y => 2)
        if self.is_at_start_of_expression() {
            // Parse first expression
            self.expression();

            // Handle additional comma-separated expressions
            while self.at(SyntaxKind::COMMA) {
                self.bump(); // consume comma
                self.skip_trivia();

                if self.is_at_start_of_expression() {
                    self.expression();
                } else {
                    // Allow trailing comma
                    break;
                }
            }
        }

        // Semicolon
        self.expect(SyntaxKind::SEMICOLON);

        self.builder.finish_node();
    }

    fn use_stmt(&mut self) {
        self.use_or_no_stmt(true);
    }

    fn no_stmt(&mut self) {
        self.use_or_no_stmt(false);
    }

    // removed parse_module_name_or_qualified; logic centralized in parse_identifier_or_qualified

    fn for_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::FOR_STMT.into());

        // "for" or "foreach" - already validated by statement()
        self.bump();
        self.skip_trivia();

        // Check what comes next to determine the for loop style:
        // 1. Perl-style: for VAR (LIST) BLOCK - VAR starts with sigil or "my"
        // 2. C-style: for (EXPR) BLOCK - starts with "("

        if self.at(SyntaxKind::L_PAREN) {
            // C-style for loop: for (EXPR) BLOCK
            self.bump(); // (
            self.skip_trivia();

            // Parse the condition/iterator expression
            if !self.expression() {
                self.error("Expected expression in for condition");
            }

            self.skip_trivia();
            self.expect(SyntaxKind::R_PAREN);
        } else {
            // Perl-style for loop: for VAR (LIST) BLOCK
            // Parse the iterator variable (VAR part): my $var, $var, @var, etc.
            self.parse_for_variable();
            self.skip_trivia();

            // List expression in parentheses: (LIST)
            if self.at(SyntaxKind::L_PAREN) {
                self.bump(); // (
                self.skip_trivia();

                // Parse the list expression
                if !self.expression() {
                    self.error("Expected expression in for list");
                }

                self.skip_trivia();
                self.expect(SyntaxKind::R_PAREN);
            } else {
                self.error("Expected '(' after for variable");
            }
        }

        self.skip_trivia();

        // Block
        self.block();

        self.builder.finish_node();
    }

    /// Parse the variable part of a for loop (my $var, $var)
    fn parse_for_variable(&mut self) {
        if self.at_any(&[
            SyntaxKind::MY_KW,
            SyntaxKind::OUR_KW,
            SyntaxKind::STATE_KW,
            SyntaxKind::LOCAL_KW,
        ]) {
            // Variable declaration case - parse as a variable declaration
            self.builder.start_node(SyntaxKind::DECLARATION_STMT.into());

            let decl_kind = self.current_kind().unwrap();
            self.bump(); // consume the keyword
            self.skip_trivia();

            // Parse the variable - must be a scalar
            if self.at(SyntaxKind::DOLLAR) {
                // Use qualified parsing for our/local, simple for my/state
                if matches!(decl_kind, SyntaxKind::OUR_KW | SyntaxKind::LOCAL_KW) {
                    self.parse_variable_qualified();
                } else {
                    self.parse_variable_simple();
                }
            } else {
                self.error(
                    "Expected scalar variable after variable declaration keyword in for loop",
                );
            }

            self.builder.finish_node();
        } else if self.at(SyntaxKind::DOLLAR) {
            // $var case - parse as a variable reference
            self.parse_variable();
        } else {
            self.error("Expected scalar variable or 'my' declaration in for loop");
        }
    }

    fn while_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::WHILE_STMT.into());

        // "while"
        self.expect(SyntaxKind::WHILE_KW);
        self.skip_trivia();

        // Parse parenthesized condition
        self.parse_parenthesized_condition("while");

        self.skip_trivia();

        // Block
        self.block();

        self.builder.finish_node();
    }

    fn if_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::IF_STMT.into());

        // "if"
        self.expect(SyntaxKind::IF_KW);
        self.skip_trivia();

        // Parse parenthesized condition
        self.parse_parenthesized_condition("if");

        self.skip_trivia();

        // If block
        self.block();

        // Skip trivia only if we detect elsif/else ahead, to avoid consuming inter-statement whitespace
        // FIXME: This implementation is buggy. It fails if there's a comment between the if block and elsif/else.
        // A more robust solution would be:
        // 1. Implement a proper multi-lookahead mechanism in the lexer.
        // 2. Implement a lexer with pushback capability.
        if self.lookahead_for_elsif_or_else() {
            self.skip_trivia();
        }

        while self.at(SyntaxKind::ELSIF_KW) {
            self.bump(); // elsif
            self.skip_trivia();

            // Parse parenthesized condition
            self.parse_parenthesized_condition("elsif");

            self.skip_trivia();

            self.block();

            // Skip trivia only if more elsif/else ahead
            if self.lookahead_for_elsif_or_else() {
                self.skip_trivia();
            }
        }

        // "else"
        if self.at(SyntaxKind::ELSE_KW) {
            self.bump(); // else
            self.skip_trivia();

            // Else block
            self.block();
        }

        self.builder.finish_node();
    }

    /// Look ahead to see if there's an elsif or else keyword after whitespace
    fn lookahead_for_elsif_or_else(&self) -> bool {
        // Use token-based lookahead to check for elsif or else keywords
        self.lookahead_for_any(&[SyntaxKind::ELSIF_KW, SyntaxKind::ELSE_KW])
    }

    fn unless_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::UNLESS_STMT.into());

        // "unless"
        self.expect(SyntaxKind::UNLESS_KW);
        self.skip_trivia();

        // Parse parenthesized condition
        self.parse_parenthesized_condition("unless");

        self.skip_trivia();

        // Unless block
        self.block();

        self.builder.finish_node();
    }

    fn expression_stmt(&mut self) -> bool {
        if !self.is_at_start_of_expression() {
            return false;
        }

        self.builder.start_node(SyntaxKind::STMT.into());
        let success = self.expression();

        if !success {
            // Since this is checked by `is_at_start_of_expression`, this branch should
            // only be reached if the implementation of `expression()` is incomplete.
            // Ideally, something like `builder.abandon_node()` would be desirable,
            // but since `GreenNodeBuilder` doesn't have it, we close it as an error node.
            self.error("Invalid expression statement");
            self.builder.finish_node();
            return true; // Consumed as an error, so return true.
        }

        self.skip_trivia();

        // Check for postfix conditionals (if/unless modifiers)
        self.parse_optional_postfix_conditional();

        // Check if semicolon is required
        // Semicolons are required except for the last statement in a block, end of file, or before data sections
        if self.at(SyntaxKind::SEMICOLON) {
            self.bump();
        } else if self.at_end()
            || self.at_any(&[SyntaxKind::R_BRACE, SyntaxKind::END_KW, SyntaxKind::DATA_KW])
        {
            // Last statement in a block, end of file, or before data section - semicolon is optional
            // Don't consume tokens here, let the appropriate handler consume them
        } else {
            // Semicolon is required but missing
            self.error("Expected ';' after expression statement");
        }

        self.builder.finish_node();
        true
    }

    pub fn block(&mut self) {
        self.builder.start_node(SyntaxKind::BLOCK_STMT.into());

        // Entering a block; inside expects statements/values
        self.expect_value(SyntaxKind::L_BRACE);
        self.skip_trivia();

        while !self.at(SyntaxKind::R_BRACE) && !self.at_end() {
            if !self.statement() {
                self.error("Expected a statement in block, but found an unexpected token.");
            }
            self.skip_trivia();
        }

        // After closing '}', expect an operator/statement boundary
        self.expect_op(SyntaxKind::R_BRACE);

        self.builder.finish_node();
    }

    fn parse_optional_postfix_conditional(&mut self) {
        if self.at(SyntaxKind::IF_KW) || self.at(SyntaxKind::UNLESS_KW) {
            self.parse_postfix_conditional();
        }
    }

    fn parse_postfix_conditional(&mut self) {
        let keyword_kind = self
            .current_kind()
            .expect("Current token should be if/unless keyword");
        let modifier_kind = if keyword_kind == SyntaxKind::IF_KW {
            SyntaxKind::IF_MODIFIER
        } else {
            SyntaxKind::UNLESS_MODIFIER
        };

        self.builder.start_node(modifier_kind.into());

        // Consume the if/unless keyword; next should be a value (condition)
        self.bump_value();
        self.skip_trivia();

        // Parse the condition expression
        if !self.expression() {
            self.error("Expected condition after postfix if/unless");
        }

        self.builder.finish_node();
    }

    /// Helper function to parse parenthesized conditions for if/unless/while/elsif statements
    fn parse_parenthesized_condition(&mut self, construct_name: &str) {
        if self.at(SyntaxKind::L_PAREN) {
            // Inside condition parens, expect values
            self.bump_value(); // (
            self.skip_trivia();

            // Parse the condition
            if !self.expression() {
                self.error(&format!(
                    "Expected expression in {construct_name} condition"
                ));
            }

            self.skip_trivia();
            // After ')', expect operator/statement boundary
            self.expect_op(SyntaxKind::R_PAREN);
        } else {
            self.error(&format!("Expected '(' after '{construct_name}'"));
        }
    }

    /// Parse subroutine prototype like (\@@), ($@), (\@$@), etc.
    fn parse_sub_prototype(&mut self) {
        use crate::lexer::LexMode;
        self.builder.start_node(SyntaxKind::SUB_PROTOTYPE.into());

        self.expect(SyntaxKind::L_PAREN);
        self.skip_trivia();

        while let Some((kind, _)) = self.peek_non_trivia_token_with(LexMode::Value) {
            if kind == SyntaxKind::R_PAREN {
                break;
            }
            match kind {
                SyntaxKind::BACKSLASH
                | SyntaxKind::AT
                | SyntaxKind::PERCENT
                | SyntaxKind::DOLLAR
                | SyntaxKind::AMPERSAND
                | SyntaxKind::ASTERISK
                | SyntaxKind::SEMICOLON
                | SyntaxKind::L_BRACKET
                | SyntaxKind::R_BRACKET
                | SyntaxKind::L_PAREN
                | SyntaxKind::R_PAREN => {
                    self.bump_with_expectation(LexMode::Value);
                    self.skip_trivia();
                }
                _ => {
                    self.error("Invalid character in subroutine prototype");
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_PAREN);
        self.builder.finish_node();
    }
}

#[cfg(test)]
mod tests {
    use crate::{parse, PerlNode, SyntaxKind};

    #[test]
    fn test_semicolon_requirements() {
        // Test cases to verify semicolon requirements
        let test_cases = [
            // Valid cases: semicolons present where required
            ("foo(); bar();", true),
            ("print(1); print(2);", true),
            // Valid cases: single statement at EOF without semicolon
            ("foo()", true),
            // Valid cases: subroutines with multiple statements (last one without semicolon)
            ("sub test { foo(); bar() }", true),
            ("sub test { foo(); bar(); baz() }", true),
            // Invalid cases: statements within blocks missing required semicolons
            ("sub test { foo() bar() }", false), // Missing semicolon between foo() and bar()
            // Invalid cases: missing semicolon between statements
            ("foo() bar()", false),
            ("print(1) print(2)", false),
            ("$x = 1 $y = 2", false),
            // Valid cases: semicolon before data sections
            (
                "foo()
__DATA__",
                true,
            ),
            (
                "foo()
__END__",
                true,
            ),
        ];

        for (input, should_succeed) in test_cases {
            let (green, errors) = parse(input);
            let syntax = PerlNode::new_root(green);

            // All inputs should parse structurally (create a CST)
            assert_eq!(
                syntax.kind(),
                SyntaxKind::ROOT,
                "Failed to parse: '{}'",
                input
            );

            if should_succeed {
                // Should parse without errors
                assert!(
                    errors.is_empty(),
                    "Should parse '{}' without errors, but got: {:?}",
                    input,
                    errors
                );
            } else {
                // Should generate parse errors for missing semicolons
                assert!(
                    !errors.is_empty(),
                    "Should generate parse error for '{}' but didn't",
                    input
                );
                // Check that the error mentions semicolon
                assert!(
                    errors.iter().any(|e| e.message.contains(";")),
                    "Error message should mention semicolon for '{}', but got: {:?}",
                    input,
                    errors
                );
            }
        }
    }

    #[test]
    fn test_elsif_else_lookahead_functionality() {
        // Test that lookahead_for_elsif_or_else works with token-based lookahead
        // This method is used to peek ahead and see if elsif/else follows

        // Test with whitespace before keywords - this is the main use case
        let parser = crate::parser::Parser::new("  elsif");
        assert!(
            parser.lookahead_for_elsif_or_else(),
            "Should detect 'elsif' with leading whitespace"
        );

        let parser = crate::parser::Parser::new("\n\telse");
        assert!(
            parser.lookahead_for_elsif_or_else(),
            "Should detect 'else' with leading whitespace and newline"
        );

        // Test the realistic scenario - positioned after a closing brace, looking for elsif/else
        let parser = crate::parser::Parser::new("} elsif");
        // The parser starts at the closing brace
        assert_eq!(parser.current_kind(), Some(SyntaxKind::R_BRACE));
        // Advance past the brace to simulate the real usage
        let mut parser = parser;
        parser.bump(); // consume the }
                       // Now we should be at whitespace, and lookahead should find elsif
        assert!(
            parser.lookahead_for_elsif_or_else(),
            "Should detect 'elsif' after closing brace"
        );

        // Test cases where elsif/else should NOT be detected
        let should_not_detect_cases = [
            "foo",
            "my",
            "", // empty input
            "if",
            "while",
            "# comment\nfoo", // comment followed by non-elsif/else
        ];

        for input in should_not_detect_cases {
            let parser = crate::parser::Parser::new(input);
            assert!(
                !parser.lookahead_for_elsif_or_else(),
                "Should NOT detect elsif/else for input: '{}'",
                input
            );
        }

        // Test a simpler validation that if/elsif/else can be parsed correctly
        let full_if_input = "if (1) { } elsif (2) { }";
        let (green, errors) = crate::parse(full_if_input);

        // This should parse without errors
        assert!(
            errors.is_empty(),
            "Should parse if/elsif without errors, got: {:?}",
            errors
        );

        let syntax = crate::PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
    }
}
