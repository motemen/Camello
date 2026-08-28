//! Byte offsets on one side, line and character on the other
//! (`docs/lsp.md`, "Documents and positions").
//!
//! camello speaks UTF-8 byte offsets everywhere — `TextRange`, `TextSize` —
//! and an LSP client speaks a line number and a *character* offset within it,
//! where "character" is whatever `positionEncoding` was negotiated and, for
//! VS Code, a UTF-16 code unit. Neither is the other, and
//! `camello_sema::LineIndex` is a third thing again: it counts Unicode
//! characters, which is right for a human-readable `path:line:col` and wrong
//! here.
//!
//! So this is the only place in the server where the two coordinate systems
//! meet. Nothing below the handler layer sees an LSP type, and nothing above
//! it sees a `TextRange`.

use rowan::{TextRange, TextSize};
use tower_lsp_server::ls_types::{Position, Range};

/// What a client's `character` counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// LSP 3.17's opt-in, and the cheap case: `character` is a byte offset,
    /// so the line table alone answers.
    Utf8,
    /// The protocol's default and what VS Code speaks.
    #[default]
    Utf16,
}

/// One document version's line table.
///
/// Built per version, walked per query. Lines are short, so converting by
/// walking one is cheaper than a per-line cache would be to keep correct —
/// and a cache here is the classic place a server starts answering questions
/// about text it no longer holds.
#[derive(Debug)]
pub struct PositionMap {
    text: String,
    /// Byte offset of the first character of each line.
    starts: Vec<u32>,
    encoding: Encoding,
}

