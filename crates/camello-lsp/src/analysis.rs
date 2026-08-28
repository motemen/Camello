//! What the checker says about one open buffer, and what it knew while
//! saying it (`docs/lsp.md`, "Diagnostics").
//!
//! Two producers, one publication: the parse errors the document store
//! already holds, and the checker's own diagnostics over the stored tree.
//!
//! The deliberate divergence from the CLI is here. `check_one` in
//! `src/report.rs` discards every sema diagnostic for a file that fails to
//! parse, which is the right call for a batch tool — a broken file is one
//! error, not fifty — and the wrong one for an editor, where the buffer is
//! broken *most of the time* and the user still wants the fifty real answers
//! about the parts they are not touching. The recovery machinery guarantees a
//! usable partial tree, so the checker runs; what is left is noise control
//! near the damage, and that is the blast radius below.

use std::sync::Arc;

use camello_sema::flow::TypeTable;
use camello_sema::scope::ScopeReport;
use camello_sema::{Analysis, Diagnostic};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode};
use rowan::TextRange;

use crate::document::Document;
use crate::index::Index;
use crate::settings::Settings;

/// The tables one analysis of one document version produced.
///
/// Held so that hover and completion do not re-run the body pass per
/// keystroke, and so that completion has a *previous* table to fall back to
/// when the current parse put the receiver inside an `ERROR` node.
pub struct Tables {
    pub version: i32,
    pub types: TypeTable,
    pub scope: ScopeReport,
    /// Whether the parse behind these tables was clean. A table from a clean
    /// parse is what the dangling-arrow fallback wants.
    pub clean: bool,
    /// Which file the graph knew this as, or `None` in single-file mode.
    pub file: Option<usize>,
}

/// One analysis: what to publish, and what was learnt.
pub struct Analysed {
    pub diagnostics: Vec<Diagnostic>,
    pub tables: Arc<Tables>,
}

/// Which analysis is answering about a document, and the file index it knows
/// it by.
///
/// Single-file mode is not a degraded mode so much as an earlier one: the
/// lexical diagnostics are exact either way, and what the graph adds is the
/// cross-file half — arity against a sub two files over, a method on an
/// inherited class. Before the index finishes, and for a buffer outside the
/// workspace, that half is absent rather than guessed.
///
/// It is a borrow of the graph or a whole analysis of its own, and every
/// handler asks it the same two questions, so neither has to know which.
pub enum Context<'a> {
    Graph(&'a Analysis),
    /// Boxed because the two arms are a pointer and a whole graph, and every
    /// handler holds one of these on the stack.
    Alone(Box<Analysis>),
}

impl Context<'_> {
    #[must_use]
    pub fn analysis(&self) -> &Analysis {
        match self {
            Context::Graph(analysis) => analysis,
            Context::Alone(analysis) => analysis,
        }
    }

    #[must_use]
    pub fn program(&self) -> &camello_sema::Program {
        self.analysis().program()
    }
}

/// Pick the analysis that can answer about this document.
#[must_use]
pub fn context<'a>(document: &Document, index: &'a Index, settings: &Settings) -> Context<'a> {
    let path = document.analysis_path();
    if index.holds(&path) {
        Context::Graph(&index.analysis)
    } else {
        Context::Alone(Box::new(single_file(&path, &document.tree(), settings)))
    }
}

/// Run the checker over a document.
#[must_use]
pub fn analyse(
    document: &Document,
    context: &Context<'_>,
    settings: &Settings,
    record: bool,
) -> Analysed {
    let root = document.tree();
    let path = document.analysis_path();
    let found =
        context
            .analysis()
            .analyse_file(&path, &root, &document.text, &settings.options, record);
    let mut diagnostics = found.diagnostics;
    diagnostics.retain(|diagnostic| diagnostic.severity >= settings.min_severity);
    if !document.parsed_cleanly() {
        let blast = blast_radius(&root, document);
        diagnostics
            .retain(|diagnostic| !blast.iter().any(|range| overlaps(diagnostic.range, *range)));
    }

    Analysed {
        diagnostics,
        tables: Arc::new(Tables {
            version: document.version,
            types: found.types,
            scope: found.scope,
            clean: document.parsed_cleanly(),
            file: found.file,
        }),
    }
}

