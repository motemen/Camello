# Changelog

One line per change — two or three where it takes them, never a paragraph.
The reasoning, the measurements and the caveats belong in the commit it came
from, in a comment, or in `docs/`.

## Unreleased

### Added

- **`camello lsp`**, a language server over the machinery that was already here
  (`docs/lsp.md`): diagnostics as you type, hover, method completion after `->`,
  an outline, go-to-definition and whole-file formatting, configured by the
  `[check]` table `camello check` already reads. A VS Code client is in
  `editors/vscode/`.
- The checker runs on a **broken buffer**: a diagnostic whose range meets the
  enclosing statement of a parse error is dropped and the rest is published,
  where `camello check` discards them all.
- Hover answers `Unknown` rather than nothing where there is something it cannot
  type.
- `camello-sema` hands out what its passes already knew: a type side-table
  (`flow::analyse_recording`), a method surface (`Program::methods_of`) and a
  scope table (`ScopeReport::bindings`, `::references`).
- `camello dev index` and `scripts/lsp-bar`, the corpus bars for the server.
- `camello check --group`: one diagnostic per subject per file, with a count.
- `camello check --returns-drift`: the subs whose written `Returns:` and whose
  body disagree (`docs/return-inference.md`).
- `--min-severity`, and `min-severity` in `camello.toml`: print nothing below a
  severity, and do not exit 1 for it.
- `guard-classes` and `read-as` in `camello.toml`: a project's own guard classes,
  and a module of its own that re-exports another's interface.
- `unused-parameter` (`info`), a code of its own for a parameter the body never
  reads.
- `missing-argument` (`error`): a call that omits a name the callee requires.
- `ignored-prototype` (`info`): a method call to a sub declared `()`, which perl
  does not apply the prototype to.
- `unknown-key` for the `Class::Accessor::Lite` constructor, at `warning`.
- `Class::Tiny` and `Class::Tiny::Antlers` are recognised (ANNOT-13), as are the
  `Class::Accessor::Lite` family, `Class::Accessor` with its two speed variants,
  and `use constant` in all three spellings.
- A lazy slot is typed by its builder (ANNOT-10f).
- A class method's `$class` is `ClassName['__PACKAGE__']` and resolves against
  the package's own MRO (INFER-9); a value built from it is a `Self` (INFER-9b).
- A list shape may be an alternation — `(Value, Undef) | (Undef, Error)` is the
  ok-or-error idiom and its slots stay correlated (ANNOT-7e).
- `is_assignable`, the set-inclusion relation beside the `compatible` the checker
  reports against. Nothing reports through it yet.
- Appendix A of `docs/types.md`: everything camello knows by name without
  reading it.

### Changed

- **`camello lint` and `camello typecheck` are one command, `camello check`.**
  Breaking: neither old name is accepted. `--disable` turns a code off, and
  `scripts/corpus-check` takes `--check`.
- `camello.toml` and the tree walk moved into `camello_sema::config` and
  `::workspace`, so the language server reads them under the same rules.
- `unused-variable`, `shadowed-variable` and `unknown-method` are `info` — the
  last unless every `use` in the class's linearisation was read — and
  `maybe-deref` is a `warning` (DIAG-2b, DIAG-3a, DIAG-7a, DIAG-14a).
- `return-mismatch` is always a `warning`: the other side is a comment perl does
  not enforce (ANNOT-7e).
- A diagnostic names the code it is about, not only the type (DIAG-0a).
- An in-place string operator — `s///`, `tr///`, `.=` — leaves a `Str` behind
  (INFER-3d).
- A `goto &NAME` is a tail call, and its answer is the target's.
- A quoted string in a type position is that string, not a class name (TYPE-3a).
- `grep` narrows the list it filters (NARROW-7).
- A condition is read as a tree rather than as a flat scan of the names in it, so
  `if (!$x) { $x->foo }` reports and `$x && $x->foo` no longer does.
- `--returns-drift` reads a `Bool` and an `Enum` loosely (TYPE-5c, TYPE-5e).

### Fixed

- `x=` lexes as one token, taken where the word `x` is; split, it made the whole
  file a parse error.
- The declaration cache's `FORMAT` is bumped: a dependency's bytes do not change
  when camello learns to read them.
- `@ISA` is read in all three of its spellings (METHOD-1a).
- A file's own declarations answer about its own body, instead of a first-wins
  global index a duplicate package could win.
- A package's framework is read from its own imports, not from the file's.
- A dereference names what it dereferences: `$c->{a}{b}` reports `$c`, then
  `$c->{a}`.
- A `Smart::Args` invocant is no longer mistaken for the first named key, and an
  `args my $class` is bound under its own name.
- A value held for its destructor is not an unused variable — the rule is a
  `DESTROY` in the class's linearisation, not a name list (DIAG-12d).
- `# returns:` is a `Returns:`; the keyword is matched without regard to case.
- A discarded slot is still a parameter (INFER-3b), and `@_` read from inside a
  string is `@_` (INFER-3c).
- The methods an attribute generates are callables, so their argument and arity
  are checked; a `predicate` gives back `Bool` and a `clearer` nothing worth
  claiming, and `coerce => 1` widens what goes in and not what comes out.
- A family's head takes everything in its family, so `[1]` is a `Ref`; `Defined`
  and `Value` rule something out instead of being tops beside `Any`.
- A `return` inside an anonymous sub is that sub's.
- A `sub new` with an empty body says nothing about what it constructs, and a
  hand-written one is a constructor only where its body `bless`es or borrows
  `SUPER::`'s.
- A bareword call to a builtin's name is the builtin, unless the name was
  imported.
- `keys` and `values` follow their context instead of being `Int` everywhere, and
  `scalar` reads its argument.
- Arithmetic on two integers is an integer.
- A `method` has the invocant perl gives it, so `$obj->f` is not one argument too
  many.
- The `class` feature's `field` declares its name.
- A name a module exports is a method of every package that `use`s it; a package
  that calls a code generator at file scope is opaque.
- A project's own type library stands behind the annotations that write it,
  `type Foo => as ...` and `intersection` are recognised, and the type DSL is
  gated on a family of imports rather than a list of exporters.
- A `+{ ... }` disambiguator is looked through.
- `optional => 0` in an `args` rule means required, and an `optional` one may be
  passed `undef`.
- `Class::Accessor::Typed` slots are mandatory unless they say otherwise, and
  `(new => 0)` keeps its answer in a file that also says `use Moose`.
- `undef` fits a `Bool` slot.
- The compatibility rules cover `Dict`, `Map`, `Enum` and `RegexpRef`;
  parameterless `Dict`, `Map` and `Tuple` are the unparameterised reference; and
  an `InstanceOf` of a class the run never read says nothing.
- `Class::Accessor::Lite::Lazy`'s `mk_lazy_accessors` and `mk_ro_lazy_accessors`
  read the hashref form.
- A call's arguments are typed once instead of twice; the second walk cost
  2^depth on a nested list operator.
- A `my $name =>` is a key the paren-less call's pair lookahead can see.

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
