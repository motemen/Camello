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
use rowan::{NodeOrToken, SyntaxNode, SyntaxToken, TextRange, WalkEvent};

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

        root.token_at_offset(self.range.start())
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

/// Identifier for a comment block consisting of one or more consecutive comment tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommentBlockId(usize);

impl CommentBlockId {
    /// Returns the underlying index for this block.
    #[must_use]
    pub fn as_usize(self) -> usize {
        self.0
    }
}

/// A contiguous run of comment tokens that should be treated as a single block.
#[derive(Debug, Clone)]
pub struct CommentBlock {
    id: CommentBlockId,
    comments: Vec<CommentId>,
}

impl CommentBlock {
    fn new(id: CommentBlockId, comments: Vec<CommentId>) -> Self {
        debug_assert!(!comments.is_empty(), "comment block must contain comments");
        Self { id, comments }
    }

    /// Returns the identifier of this block.
    #[must_use]
    pub fn id(&self) -> CommentBlockId {
        self.id
    }

    /// Returns the comments contained in this block.
    #[must_use]
    pub fn comments(&self) -> &[CommentId] {
        &self.comments
    }

    /// Returns the first comment contained in this block.
    #[must_use]
    pub fn first_comment(&self) -> Option<CommentId> {
        self.comments.first().copied()
    }

    /// Returns `true` if the block contains the specified comment identifier.
    #[must_use]
    pub fn contains(&self, id: CommentId) -> bool {
        self.comments.contains(&id)
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

/// Mapping between comment blocks and their placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommentAssignment {
    block: CommentBlockId,
    placement: CommentPlacement,
}

impl CommentAssignment {
    /// Creates a new assignment.
    #[must_use]
    pub fn new(block: CommentBlockId, placement: CommentPlacement) -> Self {
        Self { block, placement }
    }

    /// Returns the associated comment block identifier.
    #[must_use]
    pub fn block(self) -> CommentBlockId {
        self.block
    }

    /// Returns the stored placement.
    #[must_use]
    pub fn placement(self) -> CommentPlacement {
        self.placement
    }
}

/// A registry describing all comment blocks and their assignments for a syntax tree.
#[derive(Debug, Default, Clone)]
pub struct CommentRegistry {
    blocks: Vec<Option<CommentBlock>>,
    assignments: Vec<Option<CommentAssignment>>,
    comment_to_block: HashMap<CommentId, CommentBlockId>,
}

impl CommentRegistry {
    /// Creates an empty comment registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty registry with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            blocks: Vec::with_capacity(capacity),
            assignments: Vec::with_capacity(capacity),
            comment_to_block: HashMap::with_capacity(capacity),
        }
    }

    /// Builds a comment registry for the provided syntax tree.
    #[must_use]
    pub fn from_syntax(root: &SyntaxNode<PerlLanguage>) -> Self {
        let mut registry = CommentRegistry::new();
        build_comment_blocks(root, &mut registry);

        for node in std::iter::once(root.clone()).chain(root.descendants()) {
            if node.kind() == SyntaxKind::SUB_DEF {
                collect_leading_function_comments(&node, &mut registry);
            }
        }

        registry
    }

    /// Registers a new comment block with the registry.
    fn add_block(&mut self, comments: Vec<CommentId>) -> CommentBlockId {
        debug_assert!(!comments.is_empty(), "cannot register empty comment block");
        let id = CommentBlockId(self.blocks.len());
        let block = CommentBlock::new(id, comments);
        for comment in block.comments() {
            let previous = self.comment_to_block.insert(*comment, id);
            debug_assert!(previous.is_none(), "comment assigned to multiple blocks");
        }
        self.blocks.push(Some(block));
        self.assignments.push(None);
        id
    }

    /// Returns an iterator over all registered comment blocks in lexical order.
    pub fn blocks(&self) -> impl Iterator<Item = &CommentBlock> {
        self.blocks.iter().filter_map(Option::as_ref)
    }

    /// Returns the comment block corresponding to the provided identifier.
    #[must_use]
    pub fn block(&self, id: CommentBlockId) -> Option<&CommentBlock> {
        self.blocks
            .get(id.as_usize())
            .and_then(|entry| entry.as_ref())
    }

    /// Returns the block identifier containing the specified comment.
    #[must_use]
    pub fn block_of(&self, comment: CommentId) -> Option<CommentBlockId> {
        self.comment_to_block.get(&comment).copied()
    }

    /// Returns the comments that belong to the provided block.
    #[must_use]
    pub fn block_comments(&self, id: CommentBlockId) -> Option<&[CommentId]> {
        self.block(id).map(|block| block.comments())
    }

    /// Returns `true` if the supplied comment is the first one in its block.
    #[must_use]
    pub fn is_first_in_block(&self, comment: CommentId) -> bool {
        self.block_of(comment)
            .and_then(|block_id| self.block(block_id))
            .and_then(|block| block.first_comment())
            .is_some_and(|first| first == comment)
    }

    /// Adds or replaces a comment block assignment.
    ///
    /// Returns the previous assignment for the same block if it existed.
    pub fn set(&mut self, assignment: CommentAssignment) -> Option<CommentAssignment> {
        let index = assignment.block().as_usize();
        if index >= self.assignments.len() {
            debug_assert!(false, "assignment references unknown comment block");
            return None;
        }
        self.assignments[index].replace(assignment)
    }

    /// Returns the assignment associated with the given comment block identifier.
    #[must_use]
    pub fn assignment(&self, block: CommentBlockId) -> Option<&CommentAssignment> {
        self.assignments
            .get(block.as_usize())
            .and_then(|entry| entry.as_ref())
    }

    /// Returns the placement associated with the given comment block identifier.
    #[must_use]
    pub fn placement_of_block(&self, block: CommentBlockId) -> Option<CommentPlacement> {
        self.assignment(block).map(|entry| entry.placement())
    }

    /// Returns the placement associated with the given comment identifier.
    #[must_use]
    pub fn placement_of(&self, comment: CommentId) -> Option<CommentPlacement> {
        self.block_of(comment)
            .and_then(|block| self.placement_of_block(block))
    }

    /// Removes the assignment for the provided comment block identifier.
    pub fn remove(&mut self, block: CommentBlockId) -> Option<CommentAssignment> {
        self.assignments
            .get_mut(block.as_usize())
            .and_then(|entry| entry.take())
    }

    /// Removes the assignment for the block containing the specified comment identifier.
    pub fn remove_for_comment(&mut self, comment: CommentId) -> Option<CommentAssignment> {
        let block = self.block_of(comment)?;
        self.remove(block)
    }

    /// Returns an iterator over all assignments in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &CommentAssignment> {
        self.assignments.iter().filter_map(Option::as_ref)
    }

    /// Returns an iterator over assignments attached to the specified owner.
    pub fn attached_to(&self, owner: CommentOwner) -> impl Iterator<Item = &CommentAssignment> {
        self.assignments
            .iter()
            .filter_map(Option::as_ref)
            .filter(move |assignment| assignment.placement.owner() == Some(owner))
    }

    /// Returns the number of stored assignments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.assignments
            .iter()
            .filter(|entry| entry.is_some())
            .count()
    }

    /// Returns `true` if there are no stored assignments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.assignments.iter().all(|entry| entry.is_none())
    }
}

