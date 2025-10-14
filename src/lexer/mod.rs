use crate::{SyntaxKind, T};
use logos::Logos;
use std::collections::VecDeque;

mod contextual;
mod pod;
mod quote;

#[derive(Debug, Clone)]
pub(super) struct HeredocMarker<'a> {
    marker: &'a str,
    strip_indent: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peek_non_trivia_token_skips_trivia() {
        let mut lexer = Lexer::new("$var\n@array");
        assert_eq!(
            lexer.peek_non_trivia_token(),
            Some((SyntaxKind::SCALAR_SIGIL, "$"))
        );
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SCALAR_SIGIL, "$")));
        assert_eq!(
            lexer.peek_non_trivia_token(),
            Some((SyntaxKind::IDENT, "var"))
        );
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "var")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::NEWLINE, "\n")));
        assert_eq!(
            lexer.peek_non_trivia_token(),
            Some((SyntaxKind::ARRAY_SIGIL, "@"))
        );
    }

    #[test]
    fn array_index_variable_allows_quote_keywords_as_names() {
        let mut lexer = Lexer::new("$#q");

        assert_eq!(
            lexer.next_token_with_context(LexContext::Value),
            Some((SyntaxKind::ARRAY_INDEX_SIGIL, "$#"))
        );
        assert_eq!(
            lexer.next_token_with_context(LexContext::Value),
            Some((SyntaxKind::IDENT, "q"))
        );
    }
}

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    // Sigils（変数の型を示すプレフィックス）
    #[token("$#")]
    DollarHash,

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

    // Keywords (must come before Ident to take precedence)
    #[token("require")]
    RequireKw,

    #[token("BEGIN")]
    BeginKw,

    #[token("END")]
    EndBlockKw,

    #[token("INIT")]
    InitKw,

    #[token("CHECK")]
    CheckKw,

    #[token("UNITCHECK")]
    UnitcheckKw,

    // POD commands (must be at line start)
    // Note: POD detection is handled manually in lexer due to line-start requirement
    PodCommand,

    // 識別子（サブルーチン名など）
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,

    // リテラル
    // Numeric literals with underscores and multiple bases
    // Hex float (e.g., 0x1.999ap-4), must have '.' and 'p' exponent
    #[regex(
        r"0x[0-9A-Fa-f](?:_*[0-9A-Fa-f])*\.[0-9A-Fa-f](?:_*[0-9A-Fa-f])*[pP][+-]?[0-9](?:_*[0-9])*",
        priority = 3
    )]
    // Hex, binary, and octal integers
    #[regex(r"0x[0-9A-Fa-f](?:_*[0-9A-Fa-f])*")]
    #[regex(r"0b[01](?:_*[01])*")]
    #[regex(r"0o[0-7](?:_*[0-7])*")]
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
    // Backtick command substitution (handled manually like RegexLiteral due to complexity)
    BacktickString,

    // Version literal (v1.23, v5.008_001, etc.) - requires at least one dot component
    #[regex(r"v[0-9]+(\.[0-9_]+)+")]
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

    #[token("~~")]
    SmartMatch,

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

    // 改行（重要なので個別にトークン化）
    #[regex(r"\r\n|\r|\n")]
    Newline,

    // コメント
    #[regex(r"#[^\r\n]*(?:\r\n|\r|\n)?")]
    Comment,

    // データセクション（__END__ / __DATA__ 以降のすべてのテキスト）
    DataSection,

    // Postfix dereference operators (handled manually due to context sensitivity)
    PostfixDerefArray,          // ->@*
    PostfixDerefHash,           // ->%*
    PostfixDerefScalar,         // ->$*
    PostfixDerefArrayLastIndex, // ->$#*
    PostfixDerefCode,           // ->&*
    PostfixDerefGlob,           // ->**
}

