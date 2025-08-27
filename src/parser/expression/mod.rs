pub mod precedence;
pub mod primary;
pub mod quoted;

use crate::SyntaxKind;
use precedence::{get_operator_info, Precedence};

use super::Parser;

impl<'a> Parser<'a> {
    // Parse block function arguments: block + optional additional arguments
    fn parse_block_function_args(&mut self) {
        // Parse the block (which should be at L_BRACE)
        if self.at(SyntaxKind::L_BRACE) {
            self.builder.start_node(SyntaxKind::BLOCK_STMT.into());
            self.bump(); // {
            self.skip_trivia();

            // Parse statements inside the block
            while !self.at(SyntaxKind::R_BRACE) && !self.at_end() {
                if !self.statement() {
                    // If we can't parse a statement, try to recover
                    self.error("Expected statement in block");
                    if self.current_kind().is_some() {
                        self.bump(); // Skip the problematic token
                    }
                }
                self.skip_trivia();
            }

            self.expect(SyntaxKind::R_BRACE);
            self.builder.finish_node();
            self.skip_trivia();
        }

        // Parse additional arguments if present (no comma before them)
        // For example: map { ... } @list
        if self.is_at_start_of_expression() {
            self.expression_list();
        }
    }

    fn is_block_function(function_name: &str) -> bool {
        matches!(function_name, "eval" | "map" | "grep" | "sort" | "do")
    }

    pub fn expression(&mut self) -> bool {
        self.parse_expression_with_precedence(Precedence::LOWEST)
    }

    /// Core Pratt parser: parse expression with given minimum precedence
    pub fn parse_expression_with_precedence(&mut self, min_precedence: Precedence) -> bool {
        // Parse left-hand side (primary expression with postfix operations)
        if !self.parse_primary_with_postfix() {
            return false;
        }

        // Parse binary operators with precedence climbing
        loop {
            // Check if we have a binary operator or ternary operator
            let Some(current_kind) = self.current_kind() else {
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
                let checkpoint = self.builder.checkpoint();
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::TERNARY_EXPR.into());

                // Consume the ? operator
                self.bump();
                self.skip_trivia();

                // Parse the true expression with ternary precedence (right associative)
                if !self.parse_expression_with_precedence(ternary_precedence) {
                    self.error("Expected expression after '?'");
                }

                // Expect the : operator
                if !self.at(SyntaxKind::COLON) {
                    self.error("Expected ':' after true expression in ternary operator");
                } else {
                    self.bump(); // consume :
                    self.skip_trivia();
                }

                // Parse the false expression with ternary precedence (right associative)
                if !self.parse_expression_with_precedence(ternary_precedence) {
                    self.error("Expected expression after ':' in ternary operator");
                }

                self.builder.finish_node();
                continue;
            }

            let Some(op_info) = get_operator_info(current_kind) else {
                break;
            };

            // If precedence is too low, stop here
            if op_info.precedence < min_precedence {
                break;
            }

            // Start building binary expression node
            let checkpoint = self.builder.checkpoint();
            self.builder
                .start_node_at(checkpoint, op_info.node_kind.into());

            // Consume the operator
            self.bump();
            self.skip_trivia();

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
        if !self.primary_expr() {
            return false;
        }

        // Handle postfix operations
        self.parse_postfix_operations()
    }

