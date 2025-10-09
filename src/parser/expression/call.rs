use crate::lexer::LexContext;
use crate::SyntaxKind;
use crate::T;
use std::collections::HashMap;
use std::sync::LazyLock;

use super::precedence;
use super::Parser;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrototypeArg {
    Block,
    Filehandle,
    Array,
    HashOrArray,
    Any,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrototypeProfile {
    NoArgs,
    Single(PrototypeArg),
    Multi(PrototypeArg),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilehandleCandidate {
    Bareword,
    Scalar,
}

impl PrototypeProfile {
    const fn zero_arity(self) -> bool {
        matches!(self, Self::NoArgs)
    }

    const fn leading(self) -> Option<PrototypeArg> {
        match self {
            Self::NoArgs => None,
            Self::Single(kind) | Self::Multi(kind) => Some(kind),
        }
    }

    const fn allows_trailing_list(self) -> bool {
        matches!(self, Self::Multi(_)) || self.print_like()
    }

    const fn takes_block(self) -> bool {
        matches!(self.leading(), Some(PrototypeArg::Block))
    }

    const fn print_like(self) -> bool {
        matches!(self.leading(), Some(PrototypeArg::Filehandle))
    }
}

static BUILTIN_PROTOTYPES: LazyLock<HashMap<&'static str, PrototypeProfile>> =
    LazyLock::new(|| {
        [
            ("do", PrototypeProfile::Single(PrototypeArg::Block)),
            ("eval", PrototypeProfile::Single(PrototypeArg::Block)),
            ("grep", PrototypeProfile::Multi(PrototypeArg::Block)),
            ("map", PrototypeProfile::Multi(PrototypeArg::Block)),
            ("sort", PrototypeProfile::Multi(PrototypeArg::Block)),
            ("fork", PrototypeProfile::NoArgs),
            ("time", PrototypeProfile::NoArgs),
            ("wait", PrototypeProfile::NoArgs),
            ("wantarray", PrototypeProfile::NoArgs),
            ("shift", PrototypeProfile::Multi(PrototypeArg::Array)),
            ("pop", PrototypeProfile::Single(PrototypeArg::Array)),
            ("print", PrototypeProfile::Multi(PrototypeArg::Filehandle)),
            ("printf", PrototypeProfile::Multi(PrototypeArg::Filehandle)),
            ("say", PrototypeProfile::Multi(PrototypeArg::Filehandle)),
            ("warn", PrototypeProfile::Multi(PrototypeArg::Any)),
            ("keys", PrototypeProfile::Single(PrototypeArg::HashOrArray)),
            ("split", PrototypeProfile::Multi(PrototypeArg::Any)),
            ("scalar", PrototypeProfile::Single(PrototypeArg::Any)),
            ("substr", PrototypeProfile::Multi(PrototypeArg::Any)),
            ("index", PrototypeProfile::Multi(PrototypeArg::Any)),
        ]
        .into_iter()
        .collect()
    });

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
    pub(super) fn parse_ident_like_expr(&mut self) {
        let start = self.builder.checkpoint();

        // Capture name before consuming, for prototype lookups.
        let function_name = self.peek_block_function_basename().unwrap_or_default();

        // Check if this is a builtin with special parsing requirements
        if let Some(prototype) = Self::builtin_prototype(&function_name) {
            // Zero-arity builtins should just be parsed as identifiers (they can participate in expressions)
            if !prototype.zero_arity() {
                // Delegate to specialized builtin handler
                self.parse_builtin_function_call(&function_name, prototype, start);
                return;
            }
        }

        // Not a special builtin - parse as regular identifier
        self.parse_identifier_or_qualified();
        self.skip_whitespace_and_newlines();

        // Handle block-style calls for non-builtins
        if self.at(SyntaxKind::L_BRACE) && Self::is_block_function(&function_name) {
            self.builder
                .start_node_at(start, SyntaxKind::BLOCK_FUNCTION_CALL_EXPR.into());
            self.parse_block_function_args(&function_name);
            self.builder.finish_node();
            return;
        }

        // Handle parenthesized calls (leave for postfix parser)
        if self.at(SyntaxKind::L_PAREN) {
            return;
        }

        // Handle indirect function calls for non-builtins
        self.parse_non_builtin_indirect_call(&function_name, start);
    }

    /// Parse a builtin function call with specialized handling based on its prototype.
    /// This method consolidates all builtin-specific parsing logic.
    fn parse_builtin_function_call(
        &mut self,
        function_name: &str,
        prototype: &PrototypeProfile,
        start: rowan::Checkpoint,
    ) {
        // First, consume the function name
        self.parse_identifier_or_qualified();
        self.skip_whitespace_and_newlines();

        // Dispatch based on what follows and the function's prototype
        if self.at(SyntaxKind::L_BRACE) && prototype.takes_block() {
            // Block-style call for builtins that take blocks (e.g., grep { ... } @list)
            self.builder
                .start_node_at(start, SyntaxKind::BLOCK_FUNCTION_CALL_EXPR.into());
            self.parse_block_function_args(function_name);
            self.builder.finish_node();
        } else if self.at(SyntaxKind::L_PAREN) {
            // Parenthesized call
            self.parse_builtin_parenthesized_call(function_name, prototype, start);
        } else if prototype.print_like() {
            // Print-like functions without parentheses (e.g., print "hello")
            if self.is_at_start_of_expression() {
                self.builder
                    .start_node_at(start, SyntaxKind::FUNCTION_CALL_EXPR.into());
                self.parse_print_like_args();
                self.builder.finish_node();
            }
        } else {
            // Indirect call for other builtins (e.g., shift @array)
            self.parse_builtin_indirect_call(function_name, prototype, start);
        }
    }

    /// Handle parenthesized builtin function calls.
    fn parse_builtin_parenthesized_call(
        &mut self,
        function_name: &str,
        prototype: &PrototypeProfile,
        start: rowan::Checkpoint,
    ) {
        // Check for special parenthesized patterns
        if prototype.takes_block()
            && self
                .peek_nth_non_trivia_token_with_context(LexContext::Value, 1)
                .is_some_and(|(kind, _)| kind == SyntaxKind::L_BRACE)
            && !self.looks_like_hash_ref_at_offset(1)
        {
            // Block inside parentheses: grep({ ... } @list)
            self.parse_parenthesized_special_call(
                start,
                SyntaxKind::BLOCK_FUNCTION_CALL_EXPR,
                "Expected ')' after block arguments",
                |parser| parser.parse_block_function_args(function_name),
            );
        } else if prototype.print_like() {
            // Print-like with parentheses: print("hello")
            self.parse_parenthesized_special_call(
                start,
                SyntaxKind::FUNCTION_CALL_EXPR,
                "Expected ')' after print arguments",
                |parser| parser.parse_print_like_args(),
            );
        }
        // Otherwise, leave for postfix parser to handle as regular function call
    }

    /// Handle indirect builtin function calls (no parentheses).
    fn parse_builtin_indirect_call(
        &mut self,
        function_name: &str,
        _prototype: &PrototypeProfile,
        start: rowan::Checkpoint,
    ) {
        // Check if we have arguments following
        let next_value_token = self.peek_non_trivia_token_with_context(LexContext::Value);
        let mut next_kind = self
            .peek_non_trivia_token_with_context(LexContext::AmbiguousValueLookahead)
            .map(|(kind, _)| kind);

        if next_value_token.is_some_and(|(kind, _)| kind == SyntaxKind::HEREDOC_START) {
            next_kind = Some(SyntaxKind::HEREDOC_START);
        }

        // Adjust lookahead for ambiguous tokens based on built-in prototypes.
        next_kind = Self::adjust_ambiguous_next_kind_for_builtin(
            function_name,
            next_value_token,
            next_kind,
        );

        // If the next token can start an expression, parse it as a list of arguments.
        if let Some(kind) = next_kind {
            if Self::can_start_expression(kind)
                || (kind.is_keyword() && self.is_followed_by_fat_comma(0))
            {
                self.builder
                    .start_node_at(start, SyntaxKind::FUNCTION_CALL_EXPR.into());
                self.expression_list();
                self.builder.finish_node();
            }
        }
    }

    /// Handle indirect function calls for non-builtin identifiers (no parentheses).
    fn parse_non_builtin_indirect_call(&mut self, function_name: &str, start: rowan::Checkpoint) {
        let next_value_token = self.peek_non_trivia_token_with_context(LexContext::Value);
        let mut next_kind = self
            .peek_non_trivia_token_with_context(LexContext::AmbiguousValueLookahead)
            .map(|(kind, _)| kind);

        if next_value_token.is_some_and(|(kind, _)| kind == SyntaxKind::HEREDOC_START) {
            next_kind = Some(SyntaxKind::HEREDOC_START);
        }

        // Adjust for zero-arity builtins (e.g., fork, wait, time) - they don't take arguments
        if Self::builtin_prototype(function_name).is_some_and(|spec| spec.zero_arity()) {
            return;
        }

        // For non-builtins, treat `/` as division operator instead of forcing regex
        if next_kind == Some(SyntaxKind::SLASH)
            && next_value_token.is_some_and(|(kind, _)| kind == SyntaxKind::REGEX_LITERAL)
        {
            next_kind = None;
        }

        // If the next token can start an expression, parse it as a list of arguments.
        if let Some(kind) = next_kind {
            if Self::can_start_expression(kind)
                || (kind.is_keyword() && self.is_followed_by_fat_comma(0))
            {
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
        let allow_more_args =
            Self::builtin_prototype(function_name).is_none_or(|spec| spec.allows_trailing_list());

        if allow_more_args && self.is_at_start_of_expression() {
            self.expression_list();
        }
    }

    /// Determine whether a function name should be treated as accepting a leading block argument.
    ///
    /// We continue to allow any function name (including qualified names) to take a block
    /// argument. Builtins opt out via prototype metadata when they are truly zero-arity.
    fn is_block_function(function_name: &str) -> bool {
        if let Some(spec) = Self::builtin_prototype(function_name) {
            return spec.takes_block();
        }

        !function_name.is_empty()
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

        // Use lookahead to determine if this is a filehandle pattern. Only treat IDENT/SCALAR as
        // filehandle if followed by whitespace or end of statement. Otherwise treat as regular
        // expression arguments.
        if let Some(candidate) = self.filehandle_candidate_to_consume() {
            match candidate {
                FilehandleCandidate::Bareword => {
                    self.bump_value();
                    consumed_filehandle = true;
                    self.skip_whitespace_and_newlines();
                }
                FilehandleCandidate::Scalar => {
                    self.parse_variable();
                    consumed_filehandle = true;
                }
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

    fn filehandle_candidate_to_consume(&self) -> Option<FilehandleCandidate> {
        let candidate = if self.at(SyntaxKind::IDENT) {
            FilehandleCandidate::Bareword
        } else if self.at(SyntaxKind::SCALAR_SIGIL) {
            let next_after_dollar =
                self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1);

            if !matches!(next_after_dollar, Some((SyntaxKind::IDENT, _))) {
                return None;
            }
            FilehandleCandidate::Scalar
        } else {
            return None;
        };

        // If a heredoc start follows, keep treating the preceding token as a filehandle even
        // though `<<` ordinarily lexes as a shift operator in ambiguous contexts.
        let heredoc_offset = match candidate {
            FilehandleCandidate::Bareword => 1,
            FilehandleCandidate::Scalar => 2,
        };

        if self
            .peek_nth_non_trivia_token_with_context(LexContext::Value, heredoc_offset)
            .is_some_and(|(kind, _)| kind == SyntaxKind::HEREDOC_START)
        {
            return Some(candidate);
        }

        match candidate {
            FilehandleCandidate::Bareword => {
                // Look ahead to see what follows the IDENT. Use Operator context to help disambiguate.
                let next_token = self
                    .peek_nth_non_trivia_token_with_context(LexContext::AmbiguousValueLookahead, 1);

                // If followed by parentheses or method/package separators, it's an expression
                match next_token {
                    Some((T!['('] | T![::] | T![->], _)) => None,
                    _ => self.is_filehandle_context(next_token).then_some(candidate),
                }
            }
            FilehandleCandidate::Scalar => {
                // Now check what follows the $IDENT pattern
                let token_after_var = self
                    .peek_nth_non_trivia_token_with_context(LexContext::AmbiguousValueLookahead, 2);

                // If followed by postfix operations (arrow, brackets, etc.), it's not a simple filehandle
                match token_after_var {
                    Some((T![->] | T!['['] | T!['{'] | T!['('], _)) => None,
                    _ => self
                        .is_filehandle_context(token_after_var)
                        .then_some(candidate),
                }
            }
        }
    }

    /// Common logic to determine if the token following a potential filehandle indicates
    /// that it should be treated as such.
    fn is_filehandle_context(&self, token: Option<(SyntaxKind, &str)>) -> bool {
        match token {
            // If followed by a binary operator, it's part of an expression, not a filehandle.
            Some((kind, _)) if precedence::get_operator_info(kind).is_some() => false,
            // If followed by something that can start an expression, treat as filehandle
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
    ///
    /// Earlier we patched in ad-hoc cases (e.g. shifting `%`/`@`, fixing `//` and `<` for `split`
    /// and `scalar`). Those rules now live inside the prototype metadata below, so we keep the
    /// comment to explain why the adjustments exist.
    fn adjust_ambiguous_next_kind_for_builtin(
        function_name: &str,
        next_value_token: Option<(SyntaxKind, &str)>,
        next_kind: Option<SyntaxKind>,
    ) -> Option<SyntaxKind> {
        if let Some(profile) = Self::builtin_prototype(function_name) {
            if profile.zero_arity() {
                return None;
            }

            if let Some(leading) = profile.leading() {
                match leading {
                    PrototypeArg::Array => {
                        if let Some((SyntaxKind::ARRAY_SIGIL, _)) = next_value_token {
                            return Some(SyntaxKind::ARRAY_SIGIL);
                        }
                        if let Some((SyntaxKind::REGEX_LITERAL, literal)) = next_value_token {
                            if literal == "//" {
                                return Some(SyntaxKind::DEFINED_OR);
                            }
                        }
                    }
                    PrototypeArg::HashOrArray => {
                        if let Some((value_kind, _)) = next_value_token {
                            if matches!(
                                value_kind,
                                SyntaxKind::HASH_SIGIL | SyntaxKind::ARRAY_SIGIL
                            ) {
                                return Some(value_kind);
                            }
                        }
                    }
                    PrototypeArg::Any => {
                        if next_kind == Some(SyntaxKind::DEFINED_OR) {
                            if let Some((SyntaxKind::REGEX_LITERAL, _)) = next_value_token {
                                return Some(SyntaxKind::REGEX_LITERAL);
                            }
                        }
                        if next_kind == Some(SyntaxKind::LT) {
                            if let Some((SyntaxKind::IO_EXPR, _)) = next_value_token {
                                return Some(SyntaxKind::IO_EXPR);
                            }
                        }
                    }
                    PrototypeArg::Block | PrototypeArg::Filehandle => {}
                }
            }
        }

        next_kind
    }

    fn builtin_prototype(function_name: &str) -> Option<&'static PrototypeProfile> {
        BUILTIN_PROTOTYPES.get(function_name)
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
