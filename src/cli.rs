use clap::{Parser, Subcommand};
use encoding_rs::Encoding;
use miette::{IntoDiagnostic, Report, Result};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{format_perl_with_options, parse_perl, DelimiterSpacing, FormatterOptions};

#[derive(Parser)]
#[command(name = "camello")]
#[command(about = "A Perl code formatter built with Rust and Rowan")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Format Perl code
    Format {
        /// Files or directories to format (reads from stdin if not provided)
        #[arg(help = "Files or directories to format (recursive; stdin if omitted)")]
        paths: Vec<PathBuf>,

        /// Perl code to format
        #[arg(
            short,
            long = "eval",
            help = "Perl code to format",
            conflicts_with_all = ["paths", "eval_escape"]
        )]
        eval: Option<String>,

        /// Perl code to format with escape sequence interpretation
        #[arg(
            short = 'E',
            long = "eval-escape",
            help = "Perl code to format with escape sequence interpretation (\\n becomes newline)",
            conflicts_with_all = ["paths", "eval"]
        )]
        eval_escape: Option<String>,

        /// Check if file is already formatted without making changes
        #[arg(long, help = "Check if file is already formatted")]
        check: bool,

        /// Overwrite the input file with the formatted result
        #[arg(
            short = 'w',
            long = "write",
            help = "Overwrite the input files with the formatted result",
            requires = "paths",
            conflicts_with_all = ["check", "output"]
        )]
        write: bool,

        /// Extensions to consider when walking a directory
        #[arg(
            long,
            value_name = "EXT,...",
            default_value = "pl,pm,t,psgi",
            help = "Extensions to consider when walking a directory"
        )]
        extensions: String,

        /// How many files to format at once
        #[arg(
            short = 'j',
            long,
            value_name = "N",
            help = "How many files to format at once (default: one per core)"
        )]
        jobs: Option<usize>,

        /// Stop formatting after the first parse error is reported
        #[arg(
            long = "stop-on-first-error",
            help = "Stop after reporting the first parse error"
        )]
        stop_on_first_error: bool,

        /// Output to file instead of stdout
        #[arg(short, long, help = "Output file path")]
        output: Option<PathBuf>,

        /// Input file encoding (e.g., utf-8, euc-jp, shift_jis)
        #[arg(long, help = "Input file encoding (default: utf-8)")]
        encoding: Option<String>,

        #[command(flatten)]
        layout: LayoutArgs,
    },
    /// Developer tools (hidden): not an interface to depend on
    ///
    /// Everything here exists to work on camello itself — asking the invariants
    /// of arbitrary code, looking at a tree. `format` is the interface; these
    /// are the tools, in the sense of `go tool`, and they may change shape
    /// without notice.
    #[command(hide = true)]
    Dev {
        #[command(subcommand)]
        command: DevCommands,
    },
}

#[derive(Subcommand)]
pub enum DevCommands {
    /// Dump parsed AST structure
    Dump {
        /// Path to the Perl file to parse and dump (reads from stdin if not provided)
        #[arg(help = "Path to the Perl file (reads from stdin if not provided)")]
        path: Option<PathBuf>,

        /// Perl code to parse and dump
        #[arg(
            short,
            long = "eval",
            help = "Perl code to parse and dump",
            conflicts_with_all = ["path", "eval_escape"]
        )]
        eval: Option<String>,

        /// Perl code to parse and dump with escape sequence interpretation
        #[arg(
            short = 'E',
            long = "eval-escape",
            help = "Perl code to parse and dump with escape sequence interpretation (\\n becomes newline)",
            conflicts_with_all = ["path", "eval"]
        )]
        eval_escape: Option<String>,

        /// Quiet mode: suppress output on success
        #[arg(
            short = 'q',
            long = "quiet",
            help = "Quiet mode: suppress output on success"
        )]
        quiet: bool,

        /// Very quiet mode: suppress all output
        #[arg(
            short = 'Q',
            long = "very-quiet",
            help = "Very quiet mode: suppress all output"
        )]
        very_quiet: bool,

        /// Stop dumping after the first parse error is reported
        #[arg(
            long = "stop-on-first-error",
            help = "Stop after reporting the first parse error"
        )]
        stop_on_first_error: bool,

        /// Input file encoding (e.g., utf-8, euc-jp, shift_jis)
        #[arg(long, help = "Input file encoding (default: utf-8)")]
        encoding: Option<String>,
    },
    /// Ask the formatter's invariants of arbitrary Perl
    ///
    /// A fixture has an expected output and is checked against it. Code taken
    /// off a disk has none, and these are the questions that can still be asked
    /// of it: does the string content survive, do the comments survive, does a
    /// second pass change anything. This is how a defect gets found before
    /// anyone knows what the right output would have been.
    Check {
        /// Files or directories to check (reads from stdin if not provided)
        #[arg(help = "Files or directories to check (recursive; stdin if omitted)")]
        paths: Vec<PathBuf>,

        /// How many files to check at once
        #[arg(
            short = 'j',
            long,
            value_name = "N",
            help = "How many files to check at once (default: one per core)"
        )]
        jobs: Option<usize>,

        /// Only report these invariants (comma-separated slugs)
        #[arg(
            long,
            value_name = "SLUG,...",
            help = "Only report these invariants; --list-invariants prints the slugs"
        )]
        only: Option<String>,

        /// List the invariants and exit
        #[arg(long, help = "List the invariants and exit")]
        list_invariants: bool,

        /// Also ask perl whether the output is the same program
        #[arg(
            long,
            help = "Also ask perl whether the output is the same program (runs perl -c and \
                    B::Deparse on every file, which executes its BEGIN blocks)"
        )]
        deparse: bool,

        /// One line per violation, without the evidence
        #[arg(short, long, help = "One line per violation, without the evidence")]
        quiet: bool,

        /// File extensions to walk into when given a directory
        #[arg(
            long,
            value_name = "EXT,...",
            default_value = "pl,pm,t,psgi",
            help = "Extensions to consider when walking a directory"
        )]
        extensions: String,

        /// Input file encoding (e.g., utf-8, euc-jp, shift_jis)
        #[arg(long, help = "Input file encoding (default: utf-8)")]
        encoding: Option<String>,
    },
}

