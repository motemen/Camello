//! Format skipping: `#<<<` and `#>>>`, perltidy's markers and perltidy's job.
//!
//! A hand-aligned table is a thing a formatter cannot be told about, so every
//! formatter that meets real code grows a way to be told to leave a region
//! alone. perltidy's is two comments, and the notation is worth keeping as it
//! is: a repository migrating to camello has the markers in it already.
//!
//! The region is a run of *lines*, which is what makes this a pass over the
//! rendered output rather than a rule in the builder. The lines between the
//! markers, and the marker lines themselves, are replaced by the source lines
//! they came from — so what the formatter decided about them is computed and
//! then thrown away, which costs a little and keeps the rest of the formatter
//! free of a flag that would have to travel through all of it.
//!
//! Finding the markers again in the output is safe because a comment is the one
//! thing that cannot be moved onto another line: an own-line comment breaks the
//! line before it and is followed by a hard break, so the marker is still alone
//! on a line of its own. A `#<<<` written inside a heredoc or a string is not a
//! marker, and cannot be mistaken for one in either direction — the input side
//! reads `COMMENT` tokens, and the output side skips lines the renderer marked
//! verbatim.

use crate::fmt::render::Line;
use crate::lang::{SyntaxNode, TokenExt, TokenKind};

const BEGIN: &str = "#<<<";
const END: &str = "#>>>";

/// A run of source lines to be reproduced as they were written, both marker
/// lines included. Zero-based, and inclusive at both ends.
pub struct Region {
    first: usize,
    /// `None` for a `#<<<` nothing closed: perltidy skips to the end of the
    /// file, and so does this.
    last: Option<usize>,
}

/// Is this comment the marker `tag`?
///
/// The marker is the whole comment or is followed by a space — perltidy asks
/// for the space, and it is what lets `#<<< keep this table` say why.
fn is_marker(text: &str, tag: &str) -> bool {
    let text = text.trim_end();
    text == tag
        || text
            .strip_prefix(tag)
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

/// The regions the source marks, in the order they were written.
///
/// Read off `COMMENT` tokens, so a `#<<<` inside a string, a heredoc body or
/// POD is text and not a marker. A marker is on a line of its own, as perltidy
/// asks; where it sits on that line is not asked about, because the code that
/// already carries these markers does not indent them consistently and there is
/// nothing to gain by refusing it.
pub fn regions(root: &SyntaxNode) -> Vec<Region> {
    let mut regions = Vec::new();
    let mut open: Option<usize> = None;
    let mut line = 0usize;
    // Nothing but whitespace on this line so far.
    let mut alone = true;

    for token in root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        let text = token.text();
        if token.token_kind() == TokenKind::COMMENT && alone {
            match open {
                None if is_marker(text, BEGIN) => open = Some(line),
                // No nesting: perltidy has none either, and a second `#<<<`
                // inside a region is text like any other line in it.
                Some(first) if is_marker(text, END) => {
                    regions.push(Region {
                        first,
                        last: Some(line),
                    });
                    open = None;
                }
                _ => {}
            }
        }

        // Counted from the text rather than from `NEWLINE` tokens: a heredoc
        // body, POD and a `__DATA__` section are each one token holding many
        // lines (the lexer contract), and counting only the newlines the parser
        // sees would put every marker below one on the wrong line.
        match text.rfind('\n') {
            Some(at) => {
                line += text.bytes().filter(|byte| *byte == b'\n').count();
                alone = text[at + 1..].chars().all(char::is_whitespace);
            }
            None => alone = alone && text.chars().all(char::is_whitespace),
        }
    }

    if let Some(first) = open {
        regions.push(Region { first, last: None });
    }
    regions
}

/// Put the source's own lines back over the ones the formatter produced.
pub fn restore(lines: &mut Vec<Line>, source: &str, regions: &[Region]) {
    let mut source_lines: Vec<&str> = source.split('\n').collect();
    // The piece after the file's final newline is that newline, not a line: the
    // output joins lines with one newline each, and counting it would leave a
    // blank line behind every region that runs to the end of the file.
    if source.ends_with('\n') {
        source_lines.pop();
    }
    if source_lines.is_empty() {
        return;
    }
    // Where to resume looking. The regions are in the order they were written
    // and the comments come out in the order they went in, so the next marker
    // in the output is the next region's.
    let mut from = 0usize;

    for region in regions {
        let Some(begin) = find_marker(lines, from, BEGIN) else {
            break;
        };
        let end = match region.last {
            Some(_) => match find_marker(lines, begin + 1, END) {
                Some(end) => end,
                None => break,
            },
            None => lines.len().saturating_sub(1),
        };
        let last = region.last.unwrap_or(source_lines.len().saturating_sub(1));
        let Some(replacement) = source_lines.get(region.first..=last.min(source_lines.len() - 1))
        else {
            break;
        };
        let replacement: Vec<Line> = replacement
            .iter()
            .map(|text| Line {
                text: (*text).to_string(),
                // Its whitespace is the writer's, down to the trailing kind:
                // the point of the region is that nothing here is the
                // formatter's to decide.
                verbatim: true,
                ..Line::default()
            })
            .collect();
        let count = replacement.len();
        lines.splice(begin..=end, replacement);
        from = begin + count;
    }
}

/// The first line at or after `from` that is the marker `tag` and nothing else.
///
/// A verbatim line is content — a heredoc body may hold a line reading `#<<<`
/// and it is a string, not a marker.
fn find_marker(lines: &[Line], from: usize, tag: &str) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, line)| !line.verbatim && is_marker(line.text.trim_start(), tag))
        .map(|(index, _)| index)
}
