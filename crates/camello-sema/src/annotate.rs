//! The annotation recognisers (`docs/typecheck.md`, "Annotation sources").
//!
//! Each is a match on a declaration shape that yields symbols or parameter
//! lists. None of them is special-cased in the parser: `has` is a
//! `LIST_CALL_EXPR` like every other bareword (`camello dev dump` shows it) and
//! stays one.
//!
//! Recognition is by callee name **and** by an import that could have provided
//! it, so a project's own `sub has` is not mistaken for Moose's. That test is
//! [`Frameworks`], which the declaration pass fills in from the file's `use`
//! statements before it reads anything else.

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

/// What the file's imports say a bareword could mean.
///
/// The point of this is one line in the design: recognition is by callee name
/// *and* by an import that could have provided it. A file that never says
/// `use Moose` has no `has` to recognise, whatever it calls its own subs.
#[derive(Debug, Clone, Default)]
pub struct Frameworks {
    pub moose: bool,
    pub smart_args: bool,
    pub accessor_typed: bool,
    pub accessor_lite: bool,
    pub type_library: bool,
}

impl Frameworks {
    /// Fold one `use Foo` into what the file can be expected to mean.
    pub fn note(&mut self, module: &str) {
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
        match module {
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
    /// The methods this attribute generates besides the accessor itself:
    /// `reader`, `writer`, `predicate`, `clearer`, and whatever `handles`
    /// delegates.
    pub methods: Vec<String>,
    /// `handles` naming a regexp or a role: the delegated set is unknowable,
    /// so the class may have any method and "no such method" is off.
    pub opaque_delegation: bool,
    #[serde(with = "crate::serde_range")]
    pub range: TextRange,
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
                // (`docs/typecheck.md`, non-goals): the coercion is a function
                // the checker cannot see.
                if is_true(value) {
                    ty = Type::Unknown;
                }
            }
            Some(key @ ("reader" | "writer" | "accessor" | "predicate" | "clearer")) => {
                let _ = key;
                if let Some(name) = literal_name(value) {
                    methods.push(name);
                }
            }
            Some("handles") => match delegated(value) {
                Some(delegated) => methods.extend(delegated),
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
                methods: methods.clone(),
                opaque_delegation,
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

fn is_true(node: &SyntaxNode) -> bool {
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
        if pair.node().node_kind() != NodeKind::ANON_HASH {
            continue;
        }
        let hash = AnonHash::cast(pair.node().clone()).expect("kind checked");
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
                methods: Vec::new(),
                opaque_delegation: false,
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
    let ty = crate::decl::annotation_of(node).map_or(Type::Unknown, |annotation| {
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
        let access = match pair.key() {
            Some("rw" | "rw_lazy") => Access::Rw,
            Some("ro" | "ro_lazy") => Access::Ro,
            Some("wo") => Access::Wo,
            Some("new") => {
                constructor = is_true(pair.node());
                continue;
            }
            _ => continue,
        };
        let names = accessor_names(pair.node());
        attributes.extend(accessor_attributes(&names, access, pair.range()));
    }
    (attributes, constructor)
}

/// The names one argument of a `mk_accessors(...)` call spells out.
///
/// `qw(foo bar)`, `'foo'`, and a bareword all name accessors; anything
/// computed names none this pass can read.
#[must_use]
pub fn listed_names(node: &SyntaxNode) -> Vec<String> {
    attribute_names(node)
}

/// The property names in one `rw => [...]` value.
///
/// An arrayref of names, a hashref of `name => builder`, or — the shape
/// `Class::Accessor::Lite::Lazy` documents — an arrayref holding both.
fn accessor_names(node: &SyntaxNode) -> Vec<String> {
    match node.node_kind() {
        NodeKind::ANON_HASH => AnonHash::cast(node.clone())
            .expect("kind checked")
            .pairs()
            .iter()
            .filter_map(|slot| slot.key().map(str::to_string))
            .collect(),
        NodeKind::ANON_ARRAY => ast::AnonArray::cast(node.clone())
            .expect("kind checked")
            .elements()
            .iter()
            .flat_map(accessor_names)
            .collect(),
        _ => attribute_names(node),
    }
}

/// One `mk_accessors` list, as attributes.
///
/// Everything here is `defaulted`: this family's `new` requires nothing, and
/// a lazy slot is filled by its builder, so either may be absent from a call.
#[must_use]
pub fn accessor_attributes(
    names: &[String],
    access: Access,
    range: TextRange,
) -> Vec<AttributeDecl> {
    names
        .iter()
        .map(|name| AttributeDecl {
            name: name.clone(),
            ty: Type::Unknown,
            access,
            required: false,
            // Nothing here is required by the constructor, and a lazy slot is
            // filled by its builder, so `new` may leave either out.
            defaulted: true,
            methods: Vec::new(),
            opaque_delegation: false,
            range,
        })
        .collect()
}

/// The `get_x` / `set_x` names `follow_best_practice` puts an accessor under.
#[must_use]
pub fn best_practice_methods(name: &str, access: Access) -> Vec<String> {
    let mut methods = Vec::new();
    if access != Access::Wo {
        methods.push(format!("get_{name}"));
    }
    if access != Access::Ro {
        methods.push(format!("set_{name}"));
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
    let hash = AnonHash::cast(node.clone())?;
    hash.pairs()
        .iter()
        .find(|pair| pair.key() == Some(key))
        .and_then(|pair| ast::key_text(pair.node()))
}

// ===== `Returns:` =====

/// What a sub gives back, in each of the two contexts Perl has.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Returns {
    pub scalar: Type,
    pub list: ListShape,
}

impl Default for Returns {
    fn default() -> Self {
        Returns {
            scalar: Type::Unknown,
            list: ListShape::Unknown,
        }
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
    /// `Returns: ... | list: (Str, Int)`.
    Fixed(Vec<Type>),
}

/// Read the `Returns:` line out of a sub's leading comment block.
///
/// Grammar: within the comment block immediately preceding a `sub` (blank
/// lines allowed between the block and the `sub`, not within it), a line whose
/// comment text after `#` and whitespace starts with `Returns:`. The rest is
/// `<type>` for scalar context, `list: (<type>, ...)` for list context, both
/// joined by `|`, or `()` for "returns nothing".
#[must_use]
pub fn read_returns(definition: &SubDef, into: &mut Sink) -> Option<Returns> {
    let comment = definition
        .leading_comments()
        .into_iter()
        .find(|token| annotation_body(token).is_some())?;
    let (range, body) = (comment.text_range(), annotation_body(&comment)?);
    Some(parse_returns(&body, range, into))
}

/// The text after `# Returns:`, if this comment is one.
fn annotation_body(token: &SyntaxToken) -> Option<String> {
    let text = token.text().trim_start_matches('#').trim_start();
    text.strip_prefix("Returns:")
        .map(|rest| rest.trim().to_string())
}

fn parse_returns(body: &str, range: TextRange, into: &mut Sink) -> Returns {
    let mut returns = Returns::default();
    let bad = |message: String, into: &mut Sink| {
        into.diagnostics.push(Diagnostic::new(
            Code::BadAnnotation,
            range,
            format!("`Returns: {body}` does not parse: {message}"),
        ));
    };

    // `... | list: (...)` splits into the two contexts.
    let (scalar_part, list_part) = match body.find("list:") {
        Some(position) => {
            let scalar = body[..position].trim().trim_end_matches('|').trim();
            (scalar, Some(body[position + "list:".len()..].trim()))
        }
        None => (body.trim(), None),
    };

    // A `Returns:` that is prose rather than an annotation is not a broken
    // annotation; see `types::is_type_shaped`.
    if !scalar_part.is_empty() && scalar_part != "()" && !types::is_type_shaped(scalar_part) {
        return Returns::default();
    }

    if scalar_part == "()" {
        returns.list = ListShape::Nothing;
        returns.scalar = Type::Undef;
    } else if !scalar_part.is_empty() {
        match types::parse(scalar_part) {
            Ok(ty) => {
                into.note(&ty, range);
                returns.scalar = ty;
            }
            Err(error) => bad(error.message, into),
        }
    }

    if let Some(list) = list_part {
        let inner = list
            .strip_prefix('(')
            .and_then(|rest| rest.strip_suffix(')'));
        match inner {
            Some(inner) if inner.trim().is_empty() => returns.list = ListShape::Fixed(Vec::new()),
            Some(inner) => {
                let mut members = Vec::new();
                let mut ok = true;
                for part in split_top_level(inner) {
                    match types::parse(part.trim()) {
                        Ok(ty) => {
                            into.note(&ty, range);
                            members.push(ty);
                        }
                        Err(error) => {
                            bad(error.message, into);
                            ok = false;
                            break;
                        }
                    }
                }
                if ok {
                    returns.list = ListShape::Fixed(members);
                }
            }
            None => bad("a `list:` shape is written `(T, U)`".to_string(), into),
        }
    }
    returns
}

/// Split on commas that are not inside brackets.
fn split_top_level(text: &str) -> Vec<String> {
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
            ',' if depth == 0 => {
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
