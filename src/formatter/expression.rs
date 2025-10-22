use rowan::{NodeOrToken, SyntaxElementChildren};

use crate::{PerlLanguage, PerlNode, SyntaxKind, T};

use super::Formatter;

impl Formatter {
    pub(super) fn format_expr(&mut self, node: &PerlNode) {
        match node.kind() {
            SyntaxKind::ANON_SUB_EXPR => {
                self.format_anon_sub_expr(node);
            }
            SyntaxKind::FUNCTION_CALL_EXPR => {
                self.format_function_call_expr(node);
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
                if self.should_use_parenthesized_formatter(node) {
                    self.format_parenthesized_expr(node);
                } else {
                    self.format_children(node, false);
                }
            }
        }
    }

    /// Format function call expressions, handling special spacing for complex sigil expressions
    fn format_function_call_expr(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();

        // Check if the first child is a COMPOUND_VAR starting with CODE_SIGIL
        let first_child = children.by_ref().next();
        let is_complex_code_sigil = if let Some(NodeOrToken::Node(first_node)) = &first_child {
            first_node.kind() == SyntaxKind::COMPOUND_VAR
                && first_node.first_token().map(|t| t.kind()) == Some(SyntaxKind::CODE_SIGIL)
        } else {
            false
        };

        if is_complex_code_sigil {
            // Format complex code sigil expressions like &{$coderef}(1,2,3) without space before (
            if let Some(first_child) = first_child {
                match first_child {
                    NodeOrToken::Node(child_node) => self.format_node(&child_node),
                    NodeOrToken::Token(token) => self.format_token(&token),
                }
            }

            // Format the rest without adding spaces before L_PAREN
            for child in children {
                match child {
                    NodeOrToken::Node(child_node) => self.format_node(&child_node),
                    NodeOrToken::Token(token) => {
                        if token.kind() == T!['('] {
                            // Force no space before this L_PAREN by writing it directly
                            self.writer.write_str("(", Some(T!['(']), None);
                            self.remember_token(&token);
                        } else {
                            self.format_token(&token);
                        }
                    }
                }
            }
        } else {
            // For regular function calls, use the default parenthesized formatter logic
            if self.should_use_parenthesized_formatter(node) {
                self.format_parenthesized_expr(node);
            } else {
                self.format_children(node, false);
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
            self.format_multiline_delimited_elements(node.children_with_tokens(), T!['('], T![')']);
        } else {
            // Use single-line formatting with contextual spacing for compact expressions
            self.format_single_line_delimited_children(node, T!['('], T![')'], true);
        }
    }

    pub(super) fn format_block_function_call(&mut self, node: &PerlNode) {
        // Format block function call: function_name { ... } additional_args
        // Use single-line for simple blocks (single statement, no semicolon)
        // Use multi-line for complex blocks

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    if child_node.kind() == SyntaxKind::BLOCK_STMT {
                        if self.is_simple_block(&child_node) {
                            self.format_simple_block(&child_node);
                        } else {
                            // Use format_node for complex blocks to preserve empty lines
                            self.format_node(&child_node);
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
        self.format_subscription_iter(children, T!['('], T![')']);
    }

    fn format_until_arrow_iter(&mut self, iter: &mut SyntaxElementChildren<PerlLanguage>) {
        for child in iter {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) if token.kind() == SyntaxKind::WHITESPACE => {}
                NodeOrToken::Token(token) => {
                    // Use format_token to ensure proper spacing is applied
                    self.format_token(&token);

                    if token.kind() == T![->] {
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
            self.format_multiline_delimited_elements(iter, opening, closing);
        } else {
            let elements: Vec<_> = iter.collect();
            self.format_single_line_delimited_elements(elements, opening, closing, true);
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
            T!['{']
        } else {
            T!['[']
        };
        let mut closing = if sigil_kind == SyntaxKind::HASH_SIGIL {
            T!['}']
        } else {
            T![']']
        };

        for element in children.clone() {
            match element {
                NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::COMMENT => continue,
                    T!['{'] => {
                        opening = T!['{'];
                        closing = T!['}'];
                        break;
                    }
                    T!['['] => {
                        opening = T!['['];
                        closing = T![']'];
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
        self.format_ref_access_expr(node, T!['{'], T!['}']);
    }

    pub(super) fn format_array_ref_access(&mut self, node: &PerlNode) {
        self.format_ref_access_expr(node, T!['['], T![']']);
    }

    pub(super) fn format_code_ref_call(&mut self, node: &PerlNode) {
        self.format_ref_access_expr(node, T!['('], T![')']);
    }

    pub(super) fn format_compound_var(&mut self, node: &PerlNode) {
        // Handle compound variables like @{expr}, %$var, $#array
        // For braced expressions, default to compact formatting unless the braces contain
        // statement blocks (e.g., @{ my $x = 1; $x }) in which case we format as a block.
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    if child_node.kind() == SyntaxKind::BLOCK_STMT {
                        if self.is_simple_block(&child_node) {
                            self.format_compound_var_simple_block(&child_node);
                        } else if child_node
                            .children()
                            .any(|grandchild| grandchild.kind() == SyntaxKind::STMT)
                        {
                            self.format_multiline_delimited_elements(
                                child_node.children_with_tokens(),
                                T!['{'],
                                T!['}'],
                            );
                        } else {
                            self.format_node(&child_node);
                        }
                    } else {
                        self.format_node(&child_node);
                    }
                }
                NodeOrToken::Token(token) => {
                    match token.kind() {
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace to ensure compact formatting.
                        }
                        T!['{'] | T!['}'] => {
                            // Format braces without extra spacing or newlines
                            self.writer.write_token(&token);
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

    fn format_compound_var_simple_block(&mut self, node: &PerlNode) {
        // Use context with suppress_newlines enabled for simple blocks
        let ctx = super::FormatContext::default().with_suppress_newlines();
        self.format_single_line_delimited_children_with_context(node, T!['{'], T!['}'], true, ctx);
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
        self.format_subscription_expr(node, T!['{'], T!['}']);
    }

    pub(super) fn format_array_subscription(&mut self, node: &PerlNode) {
        self.format_subscription_expr(node, T!['['], T![']']);
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
                        T!['('] => {
                            if self.writer.at_line_start() {
                                self.writer.add_indent();
                                self.writer.set_at_line_start(false);
                            }

                            // Subroutine prototypes always get a space before opening paren
                            self.writer.write_char(' ');
                            self.writer.write_token(&token);
                            self.remember_token(&token);
                        }
                        _ => {
                            // R_PAREN and prototype symbols: no spacing, just output them directly
                            self.writer.write_token(&token);
                            self.remember_token(&token);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn format_sub_signature(&mut self, node: &PerlNode) {
        use SyntaxKind::{NEWLINE, WHITESPACE};

        let ctx = super::FormatContext::default().with_multiline_context();

        if self.has_newline_before_first_value(node) {
            for child in node.children_with_tokens() {
                match child {
                    NodeOrToken::Node(child_node) => {
                        self.format_node_with_context(&child_node, ctx);
                    }
                    NodeOrToken::Token(token) => {
                        let kind = token.kind();
                        match kind {
                            WHITESPACE | NEWLINE => {
                                // Skip trivia
                            }
                            T!['('] => {
                                // Add space before opening paren for signature style: sub foo ($x)
                                if !self.writer.at_line_start()
                                    && !self.writer.current_line_ends_with_space()
                                {
                                    self.writer.write_char(' ');
                                }
                                self.handle_multiline_opening_delimiter(&token);
                            }
                            T![')'] => {
                                self.handle_multiline_closing_delimiter(&token);
                            }
                            T![,] => {
                                self.format_token_with_context(&token, ctx);
                                self.writer.handle_formatter_newline();
                            }
                            _ => {
                                self.format_token_with_context(&token, ctx);
                            }
                        }
                    }
                }
            }
        } else {
            // Single-line formatting
            for child in node.children_with_tokens() {
                match child {
                    NodeOrToken::Node(child_node) => {
                        self.format_node(&child_node);
                    }
                    NodeOrToken::Token(token) => {
                        let kind = token.kind();
                        match kind {
                            WHITESPACE => {
                                // Skip whitespace
                            }
                            T!['('] => {
                                // Add space before opening paren for signature style: sub foo ($x)
                                if !self.writer.at_line_start()
                                    && !self.writer.current_line_ends_with_space()
                                {
                                    self.writer.write_char(' ');
                                }
                                self.writer.write_token(&token);
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

    pub(super) fn format_signature_default(&mut self, node: &PerlNode) {
        use SyntaxKind::{DEFINED_OR, LOGICAL_OR, WHITESPACE};

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => self.format_node(&child_node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    match kind {
                        WHITESPACE => {
                            // Skip whitespace - spacing will be added by the rules below
                        }
                        T![=] => {
                            // Always add space before = in signature defaults
                            // This handles cases like "$ = 1" where $ is a placeholder
                            if !self.writer.current_line_ends_with_space()
                                && !matches!(
                                    self.writer.prev_token_kind(),
                                    Some(DEFINED_OR | LOGICAL_OR)
                                )
                            {
                                self.writer.write_char(' ');
                            }
                            // Write token directly without calling format_token
                            // to avoid double spacing from handle_spacing_before
                            self.writer.write_token(&token);
                            self.remember_token(&token);
                        }
                        DEFINED_OR | LOGICAL_OR => {
                            // Add space before // and || in signature defaults
                            if !self.writer.current_line_ends_with_space() {
                                self.writer.write_char(' ');
                            }
                            // Write token directly without calling format_token
                            // to avoid double spacing from handle_spacing_before
                            self.writer.write_token(&token);
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