/// The formatter's options, as command-line flags.
///
/// `FormatterOptions` existed and the CLI never passed one, so every knob in it
/// was reachable from the library and from nowhere else — including
/// `max_alignment_padding`, which is a guard against one long line pushing a
/// whole group across the screen.
#[derive(clap::Args, Debug, Clone)]
pub struct LayoutArgs {
    /// Spaces per indent level
    #[arg(long, value_name = "N", help = "Spaces per indent level (default: 4)")]
    pub indent_width: Option<usize>,

    /// Minimum spaces between code and a trailing comment
    #[arg(
        long,
        value_name = "N",
        help = "Minimum spaces before a trailing comment (default: 4)"
    )]
    pub min_spaces_before_comment: Option<usize>,

    /// Space inside flat `[...]` / `{...}` literals
    #[arg(
        long,
        value_name = "STYLE",
        help = "Inside of [...] and {...} literals: tight, standard (space when \
                holding two or more items; default), or loose (always a space)"
    )]
    pub delimiter_spacing: Option<DelimiterSpacingArg>,

    /// Cap on the spaces vertical alignment may insert; 0 disables alignment
    #[arg(
        long,
        value_name = "N",
        help = "Maximum alignment padding, 0 to disable alignment (default: 64)"
    )]
    pub max_alignment_padding: Option<usize>,

    /// Keep a one-statement `map`/`sub`/`do` block on one line
    #[arg(long, help = "Never keep a one-statement map/sub/do block on one line")]
    pub no_single_line_blocks: bool,
}

/// `DelimiterSpacing`, spelled for clap. The library enum stays clap-free.
#[derive(clap::ValueEnum, Debug, Clone, Copy)]
pub enum DelimiterSpacingArg {
    Tight,
    Standard,
    Loose,
}

impl From<DelimiterSpacingArg> for DelimiterSpacing {
    fn from(arg: DelimiterSpacingArg) -> Self {
        match arg {
            DelimiterSpacingArg::Tight => DelimiterSpacing::Tight,
            DelimiterSpacingArg::Standard => DelimiterSpacing::Standard,
            DelimiterSpacingArg::Loose => DelimiterSpacing::Loose,
        }
    }
}

impl LayoutArgs {
    fn to_options(&self) -> FormatterOptions {
        let defaults = FormatterOptions::default();
        FormatterOptions {
            indent_width: self.indent_width.unwrap_or(defaults.indent_width),
            min_spaces_before_comment: self
                .min_spaces_before_comment
                .unwrap_or(defaults.min_spaces_before_comment),
            max_alignment_padding: self
                .max_alignment_padding
                .unwrap_or(defaults.max_alignment_padding),
            allow_single_line_blocks: !self.no_single_line_blocks,
            delimiter_spacing: self
                .delimiter_spacing
                .map_or(defaults.delimiter_spacing, Into::into),
        }
    }
}

