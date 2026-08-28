//! The request handlers.
//!
//! Thin, all of them, and deliberately so (`docs/lsp.md`, "What sema must
//! newly expose"): the work is in the tables `camello-sema` now hands out, and
//! a handler's job is to find the offset, ask, and spell the answer in LSP.
//! Every boundary crossing goes through [`crate::position::PositionMap`] — no
//! LSP type appears below this layer, and no `TextRange` above it.

pub mod completion;
pub mod definition;
pub mod diagnostics;
pub mod formatting;
pub mod hover;
pub mod symbols;

use camello_syntax::lang::{SyntaxNode, SyntaxToken, TokenExt, TokenKind};
use rowan::TextSize;

/// The token at an offset, preferring the one to the left.
///
/// A cursor sits *between* two tokens, and what the user means by "here" is
/// almost always the thing they just typed or the word they are inside. Trivia
/// is not an answer, so it is stepped over.
#[must_use]
pub fn token_at(root: &SyntaxNode, offset: TextSize) -> Option<SyntaxToken> {
    let found = root.token_at_offset(offset);
    let candidates: Vec<SyntaxToken> = found.into_iter().collect();
    candidates
        .iter()
        .find(|token| !is_skippable(token))
        .or_else(|| candidates.first())
        .cloned()
}

/// The previous token that is not whitespace or a comment.
#[must_use]
pub fn prev_meaningful(token: &SyntaxToken) -> Option<SyntaxToken> {
    let mut current = token.prev_token();
    while let Some(found) = current {
        if !is_skippable(&found) {
            return Some(found);
        }
        current = found.prev_token();
    }
    None
}

fn is_skippable(token: &SyntaxToken) -> bool {
    matches!(
        token.token_kind(),
        TokenKind::WHITESPACE | TokenKind::NEWLINE | TokenKind::COMMENT
    )
}

/// What the cursor is on.
///
/// One reading, shared: hover says what it is and definition says where it
/// came from, and two readings of the same cursor would be two features that
/// disagree about what the user pointed at.
#[derive(Debug, Clone)]
pub enum Target {
    /// A `->` whose receiver resolved to a class.
    Method(camello_sema::flow::MethodSite),
    /// A lexical reference or the declaration itself.
    Lexical {
        binding: usize,
        range: rowan::TextRange,
    },
    /// A bareword call: `foo(...)`, `foo @list`.
    Call {
        name: String,
        range: rowan::TextRange,
    },
    /// The name in a `sub name { ... }`.
    Definition {
        package: String,
        name: String,
        range: rowan::TextRange,
    },
    /// A `->` whose receiver did *not* resolve to a class, so nothing knows
    /// which method this is. Still a name the cursor is on, and still a
    /// question worth answering with "I do not know".
    UnresolvedMethod {
        name: String,
        range: rowan::TextRange,
    },
}

/// Read the cursor.
///
/// The order is the order of specificity: a resolved method call says more
/// than the fact that its invocant is a lexical, and a sub's own name says
/// more than the call syntax it sits in.
#[must_use]
pub fn target_at(
    root: &SyntaxNode,
    tables: &crate::analysis::Tables,
    offset: TextSize,
) -> Option<Target> {
    use camello_syntax::ast::{AstNode, Call, SubDef};
    use camello_syntax::lang::{NodeExt, NodeKind};

    if let Some(site) = tables.types.method_at(offset) {
        return Some(Target::Method(site.clone()));
    }

    let token = token_at(root, offset)?;
    for node in token.parent_ancestors() {
        match node.node_kind() {
            NodeKind::SUB_DEF => {
                // A `sub` a broken buffer left nameless answers nothing here
                // and falls through to the readings below, rather than
                // ending the search.
                if let Some(name) = SubDef::cast(node.clone()).and_then(|view| view.name()) {
                    if name.syntax().text_range().contains_inclusive(offset) {
                        return Some(Target::Definition {
                            package: package_at(root, offset),
                            name: name.text(),
                            range: name.syntax().text_range(),
                        });
                    }
                }
            }
            NodeKind::CALL_EXPR | NodeKind::LIST_CALL_EXPR => {
                if let Some(view) = Call::cast(node.clone()) {
                    if view.callee_range().contains_inclusive(offset) {
                        if let Some(name) = view.callee_name() {
                            return Some(Target::Call {
                                name,
                                range: view.callee_range(),
                            });
                        }
                    }
                }
            }
            // The receiver's class is unknown — otherwise `method_at` would
            // have answered above — but the cursor is on a method name all
            // the same.
            NodeKind::METHOD_CALL_EXPR => {
                if let Some(view) = camello_syntax::ast::MethodCall::cast(node.clone()) {
                    if view.method_range().contains_inclusive(offset) {
                        if let Some(name) = view.method_name() {
                            return Some(Target::UnresolvedMethod {
                                name,
                                range: view.method_range(),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if let Some((reference, _)) = tables.scope.reference_at(offset) {
        return Some(Target::Lexical {
            binding: reference.binding,
            range: reference.range,
        });
    }
    if let Some(binding) = tables.scope.binding_at(offset) {
        let index = tables
            .scope
            .bindings
            .iter()
            .position(|candidate| candidate.range == binding.range)?;
        return Some(Target::Lexical {
            binding: index,
            range: binding.range,
        });
    }
    None
}

/// The package an offset is written in, by the same rule the declaration pass
/// reads: the last `package` statement at or before it, or `main`.
#[must_use]
pub fn package_at(root: &SyntaxNode, offset: TextSize) -> String {
    use camello_syntax::ast::{AstNode, PackageStmt};
    use camello_syntax::lang::{NodeExt, NodeKind};

    let mut package = "main".to_string();
    for node in root.descendants() {
        if node.node_kind() != NodeKind::PACKAGE_STMT {
            continue;
        }
        if node.text_range().start() > offset {
            break;
        }
        if let Some(name) = PackageStmt::cast(node).and_then(|view| view.name()) {
            package = name;
        }
    }
    package
}
