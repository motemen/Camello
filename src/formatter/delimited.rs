use crate::{PerlLanguage, PerlNode, SyntaxKind, T};
use rowan::{NodeOrToken, SyntaxElement, SyntaxElementChildren, SyntaxToken};

use super::{AlignmentState, AlignmentStrategy, FormatContext, Formatter};

impl Formatter {
    pub(super) fn format_single_line_delimited_children(
        &mut self,
        node: &PerlNode,
        opening: SyntaxKind,
        closing: SyntaxKind,
        skip_whitespace: bool,
    ) {
        self.format_single_line_delimited_children_with_context(
            node,
            opening,
            closing,
            skip_whitespace,
            FormatContext::default(),
        );
    }

    pub(super) fn format_single_line_delimited_children_with_context(
        &mut self,
        node: &PerlNode,
        opening: SyntaxKind,
        closing: SyntaxKind,
        skip_whitespace: bool,
        ctx: FormatContext,
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
            if ctx.suppress_newlines {
                for child in node.children_with_tokens() {
                    match child {
                        NodeOrToken::Node(child_node) => {
                            self.format_node_with_context(&child_node, ctx);
                        }
                        NodeOrToken::Token(token) => {
                            if skip_whitespace && token.kind() == WHITESPACE {
                                continue;
                            }
                            self.format_token_with_context(&token, ctx);
                        }
                    }
                }
            } else {
                self.format_children(node, skip_whitespace);
            }
            return;
        }

        let mut open_spacing: Vec<Option<bool>> = vec![None; children.len()];
        let mut close_spacing: Vec<Option<bool>> = vec![None; children.len()];

        for (open_index, close_index) in &pairs {
            if close_index <= open_index {
                continue;
            }

            let mut significant_tokens = 0;
            let mut contains_qw = false;

            for child in &children[open_index + 1..*close_index] {
                match child {
                    NodeOrToken::Node(inner) => {
                        let remaining = 2usize.saturating_sub(significant_tokens);
                        if remaining == 0 {
                            break;
                        }
                        let (count, has_qw) =
                            Self::count_significant_tokens_in_node(inner, remaining);
                        significant_tokens += count;
                        contains_qw |= has_qw;
                    }
                    NodeOrToken::Token(token) => {
                        if !token.kind().is_trivia() {
                            significant_tokens += 1;
                            if matches!(token.kind(), SyntaxKind::QW_STRING | SyntaxKind::QW_KW) {
                                contains_qw = true;
                            }
                        }
                    }
                }

                if significant_tokens >= 2 {
                    break;
                }
            }

            let tightness = self.options.delimiter_tightness.for_kind(opening);
            let mut add_interior_space = tightness.should_add_space(significant_tokens);
            if contains_qw {
                add_interior_space = true;
            }
            open_spacing[*open_index] = Some(add_interior_space);
            close_spacing[*close_index] = Some(add_interior_space);
        }

