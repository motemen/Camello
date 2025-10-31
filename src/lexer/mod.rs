use crate::{SyntaxKind, T};
use logos::Logos;
use std::cell::RefCell;
use std::collections::VecDeque;

mod contextual;
mod lookahead;
mod pod;
mod quote;
mod token;
mod types;

pub use token::Token;
pub use types::{DelimiterPhase, DelimiterType, LexContext, QuoteLikeMode, QuoteLikeState};

use lookahead::CachedEntry;
use types::{HeredocMarker, LexerMode};

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

    #[test]
    fn peek_token_caches_result() {
        let mut lexer = Lexer::new("$foo + 1");
        assert_eq!(lexer.peek_token(), Some((SyntaxKind::SCALAR_SIGIL, "$")));
        assert_eq!(lexer.peek_token(), Some((SyntaxKind::SCALAR_SIGIL, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SCALAR_SIGIL, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "foo")));
    }

    #[test]
    fn peek_nth_non_trivia_with_context_is_stable() {
        let lexer_src = "$foo   + $bar";
        let mut lexer = Lexer::new(lexer_src);

        assert_eq!(
            lexer.peek_nth_non_trivia_with_context(LexContext::Value, 0),
            Some((SyntaxKind::SCALAR_SIGIL, "$"))
        );
        assert_eq!(
            lexer.peek_nth_non_trivia_with_context(LexContext::Value, 1),
            Some((SyntaxKind::IDENT, "foo"))
        );
        assert_eq!(
            lexer.peek_nth_non_trivia_with_context(LexContext::Value, 2),
            Some((T![+], "+"))
        );
        // Repeating lookahead should produce consistent results
        assert_eq!(
            lexer.peek_nth_non_trivia_with_context(LexContext::Value, 1),
            Some((SyntaxKind::IDENT, "foo"))
        );

        // Consuming tokens afterwards should follow the peeked sequence
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SCALAR_SIGIL, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "foo")));
    }

    #[test]
    fn peek_nth_non_trivia_handles_quote_like_hash_delimiter() {
        let mut lexer = Lexer::new("qq#foo#");

        assert_eq!(
            lexer.peek_nth_non_trivia_with_context(LexContext::Value, 0),
            Some((T![qq], "qq"))
        );
        assert_eq!(
            lexer.peek_nth_non_trivia_with_context(LexContext::Value, 1),
            Some((SyntaxKind::DELIMITER, "#"))
        );
        assert_eq!(
            lexer.peek_nth_non_trivia_with_context(LexContext::Value, 2),
            Some((SyntaxKind::INTERPOLATED_STRING, "foo"))
        );
        assert_eq!(
            lexer.peek_nth_non_trivia_with_context(LexContext::Value, 3),
            Some((SyntaxKind::DELIMITER, "#"))
        );

        // The actual token stream should match the peeked sequence once quote-like mode begins
        assert_eq!(
            lexer.next_token_with_context(LexContext::Value),
            Some((T![qq], "qq"))
        );
        lexer.begin_quote_like(T![qq], QuoteLikeMode::Q);
        assert_eq!(
            lexer.next_token_with_context(LexContext::Value),
            Some((SyntaxKind::DELIMITER, "#"))
        );
        assert_eq!(
            lexer.next_token_with_context(LexContext::Value),
            Some((SyntaxKind::INTERPOLATED_STRING, "foo"))
        );
        assert_eq!(
            lexer.next_token_with_context(LexContext::Value),
            Some((SyntaxKind::DELIMITER, "#"))
        );
    }

    #[test]
    fn peek_context_switch_clears_cached_tokens() {
        let mut lexer = Lexer::new("/foo/");
        assert_eq!(
            lexer
                .peek_non_trivia_with_context(LexContext::Operator)
                .map(|(kind, _)| kind),
            Some(T![/])
        );

        // Switching to value context should allow regex literal handling
        assert_eq!(
            lexer
                .peek_non_trivia_with_context(LexContext::Value)
                .map(|(kind, _)| kind),
            Some(SyntaxKind::REGEX_LITERAL)
        );
        assert_eq!(
            lexer
                .next_token_with_context(LexContext::Value)
                .map(|(kind, _)| kind),
            Some(SyntaxKind::REGEX_LITERAL)
        );
    }

    #[test]
    fn peek_token_and_next_char_handles_hash_delimiter() {
        let lexer_src = "qq#foo#";
        let lexer = Lexer::new(lexer_src);

        let (first_kind, first_char) = lexer.peek_token_and_next_char();
        assert_eq!(first_kind, Some(T![qq]));
        assert_eq!(first_char, Some('#'));

        // Subsequent peeks should return the same values without consuming
        let (second_kind, second_char) = lexer.peek_token_and_next_char();
        assert_eq!(second_kind, Some(T![qq]));
        assert_eq!(second_char, Some('#'));
    }

    #[test]
    fn quote_like_peek_and_consume_flow() {
        let mut lexer = Lexer::new("qq#foo#");

        // Initial peek should see the keyword without consuming it
        assert_eq!(lexer.peek_token(), Some((T![qq], "qq")));

        let (kw_kind, _) = lexer
            .next_token_with_context(LexContext::Value)
            .expect("expected quote-like keyword");
        assert_eq!(kw_kind, T![qq]);

        lexer.begin_quote_like(kw_kind, QuoteLikeMode::Q);

        // After entering quote-like mode, the cached lookahead should expose the delimiter
        assert_eq!(lexer.peek_token(), Some((SyntaxKind::DELIMITER, "#")));
    }

    #[test]
    fn heredoc_marker_variants_are_parsed_correctly() {
        let cases = [
            ("<<'EOF'\n", "EOF"),
            ("<<\"EOF\"\n", "EOF"),
            ("<<`EOF`\n", "EOF"),
            ("<<\\EOF\n", "EOF"),
            ("<<\"\"\n", ""),
        ];

        for (source, expected_marker) in cases {
            let mut lexer = Lexer::new(source);
            let (kind, _) = lexer
                .next_token_with_context(LexContext::Value)
                .expect("expected heredoc start");
            assert_eq!(kind, SyntaxKind::HEREDOC_START);

            let marker = lexer
                .heredoc_queue
                .back()
                .expect("heredoc marker should be queued");
            assert_eq!(marker.marker, expected_marker);
        }
    }

    #[test]
    fn heredoc_marker_handles_escaped_quotes() {
        let mut lexer = Lexer::new("<<\"foo\\\"bar\"\n");
        let (kind, _) = lexer
            .next_token_with_context(LexContext::Value)
            .expect("expected heredoc start");
        assert_eq!(kind, SyntaxKind::HEREDOC_START);

        let marker = lexer
            .heredoc_queue
            .back()
            .expect("heredoc marker should be queued");
        assert_eq!(marker.marker, "foo\\\"bar");
    }

    #[test]
    fn iter_non_trivia_from_basic_iteration() {
        let lexer = Lexer::new("$a + $b * $c");

        let iter = lexer
            .iter_non_trivia_from(LexContext::Value, 0)
            .expect("Should return iterator");
        let tokens: Vec<_> = iter.take(5).collect();

        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0].0, SyntaxKind::SCALAR_SIGIL);
        assert_eq!(tokens[1].0, SyntaxKind::IDENT);
        assert_eq!(tokens[2].0, T![+]);
        assert_eq!(tokens[3].0, SyntaxKind::SCALAR_SIGIL);
        assert_eq!(tokens[4].0, SyntaxKind::IDENT);

        let iter2 = lexer
            .iter_non_trivia_from(LexContext::Value, 2)
            .expect("Should return iterator");
        let tokens2: Vec<_> = iter2.take(3).collect();

        assert_eq!(tokens2.len(), 3);
        assert_eq!(tokens2[0].0, T![+]);
        assert_eq!(tokens2[1].0, SyntaxKind::SCALAR_SIGIL);
        assert_eq!(tokens2[2].0, SyntaxKind::IDENT);
    }

    #[test]
    fn iter_non_trivia_from_with_braces() {
        let lexer = Lexer::new("{ a => 1; b => 2 }");

        let iter = lexer
            .iter_non_trivia_from(LexContext::Value, 1)
            .expect("Should return iterator");

        let mut found_semicolon = false;
        let mut brace_depth = 0;

        for (kind, _) in iter {
            match kind {
                T!['{'] => brace_depth += 1,
                T!['}'] => {
                    if brace_depth == 0 {
                        break;
                    }
                    brace_depth -= 1;
                }
                T![;] if brace_depth == 0 => {
                    found_semicolon = true;
                    break;
                }
                _ => {}
            }
        }

        assert!(found_semicolon, "Should find semicolon at top level");
    }

    #[test]
    fn iter_non_trivia_from_skips_trivia() {
        let lexer = Lexer::new("$a   # comment\n  + $b");

        let iter = lexer
            .iter_non_trivia_from(LexContext::Value, 0)
            .expect("Should return iterator");
        let tokens: Vec<_> = iter.take(4).collect();

        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].0, SyntaxKind::SCALAR_SIGIL);
        assert_eq!(tokens[1].0, SyntaxKind::IDENT);
        assert_eq!(tokens[2].0, T![+]);
        assert_eq!(tokens[3].0, SyntaxKind::SCALAR_SIGIL);
    }

    #[test]
    fn iter_non_trivia_from_beyond_end() {
        let lexer = Lexer::new("$a + $b");

        let iter = lexer.iter_non_trivia_from(LexContext::Value, 100);
        assert!(iter.is_none(), "Should return None for offset beyond end");
    }
}

