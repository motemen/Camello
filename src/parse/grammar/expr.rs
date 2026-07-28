//! Expressions (ADR 0007 §2, §4).

use crate::lang::{NodeKind, TokenKind, T};
use crate::lex::Expect;
use crate::parse::event::CompletedMarker;
use crate::parse::{Parser, Recovery};

use super::builtins::{self, Shape};
use super::precedence::{infix_op, right_binding_power, Associativity, Precedence};
use super::{block, name};

/// A full expression, up to but not including a top-level comma.
pub(crate) fn expr(parser: &mut Parser<'_>) -> Option<CompletedMarker> {
    expr_bp(parser, Precedence::LOWEST)
}

/// An expression that stops short of the low-precedence `and`/`or`/`not`.
///
/// Signature defaults need this: `sub f($x = 1 or 2)` is not a signature, and
/// parsing the default at full precedence would silently accept it.
pub(crate) fn expr_assignment(parser: &mut Parser<'_>) -> Option<CompletedMarker> {
    expr_bp(parser, Precedence::ASSIGNMENT)
}

/// A comma-separated series, always wrapped in `LIST_EXPR`.
///
/// "Always" is the point: the old parser produced a wrapper only when a comma
/// was present, so every consumer had to handle both shapes (ADR 0007 §2).
pub(crate) fn list_expr(parser: &mut Parser<'_>, recovery: Recovery) -> CompletedMarker {
    let marker = parser.start();
    let _ = recovery;
    list_contents(parser, &[]);
    parser.complete(marker, NodeKind::LIST_EXPR)
}

/// Parse elements until a terminator, without opening a node.
pub(crate) fn list_contents(parser: &mut Parser<'_>, terminators: &[TokenKind]) {
    let recovery = if terminators.is_empty() {
        Recovery::Statement
    } else {
        Recovery::List
    };
    loop {
        parser.expect_term();
        if parser.at_end() || parser.at_any(terminators) {
            break;
        }

        let before = parser.checkpoint();
        if expr(parser).is_none() {
            parser.rollback(before);
            parser.error_recover("expected an expression", recovery);
            if !parser.at_any(&[T![","], T!["=>"]]) {
                break;
            }
        }

        if parser.at_any(&[T![","], T!["=>"]]) {
            parser.bump();
            continue;
        }
        break;
    }
}

fn expr_bp(parser: &mut Parser<'_>, min: Precedence) -> Option<CompletedMarker> {
    let mut lhs = unary(parser)?;

    loop {
        // A postfix statement modifier ends the expression; the statement layer
        // owns it.
        if parser
            .current()
            .is_some_and(|kind| kind.is_stmt_modifier() && min <= Precedence::COMMA)
        {
            break;
        }

        let Some(kind) = parser.current() else { break };

        if kind == T!["?"] && Precedence::TERNARY >= min {
            lhs = ternary(parser, lhs);
            continue;
        }

        let Some(op) = infix_op(kind) else { break };
        if op.precedence < min {
            break;
        }

        let marker = parser.precede(lhs);
        parser.bump();

        // The one place the builtin table feeds the lexer: after `=~` a pattern
        // is expected, after `+` a term, and so on (ADR 0005 §2).
        parser.expect_term();
        if expr_bp(parser, right_binding_power(op)).is_none() {
            parser.error("expected an expression after the operator");
        }
        lhs = parser.complete(marker, op.node);
        parser.expect_operator();

        if op.associativity == Associativity::None {
            // `1 < 2 < 3` is a syntax error in perl; stopping here reports it
            // once instead of silently building a left-nested tree.
            if parser.current().and_then(infix_op).is_some_and(|next| {
                next.precedence == op.precedence && next.associativity == Associativity::None
            }) {
                parser.error("this operator cannot be chained");
            }
        }
    }

    Some(lhs)
}

fn ternary(parser: &mut Parser<'_>, condition: CompletedMarker) -> CompletedMarker {
    let marker = parser.precede(condition);
    parser.bump();
    parser.expect_term();
    expr_bp(parser, Precedence::ASSIGNMENT);
    if !parser.expect(T![":"]) {
        return parser.complete(marker, NodeKind::TERNARY_EXPR);
    }
    parser.expect_term();
    expr_bp(parser, Precedence::ASSIGNMENT);
    parser.expect_operator();
    parser.complete(marker, NodeKind::TERNARY_EXPR)
}

