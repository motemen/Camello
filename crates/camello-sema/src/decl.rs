//! The declaration pass (`docs/typecheck.md`, "Data flow").
//!
//! What a file *declares*, read without opening a single sub body. That
//! restriction is the design's and it is what makes dependencies cheap: a body
//! can only use a declaration, never make one another file could see, so the
//! program graph is complete after this pass and editing a body invalidates
//! one sub and nothing else.
//!
//! The one thing a body is read for is the sub's own parameter list, which is
//! written *inside* it when the sub uses `args` or unpacks `@_`. That is still
//! a declaration about the sub, not about the program, and it is read from the
//! body's leading statements rather than from the body.

use std::collections::HashMap;

use camello_syntax::ast::{
    self, AnonHash, Args, AstNode, DeclKeyword, Literal, Sigil, SubDef, VarDecl, Variable,
};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode, TokenExt, TokenKind};
use rowan::TextRange;

/// Where a parameter list came from, which is what decides how loudly a
/// mismatch against it is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamSource {
    /// `sub f ($x, $y = 1)`. perl dies on a mismatch, so reporting one
    /// statically is free of false positives.
    Signature,
    /// `args my $x => 'Int'`. Smart::Args dies too.
    Args,
    /// `my ($self, $x) = @_` and a run of `my $x = shift`. perl does *not*
    /// die: a missing argument is `undef` and an extra one is ignored. So a
    /// mismatch here is a warning about a shape, not an error about a rule.
    Unpacking,
}

/// One parameter of a sub.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    /// A default, an `optional => 1`, or a position past the first default.
    pub optional: bool,
    /// The type annotation's source text, parsed in milestone 4.
    pub annotation: Option<Annotation>,
}

/// A type annotation as it was written, before the type-expression parser.
///
/// The source text rather than the subtree it came from, for two reasons. A
/// declaration outlives the tree it was read from — the declaration pass runs
/// over every file before the body pass runs over any — and a rowan node is
/// not `Send`, so a graph holding one could not be built by more than one
/// thread. The bareword syntax is Perl, so re-parsing the text gives the
/// subtree back when milestone 4's parser wants it.
#[derive(Debug, Clone)]
pub struct Annotation {
    pub text: String,
    /// Whether it arrived as a string (`'ArrayRef[Str]'`, the Moose grammar)
    /// rather than as an expression (`ArrayRef[Str]`, which is Perl).
    pub quoted: bool,
    pub range: TextRange,
}

/// What a call has to supply.
#[derive(Debug, Clone, Default)]
pub enum Params {
    /// Nothing is known, and nothing is ever reported against it.
    #[default]
    Unknown,
    Positional {
        /// Every parameter, the invocant included: a method call passes it,
        /// so leaving it out would make both sides of the comparison count
        /// different things.
        params: Vec<Param>,
        /// A `@rest` or `%opts` at the end: the maximum is unbounded.
        slurpy: bool,
        /// Whether the first parameter is a leading `$self` or `$class`, which
        /// is what says the sub is meant to be called through `->`.
        invocant: bool,
        source: ParamSource,
    },
    /// `args` — a `Dict` of named parameters.
    Named {
        params: Vec<Param>,
        invocant: bool,
        source: ParamSource,
    },
}

impl Params {
    /// The fewest arguments a call may pass, invocant excluded.
    #[must_use]
    pub fn minimum(&self) -> Option<usize> {
        match self {
            Params::Unknown => None,
            Params::Positional { params, .. } | Params::Named { params, .. } => {
                Some(params.iter().filter(|param| !param.optional).count())
            }
        }
    }

    /// The most arguments a call may pass, or `None` for unbounded.
    #[must_use]
    pub fn maximum(&self) -> Option<usize> {
        match self {
            Params::Unknown => None,
            Params::Positional {
                params,
                slurpy: false,
                ..
            } => Some(params.len()),
            Params::Positional { slurpy: true, .. } | Params::Named { .. } => None,
        }
    }

