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

use crate::annotate::{self, AttributeDecl, Framework, Frameworks, NamedType, Returns};
use crate::diag::Diagnostic;
use crate::types::Type;

/// Where a parameter list came from, which is what decides how loudly a
/// mismatch against it is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Param {
    pub name: String,
    /// A default, an `optional => 1`, or a position past the first default.
    pub optional: bool,
    /// What the annotation says, `Unknown` when there is none.
    pub ty: Type,
}

/// A type annotation as it was written, before the type-expression parser.
///
/// The source text rather than the subtree it came from, for two reasons. A
/// declaration outlives the tree it was read from — the declaration pass runs
/// over every file before the body pass runs over any — and a rowan node is
/// not `Send`, so a graph holding one could not be built by more than one
/// thread. The bareword syntax is Perl, so re-parsing the text gives the
/// subtree back when milestone 4's parser wants it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Annotation {
    pub text: String,
    /// Whether it arrived as a string (`'ArrayRef[Str]'`, the Moose grammar)
    /// rather than as an expression (`ArrayRef[Str]`, which is Perl).
    pub quoted: bool,
    #[serde(with = "crate::serde_range")]
    pub range: TextRange,
}

/// What a call has to supply.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
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

/// Where a sub's shape came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SymbolSource {
    /// A signature, an `args` list, a `Returns:` — something written down.
    Annotated,
    /// Read off the body.
    Inferred,
    /// Nothing is known.
    Unknown,
}

/// A sub, as the program graph holds it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubDecl {
    pub package: String,
    pub name: String,
    pub params: Params,
    pub returns: Returns,
    pub source: SymbolSource,
    /// Where the name is, for "declared at".
    #[serde(with = "crate::serde_range")]
    pub range: TextRange,
    /// Filled in by [`crate::program::Program::add`].
    pub file: usize,
}

/// What a package is, beyond the subs in it (`docs/typecheck.md`, "Files,
/// packages, symbols").
///
/// These are the package-level facts that are not symbols: the ancestors, the
/// roles, and the object framework — which is what decides whether `new`
/// exists and what it accepts.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PackageFacts {
    pub name: String,
    pub isa: Vec<String>,
    pub roles: Vec<String>,
    pub framework: Framework,
    pub attributes: Vec<AttributeDecl>,
    /// What a `Type::Library` in this package declares.
    pub types: Vec<NamedType>,
    /// Whether the framework generates a `new`.
    pub constructor: bool,
    /// A `BUILDARGS` turns the constructor's argument check off: the class
    /// rewrites what it was given before anything sees it.
    pub buildargs: bool,
    /// The package makes methods by means this pass cannot read: an XS
    /// `bootstrap`, an `@ISA` computed at run time, a glob assignment. Such a
    /// class might have any method, so "no such method" is never said of it.
    pub dynamic: bool,
}

/// What one file declares.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct FileDecls {
    pub subs: Vec<SubDecl>,
    /// `use Foo qw(bar)` — the name and the package it came from.
    pub imports: HashMap<String, String>,
    /// The packages this file opens, with the offset each takes effect at.
    pub packages: Vec<(u32, String)>,
    /// What each package here is, beyond its subs.
    pub facts: Vec<PackageFacts>,
    /// Every module this file `use`s or `require`s, for the resolver.
    pub uses: Vec<String>,
    /// What the declaration pass had to say. Reported only for a file in the
    /// roots; a dependency contributes declarations and nothing else.
    pub diagnostics: Vec<Diagnostic>,
    /// Every type annotation the pass read, with where it was written. The
    /// questions that need the whole program — is this class one anything
    /// declares — are asked of this, later.
    pub annotations: Vec<crate::annotate::Annotated>,
}

impl FileDecls {
    /// What the pass learned about one package.
    #[must_use]
    pub fn facts_for(&self, name: &str) -> Option<&PackageFacts> {
        self.facts.iter().find(|facts| facts.name == name)
    }

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
    // The imports first, and all of them: recognition is by callee name *and*
    // by an import that could have provided it, and `use Moose` may sit below
    // the `has` it explains (a `package Foo { use Moose; ... }` block, or a
    // second package in the same file).
    let mut frameworks = Frameworks::default();
    for node in root.descendants() {
        if node.node_kind() == NodeKind::USE_STMT {
            if let Some(module) = ast::UseStmt::cast(node).and_then(|view| view.module()) {
                frameworks.note(&module);
            }
        }
    }

