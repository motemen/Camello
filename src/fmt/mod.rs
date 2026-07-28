//! The formatter (ADR 0008).
//!
//! ```text
//! CST + TriviaMap
//!   → [build]  Doc IR     layout decided here, once
//!   → [render] Vec<Line>  spacing and indentation applied
//!   → [align]  String     vertical alignment padding
//! ```
//!
//! Splitting it this way is what makes the invariants of ADR 0008 §6 hold by
//! construction rather than by care: verbatim content cannot be indented into
//! because the renderer never touches a `Raw` atom, and alignment cannot depend
//! on the source because the align pass only ever sees rendered columns.

mod align;
mod build;
pub mod doc;
mod render;

#[cfg(test)]
mod tests;

use crate::lang::SyntaxNode;
use crate::parse::trivia::TriviaMap;

#[derive(Debug, Clone)]
pub struct FormatterOptions {
    pub indent_width: usize,
    /// Minimum spaces between code and a trailing comment. One rule, applied in
    /// one place (ADR 0008 §4).
    pub min_spaces_before_comment: usize,
    /// Cap on alignment padding, so one very long line cannot push a whole
    /// group across the screen (issue #273).
    pub max_alignment_padding: usize,
    /// Whether a one-statement `map`/`sub`/`do` block may stay on one line.
    pub allow_single_line_blocks: bool,
}

impl Default for FormatterOptions {
    fn default() -> Self {
        Self {
            indent_width: 4,
            min_spaces_before_comment: 1,
            max_alignment_padding: 40,
            allow_single_line_blocks: true,
        }
    }
}

/// Format a parsed tree.
#[must_use]
pub fn format(root: &SyntaxNode, trivia: &TriviaMap, options: &FormatterOptions) -> String {
    let document = build::Builder::new(trivia, options).file(root);
    let mut lines = render::Renderer::new(options).render(&document);
    align::align(&mut lines, options);

    let mut out = String::new();
    for line in &lines {
        out.push_str(&line.text);
        out.push('\n');
    }
    out
}

/// Parse and format in one step.
#[must_use]
pub fn format_source(source: &str, options: &FormatterOptions) -> String {
    let parsed = crate::parse::parse(source);
    format(&parsed.syntax(), &parsed.trivia, options)
}
