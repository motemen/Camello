use std::collections::HashMap;

use rowan::{NodeOrToken, SyntaxNode, SyntaxToken};

use crate::{PerlLanguage, PerlNode, SyntaxKind};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CommentType {
    /// Comment appears after a construct on the same line
    Trailing { owner: PerlNode },
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
        // For now, we only analyze trailing comments after variable declarations
        // Other comment types could be added here in the future
        self.find_trailing_var_decl_owner(comment)
            .map(|owner| CommentType::Trailing { owner })
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
}
