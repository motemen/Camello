# Changelog

One line per change. The reasoning is in the commit it came from.

## Unreleased

### Added

- `--min-severity`, and `min-severity` in `camello.toml`: print nothing below
  a severity. What it drops is dropped whole — not counted in the summary, and
  not a reason to exit 1.
- `unused-parameter`, a code of its own for a parameter the body never reads,
  reported at `info`. A parameter goes on saying what a sub takes whether or
  not the body wants the value, so a project can turn it off without losing
  `unused-variable`. It covers the `my (...) = @_` and `my $x = shift`
  unpackings, which used to report as `unused-variable`, as well as signature
  parameters and `args` items, which used to report as nothing.
- `Class::Accessor::Lite`, `Class::Accessor::Lite::Lazy`, `Class::Accessor` and
  its two speed variants: the `use`-list declaration, the whole `mk_accessors`
  family as a statement, `follow_best_practice`, and `use Class::Accessor
  'antlers'`. The attributes have no types — the modules declare none — but
  the accessor names and whether there is a `new` are enough for
  `unknown-method` to have an answer. The generated `new` blesses whatever it
  is handed, so `unknown-key` stays quiet for it.
- `missing-argument`: a call that does not pass a name the callee requires,
  at `error`, because Moose, `Smart::Args` and `Class::Accessor::Typed` all die
  on it. Named once per call with the names listed. An argument list with no
  written-down keys — `Foo->new(%args)`, `Foo->new({...})` — is not one this
  can count, and says nothing.
- `unknown-key` is now reported for the `Class::Accessor::Lite` constructor,
  at `warning` rather than `error`: it blesses the hash it was handed, so the
  key is still readable as `$self->{key}` and the program may well be right.
- `guard-classes` in `camello.toml`, for a project's own guard classes. A
  value held for its destructor — `Scope::Guard->new(...)`, `guard { ... }` —
  is neither an unused variable nor an unused parameter.
- `use constant`, in all three spellings, declares the subs it names. What a
  constant gives back is an expression nobody here evaluates, so the type is
  `Unknown` — but the name is there, and a package whose constants were
  invisible answered `unknown-method` to every one of them.
- `read-as` in `camello.toml`: a module of the project's own, and the module
  whose interface it re-exports. Recognition is by an import that could have
  provided the name, and a wrapper around `Class::Accessor::Typed` is exactly
  what takes that import away — every file says `use My::Accessors`, and the
  wrapper's own file is a run-time `sub import` that no recogniser can read.
  It renames a module only for the recognisers: the resolver still looks for
  the wrapper's own path. The declaration cache is keyed by it.

### Fixed

- A bareword call to a builtin's name is the builtin. A `sub delete` in the
  package no longer takes `delete $h->{k}` away from perl's own `delete`,
  a shape covered by the builtin-call fixture. An import still answers, because importing the name is the one
  mechanism perlsub gives for overriding a builtin.
- `keys` and `values` are no longer `Int` everywhere. The answer depends on
  the context, and the elements of a `[ ... ]` are the one place it is written
  down rather than guessed at: `[ values %$h ]` is an `ArrayRef` of what the
  hash holds, and `[ keys %$h ]` one of `Str`. In scalar context they are
  `Unknown`.
- `scalar` reads its argument. A container's is its count; anything else is
  that expression in scalar context, which is what every type here already is.
  `scalar $sth->bind` was an `Int` that then failed to be an `ArrayRef`.
- A `method` has the invocant perl gives it and the declaration never names,
  so `$obj->f` is no longer one argument too many for a `method f()`. An empty
  `()` on a `method` is always a signature — prototypes do not exist under the
  `class` feature — so the prototype guess does not apply to it.
- An `optional` `Smart::Args` parameter may be passed `undef`. The module
  reads the rule before the type and returns an undefined value without ever
  asking the constraint, so `f(x => undef)` against `{ isa => 'Str', optional
  => 1 }` is a program that runs.
- `undef` fits a `Bool` slot. Moose and Types::Standard both give `Bool` the
  four values `0`, `1`, `''` and `undef`, which is what the type here already
  said it was.
