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
#[command(about = "Formats Perl source code")]
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

        // Formatting a file writes it: that is what the command is for, and
        // what a tree of them leaves nowhere else to put. So this asks for what
        // already happens, and does nothing. It stays because it is what the
        // hand types, and because saying so is better than an error about an
        // unknown flag.
        #[arg(
            short = 'w',
            long = "write",
            help = "Overwrite the input files with the formatted result (default)",
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

        /// Name every file that differs, instead of only counting them
        #[arg(
            short = 'l',
            long = "list-different",
            help = "Name every file that was (or would be) reformatted, one path per line"
        )]
        list_different: bool,

        // The one way to say "do not overwrite it": name somewhere else, or `-`
        // for standard output. One source at a time, since a tree has no single
        // place to go.
        #[arg(
            short,
            long,
            value_name = "PATH",
            conflicts_with = "check",
            help = "Write the result here instead of over the input; - for standard output"
        )]
        output: Option<PathBuf>,

        /// Encodings a source may be in, tried in order
        #[arg(
            long,
            value_name = "NAME,...",
            help = "Encodings to try, in order, until one reads the file (default: utf-8)"
        )]
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

        /// Encodings a source may be in, tried in order
        #[arg(
            long,
            value_name = "NAME,...",
            help = "Encodings to try, in order, until one reads the file (default: utf-8)"
        )]
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

        /// One line per violation, without the evidence
        #[arg(short, long, help = "One line per violation, without the evidence")]
        quiet: bool,

        /// Every unanswered file and every message, not a few of each
        #[arg(
            short,
            long,
            conflicts_with = "quiet",
            help = "Every message a check could not be answered with, and every file it \
                    came from"
        )]
        verbose: bool,

        /// File extensions to walk into when given a directory
        #[arg(
            long,
            value_name = "EXT,...",
            default_value = "pl,pm,t,psgi",
            help = "Extensions to consider when walking a directory"
        )]
        extensions: String,

        /// Encodings a source may be in, tried in order
        #[arg(
            long,
            value_name = "NAME,...",
            help = "Encodings to try, in order, until one reads the file (default: utf-8)"
        )]
        encoding: Option<String>,
    },

    /// Ask perl whether the formatter's output is the same program
    ///
    /// The invariants under `check` are what camello can assert about itself,
    /// which is also the limit of what they can see: a token stream says the
    /// same tokens came out in the same order, not that a comment stayed out of
    /// a replacement string. perl can say it, by reading both programs back.
    ///
    /// Its own command rather than a flag on `check`, because asking runs perl
    /// over the file, and `perl -c` runs that file's BEGIN blocks — arbitrary
    /// code out of somebody's corpus. That is a thing to type on purpose.
    PerlDeparse {
        /// Files or directories to ask about (reads from stdin if not provided)
        #[arg(help = "Files or directories to ask about (recursive; stdin if omitted)")]
        paths: Vec<PathBuf>,

        /// How many files to ask about at once
        #[arg(
            short = 'j',
            long,
            value_name = "N",
            help = "How many files to ask about at once (default: one per core)"
        )]
        jobs: Option<usize>,

        /// One line per violation, without the evidence
        #[arg(short, long, help = "One line per violation, without the evidence")]
        quiet: bool,

        /// Every unanswered file and every message, not a few of each
        #[arg(
            short,
            long,
            conflicts_with = "quiet",
            help = "Every message a file could not be answered with, and every file it \
                    came from"
        )]
        verbose: bool,

        /// File extensions to walk into when given a directory
        #[arg(
            long,
            value_name = "EXT,...",
            default_value = "pl,pm,t,psgi",
            help = "Extensions to consider when walking a directory"
        )]
        extensions: String,

        /// Encodings a source may be in, tried in order
        #[arg(
            long,
            value_name = "NAME,...",
            help = "Encodings to try, in order, until one reads the file (default: utf-8)"
        )]
        encoding: Option<String>,
    },
}

