//! `textDocument/definition` (`docs/lsp.md`, milestone 3 and milestone 6).
//!
//! Nearly free once a `Program` exists to ask: `resolve_call` and
//! `resolve_method_from` are the checker's own lookups, and a `SubDecl` has
//! carried where its name is written since the declaration pass was built.
//! What is new is only that somebody asks.
//!
//! Rename and find-references are deliberately not here. The resolution table
//! this reads is the same one they would need, and the design's rule is that
//! it is exercised read-only first (`docs/lsp.md`, non-goals).

use camello_sema::program::MethodLookup;
use rowan::TextSize;
use tower_lsp_server::ls_types::{Location, Uri};

use crate::analysis::{Context, Tables};
use crate::document::Document;
use crate::handlers::{target_at, Target};
use crate::position::{Encoding, PositionMap};

#[must_use]
pub fn definition(
    document: &Document,
    uri: &Uri,
    tables: &Tables,
    context: &Context<'_>,
    offset: TextSize,
    encoding: Encoding,
) -> Option<Location> {
    let root = document.tree();
    let program = context.program();
    match target_at(&root, tables, offset)? {
        Target::Method(site) => {
            match program.resolve_method_from(&site.class, &site.method, &site.from) {
                MethodLookup::Sub(symbol) => locate(context, symbol, encoding),
                // An attribute's accessor is written by a framework, not by a
                // file: the honest answer is that there is nowhere to go.
                _ => None,
            }
        }
        Target::Call { name, range } => {
            let file = program.index_of(&document.analysis_path())?;
            let symbol = program.resolve_call(file, u32::from(range.start()), &name)?;
            locate(context, symbol, encoding)
        }
        // A sub's name *is* its definition; jumping to it from itself is what
        // an editor does when it has nowhere better, so answer with nothing.
        // Nothing resolved the receiver, so there is no class to look in.
        Target::Definition { .. } | Target::UnresolvedMethod { .. } => None,
        Target::Lexical { binding, .. } => {
            let found = tables.scope.bindings.get(binding)?;
            if found.range.is_empty() {
                // `$_`, `@ARGV` — bound by perl and written down nowhere.
                return None;
            }
            Some(Location {
                uri: uri.clone(),
                range: document.positions.range(found.range),
            })
        }
    }
}

/// Where a declaration was written, as a client can open it.
///
/// The file behind a `SubDecl` is a path in the graph, which is usually not
/// the open buffer — so its line and character have to be counted from the
/// file on disk. That is a read per jump, which is what a jump costs.
fn locate(
    context: &Context<'_>,
    symbol: &camello_sema::decl::SubDecl,
    encoding: Encoding,
) -> Option<Location> {
    let entry = context.program().file(symbol.file)?;
    let uri = Uri::from_file_path(&entry.path)?;
    let source = std::fs::read_to_string(&entry.path).ok()?;
    let positions = PositionMap::new(&source, encoding);
    Some(Location {
        uri,
        range: positions.range(symbol.range),
    })
}