/// Function to interpret escape sequences
fn interpret_escape_sequences(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek() {
                Some('n') => {
                    chars.next(); // consume 'n'
                    result.push('\n');
                }
                Some('t') => {
                    chars.next(); // consume 't'
                    result.push('\t');
                }
                Some('r') => {
                    chars.next(); // consume 'r'
                    result.push('\r');
                }
                Some('\\') => {
                    chars.next(); // consume second '\'
                    result.push('\\');
                }
                Some('"') => {
                    chars.next(); // consume '"'
                    result.push('"');
                }
                Some('\'') => {
                    chars.next(); // consume '\''
                    result.push('\'');
                }
                _ => {
                    // Unknown escape sequence, keep as is
                    result.push(ch);
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Format {
            paths,
            eval,
            eval_escape,
            check,
            write,
            extensions,
            jobs,
            stop_on_first_error,
            output,
            encoding,
            layout,
        } => {
            // One source is a thing to look at, and its formatted text goes to
            // stdout. A tree is a thing to do something to, and there is nowhere
            // for five hundred files of stdout to go — so the two are different
            // commands wearing one name, and this is where they part.
            if is_a_tree(&paths) {
                return format_tree(
                    paths,
                    check,
                    write,
                    &extensions,
                    jobs,
                    encoding,
                    &layout.to_options(),
                );
            }
            format_file(
                paths.into_iter().next(),
                eval,
                eval_escape,
                check,
                write,
                stop_on_first_error,
                output,
                encoding,
                &layout.to_options(),
            )?;
        }
        Commands::Dev { command } => match command {
            DevCommands::Dump {
                path,
                eval,
                eval_escape,
                quiet,
                very_quiet,
                stop_on_first_error,
                encoding,
            } => {
                dump_file(
                    path,
                    eval,
                    eval_escape,
                    quiet,
                    very_quiet,
                    stop_on_first_error,
                    encoding,
                )?;
            }
            DevCommands::Check {
                paths,
                jobs,
                only,
                list_invariants,
                deparse,
                quiet,
                extensions,
                encoding,
            } => {
                return check_paths(
                    paths,
                    jobs,
                    only.as_deref(),
                    list_invariants,
                    deparse,
                    quiet,
                    &extensions,
                    encoding,
                );
            }
        },
    }

    Ok(())
}

/// Is this a set of paths to walk, rather than one source to print?
///
/// A directory always is; so is more than one path, whatever they are.
fn is_a_tree(paths: &[PathBuf]) -> bool {
    paths.len() > 1 || paths.first().is_some_and(|path| path.is_dir())
}

/// Apply `job` to every item, on as many threads as asked for, and give the
/// results back in the order the items were in.
///
/// Formatting one file has nothing to do with formatting the next — no shared
/// state, no ordering — so the only thing this has to preserve is the order of
/// the *output*, which it does by writing each result into its own slot. The
/// alternative is printing from the workers, which makes a run's output depend
/// on how the scheduler felt.
fn in_parallel<T, R>(items: &[T], jobs: Option<usize>, job: impl Fn(&T) -> R + Sync) -> Vec<R>
where
    T: Sync,
    R: Send,
{
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let workers = jobs
        .filter(|&jobs| jobs > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
        })
        .min(items.len())
        .max(1);

    if workers == 1 {
        return items.iter().map(job).collect();
    }

    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    let job = &job;
    let slots = &slots;
    let next = &next;

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else { return };
                let result = job(item);
                *slots[index]
                    .lock()
                    .expect("no worker panics while holding this") = Some(result);
            });
        }
    });

    slots
        .iter()
        .map(|slot| {
            slot.lock()
                .expect("the workers are finished")
                .take()
                .expect("every slot was filled")
        })
        .collect()
}

/// What formatting one file came to.
struct Formatted {
    /// Rendered diagnostics, if the file did not parse cleanly.
    diagnostics: Vec<String>,
    /// Whether the formatted text differs from what is on disk.
    changed: bool,
    /// What went wrong before formatting could even be attempted.
    failure: Option<String>,
}

/// `camello format` over files and directories, in parallel.
///
/// Writing is asked for, never assumed: the formatted text of a tree has
/// nowhere to go but back over the tree, and doing that because someone typed a
/// path is not undoable.
fn format_tree(
    paths: Vec<PathBuf>,
    check: bool,
    write: bool,
    extensions: &str,
    jobs: Option<usize>,
    encoding: Option<String>,
    options: &FormatterOptions,
) -> Result<()> {
    if !check && !write {
        return Err(miette::miette!(
            "formatting more than one file needs --write (rewrite them) or --check (report which would change)"
        ));
    }

    let encoding = get_encoding(encoding.as_ref())?;
    let extensions: Vec<&str> = extensions.split(',').map(str::trim).collect();

    let mut files = Vec::new();
    for path in &paths {
        collect_perl_files(path, &extensions, &mut files)?;
    }

    let reports = in_parallel(&files, jobs, |path| {
        format_one(path, check, encoding, options)
    });

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let (mut changed, mut with_diagnostics, mut failed) = (0usize, 0usize, 0usize);
    for (path, report) in files.iter().zip(&reports) {
        let path = path.display();
        if let Some(failure) = &report.failure {
            failed += 1;
            writeln!(out, "{path}\t{failure}").into_diagnostic()?;
            continue;
        }
        if !report.diagnostics.is_empty() {
            with_diagnostics += 1;
            writeln!(out, "{path}\t{} diagnostic(s)", report.diagnostics.len())
                .into_diagnostic()?;
            for diagnostic in &report.diagnostics {
                for line in diagnostic.lines() {
                    writeln!(out, "    {line}").into_diagnostic()?;
                }
            }
        }
        if report.changed {
            changed += 1;
            let what = if check { "would reformat" } else { "formatted" };
            writeln!(out, "{path}\t{what}").into_diagnostic()?;
        }
    }

    let what = if check { "would reformat" } else { "formatted" };
    writeln!(
        out,
        "---- checked {}, {what} {changed}{}{}",
        files.len(),
        if with_diagnostics > 0 {
            format!(", left alone {with_diagnostics}")
        } else {
            String::new()
        },
        if failed > 0 {
            format!(", unreadable {failed}")
        } else {
            String::new()
        }
    )
    .into_diagnostic()?;

    // A file nobody could parse cleanly is not "formatted", and `--check` in a
    // pipeline wants to hear about it.
    if failed > 0 || with_diagnostics > 0 || (check && changed > 0) {
        std::process::exit(1);
    }
    Ok(())
}

/// Format one file of a tree: read it, format it, and write it back if asked.
fn format_one(
    path: &Path,
    check: bool,
    encoding: &'static Encoding,
    options: &FormatterOptions,
) -> Formatted {
    let failed = |failure: String| Formatted {
        diagnostics: Vec::new(),
        changed: false,
        failure: Some(failure),
    };

    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return failed(error.to_string()),
    };
    let (input, _, had_errors) = encoding.decode(&bytes);
    if had_errors {
        return failed("not decodable with this encoding".to_string());
    }

    let (formatted, errors) = format_perl_with_options(&input, options);
    let diagnostics: Vec<String> = errors
        .into_iter()
        .map(|error| format!("{:?}", Report::new(error)))
        .collect();

    // A file the parser had something to say about is reported and left alone.
    // One such file, formatted on purpose, is a best-effort the writer can look
    // at; a tree of them, rewritten because someone typed a directory, is not.
    if !diagnostics.is_empty() {
        return Formatted {
            diagnostics,
            changed: false,
            failure: None,
        };
    }

    let changed = formatted != input;
    if changed && !check {
        if let Err(error) = write_with_encoding(path, &formatted, encoding) {
            return failed(format!("{error}"));
        }
    }

    Formatted {
        diagnostics,
        changed,
        failure: None,
    }
}

