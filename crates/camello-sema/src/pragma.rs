//! Which `use` turns `strict` and `warnings` on (`docs/lint.md`, LINT-5).
//!
//! Two readers ask, and they are wrong in opposite directions when a module
//! is out of sight. `undeclared-variable` reports less where `strict` is off,
//! so a module it cannot see is one that turns nothing on; `missing-strict`
//! reports less where `strict` is on, so the same module is one that turns
//! everything on. [`Enables::Unknown`] is what lets each pick its own way to
//! be quiet rather than sharing one that is loud for the other.

use camello_syntax::ast::{self, AstNode};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode, TokenExt, TokenKind};

/// What one `use` turns on.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Pragmas {
    pub strict: bool,
    pub warnings: bool,
}

impl Pragmas {
    pub const NONE: Pragmas = Pragmas {
        strict: false,
        warnings: false,
    };
    pub const BOTH: Pragmas = Pragmas {
        strict: true,
        warnings: true,
    };
    const STRICT: Pragmas = Pragmas {
        strict: true,
        warnings: false,
    };
    const WARNINGS: Pragmas = Pragmas {
        strict: false,
        warnings: true,
    };

    #[must_use]
    pub fn union(self, other: Pragmas) -> Pragmas {
        Pragmas {
            strict: self.strict || other.strict,
            warnings: self.warnings || other.warnings,
        }
    }
}

/// The answer for a module, with "nobody can say" kept apart from "nothing".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enables {
    Known(Pragmas),
    /// Neither the table nor anything the run read says what its `import` does.
    Unknown,
}

/// Modules whose `import` turns the pragmas on (`docs/lint.md`, appendix A).
///
/// Not exhaustive and cannot be: the rest is read off a module's own
/// `sub import` ([`import_enables`]), which is the part a project writes.
pub const BY_IMPORT: &[(&str, Pragmas)] = &[
    ("strict", Pragmas::STRICT),
    ("warnings", Pragmas::WARNINGS),
    ("strictures", Pragmas::BOTH),
    ("Modern::Perl", Pragmas::BOTH),
    ("common::sense", Pragmas::BOTH),
    ("Moose", Pragmas::BOTH),
    ("Moose::Role", Pragmas::BOTH),
    ("Moose::Exporter", Pragmas::BOTH),
    ("Moose::Util::TypeConstraints", Pragmas::BOTH),
    ("Moo", Pragmas::BOTH),
    ("Moo::Role", Pragmas::BOTH),
    ("Mouse", Pragmas::BOTH),
    ("Mouse::Role", Pragmas::BOTH),
    ("Any::Moose", Pragmas::BOTH),
    ("Role::Tiny", Pragmas::BOTH),
    ("Mojo::Base", Pragmas::BOTH),
    ("Mojolicious::Lite", Pragmas::BOTH),
    ("Dancer", Pragmas::BOTH),
    ("Dancer2", Pragmas::BOTH),
    ("Test2::V0", Pragmas::BOTH),
    ("Test::Most", Pragmas::BOTH),
    ("Test::Class::Moose", Pragmas::BOTH),
    ("Class::Accessor::Typed", Pragmas::BOTH),
    ("Object::Pad", Pragmas::STRICT),
];

/// What the table says about `module`, if it says anything.
#[must_use]
pub fn by_table(module: &str) -> Option<Pragmas> {
    BY_IMPORT
        .iter()
        .find(|(name, _)| *name == module)
        .map(|(_, pragmas)| *pragmas)
}

/// What the table alone can say: a name it lists, or a lowercase pragma it
/// does not (`utf8`, `lib`, `constant`), which turns nothing on.
#[must_use]
pub fn without_program(module: &str) -> Enables {
    if let Some(pragmas) = by_table(module) {
        return Enables::Known(pragmas);
    }
    if is_pragma_name(module) {
        return Enables::Known(Pragmas::NONE);
    }
    Enables::Unknown
}

fn is_pragma_name(module: &str) -> bool {
    module
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == ':')
}

