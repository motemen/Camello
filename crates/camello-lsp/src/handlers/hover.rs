//! `textDocument/hover` (`docs/lsp.md`, "Hover").
//!
//! On a typed expression or a binding: the inferred type, spelled the way
//! `docs/types.md` spells types. On a sub name — a definition or a call: the
//! signature the declaration pass read.
//!
//! Where the checker knows nothing, hover shows nothing. `Unknown` produces an
//! empty response and not a shrug-string, because the checker's silence
//! discipline is a feature to surface rather than a gap to paper over: a user
//! who is shown `Unknown` learns that camello answered, and a user who is
//! shown nothing learns that it did not.

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
                MethodLookup::Universal | MethodLookup::Unknown | MethodLookup::Missing => {
                    return None
                }
            };
            Some((text, site.method_range))
        }
        Some(Target::Definition {
            package,
            name,
            range,
        }) => {
            let symbol = program.sub(&package, &name)?;
            Some((camello_sema::decl::signature_of(symbol), range))
        }
        Some(Target::Call { name, range }) => {
            // `resolve_call` is answered against this file's own imports:
            // `use POSIX qw(floor)` makes `floor` a different sub here than
            // it is next door.
            let file = program.index_of(path)?;
            let symbol = program.resolve_call(file, u32::from(range.start()), &name)?;
            Some((camello_sema::decl::signature_of(symbol), range))
        }
        Some(Target::Lexical { binding, range }) => {
            let found = tables.scope.bindings.get(binding)?;
            // The type is keyed by where the expression is, so a reference is
            // looked up at itself and a declaration at itself.
            //
            // No type, no hover. Answering `$thing` to a hover over `$thing`
            // is a shrug-string with extra steps: it tells the reader what
            // they are already looking at, and it hides the one thing the
            // silence would have told them, which is that the checker has
            // nothing on this value.
            let ty = tables
                .types
                .of(range)
                .or_else(|| tables.types.at(range.start()).map(|(_, ty)| ty))?;
            Some((format!("{} : {ty}", found.display()), range))
        }
        None => {
            let (range, ty) = tables.types.at(offset)?;
            Some((ty.to_string(), range))
        }
    }
}
