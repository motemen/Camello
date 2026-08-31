# Design: inferring what a sub returns

`docs/types.md` says, at INFER-4a and again at LIMIT-6, that the return type
of a sub with no `Returns:` is `Unknown`. `docs/typecheck.md` planned
otherwise — "a sub with no `Returns:` gets `returns` from the join of its
`return` sites" — and the decision recorded under milestone 5 put it off
because it is the one place inference needs a fixpoint over the program.

This document is the design for putting it in. Its premise is pyright's:
the tool is only as useful as the fraction of the program it can say
something about, and in a codebase that is not going to grow a `Returns:`
on every sub, that fraction is set by whether `$self->items` can be typed
from what `sub items` does. Everything the checker already reports —
`unknown-method`, `maybe-deref`, `unknown-key`, `type-mismatch` — is
reported more often once it can, and every one of those extra reports is
also a new place to be wrong. The design is therefore mostly about which
returns *not* to infer.

## What is being built

A sub with no `Returns:` gets a `returns.scalar` read off its body, so
that a call to it — bareword, imported, or as a method — yields that type
where it yields `Unknown` today. Hover shows it, marked as inferred. The
list half follows in a second phase, once the checker can read an
expression in list context at all.

The rule that keeps the checker quiet is kept whole: the inferred type is
the **join of every site, and `Unknown` if any site is `Unknown`**. One
site the walk cannot type makes the sub `Unknown`, and `Unknown` is never
reported against. This is not a precision choice but the reason the
feature can be shipped at all — a partial join (`Str` from the two sites
that were typed, ignoring the third) would be a type the program does not
have, reported at every call site.

### Non-goals

- **List context, in the first phase.** `return (a, b)` and `return
  @list` are sites whose scalar type is `Unknown`, which makes the scalar
  half of the sub `Unknown`. The list half is a phase of its own — "List
  returns" below — because it needs the list-context reading of
  expressions that INFER-6a says does not exist yet, and that reading is
  worth building for its own sake.
- **`Returns: ()`.** Never inferred. A sub whose every site is `return;`
  is, in scalar context, one that returns `Undef`, and that is what it
  gets; "returns nothing, do not use the value" is a statement about
  intent that only an annotation can make.
- **Dependencies.** A module read off `@INC` is walked once for its
  declarations and never for its bodies. The local tier below runs
  inside that walk, so a dependency's leaf subs *are* inferred; what is
  not done is the program-wide tier over a dependency's bodies.
- **Return type as a check on the body.** `return-mismatch` is only ever
  against a written `Returns:` (ANNOT-7a). An inferred type has nothing
  to contradict.

## Sites

A site is a place a value leaves the sub. The walk collects them per
`SUB_DEF`, resetting on entry to an `ANON_SUB_EXPR` exactly as `returns`
is reset today — a `return` inside a callback is the callback's.

| site | scalar type |
| --- | --- |
| `return EXPR` | `type_of(EXPR)` — the pass's existing scalar reading |
| `return;` | `Undef` |
| `return undef` | `Undef` |
| `return (A, B)`, `return @x`, `return %h`, `return wantarray ? ... : ...` with a list on either side | `Unknown` |
| `return wantarray ? A : B` | `type_of(B)` — the scalar branch, by definition |
| `die`, `croak`, `confess`, `throw`, `exit` as a statement | not a site (bottom); contributes nothing to the join |
| the **tail**: the last statement of the body, when it is an expression statement | `type_of` of that expression |
| the tail is an `if`/`unless`/`elsif`/`else` chain | the join of each branch's tail; a chain with no `else` has an `Unknown` site, because the value of a false `if` is the condition's |
| the tail is a loop, a bare block, a `package`, a nested `sub`, or anything else | `Unknown` |
| the tail is a `return` or a `die` | already counted; nothing extra |
| an empty body | `Unknown` |

The tail is a site because `sub name { $_[0]->{name} }` is how half the
accessors in the corpus are written, and it is the site that makes a
tail-only setter — `sub set_x { $_[0]->{x} = $_[1] }` — return what it
was assigned, which is what perl does. `goto &other` anywhere in a body
makes the sub `Unknown`.

