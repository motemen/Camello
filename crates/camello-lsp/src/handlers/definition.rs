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

use std::sync::Arc;

use camello_sema::annotate::Dialect;
use camello_sema::decl::SubDecl;
use camello_sema::program::{FileEntry, MethodLookup};
use rowan::{TextRange, TextSize};
use tower_lsp_server::ls_types::{Location, Uri};

use crate::analysis::{Context, Tables};
use crate::document::Document;
use crate::handlers::{target_at, Target};
use crate::position::{Encoding, PositionMap};

/// The open buffers, which a jump into an open file is answered from.
pub struct Buffers<'a> {
    pub open: &'a [Arc<Document>],
    /// What the declaration pass over one of them reads annotations as.
    pub dialect: &'a Dialect,
}

#[must_use]
pub fn definition(
    document: &Document,
    uri: &Uri,
    tables: &Tables,
    context: &Context<'_>,
    offset: TextSize,
    encoding: Encoding,
    buffers: &Buffers<'_>,
) -> Option<Location> {
    let root = document.tree();
    let program = context.program();
    match target_at(&root, tables, offset)? {
        Target::Method(site) => {
            match program.resolve_method_from(&site.class, &site.method, &site.from) {
                MethodLookup::Sub(symbol) => locate(context, symbol, encoding, buffers),
                // An attribute's accessor is written by a framework, not by a
                // file: the honest answer is that there is nowhere to go.
                _ => None,
            }
        }
        Target::Call { name, range } => {
            let file = program.index_of(&document.analysis_path())?;
            let symbol = program.resolve_call(file, u32::from(range.start()), &name)?;
            locate(context, symbol, encoding, buffers)
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
/// The file behind a `SubDecl` is a path in the graph. Where that file is
/// open, the editor shows the buffer, and the line and character are the
/// buffer's: counted from disk they drift by every line added above the sub
/// and not saved. Otherwise they are counted from the file on disk, which is
/// a read per jump, and that is what a jump costs.
fn locate(
    context: &Context<'_>,
    symbol: &SubDecl,
    encoding: Encoding,
    buffers: &Buffers<'_>,
) -> Option<Location> {
    let entry = context.program().file(symbol.file)?;
    let uri = Uri::from_file_path(&entry.path)?;
    if let Some(buffer) = buffers
        .open
        .iter()
        .find(|buffer| buffer.path.as_deref() == Some(entry.path.as_path()))
    {
        let range = in_buffer(entry, symbol, buffer, buffers.dialect)?;
        return Some(Location {
            uri,
            range: buffer.positions.range(range),
        });
    }
    let source = std::fs::read_to_string(&entry.path).ok()?;
    let positions = PositionMap::new(&source, encoding);
    Some(Location {
        uri,
        range: positions.range(symbol.range),
    })
}

/// Where an open buffer itself declares a sub the graph knows.
///
/// Not the graph's range: the graph holds the declarations of whichever
/// version of the buffer last changed what another file can see, and an edit
/// that only moves a sub down a line is not one of those, so the range can be
/// any number of edits old. The declaration pass over the buffer as it is now
/// is one file's worth of work, and it is the pass that put the sub in the
/// graph. A name declared twice is matched by which of its declarations it
/// is; a sub the buffer no longer declares has nowhere to go.
fn in_buffer(
    entry: &FileEntry,
    symbol: &SubDecl,
    buffer: &Document,
    dialect: &Dialect,
) -> Option<TextRange> {
    let same =
        |candidate: &&SubDecl| candidate.package == symbol.package && candidate.name == symbol.name;
    let nth = entry
        .decls
        .subs
        .iter()
        .filter(same)
        .position(|candidate| candidate.range == symbol.range)
        .unwrap_or(0);
    let decls = camello_sema::read_declarations(
        &entry.path,
        &buffer.text,
        dialect,
        &camello_sema::resolve::Cache::disabled(),
    );
    let mut found = decls.subs.iter().filter(same);
    let declared = found.clone().nth(nth).or_else(|| found.next())?;
    Some(declared.range)
}
