use crate::{
    lexer::{LexContext, Lexer},
    SyntaxKind,
};
use miette::{Diagnostic, SourceSpan};
use rowan::{GreenNode, GreenNodeBuilder, TextRange};

#[derive(Debug, Clone, thiserror::Error, Diagnostic)]
#[error("Parse error: {message}")]
pub struct ParseError {
    pub message: String,
    pub range: TextRange,
    #[source_code]
    pub source_code: String,
    #[label("here")]
    pub span: SourceSpan,
}

impl ParseError {
    #[must_use]
    pub fn new(message: String, range: TextRange, source_code: &str) -> Self {
        let span = SourceSpan::new(usize::from(range.start()).into(), usize::from(range.len()));
        Self {
            message,
            range,
            source_code: source_code.to_string(),
            span,
        }
    }
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
    current_pos: usize,
    source: &'a str,
}

impl<'a> Parser<'a> {
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        let lexer = Lexer::new(input);

        Self {
            lexer,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            current_pos: 0,
            source: input,
        }
    }

    #[must_use]
    pub fn parse(input: &str) -> (GreenNode, Vec<ParseError>) {
        let mut parser = Parser::new(input);
        parser.root();
        let green_node = parser.builder.finish();
        (green_node, parser.errors)
    }

    fn root(&mut self) {
        self.builder
            .start_node(rowan::SyntaxKind(SyntaxKind::ROOT as u16));

        self.skip_whitespace_and_newlines();
        while !self.at_end() {
            // Check if we've encountered a data section keyword
            if matches!(
                self.current_kind(),
                Some(SyntaxKind::END_KW | SyntaxKind::DATA_KW)
            ) {
                self.data_section();
                // After data section, everything else is consumed as part of it
                break;
            }

            // Check for POD commands at the top level
            if self.at(SyntaxKind::POD_CONTENT) {
                self.pod_block();
            } else if self.at(SyntaxKind::CUT_KW) {
                // =cut without preceding POD
                self.error("Found =cut without a preceding POD command");
                self.bump(); // Consume the =cut token
            } else if !self.statement() {
                self.error("Expected a statement, but found an unexpected token.");
            }
            self.skip_whitespace_and_newlines();
        }

        self.builder.finish_node();
    }

    /// __END__ または __DATA__ のデータセクションをパースする
    fn data_section(&mut self) {
        self.builder.start_node(SyntaxKind::DATA_SECTION.into());

        // Consume the __END__ or __DATA__ keyword
        self.bump();

        // Use lexer's consume_data_section to get the remaining content
        if let Some((syntax_kind, text)) = self.lexer.consume_data_section() {
            self.builder.token(syntax_kind.into(), text);
        }
        // If there's no remaining content, that's also valid (empty data section)

        self.builder.finish_node();
    }

    fn pod_block(&mut self) {
        self.builder.start_node(SyntaxKind::POD_BLOCK.into());

        // Consume the entire POD content (lexer already consumed the whole block)
        self.bump();

        self.builder.finish_node();
    }

    // Helper methods
    fn current_kind(&self) -> Option<SyntaxKind> {
        self.lexer.peek_token().map(|(k, _)| k)
    }

    fn current_text(&self) -> Option<&'a str> {
        self.lexer.peek_token().map(|(_, t)| t)
    }

    // Value-context peek helpers for expression starts (parser-driven lexing)
    fn current_kind_value(&self) -> Option<SyntaxKind> {
        self.peek_non_trivia_token_with_context(LexContext::Value)
            .map(|(k, _)| k)
    }

    fn current_text_value(&self) -> Option<&'a str> {
        self.peek_non_trivia_token_with_context(LexContext::Value)
            .map(|(_, t)| t)
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.current_kind() == Some(kind)
    }

    fn at_any(&self, kinds: &[SyntaxKind]) -> bool {
        if let Some(current) = self.current_kind() {
            kinds.contains(&current)
        } else {
            false
        }
    }

    fn at_end(&self) -> bool {
        self.lexer.peek_token().is_none()
    }

    fn bump(&mut self) {
        if let Some((kind, text)) = self.lexer.next_token() {
            self.builder.token(kind.into(), text);
            self.current_pos += text.len();
        } else {
            // No token to consume; possibly at end of input
            self.error("Unexpected end of input");
        }
    }

    /// Try to consume a digit-prefixed identifier and add it as an IDENT token
    fn try_bump_digit_prefixed_ident(&mut self) -> bool {
        if let Some((kind, text)) = self.lexer.consume_digit_prefixed_ident() {
            self.builder.token(kind.into(), text);
            self.current_pos += text.len();
            true
        } else {
            false
        }
    }

    /// Consume current token and fetch next using an explicit lexical context
    fn bump_with_context(&mut self, context: LexContext) {
        if let Some((kind, text)) = self.lexer.next_token_with_context(context) {
            self.builder.token(kind.into(), text);
            self.current_pos += text.len();
        } else {
            // No token to consume; possibly at end of input
            self.error("Unexpected end of input");
        }
    }

    /// Convenience: after consuming current token, expect a Value next
    fn bump_value(&mut self) {
        self.bump_with_context(LexContext::Value);
    }

    /// Convenience: after consuming current token, expect an Operator next
    fn bump_op(&mut self) {
        self.bump_with_context(LexContext::Operator);
    }

    fn bump_as(&mut self, syntax_kind: SyntaxKind) {
        if let Some((_, text)) = self.lexer.next_token() {
            self.builder.token(syntax_kind.into(), text);
            self.current_pos += text.len();
        }
    }

    fn bump_op_as(&mut self, syntax_kind: SyntaxKind) {
        if let Some((_, text)) = self.lexer.next_token_with_context(LexContext::Operator) {
            self.builder.token(syntax_kind.into(), text);
            self.current_pos += text.len();
        }
    }

    fn expect(&mut self, expected: SyntaxKind) {
        if self.at(expected) {
            self.bump();
        } else {
            let msg = format!("Expected {:?}, found {:?}", expected, self.current_kind());
            self.error(&msg);
        }
    }

    /// Expect a token and consume it, specifying the lexical context for the next token
    fn expect_with_context(&mut self, expected: SyntaxKind, context: LexContext) {
        if self.at(expected) {
            self.bump_with_context(context);
        } else {
            let msg = format!("Expected {:?}, found {:?}", expected, self.current_kind());
            self.error(&msg);
        }
    }

    /// Convenience: expect a token and treat the next lex as a Value
    fn expect_value(&mut self, expected: SyntaxKind) {
        self.expect_with_context(expected, LexContext::Value);
    }

    /// Convenience: expect a token and treat the next lex as an Operator
    fn expect_op(&mut self, expected: SyntaxKind) {
        self.expect_with_context(expected, LexContext::Operator);
    }

    fn skip_whitespace(&mut self) {
        while let Some((kind, _)) = self.lexer.peek_token() {
            match kind {
                SyntaxKind::WHITESPACE
                | SyntaxKind::COMMENT
                | SyntaxKind::HEREDOC_CONTENT
                | SyntaxKind::HEREDOC_END => {
                    if let Some((k, t)) = self.lexer.next_token() {
                        self.builder.token(k.into(), t);
                        self.current_pos += t.len();
                    }
                }
                _ => break,
            }
        }
    }

    fn skip_newlines(&mut self) {
        while let Some((kind, _)) = self.lexer.peek_token() {
            if kind == SyntaxKind::NEWLINE {
                if let Some((k, t)) = self.lexer.next_token() {
                    self.builder.token(k.into(), t);
                    self.current_pos += t.len();
                }
            } else {
                break;
            }
        }
    }

    fn skip_whitespace_and_newlines(&mut self) {
        loop {
            let before = self.current_pos;
            self.skip_whitespace();
            self.skip_newlines();
            if self.current_pos == before {
                break;
            }
        }
    }

    fn error(&mut self, message: &str) {
        let text_len = self.current_text().map_or(0, str::len);
        let range = TextRange::new(
            (self.current_pos as u32).into(),
            ((self.current_pos + text_len) as u32).into(),
        );

        self.errors
            .push(ParseError::new(message.to_string(), range, self.source));

        // Create error token by consuming one token (if any)
        if let Some((_, text)) = self.lexer.next_token() {
            self.builder.token(SyntaxKind::ERROR.into(), text);
            self.current_pos += text.len();
        }
    }

    /// 括弧内のカンマ区切り式をパースするヘルパー関数
    fn parse_parenthesized_list(&mut self) {
        if !self.at(SyntaxKind::R_PAREN) {
            self.expression_list();
        }
    }

    /// Peek at the next non-trivia token with an explicit lexical context
    /// This is used to disambiguate contexts like operator lookahead.
    fn peek_non_trivia_token_with_context(&self, ctx: LexContext) -> Option<(SyntaxKind, &'a str)> {
        self.lexer.peek_non_trivia_with_context(ctx)
    }

    fn peek_nth_non_trivia_token_with_context(
        &self,
        ctx: LexContext,
        n: usize,
    ) -> Option<(SyntaxKind, &'a str)> {
        let mut cloned = self.lexer.clone();

        // If the current token is a quote-like keyword immediately followed by '#',
        // configure the cloned lexer for quote-like parsing to handle the delimiter correctly.
        let (current_token, next_char) = self.lexer.peek_token_and_next_char();
        if let (Some(current_kind), Some('#')) = (current_token, next_char) {
            if current_kind.is_quote_like_keyword() {
                let mode = crate::lexer::QuoteLikeMode::from_keyword(current_kind);
                cloned.begin_quote_like(current_kind, mode);
            }
        }

        cloned.peek_nth_non_trivia_with_context(ctx, n)
    }

    /// Returns true if the token at `offset` is followed by a fat comma (`=>`).
    fn is_followed_by_fat_comma(&self, offset: usize) -> bool {
        self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset + 1)
            .is_some_and(|(next_kind, _)| next_kind == SyntaxKind::FAT_COMMA)
    }

    /// Check if any of the given token kinds appears next (skipping trivia)
    fn lookahead_for_any(&self, target_kinds: &[SyntaxKind]) -> bool {
        self.lexer.peek_for_any(target_kinds).is_some()
    }

    fn is_at_start_of_expression(&self) -> bool {
        self.current_kind_value().is_some_and(|kind| {
            Self::can_start_expression(kind)
                || (kind.is_keyword() && self.is_followed_by_fat_comma(0))
        })
    }

    fn can_start_expression(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::BACKTICK_STRING
                | SyntaxKind::REGEX_LITERAL
                | SyntaxKind::SLASH
                | SyntaxKind::IO_EXPR
                | SyntaxKind::HEREDOC_START
                | SyntaxKind::IDENT
                | SyntaxKind::L_PAREN
                | SyntaxKind::L_BRACE
                | SyntaxKind::L_BRACKET
                | SyntaxKind::QW_KW
                | SyntaxKind::Q_KW
                | SyntaxKind::QQ_KW
                | SyntaxKind::QX_KW
                | SyntaxKind::M_KW
                | SyntaxKind::QR_KW
                | SyntaxKind::S_KW
                | SyntaxKind::TR_KW
                | SyntaxKind::Y_KW
                | SyntaxKind::MY_KW
                | SyntaxKind::OUR_KW
                | SyntaxKind::STATE_KW
                | SyntaxKind::LOCAL_KW
                | SyntaxKind::UNDEF_KW
                | SyntaxKind::REQUIRE_KW
                | SyntaxKind::RETURN_KW
                | SyntaxKind::NEXT_KW
                | SyntaxKind::LAST_KW
                | SyntaxKind::REDO_KW
                | SyntaxKind::SUB_KW
                | SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::UNARY_PLUS
                | SyntaxKind::UNARY_MINUS
                | SyntaxKind::INCREMENT
                | SyntaxKind::DECREMENT
                | SyntaxKind::PREFIX_INCREMENT
                | SyntaxKind::PREFIX_DECREMENT
                | SyntaxKind::LOGICAL_NOT
                | SyntaxKind::BITWISE_NOT
                | SyntaxKind::NOT_KW
                | SyntaxKind::FILE_TEST_OP
                | SyntaxKind::X
                | SyntaxKind::CODE_SIGIL
        ) || kind.is_variable()
            || kind.is_sigil()
    }
}

