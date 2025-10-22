mod delimited;
mod writer;

use crate::{
    comments::{CommentAnchor, CommentId, CommentOwner, CommentPlacement, CommentRegistry},
    PerlLanguage, PerlNode, SyntaxKind, T,
};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxToken};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelimiterTightness {
    /// Keep delimiters tight with no interior spacing.
    Tight,
    /// Apply the formatter's standard spacing heuristics.
    Standard,
    /// Prefer looser spacing when possible.
    Loose,
}

impl Default for DelimiterTightness {
    fn default() -> Self {
        Self::Standard
    }
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
pub(super) struct AlignmentState {
    pads: VecDeque<usize>,
    token_kind: SyntaxKind,
}

impl AlignmentState {
    fn new(token_kind: SyntaxKind, pads: Vec<usize>) -> Self {
        Self {
            pads: pads.into(),
            token_kind,
        }
    }

    fn token_kind(&self) -> SyntaxKind {
        self.token_kind
    }

    fn next_pad(&mut self) -> Option<usize> {
        self.pads.pop_front()
    }

    fn is_empty(&self) -> bool {
        self.pads.is_empty()
    }
}

pub struct Formatter {
    writer: Writer,
    pending_empty_lines: usize,
    /// The comment registry is wrapped in `Rc` to allow cheap sharing across multiple
    /// formatter instances. This is critical for assignment alignment, which creates
    /// temporary formatters to measure prefix widths without expensive deep clones.
    comment_registry: Rc<CommentRegistry>,
    options: FormatterOptions,
    alignment_state: Option<AlignmentState>,
}

impl Formatter {
    #[must_use]
    pub fn new(comment_registry: CommentRegistry) -> Self {
        Self::with_options(comment_registry, FormatterOptions::default())
    }

    #[must_use]
    pub fn with_options(comment_registry: CommentRegistry, options: FormatterOptions) -> Self {
        Self {
            pending_empty_lines: 0,
            writer: Writer::new(),
            // Wrap in Rc to enable cheap cloning when creating temporary formatters
            // for alignment measurements (see measure_alignment_prefix)
            comment_registry: Rc::new(comment_registry),
            options,
            alignment_state: None,
        }
    }

    fn with_shared_deps(comment_registry: Rc<CommentRegistry>, options: FormatterOptions) -> Self {
        Self {
            pending_empty_lines: 0,
            writer: Writer::new(),
            comment_registry,
            options,
            alignment_state: None,
        }
    }

    pub fn format(&mut self, node: &PerlNode) -> String {
        self.format_node(node);
        self.writer.finish()
    }

    fn remember_token(&mut self, token: &SyntaxToken<PerlLanguage>) {
        let kind = token.kind();
        self.writer.set_prev_token_kind(Some(kind));

        if kind.is_trivia() {
            return;
        }

        if kind == SyntaxKind::COMMENT {
            if let Some(comment_id) = CommentId::from_token(token) {
                match self.comment_registry.placement_of(comment_id) {
                    Some(CommentPlacement::Leading(_)) | Some(CommentPlacement::Standalone) => {}
                    Some(CommentPlacement::Trailing(_))
                    | Some(CommentPlacement::Dangling(_))
                    | None => {
                        self.writer.set_last_significant_token_kind(Some(kind));
                    }
                }
            } else {
                self.writer.set_last_significant_token_kind(Some(kind));
            }
        } else {
            self.writer.set_last_significant_token_kind(Some(kind));
        }
    }

    fn node_has_leading_comment(&self, node: &PerlNode) -> bool {
        let owner = CommentOwner::for_node(node);
        self.comment_registry
            .attached_to(owner)
            .any(|assignment| assignment.placement().is_leading())
    }

