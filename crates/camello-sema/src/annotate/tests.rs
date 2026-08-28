//! The recognisers, asked of the shapes the design document writes down.

use camello_syntax::lang::SyntaxNode;

use super::*;
use crate::decl;

fn parse(source: &str) -> SyntaxNode {
    let parsed = camello_syntax::parse::parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture must parse: {:?}",
        parsed.diagnostics
    );
    parsed.syntax()
}

fn attributes(source: &str) -> Vec<AttributeDecl> {
    let root = parse(source);
    decl::declare(&root)
        .facts
        .into_iter()
        .flat_map(|facts| facts.attributes)
        .collect()
}

/// The names of a set of generated methods, for the assertions below.
fn names(methods: &[crate::annotate::GeneratedMethod]) -> Vec<String> {
    methods.iter().map(|method| method.name.clone()).collect()
}

#[test]
fn has_yields_an_attribute() {
    let found = attributes("use Moose;\nhas name => (is => 'ro', isa => 'Str', required => 1);\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "name");
    assert_eq!(found[0].ty, Type::Str);
    assert_eq!(found[0].access, Access::Ro);
    assert!(found[0].required);
    assert!(!found[0].defaulted);
}

#[test]
fn has_is_only_moose_where_moose_was_imported() {
    // A project's own `sub has` is not Moose's (`docs/typecheck.md`,
    // "Annotation sources").
    assert!(attributes("has name => (is => 'ro', isa => 'Str');\n").is_empty());
}

#[test]
fn has_reads_a_bareword_type_expression() {
    // `MyItem` rather than `Item`, which Types::Standard already has as a
    // name for `Any`.
    let found = attributes("use Moose;\nhas items => (is => 'rw', isa => ArrayRef[MyItem]);\n");
    assert_eq!(
        found[0].ty,
        Type::ArrayRef(Box::new(Type::InstanceOf("MyItem".into())))
    );
}

#[test]
fn has_takes_a_list_of_names() {
    let found = attributes("use Moose;\nhas [qw(a b)] => (is => 'ro', isa => 'Int');\n");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].name, "a");
    assert_eq!(found[1].name, "b");
    assert_eq!(found[1].ty, Type::Int);
}

#[test]
fn an_override_takes_its_type_from_the_parent() {
    let found = attributes("use Moose;\nhas '+name' => (default => 'x');\n");
    assert_eq!(found[0].name, "name");
    assert_eq!(found[0].ty, Type::Unknown);
    assert!(found[0].defaulted);
}

#[test]
fn does_gives_a_role() {
    let found = attributes("use Moose;\nhas log => (is => 'ro', does => 'Loggable');\n");
    assert_eq!(found[0].ty, Type::ConsumerOf("Loggable".into()));
}

#[test]
fn coerce_widens_what_goes_in_and_not_what_comes_out() {
    // The coercion is a function the checker cannot see, so a coerced slot
    // accepts anything — while the reader still gives back what the slot was
    // declared (`docs/types.md`, ANNOT-2a).
    let found = attributes("use Moose;\nhas at => (is => 'ro', isa => 'Int', coerce => 1);\n");
    assert_eq!(found[0].ty, Type::Int);
    assert!(found[0].coerce);
    assert_eq!(found[0].returns("at"), Type::Int);
    assert_eq!(found[0].accepts(), Type::Any);

    let plain = attributes("use Moose;\nhas at => (is => 'ro', isa => 'Int');\n");
    assert!(!plain[0].coerce);
    assert_eq!(plain[0].accepts(), Type::Int);
}

#[test]
fn named_accessors_are_methods_of_their_own() {
    let found = attributes(
        "use Moose;\nhas name => (is => 'ro', writer => 'set_name', predicate => 'has_name');\n",
    );
    assert!(found[0].answers_to("set_name"));
    assert!(found[0].answers_to("has_name"));
    // A `predicate` says whether the slot is filled, not what is in it.
    assert_eq!(found[0].returns("has_name"), Type::Bool);
    assert_eq!(found[0].returns("set_name"), found[0].ty);
    assert_eq!(found[0].returns("name"), found[0].ty);
}