fn flush_pending_block(registry: &mut CommentRegistry, pending: &mut Vec<CommentId>) {
    if pending.is_empty() {
        return;
    }

    let comments = std::mem::take(pending);
    registry.add_block(comments);
}

fn build_comment_blocks(root: &SyntaxNode<PerlLanguage>, registry: &mut CommentRegistry) {
    let mut pending_block: Vec<CommentId> = Vec::new();

    for event in root.preorder_with_tokens() {
        if let WalkEvent::Enter(NodeOrToken::Token(token)) = event {
            match token.kind() {
                SyntaxKind::COMMENT => {
                    if let Some(comment_id) = CommentId::from_token(&token) {
                        pending_block.push(comment_id);
                    }
                }
                SyntaxKind::WHITESPACE => {
                    // Keep the current block open across indentation tokens.
                }
                _ => {
                    flush_pending_block(registry, &mut pending_block);
                }
            }
        }
    }

    flush_pending_block(registry, &mut pending_block);
}

fn collect_leading_function_comments(
    node: &SyntaxNode<PerlLanguage>,
    registry: &mut CommentRegistry,
) {
    let Some(first_token) = node.first_token() else {
        return;
    };

    let owner = CommentOwner::for_node(node);
    let mut blocks: Vec<CommentBlockId> = Vec::new();
    let mut current = first_token.prev_token();

    while let Some(token) = current {
        match token.kind() {
            SyntaxKind::WHITESPACE => {}
            SyntaxKind::NEWLINE => break,
            SyntaxKind::COMMENT => {
                if !is_line_comment(&token) {
                    break;
                }
                if let Some(comment_id) = CommentId::from_token(&token) {
                    if let Some(block_id) = registry.block_of(comment_id) {
                        if blocks.last().copied() != Some(block_id) {
                            blocks.push(block_id);
                        }
                    }
                }
            }
            _ => break,
        }

        current = token.prev_token();
    }

    for block in blocks.into_iter().rev() {
        registry.set(CommentAssignment::new(
            block,
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
            SyntaxKind::NEWLINE | SyntaxKind::COMMENT => return true,
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_perl;
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
    fn registry_insert_and_query() {
        let mut registry = CommentRegistry::new();
        let comment_range = TextRange::new(TextSize::from(0), TextSize::from(10));
        let comment_id =
            CommentId::from_token_key(TokenKey::new(SyntaxKind::COMMENT, comment_range)).unwrap();
        let owner_key = TokenKey::new(
            SyntaxKind::IDENT,
            TextRange::new(TextSize::from(11), TextSize::from(12)),
        );
        let owner = CommentOwner::from_token_key(owner_key);
        let placement = CommentPlacement::Trailing(owner);
        let block = registry.add_block(vec![comment_id]);
        let assignment = CommentAssignment::new(block, placement);

        assert!(registry.set(assignment).is_none());
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
        assert_eq!(registry.placement_of(comment_id), Some(placement));
        let collected: Vec<_> = registry.attached_to(owner).collect();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].block(), block);
        assert!(registry.is_first_in_block(comment_id));
        assert_eq!(registry.block_comments(block).unwrap(), &[comment_id]);

        let removed = registry.remove(block).expect("expected removal");
        assert_eq!(removed.block(), block);
        assert!(registry.is_empty());
    }

    #[test]
    fn leading_comment_block_attached_to_sub() {
        let source = "my $x = 1;\n# doc one\n# doc two\nsub foo { return $x; }\n";
        let (root, _errors) = parse_perl(source);
        let registry = CommentRegistry::from_syntax(&root);

        let sub = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::SUB_DEF)
            .expect("expected sub definition");

        let owner = CommentOwner::for_node(&sub);
        let assignments: Vec<_> = registry.attached_to(owner).collect();
        assert_eq!(assignments.len(), 1);

        let assignment = assignments[0];
        assert!(assignment.placement().is_leading());

        let block_id = assignment.block();
        let comments = registry
            .block_comments(block_id)
            .expect("block should contain comments");
        assert_eq!(comments.len(), 2);
        assert!(registry.is_first_in_block(comments[0]));
        assert!(!registry.is_first_in_block(comments[1]));
    }
}
