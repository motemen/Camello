//! The annotation recognisers (`docs/typecheck.md`, "Annotation sources").
//!
//! Each is a match on a declaration shape that yields symbols or parameter
//! lists. None of them is special-cased in the parser: `has` is a
//! `LIST_CALL_EXPR` like every other bareword (`camello dev dump` shows it) and
//! stays one.
//!
//! Recognition is by callee name **and** by an import that could have provided
//! it, so a project's own `sub has` is not mistaken for Moose's. That test is
//! [`Frameworks`], which the declaration pass fills in per package from the
//! `use` statements written in it, before it reads anything else.

use std::collections::BTreeMap;

use camello_syntax::ast::{self, AnonHash, Args, AstNode, Literal, SubDef};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode, SyntaxToken};
use rowan::TextRange;

use crate::diag::{Code, Diagnostic};
use crate::types::{self, Type};

/// The object framework a package is written in, which decides whether `new`
/// exists and what it accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Framework {
    #[default]
    None,
    /// Moose, Moo, Mouse and their `::Role` forms — `has` declares an
    /// attribute and `new` takes a `Dict` of them.
    Moose,
    /// `use Class::Accessor::Typed (rw => {...})`.
    AccessorTyped,
    /// `Class::Accessor::Lite` and the `mk_accessors` family it belongs to:
    /// accessors and, when asked for, a `new`, and no types anywhere.
    AccessorLite,
    /// A `bless` with no framework behind it.
    Bless,
}

/// What a project's own modules re-export (`camello.toml`, `read-as`).
///
/// Recognition is by an import that could have provided the name, and a
/// project that wraps `Class::Accessor::Typed` in a module of its own has
/// taken that import away: every file says `use My::Accessors`, and nothing
/// in it names the module whose declaration syntax it is writing. No
/// recogniser can read that from the source, because the wrapper's own file
/// is a `sub import` — so the project says it instead, once.
///
/// It renames a module only for the recognisers. What a file `use`s is still
/// what it says: the resolver looks for the wrapper's own path, and an import
/// list still comes from the module that was written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dialect {
    modules: BTreeMap<String, String>,
}