Two consequences worth naming. A sub that ends `return 1;` after doing its
work returns `Int`, and a caller that does `$obj->save->name` will be told
`Int` has no methods — which is true. And a sub with one `return undef`
among object returns is `Maybe[InstanceOf[...]]`, which is the checker's
most useful diagnostic (`maybe-deref`) and its most likely false positive
(NARROW); it is reported at the severity the design already gives it, and
the corpus bar below is where the count is watched.

## `$self` comes back as the caller's class

```perl
package Base;
sub set_x { my $self = shift; $self->{x} = shift; return $self }

package Child;  use parent -norequire, 'Base';
sub extra { ... }

Child->new->set_x(1)->extra;
```

`$self` is bound to `InstanceOf['Base']` in `Base::set_x`, so the naive
join says `set_x` returns a `Base`, and `->extra` on it is
`unknown-method` — a false positive on the single most common shape of
chained method, and one the naive design would report on every builder
in the corpus. A sub that returns its invocant returns **the class it was
called on**, not the class it was written in.

So a site whose expression *is* the invocant variable — `$self`, or
whatever the first parameter of a method is called, or `$_[0]` when the
sub does not unpack — is recorded not as `InstanceOf[package]` but as a
marker, and the marker is resolved at the call site:

- `Returns` grows `invocant: bool`. When the join of the non-invocant
  sites is empty and every site is the invocant, `returns.scalar` is
  `InstanceOf[own package]` (the honest fallback for a bareword call)
  and `invocant` is `true`.
- `Pass::method_call`, on `MethodLookup::Sub(symbol)` with
  `symbol.returns.invocant`, answers `InstanceOf[class]` where `class` is
  the receiver's class as already resolved for the lookup — the same
  substitution `constructs_own_class` performs for `new` today, and for
  the same reason.
- A site that is the invocant *joined with* something else
  (`return $ok ? $self : undef`) sets `invocant` and keeps the
  `Undef` in `scalar`; the call site substitutes the `InstanceOf` member
  and keeps the rest. Simplest form: `scalar` holds
  `InstanceOf[own package] | Undef`, and substitution replaces the
  `InstanceOf[own package]` member with `InstanceOf[receiver]`.

A `$class` invocant returning `$class->new(...)` is already covered by
`constructs_own_class` and INFER-2g; this design does not touch `new`.

## Two tiers

The declaration pass reads no body, and that is what makes it
parallel, cached, and the unit of the language server's decl-diff. Return
inference has to read bodies, and a body's type depends on other subs'
returns — across files. The design splits along that line.

### Tier 1: local, inside the declaration pass

`decl::declare_in` already has the CST. After it has collected the file's
`FileDecls`, it builds a throwaway single-file `Program` from them and
runs the return walk over every `SUB_DEF` whose symbol has no `Returns:`,
iterating until no sub's `returns` changes (bounded: a sub goes from
`Unknown` to known once and never changes after, so the rounds are at
most the depth of the file's own call chains; in practice two or three).
What it can type is what a single file can see: literals, constructors,
`bless`, attributes and `has`-generated accessors of the file's own
packages, the file's own subs, the invocant marker. What it cannot —
a call into another file — is `Unknown`, and the sub stays `Unknown`
*for now*.

The result is written into `SubDecl.returns` with `Returns.inferred =
true`, and so is

- **cached** with the rest of `FileDecls`, which is how a dependency on
  `@INC` gets its leaf subs typed at no cost beyond the first run. The
  cache key's salt gains a format version so the entries written before
  this are not read as complete;
- **fingerprinted** by the language server, because `signature_of`
  renders the return: an edit that turns `return [...]` into `return
  {...}` is now a declaration change, and the open files are reswept.
  That is correct — it *is* a change other files can see — and it is the
  reason tier 1 has to live in the declaration pass rather than beside
  it.

### Tier 2: program-wide, after `link`

Once every file is in and linked, `Analysis::infer_returns(&mut self,
sources)` runs rounds over the **root** files:

1. For each root file, in parallel, parse (or take the tree the caller
   already has) and run the return walk over the subs whose `returns` is
   still `Unknown` and not annotated, against the full `Program`.
