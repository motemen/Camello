pub mod precedence;
pub mod primary;
pub mod quoted;

use crate::lexer::LexContext;
use crate::SyntaxKind;
use precedence::{get_operator_info, OperatorInfo, Precedence};

use super::Parser;

impl Parser<'_> {
    /// Decide whether the current quote-like keyword should be parsed as a quote-like expression
    /// or treated as an identifier. In the parser-driven quote-like mode, the lexer does not
    /// auto-expand to DELIMITER at lookahead time, so we conservatively treat it as quote-like
    /// unless the next token is a fat comma (=>), in which case it's likely a bareword key.
    fn should_parse_quote_like(&self) -> bool {
        self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1)
            .is_none_or(|(k, _)| k != SyntaxKind::FAT_COMMA)
    }

    /// Parse an identifier-like expression (including cases where a keyword is coerced to IDENT)
    /// and handle possible function calls (regular or block).
    fn parse_ident_like_expr(&mut self, coerce_current_to_ident: bool) {
        let start = self.builder.checkpoint();

        // Capture name before consuming
        let function_name = self.peek_block_function_basename().unwrap_or_default();

        if coerce_current_to_ident {
            self.bump_as(SyntaxKind::IDENT);
        } else {
            // Might be a qualified identifier, so use parse_identifier_or_qualified
            self.parse_identifier_or_qualified();
        }
        self.skip_whitespace_and_newlines();

        // Block-style function call: e.g., foo { ... } @list
        if self.at(SyntaxKind::L_BRACE)
            && (Self::is_block_function(&function_name)
                || Self::is_print_like_function(&function_name))
        {
            self.builder
                .start_node_at(start, SyntaxKind::BLOCK_FUNCTION_CALL_EXPR.into());
            self.parse_block_function_args(&function_name);
            self.builder.finish_node();
            return;
        }

        let mut next_kind = self.current_kind_value();
        if next_kind.is_none() {
            next_kind = self
                .peek_non_trivia_token_with_context(LexContext::Operator)
                .map(|(kind, _)| kind);
        }

        if let Some(kind) = next_kind {
            if kind == SyntaxKind::L_PAREN {
                // Parenthesized calls are handled by postfix parsing logic
                return;
            }
        }

        if Self::is_print_like_function(&function_name) {
            if self.is_at_start_of_expression() {
                self.builder
                    .start_node_at(start, SyntaxKind::FUNCTION_CALL_EXPR.into());
                self.parse_print_like_args();
                self.builder.finish_node();
            }
            return;
        }

        if let Some(kind) = next_kind {
            if Self::can_start_expression(kind) {
                // We have a regular function call, wrap everything in FUNCTION_CALL_EXPR
                self.builder
                    .start_node_at(start, SyntaxKind::FUNCTION_CALL_EXPR.into());
                self.expression_list();
                self.builder.finish_node();
            }
        }
    }
    // Parse block function arguments: block + optional additional arguments
    fn parse_block_function_args(&mut self, function_name: &str) {
        // Parse the block (which should be at L_BRACE)
        if self.at(SyntaxKind::L_BRACE) {
            self.builder.start_node(SyntaxKind::BLOCK_STMT.into());
            // Entering a block; next should expect a Value
            self.bump_value(); // {
            self.skip_whitespace_and_newlines();

            // Parse statements inside the block
            while !self.at(SyntaxKind::R_BRACE) && !self.at_end() {
                if !self.statement() {
                    // If we can't parse a statement, try to recover
                    self.error("Expected statement in block");
                    if self.current_kind().is_some() {
                        self.bump(); // Skip the problematic token
                    }
                }
                self.skip_whitespace_and_newlines();
            }

            self.expect(SyntaxKind::R_BRACE);
            self.builder.finish_node();
            self.skip_whitespace_and_newlines();
        }

        // Parse additional arguments if present (no comma before them)
        // For example: map { ... } @list
        if !Self::block_args_end_after_block(function_name) && self.is_at_start_of_expression() {
            self.expression_list();
        }
    }

    /// Determine whether a function name should be treated as accepting a leading block argument.
    ///
    /// We currently allow any function name (including qualified names) to take a block argument.
    /// This hook remains so future work can restore more selective behavior if desired.
    fn is_block_function(function_name: &str) -> bool {
        !function_name.is_empty()
    }

    /// Certain block-taking functions (`eval`, `do`) treat the block as their only argument.
    /// Stop parsing additional arguments after the first block for these names so operators like
    /// `//` are parsed in expression position instead of as another argument.
    fn block_args_end_after_block(function_name: &str) -> bool {
        matches!(function_name, "eval" | "do")
    }

    /// Peek ahead to capture the final segment of a (possibly qualified) identifier without
    /// consuming tokens. This is used to drive block-function heuristics before we parse the name.
    fn peek_block_function_basename(&self) -> Option<String> {
        let mut name = self.current_text_value()?.to_string();
        let mut offset = 1;

        while let Some((kind, _)) =
            self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset)
        {
            if kind != SyntaxKind::DOUBLE_COLON {
                break;
            }

            let Some((next_kind, next_text)) =
                self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset + 1)
            else {
                break;
            };

            if next_kind == SyntaxKind::IDENT || next_kind.is_keyword() {
                name = next_text.to_string();
                offset += 2;
                continue;
            }

            break;
        }

        Some(name)
    }

    fn parse_print_like_args(&mut self) {
        let mut consumed_filehandle = false;

        // Use lookahead to determine if this is a filehandle pattern:
        // Only treat IDENT/SCALAR as filehandle if followed by whitespace or end of statement
        // Otherwise treat as normal function call
        if self.at(SyntaxKind::IDENT) {
            // Check if this bareword should be treated as a filehandle
            if self.should_treat_as_filehandle() {
                self.bump_value();
                consumed_filehandle = true;
                self.skip_whitespace_and_newlines();
            }
        } else if self.at(SyntaxKind::DOLLAR) {
            // Check if this scalar should be treated as a filehandle
            if self.should_treat_scalar_as_filehandle() {
                self.parse_variable();
                consumed_filehandle = true;
            }
        }

        if consumed_filehandle && self.at_any(&[SyntaxKind::COMMA, SyntaxKind::FAT_COMMA]) {
            self.bump_value();
            self.skip_whitespace_and_newlines();
        }

        if self.is_at_start_of_expression() {
            self.expression_list();
        }
    }

    /// Check if a bareword (IDENT) should be treated as a filehandle.
    /// Only treat as filehandle if followed by whitespace or end of statement.
    fn should_treat_as_filehandle(&self) -> bool {
        // Look ahead to see what follows the IDENT. Use Operator context to help disambiguate.
        let next_token = self.peek_nth_non_trivia_token_with_context(LexContext::Operator, 1);

        match next_token {
            // If followed by parentheses or method/package separators, it's an expression
            Some((SyntaxKind::L_PAREN | SyntaxKind::DOUBLE_COLON | SyntaxKind::ARROW, _)) => false,
            // If followed by a likely binary operator, it's a function call in an expression
            Some((
                SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::ASTERISK
                | SyntaxKind::SLASH
                | SyntaxKind::PERCENT
                | SyntaxKind::CARET
                | SyntaxKind::AMPERSAND
                | SyntaxKind::BITWISE_OR
                | SyntaxKind::LT
                | SyntaxKind::GT
                | SyntaxKind::EQ
                | SyntaxKind::NE
                | SyntaxKind::LE
                | SyntaxKind::GE
                | SyntaxKind::STR_CMP
                | SyntaxKind::LOGICAL_AND
                | SyntaxKind::LOGICAL_OR
                | SyntaxKind::BITWISE_XOR,
                _,
            )) => false,
            // If followed by something that can start an expression, treat as filehandle
            Some((kind, _)) if Self::can_start_expression(kind) => true,
            // End of file or other contexts - treat as filehandle
            None => true,
            // Other tokens (comma, semicolon, etc.) - treat as filehandle
            _ => true,
        }
    }

    /// Check if a scalar variable should be treated as a filehandle.
    /// Only treat as filehandle if it's a simple variable followed by whitespace or end of statement.
    fn should_treat_scalar_as_filehandle(&self) -> bool {
        // Look ahead past the $IDENT to see what follows
        // First, check if we have $IDENT pattern
        if !self.at(SyntaxKind::DOLLAR) {
            return false;
        }

        let next_after_dollar = self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1);
        if !matches!(next_after_dollar, Some((SyntaxKind::IDENT, _))) {
            return false;
        }

        // Now check what follows the $IDENT pattern
        let token_after_var = self.peek_nth_non_trivia_token_with_context(LexContext::Operator, 2);

        match token_after_var {
            // If followed by postfix operations (arrow, brackets, etc.), it's not a simple filehandle
            Some((
                SyntaxKind::ARROW
                | SyntaxKind::L_BRACKET
                | SyntaxKind::L_BRACE
                | SyntaxKind::L_PAREN,
                _,
            )) => false,
            // If followed by a likely binary operator, it's an expression, not a filehandle
            Some((
                SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::ASTERISK
                | SyntaxKind::SLASH
                | SyntaxKind::PERCENT
                | SyntaxKind::CARET
                | SyntaxKind::AMPERSAND
                | SyntaxKind::BITWISE_OR
                | SyntaxKind::LT
                | SyntaxKind::GT
                | SyntaxKind::EQ
                | SyntaxKind::NE
                | SyntaxKind::LE
                | SyntaxKind::GE
                | SyntaxKind::STR_CMP
                | SyntaxKind::LOGICAL_AND
                | SyntaxKind::LOGICAL_OR
                | SyntaxKind::BITWISE_XOR,
                _,
            )) => false,
            // If followed by something that can start an expression or end of file, treat as filehandle
            Some((kind, _)) if Self::can_start_expression(kind) => true,
            // End of file or other contexts - treat as filehandle
            None => true,
            // Other tokens (operators, semicolon, etc.) - treat as filehandle
            _ => true,
        }
    }

    fn is_print_like_function(function_name: &str) -> bool {
        matches!(function_name, "print" | "printf" | "say")
    }

    pub fn expression(&mut self) -> bool {
        self.parse_expression_with_precedence(Precedence::LOWEST)
    }

    /// Core Pratt parser: parse expression with given minimum precedence
    pub fn parse_expression_with_precedence(&mut self, min_precedence: Precedence) -> bool {
        let checkpoint = self.builder.checkpoint();

        // Parse left-hand side (primary expression with postfix operations)
        if !self.parse_primary_with_postfix() {
            return false;
        }

        // Parse binary operators with precedence climbing
        loop {
            // Check if we have a binary operator or ternary operator
            let Some(current_kind) = self
                .peek_non_trivia_token_with_context(LexContext::Operator)
                .map(|(k, _)| k)
            else {
                break;
            };

            // Handle ternary operator specially
            if current_kind == SyntaxKind::QUESTION_MARK {
                let ternary_precedence = crate::parser::expression::precedence::Precedence::TERNARY;

                // If ternary precedence is too low, stop here
                if ternary_precedence < min_precedence {
                    break;
                }

                // Start building ternary expression node
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::TERNARY_EXPR.into());

                // Consume the ? operator as Operator; RHS will be Value
                self.bump_op();
                self.skip_whitespace_and_newlines();

                // Parse the true expression with ternary precedence (right associative)
                if !self.parse_expression_with_precedence(ternary_precedence) {
                    self.error("Expected expression after '?'");
                }

                // Look for the : operator - need to check both contexts
                self.skip_whitespace_and_newlines();
                let colon_found = self
                    .peek_non_trivia_token_with_context(LexContext::Operator)
                    .map(|(k, _)| k)
                    == Some(SyntaxKind::COLON)
                    || self.current_kind() == Some(SyntaxKind::COLON);

                if colon_found {
                    // Consume ':' as Operator; next will be Value
                    self.bump_op();
                    self.skip_whitespace_and_newlines();
                } else {
                    self.error("Expected ':' after true expression in ternary operator");
                }

                // Parse the false expression with ternary precedence (right associative)
                if !self.parse_expression_with_precedence(ternary_precedence) {
                    self.error("Expected expression after ':' in ternary operator");
                }

                self.builder.finish_node();
                continue;
            }

            // Check if this is a compound assignment operator (e.g., +=, ||=, etc.)
            let is_compound_assignment = current_kind.is_compoundable_operator() && {
                // Look ahead to see if there's an '=' after the current operator
                self.peek_nth_non_trivia_token_with_context(LexContext::Operator, 1)
                    .is_some_and(|(next_kind, _)| next_kind == SyntaxKind::EQ)
            };

            let op_info = if is_compound_assignment {
                // Use assignment precedence for compound assignment operators
                Some(OperatorInfo::new(
                    Precedence::ASSIGNMENT,
                    true, // Assignment is right associative
                    SyntaxKind::INFIX_EXPR,
                ))
            } else {
                get_operator_info(current_kind)
            };

            let Some(op_info) = op_info else {
                break;
            };

            // If precedence is too low, stop here
            if op_info.precedence < min_precedence {
                break;
            }

            // Start building binary expression node
            self.builder
                .start_node_at(checkpoint, op_info.node_kind.into());

            let op_checkpoint = self.builder.checkpoint();

            // Consume the operator in Operator context; RHS will be read as Value by default
            self.bump_op();

            if is_compound_assignment {
                // Handle compound assignment operators (e.g., +=, ||=, etc.)
                self.builder
                    .start_node_at(op_checkpoint, SyntaxKind::COMPOUND_ASSIGNMENT.into());
                // Consume '=' as an operator; RHS will be read as Value by default
                self.bump_op();
                self.builder.finish_node();
            }

            self.skip_whitespace_and_newlines();

            // Calculate next precedence level
            let next_min_precedence = if op_info.right_associative {
                op_info.precedence
            } else {
                Precedence(op_info.precedence.0 + 1)
            };

            // Parse right-hand side
            if !self.parse_expression_with_precedence(next_min_precedence) {
                self.error("Expected expression after binary operator");
            }

            self.builder.finish_node();
        }

        true
    }

    /// Parse primary expression with postfix operations
    fn parse_primary_with_postfix(&mut self) -> bool {
        let checkpoint = self.builder.checkpoint();

        if !self.primary_expr() {
            return false;
        }

        // Handle postfix operations
        self.parse_postfix_operations_with_checkpoint(checkpoint)
    }

    /// Parse a postfix increment or decrement operator
    fn parse_postfix_op(&mut self, initial_checkpoint: rowan::Checkpoint, op_kind: SyntaxKind) {
        self.builder
            .start_node_at(initial_checkpoint, SyntaxKind::POSTFIX_EXPR.into());
        self.bump_op_as(op_kind);
        self.skip_whitespace_and_newlines();
        self.builder.finish_node();
    }

    /// Parse all postfix operations (method calls, subscripts, etc.)
    fn parse_postfix_operations_with_checkpoint(
        &mut self,
        initial_checkpoint: rowan::Checkpoint,
    ) -> bool {
        loop {
            // Always look ahead in Operator context for postfix continuations
            let next_kind_op = self
                .peek_non_trivia_token_with_context(LexContext::Operator)
                .map(|(k, _)| k);

            match next_kind_op {
                Some(SyntaxKind::INCREMENT) => {
                    self.parse_postfix_op(initial_checkpoint, SyntaxKind::POSTFIX_INCREMENT);
                }
                Some(SyntaxKind::DECREMENT) => {
                    self.parse_postfix_op(initial_checkpoint, SyntaxKind::POSTFIX_DECREMENT);
                }
                Some(SyntaxKind::ARROW) => {
                    // After '->', the next token is a value (method name, '{', '(', etc.)
                    self.bump_value(); // ->
                    self.skip_whitespace_and_newlines();

                    match self.current_kind() {
                        Some(SyntaxKind::L_BRACE) => {
                            // Hash reference access: expr->{key}
                            self.builder.start_node_at(
                                initial_checkpoint,
                                SyntaxKind::HASH_REF_ACCESS_EXPR.into(),
                            );
                            // Opening '{' of ref access; inside expects a value
                            self.bump_value(); // {
                            self.skip_whitespace_and_newlines();

                            if !self.expression() {
                                self.error("Expected expression in hash reference access");
                            }

                            if self.at(SyntaxKind::R_BRACE) {
                                // After '}', expect an operator
                                self.bump_op(); // }
                                self.skip_whitespace_and_newlines();
                            } else {
                                self.error("Expected '}' after hash key");
                            }

                            self.builder.finish_node();
                        }
                        Some(SyntaxKind::L_BRACKET) => {
                            // Array reference access: expr->[index]
                            self.builder.start_node_at(
                                initial_checkpoint,
                                SyntaxKind::ARRAY_REF_ACCESS_EXPR.into(),
                            );
                            // Opening '[' of ref access; inside expects a value
                            self.bump_value(); // [
                            self.skip_whitespace_and_newlines();

                            if !self.expression() {
                                self.error("Expected expression in array reference access");
                            }

                            if self.at(SyntaxKind::R_BRACKET) {
                                // After ']', expect an operator
                                self.bump_op(); // ]
                                self.skip_whitespace_and_newlines();
                            } else {
                                self.error("Expected ']' after array index");
                            }

                            self.builder.finish_node();
                        }
                        Some(SyntaxKind::L_PAREN) => {
                            // Code reference call: expr->(args)
                            self.builder.start_node_at(
                                initial_checkpoint,
                                SyntaxKind::CODE_REF_CALL_EXPR.into(),
                            );
                            // Opening '(' of code ref call; inside expects value args
                            self.bump_value(); // (
                            self.skip_whitespace_and_newlines();

                            self.expression_list();

                            // Allow newlines or other trivia before closing ')'
                            self.skip_whitespace_and_newlines();

                            if self.at(SyntaxKind::R_PAREN) {
                                // After ')', expect an operator
                                self.bump_op(); // )
                                self.skip_whitespace_and_newlines();
                            } else {
                                self.error("Expected ')' after code reference arguments");
                            }

                            self.builder.finish_node();
                        }
                        Some(kind)
                            if kind == SyntaxKind::IDENT
                                || SyntaxKind::is_keyword(kind)
                                || matches!(
                                    kind,
                                    SyntaxKind::STR_EQ
                                        | SyntaxKind::STR_NE
                                        | SyntaxKind::STR_GT
                                        | SyntaxKind::STR_LT
                                        | SyntaxKind::STR_GE
                                        | SyntaxKind::STR_LE
                                        | SyntaxKind::STR_CMP
                                        | SyntaxKind::X
                                ) =>
                        {
                            // Method call: expr->method()
                            self.builder.start_node_at(
                                initial_checkpoint,
                                SyntaxKind::METHOD_CALL_EXPR.into(),
                            );

                            self.parse_identifier_or_qualified();
                            self.skip_whitespace_and_newlines();

                            self.parse_method_arguments();

                            self.builder.finish_node();
                        }
                        Some(kind) if kind.is_sigil() => {
                            // Dynamic method call: expr->$method()
                            self.builder.start_node_at(
                                initial_checkpoint,
                                SyntaxKind::METHOD_CALL_EXPR.into(),
                            );

                            self.parse_variable();
                            self.skip_whitespace_and_newlines();

                            self.parse_method_arguments();

                            self.builder.finish_node();
                        }
                        _ => {
                            self.error(
                                "Expected '{', '[', '(', identifier, or variable after '->'",
                            );
                            break;
                        }
                    }
                }
                Some(SyntaxKind::L_PAREN) => {
                    // Function call: expr(args)
                    self.builder
                        .start_node_at(initial_checkpoint, SyntaxKind::FUNCTION_CALL_EXPR.into());
                    // Inside function args, expect values
                    self.bump_value(); // (
                    self.skip_whitespace_and_newlines();

                    self.expression_list();

                    // Allow newlines or other trivia before closing ')'
                    self.skip_whitespace_and_newlines();

                    if self.at(SyntaxKind::R_PAREN) {
                        // After ')', expect an operator
                        self.bump_op(); // )
                        self.skip_whitespace_and_newlines();
                    } else {
                        self.error("Expected ')' after function arguments");
                    }

                    self.builder.finish_node();
                }
                Some(SyntaxKind::L_BRACKET) => {
                    // Direct array subscription: expr[index]
                    self.builder.start_node_at(
                        initial_checkpoint,
                        SyntaxKind::ARRAY_SUBSCRIPTION_EXPR.into(),
                    );
                    self.bump(); // [
                    self.skip_whitespace_and_newlines();

                    if !self.expression() {
                        self.error("Expected expression in array subscription");
                    }

                    if self.at(SyntaxKind::R_BRACKET) {
                        self.bump(); // ]
                        self.skip_whitespace_and_newlines();
                    } else {
                        self.error("Expected ']' after array index");
                    }

                    self.builder.finish_node();
                }
                Some(SyntaxKind::L_BRACE) => {
                    // Direct hash subscription: expr{key}
                    self.builder.start_node_at(
                        initial_checkpoint,
                        SyntaxKind::HASH_SUBSCRIPTION_EXPR.into(),
                    );
                    self.bump(); // {
                    self.skip_whitespace_and_newlines();

                    if !self.expression() {
                        self.error("Expected expression in hash subscription");
                    }

                    if self.at(SyntaxKind::R_BRACE) {
                        self.bump(); // }
                        self.skip_whitespace_and_newlines();
                    } else {
                        self.error("Expected '}' after hash key");
                    }

                    self.builder.finish_node();
                }
                Some(
                    SyntaxKind::POSTFIX_DEREF_ARRAY
                    | SyntaxKind::POSTFIX_DEREF_HASH
                    | SyntaxKind::POSTFIX_DEREF_SCALAR,
                ) => {
                    // Postfix dereference: expr->@*, expr->%*, expr->$*
                    self.builder
                        .start_node_at(initial_checkpoint, SyntaxKind::POSTFIX_DEREF_EXPR.into());
                    // Postfix deref is a value-ending token; expect operator next
                    self.bump_op(); // ->@*, ->%*, or ->$*
                    self.skip_whitespace_and_newlines();
                    self.builder.finish_node();
                }
                _ => {
                    // No more postfix operations
                    break;
                }
            }
        }
        true
    }

    pub fn expression_list(&mut self) -> bool {
        let start = self.builder.checkpoint();
        // Parse the first expression but stop before comma-level operators like =>
        if !self.parse_expression_with_precedence(Precedence::LIST_ITEM) {
            return false;
        }

        // If we have comma-separated expressions, wrap them in a single EXPR_LIST node
        if self.at_any(&[SyntaxKind::COMMA, SyntaxKind::FAT_COMMA]) {
            self.builder
                .start_node_at(start, SyntaxKind::EXPR_LIST.into());

            while self.at_any(&[SyntaxKind::COMMA, SyntaxKind::FAT_COMMA]) {
                // After a separator, next should be a value
                self.bump_value(); // , or =>
                self.skip_whitespace_and_newlines();

                // Check for trailing comma - if we're at the end of a list context, don't require another expression
                if self.is_at_start_of_expression()
                    && !self.parse_expression_with_precedence(Precedence::LIST_ITEM)
                {
                    self.error("Expected expression after comma in list");
                }
                // If no expression follows, it's a trailing comma - that's OK
            }

            self.builder.finish_node();
        }

        true
    }

    fn primary_expr(&mut self) -> bool {
        self.skip_whitespace_and_newlines();

        let Some(current_kind) = self.current_kind_value() else {
            return false;
        };

        // Treat bare keywords as identifiers when they appear before fat comma (=>)
        // or when they are inside hash braces (for hash keys like $h->{package})
        if current_kind.is_keyword()
            && (self
                .peek_nth_non_trivia_token_with_context(LexContext::Value, 1)
                .is_some_and(|(next_kind, _)| next_kind == SyntaxKind::FAT_COMMA)
                || self.is_inside_hash_braces())
        {
            self.parse_ident_like_expr(true);
            return true;
        }

        match current_kind {
            SyntaxKind::NUMBER | SyntaxKind::STRING | SyntaxKind::REGEX_LITERAL => {
                // Consume as a value; let operators be detected on the next step
                self.bump_value();
                self.skip_whitespace_and_newlines();
            }
            SyntaxKind::BACKTICK_STRING => {
                // Backtick command substitution: `command`
                self.builder.start_node(SyntaxKind::BACKTICK_EXPR.into());
                self.bump_value();
                self.builder.finish_node();
                self.skip_whitespace_and_newlines();
            }
            SyntaxKind::IO_EXPR => {
                self.builder.start_node(SyntaxKind::IO_EXPR.into());
                // Consume I/O expression as a value
                self.bump_value();
                self.builder.finish_node();
                self.skip_whitespace_and_newlines();
            }
            SyntaxKind::HEREDOC_START => {
                self.bump_value();
                self.skip_whitespace_and_newlines();
            }
            kind if kind.is_variable() => {
                // Consume variable as a value
                self.bump_value();
                self.skip_whitespace_and_newlines();
            }
            SyntaxKind::BACKSLASH => {
                // Reference operator as prefix: \expr
                self.parse_standard_prefix_expr("\\", Precedence::PREFIX, None);
            }
            SyntaxKind::AMPERSAND => {
                // Function reference: &function
                self.parse_function_ref();
            }
            SyntaxKind::ASTERISK => {
                // Handle typeglob expressions specially
                // Check if this is followed by a brace or identifier (typeglob syntax)
                let next_token = self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1);
                if matches!(
                    next_token,
                    Some((SyntaxKind::L_BRACE | SyntaxKind::IDENT, _))
                ) {
                    self.parse_typeglob_expr();
                } else {
                    self.parse_variable();
                }
            }
            kind if kind.is_sigil() => {
                // All sigil-based variables are now handled by parse_variable
                self.parse_variable();
            }
            SyntaxKind::PLUS => {
                // Unary plus prefix operator
                self.parse_standard_prefix_expr(
                    "+",
                    Precedence::PREFIX,
                    Some(SyntaxKind::UNARY_PLUS),
                );
            }
            SyntaxKind::MINUS => {
                // Unary minus prefix operator
                self.parse_standard_prefix_expr(
                    "-",
                    Precedence::PREFIX,
                    Some(SyntaxKind::UNARY_MINUS),
                );
            }
            SyntaxKind::INCREMENT => {
                // Prefix increment operator
                self.parse_standard_prefix_expr(
                    "++",
                    Precedence::PREFIX,
                    Some(SyntaxKind::PREFIX_INCREMENT),
                );
            }
            SyntaxKind::DECREMENT => {
                // Prefix decrement operator
                self.parse_standard_prefix_expr(
                    "--",
                    Precedence::PREFIX,
                    Some(SyntaxKind::PREFIX_DECREMENT),
                );
            }
            SyntaxKind::LOGICAL_NOT => {
                // Logical NOT prefix operator
                self.parse_standard_prefix_expr("!", Precedence::PREFIX, None);
            }
            SyntaxKind::BITWISE_NOT => {
                // Bitwise NOT prefix operator
                self.parse_standard_prefix_expr("~", Precedence::PREFIX, None);
            }
            SyntaxKind::NOT_KW => {
                // NOT keyword prefix operator
                self.parse_standard_prefix_expr("not", Precedence::LOGICAL_NOT_KW, None);
            }
            SyntaxKind::MY_KW
            | SyntaxKind::OUR_KW
            | SyntaxKind::STATE_KW
            | SyntaxKind::LOCAL_KW => {
                // Variable declaration as expression (e.g., my $x = 1)
                self.var_decl_expr();
            }
            SyntaxKind::UNDEF_KW => {
                // undef can be used both as a literal and as a function call
                // Check if it's followed by an expression (function call) or not (literal)
                let next_token = self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1);
                if let Some((kind, _)) = next_token {
                    if Self::can_start_expression(kind) {
                        // This is a function call: undef $x
                        self.parse_ident_like_expr(true);
                    } else {
                        // This is a literal: undef by itself
                        self.bump_value();
                        self.skip_whitespace_and_newlines();
                    }
                } else {
                    // No next token, treat as literal
                    self.bump_value();
                    self.skip_whitespace_and_newlines();
                }
            }
            SyntaxKind::REQUIRE_KW => {
                // require expression (e.g., require v5.14, require local::lib)
                self.require_expr();
            }
            SyntaxKind::IDENT => {
                self.parse_ident_like_expr(false);
            }
            SyntaxKind::X => {
                // Handle 'x' as an identifier when it appears at the start of expressions
                // This allows expressions like "x => 1" in use statements
                // Consume 'x' as a value in this context
                self.bump_value();
                self.skip_whitespace_and_newlines();
            }
            SyntaxKind::L_PAREN => {
                // Parenthesized expression
                // Inside parens, expect a value
                self.bump_value(); // (
                self.skip_whitespace_and_newlines();

                // List inside parentheses (e.g., array initialization)
                self.parse_parenthesized_list();

                if self.at(SyntaxKind::R_PAREN) {
                    // After ')', expect an operator
                    self.bump_op(); // )
                    self.skip_whitespace_and_newlines();
                }
            }
            SyntaxKind::L_BRACE => {
                // Hash reference (anonymous hash): {}
                self.hash_ref();
            }
            SyntaxKind::L_BRACKET => {
                // Array reference (anonymous array): []
                self.array_ref();
            }
            SyntaxKind::QW_KW => {
                // qw() expression or bareword 'qw'
                if self.should_parse_quote_like() {
                    self.qw_expr();
                } else {
                    self.parse_ident_like_expr(true);
                }
            }
            SyntaxKind::RETURN_KW => {
                // return statement (handled as a keyword)
                // After 'return', if an expression follows, it is a value
                self.bump_value(); // consume return
                self.skip_whitespace_and_newlines();

                // If there is an expression after return, process it
                if self.is_at_start_of_expression() {
                    self.expression_list();
                }
            }
            SyntaxKind::NEXT_KW | SyntaxKind::LAST_KW | SyntaxKind::REDO_KW => {
                // loop control statements with optional label
                self.bump_value(); // consume keyword
                self.skip_whitespace_and_newlines();

                // Optional label
                if self.at(SyntaxKind::IDENT) {
                    self.bump_value();
                    self.skip_whitespace_and_newlines();
                }
            }
            SyntaxKind::Q_KW => {
                if self.should_parse_quote_like() {
                    self.q_expr();
                } else {
                    self.parse_ident_like_expr(true);
                }
            }
            SyntaxKind::QQ_KW => {
                if self.should_parse_quote_like() {
                    self.qq_expr();
                } else {
                    self.parse_ident_like_expr(true);
                }
            }
            SyntaxKind::QX_KW => {
                if self.should_parse_quote_like() {
                    self.qx_expr();
                } else {
                    self.parse_ident_like_expr(true);
                }
            }
            SyntaxKind::M_KW => {
                if self.should_parse_quote_like() {
                    self.m_expr();
                } else {
                    self.parse_ident_like_expr(true);
                }
            }
            SyntaxKind::QR_KW => {
                if self.should_parse_quote_like() {
                    self.qr_expr();
                } else {
                    self.parse_ident_like_expr(true);
                }
            }
            SyntaxKind::S_KW => {
                if self.should_parse_quote_like() {
                    self.s_expr();
                } else {
                    self.parse_ident_like_expr(true);
                }
            }
            SyntaxKind::TR_KW => {
                if self.should_parse_quote_like() {
                    self.tr_expr();
                } else {
                    self.parse_ident_like_expr(true);
                }
            }
            SyntaxKind::Y_KW => {
                if self.should_parse_quote_like() {
                    self.y_expr();
                } else {
                    self.parse_ident_like_expr(true);
                }
            }
            SyntaxKind::SUB_KW => {
                // Anonymous subroutine expression: sub { ... }
                self.anon_sub_expr();
            }
            SyntaxKind::FILE_TEST_OP => {
                self.builder.start_node(SyntaxKind::FILE_TEST_EXPR.into());
                // File test operator is prefix; argument is optional
                self.bump_value(); // consume file test operator
                self.skip_whitespace_and_newlines();

                // Try to parse an expression argument, but don't require it
                // File test operators like -f can be used without arguments (they operate on $_)
                self.parse_expression_with_precedence(
                    crate::parser::expression::precedence::Precedence::PREFIX,
                );

                self.builder.finish_node();
            }
            _ => {
                // Should not reach here because is_at_start_of_expression checks this
                return false;
            }
        }
        true
    }

    /// Parse anonymous subroutine expression: sub { ... }
    fn anon_sub_expr(&mut self) {
        self.builder.start_node(SyntaxKind::ANON_SUB_EXPR.into());

        // Consume 'sub' keyword
        self.expect(SyntaxKind::SUB_KW);
        self.skip_whitespace_and_newlines();

        // Parse the block
        self.block();

        self.builder.finish_node();
    }

    fn require_expr(&mut self) {
        self.builder.start_node(SyntaxKind::REQUIRE_EXPR.into());

        // "require"
        self.expect(SyntaxKind::REQUIRE_KW);
        self.skip_whitespace_and_newlines();

        // VERSION literal or module name (qualified identifier)
        if self.at(SyntaxKind::VERSION) {
            // Version literal (e.g., require v5.42)
            self.bump();
        } else if self.at(SyntaxKind::BARE_VERSION) {
            // Bare version literal (e.g., require 5.24.1)
            self.bump();
        } else if self.at(SyntaxKind::NUMBER) {
            // Simple version number (e.g., require 5)
            self.bump();
        } else {
            // Module name (qualified identifier); allow keywords as identifiers
            self.parse_identifier_or_qualified();
        }
        self.skip_whitespace_and_newlines();

        // Option: import list (e.g., qw()) or comma-separated expressions (x => 1, y => 2)
        // Note: Unlike require statement, we don't consume additional expressions here
        // as they would be handled by the expression parser at a higher level

        self.builder.finish_node();
    }

    /// Parse method arguments if parentheses are present
    fn parse_method_arguments(&mut self) {
        if self.at(SyntaxKind::L_PAREN) {
            // Inside method args, expect values
            self.bump_value(); // (
            self.skip_whitespace_and_newlines();

            self.expression_list();

            // Allow newlines or other trivia before closing ')'
            self.skip_whitespace_and_newlines();

            if self.at(SyntaxKind::R_PAREN) {
                // After ')', expect an operator
                self.bump_op(); // )
                self.skip_whitespace_and_newlines();
            } else {
                self.error("Expected ')' after method arguments");
            }
        }
    }

    /// Helper function to parse a standard prefix expression, reducing code duplication
    fn parse_standard_prefix_expr(
        &mut self,
        op_char: &str,
        precedence: Precedence,
        use_bump_as: Option<SyntaxKind>,
    ) {
        self.builder.start_node(SyntaxKind::PREFIX_EXPR.into());

        if let Some(as_kind) = use_bump_as {
            self.bump_as(as_kind);
        } else {
            self.bump_value(); // consume operator
        }

        self.skip_whitespace_and_newlines();

        if !self.parse_expression_with_precedence(precedence) {
            let message = format!("Expected expression after '{}'", op_char);
            self.error(&message);
        }

        self.builder.finish_node();
    }

    /// Parse function reference: &function
    fn parse_function_ref(&mut self) {
        self.builder.start_node(SyntaxKind::FUNCTION_REF.into());

        // Consume the &
        self.bump();
        self.skip_whitespace_and_newlines();

        // Parse the function name (identifier or qualified identifier)
        self.parse_identifier_or_qualified();

        self.builder.finish_node();
    }
}
