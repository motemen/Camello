use super::{
    DelimiterPhase, DelimiterType, Lexer, LexerContext, QuoteLikeMode, QuoteLikeState, Token,
};
use crate::SyntaxKind;
use logos::Logos;

impl<'a> Lexer<'a> {
    /// Handle quote-like delimiter recognition and consumption
    pub(super) fn try_handle_quote_like_delimiter_internal(
        &mut self,
    ) -> Option<(SyntaxKind, &'a str)> {
        let LexerContext::QuoteLike {
            state, delimiter, ..
        } = &self.context
        else {
            return None;
        };

        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None;
        }

        let first_char = remainder.chars().next().unwrap();
        let should_consume = match state {
            QuoteLikeState::Delimiter {
                kind: DelimiterType::Open,
                ..
            } => {
                // Any valid quote delimiter can start
                self.is_quote_delimiter(first_char)
            }
            QuoteLikeState::Delimiter {
                kind: DelimiterType::Close,
                ..
            } => {
                // Must match the expected closing delimiter
                let expected_closing = Self::get_closing_delimiter(*delimiter);
                first_char.to_string() == expected_closing
            }
            _ => false,
        };

        if should_consume {
            let delim_str = &remainder[..first_char.len_utf8()];
            self.logos_lexer.bump(first_char.len_utf8());
            self.handle_quote_like_delimiter(delim_str);
            Some((SyntaxKind::DELIMITER, delim_str))
        } else {
            None
        }
    }

    /// Handle quote-like tokens (both content and delimiters) based on current context and state
    pub(super) fn try_handle_quote_like_internal(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let LexerContext::QuoteLike {
            mode,
            state,
            delimiter,
            ..
        } = self.context
        else {
            return None;
        };

        match (mode, state) {
            // Content states - try content first, then fall back to delimiter
            (
                QuoteLikeMode::Q,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::First,
                },
            ) => self
                .consume_quote_content(
                    SyntaxKind::Q_STRING,
                    delimiter,
                    QuoteLikeState::Delimiter {
                        phase: DelimiterPhase::First,
                        kind: DelimiterType::Close,
                    },
                )
                .or_else(|| self.try_handle_quote_like_delimiter_internal()),

            (
                QuoteLikeMode::QW,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::First,
                },
            ) => {
                // Special handling for QW - try content first, then delimiter
                self.handle_qw_content(delimiter)
                    .or_else(|| self.try_handle_quote_like_delimiter_internal())
            }

            (
                QuoteLikeMode::M,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::First,
                },
            ) => self
                .consume_quote_content(
                    SyntaxKind::M_STRING,
                    delimiter,
                    QuoteLikeState::Delimiter {
                        phase: DelimiterPhase::First,
                        kind: DelimiterType::Close,
                    },
                )
                .or_else(|| self.try_handle_quote_like_delimiter_internal()),

            (
                QuoteLikeMode::S,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::First,
                },
            ) => self
                .consume_quote_content(
                    SyntaxKind::S_PATTERN,
                    delimiter,
                    QuoteLikeState::Delimiter {
                        phase: DelimiterPhase::Second,
                        kind: DelimiterType::Open,
                    },
                )
                .or_else(|| self.try_handle_quote_like_delimiter_internal()),

            (
                QuoteLikeMode::S,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::Second,
                },
            ) => self
                .consume_quote_content(
                    SyntaxKind::S_REPLACEMENT,
                    delimiter,
                    QuoteLikeState::Delimiter {
                        phase: DelimiterPhase::Second,
                        kind: DelimiterType::Close,
                    },
                )
                .or_else(|| self.try_handle_quote_like_delimiter_internal()),

            (
                QuoteLikeMode::TR,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::First,
                },
            ) => self
                .consume_quote_content(
                    SyntaxKind::TR_SEARCH_LIST,
                    delimiter,
                    QuoteLikeState::Delimiter {
                        phase: DelimiterPhase::Second,
                        kind: DelimiterType::Open,
                    },
                )
                .or_else(|| self.try_handle_quote_like_delimiter_internal()),

            (
                QuoteLikeMode::TR,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::Second,
                },
            ) => self
                .consume_quote_content(
                    SyntaxKind::TR_REPLACEMENT_LIST,
                    delimiter,
                    QuoteLikeState::Delimiter {
                        phase: DelimiterPhase::Second,
                        kind: DelimiterType::Close,
                    },
                )
                .or_else(|| self.try_handle_quote_like_delimiter_internal()),

            // Delimiter states - only handle delimiters
            (_, QuoteLikeState::Delimiter { .. }) => {
                self.try_handle_quote_like_delimiter_internal()
            }

            // Flag states
            (QuoteLikeMode::S | QuoteLikeMode::TR, QuoteLikeState::Flags) => {
                self.try_consume_quote_like_flags(&mode)
            }

            // Invalid state combinations that should never occur
            (
                QuoteLikeMode::Q | QuoteLikeMode::QW | QuoteLikeMode::M,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::Second,
                },
            )
            | (QuoteLikeMode::Q | QuoteLikeMode::QW | QuoteLikeMode::M, QuoteLikeState::Flags) => {
                unreachable!("Invalid state combination for single-delimiter quote-like operators: {:?}, {:?}", mode, state)
            }
        }
    }

    /// Helper method to consume quote-like content and transition state
    fn consume_quote_content(
        &mut self,
        content_kind: SyntaxKind,
        delimiter: char,
        next_state: QuoteLikeState,
    ) -> Option<(SyntaxKind, &'a str)> {
        let closing_delimiter = Self::get_closing_delimiter(delimiter);
        if let Some(content) =
            self.try_consume_quote_like_string_content(content_kind, &closing_delimiter)
        {
            // Update context to next state
            if let LexerContext::QuoteLike { prefix, mode, .. } = self.context {
                self.context = LexerContext::QuoteLike {
                    prefix,
                    mode,
                    state: next_state,
                    delimiter,
                };
            }
            Some(content)
        } else {
            None
        }
    }

    /// Handle QW (qw) content specifically
    fn handle_qw_content(&mut self, delimiter: char) -> Option<(SyntaxKind, &'a str)> {
        // Check if we hit closing delimiter first
        let remainder = self.logos_lexer.remainder();
        if !remainder.is_empty() {
            let first_char = remainder.chars().next().unwrap();
            let expected_closing_delimiter = match delimiter {
                '{' => '}',
                '[' => ']',
                '(' => ')',
                '<' => '>',
                other => other,
            };
            // If we hit closing delimiter, transition to FirstCloseDelimiter state
            if first_char == expected_closing_delimiter {
                if let LexerContext::QuoteLike { prefix, mode, .. } = self.context {
                    self.context = LexerContext::QuoteLike {
                        prefix,
                        mode,
                        state: QuoteLikeState::Delimiter {
                            phase: DelimiterPhase::First,
                            kind: DelimiterType::Close,
                        },
                        delimiter,
                    };
                }
                return None; // Let normal lexer handle the delimiter
            }
        }

        // Otherwise, try to consume QW content
        self.try_consume_qw_content()
    }

    /// Get the closing delimiter for the given opening delimiter
    fn get_closing_delimiter(opening: char) -> String {
        match opening {
            '{' => "}".to_string(),
            '[' => "]".to_string(),
            '(' => ")".to_string(),
            '<' => ">".to_string(),
            _ => opening.to_string(), // For symmetric delimiters, return the same
        }
    }

    fn try_consume_quote_like_string_content(
        &mut self,
        content_kind: SyntaxKind,
        delimiter: &str,
    ) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None;
        }

        // Find the next occurrence of the delimiter
        let mut end_pos = remainder.len(); // Default to end of input
        let mut escaped = false;
        let mut nest_level = 0; // Track nesting for symmetric delimiters like {}

        for (i, c) in remainder.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }

            if c == '\\' {
                escaped = true;
                continue;
            }

            // Check if we're at the start of the delimiter
            if remainder[i..].starts_with(delimiter) {
                if nest_level == 0 {
                    end_pos = i;
                    break;
                }
                nest_level -= 1;
            } else if self.is_nested_delimiter_pair(c, delimiter.chars().next().unwrap_or('\0')) {
                nest_level += 1;
            }
        }

        if end_pos < remainder.len() {
            let content = &remainder[..end_pos];
            self.logos_lexer.bump(end_pos);
            return Some((content_kind, content));
        }

        None
    }

    /// Check if a character opens a nested delimiter that matches the closing delimiter
    fn is_nested_delimiter_pair(&self, open_char: char, close_char: char) -> bool {
        match close_char {
            ')' => open_char == '(',
            '}' => open_char == '{',
            ']' => open_char == '[',
            '>' => open_char == '<',
            _ => false,
        }
    }

    /// Try to consume quote-like operator flags (s///flags, tr///flags)
    pub(super) fn try_consume_quote_like_flags(
        &mut self,
        mode: &QuoteLikeMode,
    ) -> Option<(SyntaxKind, &'a str)> {
        let start_pos = self.logos_lexer.span().end;
        let input = self.logos_lexer.source();
        let remaining = &input[start_pos..];

        // Define valid flag characters for each operator type
        let valid_flags = match mode {
            QuoteLikeMode::S => "msixpodualngcer",
            QuoteLikeMode::TR => "cdsr",
            _ => return None, // Other modes don't have flags
        };

        // Consume consecutive flag characters
        let mut flag_end = 0;
        let mut has_valid_flags = false;
        let mut has_invalid_flags = false;

        for ch in remaining.chars() {
            if ch.is_alphabetic() {
                if valid_flags.contains(ch) {
                    has_valid_flags = true;
                    flag_end += ch.len_utf8();
                } else {
                    // Found invalid flag character
                    has_invalid_flags = true;
                    flag_end += ch.len_utf8();
                }
            } else {
                // Non-alphabetic character - stop parsing flags
                break;
            }
        }

        if flag_end > 0 {
            if has_invalid_flags {
                // If there are any invalid flags, treat the entire flag sequence as an error
                let flags_slice = &input[start_pos..start_pos + flag_end];

                // Create a new lexer starting after the consumed flags
                let new_start = start_pos + flag_end;
                let remaining_input = &input[new_start..];
                self.logos_lexer = Token::lexer(remaining_input);

                // Reset state
                self.context = LexerContext::ExpectingOperator;

                return Some((SyntaxKind::ERROR, flags_slice));
            } else if has_valid_flags {
                // All flags are valid
                let syntax_kind = match mode {
                    QuoteLikeMode::S => SyntaxKind::S_FLAGS,
                    QuoteLikeMode::TR => SyntaxKind::TR_FLAGS,
                    _ => unreachable!(),
                };

                // Get the flags text
                let flags_slice = &input[start_pos..start_pos + flag_end];

                // Create a new lexer starting after the consumed flags
                let new_start = start_pos + flag_end;
                let remaining_input = &input[new_start..];
                self.logos_lexer = Token::lexer(remaining_input);

                // Reset state
                self.context = LexerContext::ExpectingOperator;

                return Some((syntax_kind, flags_slice));
            }
        }

        // No flags found, transition back to normal parsing
        self.context = LexerContext::ExpectingOperator;
        None
    }

    /// Handle delimiter transitions in quote-like context
    fn handle_quote_like_delimiter(&mut self, delimiter_text: &str) {
        if let LexerContext::QuoteLike {
            mode,
            state,
            delimiter,
            prefix,
        } = self.context
        {
            let delimiter_char = delimiter_text.chars().next().unwrap_or(delimiter);

            match state {
                QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                } => {
                    // Set the delimiter and transition to content
                    self.context = LexerContext::QuoteLike {
                        prefix,
                        mode,
                        state: QuoteLikeState::Content {
                            phase: DelimiterPhase::First,
                        },
                        delimiter: delimiter_char,
                    };
                }
                QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Close,
                } => {
                    // Check if this is a double-delimiter mode (S, TR)
                    match mode {
                        QuoteLikeMode::S | QuoteLikeMode::TR => {
                            // Check if delimiter is symmetric
                            let is_symmetric = self.is_symmetric_delimiter(delimiter);
                            if is_symmetric {
                                // Same delimiter for second part
                                self.context = LexerContext::QuoteLike {
                                    prefix,
                                    mode,
                                    state: QuoteLikeState::Content {
                                        phase: DelimiterPhase::Second,
                                    },
                                    delimiter,
                                };
                            } else {
                                // Need opening delimiter for second part
                                self.context = LexerContext::QuoteLike {
                                    prefix,
                                    mode,
                                    state: QuoteLikeState::Delimiter {
                                        phase: DelimiterPhase::Second,
                                        kind: DelimiterType::Open,
                                    },
                                    delimiter,
                                };
                            }
                        }
                        _ => {
                            // Single delimiter modes - we're done
                            self.context = LexerContext::ExpectingOperator;
                        }
                    }
                }
                QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::Second,
                    kind: DelimiterType::Open,
                } => {
                    // Start of second content
                    self.context = LexerContext::QuoteLike {
                        prefix,
                        mode,
                        state: QuoteLikeState::Content {
                            phase: DelimiterPhase::Second,
                        },
                        delimiter: delimiter_char,
                    };
                }
                QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::Second,
                    kind: DelimiterType::Close,
                } => {
                    // Check if this mode has flags
                    match mode {
                        QuoteLikeMode::S | QuoteLikeMode::TR => {
                            self.context = LexerContext::QuoteLike {
                                prefix,
                                mode,
                                state: QuoteLikeState::Flags,
                                delimiter,
                            };
                        }
                        _ => {
                            self.context = LexerContext::ExpectingOperator;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Check if a delimiter is symmetric (same opening and closing character)
    fn is_symmetric_delimiter(&self, delimiter: char) -> bool {
        matches!(
            delimiter,
            '/' | '|'
                | '#'
                | '!'
                | '~'
                | '@'
                | '$'
                | '%'
                | '^'
                | '&'
                | '*'
                | '+'
                | '='
                | '?'
                | '`'
                | '\''
                | '"'
        )
    }

    /// Try to consume `qw()` content, tokenizing whitespace-separated words
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

        // Get the expected closing delimiter
        let expected_closing_delimiter =
            if let LexerContext::QuoteLike { delimiter, .. } = &self.context {
                match delimiter {
                    '{' => '}',
                    '[' => ']',
                    '(' => ')',
                    '<' => '>',
                    other => *other, // For symmetric delimiters, return the same
                }
            } else {
                return None;
            };

        // If we start with the expected closing delimiter, let the normal lexer handle it
        if first_char == expected_closing_delimiter {
            return None;
        }

        // Otherwise, consume a word (non-whitespace sequence)
        let mut end_pos = 0;
        for ch in remainder.chars() {
            // Stop at whitespace or the specific closing delimiter
            if ch.is_whitespace() || ch == expected_closing_delimiter {
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
}
