use crate::SyntaxKind;
use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    // Sigils（変数の型を示すプレフィックス）
    #[token("$")]
    Dollar,

    #[token("@")]
    At,

    // 識別子（サブルーチン名など）
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,

    // リテラル
    #[regex(r"[0-9]+(\.[0-9]+)?")]
    Number,

    #[regex(r#""([^"\\]|\\.)*""#)]
    #[regex(r"'([^'\\]|\\.)*'")]
    String,

    // 記号
    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

    #[token(";")]
    Semicolon,

    #[token(",")]
    Comma,

    #[token("::")]
    DoubleColon,

    // 演算子
    #[token("=")]
    Eq,

    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("->")]
    Arrow,

    #[token("=>")]
    FatComma,

    // Multiplicative operators
    #[token("*")]
    Star,

    #[token("/")]
    Slash,

    #[token("%")]
    Percent,

    // Comparison operators (order matters - longer ones first!)
    #[token(">=")]
    GreaterEqual,

    #[token("<=")]
    LessEqual,

    #[token("==")]
    EqualEqual,

    #[token("!=")]
    NotEqual,

    #[token(">")]
    Greater,

    #[token("<")]
    Less,

    // Logical operators
    #[token("&&")]
    LogicalAnd,

    #[token("||")]
    LogicalOr,

    // 空白
    #[regex(r"[ \t\f]+")]
    Whitespace,

    // 改行（重要なので個別にトークン化）
    #[regex(r"\r\n|\r|\n")]
    Newline,

    // コメント
    #[regex(r"#[^\r\n]*")]
    Comment,
}

