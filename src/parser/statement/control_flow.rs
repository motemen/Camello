use crate::parser::Parser;
use crate::{lexer::LexContext, SyntaxKind, T};

impl Parser<'_> {
    pub(super) fn for_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::FOR_STMT.into());
        self.bump_value(); // consume "for"
        self.skip_whitespace_and_newlines();

        // Dispatch based on whether we have a left parenthesis
        if self.current_kind() == Some(T!['(']) {
            self.bump_value(); // consume "("
            self.skip_whitespace_and_newlines();

            // Parse first expression or expression list
            if self.current_kind() != Some(T![;]) && !self.expression_list() {
                self.error("Expected expression in for initializer");
            }

            // Check if this is C-style (has semicolon) or Perl-style with parentheses
            if self.current_kind() == Some(T![;]) {
                self.parse_c_style_for_loop();
            }
            // else: Perl-style for loop with parentheses - expression already parsed

            self.skip_whitespace_and_newlines();
            // Expect closing parenthesis
            self.expect_op(T![')']);
        } else {
            self.parse_perl_style_for_loop();
        }

        self.skip_whitespace_and_newlines();

        // Parse the body block
        self.block();

        self.builder.finish_node();
    }

    /// Parse the C-style for loop components: condition; increment
    /// Expects to be positioned right before the first semicolon
    fn parse_c_style_for_loop(&mut self) {
        // C-style for loop: for (init; condition; increment)
        self.bump_value(); // consume first semicolon
        self.skip_whitespace_and_newlines();

        // Parse condition (optional)
        if self.current_kind() != Some(T![;]) {
            self.expression();
        }

        self.skip_whitespace_and_newlines();
        // Expect second semicolon
        self.expect_value(T![;]);

        self.skip_whitespace_and_newlines();
        // Parse increment (optional)
        if self.current_kind() != Some(T![')']) {
            self.expression();
        }
    }

    /// Parse Perl-style for loop without parentheses: for VAR (LIST) BLOCK
    fn parse_perl_style_for_loop(&mut self) {
        // Use existing parse_for_variable function for this case
        self.parse_for_variable();
        self.skip_whitespace_and_newlines();

        // List expression in parentheses: (LIST)
        if self.current_kind() == Some(T!['(']) {
            self.bump_value(); // consume "("
            self.skip_whitespace_and_newlines();

            // Parse the list expression - can be multiple expressions separated by commas
            if !self.expression_list() {
                self.error("Expected expression in for list");
            }

            self.skip_whitespace_and_newlines();
            self.expect_op(T![')']);
        } else {
            self.error("Expected '(' after for variable");
        }
    }

    /// Parse the variable part of a for loop (my $var, $var)
    fn parse_for_variable(&mut self) {
        if matches!(
            self.current_kind(),
            Some(T![my] | T![our] | T![state] | T![local])
        ) {
            // Variable declaration case - parse as a variable declaration
            self.builder.start_node(SyntaxKind::VAR_DECL.into());

            let decl_kind = self.current_kind().unwrap();
            self.bump_value(); // consume the keyword
            self.skip_whitespace_and_newlines();

            // Parse the variable - must be a scalar
            if self.current_kind() == Some(SyntaxKind::SCALAR_SIGIL) {
                // Use qualified parsing for our/local, simple for my/state
                if matches!(decl_kind, T![our] | T![local]) {
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
        } else if self.current_kind() == Some(SyntaxKind::SCALAR_SIGIL) {
            // $var case - parse as a variable reference
            self.parse_variable();
        } else {
            self.error("Expected scalar variable or 'my' declaration in for loop");
        }
    }

    pub(super) fn while_stmt(&mut self) {
        self.parse_loop_statement(SyntaxKind::WHILE_STMT, T![while], "while", true);
    }

    pub(super) fn until_stmt(&mut self) {
        self.parse_loop_statement(SyntaxKind::UNTIL_STMT, T![until], "until", false);
    }

    /// Helper function to parse loop statements like while/until
    fn parse_loop_statement(
        &mut self,
        stmt_kind: SyntaxKind,
        kw_kind: SyntaxKind,
        construct_name: &str,
        allow_empty_condition: bool,
    ) {
        self.builder.start_node(stmt_kind.into());

        // "while" or "until"
        self.expect(kw_kind);
        self.skip_whitespace_and_newlines();

        // Parse parenthesized condition
        self.parse_parenthesized_condition(construct_name, allow_empty_condition);

        self.skip_whitespace_and_newlines();

        // Block
        self.block();

        self.builder.finish_node();
    }

    pub(super) fn if_stmt(&mut self) {
        self.parse_conditional_stmt(SyntaxKind::IF_STMT, T![if], "if");
    }

    /// Look ahead to see if there's an elsif or else keyword after whitespace
    pub(super) fn lookahead_for_elsif_or_else(&self) -> bool {
        // Use token-based lookahead to check for elsif or else keywords, skipping any trivia
        self.peek_non_trivia_token_with_context(LexContext::Operator)
            .is_some_and(|(kind, _)| matches!(kind, T![elsif] | T![else]))
    }

    pub(super) fn unless_stmt(&mut self) {
        self.parse_conditional_stmt(SyntaxKind::UNLESS_STMT, T![unless], "unless");
    }

    fn parse_conditional_stmt(
        &mut self,
        stmt_kind: SyntaxKind,
        initial_keyword: SyntaxKind,
        keyword_name: &str,
    ) {
        self.builder.start_node(stmt_kind.into());

        // Initial keyword (if/unless)
        self.expect(initial_keyword);
        self.skip_whitespace_and_newlines();

        // Parse parenthesized condition
        self.parse_parenthesized_condition(keyword_name, false);

        self.skip_whitespace_and_newlines();

        // Initial block
        self.block();

        // Skip trivia only if we detect elsif/else ahead, to avoid consuming inter-statement whitespace
        // FIXME: This implementation is buggy. It fails if there's a comment between the block and elsif/else.
        // A more robust solution would be:
        // 1. Implement a proper multi-lookahead mechanism in the lexer.
        // 2. Implement a lexer with pushback capability.
        if self.lookahead_for_elsif_or_else() {
            self.skip_whitespace_and_newlines();
        }

        while self.at(T![elsif]) {
            self.bump(); // elsif
            self.skip_whitespace_and_newlines();

            // Parse parenthesized condition
            self.parse_parenthesized_condition("elsif", false);

            self.skip_whitespace_and_newlines();

            self.block();

            // Skip trivia only if more elsif/else ahead
            if self.lookahead_for_elsif_or_else() {
                self.skip_whitespace_and_newlines();
            }
        }

        // "else"
        if self.at(T![else]) {
            self.bump(); // else
            self.skip_whitespace_and_newlines();

            // Else block
            self.block();
        }

        self.builder.finish_node();
    }

    /// Helper function to parse parenthesized conditions for if/unless/while/until/elsif statements
    fn parse_parenthesized_condition(&mut self, construct_name: &str, allow_empty_condition: bool) {
        if self.at(T!['(']) {
            // Inside condition parens, expect values
            self.bump_value(); // (
            self.skip_whitespace_and_newlines();

            // Parse the condition
            let has_condition = if self.at(T![')']) {
                false
            } else {
                self.expression_list()
            };

            if !has_condition && !allow_empty_condition {
                self.error(&format!(
                    "Expected expression in {construct_name} condition"
                ));
            }

            self.skip_whitespace_and_newlines();
            // After ')', expect operator/statement boundary
            self.expect_op(T![')']);
        } else {
            self.error(&format!("Expected '(' after '{construct_name}'"));
        }
    }

    pub(super) fn given_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::GIVEN_STATEMENT.into());

        // "given"
        self.expect(T![given]);
        self.skip_whitespace_and_newlines();

        // Parse parenthesized expression
        self.parse_parenthesized_condition("given", false);

        self.skip_whitespace_and_newlines();

        // Parse the main block containing when/default clauses
        self.parse_given_block();

        self.builder.finish_node();
    }

    fn parse_given_block(&mut self) {
        self.builder.start_node(SyntaxKind::BLOCK_STMT.into());

        if self.at(T!['{']) {
            self.bump_value(); // consume '{'
            self.skip_whitespace_and_newlines();

            // Parse when/default clauses and other statements within the block
            while !self.at(T!['}']) && !self.at_end() {
                if self.at(T![when]) {
                    self.when_clause();
                } else if self.at(T![default]) {
                    self.default_clause();
                } else {
                    // Parse regular statements within the given block
                    if !self.statement() {
                        break;
                    }
                }
                self.skip_whitespace_and_newlines();
            }

            self.expect_op(T!['}']);
        } else {
            self.error("Expected '{' after given condition");
        }

        self.builder.finish_node();
    }

    fn when_clause(&mut self) {
        self.builder.start_node(SyntaxKind::WHEN_CLAUSE.into());

        // "when"
        self.expect(T![when]);
        self.skip_whitespace_and_newlines();

        // Parse the when condition - always use parenthesized form
        self.parse_parenthesized_condition("when", false);

        self.skip_whitespace_and_newlines();

        // Parse the when block
        self.block();

        self.builder.finish_node();
    }

    fn default_clause(&mut self) {
        self.builder.start_node(SyntaxKind::DEFAULT_CLAUSE.into());

        // "default"
        self.expect(T![default]);
        self.skip_whitespace_and_newlines();

        // Parse the default block
        self.block();

        self.builder.finish_node();
    }
}
