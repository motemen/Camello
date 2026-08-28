//! Two producers, one publication (`docs/lsp.md`, "Diagnostics").
//!
//! `Diagnostic { code, severity, range, message }` maps straight across:
//! the stable kebab-case code becomes `Diagnostic.code`, so a `##
//! camello-disable:` comment, a `camello.toml` `disable` entry and what the
//! editor shows all spell the same thing.

use camello_sema::Severity;
use tower_lsp_server::ls_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString};

use crate::document::Document;

/// What the server calls itself in a diagnostic, so a user with three
/// linters can tell whose complaint this is.
pub const SOURCE: &str = "camello";

/// The code a parse error carries. `camello check --format json` uses the
/// same string for the same thing.
pub const PARSE_ERROR: &str = "parse-error";

/// Everything to publish for one document: the parser's, then the checker's.
///
/// Parse errors are always published — they are the one thing that is true of
/// the buffer whatever else is — and the checker's have already had the blast
/// radius applied to them by [`crate::analysis::analyse`].
#[must_use]
pub fn publish(document: &Document, checked: &[camello_sema::Diagnostic]) -> Vec<LspDiagnostic> {
    let mut out: Vec<LspDiagnostic> = document
        .parse_errors
        .iter()
        .map(|error| LspDiagnostic {
            range: document.positions.range(error.range),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(PARSE_ERROR.to_string())),
            source: Some(SOURCE.to_string()),
            message: error.message.clone(),
            ..LspDiagnostic::default()
        })
        .collect();
    out.extend(checked.iter().map(|diagnostic| LspDiagnostic {
        range: document.positions.range(diagnostic.range),
        severity: Some(severity(diagnostic.severity)),
        code: Some(NumberOrString::String(diagnostic.code.to_string())),
        source: Some(SOURCE.to_string()),
        message: diagnostic.message.clone(),
        ..LspDiagnostic::default()
    }));
    out
}

/// camello's three severities, in LSP's four.
///
/// `Hint` is not used: the checker's `info` is something a user asked to be
/// told, which is what `Information` means, and a hint is what an editor hides
/// by default.
#[must_use]
pub const fn severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    }
}
