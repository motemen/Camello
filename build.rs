//! Names the build in `camello --version`: the nearest `v*` tag, the commits
//! since it and whether the tree was dirty, as `git describe` puts it — so a
//! binary built past a release says so. Where git cannot say, outside a
//! checkout or in a shallow clone without the tag, it is Cargo.toml's version.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let out = String::from_utf8(output.stdout).ok()?;
    Some(out.trim().to_string())
}

fn main() {
    let version = git(&["describe", "--tags", "--match", "v*", "--dirty"])
        .map(|describe| describe.trim_start_matches('v').to_string())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=CAMELLO_VERSION={version}");

    println!("cargo:rerun-if-changed=build.rs");
    // `--dirty` is about the sources, so an edit to them asks again.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=crates");
    println!("cargo:rerun-if-changed=Cargo.toml");
    // A commit moves HEAD or the branch it points at, and a new tag lands in
    // refs/. packed-refs is left out: a path that does not exist reruns the
    // script on every build, and a clone may have none. In a worktree HEAD is
    // its own and the refs are the main checkout's.
    if let Some(git_dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
    }
    if let Some(common_dir) = git(&["rev-parse", "--git-common-dir"]) {
        println!("cargo:rerun-if-changed={common_dir}/refs");
    }
}
