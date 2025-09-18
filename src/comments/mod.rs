//! Comment ownership and placement modeling.
//!
//! This module provides reusable data structures that describe how raw comment
//! tokens in the syntax tree relate to surrounding nodes or tokens.  The
//! formatter can use these structures to decide where each comment should be
//! rendered, but other components (such as future lint passes) can also consume
//! the same information without depending on formatter internals.

use std::collections::HashMap;

use crate::{PerlLanguage, SyntaxKind};
use rowan::ast::SyntaxNodePtr;
use rowan::{SyntaxNode, SyntaxToken, TextRange};

/// A stable identifier for a token inside a syntax tree.
///
/// The identifier stores the token's [`SyntaxKind`] together with its
/// [`TextRange`].  This is sufficient to resolve the token again on demand as
/// long as the caller provides a syntax tree built from the same source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenKey {
    kind: SyntaxKind,
    range: TextRange,
}

impl TokenKey {
    /// Creates a key from raw parts.
    #[must_use]
    pub fn new(kind: SyntaxKind, range: TextRange) -> Self {
        Self { kind, range }
    }

    /// Creates a key for the provided token.
    #[must_use]
    pub fn from_token(token: &SyntaxToken<PerlLanguage>) -> Self {
        Self {
            kind: token.kind(),
            range: token.text_range(),
        }
    }

    /// The kind of token identified by this key.
    #[must_use]
    pub fn kind(self) -> SyntaxKind {
        self.kind
    }

    /// The text range covered by the token.
    #[must_use]
    pub fn text_range(self) -> TextRange {
        self.range
    }

    /// Resolves the key back to an actual token inside `root` if possible.
    #[must_use]
    pub fn resolve(self, root: &SyntaxNode<PerlLanguage>) -> Option<SyntaxToken<PerlLanguage>> {
        if root.parent().is_some() {
            return None;
        }

        root.descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == self.kind && token.text_range() == self.range)
    }
}

/// Identifier for a comment token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommentId(TokenKey);

impl CommentId {
    /// Attempts to create a comment identifier from the provided token.
    #[must_use]
    pub fn from_token(token: &SyntaxToken<PerlLanguage>) -> Option<Self> {
        if token.kind() == SyntaxKind::COMMENT {
            Some(Self(TokenKey::from_token(token)))
        } else {
            None
        }
    }

    /// Attempts to create a comment identifier from a token key.
    #[must_use]
    pub fn from_token_key(key: TokenKey) -> Option<Self> {
        if key.kind == SyntaxKind::COMMENT {
            Some(Self(key))
        } else {
            None
        }
    }

    /// Returns the underlying token key.
    #[must_use]
    pub fn token_key(self) -> TokenKey {
        self.0
    }

    /// Convenience accessor for the covered text range.
    #[must_use]
    pub fn text_range(self) -> TextRange {
        self.0.text_range()
    }

    /// Resolves this identifier back to the concrete comment token.
    #[must_use]
    pub fn resolve(self, root: &SyntaxNode<PerlLanguage>) -> Option<SyntaxToken<PerlLanguage>> {
        self.0.resolve(root)
    }
}

/// Represents the owner a comment is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommentOwner {
    /// The comment is associated with an entire syntax node.
    Node(SyntaxNodePtr<PerlLanguage>),
    /// The comment is associated with an individual token.
    Token(TokenKey),
}

impl CommentOwner {
    /// Creates an owner representing the supplied node.
    #[must_use]
    pub fn for_node(node: &SyntaxNode<PerlLanguage>) -> Self {
        Self::Node(SyntaxNodePtr::new(node))
    }

    /// Creates an owner from a previously stored pointer.
    #[must_use]
    pub fn from_node_ptr(ptr: SyntaxNodePtr<PerlLanguage>) -> Self {
        Self::Node(ptr)
    }

    /// Creates an owner representing the supplied token.
    #[must_use]
    pub fn for_token(token: &SyntaxToken<PerlLanguage>) -> Self {
        Self::Token(TokenKey::from_token(token))
    }

    /// Creates an owner from a token key.
    #[must_use]
    pub fn from_token_key(key: TokenKey) -> Self {
        Self::Token(key)
    }

    /// Returns the stored node pointer, if any.
    #[must_use]
    pub fn node_ptr(self) -> Option<SyntaxNodePtr<PerlLanguage>> {
        match self {
            Self::Node(ptr) => Some(ptr),
            Self::Token(_) => None,
        }
    }