    #[must_use]
    pub fn source(&self) -> Option<ParamSource> {
        match self {
            Params::Unknown => None,
            Params::Positional { source, .. } | Params::Named { source, .. } => Some(*source),
        }
    }

    #[must_use]
    pub fn is_method(&self) -> bool {
        match self {
            Params::Unknown => false,
            Params::Positional { invocant, .. } | Params::Named { invocant, .. } => *invocant,
        }
    }

    #[must_use]
    pub fn named(&self) -> Option<&[Param]> {
        match self {
            Params::Named { params, .. } => Some(params),
            _ => None,
        }
    }
}

/// A sub, as the program graph holds it.
#[derive(Debug, Clone)]
pub struct SubDecl {
    pub package: String,
    pub name: String,
    pub params: Params,
    /// Where the name is, for "declared at".
    pub range: TextRange,
    /// Filled in by [`crate::program::Program::add`].
    pub file: usize,
}

/// What one file declares.
#[derive(Debug, Default)]
pub struct FileDecls {
    pub subs: Vec<SubDecl>,
    /// `use Foo qw(bar)` — the name and the package it came from.
    pub imports: HashMap<String, String>,
    /// The packages this file opens, with the offset each takes effect at.
    pub packages: Vec<(u32, String)>,
}

impl FileDecls {
    /// The package in effect at an offset.
    #[must_use]
    pub fn package_at(&self, offset: u32) -> &str {
        self.packages
            .iter()
            .take_while(|(at, _)| *at <= offset)
            .last()
            .map_or("main", |(_, name)| name.as_str())
    }
}

/// Read what a file declares.
#[must_use]
pub fn declare(root: &SyntaxNode) -> FileDecls {
    let mut decls = FileDecls::default();
    walk(root, &mut decls, "main");
    decls
}

fn walk(node: &SyntaxNode, decls: &mut FileDecls, outer: &str) {
    let mut package = outer.to_string();
    for child in node.children() {
        match child.node_kind() {
            NodeKind::PACKAGE_STMT => {
                let statement = ast::PackageStmt::cast(child.clone()).expect("kind checked");
                if let Some(name) = statement.name() {
                    match statement.block() {
                        // `package Foo { ... }` scopes the name to the block.
                        Some(block) => walk(block.syntax(), decls, &name),
                        None => {
                            decls
                                .packages
                                .push((u32::from(child.text_range().start()), name.clone()));
                            package = name;
                        }
                    }
                }
            }
            NodeKind::SUB_DEF => {
                let definition = SubDef::cast(child.clone()).expect("kind checked");
                if let Some(name) = definition.name_text() {
                    decls.subs.push(SubDecl {
                        package: package.clone(),
                        name,
                        params: parameters(&definition),
                        range: definition
                            .name()
                            .map_or_else(|| child.text_range(), |view| view.range()),
                        file: 0,
                    });
                }
                // A body declares nothing another file can see.
            }
            NodeKind::USE_STMT => {
                let statement = ast::UseStmt::cast(child.clone()).expect("kind checked");
                if let (Some(module), Some(arguments)) = (statement.module(), statement.arguments())
                {
                    for name in imported_names(&arguments) {
                        decls.imports.insert(name, module.clone());
                    }
                }
            }
            // A block, an `if`, a `BEGIN` — a sub inside one is still a sub of
            // the package, so the walk goes on rather than stopping at the
            // first thing that is not a declaration.
            _ => walk(&child, decls, &package),
        }
    }
}

/// The sub names an import list asks for.
///
/// A name with a sigil is a variable and the scope pass reads it; a bareword
/// or a plain string is a sub. `:tags` and `-flags` name a set this pass
/// cannot expand, so they contribute nothing.
fn imported_names(arguments: &SyntaxNode) -> Vec<String> {
    let mut acc = Vec::new();
    let mut push = |text: String| {
        if text
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        {
            acc.push(text);
        }
    };
    for element in Args::elements(arguments) {
        match element.node_kind() {
            NodeKind::QW_EXPR => {
                for word in ast::QwExpr::cast(element).expect("kind checked").words() {
                    push(word);
                }
            }
            NodeKind::LITERAL => {
                if let Some(text) = Literal::cast(element).and_then(|view| view.as_string()) {
                    push(text);
                }
            }
            _ => {}
        }
    }
    acc
}

