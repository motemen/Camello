use crate::{PerlNode, SyntaxKind};

use super::Formatter;

impl Formatter {
    pub(super) fn format_quote_like(&mut self, node: &PerlNode) {
        match node.kind() {
            SyntaxKind::Q_EXPR => {
                self.format_q_expr(node);
            }
            SyntaxKind::QQ_EXPR => {
                self.format_qq_expr(node);
            }
            SyntaxKind::QW_EXPR => {
                self.format_qw_expr(node);
            }
            SyntaxKind::QX_EXPR => {
                self.format_qx_expr(node);
            }
            SyntaxKind::M_EXPR => {
                self.format_m_expr(node);
            }
            SyntaxKind::QR_EXPR => {
                self.format_qr_expr(node);
            }
            SyntaxKind::S_EXPR => {
                self.format_s_expr(node);
            }
            SyntaxKind::TR_EXPR => {
                self.format_tr_expr(node);
            }
            _ => {
                // Default child iteration
                for child in node.children_with_tokens() {
                    match child {
                        rowan::NodeOrToken::Node(child_node) => self.format_node(&child_node, super::FormatContext::default()),
                        rowan::NodeOrToken::Token(token) => self.format_token(&token, super::FormatContext::default()),
                    }
                }
            }
        }
    }
}
