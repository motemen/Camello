use crate::SyntaxKind;

use super::super::Parser;

impl<'a> Parser<'a> {
    pub fn qw_expr(&mut self) {
        self.builder.start_node(SyntaxKind::QW_EXPR.into());

        // "qw"
        self.expect(SyntaxKind::QW_KW);
        self.skip_trivia();

        // Consume opening delimiter (should be DELIMITER token)
        if !self.at(SyntaxKind::DELIMITER) {
            self.error("Expected qw delimiter");
            return;
        }

        // Get the delimiter text to determine the closing delimiter
        let opening_delim_text = if let Some((_, text)) = &self.current_token {
            text.to_string()
        } else {
            self.error("Expected delimiter text");
            return;
        };

        // Consume opening delimiter
        self.expect(SyntaxKind::DELIMITER);

        // Determine the closing delimiter based on opening delimiter
        let closing_delim_text = self.get_closing_delimiter(&opening_delim_text);
        // Don't skip trivia here - we need whitespace to separate words

        // Parse words inside qw() - lexer now provides QW_STRING tokens directly
        while !self.at_delimiter_text(&closing_delim_text) && !self.at_end() {
            // Skip whitespace/trivia
            if let Some(kind) = self.current_kind() {
                if kind.is_trivia() {
                    self.bump();
                    continue;
                }
            }

            // Check if we're at the closing delimiter
            if self.at_delimiter_text(&closing_delim_text) {
                break;
            }

            // Expect QW_STRING tokens from the lexer
            if self.at(SyntaxKind::QW_STRING) {
                self.bump(); // Consume QW_STRING token
            } else {
                // If we encounter unexpected token, break
                break;
            }
        }

        // Closing delimiter (should be DELIMITER token with matching text)
        self.expect_delimiter_text(&closing_delim_text);

        self.builder.finish_node();

        // Set lexer context to ExpectingOperator after completing the qw expression
        // This allows proper parsing of operators like 'x' that follow qw expressions
        self.set_lexer_context(crate::lexer::LexerContext::ExpectingOperator);

        // Force a fresh token fetch to ensure the context change takes effect
        // Skip any trivia and refresh the current token with the new context
        self.skip_trivia();
    }

    pub fn q_expr(&mut self) {
        self.parse_q_family_expr(
            SyntaxKind::Q_EXPR,
            SyntaxKind::Q_KW,
            SyntaxKind::Q_STRING,
            "q",
        );
    }

    pub fn qq_expr(&mut self) {
        self.parse_q_family_expr(
            SyntaxKind::QQ_EXPR,
            SyntaxKind::QQ_KW,
            SyntaxKind::QQ_STRING,
            "qq",
        );
    }

    pub fn qx_expr(&mut self) {
        self.parse_q_family_expr(
            SyntaxKind::QX_EXPR,
            SyntaxKind::QX_KW,
            SyntaxKind::QX_STRING,
            "qx",
        );
    }

    pub fn m_expr(&mut self) {
        self.parse_q_family_expr(
            SyntaxKind::M_EXPR,
            SyntaxKind::M_KW,
            SyntaxKind::M_STRING,
            "m",
        );
    }

    pub fn qr_expr(&mut self) {
        self.parse_q_family_expr(
            SyntaxKind::QR_EXPR,
            SyntaxKind::QR_KW,
            SyntaxKind::QR_STRING,
            "qr",
        );
    }

    pub fn s_expr(&mut self) {
        self.parse_s_expr();
    }

    pub fn tr_expr(&mut self) {
        self.parse_tr_expr();
    }

    pub fn y_expr(&mut self) {
        self.parse_tr_expr_with_keyword(SyntaxKind::Y_KW);
    }

