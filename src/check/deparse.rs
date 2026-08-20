//! perl as the oracle: does perl read the output as the same program?
//!
//! Everything in the parent module is a property camello can assert about
//! itself, which is also the limit of what it can see. A token stream says the
//! same tokens came out in the same order; it cannot say that `${^MATCH}` and
//! `${^ MATCH}` are different variables, or that a comment migrated into a
//! replacement string. Deparsing asks perl what it read.
//!
//! This is the only check that runs another program, and `perl -c` executes
//! `BEGIN` blocks — which is to say it runs arbitrary code out of the file being
//! checked. That is why it is `dev ask-perl`, a command of its own: opting in is
//! the command typed, not a flag that some other run could carry along.
//!
//! The normalisation below is `scripts/corpus-check`'s, in Rust: B::Deparse
//! emits a few kinds of line from a hash walk, so two deparses of the *same*
//! file can differ in their order.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// How long any one perl invocation is given before it is killed.
const TIMEOUT: Duration = Duration::from_secs(60);

/// The reason a corpus run is mostly unanswered, and the one worth telling the
/// reader what to do about: a file checked out of the tree it was installed in
/// cannot find what it uses.
pub const NOT_IN_INC: &str = "a module it uses is not in @INC";

/// The file compiled and means nothing at run time — POD, `package`, `use`,
/// forward declarations, and no ops under any of it. There is no program here
/// for perl to read differently, so there is nothing to ask about the output.
const NO_OPS: &str = "the input deparses to nothing: no runtime ops in it";

/// perl never ran. Not something a file can be blamed for: no perl on PATH any
/// more, or no room to start a process.
const COULD_NOT_RUN: &str = "perl could not be run";

/// perl started and would not stop, and was killed at the timeout.
const TIMED_OUT: &str = "perl did not finish before the timeout";

/// `perl -c` was happy and `B::Deparse` was not — it exited non-zero, or the
/// file's own `BEGIN` time did.
const WOULD_NOT_DEPARSE: &str = "perl would not deparse it";

/// What to do about a reason, where there is something to do.
///
/// perl runs with this process's environment, so the fix is the ordinary one
/// and it needs no flag from us.
#[must_use]
pub fn hint(why: &str) -> Option<&'static str> {
    match why {
        NOT_IN_INC => {
            Some("perl inherits this shell's environment: PERL5LIB=<the tree's lib> answers these")
        }
        NO_OPS => Some("nothing camello can do to such a file changes what it means"),
        COULD_NOT_RUN => Some("perl was there when the run started; it is not being reached now"),
        _ => None,
    }
}

