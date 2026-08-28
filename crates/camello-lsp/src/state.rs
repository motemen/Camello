//! What the server knows, and how a request gets at it
//! (`docs/lsp.md`, "State, snapshots, and `Send`").
//!
//! The rust-analyzer shape: one [`GlobalState`] behind a lock, mutated only by
//! notifications — `didOpen`, `didChange`, watched-file events, the index
//! finishing — while a request takes a [`Snapshot`] of `Arc`s and does all its
//! work on it, on a blocking thread. A request never blocks an edit; an edit
//! never invalidates a request mid-flight, it only makes the answer stale, and
//! a stale answer is dropped by version check before it is published.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use tower_lsp_server::ls_types::Uri;

use crate::analysis::Tables;
use crate::document::Document;
use crate::index::Index;
use crate::position::Encoding;
use crate::settings::Settings;

/// Everything the server holds between messages.
pub struct GlobalState {
    pub settings: Arc<Settings>,
    /// What `positionEncoding` the client agreed to.
    pub encoding: Encoding,
    /// Whether the client accepts a dynamic registration for
    /// `workspace/didChangeWatchedFiles`. A client that does not is not asked:
    /// an unanswered registration request would leave the server waiting on a
    /// reply that never comes.
    pub dynamic_watchers: bool,
    pub documents: HashMap<Uri, Arc<Document>>,
    /// The tables of the most recent analysis of each open document.
    pub tables: HashMap<Uri, Arc<Tables>>,
    /// The most recent tables that came out of a *clean* parse.
    ///
    /// Completion needs this and nothing else does: `$obj->` with nothing
    /// after it is a parse error, and the receiver's type is in the table the
    /// last complete parse produced (`docs/lsp.md`, "Completion").
    pub clean_tables: HashMap<Uri, Arc<Tables>>,
    /// Bumped per document on every edit; a debounced job that finds the
    /// counter moved on knows a newer edit is already scheduled.
    pub edits: HashMap<Uri, u64>,
    /// The program graph. Its own lock, because the background walk holds it
    /// for as long as a swap takes and a request only ever reads.
    pub index: Arc<RwLock<Index>>,
}

impl GlobalState {
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        let index = Arc::new(RwLock::new(Index::empty(&settings)));
        GlobalState {
            settings: Arc::new(settings),
            encoding: Encoding::default(),
            dynamic_watchers: false,
            documents: HashMap::new(),
            tables: HashMap::new(),
            clean_tables: HashMap::new(),
            edits: HashMap::new(),
            index,
        }
    }

    /// What a request needs, without holding the lock while it works.
    #[must_use]
    pub fn snapshot(&self, uri: &Uri) -> Option<Snapshot> {
        let document = self.documents.get(uri)?.clone();
        Some(Snapshot {
            document,
            settings: Arc::clone(&self.settings),
            index: Arc::clone(&self.index),
            tables: self.tables.get(uri).cloned(),
            clean_tables: self.clean_tables.get(uri).cloned(),
        })
    }

    /// Record what one analysis learnt.
    pub fn remember(&mut self, uri: &Uri, tables: Arc<Tables>) {
        if tables.clean {
            self.clean_tables.insert(uri.clone(), Arc::clone(&tables));
        }
        self.tables.insert(uri.clone(), tables);
    }

    /// The next edit generation for a document.
    pub fn bump(&mut self, uri: &Uri) -> u64 {
        let counter = self.edits.entry(uri.clone()).or_insert(0);
        *counter += 1;
        *counter
    }

    /// Whether the generation a debounced job was scheduled under is still the
    /// current one.
    #[must_use]
    pub fn is_current(&self, uri: &Uri, generation: u64) -> bool {
        self.edits.get(uri).copied().unwrap_or(0) == generation
    }

    pub fn forget(&mut self, uri: &Uri) {
        self.documents.remove(uri);
        self.tables.remove(uri);
        self.clean_tables.remove(uri);
        self.edits.remove(uri);
    }

    /// Every open document, for the sweep an edited declaration forces.
    #[must_use]
    pub fn open_uris(&self) -> Vec<Uri> {
        self.documents.keys().cloned().collect()
    }

    /// The same, as paths: what the graph knows a file by, and what a
    /// watched-file event has to be told not to overwrite.
    #[must_use]
    pub fn open_paths(&self) -> Vec<PathBuf> {
        self.documents
            .keys()
            .filter_map(|uri| uri.to_file_path().map(|path| path.into_owned()))
            .collect()
    }
}

/// One request's view of the world: `Arc`s, and nothing that can change under
/// it.
#[derive(Clone)]
pub struct Snapshot {
    pub document: Arc<Document>,
    pub settings: Arc<Settings>,
    pub index: Arc<RwLock<Index>>,
    /// The tables of the last analysis, of whatever version that was.
    pub tables: Option<Arc<Tables>>,
    /// The last tables from a clean parse.
    pub clean_tables: Option<Arc<Tables>>,
}

impl Snapshot {
    /// The tables for the document as it is now, computing them if the ones
    /// held are for an older version.
    ///
    /// Hover and completion arrive between debounced diagnostic passes, so
    /// the cached tables are usually a version or two behind. One body pass
    /// over one file is milliseconds; answering from a table that describes
    /// text the user has since changed is a wrong answer, which costs more.
    #[must_use]
    pub fn tables_now(&self) -> Arc<Tables> {
        if let Some(tables) = &self.tables {
            if tables.version == self.document.version {
                return Arc::clone(tables);
            }
        }
        let index = self.index.read().expect("no writer panics holding this");
        let context = crate::analysis::context(&self.document, &index, &self.settings);
        crate::analysis::analyse(&self.document, &context, &self.settings, true).tables
    }
}
