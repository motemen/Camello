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
/// How the diagnostics are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `path:line:col: severity: message [code]`, one per line.
    Text,
    /// One JSON array, for tooling.
    Json,
}

impl Format {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "text" => Some(Format::Text),
            "json" => Some(Format::Json),
            _ => None,
        }
    }
}

pub struct Request {
    pub paths: Vec<PathBuf>,
    pub error_on: Severity,
    /// The quietest severity worth printing. What it drops is dropped whole:
    /// it is not counted in the summary and it does not decide the exit
    /// status, because a diagnostic nobody was shown is not one to fail on.
    pub min_severity: Severity,
    pub format: Format,
    pub extensions: String,
    pub jobs: Option<usize>,
    pub encoding: Option<String>,
    /// Directories of stub modules, which shadow the real ones.
    pub stubs: Vec<PathBuf>,
    /// The include path, or `None` for "ask the perl on PATH".
    pub inc: Option<Vec<PathBuf>>,
    /// Where the declaration cache lives, or `None` for no cache.
    pub cache_dir: Option<PathBuf>,
    /// What this project's own modules stand in for.
    pub dialect: camello_sema::annotate::Dialect,
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

    // Two phases (`docs/typecheck.md`, "Data flow"): every file's declarations
    // first, so that a call in the first file can see a sub declared in the
    // last, and the bodies afterwards. A file is parsed once in each phase
    // rather than held between them — a rowan tree is not `Send`, and a corpus
    // the size of @INC would be a lot of trees to keep.
    let declared = crate::cli::in_parallel(&files, request.jobs, |(path, inline)| {
        let source = read_one(path, inline.as_deref(), &encodings)?;
        let parsed = camello_syntax::parse::parse(&source);
        Ok::<_, String>(camello_sema::decl::declare_in(
            &parsed.syntax(),
            &request.dialect,
        ))
    });

    // The roots are the directories the command was pointed at; a file named
    // on its own contributes the directory it is in, which is what makes
    // `camello typecheck lib/Foo.pm` resolve `use Foo::Bar` next door.
    let roots: Vec<PathBuf> = request
        .paths
        .iter()
        .map(|path| {
            if path.is_dir() {
                path.clone()
            } else {
                path.parent().unwrap_or(Path::new(".")).to_path_buf()
            }
        })
        .collect();
    let inc = match &request.inc {
        Some(inc) => inc.clone(),
        // Only `typecheck` needs what a dependency declares, and asking perl
        // for its `@INC` is a process to spawn.
        None if request.options.types => camello_sema::resolve::perl_inc(),
        None => Vec::new(),
    };
    let cache = match &request.cache_dir {
        Some(directory) => camello_sema::resolve::Cache::new(Some(directory.clone())),
        None => camello_sema::resolve::Cache::disabled(),
    };
    let mut analysis = camello_sema::Analysis::new()
        .with_resolver(
            camello_sema::resolve::Resolver::new(roots, request.stubs.clone(), inc),
            cache,
        )
        .with_dialect(request.dialect.clone());
    for ((path, _), decls) in files.iter().zip(declared) {
        if let Ok(decls) = decls {
            analysis.add(path, decls, true);
        }
    }
    // Only `typecheck` follows a `use` out of the roots: `lint`'s questions
    // are about the roots' own calls, and reading @INC to answer them would
    // buy nothing.
    if request.options.types {
        analysis.resolve_dependencies();
    }
    // The declaration phase is closed: a type library read anywhere in the run
    // now stands behind every annotation that named it.
    analysis.link();

    let reports = crate::cli::in_parallel(&files, request.jobs, |(path, inline)| {
        check_one(
            path,
            inline.as_deref(),
            &encodings,
            &analysis,
            &request.options,
        )
    });

    let mut counts = [0usize; 3];
    let mut unreadable = 0usize;
    let mut json = Vec::new();
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
                match request.format {
                    Format::Text => println!("{}: parse error: {message}", report.path),
                    Format::Json => json.push(Entry::parse_error(&report.path, message)),
                }
            }
            unreadable += 1;
            continue;
        }
        let index = LineIndex::new(&report.source);
        for diagnostic in &report.diagnostics {
            if diagnostic.severity < request.min_severity {
                continue;
            }
            counts[diagnostic.severity as usize] += 1;
            let position = index.position(&report.source, usize::from(diagnostic.range.start()));
            let end = index.position(&report.source, usize::from(diagnostic.range.end()));
            match request.format {
                Format::Text => println!(
                    "{}:{}:{}: {}: {} [{}]",
                    report.path,
                    position.line,
                    position.column,
                    diagnostic.severity,
                    diagnostic.message,
                    diagnostic.code
                ),
                Format::Json => json.push(Entry {
                    file: report.path.clone(),
                    line: position.line,
                    column: position.column,
                    end_line: end.line,
                    end_column: end.column,
                    severity: diagnostic.severity.to_string(),
                    code: diagnostic.code.to_string(),
                    message: diagnostic.message.clone(),
                }),
            }
        }
    }

    if request.format == Format::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json).unwrap_or_else(|_| "[]".to_string())
        );
    } else {
        summarise(&counts, files.len(), unreadable);
    }

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

fn read_one(
    path: &Path,
    inline: Option<&str>,
    encodings: &crate::cli::Encodings,
) -> std::result::Result<String, String> {
    match inline {
        Some(text) => Ok(text.to_string()),
        None => match crate::cli::read_source(Some(path), None, None, encodings) {
            Ok((source, _, _)) => Ok(source),
            Err(error) => Err(format!("{}: {error}", path.display())),
        },
    }
}

fn check_one(
    path: &Path,
    inline: Option<&str>,
    encodings: &crate::cli::Encodings,
    analysis: &camello_sema::Analysis,
    options: &Options,
) -> std::result::Result<FileReport, String> {
    let source = read_one(path, inline, encodings)?;
    let parsed = camello_syntax::parse::parse(&source);
    let parse_errors: Vec<String> = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.clone())
        .collect();
    let diagnostics = if parse_errors.is_empty() {
        analysis.check(path, &parsed.syntax(), &source, options)
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

/// One diagnostic, as `--format json` prints it.
///
/// Positions are one-based and columns count characters, which is what the
/// text form prints too; `end_line`/`end_column` are what an editor needs to
/// underline the span rather than the point.
#[derive(serde::Serialize)]
struct Entry {
    file: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    severity: String,
    code: String,
    message: String,
}

impl Entry {
    /// A file the parser had something to say about is reported as itself, so
    /// that a tool reading the JSON is not told the file was clean.
    fn parse_error(path: &str, message: &str) -> Self {
        Entry {
            file: path.to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 1,
            severity: "error".to_string(),
            code: "parse-error".to_string(),
            message: message.to_string(),
        }
    }
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
/// Used by the config file and `--disable`; parsing lives here so that both
/// spell a code the same way.
pub fn parse_codes(list: &str) -> Result<Vec<Code>> {
    list.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            Code::parse(item).ok_or_else(|| miette::miette!("unknown diagnostic code `{item}`"))
        })
        .collect()
}