fn unary(parser: &mut Parser<'_>) -> Option<CompletedMarker> {
    parser.expect_term();
    let kind = parser.current()?;

    // File tests bind at named-unary precedence, not prefix precedence, so
    // `-f $x . "y"` groups as perl groups it (ADR 0007 §4).
    if kind == TokenKind::FILE_TEST_OP {
        let marker = parser.start();
        parser.bump();
        parser.expect_term();
        expr_bp(parser, Precedence::NAMED_UNARY);
        parser.expect_operator();
        return Some(parser.complete(marker, NodeKind::FILE_TEST_EXPR));
    }

    if matches!(kind, T!["!"] | T!["~"] | T!["\\"] | T!["-"] | T!["+"]) {
        let marker = parser.start();
        parser.bump();
        parser.expect_term();
        if expr_bp(parser, Precedence::PREFIX).is_none() {
            parser.error("expected an operand");
        }
        parser.expect_operator();
        let node = if kind == T!["\\"] {
            NodeKind::REFERENCE_EXPR
        } else {
            NodeKind::PREFIX_EXPR
        };
        return Some(parser.complete(marker, node));
    }

    if kind == T!["not"] {
        let marker = parser.start();
        parser.bump();
        parser.expect_term();
        expr_bp(parser, Precedence::NOT_KW);
        parser.expect_operator();
        return Some(parser.complete(marker, NodeKind::PREFIX_EXPR));
    }

    if matches!(kind, T!["++"] | T!["--"]) {
        let marker = parser.start();
        parser.bump();
        parser.expect_term();
        if expr_bp(parser, Precedence::INCREMENT).is_none() {
            parser.error("expected an operand");
        }
        parser.expect_operator();
        return Some(parser.complete(marker, NodeKind::PREFIX_EXPR));
    }

    let primary = super::primary::primary(parser)?;
    Some(postfix(parser, primary))
}

/// `->` chains, subscripts, calls and `++`/`--`.
pub(crate) fn postfix(parser: &mut Parser<'_>, mut lhs: CompletedMarker) -> CompletedMarker {
    loop {
        parser.expect_operator();
        let Some(kind) = parser.current() else { break };

        // `f()[0]` and `f(){k}` are syntax errors in perl; the arrow is not
        // optional after a call.
        if matches!(kind, T!["["] | T!["{"]) && is_call(lhs.kind()) {
            parser.error("subscripting the result of a call needs `->`");
        }

        lhs = match kind {
            T!["->"] => arrow(parser, lhs),
            T!["["] => subscript(parser, lhs, NodeKind::ARRAY_SUBSCRIPT_EXPR, T!["]"]),
            T!["{"] => subscript(parser, lhs, NodeKind::HASH_SUBSCRIPT_EXPR, T!["}"]),
            T!["("] => {
                let marker = parser.precede(lhs);
                arg_list(parser);
                parser.complete(marker, NodeKind::CODE_CALL_EXPR)
            }
            T!["++"] | T!["--"] => {
                let marker = parser.precede(lhs);
                parser.bump();
                parser.complete(marker, NodeKind::POSTFIX_EXPR)
            }
            TokenKind::POSTFIX_DEREF_ARRAY
            | TokenKind::POSTFIX_DEREF_HASH
            | TokenKind::POSTFIX_DEREF_SCALAR
            | TokenKind::POSTFIX_DEREF_ARRAY_LAST_INDEX
            | TokenKind::POSTFIX_DEREF_CODE
            | TokenKind::POSTFIX_DEREF_GLOB => {
                let marker = parser.precede(lhs);
                parser.bump();
                parser.complete(marker, NodeKind::POSTFIX_DEREF_EXPR)
            }
            _ => break,
        };
    }
    parser.expect_operator();
    lhs
}

