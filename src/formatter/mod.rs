use crate::{PerlLanguage, PerlNode, SyntaxKind};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxToken};

pub struct Formatter {
    output: String,
    indent_level: usize,
    indent_string: String,
    prev_token_kind: Option<SyntaxKind>,
    at_line_start: bool,
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
            indent_string: "    ".to_string(), // 4 spaces
            prev_token_kind: None,
            at_line_start: true,
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

        // Node types that require special handling
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
            SyntaxKind::S_EXPR => {
                self.format_s_expr(node);
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

    /// Format a POD block
    /// POD blocks should be preserved exactly as-is without any formatting changes
    fn format_pod_block(&mut self, node: &PerlNode) {
        // Ensure we're on a new line before the POD block
        if !self.at_line_start {
            self.output.push('\n');
            self.at_line_start = true;
        }

        // Process all children (POD command + content + =cut) without any modifications
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Token(token) => {
                    let text = token.text();
                    match token.kind() {
                        SyntaxKind::POD_COMMAND | SyntaxKind::CUT_KW => {
                            // Output POD commands exactly as-is
                            self.output.push_str(text);
                        }
                        SyntaxKind::POD_CONTENT => {
                            // Output POD content exactly as-is, preserving all formatting
                            self.output.push_str(text);
                        }
                        _ => {
                            // Handle any other tokens (whitespace, etc.) as-is
                            self.output.push_str(text);
                        }
                    }
                }
                NodeOrToken::Node(_) => {
                    // POD blocks shouldn't contain nested nodes, but handle gracefully
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
                            self.handle_whitespace(&token);
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
                            self.handle_whitespace(&token);
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
                    Some(SyntaxKind::ELSIF_KW)
                        | Some(SyntaxKind::ELSE_KW)
                        | Some(SyntaxKind::SEMICOLON)
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
                self.handle_spacing_after_with_token(kind, token);
                self.prev_token_kind = Some(kind);
            }
        }
    }

    fn is_comparison_operator(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::GT
                | SyntaxKind::LT
                | SyntaxKind::GE
                | SyntaxKind::LE
                | SyntaxKind::EQ_EQ
                | SyntaxKind::NE
                | SyntaxKind::STR_EQ
                | SyntaxKind::STR_NE
                | SyntaxKind::STR_GT
                | SyntaxKind::STR_LT
                | SyntaxKind::STR_GE
                | SyntaxKind::STR_LE
                | SyntaxKind::STR_CMP
        )
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
            (Some(_), SyntaxKind::DOT) | (Some(SyntaxKind::DOT), _) => true,
            (Some(_), SyntaxKind::FAT_COMMA) | (Some(SyntaxKind::FAT_COMMA), _) => true,

            // Comparison operators
            (Some(_), kind) if Self::is_comparison_operator(kind) => true,
            (Some(kind), _) if Self::is_comparison_operator(kind) => true,

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
            (Some(SyntaxKind::UNLESS_KW), _) => true,
            (Some(SyntaxKind::ELSIF_KW), _) => true,
            (Some(SyntaxKind::ELSE_KW), _) => true,
            (Some(SyntaxKind::PACKAGE_KW), _) => true,
            (Some(SyntaxKind::USE_KW), _) => true,
            (Some(SyntaxKind::RETURN_KW), _) => true,

            // Postfix conditionals: add space before if/unless in postfix position
            (Some(_), SyntaxKind::IF_KW) => true,
            (Some(_), SyntaxKind::UNLESS_KW) => true,

            // Before left brace "{"
            (Some(_), SyntaxKind::L_BRACE) => true,

            // After R_BRACE, add space before expressions (for block functions) but not before semicolons
            (Some(SyntaxKind::R_BRACE), kind) if kind != SyntaxKind::SEMICOLON => true,

            // No space inside parentheses, but add space before parentheses as appropriate
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
                            | SyntaxKind::UNLESS_KW
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