impl Token {
    #[must_use]
    pub fn to_syntax_kind(&self) -> SyntaxKind {
        match self {
            Token::DollarHash => SyntaxKind::ARRAY_INDEX_SIGIL,
            Token::Dollar => SyntaxKind::SCALAR_SIGIL,
            Token::At => SyntaxKind::ARRAY_SIGIL,
            Token::Backslash => T!['\\'],
            Token::Ampersand => SyntaxKind::CODE_SIGIL, // Will be disambiguated
            Token::RequireKw => T![require],
            Token::BeginKw => T![BEGIN],
            Token::EndBlockKw => T![END],
            Token::InitKw => T![INIT],
            Token::CheckKw => T![CHECK],
            Token::UnitcheckKw => T![UNITCHECK],
            Token::Ident => SyntaxKind::IDENT,
            Token::Number => SyntaxKind::NUMBER,
            Token::String => SyntaxKind::STRING,
            Token::BacktickString => SyntaxKind::BACKTICK_STRING,
            Token::Version => SyntaxKind::VERSION,
            Token::BareVersion => SyntaxKind::BARE_VERSION,
            Token::RegexLiteral => SyntaxKind::REGEX_LITERAL,
            Token::EndKw => T![__END__],
            Token::DataKw => T![__DATA__],
            Token::PodCommand => SyntaxKind::POD_CONTENT, // Not used anymore
            Token::LBrace => T!['{'],
            Token::RBrace => T!['}'],
            Token::LParen => T!['('],
            Token::RParen => T![')'],
            Token::LBracket => T!['['],
            Token::RBracket => T![']'],
            Token::Semicolon => T![;],
            Token::Comma => T![,],
            Token::DoubleColon => T![::],
            Token::QuestionMark => T![?],
            Token::Colon => T![:],
            Token::Eq => T![=],
            Token::Increment => T![++],
            Token::Decrement => T![--],
            Token::Plus => T![+],
            Token::Minus => T![-],
            Token::DotDotDot => T![...],
            Token::DotDot => T![..],
            Token::Dot => T![.],
            Token::Arrow => T![->],
            Token::FatComma => T![=>],
            Token::Exponent => T![**],
            Token::Star => T![*],
            Token::Slash => T![/],
            Token::Percent => T![%],
            Token::Greater => T![>],
            Token::Less => T![<],
            Token::ShiftLeft => T![<<],
            Token::ShiftRight => T![>>],
            Token::Caret => SyntaxKind::CARET, // Will be disambiguated
            Token::Pipe => T![|],
            Token::GreaterEqual => T![>=],
            Token::LessEqual => T![<=],
            Token::EqualEqual => T![==],
            Token::NotEqual => T![!=],
            Token::RegexMatch => T![=~],
            Token::SmartMatch => T![~~],
            Token::RegexNotMatch => T![!~],
            Token::LogicalAnd => T![&&],
            Token::LogicalOr => T![||],
            Token::LogicalNot => T![!],
            Token::Tilde => T![~],
            Token::DefinedOr => T!["//"],
            Token::Spaceship => T![<=>],
            Token::Whitespace => SyntaxKind::WHITESPACE,
            Token::Newline => SyntaxKind::NEWLINE,
            Token::Comment => SyntaxKind::COMMENT,
            Token::DataSection => SyntaxKind::DATA_SECTION,
            Token::PostfixDerefArray => SyntaxKind::POSTFIX_DEREF_ARRAY,
            Token::PostfixDerefHash => SyntaxKind::POSTFIX_DEREF_HASH,
            Token::PostfixDerefScalar => SyntaxKind::POSTFIX_DEREF_SCALAR,
            Token::PostfixDerefArrayLastIndex => SyntaxKind::POSTFIX_DEREF_ARRAY_LAST_INDEX,
            Token::PostfixDerefCode => SyntaxKind::POSTFIX_DEREF_CODE,
            Token::PostfixDerefGlob => SyntaxKind::POSTFIX_DEREF_GLOB,
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
    /// Ambiguous value lookahead used by the parser when probing for possible arguments.
    ///
    /// This behaves like `Value` for most tokens but avoids implicit regex/io/filetest
    /// handling and performs additional disambiguation for sigils so the parser can
    /// inspect upcoming tokens without forcing value-context side effects.
    AmbiguousValueLookahead,
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
impl QuoteLikeMode {
    /// Get QuoteLikeMode from keyword SyntaxKind
    pub fn from_keyword(kind: SyntaxKind) -> Self {
        match kind {
            T![q] | T![qq] | T![qx] => Self::Q,
            T![qw] => Self::QW,
            T![m] => Self::M,
            T![qr] => Self::QR,
            T![s] => Self::S,
            T![tr] | T![y] => Self::TR,
            _ => panic!("Invalid quote-like keyword: {:?}", kind),
        }
    }
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
pub(super) enum LexerMode {
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
    pub(super) logos_lexer: logos::Lexer<'a, Token>,
    pub(super) at_line_start: bool, // Track if we're at the start of a line for POD detection
    pub(super) mode: LexerMode,
    // Pending tokens produced by stateless expansions (e.g., quote-like operators)
    pub(super) pending: VecDeque<(SyntaxKind, &'a str)>,
    pub(super) heredoc_queue: VecDeque<HeredocMarker<'a>>,
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
        self.next_token()
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

    #[must_use]
    pub fn has_pending_heredoc(&self) -> bool {
        !self.heredoc_queue.is_empty()
    }

    /// Consume exactly one character from the underlying stream and return it as an IDENT token.
    /// This is used by the parser to accept punctuation-named special variables like $", $', $`, etc.
    pub fn consume_one_char_as_ident(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None;
        }
        let ch = remainder.chars().next()?;
        let len = ch.len_utf8();
        let text = &remainder[..len];
        self.logos_lexer.bump(len);
        Some((SyntaxKind::IDENT, text))
    }

    /// Consume a digit-prefixed identifier (e.g., "123ABC", "456") from the stream and return it as an IDENT token.
    /// This is used by the parser for package names like Foo::123ABC after :: separators.
    pub fn consume_digit_prefixed_ident(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None;
        }

        // Must start with a digit
        let mut chars = remainder.char_indices();
        let (_, first_char) = chars.next()?;
        if !first_char.is_ascii_digit() {
            return None;
        }

        // Find the end of the identifier (digits and letters, similar to normal identifiers)
        let mut end_pos = first_char.len_utf8();
        for (pos, ch) in chars {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                end_pos = pos + ch.len_utf8();
            } else {
                break;
            }
        }

        let text = &remainder[..end_pos];
        self.logos_lexer.bump(end_pos);
        Some((SyntaxKind::IDENT, text))
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

        if !self.heredoc_queue.is_empty() && self.at_line_start {
            if let Some((k, t)) = self.bump_until_marker() {
                self.update_line_position(t);
                return Some((k, t));
            }
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

    /// Handle default context (Value | Operator): 通常ケースを担当
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

        // Handle special tokens when in Value context
        let allow_value_specific_handling = matches!(context, Some(LexContext::Value));
        let in_quote_like = matches!(self.mode, LexerMode::QuoteLike { .. });
        if allow_value_specific_handling && !in_quote_like {
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
                let mut syntax_kind = {
                    // If previous token was a sigil, force IDENT for following identifier
                    if let Some(ctx) = context {
                        self.disambiguate(&token, text, ctx)
                    } else {
                        token.to_syntax_kind()
                    }
                };

                let mut adjusted_text = text;

                if matches!(token, Token::Number) {
                    if let Some(stripped) = text.strip_suffix('.') {
                        if !stripped.is_empty()
                            && (stripped.starts_with("0x")
                                || stripped.starts_with("0b")
                                || stripped.starts_with("0o"))
                        {
                            let span = self.logos_lexer.span();
                            let source = self.logos_lexer.source();

                            let mut extra_dot_chars = 0usize;
                            let mut bump_bytes = 0usize;
                            for ch in self.logos_lexer.remainder().chars().take(2) {
                                if ch == '.' {
                                    extra_dot_chars += 1;
                                    bump_bytes += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }

                            let total_dots = 1 + extra_dot_chars;
                            let pending_token = match total_dots {
                                3 => Some((T![...], bump_bytes)),
                                2 => Some((T![..], bump_bytes)),
                                1 => Some((T![.], 0)),
                                _ => None,
                            };

                            if let Some((pending_kind, bump_len)) = pending_token {
                                adjusted_text = stripped;

                                if bump_len > 0 {
                                    self.logos_lexer.bump(bump_len);
                                }

                                let op_start = span.end - 1;
                                let op_byte_len = 1 + bump_len;
                                let op_end = op_start + op_byte_len;
                                let op_text = &source[op_start..op_end];
                                self.pending.push_back((pending_kind, op_text));
                            }
                        }
                    }
                }

                // Handle x followed by number literal (e.g., x5, x0xFF in "abc"x5, "abc"x0xFF)
                // In operator context, split into 'x' operator and number
                // In value context, keep as identifier (e.g., sub x100, package x1)
                if matches!(token, Token::Ident)
                    && text.starts_with('x')
                    && text.len() > 1
                    && matches!(
                        context,
                        Some(LexContext::Operator | LexContext::AmbiguousValueLookahead)
                    )
                {
                    // Validate if the text after 'x' is a valid number literal
                    // by using logos lexer directly on the substring
                    let remaining = &text[1..];
                    let mut logos_lexer = Token::lexer(remaining);
                    let is_valid_number = if let Some(Ok(Token::Number)) = logos_lexer.next() {
                        // Ensure the number token consumes the entire remaining string.
                        logos_lexer.span().end == remaining.len()
                    } else {
                        false
                    };

                    if is_valid_number {
                        // Split: return 'x' now, push the rest (number literal) to pending queue
                        syntax_kind = T![x];
                        adjusted_text = &text[..1]; // Just 'x'
                                                    // Push the remaining number literal to pending queue as a NUMBER token
                        self.pending.push_back((SyntaxKind::NUMBER, remaining));
                    }
                }

                // Quote-like auto-expansion disabled. Parser triggers begin_quote_like().

                // Special handling for __END__ and __DATA__: consume everything remaining as data section
                if matches!(syntax_kind, T![__END__] | T![__DATA__]) {
                    return Some((syntax_kind, adjusted_text));
                }

                // Track line position for POD detection
                self.update_line_position(adjusted_text);
                Some((syntax_kind, adjusted_text))
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

    /// Consume tokens until a closing parenthesis is found at depth 0.
    /// Returns the text of all consumed tokens as a RAW_STRING, excluding the closing paren.
    /// Used for attribute arguments where only parenthesis balance is checked.
    pub fn consume_balanced_parens(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let start_pos = self.logos_lexer.span().end;
        let source = self.logos_lexer.source();
        let mut paren_depth = 0;
        let mut end_pos = start_pos;

        // Manually scan for balanced parentheses
        let remainder = self.logos_lexer.remainder();
        for ch in remainder.chars() {
            if ch == ')' && paren_depth == 0 {
                break;
            }
            if ch == '(' {
                paren_depth += 1;
            } else if ch == ')' {
                paren_depth -= 1;
            }
            end_pos += ch.len_utf8();
        }

        let content_len = end_pos - start_pos;
        if content_len == 0 {
            return None;
        }

        let content = &source[start_pos..end_pos];
        self.logos_lexer.bump(content_len);

        Some((SyntaxKind::RAW_STRING, content))
    }

    fn disambiguate(&self, token: &Token, text: &str, ctx: LexContext) -> SyntaxKind {
        match token {
            // Identifier words: operators, quote-like starters, or keywords
            Token::Ident => match text {
                // Word operators in operator context; otherwise identifiers
                "eq" | "ne" | "gt" | "lt" | "ge" | "le" | "cmp" => match ctx {
                    LexContext::Operator | LexContext::AmbiguousValueLookahead => match text {
                        "eq" => T![eq],
                        "ne" => T![ne],
                        "gt" => T![gt],
                        "lt" => T![lt],
                        "ge" => T![ge],
                        "le" => T![le],
                        "cmp" => T![cmp],
                        _ => unreachable!(),
                    },
                    LexContext::Value => SyntaxKind::IDENT,
                },
                // Repetition operator vs identifier
                "x" => match ctx {
                    LexContext::Operator | LexContext::AmbiguousValueLookahead => T![x],
                    LexContext::Value => SyntaxKind::IDENT,
                },
                _ => {
                    // Check if this might be a keyword followed by ::
                    // If so, treat it as an identifier to allow qualified names like local::lib
                    if let Some(keyword_kind) = Self::map_ident_keyword(text) {
                        // Look ahead to see if :: follows
                        let remainder = self.logos_lexer.remainder();
                        if remainder.starts_with("::") || self.ident_follows_sigil() {
                            // This is part of a qualified identifier like local::lib
                            SyntaxKind::IDENT
                        } else {
                            // This is a standalone keyword
                            keyword_kind
                        }
                    } else {
                        SyntaxKind::IDENT
                    }
                }
            },
            // Ambiguous symbol tokens depending on context
            Token::Percent => match ctx {
                LexContext::Value => SyntaxKind::HASH_SIGIL,
                LexContext::Operator => T![%],
                LexContext::AmbiguousValueLookahead => {
                    if self.ambiguous_remainder_starts_sigil_target() {
                        SyntaxKind::HASH_SIGIL
                    } else {
                        T![%]
                    }
                }
            },
            Token::Star => match ctx {
                LexContext::Value => SyntaxKind::TYPEGLOB_SIGIL,
                LexContext::Operator => T![*],
                LexContext::AmbiguousValueLookahead => {
                    if self.ambiguous_remainder_starts_sigil_target() {
                        SyntaxKind::TYPEGLOB_SIGIL
                    } else {
                        T![*]
                    }
                }
            },
            Token::DotDotDot => match ctx {
                LexContext::Operator | LexContext::AmbiguousValueLookahead => {
                    T![...]
                }
                LexContext::Value => SyntaxKind::ELLIPSIS,
            },
            Token::Slash => T![/], // regex literals handled elsewhere
            Token::Ampersand => match ctx {
                LexContext::Value => SyntaxKind::CODE_SIGIL,
                LexContext::Operator => T![&],
                LexContext::AmbiguousValueLookahead => {
                    if self.ambiguous_remainder_starts_sigil_target() {
                        SyntaxKind::CODE_SIGIL
                    } else {
                        T![&]
                    }
                }
            },
            Token::Caret => match ctx {
                LexContext::Value => SyntaxKind::CARET,
                LexContext::Operator | LexContext::AmbiguousValueLookahead => {
                    T![^]
                }
            },
            Token::Pipe => T![|],
            // Everything else: direct mapping
            _ => token.to_syntax_kind(),
        }
    }

    /// While the parser runs [`LexContext::AmbiguousValueLookahead`] we need to decide if a
    /// `%`, `&`, or `*` token should keep behaving like a sigil or fall back to an infix
    /// operator. The goal is to keep arguments like `%hash`, `&func`, or `%{...}` available to
    /// function-call lookahead without accidentally reinterpreting infix operators such as
    /// `foo % +1`, `foo * { ... }`, or `foo*@_` as sigils.
    ///
    /// We therefore only recognize a sigil when the next non-trivia character is either an
    /// opening brace (for typeglob/hash dereferences) or an identifier start, provided the sigil
    /// isn't glued directly to a preceding identifier. This keeps `%hash`, `&foo`, or `*STDOUT`
    /// available to the parser's lookahead while whitespace-delimited operators like `foo % +1`,
    /// `foo * { ... }`, or `foo*@_` continue to lex as infix.
    fn ambiguous_remainder_starts_sigil_target(&self) -> bool {
        let Some(next) = self.logos_lexer.remainder().chars().next() else {
            return false;
        };

        if next.is_whitespace() {
            return false;
        }

        let span = self.logos_lexer.span();
        if span.start > 0 {
            let source = self.logos_lexer.source();
            if source[..span.start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            {
                return false;
            }
        }

        matches!(next, '{') || next.is_ascii_alphanumeric() || next == '_'
    }

    fn ident_follows_sigil(&self) -> bool {
        let span = self.logos_lexer.span();
        if span.start == 0 {
            return false;
        }

        let source = self.logos_lexer.source();
        let mut chars = source[..span.start].chars().rev();
        match chars.next() {
            Some('#') => chars.next().is_some_and(|ch| matches!(ch, '$')),
            Some('$' | '@' | '%' | '*' | '&') => true,
            _ => false,
        }
    }

    /// Map known identifier keywords and quote-like starters
    fn map_ident_keyword(text: &str) -> Option<SyntaxKind> {
        Some(match text {
            // Control/decl keywords
            "sub" => T![sub],
            "my" => T![my],
            "our" => T![our],
            "state" => T![state],
            "local" => T![local],
            "if" => T![if],
            "unless" => T![unless],
            "elsif" => T![elsif],
            "else" => T![else],
            "for" => T![for],
            "foreach" => T![foreach],
            "while" => T![while],
            "until" => T![until],
            "package" => T![package],
            "use" => T![use],
            "no" => T![no],
            "require" => T![require],
            "return" => T![return],
            "undef" => T![undef],
            "next" => T![next],
            "last" => T![last],
            "redo" => T![redo],
            "try" => T![try],
            "catch" => T![catch],
            "finally" => T![finally],
            // Quote-like starters (treated as keywords regardless of context)
            "q" => T![q],
            "qq" => T![qq],
            "qr" => T![qr],
            "qx" => T![qx],
            "qw" => T![qw],
            "m" => T![m],
            "s" => T![s],
            "tr" => T![tr],
            "y" => T![y],
            // Logical word operators as keywords
            "not" => T![not],
            "and" => T![and],
            "or" => T![or],
            "xor" => T![xor],
            _ => return None,
        })
    }

    /// Track line position for POD detection
    pub(super) fn update_line_position(&mut self, text: &str) {
        // Check if this token contains a newline
        if text.contains('\n') {
            self.at_line_start = true;
        } else if text.chars().any(|c| !c.is_whitespace()) {
            // Non-whitespace content means we're no longer at line start
            self.at_line_start = false;
        }
    }

    #[must_use]
    pub fn span(&self) -> std::ops::Range<usize> {
        self.logos_lexer.span()
    }

    /// Peek at the next token without consuming it or changing lexer state
    #[must_use]
    pub fn peek_token(&self) -> Option<(SyntaxKind, &'a str)> {
        let mut cloned = self.clone();
        cloned.next_token()
    }

    /// Peek at the next non-trivia token without consuming it or changing lexer state
    #[must_use]
    pub fn peek_non_trivia_token(&self) -> Option<(SyntaxKind, &'a str)> {
        self.peek_non_trivia_with_context(Default::default())
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

    /// Peek the nth non-trivia token using a given lexical context.
    /// This does not mutate the original lexer state.
    #[must_use]
    pub fn peek_nth_non_trivia_with_context(
        &self,
        context: LexContext,
        n: usize,
    ) -> Option<(SyntaxKind, &'a str)> {
        let mut cloned = self.clone();
        let mut count = 0;
        loop {
            match cloned.next_token_internal(Some(context)) {
                Some((k, _)) if k.is_trivia() => {
                    // skip trivia
                }
                Some((k, t)) => {
                    if count == n {
                        return Some((k, t));
                    }
                    count += 1;
                }
                None => return None,
            }
        }
    }

    /// Peek the current token and the character immediately following it.
    /// This is useful for disambiguating cases like quote-like keywords followed by delimiters.
    /// Returns (current_token_kind, next_char) where next_char is the character immediately
    /// after the current token, or None if at end of input.
    #[must_use]
    pub fn peek_token_and_next_char(&self) -> (Option<SyntaxKind>, Option<char>) {
        let mut cloned = self.clone();
        let current_token = cloned.next_token().map(|(kind, _)| kind);
        let next_char = cloned.logos_lexer.remainder().chars().next();
        (current_token, next_char)
    }

    /// Peek the next non-trivia token using a given lexical context.
    /// This does not mutate the original lexer state.
    #[must_use]
    pub fn peek_non_trivia_with_context(
        &self,
        context: LexContext,
    ) -> Option<(SyntaxKind, &'a str)> {
        self.peek_nth_non_trivia_with_context(context, 0)
    }
}
