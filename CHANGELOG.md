# Changelog

One line per change. The reasoning is in the commit it came from.

## Unreleased

### Added

- **`Class::Tiny`** is recognised (`docs/types.md`, ANNOT-13). `use
  Class::Tiny qw(name email), { size => sub { 0 } }` declares those four as
  read-write attributes — the names in the flat list and the keys of the
  hashref beside it — and the accessors they generate are methods of the
  package. Untyped, like the `Class::Accessor::Lite` family, and open the same
  way: the generated `new` blesses the hash it was handed, so an unknown key
  is a `warning` and nothing is required. Unlike that family the constructor
  is not opt-in — `use Class::Tiny` is itself what puts `Class::Tiny::Object`
  in `@ISA` — so a bare `use Class::Tiny;`, and a `use parent
  'Class::Tiny::Object'`, both leave a `new` behind. `Class::Tiny::Antlers`
  exports `has` and reads as Moose, types and all.

### Fixed

- The declaration cache's `FORMAT` is bumped with the recogniser above. A
  dependency's bytes do not change when camello learns to read them, so
  neither does the key over them, and a cached "no framework, no attributes,
  no `new`" would have outlived the release that fixed it: `Carton::Dist` is
  a `Class::Tiny` class, and a `.camello-cache/` written by the previous build
  went on reporting `unknown-method` on its own constructor.
- **`camello lsp`**, a language server over the machinery that was already
  here (`docs/lsp.md`). One binary, no separate executable to version-match: a
  client configures the command `camello lsp` and is done. It publishes the
  checker's diagnostics as you type (300 ms after the last keystroke, at once
  on save), answers hover with the inferred type or the sub's signature,
  completes methods after `->` from what the receiver's class actually
  declares, gives an outline and go-to-definition, and formats a whole file.
  A thin VS Code client is in `editors/vscode/`; eglot and nvim-lspconfig want
  nothing but the command, because everything configurable is the `[check]`
  table of `camello.toml` that `camello check` already reads.
- The checker runs on a **broken buffer**, which is the divergence from the
  CLI that makes the whole thing useful. `camello check` discards every sema
  diagnostic for a file that fails to parse — right for a batch tool, wrong
  for an editor, where the buffer is broken most of the time and the user
  still wants the real answers about the parts they are not touching. A
  diagnostic whose range meets the enclosing *statement* of an `ERROR` node or
  a parse error is dropped and everything else is published; statement
  granularity is where the parser's own recovery synchronises.
- Three additions to `camello-sema`, all on the output side — no analysis
  logic changed, only results that used to die in a local getting a way out:
  a **type side-table** (`flow::analyse_recording`) capturing `(range, type)`
  for every expression the pass types and the resolved class at every `->`;
  a **method surface** (`Program::methods_of`) answering "what is there" where
  `resolve_method_from` answers "is this one there"; and a **scope table**
  (`ScopeReport::bindings`, `::references`) exposing the resolution `scope.rs`
  has always performed and never handed out.
- `camello dev index` and `scripts/lsp-bar`: the corpus bars for the server.
  Over the 2,681 `.pm` below `@INC` the walk takes 0.94s cold and the
  `FileDecls`-only residency peaks at 217 MiB, and 200 decl-diff-clean edits
  to one of the corpus's larger files cost 17 ms each — against a 300 ms
  debounce.

### Changed

- **A quoted string in a type position is that string**, not a class name
  (`docs/types.md`, TYPE-3a). `# Returns: 'draft' | 'live'` is the `Enum` it
  reads as, and used to be two classes nothing declares — two `unknown-type`
  reports and a `return-mismatch` on the very `return 'draft'` the annotation
  was written for. A bareword still names a type, which is what leaves the
  quoted form free to name a value, and the quotes Type::Tiny puts around a
  name — `InstanceOf['Foo']`, `Dict['key' => Str]` — are still read as one.
  An `isa => 'Foo'` is untouched: a string constraint reaches the type parser
  with its quotes already off.
- **`return-mismatch` is always a `warning`** (ANNOT-7e). It was an `error`
  where both sides were written down — a literal against the annotation, a
  `return` of three values against a `Returns:` naming two. But the other side
  is a *comment*, which perl does not enforce and which the body may simply
  have outgrown; saying the two disagree is the job, deciding which of them is
  wrong is not, and neither should fail a build on its own.
