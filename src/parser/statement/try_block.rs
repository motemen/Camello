use crate::parser::{expression::PostfixSubject, Parser};
use crate::{SyntaxKind, T};
use rowan::Checkpoint;

impl Parser<'_> {
    pub(super) fn try_stmt(&mut self) -> bool {
        assert!(
            self.options.enable_try_statement,
            "try_stmt called without try statement support enabled"
        );

        // Create two checkpoints:
        // - stmt_checkpoint: will wrap as either TRY_STMT or STMT (for expression statement)
        // - expr_checkpoint: will wrap as BLOCK_FUNCTION_CALL_EXPR if it's function-style
        let stmt_checkpoint = self.builder.checkpoint();
        let expr_checkpoint = self.builder.checkpoint();

        // Parse 'try' keyword and block
        self.bump(); // consume TRY_KW
        self.skip_whitespace_and_newlines();

        if !self.at(T!['{']) {
            self.error("Expected block after 'try'");
            return false;
        }

        self.block();
        self.skip_whitespace_and_newlines();

        // Dispatch based on what follows the try block
        if self.at(T![catch]) || self.at(T![finally]) {
            self.parse_try_catch_statement(stmt_checkpoint)
        } else {
            self.parse_try_function_expression(stmt_checkpoint, expr_checkpoint)
        }
    }

    /// Parse statement-style try/catch: try { ... } catch { ... }
    fn parse_try_catch_statement(&mut self, stmt_checkpoint: Checkpoint) -> bool {
        self.builder
            .start_node_at(stmt_checkpoint, SyntaxKind::TRY_STMT.into());

        if self.at(T![catch]) {
            self.parse_catch_clause();
            self.skip_whitespace_and_newlines();
        }

        if self.at(T![finally]) {
            self.parse_finally_clause();
            self.skip_whitespace_and_newlines();
        }

        if self.at(T![;]) {
            self.bump();
        }

        self.builder.finish_node();
        true
    }

    /// Parse function-style try: try { ... } (e.g., Try::Tiny)
    fn parse_try_function_expression(
        &mut self,
        stmt_checkpoint: Checkpoint,
        expr_checkpoint: Checkpoint,
    ) -> bool {
        // Wrap try+block as BLOCK_FUNCTION_CALL_EXPR
        self.builder
            .start_node_at(expr_checkpoint, SyntaxKind::BLOCK_FUNCTION_CALL_EXPR.into());

        // Parse additional arguments if present (e.g., try { } $x, $y)
        if self.is_at_start_of_expression() {
            self.expression_list();
        }

        self.builder.finish_node();

        // Handle postfix operations (e.g., try { }->method())
        self.parse_postfix_operations_with_checkpoint(expr_checkpoint, PostfixSubject::Other);

        // Handle postfix modifiers and trailing semicolon
        self.handle_postfix_and_semicolon();

        // Wrap everything as STMT
        self.builder
            .start_node_at(stmt_checkpoint, SyntaxKind::STMT.into());
        self.builder.finish_node();

        true
    }

    fn parse_catch_clause(&mut self) {
        self.builder.start_node(SyntaxKind::CATCH_BLOCK.into());

        self.expect(T![catch]);
        self.skip_whitespace_and_newlines();

        if self.at(T!['(']) {
            self.builder.start_node(SyntaxKind::CATCH_PARAM.into());
            self.expect_value(T!['(']);
            self.skip_whitespace_and_newlines();

            // NOTE: We use expression() here which is permissive and allows any expression,
            // not just scalar variables. This is intentional - while the current Perl syntax
            // only allows scalar variables like `catch ($e)`, the syntax might evolve in the
            // future, so we keep the parser flexible rather than restricting it now.
            if !self.expression() {
                self.error_without_consuming("Expected expression in catch parameter");
            }
            self.skip_whitespace_and_newlines();

            if self.at(T![')']) {
                self.expect_op(T![')']);
            } else {
                self.error("Expected ')' after catch parameter");
            }

            self.builder.finish_node();
            self.skip_whitespace_and_newlines();
        }

        if self.at(T!['{']) {
            self.block();
        } else {
            self.error("Expected block after 'catch'");
        }

        self.builder.finish_node();
    }

    fn parse_finally_clause(&mut self) {
        self.builder.start_node(SyntaxKind::FINALLY_BLOCK.into());

        self.expect(T![finally]);
        self.skip_whitespace_and_newlines();

        if self.at(T!['{']) {
            self.block();
        } else {
            self.error("Expected block after 'finally'");
        }

        self.builder.finish_node();
    }
}
