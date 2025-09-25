use crate::lexer::LexContext;
use crate::SyntaxKind;
use crate::T;

use super::Parser;

impl Parser<'_> {
    /// Determine if {} should be parsed as a hash reference or block in statement context
    /// Returns true for hash reference if:
    /// - Number followed by comma/fat arrow: {1,} or {1=>}
    /// - Identifier followed by fat arrow: {a=>}
    /// - String followed by comma/fat arrow: {"key",} or {"key"=>}
    pub fn looks_like_hash_ref(&self) -> bool {
        self.looks_like_hash_ref_at_offset(0)
    }

    fn looks_like_hash_ref_at_offset(&self, brace_offset: usize) -> bool {
        if self
            .peek_nth_non_trivia_token_with_context(LexContext::Value, brace_offset)
            .is_none_or(|(kind, _)| kind != SyntaxKind::L_BRACE)
        {
            return false;
        }

        // Look ahead to see what's inside the braces
        let mut offset = brace_offset + 1; // Skip the opening brace

        // Skip any whitespace after opening brace
        while let Some((kind, _)) =
            self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset)
        {
            if !kind.is_trivia() {
                break;
            }
            offset += 1;
        }

        // Check the first non-trivia token inside braces
        let first_token = self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset);

        match first_token {
            // Empty braces: {} - treat as block in statement context
            Some((SyntaxKind::R_BRACE, _)) => false,

            // Number followed by comma or fat arrow: {1, or {1=> - hash reference
            Some((SyntaxKind::NUMBER, _)) => {
                let next_token =
                    self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset + 1);
                matches!(
                    next_token,
                    Some((SyntaxKind::COMMA | SyntaxKind::FAT_COMMA, _))
                )
            }

            // Identifier followed by fat arrow: {a=> - hash reference (bareword as hash key)
            Some((SyntaxKind::IDENT, _)) => {
                let next_token =
                    self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset + 1);
                matches!(next_token, Some((SyntaxKind::FAT_COMMA, _)))
            }

            // Keyword followed by fat arrow: {if=> - hash reference (keyword as hash key)
            Some((kind, _)) if kind.is_keyword() => self.is_followed_by_fat_comma(offset),

            // String followed by fat arrow or comma: {"key"=> or {"key", - hash reference
            Some((SyntaxKind::STRING, _)) => {
                let next_token =
                    self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset + 1);
                matches!(
                    next_token,
                    Some((SyntaxKind::FAT_COMMA | SyntaxKind::COMMA, _))
                )
            }

            // Everything else is a block
            _ => false,
        }
    }

    /// Parse an identifier-like expression (including cases where a keyword is coerced to IDENT)
    /// and handle possible function calls (regular or block).
    pub(super) fn parse_ident_like_expr(&mut self, coerce_current_to_ident: bool) {
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

        let next_value_token = self.peek_non_trivia_token_with_context(LexContext::Value);
        let mut next_kind = self
            .peek_non_trivia_token_with_context(LexContext::AmbiguousValueLookahead)
            .map(|(kind, _)| kind);

        if next_value_token
            .map(|(kind, _)| kind)
            .is_some_and(|kind| kind == SyntaxKind::HEREDOC_START)
        {
            next_kind = Some(SyntaxKind::HEREDOC_START);
        }

        if let Some(kind) = next_kind {
            if kind == SyntaxKind::L_PAREN {
                if self.try_parse_parenthesized_special_function_call(&function_name, start) {
                    return;
                }

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

        next_kind = Self::adjust_ambiguous_next_kind_for_builtin(
            &function_name,
            next_value_token,
            next_kind,
        );

        if let Some(kind) = next_kind {
            if Self::can_start_expression(kind)
                || (kind.is_keyword() && self.is_followed_by_fat_comma(0))
            {
                // We have a regular function call, wrap everything in FUNCTION_CALL_EXPR
                self.builder
                    .start_node_at(start, SyntaxKind::FUNCTION_CALL_EXPR.into());
                self.expression_list();
                self.builder.finish_node();
            }
        }
    }

    fn parse_parenthesized_special_call(
        &mut self,
        start: rowan::Checkpoint,
        node_kind: SyntaxKind,
        error_message: &str,
        parse_args: impl FnOnce(&mut Self),
    ) {
        self.builder.start_node_at(start, node_kind.into());

        self.bump_value(); // (
        self.skip_whitespace_and_newlines();

        parse_args(self);

        self.skip_whitespace_and_newlines();

        if self.at(SyntaxKind::R_PAREN) {
            self.bump_op(); // )
            self.skip_whitespace_and_newlines();
        } else {
            self.error(error_message);
        }

        self.builder.finish_node();
    }

    fn try_parse_parenthesized_special_function_call(
        &mut self,
        function_name: &str,
        start: rowan::Checkpoint,
    ) -> bool {
        if !self.at(SyntaxKind::L_PAREN) {
            return false;
        }

        if Self::is_parenthesized_block_builtin(function_name)
            && self
                .peek_nth_non_trivia_token_with_context(LexContext::Value, 1)
                .is_some_and(|(kind, _)| kind == SyntaxKind::L_BRACE)
            && !self.looks_like_hash_ref_at_offset(1)
        {
            self.parse_parenthesized_special_call(
                start,
                SyntaxKind::BLOCK_FUNCTION_CALL_EXPR,
                "Expected ')' after block arguments",
                |parser| parser.parse_block_function_args(function_name),
            );
            return true;
        }

        if Self::is_print_like_function(function_name) {
            self.parse_parenthesized_special_call(
                start,
                SyntaxKind::FUNCTION_CALL_EXPR,
                "Expected ')' after print arguments",
                |parser| parser.parse_print_like_args(),
            );
            return true;
        }

        false
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
        !function_name.is_empty() && !Self::is_zero_arg_builtin(function_name)
    }

    fn is_zero_arg_builtin(function_name: &str) -> bool {
        matches!(function_name, "fork" | "time" | "wait" | "wantarray")
    }

    /// Certain block-taking functions (`eval`, `do`) treat the block as their only argument.
    /// Stop parsing additional arguments after the first block for these names so operators like
    /// `//` are parsed in expression position instead of as another argument.
    fn block_args_end_after_block(function_name: &str) -> bool {
        matches!(function_name, "eval" | "do")
    }

    /// Builtins like `map`, `grep`, and `sort` accept a curious hybrid syntax where the block is
    /// wrapped in parentheses before the list arguments: `map({ ... } @list)`.  Perl only permits
    /// this exact form for those core functions—the parser still treats `{ ... }` as the leading
    /// block argument, but user-defined subs never see the same special casing.  Keep a tight
    /// whitelist so we don't accidentally parse ordinary function calls using this Perl-specific
    /// quirk as `BLOCK_FUNCTION_CALL_EXPR`s.
    fn is_parenthesized_block_builtin(function_name: &str) -> bool {
        matches!(function_name, "map" | "grep" | "sort")
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
        if self.at(T![ident]) {
            // Check if this bareword should be treated as a filehandle
            if self.should_treat_as_filehandle() {
                self.bump_value();
                consumed_filehandle = true;
                self.skip_whitespace_and_newlines();
            }
        } else if self.at(T![scalar_sigil]) {
            // Check if this scalar should be treated as a filehandle
            if self.should_treat_scalar_as_filehandle() {
                self.parse_variable();
                consumed_filehandle = true;
            }
        }

        if consumed_filehandle && self.at_any(&[T![,], T![=>]]) {
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
        let next_token =
            self.peek_nth_non_trivia_token_with_context(LexContext::AmbiguousValueLookahead, 1);

        match next_token {
            // If followed by parentheses or method/package separators, it's an expression
            Some((T!['('] | T![::] | T![->], _)) => false,
            // If followed by a likely binary operator, it's a function call in an expression
            Some((
                T![+]
                | T![-]
                | T![*]
                | T![/]
                | T![%]
                | T![^]
                | T![&]
                | T![|]
                | T![<]
                | T![>]
                | T![=]
                | T![!=]
                | T![<=]
                | T![>=]
                | SyntaxKind::STR_CMP
                | T![&&]
                | T![||],
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
        if !self.at(T![scalar_sigil]) {
            return false;
        }

        let next_after_dollar = self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1);
        if !matches!(next_after_dollar, Some((T![ident], _))) {
            return false;
        }

        // Now check what follows the $IDENT pattern
        let token_after_var =
            self.peek_nth_non_trivia_token_with_context(LexContext::AmbiguousValueLookahead, 2);

        match token_after_var {
            // If followed by postfix operations (arrow, brackets, etc.), it's not a simple filehandle
            Some((T![->] | T!['['] | T!['{'] | T!['('], _)) => false,
            // If followed by a likely binary operator, it's an expression, not a filehandle
            Some((
                T![+]
                | T![-]
                | T![*]
                | T![/]
                | T![%]
                | T![^]
                | T![&]
                | T![|]
                | T![<]
                | T![>]
                | T![=]
                | T![!=]
                | T![<=]
                | T![>=]
                | SyntaxKind::STR_CMP
                | T![&&]
                | T![||],
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

    /// Some builtins have fixed prototypes that influence how their first argument should be
    /// interpreted. When probing lookahead tokens in [`LexContext::AmbiguousValueLookahead`]
    /// we bypass value-context conveniences like regex and sigil recognition, so compensate for
    /// known names whose prototypes require those behaviors.
    fn adjust_ambiguous_next_kind_for_builtin(
        function_name: &str,
        next_value_token: Option<(SyntaxKind, &str)>,
        next_kind: Option<SyntaxKind>,
    ) -> Option<SyntaxKind> {
        if Self::is_zero_arg_builtin(function_name) {
            return None;
        }

        match (function_name, next_value_token, next_kind) {
            ("shift" | "pop", Some((SyntaxKind::REGEX_LITERAL, "//")), _) => {
                Some(SyntaxKind::DEFINED_OR)
            }
            ("split", Some((SyntaxKind::REGEX_LITERAL, _)), Some(SyntaxKind::DEFINED_OR)) => {
                Some(SyntaxKind::REGEX_LITERAL)
            }
            ("keys", Some((SyntaxKind::HASH_SIGIL, _)), Some(SyntaxKind::MODULO)) => {
                Some(SyntaxKind::HASH_SIGIL)
            }
            ("scalar", Some((SyntaxKind::IO_EXPR, _)), Some(SyntaxKind::LT)) => {
                Some(SyntaxKind::IO_EXPR)
            }
            _ => next_kind,
        }
    }

    fn is_print_like_function(function_name: &str) -> bool {
        matches!(function_name, "print" | "printf" | "say")
    }

    /// Parse method arguments if parentheses are present
    pub(super) fn parse_method_arguments(&mut self) {
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
}
