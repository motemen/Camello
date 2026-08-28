//! `textDocument/documentSymbol` from the tree (`docs/lsp.md`, milestone 2).
//!
//! Packages and subs, and nothing else. The ranges are already there — the
//! tree carries where a definition begins and ends, and the `SUB_NAME` node
//! carries what to highlight when the outline is clicked — so this is the one
//! feature that is genuinely free: no new analysis, only a shape the client
//! understands.

use camello_syntax::ast::{AstNode, PackageStmt, SubDef};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode};
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
    let packages = package_extents(&root);
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

    let mut out: Vec<DocumentSymbol> = subs
        .iter()
        .filter(|(range, _, _)| {
            !packages
                .iter()
                .any(|(extent, ..)| extent.contains_range(*range))
        })
        .map(|(range, selection, name)| {
            build(
                document,
                name.clone(),
                SymbolKind::FUNCTION,
                *range,
                *selection,
                None,
            )
        })
        .collect();

    for (extent, header, name) in &packages {
        let children: Vec<DocumentSymbol> = subs
            .iter()
            .filter(|(range, _, _)| extent.contains_range(*range))
            .map(|(range, selection, sub)| {
                build(
                    document,
                    sub.clone(),
                    SymbolKind::FUNCTION,
                    *range,
                    *selection,
                    None,
                )
            })
            .collect();
        out.push(build(
            document,
            name.clone(),
            SymbolKind::MODULE,
            *extent,
            *header,
            (!children.is_empty()).then_some(children),
        ));
    }
    out.sort_by_key(|symbol| (symbol.range.start.line, symbol.range.start.character));
    out
}

/// Each package statement: how far its declarations reach, where its own name
/// is, and what it is called.
///
/// `package Foo { ... }` says how far itself. `package Foo;` runs to the next
/// package statement or to the end of the file, which is the rule
/// `FileDecls::package_at` already reads offsets under.
fn package_extents(root: &SyntaxNode) -> Vec<(TextRange, TextRange, String)> {
    let statements: Vec<SyntaxNode> = root
        .descendants()
        .filter(|node| node.node_kind() == NodeKind::PACKAGE_STMT)
        .collect();
    let mut out = Vec::new();
    for (index, node) in statements.iter().enumerate() {
        let Some(view) = PackageStmt::cast(node.clone()) else {
            continue;
        };
        let Some(name) = view.name() else { continue };
        let extent = if view.block().is_some() {
            node.text_range()
        } else {
            let end = statements
                .get(index + 1)
                .map_or(root.text_range().end(), |next| next.text_range().start());
            TextRange::new(node.text_range().start(), end)
        };
        out.push((extent, node.text_range(), name));
    }
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