#[must_use]
pub fn parse(input: &str) -> (GreenNode, Vec<ParseError>) {
    Parser::parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PerlNode;

    #[test]
    fn test_error_recovery_no_infinite_loop() {
        // Confirm that error recovery does not cause an infinite loop
        let input = "my = @ % ^ invalid tokens here;";
        let (green, errors) = parse(input);

        // Errors occur, but parsing completes
        assert!(!errors.is_empty(), "Should have parse errors");

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // Confirm that the AST has some structure (evidence that there is no infinite loop)
        assert!(
            syntax.children().count() > 0,
            "Should have some parsed structure"
        );
    }

    #[test]
    fn test_pod_parsing() {
        let test_cases = [
            ("=pod\nContent\n=cut\n", true),
            ("=head1 TITLE\nContent\n=cut\n", true),
            ("my $var;\n=pod\nContent\n=cut\nmy $other;\n", true),
            (
                "=pod\nContent without cut",
                true, // POD at EOF is valid
            ),
        ];

        for (input, should_succeed) in test_cases {
            let (green, errors) = parse(input);
            let syntax = PerlNode::new_root(green);

            // All inputs should parse structurally
            assert_eq!(
                syntax.kind(),
                SyntaxKind::ROOT,
                "Failed to parse: '{}'",
                input
            );

            if should_succeed {
                assert!(
                    errors.is_empty(),
                    "Should parse '{}' without errors, but got: {:?}",
                    input,
                    errors
                );
            } else {
                assert!(
                    !errors.is_empty(),
                    "Should generate parse error for '{}' but didn't",
                    input
                );
            }
        }
    }

    #[test]
    fn test_cut_without_pod_error() {
        let input = "=cut\n";
        let (green, errors) = parse(input);
        let syntax = PerlNode::new_root(green);

        // Should parse structurally but with errors
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
        assert!(
            !errors.is_empty(),
            "Should generate error for =cut without POD"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("=cut") && e.message.contains("POD")),
            "Error should mention =cut and POD, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_regex_literal_with_slash_in_char_class() {
        let input = "$foo =~ /[a/]/;";
        let (_green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected no parse errors, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_regex_literal_with_literal_closing_bracket() {
        let input = "$foo =~ /[]/]/;";
        let (_green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected no parse errors, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_regex_literal_with_newline() {
        let input = "$foo =~ /foo\nbar/x;";
        let (_green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected no parse errors, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_substitution_with_escaped_delimiter() {
        let input = "s/\\//::/g;";
        let (_green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected no parse errors, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_quote_like_with_nested_delimiters() {
        let input = "m<.<a>.>;";
        let (_green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected no parse errors, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_qw_nested_delimiters() {
        let input = "my @list = qw(a (b) c);";
        let (_green, errors) = parse(input);

        assert!(
            errors.is_empty(),
            "Expected no parse errors, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_lexer_lookahead_functionality() {
        // Test the lexer's new lookahead methods
        let mut lexer = crate::lexer::Lexer::new("$var\n@array");

        // Test peek_non_trivia_token
        assert_eq!(
            lexer.peek_non_trivia_token(),
            Some((SyntaxKind::SCALAR_SIGIL, "$"))
        );

        // Consume first token and test again
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SCALAR_SIGIL, "$")));
        assert_eq!(
            lexer.peek_non_trivia_token(),
            Some((SyntaxKind::IDENT, "var"))
        );

        // Consume identifier
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "var")));

        // Skip newline and test peek again
        assert_eq!(lexer.next_token(), Some((SyntaxKind::NEWLINE, "\n")));
        assert_eq!(
            lexer.peek_non_trivia_token(),
            Some((SyntaxKind::ARRAY_SIGIL, "@"))
        );
    }

    #[test]
    fn test_debug_q_parsing() {
        use crate::PerlNode;

        let input = "q(hello)";
        println!("Testing input: {}", input);
        let (green, errors) = parse(input);

        println!("Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        println!("AST: {:#?}", syntax);

        // Even if there are errors, check if we got some structure
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
    }

    #[test]
    fn test_debug_print_q_parsing() {
        use crate::PerlNode;

        let input = "print q(hello);";
        println!("Testing input: {}", input);
        let (green, errors) = parse(input);

        println!("Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        println!("AST: {:#?}", syntax);

        // Even if there are errors, check if we got some structure
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
    }

    #[test]
    fn test_debug_qq_hash_parsing() {
        use crate::PerlNode;

        let input = "qq#hash $var#";
        println!("Testing input: {}", input);
        let (green, errors) = parse(input);

        println!("Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        println!("AST: {:#?}", syntax);

        // Even if there are errors, check if we got some structure
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
    }

    #[test]
    fn test_dereferencing_pattern_detection() {
        // Test that is_dereferencing_pattern works with token-based lookahead

        // Test valid dereferencing patterns
        let test_cases = [
            ("@$ref", true),
            ("%$ref", true),
            ("$$ref", true),
            ("@ $ref", true), // with whitespace
            ("% $ref", true), // with whitespace
            ("$ $ref", true), // with whitespace
        ];

        for (input, expected) in test_cases {
            let mut parser = crate::parser::Parser::new(input);
            parser.skip_whitespace_and_newlines();
            assert_eq!(
                parser.is_dereferencing_pattern(),
                expected,
                "Failed for input: '{}'",
                input
            );
        }

        // Test expression dereferencing patterns (new functionality)
        let expr_deref_cases = [
            ("@{$ref}", true),
            ("%{$ref}", true),
            ("${$ref}", true),
            ("@{ $ref }", true), // with whitespace
            ("@{func()}", true),
        ];
        for (input, expected) in expr_deref_cases {
            let mut parser = crate::parser::Parser::new(input);
            parser.skip_whitespace_and_newlines();
            assert_eq!(
                parser.is_dereferencing_pattern(),
                expected,
                "Failed for input: '{}'",
                input
            );
        }

        // Test non-dereferencing patterns
        let non_deref_cases = [("@array", false), ("%hash", false), ("$scalar", false)];

        for (input, expected) in non_deref_cases {
            let mut parser = crate::parser::Parser::new(input);
            parser.skip_whitespace_and_newlines();
            assert_eq!(
                parser.is_dereferencing_pattern(),
                expected,
                "Failed for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_fat_comma_chain_expr_list() {
        let input = "(a=>b=>1)";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);
        let root = PerlNode::new_root(green);
        let stmt = root.children().next().expect("missing stmt");
        assert_eq!(stmt.kind(), SyntaxKind::STMT);
        assert!(
            stmt.children()
                .any(|child| child.kind() == SyntaxKind::EXPR_LIST),
            "Expected EXPR_LIST node inside parentheses"
        );
    }

    #[test]
    fn test_return_multiple_values_expression_list() {
        let input = "return 1, 2;";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let root = PerlNode::new_root(green);
        let stmt = root.children().next().expect("missing stmt");
        assert_eq!(stmt.kind(), SyntaxKind::STMT);
        assert!(
            stmt.descendants()
                .any(|node| node.kind() == SyntaxKind::EXPR_LIST),
            "Expected EXPR_LIST node for return value list"
        );
    }

    #[test]
    fn test_block_function_accepts_expression_argument() {
        let input = "grep +$_, @list;";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let root = PerlNode::new_root(green);
        let stmt = root
            .children()
            .next()
            .expect("missing statement for grep call");
        assert_eq!(stmt.kind(), SyntaxKind::STMT);

        let mut found_call = false;
        for child in stmt.children() {
            if child.kind() == SyntaxKind::FUNCTION_CALL_EXPR {
                found_call = true;
                // Ensure we parsed arguments as an expression list, not an infix expression
                assert!(
                    child
                        .children()
                        .any(|node| node.kind() == SyntaxKind::EXPR_LIST),
                    "expected expression list inside function call"
                );
            }
            assert_ne!(child.kind(), SyntaxKind::INFIX_EXPR);
        }

        assert!(found_call, "expected to find function call for grep");
    }

    #[test]
    fn test_defined_or_after_shift_pop_and_file_test() {
        let cases = ["shift // 1;", "pop // 1;", "-f // 0;"];

        for input in cases {
            let (green, errors) = parse(input);
            assert!(
                errors.is_empty(),
                "Parse errors for '{}': {:?}",
                input,
                errors
            );

            let root = PerlNode::new_root(green);
            assert!(
                root.descendants_with_tokens().any(|element| {
                    matches!(
                        element,
                        rowan::NodeOrToken::Token(token)
                            if token.kind() == SyntaxKind::DEFINED_OR
                    )
                }),
                "expected DEFINED_OR token for '{}'",
                input
            );
        }
    }

    #[test]
    fn test_say_with_qualified_method_call() {
        let input = "say Foo::Bar->method();";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let root = PerlNode::new_root(green);
        let stmt = root.children().next().expect("missing statement");
        assert_eq!(stmt.kind(), SyntaxKind::STMT);

        assert!(
            stmt.descendants()
                .any(|node| node.kind() == SyntaxKind::METHOD_CALL_EXPR),
            "expected METHOD_CALL_EXPR inside say arguments"
        );
    }

    #[test]
    fn test_bare_block_statement() {
        let input = "warn 1; { warn 2 } warn 3;";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let root = PerlNode::new_root(green);
        let mut stmts = root.children();

        let first = stmts.next().expect("missing first statement");
        assert_eq!(first.kind(), SyntaxKind::STMT);

        let block_stmt = stmts.next().expect("missing block statement");
        assert_eq!(block_stmt.kind(), SyntaxKind::STMT);
        assert!(
            block_stmt
                .children()
                .any(|child| child.kind() == SyntaxKind::BLOCK_STMT),
            "expected block stmt inside bare block"
        );

        let third = stmts.next().expect("missing trailing statement");
        assert_eq!(third.kind(), SyntaxKind::STMT);
    }

    #[test]
    fn test_hashref_expression_after_unary_plus() {
        let input = "my $hash = +{a => 1};";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let root = PerlNode::new_root(green);
        let stmt = root.children().next().expect("missing statement");
        assert_eq!(stmt.kind(), SyntaxKind::STMT);
        assert!(
            stmt.descendants()
                .any(|node| node.kind() == SyntaxKind::VAR_DECL),
            "expected declaration inside statement"
        );
        assert!(
            stmt.descendants()
                .any(|node| node.kind() == SyntaxKind::HASH_REF),
            "expected hash ref inside declaration"
        );
    }

    #[test]
    fn test_hashref_access_after_unary_plus_with_newline() {
        let input = "+{}\n->{key}";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let root = PerlNode::new_root(green);
        let stmt = root.children().next().expect("missing statement");
        assert_eq!(stmt.kind(), SyntaxKind::STMT);
        assert!(
            stmt.descendants()
                .any(|node| node.kind() == SyntaxKind::HASH_REF_ACCESS_EXPR),
            "expected HASH_REF_ACCESS_EXPR when '->' follows a newline"
        );
    }

    #[test]
    fn test_hashref_keywords_as_keys() {
        let input = "my $hash = +{ package => 1, and => 2 };";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let root = PerlNode::new_root(green);
        let stmt = root.children().next().expect("missing statement");
        assert_eq!(stmt.kind(), SyntaxKind::STMT);

        assert!(
            stmt.descendants()
                .any(|node| node.kind() == SyntaxKind::VAR_DECL),
            "expected declaration inside statement"
        );

        let mut saw_package = false;
        let mut saw_and = false;
        for element in stmt.descendants_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = element {
                if token.kind() == SyntaxKind::IDENT {
                    match token.text() {
                        "package" => saw_package = true,
                        "and" => saw_and = true,
                        _ => {}
                    }
                }
            }
        }

        assert!(
            saw_package,
            "expected to see 'package' coerced to IDENT inside hash"
        );
        assert!(
            saw_and,
            "expected to see 'and' coerced to IDENT inside hash"
        );
    }

    #[test]
    fn test_parenthesized_list_with_trailing_space() {
        let input = "(a => [] )";
        let (_green, errors) = parse(input);
        assert!(
            errors.is_empty(),
            "Parse errors for '(a => [] )' A: {:?}",
            errors
        );

        let input_no_space = "(a => [])";
        let (_green_no_space, errors_no_space) = parse(input_no_space);
        assert!(
            errors_no_space.is_empty(),
            "Parse errors for '(a => [])' B: {:?}",
            errors_no_space
        );
    }
}

mod expression;
mod statement;
#[test]
fn test_sub_with_quote_like_name() {
    use crate::PerlNode;
    let input = "sub tr {}";
    let (green, errors) = parse(input);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors);
    let syntax = PerlNode::new_root(green);
    assert_eq!(syntax.kind(), SyntaxKind::ROOT);
}

#[test]
fn test_package_with_quote_like_name() {
    use crate::PerlNode;
    let input = "package tr;";
    let (green, errors) = parse(input);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors);
    let syntax = PerlNode::new_root(green);
    assert_eq!(syntax.kind(), SyntaxKind::ROOT);
}

#[test]
fn test_package_with_digits_after_colons() {
    use crate::PerlNode;

    let test_cases = [
        "package Foo::123;",
        "package Bar::456::Test;",
        "package Module::2024::Version;",
        "package A::1::B::2::C::3;",
        "package Foo::123 { my $x = 1; }",
        "package Bar::456 1.0;",
        // Mixed digit-letter identifiers
        "package Foo::123ABC;",
        "package Bar::456DEF::Test;",
        "package Module::123abc456;",
        "package Test::999XYZ789;",
        "package Foo::123ABC { my $x = 1; }",
        "package Bar::456DEF v1.0;",
        "package A::123ABC::B::456DEF;",
    ];

    for input in test_cases {
        let (green, errors) = parse(input);
        assert!(
            errors.is_empty(),
            "Parse errors for '{}': {:?}",
            input,
            errors
        );

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // Check that we have a package statement
        let package = syntax
            .children()
            .find(|n| n.kind() == SyntaxKind::PACKAGE_STMT)
            .expect(&format!("missing package statement in '{}'", input));

        // Verify the package statement contains a qualified identifier
        assert!(
            package
                .descendants()
                .any(|n| n.kind() == SyntaxKind::QUALIFIED_IDENT),
            "package statement should contain qualified identifier in '{}'",
            input
        );
    }
}

#[test]
fn test_digit_prefixed_ident_lexer() {
    let mut lexer = crate::lexer::Lexer::new("123ABC");
    let result = lexer.consume_digit_prefixed_ident();
    assert_eq!(result, Some((SyntaxKind::IDENT, "123ABC")));

    let mut lexer2 = crate::lexer::Lexer::new("456");
    let result2 = lexer2.consume_digit_prefixed_ident();
    assert_eq!(result2, Some((SyntaxKind::IDENT, "456")));

    let mut lexer3 = crate::lexer::Lexer::new("789XYZ123");
    let result3 = lexer3.consume_digit_prefixed_ident();
    assert_eq!(result3, Some((SyntaxKind::IDENT, "789XYZ123")));
}

#[test]
fn test_package_with_block() {
    use crate::PerlNode;
    let input = "package Foo::Bar { my $x = 1; }";
    let (green, errors) = parse(input);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors);
    let syntax = PerlNode::new_root(green);
    let package = syntax
        .children()
        .find(|n| n.kind() == SyntaxKind::PACKAGE_STMT)
        .expect("missing package statement");
    assert!(
        package
            .children()
            .any(|n| n.kind() == SyntaxKind::BLOCK_STMT),
        "package statement should contain block"
    );
}

#[test]
fn test_infix_expression_with_newline() {
    use crate::PerlNode;

    // Test cases that should all parse as valid infix expressions
    let test_cases = [
        ("[] | 1", "array ref with space"),
        ("[]\n| 1", "array ref with newline"),
        ("[] \n | 1", "array ref with space and newline"),
        ("[1,2]\n& 3", "array with elements and newline"),
        ("[]\n+ 2", "array ref with newline and plus"),
        ("[1]\n* 5", "array ref with newline and multiply"),
    ];

    for (input, description) in test_cases {
        let (green, errors) = parse(input);
        assert!(
            errors.is_empty(),
            "Parse errors in {}: {:?}",
            description,
            errors
        );

        let syntax = PerlNode::new_root(green);
        // Should contain exactly one statement with an infix expression
        let stmts: Vec<_> = syntax
            .children()
            .filter(|n| n.kind() == SyntaxKind::STMT)
            .collect();
        assert_eq!(
            stmts.len(),
            1,
            "Expected exactly one statement in {}",
            description
        );

        let has_infix = stmts[0]
            .descendants()
            .any(|n| n.kind() == SyntaxKind::INFIX_EXPR);
        assert!(has_infix, "Expected infix expression in {}", description);
    }
}

#[test]
fn test_quote_like_hash_delimiter_lookahead() {
    use crate::PerlNode;

    // Test quote-like expressions with # delimiter
    let quote_like_cases = [
        ("{ (qr#x#)\n}", "QR_EXPR"),
        ("{s#x#y#\n}", "S_EXPR"),
        ("{ (m#pattern#) }", "M_EXPR"),
        ("{tr#a#b#}", "TR_EXPR"),
        ("{y#a#b#}", "TR_EXPR"), // y uses same TR_EXPR as tr
    ];

    for (input, expected_expr) in quote_like_cases {
        let (green, errors) = parse(input);
        println!("Testing quote-like: {}", input);

        assert!(
            errors.is_empty(),
            "Parse errors for '{}': {:?}",
            input,
            errors
        );

        let syntax = PerlNode::new_root(green);
        let expected_kind = match expected_expr {
            "QR_EXPR" => SyntaxKind::QR_EXPR,
            "S_EXPR" => SyntaxKind::S_EXPR,
            "M_EXPR" => SyntaxKind::M_EXPR,
            "TR_EXPR" => SyntaxKind::TR_EXPR,
            _ => panic!("Unknown expected expr: {}", expected_expr),
        };

        assert!(
            syntax
                .descendants()
                .any(|node| node.kind() == expected_kind),
            "Expected {} in '{}', but AST was: {:#?}",
            expected_expr,
            input,
            syntax
        );
    }

    // Test bareword cases (should parse as hash)
    let bareword_cases = ["{ qr => 1 }", "{ s => 2 }"];

    for input in bareword_cases {
        let (green, errors) = parse(input);
        println!("Testing bareword: {}", input);

        assert!(
            errors.is_empty(),
            "Parse errors for bareword '{}': {:?}",
            input,
            errors
        );

        let syntax = PerlNode::new_root(green);
        assert!(
            syntax
                .descendants()
                .any(|node| node.kind() == SyntaxKind::HASH_REF),
            "Expected HASH_REF for bareword '{}', but AST was: {:#?}",
            input,
            syntax
        );
    }
}
