use rowan::NodeOrToken;

use crate::{PerlNode, SyntaxKind};

use super::Formatter;

impl Formatter {
    pub fn format_hash_ref(&mut self, node: &PerlNode) {
        self.format_delimited_literal(node, SyntaxKind::L_BRACE, SyntaxKind::R_BRACE);
    }

    pub fn format_array_ref(&mut self, node: &PerlNode) {
        self.format_delimited_literal(node, SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET);
    }

    fn format_single_line_delimited_literal(
        &mut self,
        node: &PerlNode,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        k if k == opening => {
                            self.handle_spacing_before(kind);
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.write_str(text, Some(kind));
                            self.prev_token_kind = Some(kind);
                        }
                        k if k == closing => {
                            self.write_str(text, Some(kind));
                            self.prev_token_kind = Some(kind);
                        }
                        _ => {
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }

    fn format_delimited_literal(
        &mut self,
        node: &PerlNode,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) {
        if self.has_newline_before_first_value(node) {
            self.format_multiline_delimited(node, opening, closing);
        } else {
            self.format_single_line_delimited_literal(node, opening, closing);
        }
    }

    pub fn format_qw_expr(&mut self, node: &PerlNode) {
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_qw_expr(node);
        } else {
            self.format_single_line_qw_expr(node);
        }
    }

    fn format_single_line_qw_expr(&mut self, node: &PerlNode) {
        // Special formatting for qw() expressions
        let mut first_word = true;

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        SyntaxKind::QW_STRING => {
                            // Add spaces between QW_STRING tokens
                            if !first_word {
                                self.write_char(' ');
                            }
                            self.write_str(text, Some(kind));
                            first_word = false;
                            self.prev_token_kind = Some(kind);
                        }
                        _ => {
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }

    fn format_multiline_qw_expr(&mut self, node: &PerlNode) {
        let mut opened = false;

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        SyntaxKind::DELIMITER => {
                            if opened {
                                self.handle_multiline_closing_delimiter(&token);
                            } else {
                                self.handle_multiline_opening_delimiter(&token);
                                opened = true;
                            }
                        }
                        SyntaxKind::QW_STRING => {
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.write_str(text, Some(kind));
                            self.handle_newline();
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Newline is handled for QW_STRING tokens, so skip whitespace here
                        }
                        _ => {
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }

    pub fn format_q_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::Q_KW, SyntaxKind::LITERAL_STRING);
    }

    pub fn format_qq_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QQ_KW, SyntaxKind::INTERPOLATED_STRING);
    }

    pub fn format_qx_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QX_KW, SyntaxKind::INTERPOLATED_STRING);
    }

    pub fn format_m_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::M_KW, SyntaxKind::REGEX_PATTERN);
    }

    pub fn format_qr_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QR_KW, SyntaxKind::REGEX_PATTERN);
    }

    pub fn format_s_expr(&mut self, node: &PerlNode) {
        self.format_regex_like_expr(node, &[SyntaxKind::S_KW]);
    }

    pub fn format_tr_expr(&mut self, node: &PerlNode) {
        self.format_regex_like_expr(node, &[SyntaxKind::TR_KW, SyntaxKind::Y_KW]);
    }

    fn format_regex_like_expr(&mut self, node: &PerlNode, kw_kinds: &[SyntaxKind]) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();
                    match kind {
                        k if kw_kinds.contains(&k) => {
                            self.format_token(&token);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Preserve whitespace inside these expressions
                            self.write_str(text, Some(kind));
                        }
                        _ => {
                            self.write_str(text, Some(kind));
                            self.prev_token_kind = Some(kind);
                        }
                    }
                }
            }
        }
    }

    fn format_q_family_expr(
        &mut self,
        node: &PerlNode,
        kw_kind: SyntaxKind,
        string_kind: SyntaxKind,
    ) {
        // q-family expressions always format as single line
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();
                    match kind {
                        k if k == kw_kind => {
                            self.format_token(&token);
                        }
                        SyntaxKind::L_PAREN
                        | SyntaxKind::L_BRACKET
                        | SyntaxKind::SLASH
                        | SyntaxKind::DELIMITER => {
                            self.write_str(text, Some(kind));
                            self.prev_token_kind = Some(kind);
                        }
                        k if k == string_kind => {
                            self.write_str(text, Some(kind));
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE => {
                            self.write_str(text, Some(kind));
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Special handling: preserve whitespace inside q-family strings
                            self.write_str(text, Some(kind));
                        }
                        _ => {
                            // Handle any remaining tokens (including closing slash) directly
                            self.write_str(text, Some(kind));
                            self.prev_token_kind = Some(kind);
                        }
                    }
                }
            }
        }
    }
}
