//! Vertical alignment (ADR 0008 §5).
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
use crate::fmt::render::Line;
use crate::fmt::FormatterOptions;

/// The order classes are aligned in. Earlier classes shift the columns of later
/// ones, so an assignment settles before the comment that follows it on the same
/// line.
fn class_order(class: AnchorClass) -> (u8, u8) {
    match class {
        AnchorClass::Assign => (0, 0),
        AnchorClass::FatComma(depth) => (1, depth),
        AnchorClass::PostfixKeyword => (2, 0),
        AnchorClass::TrailingComment => (3, 0),
    }
}

pub fn align(lines: &mut [Line], options: &FormatterOptions) {
    let mut classes: Vec<AnchorClass> = lines
        .iter()
        .flat_map(|line| line.anchors.iter().map(|(class, _)| *class))
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
        // ends it, as does a blank line (formatting.md §7).
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
            let target = (start..end)
                .filter_map(|index| column_of(&lines[index], class))
                .max()
                .unwrap_or(first);
            for line in &mut lines[start..end] {
                pad_to(line, class, target, options);
            }
        }

        start = end;
    }
}

fn column_of(line: &Line, class: AnchorClass) -> Option<usize> {
    line.anchors
        .iter()
        .find(|(candidate, _)| *candidate == class)
        .map(|(_, column)| *column)
}

/// Insert spaces so that `class`'s anchor sits at `target`.
///
/// Padding is only ever spaces, so it creates no new anchor and the pass is its
/// own fixed point (ADR 0008 §6, I3).
fn pad_to(line: &mut Line, class: AnchorClass, target: usize, options: &FormatterOptions) {
    let Some(column) = column_of(line, class) else {
        return;
    };
    let padding = target
        .saturating_sub(column)
        .min(options.max_alignment_padding);
    if padding == 0 {
        return;
    }

    let byte_index = line
        .text
        .char_indices()
        .nth(column)
        .map_or(line.text.len(), |(index, _)| index);
    line.text.insert_str(byte_index, &" ".repeat(padding));

    // Everything at or after the insertion point moved right.
    for (_, other) in &mut line.anchors {
        if *other >= column {
            *other += padding;
        }
    }
}