// ===== Parameter lists =====

/// What a sub's shape says about the arguments it takes.
///
/// Four recognisers, in the order the design document lists them. The first
/// that matches wins, and no match is `Unknown` — which is never reported
/// against.
#[must_use]
pub fn parameters(definition: &SubDef) -> Params {
    if let Some(signature) = definition.signature() {
        return from_signature(&signature);
    }
    let Some(body) = definition.body() else {
        return Params::Unknown;
    };
    if let Some(params) = from_args(&body) {
        return params;
    }
    from_unpacking(&body)
}

fn from_signature(signature: &ast::SubSignature) -> Params {
    // GUESS: a parameter with a sigil and no name means this was a prototype.
    // Evidence: perl has no such signature — every signature parameter is
    // named — while `($)`, `($$;@)` and `(\@)` are ordinary prototypes. The
    // parser reads `($)` as a signature (`grep -rn 'GUESS:'`), and `sub is_info
    // ($)` in HTTP::Status is what a whole run of arity errors came from.
    // Wrong: a signature the checker declines to read, which is `Unknown` and
    // silent.
    if signature.params().any(|param| param.variable().is_none()) {
        return Params::Unknown;
    }

    let mut params = Vec::new();
    let mut slurpy = false;
    // "Minimum is the count before the first default": a parameter after one
    // is optional whether or not it has a default of its own, because perl
    // fills them left to right.
    let mut optional = false;
    for param in signature.params() {
        let Some(variable) = param.variable() else {
            continue;
        };
        if variable.sigil() != Sigil::Scalar {
            slurpy = true;
            continue;
        }
        if param.default().is_some() {
            optional = true;
        }
        params.push(Param {
            name: variable.display(),
            optional,
            annotation: None,
        });
    }
    let invocant = params
        .first()
        .is_some_and(|param| is_invocant_name(&param.name));
    Params::Positional {
        params,
        slurpy,
        invocant,
        source: ParamSource::Signature,
    }
}

/// `$self` and `$class` are the two names the modules themselves treat as the
/// invocant, so they are the two this reads that way.
fn is_invocant_name(display: &str) -> bool {
    display == "$self" || display == "$class"
}

/// `args my $self, my $who => 'Str', my $times => { isa => 'Int', default => 1 };`
///
/// The *first statement* of the body being an `args` or `args_pos` call is
/// what makes this a parameter list; an `args` anywhere else is a call that
/// declares nothing.
fn from_args(body: &ast::Block) -> Option<Params> {
    let first = body.statements().next()?;
    let call = first
        .descendants()
        .find_map(ast::Call::cast)
        .filter(|call| call.syntax().text_range().start() == leading_offset(&first))?;
    let callee = call.callee_name()?;
    let positional = match callee.as_str() {
        "args" => false,
        "args_pos" => true,
        _ => return None,
    };

    let mut params = Vec::new();
    let mut invocant = false;
    for (index, item) in call.pairs().iter().enumerate() {
        let (declaration, rule) = match item {
            ast::Arg::Pair { key, value, .. } => (key.clone(), Some(value.clone())),
            ast::Arg::Positional(node) => (node.clone(), None),
        };
        let Some(variable) = declared_variable(&declaration) else {
            // Something this recogniser does not cover; the whole list is
            // then a guess, and a guess is `Unknown`.
            return Some(Params::Unknown);
        };
        let is_invocant = index == 0 && is_invocant_name(&variable);
        invocant |= is_invocant;
        // A named list is keys, and the invocant is not one of them; a
        // positional list counts it, because a method call passes it.
        if is_invocant && !positional {
            continue;
        }
        let (optional, annotation) = rule.map_or((false, None), |node| read_rule(&node));
        params.push(Param {
            name: variable,
            optional,
            annotation,
        });
    }

    Some(if positional {
        Params::Positional {
            params,
            slurpy: false,
            invocant,
            source: ParamSource::Args,
        }
    } else {
        Params::Named {
            params,
            invocant,
            source: ParamSource::Args,
        }
    })
}

