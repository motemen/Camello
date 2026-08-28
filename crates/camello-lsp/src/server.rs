//! The protocol surface (`docs/lsp.md`, "The crate and the runtime").
//!
//! `tower-lsp-server` — the maintained community fork of `tower-lsp` — is a
//! deliberate exception to camello's no-async, few-dependencies habit: the
//! alternative was `lsp-server` plus a hand-rolled main loop, and the trade
//! taken was the trait. Implement [`LanguageServer`], get the plumbing.
//!
//! The discipline that keeps the cost contained is the whole of this file's
//! shape: **tokio exists to shuttle JSON-RPC, and nothing else.** Every
//! handler here does three things and no more — take a snapshot under the
//! lock, hand it to `spawn_blocking`, spell the answer in LSP. All parsing and
//! analysis is CPU-bound and runs on a blocking thread; no analysis code is
//! async, and no async type reaches `camello-sema`.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer};

use crate::analysis;
use crate::document::Document;
use crate::handlers;
use crate::index;
use crate::position::Encoding;
use crate::settings::Settings;
use crate::state::GlobalState;

/// How long after a keystroke the checker runs.
///
/// Feedback while typing, not only on save — and not on every keystroke
/// either, which for a file of any size is a body pass thrown away before it
/// finishes (`docs/lsp.md`, "Diagnostics").
const DEBOUNCE: Duration = Duration::from_millis(300);

pub struct Backend {
    client: Client,
    state: Arc<RwLock<GlobalState>>,
}