- **`--returns-drift` reads a `Bool` and an `Enum` loosely.** There is no
  boolean literal and no enum literal in perl: `return 0` and `return 'draft'`
  are how they are handed back, and the walk reads an `Int` and a `Str` — so
  every `Returns: Bool` that was written down drifted, in the one case where
  the annotation is the only thing that can say what was meant (TYPE-5c,
  TYPE-5e). A reference under a `Bool`, and a body that hands back `undef`
  under an `Enum` that did not say `Maybe`, are drift as before.
- `camello.toml` is read by `camello_sema::config` rather than by the command
  line, and the tree walk and worker pool by `camello_sema::workspace`. The
  language server reads the same file under the same rules and walks a tree
  the same way, and it cannot reach into the binary that depends on it — two
  copies of either would have been two dialects of it.
- **`camello lint` and `camello typecheck` are one command, `camello check`.**
  Breaking: neither old name is accepted. The split was justified by speed —
  the type lattice needs the dependency resolver behind it, and `lint` was to
  be what runs where `perlcritic` runs — and measurements showed that both
  commands were comfortably below the threshold at which a user would choose
  between them.
  The two took an identical argument set in which four flags did nothing under
  `lint`, and `lint`'s output was a strict subset of `typecheck`'s. `--disable`
  is how a code is turned off now. `scripts/corpus-check` takes `--check` in
  place of `--lint` and `--typecheck`.

### Added

