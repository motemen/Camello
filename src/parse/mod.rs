//! The parser (ADR 0007).
//!
//! Records [`Event`]s rather than building the tree directly, which is what
//! makes speculative parsing possible: try one reading, and if it does not work
//! out, [`Parser::rollback`] and try another. The old parser could not do this —
//! `GreenNodeBuilder` has no way to abandon a node — so every ambiguity had to
//! be settled by unbounded lookahead before a node was opened, and 21 separate
//! heuristics grew out of that constraint.

use rowan::{TextRange, TextSize};

use crate::lang::{NodeKind, SyntaxKind, TokenKind};
use crate::lex::{Expect, Lexer, Mark};

pub mod event;
mod grammar;
mod replay;
pub mod trivia;

use event::{CompletedMarker, Diagnostic, Events, Marker};
pub use trivia::TriviaMap;

/// Result of a parse: a lossless tree, the trivia attached to each token, and
/// whatever went wrong.
pub struct Parse {
    pub green: rowan::GreenNode,
    pub trivia: TriviaMap,
    pub diagnostics: Vec<Diagnostic>,
}

impl Parse {
    #[must_use]
    pub fn syntax(&self) -> crate::lang::SyntaxNode {
        crate::lang::SyntaxNode::new_root(self.green.clone())
    }
}

/// Parse Perl source into a CST in the normal form of ADR 0007 §2.
#[must_use]
pub fn parse(source: &str) -> Parse {
    let mut parser = Parser::new(source);
    grammar::root(&mut parser);
    parser.drain_into_error();
    parser.finish()
}

/// A saved parser position, covering both the event stream and the lexer.
#[derive(Debug, Clone, Copy)]
pub struct Checkpoint {
    events: usize,
    lexer: Mark,
    diagnostics: usize,
    /// Where the next token starts. Progress has to be measured in *input*
    /// consumed, not events recorded: a rule that opens and closes a node
    /// without consuming anything has recorded events and still made no
    /// progress, which is exactly how a non-advancing loop hides.
    offset: Option<TextSize>,
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    events: Events,
    /// Diagnostics are held separately from the event stream so that a rollback
    /// can discard the ones the abandoned attempt produced.
    diagnostics: Vec<Diagnostic>,
    /// Inspections since the last token was consumed. A rule that loops without
    /// consuming input trips this instead of hanging — the old parser hit that
    /// class of bug repeatedly (notes/2025-01-17-infinite-loop-analysis.md).
    steps_without_progress: u32,
    /// Markers open right now, which is how deeply the tree nests here.
    depth: u32,
    /// Set once a limit has been reached. From then on the parser reports end of
    /// input, every rule unwinds, and [`Self::drain_into_error`] puts what is
    /// left into one ERROR node.
    ///
    /// A limit is not a bug in the input. `((((...` a thousand deep is a
    /// perfectly ordinary thing for a fuzzer or a generated file to contain, and
    /// a formatter's answer to it is a diagnostic, not an abort.
    stopped: bool,
}

/// Far more inspections than any single position legitimately needs.
const STEP_LIMIT: u32 = 10_000;

