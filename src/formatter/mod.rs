use crate::{PerlLanguage, PerlNode, SyntaxKind};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxToken};

pub struct Formatter {
    current_line: String,
    lines: Vec<String>,
    indent_level: usize,
    indent_string: String,
    prev_token_kind: Option<SyntaxKind>,
    at_line_start: bool,
    pending_empty_lines: usize, // Number of empty lines waiting to be output
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
            current_line: String::new(),
            lines: Vec::new(),
            indent_level: 0,
            indent_string: "    ".to_string(), // 4 spaces
            prev_token_kind: None,
            at_line_start: true,
            pending_empty_lines: 0,
        }
    }

    pub fn format(&mut self, node: &PerlNode) -> String {
        self.format_node(node);
        self.lines.push(std::mem::take(&mut self.current_line));
        std::mem::take(&mut self.lines).join("\n")
    }

    pub(super) fn write(&mut self, text: &str) {
        let mut parts = text.split('\n');
        if let Some(first) = parts.next() {
            self.current_line.push_str(first);
            for part in parts {
                self.handle_newline();
                self.current_line.push_str(part);
            }
        }
    }

    pub(super) fn write_char(&mut self, ch: char) {
        if ch == '\n' {
            self.handle_newline();
        } else {
            self.current_line.push(ch);
        }
    }

    pub(super) fn is_output_empty(&self) -> bool {
        self.lines.is_empty() && self.current_line.is_empty()
    }

    pub(super) fn ends_with_newline(&self) -> bool {
        self.current_line.is_empty()
    }

    pub(super) fn ends_with_double_newline(&self) -> bool {
        self.current_line.is_empty() && self.lines.last().map(|l| l.is_empty()).unwrap_or(false)
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
        ) {
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
                // Output pending empty lines before processing use/no statement
                if self.pending_empty_lines > 0 {
                    self.output_pending_empty_lines();
                }

                // Special handling for use/no statements: add space between identifier and parentheses
                for child in node.children_with_tokens() {
                    let is_module_name = match &child {
                        NodeOrToken::Node(n) => n.kind() == SyntaxKind::QUALIFIED_IDENT,
                        NodeOrToken::Token(t) => t.kind() == SyntaxKind::IDENT,
                    };

                    match &child {
                        NodeOrToken::Node(n) => self.format_node(n),
                        NodeOrToken::Token(t) => self.format_token(t),
                    }

                    if is_module_name {
                        let last_token = match &child {
                            NodeOrToken::Node(n) => n.last_token(),
                            NodeOrToken::Token(t) => Some(t.clone()),
                        };
                        if let Some(last_token) = last_token {
                            if let Some(next_token) = Self::next_significant_token(&last_token) {
                                if next_token.kind() == SyntaxKind::L_PAREN {
                                    self.write_char(' ');
                                }
                            }
                        }
                    }
                }
                return;
            }
            SyntaxKind::HASH_REF => {
                self.format_hash_ref(node);
                return;
            }
            SyntaxKind::ARRAY_REF => {
                self.format_array_ref(node);
                return;
            }
            SyntaxKind::QW_EXPR => {
                self.format_qw_expr(node);
                return;
            }
            SyntaxKind::Q_EXPR => {
                self.format_q_expr(node);
                return;
            }
            SyntaxKind::QQ_EXPR => {
                self.format_qq_expr(node);
                return;
            }
            SyntaxKind::QX_EXPR => {
                self.format_qx_expr(node);
                return;
            }
            SyntaxKind::M_EXPR => {
                self.format_m_expr(node);
                return;
            }
            SyntaxKind::QR_EXPR => {
                self.format_qr_expr(node);
                return;
            }
            SyntaxKind::S_EXPR => {
                self.format_s_expr(node);
                return;
            }
            SyntaxKind::TR_EXPR => {
                self.format_tr_expr(node);
                return;
            }
            SyntaxKind::ANON_SUB_EXPR => {
                self.format_anon_sub_expr(node);
                return;
            }
            SyntaxKind::TYPEGLOB_EXPR => {
                self.format_typeglob_expr(node);
                return;
            }
            SyntaxKind::BLOCK_FUNCTION_CALL_EXPR => {
                self.format_block_function_call(node);
                return;
            }
            SyntaxKind::SUB_PROTOTYPE => {
                self.format_sub_prototype(node);
                return;
            }
            SyntaxKind::METHOD_CALL_EXPR => {
                self.format_method_call(node);
                return;
            }
            SyntaxKind::HASH_REF_ACCESS_EXPR => {
                self.format_hash_ref_access(node);
                return;
            }
            SyntaxKind::ARRAY_REF_ACCESS_EXPR => {
                self.format_array_ref_access(node);
                return;
            }
            SyntaxKind::CODE_REF_CALL_EXPR => {
                self.format_code_ref_call(node);
                return;
            }
            SyntaxKind::HASH_SUBSCRIPTION_EXPR => {
                self.format_hash_subscription(node);
                return;
            }
            SyntaxKind::ARRAY_SUBSCRIPTION_EXPR => {
                self.format_array_subscription(node);
                return;
            }
            SyntaxKind::DEREF_EXPR => {
                self.format_deref_expr(node);
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
            SyntaxKind::REGEX_EXPR => {
                // Default handling for regex expressions - just format children
                // The spacing around regex operators is handled in format_token
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
                            self.write(token.text());
                            if has_content {
                                self.write_char(' '); // Add space after opening brace only if there's content
                            }
                            self.prev_token_kind = Some(token.kind());
                        }
                        SyntaxKind::R_BRACE => {
                            if has_content && self.prev_token_kind != Some(SyntaxKind::L_BRACE) {
                                self.write_char(' '); // Add space before closing brace only if there's content
                            }
                            self.write(token.text());
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
    }

    fn format_expr_list_multiline_iter(&mut self, iter: SyntaxElementChildren<PerlLanguage>) {
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
                            self.handle_newline();
                        }
                        _ => {
                            // その他のトークンは通常通り処理
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }

    fn handle_multiline_opening_delimiter(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        let text = token.text();
        let kind = token.kind();

        self.write(text);
        self.indent_level += 1;
        self.handle_newline();
        self.prev_token_kind = Some(kind);
    }

    fn handle_multiline_closing_delimiter(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        let text = token.text();
        let kind = token.kind();

        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
        if !self.at_line_start || !self.current_line.is_empty() {
            self.handle_newline();
        }
        self.add_indent();
        self.write(text);
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

    fn format_token(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        let kind = token.kind();
        let text = token.text();

        match kind {
            SyntaxKind::WHITESPACE => {}
            SyntaxKind::NEWLINE => {
                if self.at_line_start && self.current_line.is_empty() {
                    if self.pending_empty_lines == 0 {
                        self.pending_empty_lines = 1;
                    }
                } else {
                    self.handle_newline();
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
                self.write(text.trim());
                self.handle_newline();
            }
            SyntaxKind::HEREDOC_CONTENT | SyntaxKind::HEREDOC_END => {
                for part in text.split_inclusive('\n') {
                    if let Some(without_newline) = part.strip_suffix('\n') {
                        self.current_line.push_str(without_newline);
                        self.lines.push(std::mem::take(&mut self.current_line));
                        self.at_line_start = true;
                    } else {
                        self.current_line.push_str(part);
                        self.at_line_start = false;
                    }
                }
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

                self.write(text);

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
                    self.handle_newline();
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
                    self.at_line_start = false;
                }

                self.write(text);
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
                self.handle_newline();
            }
            SyntaxKind::L_BRACE => {
                self.indent_level += 1;
                self.handle_newline();
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
                                        self.handle_newline();
                                    }
                                    self.lines.push(String::new());
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
                        let mut total_newlines = 1;

                        while let Some(NodeOrToken::Token(peeked)) = children.peek() {
                            match peeked.kind() {
                                SyntaxKind::NEWLINE => {
                                    children.next();
                                    total_newlines += 1;
                                }
                                SyntaxKind::WHITESPACE => {
                                    children.next();
                                }
                                _ => break,
                            }
                        }

                        if self.at_line_start && self.current_line.is_empty() {
                            if total_newlines > 1 {
                                self.pending_empty_lines = 1;
                            }
                        } else {
                            if total_newlines > 1 {
                                self.pending_empty_lines = 1;
                            }
                            self.handle_newline();
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
                    self.write(token.text());
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
                    self.handle_newline();
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
mod spacing;
mod verbatim;
mod whitespace;

#[cfg(test)]
mod tests;
