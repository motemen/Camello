use crate::SyntaxKind;
use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    // Sigils（変数の型を示すプレフィックス）
    #[token("$")]
    Dollar,

    #[token("@")]
    At,

    // Reference operator
    #[token("\\")]
    Backslash,

    // Ampersand (for function references)
    #[token("&")]
    Ampersand,

    // データセクションキーワード (must come before Ident to take precedence)
    #[token("__END__")]
    EndKw,

    #[token("__DATA__")]
    DataKw,

    // POD commands (must be at line start)
    // Note: POD detection is handled manually in lexer due to line-start requirement
    PodCommand,

    // 識別子（サブルーチン名など）
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,

    // リテラル
    #[regex(r"[0-9]+(\.[0-9]+)?")]
    Number,

    #[regex(r#""([^"\\]|\\.)*""#)]
    #[regex(r"'([^'\\]|\\.)*'")]
    String,

    // Version literal (v1.23, v5.008_001, etc.)
    #[regex(r"v[0-9]+(\.[0-9_]+)*")]
    Version,

    // Bare version number (5.24.1, 5.024_001, etc. - contextually determined)
    // Matches numbers with either multiple dots OR underscores in version parts
    #[regex(r"[0-9]+(\.[0-9]+){2,}|[0-9]+\.[0-9_]*_[0-9_]*")]
    BareVersion,

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

    // Ternary operator tokens
    #[token("?")]
    QuestionMark,

    #[token(":")]
    Colon,

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

    #[token("!")]
    LogicalNot,

    #[token("//")]
    DefinedOr,

    #[token("<=>")]
    Spaceship,

    #[token("=")]
    Eq,

    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token(".")]
    Dot,

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
            Token::Backslash => SyntaxKind::BACKSLASH,
            Token::Ampersand => SyntaxKind::AMPERSAND,
            Token::Ident => SyntaxKind::IDENT,
            Token::Number => SyntaxKind::NUMBER,
            Token::String => SyntaxKind::STRING,
            Token::Version => SyntaxKind::VERSION,
            Token::BareVersion => SyntaxKind::BARE_VERSION,
            Token::RegexLiteral => SyntaxKind::REGEX_LITERAL,
            Token::EndKw => SyntaxKind::END_KW,
            Token::DataKw => SyntaxKind::DATA_KW,
            Token::PodCommand => SyntaxKind::POD_CONTENT, // Not used anymore
            Token::LBrace => SyntaxKind::L_BRACE,
            Token::RBrace => SyntaxKind::R_BRACE,
            Token::LParen => SyntaxKind::L_PAREN,
            Token::RParen => SyntaxKind::R_PAREN,
            Token::LBracket => SyntaxKind::L_BRACKET,
            Token::RBracket => SyntaxKind::R_BRACKET,
            Token::Semicolon => SyntaxKind::SEMICOLON,
            Token::Comma => SyntaxKind::COMMA,
            Token::DoubleColon => SyntaxKind::DOUBLE_COLON,
            Token::QuestionMark => SyntaxKind::QUESTION_MARK,
            Token::Colon => SyntaxKind::COLON,
            Token::Eq => SyntaxKind::EQ,
            Token::Plus => SyntaxKind::PLUS,
            Token::Minus => SyntaxKind::MINUS,
            Token::Dot => SyntaxKind::DOT,
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
            Token::LogicalNot => SyntaxKind::LOGICAL_NOT,
            Token::DefinedOr => SyntaxKind::DEFINED_OR,
            Token::Spaceship => SyntaxKind::SPACESHIP,
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
    /// Inside a quote-like operator (q, qq, qx, s, etc.), expecting a delimiter or content.
    QlikeDelimiter,
    /// After qw keyword, expecting opening delimiter.
    QwDelimiter,
    /// Inside qw() content, parsing whitespace-separated words.
    QwContent,
    /// In a data section context (after __END__ or __DATA__)
    RawData,
}

pub struct Lexer<'a> {
    logos_lexer: logos::Lexer<'a, Token>,
    context: LexerContext,
    at_line_start: bool, // Track if we're at the start of a line for POD detection
}

