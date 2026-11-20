//! The lint rules (`docs/lint.md`): what is written does not do what writing
//! it meant.
//!
//! Every rule is a question about one file's tree, and all but one need
//! nothing else. `missing-strict` and `missing-warnings` also ask what a
//! `use` turns on, which for a project's own import module is a question about
//! another file; the caller answers it, so this module never sees the program.

use std::path::{Component, Path};

use camello_syntax::ast::{self, AstNode, Call, DeclKeyword, Literal, SubDef, VarDecl, Variable};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode, TokenExt, TokenKind};

use crate::diag::{Code, Diagnostic};
use crate::pragma::{self, Enables, Pragmas};
use crate::scope::{BindingKind, ScopeReport};

/// Every lint diagnostic for one file.
///
/// `enables` answers what a `use` of a module turns on; a module it cannot
/// answer for turns everything on here (LINT-5f).
#[must_use]
pub fn analyse(
    path: &Path,
    root: &SyntaxNode,
    enables: &dyn Fn(&str) -> Enables,
    scope: &ScopeReport,
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
            NodeKind::SUB_DEF => nested_sub(&node, scope, &mut diagnostics),
            NodeKind::ASSIGN_EXPR => augmented_declaration(&node, &mut diagnostics),
            NodeKind::BLOCK_CALL_EXPR => mutating_list_function(&node, &mut diagnostics),
            _ => {}
        }
    }
    diagnostics
}

// ----- LINT-1 -----