- **A class method's `$class` is `ClassName['__PACKAGE__']`** (`docs/types.md`,
  INFER-9). `$self` has always been bound to an `InstanceOf` of the package it
  is written in; `$class` was bound to a bare `ClassName`, which names no class
  at all — so `$class->` resolved to nothing, `my $self = $class->new` was
  `Unknown`, and the body of every hand-written constructor and class method
  was invisible. It now resolves against the package's own MRO, on the
  assumption the calling convention makes: `$class` is this package or a
  subclass. **A method the package does not declare is `unknown-method`** —
  the same call pyright and mypy make for a classmethod's `cls`, and the same
  cost: a base class calling a method only its subclasses define is a false
  positive. Over the 2564 `.pm` files below `@INC` that cost was one call
  (`TheSchwartz::Worker`'s `$class->work`). `ClassName['Foo']` is writable as
  an annotation too; bare `ClassName` still means "some class's name", and the
  parameterised one is a subtype of it.
- **A value built from `$class` is a `Self`** (INFER-9b). The invocant rule
  (INFER-4f) said a sub returning `$self` returns the class it was *called* on;
  this extends it to a value built from the invocant's class. `$class->new` and
  `$self->clone` carry the marker, so `sub build { my $class = shift; return
  $class->new }` makes `Child->build` a `Child`. The marker crosses one
  lexical, because `my $self = $class->new; ...; return $self` is the shape
  every hand-written constructor has and an expression-only marker is lost at
  the assignment; another assignment to the same lexical takes it away again.
  A value that is not of the receiver's class — `$class->config` handing back
  a `Config` — is not one of these.
- **A lazy slot is typed by its builder** (`docs/types.md`, ANNOT-10f). The
  `Class::Accessor::Lite` family carries no types, and that was the whole
  answer for `Class::Accessor::Lite::Lazy` too — but a lazy accessor is
  `$self->{name} //= $self->$builder`, so what the builder returns *is* what
  the accessor hands back, and it is the one type source this family has.
  `ro_lazy`, `rw_lazy`, `mk_lazy_accessors` and `mk_ro_lazy_accessors` all get
  it, under whichever name the builder was given: the default `_build_$name`,
  or the `'make_it'` or `\&make_it` written beside the slot. An inline `sub {
  ... }` names nothing to look up and stays `Unknown`. The builder is reached
  as a method, so a subclass that overrides it builds the slot; and it is
  asked at the call, not at the declaration, because the builder's own return
  is inferred and the incremental loop re-derives it. Not a guarantee the way
  an `isa` is — this family's `new` is open (INFER-2g) — but the same
  pragmatism that reads `Foo->new` as an `InstanceOf['Foo']`.

### Changed

- **`unused-variable` and `shadowed-variable` are `info`** (DIAG-2b, DIAG-3a).
  Both are noisy out of proportion to what they catch. A name bound and never
  read is usually deliberate — a destructuring that wanted one of three slots,
  a `my` kept for a later edit — and where it is not, it costs nothing;
  shadowing is legal and a matter of taste. Over the `.pm` files below `@INC`
  they are 1261 and 340 reports.
- **A diagnostic names the code it is about, not only the type** (DIAG-0a).
  `` `Str|Undef` may be undefined here `` tells a reader nothing they can look
  for. `maybe-deref`, `type-mismatch` and `return-mismatch` put the subject
  first and the type in parentheses:

  ```text
  `$row` may be undefined here (`InstanceOf['Row']|Undef`), and nothing has checked it
  ```

  The subject is not always a variable: `$obj->rows` and `$self->{cfg}{db}` are
  subjects too, and for the second step of `$h->{a}{b}` it is `$h->{a}`, which
  is the value that may be `undef` and the thing to guard. It is the text as
  *written* with its whitespace squeezed to single spaces, not the tokens
  concatenated — `[keys %$map]` read back without the space is a different
  program.
- **`grep` narrows the list it filters** (NARROW-7). It hands back the elements
  whose condition held, so the block — or the expression of `grep EXPR, LIST` —
  is read as a narrowing of `$_`, and what survives it is the element type.
  `grep { $_ }` over an `InstanceOf['Row'] | Undef` is an `InstanceOf['Row']`,
  and so are `grep { defined $_ }`, `grep { defined }` and `grep $_, LIST`. The
  existing narrowing list is what applies, so a condition that says nothing
  about `$_` narrows nothing, and a block of several statements is not read at
  all — taking only its last would claim the earlier ones said nothing.
- **A list shape may be an alternation** (`docs/types.md`, ANNOT-7e).
  `(Value, Undef) | (Undef, Error)` is the ok-or-error idiom, and its two slots
  are correlated — one is filled exactly when the other is `undef`. Joining the
  two `return`s slot-wise into `(Value | Undef, Error | Undef)` threw away the
  only thing the shape says, so they are kept apart instead, in the inference
  and in the notation. A `return` passes when it matches any one alternative; a
  length that disagrees is still an `error`, because every alternative has the
  same width and so the count is written on both sides. Only where *every*
  part is parenthesised end to end — `Str | Undef` is a scalar union and its
  `|` belongs to the type language. A one-slot alternation has nothing to be
  correlated with and collapses to the union; so does anything past three
  alternatives, where the sub has stopped describing a choice. A *binding*
  still reads slot-wise: `my ($value, $error) = f()` binds `Value | Undef` and
  `Undef | Error`, because nothing carries the correlation past the assignment.
- **Hover answers `Unknown` rather than nothing.** Silence is the right answer
  to "there is nothing here" and the wrong one to "there is something here and
  I do not know what it is" — the two look identical to a reader, and the
  second is the common case. A lexical, a bareword call, a `sub` name and a
  `->` method now all answer, with `Unknown` where that is the answer; only a
  position that names nothing at all stays silent. A method whose receiver has
  no class is one of the names that answers, so `$thing->whatever` on an
  untyped `$thing` says `whatever -> Unknown` instead of nothing.
- **`unknown-method` is an `info` unless the world is closed, and `maybe-deref`
  is a `warning`** (`docs/types.md`, DIAG-7a, DIAG-14a). "This class declares no
  such method" is a claim about a closed world, and the world is closed only
  where every module the class and its ancestors `use` was actually read — a
  module installs subs into its importer and may assign to its globs, so an
  unresolved `use` is a hole in the method surface even when every ancestor is
  known. A framework recognised by name is not a hole: the checker knows what
  `use Moose` installs better than reading `Moose.pm` would tell it. Going the
  other way, `maybe-deref` says something specific about a specific program and
  does not rest on what the run failed to read, so it is now a `warning`.

### Fixed

- **A dereference names what it dereferences.** A subscript step's own node
  spans the base *and every step up to itself*, so reading "the text before
  this step" off its range gave the empty string: every `maybe-deref` that was
  not a method call reported `` `` `` for its subject. The part before a step
  is that step node's first child, so `$c->{a}{b}` reports `$c` and then
  `$c->{a}`. Thirty-seven messages over `@INC` were affected.
- **A `Smart::Args` invocant is no longer mistaken for the first named key.**
  `Params::Named` recorded only that there *was* an invocant, and both the
  signature renderer and the body pass then went looking for it in the list of
  keys — so `args my $class, my $foo => ..., my $bar => ...` rendered as
  `foo($foo? : Str, { bar => Int })`, with two keys of the same list on
  opposite sides of the hash. The variant carries the invocant's *name* now,
  which also fixes the body: `bind` was binding `$self` unconditionally, so an
  `args` sub whose invocant is `$class` had `$class` unbound and a phantom
  `$self` in its place. Named parameters are shown with whether they have to be
  passed, so the same sub reads `foo($class, { foo? => Str, bar? => Int })`.
- **A value held for its destructor is not an unused variable** (DIAG-12d).
  The rule was a list of names — `Scope::Guard`, `guard`, and whatever
  `camello.toml` added. What actually says so is the *class*: an instance of
  one declaring `DESTROY` anywhere in its linearisation is bound so that the
  destructor runs, whatever produced it, so `my $lock = make_lock();` gets the
  same answer as `my $lock = Lock->new;`. The name list stays for where the
  type does not arrive and gains `Scope::Container` and
  `start_scope_container`.
- **`# returns:` is a `Returns:`.** The keyword is matched without regard to
  case now.

### Added (tools)

- **`camello check --group`** reports one diagnostic per subject per file with
  a count. Twenty dereferences of one `$row` are one thing to fix.
- **`camello check --returns-drift`** lists the subs whose written `Returns:`
  and whose body disagree (`docs/return-inference.md`, "Drift"). An annotation
  wins at every call site, so the only thing that ever compares it against the
  code is `return-mismatch`, one `return` at a time — which cannot see the
  drift a file collects. Nothing is installed and the program is not changed:
  each sub's body is read against the annotations every *other* sub still
  carries. Exit 1 when anything was found.
- **Appendix A of `docs/types.md`** is the list of everything camello knows by
  name without reading it: the object frameworks, the `mk_*` family, the type
  DSL, the `use` statements the declaration pass reads itself, XS loaders, the
  modules that turn `strict` on, the guard names, `UNIVERSAL`, perl's own
  variables, and the core modules that export a variable.
- **`@ISA` is read in all three of its spellings** (METHOD-1a). `our @ISA =
  ('Base')` worked; `@ISA = qw(Base Other)` was read as a value the pass could
  not understand, which made the whole class *dynamic* and silenced every
  diagnostic about it; and the fully qualified `@Foo::ISA = qw(Base)` was not
  recognised as an `@ISA` at all. `CPAN::FTP` writes `@CPAN::FTP::ISA =
  qw(CPAN::Debug)` and lost its parent to both bugs at once. The qualified form
  names its own package, which need not be the one the statement sits in, and
  `@EXPORT` follows the same rule. Over the 2564 `.pm` files below `@INC` this
  removes 130 `unknown-method` reports.
- **A discarded slot is still a parameter** (INFER-3b). `my (undef, $name) =
  @_` takes the invocant and throws it away, and reading only the names moved
  `$name` into the invocant's place and lost an argument —
  `File::DesktopEntry::lookup` is written this way. The slot is recorded under
  a name with no sigil, which is what keeps it out of the body's environment.
- **`@_` read from inside a string is `@_`** (INFER-3c). `sub HeaderError { my
  ($self) = shift; ... "Header Error: $_[0]" }` takes an argument its names
  never mention, but the lexer hands the string over as one token so the
  subscript was invisible and the sub was read as taking none. The
  interpolation scanner is asked now. `IO::Uncompress::Base` is where this came
  from and it cost 23 false `arity` reports.
- `Class::Accessor::Lite::Lazy->mk_lazy_accessors` and `mk_ro_lazy_accessors`
  read the hashref form. The two lazy makers flatten a hashref into
  name-and-builder pairs exactly as the `use` statement's `rw_lazy`/`ro_lazy`
  do, so `mk_lazy_accessors('foo', { bar => \&build })` declares `bar` too —
  and it was being read as a list of plain names, which named none, so every
  call to such an accessor was an `unknown-method`. Only the lazy makers read
  it: a reference handed to plain `mk_accessors` is stringified into an
  accessor nobody asked for.
- A file's own declarations answer about its own body. The parameter list a
  body starts from was fetched from one global `(package, sub)` index whose
  rule is first-wins, so a second copy of a package anywhere in the workspace
  — a vendored dependency, an old checkout left in the tree — decided what the
  file being read was typed from, and it sorts before `lib/` often enough to
  win. In the editor the symptom was a type that arrived and then left: hover
  read the annotation off the buffer while single-file analysis answered, and
  said `Any` from the moment the index finished. `Program::sub_in` asks the
  file in hand first and falls back to the global index, so it differs only
  where a duplicate exists — which is where the global answer was a guess
  between two files. The body pass, a bareword call's resolution (and so
  go-to-definition on one) and hover over a `sub` name all ask that way now.