2. Collect `(file, sub index, Returns)` for every sub that became known.
3. Install them — `Program::set_returns` updates the entry's `decls.subs`
   and the flattened `subs` copy the name index answers from — and, if
   anything was installed, go to 1.

Monotone: a sub becomes known at most once, and its type is final when
it does, because it was computed from callees that were themselves final.
Termination is the number of subs; in practice the rounds are the depth
of the longest cross-file chain of unannotated subs, and each round only
walks the subs still unresolved. A recursive or mutually recursive sub
whose every path goes through the recursion stays `Unknown` in every
round, which is the "recursion cut to `Unknown`" the original design
asked for, without a call graph having to be built to find it.

Reparsing per round is the simple choice and the corpus says whether it
holds: the whole `@INC` walk parses in under a second, and a round parses
only files with something left to resolve. Keeping every root's CST alive
across rounds is the alternative if it does not; the interface — the walk
takes a `&SyntaxNode` — is the same either way.

### Where it runs

- **`camello check`** (`src/report.rs`): between `analysis.link()` and the
  per-file `check_one` pass. The bodies are parsed there for a second
  time today (`declare` parses, then `check_one` parses again); tier 2
  adds its rounds' parses in between, and the honest cost is measured,
  not guessed.
- **`camello lsp`** (`crates/camello-lsp/src/index.rs`): in the
  background walk, after `link`. The roots are the workspace files.
- **Single-file mode** (`analysis::single_file`, before the index is
  ready): tier 1 alone, which is everything a single file can know.

## What changes for the incremental loop

`docs/lsp.md` step 3 diffs the file's declarations against a memo and
resweeps open files only when they changed. Tier 1 folds into that
unchanged: the inferred return is part of `signature_of`, hence of the
fingerprint, hence of "changed".

Tier 2 needs one more step. A body edit that keeps tier 1's answer but
changes what tier 2 would say — `return $self->load` edited to `return
$self->parse`, both cross-file — is invisible to the fingerprint, and
the callers in other open files go on seeing the old type. So on each
edit, after `install`:

4′. Run tier 2 for the edited file alone against the current graph. If
    any of its subs' `returns` differ from what the graph holds, install
    them and treat the edit as a declaration change (step 5: relink,
    resweep open files, and the resweep runs 4′ for each of them in
    turn, bounded by the open set).

A file that is neither the edited one nor open keeps a possibly stale
tier-2 answer until the next full walk. That is the same coarseness step
5 already accepts and is written down here so that it is a decision.

`Program::replace` rebuilds the indexes from the installed `FileDecls`,
which carries the tier-1 returns but not tier 2's: a replace of file A
loses A's tier-2 answers until 4′ restores them a moment later, and
that is the order 4′ runs in. Tier-2 answers for *other* files survive a
`replace` because they live in those files' entries.

## What does not change

- **`Returns:` wins.** An annotated sub is never inferred. `return-mismatch`
  is still only ever against a written annotation.
- **`--strict-annotations`** still asks for something written down.
  `SymbolSource::Annotated` is unaffected; an inferred return does not
  satisfy it, because the option exists to ask for the annotation.
- **Severity.** A `type-mismatch` whose value came through an inferred
  return is `warning`, as any inferred value is; `is_literal` is about
  the argument expression, not what it was computed from, so nothing
  moves.
- **Stubs.** A stub's `Returns:` is an annotation and wins over what the
  real module's body would have said. This is what makes a stub the
  answer to `URI->new` returning a `URI::http`.

## Cost and the corpus bar

Tier 1 is one extra body walk per file in the declaration pass. The
decl walk over `@INC` is 0.94 s today with no body walked; the body pass
is the expensive one, and doubling it on the cached path is the
number to measure first (`camello dev index`, `scripts/lsp-bar`). If it
is not acceptable, the fallback is a walk that visits only the subs
whose sites are all of the cheap kind — literal, constructor, `bless`,
invocant, tail accessor — and leaves everything else to tier 2. That is
a narrowing of tier 1, not a change to the design.