/// What perl had to say about a pair.
pub enum Verdict {
    /// perl reads the two as the same program.
    Same,
    /// perl declined to load the input, so nothing here is about the formatter:
    /// a missing dependency, an XS bootstrap, a `$VERSION` check. `why` is the
    /// class, for counting; `detail` is what perl actually said, because "perl
    /// cannot load the input" against a third of a corpus is a number the
    /// reader can do nothing with.
    NotLoadable { why: &'static str, detail: String },
    /// perl reads them as different programs, or will not read the output.
    Differs { summary: String, detail: String },
}

/// Is there a perl to ask?
pub fn available() -> bool {
    Command::new("perl")
        .arg("-e0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Ask perl whether `formatted` is the program `source` was.
pub fn meaning(source: &str, formatted: &str) -> Verdict {
    let Some(work) = Workspace::new() else {
        return Verdict::NotLoadable {
            why: "a working directory could not be made",
            detail: String::new(),
        };
    };

    // A module that will not load in isolation tells us nothing about the
    // formatter, so it drops out here rather than counting against it. What
    // perl said comes along: the answer is usually a dependency this file was
    // separated from, and that is worth knowing before reading anything into
    // the size of the n/a column.
    let before = match work.ask(source) {
        Asked::Deparsed(lines) => lines,
        Asked::Failed(errors) => {
            return Verdict::NotLoadable {
                why: if errors.contains("Can't locate ") && errors.contains("in @INC") {
                    NOT_IN_INC
                } else {
                    "perl cannot load the input"
                },
                detail: evidence(&errors),
            }
        }
        Asked::Silent => {
            return Verdict::NotLoadable {
                why: NO_OPS,
                detail: String::new(),
            }
        }
        // perl not answering is not the file's doing, on either side of the
        // pair: it is this check going missing, and it says which way.
        Asked::Unanswered(NoAnswer { why, detail }) => return Verdict::NotLoadable { why, detail },
    };

    let after = match work.ask(formatted) {
        Asked::Deparsed(lines) => lines,
        Asked::Failed(errors) => {
            return Verdict::Differs {
                summary: "perl rejects the output".to_string(),
                detail: evidence(&errors),
            }
        }
        // The input had ops — it is above — so an output that has none is
        // camello having deleted the program, and that is a violation.
        Asked::Silent => {
            return Verdict::Differs {
                summary: "perl deparsed the output to nothing".to_string(),
                detail: String::new(),
            }
        }
        Asked::Unanswered(NoAnswer { why, detail }) => return Verdict::NotLoadable { why, detail },
    };

    if before == after {
        return Verdict::Same;
    }

    Verdict::Differs {
        summary: format!(
            "perl reads the output as a different program ({} lines in, {} out)",
            before.len(),
            after.len()
        ),
        detail: super::describe_divergence(
            &before,
            &after,
            &super::Report {
                unit: "deparsed line",
                sides: ("input", "output"),
                base: 1,
            },
            |line| (String::new(), line.clone()),
        ),
    }
}

/// One temporary file, asked twice.
///
/// Both texts are written to the *same* path, one after the other, rather than
/// to two files in sibling directories. perl's answer carries the path it read
/// the file from — `__FILE__`, and a `#line` directive built out of it, which
/// is a thing real modules do — so two paths would deparse to two different
/// programs for no reason at all.
///
/// That is also what lets perl run from wherever camello was run from.
/// Changing into the temporary directory would put the check somewhere the
/// file has never been: a relative `PERL5LIB`, a `use lib 'lib'`, a `do
/// './config.pl'` all resolve against the working directory, and every one of
/// them would fail there while working perfectly for the person who asked.
struct Workspace {
    root: PathBuf,
    path: PathBuf,
}

/// What came of putting one text in front of perl.
enum Asked {
    /// It compiled, and this is what perl says it means.
    Deparsed(Vec<String>),
    /// perl would not compile it, and this is what it said.
    Failed(String),
    /// It compiled and deparsed to nothing: there are no runtime ops in it.
    Silent,
    /// perl never answered — which is about this run, not about the text.
    Unanswered(NoAnswer),
}

/// perl gave no answer, and why.
///
/// `why` is the class the report counts and folds by, so it is one of the
/// constants above rather than a sentence built here; `detail` is whatever
/// evidence there is, which for a timeout or a process that never started is
/// nothing at all.
struct NoAnswer {
    why: &'static str,
    detail: String,
}

impl Workspace {
    fn new() -> Option<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "camello-deparse-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).ok()?;
        let path = root.join("input.pl");
        Some(Workspace { root, path })
    }

    fn ask(&self, text: &str) -> Asked {
        if let Err(error) = std::fs::write(&self.path, text) {
            return Asked::Failed(format!("{}: {error}", self.path.display()));
        }
        match compile_errors(&self.path) {
            Ok(Some(errors)) => return Asked::Failed(self.scrub(&errors)),
            Ok(None) => {}
            Err(no_answer) => return Asked::Unanswered(self.scrubbed(no_answer)),
        }
        match deparse(&self.path) {
            Ok(Some(lines)) => Asked::Deparsed(
                lines
                    .iter()
                    .map(|line| self.scrub(line))
                    .collect::<Vec<_>>(),
            ),
            Ok(None) => Asked::Silent,
            Err(no_answer) => Asked::Unanswered(self.scrubbed(no_answer)),
        }
    }

    /// A reason, with the temporary path taken out of its evidence.
    fn scrubbed(&self, no_answer: NoAnswer) -> NoAnswer {
        NoAnswer {
            why: no_answer.why,
            detail: self.scrub(&no_answer.detail),
        }
    }

    /// The temporary path, out of anything perl says or deparses.
    ///
    /// Both sides are read from it, so taking it out changes no comparison —
    /// and leaving it in makes every message unique to the file it came from,
    /// which is exactly the property a report has to fold them by. `input.pl`
    /// is what perl was given; it is also all the reader wants to be told.
    fn scrub(&self, text: &str) -> String {
        text.replace(&*self.path.to_string_lossy(), "input.pl")
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// What perl said, whole.
///
/// It used to be cut to three lines of 120 characters, which took the reader's
/// answer away in the middle of the sentence carrying it: the path perl went
/// looking in, the line it gave up on. A message is only ever printed once per
/// run now, so its length is not what makes a report long.
fn evidence(errors: &str) -> String {
    /// Enough for any diagnostic worth reading, and a stop for the file that
    /// answers with a thousand of them.
    const LINES: usize = 40;
    let mut kept: Vec<String> = errors
        .lines()
        .take(LINES)
        .map(|line| format!("  {}", line.trim_end()))
        .collect();
    let rest = errors.lines().count().saturating_sub(kept.len());
    if rest > 0 {
        kept.push(format!("  … {rest} more lines"));
    }
    kept.join("\n")
}

/// What `perl -c` said, if it did not say the file is fine.
///
/// `Ok(None)` is the file compiling. A perl that never answered is an `Err`
/// rather than that: it used to be the same value, so a machine with no perl
/// reachable any more read every file in the corpus as compiling and then
/// deparsing to nothing.
fn compile_errors(path: &Path) -> Result<Option<String>, NoAnswer> {
    let output = perl(path, &["-c"])?;
    let said = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok((!said.contains("syntax OK")).then(|| {
        said.lines()
            .filter(|line| !line.contains("syntax OK"))
            .collect::<Vec<_>>()
            .join("\n")
    }))
}

/// What perl says the program means, normalised.
///
/// `Ok(None)` is a program with nothing in it to run, which is a real answer
/// about a real file: POD, `package`, `use`, forward declarations. A deparse
/// that exited non-zero is not that answer and does not pretend to be.
fn deparse(path: &Path) -> Result<Option<Vec<String>>, NoAnswer> {
    let output = perl(path, &["-MO=Deparse,-p"])?;
    if !output.status.success() {
        return Err(NoAnswer {
            why: WOULD_NOT_DEPARSE,
            detail: evidence(&String::from_utf8_lossy(&output.stderr)),
        });
    }
    let raw = String::from_utf8_lossy(&output.stdout);

    // Two kinds of line are dropped or reordered before comparing, because
    // B::Deparse emits them from a hash walk and their order varies between
    // runs of the same file: forward declarations (`sub name;`) and the
    // inlinable constant stubs that `use constant` and Errno leave behind (`sub
    // NAME () { 42 }`). Neither carries behaviour; the constants are sorted
    // rather than dropped so a changed *value* still shows up.
    //
    // Two more things that walk moves around. A reference stringified into the
    // output — `autodie` puts one in `%^H` — carries the address it happened to
    // be allocated at. And a blank line sometimes accompanies a constant stub
    // and sometimes does not: deparsing `JSON::backportPP::Compat5006` twice
    // gives two different answers with no camello involved at all. Deparsed
    // output has no blank line that means anything — a newline inside a string
    // comes out escaped — so they go.
    let mut body: Vec<String> = Vec::new();
    let mut constants: Vec<String> = Vec::new();
    for line in raw.lines() {
        match sub_line(line) {
            Some(SubLine::Constant) => constants.push(line.to_string()),
            Some(SubLine::Forward) => {}
            None if line.trim().is_empty() => {}
            None => body.push(anonymise_addresses(line)),
        }
    }
    constants.sort();
    body.extend(constants);
    Ok((!body.is_empty()).then_some(body))
}

/// The two shapes of `sub` line that deparse order is not stable in.
enum SubLine {
    /// `sub NAME () { ... }` — an inlinable constant.
    Constant,
    /// `sub NAME;` or `sub NAME ($$);` — a forward declaration.
    Forward,
}

fn sub_line(line: &str) -> Option<SubLine> {
    let rest = line.strip_prefix("sub ")?;
    let name_end = rest
        .find(|character: char| !character.is_ascii_alphanumeric() && !"_:".contains(character))
        .unwrap_or(rest.len());
    let (name, rest) = rest.split_at(name_end);
    if name.is_empty() || name.starts_with(|character: char| character.is_ascii_digit()) {
        return None;
    }
    let rest = rest.trim_start();

    let rest = match rest.strip_prefix('(') {
        Some(after) => {
            let close = after.find(')')?;
            after[close + 1..].trim_start()
        }
        None => rest,
    };
    if rest == ";" {
        return Some(SubLine::Forward);
    }
    // `()` and nothing else is a prototype-less constant stub; a body follows.
    let had_empty_prototype = line[4 + name_end..].trim_start().starts_with("()");
    (had_empty_prototype && rest.starts_with('{') && rest.ends_with('}'))
        .then_some(SubLine::Constant)
}

/// `HASH(0x7f9e...)` and its kin, with the address taken out.
fn anonymise_addresses(line: &str) -> String {
    const KINDS: [&str; 6] = ["SCALAR", "ARRAY", "HASH", "CODE", "GLOB", "REF"];
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    'outer: while !rest.is_empty() {
        for kind in KINDS {
            let prefix = format!("={kind}(0x");
            if let Some(after) = rest.strip_prefix(prefix.as_str()) {
                let hex = after
                    .find(|character: char| !character.is_ascii_hexdigit())
                    .unwrap_or(after.len());
                if after[hex..].starts_with(')') {
                    out.push_str(&format!("={kind}(0xADDR)"));
                    rest = &after[hex + 1..];
                    continue 'outer;
                }
            }
        }
        let step = rest
            .char_indices()
            .nth(1)
            .map_or(rest.len(), |(index, _)| index);
        out.push_str(&rest[..step]);
        rest = &rest[step..];
    }
    out
}

/// Run perl on `path` — from camello's own working directory, which is the one
/// the caller's `PERL5LIB` and `use lib` were written against — and kill it if
/// it will not stop.
///
/// A corpus is full of files that do something surprising at `BEGIN` time, and
/// a check that hangs on one of them is a check nobody runs twice.
///
/// The two ways this returns nothing are told apart, because they are read
/// differently: a file that took longer than a minute of perl is a fact about
/// that file, and a perl that would not start is a fact about the machine.
fn perl(path: &Path, args: &[&str]) -> Result<std::process::Output, NoAnswer> {
    let no_answer = |why| NoAnswer {
        why,
        detail: String::new(),
    };
    let mut child = Command::new("perl")
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| no_answer(COULD_NOT_RUN))?;

    // The pipes are drained on their own threads: a deparse of a large file
    // fills the buffer, and a child blocked on a full pipe never exits, which
    // would turn the timeout below into the normal case.
    let mut stdout = child.stdout.take().map(drain);
    let mut stderr = child.stderr.take().map(drain);

    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(no_answer(TIMED_OUT));
            }
            Err(_) => return Err(no_answer(COULD_NOT_RUN)),
        }
    };

    Ok(std::process::Output {
        status,
        stdout: stdout.take().map(join).unwrap_or_default(),
        stderr: stderr.take().map(join).unwrap_or_default(),
    })
}