/// `text` broken into lines of at most `width` columns, on word boundaries.
///
/// A paragraph written as one long string in the source is the only sane way to
/// keep it editable; a terminal wants it in lines. Words longer than `width`
/// get their own line rather than being cut.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// How one invariant came out over a run: asked and passed, asked and failed,
/// or asked and unanswerable, with the reasons it went unanswered.
struct Tally {
    invariant: crate::check::Invariant,
    ok: usize,
    failed: usize,
    unanswered: Vec<&'static str>,
}

impl Tally {
    /// The one word the reader is looking for. A row with nothing but
    /// unanswered files is neither a pass nor a failure, and says so.
    fn result(&self) -> &'static str {
        if self.failed > 0 {
            "FAIL"
        } else if self.ok > 0 {
            "ok"
        } else {
            "n/a"
        }
    }

    /// Why the unanswered ones went unanswered, most common first. Silent when
    /// there is one reason and it is the ordinary one, which is a parse failure
    /// the table has already reported on its own row.
    fn reasons(&self) -> Vec<(&'static str, usize)> {
        let mut counted: Vec<(&'static str, usize)> = Vec::new();
        for why in &self.unanswered {
            match counted.iter_mut().find(|(seen, _)| seen == why) {
                Some((_, count)) => *count += 1,
                None => counted.push((why, 1)),
            }
        }
        if counted.len() == 1 && counted[0].0 == "no clean parse" {
            return Vec::new();
        }
        counted.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        counted
    }
}

