use crate::SyntaxKind;
use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    // Sigils（変数の型を示すプレフィックス）
    #[token("$")]
    Dollar,

    #[token("@")]
    At,

    // データセクションキーワード (must come before Ident to take precedence)
    #[token("__END__")]
    EndKw,

    #[token("__DATA__")]
    DataKw,

    // 識別子（サブルーチン名など）
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,

    // リテラル
    #[regex(r"[0-9]+(\.[0-9]+)?")]
    Number,

    #[regex(r#""([^"\\]|\\.)*""#)]
    #[regex(r"'([^'\\]|\\.)*'")]
    String,

    // RegexLiteral - handled manually via context-sensitive disambiguation
    RegexLiteral,

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

    // 演算子 (order matters - longer ones first!)
    #[token("=~")]
    RegexMatch,

    #[token("!~")]
    RegexNotMatch,

    #[token("=>")]
    FatComma,

    #[token("->")]
    Arrow,

    #[token(">=")]
    GreaterEqual,

    #[token("<=")]
    LessEqual,

    #[token("==")]
    EqualEqual,

    #[token("!=")]
    NotEqual,

    #[token("&&")]
    LogicalAnd,

    #[token("||")]
    LogicalOr,

    #[token("=")]
    Eq,

    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    // Multiplicative operators
    #[token("*")]
    Star,

    #[token("/", priority = 1)]
    Slash,

    #[token("%")]
    Percent,

    #[token(">")]
    Greater,

    #[token("<")]
    Less,

    #[token("^")]
    Caret,

    // 空白
    #[regex(r"[ \t\f]+")]
    Whitespace,

    // 改行（重要なので個別にトークン化）
    #[regex(r"\r\n|\r|\n")]
    Newline,

    // コメント
    #[regex(r"#[^\r\n]*")]
    Comment,

    // データセクション（__END__ / __DATA__ 以降のすべてのテキスト）
    DataSection,
}

