use crate::{PerlLanguage, PerlNode, SyntaxKind};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxToken};
use std::collections::VecDeque;

// Helper function for checking disallowed tokens
fn has_disallowed_tokens(node: &PerlNode) -> bool {
    node.descendants_with_tokens().any(|element| {
        element.as_token().is_some_and(|token| {
            matches!(token.kind(), SyntaxKind::SEMICOLON | SyntaxKind::COMMENT)
        })
    })
}

// Information about a line that can be aligned
#[derive(Debug, Clone)]
struct AlignmentCandidate {
    line_content: String,     // Content of the line up to the alignment operator
    operator: SyntaxKind,     // The alignment operator (EQ or FAT_COMMA)
    position: usize,          // Column position where operator should be placed
    line_number: usize,       // Line number for debugging
}

// Tracks alignment state
#[derive(Debug, Default)]
struct AlignmentState {
    candidates: VecDeque<AlignmentCandidate>,
    current_line: String,
    current_line_number: usize,
    in_alignment_group: bool,
}

pub struct Formatter {
    output: String,
    indent_level: usize,
    indent_string: String,
    prev_token_kind: Option<SyntaxKind>,
    at_line_start: bool,
    pending_empty_lines: usize, // Number of empty lines waiting to be output
    alignment_state: AlignmentState, // Track alignment candidates
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
            output: String::new(),
            indent_level: 0,
            indent_string: "    ".to_string(), // 4 spaces
            prev_token_kind: None,
            at_line_start: true,
            pending_empty_lines: 0,
            alignment_state: AlignmentState::default(),
        }
    }

    pub fn format(&mut self, node: &PerlNode) -> String {
        self.format_node(node);
        std::mem::take(&mut self.output)
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
                | SyntaxKind::DECLARATION_STMT
        ) {
            self.add_empty_line_before_if_needed(node);
        }

        // Node types that require special handling
        match node.kind() {
            SyntaxKind::ROOT => {
                // Use the same empty line detection logic as BLOCK_STMT for root-level statements
                self.format_block_stmt_with_empty_line_detection(node);
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
                                    self.output.push(' ');
                                }
                            }
                        }
                    }
                }
                return;
            }
            SyntaxKind::PREFIX_EXPR => {
                self.format_prefix_expr(node);
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
            SyntaxKind::DEREF_EXPR => {
                self.format_deref_expr(node);
                return;
            }
            SyntaxKind::REFERENCE_EXPR => {
                self.format_reference_expr(node);
                return;
            }
            SyntaxKind::IO_EXPR => {
                self.format_io_expr(node);
                return;
            }
            SyntaxKind::ANON_SUB_EXPR => {
                self.format_anon_sub_expr(node);
                return;
            }
            SyntaxKind::TERNARY_EXPR => {
                self.format_ternary_expr(node);
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
            SyntaxKind::POSTFIX_DEREF_EXPR => {
                self.format_postfix_deref_expr(node);
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
            SyntaxKind::DATA_SECTION => {
                self.format_data_section(node);
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
                self.format_block_stmt_with_empty_line_detection(node);
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
                    if token.kind() == SyntaxKind::WHITESPACE && token.text().contains('\n') {
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
                    if token.kind() == SyntaxKind::WHITESPACE && token.text().contains('\n') {
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
        statement_count <= 1 && !has_disallowed_tokens(node)
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
                        SyntaxKind::L_BRACE | SyntaxKind::R_BRACE | SyntaxKind::WHITESPACE => {
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
                            self.output.push_str(token.text());
                            if has_content {
                                self.output.push(' '); // Add space after opening brace only if there's content
                            }
                            self.prev_token_kind = Some(token.kind());
                        }
                        SyntaxKind::R_BRACE => {
                            if has_content && self.prev_token_kind != Some(SyntaxKind::L_BRACE) {
                                self.output.push(' '); // Add space before closing brace only if there's content
                            }
                            self.output.push_str(token.text());
                            self.prev_token_kind = Some(token.kind());
                        }
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace in simple blocks
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
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace here - we'll handle newlines in the delimiter handlers
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
                        SyntaxKind::WHITESPACE => {
                            // Skip whitespace here - we'll handle newlines in the delimiter handlers
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

        self.output.push_str(text);
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
        self.handle_newline();
        self.add_indent();
        self.output.push_str(text);
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
            SyntaxKind::WHITESPACE => {
                self.handle_whitespace(token);
            }
            SyntaxKind::COMMENT => {
                // コメントは保持するが、適切な位置に配置
                if self.at_line_start {
                    self.add_indent();
                    self.at_line_start = false;
                } else {
                    // This is an inline comment - add a space before it
                    self.output.push(' ');
                }
                self.output.push_str(text.trim());
                self.handle_newline();
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

                self.output.push_str(text);

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

                self.output.push_str(text);
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

    fn format_block_stmt_with_empty_line_detection(&mut self, node: &PerlNode) {
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
                                self.pending_empty_lines > 0 || self.output.ends_with("\n\n");

                            if !has_existing_empty_line {
                                // Add empty line after use/no block
                                if !self.output.is_empty() {
                                    self.handle_newline();
                                    self.output.push('\n');
                                }
                            }
                        }
                    }

                    // Output pending empty lines before processing child nodes
                    self.output_pending_empty_lines();
                    self.format_node(&child_node);

                    prev_node_kind = Some(current_kind);
                }
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::WHITESPACE {
                        let mut total_newlines = token.text().matches('\n').count();

                        // Look ahead to merge consecutive WHITESPACE tokens
                        while let Some(NodeOrToken::Token(peeked_token)) = children.peek() {
                            if peeked_token.kind() == SyntaxKind::WHITESPACE {
                                // It's a whitespace token, so we consume it and add its newlines
                                let consumed_token = children.next().unwrap().into_token().unwrap();
                                total_newlines += consumed_token.text().matches('\n').count();
                            } else {
                                // Not a whitespace token, so we stop looking ahead
                                break;
                            }
                        }

                        if total_newlines > 0 {
                            // If there are multiple newlines across tokens, preserve as one empty line
                            if total_newlines > 1 {
                                self.pending_empty_lines = 1;
                            }
                            self.handle_newline();
                        }
                    } else {
                        self.output_pending_empty_lines();
                        self.format_token(&token);
                    }
                }
            }
        }
    }

    // ===== Alignment Methods =====
    
    /// Check if we need to start tracking a potential alignment candidate
    fn maybe_start_alignment(&mut self, token_kind: SyntaxKind) {
        if matches!(token_kind, SyntaxKind::EQ | SyntaxKind::FAT_COMMA) {
            // This could be an alignment candidate
            // We'll finalize it when we see the semicolon or comma
            self.alignment_state.in_alignment_group = true;
        }
    }

    /// Check if we need to complete an alignment candidate
    fn maybe_complete_alignment(&mut self, token_kind: SyntaxKind) {
        if !self.alignment_state.in_alignment_group {
            return;
        }

        // Complete alignment on statement terminators
        if matches!(token_kind, SyntaxKind::SEMICOLON | SyntaxKind::COMMA) {
            // Find the alignment operator in current line
            if let Some(pos) = self.find_alignment_operator_in_current_line() {
                let candidate = AlignmentCandidate {
                    line_content: self.alignment_state.current_line.clone(),
                    operator: pos.0,
                    position: pos.1,
                    line_number: self.alignment_state.current_line_number,
                };
                self.alignment_state.candidates.push_back(candidate);
            }
            
            // Reset for next line
            self.alignment_state.in_alignment_group = false;
            self.alignment_state.current_line.clear();
        }
    }

    /// Find alignment operator (= or =>) in the current line buffer
    fn find_alignment_operator_in_current_line(&self) -> Option<(SyntaxKind, usize)> {
        let line = &self.alignment_state.current_line;
        
        // Look for => first (longer pattern)
        if let Some(pos) = line.find("=>") {
            return Some((SyntaxKind::FAT_COMMA, pos));
        }
        
        // Look for = (but not ==, <=, >=, etc.)
        if let Some(pos) = line.find('=') {
            // Make sure it's not part of another operator
            let is_standalone = (pos == 0 || !matches!(line.chars().nth(pos - 1), Some('!' | '<' | '>' | '=' | '~'))) 
                && (pos + 1 >= line.len() || !matches!(line.chars().nth(pos + 1), Some('=' | '~')));
            
            if is_standalone {
                return Some((SyntaxKind::EQ, pos));
            }
        }
        
        None
    }

    /// Reset alignment state (called on empty lines or incompatible statements)
    fn reset_alignment(&mut self) {
        if !self.alignment_state.candidates.is_empty() {
            // Process any pending alignment group
            self.process_alignment_group();
        }
        self.alignment_state.candidates.clear();
        self.alignment_state.current_line.clear();
        self.alignment_state.in_alignment_group = false;
    }

    /// Process and output an alignment group with proper padding
    fn process_alignment_group(&mut self) {
        if self.alignment_state.candidates.len() < 2 {
            // No alignment needed for single lines
            return;
        }

        // Group by operator type - collect into separate vectors to avoid borrowing issues
        let mut eq_candidates = Vec::new();
        let mut fat_comma_candidates = Vec::new();
        
        for candidate in &self.alignment_state.candidates {
            match candidate.operator {
                SyntaxKind::EQ => eq_candidates.push(candidate.clone()),
                SyntaxKind::FAT_COMMA => fat_comma_candidates.push(candidate.clone()),
                _ => {}
            }
        }

        // Process each group separately
        self.align_group(&eq_candidates);
        self.align_group(&fat_comma_candidates);
    }

    /// Align a group of candidates with the same operator
    fn align_group(&mut self, candidates: &[AlignmentCandidate]) {
        if candidates.len() < 2 {
            return;
        }

        // Find the maximum position needed
        let _max_pos = candidates.iter().map(|c| c.position).max().unwrap_or(0);
        
        // This would require more complex rewriting of already-output content
        // For now, let's implement a simpler approach that tracks alignment
        // during the initial formatting pass
        
        // TODO: Implement alignment padding logic
        // This is a complex change that would require buffering output
        // and rewriting with proper padding
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