impl<'a> Clone for Lexer<'a> {
    fn clone(&self) -> Self {
        Self {
            logos_lexer: self.logos_lexer.clone(),
            context: self.context,
            at_line_start: self.at_line_start,
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = (SyntaxKind, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let logos_lexer = Token::lexer(input);

        Self {
            logos_lexer,
            context: LexerContext::ExpectingValue, // Start expecting a value
            at_line_start: true,                   // Start at beginning of input (line start)
        }
    }

    /// Handle POD content and RawData modes
    fn try_handle_pod_and_raw_data(&mut self) -> Option<(SyntaxKind, &'a str)> {
        // Check for POD start at line start
        if self.at_line_start {
            // Check for standalone =cut first (error case)
            if let Some(cut_result) = self.try_consume_standalone_cut() {
                let (syntax_kind, text) = cut_result;
                self.at_line_start = false;
                return Some((syntax_kind, text));
            }
            // Check for POD start - this will consume the entire POD block
            if let Some(pod_block) = self.try_consume_pod_content() {
                let (syntax_kind, text) = pod_block;
                self.at_line_start = false;
                return Some((syntax_kind, text));
            }
        }

        // Handle raw data mode
        if self.context == LexerContext::RawData {
            let remainder = self.logos_lexer.remainder();
            if remainder.is_empty() {
                return None; // No more data to consume
            }
            self.logos_lexer.bump(remainder.len());
            return Some((SyntaxKind::RAW_STRING, remainder));
        }

        None
    }

    /// Handle special token parsing in ExpectingValue context
    fn try_handle_expecting_value_tokens(&mut self) -> Option<(SyntaxKind, &'a str)> {
        if self.context != LexerContext::ExpectingValue {
            return None;
        }

        // Array of token consumers to try in order
        let consumers = [
            Self::try_consume_file_test_op,
            Self::try_consume_regex_literal,
            Self::try_consume_io_operator,
        ];

        for consumer in consumers {
            if let Some(result) = consumer(self) {
                let (syntax_kind, text) = result;
                self.update_context(syntax_kind);
                self.update_line_position(text);
                return Some((syntax_kind, text));
            }
        }

        None
    }

    pub fn next_token(&mut self) -> Option<(SyntaxKind, &'a str)> {
        // Handle POD content mode and RawData mode first
        if let Some(result) = self.try_handle_pod_and_raw_data() {
            return Some(result);
        }

        // Handle qw content mode - parse whitespace-separated words
        if self.context == LexerContext::QwContent {
            if let Some(qw_result) = self.try_consume_qw_content() {
                return Some(qw_result);
            }
        }

        // Handle quote-like delimiter mode - parse delimiters for q/qq/qx/etc.
        if self.context == LexerContext::QlikeDelimiter
            || self.context == LexerContext::QwDelimiter
            || self.context == LexerContext::QwContent
        {
            if let Some((kind, text)) = self.try_consume_quote_delimiter() {
                // Update context based on the current context
                match self.context {
                    LexerContext::QwDelimiter => {
                        self.context = LexerContext::QwContent;
                    }
                    LexerContext::QwContent => {
                        // Closing delimiter in QwContent means end of qw expression
                        self.context = LexerContext::ExpectingOperator;
                    }
                    LexerContext::QlikeDelimiter => {
                        // For q-like expressions, we'll handle the context in the parser
                        // since they have different content handling
                    }
                    _ => {}
                }
                return Some((kind, text));
            }
        }

        // Handle special tokens in ExpectingValue context
        if let Some(result) = self.try_handle_expecting_value_tokens() {
            return Some(result);
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
                // Special case for built-in functions - they expect values as arguments
                if syntax_kind == SyntaxKind::IDENT && self.is_builtin_function(text) {
                    self.context = LexerContext::ExpectingValue;
                } else {
                    self.update_context(syntax_kind);
                }

                // Track line position for POD detection
                self.update_line_position(text);

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
                    "unless" => SyntaxKind::UNLESS_KW,
                    "elsif" => SyntaxKind::ELSIF_KW,
                    "else" => SyntaxKind::ELSE_KW,
                    "for" => SyntaxKind::FOR_KW,
                    "foreach" => SyntaxKind::FOREACH_KW,
                    "while" => SyntaxKind::WHILE_KW,
                    "package" => SyntaxKind::PACKAGE_KW,
                    "qw" => SyntaxKind::QW_KW,
                    "q" => SyntaxKind::Q_KW,
                    "qq" => SyntaxKind::QQ_KW,
                    "qx" => SyntaxKind::QX_KW,
                    "m" => SyntaxKind::M_KW,
                    "qr" => SyntaxKind::QR_KW,
                    "s" => self.disambiguate_s(),
                    "tr" => self.disambiguate_tr(),
                    "y" => self.disambiguate_y(),
                    "use" => SyntaxKind::USE_KW,
                    "return" => SyntaxKind::RETURN_KW,
                    "x" => self.disambiguate_x(),
                    "eq" => self.disambiguate_str_op("eq"),
                    "ne" => self.disambiguate_str_op("ne"),
                    "gt" => self.disambiguate_str_op("gt"),
                    "lt" => self.disambiguate_str_op("lt"),
                    "ge" => self.disambiguate_str_op("ge"),
                    "le" => self.disambiguate_str_op("le"),
                    "cmp" => self.disambiguate_str_op("cmp"),
                    "not" => self.disambiguate_logical_op("not"),
                    "and" => self.disambiguate_logical_op("and"),
                    "or" => self.disambiguate_logical_op("or"),
                    "xor" => self.disambiguate_logical_op("xor"),
                    _ => SyntaxKind::IDENT,
                }
            }
            Token::Percent => {
                // % の場合、文脈によって sigil か modulo operator かを判定
                self.disambiguate_percent()
            }
            Token::Star => {
                // * の場合、文脈によって typeglob sigil か multiplication operator かを判定
                self.disambiguate_star()
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
            | LexerContext::QlikeDelimiter
            | LexerContext::QwDelimiter
            | LexerContext::QwContent => {
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
                // Handle gracefully instead of panicking
                SyntaxKind::MODULO
            }
        }
    }

    fn disambiguate_star(&self) -> SyntaxKind {
        match self.context {
            LexerContext::ExpectingValue
            | LexerContext::VariableList
            | LexerContext::QlikeDelimiter
            | LexerContext::QwDelimiter
            | LexerContext::QwContent => {
                // When expecting a value or in variable list, * is a typeglob sigil
                // Examples: "my *glob", "*{$name}", "*STDIN"
                SyntaxKind::ASTERISK
            }
            LexerContext::ExpectingOperator => {
                // When expecting an operator, * is the multiplication operator
                // Examples: "$a * $b", "func() * 2"
                SyntaxKind::STAR
            }
            LexerContext::RawData => {
                unreachable!("* should not appear in RawData context");
            }
        }
    }

    fn disambiguate_str_op(&self, op: &str) -> SyntaxKind {
        match self.context {
            LexerContext::ExpectingOperator => {
                // When expecting an operator, eq/ne are string comparison operators
                match op {
                    "eq" => SyntaxKind::STR_EQ,
                    "ne" => SyntaxKind::STR_NE,
                    "gt" => SyntaxKind::STR_GT,
                    "lt" => SyntaxKind::STR_LT,
                    "ge" => SyntaxKind::STR_GE,
                    "le" => SyntaxKind::STR_LE,
                    "cmp" => SyntaxKind::STR_CMP,
                    _ => SyntaxKind::IDENT, // Handle unknown ops gracefully
                }
            }
            _ => {
                // In other contexts, they are identifiers
                // Examples: "sub eq", "my $ne"
                SyntaxKind::IDENT
            }
        }
    }

    fn disambiguate_x(&self) -> SyntaxKind {
        match self.context {
            LexerContext::ExpectingValue
            | LexerContext::VariableList
            | LexerContext::QlikeDelimiter
            | LexerContext::QwDelimiter
            | LexerContext::QwContent => {
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
                // Handle gracefully instead of panicking
                SyntaxKind::IDENT
            }
        }
    }

    fn disambiguate_s(&self) -> SyntaxKind {
        match self.context {
            LexerContext::VariableList => {
                // When in variable list context (after a sigil), s is an identifier
                // Examples: "$s", "my $s", "@s"
                SyntaxKind::IDENT
            }
            LexerContext::QwDelimiter | LexerContext::QwContent => {
                // In qw contexts, s is an identifier
                SyntaxKind::IDENT
            }
            LexerContext::ExpectingValue => {
                // Look ahead to determine if this is s/// substitution or a bareword function call
                // Gemini's suggestion: check for alphanumeric after optional whitespace
                let remainder = self.logos_lexer.remainder();

                // Check what follows 's' after optional whitespace
                let chars = remainder.chars();
                for c in chars {
                    if c.is_whitespace() {
                        continue;
                    }

                    // If first non-whitespace char is alphanumeric or sigil, it's likely a function call
                    if c.is_alphanumeric() || c == '$' || c == '@' || c == '%' {
                        return SyntaxKind::IDENT;
                    } else {
                        // Otherwise, it's likely substitution
                        return SyntaxKind::S_KW;
                    }
                }

                // If we reach end of input after 's', assume function call
                SyntaxKind::IDENT
            }
            LexerContext::ExpectingOperator => {
                // When expecting an operator, s is the substitution operator
                // Examples: "$str s/old/new/"
                SyntaxKind::S_KW
            }
            LexerContext::QlikeDelimiter => {
                // In q-like delimiter context, s is the substitution operator
                SyntaxKind::S_KW
            }
            LexerContext::RawData => {
                unreachable!("s should not appear in RawData context");
            }
        }
    }

    fn disambiguate_logical_op(&self, op: &str) -> SyntaxKind {
        match self.context {
            LexerContext::ExpectingOperator => {
                // When expecting an operator, not/and/or/xor are logical operators
                match op {
                    "not" => SyntaxKind::NOT_KW,
                    "and" => SyntaxKind::AND_KW,
                    "or" => SyntaxKind::OR_KW,
                    "xor" => SyntaxKind::XOR_KW,
                    _ => SyntaxKind::IDENT, // Handle unknown ops gracefully
                }
            }
            _ => {
                // In other contexts, they are identifiers
                // Examples: "sub not", "my $and", "or die"
                SyntaxKind::IDENT
            }
        }
    }

    fn disambiguate_tr(&self) -> SyntaxKind {
        match self.context {
            LexerContext::VariableList => {
                // When in variable list context (after a sigil), tr is an identifier
                // Examples: "$tr", "my $tr", "@tr"
                SyntaxKind::IDENT
            }
            LexerContext::QwDelimiter | LexerContext::QwContent => {
                // In qw contexts, tr is an identifier
                SyntaxKind::IDENT
            }
            LexerContext::ExpectingValue => {
                // When expecting a value, check what follows tr
                let remainder = self.logos_lexer.remainder();

                // Skip whitespace to see what comes next
                for c in remainder.chars() {
                    if c.is_whitespace() {
                        continue;
                    }
                    // If first non-whitespace char is alphanumeric or sigil, it's likely a function call
                    if c.is_alphanumeric() || c == '$' || c == '@' || c == '%' {
                        return SyntaxKind::IDENT;
                    } else if matches!(c, '/' | '(' | '[') {
                        // Definitely tr operator delimiters
                        return SyntaxKind::TR_KW;
                    } else if c == '{' {
                        // Special case: { could be either a tr delimiter or a block start
                        // Simple heuristic: if we can find pattern like {content}{content}, it's likely tr operator
                        if remainder.matches('{').count() >= 2 {
                            return SyntaxKind::TR_KW;
                        } else {
                            // Only one brace group, likely a function block
                            return SyntaxKind::IDENT;
                        }
                    } else {
                        // For other non-alphanumeric characters, it's likely an identifier
                        return SyntaxKind::IDENT;
                    }
                }

                // If we reach end of input after 'tr', assume identifier
                SyntaxKind::IDENT
            }
            LexerContext::ExpectingOperator => {
                // When expecting an operator, tr is the transliteration operator
                // Examples: "$str tr/a-z/A-Z/"
                SyntaxKind::TR_KW
            }
            LexerContext::QlikeDelimiter => {
                // In q-like delimiter context, tr is the transliteration operator
                SyntaxKind::TR_KW
            }
            LexerContext::RawData => {
                unreachable!("tr should not appear in RawData context");
            }
        }
    }

    fn disambiguate_y(&self) -> SyntaxKind {
        // y is an alias for tr, so use the same logic but return Y_KW
        match self.context {
            LexerContext::VariableList => {
                // When in variable list context (after a sigil), y is an identifier
                // Examples: "$y", "my $y", "@y"
                SyntaxKind::IDENT
            }
            LexerContext::QwDelimiter | LexerContext::QwContent => {
                // In qw contexts, y is an identifier
                SyntaxKind::IDENT
            }
            LexerContext::ExpectingValue => {
                // When expecting a value, check what follows y
                let remainder = self.logos_lexer.remainder();

                // Skip whitespace to see what comes next
                for c in remainder.chars() {
                    if c.is_whitespace() {
                        continue;
                    }
                    // If first non-whitespace char is alphanumeric or sigil, it's likely a function call
                    if c.is_alphanumeric() || c == '$' || c == '@' || c == '%' {
                        return SyntaxKind::IDENT;
                    } else if matches!(c, '/' | '(' | '[') {
                        // Definitely y operator delimiters
                        return SyntaxKind::Y_KW;
                    } else if c == '{' {
                        // Special case: { could be either a y delimiter or a block start
                        // Simple heuristic: if we can find pattern like {content}{content}, it's likely y operator
                        if remainder.matches('{').count() >= 2 {
                            return SyntaxKind::Y_KW;
                        } else {
                            // Only one brace group, likely a function block
                            return SyntaxKind::IDENT;
                        }
                    } else {
                        // For other non-alphanumeric characters, it's likely an identifier
                        return SyntaxKind::IDENT;
                    }
                }

                // If we reach end of input after 'y', assume identifier
                SyntaxKind::IDENT
            }
            LexerContext::ExpectingOperator => {
                // When expecting an operator, y is the transliteration operator
                // Examples: "$str y/a-z/A-Z/"
                SyntaxKind::Y_KW
            }
            LexerContext::QlikeDelimiter => {
                // In q-like delimiter context, y is the transliteration operator
                SyntaxKind::Y_KW
            }
            LexerContext::RawData => {
                unreachable!("y should not appear in RawData context");
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

    fn try_consume_io_operator(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();

        if !remainder.starts_with('<') {
            return None;
        }

        let mut closing_angle_pos: Option<usize> = None;

        // Find the closing '>'
        for (i, c) in remainder.char_indices().skip(1) {
            match c {
                '>' => {
                    closing_angle_pos = Some(i);
                    break;
                }
                '\n' => return None, // I/O operators don't span lines
                _ => {}
            }
        }

        if let Some(pos) = closing_angle_pos {
            let text = &remainder[..pos + 1];
            self.logos_lexer.bump(text.len());
            return Some((SyntaxKind::IO_EXPR, text));
        }

        None
    }

    fn try_consume_file_test_op(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if remainder.starts_with('-') {
            if let Some(c) = remainder.chars().nth(1) {
                if c.is_alphabetic() {
                    let text = &remainder[..2];
                    self.logos_lexer.bump(2);
                    return Some((SyntaxKind::FILE_TEST_OP, text));
                }
            }
        }
        None
    }

    fn disambiguate_slash(&self) -> SyntaxKind {
        match self.context {
            LexerContext::QlikeDelimiter => {
                // After q-string family keywords, slash is a delimiter, not regex literal or division
                SyntaxKind::SLASH
            }
            _ => {
                // Slash is always division operator in disambiguate context
                // because regex literals are handled in try_consume_regex_literal
                SyntaxKind::SLASH
            }
        }
    }

    /// Check if the given identifier text represents a built-in function
    /// Built-in functions expect values as arguments, so they should transition to ExpectingValue context
    fn is_builtin_function(&self, text: &str) -> bool {
        matches!(
            text,
            // Core functions that commonly take regex patterns
            "split" | "grep" | "map" | "sort" |
            // I/O functions
            "print" | "printf" | "say" | "warn" | "die" |
            // Array/Hash functions
            "push" | "pop" | "shift" | "unshift" | "splice" |
            "keys" | "values" | "each" | "exists" | "delete" |
            // String functions
            "length" | "substr" | "index" | "rindex" | "chomp" | "chop" |
            // File operations
            "open" | "close" | "read" | "write" | "seek" | "tell" |
            // Other common functions
            "defined" | "undef" | "ref" | "bless" | "tie" | "untie" |
            "eval" | "exec" | "system" | "sleep" | "exit"
        )
    }

    fn is_sigil(&self, kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::DOLLAR | SyntaxKind::AT | SyntaxKind::PERCENT | SyntaxKind::ASTERISK
        )
    }

    fn is_keyword(&self, kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::SUB_KW
                | SyntaxKind::MY_KW
                | SyntaxKind::OUR_KW
                | SyntaxKind::STATE_KW
                | SyntaxKind::LOCAL_KW
                | SyntaxKind::FOR_KW
                | SyntaxKind::FOREACH_KW
                | SyntaxKind::IF_KW
                | SyntaxKind::UNLESS_KW
                | SyntaxKind::WHILE_KW
                | SyntaxKind::PACKAGE_KW
                | SyntaxKind::USE_KW
                | SyntaxKind::RETURN_KW
                | SyntaxKind::QW_KW
                | SyntaxKind::Q_KW
                | SyntaxKind::QQ_KW
                | SyntaxKind::QX_KW
                | SyntaxKind::M_KW
                | SyntaxKind::QR_KW
                | SyntaxKind::S_KW
                | SyntaxKind::TR_KW
                | SyntaxKind::Y_KW
                | SyntaxKind::END_KW
                | SyntaxKind::DATA_KW
                | SyntaxKind::BACKSLASH
        )
    }

    fn is_operator(&self, kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::EQ
                | SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::DOT
                | SyntaxKind::ARROW
                | SyntaxKind::STAR
                | SyntaxKind::MODULO
                | SyntaxKind::X
                | SyntaxKind::SLASH
                | SyntaxKind::GT
                | SyntaxKind::LT
                | SyntaxKind::GE
                | SyntaxKind::LE
                | SyntaxKind::EQ_EQ
                | SyntaxKind::NE
                | SyntaxKind::STR_EQ
                | SyntaxKind::STR_NE
                | SyntaxKind::STR_GT
                | SyntaxKind::STR_LT
                | SyntaxKind::STR_GE
                | SyntaxKind::STR_LE
                | SyntaxKind::STR_CMP
                | SyntaxKind::LOGICAL_AND
                | SyntaxKind::LOGICAL_OR
                | SyntaxKind::LOGICAL_NOT
                | SyntaxKind::NOT_KW
                | SyntaxKind::AND_KW
                | SyntaxKind::OR_KW
                | SyntaxKind::XOR_KW
                | SyntaxKind::DEFINED_OR
                | SyntaxKind::SPACESHIP
                | SyntaxKind::FILE_TEST_OP
                | SyntaxKind::REGEX_MATCH
                | SyntaxKind::REGEX_NOT_MATCH
                | SyntaxKind::COMMA
        )
    }

    fn is_literal(&self, kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::VERSION
                | SyntaxKind::BARE_VERSION
                | SyntaxKind::REGEX_LITERAL
                | SyntaxKind::IO_EXPR
                | SyntaxKind::Q_STRING
                | SyntaxKind::QQ_STRING
                | SyntaxKind::QX_STRING
                | SyntaxKind::QW_STRING
        )
    }