The bar is the milestone-5 bar (`scripts/corpus-check --check`, and the
counting recipe in `docs/typecheck.md`, "Where the corpus bars actually
landed"): **zero errors over `@INC`**, and the warning count compared
line by line (`comm`) before and after. Every new warning is one of two
things — a true positive the feature exists for, or a shape of return
the site table above should have made `Unknown` — and the review of that
diff is the last step of the implementation, not the first step of the
next feature. The families expected to show up:

- `maybe-deref` from `return undef` sites — kept, that is the point;
- `unknown-method` on a `$self` returned through a helper the invocant
  rule does not see (`my $s = $self; ... return $s`) — narrow the rule to
  the variable's *type* carrying the marker rather than its name if
  they are numerous;
- `type-mismatch` on a sub whose tail is incidental (`$h{k} = 1` as the
  last statement of a sub whose result a caller passes to an `ArrayRef`
  slot) — expected to be rare; if not, assignments stop being tails.

## Testing

Fixtures under `crates/camello-sema/src/fixtures/types/returns/`:

- `local.pl` — literals, constructors, `bless`, tail accessors, `return;`
  giving `Maybe`, a `wantarray` ternary, a `die` that is not a site, a
  list return that makes the sub `Unknown`, an empty body;
- `invocant/` — a two-file fixture: `Base` returning `$self`, `Child`
  chaining a method of its own off it, with no `#~` expected; and the
  `$ok ? $self : undef` shape reporting `maybe-deref`;
- `cross-file/` — a chain of three unannotated subs across three files
  resolved in tier 2, a mutual recursion left `Unknown`, and an
  annotated sub in the middle of the chain shadowing what its body says;
- `annotation-wins.pl` — a `Returns:` beside a body that says otherwise
  reports `return-mismatch` and yields the annotation at the call.

Hover tests in `crates/camello-lsp` for the ` -> Str (inferred)` rendering,
and an index test for step 4′: edit a body across the tier-1/tier-2 line
and assert the other open file's diagnostics moved.

## Implementation plan

Each step leaves the tree green and shippable.

1. **The walk** (`flow.rs`). `Pass` gains `sites: Option<Vec<Type>>`
   and `tail: Option<Type>`. `call()` on `return` pushes a site; `expression_statement` records the tail (`None` for a `return`/`die`
   statement); `if_statement` joins its branches' tails; `loop_statement`
   and the rest clear it. `pub fn infer_returns(root, file, program,
   only: &[sub index]) -> Vec<(usize, Returns)>` walks the named
   `SUB_DEF`s and applies the site table. No diagnostics leave it. The
   invocant marker is a site of `Type::InstanceOf(package)` flagged by
   comparing the expression against the bound invocant name.
2. **The model** (`annotate.rs`, `decl.rs`). `Returns { scalar, list,
   inferred: bool, invocant: bool }` with serde defaults; `signature_of`
   renders `-> T (inferred)`; the cache salt gains a version.
3. **Tier 1** (`decl.rs`). After `declare_in` has its `FileDecls`, build
   the single-file `Program`, iterate `infer_returns` to a fixpoint, write
   back. Fixtures `local.pl` and `annotation-wins.pl` go green here, and
   single-file LSP mode gets the feature for free.
4. **The call site** (`flow.rs`). `call()` and `method_call()` already
   return `symbol.returns.scalar`; the invocant substitution goes into
   `method_call`. Fixture `invocant/`.
5. **Tier 2** (`lib.rs`, `program.rs`, `src/report.rs`, `index.rs`).
   `Program::set_returns`, `Analysis::infer_returns`, the rounds, wired
   into the CLI and the background walk. Fixture `cross-file/`; the
   fixture harness runs tier 2 over a multi-file fixture's roots.
6. **The edit loop** (`index.rs`, `server.rs`). Step 4′. The index test.
7. **The corpus.** `scripts/corpus-check --check` before and after, the
   `comm` diff, the site table revised from what it shows, and the
   numbers written into `docs/typecheck.md` beside milestone 5's.
8. **The spec.** INFER-4a and LIMIT-6 rewritten in `docs/types.md`;
   the site table and the invocant rule become numbered rules there.

## List returns

Half the subs in a Perl codebase hand back a list — `return @rows`,
`return map { ... } @x`, `return ($ok, $err)`, `return %h` — and a
design that types only the scalar half leaves every `my @rows =
$self->rows` and `my ($ok, $err) = validate()` where it is today. So the
list half is in scope. It is not hard to *collect*; what makes it a phase
of its own is that the checker has no notion of list context to hand the
result to. INFER-6a says every expression is typed in scalar context, and
the assignment walk says so in code: a list assignment "hands out elements
nobody here counts", and every target of `my ($a, $b) = ...` or `my @a =
...` is bound to `Unknown`. The `list:` half of `Returns:` is parsed and
never consulted (INFER-6b). Building the reading is what makes both the
annotation and the inference mean something, and it is the same work
either way.

### The notation

The `list:` form goes. `Returns:` reads one of four things, and a
parenthesised body is a list:

```perl
# Returns: Str               scalar context: Str
# Returns: (Str, Int)        list context: exactly two, Str then Int
# Returns: (Row ...)         list context: any number of Row
# Returns: ()                nothing — as today
```

- A body that is parenthesised from its first character to its last is
  a list shape. Inside, a top-level comma separates slots, and a single
  type followed by `...` is the repeated form. `(Str)` is a list of one,
  because `()` is a list of none and the two have to agree; a grouping
  parenthesis around a whole scalar type has no use that `Str | Undef`
  does not serve, so nothing is lost. Parentheses *inside* a slot or a
  scalar type keep grouping (`(Str | Undef, Int)` is two slots).
- A sub that has both halves writes **two `Returns:` lines** in its
  leading comment block, one scalar and one list, in either order. The
  reader today stops at the first `Returns:` line it finds; it reads
  every one instead, and a second line of the same kind is a
  `bad-annotation` ("`Returns:` names a scalar type twice").
- A list-only annotation says nothing about scalar context, and a
  scalar-only one nothing about list context: the other half is
  `Unknown`, and silent. The comma operator would make `Returns: (A, B)`
  a `B` in scalar context and `(Row ...)` a count; the two rules
  disagree, and a sub that wants a scalar type writes it.
- `| list: (...)` is a `bad-annotation` whose message shows the new form.
  Two fixtures and one unit test carry the old one; they are rewritten.
- `signature_of` renders `-> Str`, `-> (Str, Int)`, `-> (Row ...)`, and
  `-> Str, (Str, Int)` for both; the fingerprint follows it.

This is a change to a recogniser that exists, so it is the first step of
the list phase and can ship on its own — before any inference — as the
annotation half of the same feature.

### The shape

`ListShape` grows one variant:

```text
Unknown
Nothing            Returns: ()
Fixed(Vec<Type>)   (Str, Int)       — a known length, a type per slot
Of(Type)           (Row ...)        — any length, one element type   [new]
```

*(Superseded: two shapes of the same length are kept apart as
`Either`, not joined slot-wise — see ANNOT-7e in [types.md](types.md).
The correlation between the slots of `(Value, Undef) | (Undef, Error)`
is the whole content of the idiom, and the slot-wise answer says the two
slots vary independently, which is what it promises they do not. Slot-wise
is still what a *binding* gets, because nothing carries the correlation
past the assignment, and it is what more than `ListShape::ALTERNATIVES`
alternatives collapse to.)*

The join of two shapes: both `Fixed` of the same length is slot-wise
union; `Fixed` against `Fixed` of another length, or against `Of`, is
`Of(union of every member)`; anything against `Unknown` is `Unknown`.
`return $x` beside `return;` is therefore `Of(T)`, and a `my ($x) = f()`
off it binds `Maybe[T]` — the length was not known, so the slot may be
empty.

### The list-context reading

`Pass::shape_of(node) -> ListShape`, beside `type_of`, for the node kinds
that have a list-context answer. Everything else is `Unknown`, and the
list stays as short as the scalar table did when it started:

| expression | shape |
| --- | --- |
| `(A, B, ...)` and a bare `A, B` list | `Fixed([type_of(A), type_of(B), ...])`; an element that is itself plural (`@x`, a call, a `map`) makes the whole thing `Of(join)` |
| `()` | `Fixed([])` |
| a single scalar expression | `Fixed([type_of])` |
| `@a`, `@$ref`, `$ref->@*` | `Of(element type)` when the array's element type is known, else `Unknown` — the same `element_of` the `foreach` header uses |
| `%h`, `%$ref` | `Unknown` (a hash in list context is key/value pairs, and nothing downstream wants them as a list) |
| `map { ... } LIST`, `grep { ... } LIST`, `sort LIST`, `reverse LIST` | `grep`, `sort`, `reverse`: the shape of `LIST` widened to `Of`; `map`: `Of(Unknown)` unless the block is a single expression, in which case `Of(type_of(block))` with `$_` bound to the element |
| `keys`, `values` | `Of(...)` by the rule `list_element` already applies to them |
| `f(...)`, `$obj->m(...)` | the callee's `returns.list` |
| `wantarray ? A : B` | `shape_of(A)` |
| `A ? B : C` otherwise | the join |
| `@{[ ... ]}`, `[...]`, `{...}`, a literal, a `bless` | `Fixed([type_of])` — one reference |

### Sites, in list context

The site table gains a column. Each site contributes both halves:

| site | scalar | list |
| --- | --- | --- |
| `return EXPR` | `type_of(EXPR)` | `shape_of(EXPR)` |
| `return;` | `Undef` | `Fixed([])` |
| `return (A, B)` | `Unknown` | `Fixed([A, B])` |
| `return @x` | `Unknown` (a count, but only because it is; saying `Int` invites `my $rows = $self->rows` to be typed as an `Int` when the author meant the list and got the count — a bug the checker should stay quiet about rather than certify) | `Of(T)` |
| `return wantarray ? A : B` | `type_of(B)` | `shape_of(A)` |
| tail | as before | `shape_of` of the same expression |

A sub's `returns` is then `(join of scalar sites, join of list sites)`,
each half independently `Unknown` if any of its sites is. The two halves
are independent: `return @x` sinks the scalar half and not the list half,
and `return $obj->maybe` — a method with a scalar type and no list
shape — sinks the list half and not the scalar one.

### Consumers

Where the shape is read. Each is a small change to `assignment()` or
`list_element()`, and together they are what the phase delivers:

- `my ($a, $b) = EXPR` — `Fixed`: slot `i` to target `i`, a missing slot
  is `Undef`, `%h` or `@a` as the last target takes the rest; `Of(T)`:
  every scalar target is `Maybe[T]`. `Unknown` binds `Unknown`, as
  today. The `unpacks_arguments` guard stays: `@_` is typed by the
  parameter list, not by this.
- `my @a = EXPR` — the array's element type is the join of the shape's
  members, which is what `foreach my $x (@a)` and `\@a` already read.
  Today `@a` is never bound at all.
- `my %h = EXPR` — `Unknown`, until something wants the pairs.
- `[ EXPR ]` — `list_element` becomes `shape_of` over each element,
  flattened, so `[ $self->rows ]` is `ArrayRef[Row]` rather than
  `ArrayRef[Unknown]`.
- `foreach my $x (EXPR)` — `Of(T)`/`Fixed` give `$x` the element type
  where `element_of(type_of(EXPR))` gives `Unknown` today.
- `return EXPR` — the shape becomes a site, above, which is what makes
  `sub rows { return $self->_load_rows }` transitive.
- **Arity is not a consumer.** `g(f())` where `f` returns `Fixed` of two
  is an arity of two in perl; the arity pass counts arguments
  syntactically and keeps doing so. Flattening a call into a parameter
  list is where the false positives would live, and it is not built.

### What it checks

Nothing new is reported by this phase except through the types it
binds: a `$row` bound to `Row` off `my ($row) = $self->rows` gets
`unknown-method` where an `Unknown` did not. The one addition is that
`Returns: (A, B)` starts being *used*: a call in list context yields the
annotated shape, and — the other half of ANNOT-7a — a `return (A, B)` in
a sub declared `Returns: (Str)` is a `return-mismatch`, length included,
and `return @rows` against `(Row ...)` is checked by element. `Returns: ()` keeps its existing meaning and is still never
inferred. LIMIT-7 is rewritten to say what is now matched and what is
not (hashes, arity).

### Ordering and cost

The scalar phase ships first (steps 1–7). The list phase is its own
sequence after it:

9. The notation: `read_returns` reads every `Returns:` line, the
   parenthesised body, `...`, and rejects `list:`; `ListShape::Of`, the
   join, `signature_of`. Shippable alone.
10. `shape_of` over the table above; `map`/`grep` with `$_` bound.
11. The consumers, one fixture each under `fixtures/types/returns/list/`:
    `assign.pl`, `array.pl`, `anon-array.pl`, `foreach.pl`, and
    `annotation.pl` for `Returns: (A, B)` finally being matched.
12. Sites: the list column, through both tiers unchanged — a shape is a
    value in `Returns` like the scalar half, and everything that carries
    `returns.scalar` across the graph (cache, fingerprint, `set_returns`,
    step 4′) carries the pair.
13. The corpus, again, with the same bar. The families to expect:
    `unknown-method` on an element type that was joined too wide
    (`return @objects` where the array held two classes — a union, and
    the union rule already handles it) and `maybe-deref` on `my ($x) =
    f()` off an `Of` — a true positive, and an idiom (`my ($first) =
    grep {...}`) common enough that its count decides whether `Of` binds
    `Maybe[T]` or `T` to a single target.

The cost is the reading itself: `shape_of` is a second dispatch over the
same node kinds, and the consumers are edits to walks that exist. The
inference and the tiers are unchanged by it. Nothing here needs a new
pass, a new phase, or a change to the program graph beyond the variant.

## Drift

An annotation wins at every call site (ANNOT-7a), so the only thing that
ever compares a `Returns:` against the code is `return-mismatch`, and that
looks at one `return` at a time. What it cannot see is the drift a file
collects: an annotation that was right when it was written and has since
been widened by a new `return undef`, narrowed by a branch that went away,
or simply contradicted.

`camello check --returns-drift` asks the walk what each annotated sub's
body says and puts the two side by side:

```text
lib/Store.pm:12:5: Store::find: `Returns:` says `InstanceOf['Row']`, the body says `InstanceOf['Row']|Undef`
```

A `Bool` and an `Enum` are read loosely, and only there. There is no boolean
literal and no enum literal in perl: `return 0` and `return 'draft'` are how
they are handed back, and the walk reads an `Int` and a `Str` — so comparing
by equality made `Returns: Bool` and `Returns: 'draft' | 'live'` drift every
time they were written down, which is also the one case where the annotation
is the only thing that can say what was meant (`docs/types.md`, TYPE-5c and
TYPE-5e). What is still drift there is a reference under a `Bool`, and a body
that hands back `undef` under an `Enum` that did not say `Maybe`.

Nothing is installed and the program is not changed, which is the point:
the answer for one sub is computed against the annotations every *other*
sub still carries, so what comes out is a report on that annotation rather
than on the whole file at once. Only a half the annotation actually claims
is compared — a scalar-only `Returns:` says nothing about list context, so
a list the body inferred beside it is an addition and not a disagreement —
and a half the walk could not read is never evidence against something
written down. The exit status is 1 when anything was found, so a CI step
can hold a codebase to its own annotations.

## Open questions, and what they turned out to be

All four are settled; the numbers behind them are in `docs/typecheck.md`,
"Where the corpus bars actually landed".

- **Should tier 1 see `@INC` bodies at all?** Yes, as designed, and no knob
  was needed. The worry was that an inferred `Maybe` in a CPAN module's sub
  becomes a `maybe-deref` in the project with no `Returns:` the project can
  edit to disagree. The corpus shows that family exists and is small, and
  every instance of it is a sub that really does hand back `undef` on one
  path; `[check] infer-returns-in-dependencies = false` stays unwritten
  until somebody has a count that asks for it.
- **Widening.** None was added. The join of `Int` and `Str` sites is `Int |
  Str`, which `compatible` treats member-wise, and the corpus proposed
  nothing further.
- **Hover wording.** `-> Str (inferred)`. `~> Str` was the shorter
  alternative and it reads as a *different relation* rather than as the same
  one read differently, which is the opposite of what the note is for. A
  list of one holding the scalar type is not shown at all: every `return $x`
  is one, and it says nothing the scalar half did not.
- **`Of` at a single target.** `Maybe[T]`. The count that was to decide it
  is one `info` over `@INC`, and it is also the honest answer: `my ($first)
  = grep {...}` really can find nothing.
