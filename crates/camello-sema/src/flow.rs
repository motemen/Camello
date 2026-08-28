//! Type flow (`docs/typecheck.md`, "Inference" and "Diagnostics").
//!
//! Local, forward, and it gives up early. Inference exists for one reason: to
//! give the annotated parts something to check against without asking the user
//! to annotate everything first — so every rule here is written to reach
//! [`Type::Unknown`] rather than to reach an answer, and `Unknown` is never
//! reported against.
//!
//! Four diagnostics come out of it:
//!
//! - `type-mismatch` — a value whose shape contradicts the slot it goes in.
//!   An `error` when the value is a literal or a declaration (the two sides
//!   are both written down) and a `warning` when it is inferred.
//! - `unknown-key` — a key passed to a constructor that declares no such
//!   attribute, or read off a restricted `Dict`.
//! - `unknown-method` — a method on a class that declares none such and has no
//!   unknown ancestor. A `warning`: the class might still be right and the
//!   program wrong about which class it holds.
//! - `maybe-deref` — a `Maybe[...]` used without a narrowing check. The most
//!   useful diagnostic and the most likely false positive, so it is a
//!   `warning` and the narrowing set below is a list rather than a theorem.

use std::collections::HashMap;

use camello_syntax::ast::{self, AstNode, DeclKeyword, Sigil, Variable};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode, TokenExt, TokenKind};
use rowan::{TextRange, TextSize};

use crate::annotate::{ListShape, Returns};
use crate::decl::{Param, ParamSource, Params};
use crate::diag::{Code, Diagnostic, Severity};
use crate::program::{MethodLookup, Program};
use crate::types::Type;

/// Check one file's bodies against everything the program declares.
#[must_use]
pub fn analyse(root: &SyntaxNode, file: usize, program: &Program) -> Vec<Diagnostic> {
    run(root, file, program, false).0
}

/// The same walk, keeping the type it inferred for every expression it typed
/// (`docs/lsp.md`, "The type side-table").
///
/// Off for the CLI and on for an editor's per-file pass: the pass computes
/// these types either way, and the only question is whether they are written
/// down or dropped. One table backs both hover and completion.
#[must_use]
pub fn analyse_recording(
    root: &SyntaxNode,
    file: usize,
    program: &Program,
) -> (Vec<Diagnostic>, TypeTable) {
    let (diagnostics, table) = run(root, file, program, true);
    (diagnostics, table.unwrap_or_default())
}

fn run(
    root: &SyntaxNode,
    file: usize,
    program: &Program,
    record: bool,
) -> (Vec<Diagnostic>, Option<TypeTable>) {
    let mut pass = Pass {
        program,
        file,
        env: Env::default(),
        diagnostics: Vec::new(),
        package: "main".to_string(),
        returns: Returns::default(),
        record: record.then(TypeTable::default),
        infer: None,
    };
    pass.block(root);
    pass.check_annotations();
    (pass.diagnostics, pass.record)
}

/// What the subs named by `only` return, read off their bodies
/// (`docs/return-inference.md`).
///
/// `only` indexes `program.file(file).decls.subs`, and what comes back names
/// the same indexes — the subs that *became known*, so an empty answer is
/// what says a round of the fixpoint changed nothing.
///
/// The same walk as [`analyse`], because the types have to be the ones the
/// checker will read at the call site; the diagnostics it produces on the way
/// are dropped, because the checking pass reports them and an inferred return
/// is never a check on the body it came from (`docs/types.md`, ANNOT-7a).
#[must_use]
pub fn infer_returns(
    root: &SyntaxNode,
    file: usize,
    program: &Program,
    only: &[usize],
) -> Vec<(usize, Returns)> {
    let Some(entry) = program.file(file) else {
        return Vec::new();
    };
    // By name rather than by position, for the reason the language server's
    // decl fingerprint leaves position out: an edit that moves a sub down a
    // line changes no declaration, so the graph is allowed to go on holding
    // the range it had before that edit — and a walk keyed by that range
    // would then match nothing. First wins, the way every other name lookup
    // here reads a redefinition.
    let mut wanted: HashMap<(String, String), usize> = HashMap::new();
    for index in only {
        if let Some(symbol) = entry.decls.subs.get(*index) {
            wanted
                .entry((symbol.package.clone(), symbol.name.clone()))
                .or_insert(*index);
        }
    }
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut pass = Pass {
        program,
        file,
        env: Env::default(),
        diagnostics: Vec::new(),
        package: "main".to_string(),
        returns: Returns::default(),
        record: None,
        infer: Some(Box::new(Inference {
            wanted,
            ..Inference::default()
        })),
    };
    pass.block(root);
    pass.infer.expect("set above").found
}

/// What the walk inferred, kept by range.
///
/// Ranges nest, the way expressions do, and nothing here flattens them: the
/// answer to "what is the type *here*" is the innermost range that has one,
/// which is what [`TypeTable::at`] returns.
#[derive(Debug, Default, Clone)]
pub struct TypeTable {
    /// Every expression the pass typed, in the order it typed them.
    pub types: Vec<(TextRange, Type)>,
    /// Every `->` whose receiver named a class the run knows.
    pub methods: Vec<MethodSite>,
}

/// One resolved `->` call site.
#[derive(Debug, Clone)]
pub struct MethodSite {
    /// The invocant expression: `$obj` in `$obj->name(...)`.
    pub receiver: TextRange,
    /// The method name alone.
    pub method_range: TextRange,
    /// The class the receiver was resolved to.
    pub class: String,
    pub method: String,
    /// The package the call is *written* in, which is what `SUPER::` is
    /// relative to — so a reader of this table resolves the method the same
    /// way the pass did.
    pub from: String,
}

impl TypeTable {
    /// The innermost type known at an offset, or `None` where the checker
    /// knows nothing.
    ///
    /// `Unknown` is not an answer — it is the checker saying it did not
    /// analyse this — so it is skipped rather than returned, which is what
    /// makes hover silent instead of shrugging (`docs/lsp.md`, "Hover").
    #[must_use]
    pub fn at(&self, offset: TextSize) -> Option<(TextRange, &Type)> {
        self.types
            .iter()
            .filter(|(range, ty)| !ty.is_unknown() && range.contains_inclusive(offset))
            .min_by_key(|(range, _)| range.len())
            .map(|(range, ty)| (*range, ty))
    }

    /// The type recorded for exactly this range, latest first.
    ///
    /// Completion asks this way: it has found the receiver by walking tokens,
    /// so it knows the range and wants that expression's own type rather than
    /// whatever encloses it.
    #[must_use]
    pub fn of(&self, range: TextRange) -> Option<&Type> {
        self.types
            .iter()
            .rev()
            .find(|(recorded, ty)| *recorded == range && !ty.is_unknown())
            .map(|(_, ty)| ty)
    }

    /// The `->` call site whose method name covers an offset.
    #[must_use]
    pub fn method_at(&self, offset: TextSize) -> Option<&MethodSite> {
        self.methods
            .iter()
            .find(|site| site.method_range.contains_inclusive(offset))
    }
}

/// What each lexical holds, as far as the walk has got.
#[derive(Debug, Clone, Default)]
struct Env {
    vars: HashMap<(Sigil, String), Type>,
}

impl Env {
    fn get(&self, sigil: Sigil, name: &str) -> Type {
        self.vars
            .get(&(sigil, name.to_string()))
            .cloned()
            .unwrap_or(Type::Unknown)
    }

    fn set(&mut self, sigil: Sigil, name: &str, ty: Type) {
        self.vars.insert((sigil, name.to_string()), ty);
    }

    /// What both branches agree on, which is the union of what each says.
    fn join(&mut self, other: &Env) {
        for (key, ty) in &other.vars {
            let merged = match self.vars.get(key) {
                Some(mine) => Type::union(vec![mine.clone(), ty.clone()]),
                None => ty.clone(),
            };
            self.vars.insert(key.clone(), merged);
        }
    }
}

/// A call's arguments, each typed exactly once.
///
/// [`Pass::type_of`] walks the whole subtree under the node it is handed, so
/// asking it a second time for the same argument is not a lookup — it is the
/// walk again. A Perl list operator swallows everything to its right, so
/// `type Row => as Dict ['a' => header Str, 'b' => header Str, ...]` is not a
/// flat list of a hundred entries but a hundred nested calls, and a second
/// walk per level costs 2^depth. That is what turned a 100-key `Dict` from a
/// file that takes a moment into a run that does not end.
struct Typed<'a> {
    nodes: &'a [SyntaxNode],
    types: Vec<Type>,
}

impl<'a> Typed<'a> {
    /// The type of the argument in position `index`.
    fn nth(&self, index: usize) -> Type {
        self.types.get(index).cloned().unwrap_or(Type::Unknown)
    }

    /// The type of one of the argument nodes, found by identity: the value of
    /// a `key => value` pair is one of the elements, so a caller holding
    /// [`ast::Arg`]s reaches its type through here rather than typing it again.
    fn of(&self, node: &SyntaxNode) -> Option<Type> {
        self.nodes
            .iter()
            .position(|argument| argument == node)
            .map(|index| self.nth(index))
    }
}

struct Pass<'a> {
    program: &'a Program,
    file: usize,
    env: Env,
    diagnostics: Vec<Diagnostic>,
    package: String,
    /// What the sub being walked said it returns.
    returns: Returns,
    /// Where the inferred types go when an editor asked for them, and `None`
    /// when nobody did (`docs/lsp.md`, "The type side-table").
    record: Option<TypeTable>,
    /// Where the return walk collects, and `None` when the pass is checking
    /// bodies rather than reading returns off them.
    infer: Option<Box<Inference>>,
}

/// The return walk's state (`docs/return-inference.md`, "Sites").
#[derive(Debug, Default)]
struct Inference {
    /// The subs the walk was asked about, by package and name.
    wanted: HashMap<(String, String), usize>,
    /// What each of them turned out to return.
    found: Vec<(usize, Returns)>,
    /// The sites of the sub being walked — reset on entry to a sub, so that a
    /// `return` inside a callback is the callback's.
    sites: Sites,
    /// What the statement just walked leaves as the sub's value.
    tail: Tail,
    /// How the sub being walked names the value it was called on.
    invocant: Invocant,
}

/// Every place a value leaves one sub, as the walk collected them.
#[derive(Debug, Default)]
struct Sites {
    /// The scalar type of each.
    scalar: Vec<Type>,
    /// Whether one of them was the invocant.
    invocant: bool,
    /// A `goto` hands the call over to another sub, and what comes back is
    /// that sub's answer to a question this walk never asked.
    opaque: bool,
}

impl Sites {
    /// The join of every site, and `Unknown` if any site is
    /// (`docs/return-inference.md`, "What is being built").
    ///
    /// Not a precision choice but the reason the feature can be shipped at
    /// all: a partial join — `Str` from the two sites that were typed,
    /// ignoring the third — is a type the program does not have, and it would
    /// be reported at every call site.
    fn joined(&self, tail: &Tail, package: &str) -> Returns {
        if self.opaque {
            return Returns::default();
        }
        let mut members = self.scalar.clone();
        let mut invocant = self.invocant;
        match tail {
            Tail::Value { ty, invocant: tail } => {
                members.push(ty.clone());
                invocant |= tail;
            }
            // A `return` or a `die`: counted already, or never read.
            Tail::Left => {}
            // A loop, a bare block, a `package`, a nested `sub`, an empty
            // body — and an `if` chain with no `else`, whose false value is
            // its condition's.
            Tail::Opaque => members.push(Type::Unknown),
        }
        let scalar = Type::union(members);
        Returns::inferred(
            scalar.clone(),
            invocant && holds_own_class(&scalar, package),
        )
    }
}

/// What the statement just walked leaves as the value of the sub it is in.
///
/// The tail is a site because `sub name { $_[0]->{name} }` is how half the
/// accessors in a corpus are written, and it is what makes a tail-only setter
/// — `sub set_x { $_[0]->{x} = $_[1] }` — return what it was assigned, which
/// is what perl does.
#[derive(Debug, Clone, Default)]
enum Tail {
    /// An expression statement, and this is what it evaluated to — read as a
    /// site, so the invocant marker is the tail's too: `sub build { my $self
    /// = shift; ...; $self }` is the same builder as the one that writes the
    /// `return` out.
    Value { ty: Type, invocant: bool },
    /// A `return` or a `die`, whose site the walk has counted already.
    Left,
    /// Anything else, including a body with no statements in it.
    #[default]
    Opaque,
}

/// How the sub being walked names the value it was called on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Invocant {
    /// Its parameter list does not say it is a method.
    #[default]
    None,
    /// `$self`, or whatever the first parameter is called, without the sigil.
    Named(String),
    /// The sub unpacks nothing, so `$_[0]` is the invocant. This is the half
    /// of the rule the corpus is expected to argue with: a sub that hands
    /// back `$_[0]` and is *not* a method is told it returns an instance of
    /// the package it was written in.
    Implicit,
}

