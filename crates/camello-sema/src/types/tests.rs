//! What the type-expression parser promises, in both syntaxes.

use super::*;

fn read(text: &str) -> Type {
    parse(text).unwrap_or_else(|error| panic!("{text:?} did not parse: {}", error.message))
}

#[test]
fn the_two_syntaxes_are_one_grammar() {
    // `'ArrayRef[Str]'` and `ArrayRef[Str]` are the same characters, arriving
    // as a string and as Perl.
    assert_eq!(
        read("ArrayRef[HashRef[Str]]"),
        Type::ArrayRef(Box::new(Type::HashRef(Box::new(Type::Str))))
    );
}

#[test]
fn a_union_flattens_and_deduplicates() {
    assert_eq!(read("Str|Undef"), Type::union(vec![Type::Str, Type::Undef]));
    assert_eq!(read("Str | Str"), Type::Str);
    assert_eq!(read("Str | Int | Str"), read("Str|Int"));
}

#[test]
fn maybe_is_a_union_with_undef() {
    assert_eq!(read("Maybe[Str]"), read("Str|Undef"));
    assert!(read("Maybe[Str]").is_maybe());
    assert!(!read("Str").is_maybe());
    assert_eq!(read("Maybe[Str]").without_undef(), Type::Str);
}

#[test]
fn a_dict_keeps_its_slots_in_order() {
    let ty = read("Dict[name => Str, age => Optional[Int]]");
    let Type::Dict { slots, slurpy } = &ty else {
        panic!("not a Dict: {ty}");
    };
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].0, "name");
    assert_eq!(slots[0].1, Type::Str);
    assert!(slots[1].1.is_optional());
    assert!(slurpy.is_none(), "a Dict without a slurpy is restricted");
}

#[test]
fn a_slurpy_dict_accepts_any_key() {
    let ty = read("Dict[name => Str, slurpy HashRef[Str]]");
    let Type::Dict { slurpy, .. } = &ty else {
        panic!("not a Dict: {ty}");
    };
    assert_eq!(slurpy.as_deref(), Some(&Type::HashRef(Box::new(Type::Str))));
}

#[test]
fn a_quoted_argument_is_a_name() {
    assert_eq!(
        read("InstanceOf['Foo::Bar']"),
        Type::InstanceOf("Foo::Bar".into())
    );
    assert_eq!(read("ConsumerOf['Role']"), Type::ConsumerOf("Role".into()));
}

#[test]
fn an_unrecognised_bareword_is_a_class_name() {
    // The Moose reading (`docs/typecheck.md`, "Open questions"): a typo in a
    // type name becomes an `InstanceOf` of a class nothing declares, which is
    // resolvable to nothing and therefore silent.
    assert_eq!(read("Foo::Bar"), Type::InstanceOf("Foo::Bar".into()));
    assert_eq!(read("Srt"), Type::InstanceOf("Srt".into()));
}

#[test]
fn a_refinement_reads_as_its_base_type() {
    assert_eq!(read("PositiveInt"), Type::Int);
    assert_eq!(read("NonEmptyStr"), Type::Str);
    assert_eq!(read("StrictNum"), Type::Num);
}

#[test]
fn bool_is_not_int() {
    assert_ne!(read("Bool"), read("Int"));
}

#[test]
fn a_tuple_keeps_its_members() {
    assert_eq!(
        read("Tuple[Str, Int]"),
        Type::Tuple(vec![Type::Str, Type::Int])
    );
}

#[test]
fn a_parenthesised_union_is_that_union() {
    assert_eq!(read("ArrayRef[(Str|Int)]"), read("ArrayRef[Str|Int]"));
}

#[test]
fn what_is_not_a_type_expression_says_so() {
    assert!(parse("$MyType").is_err());
    assert!(parse("ArrayRef[Str").is_err());
    assert!(parse("").is_err());
    assert!(parse("Str Int").is_err());
}

#[test]
fn unknown_swallows_a_union() {
    assert_eq!(Type::union(vec![Type::Str, Type::Unknown]), Type::Unknown);
    assert_eq!(Type::maybe(Type::Unknown), Type::Unknown);
}

#[test]
fn a_type_prints_the_way_it_was_written() {
    assert_eq!(read("ArrayRef[Str]").to_string(), "ArrayRef[Str]");
    assert_eq!(read("Maybe[Str]").to_string(), "Str|Undef");
    assert_eq!(read("Dict[a => Int]").to_string(), "Dict[a => Int]");
}
