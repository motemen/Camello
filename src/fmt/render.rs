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
        }
    }

    pub fn render(mut self, doc: &Doc) -> Vec<Line> {
        self.walk(doc);
        if !self.current.text.is_empty() || !self.current.anchors.is_empty() {
            self.finish_line();
        }
        // A trailing blank line is an artefact of the final HardLine.
        while self.lines.last().is_some_and(Line::is_blank) {
            self.lines.pop();
        }
        self.lines
    }

    fn walk(&mut self, doc: &Doc) {
        match doc {
            Doc::Nil => {}
            Doc::Token(token) => self.write(token.text()),
            Doc::Raw(token) => self.write_raw(token.text()),
            Doc::VerbatimLines(token) => self.write_verbatim_lines(token.text()),
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
                    || previous.is_some_and(|line| line.text.trim_end().ends_with('{'));
                if !suppress {
                    self.finish_line();
                }
            }
            Doc::Anchor(class) => {
                let column = self.current.text.chars().count();
                self.current.anchors.push((*class, column));
            }
            Doc::Comment(text, placement) => self.comment(text, *placement),
            Doc::Shape(shape) => self.shape = Some(*shape),
        }
    }

    fn comment(&mut self, text: &str, placement: Placement) {
        match placement {
            Placement::OwnLine => {
                if !self.current.text.trim().is_empty() {
                    self.newline();
                }
                self.write(text);
            }
            Placement::Trailing => {
                // One rule, one place. The old formatter had two comment output
                // paths — one hard-coding four spaces, one copying the source's
                // whitespace — and the option only reached one of them.
                if !self.current.text.is_empty() {
                    for _ in 0..self.options.min_spaces_before_comment.max(1) {
                        self.current.text.push(' ');
                    }
                }
                let column = self.current.text.chars().count()
                    - self.options.min_spaces_before_comment.max(1);
                self.current
                    .anchors
                    .push((AnchorClass::TrailingComment, column));
                self.current.text.push_str(text);
            }
        }
    }

    fn write(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.ensure_indent();
        self.current.text.push_str(text);
    }

    /// Verbatim content: written exactly, and any newlines inside it end lines
    /// without the renderer adding indentation to what follows.
    fn write_raw(&mut self, text: &str) {
        self.ensure_indent();
        let mut parts = text.split('\n');
        if let Some(first) = parts.next() {
            self.current.text.push_str(first);
        }
        for part in parts {
            self.finish_line();
            // Deliberately not `write`: the continuation of a raw atom starts at
            // column 0, because that is where it was.
            self.current.text.push_str(part);
        }
    }

    /// Content that begins its own line at column 0.
    fn write_verbatim_lines(&mut self, text: &str) {
        self.continuation = false;
        if !self.current.text.is_empty() {
            self.finish_line();
        }
        let mut parts = text.split('\n').peekable();
        while let Some(part) = parts.next() {
            self.current.verbatim = true;
            self.current.text.push_str(part);
            if parts.peek().is_some() {
                self.finish_line();
            }
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
        let mut line = std::mem::take(&mut self.current);
        // Trailing whitespace is formatting, except inside a verbatim region
        // where it is content.
        if !line.verbatim {
            line.text = line.text.trim_end().to_string();
        }
        // A blank line does not consume the pending shape: the statement it was
        // declared for has not been emitted yet, and the align pass needs the
        // shape on the line that carries the anchor.
        if !line.text.trim().is_empty() {
            line.shape = self.shape.take();
        }
        self.lines.push(line);
    }
}