impl Invocant {
    fn of(params: &Params) -> Self {
        match params {
            Params::Positional {
                params,
                invocant: true,
                ..
            } => params.first().map_or(Invocant::None, |param| {
                Invocant::Named(param.name.trim_start_matches('$').to_string())
            }),
            // `args` binds the invocant under that name and no other.
            Params::Named { invocant: true, .. } => Invocant::Named("self".to_string()),
            Params::Unknown => Invocant::Implicit,
            _ => Invocant::None,
        }
    }
}

impl Pass<'_> {
    /// Every class named in an annotation, against what the program declares.
    ///
    /// The Str-as-class reading makes a typo in a type name (`'Srt'`) into an
    /// `InstanceOf['Srt']` — resolvable to nothing, hence `Unknown`, hence
    /// silent. This is what catches it, at the cost of firing on every class
    /// from a dependency the run could not resolve, which is why it is `info`
    /// (`docs/typecheck.md`, "Open questions").
    fn check_annotations(&mut self) {
        let Some(entry) = self.program.file(self.file) else {
            return;
        };
        for annotated in &entry.decls.annotations {
            let mut unknown = Vec::new();
            collect_classes(&annotated.ty, &mut unknown);
            for name in unknown {
                if self.program.knows_package(&name) {
                    continue;
                }
                self.diagnostics.push(Diagnostic::new(
                    Code::UnknownType,
                    annotated.range,
                    format!("type or class `{name}` is not known to the program"),
                ));
            }
        }
    }

    // ----- statements -----

    fn block(&mut self, node: &SyntaxNode) {
        for child in node.children() {
            self.statement(&child);
        }
    }

    fn statement(&mut self, node: &SyntaxNode) {
        // Every statement says what it leaves behind; the ones that leave a
        // value say so below, and everything else is opaque by not saying.
        self.set_tail(Tail::Opaque);
        match node.node_kind() {
            NodeKind::PACKAGE_STMT => {
                if let Some(name) =
                    ast::PackageStmt::cast(node.clone()).and_then(|view| view.name())
                {
                    self.package = name;
                }
                if let Some(block) = ast::PackageStmt::cast(node.clone()).and_then(|v| v.block()) {
                    self.block(block.syntax());
                }
            }
            NodeKind::SUB_DEF => self.sub(node),
            NodeKind::VAR_DECL_STMT | NodeKind::EXPR_STMT => {
                self.expression_statement(node);
            }
            NodeKind::IF_STMT => self.if_statement(node),
            NodeKind::LOOP_STMT => self.loop_statement(node),
            NodeKind::BLOCK => {
                let saved = self.env.clone();
                self.block(node);
                self.env = saved;
            }
            _ => {
                for child in node.children() {
                    self.statement(&child);
                }
            }
        }
    }

    fn sub(&mut self, node: &SyntaxNode) {
        let definition = ast::SubDef::cast(node.clone()).expect("kind checked");
        let Some(body) = definition.body() else {
            return;
        };
        let saved = std::mem::take(&mut self.env);
        let saved_returns = std::mem::take(&mut self.returns);

        // The sub's own parameters are the one place a body starts with types
        // rather than earning them. Asked of this file first: the body being
        // walked is this file's, and so are the annotations that type it
        // (`Program::sub_in`).
        let mut invocant = Invocant::None;
        if let Some(symbol) = definition
            .name_text()
            .and_then(|name| self.program.sub_in(self.file, &self.package, &name))
        {
            self.returns = symbol.returns.clone();
            invocant = Invocant::of(&symbol.params);
            bind_params(&mut self.env, &symbol.params, &self.package);
        }
        let sites = self.enter_sub(invocant);
        self.block(body.syntax());
        self.leave_sub(definition.name_text(), sites);
        self.env = saved;
        self.returns = saved_returns;
    }

    /// Put the enclosing sub's sites aside: a `return` in here is this sub's.
    fn enter_sub(&mut self, invocant: Invocant) -> Option<Box<(Sites, Tail, Invocant)>> {
        let inference = self.infer.as_mut()?;
        Some(Box::new((
            std::mem::take(&mut inference.sites),
            std::mem::take(&mut inference.tail),
            std::mem::replace(&mut inference.invocant, invocant),
        )))
    }

    /// Apply the site table to what the body left, and give the enclosing sub
    /// its sites back.
    ///
    /// `name` is the sub's own, which is how the declaration pass named it;
    /// `None` is an anonymous sub, which nothing asked about.
    fn leave_sub(&mut self, name: Option<String>, saved: Option<Box<(Sites, Tail, Invocant)>>) {
        let Some(saved) = saved else { return };
        let package = self.package.clone();
        let key = name.map(|name| (package.clone(), name));
        let inference = self.infer.as_mut().expect("saved implies collecting");
        if let Some(index) = key.and_then(|key| inference.wanted.get(&key).copied()) {
            let returns = inference.sites.joined(&inference.tail, &package);
            // Only what became known: an answer of `Unknown` is the round
            // saying it has nothing to install, and installing it would make
            // every round look like progress.
            if !returns.is_unresolved() {
                inference.found.push((index, returns));
            }
        }
        let (sites, tail, invocant) = *saved;
        inference.sites = sites;
        inference.tail = tail;
        inference.invocant = invocant;
    }

    /// Note what the statement just walked leaves as the sub's value.
    fn set_tail(&mut self, tail: Tail) {
        if let Some(inference) = &mut self.infer {
            inference.tail = tail;
        }
    }

    fn expression_statement(&mut self, node: &SyntaxNode) {
        let mut value = None;
        for child in node.children() {
            let ty = self.type_of(&child);
            if child.node_kind() != NodeKind::STMT_MODIFIER {
                value = Some((child, ty));
            }
        }
        // A guard narrows what follows it: `return unless defined $x;` is how
        // half the corpus turns a `Maybe` into a value.
        self.apply_guard(node);
        if self.infer.is_some() {
            // A modified statement falls through to the value of its own
            // condition when the condition does not hold, so `$h{k} = 1 if
            // $ok` is not a tail this walk can read — and neither is `return
            // 1 if $ok`, whose `return` is a site of its own regardless.
            let tail = if node
                .children()
                .any(|child| child.node_kind() == NodeKind::STMT_MODIFIER)
            {
                Tail::Opaque
            } else if leaves_the_sub(node) {
                Tail::Left
            } else {
                match value {
                    Some((expression, ty)) => {
                        let (ty, invocant) = self.read_site(&expression, ty);
                        Tail::Value { ty, invocant }
                    }
                    None => Tail::Opaque,
                }
            };
            self.set_tail(tail);
        }
    }

    fn if_statement(&mut self, node: &SyntaxNode) {
        let before = self.env.clone();
        let mut after: Option<Env> = None;
        let mut condition_seen = false;
        // What a branch below this condition starts from. The block that
        // follows a condition runs where it held; everything after runs where
        // it did not, and reading both sides off one condition is what makes
        // `unless (defined $x) { ... } else { ... }` narrow the `else`.
        let mut otherwise = before.clone();
        let negated = ast::tokens(node).any(|token| token.token_kind() == TokenKind::UNLESS_KW);
        // What each branch leaves as the sub's value, for the tail of a chain
        // that is the last statement of a body.
        let mut tails: Vec<Tail> = Vec::new();
        let mut has_else = false;

        for child in node.children() {
            match child.node_kind() {
                NodeKind::BLOCK => {
                    self.set_tail(Tail::Opaque);
                    self.block(&child);
                    tails.extend(self.tail());
                    let ended = std::mem::replace(&mut self.env, otherwise.clone());
                    match &mut after {
                        Some(env) => env.join(&ended),
                        None => after = Some(ended),
                    }
                }
                NodeKind::ELSIF_CLAUSE | NodeKind::ELSE_CLAUSE => {
                    has_else |= child.node_kind() == NodeKind::ELSE_CLAUSE;
                    self.env = otherwise.clone();
                    let mut clause_seen = false;
                    for inner in child.children() {
                        if inner.node_kind() == NodeKind::BLOCK {
                            self.set_tail(Tail::Opaque);
                            self.block(&inner);
                            tails.extend(self.tail());
                            let ended = std::mem::replace(&mut self.env, otherwise.clone());
                            match &mut after {
                                Some(env) => env.join(&ended),
                                None => after = Some(ended),
                            }
                        } else {
                            self.expression(&inner);
                            // An `elsif` carries a condition of its own, and
                            // it narrows the block it belongs to.
                            if !clause_seen && child.node_kind() == NodeKind::ELSIF_CLAUSE {
                                clause_seen = true;
                                let facts = narrowing(&self.env, &inner);
                                let mut yes = self.env.clone();
                                facts.apply_true(&mut yes);
                                facts.apply_false(&mut otherwise);
                                self.env = yes;
                            }
                        }
                    }
                }
                _ if !condition_seen => {
                    condition_seen = true;
                    self.expression(&child);
                    let facts = narrowing(&self.env, &child);
                    let (mut yes, mut no) = (before.clone(), before.clone());
                    facts.apply_true(&mut yes);
                    facts.apply_false(&mut no);
                    // `unless` runs its block where the condition did *not*
                    // hold, which is the other side of the same read.
                    if negated {
                        std::mem::swap(&mut yes, &mut no);
                    }
                    self.env = yes;
                    otherwise = no;
                }
                _ => self.expression(&child),
            }
        }

        // Joining what each branch left with what came in is the "branches
        // joined" of the design; a branch that did not run leaves `before`.
        if let Some(mut ended) = after {
            ended.join(&before);
            self.env = ended;
        } else {
            self.env = before;
        }
        if self.infer.is_some() {
            self.set_tail(join_tails(&tails, has_else));
        }
    }

    /// What the statement just walked left, for a caller collecting branches.
    fn tail(&self) -> Option<Tail> {
        self.infer.as_ref().map(|inference| inference.tail.clone())
    }

    fn loop_statement(&mut self, node: &SyntaxNode) {
        let saved = self.env.clone();
        for child in node.children() {
            match child.node_kind() {
                NodeKind::BLOCK => self.block(&child),
                NodeKind::FOREACH_HEADER => {
                    // `foreach my $x (@list)` binds the element type.
                    let element = child
                        .children()
                        .find(|inner| inner.node_kind() != NodeKind::VAR_DECL)
                        .map(|inner| element_of(&self.type_of(&inner)))
                        .unwrap_or(Type::Unknown);
                    if let Some(declaration) = child.children().find_map(ast::VarDecl::cast) {
                        for target in declaration.targets() {
                            if let Some(name) = target.name() {
                                self.env.set(target.sigil(), &name, element.clone());
                            }
                        }
                    }
                }
                _ => self.expression(&child),
            }
        }
        // A loop is widened to the join of its body: whatever it left behind
        // may or may not have happened.
        let ended = std::mem::replace(&mut self.env, saved);
        self.env.join(&ended);
    }

    /// `return unless $x;`, `$x or die;`, `die unless defined $x;` — a
    /// statement whose whole job is to leave `$x` defined below it.
    fn apply_guard(&mut self, statement: &SyntaxNode) {
        let text_has = |kind: TokenKind| {
            statement
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .any(|token| token.token_kind() == kind)
        };
        let leaves = statement.descendants().any(|node| {
            ast::Call::cast(node).is_some_and(|call| {
                matches!(
                    call.callee_name().as_deref(),
                    Some("return" | "die" | "croak" | "confess" | "next" | "last")
                )
            })
        });
        if !leaves {
            return;
        }
        // `return unless COND` and `COND or return` both mean "below here,
        // COND held"; `return if COND` means the opposite of it did. Which
        // part of the statement is the condition depends on which of the
        // three it is, and reading the whole statement instead — as a flat
        // scan of every variable in it did — narrows on the strength of names
        // the guard never tested.
        let (condition, held) = if text_has(TokenKind::UNLESS_KW) {
            (modifier_condition(statement, TokenKind::UNLESS_KW), true)
        } else if text_has(TokenKind::IF_KW) {
            (modifier_condition(statement, TokenKind::IF_KW), false)
        } else if text_has(TokenKind::OR_KW) || text_has(TokenKind::LOGICAL_OR) {
            (leaving_alternative(statement), true)
        } else {
            (None, true)
        };
        if let Some(condition) = condition {
            let facts = narrowing(&self.env, &condition);
            if held {
                facts.apply_true(&mut self.env);
            } else {
                facts.apply_false(&mut self.env);
            }
        }
    }

    // ----- expressions -----

    fn expression(&mut self, node: &SyntaxNode) {
        let _ = self.type_of(node);
    }

    /// The type of an expression in scalar context, checking what it holds on
    /// the way down.
    ///
    /// The recording is here rather than at each arm because this is the one
    /// door every expression goes through, so a table built here covers
    /// exactly what the pass typed and cannot drift from it.
    fn type_of(&mut self, node: &SyntaxNode) -> Type {
        let ty = self.infer(node);
        if let Some(table) = &mut self.record {
            table.types.push((node.text_range(), ty.clone()));
        }
        ty
    }

    fn infer(&mut self, node: &SyntaxNode) -> Type {
        match node.node_kind() {
            NodeKind::LITERAL => literal_type(node),
            NodeKind::Q_EXPR | NodeKind::QQ_EXPR | NodeKind::HEREDOC_EXPR | NodeKind::QX_EXPR => {
                Type::Str
            }
            NodeKind::QR_EXPR => Type::RegexpRef,
            NodeKind::ANON_SUB_EXPR => {
                if let Some(body) = ast::AnonSubExpr::cast(node.clone()).and_then(|v| v.body()) {
                    let saved = self.env.clone();
                    // A `return` in here is this sub's, not the enclosing
                    // one's, and nothing annotates an anonymous sub — so the
                    // `Returns:` above the sub it is written in has to be put
                    // down before the body is walked. Left standing, `sub f {
                    // my $cb = sub { return [1] }; ... }` reported the
                    // callback's `return` against `f`'s declared type.
                    let saved_returns = std::mem::take(&mut self.returns);
                    let sites = self.enter_sub(Invocant::None);
                    self.block(body.syntax());
                    self.leave_sub(None, sites);
                    self.returns = saved_returns;
                    self.env = saved;
                }
                Type::CodeRef
            }
            NodeKind::ANON_ARRAY => {
                let view = ast::AnonArray::cast(node.clone()).expect("kind checked");
                let members: Vec<Type> = view
                    .elements()
                    .iter()
                    .map(|element| self.list_element(element))
                    .collect();
                if members.is_empty() {
                    Type::ArrayRef(Box::new(Type::Unknown))
                } else {
                    Type::ArrayRef(Box::new(Type::union(members)))
                }
            }
            NodeKind::ANON_HASH => self.anon_hash(node),
            // `+{ ... }` is the hashref it wraps, and `+(...)` the list.
            NodeKind::PREFIX_EXPR if ast::without_plus(node) != *node => {
                self.type_of(&ast::without_plus(node))
            }
            NodeKind::REFERENCE_EXPR => {
                let inner = node.children().next();
                match inner
                    .as_ref()
                    .and_then(|inner| Variable::cast(inner.clone()))
                {
                    Some(variable) => {
                        let name = variable.name().unwrap_or_default();
                        let inner_type = self.env.get(variable.sigil(), &name);
                        match variable.sigil() {
                            Sigil::Array => Type::ArrayRef(Box::new(element_of(&inner_type))),
                            Sigil::Hash => Type::HashRef(Box::new(Type::Unknown)),
                            Sigil::Code => Type::CodeRef,
                            _ => Type::ScalarRef(Box::new(inner_type)),
                        }
                    }
                    None => {
                        if let Some(inner) = inner {
                            self.expression(&inner);
                        }
                        Type::Ref
                    }
                }
            }
            NodeKind::SCALAR_VAR
            | NodeKind::ARRAY_VAR
            | NodeKind::HASH_VAR
            | NodeKind::CODE_VAR
            | NodeKind::TYPEGLOB_VAR
            | NodeKind::ARRAY_LAST_INDEX => {
                let variable = Variable::cast(node.clone()).expect("kind checked");
                match variable.name() {
                    Some(name) => self.env.get(variable.sigil(), &name),
                    None => Type::Unknown,
                }
            }
            NodeKind::ASSIGN_EXPR => self.assignment(node),
            NodeKind::METHOD_CALL_EXPR => self.method_call(node),
            NodeKind::CALL_EXPR | NodeKind::LIST_CALL_EXPR | NodeKind::CODE_CALL_EXPR => {
                self.call(node)
            }
            NodeKind::HASH_SUBSCRIPT_EXPR
            | NodeKind::ARRAY_SUBSCRIPT_EXPR
            | NodeKind::POSTFIX_DEREF_EXPR
            | NodeKind::SLICE_EXPR => self.subscript(node),
            NodeKind::PAREN_EXPR => {
                let inner = ast::ParenExpr::cast(node.clone()).and_then(|view| view.inner());
                match inner {
                    Some(inner) => self.type_of(&inner),
                    None => Type::Unknown,
                }
            }
            NodeKind::LIST_EXPR => {
                let mut last = Type::Unknown;
                for child in node.children() {
                    last = self.type_of(&child);
                }
                // A list in scalar context is not its last element in general,
                // and nothing here needs it to be.
                if node.children().count() > 1 {
                    Type::Unknown
                } else {
                    last
                }
            }
            NodeKind::BINARY_EXPR => self.binary(node),
            NodeKind::TERNARY_EXPR => {
                let branches: Vec<Type> = node
                    .children()
                    .skip(1)
                    .map(|child| self.type_of(&child))
                    .collect();
                if let Some(condition) = node.children().next() {
                    self.expression(&condition);
                }
                Type::union(branches)
            }
            NodeKind::UNDEF_EXPR => Type::Undef,
            _ => {
                for child in node.children() {
                    self.expression(&child);
                }
                Type::Unknown
            }
        }
    }

    fn anon_hash(&mut self, node: &SyntaxNode) -> Type {
        let view = ast::AnonHash::cast(node.clone()).expect("kind checked");
        let pairs = view.pairs();
        let mut slots = Vec::new();
        let mut values = Vec::new();
        let mut all_named = !pairs.is_empty();
        for pair in &pairs {
            let ty = self.type_of(pair.node());
            values.push(ty.clone());
            match pair.key() {
                Some(key) => slots.push((key.to_string(), ty)),
                None => all_named = false,
            }
        }
        if all_named {
            // A hash written with literal keys is a `Dict`, but a *slurpy*
            // one: nothing says the program will not put another key in it.
            Type::Dict {
                slots,
                slurpy: Some(Box::new(Type::Unknown)),
            }
        } else {
            Type::HashRef(Box::new(Type::union(values)))
        }
    }

    fn binary(&mut self, node: &SyntaxNode) -> Type {
        let view = ast::BinaryExpr::cast(node.clone()).expect("kind checked");
        let operator = view.operator();
        let left_node = view.left();
        let left = left_node.as_ref().map(|left| self.type_of(left));
        // perl short-circuits, so the right side is read under what the left
        // said: `$x && $x->foo` runs the call only where `$x` held, and `!$x
        // || $x->foo` only where it did not (`docs/types.md`, NARROW-6).
        let saved = left_node.as_ref().and_then(|left_node| {
            let facts = narrowing(&self.env, left_node);
            let saved = self.env.clone();
            match operator {
                Some(TokenKind::LOGICAL_AND | TokenKind::AND_KW) => {
                    facts.apply_true(&mut self.env);
                    Some(saved)
                }
                Some(TokenKind::LOGICAL_OR | TokenKind::OR_KW | TokenKind::DEFINED_OR) => {
                    facts.apply_false(&mut self.env);
                    Some(saved)
                }
                _ => None,
            }
        });
        let right = view.right().map(|right| self.type_of(&right));
        if let Some(saved) = saved {
            self.env = saved;
        }
        match operator {
            Some(TokenKind::PLUS | TokenKind::MINUS | TokenKind::STAR) => {
                arithmetic(left.as_ref(), right.as_ref(), Arith::Widening)
            }
            // `%` truncates both sides before it divides, so it is an integer
            // whatever it was handed; `2 / 4` and `2 ** -1` are fractions, so
            // those two are `Num` however integral their operands are.
            Some(TokenKind::MODULO) => arithmetic(left.as_ref(), right.as_ref(), Arith::Integer),
            Some(TokenKind::SLASH | TokenKind::EXPONENT) => {
                arithmetic(left.as_ref(), right.as_ref(), Arith::Fractional)
            }
            Some(TokenKind::DOT | TokenKind::X_OP) => Type::Str,
            Some(
                TokenKind::EQ_EQ
                | TokenKind::NE
                | TokenKind::LT
                | TokenKind::GT
                | TokenKind::LE
                | TokenKind::GE
                | TokenKind::STR_EQ
                | TokenKind::STR_NE
                | TokenKind::STR_LT
                | TokenKind::STR_GT
                | TokenKind::STR_LE
                | TokenKind::STR_GE
                | TokenKind::ISA_KW,
            ) => Type::Bool,
            Some(TokenKind::SPACESHIP | TokenKind::STR_CMP) => Type::Int,
            // `$x // $default` and `$x || $default` leave the left's type
            // without its `undef`, joined with the right's.
            Some(TokenKind::DEFINED_OR | TokenKind::LOGICAL_OR | TokenKind::OR_KW) => {
                match (left, right) {
                    (Some(left), Some(right)) => Type::union(vec![left.without_undef(), right]),
                    _ => Type::Unknown,
                }
            }
            Some(TokenKind::LOGICAL_AND | TokenKind::AND_KW) => right.unwrap_or(Type::Unknown),
            _ => Type::Unknown,
        }
    }

    fn assignment(&mut self, node: &SyntaxNode) -> Type {
        let view = ast::Assign::cast(node.clone()).expect("kind checked");
        let value = view.value().map(|value| self.type_of(&value));
        let Some(target) = view.target() else {
            return value.unwrap_or(Type::Unknown);
        };
        let ty = value.unwrap_or(Type::Unknown);

        // `my ($self, %args) = @_` and `my $x = ...` share one path.
        let declaration = ast::VarDecl::cast(target.clone());
        let targets: Vec<Variable> = match &declaration {
            Some(declaration) => declaration.targets(),
            // Only a bare variable is a target whose type this records.
            // `$h->{k} = 1` assigns into `$h`, not to it — and a subscript on
            // the left is never a diagnostic anyway (autovivification).
            None => Variable::cast(target.clone()).into_iter().collect(),
        };
        let plural = targets.len() > 1
            || declaration
                .as_ref()
                .is_some_and(|d| d.syntax().text().to_string().contains('('));

        if declaration.as_ref().and_then(ast::VarDecl::keyword) == Some(DeclKeyword::Local) {
            return ty;
        }
        // `my ($class, %args) = @_;` and `my $self = shift;` are the statements
        // the parameter list was *read from* (`decl::from_unpacking`), so the
        // types are already bound and this would only overwrite them with the
        // `Unknown` that a list assignment hands out. Which it did: every
        // `$class` in the corpus was `Unknown` by the time its `bless` asked.
        if view.value().is_some_and(|value| unpacks_arguments(&value)) {
            return ty;
        }

        for variable in &targets {
            let Some(name) = variable.name() else {
                continue;
            };
            let assigned = if plural || variable.sigil() != Sigil::Scalar {
                // A list assignment hands out elements nobody here counts.
                Type::Unknown
            } else if view.is_plain() {
                ty.clone()
            } else {
                // `$x //= ...` and friends combine rather than replace.
                Type::union(vec![self.env.get(variable.sigil(), &name), ty.clone()])
            };
            self.env.set(variable.sigil(), &name, assigned);
        }
        if targets.is_empty() {
            // A subscript on the left is never a diagnostic
            // (autovivification), so it is walked but not read.
            self.expression(&target);
        }
        ty
    }

    fn subscript(&mut self, node: &SyntaxNode) -> Type {
        let Some(chain) = ast::SubscriptChain::cast(node.clone()) else {
            for child in node.children() {
                self.expression(&child);
            }
            return Type::Unknown;
        };
        // `$h{k}` is an element of `%h` and `$a[0]` one of `@a` — the same
        // rule the scope pass holds to. Nothing tracks what a plain hash or
        // array holds, so such a step is `Unknown` and the chain starts over
        // from there.
        let steps = chain.steps();
        let direct = !arrowed(steps.first())
            && Variable::cast(chain.base().clone())
                .is_some_and(|variable| variable.sigil() == Sigil::Scalar);

        let mut current = if direct {
            Type::Unknown
        } else {
            self.type_of(chain.base())
        };
        for (index, step) in steps.iter().enumerate() {
            for inner in step.node().children().skip(1) {
                self.expression(&inner);
            }
            if direct && index == 0 {
                continue;
            }
            current = self.step(&current, step, step.node().text_range());
        }
        current
    }

    fn step(&mut self, base: &Type, step: &ast::Step, range: TextRange) -> Type {
        if base.is_unknown() {
            return Type::Unknown;
        }
        self.warn_maybe(base, range);
        let base = base.without_undef();
        match step {
            ast::Step::Hash { key, .. } => match (&base, key) {
                (Type::Dict { slots, slurpy }, Some(key)) => {
                    match slots.iter().find(|(name, _)| name == key) {
                        Some((_, ty)) => ty.required().clone(),
                        None if slurpy.is_none() => {
                            self.diagnostics.push(Diagnostic::new(
                                Code::UnknownKey,
                                range,
                                format!("`{key}` is not a key of `{base}`"),
                            ));
                            Type::Unknown
                        }
                        None => slurpy
                            .as_ref()
                            .map_or(Type::Unknown, |rest| element_of(rest)),
                    }
                }
                (Type::HashRef(value), _) => Type::maybe(value.as_ref().clone()),
                (Type::Map(_, value), _) => Type::maybe(value.as_ref().clone()),
                _ => Type::Unknown,
            },
            ast::Step::Array { index, .. } => match (&base, index) {
                (Type::Tuple(members), Some(index)) => usize::try_from(*index)
                    .ok()
                    .and_then(|index| members.get(index).cloned())
                    .unwrap_or(Type::Unknown),
                (Type::ArrayRef(element), _) => Type::maybe(element.as_ref().clone()),
                _ => Type::Unknown,
            },
            ast::Step::Deref { sigil, .. } => match (sigil, &base) {
                (Sigil::Array, Type::ArrayRef(element)) => element.as_ref().clone(),
                (Sigil::Hash, Type::HashRef(value)) => value.as_ref().clone(),
                (Sigil::Scalar, Type::ScalarRef(inner)) => inner.as_ref().clone(),
                _ => Type::Unknown,
            },
            ast::Step::Slice { .. } => Type::Unknown,
        }
    }

    /// The type of one value an expression contributes to a list literal.
    ///
    /// Every other type in this pass is a scalar-context one
    /// (`docs/types.md`, INFER-6a), and that is exactly what `keys` and
    /// `values` have no single answer to: `scalar keys %h` is a count and
    /// `[ keys %h ]` is the keys themselves. The elements of a `[ ... ]` are
    /// the one place where the context is written down rather than guessed
    /// at, so they are the one place that asks — this is not the list-context
    /// matching that `Returns:` still leaves alone (LIMIT-7).
    fn list_element(&mut self, node: &SyntaxNode) -> Type {
        let Some(call) = ast::Call::cast(node.clone()) else {
            return self.type_of(node);
        };
        let name = call.callee_name().unwrap_or_default();
        if !matches!(name.as_str(), "keys" | "values") {
            return self.type_of(node);
        }
        let arguments = operands(&call);
        let Some((first, rest)) = arguments.split_first() else {
            return Type::Unknown;
        };
        let hash = self.hash_argument(first);
        for extra in rest {
            self.expression(extra);
        }
        match (name.as_str(), hash) {
            ("keys", Type::Map(key, _)) => *key,
            ("keys", Type::HashRef(_) | Type::Dict { .. }) => Type::Str,
            ("values", Type::HashRef(value) | Type::Map(_, value)) => *value,
            ("values", Type::Dict { slots, slurpy }) => {
                let mut members: Vec<Type> = slots.into_iter().map(|(_, ty)| ty).collect();
                // A slurpy `Dict` says the other keys hold something too, and
                // what that is joins what the named ones hold.
                if let Some(rest) = slurpy {
                    members.push(*rest);
                }
                Type::union(members)
            }
            _ => Type::Unknown,
        }
    }

    /// The hash a `keys` or `values` was handed, as the type of a reference
    /// to it.
    ///
    /// `%$h` and `%{ ... }` are the hash a reference points at, so what is
    /// wanted is the referent's type. `$h->%*` is the same hash written the
    /// other way, and the subscript pass already reads it — down to its
    /// *element*, which is put back into a `HashRef` here so that one match
    /// answers all three. A bare `%h` is a hash whose element type this pass
    /// does not track (INFER-5a), and everything else is `Unknown`.
    fn hash_argument(&mut self, node: &SyntaxNode) -> Type {
        let hash_sigil = ast::tokens(node).any(|token| token.token_kind() == TokenKind::HASH_SIGIL);
        match node.node_kind() {
            NodeKind::DEREF_EXPR | NodeKind::BLOCK_DEREF_EXPR if hash_sigil => {
                match node.children().next() {
                    Some(inner) => self.type_of(&inner),
                    None => Type::Unknown,
                }
            }
            NodeKind::POSTFIX_DEREF_EXPR if hash_sigil => {
                Type::HashRef(Box::new(self.type_of(node)))
            }
            _ => {
                self.expression(node);
                Type::Unknown
            }
        }
    }

    fn call(&mut self, node: &SyntaxNode) -> Type {
        let call = ast::Call::cast(node.clone()).expect("kind checked");
        let arguments = call.args();
        let typed = self.type_all(&arguments);
        let Some(name) = call.callee_name() else {
            return Type::Unknown;
        };
        if name == "return" {
            self.check_return(&typed);
            if self.infer.is_some() {
                self.return_site(&typed);
            }
            return Type::Unknown;
        }
        // `goto &other` hands the whole call over, arguments included; a
        // `goto LABEL` moves the body's end somewhere this walk did not look.
        // Either way the sub's answer is not one the site table can read.
        if name == "goto" {
            if let Some(inference) = &mut self.infer {
                inference.sites.opaque = true;
            }
        }
        if name == "bless" {
            return self.bless(&typed);
        }
        // `scalar @a` is a count and `scalar $obj->rows` is whatever `rows`
        // gives back in scalar context — which is what every type in this
        // pass already is. Saying `Int` for both was how `scalar $sth->bind`
        // became an `Int` that then failed to be an `ArrayRef`.
        if name == "scalar" {
            let operands = operands(&call);
            let Some(operand) = operands.first() else {
                return Type::Unknown;
            };
            let ty = self.typed_or_walk(&typed, operand);
            return if counts_elements(operand) {
                Type::Int
            } else {
                ty
            };
        }
        if let Some(builtin) = builtin_return(&name) {
            return builtin;
        }
        let offset = u32::from(node.text_range().start());
        let Some(symbol) = self.program.resolve_call(self.file, offset, &name) else {
            return Type::Unknown;
        };
        let params = symbol.params.clone();
        let returns = symbol.returns.clone();
        self.check_arguments(&params, &call.pairs(), &typed, &name, call.callee_range());
        returns.scalar
    }

    fn method_call(&mut self, node: &SyntaxNode) -> Type {
        let call = ast::MethodCall::cast(node.clone()).expect("kind checked");
        let arguments = call.args();
        let typed = self.type_all(&arguments);
        let Some(invocant) = call.invocant() else {
            return Type::Unknown;
        };
        let Some(method) = call.method_name() else {
            // `$obj->$name(...)` is opaque (`docs/typecheck.md`, non-goals).
            return Type::Unknown;
        };

        // A bareword invocant is a class; a value's class comes from its type.
        let bareword = bareword_class(&invocant);
        let through_a_value = bareword.is_none();
        let class = match bareword {
            Some(class) => Some(class),
            None => {
                let ty = self.type_of(&invocant);
                self.warn_maybe(&ty, call.method_range());
                match ty.without_undef() {
                    Type::InstanceOf(class) => Some(class),
                    _ => None,
                }
            }
        };
        let Some(class) = class else {
            return Type::Unknown;
        };
        if let Some(table) = &mut self.record {
            table.methods.push(MethodSite {
                receiver: invocant.text_range(),
                method_range: call.method_range(),
                class: class.clone(),
                method: method.clone(),
                from: self.package.clone(),
            });
        }

        match self
            .program
            .resolve_method_from(&class, &method, &self.package)
        {
            MethodLookup::Sub(symbol) => {
                let params = symbol.params.clone();
                let returns = symbol.returns.clone();
                // The count as well as the types — but only for a method
                // reached through the *type* of its invocant, which is the one
                // the arity pass never saw: that pass resolves a bareword
                // invocant and nothing else, and would otherwise say it twice.
                if through_a_value {
                    let shape = crate::arity::CallShape::of(&arguments, &call.pairs());
                    crate::arity::check_shape(
                        &params,
                        &shape,
                        true,
                        &symbol.name,
                        call.method_range(),
                        &mut self.diagnostics,
                    );
                }
                self.check_arguments(&params, &call.pairs(), &typed, &method, call.method_range());
                // `Foo->new(...)` is an `InstanceOf['Foo']` (`docs/typecheck.md`,
                // "Inference"). Only where the run actually read a `sub new`,
                // so a class it never saw stays `Unknown`; a `Returns:` wins
                // over it; a framework's generated constructor never reaches
                // here; and the body has to say the value is one of the class
                // (INFER-2g), because `URI->new` hands back a `URI::http`.
                if returns.scalar.is_unknown() && method == "new" && symbol.constructs_own_class {
                    Type::InstanceOf(class)
                } else if returns.invocant {
                    // A sub that returns its invocant returns the class it
                    // was *called* on: `Child->new->set_x(1)->extra` is a
                    // `Child` walking through `Base::set_x`
                    // (`docs/return-inference.md`, "`$self` comes back as the
                    // caller's class").
                    with_invocant(&returns.scalar, &symbol.package, &class)
                } else {
                    returns.scalar
                }
            }
            // An attribute's methods are callables like any other: what
            // `$obj->set_count(...)` may be passed is what the slot was
            // declared, and how many arguments it takes is the framework's
            // shape (`docs/types.md`, METHOD-4c). Read as a type and nothing
            // else, none of that was ever compared.
            MethodLookup::Attribute(attribute) => {
                let params = attribute.params(&method);
                let returns = attribute.returns(&method);
                if through_a_value {
                    let shape = crate::arity::CallShape::of(&arguments, &call.pairs());
                    crate::arity::check_shape(
                        &params,
                        &shape,
                        true,
                        &method,
                        call.method_range(),
                        &mut self.diagnostics,
                    );
                }
                self.check_arguments(&params, &call.pairs(), &typed, &method, call.method_range());
                returns
            }
            MethodLookup::Constructor => {
                self.check_constructor(&class, &call.pairs(), &typed, call.method_range());
                Type::InstanceOf(class)
            }
            MethodLookup::Universal | MethodLookup::Unknown => Type::Unknown,
            MethodLookup::Missing => {
                // Name what was actually searched: `SUPER::` looked above the
                // package holding the line, and `A::b` looked in `A`.
                let message = match method.rsplit_once("::") {
                    Some(("SUPER", bare)) => {
                        format!("no parent of `{}` declares a method `{bare}`", self.package)
                    }
                    Some((qualified, bare)) => {
                        format!("`{qualified}` declares no method `{bare}`")
                    }
                    None => format!("`{class}` declares no method `{method}`"),
                };
                self.diagnostics.push(Diagnostic::new(
                    Code::UnknownMethod,
                    call.method_range(),
                    message,
                ));
                Type::Unknown
            }
        }
    }

    /// `bless $self, $class` — the value's class, and the variable's from here on.
    ///
    /// The second half is what makes a constructor that borrows a parent's
    /// readable: `XML::Twig::new` writes `$self = XML::Parser->new(%args)` and
    /// then blesses `$self` into its own class four lines later, and without
    /// the re-bless every method called on it afterwards is looked up on
    /// `XML::Parser`.
    fn bless(&mut self, typed: &Typed) -> Type {
        let class = typed
            .nodes
            .get(1)
            .map_or(Type::Unknown, |node| self.class_named(node, &typed.nth(1)));
        // A `bless` always changes what its first argument is. Where the
        // class cannot be read, the honest answer is that nobody knows what
        // the value is now — not that it is still whatever it was before.
        if let Some(variable) = typed.nodes.first().cloned().and_then(Variable::cast) {
            if let Some(name) = variable.name() {
                self.env.set(variable.sigil(), &name, class.clone());
            }
        }
        class
    }

    /// What class an expression in `bless`'s second slot names.
    ///
    /// A literal names itself; `__PACKAGE__` and a `ClassName` — which is what
    /// a sub's `$class` invocant is bound to — name the package the `bless` is
    /// written in, since that is what `Package->new` passes. `ref($proto) ||
    /// $proto` reaches here as a union holding a `ClassName` and is read the
    /// same way; a subclass calling it inherits the answer, which is the
    /// assumption every `$self` in the file is already bound under.
    fn class_named(&mut self, node: &SyntaxNode, ty: &Type) -> Type {
        if let Some(text) = ast::key_text(node) {
            if text == "__PACKAGE__" {
                return Type::InstanceOf(self.package.clone());
            }
            if text
                .chars()
                .next()
                .is_some_and(|ch| ch.is_uppercase() || ch == '_')
            {
                return Type::InstanceOf(text);
            }
            return Type::Unknown;
        }
        if names_a_class(ty) {
            Type::InstanceOf(self.package.clone())
        } else {
            Type::Unknown
        }
    }

    /// One `return`, as the site table reads it
    /// (`docs/return-inference.md`, "Sites").
    fn return_site(&mut self, typed: &Typed) {
        let (ty, invocant) = match typed.nodes {
            // `return;` is `undef` in scalar context, and never `Returns: ()`
            // — "returns nothing, do not use the value" is a statement about
            // intent that only an annotation can make.
            [] => (Type::Undef, false),
            [only] => self.read_site(only, typed.nth(0)),
            // `return $a, $b` is a list, whose scalar reading nobody wants.
            _ => (Type::Unknown, false),
        };
        if let Some(inference) = &mut self.infer {
            inference.sites.scalar.push(ty);
            inference.sites.invocant |= invocant;
        }
    }

    /// The scalar type of one value handed back, and whether it is the
    /// invocant.
    fn read_site(&mut self, node: &SyntaxNode, ty: Type) -> (Type, bool) {
        // `wantarray ? LIST : SCALAR` hands back the scalar branch, by
        // definition of what it asked — whatever the list branch holds, which
        // is the half a list-context reading takes.
        if let Some(branch) = wantarray_branch(node) {
            if is_plural(&branch) {
                return (Type::Unknown, false);
            }
            let ty = self.type_of(&branch);
            return (ty, false);
        }
        // A list's scalar reading is a count, and only because it is: saying
        // `Int` would invite `my $rows = $self->rows` off a `return @rows` to
        // be typed as one — a bug the checker should stay quiet about rather
        // than certify.
        if is_plural(node) {
            return (Type::Unknown, false);
        }
        if self.is_invocant(node) {
            // `$_[0]` is bound to nothing, so the marker is also what says
            // what the value is.
            if ty.is_unknown() {
                return (Type::InstanceOf(self.package.clone()), true);
            }
            return (ty, true);
        }
        (ty, false)
    }

    /// Whether an expression hands back the invocant itself.
    ///
    /// The invocant, or a choice between it and something else: `return $ok ?
    /// $self : undef` is a `Maybe` whose `InstanceOf` is still the caller's
    /// class. Nothing deeper than that — `$self->{parent}` mentions `$self`
    /// and is not it.
    fn is_invocant(&self, node: &SyntaxNode) -> bool {
        let Some(inference) = &self.infer else {
            return false;
        };
        if inference.invocant == Invocant::None {
            return false;
        }
        match node.node_kind() {
            NodeKind::SCALAR_VAR => match &inference.invocant {
                Invocant::Named(name) => {
                    Variable::cast(node.clone())
                        .and_then(|view| view.name())
                        .as_deref()
                        == Some(name.as_str())
                }
                _ => false,
            },
            NodeKind::ARRAY_SUBSCRIPT_EXPR => {
                inference.invocant == Invocant::Implicit && is_first_argument(node)
            }
            NodeKind::PAREN_EXPR | NodeKind::LIST_EXPR => match sole_child(node) {
                Some(only) => self.is_invocant(&only),
                None => false,
            },
            NodeKind::TERNARY_EXPR => node
                .children()
                .skip(1)
                .any(|branch| self.is_invocant(&branch)),
            // `bless {...}, $class` is the invocant written the other way
            // round: a class method blesses into the class it was called on,
            // so what comes back is one of *that* class and not one of the
            // package the `bless` is written in.
            NodeKind::CALL_EXPR | NodeKind::LIST_CALL_EXPR => {
                let Some(call) = ast::Call::cast(node.clone()) else {
                    return false;
                };
                call.callee_name().as_deref() == Some("bless")
                    && call
                        .args()
                        .get(1)
                        .is_some_and(|class| self.is_invocant(class))
            }
            _ => false,
        }
    }

    /// A `return` against the sub's `Returns:`.
    ///
    /// The annotation wins and the inferred shape is checked against it
    /// (`docs/typecheck.md`, "`Returns:`"): a `return "x"` in a sub declared
    /// `Returns: Int` is a diagnostic at the `return`.
    fn check_return(&mut self, typed: &Typed) {
        if self.returns.list == ListShape::Nothing {
            if let Some(first) = typed.nodes.first() {
                self.diagnostics.push(Diagnostic::new(
                    Code::ReturnMismatch,
                    first.text_range(),
                    "this sub is declared `Returns: ()` and returns a value".to_string(),
                ));
            }
            return;
        }
        let declared = self.returns.scalar.clone();
        if declared.is_unknown() || matches!(declared, Type::Any) {
            return;
        }
        // Only a single value has a shape to compare: `return ($a, $b)` is a
        // list, and the list half of `Returns:` is what would speak to it.
        let [only] = typed.nodes else {
            return;
        };
        let value = typed.nth(0);
        if value.is_unknown() || compatible(&value, &declared, self.program) {
            return;
        }
        let severity = if is_literal(only) {
            Severity::Error
        } else {
            Severity::Warning
        };
        self.diagnostics.push(
            Diagnostic::new(
                Code::ReturnMismatch,
                only.text_range(),
                format!("`{value}` returned from a sub declared `Returns: {declared}`"),
            )
            .at(severity),
        );
    }

    /// A `Foo->new(key => value)` against the attributes `Foo` declares.
    fn check_constructor(
        &mut self,
        class: &str,
        pairs: &[ast::Arg],
        typed: &Typed,
        range: TextRange,
    ) {
        if self
            .program
            .facts(class)
            .iter()
            .any(|facts| facts.buildargs)
            || self.program.has_unknown_ancestor(class)
        {
            // `BUILDARGS` rewrites what it was given before anything sees it.
            return;
        }
        // The `Class::Accessor::Lite` constructor blesses the hash it was
        // handed rather than checking it, so a key with no accessor behind it
        // is still readable as `$self->{key}` and the program may well be
        // right. Worth saying, at one severity below the contradiction it
        // would be against a constructor that rejects the key.
        let open = self
            .program
            .facts(class)
            .iter()
            .any(|facts| facts.open_constructor);
        let attributes = self.program.attributes(class);
        if attributes.is_empty() {
            return;
        }
        let mut given: Vec<&str> = Vec::new();
        for pair in pairs {
            let Some(key) = pair.key() else {
                // Anything that is not a written-down key means the argument
                // list is not one this can count.
                return;
            };
            given.push(key);
            match attributes.iter().find(|attribute| attribute.name == key) {
                Some(attribute) => {
                    let value = self.typed_or_walk(typed, pair.node());
                    self.check_value(&value, &attribute.ty, pair.node(), &format!("`{key}`"));
                }
                None => {
                    let severity = if open {
                        Severity::Warning
                    } else {
                        Code::UnknownKey.default_severity()
                    };
                    self.diagnostics.push(
                        Diagnostic::new(
                            Code::UnknownKey,
                            pair.range(),
                            format!("`{class}` declares no attribute `{key}`"),
                        )
                        .at(severity),
                    );
                }
            }
        }
        // An open constructor requires nothing: it never looks at what it was
        // given, so there is nothing it can find missing.
        if open {
            return;
        }
        // A name is required only when *every* declaration of it says so.
        // `has '+name' => (default => 'x')` restates an inherited attribute to
        // fill it in, so the parent's `required => 1` is no longer the last
        // word on it — and the `+` is part of the spelling, not of the name.
        let mut missing: Vec<&str> = Vec::new();
        for attribute in &attributes {
            let name = attribute.name.trim_start_matches('+');
            if given.contains(&name) || missing.contains(&name) {
                continue;
            }
            let filled = attributes.iter().any(|other| {
                other.name.trim_start_matches('+') == name && (other.defaulted || !other.required)
            });
            if !filled {
                missing.push(name);
            }
        }
        self.report_missing(&missing, range, &format!("`{class}`"));
    }

    /// The names a call had to pass and did not (`docs/types.md`, DIAG-13).
    ///
    /// Reported once, against the call, rather than once per name: what the
    /// reader has to fix is the argument list, and a constructor with four
    /// required slots would otherwise be four diagnostics on one line.
    fn report_missing(&mut self, missing: &[&str], range: TextRange, what: &str) {
        if missing.is_empty() {
            return;
        }
        let names: Vec<String> = missing.iter().map(|name| format!("`{name}`")).collect();
        self.diagnostics.push(Diagnostic::new(
            Code::MissingArgument,
            range,
            format!("{what} requires {}, not passed here", names.join(", ")),
        ));
    }

    /// Every argument of a call, typed once, for everything the call then
    /// asks about them.
    fn type_all<'nodes>(&mut self, arguments: &'nodes [SyntaxNode]) -> Typed<'nodes> {
        let types = arguments
            .iter()
            .map(|argument| self.type_of(argument))
            .collect();
        Typed {
            nodes: arguments,
            types,
        }
    }

    /// The type already computed for a node, or the walk if it was not one of
    /// the arguments after all.
    fn typed_or_walk(&mut self, typed: &Typed, node: &SyntaxNode) -> Type {
        match typed.of(node) {
            Some(ty) => ty,
            None => self.type_of(node),
        }
    }

    /// A call's arguments against the callee's declared parameter types.
    fn check_arguments(
        &mut self,
        params: &Params,
        pairs: &[ast::Arg],
        typed: &Typed,
        callee: &str,
        range: TextRange,
    ) {
        // Smart::Args lets an optional parameter be *passed* `undef`: the
        // rule is read before the type is, and a value that is not defined
        // returns straight out of it. So `f(x => undef)` against `my $x =>
        // { isa => 'Str', optional => 1 }` is a program that runs, and the
        // declaration is what says so.
        let undef_ok = matches!(params.source(), Some(ParamSource::Args));
        match params {
            Params::Named { params, .. } => {
                let mut given: Vec<&str> = Vec::new();
                for pair in pairs {
                    let Some(key) = pair.key() else {
                        return;
                    };
                    given.push(key);
                    match params.iter().find(|param| param.name == format!("${key}")) {
                        Some(param) => {
                            let value = self.typed_or_walk(typed, pair.node());
                            if undef_ok && is_optional(param) && value == Type::Undef {
                                continue;
                            }
                            self.check_value(
                                &value,
                                &param.ty,
                                pair.node(),
                                &format!("`{key}` of `{callee}`"),
                            );
                        }
                        None => self.diagnostics.push(Diagnostic::new(
                            Code::UnknownKey,
                            pair.range(),
                            format!("`{callee}` takes no argument named `{key}`"),
                        )),
                    }
                }
                // `args` dies on a missing one, the same way it dies on a
                // count a signature cannot take.
                let missing: Vec<&str> = params
                    .iter()
                    .filter(|param| !param.optional && !param.ty.is_optional())
                    .map(|param| param.name.trim_start_matches('$'))
                    .filter(|name| !given.contains(name))
                    .collect();
                self.report_missing(&missing, range, &format!("`{callee}`"));
            }
            Params::Positional {
                params, invocant, ..
            } => {
                let declared: &[Param] = if *invocant && !params.is_empty() {
                    &params[1..]
                } else {
                    params
                };
                for ((index, argument), param) in typed.nodes.iter().enumerate().zip(declared) {
                    let value = typed.nth(index);
                    if undef_ok && is_optional(param) && value == Type::Undef {
                        continue;
                    }
                    self.check_value(
                        &value,
                        &param.ty,
                        argument,
                        &format!("`{}` of `{callee}`", param.name),
                    );
                }
            }
            Params::Unknown => {}
        }
    }

    /// One value against one declared slot.
    fn check_value(&mut self, value: &Type, slot: &Type, node: &SyntaxNode, what: &str) {
        let slot = slot.required();
        if value.is_unknown() || slot.is_unknown() || matches!(slot, Type::Any) {
            return;
        }
        if compatible(value, slot, self.program) {
            return;
        }
        // A literal is right there, so the two sides are both written down and
        // the contradiction is an error. An inferred value is a warning.
        let severity = if is_literal(node) {
            Severity::Error
        } else {
            Severity::Warning
        };
        self.diagnostics.push(
            Diagnostic::new(
                Code::TypeMismatch,
                node.text_range(),
                format!("`{value}` passed to {what}, which is declared `{slot}`"),
            )
            .at(severity),
        );
    }

    /// A `Maybe[...]` used where a value is wanted.
    fn warn_maybe(&mut self, ty: &Type, range: TextRange) {
        if !ty.is_maybe() || ty.without_undef().is_unknown() {
            return;
        }
        self.diagnostics.push(Diagnostic::new(
            Code::MaybeDeref,
            range,
            format!("`{ty}` may be undefined here, and nothing has checked it"),
        ));
    }
}

