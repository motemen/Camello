// Opt-in; Cargo.toml says why it is not on by default.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use camello::cli;

fn main() {
    if let Err(err) = cli::run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