/// Where a statement's code begins, trivia excluded.
fn leading_offset(statement: &SyntaxNode) -> rowan::TextSize {
    statement.text_range().start()
}

/// `my $who` inside an `args` list — the variable it declares.
fn declared_variable(node: &SyntaxNode) -> Option<String> {
    let declaration = VarDecl::cast(node.clone())?;
    if declaration.keyword() != Some(DeclKeyword::My) {
        return None;
    }
    let targets = declaration.targets();
    (targets.len() == 1).then(|| targets[0].display())
}

/// The rule after `=>`: a type string, a type expression, or a hashref with
/// `isa` / `optional` / `default`.
fn read_rule(node: &SyntaxNode) -> (bool, Option<Annotation>) {
    if node.node_kind() == NodeKind::ANON_HASH {
        let hash = AnonHash::cast(node.clone()).expect("kind checked");
        let mut optional = false;
        let mut annotation = None;
        for pair in hash.pairs() {
            match pair.key() {
                Some("isa") => annotation = Some(annotation_of(pair.node())),
                Some("optional") | Some("default") | Some("builder") => optional = true,
                _ => {}
            }
        }
        return (optional, annotation.flatten());
    }
    (false, annotation_of(node))
}

/// A type annotation, read as whichever of the two syntaxes it is written in.
#[must_use]
pub fn annotation_of(node: &SyntaxNode) -> Option<Annotation> {
    if let Some(text) = Literal::cast(node.clone()).and_then(|view| view.as_string()) {
        return Some(Annotation {
            text,
            quoted: true,
            range: node.text_range(),
        });
    }
    match node.node_kind() {
        // A number or an `undef` in a type position is not a type.
        NodeKind::LITERAL | NodeKind::UNDEF_EXPR => None,
        _ => Some(Annotation {
            text: ast::joined_text(node),
            quoted: false,
            range: node.text_range(),
        }),
    }
}

/// `my ($self, $x, %opts) = @_;`, or a run of `my $x = shift;`.
///
/// Only when the sub touches `@_` in no other way. `$_[0]`, `scalar @_` and
/// `goto &sub` all mean the list is read as a list, and a parameter list read
/// off the first statement would then be a fiction.
fn from_unpacking(body: &ast::Block) -> Params {
    let mut params = Vec::new();
    let mut slurpy = false;
    let mut consumed = Vec::new();
    let mut statements = body.statements().peekable();

    // Form one: one list assignment from `@_`.
    if let Some(statement) = statements.peek() {
        if let Some((names, has_slurpy)) = list_unpacking(statement) {
            params = names;
            slurpy = has_slurpy;
            consumed.push(statement.text_range());
            statements.next();
        }
    }

    // Form two: a run of `my $x = shift;`.
    let mut optionals = Vec::new();
    if params.is_empty() && !slurpy {
        while let Some(statement) = statements.peek() {
            let Some((name, optional)) = shift_unpacking(statement) else {
                break;
            };
            params.push(name);
            optionals.push(optional);
            consumed.push(statement.text_range());
            statements.next();
        }
    }

    if params.is_empty() && !slurpy {
        return Params::Unknown;
    }
    if touches_arguments_elsewhere(body, &consumed) {
        return Params::Unknown;
    }

    let invocant = params.first().is_some_and(|name| is_invocant_name(name));
    let params: Vec<Param> = params
        .into_iter()
        .enumerate()
        .map(|(index, name)| Param {
            name,
            optional: optionals.get(index).copied().unwrap_or(false),
            annotation: None,
        })
        .collect();
    Params::Positional {
        params,
        slurpy,
        invocant,
        source: ParamSource::Unpacking,
    }
}

