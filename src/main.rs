// Opt-in; Cargo.toml says why it is not on by default.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use camello::cli;

fn main() -> std::process::ExitCode {
    match cli::run() {
        Ok(exit) => exit.into(),
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}
