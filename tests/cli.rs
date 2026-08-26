//! What the command line does, asked of the command line.
//!
//! These run the binary. The decisions under test end in `std::process::exit`,
//! and an exit status is not a thing a unit test can be told about — a test in
//! the same process that reached one would take the test runner with it.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Source no parser can make sense of, so that every run of it reports.
const UNPARSABLE: &str = "my $foo = ;\nsub {\n";

fn camello(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_camello"))
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("failed to run camello")
}

fn camello_with_stdin(directory: &Path, arguments: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_camello"))
        .args(arguments)
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run camello");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(input.as_bytes())
        .expect("failed to write to camello");
    child.wait_with_output().expect("failed to run camello")
}

/// A source the parser reports on is left alone however it was handed over.
///
/// One path is the way an editor formatting on save and a pre-commit hook both
/// ask, so it is the path a best-effort rewrite of an unparsed file takes in
/// practice — and it was the one path that took it.
#[test]
fn one_unparsable_file_is_left_alone() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("bad.pl");
    std::fs::write(&path, UNPARSABLE).expect("failed to write the fixture");

    let output = camello(directory.path(), &["format", "bad.pl"]);

    assert!(!output.status.success(), "an unparsed file exits non-zero");
    assert_eq!(
        std::fs::read_to_string(&path).expect("failed to read the fixture back"),
        UNPARSABLE,
        "the file was rewritten"
    );
}

/// A run over several paths already left it alone, and still does.
#[test]
fn an_unparsable_file_beside_a_good_one_is_left_alone() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(directory.path().join("bad.pl"), UNPARSABLE).expect("failed to write");
    std::fs::write(directory.path().join("good.pl"), "my $foo=1;\n").expect("failed to write");

    let output = camello(directory.path(), &["format", "bad.pl", "good.pl"]);

    assert!(!output.status.success());
    assert_eq!(
        std::fs::read_to_string(directory.path().join("bad.pl")).expect("failed to read"),
        UNPARSABLE
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("good.pl")).expect("failed to read"),
        "my $foo = 1;\n",
        "the file beside it is still formatted"
    );
}

/// Standard input has no file to be left alone in, so what comes out is what
/// went in — "left alone" says the same thing wherever the result was going.
#[test]
fn unparsable_standard_input_comes_back_unchanged() {
    let directory = tempfile::tempdir().expect("a temporary directory");

    let output = camello_with_stdin(directory.path(), &["format"], UNPARSABLE);

    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("output is utf-8"),
        UNPARSABLE
    );
}

/// The same for a file sent to standard output by name.
#[test]
fn an_unparsable_file_sent_to_stdout_comes_back_unchanged() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(directory.path().join("bad.pl"), UNPARSABLE).expect("failed to write");

    let output = camello(directory.path(), &["format", "bad.pl", "-o", "-"]);

    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("output is utf-8"),
        UNPARSABLE
    );
}

/// `--check` asks a question and writes nothing, so an unparsed file answers
/// with its diagnostics and the exit status, and nothing on standard output for
/// a pipeline to read as a name.
#[test]
fn check_on_an_unparsable_file_names_nothing_and_fails() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(directory.path().join("bad.pl"), UNPARSABLE).expect("failed to write");

    let output = camello(directory.path(), &["format", "--check", "bad.pl"]);

    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("output is utf-8"),
        "",
        "an unparsed file is not a file that would be reformatted"
    );
}

/// A file that parses is formatted over itself, which is the point of all this.
#[test]
fn one_good_file_is_still_formatted() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("good.pl");
    std::fs::write(&path, "my $foo=1;\n").expect("failed to write");

    let output = camello(directory.path(), &["format", "good.pl"]);

    assert!(output.status.success());
    assert_eq!(
        std::fs::read_to_string(&path).expect("failed to read"),
        "my $foo = 1;\n"
    );
}
