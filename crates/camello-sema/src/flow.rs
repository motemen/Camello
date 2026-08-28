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
use rowan::TextRange;

use crate::annotate::{ListShape, Returns};
use crate::decl::{Param, ParamSource, Params};
use crate::diag::{Code, Diagnostic, Severity};
use crate::program::{MethodLookup, Program};
use crate::types::Type;

/// Check one file's bodies against everything the program declares.
#[must_use]
pub fn analyse(root: &SyntaxNode, file: usize, program: &Program) -> Vec<Diagnostic> {
    let mut pass = Pass {
        program,
        file,
        env: Env::default(),
        diagnostics: Vec::new(),
        package: "main".to_string(),
        returns: Returns::default(),
    };
    pass.block(root);
    pass.check_annotations();
    pass.diagnostics
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
        // rather than earning them.
        if let Some(symbol) = definition
            .name_text()
            .and_then(|name| self.program.sub(&self.package, &name))
        {
            self.returns = symbol.returns.clone();
            bind_params(&mut self.env, &symbol.params, &self.package);
        }
        self.block(body.syntax());
        self.env = saved;
        self.returns = saved_returns;
    }

    fn expression_statement(&mut self, node: &SyntaxNode) {
        for child in node.children() {
            self.expression(&child);
        }
        // A guard narrows what follows it: `return unless defined $x;` is how
        // half the corpus turns a `Maybe` into a value.
        self.apply_guard(node);
    }

    fn if_statement(&mut self, node: &SyntaxNode) {
        let before = self.env.clone();
        let mut after: Option<Env> = None;
        let mut condition_seen = false;

        for child in node.children() {
            match child.node_kind() {
                NodeKind::BLOCK => {
                    let branch = self.env.clone();
                    self.block(&child);
                    let ended = std::mem::replace(&mut self.env, before.clone());
                    match &mut after {
                        Some(env) => env.join(&ended),
                        None => after = Some(ended),
                    }
                    let _ = branch;
                }
                NodeKind::ELSIF_CLAUSE | NodeKind::ELSE_CLAUSE => {
                    self.env = before.clone();
                    for inner in child.children() {
                        if inner.node_kind() == NodeKind::BLOCK {
                            let ended_env = self.env.clone();
                            self.block(&inner);
                            let ended = std::mem::replace(&mut self.env, before.clone());
                            match &mut after {
                                Some(env) => env.join(&ended),
                                None => after = Some(ended),
                            }
                            let _ = ended_env;
                        } else {
                            self.expression(&inner);
                        }
                    }
                }
                _ if !condition_seen => {
                    condition_seen = true;
                    self.expression(&child);
                    // The narrowing applies inside the block that follows.
                    let negated =
                        ast::tokens(node).any(|token| token.token_kind() == TokenKind::UNLESS_KW);
                    if !negated {
                        narrow(&mut self.env, &child, self.program);
                    }
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
        // `unless COND` and `COND or LEAVE` both mean "below here, COND held".
        if text_has(TokenKind::UNLESS_KW)
            || text_has(TokenKind::OR_KW)
            || text_has(TokenKind::LOGICAL_OR)
        {
            for node in statement.descendants() {
                narrow_one(&mut self.env, &node, self.program);
            }
        }
    }

    // ----- expressions -----

    fn expression(&mut self, node: &SyntaxNode) {
        let _ = self.type_of(node);
    }

    /// The type of an expression in scalar context, checking what it holds on
    /// the way down.
    fn type_of(&mut self, node: &SyntaxNode) -> Type {
        match node.node_kind() {
            NodeKind::LITERAL => literal_type(node),
            NodeKind::Q_EXPR | NodeKind::QQ_EXPR | NodeKind::HEREDOC_EXPR | NodeKind::QX_EXPR => {
                Type::Str
            }
            NodeKind::QR_EXPR => Type::RegexpRef,
            NodeKind::ANON_SUB_EXPR => {
                if let Some(body) = ast::AnonSubExpr::cast(node.clone()).and_then(|v| v.body()) {
                    let saved = self.env.clone();
                    self.block(body.syntax());
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
        let left = view.left().map(|left| self.type_of(&left));
        let right = view.right().map(|right| self.type_of(&right));
        match view.operator() {
            Some(
                TokenKind::PLUS
                | TokenKind::MINUS
                | TokenKind::STAR
                | TokenKind::SLASH
                | TokenKind::MODULO
                | TokenKind::EXPONENT,
            ) => Type::Num,
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
            return Type::Unknown;
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
                        symbol,
                        call.method_range(),
                        &mut self.diagnostics,
                    );
                }
                self.check_arguments(&params, &call.pairs(), &typed, &method, call.method_range());
                // `Foo->new(...)` is an `InstanceOf['Foo']` (`docs/typecheck.md`,
                // "Inference"). Only where the run actually read a `sub new`,
                // so a class it never saw stays `Unknown`; a `Returns:` wins
                // over it; and a framework's generated constructor never
                // reaches here. The classes this is wrong about are the ones
                // whose `new` hands back something else — `URI->new` returns a
                // `URI::http` — and what they need is to be marked opaque,
                // which is a fact about them and not about every `new`.
                if returns.scalar.is_unknown() && method == "new" {
                    Type::InstanceOf(class)
                } else {
                    returns.scalar
                }
            }
            MethodLookup::Attribute(attribute) => attribute.ty.clone(),
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

/// Whether a value of `value` could be in a slot declared `slot`.
///
/// "Could be", not "is": the checker reports only what it can rule out. Every
/// rule below is a *contradiction* — two shapes that cannot be the same value
/// — and anything else is silence.
#[must_use]
/// Whether a parameter may be left out, however the declaration said so.
///
/// `optional => 1` and a `default` are the rule's own words; `Optional[T]` is
/// the same thing said in the type.
fn is_optional(param: &Param) -> bool {
    param.optional || param.ty.is_optional()
}

pub fn compatible(value: &Type, slot: &Type, program: &Program) -> bool {
    let slot = slot.required();
    if value.is_unknown()
        || slot.is_unknown()
        || matches!(slot, Type::Any | Type::Defined)
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
        (Type::Undef, Type::Undef | Type::Bool) => true,
        (Type::Undef, _) => false,

        // Stringification: `Int <: Num <: Str`, so a number fits a string slot
        // and never the other way round unless the string is numeric — which
        // `literal_type` already decided.
        (Type::Int, Type::Num | Type::Str | Type::Value) => true,
        (Type::Num, Type::Str | Type::Value) => true,
        (Type::Str, Type::Value) => true,
        (Type::Int | Type::Num | Type::Str | Type::Bool, Type::Enum(_)) => true,
        (Type::Str, Type::ClassName | Type::RoleName) => true,
        (Type::ClassName | Type::RoleName, Type::Str) => true,

        // Bool is nominal: `0`, `1`, `''` and `undef` are the values it has.
        (Type::Bool, Type::Int | Type::Num | Type::Str) => true,
        (Type::Int, Type::Bool) => true,

        // A reference and a value are never the same thing.
        (value, slot) if is_reference(value) && is_value(slot) => false,
        (value, slot) if is_value(value) && is_reference(slot) => false,

        (Type::ArrayRef(_) | Type::Tuple(_), Type::HashRef(_) | Type::Dict { .. }) => false,
        (Type::HashRef(_) | Type::Dict { .. }, Type::ArrayRef(_) | Type::Tuple(_)) => false,
        (Type::CodeRef, Type::ArrayRef(_) | Type::HashRef(_) | Type::Dict { .. }) => false,
        (Type::ArrayRef(_) | Type::HashRef(_) | Type::Dict { .. }, Type::CodeRef) => false,

        (Type::ArrayRef(left), Type::ArrayRef(right)) => compatible(left, right, program),
        (Type::HashRef(left), Type::HashRef(right)) => compatible(left, right, program),
        (Type::Dict { .. }, Type::HashRef(_)) | (Type::HashRef(_), Type::Dict { .. }) => true,
        (Type::Tuple(_), Type::ArrayRef(_)) | (Type::ArrayRef(_), Type::Tuple(_)) => true,

        (Type::InstanceOf(left), Type::InstanceOf(right)) => {
            // Both classes have to be known before a "no" means anything.
            if !program.knows_package(left) || !program.knows_package(right) {
                return true;
            }
            program.isa(left, right)
        }
        (Type::InstanceOf(_), Type::Object) => true,
        (Type::Object, Type::InstanceOf(_)) => true,

        (left, right) => left == right || !is_settled(left) || !is_settled(right),
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
        Type::Str | Type::Num | Type::Int | Type::Bool | Type::ClassName | Type::RoleName
    )
}

/// Whether a type says enough for a "no" to mean anything.
fn is_settled(ty: &Type) -> bool {
    is_value(ty) || is_reference(ty) || matches!(ty, Type::Undef | Type::Enum(_))
}

// ----- narrowing -----

/// Narrow every variable a condition tests (`docs/typecheck.md`,
/// "Narrowing").
///
/// A fixture-tested list rather than a general theorem, because the diagnostic
/// it feeds — `maybe-deref` — is the checker's most useful and its most likely
/// false positive.
fn narrow(env: &mut Env, condition: &SyntaxNode, program: &Program) {
    for node in condition.descendants() {
        narrow_one(env, &node, program);
    }
}

fn narrow_one(env: &mut Env, node: &SyntaxNode, program: &Program) {
    let _ = program;
    match node.node_kind() {
        // `defined $x`, `blessed $x`, `ref $x`, `exists $h{k}`
        NodeKind::CALL_EXPR | NodeKind::LIST_CALL_EXPR => {
            let Some(call) = ast::Call::cast(node.clone()) else {
                return;
            };
            let name = call.callee_name().unwrap_or_default();
            if !matches!(name.as_str(), "defined" | "blessed" | "ref" | "exists") {
                return;
            }
            for argument in call.args() {
                if let Some(variable) = Variable::cast(argument.clone()) {
                    if let Some(name) = variable.name() {
                        let narrowed = env.get(variable.sigil(), &name).without_undef();
                        env.set(variable.sigil(), &name, narrowed);
                    }
                }
                // `defined $x->{k}` narrows `$x` too.
                if let Some(chain) = ast::SubscriptChain::cast(argument.clone()) {
                    if let Some(variable) = Variable::cast(chain.base().clone()) {
                        if let Some(name) = variable.name() {
                            let narrowed = env.get(variable.sigil(), &name).without_undef();
                            env.set(variable.sigil(), &name, narrowed);
                        }
                    }
                }
            }
        }
        // `if ($x)`, `$x or return`
        NodeKind::SCALAR_VAR => {
            let Some(variable) = Variable::cast(node.clone()) else {
                return;
            };
            let Some(name) = variable.name() else {
                return;
            };
            let narrowed = env.get(variable.sigil(), &name).without_undef();
            env.set(variable.sigil(), &name, narrowed);
        }
        // `$x->isa('Foo')`
        NodeKind::METHOD_CALL_EXPR => {
            let Some(call) = ast::MethodCall::cast(node.clone()) else {
                return;
            };
            if call.method_name().as_deref() != Some("isa") {
                return;
            }
            let (Some(invocant), Some(class)) =
                (call.invocant(), call.args().first().and_then(ast::key_text))
            else {
                return;
            };
            if let Some(variable) = Variable::cast(invocant) {
                if let Some(name) = variable.name() {
                    env.set(variable.sigil(), &name, Type::InstanceOf(class));
                }
            }
        }
        _ => {}
    }
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

/// What the design calls the sub's `Returns:` — kept here so that the flow
/// pass and the declaration pass agree on the empty case.
#[must_use]
pub fn returns_nothing(returns: &Returns) -> bool {
    returns.list == ListShape::Nothing
}
