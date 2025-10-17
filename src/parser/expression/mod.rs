mod call;
mod postfix;
pub mod precedence;
pub mod primary;
pub mod quoted;

use crate::lexer::LexContext;
use crate::{SyntaxKind, T};
use precedence::{get_operator_info, OperatorInfo, Precedence};
use rowan::Checkpoint;

use super::{Parser, PrimaryRole};

/// Result of parsing a primary expression, indicating subscript eligibility
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PostfixSubject {
    /// No primary expression was parsed
    None,
    /// Variable: allows both [] and {} direct subscripts
    Variable,
    /// Parenthesized list: allows [] subscript only, {} requires ->
    List,
    /// Other expressions: both [] and {} require ->
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOperatorKind {
    Standard,
    CompoundAssignment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOperatorOutcome {
    Continue,
    Break,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TernaryOutcome {
    NotTernary,
    Handled,
    Break,
}

#[derive(Debug, Clone, Copy)]
struct BinaryOperatorState {
    info: OperatorInfo,
    kind: BinaryOperatorKind,
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
            .is_none_or(|(k, _)| k != T![=>])
    }

    /// Check if a `<` token in operator position should be treated as an IO operator.
    ///
    /// This implements the "cover syntax" approach to disambiguate between:
    /// - `<` as a less-than comparison operator (e.g., `f < $x > 1`)
    /// - `<...>` as an IO operator (e.g., `decode <$fh>`)
    ///
    /// Strategy:
    /// 1. In Value context, the lexer would tokenize `<...>` as IO_EXPR
    /// 2. Check if there's a valid expression after the closing `>`
    /// 3. If there IS a valid RHS, it's a comparison (e.g., `f < $x > 1`)
    /// 4. If there is NO valid RHS, it's an IO operator (e.g., `decode <$fh>`)
    fn is_lt_an_io_operator(&self) -> bool {
        // First check: can this be tokenized as IO_EXPR in Value context?
        let Some((SyntaxKind::IO_EXPR, _io_text)) =
            self.peek_non_trivia_token_with_context(LexContext::Value)
        else {
            return false; // Not an IO operator pattern
        };

        // Second check: is there a valid expression after the IO_EXPR?
        // Clone the lexer and consume the IO_EXPR to see what follows
        let mut cloned = self.lexer.clone();
        cloned.next_token_with_context(LexContext::Value); // consume IO_EXPR

        // Check what comes after
        let after_io = cloned.peek_non_trivia_with_context(LexContext::Value);

        // If there's a valid expression starter after the IO pattern, this is actually
        // a comparison with a valid RHS (e.g., `< $x > 1`)
        // Otherwise it's an IO operator (e.g., `<$fh>;` or `<$fh>,`)
        match after_io {
            Some((kind, _)) if Self::can_start_expression(kind) => {
                // There's a valid RHS expression, so this is a comparison, not an IO operator
                false
            }
            _ => {
                // No valid RHS expression, so this is an IO operator
                true
            }
        }
    }

    pub fn expression(&mut self) -> bool {
        self.parse_expression_with_precedence(Precedence::LOWEST)
    }

    /// Core Pratt parser: parse expression with given minimum precedence
    pub fn parse_expression_with_precedence(&mut self, min_precedence: Precedence) -> bool {
        let checkpoint = self.builder.checkpoint();

        // Parse left-hand side (primary expression with postfix operations)
        let primary_role = self.parse_primary_with_postfix();
        if primary_role == PrimaryRole::None {
            return false;
        }

        // Parse binary operators with precedence climbing
        loop {
            let Some(current_kind) = self
                .peek_non_trivia_token_with_context(LexContext::Operator)
                .map(|(k, _)| k)
            else {
                break;
            };

            if let Some(result) = self.handle_io_operator(primary_role, current_kind, checkpoint) {
                return result;
            }

            match self.handle_ternary(current_kind, checkpoint, min_precedence) {
                TernaryOutcome::NotTernary => {}
                TernaryOutcome::Handled => continue,
                TernaryOutcome::Break => break,
            }

            match self.handle_binary_operator(current_kind, checkpoint, min_precedence) {
                BinaryOperatorOutcome::Continue => continue,
                BinaryOperatorOutcome::Break => break,
            }
        }

        true
    }

    fn handle_io_operator(
        &mut self,
        primary_role: PrimaryRole,
        current_kind: SyntaxKind,
        checkpoint: Checkpoint,
    ) -> Option<bool> {
        if current_kind == T![<]
            && primary_role == PrimaryRole::Bareword
            && self.is_lt_an_io_operator()
        {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::FUNCTION_CALL_EXPR.into());

            self.skip_whitespace_and_newlines();
            self.expression_list();
            self.builder.finish_node();

            return Some(true);
        }

        None
    }

    fn handle_ternary(
        &mut self,
        current_kind: SyntaxKind,
        checkpoint: Checkpoint,
        min_precedence: Precedence,
    ) -> TernaryOutcome {
        if current_kind != T![?] {
            return TernaryOutcome::NotTernary;
        }

        let ternary_precedence = Precedence::TERNARY;
        if ternary_precedence < min_precedence {
            return TernaryOutcome::Break;
        }

        self.builder
            .start_node_at(checkpoint, SyntaxKind::TERNARY_EXPR.into());

        self.bump_op();
        self.skip_whitespace_and_newlines();

        if !self.parse_expression_with_precedence(Precedence::ASSIGNMENT) {
            self.error("Expected expression after '?'");
        }

        self.skip_whitespace_and_newlines();
        let colon_found = self
            .peek_non_trivia_token_with_context(LexContext::Operator)
            .map(|(k, _)| k)
            == Some(T![:])
            || self.current_kind() == Some(T![:]);

        if colon_found {
            self.bump_op();
            self.skip_whitespace_and_newlines();
        } else {
            self.error("Expected ':' after true expression in ternary operator");
        }

        if !self.parse_expression_with_precedence(Precedence::ASSIGNMENT) {
            self.error("Expected expression after ':' in ternary operator");
        }

        self.builder.finish_node();
        TernaryOutcome::Handled
    }

    fn handle_binary_operator(
        &mut self,
        current_kind: SyntaxKind,
        checkpoint: Checkpoint,
        min_precedence: Precedence,
    ) -> BinaryOperatorOutcome {
        let Some(operator_state) = self.prepare_binary_operator(current_kind) else {
            return BinaryOperatorOutcome::Break;
        };

        if operator_state.info.precedence < min_precedence {
            return BinaryOperatorOutcome::Break;
        }

        self.builder
            .start_node_at(checkpoint, operator_state.info.node_kind.into());

        let op_checkpoint = self.builder.checkpoint();
        self.bump_op();

        self.handle_compound_assignment(op_checkpoint, operator_state.kind);

        self.skip_whitespace_and_newlines();

        let next_min_precedence = if operator_state.info.right_associative {
            operator_state.info.precedence
        } else {
            Precedence(operator_state.info.precedence.0 + 1)
        };

        let parsed_rhs = self.parse_expression_with_precedence(next_min_precedence);
        if !parsed_rhs {
            if self.allow_trailing_separator(current_kind) {
                self.builder.finish_node();
                return BinaryOperatorOutcome::Break;
            }

            self.error_without_consuming("Expected expression after binary operator");
        }

        self.builder.finish_node();
        BinaryOperatorOutcome::Continue
    }

    fn prepare_binary_operator(&mut self, current_kind: SyntaxKind) -> Option<BinaryOperatorState> {
        if let Some(state) = self.try_prepare_compound_assignment(current_kind) {
            return Some(state);
        }

        get_operator_info(current_kind).map(|info| BinaryOperatorState {
            info,
            kind: BinaryOperatorKind::Standard,
        })
    }

    fn try_prepare_compound_assignment(
        &mut self,
        current_kind: SyntaxKind,
    ) -> Option<BinaryOperatorState> {
        if !current_kind.is_compoundable_operator() {
            return None;
        }

        let is_followed_by_assignment = self
            .peek_nth_non_trivia_token_with_context(LexContext::Operator, 1)
            .is_some_and(|(next_kind, _)| next_kind == T![=]);

        if !is_followed_by_assignment {
            return None;
        }

        Some(BinaryOperatorState {
            info: OperatorInfo::new(Precedence::ASSIGNMENT, true, SyntaxKind::INFIX_EXPR),
            kind: BinaryOperatorKind::CompoundAssignment,
        })
    }

    fn handle_compound_assignment(
        &mut self,
        op_checkpoint: Checkpoint,
        operator_kind: BinaryOperatorKind,
    ) {
        if operator_kind != BinaryOperatorKind::CompoundAssignment {
            return;
        }

        self.builder
            .start_node_at(op_checkpoint, SyntaxKind::COMPOUND_ASSIGNMENT.into());
        self.bump_op();
        self.builder.finish_node();
    }

    fn allow_trailing_separator(&mut self, operator_kind: SyntaxKind) -> bool {
        (operator_kind == T![,] || operator_kind == T![=>])
            && (self.at(T!['}'])
                || self.at(T![;])
                || self.at_end()
                || self.is_at_postfix_modifier_keyword())
    }

    /// Parse primary expression with postfix operations
    fn parse_primary_with_postfix(&mut self) -> PrimaryRole {
        let checkpoint = self.builder.checkpoint();

        let (subject_kind, role) = self.primary_expr();
        if subject_kind == PostfixSubject::None {
            return PrimaryRole::None;
        }

        // Handle postfix operations
        let consumed_postfix =
            self.parse_postfix_operations_with_checkpoint(checkpoint, subject_kind);

        // If postfix operations were consumed, the primary role is no longer relevant
        if consumed_postfix {
            PrimaryRole::Other
        } else {
            role
        }
    }

    pub fn expression_list(&mut self) -> bool {
        let start = self.builder.checkpoint();
        // Parse the first expression but stop before comma-level operators like =>
        if !self.parse_expression_with_precedence(Precedence::LIST_ITEM) {
            return false;
        }

        // If we have comma-separated expressions, wrap them in a single EXPR_LIST node
        if self.at_any(&[T![,], T![=>]]) {
            self.builder
                .start_node_at(start, SyntaxKind::EXPR_LIST.into());

            while self.at_any(&[T![,], T![=>]]) {
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

    fn is_at_postfix_modifier_keyword(&self) -> bool {
        self.at_any(&[
            T![if],
            T![unless],
            T![while],
            T![until],
            T![for],
            T![foreach],
        ])
    }

    fn primary_expr(&mut self) -> (PostfixSubject, PrimaryRole) {
        self.skip_whitespace_and_newlines();

        let Some(current_kind) = self.current_kind_value() else {
            return (PostfixSubject::None, PrimaryRole::None);
        };

        // Treat bare keywords as identifiers when they appear before fat comma (=>)
        // or when they are inside hash braces (for hash keys like $h->{package})
        if current_kind.is_keyword()
            && (self.is_followed_by_fat_comma(0) || self.is_inside_hash_braces())
        {
            self.parse_ident_like_expr();
            return (PostfixSubject::Other, PrimaryRole::Bareword);
        }

        // Track the primary role for bareword identification
        let mut role = PrimaryRole::Other;

        match current_kind {
            SyntaxKind::NUMBER
            | SyntaxKind::STRING
            | SyntaxKind::REGEX_LITERAL
            | SyntaxKind::VERSION
            | SyntaxKind::BARE_VERSION => {
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
                return (PostfixSubject::Variable, PrimaryRole::Variable);
            }
            T!['\\'] => {
                // Reference operator as prefix: \expr
                self.parse_standard_prefix_expr("\\", Precedence::PREFIX, None);
            }
            SyntaxKind::CODE_SIGIL => {
                // Check if this is a complex code reference like &{expr} or &$var
                let next_token = self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1);
                match next_token {
                    Some((T!['{'], _)) => {
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
                        if self.at(T!['}']) {
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
                return (PostfixSubject::Variable, PrimaryRole::Variable);
            }
            T![+] => {
                // Unary plus prefix operator
                self.parse_standard_prefix_expr(
                    "+",
                    Precedence::PREFIX,
                    Some(SyntaxKind::UNARY_PLUS),
                );
            }
            T![-] => {
                // Unary minus prefix operator
                self.parse_standard_prefix_expr(
                    "-",
                    Precedence::PREFIX,
                    Some(SyntaxKind::UNARY_MINUS),
                );
            }
            T![++] => {
                // Prefix increment operator
                self.parse_standard_prefix_expr(
                    "++",
                    Precedence::PREFIX,
                    Some(SyntaxKind::PREFIX_INCREMENT),
                );
            }
            T![--] => {
                // Prefix decrement operator
                self.parse_standard_prefix_expr(
                    "--",
                    Precedence::PREFIX,
                    Some(SyntaxKind::PREFIX_DECREMENT),
                );
            }
            T![!] => {
                // Logical NOT prefix operator
                self.parse_standard_prefix_expr("!", Precedence::PREFIX, None);
            }
            T![~] => {
                // Bitwise NOT prefix operator
                self.parse_standard_prefix_expr("~", Precedence::PREFIX, None);
            }
            T![not] => {
                // NOT keyword prefix operator
                self.parse_standard_prefix_expr("not", Precedence::LOGICAL_NOT_KW, None);
            }
            T![my] | T![our] | T![state] | T![local] => {
                // Variable declaration as prefix operator
                self.parse_var_decl_prefix();
            }
            T![undef] => {
                // undef can be used both as a literal and as a function call
                // Check if it's followed by an expression (function call) or not (literal)
                let next_token = self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1);
                let is_function_call =
                    next_token.is_some_and(|(kind, _)| Self::can_start_expression(kind));

                if is_function_call {
                    // This is a function call: undef $x
                    self.parse_ident_like_expr();
                    role = PrimaryRole::Bareword;
                } else {
                    // This is a literal: undef by itself
                    self.bump_value();
                    self.skip_whitespace_and_newlines();
                }
            }
            T![require] => {
                // require expression (e.g., require v5.14, require local::lib)
                self.require_expr();
            }
            T![try] | T![catch] | T![finally] | SyntaxKind::IDENT | T![::] => {
                self.parse_ident_like_expr();
                role = PrimaryRole::Bareword;
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
            T![x] => {
                // Handle 'x' as an identifier when it appears at the start of expressions
                // This allows expressions like "x => 1" in use statements
                // Consume 'x' as a value in this context
                self.bump_value();
                self.skip_whitespace_and_newlines();
            }
            T!['('] => {
                // Parenthesized expression
                // Inside parens, expect a value
                self.bump_value(); // (
                self.skip_whitespace_and_newlines();

                // List inside parentheses (e.g., array initialization)
                self.parse_parenthesized_list();

                self.skip_whitespace_and_newlines();

                if self.at(T![')']) {
                    // After ')', expect an operator
                    self.bump_op(); // )
                    self.skip_whitespace_and_newlines();
                } else {
                    self.error("Expected ')' to close parenthesized list");
                }

                // Parenthesized expressions (including empty ()) allow [] subscript (list slices)
                return (PostfixSubject::List, PrimaryRole::Other);
            }
            T!['{'] => {
                // In expression context, always treat as hash reference
                self.hash_ref();
            }
            T!['['] => {
                // Array reference (anonymous array): []
                self.array_ref();
            }
            T![qw] => {
                // qw() expression or bareword 'qw'
                if self.should_parse_quote_like() {
                    self.qw_expr();
                    // qw() returns a list, so allow direct array subscripts like qw(...)[0]
                    return (PostfixSubject::List, PrimaryRole::Other);
                } else {
                    self.parse_ident_like_expr();
                    role = PrimaryRole::Bareword;
                }
            }
            T![return] => {
                // return statement (handled as a keyword)
                // After 'return', if an expression follows, it is a value
                self.bump_value(); // consume return
                self.skip_whitespace_and_newlines();

                // If there is an expression after return, process it
                if self.is_at_start_of_expression() {
                    self.expression_list();
                }
            }
            T![next] | T![last] | T![redo] => {
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
                    self.parse_ident_like_expr();
                    role = PrimaryRole::Bareword;
                }
            }
            T![s] | T![tr] | T![y] => {
                if self.should_parse_quote_like() {
                    self.two_part_qlike_expr(current_kind);
                } else {
                    self.parse_ident_like_expr();
                    role = PrimaryRole::Bareword;
                }
            }
            T![sub] => {
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
                return (PostfixSubject::None, PrimaryRole::None);
            }
        }
        (PostfixSubject::Other, role)
    }

    /// Parse anonymous subroutine expression: sub [PROTO]? [:ATTR]* { ... }
    fn anon_sub_expr(&mut self) {
        self.builder.start_node(SyntaxKind::ANON_SUB_EXPR.into());

        // Consume 'sub' keyword
        self.expect(T![sub]);
        self.skip_whitespace_and_newlines();

        // Parse optional prototype, attributes, and required block shared with named subs
        self.parse_sub_tail();

        self.builder.finish_node();
    }

    fn require_expr(&mut self) {
        self.builder.start_node(SyntaxKind::REQUIRE_EXPR.into());

        // "require"
        self.expect(T![require]);
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

        // Parse the variable portion with LIST_ITEM precedence so a trailing comma in contexts
        // like func(my $a,) doesn't get treated as part of the declaration expression.
        let expr_checkpoint = self.builder.checkpoint();

        if !self.parse_expression_with_precedence(Precedence::LIST_ITEM) {
            self.error("Expected expression after variable declaration keyword");
        }

        self.skip_whitespace_and_newlines();

        // Handle optional attribute annotations (e.g., my $x :shared)
        while self.at(T![:]) {
            self.parse_attribute();
            self.skip_whitespace_and_newlines();
        }

        // After attributes, allow an optional assignment or compound assignment inside the
        // declaration node so the tree matches non-attribute declarations.
        let assignment_kind = match self.current_kind() {
            Some(T![=]) => Some(false),
            Some(kind) if kind.is_compoundable_operator() => self
                .peek_nth_non_trivia_token_with_context(LexContext::Operator, 1)
                .filter(|(next_kind, _)| *next_kind == T![=])
                .map(|_| true),
            _ => None,
        };

        if let Some(is_compound_assignment) = assignment_kind {
            self.builder
                .start_node_at(expr_checkpoint, SyntaxKind::INFIX_EXPR.into());

            let op_checkpoint = self.builder.checkpoint();

            self.bump_op();

            if is_compound_assignment {
                self.builder
                    .start_node_at(op_checkpoint, SyntaxKind::COMPOUND_ASSIGNMENT.into());
                self.bump_op();
                self.builder.finish_node();
            }

            self.skip_whitespace_and_newlines();

            if !self.parse_expression_with_precedence(Precedence::ASSIGNMENT) {
                self.error("Expected expression after assignment in variable declaration");
            }

            self.builder.finish_node();
        }

        self.builder.finish_node();
    }
}
