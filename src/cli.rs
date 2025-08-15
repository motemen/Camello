use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Report, Result};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::format_perl;

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
        /// Path to Perl file to format
        #[arg(help = "Path to the Perl file")]
        path: PathBuf,

        /// Check if the file is already formatted without making changes
        #[arg(long, help = "Check if file is already formatted")]
        check: bool,

        /// Write output to a file instead of stdout
        #[arg(short, long, help = "Output file path")]
        output: Option<PathBuf>,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Format {
            path,
            check,
            output,
        } => {
            format_file(path, check, output)?;
        }
    }

    Ok(())
}

fn format_file(path: PathBuf, check: bool, output: Option<PathBuf>) -> Result<()> {
    // ファイルを読み込み
    let input = fs::read_to_string(&path).into_diagnostic()?;

    // フォーマット実行
    let (formatted, errors) = format_perl(&input);
    
    // エラーがある場合は表示するが、処理は継続
    if !errors.is_empty() {
        eprintln!("Parse error in '{}':", path.display());
        errors
            .iter()
            .for_each(|e| eprintln!("{:?}", Report::new(e.clone())));
        eprintln!("Proceeding with best-effort formatting...\n");
    }

    if check {
        // チェックモード: フォーマット済みかどうかをチェック
        if input.trim() != formatted.trim() {
            eprintln!("File '{}' is not formatted", path.display());
            std::process::exit(1);
        } else {
            println!("File '{}' is already formatted", path.display());
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
        assert!(format_file(file_path, false, None).is_ok());

        Ok(())
    }

    #[test]
    fn test_check_mode() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join("formatted.pl");
        fs::write(&file_path, "my $var = 1;\n")?;

        // 正しくフォーマットされたファイルのチェック
        assert!(format_file(file_path, true, None).is_ok());

        Ok(())
    }
}
