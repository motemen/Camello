//! Terms (ADR 0007 §2).

use crate::lang::{NodeKind, TokenKind, T};
use crate::parse::event::CompletedMarker;
use crate::parse::{Parser, Recovery};

use super::expr::{arg_list, bareword_call, expr, list_expr, postfix_subscript};
use super::{block, name};

pub(crate) fn primary(parser: &mut Parser<'_>) -> Option<CompletedMarker> {
    parser.expect_term();
    let kind = parser.current()?;

    let completed = match kind {
        kind if kind.is_sigil() => variable(parser),

        TokenKind::NUMBER | TokenKind::VERSION | TokenKind::STRING => {
            let marker = parser.start();
            parser.bump();
            parser.expect_operator();
            parser.complete(marker, NodeKind::LITERAL)
        }

        TokenKind::IO_HANDLE => {
            let marker = parser.start();
            parser.bump();
            parser.expect_operator();
            parser.complete(marker, NodeKind::IO_EXPR)
        }

        TokenKind::HEREDOC_START => {
            let marker = parser.start();
            parser.bump();
            parser.expect_operator();
            parser.complete(marker, NodeKind::HEREDOC_EXPR)
        }

        kind if kind.is_quote_like_keyword() => quote_like(parser, kind),

        // A bare `/pattern/flags`. The lexer emitted the whole run
        // (ADR 0005 §3), so there is nothing to decide here.
        TokenKind::DELIMITER => {
            let marker = parser.start();
            while parser.current().is_some_and(is_quote_like_part) {
                parser.bump();
            }
            parser.expect_operator();
            parser.complete(marker, NodeKind::M_EXPR)
        }

        T!["("] => {
            let marker = parser.start();
            parser.bump();
            parser.expect_term();
            let list = parser.start();
            super::expr::list_contents(parser, &[T![")"]]);
            parser.complete(list, NodeKind::LIST_EXPR);
            if !parser.expect(T![")"]) {
                parser.recover(Recovery::List);
                if parser.at(T![")"]) {
                    parser.bump();
                }
            }
            parser.expect_operator();
            parser.complete(marker, NodeKind::PAREN_EXPR)
        }

        T!["["] => {
            let marker = parser.start();
            parser.bump();
            parser.expect_term();
            let list = parser.start();
            super::expr::list_contents(parser, &[T!["]"]]);
            parser.complete(list, NodeKind::LIST_EXPR);
            parser.expect(T!["]"]);
            parser.expect_operator();
            parser.complete(marker, NodeKind::ANON_ARRAY)
        }

        T!["{"] => anon_hash_or_block(parser),

        T!["sub"] => {
            let marker = parser.start();
            parser.bump();
            super::subroutine_tail(parser, false);
            parser.expect_operator();
            parser.complete(marker, NodeKind::ANON_SUB_EXPR)
        }

        T!["do"] => {
            let marker = parser.start();
            parser.bump();
            parser.expect_term();
            if parser.at(T!["{"]) {
                block(parser);
            } else {
                expr(parser);
            }
            parser.expect_operator();
            parser.complete(marker, NodeKind::DO_BLOCK_EXPR)
        }

        T!["my"] | T!["our"] | T!["state"] | T!["local"] => var_decl(parser),

        T!["undef"] => {
            let marker = parser.start();
            parser.bump();
            parser.expect_term();
            if parser.at(T!["("]) {
                arg_list(parser);
            } else if parser.current().is_some_and(TokenKind::can_start_term) {
                super::expr::expr(parser);
            }
            parser.expect_operator();
            parser.complete(marker, NodeKind::UNDEF_EXPR)
        }

        T!["require"] => {
            let marker = parser.start();
            parser.bump();
            parser.expect_term();
            if parser.at(TokenKind::IDENT) || parser.at(TokenKind::VERSION) {
                name(parser, NodeKind::SUB_NAME);
            } else {
                expr(parser);
            }
            parser.expect_operator();
            parser.complete(marker, NodeKind::REQUIRE_EXPR)
        }

        T!["return"] | T!["next"] | T!["last"] | T!["redo"] | T!["goto"] => {
            let marker = parser.start();
            name(parser, NodeKind::SUB_NAME);
            parser.expect_term();
            if parser
                .current()
                .is_some_and(|kind| kind.can_start_term() && !kind.is_stmt_modifier())
            {
                list_expr(parser, Recovery::Statement);
            }
            parser.expect_operator();
            parser.complete(marker, NodeKind::LIST_CALL_EXPR)
        }

        T!["..."] => {
            let marker = parser.start();
            parser.bump();
            parser.expect_operator();
            parser.complete(marker, NodeKind::RANGE_EXPR)
        }

        TokenKind::IDENT => bareword_call(parser),

        // A keyword in term position that no rule claimed is being used as a
        // name: `Foo::when()`, `$h{default}`. One routine coerces it
        // (ADR 0007 §5).
        kind if kind.is_keyword() => bareword_call(parser),

        kind if kind.is_error() => {
            let marker = parser.start();
            let range = parser.current_range();
            parser.bump();
            parser.error_at(format!("{kind}"), range);
            parser.expect_operator();
            parser.complete(marker, NodeKind::ERROR)
        }

        _ => return None,
    };

    Some(completed)
}