/// Bind a sub's declared parameters, so its body starts with what it was told.
fn bind_params(env: &mut Env, params: &Params, package: &str) {
    match params {
        Params::Positional {
            params, invocant, ..
        } => {
            for (index, param) in params.iter().enumerate() {
                let ty = if index == 0 && *invocant {
                    if param.name == "$class" {
                        Type::ClassName
                    } else {
                        Type::InstanceOf(package.to_string())
                    }
                } else {
                    param.ty.clone()
                };
                bind(env, &param.name, ty);
            }
        }
        Params::Named {
            params, invocant, ..
        } => {
            if *invocant {
                bind(env, "$self", Type::InstanceOf(package.to_string()));
            }
            for param in params {
                bind(env, &param.name, param.ty.clone());
            }
        }
        Params::Unknown => {}
    }
}

fn bind(env: &mut Env, display: &str, ty: Type) {
    let mut chars = display.chars();
    let sigil = match chars.next() {
        Some('$') => Sigil::Scalar,
        Some('@') => Sigil::Array,
        Some('%') => Sigil::Hash,
        _ => return,
    };
    env.set(sigil, chars.as_str(), ty);
}

/// `42` is `Int`, `1.5` is `Num`, `"x"` is `Str` — and `"3"` is `Int`, because
/// that is the subtyping Perl's values actually have.
fn literal_type(node: &SyntaxNode) -> Type {
    let Some(literal) = ast::Literal::cast(node.clone()) else {
        return Type::Unknown;
    };
    if let Some(number) = literal.as_number() {
        return if number.contains('.') || number.contains('e') || number.contains('E') {
            Type::Num
        } else {
            Type::Int
        };
    }
    match literal.as_string() {
        Some(text) => numeric_type(&text).unwrap_or(Type::Str),
        // An interpolating string is a `Str` whatever it holds.
        None => Type::Str,
    }
}

