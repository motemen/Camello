use crate::{PerlLanguage, PerlNode, SyntaxKind, T};
use rowan::{NodeOrToken, SyntaxElement, SyntaxToken};
use std::collections::{HashMap, VecDeque};

use super::{AlignmentState, AlignmentStrategy, FormatContext, Formatter};

type DelimiterSpacing = (Vec<Option<bool>>, Vec<Option<bool>>);

/// Iterator wrapper that supports peeking ahead multiple elements
struct BufferedIterator<I: Iterator> {
    iter: I,
    buffer: VecDeque<I::Item>,
}

impl<I: Iterator> BufferedIterator<I> {
    fn new(iter: I) -> Self {
        Self {
            iter,
            buffer: VecDeque::new(),
        }
    }

    /// Find the kind of the next non-whitespace token without consuming
    fn peek_next_non_whitespace_kind(&mut self) -> Option<SyntaxKind>
    where
        I: Iterator<Item = SyntaxElement<PerlLanguage>>,
    {
        let mut n = 0;
        loop {
            // Fill buffer up to n+1 elements
            while self.buffer.len() <= n {
                if let Some(item) = self.iter.next() {
                    self.buffer.push_back(item);
                } else {
                    return None;
                }
            }

            match self.buffer.get(n) {
                Some(NodeOrToken::Token(token)) if token.kind() != SyntaxKind::WHITESPACE => {
                    return Some(token.kind());
                }
                Some(_) => n += 1,
                None => return None,
            }
        }
    }
}

impl<I: Iterator> Iterator for BufferedIterator<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.buffer.pop_front().or_else(|| self.iter.next())
    }
}

impl Formatter {
    pub(super) fn format_single_line_delimited_children(
        &mut self,
        node: &PerlNode,
        opening: SyntaxKind,
        closing: SyntaxKind,
        skip_whitespace: bool,
        ctx: FormatContext,
    ) {
        let elements = node.children_with_tokens();

        if let Some((open_spacing, close_spacing)) =
            self.compute_delimited_spacing(elements.clone(), opening, closing)
        {
            self.apply_delimited_spacing(
                elements,
                open_spacing,
                close_spacing,
                skip_whitespace,
                ctx,
            );
        } else if ctx.suppress_newlines {
            let suppressed_ctx = ctx.with_suppress_newlines();
            self.format_elements_without_pairs(elements, skip_whitespace, suppressed_ctx);
        } else {
            self.format_children(node, skip_whitespace, ctx);
        }
    }

    pub(super) fn format_single_line_delimited_elements<I>(
        &mut self,
        elements: I,
        opening: SyntaxKind,
        closing: SyntaxKind,
        skip_whitespace: bool,
        ctx: FormatContext,
    ) where
        I: IntoIterator<Item = SyntaxElement<PerlLanguage>>,
        I::IntoIter: Clone,
    {
        let ctx = ctx.with_suppress_newlines();
        let iter = elements.into_iter();

        if let Some((open_spacing, close_spacing)) =
            self.compute_delimited_spacing(iter.clone(), opening, closing)
        {
            self.apply_delimited_spacing(iter, open_spacing, close_spacing, skip_whitespace, ctx);
        } else {
            self.format_elements_without_pairs(iter, skip_whitespace, ctx);
        }
    }