        for (index, child) in children.into_iter().enumerate() {
            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node_with_context(&child_node, ctx);
                }
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    if let Some(add_space) = open_spacing[index] {
                        self.handle_spacing_before(kind);
                        if self.writer.at_line_start() {
                            self.writer.add_indent();
                            self.writer.set_at_line_start(false);
                        }
                        self.writer.write_token(&token);
                        if add_space {
                            self.writer.write_char(' ');
                        }
                        self.writer.set_prev_token_kind(Some(kind));
                    } else if let Some(add_space) = close_spacing[index] {
                        if add_space && !self.writer.current_line_ends_with_space() {
                            if self.writer.at_line_start() {
                                self.writer.add_indent();
                                self.writer.set_at_line_start(false);
                            }
                            self.writer.write_char(' ');
                        }
                        self.writer.write_token(&token);
                        self.writer.set_prev_token_kind(Some(kind));
                    } else if skip_whitespace && kind == WHITESPACE {
                        continue;
                    } else {
                        self.format_token_with_context(&token, ctx);
                    }
                }
            }
        }
    }

    fn count_significant_tokens_in_node(node: &PerlNode, remaining: usize) -> (usize, bool) {
        use SyntaxKind::{
            ARRAY_VAR, HASH_VAR, QW_EXPR, QW_KW, QW_STRING, SCALAR_VAR, TYPEGLOB_VAR,
        };

        if remaining == 0 {
            return (0, false);
        }

        match node.kind() {
            SCALAR_VAR | ARRAY_VAR | HASH_VAR | TYPEGLOB_VAR => (1, false),
            _ => {
                let is_qw_expr = node.kind() == QW_EXPR;
                let mut count = 0;
                let mut contains_qw = is_qw_expr;

                for child in node.children_with_tokens() {
                    let rem = remaining.saturating_sub(count);
                    if rem == 0 {
                        break;
                    }

                    match child {
                        NodeOrToken::Node(inner) => {
                            let (sub_count, sub_qw) =
                                Self::count_significant_tokens_in_node(&inner, rem);
                            count += sub_count;
                            if !is_qw_expr {
                                contains_qw |= sub_qw;
                            }
                        }
                        NodeOrToken::Token(token) => {
                            if !token.kind().is_trivia() {
                                count += 1;
                                if !is_qw_expr && matches!(token.kind(), QW_STRING | QW_KW) {
                                    contains_qw = true;
                                }
                            }
                        }
                    }

                    if count >= remaining {
                        break;
                    }
                }

                (count.min(remaining), contains_qw)
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
        self.format_multiline_delimited_elements(iter, open_delimiter, close_delimiter);
    }

    pub(super) fn format_multiline_delimited_elements(
        &mut self,
        elements: impl IntoIterator<Item = SyntaxElement<PerlLanguage>>,
        open_delimiter: SyntaxKind,
        close_delimiter: SyntaxKind,
    ) {
        let ctx = super::FormatContext::default().with_multiline_context();
        for child in elements.into_iter() {
            match child {
                NodeOrToken::Node(node) => {
                    let kind = node.kind();

                    match kind {
                        SyntaxKind::EXPR_LIST => {
                            // Special handling for expression lists inside delimiters
                            self.format_expr_list_multiline_iter(&node);
                        }
                        _ => self.format_node_with_context(&node, ctx),
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
                            if self.writer.at_line_start() {
                                self.writer.add_indent();
                                self.writer.set_at_line_start(false);
                            }
                            self.handle_multiline_opening_delimiter(&token);
                        }
                        k if k == close_delimiter => {
                            self.handle_multiline_closing_delimiter(&token);
                        }
                        _ => {
                            // その他のトークンは通常通り処理
                            self.format_token_with_context(&token, ctx);
                        }
                    }
                }
            }
        }
    }

    fn format_expr_list_multiline_iter(&mut self, list: &PerlNode) {
        let ctx = super::FormatContext::default().with_multiline_context();
        let elements: Vec<_> = list.children_with_tokens().collect();

        let set_local_alignment = if self.alignment_state.is_none() {
            if let Some(state) = self.collect_expr_list_alignment_state(list, &elements) {
                self.alignment_state = Some(state);
                true
            } else {
                false
            }
        } else {
            false
        };

        for child in elements {
            match child {
                NodeOrToken::Node(node) => self.format_node_with_context(&node, ctx),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    match kind {
                        SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {
                            // Skip trivia here - newlines handled in the delimiter handlers
                        }
                        T![,] => {
                            self.format_token_with_context(&token, ctx);
                            self.writer.handle_formatter_newline();
                        }
                        _ => {
                            // その他のトークンは通常通り処理
                            self.format_token_with_context(&token, ctx);
                        }
                    }
                }
            }
        }

        // Reset alignment state only if we set it locally, to prevent it from affecting subsequent nodes
        if set_local_alignment {
            self.alignment_state = None;
        }
    }

    pub(super) fn collect_expr_list_alignment_state(
        &self,
        list: &PerlNode,
        elements: &[SyntaxElement<PerlLanguage>],
    ) -> Option<AlignmentState> {
        if !self
            .options
            .alignment_strategies
            .contains(&AlignmentStrategy::FatCommas)
        {
            return None;
        }

        let mut saw_newline = false;
        let mut comma_count = 0;

        for element in elements {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    SyntaxKind::FAT_COMMA => comma_count += 1,
                    SyntaxKind::NEWLINE => saw_newline = true,
                    _ => {}
                }
            }
        }

        if comma_count < 2 || !saw_newline {
            return None;
        }

        let mut options = self.options.clone();
        options.alignment_strategies.clear();
        let mut formatter = Formatter::with_shared_deps(self.comment_registry.clone(), options);
        formatter.format_node(list);

        let columns = formatter
            .writer
            .collect_token_columns(SyntaxKind::FAT_COMMA);

        if columns.len() != comma_count {
            return None;
        }

        let indent_width = self.writer.indent_level() * self.writer.indent_string_len();
        let mut widths = Vec::with_capacity(columns.len());

        for column in columns {
            let content_width = column.column.saturating_sub(column.indent);
            widths.push(indent_width + content_width);
        }

        let max_width = widths.iter().copied().max()?;
        if widths.iter().all(|&width| width == max_width) {
            return None;
        }

        let pads = widths
            .into_iter()
            .map(|width| max_width - width)
            .collect::<Vec<_>>();

        Some(AlignmentState::new(SyntaxKind::FAT_COMMA, pads))
    }

    pub(super) fn handle_multiline_opening_delimiter(&mut self, token: &SyntaxToken<PerlLanguage>) {
        self.writer.write_token(token);
        self.writer.increase_indent();
        self.writer.handle_formatter_newline();
        self.remember_token(token);
    }

    pub(super) fn handle_multiline_closing_delimiter(&mut self, token: &SyntaxToken<PerlLanguage>) {
        self.writer.decrease_indent();
        if !self.writer.at_line_start() || !self.writer.current_line_is_empty() {
            self.writer.handle_formatter_newline();
        }
        self.writer.add_indent();
        self.writer.write_token(token);
        self.writer.set_at_line_start(false);
        self.remember_token(token);
    }
}
