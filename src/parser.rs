use crate::{lexer::Lexer, SyntaxKind};
use rowan::{GreenNode, GreenNodeBuilder, TextRange};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub range: TextRange,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {:?}", self.message, self.range)
    }
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
    current_token: Option<(SyntaxKind, &'a str)>,
    current_pos: usize,
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

        while !self.at_end() {
            if !self.statement() {
                // エラー回復: 次の文の開始まで読み飛ばし
                self.recover_to_statement_boundary();
            }
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
            Some(_) => {
                self.expression_stmt();
                false
            }
            None => false,
        }
    }

    fn var_decl(&mut self) {
        self.builder.start_node(SyntaxKind::VAR_DECL.into());

        // "my"
        self.expect(SyntaxKind::MY_KW);
        self.skip_trivia();

        // 変数名
        if self.at_any(&[
            SyntaxKind::SCALAR_VAR,
            SyntaxKind::ARRAY_VAR,
            SyntaxKind::HASH_VAR,
        ]) {
            self.bump();
        } else {
            self.error("Expected variable after 'my'");
        }

        self.skip_trivia();

        // 初期化式があれば処理
        if self.at(SyntaxKind::EQ) {
            self.bump(); // =
            self.skip_trivia();
            self.expression();
        }

        self.skip_trivia();
        self.expect(SyntaxKind::SEMICOLON);

        self.builder.finish_node();
    }

    fn sub_def(&mut self) {
        self.builder.start_node(SyntaxKind::SUB_DEF.into());

        self.expect(SyntaxKind::SUB_KW);
        self.skip_trivia();

        self.expect(SyntaxKind::IDENT);
        self.skip_trivia();

        self.block();

        self.builder.finish_node();
    }

    fn block(&mut self) {
        self.builder.start_node(SyntaxKind::BLOCK_STMT.into());

        self.expect(SyntaxKind::L_BRACE);
        self.skip_trivia();

        while !self.at(SyntaxKind::R_BRACE) && !self.at_end() {
            if !self.statement() {
                self.recover_to_statement_boundary();
            }
        }

        self.expect(SyntaxKind::R_BRACE);

        self.builder.finish_node();
    }

    fn expression_stmt(&mut self) {
        self.expression();
        self.expect(SyntaxKind::SEMICOLON);
    }

    fn expression(&mut self) {
        self.binary_expr();
    }

    fn binary_expr(&mut self) {
        let start = self.builder.checkpoint();
        self.primary_expr();

        while let Some(op) = self.current_kind() {
            if !op.is_operator() {
                break;
            }

            let _m = self
                .builder
                .start_node_at(start.clone(), SyntaxKind::BINARY_EXPR.into());
            self.bump(); // operator
            self.skip_trivia();
            self.primary_expr();
            self.builder.finish_node();
        }
    }

    fn primary_expr(&mut self) {
        self.skip_trivia();

        match self.current_kind() {
            Some(SyntaxKind::NUMBER) | Some(SyntaxKind::STRING) => {
                self.bump();
            }
            Some(kind) if kind.is_variable() => {
                self.bump();
            }
            Some(SyntaxKind::IDENT) => {
                self.bump();
            }
            _ => {
                self.error("Expected expression");
            }
        }
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
        let range = TextRange::new(
            (self.current_pos as u32).into(),
            ((self.current_pos + self.current_text().map_or(0, |t| t.len())) as u32).into(),
        );

        self.errors.push(ParseError {
            message: message.to_string(),
            range,
        });

        // エラートークンを作成
        if let Some((_, text)) = self.current_token.take() {
            self.builder.token(SyntaxKind::ERROR.into(), text);
            self.current_pos += text.len();
        }
        self.current_token = self.lexer.next_token();
    }

    fn recover_to_statement_boundary(&mut self) {
        while !self.at_end() {
            match self.current_kind() {
                Some(SyntaxKind::SEMICOLON) => {
                    self.bump();
                    break;
                }
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::SUB_KW) | Some(SyntaxKind::MY_KW) => break,
                _ => self.bump(),
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
        assert!(errors.is_empty());

        let syntax = PerlNode::new_root(green);
        assert_eq!(syntax.kind(), SyntaxKind::ROOT);
    }
}