impl Backend {
    #[must_use]
    pub fn new(client: Client) -> Self {
        let (settings, _) = Settings::load(&std::env::current_dir().unwrap_or_default(), &[]);
        Backend {
            client,
            state: Arc::new(RwLock::new(GlobalState::new(settings))),
        }
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, GlobalState> {
        self.state.read().expect("no writer panics holding this")
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, GlobalState> {
        self.state.write().expect("no writer panics holding this")
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
    async fn check_and_publish(&self, uri: Uri) {
        publish_for(&self.state, &self.client, uri).await;
    }

    /// Wait out the debounce, then run the edit loop — unless a newer edit
    /// arrived meanwhile, in which case that one's timer is already running
    /// and this text is not what anybody is looking at.
    ///
    /// The five steps of "Incremental reanalysis" are here in order: the
    /// reparse happened in the document store, the declaration pass and the
    /// diff are [`Backend::reindex_if_declarations_changed`], and the body
    /// pass is the publication.
    async fn schedule_check(&self, uri: Uri) {
        let generation = self.write().bump(&uri);
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
        let state = Arc::clone(&self.state);
        let outcome = tokio::task::spawn_blocking(move || {
            let cache = crate::settings::cache(snapshot.settings.cache_dir.as_deref());
            let decls = index::declarations(
                &path,
                &snapshot.document.text,
                &snapshot.settings.dialect,
                &cache,
            );
            let fingerprint = index::fingerprint(&decls);
            (path, decls, fingerprint)
        })
        .await;
        let Ok((path, decls, fingerprint)) = outcome else {
            return;
        };

        // The first edit after a file is opened has no remembered
        // fingerprint, and the graph's own declarations are what it should be
        // compared against — otherwise every freshly opened file's first
        // keystroke would sweep every other open file.
        let held = {
            let state = state.read().expect("no writer panics holding this");
            let remembered = state.fingerprints.get(&path).cloned();
            let index = Arc::clone(&state.index);
            drop(state);
            remembered.or_else(|| {
                let index = index.read().expect("no writer panics holding this");
                let program = index.analysis.program();
                let file = program.index_of(&path)?;
                Some(index::fingerprint(&program.file(file)?.decls))
            })
        };
        let changed = held.as_deref() != Some(fingerprint.as_str());
        state
            .write()
            .expect("no writer panics holding this")
            .fingerprints
            .insert(path.clone(), fingerprint);
        if !changed {
            return;
        }
        {
            let index = Arc::clone(&self.read().index);
            let mut index = index.write().expect("no reader panics holding this");
            index.analysis.replace(&path, decls, true);
            index.analysis.link();
        }
        // Every *open* file, and no other: nobody is told about a broken
        // caller in a file nobody is looking at — that is `camello check`'s
        // job in CI, not the editor's.
        let open = { self.read().open_uris() };
        for other in open {
            if &other == uri {
                continue;
            }
            self.check_and_publish(other).await;
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
            // carry, and the configuration — which reloads and relinks.
            let watchers = [
                "**/*.pl",
                "**/*.pm",
                "**/*.t",
                "**/*.psgi",
                "**/camello.toml",
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
    /// built.
    fn spawn_index(&self) {
        let state = Arc::clone(&self.state);
        let client = self.client.clone();
        let settings = Arc::clone(&self.read().settings);
        let index = Arc::clone(&self.read().index);
        tokio::spawn(async move {
            let built = tokio::task::spawn_blocking(move || index::build(&settings)).await;
            let Ok(built) = built else { return };
            let files = built.files;
            {
                let mut slot = index.write().expect("no reader panics holding this");
                *slot = built;
            }
            client
                .log_message(
                    MessageType::INFO,
                    format!("camello: indexed {files} file{}", plural(files)),
                )
                .await;
            // Everything already open was answered from single-file analysis
            // while the walk ran; now that the graph is there, they get the
            // cross-file half of the answer.
            let open = {
                let held = state.read().expect("no writer panics holding this");
                held.open_uris()
            };
            for uri in open {
                publish_for(&state, &client, uri).await;
            }
        });
    }
}

/// Check one document and publish what came out.
///
/// A free function rather than a method because the background index walk
/// republishes the open files when the graph arrives, and it has an `Arc` of
/// the state rather than a `&Backend`.
///
/// The version is carried through: an edit landing while this runs makes the
/// answer stale, and a stale answer is dropped here rather than shown.
async fn publish_for(state: &Arc<RwLock<GlobalState>>, client: &Client, uri: Uri) {
    let snapshot = {
        let held = state.read().expect("no writer panics holding this");
        held.snapshot(&uri)
    };
    let Some(snapshot) = snapshot else { return };
    let version = snapshot.document.version;
    let outcome = tokio::task::spawn_blocking(move || {
        let index = snapshot
            .index
            .read()
            .expect("no writer panics holding this");
        let context = analysis::context(&snapshot.document, &index, &snapshot.settings);
        let found = analysis::analyse(&snapshot.document, &context, &snapshot.settings, true);
        let published = handlers::diagnostics::publish(&snapshot.document, &found.diagnostics);
        (published, found.tables)
    })
    .await;
    let Ok((published, tables)) = outcome else {
        return;
    };
    {
        let mut held = state.write().expect("no writer panics holding this");
        // Still the version that was analysed? If not, the newer text has its
        // own debounce already running and this answer is about text nobody is
        // looking at.
        if held
            .documents
            .get(&uri)
            .is_none_or(|document| document.version != version)
        {
            return;
        }
        held.remember(&uri, tables);
    }
    client
        .publish_diagnostics(uri, published, Some(version))
        .await;
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
        self.check_and_publish(document.uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Full sync: one change, and it is the whole buffer.
        let Some(change) = params.content_changes.into_iter().next_back() else {
            return;
        };
        let uri = params.text_document.uri;
        self.store(&uri, &change.text, params.text_document.version);
        self.schedule_check(uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        // Immediately, not after the debounce: a save is a moment the user
        // chose, and waiting through it would be the server ignoring them.
        self.check_and_publish(params.text_document.uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.write().forget(&uri);
        // A file nobody is looking at has no diagnostics to look at.
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let mut reload = false;
        let mut changed: Vec<PathBuf> = Vec::new();
        for event in params.changes {
            let Some(path) = event.uri.to_file_path() else {
                continue;
            };
            if path
                .file_name()
                .is_some_and(|name| name == std::ffi::OsStr::new(camello_sema::config::FILE_NAME))
            {
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
            // rather than patched.
            self.spawn_index();
            return;
        }

        if changed.is_empty() {
            return;
        }
        let settings = Arc::clone(&self.read().settings);
        let index = Arc::clone(&self.read().index);
        let updated = tokio::task::spawn_blocking(move || {
            let cache = crate::settings::cache(settings.cache_dir.as_deref());
            let mut updated = Vec::new();
            for path in changed {
                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let decls = index::declarations(&path, &source, &settings.dialect, &cache);
                updated.push((path, decls));
            }
            let mut index = index.write().expect("no reader panics holding this");
            // The same decl-diff an edit applies: a file that was touched, or
            // rewritten with the same declarations, is nobody else's business,
            // and a sweep of the open files for it would be a sweep for
            // nothing.
            let mut any = false;
            for (path, decls) in updated {
                let before = index
                    .analysis
                    .program()
                    .index_of(&path)
                    .and_then(|file| index.analysis.program().file(file))
                    .map(|entry| index::fingerprint(&entry.decls));
                let after = index::fingerprint(&decls);
                any |= before.as_deref() != Some(after.as_str());
                index.analysis.replace(&path, decls, true);
            }
            if any {
                index.analysis.link();
            }
            any
        })
        .await;
        if matches!(updated, Ok(true)) {
            let open = { self.read().open_uris() };
            for uri in open {
                self.check_and_publish(uri).await;
            }
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params;
        let Some(snapshot) = self.read().snapshot(&position.text_document.uri) else {
            return Ok(None);
        };
        let offset = snapshot.document.positions.offset(position.position);
        Ok(tokio::task::spawn_blocking(move || {
            let tables = snapshot.tables_now();
            let index = snapshot
                .index
                .read()
                .expect("no writer panics holding this");
            let context = analysis::context(&snapshot.document, &index, &snapshot.settings);
            handlers::hover::hover(&snapshot.document, &tables, &context, offset)
        })
        .await
        .unwrap_or(None))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let position = params.text_document_position;
        let Some(snapshot) = self.read().snapshot(&position.text_document.uri) else {
            return Ok(None);
        };
        let offset = snapshot.document.positions.offset(position.position);
        let items = tokio::task::spawn_blocking(move || {
            let tables = snapshot.tables_now();
            let fallback = snapshot.clean_tables.clone();
            let index = snapshot
                .index
                .read()
                .expect("no writer panics holding this");
            let context = analysis::context(&snapshot.document, &index, &snapshot.settings);
            handlers::completion::completion(
                &snapshot.document,
                &tables,
                fallback.as_deref(),
                &context,
                offset,
            )
        })
        .await
        .unwrap_or_default();
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params;
        let uri = position.text_document.uri.clone();
        let Some(snapshot) = self.read().snapshot(&uri) else {
            return Ok(None);
        };
        let encoding = self.read().encoding;
        let offset = snapshot.document.positions.offset(position.position);
        let found = tokio::task::spawn_blocking(move || {
            let tables = snapshot.tables_now();
            let index = snapshot
                .index
                .read()
                .expect("no writer panics holding this");
            let context = analysis::context(&snapshot.document, &index, &snapshot.settings);
            handlers::definition::definition(
                &snapshot.document,
                &uri,
                &tables,
                &context,
                offset,
                encoding,
            )
        })
        .await
        .unwrap_or(None);
        Ok(found.map(GotoDefinitionResponse::Scalar))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(snapshot) = self.read().snapshot(&params.text_document.uri) else {
            return Ok(None);
        };
        let symbols =
            tokio::task::spawn_blocking(move || handlers::symbols::symbols(&snapshot.document))
                .await
                .unwrap_or_default();
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some(snapshot) = self.read().snapshot(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(
            tokio::task::spawn_blocking(move || handlers::formatting::formatting(&snapshot))
                .await
                .unwrap_or(None),
        )
    }
}
