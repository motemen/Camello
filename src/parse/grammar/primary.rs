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
        // Asked before anything else claims the keyword: `sub => $id` is the
        // string `"sub"`, not the start of an anonymous subroutine, and
        // `state => 'ready'` is a hash key rather than a declaration.
        kind if kind.is_keyword() && super::quoted_bareword(parser) => bareword_call(parser),

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
                let last = parser.current_ends_quote_like_run();
                parser.bump();
                if last {
                    break;
                }
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

        T!["try"] if parser.nth_at(1, T!["{"]) => {
            let marker = parser.start();
            parser.bump();
            super::try_tail(parser);
            parser.expect_operator();
            parser.complete(marker, NodeKind::TRY_STMT)
        }

        T!["my"] | T!["our"] | T!["state"] | T!["local"] => var_decl(parser),

        // `field $slot : param = $default;` declares a variable in every
        // respect, attributes and default included. Only where a variable
        // follows: `field(...)` is a call on a subroutine of that name.
        T!["field"] if parser.nth(1).is_some_and(TokenKind::is_sigil) => var_decl(parser),

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
            // A version is not a name. `name` coerces through `take_name`,
            // which re-reads the token with the identifier scanner — and
            // `require v5.26` came back as `v5 . 26`, a concatenation of a
            // bareword and a number.
            if parser.at(TokenKind::VERSION) {
                parser.bump();
            } else if parser.at(TokenKind::IDENT) {
                name(parser, NodeKind::SUB_NAME);
            } else {
                expr(parser);
            }
            parser.expect_operator();
            parser.complete(marker, NodeKind::REQUIRE_EXPR)
        }

        T!["return"] | T!["next"] | T!["last"] | T!["redo"] | T!["goto"] => {
            let marker = parser.start();
            // Keep the keyword token: these are control flow, not names, and a
            // lint will want to recognise them without matching on text.
            let keyword = parser.start();
            parser.bump();
            parser.complete(keyword, NodeKind::SUB_NAME);
            parser.expect_term();
            // What follows may be a label, and a label may be spelled with a
            // keyword: `last CHECK;` inside `CHECK: { ... }`. A keyword that
            // starts no term is still a name here.
            if parser.current().is_some_and(|kind| {
                (kind.can_start_term() || super::is_name_like(kind)) && !kind.is_stmt_modifier()
            }) {
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
            // `${^MATCH}` and `${name}` hold a name, not an expression.
            if parser.at(TokenKind::BITWISE_XOR) && parser.nth_at(1, TokenKind::IDENT) {
                let name = parser.start();
                // One token, so no space can be rendered inside the name and a
                // regression would show up in the token stream.
                parser.bump_caret_name();
                parser.complete(name, NodeKind::SUB_NAME);
            } else if parser
                .current()
                .is_some_and(|kind| kind.is_keyword() || kind == TokenKind::IDENT)
                && parser.nth_at(1, T!["}"])
            {
                name(parser, NodeKind::SUB_NAME);
            } else {
                // What the braces hold is a block, not an expression (perlref),
                // so it may be a series and may end with its own terminator:
                // `@{ get_ref(); }` and `@{ get_ref(), }` are both perl.
                expr(parser);
                while parser.at_any(&[T![";"], T![","]]) {
                    parser.bump();
                    parser.expect_term();
                    if parser.at(T!["}"]) || parser.at_end() {
                        break;
                    }
                    if expr(parser).is_none() {
                        break;
                    }
                }
            }
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
    let keyword = parser.current();
    parser.bump();
    parser.expect_term();

    // `local our @X;` — perl lets `local` save the value of a variable the same
    // statement declares as a package global (perlsub, "Localized variables");
    // no other pair of declarators combines.
    if keyword == Some(T!["local"]) && parser.at(T!["our"]) {
        parser.bump();
        parser.expect_term();
    }

    let target = parser.start();
    if parser.at(T!["("]) {
        parser.bump();
        parser.expect_term();
        let list = parser.start();
        super::expr::list_contents(parser, &[T![")"]]);
        parser.complete(list, NodeKind::LIST_EXPR);
        parser.expect(T![")"]);
    } else if parser.at(TokenKind::IDENT) && parser.nth_at(1, T!["->"]) {
        // `local Module->hash->{key} = $value;` — the target is a general
        // lvalue, not necessarily a variable.
        if let Some(base) = primary(parser) {
            super::expr::postfix(parser, base);
        }
    } else if parser.current().is_some_and(TokenKind::is_sigil)
        || (parser.at(TokenKind::IDENT) && parser.nth(1).is_some_and(TokenKind::is_sigil))
    {
        // `my Proc::Daemon $self = shift;` — a class name in front of the
        // variable (perlsub, "Private Variables via my()"). What it declares is
        // the variable after the name, which the rest of this arm reads.
        if parser.at(TokenKind::IDENT) {
            name(parser, NodeKind::SUB_NAME);
            parser.expect_term();
        }
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

    // `our $value :shared :Foo(1, 2);` — a declared variable takes attributes
    // exactly as a subroutine does (perlsub, "Subroutine Attributes"; the
    // variable case is `attributes`). The same routine reads both, so
    // `state $scalar :lvalue += 2` needs nothing of its own.
    super::attribute_list(parser, false);

    parser.expect_operator();
    parser.complete(marker, NodeKind::VAR_DECL)
}

/// A quote-like operator, already lexed as a complete run (ADR 0005 §3), so the
/// parser only has to consume the tokens the lexer produced.
fn quote_like(parser: &mut Parser<'_>, keyword: TokenKind) -> CompletedMarker {
    let marker = parser.start();
    parser.bump();

    // The scanner marked where the run ends, and stopping there is the point:
    // peeking one token past it asks the lexer for a token under the wrong
    // expectation, and `s/_//gr // $default` came back with a fourth delimiter
    // and half the statement inside a substitution.
    while parser.current().is_some_and(is_quote_like_part) {
        let last = parser.current_ends_quote_like_run();
        parser.bump();
        if last {
            break;
        }
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
