use super::{HeredocMarker, Lexer};
use crate::SyntaxKind;

impl<'a> Lexer<'a> {
    /// Handle special tokens when in Value context
    pub(super) fn try_handle_expecting_value_context(&mut self) -> Option<(SyntaxKind, &'a str)> {
        // 1) Heredoc start
        if let Some(result) = self.try_consume_heredoc_start() {
            let (k, t) = result;
            self.update_line_position(t);
            return Some((k, t));
        }
        // 2) File test operator like -f
        if let Some(result) = self.try_consume_file_test_op() {
            let (k, t) = result;
            self.update_line_position(t);
            return Some((k, t));
        }
        // 3) Regex literal /.../
        if let Some(result) = self.try_consume_regex_literal() {
            let (k, t) = result;
            self.update_line_position(t);
            return Some((k, t));
        }
        // 4) Backtick command substitution `...`
        if let Some(result) = self.try_consume_backtick_literal() {
            let (k, t) = result;
            self.update_line_position(t);
            return Some((k, t));
        }
        None
    }

    fn try_consume_regex_literal(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();

        if !remainder.starts_with('/') {
            return None;
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum CharClassContext {
            Normal { allow_closing: bool },
            Posix { terminator: char },
        }

        let mut contexts: Vec<CharClassContext> = Vec::new();
        let mut escaped = false;
        let mut prev_char: Option<char> = None;
        let mut closing_slash_pos: Option<usize> = None;

        let mut idx = 1usize; // Skip initial '/'
        while idx < remainder.len() {
            let ch = match remainder[idx..].chars().next() {
                Some(c) => c,
                None => break,
            };
            let ch_len = ch.len_utf8();

            if escaped {
                escaped = false;
                if let Some(CharClassContext::Normal { allow_closing }) = contexts.last_mut() {
                    *allow_closing = true;
                }
                prev_char = Some(ch);
                idx += ch_len;
                continue;
            }

            if contexts.is_empty() {
                match ch {
                    '/' => {
                        closing_slash_pos = Some(idx);
                        break;
                    }
                    '\\' => {
                        escaped = true;
                    }
                    '[' => {
                        contexts.push(CharClassContext::Normal {
                            allow_closing: false,
                        });
                    }
                    _ => {}
                }
                prev_char = Some(ch);
                idx += ch_len;
                continue;
            }

            let mut push_context: Option<CharClassContext> = None;
            let mut pop_normal = false;
            let mut pop_posix = false;

            {
                let ctx = contexts.last_mut().unwrap();
                match ctx {
                    CharClassContext::Normal { allow_closing } => match ch {
                        '\\' => {
                            escaped = true;
                            *allow_closing = true;
                        }
                        '[' => {
                            let rest = &remainder[idx + ch_len..];
                            let mut chars = rest.chars();
                            match chars.next() {
                                Some(next @ (':' | '=' | '.')) => {
                                    push_context =
                                        Some(CharClassContext::Posix { terminator: next });
                                    *allow_closing = true;
                                }
                                _ => {
                                    *allow_closing = true;
                                }
                            }
                        }
                        ']' => {
                            if *allow_closing {
                                pop_normal = true;
                            } else {
                                *allow_closing = true;
                            }
                        }
                        '^' => {
                            if *allow_closing {
                                // treat '^' as a literal after the first position
                            } else {
                                // Leading '^' keeps allow_closing false so that an immediate ']'
                                // is still treated as literal content.
                            }
                        }
                        _ => {
                            *allow_closing = true;
                        }
                    },
                    CharClassContext::Posix { terminator } => {
                        if ch == ']' && prev_char == Some(*terminator) {
                            pop_posix = true;
                        }
                    }
                }
            }

            if pop_posix {
                contexts.pop();
                if let Some(CharClassContext::Normal { allow_closing }) = contexts.last_mut() {
                    *allow_closing = true;
                }
            }

            if pop_normal {
                contexts.pop();
                if let Some(CharClassContext::Normal { allow_closing }) = contexts.last_mut() {
                    *allow_closing = true;
                }
            }

            if let Some(ctx) = push_context {
                contexts.push(ctx);
            }

            prev_char = Some(ch);
            idx += ch_len;
        }

        if let Some(pos) = closing_slash_pos {
            let mut end_pos = pos + 1;
            const VALID_FLAGS: &str = "msixpodualngcer";
            for c in remainder[end_pos..].chars() {
                if VALID_FLAGS.contains(c) {
                    end_pos += c.len_utf8();
                } else {
                    break;
                }
            }

            let text = &remainder[..end_pos];
            self.logos_lexer.bump(end_pos);
            return Some((SyntaxKind::REGEX_LITERAL, text));
        }

        None
    }

    fn try_consume_backtick_literal(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();

        if !remainder.starts_with('`') {
            return None;
        }

        let mut closing_backtick_pos: Option<usize> = None;
        let mut escaped = false;

        for (i, c) in remainder.char_indices().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '`' => {
                    closing_backtick_pos = Some(i);
                    break;
                }
                '\\' => {
                    escaped = true;
                }
                '\n' => {
                    // Backticks can span lines unlike regex literals
                }
                _ => {}
            }
        }

        if let Some(pos) = closing_backtick_pos {
            let text = &remainder[..=pos];
            self.logos_lexer.bump(text.len());
            return Some((SyntaxKind::BACKTICK_STRING, text));
        }

