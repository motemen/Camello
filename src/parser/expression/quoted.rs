use crate::SyntaxKind;

use super::super::Parser;

impl<'a> Parser<'a> {
    pub fn qw_expr(&mut self) {
        self.builder.start_node(SyntaxKind::QW_EXPR.into());

        // "qw"
        self.expect(SyntaxKind::QW_KW);
        self.skip_trivia();

        // Determine delimiter and find closing delimiter
        let (opening_delim, closing_delim) = match self.current_kind() {
            Some(SyntaxKind::L_PAREN) => (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN),
            Some(SyntaxKind::L_BRACKET) => (SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET),
            Some(SyntaxKind::L_BRACE) => (SyntaxKind::L_BRACE, SyntaxKind::R_BRACE),
            Some(SyntaxKind::SLASH) => (SyntaxKind::SLASH, SyntaxKind::SLASH),
            _ => {
                self.error("Expected qw() delimiter: (, [, {, or /");
                return;
            }
        };

        // Consume opening delimiter
        self.expect(opening_delim);
        // Don't skip trivia here - we need whitespace to separate words

        // Parse words inside qw() - consume existing tokens and convert to QW_STRING
        while !self.at(closing_delim) && !self.at_end() {
            // Skip whitespace/trivia
            if let Some(kind) = self.current_kind() {
                if kind.is_trivia() {
                    self.bump();
                    continue;
                }
            }

            // Check if we're at the closing delimiter
            if self.at(closing_delim) {
                break;
            }

            // Consume any non-whitespace tokens as QW_STRING
            if let Some((_, text)) = self.current_token.take() {
                // Add as QW_STRING token
                self.builder.token(SyntaxKind::QW_STRING.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        // Closing delimiter
        self.expect(closing_delim);

        self.builder.finish_node();
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
        self.parse_tr_expr_with_keyword(SyntaxKind::Y_KW, "y");
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

        // Determine delimiter and find closing delimiter
        let (opening_delim, closing_delim) = match self.current_kind() {
            Some(SyntaxKind::L_PAREN) => (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN),
            Some(SyntaxKind::L_BRACKET) => (SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET),
            Some(SyntaxKind::L_BRACE) => (SyntaxKind::L_BRACE, SyntaxKind::R_BRACE),
            Some(SyntaxKind::SLASH) => (SyntaxKind::SLASH, SyntaxKind::SLASH),
            _ => {
                self.error(&format!(
                    "Expected {}() delimiter: (, [, {{, or /",
                    operator_name
                ));
                self.builder.finish_node(); // Finish the node to avoid panic
                return;
            }
        };

        // Consume opening delimiter
        self.expect(opening_delim);

        // Parse content inside - everything becomes the specific string kind
        while !self.at(closing_delim) && !self.at_end() {
            // Check if we're at the closing delimiter
            if self.at(closing_delim) {
                break;
            }

            // Consume any tokens as the string kind (preserving original text)
            if let Some((_, text)) = self.current_token.take() {
                self.builder.token(string_kind.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        // Closing delimiter
        self.expect(closing_delim);

        // For regex expressions (m and qr), consume optional flags
        if matches!(expr_kind, SyntaxKind::M_EXPR | SyntaxKind::QR_EXPR) {
            self.consume_regex_flags();
        }

        self.builder.finish_node();
    }

    pub fn parse_s_expr(&mut self) {
        self.builder.start_node(SyntaxKind::S_EXPR.into());

        // "s"
        self.expect(SyntaxKind::S_KW);
        self.skip_trivia();

        // Determine delimiter and find closing delimiter
        let (opening_delim, closing_delim) = match self.current_kind() {
            Some(SyntaxKind::L_PAREN) => (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN),
            Some(SyntaxKind::L_BRACKET) => (SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET),
            Some(SyntaxKind::L_BRACE) => (SyntaxKind::L_BRACE, SyntaxKind::R_BRACE),
            Some(SyntaxKind::SLASH) => (SyntaxKind::SLASH, SyntaxKind::SLASH),
            _ => {
                self.error("Expected s() delimiter: (, [, {, or /");
                self.builder.finish_node(); // Finish the node to avoid panic
                return;
            }
        };

        // Consume opening delimiter
        self.expect(opening_delim);

        // Parse pattern part - everything until the middle delimiter becomes S_PATTERN
        while !self.at(closing_delim) && !self.at_end() {
            // Consume any tokens as pattern (preserving original text)
            if let Some((_, text)) = self.current_token.take() {
                self.builder.token(SyntaxKind::S_PATTERN.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        self.expect(closing_delim);

        let closing_delim_repl = if opening_delim != closing_delim {
            // Paired delimiters for pattern. Replacement can have its own delimiters.
            let (opening_repl, closing_repl) = match self.current_kind() {
                Some(SyntaxKind::L_PAREN) => (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN),
                Some(SyntaxKind::L_BRACKET) => (SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET),
                Some(SyntaxKind::L_BRACE) => (SyntaxKind::L_BRACE, SyntaxKind::R_BRACE),
                Some(SyntaxKind::SLASH) => (SyntaxKind::SLASH, SyntaxKind::SLASH), // s(pat)/repl/ is also valid
                _ => {
                    self.error("Expected opening delimiter for replacement part of s expression");
                    self.builder.finish_node();
                    return;
                }
            };
            self.expect(opening_repl);
            closing_repl
        } else {
            // Symmetric delimiter for pattern. Replacement uses the same.
            closing_delim
        };

        // Parse replacement part - everything until the final delimiter becomes S_REPLACEMENT
        while !self.at(closing_delim_repl) && !self.at_end() {
            // Consume any tokens as replacement (preserving original text)
            if let Some((_, text)) = self.current_token.take() {
                self.builder.token(SyntaxKind::S_REPLACEMENT.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        // Final closing delimiter
        self.expect(closing_delim_repl);

        // Consume optional flags (like 'g', 'i', 'm', 's', 'x')
        self.consume_regex_flags();

        self.builder.finish_node();
    }

    pub fn parse_tr_expr(&mut self) {
        self.parse_tr_expr_with_keyword(SyntaxKind::TR_KW, "tr");
    }

    pub fn parse_tr_expr_with_keyword(&mut self, keyword_kind: SyntaxKind, keyword_name: &str) {
        self.builder.start_node(SyntaxKind::TR_EXPR.into());

        // "tr" or "y"
        self.expect(keyword_kind);
        self.skip_trivia();

        // Determine delimiter and find closing delimiter
        let (opening_delim, closing_delim) = match self.current_kind() {
            Some(SyntaxKind::L_PAREN) => (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN),
            Some(SyntaxKind::L_BRACKET) => (SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET),
            Some(SyntaxKind::L_BRACE) => (SyntaxKind::L_BRACE, SyntaxKind::R_BRACE),
            Some(SyntaxKind::SLASH) => (SyntaxKind::SLASH, SyntaxKind::SLASH),
            _ => {
                self.error(&format!(
                    "Expected {}() delimiter: (, [, {{, or /",
                    keyword_name
                ));
                self.builder.finish_node(); // Finish the node to avoid panic
                return;
            }
        };

        // Consume opening delimiter
        self.expect(opening_delim);

        // Parse search list part - everything until the middle delimiter becomes TR_SEARCH_LIST
        while !self.at(closing_delim) && !self.at_end() {
            // Consume any tokens as search list (preserving original text)
            if let Some((_, text)) = self.current_token.take() {
                self.builder.token(SyntaxKind::TR_SEARCH_LIST.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        self.expect(closing_delim);

        let closing_delim_repl = if opening_delim != closing_delim {
            // Paired delimiters for search list. Replacement can have its own delimiters.
            let (opening_repl, closing_repl) = match self.current_kind() {
                Some(SyntaxKind::L_PAREN) => (SyntaxKind::L_PAREN, SyntaxKind::R_PAREN),
                Some(SyntaxKind::L_BRACKET) => (SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET),
                Some(SyntaxKind::L_BRACE) => (SyntaxKind::L_BRACE, SyntaxKind::R_BRACE),
                Some(SyntaxKind::SLASH) => (SyntaxKind::SLASH, SyntaxKind::SLASH), // tr(search)/repl/ is also valid
                _ => {
                    self.error(&format!(
                        "Expected opening delimiter for replacement part of {} expression",
                        keyword_name
                    ));
                    self.builder.finish_node();
                    return;
                }
            };
            self.expect(opening_repl);
            closing_repl
        } else {
            // Symmetric delimiter for search list. Replacement uses the same.
            closing_delim
        };

        // Parse replacement list part - everything until the final delimiter becomes TR_REPLACEMENT_LIST
        while !self.at(closing_delim_repl) && !self.at_end() {
            // Consume any tokens as replacement list (preserving original text)
            if let Some((_, text)) = self.current_token.take() {
                self.builder
                    .token(SyntaxKind::TR_REPLACEMENT_LIST.into(), text);
                self.current_pos += text.len();
                self.current_token = self.lexer.next_token();
            }
        }

        // Final closing delimiter
        self.expect(closing_delim_repl);

        // Consume optional flags (like 'd', 'c', 's')
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
