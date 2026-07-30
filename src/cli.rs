use clap::{Parser, Subcommand};
use encoding_rs::Encoding;
use miette::{IntoDiagnostic, Report, Result};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

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
        /// Path to the Perl file to format (reads from stdin if not provided)
        #[arg(help = "Path to the Perl file (reads from stdin if not provided)")]
        path: Option<PathBuf>,

        /// Perl code to format
        #[arg(
            short,
            long = "eval",
            help = "Perl code to format",
            conflicts_with_all = ["path", "eval_escape"]
        )]
        eval: Option<String>,

        /// Perl code to format with escape sequence interpretation
        #[arg(
            short = 'E',
            long = "eval-escape",
            help = "Perl code to format with escape sequence interpretation (\\n becomes newline)",
            conflicts_with_all = ["path", "eval"]
        )]
        eval_escape: Option<String>,

        /// Check if file is already formatted without making changes
        #[arg(long, help = "Check if file is already formatted")]
        check: bool,

        /// Overwrite the input file with the formatted result
        #[arg(
            short = 'w',
            long = "write",
            help = "Overwrite the input file with the formatted result",
            requires = "path",
            conflicts_with_all = ["check", "output"]
        )]
        write: bool,

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
        help = "Maximum alignment padding, 0 to disable alignment (default: 40)"
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
            path,
            eval,
            eval_escape,
            check,
            write,
            stop_on_first_error,
            output,
            encoding,
            layout,
        } => {
            format_file(
                path,
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
                only,
                list_invariants,
                quiet,
                extensions,
                encoding,
            } => {
                return check_paths(
                    paths,
                    only.as_deref(),
                    list_invariants,
                    quiet,
                    &extensions,
                    encoding,
                );
            }
        },
    }

    Ok(())
}

/// `camello dev check`: run the invariants over files, directories, or stdin.
///
/// Exits non-zero when anything is violated, so it can gate a corpus run.
fn check_paths(
    paths: Vec<PathBuf>,
    only: Option<&str>,
    list_invariants: bool,
    quiet: bool,
    extensions: &str,
    encoding: Option<String>,
) -> Result<()> {
    use crate::check::{check, Invariant};

    if list_invariants {
        for invariant in Invariant::ALL {
            println!("{:<16} {}", invariant.slug(), invariant.name());
        }
        return Ok(());
    }

    let wanted: Option<Vec<&str>> = only.map(|list| list.split(',').map(str::trim).collect());
    if let Some(wanted) = &wanted {
        for slug in wanted {
            if !Invariant::ALL.iter().any(|kind| kind.slug() == *slug) {
                return Err(miette::miette!(
                    "unknown invariant {slug:?}; --list-invariants prints them"
                ));
            }
        }
    }

    let extensions: Vec<&str> = extensions.split(',').map(str::trim).collect();
    let encoding = get_encoding(encoding.as_ref())?;

    let mut files = Vec::new();
    for path in &paths {
        collect_perl_files(path, &extensions, &mut files)?;
    }

    // No paths at all means stdin, so the command composes with a pipeline the
    // way `format` does.
    let sources: Vec<(String, String)> = if paths.is_empty() {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes).into_diagnostic()?;
        let (decoded, _, _) = encoding.decode(&bytes);
        vec![("<stdin>".to_string(), decoded.into_owned())]
    } else {
        files
            .iter()
            .filter_map(|path| {
                let bytes = fs::read(path).ok()?;
                let (decoded, _, had_errors) = encoding.decode(&bytes);
                // Not decodable is not a violation; it is a file this command
                // has nothing to say about.
                (!had_errors).then(|| (path.display().to_string(), decoded.into_owned()))
            })
            .collect()
    };

    // Saturating: reading stdin puts one source in with no file behind it.
    let skipped = files.len().saturating_sub(sources.len());
    let mut violated = 0usize;
    let mut counts: Vec<(Invariant, usize)> =
        Invariant::ALL.iter().map(|kind| (*kind, 0)).collect();

    let stdout = io::stdout();
    let mut out = stdout.lock();
    for (label, source) in &sources {
        let violations: Vec<_> = check(source)
            .into_iter()
            .filter(|violation| {
                wanted
                    .as_ref()
                    .is_none_or(|wanted| wanted.contains(&violation.invariant.slug()))
            })
            .collect();
        if violations.is_empty() {
            continue;
        }
        violated += 1;
        for violation in violations {
            for entry in &mut counts {
                if entry.0 == violation.invariant {
                    entry.1 += 1;
                }
            }
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

    writeln!(
        out,
        "---- checked {}, clean {}, violated {violated}{}",
        sources.len(),
        sources.len() - violated,
        if skipped > 0 {
            format!(", not decodable {skipped}")
        } else {
            String::new()
        }
    )
    .into_diagnostic()?;
    for (invariant, count) in counts.iter().filter(|(_, count)| *count > 0) {
        writeln!(out, "     {:<16} {count}", invariant.slug()).into_diagnostic()?;
    }

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
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
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
            eprintln!(
                "Warning: encoding errors detected while reading '{}'",
                path.display()
            );
        }
        Ok((decoded.into_owned(), path.display().to_string()))
    } else {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes).into_diagnostic()?;
        let (decoded, _, had_errors) = encoding.decode(&bytes);
        if had_errors {
            eprintln!("Warning: encoding errors detected while reading from stdin");
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
    fs::write(path, encoded).into_diagnostic()
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
}
