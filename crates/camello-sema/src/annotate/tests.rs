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
fn coerce_widens_the_slot() {
    // A coerced slot accepts `Any`, because the coercion is a function the
    // checker cannot see (`docs/typecheck.md`, non-goals).
    let found = attributes("use Moose;\nhas at => (is => 'ro', isa => 'Int', coerce => 1);\n");
    assert_eq!(found[0].ty, Type::Unknown);
}

#[test]
fn named_accessors_are_methods_of_their_own() {
    let found = attributes(
        "use Moose;\nhas name => (is => 'ro', writer => 'set_name', predicate => 'has_name');\n",
    );
    assert!(found[0].methods.contains(&"set_name".to_string()));
    assert!(found[0].methods.contains(&"has_name".to_string()));
}

#[test]
fn a_regexp_in_handles_makes_the_delegation_opaque() {
    let found = attributes("use Moose;\nhas c => (is => 'ro', handles => qr/^x/);\n");
    assert!(found[0].opaque_delegation);
    let listed = attributes("use Moose;\nhas c => (is => 'ro', handles => [qw(a b)]);\n");
    assert!(!listed[0].opaque_delegation);
    assert_eq!(listed[0].methods, vec!["a", "b"]);
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
fn returns_annotates_both_contexts() {
    let returns = returns_of("# Returns: Maybe[Str] | list: (Str, Int)\nsub pair { }\n");
    assert!(returns.scalar.is_maybe());
    assert_eq!(returns.list, ListShape::Fixed(vec![Type::Str, Type::Int]));
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
