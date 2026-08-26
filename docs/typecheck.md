# Design: `camello typecheck` and `camello lint`

Status: implemented, 2026-08-26. This document decided the shape; the code is
now the authority, in the same way `docs/contracts.md` treats its ADRs. Where
the two disagree, the code is right and "Decisions made during implementation"
at the end of this file says why.

This is the *design*. What the checker concludes about a program, and what it
deliberately leaves unknown, is specified for its users in
[types.md](types.md) — the checker's `formatting.md`.

## What is being built

A static checker for Perl that runs over the CST camello already produces,
and that treats the type annotations Perl code actually carries — `has ...
isa => 'Str'`, `args my $x => 'Int'`, `Class::Accessor::Typed`, and a
`Returns:` comment introduced here — as first-class input rather than as
strings it happens to see.

Perl has no static types, so almost every answer is a derivation from
evidence: a sigil, a literal, a constructor call, an annotation on the callee.
The checker is therefore closer to a pyright in `basic` mode than to a
compiler's type checker, and weaker still: where pyright has a nominal type
system underneath, this has a lattice of *shapes* and a set of annotations
that a real program may or may not honour at run time.

Two subcommands, one analysis:

- `camello lint lib t` — diagnostics that need no type lattice: undeclared or
  unused lexicals, shadowing, arity against a signature or an `args` list,
  `return` shapes that disagree across paths. Fast; meant to run in the same
  place `perlcritic` does.
- `camello typecheck lib t` — everything `lint` reports, plus what the type
  lattice adds: a `Str` passed where an `ArrayRef[Int]` was declared, a method
  called on a class that does not declare it, a hash key read off a `Dict`
  that has no such key, an `InstanceOf['Foo']` where a `Maybe[...]` was
  returned and never checked.

