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
        // ハッシュリファレンスは改行なしでフォーマット
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            self.handle_whitespace(&token);
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
        // 配列リファレンスは改行なしでフォーマット
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => {
                    let kind = token.kind();
                    let text = token.text();

                    match kind {
                        SyntaxKind::WHITESPACE => {
                            self.handle_whitespace(&token);
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
}

#[cfg(test)]
mod tests {}
