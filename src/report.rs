//! `camello lint` and `camello typecheck`: running the checker over paths and
//! saying what it found (`docs/typecheck.md`).
//!
//! One diagnostic per line, in `path:line:col: severity: message` form, sorted
//! by file and then by position, and an exit status of 1 when anything at or
//! above `--error-on` was reported. That shape is chosen for the first
//! consumer, which is CI: it is what `grep` and an editor's error parser both
//! already understand.

use std::path::{Path, PathBuf};

use camello_sema::{Code, Diagnostic, LineIndex, Options, Severity};
use miette::Result;

/// What one run of `lint` or `typecheck` was asked for.
pub struct Request {
    pub paths: Vec<PathBuf>,
    pub error_on: Severity,
    pub extensions: String,
    pub jobs: Option<usize>,
    pub encoding: Option<String>,
    pub options: Options,
}

/// What one file answered.
struct FileReport {
    path: String,
    source: String,
    diagnostics: Vec<Diagnostic>,
    /// A file the parser had something to say about is left alone, the way
    /// `format` leaves it alone: a tree camello could not build is not one to
    /// report scope errors from.
    parse_errors: Vec<String>,
}

/// Run the checker and print what it found.
///
/// Exits the process rather than returning a status, because the status is the
/// interface here: a CI step reads it and nothing else.
pub fn run(request: &Request) -> Result<()> {
    let encodings = crate::cli::Encodings::parse(request.encoding.as_ref())?;
    let extensions: Vec<&str> = request
        .extensions
        .split(',')
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
        .collect();

    let mut files = Vec::new();
    if request.paths.is_empty() {
        // Standard input has no path, so it reports as `<stdin>`.
        let (source, name, _) = crate::cli::read_source(None, None, None, &encodings)?;
        files.push((PathBuf::from(name), Some(source)));
    } else {
        let mut collected = Vec::new();
        for path in &request.paths {
            crate::cli::collect_perl_files(path, &extensions, &mut collected)?;
        }
        files.extend(collected.into_iter().map(|path| (path, None)));
    }

    let reports = crate::cli::in_parallel(&files, request.jobs, |(path, inline)| {
        check_one(path, inline.as_deref(), &encodings, &request.options)
    });

    let mut counts = [0usize; 3];
    let mut unreadable = 0usize;
    for report in &reports {
        let report = match report {
            Ok(report) => report,
            Err(message) => {
                eprintln!("{message}");
                unreadable += 1;
                continue;
            }
        };
        if !report.parse_errors.is_empty() {
            for message in &report.parse_errors {
                println!("{}: parse error: {message}", report.path);
            }
            unreadable += 1;
            continue;
        }
        let index = LineIndex::new(&report.source);
        for diagnostic in &report.diagnostics {
            counts[diagnostic.severity as usize] += 1;
            let position = index.position(&report.source, usize::from(diagnostic.range.start()));
            println!(
                "{}:{}:{}: {}: {} [{}]",
                report.path,
                position.line,
                position.column,
                diagnostic.severity,
                diagnostic.message,
                diagnostic.code
            );
        }
    }

    summarise(&counts, files.len(), unreadable);

    let reportable: usize = counts
        .iter()
        .enumerate()
        .filter(|(level, _)| *level >= request.error_on as usize)
        .map(|(_, count)| count)
        .sum();
    if reportable > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn check_one(
    path: &Path,
    inline: Option<&str>,
    encodings: &crate::cli::Encodings,
    options: &Options,
) -> std::result::Result<FileReport, String> {
    let source = match inline {
        Some(text) => text.to_string(),
        None => match crate::cli::read_source(Some(path), None, None, encodings) {
            Ok((source, _, _)) => source,
            Err(error) => return Err(format!("{}: {error}", path.display())),
        },
    };
    let parsed = camello_syntax::parse::parse(&source);
    let parse_errors: Vec<String> = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.clone())
        .collect();
    let diagnostics = if parse_errors.is_empty() {
        camello_sema::check(&parsed.syntax(), &source, options)
    } else {
        Vec::new()
    };
    Ok(FileReport {
        path: path.display().to_string(),
        source,
        diagnostics,
        parse_errors,
    })
}

/// One line at the end, the way `format` ends a tree with one.
///
/// Quiet when there is nothing to say: a clean run over a clean tree prints
/// the count and stops, and a run over one file prints nothing at all.
fn summarise(counts: &[usize; 3], files: usize, unreadable: usize) {
    let total: usize = counts.iter().sum();
    if total == 0 && unreadable == 0 && files <= 1 {
        return;
    }
    let mut parts = Vec::new();
    for severity in [Severity::Error, Severity::Warning, Severity::Info] {
        let count = counts[severity as usize];
        if count > 0 {
            parts.push(format!(
                "{count} {}{}",
                severity,
                if count == 1 { "" } else { "s" }
            ));
        }
    }
    if unreadable > 0 {
        parts.push(format!("{unreadable} not checked"));
    }
    if parts.is_empty() {
        parts.push("nothing to report".to_string());
    }
    eprintln!(
        "{} in {files} file{}",
        parts.join(", "),
        if files == 1 { "" } else { "s" }
    );
}

/// The codes a run may be told to ignore, parsed from a comma-separated list.
///
/// Used by the config file and `--disable` (milestone 6); parsing lives here
/// so that both spell a code the same way.
#[allow(dead_code)]
pub fn parse_codes(list: &str) -> Result<Vec<Code>> {
    list.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            Code::parse(item).ok_or_else(|| miette::miette!("unknown diagnostic code `{item}`"))
        })
        .collect()
}
