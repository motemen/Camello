use rowan::NodeOrToken;

use crate::PerlNode;

use super::Formatter;

impl Formatter {
    pub(super) fn format_use_no_stmt(&mut self, node: &PerlNode) {
        // Output pending empty lines before processing use/no statement
        if self.pending_empty_lines > 0 {
            self.output_pending_empty_lines();
        }

        // Special handling for use/no statements: add space between identifier and parentheses
        for child in node.children_with_tokens() {
            let is_module_name = match &child {
                NodeOrToken::Node(n) => n.kind() == crate::SyntaxKind::QUALIFIED_IDENT,
                NodeOrToken::Token(t) => t.kind() == crate::SyntaxKind::IDENT,
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
                        if next_token.kind() == crate::SyntaxKind::L_PAREN {
                            self.write_char(' ');
                        }
                    }
                }
            }
        }
    }
}
