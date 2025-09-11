//! This module provides functionality for handling quote-like delimiters and tokens in the lexer.
//! It includes methods for recognizing, consuming, and transitioning between different states
//! of quote-like constructs, such as `q`, `qw`, `m`, `qr`, `s`, and `tr` operators.

use super::{
    DelimiterPhase, DelimiterType, Lexer, LexerContext, QuoteLikeMode, QuoteLikeState, Token,
};
use crate::SyntaxKind;
use logos::Logos;

impl<'a> Lexer<'a> {
    /// Attempts to handle a quote-like delimiter based on the current lexer context.
    ///
    /// This method checks the current state and determines whether the next character
    /// in the input matches the expected delimiter. If it does, the delimiter is consumed
    /// and the lexer transitions to the appropriate state.
    ///
    /// Returns:
    /// - `Some((SyntaxKind::DELIMITER, &str))` if a delimiter is successfully consumed.
    /// - `None` if no valid delimiter is found.
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

    /// Handles quote-like tokens, including both content and delimiters, based on the current context and state.
    ///
    /// This method processes various quote-like modes (`q`, `qw`, `m`, `qr`, `s`, `tr`) and their respective states.
    /// It transitions between content and delimiter states as needed.
    ///
    /// Returns:
    /// - `Some((SyntaxKind, &str))` if a token is successfully consumed.
    /// - `None` if no valid token is found.
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
            ) => {
                let LexerContext::QuoteLike { prefix, .. } = self.context else {
                    return None;
                };
                let content_kind = self.get_q_mode_content_kind(prefix);
                self.consume_quote_content(
                    content_kind,
                    delimiter,
                    QuoteLikeState::Delimiter {
                        phase: DelimiterPhase::First,
                        kind: DelimiterType::Close,
                    },
                )
                .or_else(|| self.try_handle_quote_like_delimiter_internal())
            }

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
                    SyntaxKind::REGEX_PATTERN,
                    delimiter,
                    QuoteLikeState::Delimiter {
                        phase: DelimiterPhase::First,
                        kind: DelimiterType::Close,
                    },
                )
                .or_else(|| self.try_handle_quote_like_delimiter_internal()),

            (
                QuoteLikeMode::QR,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::First,
                },
            ) => self
                .consume_quote_content(
                    SyntaxKind::REGEX_PATTERN,
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
                    SyntaxKind::REGEX_PATTERN,
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
                    phase: DelimiterPhase::Second,
                },
            ) => self
                .consume_quote_content(
                    SyntaxKind::INTERPOLATED_STRING,
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
                        phase: DelimiterPhase::First,
                        kind: DelimiterType::Close,
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

            // Delimiter states - handle delimiters or fallback to logos for other tokens
            (_, QuoteLikeState::Delimiter { .. }) => {
                self.try_handle_quote_like_delimiter_internal().or_else(|| {
                    // If no specific delimiter handling, try generic token processing
                    match self.logos_lexer.next() {
                        Some(Ok(token)) => {
                            let text = self.logos_lexer.slice();
                            let syntax_kind = match token {
                                // In quote-like context, these should be treated as delimiters
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
                                | Token::Semicolon
                                | Token::Slash
                                | Token::Ampersand
                                | Token::Caret
                                | Token::Pipe
                                | Token::Percent
                                | Token::Star => {
                                    // Call handle_quote_like_delimiter to update state
                                    self.handle_quote_like_delimiter(text);
                                    SyntaxKind::DELIMITER
                                }
                                _ => token.to_syntax_kind(),
                            };
                            Some((syntax_kind, text))
                        }
                        Some(Err(())) => {
                            let text = self.logos_lexer.slice();
                            Some((SyntaxKind::ERROR, text))
                        }
                        None => None,
                    }
                })
            }

            // Flag states
            (
                QuoteLikeMode::M | QuoteLikeMode::QR | QuoteLikeMode::S | QuoteLikeMode::TR,
                QuoteLikeState::Flags,
            ) => self.try_consume_quote_like_flags(&mode),

            // Invalid state combinations that should never occur
            (
                QuoteLikeMode::Q | QuoteLikeMode::QW,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::Second,
                },
            )
            | (QuoteLikeMode::Q | QuoteLikeMode::QW, QuoteLikeState::Flags) => {
                unreachable!("Invalid state combination for single-delimiter quote-like operators: {:?}, {:?}", mode, state)
            }

            (
                QuoteLikeMode::M | QuoteLikeMode::QR,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::Second,
                },
            ) => {
                unreachable!("Invalid state combination for single-delimiter quote-like operators: {:?}, {:?}", mode, state)
            }
        }
    }

    /// Consumes quote-like content and transitions the lexer state.
    ///
    /// This helper method is used to process the content of quote-like constructs.
    /// It identifies the closing delimiter and updates the lexer context to the next state.
    ///
    /// Parameters:
    /// - `content_kind`: The syntax kind of the content being consumed.
    /// - `delimiter`: The opening delimiter character.
    /// - `next_state`: The next lexer state to transition to after consuming the content.
    ///
    /// Returns:
    /// - `Some((SyntaxKind, &str))` if content is successfully consumed.
    /// - `None` if no valid content is found.
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

    /// Determines the appropriate content SyntaxKind for Q mode based on the prefix.
    ///
    /// Parameters:
    /// - `prefix`: The quote-like keyword prefix (Q_KW, QQ_KW, QX_KW).
    ///
    /// Returns:
    /// - The corresponding SyntaxKind for the content.
    fn get_q_mode_content_kind(&self, prefix: SyntaxKind) -> SyntaxKind {
        match prefix {
            SyntaxKind::Q_KW => SyntaxKind::LITERAL_STRING,
            SyntaxKind::QQ_KW | SyntaxKind::QX_KW => SyntaxKind::INTERPOLATED_STRING,
            _ => SyntaxKind::LITERAL_STRING, // fallback
        }
    }

    /// Handles `qw` (quote word) content specifically.
    ///
    /// This method processes whitespace-separated words within `qw` constructs.
    /// It also checks for the closing delimiter and transitions the lexer state accordingly.
    ///
    /// Parameters:
    /// - `delimiter`: The opening delimiter character.
    ///
    /// Returns:
    /// - `Some((SyntaxKind, &str))` if content is successfully consumed.
    /// - `None` if no valid content is found.
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

    /// Retrieves the closing delimiter corresponding to a given opening delimiter.
    ///
    /// Parameters:
    /// - `opening`: The opening delimiter character.
    ///
    /// Returns:
    /// - A `String` representing the closing delimiter.
    fn get_closing_delimiter(opening: char) -> String {
        match opening {
            '{' => "}".to_string(),
            '[' => "]".to_string(),
            '(' => ")".to_string(),
            '<' => ">".to_string(),
            _ => opening.to_string(), // For symmetric delimiters, return the same
        }
    }

    /// Attempts to consume the content of a quote-like string.
    ///
    /// This method identifies the end of the string content based on the closing delimiter
    /// and handles escape sequences and nested delimiters.
    ///
    /// Parameters:
    /// - `content_kind`: The syntax kind of the content being consumed.
    /// - `delimiter`: The closing delimiter string.
    ///
    /// Returns:
    /// - `Some((SyntaxKind, &str))` if content is successfully consumed.
    /// - `None` if no valid content is found.
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

    /// Checks if a character opens a nested delimiter that matches the closing delimiter.
    ///
    /// Parameters:
    /// - `open_char`: The opening character.
    /// - `close_char`: The closing character.
    ///
    /// Returns:
    /// - `true` if the characters form a valid nested delimiter pair.
    /// - `false` otherwise.
    fn is_nested_delimiter_pair(&self, open_char: char, close_char: char) -> bool {
        match close_char {
            ')' => open_char == '(',
            '}' => open_char == '{',
            ']' => open_char == '[',
            '>' => open_char == '<',
            _ => false,
        }
    }

    /// Attempts to consume flags for quote-like operators (e.g., `m//flags`, `qr//flags`).
    ///
    /// This method processes consecutive flag characters and validates them based on the operator mode.
    ///
    /// Parameters:
    /// - `mode`: The quote-like mode (e.g., `m`, `qr`, `s`, `tr`).
    ///
    /// Returns:
    /// - `Some((SyntaxKind, &str))` if flags are successfully consumed.
    /// - `None` if no valid flags are found.
    pub(super) fn try_consume_quote_like_flags(
        &mut self,
        mode: &QuoteLikeMode,
    ) -> Option<(SyntaxKind, &'a str)> {
        let start_pos = self.logos_lexer.span().end;
        let input = self.logos_lexer.source();
        let remaining = &input[start_pos..];

        // Define valid flag characters for each operator type
        let valid_flags = match mode {
            QuoteLikeMode::M => "msixpodualngcer",
            QuoteLikeMode::QR => "msixpodualngcer",
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
                self.context = LexerContext::Default;

                return Some((SyntaxKind::ERROR, flags_slice));
            } else if has_valid_flags {
                // All flags are valid
                let syntax_kind = match mode {
                    QuoteLikeMode::M => SyntaxKind::M_FLAGS,
                    QuoteLikeMode::QR => SyntaxKind::QR_FLAGS,
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
                self.context = LexerContext::Default;

                return Some((syntax_kind, flags_slice));
            }
        }

        // No flags found, transition back to normal parsing
        self.context = LexerContext::Default;
        None
    }

    /// Handles delimiter transitions in quote-like contexts.
    ///
    /// This method updates the lexer state based on the current delimiter and context.
    ///
    /// Parameters:
    /// - `delimiter_text`: The text of the delimiter being processed.
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
                    // Check if this is a double-delimiter mode (S, TR) or a mode with flags (M, QR)
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
                        QuoteLikeMode::M | QuoteLikeMode::QR => {
                            // m// and qr// can have flags, so transition to Flags state
                            self.context = LexerContext::QuoteLike {
                                prefix,
                                mode,
                                state: QuoteLikeState::Flags,
                                delimiter,
                            };
                        }
                        _ => {
                            // Other single delimiter modes (q, qw) are done
                            self.context = LexerContext::Default;
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
                            self.context = LexerContext::Default;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Checks if a delimiter is symmetric (i.e., the same character is used for opening and closing).
    ///
    /// Parameters:
    /// - `delimiter`: The delimiter character.
    ///
    /// Returns:
    /// - `true` if the delimiter is symmetric.
    /// - `false` otherwise.
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

    /// Attempts to consume `qw()` content, tokenizing whitespace-separated words.
    ///
    /// This method processes words within `qw` constructs and handles leading whitespace.
    ///
    /// Returns:
    /// - `Some((SyntaxKind, &str))` if content is successfully consumed.
    /// - `None` if no valid content is found.
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

    /// Checks if a character can be used as a quote-like delimiter.
    ///
    /// Parameters:
    /// - `ch`: The character to check.
    ///
    /// Returns:
    /// - `true` if the character is a valid quote-like delimiter.
    /// - `false` otherwise.
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