/// Far deeper than any hand-written Perl nests, and far shallower than the
/// depth at which the formatter's recursive walk runs out of stack (~3500).
///
/// The cap is here rather than in the formatter because it is here that it can
/// be reported: the parser produces diagnostics and the tree it produces is what
/// the formatter walks, so bounding the tree bounds everything downstream.
const DEPTH_LIMIT: u32 = 512;

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            lexer: Lexer::new(source),
            events: Events::default(),
            diagnostics: Vec::new(),
            steps_without_progress: 0,
            depth: 0,
            stopped: false,
        }
    }

    /// Stop parsing, with a diagnostic saying why.
    ///
    /// Reporting end of input is what unwinds the rules: every loop in the
    /// grammar is guarded on it, and every open marker is completed on the way
    /// out, so the event stream stays well-formed. What has not been consumed is
    /// picked up by [`Self::drain_into_error`].
    fn stop(&mut self, message: &str) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        let range = self.current_range();
        self.push_diagnostic(message.to_string(), range);
    }

    /// Consume whatever is left into one ERROR node.
    ///
    /// Only ever does anything after [`Self::stop`]. Without it the remaining
    /// tokens would reach the tree through the replayer's end-of-file flush, as
    /// bare children of ROOT that no formatter rule covers — and the file's tail
    /// would be dropped from the output.
    fn drain_into_error(&mut self) {
        if !self.stopped {
            return;
        }
        self.stopped = false;
        if self.lexer.peek(0).is_none() {
            return;
        }
        let marker = self.start();
        while self.lexer.peek(0).is_some() {
            self.lexer.bump();
            self.events.token();
        }
        self.complete(marker, NodeKind::ERROR);
    }

    fn finish(mut self) -> Parse {
        let events = std::mem::take(&mut self.events).into_vec();
        let source = self.lexer.source();
        let tokens = self.lexer.scan_all().to_vec();
        let replayed = replay::replay(source, &tokens, events);

        let mut diagnostics = replayed.diagnostics;
        diagnostics.extend(self.diagnostics);
        diagnostics.sort_by_key(|diagnostic| diagnostic.range.start());

        Parse {
            green: replayed.green,
            trivia: replayed.trivia,
            diagnostics,
        }
    }

    // ===== Lexer-facing =====

    /// Tell the lexer what the grammar expects next (ADR 0005 §2).
    pub(crate) fn expect_term(&mut self) {
        self.lexer.set_expect(Expect::Term);
    }

    /// Whether the lexer is currently expecting an operator.
    pub(crate) fn expect_is_operator(&self) -> bool {
        self.lexer.expect() == Expect::Operator
    }

    pub(crate) fn expect_operator(&mut self) {
        self.lexer.set_expect(Expect::Operator);
    }

    pub(crate) fn current(&mut self) -> Option<TokenKind> {
        self.nth(0)
    }

    pub(crate) fn nth(&mut self, n: usize) -> Option<TokenKind> {
        if self.stopped {
            return None;
        }
        self.step();
        if self.stopped {
            return None;
        }
        self.lexer.peek_kind(n)
    }

    fn step(&mut self) {
        self.steps_without_progress += 1;
        if self.steps_without_progress >= STEP_LIMIT {
            self.steps_without_progress = 0;
            self.stop("parser stopped making progress here");
        }
    }

    pub(crate) fn at(&mut self, kind: TokenKind) -> bool {
        self.current() == Some(kind)
    }

    pub(crate) fn at_any(&mut self, kinds: &[TokenKind]) -> bool {
        self.current().is_some_and(|kind| kinds.contains(&kind))
    }

    pub(crate) fn nth_at(&mut self, n: usize, kind: TokenKind) -> bool {
        self.nth(n) == Some(kind)
    }

    pub(crate) fn current_text(&mut self) -> Option<&'a str> {
        self.lexer.peek_text(0)
    }

    pub(crate) fn current_range(&mut self) -> TextRange {
        self.lexer
            .peek(0)
            .map(|token| token.range)
            .unwrap_or_else(|| self.eof_range())
    }

    fn eof_range(&self) -> TextRange {
        let end = TextSize::try_from(self.lexer.source().len()).expect("source larger than 4GiB");
        TextRange::empty(end)
    }

    pub(crate) fn at_end(&mut self) -> bool {
        self.current().is_none()
    }

    // ===== Node construction =====

    pub(crate) fn start(&mut self) -> Marker {
        self.depth += 1;
        if self.depth > DEPTH_LIMIT {
            self.stop("expression nests too deeply to format");
        }
        self.events.start()
    }

    pub(crate) fn complete(&mut self, marker: Marker, kind: NodeKind) -> CompletedMarker {
        self.depth = self.depth.saturating_sub(1);
        self.events.complete(marker, kind)
    }

    pub(crate) fn abandon(&mut self, marker: Marker) {
        self.depth = self.depth.saturating_sub(1);
        self.events.abandon(marker);
    }

    pub(crate) fn precede(&mut self, completed: CompletedMarker) -> Marker {
        self.events.precede(completed)
    }

    /// Consume the current token into the tree.
    pub(crate) fn bump(&mut self) {
        self.steps_without_progress = 0;
        if self.lexer.bump().is_some() {
            self.events.token();
        }
    }

    /// Consume the current token, whatever it is, as `kind`.
    ///
    /// Used where the grammar overrides the lexer's classification — a keyword
    /// standing in for a name, for instance. All such coercion goes through
    /// [`grammar::name`] so there is one place to look, not eight
    /// (ADR 0007 §5).
    pub(crate) fn bump_any(&mut self) {
        self.bump();
    }

    /// Consume `kind` if present; otherwise report and consume nothing.
    pub(crate) fn expect(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            return true;
        }
        let found = self.found_suffix();
        self.error(format!("expected {kind}{found}"));
        false
    }

    fn found_suffix(&mut self) -> String {
        match self.current() {
            Some(kind) => format!(", found {kind}"),
            None => ", found end of file".to_string(),
        }
    }

    // ===== Errors and recovery (ADR 0007 §3) =====

    /// Report without consuming. This is the default, deliberately: the old
    /// `error()` ate a token, and two mistakes turned into six diagnostics.
    pub(crate) fn error(&mut self, message: impl Into<String>) {
        let range = self.current_range();
        self.push_diagnostic(message.into(), range);
    }

    fn push_diagnostic(&mut self, message: String, range: TextRange) {
        // One diagnostic per position: a recovery loop that reports twice at the
        // same token is reporting the same mistake twice.
        if self
            .diagnostics
            .last()
            .is_some_and(|last| last.range == range)
        {
            return;
        }
        self.diagnostics.push(Diagnostic { message, range });
    }

    /// Report and consume the offending token inside an `ERROR` node.
    pub(crate) fn error_and_bump(&mut self, message: impl Into<String>) {
        self.error(message);
        if self.at_end() {
            return;
        }
        let marker = self.start();
        self.bump();
        self.complete(marker, NodeKind::ERROR);
    }

    /// Report, then skip to the next token in `recovery`, wrapping everything
    /// skipped in a single `ERROR` node.
    ///
    /// Panic-mode recovery is what keeps a cascade from forming: one mistake
    /// produces one diagnostic and one `ERROR` node, and parsing resumes at a
    /// point the grammar can actually use.
    pub(crate) fn error_recover(&mut self, message: impl Into<String>, recovery: Recovery) {
        self.error(message);
        self.recover(recovery);
    }

    /// Skip to the next token in `recovery` without reporting again.
    pub(crate) fn recover(&mut self, recovery: Recovery) {
        if self.at_end() || recovery.accepts(self) {
            return;
        }
        let marker = self.start();
        while !self.at_end() && !recovery.accepts(self) {
            self.expect_term();
            self.bump();
        }
        self.complete(marker, NodeKind::ERROR);
    }

    // ===== Speculation (ADR 0007 §1) =====

    pub(crate) fn checkpoint(&mut self) -> Checkpoint {
        Checkpoint {
            events: self.events.len(),
            lexer: self.lexer.mark(),
            diagnostics: self.diagnostics.len(),
            offset: self.offset(),
        }
    }

    fn offset(&mut self) -> Option<TextSize> {
        self.lexer.peek(0).map(|token| token.range.start())
    }

    /// Undo everything since `checkpoint`: events, lexer position and the
    /// diagnostics the abandoned attempt produced.
    pub(crate) fn rollback(&mut self, checkpoint: Checkpoint) {
        self.events.truncate(checkpoint.events);
        self.lexer.rollback(checkpoint.lexer);
        self.diagnostics.truncate(checkpoint.diagnostics);
    }

    /// Number of diagnostics recorded so far, for a speculative parse to judge
    /// whether its attempt went well.
    pub(crate) fn diagnostic_count(&self) -> usize {
        self.diagnostics.len()
    }

    /// True if nothing has been consumed since `checkpoint`.
    ///
    /// Rules use this to notice they made no progress, so a malformed input
    /// produces a diagnostic instead of a hang.
    pub(crate) fn checkpoint_is_unmoved(&mut self, checkpoint: Checkpoint) -> bool {
        self.offset() == checkpoint.offset
    }

    /// Report at a range other than the current token.
    pub(crate) fn error_at(&mut self, message: impl Into<String>, range: TextRange) {
        self.push_diagnostic(message.into(), range);
    }

    /// Consume the current token as a name (ADR 0007 §5).
    pub(crate) fn bump_name(&mut self) -> bool {
        if self.lexer.take_name().is_none() {
            return false;
        }
        self.steps_without_progress = 0;
        self.events.token();
        true
    }

    /// The text a punctuation variable's name would be, if the current token is
    /// a sigil whose name is a single punctuation character.
    pub(crate) fn raw_after_sigil(&mut self) -> Option<&'a str> {
        if !self.current().is_some_and(TokenKind::is_sigil) {
            return None;
        }
        (self.nth(1) == Some(TokenKind::RAW_CONTENT))
            .then(|| self.lexer.peek_text(1))
            .flatten()
    }

    /// Consume `^NAME` as one token (`${^MATCH}`).
    pub(crate) fn bump_caret_name(&mut self) -> bool {
        if self.lexer.take_caret_name().is_none() {
            return false;
        }
        self.steps_without_progress = 0;
        self.events.token();
        true
    }

    /// Consume the current token as a bare sigil (signature placeholders).
    pub(crate) fn bump_sigil(&mut self) -> bool {
        if self.lexer.take_sigil().is_none() {
            return false;
        }
        self.steps_without_progress = 0;
        self.events.token();
        true
    }

    /// The raw body of the `(...)` group at the cursor, without consuming it.
    pub(crate) fn raw_paren_body(&mut self) -> Option<&'a str> {
        self.lexer.peek_raw_paren_body()
    }

    /// Consume a `(...)` group as raw text (prototypes, attribute arguments).
    pub(crate) fn bump_raw_parens(&mut self) -> bool {
        if self.lexer.take_raw_parens().is_none() {
            return false;
        }
        // Input was consumed, so the no-progress counter starts again. Every
        // other `bump_*` says so; this one did not, and a file with enough
        // prototypes in it could reach the step limit on legitimate progress.
        self.steps_without_progress = 0;
        self.events.token();
        self.events.token();
        self.events.token();
        true
    }
}

/// A synchronisation set for panic-mode recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Recovery {
    /// `;`, `}` or a keyword that can start a statement.
    Statement,
    /// The statement set plus `,` `)` `]`, for use inside a list.
    List,
}

impl Recovery {
    fn accepts(self, parser: &mut Parser<'_>) -> bool {
        let Some(kind) = parser.current() else {
            return true;
        };
        let statement = matches!(kind, T![";"] | T!["}"]) || kind.starts_statement();
        match self {
            Recovery::Statement => statement,
            Recovery::List => statement || matches!(kind, T![","] | T![")"] | T!["]"]),
        }
    }
}

use crate::lang::T;

/// The rowan kind of a node, for callers that only have a `NodeKind`.
#[must_use]
pub fn node_kind(kind: NodeKind) -> SyntaxKind {
    SyntaxKind::from(kind)
}

#[cfg(test)]
mod tests;
