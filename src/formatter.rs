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
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => self.format_token(&token),
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
                if !self.at_line_start {
                    self.output.push(' ');
                }
                self.output.push_str(text);
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

            // カンマの後
            (Some(SyntaxKind::COMMA), _) => true,

            // キーワードの後
            (Some(SyntaxKind::MY_KW), _) => true,
            (Some(SyntaxKind::SUB_KW), SyntaxKind::IDENT) => true,

            // Before left brace "{""
            (Some(_), SyntaxKind::L_BRACE) => true,

            // After identifier not followed by a semicolon
            (Some(SyntaxKind::IDENT), kind) if kind != SyntaxKind::SEMICOLON => true,

            // 括弧の内側はスペースなし
            (Some(SyntaxKind::L_PAREN), _) | (Some(_), SyntaxKind::R_PAREN) => false,
            (Some(SyntaxKind::L_BRACE), _) => false,

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
        let (syntax, _) = parse_perl(input);
        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @"my $var = 1;");
    }

    #[test]
    fn test_sub_def_formatting() {
        let input = "sub test{my$x=1;foo$x;bar;}";
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        sub test {
            my $x = 1;
            foo $x;
            bar;
        }
        ");
    }

    #[test]
    fn test_indentation() {
        let input = "sub outer { sub inner { my $var = 1; } }";
        let (syntax, _) = parse_perl(input);
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
        let (syntax, _) = parse_perl(input);
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
}
