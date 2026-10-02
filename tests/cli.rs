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

/// Source the formatter changes, so that whether a file was touched shows.
const UNFORMATTED: &str = "my$x=1;\n";

/// A tree of `UNFORMATTED` files, at the paths given, under a fresh directory.
fn tree(files: &[&str]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    for file in files {
        let path = directory.path().join(file);
        std::fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("failed to make the fixture's directory");
        std::fs::write(&path, UNFORMATTED).expect("failed to write the fixture");
    }
    directory
}

fn write(directory: &Path, file: &str, text: &str) {
    std::fs::write(directory.join(file), text).expect("failed to write the fixture");
}

fn touched(directory: &Path, file: &str) -> bool {
    std::fs::read_to_string(directory.join(file)).expect("failed to read the fixture back")
        != UNFORMATTED
}

/// `.gitignore` says what a walk does not descend into, and nothing about a
/// file somebody named.
#[test]
fn gitignore_applies_beneath_a_named_path_and_not_to_it() {
    let directory = tree(&["lib/Kept.pm", "lib/Built.pm"]);
    write(directory.path(), ".gitignore", "Built.pm\n");

    let output = camello(directory.path(), &["format", "lib"]);
    assert!(output.status.success(), "{output:?}");
    assert!(touched(directory.path(), "lib/Kept.pm"));
    assert!(
        !touched(directory.path(), "lib/Built.pm"),
        "a gitignored file was walked into"
    );

    let output = camello(directory.path(), &["format", "lib/Built.pm"]);
    assert!(output.status.success(), "{output:?}");
    assert!(
        touched(directory.path(), "lib/Built.pm"),
        "a named file was not formatted"
    );
}

/// `.camelloignore` says what camello is not to touch, and naming the file is
/// no exception — but the run says so, and it is not a failure.
#[test]
fn camelloignore_applies_to_a_named_file_too_and_says_so() {
    let directory = tree(&["lib/Kept.pm", "lib/Gen/Table.pm"]);
    write(directory.path(), ".camelloignore", "lib/Gen/\n");

    let output = camello(directory.path(), &["format", "lib"]);
    assert!(output.status.success(), "{output:?}");
    assert!(touched(directory.path(), "lib/Kept.pm"));
    assert!(!touched(directory.path(), "lib/Gen/Table.pm"));

    for named in ["lib/Gen/Table.pm", "lib/Gen"] {
        let output = camello(directory.path(), &["format", named]);
        assert!(output.status.success(), "{output:?}");
        assert!(
            !touched(directory.path(), "lib/Gen/Table.pm"),
            "{named} was formatted"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("ignored by `lib/Gen/`"),
            "{named}: the run did not say why: {stderr}"
        );
    }

    // To standard output, what comes out is what went in.
    let output = camello(directory.path(), &["format", "lib/Gen/Table.pm", "-o", "-"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), UNFORMATTED);
}

/// The nearest `.camelloignore` decides, the way a nested `.gitignore` does.
#[test]
fn a_nested_camelloignore_overrides_the_one_above_it() {
    let directory = tree(&["lib/Gen/Table.pm", "lib/Gen/Kept.pm"]);
    write(directory.path(), ".camelloignore", "lib/Gen/*.pm\n");
    write(directory.path(), "lib/Gen/.camelloignore", "!Kept.pm\n");

    let output = camello(directory.path(), &["format", "."]);
    assert!(output.status.success(), "{output:?}");
    assert!(!touched(directory.path(), "lib/Gen/Table.pm"));
    assert!(touched(directory.path(), "lib/Gen/Kept.pm"));

    let directory = tree(&["lib/Gen/Kept.pm"]);
    write(directory.path(), ".camelloignore", "lib/Gen/*.pm\n");
    write(directory.path(), "lib/Gen/.camelloignore", "!Kept.pm\n");
    let output = camello(directory.path(), &["format", "lib/Gen/Kept.pm"]);
    assert!(output.status.success(), "{output:?}");
    assert!(touched(directory.path(), "lib/Gen/Kept.pm"));
}

/// `--exclude` is this run's `.camelloignore`, relative to where it runs.
#[test]
fn exclude_leaves_out_what_it_matches_walked_or_named() {
    let directory = tree(&["lib/Kept.pm", "lib/Table.gen.pm"]);

    let output = camello(
        directory.path(),
        &["format", "lib", "--exclude", "*.gen.pm"],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(touched(directory.path(), "lib/Kept.pm"));
    assert!(!touched(directory.path(), "lib/Table.gen.pm"));

    let output = camello(
        directory.path(),
        &["format", "lib/Table.gen.pm", "--exclude", "lib/*.gen.pm"],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(!touched(directory.path(), "lib/Table.gen.pm"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--exclude `lib/*.gen.pm`"));
}

/// `--no-ignore` sets the files aside; what was typed for this run stays.
#[test]
fn no_ignore_reads_no_ignore_file_but_keeps_exclude() {
    let directory = tree(&["lib/Built.pm", "lib/Gen.pm", "lib/Table.gen.pm"]);
    write(directory.path(), ".gitignore", "Built.pm\n");
    write(directory.path(), ".camelloignore", "Gen.pm\n");

    let output = camello(
        directory.path(),
        &["format", "lib", "--no-ignore", "--exclude", "*.gen.pm"],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(touched(directory.path(), "lib/Built.pm"));
    assert!(touched(directory.path(), "lib/Gen.pm"));
    assert!(!touched(directory.path(), "lib/Table.gen.pm"));
}

/// `check` walks the way `format` does.
#[test]
fn check_leaves_out_what_format_does() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir(directory.path().join("lib")).expect("failed to make lib");
    write(directory.path(), "lib/Gen.pm", "my $unused = 1;\n");
    write(directory.path(), ".camelloignore", "Gen.pm\n");

    let output = camello(
        directory.path(),
        &["check", "lib", "--min-severity", "info"],
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("Gen.pm"),
        "{output:?}"
    );

    let output = camello(
        directory.path(),
        &["check", "lib", "--min-severity", "info", "--no-ignore"],
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Gen.pm"),
        "{output:?}"
    );
}

/// A `.camelloignore` line that does not parse is an error, not a rule dropped.
#[test]
fn a_camelloignore_that_does_not_parse_is_an_error() {
    let directory = tree(&["lib/Gen.pm"]);
    write(directory.path(), "lib/.camelloignore", "Gen{.pm\n");

    let output = camello(directory.path(), &["format", "."]);
    assert!(!output.status.success(), "{output:?}");
    assert!(!touched(directory.path(), "lib/Gen.pm"));

    let output = camello(directory.path(), &["format", "lib/Gen.pm"]);
    assert!(!output.status.success(), "{output:?}");
    assert!(!touched(directory.path(), "lib/Gen.pm"));
}
