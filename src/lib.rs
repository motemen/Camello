pub mod syntax_kind;
pub mod lexer;
pub mod parser;
pub mod formatter;
pub mod cli;

pub use syntax_kind::SyntaxKind;
pub use parser::parse;
pub use formatter::format;

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

pub fn parse_perl(input: &str) -> (PerlNode, Vec<String>) {
    let (green, errors) = parse(input);
    let syntax = PerlNode::new_root(green);
    let error_messages = errors.into_iter().map(|e| e.to_string()).collect();
    (syntax, error_messages)
}

pub fn format_perl(input: &str) -> Result<String, String> {
    let (syntax, errors) = parse_perl(input);
    if !errors.is_empty() {
        return Err(format!("Parse errors: {}", errors.join(", ")));
    }
    Ok(format(&syntax))
}