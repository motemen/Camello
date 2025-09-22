use crate::{lexer::LexContext, SyntaxKind};

use super::Parser;

impl Parser<'_> {
    pub fn statement(&mut self) -> bool {
        self.skip_whitespace_and_newlines();

        // Check for labeled statement: IDENT followed by ':'
        if self.at(SyntaxKind::IDENT) {
            if let Some((next_kind, _)) =
                self.peek_nth_non_trivia_token_with_context(LexContext::Operator, 1)
            {
                if next_kind == SyntaxKind::COLON {
                    self.labeled_stmt();
                    return true;
                }
            }
        }

        match self.current_kind() {
            Some(kind @ (SyntaxKind::MY_KW | SyntaxKind::OUR_KW | SyntaxKind::STATE_KW)) => {
                if self.looks_like_lexical_sub_definition() {
                    self.lexical_sub_def(kind);
                    true
                } else {
                    // Route through expression system for prefix operator handling
                    self.expression_stmt()
                }
            }
            Some(SyntaxKind::LOCAL_KW) => {
                // Route through expression system for prefix operator handling
                self.expression_stmt()
            }
            Some(SyntaxKind::SUB_KW) => {
                if self.looks_like_sub_definition() {
                    self.sub_def();
                    true
                } else {
                    self.expression_stmt()
                }
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
            Some(SyntaxKind::UNTIL_KW) => {
                self.until_stmt();
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
            Some(kind) if kind.is_phase_block_kw() => {
                self.phase_block_stmt(kind);
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
            Some(SyntaxKind::ELLIPSIS) => {
                self.ellipsis_stmt();
                true
            }
            Some(SyntaxKind::SEMICOLON) => {
                self.empty_stmt();
                true
            }
            Some(SyntaxKind::L_BRACE) => {
                // Check if this looks like a hash reference based on content only
                if self.looks_like_hash_ref() {
                    // This is likely a hash reference expression, parse as expression statement
                    self.expression_stmt()
                } else {
                    // Bare block statement (e.g., { ... }) used for scoping or flow control
                    self.builder.start_node(SyntaxKind::STMT.into());
                    self.block();
                    self.builder.finish_node();
                    true
                }
            }
            Some(_) => {
                // Try to parse as an expression statement
                self.expression_stmt()
            }
            None => false, // EOF
        }
    }

    fn looks_like_sub_definition(&self) -> bool {
        match self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1) {
            Some((next, _)) => {
                next == SyntaxKind::DOUBLE_COLON || next == SyntaxKind::IDENT || next.is_keyword()
            }
            None => false,
        }
    }

    fn looks_like_lexical_sub_definition(&self) -> bool {
        self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1)
            .is_some_and(|(next, _)| next == SyntaxKind::SUB_KW)
    }

    fn labeled_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::LABELED_STMT.into());

        // Label node: IDENT ':'
        self.builder.start_node(SyntaxKind::LABEL.into());
        self.expect(SyntaxKind::IDENT);
        self.skip_whitespace_and_newlines();
        self.expect(SyntaxKind::COLON);
        self.builder.finish_node();

        self.skip_whitespace_and_newlines();

        if !self.statement() {
            self.error("Expected statement after label");
        }

        self.builder.finish_node();
    }

    fn sub_def(&mut self) {
        self.sub_def_with_modifier(None);
    }

    fn lexical_sub_def(&mut self, modifier_kind: SyntaxKind) {
        self.sub_def_with_modifier(Some(modifier_kind));
    }

    fn sub_def_with_modifier(&mut self, modifier_kind: Option<SyntaxKind>) {
        self.builder.start_node(SyntaxKind::SUB_DEF.into());

        if let Some(kind) = modifier_kind {
            self.expect(kind);
            self.skip_whitespace_and_newlines();
        }

        self.expect(SyntaxKind::SUB_KW);
        self.skip_whitespace_and_newlines();

        // Subroutine name (qualified identifier also allowed); keywords accepted as identifiers
        self.parse_identifier_or_qualified();
        self.skip_whitespace_and_newlines();

        self.parse_sub_tail();

        self.builder.finish_node();
    }

    fn phase_block_stmt(&mut self, keyword_kind: SyntaxKind) {
        let name = match keyword_kind {
            SyntaxKind::BEGIN_KW => "BEGIN",
            SyntaxKind::END_BLOCK_KW => "END",
            SyntaxKind::INIT_KW => "INIT",
            SyntaxKind::CHECK_KW => "CHECK",
            SyntaxKind::UNITCHECK_KW => "UNITCHECK",
            _ => unreachable!("invalid phase block keyword"),
        };

        self.builder.start_node(SyntaxKind::PHASE_BLOCK_STMT.into());

        self.expect(keyword_kind);
        self.skip_whitespace_and_newlines();

        if self.at(SyntaxKind::L_BRACE) {
            self.block();
        } else {
            self.error(&format!("Expected block after {name}"));
        }

        self.builder.finish_node();
    }

    fn package_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::PACKAGE_STMT.into());

        // "package"
        self.expect(SyntaxKind::PACKAGE_KW);
        self.skip_whitespace_and_newlines();

        // Package name (qualified identifier); allow keywords as identifiers
        self.parse_identifier_or_qualified();
        self.skip_whitespace_and_newlines();

        // After the package name, parse an optional version
        if self.at_any(&[
            SyntaxKind::VERSION,
            SyntaxKind::BARE_VERSION,
            SyntaxKind::NUMBER,
        ]) {
            self.bump();
            self.skip_whitespace_and_newlines();
        }

        // After the package name and optional version, allow either a terminating semicolon
        // or a block to introduce a scoped package
        if self.at(SyntaxKind::SEMICOLON) {
            self.bump();
        } else if self.at(SyntaxKind::L_BRACE) {
            // package Foo::Bar { ... }
            self.block();
        } else {
            // Neither a semicolon nor a block – report an error but continue
            self.error("Expected ';' or block after package declaration");
        }

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
        self.skip_whitespace_and_newlines();

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
            self.skip_whitespace_and_newlines();

            // Check for optional version after module name
            if self.at(SyntaxKind::VERSION)
                || self.at(SyntaxKind::BARE_VERSION)
                || self.at(SyntaxKind::NUMBER)
            {
                self.bump();
            }
        }
        self.skip_whitespace_and_newlines();

        // Option: import list (e.g., qw()) or comma-separated expressions (x => 1, y => 2)
        if self.is_at_start_of_expression() {
            // Parse first expression
            self.expression();

            // Handle additional comma-separated expressions
            while self.at(SyntaxKind::COMMA) {
                self.bump(); // consume comma
                self.skip_whitespace_and_newlines();

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

    fn for_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::FOR_STMT.into());
        self.bump_value(); // consume "for"
        self.skip_whitespace_and_newlines();

        if self.current_kind() == Some(SyntaxKind::L_PAREN) {
            self.bump_value(); // consume "("
            self.skip_whitespace_and_newlines();

            // Parse first expression
            if self.current_kind() != Some(SyntaxKind::SEMICOLON) {
                self.expression();
            }

            // Check if this is C-style (has semicolon)
            if self.current_kind() == Some(SyntaxKind::SEMICOLON) {
                // C-style for loop: for (init; condition; increment)
                self.bump_value(); // consume first semicolon
                self.skip_whitespace_and_newlines();

                // Parse condition (optional)
                if self.current_kind() != Some(SyntaxKind::SEMICOLON) {
                    self.expression();
                }

                self.skip_whitespace_and_newlines();
                // Expect second semicolon
                if self.current_kind() == Some(SyntaxKind::SEMICOLON) {
                    self.bump_value();
                }

                self.skip_whitespace_and_newlines();
                // Parse increment (optional)
                if self.current_kind() != Some(SyntaxKind::R_PAREN) {
                    self.expression();
                }
            }
            // else: Perl-style for loop with parentheses - expression already parsed

            self.skip_whitespace_and_newlines();
            // Expect closing parenthesis
            if self.current_kind() == Some(SyntaxKind::R_PAREN) {
                self.bump_value();
            }
        } else {
            // Perl-style for loop without parentheses: for VAR (LIST) BLOCK
            // Use existing parse_for_variable function for this case
            self.parse_for_variable();
            self.skip_whitespace_and_newlines();

            // List expression in parentheses: (LIST)
            if self.current_kind() == Some(SyntaxKind::L_PAREN) {
                self.bump_value(); // consume "("
                self.skip_whitespace_and_newlines();

                // Parse the list expression - can be multiple expressions separated by commas
                if !self.expression_list() {
                    self.error("Expected expression in for list");
                }

                self.skip_whitespace_and_newlines();
                if self.current_kind() == Some(SyntaxKind::R_PAREN) {
                    self.bump_value();
                }
            } else {
                self.error("Expected '(' after for variable");
            }
        }

        self.skip_whitespace_and_newlines();

        // Parse the body block
        self.block();

        self.builder.finish_node();
    }

    /// Parse the variable part of a for loop (my $var, $var)
    fn parse_for_variable(&mut self) {
        if matches!(
            self.current_kind(),
            Some(
                SyntaxKind::MY_KW
                    | SyntaxKind::OUR_KW
                    | SyntaxKind::STATE_KW
                    | SyntaxKind::LOCAL_KW
            )
        ) {
            // Variable declaration case - parse as a variable declaration
            self.builder.start_node(SyntaxKind::VAR_DECL.into());

            let decl_kind = self.current_kind().unwrap();
            self.bump_value(); // consume the keyword
            self.skip_whitespace_and_newlines();

            // Parse the variable - must be a scalar
            if self.current_kind() == Some(SyntaxKind::SCALAR_SIGIL) {
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
        } else if self.current_kind() == Some(SyntaxKind::SCALAR_SIGIL) {
            // $var case - parse as a variable reference
            self.parse_variable();
        } else {
            self.error("Expected scalar variable or 'my' declaration in for loop");
        }
    }

    fn while_stmt(&mut self) {
        self.parse_loop_statement(SyntaxKind::WHILE_STMT, SyntaxKind::WHILE_KW, "while");
    }

    fn until_stmt(&mut self) {
        self.parse_loop_statement(SyntaxKind::UNTIL_STMT, SyntaxKind::UNTIL_KW, "until");
    }

    /// Helper function to parse loop statements like while/until
    fn parse_loop_statement(
        &mut self,
        stmt_kind: SyntaxKind,
        kw_kind: SyntaxKind,
        construct_name: &str,
    ) {
        self.builder.start_node(stmt_kind.into());

        // "while" or "until"
        self.expect(kw_kind);
        self.skip_whitespace_and_newlines();

        // Parse parenthesized condition
        self.parse_parenthesized_condition(construct_name);

        self.skip_whitespace_and_newlines();

        // Block
        self.block();

        self.builder.finish_node();
    }

    fn if_stmt(&mut self) {
        self.parse_conditional_stmt(SyntaxKind::IF_STMT, SyntaxKind::IF_KW, "if");
    }

    /// Look ahead to see if there's an elsif or else keyword after whitespace
    fn lookahead_for_elsif_or_else(&self) -> bool {
        // Use token-based lookahead to check for elsif or else keywords
        self.lookahead_for_any(&[SyntaxKind::ELSIF_KW, SyntaxKind::ELSE_KW])
    }

    fn unless_stmt(&mut self) {
        self.parse_conditional_stmt(SyntaxKind::UNLESS_STMT, SyntaxKind::UNLESS_KW, "unless");
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
        self.parse_parenthesized_condition(keyword_name);

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

        while self.at(SyntaxKind::ELSIF_KW) {
            self.bump(); // elsif
            self.skip_whitespace_and_newlines();

            // Parse parenthesized condition
            self.parse_parenthesized_condition("elsif");

            self.skip_whitespace_and_newlines();

            self.block();

            // Skip trivia only if more elsif/else ahead
            if self.lookahead_for_elsif_or_else() {
                self.skip_whitespace_and_newlines();
            }
        }

        // "else"
        if self.at(SyntaxKind::ELSE_KW) {
            self.bump(); // else
            self.skip_whitespace_and_newlines();

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

        self.skip_whitespace_and_newlines();

        // Check for postfix modifiers (if/unless/for)
        self.parse_optional_postfix_modifier();

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
        self.skip_whitespace_and_newlines();

        while !self.at(SyntaxKind::R_BRACE) && !self.at_end() {
            if !self.statement() {
                self.error("Expected a statement in block, but found an unexpected token.");
            }
            self.skip_whitespace_and_newlines();
        }

        // After closing '}', expect an operator/statement boundary
        self.expect_op(SyntaxKind::R_BRACE);

        self.builder.finish_node();
    }

    fn ellipsis_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::ELLIPSIS_STMT.into());
        self.bump(); // consume '...'
        self.skip_whitespace_and_newlines();

        if self.at(SyntaxKind::SEMICOLON) {
            self.bump();
        } else if self.at_end()
            || self.at_any(&[SyntaxKind::R_BRACE, SyntaxKind::END_KW, SyntaxKind::DATA_KW])
        {
            // semicolon optional
        } else {
            self.error("Expected ';' after ellipsis statement");
        }

        self.builder.finish_node();
    }

    fn empty_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::EMPTY_STMT.into());
        self.bump(); // consume ';'
        self.builder.finish_node();
    }

    fn parse_optional_postfix_modifier(&mut self) {
        if self.at(SyntaxKind::IF_KW)
            || self.at(SyntaxKind::UNLESS_KW)
            || self.at(SyntaxKind::WHILE_KW)
            || self.at(SyntaxKind::UNTIL_KW)
        {
            self.parse_postfix_conditional();
        } else if self.at(SyntaxKind::FOR_KW) || self.at(SyntaxKind::FOREACH_KW) {
            self.parse_postfix_for();
        }
    }

    fn parse_postfix_conditional(&mut self) {
        let keyword_kind = self
            .current_kind()
            .expect("Current token should be if/unless/while/until keyword");
        let modifier_kind = match keyword_kind {
            SyntaxKind::IF_KW => SyntaxKind::IF_MODIFIER,
            SyntaxKind::UNLESS_KW => SyntaxKind::UNLESS_MODIFIER,
            SyntaxKind::WHILE_KW => SyntaxKind::WHILE_MODIFIER,
            SyntaxKind::UNTIL_KW => SyntaxKind::UNTIL_MODIFIER,
            _ => {
                self.error("Unexpected keyword in postfix conditional");
                return;
            }
        };

        self.builder.start_node(modifier_kind.into());

        // Consume the if/unless/while/until keyword; next should be a value (condition)
        self.bump_value();
        self.skip_whitespace_and_newlines();

        // Parse the condition expression
        if !self.expression() {
            self.error("Expected condition after postfix if/unless/while/until");
        }

        self.builder.finish_node();
    }

    fn parse_postfix_for(&mut self) {
        self.builder.start_node(SyntaxKind::FOR_MODIFIER.into());

        // Consume the for/foreach keyword; next should be a value (list expression)
        self.bump_value();
        self.skip_whitespace_and_newlines();

        if !self.expression_list() {
            self.error("Expected expression list after postfix for");
        }

        self.builder.finish_node();
    }

    pub(crate) fn parse_sub_tail(&mut self) {
        if self.at(SyntaxKind::L_PAREN) {
            self.parse_sub_prototype();
            self.skip_whitespace_and_newlines();
        }

        while self.at(SyntaxKind::COLON) {
            self.parse_sub_attribute();
            self.skip_whitespace_and_newlines();
        }

        self.skip_whitespace_and_newlines();

        if self.at(SyntaxKind::SEMICOLON) {
            self.bump();
        } else if self.at(SyntaxKind::L_BRACE) {
            self.block();
        } else {
            self.error("Expected block or ';' after subroutine declaration");
        }
    }

    pub(crate) fn parse_sub_attribute(&mut self) {
        self.builder.start_node(SyntaxKind::ATTR.into());

        self.expect(SyntaxKind::COLON);
        self.skip_whitespace_and_newlines();

        // Attribute name (qualified identifier allowed); keywords accepted as identifiers
        self.parse_identifier_or_qualified();
        self.skip_whitespace_and_newlines();

        if self.at(SyntaxKind::L_PAREN) {
            self.parse_attr_args();
        }

        self.builder.finish_node();
    }

    fn parse_attr_args(&mut self) {
        self.builder.start_node(SyntaxKind::ATTR_ARGS.into());

        self.expect(SyntaxKind::L_PAREN);
        self.skip_whitespace_and_newlines();

        if !self.at(SyntaxKind::R_PAREN) {
            if !self.expression_list() {
                self.error("Expected expression list in attribute arguments");
            }
            self.skip_whitespace_and_newlines();
        }

        self.expect_op(SyntaxKind::R_PAREN);

        self.builder.finish_node();
    }

    /// Helper function to parse parenthesized conditions for if/unless/while/until/elsif statements
    fn parse_parenthesized_condition(&mut self, construct_name: &str) {
        if self.at(SyntaxKind::L_PAREN) {
            // Inside condition parens, expect values
            self.bump_value(); // (
            self.skip_whitespace_and_newlines();

            // Parse the condition
            if !self.expression_list() {
                self.error(&format!(
                    "Expected expression in {construct_name} condition"
                ));
            }

            self.skip_whitespace_and_newlines();
            // After ')', expect operator/statement boundary
            self.expect_op(SyntaxKind::R_PAREN);
        } else {
            self.error(&format!("Expected '(' after '{construct_name}'"));
        }
    }

    /// Parse subroutine prototype like (\@@), ($@), (\@$@), etc.
    pub(crate) fn parse_sub_prototype(&mut self) {
        use crate::lexer::LexContext;
        self.builder.start_node(SyntaxKind::SUB_PROTOTYPE.into());

        self.expect(SyntaxKind::L_PAREN);
        self.skip_whitespace_and_newlines();

        while let Some((kind, _)) = self.peek_non_trivia_token_with_context(LexContext::Value) {
            if kind == SyntaxKind::R_PAREN {
                break;
            }
            match kind {
                SyntaxKind::BACKSLASH
                | SyntaxKind::ARRAY_SIGIL
                | SyntaxKind::HASH_SIGIL
                | SyntaxKind::SCALAR_SIGIL
                | SyntaxKind::CODE_SIGIL
                | SyntaxKind::TYPEGLOB_SIGIL
                | SyntaxKind::SEMICOLON
                | SyntaxKind::L_BRACKET
                | SyntaxKind::R_BRACKET
                | SyntaxKind::L_PAREN
                | SyntaxKind::R_PAREN => {
                    self.bump_with_context(LexContext::Value);
                    self.skip_whitespace_and_newlines();
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
    fn variable_declarations_allow_compound_assignment() {
        let inputs = [
            "my $x += 1;",
            "state $count ||= 0;",
            "our $total //= 0;",
            "say my $value += 2;",
            "my $x + 1;",
            "my $y // 'abc';",
            "my $z => {};",
        ];

        for input in inputs {
            let (_green, errors) = parse(input);
            assert!(
                errors.is_empty(),
                "Expected '{}' to parse without errors, got: {:?}",
                input,
                errors
            );
        }
    }

    #[test]
    fn lexical_sub_definitions_parse_without_errors() {
        let input = r#"
            my sub foo { 42 }
            state sub bar($) { $_[0] }
            our sub baz :method :Attr(1) { }
        "#;

        let (green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected lexical subroutines to parse without errors, got: {:?}",
            errors
        );

        let root = PerlNode::new_root(green);
        let sub_defs: Vec<_> = root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::SUB_DEF)
            .collect();

        assert_eq!(
            sub_defs.len(),
            3,
            "Expected three subroutine definitions, found {}",
            sub_defs.len()
        );
    }

    #[test]
    fn sub_forward_declarations_supported() {
        let input = r#"
            sub foo;
            sub bar($);
            sub baz :method;
            sub quux ($) :method :Attr(1);
        "#;

        let (green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected forward sub declarations to parse without errors, got: {:?}",
            errors
        );

        let root = PerlNode::new_root(green);
        let sub_defs: Vec<_> = root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::SUB_DEF)
            .collect();

        assert_eq!(
            sub_defs.len(),
            4,
            "Expected four subroutine declarations, found {}",
            sub_defs.len()
        );

        assert!(sub_defs.iter().all(|node| {
            !node
                .children()
                .any(|child| child.kind() == SyntaxKind::BLOCK_STMT)
        }));
    }

    #[test]
    fn anonymous_sub_expression_can_be_top_level_statement() {
        let input = "sub f { sub {} }";
        let (green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected anonymous sub expression to parse without errors, got: {:?}",
            errors
        );

        let root = PerlNode::new_root(green);
        assert_eq!(root.kind(), SyntaxKind::ROOT);

        let has_anon_sub = root
            .descendants()
            .any(|node| node.kind() == SyntaxKind::ANON_SUB_EXPR);
        assert!(
            has_anon_sub,
            "Parsed tree should contain an anonymous subexpression node"
        );
    }

    #[test]
    fn anonymous_sub_expression_can_have_prototype() {
        let input = "sub f { sub () { 0 } }";
        let (green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected anonymous sub expression with prototype to parse without errors, got: {:?}",
            errors
        );

        let root = PerlNode::new_root(green);
        let anon_sub = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::ANON_SUB_EXPR)
            .expect("Parsed tree should contain an anonymous subexpression node");

        assert!(
            anon_sub
                .children()
                .any(|child| child.kind() == SyntaxKind::SUB_PROTOTYPE),
            "Anonymous subexpression should include a prototype child"
        );
    }

    #[test]
    fn anonymous_sub_expression_can_have_attributes() {
        let input = "sub f { sub :method :foo(1) { 0 } }";
        let (green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected anonymous sub expression with attributes to parse without errors, got: {:?}",
            errors
        );

        let root = PerlNode::new_root(green);
        let anon_sub = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::ANON_SUB_EXPR)
            .expect("Parsed tree should contain an anonymous subexpression node");

        let attr_count = anon_sub
            .children()
            .filter(|child| child.kind() == SyntaxKind::ATTR)
            .count();
        assert!(
            attr_count >= 2,
            "Anonymous subexpression should include multiple attribute nodes"
        );

        assert!(
            anon_sub
                .descendants()
                .any(|node| node.kind() == SyntaxKind::ATTR_ARGS),
            "Anonymous subexpression should include attribute argument list"
        );
    }

    #[test]
    fn test_postfix_for_modifier() {
        let input = "print $_ for @values;";
        let (green, errors) = parse(input);
        assert!(
            errors.is_empty(),
            "Should parse postfix for modifier without errors, got: {:?}",
            errors
        );
        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
    }

    #[test]
    fn test_unless_with_elsif() {
        let input =
            "unless ($condition) { print 1; } elsif ($other) { print 2; } else { print 3; }";
        let (green, errors) = parse(input);
        assert!(
            errors.is_empty(),
            "Should parse unless with elsif without errors, got: {:?}",
            errors
        );
        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
    }

    #[test]
    fn if_condition_accepts_expression_list_with_trailing_comma() {
        let input = "if ($x,) { 1 }";
        let (green, errors) = parse(input);
        assert!(
            errors.is_empty(),
            "Should parse if statement with trailing comma in condition, got: {:?}",
            errors
        );

        let root = PerlNode::new_root(green);
        let if_stmt = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::IF_STMT)
            .expect("Parsed tree should contain an if statement");

        assert!(
            if_stmt
                .descendants()
                .any(|node| node.kind() == SyntaxKind::EXPR_LIST),
            "If condition should produce an expression list node"
        );
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
