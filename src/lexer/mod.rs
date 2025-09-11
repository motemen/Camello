use crate::SyntaxKind;
use logos::Logos;

mod quote;

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

    #[token("|")]
    Pipe,

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

    // Postfix dereference operators (handled manually due to context sensitivity)
    PostfixDerefArray,  // ->@*
    PostfixDerefHash,   // ->%*
    PostfixDerefScalar, // ->$*
}

impl Token {
    #[must_use]
    pub fn to_syntax_kind(&self) -> SyntaxKind {
        match self {
            Token::Dollar => SyntaxKind::DOLLAR,
            Token::At => SyntaxKind::AT,
            Token::Backslash => SyntaxKind::BACKSLASH,
            Token::Ampersand => SyntaxKind::AMPERSAND, // Will be disambiguated
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
            Token::Caret => SyntaxKind::CARET, // Will be disambiguated
            Token::Pipe => SyntaxKind::BITWISE_OR,
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
            Token::PostfixDerefArray => SyntaxKind::POSTFIX_DEREF_ARRAY,
            Token::PostfixDerefHash => SyntaxKind::POSTFIX_DEREF_HASH,
            Token::PostfixDerefScalar => SyntaxKind::POSTFIX_DEREF_SCALAR,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LexerContext {
    /// Expecting a value, identifier, or sigil (after keywords, operators, sigils)
    ExpectingValue,
    /// Expecting an operator (after identifiers, numbers, variables)
    ExpectingOperator,
    /// After a sigil, expecting a variable name
    VariableName,
    /// In a subroutine prototype
    SubPrototype,
    /// In a data section context (after __END__ or __DATA__)
    RawData,
    /// In a quote-like operator context
    QuoteLike {
        prefix: SyntaxKind,    // S_KW, Q_KW, QQ_KW, QW_KW, TR_KW, Y_KW, M_KW, QR_KW
        mode: QuoteLikeMode,   // Q (q/qq/qx), QW (qw), M (m/qr), S (s), TR (tr/y)
        state: QuoteLikeState, // Parsing state
        delimiter: char,       // Current delimiter: '{', '(', '/', etc.
    },
}
/// Context for token disambiguation in default parsing contexts
/// This enum limits disambiguation to only the contexts where ambiguity resolution is needed
#[derive(Debug, Clone, Copy, PartialEq)]
enum DisambiguationContext {
    /// Expecting a value, identifier, or sigil (after keywords, operators, sigils)
    ExpectingValue,
    /// Expecting an operator (after identifiers, numbers, variables)
    ExpectingOperator,
}

/// External lexical expectation provided by the parser to disambiguate
/// only operator/value sensitive tokens in default contexts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LexExpectation {
    /// Expecting a value, identifier, or sigil (after keywords, operators, sigils)
    Value,
    /// Expecting an operator (after identifiers, numbers, variables)
    Operator,
}

impl From<LexExpectation> for DisambiguationContext {
    fn from(expect: LexExpectation) -> Self {
        match expect {
            LexExpectation::Value => DisambiguationContext::ExpectingValue,
            LexExpectation::Operator => DisambiguationContext::ExpectingOperator,
        }
    }
}

impl From<LexerContext> for Option<DisambiguationContext> {
    fn from(lexer_context: LexerContext) -> Self {
        match lexer_context {
            LexerContext::ExpectingValue => Some(DisambiguationContext::ExpectingValue),
            LexerContext::ExpectingOperator => Some(DisambiguationContext::ExpectingOperator),
            // Other contexts (SubPrototype, RawData, QuoteLike, ExpectingVariableName)
            // don't use disambiguation - they have dedicated handlers
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuoteLikeMode {
    Q,  // q, qq, qx (single delimiter)
    QW, // qw (single delimiter, whitespace-separated words)
    M,  // m (single delimiter, regex)
    QR, // qr (single delimiter, compiled regex)
    S,  // s (double delimiter)
    TR, // tr, y (double delimiter)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DelimiterPhase {
    First,
    Second,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DelimiterType {
    Open,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuoteLikeState {
    Delimiter {
        phase: DelimiterPhase,
        kind: DelimiterType,
    },
    Content {
        phase: DelimiterPhase,
    },
    Flags,
}

pub struct Lexer<'a> {
    logos_lexer: logos::Lexer<'a, Token>,
    context: LexerContext,
    at_line_start: bool, // Track if we're at the start of a line for POD detection
}

impl Clone for Lexer<'_> {
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
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        let logos_lexer = Token::lexer(input);

        Self {
            logos_lexer,
            context: LexerContext::ExpectingValue, // Start expecting a value
            at_line_start: true,                   // Start at beginning of input (line start)
        }
    }

    /// Handle special tokens when expecting a value
    fn try_handle_expecting_value_context(&mut self) -> Option<(SyntaxKind, &'a str)> {
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
        self.next_token_internal(None)
    }

    /// Core tokenization step with an optional disambiguation context override.
    /// When `override_ctx` is provided, it influences only the default context
    /// (ExpectingValue/ExpectingOperator) for this single step.
    fn next_token_internal(
        &mut self,
        override_ctx: Option<DisambiguationContext>,
    ) -> Option<(SyntaxKind, &'a str)> {
        match self.context {
            LexerContext::RawData => self.handle_raw_data(),
            LexerContext::QuoteLike { .. } => self.handle_quote_like(),
            LexerContext::VariableName => self.handle_variable_name(),
            LexerContext::SubPrototype => self.handle_sub_prototype(),
            LexerContext::ExpectingValue | LexerContext::ExpectingOperator => {
                self.handle_default_context_with(override_ctx)
            }
        }
    }

    /// Handle RawData context: __DATA__ や POD の処理を担当
    fn handle_raw_data(&mut self) -> Option<(SyntaxKind, &'a str)> {
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
        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None; // No more data to consume
        }
        self.logos_lexer.bump(remainder.len());
        Some((SyntaxKind::RAW_STRING, remainder))
    }

    /// Handle QuoteLike context: q{...} や s/.../.../などのクオート風演算子の処理を担当
    fn handle_quote_like(&mut self) -> Option<(SyntaxKind, &'a str)> {
        if let Some(result) = self.try_handle_quote_like_internal() {
            let (syntax_kind, text) = result;
            // Quote-like internal already handles context transitions
            // Only update line position here
            self.update_line_position(text);
            Some((syntax_kind, text))
        } else {
            // Check if context changed during quote-like processing
            // If so, fall back to appropriate handler
            match self.context {
                LexerContext::QuoteLike { .. } => None, // Still in quote-like, no token available
                _ => {
                    // Context changed, delegate to new context handler
                    match self.context {
                        LexerContext::ExpectingValue | LexerContext::ExpectingOperator => {
                            self.handle_default_context_with(None)
                        }
                        LexerContext::VariableName => self.handle_variable_name(),
                        LexerContext::SubPrototype => self.handle_sub_prototype(),
                        LexerContext::RawData => self.handle_raw_data(),
                        LexerContext::QuoteLike { .. } => unreachable!(), // Already handled above
                    }
                }
            }
        }
    }

    /// Handle ExpectingVariableName context: 変数名のトークン化を担当
    fn handle_variable_name(&mut self) -> Option<(SyntaxKind, &'a str)> {
        if let Some((syntax_kind, text)) = self.try_consume_variable_name() {
            self.update_context(syntax_kind);
            self.update_line_position(text);
            Some((syntax_kind, text))
        } else {
            // Fall back to default context handling if no variable name found
            self.set_context(LexerContext::ExpectingValue);
            self.handle_default_context_with(None)
        }
    }

    /// Map tokens to appropriate SyntaxKind in subroutine prototype context
    /// In prototypes, symbols have fixed meanings regardless of position
    fn map_prototype_token(&self, token: Token) -> SyntaxKind {
        match token {
            Token::Backslash => SyntaxKind::BACKSLASH,
            Token::At => SyntaxKind::AT,
            Token::Percent => SyntaxKind::PERCENT, // Always hash sigil in prototypes
            Token::Dollar => SyntaxKind::DOLLAR,
            Token::Ampersand => SyntaxKind::AMPERSAND,
            Token::Star => SyntaxKind::ASTERISK, // Always typeglob sigil in prototypes
            Token::Semicolon => SyntaxKind::SEMICOLON,
            Token::LBracket => SyntaxKind::L_BRACKET,
            Token::RBracket => SyntaxKind::R_BRACKET,
            Token::LParen => SyntaxKind::L_PAREN,
            Token::RParen => SyntaxKind::R_PAREN,
            // Whitespace and comments are handled normally
            Token::Whitespace => SyntaxKind::WHITESPACE,
            Token::Newline => SyntaxKind::WHITESPACE,
            Token::Comment => SyntaxKind::COMMENT,
            // All other tokens are converted using default mapping
            _ => token.to_syntax_kind(),
        }
    }

    /// Handle SubPrototype context: サブルーチンプロトタイプの解析を担当
    fn handle_sub_prototype(&mut self) -> Option<(SyntaxKind, &'a str)> {
        // Get the next token from logos lexer
        match self.logos_lexer.next() {
            Some(Ok(token)) => {
                let text = self.logos_lexer.slice();
                // Map token to appropriate SyntaxKind using prototype-specific rules
                let syntax_kind = self.map_prototype_token(token);

                // Update line position for POD detection
                self.update_line_position(text);

                Some((syntax_kind, text))
            }
            Some(Err(())) => {
                // エラートークンとして処理
                let text = self.logos_lexer.slice();
                Some((SyntaxKind::ERROR, text))
            }
            None => None,
        }
    }

    /// Handle default context (ExpectingValue | ExpectingOperator): 通常ケースを担当
    fn handle_default_context_with(
        &mut self,
        override_ctx: Option<DisambiguationContext>,
    ) -> Option<(SyntaxKind, &'a str)> {
        // Handle POD content at line start first
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

        // Handle special tokens when expecting a value
        // Respect per-call override when present; otherwise use current lexer context
        let is_value_context = match override_ctx.or_else(|| self.context.into()) {
            Some(DisambiguationContext::ExpectingValue) => true,
            _ => false,
        };
        if is_value_context {
            if let Some(result) = self.try_handle_expecting_value_context() {
                let (syntax_kind, text) = result;
                self.update_context(syntax_kind);
                self.update_line_position(text);
                return Some((syntax_kind, text));
            }
        }

        // Handle postfix dereference operators (->@*, ->%*, ->$*)
        if let Some((syntax_kind, text)) = self.try_consume_postfix_deref() {
            self.update_context(syntax_kind);
            self.update_line_position(text);
            return Some((syntax_kind, text));
        }

        match self.logos_lexer.next() {
            Some(Ok(token)) => {
                let text = self.logos_lexer.slice();
                // Convert override/current context into a disambiguation context for this step
                let disambiguation_context = override_ctx
                    .or_else(|| Option::<DisambiguationContext>::from(self.context))
                    .expect(
                        "disambiguate should only be called from ExpectingValue/ExpectingOperator/ExpectingVariableName contexts",
                    );
                let syntax_kind = self.disambiguate_with(token, text, disambiguation_context);

                // Special handling for __END__ and __DATA__: consume everything remaining as data section
                if matches!(syntax_kind, SyntaxKind::END_KW | SyntaxKind::DATA_KW) {
                    self.update_context(syntax_kind);
                    return Some((syntax_kind, text));
                }

                // Update context for special modes only; default Value/Operator is parser-driven
                self.update_context(syntax_kind);

                // Track line position for POD detection
                self.update_line_position(text);

                Some((syntax_kind, text))
            }
            Some(Err(())) => {
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
        Some((SyntaxKind::RAW_STRING, data_text))
    }

    fn disambiguate_with(
        &self,
        token: Token,
        text: &str,
        disambiguation_context: DisambiguationContext,
    ) -> SyntaxKind {
        match token {
            Token::Ident => {
                // Common keywords that need disambiguation regardless of context
                match text {
                    "s" => self.disambiguate_s(disambiguation_context),
                    "tr" => self.disambiguate_tr(disambiguation_context),
                    "y" => self.disambiguate_y(disambiguation_context),
                    "x" => Self::disambiguate_x(disambiguation_context),
                    "eq" | "ne" | "gt" | "lt" | "ge" | "le" | "cmp" => {
                        Self::disambiguate_str_op(disambiguation_context, text)
                    }
                    "not" | "and" | "or" | "xor" => {
                        Self::disambiguate_logical_op(disambiguation_context, text)
                    }
                    _ => {
                        // Handle context-sensitive keywords
                        // Normal context - all keywords are recognized
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
                            "use" => SyntaxKind::USE_KW,
                            "no" => SyntaxKind::NO_KW,
                            "return" => SyntaxKind::RETURN_KW,
                            "undef" => SyntaxKind::UNDEF_KW,
                            _ => SyntaxKind::IDENT,
                        }
                    }
                }
            }
            Token::Percent => {
                // % の場合、文脈によって sigil か modulo operator かを判定
                Self::disambiguate_percent(disambiguation_context)
            }
            Token::Star => {
                // * の場合、文脈によって typeglob sigil か multiplication operator かを判定
                Self::disambiguate_star(disambiguation_context)
            }
            Token::Slash => {
                // Context-sensitive disambiguation between regex literal and division
                Self::disambiguate_slash(disambiguation_context)
            }
            Token::Ampersand => Self::disambiguate_ampersand(disambiguation_context),
            Token::Caret => Self::disambiguate_caret(disambiguation_context),
            Token::Pipe => {
                // Pipe is always bitwise OR in current context
                SyntaxKind::BITWISE_OR
            }
            // Bracket-like delimiters - simplified for default context only
            Token::LParen
            | Token::LBrace
            | Token::LBracket
            | Token::RParen
            | Token::RBrace
            | Token::RBracket => token.to_syntax_kind(),
            // Other tokens - no special handling needed for default context
            Token::Greater
            | Token::Less
            | Token::Plus
            | Token::Minus
            | Token::Eq
            | Token::At
            | Token::Dollar
            | Token::Colon
            | Token::QuestionMark
            | Token::Dot
            | Token::Comma
            | Token::Semicolon => token.to_syntax_kind(),
            _ => token.to_syntax_kind(),
        }
    }

    /// Disambiguate % between hash sigil and modulo operator
    fn disambiguate_percent(context: DisambiguationContext) -> SyntaxKind {
        match context {
            DisambiguationContext::ExpectingValue => {
                // When expecting a value, % is a sigil for a hash
                // Examples: "my %hash", "{ key => %val }"
                SyntaxKind::PERCENT
            }
            DisambiguationContext::ExpectingOperator => {
                // When expecting an operator, % is the modulo operator
                // Examples: "@array % hash", "$var % other_var", "func() % 2"
                SyntaxKind::MODULO
            }
        }
    }

    /// Disambiguate * between typeglob sigil and multiplication operator  
    fn disambiguate_star(context: DisambiguationContext) -> SyntaxKind {
        match context {
            DisambiguationContext::ExpectingValue => {
                // When expecting a value, * is a typeglob sigil
                // Examples: "my *glob", "*{$name}", "*STDIN"
                SyntaxKind::ASTERISK
            }
            DisambiguationContext::ExpectingOperator => {
                // When expecting an operator, * is the multiplication operator
                // Examples: "$a * $b", "func() * 2"
                SyntaxKind::STAR
            }
        }
    }

    fn disambiguate_str_op(context: DisambiguationContext, op: &str) -> SyntaxKind {
        match context {
            DisambiguationContext::ExpectingOperator => {
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
            DisambiguationContext::ExpectingValue => {
                // In ExpectingValue context, they are identifiers
                // Examples: "sub eq", "my $ne"
                SyntaxKind::IDENT
            }
        }
    }

    fn disambiguate_x(context: DisambiguationContext) -> SyntaxKind {
        match context {
            DisambiguationContext::ExpectingValue => {
                // When expecting a value, x is an identifier
                // Examples: "sub x", "$x", "my $x"
                SyntaxKind::IDENT
            }
            DisambiguationContext::ExpectingOperator => {
                // When expecting an operator, x is the repetition operator
                // Examples: "$str x 3", "'hello' x 2"
                SyntaxKind::X
            }
        }
    }

    fn disambiguate_s(&self, context: DisambiguationContext) -> SyntaxKind {
        match context {
            DisambiguationContext::ExpectingValue => {
                // Look ahead to determine if this is s/// substitution or a bareword function call
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
                    }
                    // Otherwise, it's likely substitution
                    return SyntaxKind::S_KW;
                }

                // If we reach end of input after 's', assume function call
                SyntaxKind::IDENT
            }
            DisambiguationContext::ExpectingOperator => {
                // When expecting an operator, s is the substitution operator
                // Examples: "$str s/old/new/"
                SyntaxKind::S_KW
            }
        }
    }

    fn disambiguate_logical_op(context: DisambiguationContext, op: &str) -> SyntaxKind {
        match context {
            DisambiguationContext::ExpectingOperator => {
                // When expecting an operator, not/and/or/xor are logical operators
                match op {
                    "not" => SyntaxKind::NOT_KW,
                    "and" => SyntaxKind::AND_KW,
                    "or" => SyntaxKind::OR_KW,
                    "xor" => SyntaxKind::XOR_KW,
                    _ => SyntaxKind::IDENT, // Handle unknown ops gracefully
                }
            }
            DisambiguationContext::ExpectingValue => {
                // In ExpectingValue context, they are identifiers
                // Examples: "sub not", "my $and", "or die"
                SyntaxKind::IDENT
            }
        }
    }

    fn disambiguate_tr(&self, context: DisambiguationContext) -> SyntaxKind {
        match context {
            DisambiguationContext::ExpectingValue => {
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
                        }
                        // Only one brace group, likely a function block
                        return SyntaxKind::IDENT;
                    }
                    // For other non-alphanumeric characters, it's likely an identifier
                    return SyntaxKind::IDENT;
                }

                // If we reach end of input after 'tr', assume identifier
                SyntaxKind::IDENT
            }
            DisambiguationContext::ExpectingOperator => {
                // When expecting an operator, tr is the transliteration operator
                // Examples: "$str tr/a-z/A-Z/"
                SyntaxKind::TR_KW
            }
        }
    }

