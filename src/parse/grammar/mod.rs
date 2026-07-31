//! The grammar (ADR 0007 §2, §3, §5).

pub(crate) mod builtins;
mod expr;
pub(crate) mod precedence;
mod primary;

use crate::lang::{NodeKind, TokenKind, T};
use crate::parse::{Parser, Recovery};

pub(super) fn root(parser: &mut Parser<'_>) {
    while !parser.at_end() {
        statement(parser);
    }
}

/// One statement. Every arm produces a closed statement node — there is no
/// generic `STMT` wrapper, so nothing is "sometimes wrapped" (ADR 0007 §2).
fn statement(parser: &mut Parser<'_>) {
    parser.expect_term();
    let Some(kind) = parser.current() else { return };

    // `default => 1;` is an expression whose first element happens to be a
    // keyword, not the `default` block of a `given`.
    if kind.is_keyword() && quoted_bareword(parser) {
        expr_stmt(parser);
        return;
    }

    match kind {
        T![";"] => {
            let marker = parser.start();
            parser.bump();
            parser.complete(marker, NodeKind::EMPTY_STMT);
        }
        TokenKind::POD_CONTENT => {
            let marker = parser.start();
            parser.bump();
            parser.complete(marker, NodeKind::POD);
        }
        // The whole declaration arrived as one run from the lexer; there is
        // nothing to parse inside it, only somewhere to put it.
        T!["format"] => {
            let marker = parser.start();
            parser.bump();
            if parser.at(TokenKind::RAW_CONTENT) {
                parser.bump();
            }
            if parser.at(TokenKind::FORMAT_CONTENT) {
                parser.bump();
            }
            parser.complete(marker, NodeKind::FORMAT_DECL);
        }
        T!["__END__"] | T!["__DATA__"] => {
            let marker = parser.start();
            parser.bump();
            if parser.at(TokenKind::DATA_CONTENT) {
                parser.bump();
            }
            parser.complete(marker, NodeKind::DATA_SECTION);
        }
        // `sub` at statement level names a subroutine unless a body follows
        // straight away. Deciding it here, rather than by peeking at a name the
        // lexer may have read as a quote-like operator, is what lets
        // `sub tr {}` and `sub s {}` work.
        T!["sub"] if !parser.nth_at(1, T!["{"]) => sub_def(parser),
        // `my sub NAME { ... }` — a lexically scoped named subroutine
        // (perlsub). What follows the declaration keyword is a subroutine, not
        // the variable the declaration rule would look for.
        T!["my"] | T!["our"] | T!["state"]
            if parser.nth_at(1, T!["sub"]) && !parser.nth_at(2, T!["{"]) =>
        {
            sub_def(parser);
        }
        T!["package"] => package_stmt(parser),
        T!["use"] => use_stmt(parser, NodeKind::USE_STMT),
        T!["no"] => use_stmt(parser, NodeKind::NO_STMT),
        T!["if"] | T!["unless"] => if_stmt(parser),
        T!["while"] | T!["until"] | T!["for"] | T!["foreach"] => loop_stmt(parser),
        T!["try"] if parser.nth_at(1, T!["{"]) => try_stmt(parser),
        T!["given"] => given_stmt(parser),
        T!["when"] if parser.nth_at(1, T!["("]) => when_clause(parser, NodeKind::WHEN_CLAUSE),
        T!["default"] if parser.nth_at(1, T!["{"]) => when_clause(parser, NodeKind::DEFAULT_CLAUSE),
        T!["BEGIN"] | T!["END"] | T!["INIT"] | T!["CHECK"] | T!["UNITCHECK"]
            if parser.nth_at(1, T!["{"]) =>
        {
            phase_block(parser);
        }
        T!["{"] => {
            let marker = parser.start();
            block(parser);
            parser.complete(marker, NodeKind::BLOCK_STMT);
        }
        TokenKind::IDENT if parser.nth_at(1, T![":"]) && !parser.nth_at(2, T![":"]) => {
            labeled_stmt(parser);
        }
        _ => expr_stmt(parser),
    }
}

/// An expression statement, plus any postfix modifier and the terminating `;`.
fn expr_stmt(parser: &mut Parser<'_>) {
    let marker = parser.start();
    let declaration = parser.at_any(&[T!["my"], T!["our"], T!["state"], T!["local"]]);

    let before = parser.checkpoint();
    let list = parser.start();
    expr::list_contents(parser, &[]);
    parser.complete(list, NodeKind::LIST_EXPR);

    if parser.checkpoint_is_unmoved(before) {
        // Nothing was consumed; without this the outer loop would spin.
        parser.abandon(marker);
        parser.error_and_bump("expected a statement");
        return;
    }

    stmt_modifier(parser);
    semicolon(parser);

    let node = if declaration {
        NodeKind::VAR_DECL_STMT
    } else {
        NodeKind::EXPR_STMT
    };
    parser.complete(marker, node);
}

