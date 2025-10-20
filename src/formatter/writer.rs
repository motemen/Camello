use crate::{PerlLanguage, SyntaxKind};
use rowan::SyntaxToken;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct TokenSpan {
    pub(super) kind: SyntaxKind,
    pub(super) start_byte: usize,
    pub(super) end_byte: usize,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Line {
    pub(super) text: String,
    pub(super) tokens: Vec<TokenSpan>,
}

impl Line {
    pub(super) fn new() -> Self {
        Self {
            text: String::new(),
            tokens: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineBreakSource {
    User,
    Formatter,
}

#[derive(Debug, Default)]
pub(super) struct Writer {
    current_line: Line,
    lines: Vec<Line>,
    indent_level: usize,
    indent_string: String,
    prev_token_kind: Option<SyntaxKind>,
    last_significant_token_kind: Option<SyntaxKind>,
    last_line_break: Option<LineBreakSource>,
    at_line_start: bool,
}

impl Writer {
    pub(super) fn new() -> Self {
        Self {
            indent_string: "    ".to_string(),
            at_line_start: true,
            ..Self::default()
        }
    }

    pub(super) fn finish(&mut self) -> String {
        self.lines.push(std::mem::take(&mut self.current_line));
        std::mem::take(&mut self.lines)
            .into_iter()
            .map(|line| line.text)
            .fold(String::new(), |mut acc, line| {
                if !acc.is_empty() {
                    acc.push('\n');
                }
                acc.push_str(&line);
                acc
            })
    }

    pub(super) fn write_token(&mut self, token: &SyntaxToken<PerlLanguage>) {
        self.write_str(token.text(), Some(token.kind()));
    }

    pub(super) fn write_str(&mut self, text: &str, kind: Option<SyntaxKind>) {
        let mut is_first_part = true;
        for part in text.split('\n') {
            if is_first_part {
                is_first_part = false;
            } else {
                self.handle_user_newline();
            }

            if part.is_empty() {
                continue;
            }

            if self.at_line_start && kind.is_some_and(|k| k.is_content_token()) {
                self.add_indent();
            }

            let start = self.current_line.text.len();
            self.current_line.text.push_str(part);
            self.at_line_start = false;

            if let Some(kind) = kind {
                let end = self.current_line.text.len();
                self.current_line.tokens.push(TokenSpan {
                    kind,
                    start_byte: start,
                    end_byte: end,
                });
            }
        }
    }

    pub(super) fn write_char(&mut self, ch: char) {
        if ch == '\n' {
            self.handle_formatter_newline();
        } else {
            self.current_line.text.push(ch);
        }
    }

    pub(super) fn handle_newline_from(&mut self, source: LineBreakSource) {
        let line = std::mem::take(&mut self.current_line);
        self.lines.push(line);
        self.at_line_start = true;
        self.last_line_break = Some(source);
    }

    pub(super) fn handle_user_newline(&mut self) {
        self.handle_newline_from(LineBreakSource::User);
    }

    pub(super) fn handle_formatter_newline(&mut self) {
        self.handle_newline_from(LineBreakSource::Formatter);
    }

    pub(super) fn add_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.current_line.text.push_str(&self.indent_string);
        }
    }

    pub(super) fn push_indent_string(&mut self) {
        self.current_line.text.push_str(&self.indent_string);
    }

    pub(super) fn push_empty_line(&mut self) {
        self.lines.push(Line::new());
    }

    pub(super) fn is_output_empty(&self) -> bool {
        self.lines.is_empty() && self.current_line.text.is_empty()
    }

    pub(super) fn ends_with_newline(&self) -> bool {
        self.current_line.text.is_empty()
    }

    pub(super) fn ends_with_double_newline(&self) -> bool {
        self.current_line.text.is_empty()
            && self
                .lines
                .last()
                .map(|line| line.text.is_empty())
                .unwrap_or(false)
    }

    pub(super) fn current_line_is_empty(&self) -> bool {
        self.current_line.text.is_empty()
    }

    pub(super) fn current_line_ends_with_space(&self) -> bool {
        self.current_line.text.ends_with(' ')
    }

    pub(super) fn prev_token_kind(&self) -> Option<SyntaxKind> {
        self.prev_token_kind
    }

    pub(super) fn set_prev_token_kind(&mut self, kind: Option<SyntaxKind>) {
        self.prev_token_kind = kind;
    }

    pub(super) fn last_significant_token_kind(&self) -> Option<SyntaxKind> {
        self.last_significant_token_kind
    }

    pub(super) fn set_last_significant_token_kind(&mut self, kind: Option<SyntaxKind>) {
        self.last_significant_token_kind = kind;
    }

    pub(super) fn last_line_break(&self) -> Option<LineBreakSource> {
        self.last_line_break
    }

    pub(super) fn at_line_start(&self) -> bool {
        self.at_line_start
    }

    pub(super) fn set_at_line_start(&mut self, value: bool) {
        self.at_line_start = value;
    }

    pub(super) fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    pub(super) fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    pub(super) fn indent_level(&self) -> usize {
        self.indent_level
    }

    pub(super) fn indent_string_len(&self) -> usize {
        self.indent_string.len()
    }
}