/// `my ($a, $b, %rest) = @_;` — the names, and whether the last one slurps.
fn list_unpacking(statement: &SyntaxNode) -> Option<(Vec<String>, bool)> {
    if statement.node_kind() != NodeKind::VAR_DECL_STMT {
        return None;
    }
    let assign = statement.descendants().find_map(ast::Assign::cast)?;
    let value = assign.value()?;
    if !is_argument_list(&value) {
        return None;
    }
    let declaration = VarDecl::cast(assign.target()?)?;
    if declaration.keyword() != Some(DeclKeyword::My) {
        return None;
    }
    let mut names = Vec::new();
    let mut slurpy = false;
    for target in declaration.targets() {
        if target.sigil() == Sigil::Scalar {
            names.push(target.display());
        } else {
            slurpy = true;
        }
    }
    Some((names, slurpy))
}

/// `my $x = shift;`, `my $x = shift @_;`, `my $x = shift || 'default';`.
///
/// The third form is the same parameter with a default, which makes it
/// optional; `Carp::str_len_trim` writes both in two lines.
fn shift_unpacking(statement: &SyntaxNode) -> Option<(String, bool)> {
    if statement.node_kind() != NodeKind::VAR_DECL_STMT {
        return None;
    }
    let assign = statement.descendants().find_map(ast::Assign::cast)?;
    if !assign.is_plain() {
        return None;
    }
    let (value, optional) = match assign.value()? {
        node if node.node_kind() == NodeKind::BINARY_EXPR => {
            let binary = ast::BinaryExpr::cast(node)?;
            match binary.operator() {
                Some(TokenKind::LOGICAL_OR | TokenKind::DEFINED_OR | TokenKind::OR_KW) => {
                    (binary.left()?, true)
                }
                _ => return None,
            }
        }
        node => (node, false),
    };
    let call = ast::Call::cast(value)?;
    if call.callee_name().as_deref() != Some("shift") {
        return None;
    }
    let arguments = call.args();
    if !arguments.is_empty() && !arguments.iter().all(is_argument_list) {
        return None;
    }
    let declaration = VarDecl::cast(assign.target()?)?;
    if declaration.keyword() != Some(DeclKeyword::My) {
        return None;
    }
    let targets = declaration.targets();
    (targets.len() == 1 && targets[0].sigil() == Sigil::Scalar)
        .then(|| (targets[0].display(), optional))
}

fn is_argument_list(node: &SyntaxNode) -> bool {
    Variable::cast(node.clone()).is_some_and(|variable| {
        variable.sigil() == Sigil::Array && variable.name().as_deref() == Some("_")
    })
}

/// Whether `@_` is read anywhere but in the statements already accounted for.
///
/// A bare `shift` counts: inside a sub it shifts `@_`, so a body that reaches
/// for one past the leading run takes an argument the run did not name.
/// `Net::DBus::RemoteService::new` shifts its class and then shifts three more
/// into a hash, and `Carp::str_len_trim` writes `shift || 0` on its second
/// line; both were reported as taking one argument until this looked.
fn touches_arguments_elsewhere(body: &ast::Block, consumed: &[TextRange]) -> bool {
    body.syntax().descendants().any(|node| {
        if consumed
            .iter()
            .any(|range| range.contains_range(node.text_range()))
        {
            return false;
        }
        if let Some(call) = ast::Call::cast(node.clone()) {
            if matches!(call.callee_name().as_deref(), Some("shift" | "pop"))
                && call.args().is_empty()
            {
                return true;
            }
        }
        match node.node_kind() {
            NodeKind::ARRAY_VAR | NodeKind::ARRAY_LAST_INDEX => {
                Variable::cast(node).and_then(|view| view.name()).as_deref() == Some("_")
            }
            // `$_[0]` is an element of `@_`.
            NodeKind::ARRAY_SUBSCRIPT_EXPR => {
                let arrow = ast::tokens(&node).any(|token| token.token_kind() == TokenKind::ARROW);
                !arrow
                    && node
                        .children()
                        .next()
                        .and_then(Variable::cast)
                        .and_then(|view| view.name())
                        .as_deref()
                        == Some("_")
            }
            _ => false,
        }
    })
}