    let mut pass = Pass {
        decls: FileDecls::default(),
        sink: annotate::Sink::default(),
        frameworks,
        dynamic: false,
    };
    pass.walk(root, "main");
    // XS registers methods into whichever package it likes, and a glob
    // assignment can too, so a file that does either makes every package in it
    // one whose method set nobody here can enumerate.
    if pass.dynamic {
        for facts in &mut pass.decls.facts {
            facts.dynamic = true;
        }
    }
    // A package with a framework generates a constructor unless it said not to.
    for facts in &mut pass.decls.facts {
        if facts.framework == Framework::Moose {
            facts.constructor = true;
        }
    }
    pass.decls.diagnostics.append(&mut pass.sink.diagnostics);
    pass.decls.annotations = std::mem::take(&mut pass.sink.annotations);
    pass.decls
}

struct Pass {
    decls: FileDecls,
    sink: annotate::Sink,
    frameworks: Frameworks,
    /// The file loads XS or assigns a glob.
    dynamic: bool,
}

impl Pass {
    /// The facts for `package`, created on first mention.
    fn facts(&mut self, package: &str) -> &mut PackageFacts {
        if let Some(index) = self
            .decls
            .facts
            .iter()
            .position(|facts| facts.name == package)
        {
            return &mut self.decls.facts[index];
        }
        self.decls.facts.push(PackageFacts {
            name: package.to_string(),
            framework: self.frameworks.framework(),
            constructor: self.frameworks.framework() == Framework::AccessorTyped,
            ..PackageFacts::default()
        });
        self.decls.facts.last_mut().expect("just pushed")
    }

    fn walk(&mut self, node: &SyntaxNode, outer: &str) {
        let mut package = outer.to_string();
        for child in node.children() {
            match child.node_kind() {
                NodeKind::PACKAGE_STMT => {
                    let statement = ast::PackageStmt::cast(child.clone()).expect("kind checked");
                    if let Some(name) = statement.name() {
                        self.facts(&name);
                        match statement.block() {
                            // `package Foo { ... }` scopes the name to the block.
                            Some(block) => self.walk(block.syntax(), &name),
                            None => {
                                self.decls
                                    .packages
                                    .push((u32::from(child.text_range().start()), name.clone()));
                                package = name;
                            }
                        }
                    }
                }
                NodeKind::SUB_DEF => self.sub(&child, &package),
                NodeKind::USE_STMT | NodeKind::NO_STMT => self.use_statement(&child, &package),
                NodeKind::EXPR_STMT => {
                    self.glob_assignment(&child);
                    self.require_statement(&child);
                    self.expression_statement(&child, &package);
                    self.walk(&child, &package);
                }
                // `our @ISA = ('Base');` is a declaration statement, not an
                // expression one.
                NodeKind::VAR_DECL_STMT => {
                    self.isa_assignment(&child, &package);
                    self.glob_assignment(&child);
                }
                // A block, an `if`, a `BEGIN` — a sub inside one is still a sub
                // of the package, so the walk goes on rather than stopping at
                // the first thing that is not a declaration.
                _ => self.walk(&child, &package),
            }
        }
    }

    fn sub(&mut self, node: &SyntaxNode, package: &str) {
        let definition = SubDef::cast(node.clone()).expect("kind checked");
        let Some(name) = definition.name_text() else {
            return;
        };
        if name == "BUILDARGS" {
            self.facts(package).buildargs = true;
        }
        let params = parameters(&definition, self.frameworks.smart_args, &mut self.sink);
        let annotated = annotate::read_returns(&definition, &mut self.sink);
        let source = if annotated.is_some()
            || matches!(
                params.source(),
                Some(ParamSource::Signature | ParamSource::Args)
            ) {
            SymbolSource::Annotated
        } else if params.source().is_some() {
            SymbolSource::Inferred
        } else {
            SymbolSource::Unknown
        };
        self.decls.subs.push(SubDecl {
            package: package.to_string(),
            name,
            params,
            returns: annotated.unwrap_or_default(),
            source,
            range: definition
                .name()
                .map_or_else(|| node.text_range(), |view| view.range()),
            file: 0,
        });
        // A body declares nothing another file can see.
    }

