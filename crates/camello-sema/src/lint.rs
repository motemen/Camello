//! The lint rules (`docs/lint.md`): what is written does not do what writing
//! it meant.
//!
//! Every rule is a question about one file's tree, and all but one need
//! nothing else. `missing-strict` and `missing-warnings` also ask what a
//! `use` turns on, which for a project's own import module is a question about
//! another file; the caller answers it, so this module never sees the program.

use std::path::{Component, Path};

use camello_syntax::ast::{self, AstNode, Call, DeclKeyword, Literal, SubDef, VarDecl, Variable};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode, SyntaxToken, TokenExt, TokenKind};

use crate::diag::{Code, Diagnostic};
use crate::pragma::{self, Enables, Pragmas};

/// Every lint diagnostic for one file.
///
/// `enables` answers what a `use` of a module turns on; a module it cannot
/// answer for turns everything on here (LINT-5f).
#[must_use]
pub fn analyse(
    path: &Path,
    root: &SyntaxNode,
    enables: &dyn Fn(&str) -> Enables,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    pragmas(root, enables, &mut diagnostics);
    package_filename(path, root, &mut diagnostics);
    for node in root.descendants() {
        match node.node_kind() {
            NodeKind::VAR_DECL_STMT => conditional_declaration(&node, &mut diagnostics),
            NodeKind::CALL_EXPR | NodeKind::LIST_CALL_EXPR => {
                let call = Call::cast(node.clone()).expect("kind checked");
                match call.callee_name().as_deref() {
                    Some("return") => {
                        return_in_do(&call, &mut diagnostics);
                        return_sort(&call, &mut diagnostics);
                    }
                    Some("open") => {
                        two_arg_open(&call, &mut diagnostics);
                        bareword_handles(&call, 1, &mut diagnostics);
                    }
                    Some("opendir" | "sysopen" | "socket") => {
                        bareword_handles(&call, 1, &mut diagnostics);
                    }
                    Some("pipe" | "socketpair" | "accept") => {
                        bareword_handles(&call, 2, &mut diagnostics);
                    }
                    _ => {}
                }
            }
            NodeKind::FOREACH_HEADER => loop_iterator(&node, &mut diagnostics),
            NodeKind::EMPTY_STMT => double_semicolon(&node, &mut diagnostics),
            NodeKind::SUB_DEF => nested_sub(&node, &mut diagnostics),
            _ => {}
        }
    }
    for token in root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.token_kind() == TokenKind::NUMBER)
    {
        leading_zero(&token, &mut diagnostics);
    }
    diagnostics
}

// ----- LINT-1 -----

/// A `return` inside a `do` whose value is used.
fn return_in_do(call: &Call, into: &mut Vec<Diagnostic>) {
    let mut node = call.syntax().parent();
    while let Some(current) = node {
        match current.node_kind() {
            // The `return` leaves this, not anything around it (LINT-1c).
            NodeKind::SUB_DEF | NodeKind::ANON_SUB_EXPR => return,
            NodeKind::BLOCK_CALL_EXPR if callee_of(&current).as_deref() == Some("eval") => return,
            NodeKind::DO_BLOCK_EXPR if !discarded(&current) => {
                into.push(Diagnostic::new(
                    Code::ReturnInDoValue,
                    call.callee_range(),
                    "`return` inside a `do` block whose value is used returns from the \
                     enclosing sub, not from the `do`",
                ));
                return;
            }
            _ => {}
        }
        node = current.parent();
    }
}

