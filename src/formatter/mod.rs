use crate::{PerlLanguage, PerlNode, SyntaxKind};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxToken};

// Helper function for checking disallowed tokens
fn has_disallowed_tokens(node: &PerlNode) -> bool {
    node.descendants_with_tokens().any(|element| {
        element.as_token().is_some_and(|token| {
            matches!(token.kind(), SyntaxKind::SEMICOLON | SyntaxKind::COMMENT)
        })
    })
}

pub struct Formatter {
    output: String,
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
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            indent_string: "    ".to_string(), // 4 spaces
            prev_token_kind: None,
            at_line_start: true,
            pending_empty_lines: 0,
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
                | SyntaxKind::STMT
                | SyntaxKind::DECLARATION_STMT
        ) {
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
            SyntaxKind::REFERENCE_EXPR => {
                self.format_reference_expr(node);
                return;
            }
            SyntaxKind::TERNARY_EXPR => {
                self.format_ternary_expr(node);
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
                            self.output.push(' '); // Add space after opening brace
                            self.prev_token_kind = Some(token.kind());
                        }
                        SyntaxKind::R_BRACE => {
                            if self.prev_token_kind != Some(SyntaxKind::L_BRACE) {
                                self.output.push(' '); // Add space before closing brace
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
                    Some(SyntaxKind::ELSIF_KW)
                        | Some(SyntaxKind::ELSE_KW)
                        | Some(SyntaxKind::SEMICOLON)
                        | Some(SyntaxKind::L_PAREN)
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
        // Collect all children as a vector for lookahead
        // FIXME:
        // This function collects all children of a BLOCK_STMT into a Vec on every call. For files with many or large blocks, this could lead to significant memory allocations and a potential performance overhead. The design document mentions performance as a consideration, so it might be worth exploring a more memory-efficient approach.
        // Consider using an iterator-based approach that avoids collecting all children into a vector. The itertools crate, for example, provides utilities like peekable() or PeekingNext that could allow you to look ahead at the next token without needing to allocate a Vec for the entire block.
        let children: Vec<_> = node.children_with_tokens().collect();
        let mut i = 0;

        while i < children.len() {
            match &children[i] {
                NodeOrToken::Node(child_node) => {
                    // Output pending empty lines before processing child nodes
                    self.output_pending_empty_lines();
                    self.format_node(child_node);
                    i += 1;
                }
                NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::WHITESPACE {
                        // Look ahead to collect consecutive WHITESPACE tokens
                        let mut consecutive_whitespace = vec![token];
                        let mut j = i + 1;

                        while j < children.len() {
                            if let NodeOrToken::Token(next_token) = &children[j] {
                                if next_token.kind() == SyntaxKind::WHITESPACE {
                                    consecutive_whitespace.push(next_token);
                                    j += 1;
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }

                        // Count total newlines across all consecutive whitespace tokens
                        let total_newlines: usize = consecutive_whitespace
                            .iter()
                            .map(|t| t.text().matches('\n').count())
                            .sum();

                        if total_newlines > 0 {
                            // If there are multiple newlines across tokens, preserve as empty line
                            if total_newlines > 1 {
                                self.pending_empty_lines = 1;
                            }
                            self.handle_newline();
                        }

                        // Skip all the consecutive whitespace tokens we processed
                        i = j;
                    } else {
                        self.output_pending_empty_lines();
                        self.format_token(token);
                        i += 1;
                    }
                }
            }
        }
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
    fn test_map_with_parentheses_formatting() {
        let input = "map{$_*2}(1,2,3); sort{$a+$b}@values;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        map { $_ * 2 } (1, 2, 3);
        sort { $a + $b } @values;
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
    fn test_single_line_function_call_formatting() {
        let input = "func(arg1, arg2, arg3);";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"func(arg1, arg2, arg3);");
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

    #[test]
    fn test_nested_eval_in_sub() {
        let input = "sub f{eval{print$x;};return 1;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub f {
            eval {
                print $x;
            };
            return 1;
        }
        ");
    }

    #[test]
    fn test_version_use_statements_formatting() {
        check_formatting_cases(&[
            // v-prefixed versions (current support)
            ("use v5.24.1;", "use v5.24.1;\n"),
            ("use v5.008_001;", "use v5.008_001;\n"),
            ("use v5.36;", "use v5.36;\n"),
            // Bare version formats (new support)
            ("use 5.24.1;", "use 5.24.1;\n"),
            ("use 5.008_001;", "use 5.008_001;\n"),
            ("use 5.36.0;", "use 5.36.0;\n"),
            // Simple version numbers
            ("use 5;", "use 5;\n"),
            ("use 5.24;", "use 5.24;\n"),
            // With spacing variations
            ("use  v5.24.1 ;", "use v5.24.1;\n"),
            ("use  5.24.1 ;", "use 5.24.1;\n"),
            ("use\tv5.24.1\t;", "use v5.24.1;\n"),
            ("use\t5.24.1\t;", "use 5.24.1;\n"),
        ]);
    }

    #[test]
    fn test_empty_lines_preservation() {
        let input =
            "use strict;\n\n\nuse warnings;\n\nmy $x = 1;\n\n\nsub foo {\n    return $x;\n}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        use strict;
        use warnings;

        my $x = 1;

        sub foo {
            return $x;
        }
        ");
    }

    #[test]
    fn test_no_empty_lines_automatic_insertion() {
        let input = "use strict;use warnings;my $x = 1;sub foo {return $x;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        use strict;
        use warnings;

        my $x = 1;

        sub foo {
            return $x;
        }
        ");
    }

    #[test]
    fn test_block_stmt_empty_line_preservation() {
        // Test that user-written empty lines inside BLOCK_STMT are preserved
        let input = r#"sub f {
bar();

return 1;
}"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub f {
            bar();

            return 1;
        }
        ");
    }

    #[test]
    fn test_multiple_empty_lines_in_block_stmt() {
        // Test that multiple consecutive empty lines are collapsed to one
        let input = r#"sub f {
bar();



return 1;
}"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub f {
            bar();

            return 1;
        }
        ");
    }

    #[test]
    fn test_empty_lines_in_various_block_contexts() {
        // Test empty line preservation in different block contexts
        let input = r#"if ($condition) {
    1;

    2;


    3;

    # space ⬆️
    4;
    # space ⬇️

    5;

    # space ↕️

    6;

}"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        if ($condition) {
            1;

            2;

            3;

            # space ⬆️
            4;
            # space ⬇️

            5;

            # space ↕️

            6;

        }
        ");
    }

    #[test]
    fn test_logical_operators_formatting() {
        let cases = [
            // Logical NOT prefix operator (no space after !)
            ("!$x;", "!$x;\n"),
            ("$a||!$b;", "$a || !$b;\n"),
            ("(!$a&&$b);", "(!$a && $b);\n"),
            // Low-precedence logical operators (space around)
            ("$a and $b;", "$a and $b;\n"),
            ("$x or $y;", "$x or $y;\n"),
            ("$a xor $b;", "$a xor $b;\n"),
            ("not $x;", "not $x;\n"),
            // Defined-or operator
            ("$a//$b;", "$a // $b;\n"),
            ("$x//$y//$z;", "$x // $y // $z;\n"),
            // Spaceship operator
            ("$a<=>$b;", "$a <=> $b;\n"),
            ("$x<=>$y;", "$x <=> $y;\n"),
            // Mixed precedence expressions
            ("$a&&$b||$c;", "$a && $b || $c;\n"),
            ("$a||$b//$c;", "$a || $b // $c;\n"),
            ("$a and $b or $c;", "$a and $b or $c;\n"),
            ("$a&&$b and $c;", "$a && $b and $c;\n"),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_complex_logical_expressions_formatting() {
        let input = "$a&&$b||$c and $d or $e xor $f;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"$a && $b || $c and $d or $e xor $f;");
    }

    #[test]
    fn test_logical_operators_with_parentheses() {
        let input = "(!$a&&($b||$c))and($x//$y);";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"(!$a && ($b || $c)) and ($x // $y);");
    }

    #[test]
    fn test_spaceship_in_expressions() {
        let input = "$result=$a<=>$b;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"$result = $a <=> $b;");
    }

    #[test]
    fn test_contextual_logical_keywords() {
        // Test that and, or, etc. are treated as identifiers in non-operator contexts
        let input = "sub and { } my $or = 1;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub and {
        }

        my $or = 1;
        ");
    }

    #[test]
    fn test_ternary_in_data_structures_formatting() {
        let input =
            "my $config = { timeout => $is_production ? 30 : 5, retries => $is_critical ? 3 : 1 };";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let output = format(&syntax);
        insta::assert_snapshot!(output, @"my $config = {timeout => $is_production ? 30 : 5, retries => $is_critical ? 3 : 1};");
    }
}

mod expression;
mod literal;
mod spacing;
mod verbatim;
mod whitespace;