/// What `looks_like_number` would say about a string.
fn numeric_type(text: &str) -> Option<Type> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.parse::<i64>().is_ok() {
        return Some(Type::Int);
    }
    trimmed.parse::<f64>().ok().map(|_| Type::Num)
}

fn is_literal(node: &SyntaxNode) -> bool {
    matches!(
        node.node_kind(),
        NodeKind::LITERAL
            | NodeKind::Q_EXPR
            | NodeKind::QQ_EXPR
            | NodeKind::ANON_ARRAY
            | NodeKind::ANON_HASH
            | NodeKind::ANON_SUB_EXPR
            | NodeKind::UNDEF_EXPR
            | NodeKind::QW_EXPR
    )
}

/// Every class or role a type names.
fn collect_classes(ty: &Type, into: &mut Vec<String>) {
    match ty {
        Type::InstanceOf(name) | Type::ConsumerOf(name) => into.push(name.clone()),
        Type::ScalarRef(inner)
        | Type::ArrayRef(inner)
        | Type::HashRef(inner)
        | Type::Optional(inner) => collect_classes(inner, into),
        Type::Map(key, value) => {
            collect_classes(key, into);
            collect_classes(value, into);
        }
        Type::Tuple(members) | Type::Union(members) => {
            for member in members {
                collect_classes(member, into);
            }
        }
        Type::Dict { slots, slurpy } => {
            for (_, value) in slots {
                collect_classes(value, into);
            }
            if let Some(rest) = slurpy {
                collect_classes(rest, into);
            }
        }
        _ => {}
    }
}

