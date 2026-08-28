//! The two type relations, and what holds between them.
//!
//! [`compatible`] is what the checker reports against — "these two could be
//! the same value" — and [`is_assignable`] is set inclusion. Neither is
//! obvious enough to leave to the fixtures: a fixture says what one program
//! gets told, and these say what the relation *is*.

use super::{compatible, is_assignable};
use crate::program::Program;
use crate::types::{parse, Type};

fn ty(text: &str) -> Type {
    parse(text).unwrap_or_else(|error| panic!("{text:?} did not parse: {}", error.message))
}

/// Every type in the language, as far as the relations are concerned.
fn every_type() -> Vec<Type> {
    [
        "Any",
        "Defined",
        "Value",
        "Str",
        "Num",
        "Int",
        "Bool",
        "ClassName",
        "RoleName",
        "Enum['a','b']",
        "Ref",
        "ScalarRef[Str]",
        "ArrayRef[Int]",
        "Tuple[Int, Str]",
        "HashRef[Str]",
        "Dict[a => Str]",
        "Map[Str, Int]",
        "CodeRef",
        "RegexpRef",
        "GlobRef",
        "FileHandle",
        "Object",
        "InstanceOf['Foo']",
        "Undef",
        "Str|Undef",
        "Int|ArrayRef[Int]",
    ]
    .into_iter()
    .map(ty)
    .collect()
}

#[test]
fn assignable_implies_compatible() {
    // The two are a pair: if every value of one is a value of the other, they
    // certainly have a value in common. A relation that said otherwise would
    // be reporting a contradiction against something it also calls a subtype.
    let program = Program::default();
    for value in every_type() {
        for slot in every_type() {
            if is_assignable(&value, &slot, &program) {
                assert!(
                    compatible(&value, &slot, &program),
                    "{value} is assignable to {slot} and yet not compatible with it"
                );
            }
        }
    }
}

#[test]
fn both_relations_are_reflexive() {
    let program = Program::default();
    for one in every_type() {
        assert!(is_assignable(&one, &one, &program), "{one} is not itself");
        assert!(
            compatible(&one, &one, &program),
            "{one} does not meet itself"
        );
    }
}

#[test]
fn compatible_is_directed() {
    // `compatible(value, slot)` asks whether a value of one could go in a
    // slot of the other, and that is not a symmetric question. The
    // stringification chain is where it shows: a number goes where a string
    // was asked for, and a `Str` value does not go into an `Int` slot,
    // because a literal that looked like a number is already an `Int`
    // (`docs/types.md`, TYPE-5b) and what is left is a string that is not one.
    let program = Program::default();
    for (narrow, broad) in [
        ("Int", "Str"),
        ("Int", "Num"),
        ("Num", "Str"),
        ("HashRef[Int]", "HashRef[Str]"),
    ] {
        let (narrow, broad) = (ty(narrow), ty(broad));
        assert!(
            compatible(&narrow, &broad, &program),
            "{narrow} should fit a {broad} slot"
        );
        assert!(
            !compatible(&broad, &narrow, &program),
            "{broad} should not fit a {narrow} slot"
        );
    }
}

#[test]
fn a_kind_meets_its_family_from_either_side() {
    // Whether two *kinds* meet is a question about sets, and sets meet both
    // ways: a value known only as `Ref` could be an `ArrayRef` as much as an
    // `ArrayRef` is a `Ref`. Nothing is ruled out in either direction, which
    // is all this relation ever says.
    let program = Program::default();
    for (head, member) in [
        ("Ref", "ArrayRef[Int]"),
        ("Ref", "Dict[a => Str]"),
        ("Ref", "CodeRef"),
        ("Defined", "Str"),
        ("Defined", "ArrayRef[Int]"),
        ("Value", "Str"),
        ("Object", "InstanceOf['Foo']"),
        ("GlobRef", "FileHandle"),
        // A class name is a string and a string may well name a class; an
        // enum's values are strings and this checker does not follow values
        // (TYPE-5c). Both pairs meet from either side on purpose.
        ("Str", "ClassName"),
        ("Str", "Enum['a','b']"),
    ] {
        let (head, member) = (ty(head), ty(member));
        assert!(
            compatible(&member, &head, &program),
            "{member} should fit a {head} slot"
        );
        assert!(
            compatible(&head, &member, &program),
            "a {head} could be a {member}, so nothing is ruled out"
        );
    }
}

