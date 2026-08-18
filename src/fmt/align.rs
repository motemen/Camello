//! Vertical alignment (the formatter contract).
//!
//! An independent pass over rendered lines, in the manner of perltidy's vertical
//! aligner. It sees columns and nothing else — not the source's whitespace, not
//! the source's newlines — which is what makes it idempotent and what removes
//! the whole class of bug where a comment or a nested list broke alignment.
//!
//! It is also linear. The old formatter measured widths by re-running itself,
//! which made a file of aligned assignments quadratic: 800 lines took 30
//! seconds.

use crate::fmt::doc::AnchorClass;
use crate::fmt::render::{Anchor, Line};
use crate::fmt::FormatterOptions;

/// The order classes are aligned in. Earlier classes shift the columns of later
/// ones, so an assignment settles before the comment that follows it on the same
/// line.
fn class_order(class: AnchorClass) -> (u8, u8) {
    match class {
        AnchorClass::Assign => (0, 0),
        AnchorClass::FatComma(depth) => (1, depth),
        AnchorClass::Fallback => (2, 0),
        AnchorClass::PostfixKeyword => (3, 0),
        AnchorClass::TrailingComment => (4, 0),
    }
}

pub fn align(lines: &mut [Line], options: &FormatterOptions) {
    let mut classes: Vec<AnchorClass> = lines
        .iter()
        .flat_map(|line| line.anchors.iter().map(|anchor| anchor.class))
        .collect();
    classes.sort_by_key(|class| class_order(*class));
    classes.dedup();

    for class in classes {
        align_class(lines, class, options);
    }
}

fn align_class(lines: &mut [Line], class: AnchorClass, options: &FormatterOptions) {
    let mut start = 0;
    while start < lines.len() {
        let Some(first) = column_of(&lines[start], class) else {
            start += 1;
            continue;
        };

        // A group runs while consecutive lines carry the same anchor class at
        // the same nesting, with the same statement shape. Any of those changing
        // ends it, as does a blank line (docs/formatting.md §7).
        let mut end = start + 1;
        while end < lines.len()
            && !lines[end].is_blank()
            && column_of(&lines[end], class).is_some()
            && lines[end].shape == lines[start].shape
            && lines[end].indent == lines[start].indent
        {
            end += 1;
        }

        if end - start >= 2 {
            let columns = || (start..end).filter_map(|index| column_of(&lines[index], class));
            let target = columns().max().unwrap_or(first);
            let narrowest = columns().min().unwrap_or(first);
            // The cap is a property of the group, not of each line in it: one
            // very long member should stop the group being aligned, never leave
            // it half aligned. Applying it per line gave a group in Test2's
            // `object { ... }` style three different columns — an output that is
            // aligned by no measure and stable under re-formatting, so nothing
            // ever took it back.
            if target - narrowest <= options.max_alignment_padding {
                for line in &mut lines[start..end] {
                    pad_to(line, class, target);
                }
            }
        }

        start = end;
    }
}

fn anchor_of(line: &Line, class: AnchorClass) -> Option<Anchor> {
    line.anchors
        .iter()
        .find(|anchor| anchor.class == class)
        .copied()
}

/// The column the group has to agree on for this line.
///
/// The end of the anchored operator, not its start: `=` and `-=` line up on
/// their `=`, so what the group agrees on is where the operator *finishes*
/// (docs/formatting.md ALIGNMENT-2). For a class whose anchor has no width — `=>`, a
/// trailing comment — the two are the same column.
fn column_of(line: &Line, class: AnchorClass) -> Option<usize> {
    anchor_of(line, class).map(|anchor| anchor.column + anchor.tail)
}

/// Insert spaces so that `class`'s anchor sits at `target`.
///
/// Padding is only ever spaces, so it creates no new anchor and the pass is its
/// own fixed point (the formatter contract, I3).
fn pad_to(line: &mut Line, class: AnchorClass, target: usize) {
    let Some(anchor) = anchor_of(line, class) else {
        return;
    };
    let padding = target.saturating_sub(anchor.column + anchor.tail);
    if padding == 0 {
        return;
    }

    line.text.insert_str(anchor.byte, &" ".repeat(padding));

    // Everything at or after the insertion point moved right. A space is one
    // byte and one column, so the two shift by the same amount.
    for other in &mut line.anchors {
        if other.byte >= anchor.byte {
            other.byte += padding;
            other.column += padding;
        }
    }
}
