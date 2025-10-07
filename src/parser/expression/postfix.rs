use crate::lexer::LexContext;
use crate::SyntaxKind;

use super::{Parser, PostfixSubject};

impl Parser<'_> {
    /// Parse a postfix increment or decrement operator
    pub(super) fn parse_postfix_op(
        &mut self,
        initial_checkpoint: rowan::Checkpoint,
        op_kind: SyntaxKind,
    ) {
        self.builder
            .start_node_at(initial_checkpoint, SyntaxKind::POSTFIX_EXPR.into());
        self.bump_op_as(op_kind);
        self.skip_whitespace_and_newlines();
        self.builder.finish_node();
    }

    pub(super) fn parse_postfix_slice_expr(
        &mut self,
        initial_checkpoint: rowan::Checkpoint,
        sigil_kind: SyntaxKind,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) {
        let node_kind = match sigil_kind {
            SyntaxKind::ARRAY_SIGIL => SyntaxKind::POSTFIX_ARRAY_SLICE_EXPR,
            SyntaxKind::HASH_SIGIL => SyntaxKind::POSTFIX_HASH_SLICE_EXPR,
            _ => unreachable!("Unsupported sigil for postfix slice"),
        };

        self.builder
            .start_node_at(initial_checkpoint, node_kind.into());

        // Consume the sigil (@ or %)
        self.bump_value();
        self.skip_whitespace_and_newlines();

        if self.at(opening) {
            self.bump_value();
            self.skip_whitespace_and_newlines();
        } else {
            let message = match (sigil_kind, opening) {
                (SyntaxKind::ARRAY_SIGIL, SyntaxKind::L_BRACKET) => {
                    "Expected '[' after '@' in postfix slice"
                }
                (SyntaxKind::ARRAY_SIGIL, SyntaxKind::L_BRACE) => {
                    "Expected '{' after '@' in postfix slice"
                }
                (SyntaxKind::HASH_SIGIL, SyntaxKind::L_BRACKET) => {
                    "Expected '[' after '%' in postfix slice"
                }
                (SyntaxKind::HASH_SIGIL, SyntaxKind::L_BRACE) => {
                    "Expected '{' after '%' in postfix slice"
                }
                _ => "Expected slice delimiter after sigil",
            };
            self.error(message);
        }

        if !self.expression() {
            let message = match sigil_kind {
                SyntaxKind::ARRAY_SIGIL => "Expected expression in postfix array slice",
                SyntaxKind::HASH_SIGIL => "Expected expression in postfix hash slice",
                _ => "Expected expression in postfix slice",
            };
            self.error(message);
        }

        self.skip_whitespace_and_newlines();

        if self.at(closing) {
            self.bump_op();
            self.skip_whitespace_and_newlines();
        } else {
            let message = match closing {
                SyntaxKind::R_BRACKET => "Expected ']' after postfix slice expression",
                SyntaxKind::R_BRACE => "Expected '}' after postfix slice expression",
                _ => "Expected closing delimiter after postfix slice expression",
            };
            self.error(message);
        }

        self.builder.finish_node();
    }

    /// Parse all postfix operations (method calls, subscripts, etc.)
    pub(crate) fn parse_postfix_operations_with_checkpoint(
        &mut self,
        initial_checkpoint: rowan::Checkpoint,
        initial_subject: PostfixSubject,
    ) -> bool {
        let mut current_subject = initial_subject;
        loop {
            // Always look ahead in Operator context for postfix continuations
            let Some(next_kind_op) = self
                .peek_non_trivia_token_with_context(LexContext::Operator)
                .map(|(k, _)| k)
            else {
                break;
            };

            // Align the real lexer position with the non-trivia operator we just peeked.
            // Without this, trivia like newlines remain pending and the upcoming bump_* call
            // ends up consuming the trivia instead of the operator (e.g. expr\n->).
            self.skip_whitespace_and_newlines();

            match next_kind_op {
                SyntaxKind::INCREMENT => {
                    self.parse_postfix_op(initial_checkpoint, SyntaxKind::POSTFIX_INCREMENT);
                }
                SyntaxKind::DECREMENT => {
                    self.parse_postfix_op(initial_checkpoint, SyntaxKind::POSTFIX_DECREMENT);
                }
                SyntaxKind::ARROW => {
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
                            current_subject = PostfixSubject::Variable;
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
                            current_subject = PostfixSubject::Variable;
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
                            current_subject = PostfixSubject::Other;
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
                            current_subject = PostfixSubject::Other;
                        }
                        Some(kind) if kind.is_sigil() => {
                            let mut handled_slice = false;

                            if matches!(kind, SyntaxKind::ARRAY_SIGIL | SyntaxKind::HASH_SIGIL) {
                                let next_token = self
                                    .peek_nth_non_trivia_token_with_context(LexContext::Value, 1)
                                    .map(|(k, _)| k);

                                handled_slice = if let Some(
                                    opening @ (SyntaxKind::L_BRACKET | SyntaxKind::L_BRACE),
                                ) = next_token
                                {
                                    let closing = if opening == SyntaxKind::L_BRACKET {
                                        SyntaxKind::R_BRACKET
                                    } else {
                                        SyntaxKind::R_BRACE
                                    };

                                    self.parse_postfix_slice_expr(
                                        initial_checkpoint,
                                        kind,
                                        opening,
                                        closing,
                                    );
                                    current_subject = PostfixSubject::Other;
                                    true
                                } else {
                                    false
                                };
                            }

                            if handled_slice {
                                continue;
                            }

                            // Dynamic method call: expr->$method()
                            self.builder.start_node_at(
                                initial_checkpoint,
                                SyntaxKind::METHOD_CALL_EXPR.into(),
                            );

                            self.parse_variable();
                            self.skip_whitespace_and_newlines();

                            self.parse_method_arguments();

                            self.builder.finish_node();
                            current_subject = PostfixSubject::Other;
                        }
                        _ => {
                            self.error(
                                "Expected '{', '[', '(', identifier, or variable after '->'",
                            );
                            break;
                        }
                    }
                }
                SyntaxKind::L_PAREN => {
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
                    current_subject = PostfixSubject::Other;
                }
                SyntaxKind::L_BRACKET => {
                    // Direct array subscription: expr[index]
                    // Allowed on variables and parenthesized lists
                    if current_subject == PostfixSubject::Other {
                        self.error(
                            "Direct array subscription requires '->' operator (e.g., func()->[0])",
                        );
                        break;
                    }

                    self.builder.start_node_at(
                        initial_checkpoint,
                        SyntaxKind::ARRAY_SUBSCRIPTION_EXPR.into(),
                    );
                    self.bump(); // [
                    self.skip_whitespace_and_newlines();

                    if !self.expression() {
                        self.error("Expected expression in array subscription");
                    }

                    self.skip_whitespace_and_newlines();

                    if self.at(SyntaxKind::R_BRACKET) {
                        // After ']', expect an operator
                        self.bump_op(); // ]
                        self.skip_whitespace_and_newlines();
                    } else {
                        self.error("Expected ']' after array index");
                    }

                    self.builder.finish_node();
                    current_subject = PostfixSubject::Variable;
                }
                SyntaxKind::L_BRACE => {
                    // Direct hash subscription: expr{key}
                    // Only allowed on variables (not on parenthesized lists)
                    if current_subject != PostfixSubject::Variable {
                        self.error(
                            "Direct hash subscription requires '->' operator (e.g., func()->{key})",
                        );
                        break;
                    }

                    self.builder.start_node_at(
                        initial_checkpoint,
                        SyntaxKind::HASH_SUBSCRIPTION_EXPR.into(),
                    );
                    self.bump(); // {
                    self.skip_whitespace_and_newlines();

                    if !self.expression() {
                        self.error("Expected expression in hash subscription");
                    }

                    self.skip_whitespace_and_newlines();

                    if self.at(SyntaxKind::R_BRACE) {
                        // After '}', expect an operator
                        self.bump_op(); // }
                        self.skip_whitespace_and_newlines();
                    } else {
                        self.error("Expected '}' after hash key");
                    }

                    self.builder.finish_node();
                    current_subject = PostfixSubject::Variable;
                }
                SyntaxKind::POSTFIX_DEREF_ARRAY
                | SyntaxKind::POSTFIX_DEREF_HASH
                | SyntaxKind::POSTFIX_DEREF_SCALAR
                | SyntaxKind::POSTFIX_DEREF_ARRAY_LAST_INDEX
                | SyntaxKind::POSTFIX_DEREF_CODE
                | SyntaxKind::POSTFIX_DEREF_GLOB => {
                    // Postfix dereference: expr->@*, expr->%*, expr->$*, expr->$#*, expr->&*, expr->**
                    self.builder
                        .start_node_at(initial_checkpoint, SyntaxKind::POSTFIX_DEREF_EXPR.into());
                    // Postfix deref is a value-ending token; expect operator next
                    self.bump_op(); // ->@*, ->%*, ->$*, ->$#*, ->&*, or ->**
                    self.skip_whitespace_and_newlines();
                    self.builder.finish_node();
                    current_subject = PostfixSubject::Other;
                }
                _ => {
                    // No more postfix operations
                    break;
                }
            }
        }
        true
    }
}
