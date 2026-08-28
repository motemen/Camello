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

// ===== the site table (`docs/return-inference.md`, "Sites") =====

/// What the return walk read off each sub's body, as `signature_of` renders
/// it.
///
/// The fixtures say what a *program* gets told, which is one step further on:
/// a type only shows up there when something reports against it, and half the
/// rows of the site table are about a type nothing reports against. This is
/// the table itself.
fn inferred(source: &str) -> Vec<String> {
    let parsed = camello_syntax::parse::parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the source does not parse: {:?}",
        parsed.diagnostics
    );
    let decls = crate::decl::declare(&parsed.syntax());
    decls.subs.iter().map(crate::decl::signature_of).collect()
}

#[test]
fn a_site_is_read_the_way_the_table_says() {
    let found = inferred(
        "package P;\n\
         sub literal { return 42 }\n\
         sub tail { 'text' }\n\
         sub nothing { return }\n\
         sub explicit_undef { return undef }\n\
         sub a_list { return (1, 2) }\n\
         sub an_array { my @rows = (1); return @rows }\n\
         sub a_hash { my %h; return %h }\n\
         sub wants { return wantarray ? (1, 2) : 'one' }\n\
         sub only_dies { die 'no' }\n\
         sub joined { my $ok = 1; if ($ok) { return 1 } else { return 'x' } }\n\
         sub no_else { my $ok = 1; if ($ok) { 1 } }\n\
         sub empty { }\n\
         sub gone { goto &literal }\n",
    );
    assert_eq!(
        found,
        vec![
            "P::literal -> Int (inferred)",
            "P::tail -> Str (inferred)",
            "P::nothing -> Undef (inferred)",
            "P::explicit_undef -> Undef (inferred)",
            // A list, whose scalar reading is not a type the program has.
            "P::a_list",
            "P::an_array",
            "P::a_hash",
            // The scalar branch, whatever the list branch holds.
            "P::wants -> Str (inferred)",
            // `die` is bottom: it contributes nothing, and nothing joined is
            // nothing known.
            "P::only_dies",
            "P::joined -> Int|Str (inferred)",
            // The value of a false `if` is its condition's.
            "P::no_else",
            "P::empty",
            "P::gone",
        ]
    );
}

#[test]
fn a_sub_that_hands_back_its_invocant_is_left_to_the_call_site() {
    let found = inferred(
        "package P;\n\
         sub chained { my $self = shift; $self->{x} = 1; return $self }\n\
         sub tail_chained { my $self = shift; $self }\n\
         sub implicit { $_[0] }\n\
         sub perhaps { my ($self, $ok) = @_; return $ok ? $self : undef }\n\
         sub classy { my $class = shift; return bless {}, $class }\n\
         sub named { my $self = shift; return $self->{name} }\n",
    );
    assert_eq!(
        found,
        vec![
            "P::chained($self : Any) -> InstanceOf['P'] (inferred)",
            "P::tail_chained($self : Any) -> InstanceOf['P'] (inferred)",
            "P::implicit -> InstanceOf['P'] (inferred)",
            "P::perhaps($self : Any, $ok : Any) -> InstanceOf['P']|Undef (inferred)",
            "P::classy($class : Any) -> InstanceOf['P'] (inferred)",
            // An object's own hash slots are not typed, so this is the
            // accessor the walk cannot read.
            "P::named($self : Any)",
        ]
    );
}

#[test]
fn one_untyped_site_makes_the_whole_sub_unknown() {
    // The rule that keeps the checker quiet, and the reason the feature can
    // ship: a partial join — `Int` from the site that was typed, ignoring the
    // one that was not — is a type the program does not have, and it would be
    // reported at every call site.
    let found = inferred(
        "package P;\n\
         sub partial { my ($self, $ok) = @_; return 1 if $ok; return $self->outside }\n\
         sub whole { my ($self, $ok) = @_; return 1 if $ok; return 2 }\n",
    );
    assert_eq!(
        found,
        vec![
            "P::partial($self : Any, $ok : Any)",
            "P::whole($self : Any, $ok : Any) -> Int (inferred)",
        ]
    );
}

#[test]
fn a_sub_resolves_through_another_in_the_same_file() {
    // Tier 1's fixpoint: `outer` cannot be read until `inner` has been, and
    // the rounds are the depth of the file's own call chains.
    let found = inferred(
        "package P;\n\
         sub outer { return inner() }\n\
         sub inner { return deepest() }\n\
         sub deepest { return 'text' }\n",
    );
    assert_eq!(
        found,
        vec![
            "P::outer -> Str (inferred)",
            "P::inner -> Str (inferred)",
            "P::deepest -> Str (inferred)",
        ]
    );
}

#[test]
fn a_recursive_sub_is_cut_to_unknown() {
    // Without a call graph having to be built to find it: every round asks
    // the same question and gets the same `Unknown` back.
    let found = inferred(
        "package P;\n\
         sub loops { my ($n) = @_; return $n ? loops($n - 1) : 0 }\n\
         sub ping { return pong() }\n\
         sub pong { return ping() }\n",
    );
    assert_eq!(found, vec!["P::loops($n : Any)", "P::ping", "P::pong"]);
}