#[test]
fn a_regexp_in_handles_makes_the_delegation_opaque() {
    let found = attributes("use Moose;\nhas c => (is => 'ro', handles => qr/^x/);\n");
    assert!(found[0].opaque_delegation);
    let listed = attributes("use Moose;\nhas c => (is => 'ro', handles => [qw(a b)]);\n");
    assert!(!listed[0].opaque_delegation);
    assert_eq!(names(&listed[0].methods), vec!["a", "b"]);
    // A delegated method is another class's, and nothing here read it.
    assert_eq!(listed[0].returns("a"), Type::Unknown);
}

#[test]
fn class_accessor_typed_declares_the_same_way() {
    let found = attributes(
        "use Class::Accessor::Typed (\n    rw => { name => 'Str' },\n    ro => { id => { isa => 'Int' } },\n    new => 1,\n);\n",
    );
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].name, "name");
    assert_eq!(found[0].ty, Type::Str);
    assert_eq!(found[0].access, Access::Rw);
    assert_eq!(found[1].name, "id");
    assert_eq!(found[1].ty, Type::Int);
    assert_eq!(found[1].access, Access::Ro);
}

#[test]
fn new_zero_removes_the_constructor() {
    let root = parse("use Class::Accessor::Typed (rw => { a => 'Str' }, new => 0);\n");
    let decls = decl::declare(&root);
    assert!(!decls.facts[0].constructor);
}

#[test]
fn class_accessor_lite_declares_its_accessors() {
    let root = parse(
        "package L;\nuse Class::Accessor::Lite (\n  new => 1,\n  rw => [qw(foo bar)],\n  ro => [qw(baz)],\n  wo => [qw(quux)],\n);\n",
    );
    let decls = decl::declare(&root);
    let facts = decls.facts_for("L").expect("the package");
    assert!(facts.constructor, "`new => 1` asked for one");
    assert!(facts.open_constructor, "it blesses whatever it is handed");
    let names: Vec<(&str, Access)> = facts
        .attributes
        .iter()
        .map(|attribute| (attribute.name.as_str(), attribute.access))
        .collect();
    assert_eq!(
        names,
        [
            ("foo", Access::Rw),
            ("bar", Access::Rw),
            ("baz", Access::Ro),
            ("quux", Access::Wo),
        ]
    );
    assert!(
        facts.attributes.iter().all(|one| one.ty == Type::Unknown),
        "the module says nothing about types"
    );
}

#[test]
fn a_lazy_accessor_is_named_however_its_builder_is_given() {
    let root = parse(
        "package L;\nuse Class::Accessor::Lite::Lazy (\n  ro_lazy => ['hoge', { poyo => \\&make_poyo, poe => 'make_poe' }],\n  rw_lazy => { baz => 'make_baz' },\n);\n",
    );
    let decls = decl::declare(&root);
    let mut names: Vec<&str> = decls
        .facts_for("L")
        .expect("the package")
        .attributes
        .iter()
        .map(|attribute| attribute.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["baz", "hoge", "poe", "poyo"]);
    assert!(
        !decls.facts_for("L").expect("the package").constructor,
        "no `new => 1`, no `new`"
    );
}

#[test]
fn mk_accessors_installs_into_the_package_it_is_written_in() {
    // `Class::Accessor::Lite->mk_accessors` installs into `caller`, and a
    // `Class::Accessor` subclass calls the inherited method on itself. Both
    // mean the package the statement is in.
    let root = parse(
        "package K;\nuse Class::Accessor::Lite;\nClass::Accessor::Lite->mk_new_and_accessors(qw(foo));\npackage S;\nuse base 'Class::Accessor';\n__PACKAGE__->mk_ro_accessors(qw(bar));\n",
    );
    let decls = decl::declare(&root);
    let k = decls.facts_for("K").expect("K");
    assert_eq!(k.attributes.len(), 1);
    assert_eq!(k.attributes[0].name, "foo");
    assert!(k.constructor && k.open_constructor);
    let s = decls.facts_for("S").expect("S");
    assert_eq!(s.attributes.len(), 1);
    assert_eq!(s.attributes[0].access, Access::Ro);
    assert!(!s.constructor, "`new` is the parent's, not generated here");
}