pub struct Lexer<'a> {
    pub(super) logos_lexer: logos::Lexer<'a, Token>,
    pub(super) at_line_start: bool, // Track if we're at the start of a line for POD detection
    pub(super) mode: LexerMode,
    // Pending tokens produced by stateless expansions (e.g., quote-like operators)
    pub(super) pending: VecDeque<(SyntaxKind, &'a str)>,
    pub(super) heredoc_queue: VecDeque<HeredocMarker<'a>>,
    lookahead: RefCell<VecDeque<CachedEntry<'a>>>,
}

impl Clone for Lexer<'_> {
    fn clone(&self) -> Self {
        Self {
            logos_lexer: self.logos_lexer.clone(),
            at_line_start: self.at_line_start,
            mode: self.mode,
            pending: self.pending.clone(),
            heredoc_queue: self.heredoc_queue.clone(),
            lookahead: RefCell::new(self.lookahead.borrow().clone()),
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
            lookahead: RefCell::new(VecDeque::new()),
        }
    }

    #[must_use]
    pub fn has_pending_heredoc(&self) -> bool {
        !self.heredoc_queue.is_empty()
    }

    /// Consume exactly one character from the underlying stream and return it as an IDENT token.
    /// This is used by the parser to accept punctuation-named special variables like $", $', $`, etc.
    pub fn consume_one_char_as_ident(&mut self) -> Option<(SyntaxKind, &'a str)> {
        self.clear_lookahead();
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
        self.clear_lookahead();
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
    pub(super) fn next_token_internal(
        &mut self,
        context: Option<LexContext>,
    ) -> Option<(SyntaxKind, &'a str)> {
        if let Some(ctx) = context {
            if let Some(token) = self.consume_cached(ctx) {
                return Some(token);
            }
        } else {
            self.lookahead.borrow_mut().clear();
        }

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
        self.clear_lookahead();
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
        self.clear_lookahead();
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

    /// Get the next token using an explicit lexical context for ambiguous cases.
    /// For non-default contexts (QuoteLike), this context hint is ignored.
    pub fn next_token_with_context(
        &mut self,
        context: LexContext,
    ) -> Option<(SyntaxKind, &'a str)> {
        self.next_token_internal(Some(context))
    }
}
