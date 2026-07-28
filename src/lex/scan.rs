//! The scanner proper (ADR 0005 §1, §5).
//!
//! Hand-written rather than generated: the constructs a generator could handle
//! are the easy half, and the old lexer ended up performing surgery on the
//! generated tokens afterwards (splitting `0x7f..`, re-splitting `x5`). Scanning
//! these correctly the first time is simpler than repairing them.

use crate::lang::{TokenKind, T};

use super::{Expect, Lexer};

/// The characters perl actually accepts after `-` as a file test (`perlfunc`).
/// The old lexer took any single letter, so `-q $x` became a file test.
const FILE_TEST_CHARS: &[u8] = b"efdlpSbcugktrwxoRWXOszAMC";

/// Punctuation variables that are a single character after their sigil.
const PUNCT_VAR_CHARS: &[u8] = b"!@/\\,;.&`'+^:?<>()[]|\"-_0";

impl<'a> Lexer<'a> {
    fn rest(&self) -> &'a str {
        &self.source[self.scan_pos..]
    }

    fn rest_at(&self, offset: usize) -> &'a str {
        &self.source[self.scan_pos + offset..]
    }

    /// True at byte 0 or immediately after a line terminator.
    ///
    /// Computed from the source rather than carried in a flag, which is what
    /// makes an indented `=head1` stay ordinary code (D1): the old
    /// `at_line_start` survived whitespace tokens, so column tracking was wrong
    /// by construction.
    fn at_line_start(&self) -> bool {
        self.scan_pos == 0 || self.source.as_bytes()[self.scan_pos - 1] == b'\n'
    }

    /// Scan one step, pushing at least one token unless input is exhausted.
    ///
    /// Some steps push a whole run: a quote-like operator, a heredoc body, POD.
    /// That is the atomicity guarantee of ADR 0005 §3 — no scanning mode is
    /// observable between calls.
    pub(super) fn scan_next(&mut self) {
        if self.scan_pos >= self.source.len() {
            self.exhausted = true;
            return;
        }

        // A heredoc body interrupts everything else at the start of a line.
        if self.at_line_start() && self.next_heredoc().is_some() {
            self.scan_heredoc_bodies();
            return;
        }

        let start = self.scan_pos;
        let bytes = self.rest().as_bytes();
        let first = bytes[0];

        match first {
            b' ' | b'\t' => {
                let len = bytes
                    .iter()
                    .position(|byte| !matches!(byte, b' ' | b'\t'))
                    .unwrap_or(bytes.len());
                self.push(TokenKind::WHITESPACE, start, start + len);
                return;
            }
            // Exactly one line terminator per token, so blank lines survive as
            // consecutive NEWLINEs (ADR 0006 §1).
            b'\n' => {
                self.push(TokenKind::NEWLINE, start, start + 1);
                return;
            }
            b'\r' => {
                let len = if bytes.get(1) == Some(&b'\n') { 2 } else { 1 };
                self.push(TokenKind::NEWLINE, start, start + len);
                return;
            }
            b'#' => {
                let len = bytes
                    .iter()
                    .position(|byte| matches!(byte, b'\n' | b'\r'))
                    .unwrap_or(bytes.len());
                self.push(TokenKind::COMMENT, start, start + len);
                return;
            }
            _ => {}
        }

        if self.at_line_start() && self.scan_line_start_construct() {
            return;
        }

        if first.is_ascii_digit() {
            self.scan_number();
            return;
        }

        if first.is_ascii_alphabetic() || first == b'_' {
            self.scan_word();
            return;
        }

        if matches!(first, b'\'' | b'"' | b'`') {
            self.scan_quoted_string(first);
            return;
        }

        self.scan_punctuation();
    }

    /// POD and `__DATA__` are only recognised in column 0 (ADR 0005 §5).
    fn scan_line_start_construct(&mut self) -> bool {
        let rest = self.rest();

        if rest.starts_with('=') && rest[1..].starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            self.scan_pod();
            return true;
        }

        for (marker, kind) in [("__END__", T!["__END__"]), ("__DATA__", T!["__DATA__"])] {
            if let Some(after) = rest.strip_prefix(marker) {
                if after.is_empty() || after.starts_with(['\n', '\r']) {
                    let start = self.scan_pos;
                    self.push(kind, start, start + marker.len());
                    self.scan_data_section();
                    return true;
                }
            }
        }

        false
    }

    fn scan_number(&mut self) {
        let start = self.scan_pos;
        let bytes = self.rest().as_bytes();

        // Radix prefixes never take a fractional part, so `0x7f..3` closes the
        // number at `f` and leaves `..` to the operator scanner. The old lexer
        // produced `0x7f.` and then cut it back up (D-class bug 20).
        if bytes[0] == b'0' && bytes.len() > 1 {
            let radix_digits: Option<fn(u8) -> bool> = match bytes[1] {
                b'x' | b'X' => Some(|byte| byte.is_ascii_hexdigit() || byte == b'_'),
                b'b' | b'B' => Some(|byte| matches!(byte, b'0' | b'1' | b'_')),
                _ => None,
            };
            if let Some(is_digit) = radix_digits {
                let mut end = 2;
                while end < bytes.len() && is_digit(bytes[end]) {
                    end += 1;
                }
                self.push(TokenKind::NUMBER, start, start + end);
                return;
            }
        }

        let mut end = 0;
        let digits = |bytes: &[u8], from: usize| {
            let mut index = from;
            while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
                index += 1;
            }
            index
        };

        end = digits(bytes, end);
        let mut dots = 0;

        // `1..5` must not eat the range operator, so a `.` only continues the
        // number when it is not the start of `..`.
        while end < bytes.len() && bytes[end] == b'.' && bytes.get(end + 1) != Some(&b'.') {
            let after = digits(bytes, end + 1);
            if after == end + 1 {
                // A trailing `.` as in `1.` is part of the number.
                end += 1;
                break;
            }
            end = after;
            dots += 1;
        }

        if dots < 2 && end < bytes.len() && matches!(bytes[end], b'e' | b'E') {
            let mut exponent = end + 1;
            if exponent < bytes.len() && matches!(bytes[exponent], b'+' | b'-') {
                exponent += 1;
            }
            let after = digits(bytes, exponent);
            if after > exponent {
                end = after;
            }
        }

        // `5.10.1` is a version, not a number with two decimal points.
        let kind = if dots >= 2 {
            TokenKind::VERSION
        } else {
            TokenKind::NUMBER
        };
        self.push(kind, start, start + end);
    }

    /// Length of the identifier at `offset`, including `::` separators.
    fn ident_len(&self, offset: usize) -> usize {
        let bytes = self.rest_at(offset).as_bytes();
        if bytes.is_empty() || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
            return 0;
        }
        let mut end = 1;
        while end < bytes.len() {
            if bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' {
                end += 1;
            } else if bytes[end] == b':' && bytes.get(end + 1) == Some(&b':') {
                // Only a separator if a name follows; `Foo::` at the end of a
                // package statement is still one name.
                end += 2;
            } else {
                break;
            }
        }
        end
    }

    fn scan_word(&mut self) {
        let start = self.scan_pos;
        let len = self.ident_len(0);
        let text = &self.source[start..start + len];

        // A v-string (`v5.10.1`) looks like an identifier followed by numbers.
        if let Some(version_len) = self.vstring_len() {
            self.push(TokenKind::VERSION, start, start + version_len);
            return;
        }

        // `"abc"x5`: in operator position `x` binds as the repetition operator
        // and the digits are a separate number. Scanning it this way avoids the
        // old lexer's re-splitting pass.
        if self.expect == Expect::Operator
            && text.starts_with('x')
            && text.len() > 1
            && text[1..].bytes().all(|byte| byte.is_ascii_digit())
        {
            self.push(T!["x"], start, start + 1);
            return;
        }

        let Some(keyword) = TokenKind::from_keyword(text) else {
            self.push(TokenKind::IDENT, start, start + len);
            return;
        };

        if !self.keyword_applies_here(keyword, len) {
            self.push(TokenKind::IDENT, start, start + len);
            return;
        }

        if keyword.is_quote_like_keyword() {
            self.scan_quote_like(keyword, len);
            return;
        }

        self.push(keyword, start, start + len);
    }

    /// Whether a reserved word is actually reserved at this position.
    ///
    /// Perl keywords are positional: `x` is an operator only where one can go,
    /// and `s` starts a substitution only where a term can. Deciding this from
    /// `expect` alone replaces the old lexer's raw-character probing.
    fn keyword_applies_here(&mut self, keyword: TokenKind, len: usize) -> bool {
        let infix_only = keyword.is_quote_like_keyword();
        match self.expect {
            // In operator position a quote-like operator cannot start.
            Expect::Operator => !infix_only,
            Expect::Term => {
                if matches!(
                    keyword,
                    T!["x"]
                        | T!["eq"]
                        | T!["ne"]
                        | T!["lt"]
                        | T!["gt"]
                        | T!["le"]
                        | T!["ge"]
                        | T!["cmp"]
                        | T!["and"]
                        | T!["or"]
                        | T!["xor"]
                ) {
                    // These can only be infix, so in term position they are a
                    // bareword: `x(1)`, `{ or => 1 }`.
                    return false;
                }
                if infix_only {
                    return !self.quote_like_is_bareword(len);
                }
                true
            }
        }
    }

    /// The bareword exception of ADR 0005 §5.
    ///
    /// `(s => 1)` and `$h{q}` use quote-like names as plain words. Deciding it
    /// by looking one token ahead — past horizontal space only — keeps `s {a}{b}`
    /// working, which a "next character is `{`" rule would break.
    fn quote_like_is_bareword(&self, len: usize) -> bool {
        let after = self.rest_at(len).trim_start_matches([' ', '\t']);
        after.starts_with("=>") || after.starts_with('}') || after.starts_with(',')
    }

    /// `v5`, `v5.10.1`. Requires at least one digit, and two dots unless the
    /// name is exactly `v` followed by digits.
    fn vstring_len(&self) -> Option<usize> {
        let bytes = self.rest().as_bytes();
        if bytes[0] != b'v' {
            return None;
        }
        let mut end = 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == 1 {
            return None;
        }
        let mut dots = 0;
        while end < bytes.len()
            && bytes[end] == b'.'
            && bytes.get(end + 1).is_some_and(u8::is_ascii_digit)
        {
            end += 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            dots += 1;
        }
        // `v10` alone is a v-string only when followed by nothing name-like;
        // requiring a dot keeps ordinary identifiers like `v1` as identifiers
        // unless they are clearly versions.
        (dots >= 1).then_some(end)
    }

    fn scan_quoted_string(&mut self, quote: u8) {
        let start = self.scan_pos;
        let bytes = self.rest().as_bytes();
        let mut index = 1;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index += 2,
                byte if byte == quote => {
                    self.push(TokenKind::STRING, start, start + index + 1);
                    return;
                }
                _ => index += 1,
            }
        }
        // Never fall back to "not a string after all" — that is how the old
        // lexer let a stray quote rewrite the rest of the file (ADR 0005 §4).
        self.push(TokenKind::UNTERMINATED_STRING, start, self.source.len());
    }

    fn scan_punctuation(&mut self) {
        let start = self.scan_pos;
        let rest = self.rest();

        // Context-sensitive forms first: these are the ones whose spelling does
        // not determine the token.
        if self.expect == Expect::Term {
            if rest.starts_with('/') {
                // Committing to a match here — rather than searching for a
                // closing `/` first — is what makes lexing local: whether line 5
                // is a regex no longer depends on line 900 (D4).
                self.scan_bare_regex();
                return;
            }
            if let Some(len) = self.heredoc_marker_len() {
                self.scan_heredoc_marker(len);
                return;
            }
            if rest.starts_with('<') {
                if let Some(len) = self.io_operator_len() {
                    self.push(TokenKind::IDENT, start, start + len);
                    return;
                }
            }
            if let Some(len) = self.file_test_len() {
                self.push(TokenKind::FILE_TEST_OP, start, start + len);
                return;
            }
            if let Some((kind, len)) = self.sigil_at() {
                self.push(kind, start, start + len);
                self.scan_variable_name();
                return;
            }
        }

        for kind in OPERATORS {
            let text = kind.text().expect("operator table holds spelled kinds");
            if !rest.starts_with(text) {
                continue;
            }
            if !self.operator_applies_here(*kind) {
                continue;
            }
            self.push(*kind, start, start + text.len());
            return;
        }

        // Unknown byte. One token, one diagnostic, and scanning continues.
        let len = rest.chars().next().map_or(1, char::len_utf8);
        self.push(TokenKind::ERROR_CHAR, start, start + len);
    }

    fn operator_applies_here(&self, kind: TokenKind) -> bool {
        match self.expect {
            // `%`, `*` and `&` are sigils in term position, and the sigil branch
            // above has already claimed them.
            Expect::Term => !matches!(
                kind,
                TokenKind::MODULO | TokenKind::STAR | TokenKind::BITWISE_AND
            ),
            Expect::Operator => !matches!(
                kind,
                TokenKind::HASH_SIGIL | TokenKind::TYPEGLOB_SIGIL | TokenKind::CODE_SIGIL
            ),
        }
    }

    /// `-e`, `-f`, … but not `-q`; and not `-e_foo`, which is subtraction.
    fn file_test_len(&self) -> Option<usize> {
        let bytes = self.rest().as_bytes();
        if bytes[0] != b'-' {
            return None;
        }
        let letter = *bytes.get(1)?;
        if !FILE_TEST_CHARS.contains(&letter) {
            return None;
        }
        match bytes.get(2) {
            Some(byte) if byte.is_ascii_alphanumeric() || *byte == b'_' => None,
            _ => Some(2),
        }
    }

    /// `<STDIN>`, `<$fh>`, `<>`, `<Foo::Bar>`.
    ///
    /// Bounded to a single line, so an unmatched `<` is a comparison rather than
    /// a scan to end of file.
    fn io_operator_len(&self) -> Option<usize> {
        let bytes = self.rest().as_bytes();
        let mut index = 1;
        if bytes.get(index) == Some(&b'$') {
            index += 1;
        }
        while index < bytes.len() {
            match bytes[index] {
                b'>' => return Some(index + 1),
                byte if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b':' => index += 1,
                _ => return None,
            }
        }
        None
    }

    /// The sigil at the cursor, if a variable starts here.
    fn sigil_at(&self) -> Option<(TokenKind, usize)> {
        let bytes = self.rest().as_bytes();
        let kind = match bytes[0] {
            b'$' if bytes.get(1) == Some(&b'#') => {
                // `$#array`, but `$#` alone is the format-accumulator variable.
                let follows = bytes.get(2).copied();
                return match follows {
                    Some(byte)
                        if byte.is_ascii_alphabetic()
                            || byte == b'_'
                            || byte == b'{'
                            || byte == b'$' =>
                    {
                        Some((TokenKind::ARRAY_INDEX_SIGIL, 2))
                    }
                    _ => Some((TokenKind::SCALAR_SIGIL, 1)),
                };
            }
            b'$' => TokenKind::SCALAR_SIGIL,
            b'@' => TokenKind::ARRAY_SIGIL,
            b'%' => TokenKind::HASH_SIGIL,
            b'&' => TokenKind::CODE_SIGIL,
            b'*' => TokenKind::TYPEGLOB_SIGIL,
            _ => return None,
        };

        // No inspection of what follows. `foo %h` and `foo % h` differ only in
        // whitespace, and the old lexer read the raw bytes either side to guess
        // — which is exactly why the two disagreed (D7). Where a term is
        // expected, a sigil is a sigil; that is the rule, and it is written
        // down (ADR 0005 §5) rather than emergent.
        Some((kind, 1))
    }

    /// Emit the name part of a variable straight after its sigil.
    ///
    /// Doing this inside the lexer is what removes the old
    /// `consume_one_char_as_ident` / `consume_digit_prefixed_ident` escape
    /// hatches: `$@` and `$1` are ordinary tokens here, not raw-text pokes from
    /// the parser (ADR 0004 §5).
    fn scan_variable_name(&mut self) {
        let start = self.scan_pos;
        let bytes = self.rest().as_bytes();
        let Some(&first) = bytes.first() else { return };

        // A following `{` or `$` is a dereference; the parser builds that.
        if matches!(first, b'{' | b'$') {
            return;
        }

        if first.is_ascii_alphabetic() || first == b'_' {
            let len = self.ident_len(0);
            self.push(TokenKind::IDENT, start, start + len);
            return;
        }

        if first.is_ascii_digit() {
            let len = bytes
                .iter()
                .position(|byte| !byte.is_ascii_digit())
                .unwrap_or(bytes.len());
            self.push(TokenKind::NUMBER, start, start + len);
            return;
        }

        // `$^W` and friends.
        if first == b'^' && bytes.get(1).is_some_and(u8::is_ascii_alphabetic) {
            self.push(TokenKind::RAW_CONTENT, start, start + 2);
            return;
        }

        if PUNCT_VAR_CHARS.contains(&first) {
            self.push(TokenKind::RAW_CONTENT, start, start + 1);
        }
    }
}

