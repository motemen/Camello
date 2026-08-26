//! What the views promise: that a shape in the source reaches the checker as
//! the shape a reader would name.

use super::*;
use crate::parse::parse;

fn root(source: &str) -> Root {
    let parsed = parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture must parse: {:?}",
        parsed.diagnostics
    );
    Root::cast(parsed.syntax()).expect("the root is a ROOT")
}

fn first<N: AstNode>(source: &str) -> N {
    root(source)
        .syntax()
        .descendants()
        .find_map(N::cast)
        .unwrap_or_else(|| panic!("no {} in {source:?}", N::KIND_NAME))
}

fn first_call(source: &str) -> Call {
    root(source)
        .syntax()
        .descendants()
        .find_map(Call::cast)
        .expect("a call")
}

#[test]
fn every_node_kind_has_a_view_name() {
    // The generation is what makes the views exhaustive; this is what makes
    // `dev dump`'s second column exhaustive with it.
    for index in 0..crate::lang::NODE_COUNT {
        let kind = crate::lang::SyntaxKind::from(crate::lang::NodeKind::ROOT);
        let _ = kind;
        let node = crate::lang::SyntaxKind(crate::lang::TOKEN_COUNT + index)
            .as_node()
            .expect("in range");
        assert!(!node.view_name().is_empty(), "{node} has no view");
    }
}

#[test]
fn a_sub_offers_its_parts() {
    let sub: SubDef = first("sub greet ($self, $who = 'x') { return 1 }");
    assert_eq!(sub.name_text().as_deref(), Some("greet"));
    let params: Vec<_> = sub.signature().expect("a signature").params().collect();
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].variable().expect("a variable").display(), "$self");
    assert!(params[0].default().is_none());
    assert!(params[1].default().is_some());
    assert!(sub.body().is_some());
    assert!(!sub.is_forward_declaration());
}

#[test]
fn a_forward_declaration_has_no_body() {
    let sub: SubDef = first("sub later;\n");
    assert!(sub.is_forward_declaration());
}

#[test]
fn leading_comments_are_the_block_immediately_above() {
    let source = "\
# not this one

# Returns: Int
# and this
sub f { 1 }
";
    let sub: SubDef = first(source);
    let texts: Vec<_> = sub
        .leading_comments()
        .iter()
        .map(|token| token.text().to_string())
        .collect();
    assert_eq!(texts, vec!["# Returns: Int", "# and this"]);
}

#[test]
fn a_trailing_comment_belongs_to_its_own_line() {
    // The trivia model gives it to the code it follows, and so does this: a
    // `Returns:` written after a statement is not an annotation on the sub.
    let source = "my $x = 1;  # Returns: Int\nsub f { 1 }\n";
    let sub: SubDef = first(source);
    assert!(sub.leading_comments().is_empty());
}

#[test]
fn fat_comma_pairs_are_exposed_as_pairs() {
    let call = first_call("has name => (is => 'ro', isa => 'Str');");
    assert_eq!(call.callee_name().as_deref(), Some("has"));
    // `has name => (...)` is one pair whose value is the option list, and two
    // arguments flat — which is what the document says a `Call` offers.
    let pairs = call.pairs();
    assert_eq!(pairs.len(), 1, "{pairs:?}");
    assert_eq!(pairs[0].key(), Some("name"));
    assert_eq!(call.args().len(), 2);
    let options = Args::pairs(pairs[0].node());
    let keys: Vec<_> = options.iter().map(Arg::key).collect();
    assert_eq!(keys, vec![Some("is"), Some("isa")]);
}

#[test]
fn a_bareword_is_a_call_with_no_arguments() {
    let call = first_call("foo;");
    assert_eq!(call.callee_name().as_deref(), Some("foo"));
    assert!(call.args().is_empty());
}

#[test]
fn a_filehandle_is_not_an_argument() {
    let call = first_call("print STDERR \"x\";");
    assert!(call.filehandle().is_some());
    assert_eq!(call.args().len(), 1);
}