Both exit `1` if anything at or above `--error-on` (default: `error`) was
reported, print one diagnostic per line in `path:line:col: severity: message`
form, and take `--format json` for tooling. CI is the first consumer; an
editor is the second and the design keeps it possible (the "Incremental
reanalysis" section) without building it.

### Non-goals

- Soundness. The checker is silent when it does not know. A program with no
  annotations and no recognisable constructors gets no type diagnostics at
  all, and that is correct behaviour, not a gap. `lint` diagnostics about
  scope are the exception: `my` is a declaration and `use strict` makes an
  undeclared name an error, so those are sound within a file.
- Running Perl. Neither subcommand executes the program, loads a module, or
  asks `perl -c`. `dev perl-deparse` is the only place camello runs perl, and
  it stays that way. Consequences: `BEGIN` blocks, `eval "..."`, `AUTOLOAD`,
  `local *glob = ...`, string-named method calls `$obj->$name`, and `no
  strict 'refs'` symbol-table tricks are all opaque. A sub reached only
  through them is `Unknown`, which never produces a diagnostic.
- Checking dependencies. Files under the project roots are analysed in full;
  a module found through `@INC`-style resolution outside them contributes
  only its declarations (the "Dependencies" section). No diagnostic is ever
  reported against a file outside the roots.
- Type coercion. `coerce => 1` on an attribute or an `args` rule makes the
  declared type an upper bound on the *coerced* value, which the checker
  cannot see. Such a slot is treated as accepting `Any` and yielding the
  declared type.

## Where it lives

```text
src/lang, src/lex, src/parse    (unchanged)  ->  crate camello-syntax
src/fmt                         (unchanged)  ->  crate camello-fmt
src/ast        typed views over the CST      ->  crate camello-syntax
src/sema       symbols, scopes, types, flow  ->  crate camello-sema
src/cli        format | lint | typecheck     ->  crate camello
```

`sema` depends on `syntax` and on nothing in `fmt`; the lossless-CST and
trivia machinery that the formatter is built around is irrelevant to it, and
a build of the checker should not carry the Doc IR. The workspace split is a
mechanical change and is the first step, so that the dependency direction is
enforced by Cargo rather than by review.

`camello-syntax` gains one module, `ast`. Everything else new is `sema`.

### Data flow

```text
files in roots
  -> parse (camello-syntax)          one CST per file, cached by content hash
  -> ast views
  -> declaration pass  (per file)    packages, subs, attributes, annotations,
                                     imports, ISA — no bodies read
  -> program graph                   packages across files; dependencies
                                     contribute declarations only
  -> body pass (per sub)             scopes, flow, type environments
  -> diagnostics                     sorted, deduplicated, filtered by severity
```

The declaration pass is what makes the program graph, and it deliberately
reads no sub body: a body can only *use* a declaration, never make one that
another file could see (the ways it can — `*Foo::bar = sub {...}` — are in
the non-goals). This is the property that makes dependencies cheap and
incremental reanalysis possible: editing a body invalidates one sub's body
pass and nothing else.

## The AST layer

`src/parse` produces a rowan CST and `src/lang` offers `NodeExt` and nothing
more. The formatter walks kinds directly because its questions are about
tokens and trivia. The checker's questions are about structure, and asking
them by kind-matching over child iterators in every pass is how a checker
becomes unmaintainable.

`ast` is the standard rowan pattern: one newtype per `NodeKind` that carries
meaning, `AstNode::cast(SyntaxNode) -> Option<Self>`, and accessors that
return other views or tokens.

```rust
pub struct SubDef(SyntaxNode);
impl SubDef {
    pub fn name(&self) -> Option<SubName>;
    pub fn signature(&self) -> Option<SubSignature>;
    pub fn body(&self) -> Option<Block>;
    pub fn attrs(&self) -> impl Iterator<Item = Attr>;
    pub fn leading_comments(&self) -> impl Iterator<Item = SyntaxToken>;
}
```

`leading_comments` is the one accessor the formatter would not have wanted
and the checker cannot do without: it is where `Returns:` lives.

Views are generated from the `nodes` section of `define_language!` for the
`cast`/`syntax` boilerplate, and hand-written for accessors, the way
`src/lang/predicates.rs` is hand-written beside the generated enums. The
generation must not change the CST, `SyntaxKind` numbering, or any formatter
output — `cargo test -q` over the formatter fixtures is the check.

Expression views that matter most, because the checker spends its time in
them:

- `Call` over `CALL_EXPR` / `LIST_CALL_EXPR` / `CODE_CALL_EXPR` — callee name,
  arguments as a flat list with fat-comma pairs exposed as `(key, value)`
  when the key is a bareword or string. `has name => (is => 'ro')` is a
  `Call` whose callee is `has` and whose args are `[name, Paren(is => 'ro')]`.
- `MethodCall` over `METHOD_CALL_EXPR` — invocant, method name (a bareword,
  or `Dynamic` when it is `$var`), arguments.
- `Subscript` over `HASH_SUBSCRIPT_EXPR` / `ARRAY_SUBSCRIPT_EXPR` /
  `POSTFIX_DEREF_EXPR` / `SLICE_EXPR` — the chain `$x->{a}[0]{b}` as a base
  plus a list of steps, each `Hash(key)` / `Array(index)` / `Deref(sigil)`.
- `Assign` over `ASSIGN_EXPR` — targets as a list, so `my ($self, %args) =
  @_` and `my $x = ...` share one path.
- `Literal`, `AnonHash`, `AnonArray`, `AnonSub`, `Interpolated`.

## The program model

### Files, packages, symbols

A **file** is one CST. A **package** is a name (`Foo::Bar`) and the set of
files that have a `package Foo::Bar` statement — it is common for one file to
hold several packages and rare, but legal, for one package to span files, so
the model is many-to-many and a package's declarations are the union.

A **symbol** belongs to a package and is one of:

- `Sub { name, params: Params, returns: Type, source: Annotated | Inferred |
  Unknown }`
- `Attribute { name, type: Type, access: Ro | Rw | Wo, required, default,
  source }` — produced by `has`, `Class::Accessor::Typed`, and the
  `__PACKAGE__->mk_accessors` family
- `Constant { name, type }` from `use constant`
- `Import { name, from: package }` from `use Foo qw(bar)` when `Foo` is
  analysable and exports `bar`

Package-level facts that are not symbols: `isa: Vec<Package>` (from
`use parent`, `use base`, `extends`, `our @ISA = (...)`), `roles` (`with`,
`does`), and the object framework in use (Moose, Moo, Mouse,
Class::Accessor::Typed, plain `bless`, or none), because the framework
decides whether `new` exists and what it accepts.

Method resolution is C3 over `isa` with roles flattened in, and falls back to
`Unknown` — not an error — when any ancestor is itself `Unknown`. A class
with an unknown ancestor might have any method, and reporting "no such
method" there would be the worst kind of false positive.

### Scopes

Lexical scopes are built per file from `my` / `our` / `state` / `local`,
signature parameters, `foreach my $x`, `catch ($e)`, and the implicit `$_`
/ `@_` / `%ENV` / `$0` set. `our $x` binds a lexical alias to a package
variable; `local` does not declare. String interpolation is the notable
extra: `"hi $who"` and `"$h->{k}[0]"` contain variable uses, so the
`INTERPOLATED_STRING` token (one token today, `src/lex/atomic.rs`) is
re-scanned by a small interpolation scanner in `sema` that finds `$name`,
`@name`, `${...}`, `@{[ ... ]}` and subscripts. It produces uses, not a
CST; the CST is not changed. Getting this wrong means either a phantom
"unused variable" or a missed "undeclared variable", both of which are
`lint` bread and butter, so the scanner is tested against perl's own
interpolation rules (`perldoc perlop`, "Gory details of parsing quoted
constructs") as a fixture set.

`use strict` is taken as on by default (a project with `no strict` in it
almost certainly wants it reported); `no strict 'vars'` in scope suppresses
the undeclared-variable diagnostic for the scope.

## Types

### The lattice

The type language is the intersection of Moose's string constraints and
Types::Standard, because that is what the annotations are written in, and it
is deliberately not extended: a type the annotations cannot express is a type
the checker cannot be told to expect.

```text
Any                           top; everything is Any
  Unknown                     top too, but "not analysed" rather than "anything":
                              an Unknown is never reported against
  Defined
    Value
      Str
        Num
          Int
        Bool                  0 1 "" undef — kept nominal; Bool is not Int here
        ClassName             a Str naming a package known to the program
        Enum[...]
    Ref
      ScalarRef[T]
      ArrayRef[T]     Tuple[T, U, ...]
      HashRef[T]      Dict[k => T, ...; slurpy]   Map[K, V]
      CodeRef
      RegexpRef
      Object          InstanceOf['Foo']   ConsumerOf['Role']   HasMethods[...]
  Undef
Maybe[T]              = T | Undef
Optional[T]           only inside Dict / a parameter list; means the slot may be absent
T | U                 union
```

Two facts about Perl that the lattice has to carry and that a Python one
would not:

- **Context.** An expression has a type in scalar context and a *list
  shape* in list context, and they are different things. `@a` is
  `ArrayRef[T]`-like in the sense of "a list of T" and `Int` in scalar
  context. The checker tracks `Ctx::Scalar | Ctx::List` down the expression
  tree from the assignment or call that imposes it, and `return` sites are
  collected under both. A sub's `returns` is therefore a pair
  `(scalar: Type, list: ListShape)`; `Returns:` annotates one or both.
- **Stringification.** `Int <: Num <: Str` is the subtyping that Perl's
  values actually have: `"3"` is an `Int` when `looks_like_number`, and a
  `Str` is what an `Int` becomes under `.`. The lattice takes this as given
  and never reports a `Num` used as a `Str`. It *does* report a `Str`
  literal that is not numeric passed to an `Int` slot, because the literal is
  right there.

`Unknown` propagates: any operation on an `Unknown` yields `Unknown`, and a
diagnostic is only ever raised when both sides of a comparison are known.
This is the single rule that keeps the checker quiet on unannotated code.

### The type-expression parser

Annotations arrive in two syntaxes and one grammar:

- as a string: `'ArrayRef[HashRef[Str]]'`, `'Maybe[Foo::Bar]'`,
  `'Str|Undef'` — the Moose/Mouse string grammar, parsed by `sema` from the
  literal's text;
- as a bareword expression: `ArrayRef[HashRef[Str]]`, `Maybe[InstanceOf['Foo']]`,
  `Dict[name => Str, age => Optional[Int]]`, `Str | Undef` — which is Perl
  code and already a CST subtree (`LIST_CALL_EXPR` of `ArrayRef` applied to
  an `ANON_ARRAY`, a `BINARY_EXPR` of `|`). The parser walks the subtree
  instead of text.

Both produce the same `Type`. A bareword that is not a known type
constructor and not imported from a `Type::Library` is read as a class name
(`Foo::Bar` bareword → `InstanceOf['Foo::Bar']`), matching what Moose does
with an unknown string constraint. A bareword the checker recognises as
neither is `Unknown`, silently.

The known constructor set is Types::Standard's, plus `Types::Common::Numeric`
and `Types::Common::String` (`PositiveInt`, `NonEmptyStr` — read as their
base type). A project's own `Type::Library` is handled in the
"Custom type libraries" section.

### Annotation sources

Each source is a recogniser over the AST: it matches a declaration shape and
yields symbols or parameter lists. None of them is special-cased in the
parser; `has` is a `LIST_CALL_EXPR` today (`camello dev dump` shows it) and
stays one. Recognition is by callee name *and* by an import that could have
provided it — `has` from `use Moose`/`Moo`/`Mouse`/`Moose::Role`, `args`
from `use Smart::Args` or `use Smart::Args::TypeTiny` — so a project's own
`sub has` is not mistaken for Moose's.

**Moose / Moo / Mouse `has`.**

```perl
has name  => (is => 'ro', isa => 'Str', required => 1);
has items => (is => 'rw', isa => ArrayRef[Item], default => sub { [] });
has [qw(a b)] => (is => 'ro', isa => 'Int');
has '+name' => (default => 'x');            # override; type from the parent
```

Yields `Attribute` symbols. `isa` gives the type (`Unknown` when absent or
when the value is not a literal or a type expression); `does => 'Role'` gives
`ConsumerOf['Role']`; `required` and `default`/`builder`/`lazy` decide
whether `new` may omit it. `is => 'ro'` with `writer => 'set_name'` and
`reader`/`accessor`/`predicate`/`clearer` all produce their named methods.
`handles` is read for `ArrayRef[Str]` and `HashRef[Str]` forms; a regexp or
role name in `handles` makes the delegated set `Unknown`.

`new` for a Moose-family class accepts a `Dict` of the attributes with
`required` ones mandatory and yields `InstanceOf[the class]`. `BUILDARGS` in
the class, or any ancestor that is `Unknown`, turns the argument check off.

**Smart::Args, Smart::Args::TypeTiny.**

```perl
sub greet {
    args my $self,
         my $who   => 'Str',
         my $times => { isa => 'Int', default => 1 },
         my $loud  => { isa => Bool, optional => 1 };
    ...
}
sub at { args_pos my $self, my $i => 'Int'; ... }
```

The first statement of a sub body being an `args` or `args_pos` call is what
makes this a parameter list for the enclosing sub; an `args` anywhere else is
checked as a call but declares nothing. Each `my $var [=> rule]` item becomes
a parameter: named for `args`, positional for `args_pos`; the rule is a type
string, a type expression, or a hashref with `isa` / `optional` / `default`.
A first item named `$self` or `$class` is the invocant (as the module itself
decides) and marks the sub as a method; `$class` types it `ClassName`, `$self`
`InstanceOf[the package]`. A bare `my $x` with no rule is `Any`, not
`Unknown` — the module treats it as mandatory, and so does the checker for
arity.

`args` also *declares* the lexicals, so the scope pass reads it too: without
this every `args` sub would report every parameter undeclared.

Call sites of an `args` sub are checked as `f(key => value, ...)` against a
`Dict`, with the same "unknown key" and "missing required" diagnostics as a
Moose `new`. `args` in Smart::Args accepts a single hashref as well as pairs;
both shapes are accepted.

**Data::Validator** — deferred. The validator is a value (`my $rule =
Data::Validator->new(...)->with('Method')`) and the parameter list is a call
on it, so recognising it means following a lexical across statements and
interpreting `->with` modes; that is a flow analysis dressed as an annotation
and is not worth doing before the flow pass exists. Until then a sub that
validates through it has `Unknown` parameters and is silent. The recogniser
belongs after milestone 5, and the `Dict` from `validate` makes a restricted
hash, so an unknown key read off it would be an `error` then.

**Class::Accessor::Typed.**

```perl
use Class::Accessor::Typed (
    rw => { name => 'Str', tags => 'ArrayRef[Str]' },
    ro => { id => { isa => 'Int' } },
    ro_lazy => { conn => { isa => 'DBI::db', builder => 'build_conn' } },
    new => 1,
);
```

This is a `use` statement whose argument list is a declaration. The `USE_STMT`
view exposes the arguments as an expression, and the recogniser reads the
`rw`/`ro`/`wo`/`rw_lazy`/`ro_lazy` keys into `Attribute` symbols exactly as
`has` does. `new => 0` removes the generated constructor; otherwise `new`
takes a `Dict` of the attributes.

**Signatures.**

```perl
sub greet ($self, $who, $times = 1, @rest) { ... }
```

Types are all `Any`; arity is exact: minimum is the count before the first
default, maximum is unbounded with a slurpy and the total otherwise. Perl
already dies on arity mismatch at run time, so reporting it statically is
free of false positives.

**`@_` unpacking.** Unannotated subs whose first statement is
`my ($self, $x, %opts) = @_;` or a run of `my $x = shift;` are given a
positional parameter list of `Any`s for arity's sake. A `%opts` or `@rest`
slurpy makes the maximum unbounded; a sub that touches `@_` in any other way
(`$_[0]`, `scalar @_`, `goto &sub`) gets `Unknown` arity, never reported.

**`Returns:`.** New, and the one annotation this document introduces.

```perl
# Returns: ArrayRef[Item]
sub items { ... }

# Returns: Maybe[Str] | list: (Str, Int)
sub pair { ... }

# Returns: ()
sub notify { ... }
```

Grammar: within the comment block immediately preceding a `sub` (blank
lines allowed between the block and the `sub`, not within it), a line whose
comment text after `#` and whitespace starts with `Returns:`. The rest of the
line is `<type>` for scalar context, `list: (<type>, <type>, ...)` for list
context, both joined by `|`, or `()` meaning "returns nothing; calling in a
context that uses the value is a diagnostic". The `<type>` is the string
grammar. A `Returns:` that fails to parse is a diagnostic on the comment,
because an annotation that is silently ignored is worse than none.

`Returns:` is placed in a comment rather than an attribute (`sub f :Returns(Str)`)
because it has to be addable to code that runs on any perl and under any
attribute handler, and because the formatter already preserves comment text
byte for byte (`docs/contracts.md`, the `comments` invariant), so it cannot
be damaged by `camello format`. The formatter needs no change.

When a sub has both a `Returns:` and inferable returns, the annotation wins
and the inferred shape is checked against it: a `return "x"` in a sub
declared `Returns: Int` is a diagnostic at the `return`.

### Custom type libraries

A project's `MyApp::Types` built on `Type::Library` is the one place a
declaration is made by code rather than by a literal, and the checker reads
the common shapes only:

```perl
declare 'PositiveInt', as Int, where { $_ > 0 };      # -> subtype of Int
declare 'Handle', as InstanceOf['IO::Handle'];        # -> that
class_type 'User', { class => 'MyApp::User' };        # -> InstanceOf
role_type 'Loggable';                                 # -> ConsumerOf
enum 'Color', [qw(red green blue)];                   # -> Enum
union 'Id', [Int, Str];                                # -> Int | Str
```

`as T` gives the parent and `where` is ignored: the structural part of a
constraint is what the checker can use, and the predicate is a run-time
refinement it cannot. A `declare` with a `constraint => sub {...}` and no
`as` is `Any` under that name. Coercions (`coerce ... from ... via`) are
noted so that a slot with `coerce => 1` widens to `Any`, per the non-goals.

A `Type::Library` outside the roots (a CPAN one) is read by the same
recogniser as part of its declarations.

### Stubs

For a dependency that is not analysable — XS, `AUTOLOAD`-generated, or just
written in a style the recognisers do not cover — a project can supply a stub:
a `.pm` file under a stub root (`camello typecheck --stubs stubs/`, or
`stubs = ["stubs"]` in the config) that declares the package's subs with
`Returns:` and signatures and no bodies:

```perl
package DBI::db;
# Returns: Maybe[DBI::st]
sub prepare ($self, $sql) {}
# Returns: Maybe[HashRef]
sub selectrow_hashref ($self, $sql, $attr = undef, @bind) {}
```

A stub is ordinary Perl and goes through the ordinary declaration pass; it
shadows the real module's declarations wholesale when present. This is the
`.pyi` idea with no new syntax, and the same mechanism a project uses to type
its own dynamic corners (`AUTOLOAD` accessors on a legacy base class).

## Inference

Inference exists for one reason: to give the annotated parts something to
check against without asking the user to annotate everything first. It is
local, forward, and gives up early.

- **Literals and constructors.** `"x"` is `Str`, `42` is `Int`, `1.5` is
  `Num`, `[...]` is `ArrayRef[join of element types]` or `Tuple` when short
  and heterogeneous, `{ k => v, ... }` with all-literal keys is `Dict`,
  otherwise `HashRef[join]`, `sub {...}` is `CodeRef`, `qr//` is `RegexpRef`,
  `\$x` / `\@a` / `\%h` are refs of the variable's type, `Foo->new(...)` is
  `InstanceOf['Foo']` when `Foo` resolves, `bless {...}, $class` is
  `InstanceOf[$class's ClassName]` when known.
- **Variables.** A lexical's type is the join of every assignment reaching
  the use, computed over the sub's statements in order with branches joined
  and loops widened to the join of the body — a simple dataflow with no
  path-sensitivity except for the narrowing below. Reassignment is allowed
  to change the type entirely (Perl code does this and it is not a diagnostic).
- **Narrowing.** `if (defined $x)`, `if ($x)`, `if (ref $x eq 'ARRAY')`,
  `if ($x->isa('Foo'))`, `if (blessed $x)`, `if (exists $h{k})`, `// default`
  narrow within the guarded branch. `Maybe[T]` is what most of this is for:
  a method call on a `Maybe[InstanceOf[...]]` with no narrowing is the
  checker's most useful diagnostic and its most likely false positive, so it
  is reported at `warning`, and the narrowing set is a fixture-tested list
  rather than a general theorem.
- **Calls.** A call to a symbol with `returns` yields it; a call to an
  `Unknown` symbol, an unresolved bareword, a `&$code`, or a dynamic method
  name yields `Unknown`. Builtins get a table (`length` → `Int`, `keys` →
  list of `Str`, `shift` → the element type when the array is known…),
  derived from `src/parse/grammar/builtins.rs`'s list and extended with a
  return column.
- **Subscripts.** `$x->{k}` on a `Dict` yields the slot's type or a
  diagnostic when the key is a literal not in the `Dict` and the `Dict` is
  not slurpy; on a `HashRef[T]` yields `Maybe[T]`; on `Unknown` yields
  `Unknown`. `$x->[0]` on a `Tuple` yields the slot, on `ArrayRef[T]` yields
  `Maybe[T]`. Autovivification means a subscript on the *left* of an
  assignment is never a diagnostic.
- **Returns.** A sub with no `Returns:` gets `returns` from the join of its
  `return` sites and its final statement, under both contexts, but only when
  every site is known; one `Unknown` site makes the whole `Unknown`. This
  is the one place inference crosses a sub boundary, and it is done in
  dependency order with recursion cut to `Unknown`.

Nothing is inferred across files except through symbols, and nothing is
inferred *for* a dependency: its subs are `Unknown` unless declared or
stubbed.

## Diagnostics

Every diagnostic has a stable code (`undeclared-variable`, `unknown-method`,
`arity`, `type-mismatch`, `unknown-key`, `maybe-deref`, `bad-annotation`,
…), a severity, a span, and a message that names both sides ("`Str` passed to
parameter `$count` declared `Int` at lib/Foo.pm:12"). Severities:

- `error` — a contradiction between two declared things, or a scope error:
  arity against a signature, a literal against an annotation, a key not in a
  restricted `Dict`, an undeclared variable under `strict`.
- `warning` — a contradiction between a declared thing and an *inferred*
  one, or anything resting on narrowing.
- `info` — the annotation is unparseable, the sub is public and unannotated
  under `--strict-annotations`, and other things a user asked to be told.

`--error-on warning` promotes for CI. Codes can be disabled per project in
the config and per line with `## camello-disable: <code>` (a comment, so
`format` keeps it; the form is chosen not to collide with `## no critic`).

The `GUESS:` discipline from `docs/architecture.md` applies unchanged: a
diagnostic that depends on a parser guess (`foo %h` read as a call, an
all-caps bareword read as a filehandle) is downgraded one level and says so.
The guesses are already enumerable (`grep -rn 'GUESS:' src`), and the CST
does not currently record which of them fired; recording that — a flag on
the node, set by the parser at the guess site — is a small change to `parse`
and the only change this design needs there.

## Dependencies

The project roots are what the command is pointed at (`camello typecheck lib
t`), plus any `lib` dirs in the config. A `use Foo::Bar` from a file in the
roots is resolved:

1. in the roots, as `Foo/Bar.pm` — analysed in full;
2. in the stub roots — declarations, shadowing everything below;
3. in `PERL5LIB` and the `@INC` of the `perl` on `PATH`, asked once per run
   (`perl -e 'print join "\n", @INC'` — reading a list, not running the
   project; this is the one perl invocation and it is cacheable with
   `--inc`) — declaration pass only;
4. nowhere — the package is `Unknown` and every use of it is silent.

The declaration pass over a dependency runs the same recognisers, so a CPAN
Moose class contributes its `has` types and a `Type::Library` its declared
types without any per-module work. The pass reads no bodies and its result
is cached on disk (`.camello-cache/`, keyed by path, size, mtime, and content
hash) so that a run over a project with a large `@INC` costs the declaration
scan once. `scripts/corpus-check` already knows how to find a corpus in
`@INC`; the resolver replaces that shell with a library function.

## Incremental reanalysis

Not built in the first version, but the design leaves the door open, and the
door is the separation above: parsing is per file, declarations are per
file and read no bodies, body analysis is per sub and reads only symbols. An
editor session that keeps the program graph in memory needs, on edit:
reparse the file (rowan makes this cheap), rerun its declaration pass, diff
the declarations, and rerun the body pass for every sub in the file plus
every sub whose *inferred* return depended on a changed symbol. That
dependency set is recorded during the body pass at no extra cost. Salsa or
a hand-rolled memo table can be chosen then; nothing in the first version
should hold a reference that would make the choice harder.

## Testing

Fixtures beside the code, as elsewhere in camello (`docs/contracts.md`,
"Tests and fixtures"). A `sema` fixture is a Perl file with expected
diagnostics as comments on the line they belong to:

```perl
my $x = Foo->new(name => 1);   #~ error type-mismatch: `Int` passed to `name` declared `Str`
$x->nope;                       #~ warning unknown-method
```

A harness runs the checker over the file, and the set of `#~` comments must
equal the set of diagnostics, positions included. Multi-file fixtures are a
directory with a `roots` marker. The formatter's `semantics` invariant is
asked of every fixture too: `camello format` must not move a `#~` comment
off its line, because that would move a `Returns:` off its `sub` in the same
way.

Beyond fixtures: `scripts/corpus-check` gains a `typecheck` mode that runs
the checker over `@INC` and asserts *zero errors* — not because CPAN is
well-typed but because a checker that cannot stay silent on code it was
not told anything about is broken. Warnings are counted and reported, and
the count going up is a regression to look at.

## Milestones

Each is shippable on its own and the order is chosen so that the foundation
under the type work is used by something before the type work needs it.

1. **Workspace split and `ast`.** No behaviour change; formatter fixtures
   green. `camello dev dump` starts printing view names beside kinds.
2. **`camello lint`: scopes.** Undeclared, unused, shadowed lexicals;
   interpolation scanning; `args` and signatures as declarations. Run over
   `@INC`: zero false undeclared-variable errors is the bar.
3. **`camello lint`: arity.** Signatures, `@_` unpacking, `args`/`args_pos`,
   call sites within the roots.
4. **Types: declarations.** The type-expression parser, both syntaxes; the
   four annotation recognisers; `Returns:`; `Type::Library`; stubs; the
   dependency resolver and its cache. `camello typecheck` exists and reports
   `bad-annotation` and nothing else — the point is that every annotation in
   the corpus parses.
5. **Types: flow.** Literals, variables, narrowing, calls, subscripts,
   returns. `type-mismatch`, `unknown-key`, `unknown-method`, `maybe-deref`.
   Corpus bar: zero errors over `@INC`.
6. **`--format json`, per-line suppression, config file, `--strict-annotations`.**

## Open questions

Decided provisionally; written down so that the decision is visible.

- **Bool.** Kept nominal (`Bool` is not `Int`) so that `isa => 'Bool'`
  slots accept `0`, `1`, `''`, `undef`, and `!!$x`, and reject `2`. Moose
  agrees; Perl does not care.
- **Str-as-class.** An unrecognised bareword or string in a type position
  is a class name. This is the Moose reading and it makes a typo in a type
  name (`'Srt'`) into `InstanceOf['Srt']` — resolvable to nothing, hence
  `Unknown`, hence silent. A `warning` for "type or class `Srt` is not known
  to the program" catches it at the cost of firing on every class from an
  unresolved dependency; it is on, at `info`, and promotable.
- **`Returns:` versus an attribute.** Comment chosen; see above. If a
  project already uses `Function::Parameters` or `Kavorka` with return
  types, a recogniser for those is a later addition and does not conflict.
- **Where the config lives.** `camello.toml` at the root the command is
  run from, shared with the formatter's options when those become
  configurable. Not `.perlcriticrc`.

## Decisions made during implementation

The document above was written before the code. Where the code found it wrong
or short of an answer, the reading that keeps the checker *quiet* was taken,
the section was corrected, and the decision is recorded here so that it can be
reviewed as a decision rather than discovered as a difference.

- **`Subscript` is `SubscriptChain`** (milestone 1). Views are generated one
  per `NodeKind` and named after the kind, so `SUBSCRIPT` — the node holding
  one key between one pair of braces — already claims `Subscript`. The union
  the document describes, base plus steps, is `SubscriptChain`. `Call`,
  `MethodCall`, `Assign` and `AnonSub` keep their names, the last three as
  aliases for the generated `MethodCallExpr` / `AssignExpr` / `AnonSubExpr`.
- **A view for every kind, not only the meaningful ones** (milestone 1). The
  document says "one newtype per `NodeKind` that carries meaning", which would
  need a list of which kinds those are and a second list for `dev dump` to
  print from. Generating all of them costs nothing, and `NodeKind::view_name`
  is then total, which is what makes the dump's second column exhaustive.
- **`strict` is read, and positional** (milestone 2). The "Scopes" section
  said `use strict` is taken as on by default. Over @INC that reported an
  undeclared variable in every file written before the pragma was common —
  code perl accepts, because without `strict` an undeclared name *is* a
  package variable. So `strict` is read from the file (`use strict`, a module
  whose import turns it on, `use v5.12` and up), and it is read *positionally*:
  it is a lexical pragma, and `WWW::RobotRules` sets `$VERSION` on line 3 and
  says `use strict` on line 6. `no strict` (bare, or naming `vars`) turns it
  off from where it appears to the end of the file — taking it to the end of
  its block would be more precise and less quiet.
- **`use vars` declares for the file, `our` for its scope** (milestone 2). The
  `vars` pragma's own documentation calls its declarations package-wide rather
  than lexical, and `Time::Zone` relies on it: `use vars qw(%zoneOff)` inside a
  block, read in a sub two hundred lines below. `our` stays lexical, which is
  what perl does with it.
- **Modules that export variables are a table for now** (milestone 2). `use
  English` binds sixty long names to punctuation variables and `use Config`
  binds `%Config`, and neither is visible without running the module's
  `import`. Until the dependency resolver of milestone 4 can read an `@EXPORT`,
  those two are a table, and an import list that names a variable
  (`use POSIX qw($errno)`) declares it.
- **The interpolation scanner has four rules the corpus wrote** (milestone 2).
  A single colon ends a name (`"$filename: not found"`), a subscript after a
  dereference belongs to the dereference (`"$$argv[0]"` reads `$argv`, not
  `@argv`), an `/x` pattern's `#` comments hold no interpolation, and an
  `s///e` replacement is code rather than a string — its `my` is a declaration
  this pass never sees, so scanning it reported the use two lines later.
  Between them these were 402 of the 408 undeclared-variable errors the first
  run over @INC produced.
- **An unpacking list has no minimum** (milestone 3). `my ($data, $header,
  $password, $cipher) = @_` is routinely called with two arguments: perl fills
  the rest with `undef` and the body asks `if defined`. All 285 of the arity
  findings the first run over @INC produced were that shape. So a parameter
  list read off an `@_` unpacking bounds only the *maximum*, and the design's
  "for arity's sake" is read as "for the maximum's sake". A signature and an
  `args` list still bound both, because perl and Smart::Args both die.
- **A bare `shift` past the leading run means the list is unknown**
  (milestone 3). `Net::DBus::RemoteService::new` shifts its class and then
  shifts three more into a hash; `Carp::str_len_trim` writes `shift || 0` on
  its second line. The second is a parameter with a default and is read as
  one; the first makes the whole parameter list `Unknown`.
- **`($)` is a prototype, whatever the parser called it** (milestone 3). The
  parser reads `sub is_info ($)` as a signature with one nameless parameter,
  which is a `GUESS:` — perl has no such signature, since every signature
  parameter is named. A "signature" with a nameless parameter is therefore a
  prototype and the sub's parameters are `Unknown`. This was a run of arity
  errors across `HTTP::Status`, `Crypt::PRNG` and `Getopt::Long`.
- **A method call counts its invocant on both sides** (milestone 3). The
  parameter list keeps its leading `$self` and a call through `->` counts one
  more argument than it writes, because that is what perl passes. Stripping it
  from one side and not the other reported `Dpkg::Email::Address->new()` as
  passing nothing to a sub that wants one.
- **One type-expression parser, not two** (milestone 4). The document has the
  string syntax parsed from text and the bareword syntax walked as a CST
  subtree. They are the same grammar written the same way —
  `ArrayRef[HashRef[Str]]` is one string either way — and a declaration keeps
  the annotation's *text* rather than its subtree, because a declaration
  outlives the tree it was read from and a rowan node is not `Send`. So there
  is one parser, and the bareword form reaches it as the source text the CST
  covered.
- **An annotation is told from prose** (milestone 4). "A `Returns:` that fails
  to parse is a diagnostic" met `File::Temp`, which writes `# Returns:
  modified template` as a sentence and predates the syntax by twenty years.
  So a `Returns:` or an `isa` whose text is not *shaped* like a type — two
  bare names side by side outside any bracket, or anything holding a sigil, an
  arrow or a call — is read as prose or as code, and nothing is said about it.
  What is still reported is `'ArrayRef[Str'`, which is a type expression with
  a bracket missing.
- **A dependency is followed only by `typecheck`** (milestone 4). `lint`'s
  questions are about the roots' own calls, so asking perl for its `@INC` and
  reading a hundred modules to answer them would buy nothing. `require Foo` is
  followed as well as `use Foo`: `HTTP::Date` reaches `Time::Local` that way
  and no other.
- **A hand-written `new` *does* return its own class** (milestone 5, revised).
  This first read the other way — an instance came only from a
  framework-generated constructor or a `Returns:` — on the strength of 380
  `unknown-method` warnings over @INC. That was measured wrong: the 380 fell
  to 31 when opaque packages started reading as `Unknown`, and only 31 to 8
  from refusing to type `Foo->new`. The refusal cost every plain `bless`
  class in the language its types, so the design's reading is back: `Foo->new`
  is an `InstanceOf['Foo']` wherever the run actually read a `sub new`. A
  class it never saw stays `Unknown`, a `Returns:` wins, and a framework's
  constructor never came this way.
- **Four rules keep `$self` honest** (milestone 5, revised). Restoring the
  above made `$self` and `$class` reachable in every body and lit up four
  things that had been quietly wrong: the unpacking statement overwrote the
  parameters it declared (`my ($class, %args) = @_` assigned `Unknown` over
  what `from_unpacking` had just read); an empty `()` is a prototype and not a
  zero-parameter signature, which the body reading `@_` is the evidence for;
  `SUPER::init` was resolved as a method of that name; and a `bless` whose
  class could not be read left the old type in place, when a `bless` always
  changes what its argument is and "unreadable" means nobody knows any more.
- **A dynamic package makes its whole namespace dynamic** (milestone 5,
  revised). XS registers into a distribution's namespace and a distribution's
  namespace is a name prefix: `Net::DBus` calls `XSLoader::load` and the
  methods land on `Net::DBus::Binding::Iterator`, whose own file has no idea.
- **A class the run knows only the name of is `Unknown`** (milestone 5).
  Three more shapes make a package one whose method set nobody can enumerate,
  and each was a run of warnings over @INC: a file that loads XS (every
  package *in that file*, because XS registers methods where it likes), an
  `@ISA` assigned something computed (`File::Spec` picks its parent at run
  time), and a glob assignment. `UNIVERSAL`'s methods — `isa`, `can`, `DOES`,
  `VERSION` — and `import`/`unimport` are on every class and are never
  missing.
- **An element names its container in the flow pass too** (milestone 5).
  `$options{"suffixlen"}` reads `%options`, and reading it as `$options` gave
  every such subscript the scalar's type — which was where all eleven
  `maybe-deref` warnings over @INC came from. The scope pass had this rule
  from the start; the flow pass needed it too.
- **Return inference does not cross a sub boundary** (milestone 5). The
  design has a sub with no `Returns:` take its type from the join of its
  return sites, in dependency order with recursion cut to `Unknown`. That is
  the one place inference crosses a boundary, and it is what would need a
  fixpoint over the program; without it every unannotated sub returns
  `Unknown`, which is silence. `Returns:` is how a sub says otherwise.
- **A suppression comment reads two ways** (milestone 6). `##
  camello-disable: <code>` on a line of code is about that line, and on a line
  of its own is about the line below it. The second is what a long line needs,
  and what a diagnostic whose span *is* a comment needs: a marker about a
  `# Returns:` line cannot sit on that line without becoming part of the
  annotation. A marker naming nothing, or naming something that is not a code,
  silences the whole line — guessing which code was meant would be worse than
  taking the user at their word.
- **`camello.toml` has one table** (milestone 6). `[check]`, not `[lint]` and
  `[typecheck]`: what it holds is true of both subcommands, and a project that
  wanted them to differ would be asking the same question two ways. A flag on
  the command line wins over it, because the file says what the project is and
  the flag says what this run is. A file that does not parse is an error, not a
  shrug: a config silently ignored is a project checked under rules nobody
  asked for.
- **A lone parenthesised list is that list** (milestone 1). `Args::elements`
  and `Args::pairs` descend into a `PAREN_EXPR` that is a list's only element,
  so `use Foo (a => 1)` and `use Foo a => 1` reach a recogniser as the same
  import. perl flattens `f((1, 2))` to two arguments for the same reason.

### Where the corpus bars actually landed

- **Milestone 2, undeclared variables over @INC.** Six remain, all of them in
  `Debconf::Element::Noninteractive::Error`, and `perl -c` reports the same
  six: the file opens `my $mail` in the condition of an `unless` and reads it
  after the statement, which is out of scope. They are true positives in a file
  that does not compile, so the bar — zero *false* undeclared-variable errors —
  is met. 590 warnings (457 unused, 133 shadowed) and 18 files not checked (17
  in an encoding the run was not told about, 1 a parse error).
- **Milestone 3, arity over @INC.** Zero errors. One warning:
  `XML::XPathEngine` calls `XML::XPathEngine::NodeSet->new($results)`, and
  that constructor shifts its class and ignores everything after it — the
  argument is silently dropped, which is what the warning is for.
- **Milestone 4, annotations over @INC.** Zero `bad-annotation`. Every
  annotation the corpus carries parses or is correctly read as something that
  is not one; the single finding of the first run was `File::Temp`'s prose
  `Returns:` line, which the "shaped like a type" test above now leaves alone.
- **Milestone 5, type flow over @INC.** Zero errors from the type
  diagnostics — no `type-mismatch`, no `unknown-key`, no `return-mismatch`,
  no `unknown-type`. 627 warnings in all: 457 `unused-variable`, 133
  `shadowed-variable`, 30 `unknown-method`, 5 `maybe-deref`, 2 `arity`.

  The 5 `maybe-deref` are one file (`LWP::Protocol::nntp`) setting `$nntp =
  undef` in one branch and calling a method on it in another, which is the
  documented cost of an analysis with no path-sensitivity beyond narrowing.

  The 30 `unknown-method` are two kinds of thing. 23 are three families whose
  `new` hands back something that is not an instance of them: `URI` (a
  `URI::http`), `JSON` (a backend chosen at run time), `Crypt::Mode::*` (XS
  bootstrapped from `CryptX`, which is not a prefix of them). That is a fact
  about those three and a stub is what it is for. The other 7 are template
  methods — a base class calling something its subclasses supply, as
  `Dpkg::Interface::Storable` does after asking `$self->can('parse')`. `$self`
  is really "this class or any subclass" and a subclass may define anything;
  reading it that way is the honest fix and is not built.