/// Whether an expression is the one a parameter list was unpacked from.
fn unpacks_arguments(node: &SyntaxNode) -> bool {
    if Variable::cast(node.clone()).is_some_and(|variable| {
        variable.sigil() == Sigil::Array && variable.name().as_deref() == Some("_")
    }) {
        return true;
    }
    ast::Call::cast(node.clone()).is_some_and(|call| {
        matches!(call.callee_name().as_deref(), Some("shift" | "pop"))
            && call.args().iter().all(unpacks_arguments)
    })
}

/// Whether a type is, or may be, the name of a class.
fn names_a_class(ty: &Type) -> bool {
    match ty {
        Type::ClassName => true,
        Type::Union(members) => members.iter().any(names_a_class),
        _ => false,
    }
}

/// Whether a step reaches its base through `->`.
fn arrowed(step: Option<&ast::Step>) -> bool {
    step.is_some_and(|step| {
        ast::tokens(step.node()).any(|token| token.token_kind() == TokenKind::ARROW)
    })
}

/// `Foo::Bar` in `Foo::Bar->new`.
fn bareword_class(node: &SyntaxNode) -> Option<String> {
    let call = ast::Call::cast(node.clone())?;
    if !call.args().is_empty() {
        return None;
    }
    let name = call.callee_name()?;
    // `__PACKAGE__` and `shift` are not class names.
    name.chars()
        .next()
        .is_some_and(char::is_uppercase)
        .then_some(name)
}