    fn should_isolate_comment(&self, token: &SyntaxToken<PerlLanguage>) -> bool {
        let Some(comment_id) = CommentId::from_token(token) else {
            return false;
        };

        if !self.comment_registry.is_first_in_block(comment_id) {
            return false;
        }

        let Some(block_id) = self.comment_registry.block_of(comment_id) else {
            return false;
        };

        let Some(CommentPlacement::Leading(owner)) =
            self.comment_registry.placement_of_block(block_id)
        else {
            return false;
        };

        let Some(root) = Self::comment_root(token) else {
            return false;
        };

        matches!(
            owner.resolve(&root),
            Some(CommentAnchor::Node(node)) if node.kind() == SyntaxKind::SUB_DEF
        )
    }

    fn comment_root(token: &SyntaxToken<PerlLanguage>) -> Option<PerlNode> {
        let mut node = token.parent()?;
        while let Some(parent) = node.parent() {
            node = parent;
        }
        Some(node)
    }

    fn format_node(&mut self, node: &PerlNode) {
        self.format_node_with_context(node, FormatContext::default())
    }

    fn format_node_with_context(&mut self, node: &PerlNode, ctx: FormatContext) {
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
                self.format_block(node);
                return;
            }
            SyntaxKind::USE_STMT | SyntaxKind::NO_STMT => {
                self.format_use_no_stmt(node);
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
                self.format_hash_ref(node);
                return;
            }
            SyntaxKind::ARRAY_REF => {
                self.format_array_ref(node);
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
                self.format_quote_like(node);
                return;
            }

            SyntaxKind::DATA_SECTION => {
                self.format_data_section(node);
                return;
            }
            SyntaxKind::LABELED_STMT => {
                self.format_labeled_stmt(node);
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
                self.format_sub_prototype(node);
                return;
            }
            SyntaxKind::SUB_SIGNATURE => {
                self.format_sub_signature(node);
                return;
            }
            SyntaxKind::SIGNATURE_PARAM => {
                // Use default child iteration - spacing managed by general rules
            }
            SyntaxKind::SIGNATURE_DEFAULT => {
                self.format_signature_default(node);
                return;
            }
            SyntaxKind::FOR_STMT => {
                self.format_for_stmt(node);
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
            | SyntaxKind::BACKTICK_EXPR => {
                self.format_expr(node);
                return;
            }
            SyntaxKind::BLOCK_STMT => {
                // Special handling for BLOCK_STMT: detect empty lines between statements
                self.format_block(node);
                return;
            }
            _ => {
                if self.should_use_parenthesized_formatter(node) {
                    self.format_parenthesized_expr(node);
                    return;
                }
            }
        }

