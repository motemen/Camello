//! Expressions.

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
/// was present, so every consumer had to handle both shapes (the parser contract).
pub(crate) fn list_expr(parser: &mut Parser<'_>, recovery: Recovery) -> CompletedMarker {
    let marker = parser.start();
    let _ = recovery;
    list_contents(parser, &[]);
    parser.complete(marker, NodeKind::LIST_EXPR)
}

/// Parse elements until a terminator, without opening a node.
pub(crate) fn list_contents(parser: &mut Parser<'_>, terminators: &[TokenKind]) {
    list_elements(parser, terminators, false);
}

/// Does a `key => ...` pair start just past the separator the parser is on?
///
/// Three tokens of lookahead — the separator, a one-token key, and the `=>` —
/// which is bounded and so allowed (the parser contract). A key spelled with
/// more than one token, `$name => 1`, is not seen, and the list operator keeps
/// everything as it always did.
///
/// The peek happens under whichever expectation the last element left, which
/// settles nothing on its own: the answer is used to decide and never consumed,
/// and `set_expect` rescans from the cursor before anything is (the lexer
/// contract).
fn pair_follows_separator(parser: &mut Parser<'_>) -> bool {
    let key = matches!(parser.nth(1), Some(kind) if kind == TokenKind::IDENT
        || kind == TokenKind::STRING
        || kind == TokenKind::NUMBER
        || kind.is_keyword());
    key && parser.nth(2) == Some(T!["=>"])
}