/// What an arithmetic operator does to whole numbers.
#[derive(Clone, Copy)]
enum Arith {
    /// An integer whatever it was handed: `%`.
    Integer,
    /// An integer when both sides are: `+`, `-`, `*`.
    Widening,
    /// Never an integer on that ground alone: `/`, `**`.
    Fractional,
}

/// The type of an arithmetic expression (`docs/types.md`, INFER-1a).
///
/// An operand nobody typed makes the answer one nobody typed either — `Num`
/// would be a claim, and `Int` slots are what it would be reported against.
fn arithmetic(left: Option<&Type>, right: Option<&Type>, operator: Arith) -> Type {
    let (Some(left), Some(right)) = (left, right) else {
        return Type::Unknown;
    };
    if left.is_unknown() || right.is_unknown() {
        return Type::Unknown;
    }
    match operator {
        Arith::Integer => Type::Int,
        Arith::Widening if is_integral(left) && is_integral(right) => Type::Int,
        Arith::Widening | Arith::Fractional => Type::Num,
    }
}

/// Whether every value this type has numifies to a whole number.
///
/// `Bool` is `0`, `1`, `''` and `undef`, all of which do; `undef` inside a
/// `Maybe` is the same `0`.
fn is_integral(ty: &Type) -> bool {
    match ty {
        Type::Int | Type::Bool | Type::Undef => true,
        Type::Union(members) => members.iter().all(is_integral),
        _ => false,
    }
}

/// What one element of a container holds.
fn element_of(ty: &Type) -> Type {
    match ty {
        Type::ArrayRef(element) | Type::HashRef(element) | Type::ScalarRef(element) => {
            element.as_ref().clone()
        }
        Type::Map(_, value) => value.as_ref().clone(),
        Type::Tuple(members) => Type::union(members.clone()),
        _ => Type::Unknown,
    }
}

/// Whether a parameter may be left out, however the declaration said so.
///
/// `optional => 1` and a `default` are the rule's own words; `Optional[T]` is
/// the same thing said in the type.
fn is_optional(param: &Param) -> bool {
    param.optional || param.ty.is_optional()
}

/// Whether `broad` heads a family that `narrow` belongs to (`docs/types.md`,
/// TYPE-6).
///
/// These are the types that say what *kind* of thing a value is and nothing
/// more: `Ref` is every reference, `Value` every defined non-reference,
/// `Object` every blessed one, and `Str`, `Num` and `GlobRef` each head a
/// family too. Reading them as ordinary names — equal to themselves and to
/// nothing else — is what made `[1]` fail to be a `Ref`, and a `Dict` fail to
/// be one along with it.
///
/// Set-wise this is `narrow ⊆ broad` for the non-structural types, with
/// `Bool`'s `undef` the one thing it is inexact about: `Bool` is counted into
/// `Value` and `Defined` because three of its four values are, which is the
/// overlap reading. [`is_assignable`] is the one that says otherwise.
fn heads_family_of(broad: &Type, narrow: &Type) -> bool {
    heads_kind_family_of(broad, narrow)
        || match broad {
            Type::Str => matches!(
                narrow,
                Type::Str
                    | Type::Num
                    | Type::Int
                    | Type::Enum(_)
                    | Type::ClassName
                    | Type::RoleName
            ),
            Type::Num => matches!(narrow, Type::Num | Type::Int),
            _ => false,
        }
}

/// The half of [`heads_family_of`] that says what *kind* of thing a value is.
///
/// Read in reverse it is still true: a value known only as a `Ref` could be
/// any reference, so nothing is ruled out either way. The stringification
/// chain is not like that — `Int` fits a `Str` slot and a `Str` value is not
/// shown to be an `Int`, because a numeric-looking literal is already an
/// `Int` by TYPE-5b — so it is left out of this one.
fn heads_kind_family_of(broad: &Type, narrow: &Type) -> bool {
    match broad {
        Type::Defined => !matches!(narrow, Type::Undef),
        Type::Value => is_value(narrow),
        Type::Ref => is_reference(narrow),
        Type::Object => matches!(
            narrow,
            Type::Object | Type::InstanceOf(_) | Type::ConsumerOf(_) | Type::HasMethods(_)
        ),
        Type::GlobRef => matches!(narrow, Type::GlobRef | Type::FileHandle),
        _ => false,
    }
}

/// Whether a value of `value` could be in a slot declared `slot`.
///
/// "Could be", not "is": the checker reports only what it can rule out. Every
/// rule below is a *contradiction* — two shapes that cannot be the same value
/// — and anything else is silence. [`is_assignable`] is the stricter relation
/// beside it, and `docs/types.md` TYPE-7 is why the checker reports against
/// this one.
#[must_use]
pub fn compatible(value: &Type, slot: &Type, program: &Program) -> bool {
    let slot = slot.required();
    if value.is_unknown()
        || slot.is_unknown()
        || matches!(slot, Type::Any)
        || matches!(value, Type::Any)
    {
        return true;
    }
    match (value, slot) {
        // A union fits if any of its members might.
        (Type::Union(members), _) => members
            .iter()
            .any(|member| compatible(member, slot, program)),
        (_, Type::Union(members)) => members
            .iter()
            .any(|member| compatible(value, member, program)),

        // `undef` fits an `Undef` slot and a `Maybe` — and a `Bool`, whose
        // four values are `0`, `1`, `''` and `undef` in Moose and in
        // Types::Standard alike (`docs/types.md`, TYPE-5). Nothing else.
        (Type::Undef, Type::Undef | Type::Bool) | (Type::Bool, Type::Undef) => true,
        (Type::Undef, _) | (_, Type::Undef) => false,

        // A family's head accepts everything in it, and — for the families
        // that say what *kind* of thing a value is — a value known only as
        // the head could be any of them, so nothing is ruled out either way.
        (value, slot) if heads_family_of(slot, value) || heads_kind_family_of(value, slot) => true,

        // Stringification: `Int <: Num <: Str`, so a number fits a string slot
        // and never the other way round unless the string is numeric — which
        // `literal_type` already decided.
        (Type::Int, Type::Num | Type::Str | Type::Value) => true,
        (Type::Num, Type::Str | Type::Value) => true,
        (Type::Str, Type::Value) => true,
        // An `Enum`'s values *are* strings, so it meets every kind of scalar
        // whichever side each is on (`docs/types.md`, TYPE-5e). Which of them
        // it actually holds is a question about values, and this checker
        // follows shapes.
        (Type::Int | Type::Num | Type::Str | Type::Bool, Type::Enum(_))
        | (Type::Enum(_), Type::Int | Type::Num | Type::Str | Type::Bool) => true,
        // One enum fits another when every value it may hold is a value the
        // other accepts.
        (Type::Enum(value), Type::Enum(slot)) => value.iter().all(|one| slot.contains(one)),
        (Type::Str, Type::ClassName | Type::RoleName) => true,
        (Type::ClassName | Type::RoleName, Type::Str) => true,

        // Bool is nominal: `0`, `1`, `''` and `undef` are the values it has,
        // and three of those four are numbers or strings — so it meets both,
        // whichever side each is on (`docs/types.md`, TYPE-5c).
        (Type::Bool, Type::Int | Type::Num | Type::Str)
        | (Type::Int | Type::Num | Type::Str, Type::Bool) => true,

        // A reference and a value are never the same thing.
        (value, slot) if is_reference(value) && is_value(slot) => false,
        (value, slot) if is_value(value) && is_reference(slot) => false,

        // `Value` is a defined non-reference (Types::Standard), so a reference
        // is never one. That is the whole of what `Value` rules out; anything
        // else it is asked about it accepts, which is what the last arm does
        // for it. `Defined` rules out exactly `undef`, which the `Undef` arm
        // above has already said.
        (value, Type::Value) | (Type::Value, value) if is_reference(value) => false,

        (Type::ArrayRef(_) | Type::Tuple(_), Type::HashRef(_) | Type::Dict { .. }) => false,
        (Type::HashRef(_) | Type::Dict { .. }, Type::ArrayRef(_) | Type::Tuple(_)) => false,
        (Type::CodeRef, Type::ArrayRef(_) | Type::HashRef(_) | Type::Dict { .. }) => false,
        (Type::ArrayRef(_) | Type::HashRef(_) | Type::Dict { .. }, Type::CodeRef) => false,

        (Type::ArrayRef(left), Type::ArrayRef(right)) => compatible(left, right, program),
        (Type::HashRef(left), Type::HashRef(right)) => compatible(left, right, program),
        (Type::Dict { .. }, Type::HashRef(_)) | (Type::HashRef(_), Type::Dict { .. }) => true,
        (Type::Tuple(_), Type::ArrayRef(_)) | (Type::ArrayRef(_), Type::Tuple(_)) => true,

        // `Map[K, V]` is a hash whose keys are also constrained. Against a
        // `HashRef` only the value side is comparable, and a `Dict` names its
        // keys one at a time, so it is compared the way a `Dict` and a
        // `HashRef` are — not at all (`docs/types.md`, TYPE-4c).
        (Type::Map(value_key, value_item), Type::Map(slot_key, slot_item)) => {
            compatible(value_key, slot_key, program) && compatible(value_item, slot_item, program)
        }
        (Type::Map(_, left), Type::HashRef(right)) | (Type::HashRef(left), Type::Map(_, right)) => {
            compatible(left, right, program)
        }
        (Type::Dict { .. }, Type::Map(_, _)) | (Type::Map(_, _), Type::Dict { .. }) => true,

        // Two `Dict`s: every slot the declaration names has to be there and to
        // fit (`docs/types.md`, TYPE-4d). Keys the declaration does not name
        // are not a contradiction — the value's are open unless it says
        // otherwise, and an inferred hash always says otherwise (TYPE-4).
        (
            Type::Dict {
                slots: value_slots,
                slurpy: value_slurpy,
            },
            Type::Dict {
                slots: slot_slots,
                slurpy: _,
            },
        ) => slot_slots.iter().all(|(key, declared)| {
            match value_slots.iter().find(|(name, _)| name == key) {
                Some((_, held)) => compatible(held, declared.required(), program),
                // Absent: fine if the slot may be left out, and fine if the
                // value is open, since an open hash may hold the key after all.
                None => declared.is_optional() || value_slurpy.is_some(),
            }
        }),

        // Type::Tiny has one regexp type under two names.
        (Type::RegexpRef, Type::InstanceOf(class)) | (Type::InstanceOf(class), Type::RegexpRef)
            if class == "Regexp" =>
        {
            true
        }

        (Type::InstanceOf(left), Type::InstanceOf(right)) => {
            // Both classes have to be known before a "no" means anything.
            if !program.knows_package(left) || !program.knows_package(right) {
                return true;
            }
            program.isa(left, right)
        }
        // A bareword nothing declares reads as a class name (TYPE-3), which is
        // also how an unread type library's structured type arrives here. Any
        // reference could be an object of a class the program never saw, so
        // there is nothing to rule out.
        (value, Type::InstanceOf(class)) | (Type::InstanceOf(class), value)
            if is_reference(value) && !program.knows_package(class) =>
        {
            true
        }
        (Type::InstanceOf(_), Type::Object) => true,
        (Type::Object, Type::InstanceOf(_)) => true,

        (left, right) => left == right || !is_settled(left) || !is_settled(right),
    }
}

