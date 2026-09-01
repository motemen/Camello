//! Arity: what a call passes against what the callee declared
//! (`docs/typecheck.md`, milestone 3).
//!
//! Three things have to be true before a count is compared, and each of them
//! is a class of false positive that would otherwise have arrived:
//!
//! - **the callee is known.** A bareword resolves through the package in
//!   effect and then through the imports (`Program::resolve_call`); anything
//!   else is `Unknown` and silent.
//! - **the callee's parameter list is known.** A sub that never touches `@_`
//!   takes any number of arguments and perl does not mind, so it declares
//!   nothing and nothing is compared.
//! - **the count is known.** Perl flattens: `f(@list)` passes as many
//!   arguments as `@list` has, and nobody knows how many that is. Only a call
//!   whose every argument is one value has a count at all — see [`arg_count`].
//!
//! The severity follows the source of the parameter list ([`ParamSource`]),
//! because it is what decides whether a mismatch is a contradiction or a
//! shape: perl dies on a signature mismatch and Smart::Args dies on a missing
//! named argument, while `my ($a, $b) = @_` called with one argument is a
//! program that runs.

use camello_syntax::ast::{self, AstNode, Call, MethodCall, Sigil, Variable};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode, TokenExt, TokenKind};

use crate::decl::{ParamSource, Params};
use crate::diag::{Code, Diagnostic, Severity};
use crate::program::Program;

/// Check every call in a file against what it resolves to.
#[must_use]
pub fn analyse(root: &SyntaxNode, file: usize, program: &Program) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for node in root.descendants() {
        if let Some(call) = Call::cast(node.clone()) {
            check_call(&call, file, program, &mut diagnostics);
        } else if node.node_kind() == NodeKind::METHOD_CALL_EXPR {
            let call = MethodCall::cast(node).expect("kind checked");
            check_method_call(&call, program, &mut diagnostics);
        }
    }
    diagnostics
}

fn check_call(call: &Call, file: usize, program: &Program, into: &mut Vec<Diagnostic>) {
    let Some(name) = call.callee_name() else {
        return;
    };
    let offset = u32::from(call.syntax().text_range().start());
    let Some(symbol) = program.resolve_call(file, offset, &name) else {
        return;
    };
    // A method's invocant comes through `->`, so calling it as a plain
    // function is a different shape and not one to count against.
    if symbol.params.is_method() {
        return;
    }
    let shape = CallShape::of(&call.args(), &call.pairs());
    compare(
        &symbol.params,
        &shape,
        false,
        &symbol.name,
        call.callee_range(),
        into,
    );
}

fn check_method_call(call: &MethodCall, program: &Program, into: &mut Vec<Diagnostic>) {
    let (Some(invocant), Some(method)) = (call.invocant(), call.method_name()) else {
        return;
    };
    // Only a bareword class resolves without the type lattice; `$obj->m()`
    // waits for milestone 5.
    let Some(class) = bareword_name(&invocant) else {
        return;
    };
    let Some(symbol) = program.sub(&class, &method) else {
        return;
    };
    let shape = CallShape::of(&call.args(), &call.pairs());
    compare(
        &symbol.params,
        &shape,
        true,
        &symbol.name,
        call.method_range(),
        into,
    );
}

/// The name of a bareword invocant, `Foo::Bar` in `Foo::Bar->new`.
fn bareword_name(node: &SyntaxNode) -> Option<String> {
    let call = Call::cast(node.clone())?;
    call.args().is_empty().then(|| call.callee_name()).flatten()
}

/// The count comparison, for a caller that resolved the callee some other way.
///
/// The flow pass reaches a method through the *type* of its invocant, which is
/// the only way `$counter->add(1, 2, 3)` is ever resolved — a bareword
/// invocant is all this pass can see on its own.
pub fn check_shape(
    params: &Params,
    call: &CallShape,
    through_arrow: bool,
    name: &str,
    range: rowan::TextRange,
    into: &mut Vec<Diagnostic>,
) {
    compare(params, call, through_arrow, name, range, into);
}