- A family's head takes everything in its family. `Ref` was equal to itself
  and to nothing else, so `[1]`, `{ a => 1 }`, `sub {}` and `qr//` all failed
  to be one — every reference in the language was a `type-mismatch` against a
  `Ref` slot. `Object`, `Defined`, `Value` and `GlobRef` head families too,
  and reading them in reverse is sound for the ones that say what *kind* of
  thing a value is: a value known only as a `Ref` could be an `ArrayRef`, so
  nothing is ruled out either way. The stringification chain is left directed,
  because a literal that looked like a number is already an `Int` and what is
  left in `Str` is a string that is not one.
- The methods an attribute generates are **callables**, not a type. A `has`
  slot answered every name it owns with the attribute's type and nothing else,
  so `$obj->set_count([1, 2])` against an `isa => 'Int'` had nothing to be
  compared with — while the same sub written by hand was checked. Each
  generated method now carries a parameter list: an accessor, a `reader`, a
  `predicate` and a `clearer` take the invocant, a `writer` and a `wo`
  accessor take the value, and an `rw` accessor may be read. So the value's
  type is checked, the count is checked, writing to a `ro` slot and reading a
  `wo` one are visible, and `Access` reaches the checker for the first time.
  At `warning`: the shape is the framework's rather than the author's, and
  Moose's reader ignores an argument it did not want where
  `Class::Accessor::Lite`'s croaks.
