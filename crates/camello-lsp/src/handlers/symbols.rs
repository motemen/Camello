//! `textDocument/documentSymbol` from the tree (`docs/lsp.md`, milestone 2).
//!
//! Packages and subs, and nothing else. The ranges are already there — the
//! tree carries where a definition begins and ends, and the `SUB_NAME` node
//! carries what to highlight when the outline is clicked — so this is the one
//! feature that is genuinely free: no new analysis, only a shape the client
//! understands.

use camello_syntax::ast::{AstNode, SubDef};
use camello_syntax::lang::{NodeExt, NodeKind};
use rowan::TextRange;
use tower_lsp_server::ls_types::{DocumentSymbol, SymbolKind};

use crate::document::Document;

/// The symbols of one document: packages, holding the subs written under
/// them.
///
/// A sub written before any `package` statement belongs to `main`, and `main`
/// is not a heading anybody wants to fold, so those stay at the top level.
#[must_use]
pub fn symbols(document: &Document) -> Vec<DocumentSymbol> {
    let root = document.tree();
    let packages = camello_sema::decl::package_spans(&root);
    let subs: Vec<(TextRange, TextRange, String)> = root
        .descendants()
        .filter(|node| node.node_kind() == NodeKind::SUB_DEF)
        .filter_map(|node| {
            let view = SubDef::cast(node.clone())?;
            let name = view.name()?;
            Some((
                node.text_range(),
                name.syntax().text_range(),
                view.name_text()?,
            ))
        })
        .collect();
    // A sub belongs to the innermost package around it: `package Foo { ... }`
    // inside `package Outer;` holds its own subs, and Outer the rest.
    let owner = |range: &TextRange| {
        packages
            .iter()
            .enumerate()
            .filter(|(_, span)| {
                span.start <= u32::from(range.start()) && u32::from(range.end()) <= span.end
            })
            .max_by_key(|(_, span)| span.start)
            .map(|(index, _)| index)
    };
    let function = |(range, selection, name): &(TextRange, TextRange, String)| {
        build(
            document,
            name.clone(),
            SymbolKind::FUNCTION,
            *range,
            *selection,
            None,
        )
    };

    let mut out: Vec<DocumentSymbol> = subs
        .iter()
        .filter(|(range, ..)| owner(range).is_none())
        .map(function)
        .collect();
    for (index, span) in packages.iter().enumerate() {
        let children: Vec<DocumentSymbol> = subs
            .iter()
            .filter(|(range, ..)| owner(range) == Some(index))
            .map(function)
            .collect();
        out.push(build(
            document,
            span.name.clone(),
            SymbolKind::MODULE,
            TextRange::new(span.start.into(), span.end.into()),
            span.statement,
            (!children.is_empty()).then_some(children),
        ));
    }
    out.sort_by_key(|symbol| (symbol.range.start.line, symbol.range.start.character));
    out
}

/// `DocumentSymbol` still carries a deprecated field that has to be named to
/// build one; the `allow` is confined here.
#[allow(deprecated)]
fn build(
    document: &Document,
    name: String,
    kind: SymbolKind,
    range: TextRange,
    selection: TextRange,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range: document.positions.range(range),
        selection_range: document.positions.range(selection),
        children,
    }
}
