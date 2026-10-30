// Opt-in; Cargo.toml says why it is not on by default.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use camello::cli;

/// The one place the process ends. An error is printed through miette's
/// report handler — the same rendering a parse error already gets — and exits 1.
fn main() -> miette::Result<std::process::ExitCode> {
    cli::run().map(Into::into)
}