impl Dialect {
    #[must_use]
    pub fn new(modules: BTreeMap<String, String>) -> Self {
        Dialect { modules }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// The module a `use` is to be read as, which is itself unless the
    /// project said otherwise.
    #[must_use]
    pub fn read_as<'a>(&'a self, module: &'a str) -> &'a str {
        self.modules.get(module).map_or(module, String::as_str)
    }

    /// What the declaration cache has to be keyed by on top of the file.
    ///
    /// A cached declaration was read under one dialect, and the same bytes
    /// read under another are different declarations.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        self.modules
            .iter()
            .map(|(from, to)| format!("{from}={to}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// What a package's imports say a bareword in it could mean.
///
/// The point of this is one line in the design: recognition is by callee name
/// *and* by an import that could have provided it. A package that never says
/// `use Moose` has no `has` to recognise, whatever it calls its own subs — and
/// the unit is the package rather than the file, because that is the unit
/// perl imports into (`docs/types.md`, ANNOT-1a).
#[derive(Debug, Clone, Default)]
pub struct Frameworks {
    pub moose: bool,
    pub smart_args: bool,
    pub accessor_typed: bool,
    pub accessor_lite: bool,
    pub type_library: bool,
    dialect: Dialect,
}

impl Frameworks {
    /// The same, for a project whose own modules stand in for these.
    #[must_use]
    pub fn with_dialect(dialect: Dialect) -> Self {
        Frameworks {
            dialect,
            ..Frameworks::default()
        }
    }

    /// Fold one `use Foo` into what the file can be expected to mean.
    pub fn note(&mut self, module: &str) {
        let module = self.dialect.read_as(module);
        match module {
            "Moose"
            | "Moo"
            | "Mouse"
            | "Moose::Role"
            | "Moo::Role"
            | "Mouse::Role"
            | "MooseX::Declare"
            | "Mojo::Base"
            | "Moose::Util::TypeConstraints"
            | "Mouse::Util::TypeConstraints" => {
                self.moose = true;
            }
            "Smart::Args" | "Smart::Args::TypeTiny" => self.smart_args = true,
            "Class::Accessor::Typed" => self.accessor_typed = true,
            // `Class::Accessor::Lite` installs its accessors from `import`;
            // the other three are inherited from and hand out the same
            // `mk_accessors` family. Reached by `use`, and by the `use base`
            // that is how a `Class::Accessor` subclass says it.
            "Class::Accessor::Lite"
            | "Class::Accessor::Lite::Lazy"
            | "Class::Accessor"
            | "Class::Accessor::Fast"
            | "Class::Accessor::Faster" => self.accessor_lite = true,
            _ => {}
        }
        if supplies_the_type_dsl(module) {
            self.type_library = true;
        }
    }

    /// Fold what a `use` was *given* into the same picture.
    ///
    /// Two things are only visible in the import list: `use base
    /// 'Class::Accessor'` names the framework in its arguments rather than in
    /// its module, and `use Class::Accessor 'antlers'` is the one spelling of
    /// `Class::Accessor` that exports `has`.
    pub fn note_arguments(&mut self, module: &str, names: &[String]) {
        let module = self.dialect.read_as(module).to_string();
        match module.as_str() {
            "parent" | "base" => {
                for name in names {
                    if name != "-norequire" && name != "norequire" {
                        self.note(name);
                    }
                }
            }
            "Class::Accessor" | "Class::Accessor::Fast" | "Class::Accessor::Faster"
                if names
                    .iter()
                    .any(|name| name == "antlers" || name == "moose-like") =>
            {
                self.moose = true;
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn framework(&self) -> Framework {
        if self.moose {
            Framework::Moose
        } else if self.accessor_typed {
            Framework::AccessorTyped
        } else if self.accessor_lite {
            Framework::AccessorLite
        } else {
            Framework::None
        }
    }
}

/// Whether a `use` of this module is one the checker understands without
/// reading it (`docs/types.md`, DIAG-7a).
///
/// A framework is recognised by *name*: the checker knows what `use Moose`
/// installs better than reading `Moose.pm` would tell it, so the file not
/// being on the search path is not a hole in the method surface. Everything
/// else that is `use`d and was never found is one, because a module installs
/// subs into its importer and may assign to its globs.
///
/// This is the list [`Frameworks::note`] and the declaration pass's `use`
/// recogniser branch on, kept beside them so the three stay in step.
#[must_use]
pub fn is_recognised(module: &str) -> bool {
    matches!(
        module,
        // Moose and the dialects that read as it.
        "Moose"
            | "Moo"
            | "Mouse"
            | "Moose::Role"
            | "Moo::Role"
            | "Mouse::Role"
            | "MooseX::Declare"
            | "Mojo::Base"
            | "Moose::Util::TypeConstraints"
            | "Mouse::Util::TypeConstraints"
            // Parameter lists, and the accessor families.
            | "Smart::Args"
            | "Smart::Args::TypeTiny"
            | "Class::Accessor::Typed"
            | "Class::Accessor::Lite"
            | "Class::Accessor::Lite::Lazy"
            | "Class::Accessor"
            | "Class::Accessor::Fast"
            | "Class::Accessor::Faster"
            // Statements the declaration pass reads for itself.
            | "parent"
            | "base"
            | "constant"
            | "Exporter"
            // XS, whose meaning is that the package is dynamic — which
            // `Program::has_unknown_ancestor` answers, not this.
            | "XSLoader"
            | "DynaLoader"
            | "Inline"
            | "Alien::Base"
    ) || supplies_the_type_dsl(module)
}

/// Whether a module could have supplied the type-library DSL (`docs/types.md`,
/// ANNOT-8d).
///
/// A family rather than a list, which is a departure from how the other
/// recognisers are gated. The DSL is one vocabulary — `declare`, `type`, `as`,
/// `enum`, `class_type` — and half a dozen distributions supply it under
/// half a dozen names: `Type::Utils` exports `type` only under `-all`,
/// `Type::Library -base` re-exports it, `MooseX::Types` has its own, and a
/// file commonly names only the `Types::` module its constants come from. A
/// list of exporters would have to be right about which one a given file's
/// `type` came from, and being wrong there costs a whole library's worth of
/// annotations; the family costs a bareword `enum` in a `Types::`-importing
/// file being read as a declaration, which resolves to nothing and is silent.
fn supplies_the_type_dsl(module: &str) -> bool {
    let family = |prefix: &str| module == prefix || module.starts_with(&format!("{prefix}::"));
    family("Type")
        || family("Types")
        || family("MooseX::Types")
        || family("MouseX::Types")
        // `Moose::Util::TypeConstraints` and the Mouse and Moo spellings of it.
        || module.ends_with("::Util::TypeConstraints")
}

/// How a reader may reach an attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Access {
    Ro,
    Rw,
    Wo,
}

/// One method an attribute declaration generates, and what calling it says.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeneratedMethod {
    pub name: String,
    pub role: AccessorRole,
}

/// What a generated method does, which is what decides its result type.
///
/// Reading these apart is what keeps a `predicate` from claiming to give back
/// the attribute's own type — `$obj->has_items` against an `ArrayRef[Int]`
/// slot was a `type-mismatch` on correct code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AccessorRole {
    /// Gives the attribute's value back: the accessor itself, and `reader`.
    Reader,
    /// Sets the slot and gives back what it set, which Moose's writers do.
    Writer,
    /// `predicate`: whether the slot is filled.
    Predicate,
    /// `clearer`: empties the slot. What it gives back is the deletion's
    /// answer, which no caller uses and which is not worth claiming.
    Clearer,
    /// `handles`: another object's method, which nothing here read.
    Delegated,
}

impl AccessorRole {
    /// What calling a method in this role gives back, for an attribute
    /// declared `ty`.
    #[must_use]
    pub fn returns(self, ty: &Type) -> Type {
        match self {
            AccessorRole::Reader | AccessorRole::Writer => ty.clone(),
            AccessorRole::Predicate => Type::Bool,
            AccessorRole::Clearer | AccessorRole::Delegated => Type::Unknown,
        }
    }
}

/// An attribute, from `has`, `Class::Accessor::Typed`, or the `mk_accessors`
/// family.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttributeDecl {
    pub name: String,
    pub ty: Type,
    pub access: Access,
    pub required: bool,
    /// A `default`, a `builder` or a `lazy` — anything that means `new` may
    /// leave the slot out.
    pub defaulted: bool,
    /// `coerce => 1`: what goes in may be anything, and the coercion that
    /// turns it into the declared type is a function nobody here read. What
    /// comes back out is still the declared type.
    #[serde(default)]
    pub coerce: bool,
    /// The methods this attribute generates besides the accessor itself:
    /// `reader`, `writer`, `predicate`, `clearer`, and whatever `handles`
    /// delegates. Each with what it gives back, which is not the attribute's
    /// type for all of them (`docs/types.md`, METHOD-4).
    pub methods: Vec<GeneratedMethod>,
    /// `handles` naming a regexp or a role: the delegated set is unknowable,
    /// so the class may have any method and "no such method" is off.
    pub opaque_delegation: bool,
    /// The sub that fills a lazy slot, where the declaration named one that
    /// can be looked up (`docs/types.md`, ANNOT-10f).
    ///
    /// `Class::Accessor::Lite::Lazy` carries no types, so what the builder
    /// returns is the only thing that says what the slot holds — the accessor
    /// is `$self->{name} //= $self->$builder`, and every other member of the
    /// family leaves this `None`.
    #[serde(default)]
    pub builder: Option<String>,
    #[serde(with = "crate::serde_range")]
    pub range: TextRange,
}

impl AttributeDecl {
    /// Whether this attribute answers to `name`, as its accessor or as one of
    /// the methods it generates.
    #[must_use]
    pub fn answers_to(&self, name: &str) -> bool {
        self.name == name || self.methods.iter().any(|method| method.name == name)
    }

    /// What may be put *into* the slot: anything, where a coercion stands
    /// between the caller and the declared type (`docs/types.md`, ANNOT-2a).
    #[must_use]
    pub fn accepts(&self) -> Type {
        if self.coerce {
            Type::Any
        } else {
            self.ty.clone()
        }
    }

    /// The parameter list of the method `name` generates
    /// (`docs/types.md`, METHOD-4c).
    ///
    /// This is what makes an attribute's methods checkable at all: they used
    /// to be a *type* and nothing else, so `$obj->set_count([1, 2])` against
    /// an `Int` slot had nothing to be compared with, while the same sub
    /// written by hand was checked.
    #[must_use]
    pub fn params(&self, name: &str) -> crate::decl::Params {
        use crate::decl::{Param, ParamSource, Params};
        let invocant = Param {
            name: "$self".to_string(),
            optional: false,
            ty: Type::Any,
        };
        let value = |optional: bool| Param {
            name: format!("${}", self.name),
            optional,
            ty: self.accepts(),
        };
        let positional = |params: Vec<Param>| Params::Positional {
            params,
            slurpy: false,
            invocant: true,
            source: ParamSource::Generated,
        };
        let role = self
            .methods
            .iter()
            .find(|method| method.name == name)
            .map(|method| method.role);
        match role {
            Some(AccessorRole::Reader | AccessorRole::Predicate | AccessorRole::Clearer) => {
                positional(vec![invocant])
            }
            Some(AccessorRole::Writer) => positional(vec![invocant, value(false)]),
            // Another class's method, and nothing here read it.
            Some(AccessorRole::Delegated) => Params::Unknown,
            // The accessor itself, whose name is the attribute's. A `ro` one
            // takes nothing; the others take the value, and may be read.
            None => match self.access {
                Access::Ro => positional(vec![invocant]),
                Access::Wo => positional(vec![invocant, value(false)]),
                Access::Rw => positional(vec![invocant, value(true)]),
            },
        }
    }

    /// Whether calling `name` gives back what the slot holds, rather than
    /// something *about* it: the accessor itself, a `reader` and a `writer`
    /// do; a `predicate` says whether the slot is filled and a `clearer` and
    /// a delegate say nothing about it at all.
    #[must_use]
    pub fn yields_the_slot(&self, name: &str) -> bool {
        self.methods
            .iter()
            .find(|method| method.name == name)
            .is_none_or(|method| matches!(method.role, AccessorRole::Reader | AccessorRole::Writer))
    }

    /// What calling `name` on it gives back (`docs/types.md`, METHOD-4a).
    ///
    /// Not the attribute's type for all of them: a `predicate` says whether
    /// the slot is filled, and a `clearer` and a delegated method say nothing
    /// this pass can read.
    #[must_use]
    pub fn returns(&self, name: &str) -> Type {
        match self.methods.iter().find(|method| method.name == name) {
            Some(method) => method.role.returns(&self.ty),
            // The accessor itself, which is the attribute's own name.
            None => self.ty.clone(),
        }
    }
}

/// What reading a file's annotations produced.
///
/// The diagnostics are the pass's own; the ledger is for the questions that
/// need the whole program and cannot be asked here — whether a class named in
/// a type is one anything declares (`unknown-type`).
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Sink {
    pub diagnostics: Vec<Diagnostic>,
    pub annotations: Vec<Annotated>,
}

/// One annotation, as read, and where it was written.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Annotated {
    pub ty: Type,
    #[serde(with = "crate::serde_range")]
    pub range: TextRange,
}

impl Sink {
    fn note(&mut self, ty: &Type, range: TextRange) {
        self.annotations.push(Annotated {
            ty: ty.clone(),
            range,
        });
    }
}

/// Read a type annotation's text, reporting when it does not parse.
///
/// An annotation that is silently ignored is worse than none
/// (`docs/typecheck.md`, "`Returns:`"), so a failure is a diagnostic — but
/// only when the text was *meant* as a type. A `$type_object` or a
/// `Foo->meta->type` is code the checker cannot read, not an annotation it
/// read wrongly, and it is `Unknown` and silent.
#[must_use]
pub fn read_annotation(annotation: &crate::decl::Annotation, into: &mut Sink) -> Type {
    match types::parse(&annotation.text) {
        Ok(ty) => {
            into.note(&ty, annotation.range);
            ty
        }
        Err(error) => {
            if looks_like_a_type(&annotation.text) {
                into.diagnostics.push(Diagnostic::new(
                    Code::BadAnnotation,
                    annotation.range,
                    format!("`{}` is not a type: {}", annotation.text, error.message),
                ));
            }
            Type::Unknown
        }
    }
}

/// Whether text that failed to parse was nonetheless meant as a type.
///
/// The corpus is full of `isa => $class_object` and `isa => __PACKAGE__ . '::X'`
/// — code that computes a constraint, which the checker cannot read and has no
/// business complaining about. What it *can* say is that `'ArrayRef[Str'` is a
/// type expression with a bracket missing.
fn looks_like_a_type(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Anything holding a sigil, an arrow, or a call is code.
    if trimmed.contains('$')
        || trimmed.contains('@')
        || trimmed.contains('%')
        || trimmed.contains("->")
        || trimmed.contains('(')
        || trimmed.contains('\\')
    {
        return false;
    }
    // A type expression begins with a name and holds only what one holds.
    types::is_type_shaped(trimmed)
}

// ===== `has` =====

/// `has name => (is => 'ro', isa => 'Str', required => 1);`
///
/// Also `has [qw(a b)] => (...)` for several at once, and `has '+name' => (...)`
/// for an override, whose type comes from the parent.
#[must_use]
pub fn read_has(call: &ast::Call, into: &mut Sink) -> Vec<AttributeDecl> {
    let arguments = call.args();
    let Some(first) = arguments.first() else {
        return Vec::new();
    };
    let names = attribute_names(first);
    if names.is_empty() {
        return Vec::new();
    }
    let options = arguments.get(1).map(Args::pairs).unwrap_or_default();

    let mut ty = Type::Unknown;
    let mut coerce = false;
    let mut access = Access::Rw;
    let mut required = false;
    let mut defaulted = false;
    let mut methods = Vec::new();
    let mut opaque_delegation = false;

    for option in &options {
        let value = option.node();
        match option.key() {
            Some("isa") => {
                if let Some(annotation) = crate::decl::annotation_of(value) {
                    ty = read_annotation(&annotation, into);
                }
            }
            Some("does") => {
                if let Some(role) = literal_name(value) {
                    ty = Type::ConsumerOf(role);
                }
            }
            Some("is") => {
                access = match literal_name(value).as_deref() {
                    Some("ro") | Some("lazy") | Some("rwp") => Access::Ro,
                    Some("wo") => Access::Wo,
                    _ => Access::Rw,
                };
            }
            Some("required") => required = is_true(value),
            Some("default") | Some("builder") | Some("lazy") | Some("lazy_build") => {
                defaulted = true;
            }
            Some("coerce") => {
                // A coerced slot accepts `Any` and yields the declared type
                // (`docs/types.md`, ANNOT-2a): the coercion is a function the
                // checker cannot see, so what may go *in* is anything — and
                // what comes back out is still what the slot was declared.
                coerce |= is_true(value);
            }
            Some(key @ ("reader" | "writer" | "accessor" | "predicate" | "clearer")) => {
                let role = match key {
                    "predicate" => AccessorRole::Predicate,
                    "clearer" => AccessorRole::Clearer,
                    "writer" => AccessorRole::Writer,
                    _ => AccessorRole::Reader,
                };
                if let Some(name) = literal_name(value) {
                    methods.push(GeneratedMethod { name, role });
                }
            }
            Some("handles") => match delegated(value) {
                Some(delegated) => {
                    methods.extend(delegated.into_iter().map(|name| GeneratedMethod {
                        name,
                        role: AccessorRole::Delegated,
                    }))
                }
                None => opaque_delegation = true,
            },
            _ => {}
        }
    }

    names
        .into_iter()
        .map(|name| {
            // `+name` overrides the parent's attribute; the type is the
            // parent's, which this pass has no way to reach.
            let overriding = name.starts_with('+');
            AttributeDecl {
                name: name.trim_start_matches('+').to_string(),
                ty: if overriding {
                    Type::Unknown
                } else {
                    ty.clone()
                },
                access,
                required,
                defaulted,
                coerce,
                methods: methods.clone(),
                opaque_delegation,
                builder: None,
                range: first.text_range(),
            }
        })
        .collect()
}

/// The names `has` was given: one, or a list in brackets.
fn attribute_names(node: &SyntaxNode) -> Vec<String> {
    match node.node_kind() {
        NodeKind::ANON_ARRAY => ast::AnonArray::cast(node.clone())
            .expect("kind checked")
            .elements()
            .iter()
            .flat_map(attribute_names)
            .collect(),
        NodeKind::QW_EXPR => ast::QwExpr::cast(node.clone())
            .expect("kind checked")
            .words(),
        _ => ast::key_text(node).into_iter().collect(),
    }
}

/// `handles => [qw(a b)]` and `handles => { local => 'remote' }` name their
/// delegates; a regexp or a role name does not.
fn delegated(node: &SyntaxNode) -> Option<Vec<String>> {
    match node.node_kind() {
        NodeKind::ANON_ARRAY => Some(
            ast::AnonArray::cast(node.clone())
                .expect("kind checked")
                .elements()
                .iter()
                .flat_map(attribute_names)
                .collect(),
        ),
        NodeKind::ANON_HASH => Some(
            AnonHash::cast(node.clone())
                .expect("kind checked")
                .pairs()
                .iter()
                .filter_map(|pair| pair.key().map(str::to_string))
                .collect(),
        ),
        _ => None,
    }
}

fn literal_name(node: &SyntaxNode) -> Option<String> {
    ast::key_text(node)
}

pub(crate) fn is_true(node: &SyntaxNode) -> bool {
    Literal::cast(node.clone())
        .and_then(|literal| literal.as_number())
        .is_none_or(|text| text != "0")
}

// ===== `Class::Accessor::Typed` =====

/// ```perl
/// use Class::Accessor::Typed (
///     rw => { name => 'Str', tags => 'ArrayRef[Str]' },
///     ro => { id => { isa => 'Int' } },
///     new => 1,
/// );
/// ```
///
/// A `use` statement whose argument list is a declaration.
#[must_use]
pub fn read_accessor_typed(arguments: &SyntaxNode, into: &mut Sink) -> (Vec<AttributeDecl>, bool) {
    let mut attributes = Vec::new();
    let mut constructor = true;
    for pair in Args::pairs(arguments) {
        let access = match pair.key() {
            Some("rw" | "rw_lazy") => Access::Rw,
            Some("ro" | "ro_lazy") => Access::Ro,
            Some("wo" | "wo_lazy") => Access::Wo,
            Some("new") => {
                constructor = is_true(pair.node());
                continue;
            }
            _ => continue,
        };
        let lazy = matches!(pair.key(), Some("rw_lazy" | "ro_lazy" | "wo_lazy"));
        let value = ast::without_plus(pair.node());
        if value.node_kind() != NodeKind::ANON_HASH {
            continue;
        }
        let hash = AnonHash::cast(value).expect("kind checked");
        for slot in hash.pairs() {
            let Some(name) = slot.key() else { continue };
            let (ty, required, defaulted) = read_slot(slot.node(), into);
            attributes.push(AttributeDecl {
                name: name.to_string(),
                ty,
                access,
                // A lazy slot is filled by its builder, so `new` skips it
                // rather than finding it missing.
                required: required && !lazy,
                defaulted: defaulted || lazy,
                coerce: false,
                methods: Vec::new(),
                opaque_delegation: false,
                // This family writes the type down; the builder adds nothing
                // the `isa` did not already say.
                builder: None,
                range: slot.range(),
            });
        }
    }
    (attributes, constructor)
}

/// A slot's value: a type, or a hashref with `isa` / `default` / `builder`.
///
/// Requiredness follows `Class::Accessor::Typed`'s rule, which is the reverse
/// of Moose's: a slot is **mandatory** unless it says `optional`, gives a
/// `default`, or is lazy. The generated `new` dies with "missing mandatory
/// parameter named '$x'" otherwise, so this is a rule and not a guess.
fn read_slot(node: &SyntaxNode, into: &mut Sink) -> (Type, bool, bool) {
    let node = ast::without_plus(node);
    if node.node_kind() == NodeKind::ANON_HASH {
        let hash = AnonHash::cast(node.clone()).expect("kind checked");
        let mut ty = Type::Unknown;
        let mut required = true;
        let mut defaulted = false;
        for option in hash.pairs() {
            match option.key() {
                Some("isa" | "does") => {
                    if let Some(annotation) = crate::decl::annotation_of(option.node()) {
                        ty = read_annotation(&annotation, into);
                    }
                }
                Some("required") => required = is_true(option.node()),
                Some("optional") => required = !is_true(option.node()),
                Some("default" | "builder" | "lazy") => defaulted = true,
                _ => {}
            }
        }
        return (ty, required, defaulted);
    }
    let ty = crate::decl::annotation_of(&node).map_or(Type::Unknown, |annotation| {
        read_annotation(&annotation, into)
    });
    (ty, true, false)
}

// ===== `Class::Accessor::Lite` =====

/// Which of the `mk_*` family a name is, and what it makes.
///
/// The family is shared: `Class::Accessor::Lite` installs into `caller`, and
/// `Class::Accessor` and its two speed variants are inherited from. Both are
/// read the same way, because both say the same thing — these names are
/// accessors of this package, and nothing about their types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessorMaker {
    /// Accessors, at this access, lazy or not.
    Accessors { access: Access, lazy: bool },
    /// `mk_new` — the constructor and nothing else.
    New,
    /// `mk_new_and_accessors` — both.
    NewAndAccessors,
    /// `follow_best_practice` — from here on the accessors are `get_x`/`set_x`.
    BestPractice,
}

impl AccessorMaker {
    /// What a method name in the family makes, or `None` for a name outside it.
    #[must_use]
    pub fn of(method: &str) -> Option<Self> {
        let accessors = |access, lazy| Some(AccessorMaker::Accessors { access, lazy });
        match method {
            "mk_accessors" => accessors(Access::Rw, false),
            "mk_ro_accessors" => accessors(Access::Ro, false),
            "mk_wo_accessors" => accessors(Access::Wo, false),
            "mk_lazy_accessors" => accessors(Access::Rw, true),
            "mk_ro_lazy_accessors" => accessors(Access::Ro, true),
            "mk_new" => Some(AccessorMaker::New),
            "mk_new_and_accessors" => Some(AccessorMaker::NewAndAccessors),
            "follow_best_practice" => Some(AccessorMaker::BestPractice),
            _ => None,
        }
    }
}

/// ```perl
/// use Class::Accessor::Lite (
///     new => 1,
///     rw  => [ qw(foo bar) ],
///     ro  => [ qw(baz) ],
///     wo  => [ qw(hoge) ],
/// );
/// use Class::Accessor::Lite::Lazy (
///     ro_lazy => [ 'hoge', { poyo => \&make_poyo, poe => 'make_poe' } ],
///     rw_lazy => { baz => 'make_baz' },
/// );
/// ```
///
/// A `use` statement whose argument list is a declaration, as
/// [`read_accessor_typed`] is — but the values are *names*, not types, so
/// every attribute here is [`Type::Unknown`]. Saying `Any` instead would be a
/// claim the module never made.
///
/// The constructor is opt-in: no `new => 1`, no `new`.
#[must_use]
pub fn read_accessor_lite(arguments: &SyntaxNode) -> (Vec<AttributeDecl>, bool) {
    let mut attributes = Vec::new();
    let mut constructor = false;
    for pair in Args::pairs(arguments) {
        let (access, lazy) = match pair.key() {
            Some("rw") => (Access::Rw, false),
            Some("rw_lazy") => (Access::Rw, true),
            Some("ro") => (Access::Ro, false),
            Some("ro_lazy") => (Access::Ro, true),
            Some("wo") => (Access::Wo, false),
            Some("new") => {
                constructor = is_true(pair.node());
                continue;
            }
            _ => continue,
        };
        let names = accessor_names(pair.node());
        attributes.extend(accessor_attributes(&names, access, lazy, pair.range()));
    }
    (attributes, constructor)
}

/// The names one argument of a `mk_accessors(...)` call spells out.
///
/// `qw(foo bar)`, `'foo'`, and a bareword all name accessors; anything
/// computed names none this pass can read.
///
/// The lazy makers take one shape more: `mk_lazy_accessors('foo', { bar =>
/// \&build })` flattens a hashref into name-and-builder pairs, so its keys
/// are names too. The plain makers do not — a reference passed to those is
/// stringified into an accessor nobody meant to ask for.
#[must_use]
pub fn listed_names(node: &SyntaxNode, lazy: bool) -> Vec<AccessorName> {
    if lazy {
        accessor_names(node)
    } else {
        attribute_names(node)
            .into_iter()
            .map(AccessorName::implicit)
            .collect()
    }
}

/// One accessor a declaration spells out, and the builder written beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessorName {
    pub name: String,
    pub builder: Builder,
}