    fn is_left_delimiter(&self, kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::L_PAREN | SyntaxKind::L_BRACE | SyntaxKind::L_BRACKET
        )
    }

    fn is_right_delimiter(&self, kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::R_PAREN | SyntaxKind::R_BRACE | SyntaxKind::R_BRACKET
        )
    }

    fn handle_sigil_context(&self, kind: SyntaxKind) -> LexerContext {
        match kind {
            SyntaxKind::BACKSLASH => LexerContext::ExpectingValue,
            _ => LexerContext::VariableList,
        }
    }

    fn handle_keyword_context(&self, kind: SyntaxKind) -> LexerContext {
        match kind {
            SyntaxKind::MY_KW
            | SyntaxKind::OUR_KW
            | SyntaxKind::STATE_KW
            | SyntaxKind::LOCAL_KW => LexerContext::VariableList,
            SyntaxKind::QW_KW => LexerContext::QwDelimiter,
            SyntaxKind::Q_KW
            | SyntaxKind::QQ_KW
            | SyntaxKind::QX_KW
            | SyntaxKind::M_KW
            | SyntaxKind::QR_KW
            | SyntaxKind::S_KW
            | SyntaxKind::TR_KW
            | SyntaxKind::Y_KW => LexerContext::QlikeDelimiter,
            SyntaxKind::END_KW | SyntaxKind::DATA_KW => LexerContext::RawData,
            _ => LexerContext::ExpectingValue,
        }
    }

    fn handle_operator_context(&self, kind: SyntaxKind) -> LexerContext {
        match kind {
            SyntaxKind::SLASH => self.handle_slash_context(),
            _ => LexerContext::ExpectingValue,
        }
    }

    fn handle_left_delimiter_context(&self, _kind: SyntaxKind) -> LexerContext {
        if self.context == LexerContext::QwDelimiter {
            LexerContext::QwContent
        } else {
            LexerContext::ExpectingValue
        }
    }

    fn handle_slash_context(&self) -> LexerContext {
        match self.context {
            LexerContext::QwDelimiter => LexerContext::QwContent,
            LexerContext::QlikeDelimiter => LexerContext::ExpectingOperator,
            LexerContext::QwContent => LexerContext::ExpectingOperator,
            _ => LexerContext::ExpectingValue,
        }
    }

    fn handle_identifier_context(&self) -> LexerContext {
        match self.context {
            LexerContext::VariableList | LexerContext::ExpectingValue => {
                LexerContext::ExpectingOperator
            }
            LexerContext::ExpectingOperator => LexerContext::ExpectingOperator,
            LexerContext::QlikeDelimiter | LexerContext::QwContent | LexerContext::RawData => {
                self.context
            }
            LexerContext::QwDelimiter => LexerContext::ExpectingOperator,
        }
    }

    fn update_context(&mut self, syntax_kind: SyntaxKind) {
        self.context = match syntax_kind {
            // Sigils and reference operators
            kind if self.is_sigil(kind) || kind == SyntaxKind::BACKSLASH => {
                self.handle_sigil_context(kind)
            }

            // Keywords
            kind if self.is_keyword(kind) => self.handle_keyword_context(kind),

            // Operators
            kind if self.is_operator(kind) => self.handle_operator_context(kind),

            // Left delimiters need special handling for qw
            kind if self.is_left_delimiter(kind) => self.handle_left_delimiter_context(kind),

            // Identifiers need context-dependent handling
            SyntaxKind::IDENT => self.handle_identifier_context(),

            // Literals and closing delimiters expect operators
            kind if self.is_literal(kind) || self.is_right_delimiter(kind) => {
                LexerContext::ExpectingOperator
            }

            // Statement terminators and POD reset context
            SyntaxKind::SEMICOLON | SyntaxKind::CUT_KW | SyntaxKind::POD_CONTENT => {
                LexerContext::ExpectingValue
            }

            // Keep current context for other tokens
            _ => self.context,
        };
    }

    /// Set lexer context explicitly (for parser to use)
    pub fn set_context(&mut self, context: LexerContext) {
        self.context = context;
    }

    /// Track line position for POD detection
    fn update_line_position(&mut self, text: &str) {
        // Check if this token contains a newline
        if text.contains('\n') {
            self.at_line_start = true;
        } else if text.chars().any(|c| !c.is_whitespace()) {
            // Non-whitespace content means we're no longer at line start
            self.at_line_start = false;
        }
    }

    /// Try to consume entire POD block (=identifier to =cut or EOF)
    fn try_consume_pod_content(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();

        // Check if this starts with POD (=identifier, but not =cut)
        let is_pod_start = if let Some(line_end) = remainder.find('\n') {
            let line = &remainder[..line_end];
            line.len() > 1
                && line.starts_with('=')
                && line.chars().nth(1).is_some_and(|c| c.is_alphabetic())
                && !line.starts_with("=cut")
        } else {
            remainder.len() > 1
                && remainder.starts_with('=')
                && remainder.chars().nth(1).is_some_and(|c| c.is_alphabetic())
                && !remainder.starts_with("=cut")
        };

        if !is_pod_start {
            return None;
        }

        // Find the end of the POD block (=cut or EOF)
        let mut search_pos = 0;
        let bytes = remainder.as_bytes();

        while search_pos < bytes.len() {
            // Check if we're at the start of a line
            let at_line_start = search_pos == 0 || bytes[search_pos - 1] == b'\n';

            if at_line_start && remainder[search_pos..].starts_with("=cut") {
                // Check that =cut is followed by non-alphanumeric or end of line/string
                let after_cut_pos = search_pos + 4;
                let is_complete_cut = if after_cut_pos >= bytes.len() {
                    true // =cut at end of input
                } else {
                    let next_char = bytes[after_cut_pos] as char;
                    !next_char.is_alphanumeric()
                };

                if is_complete_cut {
                    // Found =cut, find the end of the =cut line
                    let cut_line_end = if let Some(newline_pos) = remainder[search_pos..].find('\n')
                    {
                        search_pos + newline_pos + 1 // Include the newline
                    } else {
                        remainder.len() // =cut at EOF
                    };

                    // Consume everything including =cut
                    let pod_content = &remainder[..cut_line_end];
                    self.logos_lexer.bump(cut_line_end);
                    self.at_line_start = true;
                    return Some((SyntaxKind::POD_CONTENT, pod_content));
                }
            }

            search_pos += 1;
        }

        // No =cut found, consume all remaining content as POD
        self.logos_lexer.bump(remainder.len());
        Some((SyntaxKind::POD_CONTENT, remainder))
    }

    /// Try to consume standalone =cut at line start (error case)
    fn try_consume_standalone_cut(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();

        if let Some(line_end) = remainder.find('\n') {
            let line = &remainder[..line_end];
            if line.starts_with("=cut") {
                // Check that =cut is followed by non-alphanumeric or end of line
                if line.len() == 4 || !line.chars().nth(4).unwrap().is_alphanumeric() {
                    // Consume the =cut line including newline
                    let cut_text = &remainder[..line_end + 1];
                    self.logos_lexer.bump(cut_text.len());
                    return Some((SyntaxKind::CUT_KW, cut_text));
                }
            }
        } else if remainder.starts_with("=cut") {
            // =cut at EOF
            if remainder.len() == 4 || !remainder.chars().nth(4).unwrap().is_alphanumeric() {
                self.logos_lexer.bump(remainder.len());
                return Some((SyntaxKind::CUT_KW, remainder));
            }
        }

        None
    }

    /// Try to consume qw() content, tokenizing whitespace-separated words
    fn try_consume_qw_content(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None;
        }

        let first_char = remainder.chars().next().unwrap();

        // If we start with whitespace, consume all leading whitespace
        if first_char.is_whitespace() {
            let mut end_pos = 0;
            for ch in remainder.chars() {
                if !ch.is_whitespace() {
                    break;
                }
                end_pos += ch.len_utf8();
            }

            if end_pos > 0 {
                let whitespace = &remainder[..end_pos];
                self.logos_lexer.bump(end_pos);
                return Some((SyntaxKind::WHITESPACE, whitespace));
            }
            return None;
        }

        // If we start with a closing delimiter, let the normal lexer handle it
        if self.is_quote_delimiter(first_char) {
            return None;
        }

        // Otherwise, consume a word (non-whitespace sequence)
        let mut end_pos = 0;
        for ch in remainder.chars() {
            // Stop at whitespace or closing delimiters
            if ch.is_whitespace() || self.is_quote_delimiter(ch) {
                break;
            }
            end_pos += ch.len_utf8();
        }

        if end_pos > 0 {
            let word = &remainder[..end_pos];
            self.logos_lexer.bump(end_pos);
            return Some((SyntaxKind::QW_STRING, word));
        }

        None
    }

    pub fn span(&self) -> std::ops::Range<usize> {
        self.logos_lexer.span()
    }

    /// Peek at the next token without consuming it or changing lexer state
    pub fn peek_token(&self) -> Option<(SyntaxKind, &'a str)> {
        self.clone().next()
    }

    /// Peek at the next non-trivia token without consuming it or changing lexer state
    pub fn peek_non_trivia_token(&self) -> Option<(SyntaxKind, &'a str)> {
        self.clone().find(|(kind, _)| !kind.is_trivia())
    }

    /// Peek ahead multiple tokens, skipping trivia, and return the first non-trivia token
    /// that matches any of the given kinds
    pub fn peek_for_any(&self, target_kinds: &[SyntaxKind]) -> Option<(SyntaxKind, &'a str)> {
        self.clone()
            .find(|(kind, _)| !kind.is_trivia())
            .filter(|(kind, _)| target_kinds.contains(kind))
    }

    /// Check if a character can be used as a quote-like delimiter
    fn is_quote_delimiter(&self, ch: char) -> bool {
        match ch {
            // Paired delimiters
            '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' => true,
            // Common single-character delimiters (excluding : ; , . - _ which are common in words)
            '/' | '|' | '#' | '!' | '~' | '@' | '$' | '%' | '^' | '&' | '*' | '+' | '=' | '?'
            | '`' | '\'' | '"' => true,
            _ => false,
        }
    }

    /// Try to consume a delimiter token in QlikeDelimiter context
    fn try_consume_quote_delimiter(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None;
        }

        let first_char = remainder.chars().next().unwrap();

        if self.is_quote_delimiter(first_char) {
            let delim_str = &remainder[..first_char.len_utf8()];
            self.logos_lexer.bump(first_char.len_utf8());
            return Some((SyntaxKind::DELIMITER, delim_str));
        }

        None
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

    #[test]
    fn test_string_comparison_operators() {
        // Test that 'eq' is an operator when expecting an operator
        let mut lexer = Lexer::new(r#"$a eq "b""#);
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "a")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_EQ, "eq")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STRING, r#""b""#)));

        // Test that 'ne' is an operator when expecting an operator
        let mut lexer = Lexer::new(r#"$a ne "b""#);
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "a")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_NE, "ne")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STRING, r#""b""#)));

        // Test that 'gt' is an operator
        let mut lexer = Lexer::new(r#"$a gt "b""#);
        lexer.next_token(); // $
        lexer.next_token(); // a
        lexer.next_token(); // whitespace
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_GT, "gt")));

        // Test that 'lt' is an operator
        let mut lexer = Lexer::new(r#"$a lt "b""#);
        lexer.next_token();
        lexer.next_token();
        lexer.next_token();
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_LT, "lt")));

        // Test that 'ge' is an operator
        let mut lexer = Lexer::new(r#"$a ge "b""#);
        lexer.next_token();
        lexer.next_token();
        lexer.next_token();
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_GE, "ge")));

        // Test that 'le' is an operator
        let mut lexer = Lexer::new(r#"$a le "b""#);
        lexer.next_token();
        lexer.next_token();
        lexer.next_token();
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_LE, "le")));

        // Test that 'cmp' is an operator
        let mut lexer = Lexer::new(r#"$a cmp "b""#);
        lexer.next_token();
        lexer.next_token();
        lexer.next_token();
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_CMP, "cmp")));

        // Test that 'eq' is an identifier when expecting a value
        let mut lexer = Lexer::new("sub eq { }");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "eq")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::L_BRACE, "{")));

        // Test that 'ne' is an identifier when expecting a value
        let mut lexer = Lexer::new("my $ne;");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "ne")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SEMICOLON, ";")));

        // Test that 'gt' is an identifier
        let mut lexer = Lexer::new("sub gt {}");
        lexer.next_token(); // sub
        lexer.next_token(); // whitespace
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "gt")));
    }

    #[test]
    fn test_s_operator_disambiguation() {
        // Test that 's' is recognized as an operator when followed by delimiters (baseline for tr/y)
        let mut lexer = Lexer::new("$str s/abc/xyz/");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "str")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::S_KW, "s")));
    }

    #[test]
    fn test_tr_operator_disambiguation() {
        // Test that 'tr' is recognized as an operator when followed by delimiters
        let mut lexer = Lexer::new("$str tr/abc/xyz/");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "str")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::TR_KW, "tr")));
    }

    #[test]
    fn test_tr_as_function_name() {
        // Test that 'tr' is an identifier when used as function name
        let mut lexer = Lexer::new("sub tr {}");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "tr")));
    }

    #[test]
    fn test_tr_as_variable_name() {
        // Test that 'tr' is an identifier when used as variable name
        let mut lexer = Lexer::new("my $tr = 1;");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "tr")));
    }

    #[test]
    fn test_y_operator_disambiguation() {
        // Test that 'y' is recognized as an operator when followed by delimiters
        let mut lexer = Lexer::new("$str y/abc/xyz/");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "str")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::Y_KW, "y")));
    }

    #[test]
    fn test_y_as_function_name() {
        // Test that 'y' is an identifier when used as function name
        let mut lexer = Lexer::new("sub y {}");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "y")));
    }

    #[test]
    fn test_y_as_variable_name() {
        // Test that 'y' is an identifier when used as variable name
        let mut lexer = Lexer::new("my $y = 1;");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "y")));
    }

    #[test]
    fn test_qw_basic_parsing() {
        // Test basic qw() parsing with parentheses
        let mut lexer = Lexer::new("qw(hello world)");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "hello")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "world")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_qw_with_colon_content() {
        // Test the specific case that was broken: qw(:common)
        let mut lexer = Lexer::new("qw(:common)");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, ":common")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_qw_with_multiple_words() {
        // Test qw with multiple words including special characters
        let mut lexer = Lexer::new("qw(a:b c:d e)");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "a:b")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "c:d")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "e")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_qw_with_different_delimiters() {
        // Test qw with slash delimiters
        let mut lexer = Lexer::new("qw/x:y z/");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "x:y")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "z")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
        assert_eq!(lexer.next_token(), None);

        // Test qw with bracket delimiters
        let mut lexer = Lexer::new("qw[foo bar]");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "[")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "foo")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "bar")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "]")));
        assert_eq!(lexer.next_token(), None);

        // Test qw with brace delimiters
        let mut lexer = Lexer::new("qw{alpha beta}");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "{")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "alpha")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "beta")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "}")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_qw_empty() {
        // Test empty qw()
        let mut lexer = Lexer::new("qw()");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_qw_with_whitespace() {
        // Test qw with extra whitespace
        let mut lexer = Lexer::new("qw(  hello   world  )");
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, "  ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "hello")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, "   ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "world")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, "  ")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_tr_y_with_different_delimiters() {
        // Test tr with various delimiters
        let test_cases = [
            ("tr/abc/xyz/", SyntaxKind::TR_KW),
            ("tr(abc)(xyz)", SyntaxKind::TR_KW),
            ("tr[abc][xyz]", SyntaxKind::TR_KW),
            ("tr{abc}{xyz}", SyntaxKind::TR_KW),
            ("y/abc/xyz/", SyntaxKind::Y_KW),
            ("y(abc)(xyz)", SyntaxKind::Y_KW),
            ("y[abc][xyz]", SyntaxKind::Y_KW),
            ("y{abc}{xyz}", SyntaxKind::Y_KW),
        ];

        for (input, expected_kind) in test_cases {
            let mut lexer = Lexer::new(input);
            assert_eq!(
                lexer.next_token(),
                Some((
                    expected_kind,
                    &input[..if input.starts_with("tr") { 2 } else { 1 }]
                ))
            );
        }
    }
}