/// Whether every value of `value` is a value of `slot` — set inclusion, which
/// is what an assignment would have to satisfy (`docs/types.md`, TYPE-7).
///
/// The strict relation beside [`compatible`], and **not** the one the checker
/// reports against. The two differ wherever a type holds more than the slot
/// does without contradicting it: a `Str|ArrayRef[Str]` may be the `Str` the
/// slot wanted and may not, and `Bool` holds an `undef` that a `Value` does
/// not. Silence there is the checker's stance (`docs/types.md`, POLICY-1), so
/// this exists to be asked rather than to be enforced — by a stricter reading
/// later, and by the tests that hold the two relations against each other.
#[must_use]
pub fn is_assignable(value: &Type, slot: &Type, program: &Program) -> bool {
    let slot = slot.required();
    let value = value.required();
    // Neither side is a claim, so there is nothing to fail. `Any` on the
    // value side is one of these: a signature's parameters are all `Any`
    // because a signature says nothing about types (`docs/types.md`,
    // ANNOT-5), so it reaches here meaning "unannotated" rather than "every
    // value there is".
    if value.is_unknown()
        || slot.is_unknown()
        || matches!(slot, Type::Any)
        || matches!(value, Type::Any)
    {
        return true;
    }
    if value == slot {
        return true;
    }
    match (value, slot) {
        // The whole of the difference: *every* value the union may hold has
        // to be one the slot takes.
        (Type::Union(members), _) => members
            .iter()
            .all(|member| is_assignable(member, slot, program)),
        (_, Type::Union(members)) => members
            .iter()
            .any(|member| is_assignable(value, member, program)),

        (Type::Undef, Type::Bool) => true,
        (Type::Undef, _) => false,
        // `Bool` is `0`, `1`, `''` and `undef`. The `undef` is why it is not a
        // `Value`, not a `Defined` and not a `Str`, however much the other
        // three are.
        (Type::Bool, _) => false,

        (value, slot) if heads_family_of(slot, value) => true,

        (Type::ScalarRef(value), Type::ScalarRef(slot)) => is_assignable(value, slot, program),
        (Type::ArrayRef(value), Type::ArrayRef(slot)) => is_assignable(value, slot, program),
        (Type::Tuple(members), Type::ArrayRef(slot)) => members
            .iter()
            .all(|member| is_assignable(member, slot, program)),
        (Type::Tuple(value), Type::Tuple(slot)) => {
            value.len() == slot.len()
                && value
                    .iter()
                    .zip(slot)
                    .all(|(value, slot)| is_assignable(value, slot, program))
        }
        (Type::HashRef(value), Type::HashRef(slot) | Type::Map(_, slot)) => {
            is_assignable(value, slot, program)
        }
        (Type::Map(_, value), Type::HashRef(slot)) => is_assignable(value, slot, program),
        (Type::Map(value_key, value_item), Type::Map(slot_key, slot_item)) => {
            is_assignable(value_key, slot_key, program)
                && is_assignable(value_item, slot_item, program)
        }
        (Type::Dict { slots, slurpy }, Type::HashRef(slot) | Type::Map(_, slot)) => {
            slots.iter().all(|(_, ty)| is_assignable(ty, slot, program))
                && slurpy
                    .as_ref()
                    .is_none_or(|rest| is_assignable(rest, slot, program))
        }
        // Every key the slot names has to be there and to fit, and a key it
        // does not name has to be one it allows.
        (
            Type::Dict {
                slots: held,
                slurpy: extra,
            },
            Type::Dict {
                slots: declared,
                slurpy: allowed,
            },
        ) => {
            let names_it = |key: &str| declared.iter().any(|(name, _)| name == key);
            declared.iter().all(
                |(key, want)| match held.iter().find(|(name, _)| name == key) {
                    Some((_, have)) => is_assignable(have, want.required(), program),
                    None => want.is_optional(),
                },
            ) && (allowed.is_some()
                || (extra.is_none() && held.iter().all(|(key, _)| names_it(key))))
        }

        // Type::Tiny has one regexp type under two names.
        (Type::RegexpRef, Type::InstanceOf(class)) | (Type::InstanceOf(class), Type::RegexpRef) => {
            class == "Regexp"
        }
        (Type::InstanceOf(value), Type::InstanceOf(slot)) => {
            // A class the run never read is one nothing can be shown about.
            if !program.knows_package(value) || !program.knows_package(slot) {
                return true;
            }
            program.isa(value, slot)
        }
        (Type::Enum(value), Type::Enum(slot)) => value.iter().all(|one| slot.contains(one)),

        _ => false,
    }
}

fn is_reference(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Ref
            | Type::ScalarRef(_)
            | Type::ArrayRef(_)
            | Type::Tuple(_)
            | Type::HashRef(_)
            | Type::Dict { .. }
            | Type::Map(_, _)
            | Type::CodeRef
            | Type::RegexpRef
            | Type::GlobRef
            | Type::Object
            | Type::InstanceOf(_)
            | Type::ConsumerOf(_)
            | Type::HasMethods(_)
    )
}

fn is_value(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Str
            | Type::Num
            | Type::Int
            | Type::Bool
            | Type::ClassName
            | Type::RoleName
            | Type::Enum(_)
    )
}

/// Whether a type says enough for a "no" to mean anything.
///
/// A family's head says as much as its members do — `Defined` and `Value` each
/// rule something out — so they are settled too, and the arm above them is
/// what compares them.
fn is_settled(ty: &Type) -> bool {
    is_value(ty) || is_reference(ty) || matches!(ty, Type::Undef | Type::Defined | Type::Value)
}

// ----- narrowing -----

/// What a condition says about the variables in it, on each side of it
/// (`docs/typecheck.md`, "Narrowing").
///
/// A **fixed list of shapes** rather than a general theorem, because the
/// diagnostic it feeds — `maybe-deref` — is the checker's most useful and its
/// most likely false positive. What is new against the flat scan this
/// replaced is that the list is read *structurally*: a condition is a tree,
/// and `!`, `||` and a call nobody read all change what its parts say.
#[derive(Debug, Default)]
struct Narrowing {
    /// What holds however the condition went, because the part of it that
    /// says so was evaluated either way: `!$x->name` ran the call before it
    /// negated the answer.
    certain: Vec<Fact>,
    /// What holds only where the condition was true.
    when_true: Vec<Fact>,
    /// What holds only where it was false. Most shapes here have none — `ref
    /// $x` failing leaves `$x` a perfectly good non-reference — and the side
    /// exists because `!` swaps the two, which is how a negation stops
    /// claiming anything.
    when_false: Vec<Fact>,
}

type Fact = (Sigil, String, Type);

impl Narrowing {
    /// One fact, learned where the condition held.
    fn when_true(fact: Fact) -> Self {
        Narrowing {
            when_true: vec![fact],
            ..Narrowing::default()
        }
    }

    /// One fact that holds whichever way the condition went.
    fn certain(fact: Fact) -> Self {
        Narrowing {
            certain: vec![fact],
            ..Narrowing::default()
        }
    }

    fn swapped(self) -> Self {
        Narrowing {
            certain: self.certain,
            when_true: self.when_false,
            when_false: self.when_true,
        }
    }

    /// Two parts that are *both* evaluated — a comparison, a parenthesised
    /// list. What either says where it held holds where the whole did.
    fn both(mut self, other: Self) -> Self {
        self.certain.extend(other.certain);
        self.when_true.extend(other.when_true);
        // Which of the two made the whole false is not known.
        self.when_false = Vec::new();
        self
    }

    /// `A && B`. B runs only where A held, so what B says — including what it
    /// says merely by running — belongs to the true side and nowhere else.
    fn and(mut self, other: Self) -> Self {
        self.when_true.extend(other.certain);
        self.when_true.extend(other.when_true);
        self.when_false = Vec::new();
        self
    }

    /// `A || B`. The whole may hold because A did, so neither part's fact is
    /// one the true side can claim; B runs only where A failed.
    fn or(mut self, other: Self) -> Self {
        self.when_true = Vec::new();
        self.when_false.extend(other.certain);
        self.when_false.extend(other.when_false);
        self
    }

    fn apply_true(&self, env: &mut Env) {
        for (sigil, name, ty) in self.certain.iter().chain(&self.when_true) {
            env.set(*sigil, name, ty.clone());
        }
    }

    fn apply_false(&self, env: &mut Env) {
        for (sigil, name, ty) in self.certain.iter().chain(&self.when_false) {
            env.set(*sigil, name, ty.clone());
        }
    }
}

