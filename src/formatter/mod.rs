mod delimited;
mod simple_block;
mod writer;

use crate::{
    comments::{TriviaPosition, TriviaTable},
    PerlLanguage, PerlNode, SyntaxKind, T,
};
use rowan::{ast::SyntaxNodePtr, NodeOrToken, SyntaxElement, SyntaxElementChildren, SyntaxToken};
use simple_block::is_simple_block_cached;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;

use writer::{LineBreakSource, Writer};

#[derive(Clone, Copy, Default)]
struct FormatContext {
    suppress_newlines: bool,
    in_multiline_context: bool,
}

impl FormatContext {
    fn with_suppress_newlines(self) -> Self {
        Self {
            suppress_newlines: true,
            ..self
        }
    }

    fn with_multiline_context(self) -> Self {
        Self {
            in_multiline_context: true,
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DelimiterTightness {
    /// Keep delimiters tight with no interior spacing.
    Tight,
    /// Apply the formatter's standard spacing heuristics.
    #[default]
    Standard,
    /// Prefer looser spacing when possible.
    Loose,
}

impl DelimiterTightness {
    fn should_add_space(self, significant_tokens: usize) -> bool {
        match self {
            Self::Tight => false,
            Self::Standard => significant_tokens >= 2,
            Self::Loose => significant_tokens > 0,
        }
    }

    fn should_add_space_for_simple_block(self) -> bool {
        match self {
            Self::Tight => false,
            Self::Standard | Self::Loose => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DelimiterTightnessConfig {
    pub parentheses: DelimiterTightness,
    pub brackets: DelimiterTightness,
    pub braces: DelimiterTightness,
}

impl DelimiterTightnessConfig {
    #[must_use]
    pub fn new(
        parentheses: DelimiterTightness,
        brackets: DelimiterTightness,
        braces: DelimiterTightness,
    ) -> Self {
        Self {
            parentheses,
            brackets,
            braces,
        }
    }

    #[must_use]
    pub fn with_parentheses(mut self, tightness: DelimiterTightness) -> Self {
        self.parentheses = tightness;
        self
    }

    #[must_use]
    pub fn with_brackets(mut self, tightness: DelimiterTightness) -> Self {
        self.brackets = tightness;
        self
    }

    #[must_use]
    pub fn with_braces(mut self, tightness: DelimiterTightness) -> Self {
        self.braces = tightness;
        self
    }

    fn for_kind(&self, kind: SyntaxKind) -> DelimiterTightness {
        match kind {
            T!['('] | T![')'] => self.parentheses,
            T!['['] | T![']'] => self.brackets,
            T!['{'] | T!['}'] => self.braces,
            _ => DelimiterTightness::Standard,
        }
    }
}

impl Default for DelimiterTightnessConfig {
    fn default() -> Self {
        Self {
            parentheses: DelimiterTightness::Tight,
            brackets: DelimiterTightness::Standard,
            braces: DelimiterTightness::Standard,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignmentStrategy {
    Assignments,
    FatCommas,
    PostfixConditionals,
    Comments,
}

#[derive(Debug, Clone)]
pub struct FormatterOptions {
    pub delimiter_tightness: DelimiterTightnessConfig,
    pub alignment_strategies: Vec<AlignmentStrategy>,
    /// Controls whether compound assignment operators are aligned with simple assignments.
    pub align_compound_assignments: bool,
}

impl Default for FormatterOptions {
    fn default() -> Self {
        Self {
            delimiter_tightness: DelimiterTightnessConfig::default(),
            alignment_strategies: vec![
                AlignmentStrategy::Assignments,
                AlignmentStrategy::FatCommas,
                AlignmentStrategy::PostfixConditionals,
                AlignmentStrategy::Comments,
            ],
            align_compound_assignments: true,
        }
    }
}

impl FormatterOptions {
    #[must_use]
    pub fn with_delimiter_tightness(mut self, config: DelimiterTightnessConfig) -> Self {
        self.delimiter_tightness = config;
        self
    }

    #[must_use]
    pub fn with_alignment_strategies(mut self, strategies: Vec<AlignmentStrategy>) -> Self {
        self.alignment_strategies = strategies;
        self
    }

    #[must_use]
    pub fn with_align_compound_assignments(mut self, align: bool) -> Self {
        self.align_compound_assignments = align;
        self
    }
}

#[derive(Debug)]
struct AlignmentEntry {
    pad: usize,
    token: Option<SyntaxToken<PerlLanguage>>,
}

#[derive(Debug)]
pub(super) struct AlignmentState {
    entries: VecDeque<AlignmentEntry>,
    token_kind: SyntaxKind,
}

impl AlignmentState {
    fn new(token_kind: SyntaxKind, pads: Vec<usize>) -> Self {
        let entries = pads
            .into_iter()
            .map(|pad| AlignmentEntry { pad, token: None })
            .collect();
        Self {
            entries,
            token_kind,
        }
    }

    fn with_token_targets(
        token_kind: SyntaxKind,
        targets: Vec<(SyntaxToken<PerlLanguage>, usize)>,
    ) -> Self {
        let entries = targets
            .into_iter()
            .map(|(token, pad)| AlignmentEntry {
                pad,
                token: Some(token),
            })
            .collect();
        Self {
            entries,
            token_kind,
        }
    }

    fn token_kind(&self) -> SyntaxKind {
        self.token_kind
    }

    fn consume_pad_for(&mut self, token: &SyntaxToken<PerlLanguage>) -> Option<usize> {
        let front = self.entries.front()?;
        if let Some(expected) = front.token.as_ref() {
            if expected != token {
                return None;
            }
        }

        self.entries.pop_front().map(|entry| entry.pad)
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub struct Formatter {
    writer: Writer,
    pending_empty_lines: usize,
    pending_space_after_block_call: bool,
    /// The trivia table is wrapped in `Rc` to allow cheap sharing across multiple
    /// formatter instances. This is critical for alignment calculations, which create
    /// temporary formatters to measure prefix widths without expensive deep clones.
    trivia_table: Rc<TriviaTable>,
    options: FormatterOptions,
    alignment_state: Option<AlignmentState>,
    block_simplicity_cache: HashMap<SyntaxNodePtr<PerlLanguage>, bool>,
    root: PerlNode,
}

impl Formatter {
    #[must_use]
    pub fn new(trivia_table: TriviaTable, root: PerlNode) -> Self {
        Self::with_options(trivia_table, FormatterOptions::default(), root)
    }

    #[must_use]
    pub fn with_options(
        trivia_table: TriviaTable,
        options: FormatterOptions,
        root: PerlNode,
    ) -> Self {
        Self {
            pending_empty_lines: 0,
            pending_space_after_block_call: false,
            writer: Writer::new(),
            // Wrap in Rc to enable cheap cloning when creating temporary formatters
            // for alignment measurements (see measure_alignment_prefix)
            trivia_table: Rc::new(trivia_table),
            options,
            alignment_state: None,
            block_simplicity_cache: HashMap::default(),
            root,
        }
    }

    fn with_shared_deps(
        trivia_table: Rc<TriviaTable>,
        options: FormatterOptions,
        root: PerlNode,
    ) -> Self {
        Self {
            pending_empty_lines: 0,
            pending_space_after_block_call: false,
            writer: Writer::new(),
            trivia_table,
            options,
            alignment_state: None,
            block_simplicity_cache: HashMap::default(),
            root,
        }
    }

    pub fn format(&mut self, node: &PerlNode) -> String {
        self.format_node(node, FormatContext::default());
        self.writer.finish()
    }

    fn remember_token(&mut self, token: &SyntaxToken<PerlLanguage>) {
        let kind = token.kind();
        self.writer.set_prev_token_kind(Some(kind));

        if kind.is_trivia() {
            return;
        }

        if kind == SyntaxKind::COMMENT {
            match self.trivia_table.position_of(token) {
                Some(TriviaPosition::Leading(_)) | Some(TriviaPosition::Trailing(_)) => {}
                _ => self.writer.set_last_significant_token_kind(Some(kind)),
            }
        } else {
            self.writer.set_last_significant_token_kind(Some(kind));
        }
    }

    fn node_has_leading_comment(&self, node: &PerlNode) -> bool {
        let Some(first_token) = node.first_token() else {
            return false;
        };
        self.trivia_table
            .leading_trivia(&first_token)
            .iter()
            .any(|piece| piece.kind() == SyntaxKind::COMMENT)
    }

    fn node_has_trailing_comment(&self, node: &PerlNode) -> bool {
        node.descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::COMMENT)
            .any(|comment| {
                matches!(
                    self.trivia_table.position_of(&comment),
                    Some(TriviaPosition::Trailing(_))
                )
            })
    }

    /// Checks if a node has a comment sibling immediately before it (skipping whitespace/newlines)
    fn node_has_preceding_comment_sibling(node: &PerlNode) -> bool {
        std::iter::successors(node.prev_sibling_or_token(), |elem| {
            elem.prev_sibling_or_token()
        })
        .find(|elem| !matches!(elem.kind(), SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE))
        .is_some_and(|elem| elem.kind() == SyntaxKind::COMMENT)
    }

    fn format_node(&mut self, node: &PerlNode, ctx: FormatContext) {
        // Add empty line before subs, use statements, and regular statements when appropriate
        // This preserves existing behavior for simple cases while also handling statement spacing
        if node.kind().is_phase_block_stmt()
            || matches!(
                node.kind(),
                SyntaxKind::SUB_DEF
                    | SyntaxKind::USE_STMT
                    | SyntaxKind::NO_STMT
                    | SyntaxKind::STMT
                    | SyntaxKind::LABELED_STMT
                    | SyntaxKind::VAR_DECL
                    | SyntaxKind::ELLIPSIS_STMT
                    | SyntaxKind::EMPTY_STMT
            )
        {
            self.add_empty_line_before_if_needed(node);
        }

        // Node types that require special handling
        match node.kind() {
            SyntaxKind::ROOT => {
                // Use the same empty line detection logic as BLOCK_STMT for root-level statements
                self.format_block(node, ctx);
                return;
            }
            SyntaxKind::USE_STMT | SyntaxKind::NO_STMT => {
                self.format_use_no_stmt(node, ctx);
                return;
            }
            SyntaxKind::EMPTY_STMT => {
                // Empty statements are just a semicolon, format with default handling
                // but output pending empty lines first
                if self.pending_empty_lines > 0 {
                    self.output_pending_empty_lines();
                }
                // Default child iteration will handle the semicolon token
            }
            SyntaxKind::HASH_REF => {
                self.format_hash_ref(node, ctx);
                return;
            }
            SyntaxKind::ARRAY_REF => {
                self.format_array_ref(node, ctx);
                return;
            }
            SyntaxKind::QW_EXPR
            | SyntaxKind::Q_EXPR
            | SyntaxKind::QQ_EXPR
            | SyntaxKind::QX_EXPR
            | SyntaxKind::M_EXPR
            | SyntaxKind::QR_EXPR
            | SyntaxKind::S_EXPR
            | SyntaxKind::TR_EXPR => {
                self.format_quote_like(node, ctx);
                return;
            }

            SyntaxKind::DATA_SECTION => {
                self.format_data_section(node);
                return;
            }
            SyntaxKind::LABELED_STMT => {
                self.format_labeled_stmt(node, ctx);
                return;
            }
            SyntaxKind::LABEL => {
                self.format_label(node);
                return;
            }
            SyntaxKind::POD_BLOCK => {
                self.format_pod_block(node);
                return;
            }
            SyntaxKind::SUB_PROTOTYPE => {
                self.format_sub_prototype(node, ctx);
                return;
            }
            SyntaxKind::SUB_SIGNATURE => {
                self.format_sub_signature(node, ctx);
                return;
            }
            SyntaxKind::SIGNATURE_PARAM => {
                // Use default child iteration - spacing managed by general rules
            }
            SyntaxKind::SIGNATURE_DEFAULT => {
                self.format_signature_default(node, ctx);
                return;
            }
            SyntaxKind::FOR_STMT => {
                self.format_for_stmt(node, ctx);
                return;
            }
            SyntaxKind::EXPR_LIST => {
                self.format_expr_list_node(node, ctx);
                return;
            }
            SyntaxKind::ANON_SUB_EXPR
            | SyntaxKind::FUNCTION_CALL_EXPR
            | SyntaxKind::BLOCK_FUNCTION_CALL_EXPR
            | SyntaxKind::METHOD_CALL_EXPR
            | SyntaxKind::HASH_REF_ACCESS_EXPR
            | SyntaxKind::ARRAY_REF_ACCESS_EXPR
            | SyntaxKind::POSTFIX_ARRAY_SLICE_EXPR
            | SyntaxKind::POSTFIX_HASH_SLICE_EXPR
            | SyntaxKind::CODE_REF_CALL_EXPR
            | SyntaxKind::HASH_SUBSCRIPTION_EXPR
            | SyntaxKind::ARRAY_SUBSCRIPTION_EXPR
            | SyntaxKind::COMPOUND_VAR
            | SyntaxKind::REGEX_EXPR
            | SyntaxKind::BACKTICK_EXPR
            | SyntaxKind::PAREN_EXPR => {
                self.format_expr(node, ctx);
                return;
            }
            SyntaxKind::BLOCK_STMT => {
                // Special handling for BLOCK_STMT: detect empty lines between statements
                self.format_block(node, ctx);
                return;
            }
            _ => {
                if self.should_use_parenthesized_formatter(node) {
                    self.format_parenthesized_expr(node, ctx);
                    return;
                }
            }
        }

        // Default child iteration with automatic multiline parenthesis detection
        self.format_children_with_options(node, ctx, false, true);

        // Add empty line after subs and phase block statements, but only if there are siblings
        if node.kind().is_phase_block_stmt() || matches!(node.kind(), SyntaxKind::SUB_DEF) {
            self.add_empty_line_after_if_needed(node);
        }

        // Special handling after children are processed
        if node.kind().is_variable() {
            // This is the logic from format_variable
            self.writer.set_prev_token_kind(Some(node.kind()));
        }
    }

    /// Helper method to format children with multiline paren detection.
    ///
    /// # Parameters
    /// - `node`: The node whose children to format
    /// - `ctx`: The format context
    /// - `skip_whitespace`: If true, skip whitespace tokens
    /// - `handle_pending_empty_lines`: If true, output pending empty lines before certain child nodes
    fn format_children_with_options(
        &mut self,
        node: &PerlNode,
        ctx: FormatContext,
        skip_whitespace: bool,
        handle_pending_empty_lines: bool,
    ) {
        let mut children = node.children_with_tokens();
        while let Some(child) = children.next() {
            match child {
                NodeOrToken::Token(token) => {
                    if skip_whitespace && token.kind() == SyntaxKind::WHITESPACE {
                        continue;
                    }
                    if self.try_format_multiline_parens(&token, &mut children, ctx) {
                        continue;
                    }
                    self.format_token(&token, ctx);
                }
                NodeOrToken::Node(child_node) => {
                    // Output pending empty lines before processing child nodes if requested
                    if handle_pending_empty_lines
                        && self.pending_empty_lines > 0
                        && (child_node.kind().is_phase_block_stmt()
                            || matches!(child_node.kind(), SyntaxKind::STMT | SyntaxKind::VAR_DECL))
                    {
                        self.output_pending_empty_lines();
                    }
                    self.format_node(&child_node, ctx);
                }
            }
        }
    }

    fn format_expr_list_node(&mut self, node: &PerlNode, ctx: FormatContext) {
        let expr_list_alignment = self.collect_expr_list_alignment_state(node);
        self.with_alignment(expr_list_alignment, |formatter| {
            // Use the shared helper for multiline paren detection
            formatter.format_children_with_options(node, ctx, false, false);
        });
    }

    /// Execute a closure with a temporary alignment state, automatically restoring
    /// the previous alignment state when the closure completes.
    fn with_alignment<F>(&mut self, alignment: Option<AlignmentState>, f: F)
    where
        F: FnOnce(&mut Self),
    {
        let should_restore = alignment.is_some();
        let saved_alignment = if should_restore {
            self.alignment_state.replace(alignment.unwrap())
        } else {
            None
        };

        f(self);

        if should_restore {
            self.alignment_state = saved_alignment;
        }
    }

    fn has_newline_before_first_value(&self, node: &PerlNode) -> bool {
        // Check if there's a newline between the opening delimiter and the first non-trivial token
        let mut children = node.children_with_tokens();

        // Find the opening delimiter.
        if !children.by_ref().any(|child| {
            matches!(
                child.as_token().map(rowan::SyntaxToken::kind),
                Some(T!['{'] | T!['['] | T!['('] | SyntaxKind::DELIMITER)
            )
        }) {
            return false;
        }

        // Check subsequent children for a newline before a non-trivia element.
        for child in children {
            match child {
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::NEWLINE {
                        return true;
                    }
                    if !token.kind().is_trivia() {
                        return false;
                    }
                }
                NodeOrToken::Node(_) => {
                    // Any node is considered non-trivia.
                    return false;
                }
            }
        }

        false
    }

    fn has_newline_before_first_value_in_elements(
        &self,
        elements: impl IntoIterator<Item = SyntaxElement<PerlLanguage>>,
    ) -> bool {
        for child in elements.into_iter() {
            match child {
                NodeOrToken::Token(token) => {
                    if matches!(token.kind(), T!['{'] | T!['['] | T!['(']) {
                        continue;
                    }
                    if token.kind() == SyntaxKind::NEWLINE
                        || (token.kind() == SyntaxKind::QW_STRING && token.text().starts_with('\n'))
                    {
                        return true;
                    }
                    if !token.kind().is_trivia() {
                        return false;
                    }
                }
                NodeOrToken::Node(_) => {
                    // Any node is considered non-trivia.
                    return false;
                }
            }
        }

        false
    }

    fn is_simple_block(&mut self, node: &PerlNode) -> bool {
        let trivia = self.trivia_table.as_ref();
        is_simple_block_cached(node, trivia, &mut self.block_simplicity_cache)
    }

    pub(super) fn node_spans_multiple_lines(&self, node: &PerlNode) -> bool {
        // Check if a node contains newlines in its source representation
        node.descendants_with_tokens().any(|element| {
            element
                .as_token()
                .is_some_and(|token| token.kind() == SyntaxKind::NEWLINE)
        })
    }

    fn should_use_parenthesized_formatter(&self, node: &PerlNode) -> bool {
        if !self.node_contains_parentheses(node) {
            return false;
        }

        if self.has_newline_before_first_value(node) {
            return true;
        }

        use SyntaxKind::*;

        if matches!(
            node.kind(),
            FUNCTION_CALL_EXPR
                | BLOCK_FUNCTION_CALL_EXPR
                | METHOD_CALL_EXPR
                | CODE_REF_CALL_EXPR
                | HASH_REF_ACCESS_EXPR
                | ARRAY_REF_ACCESS_EXPR
                | HASH_SUBSCRIPTION_EXPR
                | ARRAY_SUBSCRIPTION_EXPR
                | POSTFIX_ARRAY_SLICE_EXPR
                | POSTFIX_HASH_SLICE_EXPR
                | SUB_PROTOTYPE
                | SUB_SIGNATURE
                | ATTR_ARGS
                | USE_STMT
                | NO_STMT
                | IF_STMT
                | UNLESS_STMT
                | WHILE_STMT
                | UNTIL_STMT
                | FOR_STMT
                | TRY_STMT
                | GIVEN_STATEMENT
                | WHEN_CLAUSE
                | DEFAULT_CLAUSE
        ) {
            return false;
        }

        true
    }

    fn node_contains_parentheses(&self, node: &PerlNode) -> bool {
        let mut has_open = false;
        let mut has_close = false;

        for element in node.descendants_with_tokens() {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    T!['('] => has_open = true,
                    T![')'] => has_close = true,
                    _ => {}
                }
                if has_open && has_close {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a delimited section (e.g., parentheses) should use multiline formatting.
    /// Returns (count_to_closing_delimiter, has_immediate_newline_after_opening).
    fn check_delimited_multiline(
        iter: SyntaxElementChildren<PerlLanguage>,
        open: SyntaxKind,
        close: SyntaxKind,
    ) -> Option<(usize, bool)> {
        let mut depth = 1usize;
        let mut has_immediate_newline = false;
        let mut count = 0usize;
        let mut is_first_after_open = true;

        for element in iter {
            count += 1;

            // Check if the first non-whitespace element after opening delimiter is a newline
            if is_first_after_open {
                match &element {
                    NodeOrToken::Token(token) => {
                        if token.kind() == SyntaxKind::NEWLINE {
                            has_immediate_newline = true;
                            is_first_after_open = false;
                        } else if token.kind() != SyntaxKind::WHITESPACE {
                            is_first_after_open = false;
                        }
                    }
                    NodeOrToken::Node(_) => {
                        is_first_after_open = false;
                    }
                }
            }

            if let NodeOrToken::Token(token) = &element {
                if token.kind() == open {
                    depth += 1;
                } else if token.kind() == close {
                    if depth == 0 {
                        return None;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return Some((count, has_immediate_newline));
                    }
                }
            }
        }

        None
    }

    /// Checks for and formats multiline parenthesized expressions.
    /// Returns `true` if it handled the formatting, `false` otherwise.
    fn try_format_multiline_parens(
        &mut self,
        token: &SyntaxToken<PerlLanguage>,
        children: &mut SyntaxElementChildren<PerlLanguage>,
        ctx: FormatContext,
    ) -> bool {
        if token.kind() == T!['('] {
            if let Some((take_count, has_immediate_newline)) =
                Self::check_delimited_multiline(children.clone(), T!['('], T![')'])
            {
                if has_immediate_newline {
                    let range_iter = std::iter::once(NodeOrToken::Token(token.clone()))
                        .chain(children.by_ref().take(take_count));
                    self.format_multiline_delimited_elements(range_iter, T!['('], T![')'], ctx);
                    return true;
                }
            }
        }
        false
    }

    fn next_significant_token(
        token: &SyntaxToken<PerlLanguage>,
    ) -> Option<SyntaxToken<PerlLanguage>> {
        let mut current = token.next_token();
        while let Some(t) = current {
            if !t.kind().is_trivia() {
                // or just WHITESPACE if that's all you want to skip
                return Some(t);
            }
            current = t.next_token();
        }
        None
    }

    fn next_token_after_whitespace(
        token: &SyntaxToken<PerlLanguage>,
    ) -> Option<SyntaxToken<PerlLanguage>> {
        let mut current = token.next_token();
        while let Some(t) = current {
            if t.kind() != SyntaxKind::WHITESPACE {
                return Some(t);
            }
            current = t.next_token();
        }
        None
    }

    fn needs_continuation_indent(&self, current: SyntaxKind, ctx: FormatContext) -> bool {
        if !self.writer.at_line_start() {
            return false;
        }

        if self.writer.last_line_break() != Some(LineBreakSource::User) {
            return false;
        }

        let Some(prev_kind) = self.writer.last_significant_token_kind() else {
            return false;
        };

        if prev_kind == SyntaxKind::COMMENT {
            return false;
        }

        use SyntaxKind::*;

        if prev_kind == T![package] {
            return true;
        }

        if matches!(
            current,
            IF_KW | UNLESS_KW | WHILE_KW | UNTIL_KW | FOR_KW | FOREACH_KW
        ) && !matches!(prev_kind, L_BRACE | R_BRACE | SEMICOLON)
        {
            return true;
        }

        // Ternary operator parts at line start need continuation indent
        if matches!(current, QUESTION_MARK | COLON) {
            return true;
        }

        // After opening parenthesis in loops, add continuation indent
        // This is primarily for cases like: for my $item (\n@list\n)
        // Skip in multiline context to avoid double indenting function call arguments
        if prev_kind == L_PAREN
            && !matches!(current, R_PAREN)
            && !ctx.in_multiline_context
            && matches!(
                current,
                SyntaxKind::ARRAY_VAR | SyntaxKind::HASH_VAR | SyntaxKind::SCALAR_VAR
            )
        {
            return true;
        }

        if matches!(prev_kind, IDENT | QUALIFIED_IDENT)
            && !ctx.in_multiline_context
            && !matches!(current, SEMICOLON | COMMA)
        {
            return true;
        }

        // After operators (including assignment), add continuation indent
        // Exception: in multiline context, skip for COMMA (but not for FAT_COMMA or other operators)
        if prev_kind.is_operator() || prev_kind.is_assignment_operator() {
            if ctx.in_multiline_context && prev_kind == COMMA {
                // Skip continuation indent for comma in multiline context
            } else {
                return true;
            }
        }

        if current.is_operator()
            && !matches!(
                current,
                UNARY_PLUS
                    | UNARY_MINUS
                    | PREFIX_INCREMENT
                    | PREFIX_DECREMENT
                    | POSTFIX_INCREMENT
                    | POSTFIX_DECREMENT
                    | LOGICAL_NOT
                    | BITWISE_NOT
            )
        {
            return true;
        }

        if !ctx.in_multiline_context && matches!(prev_kind, COMMA | FAT_COMMA) {
            return true;
        }

        false
    }

    fn apply_alignment_padding(&mut self, token: &SyntaxToken<PerlLanguage>) {
        let kind = token.kind();
        if let Some(state) = self.alignment_state.as_mut() {
            let target_kind = state.token_kind();
            let matches_target = if target_kind == SyntaxKind::EQ {
                if kind.is_compoundable_operator() {
                    Self::next_significant_token(token)
                        .map(|next| next.kind().is_assignment_operator())
                        .unwrap_or(false)
                } else if kind.is_assignment_operator() {
                    !self
                        .writer
                        .prev_token_kind()
                        .is_some_and(|prev| prev.is_compoundable_operator())
                } else {
                    false
                }
            } else {
                target_kind == kind
            };

            if matches_target {
                if let Some(pad) = state.consume_pad_for(token) {
                    if pad > 0 {
                        let spaces = " ".repeat(pad);
                        self.writer.write_str(&spaces, None, None);
                    }

                    if state.is_empty() {
                        self.alignment_state = None;
                    }
                }
            }
        }
    }

    fn format_token(&mut self, token: &SyntaxToken<crate::PerlLanguage>, ctx: FormatContext) {
        let kind = token.kind();
        let text = token.text();

        match kind {
            SyntaxKind::WHITESPACE => {}
            SyntaxKind::NEWLINE => {
                // Heredoc content must start on a new line after the heredoc start marker
                // Always preserve newlines after HEREDOC_START
                if self.writer.prev_token_kind() == Some(SyntaxKind::HEREDOC_START) {
                    self.writer.handle_user_newline();
                } else if ctx.suppress_newlines {
                    // Suppress newlines when formatting simple blocks
                } else if self.writer.at_line_start() && self.writer.current_line_is_empty() {
                    if self.pending_empty_lines == 0 {
                        self.pending_empty_lines = 1;
                    }
                } else {
                    self.writer.handle_user_newline();
                }
            }
            SyntaxKind::COMMENT => {
                self.output_pending_empty_lines();

                // コメントは保持するが、適切な位置に配置
                if self.writer.at_line_start() {
                    self.writer.add_indent();
                    self.writer.set_at_line_start(false);
                } else {
                    // This is an inline comment - add 4 spaces before it (like perltidy)
                    self.writer.write_str("    ", None, None);
                }
                self.apply_alignment_padding(token);
                self.writer.write_str(text.trim(), Some(kind), None);
                self.writer.handle_user_newline();
                self.remember_token(token);
            }
            SyntaxKind::HEREDOC_START => {
                // Output pending empty lines before heredoc start
                if self.pending_empty_lines > 0 {
                    self.output_pending_empty_lines();
                }

                // Handle spacing and indentation for heredoc start
                self.handle_spacing_before(kind);

                if self.writer.at_line_start() && !kind.is_trivia() {
                    self.writer.add_indent();
                    if self.needs_continuation_indent(kind, ctx) {
                        self.writer.push_indent_string();
                    }
                    self.writer.set_at_line_start(false);
                }

                self.writer.write_token(token);
                self.remember_token(token);
            }
            SyntaxKind::HEREDOC_CONTENT | SyntaxKind::HEREDOC_END => {
                // Heredoc content and end must preserve exact formatting
                // including all newlines and indentation from the source.
                // The `write_raw` method handles writing without adding indentation.
                for (i, line) in text.split('\n').enumerate() {
                    if i > 0 {
                        self.writer.handle_user_newline();
                    }
                    if !line.is_empty() {
                        // Use write_raw to write without indentation
                        self.writer.write_raw(line, Some(kind), Some(token));
                    }
                }

                self.remember_token(token);
            }
            T!['}'] => {
                if let Some(parent_block) = token.parent() {
                    if parent_block.kind() == SyntaxKind::BLOCK_STMT {
                        if let Some(grandparent) = parent_block.parent() {
                            if grandparent.kind() == SyntaxKind::BLOCK_FUNCTION_CALL_EXPR
                                && self.node_spans_multiple_lines(&parent_block)
                                && !self.writer.at_line_start()
                            {
                                self.writer.handle_formatter_newline();
                            }
                        }
                    }
                }

                // 閉じブレースは特別処理：先にインデントを下げる
                if self.writer.indent_level() > 0 {
                    self.writer.decrease_indent();
                }

                if self.writer.at_line_start() {
                    self.writer.add_indent();
                    self.writer.set_at_line_start(false);
                }

                self.writer.write_token(token);

                let next_token = Self::next_significant_token(token);

                // Check if this closing brace is part of a block function call (grep, map, etc.)
                // where the block is followed by arguments rather than a statement terminator
                let is_block_function_call = token
                    .parent()
                    .and_then(|block| block.parent())
                    .map(|parent| parent.kind() == SyntaxKind::BLOCK_FUNCTION_CALL_EXPR)
                    .unwrap_or(false);

                let should_skip_newline = is_block_function_call
                    || next_token.as_ref().is_some_and(|t| {
                        match t.kind() {
                            T![elsif]
                            | T![else]
                            | T![catch]
                            | T![finally]
                            | T![when]
                            | T![default]
                            | T![;]
                            | T![,]
                            | T!['(']
                            | T![')']
                            | SyntaxKind::COLON => true,
                            // Also check for IDENT tokens with specific text for Try::Tiny style
                            SyntaxKind::IDENT => {
                                matches!(t.text(), "catch" | "finally")
                            }
                            _ => false,
                        }
                    });

                if !should_skip_newline {
                    self.writer.handle_formatter_newline();
                }

                self.remember_token(token);
            }
            _ => {
                // Output pending empty lines before processing non-trivia tokens
                if !kind.is_trivia() && self.pending_empty_lines > 0 {
                    self.output_pending_empty_lines();
                }

                // 通常のトークンの処理
                self.handle_spacing_before(kind);

                if self.writer.at_line_start() && !kind.is_trivia() {
                    self.writer.add_indent();
                    if self.needs_continuation_indent(kind, ctx) {
                        self.writer.push_indent_string();
                    }
                    self.writer.set_at_line_start(false);
                }

                self.apply_alignment_padding(token);

                let prev_token_kind_before = self.writer.prev_token_kind();
                self.writer.write_token(token);
                if matches!(kind, SyntaxKind::UNARY_PLUS | SyntaxKind::UNARY_MINUS)
                    && matches!(
                        prev_token_kind_before,
                        Some(SyntaxKind::IDENT | SyntaxKind::QUALIFIED_IDENT)
                    )
                {
                    if let Some(next_token) = Self::next_significant_token(token) {
                        if next_token.kind() == T!['{'] {
                            self.writer.write_char(' ');
                        }
                    }
                }
                self.handle_spacing_after_with_token(kind, token);
                self.remember_token(token);
            }
        }
    }

    fn handle_spacing_after_with_token(
        &mut self,
        current: SyntaxKind,
        token: &SyntaxToken<crate::PerlLanguage>,
    ) {
        match current {
            T![;] | T!['{'] => {
                let next_token_is_inline_comment = Self::next_token_after_whitespace(token)
                    .is_some_and(|next| next.kind() == SyntaxKind::COMMENT);

                if current == T!['{'] {
                    self.writer.increase_indent();
                }

                if next_token_is_inline_comment {
                    // Don't add newline here—the comment will handle it
                    return;
                }

                self.writer.handle_formatter_newline();
            }
            _ => {}
        }
    }

    fn format_label(&mut self, node: &PerlNode) {
        if self.writer.at_line_start() {
            self.writer.add_indent();
            self.writer.set_at_line_start(false);
        }

        let mut last_token_kind = None;
        for child in node.children_with_tokens() {
            if let NodeOrToken::Token(token) = child {
                if !token.kind().is_trivia() {
                    self.writer.write_token(&token);
                    last_token_kind = Some(token.kind());
                }
            }
        }

        self.writer.set_prev_token_kind(last_token_kind);
        if let Some(kind) = last_token_kind {
            if !kind.is_trivia() {
                self.writer.set_last_significant_token_kind(Some(kind));
            }
        }
    }
}

#[must_use]
pub fn format(node: &PerlNode) -> String {
    format_with_options(node, FormatterOptions::default())
}

#[must_use]
pub fn format_with_options(node: &PerlNode, options: FormatterOptions) -> String {
    let mut root = node.clone();
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let trivia_table = TriviaTable::from_syntax(&root);
    let mut formatter = Formatter::with_options(trivia_table, options, root);
    formatter.format(node)
}

mod expression;
mod literal;
mod quote;
mod spacing;
mod statement;
mod verbatim;
mod whitespace;

#[cfg(test)]
mod tests;
