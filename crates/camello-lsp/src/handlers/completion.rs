//! `textDocument/completion` — methods on a receiver
//! (`docs/lsp.md`, "Completion").
//!
//! The wanted feature, and the one the type side-table exists for.
//! `$obj-><C-x>` shows what the receiver's class actually has, in MRO order,
//! the class's own methods first.
//!
//! If the receiver's type is `Unknown`: **no items**, deliberately. The
//! interview chose precision over recall — a flood of every sub name in a
//! thousand-file workspace teaches the user to ignore the feature, while an
//! empty list teaches them what the checker can and cannot see, which is the
//! same thing `camello check`'s silence teaches.
//!
//! One mechanical consequence of completing *while typing*: `$obj->` with
//! nothing after it is a parse error, so the receiver cannot be found by
//! asking the tree what expression the cursor is in. It is found by walking
//! tokens left from the cursor instead — past the `->`, taking the primary
//! expression before it. This is the one place the server reads tokens rather
//! than the tree, and it is confined to this file.

use camello_sema::program::MethodKind;
use camello_sema::types::Type;
use camello_syntax::lang::{SyntaxNode, SyntaxToken, TokenExt, TokenKind};
use rowan::{TextRange, TextSize};
use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind};

use crate::analysis::{Context, Tables};
use crate::document::Document;
use crate::handlers::prev_meaningful;

/// The character that triggers completion without being asked.
pub const TRIGGER: &str = ">";

#[must_use]
pub fn completion(
    document: &Document,
    tables: &Tables,
    fallback: Option<&Tables>,
    context: &Context<'_>,
    offset: TextSize,
) -> Vec<CompletionItem> {
    let root = document.tree();
    let Some(receiver) = receiver_range(&root, offset) else {
        return Vec::new();
    };
    let Some(class) = class_of(&root, receiver, tables, fallback, context) else {
        return Vec::new();
    };
    items(context, &class)
}

/// The expression the `->` under the cursor hangs off, by walking left.
///
/// Two shapes reach here: the cursor straight after the arrow (`$obj->`) and
/// the cursor inside a name being typed after it (`$obj->fo`). Anything else
/// is not a method completion and answers `None`, which is how "no items"
/// stays the default rather than a special case.
fn receiver_range(root: &SyntaxNode, offset: TextSize) -> Option<TextRange> {
    let token = crate::handlers::token_at(root, offset)?;
    let arrow = if token.token_kind() == TokenKind::ARROW {
        token
    } else {
        let previous = prev_meaningful(&token)?;
        if previous.token_kind() == TokenKind::ARROW {
            previous
        } else {
            // `$obj->fo|` — the token under the cursor is the partial name,
            // and the arrow is one further left.
            let before = prev_meaningful(&previous)?;
            if before.token_kind() != TokenKind::ARROW {
                return None;
            }
            before
        }
    };
    let last = prev_meaningful(&arrow)?;
    Some(widest_ending_at(&last))
}

/// The largest expression that ends where this token does.
///
/// `$self->name->` has `name` as the token before the second arrow, and what
/// the completion is about is `$self->name` — so the ancestors are climbed
/// while they still end on that token. An `ERROR` node the broken parse put
/// around the arrow ends *after* it, so this stops short of it, which is
/// exactly what makes the walk survive a half-typed line.
fn widest_ending_at(token: &SyntaxToken) -> TextRange {
    let end = token.text_range().end();
    let mut range = token.text_range();
    for node in token.parent_ancestors() {
        if node.text_range().end() == end {
            range = node.text_range();
        } else {
            break;
        }
    }
    range
}

/// The class the receiver names, from the type side-table or from the text.
fn class_of(
    root: &SyntaxNode,
    receiver: TextRange,
    tables: &Tables,
    fallback: Option<&Tables>,
    context: &Context<'_>,
) -> Option<String> {
    // A bareword invocant is a class outright — `Foo::Bar->new` — and needs
    // no inference at all, which is what makes it work in a buffer too broken
    // to type.
    let text = root.text().slice(receiver).to_string();
    if is_class_name(&text) && context.program().knows_package(&text) {
        return Some(text);
    }

    let from_table = |tables: &Tables| -> Option<Type> {
        tables
            .types
            .of(receiver)
            .or_else(|| tables.types.at(receiver.start()).map(|(_, ty)| ty))
            .cloned()
    };
    // The current version first. The fallback is the last table from a clean
    // parse, and it is consulted because the ranges of a *broken* parse may
    // not hold the receiver at all: it went into an `ERROR` node and was
    // never typed. Its offsets are those of the previous text, which is why
    // the start of the receiver — before the edit, not after it — is what is
    // looked up.
    let ty = from_table(tables).or_else(|| fallback.and_then(from_table))?;
    match ty.without_undef() {
        Type::InstanceOf(class) | Type::ConsumerOf(class) => Some(class),
        _ => None,
    }
}

fn is_class_name(text: &str) -> bool {
    !text.is_empty()
        && text.chars().next().is_some_and(char::is_uppercase)
        && text
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == ':')
}

/// Everything callable on the class, the class's own first.
fn items(context: &Context<'_>, class: &str) -> Vec<CompletionItem> {
    context
        .program()
        .methods_of(class)
        .into_iter()
        .map(|method| {
            let kind = match method.kind {
                MethodKind::Sub(_) => CompletionItemKind::METHOD,
                MethodKind::Attribute(_) => CompletionItemKind::PROPERTY,
                MethodKind::Constructor => CompletionItemKind::CONSTRUCTOR,
                MethodKind::Universal => CompletionItemKind::METHOD,
            };
            CompletionItem {
                label: method.name.clone(),
                kind: Some(kind),
                detail: method.signature(),
                // Inherited last, and the class's own in declaration order:
                // the depth is the linearisation position, so sorting on it
                // is sorting by how perl would find them.
                sort_text: Some(format!("{:04}{}", method.depth, method.name)),
                label_details: (method.class != class).then(|| {
                    tower_lsp_server::ls_types::CompletionItemLabelDetails {
                        detail: None,
                        description: Some(method.class.clone()),
                    }
                }),
                ..CompletionItem::default()
            }
        })
        .collect()
}
