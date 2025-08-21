use crate::{PerlLanguage, PerlNode, SyntaxKind};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxToken};

pub struct Formatter {
    output: String,
    indent_level: usize,
    indent_string: String,
    prev_token_kind: Option<SyntaxKind>,
    at_line_start: bool,
    consecutive_newlines: usize,
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

impl Formatter {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            indent_string: "    ".to_string(), // 4スペース
            prev_token_kind: None,
            at_line_start: true,
            consecutive_newlines: 0,
        }
    }

    pub fn format(&mut self, node: &PerlNode) -> String {
        self.format_node(node);
        std::mem::take(&mut self.output)
    }

    fn format_node(&mut self, node: &PerlNode) {
        // Add empty line before subs and use statements only in specific contexts
        // This preserves existing behavior for simple cases
        if matches!(node.kind(), SyntaxKind::SUB_DEF | SyntaxKind::USE_STMT) {
            self.add_empty_line_before_if_needed(node);
        }

        // 特別な処理が必要なノードタイプ
        match node.kind() {
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
            SyntaxKind::DEREF_EXPR => {
                self.format_deref_expr(node);
                return;
            }
            SyntaxKind::FUNCTION_CALL_EXPR => {
                self.format_function_call(node);
                return;
            }
            SyntaxKind::BLOCK_FUNCTION_CALL_EXPR => {
                self.format_block_function_call(node);
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
            SyntaxKind::DATA_SECTION => {
                self.format_data_section(node);
                return;
            }
            SyntaxKind::REGEX_EXPR => {
                // Default handling for regex expressions - just format children
                // The spacing around regex operators is handled in format_token
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
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => self.format_token(&token),
            }
        }

        // Add empty line after subs and use statements, but only if there are siblings
        if matches!(node.kind(), SyntaxKind::SUB_DEF | SyntaxKind::USE_STMT) {
            self.add_empty_line_after_if_needed(node);
        }

        // Special handling after children are processed
        if node.kind().is_variable() {
            // This is the logic from format_variable
            self.prev_token_kind = Some(node.kind());
        }
    }

    /// Format a data section (__END__ or __DATA__)
    /// Data sections should be preserved exactly as-is without any formatting changes
    fn format_data_section(&mut self, node: &PerlNode) {
        // Ensure we're on a new line before the data section
        if !self.at_line_start {
            self.output.push('\n');
            self.at_line_start = true;
        }

        // Process all children (keyword + data content) without any modifications
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Token(token) => {
                    let text = token.text();
                    match token.kind() {
                        SyntaxKind::END_KW | SyntaxKind::DATA_KW => {
                            // Output the keyword exactly as-is
                            self.output.push_str(text);
                        }
                        SyntaxKind::DATA_SECTION => {
                            // Output the data content exactly as-is, preserving all formatting
                            self.output.push_str(text);
                        }
                        _ => {
                            // Handle any other tokens (whitespace, etc.) as-is
                            self.output.push_str(text);
                        }
                    }
                }
                NodeOrToken::Node(_) => {
                    // Data sections shouldn't contain nested nodes, but handle gracefully
                    // by preserving the original text
                }
            }
        }
    }

    fn has_newline_before_first_value(&self, node: &PerlNode) -> bool {
        // Check if there's a newline between the opening delimiter and the first non-trivial token
        let mut children = node.children_with_tokens();

        // Find the opening delimiter.
        if !children.by_ref().any(|child| {
            matches!(
                child.as_token().map(|t| t.kind()),
                Some(SyntaxKind::L_BRACE | SyntaxKind::L_BRACKET | SyntaxKind::L_PAREN)
            )
        }) {
            return false;
        };

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

    fn format_hash_ref(&mut self, node: &PerlNode) {
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_hash_ref(node);
        } else {
            self.format_single_line_hash_ref(node);
        }
    }

    fn format_single_line_hash_ref(&mut self, node: &PerlNode) {
        // ハッシュリファレンスは改行なしでフォーマット
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            // ハッシュリファレンス内の空白は無視
                        }
                        SyntaxKind::L_BRACE => {
                            self.handle_spacing_before(kind);
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::R_BRACE => {
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
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

    fn format_multiline_delimited(
        &mut self,
        node: &PerlNode,
        open_delimiter: SyntaxKind,
        close_delimiter: SyntaxKind,
    ) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => {
                    self.format_node(&node);
                }
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            self.handle_multiline_whitespace(&token);
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

    fn format_multiline_delimited_iter(
        &mut self,
        iter: SyntaxElementChildren<PerlLanguage>,
        open_delimiter: SyntaxKind,
        close_delimiter: SyntaxKind,
    ) {
        for child in iter {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            self.handle_multiline_whitespace(&token);
                        }
                        k if k == open_delimiter => {
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

    fn format_multiline_hash_ref(&mut self, node: &PerlNode) {
        self.format_multiline_delimited(node, SyntaxKind::L_BRACE, SyntaxKind::R_BRACE);
    }

    fn format_array_ref(&mut self, node: &PerlNode) {
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_array_ref(node);
        } else {
            self.format_single_line_array_ref(node);
        }
    }

    fn format_single_line_array_ref(&mut self, node: &PerlNode) {
        // 配列リファレンスは改行なしでフォーマット
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            // 配列リファレンス内の空白は無視
                        }
                        SyntaxKind::L_BRACKET => {
                            self.handle_spacing_before(kind);
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::R_BRACKET => {
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
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

    fn format_multiline_array_ref(&mut self, node: &PerlNode) {
        self.format_multiline_delimited(node, SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET);
    }

    fn format_qw_expr(&mut self, node: &PerlNode) {
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_qw_expr(node);
        } else {
            self.format_single_line_qw_expr(node);
        }
    }

    fn format_single_line_qw_expr(&mut self, node: &PerlNode) {
        // qw() 式の特別フォーマット
        let mut first_word = true;

        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        SyntaxKind::QW_KW => {
                            self.format_token(&token);
                        }
                        SyntaxKind::L_PAREN | SyntaxKind::L_BRACKET | SyntaxKind::L_BRACE => {
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::QW_STRING => {
                            // QW_STRINGの間には空白を追加
                            if !first_word {
                                self.output.push(' ');
                            }
                            self.output.push_str(text);
                            first_word = false;
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE => {
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::WHITESPACE => {
                            // qw() 内の空白は制御下でスキップ
                        }
                        _ => {
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }

    fn format_multiline_qw_expr(&mut self, node: &PerlNode) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        SyntaxKind::QW_KW => {
                            self.format_token(&token);
                        }
                        SyntaxKind::L_PAREN | SyntaxKind::L_BRACKET | SyntaxKind::L_BRACE => {
                            self.handle_multiline_opening_delimiter(&token);
                        }
                        SyntaxKind::QW_STRING => {
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.output.push_str(text);
                            self.handle_newline();
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE => {
                            self.handle_multiline_closing_delimiter(&token);
                        }
                        SyntaxKind::WHITESPACE => {
                            self.handle_multiline_whitespace(&token);
                        }
                        _ => {
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

    fn handle_multiline_whitespace(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        let text = token.text();

        // In multiline mode, handle whitespace for proper newlines
        if text.contains('\n') {
            self.handle_newline();
        }
    }

    fn format_q_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::Q_KW, SyntaxKind::Q_STRING);
    }

    fn format_qq_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QQ_KW, SyntaxKind::QQ_STRING);
    }

    fn format_qx_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QX_KW, SyntaxKind::QX_STRING);
    }

    fn format_m_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::M_KW, SyntaxKind::M_STRING);
    }

    fn format_qr_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QR_KW, SyntaxKind::QR_STRING);
    }

    fn format_q_family_expr(
        &mut self,
        node: &PerlNode,
        kw_kind: SyntaxKind,
        string_kind: SyntaxKind,
    ) {
        // q-family expressions always format as single line
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();
                    match kind {
                        k if k == kw_kind => {
                            self.format_token(&token);
                        }
                        SyntaxKind::L_PAREN
                        | SyntaxKind::L_BRACKET
                        | SyntaxKind::L_BRACE
                        | SyntaxKind::SLASH => {
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                        k if k == string_kind => {
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE => {
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Preserve whitespace in q-family strings
                            self.output.push_str(text);
                        }
                        _ => {
                            // Handle any remaining tokens (including closing slash) directly
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                    }
                }
            }
        }
    }

    fn format_deref_expr(&mut self, node: &PerlNode) {
        // デリファレンス式（例: @$var, %$var, $$var）のフォーマット
        // 全ての子要素を空白なしで連続出力
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    // デリファレンス式では空白を入れずに続ける
                    match kind {
                        SyntaxKind::WHITESPACE => {
                            // デリファレンス式内の空白はスキップ
                        }
                        _ => {
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                    }
                }
            }
        }
    }

    fn format_function_call(&mut self, node: &PerlNode) {
        // Format function call: function_name arg1, arg2, arg3
        // Ensure proper spacing: space after function name, space after commas
        // Handle multiline parentheses for function parameters
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_function_call(node);
        } else {
            self.format_single_line_function_call(node);
        }
    }

    fn should_format_parentheses_multiline(&self, node: &PerlNode) -> bool {
        // Check if this node contains parentheses with newlines that should be multiline formatted
        self.has_newline_before_first_value(node)
    }

    fn format_parenthesized_expr(&mut self, node: &PerlNode) {
        // Format any parenthesized expression with proper multiline indentation
        self.format_multiline_delimited(node, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    fn format_single_line_function_call(&mut self, node: &PerlNode) {
        // Format function call on a single line
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child_node) => {
                    self.format_node(&child_node);
                }
                NodeOrToken::Token(token) => {
                    self.format_token(&token);
                }
            }
        }
    }

    fn format_multiline_function_call(&mut self, node: &PerlNode) {
        // Format function call with multiline parentheses
        self.format_multiline_delimited(node, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    fn format_block_function_call(&mut self, node: &PerlNode) {
        // Format block function call: function_name { ... } additional_args
        // Keep short blocks on same line, longer blocks with proper indentation

        let children = node.children_with_tokens().peekable();

        for child in children {
            match child {
                NodeOrToken::Node(child_node) => {
                    match child_node.kind() {
                        SyntaxKind::BLOCK_STMT => {
                            // Check if this is a simple, short block
                            if self.is_simple_block(&child_node) {
                                self.format_simple_block(&child_node);
                            } else {
                                self.format_node(&child_node);
                            }
                        }
                        _ => {
                            self.format_node(&child_node);
                        }
                    }
                }
                NodeOrToken::Token(token) => {
                    self.format_token(&token);
                }
            }
        }
    }

    fn is_simple_block(&self, block_node: &PerlNode) -> bool {
        // Consider a block simple if it has only one statement and is relatively short
        let statements: Vec<_> = block_node
            .children()
            .filter(|child| {
                child.kind() == SyntaxKind::STMT || child.kind() == SyntaxKind::DECLARATION_STMT
            })
            .collect();

        statements.len() <= 1
    }

    fn format_simple_block(&mut self, block_node: &PerlNode) {
        // Format simple blocks on the same line: { expr }
        for child in block_node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            // In simple blocks, reduce whitespace to single spaces
                            // Only add space if not adjacent to braces and content exists
                            if !self.output.ends_with(' ') && !self.output.ends_with('{') {
                                self.output.push(' ');
                            }
                        }
                        SyntaxKind::L_BRACE => {
                            self.handle_spacing_before(kind);
                            if self.at_line_start {
                                self.add_indent();
                                self.at_line_start = false;
                            }
                            self.output.push_str(text);
                            self.output.push(' '); // Space after opening brace
                            self.prev_token_kind = Some(kind);
                        }
                        SyntaxKind::R_BRACE => {
                            self.output.push(' '); // Space before closing brace
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                        _ => {
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }

    fn format_token(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        let kind = token.kind();
        let text = token.text();

        match kind {
            SyntaxKind::WHITESPACE => {
                // 空白は基本的に再構築する
                if text.contains('\n') {
                    self.handle_newline();
                    // Squeeze multiple consecutive empty lines
                    self.squeeze_multiple_newlines();
                }
            }
            SyntaxKind::COMMENT => {
                // コメントは保持するが、適切な位置に配置
                if self.at_line_start {
                    self.add_indent();
                    self.at_line_start = false;
                } else {
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

                // Find the next non-whitespace sibling token kind
                let mut next_kind = None;
                let mut next = token.next_token();
                while let Some(t) = next {
                    if t.kind() != SyntaxKind::WHITESPACE {
                        next_kind = Some(t.kind());
                        break;
                    }
                    next = t.next_token();
                }
                if !matches!(
                    next_kind,
                    Some(SyntaxKind::ELSIF_KW) | Some(SyntaxKind::ELSE_KW)
                ) {
                    self.handle_newline();
                }

                self.prev_token_kind = Some(kind);
            }
            _ => {
                // 通常のトークンの処理
                self.handle_spacing_before(kind);

                if self.at_line_start && !kind.is_trivia() {
                    self.add_indent();
                    self.at_line_start = false;
                }

                self.output.push_str(text);
                // Reset consecutive newlines when adding actual content
                self.consecutive_newlines = 0;
                self.handle_spacing_after(kind);
                self.prev_token_kind = Some(kind);
            }
        }
    }

    fn handle_spacing_before(&mut self, current: SyntaxKind) {
        if self.at_line_start {
            return;
        }

        let needs_space = match (self.prev_token_kind, current) {
            // 演算子の前後
            (Some(_), SyntaxKind::EQ) | (Some(SyntaxKind::EQ), _) => true,
            (Some(_), SyntaxKind::PLUS) | (Some(SyntaxKind::PLUS), _) => true,
            (Some(_), SyntaxKind::MINUS) | (Some(SyntaxKind::MINUS), _) => true,
            (Some(_), SyntaxKind::FAT_COMMA) | (Some(SyntaxKind::FAT_COMMA), _) => true,

            // Comparison operators
            (Some(_), SyntaxKind::GT) | (Some(SyntaxKind::GT), _) => true,
            (Some(_), SyntaxKind::LT) | (Some(SyntaxKind::LT), _) => true,
            (Some(_), SyntaxKind::GE) | (Some(SyntaxKind::GE), _) => true,
            (Some(_), SyntaxKind::LE) | (Some(SyntaxKind::LE), _) => true,
            (Some(_), SyntaxKind::EQ_EQ) | (Some(SyntaxKind::EQ_EQ), _) => true,
            (Some(_), SyntaxKind::NE) | (Some(SyntaxKind::NE), _) => true,

            // Regex operators
            (Some(_), SyntaxKind::REGEX_MATCH) | (Some(SyntaxKind::REGEX_MATCH), _) => true,
            (Some(_), SyntaxKind::REGEX_NOT_MATCH) | (Some(SyntaxKind::REGEX_NOT_MATCH), _) => true,

            // Exception: no space before semicolon when previous token is slash (for q-string delimiters)
            (Some(SyntaxKind::SLASH), SyntaxKind::SEMICOLON) => false,

            // Multiplicative operators (but not PERCENT which is used as sigil)
            (Some(_), SyntaxKind::STAR) | (Some(SyntaxKind::STAR), _) => true,
            (Some(_), SyntaxKind::SLASH) | (Some(SyntaxKind::SLASH), _) => true,
            (Some(_), SyntaxKind::MODULO) | (Some(SyntaxKind::MODULO), _) => true,
            (Some(_), SyntaxKind::X) | (Some(SyntaxKind::X), _) => true,

            // Logical operators
            (Some(_), SyntaxKind::LOGICAL_AND) | (Some(SyntaxKind::LOGICAL_AND), _) => true,
            (Some(_), SyntaxKind::LOGICAL_OR) | (Some(SyntaxKind::LOGICAL_OR), _) => true,

            // foo, bar
            (Some(SyntaxKind::COMMA), _) => true,
            (Some(_), SyntaxKind::COMMA) => false,

            // キーワードの後
            (
                Some(
                    SyntaxKind::MY_KW
                    | SyntaxKind::OUR_KW
                    | SyntaxKind::STATE_KW
                    | SyntaxKind::LOCAL_KW,
                ),
                _,
            ) => true,
            (Some(SyntaxKind::SUB_KW), SyntaxKind::IDENT) => true,
            (Some(SyntaxKind::SUB_KW), SyntaxKind::QUALIFIED_IDENT) => true,
            (Some(SyntaxKind::FOR_KW), _) => true,
            (Some(SyntaxKind::FOREACH_KW), _) => true,
            (Some(SyntaxKind::WHILE_KW), _) => true,
            (Some(SyntaxKind::IF_KW), _) => true,
            (Some(SyntaxKind::ELSIF_KW), _) => true,
            (Some(SyntaxKind::ELSE_KW), _) => true,
            (Some(SyntaxKind::PACKAGE_KW), _) => true,
            (Some(SyntaxKind::USE_KW), _) => true,
            (Some(SyntaxKind::RETURN_KW), _) => true,

            // Before left brace "{"
            (Some(_), SyntaxKind::L_BRACE) => true,

            // After R_BRACE, add space before expressions (for block functions) but not before semicolons
            (Some(SyntaxKind::R_BRACE), kind) if kind != SyntaxKind::SEMICOLON => true,

            // 括弧の内側はスペースなし、但し括弧の前は適切にスペースを入れる
            (Some(SyntaxKind::L_PAREN), _) => false,
            (Some(_), SyntaxKind::R_PAREN) => false,
            (Some(SyntaxKind::L_BRACE), _) => false,

            // Before L_PAREN, add space after variables and keywords (but not after identifiers or qualified identifiers for function calls)
            (Some(kind), SyntaxKind::L_PAREN)
                if kind.is_variable()
                    || matches!(
                        kind,
                        SyntaxKind::MY_KW
                            | SyntaxKind::OUR_KW
                            | SyntaxKind::STATE_KW
                            | SyntaxKind::LOCAL_KW
                            | SyntaxKind::FOR_KW
                            | SyntaxKind::FOREACH_KW
                            | SyntaxKind::WHILE_KW
                            | SyntaxKind::IF_KW
                            | SyntaxKind::ELSIF_KW
                    ) =>
            {
                true
            }

            // a->b
            (Some(SyntaxKind::ARROW), _) | (Some(_), SyntaxKind::ARROW) => false,

            // After identifier not followed by a semicolon, double colon, or left parenthesis
            (Some(SyntaxKind::IDENT), kind)
                if kind != SyntaxKind::SEMICOLON
                    && kind != SyntaxKind::DOUBLE_COLON
                    && kind != SyntaxKind::L_PAREN =>
            {
                true
            }

            // :: の前後はスペースなし（パッケージ名区切り）
            (Some(_), SyntaxKind::DOUBLE_COLON) | (Some(SyntaxKind::DOUBLE_COLON), _) => false,

            _ => false,
        };

        if needs_space {
            self.output.push(' ');
        }
    }

    fn handle_spacing_after(&mut self, current: SyntaxKind) {
        match current {
            SyntaxKind::SEMICOLON => {
                self.handle_newline();
            }
            SyntaxKind::L_BRACE => {
                self.indent_level += 1;
                self.handle_newline();
            }
            _ => {}
        }
    }

    fn handle_newline(&mut self) {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
            self.consecutive_newlines = 1;
        } else {
            self.consecutive_newlines += 1;
        }
        self.at_line_start = true;
    }

    fn add_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str(&self.indent_string);
        }
    }

    fn add_empty_line_before_if_needed(&mut self, node: &PerlNode) {
        // Add an empty line if the previous sibling is of a different type,
        // or if this is a SUB_DEF with any preceding sibling (to separate all subs)
        // Exception: Don't add empty line between PACKAGE_STMT and USE_STMT
        if let Some(prev) = node.prev_sibling() {
            let should_add_empty_line = if prev.kind() != node.kind() {
                // Don't add empty line between PACKAGE_STMT and USE_STMT
                !(prev.kind() == SyntaxKind::PACKAGE_STMT && node.kind() == SyntaxKind::USE_STMT)
            } else {
                false
            };

            if should_add_empty_line || node.kind() == SyntaxKind::SUB_DEF {
                self.add_empty_line_before();
            }
        }
    }

    fn add_empty_line_after_if_needed(&mut self, node: &PerlNode) {
        // Add an empty line if the next sibling is of a different type.
        // Exception: Don't add empty line between PACKAGE_STMT and USE_STMT
        if let Some(next) = node.next_sibling() {
            if next.kind() != node.kind() {
                // Don't add empty line between PACKAGE_STMT and USE_STMT
                if !(node.kind() == SyntaxKind::PACKAGE_STMT && next.kind() == SyntaxKind::USE_STMT)
                {
                    self.add_empty_line_after();
                }
            }
        }
    }

    fn add_empty_line_before(&mut self) {
        // Only add empty line if this is not the first node and we don't already have one
        if !self.output.is_empty() && !self.output.ends_with("\n\n") {
            if !self.output.ends_with('\n') {
                self.handle_newline();
            }
            // Add one more newline to create an empty line
            self.output.push('\n');
            self.consecutive_newlines += 1;
            self.at_line_start = true;
        }
    }

    fn add_empty_line_after(&mut self) {
        // Force at least one empty line after the node
        if !self.output.ends_with('\n') {
            self.handle_newline();
        }
        // Add one more newline to create an empty line
        if !self.output.ends_with("\n\n") {
            self.output.push('\n');
            self.consecutive_newlines += 1;
        }
    }

    fn squeeze_multiple_newlines(&mut self) {
        // Limit consecutive newlines to maximum of 2 (one empty line)
        if self.consecutive_newlines > 2 {
            // Find the start of the trailing newlines
            if let Some(i) = self.output.rfind(|c| c != '\n') {
                // Found a non-newline character. Truncate after it.
                self.output.truncate(i + 1);
            } else {
                // The string is all newlines.
                self.output.clear();
            }
            self.output.push_str("\n\n");
            self.consecutive_newlines = 2;
        }
    }

    fn format_method_call(&mut self, node: &PerlNode) {
        // Check if this method call should be formatted multiline
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_method_call(node);
        } else {
            self.format_single_line_method_call(node);
        }
    }

    fn format_single_line_method_call(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        for child in children.by_ref() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::WHITESPACE {
                        // Skip whitespace in method calls
                        continue;
                    }
                    self.format_token(&token);
                }
            }
            break;
        }
        self.format_subscription_iter(children, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    fn format_multiline_method_call(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        for child in children.by_ref() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::WHITESPACE {
                        // Skip whitespace in method calls
                        continue;
                    }
                    self.format_token(&token);
                }
            }
            break;
        }
        // Use multiline formatting for the parenthesized arguments
        self.format_multiline_delimited_iter(children, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }

    fn format_until_arrow_iter(&mut self, iter: &mut SyntaxElementChildren<PerlLanguage>) {
        for child in iter {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) if token.kind() == SyntaxKind::WHITESPACE => {}
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();
                    self.output.push_str(text);

                    if kind == SyntaxKind::ARROW {
                        break;
                    }
                }
            }
        }
    }

    /// formats @array, %hash or its ref's [ ... ] or { ... } part
    fn format_subscription_iter(
        &mut self,
        iter: SyntaxElementChildren<PerlLanguage>,
        opening: SyntaxKind,
        closing: SyntaxKind,
    ) {
        if self.has_newline_before_first_value_iter(iter.clone()) {
            self.format_multiline_delimited_iter(iter, opening, closing);
        } else {
            for child in iter {
                match child {
                    NodeOrToken::Node(node) => self.format_node(&node),
                    NodeOrToken::Token(token) => {
                        let kind = token.kind();
                        let text = token.text();

                        match kind {
                            _ if kind == opening || kind == closing => {
                                self.output.push_str(text);
                                self.prev_token_kind = Some(kind);
                            }
                            SyntaxKind::WHITESPACE => {
                                // pass
                            }
                            _ => {
                                self.format_token(&token);
                            }
                        }
                    }
                }
            }
        }
    }

    fn format_hash_ref_access(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        self.format_subscription_iter(children, SyntaxKind::L_BRACE, SyntaxKind::R_BRACE);
    }

    fn format_array_ref_access(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        self.format_subscription_iter(children, SyntaxKind::L_BRACKET, SyntaxKind::R_BRACKET);
    }

    fn format_code_ref_call(&mut self, node: &PerlNode) {
        let mut children = node.children_with_tokens();
        self.format_until_arrow_iter(children.by_ref());
        self.format_subscription_iter(children, SyntaxKind::L_PAREN, SyntaxKind::R_PAREN);
    }
}