- `Class::Accessor::Typed` slots are **mandatory** unless they say `optional`,
  give a `default`, or are lazy — the reverse of Moose's rule, and what the
  generated `new` dies on. They were all being read as optional.
- `use Class::Accessor::Typed (new => 0)` keeps its answer in a file that also
  says `use Moose` or `use Mouse`. Frameworks are read per file, so the sweep
  that gives every Moose package a constructor was handing one back to a
  package that had just turned it down — which is how
  `Class-Accessor-Typed-0.03/t/02_does.t` is written.
- A name a project's own type library declares now stands behind the
  annotations that write it. `declare` / `subtype` / `enum` and the rest were
  read into the program graph and then never consulted, so `args my $n =>
  Count` was checked against `InstanceOf['Count']` — an `unknown-type` and a
  `type-mismatch` against every real argument.
- `type Foo => as ...` and `intersection` are recognised alongside `declare`
  and `subtype`.
- The type DSL is now gated on a *family* of imports rather than a list of
  exporters: `Type::*`, `Types::*`, `MooseX::Types*`, `MouseX::Types*` and
  `*::Util::TypeConstraints`. Half a dozen distributions supply the same
  vocabulary and a file commonly names only the `Types::` module its constants
  came from; being wrong about which one exported `type` cost a whole library's
  worth of annotations.
- A call's arguments are typed once instead of twice. A Perl list operator
  swallows everything to its right, so the hundred entries of a `Dict [ 'k' =>
  header nullable Str, ... ]` are a hundred *nested* calls, and a second walk
  per level cost 2^depth: `camello typecheck` on a real `Type::Library` file
  did not finish. It also said whatever an argument had to say twice, which is
  where a run's duplicate `unknown-method` and `maybe-deref` lines came from.
- A `+` disambiguator is looked through. `+{ ... }` is how a writer keeps perl
  from reading the brace as a block and says nothing about the value, but every
  reader that matched on the shape of a hashref missed it: `args my $x => +{
  isa => 'Int', default => 10 }` was read as a rule with no `default`, so the
  parameter came out mandatory and `missing-argument` fired at every call that
  left it out. The same value written `{ ... }` was read correctly. It also
  covers a `Class::Accessor::Typed` slot, the accessor and attribute name
  lists, and the type a `+{ ... }` expression has.
- `optional => 0` in an `args` rule means the parameter is required, the way it
  already did for a `Class::Accessor::Typed` slot. Anything the rule cannot
  read as a number is still taken as optional.

## 0.1.1 — 2026-08-27

### Added

- perltidy's `#<<<` / `#>>>` format-skipping markers (VERBATIM-2).
- `--align-use-imports`, hidden and off: line up the import lists of a run of
  `use` or `no` (ALIGNMENT-2).

### Fixed

- A paren-less call no longer pushes the lines under it out of the list they
  belong to.
- A paren-less call whose first argument is a block written across lines no
  longer puts what follows to the right of the brace that closes it.
- An own-line comment before `)`, `]` or `}` is indented with the contents
  (COMMENT-2).
- A group the writer broke across lines without a newline after the opening
  bracket is aligned like any other (ALIGNMENT-1).
- One unparsed source is left alone and exits 1, as a tree of them already was;
  standard input comes back unchanged.
- A paren-less call written as the value of a pair keeps the lines under it at
  the column the list around them hangs from, instead of sending them out to
  the margin.
- A `{ key => ... }` opening a statement is the anonymous hash perl reads it as,
  so its pairs take a line each instead of a continuation indent.
- A wrap inside a block written in an argument list gives its indent level back
  at the block's brace, instead of at the call's closing bracket (INDENT-3).
- A comment written in a gap that takes no space — before `->`, after an opening
  bracket — is indented with the expression around it instead of landing in
  column 0.
- The parentheses of `my (...)` and of a subroutine signature break where the
  writer broke them, like every other bracket (INDENT-2).

### Changed

- An unknown list operator written as the value of a `key => value` pair stops
  before the next pair.

## 0.1.0 — 2026-08-26

The first tag. No changelog was kept before it; `git log` is the record.