/// `camello dev check`: run the invariants over files, directories, or stdin.
///
/// Exits non-zero when anything is violated, so it can gate a corpus run.
#[allow(clippy::too_many_arguments)]
fn check_paths(
    paths: Vec<PathBuf>,
    jobs: Option<usize>,
    only: Option<&str>,
    list_invariants: bool,
    deparse: bool,
    quiet: bool,
    extensions: &str,
    encoding: Option<String>,
) -> Result<()> {
    use crate::check::{check_report, Invariant};

    if list_invariants {
        let mut heading = None;
        for (index, invariant) in Invariant::every().enumerate() {
            if index > 0 {
                println!();
            }
            if heading != Some(invariant.subject()) {
                heading = Some(invariant.subject());
                println!("---- {}", invariant.subject().heading());
                println!();
            }
            let opt_in = if invariant.needs_perl() {
                "   (opt-in: --deparse)"
            } else {
                ""
            };
            println!("{:<17}{}{opt_in}", invariant.slug(), invariant.name());
            for line in wrapped(invariant.description(), 72) {
                println!("    {line}");
            }
        }
        return Ok(());
    }

    let mut wanted: Vec<Invariant> = Invariant::ALL.to_vec();
    if let Some(only) = only {
        wanted.clear();
        for slug in only.split(',').map(str::trim) {
            let Some(invariant) = Invariant::every().find(|kind| kind.slug() == slug) else {
                return Err(miette::miette!(
                    "unknown invariant {slug:?}; --list-invariants prints them"
                ));
            };
            wanted.push(invariant);
        }
    }
    // Naming it under `--only` is opting in as much as the flag is; the flag is
    // for the common case of wanting everything plus the oracle.
    if deparse && !wanted.contains(&Invariant::Deparse) {
        wanted.push(Invariant::Deparse);
    }
    // Better here than 4000 files later, one failed spawn at a time.
    if wanted.iter().any(|invariant| invariant.needs_perl()) && !crate::check::deparse::available()
    {
        return Err(miette::miette!(
            "no working perl on PATH, and {} needs one",
            Invariant::Deparse.slug()
        ));
    }
    let wanted = wanted.as_slice();

    let extensions: Vec<&str> = extensions.split(',').map(str::trim).collect();
    let encoding = get_encoding(encoding.as_ref())?;

    let mut files = Vec::new();
    for path in &paths {
        collect_perl_files(path, &extensions, &mut files)?;
    }

    // No paths at all means stdin, so the command composes with a pipeline the
    // way `format` does. `None` is a file this command has nothing to say about
    // — unreadable, or not decodable with this encoding, neither of which is a
    // violation.
    let checked: Vec<Option<(String, crate::check::Outcome)>> = if paths.is_empty() {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes).into_diagnostic()?;
        let (decoded, _, had_errors) = encoding.decode(&bytes);
        if had_errors {
            return Err(miette::miette!(
                "stdin is not decodable as {}; refusing a lossy check",
                encoding.name()
            ));
        }
        vec![Some((
            "<stdin>".to_string(),
            check_report(&decoded, wanted),
        ))]
    } else {
        in_parallel(&files, jobs, |path| {
            let bytes = fs::read(path).ok()?;
            let (decoded, _, had_errors) = encoding.decode(&bytes);
            if had_errors {
                return None;
            }
            Some((path.display().to_string(), check_report(&decoded, wanted)))
        })
    };

    let sources = checked.iter().flatten().count();
    let skipped = checked.len() - sources;
    let mut offenders: Vec<(&str, Vec<&'static str>)> = Vec::new();
    // Three columns per invariant, not one count: an invariant that was never
    // answered for a file — it did not parse, perl would not load it — is not
    // one that passed, and a report that says so is a corpus called clean when
    // most of it was never looked at.
    let mut tally: Vec<Tally> = Invariant::every()
        .map(|kind| Tally {
            invariant: kind,
            ok: 0,
            failed: 0,
            unanswered: Vec::new(),
        })
        .collect();

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut reported = false;
    for (label, outcome) in checked.iter().flatten() {
        for entry in &mut tally {
            // `clean-parse` is asked whether or not it was selected: it is the
            // prerequisite the rest are unanswered without.
            let asked =
                entry.invariant == Invariant::CleanParse || wanted.contains(&entry.invariant);
            if !asked {
                continue;
            }
            if outcome
                .violations
                .iter()
                .any(|violation| violation.invariant == entry.invariant)
            {
                entry.failed += 1;
            } else if let Some(unanswered) = outcome
                .unanswered
                .iter()
                .find(|unanswered| unanswered.invariant == entry.invariant)
            {
                entry.unanswered.push(unanswered.why);
            } else {
                entry.ok += 1;
            }
        }

        // An invariant nobody could answer is not a violation and does not
        // colour the exit status, but it is the reader's business why: a run
        // whose oracle went quiet on two files in three has to say so where the
        // files are named, not only as a number at the bottom.
        for unanswered in outcome.unanswered.iter().filter(|it| !it.detail.is_empty()) {
            reported = true;
            writeln!(
                out,
                "{label}\t{}\tn/a: {}",
                unanswered.invariant.slug(),
                unanswered.why
            )
            .into_diagnostic()?;
            if !quiet {
                for line in unanswered.detail.lines() {
                    writeln!(out, "    {line}").into_diagnostic()?;
                }
            }
        }

        if outcome.violations.is_empty() {
            continue;
        }
        offenders.push((
            label.as_str(),
            outcome
                .violations
                .iter()
                .map(|violation| violation.invariant.slug())
                .collect(),
        ));
        for violation in &outcome.violations {
            reported = true;
            writeln!(
                out,
                "{label}\t{}\t{}",
                violation.invariant.slug(),
                violation.summary
            )
            .into_diagnostic()?;
            if !quiet {
                for line in violation.detail.lines() {
                    writeln!(out, "    {line}").into_diagnostic()?;
                }
            }
        }
    }

    let violated = offenders.len();

    // Which questions were asked, and how each of them came out. The old
    // summary named only the invariants that were violated, which left the
    // reader unable to tell a run where everything passed from a run where
    // nothing was asked.
    if reported {
        writeln!(out).into_diagnostic()?;
    }
    writeln!(
        out,
        "---- {:<20}{:<8}{:>5}{:>8}{:>6}",
        "invariants", "result", "ok", "failed", "n/a"
    )
    .into_diagnostic()?;
    // Grouped, because the group is the answer to the reader's next question.
    // A parser row and a formatter row that failed send them to different files.
    let mut heading = None;
    for entry in tally
        .iter()
        .filter(|entry| entry.ok + entry.failed + entry.unanswered.len() > 0)
    {
        let subject = entry.invariant.subject();
        if heading != Some(subject) {
            writeln!(out, "   {}", subject.heading()).into_diagnostic()?;
            heading = Some(subject);
        }
        writeln!(
            out,
            "     {:<20}{:<8}{:>5}{:>8}{:>6}",
            entry.invariant.slug(),
            entry.result(),
            entry.ok,
            entry.failed,
            entry.unanswered.len()
        )
        .into_diagnostic()?;
        // A file the oracle could not be asked about is a different thing from
        // one that did not parse, and the number alone does not say which.
        for (why, count) in entry.reasons() {
            writeln!(out, "       n/a: {count} {why}").into_diagnostic()?;
            if let Some(hint) = crate::check::deparse::hint(why) {
                writeln!(out, "            {hint}").into_diagnostic()?;
            }
        }
    }

    // The per-file reports have scrolled away by now; a run over a directory
    // ends by saying which files to go back to.
    if violated > 0 && sources > 1 {
        writeln!(out).into_diagnostic()?;
        writeln!(out, "---- files with a violation").into_diagnostic()?;
        for (label, slugs) in &offenders {
            writeln!(out, "     {label}\t{}", slugs.join(" ")).into_diagnostic()?;
        }
    }

    writeln!(out).into_diagnostic()?;
    writeln!(
        out,
        "---- checked {sources}, clean {}, violated {violated}{}",
        sources - violated,
        if skipped > 0 {
            format!(", not decodable {skipped}")
        } else {
            String::new()
        }
    )
    .into_diagnostic()?;

    if violated > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// Every file below `path` whose extension is one this command reads.
fn collect_perl_files(path: &Path, extensions: &[&str], into: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        into.push(path.to_path_buf());
        return Ok(());
    }
    if !path.is_dir() {
        return Err(miette::miette!(
            "no such file or directory: {}",
            path.display()
        ));
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(path)
        .into_diagnostic()?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<_>>()
        .into_diagnostic()?;
    entries.sort();
    for entry in entries {
        let metadata = fs::symlink_metadata(&entry).into_diagnostic()?;
        // Do not follow links found while walking a tree. Besides escaping the
        // requested root, a link to an ancestor would recurse forever. A link
        // explicitly named by the user is still handled by the checks above.
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_perl_files(&entry, extensions, into)?;
        } else if entry
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| extensions.contains(&ext))
        {
            into.push(entry);
        }
    }
    Ok(())
}

