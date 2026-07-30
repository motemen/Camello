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
    /// How far apart a group's anchors may be and still be aligned, so that one
    /// very long line cannot push a whole group across the screen (issue #273).
    ///
    /// A group either agrees on one column or is not aligned at all: capping
    /// each line's own padding instead gave a group whose members ended up in
    /// three different columns, which is neither (formatting.md §7).
    pub max_alignment_padding: usize,
    /// Whether a one-statement `map`/`sub`/`do` block may stay on one line.
    pub allow_single_line_blocks: bool,
    /// Space inside flat `[...]` and `{...}` literals (formatting.md SPACING-7).
    /// Parentheses are always tight; blocks always get one space; this only
    /// concerns anonymous array/hash constructors.
    pub delimiter_spacing: DelimiterSpacing,
}

/// How the inside of a flat `[...]` / `{...}` literal is padded.
///
/// Decided at build time as part of the spacing rules — a `Doc::Space` is or is
/// not placed next to the bracket — never in the renderer (ADR 0008 §2). This
/// is the reintroduction the deviation log's L-006 anticipated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelimiterSpacing {
    /// Never a space: `[1, 2]`, `{ a => 1 }` becomes `{a => 1}`.
    Tight,
    /// A space when the literal holds two or more items: `[ 1, 2 ]` but `[$x]`.
    /// A lone `key => value` pair counts as two items, matching SPACING-7's
    /// `my $h = { key => 'val' };`.
    Standard,
    /// Always a space when non-empty: `[ $x ]`.
    Loose,
}

impl Default for FormatterOptions {
    fn default() -> Self {
        Self {
            indent_width: 4,
            min_spaces_before_comment: 4,
            max_alignment_padding: 64,
            allow_single_line_blocks: true,
            delimiter_spacing: DelimiterSpacing::Standard,
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

/// The flat-or-broken decision of every group in `source`, in document order.
///
/// Layout is decided once, at build time, from the source (ADR 0008 §3), so
/// these are the seeds a second pass would have to reproduce for the output to
/// be stable (ADR 0008 §6 I2).
#[must_use]
pub fn layout_seeds(source: &str) -> Vec<bool> {
    fn collect(document: &doc::Doc, into: &mut Vec<bool>) {
        match document {
            doc::Doc::Group { broken, body } => {
                into.push(*broken);
                collect(body, into);
            }
            doc::Doc::Concat(parts) => parts.iter().for_each(|part| collect(part, into)),
            doc::Doc::Indent(body) | doc::Doc::Continuation(body) => collect(body, into),
            _ => {}
        }
    }

    let parsed = crate::parse::parse(source);
    let options = FormatterOptions::default();
    let document = build::Builder::new(&parsed.trivia, &options).file(&parsed.syntax());
    let mut seeds = Vec::new();
    collect(&document, &mut seeds);
    seeds
}