fn list_elements(parser: &mut Parser<'_>, terminators: &[TokenKind], cut_at_pair: bool) {
    let recovery = if terminators.is_empty() {
        Recovery::Statement
    } else {
        Recovery::List
    };
    let mut pair_value = false;
    loop {
        parser.expect_term();
        if parser.at_end() || parser.at_any(terminators) {
            break;
        }

        // An empty element: `f(1,, 2)`, `f(k =>, 1)`, `f(, 'x')`. perl lets the
        // separator stand on its own and drops the element, and leftover commas
        // are common enough in real code that reading one as a missing
        // expression loses the rest of the list.
        if parser.at_any(&[T![","], T!["=>"]]) {
            parser.bump();
            parser.expect_term();
            continue;
        }

        let before = parser.checkpoint();
        parser.set_at_pair_value(pair_value);
        if expr(parser).is_none() {
            parser.rollback(before);
            parser.error_recover("expected an expression", recovery);
            if !parser.at_any(&[T![","], T!["=>"]]) {
                break;
            }
        }

        if parser.at_any(&[T![","], T!["=>"]]) {
            // The `,` is deliberately left unconsumed: it separates two elements
            // of the list around this one, and taking it here would leave that
            // list with two elements and nothing between them — `f Strbbb`.
            if cut_at_pair && parser.at(T![","]) && pair_follows_separator(parser) {
                break;
            }
            pair_value = parser.at(T!["=>"]);
            parser.bump();
            // A trailing separator is allowed everywhere, which is one option on
            // one list parser rather than four implementations (the parser contract).
            parser.expect_term();
            if parser.at_any(&[T!["}"], T![")"], T!["]"], T![";"]]) {
                break;
            }
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
            && !super::quoted_bareword(parser)
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
        // is expected, after `+` a term, and so on (the lexer contract).
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
    // `-f $x . "y"` groups as perl groups it (the parser contract).
    if kind == TokenKind::FILE_TEST_OP {
        let marker = parser.start();
        parser.bump();
        // `-f // 0` tests `$_` and falls back; see `starts_argument` for why
        // only `//` is decided in operator position.
        parser.expect_operator();
        if parser.at_any(&[T!["//"], T!["//="]]) {
            return Some(parser.complete(marker, NodeKind::FILE_TEST_EXPR));
        }
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
        // optional after a call. A parenthesised expression can be sliced with
        // `[...]`, but `($x){key}` is likewise invalid without an arrow.
        if matches!(kind, T!["["] | T!["{"]) && is_call(lhs.kind()) {
            parser.error("subscripting the result of a call needs `->`");
        } else if kind == T!["{"] && lhs.kind() == NodeKind::PAREN_EXPR {
            parser.error("hash subscription after a parenthesized expression needs `->`");
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
        // Postfix slices: `$r->@[0, 1]` and `$r->%{...}`. The sigil says which
        // slice, the bracket says which subscript.
        // `%` reads as modulo in operator position, which is where the arrow
        // leaves the lexer; a bracket after it says it is a slice sigil instead.
        Some(TokenKind::ARRAY_SIGIL | TokenKind::HASH_SIGIL | TokenKind::MODULO)
            if parser.nth_at(1, T!["["]) || parser.nth_at(1, T!["{"]) =>
        {
            let array = parser.at(TokenKind::ARRAY_SIGIL);
            parser.expect_term();
            // A bare sigil: the scanner would otherwise read `@[` as a
            // punctuation variable and swallow the bracket.
            parser.bump_sigil();
            let close = if parser.at(T!["["]) { T!["]"] } else { T!["}"] };
            parser.bump();
            bracketed_index(parser, close);
            parser.complete(
                marker,
                if array {
                    NodeKind::POSTFIX_ARRAY_SLICE_EXPR
                } else {
                    NodeKind::POSTFIX_HASH_SLICE_EXPR
                },
            )
        }
        _ => {
            // A method name. `->if` and `->s` are legal, which is why the name
            // routine coerces keywords rather than each call site doing it.
            parser.expect_operator();
            // `$invocant->&name(...)` calls the lexical sub `name` with the
            // invocant as its first argument (perl 5.42). The `&` is part of
            // the call, not a sigil on a name to be looked up in a package, so
            // it stays a token of the method call rather than opening a
            // variable — the scanner reads it as bitwise and, this being
            // operator position.
            if parser.at(TokenKind::BITWISE_AND) {
                parser.bump();
            }
            if parser.at(TokenKind::SCALAR_SIGIL) {
                // `$obj->$method()`. Asked again in term position, because a
                // `$` after an arrow is always a sigil and only there does the
                // scanner read what follows it as a variable name: `$obj->$state`
                // otherwise scans `state` as the keyword it is spelled like and
                // the call ends at the bare sigil. The question cannot be asked
                // in term position to begin with — `$obj->s` is a method named
                // `s`, and a term is where that starts a substitution.
                parser.expect_term();
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
/// One implementation, where the old parser had four (the parser contract).
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
    let sigil_argument_follows = sigil_argument_follows(parser.source_after_current());
    let marker = parser.start();
    name(parser, NodeKind::SUB_NAME);

    // Parenthesised call: the shape is unambiguous, whatever the name means —
    // except for the block-taking builtins, handled below.
    let takes_block = builtin.is_some_and(|builtin| builtin.shape == Shape::BlockList);
    // `print( {$fh} @data )` and `print($fh 'text')` put the handle inside the
    // parentheses, where the ordinary argument-list parser reads it as the first
    // element and then finds no comma after it.
    let takes_filehandle = builtin.is_some_and(|builtin| builtin.shape == Shape::FilehandleList)
        && parser.at(T!["("])
        && at_filehandle(parser, 1);
    if parser.at(T!["("]) && !(takes_block && parser.nth_at(1, T!["{"])) && !takes_filehandle {
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
    } else if sigil_argument_follows {
        // GUESS: `foo %h` passes a hash where `foo % h` takes a remainder.
        // Evidence: the spacing alone. Only `%`, `&` and `*` are in doubt at
        // all, and the sub's declaration is not in sight.
        // Wrong: `_error %state, $cb` in AnyEvent::HTTP and `getdata %^H, $wiz`
        // in B::Hooks::EndOfScope lose their argument to an operator.
        parser.expect_term();
    } else {
        // GUESS: `f / 10` divides rather than starting a match.
        // Evidence: none — no declaration in sight. Perl guesses here too; the
        // difference is that the guess is written down in one place (the parser
        // contract) instead of being a special case in the lexer.
        // Wrong: the match runs to the next `/`, taking whatever lies between.
        parser.expect_operator();
    }

    match shape {
        Shape::Nullary => parser.complete(marker, NodeKind::LIST_CALL_EXPR),
        Shape::NamedUnary => {
            if starts_argument(parser, true) {
                parser.expect_term();
                expr_bp(parser, Precedence::NAMED_UNARY);
            }
            parser.expect_operator();
            parser.complete(marker, NodeKind::LIST_CALL_EXPR)
        }
        Shape::BlockList if parser.at(T!["("]) => {
            // `map({...} @xs)` puts the block inside the parentheses. The
            // ordinary argument-list parser would read `{...}` as an anonymous
            // hash and then find no comma.
            let args = parser.start();
            parser.expect(T!["("]);
            parser.expect_term();
            block(parser);
            parser.expect_term();
            // A `,` after the block is perl's and stays in the list, where the
            // formatter can see it: consuming it here dropped it from the
            // output, and `map({$_}, @list)` came back a token short.
            let list = parser.start();
            list_contents(parser, &[T![")"]]);
            parser.complete(list, NodeKind::LIST_EXPR);
            parser.expect(T![")"]);
            parser.complete(args, NodeKind::ARG_LIST);
            parser.expect_operator();
            parser.complete(marker, NodeKind::BLOCK_CALL_EXPR)
        }
        Shape::BlockList => {
            // `sort SUBNAME LIST` and `sort $coderef LIST`: the comparator is
            // followed by the list with no comma between them. Parsing it as the
            // first list element would stop at the second, so it gets its own
            // slot — which is also what makes a name-based special case for
            // `sort` unnecessary (the parser contract).
            if comparator_follows(parser) {
                let comparator = parser.start();
                // `unary`, not `primary`: `\&cmp` is a reference expression.
                unary(parser);
                parser.complete(comparator, NodeKind::FILEHANDLE);
                parser.expect_term();
                list_arguments(parser, builtin.is_none());
                return parser.complete(marker, NodeKind::LIST_CALL_EXPR);
            }
            if parser.at(T!["{"]) {
                block(parser);
                parser.expect_term();
                if parser.at(T![","]) {
                    parser.bump();
                }
                list_arguments(parser, builtin.is_none());
                return parser.complete(marker, NodeKind::BLOCK_CALL_EXPR);
            }
            list_arguments(parser, builtin.is_none());
            parser.complete(marker, NodeKind::LIST_CALL_EXPR)
        }
        Shape::BlockOrTerm => {
            // `eval { ... }` versus `eval $string`. perl never reads the brace
            // as an anonymous hash here, and neither do we.
            if parser.at(T!["{"]) {
                block(parser);
                parser.expect_operator();
                return parser.complete(marker, NodeKind::BLOCK_CALL_EXPR);
            }
            if starts_argument(parser, true) {
                parser.expect_term();
                expr_bp(parser, Precedence::NAMED_UNARY);
            }
            parser.expect_operator();
            parser.complete(marker, NodeKind::LIST_CALL_EXPR)
        }
        Shape::FilehandleList => {
            // `print( {$fh} @data )`: the parentheses are the argument list and
            // the handle is the first thing inside them, so the group is opened
            // here rather than by `arg_list`.
            let parenthesised = parser.at(T!["("]);
            let args = parenthesised.then(|| parser.start());
            if parenthesised {
                parser.bump();
                parser.expect_term();
            }
            filehandle(parser);
            if let Some(args) = args {
                let list = parser.start();
                list_contents(parser, &[T![")"]]);
                parser.complete(list, NodeKind::LIST_EXPR);
                parser.expect(T![")"]);
                parser.complete(args, NodeKind::ARG_LIST);
                parser.expect_operator();
            } else {
                list_arguments(parser, builtin.is_none());
            }
            parser.complete(marker, NodeKind::LIST_CALL_EXPR)
        }
        Shape::List => {
            // `autoflush $fh 1;` — the indirect-object form of `$fh->autoflush(1)`,
            // which Parallel::Scoreboard writes. Only the scalar shape, and only
            // where no declaration says otherwise: a bareword invocant would
            // claim the constant in `mylog ERROR . "x"`, and the comma in
            // `f $x, 1` already says that call is not one of these.
            if builtin.is_none() && parser.at(TokenKind::SCALAR_SIGIL) && at_filehandle(parser, 0) {
                filehandle(parser);
            }
            if block_call_follows(parser) {
                list_arguments(parser, builtin.is_none());
                return parser.complete(marker, NodeKind::BLOCK_CALL_EXPR);
            }
            list_arguments(parser, builtin.is_none());
            parser.complete(marker, NodeKind::LIST_CALL_EXPR)
        }
    }
}

/// `any { $_ eq $key } qw(a b)` — a list operator with a block prototype whose
/// prototype we cannot see.
///
/// `List::Util`, `Try::Tiny` and everything like them export subs declared
/// `(&@)`, and without the declaration the `{...}` reads as an anonymous hash.
/// That is not merely an odd tree: after an anonymous hash the lexer expects an
/// operator, so the `qw` that follows demotes to a bareword and the whole
/// statement recovers into an ERROR node — which is where the formatter stopped
/// recognising the `qw` run and started spacing out its delimiters.
///
/// The tie is broken by which reading parses. A term cannot follow an expression
/// with no operator between them, so if one does, the hash reading was wrong.
/// `-` and `+` are held back: they are terms and operators both, and
/// `$config->{limit} - 1` must not be read as a block call on `-1`.
///
/// Consumes the block and returns true when it takes that reading; leaves the
/// parser exactly where it found it otherwise.
fn block_call_follows(parser: &mut Parser<'_>) -> bool {
    if !parser.at(T!["{"]) {
        return false;
    }

    let checkpoint = parser.checkpoint();
    let errors_before = parser.diagnostic_count();
    block(parser);
    parser.expect_term();

    let list_follows = parser.diagnostic_count() == errors_before
        && !parser.at_any(&[T!["-"], T!["+"], T!["++"], T!["--"]])
        && parser.current().is_some_and(TokenKind::can_start_term);
    if list_follows {
        if parser.at(T![","]) {
            parser.bump();
        }
        return true;
    }

    parser.rollback(checkpoint);
    false
}

/// `print STDERR ...` / `print {$fh} ...`.
///
/// Marked as its own node rather than left as an unexplained first argument
/// (the parser contract).
/// Does a filehandle slot start `base` tokens ahead?
///
/// Asked at the name (`print $fh @lines`) and one token past a `(`
/// (`print( {$fh} @lines )`), which is why it takes an offset rather than
/// looking at the current token.
fn at_filehandle(parser: &mut Parser<'_>, base: usize) -> bool {
    // GUESS: an all-capital bareword in the first slot is a filehandle.
    // Evidence: the capitals, and that no `(`, `,`, `=>` or `;` follows. perl
    // decides this from its symbol table, and we have none, so `print FOO(1)`
    // stays ambiguous — with one exception: perl's own handles are always
    // handles, and `print STDERR ("x")` prints to standard error rather than
    // calling a sub named STDERR.
    // Wrong: every such line in `Getopt::Long` and `Debian::AdduserLogging`
    // becomes a function call.
    const PERL_HANDLES: &[&str] = &["STDIN", "STDOUT", "STDERR", "ARGV", "ARGVOUT", "DATA"];
    let is_perl_handle = parser
        .nth_text(base)
        .is_some_and(|text| PERL_HANDLES.contains(&text));

    let is_bareword_handle = parser.nth_at(base, TokenKind::IDENT)
        && parser
            .nth_text(base)
            .is_some_and(|text| text.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_'))
        && (is_perl_handle || !parser.nth_at(base + 1, T!["("]))
        && !parser.nth_at(base + 1, T![","])
        && !parser.nth_at(base + 1, T!["=>"])
        && !parser.nth_at(base + 1, T![";"]);

    let is_block_handle = parser.nth_at(base, T!["{"]);

    // `print $fh @lines` — a scalar handle, told apart from `print $x, $y` by
    // the absence of a comma. A subscript says the scalar is not the handle:
    // `print $claim{sub}, $rest` prints a hash element, and the handle slot
    // takes a simple scalar and nothing else.
    let is_scalar_handle = parser.nth_at(base, TokenKind::SCALAR_SIGIL)
        && parser.nth_at(base + 1, TokenKind::IDENT)
        && !parser.nth_at(base + 2, T!["{"])
        && !parser.nth_at(base + 2, T!["["])
        && !parser.nth_at(base + 2, T!["->"])
        && parser
            .nth(base + 2)
            .is_some_and(|kind| kind.can_start_term() || kind == TokenKind::HEREDOC_START);

    // `print ${fh} "text"` is the braced spelling of the same scalar
    // filehandle. Keep the lookahead deliberately narrow: a simple name may be
    // a handle, while `${expr}` and `${fh}{key}` are ordinary values. The
    // variable parser already knows how to consume the braced form once this
    // predicate selects it.
    let is_braced_scalar_handle = parser.nth_at(base, TokenKind::SCALAR_SIGIL)
        && parser.nth_at(base + 1, T!["{"])
        && parser
            .nth(base + 2)
            .is_some_and(|kind| kind == TokenKind::IDENT || kind.is_keyword())
        && parser.nth_at(base + 3, T!["}"])
        && parser
            .nth(base + 4)
            .is_some_and(|kind| kind.can_start_term() || kind == TokenKind::HEREDOC_START);

    is_bareword_handle || is_block_handle || is_scalar_handle || is_braced_scalar_handle
}

fn filehandle(parser: &mut Parser<'_>) {
    if !at_filehandle(parser, 0) {
        return;
    }
    let is_scalar_handle = parser.at(TokenKind::SCALAR_SIGIL);
    let is_block_handle = parser.at(T!["{"]);

    let marker = parser.start();
    if is_scalar_handle {
        super::primary::variable(parser);
    } else if is_block_handle {
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

/// The arguments of a call written without parentheses.
///
/// GUESS: a bareword written as the value of a pair takes no argument past the
/// next pair.
/// Evidence: the name is not in the builtin table, so nothing is known about
/// its arity; the list around it is already a table of pairs, because this call
/// is the value of one; and the next element is a pair too. perl reads the
/// arity off a prototype camello cannot see, and reading it the other way is
/// reading it as `($)` rather than as no prototype at all — both are Perl, and
/// neither is more the program than the other.
/// Wrong: only the shape. The pairs stay in the call's argument list, where the
/// one-element-per-line rule of the brackets around them does not reach them
/// (docs/formatting.md INDENT-2).
///
/// Written after anything else — `getopt \@args, 'a|all' => \$all, ...` — the
/// call keeps the whole list, which is what the option table of every `getopt`
/// in the wild wants.
fn list_arguments(parser: &mut Parser<'_>, unknown_name: bool) {
    let cut_at_pair = parser.take_at_pair_value() && unknown_name;
    // A list operator's argument is not optional in the sense `shift`'s is:
    // `split //, $s` really does take a pattern.
    if !starts_argument(parser, false) {
        return;
    }
    let marker = parser.start();
    list_elements(parser, &[], cut_at_pair);
    parser.complete(marker, NodeKind::LIST_EXPR);
    parser.expect_operator();
}

/// A `sort`-style comparator: a scalar or a code reference, with a term after
/// it and no comma in between.
fn comparator_follows(parser: &mut Parser<'_>) -> bool {
    parser.expect_term();
    let leads = match parser.current() {
        Some(TokenKind::SCALAR_SIGIL) => parser.nth_at(1, TokenKind::IDENT),
        Some(T!["\\"]) => parser.nth(1) == Some(TokenKind::CODE_SIGIL),
        _ => false,
    };
    if !leads {
        return false;
    }
    // Two tokens for `$cmp`, three for `\&cmp`; whatever comes next must start
    // the list rather than continue the expression.
    let after = if parser.at(TokenKind::SCALAR_SIGIL) {
        2
    } else {
        3
    };
    parser
        .nth(after)
        .is_some_and(|kind| kind.is_sigil() || kind == TokenKind::IDENT)
}

/// Does an argument list start here?
///
/// Asked under whatever `expect` the caller established. For a name whose
/// declaration is unknown that is operator position, so `f / 10` divides; for a
/// builtin that takes a list it is term position, so `keys %h` keeps its
/// argument. Deciding it once, in the builtin table, is what keeps the question
/// out of the lexer (the parser contract).
fn starts_argument(parser: &mut Parser<'_>, argument_is_optional: bool) -> bool {
    // `shift // 1` is defined-or applied to an argument-less `shift`, and perl
    // special-cases exactly this. Ask in operator position to see it; asking in
    // term position would read `//` as an empty match and the "argument" would
    // swallow the rest of the statement.
    //
    // Only `//` is decided this way. Widening it to every infix operator would
    // settle `%`, `*` and `&` in operator position too, and `keys %seen` would
    // lose its argument to modulo.
    // `run until => 1` passes the string `"until"`: the `=>` quotes it, so it
    // neither ends the call nor modifies the statement.
    if super::quoted_bareword(parser) {
        return true;
    }

    let operator_position = parser.expect_is_operator();
    if argument_is_optional {
        if !operator_position {
            parser.expect_operator();
        }
        if parser.at_any(&[T!["//"], T!["//="]]) {
            if !operator_position {
                parser.expect_term();
            }
            return false;
        }
        if !operator_position {
            parser.expect_term();
        }
    }
    if operator_position {
        // The caller said an operator is expected here, so anything that is one
        // ends the call rather than starting an argument — with one exception.
        // `decode <$fh>` reads a line and `f < $x` compares, and the two are
        // told apart by whether a complete readline operator lexes here.
        if parser.at(T!["<"]) {
            parser.expect_term();
            if parser.at(TokenKind::IO_HANDLE) {
                return true;
            }
            parser.expect_operator();
            return false;
        }
        // `matches <<"EOF"` hands over a heredoc and `$bits << 2` shifts, and
        // the same question tells them apart: does a complete heredoc marker
        // lex here. Getting it wrong is the one failure that is silent — the
        // body is then read as code, and only says so when it does not happen
        // to be valid Perl.
        if parser.at(T!["<<"]) {
            parser.expect_term();
            if parser.at(TokenKind::HEREDOC_START) {
                return true;
            }
            parser.expect_operator();
            return false;
        }
        // GUESS: `-` here starts a file test rather than a subtraction.
        // Evidence: whether a file test operator lexes at all. `ok -f $path`
        // and `PI - 1` differ by nothing else.
        // Wrong: `s`, `y` and `m` are file tests and quote-like operators both,
        // so `ok -s $path` read as subtraction starts a substitution whose body
        // runs to the end of the file.
        if parser.at(T!["-"]) {
            parser.expect_term();
            if parser.at(TokenKind::FILE_TEST_OP) {
                return true;
            }
            parser.expect_operator();
            return false;
        }
        // GUESS: a `+` glued to its operand is perl's documented disambiguating
        // unary plus, whose only purpose is to open an argument list where a
        // bare `{` or `(` would be read as something else.
        // Evidence: the spacing, with no declaration in sight. The idiom hugs
        // its operand and leaves a space in front — `ok +Foo->bar` — where
        // arithmetic on a constant (`PI + 1`, `MAX-1`) does not.
        // Wrong: the argument list becomes addition applied to an argument-less
        // call, the one reading the idiom cannot have.
        if parser.at(T!["+"]) && parser.current_is_glued_prefix() {
            parser.expect_term();
            if parser.nth(1).is_some_and(TokenKind::can_start_term) {
                return true;
            }
            parser.expect_operator();
        }
        if parser
            .current()
            .is_some_and(|kind| infix_op(kind).is_some() || kind.is_stmt_modifier())
        {
            return false;
        }
    }

    parser.expect_term();
    match parser.current() {
        None => false,
        Some(kind) if kind.is_stmt_modifier() => false,
        // A word spelling an infix operator does not start an argument, in term
        // position any more than in operator position: `ref and /x/` tests `$_`
        // and then matches, and reading `and` as a bareword argument left the
        // `/x/` to divide by. In term position the lexer hands these back as
        // identifiers, because `{ or => 1 }` needs it to (the lexer contract), so the
        // text is what has to be asked. A quoted bareword was settled above.
        Some(TokenKind::IDENT)
            if parser
                .current_text()
                .and_then(TokenKind::from_keyword)
                .is_some_and(|kind| infix_op(kind).is_some()) =>
        {
            false
        }
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

/// Does a `%`, `&` or `*` written against the name after it follow here?
///
/// These three characters are a sigil where a term is expected and an operator
/// where one is not, and after a bareword with no declaration in sight both
/// readings are open: `f %h` is a call taking a hash, `$x % $y` is a modulus.
/// The spacing is the only evidence there is — a space in front and none behind
/// is how `_error %state, $cb` is written and how nobody writes a modulus.
fn sigil_argument_follows(rest: &str) -> bool {
    let trimmed = rest.trim_start_matches([' ', '\t']);
    if trimmed.len() == rest.len() {
        return false;
    }
    let mut chars = trimmed.chars();
    matches!(chars.next(), Some('%' | '&' | '*'))
        && chars
            .next()
            .is_some_and(|ch| ch.is_alphabetic() || matches!(ch, '_' | '{' | '$' | '^'))
}
