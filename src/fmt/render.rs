//! Doc → lines (ADR 0008 §4).
//!
//! Indentation is applied here, when a line is started — never while appending
//! text. A `Raw` atom is written as-is, so no indentation can end up inside one.

use crate::fmt::doc::{AnchorClass, Doc, Placement, ShapeKey};
use crate::fmt::FormatterOptions;

/// One output line, with the column of each alignment anchor on it.
#[derive(Debug, Clone, Default)]
pub struct Line {
    pub text: String,
    pub anchors: Vec<(AnchorClass, usize)>,
    pub shape: Option<ShapeKey>,
    pub indent: usize,
    /// Part of a verbatim region. Its trailing whitespace is content, not
    /// formatting, so it is left alone.
    pub verbatim: bool,
}

impl Line {
    /// True for a line with no code on it.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }
}

pub struct Renderer<'a> {
    options: &'a FormatterOptions,
    lines: Vec<Line>,
    current: Line,
    indent: usize,
    /// Set while rendering the body of a broken group; `Line` and `SoftLine`
    /// only break inside one.
    broken: bool,
    /// The shape key most recently declared, applied to lines as they are
    /// finished.
    shape: Option<ShapeKey>,
    /// The next line is a continuation of the one before it.
    continuation: bool,
    /// A trailing comment has claimed the rest of this line.
    ///
    /// Everything after `#` on a line is inside the comment, so writing code
    /// there does not lay it out badly — it deletes it. The builder is
    /// responsible for never asking (a group holding a comment breaks), and this
    /// is here so that a builder bug costs a stray line break instead of a
    /// missing hash entry.
    line_closed: bool,
}

/// The empty piece after an atom's final newline is that newline, not a line of
/// its own: the output joins lines with one newline each, so counting it would
/// add a blank line per pass. Clearing `verbatim` on it is what lets the
/// trailing-blank sweep in [`Renderer::render`] take it away again.
fn mark_terminator(line: &mut Line, index: usize, parts: &[&str]) {
    if index + 1 == parts.len() && parts.len() > 1 && parts[index].is_empty() {
        line.verbatim = false;
    }
}

impl<'a> Renderer<'a> {
    pub fn new(options: &'a FormatterOptions) -> Self {
        Self {
            options,
            lines: Vec::new(),
            current: Line::default(),
            indent: 0,
            broken: true,
            shape: None,
            continuation: false,
            line_closed: false,
        }
    }

    pub fn render(mut self, doc: &Doc) -> Vec<Line> {
        self.walk(doc);
        if !self.current.text.is_empty() || !self.current.anchors.is_empty() {
            self.finish_line();
        }
        // A trailing blank line is an artefact of the final HardLine — unless it
        // is verbatim, in which case it is the file's content. `while (<DATA>)`
        // counts the blank lines at the end of a `__DATA__` section, and dropping
        // them changes what the program reads.
        while self
            .lines
            .last()
            .is_some_and(|line| line.is_blank() && !line.verbatim)
        {
            self.lines.pop();
        }
        self.lines
    }

    fn walk(&mut self, doc: &Doc) {
        match doc {
            Doc::Nil => {}
            Doc::Token(token) => self.write(token.text()),
            Doc::Raw(token) => self.write_raw(token.text()),
            Doc::VerbatimLines(text) => self.write_verbatim_lines(text),
            Doc::Space => self.write(" "),
            Doc::Concat(parts) => {
                for part in parts {
                    self.walk(part);
                }
            }
            Doc::Group { broken, body } => {
                let outer = std::mem::replace(&mut self.broken, *broken);
                self.walk(body);
                self.broken = outer;
            }
            Doc::Indent(body) => {
                self.indent += 1;
                self.walk(body);
                self.indent -= 1;
            }
            Doc::Line => {
                if self.broken {
                    self.newline();
                } else {
                    self.write(" ");
                }
            }
            Doc::SoftLine => {
                if self.broken {
                    self.newline();
                }
            }
            Doc::HardLine => self.newline(),
            Doc::UserLine { broken } => {
                if *broken {
                    self.newline();
                    // A line the user wrapped is indented one level
                    // (formatting.md INDENT-3). Applying it to the line rather
                    // than wrapping the doc in `Indent` is what keeps it at
                    // exactly one level however deeply the expression nests —
                    // and is why ADR 0002's fourteen branches are not needed.
                    self.continuation = true;
                }
            }
            Doc::BlankLine => {
                if !self.current.text.trim().is_empty() {
                    self.newline();
                }
                // Never two in a row (BLANK_LINE-3), and never straight after an
                // opening brace or at the top of the file.
                let previous = self.lines.last();
                let suppress = previous.is_none_or(Line::is_blank)
                    || previous.is_some_and(|line| line.text.trim_end().ends_with('{'))
                    // The newline that ended a heredoc terminator is structural,
                    // not a blank line the user left.
                    || previous.is_some_and(|line| line.verbatim);
                if !suppress {
                    self.finish_line();
                }
            }
            Doc::Anchor(class) => {
                let column = self.current.text.chars().count();
                self.current.anchors.push((*class, column));
            }
            Doc::Comment(text, placement) => self.comment(text, *placement),
            Doc::Shape(shape) => self.shape = *shape,
        }
    }