- `coerce => 1` widens what goes *in* and not what comes out. The declared
  type is the ceiling on the coerced value and the coercion is a function
  nobody here read — but the whole slot was being blanked to `Unknown`, so the
  reader's type went with it.
- A condition is read as a **tree** rather than as a flat scan of the names in
  it. `if (!$x) { $x->foo }` — a program that dies every time it takes the
  branch — was silent, because the old reading narrowed any variable that
  appeared anywhere in the condition. So were `if ($x || $y)`, where only `$y`
  may have held, and `if (validate($x))`, whose body this pass never opened.
  Falling out of the same change: `$x && $x->foo` and `!$x || $x->foo` are no
  longer `maybe-deref` on themselves, because perl short-circuits and the
  right side is now read under what the left said; an `elsif` narrows its own
  block; `unless (COND) {...} else {...}` narrows the `else`; `return ... if
  !$x` narrows what follows it; and a guard no longer narrows names it never
  tested — `return $a // $default unless $b` said nothing about `$a` and was
  taken to. The fixtures cover both the removed false positives and the
  `validate($x)` shape that remains deliberately unknown.
- A package's framework is read from *its own* imports, not from the file's.
  `use Moose` imports `has` into the package it is written in, so a second
  package in the same file was being handed Moose's `has`, its attributes and
  its `new` — and `Plain->new(...)` against a package that never said `use
  Moose` was an `unknown-key` **error**. The unit is now the package, scoped
  the way perl scopes the import.
- A `predicate` gives back `Bool` and a `clearer` gives back nothing worth
  claiming. Every name an attribute answered to used to give back the
  attribute's own type, so `$obj->has_items` against an `ArrayRef[Int]` slot
  was an `ArrayRef[Int]` and a `type-mismatch` in every string slot it went
  into. A `handles` delegation is another class's method and is `Unknown`.
- A `return` inside an anonymous sub is that sub's. The `Returns:` of the sub
  it is *written in* stayed in scope while the body was walked, so `sub f { my
  $cb = sub { return [1] }; ... }` reported the callback's `return` against
  `f`'s declared type.
- `Defined` and `Value` rule something out. Both were read as tops beside
  `Any`, so `undef` fitted a `Defined` slot and an `ArrayRef` fitted a `Value`
  one — the one thing each of them says went unsaid.
- A `sub new` with an *empty* body says nothing about what it constructs,
  the way a forward declaration does. A stub writes `sub new ($class, $fields
  = undef) {}`, and reading that as "no evidence" took the type off every
  class that inherits its `new`.

### Added