/// Symbolic operators, longest first so that `**=` wins over `**` and `*`.
///
/// Built from the language definition rather than repeated here: every entry is
/// a kind whose `text()` is its spelling.
static OPERATORS: &[TokenKind] = &{
    // Order matters, so this is written out rather than sorted at runtime.
    // Grouped by length, descending.
    [
        // 4
        T!["->$#*"],
        // 3
        T!["->@*"],
        T!["->%*"],
        T!["->$*"],
        T!["->&*"],
        T!["->**"],
        T!["**="],
        T!["//="],
        T!["||="],
        T!["&&="],
        T!["<<="],
        T![">>="],
        T!["<=>"],
        T!["..."],
        // 2
        T!["=>"],
        T!["->"],
        T!["=="],
        T!["!="],
        T!["<="],
        T![">="],
        T!["=~"],
        T!["!~"],
        T!["~~"],
        T!["&&"],
        T!["||"],
        T!["//"],
        T!["**"],
        T!["++"],
        T!["--"],
        T!["<<"],
        T![">>"],
        T!["+="],
        T!["-="],
        T!["*="],
        T!["/="],
        T!["%="],
        T![".="],
        T!["x="],
        T!["|="],
        T!["&="],
        T!["^="],
        T![".."],
        T!["::"],
        T!["$#"],
        // 1
        T!["{"],
        T!["}"],
        T!["("],
        T![")"],
        T!["["],
        T!["]"],
        T![";"],
        T![","],
        T!["?"],
        T![":"],
        T!["="],
        T!["+"],
        T!["-"],
        T!["."],
        T!["/"],
        T!["<"],
        T![">"],
        T!["!"],
        T!["|"],
        T!["^"],
        T!["~"],
        T!["\\"],
        T!["$"],
        T!["@"],
        TokenKind::MODULO,
        TokenKind::STAR,
        TokenKind::BITWISE_AND,
        TokenKind::HASH_SIGIL,
        TokenKind::TYPEGLOB_SIGIL,
        TokenKind::CODE_SIGIL,
    ]
};