    fn compute_delimited_spacing<I>(
        &self,
        elements: I,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) -> Option<DelimiterSpacing>
    where
        I: Iterator<Item = SyntaxElement<PerlLanguage>>,
    {
        struct Frame {
            open_index: usize,
            significant_tokens: usize,
            contains_qw: bool,
        }

        let mut open_spacing: Vec<Option<bool>> = Vec::new();
        let mut close_spacing: Vec<Option<bool>> = Vec::new();
        let mut stack: Vec<Frame> = Vec::new();
        let mut saw_pair = false;

        for (index, element) in elements.enumerate() {
            open_spacing.push(None);
            close_spacing.push(None);

            match element {
                NodeOrToken::Node(node) => {
                    if stack.is_empty() {
                        continue;
                    }

                    let max_remaining = stack
                        .iter()
                        .map(|frame| 2usize.saturating_sub(frame.significant_tokens))
                        .max()
                        .unwrap_or(0);

                    if max_remaining == 0 {
                        continue;
                    }

                    let (count, contains_qw) =
                        Self::count_significant_tokens_in_node(&node, max_remaining);

                    if count == 0 && !contains_qw {
                        continue;
                    }

                    for frame in &mut stack {
                        let remaining = 2usize.saturating_sub(frame.significant_tokens);
                        if remaining == 0 {
                            continue;
                        }
                        frame.significant_tokens += count.min(remaining);
                        if contains_qw {
                            frame.contains_qw = true;
                        }
                    }
                }
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    if kind == opening {
                        if !stack.is_empty() {
                            for frame in &mut stack {
                                if frame.significant_tokens < 2 {
                                    frame.significant_tokens += 1;
                                }
                            }
                        }
                        stack.push(Frame {
                            open_index: index,
                            significant_tokens: 0,
                            contains_qw: false,
                        });
                        continue;
                    }

                    if kind == closing {
                        if let Some(frame) = stack.pop() {
                            saw_pair = true;
                            let tightness = self.options.delimiter_tightness.for_kind(opening);
                            let mut add_interior_space =
                                tightness.should_add_space(frame.significant_tokens);
                            if frame.contains_qw && opening != T!['('] {
                                add_interior_space = true;
                            }
                            open_spacing[frame.open_index] = Some(add_interior_space);
                            close_spacing[index] = Some(add_interior_space);
                        }
                        continue;
                    }

                    if stack.is_empty() {
                        continue;
                    }

                    if !kind.is_trivia() {
                        let is_qw = matches!(kind, SyntaxKind::QW_STRING | SyntaxKind::QW_KW);
                        for frame in &mut stack {
                            if frame.significant_tokens < 2 {
                                frame.significant_tokens += 1;
                            }
                            if is_qw {
                                frame.contains_qw = true;
                            }
                        }
                    }
                }
            }
        }

