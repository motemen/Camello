//! The lexer (ADR 0005).
//!
//! Three properties distinguish this from the lexer it replaces:
//!
//! 1. **`expect` is a single piece of lexer state**, not an argument threaded
//!    through ~73 call sites. `peek` and `bump` take no context, so "peeked in
//!    one context, consumed in another" is not expressible.
//! 2. **Lookahead is a token buffer**, not a clone of the lexer. Changing
//!    `expect` invalidates the buffer from the cursor forward and re-scans;
//!    nothing is silently stale.
//! 3. **Constructs that switch scanning mode are atomic.** A quote-like
//!    operator, a heredoc body, POD and `__DATA__` are each scanned in a single
//!    call that pushes a whole run of tokens. No mode survives the call, so a
//!    lookahead can never observe a half-open quote and an unterminated one can
//!    never poison the rest of the file.

use rowan::{TextRange, TextSize};

use crate::lang::TokenKind;

mod atomic;
mod scan;
#[cfg(test)]
mod tests;

/// What the grammar expects next. Perl cannot be lexed without it: `/` is
/// division after a term and a match otherwise, `%` is a sigil before a name and
/// modulo after one.
///
/// This mirrors `PL_expect` in perl's own `toke.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Expect {
    /// A term (value, variable, prefix operator) may start here.
    #[default]
    Term,
    /// A term has just been read; an infix or postfix operator may follow.
    Operator,
}

/// One token, with the range it covers and the state it was scanned under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexedToken {
    pub kind: TokenKind,
    pub range: TextRange,
    /// The `expect` in force when this token was scanned. Used by the debug
    /// assertion in [`Lexer::bump`] that peek and consume agree (ADR 0005 §2).
    pub expect_at_lex: Expect,
}

impl LexedToken {
    #[must_use]
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.range]
    }
}

/// A position in the token stream, for speculative parsing (ADR 0007 §1).
///
/// Rolling back is just moving the cursor, so it is O(1) — the buffer keeps
/// everything already scanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    cursor: usize,
    expect: Expect,
}

/// A heredoc marker that has been scanned. Perl allows several on one line,
/// whose bodies follow in order from the next line start.
///
/// The record persists after the body is emitted so that the queue stays a pure
/// function of how far scanning has progressed — see
/// [`Lexer::invalidate_from_cursor`], which has to be able to *un*-consume a
/// body when lookahead is thrown away.
#[derive(Debug, Clone)]
struct Heredoc {
    terminator: String,
    /// `<<~EOF`: the terminator may be indented.
    indentable: bool,
    /// Byte offset of the `<<` that introduced it.
    marker_offset: usize,
    /// Where the body was emitted, once it has been.
    body_start: Option<usize>,
}

pub struct Lexer<'a> {
    source: &'a str,
    /// Every token, trivia included, in source order. Keeping trivia here is
    /// what lets the replayer re-attach it (ADR 0006 §4) without re-scanning,
    /// and what keeps the stream lossless.
    buffer: Vec<LexedToken>,
    /// Index into `buffer` of the next token not yet consumed by the parser.
    cursor: usize,
    /// Byte offset where scanning resumes when the buffer needs extending.
    scan_pos: usize,
    expect: Expect,
    heredocs: Vec<Heredoc>,
    /// Set once the scanner reaches end of input.
    exhausted: bool,
}