#[test]
fn follow_best_practice_renames_what_comes_after_it() {
    let root = parse(
        "package S;\nuse base 'Class::Accessor';\n__PACKAGE__->mk_accessors(qw(before));\n__PACKAGE__->follow_best_practice;\n__PACKAGE__->mk_accessors(qw(after));\n__PACKAGE__->mk_ro_accessors(qw(readable));\n",
    );
    let decls = decl::declare(&root);
    let facts = decls.facts_for("S").expect("S");
    let methods = |name: &str| {
        facts
            .attributes
            .iter()
            .find(|one| one.name == name)
            .expect("the attribute")
            .methods
            .clone()
    };
    let methods = |name: &str| names(&methods(name));
    assert!(methods("before").is_empty(), "declared above the call");
    assert_eq!(methods("after"), ["get_after", "set_after"]);
    assert_eq!(methods("readable"), ["get_readable"]);
}

#[test]
fn a_type_library_declares_names() {
    let root = parse(
        "package MyApp::Types;\nuse Type::Library;\ndeclare 'PositiveInt', as Int;\nclass_type 'User', { class => 'MyApp::User' };\nrole_type 'Loggable';\nenum 'Color', [qw(red green blue)];\n",
    );
    let decls = decl::declare(&root);
    let types = &decls.facts_for("MyApp::Types").expect("the package").types;
    assert_eq!(types.len(), 4, "{types:?}");
    assert_eq!(types[0].name, "PositiveInt");
    assert_eq!(types[0].ty, Type::Int);
    assert_eq!(types[1].ty, Type::InstanceOf("MyApp::User".into()));
    assert_eq!(types[2].ty, Type::ConsumerOf("Loggable".into()));
    assert_eq!(
        types[3].ty,
        Type::Enum(vec!["red".into(), "green".into(), "blue".into()])
    );
}

#[test]
fn a_type_library_reads_type_and_unions_of_its_own_names() {
    let root = parse(
        "package MyApp::Types;\nuse Type::Utils;\ntype Foo => as Enum[qw(foo)];\ntype Bar => as Enum[qw(bar)];\ntype FooBar => as Foo | Bar;\n",
    );
    let decls = decl::declare(&root);
    let types = &decls.facts_for("MyApp::Types").expect("the package").types;
    assert_eq!(types.len(), 3, "{types:?}");
    assert_eq!(
        types[2].ty,
        Type::Union(vec![
            Type::InstanceOf("Foo".into()),
            Type::InstanceOf("Bar".into())
        ]),
        "the members stay names until the program links them"
    );
}

#[test]
fn a_package_knows_its_parents_and_roles() {
    let root = parse(
        "package Child;\nuse Moose;\nextends 'Parent';\nwith 'Loggable';\npackage Other;\nuse parent -norequire, 'Base';\n",
    );
    let decls = decl::declare(&root);
    let child = decls.facts_for("Child").expect("Child");
    assert_eq!(child.isa, vec!["Parent"]);
    assert_eq!(child.roles, vec!["Loggable"]);
    assert_eq!(decls.facts_for("Other").expect("Other").isa, vec!["Base"]);
}

#[test]
fn our_isa_is_a_parent_too() {
    let root = parse("package Child;\nour @ISA = ('Base', 'Other');\n");
    let decls = decl::declare(&root);
    assert_eq!(
        decls.facts_for("Child").expect("Child").isa,
        ["Base", "Other"]
    );
}

fn returns_of(source: &str) -> Returns {
    let root = parse(source);
    decl::declare(&root).subs.remove(0).returns
}

#[test]
fn returns_annotates_scalar_context() {
    let returns = returns_of("# Returns: ArrayRef[MyItem]\nsub items { }\n");
    assert_eq!(
        returns.scalar,
        Type::ArrayRef(Box::new(Type::InstanceOf("MyItem".into())))
    );
    assert_eq!(returns.list, ListShape::Unknown);
}

#[test]
fn returns_annotates_both_contexts_in_two_lines() {
    // One line per context, in either order, because the comma operator would
    // make `(A, B)` a `B` in scalar context and `(Row ...)` a count — two
    // rules that disagree, so a sub that wants a scalar type writes one.
    let returns = returns_of("# Returns: Maybe[Str]\n# Returns: (Str, Int)\nsub pair { }\n");
    assert!(returns.scalar.is_maybe());
    assert_eq!(returns.list, ListShape::Fixed(vec![Type::Str, Type::Int]));

    let swapped = returns_of("# Returns: (Str, Int)\n# Returns: Maybe[Str]\nsub pair { }\n");
    assert!(swapped.scalar.is_maybe());
    assert_eq!(swapped.list, ListShape::Fixed(vec![Type::Str, Type::Int]));
}

