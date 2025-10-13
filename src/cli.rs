use clap::{Parser, Subcommand};
use encoding_rs::Encoding;
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

        /// Stop formatting after the first parse error is reported
        #[arg(
            long = "stop-on-error",
            help = "Stop after reporting the first parse error"
        )]
        stop_on_error: bool,

        /// Output to file instead of stdout
        #[arg(short, long, help = "Output file path")]
        output: Option<PathBuf>,

        /// Input file encoding (e.g., utf-8, euc-jp, shift_jis)
        #[arg(long, help = "Input file encoding (default: utf-8)")]
        encoding: Option<String>,
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

        /// Input file encoding (e.g., utf-8, euc-jp, shift_jis)
        #[arg(long, help = "Input file encoding (default: utf-8)")]
        encoding: Option<String>,
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
            stop_on_error,
            output,
            encoding,
        } => {
            format_file(
                path,
                eval,
                eval_escape,
                check,
                stop_on_error,
                output,
                encoding,
            )?;
        }
        Commands::Dump {
            path,
            eval,
            eval_escape,
            quiet,
            very_quiet,
            encoding,
        } => {
            dump_file(path, eval, eval_escape, quiet, very_quiet, encoding)?;
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
    path: Option<PathBuf>,
    eval: Option<String>,
    eval_escape: Option<String>,
    encoding: Option<&String>,
) -> Result<(String, String)> {
    if let Some(code) = eval {
        return Ok((code, "<command-line>".to_string()));
    }
    if let Some(code) = eval_escape {
        let interpreted_code = interpret_escape_sequences(&code);
        return Ok((interpreted_code, "<command-line>".to_string()));
    }

    let enc = get_encoding(encoding)?;

    if let Some(path) = path {
        let bytes = fs::read(&path).into_diagnostic()?;
        let (decoded, _, had_errors) = enc.decode(&bytes);
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
        let (decoded, _, had_errors) = enc.decode(&bytes);
        if had_errors {
            eprintln!("Warning: encoding errors detected while reading from stdin");
        }
        Ok((decoded.into_owned(), "<stdin>".to_string()))
    }
}

fn format_file(
    path: Option<PathBuf>,
    eval: Option<String>,
    eval_escape: Option<String>,
    check: bool,
    stop_on_error: bool,
    output: Option<PathBuf>,
    encoding: Option<String>,
) -> Result<()> {
    // Read from file or standard input
    let (input, source_name) = read_source(path, eval, eval_escape, encoding.as_ref())?;

    // Execute formatting
    let (formatted, errors) = format_perl(&input);

    // If there are errors, display them, and optionally stop immediately
    if !errors.is_empty() {
        eprintln!("Parse error in '{source_name}':");
        if stop_on_error {
            if let Some(error) = errors.first() {
                eprintln!("{:?}", Report::new(error.clone()));
            }
            std::process::exit(2);
        } else {
            for e in errors.iter() {
                eprintln!("{:?}", Report::new(e.clone()));
            }
            eprintln!("Proceeding with best-effort formatting...\\n");
        }
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
    encoding: Option<String>,
) -> Result<()> {
    // Read from file or standard input
    let (input, source_name) = read_source(path, eval, eval_escape, encoding.as_ref())?;
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
        false,
        None,
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
            false,
            None,
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
        assert!(format_file(Some(file_path), None, None, false, false, None, None).is_ok());

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
            None
        )
        .is_ok());

        Ok(())
    }

    #[test]
    fn test_check_mode() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("formatted.pl");
        fs::write(&file_path, "my $var = 1;\n")?; // Use actual newline, not escaped

        // Check that the file is correctly formatted
        assert!(format_file(Some(file_path), None, None, true, false, None, None).is_ok());

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
            None,
            Some("utf-8".to_string())
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
            None,
            Some("euc-jp".to_string())
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
            None,
            Some("shift_jis".to_string())
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
