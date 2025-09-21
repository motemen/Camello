use rowan::{NodeOrToken, SyntaxElementChildren};

use crate::{PerlLanguage, PerlNode, SyntaxKind};

use super::Formatter;

impl Formatter {
    pub(super) fn format_expr(&mut self, node: &PerlNode) {
        match node.kind() {
            SyntaxKind::ANON_SUB_EXPR => {
                self.format_anon_sub_expr(node);
            }
            SyntaxKind::TYPEGLOB_EXPR => {
                self.format_typeglob_expr(node);
            }
            SyntaxKind::BLOCK_FUNCTION_CALL_EXPR => {
                self.format_block_function_call(node);
            }
            SyntaxKind::METHOD_CALL_EXPR => {
                self.format_method_call(node);
            }
            SyntaxKind::HASH_REF_ACCESS_EXPR => {
                self.format_hash_ref_access(node);
            }
            SyntaxKind::ARRAY_REF_ACCESS_EXPR => {
                self.format_array_ref_access(node);
            }
            SyntaxKind::POSTFIX_ARRAY_SLICE_EXPR => {
                self.format_postfix_slice_expr(node, SyntaxKind::ARRAY_SIGIL);
            }
            SyntaxKind::POSTFIX_HASH_SLICE_EXPR => {
                self.format_postfix_slice_expr(node, SyntaxKind::HASH_SIGIL);
            }
            SyntaxKind::CODE_REF_CALL_EXPR => {
                self.format_code_ref_call(node);
            }
            SyntaxKind::HASH_SUBSCRIPTION_EXPR => {
                self.format_hash_subscription(node);
            }
            SyntaxKind::ARRAY_SUBSCRIPTION_EXPR => {
                self.format_array_subscription(node);
            }
            SyntaxKind::COMPOUND_VAR => {
                self.format_compound_var(node);
            }
            SyntaxKind::REGEX_EXPR => {
                // Default handling for regex expressions - just format children
                // The spacing around regex operators is handled in format_token
                self.format_children(node, false);
            }
            SyntaxKind::BACKTICK_EXPR => {
                // Backtick command substitution: just format children (the backtick string literal)
                self.format_children(node, false);
            }
            _ => {
                // Check if this node contains parentheses that should be formatted multiline
                if self.should_format_parentheses_multiline(node) {
                    self.format_parenthesized_expr(node);
                } else {
                    self.format_children(node, false);
                }
            }
        }
    }

