use crate::{PerlLanguage, PerlNode, SyntaxKind};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineBreakSource {
    User,
    Formatter,
}

pub struct Formatter {
    current_line: Line,
    lines: Vec<Line>,
    indent_level: usize,
    indent_string: String,
    prev_token_kind: Option<SyntaxKind>,
    at_line_start: bool,
    pending_empty_lines: usize, // Number of empty lines waiting to be output
    in_multiline_context: bool, // Track when we're in structured multiline formatting
    last_line_break_was_user: bool,
    user_newlines_to_skip: usize,
    prev_brace_was_statement_level: bool, // Track if the previous R_BRACE closed a statement-level construct
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

impl Formatter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_line: Line::new(),
            lines: Vec::new(),
            indent_level: 0,
            indent_string: "    ".to_string(), // 4 spaces
            prev_token_kind: None,
            at_line_start: true,
            pending_empty_lines: 0,
            in_multiline_context: false,
            last_line_break_was_user: false,
            user_newlines_to_skip: 0,
            prev_brace_was_statement_level: false,
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
                self.handle_newline(LineBreakSource::Formatter);
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
            self.handle_newline(LineBreakSource::Formatter);
        } else {
            self.current_line.text.push(ch);
        }
    }

    fn handle_user_newline_from_token(&mut self) {
        // Handle a user-supplied newline directly without affecting the skip counter
        // This is equivalent to handle_newline(User) but without incrementing user_newlines_to_skip
        self.last_line_break_was_user = true;
        let line = std::mem::take(&mut self.current_line);
        self.lines.push(line);
        self.at_line_start = true;
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

    fn format_node(&mut self, node: &PerlNode) {
        // Add empty line before subs, use statements, and regular statements when appropriate
        // This preserves existing behavior for simple cases while also handling statement spacing
        if matches!(
            node.kind(),
            SyntaxKind::SUB_DEF
                | SyntaxKind::USE_STMT
                | SyntaxKind::NO_STMT
                | SyntaxKind::STMT
                | SyntaxKind::LABELED_STMT
                | SyntaxKind::DECLARATION_STMT
                | SyntaxKind::ELLIPSIS_STMT
                | SyntaxKind::EMPTY_STMT
        ) {
            self.add_empty_line_before_if_needed(node);
        }

        // Track statement-level context for logical continuation indent decisions
        match node.kind() {
            // Statement-level constructs that end with closing braces
            SyntaxKind::IF_STMT | SyntaxKind::UNLESS_STMT | SyntaxKind::WHILE_STMT 
            | SyntaxKind::UNTIL_STMT | SyntaxKind::FOR_STMT | SyntaxKind::SUB_DEF 
            | SyntaxKind::BLOCK_STMT => {
                self.prev_brace_was_statement_level = true;
            }
            // Expression-level constructs that end with closing braces
            SyntaxKind::HASH_REF | SyntaxKind::ARRAY_REF | SyntaxKind::ANON_SUB_EXPR 
            | SyntaxKind::TYPEGLOB_EXPR | SyntaxKind::BLOCK_FUNCTION_CALL_EXPR => {
                self.prev_brace_was_statement_level = false;
            }
            _ => {
                // For other constructs, don't change the flag
            }
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
            SyntaxKind::ANON_SUB_EXPR
            | SyntaxKind::TYPEGLOB_EXPR
            | SyntaxKind::BLOCK_FUNCTION_CALL_EXPR
            | SyntaxKind::METHOD_CALL_EXPR
            | SyntaxKind::HASH_REF_ACCESS_EXPR
            | SyntaxKind::ARRAY_REF_ACCESS_EXPR
            | SyntaxKind::CODE_REF_CALL_EXPR
            | SyntaxKind::HASH_SUBSCRIPTION_EXPR
            | SyntaxKind::ARRAY_SUBSCRIPTION_EXPR
            | SyntaxKind::DEREF_EXPR
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
                // Check if this node contains parentheses that should be formatted multiline
                if self.should_format_parentheses_multiline(node) {
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
                        && matches!(
                            child_node.kind(),
                            SyntaxKind::STMT | SyntaxKind::DECLARATION_STMT
                        )
                    {
                        self.output_pending_empty_lines();
                    }
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => self.format_token(&token),
            }
        }

        // Add empty line after subs, use, and no statements, but only if there are siblings
        if matches!(
            node.kind(),
            SyntaxKind::SUB_DEF | SyntaxKind::USE_STMT | SyntaxKind::NO_STMT
        ) {
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
        // Check if a block contains only a single expression without semicolon or comments

        let statement_count = node
            .children()
            .filter(|child| {
                matches!(
                    child.kind(),
                    SyntaxKind::STMT | SyntaxKind::DECLARATION_STMT
                )
            })
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
        // For empty blocks, use {} without spaces

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
                            if has_content {
                                self.write_char(' '); // Add space after opening brace only if there's content
                            }
                            self.prev_token_kind = Some(token.kind());
                        }
                        SyntaxKind::R_BRACE => {
                            if has_content && self.prev_token_kind != Some(SyntaxKind::L_BRACE) {
                                self.write_char(' '); // Add space before closing brace only if there's content
                            }
                            self.write(&token);
                            self.prev_token_kind = Some(token.kind());
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
                            self.handle_newline(LineBreakSource::Formatter);
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
        let kind = token.kind();

        self.write(token);
        self.indent_level += 1;
        self.handle_newline(LineBreakSource::Formatter);
        self.prev_token_kind = Some(kind);
    }

    fn handle_multiline_closing_delimiter(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        let kind = token.kind();

        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
        if !self.at_line_start || !self.current_line.text.is_empty() {
            self.handle_newline(LineBreakSource::Formatter);
        }
        self.add_indent();
        self.write(token);
        self.at_line_start = false;
        self.prev_token_kind = Some(kind);
    }

    fn should_format_parentheses_multiline(&self, node: &PerlNode) -> bool {
        // Check if this node contains parentheses with newlines that should be multiline formatted
        self.has_newline_before_first_value(node)
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
        if !self.at_line_start || !self.last_line_break_was_user {
            return false;
        }

        match self.prev_token_kind {
            None => return false,
            Some(SyntaxKind::L_BRACE) => return false,
            Some(SyntaxKind::SEMICOLON) => return false,
            Some(SyntaxKind::COMMENT) => {
                // Allow continuation indent for inline comments, prevent for standalone comments
                return self.is_prev_comment_inline();
            }
            Some(SyntaxKind::R_BRACE) => {
                // Suppress continuation indent for else/elsif keywords so they align with their if
                if matches!(current, SyntaxKind::ELSE_KW | SyntaxKind::ELSIF_KW) {
                    return false; // These should align with the if, not be indented
                }
                if matches!(current, SyntaxKind::ARROW | SyntaxKind::DOT) {
                    return true; // Method chaining
                }
                // For postfix modifier keywords, use logical statement-level context
                if matches!(
                    current,
                    SyntaxKind::IF_KW
                        | SyntaxKind::UNLESS_KW
                        | SyntaxKind::WHILE_KW
                        | SyntaxKind::UNTIL_KW
                        | SyntaxKind::FOR_KW
                        | SyntaxKind::FOREACH_KW
                ) {
                    // Logical approach: if the previous brace closed a statement-level construct,
                    // then this keyword is a new statement (no continuation indent).
                    // Otherwise, it's likely a postfix modifier (needs continuation indent).
                    return !self.prev_brace_was_statement_level;
                }
                return false; // Default: no continuation
            }
            _ => {}
        }

        // Never apply continuation indent to closing delimiters
        if matches!(
            current,
            SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE
        ) {
            return false;
        }

        true
    }

    fn is_prev_comment_inline(&self) -> bool {
        // Check if the previous comment was inline AND if it's in a continuation context
        // Comments after complete statements (ending with semicolon) should not allow continuation
        if let Some(last_line) = self.lines.last() {
            let mut has_content_before_comment = false;
            let mut has_semicolon_before_comment = false;

            // Look for content before any comment token
            for token_span in &last_line.tokens {
                if token_span.kind == SyntaxKind::COMMENT {
                    if token_span.start_byte > 0 {
                        let content_before = last_line.text[..token_span.start_byte].trim();
                        has_content_before_comment = !content_before.is_empty();
                        has_semicolon_before_comment = content_before.ends_with(';') || content_before.ends_with('}');
                    }
                    break;
                }
            }

            // Only allow continuation if:
            // 1. There's content before the comment (it's inline)
            // 2. The content doesn't end with a statement terminator (like ';' or '}') (not a complete statement)
            return has_content_before_comment && !has_semicolon_before_comment;
        }
        // If we can't determine, be conservative and don't allow continuation
        false
    }


    fn format_token(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        let kind = token.kind();
        let text = token.text();

        match kind {
            SyntaxKind::WHITESPACE => {}
            SyntaxKind::NEWLINE => {
                if self.at_line_start && self.current_line.text.is_empty() {
                    if self.user_newlines_to_skip > 0 {
                        self.user_newlines_to_skip -= 1;
                    } else if self.pending_empty_lines == 0 {
                        self.pending_empty_lines = 1;
                    }
                } else {
                    self.handle_user_newline_from_token();
                }
            }
            SyntaxKind::COMMENT => {
                // コメントは保持するが、適切な位置に配置
                if self.at_line_start {
                    self.add_indent();
                    self.at_line_start = false;
                } else {
                    // This is an inline comment - add a space before it
                    self.write_char(' ');
                }
                self.write_str(text.trim(), Some(kind));
                self.handle_newline(LineBreakSource::User);
                self.prev_token_kind = Some(kind);
            }
            SyntaxKind::HEREDOC_CONTENT | SyntaxKind::HEREDOC_END => {
                self.write_str(text, Some(kind));
                self.prev_token_kind = Some(kind);
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
                            | SyntaxKind::SEMICOLON
                            | SyntaxKind::L_PAREN
                    )
                ) {
                    self.handle_newline(LineBreakSource::Formatter);
                }

                self.prev_token_kind = Some(kind);
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
                self.prev_token_kind = Some(kind);
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
                self.handle_newline(LineBreakSource::Formatter);
            }
            SyntaxKind::L_BRACE => {
                self.indent_level += 1;
                self.handle_newline(LineBreakSource::Formatter);
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
                                        self.handle_newline(LineBreakSource::Formatter);
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
                            self.handle_user_newline_from_token();
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
                    self.handle_newline(LineBreakSource::Formatter);
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
}

#[must_use]
pub fn format(node: &PerlNode) -> String {
    let mut formatter = Formatter::new();
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
