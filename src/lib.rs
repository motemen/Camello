pub mod cli;
pub mod comments;
pub mod formatter;
pub mod lang;
pub mod lex;
pub mod lexer;
pub mod parse;
pub mod parser;
pub mod syntax_kind;

pub use formatter::{
    format, format_with_options, AlignmentStrategy, DelimiterTightness, DelimiterTightnessConfig,
    FormatterOptions,
};
pub use parser::{parse, parse_with_options, ParserOptions};
pub use syntax_kind::SyntaxKind;

use rowan::{Language, SyntaxNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PerlLanguage;

impl Language for PerlLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::ERROR as u16);
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

pub type PerlNode = SyntaxNode<PerlLanguage>;

#[must_use]
pub fn parse_perl(input: &str) -> (PerlNode, Vec<parser::ParseError>) {
    let (green, errors) = parse(input);
    let syntax = PerlNode::new_root(green);
    (syntax, errors)
}

#[must_use]
pub fn format_perl(input: &str) -> (String, Vec<parser::ParseError>) {
    let (syntax, errors) = parse_perl(input);
    let formatted = format(&syntax);
    (formatted, errors)
}
