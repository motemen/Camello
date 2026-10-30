//! The protocol surface (`docs/lsp.md`, "The crate and the runtime").
//!
//! `tower-lsp-server` — the maintained community fork of `tower-lsp` — is a
//! deliberate exception to camello's no-async, few-dependencies habit: the
//! alternative was `lsp-server` plus a hand-rolled main loop, and the trade
//! taken was the trait. Implement [`LanguageServer`], get the plumbing.
//!
//! The discipline that keeps the cost contained is the whole of this file's
//! shape: **tokio exists to shuttle JSON-RPC, and nothing else.** Every
//! request handler here does three things and no more — take a snapshot under
//! the lock, hand it to `spawn_blocking`, spell the answer in LSP. Every
//! notification handler updates the store and hands the rest to a task of its
//! own (`docs/lsp.md`, "State, snapshots, and `Send`"). All parsing and
//! analysis is CPU-bound and runs on a blocking thread; no analysis code is
//! async, and no async type reaches `camello-sema`.

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use rowan::TextSize;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer};

use crate::analysis::{self, Tables};
use crate::document::Document;
use crate::handlers;
use crate::index;
use crate::position::Encoding;
use crate::settings::Settings;
use crate::state::{self, GlobalState, Snapshot};

/// How long after a keystroke the checker runs.
///
/// Feedback while typing, not only on save — and not on every keystroke
/// either, which for a file of any size is a body pass thrown away before it
/// finishes (`docs/lsp.md`, "Diagnostics").
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(300);

/// The server. Cheap to clone — `Arc`s and a client handle — so that a
/// notification can hand a copy of it to the task that does its work.
#[derive(Clone)]
pub struct Backend {
    client: Client,
    state: Arc<RwLock<GlobalState>>,
    /// Held from a publication's version check to its send, and around a
    /// close's forgetting and its empty publication.
    ///
    /// Checks run concurrently, and a version check that passed means nothing
    /// if another task can check, publish a newer answer, and be overtaken by
    /// this one's send. With the check and the send one step, an older answer
    /// is never the last one a client is sent.
    publishing: Arc<tokio::sync::Mutex<()>>,
}

