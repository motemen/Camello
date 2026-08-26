//! The event buffer (the parser contract).
//!
//! The parser records what it decided instead of building the tree as it goes.
//! That indirection is what buys speculative parsing: a `Marker` can be
//! abandoned, and the events it produced vanish. `GreenNodeBuilder` has no
//! `abandon_node`, which is precisely why the old parser had to resolve every
//! ambiguity by unbounded lookahead before it dared open a node.

use rowan::TextRange;

use crate::lang::NodeKind;

/// A parse diagnostic. Messages are written for humans — no internal enum names
/// (the parser contract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Open a node. `forward_parent` chains to a node that was decided *later*
    /// but must enclose this one — how left-associative infix expressions get
    /// their left operand wrapped after the fact.
    Start {
        kind: NodeKind,
        forward_parent: Option<usize>,
    },
    /// Consume the next non-trivia token.
    Token,
    /// Close the innermost open node.
    Finish,
    /// A diagnostic that does not change the tree.
    Error(Diagnostic),
    /// An abandoned `Start`, skipped at replay.
    Tombstone,
}

/// A position in the event stream that can later become a node, be abandoned,
/// or be wrapped by an enclosing node.
#[derive(Debug)]
#[must_use = "a marker must be completed or abandoned"]
pub struct Marker {
    index: usize,
    /// Guards against a marker being dropped without a decision, which would
    /// silently leave a `Tombstone` where a node was meant to be.
    resolved: bool,
}

impl Drop for Marker {
    fn drop(&mut self) {
        debug_assert!(
            self.resolved || std::thread::panicking(),
            "marker dropped without complete() or abandon()"
        );
    }
}

/// A completed node, kept so that it can still be wrapped by an outer one.
#[derive(Debug, Clone, Copy)]
pub struct CompletedMarker {
    start: usize,
    kind: NodeKind,
}

impl CompletedMarker {
    #[must_use]
    pub fn kind(self) -> NodeKind {
        self.kind
    }
}

#[derive(Debug, Default)]
pub struct Events {
    events: Vec<Event>,
}

impl Events {
    pub fn start(&mut self) -> Marker {
        let index = self.events.len();
        self.events.push(Event::Tombstone);
        Marker {
            index,
            resolved: false,
        }
    }

    pub fn token(&mut self) {
        self.events.push(Event::Token);
    }

    pub fn error(&mut self, diagnostic: Diagnostic) {
        self.events.push(Event::Error(diagnostic));
    }

    pub fn complete(&mut self, mut marker: Marker, kind: NodeKind) -> CompletedMarker {
        marker.resolved = true;
        self.events[marker.index] = Event::Start {
            kind,
            forward_parent: None,
        };
        self.events.push(Event::Finish);
        CompletedMarker {
            start: marker.index,
            kind,
        }
    }

    pub fn abandon(&mut self, mut marker: Marker) {
        marker.resolved = true;
        // A tombstone at the very end leaves no trace at all; one in the middle
        // is skipped at replay.
        if marker.index + 1 == self.events.len() {
            self.events.pop();
        }
    }

    /// Wrap an already-completed node in a new one.
    ///
    /// `$a + $b * $c` completes `$b * $c` before it knows the whole thing is an
    /// addition; `precede` supplies the outer node without re-parsing.
    pub fn precede(&mut self, completed: CompletedMarker) -> Marker {
        let marker = self.start();
        if let Event::Start { forward_parent, .. } = &mut self.events[completed.start] {
            *forward_parent = Some(marker.index - completed.start);
        }
        marker
    }

    /// Discard everything recorded after `len`. Used to undo a speculative
    /// parse (the parser contract).
    pub fn truncate(&mut self, len: usize) {
        self.events.truncate(len);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// The recorded events, for the replayer to consume.
    ///
    /// `forward_parent` links are still unresolved; the replayer follows them
    /// as it walks, which is cheaper than rewriting the vector.
    #[must_use]
    pub fn into_vec(self) -> Vec<Event> {
        self.events
    }
}
