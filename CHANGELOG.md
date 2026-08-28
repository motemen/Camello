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

### Fixed

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
