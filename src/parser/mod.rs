use crate::{lexer::Lexer, SyntaxKind};
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
    current_token: Option<(SyntaxKind, &'a str)>,
    current_pos: usize,
    source: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Lexer::new(input);
        let current_token = lexer.next_token();

        Self {
            lexer,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            current_token,
            current_pos: 0,
            source: input,
        }
    }

    pub fn parse(input: &str) -> (GreenNode, Vec<ParseError>) {
        let mut parser = Parser::new(input);
        parser.root();
        let green_node = parser.builder.finish();
        (green_node, parser.errors)
    }

    fn root(&mut self) {
        self.builder
            .start_node(rowan::SyntaxKind(SyntaxKind::ROOT as u16));

        self.skip_trivia();
        while !self.at_end() {
            // Check if we've encountered a data section keyword
            if matches!(
                self.current_kind(),
                Some(SyntaxKind::END_KW) | Some(SyntaxKind::DATA_KW)
            ) {
                self.data_section();
                // After data section, everything else is consumed as part of it
                break;
            }

            // Check for POD commands at the top level
            if matches!(
                self.current_kind(),
                Some(SyntaxKind::POD_COMMAND) | Some(SyntaxKind::CUT_KW)
            ) {
                if self.current_kind() == Some(SyntaxKind::POD_COMMAND) {
                    self.pod_block();
                } else {
                    // =cut without preceding POD
                    self.error("Found =cut without a preceding POD command");
                }
            } else if !self.statement() {
                self.error("Expected a statement, but found an unexpected token.");
            }
            self.skip_trivia();
        }

        self.builder.finish_node();
    }

    /// __END__ または __DATA__ のデータセクションをパースする
    fn data_section(&mut self) {
        self.builder.start_node(SyntaxKind::DATA_SECTION.into());

        // Consume the __END__ or __DATA__ keyword
        self.bump();

        if self.at(SyntaxKind::RAW_STRING) || self.at_end() {
            self.bump()
        } else {
            self.error("Expected raw string after data section keyword");
        }

        self.builder.finish_node();
    }

    fn pod_block(&mut self) {
        self.builder.start_node(SyntaxKind::POD_BLOCK.into());

        // Consume the POD command (=pod, =head1, etc.)
        self.bump();

        // Consume any POD content until =cut
        while !self.at_end() && !self.at(SyntaxKind::CUT_KW) {
            if self.at(SyntaxKind::POD_CONTENT) {
                self.bump();
            } else {
                // This shouldn't happen in POD mode, but handle gracefully
                break;
            }
        }

        // Consume the =cut if present (or handle EOF gracefully)
        if self.at(SyntaxKind::CUT_KW) {
            self.bump();
        }

        self.builder.finish_node();
    }

    // Helper methods
    fn current_kind(&self) -> Option<SyntaxKind> {
        self.current_token.map(|(kind, _)| kind)
    }

    fn current_text(&self) -> Option<&'a str> {
        self.current_token.map(|(_, text)| text)
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
        self.current_token.is_none()
    }

    fn bump(&mut self) {
        if let Some((kind, text)) = self.current_token.take() {
            self.builder.token(kind.into(), text);
            self.current_pos += text.len();
        }
        self.current_token = self.lexer.next_token();
    }

    fn expect(&mut self, expected: SyntaxKind) {
        if self.at(expected) {
            self.bump();
        } else {
            let msg = format!("Expected {:?}, found {:?}", expected, self.current_kind());
            self.error(&msg);
        }
    }

    fn skip_trivia(&mut self) {
        while let Some(kind) = self.current_kind() {
            if kind.is_trivia() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn error(&mut self, message: &str) {
        let text_len = self.current_text().map_or(0, |t| t.len());
        let range = TextRange::new(
            (self.current_pos as u32).into(),
            ((self.current_pos + text_len) as u32).into(),
        );

        self.errors
            .push(ParseError::new(message.to_string(), range, self.source));

        // Create error token
        if let Some((_, text)) = self.current_token.take() {
            self.builder.token(SyntaxKind::ERROR.into(), text);
            self.current_pos += text.len();
        }
        self.current_token = self.lexer.next_token();
    }

    /// 括弧内のカンマ区切り式をパースするヘルパー関数
    fn parse_parenthesized_list(&mut self) {
        if !self.at(SyntaxKind::R_PAREN) {
            self.expression_list();
        }
    }

    /// Peek at the next non-trivia token without consuming it
    fn peek_non_trivia_token(&self) -> Option<(SyntaxKind, &'a str)> {
        self.lexer.peek_non_trivia_token()
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
                SyntaxKind::RETURN_KW, // return statements can start expressions
                SyntaxKind::LOGICAL_NOT, // prefix logical NOT operator
                SyntaxKind::NOT_KW,    // prefix NOT keyword operator
            ]) || kind.is_variable()
                || kind.is_sigil()
        } else {
            false
        }
    }
}

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
        let mut lexer = crate::lexer::Lexer::new("$var @array");

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

        // Skip whitespace and test peek again
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.peek_non_trivia_token(), Some((SyntaxKind::AT, "@")));
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
            parser.skip_trivia();
            assert_eq!(
                parser.is_dereferencing_pattern(),
                expected,
                "Failed for input: '{}'",
                input
            );
        }

        // Test non-dereferencing patterns
        let non_deref_cases = [
            ("@array", false),
            ("%hash", false),
            ("$scalar", false),
            ("@{$ref}", false), // This is different - not a simple dereference pattern
        ];

        for (input, expected) in non_deref_cases {
            let mut parser = crate::parser::Parser::new(input);
            parser.skip_trivia();
            assert_eq!(
                parser.is_dereferencing_pattern(),
                expected,
                "Failed for input: '{}'",
                input
            );
        }
    }
}

mod expression;
mod statement;
