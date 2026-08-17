//! Constructs that switch scanning mode, scanned as indivisible runs of tokens
//! (ADR 0005 §3).
//!
//! Each function here enters at the start of a construct and returns only once
//! the whole construct has been pushed. No caller can observe a half-open quote
//! or a heredoc waiting for its body, which is what makes the two structural
//! bugs D2 (a mode left switched on after a failure poisons the rest of the
//! file) and D3 (lookahead cannot see a mode the parser switched on) impossible
//! rather than merely fixed.

use crate::lang::{TokenKind, T};

use super::{Heredoc, Lexer};

/// Flags accepted after `m`, `qr` and `s`.
const MATCH_FLAGS: &[u8] = b"msixpodualngcer";
/// Flags accepted after `tr` and `y`.
const TRANSLITERATION_FLAGS: &[u8] = b"cdsr";

/// The closing delimiter for an opening one, and whether it nests.
///
/// This table used to exist twice — once in the lexer and once in the parser's
/// mirror of the quote-like state machine. There is now one quote-like state
/// machine, so there is one table.
fn closing_delimiter(open: char) -> (char, bool) {
    match open {
        '(' => (')', true),
        '[' => (']', true),
        '{' => ('}', true),
        '<' => ('>', true),
        other => (other, false),
    }
}

