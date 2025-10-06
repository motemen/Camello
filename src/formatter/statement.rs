use rowan::NodeOrToken;

use crate::{PerlNode, SyntaxKind};

use super::Formatter;

impl Formatter {
    pub(super) fn format_use_no_stmt(&mut self, node: &PerlNode) {
        // Output pending empty lines before processing use/no statement
        if self.pending_empty_lines > 0 {
            self.output_pending_empty_lines();
        }

        // Special handling for use/no statements: add space between identifier and parentheses
        // and between version and following expressions
        for child in node.children_with_tokens() {
            let is_module_name = match &child {
                NodeOrToken::Node(n) => n.kind() == crate::SyntaxKind::QUALIFIED_IDENT,
                NodeOrToken::Token(t) => t.kind() == crate::SyntaxKind::IDENT,
            };

            let is_version = match &child {
                NodeOrToken::Token(t) => {
                    matches!(
                        t.kind(),
                        crate::SyntaxKind::VERSION
                            | crate::SyntaxKind::BARE_VERSION
                            | crate::SyntaxKind::NUMBER
                    ) && t
                        .text()
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_digit() || c == 'v')
                }
                _ => false,
            };

            match &child {
                NodeOrToken::Node(n) => self.format_node(n),
                NodeOrToken::Token(t) => self.format_token(t),
            }

            if is_module_name {
                let last_token = match &child {
                    NodeOrToken::Node(n) => n.last_token(),
                    NodeOrToken::Token(t) => Some(t.clone()),
                };
                if let Some(last_token) = last_token {
                    if let Some(next_token) = Self::next_significant_token(&last_token) {
                        if next_token.kind() == crate::SyntaxKind::L_PAREN {
                            self.write_char(' ');
                        }
                    }
                }
            }

            // Add space after version if followed by an expression
            if is_version {
                let last_token = match &child {
                    NodeOrToken::Token(t) => Some(t.clone()),
                    _ => None,
                };
                if let Some(last_token) = last_token {
                    if let Some(next_token) = Self::next_significant_token(&last_token) {
                        if matches!(
                            next_token.kind(),
                            crate::SyntaxKind::IDENT
                                | crate::SyntaxKind::L_PAREN
                                | crate::SyntaxKind::QW_KW
                        ) {
                            self.write_char(' ');
                        }
                    }
                }
            }
        }
    }

    pub(super) fn format_simple_block(&mut self, node: &PerlNode) {
        let mut has_content = false;

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(_) => {
                    has_content = true;
                }
                NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::L_BRACE
                    | SyntaxKind::R_BRACE
                    | SyntaxKind::WHITESPACE
                    | SyntaxKind::NEWLINE => {}
                    _ => {
                        has_content = true;
                    }
                },
            }
        }

        let brace_tightness = self
            .options
            .delimiter_tightness
            .for_kind(SyntaxKind::L_BRACE);
        let add_space_for_block = brace_tightness.should_add_space_for_simple_block();

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => self.format_node(&child_node),
                NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::L_BRACE => {
                        self.handle_spacing_before(token.kind());
                        if self.at_line_start() {
                            self.add_indent();
                            self.set_at_line_start(false);
                        }
                        self.write(&token);
                        if add_space_for_block {
                            self.write_char(' ');
                        }
                        self.remember_token(&token);
                    }
                    SyntaxKind::R_BRACE => {
                        if add_space_for_block
                            && has_content
                            && self.prev_token_kind() != Some(SyntaxKind::L_BRACE)
                            && !self.current_line_ends_with_space()
                        {
                            self.write_char(' ');
                        }
                        self.write(&token);
                        self.remember_token(&token);
                    }
                    SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {}
                    _ => {
                        self.format_token(&token);
                    }
                },
            }
        }
    }

    pub(super) fn format_block(&mut self, node: &PerlNode) {
        if self.is_simple_block(node) {
            self.format_simple_block(node);
            return;
        }

        let mut children = node.children_with_tokens().peekable();
        let mut prev_node_kind: Option<SyntaxKind> = None;

        while let Some(child) = children.next() {
            match child {
                NodeOrToken::Node(child_node) => {
                    let current_kind = child_node.kind();

                    if let Some(prev_kind) = prev_node_kind {
                        if (prev_kind == SyntaxKind::USE_STMT || prev_kind == SyntaxKind::NO_STMT)
                            && (current_kind != SyntaxKind::USE_STMT
                                && current_kind != SyntaxKind::NO_STMT)
                        {
                            let has_existing_empty_line =
                                self.pending_empty_lines > 0 || self.ends_with_double_newline();

                            if !has_existing_empty_line && !self.is_output_empty() {
                                if !self.ends_with_newline() {
                                    self.handle_formatter_newline();
                                }
                                self.writer.push_empty_line();
                            }
                        }
                    }

                    self.output_pending_empty_lines();
                    self.format_node(&child_node);

                    prev_node_kind = Some(current_kind);
                }
                NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::NEWLINE => {
                        let mut saw_extra_newline = false;

                        while let Some(NodeOrToken::Token(peeked)) = children.peek() {
                            match peeked.kind() {
                                SyntaxKind::NEWLINE => {
                                    children.next();
                                    saw_extra_newline = true;
                                }
                                SyntaxKind::WHITESPACE => {
                                    children.next();
                                }
                                _ => break,
                            }
                        }

                        if !self.at_line_start() || !self.current_line_is_empty() {
                            self.handle_user_newline();
                        }

                        if saw_extra_newline || self.prev_token_kind() == Some(SyntaxKind::COMMENT)
                        {
                            self.pending_empty_lines = 1;
                        }
                    }
                    SyntaxKind::WHITESPACE => {
                        while let Some(NodeOrToken::Token(peeked)) = children.peek() {
                            if peeked.kind() == SyntaxKind::WHITESPACE {
                                children.next();
                            } else {
                                break;
                            }
                        }
                    }
                    _ => {
                        self.output_pending_empty_lines();
                        self.format_token(&token);
                    }
                },
            }
        }
    }

    pub(super) fn format_labeled_stmt(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens().peekable();

        if let Some(child) = children.next() {
            match child {
                NodeOrToken::Node(n) => self.format_node(&n),
                NodeOrToken::Token(t) => self.format_token(&t),
            }
        }

        if let Some(child) = children.peek() {
            match child {
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::NEWLINE => {
                    self.handle_user_newline();
                    children.next();
                }
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::WHITESPACE => {
                    self.write_char(' ');
                    self.set_at_line_start(false);
                    children.next();
                }
                NodeOrToken::Token(_) | NodeOrToken::Node(_) => {
                    self.write_char(' ');
                    self.set_at_line_start(false);
                }
            }
        }
        self.set_prev_token_kind(None);

        for child in children {
            match child {
                NodeOrToken::Node(n) => self.format_node(&n),
                NodeOrToken::Token(t) => self.format_token(&t),
            }
        }
    }

    pub(super) fn format_for_stmt(&mut self, node: &PerlNode) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::SEMICOLON => {
                        self.write(&token);
                        self.write_char(' ');
                    }
                    _ => {
                        self.format_token(&token);
                    }
                },
            }
        }
    }
}