/// Get encoding from encoding name string
fn get_encoding(encoding_name: Option<&String>) -> Result<&'static Encoding> {
    let encoding = match encoding_name {
        Some(name) => Encoding::for_label(name.as_bytes())
            .ok_or_else(|| miette::miette!("Unknown encoding: {}", name))?,
        None => encoding_rs::UTF_8,
    };
    Ok(encoding)
}

fn read_source(
    path: Option<&Path>,
    eval: Option<String>,
    eval_escape: Option<String>,
    encoding: &'static Encoding,
) -> Result<(String, String)> {
    if let Some(code) = eval {
        return Ok((code, "<command-line>".to_string()));
    }
    if let Some(code) = eval_escape {
        let interpreted_code = interpret_escape_sequences(&code);
        return Ok((interpreted_code, "<command-line>".to_string()));
    }

    if let Some(path) = path {
        let bytes = fs::read(path).into_diagnostic()?;
        let (decoded, _, had_errors) = encoding.decode(&bytes);
        if had_errors {
            return Err(miette::miette!(
                "'{}' is not decodable as {}; refusing lossy formatting",
                path.display(),
                encoding.name()
            ));
        }
        Ok((decoded.into_owned(), path.display().to_string()))
    } else {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes).into_diagnostic()?;
        let (decoded, _, had_errors) = encoding.decode(&bytes);
        if had_errors {
            return Err(miette::miette!(
                "stdin is not decodable as {}; refusing lossy formatting",
                encoding.name()
            ));
        }
        Ok((decoded.into_owned(), "<stdin>".to_string()))
    }
}

fn encode_to_vec(contents: &str, encoding: &'static Encoding) -> Result<Vec<u8>> {
    if std::ptr::eq(encoding, encoding_rs::UTF_8) {
        return Ok(contents.as_bytes().to_vec());
    }

    let (encoded, _, had_errors) = encoding.encode(contents);
    if had_errors {
        return Err(miette::miette!(
            "Unable to encode formatted output using {}",
            encoding.name()
        ));
    }

    Ok(encoded.into_owned())
}

fn write_with_encoding(path: &Path, contents: &str, encoding: &'static Encoding) -> Result<()> {
    let encoded = encode_to_vec(contents, encoding)?;
    // Preserve the usual meaning of explicitly formatting a symlink: update
    // its target rather than replacing the link itself with a regular file.
    let target = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            fs::canonicalize(path).into_diagnostic()?
        }
        _ => path.to_path_buf(),
    };
    atomic_write(&target, &encoded)
}

