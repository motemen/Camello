use crate::SyntaxKind;
use logos::Logos;
use std::collections::VecDeque;

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
pub enum LexExpectation {
    /// Expecting a value, identifier, or sigil (after keywords, operators, sigils)
    Value,
    /// Expecting an operator (after identifiers, numbers, variables)
    Operator,
}

// No separate DisambiguationContext; use LexExpectation directly

// No LexerContext: parser provides expectations and lexer remains stateless

pub struct Lexer<'a> {
    logos_lexer: logos::Lexer<'a, Token>,
    at_line_start: bool, // Track if we're at the start of a line for POD detection
    // Track the last non-trivia token kind to derive a default expectation for standalone lexing
    last_non_trivia_kind: Option<SyntaxKind>,
    // Pending tokens produced by stateless expansions (e.g., quote-like operators)
    pending: VecDeque<(SyntaxKind, &'a str)>,
}

impl Clone for Lexer<'_> {
    fn clone(&self) -> Self {
        Self {
            logos_lexer: self.logos_lexer.clone(),
            at_line_start: self.at_line_start,
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
            last_non_trivia_kind: None,
            pending: VecDeque::new(),
        }
    }

    fn derive_default_expectation(&self) -> LexExpectation {
        match self.last_non_trivia_kind {
            None => LexExpectation::Value,
            Some(k) => {
                // After literals, identifiers, postfix deref, expect operator
                if self.is_literal(k)
                    || matches!(
                        k,
                        SyntaxKind::IDENT
                            | SyntaxKind::POSTFIX_DEREF_ARRAY
                            | SyntaxKind::POSTFIX_DEREF_HASH
                            | SyntaxKind::POSTFIX_DEREF_SCALAR
                    )
                {
                    LexExpectation::Operator
                } else if self.is_operator(k)
                    || matches!(
                        k,
                        SyntaxKind::L_PAREN | SyntaxKind::L_BRACE | SyntaxKind::L_BRACKET | SyntaxKind::COMMA
                            | SyntaxKind::FAT_COMMA
                    )
                    || k.is_keyword()
                {
                    // After operators, openings, commas, or keywords, expect a value
                    LexExpectation::Value
                } else {
                    // Default to value for safety
                    LexExpectation::Value
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
        // Array of token consumers to try in order
        let consumers = [
            Self::try_consume_file_test_op,
            Self::try_consume_regex_literal,
            Self::try_consume_io_operator,
        ];

        for consumer in consumers {
            if let Some(result) = consumer(self) {
                let (syntax_kind, text) = result;
                self.update_line_position(text);
                return Some((syntax_kind, text));
            }
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
        override_ctx: Option<LexExpectation>,
    ) -> Option<(SyntaxKind, &'a str)> {
        // Serve any pending expanded tokens first
        if let Some((k, t)) = self.pending.pop_front() {
            if !k.is_trivia() {
                self.last_non_trivia_kind = Some(k);
            }
            self.update_line_position(t);
            return Some((k, t));
        }
        // Quote-like context is no longer used; always handle default context
        self.handle_default_context_with(override_ctx)
    }

    // Raw data consumption is handled via consume_data_section from the parser

    // Quote-like handling moved to stateless expansion; no separate handler needed

    // Removed VariableName handling; variable names are parsed by the parser.

    // Removed SubPrototype handling; parser reads prototype symbols with explicit expectations

    /// Handle default context (Value | Operator): 通常ケースを担当
    fn handle_default_context_with(
        &mut self,
        override_ctx: Option<LexExpectation>,
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
        let is_value_context = matches!(override_ctx, Some(LexExpectation::Value));
        if is_value_context {
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
                                    let ctx = override_ctx.expect(
                                        "context required for word operator disambiguation",
                                    );
                                    Self::disambiguate_str_op(ctx, text)
                                }
                                "x" => {
                                    let ctx = override_ctx
                                        .expect("context required for 'x' disambiguation");
                                    Self::disambiguate_x(ctx)
                                }
                                "s" => {
                                    let ctx = override_ctx
                                        .expect("context required for 's' disambiguation");
                                    self.disambiguate_s(ctx)
                                }
                                "tr" => {
                                    let ctx = override_ctx
                                        .expect("context required for 'tr' disambiguation");
                                    self.disambiguate_tr(ctx)
                                }
                                "y" => {
                                    let ctx = override_ctx
                                        .expect("context required for 'y' disambiguation");
                                    self.disambiguate_y(ctx)
                                }
                                _ => {
                                    if let Some(kw) = Self::map_ident_keyword(text) {
                                        // For quote-like keywords, expand into pending tokens
                                        if matches!(
                                            kw,
                                            SyntaxKind::QW_KW
                                                | SyntaxKind::Q_KW
                                                | SyntaxKind::QQ_KW
                                                | SyntaxKind::QX_KW
                                                | SyntaxKind::M_KW
                                                | SyntaxKind::QR_KW
                                                | SyntaxKind::S_KW
                                                | SyntaxKind::TR_KW
                                                | SyntaxKind::Y_KW
                                        ) {
                                            self.enqueue_quote_like_from(kw, text);
                                            if let Some((k, t)) = self.pending.pop_front() {
                                                // Return the keyword token immediately
                                                self.update_line_position(t);
                                                if !k.is_trivia() {
                                                    self.last_non_trivia_kind = Some(k);
                                                }
                                                return Some((k, t));
                                            }
                                        }
                                        kw
                                    } else {
                                        SyntaxKind::IDENT
                                    }
                                }
                            }
                        }
                    }
                    Token::Percent | Token::Star | Token::Ampersand | Token::Caret => {
                        let expect = override_ctx
                            .expect("context required for ambiguous token disambiguation");
                        self.disambiguate_with(token, text, expect)
                    }
                    _ => token.to_syntax_kind(),
                };

                // If we identified a quote-like keyword (e.g., from disambiguated 's', 'tr', 'y'),
                // enqueue its expansion and return the keyword immediately.
                if matches!(
                    syntax_kind,
                    SyntaxKind::QW_KW
                        | SyntaxKind::Q_KW
                        | SyntaxKind::QQ_KW
                        | SyntaxKind::QX_KW
                        | SyntaxKind::M_KW
                        | SyntaxKind::QR_KW
                        | SyntaxKind::S_KW
                        | SyntaxKind::TR_KW
                        | SyntaxKind::Y_KW
                ) {
                    self.enqueue_quote_like_from(syntax_kind, text);
                    if let Some((k, t)) = self.pending.pop_front() {
                        self.update_line_position(t);
                        if !k.is_trivia() {
                            self.last_non_trivia_kind = Some(k);
                        }
                        return Some((k, t));
                    }
                }

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

    /// Enqueue a complete quote-like token sequence starting at the current input position.
    /// The first pending item is always the keyword itself with `prefix_text`.
    fn enqueue_quote_like_from(&mut self, prefix: SyntaxKind, prefix_text: &'a str) {
        // Always push the keyword token first
        self.pending.push_back((prefix, prefix_text));

        // Work on a snapshot of the remaining input after the keyword
        let input = self.logos_lexer.remainder();
        let bytes = input.as_bytes();
        let mut i = 0usize;

        // Optionally capture leading whitespace between keyword and delimiter
        let ws_start = i;
        while i < bytes.len() {
            let ch = input[i..].chars().next().unwrap();
            if ch.is_whitespace() {
                i += ch.len_utf8();
            } else {
                break;
            }
        }
        if i > ws_start {
            self.pending
                .push_back((SyntaxKind::WHITESPACE, &input[ws_start..i]));
        }

        // Opening delimiter
        if i >= bytes.len() {
            return;
        }
        let open = input[i..].chars().next().unwrap();
        let open_len = open.len_utf8();
        // Push opening delimiter token
        self.pending
            .push_back((SyntaxKind::DELIMITER, &input[i..i + open_len]));
        i += open_len;

        // Helper to get closing delimiter for an opening character
        fn closing_for(c: char) -> char {
            match c {
                '(' => ')',
                '[' => ']',
                '{' => '}',
                '<' => '>',
                other => other,
            }
        }

        fn is_paired(c: char) -> bool {
            matches!(c, '(' | '[' | '{' | '<')
        }

        // Scan content until closing delimiter, with simple nesting for paired delimiters
        fn scan_until<'a>(s: &'a str, mut idx: usize, open: char, close: char) -> (usize, &'a str) {
            let mut escaped = false;
            let mut nest = 0i32;
            let bytes = s.as_bytes();
            while idx < bytes.len() {
                let ch = s[idx..].chars().next().unwrap();
                let w = ch.len_utf8();
                if escaped {
                    escaped = false;
                    idx += w;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    idx += w;
                    continue;
                }
                if ch == close {
                    if nest == 0 {
                        break;
                    } else {
                        nest -= 1;
                        idx += w;
                        continue;
                    }
                }
                if ch == open && is_paired(open) {
                    nest += 1;
                }
                idx += w;
            }
            let content = &s[..idx];
            (idx, content)
        }

        // QW: special processing of words
        fn enqueue_qw<'a>(pending: &mut VecDeque<(SyntaxKind, &'a str)>, s: &'a str, mut idx: usize, close: char) -> usize {
            let bytes = s.as_bytes();
            while idx < bytes.len() {
                let ch = s[idx..].chars().next().unwrap();
                if ch == close {
                    break;
                }
                if ch.is_whitespace() {
                    let start = idx;
                    let mut j = idx;
                    while j < bytes.len() {
                        let c2 = s[j..].chars().next().unwrap();
                        if !c2.is_whitespace() { break; }
                        j += c2.len_utf8();
                    }
                    pending.push_back((SyntaxKind::WHITESPACE, &s[start..j]));
                    idx = j;
                } else {
                    let start = idx;
                    let mut j = idx;
                    while j < bytes.len() {
                        let c2 = s[j..].chars().next().unwrap();
                        if c2 == close || c2.is_whitespace() { break; }
                        j += c2.len_utf8();
                    }
                    if j > start {
                        pending.push_back((SyntaxKind::QW_STRING, &s[start..j]));
                        idx = j;
                    } else {
                        break;
                    }
                }
            }
            idx
        }

        // Determine closing delimiter for first part
        let close1 = closing_for(open);

        match prefix {
            SyntaxKind::QW_KW => {
                i = enqueue_qw(&mut self.pending, &input, i, close1);
                // Closing delimiter of first part
                if i < input.len() && input[i..].starts_with(close1) {
                    self.pending
                        .push_back((SyntaxKind::DELIMITER, &input[i..i + close1.len_utf8()]));
                    i += close1.len_utf8();
                }
            }
            SyntaxKind::Q_KW | SyntaxKind::QQ_KW | SyntaxKind::QX_KW | SyntaxKind::M_KW | SyntaxKind::QR_KW => {
                let (end_idx, content) = scan_until(&input[i..], 0, open, close1);
                let content_kind = match prefix {
                    SyntaxKind::Q_KW => SyntaxKind::LITERAL_STRING,
                    SyntaxKind::QQ_KW | SyntaxKind::QX_KW => SyntaxKind::INTERPOLATED_STRING,
                    SyntaxKind::M_KW | SyntaxKind::QR_KW => SyntaxKind::REGEX_PATTERN,
                    _ => SyntaxKind::LITERAL_STRING,
                };
                if !content.is_empty() {
                    self.pending.push_back((content_kind, content));
                }
                i += end_idx;
                // Closing delimiter
                if i < input.len() && input[i..].starts_with(close1) {
                    self.pending
                        .push_back((SyntaxKind::DELIMITER, &input[i..i + close1.len_utf8()]));
                    i += close1.len_utf8();
                }
                // Optional flags for m/qr
                if matches!(prefix, SyntaxKind::M_KW | SyntaxKind::QR_KW) {
                    let valid = "msixpodualngcer";
                    let mut j = i;
                    let mut any = false;
                    let mut all_valid = true;
                    while j < input.len() {
                        let c = input[j..].chars().next().unwrap();
                        if c.is_alphabetic() {
                            any = true;
                            if !valid.contains(c) { all_valid = false; }
                            j += c.len_utf8();
                        } else { break; }
                    }
                    if any {
                        let kind = if all_valid {
                            if prefix == SyntaxKind::M_KW { SyntaxKind::M_FLAGS } else { SyntaxKind::QR_FLAGS }
                        } else { SyntaxKind::ERROR };
                        self.pending.push_back((kind, &input[i..j]));
                        i = j;
                    }
                }
            }
            SyntaxKind::S_KW | SyntaxKind::TR_KW | SyntaxKind::Y_KW => {
                // First part
                let (end_idx1, c1) = scan_until(&input[i..], 0, open, close1);
                let first_kind = match prefix {
                    SyntaxKind::S_KW => SyntaxKind::REGEX_PATTERN,
                    _ => SyntaxKind::TR_SEARCH_LIST,
                };
                if !c1.is_empty() {
                    self.pending.push_back((first_kind, c1));
                }
                i += end_idx1;
                // Close first
                if i < input.len() && input[i..].starts_with(close1) {
                    self.pending
                        .push_back((SyntaxKind::DELIMITER, &input[i..i + close1.len_utf8()]));
                    i += close1.len_utf8();
                }
                // Second part (handle symmetric vs paired delimiters)
                if is_paired(open) {
                    if i < input.len() && input[i..].starts_with(open) {
                        self.pending
                            .push_back((SyntaxKind::DELIMITER, &input[i..i + open_len]));
                        i += open_len;
                    }
                    let (end_idx2, c2) = scan_until(&input[i..], 0, open, close1);
                    let second_kind = match prefix {
                        SyntaxKind::S_KW => SyntaxKind::INTERPOLATED_STRING,
                        _ => SyntaxKind::TR_REPLACEMENT_LIST,
                    };
                    if !c2.is_empty() {
                        self.pending.push_back((second_kind, c2));
                    }
                    i += end_idx2;
                    if i < input.len() && input[i..].starts_with(close1) {
                        self.pending.push_back((SyntaxKind::DELIMITER, &input[i..i + close1.len_utf8()]));
                        i += close1.len_utf8();
                    }
                } else {
                    // Symmetric delimiter like '/'
                    if i < input.len() && input[i..].starts_with(close1) {
                        // Empty replacement: just the closing delimiter
                        self.pending.push_back((SyntaxKind::DELIMITER, &input[i..i + close1.len_utf8()]));
                        i += close1.len_utf8();
                    } else {
                        let (end_idx2, c2) = scan_until(&input[i..], 0, open, close1);
                        let second_kind = match prefix {
                            SyntaxKind::S_KW => SyntaxKind::INTERPOLATED_STRING,
                            _ => SyntaxKind::TR_REPLACEMENT_LIST,
                        };
                        if !c2.is_empty() {
                            self.pending.push_back((second_kind, c2));
                        }
                        i += end_idx2;
                        if i < input.len() && input[i..].starts_with(close1) {
                            self.pending.push_back((SyntaxKind::DELIMITER, &input[i..i + close1.len_utf8()]));
                            i += close1.len_utf8();
                        }
                    }
                }
                // Optional flags
                let valid = if prefix == SyntaxKind::TR_KW || prefix == SyntaxKind::Y_KW { "cdsr" } else { "msixpodualngcer" };
                let mut j = i;
                let mut any = false;
                let mut all_valid = true;
                while j < input.len() {
                    let c = input[j..].chars().next().unwrap();
                    if c.is_alphabetic() {
                        any = true;
                        if !valid.contains(c) { all_valid = false; }
                        j += c.len_utf8();
                    } else { break; }
                }
                if any {
                    let kind = if all_valid {
                        match prefix { SyntaxKind::S_KW => SyntaxKind::S_FLAGS, SyntaxKind::TR_KW | SyntaxKind::Y_KW => SyntaxKind::TR_FLAGS, _ => SyntaxKind::ERROR }
                    } else { SyntaxKind::ERROR };
                    self.pending.push_back((kind, &input[i..j]));
                    i = j;
                }
            }
            _ => {}
        }

        // Finally advance the underlying lexer by the total bytes we consumed after the keyword
        self.logos_lexer.bump(i);
    }

    fn disambiguate_with(
        &self,
        token: Token,
        text: &str,
        expect: LexExpectation,
    ) -> SyntaxKind {
        match token {
            Token::Ident => {
                // Common keywords that need disambiguation regardless of context
                match text {
                    "s" => self.disambiguate_s(expect),
                    "tr" => self.disambiguate_tr(expect),
                    "y" => self.disambiguate_y(expect),
                    "x" => Self::disambiguate_x(expect),
                    "eq" | "ne" | "gt" | "lt" | "ge" | "le" | "cmp" => {
                        Self::disambiguate_str_op(expect, text)
                    }
                    "not" | "and" | "or" | "xor" => {
                        Self::disambiguate_logical_op(expect, text)
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
                Self::disambiguate_percent(expect)
            }
            Token::Star => {
                Self::disambiguate_star(expect)
            }
            Token::Slash => {
                Self::disambiguate_slash(expect)
            }
            Token::Ampersand => Self::disambiguate_ampersand(expect),
            Token::Caret => Self::disambiguate_caret(expect),
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
    fn disambiguate_percent(expect: LexExpectation) -> SyntaxKind {
        match expect {
            LexExpectation::Value => {
                // When expecting a value, % is a sigil for a hash
                // Examples: "my %hash", "{ key => %val }"
                SyntaxKind::PERCENT
            }
            LexExpectation::Operator => {
                // When expecting an operator, % is the modulo operator
                // Examples: "@array % hash", "$var % other_var", "func() % 2"
                SyntaxKind::MODULO
            }
        }
    }

    /// Disambiguate * between typeglob sigil and multiplication operator  
    fn disambiguate_star(expect: LexExpectation) -> SyntaxKind {
        match expect {
            LexExpectation::Value => {
                // When expecting a value, * is a typeglob sigil
                // Examples: "my *glob", "*{$name}", "*STDIN"
                SyntaxKind::ASTERISK
            }
            LexExpectation::Operator => {
                // When expecting an operator, * is the multiplication operator
                // Examples: "$a * $b", "func() * 2"
                SyntaxKind::STAR
            }
        }
    }

    fn disambiguate_str_op(expect: LexExpectation, op: &str) -> SyntaxKind {
        match expect {
            LexExpectation::Operator => {
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
            LexExpectation::Value => {
                // In ExpectingValue context, they are identifiers
                // Examples: "sub eq", "my $ne"
                SyntaxKind::IDENT
            }
        }
    }

    fn disambiguate_x(expect: LexExpectation) -> SyntaxKind {
        match expect {
            LexExpectation::Value => {
                // When expecting a value, x is an identifier
                // Examples: "sub x", "$x", "my $x"
                SyntaxKind::IDENT
            }
            LexExpectation::Operator => {
                // When expecting an operator, x is the repetition operator
                // Examples: "$str x 3", "'hello' x 2"
                SyntaxKind::X
            }
        }
    }

    fn disambiguate_s(&self, expect: LexExpectation) -> SyntaxKind {
        match expect {
            LexExpectation::Value => {
                // Look ahead to determine if this is s/// substitution or a bareword function call
                let remainder = self.logos_lexer.remainder();

                // Check what follows 's' after optional whitespace
                let mut iter = remainder.chars().peekable();
                while let Some(&c) = iter.peek() {
                    if c.is_whitespace() {
                        iter.next();
                        continue;
                    }
                    // If first non-whitespace char is alphanumeric or sigil, it's likely a function call
                    if c.is_alphanumeric() || matches!(c, '$' | '@' | '%') {
                        return SyntaxKind::IDENT;
                    }
                    // Otherwise, it's likely substitution
                    return SyntaxKind::S_KW;
                }

                // If we reach end of input after 's', assume function call
                SyntaxKind::IDENT
            }
            LexExpectation::Operator => {
                // In operator context, prefer treating `s` as substitution if a delimiter can follow,
                // except when immediately followed by fat comma (=>), where it should be an identifier.
                let remainder = self.logos_lexer.remainder();
                let mut iter = remainder.chars().peekable();
                // Skip optional whitespace
                while let Some(&c) = iter.peek() {
                    if c.is_whitespace() { iter.next(); } else { break; }
                }
                // Check for fat comma '=>'
                if let (Some('='), Some('>')) = (iter.peek().copied(), {
                    let mut tmp = iter.clone();
                    tmp.next();
                    tmp.peek().copied()
                }) {
                    return SyntaxKind::IDENT;
                }
                // Check if next non-whitespace char could be a common delimiter
                if let Some(next) = iter.peek().copied() {
                    if matches!(next, '/' | '(' | '[' | '{' | '<' | '|' | '#') {
                        return SyntaxKind::S_KW;
                    }
                }
                // Fallback: treat as identifier
                SyntaxKind::IDENT
            }
        }
    }

    fn disambiguate_logical_op(expect: LexExpectation, op: &str) -> SyntaxKind {
        match expect {
            LexExpectation::Operator => {
                // When expecting an operator, not/and/or/xor are logical operators
                match op {
                    "not" => SyntaxKind::NOT_KW,
                    "and" => SyntaxKind::AND_KW,
                    "or" => SyntaxKind::OR_KW,
                    "xor" => SyntaxKind::XOR_KW,
                    _ => SyntaxKind::IDENT, // Handle unknown ops gracefully
                }
            }
            LexExpectation::Value => {
                // In ExpectingValue context, they are identifiers
                // Examples: "sub not", "my $and", "or die"
                SyntaxKind::IDENT
            }
        }
    }

    fn disambiguate_tr(&self, expect: LexExpectation) -> SyntaxKind {
        match expect {
            LexExpectation::Value => {
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
            LexExpectation::Operator => {
                // When expecting an operator, tr is the transliteration operator
                // Examples: "$str tr/a-z/A-Z/"
                SyntaxKind::TR_KW
            }
        }
    }

    fn disambiguate_y(&self, expect: LexExpectation) -> SyntaxKind {
        // y is an alias for tr, so use the same logic but return Y_KW
        match expect {
            LexExpectation::Value => {
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
            LexExpectation::Operator => {
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

    // Removed: variable-name tokenizer (parser handles complex variable forms)

    fn disambiguate_slash(_expect: LexExpectation) -> SyntaxKind {
        // Slash is always division operator in disambiguate context
        // because regex literals are handled in try_consume_regex_literal
        SyntaxKind::SLASH
    }

    /// Disambiguate ampersand (&) based on context
    fn disambiguate_ampersand(expect: LexExpectation) -> SyntaxKind {
        match expect {
            LexExpectation::Value => {
                // In value context, & is reference/sigil
                SyntaxKind::AMPERSAND
            }
            LexExpectation::Operator => {
                // In operator context, it's bitwise AND
                SyntaxKind::BITWISE_AND
            }
        }
    }

    /// Disambiguate caret (^) based on context
    fn disambiguate_caret(expect: LexExpectation) -> SyntaxKind {
        match expect {
            LexExpectation::Value => {
                // In expecting value context, ^ is likely a sigil
                // (e.g., special variables like $^O, $^X)
                SyntaxKind::CARET
            }
            LexExpectation::Operator => {
                // In operator context, it's bitwise XOR
                SyntaxKind::BITWISE_XOR
            }
        }
    }

    // Removed: is_builtin_function (no longer used)

    // Removed unused is_sigil / is_keyword helpers

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
        self.peek_non_trivia_with(LexExpectation::Value)
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
    pub fn next_token_with(
        &mut self,
        expect: LexExpectation,
    ) -> Option<(SyntaxKind, &'a str)> {
        let override_ctx = Some(expect);
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
    pub fn peek_with(&self, expect: LexExpectation) -> Option<(SyntaxKind, &'a str)> {
        let mut cloned = self.clone();
        let override_ctx = Some(expect);
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