fn compare(
    params: &Params,
    call: &CallShape,
    through_arrow: bool,
    name: &str,
    range: rowan::TextRange,
    into: &mut Vec<Diagnostic>,
) {
    let Some(source) = params.source() else {
        return;
    };

    // A `Dict` of named parameters is checked by shape rather than by count:
    // `f(a => 1, b => 2)` is pairs, and Smart::Args also accepts one hashref.
    // What it does not accept is a value that is neither — and only a value
    // that *cannot* be a hashref says so, because `f($options)` passes one.
    if let Params::Named { params: named, .. } = params {
        if named.is_empty() {
            return;
        }
        if let Some(stray) = call.stray_positional() {
            into.push(
                Diagnostic::new(
                    Code::Arity,
                    stray,
                    format!(
                        "`{}` takes named arguments; this one is neither a `key => value` pair \
                         nor a hash reference",
                        name
                    ),
                )
                .at(severity(source)),
            );
        }
        return;
    }

    // perlsub, "Prototypes": "Method calls are not influenced by prototypes
    // either, because the function to be called is indeterminate at compile
    // time". A bare `()` is a prototype where the signatures feature is off
    // and a signature where it is on, and this pass cannot tell which — so
    // the invocant it passes is either harmless or fatal, and saying which
    // would be a guess. It is said once, at `info` (DIAG-15).
    if through_arrow && params.is_empty_parens() {
        into.push(
            Diagnostic::new(
                Code::IgnoredPrototype,
                range,
                format!(
                    "`{}` is declared `()`, which a method call ignores: perl passes the \
                     invocant regardless",
                    name
                ),
            )
            .at(Code::IgnoredPrototype.default_severity()),
        );
        return;
    }

    let Some(mut count) = call.count else {
        return;
    };
    // `Class->m(1)` passes two arguments, the invocant and the one written.
    if through_arrow {
        count += 1;
    }
    let minimum = params.minimum().unwrap_or(0);
    let maximum = params.maximum();
    // `my ($data, $header, $password, $cipher) = @_` declares four names and
    // is routinely called with two: perl fills what it has with `undef`, and
    // the body asks `if defined`. So an unpacking list has no minimum worth
    // reporting — 285 of the 285 arity findings over @INC were this — and only
    // an argument the sub can never read is a shape worth a word.
    let minimum = if source == ParamSource::Unpacking {
        0
    } else {
        minimum
    };
    let bound = if count < minimum {
        Some(("at least", minimum))
    } else if maximum.is_some_and(|maximum| count > maximum) {
        Some(("at most", maximum.expect("checked")))
    } else {
        None
    };
    if let Some((direction, bound)) = bound {
        into.push(
            Diagnostic::new(
                Code::Arity,
                range,
                format!(
                    "`{}` takes {direction} {bound} argument{}{}; {count} passed",
                    name,
                    if bound == 1 { "" } else { "s" },
                    if through_arrow {
                        " including its invocant"
                    } else {
                        ""
                    }
                ),
            )
            .at(severity(source)),
        );
    }
}

/// perl dies on a signature mismatch and Smart::Args dies on a bad `args`
/// list, so those are contradictions between two declared things. An `@_`
/// unpacking declares a shape and not a rule: the program runs either way.
fn severity(source: ParamSource) -> Severity {
    match source {
        ParamSource::Signature | ParamSource::Args => Severity::Error,
        ParamSource::Unpacking | ParamSource::Generated => Severity::Warning,
    }
}

/// What a call site passes, as far as anyone can tell from the source.
pub struct CallShape {
    /// How many arguments, or `None` when nobody can know.
    pub count: Option<usize>,
    /// Where an argument sits that is neither a `key => value` pair nor
    /// something that could be a hash reference.
    stray: Option<rowan::TextRange>,
}

impl CallShape {
    #[must_use]
    pub fn of(arguments: &[SyntaxNode], pairs: &[ast::Arg]) -> Self {
        CallShape {
            count: arg_count(arguments),
            stray: pairs.iter().find_map(|item| match item {
                ast::Arg::Positional(node) if cannot_be_a_hash(node) => Some(node.text_range()),
                _ => None,
            }),
        }
    }

    #[must_use]
    pub fn stray_positional(&self) -> Option<rowan::TextRange> {
        self.stray
    }
}

