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
                    self.bump();
                }
            } else if !self.statement() {
                self.error("Expected a statement, but found an unexpected token.");
                self.bump(); // トークンを消費して回復
            }
            self.skip_trivia();
        }

        self.builder.finish_node();
    }

    /// Parse a data section (__END__ or __DATA__)
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
    // Parse block function arguments: block + optional additional arguments
    fn parse_block_function_args(&mut self) {
        // Parse the block (which should be at L_BRACE)
        if self.at(SyntaxKind::L_BRACE) {
            self.builder.start_node(SyntaxKind::BLOCK_STMT.into());
            self.bump(); // {
            self.skip_trivia();

            // Parse statements inside the block
            while !self.at(SyntaxKind::R_BRACE) && !self.at_end() {
                if !self.statement() {
                    // If we can't parse a statement, try to recover
                    self.error("Expected statement in block");
                    if self.current_kind().is_some() {
                        self.bump(); // Skip the problematic token
                    }
                }
                self.skip_trivia();
            }

            self.expect(SyntaxKind::R_BRACE);
            self.builder.finish_node();
            self.skip_trivia();
        }

        // Parse additional arguments if present (no comma before them)
        // For example: map { ... } @list
        if self.is_at_start_of_expression() {
            self.expression_list();
        }
    }

    fn block(&mut self) {
        self.builder.start_node(SyntaxKind::BLOCK_STMT.into());

        self.expect(SyntaxKind::L_BRACE);
        self.skip_trivia();

        while !self.at(SyntaxKind::R_BRACE) && !self.at_end() {
            if !self.statement() {
                self.error("Expected a statement in block, but found an unexpected token.");
                self.bump(); // トークンを消費して回復
            }
            self.skip_trivia();
        }

        self.expect(SyntaxKind::R_BRACE);

        self.builder.finish_node();
    }

    fn is_at_start_of_expression(&self) -> bool {
        if let Some(kind) = self.current_kind() {
            self.at_any(&[
                SyntaxKind::NUMBER,
                SyntaxKind::STRING,
                SyntaxKind::REGEX_LITERAL,
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
                SyntaxKind::MY_KW, // Add variable declaration keywords as start of expression
                SyntaxKind::OUR_KW,
                SyntaxKind::STATE_KW,
                SyntaxKind::LOCAL_KW,
                SyntaxKind::RETURN_KW, // return statements can start expressions
            ]) || kind.is_variable()
                || kind.is_sigil()
        } else {
            false
        }
    }

    fn is_block_function(function_name: &str) -> bool {
        matches!(function_name, "eval" | "map" | "grep" | "sort" | "do")
    }

    fn expression(&mut self) -> bool {
        self.assignment_expr()
    }

    // Helper function for parsing binary expressions with reduced code duplication
    fn parse_binary_expr<F>(
        &mut self,
        mut next_precedence_fn: F,
        operators: &[SyntaxKind],
        node_kind: SyntaxKind,
        error_message: &str,
    ) -> bool
    where
        F: FnMut(&mut Self) -> bool,
    {
        let start = self.builder.checkpoint();
        if !next_precedence_fn(self) {
            return false;
        }

        while self.at_any(operators) {
            self.builder.start_node_at(start, node_kind.into());
            self.bump(); // operator
            self.skip_trivia();
            if !next_precedence_fn(self) {
                self.error(error_message);
            }
            self.builder.finish_node();
        }
        true
    }

    // Assignment expression: expr = expr
    fn assignment_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.logical_or_expr() {
            return false;
        }

        if self.at(SyntaxKind::EQ) {
            self.builder
                .start_node_at(start, SyntaxKind::INFIX_EXPR.into());
            self.bump(); // =
            self.skip_trivia();
            if !self.assignment_expr() {
                self.error("Expected expression after assignment operator");
            }
            self.builder.finish_node();
        }
        true
    }

    // Logical OR operators: ||
    fn logical_or_expr(&mut self) -> bool {
        self.parse_binary_expr(
            Self::logical_and_expr,
            &[SyntaxKind::LOGICAL_OR],
            SyntaxKind::INFIX_EXPR,
            "Expected expression after logical OR operator",
        )
    }

    // Logical AND operators: &&
    fn logical_and_expr(&mut self) -> bool {
        self.parse_binary_expr(
            Self::comparison_expr,
            &[SyntaxKind::LOGICAL_AND],
            SyntaxKind::INFIX_EXPR,
            "Expected expression after logical AND operator",
        )
    }

    // Regex operators: =~ !~
    fn regex_expr(&mut self) -> bool {
        self.parse_binary_expr(
            Self::postfix_expr,
            &[SyntaxKind::REGEX_MATCH, SyntaxKind::REGEX_NOT_MATCH],
            SyntaxKind::REGEX_EXPR,
            "Expected expression after regex operator",
        )
    }

    fn expression_list(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.expression() {
            return false;
        }

        // If we have comma-separated expressions, wrap them in a single EXPR_LIST node
        if self.at_any(&[SyntaxKind::COMMA, SyntaxKind::FAT_COMMA]) {
            self.builder
                .start_node_at(start, SyntaxKind::EXPR_LIST.into());

            while self.at_any(&[SyntaxKind::COMMA, SyntaxKind::FAT_COMMA]) {
                self.bump(); // , or =>
                self.skip_trivia();

                // Check for trailing comma - if we're at the end of a list context, don't require another expression
                if self.is_at_start_of_expression() && !self.expression() {
                    self.error("Expected expression after comma in list");
                }
                // If no expression follows, it's a trailing comma - that's OK
            }

            self.builder.finish_node();
        }

        true
    }

    // Additive operators: + - .
    fn additive_expr(&mut self) -> bool {
        self.parse_binary_expr(
            Self::multiplicative_expr,
            &[SyntaxKind::PLUS, SyntaxKind::MINUS, SyntaxKind::DOT],
            SyntaxKind::INFIX_EXPR,
            "Expected expression after additive operator",
        )
    }

    // Comparison operators: < > <= >= == !=
    fn comparison_expr(&mut self) -> bool {
        const OPERATORS: &[SyntaxKind] = &[
            SyntaxKind::LT,
            SyntaxKind::GT,
            SyntaxKind::LE,
            SyntaxKind::GE,
            SyntaxKind::EQ_EQ,
            SyntaxKind::NE,
            SyntaxKind::STR_EQ,
            SyntaxKind::STR_NE,
            SyntaxKind::STR_GT,
            SyntaxKind::STR_LT,
            SyntaxKind::STR_GE,
            SyntaxKind::STR_LE,
            SyntaxKind::STR_CMP,
        ];
        self.parse_binary_expr(
            Self::additive_expr,
            OPERATORS,
            SyntaxKind::INFIX_EXPR,
            "Expected expression after comparison operator",
        )
    }

    // Multiplicative operators: * / % x
    fn multiplicative_expr(&mut self) -> bool {
        const OPERATORS: &[SyntaxKind] = &[
            SyntaxKind::STAR,
            SyntaxKind::SLASH,
            SyntaxKind::MODULO,
            SyntaxKind::X,
        ];
        self.parse_binary_expr(
            Self::regex_expr,
            OPERATORS,
            SyntaxKind::INFIX_EXPR,
            "Expected expression after multiplicative operator",
        )
    }

    // Postfix expressions: expr -> method(), expr->{key}, expr->[index], expr->(args), expr()
    fn postfix_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.primary_expr() {
            return false;
        }

        loop {
            if self.at(SyntaxKind::ARROW) {
                self.bump(); // ->
                self.skip_trivia();

                match self.current_kind() {
                    Some(SyntaxKind::L_BRACE) => {
                        // Hash reference access: expr->{key}
                        self.builder
                            .start_node_at(start, SyntaxKind::HASH_REF_ACCESS_EXPR.into());
                        self.bump(); // {
                        self.skip_trivia();

                        if !self.expression() {
                            self.error("Expected expression in hash reference access");
                        }

                        if !self.at(SyntaxKind::R_BRACE) {
                            self.error("Expected '}' after hash key");
                        } else {
                            self.bump(); // }
                            self.skip_trivia();
                        }

                        self.builder.finish_node();
                    }
                    Some(SyntaxKind::L_BRACKET) => {
                        // Array reference access: expr->[index]
                        self.builder
                            .start_node_at(start, SyntaxKind::ARRAY_REF_ACCESS_EXPR.into());
                        self.bump(); // [
                        self.skip_trivia();

                        if !self.expression() {
                            self.error("Expected expression in array reference access");
                        }

                        if !self.at(SyntaxKind::R_BRACKET) {
                            self.error("Expected ']' after array index");
                        } else {
                            self.bump(); // ]
                            self.skip_trivia();
                        }

                        self.builder.finish_node();
                    }
                    Some(SyntaxKind::L_PAREN) => {
                        // Code reference call: expr->(args)
                        self.builder
                            .start_node_at(start, SyntaxKind::CODE_REF_CALL_EXPR.into());
                        self.bump(); // (
                        self.skip_trivia();

                        self.expression_list();

                        if !self.at(SyntaxKind::R_PAREN) {
                            self.error("Expected ')' after code reference arguments");
                        } else {
                            self.bump(); // )
                            self.skip_trivia();
                        }

                        self.builder.finish_node();
                    }
                    Some(SyntaxKind::IDENT) => {
                        // Method call: expr->method()
                        self.builder
                            .start_node_at(start, SyntaxKind::METHOD_CALL_EXPR.into());

                        self.parse_identifier_or_qualified();
                        self.skip_trivia();

                        if self.at(SyntaxKind::L_PAREN) {
                            self.bump(); // (
                            self.skip_trivia();

                            self.expression_list();

                            if !self.at(SyntaxKind::R_PAREN) {
                                self.error("Expected ')' after method arguments");
                            } else {
                                self.bump(); // )
                                self.skip_trivia();
                            }
                        }

                        self.builder.finish_node();
                    }
                    _ => {
                        self.error("Expected '{', '[', '(' or identifier after '->'");
                        break;
                    }
                }
            } else if self.at(SyntaxKind::L_PAREN) {
                // Function call: expr(args)
                self.builder
                    .start_node_at(start, SyntaxKind::FUNCTION_CALL_EXPR.into());
                self.bump(); // (
                self.skip_trivia();

                self.expression_list();

                if !self.at(SyntaxKind::R_PAREN) {
                    self.error("Expected ')' after function arguments");
                } else {
                    self.bump(); // )
                    self.skip_trivia();
                }

                self.builder.finish_node();
            } else if self.at(SyntaxKind::L_BRACKET) {
                // Direct array subscription: expr[index]
                self.builder
                    .start_node_at(start, SyntaxKind::ARRAY_SUBSCRIPTION_EXPR.into());
                self.bump(); // [
                self.skip_trivia();

                if !self.expression() {
                    self.error("Expected expression in array subscription");
                }

                if !self.at(SyntaxKind::R_BRACKET) {
                    self.error("Expected ']' after array index");
                } else {
                    self.bump(); // ]
                    self.skip_trivia();
                }

                self.builder.finish_node();
            } else if self.at(SyntaxKind::L_BRACE) {
                // Direct hash subscription: expr{key}
                self.builder
                    .start_node_at(start, SyntaxKind::HASH_SUBSCRIPTION_EXPR.into());
                self.bump(); // {
                self.skip_trivia();

                if !self.expression() {
                    self.error("Expected expression in hash subscription");
                }

                if !self.at(SyntaxKind::R_BRACE) {
                    self.error("Expected '}' after hash key");
                } else {
                    self.bump(); // }
                    self.skip_trivia();
                }

                self.builder.finish_node();
            } else {
                // No more postfix operations
                break;
            }
        }
        true
    }

    fn primary_expr(&mut self) -> bool {
        self.skip_trivia();

        let at_start = self.is_at_start_of_expression();
        if !at_start {
            return false;
        }

        match self.current_kind() {
            Some(SyntaxKind::NUMBER)
            | Some(SyntaxKind::STRING)
            | Some(SyntaxKind::REGEX_LITERAL) => {
                self.bump();
                self.skip_trivia();
            }
            Some(kind) if kind.is_variable() => {
                self.bump();
                self.skip_trivia();
            }
            Some(kind) if kind.is_sigil() => {
                // Check if this is a dereferencing pattern (sigil followed by another sigil)
                if self.is_dereferencing_pattern() {
                    self.parse_dereferencing();
                } else {
                    self.parse_variable();
                }
            }
            Some(
                SyntaxKind::MY_KW
                | SyntaxKind::OUR_KW
                | SyntaxKind::STATE_KW
                | SyntaxKind::LOCAL_KW,
            ) => {
                // Variable declaration as expression (e.g., my $x = 1)
                self.var_decl_expr();
            }
            Some(SyntaxKind::IDENT) => {
                let start = self.builder.checkpoint();

                // Get the function name before parsing
                let function_name = self.current_text().unwrap_or("").to_string();

                // 修飾付き識別子かもしれないのでparse_identifier_or_qualifiedを使用
                self.parse_identifier_or_qualified();
                self.skip_trivia();

                // Check for block functions first
                if Self::is_block_function(&function_name) && self.at(SyntaxKind::L_BRACE) {
                    // This is a block function call
                    self.builder
                        .start_node_at(start, SyntaxKind::BLOCK_FUNCTION_CALL_EXPR.into());

                    self.parse_block_function_args();

                    self.builder.finish_node();
                } else if let Some(kind) = self.current_kind() {
                    // Check if we have regular function arguments following the identifier
                    // Value-like objects
                    if kind.is_variable()
                        || self.at_any(&[
                            SyntaxKind::NUMBER,
                            SyntaxKind::STRING,
                            SyntaxKind::L_BRACE,   // Hash reference: {}
                            SyntaxKind::L_BRACKET, // Array reference: []
                            SyntaxKind::MY_KW,     // Variable declarations as arguments
                            SyntaxKind::OUR_KW,
                            SyntaxKind::STATE_KW,
                            SyntaxKind::LOCAL_KW,
                        ])
                        || kind.is_sigil()
                    {
                        // We have a regular function call, wrap everything in FUNCTION_CALL_EXPR
                        self.builder
                            .start_node_at(start, SyntaxKind::FUNCTION_CALL_EXPR.into());

                        // Parse arguments as an expression list
                        self.expression_list();

                        self.builder.finish_node();
                    } else if kind == SyntaxKind::IDENT {
                        // Might be a nested function call eg. `foo bar(1)`
                        self.builder
                            .start_node_at(start, SyntaxKind::FUNCTION_CALL_EXPR.into());

                        // Parse arguments as an expression list
                        self.expression_list();

                        self.builder.finish_node();
                    }
                }
            }
            Some(SyntaxKind::L_PAREN) => {
                // 括弧式
                self.bump(); // (
                self.skip_trivia();

                // 括弧内のリスト（配列の初期化など）
                self.parse_parenthesized_list();

                if self.at(SyntaxKind::R_PAREN) {
                    self.bump(); // )
                    self.skip_trivia();
                }
            }
            Some(SyntaxKind::L_BRACE) => {
                // ハッシュリファレンス（匿名ハッシュ）: {}
                self.hash_ref();
            }
            Some(SyntaxKind::L_BRACKET) => {
                // 配列リファレンス（匿名配列）: []
                self.array_ref();
            }
            Some(SyntaxKind::QW_KW) => {
                // qw() 式
                self.qw_expr();
            }
            Some(SyntaxKind::RETURN_KW) => {
                // return 文 (キーワードとして処理)
                self.bump(); // consume return
                self.skip_trivia();

                // return の後に式がある場合は処理する
                if self.is_at_start_of_expression() {
                    self.expression();
                }
            }
            Some(SyntaxKind::Q_KW) => {
                // q() 式
                self.q_expr();
            }
            Some(SyntaxKind::QQ_KW) => {
                // qq() 式
                self.qq_expr();
            }
            Some(SyntaxKind::QX_KW) => {
                // qx() 式
                self.qx_expr();
            }
            Some(SyntaxKind::M_KW) => {
                // m() 式
                self.m_expr();
            }
            Some(SyntaxKind::QR_KW) => {
                // qr() 式
                self.qr_expr();
            }
            Some(SyntaxKind::S_KW) => {
                // s() 式
                self.s_expr();
            }
            _ => {
                // is_at_start_of_expression でチェックしているので、ここには来ないはず
                return false;
            }
        }
        true
    }

    fn hash_ref(&mut self) {
        self.builder.start_node(SyntaxKind::HASH_REF.into());

        self.expect(SyntaxKind::L_BRACE);
        self.skip_trivia();

        // Parse expressions inside braces - could be key => value pairs or a simple expression list
        if !self.at(SyntaxKind::R_BRACE) {
            self.expression_list();
        }

        self.skip_trivia();
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn array_ref(&mut self) {
        self.builder.start_node(SyntaxKind::ARRAY_REF.into());

        self.expect(SyntaxKind::L_BRACKET);
        self.skip_trivia();

        // Parse expression list inside brackets (supports trailing comma)
        if !self.at(SyntaxKind::R_BRACKET) {
            self.expression_list();
        }

        self.skip_trivia();
        self.expect(SyntaxKind::R_BRACKET);
        self.builder.finish_node();
    }

    fn qw_expr(&mut self) {
        self.builder.start_node(SyntaxKind::QW_EXPR.into());

        // "qw"
        self.expect(SyntaxKind::QW_KW);
        self.skip_trivia();

        // Determine delimiter and find closing delimiter
        let (opening_delim, closing_delim) = match self.current_kind() {
            Some(SyntaxKind::L_PAREN) => (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN),
            Some(SyntaxKind::L_BRACKET) => (SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET),
            Some(SyntaxKind::L_BRACE) => (SyntaxKind::L_BRACE, SyntaxKind::R_BRACE),
            Some(SyntaxKind::SLASH) => (SyntaxKind::SLASH, SyntaxKind::SLASH),
            _ => {
                self.error("Expected qw() delimiter: (, [, {, or /");
                return;
            }
        };

        // Consume opening delimiter
        self.expect(opening_delim);
        // Don't skip trivia here - we need whitespace to separate words

        // Parse words inside qw() - consume existing tokens and convert to QW_STRING
        while !self.at(closing_delim) && !self.at_end() {
            // Skip whitespace/trivia
            if let Some(kind) = self.current_kind() {
                if kind.is_trivia() {
                    self.bump();
                    continue;
                }
            }

            // Check if we're at the closing delimiter
            if self.at(closing_delim) {
                break;
            }

            // Consume any non-whitespace tokens as QW_STRING
            if let Some((_, text)) = self.current_token.take() {
                // Add as QW_STRING token
                self.builder.token(SyntaxKind::QW_STRING.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        // Closing delimiter
        self.expect(closing_delim);

        self.builder.finish_node();
    }

    fn q_expr(&mut self) {
        self.parse_q_family_expr(
            SyntaxKind::Q_EXPR,
            SyntaxKind::Q_KW,
            SyntaxKind::Q_STRING,
            "q",
        );
    }

    fn qq_expr(&mut self) {
        self.parse_q_family_expr(
            SyntaxKind::QQ_EXPR,
            SyntaxKind::QQ_KW,
            SyntaxKind::QQ_STRING,
            "qq",
        );
    }

    fn qx_expr(&mut self) {
        self.parse_q_family_expr(
            SyntaxKind::QX_EXPR,
            SyntaxKind::QX_KW,
            SyntaxKind::QX_STRING,
            "qx",
        );
    }

    fn m_expr(&mut self) {
        self.parse_q_family_expr(
            SyntaxKind::M_EXPR,
            SyntaxKind::M_KW,
            SyntaxKind::M_STRING,
            "m",
        );
    }

    fn qr_expr(&mut self) {
        self.parse_q_family_expr(
            SyntaxKind::QR_EXPR,
            SyntaxKind::QR_KW,
            SyntaxKind::QR_STRING,
            "qr",
        );
    }

    fn s_expr(&mut self) {
        self.parse_s_expr();
    }

    fn consume_regex_flags(&mut self) {
        // Consume regex flags like 'g', 'i', 'm', 's', 'x' after the closing delimiter
        // These might be treated as IDENT tokens or specific keywords, but should be part of the regex
        while let Some(kind) = self.current_kind() {
            let is_valid_flag = match kind {
                SyntaxKind::IDENT => {
                    if let Some((_, text)) = &self.current_token {
                        // Check if it's a valid regex flag
                        text.chars()
                            .all(|c| matches!(c, 'g' | 'i' | 'm' | 's' | 'x'))
                            && !text.is_empty()
                    } else {
                        false
                    }
                }
                // Handle single character flags that might be interpreted as keywords
                SyntaxKind::M_KW => {
                    // 'm' flag might be interpreted as M_KW
                    if let Some((_, text)) = &self.current_token {
                        *text == "m"
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if is_valid_flag {
                self.bump(); // consume the flags
            } else {
                break; // Not a regex flag, stop consuming
            }
        }
    }

    fn parse_q_family_expr(
        &mut self,
        expr_kind: SyntaxKind,
        kw_kind: SyntaxKind,
        string_kind: SyntaxKind,
        operator_name: &str,
    ) {
        self.builder.start_node(expr_kind.into());

        // "q", "qq", or "qx"
        self.expect(kw_kind);
        self.skip_trivia();

        // Determine delimiter and find closing delimiter
        let (opening_delim, closing_delim) = match self.current_kind() {
            Some(SyntaxKind::L_PAREN) => (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN),
            Some(SyntaxKind::L_BRACKET) => (SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET),
            Some(SyntaxKind::L_BRACE) => (SyntaxKind::L_BRACE, SyntaxKind::R_BRACE),
            Some(SyntaxKind::SLASH) => (SyntaxKind::SLASH, SyntaxKind::SLASH),
            _ => {
                self.error(&format!(
                    "Expected {}() delimiter: (, [, {{, or /",
                    operator_name
                ));
                self.builder.finish_node(); // Finish the node to avoid panic
                return;
            }
        };

        // Consume opening delimiter
        self.expect(opening_delim);

        // Parse content inside - everything becomes the specific string kind
        while !self.at(closing_delim) && !self.at_end() {
            // Check if we're at the closing delimiter
            if self.at(closing_delim) {
                break;
            }

            // Consume any tokens as the string kind (preserving original text)
            if let Some((_, text)) = self.current_token.take() {
                self.builder.token(string_kind.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        // Closing delimiter
        self.expect(closing_delim);

        // For regex expressions (m and qr), consume optional flags
        if matches!(expr_kind, SyntaxKind::M_EXPR | SyntaxKind::QR_EXPR) {
            self.consume_regex_flags();
        }

        self.builder.finish_node();
    }

    fn parse_s_expr(&mut self) {
        self.builder.start_node(SyntaxKind::S_EXPR.into());

        // "s"
        self.expect(SyntaxKind::S_KW);
        self.skip_trivia();

        // Determine delimiter and find closing delimiter
        let (opening_delim, closing_delim) = match self.current_kind() {
            Some(SyntaxKind::L_PAREN) => (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN),
            Some(SyntaxKind::L_BRACKET) => (SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET),
            Some(SyntaxKind::L_BRACE) => (SyntaxKind::L_BRACE, SyntaxKind::R_BRACE),
            Some(SyntaxKind::SLASH) => (SyntaxKind::SLASH, SyntaxKind::SLASH),
            _ => {
                self.error("Expected s() delimiter: (, [, {, or /");
                self.builder.finish_node(); // Finish the node to avoid panic
                return;
            }
        };

        // Consume opening delimiter
        self.expect(opening_delim);

        // Parse pattern part - everything until the middle delimiter becomes S_PATTERN
        while !self.at(closing_delim) && !self.at_end() {
            // Consume any tokens as pattern (preserving original text)
            if let Some((_, text)) = self.current_token.take() {
                self.builder.token(SyntaxKind::S_PATTERN.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        self.expect(closing_delim);

        let closing_delim_repl = if opening_delim != closing_delim {
            // Paired delimiters for pattern. Replacement can have its own delimiters.
            let (opening_repl, closing_repl) = match self.current_kind() {
                Some(SyntaxKind::L_PAREN) => (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN),
                Some(SyntaxKind::L_BRACKET) => (SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET),
                Some(SyntaxKind::L_BRACE) => (SyntaxKind::L_BRACE, SyntaxKind::R_BRACE),
                Some(SyntaxKind::SLASH) => (SyntaxKind::SLASH, SyntaxKind::SLASH), // s(pat)/repl/ is also valid
                _ => {
                    self.error("Expected opening delimiter for replacement part of s expression");
                    self.builder.finish_node();
                    return;
                }
            };
            self.expect(opening_repl);
            closing_repl
        } else {
            // Symmetric delimiter for pattern. Replacement uses the same.
            closing_delim
        };

        // Parse replacement part - everything until the final delimiter becomes S_REPLACEMENT
        while !self.at(closing_delim_repl) && !self.at_end() {
            // Consume any tokens as replacement (preserving original text)
            if let Some((_, text)) = self.current_token.take() {
                self.builder.token(SyntaxKind::S_REPLACEMENT.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        // Final closing delimiter
        self.expect(closing_delim_repl);

        // Consume optional flags (like 'g', 'i', 'm', 's', 'x')
        self.consume_regex_flags();

        self.builder.finish_node();
    }

    fn parse_variable(&mut self) {
        let sigil = self.current_kind().unwrap();
        let var_kind = match sigil {
            SyntaxKind::DOLLAR => SyntaxKind::SCALAR_VAR,
            SyntaxKind::AT => SyntaxKind::ARRAY_VAR,
            SyntaxKind::PERCENT => SyntaxKind::HASH_VAR,
            _ => unreachable!(),
        };

        self.builder.start_node(var_kind.into());

        // Sigil を消費
        self.bump();
        self.skip_trivia();

        // Check what comes after the sigil
        match self.current_kind() {
            Some(SyntaxKind::IDENT) => {
                // Regular identifier or qualified identifier (including $_, $_foo, etc.)
                self.parse_identifier_or_qualified();
            }
            Some(SyntaxKind::NUMBER) => {
                // Number like $1, $2, etc. - treat as regular variable name
                self.bump();
            }
            Some(SyntaxKind::AT) => {
                // Special punctuation like $@ - treat as regular variable name
                self.bump();
            }
            Some(SyntaxKind::CARET) => {
                // Handle $^ or $^X patterns
                self.bump(); // consume ^

                // Check if there's a character after ^
                if self.at(SyntaxKind::IDENT) {
                    // This is $^X pattern where X is an identifier (single char)
                    self.bump();
                }
            }
            Some(SyntaxKind::L_BRACE) => {
                // Handle ${...} syntax (e.g., ${^NAME})
                self.bump(); // consume {

                // Check for ^ inside braces
                if self.at(SyntaxKind::CARET) {
                    self.bump(); // consume ^
                }

                // Parse identifier inside braces
                if self.at(SyntaxKind::IDENT) {
                    self.bump();
                }

                // Expect closing brace
                if self.at(SyntaxKind::R_BRACE) {
                    self.bump();
                } else {
                    self.error("Expected '}' to close variable name");
                }
            }
            _ => {
                // Check for other punctuation characters that might be tokenized differently
                let text = self.current_text().unwrap_or("");
                if matches!(
                    text,
                    "!" | "?" | "|" | "&" | "`" | "'" | "\"" | "~" | ":" | "\\" | "$"
                ) {
                    // These are punctuation characters like $!, $?, $$, etc. - treat as regular variable names
                    self.bump();
                } else {
                    // 識別子を期待（修飾付き識別子も含む）
                    self.parse_identifier_or_qualified();
                }
            }
        }

        self.builder.finish_node();

        self.skip_trivia();
    }

    /// my/state宣言専用の変数パース（修飾付き識別子は使用しない）
    fn parse_variable_simple(&mut self) {
        let sigil = self.current_kind().unwrap();
        let var_kind = match sigil {
            SyntaxKind::DOLLAR => SyntaxKind::SCALAR_VAR,
            SyntaxKind::AT => SyntaxKind::ARRAY_VAR,
            SyntaxKind::PERCENT => SyntaxKind::HASH_VAR,
            _ => unreachable!(),
        };

        self.builder.start_node(var_kind.into());

        // Sigil を消費
        self.bump();
        self.skip_trivia();

        // 識別子を期待（単純な識別子のみ、修飾付きは不可）
        if self.at(SyntaxKind::IDENT) {
            self.bump();

            // Check for :: after identifier - if found, it's a package-qualified name which is not allowed for my/state
            if self.at(SyntaxKind::DOUBLE_COLON) {
                self.error("Package-qualified variable names are not allowed with 'my' or 'state' declarations");
            }
        } else {
            self.error("Expected identifier after sigil");
        }

        self.builder.finish_node();
    }

    /// our/local宣言専用の変数パース（修飾付き識別子も可能）
    fn parse_variable_qualified(&mut self) {
        let sigil = self.current_kind().unwrap();
        let var_kind = match sigil {
            SyntaxKind::DOLLAR => SyntaxKind::SCALAR_VAR,
            SyntaxKind::AT => SyntaxKind::ARRAY_VAR,
            SyntaxKind::PERCENT => SyntaxKind::HASH_VAR,
            _ => unreachable!(),
        };

        self.builder.start_node(var_kind.into());

        // Sigil を消費
        self.bump();
        self.skip_trivia();

        // 識別子を期待（修飾付き識別子も可能）
        self.parse_identifier_or_qualified();

        self.builder.finish_node();
    }

    /// デリファレンスパターンかどうかをチェック（sigil followed by sigil）
    fn is_dereferencing_pattern(&self) -> bool {
        // 現在のトークンがsigilでない場合、デリファレンスではない
        if let Some(current) = self.current_kind() {
            if !current.is_sigil() {
                return false;
            }
        } else {
            return false;
        }

        // 次のトークンの先読み（簡単な実装）
        // 現在位置から先を見て、最初の非triviaトークンがsigilかチェック
        let current_text = self.current_text().unwrap_or("");
        let remaining_source = &self.source[self.current_pos + current_text.len()..];

        // 空白をスキップ
        let trimmed = remaining_source.trim_start();

        // Valid dereference patterns: @$ref, %$ref, $$ref (sigil followed by $)
        // Only $ sigil can be dereferenced, so we check if next token is $
        trimmed.starts_with('$')
    }

    /// デリファレンス式をパース（例: @$var, %$var, $$var）
    fn parse_dereferencing(&mut self) {
        self.builder.start_node(SyntaxKind::DEREF_EXPR.into());

        // 最初のsigil（デリファレンス演算子）を消費
        self.bump();
        self.skip_trivia();

        // 次のsigilとそれに続く変数をパース
        if let Some(kind) = self.current_kind() {
            if kind.is_sigil() {
                self.parse_variable();
            } else {
                self.error("Expected variable after dereference sigil");
            }
        } else {
            self.error("Expected variable after dereference sigil");
        }

        self.builder.finish_node();
    }

    /// 通常の識別子または修飾付き識別子をパースする
    /// 例: "Foo", "Foo::Bar", "Foo::Bar::Baz"
    fn parse_identifier_or_qualified(&mut self) {
        if !self.at(SyntaxKind::IDENT) {
            self.error("Expected identifier");
            return;
        }

        // チェックポイントを作成してから最初の識別子を消費
        let checkpoint = self.builder.checkpoint();
        self.bump(); // 最初の識別子

        // :: があるかチェック
        if self.at(SyntaxKind::DOUBLE_COLON) {
            // 修飾付き識別子として扱う
            self.builder
                .start_node_at(checkpoint, SyntaxKind::QUALIFIED_IDENT.into());

            // :: の後の部分を処理
            while self.at(SyntaxKind::DOUBLE_COLON) {
                self.bump(); // ::

                if self.at(SyntaxKind::IDENT) {
                    self.bump();
                } else {
                    self.error("Expected identifier after '::'");
                    break;
                }
            }

            self.builder.finish_node(); // QUALIFIED_IDENT
        }
        // else: 単純な識別子なのでそのまま（既に消費済み）
    }

    // ヘルパーメソッド
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

        // エラートークンを作成
        if let Some((_, text)) = self.current_token.take() {
            self.builder.token(SyntaxKind::ERROR.into(), text);
            self.current_pos += text.len();
        }
        self.current_token = self.lexer.next_token();
    }

    /// Helper function to parse comma-separated expressions within parentheses
    fn parse_parenthesized_list(&mut self) {
        if !self.at(SyntaxKind::R_PAREN) {
            self.expression_list();
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
        // エラーリカバリが無限ループを起こさないことを確認
        let input = "my = @ % ^ invalid tokens here;";
        let (green, errors) = parse(input);

        // エラーは発生するが、パースは完了すること
        assert!(!errors.is_empty(), "Should have parse errors");

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // ASTに何らかの構造があることを確認（無限ループしていない証拠）
        assert!(
            syntax.children().count() > 0,
            "Should have some parsed structure"
        );
    }

    #[test]
    fn test_infinite_loop_with_unknown_operator() {
        // 未実装の論理OR演算子（||）で無限ループが発生しないことを確認
        let input = "sub f { return { a => 1||2 } }";
        let (green, errors) = parse(input);

        // パースが完了すること（無限ループしない）
        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // エラーがあっても構造は存在する
        println!("Errors: {:?}", errors);
        println!("AST structure: {:?}", syntax);
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
}

mod statement;
