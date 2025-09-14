use std::collections::HashMap;

use rowan::{NodeOrToken, SyntaxNode, SyntaxToken};

use crate::{PerlLanguage, PerlNode, SyntaxKind};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CommentType {
    /// Comment appears after a construct on the same line
    Trailing { owner: PerlNode },
    /// Comment appears before a subroutine definition (documentation)
    SubroutineDoc { owner: PerlNode },
}

#[derive(Debug, Default)]
pub(crate) struct CommentAnalyzer {
    pub ownership: HashMap<SyntaxToken<PerlLanguage>, CommentType>,
}

impl CommentAnalyzer {
    pub fn analyze(root: &PerlNode) -> Self {
        let mut analyzer = Self {
            ownership: HashMap::new(),
        };
        analyzer.analyze_node(root);
        analyzer
    }

    fn analyze_node(&mut self, node: &SyntaxNode<PerlLanguage>) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::COMMENT => {
                    if let Some(ownership) = self.determine_comment_ownership(&t) {
                        self.ownership.insert(t, ownership);
                    }
                }
                NodeOrToken::Node(n) => self.analyze_node(&n),
                _ => {}
            }
        }
    }

    fn determine_comment_ownership(
        &self,
        comment: &SyntaxToken<PerlLanguage>,
    ) -> Option<CommentType> {
        // Check for trailing comments after variable declarations
        if let Some(owner) = self.find_trailing_var_decl_owner(comment) {
            return Some(CommentType::Trailing { owner });
        }

        // Check for subroutine documentation comments
        if let Some(owner) = self.find_subroutine_doc_owner(comment) {
            return Some(CommentType::SubroutineDoc { owner });
        }

        None
    }

    fn find_trailing_var_decl_owner(
        &self,
        comment: &SyntaxToken<PerlLanguage>,
    ) -> Option<PerlNode> {
        let mut prev = comment.prev_token()?;
        let mut saw_newline = false;

        while prev.kind() == SyntaxKind::WHITESPACE {
            if prev.text().contains('\n') {
                saw_newline = true;
                break;
            }
            prev = prev.prev_token()?;
        }

        if saw_newline {
            return None;
        }

        if prev.kind() == SyntaxKind::SEMICOLON {
            let ancestor = prev.parent()?;
            return ancestor
                .ancestors()
                .find(|n| n.kind() == SyntaxKind::DECLARATION_STMT);
        }
        None
    }

    fn find_subroutine_doc_owner(&self, comment: &SyntaxToken<PerlLanguage>) -> Option<PerlNode> {
        // A doc comment must be at the start of a line (preceded only by whitespace).
        let mut prev_opt = comment.prev_token();
        while let Some(prev) = prev_opt {
            if prev.text().contains('\n') {
                break;
            }
            if prev.kind() != SyntaxKind::WHITESPACE {
                return None; // It's a trailing comment.
            }
            prev_opt = prev.prev_token();
        }

        // Find comments that appear immediately before a SUB_DEF
        // We need to check if the next non-whitespace node after this comment is a SUB_DEF

        let mut next = comment.next_token();
        let mut saw_newline = false;

        // Skip whitespace and comment tokens, but track if we see newlines
        while let Some(token) = &next {
            if token.kind() == SyntaxKind::WHITESPACE {
                if token.text().contains('\n') {
                    saw_newline = true;
                }
                next = token.next_token();
            } else if token.kind() == SyntaxKind::COMMENT {
                next = token.next_token();
            } else {
                break;
            }
        }

        // If we found a token after whitespace, check if its parent is a SUB_DEF
        if let Some(next_token) = next {
            // Look for a parent that is a SUB_DEF
            let mut parent = next_token.parent();
            while let Some(node) = parent {
                if node.kind() == SyntaxKind::SUB_DEF {
                    // Make sure this comment appears on a line before the sub
                    // (not inline with the sub keyword)
                    if saw_newline {
                        return Some(node);
                    }
                }
                parent = node.parent();
            }
        }

        None
    }
}
