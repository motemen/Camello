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
    /// Perlコードを整形する
    Format {
        /// 整形するPerlファイルのパス（指定しない場合は標準入力）
        #[arg(help = "Path to the Perl file (reads from stdin if not provided)")]
        path: Option<PathBuf>,

        /// 整形するPerlコード
        #[arg(
            short,
            long = "eval",
            help = "Perl code to format",
            conflicts_with = "path"
        )]
        eval: Option<String>,

        /// ファイルがすでに整形済みかどうかを確認し、変更は行わない
        #[arg(long, help = "Check if file is already formatted")]
        check: bool,

        /// 標準出力の代わりにファイルへ出力する
        #[arg(short, long, help = "Output file path")]
        output: Option<PathBuf>,
    },
    /// パースしたAST構造をダンプする
    Dump {
        /// パース・ダンプするPerlファイルのパス（指定しない場合は標準入力）
        #[arg(help = "Path to the Perl file (reads from stdin if not provided)")]
        path: Option<PathBuf>,

        /// パース・ダンプするPerlコード
        #[arg(
            short,
            long = "eval",
            help = "Perl code to parse and dump",
            conflicts_with = "path"
        )]
        eval: Option<String>,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Format {
            path,
            eval,
            check,
            output,
        } => {
            format_file(path, eval, check, output)?;
        }
        Commands::Dump { path, eval } => {
            dump_file(path, eval)?;
        }
    }

    Ok(())
}

fn read_source(path: Option<PathBuf>, eval: Option<String>) -> Result<(String, String)> {
    if let Some(code) = eval {
        return Ok((code, "<command-line>".to_string()));
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
    check: bool,
    output: Option<PathBuf>,
) -> Result<()> {
    // Read from file or standard input
    let (input, source_name) = read_source(path, eval)?;

    // Execute formatting
    let (formatted, errors) = format_perl(&input);

    // If there are errors, display them, but continue processing
    if !errors.is_empty() {
        eprintln!("Parse error in '{source_name}':");
        for e in errors.iter() {
            eprintln!("{:?}", Report::new(e.clone()));
        }
        eprintln!("Proceeding with best-effort formatting...\n");
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

fn dump_file(path: Option<PathBuf>, eval: Option<String>) -> Result<()> {
    // ファイルまたは標準入力から読み込む
    let (input, source_name) = read_source(path, eval)?;
    let (syntax, errors) = parse_perl(&input);

    if !errors.is_empty() {
        eprintln!("Parse errors in '{source_name}':");
        for error in errors {
            eprintln!("{:?}", Report::new(error));
        }
    }

    println!("Parsed AST for '{source_name}':");
    println!("{syntax:#?}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_format_file_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
        // Create a temporary file
        let dir = tempdir()?;
        let file_path = dir.path().join("test.pl");
        fs::write(&file_path, "my$var=1;")?;

        // Execute formatting (not actually executed, but confirm no errors)
        assert!(format_file(Some(file_path), None, false, None).is_ok());

        Ok(())
    }

    #[test]
    fn test_format_string_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
        // Execute formatting (not actually executed, but confirm no errors)
        assert!(format_file(None, Some("my$var=1;".to_string()), false, None).is_ok());

        Ok(())
    }

    #[test]
    fn test_check_mode() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("formatted.pl");
        fs::write(&file_path, "my $var = 1;\n")?;

        // Check that the file is correctly formatted
        assert!(format_file(Some(file_path), None, true, None).is_ok());

        Ok(())
    }
}