impl AccessorName {
    fn implicit(name: String) -> Self {
        AccessorName {
            name,
            builder: Builder::Implicit,
        }
    }
}

/// Where a lazy slot's builder is, as the declaration wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Builder {
    /// Nothing written beside the name, which is the common shape:
    /// `Class::Accessor::Lite::Lazy` builds `foo` with `_build_foo`.
    Implicit,
    /// `{ poyo => \&make_poyo }` or `{ poe => 'make_poe' }` — either way a
    /// name, because the coderef form still resolves to a sub of this file.
    Named(String),
    /// `{ yyy => sub { ... } }` — a builder with no name to ask about.
    Anonymous,
}

/// The property names in one `rw => [...]` value.
///
/// An arrayref of names, a hashref of `name => builder`, or — the shape
/// `Class::Accessor::Lite::Lazy` documents — an arrayref holding both.
fn accessor_names(node: &SyntaxNode) -> Vec<AccessorName> {
    let node = ast::without_plus(node);
    match node.node_kind() {
        NodeKind::ANON_HASH => AnonHash::cast(node.clone())
            .expect("kind checked")
            .pairs()
            .iter()
            .filter_map(|slot| {
                Some(AccessorName {
                    name: slot.key()?.to_string(),
                    builder: builder_of(slot.node()),
                })
            })
            .collect(),
        NodeKind::ANON_ARRAY => ast::AnonArray::cast(node.clone())
            .expect("kind checked")
            .elements()
            .iter()
            .flat_map(accessor_names)
            .collect(),
        _ => attribute_names(&node)
            .into_iter()
            .map(AccessorName::implicit)
            .collect(),
    }
}