    fn disambiguate_y(&self, context: DisambiguationContext) -> SyntaxKind {
        // y is an alias for tr, so use the same logic but return Y_KW
        match context {
            DisambiguationContext::ExpectingValue => {
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
                        }
                        // Only one brace group, likely a function block
                        return SyntaxKind::IDENT;
                    }
                    // For other non-alphanumeric characters, it's likely an identifier
                    return SyntaxKind::IDENT;
                }

                // If we reach end of input after 'y', assume identifier
                SyntaxKind::IDENT
            }
            DisambiguationContext::ExpectingOperator => {
                // When expecting an operator, y is the transliteration operator
                // Examples: "$str y/a-z/A-Z/"
                SyntaxKind::Y_KW
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
            let text = &remainder[..=pos];
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

    /// Try to consume postfix dereference operators (->@*, ->%*, ->$*)
    // FIXME: This is a bit of a hacky solution - ideally Logos would support context-sensitive lexing
    fn try_consume_postfix_deref(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();

        // Check for ->@*, ->%*, ->$*
        if remainder.len() >= 4 && remainder.starts_with("->") {
            let chars: Vec<char> = remainder.chars().collect();
            if chars.len() >= 4 && chars[3] == '*' {
                let syntax_kind = match chars[2] {
                    '@' => Some(SyntaxKind::POSTFIX_DEREF_ARRAY),
                    '%' => Some(SyntaxKind::POSTFIX_DEREF_HASH),
                    '$' => Some(SyntaxKind::POSTFIX_DEREF_SCALAR),
                    _ => None,
                };

                if let Some(kind) = syntax_kind {
                    let text = &remainder[..4]; // "->@*", "->%*", or "->$*"
                    self.logos_lexer.bump(4);
                    return Some((kind, text));
                }
            }
        }
        None
    }

    // If in ExpectingVariableName context, the next token must be an identifier
    // eg.
    // - Regular variables: $var, @array, %hash, *glob
    // - Special variables: $_, $!, $?, etc.
    // - Special variables: $^O, $^X, etc.
    // - Special variables: ${^MATCH}, etc.
    fn try_consume_variable_name(&mut self) -> Option<(SyntaxKind, &'a str)> {
        if self.context != LexerContext::VariableName {
            return None;
        }

        let remainder = self.logos_lexer.remainder();

        remainder
            .chars()
            .next()
            .and_then(|first_char| match first_char {
                'a'..='z' | 'A'..='Z' | '_' => {
                    // Standard variable name starting with letter or underscore
                    let end_pos = remainder
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .map(|c| c.len_utf8())
                        .sum();
                    let text = &remainder[..end_pos];
                    self.logos_lexer.bump(end_pos);
                    Some((SyntaxKind::IDENT, text))
                }
                '0'..='9' => {
                    // Variable name starting with digit (e.g. $1, $2)
                    let end_pos = remainder
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .map(|c| c.len_utf8())
                        .sum();
                    let text = &remainder[..end_pos];
                    self.logos_lexer.bump(end_pos);
                    Some((SyntaxKind::IDENT, text))
                }
                '^' => {
                    // Special variable starting with ^ (e.g. $^O, $^X)
                    let mut end_pos = first_char.len_utf8();
                    remainder[end_pos..].chars().next().map(|c| {
                        if c.is_ascii_uppercase() || c == '_' {
                            end_pos += c.len_utf8();
                            let text = &remainder[..end_pos];
                            self.logos_lexer.bump(end_pos);
                            (SyntaxKind::IDENT, text)
                        } else {
                            // `$^` is also a valid variable
                            let text = &remainder[..end_pos];
                            self.logos_lexer.bump(end_pos);
                            (SyntaxKind::IDENT, text)
                        }
                    })
                }
                '{' => {
                    // Handles ${var} and ${^VAR}
                    let mut end_pos = first_char.len_utf8();
                    let is_special = remainder[end_pos..].starts_with('^');
                    if is_special {
                        end_pos += 1; // Skip '^'
                    }

                    let name_start_pos = end_pos;
                    for c in remainder[end_pos..].chars() {
                        end_pos += c.len_utf8();
                        if c == '}' {
                            // Check for empty name, e.g., ${} or ${^}
                            if end_pos > name_start_pos + c.len_utf8() {
                                let text = &remainder[..end_pos];
                                self.logos_lexer.bump(end_pos);
                                return Some((SyntaxKind::IDENT, text));
                            }
                            break; // Empty name is invalid
                        }

                        let is_valid_char = if is_special {
                            c.is_ascii_uppercase() || c == '_'
                        } else {
                            c.is_alphanumeric() || c == '_'
                        };

                        if !is_valid_char {
                            break; // Invalid character in variable name
                        }
                    }
                    None
                }
                '$' => {
                    // Special variable like @$, $$
                    // If followed by identifier, it is a dereference (e.g. $$var) so return None here
                    let end_pos = first_char.len_utf8();
                    if remainder[end_pos..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                    {
                        return None; // Likely a dereference, not a variable name
                    }

                    // Otherwise, treat $ as a special variable
                    let text = &remainder[..end_pos];
                    self.logos_lexer.bump(end_pos);
                    Some((SyntaxKind::IDENT, text))
                }
                '#' => {
                    // Special variable like `$#array` or `$#`
                    // Check if followed by identifier
                    let end_pos = first_char.len_utf8();
                    if remainder[end_pos..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                    {
                        let id_end = remainder[end_pos..]
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .map(|c| c.len_utf8())
                            .sum::<usize>();
                        let total_end = end_pos + id_end;
                        let text = &remainder[..total_end];
                        self.logos_lexer.bump(total_end);
                        return Some((SyntaxKind::IDENT, text));
                    }
                    // Otherwise, treat # as a special variable
                    let text = &remainder[..end_pos];
                    self.logos_lexer.bump(end_pos);
                    Some((SyntaxKind::IDENT, text))
                }
                // Special single-character variables like $_, $!, $?, etc.
                '!' | '@' | '%' | '&' | '*' | '+' | '-' | '/' | '\\' | '<' | '>' | '=' | '~'
                | '`' | ':' | ';' | ',' | '.' | '?' | '(' | ')' => {
                    let end_pos = first_char.len_utf8();
                    let text = &remainder[..end_pos];
                    self.logos_lexer.bump(end_pos);
                    Some((SyntaxKind::IDENT, text))
                }
                _ => None,
            })
    }

    fn disambiguate_slash(_context: DisambiguationContext) -> SyntaxKind {
        // Slash is always division operator in disambiguate context
        // because regex literals are handled in try_consume_regex_literal
        SyntaxKind::SLASH
    }

    /// Disambiguate ampersand (&) based on context
    fn disambiguate_ampersand(context: DisambiguationContext) -> SyntaxKind {
        match context {
            DisambiguationContext::ExpectingValue => {
                // In value context, & is reference/sigil
                SyntaxKind::AMPERSAND
            }
            DisambiguationContext::ExpectingOperator => {
                // In operator context, it's bitwise AND
                SyntaxKind::BITWISE_AND
            }
        }
    }

    /// Disambiguate caret (^) based on context
    fn disambiguate_caret(context: DisambiguationContext) -> SyntaxKind {
        match context {
            DisambiguationContext::ExpectingValue => {
                // In expecting value context, ^ is likely a sigil
                // (e.g., special variables like $^O, $^X)
                SyntaxKind::CARET
            }
            DisambiguationContext::ExpectingOperator => {
                // In operator context, it's bitwise XOR
                SyntaxKind::BITWISE_XOR
            }
        }
    }

    /// Check if the given identifier text represents a built-in function
    /// Built-in functions expect values as arguments, so they should transition to `ExpectingValue` context
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
                | SyntaxKind::POSTFIX_DEREF_ARRAY
                | SyntaxKind::POSTFIX_DEREF_HASH
                | SyntaxKind::POSTFIX_DEREF_SCALAR
                | SyntaxKind::BACKSLASH
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
                | SyntaxKind::LITERAL_STRING
                | SyntaxKind::INTERPOLATED_STRING
                | SyntaxKind::REGEX_PATTERN
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

    fn handle_keyword_context(&self, kind: SyntaxKind) -> LexerContext {
        match kind {
            SyntaxKind::MY_KW
            | SyntaxKind::OUR_KW
            | SyntaxKind::STATE_KW
            | SyntaxKind::LOCAL_KW => LexerContext::ExpectingValue,
            SyntaxKind::QW_KW => LexerContext::QuoteLike {
                prefix: SyntaxKind::QW_KW,
                mode: QuoteLikeMode::QW,
                state: QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                },
                delimiter: '\0', // Will be set when delimiter is found
            },
            SyntaxKind::Q_KW => LexerContext::QuoteLike {
                prefix: SyntaxKind::Q_KW,
                mode: QuoteLikeMode::Q,
                state: QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                },
                delimiter: '\0',
            },
            SyntaxKind::QQ_KW => LexerContext::QuoteLike {
                prefix: SyntaxKind::QQ_KW,
                mode: QuoteLikeMode::Q,
                state: QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                },
                delimiter: '\0',
            },
            SyntaxKind::QX_KW => LexerContext::QuoteLike {
                prefix: SyntaxKind::QX_KW,
                mode: QuoteLikeMode::Q,
                state: QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                },
                delimiter: '\0',
            },
            SyntaxKind::M_KW => LexerContext::QuoteLike {
                prefix: SyntaxKind::M_KW,
                mode: QuoteLikeMode::M,
                state: QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                },
                delimiter: '\0',
            },
            SyntaxKind::QR_KW => LexerContext::QuoteLike {
                prefix: SyntaxKind::QR_KW,
                mode: QuoteLikeMode::QR,
                state: QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                },
                delimiter: '\0',
            },
            SyntaxKind::S_KW => LexerContext::QuoteLike {
                prefix: SyntaxKind::S_KW,
                mode: QuoteLikeMode::S,
                state: QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                },
                delimiter: '\0',
            },
            SyntaxKind::TR_KW => LexerContext::QuoteLike {
                prefix: SyntaxKind::TR_KW,
                mode: QuoteLikeMode::TR,
                state: QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                },
                delimiter: '\0',
            },
            SyntaxKind::Y_KW => LexerContext::QuoteLike {
                prefix: SyntaxKind::Y_KW,
                mode: QuoteLikeMode::TR,
                state: QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                },
                delimiter: '\0',
            },
            _ => LexerContext::ExpectingValue, // For other keywords
        }
    }

    fn handle_operator_context(&self, _kind: SyntaxKind) -> LexerContext {
        // Do not flip default context here; parser drives Value/Operator expectation.
        // Keep VariableName/SubPrototype/QuoteLike/RawData intact.
        self.context
    }

    fn handle_identifier_context(&self) -> LexerContext {
        match &self.context {
            LexerContext::SubPrototype => self.context, // Remain in prototype context
            LexerContext::VariableName => LexerContext::ExpectingOperator, // Exit variable-name mode
            // Do not flip between Value/Operator in default flow
            LexerContext::ExpectingValue | LexerContext::ExpectingOperator => self.context,
            LexerContext::QuoteLike { .. } | LexerContext::RawData => self.context,
        }
    }

    fn update_context(&mut self, syntax_kind: SyntaxKind) {
        if self.context == LexerContext::SubPrototype {
            // In Prototype context, do not change context
            return;
        }

        self.context = match syntax_kind {
            kind if self.is_sigil(kind) => LexerContext::VariableName,

            // Keywords
            kind if self.is_keyword(kind) => self.handle_keyword_context(kind),

            // Operators
            kind if self.is_operator(kind) => self.handle_operator_context(kind),

            // DELIMITER tokens in quote-like contexts are handled by handle_quote_like_delimiter
            // Don't override the context here, let handle_quote_like_delimiter manage it
            SyntaxKind::DELIMITER => {
                // Keep current context - handle_quote_like_delimiter has already processed this
                self.context
            }

            // Left delimiters in quote-like context (fallback for non-DELIMITER tokens)
            kind if self.is_left_delimiter(kind) => {
                if let LexerContext::QuoteLike {
                    prefix,
                    mode,
                    state,
                    ..
                } = self.context
                {
                    match state {
                        QuoteLikeState::Delimiter {
                            phase: DelimiterPhase::First,
                            kind: DelimiterType::Open,
                        } => {
                            let delimiter = match kind {
                                SyntaxKind::L_PAREN => ')',
                                SyntaxKind::L_BRACE => '}',
                                SyntaxKind::L_BRACKET => ']',
                                _ => '\0',
                            };
                            LexerContext::QuoteLike {
                                prefix,
                                mode,
                                state: QuoteLikeState::Content {
                                    phase: DelimiterPhase::First,
                                },
                                delimiter,
                            }
                        }
                        QuoteLikeState::Delimiter {
                            phase: DelimiterPhase::Second,
                            kind: DelimiterType::Open,
                        } => {
                            let delimiter = match kind {
                                SyntaxKind::L_PAREN => ')',
                                SyntaxKind::L_BRACE => '}',
                                SyntaxKind::L_BRACKET => ']',
                                _ => '\0',
                            };
                            LexerContext::QuoteLike {
                                prefix,
                                mode,
                                state: QuoteLikeState::Content {
                                    phase: DelimiterPhase::Second,
                                },
                                delimiter,
                            }
                        }
                        _ => self.context,
                    }
                } else {
                    LexerContext::ExpectingValue
                }
            }

            // Identifiers need context-dependent handling
            SyntaxKind::IDENT => self.handle_identifier_context(),

            // Do not flip default context on literals/right delimiters; parser drives it
            kind if self.is_literal(kind) || self.is_right_delimiter(kind) => self.context,

            // Postfix dereference operators: keep default context unchanged
            SyntaxKind::POSTFIX_DEREF_ARRAY
            | SyntaxKind::POSTFIX_DEREF_HASH
            | SyntaxKind::POSTFIX_DEREF_SCALAR => self.context,

            // Data section keywords transition to raw data context
            SyntaxKind::END_KW | SyntaxKind::DATA_KW => LexerContext::RawData,

            // Statement terminators and POD: keep default context unchanged
            SyntaxKind::SEMICOLON | SyntaxKind::CUT_KW | SyntaxKind::POD_CONTENT => self.context,

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
                && line.chars().nth(1).is_some_and(char::is_alphabetic)
                && !line.starts_with("=cut")
        } else {
            remainder.len() > 1
                && remainder.starts_with('=')
                && remainder.chars().nth(1).is_some_and(char::is_alphabetic)
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
                    let cut_text = &remainder[..=line_end];
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

    #[must_use]
    pub fn span(&self) -> std::ops::Range<usize> {
        self.logos_lexer.span()
    }

    /// Peek at the next token without consuming it or changing lexer state
    #[must_use]
    pub fn peek_token(&self) -> Option<(SyntaxKind, &'a str)> {
        self.clone().next()
    }

    /// Peek at the next non-trivia token without consuming it or changing lexer state
    #[must_use]
    pub fn peek_non_trivia_token(&self) -> Option<(SyntaxKind, &'a str)> {
        self.clone().find(|(kind, _)| !kind.is_trivia())
    }

    /// Peek ahead multiple tokens, skipping trivia, and return the first non-trivia token
    /// that matches any of the given kinds
    #[must_use]
    pub fn peek_for_any(&self, target_kinds: &[SyntaxKind]) -> Option<(SyntaxKind, &'a str)> {
        self.clone()
            .find(|(kind, _)| !kind.is_trivia())
            .filter(|(kind, _)| target_kinds.contains(kind))
    }

    /// Get the next token using an explicit lexical expectation for ambiguous cases.
    /// For non-default contexts (QuoteLike/SubPrototype/RawData/VariableName), this expectation is ignored.
    pub fn next_token_with(
        &mut self,
        expect: LexExpectation,
    ) -> Option<(SyntaxKind, &'a str)> {
        let override_ctx = Some(expect.into());
        self.next_token_internal(override_ctx)
    }

    /// Peek the next non-trivia token using a given lexical expectation.
    /// This does not mutate the original lexer state.
    #[must_use]
    pub fn peek_non_trivia_with(
        &self,
        expect: LexExpectation,
    ) -> Option<(SyntaxKind, &'a str)> {
        let mut cloned = self.clone();
        let override_ctx = Some(expect.into());
        // Iterate tokens using the internal single-step with override until non-trivia
        loop {
            match cloned.next_token_internal(override_ctx) {
                Some((k, t)) if k.is_trivia() => {
                    // continue skipping trivia
                    let _ = t; // avoid unused
                }
                Some((k, t)) => return Some((k, t)),
                None => return None,
            }
        }
    }

    /// Peek the next token (including trivia) using a given lexical expectation.
    /// This does not mutate the original lexer state and does not skip trivia.
    #[must_use]
    pub fn peek_with(&self, expect: LexExpectation) -> Option<(SyntaxKind, &'a str)> {
        let mut cloned = self.clone();
        let override_ctx = Some(expect.into());
        cloned.next_token_internal(override_ctx)
    }

    /// Convenience: default expectation is Value
    #[must_use]
    pub fn peek_non_trivia(&self) -> Option<(SyntaxKind, &'a str)> {
        self.peek_non_trivia_with(LexExpectation::Value)
    }

    /// Convenience: default expectation is Value
    pub fn next_token_default(&mut self) -> Option<(SyntaxKind, &'a str)> {
        self.next_token_with(LexExpectation::Value)
    }
}

#[cfg(test)]
mod tests;
