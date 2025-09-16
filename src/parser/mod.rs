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
        self.lexer.peek_nth_non_trivia_with_context(ctx, n)
    }

    /// Check if any of the given token kinds appears next (skipping trivia)
    fn lookahead_for_any(&self, target_kinds: &[SyntaxKind]) -> bool {
        self.lexer.peek_for_any(target_kinds).is_some()
    }

    fn is_at_start_of_expression(&self) -> bool {
        if let Some(kind) = self.current_kind() {
            self.at_any(&[
                SyntaxKind::NUMBER,
                SyntaxKind::STRING,
                SyntaxKind::REGEX_LITERAL,
                SyntaxKind::IO_EXPR,
                SyntaxKind::HEREDOC_START,
                SyntaxKind::IDENT,
                SyntaxKind::L_PAREN,
                SyntaxKind::L_BRACE,
                SyntaxKind::L_BRACKET,
                SyntaxKind::QW_KW,
                SyntaxKind::Q_KW,
                SyntaxKind::QQ_KW,
                SyntaxKind::QX_KW,
                SyntaxKind::M_KW,
                SyntaxKind::QR_KW,
                SyntaxKind::S_KW,
                SyntaxKind::TR_KW,
                SyntaxKind::Y_KW,
                SyntaxKind::MY_KW, // Add variable declaration keywords as start of expression
                SyntaxKind::OUR_KW,
                SyntaxKind::STATE_KW,
                SyntaxKind::LOCAL_KW,
                SyntaxKind::UNDEF_KW,  // undef can appear in expression context
                SyntaxKind::RETURN_KW, // return statements can start expressions
                SyntaxKind::NEXT_KW,   // next can start expressions
                SyntaxKind::LAST_KW,   // last can start expressions
                SyntaxKind::REDO_KW,   // redo can start expressions
                SyntaxKind::SUB_KW,    // anonymous subroutines in expression context
                SyntaxKind::PLUS,      // unary plus operator
                SyntaxKind::MINUS,     // unary minus operator
                SyntaxKind::INCREMENT, // prefix increment operator
                SyntaxKind::DECREMENT, // prefix decrement operator
                SyntaxKind::LOGICAL_NOT, // prefix logical NOT operator
                SyntaxKind::BITWISE_NOT, // prefix bitwise NOT operator
                SyntaxKind::NOT_KW,    // prefix NOT keyword operator
                SyntaxKind::FILE_TEST_OP, // file test operators
                SyntaxKind::X,         // x can start expressions like "x => 1" in use statements
            ]) || kind.is_variable()
                || kind.is_sigil()
        } else {
            false
        }
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
    fn test_lexer_lookahead_functionality() {
        // Test the lexer's new lookahead methods
        let mut lexer = crate::lexer::Lexer::new("$var\n@array");

        // Test peek_non_trivia_token
        assert_eq!(
            lexer.peek_non_trivia_token(),
            Some((SyntaxKind::DOLLAR, "$"))
        );

        // Consume first token and test again
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(
            lexer.peek_non_trivia_token(),
            Some((SyntaxKind::IDENT, "var"))
        );

        // Consume identifier
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "var")));

        // Skip newline and test peek again
        assert_eq!(lexer.next_token(), Some((SyntaxKind::NEWLINE, "\n")));
        assert_eq!(lexer.peek_non_trivia_token(), Some((SyntaxKind::AT, "@")));
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
            stmt.descendants().any(|node| node.kind() == SyntaxKind::EXPR_LIST),
            "Expected EXPR_LIST node for return value list"
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
