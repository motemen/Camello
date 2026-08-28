//! Per-line suppression (`docs/typecheck.md`, "Diagnostics").
//!
//! ```perl
//! my $legacy = $thing->whatever;   ## camello-disable: unknown-method
//! ```
//!
//! A comment, so `camello format` keeps it — the formatter preserves comment
//! text byte for byte (the `comments` invariant) and a suppression that a
//! reformat could damage would be worse than none. The `##` form is chosen not
//! to collide with `## no critic`, which reads the same way and means something
//! else.
//!
//! Two placements, and both are the same rule the checker's own fixtures use:
//! a comment sharing a line with code is about that line, and a comment on a
//! line of its own is about the line below it. The second is what a long line
//! needs, and what a diagnostic whose span *is* a comment needs — a
//! `## camello-disable: bad-annotation` cannot sit on the `# Returns:` line it
//! is about without becoming part of it.

use std::collections::{HashMap, HashSet};

use camello_syntax::lang::{SyntaxNode, TokenExt, TokenKind};

use crate::diag::{Code, Diagnostic, LineIndex};

const MARKER: &str = "## camello-disable:";

/// What each line was told to stay quiet about.
#[derive(Debug, Default)]
pub struct Suppressions {
    /// `None` in the value means "every code".
    lines: HashMap<usize, Option<HashSet<Code>>>,
}

impl Suppressions {
    /// Read every `## camello-disable:` in a file.
    #[must_use]
    pub fn of(root: &SyntaxNode, source: &str) -> Self {
        let index = LineIndex::new(source);
        let mut lines: HashMap<usize, Option<HashSet<Code>>> = HashMap::new();
        for token in root
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.token_kind() == TokenKind::COMMENT)
        {
            let text = token.text();
            let Some(position) = text.find(MARKER) else {
                continue;
            };
            let listed = text[position + MARKER.len()..]
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(Code::parse)
                .collect::<Option<HashSet<Code>>>();
            let codes = match listed {
                // A marker with no codes, or one naming something that is not
                // a code, silences the whole line: guessing which code was
                // meant would be worse than taking the user at their word.
                Some(codes) if !codes.is_empty() => Some(codes),
                _ => None,
            };

            let start = usize::from(token.text_range().start());
            let line = index.position(source, start).line;
            // A comment on a line of its own is about the line below it.
            let target = if source[index_of_line_start(source, start)..start]
                .trim()
                .is_empty()
            {
                line + 1
            } else {
                line
            };
            merge(&mut lines, target, codes);
        }
        Suppressions { lines }
    }

    /// Whether this diagnostic was told to stay quiet.
    #[must_use]
    pub fn silences(&self, diagnostic: &Diagnostic, source: &str, index: &LineIndex) -> bool {
        let line = index
            .position(source, usize::from(diagnostic.range.start()))
            .line;
        match self.lines.get(&line) {
            Some(None) => true,
            Some(Some(codes)) => codes.contains(&diagnostic.code),
            None => false,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

fn merge(
    lines: &mut HashMap<usize, Option<HashSet<Code>>>,
    line: usize,
    codes: Option<HashSet<Code>>,
) {
    match lines.entry(line) {
        std::collections::hash_map::Entry::Occupied(mut entry) => match (entry.get_mut(), codes) {
            (None, _) | (_, None) => {
                entry.insert(None);
            }
            (Some(existing), Some(more)) => existing.extend(more),
        },
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(codes);
        }
    }
}

/// Where the line holding `offset` begins.
fn index_of_line_start(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .rfind('\n')
        .map_or(0, |position| position + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Options;

    fn diagnostics(source: &str) -> Vec<Code> {
        crate::check_source(source, &Options::default())
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    #[test]
    fn a_trailing_marker_silences_its_own_line() {
        let source = "use strict;\nmy $unread = 1;  ## camello-disable: unused-variable\n";
        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn an_own_line_marker_silences_the_line_below() {
        let source = "use strict;\n## camello-disable: unused-variable\nmy $unread = 1;\n";
        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn a_marker_names_the_code_it_means() {
        let source = "use strict;\nmy $unread = 1;  ## camello-disable: arity\n";
        assert_eq!(diagnostics(source), vec![Code::UnusedVariable]);
    }

    #[test]
    fn a_marker_with_no_code_silences_the_line() {
        let source = "use strict;\nmy $unread = 1;  ## camello-disable:\n";
        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn several_codes_are_one_marker() {
        let source =
            "use strict;\nmy $x = 1;\nif (1) { my $x = 2; print $x }  ## camello-disable: shadowed-variable, unused-variable\nprint $x;\n";
        assert!(diagnostics(source).is_empty(), "{:?}", diagnostics(source));
    }

    #[test]
    fn no_critic_is_a_different_comment() {
        let source = "use strict;\nmy $unread = 1;  ## no critic\n";
        assert_eq!(diagnostics(source), vec![Code::UnusedVariable]);
    }
}
