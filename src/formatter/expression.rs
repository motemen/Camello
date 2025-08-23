// formatter/expression.rs
// 式（expression）に関するフォーマットロジック

use rowan::NodeOrToken;

use crate::{PerlNode, SyntaxKind};

use super::Formatter;

impl Formatter {
    pub fn format_function_call(&mut self, node: &PerlNode) {
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

    pub fn format_parenthesized_expr(&mut self, node: &PerlNode) {
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

    pub fn format_block_function_call(&mut self, node: &PerlNode) {
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
}
