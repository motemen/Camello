use crate::{PerlNode, SyntaxKind};
use rowan::{NodeOrToken, SyntaxToken};

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
            indent_string: "    ".to_string(), // 4スペース
            prev_token_kind: None,
            at_line_start: true,
        }
    }

    pub fn format(&mut self, node: &PerlNode) -> String {
        self.format_node(node);
        std::mem::take(&mut self.output)
    }

    fn format_node(&mut self, node: &PerlNode) {
        // 特別な処理が必要なノードタイプ
        match node.kind() {
            SyntaxKind::HASH_REF => {
                self.format_hash_ref(node);
                return;
            }
            SyntaxKind::QW_EXPR => {
                self.format_qw_expr(node);
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
            _ => {}
        }

        // Default child iteration
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => self.format_token(&token),
            }
        }

        // Special handling after children are processed
        if node.kind().is_variable() {
            // This is the logic from format_variable
            self.prev_token_kind = Some(node.kind());
        }
    }

    fn format_hash_ref(&mut self, node: &PerlNode) {
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

    fn format_qw_expr(&mut self, node: &PerlNode) {
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
                self.handle_newline();
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
            (Some(SyntaxKind::MY_KW), _) => true,
            (Some(SyntaxKind::SUB_KW), SyntaxKind::IDENT) => true,
            (Some(SyntaxKind::SUB_KW), SyntaxKind::QUALIFIED_IDENT) => true,
            (Some(SyntaxKind::FOR_KW), _) => true,
            (Some(SyntaxKind::FOREACH_KW), _) => true,
            (Some(SyntaxKind::WHILE_KW), _) => true,
            (Some(SyntaxKind::PACKAGE_KW), _) => true,
            (Some(SyntaxKind::USE_KW), _) => true,

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
                            | SyntaxKind::FOR_KW
                            | SyntaxKind::FOREACH_KW
                            | SyntaxKind::WHILE_KW
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
        }
        self.at_line_start = true;
    }

    fn add_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str(&self.indent_string);
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

    #[test]
    fn test_var_decl_formatting() {
        let input = "my$var=1;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $var = 1;");
    }

    #[test]
    fn test_sub_def_formatting() {
        let input = "sub test{my$x=1;foo$x;bar;baz quux;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub test {
            my $x = 1;
            foo $x;
            bar;
            baz quux;
        }
        ");
    }

    #[test]
    fn test_indentation() {
        let input = "sub outer { sub inner { my $var = 1; } }";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub outer {
            sub inner {
                my $var = 1;
            }
        }
        ");
    }

    #[test]
    fn test_comprehensive_formatting() {
        let input = "my$a=1+2;my$b=3;sub example{my$result=$a+$b;return$result;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $a = 1 + 2;
        my $b = 3;
        sub example {
            my $result = $a + $b;
            return $result;
        }
        ");
    }

    #[test]
    fn test_sub_with_var_decl_formatting() {
        let input = r#" 
        my $var = 1;
        sub test {
            my $x = 2;
        }
        "#;
        let (syntax, err) = parse_perl(input);
        assert!(
            err.is_empty(),
            "{:?}",
            err.iter()
                .map(|e| miette::Report::new(e.clone()))
                .collect::<Vec<_>>()
        );

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $var = 1;
        sub test {
            my $x = 2;
        }
        ");
    }

    #[test]
    fn test_hash_ref_formatting() {
        let input = "my$hash_ref={};";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $hash_ref = {};");
    }

    #[test]
    fn test_return_hash_ref_formatting() {
        let input = "return{};";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"return {};");
    }

    #[test]
    fn test_sub_with_hash_ref_formatting() {
        let input = "sub get_empty{return{};}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub get_empty {
            return {};
        }
        ");
    }

    #[test]
    fn test_hash_ref_with_key_value_formatting() {
        let input = "sub f{return{a=>1};}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub f {
            return {a => 1};
        }
        ");
    }

    #[test]
    fn test_multiplicative_operators_formatting() {
        let input = "my$result=$a*$b/$c%$d;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $result = $a * $b / $c % $d;");
    }

    #[test]
    fn test_operator_precedence_formatting() {
        let input = "my$result=$a+$b*$c;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $result = $a + $b * $c;");
    }

    #[test]
    fn test_x_operator_formatting() {
        let input = "my$str=$a x 3;";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $str = $a x 3;");
    }

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
    fn test_package_formatting() {
        let cases = [
            ("package Foo::Bar;", "package Foo::Bar;\n"),
            ("package   Foo  ;", "package Foo;\n"),
            (
                "package Foo::Bar::Baz::Qux;",
                "package Foo::Bar::Baz::Qux;\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_qualified_variable_formatting() {
        let cases = [
            ("$Foo::Bar::var;", "$Foo::Bar::var;\n"),
            ("@Foo::Bar::array;", "@Foo::Bar::array;\n"),
            ("%Foo::Bar::hash;", "%Foo::Bar::hash;\n"),
            (
                "$Very::Deep::Nested::Package::Name::var;",
                "$Very::Deep::Nested::Package::Name::var;\n",
            ),
        ];
        check_formatting_cases(&cases);
    }

    #[test]
    fn test_qualified_function_formatting() {
        let cases = [
            ("Foo::Bar::func();", "Foo::Bar::func();\n"),
            (
                "Very::Deep::Nested::function();",
                "Very::Deep::Nested::function();\n",
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
        let input = "printf\"%s:%d\\n\",$name,$age;";
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
}
