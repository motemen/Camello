use std::collections::HashMap;

use rowan::{NodeOrToken, SyntaxNode, SyntaxToken};

use crate::{PerlLanguage, PerlNode, SyntaxKind};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CommentType {
    /// Comment appears after a construct on the same line
    Trailing { owner: PerlNode },
    /// Comment not associated with nearby code
    Standalone,
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
                    let ownership = self.determine_comment_ownership(&t);
                    self.ownership.insert(t, ownership);
                }
                NodeOrToken::Node(n) => self.analyze_node(&n),
                _ => {}
            }
        }
    }

    fn determine_comment_ownership(&self, comment: &SyntaxToken<PerlLanguage>) -> CommentType {
        if let Some(owner) = self.find_trailing_var_decl_owner(comment) {
            CommentType::Trailing { owner }
        } else {
            CommentType::Standalone
        }
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
            let mut ancestor = prev.parent()?;
            loop {
                if ancestor.kind() == SyntaxKind::DECLARATION_STMT {
                    return Some(ancestor);
                }
                match ancestor.parent() {
                    Some(p) => ancestor = p,
                    None => break,
                }
            }
        }
        None
    }
}