impl Token {
    pub fn to_syntax_kind(&self) -> SyntaxKind {
        match self {
            Token::Dollar => SyntaxKind::DOLLAR,
            Token::At => SyntaxKind::AT,
            Token::Ident => SyntaxKind::IDENT,
            Token::Number => SyntaxKind::NUMBER,
            Token::String => SyntaxKind::STRING,
            Token::RegexLiteral => SyntaxKind::REGEX_LITERAL,
            Token::EndKw => SyntaxKind::END_KW,
            Token::DataKw => SyntaxKind::DATA_KW,
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
            Token::Percent => SyntaxKind::MODULO,
            Token::Greater => SyntaxKind::GT,
            Token::Less => SyntaxKind::LT,
            Token::Caret => SyntaxKind::CARET,
            Token::GreaterEqual => SyntaxKind::GE,
            Token::LessEqual => SyntaxKind::LE,
            Token::EqualEqual => SyntaxKind::EQ_EQ,
            Token::NotEqual => SyntaxKind::NE,
            Token::RegexMatch => SyntaxKind::REGEX_MATCH,
            Token::RegexNotMatch => SyntaxKind::REGEX_NOT_MATCH,
            Token::LogicalAnd => SyntaxKind::LOGICAL_AND,
            Token::LogicalOr => SyntaxKind::LOGICAL_OR,
            Token::Whitespace => SyntaxKind::WHITESPACE,
            Token::Newline => SyntaxKind::WHITESPACE,
            Token::Comment => SyntaxKind::COMMENT,
            Token::DataSection => SyntaxKind::DATA_SECTION,
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
    /// After qw keyword, expecting a delimiter
    QwDelimiter,
    /// In a data section context (after __END__ or __DATA__)
    RawData,
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
        // Special handling for regex literals in ExpectingValue context (but not after qw)
        if self.context == LexerContext::ExpectingValue {
            if let Some(regex_result) = self.try_consume_regex_literal() {
                let (syntax_kind, text) = regex_result;
                if !syntax_kind.is_trivia() {
                    self.update_context(syntax_kind);
                }
                return Some((syntax_kind, text));
            }
        } else if self.context == LexerContext::RawData {
            let remainder = self.logos_lexer.remainder();
            if remainder.is_empty() {
                return None; // No more data to consume
            }
            self.logos_lexer.bump(remainder.len());
            return Some((SyntaxKind::RAW_STRING, remainder));
        }

        match self.logos_lexer.next() {
            Some(Ok(token)) => {
                let text = self.logos_lexer.slice();
                let syntax_kind = self.disambiguate(token, text);

                // Special handling for __END__ and __DATA__: consume everything remaining as data section
                if matches!(syntax_kind, SyntaxKind::END_KW | SyntaxKind::DATA_KW) {
                    self.update_context(syntax_kind);
                    return Some((syntax_kind, text));
                }

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

    /// Consume the entire remaining input as a data section after __END__ or __DATA__
    pub fn consume_data_section(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None;
        }

        // Consume everything remaining as data section, preserving all content
        let data_text = remainder;
        self.logos_lexer.bump(remainder.len());
        Some((SyntaxKind::DATA_SECTION, data_text))
    }

    fn disambiguate(&self, token: Token, text: &str) -> SyntaxKind {
        match token {
            Token::Ident => {
                // 識別子の場合、キーワードかどうかチェック
                match text {
                    "sub" => SyntaxKind::SUB_KW,
                    "my" => SyntaxKind::MY_KW,
                    "our" => SyntaxKind::OUR_KW,
                    "state" => SyntaxKind::STATE_KW,
                    "local" => SyntaxKind::LOCAL_KW,
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
            Token::Slash => {
                // Context-sensitive disambiguation between regex literal and division
                self.disambiguate_slash()
            }
            _ => token.to_syntax_kind(),
        }
    }

    fn disambiguate_percent(&self) -> SyntaxKind {
        match self.context {
            LexerContext::ExpectingValue
            | LexerContext::VariableList
            | LexerContext::QwDelimiter => {
                // When expecting a value or in variable list, % is a sigil for a hash
                // Examples: "my %hash", "{ key => %val }"
                SyntaxKind::PERCENT
            }
            LexerContext::ExpectingOperator => {
                // When expecting an operator, % is the modulo operator
                // Examples: "@array % hash", "$var % other_var", "func() % 2"
                SyntaxKind::MODULO
            }
            LexerContext::RawData => {
                unreachable!("% should not appear in raw data context");
            }
        }
    }

    fn disambiguate_x(&self) -> SyntaxKind {
        match self.context {
            LexerContext::ExpectingValue
            | LexerContext::VariableList
            | LexerContext::QwDelimiter => {
                // When expecting a value or in variable list, x is an identifier
                // Examples: "sub x", "$x", "my $x"
                SyntaxKind::IDENT
            }
            LexerContext::ExpectingOperator => {
                // When expecting an operator, x is the repetition operator
                // Examples: "$str x 3", "'hello' x 2"
                SyntaxKind::X
            }
            LexerContext::RawData => {
                unreachable!("x should not appear in raw data context");
            }
        }
    }

    fn try_consume_regex_literal(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();

        if !remainder.starts_with('/') {
            return None;
        }

        let mut closing_slash_pos: Option<usize> = None;
        let mut escaped = false;

        for (i, c) in remainder.char_indices().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '/' => {
                    closing_slash_pos = Some(i);
                    break;
                }
                '\\' => {
                    escaped = true;
                }
                '\n' => return None,
                _ => {}
            }
        }

        if let Some(pos) = closing_slash_pos {
            let mut end_pos = pos + 1;
            // Consume optional flags
            for c in remainder[end_pos..].chars() {
                if matches!(c, 'g' | 'i' | 'm' | 's' | 'x') {
                    end_pos += c.len_utf8();
                } else {
                    break;
                }
            }

            let text = &remainder[..end_pos];
            self.logos_lexer.bump(end_pos);
            return Some((SyntaxKind::REGEX_LITERAL, text));
        }

        None
    }

    fn disambiguate_slash(&self) -> SyntaxKind {
        match self.context {
            LexerContext::QwDelimiter => {
                // After qw keyword, slash is a delimiter, not regex literal or division
                SyntaxKind::SLASH
            }
            _ => {
                // Slash is always division operator in disambiguate context
                // because regex literals are handled in try_consume_regex_literal
                SyntaxKind::SLASH
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
            SyntaxKind::MY_KW
            | SyntaxKind::OUR_KW
            | SyntaxKind::STATE_KW
            | SyntaxKind::LOCAL_KW => LexerContext::VariableList, // Expects variables or variable lists
            SyntaxKind::FOR_KW => LexerContext::ExpectingValue, // Expects for condition/iterator
            SyntaxKind::FOREACH_KW => LexerContext::ExpectingValue, // Expects foreach condition/iterator
            SyntaxKind::WHILE_KW => LexerContext::ExpectingValue,   // Expects while condition
            SyntaxKind::PACKAGE_KW => LexerContext::ExpectingValue, // Expects package name
            SyntaxKind::QW_KW => LexerContext::QwDelimiter,         // Expects qw delimiter
            SyntaxKind::USE_KW => LexerContext::ExpectingValue,     // Expects module name

            // Data section keywords - after these, normal parsing stops
            SyntaxKind::END_KW | SyntaxKind::DATA_KW => LexerContext::RawData,

            // Operators expect a value next and break out of VariableList context
            SyntaxKind::EQ | SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::ARROW => {
                LexerContext::ExpectingValue
            }
            SyntaxKind::STAR | SyntaxKind::MODULO | SyntaxKind::X => LexerContext::ExpectingValue,
            SyntaxKind::SLASH => {
                // After slash in different contexts
                match self.context {
                    LexerContext::QwDelimiter => LexerContext::ExpectingOperator, // qw/word/ closing delimiter
                    _ => LexerContext::ExpectingValue,
                }
            }
            // Comparison operators
            SyntaxKind::GT
            | SyntaxKind::LT
            | SyntaxKind::GE
            | SyntaxKind::LE
            | SyntaxKind::EQ_EQ
            | SyntaxKind::NE => LexerContext::ExpectingValue,
            SyntaxKind::LOGICAL_AND | SyntaxKind::LOGICAL_OR => LexerContext::ExpectingValue,
            SyntaxKind::REGEX_MATCH | SyntaxKind::REGEX_NOT_MATCH => LexerContext::ExpectingValue,
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
                    LexerContext::QwDelimiter => {
                        // In qw delimiter context, after identifier, stay in same context
                        // (we're inside qw/words/ construct)
                        LexerContext::QwDelimiter
                    }
                    LexerContext::RawData => {
                        unreachable!("IDENT should not appear in raw data context");
                    }
                }
            }

            // Literals expect an operator next
            SyntaxKind::NUMBER | SyntaxKind::STRING | SyntaxKind::REGEX_LITERAL => {
                LexerContext::ExpectingOperator
            }
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
}
