use rowan::NodeOrToken;

use crate::{PerlNode, SyntaxKind};

use super::Formatter;

impl Formatter {
    pub fn format_hash_ref(&mut self, node: &PerlNode) {
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_hash_ref(node);
        } else {
            self.format_single_line_hash_ref(node);
        }
    }

    fn format_single_line_hash_ref(&mut self, node: &PerlNode) {
        // Format hash reference without newlines
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
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
                            // Other tokens are processed as usual
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

    pub fn format_array_ref(&mut self, node: &PerlNode) {
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_array_ref(node);
        } else {
            self.format_single_line_array_ref(node);
        }
    }

    fn format_single_line_array_ref(&mut self, node: &PerlNode) {
        // Array references are formatted without newlines
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
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
                            // Other tokens are processed as usual
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

    pub fn format_qw_expr(&mut self, node: &PerlNode) {
        let should_multiline = self.has_newline_before_first_value(node);

        if should_multiline {
            self.format_multiline_qw_expr(node);
        } else {
            self.format_single_line_qw_expr(node);
        }
    }

    fn format_single_line_qw_expr(&mut self, node: &PerlNode) {
        // Special formatting for qw() expressions
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
                            // Add spaces between QW_STRING tokens
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
                        _ => {
                            self.format_token(&token);
                        }
                    }
                }
            }
        }
    }

    pub fn format_q_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::Q_KW, SyntaxKind::Q_STRING);
    }

    pub fn format_qq_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QQ_KW, SyntaxKind::QQ_STRING);
    }

    pub fn format_qx_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QX_KW, SyntaxKind::QX_STRING);
    }

    pub fn format_m_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::M_KW, SyntaxKind::M_STRING);
    }

    pub fn format_qr_expr(&mut self, node: &PerlNode) {
        self.format_q_family_expr(node, SyntaxKind::QR_KW, SyntaxKind::QR_STRING);
    }

    pub fn format_s_expr(&mut self, node: &PerlNode) {
        // Substitution expressions have the form s/pattern/replacement/flags
        // We need to handle the pattern and replacement parts separately
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();
                    match kind {
                        SyntaxKind::S_KW => {
                            self.format_token(&token);
                        }
                        SyntaxKind::WHITESPACE => {
                            // Special handling: preserve whitespace inside substitution strings
                            self.output.push_str(text);
                        }
                        _ => {
                            // Handle any remaining tokens (including delimiters, pattern, replacement, and flags) directly
                            self.output.push_str(text);
                            self.prev_token_kind = Some(kind);
                        }
                    }
                }
            }
        }
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
                        SyntaxKind::L_PAREN | SyntaxKind::L_BRACKET | SyntaxKind::SLASH => {
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
                            // Special handling: preserve whitespace inside q-family strings
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
}

#[cfg(test)]
mod tests {
    use crate::format;
    use crate::parse_perl;

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
    2, 3
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
}
