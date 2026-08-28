//! Whole-file formatting (`docs/lsp.md`, "Formatting").
//!
//! `camello_fmt::format` over the stored tree, returned as one
//! whole-document edit. Minimal-diff splitting is cosmetic — the idempotency
//! invariant means a second application changes nothing — and can come later
//! if cursor-jumping annoys.
//!
//! A file whose parse has errors is not formatted, and the request answers
//! `null`. That is the CLI's rule, not a new one: `camello format` leaves
//! alone what it cannot fully parse, however it was asked, and an editor
//! rewriting a half-typed file on save would be the one place camello did
//! otherwise.

use tower_lsp_server::ls_types::TextEdit;

use crate::state::Snapshot;

#[must_use]
pub fn formatting(snapshot: &Snapshot) -> Option<Vec<TextEdit>> {
    let document = &snapshot.document;
    if !document.parsed_cleanly() {
        return None;
    }
    let formatted = camello_fmt::format(
        &document.tree(),
        &document.trivia,
        &snapshot.settings.formatter,
    );
    if formatted == *document.text {
        // An empty edit list, not `null`: the file is formatted, which is a
        // different answer from "I will not format it".
        return Some(Vec::new());
    }
    Some(vec![TextEdit {
        range: document.positions.whole(),
        new_text: formatted,
    }])
}