/// The builder a hashref slot names: `'make_poe'`, a bareword, or `\&make_poyo`.
///
/// An inline `sub { ... }` names nothing, and neither does anything computed.
fn builder_of(value: &SyntaxNode) -> Builder {
    if let Some(name) = ast::key_text(value) {
        return Builder::Named(name);
    }
    if value.node_kind() == NodeKind::REFERENCE_EXPR {
        let named = value
            .children()
            .next()
            .filter(|inner| inner.node_kind() == NodeKind::CODE_VAR)
            .and_then(ast::Variable::cast)
            .and_then(|code| code.name());
        if let Some(name) = named {
            return Builder::Named(name);
        }
    }
    Builder::Anonymous
}

/// One `mk_accessors` list, as attributes.
///
/// Everything here is `defaulted`: this family's `new` requires nothing, and
/// a lazy slot is filled by its builder, so either may be absent from a call.
#[must_use]
pub fn accessor_attributes(
    names: &[AccessorName],
    access: Access,
    lazy: bool,
    range: TextRange,
) -> Vec<AttributeDecl> {
    names
        .iter()
        .map(|listed| AttributeDecl {
            name: listed.name.clone(),
            ty: Type::Unknown,
            access,
            required: false,
            // Nothing here is required by the constructor, and a lazy slot is
            // filled by its builder, so `new` may leave either out.
            defaulted: true,
            coerce: false,
            methods: Vec::new(),
            opaque_delegation: false,
            builder: lazy
                .then(|| match &listed.builder {
                    Builder::Implicit => Some(format!("_build_{}", listed.name)),
                    Builder::Named(builder) => Some(builder.clone()),
                    Builder::Anonymous => None,
                })
                .flatten(),
            range,
        })
        .collect()
}