    /// Start a new line if `text` would otherwise land inside a comment.
    ///
    /// Whitespace is dropped rather than wrapped: a separator between something
    /// and a line break has nothing left to separate.
    fn open_line_for(&mut self, text: &str) -> bool {
        if !self.line_closed {
            return true;
        }
        if text.trim().is_empty() {
            return false;
        }
        self.newline();
        // Whatever this is, it was written as part of the line above.
        self.continuation = true;
        true
    }

    fn comment(&mut self, text: &str, placement: Placement) {
        if !self.open_line_for(text) {
            return;
        }
        match placement {
            Placement::OwnLine => {
                // The comment sits on the continuation's line, but it is the
                // code after it that the continuation indent is for, so the flag
                // survives the comment.
                let continuation = self.continuation;
                if !self.current.text.trim().is_empty() {
                    self.newline();
                }
                self.write(text);
                self.continuation = continuation;
            }
            Placement::Trailing => {
                // One rule, one place. The old formatter had two comment output
                // paths — one hard-coding four spaces, one copying the source's
                // whitespace — and the option only reached one of them.
                //
                // A trailing comment can reach a line with nothing on it — the
                // token it trailed produced no output — and then there is
                // nothing to separate it from, so it takes no padding and the
                // anchor sits where the line starts.
                let padding = if self.current.text.is_empty() {
                    0
                } else {
                    self.options.min_spaces_before_comment.max(1)
                };
                self.ensure_indent();
                for _ in 0..padding {
                    self.current.text.push(' ');
                }
                let column = self.current.text.chars().count() - padding;
                self.current
                    .anchors
                    .push((AnchorClass::TrailingComment, column));
                self.current.text.push_str(text);
                self.line_closed = true;
            }
        }
    }

    fn write(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if !self.open_line_for(text) {
            return;
        }
        self.ensure_indent();
        self.current.text.push_str(text);
    }

    /// Verbatim content: written exactly, and any newlines inside it end lines
    /// without the renderer adding indentation to what follows.
    fn write_raw(&mut self, text: &str) {
        if !self.open_line_for(text) {
            return;
        }
        self.ensure_indent();
        let parts: Vec<&str> = text.split('\n').collect();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                // The line ends inside the atom, so whatever whitespace is at
                // the end of it is content. `qr/\n  a  \n/x` and a `__DATA__`
                // section both lost characters to the trim, which is the I1
                // violation the renderer was supposed to make unrepresentable.
                self.current.verbatim = true;
                self.finish_line();
            }
            // Deliberately not `write`: the continuation of a raw atom starts at
            // column 0, because that is where it was.
            self.current.text.push_str(part);
            mark_terminator(&mut self.current, index, &parts);
        }
    }

    /// Content that begins its own line at column 0.
    fn write_verbatim_lines(&mut self, text: &str) {
        self.continuation = false;
        if !self.current.text.is_empty() {
            self.finish_line();
        }
        let parts: Vec<&str> = text.split('\n').collect();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                self.finish_line();
            }
            self.current.verbatim = true;
            self.current.text.push_str(part);
            mark_terminator(&mut self.current, index, &parts);
        }
    }

    fn ensure_indent(&mut self) {
        if !self.current.text.is_empty() {
            return;
        }
        let indent = self.indent + usize::from(self.continuation);
        self.continuation = false;
        self.current.indent = indent;
        self.current
            .text
            .push_str(&" ".repeat(indent * self.options.indent_width));
    }

    fn newline(&mut self) {
        self.finish_line();
    }

    fn finish_line(&mut self) {
        self.line_closed = false;
        let mut line = std::mem::take(&mut self.current);
        // Trailing whitespace is formatting, except inside a verbatim region
        // where it is content.
        if !line.verbatim {
            line.text = line.text.trim_end().to_string();
        }
        // The shape belongs to the line that carries an anchor, and to every such
        // line of the statement it was declared for — not just the first.
        // Consuming it here is what put `alpha => 1` in a group of its own and
        // left the entries under it to align without it: one statement, one
        // shape, and the lines it breaks across all carry it.
        if !line.anchors.is_empty() {
            line.shape = self.shape;
        }
        self.lines.push(line);
    }
}
