use crate::{PerlLanguage, PerlNode, SyntaxKind};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxToken};

use super::Formatter;

impl Formatter {
    pub(super) fn format_single_line_delimited_children(
        &mut self,
        node: &PerlNode,
        opening: SyntaxKind,
        closing: SyntaxKind,
        skip_whitespace: bool,
    ) {
        use SyntaxKind::WHITESPACE;

        let children: Vec<_> = node.children_with_tokens().collect();

        let mut stack: Vec<usize> = Vec::new();
        let mut pairs: Vec<(usize, usize)> = Vec::new();

        for (index, child) in children.iter().enumerate() {
            if let NodeOrToken::Token(token) = child {
                match token.kind() {
                    k if k == opening => stack.push(index),
                    k if k == closing => {
                        if let Some(open_index) = stack.pop() {
                            pairs.push((open_index, index));
                        }
                    }
                    _ => {}
                }
            }
        }

        if pairs.is_empty() {
            self.format_children(node, skip_whitespace);
            return;
        }

        let mut open_spacing: Vec<Option<bool>> = vec![None; children.len()];
        let mut close_spacing: Vec<Option<bool>> = vec![None; children.len()];

        for (open_index, close_index) in &pairs {
            if close_index <= open_index {
                continue;
            }

            let mut significant_tokens = 0;
            for child in &children[open_index + 1..*close_index] {
                match child {
                    NodeOrToken::Node(inner) => {
                        for element in inner.descendants_with_tokens() {
                            if let Some(token) = element.as_token() {
                                if !token.kind().is_trivia() {
                                    significant_tokens += 1;
                                    if significant_tokens >= 2 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    NodeOrToken::Token(token) => {
                        if !token.kind().is_trivia() {
                            significant_tokens += 1;
                        }
                    }
                }

                if significant_tokens >= 2 {
                    break;
                }
            }

            let tightness = self.options.delimiter_tightness.for_kind(opening);
            let add_interior_space = tightness.should_add_space(significant_tokens);
            open_spacing[*open_index] = Some(add_interior_space);
            close_spacing[*close_index] = Some(add_interior_space);
        }

        for (index, child) in children.into_iter().enumerate() {
            match child {
                NodeOrToken::Node(child_node) => self.format_node(&child_node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    if let Some(add_space) = open_spacing[index] {
                        self.handle_spacing_before(kind);
                        if self.at_line_start() {
                            self.add_indent();
                            self.set_at_line_start(false);
                        }
                        self.write(&token);
                        if add_space {
                            self.write_char(' ');
                        }
                        self.set_prev_token_kind(Some(kind));
                    } else if let Some(add_space) = close_spacing[index] {
                        if add_space && !self.current_line_ends_with_space() {
                            if self.at_line_start() {
                                self.add_indent();
                                self.set_at_line_start(false);
                            }
                            self.write_char(' ');
                        }
                        self.write(&token);
                        self.set_prev_token_kind(Some(kind));
                    } else if skip_whitespace && kind == WHITESPACE {
                        continue;
                    } else {
                        self.format_token(&token);
                    }
                }
            }
        }
    }

    pub(super) fn format_multiline_delimited(
        &mut self,
        node: &PerlNode,
        open_delimiter: SyntaxKind,
        close_delimiter: SyntaxKind,
    ) {
        self.format_multiline_delimited_iter(
            node.children_with_tokens(),
            open_delimiter,
            close_delimiter,
        );
    }

    pub(super) fn format_multiline_delimited_iter(
        &mut self,
        iter: SyntaxElementChildren<PerlLanguage>,
        open_delimiter: SyntaxKind,
        close_delimiter: SyntaxKind,
    ) {
        let old_multiline_context = self.in_multiline_context;
        self.in_multiline_context = true;
        for child in iter {
            match child {
                NodeOrToken::Node(node) => {
                    let kind = node.kind();

                    match kind {
                        SyntaxKind::EXPR_LIST => {
                            // Special handling for expression lists inside delimiters
                            self.format_expr_list_multiline_iter(node.children_with_tokens());
                        }
                        _ => self.format_node(&node),
                    }
                }
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    match kind {
                        SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {
                            // Skip trivia here - newlines handled in delimiter handlers
                        }
                        k if k == open_delimiter => {
                            self.handle_spacing_before(kind);
                            if self.at_line_start() {
                                self.add_indent();
                                self.set_at_line_start(false);
                            }
                            self.handle_multiline_opening_delimiter(&token);
                        }
                        k if k == close_delimiter => {
                            self.handle_multiline_closing_delimiter(&token);
                        }
                        _ => {
                            // その他のトークンは通常通り処理
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
        self.in_multiline_context = old_multiline_context;
    }

    fn format_expr_list_multiline_iter(&mut self, iter: SyntaxElementChildren<PerlLanguage>) {
        let old_multiline_context = self.in_multiline_context;
        self.in_multiline_context = true;
        for child in iter {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    match kind {
                        SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {
                            // Skip trivia here - newlines handled in the delimiter handlers
                        }
                        SyntaxKind::COMMA => {
                            self.format_token(&token);
                            self.handle_formatter_newline();
                        }
                        _ => {
                            // その他のトークンは通常通り処理
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
        self.in_multiline_context = old_multiline_context;
    }

    pub(super) fn handle_multiline_opening_delimiter(&mut self, token: &SyntaxToken<PerlLanguage>) {
        self.write(token);
        self.increase_indent();
        self.handle_formatter_newline();
        self.remember_token(token);
    }

    pub(super) fn handle_multiline_closing_delimiter(&mut self, token: &SyntaxToken<PerlLanguage>) {
        self.decrease_indent();
        if !self.at_line_start() || !self.current_line_is_empty() {
            self.handle_formatter_newline();
        }
        self.add_indent();
        self.write(token);
        self.set_at_line_start(false);
        self.remember_token(token);
    }
}
