use crate::{PerlNode, SyntaxKind};
use rowan::NodeOrToken;

use super::Formatter;

impl Formatter {
    /// Format a data section (__END__ or __DATA__)
    /// Data sections should be preserved exactly as-is without any formatting changes
    pub fn format_data_section(&mut self, node: &PerlNode) {
        // Ensure we're on a new line before the data section
        if !self.at_line_start {
            self.handle_newline();
        }

        // Process all children (keyword + data content) without any modifications
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Token(token) => {
                    let text = token.text();
                    match token.kind() {
                        SyntaxKind::END_KW | SyntaxKind::DATA_KW => {
                            // Output the keyword exactly as-is; do not force a newline
                            self.write(text);
                        }
                        SyntaxKind::DATA_SECTION | SyntaxKind::RAW_STRING => {
                            // Output the data content exactly as-is, preserving all formatting
                            self.write(text);
                        }
                        _ => {
                            // Handle any other tokens (whitespace, etc.) as-is
                            self.write(text);
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
    pub fn format_pod_block(&mut self, node: &PerlNode) {
        // Ensure we're on a new line before the POD block
        if !self.at_line_start {
            self.handle_newline();
        }

        // Process all children (POD command + content + =cut) without any modifications
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Token(token) => {
                    self.write(token.text());
                }
                NodeOrToken::Node(_) => {
                    unreachable!("POD blocks should not contain nested nodes");
                }
            }
        }
    }
}
