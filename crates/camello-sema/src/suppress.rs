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
//!
//! `## no critic` is read too (`docs/lint.md`, LINT-OFF), for the code a
//! project carries over from Perl::Critic. It reaches only the lint rules, by
//! their Perl::Critic names, and its own-line form opens a region rather than
//! naming the line below — the same comment means what perlcritic takes it to
//! mean, or a project moving over would have to rewrite every one.

use std::collections::{HashMap, HashSet};

use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode, SyntaxToken, TokenExt, TokenKind};

use crate::diag::{Code, Diagnostic, LineIndex};

const MARKER: &str = "## camello-disable:";

/// What each line was told to stay quiet about.
#[derive(Debug, Default)]
pub struct Suppressions {
    /// `None` in the value means "every code".
    lines: HashMap<usize, Option<HashSet<Code>>>,
    /// `## no critic` on a line of code: that line. `None` is every lint rule.
    critic_lines: HashMap<usize, Option<HashSet<Code>>>,
    /// `## no critic` on a line of its own: the lines from it to the end of
    /// its region, inclusive.
    critic_regions: Vec<(usize, usize, Option<HashSet<Code>>)>,
}

impl Suppressions {
    /// Read every `## camello-disable:` in a file.
    #[must_use]
    pub fn of(root: &SyntaxNode, source: &str) -> Self {
        let index = LineIndex::new(source);
        let mut lines: HashMap<usize, Option<HashSet<Code>>> = HashMap::new();
        let mut critic = Critic::default();
        for token in root
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.token_kind() == TokenKind::COMMENT)
        {
            let text = token.text();
            critic.read(&token, source, &index);
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
        let last_line = index.position(source, source.len()).line;
        let critic_lines = std::mem::take(&mut critic.lines);
        Suppressions {
            lines,
            critic_lines,
            critic_regions: critic.regions(last_line),
        }
    }