- `is_assignable`, the set-inclusion relation, beside the `compatible` the
  checker reports against. Nothing reports through it yet: using assignability
  as the reporting relation would report values such as `Bool` and `Enum` slots
  that TYPE-5c deliberately does not follow. It exists to be the foundation of a
  stricter reading, and to hold `compatible` to account: `assignable ⇒
  compatible` is a test, and it is what found two asymmetries in `compatible`
  (`Defined` against `Undef`, and `Bool` against `Str`).
- `ignored-prototype`, at `info`: a method call to a sub declared `()`.
  perlsub says a method call is not influenced by a prototype, and a bare `()`
  is a prototype where the signatures feature is off and a signature where it
  is on — so `$self->duration_class` against `sub duration_class ()` was an
  `arity` error and should not have been. Which of the two it was decides
  whether the call is harmless or fatal, and saying which would be a guess, so
  it is reported once per call and left at that. A bareword call still gets
  `arity`: perl really does apply the prototype there.

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
  package no longer takes `delete $h->{k}` away from perl's own `delete`. The
  builtin-call fixture covers this shape. An import still answers, because
  importing the name is the one mechanism perlsub gives for overriding a
  builtin.
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
- The compatibility rules cover the structured types they were missing. Two
  `Dict`s are compared slot by slot, `Map` and `HashRef` are the same reference
  with the key side said or unsaid, an `Enum` goes where a `Str` goes and fits
  another `Enum` whose values include its own, and `RegexpRef` and
  `InstanceOf['Regexp']` are one type. Every one of them used to fall through
  to "not the same type": a hash written out was a `type-mismatch` against the
  `Dict` it was written for, because an inferred hash always carries a slurpy
  and the declared one does not.
- A reference passed to an `InstanceOf` of a class the run never read says
  nothing, the way two unknown classes already did. A structured type out of a
  type library that could not be read arrives as exactly that (TYPE-3), and
  "not that shape" is not something a declaration nobody read can support.
- Parameterless `Dict`, `Map` and `Tuple` are the unparameterised reference —
  `HashRef` and `ArrayRef` — rather than the empty structure. `args my $x =>
  'Dict'` accepts any hash in Type::Tiny, and reading it as a `Dict` with no
  slots made every key read off it an `unknown-key`.
- A name a module exports is a method of every package that `use`s it.
  `use Exporter 'import'; our @EXPORT = qw(render)` installs `render` in the
  importer, and `$self->render` there was an `unknown-method`. An `@EXPORT`
  whose value cannot be read — `our @EXPORT = get_public_functions;` — makes
  the importer a package whose methods nobody can enumerate, and a package
  that exports its own subs is a mixin, so `$self` inside one of them is
  whichever class imported it rather than this one.
- A package that calls a code generator at file scope is opaque. A file that
  assigns to globs, or that calls an accessor maker whose list of names is a
  variable, makes methods nobody here can name — and it makes them in the
  package that *called* it, whose own file says nothing about them.
  `__PACKAGE__->mk_field_accessors` and `Some::Util->ro_datetime([...])` are
  the two spellings covered by the code-generation fixtures.
- `maybe-deref` is `info` rather than `warning`. Every subscript is a `Maybe`
  by construction, so this is about an idiom rather than about a program, and
  the narrowing list it is checked against is a list rather than a theorem.
- A hand-written `sub new` is read as a constructor only where its body says
  the value it hands back is one of its own class — a `bless`, or a `SUPER::`
  that borrows the parent's. `URI->new` returns a `URI::http`, and calling it
  a `URI` made methods after it appear missing; the constructor fixture records
  the distinction.
- The `class` feature's `field` declares its name. The parser was already
  building the declaration; the scope pass did not read the keyword, so every
  field reference in a class under `strict` was an `undeclared-variable`. A
  `field` with an attribute is left out of `unused-variable`, because `:param`
  and `:reader` hand the name to something outside the body.
- Arithmetic on two integers is an integer. `+`, `-` and `*` were all `Num`,
  so `$limit + 1` for an `Int` slot was a `type-mismatch`; `%` truncates both
  sides and is an `Int` whatever it was handed, and `/` and `**` stay `Num`.
  An operand nobody typed now leaves the answer untyped rather than claiming
  `Num` — `Int` slots are what that claim would be reported against.
- A `my $name =>` is a key the paren-less call's pair lookahead can see. `args
  my $a => ArrayRef[Str], my $b => 'Int'` handed the whole rest of the list to
  `ArrayRef`, so every rule written after the first parameterised type was
  lost — types, `optional`, `default` and the arity with them.

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