/// `... if $x`, `... for @xs`.
fn stmt_modifier(parser: &mut Parser<'_>) {
    if !parser.current().is_some_and(TokenKind::is_stmt_modifier) || quoted_bareword(parser) {
        return;
    }
    let marker = parser.start();
    parser.bump();
    parser.expect_term();
    let list = parser.start();
    expr::list_contents(parser, &[]);
    parser.complete(list, NodeKind::LIST_EXPR);
    parser.complete(marker, NodeKind::STMT_MODIFIER);
}

/// Consume the statement terminator, recovering to one if it is missing.
///
/// The old parser reported the missing `;` and then swallowed the next
/// statement's first token as an `ERROR`, which is how `use A use X;` lost its
/// second `use` (ADR 0007 §3).
fn semicolon(parser: &mut Parser<'_>) {
    if parser.at(T![";"]) {
        parser.bump();
        return;
    }
    if parser.at_end() || parser.at(T!["}"]) {
        // A final statement may omit its semicolon.
        return;
    }
    parser.error("expected `;` at the end of the statement");
    parser.recover(Recovery::Statement);
    if parser.at(T![";"]) {
        parser.bump();
    }
}

/// A block, from `{` to `}`.
///
/// One implementation, where the old parser had three (ADR 0007 §5).
pub(crate) fn block(parser: &mut Parser<'_>) {
    let marker = parser.start();
    parser.expect_term();
    if !parser.expect(T!["{"]) {
        parser.complete(marker, NodeKind::BLOCK);
        return;
    }

    while !parser.at_end() && !parser.at(T!["}"]) {
        let before = parser.checkpoint();
        statement(parser);
        if parser.checkpoint_is_unmoved(before) {
            parser.error_and_bump("expected a statement");
        }
    }

    parser.expect(T!["}"]);
    parser.expect_operator();
    parser.complete(marker, NodeKind::BLOCK);
}

/// A name, coercing a keyword into an identifier where the grammar wants one.
///
/// `sub if {}`, `package tr;` and `$h{default}` are all legal Perl. The old
/// parser forced the coercion in eight separate places; here there is one
/// (ADR 0007 §5).
pub(crate) fn name(parser: &mut Parser<'_>, node: NodeKind) {
    let marker = parser.start();
    if !parser.bump_name() {
        if parser.current().is_some_and(is_name_like) {
            parser.bump_any();
        } else {
            parser.error("expected a name");
        }
    }
    parser.complete(marker, node);
}

fn is_name_like(kind: TokenKind) -> bool {
    kind == TokenKind::IDENT || kind.is_keyword()
}

/// Is the token here a bareword that the `=>` after it quotes?
///
/// `=>` quotes the bareword to its left (perlop, "Comma Operator"), so
/// `state => 'paid'` and `sub => $id` hold strings whatever the word means
/// elsewhere. Every rule that would otherwise claim the keyword — the statement
/// dispatch, the declaration and anonymous-subroutine terms, the postfix
/// modifiers — asks this first, so the quoting rule is written down once rather
/// than once per keyword (ADR 0007 §5).
pub(crate) fn quoted_bareword(parser: &mut Parser<'_>) -> bool {
    parser.current().is_some_and(is_name_like) && parser.nth_at(1, T!["=>"])
}

// ===== Declarations =====

fn sub_def(parser: &mut Parser<'_>) {
    let marker = parser.start();
    // The `my` of `my sub helper { ... }`; the rest is an ordinary definition.
    if !parser.at(T!["sub"]) {
        parser.bump();
        parser.expect_term();
    }
    parser.bump();
    name(parser, NodeKind::SUB_NAME);
    subroutine_tail(parser, true);
    parser.complete(marker, NodeKind::SUB_DEF);
}

/// Everything after a subroutine's name: prototype or signature, attributes,
/// and either a body or a `;`.
pub(crate) fn subroutine_tail(parser: &mut Parser<'_>, allow_forward_declaration: bool) {
    parser.expect_term();

    if parser.at(T!["("]) {
        signature_or_prototype(parser);
    }

    while parser.at(T![":"]) {
        attribute(parser);
    }

    parser.expect_term();
    if parser.at(T!["{"]) {
        block(parser);
        return;
    }
    if allow_forward_declaration && parser.at(T![";"]) {
        parser.bump();
        return;
    }
    parser.error_recover("expected a subroutine body", Recovery::Statement);
}