/// What `use v5.12` and its kind turn on: `strict` from 5.11, `warnings`
/// from 5.35.
#[must_use]
pub fn by_version(node: &SyntaxNode) -> Pragmas {
    // `use 5.012` is a number and `use v5.12` a version string; either is a
    // direct child, where an argument list's numbers are not.
    let Some(token) = node
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| matches!(token.token_kind(), TokenKind::VERSION | TokenKind::NUMBER))
    else {
        return Pragmas::NONE;
    };
    let vstring = token.text().starts_with('v');
    let text = token.text().trim_start_matches('v');
    let mut parts = text.split('.');
    let major: u32 = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    let minor_text = parts.next().unwrap_or("0");
    // `use 5.012` and `use v5.12` are the same request written two ways: the
    // decimal form spends three digits per component.
    let minor: u32 = if vstring || parts.clone().next().is_some() {
        minor_text.parse().unwrap_or(0)
    } else {
        let padded = format!("{minor_text:0<3}");
        padded[..3].parse().unwrap_or(0)
    };
    let at_least = |wanted: u32| major > 5 || (major == 5 && minor >= wanted);
    Pragmas {
        strict: at_least(11),
        warnings: at_least(35),
    }
}

/// What a module's own `sub import` turns on in its caller
/// (`docs/lint.md`, LINT-5e), or `None` for a body that calls no `import`.
///
/// A body that calls an `import` and names `strict` or `warnings` turns that
/// on — `strict->import`, `warnings->import::into($target)`, `$_->import for
/// qw(strict warnings)` are three spellings of it. One that calls the
/// `import` of a module the table lists turns on what that one does.
#[must_use]
pub fn import_enables(body: &SyntaxNode) -> Option<Pragmas> {
    let mut calls_import = false;
    let mut pragmas = Pragmas::NONE;
    for node in body.descendants() {
        if node.node_kind() == NodeKind::METHOD_CALL_EXPR {
            let Some(call) = ast::MethodCallExpr::cast(node.clone()) else {
                continue;
            };
            let Some(method) = call.method_name() else {
                continue;
            };
            if method != "import" && method != "import::into" {
                continue;
            }
            calls_import = true;
            if let Some(invocant) = call.invocant() {
                let name = invocant.text().to_string();
                if let Some(table) = by_table(name.trim()) {
                    pragmas = pragmas.union(table);
                }
            }
        }
    }
    if !calls_import {
        return None;
    }
    for token in body
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        let words: Vec<&str> = match token.token_kind() {
            TokenKind::IDENT => vec![token.text()],
            TokenKind::STRING | TokenKind::QW_STRING => token
                .text()
                .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != ':')
                .collect(),
            _ => continue,
        };
        for word in words {
            match word {
                "strict" => pragmas.strict = true,
                "warnings" => pragmas.warnings = true,
                _ => {}
            }
        }
    }
    Some(pragmas)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(source: &str) -> Pragmas {
        let root = camello_syntax::parse::parse(source).syntax();
        let node = root
            .descendants()
            .find(|node| node.node_kind() == NodeKind::USE_STMT)
            .expect("a use statement");
        by_version(&node)
    }

    #[test]
    fn versions_turn_on_strict_then_warnings() {
        assert_eq!(version("use v5.10;"), Pragmas::NONE);
        assert_eq!(version("use v5.12;"), Pragmas::STRICT);
        assert_eq!(version("use 5.012;"), Pragmas::STRICT);
        assert_eq!(version("use 5.010_001;"), Pragmas::NONE);
        assert_eq!(version("use v5.36;"), Pragmas::BOTH);
        assert_eq!(version("use 5.036;"), Pragmas::BOTH);
        assert_eq!(version("use 5.12.0;"), Pragmas::STRICT);
    }

    fn import(source: &str) -> Option<Pragmas> {
        let root = camello_syntax::parse::parse(source).syntax();
        let body = root
            .descendants()
            .find(|node| node.node_kind() == NodeKind::SUB_DEF)
            .and_then(ast::SubDef::cast)
            .and_then(|sub| sub.body())
            .expect("a sub with a body");
        import_enables(body.syntax())
    }

    #[test]
    fn an_import_that_imports_the_pragmas_turns_them_on() {
        assert_eq!(
            import("sub import { strict->import; warnings->import }"),
            Some(Pragmas::BOTH)
        );
        assert_eq!(
            import("sub import { my $t = caller; strict->import::into($t) }"),
            Some(Pragmas::STRICT)
        );
        assert_eq!(
            import("sub import { $_->import for qw(strict warnings) }"),
            Some(Pragmas::BOTH)
        );
        assert_eq!(
            import("sub import { Mojo::Base->import(-base) }"),
            Some(Pragmas::BOTH)
        );
        assert_eq!(import("sub import { 1 }"), None);
        assert_eq!(
            import("sub import { __PACKAGE__->export_to_level(1, @_); Foo->import }"),
            Some(Pragmas::NONE)
        );
    }
}
