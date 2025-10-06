mod call;
mod postfix;
pub mod precedence;
pub mod primary;
pub mod quoted;

use crate::lexer::LexContext;
use crate::{SyntaxKind, T};
use precedence::{get_operator_info, OperatorInfo, Precedence};

use super::Parser;

/// Result of parsing a primary expression, indicating subscript eligibility
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PostfixSubject {
    /// No primary expression was parsed
    None,
    /// Variable: allows both [] and {} direct subscripts
    Variable,
    /// Parenthesized list: allows [] subscript only, {} requires ->
    List,
    /// Other expressions: both [] and {} require ->
    Other,
}

fn is_empty_regex(token: Option<(SyntaxKind, &str)>) -> bool {
    matches!(token, Some((SyntaxKind::REGEX_LITERAL, text)) if text == "//")
}

impl Parser<'_> {
    /// Decide whether the current quote-like keyword should be parsed as a quote-like expression
    /// or treated as an identifier. In the parser-driven quote-like mode, the lexer does not
    /// auto-expand to DELIMITER at lookahead time, so we conservatively treat it as quote-like
    /// unless the next token is a fat comma (=>), in which case it's likely a bareword key.
    fn should_parse_quote_like(&self) -> bool {
        self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1)
            .is_none_or(|(k, _)| k != SyntaxKind::FAT_COMMA)
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

                // Parse the true expression allowing assignment-level precedence
                if !self.parse_expression_with_precedence(Precedence::ASSIGNMENT) {
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

                // Parse the false expression allowing assignment-level precedence
                if !self.parse_expression_with_precedence(Precedence::ASSIGNMENT) {
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
            // For comma and fat comma, allow trailing operators in appropriate contexts
            let parsed_rhs = self.parse_expression_with_precedence(next_min_precedence);
            if !parsed_rhs {
                // Check if this is a trailing comma or fat comma
                if (current_kind == SyntaxKind::COMMA || current_kind == SyntaxKind::FAT_COMMA)
                    && (self.at(SyntaxKind::R_BRACE)
                        || self.at(SyntaxKind::SEMICOLON)
                        || self.at_end())
                {
                    // This is a trailing comma/fat comma - that's OK, just finish the node
                    self.builder.finish_node();
                    break;
                } else {
                    self.error("Expected expression after binary operator");
                }
            }

            self.builder.finish_node();
        }

        true
    }

    /// Parse primary expression with postfix operations
    fn parse_primary_with_postfix(&mut self) -> bool {
        let checkpoint = self.builder.checkpoint();

        let subject_kind = self.primary_expr();
        if subject_kind == PostfixSubject::None {
            return false;
        }

        // Handle postfix operations
        self.parse_postfix_operations_with_checkpoint(checkpoint, subject_kind)
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

    fn primary_expr(&mut self) -> PostfixSubject {
        self.skip_whitespace_and_newlines();

        let Some(current_kind) = self.current_kind_value() else {
            return PostfixSubject::None;
        };

        // Treat bare keywords as identifiers when they appear before fat comma (=>)
        // or when they are inside hash braces (for hash keys like $h->{package})
        if current_kind.is_keyword()
            && (self.is_followed_by_fat_comma(0) || self.is_inside_hash_braces())
        {
            self.parse_ident_like_expr(true);
            return PostfixSubject::Other;
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
            SyntaxKind::GLOB_CONTENT => {
                self.builder
                    .start_node(SyntaxKind::ANGLE_BRACKET_EXPR.into());
                // Consume glob/IO expression as a value
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
                return PostfixSubject::Variable;
            }
            SyntaxKind::BACKSLASH => {
                // Reference operator as prefix: \expr
                self.parse_standard_prefix_expr("\\", Precedence::PREFIX, None);
            }
            SyntaxKind::CODE_SIGIL => {
                // Check if this is a complex code reference like &{expr} or &$var
                let next_token = self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1);
                match next_token {
                    Some((SyntaxKind::L_BRACE, _)) => {
                        // Complex code reference: &{$coderef}, &{"package::method"}, etc.
                        self.builder.start_node(SyntaxKind::COMPOUND_VAR.into());
                        self.bump(); // consume &
                        self.skip_whitespace_and_newlines();

                        self.bump(); // consume {
                        self.skip_whitespace_and_newlines();

                        if !self.expression() {
                            self.error("Expected expression inside braces after &");
                        }

                        self.skip_whitespace_and_newlines();
                        if self.at(SyntaxKind::R_BRACE) {
                            self.bump(); // consume }
                        } else {
                            self.error("Expected '}' to close code reference");
                        }

                        self.builder.finish_node();
                    }
                    Some((SyntaxKind::SCALAR_SIGIL, _)) => {
                        // Code dereference: &$coderef
                        self.builder.start_node(SyntaxKind::COMPOUND_VAR.into());
                        self.bump(); // consume &
                        self.skip_whitespace_and_newlines();

                        self.parse_variable(); // parse $var

                        self.builder.finish_node();
                    }
                    _ => {
                        // Simple function reference: &function
                        self.parse_function_ref();
                    }
                }
            }
            kind if kind.is_sigil() => {
                // All sigil-based variables are now handled by parse_variable
                self.parse_variable();
                return PostfixSubject::Variable;
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
                // Variable declaration as prefix operator
                self.parse_var_decl_prefix();
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
            SyntaxKind::TRY_KW | SyntaxKind::CATCH_KW | SyntaxKind::FINALLY_KW => {
                self.parse_ident_like_expr(true);
            }
            SyntaxKind::IDENT => {
                self.parse_ident_like_expr(false);
            }
            SyntaxKind::CARET => {
                // Handle caret followed by identifier: ^MATCH
                // Just consume as separate tokens
                self.bump_value(); // consume ^
                self.skip_whitespace_and_newlines();

                // Expect an identifier after ^
                if self.at(SyntaxKind::IDENT) {
                    self.bump_value();
                } else if self.current_kind().is_some_and(SyntaxKind::is_keyword) {
                    self.bump_as(SyntaxKind::IDENT);
                } else {
                    self.error("Expected identifier after '^'");
                }

                self.skip_whitespace_and_newlines();
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

                self.skip_whitespace_and_newlines();

                if self.at(SyntaxKind::R_PAREN) {
                    // After ')', expect an operator
                    self.bump_op(); // )
                    self.skip_whitespace_and_newlines();
                } else {
                    self.error("Expected ')' to close parenthesized list");
                }

                // Parenthesized expressions (including empty ()) allow [] subscript (list slices)
                return PostfixSubject::List;
            }
            SyntaxKind::L_BRACE => {
                // In expression context, always treat as hash reference
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
            T![q] | T![qq] | T![qx] | T![m] | T![qr] => {
                if self.should_parse_quote_like() {
                    self.qlike_expr(current_kind);
                } else {
                    self.parse_ident_like_expr(true);
                }
            }
            T![s] | T![tr] | T![y] => {
                if self.should_parse_quote_like() {
                    self.two_part_qlike_expr(current_kind);
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

                let next_value_token = self.peek_non_trivia_token_with_context(LexContext::Value);
                let should_parse_argument = !is_empty_regex(next_value_token);

                // Try to parse an expression argument, but don't require it
                // File test operators like -f can be used without arguments (they operate on $_)
                if should_parse_argument {
                    self.parse_expression_with_precedence(
                        crate::parser::expression::precedence::Precedence::PREFIX,
                    );
                }

                self.builder.finish_node();
            }
            _ => {
                // Should not reach here because is_at_start_of_expression checks this
                return PostfixSubject::None;
            }
        }
        PostfixSubject::Other
    }

    /// Parse anonymous subroutine expression: sub [PROTO]? [:ATTR]* { ... }
    fn anon_sub_expr(&mut self) {
        self.builder.start_node(SyntaxKind::ANON_SUB_EXPR.into());

        // Consume 'sub' keyword
        self.expect(SyntaxKind::SUB_KW);
        self.skip_whitespace_and_newlines();

        // Parse optional prototype, attributes, and required block shared with named subs
        self.parse_sub_tail();

        self.builder.finish_node();
    }

    fn require_expr(&mut self) {
        self.builder.start_node(SyntaxKind::REQUIRE_EXPR.into());

        // "require"
        self.expect(SyntaxKind::REQUIRE_KW);
        self.skip_whitespace_and_newlines();

        // VERSION literal, module name (qualified identifier), or general expression
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
            // Parse as general expression (handles module names, variables, etc.)
            self.parse_expression_with_precedence(precedence::Precedence(0));
        }
        self.skip_whitespace_and_newlines();

        // Option: import list (e.g., qw()) or comma-separated expressions (x => 1, y => 2)
        // Note: Unlike require statement, we don't consume additional expressions here
        // as they would be handled by the expression parser at a higher level

        self.builder.finish_node();
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

    /// Parse variable declaration as prefix operator (my/our/state/local)
    fn parse_var_decl_prefix(&mut self) {
        self.builder.start_node(SyntaxKind::VAR_DECL.into());

        // Variable declaration keyword (my, our, state, local)
        self.bump_value(); // consume the keyword
        self.skip_whitespace_and_newlines();

        // Parse the variable and any assignment with minimum precedence
        // Use LIST_ITEM precedence so a trailing comma in contexts like func(my $a,)
        // doesn't get treated as part of the declaration expression.
        if !self.parse_expression_with_precedence(Precedence::LIST_ITEM) {
            self.error("Expected expression after variable declaration keyword");
        }

        self.builder.finish_node();
    }
}
