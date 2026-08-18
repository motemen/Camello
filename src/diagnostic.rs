//! Diagnostics as the CLI reports them.
//!
//! The parser produces plain messages and ranges (`parse::event::Diagnostic`);
//! this attaches the source text so `miette` can draw the snippet. Messages are
//! written for people — the language definition's `Display` is what keeps enum
//! names out of them (the language model, the parser contract).

use std::sync::Arc;

use miette::{Diagnostic, SourceSpan};
use rowan::TextRange;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
pub struct ParseError {
    pub message: String,
    pub range: TextRange,
    #[source_code]
    pub source_code: Arc<str>,
    #[label("here")]
    pub span: SourceSpan,
}

impl ParseError {
    #[must_use]
    pub fn new(message: String, range: TextRange, source_code: Arc<str>) -> Self {
        let span = SourceSpan::new(
            usize::from(range.start()).into(),
            usize::from(range.len()).max(1),
        );
        Self {
            message,
            range,
            source_code,
            span,
        }
    }

    pub(crate) fn from_parse(
        diagnostic: crate::parse::event::Diagnostic,
        source_code: &Arc<str>,
    ) -> Self {
        Self::new(
            diagnostic.message,
            diagnostic.range,
            Arc::clone(source_code),
        )
    }
}
