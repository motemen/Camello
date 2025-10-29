//! Comment and trivia ownership modeling.
//!
//! This module performs a single linear pass over the token stream to assign
//! trivia (comments, whitespace, and newlines) to their surrounding tokens. For
//! each non-trivia token we record the trivia that precedes it (leading trivia)
//! as well as the trivia that trails it on the same line (trailing trivia). A
//! newline acts as the split point: trivia before the first newline belongs to
//! the trailing side of the previous token, while the newline itself and any
//! following trivia are associated with the leading side of the next token.

use std::collections::HashMap;

use crate::{PerlLanguage, SyntaxKind};
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
        debug_assert!(
            root.parent().is_none(),
            "resolve must be called on the root node"
        );
        if root.parent().is_some() {
            return None;
        }

        root.token_at_offset(self.range.start())
            .find(|token| token.kind() == self.kind && token.text_range() == self.range)
    }
}

/// A piece of trivia (whitespace, newline, or comment) that belongs to a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TriviaPiece {
    key: TokenKey,
}

impl TriviaPiece {
    #[must_use]
    pub fn new(key: TokenKey) -> Self {
        Self { key }
    }

    /// Returns the underlying token key.
    #[must_use]
    pub fn token_key(self) -> TokenKey {
        self.key
    }

    /// Returns the kind of trivia represented by this piece.
    #[must_use]
    pub fn kind(self) -> SyntaxKind {
        self.key.kind()
    }

    /// Resolves the trivia piece back to the concrete token inside `root`.
    #[must_use]
    pub fn resolve(self, root: &SyntaxNode<PerlLanguage>) -> Option<SyntaxToken<PerlLanguage>> {
        self.key.resolve(root)
    }
}

/// Describes which token owns a trivia piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriviaPosition {
    Leading(TokenKey),
    Trailing(TokenKey),
}

impl TriviaPosition {
    /// Returns the owning token key.
    #[must_use]
    pub fn token_key(self) -> TokenKey {
        match self {
            Self::Leading(key) | Self::Trailing(key) => key,
        }
    }

    /// Returns `true` if the trivia belongs to the leading side of its owner.
    #[must_use]
    pub fn is_leading(self) -> bool {
        matches!(self, Self::Leading(_))
    }

    /// Returns `true` if the trivia belongs to the trailing side of its owner.
    #[must_use]
    pub fn is_trailing(self) -> bool {
        matches!(self, Self::Trailing(_))
    }

    /// Resolves the owning token to the current syntax tree.
    #[must_use]
    pub fn resolve(self, root: &SyntaxNode<PerlLanguage>) -> Option<SyntaxToken<PerlLanguage>> {
        self.token_key().resolve(root)
    }
}

#[derive(Debug, Clone)]
struct TokenTrivia {
    leading: Vec<TriviaPiece>,
    trailing: Vec<TriviaPiece>,
}

impl TokenTrivia {
    fn new() -> Self {
        Self {
            leading: Vec::new(),
            trailing: Vec::new(),
        }
    }
}

/// Stores trivia ownership information for every token in a syntax tree.
#[derive(Debug, Default, Clone)]
pub struct TriviaTable {
    entries: Vec<TokenTrivia>,
    token_index: HashMap<TokenKey, usize>,
    trivia_index: HashMap<TokenKey, TriviaPosition>,
}

impl TriviaTable {
    /// Builds a trivia table for the provided syntax tree.
    #[must_use]
    pub fn from_syntax(root: &SyntaxNode<PerlLanguage>) -> Self {
        let mut table = Self {
            entries: Vec::new(),
            token_index: HashMap::new(),
            trivia_index: HashMap::new(),
        };

        let mut pending_leading: Vec<TriviaPiece> = Vec::new();
        let mut pending_trailing: Vec<TriviaPiece> = Vec::new();
        let mut after_newline = false;
        let mut previous_entry: Option<usize> = None;
        let mut previous_key: Option<TokenKey> = None;

        let mut current = root.first_token();
        while let Some(token) = current {
            if token.kind().is_trivia() {
                let piece = TriviaPiece::new(TokenKey::from_token(&token));

                if token.kind() == SyntaxKind::NEWLINE {
                    if let (Some(entry_idx), Some(owner_key)) = (previous_entry, previous_key) {
                        table.attach_trailing(entry_idx, owner_key, &mut pending_trailing);
                    } else {
                        pending_leading.append(&mut pending_trailing);
                    }
                    pending_leading.push(piece);
                    after_newline = true;
                } else if after_newline || previous_entry.is_none() {
                    pending_leading.push(piece);
                } else {
                    pending_trailing.push(piece);
                }
            } else {
                let token_key = TokenKey::from_token(&token);

                if previous_entry.is_none() && !pending_trailing.is_empty() {
                    pending_leading.append(&mut pending_trailing);
                } else if let (Some(entry_idx), Some(owner_key)) = (previous_entry, previous_key) {
                    table.attach_trailing(entry_idx, owner_key, &mut pending_trailing);
                }

                let entry_idx = table.entries.len();
                table.entries.push(TokenTrivia::new());
                table.token_index.insert(token_key, entry_idx);
                table.attach_leading(entry_idx, token_key, &mut pending_leading);

                previous_entry = Some(entry_idx);
                previous_key = Some(token_key);
                after_newline = false;
            }

            current = token.next_token();
        }

        if let (Some(entry_idx), Some(owner_key)) = (previous_entry, previous_key) {
            if !pending_leading.is_empty() {
                pending_trailing.append(&mut pending_leading);
            }
            table.attach_trailing(entry_idx, owner_key, &mut pending_trailing);
        }

        table
    }