fn drain<R: std::io::Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        buffer
    })
}

fn join(handle: std::thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    handle.join().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{anonymise_addresses, meaning, sub_line, SubLine, Verdict};

    #[test]
    fn the_lines_deparse_order_moves_are_recognised() {
        assert!(matches!(
            sub_line("sub PI () { 3.14 }"),
            Some(SubLine::Constant)
        ));
        assert!(matches!(
            sub_line("sub Foo::BAR () { 'x' }"),
            Some(SubLine::Constant)
        ));
        assert!(matches!(sub_line("sub name;"), Some(SubLine::Forward)));
        assert!(matches!(sub_line("sub name ($$);"), Some(SubLine::Forward)));
        assert!(sub_line("sub name {").is_none());
        assert!(sub_line("    sub name;").is_none());
        assert!(sub_line("print 'sub x;';").is_none());
    }

    #[test]
    fn an_address_is_not_part_of_a_program() {
        assert_eq!(
            anonymise_addresses("$h{'x'} = 'autodie=HASH(0x7fb1c0)';"),
            "$h{'x'} = 'autodie=HASH(0xADDR)';"
        );
        assert_eq!(anonymise_addresses("$x = 1;"), "$x = 1;");
    }

    /// Only runs where there is a perl; the oracle is opt-in for that reason.
    #[test]
    fn perl_notices_a_program_that_changed() {
        if !super::available() {
            return;
        }
        let source = "my $x = 1;\nprint $x;\n";
        assert!(matches!(meaning(source, source), Verdict::Same));
        assert!(matches!(
            meaning(source, "my $x = 2;\nprint $x;\n"),
            Verdict::Differs { .. }
        ));
        assert!(matches!(
            meaning(source, "my $x = ;\n"),
            Verdict::Differs { .. }
        ));
    }

    /// A file with no ops in it is unanswerable for that reason and no other.
    /// The three shapes below all compile and all deparse to nothing.
    #[test]
    fn a_file_with_nothing_to_run_says_so() {
        if !super::available() {
            return;
        }
        for source in [
            "=head1 NAME\n\nFoo - a file that is documentation\n\n=cut\n",
            "package Foo;\n",
            "use strict;\n",
        ] {
            let verdict = meaning(source, source);
            assert!(
                matches!(&verdict, Verdict::NotLoadable { why, .. } if *why == super::NO_OPS),
                "{source:?} should be unanswered for having no ops"
            );
        }
    }
}
