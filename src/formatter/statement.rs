use std::iter::Peekable;

use rowan::{NodeOrToken, SyntaxElementChildren};

use crate::{PerlLanguage, PerlNode, SyntaxKind, T};

use super::{AlignmentState, AlignmentStrategy, Formatter};

impl Formatter {
    pub(super) fn format_use_no_stmt(&mut self, node: &PerlNode) {
        // Output pending empty lines before processing use/no statement
        if self.pending_empty_lines > 0 {
            self.output_pending_empty_lines();
        }

        // Special handling for use/no statements: add space between identifier and parentheses
        // and between version and following expressions
        let mut children = node.children_with_tokens();

        while let Some(child) = children.next() {
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

            // Check for multiline parentheses
            if let NodeOrToken::Token(token) = &child {
                if self.try_format_multiline_parens(
                    token,
                    &mut children,
                    super::FormatContext::default(),
                ) {
                    continue;
                }
            }

            match &child {
                NodeOrToken::Node(n) => self.format_node(n, super::FormatContext::default()),
                NodeOrToken::Token(t) => self.format_token(t, super::FormatContext::default()),
            }

            if is_module_name {
                let last_token = match &child {
                    NodeOrToken::Node(n) => n.last_token(),
                    NodeOrToken::Token(t) => Some(t.clone()),
                };
                if let Some(last_token) = last_token {
                    if let Some(next_token) = Self::next_significant_token(&last_token) {
                        if next_token.kind() == T!['('] {
                            self.writer.write_char(' ');
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
                            crate::SyntaxKind::IDENT | T!['('] | T![qw]
                        ) {
                            self.writer.write_char(' ');
                        }
                    }
                }
            }
        }
    }

    pub(super) fn format_simple_block(&mut self, node: &PerlNode) {
        let brace_tightness = self.options.delimiter_tightness.for_kind(T!['{']);
        let add_space_for_block = brace_tightness.should_add_space_for_simple_block();

        // Use context with suppress_newlines enabled for simple blocks
        let ctx = super::FormatContext::default().with_suppress_newlines();

        let mut has_content = false;

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    has_content = true;
                    self.format_node(&child_node, ctx);
                }
                NodeOrToken::Token(token) => match token.kind() {
                    T!['{'] => {
                        self.handle_spacing_before(token.kind());
                        if self.writer.at_line_start() {
                            self.writer.add_indent();
                            self.writer.set_at_line_start(false);
                        }
                        self.writer.write_token(&token);
                        if add_space_for_block {
                            self.writer.write_char(' ');
                        }
                        self.remember_token(&token);
                    }
                    T!['}'] => {
                        if add_space_for_block
                            && has_content
                            && self.writer.prev_token_kind() != Some(T!['{'])
                            && !self.writer.current_line_ends_with_space()
                        {
                            self.writer.write_char(' ');
                        }
                        self.writer.write_token(&token);
                        self.remember_token(&token);
                    }
                    SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {}
                    _ => {
                        has_content = true;
                        self.format_token(&token, ctx);
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
                            let has_existing_empty_line = self.pending_empty_lines > 0
                                || self.writer.ends_with_double_newline();

                            if !has_existing_empty_line && !self.writer.is_output_empty() {
                                if !self.writer.ends_with_newline() {
                                    self.writer.handle_formatter_newline();
                                }
                                self.writer.push_empty_line();
                            }
                        }
                    }

                    if self.alignment_state.is_none() {
                        if let Some(state) = self.collect_alignment_group(&child_node, &children) {
                            self.alignment_state = Some(state);
                        }
                    }

                    self.output_pending_empty_lines();
                    self.format_node(&child_node, super::FormatContext::default());

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

                        if !self.writer.at_line_start() || !self.writer.current_line_is_empty() {
                            self.writer.handle_user_newline();
                        }