/// `sub f($x, $y)` is a signature; `sub f($$)` is a prototype.
///
/// Try the signature; if it does not parse, take the whole group as raw text.
/// The old parser ran a hand-written mini-parser over the characters to decide
/// in advance, which is what rejected the legal prototypes `(_)` and `(+)` (D6).
fn signature_or_prototype(parser: &mut Parser<'_>) {
    let checkpoint = parser.checkpoint();
    let errors_before = parser.diagnostic_count();

    let marker = parser.start();
    parser.bump();
    parser.expect_term();

    let mut ok = true;
    while !parser.at_end() && !parser.at(T![")"]) {
        // A signature parameter is a sigil and a name. `$1` is a perfectly good
        // variable but not a parameter, so this is where `sub f($1)` stops being
        // a signature and becomes a candidate prototype.
        if !parser.current().is_some_and(TokenKind::is_sigil) {
            ok = false;
            break;
        }

        // A placeholder parameter is a bare sigil holding a slot: `sub f($,@,%)`.
        // The scanner reads `$,` as the output field separator variable, which is
        // also real Perl, so only the grammar can say which is meant.
        // Written with a space (`$ = 1`, `$ , `) the sigil stands alone; written
        // without one the scanner has already glued it to a punctuation
        // variable, so both spellings have to be recognised.
        let placeholder = matches!(parser.raw_after_sigil(), Some("," | ")" | "="))
            || (parser.nth_at(1, T![","])
                || parser.nth_at(1, T![")"])
                || parser.nth(1).is_some_and(TokenKind::is_assignment_op));

        let param = parser.start();
        if placeholder {
            parser.bump_sigil();
        } else if parser.nth_at(1, TokenKind::IDENT) {
            primary::variable(parser);
        } else {
            parser.abandon(param);
            ok = false;
            break;
        }
        if parser.at(T!["="]) || parser.at(T!["//="]) || parser.at(T!["||="]) {
            let default = parser.start();
            parser.bump();
            parser.expect_term();
            if expr::expr_assignment(parser).is_none() {
                ok = false;
            }
            parser.complete(default, NodeKind::SIGNATURE_DEFAULT);
        }
        parser.complete(param, NodeKind::SIGNATURE_PARAM);

        if parser.at(T![","]) {
            parser.bump();
            parser.expect_term();
            continue;
        }
        break;
    }

    if ok && parser.at(T![")"]) && parser.diagnostic_count() == errors_before {
        parser.bump();
        parser.expect_operator();
        parser.complete(marker, NodeKind::SUB_SIGNATURE);
        return;
    }

    parser.abandon(marker);
    parser.rollback(checkpoint);

    // Not a signature. It is a prototype if it reads like one; otherwise it is
    // neither, and saying so once is better than reporting each token of it.
    let marker = parser.start();
    let Some(body) = parser.raw_paren_body() else {
        parser.abandon(marker);
        parser.error_recover("expected a signature or prototype", Recovery::Statement);
        return;
    };
    if !is_prototype(body) {
        parser.error("this is neither a valid signature nor a valid prototype");
    }
    parser.bump_raw_parens();
    parser.expect_operator();
    parser.complete(marker, NodeKind::SUB_PROTOTYPE);
}

/// The characters perl allows in a prototype (`perlsub`).
///
/// `_` and `+` are in the set; the old parser's hand-written mini-parser left
/// them out, which is bug D6.
fn is_prototype(body: &str) -> bool {
    body.chars().all(|ch| {
        matches!(
            ch,
            '\\' | '$' | '@' | '%' | '&' | '*' | ';' | '[' | ']' | '+' | '_'
        ) || ch.is_whitespace()
    })
}

fn attribute(parser: &mut Parser<'_>) {
    let marker = parser.start();
    parser.bump();
    parser.expect_operator();
    // `sub f : { ... }` — perl accepts an attribute list that is only its
    // colon, and a codebase that writes `sub f : Tests` everywhere acquires one
    // of these by a dropped word.
    if !parser.current().is_some_and(is_name_like) {
        parser.expect_term();
        parser.complete(marker, NodeKind::ATTR);
        return;
    }
    name(parser, NodeKind::SUB_NAME);
    if parser.at(T!["("]) {
        let args = parser.start();
        if parser.bump_raw_parens() {
            parser.complete(args, NodeKind::ATTR_ARGS);
        } else {
            parser.abandon(args);
        }
    }
    parser.expect_term();
    parser.complete(marker, NodeKind::ATTR);
}