#[test]
fn a_repeated_slot_is_a_list_of_any_length() {
    let returns = returns_of("# Returns: (Str ...)\nsub rows { }\n");
    assert_eq!(returns.list, ListShape::Of(Type::Str));
    assert!(returns.scalar.is_unknown(), "and says nothing about scalar");
}

#[test]
fn one_slot_is_a_list_of_one_and_not_a_grouping() {
    // `()` is a list of none, so `(Str)` has to be a list of one; a grouping
    // parenthesis around a whole scalar type has no use that `Str | Undef`
    // does not serve.
    let returns = returns_of("# Returns: (Str)\nsub one { }\n");
    assert_eq!(returns.list, ListShape::Fixed(vec![Type::Str]));
    assert!(returns.scalar.is_unknown());
}

#[test]
fn a_parenthesis_inside_a_slot_still_groups() {
    let returns = returns_of("# Returns: (Str | Undef, Int)\nsub two { }\n");
    let ListShape::Fixed(slots) = returns.list else {
        panic!("two slots wanted");
    };
    assert_eq!(slots.len(), 2, "{slots:?}");
    assert!(slots[0].is_maybe());
}

#[test]
fn shapes_join_slot_wise_and_widen_when_the_length_does_not_agree() {
    let pair = || ListShape::Fixed(vec![Type::Str, Type::Int]);
    assert_eq!(pair().join(pair()), pair());
    assert_eq!(
        pair().join(ListShape::Fixed(vec![Type::Str])),
        ListShape::Of(Type::union(vec![Type::Str, Type::Int])),
    );
    // `return $x` beside `return;` is a list whose length is not known, so a
    // single target off it may be empty.
    assert_eq!(
        ListShape::Fixed(vec![Type::Str]).join(ListShape::Fixed(Vec::new())),
        ListShape::Of(Type::Str),
    );
    assert_eq!(pair().join(ListShape::Unknown), ListShape::Unknown);
}

#[test]
fn returns_nothing_is_written_with_empty_parentheses() {
    let returns = returns_of("# Returns: ()\nsub notify { }\n");
    assert_eq!(returns.list, ListShape::Nothing);
}

#[test]
fn a_returns_that_does_not_parse_is_reported() {
    let root = parse("# Returns: ArrayRef[Str\nsub items { }\n");
    let decls = decl::declare(&root);
    assert_eq!(decls.diagnostics.len(), 1, "{:?}", decls.diagnostics);
    assert_eq!(decls.diagnostics[0].code, crate::Code::BadAnnotation);
}

#[test]
fn a_comment_that_is_not_an_annotation_is_left_alone() {
    let root = parse("# just a note\nsub items { }\n");
    assert!(decl::declare(&root).diagnostics.is_empty());
}

#[test]
fn an_isa_that_is_code_is_not_an_annotation() {
    // The corpus computes constraints: `isa => $type`, `isa => __PACKAGE__ .
    // '::X'`. That is code the checker cannot read, not an annotation it read
    // wrongly, so it is `Unknown` and silent.
    let source = "use Moose;\nhas a => (is => 'ro', isa => $type);\n";
    let root = parse(source);
    assert!(decl::declare(&root).diagnostics.is_empty());
    assert_eq!(attributes(source)[0].ty, Type::Unknown);
}

#[test]
fn an_isa_with_a_bracket_missing_is_reported() {
    let root = parse("use Moose;\nhas a => (is => 'ro', isa => 'ArrayRef[Str');\n");
    let decls = decl::declare(&root);
    assert_eq!(decls.diagnostics.len(), 1, "{:?}", decls.diagnostics);
}

#[test]
fn args_reads_its_annotations() {
    let root = parse(
        "use Smart::Args;\nsub greet {\n    args my $self,\n         my $who => 'Str',\n         my $times => { isa => 'Int', default => 1 };\n}\n",
    );
    let decls = decl::declare(&root);
    let params = match &decls.subs[0].params {
        decl::Params::Named { params, .. } => params.clone(),
        other => panic!("not a named list: {other:?}"),
    };
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "$who");
    assert_eq!(params[0].ty, Type::Str);
    assert!(!params[0].optional);
    assert_eq!(params[1].ty, Type::Int);
    assert!(params[1].optional);
}
