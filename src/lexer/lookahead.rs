//! Lookahead and snapshot logic for the lexer.

use super::{types::LexerMode, Lexer, Token};
use crate::SyntaxKind;
use std::collections::VecDeque;

use super::types::{HeredocMarker, LexContext};

#[derive(Debug, Clone)]
pub(super) struct LexerSnapshot<'a> {
    pub(super) logos_lexer: logos::Lexer<'a, Token>,
    pub(super) at_line_start: bool,
    pub(super) mode: LexerMode,
    pub(super) pending: VecDeque<(SyntaxKind, &'a str)>,
    pub(super) heredoc_queue: VecDeque<HeredocMarker<'a>>,
}

impl<'a> From<&Lexer<'a>> for LexerSnapshot<'a> {
    fn from(lexer: &Lexer<'a>) -> Self {
        Self {
            logos_lexer: lexer.logos_lexer.clone(),
            at_line_start: lexer.at_line_start,
            mode: lexer.mode,
            pending: lexer.pending.clone(),
            heredoc_queue: lexer.heredoc_queue.clone(),
        }
    }
}

impl<'a> LexerSnapshot<'a> {
    pub(super) fn into_lexer(self) -> Lexer<'a> {
        use std::cell::RefCell;
        Lexer {
            logos_lexer: self.logos_lexer,
            at_line_start: self.at_line_start,
            mode: self.mode,
            pending: self.pending,
            heredoc_queue: self.heredoc_queue,
            lookahead: RefCell::new(VecDeque::new()),
        }
    }

    pub(super) fn next_char(&self) -> Option<char> {
        self.logos_lexer.remainder().chars().next()
    }
}

#[derive(Debug, Clone)]
pub(super) struct CachedEntry<'a> {
    pub(super) context: LexContext,
    pub(super) token: (SyntaxKind, &'a str),
    pub(super) state: LexerSnapshot<'a>,
}

impl<'a> Lexer<'a> {
    pub(super) fn clear_lookahead(&self) {
        self.lookahead.borrow_mut().clear();
    }

    pub(super) fn apply_snapshot(&mut self, snapshot: LexerSnapshot<'a>) {
        self.logos_lexer = snapshot.logos_lexer;
        self.at_line_start = snapshot.at_line_start;
        self.mode = snapshot.mode;
        self.pending = snapshot.pending;
        self.heredoc_queue = snapshot.heredoc_queue;
    }

    pub(super) fn consume_cached(&mut self, context: LexContext) -> Option<(SyntaxKind, &'a str)> {
        let mut cache = self.lookahead.borrow_mut();
        if cache.front().is_some_and(|entry| entry.context != context) {
            cache.clear();
            return None;
        }
        let entry = cache.pop_front()?;
        drop(cache);
        let token = entry.token;
        self.apply_snapshot(entry.state);
        Some(token)
    }

    pub(super) fn ensure_cached(&self, context: LexContext, count: usize) -> bool {
        let mut cache = self.lookahead.borrow_mut();
        if cache.front().is_some_and(|entry| entry.context != context) {
            cache.clear();
        }

        while cache.len() < count {
            let base_snapshot = if let Some(last) = cache.back() {
                last.state.clone()
            } else {
                LexerSnapshot::from(self)
            };

            let mut cursor = base_snapshot.clone().into_lexer();
            let Some(token) = cursor.next_token_internal(Some(context)) else {
                return false;
            };
            let state = LexerSnapshot::from(&cursor);
            cache.push_back(CachedEntry {
                context,
                token,
                state,
            });
        }

        true
    }

    /// Peek at the next token without consuming it or changing lexer state
    #[must_use]
    pub fn peek_token(&self) -> Option<(SyntaxKind, &'a str)> {
        if !self.ensure_cached(LexContext::Value, 1) {
            return None;
        }
        self.lookahead.borrow().front().map(|entry| entry.token)
    }

    /// Peek at the next non-trivia token without consuming it or changing lexer state
    #[must_use]
    pub fn peek_non_trivia_token(&self) -> Option<(SyntaxKind, &'a str)> {
        self.peek_non_trivia_with_context(Default::default())
    }

    /// Peek ahead multiple tokens, skipping trivia, and return the first non-trivia token
    /// that matches any of the given kinds
    #[must_use]
    pub fn peek_for_any(&self, target_kinds: &[SyntaxKind]) -> Option<(SyntaxKind, &'a str)> {
        let mut index = 0;
        loop {
            if !self.ensure_cached(LexContext::Value, index + 1) {
                return None;
            }
            let token = {
                let cache = self.lookahead.borrow();
                match cache.get(index) {
                    Some(entry) => entry.token,
                    None => return None,
                }
            };
            if !token.0.is_trivia() && target_kinds.contains(&token.0) {
                return Some(token);
            }
            index += 1;
        }
    }

    /// Peek the nth non-trivia token using a given lexical context.
    /// This does not mutate the original lexer state.
    #[must_use]
    pub fn peek_nth_non_trivia_with_context(
        &self,
        context: LexContext,
        n: usize,
    ) -> Option<(SyntaxKind, &'a str)> {
        let mut index = 0;
        let mut seen = 0;
        loop {
            if !self.ensure_cached(context, index + 1) {
                return None;
            }
            let token = {
                let cache = self.lookahead.borrow();
                match cache.get(index) {
                    Some(entry) => entry.token,
                    None => return None,
                }
            };
            if token.0.is_trivia() {
                index += 1;
                continue;
            }
            if seen == n {
                return Some(token);
            }
            seen += 1;
            index += 1;
        }
    }

    /// Peek the current token and the character immediately following it.
    /// This is useful for disambiguating cases like quote-like keywords followed by delimiters.
    /// Returns (current_token_kind, next_char) where next_char is the character immediately
    /// after the current token, or None if at end of input.
    #[must_use]
    pub fn peek_token_and_next_char(&self) -> (Option<SyntaxKind>, Option<char>) {
        let current_kind = self.peek_token().map(|(kind, _)| kind);
        let next_char = self
            .lookahead
            .borrow()
            .front()
            .and_then(|entry| entry.state.next_char());
        (current_kind, next_char)
    }

    /// Peek the next non-trivia token using a given lexical context.
    /// This does not mutate the original lexer state.
    #[must_use]
    pub fn peek_non_trivia_with_context(
        &self,
        context: LexContext,
    ) -> Option<(SyntaxKind, &'a str)> {
        self.peek_nth_non_trivia_with_context(context, 0)
    }
}
