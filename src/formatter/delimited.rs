use crate::{PerlLanguage, PerlNode, SyntaxKind, T};
use rowan::{NodeOrToken, SyntaxElement, SyntaxToken};

use super::{AlignmentState, AlignmentStrategy, FormatContext, Formatter};

type DelimiterSpacing = (Vec<Option<bool>>, Vec<Option<bool>>);

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
        let elements: Vec<_> = node.children_with_tokens().collect();

        if let Some((open_spacing, close_spacing)) =
            self.compute_delimited_spacing(&elements, opening, closing)
        {
            self.apply_delimited_spacing(
                elements,
                open_spacing,
                close_spacing,
                skip_whitespace,
                ctx,
            );
        } else if ctx.suppress_newlines {
            self.format_elements_without_pairs(elements, skip_whitespace, ctx);
        } else {
            self.format_children(node, skip_whitespace);
        }
    }

    pub(super) fn format_single_line_delimited_elements(
        &mut self,
        elements: Vec<SyntaxElement<PerlLanguage>>,
        opening: SyntaxKind,
        closing: SyntaxKind,
        skip_whitespace: bool,
    ) {
        let ctx = FormatContext::default().with_suppress_newlines();
        if let Some((open_spacing, close_spacing)) =
            self.compute_delimited_spacing(&elements, opening, closing)
        {
            self.apply_delimited_spacing(
                elements,
                open_spacing,
                close_spacing,
                skip_whitespace,
                ctx,
            );
        } else {
            self.format_elements_without_pairs(elements, skip_whitespace, ctx);
        }
    }

    fn compute_delimited_spacing(
        &self,
        elements: &[SyntaxElement<PerlLanguage>],
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) -> Option<DelimiterSpacing> {
        let mut stack: Vec<usize> = Vec::new();
        let mut pairs: Vec<(usize, usize)> = Vec::new();

        for (index, element) in elements.iter().enumerate() {
            if let Some(token) = element.as_token() {
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
            return None;
        }

        let mut open_spacing: Vec<Option<bool>> = vec![None; elements.len()];
        let mut close_spacing: Vec<Option<bool>> = vec![None; elements.len()];

        for (open_index, close_index) in &pairs {
            if close_index <= open_index {
                continue;
            }

            let mut significant_tokens = 0;
            let mut contains_qw = false;

            for element in &elements[open_index + 1..*close_index] {
                match element {
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

        Some((open_spacing, close_spacing))
    }

    fn apply_delimited_spacing(
        &mut self,
        elements: Vec<SyntaxElement<PerlLanguage>>,
        open_spacing: Vec<Option<bool>>,
        close_spacing: Vec<Option<bool>>,
        skip_whitespace: bool,
        ctx: FormatContext,
    ) {
        use SyntaxKind::WHITESPACE;

        for (index, element) in elements.into_iter().enumerate() {
            match element {
                NodeOrToken::Node(node) => {
                    self.format_node_with_context(&node, ctx);
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

    fn format_elements_without_pairs(
        &mut self,
        elements: Vec<SyntaxElement<PerlLanguage>>,
        skip_whitespace: bool,
        ctx: FormatContext,
    ) {
        for element in elements {
            match element {
                NodeOrToken::Node(node) => self.format_node_with_context(&node, ctx),
                NodeOrToken::Token(token) => {
                    if skip_whitespace && token.kind() == SyntaxKind::WHITESPACE {
                        continue;
                    }
                    self.format_token_with_context(&token, ctx);
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

    pub(super) fn format_multiline_delimited_elements(
        &mut self,
        elements: impl IntoIterator<Item = SyntaxElement<PerlLanguage>>,
        open_delimiter: SyntaxKind,
        close_delimiter: SyntaxKind,
    ) {
        let ctx = super::FormatContext::default().with_multiline_context();
        let elements_vec: Vec<_> = elements.into_iter().collect();

        for (index, child) in elements_vec.iter().enumerate() {
            match child {
                NodeOrToken::Node(node) => {
                    let kind = node.kind();

                    match kind {
                        SyntaxKind::EXPR_LIST => {
                            // Special handling for expression lists inside delimiters
                            self.format_expr_list_multiline_iter(node);
                        }
                        _ => self.format_node_with_context(node, ctx),
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
                            self.format_token_with_context(token, ctx);
                        }
                        k if k == open_delimiter => {
                            self.handle_spacing_before(kind);
                            if self.writer.at_line_start() {
                                self.writer.add_indent();
                                self.writer.set_at_line_start(false);
                            }

                            // Check if next non-whitespace token is a newline
                            let has_user_newline_after = elements_vec
                                .iter()
                                .skip(index + 1)
                                .find(|e| {
                                    !matches!(
                                        e.as_token().map(|t| t.kind()),
                                        Some(SyntaxKind::WHITESPACE)
                                    )
                                })
                                .and_then(|e| e.as_token())
                                .map(|t| t.kind())
                                == Some(SyntaxKind::NEWLINE);

                            self.writer.write_token(token);
                            self.writer.increase_indent();
                            if !has_user_newline_after {
                                self.writer.handle_formatter_newline();
                            }
                            self.remember_token(token);
                        }
                        k if k == close_delimiter => {
                            self.handle_multiline_closing_delimiter(token);
                        }
                        _ => {
                            // その他のトークンは通常通り処理
                            self.format_token_with_context(token, ctx);
                        }
                    }
                }
            }
        }
    }

    fn format_expr_list_multiline_iter(&mut self, list: &PerlNode) {
        let ctx = super::FormatContext::default().with_multiline_context();
        let elements: Vec<_> = list.children_with_tokens().collect();

        let mut set_local_alignment = false;
        if self.alignment_state.is_none() {
            if let Some(state) = self.collect_expr_list_alignment_state(list, &elements) {
                self.alignment_state = Some(state);
                set_local_alignment = true;
            }
        }

        for (index, child) in elements.iter().enumerate() {
            match child {
                NodeOrToken::Node(node) => self.format_node_with_context(node, ctx),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace - spacing is managed by formatter
                        }
                        SyntaxKind::NEWLINE => {
                            // Preserve user-provided newlines
                            self.format_token_with_context(token, ctx);
                        }
                        T![,] => {
                            self.format_token_with_context(token, ctx);

                            // Only add automatic newline if user hasn't provided one
                            let has_user_newline_after = elements
                                .iter()
                                .skip(index + 1)
                                .find(|e| {
                                    !matches!(
                                        e.as_token().map(|t| t.kind()),
                                        Some(SyntaxKind::WHITESPACE)
                                    )
                                })
                                .and_then(|e| e.as_token())
                                .map(|t| t.kind())
                                != Some(SyntaxKind::NEWLINE);

                            if has_user_newline_after {
                                self.writer.handle_formatter_newline();
                            }
                        }
                        _ => {
                            // その他のトークンは通常通り処理
                            self.format_token_with_context(token, ctx);
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
        let mut top_level_commas = Vec::new();

        for element in elements {
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
        let mut formatter = Formatter::with_shared_deps(self.comment_registry.clone(), options);
        formatter
            .writer
            .set_indent_level(self.writer.indent_level());
        formatter.format_node(list);
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