    /// Returns the stored token key, if any.
    #[must_use]
    pub fn token_key(self) -> Option<TokenKey> {
        match self {
            Self::Node(_) => None,
            Self::Token(key) => Some(key),
        }
    }

    /// Resolves the owner to the current syntax tree.
    #[must_use]
    pub fn resolve(self, root: &SyntaxNode<PerlLanguage>) -> Option<CommentAnchor> {
        match self {
            Self::Node(ptr) => ptr.try_to_node(root).map(CommentAnchor::Node),
            Self::Token(key) => key.resolve(root).map(CommentAnchor::Token),
        }
    }
}

/// Result of resolving a [`CommentOwner`] against a syntax tree.
#[derive(Debug, Clone)]
pub enum CommentAnchor {
    Node(SyntaxNode<PerlLanguage>),
    Token(SyntaxToken<PerlLanguage>),
}

/// Describes how a comment relates to its owner (if any).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommentPlacement {
    /// The comment appears before its owner, typically on a separate line.
    Leading(CommentOwner),
    /// The comment trails its owner on the same line (inline comment).
    Trailing(CommentOwner),
    /// The comment is inside a construct but not directly tied to any child.
    Dangling(CommentOwner),
    /// The comment does not have a semantic owner.
    Standalone,
}

impl CommentPlacement {
    /// Returns the owner referenced by this placement, if any.
    #[must_use]
    pub fn owner(self) -> Option<CommentOwner> {
        match self {
            Self::Leading(owner) | Self::Trailing(owner) | Self::Dangling(owner) => Some(owner),
            Self::Standalone => None,
        }
    }

    /// Convenience helper for `matches!(self, Self::Leading(_))`.
    #[must_use]
    pub fn is_leading(self) -> bool {
        matches!(self, Self::Leading(_))
    }

    /// Convenience helper for `matches!(self, Self::Trailing(_))`.
    #[must_use]
    pub fn is_trailing(self) -> bool {
        matches!(self, Self::Trailing(_))
    }

    /// Convenience helper for `matches!(self, Self::Dangling(_))`.
    #[must_use]
    pub fn is_dangling(self) -> bool {
        matches!(self, Self::Dangling(_))
    }

    /// Convenience helper for `matches!(self, Self::Standalone)`.
    #[must_use]
    pub fn is_standalone(self) -> bool {
        matches!(self, Self::Standalone)
    }
}

/// Mapping between comment identifiers and their placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommentAssignment {
    comment: CommentId,
    placement: CommentPlacement,
}

impl CommentAssignment {
    /// Creates a new assignment.
    #[must_use]
    pub fn new(comment: CommentId, placement: CommentPlacement) -> Self {
        Self { comment, placement }
    }

    /// Returns the associated comment identifier.
    #[must_use]
    pub fn comment(self) -> CommentId {
        self.comment
    }

    /// Returns the stored placement.
    #[must_use]
    pub fn placement(self) -> CommentPlacement {
        self.placement
    }
}

/// A collection describing all comment assignments for a syntax tree.
#[derive(Debug, Default, Clone)]
pub struct CommentModel {
    entries: Vec<CommentAssignment>,
    index: HashMap<CommentId, usize>,
}

