//! Doc → lines (the formatter contract).
//!
//! Indentation is applied here, when a line is started — never while appending
//! text. A `Raw` atom is written as-is, so no indentation can end up inside one.

use unicode_width::UnicodeWidthStr;

use crate::doc::{AnchorClass, Doc, Placement, ShapeKey};
use crate::FormatterOptions;

/// A place on a line that wants to agree with the lines around it.
#[derive(Debug, Clone, Copy)]
pub struct Anchor {
    pub class: AnchorClass,
    /// Where it sits on screen — the display width of the text before it, so
    /// that `'あいう'` counts the six columns it occupies rather than the three
    /// characters it is written with.
    pub column: usize,
    /// Where padding for it goes, as a byte index into the line's text. Carried
    /// rather than re-derived, because a column no longer identifies a position
    /// in the string once one character can be two columns wide.
    pub byte: usize,
    /// The width of what follows that has to end at the group's column, so that
    /// `=` and `-=` agree on their `=` rather than on where the operator starts.
    /// Zero for the classes that agree at the anchor itself.
    pub tail: usize,
}

/// One output line, with the alignment anchors on it.
#[derive(Debug, Clone, Default)]
pub struct Line {
    pub text: String,
    pub anchors: Vec<Anchor>,
    pub shape: Option<ShapeKey>,
    pub indent: usize,
    /// The column this line starts from before its own indentation: the
    /// hanging column it was placed at, plus whatever the construct around it
    /// starts from. A bracket opened on such a line is placed from there
    /// (docs/formatting.md INDENT-4), so its contents are one level in from it
    /// rather than from the margin.
    ///
    /// `None` for a line the renderer did not place: verbatim content owns its
    /// own lines and starts them in column 0 (`Doc::VerbatimLines`, and the
    /// lines after the first of a `Doc::Raw`), so neither that column nor this
    /// line's `indent` says anything about where a construct opened on it
    /// belongs. `$obj->meth(q[\n    foo\n], {` is the case: the `{` is written
    /// after a line the heredoc-like literal placed, and what it opens is still
    /// the argument list's.
    pub origin: Option<usize>,
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
    /// Set while rendering the body of a group that occupies more than one line
    /// — usually the same thing as `broken`, and not always (`Doc::Group`).
    /// Anchors written where this is false have no second line to agree with.
    anchored: bool,
    /// The shape key most recently declared, applied to lines as they are
    /// finished.
    shape: Option<ShapeKey>,
    /// The next line is a continuation of the one before it.
    continuation: bool,
    /// The column the lines being written start from, before their own
    /// indentation: zero at the margin, and the column a bracket was opened at
    /// for everything written inside it.
    origin: usize,
    /// Exact continuation offset requested by a hanging-indent scope.
    hanging_continuation: Option<usize>,
    /// Hanging offset currently in scope, if any.
    hanging: Option<usize>,
    /// Whether a continuation scope is open, and whether a user break in it has
    /// already taken its indent level (docs/formatting.md INDENT-3): one level for
    /// the whole of a wrapped expression, however many times it wraps.
    continued: Option<bool>,
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
            anchored: true,
            shape: None,
            continuation: false,
            hanging_continuation: None,
            hanging: None,
            origin: 0,
            continued: None,
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
            Doc::Raw(text) => self.write_raw(text),
            Doc::VerbatimLines(text) => self.write_verbatim_lines(text),
            Doc::Space => self.write(" "),
            Doc::Concat(parts) => {
                for part in parts {
                    self.walk(part);
                }
            }
            Doc::Group {
                broken,
                anchored,
                body,
            } => {
                let outer_broken = std::mem::replace(&mut self.broken, *broken);
                let outer_anchored = std::mem::replace(&mut self.anchored, *anchored);
                self.walk(body);
                self.broken = outer_broken;
                self.anchored = outer_anchored;
            }
            Doc::Indent(body) => {
                self.indent += 1;
                self.walk(body);
                self.indent -= 1;
            }
            Doc::Hanging { columns, body } => {
                // No offset asks for where the lines already are rather than
                // for a column of its own, so a scope around it that hangs is
                // the answer and the line's base indentation is what is left
                // when none does. `args a => f [],` hangs its arguments under
                // `a`, and the call written as the value of that pair takes the
                // lines under it back to the same column — column zero put them
                // out at the margin, and further out still with the statement
                // indented.
                // A column of zero is no column at all: the lines are already
                // where they belong, and a wrap in them is an ordinary
                // continuation rather than something to hold at a column.
                let columns = columns.or(self.hanging).filter(|columns| *columns > 0);
                let outer = std::mem::replace(&mut self.hanging, columns);
                self.walk(body);
                self.hanging = outer;
            }
            Doc::Rooted(body) => {
                // The level of the line this begins on. Once something is on
                // the line, that line's own level is the answer — the
                // continuation, if there was one, is already in it. On a line
                // nobody has written to, a pending continuation is taken here
                // instead of one line later, so it holds for the whole
                // construct rather than for its first line.
                let outer_indent = self.indent;
                let outer_origin = self.origin;
                if self.current.text.is_empty() {
                    if self.continuation && self.hanging_continuation.is_none() {
                        self.indent += 1;
                        self.continuation = false;
                    }
                } else if let Some(origin) = self.current.origin {
                    // Where the line starts is where this construct starts, so
                    // what it opens is measured from there. A bracket opened on
                    // a line placed at a hanging column read its own level from
                    // the margin instead, and put its contents to the left of
                    // the bracket and its closer in column zero.
                    self.indent = self.current.indent;
                    self.origin = origin;
                }
                self.walk(body);
                self.indent = outer_indent;
                self.origin = outer_origin;
            }
            Doc::Statements(body) => {
                // A wrap inside takes its level from the block, not from the
                // scope the block was written in, and gives it back at the
                // closing brace.
                let indent = self.indent;
                let outer = self.continued.take();
                self.walk(body);
                self.indent = indent;
                self.continued = outer;
            }
            Doc::Continuation(body) => {
                // The level a user break takes belongs to this scope: an
                // `Indent` inside it starts from the deeper level, and whatever
                // is emitted after it — a closing bracket — starts from the
                // level the construct began at.
                let indent = self.indent;
                let outer = self.continued.replace(false);
                self.walk(body);
                self.indent = indent;
                self.continued = outer;
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
            Doc::UserLine { broken, wraps } => {
                if *broken {
                    self.newline();
                    if !*wraps {
                        // Not a wrap: the line was ended by the layout of what
                        // came before it, so there is no level to take. A
                        // bareword call's arguments still hang from where they
                        // started — that column is not a continuation indent
                        // but where the argument list began.
                        if let Some(columns) = self.hanging {
                            self.hanging_continuation = Some(columns);
                        }
                        return;
                    }
                    // A line the user wrapped is indented one level
                    // (docs/formatting.md INDENT-3), and one level is all it takes
                    // however many times the expression wraps — so the level is
                    // taken once per continuation scope, not once per break.
                    match self.continued {
                        // Inside a bracket, the level lasts to the end of its
                        // contents, so what nests inside nests deeper.
                        Some(false) => {
                            self.continued = Some(true);
                            self.indent += 1;
                        }
                        Some(true) => {}
                        // Outside one there is nothing to hold it: a wrapped
                        // condition or signature is followed by the block it
                        // belongs to, which starts again from the statement's
                        // own level.
                        None => match self.hanging {
                            Some(columns) => self.hanging_continuation = Some(columns),
                            None => self.continuation = true,
                        },
                    }
                }
            }
            Doc::BlankLine => {
                // Never two in a row (BLANK_LINE-3), and never straight after an
                // opening brace or at the top of the file.
                //
                // The line break that would carry the blank line is suppressed
                // with it. Breaking first and only then declining to leave the
                // line empty is what turned `f({\n\n})` into `f({\n})` — output
                // holding a break that nothing asked for, and which the next
                // pass closes up, so the first pass was not a fixed point.
                let pending = !self.current.text.trim().is_empty();
                let suppress = if pending {
                    self.current.text.trim_end().ends_with('{')
                } else {
                    self.lines
                        .last()
                        .is_none_or(|line| line.is_blank() || line.text.trim_end().ends_with('{'))
                };
                if !suppress {
                    if pending {
                        self.newline();
                    }
                    self.finish_line();
                }
            }
            Doc::Anchor(class, tail) => {
                // Alignment is a relation between lines, so an anchor inside a
                // group that occupies one line has nothing to hold. Keeping it
                // let `bar(b => $y, charlie => $z)` join the vertical group of
                // the call above it and pad a `=>` that no other line shares —
                // and, because only the first anchor of a class on a line is
                // read, it was the padding of an arbitrary one of the pair.
                // A flat hash nested inside a broken hash still participates
                // in the outer lines' per-depth alignment. This is how
                // `{ aaa => 1 }` and `{ b => 2 }` line up when they are values
                // on adjacent lines. Other one-line groups stay anchor-free so a
                // one-line call cannot leak into a vertical group around it.
                let nested_flat_fat_comma =
                    matches!(class, AnchorClass::FatComma(depth) if *depth > 1);
                if self.anchored || nested_flat_fat_comma {
                    self.current.anchors.push(Anchor {
                        class: *class,
                        column: self.current.text.width(),
                        byte: self.current.text.len(),
                        tail: *tail,
                    });
                }
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
                let hanging_continuation = self.hanging_continuation;
                if !self.current.text.trim().is_empty() {
                    self.newline();
                }
                self.write(text);
                self.continuation = continuation;
                self.hanging_continuation = hanging_continuation;
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
                // The anchor is where the padding starts, not where the comment
                // does: aligning a group means agreeing on that column and then
                // each line paying its own minimum out of it.
                let anchor = Anchor {
                    class: AnchorClass::TrailingComment,
                    column: self.current.text.width(),
                    byte: self.current.text.len(),
                    tail: 0,
                };
                self.current.anchors.push(anchor);
                for _ in 0..padding {
                    self.current.text.push(' ');
                }
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
        self.hanging_continuation = None;
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
        let hanging = self.hanging_continuation.take();
        let indent = self.indent + usize::from(self.continuation && hanging.is_none());
        self.continuation = false;
        self.current.indent = indent;
        let origin = self.origin + hanging.unwrap_or(0);
        self.current.origin = Some(origin);
        let columns = origin + indent * self.options.indent_width;
        self.current.text.push_str(&" ".repeat(columns));
    }

    /// End the current line — unless there is nothing on it.
    ///
    /// A line break where a line has not been started adds nothing, and a
    /// verbatim region ends by starting a line nobody has written to yet: the
    /// break that closes a heredoc terminator and the one the enclosing group
    /// puts after the element the marker belonged to are the same break, and
    /// counting both left a blank line between the body and the code after it.
    /// `Doc::BlankLine` is how a blank line is asked for, and it finishes the
    /// line itself rather than coming through here.
    fn newline(&mut self) {
        if self.current.text.trim().is_empty() && !self.current.verbatim {
            // Indentation written for text that never arrived belongs to no
            // line; the next one starts its own.
            self.current.text.clear();
            return;
        }
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
        // An anchor with nothing after it on the line has nothing to align: the
        // thing it was going to hold went to the next line. Padding it would put
        // spaces at the end of a line — which the next pass trims, so the output
        // would not be its own fixed point (the formatter contract). `$x == 200\n    || $y`
        // in HTTP::Status leaves one behind on every operand but the last.
        line.anchors.retain(|anchor| anchor.byte < line.text.len());
        // A flat nested hash may contain several pairs on one physical line.
        // There is no single column that can represent that class on the line,
        // so exclude the whole class rather than aligning an arbitrary first
        // occurrence.
        let duplicate_classes: Vec<AnchorClass> = line
            .anchors
            .iter()
            .enumerate()
            .filter_map(|(index, anchor)| {
                line.anchors[index + 1..]
                    .iter()
                    .any(|later| later.class == anchor.class)
                    .then_some(anchor.class)
            })
            .collect();
        line.anchors
            .retain(|anchor| !duplicate_classes.contains(&anchor.class));
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