impl<'a> Lexer<'a> {
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            buffer: Vec::new(),
            cursor: 0,
            scan_pos: 0,
            expect: Expect::default(),
            heredocs: Vec::new(),
            exhausted: false,
        }
    }

    #[must_use]
    pub fn source(&self) -> &'a str {
        self.source
    }

    #[must_use]
    pub fn expect(&self) -> Expect {
        self.expect
    }

    /// Set what the grammar expects next.
    ///
    /// Any buffered lookahead scanned under the old expectation is dropped and
    /// re-scanned on demand, which is why a lookahead can never disagree with
    /// the eventual consume.
    pub fn set_expect(&mut self, expect: Expect) {
        if self.expect == expect {
            return;
        }
        self.expect = expect;
        self.invalidate_from_cursor();
    }

    fn invalidate_from_cursor(&mut self) {
        if self.buffer.len() <= self.cursor {
            return;
        }
        // Re-scan starts at the first token we are throwing away.
        let resume = usize::from(self.buffer[self.cursor].range.start());
        self.scan_pos = resume;
        self.buffer.truncate(self.cursor);
        self.exhausted = false;

        // Heredoc bookkeeping has to be rewound too, or a `print <<EOF;` whose
        // body was reached by lookahead would either be emitted twice or not at
        // all. Both the registration and the emission are keyed on byte offset,
        // so this restores exactly the state scanning had at `resume`.
        self.heredocs
            .retain(|heredoc| heredoc.marker_offset < resume);
        for heredoc in &mut self.heredocs {
            if heredoc.body_start.is_some_and(|start| start >= resume) {
                heredoc.body_start = None;
            }
        }
    }

    /// The `n`-th upcoming non-trivia token (`n == 0` is the current one).
    ///
    /// Returns `None` only at end of input; every other failure is a token
    /// (ADR 0005 §4).
    pub fn peek(&mut self, n: usize) -> Option<LexedToken> {
        let index = self.non_trivia_index(n)?;
        Some(self.buffer[index])
    }

    /// Kind of the `n`-th upcoming non-trivia token.
    pub fn peek_kind(&mut self, n: usize) -> Option<TokenKind> {
        self.peek(n).map(|token| token.kind)
    }

    /// Text of the `n`-th upcoming non-trivia token.
    pub fn peek_text(&mut self, n: usize) -> Option<&'a str> {
        self.peek(n).map(|token| token.text(self.source))
    }

    /// Is the current token this kind?
    pub fn at(&mut self, kind: TokenKind) -> bool {
        self.peek_kind(0) == Some(kind)
    }

    /// Consume the current non-trivia token.
    pub fn bump(&mut self) -> Option<LexedToken> {
        let index = self.non_trivia_index(0)?;
        let token = self.buffer[index];

        debug_assert_eq!(
            token.expect_at_lex, self.expect,
            "token {:?} at {:?} was lexed under {:?} but is being consumed under {:?}; \
             a set_expect call is missing or came too late",
            token.kind, token.range, token.expect_at_lex, self.expect
        );

        self.cursor = index + 1;
        Some(token)
    }

    /// Position for speculative parsing.
    #[must_use]
    pub fn mark(&self) -> Mark {
        Mark {
            cursor: self.cursor,
            expect: self.expect,
        }
    }

    /// Return to a previously taken [`Mark`].
    pub fn rollback(&mut self, mark: Mark) {
        self.cursor = mark.cursor;
        // Restoring expect must not invalidate: the buffer entries between the
        // mark and here were scanned under the expectations in force at the
        // time, and re-entering that prefix will re-derive the same states.
        // Only a *change* relative to what the buffer holds needs a re-scan, and
        // rolling back re-establishes exactly the state the buffer was built
        // under.
        self.expect = mark.expect;
    }

    /// The full token stream, trivia included, up to the given byte offset.
    ///
    /// Used by the event replayer, which needs every token in order to rebuild a
    /// lossless tree.
    pub fn scan_all(&mut self) -> &[LexedToken] {
        while !self.exhausted {
            self.scan_next();
        }
        &self.buffer
    }

    /// Read the token at the cursor as a plain identifier, whatever `expect`
    /// would otherwise have made of it.
    ///
    /// A name is never an operator and never a quote-like operator, but the two
    /// `expect` states cannot express "a name goes here": under `Term`,
    /// `sub tr {}` opens a substitution, and under `Operator`, `sub x100 {}`
    /// splits into the repetition operator and a number. The grammar knows which
    /// positions take a name, and this is how it says so — through one routine
    /// (ADR 0007 §5), not eight coercions.
    ///
    /// Returns `None` if there is no identifier here.
    pub fn take_name(&mut self) -> Option<LexedToken> {
        let index = self.non_trivia_index(0)?;
        let start = usize::from(self.buffer[index].range.start());
        let len = scan::ident_len_at(&self.source[start..]);
        if len == 0 {
            return None;
        }

        self.cursor = index;
        self.invalidate_from_cursor();
        self.push(TokenKind::IDENT, start, start + len);
        self.cursor = self.buffer.len();
        self.buffer.last().copied()
    }

    /// Read the token at the cursor as a bare sigil, without the name the
    /// scanner would otherwise attach to it.
    ///
    /// A signature placeholder is written `$,` or `@,` — a sigil holding a slot
    /// and then the separator. Scanning normally, `$,` is the output field
    /// separator variable, which is also real Perl; only the grammar knows which
    /// is meant here.
    pub fn take_sigil(&mut self) -> Option<LexedToken> {
        let index = self.non_trivia_index(0)?;
        let token = self.buffer[index];
        if !token.kind.is_sigil() {
            return None;
        }
        let start = usize::from(token.range.start());
        let len = self.source[start..].chars().next()?.len_utf8();

        self.cursor = index;
        self.invalidate_from_cursor();
        self.push(token.kind, start, start + len);
        self.cursor = self.buffer.len();
        self.buffer.last().copied()
    }

    /// The raw body of the `(...)` group at the cursor, without consuming it.
    pub fn peek_raw_paren_body(&mut self) -> Option<&'a str> {
        let open = self.peek(0)?;
        if open.kind != crate::lang::TokenKind::L_PAREN {
            return None;
        }
        let body_start = usize::from(open.range.end());
        let body_len = balanced_paren_body_len(&self.source[body_start..])?;
        Some(&self.source[body_start..body_start + body_len])
    }

    /// Consume a balanced `(...)` group with its body as one `RAW_CONTENT`
    /// token.
    ///
    /// Prototypes and attribute arguments are not Perl expressions —
    /// `sub f(_)`, `sub f(+)`, `sub f(\[$@])` are all legal, and re-lexing them
    /// as ordinary tokens is what made the old parser reject them (D6). The
    /// parser asks for raw text at the one point it knows the grammar calls for
    /// it, and gets a token rather than a poke at the underlying string
    /// (ADR 0004 §5).
    ///
    /// Returns `false` if the cursor is not on `(`, leaving the lexer untouched.
    pub fn take_raw_parens(&mut self) -> Option<(LexedToken, LexedToken, LexedToken)> {
        let open_index = self.non_trivia_index(0)?;
        let open = self.buffer[open_index];
        if open.kind != crate::lang::TokenKind::L_PAREN {
            return None;
        }

        let body_start = usize::from(open.range.end());
        let body_len = balanced_paren_body_len(&self.source[body_start..])?;

        // Anything scanned past the `(` was scanned as code; throw it away.
        self.cursor = open_index + 1;
        self.buffer.truncate(self.cursor);
        self.scan_pos = body_start;
        self.exhausted = false;

        if body_len > 0 {
            self.push(TokenKind::RAW_CONTENT, body_start, body_start + body_len);
        } else {
            self.buffer.push(LexedToken {
                kind: TokenKind::RAW_CONTENT,
                range: TextRange::empty(
                    TextSize::try_from(body_start).expect("source larger than 4GiB"),
                ),
                expect_at_lex: self.expect,
            });
        }
        let body = *self.buffer.last().expect("just pushed");

        let close_start = body_start + body_len;
        self.push(TokenKind::R_PAREN, close_start, close_start + 1);
        let close = *self.buffer.last().expect("just pushed");

        self.cursor = self.buffer.len();
        Some((open, body, close))
    }

    /// Index in `buffer` of the `n`-th non-trivia token at or after the cursor,
    /// extending the buffer as needed.
    fn non_trivia_index(&mut self, n: usize) -> Option<usize> {
        let mut index = self.cursor;
        let mut remaining = n;
        loop {
            while index >= self.buffer.len() {
                if self.exhausted {
                    return None;
                }
                self.scan_next();
            }
            if self.buffer[index].kind.is_parser_invisible() {
                index += 1;
                continue;
            }
            if remaining == 0 {
                return Some(index);
            }
            remaining -= 1;
            index += 1;
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        debug_assert!(end > start || kind == TokenKind::ERROR_CHAR);
        self.buffer.push(LexedToken {
            kind,
            range: TextRange::new(
                TextSize::try_from(start).expect("source larger than 4GiB"),
                TextSize::try_from(end).expect("source larger than 4GiB"),
            ),
            expect_at_lex: self.expect,
        });
        self.scan_pos = end;
    }
}

/// Length of the body of a `(...)` group whose `(` has already been consumed,
/// or `None` if it is never closed.
///
/// Quoted sections are skipped so that `sub f("(")` does not confuse the depth
/// count.
fn balanced_paren_body_len(rest: &str) -> Option<usize> {
    let mut depth = 1usize;
    let mut chars = rest.char_indices();
    while let Some((offset, ch)) = chars.next() {
        match ch {
            '\\' => {
                chars.next();
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}
