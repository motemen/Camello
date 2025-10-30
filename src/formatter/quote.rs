use crate::{PerlNode, SyntaxKind};

use super::Formatter;

impl Formatter {
    pub(super) fn format_quote_like(&mut self, node: &PerlNode, ctx: super::FormatContext) {
        match node.kind() {
            SyntaxKind::Q_EXPR => {
                self.format_q_expr(node, ctx);
            }
            SyntaxKind::QQ_EXPR => {
                self.format_qq_expr(node, ctx);
            }
            SyntaxKind::QW_EXPR => {
                self.format_qw_expr(node, ctx);
            }
            SyntaxKind::QX_EXPR => {
                self.format_qx_expr(node, ctx);
            }
            SyntaxKind::M_EXPR => {
                self.format_m_expr(node, ctx);
            }
            SyntaxKind::QR_EXPR => {
                self.format_qr_expr(node, ctx);
            }
            SyntaxKind::S_EXPR => {
                self.format_s_expr(node, ctx);
            }
            SyntaxKind::TR_EXPR => {
                self.format_tr_expr(node, ctx);
            }
            _ => {
                // Default child iteration
                for child in node.children_with_tokens() {
                    match child {
                        rowan::NodeOrToken::Node(child_node) => self.format_node(&child_node, ctx),
                        rowan::NodeOrToken::Token(token) => self.format_token(&token, ctx),
                    }
                }
            }
        }
    }
}
