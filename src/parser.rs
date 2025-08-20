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

            if !self.statement() {
                self.error("Expected a statement, but found an unexpected token.");
                self.bump(); // トークンを消費して回復
            }
            self.skip_trivia();
        }

        self.builder.finish_node();
    }

    fn statement(&mut self) -> bool {
        self.skip_trivia();

        match self.current_kind() {
            Some(k) if matches!(k, SyntaxKind::MY_KW | SyntaxKind::OUR_KW | SyntaxKind::STATE_KW | SyntaxKind::LOCAL_KW) => {
                self.var_decl();
                true
            }
            Some(SyntaxKind::SUB_KW) => {
                self.sub_def();
                true
            }
            Some(SyntaxKind::IF_KW) => {
                self.if_stmt();
                true
            }
            Some(SyntaxKind::FOR_KW) | Some(SyntaxKind::FOREACH_KW) => {
                self.for_stmt();
                true
            }
            Some(SyntaxKind::WHILE_KW) => {
                self.while_stmt();
                true
            }
            Some(SyntaxKind::PACKAGE_KW) => {
                self.package_stmt();
                true
            }
            Some(SyntaxKind::USE_KW) => {
                self.use_stmt();
                true
            }
            Some(SyntaxKind::END_KW) | Some(SyntaxKind::DATA_KW) => {
                self.data_section();
                true
            }
            Some(SyntaxKind::R_BRACE) => {
                // ブロック終了なので呼び出し元に知らせる
                false
            }
            Some(_) => {
                // 式文としてパースを試みる
                self.expression_stmt()
            }
            None => false, // EOF
        }
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

    fn var_decl(&mut self) {
        self.var_decl_common(true);
    }

    // Variable declaration as expression (no semicolon expected)
    fn var_decl_expr(&mut self) {
        self.var_decl_common(false);
    }

    // Common logic for variable declarations
    fn var_decl_common(&mut self, expect_semicolon: bool) {
        self.builder.start_node(SyntaxKind::DECLARATION_STMT.into());

        // Variable declaration keyword (my, our, state, local)
        let decl_kind = self.current_kind().unwrap();
        self.bump(); // consume the keyword
        self.skip_trivia();

        // my $var or my ($var, ...)
        if self.at(SyntaxKind::L_PAREN) {
            self.bump(); // (
            self.skip_trivia();

            while !self.at(SyntaxKind::R_PAREN) && !self.at_end() {
                if self.current_kind().map(|k| k.is_sigil()).unwrap_or(false) {
                    // Use qualified parsing for our/local, simple for my/state
                    if matches!(decl_kind, SyntaxKind::OUR_KW | SyntaxKind::LOCAL_KW) {
                        self.parse_variable_qualified();
                    } else {
                        self.parse_variable_simple();
                    }
                } else {
                    self.error("Expected variable in parenthesized list");
                    break; // エラーが発生したらループを抜ける
                }

                self.skip_trivia();

                if self.at(SyntaxKind::COMMA) {
                    self.bump();
                    self.skip_trivia();
                } else if !self.at(SyntaxKind::R_PAREN) {
                    self.error("Expected ',' or ')' in variable list");
                    break; // エラーが発生したらループを抜ける
                }
            }

            self.expect(SyntaxKind::R_PAREN);
        } else if self.current_kind().map(|k| k.is_sigil()).unwrap_or(false) {
            // Use qualified parsing for our/local, simple for my/state
            if matches!(decl_kind, SyntaxKind::OUR_KW | SyntaxKind::LOCAL_KW) {
                self.parse_variable_qualified();
            } else {
                self.parse_variable_simple();
            }
        } else {
            self.error("Expected variable or parenthesized list of variables after variable declaration keyword");
        }

        self.skip_trivia();

        // 初期化式があれば処理
        if self.at(SyntaxKind::EQ) {
            self.bump(); // =
            self.skip_trivia();
            if !self.expression() {
                self.error("Invalid expression in variable assignment");
            }
        }

        self.skip_trivia();
        if expect_semicolon {
            self.expect(SyntaxKind::SEMICOLON);
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

    fn sub_def(&mut self) {
        self.builder.start_node(SyntaxKind::SUB_DEF.into());

        self.expect(SyntaxKind::SUB_KW);
        self.skip_trivia();

        // サブルーチン名（修飾付き識別子も可能）
        self.parse_identifier_or_qualified();
        self.skip_trivia();

        self.block();

        self.builder.finish_node();
    }

    fn package_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::PACKAGE_STMT.into());

        // "package"
        self.expect(SyntaxKind::PACKAGE_KW);
        self.skip_trivia();

        // パッケージ名（修飾付き識別子）
        self.parse_identifier_or_qualified();
        self.skip_trivia();

        // セミコロン
        self.expect(SyntaxKind::SEMICOLON);

        self.builder.finish_node();
    }

    fn use_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::USE_STMT.into());

        // "use"
        self.expect(SyntaxKind::USE_KW);
        self.skip_trivia();

        // モジュール名（修飾付き識別子）
        self.parse_identifier_or_qualified();
        self.skip_trivia();

        // オプション：インポートリスト（qw() など）
        if self.is_at_start_of_expression() {
            self.expression();
            self.skip_trivia();
        }

        // セミコロン
        self.expect(SyntaxKind::SEMICOLON);

        self.builder.finish_node();
    }

    fn for_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::FOR_STMT.into());

        // "for" or "foreach" - already validated by statement()
        self.bump();
        self.skip_trivia();

        // Check what comes next to determine the for loop style:
        // 1. Perl-style: for VAR (LIST) BLOCK - VAR starts with sigil or "my"
        // 2. C-style: for (EXPR) BLOCK - starts with "("

        if self.at(SyntaxKind::L_PAREN) {
            // C-style for loop: for (EXPR) BLOCK
            self.bump(); // (
            self.skip_trivia();

            // Parse the condition/iterator expression
            if !self.expression() {
                self.error("Expected expression in for condition");
            }

            self.skip_trivia();
            self.expect(SyntaxKind::R_PAREN);
        } else {
            // Perl-style for loop: for VAR (LIST) BLOCK
            // Parse the iterator variable (VAR part): my $var, $var, @var, etc.
            self.parse_for_variable();
            self.skip_trivia();

            // List expression in parentheses: (LIST)
            if self.at(SyntaxKind::L_PAREN) {
                self.bump(); // (
                self.skip_trivia();

                // Parse the list expression
                if !self.expression() {
                    self.error("Expected expression in for list");
                }

                self.skip_trivia();
                self.expect(SyntaxKind::R_PAREN);
            } else {
                self.error("Expected '(' after for variable");
            }
        }

        self.skip_trivia();

        // Block
        self.block();

        self.builder.finish_node();
    }

    /// Parse the variable part of a for loop (my $var, $var)
    fn parse_for_variable(&mut self) {
        if self.at_any(&[SyntaxKind::MY_KW, SyntaxKind::OUR_KW, SyntaxKind::LOCAL_KW]) {
            // Variable declaration case - parse as a variable declaration
            self.builder.start_node(SyntaxKind::DECLARATION_STMT.into());

            let decl_kind = self.current_kind().unwrap();
            self.bump(); // consume the keyword
            self.skip_trivia();

            // Parse the variable - must be a scalar
            if self.at(SyntaxKind::DOLLAR) {
                // Use qualified parsing for our/local, simple for my
                if matches!(decl_kind, SyntaxKind::OUR_KW | SyntaxKind::LOCAL_KW) {
                    self.parse_variable_qualified();
                } else {
                    self.parse_variable_simple();
                }
            } else {
                self.error("Expected scalar variable after variable declaration keyword in for loop");
            }

            self.builder.finish_node();
        } else if self.at(SyntaxKind::DOLLAR) {
            // $var case - parse as a variable reference
            self.parse_variable();
        } else {
            self.error("Expected scalar variable or 'my' declaration in for loop");
        }
    }

    fn while_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::WHILE_STMT.into());

        // "while"
        self.expect(SyntaxKind::WHILE_KW);
        self.skip_trivia();

        // Condition expression in parentheses: while (expr)
        if self.at(SyntaxKind::L_PAREN) {
            self.bump(); // (
            self.skip_trivia();

            // Parse the while condition
            if !self.expression() {
                self.error("Expected expression in while condition");
            }

            self.skip_trivia();
            self.expect(SyntaxKind::R_PAREN);
        } else {
            self.error("Expected '(' after 'while'");
        }

        self.skip_trivia();

        // Block
        self.block();

        self.builder.finish_node();
    }

    fn if_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::IF_STMT.into());

        // "if"
        self.expect(SyntaxKind::IF_KW);
        self.skip_trivia();

        // Condition expression in parentheses: if (expr)
        if self.at(SyntaxKind::L_PAREN) {
            self.bump(); // (
            self.skip_trivia();

            // Parse the if condition
            if !self.expression() {
                self.error("Expected expression in if condition");
            }

            self.skip_trivia();
            self.expect(SyntaxKind::R_PAREN);
        } else {
            self.error("Expected '(' after 'if'");
        }

        self.skip_trivia();

        // If block
        self.block();

        self.skip_trivia();

        while self.at(SyntaxKind::ELSIF_KW) {
            self.bump(); // elsif
            self.skip_trivia();

            if self.at(SyntaxKind::L_PAREN) {
                self.bump(); // (
                self.skip_trivia();

                // Parse the if condition
                if !self.expression() {
                    self.error("Expected expression in elsif condition");
                }

                self.skip_trivia();
                self.expect(SyntaxKind::R_PAREN);
            } else {
                self.error("Expected '(' after 'elsif'");
            }

            self.skip_trivia();

            self.block();

            self.skip_trivia();
        }

        // "else"
        if self.at(SyntaxKind::ELSE_KW) {
            self.bump(); // else
            self.skip_trivia();

            // Else block
            self.block();
        }

        self.builder.finish_node();
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

    fn expression_stmt(&mut self) -> bool {
        if !self.is_at_start_of_expression() {
            return false;
        }

        self.builder.start_node(SyntaxKind::STMT.into());
        let success = self.expression();

        if !success {
            // is_at_start_of_expression でチェックしているので、ここに来ることは
            // expressionの実装が不完全な場合のみのはず。
            // 本来は builder.abandon_node() のようなものが望ましいが、
            // GreenNodeBuilder にはないので、エラーノードとして閉じておく。
            self.error("Invalid expression statement");
            self.builder.finish_node();
            return true; // エラーとして消費はしたのでtrue
        }

        // セミコロンは必須ではない（関数呼び出しなどの場合）
        if self.at(SyntaxKind::SEMICOLON) {
            self.bump();
        }

        self.builder.finish_node();
        true
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
                SyntaxKind::MY_KW, // Add variable declaration keywords as start of expression
                SyntaxKind::OUR_KW,
                SyntaxKind::STATE_KW,
                SyntaxKind::LOCAL_KW,
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
        self.logical_or_expr()
    }

    // Logical OR operators: ||
    fn logical_or_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.logical_and_expr() {
            return false;
        }

        while let Some(op) = self.current_kind() {
            if !matches!(op, SyntaxKind::LOGICAL_OR) {
                break;
            }

            self.builder
                .start_node_at(start, SyntaxKind::INFIX_EXPR.into());
            self.bump(); // operator
            self.skip_trivia();
            if !self.logical_and_expr() {
                self.error("Expected expression after logical OR operator");
            }
            self.builder.finish_node();
        }
        true
    }

    // Logical AND operators: &&
    fn logical_and_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.comparison_expr() {
            return false;
        }

        while let Some(op) = self.current_kind() {
            if !matches!(op, SyntaxKind::LOGICAL_AND) {
                break;
            }

            self.builder
                .start_node_at(start, SyntaxKind::INFIX_EXPR.into());
            self.bump(); // operator
            self.skip_trivia();
            if !self.comparison_expr() {
                self.error("Expected expression after logical AND operator");
            }
            self.builder.finish_node();
        }
        true
    }

    // Regex operators: =~ !~
    fn regex_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.method_call_expr() {
            return false;
        }

        while self.at_any(&[SyntaxKind::REGEX_MATCH, SyntaxKind::REGEX_NOT_MATCH]) {
            self.builder
                .start_node_at(start, SyntaxKind::REGEX_EXPR.into());
            self.bump(); // operator
            self.skip_trivia();
            if !self.method_call_expr() {
                self.error("Expected expression after regex operator");
            }
            self.builder.finish_node();
        }
        true
    }

    fn expression_list(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.expression() {
            return false;
        }

        while self.at_any(&[SyntaxKind::COMMA, SyntaxKind::FAT_COMMA]) {
            self.builder
                .start_node_at(start, SyntaxKind::EXPR_LIST.into());
            self.bump(); // , or =>
            self.skip_trivia();

            // Check for trailing comma - if we're at the end of a list context, don't require another expression
            if self.is_at_start_of_expression() && !self.expression() {
                self.error("Expected expression after comma in list");
            }
            // If no expression follows, it's a trailing comma - that's OK
            self.builder.finish_node();
        }
        true
    }

    // Additive operators: + - .
    fn additive_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.multiplicative_expr() {
            return false;
        }

        while self.at_any(&[SyntaxKind::PLUS, SyntaxKind::MINUS]) {
            self.builder
                .start_node_at(start, SyntaxKind::INFIX_EXPR.into());
            self.bump(); // operator
            self.skip_trivia();
            if !self.multiplicative_expr() {
                self.error("Expected expression after additive operator");
            }
            self.builder.finish_node();
        }
        true
    }

    // Comparison operators: < > <= >= == !=
    fn comparison_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.additive_expr() {
            return false;
        }

        while self.at_any(&[
            SyntaxKind::LT,
            SyntaxKind::GT,
            SyntaxKind::LE,
            SyntaxKind::GE,
            SyntaxKind::EQ_EQ,
            SyntaxKind::NE,
        ]) {
            self.builder
                .start_node_at(start, SyntaxKind::INFIX_EXPR.into());
            self.bump(); // operator
            self.skip_trivia();
            if !self.additive_expr() {
                self.error("Expected expression after comparison operator");
            }
            self.builder.finish_node();
        }
        true
    }

    // Multiplicative operators: * / % x
    fn multiplicative_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.regex_expr() {
            return false;
        }

        while self.at_any(&[
            SyntaxKind::STAR,
            SyntaxKind::SLASH,
            SyntaxKind::MODULO,
            SyntaxKind::X,
        ]) {
            self.builder
                .start_node_at(start, SyntaxKind::INFIX_EXPR.into());
            self.bump(); // operator
            self.skip_trivia();
            if !self.regex_expr() {
                self.error("Expected expression after multiplicative operator");
            }
            self.builder.finish_node();
        }
        true
    }

    // Method call expression: expr -> method_name()
    fn method_call_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.primary_expr() {
            return false;
        }

        while self.at(SyntaxKind::ARROW) {
            self.builder
                .start_node_at(start, SyntaxKind::METHOD_CALL_EXPR.into());
            self.bump(); // ->
            self.skip_trivia();

            self.parse_identifier_or_qualified();
            self.skip_trivia();

            if self.at(SyntaxKind::L_PAREN) {
                self.bump();

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
            Some(k) if matches!(k, SyntaxKind::MY_KW | SyntaxKind::OUR_KW | SyntaxKind::STATE_KW | SyntaxKind::LOCAL_KW) => {
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
                    if kind.is_variable()
                        || self.at_any(&[
                            SyntaxKind::NUMBER,
                            SyntaxKind::STRING,
                            SyntaxKind::L_PAREN,
                        ])
                        || kind.is_sigil()
                    {
                        // We have a regular function call, wrap everything in FUNCTION_CALL_EXPR
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
                self.skip_trivia();
            }
            Some(SyntaxKind::AT) => {
                // Special punctuation like $@ - treat as regular variable name
                self.bump();
                self.skip_trivia();
            }
            Some(SyntaxKind::CARET) => {
                // Handle $^ or $^X patterns
                self.bump(); // consume ^
                self.skip_trivia();

                // Check if there's a character after ^
                if self.at(SyntaxKind::IDENT) {
                    // This is $^X pattern where X is an identifier (single char)
                    self.bump();
                    self.skip_trivia();
                }
            }
            Some(SyntaxKind::L_BRACE) => {
                // Handle ${...} syntax (e.g., ${^NAME})
                self.bump(); // consume {
                self.skip_trivia();

                // Check for ^ inside braces
                if self.at(SyntaxKind::CARET) {
                    self.bump(); // consume ^
                    self.skip_trivia();
                }

                // Parse identifier inside braces
                if self.at(SyntaxKind::IDENT) {
                    self.bump();
                    self.skip_trivia();
                }

                // Expect closing brace
                if self.at(SyntaxKind::R_BRACE) {
                    self.bump();
                    self.skip_trivia();
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
                    self.skip_trivia();
                } else {
                    // 識別子を期待（修飾付き識別子も含む）
                    self.parse_identifier_or_qualified();
                }
            }
        }

        self.builder.finish_node();
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
        self.skip_trivia();

        // :: があるかチェック
        if self.at(SyntaxKind::DOUBLE_COLON) {
            // 修飾付き識別子として扱う
            self.builder
                .start_node_at(checkpoint, SyntaxKind::QUALIFIED_IDENT.into());

            // :: の後の部分を処理
            while self.at(SyntaxKind::DOUBLE_COLON) {
                self.bump(); // ::
                self.skip_trivia();

                if self.at(SyntaxKind::IDENT) {
                    self.bump();
                    self.skip_trivia();
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
    fn test_package_qualified_variable_declarations() {
        // our and local should accept package-qualified variable names
        let cases = [
            // our accepts package-qualified names
            ("our $Foo::Bar::var = 1;", true),
            ("our @Namespace::array = (1, 2, 3);", true),
            ("our %Pkg::hash = (a => 1);", true),
            
            // local accepts package-qualified names  
            ("local $Foo::Bar::var = 1;", true),
            ("local @Namespace::array = (1, 2, 3);", true),
            ("local %Pkg::hash = (a => 1);", true),
            
            // my should not accept package-qualified names (should parse but create error)
            ("my $Foo::Bar::var = 1;", false),
            ("my @Namespace::array = (1, 2, 3);", false),
            
            // state should not accept package-qualified names (should parse but create error)
            ("state $Foo::Bar::var = 1;", false), 
            ("state @Namespace::array = (1, 2, 3);", false),
        ];

        for (input, should_succeed) in cases {
            let (green, errors) = parse(input);
            let syntax = PerlNode::new_root(green);
            
            // All inputs should parse structurally
            assert_eq!(syntax.kind(), SyntaxKind::ROOT, "Failed to parse: {}", input);
            
            if should_succeed {
                // our and local should parse without errors for package-qualified names
                assert!(
                    errors.is_empty(), 
                    "Should parse '{}' without errors, but got: {:?}", 
                    input, errors
                );
            } else {
                // my and state should generate errors for package-qualified names
                assert!(
                    !errors.is_empty(),
                    "Should generate parse error for '{}' but didn't",
                    input
                );
            }
        }
    }

    #[test]
    fn test_simple_variable_declarations() {
        // All declaration types should accept simple variable names
        let cases = [
            "my $var = 1;",
            "our $var = 1;", 
            "state $var = 1;",
            "local $var = 1;",
            "my @array = (1, 2, 3);",
            "our @array = (1, 2, 3);",
            "state @array = (1, 2, 3);", 
            "local @array = (1, 2, 3);",
        ];

        for input in cases {
            let (green, errors) = parse(input);
            let syntax = PerlNode::new_root(green);
            
            assert_eq!(syntax.kind(), SyntaxKind::ROOT, "Failed to parse: {}", input);
            assert!(
                errors.is_empty(),
                "Should parse '{}' without errors, but got: {:?}",
                input, errors
            );
        }
    }

    #[test]
    fn test_for_loop_with_package_qualified_variables() {
        // Test for loops with package-qualified variable names
        let cases = [
            // our and local should accept package-qualified names in for loops
            ("for our $Package::var (@list) { print $Package::var; }", true),
            ("for local $Foo::Bar::item (@array) { print $Foo::Bar::item; }", true),
            
            // my should not accept package-qualified names in for loops
            ("for my $Package::var (@list) { print $Package::var; }", false),
            
            // state should not be allowed in for loops at all
            ("for state $var (@list) { print $var; }", false),
            ("for state $Package::var (@list) { print $Package::var; }", false),
        ];

        for (input, should_succeed) in cases {
            let (green, errors) = parse(input);
            let syntax = PerlNode::new_root(green);
            
            // All inputs should parse structurally  
            assert_eq!(syntax.kind(), SyntaxKind::ROOT, "Failed to parse: {}", input);
            
            if should_succeed {
                // our and local should parse without errors for package-qualified names
                assert!(
                    errors.is_empty(), 
                    "Should parse '{}' without errors, but got: {:?}", 
                    input, errors
                );
            } else {
                // my with package-qualified names and state should generate errors
                assert!(
                    !errors.is_empty(),
                    "Should generate parse error for '{}' but didn't",
                    input
                );
            }
        }
    }
}