/// The `get_x` / `set_x` names `follow_best_practice` puts an accessor under.
#[must_use]
pub fn best_practice_methods(name: &str, access: Access) -> Vec<GeneratedMethod> {
    let mut methods = Vec::new();
    if access != Access::Wo {
        methods.push(GeneratedMethod {
            name: format!("get_{name}"),
            role: AccessorRole::Reader,
        });
    }
    if access != Access::Ro {
        methods.push(GeneratedMethod {
            name: format!("set_{name}"),
            role: AccessorRole::Writer,
        });
    }
    methods
}

// ===== `Type::Library` =====

/// One type a project's own library declares.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NamedType {
    pub name: String,
    pub ty: Type,
}

/// ```perl
/// declare 'PositiveInt', as Int, where { $_ > 0 };
/// type 'FooBar', as Foo | Bar;
/// class_type 'User', { class => 'MyApp::User' };
/// role_type 'Loggable';
/// enum 'Color', [qw(red green blue)];
/// union 'Id', [Int, Str];
/// intersection 'Both', [Foo, Bar];
/// ```
///
/// `as T` gives the parent and `where` is ignored: the structural part of a
/// constraint is what the checker can use, and the predicate is a run-time
/// refinement it cannot.
#[must_use]
pub fn read_type_library(call: &ast::Call, into: &mut Sink) -> Option<NamedType> {
    let callee = call.callee_name()?;
    let arguments = call.args();
    let name = arguments.first().and_then(ast::key_text)?;

    let ty = match callee.as_str() {
        "declare" | "subtype" | "type" => {
            // `as T` is a call whose argument is the parent type.
            arguments
                .iter()
                .skip(1)
                .find_map(|node| as_clause(node, into))
                .unwrap_or(Type::Any)
        }
        "class_type" => arguments
            .get(1)
            .and_then(|node| hash_value(node, "class"))
            .map_or_else(|| Type::InstanceOf(name.clone()), Type::InstanceOf),
        "role_type" => arguments
            .get(1)
            .and_then(|node| hash_value(node, "role"))
            .map_or_else(|| Type::ConsumerOf(name.clone()), Type::ConsumerOf),
        "duck_type" => Type::Object,
        "enum" => Type::Enum(arguments.get(1).map(attribute_names).unwrap_or_default()),
        // The lattice has no intersection, so the name resolves to `Unknown`:
        // not analysed, never reported against. That is still worth reading —
        // a name nothing declares is read as a *class* instead, and a class the
        // run cannot find is what `unknown-type` fires on.
        "intersection" => Type::Unknown,
        "union" => Type::union(
            arguments
                .get(1)
                .map(|node| {
                    ast::AnonArray::cast(node.clone())
                        .map(|array| {
                            array
                                .elements()
                                .iter()
                                .map(|element| member_type(element, into))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
        ),
        _ => return None,
    };
    Some(NamedType { name, ty })
}

/// `as Int` — the parent of a declared type.
fn as_clause(node: &SyntaxNode, into: &mut Sink) -> Option<Type> {
    let call = ast::Call::cast(node.clone())?;
    if call.callee_name().as_deref() != Some("as") {
        return None;
    }
    let inner = call.args().into_iter().next()?;
    Some(member_type(&inner, into))
}

fn member_type(node: &SyntaxNode, into: &mut Sink) -> Type {
    crate::decl::annotation_of(node).map_or(Type::Unknown, |annotation| {
        read_annotation(&annotation, into)
    })
}

/// The value of one key of a hashref argument.
fn hash_value(node: &SyntaxNode, key: &str) -> Option<String> {
    let hash = AnonHash::cast(ast::without_plus(node))?;
    hash.pairs()
        .iter()
        .find(|pair| pair.key() == Some(key))
        .and_then(|pair| ast::key_text(pair.node()))
}

// ===== `Returns:` =====

/// What a sub gives back, in each of the two contexts Perl has.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Returns {
    pub scalar: Type,
    pub list: ListShape,
    /// Read off the body rather than written down (`docs/return-inference.md`).
    ///
    /// It changes nothing about how the type is used — an inferred return
    /// yields the same value at a call site as a written one — and two things
    /// about how it is talked about: hover says so, and
    /// `--strict-annotations` still asks for the annotation.
    #[serde(default)]
    pub inferred: bool,
    /// The `InstanceOf[own package]` member of `scalar` stands for *the class
    /// the sub was called on* rather than the one it was written in.
    ///
    /// A sub that returns its invocant — every builder in a corpus — is a
    /// `Child` when a `Child` called it, and saying `Base` instead is an
    /// `unknown-method` on the next link of the chain
    /// (`docs/return-inference.md`, "`$self` comes back as the caller's
    /// class"). The substitution is the call site's:
    /// [`crate::flow`] does it where it resolved the receiver.
    #[serde(default)]
    pub invocant: bool,
}

impl Default for Returns {
    fn default() -> Self {
        Returns {
            scalar: Type::Unknown,
            list: ListShape::Unknown,
            inferred: false,
            invocant: false,
        }
    }
}

impl Returns {
    /// What a sub returns, as the return walk read it off the body.
    #[must_use]
    pub fn inferred(scalar: Type, list: ListShape, invocant: bool) -> Self {
        Returns {
            scalar,
            list,
            inferred: true,
            invocant,
        }
    }

    /// Whether the walk has anything left to say about this sub.
    ///
    /// The tiers iterate over the subs that are still `Unknown`, and this is
    /// the test: an annotated sub is never inferred, and an inferred type is
    /// final once it is known (`docs/return-inference.md`, "Two tiers").
    #[must_use]
    pub fn is_unresolved(&self) -> bool {
        self.scalar.is_unknown() && self.list == ListShape::Unknown
    }

    /// Whether the walk may read this sub's return off its body at all.
    ///
    /// Not a written `Returns:`, which wins over anything a body says. What is
    /// left is a sub nothing is known about yet and one the walk has already
    /// answered — and the second is a candidate because an *edit* can change
    /// what the answer is (`docs/return-inference.md`, step 4′).
    #[must_use]
    pub fn is_inferable(&self) -> bool {
        self.inferred || self.is_unresolved()
    }
}

/// The shape of what a sub returns in list context.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ListShape {
    #[default]
    Unknown,
    /// `Returns: ()` — nothing. Calling it where the value is used is a
    /// diagnostic.
    Nothing,
    /// `Returns: (Str, Int)` — a known length, and a type per slot.
    Fixed(Vec<Type>),
    /// `Returns: (Row ...)` — any length, one element type.
    Of(Type),
    /// `Returns: (Value, Undef) | (Undef, Error)` — several fixed shapes of
    /// one length, one of which a call hands back.
    ///
    /// The ok-or-error idiom is written this way in half a corpus, and joining
    /// its two `return`s slot-wise into `(Value|Undef, Error|Undef)` throws
    /// away the only thing they say: *which* slot is filled when. Every
    /// alternative has the same number of slots, there are at least two of
    /// them, and no two are the same.
    Either(Vec<Vec<Type>>),
}

impl ListShape {
    /// A list of a known length, or `Unknown` when nothing is known about any
    /// of its slots.
    ///
    /// A `(Unknown)` is a shape that says only "one value", and it would cost
    /// more than it says: the tiers take a shape that is not `Unknown` as
    /// final, so a sub whose slots were all `Unknown` in the first round would
    /// never be looked at again — and the whole point of the second tier is
    /// that a later round knows more. An empty list is not this case: a
    /// length of zero is something.
    #[must_use]
    pub fn fixed(types: Vec<Type>) -> ListShape {
        if !types.is_empty() && types.iter().all(Type::is_unknown) {
            return ListShape::Unknown;
        }
        ListShape::Fixed(types)
    }

    /// Any number of one type, or `Unknown` when the type is not known — which
    /// is "a list this pass can say nothing about" rather than "a list of
    /// nothing".
    #[must_use]
    pub fn of(ty: Type) -> ListShape {
        if ty.is_unknown() {
            ListShape::Unknown
        } else {
            ListShape::Of(ty)
        }
    }

    /// Several fixed shapes of one length, or the one they collapse to.
    ///
    /// Deduplicated, because two `return`s of the same shape are one shape;
    /// collapsed to a single `Fixed` when that is all that is left; and
    /// collapsed slot-wise past [`ListShape::ALTERNATIVES`], where the
    /// alternation has stopped being the thing the sub is about and the
    /// slot-wise union is the honest summary.
    #[must_use]
    pub fn either(shapes: Vec<Vec<Type>>) -> ListShape {
        let mut kept: Vec<Vec<Type>> = Vec::new();
        for shape in shapes {
            if !kept.contains(&shape) {
                kept.push(shape);
            }
        }
        match kept.len() {
            0 => ListShape::Unknown,
            1 => ListShape::fixed(kept.remove(0)),
            // One slot has nothing to be correlated *with*, so an alternation
            // of them says exactly what the union of them says.
            _ if width(&kept) <= 1 => ListShape::fixed(slotwise(&kept)),
            _ if kept.len() > Self::ALTERNATIVES => ListShape::fixed(slotwise(&kept)),
            _ => ListShape::Either(kept),
        }
    }

    /// How many alternatives are worth keeping apart.
    ///
    /// Two is the idiom this exists for. A sub with five differently-shaped
    /// `return`s is not describing a choice a caller makes decisions from, and
    /// a five-way alternation in a signature reads worse than the union does.
    pub const ALTERNATIVES: usize = 3;

    /// The join of two shapes (`docs/return-inference.md`, "The shape").
    ///
    /// Two shapes of the same length are kept *apart*, as alternatives, rather
    /// than joined slot-wise: `return (undef, $err)` beside `return ($value,
    /// undef)` is `(Undef, Error) | (Value, Undef)`, and the slot-wise answer
    /// would say the two slots vary independently, which is exactly what the
    /// idiom promises they do not. A length that does not agree, or an `Of` on
    /// either side, is `Of` of every member joined — because once the length
    /// is not known, neither is which slot is which. Anything against
    /// `Unknown` is `Unknown`, which is the same rule the scalar half's join
    /// has and for the same reason.
    #[must_use]
    pub fn join(self, other: ListShape) -> ListShape {
        if self == ListShape::Unknown || other == ListShape::Unknown {
            return ListShape::Unknown;
        }
        match (self.alternatives(), other.alternatives()) {
            (Some(left), Some(right)) if width(&left) == width(&right) => {
                let mut shapes = left;
                shapes.extend(right);
                ListShape::either(shapes)
            }
            _ => {
                let mut members = self.members();
                members.extend(other.members());
                ListShape::of(Type::union(members))
            }
        }
    }

    /// The shapes this may be, when the length is known. `Returns: ()` is a
    /// list of none, which is what makes it agree with `(Str)` being a list of
    /// one.
    #[must_use]
    fn alternatives(&self) -> Option<Vec<Vec<Type>>> {
        match self {
            ListShape::Nothing => Some(vec![Vec::new()]),
            ListShape::Fixed(types) => Some(vec![types.clone()]),
            ListShape::Either(shapes) => Some(shapes.clone()),
            ListShape::Unknown | ListShape::Of(_) => None,
        }
    }

    /// The slots, when the length is known — one type per position.
    ///
    /// An alternation answers slot-wise here, which is what keeps every reader
    /// of a *position* working unchanged: `my ($value, $error) = f()` binds
    /// `Value|Undef` and `Undef|Error` whether or not the two are correlated,
    /// because nothing here can carry the correlation past the assignment.
    /// What the alternation is for is what gets *shown* and what gets written
    /// down.
    #[must_use]
    pub fn slots(&self) -> Option<Vec<Type>> {
        match self {
            ListShape::Nothing => Some(Vec::new()),
            ListShape::Fixed(types) => Some(types.clone()),
            ListShape::Either(shapes) => Some(slotwise(shapes)),
            ListShape::Unknown | ListShape::Of(_) => None,
        }
    }

    /// Every type an element of this list may have.
    #[must_use]
    pub fn members(&self) -> Vec<Type> {
        match self {
            ListShape::Unknown => vec![Type::Unknown],
            ListShape::Nothing => Vec::new(),
            ListShape::Fixed(types) => types.clone(),
            ListShape::Of(ty) => vec![ty.clone()],
            ListShape::Either(shapes) => shapes.concat(),
        }
    }

    /// How the annotation for this shape is written, which is how hover shows
    /// it too.
    #[must_use]
    pub fn written(&self) -> Option<String> {
        match self {
            ListShape::Unknown => None,
            ListShape::Nothing => Some("()".to_string()),
            ListShape::Fixed(types) => Some(format!(
                "({})",
                types
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            ListShape::Of(ty) => Some(format!("({ty} ...)")),
            ListShape::Either(shapes) => Some(
                shapes
                    .iter()
                    .map(|slots| {
                        format!(
                            "({})",
                            slots
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" | "),
            ),
        }
    }
}

/// How many slots the alternatives have, which is one number because they all
/// have it.
fn width(shapes: &[Vec<Type>]) -> usize {
    shapes.first().map_or(0, Vec::len)
}

/// The union at each position, which is what an alternation collapses to when
/// nothing can carry the correlation any further.
fn slotwise(shapes: &[Vec<Type>]) -> Vec<Type> {
    (0..width(shapes))
        .map(|slot| {
            Type::union(
                shapes
                    .iter()
                    .filter_map(|one| one.get(slot).cloned())
                    .collect(),
            )
        })
        .collect()
}

/// Read the `Returns:` lines out of a sub's leading comment block.
///
/// Grammar: within the comment block immediately preceding a `sub` (blank
/// lines allowed between the block and the `sub`, not within it), a line whose
/// comment text after `#` and whitespace starts with `Returns:` — in any case,
/// so `# returns: Str` is the same annotation. The rest is
/// one of four things:
///
/// ```text
/// # Returns: Str               scalar context: Str
/// # Returns: (Str, Int)        list context: exactly two, Str then Int
/// # Returns: (Row ...)         list context: any number of Row
/// # Returns: ()                nothing
/// ```
///
/// A sub with both halves writes **two lines**, one of each kind, in either
/// order — so every line in the block is read rather than the first. A second
/// line of the same kind is a `bad-annotation`: two answers to one question
/// is a question nobody answered.
///
/// A list-only annotation says nothing about scalar context and a scalar-only
/// one nothing about list context. The comma operator would make `Returns: (A,
/// B)` a `B` in scalar context and `(Row ...)` a count; the two rules
/// disagree, and a sub that wants a scalar type writes one.
#[must_use]
pub fn read_returns(definition: &SubDef, into: &mut Sink) -> Option<Returns> {
    let mut returns = Returns::default();
    let mut found = false;
    let (mut scalar_seen, mut list_seen) = (false, false);
    for comment in definition.leading_comments() {
        let Some(body) = annotation_body(&comment) else {
            continue;
        };
        found = true;
        let range = comment.text_range();
        match parse_returns(&body, range, into) {
            Written::Scalar(ty) => {
                if scalar_seen {
                    bad_returns(&body, range, "`Returns:` names a scalar type twice", into);
                } else {
                    scalar_seen = true;
                    returns.scalar = ty;
                }
            }
            Written::List(shape) => {
                if list_seen {
                    bad_returns(&body, range, "`Returns:` names a list shape twice", into);
                } else {
                    list_seen = true;
                    returns.list = shape;
                }
            }
            // `Returns: ()` is one statement about both contexts: nothing
            // comes back, and in scalar context nothing is `undef`.
            Written::Empty => {
                if scalar_seen || list_seen {
                    bad_returns(&body, range, "`Returns: ()` is the whole answer", into);
                } else {
                    scalar_seen = true;
                    list_seen = true;
                    returns.scalar = Type::Undef;
                    returns.list = ListShape::Nothing;
                }
            }
            Written::Silent => {}
        }
    }
    found.then_some(returns)
}

/// The text after `# Returns:`, if this comment is one.
///
/// The keyword is matched without regard to case: `# returns: Str` is the same
/// annotation, and a reader who wrote it meant it.
fn annotation_body(token: &SyntaxToken) -> Option<String> {
    const KEYWORD: &str = "Returns:";
    let text = token.text().trim_start_matches('#').trim_start();
    let (head, rest) = text.split_at_checked(KEYWORD.len())?;
    head.eq_ignore_ascii_case(KEYWORD)
        .then(|| rest.trim().to_string())
}

/// What one `Returns:` line says.
enum Written {
    Scalar(Type),
    List(ListShape),
    /// `Returns: ()`.
    Empty,
    /// Prose, or something already reported.
    Silent,
}

fn bad_returns(body: &str, range: TextRange, message: &str, into: &mut Sink) {
    into.diagnostics.push(Diagnostic::new(
        Code::BadAnnotation,
        range,
        format!("`Returns: {body}` does not parse: {message}"),
    ));
}

fn parse_returns(body: &str, range: TextRange, into: &mut Sink) -> Written {
    let body = body.trim();
    if body.is_empty() {
        return Written::Silent;
    }
    // The form this replaced, named so that the message can show the new one.
    if body.contains("list:") {
        bad_returns(
            body,
            range,
            "a list shape is written `(T, U)` or `(T ...)`, on a `Returns:` line of its own",
            into,
        );
        return Written::Silent;
    }

    if let Some(written) = alternation(body, range, into) {
        return written;
    }

    let Some(inner) = list_body(body) else {
        // A `Returns:` that is prose rather than an annotation is not a broken
        // annotation; see `types::is_type_shaped`.
        if !types::is_type_shaped(body) {
            return Written::Silent;
        }
        return match types::parse(body) {
            Ok(ty) => {
                into.note(&ty, range);
                Written::Scalar(ty)
            }
            Err(error) => {
                bad_returns(body, range, &error.message, into);
                Written::Silent
            }
        };
    };

    let inner = inner.trim();
    if inner.is_empty() {
        return Written::Empty;
    }
    // `(Row ...)` — any number of one type. Only where there is one slot to
    // repeat: `(Str, Int ...)` names no shape this has.
    if let Some(repeated) = inner.strip_suffix("...") {
        let repeated = repeated.trim().trim_end_matches(',').trim();
        if repeated.is_empty() || split_top_level(repeated).len() != 1 {
            bad_returns(body, range, "`...` repeats one type, so there is one", into);
            return Written::Silent;
        }
        if !types::is_type_shaped(repeated) {
            return Written::Silent;
        }
        return match types::parse(repeated) {
            Ok(ty) => {
                into.note(&ty, range);
                Written::List(ListShape::Of(ty))
            }
            Err(error) => {
                bad_returns(body, range, &error.message, into);
                Written::Silent
            }
        };
    }

    match fixed_slots(inner, body, range, into) {
        Some(members) => Written::List(ListShape::Fixed(members)),
        None => Written::Silent,
    }
}

/// The slots of one parenthesised list body, or `None` where it is prose or
/// does not parse.
fn fixed_slots(inner: &str, body: &str, range: TextRange, into: &mut Sink) -> Option<Vec<Type>> {
    let parts = split_top_level(inner);
    // Prose in parentheses is still prose: `# Returns: (see below)`.
    if parts.iter().any(|part| !types::is_type_shaped(part.trim())) {
        return None;
    }
    let mut members = Vec::new();
    for part in parts {
        match types::parse(part.trim()) {
            Ok(ty) => {
                into.note(&ty, range);
                members.push(ty);
            }
            Err(error) => {
                bad_returns(body, range, &error.message, into);
                return None;
            }
        }
    }
    Some(members)
}

/// `(Value, Undef) | (Undef, Error)` — the alternatives of a list shape.
///
/// Only where *every* part is parenthesised from its first character to its
/// last: `Str | Undef` is a scalar union and the `|` in it belongs to the type
/// language, not to this.
fn alternation(body: &str, range: TextRange, into: &mut Sink) -> Option<Written> {
    let parts = split_top_level_on(body, '|');
    if parts.len() < 2 {
        return None;
    }
    let bodies: Vec<&str> = parts
        .iter()
        .map(|part| list_body(part.trim()))
        .collect::<Option<Vec<_>>>()?;
    let mut shapes = Vec::new();
    for inner in bodies {
        shapes.push(fixed_slots(inner.trim(), body, range, into)?);
    }
    if shapes.iter().any(|one| one.len() != shapes[0].len()) {
        bad_returns(
            body,
            range,
            "the alternatives of a list shape all have the same number of slots",
            into,
        );
        return Some(Written::Silent);
    }
    Some(Written::List(ListShape::either(shapes)))
}

/// The inside of a body that is parenthesised from its first character to its
/// last, which is what makes it a list shape.
///
/// A grouping parenthesis around a whole scalar type has no use that `Str |
/// Undef` does not serve, so nothing is lost by spending the notation here.
/// Parentheses *inside* a slot keep grouping, which is why `(Str | Undef,
/// Int)` is two slots and `(Str) | (Int)` is not a list at all.
fn list_body(body: &str) -> Option<&str> {
    let inner = body.strip_prefix('(')?.strip_suffix(')')?;
    let mut depth = 0usize;
    for ch in inner.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    Some(inner)
}

/// Split on commas that are not inside brackets.
fn split_top_level(text: &str) -> Vec<String> {
    split_top_level_on(text, ',')
}

/// The same, on any separator: `|` is what separates the alternatives of a
/// list shape, and it is inside a slot as often as it separates one.
fn split_top_level_on(text: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in text.chars() {
        match ch {
            '[' | '(' => {
                depth += 1;
                current.push(ch);
            }
            ']' | ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            _ if ch == separator && depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

#[cfg(test)]
mod tests;
