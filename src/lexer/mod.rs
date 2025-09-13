use crate::SyntaxKind;
use logos::Logos;
use std::collections::VecDeque;

mod quote;

#[derive(Debug, Clone)]
struct HeredocMarker<'a> {
    marker: &'a str,
}

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    // Sigils (prefixes indicating variable type)
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

    // Data section keywords (must come before Ident to take precedence)
    #[token("__END__")]
    EndKw,

    #[token("__DATA__")]
    DataKw,

    // POD commands (must be at line start)
    // Note: POD detection is handled manually in lexer due to line-start requirement
    PodCommand,

    // Identifiers (subroutine names, etc.)
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,

    // Literals
    // Numeric literals with underscores and multiple bases
    // Hex float (e.g., 0x1.999ap-4), must have '.' and 'p' exponent
    #[regex(
        r"0x[0-9A-Fa-f](?:_*[0-9A-Fa-f])*\.[0-9A-Fa-f](?:_*[0-9A-Fa-f])*[pP][+-]?[0-9](?:_*[0-9])*",
        priority = 3
    )]
    // Hex, binary, and octal integers
    #[regex(r"0x[0-9A-Fa-f](?:_*[0-9A-Fa-f])*", priority = 2)]
    #[regex(r"0b[01](?:_*[01])*", priority = 2)]
    #[regex(r"0o[0-7](?:_*[0-7])*", priority = 2)]
    // Decimal numbers (int, float, scientific)
    #[regex(
        r"[0-9](?:_*[0-9])*(?:\.[0-9](?:_*[0-9])*)?(?:[eE][+-]?[0-9](?:_*[0-9])*)?",
        priority = 1
    )]
    #[regex(r"\.[0-9](?:_*[0-9])*(?:[eE][+-]?[0-9](?:_*[0-9])*)?", priority = 1)]
    Number,

    #[regex(r#""([^"\\]|\\.)*""#)]
    #[regex(r"'([^'\\]|\\.)*'")]
    String,

    // Version literal (v1.23, v5.008_001, etc.)
    #[regex(r"v[0-9]+(\.[0-9_]+)*")]
    Version,

    // Bare version number (e.g., 5.24.1) - contextually determined
    #[regex(r"[0-9]+(\.[0-9]+){2,}")]
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

    #[token("<<")]
    ShiftLeft,

    #[token(">>")]
    ShiftRight,

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

    #[token("~")]
    Tilde,

    #[token("//")]
    DefinedOr,

    #[token("<=>")]
    Spaceship,

    #[token("=")]
    Eq,

    #[token("++")]
    Increment,

    #[token("--")]
    Decrement,

    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("...")]
    DotDotDot,

    #[token("..")]
    DotDot,

    #[token(".")]
    Dot,

    // Multiplicative operators
    #[token("**")]
    Exponent,

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

    // Newlines (tokenized separately as they are significant)
    #[regex(r"\r\n|\r|\n")]
    Newline,

    // Comments
    #[regex(r"#[^\r\n]*")]
    Comment,

    // Data section (all text after __END__ / __DATA__)
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
            Token::Increment => SyntaxKind::INCREMENT,
            Token::Decrement => SyntaxKind::DECREMENT,
            Token::Plus => SyntaxKind::PLUS,
            Token::Minus => SyntaxKind::MINUS,
            Token::DotDotDot => SyntaxKind::RANGE_EXCLUSIVE,
            Token::DotDot => SyntaxKind::RANGE,
            Token::Dot => SyntaxKind::DOT,
            Token::Arrow => SyntaxKind::ARROW,
            Token::FatComma => SyntaxKind::FAT_COMMA,
            Token::Exponent => SyntaxKind::EXPONENT,
            Token::Star => SyntaxKind::STAR,
            Token::Slash => SyntaxKind::SLASH,
            Token::Percent => SyntaxKind::MODULO,
            Token::Greater => SyntaxKind::GT,
            Token::Less => SyntaxKind::LT,
            Token::ShiftLeft => SyntaxKind::SHIFT_LEFT,
            Token::ShiftRight => SyntaxKind::SHIFT_RIGHT,
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
            Token::Tilde => SyntaxKind::BITWISE_NOT,
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

/// External lexical context hint provided by the parser to disambiguate
/// only operator/value sensitive tokens in default contexts.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LexContext {
    /// Expecting a value, identifier, or sigil (after keywords, operators, sigils)
    #[default]
    Value,
    /// Expecting an operator (after identifiers, numbers, variables)
    Operator,
}