/// Calls whose result cannot be subscripted without an arrow.
///
/// `$code->()[0]` is fine — the arrow is already there — so a code-reference
/// call is not in this set.
fn is_call(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::CALL_EXPR | NodeKind::LIST_CALL_EXPR | NodeKind::BLOCK_CALL_EXPR
    )
}

fn arrow(parser: &mut Parser<'_>, lhs: CompletedMarker) -> CompletedMarker {
    let marker = parser.precede(lhs);
    parser.bump();

    match parser.current() {
        Some(T!["["]) => {
            parser.bump();
            bracketed_index(parser, T!["]"]);
            parser.complete(marker, NodeKind::ARRAY_SUBSCRIPT_EXPR)
        }
        Some(T!["{"]) => {
            parser.bump();
            bracketed_index(parser, T!["}"]);
            parser.complete(marker, NodeKind::HASH_SUBSCRIPT_EXPR)
        }
        Some(T!["("]) => {
            arg_list(parser);
            parser.complete(marker, NodeKind::CODE_CALL_EXPR)
        }
        _ => {
            // A method name. `->if` and `->s` are legal, which is why the name
            // routine coerces keywords rather than each call site doing it.
            parser.expect_operator();
            if parser.at(TokenKind::SCALAR_SIGIL) {
                // `$obj->$method()`
                super::primary::variable(parser);
            } else {
                name(parser, NodeKind::SUB_NAME);
            }
            if parser.at(T!["("]) {
                arg_list(parser);
            }
            parser.complete(marker, NodeKind::METHOD_CALL_EXPR)
        }
    }
}

/// One `[...]` or `{...}` subscript, for callers that must not treat a
/// following `(` as a call.
pub(crate) fn postfix_subscript(parser: &mut Parser<'_>, lhs: CompletedMarker) -> CompletedMarker {
    let (node, close) = if parser.at(T!["["]) {
        (NodeKind::ARRAY_SUBSCRIPT_EXPR, T!["]"])
    } else {
        (NodeKind::HASH_SUBSCRIPT_EXPR, T!["}"])
    };
    subscript(parser, lhs, node, close)
}

fn subscript(
    parser: &mut Parser<'_>,
    lhs: CompletedMarker,
    node: NodeKind,
    close: TokenKind,
) -> CompletedMarker {
    let marker = parser.precede(lhs);
    parser.bump();
    bracketed_index(parser, close);
    parser.complete(marker, node)
}

/// The inside of `[...]` or `{...}` used as a subscript.
fn bracketed_index(parser: &mut Parser<'_>, close: TokenKind) {
    let marker = parser.start();
    parser.expect_term();

    // A bareword hash key: `$h{key}`, including keywords such as `$h{if}`.
    if close == T!["}"]
        && parser.nth_at(1, T!["}"])
        && parser.current().is_some_and(is_bareword_key)
    {
        name(parser, NodeKind::SUB_NAME);
    } else {
        list_contents(parser, &[close]);
    }

    parser.complete(marker, NodeKind::SUBSCRIPT);
    if !parser.expect(close) {
        parser.recover(Recovery::List);
        if parser.at(close) {
            parser.bump();
        }
    }
    parser.expect_operator();
}

fn is_bareword_key(kind: TokenKind) -> bool {
    kind == TokenKind::IDENT || kind.is_keyword()
}

/// A parenthesised argument list, including the parentheses.
///
/// One implementation, where the old parser had four (ADR 0007 §5).
pub(crate) fn arg_list(parser: &mut Parser<'_>) {
    let marker = parser.start();
    parser.expect(T!["("]);
    let list = parser.start();
    list_contents(parser, &[T![")"]]);
    parser.complete(list, NodeKind::LIST_EXPR);
    if !parser.expect(T![")"]) {
        parser.recover(Recovery::List);
        if parser.at(T![")"]) {
            parser.bump();
        }
    }
    parser.complete(marker, NodeKind::ARG_LIST);
    parser.expect_operator();
}