/// Whether this value is definitely not a hash reference.
///
/// `f($options)` may well pass one, and `f(%options)` flattens into pairs;
/// only a literal, a quoted string or an array reference says for certain that
/// it is neither.
fn cannot_be_a_hash(node: &SyntaxNode) -> bool {
    matches!(
        node.node_kind(),
        NodeKind::LITERAL
            | NodeKind::Q_EXPR
            | NodeKind::QQ_EXPR
            | NodeKind::ANON_ARRAY
            | NodeKind::ANON_SUB_EXPR
            | NodeKind::UNDEF_EXPR
    )
}

/// How many arguments a call passes, or `None` when nobody can know.
///
/// Perl flattens lists into the argument list, so a count exists only when
/// every argument is exactly one value. That rules out an array, a hash, a
/// slice, a call (which may return a list), a `wantarray`-dependent ternary,
/// and a parenthesised list — which is most of what makes this safe.
#[must_use]
pub fn arg_count(arguments: &[SyntaxNode]) -> Option<usize> {
    let mut count = 0;
    for argument in arguments {
        if !is_single_value(argument) {
            return None;
        }
        count += 1;
    }
    Some(count)
}

fn is_single_value(node: &SyntaxNode) -> bool {
    match node.node_kind() {
        NodeKind::LITERAL
        | NodeKind::ANON_HASH
        | NodeKind::ANON_ARRAY
        | NodeKind::ANON_SUB_EXPR
        | NodeKind::REFERENCE_EXPR
        | NodeKind::UNDEF_EXPR
        | NodeKind::Q_EXPR
        | NodeKind::QQ_EXPR
        | NodeKind::QX_EXPR
        | NodeKind::HEREDOC_EXPR
        | NodeKind::M_EXPR
        | NodeKind::QR_EXPR
        | NodeKind::S_EXPR
        | NodeKind::TR_EXPR
        | NodeKind::FILE_TEST_EXPR
        | NodeKind::ARRAY_LAST_INDEX
        | NodeKind::POSTFIX_EXPR => true,

        NodeKind::SCALAR_VAR => true,

        // `$h{k}` and `$x->[0]` are one element; `@h{...}` is a slice.
        NodeKind::HASH_SUBSCRIPT_EXPR | NodeKind::ARRAY_SUBSCRIPT_EXPR => node
            .children()
            .next()
            .is_some_and(|base| yields_one_element(node, &base)),

        // A dereference is one value only when the sigil says so.
        NodeKind::DEREF_EXPR | NodeKind::BLOCK_DEREF_EXPR | NodeKind::POSTFIX_DEREF_EXPR => {
            ast::tokens(node).any(|token| {
                matches!(
                    token.token_kind(),
                    TokenKind::SCALAR_SIGIL | TokenKind::POSTFIX_DEREF_SCALAR
                )
            })
        }

        // `$a . $b`, `$a + 1`, `$a == $b` — an operator that is not `x` or a
        // range, both of which build lists.
        NodeKind::BINARY_EXPR => {
            let operator = ast::BinaryExpr::cast(node.clone()).and_then(|view| view.operator());
            !matches!(operator, Some(TokenKind::X_OP) | None)
        }
        NodeKind::RANGE_EXPR => false,

        NodeKind::PREFIX_EXPR => ast::tokens(node).next().is_some_and(|token| {
            matches!(
                token.token_kind(),
                TokenKind::MINUS
                    | TokenKind::PLUS
                    | TokenKind::LOGICAL_NOT
                    | TokenKind::BITWISE_NOT
                    | TokenKind::BACKSLASH
                    | TokenKind::NOT_KW
                    | TokenKind::INCREMENT
                    | TokenKind::DECREMENT
            )
        }),

        _ => false,
    }
}

/// Whether a subscript reads one element rather than a slice.
fn yields_one_element(subscript: &SyntaxNode, base: &SyntaxNode) -> bool {
    if ast::tokens(subscript).any(|token| token.token_kind() == TokenKind::ARROW) {
        return true;
    }
    Variable::cast(base.clone()).is_some_and(|variable| variable.sigil() == Sigil::Scalar)
}