    pub(super) fn format_anon_sub_expr(&mut self, node: &PerlNode) {
        // Format anonymous subroutine: sub { ... }
        // Use K&R style like regular subroutines: space before opening brace

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    match child_node.kind() {
                        SyntaxKind::BLOCK_STMT => {
                            // Check if this is a simple block that can be formatted inline
                            if self.is_simple_block(&child_node) {
                                self.format_simple_block(&child_node);
                            } else {
                                self.format_node(&child_node);
                            }
                        }
                        _ => {
                            self.format_node(&child_node);
                        }
                    }
                }
                NodeOrToken::Token(token) => {
                    self.format_token(&token);
                }
            }
        }
    }
    pub(super) fn format_parenthesized_expr(&mut self, node: &PerlNode) {
        // Check if the parenthesized expression contains newlines
        if self.has_newline_before_first_value(node) {
            // Use multiline formatting for expressions with newlines
            self.format_multiline_delimited(node, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
        } else {
            // Use single-line formatting with contextual spacing for compact expressions
            self.format_single_line_delimited_children(
                node,
                SyntaxKind::L_PAREN,
                SyntaxKind::R_PAREN,
                true,
            );
        }
    }

    pub(super) fn format_block_function_call(&mut self, node: &PerlNode) {
        // Format block function call: function_name { ... } additional_args
        // Use single-line for simple blocks (single statement, no semicolon)
        // Use multi-line for complex blocks

        // Pre-calculate which blocks are simple to avoid repeated checks
        let simple_block_ranges: std::collections::HashSet<_> = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::BLOCK_STMT && self.is_simple_block(child))
            .map(|child| child.text_range())
            .collect();

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    if child_node.kind() == SyntaxKind::BLOCK_STMT {
                        if simple_block_ranges.contains(&child_node.text_range()) {
                            self.format_simple_block(&child_node);
                        } else {
                            // Consistently use multiline formatting for complex blocks
                            self.format_multiline_delimited(
                                &child_node,
                                SyntaxKind::L_BRACE,
                                SyntaxKind::R_BRACE,
                            );
                        }
                    } else {
                        self.format_node(&child_node);
                    }
                }
                NodeOrToken::Token(token) => {
                    self.format_token(&token);
                }
            }
        }
    }

    pub(super) fn format_method_call(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        // Format the method name part
        for child in children.by_ref() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::WHITESPACE {
                        continue;
                    }
                    self.format_token(&token);
                }
            }
            break;
        }
        // Use multiline formatting for the parenthesized arguments
        self.format_subscription_iter(children, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    fn format_until_arrow_iter(&mut self, iter: &mut SyntaxElementChildren<PerlLanguage>) {
        for child in iter {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) if token.kind() == SyntaxKind::WHITESPACE => {}
                NodeOrToken::Token(token) => {
                    // Use format_token to ensure proper spacing is applied
                    self.format_token(&token);

                    if token.kind() == SyntaxKind::ARROW {
                        break;
                    }
                }
            }
        }
    }

    /// formats @array, %hash or its ref's [ ... ] or { ... } part
    fn format_subscription_iter(
        &mut self,
        iter: SyntaxElementChildren<PerlLanguage>,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) {
        if self.has_newline_before_first_value_iter(iter.clone()) {
            self.format_multiline_delimited_iter(iter, opening, closing);
        } else {
            for child in iter {
                match child {
                    NodeOrToken::Node(node) => self.format_node(&node),
                    NodeOrToken::Token(token) => {
                        let kind = token.kind();

                        match kind {
                            _ if kind == opening || kind == closing => {
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
    }

    fn format_ref_access_expr(
        &mut self,
        node: &PerlNode,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        self.format_subscription_iter(children, opening, closing);
    }

    fn format_postfix_slice_expr(&mut self, node: &PerlNode, sigil_kind: SyntaxKind) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());

        for child in children.by_ref() {
            match child {
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::WHITESPACE {
                        continue;
                    }
                    self.format_token(&token);
                }
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node);
                }
            }
            break;
        }

        let mut opening = if sigil_kind == SyntaxKind::HASH_SIGIL {
            SyntaxKind::L_BRACE
        } else {
            SyntaxKind::L_BRACKET
        };
        let mut closing = if sigil_kind == SyntaxKind::HASH_SIGIL {
            SyntaxKind::R_BRACE
        } else {
            SyntaxKind::R_BRACKET
        };

        for element in children.clone() {
            match element {
                NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::COMMENT => continue,
                    SyntaxKind::L_BRACE => {
                        opening = SyntaxKind::L_BRACE;
                        closing = SyntaxKind::R_BRACE;
                        break;
                    }
                    SyntaxKind::L_BRACKET => {
                        opening = SyntaxKind::L_BRACKET;
                        closing = SyntaxKind::R_BRACKET;
                        break;
                    }
                    _ => break,
                },
                NodeOrToken::Node(_) => break,
            }
        }

        self.format_subscription_iter(children, opening, closing);
    }

    pub(super) fn format_hash_ref_access(&mut self, node: &PerlNode) {
        self.format_ref_access_expr(node, SyntaxKind::L_BRACE, SyntaxKind::R_BRACE);
    }

    pub(super) fn format_array_ref_access(&mut self, node: &PerlNode) {
        self.format_ref_access_expr(node, SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET);
    }

    pub(super) fn format_code_ref_call(&mut self, node: &PerlNode) {
        self.format_ref_access_expr(node, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    pub(super) fn format_compound_var(&mut self, node: &PerlNode) {
        // Handle compound variables like @{expr}, %$var, $#array
        // For braced expressions, format them compactly without newlines or indentation
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => {
                    match token.kind() {
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace to ensure compact formatting.
                        }
                        SyntaxKind::L_BRACE | SyntaxKind::R_BRACE => {
                            // Format braces without extra spacing or newlines
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

    fn format_subscription_expr(
        &mut self,
        node: &PerlNode,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) {
        let children = node.children_with_tokens();
        self.format_subscription_iter(children, opening, closing);
    }

    pub(super) fn format_hash_subscription(&mut self, node: &PerlNode) {
        self.format_subscription_expr(node, SyntaxKind::L_BRACE, SyntaxKind::R_BRACE);
    }

    pub(super) fn format_array_subscription(&mut self, node: &PerlNode) {
        self.format_subscription_expr(node, SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET);
    }

    pub(super) fn format_typeglob_expr(&mut self, node: &PerlNode) {
        // Format typeglob expressions (e.g., *{$name}, *STDIN)
        // Keep braces compact - no multiline formatting
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace inside typeglob expressions to keep them compact
                        }
                        SyntaxKind::L_BRACE | SyntaxKind::R_BRACE => {
                            // Handle braces directly without spacing - keep typeglobs compact
                            self.write(&token);
                            self.remember_token(&token);
                        }
                        _ => {
                            // For other tokens (asterisk, identifiers, variables, etc.), apply normal formatting
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn format_sub_prototype(&mut self, node: &PerlNode) {
        // Format subroutine prototype: no spaces between parentheses and prototype symbols
        // Example: (@@), ($@), (\@$$@), etc.
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => {
                    match token.kind() {
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace inside prototypes to ensure compact formatting
                        }
                        SyntaxKind::L_PAREN => {
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }

                            // Subroutine prototypes always get a space before opening paren
                            self.write_char(' ');
                            self.write(&token);
                            self.remember_token(&token);
                        }
                        _ => {
                            // R_PAREN and prototype symbols: no spacing, just output them directly
                            self.write(&token);
                            self.remember_token(&token);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn format_children(&mut self, node: &PerlNode, skip_whitespace: bool) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    if skip_whitespace && kind == SyntaxKind::WHITESPACE {
                        // Skip whitespace if the flag is set
                        continue;
                    }

                    self.format_token(&token);
                }
            }
        }
    }
}