    pub fn consume_regex_flags(&mut self) {
        // Consume regex flags like 'g', 'i', 'm', 's', 'x' after the closing delimiter
        // These might be treated as IDENT tokens or specific keywords, but should be part of the regex
        while let Some(kind) = self.current_kind() {
            let is_valid_flag = match kind {
                SyntaxKind::IDENT => {
                    if let Some((_, text)) = &self.current_token {
                        // Check if it's a valid regex flag
                        text.chars()
                            .all(|c| matches!(c, 'g' | 'i' | 'm' | 's' | 'x'))
                            && !text.is_empty()
                    } else {
                        false
                    }
                }
                // Handle single character flags that might be interpreted as keywords
                SyntaxKind::M_KW => {
                    // 'm' flag might be interpreted as M_KW
                    if let Some((_, text)) = &self.current_token {
                        *text == "m"
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if is_valid_flag {
                self.bump(); // consume the flags
            } else {
                break; // Not a regex flag, stop consuming
            }
        }
    }

    pub fn parse_q_family_expr(
        &mut self,
        expr_kind: SyntaxKind,
        kw_kind: SyntaxKind,
        string_kind: SyntaxKind,
        operator_name: &str,
    ) {
        self.builder.start_node(expr_kind.into());

        // "q", "qq", or "qx"
        self.expect(kw_kind);
        self.skip_trivia();

        // Consume opening delimiter (should be DELIMITER token)
        if !self.at(SyntaxKind::DELIMITER) {
            self.error(&format!("Expected {} delimiter", operator_name));
            self.builder.finish_node();
            return;
        }

        // Get the delimiter text to determine the closing delimiter
        let opening_delim_text = if let Some((_, text)) = &self.current_token {
            text.to_string()
        } else {
            self.error("Expected delimiter text");
            self.builder.finish_node();
            return;
        };

        // Consume opening delimiter
        self.expect(SyntaxKind::DELIMITER);

        // Determine the closing delimiter based on opening delimiter
        let closing_delim_text = self.get_closing_delimiter(&opening_delim_text);

        // Parse content inside - everything becomes the specific string kind
        while !self.at_delimiter_text(&closing_delim_text) && !self.at_end() {
            // Consume any tokens as the string kind (preserving original text)
            if let Some((_, text)) = self.current_token.take() {
                self.builder.token(string_kind.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        // Closing delimiter (should be DELIMITER token with matching text)
        self.expect_delimiter_text(&closing_delim_text);

        // For regex expressions (m and qr), consume optional flags
        if matches!(expr_kind, SyntaxKind::M_EXPR | SyntaxKind::QR_EXPR) {
            self.consume_regex_flags();
        }

        self.builder.finish_node();

        // Set lexer context to ExpectingOperator after completing the q-family expression
        // This allows proper parsing of operators like 'x' that follow q expressions
        self.set_lexer_context(crate::lexer::LexerContext::ExpectingOperator);

        // Force a fresh token fetch to ensure the context change takes effect
        // Skip any trivia and refresh the current token with the new context
        self.skip_trivia();
    }

    pub fn parse_s_expr(&mut self) {
        self.builder.start_node(SyntaxKind::S_EXPR.into());

        // "s"
        self.expect(SyntaxKind::S_KW);
        self.skip_trivia();

        // Opening delimiter
        self.expect(SyntaxKind::DELIMITER);
        self.skip_trivia();

        // Pattern content (lexer generates S_PATTERN token)
        if self.at(SyntaxKind::S_PATTERN) {
            self.bump();
            self.skip_trivia();
        }

        // Middle delimiter (closing pattern delimiter)
        self.expect(SyntaxKind::DELIMITER);
        self.skip_trivia();

        // For asymmetric delimiters, there might be another opening delimiter for replacement
        if self.at(SyntaxKind::DELIMITER) {
            self.bump(); // Opening delimiter for replacement part
            self.skip_trivia();
        }

        // Replacement content (lexer generates S_REPLACEMENT token)
        if self.at(SyntaxKind::S_REPLACEMENT) {
            self.bump();
            self.skip_trivia();
        }

        // Closing delimiter (for replacement part)
        if self.at(SyntaxKind::DELIMITER) {
            self.bump();
            self.skip_trivia();
        }

        // Optional flags
        while self.at(SyntaxKind::IDENT) {
            let flag_text = self.current_text().unwrap_or("");
            if flag_text
                .chars()
                .all(|c| matches!(c, 'g' | 'i' | 'm' | 's' | 'x' | 'e' | 'o'))
            {
                self.bump();
            } else {
                break;
            }
        }

        self.builder.finish_node();
    }

    pub fn parse_tr_expr(&mut self) {
        self.parse_tr_expr_with_keyword(SyntaxKind::TR_KW);
    }

    pub fn parse_tr_expr_with_keyword(&mut self, keyword_kind: SyntaxKind) {
        self.builder.start_node(SyntaxKind::TR_EXPR.into());

        // "tr" or "y"
        self.expect(keyword_kind);
        self.skip_trivia();

        // Opening delimiter
        self.expect(SyntaxKind::DELIMITER);
        self.skip_trivia();

        // Search list content (lexer generates TR_SEARCH_LIST token)
        if self.at(SyntaxKind::TR_SEARCH_LIST) {
            self.bump();
            self.skip_trivia();
        }

        // Middle delimiter
        self.expect(SyntaxKind::DELIMITER);
        self.skip_trivia();

        // For asymmetric delimiters, there might be another opening delimiter for replacement
        if self.at(SyntaxKind::DELIMITER) {
            self.bump(); // Opening delimiter for replacement part
            self.skip_trivia();
        }

        // Replacement list content (lexer generates TR_REPLACEMENT_LIST token)
        if self.at(SyntaxKind::TR_REPLACEMENT_LIST) {
            self.bump();
            self.skip_trivia();
        }

        // Closing delimiter (for replacement part)
        if self.at(SyntaxKind::DELIMITER) {
            self.bump();
            self.skip_trivia();
        }

        // Optional flags
        self.consume_tr_flags();

        self.builder.finish_node();
    }

    pub fn consume_tr_flags(&mut self) {
        // Consume tr/y flags like 'd', 'c', 's' after the closing delimiter
        // These might be treated as IDENT tokens but should be part of the tr expression
        while let Some(kind) = self.current_kind() {
            let is_valid_flag = match kind {
                SyntaxKind::IDENT => {
                    if let Some((_, text)) = &self.current_token {
                        // Check if it's a valid tr flag
                        text.chars().all(|c| matches!(c, 'd' | 'c' | 's')) && !text.is_empty()
                    } else {
                        false
                    }
                }
                SyntaxKind::S_KW => {
                    // Special case: 's' might be disambiguated as S_KW but should be treated as a flag
                    // FIXME: This is a bit hacky - ideally lexer shouldn't produce S_KW here
                    if let Some((_, text)) = &self.current_token {
                        *text == "s"
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if is_valid_flag {
                self.bump(); // consume the flags
            } else {
                break; // Not a tr flag, stop consuming
            }
        }
    }
}
