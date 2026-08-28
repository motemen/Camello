//! The declaration pass (`docs/typecheck.md`, "Data flow").
//!
//! What a file *declares*, read without opening a single sub body. That
//! restriction is the design's and it is what makes dependencies cheap: a body
//! can only use a declaration, never make one another file could see, so the
//! program graph is complete after this pass and editing a body invalidates
//! one sub and nothing else.
//!
//! Three things are read out of a body, and each is a fact about the sub or
//! its file rather than about the program. The parameter list is written
//! *inside* it when the sub uses `args` or unpacks `@_`, and is read from the
//! body's leading statements. A `sub new` is asked whether it says the value
//! it hands back is one of its own class (`docs/types.md`, INFER-2g), because
//! a `new` that is a factory makes every method called on its answer missing.
//! And every body is asked whether it *makes* methods — a glob assignment, or
//! an accessor maker whose list of names cannot be read — because that is
//! where a code generator keeps its (METHOD-5d, METHOD-5g).

use std::collections::{HashMap, HashSet};

use camello_syntax::ast::{
    self, AnonHash, Args, AstNode, DeclKeyword, Literal, Sigil, SubDef, VarDecl, Variable,
};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode, SyntaxToken, TokenExt, TokenKind};
use rowan::TextRange;

use crate::annotate::{
    self, Access, AccessorMaker, AttributeDecl, Dialect, Framework, Frameworks, NamedType, Returns,
};
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
    /// What an attribute declaration generates: an accessor, a `writer`, a
    /// `predicate`. The shape is the framework's rather than the author's,
    /// and what each of them does with an argument it did not want is the
    /// framework's business — Moose's reader ignores one and
    /// `Class::Accessor::Lite`'s croaks — so a mismatch here is a warning.
    Generated,
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
        /// The keys, and only the keys.
        params: Vec<Param>,
        /// The invocant's name (`$self`, `$class`), where the list has one.
        ///
        /// Not in `params`, which is the difference from `Positional`: `args`
        /// takes the invocant by position and everything after it by name, so
        /// putting it in the list would make it look like a key nobody passes.
        /// The *name* rather than a flag, because the body binds it under
        /// whichever of the two it was written as.
        invocant: Option<String>,
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

    /// Whether the declaration was a bare `()` that might have been a
    /// prototype rather than a signature (`docs/types.md`, DIAG-15).
    ///
    /// A `method ()` is not one: the keyword exists only where the `class`
    /// feature is on, and there `()` is a signature and never a prototype —
    /// which is also why its invocant is already in the list.
    #[must_use]
    pub fn is_empty_parens(&self) -> bool {
        matches!(
            self,
            Params::Positional {
                params,
                slurpy: false,
                invocant: false,
                source: ParamSource::Signature,
            } if params.is_empty()
        )
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
            Params::Positional { invocant, .. } => *invocant,
            Params::Named { invocant, .. } => invocant.is_some(),
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
    /// For a `new`: whether the body says the thing it hands back is one of
    /// this class (`docs/types.md`, INFER-2g).
    #[serde(default)]
    pub constructs_own_class: bool,
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
    /// The generated `new` blesses whatever hash it was handed rather than
    /// checking it against the attributes — `Class::Accessor::Lite` and the
    /// `mk_new` family. A key it declares no accessor for is legal there, so
    /// `unknown-key` has nothing to contradict.
    pub open_constructor: bool,
    /// The package makes methods by means this pass cannot read: an XS
    /// `bootstrap`, an `@ISA` computed at run time, a glob assignment. Such a
    /// class might have any method, so "no such method" is never said of it.
    pub dynamic: bool,
    /// What `our @EXPORT` puts into the namespace of every package that `use`s
    /// this one (`docs/types.md`, METHOD-6).
    #[serde(default)]
    pub exports: Vec<String>,
    /// An `@EXPORT` whose value this pass could not read — `our @EXPORT =
    /// get_public_functions;`. What it names cannot be enumerated, so neither
    /// can the method set of a package that imports it.
    #[serde(default)]
    pub exports_unknown: bool,
    /// The modules this package `use`s, for the two questions above.
    #[serde(default)]
    pub uses: Vec<String>,
    /// `(class, method)` for every method call this package makes at file
    /// scope, which is where a code generator is invoked (METHOD-5g).
    #[serde(default)]
    pub file_scope_calls: Vec<(String, String)>,
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
    declare_in(root, &Dialect::default())
}