#[test]
fn a_family_head_holds_its_family() {
    // What both relations were missing until the family heads were wired up:
    // every reference is a `Ref`, and a `Dict` is one along with the rest.
    let program = Program::default();
    for narrow in [
        "ScalarRef[Str]",
        "ArrayRef[Int]",
        "Tuple[Int, Str]",
        "HashRef[Str]",
        "Dict[a => Str]",
        "Map[Str, Int]",
        "CodeRef",
        "RegexpRef",
        "GlobRef",
        "Object",
        "InstanceOf['Foo']",
    ] {
        let narrow = ty(narrow);
        assert!(
            is_assignable(&narrow, &Type::Ref, &program),
            "{narrow} is not a Ref"
        );
        assert!(
            compatible(&narrow, &Type::Ref, &program),
            "{narrow} does not meet Ref"
        );
    }
    for narrow in ["Str", "Num", "Int", "Enum['a']", "ClassName", "RoleName"] {
        let narrow = ty(narrow);
        assert!(
            is_assignable(&narrow, &Type::Value, &program),
            "{narrow} is not a Value"
        );
    }
    assert!(is_assignable(&ty("Int"), &ty("Num"), &program));
    assert!(is_assignable(&ty("Num"), &ty("Str"), &program));
    assert!(is_assignable(
        &ty("InstanceOf['Foo']"),
        &Type::Object,
        &program
    ));
    assert!(is_assignable(&ty("FileHandle"), &Type::GlobRef, &program));
}

#[test]
fn the_two_relations_part_company_where_they_should() {
    let program = Program::default();
    // A union may be the slot's type and may not: they meet, and it is not a
    // subtype. This is the case the checker deliberately stays quiet about.
    let both = ty("Int|ArrayRef[Int]");
    assert!(compatible(&both, &Type::Int, &program));
    assert!(!is_assignable(&both, &Type::Int, &program));

    // `Maybe[Str]` in a `Str` slot is the same shape.
    assert!(compatible(&ty("Str|Undef"), &Type::Str, &program));
    assert!(!is_assignable(&ty("Str|Undef"), &Type::Str, &program));

    // `Bool` holds an `undef`, so three quarters of it is a `Value` and it is
    // not one.
    assert!(compatible(&Type::Bool, &Type::Value, &program));
    assert!(!is_assignable(&Type::Bool, &Type::Value, &program));
    assert!(!is_assignable(&Type::Bool, &Type::Defined, &program));

    // A family head could be any of its family, which is a meeting and not an
    // inclusion.
    assert!(compatible(&Type::Ref, &ty("ArrayRef[Int]"), &program));
    assert!(!is_assignable(&Type::Ref, &ty("ArrayRef[Int]"), &program));
}

#[test]
fn what_neither_relation_allows() {
    let program = Program::default();
    for (value, slot) in [
        ("Str", "Ref"),
        ("ArrayRef[Int]", "Value"),
        ("Undef", "Defined"),
        ("ArrayRef[Int]", "HashRef[Str]"),
        ("CodeRef", "ArrayRef[Int]"),
    ] {
        let (value, slot) = (ty(value), ty(slot));
        assert!(
            !compatible(&value, &slot, &program),
            "{value} should not meet {slot}"
        );
        assert!(
            !is_assignable(&value, &slot, &program),
            "{value} should not be a {slot}"
        );
    }
}