impl CommentModel {
    /// Creates an empty comment model.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty model with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            index: HashMap::with_capacity(capacity),
        }
    }

    /// Builds a comment model for the provided syntax tree.
    #[must_use]
    pub fn from_syntax(root: &SyntaxNode<PerlLanguage>) -> Self {
        let mut model = CommentModel::new();
        for node in std::iter::once(root.clone()).chain(root.descendants()) {
            if node.kind() == SyntaxKind::SUB_DEF {
                collect_leading_function_comments(&node, &mut model);
            }
        }
        model
    }

    /// Adds or replaces a comment assignment.
    ///
    /// Returns the previous assignment for the same comment if it existed.
    pub fn set(&mut self, assignment: CommentAssignment) -> Option<CommentAssignment> {
        if let Some(index) = self.index.get(&assignment.comment).copied() {
            let previous = std::mem::replace(&mut self.entries[index], assignment);
            Some(previous)
        } else {
            let index = self.entries.len();
            self.entries.push(assignment);
            self.index.insert(assignment.comment, index);
            None
        }
    }

    /// Returns the assignment associated with the given comment identifier.
    #[must_use]
    pub fn assignment(&self, id: CommentId) -> Option<&CommentAssignment> {
        self.index.get(&id).copied().map(|idx| &self.entries[idx])
    }

    /// Returns the placement associated with the given comment identifier.
    #[must_use]
    pub fn placement_of(&self, id: CommentId) -> Option<CommentPlacement> {
        self.assignment(id).map(|entry| entry.placement)
    }

    /// Removes the assignment for the provided comment identifier.
    pub fn remove(&mut self, id: CommentId) -> Option<CommentAssignment> {
        let index = self.index.remove(&id)?;
        let removed = self.entries.remove(index);
        for (idx, entry) in self.entries.iter().enumerate().skip(index) {
            self.index.insert(entry.comment, idx);
        }
        Some(removed)
    }

    /// Returns an iterator over all assignments in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &CommentAssignment> {
        self.entries.iter()
    }

    /// Returns an iterator over assignments attached to the specified owner.
    pub fn attached_to(&self, owner: CommentOwner) -> impl Iterator<Item = &CommentAssignment> {
        self.entries
            .iter()
            .filter(move |assignment| assignment.placement.owner() == Some(owner))
    }

    /// Returns the number of stored assignments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if there are no stored assignments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn collect_leading_function_comments(node: &SyntaxNode<PerlLanguage>, model: &mut CommentModel) {
    let Some(first_token) = node.first_token() else {
        return;
    };

    let owner = CommentOwner::for_node(node);
    let mut comments: Vec<CommentId> = Vec::new();
    let mut current = first_token.prev_token();
    let mut newline_streak = 0usize;

    while let Some(token) = current {
        match token.kind() {
            SyntaxKind::WHITESPACE => {
                current = token.prev_token();
            }
            SyntaxKind::NEWLINE => {
                newline_streak += 1;
                if newline_streak >= 2 {
                    break;
                }
                current = token.prev_token();
            }
            SyntaxKind::COMMENT => {
                if !is_line_comment(&token) {
                    break;
                }
                newline_streak = 0;
                if let Some(comment_id) = CommentId::from_token(&token) {
                    comments.push(comment_id);
                }
                current = token.prev_token();
            }
            _ => break,
        }
    }

    for comment_id in comments.into_iter().rev() {
        model.set(CommentAssignment::new(
            comment_id,
            CommentPlacement::Leading(owner),
        ));
    }
}

fn is_line_comment(token: &SyntaxToken<PerlLanguage>) -> bool {
    let mut current = token.prev_token();
    while let Some(prev) = current {
        match prev.kind() {
            SyntaxKind::WHITESPACE => {
                current = prev.prev_token();
            }
            SyntaxKind::NEWLINE => return true,
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use rowan::TextSize;

    #[test]
    fn token_key_basic() {
        let range = TextRange::new(TextSize::from(0), TextSize::from(7));
        let key = TokenKey::new(SyntaxKind::COMMENT, range);
        assert_eq!(key.kind(), SyntaxKind::COMMENT);
        assert_eq!(key.text_range(), range);
        let comment_id = CommentId::from_token_key(key).expect("should be a comment");
        assert_eq!(comment_id.text_range(), range);
    }

    #[test]
    fn model_insert_and_query() {
        let mut model = CommentModel::new();
        let comment_range = TextRange::new(TextSize::from(0), TextSize::from(10));
        let comment_id =
            CommentId::from_token_key(TokenKey::new(SyntaxKind::COMMENT, comment_range)).unwrap();
        let owner_key = TokenKey::new(
            SyntaxKind::IDENT,
            TextRange::new(TextSize::from(11), TextSize::from(12)),
        );
        let owner = CommentOwner::from_token_key(owner_key);
        let placement = CommentPlacement::Trailing(owner);
        let assignment = CommentAssignment::new(comment_id, placement);

        assert!(model.set(assignment).is_none());
        assert_eq!(model.len(), 1);
        assert!(!model.is_empty());
        assert_eq!(model.placement_of(comment_id), Some(placement));
        let collected: Vec<_> = model.attached_to(owner).collect();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].comment(), comment_id);

        let removed = model.remove(comment_id).expect("expected removal");
        assert_eq!(removed.comment(), comment_id);
        assert!(model.is_empty());
    }
}
