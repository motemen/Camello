#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use camello::cli;

fn main() {
    if let Err(err) = cli::run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
