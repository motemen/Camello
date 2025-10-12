use crate::{
    lexer::{LexContext, Lexer},
    SyntaxKind, T,
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

#[derive(Debug, Clone, Copy)]
pub struct ParserOptions {
    pub enable_try_statement: bool,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            enable_try_statement: true,
        }
    }
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
    current_pos: usize,
    source: &'a str,
    options: ParserOptions,
}

impl<'a> Parser<'a> {
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        Self::new_with_options(input, ParserOptions::default())
    }

    #[must_use]
    pub fn new_with_options(input: &'a str, options: ParserOptions) -> Self {
        let lexer = Lexer::new(input);

        Self {
            lexer,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            current_pos: 0,
            source: input,
            options,
        }
    }

    #[must_use]
    pub fn parse(input: &str) -> (GreenNode, Vec<ParseError>) {
        Self::parse_with_options(input, ParserOptions::default())
    }

    #[must_use]
    pub fn parse_with_options(input: &str, options: ParserOptions) -> (GreenNode, Vec<ParseError>) {
        let mut parser = Parser::new_with_options(input, options);
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
            if matches!(self.current_kind(), Some(T![__END__] | T![__DATA__])) {
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

    fn error_without_consuming(&mut self, message: &str) {
        let text_len = self.current_text().map_or(0, str::len);
        let range = TextRange::new(
            (self.current_pos as u32).into(),
            ((self.current_pos + text_len) as u32).into(),
        );

        self.errors
            .push(ParseError::new(message.to_string(), range, self.source));
    }

    /// Helper function to handle optional or required semicolons at the end of statements.
    ///
    /// A semicolon is optional when:
    /// - We're at the end of the file
    /// - The next token is a closing brace (last statement in a block)
    /// - The next token is END_KW or DATA_KW (before data sections)
    ///
    /// Otherwise, a semicolon is required.
    fn expect_optional_semicolon(&mut self, statement_name: &str) {
        if self.at(T![;]) {
            self.bump();
        } else if self.at_end() || self.at_any(&[T!['}'], T![__END__], T![__DATA__]]) {
            // Semicolon is optional in these contexts
        } else {
            // Semicolon is required but missing
            self.error(&format!("Expected ';' after {statement_name}"));
        }
    }

    /// 括弧内のカンマ区切り式をパースするヘルパー関数
    fn parse_parenthesized_list(&mut self) {
        if !self.at(T![')']) {
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

    /// Creates an iterator over non-trivia tokens starting from the given offset.
    ///
    /// This is more efficient than repeatedly calling [`peek_nth_non_trivia_token_with_context`]
    /// in a loop, as it only clones the lexer once and iterates linearly (O(N) instead of O(N²)).
    ///
    /// # Arguments
    /// * `context` - The lexical context to use for tokenization
    /// * `start_offset` - The offset (in non-trivia tokens) from the current position to start iterating
    ///
    /// # Returns
    /// An iterator over `(SyntaxKind, &'a str)` tuples, or `None` if the start offset is beyond available tokens.
    ///
    /// # Example
    /// ```ignore
    /// // ❌ Bad: O(N²) - repeatedly calls peek_nth with incrementing offset
    /// let mut scan_offset = start;
    /// while let Some((kind, text)) = self.peek_nth_non_trivia_token_with_context(ctx, scan_offset) {
    ///     // ... process token ...
    ///     scan_offset += 1;
    /// }
    ///
    /// // ✅ Good: O(N) - creates iterator once and iterates linearly
    /// if let Some(iter) = self.iter_non_trivia_tokens_from(ctx, start) {
    ///     for (kind, text) in iter {
    ///         // ... process token ...
    ///     }
    /// }
    /// ```
    fn iter_non_trivia_tokens_from(
        &self,
        context: LexContext,
        start_offset: usize,
    ) -> Option<impl Iterator<Item = (SyntaxKind, &'a str)> + '_> {
        let mut temp_lexer = self.lexer.clone();

        // Configure the lexer for quote-like parsing if needed (same as peek_nth_non_trivia_token_with_context)
        let (current_token, next_char) = self.lexer.peek_token_and_next_char();
        if let (Some(current_kind), Some('#')) = (current_token, next_char) {
            if current_kind.is_quote_like_keyword() {
                let mode = crate::lexer::QuoteLikeMode::from_keyword(current_kind);
                temp_lexer.begin_quote_like(current_kind, mode);
            }
        }

        // Create an iterator that skips trivia tokens
        let mut token_iter = std::iter::from_fn(move || loop {
            match temp_lexer.next_token_with_context(context) {
                Some((kind, text)) if !kind.is_trivia() => return Some((kind, text)),
                Some(_) => continue, // Skip trivia
                None => return None,
            }
        });

        // Skip to the start offset
        if start_offset > 0 {
            token_iter.nth(start_offset - 1)?;
        }

        Some(token_iter)
    }

    /// Returns true if the token at `offset` is followed by a fat comma (`=>`).
    fn is_followed_by_fat_comma(&self, offset: usize) -> bool {
        self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset + 1)
            .is_some_and(|(next_kind, _)| next_kind == T![=>])
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
                | SyntaxKind::VERSION
                | SyntaxKind::BARE_VERSION
                | SyntaxKind::STRING
                | SyntaxKind::BACKTICK_STRING
                | SyntaxKind::REGEX_LITERAL
                | T![/]
                | SyntaxKind::IO_EXPR
                | SyntaxKind::HEREDOC_START
                | SyntaxKind::IDENT
                | T!['(']
                | T!['{']
                | T!['[']
                | T![qw]
                | T![q]
                | T![qq]
                | T![qx]
                | T![m]
                | T![qr]
                | T![s]
                | T![tr]
                | T![y]
                | T![my]
                | T![our]
                | T![state]
                | T![local]
                | T![undef]
                | T![require]
                | T![return]
                | T![next]
                | T![last]
                | T![redo]
                | T![try]
                | T![catch]
                | T![finally]
                | T![sub]
                | T![+]
                | T![-]
                | SyntaxKind::UNARY_PLUS
                | SyntaxKind::UNARY_MINUS
                | T![++]
                | T![--]
                | SyntaxKind::PREFIX_INCREMENT
                | SyntaxKind::PREFIX_DECREMENT
                | T![!]
                | T![~]
                | T![not]
                | SyntaxKind::FILE_TEST_OP
                | T![x]
                | T![::]
                | SyntaxKind::CODE_SIGIL
        ) || kind.is_variable()
            || kind.is_sigil()
    }
}

#[must_use]
pub fn parse(input: &str) -> (GreenNode, Vec<ParseError>) {
    Parser::parse(input)
}

#[must_use]
pub fn parse_with_options(input: &str, options: ParserOptions) -> (GreenNode, Vec<ParseError>) {
    Parser::parse_with_options(input, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::test_utils::*;
    use crate::PerlNode;
    use std::fs;

    #[test]
    fn parser_success_snapshots() {
        let success_dir = fixtures_root().join("success");
        let mut files = Vec::new();
        collect_fixture_files(&success_dir, &mut files);
        files.sort();
        assert!(
            !files.is_empty(),
            "No success fixtures found in {:?}",
            success_dir
        );

        for path in files {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("Failed to read {}: {}", path.display(), err));
            let (green, errors) = parse(&source);
            assert!(
                errors.is_empty(),
                "Unexpected parse errors for {}: {:?}",
                path.display(),
                errors
            );

            let syntax = PerlNode::new_root(green);
            let relative = path
                .strip_prefix(&success_dir)
                .expect("Success fixture should live under success directory");
            let mut parts = relative
                .iter()
                .map(|component| component.to_string_lossy().into_owned())
                .collect::<Vec<String>>();
            if let Some(last) = parts.last_mut() {
                if let Some(stripped) = last.strip_suffix(".pl") {
                    *last = stripped.to_string();
                }
            }
            let snapshot_name = format!("success__{}", parts.join("__"));

            insta::assert_snapshot!(snapshot_name, render_success_tree(&syntax));
        }
    }

    #[test]
    fn parser_error_snapshots() {
        let error_dir = fixtures_root().join("errors");
        let mut files = Vec::new();
        collect_fixture_files(&error_dir, &mut files);
        files.sort();
        assert!(
            !files.is_empty(),
            "No error fixtures found in {:?}",
            error_dir
        );

        for path in files {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("Failed to read {}: {}", path.display(), err));
            let (_green, errors) = parse(&source);
            assert!(
                !errors.is_empty(),
                "Expected parse errors for {} but parser succeeded",
                path.display()
            );

            let relative = path
                .strip_prefix(&error_dir)
                .expect("Fixture should live under error directory");
            let mut parts = relative
                .iter()
                .map(|component| component.to_string_lossy().into_owned())
                .collect::<Vec<String>>();
            if let Some(last) = parts.last_mut() {
                if let Some(stripped) = last.strip_suffix(".pl") {
                    *last = stripped.to_string();
                }
            }
            let snapshot_name = format!("errors__{}", parts.join("__"));

            insta::assert_snapshot!(snapshot_name, render_errors(&errors));
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
}

mod expression;
mod statement;
#[cfg(test)]
mod test_utils;
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
            .unwrap_or_else(|| panic!("missing package statement in '{}'", input));

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

#[test]
fn test_iter_non_trivia_tokens_from() {
    // Test basic iteration
    let input = "$a + $b * $c";
    let parser = Parser::new(input);

    // Start from offset 0 (should start at $a)
    let iter = parser
        .iter_non_trivia_tokens_from(LexContext::Value, 0)
        .expect("Should return iterator");
    let tokens: Vec<_> = iter.take(5).collect();

    assert_eq!(tokens.len(), 5);
    assert_eq!(tokens[0].0, SyntaxKind::SCALAR_SIGIL); // $
    assert_eq!(tokens[1].0, SyntaxKind::IDENT); // a
    assert_eq!(tokens[2].0, T![+]); // +
    assert_eq!(tokens[3].0, SyntaxKind::SCALAR_SIGIL); // $
    assert_eq!(tokens[4].0, SyntaxKind::IDENT); // b

    // Test with offset (start from + operator)
    let iter2 = parser
        .iter_non_trivia_tokens_from(LexContext::Value, 2)
        .expect("Should return iterator");
    let tokens2: Vec<_> = iter2.take(3).collect();

    assert_eq!(tokens2.len(), 3);
    assert_eq!(tokens2[0].0, T![+]); // +
    assert_eq!(tokens2[1].0, SyntaxKind::SCALAR_SIGIL); // $
    assert_eq!(tokens2[2].0, SyntaxKind::IDENT); // b
}

#[test]
fn test_iter_non_trivia_tokens_from_with_braces() {
    // Test the specific case from looks_like_hash_ref_at_offset
    let input = "{ a => 1; b => 2 }";
    let parser = Parser::new(input);

    // Start from offset 1 (after the opening {, just like looks_like_hash_ref_at_offset does)
    let iter = parser
        .iter_non_trivia_tokens_from(LexContext::Value, 1)
        .expect("Should return iterator");

    let mut found_semicolon = false;
    let mut brace_depth = 0;

    for (kind, _) in iter {
        match kind {
            T!['{'] => brace_depth += 1,
            T!['}'] => {
                if brace_depth == 0 {
                    break;
                }
                brace_depth -= 1;
            }
            T![;] if brace_depth == 0 => {
                found_semicolon = true;
                break;
            }
            _ => {}
        }
    }

    assert!(found_semicolon, "Should find semicolon at top level");
}

#[test]
fn test_iter_non_trivia_tokens_from_skips_trivia() {
    // Test that the iterator skips whitespace and comments
    let input = "$a   # comment\n  + $b";
    let parser = Parser::new(input);

    let iter = parser
        .iter_non_trivia_tokens_from(LexContext::Value, 0)
        .expect("Should return iterator");
    let tokens: Vec<_> = iter.take(4).collect();

    // Should only get non-trivia tokens
    assert_eq!(tokens.len(), 4);
    assert_eq!(tokens[0].0, SyntaxKind::SCALAR_SIGIL); // $
    assert_eq!(tokens[1].0, SyntaxKind::IDENT); // a
    assert_eq!(tokens[2].0, T![+]); // +
    assert_eq!(tokens[3].0, SyntaxKind::SCALAR_SIGIL); // $
}

#[test]
fn test_iter_non_trivia_tokens_from_beyond_end() {
    // Test with offset beyond available tokens
    let input = "$a + $b";
    let parser = Parser::new(input);

    let iter = parser.iter_non_trivia_tokens_from(LexContext::Value, 100);
    assert!(iter.is_none(), "Should return None for offset beyond end");
}
