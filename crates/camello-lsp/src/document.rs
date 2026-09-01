//! The open files, and what has been parsed out of them
//! (`docs/lsp.md`, "State, snapshots, and `Send`").
//!
//! The one hard constraint here is rowan's: a `SyntaxNode` is `Rc`-based and
//! therefore not `Send`, while a `GreenNode` is `Send + Sync`. `src/report.rs`
//! lives with that by parsing every file twice — once for declarations, once
//! for bodies — which is the right bargain for a batch run over a corpus and
//! the wrong one for a file being retyped. So an open file is parsed once per
//! edit and what is kept is the green tree; any thread that needs a walkable
//! tree calls `SyntaxNode::new_root(green.clone())`, which is an `Rc`
//! allocation and not a reparse.
//!
//! Everything in here is `Send + Sync`, so a request handler can take an
//! `Arc<Document>` to a blocking thread and an edit landing meanwhile makes
//! the answer stale rather than unsound. Stale answers are dropped by version
//! check before they are published.

use std::path::PathBuf;
use std::sync::Arc;

use camello_syntax::lang::SyntaxNode;
use camello_syntax::parse::TriviaMap;
use rowan::{GreenNode, TextRange};

use crate::position::{Encoding, PositionMap};

/// One parse diagnostic, kept without the source snippet `miette` wants.
///
/// `camello_syntax::ParseError` carries an `Arc<str>` of the whole file so a
/// CLI can draw a caret under the line. An editor draws its own, from the
/// range, over text it already has.
#[derive(Debug, Clone)]
pub struct ParseDiagnostic {
    pub message: String,
    pub range: TextRange,
}

/// An open buffer, as of one LSP document version.
#[derive(Debug)]
pub struct Document {
    /// The file behind the buffer, where there is one. An `untitled:` document
    /// has none, and is analysed on its own.
    pub path: Option<PathBuf>,
    pub text: Arc<str>,
    /// `Send + Sync`, and the reason a tree need not be.
    pub green: GreenNode,
    pub trivia: TriviaMap,
    pub parse_errors: Vec<ParseDiagnostic>,
    /// The client's version of the text above. An answer computed from this
    /// document is published only while the store still holds this version.
    pub version: i32,
    pub positions: PositionMap,
}

impl Document {
    #[must_use]
    pub fn new(path: Option<PathBuf>, text: &str, version: i32, encoding: Encoding) -> Self {
        let parsed = camello_syntax::parse::parse(text);
        Document {
            path,
            text: Arc::from(text),
            green: parsed.green,
            trivia: parsed.trivia,
            parse_errors: parsed
                .diagnostics
                .into_iter()
                .map(|diagnostic| ParseDiagnostic {
                    message: diagnostic.message,
                    range: diagnostic.range,
                })
                .collect(),
            version,
            positions: PositionMap::new(text, encoding),
        }
    }

    /// A walkable tree for this thread.
    ///
    /// Cheap, and not a reparse: the green tree is shared and this is the
    /// `Rc` wrapper around it.
    #[must_use]
    pub fn tree(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// Whether the parser had anything to say about this buffer.
    #[must_use]
    pub fn parsed_cleanly(&self) -> bool {
        self.parse_errors.is_empty()
    }

    /// The path the checker knows this file by.
    ///
    /// A buffer with no file behind it still needs a name, because the program
    /// graph is keyed by path and a declaration has to be attributable to
    /// something. `<untitled>` is that name, and nothing resolves to it.
    #[must_use]
    pub fn analysis_path(&self) -> PathBuf {
        self.path
            .clone()
            .unwrap_or_else(|| PathBuf::from("<untitled>"))
    }
}