                        if saw_extra_newline {
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
                        self.format_token(&token, super::FormatContext::default());
                    }
                },
            }
        }
    }

    pub(super) fn format_labeled_stmt(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens().peekable();

        if let Some(child) = children.next() {
            match child {
                NodeOrToken::Node(n) => self.format_node(&n, super::FormatContext::default()),
                NodeOrToken::Token(t) => self.format_token(&t, super::FormatContext::default()),
            }
        }

        if let Some(child) = children.peek() {
            match child {
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::NEWLINE => {
                    self.writer.handle_user_newline();
                    children.next();
                }
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::WHITESPACE => {
                    self.writer.write_char(' ');
                    self.writer.set_at_line_start(false);
                    children.next();
                }
                NodeOrToken::Token(_) | NodeOrToken::Node(_) => {
                    self.writer.write_char(' ');
                    self.writer.set_at_line_start(false);
                }
            }
        }
        self.writer.set_prev_token_kind(None);

        for child in children {
            match child {
                NodeOrToken::Node(n) => self.format_node(&n, super::FormatContext::default()),
                NodeOrToken::Token(t) => self.format_token(&t, super::FormatContext::default()),
            }
        }
    }

    pub(super) fn format_for_stmt(&mut self, node: &PerlNode) {
        // Special handling for C-style for loops: add space after semicolons
        let mut children = node.children_with_tokens();

        while let Some(child) = children.next() {
            // Check for multiline parentheses
            if let NodeOrToken::Token(token) = &child {
                if self.try_format_multiline_parens(
                    token,
                    &mut children,
                    super::FormatContext::default(),
                ) {
                    continue;
                }
            }

            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node, super::FormatContext::default());
                }
                NodeOrToken::Token(token) => match token.kind() {
                    T![;] => {
                        // C-style for loop: add space after semicolon
                        self.writer.write_token(&token);
                        self.writer.write_char(' ');
                    }
                    _ => {
                        self.format_token(&token, super::FormatContext::default());
                    }
                },
            }
        }
    }

    fn collect_alignment_group(
        &self,
        first_node: &PerlNode,
        iter: &Peekable<SyntaxElementChildren<PerlLanguage>>,
    ) -> Option<AlignmentState> {
        for &strategy in &self.options.alignment_strategies {
            if let Some(state) =
                self.collect_alignment_group_for_strategy(strategy, first_node, iter)
            {
                return Some(state);
            }
        }

        None
    }

    fn collect_alignment_group_for_strategy(
        &self,
        strategy: AlignmentStrategy,
        first_node: &PerlNode,
        iter: &Peekable<SyntaxElementChildren<PerlLanguage>>,
    ) -> Option<AlignmentState> {
        let token_kind = self.alignment_token_kind_for_node(strategy, first_node)?;

        // For assignment alignment, get the assignment type of the first node
        let first_assignment_type = if strategy == AlignmentStrategy::Assignments {
            Self::get_assignment_type(first_node)
        } else {
            None
        };

        let mut nodes = vec![first_node.clone()];
        let lookahead = iter.clone();
        let mut saw_newline = false;
        let mut prev_node_is_multiline = self.node_spans_multiple_lines(first_node);

        for element in lookahead {
            match element {
                NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::WHITESPACE => continue,
                    SyntaxKind::NEWLINE => {
                        if saw_newline {
                            break;
                        }
                        saw_newline = true;
                    }
                    SyntaxKind::COMMENT => {
                        if !saw_newline {
                            saw_newline = true;
                            continue;
                        }
                        break;
                    }
                    _ => break,
                },
                NodeOrToken::Node(node) => {
                    if !saw_newline {
                        break;
                    }

                    // Stop collecting if the previous node was multi-line
                    if prev_node_is_multiline {
                        break;
                    }

                    if self.alignment_token_kind_for_node(strategy, &node) != Some(token_kind) {
                        break;
                    }

                    // For assignment alignment, check that the assignment type matches
                    if strategy == AlignmentStrategy::Assignments
                        && Self::get_assignment_type(&node) != first_assignment_type
                    {
                        break;
                    }

                    prev_node_is_multiline = self.node_spans_multiple_lines(&node);
                    nodes.push(node);
                    saw_newline = false;
                }
            }
        }

        if nodes.len() < 2 {
            return None;
        }

        self.build_alignment_state(&nodes, token_kind)
    }

    fn build_alignment_state(
        &self,
        nodes: &[PerlNode],
        token_kind: SyntaxKind,
    ) -> Option<AlignmentState> {
        let mut widths = Vec::with_capacity(nodes.len());
        for node in nodes {
            let width = self.measure_alignment_prefix(node, token_kind)?;
            widths.push(width);
        }

        let max_width = widths.iter().copied().max()?;
        if widths.iter().all(|&width| width == max_width) {
            return None;
        }

        let pads = widths
            .into_iter()
            .map(|width| max_width - width)
            .collect::<Vec<_>>();

        Some(AlignmentState::new(token_kind, pads))
    }

    fn node_spans_multiple_lines(&self, node: &PerlNode) -> bool {
        // Check if a node contains newlines in its source representation
        node.descendants_with_tokens().any(|element| {
            element
                .as_token()
                .is_some_and(|token| token.kind() == SyntaxKind::NEWLINE)
        })
    }

    fn measure_alignment_prefix(&self, node: &PerlNode, token_kind: SyntaxKind) -> Option<usize> {
        // Create a temporary formatter to measure the prefix width.
        // This is efficient: trivia_table.clone() only increments the Rc refcount,
        // and options.clone() is lightweight. Full formatting is necessary to accurately
        // measure the width including proper spacing and token formatting.
        let mut options = self.options.clone();
        options.alignment_strategies.clear();
        let mut formatter =
            Formatter::with_shared_deps(self.trivia_table.clone(), options, self.root.clone());
        formatter.format_node(node, super::FormatContext::default());

        let columns = if token_kind == SyntaxKind::EQ {
            formatter.writer.collect_assignment_columns()
        } else {
            formatter.writer.collect_token_columns(token_kind)
        };
        if columns.len() != 1 {
            return None;
        }

        columns.into_iter().next().map(|column| column.column)
    }

    fn alignment_token_kind_for_node(
        &self,
        strategy: AlignmentStrategy,
        node: &PerlNode,
    ) -> Option<SyntaxKind> {
        match strategy {
            AlignmentStrategy::Assignments => {
                if !matches!(node.kind(), SyntaxKind::VAR_DECL | SyntaxKind::STMT) {
                    return None;
                }

                if !self.options.align_compound_assignments
                    && node
                        .descendants()
                        .any(|child| child.kind() == SyntaxKind::COMPOUND_ASSIGNMENT)
                {
                    return None;
                }

                let mut seen_assignment = false;
                for token in node.descendants_with_tokens().filter_map(|element| {
                    if let NodeOrToken::Token(token) = element {
                        Some(token)
                    } else {
                        None
                    }
                }) {
                    if token.kind().is_assignment_operator() {
                        if seen_assignment {
                            return None;
                        }
                        seen_assignment = true;
                    }
                }

                if seen_assignment {
                    Some(SyntaxKind::EQ)
                } else {
                    None
                }
            }
            AlignmentStrategy::FatCommas => {
                if matches!(
                    node.kind(),
                    SyntaxKind::VAR_DECL
                        | SyntaxKind::STMT
                        | SyntaxKind::USE_STMT
                        | SyntaxKind::NO_STMT
                ) && Self::count_tokens_of_kind(node, SyntaxKind::FAT_COMMA) == 1
                {
                    Some(SyntaxKind::FAT_COMMA)
                } else {
                    None
                }
            }
            AlignmentStrategy::PostfixConditionals => {
                if node.kind() != SyntaxKind::STMT {
                    return None;
                }

                if Self::has_child_of_kind(node, SyntaxKind::IF_MODIFIER)
                    && Self::count_tokens_of_kind(node, SyntaxKind::IF_KW) == 1
                {
                    return Some(SyntaxKind::IF_KW);
                }

                if Self::has_child_of_kind(node, SyntaxKind::UNLESS_MODIFIER)
                    && Self::count_tokens_of_kind(node, SyntaxKind::UNLESS_KW) == 1
                {
                    return Some(SyntaxKind::UNLESS_KW);
                }

                None
            }
            AlignmentStrategy::Comments => {
                if !matches!(node.kind(), SyntaxKind::VAR_DECL | SyntaxKind::STMT) {
                    return None;
                }

                if self.node_has_trailing_comment(node) {
                    Some(SyntaxKind::COMMENT)
                } else {
                    None
                }
            }
        }
    }

    fn has_child_of_kind(node: &PerlNode, kind: SyntaxKind) -> bool {
        node.children().any(|child| child.kind() == kind)
    }

    fn count_tokens_of_kind(node: &PerlNode, kind: SyntaxKind) -> usize {
        node.descendants_with_tokens()
            .filter(|element| match element {
                NodeOrToken::Token(token) => token.kind() == kind,
                NodeOrToken::Node(_) => false,
            })
            .count()
    }

    /// Determines the assignment type for alignment purposes.
    /// Returns a tuple: (is_var_decl, is_list_assignment)
    fn get_assignment_type(node: &PerlNode) -> Option<(bool, bool)> {
        let is_var_decl = if node.kind() == SyntaxKind::VAR_DECL {
            true
        } else if node.kind() == SyntaxKind::STMT {
            // Check if the STMT contains a VAR_DECL
            node.children()
                .any(|child| child.kind() == SyntaxKind::VAR_DECL)
        } else {
            return None;
        };

        // Find the INFIX_EXPR to check if it's a list assignment
        let infix_expr = node
            .descendants()
            .find(|n| n.kind() == SyntaxKind::INFIX_EXPR)?;

        // Check if the left side is a list assignment
        // For list assignments like `my ($a, $b) = @_;`, the INFIX_EXPR starts with L_PAREN
        // For scalar assignments like `my $z = 10;`, the INFIX_EXPR starts with a variable
        let is_list = infix_expr
            .children_with_tokens()
            .find(|e| !e.kind().is_trivia())
            .is_some_and(|e| e.kind() == T!['(']);

        Some((is_var_decl, is_list))
    }
}