#[test]
fn a_method_call_names_its_invocant_and_method() {
    let call: MethodCall = first("$obj->greet(1);");
    assert_eq!(call.method_name().as_deref(), Some("greet"));
    assert!(!call.is_dynamic());
    assert_eq!(call.args().len(), 1);
}

#[test]
fn a_dynamic_method_call_has_no_name() {
    let call: MethodCall = first("$obj->$name();");
    assert!(call.is_dynamic());
}

#[test]
fn a_subscript_chain_reads_left_to_right() {
    let node = root("$self->{a}[0]{b};")
        .syntax()
        .descendants()
        .find_map(SubscriptChain::cast)
        .expect("a chain");
    assert_eq!(joined_text(node.base()), "$self");
    let steps = node.steps();
    assert_eq!(steps.len(), 3);
    assert!(matches!(&steps[0], Step::Hash { key: Some(k), .. } if k == "a"));
    assert!(matches!(&steps[1], Step::Array { index: Some(0), .. }));
    assert!(matches!(&steps[2], Step::Hash { key: Some(k), .. } if k == "b"));
}

#[test]
fn a_postfix_deref_is_a_step() {
    let node = root("$x->@*;")
        .syntax()
        .descendants()
        .find_map(SubscriptChain::cast)
        .expect("a chain");
    assert!(matches!(
        node.steps()[0],
        Step::Deref {
            sigil: Sigil::Array,
            ..
        }
    ));
}

#[test]
fn a_declaration_names_every_target() {
    let decl: VarDecl = first("my ($self, %args) = @_;");
    assert_eq!(decl.keyword(), Some(DeclKeyword::My));
    let names: Vec<_> = decl.targets().iter().map(Variable::display).collect();
    assert_eq!(names, vec!["$self", "%args"]);
}

#[test]
fn local_is_a_keyword_of_its_own() {
    let decl: VarDecl = first("local $x = 1;");
    assert_eq!(decl.keyword(), Some(DeclKeyword::Local));
}

#[test]
fn a_string_literal_says_what_it_holds() {
    let literal: Literal = first("my $x = 'ArrayRef[Str]';");
    assert_eq!(literal.as_string().as_deref(), Some("ArrayRef[Str]"));
}

#[test]
fn an_interpolating_string_is_not_a_literal_string() {
    let literal: Literal = first("my $x = \"hi $who\";");
    assert_eq!(literal.as_string(), None);
}

#[test]
fn a_use_statement_offers_its_arguments() {
    let statement: UseStmt = first("use Class::Accessor::Typed (rw => { name => 'Str' });");
    assert_eq!(
        statement.module().as_deref(),
        Some("Class::Accessor::Typed")
    );
    let arguments = statement.arguments().expect("an argument list");
    let pairs = Args::pairs(&arguments);
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].key(), Some("rw"));
}

#[test]
fn an_anonymous_hash_reads_as_pairs() {
    let hash: AnonHash = first("my $x = { isa => 'Int', default => 1 };");
    let pairs = hash.pairs();
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].key(), Some("isa"));
    assert_eq!(pairs[1].key(), Some("default"));
}

#[test]
fn a_word_list_is_its_words() {
    let words: QwExpr = first("my @a = qw(a b c);");
    assert_eq!(words.words(), vec!["a", "b", "c"]);
}

#[test]
fn an_assignment_has_a_target_and_a_value() {
    let assign: Assign = first("$x //= 1;");
    assert_eq!(
        assign.operator(),
        Some(crate::lang::TokenKind::DEFINED_OR_EQ)
    );
    assert!(!assign.is_plain());
    assert!(assign.target().is_some());
    assert!(assign.value().is_some());
}

#[test]
fn a_package_names_itself() {
    let package: PackageStmt = first("package Foo::Bar;");
    assert_eq!(package.name().as_deref(), Some("Foo::Bar"));
    assert!(package.block().is_none());
}
