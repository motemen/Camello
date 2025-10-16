use crate::{lexer::LexContext, SyntaxKind, T};

use super::Parser;

mod control_flow;
mod declarations;
mod postfix;
mod subroutine;
mod try_block;

#[cfg(test)]
mod tests;

impl Parser<'_> {
    pub fn statement(&mut self) -> bool {
        self.skip_whitespace_and_newlines();

        match self.current_kind() {
            Some(kind)
                if Self::is_label_identifier_kind(kind)
                    && self
                        .peek_nth_non_trivia_token_with_context(LexContext::Operator, 1)
                        .is_some_and(|(next_kind, _)| next_kind == T![:]) =>
            {
                self.labeled_stmt();
                true
            }
            Some(kind @ (T![my] | T![our] | T![state])) => {
                if self.looks_like_lexical_sub_definition() {
                    self.sub_def_with_modifier(Some(kind));
                    true
                } else {
                    // Route through expression system for prefix operator handling
                    self.expression_stmt()
                }
            }
            Some(T![local]) => {
                // Route through expression system for prefix operator handling
                self.expression_stmt()
            }
            Some(T![sub]) => {
                if self.looks_like_sub_definition() {
                    self.sub_def_with_modifier(None);
                    true
                } else {
                    self.expression_stmt()
                }
            }
            Some(T![if]) => {
                self.if_stmt();
                true
            }
            Some(T![unless]) => {
                self.unless_stmt();
                true
            }
            Some(T![for] | T![foreach]) => {
                self.for_stmt();
                true
            }
            Some(T![while]) => {
                self.while_stmt();
                true
            }
            Some(T![until]) => {
                self.until_stmt();
                true
            }
            Some(T![given]) => {
                self.given_stmt();
                true
            }
            Some(T![package]) => {
                self.package_stmt();
                true
            }
            Some(T![try]) => {
                if self.options.enable_try_statement
                    && self
                        .peek_nth_non_trivia_token_with_context(LexContext::Value, 1)
                        .is_some_and(|(next_kind, _)| next_kind == T!['{'])
                {
                    self.try_stmt()
                } else {
                    self.expression_stmt()
                }
            }
            Some(T![use]) => {
                self.use_or_no_stmt(true);
                true
            }
            Some(T![no]) => {
                self.use_or_no_stmt(false);
                true
            }
            Some(kind) if kind.is_phase_block_kw() => {
                self.phase_block_stmt(kind);
                true
            }
            Some(T![__END__] | T![__DATA__]) => {
                self.data_section();
                true
            }
            Some(SyntaxKind::POD_CONTENT) => {
                self.pod_block();
                true
            }
            Some(T![=cut]) => {
                // =cut without a preceding POD block is an error
                self.error("Found =cut without a preceding POD command");
                self.bump(); // Consume the =cut token
                true
            }
            Some(T!['}']) => {
                // End of block, notify the caller.
                false
            }
            Some(SyntaxKind::ELLIPSIS) => {
                self.ellipsis_stmt();
                true
            }
            Some(T![;]) => {
                self.empty_stmt();
                true
            }
            Some(T!['{']) => {
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

    fn is_label_identifier_kind(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::IDENT
                | SyntaxKind::BEGIN_KW
                | SyntaxKind::END_BLOCK_KW
                | SyntaxKind::INIT_KW
                | SyntaxKind::CHECK_KW
                | SyntaxKind::UNITCHECK_KW
        )
    }

    pub(super) fn bump_label_identifier(&mut self) {
        match self.current_kind() {
            Some(SyntaxKind::IDENT) => self.bump(),
            Some(kind) if Self::is_label_identifier_kind(kind) => {
                self.bump_as(SyntaxKind::IDENT);
            }
            Some(_) => {
                self.error("Expected label identifier");
                self.bump();
            }
            None => self.error("Expected label identifier"),
        }
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

        // Handle postfix modifiers and trailing semicolon
        self.handle_postfix_and_semicolon();

        self.builder.finish_node();
        true
    }

    pub fn block(&mut self) {
        self.builder.start_node(SyntaxKind::BLOCK_STMT.into());

        // Entering a block; inside expects statements/values
        self.expect_value(T!['{']);
        self.skip_whitespace_and_newlines();

        while !self.at(T!['}']) && !self.at_end() {
            if !self.statement() {
                self.error("Expected a statement in block, but found an unexpected token.");
            }
            self.skip_whitespace_and_newlines();
        }

        // After closing '}', expect an operator/statement boundary
        self.expect_op(T!['}']);

        self.builder.finish_node();
    }
}