    /// Parse all postfix operations (method calls, subscripts, etc.)
    fn parse_postfix_operations(&mut self) -> bool {
        loop {
            let checkpoint = self.builder.checkpoint();

            match self.current_kind() {
                Some(SyntaxKind::ARROW) => {
                    self.bump(); // ->
                    self.skip_trivia();

                    match self.current_kind() {
                        Some(SyntaxKind::L_BRACE) => {
                            // Hash reference access: expr->{key}
                            self.builder
                                .start_node_at(checkpoint, SyntaxKind::HASH_REF_ACCESS_EXPR.into());
                            self.bump(); // {
                            self.skip_trivia();

                            if !self.expression() {
                                self.error("Expected expression in hash reference access");
                            }

                            if !self.at(SyntaxKind::R_BRACE) {
                                self.error("Expected '}' after hash key");
                            } else {
                                self.bump(); // }
                                self.skip_trivia();
                            }

                            self.builder.finish_node();
                        }
                        Some(SyntaxKind::L_BRACKET) => {
                            // Array reference access: expr->[index]
                            self.builder.start_node_at(
                                checkpoint,
                                SyntaxKind::ARRAY_REF_ACCESS_EXPR.into(),
                            );
                            self.bump(); // [
                            self.skip_trivia();

                            if !self.expression() {
                                self.error("Expected expression in array reference access");
                            }

                            if !self.at(SyntaxKind::R_BRACKET) {
                                self.error("Expected ']' after array index");
                            } else {
                                self.bump(); // ]
                                self.skip_trivia();
                            }

                            self.builder.finish_node();
                        }
                        Some(SyntaxKind::L_PAREN) => {
                            // Code reference call: expr->(args)
                            self.builder
                                .start_node_at(checkpoint, SyntaxKind::CODE_REF_CALL_EXPR.into());
                            self.bump(); // (
                            self.skip_trivia();

                            self.expression_list();

                            if !self.at(SyntaxKind::R_PAREN) {
                                self.error("Expected ')' after code reference arguments");
                            } else {
                                self.bump(); // )
                                self.skip_trivia();
                            }

                            self.builder.finish_node();
                        }
                        Some(SyntaxKind::IDENT) => {
                            // Method call: expr->method()
                            self.builder
                                .start_node_at(checkpoint, SyntaxKind::METHOD_CALL_EXPR.into());

                            self.parse_identifier_or_qualified();
                            self.skip_trivia();

                            if self.at(SyntaxKind::L_PAREN) {
                                self.bump(); // (
                                self.skip_trivia();

                                self.expression_list();

                                if !self.at(SyntaxKind::R_PAREN) {
                                    self.error("Expected ')' after method arguments");
                                } else {
                                    self.bump(); // )
                                    self.skip_trivia();
                                }
                            }

                            self.builder.finish_node();
                        }
                        _ => {
                            self.error("Expected '{', '[', '(' or identifier after '->'");
                            break;
                        }
                    }
                }
                Some(SyntaxKind::L_PAREN) => {
                    // Function call: expr(args)
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::FUNCTION_CALL_EXPR.into());
                    self.bump(); // (
                    self.skip_trivia();

                    self.expression_list();

                    if !self.at(SyntaxKind::R_PAREN) {
                        self.error("Expected ')' after function arguments");
                    } else {
                        self.bump(); // )
                        self.skip_trivia();
                    }

                    self.builder.finish_node();
                }
                Some(SyntaxKind::L_BRACKET) => {
                    // Direct array subscription: expr[index]
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::ARRAY_SUBSCRIPTION_EXPR.into());
                    self.bump(); // [
                    self.skip_trivia();

                    if !self.expression() {
                        self.error("Expected expression in array subscription");
                    }

                    if !self.at(SyntaxKind::R_BRACKET) {
                        self.error("Expected ']' after array index");
                    } else {
                        self.bump(); // ]
                        self.skip_trivia();
                    }

                    self.builder.finish_node();
                }
                Some(SyntaxKind::L_BRACE) => {
                    // Direct hash subscription: expr{key}
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::HASH_SUBSCRIPTION_EXPR.into());
                    self.bump(); // {
                    self.skip_trivia();

                    if !self.expression() {
                        self.error("Expected expression in hash subscription");
                    }

                    if !self.at(SyntaxKind::R_BRACE) {
                        self.error("Expected '}' after hash key");
                    } else {
                        self.bump(); // }
                        self.skip_trivia();
                    }

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
        if !self.expression() {
            return false;
        }

        // If we have comma-separated expressions, wrap them in a single EXPR_LIST node
        if self.at_any(&[SyntaxKind::COMMA, SyntaxKind::FAT_COMMA]) {
            self.builder
                .start_node_at(start, SyntaxKind::EXPR_LIST.into());

            while self.at_any(&[SyntaxKind::COMMA, SyntaxKind::FAT_COMMA]) {
                self.bump(); // , or =>
                self.skip_trivia();

                // Check for trailing comma - if we're at the end of a list context, don't require another expression
                if self.is_at_start_of_expression() && !self.expression() {
                    self.error("Expected expression after comma in list");
                }
                // If no expression follows, it's a trailing comma - that's OK
            }

            self.builder.finish_node();
        }

        true
    }

