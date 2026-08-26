//! The formatter.
//!
//! ```text
//! CST + TriviaMap
//!   → [build]  Doc IR     layout decided here, once
//!   → [render] Vec<Line>  spacing and indentation applied
//!   → [align]  String     vertical alignment padding
//! ```
//!
//! Splitting it this way is what makes the invariants of the formatter contract hold by
//! construction rather than by care: verbatim content cannot be indented into
//! because the renderer never touches a `Raw` atom, and alignment cannot depend
//! on the source because the align pass only ever sees rendered columns.

mod align;
mod build;
pub mod doc;
mod render;
mod skip;

#[cfg(test)]
mod tests;

use crate::lang::{SyntaxNode, TokenExt, TokenKind};
use crate::parse::trivia::TriviaMap;

#[derive(Debug, Clone)]
pub struct FormatterOptions {
    pub indent_width: usize,
    /// Minimum spaces between code and a trailing comment. One rule, applied in
    /// one place (the formatter contract).
    pub min_spaces_before_comment: usize,
    /// How far apart a group's anchors may be and still be aligned, so that one
    /// very long line cannot push a whole group across the screen (issue #273).
    ///
    /// A group either agrees on one column or is not aligned at all: capping
    /// each line's own padding instead gave a group whose members ended up in
    /// three different columns, which is neither (docs/formatting.md §7).
    pub max_alignment_padding: usize,
    /// Whether a one-statement `map`/`sub`/`do` block may stay on one line.
    pub allow_single_line_blocks: bool,
    /// Space inside flat `[...]` and `{...}` literals (docs/formatting.md SPACING-7).
    /// Parentheses are always tight; blocks always get one space; this only
    /// concerns anonymous array/hash constructors.
    pub delimiter_spacing: DelimiterSpacing,
    /// Whether a run of `use` — or of `no` — lines up its import lists
    /// (docs/formatting.md ALIGNMENT-2).
    ///
    /// Off. A `use` block is a table by the same reading a hash is, so the rule
    /// fits; but it is the single largest diff a repository adopting camello
    /// sees — twelve thousand lines across three thousand files in the one this
    /// was measured on — and which way that block should read is not settled
    /// enough to be the answer everybody gets.
    pub align_use_imports: bool,
}

/// How the inside of a flat `[...]` / `{...}` literal is padded.
///
/// Decided at build time as part of the spacing rules — a `Doc::Space` is or is
/// not placed next to the bracket — never in the renderer (the formatter contract). This
/// concerns anonymous array/hash constructors only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelimiterSpacing {
    /// Never a space: `[1, 2]`, `{ a => 1 }` becomes `{a => 1}`.
    Tight,
    /// A space unless the literal holds a single simple term — one word to the
    /// eye: `[ 1, 2 ]` and `[ foo($body) ]`, but `[$x]`, `[-1]`, `[@$pair]`.
    /// A lone `key => value` pair is not one term, matching SPACING-7's
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
            align_use_imports: false,
        }
    }
}

/// Format a parsed tree.
#[must_use]
pub fn format(root: &SyntaxNode, trivia: &TriviaMap, options: &FormatterOptions) -> String {
    let document = build::Builder::new(trivia, options).file(root);
    let mut lines = render::Renderer::new(options).render(&document);
    align::align(&mut lines, options);
    // Last, and over the rendered lines: a `#<<<` region is a run of lines the
    // writer settled, so what it replaces is the formatter's whole answer for
    // them — indentation, spacing and alignment together.
    let regions = skip::regions(root);
    if !regions.is_empty() {
        skip::restore(&mut lines, &root.text().to_string(), &regions);
    }

    let mut out = String::new();
    for line in &lines {
        out.push_str(&line.text);
        out.push('\n');
    }
    // A file whose last content is verbatim ends exactly as that content does.
    // `__END__` followed by POD that stops at `=cut` with no newline after it is
    // XML::Parser::Style::Subs, and the newline this loop adds would land inside
    // a token the formatter must reproduce byte for byte (the formatter contract, I1).
    if ends_in_unterminated_verbatim(root) {
        out.pop();
    }
    out
}

/// Does the file's last content end without the line terminator it was written
/// without?
fn ends_in_unterminated_verbatim(root: &SyntaxNode) -> bool {
    // From the end, not by walking the file: this is asked once per format, and
    // the answer is in the last token or two.
    let mut token = root.last_token();
    while let Some(current) = token {
        if current.token_kind() != TokenKind::WHITESPACE {
            return current.token_kind().is_verbatim() && !current.text().ends_with('\n');
        }
        token = current.prev_token();
    }
    false
}

/// Parse and format in one step.
#[must_use]
pub fn format_source(source: &str, options: &FormatterOptions) -> String {
    let parsed = crate::parse::parse(source);
    format(&parsed.syntax(), &parsed.trivia, options)
}

/// The flat-or-broken decision of every group in `source`, in document order.
///
/// Layout is decided once, at build time, from the source (the formatter contract), so
/// these are the seeds a second pass would have to reproduce for the output to
/// be stable (the formatter contract I2).
#[must_use]
pub fn layout_seeds(source: &str) -> Vec<bool> {
    fn collect(document: &doc::Doc, into: &mut Vec<bool>) {
        match document {
            doc::Doc::Group { broken, body, .. } => {
                into.push(*broken);
                collect(body, into);
            }
            doc::Doc::Concat(parts) => parts.iter().for_each(|part| collect(part, into)),
            doc::Doc::Indent(body)
            | doc::Doc::Continuation(body)
            | doc::Doc::Rooted(body)
            | doc::Doc::Hanging { body, .. } => collect(body, into),
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