        None
    }

    fn try_consume_heredoc_start(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if !remainder.starts_with("<<") {
            return None;
        }

        let bytes = remainder.as_bytes();
        let mut idx = 2;
        while idx < bytes.len() && (bytes[idx] == b' ' || bytes[idx] == b'\t') {
            idx += 1;
        }

        let mut strip_indent = false;
        if idx < bytes.len() && bytes[idx] == b'~' {
            strip_indent = true;
            idx += 1;
            while idx < bytes.len() && (bytes[idx] == b' ' || bytes[idx] == b'\t') {
                idx += 1;
            }
        }

        if idx >= bytes.len() {
            return None;
        }

        let marker_start = idx;
        let marker: &str;
        let is_quoted;

        match bytes[idx] {
            b'\'' | b'"' | b'`' => {
                is_quoted = true;
                let quote = bytes[idx];
                idx += 1;
                let content_start = idx;
                while idx < bytes.len() {
                    if bytes[idx] == quote {
                        break;
                    }
                    idx += 1;
                }
                if idx >= bytes.len() {
                    return None;
                }
                marker = &remainder[content_start..idx];
                idx += 1;
            }
            _ => {
                is_quoted = false;
                if !(bytes[idx].is_ascii_alphabetic() || bytes[idx] == b'_') {
                    return None;
                }
                idx += 1;
                while idx < bytes.len() {
                    let ch = bytes[idx];
                    if ch.is_ascii_alphanumeric() || ch == b'_' {
                        idx += 1;
                    } else {
                        break;
                    }
                }
                marker = &remainder[marker_start..idx];
            }
        }

        // Only validate marker characters for unquoted markers
        // Quoted markers can contain any characters
        if marker.is_empty() {
            return None;
        }

        if !is_quoted
            && (!marker
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                || !marker
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'))
        {
            return None;
        }

        let text = &remainder[..idx];
        self.logos_lexer.bump(idx);
        self.heredoc_queue.push_back(HeredocMarker {
            marker,
            strip_indent,
        });
        Some((SyntaxKind::HEREDOC_START, text))
    }

    fn try_consume_file_test_op(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();
        if !remainder.starts_with('-') {
            return None;
        }

        let mut chars = remainder.chars();
        if chars.next() != Some('-') {
            return None;
        }

        let op = chars.next()?;
        if !op.is_alphabetic() {
            return None;
        }

        // If the third char exists and is alphanumeric, it's not a file test op (e.g., -abcde)
        if remainder.chars().nth(2).is_some_and(char::is_alphanumeric) {
            return None;
        }

        let text = &remainder[..2];
        self.logos_lexer.bump(2);
        Some((SyntaxKind::FILE_TEST_OP, text))
    }

    /// Try to consume postfix dereference operators (->@*, ->%*, ->$*, ->$#*, ->&*, ->**)
    // FIXME: This is a bit of a hacky solution - ideally Logos would support context-sensitive lexing
    pub(super) fn try_consume_postfix_deref(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let remainder = self.logos_lexer.remainder();

        if let Some(rest) = remainder.strip_prefix("->") {
            let (kind, len) = if rest.starts_with("$#*") {
                (Some(SyntaxKind::POSTFIX_DEREF_ARRAY_LAST_INDEX), 5)
            } else if rest.starts_with("@*") {
                (Some(SyntaxKind::POSTFIX_DEREF_ARRAY), 4)
            } else if rest.starts_with("%*") {
                (Some(SyntaxKind::POSTFIX_DEREF_HASH), 4)
            } else if rest.starts_with("$*") {
                (Some(SyntaxKind::POSTFIX_DEREF_SCALAR), 4)
            } else if rest.starts_with("&*") {
                (Some(SyntaxKind::POSTFIX_DEREF_CODE), 4)
            } else if rest.starts_with("**") {
                (Some(SyntaxKind::POSTFIX_DEREF_GLOB), 4)
            } else {
                (None, 0)
            };

            if let Some(kind) = kind {
                let text = &remainder[..len];
                self.logos_lexer.bump(len);
                return Some((kind, text));
            }
        }
        None
    }

    pub(super) fn bump_until_marker(&mut self) -> Option<(SyntaxKind, &'a str)> {
        let HeredocMarker {
            marker,
            strip_indent,
        } = self.heredoc_queue.pop_front()?;
        let remainder = self.logos_lexer.remainder();
        let bytes = remainder.as_bytes();
        let mut search_pos = 0;

        while search_pos <= bytes.len() {
            // Find end of current line
            let mut line_end = search_pos;
            let mut newline_len = 0;
            while line_end < bytes.len() {
                match bytes[line_end] {
                    b'\n' => {
                        newline_len = 1;
                        break;
                    }
                    b'\r' => {
                        newline_len = if line_end + 1 < bytes.len() && bytes[line_end + 1] == b'\n'
                        {
                            2
                        } else {
                            1
                        };
                        break;
                    }
                    _ => line_end += 1,
                }
            }

            let line = &remainder[search_pos..line_end];
            let mut is_marker_line = line == marker;

            if strip_indent && !is_marker_line {
                let trimmed = line.trim_start_matches([' ', '\t']);
                if trimmed == marker {
                    is_marker_line = true;
                }
            }

            if is_marker_line && (line_end == bytes.len() || newline_len > 0) {
                // Found terminator
                let content = &remainder[..search_pos];
                let end = &remainder[search_pos..line_end + newline_len];
                self.logos_lexer.bump(line_end + newline_len);
                if !end.is_empty() {
                    self.pending.push_back((SyntaxKind::HEREDOC_END, end));
                }
                return Some((SyntaxKind::HEREDOC_CONTENT, content));
            }

            if newline_len == 0 {
                // EOF without marker
                self.logos_lexer.bump(remainder.len());
                return Some((SyntaxKind::HEREDOC_CONTENT, remainder));
            }

            search_pos = line_end + newline_len;
        }
        None
    }
}
