use rowan::NodeOrToken;

use crate::{PerlNode, SyntaxKind};

use super::Formatter;

impl Formatter {
    pub(super) fn format_hash_ref(&mut self, node: &PerlNode) {
        self.format_delimited_literal(node, SyntaxKind::L_BRACE, SyntaxKind::R_BRACE);
    }

    pub(super) fn format_array_ref(&mut self, node: &PerlNode) {
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

                    match kind {
                        k if k == opening => {
                            self.handle_spacing_before(kind);
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.write(&token);
                            self.remember_token(&token);
                        }
                        k if k == closing => {
                            self.write(&token);
                            self.remember_token(&token);
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
        let should_multiline = node
            .descendants_with_tokens()
            .any(|element| element.kind() == SyntaxKind::NEWLINE);

        if should_multiline {
            self.format_multiline_delimited(node, opening, closing);
        } else {
            self.format_single_line_delimited_literal(node, opening, closing);
        }
    }

    pub(super) fn format_qw_expr(&mut self, node: &PerlNode) {
        let should_multiline = node
            .children_with_tokens()
            .any(|child| child.as_token().is_some_and(|t| t.text().contains('\n')));

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

                    match kind {
                        SyntaxKind::QW_STRING => {
                            // Add spaces between QW_STRING tokens
                            if !first_word {
                                self.write_char(' ');
                            }
                            self.write(&token);
                            first_word = false;
                            self.remember_token(&token);
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
                            self.write(&token);
                            if !self.ends_with_newline() {
                                self.handle_formatter_newline();
                            }
                            self.remember_token(&token);
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

    pub(super) fn format_q_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::Q_KW, SyntaxKind::LITERAL_STRING);
    }

    pub(super) fn format_qq_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QQ_KW, SyntaxKind::INTERPOLATED_STRING);
    }

    pub(super) fn format_qx_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QX_KW, SyntaxKind::INTERPOLATED_STRING);
    }

    pub(super) fn format_m_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::M_KW, SyntaxKind::REGEX_PATTERN);
    }

    pub(super) fn format_qr_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QR_KW, SyntaxKind::REGEX_PATTERN);
    }

    pub(super) fn format_s_expr(&mut self, node: &PerlNode) {
        self.format_regex_like_expr(node, &[SyntaxKind::S_KW]);
    }

    pub(super) fn format_tr_expr(&mut self, node: &PerlNode) {
        self.format_regex_like_expr(node, &[SyntaxKind::TR_KW, SyntaxKind::Y_KW]);
    }

    fn format_regex_like_expr(&mut self, node: &PerlNode, kw_kinds: &[SyntaxKind]) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    match kind {
                        k if kw_kinds.contains(&k) => {
                            self.format_token(&token);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Preserve whitespace inside these expressions
                            self.write(&token);
                        }
                        _ => {
                            self.write(&token);
                            self.remember_token(&token);
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
                    match kind {
                        k if k == kw_kind => {
                            self.format_token(&token);
                        }
                        SyntaxKind::L_PAREN
                        | SyntaxKind::L_BRACKET
                        | SyntaxKind::SLASH
                        | SyntaxKind::DELIMITER => {
                            self.write(&token);
                            self.remember_token(&token);
                        }
                        k if k == string_kind => {
                            self.write(&token);
                            self.remember_token(&token);
                        }
                        SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE => {
                            self.write(&token);
                            self.remember_token(&token);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Special handling: preserve whitespace inside q-family strings
                            self.write(&token);
                        }
                        _ => {
                            // Handle any remaining tokens (including closing slash) directly
                            self.write(&token);
                            self.remember_token(&token);
                        }
                    }
                }
            }
        }
    }
}
