//! `camello lsp`: the editor half of the checker (`docs/lsp.md`).
//!
//! A Language Server Protocol server over the machinery camello already has —
//! the lossless CST from `camello-syntax`, the formatter from `camello-fmt`,
//! and the two-phase checker from `camello-sema`, whose "Incremental
//! reanalysis" section left a door open for exactly this.
//!
//! ```text
//! stdin/stdout  ── tower-lsp-server ──▶ server.rs      the protocol
//!                                       state.rs       what is open, and the graph
//!                                       document.rs    green trees, per version
//!                                       position.rs    byte offsets ⇄ line/character
//!                                       index.rs       the background declaration walk
//!                                       analysis.rs    one file's diagnostics and tables
//!                                       handlers/      hover, completion, symbols, …
//! ```
//!
//! This is the first crate to see both sides — `sema` *and* `fmt` — and the
//! rule that matters is unchanged and still Cargo-enforced: nothing under
//! `sema` reaches `fmt`. An LSP sits above both, so it may see both, the same
//! way the root crate does.
//!
//! Tokio is here to shuttle JSON-RPC and for nothing else. All parsing and
//! analysis is CPU-bound and runs on blocking threads; no analysis code
//! becomes async, and no async type appears in `camello-sema` or below.

pub mod analysis;
pub mod bar;
pub mod document;
pub mod handlers;
pub mod index;
pub mod position;
pub mod server;
pub mod settings;
pub mod state;

pub use server::Backend;

#[cfg(test)]
mod tests;

/// Serve the protocol over standard input and output until the client says to
/// stop.
///
/// The runtime is built here rather than by a `#[tokio::main]` on the binary,
/// so that the whole of the async world stays inside this crate: `camello`'s
/// `main` is still a plain function that dispatches a subcommand.
pub fn run() -> std::io::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = tower_lsp_server::LspService::new(Backend::new);
        tower_lsp_server::Server::new(stdin, stdout, socket)
            .serve(service)
            .await;
    });
    Ok(())
}

/// What the server tells a client it is, in `initialize`'s `serverInfo`.
pub const SERVER_NAME: &str = "camello";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