    fn primary_expr(&mut self) -> bool {
        self.skip_trivia();

        let at_start = self.is_at_start_of_expression();
        if !at_start {
            return false;
        }

        match self.current_kind() {
            Some(SyntaxKind::NUMBER)
            | Some(SyntaxKind::STRING)
            | Some(SyntaxKind::REGEX_LITERAL) => {
                self.bump();
                self.skip_trivia();
            }
            Some(kind) if kind.is_variable() => {
                self.bump();
                self.skip_trivia();
            }
            Some(SyntaxKind::BACKSLASH) => {
                // Reference expression: \$scalar, \@array, \%hash, \&code
                self.parse_reference_expr();
            }
            Some(kind) if kind.is_sigil() => {
                // Check if this is a dereferencing pattern (sigil followed by another sigil)
                if self.is_dereferencing_pattern() {
                    self.parse_dereferencing();
                } else {
                    self.parse_variable();
                }
            }
            Some(SyntaxKind::LOGICAL_NOT) => {
                // Logical NOT prefix operator
                self.builder.start_node(SyntaxKind::PREFIX_EXPR.into());
                self.bump(); // consume !
                self.skip_trivia();

                // Parse the operand with higher precedence
                if !self.parse_expression_with_precedence(
                    crate::parser::expression::precedence::Precedence::PREFIX,
                ) {
                    self.error("Expected expression after '!'");
                }

                self.builder.finish_node();
            }
            Some(SyntaxKind::NOT_KW) => {
                // NOT keyword prefix operator
                self.builder.start_node(SyntaxKind::PREFIX_EXPR.into());
                self.bump(); // consume 'not'
                self.skip_trivia();

                // Parse the operand with logical not keyword precedence
                if !self.parse_expression_with_precedence(
                    crate::parser::expression::precedence::Precedence::LOGICAL_NOT_KW,
                ) {
                    self.error("Expected expression after 'not'");
                }

                self.builder.finish_node();
            }
            Some(
                SyntaxKind::MY_KW
                | SyntaxKind::OUR_KW
                | SyntaxKind::STATE_KW
                | SyntaxKind::LOCAL_KW,
            ) => {
                // Variable declaration as expression (e.g., my $x = 1)
                self.var_decl_expr();
            }
            Some(SyntaxKind::IDENT) => {
                let start = self.builder.checkpoint();

                // Get the function name before parsing
                let function_name = self.current_text().unwrap_or("").to_string();

                // Might be a qualified identifier, so use parse_identifier_or_qualified
                self.parse_identifier_or_qualified();
                self.skip_trivia();

                // Check for block functions first
                if Self::is_block_function(&function_name) && self.at(SyntaxKind::L_BRACE) {
                    // This is a block function call
                    self.builder
                        .start_node_at(start, SyntaxKind::BLOCK_FUNCTION_CALL_EXPR.into());

                    self.parse_block_function_args();

                    self.builder.finish_node();
                } else if let Some(kind) = self.current_kind() {
                    // Check if we have regular function arguments following the identifier
                    // Value-like objects
                    if kind.is_variable()
                        || self.at_any(&[
                            SyntaxKind::NUMBER,
                            SyntaxKind::STRING,
                            SyntaxKind::L_BRACE,   // Hash reference: {}
                            SyntaxKind::L_BRACKET, // Array reference: []
                            SyntaxKind::MY_KW,     // Variable declarations as arguments
                            SyntaxKind::OUR_KW,
                            SyntaxKind::STATE_KW,
                            SyntaxKind::LOCAL_KW,
                        ])
                        || kind.is_sigil()
                    {
                        // We have a regular function call, wrap everything in FUNCTION_CALL_EXPR
                        self.builder
                            .start_node_at(start, SyntaxKind::FUNCTION_CALL_EXPR.into());

                        // Parse arguments as an expression list
                        self.expression_list();

                        self.builder.finish_node();
                    } else if kind == SyntaxKind::IDENT {
                        // Might be a nested function call eg. `foo bar(1)`
                        self.builder
                            .start_node_at(start, SyntaxKind::FUNCTION_CALL_EXPR.into());

                        // Parse arguments as an expression list
                        self.expression_list();

                        self.builder.finish_node();
                    }
                }
            }
            Some(SyntaxKind::L_PAREN) => {
                // Parenthesized expression
                self.bump(); // (
                self.skip_trivia();

                // List inside parentheses (e.g., array initialization)
                self.parse_parenthesized_list();

                if self.at(SyntaxKind::R_PAREN) {
                    self.bump(); // )
                    self.skip_trivia();
                }
            }
            Some(SyntaxKind::L_BRACE) => {
                // Hash reference (anonymous hash): {}
                self.hash_ref();
            }
            Some(SyntaxKind::L_BRACKET) => {
                // Array reference (anonymous array): []
                self.array_ref();
            }
            Some(SyntaxKind::QW_KW) => {
                // qw() expression
                self.qw_expr();
            }
            Some(SyntaxKind::RETURN_KW) => {
                // return statement (handled as a keyword)
                self.bump(); // consume return
                self.skip_trivia();

                // If there is an expression after return, process it
                if self.is_at_start_of_expression() {
                    self.expression();
                }
            }
            Some(SyntaxKind::Q_KW) => {
                // q() expression
                self.q_expr();
            }
            Some(SyntaxKind::QQ_KW) => {
                // qq() expression
                self.qq_expr();
            }
            Some(SyntaxKind::QX_KW) => {
                // qx() expression
                self.qx_expr();
            }
            Some(SyntaxKind::M_KW) => {
                // m() expression
                self.m_expr();
            }
            Some(SyntaxKind::QR_KW) => {
                // qr() expression
                self.qr_expr();
            }
            Some(SyntaxKind::S_KW) => {
                // s() expression
                self.s_expr();
            }
            _ => {
                // Should not reach here because is_at_start_of_expression checks this
                return false;
            }
        }
        true
    }
}
