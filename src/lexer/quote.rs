//! Quote-like handling for the lexer (parser-driven begin).

use super::{DelimiterPhase, DelimiterType, Lexer, LexerMode, QuoteLikeMode, QuoteLikeState};
use crate::SyntaxKind;

impl<'a> Lexer<'a> {
    /// Called by the parser after consuming q/qq/qx/qw/m/qr/s/tr/y.
    pub fn begin_quote_like(&mut self, prefix: SyntaxKind, mode: QuoteLikeMode) {
        self.mode = LexerMode::QuoteLike {
            prefix,
            mode,
            state: QuoteLikeState::Delimiter {
                phase: DelimiterPhase::First,
                kind: DelimiterType::Open,
            },
            delimiter: '\0',
        };
    }

    pub(super) fn try_handle_quote_like_internal(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let LexerMode::QuoteLike {
            mode,
            state,
            delimiter,
            prefix,
            ..
        } = self.mode
        else {
            return None;
        };

        match (mode, state) {
            (
                QuoteLikeMode::Q,
                QuoteLikeState::Content {
                    phase: DelimiterPhase::First,
                },
            ) => {
                let kind = self.get_q_mode_content_kind(prefix);
                self.consume_quote_content(
                    kind,
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
            ) => self
                .handle_qw_content(delimiter)
                .or_else(|| self.try_handle_quote_like_delimiter_internal()),
            (
                QuoteLikeMode::M | QuoteLikeMode::QR,
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
            // Delimiters only
            (_, QuoteLikeState::Delimiter { .. }) => {
                self.try_handle_quote_like_delimiter_internal()
            }
            // Flags
            (
                QuoteLikeMode::M | QuoteLikeMode::QR | QuoteLikeMode::S | QuoteLikeMode::TR,
                QuoteLikeState::Flags,
            ) => self.try_consume_quote_like_flags(&mode),
            _ => None,
        }
    }

    pub(super) fn try_handle_quote_like_delimiter_internal(
        &mut self,
    ) -> Option<(SyntaxKind, &'a str)> {
        let LexerMode::QuoteLike {
            state, delimiter, ..
        } = self.mode
        else {
            return None;
        };
        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None;
        }
        let first = remainder.chars().next().unwrap();

        let should_consume = match state {
            QuoteLikeState::Delimiter {
                kind: DelimiterType::Open,
                ..
            } => self.is_quote_delimiter(first),
            QuoteLikeState::Delimiter {
                kind: DelimiterType::Close,
                ..
            } => {
                let expected = Self::get_closing_delimiter(delimiter);
                first == expected
            }
            _ => false,
        };

        if should_consume {
            let text = &remainder[..first.len_utf8()];
            self.logos_lexer.bump(first.len_utf8());
            self.handle_quote_like_delimiter(text);
            Some((SyntaxKind::DELIMITER, text))
        } else {
            None
        }
    }

    fn consume_quote_content(
        &mut self,
        content_kind: SyntaxKind,
        delimiter: char,
        next_state: QuoteLikeState,
    ) -> Option<(SyntaxKind, &'a str)> {
        let LexerMode::QuoteLike { prefix, mode, .. } = self.mode else {
            panic!("Invalid state in consume_quote_content");
        };

        let closing = Self::get_closing_delimiter(delimiter);
        // If the very next char is the closing delimiter, treat content as empty and
        // transition to the next delimiter state so the delimiter handler can consume it.
        let rem = self.logos_lexer.remainder();
        if let Some(first) = rem.chars().next() {
            if first == closing {
                self.mode = LexerMode::QuoteLike {
                    prefix,
                    mode,
                    state: next_state,
                    delimiter,
                };

                return None;
            }
        }

        if let Some(tok) =
            self.try_consume_quote_like_string_content(content_kind, delimiter, closing)
        {
            self.mode = LexerMode::QuoteLike {
                prefix,
                mode,
                state: next_state,
                delimiter,
            };

            Some(tok)
        } else {
            None
        }
    }

    fn get_q_mode_content_kind(&self, prefix: SyntaxKind) -> SyntaxKind {
        match prefix {
            SyntaxKind::Q_KW => SyntaxKind::LITERAL_STRING,
            SyntaxKind::QQ_KW | SyntaxKind::QX_KW => SyntaxKind::INTERPOLATED_STRING,
            _ => SyntaxKind::LITERAL_STRING,
        }
    }

    fn handle_qw_content(&mut self, delimiter: char) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if !remainder.is_empty() {
            let ch = remainder.chars().next().unwrap();
            if ch == Self::get_closing_delimiter(delimiter) {
                if let LexerMode::QuoteLike { prefix, mode, .. } = self.mode {
                    self.mode = LexerMode::QuoteLike {
                        prefix,
                        mode,
                        state: QuoteLikeState::Delimiter {
                            phase: DelimiterPhase::First,
                            kind: DelimiterType::Close,
                        },
                        delimiter,
                    };
                }
                return None;
            }
        }
        let closing = Self::get_closing_delimiter(delimiter);
        // Emit whitespace between words as normal tokens so tests see them
        let remainder = self.logos_lexer.remainder();
        if !remainder.is_empty() {
            let ch = remainder.chars().next().unwrap();
            if ch.is_whitespace() {
                let mut i = 0usize;
                for c in remainder.chars() {
                    if c.is_whitespace() {
                        i += c.len_utf8();
                    } else {
                        break;
                    }
                }
                let text = &remainder[..i];
                self.logos_lexer.bump(i);
                return Some((SyntaxKind::WHITESPACE, text));
            }
        }
        self.try_consume_qw_content(delimiter, closing)
    }

