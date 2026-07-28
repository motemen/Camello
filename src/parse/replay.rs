//! Turning events into a tree, and attaching trivia while doing it
//! (ADR 0006 §4, ADR 0007 §1).

use rowan::{GreenNode, GreenNodeBuilder, TextSize};

use crate::lang::{NodeKind, SyntaxKind, TokenKind};
use crate::lex::LexedToken;

use super::event::{Diagnostic, Event};
use super::trivia::{Trivia, TriviaMap};

/// Builds the green tree. The only place in the crate that touches
/// `GreenNodeBuilder`, and it is typed on `NodeKind` / `TokenKind` so a node
/// kind cannot reach a token slot (ADR 0004 §1).
struct TreeBuilder<'a> {
    inner: GreenNodeBuilder<'static>,
    source: &'a str,
}

impl TreeBuilder<'_> {
    fn start_node(&mut self, kind: NodeKind) {
        self.inner.start_node(SyntaxKind::from(kind).into());
    }

    fn finish_node(&mut self) {
        self.inner.finish_node();
    }

    fn token(&mut self, kind: TokenKind, range: rowan::TextRange) {
        self.inner
            .token(SyntaxKind::from(kind).into(), &self.source[range]);
    }
}

pub struct Replayed {
    pub green: GreenNode,
    pub trivia: TriviaMap,
    pub diagnostics: Vec<Diagnostic>,
}

/// Replay `events` against `tokens`, producing the tree and the trivia map.
///
/// The placement rule is the whole point: trivia is flushed *before* a node is
/// opened and *after* a node is closed, never inside the boundary. That makes
/// every node's range start and end on real code, so the formatter's
/// "does this span more than one line" questions have exact answers
/// (ADR 0006 §4).
pub fn replay(source: &str, tokens: &[LexedToken], events: Vec<Event>) -> Replayed {
    let mut builder = TreeBuilder {
        inner: GreenNodeBuilder::new(),
        source,
    };
    let mut diagnostics = Vec::new();
    let mut trivia_map = TriviaMap::new();

    let mut cursor = TokenCursor::new(tokens);
    let mut events = events;
    // `forward_parent` chains are followed here rather than rewritten in place;
    // the chain is walked outward and re-emitted outermost-first.
    let mut pending_parents = Vec::new();

    builder.start_node(NodeKind::ROOT);

    for index in 0..events.len() {
        match std::mem::replace(&mut events[index], Event::Tombstone) {
            Event::Tombstone => {}
            Event::Error(diagnostic) => diagnostics.push(diagnostic),
            Event::Start {
                kind,
                forward_parent,
            } => {
                pending_parents.clear();
                pending_parents.push(kind);

                let mut slot = index;
                let mut next = forward_parent;
                while let Some(offset) = next {
                    slot += offset;
                    let Event::Start {
                        kind,
                        forward_parent,
                    } = std::mem::replace(&mut events[slot], Event::Tombstone)
                    else {
                        unreachable!("a forward parent must point at a Start event")
                    };
                    pending_parents.push(kind);
                    next = forward_parent;
                }

                // Trivia before the node's first token belongs outside it.
                cursor.flush_trivia(&mut builder, &mut trivia_map);
                for kind in pending_parents.drain(..).rev() {
                    builder.start_node(kind);
                }
            }
            // Closing happens before any following trivia is flushed, so no node
            // ends with trivia either.
            Event::Finish => builder.finish_node(),
            Event::Token => {
                cursor.flush_trivia(&mut builder, &mut trivia_map);
                cursor.emit_token(&mut builder);
            }
        }
    }

    // Whatever is left is end-of-file trivia; it belongs to the root.
    cursor.flush_trivia(&mut builder, &mut trivia_map);
    cursor.flush_remaining(&mut builder, &mut trivia_map);
    builder.finish_node();

    Replayed {
        green: builder.inner.finish(),
        trivia: trivia_map,
        diagnostics,
    }
}

/// Walks the full token stream in step with the events, keeping the pending
/// trivia run and the token it will be split around.
struct TokenCursor<'a> {
    tokens: &'a [LexedToken],
    index: usize,
    /// Start offset of the last non-trivia token emitted, for trivia ownership.
    previous_token_start: Option<TextSize>,
    run: Vec<Trivia>,
}

impl<'a> TokenCursor<'a> {
    fn new(tokens: &'a [LexedToken]) -> Self {
        Self {
            tokens,
            index: 0,
            previous_token_start: None,
            run: Vec::new(),
        }
    }

    /// Emit every trivia token up to the next non-trivia one, recording its
    /// ownership.
    fn flush_trivia(&mut self, builder: &mut TreeBuilder<'_>, trivia: &mut TriviaMap) {
        let start = self.index;
        while self
            .tokens
            .get(self.index)
            .is_some_and(|token| token.kind.is_trivia())
        {
            self.index += 1;
        }
        if start == self.index {
            return;
        }

        self.run.clear();
        for token in &self.tokens[start..self.index] {
            builder.token(token.kind, token.range);
            self.run.push(Trivia {
                kind: token.kind,
                range: token.range,
                text: builder.source[token.range].into(),
            });
        }

        let next_start = self.tokens.get(self.index).map(|token| token.range.start());
        trivia.attach_run(&self.run, self.previous_token_start, next_start);
    }

    fn emit_token(&mut self, builder: &mut TreeBuilder<'_>) {
        let Some(token) = self.tokens.get(self.index) else {
            return;
        };
        builder.token(token.kind, token.range);
        self.previous_token_start = Some(token.range.start());
        self.index += 1;
    }

    /// Emit anything the events did not account for.
    ///
    /// A well-formed parse leaves nothing here; error recovery can, and dropping
    /// it would break losslessness.
    fn flush_remaining(&mut self, builder: &mut TreeBuilder<'_>, trivia: &mut TriviaMap) {
        while self.index < self.tokens.len() {
            if self.tokens[self.index].kind.is_trivia() {
                self.flush_trivia(builder, trivia);
            } else {
                self.emit_token(builder);
            }
        }
    }
}

impl TriviaMap {
    fn new() -> Self {
        Self::default()
    }
}