// No separate DisambiguationContext; use LexContext directly

// No LexerContext: parser provides context and lexer remains stateless

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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum LexerMode {
    #[default]
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
    // Pending tokens produced by stateless expansions (e.g., quote-like operators)
    pending: VecDeque<(SyntaxKind, &'a str)>,
    heredoc_queue: VecDeque<HeredocMarker<'a>>,
}

impl Clone for Lexer<'_> {
    fn clone(&self) -> Self {
        Self {
            logos_lexer: self.logos_lexer.clone(),
            at_line_start: self.at_line_start,
            mode: self.mode,
            pending: self.pending.clone(),
            heredoc_queue: self.heredoc_queue.clone(),
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = (SyntaxKind, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        // Default to Value context for standalone lexer usage
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
            pending: VecDeque::new(),
            heredoc_queue: VecDeque::new(),
        }
    }

    /// Handle special tokens when in Value context
    fn try_handle_expecting_value_context(&mut self) -> Option<(SyntaxKind, &'a str)> {
        // 1) File test operator like -f
        if let Some(result) = self.try_consume_file_test_op() {
            let (k, t) = result;
            self.update_line_position(t);
            return Some((k, t));
        }
        // 2) Regex literal /.../
        if let Some(result) = self.try_consume_regex_literal() {
            let (k, t) = result;
            self.update_line_position(t);
            return Some((k, t));
        }
        // 3) IO operator like <...>
        if let Some(result) = self.try_consume_io_operator() {
            let (k, t) = result;
            self.update_line_position(t);
            return Some((k, t));
        }
        None
    }

    pub fn next_token(&mut self) -> Option<(SyntaxKind, &'a str)> {
        self.next_token_with_context(Default::default())
    }

    /// Core tokenization step with an optional lexical context override.
    /// When `override_ctx` is provided, it influences only this single step.
    fn next_token_internal(
        &mut self,
        context: Option<LexContext>,
    ) -> Option<(SyntaxKind, &'a str)> {
        // Serve any pending expanded tokens first
        if let Some((k, t)) = self.pending.pop_front() {
            self.update_line_position(t);
            return Some((k, t));
        }
        // Quote-like context handling (parser-driven)
        if let LexerMode::QuoteLike { .. } = self.mode {
            if let Some((k, t)) = self.try_handle_quote_like_internal() {
                self.update_line_position(t);
                return Some((k, t));
            }
        }
        // Default context
        self.handle_default_context_with(context)
    }

    // Raw data consumption is handled via consume_data_section from the parser

    /// Handle default context (Value | Operator): handles normal cases
    fn handle_default_context_with(
        &mut self,
        context: Option<LexContext>,
    ) -> Option<(SyntaxKind, &'a str)> {
        // If already in quote-like context, delegate immediately (pure state machine)
        if let LexerMode::QuoteLike { .. } = self.mode {
            if let Some((k, t)) = self.try_handle_quote_like_internal() {
                self.update_line_position(t);
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

        // Handle heredoc start regardless of context
        let in_quote_like = matches!(self.mode, LexerMode::QuoteLike { .. });
        if !in_quote_like {
            if let Some(result) = self.try_consume_heredoc_start() {
                let (syntax_kind, text) = result;
                self.update_line_position(text);
                return Some((syntax_kind, text));
            }
        }

        // Handle other special tokens only in Value context
        let is_value_context = matches!(context, Some(LexContext::Value));
        if is_value_context && !in_quote_like {
            if let Some(result) = self.try_handle_expecting_value_context() {
                let (syntax_kind, text) = result;
                self.update_line_position(text);
                return Some((syntax_kind, text));
            }
        }

        // Handle postfix dereference operators (->@*, ->%*, ->$*)
        if let Some((syntax_kind, text)) = self.try_consume_postfix_deref() {
            self.update_line_position(text);
            return Some((syntax_kind, text));
        }

        match self.logos_lexer.next() {
            Some(Ok(token)) => {
                let text = self.logos_lexer.slice();
                // Decide mapping strategy based on token kind and text via a single disambiguator
                let syntax_kind = {
                    // If previous token was a sigil, force IDENT for following identifier
                    if let Some(ctx) = context {
                        self.disambiguate(&token, text, ctx)
                    } else {
                        token.to_syntax_kind()
                    }
                };

                // Quote-like auto-expansion disabled. Parser triggers begin_quote_like().

                // Special handling for __END__ and __DATA__: consume everything remaining as data section
                if matches!(syntax_kind, SyntaxKind::END_KW | SyntaxKind::DATA_KW) {
                    return Some((syntax_kind, text));
                }

                // Track line position for POD detection
                self.update_line_position(text);

                Some((syntax_kind, text))
            }
            Some(Err(())) => {
                // Process as error token
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

    fn disambiguate(&self, token: &Token, text: &str, ctx: LexContext) -> SyntaxKind {
        match token {
            // Identifier words: operators, quote-like starters, or keywords
            Token::Ident => match text {
                // Word operators in operator context; otherwise identifiers
                "eq" | "ne" | "gt" | "lt" | "ge" | "le" | "cmp" => match ctx {
                    LexContext::Operator => match text {
                        "eq" => SyntaxKind::STR_EQ,
                        "ne" => SyntaxKind::STR_NE,
                        "gt" => SyntaxKind::STR_GT,
                        "lt" => SyntaxKind::STR_LT,
                        "ge" => SyntaxKind::STR_GE,
                        "le" => SyntaxKind::STR_LE,
                        "cmp" => SyntaxKind::STR_CMP,
                        _ => unreachable!(),
                    },
                    LexContext::Value => SyntaxKind::IDENT,
                },
                // Repetition operator vs identifier
                "x" => match ctx {
                    LexContext::Operator => SyntaxKind::X,
                    LexContext::Value => SyntaxKind::IDENT,
                },
                _ => Self::map_ident_keyword(text).unwrap_or(SyntaxKind::IDENT),
            },
            // Ambiguous symbol tokens depending on context
            Token::Percent => match ctx {
                LexContext::Value => SyntaxKind::PERCENT,
                LexContext::Operator => SyntaxKind::MODULO,
            },
            Token::Star => match ctx {
                LexContext::Value => SyntaxKind::ASTERISK,
                LexContext::Operator => SyntaxKind::STAR,
            },
            Token::DotDotDot => match ctx {
                LexContext::Operator => SyntaxKind::RANGE_EXCLUSIVE,
                LexContext::Value => SyntaxKind::ELLIPSIS,
            },
            Token::Slash => SyntaxKind::SLASH, // regex literals handled elsewhere
            Token::Ampersand => match ctx {
                LexContext::Value => SyntaxKind::AMPERSAND,
                LexContext::Operator => SyntaxKind::BITWISE_AND,
            },
            Token::Caret => match ctx {
                LexContext::Value => SyntaxKind::CARET,
                LexContext::Operator => SyntaxKind::BITWISE_XOR,
            },
            Token::Pipe => SyntaxKind::BITWISE_OR,
            // Everything else: direct mapping
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
            // Quote-like starters (treated as keywords regardless of context)
            "q" => SyntaxKind::Q_KW,
            "qq" => SyntaxKind::QQ_KW,
            "qr" => SyntaxKind::QR_KW,
            "qx" => SyntaxKind::QX_KW,
            "qw" => SyntaxKind::QW_KW,
            "m" => SyntaxKind::M_KW,
            "s" => SyntaxKind::S_KW,
            "tr" => SyntaxKind::TR_KW,
            "y" => SyntaxKind::Y_KW,
            // Logical word operators as keywords
            "not" => SyntaxKind::NOT_KW,
            "and" => SyntaxKind::AND_KW,
            "or" => SyntaxKind::OR_KW,
            "xor" => SyntaxKind::XOR_KW,
            _ => return None,
        })
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

    fn try_consume_heredoc_start(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if !remainder.starts_with("<<") {
            return None;
        }

        let after = &remainder[2..];
        let mut chars = after.char_indices();
        let (idx0, first_ch) = chars.next()?;
        if !(first_ch.is_ascii_alphabetic() || first_ch == '_') {
            return None;
        }
        let mut end = idx0 + first_ch.len_utf8();
        for (idx, ch) in chars {
            if ch.is_alphanumeric() || ch == '_' {
                end = idx + ch.len_utf8();
            } else {
                break;
            }
        }

        let marker = &after[..end];
        let total_len = 2 + end;
        let text = &remainder[..total_len];
        self.logos_lexer.bump(total_len);
        self.heredoc_queue.push_back(HeredocMarker { marker });
        Some((SyntaxKind::HEREDOC_START, text))
    }

    fn try_consume_file_test_op(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if !remainder.starts_with('-') {
            return None;
        }

        let mut chars = remainder.chars();
        if chars.next() != Some('-') {
            return None;
        }

        let op = chars.next()?;
        if !op.is_alphabetic() {
            return None;
        }

        // If the third char exists and is alphanumeric, it's not a file test op (e.g., -abcde)
        if remainder.chars().nth(2).is_some_and(char::is_alphanumeric) {
            return None;
        }

        let text = &remainder[..2];
        self.logos_lexer.bump(2);
        Some((SyntaxKind::FILE_TEST_OP, text))
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

    /// Track line position for POD detection
    fn update_line_position(&mut self, text: &str) {
        // Check if this token contains a newline
        if text.contains('\n') {
            self.at_line_start = true;
            if !self.heredoc_queue.is_empty() {
                self.bump_until_marker();
            }
        } else if text.chars().any(|c| !c.is_whitespace()) {
            // Non-whitespace content means we're no longer at line start
            self.at_line_start = false;
        }
    }

    fn bump_until_marker(&mut self) {
        if let Some(marker_info) = self.heredoc_queue.pop_front() {
            let remainder = self.logos_lexer.remainder();
            let marker = marker_info.marker;
            let mut search_offset = 0;
            let mut found_marker_start = None;

            while let Some(relative_pos) = remainder[search_offset..].find(marker) {
                let marker_start = search_offset + relative_pos;
                let at_line_start =
                    marker_start == 0 || remainder.as_bytes()[marker_start - 1] == b'\n';

                if at_line_start {
                    let after_marker = marker_start + marker.len();
                    if after_marker == remainder.len()
                        || remainder.as_bytes()[after_marker] == b'\n'
                    {
                        found_marker_start = Some(marker_start);
                        break;
                    }
                }

                search_offset = marker_start + 1;
            }

            if let Some(marker_start) = found_marker_start {
                let content_end = marker_start;
                let content = &remainder[..content_end];

                let end_start = marker_start;
                let mut end_end = end_start + marker.len();
                if remainder.len() > end_end && remainder.as_bytes()[end_end] == b'\n' {
                    end_end += 1; // include trailing newline after marker
                }
                let end_slice = &remainder[end_start..end_end];

                self.pending
                    .push_back((SyntaxKind::HEREDOC_CONTENT, content));
                self.pending.push_back((SyntaxKind::HEREDOC_END, end_slice));

                self.logos_lexer.bump(end_end);
            } else {
                // Unclosed heredoc: consume the rest of the input as content.
                let content = remainder;
                self.logos_lexer.bump(remainder.len());
                self.pending
                    .push_back((SyntaxKind::HEREDOC_CONTENT, content));
            }
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

    #[must_use]
    pub fn has_pending_heredoc(&self) -> bool {
        !self.heredoc_queue.is_empty()
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
        self.peek_non_trivia_with_context(LexContext::Value)
    }

    /// Peek ahead multiple tokens, skipping trivia, and return the first non-trivia token
    /// that matches any of the given kinds
    #[must_use]
    pub fn peek_for_any(&self, target_kinds: &[SyntaxKind]) -> Option<(SyntaxKind, &'a str)> {
        self.clone()
            .find(|(kind, _)| !kind.is_trivia())
            .filter(|(kind, _)| target_kinds.contains(kind))
    }

    /// Get the next token using an explicit lexical context for ambiguous cases.
    /// For non-default contexts (QuoteLike), this context hint is ignored.
    pub fn next_token_with_context(
        &mut self,
        context: LexContext,
    ) -> Option<(SyntaxKind, &'a str)> {
        self.next_token_internal(Some(context))
    }

    /// Peek the next non-trivia token using a given lexical context.
    /// This does not mutate the original lexer state.
    #[must_use]
    pub fn peek_non_trivia_with_context(
        &self,
        context: LexContext,
    ) -> Option<(SyntaxKind, &'a str)> {
        let mut cloned = self.clone();
        // Iterate tokens using the internal single-step with override until non-trivia
        loop {
            match cloned.next_token_internal(Some(context)) {
                Some((k, t)) if k.is_trivia() => {
                    // continue skipping trivia
                    let _ = t; // avoid unused
                }
                Some((k, t)) => return Some((k, t)),
                None => return None,
            }
        }
    }

    /// Convenience: default context is Value
    pub fn next_token_default(&mut self) -> Option<(SyntaxKind, &'a str)> {
        self.next_token_with_context(Default::default())
    }
}

#[cfg(test)]
mod tests;