/// A `return` in a statement of a `do` block whose value is used.
///
/// Only a statement of the `do`'s own (LINT-1a): a `return` in an `if`, a
/// loop or a bare block inside it is an early exit the writer chose a branch
/// for, which is what Perl::Critic's ProhibitReturnInDoBlock counts too. So
/// the block that matters is the nearest one around the `return`, and that
/// also settles a `sub`, an `eval { }` or a `first { }` in between (LINT-1c):
/// the nearest block is theirs.
fn return_in_do(call: &Call, into: &mut Vec<Diagnostic>) {
    // A function the parser did not know takes a block, `scraper { ... }`:
    // its block reads as a hash, which never holds a `return`.
    let Some(block) = call
        .syntax()
        .ancestors()
        .skip(1)
        .take_while(|ancestor| ancestor.node_kind() != NodeKind::ANON_HASH)
        .find(|ancestor| ancestor.node_kind() == NodeKind::BLOCK)
    else {
        return;
    };
    let Some(construct) = block.parent() else {
        return;
    };
    // A `do` with no `do` written is the parser's reading of `scraper { }`.
    let is_do = construct.node_kind() == NodeKind::DO_BLOCK_EXPR
        && ast::tokens(&construct).any(|token| token.token_kind() == TokenKind::DO_KW);
    if is_do && !discarded(&construct) {
        into.push(Diagnostic::new(
            Code::ReturnInDoValue,
            call.callee_range(),
            "`return` inside a `do` block whose value is used returns from the \
             enclosing sub, not from the `do`",
        ));
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
            modified || !gives_its_block_value(&parent)
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

/// Whether a statement is the last of a block whose value is used: a sub's
/// body, a `do` whose value is used, or a branch of an `if` or a bare block
/// that is itself the last of one (LINT-1a).
fn gives_its_block_value(statement: &SyntaxNode) -> bool {
    let Some(block) = statement.parent() else {
        return false;
    };
    if block.node_kind() != NodeKind::BLOCK {
        return false;
    }
    let last = block
        .children()
        .filter(|child| child.node_kind() != NodeKind::EMPTY_STMT)
        .last();
    if last.as_ref() != Some(statement) {
        return false;
    }
    let Some(owner) = block.parent() else {
        return false;
    };
    match owner.node_kind() {
        NodeKind::SUB_DEF | NodeKind::ANON_SUB_EXPR => true,
        NodeKind::DO_BLOCK_EXPR => {
            ast::tokens(&owner).any(|token| token.token_kind() == TokenKind::DO_KW)
                && !discarded(&owner)
        }
        NodeKind::IF_STMT | NodeKind::BLOCK_STMT => gives_its_block_value(&owner),
        NodeKind::ELSIF_CLAUSE | NodeKind::ELSE_CLAUSE => owner
            .parent()
            .is_some_and(|statement| gives_its_block_value(&statement)),
        _ => false,
    }
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

/// `missing-strict` and `missing-warnings`: the first statement that is code
/// and runs without each.
fn pragmas(root: &SyntaxNode, enables: &dyn Fn(&str) -> Enables, into: &mut Vec<Diagnostic>) {
    let mut uncovered = Uncovered::default();
    uncovered.walk(root, enables, shebang(root));
    let start = |statement: &SyntaxNode| {
        statement
            .first_token()
            .map_or_else(|| statement.text_range(), |token| token.text_range())
    };
    if let Some(statement) = &uncovered.strict {
        into.push(Diagnostic::new(
            Code::MissingStrict,
            start(statement),
            "this code runs without `use strict`: nothing before it turns `strict` on",
        ));
    }
    if let Some(statement) = &uncovered.warnings {
        into.push(Diagnostic::new(
            Code::MissingWarnings,
            start(statement),
            "this code runs without `use warnings`: nothing before it turns `warnings` on",
        ));
    }
}

/// The first statement that is code (LINT-5b, LINT-5c) and runs without
/// `strict`, and the first without `warnings`.
#[derive(Default)]
struct Uncovered {
    strict: Option<SyntaxNode>,
    warnings: Option<SyntaxNode>,
}

impl Uncovered {
    fn done(&self) -> bool {
        self.strict.is_some() && self.warnings.is_some()
    }

    /// Reads the statements of `parent` in order, with `on` updated by every
    /// `use` among them.
    fn walk(&mut self, parent: &SyntaxNode, enables: &dyn Fn(&str) -> Enables, mut on: Pragmas) {
        for statement in parent.children() {
            if self.done() {
                return;
            }
            match statement.node_kind() {
                NodeKind::USE_STMT => {
                    let use_statement =
                        ast::UseStmt::cast(statement.clone()).expect("kind checked");
                    let turned = match use_statement.module() {
                        Some(_) if pragma::imports_nothing(&use_statement) => Pragmas::NONE,
                        Some(module) => match enables(&module) {
                            Enables::Known(pragmas) => pragmas,
                            Enables::Unknown => Pragmas::BOTH,
                        },
                        None => pragma::by_version(&statement),
                    };
                    on = on.union(turned);
                }
                NodeKind::NO_STMT
                | NodeKind::EMPTY_STMT
                | NodeKind::POD
                | NodeKind::DATA_SECTION => {}
                _ if is_prelude(&statement) => {}
                // `{ use 5.006; }` and `package Foo { use strict; }`: what the
                // block turns on ends with it, so it is read with a copy.
                NodeKind::BLOCK_STMT => {
                    if let Some(block) = statement
                        .children()
                        .find(|child| child.node_kind() == NodeKind::BLOCK)
                    {
                        self.walk(&block, enables, on);
                    }
                }
                NodeKind::PACKAGE_STMT => {
                    let package = ast::PackageStmt::cast(statement.clone()).expect("kind checked");
                    if let Some(block) = package.block() {
                        self.walk(block.syntax(), enables, on);
                    }
                }
                _ => {
                    if !on.strict && self.strict.is_none() {
                        self.strict = Some(statement.clone());
                    }
                    if !on.warnings && self.warnings.is_none() {
                        self.warnings = Some(statement.clone());
                    }
                    // Nothing at this level turns either off again.
                    if on.strict && on.warnings {
                        return;
                    }
                }
            }
        }
    }
}

/// `#!/usr/bin/perl -w` and `-W` turn warnings on for the whole program, and
/// `-X` off again (LINT-5a).
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
    let mut warns = false;
    'words: for word in text[position..].split_whitespace().skip(1) {
        let Some(switches) = word.strip_prefix('-') else {
            break;
        };
        if switches.is_empty() || switches.starts_with('-') {
            break;
        }
        let mut chars = switches.chars().peekable();
        while let Some(switch) = chars.next() {
            match switch {
                'w' | 'W' => warns = true,
                'X' => warns = false,
                // `-l`, `-0` take an octal number and run on to the next
                // switch: `-l0w`, `-0777w`.
                'l' | '0' => {
                    while chars.peek().is_some_and(char::is_ascii_digit) {
                        chars.next();
                    }
                }
                'a' | 'c' | 'f' | 'n' | 'p' | 's' | 'S' | 't' | 'T' | 'u' | 'U' | 'v' => {}
                // `-I/home/www/lib`, `-Mstrict`, `-e ...`: the rest of the
                // word is the switch's argument, not more switches.
                _ => continue 'words,
            }
        }
    }
    Pragmas {
        strict: false,
        warnings: warns,
    }
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
    // `package\n    DBI;` is how a file says "do not index me": PAUSE reads
    // a package's name off the line of its keyword, and a file that moves it
    // to the next one is not meant to be found by `use` (LINT-8c).
    if packages.iter().any(|(package, _)| {
        package
            .syntax()
            .children()
            .find(|child| child.node_kind() == NodeKind::SUB_NAME)
            .is_some_and(|name| {
                let start = name.text_range().start() - package.syntax().text_range().start();
                package.syntax().text().to_string()[..usize::from(start)].contains('\n')
            })
    }) {
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
    // `XML/SAX/PurePerl/DTDDecls.pm` holding `package XML::SAX::PurePerl` is
    // a part of that package's implementation, split out and `require`d by
    // path (LINT-8c).
    let names_an_ancestor = |name: &str| {
        (1..parts.len()).any(|dropped| {
            let kept = &parts[..parts.len() - dropped];
            name.rsplit("::")
                .zip(kept.iter().rev())
                .all(|(wanted, part)| wanted == part)
                && name.split("::").count() <= kept.len()
        })
    };
    if packages.iter().any(|(_, name)| names_an_ancestor(name)) {
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

fn nested_sub(node: &SyntaxNode, scope: &ScopeReport, into: &mut Vec<Diagnostic>) {
    let definition = SubDef::cast(node.clone()).expect("kind checked");
    if definition.is_forward_declaration() {
        return;
    }
    // `my sub` and `state sub` are made where they are written, each time
    // (LINT-10b).
    let lexical = ast::tokens(node)
        .any(|token| matches!(token.token_kind(), TokenKind::MY_KW | TokenKind::STATE_KW));
    if lexical {
        return;
    }
    let Some(outer) = node.ancestors().skip(1).find(|ancestor| {
        matches!(
            ancestor.node_kind(),
            NodeKind::SUB_DEF | NodeKind::ANON_SUB_EXPR
        )
    }) else {
        return;
    };
    // What perl warns "will not stay shared" about: a variable the outer sub
    // makes afresh on each call — a `my`, a parameter, a `for my`, a `catch`
    // — that the inner one reads (LINT-10a). A `state` or an `our` is one
    // variable however many calls there are, and a name the inner sub
    // declares again resolves to its own.
    let inner = node.text_range();
    let outer = outer.text_range();
    let Some(shared) = scope
        .references
        .iter()
        .filter(|reference| inner.contains_range(reference.range))
        .map(|reference| &scope.bindings[reference.binding])
        .find(|binding| {
            matches!(
                binding.kind,
                BindingKind::My | BindingKind::Param | BindingKind::Caught
            ) && outer.contains_range(binding.range)
                && !inner.contains_range(binding.range)
        })
    else {
        return;
    };
    let Some(name) = definition.name() else {
        return;
    };
    let text = name.text();
    let variable = shared.display();
    into.push(
        Diagnostic::new(
            Code::NestedNamedSub,
            name.syntax().text_range(),
            format!(
                "named sub `{text}` inside another sub is created once, and sees the outer \
                 sub's `{variable}` from its first call only"
            ),
        )
        .about(text),
    );
}

// ----- LINT-11 -----

fn augmented_declaration(node: &SyntaxNode, into: &mut Vec<Diagnostic>) {
    let assignment = ast::AssignExpr::cast(node.clone()).expect("kind checked");
    if assignment.is_plain() {
        return;
    }
    let Some(declaration) = assignment.target().and_then(VarDecl::cast) else {
        return;
    };
    let Some(operator) = ast::tokens(node)
        .find(|token| token.token_kind().is_assignment_op())
        .map(|token| token.text().to_string())
    else {
        return;
    };
    // `our` names the package variable that is already there, and `state`
    // keeps its value from the call before, so for both the operator has a
    // value to combine with (LINT-11b).
    let message = match declaration.keyword() {
        Some(DeclKeyword::My) => format!(
            "`my ... {operator}` combines with a variable that is new and undefined; \
             write `=`, or drop `my` to change the variable already there"
        ),
        Some(DeclKeyword::Local) => format!(
            "`local` empties the variable before `{operator}` runs, so the value it had \
             is not part of the result"
        ),
        _ => return,
    };
    into.push(Diagnostic::new(
        Code::AugmentedAssignmentInDeclaration,
        node.text_range(),
        message,
    ));
}

// ----- LINT-12 -----

/// The functions whose block sees each element of the list as `$_` (LINT-12a).
const LIST_FUNCTIONS: &[&str] = &["map", "grep", "first", "any", "all", "none", "notall"];

fn mutating_list_function(node: &SyntaxNode, into: &mut Vec<Diagnostic>) {
    let Some(name) = callee_of(node).filter(|name| LIST_FUNCTIONS.contains(&name.as_str())) else {
        return;
    };
    // `map { s/a/b/ } @lines;` with its value thrown away is a loop written
    // with `map`, and changing the array is what it is for (LINT-12b).
    if discarded(node) {
        return;
    }
    let Some(block) = node
        .children()
        .find(|child| child.node_kind() == NodeKind::BLOCK)
    else {
        return;
    };
    let Some(array) = node
        .children()
        .skip_while(|child| child.node_kind() != NodeKind::BLOCK)
        .skip(1)
        .flat_map(|list| {
            if ast::Args::is_list(&list) {
                ast::Args::elements(&list)
            } else {
                vec![list]
            }
        })
        .find(holds_its_elements)
    else {
        return;
    };
    let array = array.text().to_string();
    if assigned_back(node, &array) {
        return;
    }
    let Some(mutation) = first_mutation(&block) else {
        return;
    };
    // `scalar grep { s/^>// } @$body`: the change is the block's value, the
    // count of what it changed, and changing the array is the point
    // (LINT-12b). `map` is not a test, and its value is what it builds.
    if name != "map" && is_the_value_of(&mutation, &block) {
        return;
    }
    into.push(
        Diagnostic::new(
            Code::MutatingListFunction,
            mutation.text_range(),
            format!(
                "`{name}` sees each element of `{array}` as `$_`, so changing `$_` here \
                 changes `{array}` too"
            ),
        )
        .about(name),
    );
}

/// Whether what the list function builds is assigned to the array it reads,
/// through whatever `map`, `grep` or `sort` it passes on the way:
/// `@a = map { s/x//; $_ } @a` changes `@a` in place and then replaces it
/// with the same values (LINT-12b).
fn assigned_back(node: &SyntaxNode, array: &str) -> bool {
    let mut current = node.clone();
    while let Some(parent) = current.parent() {
        match parent.node_kind() {
            NodeKind::LIST_EXPR | NodeKind::PAREN_EXPR | NodeKind::BLOCK_CALL_EXPR => {
                current = parent;
            }
            NodeKind::ASSIGN_EXPR => {
                let assignment = ast::AssignExpr::cast(parent).expect("kind checked");
                return assignment.value().as_ref() == Some(&current)
                    && assignment
                        .target()
                        .is_some_and(|target| target.text().to_string().trim() == array);
            }
            _ => return false,
        }
    }
    false
}

/// Whether an expression is the whole of the last statement of a block.
fn is_the_value_of(expression: &SyntaxNode, block: &SyntaxNode) -> bool {
    let Some(last) = block
        .children()
        .filter(|child| child.node_kind() != NodeKind::EMPTY_STMT)
        .last()
    else {
        return false;
    };
    let mut node = expression.clone();
    // `$_ =~ s///` is the statement, not the `s///` alone.
    if node
        .parent()
        .is_some_and(|parent| parent.node_kind() == NodeKind::MATCH_EXPR)
    {
        node = node.parent().expect("checked");
    }
    while let Some(parent) = node.parent() {
        if parent == last {
            return true;
        }
        if parent.node_kind() != NodeKind::LIST_EXPR {
            return false;
        }
        node = parent;
    }
    false
}

/// Whether a list element hands the block the elements of a variable itself,
/// not copies: an array, a dereferenced one, or the values of a hash
/// (LINT-12a). What a call returns is a fresh value — `split`, `<$fh>`,
/// `keys` — and changing it changes nothing else.
fn holds_its_elements(element: &SyntaxNode) -> bool {
    match element.node_kind() {
        NodeKind::ARRAY_VAR => true,
        NodeKind::DEREF_EXPR | NodeKind::BLOCK_DEREF_EXPR => element
            .first_token()
            .is_some_and(|token| token.token_kind() == TokenKind::ARRAY_SIGIL),
        NodeKind::POSTFIX_DEREF_EXPR => {
            ast::tokens(element).any(|token| token.token_kind() == TokenKind::POSTFIX_DEREF_ARRAY)
        }
        NodeKind::CALL_EXPR | NodeKind::LIST_CALL_EXPR => {
            callee_of(element).as_deref() == Some("values")
        }
        _ => false,
    }
}

/// The first expression in a block that changes `$_` (LINT-12a), not looking
/// into what binds a `$_` of its own (LINT-12c).
fn first_mutation(block: &SyntaxNode) -> Option<SyntaxNode> {
    let mut mutations = Vec::new();
    let mut stack: Vec<SyntaxNode> = block.children().collect();
    stack.reverse();
    while let Some(node) = stack.pop() {
        if binds_its_own_topic(&node) {
            continue;
        }
        // `local $_` gives the block a `$_` of its own, and the element is
        // out of reach from there on.
        if VarDecl::cast(node.clone()).is_some_and(|declaration| {
            declaration
                .syntax()
                .descendants()
                .any(|variable| is_topic(&variable))
        }) {
            return None;
        }
        if changes_topic(&node) {
            mutations.push(node);
            continue;
        }
        let mut children: Vec<SyntaxNode> = node.children().collect();
        children.reverse();
        stack.extend(children);
    }
    mutations.into_iter().next()
}

fn binds_its_own_topic(node: &SyntaxNode) -> bool {
    match node.node_kind() {
        NodeKind::BLOCK_CALL_EXPR
        | NodeKind::ANON_SUB_EXPR
        | NodeKind::SUB_DEF
        | NodeKind::LOOP_STMT
        | NodeKind::GIVEN_STMT => true,
        // `s/a/b/ for @other` loops over `@other`.
        NodeKind::EXPR_STMT => node
            .children()
            .filter(|child| child.node_kind() == NodeKind::STMT_MODIFIER)
            .filter_map(|modifier| modifier.first_token())
            .any(|token| {
                matches!(
                    token.token_kind(),
                    TokenKind::FOR_KW | TokenKind::FOREACH_KW
                )
            }),
        _ => false,
    }
}

fn is_topic(node: &SyntaxNode) -> bool {
    node.node_kind() == NodeKind::SCALAR_VAR
        && Variable::cast(node.clone())
            .and_then(|variable| variable.name())
            .as_deref()
            == Some("_")
}

/// Whether an expression changes `$_`: an `s///` or a `tr///` that changes
/// what it is bound to, an assignment, `++` or `--`, `chomp` or `chop`.
fn changes_topic(node: &SyntaxNode) -> bool {
    match node.node_kind() {
        NodeKind::S_EXPR => !flags(node).contains('r') && bound_to_topic(node),
        NodeKind::TR_EXPR => {
            let flags = flags(node);
            let replaces = ast::tokens(node).any(|token| {
                token.token_kind() == TokenKind::TR_REPLACEMENT_LIST && !token.text().is_empty()
            });
            // `tr/a//` counts and changes nothing.
            !flags.contains('r') && (replaces || flags.contains('d')) && bound_to_topic(node)
        }
        NodeKind::ASSIGN_EXPR => ast::AssignExpr::cast(node.clone())
            .and_then(|assignment| assignment.target())
            .is_some_and(|target| is_topic(&target)),
        NodeKind::PREFIX_EXPR | NodeKind::POSTFIX_EXPR => {
            ast::tokens(node).any(|token| {
                matches!(
                    token.token_kind(),
                    TokenKind::INCREMENT | TokenKind::DECREMENT
                )
            }) && node
                .children()
                .next()
                .is_some_and(|operand| is_topic(&operand))
        }
        NodeKind::CALL_EXPR | NodeKind::LIST_CALL_EXPR => {
            let Some(call) = Call::cast(node.clone()) else {
                return false;
            };
            if !matches!(call.callee_name().as_deref(), Some("chomp" | "chop")) {
                return false;
            }
            let args = call.args();
            args.is_empty() || (args.len() == 1 && is_topic(&args[0]))
        }
        _ => false,
    }
}

fn flags(node: &SyntaxNode) -> String {
    ast::tokens(node)
        .filter(|token| token.token_kind() == TokenKind::REGEX_FLAGS)
        .map(|token| token.text().to_string())
        .collect()
}

/// Whether an `s///` or `tr///` works on `$_`: written alone, or bound to it
/// with `=~`.
fn bound_to_topic(node: &SyntaxNode) -> bool {
    match node.parent() {
        Some(parent) if parent.node_kind() == NodeKind::MATCH_EXPR => parent
            .children()
            .next()
            .is_some_and(|target| is_topic(&target)),
        _ => true,
    }
}

/// The name a call-shaped node calls, `BLOCK_CALL_EXPR` included.
fn callee_of(node: &SyntaxNode) -> Option<String> {
    node.children()
        .find(|child| child.node_kind() == NodeKind::SUB_NAME)
        .and_then(ast::SubName::cast)
        .map(|name| name.text())
}