/// A call written as a bareword: `foo(...)`, `print $fh @xs`, `map { ... } @xs`.
pub(crate) fn bareword_call(parser: &mut Parser<'_>) -> CompletedMarker {
    let text = parser.current_text().unwrap_or_default();
    let builtin = builtins::lookup(text);
    let marker = parser.start();
    name(parser, NodeKind::SUB_NAME);

    // Parenthesised call: the shape is unambiguous, whatever the name means.
    if parser.at(T!["("]) {
        arg_list(parser);
        return parser.complete(marker, NodeKind::CALL_EXPR);
    }

    let shape = builtin.map_or(
        if builtins::UNKNOWN_IS_LIST_OPERATOR {
            Shape::List
        } else {
            Shape::Nullary
        },
        |builtin| builtin.shape,
    );

    if let Some(builtin) = builtin {
        parser.set_expect(builtin.expect_after_name);
    } else {
        parser.expect_term();
    }

    match shape {
        Shape::Nullary => parser.complete(marker, NodeKind::LIST_CALL_EXPR),
        Shape::NamedUnary => {
            if starts_argument(parser) {
                parser.expect_term();
                expr_bp(parser, Precedence::NAMED_UNARY);
            }
            parser.expect_operator();
            parser.complete(marker, NodeKind::LIST_CALL_EXPR)
        }
        Shape::BlockList => {
            if parser.at(T!["{"]) {
                block(parser);
                parser.expect_term();
                if parser.at(T![","]) {
                    parser.bump();
                }
                list_arguments(parser);
                return parser.complete(marker, NodeKind::BLOCK_CALL_EXPR);
            }
            list_arguments(parser);
            parser.complete(marker, NodeKind::LIST_CALL_EXPR)
        }
        Shape::FilehandleList => {
            filehandle(parser);
            list_arguments(parser);
            parser.complete(marker, NodeKind::LIST_CALL_EXPR)
        }
        Shape::List => {
            list_arguments(parser);
            parser.complete(marker, NodeKind::LIST_CALL_EXPR)
        }
    }
}

/// `print STDERR ...` / `print {$fh} ...`.
///
/// Marked as its own node rather than left as an unexplained first argument
/// (ADR 0007 §2).
fn filehandle(parser: &mut Parser<'_>) {
    let is_bareword_handle = parser.at(TokenKind::IDENT)
        && parser
            .current_text()
            .is_some_and(|text| text.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_'))
        && !parser.nth_at(1, T!["("])
        && !parser.nth_at(1, T![","])
        && !parser.nth_at(1, T!["=>"])
        && !parser.nth_at(1, T![";"]);

    let is_block_handle = parser.at(T!["{"]);

    if !is_bareword_handle && !is_block_handle {
        return;
    }

    let marker = parser.start();
    if is_block_handle {
        parser.bump();
        parser.expect_term();
        expr(parser);
        parser.expect(T!["}"]);
    } else {
        parser.bump();
    }
    parser.complete(marker, NodeKind::FILEHANDLE);
    parser.expect_term();
}

fn list_arguments(parser: &mut Parser<'_>) {
    if !starts_argument(parser) {
        return;
    }
    let marker = parser.start();
    list_contents(parser, &[]);
    parser.complete(marker, NodeKind::LIST_EXPR);
    parser.expect_operator();
}

fn starts_argument(parser: &mut Parser<'_>) -> bool {
    // `shift // 1` is defined-or applied to an argument-less `shift`, and perl
    // special-cases exactly this. Ask in operator position to see it; asking in
    // term position would read `//` as an empty match and the "argument" would
    // swallow the rest of the statement.
    //
    // Only `//` is decided this way. Widening it to every infix operator would
    // settle `%`, `*` and `&` in operator position too, and `keys %seen` would
    // lose its argument to modulo.
    parser.expect_operator();
    if parser.at_any(&[T!["//"], T!["//="]]) {
        return false;
    }

    parser.expect_term();
    match parser.current() {
        None => false,
        Some(kind) if kind.is_stmt_modifier() => false,
        Some(kind) => kind.can_start_term(),
    }
}

impl Parser<'_> {
    fn set_expect(&mut self, expect: Expect) {
        match expect {
            Expect::Term => self.expect_term(),
            Expect::Operator => self.expect_operator(),
        }
    }
}