/// The same, for a project whose own modules stand in for the ones the
/// recognisers know (`camello.toml`, `read-as`).
#[must_use]
pub fn declare_in(root: &SyntaxNode, dialect: &Dialect) -> FileDecls {
    // The imports first, and all of a package's: recognition is by callee name
    // *and* by an import that could have provided it, and `use Moose` may sit
    // below the `has` it explains.
    let mut frameworks = HashMap::new();
    collect_frameworks(root, "main", dialect, &mut frameworks);

    let mut pass = Pass {
        decls: FileDecls::default(),
        sink: annotate::Sink::default(),
        frameworks,
        dialect: dialect.clone(),
        dynamic: false,
        best_practice: HashSet::new(),
        decided_constructor: HashSet::new(),
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
    // A package with a framework generates a constructor unless it said not to
    // — a package that decided for itself is left alone, which is how
    // `Class-Accessor-Typed-0.03/t/02_does.t` gets to hold both a Mouse role
    // and a `use Class::Accessor::Typed (new => 0)`.
    for facts in &mut pass.decls.facts {
        if facts.framework == Framework::Moose && !pass.decided_constructor.contains(&facts.name) {
            facts.constructor = true;
        }
    }
    pass.decls.diagnostics.append(&mut pass.sink.diagnostics);
    pass.decls.annotations = std::mem::take(&mut pass.sink.annotations);
    infer_returns_locally(pass.decls, root, dialect)
}

/// How many rounds tier 1 gives one file (`docs/return-inference.md`,
/// "Tier 1").
///
/// A sub goes from `Unknown` to known once and never changes after, so the
/// rounds a file needs are the depth of its own chains of unannotated calls —
/// two or three in practice. The cap is what keeps the declaration pass's
/// cost a small multiple of one body walk rather than a multiple of the
/// file's longest chain: what it cuts off stays `Unknown`, which is silent,
/// and tier 2 gets another go at it.
const LOCAL_ROUNDS: usize = 4;

/// Everything a single file can say about its own subs' returns.
///
/// Inside the declaration pass, not beside it, because the answer is part of
/// `signature_of` — hence of the language server's decl fingerprint, hence of
/// "this edit changed what other files can see" — and because it is what
/// makes a cached dependency's leaf accessors typed at no cost beyond the
/// first run.
///
/// What a single file can see is literals, constructors, `bless`, its own
/// packages' attributes and its own subs. A call into another file is
/// `Unknown`, and the sub stays `Unknown` *for now*: tier 2 runs over the
/// whole program once every file is in.
fn infer_returns_locally(decls: FileDecls, root: &SyntaxNode, dialect: &Dialect) -> FileDecls {
    let mut program = crate::program::Program::new();
    program.set_dialect(dialect.clone());
    let file = program.add(std::path::Path::new(""), decls, false);
    for _ in 0..LOCAL_ROUNDS {
        let only = program.unresolved_returns(file);
        if only.is_empty() {
            break;
        }
        let found = crate::flow::infer_returns(root, file, &program, &only);
        if found.is_empty() {
            break;
        }
        for (index, returns) in found {
            program.set_returns(file, index, returns);
        }
    }
    program.take_decls(file)
}

struct Pass {
    decls: FileDecls,
    sink: annotate::Sink,
    frameworks: HashMap<String, Frameworks>,
    /// What this project's own modules stand in for.
    dialect: Dialect,
    /// The file loads XS or assigns a glob.
    dynamic: bool,
    /// Packages that have called `follow_best_practice`. Positional, because
    /// the renaming only applies to the `mk_*` calls below it.
    best_practice: HashSet<String>,
    /// Packages whose accessor declaration said whether there is a `new`.
    decided_constructor: HashSet<String>,
}

impl Pass {
    /// The facts for `package`, created on first mention.
    /// What a package's own imports say a bareword in it could mean.
    ///
    /// Per package rather than per file: `use Moose` imports `has` into the
    /// package it is written in, so a second package in the same file that
    /// declares a `sub has` of its own is not writing Moose's
    /// (`docs/types.md`, ANNOT-1a). Read as a file's, `Mine->new(...)` in a
    /// package that never said `use Moose` was an `unknown-key` error against
    /// a constructor it does not have.
    fn frameworks(&self, package: &str) -> Frameworks {
        self.frameworks
            .get(package)
            .cloned()
            .unwrap_or_else(|| Frameworks::with_dialect(self.dialect.clone()))
    }

    fn facts(&mut self, package: &str) -> &mut PackageFacts {
        if let Some(index) = self
            .decls
            .facts
            .iter()
            .position(|facts| facts.name == package)
        {
            return &mut self.decls.facts[index];
        }
        let framework = self.frameworks(package).framework();
        self.decls.facts.push(PackageFacts {
            name: package.to_string(),
            framework,
            constructor: framework == Framework::AccessorTyped,
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
                    self.export_assignment(&child, &package);
                    self.require_statement(&child);
                    self.expression_statement(&child, &package);
                    self.accessor_statement(&child, &package);
                    self.file_scope_call(&child, &package);
                    self.walk(&child, &package);
                }
                // `our @ISA = ('Base');` is a declaration statement, not an
                // expression one.
                NodeKind::VAR_DECL_STMT => {
                    self.isa_assignment(&child, &package);
                    self.export_assignment(&child, &package);
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
        let params = parameters(
            &definition,
            self.frameworks(package).smart_args,
            &mut self.sink,
        );
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
        // A body that makes methods makes them just as one at file scope does
        // (METHOD-5d), and a body is where every accessor generator keeps its:
        // `sub mk_fields { ... *{"${class}::$name"} = sub {...} }`, or the
        // same thing delegated to a maker whose list is a variable.
        if let Some(body) = definition.body() {
            self.makes_methods_within(body.syntax());
        }
        let constructs_own_class = name == "new" && constructs_its_class(&definition);
        self.decls.subs.push(SubDecl {
            package: package.to_string(),
            name,
            params,
            returns: annotated.unwrap_or_default(),
            source,
            constructs_own_class,
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
            self.facts(package).uses.push(module.clone());
        }

        // What the recognisers below are asked about. The resolver and the
        // import list keep the name that was written: a wrapper is still a
        // module of its own, at a path of its own.
        let read_as = self.dialect.read_as(&module).to_string();

        // A module that loads XS has its methods written in C, where no
        // recogniser can reach them.
        if matches!(
            read_as.as_str(),
            "XSLoader" | "DynaLoader" | "Inline" | "Alien::Base"
        ) {
            self.dynamic = true;
        }

        match read_as.as_str() {
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
                    self.decided_constructor.insert(package.to_string());
                    let facts = self.facts(package);
                    facts.attributes.extend(attributes);
                    facts.constructor = constructor;
                }
                return;
            }
            "constant" => {
                if let Some(arguments) = &arguments {
                    for (name, range) in constant_names(arguments) {
                        self.decls.subs.push(SubDecl {
                            package: package.to_string(),
                            name,
                            // A constant takes no arguments — and a call may
                            // still pass one, because `Foo->NAME` is how a
                            // class constant is commonly read and perl does
                            // not mind. Nothing to count, so nothing counted.
                            params: Params::Unknown,
                            returns: Returns::default(),
                            source: SymbolSource::Unknown,
                            constructs_own_class: false,
                            range,
                            file: 0,
                        });
                    }
                }
                return;
            }
            "Class::Accessor::Lite" | "Class::Accessor::Lite::Lazy" => {
                if let Some(arguments) = &arguments {
                    let (attributes, constructor) = annotate::read_accessor_lite(arguments);
                    let facts = self.facts(package);
                    facts.attributes.extend(attributes);
                    if constructor {
                        facts.constructor = true;
                        facts.open_constructor = true;
                    }
                }
                // `rw`, `ro` and `new` are the declaration's own words, not
                // subs this file imported.
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
    /// Whether a sub body makes methods: a glob assignment, or a call to an
    /// accessor maker whose list of names this pass cannot read.
    ///
    /// `Class::Accessor::Lite->mk_ro_accessors($field)` in a loop is the same
    /// generator a glob assignment is, one layer of politeness up, and the
    /// names it makes are no more enumerable.
    fn makes_methods_within(&mut self, node: &SyntaxNode) {
        for statement in node.descendants() {
            if matches!(
                statement.node_kind(),
                NodeKind::EXPR_STMT | NodeKind::VAR_DECL_STMT
            ) {
                self.glob_assignment(&statement);
            }
        }
        for call in node.descendants().filter_map(ast::MethodCallExpr::cast) {
            let Some(maker) = call.method_name().as_deref().and_then(AccessorMaker::of) else {
                continue;
            };
            if maker == AccessorMaker::BestPractice {
                continue;
            }
            let lazy = matches!(maker, AccessorMaker::Accessors { lazy: true, .. });
            let readable = call
                .args()
                .iter()
                .all(|argument| !annotate::listed_names(argument, lazy).is_empty());
            if !readable {
                self.dynamic = true;
            }
        }
    }

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
        let frameworks = self.frameworks(package);
        match callee.as_str() {
            "has" if frameworks.moose => {
                let attributes = annotate::read_has(&call, &mut self.sink);
                self.facts(package).attributes.extend(attributes);
            }
            "extends" if frameworks.moose => {
                let parents: Vec<String> = call.args().iter().filter_map(ast::key_text).collect();
                self.facts(package).isa.extend(parents);
            }
            "with" if frameworks.moose => {
                let roles: Vec<String> = call.args().iter().filter_map(ast::key_text).collect();
                self.facts(package).roles.extend(roles);
            }
            "declare" | "subtype" | "type" | "class_type" | "role_type" | "duck_type" | "enum"
            | "union" | "intersection"
                if frameworks.type_library =>
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

    /// `__PACKAGE__->mk_accessors(qw(foo bar));` and the rest of the family.
    ///
    /// ```perl
    /// use base 'Class::Accessor';
    /// __PACKAGE__->follow_best_practice;
    /// __PACKAGE__->mk_accessors(qw(name role));
    ///
    /// use Class::Accessor::Lite;
    /// Class::Accessor::Lite->mk_new_and_accessors(qw(foo bar));
    /// ```
    ///
    /// The two spellings differ only in what they are written on:
    /// `Class::Accessor::Lite` installs into `caller`, and a `Class::Accessor`
    /// subclass calls the inherited method on itself. Either way the
    /// accessors belong to the package the statement is in — unless it names
    /// another class outright, which is the one case worth following.
    fn accessor_statement(&mut self, node: &SyntaxNode, package: &str) {
        if !self.frameworks(package).accessor_lite {
            return;
        }
        let Some(call) = node
            .descendants()
            .find_map(ast::MethodCallExpr::cast)
            .filter(|call| call.syntax().text_range().start() == node.text_range().start())
        else {
            return;
        };
        let Some(maker) = call.method_name().as_deref().and_then(AccessorMaker::of) else {
            return;
        };
        let Some(target) = self.accessor_target(&call, package) else {
            return;
        };
        let range = call.method_range();
        if maker == AccessorMaker::BestPractice {
            self.best_practice.insert(target);
            return;
        }
        if matches!(maker, AccessorMaker::New | AccessorMaker::NewAndAccessors) {
            let facts = self.facts(&target);
            facts.constructor = true;
            facts.open_constructor = true;
        }
        let (access, lazy) = match maker {
            AccessorMaker::Accessors { access, lazy } => (access, lazy),
            AccessorMaker::NewAndAccessors => (Access::Rw, false),
            // `mk_new` takes no names.
            AccessorMaker::New | AccessorMaker::BestPractice => return,
        };
        let names: Vec<annotate::AccessorName> = call
            .args()
            .iter()
            .flat_map(|argument| annotate::listed_names(argument, lazy))
            .collect();
        let mut attributes = annotate::accessor_attributes(&names, access, lazy, range);
        if self.best_practice.contains(&target) {
            for attribute in &mut attributes {
                attribute.methods = annotate::best_practice_methods(&attribute.name, access);
            }
        }
        self.facts(&target).attributes.extend(attributes);
    }

    /// Which package a `mk_accessors` call puts its accessors in.
    ///
    /// `Class::Accessor::Lite->mk_accessors(...)` installs into its caller, so
    /// the module's own name means "here", as `__PACKAGE__` does. A dynamic
    /// invocant (`$class->mk_accessors(...)`) names a package this pass cannot
    /// know, and is left alone rather than guessed at.
    fn accessor_target(&self, call: &ast::MethodCallExpr, package: &str) -> Option<String> {
        let invocant = call.invocant()?;
        let name = ast::Call::cast(invocant.clone())
            .and_then(|view| view.callee_name())
            .or_else(|| ast::key_text(&invocant))?;
        if name == "__PACKAGE__" || name.starts_with("Class::Accessor") {
            return Some(package.to_string());
        }
        Some(name)
    }

    /// `our @ISA = ('Base');` and `push @ISA, 'Base';`.
    /// `our @EXPORT = qw(a b);` — the names this package puts into every
    /// importer's namespace (`docs/types.md`, METHOD-6).
    ///
    /// A value this cannot read is the interesting half: `our @EXPORT =
    /// get_public_functions;` is a whole mixin's worth of methods appearing in
    /// a package whose own file never names one of them.
    fn export_assignment(&mut self, node: &SyntaxNode, package: &str) {
        let Some(assign) = node.descendants().find_map(ast::Assign::cast) else {
            return;
        };
        let Some(target) = assign.target() else {
            return;
        };
        let Some(owner) = assigned_package(&target, "EXPORT", package) else {
            return;
        };
        let Some(value) = assign.value() else {
            return;
        };
        match exported_names(&value) {
            Some(names) => self.facts(&owner).exports.extend(names),
            None => self.facts(&owner).exports_unknown = true,
        }
    }

    /// `Some::Generator->make_accessors([...])` written at file scope.
    ///
    /// A generator that assigns to globs puts its methods into the *caller's*
    /// package, and the caller's own file says nothing about them
    /// (`docs/types.md`, METHOD-5g). Only file scope, because that is when a
    /// generator has to run.
    fn file_scope_call(&mut self, node: &SyntaxNode, package: &str) {
        let Some(call) = node
            .descendants()
            .find_map(ast::MethodCallExpr::cast)
            .filter(|call| call.syntax().text_range().start() == node.text_range().start())
        else {
            return;
        };
        let Some(method) = call.method_name() else {
            return;
        };
        let Some(target) = call.invocant().as_ref().and_then(ast::key_text) else {
            return;
        };
        // `__PACKAGE__->mk_methods(...)` is how a class asks the generator it
        // inherited to write into it.
        let target = if target == "__PACKAGE__" {
            package.to_string()
        } else {
            target
        };
        self.facts(package).file_scope_calls.push((target, method));
    }

    fn isa_assignment(&mut self, node: &SyntaxNode, package: &str) {
        let Some(assign) = node.descendants().find_map(ast::Assign::cast) else {
            return;
        };
        let Some(target) = assign.target() else {
            return;
        };
        let Some(owner) = assigned_package(&target, "ISA", package) else {
            return;
        };
        let Some(value) = assign.value() else {
            return;
        };
        let mut parents = Vec::new();
        let mut opaque = false;
        for element in Args::elements(&value) {
            // `qw(A B)` is one element holding two names, which is how most
            // of a corpus spells its `@ISA`.
            if element.node_kind() == NodeKind::QW_EXPR {
                parents.extend(ast::QwExpr::cast(element).expect("kind checked").words());
                continue;
            }
            // `@ISA = ($module)` — `File::Spec` picks its parent at run time,
            // and a class whose ancestry is computed might have any method.
            match ast::key_text(&element) {
                Some(name) => parents.push(name),
                None => opaque = true,
            }
        }
        if opaque {
            self.facts(&owner).dynamic = true;
        }
        self.facts(&owner).isa.extend(parents);
    }
}

/// Which package an assignment to `@ISA` or `@EXPORT` is about, or `None` when
/// the left side is neither.
///
/// `our @ISA = (...)` is the package the statement sits in. `@CPAN::FTP::ISA =
/// qw(CPAN::Debug)` names its own, which is how a file written before `our`
/// existed says it — and how a file holding several packages says which one it
/// means. The qualified name wins over the enclosing package, because it is
/// the one perl would write to.
fn assigned_package(target: &SyntaxNode, array: &str, package: &str) -> Option<String> {
    let suffix = format!("::{array}");
    target
        .descendants()
        .filter_map(Variable::cast)
        .filter(|variable| variable.sigil() == Sigil::Array)
        .find_map(|variable| {
            let name = variable.name()?;
            if name == array {
                return Some(package.to_string());
            }
            let owner = name.strip_suffix(&suffix)?;
            (!owner.is_empty()).then(|| owner.to_string())
        })
}

/// The names `use constant` declares, and where each is written.
///
/// ```perl
/// use constant PI       => 3.14159;
/// use constant WEEKDAYS => qw(Mon Tue);
/// use constant { E => 2.71, PHI => 1.61 };
/// ```
///
/// One name and a value, or a hash of them. The value is not read: what a
/// constant gives back is what the expression after it evaluates to, and this
/// pass evaluates nothing (`docs/types.md`, POLICY-5). Declaring the name is
/// the point — a constant is a sub, so `Foo->NAME` is a method call like any
/// other, and a package whose constants were invisible answered
/// `unknown-method` to every one of them.
fn constant_names(arguments: &SyntaxNode) -> Vec<(String, TextRange)> {
    let elements = Args::elements(arguments);
    let first = elements.first();
    // `use constant { ... }` — every key is a name.
    if let Some(hash) = first
        .map(ast::without_plus)
        .filter(|node| node.node_kind() == NodeKind::ANON_HASH)
        .and_then(AnonHash::cast)
    {
        return hash
            .pairs()
            .iter()
            .filter_map(|pair| match pair {
                ast::Arg::Pair {
                    key,
                    key_text: Some(name),
                    ..
                } => Some((name.clone(), key.text_range())),
                _ => None,
            })
            .collect();
    }
    // `use constant NAME => ...` — one name, and the rest is its value.
    first
        .and_then(|node| Some((ast::key_text(node)?, node.text_range())))
        .into_iter()
        .collect()
}

/// The sub names an import list asks for.
///
/// A name with a sigil is a variable and the scope pass reads it; a bareword
/// or a plain string is a sub. `:tags` and `-flags` name a set this pass
/// cannot expand, so they contribute nothing.
/// The sub names an `@EXPORT` list holds, or `None` when it is not a list of
/// names at all (`docs/types.md`, METHOD-6).
///
/// `&name` is a sub written the long way; `$name`, `@name` and `%name` are
/// variables, which are exported too and are never methods, so they are
/// passed over rather than making the whole list unreadable.
fn exported_names(value: &SyntaxNode) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let elements = Args::elements(value);
    if elements.is_empty() {
        return None;
    }
    for element in elements {
        let words = match element.node_kind() {
            NodeKind::QW_EXPR => ast::QwExpr::cast(element).expect("kind checked").words(),
            NodeKind::LITERAL => vec![Literal::cast(element).and_then(|view| view.as_string())?],
            _ => return None,
        };
        for word in words {
            let name = word.strip_prefix('&').unwrap_or(&word);
            if name.starts_with(['$', '@', '%', '*']) {
                continue;
            }
            names.push(name.to_string());
        }
    }
    Some(names)
}

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

/// A sub's declaration as one line, the way [types.md](../../../docs/types.md)
/// spells types.
///
/// One renderer rather than two, because hover and completion show the same
/// thing in two places and a signature that reads differently in each is a
/// signature the reader has to learn twice (`docs/lsp.md`, "Hover").
///
/// Silent where the checker is silent: a sub with no signature, no `args` and
/// no `Returns:` renders as its name and nothing else, rather than as a lie
/// about taking no arguments.
#[must_use]
pub fn signature_of(symbol: &SubDecl) -> String {
    let mut out = String::new();
    if !symbol.package.is_empty() && symbol.package != "main" {
        out.push_str(&symbol.package);
        out.push_str("::");
    }
    out.push_str(&symbol.name);
    match &symbol.params {
        Params::Unknown => {}
        Params::Positional { params, slurpy, .. } => {
            let mut parts: Vec<String> = params.iter().map(render_param).collect();
            if *slurpy {
                parts.push("...".to_string());
            }
            out.push('(');
            out.push_str(&parts.join(", "));
            out.push(')');
        }
        Params::Named {
            params, invocant, ..
        } => {
            // The invocant is not one of the keys, so it is shown beside the
            // hash rather than taken out of it — and every key is shown with
            // whether it has to be passed, which is the whole shape of an
            // `args` list.
            let mut parts: Vec<String> = invocant.iter().cloned().collect();
            let named: Vec<String> = params
                .iter()
                .map(|param| {
                    let optional = if param.optional { "?" } else { "" };
                    format!(
                        "{}{optional} => {}",
                        param.name.trim_start_matches('$'),
                        param.ty
                    )
                })
                .collect();
            out.push('(');
            parts.push(format!("{{ {} }}", named.join(", ")));
            out.push_str(&parts.join(", "));
            out.push(')');
        }
    }
    let returns = &symbol.returns;
    let scalar = (!returns.scalar.is_unknown()).then(|| returns.scalar.to_string());
    // A list of one holding the scalar half says nothing the scalar half did
    // not, and every `return $x` is one: shown, it would be noise on most of a
    // codebase.
    let list = match &returns.list {
        annotate::ListShape::Fixed(types) if types.as_slice() == [returns.scalar.clone()] => None,
        shape => shape.written(),
    };
    let said = match (scalar, list) {
        // `Returns: ()` is one statement about both contexts, so it is shown
        // as the one thing it is rather than as `Undef, ()`.
        (_, Some(list)) if returns.list == annotate::ListShape::Nothing => {
            out.push_str(&format!(" -> {list}"));
            true
        }
        // The two halves as they are written: a scalar type, a list shape, or
        // both — which is two `Returns:` lines and one signature.
        (Some(scalar), Some(list)) => {
            out.push_str(&format!(" -> {scalar}, {list}"));
            true
        }
        (Some(scalar), None) => {
            out.push_str(&format!(" -> {scalar}"));
            true
        }
        (None, Some(list)) => {
            out.push_str(&format!(" -> {list}"));
            true
        }
        (None, None) => false,
    };
    // Said once, after both halves: the reader is being told where the type
    // came from, and a sub with two inferred halves has one answer to that.
    if returns.inferred && said {
        out.push_str(" (inferred)");
    }
    out
}

fn render_param(param: &Param) -> String {
    let mut text = param.name.clone();
    if param.optional {
        text.push('?');
    }
    if !param.ty.is_unknown() {
        text.push_str(" : ");
        text.push_str(&param.ty.to_string());
    }
    text
}

/// What a sub's shape says about the arguments it takes.
///
/// Four recognisers, in the order the design document lists them. The first
/// that matches wins, and no match is `Unknown` — which is never reported
/// against.
#[must_use]
pub fn parameters(definition: &SubDef, smart_args: bool, into: &mut annotate::Sink) -> Params {
    let is_method = definition.is_method();
    let params = written_parameters(definition, smart_args, is_method, into);
    if is_method {
        with_implicit_invocant(params)
    } else {
        params
    }
}

/// The parameter list as the declaration writes it, invocant aside.
fn written_parameters(
    definition: &SubDef,
    smart_args: bool,
    is_method: bool,
    into: &mut annotate::Sink,
) -> Params {
    if let Some(signature) = definition.signature() {
        // GUESS: `sub f()` with a body that reads `@_` was a prototype.
        // Evidence: the body. An empty `()` is a signature only where the
        // feature is on, and is otherwise a prototype saying "call me with no
        // arguments" that a method still receives `$self` through —
        // `Mail::Internet::cleaned_header_dup()` shifts its invocant out of
        // one. Wrong: a real empty signature whose body reads `@_` anyway,
        // which perl would have made unreachable.
        //
        // A `method` is not a guess: the keyword exists only where the
        // `class` feature is on, and there `()` is a signature and never a
        // prototype.
        let empty = signature.params().next().is_none();
        let reads_arguments = definition
            .body()
            .is_some_and(|body| touches_arguments_elsewhere(&body, &[]));
        if is_method || !(empty && reads_arguments) {
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

/// The invocant a `method` has and never names.
///
/// perl hands a `method` its invocant and keeps it out of `@_`, so nothing in
/// the declaration mentions it — while a call still passes it. A positional
/// list counts the invocant (see [`Params::Positional`]), so leaving it out
/// would have the two sides counting different things: `$obj->f` is one
/// argument against a `method f()` that declares none.
fn with_implicit_invocant(params: Params) -> Params {
    match params {
        Params::Unknown => Params::Unknown,
        Params::Positional {
            mut params,
            slurpy,
            source,
            ..
        } => {
            params.insert(
                0,
                Param {
                    name: "$self".to_string(),
                    optional: false,
                    // A signature says nothing about types, and this one is
                    // not even written down.
                    ty: Type::Any,
                },
            );
            Params::Positional {
                params,
                slurpy,
                invocant: true,
                source,
            }
        }
        Params::Named { params, source, .. } => Params::Named {
            params,
            invocant: Some("$self".to_string()),
            source,
        },
    }
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
    display == "$self" || display == "$class" || display == PLACEHOLDER
}

/// The name a discarded slot is given: `my (undef, $name) = @_` takes the
/// invocant and throws it away, and the slot is still a parameter.
///
/// It carries no sigil, which is what keeps it out of the body's environment —
/// [`crate::flow`]'s `bind` binds nothing under a name that has none.
pub const PLACEHOLDER: &str = "undef";

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
    let mut invocant = None;
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
        if is_invocant {
            invocant = Some(variable.clone());
            // A named list is keys, and the invocant is not one of them; a
            // positional list counts it, because a method call passes it.
            if !positional {
                continue;
            }
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
            invocant: invocant.is_some(),
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

/// Whether a hand-written `new` says the value it hands back is one of its
/// own class (`docs/types.md`, INFER-2g).
///
/// The declaration pass reads a body for one thing only, the sub's own
/// parameter list, and this is the second: still a fact about the sub rather
/// than about the program, and asked of `new` alone.
///
/// The evidence is a `bless` — into whatever class, since `bless $self,
/// $class` is how every constructor written for subclassing spells its own —
/// or a `SUPER::`, which is a constructor borrowing its parent's and getting
/// back what the parent blessed. A `new` with neither is a factory as easily
/// as a constructor: `URI::new` ends `return $impclass->_init(...)` and hands
/// back a `URI::http`, and calling it a `URI` made every method after it
/// missing.
fn constructs_its_class(definition: &SubDef) -> bool {
    let Some(body) = definition.body() else {
        // A forward declaration says nothing either way, and saying nothing is
        // what the old reading did.
        return true;
    };
    // Neither does an empty one. A stub writes `sub new ($class, $fields =
    // undef) {}` because a stub declares an interface and not a body
    // (`docs/types.md`, ANNOT-9); reading that as "no evidence, so not a
    // constructor" took the type off every class that inherits its `new`.
    if body.statements().next().is_none() {
        return true;
    }
    body.syntax().descendants_with_tokens().any(|element| {
        element
            .as_token()
            .is_some_and(|token| token.token_kind() == TokenKind::IDENT && token.text() == "bless")
            || element
                .as_node()
                .is_some_and(|node| node.node_kind() == NodeKind::SUB_NAME && is_super(node))
    })
}

/// `SUPER::new` and the rest — a call into the parent's method of that name.
fn is_super(node: &SyntaxNode) -> bool {
    node.text().to_string().starts_with("SUPER::")
}

/// What each package in a file imports, by the package it is written in.
///
/// The package tracking is the declaration walk's, so that `package Foo {
/// ... }` scopes to its block and `package Foo;` runs to the next one — which
/// is what perl does with the import itself.
fn collect_frameworks(
    node: &SyntaxNode,
    outer: &str,
    dialect: &Dialect,
    into: &mut HashMap<String, Frameworks>,
) {
    let mut package = outer.to_string();
    for child in node.children() {
        match child.node_kind() {
            NodeKind::PACKAGE_STMT => {
                let Some(statement) = ast::PackageStmt::cast(child.clone()) else {
                    continue;
                };
                let Some(name) = statement.name() else {
                    continue;
                };
                into.entry(name.clone())
                    .or_insert_with(|| Frameworks::with_dialect(dialect.clone()));
                match statement.block() {
                    Some(block) => collect_frameworks(block.syntax(), &name, dialect, into),
                    None => package = name,
                }
            }
            NodeKind::USE_STMT => {
                let Some(statement) = ast::UseStmt::cast(child.clone()) else {
                    continue;
                };
                let Some(module) = statement.module() else {
                    continue;
                };
                let frameworks = into
                    .entry(package.clone())
                    .or_insert_with(|| Frameworks::with_dialect(dialect.clone()));
                frameworks.note(&module);
                // `use base 'Class::Accessor'` names the framework in its
                // arguments, and `use Class::Accessor 'antlers'` is the
                // spelling that has `has`.
                if let Some(arguments) = statement.arguments() {
                    frameworks.note_arguments(&module, &imported_names(&arguments));
                }
            }
            _ => collect_frameworks(&child, &package, dialect, into),
        }
    }
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
    let node = ast::without_plus(node);
    if node.node_kind() == NodeKind::ANON_HASH {
        let hash = AnonHash::cast(node.clone()).expect("kind checked");
        let mut optional = false;
        let mut annotation = None;
        for pair in hash.pairs() {
            match pair.key() {
                Some("isa") => annotation = Some(annotation_of(pair.node())),
                // `optional => 0` is the one spelling that says the opposite;
                // anything this cannot read as a number is taken at its word.
                Some("optional") => optional = crate::annotate::is_true(pair.node()),
                Some("default") | Some("builder") => optional = true,
                _ => {}
            }
        }
        return (optional, annotation.flatten());
    }
    (false, annotation_of(&node))
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
    for slot in unpacked_slots(&declaration) {
        match slot {
            Some(target) if target.sigil() == Sigil::Scalar => names.push(target.display()),
            Some(_) => slurpy = true,
            None => names.push(PLACEHOLDER.to_string()),
        }
    }
    Some((names, slurpy))
}

/// The slots of `my (...) = @_`, in order and including the discarded ones.
///
/// [`VarDecl::targets`] answers "which variables", which is the wrong question
/// here: `my (undef, $name) = @_` binds one name and takes two arguments, and
/// dropping the `undef` moves `$name` into the invocant's place and loses a
/// parameter — `File::DesktopEntry::lookup` is written this way and its arity
/// was off by one.
fn unpacked_slots(declaration: &VarDecl) -> Vec<Option<ast::Variable>> {
    fn walk(node: &SyntaxNode, acc: &mut Vec<Option<ast::Variable>>) {
        for child in node.children() {
            if let Some(variable) = ast::Variable::cast(child.clone()) {
                acc.push(Some(variable));
            } else if child.node_kind() == NodeKind::UNDEF_EXPR {
                acc.push(None);
            } else {
                walk(&child, acc);
            }
        }
    }
    let Some(target) = ast::child::<ast::DeclTarget>(declaration.syntax()) else {
        return Vec::new();
    };
    let mut acc = Vec::new();
    walk(target.syntax(), &mut acc);
    acc
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
        // `"Header Error: $_[0]"` reads `@_` as surely as the bare form does,
        // and the lexer hands the whole string over as one token — so the
        // interpolation scanner is what sees it (`docs/typecheck.md`,
        // "Scopes"). `IO::Uncompress::Base::HeaderError` is written this way,
        // and its whole family was reported as taking no arguments.
        if ast::tokens(&node).any(|token| interpolates_arguments(&token)) {
            return true;
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

/// Whether a quoted construct interpolates `@_`.
///
/// Over-eager on purpose: a construct this misreads makes the parameter list
/// `Unknown`, which is the quiet answer.
fn interpolates_arguments(token: &SyntaxToken) -> bool {
    let text = match token.token_kind() {
        // A single-quoted string is the same token kind and interpolates
        // nothing.
        TokenKind::STRING => {
            if token.text().starts_with('\'') {
                return false;
            }
            token.text()
        }
        TokenKind::INTERPOLATED_STRING | TokenKind::REGEX_PATTERN | TokenKind::HEREDOC_CONTENT => {
            token.text()
        }
        _ => return false,
    };
    crate::interp::scan(text)
        .iter()
        .any(|found| found.sigil == Sigil::Array && found.name == "_")
}
