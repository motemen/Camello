use crate::SyntaxKind;

use super::Parser;

impl<'a> Parser<'a> {
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
            Some(SyntaxKind::FOR_KW) | Some(SyntaxKind::FOREACH_KW) => {
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
            Some(SyntaxKind::END_KW) | Some(SyntaxKind::DATA_KW) => {
                self.data_section();
                true
            }
            Some(SyntaxKind::POD_COMMAND) => {
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
                // ブロック終了なので呼び出し元に知らせる
                false
            }
            Some(_) => {
                // 式文としてパースを試みる
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
                if self.current_kind().map(|k| k.is_sigil()).unwrap_or(false) {
                    self.parse_variable_by_decl_kind(decl_kind);
                } else {
                    break; // Break loop when error occurs
                }

                self.skip_trivia();

                if self.at(SyntaxKind::COMMA) {
                    self.bump();
                    self.skip_trivia();
                } else if !self.at(SyntaxKind::R_PAREN) {
                    break; // Break loop when error occurs
                }
            }

            self.expect(SyntaxKind::R_PAREN);
        } else if self.current_kind().map(|k| k.is_sigil()).unwrap_or(false) {
            self.parse_variable_by_decl_kind(decl_kind);
        } else {
            self.error("Expected variable or parenthesized list of variables after variable declaration keyword");
        }

        self.skip_trivia();

        // 初期化式があれば処理
        if self.at(SyntaxKind::EQ) {
            self.bump(); // =
            self.skip_trivia();
            if !self.expression() {
                self.error("Invalid expression in variable assignment");
            }
        }

        self.skip_trivia();
        if expect_semicolon {
            self.expect(SyntaxKind::SEMICOLON);
        }

        self.builder.finish_node();
    }

    fn sub_def(&mut self) {
        self.builder.start_node(SyntaxKind::SUB_DEF.into());

        self.expect(SyntaxKind::SUB_KW);
        self.skip_trivia();

        // サブルーチン名（修飾付き識別子も可能）
        self.parse_identifier_or_qualified();
        self.skip_trivia();

        self.block();

        self.builder.finish_node();
    }

    fn package_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::PACKAGE_STMT.into());

        // "package"
        self.expect(SyntaxKind::PACKAGE_KW);
        self.skip_trivia();

        // パッケージ名（修飾付き識別子）
        self.parse_identifier_or_qualified();
        self.skip_trivia();

        // セミコロン
        self.expect(SyntaxKind::SEMICOLON);

        self.builder.finish_node();
    }

    fn use_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::USE_STMT.into());

        // "use"
        self.expect(SyntaxKind::USE_KW);
        self.skip_trivia();

        // VERSION literal or module name (qualified identifier)
        if self.at(SyntaxKind::VERSION) {
            // Version literal (e.g., use v5.42;)
            self.bump();
        } else {
            // モジュール名（修飾付き識別子）
            self.parse_identifier_or_qualified();
        }
        self.skip_trivia();

        // オプション：インポートリスト（qw() など）
        if self.is_at_start_of_expression() {
            self.expression();
            self.skip_trivia();
        }

        // セミコロン
        self.expect(SyntaxKind::SEMICOLON);

        self.builder.finish_node();
    }

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
        if self.at_any(&[SyntaxKind::MY_KW, SyntaxKind::OUR_KW, SyntaxKind::LOCAL_KW]) {
            // Variable declaration case - parse as a variable declaration
            self.builder.start_node(SyntaxKind::DECLARATION_STMT.into());

            let decl_kind = self.current_kind().unwrap();
            self.bump(); // consume the keyword
            self.skip_trivia();

            // Parse the variable - must be a scalar
            if self.at(SyntaxKind::DOLLAR) {
                // Use qualified parsing for our/local, simple for my
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

        // Condition expression in parentheses: while (expr)
        if self.at(SyntaxKind::L_PAREN) {
            self.bump(); // (
            self.skip_trivia();

            // Parse the while condition
            if !self.expression() {
                self.error("Expected expression in while condition");
            }

            self.skip_trivia();
            self.expect(SyntaxKind::R_PAREN);
        } else {
            self.error("Expected '(' after 'while'");
        }

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

        // Condition expression in parentheses: if (expr)
        if self.at(SyntaxKind::L_PAREN) {
            self.bump(); // (
            self.skip_trivia();

            // Parse the if condition
            if !self.expression() {
                self.error("Expected expression in if condition");
            }

            self.skip_trivia();
            self.expect(SyntaxKind::R_PAREN);
        } else {
            self.error("Expected '(' after 'if'");
        }

        self.skip_trivia();

        // If block
        self.block();

        self.skip_trivia();

        while self.at(SyntaxKind::ELSIF_KW) {
            self.bump(); // elsif
            self.skip_trivia();

            if self.at(SyntaxKind::L_PAREN) {
                self.bump(); // (
                self.skip_trivia();

                // Parse the if condition
                if !self.expression() {
                    self.error("Expected expression in elsif condition");
                }

                self.skip_trivia();
                self.expect(SyntaxKind::R_PAREN);
            } else {
                self.error("Expected '(' after 'elsif'");
            }

            self.skip_trivia();

            self.block();

            self.skip_trivia();
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

    fn block(&mut self) {
        self.builder.start_node(SyntaxKind::BLOCK_STMT.into());

        self.expect(SyntaxKind::L_BRACE);
        self.skip_trivia();

        while !self.at(SyntaxKind::R_BRACE) && !self.at_end() {
            if !self.statement() {
                self.error("Expected a statement in block, but found an unexpected token.");
            }
            self.skip_trivia();
        }

        self.expect(SyntaxKind::R_BRACE);

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
}