fn package_stmt(parser: &mut Parser<'_>) {
    let marker = parser.start();
    parser.bump();
    parser.expect_operator();
    name(parser, NodeKind::SUB_NAME);
    parser.expect_term();
    if parser.at(TokenKind::VERSION) || parser.at(TokenKind::NUMBER) {
        parser.bump();
    }
    if parser.at(T!["{"]) {
        block(parser);
    } else {
        semicolon(parser);
    }
    parser.complete(marker, NodeKind::PACKAGE_STMT);
}

fn use_stmt(parser: &mut Parser<'_>, node: NodeKind) {
    let marker = parser.start();
    parser.bump();
    parser.expect_operator();
    if parser.current().is_some_and(is_name_like) {
        name(parser, NodeKind::SUB_NAME);
        parser.expect_term();
        // `use Module VERSION LIST`: the version sits between the module and its
        // import list and is not part of it. Reading it as the list's first
        // element leaves the real list unconsumed, and `use Exporter 5.57
        // qw( import );` recovers into an ERROR node holding the `qw` run — at
        // which point the formatter no longer recognises it as a quote-like
        // operator and starts spacing out its delimiters.
        if parser.at(TokenKind::VERSION) || parser.at(TokenKind::NUMBER) {
            parser.bump();
        }
    } else if parser.at(TokenKind::VERSION) || parser.at(TokenKind::NUMBER) {
        parser.bump();
    }
    parser.expect_term();

    if !parser.at(T![";"]) && parser.current().is_some_and(TokenKind::can_start_term) {
        let list = parser.start();
        expr::list_contents(parser, &[]);
        parser.complete(list, NodeKind::LIST_EXPR);
    }
    semicolon(parser);
    parser.complete(marker, node);
}

// ===== Control flow =====

fn if_stmt(parser: &mut Parser<'_>) {
    let marker = parser.start();
    parser.bump();
    condition(parser);
    block(parser);

    loop {
        parser.expect_term();
        if parser.at(T!["elsif"]) {
            let clause = parser.start();
            parser.bump();
            condition(parser);
            block(parser);
            parser.complete(clause, NodeKind::ELSIF_CLAUSE);
            continue;
        }
        if parser.at(T!["else"]) {
            let clause = parser.start();
            parser.bump();
            block(parser);
            parser.complete(clause, NodeKind::ELSE_CLAUSE);
        }
        break;
    }

    parser.complete(marker, NodeKind::IF_STMT);
}

/// The parenthesised part of a control structure.
///
/// Wrapped in `PAREN_EXPR` like any other parenthesised list, so the formatter
/// has one rule for "a bracketed group" rather than a separate case for
/// conditions (ADR 0008 §3).
fn condition(parser: &mut Parser<'_>) {
    parser.expect_term();
    if !parser.at(T!["("]) {
        parser.error("expected `(`");
        parser.recover(Recovery::Statement);
        return;
    }

    let marker = parser.start();
    parser.bump();
    parser.expect_term();
    let list = parser.start();
    expr::list_contents(parser, &[T![")"]]);
    parser.complete(list, NodeKind::LIST_EXPR);
    if !parser.expect(T![")"]) {
        parser.recover(Recovery::List);
        if parser.at(T![")"]) {
            parser.bump();
        }
    }
    parser.complete(marker, NodeKind::PAREN_EXPR);
    parser.expect_term();
}

/// `while`, `until`, `for`, `foreach` — one node kind, since the formatter and
/// any future lint want "a loop", not four near-identical shapes.
fn loop_stmt(parser: &mut Parser<'_>) {
    let marker = parser.start();
    let is_for = parser.at_any(&[T!["for"], T!["foreach"]]);
    parser.bump();
    parser.expect_term();

    if is_for {
        for_header(parser);
    } else {
        condition(parser);
    }

    block(parser);
    parser.complete(marker, NodeKind::LOOP_STMT);
}