impl PositionMap {
    #[must_use]
    pub fn new(text: &str, encoding: Encoding) -> Self {
        let mut starts = vec![0u32];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                // A file ending in a newline gets a last, empty line, which is
                // where a client puts the cursor after the final `\n`.
                starts.push(u32::try_from(offset + 1).unwrap_or(u32::MAX));
            }
        }
        PositionMap {
            text: text.to_string(),
            starts,
            encoding,
        }
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// The line's text without the terminator it ends with.
    ///
    /// `\r\n` is one terminator: a client counts characters in what it
    /// displays, and it displays neither half of it.
    fn line_text(&self, line: usize) -> &str {
        let Some(start) = self.starts.get(line).copied() else {
            return "";
        };
        let end = self
            .starts
            .get(line + 1)
            .copied()
            .unwrap_or_else(|| u32::try_from(self.text.len()).unwrap_or(u32::MAX));
        let slice = &self.text[start as usize..end as usize];
        slice
            .strip_suffix('\n')
            .map_or(slice, |rest| rest.strip_suffix('\r').unwrap_or(rest))
    }

    /// Where a byte offset falls, as the client counts.
    #[must_use]
    pub fn position(&self, offset: TextSize) -> Position {
        let offset = usize::from(offset).min(self.text.len());
        let line = match self.starts.binary_search(&(offset as u32)) {
            Ok(index) => index,
            Err(index) => index - 1,
        };
        let start = self.starts[line] as usize;
        // Clamped to the line's own text: an offset that lands on the `\r` of
        // a `\r\n`, or past the end, is the end of the line and not a
        // position on the next one.
        let within = offset.saturating_sub(start).min(self.line_text(line).len());
        let prefix = &self.line_text(line)[..within];
        let character = match self.encoding {
            Encoding::Utf8 => prefix.len(),
            Encoding::Utf16 => prefix.chars().map(char::len_utf16).sum(),
        };
        Position {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            character: u32::try_from(character).unwrap_or(u32::MAX),
        }
    }

    /// Where a client's position falls, as camello counts.
    ///
    /// Out-of-range input is clamped rather than refused: a position one past
    /// the last line is what a client sends for "the end of the document", and
    /// a `character` past the end of a line is what it sends while the buffer
    /// and the server disagree by one keystroke.
    #[must_use]
    pub fn offset(&self, position: Position) -> TextSize {
        let line = position.line as usize;
        if line >= self.starts.len() {
            return TextSize::from(u32::try_from(self.text.len()).unwrap_or(u32::MAX));
        }
        let start = self.starts[line] as usize;
        let text = self.line_text(line);
        let wanted = position.character as usize;
        let within = match self.encoding {
            Encoding::Utf8 => {
                // A byte offset the client chose may land inside a character;
                // the nearest boundary at or below it is the only defensible
                // reading.
                let mut within = wanted.min(text.len());
                while within > 0 && !text.is_char_boundary(within) {
                    within -= 1;
                }
                within
            }
            Encoding::Utf16 => {
                let mut units = 0usize;
                let mut within = text.len();
                for (index, ch) in text.char_indices() {
                    if units >= wanted {
                        within = index;
                        break;
                    }
                    // Half of a surrogate pair is not a place. The character
                    // it names starts here, and that is the answer — rounding
                    // down, the way the byte branch above rounds down to a
                    // char boundary.
                    if units + ch.len_utf16() > wanted {
                        within = index;
                        break;
                    }
                    units += ch.len_utf16();
                }
                within
            }
        };
        TextSize::from(u32::try_from(start + within).unwrap_or(u32::MAX))
    }

    #[must_use]
    pub fn range(&self, range: TextRange) -> Range {
        Range {
            start: self.position(range.start()),
            end: self.position(range.end()),
        }
    }

    #[must_use]
    pub fn text_range(&self, range: Range) -> TextRange {
        let start = self.offset(range.start);
        let end = self.offset(range.end);
        TextRange::new(start.min(end), start.max(end))
    }

    /// The whole document, as one range — what whole-file formatting replaces.
    #[must_use]
    pub fn whole(&self) -> Range {
        Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: self.position(TextSize::from(
                u32::try_from(self.text.len()).unwrap_or(u32::MAX),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str) -> PositionMap {
        PositionMap::new(text, Encoding::Utf16)
    }

    fn at(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn ascii_lines_round_trip() {
        let map = utf16("one\ntwo\nthree");
        assert_eq!(map.position(TextSize::from(0)), at(0, 0));
        assert_eq!(map.position(TextSize::from(4)), at(1, 0));
        assert_eq!(map.position(TextSize::from(6)), at(1, 2));
        assert_eq!(map.offset(at(1, 2)), TextSize::from(6));
        assert_eq!(map.offset(at(2, 5)), TextSize::from(13));
    }

    #[test]
    fn a_multi_byte_character_is_one_utf16_unit() {
        // `é` is two bytes and one UTF-16 unit; `あ` is three and one.
        let map = utf16("aéあb");
        assert_eq!(map.position(TextSize::from(1)), at(0, 1));
        assert_eq!(map.position(TextSize::from(3)), at(0, 2));
        assert_eq!(map.position(TextSize::from(6)), at(0, 3));
        assert_eq!(map.offset(at(0, 3)), TextSize::from(6));
        assert_eq!(map.offset(at(0, 4)), TextSize::from(7));
    }

    #[test]
    fn an_astral_character_is_two_utf16_units() {
        // U+1F600, four bytes and a surrogate pair.
        let map = utf16("a😀b");
        assert_eq!(map.position(TextSize::from(5)), at(0, 3));
        assert_eq!(map.offset(at(0, 3)), TextSize::from(5));
        // Half of a surrogate pair is not a place; the boundary below it is.
        assert_eq!(map.offset(at(0, 2)), TextSize::from(1));
    }

    #[test]
    fn a_byte_encoding_counts_bytes() {
        let map = PositionMap::new("aあb", Encoding::Utf8);
        assert_eq!(map.position(TextSize::from(4)), at(0, 4));
        assert_eq!(map.offset(at(0, 4)), TextSize::from(4));
        // Inside a character: the boundary at or below it.
        assert_eq!(map.offset(at(0, 2)), TextSize::from(1));
    }

    #[test]
    fn a_crlf_terminator_is_not_part_of_the_line() {
        let map = utf16("one\r\ntwo\r\n");
        assert_eq!(map.position(TextSize::from(3)), at(0, 3));
        // The `\r` is the end of line 0, not a position on it past its text.
        assert_eq!(map.position(TextSize::from(4)), at(0, 3));
        assert_eq!(map.position(TextSize::from(5)), at(1, 0));
        assert_eq!(map.offset(at(0, 9)), TextSize::from(3));
        // The file ends in a terminator, so there is a last, empty line.
        assert_eq!(map.line_count(), 3);
        assert_eq!(map.position(TextSize::from(10)), at(2, 0));
    }

    #[test]
    fn a_final_line_without_a_newline_is_a_line() {
        let map = utf16("one\ntwo");
        assert_eq!(map.line_count(), 2);
        assert_eq!(map.position(TextSize::from(7)), at(1, 3));
        assert_eq!(map.offset(at(1, 3)), TextSize::from(7));
        // Past the end, both ways.
        assert_eq!(map.offset(at(9, 0)), TextSize::from(7));
        assert_eq!(map.position(TextSize::from(99)), at(1, 3));
    }

    #[test]
    fn the_empty_document_has_one_line() {
        let map = utf16("");
        assert_eq!(map.line_count(), 1);
        assert_eq!(map.position(TextSize::from(0)), at(0, 0));
        assert_eq!(map.offset(at(0, 0)), TextSize::from(0));
        assert_eq!(map.whole().end, at(0, 0));
    }

    #[test]
    fn the_whole_range_ends_where_the_text_does() {
        let map = utf16("say 'あ';\n");
        assert_eq!(map.whole().start, at(0, 0));
        assert_eq!(map.whole().end, at(1, 0));
    }
}