/// Whether the value of an expression is thrown away (LINT-1a, LINT-1b).
fn discarded(node: &SyntaxNode) -> bool {
    let Some(parent) = node.parent() else {
        return true;
    };
    match parent.node_kind() {
        NodeKind::LIST_EXPR | NodeKind::PAREN_EXPR => discarded(&parent),
        NodeKind::EXPR_STMT => {
            // `do { ... } while ...` is a loop, and its value is nobody's.
            let modified = parent
                .children()
                .any(|child| child.node_kind() == NodeKind::STMT_MODIFIER);
            modified || !ends_a_sub(&parent)
        }
        NodeKind::BINARY_EXPR => {
            let binary = ast::BinaryExpr::cast(parent.clone()).expect("kind checked");
            if binary.right().as_ref() != Some(node) {
                return false;
            }
            match binary.operator() {
                Some(TokenKind::OR_KW | TokenKind::AND_KW | TokenKind::XOR_KW) => true,
                Some(TokenKind::LOGICAL_OR | TokenKind::LOGICAL_AND | TokenKind::DEFINED_OR) => {
                    discarded(&parent)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// Whether a statement is the last of a sub's body, whose value is the sub's.
fn ends_a_sub(statement: &SyntaxNode) -> bool {
    let Some(block) = statement.parent() else {
        return false;
    };
    if block.node_kind() != NodeKind::BLOCK {
        return false;
    }
    let is_body = block.parent().is_some_and(|owner| {
        matches!(
            owner.node_kind(),
            NodeKind::SUB_DEF | NodeKind::ANON_SUB_EXPR
        )
    });
    is_body
        && block
            .children()
            .filter(|child| child.node_kind() != NodeKind::EMPTY_STMT)
            .last()
            .as_ref()
            == Some(statement)
}

// ----- LINT-2 -----

fn conditional_declaration(statement: &SyntaxNode, into: &mut Vec<Diagnostic>) {
    let Some(modifier) = statement
        .children()
        .find(|child| child.node_kind() == NodeKind::STMT_MODIFIER)
    else {
        return;
    };
    let Some(declaration) = statement
        .descendants()
        .filter(|node| !modifier.text_range().contains_range(node.text_range()))
        .find_map(VarDecl::cast)
    else {
        return;
    };
    if declaration.keyword() != Some(DeclKeyword::My) {
        return;
    }
    let keyword = modifier
        .first_token()
        .map_or_else(String::new, |token| token.text().to_string());
    into.push(Diagnostic::new(
        Code::ConditionalDeclaration,
        declaration.syntax().text_range(),
        format!(
            "`my` with a statement modifier: what the variable holds when the `{keyword}` \
             does not run is undefined, and may be a value from an earlier call"
        ),
    ));
}

// ----- LINT-3 and LINT-4 -----

/// Everything a call is handed, its filehandle slot first.
fn operands(call: &Call) -> Vec<SyntaxNode> {
    call.filehandle()
        .map(|handle| handle.syntax().clone())
        .into_iter()
        .chain(call.args())
        .collect()
}

fn two_arg_open(call: &Call, into: &mut Vec<Diagnostic>) {
    let operands = operands(call);
    if operands.len() != 2 {
        return;
    }
    let standard = Literal::cast(operands[1].clone())
        .and_then(|literal| literal.as_string())
        .is_some_and(|text| text == "-" || text == ">-");
    if standard {
        return;
    }
    into.push(Diagnostic::new(
        Code::TwoArgOpen,
        call.callee_range(),
        "two-argument `open` reads the mode from the file name; give the mode as an \
         argument of its own",
    ));
}

const STANDARD_HANDLES: &[&str] = &["STDIN", "STDOUT", "STDERR", "DATA", "ARGV", "ARGVOUT"];

fn bareword_handles(call: &Call, slots: usize, into: &mut Vec<Diagnostic>) {
    for operand in operands(call).into_iter().take(slots) {
        let Some(name) = bareword(&operand) else {
            continue;
        };
        if STANDARD_HANDLES.contains(&name.as_str()) {
            continue;
        }
        into.push(
            Diagnostic::new(
                Code::BarewordFilehandle,
                operand.text_range(),
                format!(
                    "bareword filehandle `{name}` is a package global; use a lexical \
                     (`my ${}`)",
                    name.to_lowercase()
                ),
            )
            .about(name),
        );
    }
}

/// The name, where an operand is a bare word and nothing else.
fn bareword(operand: &SyntaxNode) -> Option<String> {
    match operand.node_kind() {
        NodeKind::FILEHANDLE => {
            let mut tokens = ast::tokens(operand);
            let token = tokens.next()?;
            (token.token_kind() == TokenKind::IDENT && tokens.next().is_none())
                .then(|| token.text().to_string())
        }
        NodeKind::LIST_CALL_EXPR => {
            let call = Call::cast(operand.clone())?;
            (operand.children().count() == 1)
                .then(|| call.callee_name())
                .flatten()
        }
        _ => None,
    }
}

// ----- LINT-5 -----

/// `missing-strict` and `missing-warnings`: what is on when the first
/// statement that is code runs.
fn pragmas(root: &SyntaxNode, enables: &dyn Fn(&str) -> Enables, into: &mut Vec<Diagnostic>) {
    let mut on = shebang(root);
    let Some(first) = first_code(root, enables, &mut on) else {
        return;
    };
    let range = first
        .first_token()
        .map_or_else(|| first.text_range(), |token| token.text_range());
    if !on.strict {
        into.push(Diagnostic::new(
            Code::MissingStrict,
            range,
            "this code runs without `use strict`: nothing before it turns `strict` on",
        ));
    }
    if !on.warnings {
        into.push(Diagnostic::new(
            Code::MissingWarnings,
            range,
            "this code runs without `use warnings`: nothing before it turns `warnings` on",
        ));
    }
}

/// `#!/usr/bin/perl -w` turns warnings on for the whole program.
fn shebang(root: &SyntaxNode) -> Pragmas {
    let Some(token) = root.first_token() else {
        return Pragmas::NONE;
    };
    let text = token.text();
    if token.token_kind() != TokenKind::COMMENT || !text.starts_with("#!") {
        return Pragmas::NONE;
    }
    let Some(position) = text.find("perl") else {
        return Pragmas::NONE;
    };
    let warns = text[position..]
        .split_whitespace()
        .skip(1)
        .any(|flag| flag.starts_with('-') && !flag.starts_with("--") && flag.contains('w'));
    Pragmas {
        strict: false,
        warnings: warns,
    }
}

/// The first statement that is code (LINT-5b, LINT-5c), with `on` updated by
/// every `use` above it.
fn first_code(
    parent: &SyntaxNode,
    enables: &dyn Fn(&str) -> Enables,
    on: &mut Pragmas,
) -> Option<SyntaxNode> {
    for statement in parent.children() {
        match statement.node_kind() {
            NodeKind::USE_STMT => {
                let use_statement = ast::UseStmt::cast(statement.clone()).expect("kind checked");
                let turned = match use_statement.module() {
                    Some(module) => match enables(&module) {
                        Enables::Known(pragmas) => pragmas,
                        Enables::Unknown => Pragmas::BOTH,
                    },
                    None => pragma::by_version(&statement),
                };
                *on = on.union(turned);
            }
            NodeKind::NO_STMT | NodeKind::EMPTY_STMT | NodeKind::POD | NodeKind::DATA_SECTION => {}
            _ if is_prelude(&statement) => {}
            // `{ use 5.006; }` asks for a version and nothing else. What the
            // block turns on ends with it, so it is read with a copy.
            NodeKind::BLOCK_STMT => {
                let mut inner = *on;
                let block = statement
                    .children()
                    .find(|child| child.node_kind() == NodeKind::BLOCK)?;
                if let Some(found) = first_code(&block, enables, &mut inner) {
                    *on = inner;
                    return Some(found);
                }
            }
            NodeKind::PACKAGE_STMT => {
                let package = ast::PackageStmt::cast(statement.clone()).expect("kind checked");
                if let Some(block) = package.block() {
                    if let Some(found) = first_code(block.syntax(), enables, on) {
                        return Some(found);
                    }
                }
            }
            _ => return Some(statement),
        }
    }
    None
}

/// `require 5.006`, `require Exporter`, `our $VERSION = ...` and `1;`:
/// statements a module opens
/// or closes with that do the same thing under `strict` as without it
/// (LINT-5b).
fn is_prelude(statement: &SyntaxNode) -> bool {
    if !matches!(
        statement.node_kind(),
        NodeKind::EXPR_STMT | NodeKind::VAR_DECL_STMT
    ) {
        return false;
    }
    if statement
        .children()
        .any(|child| child.node_kind() == NodeKind::STMT_MODIFIER)
    {
        return false;
    }
    let Some(expression) = statement
        .children()
        .find(|child| child.node_kind() == NodeKind::LIST_EXPR)
        .and_then(|list| {
            let mut children = list.children();
            match (children.next(), children.next()) {
                (Some(only), None) => Some(only),
                _ => None,
            }
        })
    else {
        return false;
    };
    // `1;`, the true value a module ends with.
    if expression.node_kind() == NodeKind::LITERAL {
        return true;
    }
    if expression.node_kind() == NodeKind::REQUIRE_EXPR {
        let mut operands = expression.children();
        let version = match (operands.next(), operands.next()) {
            // `require Exporter`, which loads a module the way `use` does.
            (Some(only), None) if only.node_kind() == NodeKind::SUB_NAME => true,
            (Some(only), None) => Literal::cast(only).is_some_and(|literal| literal.is_number()),
            (None, None) => {
                ast::tokens(&expression).any(|token| token.token_kind() == TokenKind::VERSION)
            }
            _ => false,
        };
        return version;
    }
    let Some(assignment) = ast::AssignExpr::cast(expression) else {
        return false;
    };
    let Some(target) = assignment.target() else {
        return false;
    };
    let text = target.text().to_string();
    let name = text.trim().trim_start_matches("our").trim();
    name == "$VERSION" || (name.starts_with('$') && name.ends_with("::VERSION"))
}

// ----- LINT-6 -----

fn return_sort(call: &Call, into: &mut Vec<Diagnostic>) {
    let Some(first) = call.args().into_iter().next() else {
        return;
    };
    if !matches!(
        first.node_kind(),
        NodeKind::LIST_CALL_EXPR | NodeKind::CALL_EXPR | NodeKind::BLOCK_CALL_EXPR
    ) || callee_of(&first).as_deref() != Some("sort")
    {
        return;
    }
    let range = first
        .children()
        .find(|child| child.node_kind() == NodeKind::SUB_NAME)
        .map_or_else(|| first.text_range(), |name| name.text_range());
    into.push(Diagnostic::new(
        Code::ReturnSort,
        range,
        "`return sort` in scalar context is undefined; sort into an array and return that",
    ));
}

// ----- LINT-7 -----

fn loop_iterator(header: &SyntaxNode, into: &mut Vec<Diagnostic>) {
    let Some(first) = header.children().next() else {
        return;
    };
    let Some(variable) = Variable::cast(first) else {
        return;
    };
    if variable.name().as_deref() == Some("_") {
        return;
    }
    let display = variable.display();
    into.push(
        Diagnostic::new(
            Code::NonLexicalLoopIterator,
            variable.range(),
            format!(
                "loop variable `{display}` is not declared by the loop: perl localises it, \
                 so its value inside the loop is gone after it; write `for my {display}`"
            ),
        )
        .about(display),
    );
}

// ----- LINT-8 -----

fn package_filename(path: &Path, root: &SyntaxNode, into: &mut Vec<Diagnostic>) {
    if path.extension().is_none_or(|extension| extension != "pm") {
        return;
    }
    let packages: Vec<(ast::PackageStmt, String)> = root
        .descendants()
        .filter_map(ast::PackageStmt::cast)
        .filter_map(|package| package.name().map(|name| (package, name)))
        .collect();
    let Some((package, name)) = packages.first() else {
        return;
    };
    if name == "main" {
        return;
    }
    let mut parts: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if let Some(last) = parts.last_mut() {
        *last = last.trim_end_matches(".pm").to_string();
    }
    let names_the_path = |name: &str| {
        name.rsplit("::")
            .zip(parts.iter().rev())
            .all(|(wanted, part)| wanted == part)
    };
    // A helper package may come first; what `use` needs is that the file
    // defines the one its path names somewhere.
    if packages.iter().any(|(_, name)| names_the_path(name)) {
        return;
    }
    let name = name.clone();
    let range = package
        .syntax()
        .children()
        .find(|child| child.node_kind() == NodeKind::SUB_NAME)
        .map_or_else(|| package.syntax().text_range(), |name| name.text_range());
    into.push(
        Diagnostic::new(
            Code::PackageFilenameMismatch,
            range,
            format!(
                "package `{name}` in `{}`: `use {name}` would not find this file",
                path.display()
            ),
        )
        .about(name),
    );
}

// ----- LINT-9 -----

fn double_semicolon(empty: &SyntaxNode, into: &mut Vec<Diagnostic>) {
    let Some(previous) = empty.prev_sibling() else {
        return;
    };
    let ends_in_semicolon = previous
        .last_token()
        .is_some_and(|token| token.token_kind() == TokenKind::SEMICOLON);
    if !ends_in_semicolon {
        return;
    }
    into.push(Diagnostic::new(
        Code::DoubleSemicolon,
        empty.text_range(),
        "`;;`: an empty statement after one already ended",
    ));
}

// ----- LINT-10 -----

fn nested_sub(node: &SyntaxNode, into: &mut Vec<Diagnostic>) {
    let definition = SubDef::cast(node.clone()).expect("kind checked");
    if definition.is_forward_declaration() {
        return;
    }
    let inside_a_sub = node.ancestors().skip(1).any(|ancestor| {
        matches!(
            ancestor.node_kind(),
            NodeKind::SUB_DEF | NodeKind::ANON_SUB_EXPR
        )
    });
    if !inside_a_sub {
        return;
    }
    let Some(name) = definition.name() else {
        return;
    };
    let text = name.text();
    into.push(
        Diagnostic::new(
            Code::NestedNamedSub,
            name.syntax().text_range(),
            format!(
                "named sub `{text}` inside another sub is created once, and sees the outer \
                 sub's variables from its first call only"
            ),
        )
        .about(text),
    );
}

// ----- LINT-11 -----

/// What a call is named when its arguments are permissions, written in octal
/// on purpose: the builtins, and the wrappers that carry their names —
/// `_chmod`, `File::Path::mkpath`, `make_path` (LINT-11b).
const OCTAL_ON_PURPOSE: &[&str] = &[
    "chmod",
    "mkdir",
    "mkpath",
    "make_path",
    "umask",
    "sysopen",
    "dbmopen",
];

/// The operators whose operands are bit masks, which octal is the way to
/// write.
const BITWISE: &[TokenKind] = &[
    TokenKind::BITWISE_AND,
    TokenKind::BITWISE_OR,
    TokenKind::BITWISE_XOR,
    TokenKind::BITWISE_NOT,
    TokenKind::SHIFT_LEFT,
    TokenKind::SHIFT_RIGHT,
    TokenKind::BITWISE_AND_EQ,
    TokenKind::BITWISE_OR_EQ,
    TokenKind::BITWISE_XOR_EQ,
    TokenKind::SHIFT_LEFT_EQ,
    TokenKind::SHIFT_RIGHT_EQ,
];

/// Whether a number sits where octal is what anyone would write.
fn octal_on_purpose(token: &SyntaxToken) -> bool {
    let in_permission_call = token
        .parent_ancestors()
        .filter_map(Call::cast)
        .filter_map(|call| call.callee_name())
        .any(|name| {
            let last = name.rsplit("::").next().unwrap_or(&name);
            OCTAL_ON_PURPOSE.contains(&last.trim_start_matches('_'))
        });
    if in_permission_call {
        return true;
    }
    // An operand of a bitwise operator, directly or through parentheses.
    let mut node = token.parent();
    while let Some(current) = node {
        match current.node_kind() {
            NodeKind::LITERAL
            | NodeKind::PAREN_EXPR
            | NodeKind::LIST_EXPR
            | NodeKind::TERNARY_EXPR => {
                node = current.parent();
            }
            NodeKind::BINARY_EXPR | NodeKind::ASSIGN_EXPR | NodeKind::PREFIX_EXPR => {
                return ast::tokens(&current)
                    .any(|operator| BITWISE.contains(&operator.token_kind()));
            }
            _ => return false,
        }
    }
    false
}

fn leading_zero(token: &SyntaxToken, into: &mut Vec<Diagnostic>) {
    let text = token.text();
    let mut chars = text.chars();
    if chars.next() != Some('0') {
        return;
    }
    if !chars
        .next()
        .is_some_and(|second| second.is_ascii_digit() || second == '_')
    {
        return;
    }
    if text.contains(['.', 'e', 'E']) {
        return;
    }
    if octal_on_purpose(token) {
        return;
    }
    let digits = text.replace('_', "");
    let reading = match u64::from_str_radix(&digits, 8) {
        Ok(value) => format!("{value} in decimal"),
        Err(_) => "not a number perl accepts".to_string(),
    };
    into.push(Diagnostic::new(
        Code::LeadingZero,
        token.text_range(),
        format!("`{text}` is octal because of its leading zero ({reading})"),
    ));
}

/// The name a call-shaped node calls, `BLOCK_CALL_EXPR` included.
fn callee_of(node: &SyntaxNode) -> Option<String> {
    node.children()
        .find(|child| child.node_kind() == NodeKind::SUB_NAME)
        .and_then(ast::SubName::cast)
        .map(|name| name.text())
}