    fn handle_newline(&mut self) {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
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
        }
    }

    fn handle_whitespace(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
        let text = token.text();

        // 改行を含む場合は改行処理を実行（従来のhandle_multiline_whitespaceの機能）
        if text.contains('\n') {
            self.handle_newline();
        }
        // 将来的にはこの関数でコンテキストを見て空行などを処理する予定
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
    pub fn check_formatting_cases(cases: &[(&str, &str)]) {
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
    fn test_nested_loop_with_complex_conditions() {
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
    fn test_function_call_with_variable_declaration_formatting() {
        let cases = [
            // Basic variable declaration as function argument
            ("foo my $x;", "foo my $x;\n"),
            ("foo my $x, my $y;", "foo my $x, my $y;\n"),
            ("bar our $a;", "bar our $a;\n"),
            ("baz state $s;", "baz state $s;\n"),
            ("qux local $l;", "qux local $l;\n"),
            // Mixed arguments
            (
                "args my $x, my $y => 'Type';",
                "args my $x, my $y => 'Type';\n",
            ),
            ("func my $a, $b, my $c;", "func my $a, $b, my $c;\n"),
            (
                "test my $x, 123, \"string\";",
                "test my $x, 123, \"string\";\n",
            ),
        ];
        check_formatting_cases(&cases);
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
        };
        ");
    }

    #[test]
    fn test_eval_block_with_method_call_formatting() {
        let input = "eval { $obj->meth; };";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        eval {
            $obj->meth;
        };
        ");
    }

    #[test]
    fn test_block_function_formatting_variants() {
        let cases = [
            // Different block functions should all use multi-line formatting
            ("eval{1;};", "eval {\n    1;\n};\n"),
            ("map{$_*2;}@list;", "map {\n    $_ * 2;\n}\n@list;\n"),
            (
                "grep{defined$_;}@array;",
                "grep {\n    defined $_;\n}\n@array;\n",
            ),
            (
                "sort{$a+$b;}@numbers;",
                "sort {\n    $a + $b;\n}\n@numbers;\n",
            ),
            (
                "do{my$x=1;return$x;};",
                "do {\n    my $x = 1;\n    return $x;\n};\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_map_with_parentheses_formatting() {
        let input = "map{$_*2}(1,2,3); sort{$a+$b}@values;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
map {
    $_ * 2}
(1, 2, 3);
sort {
    $a + $b}
@values;
        ");
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
    fn test_single_line_function_call_formatting() {
        let input = "func(arg1, arg2, arg3);";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"func(arg1, arg2, arg3);");
    }

    #[test]
    fn test_pod_with_code_before_and_after() {
        let input = r#"my $var = 1;

=head1 DESCRIPTION

This is a POD section with detailed description.
It preserves all formatting exactly.

=cut

my $other = 2;
"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $var = 1;
        =head1 DESCRIPTION

        This is a POD section with detailed description.
        It preserves all formatting exactly.

        =cut
        my $other = 2;
        ");
    }

    #[test]
    fn test_pod_at_eof_without_cut() {
        let input = r#"my $var = 1;

=pod

This POD block goes to EOF without =cut.
Everything after =pod should be treated as POD content.
"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $var = 1;
        =pod

        This POD block goes to EOF without =cut.
        Everything after =pod should be treated as POD content.
        ");
    }

    #[test]
    fn test_unless_stmt_formatting() {
        let input = "unless($condition){do_something();}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        unless ($condition) {
            do_something();
        }
        ");
    }

    #[test]
    fn test_postfix_conditional_formatting() {
        let cases = [
            // Postfix if tests
            ("return $x if $x > $y;", "return $x if $x > $y;\n"),
            ("print \"hello\" if $debug;", "print \"hello\" if $debug;\n"),
            (
                "my $result = calculate() if $do_calc;",
                "my $result = calculate() if $do_calc;\n",
            ),
            // Postfix unless tests
            ("return $x unless $x > $y;", "return $x unless $x > $y;\n"),
            (
                "print \"hello\" unless $quiet;",
                "print \"hello\" unless $quiet;\n",
            ),
            (
                "die \"Error\" unless defined $result;",
                "die \"Error\" unless defined $result;\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_inline_comment_preservation() {
        let cases = [
            // Inline comments should stay on the same line
            (
                "my $x = 1; # inline comment",
                "my $x = 1; # inline comment\n",
            ),
            ("print $var; # debug output", "print $var; # debug output\n"),
            (
                "return 42; # return the answer",
                "return 42; # return the answer\n",
            ),
            // Block comments should remain on their own line
            (
                "my $x = 1;\n# block comment",
                "my $x = 1;\n# block comment\n",
            ),
        ];
        check_formatting_cases(&cases);
    }
}

mod expression;
mod literal;