    fn attach_leading(
        &mut self,
        entry_idx: usize,
        owner: TokenKey,
        pending: &mut Vec<TriviaPiece>,
    ) {
        if pending.is_empty() {
            return;
        }
        let entry = &mut self.entries[entry_idx];
        for piece in pending.drain(..) {
            self.trivia_index
                .insert(piece.token_key(), TriviaPosition::Leading(owner));
            entry.leading.push(piece);
        }
    }

    fn attach_trailing(
        &mut self,
        entry_idx: usize,
        owner: TokenKey,
        pending: &mut Vec<TriviaPiece>,
    ) {
        if pending.is_empty() {
            return;
        }
        let entry = &mut self.entries[entry_idx];
        for piece in pending.drain(..) {
            self.trivia_index
                .insert(piece.token_key(), TriviaPosition::Trailing(owner));
            entry.trailing.push(piece);
        }
    }

    fn entry_for_token_key(&self, key: TokenKey) -> Option<&TokenTrivia> {
        self.token_index
            .get(&key)
            .and_then(|&index| self.entries.get(index))
    }

    /// Returns the leading trivia pieces for the provided token.
    pub fn leading_trivia(&self, token: &SyntaxToken<PerlLanguage>) -> &[TriviaPiece] {
        let key = TokenKey::from_token(token);
        self.entry_for_token_key(key)
            .map(|entry| entry.leading.as_slice())
            .unwrap_or_else(|| &[])
    }

    /// Returns the trailing trivia pieces for the provided token.
    pub fn trailing_trivia(&self, token: &SyntaxToken<PerlLanguage>) -> &[TriviaPiece] {
        let key = TokenKey::from_token(token);
        self.entry_for_token_key(key)
            .map(|entry| entry.trailing.as_slice())
            .unwrap_or_else(|| &[])
    }

    /// Returns the trivia position for the provided trivia token, if known.
    #[must_use]
    pub fn position_of(&self, trivia: &SyntaxToken<PerlLanguage>) -> Option<TriviaPosition> {
        if !trivia.kind().is_trivia() {
            return None;
        }
        let key = TokenKey::from_token(trivia);
        self.trivia_index.get(&key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_perl;

    #[test]
    fn inline_comment_attaches_to_trailing_trivia() {
        let source = "my $x = 1; # inline\n";
        let (root, _errors) = parse_perl(source);
        let table = TriviaTable::from_syntax(&root);

        let semicolon = root
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.text() == ";")
            .expect("expected semicolon token");
        let comment = root
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::COMMENT)
            .expect("expected comment token");

        let trailing = table.trailing_trivia(&semicolon);
        assert!(
            trailing
                .iter()
                .any(|piece| piece.token_key().text_range() == comment.text_range()),
            "expected trailing trivia to include the comment"
        );
        assert_eq!(
            table
                .position_of(&comment)
                .expect("comment should have owner"),
            TriviaPosition::Trailing(TokenKey::from_token(&semicolon))
        );
    }

    #[test]
    fn leading_comments_attach_to_following_token() {
        let source = "my $x = 1;\n# doc one\n# doc two\nsub foo { }\n";
        let (root, _errors) = parse_perl(source);
        let table = TriviaTable::from_syntax(&root);

        let sub_token = root
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.text() == "sub")
            .expect("expected sub token");
        let comments: Vec<_> = root
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::COMMENT)
            .collect();
        assert_eq!(comments.len(), 2);

        let leading = table.leading_trivia(&sub_token);
        let comment_ranges: Vec<_> = comments.iter().map(|c| c.text_range()).collect();
        let leading_ranges: Vec<_> = leading
            .iter()
            .filter(|piece| piece.kind() == SyntaxKind::COMMENT)
            .map(|piece| piece.token_key().text_range())
            .collect();
        assert_eq!(leading_ranges, comment_ranges);

        for comment in comments {
            let position = table
                .position_of(&comment)
                .expect("comment should have owner");
            assert!(position.is_leading());
            assert_eq!(position.token_key(), TokenKey::from_token(&sub_token));
        }
    }
}