        if saw_pair {
            Some((open_spacing, close_spacing))
        } else {
            None
        }
    }

    fn apply_delimited_spacing<I>(
        &mut self,
        elements: I,
        open_spacing: Vec<Option<bool>>,
        close_spacing: Vec<Option<bool>>,
        skip_whitespace: bool,
        ctx: FormatContext,
    ) where
        I: Iterator<Item = SyntaxElement<PerlLanguage>>,
    {
        use SyntaxKind::WHITESPACE;

        for (index, element) in elements.enumerate() {
            match element {
                NodeOrToken::Node(node) => {
                    self.format_node(&node, ctx);
                }
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    if let Some(add_space) = open_spacing.get(index).copied().flatten() {
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
                    } else if let Some(add_space) = close_spacing.get(index).copied().flatten() {
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
                        self.format_token(&token, ctx);
                    }
                }
            }
        }
    }

    fn format_elements_without_pairs<I>(
        &mut self,
        elements: I,
        skip_whitespace: bool,
        ctx: FormatContext,
    ) where
        I: Iterator<Item = SyntaxElement<PerlLanguage>>,
    {
        for element in elements {
            match element {
                NodeOrToken::Node(node) => self.format_node(&node, ctx),
                NodeOrToken::Token(token) => {
                    if skip_whitespace && token.kind() == SyntaxKind::WHITESPACE {
                        continue;
                    }
                    self.format_token(&token, ctx);
                }
            }
        }
    }

    fn count_significant_tokens_in_node(node: &PerlNode, remaining: usize) -> (usize, bool) {
        use SyntaxKind::{
            ARRAY_VAR, HASH_VAR, PREFIX_EXPR, QW_EXPR, QW_KW, QW_STRING, SCALAR_VAR, TYPEGLOB_VAR,
        };

        if remaining == 0 {
            return (0, false);
        }

        match node.kind() {
            SCALAR_VAR | ARRAY_VAR | HASH_VAR | TYPEGLOB_VAR => (1, false),
            PREFIX_EXPR => {
                // A prefix expression is a single "element", but we need to check if it contains `qw`.
                let contains_qw = node.descendants_with_tokens().any(|el| {
                    let el_kind = match el {
                        NodeOrToken::Node(n) => n.kind(),
                        NodeOrToken::Token(t) => t.kind(),
                    };
                    matches!(el_kind, QW_EXPR | QW_KW | QW_STRING)
                });
                (1, contains_qw)
            }
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

    pub(super) fn format_multiline_delimited_elements(
        &mut self,
        elements: impl IntoIterator<Item = SyntaxElement<PerlLanguage>>,
        open_delimiter: SyntaxKind,
        close_delimiter: SyntaxKind,
        ctx: super::FormatContext,
    ) {
        let ctx = ctx.with_multiline_context();
        let mut iter = BufferedIterator::new(elements.into_iter());

        while let Some(child) = iter.next() {
            match child {
                NodeOrToken::Node(node) => {
                    let kind = node.kind();

                    match kind {
                        SyntaxKind::EXPR_LIST => {
                            // Special handling for expression lists inside delimiters
                            self.format_expr_list_multiline(&node, ctx);
                        }
                        _ => self.format_node(&node, ctx),
                    }
                }
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace - spacing is managed by formatter
                        }
                        SyntaxKind::NEWLINE => {
                            // Preserve user-provided newlines
                            self.format_token(&token, ctx);
                        }
                        k if k == open_delimiter => {
                            self.handle_spacing_before(kind);
                            if self.writer.at_line_start() {
                                self.writer.add_indent();
                                self.writer.set_at_line_start(false);
                            }

                            // Check if next non-whitespace token is a newline
                            let has_user_newline_after =
                                iter.peek_next_non_whitespace_kind() == Some(SyntaxKind::NEWLINE);

                            self.writer.write_token(&token);
                            self.writer.increase_indent();
                            if !has_user_newline_after {
                                self.writer.handle_formatter_newline();
                            }
                            self.remember_token(&token);
                        }
                        k if k == close_delimiter => {
                            self.handle_multiline_closing_delimiter(&token);
                        }
                        _ => {
                            // その他のトークンは通常通り処理
                            self.format_token(&token, ctx);
                        }
                    }
                }
            }
        }
    }

    fn format_expr_list_multiline(&mut self, list: &PerlNode, ctx: super::FormatContext) {
        let ctx = ctx.with_multiline_context();

        let expr_list_alignment = self.collect_expr_list_alignment_state(list);
        self.with_alignment(expr_list_alignment, |formatter| {
            let mut iter = BufferedIterator::new(list.children_with_tokens());
            let mut skip_next_newline = false;
            let mut preceding_whitespace_count = 0;

            while let Some(child) = iter.next() {
                match child {
                    NodeOrToken::Node(node) => {
                        preceding_whitespace_count = 0;
                        formatter.format_node(&node, ctx);
                    }
                    NodeOrToken::Token(token) => {
                        let kind = token.kind();

                        match kind {
                            SyntaxKind::WHITESPACE => {
                                // Track the whitespace count before a comment
                                // Count only spaces (not tabs or other whitespace)
                                preceding_whitespace_count =
                                    token.text().chars().filter(|c| *c == ' ').count();
                            }
                            SyntaxKind::NEWLINE => {
                                preceding_whitespace_count = 0;
                                // Skip if we already handled the newline after a comment
                                if skip_next_newline {
                                    skip_next_newline = false;
                                } else {
                                    // Preserve user-provided newlines
                                    formatter.format_token(&token, ctx);
                                }
                            }
                            SyntaxKind::COMMENT => {
                                // Handle inline and new-line comments
                                if formatter.writer.at_line_start() {
                                    formatter.writer.add_indent();
                                    formatter.writer.set_at_line_start(false);
                                } else {
                                    // Use max of original spacing and configured minimum
                                    let spaces_to_add = preceding_whitespace_count
                                        .max(formatter.options.min_spaces_before_comment);
                                    for _ in 0..spaces_to_add {
                                        formatter.writer.write_char(' ');
                                    }
                                }
                                formatter
                                    .writer
                                    .write_str(token.text().trim(), Some(kind), None);

                                // Add newline after comment
                                formatter.writer.handle_user_newline();

                                // Skip the next newline token since we already added one
                                skip_next_newline = true;

                                formatter.remember_token(&token);
                                preceding_whitespace_count = 0;
                            }
                            T![,] => {
                                preceding_whitespace_count = 0;
                                formatter.format_token(&token, ctx);

                                // Check what comes after the comma (skipping whitespace)
                                let next_token_kind = iter.peek_next_non_whitespace_kind();

                                // Don't add automatic newline if:
                                // 1. User has provided a newline already
                                // 2. Next token is a comment (preserve inline comments)
                                let should_add_newline = next_token_kind
                                    != Some(SyntaxKind::NEWLINE)
                                    && next_token_kind != Some(SyntaxKind::COMMENT);

                                if should_add_newline {
                                    formatter.writer.handle_formatter_newline();
                                }
                            }
                            _ => {
                                preceding_whitespace_count = 0;
                                // その他のトークンは通常通り処理
                                formatter.format_token(&token, ctx);
                            }
                        }
                    }
                }
            }
        });
    }

    pub(super) fn collect_expr_list_alignment_state(
        &self,
        list: &PerlNode,
    ) -> Option<AlignmentState> {
        if !self
            .options
            .alignment_strategies
            .contains(&AlignmentStrategy::FatCommas)
        {
            return None;
        }

        let mut saw_newline = false;
        let mut entry_tokens: Vec<Vec<SyntaxToken<PerlLanguage>>> = Vec::new();
        let mut current_entry_tokens = Vec::new();
        let mut has_entry_content = false;

        for element in list.children_with_tokens() {
            match element {
                NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::FAT_COMMA => {
                        current_entry_tokens.push(token.clone());
                        has_entry_content = true;
                    }
                    SyntaxKind::COMMA => {
                        entry_tokens.push(std::mem::take(&mut current_entry_tokens));
                        has_entry_content = false;
                    }
                    SyntaxKind::NEWLINE => {
                        saw_newline = true;
                    }
                    SyntaxKind::WHITESPACE => {}
                    _ => {
                        has_entry_content = true;
                    }
                },
                NodeOrToken::Node(node) => {
                    has_entry_content = true;
                    current_entry_tokens.extend(node.descendants_with_tokens().filter_map(
                        |element| {
                            if let NodeOrToken::Token(token) = element {
                                (token.kind() == SyntaxKind::FAT_COMMA).then_some(token)
                            } else {
                                None
                            }
                        },
                    ));
                }
            }
        }

        if has_entry_content || !current_entry_tokens.is_empty() {
            entry_tokens.push(current_entry_tokens);
        }

        if entry_tokens.len() < 2 || !saw_newline {
            return None;
        }

        let mut options = self.options.clone();
        options.alignment_strategies.clear();
        let mut formatter =
            Formatter::with_shared_deps(self.trivia_table.clone(), options, self.root.clone());
        formatter
            .writer
            .set_indent_level(self.writer.indent_level());
        formatter.format_node(list, FormatContext::default());

        let token_columns = formatter
            .writer
            .collect_token_columns(SyntaxKind::FAT_COMMA);
        if token_columns.is_empty() {
            return None;
        }

        let indent_width = self.writer.indent_level() * self.writer.indent_string_len();
        let mut pad_matrix: Vec<Vec<usize>> = entry_tokens
            .iter()
            .map(|tokens| vec![0; tokens.len()])
            .collect();

        let max_ord = entry_tokens
            .iter()
            .map(|tokens| tokens.len())
            .max()
            .unwrap_or(0);

        // Build a HashMap from tokens to their column information for O(1) lookups
        let token_column_map: HashMap<_, _> = token_columns
            .into_iter()
            .filter_map(|col| col.token.as_ref().map(|tok| (tok.clone(), col.clone())))
            .collect();

        for ordinal in 0..max_ord {
            let mut ordinal_entries = Vec::new();
            for (entry_index, tokens) in entry_tokens.iter().enumerate() {
                if let Some(token) = tokens.get(ordinal) {
                    let column = token_column_map.get(token).cloned();
                    let column = column?;
                    ordinal_entries.push((entry_index, token.clone(), column));
                }
            }

            if ordinal_entries.len() < 2 {
                continue;
            }

            let mut group_start = 0;
            while group_start < ordinal_entries.len() {
                let mut group_end = group_start + 1;
                while group_end < ordinal_entries.len() {
                    let prev_line = ordinal_entries[group_end - 1].2.line_index;
                    let current_line = ordinal_entries[group_end].2.line_index;
                    if current_line > prev_line + 1 {
                        break;
                    }
                    group_end += 1;
                }

                let group = &ordinal_entries[group_start..group_end];
                if group.len() >= 2 {
                    let widths = group
                        .iter()
                        .map(|(entry_index, _, column)| {
                            let content_width = column.column.saturating_sub(column.indent);
                            let base_width = indent_width + content_width;
                            let previous_pad = pad_matrix
                                .get(*entry_index)
                                .map(|pads| pads.iter().take(ordinal).sum::<usize>())
                                .unwrap_or(0);
                            base_width + previous_pad
                        })
                        .collect::<Vec<_>>();

                    let max_width = widths.iter().copied().max().unwrap_or(0);
                    let all_equal = widths.iter().all(|&width| width == max_width);

                    for ((entry_index, _token, _column), width) in
                        group.iter().zip(widths.into_iter())
                    {
                        if let Some(pads) = pad_matrix.get_mut(*entry_index) {
                            if let Some(slot) = pads.get_mut(ordinal) {
                                *slot = if all_equal { 0 } else { max_width - width };
                            }
                        }
                    }
                }

                group_start = group_end;
            }
        }

        if pad_matrix
            .iter()
            .all(|entry| entry.iter().all(|&pad| pad == 0))
        {
            return None;
        }

        let mut targets = Vec::new();
        for (tokens, pads) in entry_tokens.iter().zip(pad_matrix.into_iter()) {
            for (token, pad) in tokens.iter().cloned().zip(pads.into_iter()) {
                targets.push((token, pad));
            }
        }

        Some(AlignmentState::with_token_targets(
            SyntaxKind::FAT_COMMA,
            targets,
        ))
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