/// Read a condition for what it says (`docs/types.md`, NARROW).
fn narrowing(env: &Env, node: &SyntaxNode) -> Narrowing {
    match node.node_kind() {
        NodeKind::PREFIX_EXPR => {
            let negated = ast::tokens(node).any(|token| {
                matches!(
                    token.token_kind(),
                    TokenKind::LOGICAL_NOT | TokenKind::NOT_KW
                )
            });
            let inner = node
                .children()
                .next()
                .map_or_else(Narrowing::default, |child| narrowing(env, &child));
            if negated {
                inner.swapped()
            } else {
                inner
            }
        }
        NodeKind::BINARY_EXPR => {
            let view = ast::BinaryExpr::cast(node.clone()).expect("kind checked");
            let left = view
                .left()
                .map_or_else(Narrowing::default, |left| narrowing(env, &left));
            let right = view
                .right()
                .map_or_else(Narrowing::default, |right| narrowing(env, &right));
            match view.operator() {
                Some(
                    TokenKind::LOGICAL_OR
                    | TokenKind::OR_KW
                    | TokenKind::DEFINED_OR
                    | TokenKind::XOR_KW,
                ) => left.or(right),
                _ => left.and(right),
            }
        }
        // `defined $x`, `blessed $x`, `ref $x`, `exists $h{k}`. Any other call
        // is one nobody here read: `validate($x)` may well be a defined-ness
        // check and may well be the opposite, and a guess either way is a
        // guess about a body this pass never opened.
        NodeKind::CALL_EXPR | NodeKind::LIST_CALL_EXPR => {
            let Some(call) = ast::Call::cast(node.clone()) else {
                return Narrowing::default();
            };
            let name = call.callee_name().unwrap_or_default();
            if !matches!(name.as_str(), "defined" | "blessed" | "ref" | "exists") {
                return Narrowing::default();
            }
            let mut found = Narrowing::default();
            for argument in operands(&call) {
                // `defined $x->{k}` narrows `$x` too.
                let base = ast::SubscriptChain::cast(argument.clone())
                    .map_or_else(|| argument.clone(), |chain| chain.base().clone());
                if let Some(fact) = defined_fact(env, &base) {
                    found = found.both(Narrowing::when_true(fact));
                }
                // And `defined $x->name` narrows `$x`, because the call had
                // to happen for there to be anything to ask about.
                found = found.both(narrowing(env, &argument));
            }
            found
        }
        // `$x->isa('Foo')` says what `$x` is; any other method call says only
        // that its invocant was there to be called.
        NodeKind::METHOD_CALL_EXPR => {
            let Some(call) = ast::MethodCall::cast(node.clone()) else {
                return Narrowing::default();
            };
            let Some(invocant) = call.invocant() else {
                return Narrowing::default();
            };
            if call.method_name().as_deref() == Some("isa") {
                if let (Some(variable), Some(class)) = (
                    Variable::cast(invocant.clone()),
                    call.args().first().and_then(ast::key_text),
                ) {
                    if let Some(name) = variable.name() {
                        return Narrowing::when_true((
                            variable.sigil(),
                            name,
                            Type::InstanceOf(class),
                        ));
                    }
                }
            }
            // The call happened, so its invocant was not `undef` — whichever
            // way the condition then went.
            defined_fact(env, &invocant).map_or_else(Narrowing::default, Narrowing::certain)
        }
        // `if ($x)` — the truth test itself, which says something only where
        // it passed.
        NodeKind::SCALAR_VAR => {
            defined_fact(env, node).map_or_else(Narrowing::default, Narrowing::when_true)
        }
        // Everything else is walked through: a parenthesised condition, a
        // comparison whose operands are both evaluated, a subscript chain.
        // Stopping here instead would lose `ref $x eq 'HASH'`, which is the
        // shape half of this list exists for.
        _ => {
            let mut parts = node.children().map(|child| narrowing(env, &child));
            let Some(first) = parts.next() else {
                return Narrowing::default();
            };
            match parts.next() {
                // A wrapper around one expression *is* that expression, both
                // sides of it included: a `!` inside a parenthesised condition
                // still has a failure that says something.
                None => first,
                Some(second) => parts.fold(first.both(second), Narrowing::both),
            }
        }
    }
}

/// The condition of a statement modifier: what follows `unless` or `if`.
fn modifier_condition(statement: &SyntaxNode, keyword: TokenKind) -> Option<SyntaxNode> {
    let mut seen = false;
    for element in statement.descendants_with_tokens() {
        match element {
            rowan::NodeOrToken::Token(token) if token.token_kind() == keyword => seen = true,
            rowan::NodeOrToken::Node(node) if seen => return Some(node),
            _ => {}
        }
    }
    None
}

/// The left side of `COND or die`, which is the part that has to have held
/// for the statement below it to be reached.
fn leaving_alternative(statement: &SyntaxNode) -> Option<SyntaxNode> {
    statement.descendants().find_map(|node| {
        let view = ast::BinaryExpr::cast(node)?;
        matches!(
            view.operator(),
            Some(TokenKind::OR_KW | TokenKind::LOGICAL_OR)
        )
        .then(|| view.left())
        .flatten()
    })
}

/// `$x` without its `undef`, if `$x` is a variable at all.
fn defined_fact(env: &Env, node: &SyntaxNode) -> Option<Fact> {
    let variable = Variable::cast(node.clone())?;
    let name = variable.name()?;
    let narrowed = env.get(variable.sigil(), &name).without_undef();
    Some((variable.sigil(), name, narrowed))
}

// ----- builtins -----

/// What a call was handed, whether or not it has an argument list.
///
/// `f(1, 2)` and `f 1, 2` both hold a `LIST_EXPR`; perl's named unary
/// operators do not. `scalar @a` and `values %$h` hang their one operand
/// beside the name, so [`ast::Call::args`] finds nothing and the operand has
/// to be read off the call's own children.
fn operands(call: &ast::Call) -> Vec<SyntaxNode> {
    let arguments = call.args();
    if !arguments.is_empty() {
        return arguments;
    }
    call.syntax()
        .children()
        .filter(|child| child.node_kind() != NodeKind::SUB_NAME)
        .collect()
}

/// Whether `scalar` was handed a container, whose scalar value is its count.
///
/// An array, a hash, a slice, and the three spellings of a dereference to
/// one. `scalar $$ref` is not one of them, and neither is a call.
fn counts_elements(node: &SyntaxNode) -> bool {
    match node.node_kind() {
        NodeKind::ARRAY_VAR | NodeKind::HASH_VAR | NodeKind::SLICE_EXPR => true,
        NodeKind::DEREF_EXPR | NodeKind::BLOCK_DEREF_EXPR | NodeKind::POSTFIX_DEREF_EXPR => {
            ast::tokens(node).any(|token| {
                matches!(
                    token.token_kind(),
                    TokenKind::ARRAY_SIGIL | TokenKind::HASH_SIGIL
                )
            })
        }
        _ => false,
    }
}

/// What a builtin gives back in scalar context.
///
/// Derived from the argument-shape table the parser already keeps
/// (`grammar/builtins.rs`) and extended with a return column, which is the
/// only thing the checker needed from it. Only the ones whose answer is worth
/// having are listed; anything else is `Unknown` and silent.
fn builtin_return(name: &str) -> Option<Type> {
    Some(match name {
        "length" | "index" | "rindex" | "ord" | "int" | "time" | "fileno" | "system" => Type::Int,
        "abs" | "sqrt" | "atan2" | "sin" | "cos" | "exp" | "log" | "rand" => Type::Num,
        "lc" | "uc" | "lcfirst" | "ucfirst" | "chr" | "sprintf" | "join" | "substr"
        | "quotemeta" | "ref" => Type::Str,
        "defined" | "exists" | "wantarray" | "eof" => Type::Bool,
        "sort" | "reverse" | "map" | "grep" | "split" => Type::Unknown,
        // `scalar`, `keys` and `values` are not here: their answer depends on
        // what they were handed or on the context they sit in, so they are
        // answered where those are known — `Pass::call` and
        // `Pass::list_element`.
        _ => return None,
    })
}

/// The join of an `if` chain's branches, as the chain's own tail.
///
/// A chain with no `else` is opaque whatever its branches said, because the
/// value of a false `if` is its condition's.
fn join_tails(tails: &[Tail], has_else: bool) -> Tail {
    if !has_else {
        return Tail::Opaque;
    }
    let mut members = Vec::new();
    let mut invocant = false;
    for tail in tails {
        match tail {
            Tail::Value { ty, invocant: one } => {
                members.push(ty.clone());
                invocant |= one;
            }
            Tail::Left => {}
            Tail::Opaque => return Tail::Opaque,
        }
    }
    if members.is_empty() {
        // Every branch returned or died, so nothing falls out of the chain.
        return Tail::Left;
    }
    Tail::Value {
        ty: Type::union(members),
        invocant,
    }
}

/// Whether a statement hands control back rather than leaving a value.
///
/// `throw` is here as a method as well as a bareword: `My::Error->throw(...)`
/// is how a class-based exception is raised, and it is the same bottom.
fn leaves_the_sub(statement: &SyntaxNode) -> bool {
    let Some(expression) = sole_expression(statement) else {
        return false;
    };
    if let Some(call) = ast::Call::cast(expression.clone()) {
        return matches!(
            call.callee_name().as_deref(),
            Some("return" | "die" | "croak" | "confess" | "throw" | "exit" | "goto")
        );
    }
    ast::MethodCall::cast(expression)
        .and_then(|call| call.method_name())
        .as_deref()
        == Some("throw")
}

/// The one expression a statement is, past the `LIST_EXPR` that wraps it.
fn sole_expression(statement: &SyntaxNode) -> Option<SyntaxNode> {
    let only = sole_child(statement)?;
    if only.node_kind() == NodeKind::LIST_EXPR {
        return sole_child(&only);
    }
    Some(only)
}

/// The node's only child, or `None` when it has none or several.
fn sole_child(node: &SyntaxNode) -> Option<SyntaxNode> {
    let mut children = node.children();
    let only = children.next()?;
    children.next().is_none().then_some(only)
}

/// Whether an expression is a list rather than one value.
fn is_plural(node: &SyntaxNode) -> bool {
    match node.node_kind() {
        NodeKind::ARRAY_VAR | NodeKind::HASH_VAR | NodeKind::SLICE_EXPR => true,
        NodeKind::DEREF_EXPR | NodeKind::BLOCK_DEREF_EXPR => ast::tokens(node).any(|token| {
            matches!(
                token.token_kind(),
                TokenKind::ARRAY_SIGIL | TokenKind::HASH_SIGIL
            )
        }),
        NodeKind::POSTFIX_DEREF_EXPR => ast::tokens(node).any(|token| {
            matches!(
                token.token_kind(),
                TokenKind::POSTFIX_DEREF_ARRAY | TokenKind::POSTFIX_DEREF_HASH
            )
        }),
        NodeKind::PAREN_EXPR => ast::ParenExpr::cast(node.clone())
            .and_then(|view| view.inner())
            .is_some_and(|inner| is_plural(&inner)),
        NodeKind::LIST_EXPR => match sole_child(node) {
            Some(only) => is_plural(&only),
            // `(A, B)`, and also `()`, whose scalar value is `undef` but
            // whose *list* half is what an author writing it meant.
            None => true,
        },
        // Either branch being a list makes the whole thing one, whichever way
        // the condition goes — `wantarray` included.
        NodeKind::TERNARY_EXPR => node.children().skip(1).any(|branch| is_plural(&branch)),
        _ => false,
    }
}

/// The scalar branch of `wantarray ? LIST : SCALAR`.
fn wantarray_branch(node: &SyntaxNode) -> Option<SyntaxNode> {
    if node.node_kind() != NodeKind::TERNARY_EXPR {
        return None;
    }
    let mut children = node.children();
    let condition = children.next()?;
    let asked = ast::Call::cast(condition).and_then(|call| call.callee_name());
    if asked.as_deref() != Some("wantarray") {
        return None;
    }
    children.nth(1)
}

/// `$_[0]` — the first argument, which is the invocant of a sub that unpacks
/// nothing.
fn is_first_argument(node: &SyntaxNode) -> bool {
    let Some(chain) = ast::SubscriptChain::cast(node.clone()) else {
        return false;
    };
    let is_arguments = Variable::cast(chain.base().clone())
        .is_some_and(|view| view.sigil() == Sigil::Scalar && view.name().as_deref() == Some("_"));
    let steps = chain.steps();
    is_arguments
        && !arrowed(steps.first())
        && matches!(steps, [ast::Step::Array { index: Some(0), .. }])
}

/// Whether `InstanceOf[package]` is one of the things a type may be.
fn holds_own_class(ty: &Type, package: &str) -> bool {
    match ty {
        Type::InstanceOf(name) => name == package,
        Type::Union(members) => members
            .iter()
            .any(|member| holds_own_class(member, package)),
        Type::Optional(inner) => holds_own_class(inner, package),
        _ => false,
    }
}

/// The same type with the invocant marker replaced by the receiver's class.
///
/// The substitution `constructs_own_class` performs for `new`, for the same
/// reason: `Child->new->set_x(1)` is a `Child`, and telling the caller `Base`
/// is an `unknown-method` on the next link of the chain.
fn with_invocant(ty: &Type, own: &str, class: &str) -> Type {
    match ty {
        Type::InstanceOf(name) if name == own => Type::InstanceOf(class.to_string()),
        Type::Union(members) => Type::union(
            members
                .iter()
                .map(|member| with_invocant(member, own, class))
                .collect(),
        ),
        Type::Optional(inner) => Type::Optional(Box::new(with_invocant(inner, own, class))),
        other => other.clone(),
    }
}

/// What the design calls the sub's `Returns:` — kept here so that the flow
/// pass and the declaration pass agree on the empty case.
#[must_use]
pub fn returns_nothing(returns: &Returns) -> bool {
    returns.list == ListShape::Nothing
}

#[cfg(test)]
mod tests;
