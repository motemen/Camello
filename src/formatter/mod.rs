use crate::{
    comments::{CommentAnchor, CommentId, CommentOwner, CommentPlacement, CommentRegistry},
    PerlLanguage, PerlNode, SyntaxKind,
};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxToken};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct TokenSpan {
    kind: SyntaxKind,
    start_byte: usize,
    end_byte: usize,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Line {
    text: String,
    tokens: Vec<TokenSpan>,
}

impl Line {
    fn new() -> Self {
        Self {
            text: String::new(),
            tokens: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineBreakSource {
    User,
    Formatter,
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
            SyntaxKind::L_PAREN | SyntaxKind::R_PAREN => self.parentheses,
            SyntaxKind::L_BRACKET | SyntaxKind::R_BRACKET => self.brackets,
            SyntaxKind::L_BRACE | SyntaxKind::R_BRACE => self.braces,
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

#[derive(Debug, Clone, Default)]
pub struct FormatterOptions {
    pub delimiter_tightness: DelimiterTightnessConfig,
}

impl FormatterOptions {
    #[must_use]
    pub fn with_delimiter_tightness(mut self, config: DelimiterTightnessConfig) -> Self {
        self.delimiter_tightness = config;
        self
    }
}

pub struct Formatter {
    current_line: Line,
    lines: Vec<Line>,
    indent_level: usize,
    indent_string: String,
    prev_token_kind: Option<SyntaxKind>,
    last_significant_token_kind: Option<SyntaxKind>,
    last_line_break: Option<LineBreakSource>,
    at_line_start: bool,
    pending_empty_lines: usize, // Number of empty lines waiting to be output
    in_multiline_context: bool, // Track when we're in structured multiline formatting
    comment_registry: CommentRegistry,
    options: FormatterOptions,
}

impl Formatter {
    #[must_use]
    pub fn new(comment_registry: CommentRegistry) -> Self {
        Self::with_options(comment_registry, FormatterOptions::default())
    }

    #[must_use]
    pub fn with_options(comment_registry: CommentRegistry, options: FormatterOptions) -> Self {
        Self {
            current_line: Line::new(),
            lines: Vec::new(),
            indent_level: 0,
            indent_string: "    ".to_string(), // 4 spaces
            prev_token_kind: None,
            last_significant_token_kind: None,
            last_line_break: None,
            at_line_start: true,
            pending_empty_lines: 0,
            in_multiline_context: false,
            comment_registry,
            options,
        }
    }

    pub fn format(&mut self, node: &PerlNode) -> String {
        self.format_node(node);
        self.lines.push(std::mem::take(&mut self.current_line));
        std::mem::take(&mut self.lines)
            .into_iter()
            .map(|l| l.text)
            .fold(String::new(), |mut acc, line| {
                if !acc.is_empty() {
                    acc.push('\n');
                }
                acc.push_str(&line);
                acc
            })
    }

    pub(super) fn write(&mut self, token: &SyntaxToken<PerlLanguage>) {
        self.write_str(token.text(), Some(token.kind()));
    }

    pub(super) fn write_str(&mut self, text: &str, kind: Option<SyntaxKind>) {
        let mut is_first_part = true;
        for part in text.split('\n') {
            if is_first_part {
                is_first_part = false;
            } else {
                self.handle_user_newline();
            }

            if !part.is_empty() {
                // Only add indentation for content tokens like heredocs and strings
                // when at line start and not for structural tokens
                if self.at_line_start && kind.is_some_and(|k| k.is_content_token()) {
                    self.add_indent();
                }
                let start = self.current_line.text.len();
                self.current_line.text.push_str(part);
                self.at_line_start = false;
                if let Some(kind) = kind {
                    let end = self.current_line.text.len();
                    self.current_line.tokens.push(TokenSpan {
                        kind,
                        start_byte: start,
                        end_byte: end,
                    });
                }
            }
        }
    }

    pub(super) fn write_char(&mut self, ch: char) {
        if ch == '\n' {
            self.handle_formatter_newline();
        } else {
            self.current_line.text.push(ch);
        }
    }

    fn remember_token(&mut self, token: &SyntaxToken<PerlLanguage>) {
        self.prev_token_kind = Some(token.kind());
        self.update_last_significant_token(token);
    }

    fn update_last_significant_token(&mut self, token: &SyntaxToken<PerlLanguage>) {
        let kind = token.kind();

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
                        self.last_significant_token_kind = Some(kind);
                    }
                }
            } else {
                self.last_significant_token_kind = Some(kind);
            }
        } else {
            self.last_significant_token_kind = Some(kind);
        }
    }

    pub(super) fn is_output_empty(&self) -> bool {
        self.lines.is_empty() && self.current_line.text.is_empty()
    }

    pub(super) fn ends_with_newline(&self) -> bool {
        self.current_line.text.is_empty()
    }

    pub(super) fn ends_with_double_newline(&self) -> bool {
        self.current_line.text.is_empty()
            && self
                .lines
                .last()
                .map(|l| l.text.is_empty())
                .unwrap_or(false)
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

            SyntaxKind::IO_EXPR => {
                self.format_io_expr(node);
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
            SyntaxKind::FOR_STMT => {
                self.format_for_stmt(node);
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
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => self.format_token(&token),
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
            self.prev_token_kind = Some(node.kind());
        }
    }

    fn has_newline_before_first_value(&self, node: &PerlNode) -> bool {
        // Check if there's a newline between the opening delimiter and the first non-trivial token
        let mut children = node.children_with_tokens();

        // Find the opening delimiter.
        if !children.by_ref().any(|child| {
            matches!(
                child.as_token().map(rowan::SyntaxToken::kind),
                Some(
                    SyntaxKind::L_BRACE
                        | SyntaxKind::L_BRACKET
                        | SyntaxKind::L_PAREN
                        | SyntaxKind::DELIMITER
                )
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
                    if matches!(
                        token.kind(),
                        SyntaxKind::L_BRACE | SyntaxKind::L_BRACKET | SyntaxKind::L_PAREN
                    ) {
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
        if node
            .children()
            .any(|child| child.kind().is_phase_block_stmt())
        {
            return false;
        }

        // Check if a block contains only a single expression without semicolon or comments

        let statement_count = node
            .children()
            .filter(|child| matches!(child.kind(), SyntaxKind::STMT | SyntaxKind::VAR_DECL))
            .count();

        // Simple if: 1 or fewer statements AND no semicolons or comments anywhere
        if statement_count > 1 {
            return false;
        }

        !node.descendants_with_tokens().any(|element| {
            element.as_token().is_some_and(|token| {
                matches!(token.kind(), SyntaxKind::SEMICOLON | SyntaxKind::COMMENT)
            })
        })
    }

    fn format_simple_block(&mut self, node: &PerlNode) {
        // Format a simple block on a single line: { expression }
        // Always emit spaces around the braces to match inline block style

        let mut has_content = false;

        // First pass: check if the block has any content
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(_) => {
                    has_content = true;
                }
                NodeOrToken::Token(token) => {
                    match token.kind() {
                        SyntaxKind::L_BRACE
                        | SyntaxKind::R_BRACE
                        | SyntaxKind::WHITESPACE
                        | SyntaxKind::NEWLINE => {
                            // These don't count as content
                        }
                        _ => {
                            has_content = true;
                        }
                    }
                }
            }
        }

        // Second pass: format with appropriate spacing
        let brace_tightness = self
            .options
            .delimiter_tightness
            .for_kind(SyntaxKind::L_BRACE);
        let add_space_for_block = brace_tightness.should_add_space_for_simple_block();
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => {
                    match token.kind() {
                        SyntaxKind::L_BRACE => {
                            self.handle_spacing_before(token.kind());
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.write(&token);
                            if add_space_for_block {
                                self.write_char(' ');
                            }
                            self.remember_token(&token);
                        }
                        SyntaxKind::R_BRACE => {
                            if add_space_for_block
                                && has_content
                                && self.prev_token_kind != Some(SyntaxKind::L_BRACE)
                                && !self.current_line.text.ends_with(' ')
                            {
                                self.write_char(' ');
                            }
                            self.write(&token);
                            self.remember_token(&token);
                        }
                        SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {
                            // Skip trivia in simple blocks
                        }
                        _ => {
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }

    fn format_single_line_delimited_children(
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
                        if self.at_line_start {
                            self.add_indent();
                            self.at_line_start = false;
                        }
                        self.write(&token);
                        if add_space {
                            self.write_char(' ');
                        }
                        self.prev_token_kind = Some(kind);
                    } else if let Some(add_space) = close_spacing[index] {
                        if add_space && !self.current_line.text.ends_with(' ') {
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.write_char(' ');
                        }
                        self.write(&token);
                        self.prev_token_kind = Some(kind);
                    } else if skip_whitespace && kind == WHITESPACE {
                        continue;
                    } else {
                        self.format_token(&token);
                    }
                }
            }
        }
    }

    fn format_multiline_delimited(
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

    fn format_multiline_delimited_iter(
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
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
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

    fn handle_multiline_opening_delimiter(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        self.write(token);
        self.indent_level += 1;
        self.handle_formatter_newline();
        self.remember_token(token);
    }

    fn handle_multiline_closing_delimiter(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
        if !self.at_line_start || !self.current_line.text.is_empty() {
            self.handle_formatter_newline();
        }
        self.add_indent();
        self.write(token);
        self.at_line_start = false;
        self.remember_token(token);
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
                | ATTR_ARGS
                | USE_STMT
                | NO_STMT
                | IF_STMT
                | UNLESS_STMT
                | WHILE_STMT
                | UNTIL_STMT
                | FOR_STMT
                | TRY_STMT
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
                    SyntaxKind::L_PAREN => has_open = true,
                    SyntaxKind::R_PAREN => has_close = true,
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

    fn needs_continuation_indent(&self, current: SyntaxKind) -> bool {
        if !self.at_line_start {
            return false;
        }

        if self.last_line_break != Some(LineBreakSource::User) {
            return false;
        }

        let Some(prev_kind) = self.last_significant_token_kind else {
            return false;
        };

        if prev_kind == SyntaxKind::COMMENT {
            return false;
        }

        use SyntaxKind::*;

        if prev_kind == SyntaxKind::PACKAGE_KW {
            return true;
        }

        if matches!(
            current,
            IF_KW | UNLESS_KW | WHILE_KW | UNTIL_KW | FOR_KW | FOREACH_KW
        ) && !matches!(prev_kind, L_BRACE | R_BRACE | SEMICOLON)
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

        if !self.in_multiline_context && matches!(prev_kind, COMMA | FAT_COMMA) {
            return true;
        }

        false
    }

    fn format_token(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        let kind = token.kind();
        let text = token.text();

        match kind {
            SyntaxKind::WHITESPACE => {}
            SyntaxKind::NEWLINE => {
                if self.at_line_start && self.current_line.text.is_empty() {
                    if self.pending_empty_lines == 0 {
                        self.pending_empty_lines = 1;
                    }
                } else {
                    self.handle_user_newline();
                }
            }
            SyntaxKind::COMMENT => {
                self.output_pending_empty_lines();

                if self.should_isolate_comment(token) {
                    self.add_empty_line_before();
                }

                // コメントは保持するが、適切な位置に配置
                if self.at_line_start {
                    self.add_indent();
                    self.at_line_start = false;
                } else {
                    // This is an inline comment - add a space before it
                    self.write_char(' ');
                }
                self.write_str(text.trim(), Some(kind));
                self.handle_user_newline();
                self.remember_token(token);
            }
            SyntaxKind::HEREDOC_CONTENT | SyntaxKind::HEREDOC_END => {
                self.write_str(text, Some(kind));
                self.remember_token(token);
            }
            SyntaxKind::R_BRACE => {
                // 閉じブレースは特別処理：先にインデントを下げる
                if self.indent_level > 0 {
                    self.indent_level -= 1;
                }

                if self.at_line_start {
                    self.add_indent();
                    self.at_line_start = false;
                }

                self.write(token);

                let next_kind = Self::next_significant_token(token).map(|t| t.kind());
                if !matches!(
                    next_kind,
                    Some(
                        SyntaxKind::ELSIF_KW
                            | SyntaxKind::ELSE_KW
                            | SyntaxKind::CATCH_KW
                            | SyntaxKind::FINALLY_KW
                            | SyntaxKind::SEMICOLON
                            | SyntaxKind::L_PAREN
                    )
                ) {
                    self.handle_formatter_newline();
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

                if self.at_line_start && !kind.is_trivia() {
                    self.add_indent();
                    if self.needs_continuation_indent(kind) {
                        self.current_line.text.push_str(&self.indent_string);
                    }
                    self.at_line_start = false;
                }

                let prev_token_kind_before = self.prev_token_kind;
                self.write(token);
                if matches!(kind, SyntaxKind::UNARY_PLUS | SyntaxKind::UNARY_MINUS)
                    && matches!(
                        prev_token_kind_before,
                        Some(SyntaxKind::IDENT | SyntaxKind::QUALIFIED_IDENT)
                    )
                {
                    if let Some(next_token) = Self::next_significant_token(token) {
                        if next_token.kind() == SyntaxKind::L_BRACE {
                            self.write_char(' ');
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
            SyntaxKind::SEMICOLON => {
                // Check if the next token (after whitespace) is a comment
                if let Some(next) = Self::next_token_after_whitespace(token) {
                    if next.kind() == SyntaxKind::COMMENT {
                        // Don't add newline here - the comment will handle it
                        return;
                    }
                }
                self.handle_formatter_newline();
            }
            SyntaxKind::L_BRACE => {
                self.indent_level += 1;
                self.handle_formatter_newline();
            }
            _ => {}
        }
    }

    fn format_block(&mut self, node: &PerlNode) {
        if self.is_simple_block(node) {
            self.format_simple_block(node);
            return;
        }

        // Use a peekable iterator to avoid collecting all children into a Vec,
        // which improves performance and reduces memory allocation.
        let mut children = node.children_with_tokens().peekable();
        let mut prev_node_kind: Option<SyntaxKind> = None;

        while let Some(child) = children.next() {
            match child {
                NodeOrToken::Node(child_node) => {
                    let current_kind = child_node.kind();

                    // Check if we need to add empty line after use/no block
                    if let Some(prev_kind) = prev_node_kind {
                        if (prev_kind == SyntaxKind::USE_STMT || prev_kind == SyntaxKind::NO_STMT)
                            && (current_kind != SyntaxKind::USE_STMT
                                && current_kind != SyntaxKind::NO_STMT)
                        {
                            // We're transitioning from USE_STMT/NO_STMT to a different node type
                            // Check if there are already empty lines from source or pending
                            let has_existing_empty_line =
                                self.pending_empty_lines > 0 || self.ends_with_double_newline();

                            if !has_existing_empty_line {
                                // Add empty line after use/no block
                                if !self.is_output_empty() {
                                    if !self.ends_with_newline() {
                                        self.handle_formatter_newline();
                                    }
                                    self.lines.push(Line::new());
                                }
                            }
                        }
                    }

                    // Output pending empty lines before processing child nodes
                    self.output_pending_empty_lines();
                    self.format_node(&child_node);

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

                        if !self.at_line_start || !self.current_line.text.is_empty() {
                            self.handle_user_newline();
                        }

                        if saw_extra_newline || self.prev_token_kind == Some(SyntaxKind::COMMENT) {
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
                        self.format_token(&token);
                    }
                },
            }
        }
    }

    fn format_label(&mut self, node: &PerlNode) {
        if self.at_line_start {
            self.add_indent();
            self.at_line_start = false;
        }

        let mut last_token_kind = None;
        for child in node.children_with_tokens() {
            if let NodeOrToken::Token(token) = child {
                if !token.kind().is_trivia() {
                    self.write(&token);
                    last_token_kind = Some(token.kind());
                }
            }
        }

        self.prev_token_kind = last_token_kind;
        if let Some(kind) = last_token_kind {
            if !kind.is_trivia() {
                self.last_significant_token_kind = Some(kind);
            }
        }
    }

    fn format_labeled_stmt(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens().peekable();

        if let Some(child) = children.next() {
            match child {
                NodeOrToken::Node(n) => self.format_node(&n),
                NodeOrToken::Token(t) => self.format_token(&t),
            }
        }

        if let Some(child) = children.peek() {
            match child {
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::NEWLINE => {
                    self.handle_user_newline();
                    children.next();
                }
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::WHITESPACE => {
                    self.write_char(' ');
                    self.at_line_start = false;
                    children.next();
                }
                NodeOrToken::Token(_) | NodeOrToken::Node(_) => {
                    self.write_char(' ');
                    self.at_line_start = false;
                }
            }
        }
        self.prev_token_kind = None;

        for child in children {
            match child {
                NodeOrToken::Node(n) => self.format_node(&n),
                NodeOrToken::Token(t) => self.format_token(&t),
            }
        }
    }

    fn format_for_stmt(&mut self, node: &PerlNode) {
        // Handle FOR statement with special semicolon treatment
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => {
                    match token.kind() {
                        SyntaxKind::SEMICOLON => {
                            // In FOR statements, semicolons are followed by space, not newline
                            self.write(&token);
                            self.write_char(' ');
                        }
                        _ => {
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }

    fn format_io_expr(&mut self, node: &PerlNode) {
        // IO expressions are formatted to preserve internal spacing, but need normal
        // spacing before the opening delimiter.

        self.handle_spacing_before(SyntaxKind::LT);

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    if self.at_line_start && !kind.is_trivia() {
                        self.add_indent();
                        self.at_line_start = false;
                    }
                    self.write(&token);
                    self.remember_token(&token);
                }
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