impl Token {
    pub fn to_syntax_kind(&self) -> SyntaxKind {
        match self {
            Token::Dollar => SyntaxKind::DOLLAR,
            Token::At => SyntaxKind::AT,
            Token::Percent => SyntaxKind::PERCENT,
            Token::Ident => SyntaxKind::IDENT,
            Token::Number => SyntaxKind::NUMBER,
            Token::String => SyntaxKind::STRING,
            Token::LBrace => SyntaxKind::L_BRACE,
            Token::RBrace => SyntaxKind::R_BRACE,
            Token::LParen => SyntaxKind::L_PAREN,
            Token::RParen => SyntaxKind::R_PAREN,
            Token::LBracket => SyntaxKind::L_BRACKET,
            Token::RBracket => SyntaxKind::R_BRACKET,
            Token::Semicolon => SyntaxKind::SEMICOLON,
            Token::Comma => SyntaxKind::COMMA,
            Token::DoubleColon => SyntaxKind::DOUBLE_COLON,
            Token::Eq => SyntaxKind::EQ,
            Token::Plus => SyntaxKind::PLUS,
            Token::Minus => SyntaxKind::MINUS,
            Token::Arrow => SyntaxKind::ARROW,
            Token::FatComma => SyntaxKind::FAT_COMMA,
            Token::Star => SyntaxKind::STAR,
            Token::Slash => SyntaxKind::SLASH,
            Token::Greater => SyntaxKind::GT,
            Token::Less => SyntaxKind::LT,
            Token::GreaterEqual => SyntaxKind::GE,
            Token::LessEqual => SyntaxKind::LE,
            Token::EqualEqual => SyntaxKind::EQ_EQ,
            Token::NotEqual => SyntaxKind::NE,
            Token::LogicalAnd => SyntaxKind::LOGICAL_AND,
            Token::LogicalOr => SyntaxKind::LOGICAL_OR,
            Token::Whitespace => SyntaxKind::WHITESPACE,
            Token::Newline => SyntaxKind::WHITESPACE,
            Token::Comment => SyntaxKind::COMMENT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LexerContext {
    /// Expecting a value, identifier, or sigil (after keywords, operators, sigils)
    ExpectingValue,
    /// Expecting an operator (after identifiers, numbers, variables)
    ExpectingOperator,
    /// In a variable list context (after a sigil, expecting more variables)
    VariableList,
}

pub struct Lexer<'a> {
    logos_lexer: logos::Lexer<'a, Token>,
    context: LexerContext,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let logos_lexer = Token::lexer(input);

        Self {
            logos_lexer,
            context: LexerContext::ExpectingValue, // Start expecting a value
        }
    }

    pub fn next_token(&mut self) -> Option<(SyntaxKind, &'a str)> {
        match self.logos_lexer.next() {
            Some(Ok(token)) => {
                let text = self.logos_lexer.slice();
                let syntax_kind = self.disambiguate(token, text);

                // Update context based on the token we just processed
                if !syntax_kind.is_trivia() {
                    self.update_context(syntax_kind);
                }

                Some((syntax_kind, text))
            }
            Some(Err(_)) => {
                // エラートークンとして処理
                let text = self.logos_lexer.slice();
                Some((SyntaxKind::ERROR, text))
            }
            None => None,
        }
    }

    fn disambiguate(&self, token: Token, text: &str) -> SyntaxKind {
        match token {
            Token::Ident => {
                // 識別子の場合、キーワードかどうかチェック
                match text {
                    "sub" => SyntaxKind::SUB_KW,
                    "my" => SyntaxKind::MY_KW,
                    "if" => SyntaxKind::IF_KW,
                    "elsif" => SyntaxKind::ELSIF_KW,
                    "else" => SyntaxKind::ELSE_KW,
                    "for" => SyntaxKind::FOR_KW,
                    "foreach" => SyntaxKind::FOREACH_KW,
                    "while" => SyntaxKind::WHILE_KW,
                    "package" => SyntaxKind::PACKAGE_KW,
                    "qw" => SyntaxKind::QW_KW,
                    "use" => SyntaxKind::USE_KW,
                    "x" => self.disambiguate_x(),
                    _ => SyntaxKind::IDENT,
                }
            }
            Token::Percent => {
                // % の場合、文脈によって sigil か modulo operator かを判定
                self.disambiguate_percent()
            }
            _ => token.to_syntax_kind(),
        }
    }

    fn disambiguate_percent(&self) -> SyntaxKind {
        match self.context {
            LexerContext::ExpectingValue | LexerContext::VariableList => {
                // When expecting a value or in variable list, % is a sigil for a hash
                // Examples: "my %hash", "{ key => %val }"
                SyntaxKind::PERCENT
            }
            LexerContext::ExpectingOperator => {
                // When expecting an operator, % is the modulo operator
                // Examples: "@array % hash", "$var % other_var", "func() % 2"
                SyntaxKind::MODULO
            }
        }
    }

    fn disambiguate_x(&self) -> SyntaxKind {
        match self.context {
            LexerContext::ExpectingValue | LexerContext::VariableList => {
                // When expecting a value or in variable list, x is an identifier
                // Examples: "sub x", "$x", "my $x"
                SyntaxKind::IDENT
            }
            LexerContext::ExpectingOperator => {
                // When expecting an operator, x is the repetition operator
                // Examples: "$str x 3", "'hello' x 2"
                SyntaxKind::X
            }
        }
    }

    /// Update the lexer context based on the token we just processed
    fn update_context(&mut self, syntax_kind: SyntaxKind) {
        self.context = match syntax_kind {
            // Sigils: start VariableList context
            SyntaxKind::DOLLAR | SyntaxKind::AT | SyntaxKind::PERCENT => LexerContext::VariableList,

            // Keywords with different expectations
            SyntaxKind::SUB_KW => LexerContext::ExpectingValue, // Expects identifier (bareword)
            SyntaxKind::MY_KW => LexerContext::VariableList, // Expects variables or variable lists
            SyntaxKind::FOR_KW => LexerContext::ExpectingValue, // Expects for condition/iterator
            SyntaxKind::FOREACH_KW => LexerContext::ExpectingValue, // Expects foreach condition/iterator
            SyntaxKind::WHILE_KW => LexerContext::ExpectingValue,   // Expects while condition
            SyntaxKind::PACKAGE_KW => LexerContext::ExpectingValue, // Expects package name
            SyntaxKind::QW_KW => LexerContext::ExpectingValue,      // Expects qw(...) parentheses
            SyntaxKind::USE_KW => LexerContext::ExpectingValue,     // Expects module name

            // Operators expect a value next and break out of VariableList context
            SyntaxKind::EQ | SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::ARROW => {
                LexerContext::ExpectingValue
            }
            SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::MODULO | SyntaxKind::X => {
                LexerContext::ExpectingValue
            }
            // Comparison operators
            SyntaxKind::GT
            | SyntaxKind::LT
            | SyntaxKind::GE
            | SyntaxKind::LE
            | SyntaxKind::EQ_EQ
            | SyntaxKind::NE => LexerContext::ExpectingValue,
            SyntaxKind::LOGICAL_AND | SyntaxKind::LOGICAL_OR => LexerContext::ExpectingValue,
            SyntaxKind::L_PAREN | SyntaxKind::L_BRACE | SyntaxKind::L_BRACKET => {
                LexerContext::ExpectingValue
            }
            SyntaxKind::COMMA => LexerContext::ExpectingValue,

            // IDENT needs special handling based on current context
            SyntaxKind::IDENT => {
                match self.context {
                    LexerContext::VariableList => {
                        // Transition out of VariableList after first identifier
                        // This makes $ followed by identifier transition to ExpectingOperator
                        // which will correctly handle % as modulo in expressions
                        LexerContext::ExpectingOperator
                    }
                    LexerContext::ExpectingValue => {
                        // If we're expecting a value and get an identifier,
                        // we now expect an operator (normal expression)
                        LexerContext::ExpectingOperator
                    }
                    LexerContext::ExpectingOperator => {
                        // This shouldn't happen, but if it does, expect operator
                        LexerContext::ExpectingOperator
                    }
                }
            }

            // Literals expect an operator next
            SyntaxKind::NUMBER | SyntaxKind::STRING => LexerContext::ExpectingOperator,
            SyntaxKind::R_PAREN | SyntaxKind::R_BRACE | SyntaxKind::R_BRACKET => {
                LexerContext::ExpectingOperator
            }

            // Statement terminators reset to expecting value
            SyntaxKind::SEMICOLON => LexerContext::ExpectingValue,

            // Keep current context for other tokens
            _ => self.context,
        };
    }

    pub fn span(&self) -> std::ops::Range<usize> {
        self.logos_lexer.span()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let mut lexer = Lexer::new("my $var = 1;");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "var")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::EQ, "=")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::NUMBER, "1")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SEMICOLON, ";")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_keywords() {
        let mut lexer = Lexer::new("sub my if else for foreach qw use elsif");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IF_KW, "if")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::ELSE_KW, "else")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::FOR_KW, "for")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(
            lexer.next_token(),
            Some((SyntaxKind::FOREACH_KW, "foreach"))
        );
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::USE_KW, "use")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::ELSIF_KW, "elsif")));
    }

    #[test]
    fn test_variables() {
        let mut lexer = Lexer::new("$scalar @array % hash");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "scalar")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::AT, "@")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "array")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MODULO, "%")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "hash")));
    }

    #[test]
    fn test_fat_comma() {
        let mut lexer = Lexer::new("a => 1");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "a")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::FAT_COMMA, "=>")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::NUMBER, "1")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_multiplicative_operators() {
        let mut lexer = Lexer::new("a * b / c % d x 3");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "a")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STAR, "*")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "b")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SLASH, "/")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "c")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MODULO, "%")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "d")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::X, "x")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::NUMBER, "3")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_sigil_tokens() {
        let mut lexer = Lexer::new("$ @ %");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::AT, "@")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::PERCENT, "%")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_percent_modulo_vs_sigil() {
        // Test the critical case mentioned by Gemini: $var % other_var should be modulo
        let mut lexer = Lexer::new("$var % other_var");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "var")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MODULO, "%"))); // Should be MODULO, not PERCENT
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "other_var")));
    }

    #[test]
    fn test_x_after_sub_keyword() {
        // Test the case mentioned by Gemini: sub x { ... } where x should be IDENT
        let mut lexer = Lexer::new("sub x {");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "x"))); // Should be IDENT, not X
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::L_BRACE, "{")));
    }

    #[test]
    fn test_array_modulo_expression() {
        // Test that "@array % hash" correctly identifies % as modulo operator
        let mut lexer = Lexer::new("@array % hash");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::AT, "@")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "array")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MODULO, "%"))); // Should be MODULO (operator)
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "hash")));
    }

    #[test]
    fn test_hash_declaration() {
        // Test that "my %hash" correctly identifies % as sigil
        let mut lexer = Lexer::new("my %hash");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::PERCENT, "%"))); // Should be PERCENT (sigil)
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "hash")));
    }

    #[test]
    fn test_package_keyword() {
        let mut lexer = Lexer::new("package Foo::Bar;");

        assert_eq!(
            lexer.next_token(),
            Some((SyntaxKind::PACKAGE_KW, "package"))
        );
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "Foo")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOUBLE_COLON, "::")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "Bar")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SEMICOLON, ";")));
    }

    #[test]
    fn test_logical_operators() {
        let mut lexer = Lexer::new("$a && $b || $c");

        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "a")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::LOGICAL_AND, "&&")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "b")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::LOGICAL_OR, "||")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "c")));
        assert_eq!(lexer.next_token(), None);
    }
}