impl<'a> Lexer<'a> {
    fn remaining(&self) -> &'a str {
        &self.source[self.scan_pos..]
    }

    /// Push trivia until a delimiter character is reached, and return it.
    ///
    /// A `#` immediately after the keyword is a delimiter; a `#` after any
    /// whitespace is a comment. That is perl's rule, and it is why `q #hello#`
    /// is `q` applied to nothing followed by a comment rather than a string
    /// delimited by `#` (D5).
    fn skip_to_delimiter(&mut self) -> Option<char> {
        let mut skipped_any = false;
        loop {
            let rest = self.remaining();
            let mut chars = rest.chars();
            let first = chars.next()?;

            match first {
                ' ' | '\t' => {
                    let len = rest
                        .find(|ch| !matches!(ch, ' ' | '\t'))
                        .unwrap_or(rest.len());
                    let start = self.scan_pos;
                    self.push(TokenKind::WHITESPACE, start, start + len);
                    skipped_any = true;
                }
                '\n' | '\r' => {
                    let len = if rest.starts_with("\r\n") { 2 } else { 1 };
                    let start = self.scan_pos;
                    self.push(TokenKind::NEWLINE, start, start + len);
                    skipped_any = true;
                }
                '#' if skipped_any => {
                    let len = rest.find(['\n', '\r']).unwrap_or(rest.len());
                    let start = self.scan_pos;
                    self.push(TokenKind::COMMENT, start, start + len);
                }
                other => return Some(other),
            }
        }
    }

    /// Scan a quote-like operator whole: keyword, delimiters, contents, flags.
    pub(super) fn scan_quote_like(&mut self, keyword: TokenKind, keyword_len: usize) {
        let keyword_start = self.scan_pos;
        self.push(keyword, keyword_start, keyword_start + keyword_len);

        let Some(open) = self.skip_to_delimiter() else {
            self.push_unterminated(TokenKind::UNTERMINATED_QUOTE_LIKE, keyword_start);
            return;
        };

        let (close, nests) = closing_delimiter(open);
        let content_kind = quote_like_content_kind(keyword);

        self.push_delimiter(open);
        if !self.scan_delimited_content(content_kind, open, close, nests, keyword_start) {
            return;
        }

        if two_part(keyword) {
            // `s{a}{b}` puts the second part behind its own opening delimiter;
            // `s/a/b/` reuses the one just consumed as the separator.
            if nests {
                let Some(second_open) = self.skip_to_delimiter() else {
                    self.push_unterminated(TokenKind::UNTERMINATED_QUOTE_LIKE, keyword_start);
                    return;
                };
                let (second_close, second_nests) = closing_delimiter(second_open);
                self.push_delimiter(second_open);
                if !self.scan_delimited_content(
                    second_content_kind(keyword),
                    second_open,
                    second_close,
                    second_nests,
                    keyword_start,
                ) {
                    return;
                }
            } else if !self.scan_delimited_content(
                second_content_kind(keyword),
                open,
                close,
                false,
                keyword_start,
            ) {
                return;
            }
        }

        self.scan_quote_like_flags(keyword);
        self.mark_end_of_run();
    }

    /// A bare `/.../` match, committed to as soon as a term is expected.
    pub(super) fn scan_bare_regex(&mut self) {
        let start = self.scan_pos;
        self.push_delimiter('/');
        if !self.scan_delimited_content(TokenKind::REGEX_PATTERN, '/', '/', false, start) {
            return;
        }
        self.scan_flags(MATCH_FLAGS);
        self.mark_end_of_run();
    }

    /// Say that the run ends at the token just pushed.
    fn mark_end_of_run(&mut self) {
        if let Some(last) = self.buffer.last_mut() {
            last.ends_quote_like_run = true;
        }
    }

    fn push_delimiter(&mut self, delimiter: char) {
        let start = self.scan_pos;
        self.push(TokenKind::DELIMITER, start, start + delimiter.len_utf8());
    }

    /// Scan content up to the matching close delimiter and push it, followed by
    /// the closing delimiter itself.
    ///
    /// Returns `false` if the construct was unterminated, in which case an error
    /// token covering the rest of the file has already been pushed.
    fn scan_delimited_content(
        &mut self,
        kind: TokenKind,
        open: char,
        close: char,
        nests: bool,
        construct_start: usize,
    ) -> bool {
        let start = self.scan_pos;
        let rest = self.remaining();
        let mut depth = 1usize;
        let content_len;

        let mut chars = rest.char_indices();
        loop {
            let Some((offset, ch)) = chars.next() else {
                let kind = if kind == TokenKind::REGEX_PATTERN && open == '/' {
                    TokenKind::UNTERMINATED_REGEX
                } else {
                    TokenKind::UNTERMINATED_QUOTE_LIKE
                };
                self.push_unterminated(kind, construct_start);
                return false;
            };

            // `q\hello\` uses the backslash itself as the delimiter, so there
            // is nothing for it to escape.
            if ch == '\\' && close != '\\' {
                chars.next();
                continue;
            }
            // A character class is *not* opaque to the delimiter search: perl
            // ends `/[a/]/` at the second slash and then reports an unmatched
            // `[`. Writing `/[a\/]/` or `m{[/]}` is how you mean the other
            // thing. Tracking classes here would make camello accept input perl
            // rejects — see scripts/perl-check.
            if nests && ch == open {
                depth += 1;
                continue;
            }
            if ch == close {
                depth -= 1;
                if depth == 0 {
                    content_len = offset;
                    break;
                }
            }
        }

        if content_len > 0 {
            self.push(kind, start, start + content_len);
        } else {
            // Empty content still needs a token so the tree shape does not vary
            // with whether the user wrote `//` or `/x/`.
            self.push_empty(kind, start);
        }
        self.push_delimiter(close);
        true
    }

    fn scan_quote_like_flags(&mut self, keyword: TokenKind) {
        match keyword {
            T!["m"] | T!["qr"] | T!["s"] => self.scan_flags(MATCH_FLAGS),
            T!["tr"] | T!["y"] => self.scan_flags(TRANSLITERATION_FLAGS),
            _ => {}
        }
    }

    fn scan_flags(&mut self, allowed: &[u8]) {
        let start = self.scan_pos;
        let bytes = self.remaining().as_bytes();
        let len = bytes
            .iter()
            .position(|byte| !allowed.contains(byte))
            .unwrap_or(bytes.len());
        if len > 0 {
            self.push(TokenKind::REGEX_FLAGS, start, start + len);
        }
    }

    /// One error token covering everything from the construct to end of input.
    ///
    /// One token means one diagnostic, and because the token ends the file there
    /// is nothing left for a stale mode to corrupt.
    fn push_unterminated(&mut self, kind: TokenKind, construct_start: usize) {
        // Drop the tokens already pushed for this construct so the error covers
        // it whole; partial delimiters would otherwise reach the parser.
        while self
            .buffer
            .last()
            .is_some_and(|token| usize::from(token.range.start()) >= construct_start)
        {
            self.buffer.pop();
        }
        self.scan_pos = construct_start;
        self.push(kind, construct_start, self.source.len());
    }

    fn push_empty(&mut self, kind: TokenKind, at: usize) {
        self.buffer.push(super::LexedToken {
            kind,
            range: rowan::TextRange::empty(
                rowan::TextSize::try_from(at).expect("source larger than 4GiB"),
            ),
            expect_at_lex: self.expect,
            ends_quote_like_run: false,
        });
    }

    /// `<<EOF`, `<<"EOF"`, `<<'EOF'`, `<<~EOF`, `<< "EOF"`.
    ///
    /// The bare form must be written against the `<<`: perl since 5.28 forbids
    /// `<< EOF` outright, and reading it as a heredoc would take a left shift's
    /// right operand for a terminator. A *quoted* terminator may be held off at
    /// a distance — perl allows it, and `eval << "    ..."` in real code relies
    /// on it, the quotes being what lets the terminator hold characters no
    /// identifier could.
    pub(super) fn heredoc_marker_len(&self) -> Option<usize> {
        let rest = self.remaining();
        let after = rest.strip_prefix("<<")?;
        let indent_len = usize::from(after.starts_with('~'));
        let after = &after[indent_len..];
        let space_len = after.len() - after.trim_start_matches([' ', '\t']).len();
        let after = &after[space_len..];

        let body_len = match after.as_bytes().first()? {
            b'"' | b'\'' => {
                let quote = after.as_bytes()[0];
                let end = after[1..].find(quote as char)?;
                end + 2
            }
            _ if space_len > 0 => return None,
            byte if byte.is_ascii_alphabetic() || *byte == b'_' => after
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .unwrap_or(after.len()),
            _ => return None,
        };

        Some(2 + indent_len + space_len + body_len)
    }

    pub(super) fn scan_heredoc_marker(&mut self, len: usize) {
        let start = self.scan_pos;
        let text = &self.source[start..start + len];
        let body = text.trim_start_matches("<<");
        let indentable = body.starts_with('~');
        let terminator = body
            .trim_start_matches('~')
            .trim_start_matches([' ', '\t'])
            .trim_matches(['"', '\''])
            .to_string();

        self.heredocs.push(Heredoc {
            terminator,
            indentable,
            marker_offset: start,
            body_start: None,
        });
        self.push(TokenKind::HEREDOC_START, start, start + len);
    }

    pub(super) fn next_heredoc(&self) -> Option<usize> {
        self.heredocs
            .iter()
            .position(|heredoc| heredoc.body_start.is_none())
    }

    /// Emit every pending heredoc body, in marker order, at the current line
    /// start.
    pub(super) fn scan_heredoc_bodies(&mut self) {
        while let Some(index) = self.next_heredoc() {
            let terminator = self.heredocs[index].terminator.clone();
            let indentable = self.heredocs[index].indentable;
            let body_start = self.scan_pos;
            self.heredocs[index].body_start = Some(body_start);

            match self.find_heredoc_end(&terminator, indentable) {
                Some((content_len, terminator_len)) => {
                    if content_len > 0 {
                        self.push(
                            TokenKind::HEREDOC_CONTENT,
                            body_start,
                            body_start + content_len,
                        );
                    } else {
                        self.push_empty(TokenKind::HEREDOC_CONTENT, body_start);
                    }
                    let end_start = self.scan_pos;
                    self.push(
                        TokenKind::HEREDOC_END,
                        end_start,
                        end_start + terminator_len,
                    );

                    // `foo(<<A, <<B)` puts two bodies one after the other, and
                    // the second starts at the line *after* the first
                    // terminator. Leaving that line terminator here made it the
                    // first byte of the next body, so B's contents gained a
                    // leading newline — which the token stream cannot see,
                    // because it is still one HEREDOC_CONTENT token with the
                    // same text either way, and perl printed a blank line.
                    //
                    // Only between bodies: with none left the newline is the
                    // ordinary scanner's, exactly as before.
                    if self.next_heredoc().is_some() && self.remaining().starts_with('\n') {
                        let newline = self.scan_pos;
                        self.push(TokenKind::NEWLINE, newline, newline + 1);
                    }
                }
                None => {
                    self.push(
                        TokenKind::UNTERMINATED_HEREDOC,
                        body_start,
                        self.source.len(),
                    );
                    return;
                }
            }
        }
    }

    /// Byte length of the body and of the terminator line, measured from the
    /// current position.
    fn find_heredoc_end(&self, terminator: &str, indentable: bool) -> Option<(usize, usize)> {
        let rest = self.remaining();
        let mut offset = 0usize;
        loop {
            let line_end = rest[offset..]
                .find('\n')
                .map_or(rest.len(), |index| offset + index);
            let line = &rest[offset..line_end];
            let candidate = if indentable { line.trim_start() } else { line };
            if candidate.trim_end_matches('\r') == terminator {
                return Some((offset, line.len()));
            }
            if line_end >= rest.len() {
                return None;
            }
            offset = line_end + 1;
        }
    }

    /// Byte length of a `format` header — `format NAME =` up to end of line —
    /// measured from the keyword, or `None` if this `format` is an ordinary word.
    ///
    /// `keyword_len` is the length of `format` itself, already scanned.
    pub(super) fn format_header_len(&self, keyword_len: usize) -> Option<usize> {
        let rest = &self.remaining()[keyword_len..];
        let mut offset = rest.len() - rest.trim_start_matches([' ', '\t']).len();

        // The name is optional: a bare `format =` writes to the currently
        // selected filehandle.
        let name = super::scan::ident_len_at(&rest[offset..]);
        offset += name;
        offset += rest[offset..].len() - rest[offset..].trim_start_matches([' ', '\t']).len();

        if !rest[offset..].starts_with('=') {
            return None;
        }
        offset += 1;
        // Nothing but the line ending may follow the `=`; `format => 1` is a
        // fat comma and `$x = format $y` is a call.
        let line_end = rest[offset..].find('\n').unwrap_or(rest[offset..].len());
        if !rest[offset..offset + line_end].trim().is_empty() {
            return None;
        }
        Some(keyword_len + offset)
    }

    /// A `format` declaration, scanned whole (ADR 0005 §3).
    ///
    /// The picture lines are not code and not an expression: `@<<<<` is a
    /// left-justified field five characters wide, and every space in it counts.
    /// Parsed as an expression it came out as `@< << <` — four tokens spaced by
    /// the formatter into something that means nothing — with no diagnostic to
    /// say so.
    pub(super) fn scan_format(&mut self, keyword_len: usize) {
        let start = self.scan_pos;
        let Some(header_len) = self.format_header_len(keyword_len) else {
            self.push(T!["format"], start, start + keyword_len);
            return;
        };

        self.push(T!["format"], start, start + keyword_len);
        // The header is one span: its internal spacing is the writer's, and
        // there is nothing in it for the formatter to lay out.
        if header_len > keyword_len {
            let header_start = self.scan_pos;
            self.push(TokenKind::RAW_CONTENT, header_start, start + header_len);
        }

        let rest = self.remaining();
        let newline_len = if rest.starts_with("\r\n") {
            2
        } else {
            usize::from(rest.starts_with('\n'))
        };
        if newline_len == 0 {
            return;
        }
        let newline_start = self.scan_pos;
        self.push(
            TokenKind::NEWLINE,
            newline_start,
            newline_start + newline_len,
        );

        // Everything up to and including the line holding only `.`.
        let body = self.remaining();
        let mut offset = 0usize;
        let end = loop {
            let line_end = body[offset..]
                .find('\n')
                .map_or(body.len(), |index| offset + index);
            if body[offset..line_end].trim_end_matches('\r') == "." {
                break line_end;
            }
            if line_end >= body.len() {
                break body.len();
            }
            offset = line_end + 1;
        };
        if end > 0 {
            let body_start = self.scan_pos;
            self.push(TokenKind::FORMAT_CONTENT, body_start, body_start + end);
        }
    }

    /// A POD block, from a `=command` in column 0 to the end of its `=cut` line.
    pub(super) fn scan_pod(&mut self) {
        let start = self.scan_pos;
        let rest = self.remaining();
        let mut offset = 0usize;

        loop {
            let line_end = rest[offset..]
                .find('\n')
                .map_or(rest.len(), |index| offset + index);
            if rest[offset..line_end].trim_end() == "=cut" {
                self.push(TokenKind::POD_CONTENT, start, start + line_end);
                return;
            }
            if line_end >= rest.len() {
                // POD running to end of file is legal and needs no `=cut`.
                self.push(TokenKind::POD_CONTENT, start, self.source.len());
                return;
            }
            offset = line_end + 1;
        }
    }

    /// Everything after `__END__` / `__DATA__` is carried verbatim.
    pub(super) fn scan_data_section(&mut self) {
        let rest = self.remaining();
        if rest.is_empty() {
            return;
        }
        let start = self.scan_pos;
        let newline_len = if rest.starts_with("\r\n") {
            2
        } else {
            usize::from(rest.starts_with('\n'))
        };
        if newline_len > 0 {
            self.push(TokenKind::NEWLINE, start, start + newline_len);
        }
        if self.scan_pos < self.source.len() {
            let content_start = self.scan_pos;
            self.push(TokenKind::DATA_CONTENT, content_start, self.source.len());
        }
    }
}

fn two_part(keyword: TokenKind) -> bool {
    matches!(keyword, T!["s"] | T!["tr"] | T!["y"])
}

fn quote_like_content_kind(keyword: TokenKind) -> TokenKind {
    match keyword {
        T!["q"] => TokenKind::LITERAL_STRING,
        T!["qq"] | T!["qx"] => TokenKind::INTERPOLATED_STRING,
        T!["qw"] => TokenKind::QW_STRING,
        T!["m"] | T!["qr"] | T!["s"] => TokenKind::REGEX_PATTERN,
        T!["tr"] | T!["y"] => TokenKind::TR_SEARCH_LIST,
        _ => TokenKind::RAW_CONTENT,
    }
}

fn second_content_kind(keyword: TokenKind) -> TokenKind {
    match keyword {
        T!["s"] => TokenKind::INTERPOLATED_STRING,
        T!["tr"] | T!["y"] => TokenKind::TR_REPLACEMENT_LIST,
        _ => TokenKind::RAW_CONTENT,
    }
}