/// A variable: sigil plus name, a braced dereference, or a chain of `$`s.
///
/// One implementation, where the old parser had four entry points
/// (ADR 0007 §5).
pub(crate) fn variable(parser: &mut Parser<'_>) -> CompletedMarker {
    let sigil = parser.current().expect("caller checked for a sigil");
    let marker = parser.start();
    parser.bump();

    match parser.current() {
        // `${...}` / `@{...}`: the braces hold an expression, not a block.
        Some(T!["{"]) => {
            parser.bump();
            parser.expect_term();
            expr(parser);
            parser.expect(T!["}"]);
            parser.expect_operator();
            return parser.complete(marker, NodeKind::BLOCK_DEREF_EXPR);
        }
        // `$$ref`, `@$ref`, and any depth of them.
        Some(kind) if kind.is_sigil() => {
            variable(parser);
            parser.expect_operator();
            return parser.complete(marker, NodeKind::DEREF_EXPR);
        }
        Some(TokenKind::IDENT | TokenKind::NUMBER | TokenKind::RAW_CONTENT) => parser.bump(),
        _ => {}
    }

    parser.expect_operator();
    let node = match sigil {
        TokenKind::SCALAR_SIGIL => NodeKind::SCALAR_VAR,
        TokenKind::ARRAY_SIGIL => NodeKind::ARRAY_VAR,
        TokenKind::HASH_SIGIL => NodeKind::HASH_VAR,
        TokenKind::CODE_SIGIL => NodeKind::CODE_VAR,
        TokenKind::TYPEGLOB_SIGIL => NodeKind::TYPEGLOB_VAR,
        TokenKind::ARRAY_INDEX_SIGIL => NodeKind::ARRAY_LAST_INDEX,
        _ => NodeKind::SCALAR_VAR,
    };
    parser.complete(marker, node)
}

/// `my $x`, `our ($a, $b)`, `local $h{key}`.
pub(crate) fn var_decl(parser: &mut Parser<'_>) -> CompletedMarker {
    let marker = parser.start();
    parser.bump();
    parser.expect_term();

    let target = parser.start();
    if parser.at(T!["("]) {
        parser.bump();
        parser.expect_term();
        let list = parser.start();
        super::expr::list_contents(parser, &[T![")"]]);
        parser.complete(list, NodeKind::LIST_EXPR);
        parser.expect(T![")"]);
    } else if parser.current().is_some_and(TokenKind::is_sigil) {
        // `local $h{key}` subscripts the declared variable, but `for my $x (@xs)`
        // must not read the list as a call on `$x`.
        let mut target = variable(parser);
        while parser.at_any(&[T!["["], T!["{"]]) {
            target = postfix_subscript(parser, target);
        }
    } else {
        parser.error("expected a variable to declare");
    }
    parser.complete(target, NodeKind::DECL_TARGET);

    parser.expect_operator();
    parser.complete(marker, NodeKind::VAR_DECL)
}

/// A quote-like operator, already lexed as a complete run (ADR 0005 §3), so the
/// parser only has to consume the tokens the lexer produced.
fn quote_like(parser: &mut Parser<'_>, keyword: TokenKind) -> CompletedMarker {
    let marker = parser.start();
    parser.bump();

    while parser.current().is_some_and(is_quote_like_part) {
        parser.bump();
    }

    parser.expect_operator();
    let node = match keyword {
        T!["q"] => NodeKind::Q_EXPR,
        T!["qq"] => NodeKind::QQ_EXPR,
        T!["qx"] => NodeKind::QX_EXPR,
        T!["qw"] => NodeKind::QW_EXPR,
        T!["m"] => NodeKind::M_EXPR,
        T!["qr"] => NodeKind::QR_EXPR,
        T!["s"] => NodeKind::S_EXPR,
        _ => NodeKind::TR_EXPR,
    };
    parser.complete(marker, node)
}

fn is_quote_like_part(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::DELIMITER
            | TokenKind::LITERAL_STRING
            | TokenKind::INTERPOLATED_STRING
            | TokenKind::REGEX_PATTERN
            | TokenKind::TR_SEARCH_LIST
            | TokenKind::TR_REPLACEMENT_LIST
            | TokenKind::QW_STRING
            | TokenKind::REGEX_FLAGS
    )
}

/// `{` in term position is either an anonymous hash or a block.
///
/// Perl guesses at this too (perlref says so). The difference here is *how*:
/// parse it as a hash, and if that does not work out, roll back and parse a
/// block (ADR 0007 §1). The old parser scanned the whole brace body looking for
/// a `;` before it dared open a node, and still got `{ $k => 1 }` wrong.
fn anon_hash_or_block(parser: &mut Parser<'_>) -> CompletedMarker {
    let checkpoint = parser.checkpoint();
    let errors_before = parser.diagnostic_count();

    let marker = parser.start();
    parser.bump();
    parser.expect_term();
    let list = parser.start();
    super::expr::list_contents(parser, &[T!["}"]]);
    parser.complete(list, NodeKind::LIST_EXPR);

    let closed = parser.at(T!["}"]);
    if closed && parser.diagnostic_count() == errors_before {
        parser.bump();
        parser.expect_operator();
        return parser.complete(marker, NodeKind::ANON_HASH);
    }

    parser.abandon(marker);
    parser.rollback(checkpoint);

    let marker = parser.start();
    block(parser);
    parser.expect_operator();
    parser.complete(marker, NodeKind::DO_BLOCK_EXPR)
}
