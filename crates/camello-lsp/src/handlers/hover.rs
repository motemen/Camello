//! `textDocument/hover` (`docs/lsp.md`, "Hover").
//!
//! On a typed expression or a binding: the inferred type, spelled the way
//! `docs/types.md` spells types. On a sub name — a definition or a call: the
//! signature the declaration pass read.
//!
//! Where the cursor is on something nameable and the checker has no answer,
//! hover says `Unknown` rather than nothing. Silence is the right answer to
//! "there is nothing here" and the wrong one to "there is something here and I
//! do not know what it is": the two are indistinguishable to a reader, and the
//! second is the common case. So a lexical, a call, a sub name and a `->`
//! method all answer — with `Unknown` where that is the answer — and only a
//! position that names nothing at all stays silent.

use camello_sema::program::MethodLookup;
use camello_syntax::lang::SyntaxNode;
use rowan::{TextRange, TextSize};
use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind};

use crate::analysis::{Context, Tables};
use crate::document::Document;
use crate::handlers::{target_at, Target};

#[must_use]
pub fn hover(
    document: &Document,
    tables: &Tables,
    context: &Context<'_>,
    offset: TextSize,
) -> Option<Hover> {
    let root = document.tree();
    let (text, range) = describe(&root, &document.analysis_path(), tables, context, offset)?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```perl\n{text}\n```"),
        }),
        range: Some(document.positions.range(range)),
    })
}

/// What to show, and what to underline while showing it.
fn describe(
    root: &SyntaxNode,
    path: &std::path::Path,
    tables: &Tables,
    context: &Context<'_>,
    offset: TextSize,
) -> Option<(String, TextRange)> {
    let program = context.program();
    match target_at(root, tables, offset) {
        Some(Target::Method(site)) => {
            let text = match program.resolve_method_from(&site.class, &site.method, &site.from) {
                MethodLookup::Sub(symbol) => camello_sema::decl::signature_of(symbol),
                MethodLookup::Attribute(attribute) => format!(
                    "{}::{} -> {}",
                    site.class,
                    site.method,
                    attribute.returns(&site.method)
                ),
                MethodLookup::Constructor => format!("{}::new(%args)", site.class),
                // `isa`, `can`, `DOES` — there is a signature, but it is
                // perl's and not this program's, and saying so teaches
                // nothing about the code under the cursor.
                MethodLookup::Universal => return None,
                // The cursor is on a method name and the checker cannot say
                // what it is — because an ancestor was never read, or because
                // nothing declares it. Named, so the reader knows which of
                // the two questions went unanswered.
                MethodLookup::Unknown | MethodLookup::Missing => {
                    format!("{}::{} -> Unknown", site.class, site.method)
                }
            };
            Some((text, site.method_range))
        }
        Some(Target::Definition {
            package,
            name,
            range,
        }) => {
            // The cursor is on the `sub` keyword's own name, so the
            // declaration wanted is this file's and not a namesake another
            // file in the workspace declares (`Program::sub_in`). A buffer the
            // graph does not hold has only the global answer, which in
            // single-file mode is this file's anyway.
            let symbol = match program.index_of(path) {
                Some(file) => program.sub_in(file, &package, &name),
                None => program.sub(&package, &name),
            };
            let text = symbol.map_or_else(
                || format!("{package}::{name} -> Unknown"),
                camello_sema::decl::signature_of,
            );
            Some((text, range))
        }
        Some(Target::Call { name, range }) => {
            // `resolve_call` is answered against this file's own imports:
            // `use POSIX qw(floor)` makes `floor` a different sub here than
            // it is next door.
            let symbol = program
                .index_of(path)
                .and_then(|file| program.resolve_call(file, u32::from(range.start()), &name));
            let text = symbol.map_or_else(
                || format!("{name} -> Unknown"),
                camello_sema::decl::signature_of,
            );
            Some((text, range))
        }
        Some(Target::UnresolvedMethod { name, range }) => {
            Some((format!("{name} -> Unknown"), range))
        }
        Some(Target::Lexical { binding, range }) => {
            let found = tables.scope.bindings.get(binding)?;
            // The type is keyed by where the expression is, so a reference is
            // looked up at itself and a declaration at itself. A binding with
            // none is `Unknown`, which is a real answer about a real name and
            // the one the reader came for.
            let ty = tables
                .types
                .of(range)
                .or_else(|| tables.types.at(range.start()).map(|(_, ty)| ty));
            let ty = ty.map_or_else(
                || camello_sema::types::Type::Unknown.to_string(),
                |ty| ty.to_string(),
            );
            Some((format!("{} : {ty}", found.display()), range))
        }
        None => {
            let (range, ty) = tables.types.at(offset)?;
            Some((ty.to_string(), range))
        }
    }
}
