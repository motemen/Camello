use crate::SyntaxKind;
use logos::Logos;
use std::collections::VecDeque;

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

// Disambiguation context is unified with LexExpectation

/// External lexical expectation provided by the parser to disambiguate
/// only operator/value sensitive tokens in default contexts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LexContext {
    /// Expecting a value, identifier, or sigil (after keywords, operators, sigils)
    Value,
    /// Expecting an operator (after identifiers, numbers, variables)
    Operator,
}

// No separate DisambiguationContext; use LexExpectation directly

// No LexerContext: parser provides expectations and lexer remains stateless

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
pub enum QuoteLikeMode {
    Q,  // q, qq, qx
    QW, // qw
    M,  // m
    QR, // qr
    S,  // s
    TR, // tr, y
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum LexerMode {
    Normal,
    QuoteLike {
        prefix: SyntaxKind,
        mode: QuoteLikeMode,
        state: QuoteLikeState,
        delimiter: char,
    },
}

pub struct Lexer<'a> {
    logos_lexer: logos::Lexer<'a, Token>,
    at_line_start: bool, // Track if we're at the start of a line for POD detection
    mode: LexerMode,
    // TODO(lexer): Remove last_non_trivia_kind and derive_default_expectation.
    // - Goal: make lexing entirely parser-driven via LexMode without relying on
    //   previous token heuristics.
    // - Plan: ensure all lexer entry points (parser-side) pass an explicit LexMode,
    //   restrict quote-like expansion to Operator mode, and eliminate default expectation.
    //   Then delete last_non_trivia_kind and associated logic.
    // - Note: standalone lexer iteration (Iterator::next) may default to Value or be
    //   retired in favor of explicit-mode helpers used by tests.
    last_non_trivia_kind: Option<SyntaxKind>,
    // Pending tokens produced by stateless expansions (e.g., quote-like operators)
    pending: VecDeque<(SyntaxKind, &'a str)>,
}

impl Clone for Lexer<'_> {
    fn clone(&self) -> Self {
        Self {
            logos_lexer: self.logos_lexer.clone(),
            at_line_start: self.at_line_start,
            mode: self.mode,
            last_non_trivia_kind: self.last_non_trivia_kind,
            pending: self.pending.clone(),
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = (SyntaxKind, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        // Default to Value expectation for standalone lexer usage
        self.next_token_default()
    }
}

impl<'a> Lexer<'a> {
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        let logos_lexer = Token::lexer(input);

        Self {
            logos_lexer,
            at_line_start: true,
            mode: LexerMode::Normal,
            last_non_trivia_kind: None,
            pending: VecDeque::new(),
        }
    }

    fn derive_default_expectation(&self) -> LexContext {
        match self.last_non_trivia_kind {
            None => LexContext::Value,
            Some(k) => {
                // After literals, identifiers, postfix deref, expect operator
                if k.is_literal()
                    || matches!(
                        k,
                        SyntaxKind::IDENT
                            | SyntaxKind::POSTFIX_DEREF_ARRAY
                            | SyntaxKind::POSTFIX_DEREF_HASH
                            | SyntaxKind::POSTFIX_DEREF_SCALAR
                    )
                {
                    LexContext::Operator
                } else if k.is_operator()
                    || matches!(
                        k,
                        SyntaxKind::L_PAREN
                            | SyntaxKind::L_BRACE
                            | SyntaxKind::L_BRACKET
                            | SyntaxKind::COMMA
                            | SyntaxKind::FAT_COMMA
                    )
                    || k.is_keyword()
                {
                    // After operators, openings, commas, or keywords, expect a value
                    LexContext::Value
                } else {
                    // Default to value for safety
                    LexContext::Value
                }
            }
        }
    }

    fn prev_is_sigil(&self) -> bool {
        matches!(
            self.last_non_trivia_kind,
            Some(SyntaxKind::DOLLAR | SyntaxKind::AT | SyntaxKind::PERCENT | SyntaxKind::ASTERISK)
        )
    }

    /// Handle special tokens when expecting a value
    fn try_handle_expecting_value_context(&mut self) -> Option<(SyntaxKind, &'a str)> {
        // 1) File test operator like -f
        if let Some(result) = Self::try_consume_file_test_op(self) {
            let (k, t) = result;
            self.update_line_position(t);
            return Some((k, t));
        }
        // 2) Regex literal /.../ only when not immediately after quote-like keywords
        let after_quote_like_kw = matches!(
            self.last_non_trivia_kind,
            Some(
                SyntaxKind::Q_KW
                    | SyntaxKind::QQ_KW
                    | SyntaxKind::QX_KW
                    | SyntaxKind::QW_KW
                    | SyntaxKind::M_KW
                    | SyntaxKind::QR_KW
                    | SyntaxKind::S_KW
                    | SyntaxKind::TR_KW
                    | SyntaxKind::Y_KW
            )
        );
        if !after_quote_like_kw {
            if let Some(result) = Self::try_consume_regex_literal(self) {
                let (k, t) = result;
                self.update_line_position(t);
                return Some((k, t));
            }
        }
        // 3) IO operator like <...>
        if let Some(result) = Self::try_consume_io_operator(self) {
            let (k, t) = result;
            self.update_line_position(t);
            return Some((k, t));
        }
        None
    }

    pub fn next_token(&mut self) -> Option<(SyntaxKind, &'a str)> {
        // Derive default expectation from last significant token for standalone usage
        let expect = self.derive_default_expectation();
        self.next_token_with(expect)
    }

    /// Core tokenization step with an optional lexical expectation override.
    /// When `override_ctx` is provided, it influences only this single step.
    fn next_token_internal(
        &mut self,
        context: Option<LexContext>,
    ) -> Option<(SyntaxKind, &'a str)> {
        // Serve any pending expanded tokens first
        if let Some((k, t)) = self.pending.pop_front() {
            if !k.is_trivia() {
                self.last_non_trivia_kind = Some(k);
            }
            self.update_line_position(t);
            return Some((k, t));
        }
        // Quote-like context handling (parser-driven)
        if let LexerMode::QuoteLike { .. } = self.mode {
            if let Some((k, t)) = self.try_handle_quote_like_internal() {
                if !k.is_trivia() {
                    self.last_non_trivia_kind = Some(k);
                }
                self.update_line_position(t);
                return Some((k, t));
            }
        }
        // Default context
        self.handle_default_context_with(context)
    }

    // Raw data consumption is handled via consume_data_section from the parser

    // Quote-like handling moved to stateless expansion; no separate handler needed

    // Removed VariableName handling; variable names are parsed by the parser.

    // Removed SubPrototype handling; parser reads prototype symbols with explicit expectations

    /// Handle default context (Value | Operator): 通常ケースを担当
    fn handle_default_context_with(
        &mut self,
        context: Option<LexContext>,
    ) -> Option<(SyntaxKind, &'a str)> {
        // If already in quote-like context, delegate immediately (pure state machine)
        if let LexerMode::QuoteLike { .. } = self.mode {
            if let Some((k, t)) = self.try_handle_quote_like_internal() {
                self.update_line_position(t);
                if !k.is_trivia() {
                    self.last_non_trivia_kind = Some(k);
                }
                return Some((k, t));
            }
        }
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
        let is_value_context = matches!(context, Some(LexContext::Value));
        let in_quote_like = matches!(self.mode, LexerMode::QuoteLike { .. });
        if is_value_context && !in_quote_like {
            if let Some(result) = self.try_handle_expecting_value_context() {
                let (syntax_kind, text) = result;
                self.update_line_position(text);
                if !syntax_kind.is_trivia() {
                    self.last_non_trivia_kind = Some(syntax_kind);
                }
                return Some((syntax_kind, text));
            }
        }

        // Handle postfix dereference operators (->@*, ->%*, ->$*)
        if let Some((syntax_kind, text)) = self.try_consume_postfix_deref() {
            self.update_line_position(text);
            if !syntax_kind.is_trivia() {
                self.last_non_trivia_kind = Some(syntax_kind);
            }
            return Some((syntax_kind, text));
        }

        match self.logos_lexer.next() {
            Some(Ok(token)) => {
                let text = self.logos_lexer.slice();
                // Decide mapping strategy based on token kind and text
                let syntax_kind = match token {
                    Token::Ident => {
                        // If previous significant token was a sigil ($, @, %, *),
                        // treat following identifier as a variable name (IDENT),
                        // not as a keyword or operator word.
                        if self.prev_is_sigil() {
                            SyntaxKind::IDENT
                        } else {
                            match text {
                                // Disambiguated word operators
                                "eq" | "ne" | "gt" | "lt" | "ge" | "le" | "cmp" => {
                                    let ctx = context.expect(
                                        "context required for word operator disambiguation",
                                    );
                                    Self::disambiguate_str_op(ctx, text)
                                }
                                "x" => {
                                    let ctx =
                                        context.expect("context required for 'x' disambiguation");
                                    Self::disambiguate_x(ctx)
                                }
                                // Quote-like starters: treat as keywords unless '=>' follows
                                "q" | "qq" | "qr" | "qx" | "qw" | "m" | "s" | "tr" | "y" => {
                                    self.classify_quote_like_keyword(text)
                                }
                                _ => {
                                    if let Some(kw) = Self::map_ident_keyword(text) {
                                        kw
                                    } else {
                                        SyntaxKind::IDENT
                                    }
                                }
                            }
                        }
                    }
                    Token::Percent | Token::Star | Token::Ampersand | Token::Caret => {
                        let context =
                            context.expect("context required for ambiguous token disambiguation");
                        self.disambiguate_with(token, context)
                    }
                    _ => token.to_syntax_kind(),
                };

                // Quote-like auto-expansion disabled. Parser triggers begin_quote_like().

                // Special handling for __END__ and __DATA__: consume everything remaining as data section
                if matches!(syntax_kind, SyntaxKind::END_KW | SyntaxKind::DATA_KW) {
                    if !syntax_kind.is_trivia() {
                        self.last_non_trivia_kind = Some(syntax_kind);
                    }
                    return Some((syntax_kind, text));
                }

                // Track line position for POD detection
                self.update_line_position(text);

                if !syntax_kind.is_trivia() {
                    self.last_non_trivia_kind = Some(syntax_kind);
                }
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

    // Removed: pending clearing API (not needed with guarded expansion)

    /// Classify quote-like identifiers as keywords unless followed by fat comma (=>)
    fn classify_quote_like_keyword(&self, word: &str) -> SyntaxKind {
        if self.fat_comma_ahead() {
            return SyntaxKind::IDENT;
        }
        match word {
            // Sorted: q, qq, qr, qx, qw, m, s, tr, y
            "q" => SyntaxKind::Q_KW,
            "qq" => SyntaxKind::QQ_KW,
            "qr" => SyntaxKind::QR_KW,
            "qx" => SyntaxKind::QX_KW,
            "qw" => SyntaxKind::QW_KW,
            "m" => SyntaxKind::M_KW,
            "s" => SyntaxKind::S_KW,
            "tr" => SyntaxKind::TR_KW,
            "y" => SyntaxKind::Y_KW,
            _ => SyntaxKind::IDENT,
        }
    }

    /// Check if the next non-trivia is a fat comma (=>)
    fn fat_comma_ahead(&self) -> bool {
        let mut chars = self.logos_lexer.remainder().chars().peekable();
        while matches!(chars.peek().copied(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        if let (Some('='), Some('>')) = (chars.peek().copied(), {
            let mut t = chars.clone();
            t.next();
            t.peek().copied()
        }) {
            return true;
        }
        false
    }

    fn disambiguate_with(&self, token: Token, context: LexContext) -> SyntaxKind {
        match token {
            Token::Percent => Self::disambiguate_percent(context),
            Token::Star => Self::disambiguate_star(context),
            Token::Slash => Self::disambiguate_slash(context),
            Token::Ampersand => Self::disambiguate_ampersand(context),
            Token::Caret => Self::disambiguate_caret(context),
            Token::Pipe => SyntaxKind::BITWISE_OR,
            // Delimiters and other simple tokens
            Token::LParen
            | Token::LBrace
            | Token::LBracket
            | Token::RParen
            | Token::RBrace
            | Token::RBracket
            | Token::Greater
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

    /// Map known identifier keywords and quote-like starters
    fn map_ident_keyword(text: &str) -> Option<SyntaxKind> {
        Some(match text {
            // Control/decl keywords
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
            "use" => SyntaxKind::USE_KW,
            "no" => SyntaxKind::NO_KW,
            "return" => SyntaxKind::RETURN_KW,
            "undef" => SyntaxKind::UNDEF_KW,
            // Quote-like starters (treated as keywords regardless of expectation)
            "qw" => SyntaxKind::QW_KW,
            "q" => SyntaxKind::Q_KW,
            "qq" => SyntaxKind::QQ_KW,
            "qx" => SyntaxKind::QX_KW,
            "m" => SyntaxKind::M_KW,
            "qr" => SyntaxKind::QR_KW,
            // Logical word operators as keywords
            "not" => SyntaxKind::NOT_KW,
            "and" => SyntaxKind::AND_KW,
            "or" => SyntaxKind::OR_KW,
            "xor" => SyntaxKind::XOR_KW,
            _ => return None,
        })
    }

    /// Disambiguate % between hash sigil and modulo operator
    fn disambiguate_percent(context: LexContext) -> SyntaxKind {
        match context {
            LexContext::Value => {
                // When expecting a value, % is a sigil for a hash
                // Examples: "my %hash", "{ key => %val }"
                SyntaxKind::PERCENT
            }
            LexContext::Operator => {
                // When expecting an operator, % is the modulo operator
                // Examples: "@array % hash", "$var % other_var", "func() % 2"
                SyntaxKind::MODULO
            }
        }
    }

    /// Disambiguate * between typeglob sigil and multiplication operator
    fn disambiguate_star(context: LexContext) -> SyntaxKind {
        match context {
            LexContext::Value => {
                // When expecting a value, * is a typeglob sigil
                // Examples: "my *glob", "*{$name}", "*STDIN"
                SyntaxKind::ASTERISK
            }
            LexContext::Operator => {
                // When expecting an operator, * is the multiplication operator
                // Examples: "$a * $b", "func() * 2"
                SyntaxKind::STAR
            }
        }
    }

    fn disambiguate_str_op(context: LexContext, op: &str) -> SyntaxKind {
        match context {
            LexContext::Operator => {
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
            LexContext::Value => {
                // In ExpectingValue context, they are identifiers
                // Examples: "sub eq", "my $ne"
                SyntaxKind::IDENT
            }
        }
    }

    fn disambiguate_x(context: LexContext) -> SyntaxKind {
        match context {
            LexContext::Value => {
                // When expecting a value, x is an identifier
                // Examples: "sub x", "$x", "my $x"
                SyntaxKind::IDENT
            }
            LexContext::Operator => {
                // When expecting an operator, x is the repetition operator
                // Examples: "$str x 3", "'hello' x 2"
                SyntaxKind::X
            }
        }
    }

    // Removed: logical word operator disambiguation; mapping handled via keyword classification

    // Removed s/tr/y disambiguators; quote-like classification uses fat-comma and delimiter lookahead

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

    // Removed: variable-name tokenizer (parser handles complex variable forms)

    fn disambiguate_slash(_expect: LexContext) -> SyntaxKind {
        // Slash is always division operator in disambiguate context
        // because regex literals are handled in try_consume_regex_literal
        SyntaxKind::SLASH
    }

    /// Disambiguate ampersand (&) based on context
    fn disambiguate_ampersand(expect: LexContext) -> SyntaxKind {
        match expect {
            LexContext::Value => {
                // In value context, & is reference/sigil
                SyntaxKind::AMPERSAND
            }
            LexContext::Operator => {
                // In operator context, it's bitwise AND
                SyntaxKind::BITWISE_AND
            }
        }
    }

    /// Disambiguate caret (^) based on context
    fn disambiguate_caret(expect: LexContext) -> SyntaxKind {
        match expect {
            LexContext::Value => {
                // In expecting value context, ^ is likely a sigil
                // (e.g., special variables like $^O, $^X)
                SyntaxKind::CARET
            }
            LexContext::Operator => {
                // In operator context, it's bitwise XOR
                SyntaxKind::BITWISE_XOR
            }
        }
    }

    // Removed: is_builtin_function (no longer used)

    // Removed unused is_sigil / is_keyword helpers

    // Removed is_operator/is_literal; use SyntaxKind::is_operator/is_literal instead

    // Removed left/right delimiter helpers (no longer used)

    // Removed keyword/operator/identifier context handlers (parser-driven lexing)

    // Removed update_context: lexer is stateless

    // Removed: external set_context; parser drives lexing via explicit expectations

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

    // Removed: variable name consumption helper, as VariableName context is removed and
    // complex forms are handled by the parser.

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
        let mut cloned = self.clone();
        cloned.next_token_default()
    }

    /// Peek at the next non-trivia token without consuming it or changing lexer state
    #[must_use]
    pub fn peek_non_trivia_token(&self) -> Option<(SyntaxKind, &'a str)> {
        self.peek_non_trivia_with(LexContext::Value)
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
    /// For non-default contexts (QuoteLike), this expectation is ignored.
    pub fn next_token_with(&mut self, expect: LexContext) -> Option<(SyntaxKind, &'a str)> {
        let override_ctx = Some(expect);
        self.next_token_internal(override_ctx)
    }

    /// Peek the next non-trivia token using a given lexical expectation.
    /// This does not mutate the original lexer state.
    #[must_use]
    pub fn peek_non_trivia_with(&self, expect: LexContext) -> Option<(SyntaxKind, &'a str)> {
        let mut cloned = self.clone();
        let override_ctx = Some(expect);
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
    pub fn peek_with(&self, expect: LexContext) -> Option<(SyntaxKind, &'a str)> {
        let mut cloned = self.clone();
        let override_ctx = Some(expect);
        cloned.next_token_internal(override_ctx)
    }

    /// Convenience: default expectation is Value
    #[must_use]
    pub fn peek_non_trivia(&self) -> Option<(SyntaxKind, &'a str)> {
        self.peek_non_trivia_with(LexContext::Value)
    }

    /// Convenience: default expectation is Value
    pub fn next_token_default(&mut self) -> Option<(SyntaxKind, &'a str)> {
        self.next_token_with(LexContext::Value)
    }
}

#[cfg(test)]
mod tests;