    /// `use Foo qw(bar)`, `use parent -norequire, 'Base'`, `use vars`, and the
    /// `Class::Accessor::Typed` declaration, which is a `use` statement whose
    /// argument list is one.
    fn use_statement(&mut self, node: &SyntaxNode, package: &str) {
        let module = match node.node_kind() {
            NodeKind::USE_STMT => ast::UseStmt::cast(node.clone()).and_then(|view| view.module()),
            _ => ast::NoStmt::cast(node.clone()).and_then(|view| view.module()),
        };
        let Some(module) = module else {
            return;
        };
        let arguments = match node.node_kind() {
            NodeKind::USE_STMT => {
                ast::UseStmt::cast(node.clone()).and_then(|view| view.arguments())
            }
            _ => ast::NoStmt::cast(node.clone()).and_then(|view| view.arguments()),
        };

        if node.node_kind() == NodeKind::USE_STMT {
            self.decls.uses.push(module.clone());
        }

        // A module that loads XS has its methods written in C, where no
        // recogniser can reach them.
        if matches!(
            module.as_str(),
            "XSLoader" | "DynaLoader" | "Inline" | "Alien::Base"
        ) {
            self.dynamic = true;
        }

        match module.as_str() {
            "parent" | "base" => {
                if let Some(arguments) = &arguments {
                    let parents: Vec<String> = imported_names(arguments)
                        .into_iter()
                        .filter(|name| name != "norequire")
                        .collect();
                    self.facts(package).isa.extend(parents);
                }
                return;
            }
            "Class::Accessor::Typed" => {
                if let Some(arguments) = &arguments {
                    let (attributes, constructor) =
                        annotate::read_accessor_typed(arguments, &mut self.sink);
                    let facts = self.facts(package);
                    facts.attributes.extend(attributes);
                    facts.constructor = constructor;
                }
                return;
            }
            _ => {}
        }

        if let Some(arguments) = &arguments {
            for name in imported_names(arguments) {
                self.decls.imports.insert(name, module.clone());
            }
        }
    }

    /// `*Foo::bar = sub {...};` and `*{"${class}::$name"} = ...` make a method
    /// out of nothing this pass can name (`docs/typecheck.md`, non-goals), so
    /// the package they are in might have any method.
    fn glob_assignment(&mut self, node: &SyntaxNode) {
        let Some(assign) = node.descendants().find_map(ast::Assign::cast) else {
            return;
        };
        let Some(target) = assign.target() else {
            return;
        };
        let is_glob = matches!(
            target.node_kind(),
            NodeKind::TYPEGLOB_VAR | NodeKind::BLOCK_DEREF_EXPR
        ) && ast::tokens(&target)
            .chain(target.descendants().flat_map(|inner| ast::tokens(&inner)))
            .any(|token| token.token_kind() == TokenKind::TYPEGLOB_SIGIL);
        if is_glob {
            self.dynamic = true;
        }
    }

    /// `require Foo::Bar;` is a dependency the resolver should follow, the
    /// same as a `use`. `HTTP::Date` reaches `Time::Local` this way and
    /// nothing else does.
    fn require_statement(&mut self, node: &SyntaxNode) {
        for inner in node.descendants() {
            if inner.node_kind() != NodeKind::REQUIRE_EXPR {
                continue;
            }
            if let Some(name) = ast::child::<ast::SubName>(&inner) {
                self.decls.uses.push(name.text());
            }
        }
    }