        // Default child iteration
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    // Output pending empty lines before processing child nodes
                    if self.pending_empty_lines > 0
                        && (child_node.kind().is_phase_block_stmt()
                            || matches!(child_node.kind(), SyntaxKind::STMT | SyntaxKind::VAR_DECL))
                    {
                        self.output_pending_empty_lines();
                    }
                    self.format_node_with_context(&child_node, ctx);
                }
                NodeOrToken::Token(token) => self.format_token_with_context(&token, ctx),
            }
        }

        // Add empty line after subs, use, and no statements, but only if there are siblings
        if node.kind().is_phase_block_stmt()
            || matches!(
                node.kind(),
                SyntaxKind::SUB_DEF | SyntaxKind::USE_STMT | SyntaxKind::NO_STMT
            )
        {
            self.add_empty_line_after_if_needed(node);
        }

        // Special handling after children are processed
        if node.kind().is_variable() {
            // This is the logic from format_variable
            self.writer.set_prev_token_kind(Some(node.kind()));
        }
    }

    fn format_expr_list_node(&mut self, node: &PerlNode, ctx: FormatContext) {
        let elements: Vec<_> = node.children_with_tokens().collect();

        let mut set_local_alignment = false;
        if self.alignment_state.is_none() {
            if let Some(state) = self.collect_expr_list_alignment_state(node, &elements) {
                self.alignment_state = Some(state);
                set_local_alignment = true;
            }
        }

        for element in elements {
            match element {
                NodeOrToken::Node(child) => self.format_node_with_context(&child, ctx),
                NodeOrToken::Token(token) => self.format_token_with_context(&token, ctx),
            }
        }

        // Reset alignment state only if we set it locally, to prevent it from affecting subsequent nodes
        if set_local_alignment {
            self.alignment_state = None;
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

    fn has_newline_before_first_value_iter(
        &self,
        iter: SyntaxElementChildren<PerlLanguage>,
    ) -> bool {
        for child in iter {
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

    fn is_simple_block(&self, node: &PerlNode) -> bool {
        let mut statement_count = 0;
        let mut consecutive_newlines = 0;
        let mut has_empty_line = false;

        for element in node.descendants_with_tokens() {
            match element {
                NodeOrToken::Node(child) => {
                    if child.parent().as_ref().is_some_and(|parent| parent == node) {
                        if child.kind().is_phase_block_stmt() {
                            return false;
                        }

                        if matches!(child.kind(), SyntaxKind::STMT | SyntaxKind::VAR_DECL) {
                            statement_count += 1;
                            if statement_count > 1 {
                                return false;
                            }
                        }
                    }
                }
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::NEWLINE {
                        consecutive_newlines += 1;
                        // Two consecutive newlines indicate an empty line
                        if consecutive_newlines >= 2 {
                            has_empty_line = true;
                        }
                    } else if !matches!(token.kind(), SyntaxKind::WHITESPACE) {
                        consecutive_newlines = 0;
                    }

                    if matches!(token.kind(), T![;] | SyntaxKind::COMMENT) {
                        return false;
                    }
                }
            }
        }

        !has_empty_line
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

        if matches!(prev_kind, IDENT | QUALIFIED_IDENT)
            && !ctx.in_multiline_context
            && !matches!(current, SEMICOLON | COMMA)
        {
            return true;
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

    fn format_token(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        self.format_token_with_context(token, FormatContext::default())
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
                if let Some(pad) = state.next_pad() {
                    if pad > 0 {
                        let spaces = " ".repeat(pad);
                        self.writer.write_str(&spaces, None);
                    }
                }

                if state.is_empty() {
                    self.alignment_state = None;
                }
            }
        }
    }

    fn format_token_with_context(
        &mut self,
        token: &SyntaxToken<crate::PerlLanguage>,
        ctx: FormatContext,
    ) {
        let kind = token.kind();
        let text = token.text();

        match kind {
            SyntaxKind::WHITESPACE => {}
            SyntaxKind::NEWLINE => {
                if ctx.suppress_newlines {
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

                if self.should_isolate_comment(token) {
                    self.add_empty_line_before();
                }

                // コメントは保持するが、適切な位置に配置
                if self.writer.at_line_start() {
                    self.writer.add_indent();
                    self.writer.set_at_line_start(false);
                } else {
                    // This is an inline comment - add a space before it
                    self.writer.write_char(' ');
                }
                self.apply_alignment_padding(token);
                self.writer.write_str(text.trim(), Some(kind));
                self.writer.handle_user_newline();
                self.remember_token(token);
            }
            SyntaxKind::HEREDOC_CONTENT | SyntaxKind::HEREDOC_END => {
                self.writer.write_str(text, Some(kind));
                self.remember_token(token);
            }
            T!['}'] => {
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
                let should_skip_newline = next_token.as_ref().is_some_and(|t| {
                    match t.kind() {
                        T![elsif]
                        | T![else]
                        | T![catch]
                        | T![finally]
                        | T![when]
                        | T![default]
                        | T![;]
                        | T![,]
                        | T!['('] => true,
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
            T![;] => {
                // Check if the next token (after whitespace) is a comment
                if let Some(next) = Self::next_token_after_whitespace(token) {
                    if next.kind() == SyntaxKind::COMMENT {
                        // Don't add newline here - the comment will handle it
                        return;
                    }
                }
                self.writer.handle_formatter_newline();
            }
            T!['{'] => {
                self.writer.increase_indent();
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
    let comment_registry = CommentRegistry::from_syntax(&root);
    let mut formatter = Formatter::with_options(comment_registry, options);
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
