use crate::{PerlLanguage, PerlNode, SyntaxKind, T};
use rowan::{NodeOrToken, SyntaxElement, SyntaxToken};
use std::collections::VecDeque;

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
    fn indent_multiline_element(&mut self, first_kind: SyntaxKind, ctx: FormatContext) {
        if !self.writer.at_line_start() || !self.writer.current_line_is_empty() {
            return;
        }

        if self.writer.indent_level() == 0 {
            return;
        }

        self.writer.add_indent();
        if self.needs_continuation_indent(first_kind, ctx) {
            self.writer.push_indent_string();
        }
        self.writer.set_at_line_start(false);
    }

    fn element_contains_newline(element: &SyntaxElement<PerlLanguage>) -> bool {
        match element {
            NodeOrToken::Token(token) => token.kind() == SyntaxKind::NEWLINE,
            NodeOrToken::Node(node) => node.descendants_with_tokens().any(|descendant| {
                descendant
                    .into_token()
                    .is_some_and(|token| token.kind() == SyntaxKind::NEWLINE)
            }),
        }
    }

    fn collect_nested_elements<I>(
        first_token: SyntaxToken<PerlLanguage>,
        iter: &mut BufferedIterator<I>,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) -> (Vec<SyntaxElement<PerlLanguage>>, bool)
    where
        I: Iterator<Item = SyntaxElement<PerlLanguage>>,
    {
        let mut depth = 1;
        let mut has_newline = false;
        let mut elements = Vec::new();
        elements.push(first_token.into());

        for next in iter {
            has_newline |= Self::element_contains_newline(&next);

            if let Some(token) = next.as_token() {
                let kind = token.kind();
                if kind == opening {
                    depth += 1;
                } else if kind == closing {
                    depth -= 1;
                }
            }

            elements.push(next.clone());

            if depth == 0 {
                break;
            }
        }

        (elements, has_newline)
    }

    fn format_nested_delimiters<I>(
        &mut self,
        token: SyntaxToken<PerlLanguage>,
        iter: &mut BufferedIterator<I>,
        open: SyntaxKind,
        close: SyntaxKind,
        ctx: FormatContext,
    ) where
        I: Iterator<Item = SyntaxElement<PerlLanguage>>,
    {
        let (elements, has_newline) = Self::collect_nested_elements(token, iter, open, close);
        let elements_iter = elements.into_iter();
        if has_newline {
            self.format_multiline_delimited_elements(elements_iter, open, close, ctx);
        } else {
            self.format_single_line_delimited_elements(elements_iter, open, close, true, ctx);
        }
    }

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
                            if frame.contains_qw {
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

                    if let Some(first_token) = node.descendants_with_tokens().find_map(|element| {
                        element
                            .into_token()
                            .filter(|token| !token.kind().is_trivia())
                    }) {
                        self.indent_multiline_element(first_token.kind(), ctx);
                    }

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
                        SyntaxKind::L_PAREN if open_delimiter != SyntaxKind::L_PAREN => {
                            self.format_nested_delimiters(token, &mut iter, T!['('], T![')'], ctx);
                        }
                        SyntaxKind::L_BRACKET if open_delimiter != SyntaxKind::L_BRACKET => {
                            self.format_nested_delimiters(token, &mut iter, T!['['], T![']'], ctx);
                        }
                        _ => {
                            self.indent_multiline_element(kind, ctx);
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

        let mut set_local_alignment = false;
        if self.alignment_state.is_none() {
            if let Some(state) = self.collect_expr_list_alignment_state(list) {
                self.alignment_state = Some(state);
                set_local_alignment = true;
            }
        }

        let mut iter = BufferedIterator::new(list.children_with_tokens());
        let mut skip_next_newline = false;

        while let Some(child) = iter.next() {
            match child {
                NodeOrToken::Node(node) => {
                    if let Some(first_token) = node.descendants_with_tokens().find_map(|element| {
                        element
                            .into_token()
                            .filter(|token| !token.kind().is_trivia())
                    }) {
                        self.indent_multiline_element(first_token.kind(), ctx);
                    }

                    self.format_node(&node, ctx)
                }
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace - spacing is managed by formatter
                        }
                        SyntaxKind::NEWLINE => {
                            // Skip if we already handled the newline after a comment
                            if skip_next_newline {
                                skip_next_newline = false;
                            } else {
                                // Preserve user-provided newlines
                                self.format_token(&token, ctx);
                            }
                        }
                        SyntaxKind::COMMENT => {
                            // Handle inline and new-line comments
                            if self.writer.at_line_start() {
                                self.writer.add_indent();
                                self.writer.set_at_line_start(false);
                            } else {
                                self.writer.write_char(' ');
                            }
                            self.writer.write_str(token.text().trim(), Some(kind), None);

                            // Add newline after comment
                            self.writer.handle_user_newline();

                            // Skip the next newline token since we already added one
                            skip_next_newline = true;

                            self.remember_token(&token);
                        }
                        T![,] => {
                            self.format_token(&token, ctx);

                            // Check what comes after the comma (skipping whitespace)
                            let next_token_kind = iter.peek_next_non_whitespace_kind();

                            // Don't add automatic newline if:
                            // 1. User has provided a newline already
                            // 2. Next token is a comment (preserve inline comments)
                            let should_add_newline = next_token_kind != Some(SyntaxKind::NEWLINE)
                                && next_token_kind != Some(SyntaxKind::COMMENT);

                            if should_add_newline {
                                self.writer.handle_formatter_newline();
                            }
                        }
                        SyntaxKind::L_PAREN => {
                            self.format_nested_delimiters(token, &mut iter, T!['('], T![')'], ctx);
                        }
                        SyntaxKind::L_BRACKET => {
                            self.format_nested_delimiters(token, &mut iter, T!['['], T![']'], ctx);
                        }
                        _ => {
                            self.indent_multiline_element(kind, ctx);
                            // その他のトークンは通常通り処理
                            self.format_token(&token, ctx);
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
    ) -> Option<AlignmentState> {
        if !self
            .options
            .alignment_strategies
            .contains(&AlignmentStrategy::FatCommas)
        {
            return None;
        }

        let mut saw_newline = false;
        let mut top_level_commas = Vec::new();

        for element in list.children_with_tokens() {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    SyntaxKind::FAT_COMMA => top_level_commas.push(token.clone()),
                    SyntaxKind::NEWLINE => saw_newline = true,
                    _ => {}
                }
            }
        }

        if top_level_commas.len() < 2 || !saw_newline {
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
        let mut filtered = Vec::new();
        for column in formatter
            .writer
            .collect_token_columns(SyntaxKind::FAT_COMMA)
        {
            if let Some(token) = column.token.as_ref() {
                if let Some(position) = top_level_commas
                    .iter()
                    .position(|candidate| candidate == token)
                {
                    filtered.push((position, column));
                }
            }
        }

        if filtered.len() != top_level_commas.len() {
            return None;
        }

        filtered.sort_by_key(|(position, _)| *position);
        let columns: Vec<_> = filtered.into_iter().map(|(_, column)| column).collect();

        // Break columns into groups where each group contains consecutive lines.
        // A line break occurs when line_index jumps by more than 1 (indicating a line
        // without a fat comma in between).
        let mut groups = Vec::new();
        if !columns.is_empty() {
            let mut start = 0;
            for i in 1..columns.len() {
                if columns[i].line_index > columns[i - 1].line_index + 1 {
                    // Line index jumped - there's a line without a fat comma
                    groups.push(&columns[start..i]);
                    start = i;
                }
            }
            groups.push(&columns[start..]);
        }

        // Filter out groups with fewer than 2 elements
        let groups: Vec<_> = groups
            .into_iter()
            .filter(|group| group.len() >= 2)
            .collect();

        if groups.is_empty() {
            return None;
        }

        let indent_width = self.writer.indent_level() * self.writer.indent_string_len();
        let mut all_pads = Vec::new();

        // Calculate padding for each group independently
        for group in groups {
            let widths = group
                .iter()
                .map(|column| {
                    let content_width = column.column.saturating_sub(column.indent);
                    indent_width + content_width
                })
                .collect::<Vec<_>>();

            let max_width = widths.iter().copied().max()?;

            // If all widths are the same, no padding needed for this group
            let group_pads = if widths.iter().all(|&width| width == max_width) {
                vec![0; widths.len()]
            } else {
                widths
                    .into_iter()
                    .map(|width| max_width - width)
                    .collect::<Vec<_>>()
            };

            all_pads.extend(group_pads);
        }

        // Return None if no padding is actually needed
        if all_pads.iter().all(|&pad| pad == 0) {
            return None;
        }

        Some(AlignmentState::new(SyntaxKind::FAT_COMMA, all_pads))
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
