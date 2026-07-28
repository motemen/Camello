//! The trivia model (ADR 0006).

use std::collections::HashMap;

use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

use crate::lang::TokenKind;

/// One piece of trivia, with its text.
///
/// Carrying the text matters for comments: the placement rule of ADR 0006 §4
/// puts a leading comment *outside* the node it belongs to, so a consumer
/// holding only the node cannot find the token again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TokenKind,
    pub range: TextRange,
    pub text: SmolStr,
}

/// The trivia attached to one non-trivia token.
#[derive(Debug, Default, Clone)]
pub struct TokenTrivia {
    /// Trivia on the same line as the *previous* token, up to and including the
    /// newline that ends it.
    pub trailing: Vec<Trivia>,
    /// Everything after that, up to this token: own-line comments and blank
    /// lines.
    pub leading: Vec<Trivia>,
}

impl TokenTrivia {
    /// Blank lines lying immediately before this token, after any comment.
    ///
    /// The formatter's blank-line policy (formatting.md BLANK_LINE) reads this
    /// rather than re-scanning the source, which is what removes the old
    /// double-check between writer state and the source text.
    #[must_use]
    pub fn blank_lines_before(&self) -> usize {
        let mut blanks = 0;
        for trivia in self.leading.iter().rev() {
            match trivia.kind {
                TokenKind::NEWLINE => blanks += 1,
                TokenKind::WHITESPACE => {}
                // A comment ends the run: the blank lines above it belong to the
                // comment, not to the token.
                _ => break,
            }
        }
        blanks
    }

    #[must_use]
    pub fn has_comment(&self) -> bool {
        self.leading
            .iter()
            .chain(&self.trailing)
            .any(|trivia| trivia.kind == TokenKind::COMMENT)
    }
}

/// Every non-trivia token's trivia, keyed by where the token starts.
///
/// Built during replay (ADR 0006 §5), so the formatter never walks the tree to
/// rediscover comments — the old `TriviaTable` rebuilt itself from a full tree
/// traversal, which is the cost behind issue #266.
#[derive(Debug, Default)]
pub struct TriviaMap {
    by_token_start: HashMap<TextSize, TokenTrivia>,
    empty: TokenTrivia,
}

impl TriviaMap {
    /// Trivia for the token starting at `offset`.
    ///
    /// Prefer [`Self::of`] where the token's range is in hand: a start offset
    /// identifies at most one token *of non-zero width*, and this cannot tell
    /// the difference.
    #[must_use]
    pub fn at(&self, offset: TextSize) -> &TokenTrivia {
        self.by_token_start.get(&offset).unwrap_or(&self.empty)
    }

    /// Trivia for the token occupying `range`.
    ///
    /// A zero-width token starts exactly where the next token starts — the empty
    /// replacement list of `s/a//` and the delimiter after it both begin at the
    /// same offset — so keying on the start alone hands one run of trivia to two
    /// owners and the formatter emits it twice. A token with no text can own no
    /// trivia, which settles the ambiguity without needing a second key.
    ///
    /// The visible form of getting this wrong was `$x =~ s/a// # c` formatting
    /// to `$x =~ s/a/ # c/ # c`: a comment moved inside the replacement string
    /// and "delete a" became "replace a with ' # c'".
    #[must_use]
    pub fn of(&self, range: TextRange) -> &TokenTrivia {
        if range.is_empty() {
            return &self.empty;
        }
        self.at(range.start())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_token_start.is_empty()
    }

    /// Attach a run of trivia lying between two non-trivia tokens.
    ///
    /// The split is at the first NEWLINE, inclusive: what shares a line with the
    /// preceding token belongs to it, and everything else belongs to the token
    /// that follows (ADR 0006 §3).
    pub(crate) fn attach_run(
        &mut self,
        run: &[Trivia],
        previous_start: Option<TextSize>,
        next_start: Option<TextSize>,
    ) {
        if run.is_empty() {
            return;
        }

        // With no preceding token there is no line to share, so the whole run
        // belongs to what follows — otherwise a comment on the first line of a
        // file would be attributed to nothing and disappear.
        let split = if previous_start.is_none() {
            0
        } else {
            run.iter()
                .position(|trivia| trivia.kind == TokenKind::NEWLINE)
                .map_or(run.len(), |index| index + 1)
        };

        let (trailing, leading) = run.split_at(split);

        if let Some(offset) = previous_start {
            if !trailing.is_empty() {
                self.by_token_start
                    .entry(offset)
                    .or_default()
                    .trailing
                    .extend_from_slice(trailing);
            }
        }

        // Trivia at end of file has no following token; per ADR 0006 §3 it would
        // belong to EOF, which is not a token here, so it stays only in the tree.
        if let Some(offset) = next_start {
            if !leading.is_empty() {
                self.by_token_start
                    .entry(offset)
                    .or_default()
                    .leading
                    .extend_from_slice(leading);
            }
        }
    }
}