    /// The recognisers that are calls: `has`, `extends`, `with`, and the
    /// `Type::Library` family.
    fn expression_statement(&mut self, node: &SyntaxNode, package: &str) {
        let Some(call) = node
            .descendants()
            .find_map(ast::Call::cast)
            .filter(|call| call.syntax().text_range().start() == node.text_range().start())
        else {
            // `our @ISA = ('Base');` is an assignment, not a call.
            self.isa_assignment(node, package);
            return;
        };
        let Some(callee) = call.callee_name() else {
            return;
        };
        match callee.as_str() {
            "has" if self.frameworks.moose => {
                let attributes = annotate::read_has(&call, &mut self.sink);
                self.facts(package).attributes.extend(attributes);
            }
            "extends" if self.frameworks.moose => {
                let parents: Vec<String> = call.args().iter().filter_map(ast::key_text).collect();
                self.facts(package).isa.extend(parents);
            }
            "with" if self.frameworks.moose => {
                let roles: Vec<String> = call.args().iter().filter_map(ast::key_text).collect();
                self.facts(package).roles.extend(roles);
            }
            "declare" | "subtype" | "class_type" | "role_type" | "duck_type" | "enum" | "union"
                if self.frameworks.type_library =>
            {
                if let Some(named) = annotate::read_type_library(&call, &mut self.sink) {
                    self.facts(package).types.push(named);
                }
            }
            // `Foo->bootstrap($VERSION)` and `XSLoader::load(...)`: the
            // methods are in the shared library, not in the file.
            "bootstrap" | "XSLoader::load" | "DynaLoader::bootstrap" => {
                self.dynamic = true;
            }
            _ => {}
        }
    }

    /// `our @ISA = ('Base');` and `push @ISA, 'Base';`.
    fn isa_assignment(&mut self, node: &SyntaxNode, package: &str) {
        let Some(assign) = node.descendants().find_map(ast::Assign::cast) else {
            return;
        };
        let Some(target) = assign.target() else {
            return;
        };
        let is_isa = target
            .descendants()
            .filter_map(Variable::cast)
            .any(|variable| {
                variable.sigil() == Sigil::Array && variable.name().as_deref() == Some("ISA")
            });
        if !is_isa {
            return;
        }
        let Some(value) = assign.value() else {
            return;
        };
        let elements = Args::elements(&value);
        let parents: Vec<String> = elements.iter().filter_map(ast::key_text).collect();
        // `@ISA = ($module)` — `File::Spec` picks its parent at run time, and
        // a class whose ancestry is computed might have any method.
        if parents.len() != elements.len() {
            self.facts(package).dynamic = true;
        }
        self.facts(package).isa.extend(parents);
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
pub fn parameters(definition: &SubDef, smart_args: bool, into: &mut annotate::Sink) -> Params {
    if let Some(signature) = definition.signature() {
        // GUESS: `sub f()` with a body that reads `@_` was a prototype.
        // Evidence: the body. An empty `()` is a signature only where the
        // feature is on, and is otherwise a prototype saying "call me with no
        // arguments" that a method still receives `$self` through —
        // `Mail::Internet::cleaned_header_dup()` shifts its invocant out of
        // one. Wrong: a real empty signature whose body reads `@_` anyway,
        // which perl would have made unreachable.
        let empty = signature.params().next().is_none();
        let reads_arguments = definition
            .body()
            .is_some_and(|body| touches_arguments_elsewhere(&body, &[]));
        if !(empty && reads_arguments) {
            return from_signature(&signature);
        }
        return Params::Unknown;
    }
    let Some(body) = definition.body() else {
        return Params::Unknown;
    };
    // Recognition is by callee name *and* by an import that could have
    // provided it: a project's own `sub args` is not Smart::Args'.
    if smart_args {
        if let Some(params) = from_args(&body, into) {
            return params;
        }
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
            // A signature says nothing about types (`docs/typecheck.md`,
            // "Signatures"): every parameter is `Any` and only the arity is
            // exact.
            ty: Type::Any,
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
fn from_args(body: &ast::Block, into: &mut annotate::Sink) -> Option<Params> {
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
        let ty = annotation.map_or(Type::Unknown, |annotation| {
            annotate::read_annotation(&annotation, into)
        });
        params.push(Param {
            name: variable,
            optional,
            ty,
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
            // `@_` unpacking says nothing about types either.
            ty: Type::Any,
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
