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
            Some(SyntaxKind::MY_KW) => {
                self.var_decl();
                true
            }
            Some(SyntaxKind::SUB_KW) => {
                self.sub_def();
                true
            }
            Some(SyntaxKind::FOR_KW) => {
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

    fn var_decl(&mut self) {
        self.builder.start_node(SyntaxKind::DECLARATION_STMT.into());

        // "my"
        self.expect(SyntaxKind::MY_KW);
        self.skip_trivia();

        // 変数名 (my宣言では修飾付き識別子は使用しない)
        if self.current_kind().map(|k| k.is_sigil()).unwrap_or(false) {
            self.parse_variable_simple(); // myでは簡単な変数のみ
        } else {
            self.error("Expected variable after 'my'");
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
        self.expect(SyntaxKind::SEMICOLON);

        self.builder.finish_node();
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

        // "for"
        self.expect(SyntaxKind::FOR_KW);
        self.skip_trivia();

        // Condition/iterator expression in parentheses: for (expr)
        if self.at(SyntaxKind::L_PAREN) {
            self.bump(); // (
            self.skip_trivia();

            // Parse the for condition/iterator
            if !self.expression() {
                self.error("Expected expression in for condition");
            }

            self.skip_trivia();
            self.expect(SyntaxKind::R_PAREN);
        } else {
            self.error("Expected '(' after 'for'");
        }

        self.skip_trivia();

        // Block
        self.block();

        self.builder.finish_node();
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
            matches!(
                kind,
                SyntaxKind::NUMBER
                    | SyntaxKind::STRING
                    | SyntaxKind::IDENT
                    | SyntaxKind::L_PAREN
                    | SyntaxKind::L_BRACE
                    | SyntaxKind::L_BRACKET
                    | SyntaxKind::QW_KW
            ) || kind.is_variable()
                || kind.is_sigil()
        } else {
            false
        }
    }

    fn expression(&mut self) -> bool {
        self.additive_expr()
    }

    // Additive operators: + - .
    fn additive_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.multiplicative_expr() {
            return false;
        }

        while let Some(op) = self.current_kind() {
            if !matches!(op, SyntaxKind::PLUS | SyntaxKind::MINUS) {
                break;
            }

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

    // Multiplicative operators: * / % x
    fn multiplicative_expr(&mut self) -> bool {
        let start = self.builder.checkpoint();
        if !self.primary_expr() {
            return false;
        }

        while let Some(op) = self.current_kind() {
            if !matches!(
                op,
                SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::MODULO | SyntaxKind::X
            ) {
                break;
            }

            self.builder
                .start_node_at(start, SyntaxKind::INFIX_EXPR.into());
            self.bump(); // operator
            self.skip_trivia();
            if !self.primary_expr() {
                self.error("Expected expression after multiplicative operator");
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
            Some(SyntaxKind::NUMBER) | Some(SyntaxKind::STRING) => {
                self.bump();
            }
            Some(kind) if kind.is_variable() => {
                self.bump();
            }
            Some(kind) if kind.is_sigil() => {
                self.parse_variable();
            }
            Some(SyntaxKind::IDENT) => {
                // 修飾付き識別子かもしれないのでparse_identifier_or_qualifiedを使用
                self.parse_identifier_or_qualified();
                self.skip_trivia();

                // 関数呼び出し: identifier の後に引数（変数、括弧など）が続く場合
                while let Some(kind) = self.current_kind() {
                    if kind.is_variable()
                        || kind == SyntaxKind::NUMBER
                        || kind == SyntaxKind::STRING
                        || kind == SyntaxKind::L_PAREN
                    {
                        if kind == SyntaxKind::L_PAREN {
                            // 括弧内の式を処理
                            self.bump(); // (
                            self.skip_trivia();

                            // 括弧内の引数リスト（簡単な実装）
                            self.parse_parenthesized_list();

                            if self.at(SyntaxKind::R_PAREN) {
                                self.bump(); // )
                                self.skip_trivia();
                            }
                        } else {
                            self.bump();
                            self.skip_trivia();
                        }
                    } else {
                        break;
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

        // キー・バリューペアの解析
        while !self.at(SyntaxKind::R_BRACE) && !self.at_end() {
            // キー（識別子、文字列、または数値）
            if self.at_any(&[SyntaxKind::IDENT, SyntaxKind::STRING, SyntaxKind::NUMBER]) {
                self.bump();
            } else {
                self.error("Expected hash key");
                break;
            }

            self.skip_trivia();

            // =>
            if self.at(SyntaxKind::ARROW) {
                self.bump();
            } else {
                self.error("Expected '=>' after hash key");
                break;
            }

            self.skip_trivia();

            // バリュー（式）
            if !self.expression() {
                self.error("Invalid expression in hash value");
                break;
            }

            self.skip_trivia();

            // カンマまたは終了
            if self.at(SyntaxKind::COMMA) {
                self.bump();
                self.skip_trivia();
            } else if !self.at(SyntaxKind::R_BRACE) {
                self.error("Expected ',' or '}' after hash value");
                break;
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn array_ref(&mut self) {
        self.builder.start_node(SyntaxKind::ARRAY_REF.into());

        self.expect(SyntaxKind::L_BRACKET);
        self.skip_trivia();

        // 要素の解析（カンマ区切りの式リスト）
        while !self.at(SyntaxKind::R_BRACKET) && !self.at_end() {
            if !self.expression() {
                self.error("Invalid expression in array reference");
                break;
            }

            self.skip_trivia();

            // カンマまたは終了
            if self.at(SyntaxKind::COMMA) {
                self.bump();
                self.skip_trivia();
            } else if !self.at(SyntaxKind::R_BRACKET) {
                self.error("Expected ',' or ']' after array element");
                break;
            }
        }

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
            _ => {
                self.error("Expected qw() delimiter: (, [, or {");
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

        // 識別子を期待（修飾付き識別子も含む）
        self.parse_identifier_or_qualified();

        self.builder.finish_node();
    }

    /// my宣言専用の変数パース（修飾付き識別子は使用しない）
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

        // 識別子を期待（単純な識別子のみ）
        if self.at(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected identifier after sigil");
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
        while !self.at(SyntaxKind::R_PAREN) && !self.at_end() {
            if !self.expression() {
                break;
            }
            self.skip_trivia();
            if self.at(SyntaxKind::COMMA) {
                self.bump();
                self.skip_trivia();
            } else if !self.at(SyntaxKind::R_PAREN) {
                self.error("Expected ',' or ')' after expression in list");
                break;
            }
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
    fn test_var_decl() {
        let (green, errors) = parse("my $var = 1;");
        assert!(errors.is_empty());

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
    }

    #[test]
    fn test_sub_def() {
        let (green, errors) = parse("sub test { my $x = 1; }");
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
    }

    #[test]
    fn test_hash_literal() {
        let input = "return {}";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // ハッシュリファレンスノードが存在することを確認
        let hash_ref_found = syntax
            .descendants()
            .any(|node| node.kind() == SyntaxKind::HASH_REF);
        assert!(hash_ref_found, "HASH_REF node should be present in AST");
    }

    #[test]
    fn test_sub_with_hash_literal() {
        let input = "sub f { return { } }";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // ハッシュリファレンスノードが存在することを確認
        let hash_ref_found = syntax
            .descendants()
            .any(|node| node.kind() == SyntaxKind::HASH_REF);
        assert!(hash_ref_found, "HASH_REF node should be present in AST");
    }

    #[test]
    fn test_hash_ref_in_assignment() {
        let input = "my $hash_ref = {};";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // ハッシュリファレンスノードが存在することを確認
        let hash_ref_found = syntax
            .descendants()
            .any(|node| node.kind() == SyntaxKind::HASH_REF);
        assert!(
            hash_ref_found,
            "HASH_REF node should be present in variable assignment"
        );
    }

    #[test]
    fn test_hash_ref_with_key_value() {
        let input = "return { a => 1 };";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // ハッシュリファレンスノードが存在することを確認
        let hash_ref_found = syntax
            .descendants()
            .any(|node| node.kind() == SyntaxKind::HASH_REF);
        assert!(
            hash_ref_found,
            "HASH_REF node should be present with key-value pair"
        );
    }

    #[test]
    fn test_sub_with_complex_hash_ref() {
        let input = "sub f { return { a => 1 } }";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // ハッシュリファレンスノードが存在することを確認
        let hash_ref_found = syntax
            .descendants()
            .any(|node| node.kind() == SyntaxKind::HASH_REF);
        assert!(
            hash_ref_found,
            "HASH_REF node should be present in subroutine"
        );
    }

    #[test]
    fn test_multiplicative_operators() {
        let input = "my $result = $a * $b / $c % $d;";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // BINARY_EXPRノードが存在することを確認
        let binary_expr_found = syntax
            .descendants()
            .any(|node| node.kind() == SyntaxKind::INFIX_EXPR);
        assert!(
            binary_expr_found,
            "INFIX_EXPR node should be present for multiplicative operations"
        );
    }

    #[test]
    fn test_operator_precedence() {
        let input = "my $result = $a + $b * $c;";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // 演算子優先度が正しく解析されることを確認（構造的テスト）
        let binary_expr_count = syntax
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::INFIX_EXPR)
            .count();
        assert!(
            binary_expr_count >= 2,
            "Should have at least 2 infix expressions for precedence"
        );
    }

    #[test]
    fn test_x_operator() {
        let input = "my $str = $a x 3;";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // x演算子がBINARY_EXPRとして解析されることを確認
        let binary_expr_found = syntax
            .descendants()
            .any(|node| node.kind() == SyntaxKind::INFIX_EXPR);
        assert!(
            binary_expr_found,
            "INFIX_EXPR node should be present for x operator"
        );
    }

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
    fn test_sigil_separated_variables() {
        // Sigilと識別子が分離されて正しく変数ノードが構築されることを確認
        let input = "my $var = 1; my @arr = 2; my %hash = 3;";
        let (green, errors) = parse(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);

        // 3つの変数宣言ノードが存在することを確認
        let var_decls: Vec<_> = syntax
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::DECLARATION_STMT)
            .collect();
        assert_eq!(var_decls.len(), 3, "Should have 3 variable declarations");

        // 各種類の変数ノードが存在することを確認
        let scalar_vars: Vec<_> = syntax
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::SCALAR_VAR)
            .collect();
        let array_vars: Vec<_> = syntax
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::ARRAY_VAR)
            .collect();
        let hash_vars: Vec<_> = syntax
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::HASH_VAR)
            .collect();

        assert_eq!(scalar_vars.len(), 1, "Should have 1 scalar variable");
        assert_eq!(array_vars.len(), 1, "Should have 1 array variable");
        assert_eq!(hash_vars.len(), 1, "Should have 1 hash variable");
    }

    /// Helper function to parse input and return syntax tree
    fn assert_parses_ok(input: &str) -> PerlNode {
        let (green, errors) = parse(input);
        assert!(
            errors.is_empty(),
            "Parse failed for '{}': {:?}",
            input,
            errors
        );
        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
        syntax
    }

    #[test]
    fn test_package_stmts() {
        let inputs = [
            "package Foo::Bar;",
            "package Foo;",
            "package Foo::Bar::Baz::Qux;",
        ];

        for input in inputs {
            let syntax = assert_parses_ok(input);

            // パッケージ文ノードが存在することを確認
            let package_stmts: Vec<_> = syntax
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::PACKAGE_STMT)
                .collect();
            assert_eq!(
                package_stmts.len(),
                1,
                "Should have 1 package statement for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_qualified_variables() {
        let inputs = [
            "$Foo::Bar::var;",
            "@Foo::Bar::array;",
            "%Foo::Bar::hash;",
            "$Very::Deep::Nested::Package::Name::var;",
        ];

        for input in inputs {
            let syntax = assert_parses_ok(input);

            // 修飾付き識別子が存在することを確認
            let qualified_idents: Vec<_> = syntax
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::QUALIFIED_IDENT)
                .collect();
            assert_eq!(
                qualified_idents.len(),
                1,
                "Should have 1 qualified identifier for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_qualified_function_calls() {
        let inputs = [
            "Foo::Bar::func;", // Without parentheses for now
            "Very::Deep::Nested::function;",
        ];

        for input in inputs {
            let syntax = assert_parses_ok(input);

            // 修飾付き識別子が存在することを確認
            let qualified_idents: Vec<_> = syntax
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::QUALIFIED_IDENT)
                .collect();
            assert_eq!(
                qualified_idents.len(),
                1,
                "Should have 1 qualified identifier for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_qualified_subroutines() {
        let inputs = ["sub Foo::Bar::func { }", "sub Very::Deep::Nested::func { }"];

        for input in inputs {
            let syntax = assert_parses_ok(input);

            // 修飾付き識別子が存在することを確認
            let qualified_idents: Vec<_> = syntax
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::QUALIFIED_IDENT)
                .collect();
            assert_eq!(
                qualified_idents.len(),
                1,
                "Should have 1 qualified identifier for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_my_declarations_remain_simple() {
        // my宣言では修飾付き識別子は使用されないことを確認
        let inputs = [
            "my $var = 1;",
            "my @array;", // Simplified without complex initialization
            "my %hash;",  // Simplified without complex initialization
        ];

        for (i, input) in inputs.iter().enumerate() {
            let (green, errors) = parse(input);
            assert!(
                errors.is_empty(),
                "Test case {} ('{}') failed with errors: {:?}",
                i,
                input,
                errors
            );

            let syntax = PerlNode::new_root(green);
            assert_eq!(syntax.kind(), SyntaxKind::ROOT);

            // 修飾付き識別子が存在しないことを確認
            let qualified_idents: Vec<_> = syntax
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::QUALIFIED_IDENT)
                .collect();
            assert_eq!(
                qualified_idents.len(),
                0,
                "Should have no qualified identifiers in my declarations for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_mixed_qualified_and_simple() {
        let input = "my $var = $Foo::Bar::other_var;";
        let syntax = assert_parses_ok(input);

        // 修飾付き識別子が1つだけ存在することを確認（右辺のみ）
        let qualified_idents: Vec<_> = syntax
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::QUALIFIED_IDENT)
            .collect();
        assert_eq!(
            qualified_idents.len(),
            1,
            "Should have 1 qualified identifier (only on right side)"
        );
    }

    #[test]
    fn test_qw_expressions() {
        let inputs = [
            "qw(all -uninitialized)",
            "qw[strict refs]",
            "qw{one two three}",
            "my @list = qw(a b c);",
        ];

        for input in inputs {
            let syntax = assert_parses_ok(input);

            // QW_EXPRノードが存在することを確認
            let qw_exprs: Vec<_> = syntax
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::QW_EXPR)
                .collect();
            assert_eq!(
                qw_exprs.len(),
                1,
                "Should have 1 qw expression for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_use_warnings_qw() {
        let input = "use warnings qw(all -uninitialized);";
        let syntax = assert_parses_ok(input);

        // USE_STMTノードが存在することを確認
        let use_stmts: Vec<_> = syntax
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::USE_STMT)
            .collect();
        assert_eq!(use_stmts.len(), 1, "Should have 1 use statement");

        // QW_EXPRノードが存在することを確認
        let qw_exprs: Vec<_> = syntax
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::QW_EXPR)
            .collect();
        assert_eq!(
            qw_exprs.len(),
            1,
            "Should have 1 qw expression in use statement"
        );

        // QW_STRINGトークンが存在することを確認
        let qw_strings: Vec<_> = syntax
            .descendants_with_tokens()
            .filter_map(|node_or_token| match node_or_token {
                rowan::NodeOrToken::Token(token) if token.kind() == SyntaxKind::QW_STRING => {
                    Some(token)
                }
                _ => None,
            })
            .collect();

        assert!(
            qw_strings.len() >= 2,
            "Should have at least 2 qw strings (all, -uninitialized)"
        );
    }

    #[test]
    fn test_array_ref() {
        let inputs = ["[1, 2, 3]", "my $arrayref = [a, b, c];"];

        for input in inputs {
            let syntax = assert_parses_ok(input);

            // ARRAY_REFノードが存在することを確認
            let array_refs: Vec<_> = syntax
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::ARRAY_REF)
                .collect();
            assert_eq!(
                array_refs.len(),
                1,
                "Should have 1 array reference for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_for_stmt() {
        let inputs = [
            "for ($i) { }",
            "for (@array) { my $x = 1; }",
            "for ($x) { print $x; }",
        ];

        for input in inputs {
            let syntax = assert_parses_ok(input);

            // FOR_STMTノードが存在することを確認
            let for_stmts: Vec<_> = syntax
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::FOR_STMT)
                .collect();
            assert_eq!(
                for_stmts.len(),
                1,
                "Should have 1 for statement for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_while_stmt() {
        let inputs = [
            "while ($x) { }",
            "while (@array) { my $y = 2; }",
            "while ($condition) { print $value; }",
        ];

        for input in inputs {
            let syntax = assert_parses_ok(input);

            // WHILE_STMTノードが存在することを確認
            let while_stmts: Vec<_> = syntax
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::WHILE_STMT)
                .collect();
            assert_eq!(
                while_stmts.len(),
                1,
                "Should have 1 while statement for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_nested_loops() {
        let input = "for ($i) { while ($j) { my $x = 1; } }";
        let syntax = assert_parses_ok(input);

        // ネストしたループのノードが存在することを確認
        let for_stmts: Vec<_> = syntax
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::FOR_STMT)
            .collect();
        let while_stmts: Vec<_> = syntax
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::WHILE_STMT)
            .collect();

        assert_eq!(for_stmts.len(), 1, "Should have 1 for statement");
        assert_eq!(while_stmts.len(), 1, "Should have 1 while statement");
    }
}