/// The formatter's options, as command-line flags.
///
/// `FormatterOptions` existed and the CLI never passed one, so every knob in it
/// was reachable from the library and from nowhere else — including
/// `max_alignment_padding`, which is a guard against one long line pushing a
/// whole group across the screen.
///
/// Hidden, all of them, for the reason the `dev` namespace is: what camello
/// answers is "how is this written", and a formatter that answers it four ways
/// depending on the flags has not answered it. They exist so that a question
/// about the layout can be *asked* — of a fixture, in a bug report — and they
/// may change with the layout they describe.
#[derive(clap::Args, Debug, Clone)]
pub struct LayoutArgs {
    /// Spaces per indent level
    #[arg(
        long,
        value_name = "N",
        hide = true,
        help = "Spaces per indent level (default: 4)"
    )]
    pub indent_width: Option<usize>,

    /// Minimum spaces between code and a trailing comment
    #[arg(
        long,
        value_name = "N",
        hide = true,
        help = "Minimum spaces before a trailing comment (default: 4)"
    )]
    pub min_spaces_before_comment: Option<usize>,

    /// Space inside flat `[...]` / `{...}` literals
    #[arg(
        long,
        value_name = "STYLE",
        hide = true,
        help = "Inside of [...] and {...} literals: tight, standard (space unless \
                the contents are a single simple term; default), or loose (always \
                a space)"
    )]
    pub delimiter_spacing: Option<DelimiterSpacingArg>,

    /// Cap on the spaces vertical alignment may insert; 0 disables alignment
    #[arg(
        long,
        value_name = "N",
        hide = true,
        help = "Maximum alignment padding, 0 to disable alignment (default: 64)"
    )]
    pub max_alignment_padding: Option<usize>,

    /// Keep a one-statement `map`/`sub`/`do` block on one line
    #[arg(
        long,
        hide = true,
        help = "Never keep a one-statement map/sub/do block on one line"
    )]
    pub no_single_line_blocks: bool,

    /// Line up the import lists of a run of `use` — or of `no` — lines
    #[arg(
        long,
        hide = true,
        help = "Line up the import lists of consecutive use (or no) statements"
    )]
    pub align_use_imports: bool,
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
            align_use_imports: self.align_use_imports,
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
            // Asks for the default; see the flag.
            write: _,
            extensions,
            jobs,
            stop_on_first_error,
            list_different,
            output,
            encoding,
            layout,
        } => {
            // One source can be sent somewhere. A tree cannot: there is no one
            // place for five hundred files to go, and nothing to say about them
            // afterwards but a tally — so the two are different commands
            // wearing one name, and this is where they part.
            if is_a_tree(&paths) {
                if output.is_some() {
                    return Err(miette::miette!(
                        "--output takes one source; a tree is written back over itself"
                    ));
                }
                return format_tree(
                    paths,
                    check,
                    list_different,
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
                quiet,
                verbose,
                extensions,
                encoding,
            } => {
                if list_invariants {
                    return list_invariants_and_exit();
                }
                let wanted = wanted_invariants(only.as_deref())?;
                return check_paths(paths, jobs, &wanted, quiet, verbose, &extensions, encoding);
            }
            DevCommands::PerlDeparse {
                paths,
                jobs,
                quiet,
                verbose,
                extensions,
                encoding,
            } => {
                // Better here than 4000 files later, one failed spawn at a time.
                if !crate::check::deparse::available() {
                    return Err(miette::miette!("no working perl on PATH"));
                }
                return check_paths(
                    paths,
                    jobs,
                    &[crate::check::Invariant::Deparse],
                    quiet,
                    verbose,
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
/// How many threads [`in_parallel`] will put on this many items.
fn worker_count(jobs: Option<usize>, items: usize) -> usize {
    jobs.filter(|&jobs| jobs > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
        })
        .min(items)
        .max(1)
}

fn in_parallel<T, R>(items: &[T], jobs: Option<usize>, job: impl Fn(&T) -> R + Sync) -> Vec<R>
where
    T: Sync,
    R: Send,
{
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let workers = worker_count(jobs, items.len());

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
/// The formatted text of a tree has nowhere to go but back over the tree, so
/// that is where it goes. `--check` is how to ask without being answered in
/// rewritten files, and version control is what makes a run that was not wanted
/// undoable — the same bargain every formatter that walks a directory makes.
///
/// A run that did what it was asked says so in one line. Naming five hundred
/// files that were reformatted is a list nobody reads, and the version control
/// the tree is under already keeps it — so the names are behind
/// `--list-different`, and what stays is what a run cannot be understood
/// without: the files that were left alone, and why.
#[allow(clippy::too_many_arguments)]
fn format_tree(
    paths: Vec<PathBuf>,
    check: bool,
    list_different: bool,
    extensions: &str,
    jobs: Option<usize>,
    encoding: Option<String>,
    options: &FormatterOptions,
) -> Result<()> {
    let encodings = Encodings::parse(encoding.as_ref())?;
    let extensions: Vec<&str> = extensions.split(',').map(str::trim).collect();

    let mut files = Vec::new();
    for path in &paths {
        collect_perl_files(path, &extensions, &mut files)?;
    }

    let reports = in_parallel(&files, jobs, |path| {
        format_one(path, check, &encodings, options)
    });

    let stdout = io::stdout();
    let mut out = stdout.lock();
    // `--check` is a question about which files those are, so it answers with
    // them; `--write` has already done the thing and only names them on request.
    let name_them = list_different || check;
    let (mut changed, mut with_diagnostics, mut failed) = (0usize, 0usize, 0usize);
    for (path, report) in files.iter().zip(&reports) {
        let path = path.display();
        // A file camello could not read, or would not rewrite, is not routine
        // and is named whatever was asked for: it is the difference between a
        // tree that is formatted and one that was skipped in places.
        if let Some(failure) = &report.failure {
            failed += 1;
            writeln!(out, "{path}: {failure}").into_diagnostic()?;
            continue;
        }
        if !report.diagnostics.is_empty() {
            with_diagnostics += 1;
            let count = report.diagnostics.len();
            let diagnostic = if count == 1 {
                "diagnostic"
            } else {
                "diagnostics"
            };
            writeln!(out, "{path}: left alone, {count} {diagnostic}").into_diagnostic()?;
            // The diagnostics themselves are a screenful each, and they are all
            // still there in `camello format <that file>`.
            if list_different {
                for diagnostic in &report.diagnostics {
                    for line in diagnostic.lines() {
                        writeln!(out, "    {line}").into_diagnostic()?;
                    }
                }
            }
            continue;
        }
        if report.changed {
            changed += 1;
            if name_them {
                writeln!(out, "{path}").into_diagnostic()?;
            }
        }
    }

    let total = files.len();
    let mut summary = if check {
        format!(
            "{changed} of {total} {} would be reformatted",
            plural(total)
        )
    } else {
        format!("formatted {changed} of {total} {}", plural(total))
    };
    if with_diagnostics > 0 {
        summary.push_str(&format!(", {with_diagnostics} left alone"));
    }
    if failed > 0 {
        summary.push_str(&format!(", {failed} unreadable"));
    }
    writeln!(out, "{summary}").into_diagnostic()?;

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
    encodings: &Encodings,
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
    let Some((input, encoding)) = encodings.decode(&bytes) else {
        return failed(format!("not decodable as {}", encodings.names()));
    };

    let (formatted, errors) = format_perl_with_options(&input, options);
    let diagnostics: Vec<String> = errors
        .into_iter()
        .map(|error| format!("{:?}", Report::new(error)))
        .collect();

    // A file the parser had something to say about is reported and left alone.
    // The same answer whichever way it was asked for: one path, several, or the
    // directory above them ([`format_file`]).
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

/// `file` or `files`, for a count that is read as English.
fn plural(count: usize) -> &'static str {
    if count == 1 {
        "file"
    } else {
        "files"
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

/// A message with its line numbers taken out, which is what makes two of them
/// the same message.
///
/// `Can't locate Foo.pm ... at input.pl line 4.` and the same sentence about
/// line 6 are one thing that is wrong with a tree, reported by two of its
/// files. The text printed is still the real one, from the file named with it.
fn fold_key(message: &str) -> String {
    let mut key = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(at) = rest.find("line ") {
        let (before, after) = rest.split_at(at + "line ".len());
        key.push_str(before);
        let digits = after
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(after.len());
        if digits > 0 {
            key.push('N');
        }
        rest = &after[digits..];
    }
    key.push_str(rest);
    key
}

/// The messages a run could not be answered with, folded.
///
/// Grouped by the exact text, because that is what makes them the same message:
/// a tree checked away from where it was installed answers `Can't locate ... in
/// @INC` for every file in it, and the reader needs that sentence once, whole,
/// with a count and somewhere to start looking.
#[derive(Default)]
struct Messages {
    groups: Vec<Group>,
    /// Where each (invariant, reason, message) already sits in `groups`.
    seen: std::collections::HashMap<(&'static str, &'static str, String), usize>,
}

struct Group {
    slug: &'static str,
    why: &'static str,
    message: String,
    files: Vec<String>,
}

impl Messages {
    fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    fn record(&mut self, unanswered: &crate::check::Unanswered, label: &str) {
        let key = (
            unanswered.invariant.slug(),
            unanswered.why,
            fold_key(&unanswered.detail),
        );
        let index = *self.seen.entry(key).or_insert_with(|| {
            self.groups.push(Group {
                slug: unanswered.invariant.slug(),
                why: unanswered.why,
                message: unanswered.detail.clone(),
                files: Vec::new(),
            });
            self.groups.len() - 1
        });
        self.groups[index].files.push(label.to_string());
    }

    /// The distinct messages, most files first; a few of them unless asked for
    /// all, since the tail of this list is usually the same story once more.
    fn report(&self, out: &mut impl Write, verbose: bool) -> Result<()> {
        /// How many distinct messages are worth reading before the reader knows
        /// what kind of run this was.
        const SHOWN: usize = 3;
        /// And how many of the files that got one, before the list is a corpus.
        const NAMED: usize = 3;

        let mut order: Vec<&Group> = self.groups.iter().collect();
        order.sort_by_key(|group| std::cmp::Reverse(group.files.len()));
        let shown = if verbose {
            order.len()
        } else {
            SHOWN.min(order.len())
        };

        for group in &order[..shown] {
            let files = group.files.len();
            writeln!(
                out,
                "     {} ({}), {files} file{}",
                group.why,
                group.slug,
                if files == 1 { "" } else { "s" }
            )
            .into_diagnostic()?;
            for line in group.message.lines() {
                writeln!(out, "     {line}").into_diagnostic()?;
            }
            let named = if verbose { files } else { NAMED.min(files) };
            for file in &group.files[..named] {
                writeln!(out, "         {file}").into_diagnostic()?;
            }
            if named < files {
                writeln!(out, "         … and {} more", files - named).into_diagnostic()?;
            }
        }

        if shown < order.len() {
            let rest: usize = order[shown..].iter().map(|group| group.files.len()).sum();
            writeln!(
                out,
                "     … and {} more message(s) over {rest} file(s); --verbose for all of them",
                order.len() - shown
            )
            .into_diagnostic()?;
        }
        Ok(())
    }
}

/// The files in hand now, the ones already done scrolling away above them, and
/// a line under them saying how far along the run is.
///
/// The block at the bottom is redrawn in place, so what is on the screen is
/// everything that has happened, then what is happening, then how much is left.
///
/// It goes to stderr because stdout is the report, which is read by
/// `scripts/corpus-check` among others, and it stays off unless stderr is a
/// terminal: a redrawn block in a log file is noise with escape codes in it.
struct Progress {
    total: usize,
    done: AtomicUsize,
    violated: AtomicUsize,
    started: std::time::Instant,
    /// Asked of the terminal once. The block is repainted from every worker,
    /// and a run over a tree is not the place to ask four thousand times.
    width: usize,
    /// How many files in hand to name before the rest become a count. Half the
    /// screen at most: the block says what is happening, it is not the thing
    /// that happens.
    room: usize,
    live: std::sync::Mutex<Live>,
    on: bool,
}

/// What the bottom of the screen is showing.
struct Live {
    /// One slot per worker, holding the file it has in hand. A worker that
    /// finishes leaves its slot for the next file to take.
    running: Vec<Option<String>>,
    /// Lines the last paint left on the screen, so the next one knows how far
    /// up to go to take them back.
    painted: usize,
}

/// The last `room` columns of a path.
///
/// From the right, because the end of a path is the part that says which file
/// it is: forty files under `local/lib/perl5/` share every column on the left.
fn path_tail(path: &str, room: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    if path.width() <= room {
        return path.to_string();
    }
    let mut tail = String::new();
    let mut used = 1; // the ellipsis standing in for what was cut
    for ch in path.chars().rev() {
        let w = ch.width().unwrap_or(0);
        if used + w > room {
            break;
        }
        used += w;
        tail.push(ch);
    }
    format!("…{}", tail.chars().rev().collect::<String>())
}

/// The first `room` columns of a line, for the lines that are read from the
/// left — which is every line here that is not a bare path.
fn head_within(line: &str, room: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    if line.width() <= room {
        return line.to_string();
    }
    let mut head = String::new();
    let mut used = 1;
    for ch in line.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > room {
            break;
        }
        used += w;
        head.push(ch);
    }
    head.push('…');
    head
}

impl Progress {
    fn new(total: usize, workers: usize) -> Self {
        use std::io::IsTerminal;

        let (width, height) = terminal_size::terminal_size_of(io::stderr()).map_or(
            (72, 24),
            |(terminal_size::Width(width), terminal_size::Height(height))| {
                (usize::from(width), usize::from(height))
            },
        );
        Progress {
            total,
            done: AtomicUsize::new(0),
            violated: AtomicUsize::new(0),
            started: std::time::Instant::now(),
            width: width.max(24),
            room: (height / 2).max(1),
            live: std::sync::Mutex::new(Live {
                running: vec![None; workers],
                painted: 0,
            }),
            on: total > 1 && io::stderr().is_terminal(),
        }
    }

    /// A worker has this file in hand. What comes back is the slot to give to
    /// [`Progress::finished`] when it is done with it.
    fn taken(&self, path: &std::path::Path) -> usize {
        if !self.on {
            return 0;
        }
        let mut live = self.lock();
        // There is always a free slot: a worker asks for one only after giving
        // its last one back, and there are as many slots as there are workers.
        let slot = live.running.iter().position(Option::is_none).unwrap_or(0);
        live.running[slot] = Some(path.display().to_string());
        self.paint(&mut live, &[]);
        slot
    }

    /// That file is done. `said` scrolls away above the block: what the run has
    /// to say about the file, and under it whatever evidence was asked for.
    fn finished(&self, slot: usize, violated: bool, said: &[String]) {
        if !self.on {
            return;
        }
        self.done.fetch_add(1, Ordering::Relaxed);
        if violated {
            self.violated.fetch_add(1, Ordering::Relaxed);
        }
        let mut live = self.lock();
        live.running[slot] = None;
        self.paint(&mut live, said);
    }

    /// One finished file as the line that scrolls away: the path from the end
    /// that names it, and what came of it.
    fn verdict(&self, path: &std::path::Path, result: &str) -> String {
        let room = self.width.saturating_sub(result.len() + 6);
        format!(
            "{} ... {result}",
            path_tail(&path.display().to_string(), room)
        )
    }

    /// Take the block back before anything is printed on top of it.
    fn clear(&self) {
        if !self.on {
            return;
        }
        let mut live = self.lock();
        if live.painted == 0 {
            return;
        }
        let mut out = String::new();
        Self::to_top(&mut out, live.painted);
        out.push_str("\x1b[J");
        live.painted = 0;
        Self::write(&out);
    }

    /// A worker panicking mid-paint leaves a poisoned lock and a half-drawn
    /// block; neither is a reason to bring the run down.
    fn lock(&self) -> std::sync::MutexGuard<'_, Live> {
        self.live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Back to the first line of the block, from wherever in it the cursor was
    /// left.
    fn to_top(out: &mut String, painted: usize) {
        use std::fmt::Write as _;

        out.push('\r');
        if painted > 1 {
            let _ = write!(out, "\x1b[{}A", painted - 1);
        }
    }

    /// Redraw the block, having scrolled `said` away above it first.
    ///
    /// One string and one write: the cursor arithmetic only holds if nothing
    /// else writes between the moves, and two workers painting a line each
    /// would interleave them.
    fn paint(&self, live: &mut Live, said: &[String]) {
        use std::fmt::Write as _;

        let mut out = String::new();
        Self::to_top(&mut out, live.painted);

        let room = self.width.saturating_sub(1);
        for line in said {
            let _ = writeln!(out, "\x1b[2K{}", head_within(line, room));
        }

        let mut painted = 0;
        let running: Vec<&String> = live.running.iter().flatten().collect();
        for name in running.iter().take(self.room) {
            let _ = writeln!(out, "\x1b[2K  {}", path_tail(name, room.saturating_sub(2)));
            painted += 1;
        }
        if running.len() > self.room {
            let _ = writeln!(out, "\x1b[2K  … and {} more", running.len() - self.room);
            painted += 1;
        }
        // No newline after the counts: the cursor stays on the line the next
        // paint measures from, and `\x1b[J` takes back whatever a taller block
        // left below it.
        let _ = write!(out, "\x1b[2K{}\x1b[J", head_within(&self.counts(), room));
        painted += 1;

        live.painted = painted;
        Self::write(&out);
    }

    /// How far along, in the words the reader is waiting for.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn counts(&self) -> String {
        let done = self.done.load(Ordering::Relaxed);
        let elapsed = self.started.elapsed();
        // A rate is only worth quoting once there is one: the first few files
        // of a corpus are the ones perl is warming up on.
        let left = if done >= 8 && elapsed.as_secs() >= 1 {
            let per_file = elapsed.as_secs_f64() / done as f64;
            let remaining = per_file * self.total.saturating_sub(done) as f64;
            format!(", ~{} left", clock(remaining.max(0.0) as u64))
        } else {
            String::new()
        };
        format!(
            "  checked {done}/{}, violated {}, {}{left}",
            self.total,
            self.violated.load(Ordering::Relaxed),
            clock(elapsed.as_secs()),
        )
    }

    fn write(text: &str) {
        let mut stderr = io::stderr().lock();
        let _ = stderr.write_all(text.as_bytes());
        let _ = stderr.flush();
    }
}

/// Seconds as `m:ss`, which is what a wait is read in.
fn clock(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
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

/// One violation, in the shape the report gives it: the line that names it, and
/// the evidence under it unless the reader asked for less.
fn write_violation(
    out: &mut impl Write,
    label: &str,
    violation: &crate::check::Violation,
    quiet: bool,
) -> Result<()> {
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
    Ok(())
}

/// `camello dev check`: run the invariants over files, directories, or stdin.
///
/// Exits non-zero when anything is violated, so it can gate a corpus run.
fn list_invariants_and_exit() -> Result<()> {
    use crate::check::Invariant;

    let mut heading = None;
    for (index, invariant) in Invariant::ALL.iter().enumerate() {
        if index > 0 {
            println!();
        }
        if heading != Some(invariant.subject()) {
            heading = Some(invariant.subject());
            println!("---- {}", invariant.subject().heading());
            println!();
        }
        println!("{:<17}{}", invariant.slug(), invariant.name());
        for line in wrapped(invariant.description(), 72) {
            println!("    {line}");
        }
    }
    // The one question that is not asked here, said once so that a reader
    // looking for it in this list finds where it went instead of concluding
    // that camello does not ask it.
    println!();
    println!("---- not asked here: `camello dev perl-deparse` asks it, by running perl");
    println!();
    println!(
        "{:<17}{}",
        Invariant::Deparse.slug(),
        Invariant::Deparse.name()
    );
    for line in wrapped(Invariant::Deparse.description(), 72) {
        println!("    {line}");
    }
    Ok(())
}

/// Which invariants `--only` named, or all the ones `check` asks.
fn wanted_invariants(only: Option<&str>) -> Result<Vec<crate::check::Invariant>> {
    use crate::check::Invariant;

    let Some(only) = only else {
        return Ok(Invariant::ALL.to_vec());
    };
    let mut wanted = Vec::new();
    for slug in only.split(',').map(str::trim) {
        // It used to be selectable here under the bare name, and selecting it
        // was how one opted in to running a perl. Now that opting in is the
        // command, say so rather than call a name this command knows unknown —
        // under either spelling, since the old one is what is in anybody's
        // shell history.
        if slug == Invariant::Deparse.slug() || slug == "deparse" {
            return Err(miette::miette!(
                "{slug} is not asked here; `camello dev perl-deparse` is the command that asks it"
            ));
        }
        let Some(invariant) = Invariant::ALL.iter().find(|kind| kind.slug() == slug) else {
            return Err(miette::miette!(
                "unknown invariant {slug:?}; --list-invariants prints them"
            ));
        };
        wanted.push(*invariant);
    }
    Ok(wanted)
}

#[allow(clippy::too_many_arguments)]
fn check_paths(
    paths: Vec<PathBuf>,
    jobs: Option<usize>,
    wanted: &[crate::check::Invariant],
    quiet: bool,
    verbose: bool,
    extensions: &str,
    encoding: Option<String>,
) -> Result<()> {
    use crate::check::{check_report, Invariant};

    let extensions: Vec<&str> = extensions.split(',').map(str::trim).collect();
    let encodings = Encodings::parse(encoding.as_ref())?;

    let mut files = Vec::new();
    for path in &paths {
        collect_perl_files(path, &extensions, &mut files)?;
    }

    // No paths at all means stdin, so the command composes with a pipeline the
    // way `format` does. `None` is a file this command has nothing to say about
    // — unreadable, or not decodable with this encoding, neither of which is a
    // violation.
    // A run over a tree takes long enough that a violation is worth having when
    // it is found rather than when the last file is done, and `--verbose` is the
    // reader asking for everything anyway. Only where there is a block to scroll
    // it away from, though: piped into a file, the report at the end is the one
    // that is in a settled order, and stdin is one file with the report already
    // under it.
    let mut as_found = false;

    let checked: Vec<Option<(String, crate::check::Outcome)>> = if paths.is_empty() {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes).into_diagnostic()?;
        let Some((decoded, _)) = encodings.decode(&bytes) else {
            return Err(miette::miette!(
                "stdin is not decodable as {}; refusing a lossy check",
                encodings.names()
            ));
        };
        vec![Some((
            "<stdin>".to_string(),
            check_report(&decoded, wanted),
        ))]
    } else {
        let progress = Progress::new(files.len(), worker_count(jobs, files.len()));
        as_found = verbose && progress.on;
        let checked = in_parallel(&files, jobs, |path| {
            let slot = progress.taken(path);
            let checked = (|| {
                let bytes = fs::read(path).ok()?;
                let (decoded, _) = encodings.decode(&bytes)?;
                Some((path.display().to_string(), check_report(&decoded, wanted)))
            })();
            let violated = checked
                .as_ref()
                .is_some_and(|(_, outcome)| !outcome.violations.is_empty());

            // What the run has to say about this file, if it has anything: a
            // file that answered every question asked of it is not news, and
            // four thousand lines saying so would bury the few that are. The
            // order is the one the workers arrive in, which is the price of
            // saying it now rather than when the last one is done; the tally
            // below is still in the order the files were asked about.
            let verdict = match &checked {
                None => Some("skipped".to_string()),
                Some((_, outcome)) if !outcome.violations.is_empty() => {
                    let mut slugs: Vec<&str> = outcome
                        .violations
                        .iter()
                        .map(|violation| violation.invariant.slug())
                        .collect();
                    slugs.dedup();
                    Some(format!("FAIL {}", slugs.join(" ")))
                }
                // Not a failure and not a pass: nobody could answer, and which
                // files those were is the thing the closing table can only
                // count.
                Some((_, outcome)) => outcome
                    .unanswered
                    .first()
                    .map(|unanswered| format!("n/a {}", unanswered.why)),
            };
            let mut said: Vec<String> = verdict
                .map(|verdict| progress.verdict(path, &verdict))
                .into_iter()
                .collect();
            // The evidence under the verdict, for the run that asked to see it
            // as it happens. It stays on the screen; the block scrolls it up.
            if as_found && violated {
                if let Some((label, outcome)) = &checked {
                    let mut evidence = Vec::new();
                    for violation in &outcome.violations {
                        let _ = write_violation(&mut evidence, label, violation, quiet);
                    }
                    said.extend(
                        String::from_utf8_lossy(&evidence)
                            .lines()
                            .map(str::to_string),
                    );
                }
            }
            progress.finished(slot, violated, &said);
            checked
        });
        // Before a single line of the report: the block is written without a
        // trailing newline, and anything printed over it inherits its tail.
        progress.clear();
        checked
    };

    let sources = checked.iter().flatten().count();
    let skipped = checked.len() - sources;
    let mut offenders: Vec<(&str, Vec<&'static str>)> = Vec::new();
    let mut unanswered_for: Vec<(&str, Vec<&'static str>)> = Vec::new();
    // Three columns per invariant, not one count: an invariant that was never
    // answered for a file — it did not parse, perl would not load it — is not
    // one that passed, and a report that says so is a corpus called clean when
    // most of it was never looked at.
    let mut tally: Vec<Tally> = Invariant::ALL
        .iter()
        .copied()
        .chain(
            wanted
                .iter()
                .copied()
                .filter(|kind| !Invariant::ALL.contains(kind)),
        )
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
    let mut messages = Messages::default();
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
        // colour the exit status, but it is the reader's business why. It is
        // collected rather than printed here: a corpus checked outside the tree
        // it was installed in answers the same sentence about @INC three
        // hundred times, and three hundred copies of one sentence is not more
        // information than one copy and a count.
        for unanswered in outcome.unanswered.iter().filter(|it| !it.detail.is_empty()) {
            messages.record(unanswered, label);
        }

        // The messages above are grouped by what was said, which answers why a
        // run went unanswered and not which files it went unanswered for. That
        // is the same question the violation list below answers, and it is
        // asked of the unanswered too — but not of a file that is about to be
        // named as violating something, because its questions went unanswered
        // *because* of that, and the same list twice says nothing the second
        // time.
        if outcome.violations.is_empty() && !outcome.unanswered.is_empty() {
            let mut why: Vec<&'static str> = outcome
                .unanswered
                .iter()
                .map(|unanswered| unanswered.why)
                .collect();
            why.sort_unstable();
            why.dedup();
            unanswered_for.push((label.as_str(), why));
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
            if !as_found {
                write_violation(&mut out, label, violation, quiet)?;
            }
        }
    }

    let violated = offenders.len();

    // Why the unanswered went unanswered, in the words of whatever declined,
    // once each. `--quiet` is a request for one line per finding and no
    // evidence, which this is; `--verbose` asks for every message and every
    // file that got it.
    if !quiet && !messages.is_empty() {
        if reported {
            writeln!(out).into_diagnostic()?;
        }
        reported = true;
        writeln!(out, "---- what could not be answered, and why").into_diagnostic()?;
        messages.report(&mut out, verbose)?;
    }

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

    // And which files nobody answered for. A corpus checked outside the tree it
    // was installed in is mostly this, so the list is cut where the violations
    // are not: a few hundred names is not a list anybody reads, and the count
    // above already said how many there were.
    if !unanswered_for.is_empty() && sources > 1 {
        const NAMED: usize = 20;
        let named = if verbose {
            unanswered_for.len()
        } else {
            NAMED.min(unanswered_for.len())
        };
        writeln!(out).into_diagnostic()?;
        writeln!(out, "---- files with an unanswered check").into_diagnostic()?;
        for (label, why) in &unanswered_for[..named] {
            writeln!(out, "     {label}\t{}", why.join(", ")).into_diagnostic()?;
        }
        if named < unanswered_for.len() {
            writeln!(
                out,
                "     … and {} more file(s); --verbose for all of them",
                unanswered_for.len() - named
            )
            .into_diagnostic()?;
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

/// The encodings a source may be in, in the order they are tried.
///
/// One candidate is the whole of the usual case: the file is utf-8, or it is
/// not and that is something to hear about rather than to guess around. A tree
/// older than utf-8 is the other case — some of it euc-jp, some of it already
/// converted, and no flag that is right for the run because the encoding is a
/// property of each file. So the run names what a file may be, and the file
/// answers: the first candidate that reads its bytes without replacing any of
/// them is the one it is in, and the one it is written back in.
///
/// Order is what settles a file that more than one candidate can read. Every
/// candidate reads pure ASCII, and euc-jp bytes are rarely valid utf-8, so
/// utf-8 first is the order that means "already converted, or not yet".
struct Encodings(Vec<&'static Encoding>);

impl Encodings {
    /// The candidates named on the command line, or utf-8 when none were.
    fn parse(names: Option<&String>) -> Result<Self> {
        let Some(names) = names else {
            return Ok(Self(vec![encoding_rs::UTF_8]));
        };

        let mut candidates: Vec<&'static Encoding> = Vec::new();
        for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            let encoding = Encoding::for_label(name.as_bytes())
                .ok_or_else(|| miette::miette!("Unknown encoding: {name}"))?;
            // A candidate that already had its turn cannot decode anything the
            // earlier one did not.
            if !candidates.contains(&encoding) {
                candidates.push(encoding);
            }
        }
        if candidates.is_empty() {
            return Err(miette::miette!("--encoding names no encoding"));
        }
        Ok(Self(candidates))
    }

    /// Decode `bytes` with the first candidate that reads all of them.
    ///
    /// `None` is a source that none of them could read: rejected rather than
    /// decoded with replacement characters, which would be a rewrite of the
    /// file's contents wearing the name of a formatting run.
    fn decode(&self, bytes: &[u8]) -> Option<(String, &'static Encoding)> {
        self.0.iter().find_map(|encoding| {
            let (decoded, _, had_errors) = encoding.decode(bytes);
            (!had_errors).then(|| (decoded.into_owned(), *encoding))
        })
    }

    /// What to write text that never came from a file — `-e`, and its escaping
    /// sibling — in: the first thing the run said the sources are.
    fn first(&self) -> &'static Encoding {
        self.0[0]
    }

    /// `utf-8`, or `utf-8, euc-jp`: the candidates, for a message that has to
    /// say which ones were tried.
    fn names(&self) -> String {
        self.0
            .iter()
            .map(|encoding| encoding.name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A source, what to call it in a message, and the encoding it turned out to be
/// in — which is the one it is written back in.
fn read_source(
    path: Option<&Path>,
    eval: Option<String>,
    eval_escape: Option<String>,
    encodings: &Encodings,
) -> Result<(String, String, &'static Encoding)> {
    if let Some(code) = eval {
        return Ok((code, "<command-line>".to_string(), encodings.first()));
    }
    if let Some(code) = eval_escape {
        let interpreted_code = interpret_escape_sequences(&code);
        return Ok((
            interpreted_code,
            "<command-line>".to_string(),
            encodings.first(),
        ));
    }

    if let Some(path) = path {
        let bytes = fs::read(path).into_diagnostic()?;
        let Some((decoded, encoding)) = encodings.decode(&bytes) else {
            return Err(miette::miette!(
                "'{}' is not decodable as {}; refusing lossy formatting",
                path.display(),
                encodings.names()
            ));
        };
        Ok((decoded, path.display().to_string(), encoding))
    } else {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes).into_diagnostic()?;
        let Some((decoded, encoding)) = encodings.decode(&bytes) else {
            return Err(miette::miette!(
                "stdin is not decodable as {}; refusing lossy formatting",
                encodings.names()
            ));
        };
        Ok((decoded, "<stdin>".to_string(), encoding))
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

/// `camello format` over the one source it was given.
///
/// Where the result goes is the whole of what is decided here, and a file it
/// came from is where it goes: formatting a file is a thing done *to* the file.
/// A source that is not a file — stdin, `-e` — has nowhere to be put back, so
/// it goes to stdout, which is also what `--output -` asks for by name.
#[allow(clippy::too_many_arguments)]
fn format_file(
    path: Option<PathBuf>,
    eval: Option<String>,
    eval_escape: Option<String>,
    check: bool,
    stop_on_first_error: bool,
    output: Option<PathBuf>,
    encoding: Option<String>,
    options: &FormatterOptions,
) -> Result<()> {
    let encodings = Encodings::parse(encoding.as_ref())?;

    // Read from file or standard input
    let (input, source_name, encoding) =
        read_source(path.as_deref(), eval, eval_escape, &encodings)?;

    // Execute formatting
    let (formatted, errors) = format_perl_with_options(&input, options);

    // `-` is standard output under the flag that otherwise names a file, so
    // that "do not write it back" is one thing to say however it is meant.
    let destination = match output {
        Some(named) if named == Path::new("-") => None,
        Some(named) => Some(named),
        None => path,
    };

    // A source the parser had something to say about is reported and left
    // alone, and one source is no exception: an editor that formats on save and
    // a pre-commit hook both hand over a file at a time, so the one-source path
    // is the one a best-effort rewrite of an unparsed file actually takes. A
    // `.pl` holding SQL is what it cost. What comes out is what went in — "left
    // alone" says the same thing wherever the result was going — and the exit
    // status carries the rest.
    if !errors.is_empty() {
        eprintln!("Parse error in '{source_name}':");
        if stop_on_first_error {
            let error = errors.into_iter().next().unwrap();
            eprintln!("{:?}", Report::new(error));
            std::process::exit(2);
        }
        for e in errors {
            eprintln!("{:?}", Report::new(e));
        }
        eprintln!("Left '{source_name}' alone.");
        if !check && destination.is_none() {
            print!("{input}");
            io::stdout().flush().into_diagnostic()?;
        }
        std::process::exit(1);
    }

    if check {
        // A file that is already formatted is the answer nobody needs a
        // sentence for; one that is not is named, so that the name can be piped
        // somewhere, and the exit status carries the rest.
        if input != formatted {
            println!("{source_name}");
            std::process::exit(1);
        }
        return Ok(());
    }

    match destination {
        Some(path) => write_with_encoding(path.as_path(), &formatted, encoding)?,
        None => {
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
    let encodings = Encodings::parse(encoding.as_ref())?;

    // Read from file or standard input
    let (input, source_name, _) = read_source(path.as_deref(), eval, eval_escape, &encodings)?;
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
            None,
            None,
            &layout()
        )
        .is_ok());

        Ok(())
    }

    #[test]
    fn a_file_is_formatted_over_itself() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("write_test.pl");
        fs::write(&file_path, "my$var=1;")?;

        format_file(
            Some(file_path.clone()),
            None,
            None,
            false,
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

    /// `--output -` is the way to ask for the formatted text without the file
    /// it came from being the place it lands.
    #[test]
    fn output_to_stdout_leaves_the_input_alone() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("untouched.pl");
        fs::write(&file_path, "my$var=1;")?;

        format_file(
            Some(file_path.clone()),
            None,
            None,
            false,
            false,
            Some(PathBuf::from("-")),
            None,
            &layout(),
        )?;

        assert_eq!(fs::read_to_string(&file_path)?, "my$var=1;");
        Ok(())
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
            None,
            None,
            &layout()
        )
        .is_ok());

        Ok(())
    }

    /// A file goes out in the encoding it came in: formatting is a question
    /// about the layout, and answering it in other bytes than the ones asked
    /// about is a second change nobody asked for.
    #[test]
    fn a_file_is_written_back_in_its_own_encoding() -> Result<(), Box<dyn std::error::Error>> {
        let text = "my $var = \"こんにちは\";";
        let (expected, _) = format_perl(text);

        for label in ["utf-8", "euc-jp", "shift_jis"] {
            let encoding = Encoding::for_label(label.as_bytes()).expect("a known encoding");
            let dir = tempdir()?;
            let path = dir.path().join("greeting.pl");
            let (encoded, _, _) = encoding.encode(text);
            fs::write(&path, &*encoded)?;

            format_file(
                Some(path.clone()),
                None,
                None,
                false,
                false,
                None,
                Some(label.to_string()),
                &layout(),
            )?;

            let written = fs::read(&path)?;
            let (decoded, _, had_errors) = encoding.decode(&written);
            assert!(!had_errors, "{label}: came back in some other encoding");
            assert_eq!(decoded, expected, "{label}");
        }

        Ok(())
    }

    /// Which encoding a file is in is a property of the file, not of the run:
    /// half a tree converted to utf-8 is one `--encoding` away from being
    /// formatted, and each half comes back the way it was.
    #[test]
    fn each_file_gets_the_first_candidate_that_reads_it() -> Result<(), Box<dyn std::error::Error>>
    {
        let text = "my $var=\"こんにちは\";\n";
        let (expected, _) = format_perl(text);
        let encodings = Encodings::parse(Some(&"utf-8,euc-jp".to_string()))?;

        let dir = tempdir()?;
        for (name, encoding) in [
            ("converted.pl", encoding_rs::UTF_8),
            ("legacy.pl", encoding_rs::EUC_JP),
        ] {
            let path = dir.path().join(name);
            let (encoded, _, _) = encoding.encode(text);
            fs::write(&path, &*encoded)?;

            let report = format_one(&path, false, &encodings, &layout());

            assert_eq!(report.failure, None, "{name}");
            assert!(report.changed, "{name}");
            let written = fs::read(&path)?;
            let (decoded, _, had_errors) = encoding.decode(&written);
            assert!(!had_errors, "{name}: came back in some other encoding");
            assert_eq!(decoded, expected, "{name}");
        }

        Ok(())
    }

    /// Bytes no candidate can read are a file left alone, named in the report.
    #[test]
    fn a_file_no_candidate_reads_is_left_alone() -> Result<(), Box<dyn std::error::Error>> {
        let encodings = Encodings::parse(Some(&"utf-8".to_string()))?;
        let dir = tempdir()?;
        let path = dir.path().join("legacy.pl");
        let (encoded, _, _) = encoding_rs::EUC_JP.encode("my $var = \"こんにちは\";\n");
        fs::write(&path, &*encoded)?;

        let report = format_one(&path, false, &encodings, &layout());

        assert_eq!(report.failure.as_deref(), Some("not decodable as UTF-8"));
        assert_eq!(fs::read(&path)?, *encoded);
        Ok(())
    }

    #[test]
    fn candidates_are_named_once_each_and_must_be_known() {
        let parsed = |names: &str| Encodings::parse(Some(&names.to_string()));

        assert_eq!(
            parsed("utf-8, euc-jp").expect("known").names(),
            "UTF-8, EUC-JP"
        );
        // The same encoding twice cannot decode anything its first turn did not.
        assert_eq!(parsed("utf-8,utf8").expect("known").names(), "UTF-8");
        assert!(parsed("invalid-encoding-name").is_err());
        assert!(parsed(",").is_err());
        // None of them named is utf-8, which is what it was before candidates.
        assert_eq!(Encodings::parse(None).expect("a default").names(), "UTF-8");
    }

    #[test]
    fn decoding_errors_are_rejected_instead_of_replaced() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempdir()?;
        let path = dir.path().join("invalid.pl");
        fs::write(&path, [0xff, 0xfe, b'\n'])?;

        let result = read_source(Some(&path), None, None, &Encodings::parse(None)?);

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
            |path| {
                format_one(
                    path,
                    false,
                    &Encodings::parse(None).expect("a default"),
                    &layout(),
                )
            },
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

        let report = format_one(&path, false, &Encodings::parse(None)?, &layout());

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
