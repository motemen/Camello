use rowan::NodeOrToken;

use crate::{PerlNode, SyntaxKind, T};

use super::Formatter;

impl Formatter {
    pub(super) fn format_hash_ref(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        self.format_delimited_literal(node, T!['{'], T!['}'], ctx);
    }

    pub(super) fn format_array_ref(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        self.format_delimited_literal(node, T!['['], T![']'], ctx);
    }

    fn format_single_line_delimited_literal(
        &mut self,
        node: &PerlNode,
        opening: SyntaxKind,
        closing: SyntaxKind,
        ctx: super::FormatContext,
    ) {
        self.format_single_line_delimited_children(node, opening, closing, true, ctx);
    }

    fn format_delimited_literal(
        &mut self,
        node: &PerlNode,
        opening: SyntaxKind,
        closing: SyntaxKind,
        ctx: super::FormatContext,
    ) {
        let should_multiline = node
            .descendants_with_tokens()
            .any(|element| element.kind() == SyntaxKind::NEWLINE);

        if should_multiline {
            // When using multiline formatting, create a new context without suppress_newlines
            // to ensure that newlines are preserved within the delimited literal
            let multiline_ctx = super::FormatContext {
                suppress_newlines: false,
                ..ctx
            };
            self.format_multiline_delimited_elements(
                node.children_with_tokens(),
                opening,
                closing,
                multiline_ctx,
            );
        } else {
            self.format_single_line_delimited_literal(node, opening, closing, ctx);
        }
    }

    pub(super) fn format_qw_expr(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        let should_multiline = node
            .children_with_tokens()
            .any(|child| child.as_token().is_some_and(|t| t.text().contains('\n')));

        if should_multiline {
            self.format_multiline_qw_expr(node, ctx);
        } else {
            self.format_single_line_qw_expr(node, ctx);
        }
    }

    fn format_single_line_qw_expr(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        // Special formatting for qw() expressions
        let mut first_word = true;

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node, ctx),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    match kind {
                        SyntaxKind::QW_STRING => {
                            // Add spaces between QW_STRING tokens
                            if !first_word {
                                self.writer.write_char(' ');
                            }
                            self.writer.write_token(&token);
                            first_word = false;
                            self.remember_token(&token);
                        }
                        _ => {
                            self.format_token(&token, ctx);
                        }
                    }
                }
            }
        }
    }

    fn format_multiline_qw_expr(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        let mut opened = false;

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node, ctx),
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
                            if self.writer.at_line_start() {
                                self.writer.add_indent();
                                self.writer.set_at_line_start(false);
                            }
                            self.writer.write_token(&token);
                            if !self.writer.ends_with_newline() {
                                self.writer.handle_formatter_newline();
                            }
                            self.remember_token(&token);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Newline is handled for QW_STRING tokens, so skip whitespace here
                        }
                        _ => {
                            self.format_token(&token, ctx);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn format_q_expr(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        self.format_q_family_expr(node, T![q], SyntaxKind::LITERAL_STRING, ctx);
    }

    pub(super) fn format_qq_expr(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        self.format_q_family_expr(node, T![qq], SyntaxKind::INTERPOLATED_STRING, ctx);
    }

    pub(super) fn format_qx_expr(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        self.format_q_family_expr(node, T![qx], SyntaxKind::INTERPOLATED_STRING, ctx);
    }

    pub(super) fn format_m_expr(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        self.format_q_family_expr(node, T![m], SyntaxKind::REGEX_PATTERN, ctx);
    }

    pub(super) fn format_qr_expr(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        self.format_q_family_expr(node, T![qr], SyntaxKind::REGEX_PATTERN, ctx);
    }

    pub(super) fn format_s_expr(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        self.format_regex_like_expr(node, &[T![s]], ctx);
    }

    pub(super) fn format_tr_expr(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        self.format_regex_like_expr(node, &[T![tr], T![y]], ctx);
    }

    fn format_regex_like_expr(
        &mut self,
        node: &PerlNode,
        kw_kinds: &[SyntaxKind],
        ctx: super::FormatContext,
    ) {
        // For s/// operator, check if 'e' flag is present
        // If not, the replacement part (INTERPOLATED_STRING) should be verbatim
        let has_e_flag = kw_kinds.contains(&T![s])
            && node.children_with_tokens().any(|child| {
                child
                    .as_token()
                    .is_some_and(|t| t.kind() == SyntaxKind::S_FLAGS && t.text().contains('e'))
            });

        // Track whether we've seen the first delimiter pair (for s///)
        // After the second delimiter close, the next INTERPOLATED_STRING is the replacement
        let mut delimiter_depth = 0;
        let mut seen_first_pattern = false;

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node, ctx),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    match kind {
                        k if kw_kinds.contains(&k) => {
                            self.format_token(&token, ctx);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Preserve whitespace inside these expressions
                            self.writer.write_token(&token);
                        }
                        SyntaxKind::DELIMITER => {
                            let text = token.text();
                            if text == "{" || text == "(" || text == "[" {
                                delimiter_depth += 1;
                            } else if text == "}" || text == ")" || text == "]" {
                                delimiter_depth -= 1;
                                if delimiter_depth == 0 && !seen_first_pattern {
                                    seen_first_pattern = true;
                                }
                            }
                            self.writer.write_token(&token);
                            self.remember_token(&token);
                        }
                        SyntaxKind::REGEX_PATTERN => {
                            // First pattern block - preserve as-is
                            self.writer.write_token(&token);
                            self.remember_token(&token);
                        }
                        SyntaxKind::INTERPOLATED_STRING => {
                            // For s/// without 'e' flag, replacement part should be verbatim
                            if kw_kinds.contains(&T![s]) && seen_first_pattern && !has_e_flag {
                                // Write verbatim, preserving all whitespace and indentation
                                let text = token.text();
                                for (i, line) in text.split('\n').enumerate() {
                                    if i > 0 {
                                        self.writer.handle_user_newline();
                                    }
                                    if !line.is_empty() {
                                        self.writer.write_raw(line, Some(kind), Some(&token));
                                    }
                                }
                                self.remember_token(&token);
                            } else {
                                // Normal handling for other cases
                                self.writer.write_token(&token);
                                self.remember_token(&token);
                            }
                        }
                        _ => {
                            self.writer.write_token(&token);
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
        ctx: super::FormatContext,
    ) {
        // q-family expressions always format as single line
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node, ctx),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    match kind {
                        k if k == kw_kind => {
                            self.format_token(&token, ctx);
                        }
                        T!['('] | T!['['] | T![/] | SyntaxKind::DELIMITER => {
                            self.writer.write_token(&token);
                            self.remember_token(&token);
                        }
                        k if k == string_kind => {
                            if token.text().contains('\n') {
                                self.writer.write_str(token.text(), None, None);
                                self.remember_token(&token);
                            } else {
                                self.writer.write_token(&token);
                                self.remember_token(&token);
                            }
                        }
                        T![')'] | T![']'] | T!['}'] => {
                            self.writer.write_token(&token);
                            self.remember_token(&token);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Special handling: preserve whitespace inside q-family strings
                            self.writer.write_token(&token);
                        }
                        _ => {
                            // Handle any remaining tokens (including closing slash) directly
                            self.writer.write_token(&token);
                            self.remember_token(&token);
                        }
                    }
                }
            }
        }
    }
}