    fn get_closing_delimiter(opening: char) -> char {
        match opening {
            '{' => '}',
            '[' => ']',
            '(' => ')',
            '<' => '>',
            other => other,
        }
    }

    fn is_quote_delimiter(&self, c: char) -> bool {
        // Perl allows virtually any non-alphanumeric, non-whitespace as a delimiter.
        // Paired delimiters '([{<' are handled specially for matching closers.
        !c.is_alphanumeric() && !c.is_whitespace()
    }

    fn is_symmetric_delimiter(&self, c: char) -> bool {
        !matches!(c, '(' | '[' | '{' | '<')
    }

    fn try_consume_quote_like_string_content(
        &mut self,
        content_kind: SyntaxKind,
        opening: char,
        closing: char,
    ) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None;
        }

        let mut chars = remainder.char_indices();
        let mut escaped = false;
        let is_paired = matches!(opening, '(' | '[' | '{' | '<');
        let mut nest: i32 = 0;
        // When backslash is the delimiter, disable escape handling
        let escape_enabled = closing != '\\';

        // consume until closing
        let mut end_idx: Option<usize> = None;
        for (i, c) in chars.by_ref() {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '\\' if escape_enabled => {
                    escaped = true;
                }
                ch if ch == closing => {
                    if nest == 0 {
                        end_idx = Some(i);
                        break;
                    } else {
                        nest -= 1;
                    }
                }
                ch if is_paired && ch == opening => {
                    nest += 1;
                }
                _ => {}
            }
        }

        let end = end_idx.unwrap_or(0);
        let text = &remainder[..end];
        self.logos_lexer.bump(text.len());
        if text.is_empty() {
            None
        } else {
            Some((content_kind, text))
        }
    }

    fn try_consume_qw_content(
        &mut self,
        opening: char,
        closing: char,
    ) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if remainder.is_empty() {
            return None;
        }
        let mut i = 0usize;
        let bytes = remainder.as_bytes();
        let is_paired = matches!(opening, '(' | '[' | '{' | '<');
        let mut depth = 0usize;
        // skip whitespace -> return as trivia via normal path; here only words
        while i < bytes.len() {
            let ch = remainder[i..].chars().next().unwrap();
            if ch.is_whitespace() {
                i += ch.len_utf8();
            } else {
                break;
            }
        }
        if i >= bytes.len() {
            return None;
        }
        let start = i;
        while i < bytes.len() {
            let ch = remainder[i..].chars().next().unwrap();
            if ch.is_whitespace() && depth == 0 {
                break;
            }
            if ch == closing && (!is_paired || depth == 0) {
                break;
            }
            if is_paired {
                if ch == opening {
                    depth += 1;
                } else if ch == closing && depth > 0 {
                    depth -= 1;
                }
            }
            i += ch.len_utf8();
        }
        if i > start {
            let text = &remainder[start..i];
            self.logos_lexer.bump(i);
            Some((SyntaxKind::QW_STRING, text))
        } else {
            None
        }
    }

    fn try_consume_quote_like_flags(
        &mut self,
        mode: &QuoteLikeMode,
    ) -> Option<(SyntaxKind, &'a str)> {
        let valid = match mode {
            QuoteLikeMode::TR => "cdsr",
            _ => "msixpodualngcer",
        };
        let remainder = self.logos_lexer.remainder();
        let mut i = 0usize;
        let mut any = false;
        let mut all_valid = true;
        for ch in remainder.chars() {
            if ch.is_alphabetic() {
                any = true;
                if !valid.contains(ch) {
                    all_valid = false;
                }
                i += ch.len_utf8();
            } else {
                break;
            }
        }
        if any {
            let kind = if all_valid {
                match mode {
                    QuoteLikeMode::M => SyntaxKind::M_FLAGS,
                    QuoteLikeMode::QR => SyntaxKind::QR_FLAGS,
                    QuoteLikeMode::S => SyntaxKind::S_FLAGS,
                    QuoteLikeMode::TR => SyntaxKind::TR_FLAGS,
                    _ => SyntaxKind::ERROR,
                }
            } else {
                SyntaxKind::ERROR
            };
            let text = &remainder[..i];
            self.logos_lexer.bump(i);
            // After flags, return to normal
            self.mode = LexerMode::Normal;
            return Some((kind, text));
        }
        // No flags -> back to normal
        self.mode = LexerMode::Normal;
        None
    }

    fn handle_quote_like_delimiter(&mut self, delimiter_text: &str) {
        if let LexerMode::QuoteLike {
            mode,
            state,
            delimiter,
            prefix,
        } = self.mode
        {
            let delim_ch = delimiter_text.chars().next().unwrap_or(delimiter);
            match state {
                QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Open,
                } => {
                    self.mode = LexerMode::QuoteLike {
                        prefix,
                        mode,
                        state: QuoteLikeState::Content {
                            phase: DelimiterPhase::First,
                        },
                        delimiter: delim_ch,
                    };
                }
                QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::First,
                    kind: DelimiterType::Close,
                } => match mode {
                    QuoteLikeMode::S | QuoteLikeMode::TR => {
                        if self.is_symmetric_delimiter(delimiter) {
                            self.mode = LexerMode::QuoteLike {
                                prefix,
                                mode,
                                state: QuoteLikeState::Content {
                                    phase: DelimiterPhase::Second,
                                },
                                delimiter,
                            };
                        } else {
                            self.mode = LexerMode::QuoteLike {
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
                        self.mode = LexerMode::QuoteLike {
                            prefix,
                            mode,
                            state: QuoteLikeState::Flags,
                            delimiter,
                        };
                    }
                    _ => {
                        self.mode = LexerMode::Normal;
                    }
                },
                QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::Second,
                    kind: DelimiterType::Open,
                } => {
                    self.mode = LexerMode::QuoteLike {
                        prefix,
                        mode,
                        state: QuoteLikeState::Content {
                            phase: DelimiterPhase::Second,
                        },
                        delimiter: delim_ch,
                    };
                }
                QuoteLikeState::Delimiter {
                    phase: DelimiterPhase::Second,
                    kind: DelimiterType::Close,
                } => match mode {
                    QuoteLikeMode::S | QuoteLikeMode::TR => {
                        self.mode = LexerMode::QuoteLike {
                            prefix,
                            mode,
                            state: QuoteLikeState::Flags,
                            delimiter,
                        };
                    }
                    _ => {
                        self.mode = LexerMode::Normal;
                    }
                },
                QuoteLikeState::Content { .. } | QuoteLikeState::Flags => {}
            }
        }
    }
}
