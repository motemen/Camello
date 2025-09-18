use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Report, Result};
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use crate::{format_perl, parse_perl};

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

        /// Output to file instead of stdout
        #[arg(short, long, help = "Output file path")]
        output: Option<PathBuf>,
    },
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
    },
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
            output,
        } => {
            format_file(path, eval, eval_escape, check, output)?;
        }
        Commands::Dump {
            path,
            eval,
            eval_escape,
            quiet,
            very_quiet,
        } => {
            dump_file(path, eval, eval_escape, quiet, very_quiet)?;
        }
    }

    Ok(())
}

fn read_source(
    path: Option<PathBuf>,
    eval: Option<String>,
    eval_escape: Option<String>,
) -> Result<(String, String)> {
    if let Some(code) = eval {
        return Ok((code, "<command-line>".to_string()));
    }
    if let Some(code) = eval_escape {
        let interpreted_code = interpret_escape_sequences(&code);
        return Ok((interpreted_code, "<command-line>".to_string()));
    }
    if let Some(path) = path {
        let input = fs::read_to_string(&path).into_diagnostic()?;
        Ok((input, path.display().to_string()))
    } else {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input).into_diagnostic()?;
        Ok((input, "<stdin>".to_string()))
    }
}

fn format_file(
    path: Option<PathBuf>,
    eval: Option<String>,
    eval_escape: Option<String>,
    check: bool,
    output: Option<PathBuf>,
) -> Result<()> {
    // Read from file or standard input
    let (input, source_name) = read_source(path, eval, eval_escape)?;

    // Execute formatting
    let (formatted, errors) = format_perl(&input);

    // If there are errors, display them, but continue processing
    if !errors.is_empty() {
        eprintln!("Parse error in '{source_name}':");
        for e in errors.iter() {
            eprintln!("{:?}", Report::new(e.clone()));
        }
        eprintln!("Proceeding with best-effort formatting...\\n");
    }

    if check {
        // Check mode: check if already formatted
        if input.trim() == formatted.trim() {
            println!("Source '{source_name}' is already formatted");
        } else {
            eprintln!("Source '{source_name}' is not formatted");
            std::process::exit(1);
        }
    } else {
        // Format mode: output the result
        if let Some(output_path) = output {
            // Write to file
            fs::write(&output_path, formatted).into_diagnostic()?;
            println!("Formatted code written to '{}'", output_path.display());
        } else {
            // Write to standard output
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
) -> Result<()> {
    // Read from file or standard input
    let (input, source_name) = read_source(path, eval, eval_escape)?;
    let (syntax, errors) = parse_perl(&input);

    if !errors.is_empty() {
        if !very_quiet {
            eprintln!("Parse errors in '{source_name}':");
            for error in errors {
                eprintln!("{:?}", Report::new(error));
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
        None
    )
    .is_ok());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

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
            None
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
        assert!(format_file(Some(file_path), None, None, false, None).is_ok());

        Ok(())
    }

    #[test]
    fn test_format_string_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
        // Execute formatting (not actually executed, but confirm no errors)
        assert!(format_file(None, Some("my$var=1;".to_string()), None, false, None).is_ok());

        Ok(())
    }

    #[test]
    fn test_check_mode() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("formatted.pl");
        fs::write(&file_path, "my $var = 1;\n")?; // Use actual newline, not escaped

        // Check that the file is correctly formatted
        assert!(format_file(Some(file_path), None, None, true, None).is_ok());

        Ok(())
    }
}