impl Backend {
    #[must_use]
    pub fn new(client: Client) -> Self {
        let (settings, _) = Settings::load(&std::env::current_dir().unwrap_or_default(), &[]);
        Backend {
            client,
            state: Arc::new(RwLock::new(GlobalState::new(settings))),
            publishing: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Run a notification's work as a task of its own.
    ///
    /// Notifications and requests share the transport's few concurrent slots,
    /// so a notification that waited out a debounce or swept every open file
    /// before returning would hold a hover up behind it. What has to happen in
    /// the order the client sent it — storing the text, numbering the edit —
    /// happens in the handler before this is called.
    fn in_background<F>(&self, work: impl FnOnce(Backend) -> F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        tokio::spawn(work(self.clone()));
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, GlobalState> {
        state::read(&self.state)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, GlobalState> {
        state::write(&self.state)
    }

    /// Parse a buffer and put it in the store.
    fn store(&self, uri: &Uri, text: &str, version: i32) -> Arc<Document> {
        let encoding = self.read().encoding;
        let document = Arc::new(Document::new(
            uri.to_file_path().map(|path| path.into_owned()),
            text,
            version,
            encoding,
        ));
        let mut state = self.write();
        state.documents.insert(uri.clone(), Arc::clone(&document));
        document
    }

    /// Check a document now and publish the result.
    ///
    /// The version is carried through: an edit landing while this runs makes
    /// the answer stale, and a stale answer is dropped here rather than shown.
    async fn check_and_publish(&self, uri: Uri) {
        let Some(snapshot) = self.read().snapshot(&uri) else {
            return;
        };
        let version = snapshot.document.version;
        let outcome = tokio::task::spawn_blocking(move || {
            let index = state::read(&snapshot.index);
            let context = analysis::context(&snapshot.document, &index, &snapshot.settings);
            let found = analysis::analyse(&snapshot.document, &context, &snapshot.settings, true);
            let published = handlers::diagnostics::publish(&snapshot.document, &found.diagnostics);
            (published, found.tables)
        })
        .await;
        let Some((published, tables)) = joined(&self.client, "a check", outcome).await else {
            return;
        };
        let _turn = self.publishing.lock().await;
        {
            let mut held = self.write();
            // Still the version that was analysed? If not, the newer text has
            // its own debounce already running and this answer is about text
            // nobody is looking at.
            if held
                .documents
                .get(&uri)
                .is_none_or(|document| document.version != version)
            {
                return;
            }
            held.remember(&uri, tables);
        }
        self.client
            .publish_diagnostics(uri, published, Some(version))
            .await;
    }

    /// Answer a request about one position in an open document: its tables
    /// as of now and the graph, or the file alone, as context — on a blocking
    /// thread, as every analysis is.
    ///
    /// `None` when the document is not open. A thread that panicked answers
    /// with nothing found rather than an error, and says so in the log.
    async fn with_analysis<T>(
        &self,
        position: TextDocumentPositionParams,
        answer: impl FnOnce(&Snapshot, &Tables, &analysis::Context<'_>, TextSize) -> T + Send + 'static,
    ) -> Option<T>
    where
        T: Default + Send + 'static,
    {
        let snapshot = self.read().snapshot(&position.text_document.uri)?;
        let offset = snapshot.document.positions.offset(position.position);
        let outcome = tokio::task::spawn_blocking(move || {
            let tables = snapshot.tables_now();
            let index = state::read(&snapshot.index);
            let context = analysis::context(&snapshot.document, &index, &snapshot.settings);
            answer(&snapshot, &tables, &context, offset)
        })
        .await;
        Some(
            joined(&self.client, "a request", outcome)
                .await
                .unwrap_or_default(),
        )
    }

    /// Wait out the debounce, then run the edit loop — unless a newer edit
    /// arrived meanwhile, in which case that one's timer is already running
    /// and this text is not what anybody is looking at.
    ///
    /// The five steps of "Incremental reanalysis" are here in order: the
    /// reparse happened in the document store, the declaration pass and the
    /// diff are [`Backend::reindex_if_declarations_changed`], and the body
    /// pass is the publication.
    ///
    /// `generation` is the edit's number, taken when the edit was stored.
    async fn debounced_check(&self, uri: Uri, generation: u64) {
        tokio::time::sleep(DEBOUNCE).await;
        if !self.read().is_current(&uri, generation) {
            return;
        }
        self.reindex_if_declarations_changed(&uri).await;
        self.check_and_publish(uri).await;
    }

    /// Re-run the declaration pass over an edited buffer and, if what it
    /// declares changed, install it and re-check every open file
    /// (`docs/lsp.md`, "Incremental reanalysis").
    ///
    /// Steps 4 and 5 of the design, and the whole of the incrementality: the
    /// overwhelmingly common edit changes a body, and a body is nobody else's
    /// business. The sweep in step 5 over-invalidates — an edited signature
    /// re-checks open files that never call it — and that is the accepted cost
    /// of coarse. The refinement (record which symbols each sub read;
    /// invalidate only dependents) slots in here without moving another piece,
    /// which is the test that this holds nothing that makes it harder.
    async fn reindex_if_declarations_changed(&self, uri: &Uri) {
        let Some(snapshot) = self.read().snapshot(uri) else {
            return;
        };
        let Some(path) = snapshot.document.path.clone() else {
            return;
        };
        let outcome = tokio::task::spawn_blocking(move || {
            let cache = crate::settings::cache(snapshot.settings.cache_dir.as_deref());
            let decls = camello_sema::read_declarations(
                &path,
                &snapshot.document.text,
                &snapshot.settings.dialect,
                &cache,
            );
            (path, decls, Arc::clone(&snapshot.document))
        })
        .await;
        let Some((path, decls, document)) =
            joined(&self.client, "the declaration pass", outcome).await
        else {
            return;
        };

        let changed = {
            let index = Arc::clone(&self.read().index);
            let mut index = state::write(&index);
            index.apply(vec![(&path, decls, &document.text)])
        };
        if changed {
            self.sweep(Some(uri)).await;
        }
    }

    /// Re-check every open file but `except` after the graph changed.
    ///
    /// Every *open* file, and no other: nobody is told about a broken caller
    /// in a file nobody is looking at — that is `camello check`'s job in CI,
    /// not the editor's. And step 4′ for each of them in turn: what changed
    /// may be what *their* returns were read from. Bounded by the open set,
    /// which is the same coarseness the sweep itself accepts.
    async fn sweep(&self, except: Option<&Uri>) {
        let open = { self.read().open_uris() };
        for uri in open {
            if Some(&uri) == except {
                continue;
            }
            self.reinfer(&uri).await;
            self.check_and_publish(uri).await;
        }
    }

    /// Step 4′ for one open file, whose returns may have been read off a file
    /// that just changed (`docs/return-inference.md`).
    async fn reinfer(&self, uri: &Uri) {
        let Some(snapshot) = self.read().snapshot(uri) else {
            return;
        };
        let Some(path) = snapshot.document.path.clone() else {
            return;
        };
        let index = Arc::clone(&self.read().index);
        let outcome = tokio::task::spawn_blocking(move || {
            let mut index = state::write(&index);
            index
                .analysis
                .reinfer_returns(&path, &snapshot.document.text)
        })
        .await;
        joined(&self.client, "return inference", outcome).await;
    }

    /// Put a file back the way disk has it, and re-check the open files if
    /// that changed anything they can see.
    ///
    /// The mirror of [`Backend::reindex_if_declarations_changed`]: that one
    /// installs a buffer over disk, this one installs disk over a buffer that
    /// is gone.
    async fn reinstall_from_disk(&self, path: PathBuf) {
        let settings = Arc::clone(&self.read().settings);
        let index = Arc::clone(&self.read().index);
        let outcome = tokio::task::spawn_blocking(move || {
            let source = std::fs::read_to_string(&path).ok()?;
            let cache = crate::settings::cache(settings.cache_dir.as_deref());
            let decls = camello_sema::read_declarations(&path, &source, &settings.dialect, &cache);
            let mut index = state::write(&index);
            Some(index.apply(vec![(&path, decls, &source)]))
        })
        .await;
        let changed = joined(&self.client, "reading a closed file back", outcome).await;
        if changed == Some(Some(true)) {
            self.sweep(None).await;
        }
    }

    /// Ask the client to watch the files that change the graph.
    ///
    /// Spawned rather than awaited, and that is not tidiness: `register_
    /// capability` is a request *to the client*, and awaiting it inside a
    /// notification handler holds up every notification behind it — the
    /// `didOpen` that follows immediately included. The registration is a
    /// side errand, so it runs as one.
    fn register_watchers(&self) {
        if !self.read().dynamic_watchers {
            return;
        }
        let client = self.client.clone();
        tokio::spawn(async move {
            // The two Perl extensions the checker walks, the two a script may
            // carry, and the configuration and the ignore files — which
            // reload and rewalk.
            let watchers = [
                "**/*.pl",
                "**/*.pm",
                "**/*.t",
                "**/*.psgi",
                "**/camello.toml",
                "**/.camelloignore",
                "**/.gitignore",
            ]
            .into_iter()
            .map(|pattern| FileSystemWatcher {
                glob_pattern: GlobPattern::String(pattern.to_string()),
                kind: None,
            })
            .collect();
            let registration = Registration {
                id: "camello-watched-files".to_string(),
                method: "workspace/didChangeWatchedFiles".to_string(),
                register_options: serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                    watchers,
                })
                .ok(),
            };
            let _ = client.register_capability(vec![registration]).await;
        });
    }

    /// Walk the workspace in the background and swap the graph in when it is
    /// built — unless another walk was started meanwhile, a configuration
    /// reload's, in which case this one read the old configuration and the
    /// newer walk's graph is the one to keep.
    fn spawn_index(&self) {
        let (settings, index, builds) = {
            let held = self.read();
            (
                Arc::clone(&held.settings),
                Arc::clone(&held.index),
                Arc::clone(&held.builds),
            )
        };
        let ticket = builds.begin();
        self.in_background(|this| async move {
            let built = tokio::task::spawn_blocking(move || index::build(&settings)).await;
            let Some(mut built) = joined(&this.client, "the index walk", built).await else {
                return;
            };
            if !builds.is_current(ticket) {
                return;
            }
            let files = built.files;
            // The walk read disk, and disk is out of date for anything the
            // user has been editing while it ran — which is every file they
            // opened, since they did not sit still for the walk. Their
            // buffers go in before the graph does, or the swap would drop
            // declarations the editor is showing and nothing would put them
            // back: the next edit to those buffers declares the same things,
            // and a diff that says "unchanged" installs nothing.
            let buffers: Vec<Arc<crate::document::Document>> =
                this.read().documents.values().map(Arc::clone).collect();
            let settings = Arc::clone(&this.read().settings);
            let relinked = tokio::task::spawn_blocking(move || {
                let cache = crate::settings::cache(settings.cache_dir.as_deref());
                let files = buffers
                    .iter()
                    .filter_map(|document| {
                        let path = document.path.as_deref()?;
                        let decls = camello_sema::read_declarations(
                            path,
                            &document.text,
                            &settings.dialect,
                            &cache,
                        );
                        Some((path, decls, &*document.text))
                    })
                    .collect();
                built.apply(files);
                built
            })
            .await;
            let Some(built) = joined(&this.client, "the index walk", relinked).await else {
                return;
            };
            if !builds.install(ticket, &index, built) {
                return;
            }
            this.client
                .log_message(
                    MessageType::INFO,
                    format!("camello: indexed {files} file{}", plural(files)),
                )
                .await;
            // Everything already open was answered from single-file analysis
            // while the walk ran; now that the graph is there, they get the
            // cross-file half of the answer.
            let open = this.read().open_uris();
            for uri in open {
                this.check_and_publish(uri).await;
            }
        });
    }
}

/// What a blocking thread handed back, or `None` if it panicked.
///
/// A panic on a blocking thread is not an answer anybody can act on, but it is
/// not nothing either: the request answers with nothing found, and the log
/// says which piece of work failed and what the panic said, so the empty
/// answer is not a mystery.
async fn joined<T>(
    client: &Client,
    what: &str,
    outcome: std::result::Result<T, tokio::task::JoinError>,
) -> Option<T> {
    match outcome {
        Ok(value) => Some(value),
        Err(error) => {
            client
                .log_message(
                    MessageType::ERROR,
                    format!("camello: {what} failed: {error}"),
                )
                .await;
            None
        }
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let folders: Vec<PathBuf> = params
            .workspace_folders
            .iter()
            .flatten()
            .filter_map(|folder| folder.uri.to_file_path().map(|path| path.into_owned()))
            .collect();
        let root = folders
            .first()
            .cloned()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();

        // UTF-16 unless the client asked for bytes. LSP 3.17 allows `utf-8`,
        // and where a client speaks it the conversion degenerates to the line
        // table alone (`docs/lsp.md`, "Documents and positions").
        let encoding = params
            .capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref())
            .filter(|kinds| kinds.contains(&PositionEncodingKind::UTF8))
            .map_or(Encoding::Utf16, |_| Encoding::Utf8);

        let dynamic_watchers = params
            .capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.did_change_watched_files.as_ref())
            .and_then(|watched| watched.dynamic_registration)
            .unwrap_or(false);

        let (settings, problems) = Settings::load(&root, &folders);
        {
            let mut state = self.write();
            let index = Arc::new(RwLock::new(index::Index::empty(&settings)));
            state.settings = Arc::new(settings);
            state.encoding = encoding;
            state.dynamic_watchers = dynamic_watchers;
            state.index = index;
        }
        for problem in problems {
            self.client
                .log_message(MessageType::WARNING, format!("camello: {problem}"))
                .await;
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: crate::SERVER_NAME.to_string(),
                version: Some(crate::SERVER_VERSION.to_string()),
            }),
            capabilities: ServerCapabilities {
                position_encoding: Some(match encoding {
                    Encoding::Utf8 => PositionEncodingKind::UTF8,
                    Encoding::Utf16 => PositionEncodingKind::UTF16,
                }),
                // Full, not incremental: the parser is fast enough that a
                // reparse is not the bottleneck for a file a human is
                // editing, and incremental text patching is the classic first
                // source of silent corruption in a young server. Revisit only
                // with a measurement in hand.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![handlers::completion::TRIGGER.to_string()]),
                    ..CompletionOptions::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            // Superseded by `position_encoding` above; a client that reads
            // the old field learns nothing from a guess.
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.register_watchers();
        self.spawn_index();
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        self.store(&document.uri, &document.text, document.version);
        let uri = document.uri;
        self.in_background(|this| async move { this.check_and_publish(uri).await });
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Full sync: one change, and it is the whole buffer.
        let Some(change) = params.content_changes.into_iter().next_back() else {
            return;
        };
        let uri = params.text_document.uri;
        self.store(&uri, &change.text, params.text_document.version);
        let generation = self.write().bump(&uri);
        self.in_background(|this| async move { this.debounced_check(uri, generation).await });
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        // Immediately, not after the debounce: a save is a moment the user
        // chose, and waiting through it would be the server ignoring them.
        let uri = params.text_document.uri;
        self.in_background(|this| async move { this.check_and_publish(uri).await });
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let path = uri.to_file_path().map(|path| path.into_owned());
        {
            // A file nobody is looking at has no diagnostics to look at, and
            // no check still running for it may say otherwise afterwards.
            let _turn = self.publishing.lock().await;
            self.write().forget(&uri);
            self.client.publish_diagnostics(uri, Vec::new(), None).await;
        }
        // While it was open the graph held the *buffer's* declarations, which
        // may be an edit the user has just thrown away by closing without
        // saving. Nothing else would ever put the file back — a closed file
        // sends no more `didChange` — so the phantom would outlive the buffer
        // it came from.
        if let Some(path) = path {
            self.in_background(|this| async move { this.reinstall_from_disk(path).await });
        }
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let mut reload = false;
        let mut changed: Vec<PathBuf> = Vec::new();
        for event in params.changes {
            let Some(path) = event.uri.to_file_path() else {
                continue;
            };
            if path.file_name().is_some_and(|name| {
                [
                    camello_sema::config::FILE_NAME,
                    camello_sema::workspace::IGNORE_FILE,
                    ".gitignore",
                ]
                .iter()
                .any(|reloads| name == std::ffi::OsStr::new(reloads))
            }) {
                reload = true;
            } else {
                changed.push(path.into_owned());
            }
        }

        if reload {
            let folders = {
                let state = self.read();
                (state.settings.roots.clone(), state.settings.root.clone())
            };
            let (settings, problems) = Settings::load(&folders.1, &folders.0);
            self.write().settings = Arc::new(settings);
            for problem in problems {
                self.client
                    .log_message(MessageType::WARNING, format!("camello: {problem}"))
                    .await;
            }
            // The dialect and the stub roots may have moved, and both are
            // read during the declaration pass — so the graph is rebuilt
            // rather than patched. An ignore file moves which files the walk
            // finds, which is the same rebuild.
            self.spawn_index();
            return;
        }

        // What is on disk is the truth for every file *except* the ones the
        // user has open: there the buffer is, and a `didChange` for it may not
        // even have been sent yet. Taking disk for an open file would install
        // declarations the editor is not showing anybody.
        let held = self.read().open_paths();
        changed.retain(|path| !held.contains(path));
        if changed.is_empty() {
            return;
        }
        let settings = Arc::clone(&self.read().settings);
        let index = Arc::clone(&self.read().index);
        self.in_background(|this| async move {
            let outcome = tokio::task::spawn_blocking(move || {
                let cache = crate::settings::cache(settings.cache_dir.as_deref());
                let read: Vec<(PathBuf, String)> = changed
                    .into_iter()
                    .filter_map(|path| {
                        let source = std::fs::read_to_string(&path).ok()?;
                        Some((path, source))
                    })
                    .collect();
                let files = read
                    .iter()
                    .map(|(path, source)| {
                        let decls = camello_sema::read_declarations(
                            path,
                            source,
                            &settings.dialect,
                            &cache,
                        );
                        (path.as_path(), decls, source.as_str())
                    })
                    .collect();
                let mut index = state::write(&index);
                // The same decl-diff an edit applies: a file that was touched, or
                // rewritten with the same declarations, is nobody else's business,
                // and a sweep of the open files for it would be a sweep for
                // nothing.
                index.apply(files)
            })
            .await;
            if joined(&this.client, "reading changed files", outcome).await == Some(true) {
                this.sweep(None).await;
            }
        });
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        Ok(self
            .with_analysis(
                params.text_document_position_params,
                |snapshot, tables, context, offset| {
                    handlers::hover::hover(&snapshot.document, tables, context, offset)
                },
            )
            .await
            .flatten())
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(self
            .with_analysis(
                params.text_document_position,
                |snapshot, tables, context, offset| {
                    handlers::completion::completion(
                        &snapshot.document,
                        tables,
                        snapshot.clean_tables.as_deref(),
                        context,
                        offset,
                    )
                },
            )
            .await
            .map(CompletionResponse::Array))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let open: Vec<Arc<Document>> = self.read().documents.values().map(Arc::clone).collect();
        Ok(self
            .with_analysis(
                params.text_document_position_params,
                move |snapshot, tables, context, offset| {
                    handlers::definition::definition(
                        &snapshot.document,
                        &uri,
                        tables,
                        context,
                        offset,
                        snapshot.encoding,
                        &handlers::definition::Buffers {
                            open: &open,
                            dialect: &snapshot.settings.dialect,
                        },
                    )
                },
            )
            .await
            .flatten()
            .map(GotoDefinitionResponse::Scalar))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(snapshot) = self.read().snapshot(&params.text_document.uri) else {
            return Ok(None);
        };
        let outcome =
            tokio::task::spawn_blocking(move || handlers::symbols::symbols(&snapshot.document))
                .await;
        let symbols = joined(&self.client, "the outline", outcome)
            .await
            .unwrap_or_default();
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some(snapshot) = self.read().snapshot(&params.text_document.uri) else {
            return Ok(None);
        };
        let outcome =
            tokio::task::spawn_blocking(move || handlers::formatting::formatting(&snapshot)).await;
        Ok(joined(&self.client, "formatting", outcome).await.flatten())
    }
}