pub fn format(node: &PerlNode) -> String {
    let mut formatter = Formatter::new();
    formatter.format(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_perl;

    /// Helper function to reduce code duplication in formatting tests
    fn check_formatting_cases(cases: &[(&str, &str)]) {
        for (input, expected) in cases {
            let (syntax, err) = parse_perl(input);
            assert!(err.is_empty(), "Parse errors for '{}': {:?}", input, err);
            let formatted = format(&syntax);
            assert_eq!(
                formatted, *expected,
                "Formatting failed for input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_var_decl_formatting() {
        let input = "my$var=1;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @r"
        my $var = 1;
        ");
    }

    #[test]
    fn test_all_var_decl_types_formatting() {
        let cases = [
            ("my $x = 1;", "my $x = 1;\n"),
            ("our $x = 2;", "our $x = 2;\n"),
            ("state $x = 3;", "state $x = 3;\n"),
            ("local $x = 4;", "local $x = 4;\n"),
            ("my@arr=(1,2,3);", "my @arr = (1, 2, 3);\n"),
            ("our%hash=(a=>1);", "our %hash = (a => 1);\n"),
            ("state($x,$y)=(1,2);", "state ($x, $y) = (1, 2);\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_for_stmt_with_various_decls_formatting() {
        let cases = [
            (
                "for my $var (@list) { print $var; }",
                "for my $var (@list) {\n    print $var;\n}\n",
            ),
            (
                "for our $var (@list) { print $var; }",
                "for our $var (@list) {\n    print $var;\n}\n",
            ),
            (
                "for local $var (@list) { print $var; }",
                "for local $var (@list) {\n    print $var;\n}\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_qualified_subroutine_formatting() {
        let cases = [
            ("sub Foo::Bar::func { }", "sub Foo::Bar::func {\n}\n"),
            (
                "sub Very::Deep::Nested::func { }",
                "sub Very::Deep::Nested::func {\n}\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_mixed_qualified_and_simple_formatting() {
        let cases = [(
            "my$var=$Foo::Bar::other_var;",
            "my $var = $Foo::Bar::other_var;\n",
        )];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_for_stmt_formatting() {
        let input = "for my$var(@list){my$x=1;print$x;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        for my $var (@list) {
            my $x = 1;
            print $x;
        }
        ");
    }

    #[test]
    fn test_foreach_stmt_formatting() {
        let input = "foreach my$item(@items){print$item;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        foreach my $item (@items) {
            print $item;
        }
        ");
    }

    #[test]
    fn test_for_stmt_with_existing_var_formatting() {
        let input = "for$var(@array){my$y=2;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        for $var (@array) {
            my $y = 2;
        }
        ");
    }

    #[test]
    fn test_while_stmt_formatting() {
        let input = "while($condition){my$y=2;func$y;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        while ($condition) {
            my $y = 2;
            func $y;
        }
        ");
    }

    #[test]
    fn test_nested_loops_formatting() {
        let input = "for($i){while($j){my$x=1;}}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        for ($i) {
            while ($j) {
                my $x = 1;
            }
        }
        ");
    }

    #[test]
    fn test_loop_with_complex_conditions() {
        let input = "while($a+$b*$c){for(@array){print;}}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        while ($a + $b * $c) {
            for (@array) {
                print;
            }
        }
        ");
    }

    #[test]
    fn test_comment_formatting() {
        let input = r#" 
sub test {
    my $x = 1;
# a comment
    my $y = 2;
}
"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub test {
            my $x = 1;
            # a comment
            my $y = 2;
        }
        ");
    }

    #[test]
    fn test_method_call_formatting() {
        let cases = [
            ("$obj->method();", "$obj->method();\n"),
            ("$obj->method($arg);", "$obj->method($arg);\n"),
            ("$obj->method($a,$b);", "$obj->method($a, $b);\n"),
            (
                "my$result=$obj->calculate();",
                "my $result = $obj->calculate();\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_chained_method_calls_formatting() {
        let cases = [
            (
                "$obj->method1()->method2();",
                "$obj->method1()->method2();\n",
            ),
            (
                "$obj->get()->set($value)->save();",
                "$obj->get()->set($value)->save();\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_method_call_on_expressions_formatting() {
        let cases = [
            ("($obj+$other)->method();", "($obj + $other)->method();\n"),
            ("func()->method();", "func()->method();\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_logical_and_operator_formatting() {
        let input = "my$result=$a&&$b;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $result = $a && $b;");
    }

    #[test]
    fn test_logical_or_operator_formatting() {
        let input = "my$result=$a||$b;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $result = $a || $b;");
    }

    #[test]
    fn test_logical_operators_precedence_formatting() {
        let input = "my$result=$a||$b&&$c;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $result = $a || $b && $c;");
    }

    #[test]
    fn test_mixed_logical_arithmetic_operators_formatting() {
        let input = "my$result=$a+$b&&$c*$d||$e;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $result = $a + $b && $c * $d || $e;");
    }

    #[test]
    fn test_chained_logical_operators_formatting() {
        let cases = [
            ("$a&&$b&&$c;", "$a && $b && $c;\n"),
            ("$a||$b||$c;", "$a || $b || $c;\n"),
            ("$a&&$b||$c&&$d;", "$a && $b || $c && $d;\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_function_call_formatting() {
        let cases = [
            ("push@array,$value;", "push @array, $value;\n"),
            ("print$var,\"hello\",123;", "print $var, \"hello\", 123;\n"),
            ("shift@array;", "shift @array;\n"),
            // TODO: ハッシュインデックス構文をサポートする必要がある: ("delete$hash{key};", "delete $hash{key};\n"),
            ("my_func$a,$b,$c;", "my_func $a, $b, $c;\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_function_call_with_tight_spacing() {
        let input = "push@array,$value,$another;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"push @array, $value, $another;");
    }

    #[test]
    fn test_function_call_with_mixed_argument_types() {
        let input = r#"printf "%s:%d\n", $name, $age;"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"printf \"%s:%d\\n\", $name, $age;");
    }

    #[test]
    fn test_multiple_function_calls_formatting() {
        let input = "push@a,$x;pop@b;unshift@c,$y,$z;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        push @a, $x;
        pop @b;
        unshift @c, $y, $z;
        ");
    }

    #[test]
    fn test_function_call_in_sub() {
        let input = "sub test{push@array,$value;return$result;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub test {
            push @array, $value;
            return $result;
        }
        ");
    }

    #[test]
    fn test_eval_block_function_formatting() {
        let input = "eval{my$x=1;print$x;};";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        eval {
            my $x = 1;
            print $x;
        }
        ;
        ");
    }

    #[test]
    fn test_map_simple_block_function_formatting() {
        let input = "map{$_*2}@numbers;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"map { $_ * 2 } @numbers;");
    }

    #[test]
    fn test_map_with_parentheses_formatting() {
        let input = "map{$_*2}(1,2,3);";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"map { $_ * 2 } (1, 2, 3);");
    }

    #[test]
    fn test_grep_block_function_formatting() {
        let input = "grep{$_+1}@items;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"grep { $_ + 1 } @items;");
    }

    #[test]
    fn test_sort_block_function_formatting() {
        let input = "sort{$a+$b}@values;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"sort { $a + $b } @values;");
    }

    #[test]
    fn test_do_block_function_formatting() {
        let input = "do{my$result=42;return$result;};";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        do {
            my $result = 42;
            return $result;
        }
        ;
        ");
    }

    #[test]
    fn test_nested_block_functions_formatting() {
        let input = "map{grep{$_+1}@$_}@arrays;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"map { grep { $_ + 1 }@$_ } @arrays;");
    }

    #[test]
    fn test_block_function_with_multiple_args_formatting() {
        let input = "map{$_*$factor}@array1,@array2;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"map { $_ * $factor } @array1, @array2;");
    }

    #[test]
    fn test_block_function_assignment_formatting() {
        let input = "my@result=map{$_*2}@input;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my @result = map { $_ * 2 } @input;");
    }

    #[test]
    fn test_if_else_stmt_formatting() {
        let input = "if($condition){do_something();}else{do_something_else();}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        if ($condition) {
            do_something();
        } else {
            do_something_else();
        }
        ");
    }

    #[test]
    fn test_regex_match_operator_formatting() {
        let cases = [
            ("$str=~\"pattern\";", "$str =~ \"pattern\";\n"),
            ("$str!~\"pattern\";", "$str !~ \"pattern\";\n"),
            ("$a==1&&$str=~\"test\";", "$a == 1 && $str =~ \"test\";\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_regex_literal_basic_formatting() {
        let input = "$str=~/pattern/;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"$str =~ /pattern/;");
    }

    #[test]
    fn test_regex_literal_with_flags_formatting() {
        let input = "$text=~/test.*pattern/ig;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"$text =~ /test.*pattern/ig;");
    }

    #[test]
    fn test_regex_literal_vs_division_formatting() {
        let input = "my$result=$a/$b;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $result = $a / $b;");
    }

    #[test]
    fn test_regex_literal_in_conditional_formatting() {
        let input = "if($text=~/hello/){print\"matched\";}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r#"
        if ($text =~ /hello/) {
            print "matched";
        }
        "#);
    }

    #[test]
    fn test_complex_regex_expression_formatting() {
        let input = "my$result=$str=~/pattern/&&$other!~/test/i;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $result = $str =~ /pattern/ && $other !~ /test/i;");
    }

    #[test]
    fn test_empty_lines_before_after_subs() {
        let input = "my$x=1;sub foo{my$y=2;}my$z=3;sub bar{return 42;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $x = 1;

        sub foo {
            my $y = 2;
        }

        my $z = 3;

        sub bar {
            return 42;
        }
        ");
    }

    #[test]
    fn test_empty_lines_before_after_use_statements() {
        let input = "use warnings;my$x=1;use strict;my$y=2;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        use warnings;

        my $x = 1;

        use strict;

        my $y = 2;
        ");
    }

    #[test]
    fn test_multiple_empty_lines_squeezing() {
        let input = r#"my $x = 1;



sub foo {
    my $y = 2;
}



my $z = 3;"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $x = 1;

        sub foo {
            my $y = 2;
        }

        my $z = 3;
        ");
    }

    #[test]
    fn test_mixed_use_and_sub_empty_lines() {
        let input = "use warnings;use strict;my$x=1;sub foo{return 42;}sub bar{return 24;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        use warnings;
        use strict;
        
        my $x = 1;
        
        sub foo {
            return 42;
        }

        sub bar {
            return 24;
        }
        ");
    }

    #[test]
    fn test_first_statement_no_empty_line_before() {
        let input = "use warnings;my$x=1;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        use warnings;

        my $x = 1;
        ");
    }

    #[test]
    fn test_end_data_section_basic() {
        let input = r#"
my $x = 1;
__DATA__
This is data after __DATA__ $#&!
  Raw string here~
        "#;

        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $x = 1;
        __DATA__
        This is data after __DATA__ $#&!
          Raw string here~
        ");
    }

    #[test]
    fn test_single_line_hash_ref_formatting() {
        let input = "my $hash = { a => 1, b => 2 };";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $hash = {a => 1, b => 2};");
    }

    #[test]
    fn test_multiline_hash_ref_formatting() {
        let input = r#"my $hash = {
    a => 1,
    b => 2
};"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $hash = {
            a => 1,
            b => 2
        };
        ");
    }

    #[test]
    fn test_single_line_array_ref_formatting() {
        let input = "my $array = [1, 2, 3];";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $array = [1, 2, 3];");
    }

    #[test]
    fn test_multiline_array_ref_formatting() {
        let input = r#"my $array = [
    1,
    2,
    3
];"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $array = [
            1,
            2,
            3
        ];
        ");
    }

    #[test]
    fn test_single_line_qw_formatting() {
        let input = "my @words = qw(hello world test);";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my @words = qw(hello world test);");
    }

    #[test]
    fn test_multiline_qw_formatting() {
        let input = r#"my @words = qw(
    hello
    world
    test
);"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my @words = qw(
            hello
            world
            test
        );
        ");
    }

    #[test]
    fn test_q_single_quoted_string_formatting() {
        let input = "my $str = q(hello world);";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $str = q(hello world);");
    }

    #[test]
    fn test_q_with_different_delimiters_formatting() {
        let cases = [
            ("my $str = q(hello);", "my $str = q(hello);\n"),
            ("my $str = q[hello];", "my $str = q[hello];\n"),
            ("my $str = q{hello};", "my $str = q{hello};\n"),
            ("my $str = q/hello/;", "my $str = q/hello/;\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_q_with_special_chars_formatting() {
        let input = r#"my $str = q(hello$world@test%hash);"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $str = q(hello$world@test%hash);");
    }

    #[test]
    fn test_qq_double_quoted_string_formatting() {
        let input = r#"my $str = qq(hello $name);"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $str = qq(hello $name);");
    }

    #[test]
    fn test_qq_with_different_delimiters_formatting() {
        let cases = [
            ("my $str = qq(hello);", "my $str = qq(hello);\n"),
            ("my $str = qq[hello];", "my $str = qq[hello];\n"),
            ("my $str = qq{hello};", "my $str = qq{hello};\n"),
            ("my $str = qq/hello/;", "my $str = qq/hello/;\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_qq_with_interpolation_formatting() {
        let input = r#"my $str = qq(Hello $user, welcome to $site!);"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $str = qq(Hello $user, welcome to $site!);");
    }

    #[test]
    fn test_qx_command_execution_formatting() {
        let input = "my $output = qx(ls -la);";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $output = qx(ls -la);");
    }

    #[test]
    fn test_qx_with_different_delimiters_formatting() {
        let cases = [
            ("my $output = qx(ls);", "my $output = qx(ls);\n"),
            ("my $output = qx[ls];", "my $output = qx[ls];\n"),
            ("my $output = qx{ls};", "my $output = qx{ls};\n"),
            ("my $output = qx/ls/;", "my $output = qx/ls/;\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_qx_with_complex_command_formatting() {
        let input = r#"my $result = qx(grep -r "pattern" /var/log/);"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r#"my $result = qx(grep -r "pattern" /var/log/);"#);
    }

    #[test]
    fn test_mixed_q_string_family_formatting() {
        let input = r#"my $single = q(no interpolation);
my $double = qq(with $var interpolation);
my $command = qx(echo "Hello World");"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r#"
        my $single = q(no interpolation);
        my $double = qq(with $var interpolation);
        my $command = qx(echo "Hello World");
        "#);
    }

    #[test]
    fn test_q_string_preserving_whitespace() {
        let input = r#"my $str = q(  hello   world  );"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $str = q(  hello   world  );");
    }

    #[test]
    fn test_nested_multiline_structures() {
        let input = r#"my $data = {
    users => [
        { name => "Alice", age => 30 },
        { name => "Bob", age => 25 }
    ],
    config => {
        debug => 1,
        timeout => 60
    }
};"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r#"
        my $data = {
            users => [
                {name => "Alice", age => 30},
                {name => "Bob", age => 25}
            ],
            config => {
                debug => 1,
                timeout => 60
            }
        };
        "#);
    }

    #[test]
    fn test_mixed_single_and_multiline() {
        let input = r#"my $mixed = {
    simple => { a => 1, b => 2 },
    complex => {
        nested => [1, 2, 3],
        items => [
            "first",
            "second"
        ]
    }
};"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r#"
        my $mixed = {
            simple => {a => 1, b => 2},
            complex => {
                nested => [1, 2, 3],
                items => [
                    "first",
                    "second"
                ]
            }
        };
        "#);
    }

    #[test]
    fn test_single_line_function_call_formatting() {
        let input = "func(arg1, arg2, arg3);";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"func(arg1, arg2, arg3);");
    }

    #[test]
    fn test_multiline_function_call_formatting() {
        let input = r#"func(
    arg1,
    arg2,
    arg3
);"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        func(
            arg1,
            arg2,
            arg3
        );
        ");
    }

    #[test]
    fn test_multiline_function_call_with_complex_args_formatting() {
        let input = r#"complex_func(
    $var1 + $var2,
    "string argument",
    42,
    $obj->method()
);"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r#"
        complex_func(
            $var1 + $var2,
            "string argument",
            42,
            $obj->method()
        );
        "#);
    }

    #[test]
    fn test_nested_multiline_function_calls_formatting() {
        let input = r#"outer_func(
    inner_func(
        nested_arg1,
        nested_arg2
    ),
    other_arg
);"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        outer_func(
            inner_func(
                nested_arg1,
                nested_arg2
            ),
            other_arg
        );
        ");
    }

    #[test]
    fn test_multiline_parenthesized_expression_formatting() {
        let input = r#"my $result = (
    $a + $b,
    $c * $d
);"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $result = (
            $a + $b,
            $c * $d
        );
        ");
    }

    #[test]
    fn test_mixed_single_and_multiline_parentheses_formatting() {
        let input = r#"func1(short, args);
func2(
    longer_arg1,
    longer_arg2,
    longer_arg3
);"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        func1(short, args);
        func2(
            longer_arg1,
            longer_arg2,
            longer_arg3
        );
        ");
    }

    #[test]
    fn test_multiline_parentheses_in_control_structures() {
        let input = r#"if (
    $condition1 &&
    $condition2 ||
    $condition3
) {
    do_something();
}"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        if (
            $condition1 &&
            $condition2 ||
            $condition3
        ) {
            do_something();
        }
        ");
    }

    #[test]
    fn test_real_world_perl_code_formatting() {
        // Test realistic Perl code patterns with currently supported syntax
        let input = r#"use strict;use warnings;sub process_data{my($input,$output)=@_;my$data=load_file($input);my@results=();push@results,$data;return@results;}sub simple_function{my$config={host=>"localhost",port=>5432};return process_config($config);}"#;

        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r###"
        use strict;
        use warnings;

        sub process_data {
            my ($input, $output) = @_;
            my $data = load_file($input);
            my @results = ();
            push @results, $data;
            return @results;
        }

        sub simple_function {
            my $config = {host => "localhost", port => 5432};
            return process_config($config);
        }
        "###);
    }

    #[test]
    fn test_return_and_function_calls_formatting() {
        // Test the specific patterns we fixed - using supported syntax only
        let input = r#"sub handler{return{};return{error=>"msg"};return process($data);return fetch($id);print format(generate($req));push@arr,transform($item);}"#;

        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r###"
        sub handler {
            return {};
            return {error => "msg"};
            return process($data);
            return fetch($id);
            print format(generate($req));
            push @arr, transform($item);
        }
        "###);
    }

    #[test]
    fn test_use_version_formatting() {
        let cases = [
            ("use v5.42;", "use v5.42;\n"),
            ("use v5.008_001;", "use v5.008_001;\n"),
            ("use v1.23.45;", "use v1.23.45;\n"),
            ("use v5.42;my $x = 1;", "use v5.42;\n\nmy $x = 1;\n"),
            (
                "use v5.008_001;use warnings;",
                "use v5.008_001;\nuse warnings;\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_m_regex_formatting() {
        let input = r#"my $result = m/pattern/gi;"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @"my $result = m/pattern/gi;");
    }

    #[test]
    fn test_m_with_different_delimiters_formatting() {
        let cases = [
            ("my $result = m/pattern/;", "my $result = m/pattern/;\n"),
            ("my $result = m(pattern);", "my $result = m(pattern);\n"),
            ("my $result = m[pattern];", "my $result = m[pattern];\n"),
            ("my $result = m{pattern};", "my $result = m{pattern};\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_m_with_flags_formatting() {
        let cases = [
            ("my $result = m/pattern/i;", "my $result = m/pattern/i;\n"),
            ("my $result = m/pattern/g;", "my $result = m/pattern/g;\n"),
            ("my $result = m/pattern/gi;", "my $result = m/pattern/gi;\n"),
            (
                "my $result = m/pattern/gims;",
                "my $result = m/pattern/gims;\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_m_with_complex_pattern_formatting() {
        let input = r#"my $result = m/[a-zA-Z]+\d*\.?\w+/gi;"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @r#"my $result = m/[a-zA-Z]+\d*\.?\w+/gi;"#);
    }

    #[test]
    fn test_qr_regex_formatting() {
        let input = r#"my $regex = qr/pattern/i;"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @"my $regex = qr/pattern/i;");
    }

    #[test]
    fn test_qr_with_different_delimiters_formatting() {
        let cases = [
            ("my $regex = qr/pattern/;", "my $regex = qr/pattern/;\n"),
            ("my $regex = qr(pattern);", "my $regex = qr(pattern);\n"),
            ("my $regex = qr[pattern];", "my $regex = qr[pattern];\n"),
            ("my $regex = qr{pattern};", "my $regex = qr{pattern};\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_qr_with_flags_formatting() {
        let cases = [
            ("my $regex = qr/pattern/i;", "my $regex = qr/pattern/i;\n"),
            ("my $regex = qr/pattern/m;", "my $regex = qr/pattern/m;\n"),
            (
                "my $regex = qr/pattern/gixms;",
                "my $regex = qr/pattern/gixms;\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_qr_with_complex_pattern_formatting() {
        let input = r#"my $regex = qr/[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/;"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @r#"my $regex = qr/[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/;"#);
    }

    #[test]
    fn test_mixed_regex_and_string_formatting() {
        let input = r#"my $str = "text";
my $pattern = qr/test/i;
my $result = m/pattern/g;"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @r###"
        my $str = "text";
        my $pattern = qr/test/i;
        my $result = m/pattern/g;
        "###);
    }

    #[test]
    fn test_regex_with_escape_sequences_formatting() {
        let input = r#"my $result = m/hello\nworld\t/gs;"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @r#"my $result = m/hello\nworld\t/gs;"#);
    }

    #[test]
    fn test_qr_assigned_to_variable_formatting() {
        let input = r#"my $email_regex = qr/[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/;"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);
        let formatted = format(&syntax);
        insta::assert_snapshot!(formatted, @r#"my $email_regex = qr/[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/;"#);
    }
}