    /// Whether this diagnostic was told to stay quiet.
    #[must_use]
    pub fn silences(&self, diagnostic: &Diagnostic, source: &str, index: &LineIndex) -> bool {
        let line = index
            .position(source, usize::from(diagnostic.range.start()))
            .line;
        let named = |codes: &Option<HashSet<Code>>| {
            codes
                .as_ref()
                .is_none_or(|codes| codes.contains(&diagnostic.code))
        };
        if self.lines.get(&line).is_some_and(named) {
            return true;
        }
        if !diagnostic.code.is_lint() {
            return false;
        }
        self.critic_lines.get(&line).is_some_and(named)
            || self
                .critic_regions
                .iter()
                .any(|(start, end, codes)| (*start..=*end).contains(&line) && named(codes))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.critic_lines.is_empty() && self.critic_regions.is_empty()
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

/// The `## no critic` and `## use critic` comments of a file, as read.
#[derive(Default)]
struct Critic {
    lines: HashMap<usize, Option<HashSet<Code>>>,
    /// An own-line `## no critic`: its line, the last line of the block it is
    /// in (`None` at file level), and what it names.
    opened: Vec<(usize, Option<usize>, Option<HashSet<Code>>)>,
    /// The lines of own-line `## use critic`s.
    closed: Vec<usize>,
}

impl Critic {
    fn read(&mut self, token: &SyntaxToken, source: &str, index: &LineIndex) {
        let Some(annotation) = Annotation::parse(token.text()) else {
            return;
        };
        let start = usize::from(token.text_range().start());
        let line = index.position(source, start).line;
        let own_line = source[index_of_line_start(source, start)..start]
            .trim()
            .is_empty();
        match annotation {
            Annotation::Use => {
                if own_line {
                    self.closed.push(line);
                }
            }
            Annotation::No(codes) if own_line => {
                let block_end = token
                    .parent_ancestors()
                    .find(|node| node.node_kind() == NodeKind::BLOCK)
                    .map(|block| {
                        index
                            .position(source, usize::from(block.text_range().end()))
                            .line
                    });
                self.opened.push((line, block_end, codes));
            }
            Annotation::No(codes) => merge(&mut self.lines, line, codes),
        }
    }

    /// Each region runs to the nearest of a `## use critic`, the end of its
    /// block, and the end of the file (LINT-OFF-4).
    fn regions(self, last_line: usize) -> Vec<(usize, usize, Option<HashSet<Code>>)> {
        self.opened
            .into_iter()
            .map(|(start, block_end, codes)| {
                let limit = block_end.unwrap_or(last_line);
                let end = self
                    .closed
                    .iter()
                    .copied()
                    .filter(|&closed| closed > start && closed <= limit)
                    .min()
                    .unwrap_or(limit);
                (start, end, codes)
            })
            .collect()
    }
}

enum Annotation {
    /// `## no critic`, with the lint rules it names; `None` names them all.
    No(Option<HashSet<Code>>),
    /// `## use critic`.
    Use,
}

impl Annotation {
    fn parse(comment: &str) -> Option<Self> {
        let rest = comment.strip_prefix("##")?.trim_start();
        let (word, rest) = rest.split_once(char::is_whitespace)?;
        let rest = rest.trim_start();
        let rest = rest.strip_prefix("critic")?;
        if !rest.is_empty() && !rest.starts_with(|ch: char| !ch.is_alphanumeric()) {
            return None;
        }
        match word {
            "use" => Some(Annotation::Use),
            "no" => Some(Annotation::No(Annotation::named(rest.trim()))),
            _ => None,
        }
    }

    /// The rules a list names (LINT-OFF-1), or `None` where there is no list.
    fn named(list: &str) -> Option<HashSet<Code>> {
        let list = list.strip_prefix("qw").unwrap_or(list).trim_start();
        let close = match list.chars().next()? {
            '(' => ')',
            '{' => '}',
            '[' => ']',
            '<' => '>',
            '"' => '"',
            '\'' => '\'',
            _ => return None,
        };
        let inner = &list[1..];
        let inner = inner.split(close).next().unwrap_or(inner);
        let names: Vec<String> = inner
            .split(|ch: char| ch == ',' || ch.is_whitespace())
            .filter(|name| !name.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
        Some(
            Code::ALL
                .iter()
                .copied()
                .filter(|code| {
                    code.critic_policy()
                        .is_some_and(|policy| names.iter().any(|name| names_policy(name, policy)))
                })
                .collect(),
        )
    }
}

/// Whether a name in a `## no critic` list is about this policy: a substring
/// of its full name, ignoring case, or — for a name with a category — of its
/// last part matched against the policy's last part.
fn names_policy(name: &str, policy: &str) -> bool {
    let full = format!("perl::critic::policy::{}", policy.to_ascii_lowercase());
    if full.contains(name) {
        return true;
    }
    match (name.rsplit_once("::"), policy.rsplit_once("::")) {
        (Some((_, last)), Some((_, policy_last))) => {
            !last.is_empty() && policy_last.to_ascii_lowercase().contains(last)
        }
        _ => false,
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
        let source =
            "use strict;\nuse warnings;\nmy $unread = 1;  ## camello-disable: unused-variable\n";
        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn an_own_line_marker_silences_the_line_below() {
        let source =
            "use strict;\nuse warnings;\n## camello-disable: unused-variable\nmy $unread = 1;\n";
        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn a_marker_names_the_code_it_means() {
        let source = "use strict;\nuse warnings;\nmy $unread = 1;  ## camello-disable: arity\n";
        assert_eq!(diagnostics(source), vec![Code::UnusedVariable]);
    }

    #[test]
    fn a_marker_with_no_code_silences_the_line() {
        let source = "use strict;\nuse warnings;\nmy $unread = 1;  ## camello-disable:\n";
        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn several_codes_are_one_marker() {
        let source =
            "use strict;\nuse warnings;\nmy $x = 1;\nif (1) { my $x = 2; print $x }  ## camello-disable: shadowed-variable, unused-variable\nprint $x;\n";
        assert!(diagnostics(source).is_empty(), "{:?}", diagnostics(source));
    }

    #[test]
    fn no_critic_is_a_different_comment() {
        let source = "use strict;\nuse warnings;\nmy $unread = 1;  ## no critic\n";
        assert_eq!(diagnostics(source), vec![Code::UnusedVariable]);
    }
}