/// Both `for (init; test; step)` and `for my $x (@xs)`.
fn for_header(parser: &mut Parser<'_>) {
    // `for my $x (...)` / `for $x (...)`
    if parser.at_any(&[T!["my"], T!["our"], T!["state"], T!["local"]])
        || (parser.current().is_some_and(TokenKind::is_sigil) && !parser.at(T!["("]))
    {
        let header = parser.start();
        if parser.at_any(&[T!["my"], T!["our"], T!["state"], T!["local"]]) {
            // The same builder as any other declaration, so `for my $x` and
            // `my $x` have the same internal shape (ADR 0007 §5).
            primary::var_decl(parser);
        } else {
            primary::variable(parser);
        }
        parser.expect_term();
        condition(parser);
        parser.complete(header, NodeKind::FOREACH_HEADER);
        return;
    }

    if !parser.at(T!["("]) {
        parser.error_recover("expected `(` after `for`", Recovery::Statement);
        return;
    }

    // Distinguish the C-style header by its semicolons.
    let checkpoint = parser.checkpoint();
    let header = parser.start();
    parser.bump();
    parser.expect_term();

    let first = parser.start();
    expr::list_contents(parser, &[T![")"], T![";"]]);
    parser.complete(first, NodeKind::LIST_EXPR);

    if !parser.at(T![";"]) {
        parser.abandon(header);
        parser.rollback(checkpoint);
        let header = parser.start();
        condition(parser);
        parser.complete(header, NodeKind::FOREACH_HEADER);
        return;
    }

    for _ in 0..2 {
        parser.expect(T![";"]);
        parser.expect_term();
        let part = parser.start();
        expr::list_contents(parser, &[T![")"], T![";"]]);
        parser.complete(part, NodeKind::LIST_EXPR);
    }

    if !parser.expect(T![")"]) {
        parser.recover(Recovery::List);
        if parser.at(T![")"]) {
            parser.bump();
        }
    }
    parser.expect_term();
    parser.complete(header, NodeKind::C_STYLE_LOOP_HEADER);
}

fn try_stmt(parser: &mut Parser<'_>) {
    let checkpoint = parser.checkpoint();
    let marker = parser.start();
    parser.bump();
    try_tail(parser);
    parser.expect_term();

    // `try { ... }->method();` is an expression that happens to start with a
    // `try`. Rather than deciding in advance which it is, parse the statement
    // form and reconsider if something follows that only an expression can
    // continue with (ADR 0007 §1).
    if parser.at_any(&[T!["->"], T!["?"]])
        || parser
            .current()
            .is_some_and(|kind| super::grammar::precedence::infix_op(kind).is_some())
    {
        parser.abandon(marker);
        parser.rollback(checkpoint);
        expr_stmt(parser);
        return;
    }

    if parser.at(T![";"]) {
        parser.bump();
    }
    parser.complete(marker, NodeKind::TRY_STMT);
}

/// The block and handlers of a `try`, shared between statement and expression
/// position.
///
/// `my $x = try { ... } catch { ... };` is the same construct in a different
/// slot; giving it one parser keeps it one shape in the tree.
pub(crate) fn try_tail(parser: &mut Parser<'_>) {
    block(parser);

    parser.expect_term();
    if parser.at(T!["catch"]) {
        let clause = parser.start();
        parser.bump();
        parser.expect_term();
        if parser.at(T!["("]) {
            let param = parser.start();
            parser.bump();
            parser.expect_term();
            if parser.current().is_some_and(TokenKind::is_sigil) {
                primary::variable(parser);
            } else {
                parser.error("expected a variable to bind the caught error to");
            }
            parser.expect(T![")"]);
            parser.complete(param, NodeKind::CATCH_PARAM);
        }
        block(parser);
        parser.complete(clause, NodeKind::CATCH_CLAUSE);
    }

    parser.expect_term();
    if parser.at(T!["finally"]) {
        let clause = parser.start();
        parser.bump();
        block(parser);
        parser.complete(clause, NodeKind::FINALLY_CLAUSE);
    }
}

fn given_stmt(parser: &mut Parser<'_>) {
    let marker = parser.start();
    parser.bump();
    condition(parser);
    block(parser);
    parser.complete(marker, NodeKind::GIVEN_STMT);
}

fn when_clause(parser: &mut Parser<'_>, node: NodeKind) {
    let marker = parser.start();
    parser.bump();
    if node == NodeKind::WHEN_CLAUSE {
        condition(parser);
    }
    block(parser);
    parser.complete(marker, node);
}

fn phase_block(parser: &mut Parser<'_>) {
    let marker = parser.start();
    parser.bump();
    block(parser);
    parser.expect_term();
    if parser.at(T![";"]) {
        parser.bump();
    }
    parser.complete(marker, NodeKind::PHASE_BLOCK);
}

fn labeled_stmt(parser: &mut Parser<'_>) {
    let marker = parser.start();
    let label = parser.start();
    parser.bump();
    parser.bump();
    parser.complete(label, NodeKind::LABEL);
    parser.expect_term();
    if parser.at_end() {
        parser.complete(marker, NodeKind::LABELED_STMT);
        return;
    }
    statement(parser);
    parser.complete(marker, NodeKind::LABELED_STMT);
}