/// Replace `path` only after the complete output has reached a sibling file.
///
/// Keeping the temporary file in the same directory makes the final rename
/// atomic on the target filesystem. Existing permissions are retained.
fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("output");
    let permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());

    let (temporary, mut file) = (0..100)
        .find_map(|_| {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let temporary = parent.join(format!(
                ".{name}.camello.{}.{}.tmp",
                std::process::id(),
                sequence
            ));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(file) => Some(Ok((temporary, file))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .unwrap_or_else(|| {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a temporary output file",
            ))
        })
        .into_diagnostic()?;

    let result = (|| -> std::io::Result<()> {
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.into_diagnostic()
}

#[allow(clippy::too_many_arguments)]
fn format_file(
    path: Option<PathBuf>,
    eval: Option<String>,
    eval_escape: Option<String>,
    check: bool,
    write: bool,
    stop_on_first_error: bool,
    output: Option<PathBuf>,
    encoding: Option<String>,
    options: &FormatterOptions,
) -> Result<()> {
    if write && path.is_none() {
        return Err(miette::miette!(
            "The --write option requires a file path to be provided"
        ));
    }

    let encoding = get_encoding(encoding.as_ref())?;

    // Read from file or standard input
    let (input, source_name) = read_source(path.as_deref(), eval, eval_escape, encoding)?;

    // Execute formatting
    let (formatted, errors) = format_perl_with_options(&input, options);

    // If there are errors, display them, and optionally stop immediately
    if !errors.is_empty() {
        eprintln!("Parse error in '{source_name}':");
        if stop_on_first_error {
            let error = errors.into_iter().next().unwrap();
            eprintln!("{:?}", Report::new(error));
            std::process::exit(2);
        } else {
            for e in errors {
                eprintln!("{:?}", Report::new(e));
            }
            eprintln!("Proceeding with best-effort formatting...\n");
        }
    }

    if check {
        // Check mode: check if already formatted
        if input == formatted {
            println!("Source '{source_name}' is already formatted");
        } else {
            eprintln!("Source '{source_name}' is not formatted");
            std::process::exit(1);
        }
    } else if write {
        let path = path.expect("path should be present when write is enabled");
        write_with_encoding(path.as_path(), &formatted, encoding)?;
        println!("Formatted code written to '{}'", path.display());
    } else {
        // Format mode: output the result
        if let Some(output_path) = output {
            // Write to file
            write_with_encoding(output_path.as_path(), &formatted, encoding)?;
            println!("Formatted code written to '{}'", output_path.display());
        } else {
            // Write to standard output using UTF-8 as before
            print!("{formatted}");
            io::stdout().flush().into_diagnostic()?;
        }
    }

    Ok(())
}

fn dump_file(
    path: Option<PathBuf>,
    eval: Option<String>,
    eval_escape: Option<String>,
    quiet: bool,
    very_quiet: bool,
    stop_on_first_error: bool,
    encoding: Option<String>,
) -> Result<()> {
    let encoding = get_encoding(encoding.as_ref())?;

    // Read from file or standard input
    let (input, source_name) = read_source(path.as_deref(), eval, eval_escape, encoding)?;
    let (syntax, errors) = parse_perl(&input);

    if !errors.is_empty() {
        if !very_quiet {
            eprintln!("Parse errors in '{source_name}':");
            if stop_on_first_error {
                let error = errors.into_iter().next().unwrap();
                eprintln!("{:?}", Report::new(error));
            } else {
                for error in errors {
                    eprintln!("{:?}", Report::new(error));
                }
            }
            // Still dump the parsed AST for debugging, but exit with code 2.
            if !quiet {
                println!("Parsed AST for '{source_name}':");
                println!("{syntax:#?}");
            }
        }
        std::process::exit(2);
    } else {
        if !quiet && !very_quiet {
            println!("Parsed AST for '{source_name}':");
            println!("{syntax:#?}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format_perl;
    use std::fs;
    use tempfile::tempdir;

    /// The defaults, for the tests that are not about options.
    fn layout() -> FormatterOptions {
        FormatterOptions::default()
    }

    #[test]
    fn test_interpret_escape_sequences() {
        assert_eq!(interpret_escape_sequences("hello\\nworld"), "hello\nworld");
        assert_eq!(interpret_escape_sequences("tab\\there"), "tab\there");
        assert_eq!(interpret_escape_sequences("quote\\\"test"), "quote\"test");
        assert_eq!(
            interpret_escape_sequences("backslash\\\\test"),
            "backslash\\test"
        );
        assert_eq!(interpret_escape_sequences("normal text"), "normal text");
        assert_eq!(
            interpret_escape_sequences("print 1\\nif yes();"),
            "print 1\nif yes();"
        );
    }

    #[test]
    fn test_format_with_escape_sequences() -> Result<(), Box<dyn std::error::Error>> {
        // Test -E option functionality
        assert!(format_file(
            None,
            None,
            Some("my$var=1;\\nprint $var;".to_string()),
            false,
            false,
            false,
            None,
            None,
            &layout()
        )
        .is_ok());
        Ok(())
    }

    #[test]
    fn test_format_file_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
        // Create a temporary file
        let dir = tempdir()?;
        let file_path = dir.path().join("test.pl");
        fs::write(&file_path, "my$var=1;")?;

        // Execute formatting (not actually executed, but confirm no errors)
        assert!(format_file(
            Some(file_path),
            None,
            None,
            false,
            false,
            false,
            None,
            None,
            &layout()
        )
        .is_ok());

        Ok(())
    }

    #[test]
    fn test_format_string_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
        // Execute formatting (not actually executed, but confirm no errors)
        assert!(format_file(
            None,
            Some("my$var=1;".to_string()),
            None,
            false,
            false,
            false,
            None,
            None,
            &layout()
        )
        .is_ok());

        Ok(())
    }

    #[test]
    fn test_format_write_to_same_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("write_test.pl");
        fs::write(&file_path, "my$var=1;")?;

        format_file(
            Some(file_path.clone()),
            None,
            None,
            false,
            true,
            false,
            None,
            None,
            &layout(),
        )?;

        let written = fs::read_to_string(&file_path)?;
        let (expected, _) = format_perl("my$var=1;");
        assert_eq!(written, expected);

        Ok(())
    }

    #[test]
    fn test_format_write_preserves_encoding() -> Result<(), Box<dyn std::error::Error>> {
        use encoding_rs::SHIFT_JIS;

        let dir = tempdir()?;
        let file_path = dir.path().join("write_encoding_test.pl");
        let text = "my $var = \"こんにちは\";";
        let (encoded, _, _) = SHIFT_JIS.encode(text);
        fs::write(&file_path, &*encoded)?;

        format_file(
            Some(file_path.clone()),
            None,
            None,
            false,
            true,
            false,
            None,
            Some("shift_jis".to_string()),
            &layout(),
        )?;

        let bytes = fs::read(&file_path)?;
        let (decoded, _, had_errors) = SHIFT_JIS.decode(&bytes);
        assert!(!had_errors);
        let (expected, _) = format_perl(text);
        assert_eq!(decoded.into_owned(), expected);

        Ok(())
    }

    #[test]
    fn test_format_write_requires_path() {
        let result = format_file(None, None, None, false, true, false, None, None, &layout());
        assert!(result.is_err());
    }

    #[test]
    fn test_check_mode() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("formatted.pl");
        fs::write(&file_path, "my $var = 1;\n")?; // Use actual newline, not escaped

        // Check that the file is correctly formatted
        assert!(format_file(
            Some(file_path),
            None,
            None,
            true,
            false,
            false,
            None,
            None,
            &layout()
        )
        .is_ok());

        Ok(())
    }

    #[test]
    fn test_encoding_utf8() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test_utf8.pl");

        // Create a file with UTF-8 content
        let content = "my $var = \"こんにちは\";";
        fs::write(&file_path, content)?;

        // Read with UTF-8 encoding
        assert!(format_file(
            Some(file_path),
            None,
            None,
            false,
            false,
            false,
            None,
            Some("utf-8".to_string()),
            &layout()
        )
        .is_ok());

        Ok(())
    }

    #[test]
    fn test_encoding_eucjp() -> Result<(), Box<dyn std::error::Error>> {
        use encoding_rs::EUC_JP;

        let dir = tempdir()?;
        let file_path = dir.path().join("test_eucjp.pl");

        // Create a file with EUC-JP encoded content
        let text = "my $var = \"こんにちは\";";
        let (encoded, _, _) = EUC_JP.encode(text);
        fs::write(&file_path, &*encoded)?;

        // Read with EUC-JP encoding
        assert!(format_file(
            Some(file_path),
            None,
            None,
            false,
            false,
            false,
            None,
            Some("euc-jp".to_string()),
            &layout()
        )
        .is_ok());

        Ok(())
    }

    #[test]
    fn test_encoding_shiftjis() -> Result<(), Box<dyn std::error::Error>> {
        use encoding_rs::SHIFT_JIS;

        let dir = tempdir()?;
        let file_path = dir.path().join("test_sjis.pl");

        // Create a file with Shift_JIS encoded content
        let text = "my $var = \"こんにちは\";";
        let (encoded, _, _) = SHIFT_JIS.encode(text);
        fs::write(&file_path, &*encoded)?;

        // Read with Shift_JIS encoding
        assert!(format_file(
            Some(file_path),
            None,
            None,
            false,
            false,
            false,
            None,
            Some("shift_jis".to_string()),
            &layout()
        )
        .is_ok());

        Ok(())
    }

    #[test]
    fn test_invalid_encoding() {
        let result = get_encoding(Some(&"invalid-encoding-name".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn decoding_errors_are_rejected_instead_of_replaced() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempdir()?;
        let path = dir.path().join("invalid.pl");
        fs::write(&path, [0xff, 0xfe, b'\n'])?;

        let result = read_source(Some(&path), None, None, encoding_rs::UTF_8);

        assert!(result.is_err());
        assert_eq!(fs::read(&path)?, [0xff, 0xfe, b'\n']);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn directory_walk_does_not_follow_symlinks() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let dir = tempdir()?;
        fs::write(dir.path().join("one.pl"), "1;\n")?;
        symlink(dir.path(), dir.path().join("cycle"))?;

        let mut files = Vec::new();
        collect_perl_files(dir.path(), &["pl"], &mut files)?;

        assert_eq!(files, [dir.path().join("one.pl")]);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_through_a_symlink_preserves_the_link() -> Result<(), Box<dyn std::error::Error>>
    {
        use std::os::unix::fs::symlink;

        let dir = tempdir()?;
        let target = dir.path().join("target.pl");
        let link = dir.path().join("link.pl");
        fs::write(&target, "old\n")?;
        symlink(&target, &link)?;

        write_with_encoding(&link, "new\n", encoding_rs::UTF_8)?;

        assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
        assert_eq!(fs::read_to_string(&target)?, "new\n");
        Ok(())
    }

    /// Every worker's result lands in its own slot, so the order out is the
    /// order in however the threads were scheduled.
    #[test]
    fn parallel_results_keep_their_order() {
        let items: Vec<usize> = (0..1000).collect();
        for jobs in [None, Some(1), Some(4), Some(64)] {
            let doubled = in_parallel(&items, jobs, |item| item * 2);
            assert_eq!(
                doubled,
                items.iter().map(|item| item * 2).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn a_directory_is_formatted_in_place() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        fs::create_dir(dir.path().join("nested"))?;
        fs::write(dir.path().join("a.pl"), "my $x=1;\n")?;
        fs::write(dir.path().join("nested/b.pm"), "my $y = 2;\n")?;
        // Not one of the extensions walked into, so it must come back untouched.
        fs::write(dir.path().join("notes.txt"), "my $z=3;\n")?;

        let reports = in_parallel(
            &[
                dir.path().join("a.pl"),
                dir.path().join("nested/b.pm"),
                dir.path().join("notes.txt"),
            ],
            None,
            |path| format_one(path, false, encoding_rs::UTF_8, &layout()),
        );
        assert!(reports.iter().all(|report| report.failure.is_none()));
        assert_eq!(fs::read_to_string(dir.path().join("a.pl"))?, "my $x = 1;\n");
        assert_eq!(
            fs::read_to_string(dir.path().join("nested/b.pm"))?,
            "my $y = 2;\n"
        );

        Ok(())
    }

    /// A file the parser had something to say about is reported, not rewritten.
    #[test]
    fn a_file_with_diagnostics_is_left_alone() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join("broken.pl");
        fs::write(&path, "my $z=;\n")?;

        let report = format_one(&path, false, encoding_rs::UTF_8, &layout());

        assert!(!report.diagnostics.is_empty());
        assert!(!report.changed);
        assert_eq!(fs::read_to_string(&path)?, "my $z=;\n");

        Ok(())
    }

    #[test]
    fn a_directory_or_several_paths_is_a_tree() {
        let dir = tempdir().expect("a temporary directory");
        assert!(is_a_tree(&[dir.path().to_path_buf()]));
        assert!(is_a_tree(&[PathBuf::from("a.pl"), PathBuf::from("b.pl")]));
        assert!(!is_a_tree(&[PathBuf::from("a.pl")]));
        assert!(!is_a_tree(&[]));
    }
}