/// A graph holding this file and nothing else.
fn single_file(path: &std::path::Path, root: &SyntaxNode, settings: &Settings) -> Analysis {
    let mut analysis = settings.empty_analysis();
    analysis.declare(path, root, true);
    analysis.link();
    analysis
}

/// Where a sema diagnostic is not to be believed, because the parse is broken
/// there.
///
/// The rule: a diagnostic whose range meets the **enclosing statement of an
/// `ERROR` node or of a parse-error range** is dropped, and everything else is
/// published. The half-typed statement under the cursor produces no cascade,
/// and the rest of the file keeps its full signal.
///
/// Statement granularity is not a guess: it is where the parser's own recovery
/// synchronises, which makes it the natural blast radius rather than an
/// arbitrary one. If real editing shows a cascade leaking past it — a broken
/// `sub` header poisoning a whole body — the next answer is the enclosing
/// block, and the broken-buffer fixtures are where that evidence accumulates
/// (`docs/lsp.md`, "Open questions").
#[must_use]
pub fn blast_radius(root: &SyntaxNode, document: &Document) -> Vec<TextRange> {
    let mut ranges: Vec<TextRange> = Vec::new();
    let push = |range: TextRange, ranges: &mut Vec<TextRange>| {
        if !ranges.contains(&range) {
            ranges.push(range);
        }
    };

    for node in root.descendants() {
        if node.node_kind() == NodeKind::ERROR {
            push(enclosing_statement(&node).text_range(), &mut ranges);
        }
    }
    for error in &document.parse_errors {
        if let Some(node) = node_covering(root, error.range) {
            push(enclosing_statement(&node).text_range(), &mut ranges);
        } else {
            push(error.range, &mut ranges);
        }
    }
    ranges
}

/// The innermost statement a node sits in, or the node itself when it is not
/// inside one.
fn enclosing_statement(node: &SyntaxNode) -> SyntaxNode {
    node.ancestors()
        .find(|ancestor| is_statement(ancestor.node_kind()))
        .unwrap_or_else(|| node.clone())
}

const fn is_statement(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::EXPR_STMT
            | NodeKind::VAR_DECL_STMT
            | NodeKind::IF_STMT
            | NodeKind::LOOP_STMT
            | NodeKind::SUB_DEF
            | NodeKind::PACKAGE_STMT
            | NodeKind::USE_STMT
            | NodeKind::NO_STMT
            | NodeKind::TRY_STMT
            | NodeKind::GIVEN_STMT
            | NodeKind::BLOCK_STMT
            | NodeKind::LABELED_STMT
            | NodeKind::PHASE_BLOCK
            | NodeKind::FORMAT_DECL
            | NodeKind::EMPTY_STMT
    )
}

/// The smallest node covering a range, with the range clamped into the tree
/// first: a parse error at end of file names an offset the tree does not
/// reach.
fn node_covering(root: &SyntaxNode, range: TextRange) -> Option<SyntaxNode> {
    let whole = root.text_range();
    let start = range.start().min(whole.end()).max(whole.start());
    let end = range.end().min(whole.end()).max(start);
    let element = root.covering_element(TextRange::new(start, end));
    match element {
        rowan::NodeOrToken::Node(node) => Some(node),
        rowan::NodeOrToken::Token(token) => token.parent(),
    }
}

/// Whether two ranges share any text, an empty range counting as the point it
/// sits at.
#[must_use]
pub fn overlaps(a: TextRange, b: TextRange) -> bool {
    if a.is_empty() || b.is_empty() {
        return a.start() <= b.end() && b.start() <= a.end();
    }
    a.start() < b.end() && b.start() < a.end()
}
