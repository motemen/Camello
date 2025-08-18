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
        /// Path to Perl file to format (stdin if not provided)
        #[arg(help = "Path to the Perl file (reads from stdin if not provided)")]
        path: Option<PathBuf>,

        /// Perl code to format
        #[arg(short, long = "eval", help = "Perl code to format", conflicts_with = "path")]
        eval: Option<String>,

        /// Check if the file is already formatted without making changes
        #[arg(long, help = "Check if file is already formatted")]
        check: bool,

        /// Write output to a file instead of stdout
        #[arg(short, long, help = "Output file path")]
        output: Option<PathBuf>,
    },
    /// Dump parsed AST structure
    Dump {
        /// Path to Perl file to parse and dump (stdin if not provided)
        #[arg(help = "Path to the Perl file (reads from stdin if not provided)")]
        path: Option<PathBuf>,

        /// Perl code to parse and dump
        #[arg(short, long = "eval", help = "Perl code to parse and dump", conflicts_with = "path")]
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
    match path {
        Some(path) => {
            let input = fs::read_to_string(&path).into_diagnostic()?;
            Ok((input, path.display().to_string()))
        }
        None => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input).into_diagnostic()?;
            Ok((input, "<stdin>".to_string()))
        }
    }
}

fn format_file(
    path: Option<PathBuf>,
    eval: Option<String>,
    check: bool,
    output: Option<PathBuf>,
) -> Result<()> {
    // ファイルまたは標準入力を読み込み
    let (input, source_name) = read_source(path, eval)?;

    // フォーマット実行
    let (formatted, errors) = format_perl(&input);

    // エラーがある場合は表示するが、処理は継続
    if !errors.is_empty() {
        eprintln!("Parse error in '{}':", source_name);
        errors
            .iter()
            .for_each(|e| eprintln!("{:?}", Report::new(e.clone())));
        eprintln!("Proceeding with best-effort formatting...\n");
    }

    if check {
        // チェックモード: フォーマット済みかどうかをチェック
        if input.trim() != formatted.trim() {
            eprintln!("Source '{}' is not formatted", source_name);
            std::process::exit(1);
        } else {
            println!("Source '{}' is already formatted", source_name);
        }
    } else {
        // フォーマットモード: 結果を出力
        match output {
            Some(output_path) => {
                // ファイルに書き出し
                fs::write(&output_path, formatted).into_diagnostic()?;
                println!("Formatted code written to '{}'", output_path.display());
            }
            None => {
                // 標準出力に書き出し
                print!("{}", formatted);
                io::stdout().flush().into_diagnostic()?;
            }
        }
    }

    Ok(())
}

fn dump_file(path: Option<PathBuf>, eval: Option<String>) -> Result<()> {
    // ファイルまたは標準入力を読み込み
    let (input, source_name) = read_source(path, eval)?;
    let (syntax, errors) = parse_perl(&input);

    if !errors.is_empty() {
        eprintln!("Parse errors in '{}':", source_name);
        for error in errors {
            eprintln!("{:?}", Report::new(error));
        }
    }

    println!("Parsed AST for '{}':", source_name);
    println!("{:#?}", syntax);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_format_file_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
        // 一時ファイルを作成
        let dir = tempdir()?;
        let file_path = dir.path().join("test.pl");
        fs::write(&file_path, "my$var=1;")?;

        // フォーマット実行（実際の実行はしないが、エラーが出ないことを確認）
        assert!(format_file(Some(file_path), None, false, None).is_ok());

        Ok(())
    }

    #[test]
    fn test_format_string_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
        // フォーマット実行（実際の実行はしないが、エラーが出ないことを確認）
        assert!(format_file(None, Some("my$var=1;".to_string()), false, None).is_ok());

        Ok(())
    }

    #[test]
    fn test_check_mode() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("formatted.pl");
        fs::write(&file_path, "my $var = 1;\n")?;

        // 正しくフォーマットされたファイルのチェック
        assert!(format_file(Some(file_path), None, true, None).is_ok());

        Ok(())
    }
}
