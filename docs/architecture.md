# Camello architecture

Camello is a Rust library and command-line formatter for modern Perl. It aims
to handle real-world code without claiming complete compatibility with every
legacy Perl grammar feature. The implementation is lossless: source text is
first represented as tokens and a concrete syntax tree (CST), and formatting
changes trivia and layout rather than program tokens.

Camello is also a static checker. `camello check` runs over the same CST and
reads the type annotations Perl code already carries —
`has ... isa => 'Str'`, `args my $x => 'Int'`, `Class::Accessor::Typed`, the
`mk_accessors` family, `use constant`, and a `Returns:` comment — as
declarations rather than as strings. What it concludes
from them is specified in [types.md](types.md); the design behind it, and the
decisions the corpus forced on it, are in [typecheck.md](typecheck.md).

The formatting rules themselves are documented in [formatting.md](formatting.md).
The rules the source holds to — the five contracts its comments refer to by
name, and the invariants numbered in them — are in [contracts.md](contracts.md).

## The crates

```text
crates/camello-syntax   lang, lex, parse, ast   the front end
crates/camello-fmt      the Doc IR              the formatter
crates/camello-sema     symbols, types, flow    the checker
crates/camello-lsp      documents, index, …     the language server
camello (root)          cli, check              the binary and the invariants
```

The split is enforced by Cargo rather than by review, and the direction it
enforces is the one that matters: **nothing under `sema` can reach `fmt`**. The
lossless-CST and trivia machinery the formatter is built around is irrelevant
to a checker, and a build of the checker should not carry the Doc IR.

`camello-lsp` is the one crate that sees both, which is what an editor front
end is: diagnostics come from the checker and `textDocument/formatting` from
the formatter. It sits above both, the same way the root crate does, and the
rule above is untouched by it.

`camello-syntax` is what they all sit on. `camello_syntax::lang` defines the shared
vocabulary, `lex` scans source, `parse` records and replays syntax events, and
`ast` offers typed views over the result. `camello-fmt` builds and renders the
output. `camello-sema` reads declarations and checks bodies, and holds two
things both of its readers need: `config`, the `camello.toml` `[check]` table,
and `workspace`, the tree walk and worker pool a declaration pass runs
through. `camello-lsp` is the language server, described in
[lsp.md](lsp.md). The root crate is the command line, plus `src/check`, which
holds the invariants — it is the one place that compares a formatter against a
parser and so the one place that depends on both.

## Data flow

```text
Perl source
  -> lexer token buffer
  -> parser events
  -> rowan CST + TriviaMap
  -> formatter Doc IR          -> rendered lines -> alignment -> format skipping -> formatted source
  -> ast views -> declarations -> program graph  -> body pass  -> diagnostics
                                                              -> type side-table -> hover, completion
```

The last line is the language server's, and it is the same body pass: the
checker infers a type for every expression it visits, and `camello lsp` is a
caller that asks it to keep them rather than drop them.

## Guesses

perl answers some questions from its symbol table or at run time, and camello
has neither. Where a reading is settled on weak evidence rather than on what the
grammar already knows, the comment carries a `GUESS:` label:

```rust
// GUESS: `f / 10` divides rather than starting a match.
// Evidence: none — no declaration in sight. Perl guesses here too.
// Wrong: the match runs to the next `/`, taking whatever lies between.
parser.expect_operator();
```

All three lines are required. `Evidence:` names what the reading rests on — the
spacing, the capitals, a newline in the source, or nothing at all; `Evidence:
none` is the honest and common answer, and it is what marks the places worth
re-reading first. `Wrong:` says what the output becomes when the guess goes the
other way. For the formatter's guesses that is always the shape and never the
meaning, and saying so is what separates the cheap ones from the expensive ones.

The label is narrow on purpose, because a label on every judgement indexes
nothing. It goes on a decision only when all three hold:

- perl would answer it from its symbol table or at run time, and camello cannot;
- the reading rests on weak evidence, not on what the grammar or the lexer
  expectation already knows;
- getting it wrong changes the output.

The contracts ([contracts.md](contracts.md)) are the other half of this: what is
decided by rule lives there, and what is decided on thin evidence carries the
label.

So `.5` is no guess — the expectation state says whether a number or a
concatenation is due. `&&` is not a sigil under any reading, not merely under
this one. And `{` opening an anonymous hash is not a guess either: it is parsed
as a hash and rolled back if that does not work out, which is evidence rather
than inference. `grep -rn 'GUESS:' src` is the list.

## The AST layer

`camello_syntax::ast` is a typed view per `NodeKind`: `AstNode::cast(SyntaxNode)
-> Option<Self>`, and accessors that return other views or tokens. The
`cast`/`syntax` boilerplate is generated from the `nodes` section of
`define_language!` — the section carries a view name beside each kind — and the
accessors are hand-written beside it, the way `predicates.rs` is hand-written
beside the generated enums.

The formatter does not use it: the formatter's questions are about tokens and
trivia, and it walks kinds directly. The checker's questions are about
structure, and asking them by kind-matching over child iterators in every pass
is how a checker becomes unmaintainable. `camello dev dump` prints the view name
beside every node kind, which is the question a reader of a dump has.

## Language vocabulary and CST

The `define_language!` invocation in `crates/camello-syntax/src/lang/mod.rs` is
the source of truth for `TokenKind`, `NodeKind`, conversions through
`SyntaxKind`, the `T![...]` macro, keyword lookup, token classification, and
diagnostic display names. Tokens and nodes occupy disjoint ranges of the rowan
syntax-kind space.

Only `crates/camello-syntax/src/parse/replay.rs` writes to `GreenNodeBuilder`.
Its wrapper accepts a `TokenKind` for tokens and a `NodeKind` for nodes,
preventing the two categories from being mixed while building the CST.

The CST is concrete and lossless: concatenating its token texts reproduces the
input. Non-root node ranges begin and end on non-trivia tokens. Error recovery
may create `ERROR` nodes, but it does not discard source text.

## Lexer

The lexer in `crates/camello-syntax/src/lex` is hand-written and
parser-directed. Perl tokens such as `/` and `%` are ambiguous without knowing
whether the grammar expects a term or an operator, so that expectation is stored
as lexer state. `peek` and `bump` therefore observe the same context. Changing
the expectation invalidates buffered lookahead from the current cursor and
rescans it.

Quote-like operators, heredoc bodies, POD, formats, and data sections are
scanned as complete token runs in `crates/camello-syntax/src/lex/atomic.rs`. No
partially open lexical mode escapes a scanning call. Scanner failures are
represented by error tokens and diagnostics rather than silently dropping
input.

The token buffer also makes speculative parsing cheap: a checkpoint records the
cursor and expectation, and rollback restores them without rescanning the
already accepted prefix.

## Parser and error recovery

The parser in `crates/camello-syntax/src/parse` emits `Start`, `Token`,
`Finish`, and `Error` events. Grammar code does not write rowan nodes directly.
After parsing, the replay pass combines events with the full token stream to
build the green tree and the `TriviaMap`.

Grammar rules are split across:

- `grammar/mod.rs` for files, declarations, statements, and blocks;
- `grammar/expr.rs` and `grammar/primary.rs` for expressions and terms;
- `grammar/precedence.rs` for operator binding powers;
- `grammar/builtins.rs` for builtin argument shapes and lexer expectations.

Rules can use `checkpoint()` and `rollback()` when syntax is ambiguous. Names
that are lexed as keywords are consumed through the common name-coercion path
when the grammar permits them as identifiers.

Diagnostics do not consume input by themselves. Recovery is explicit and uses
synchronization tokens such as statement boundaries, closing delimiters, and
list separators. Skipped tokens are grouped in an `ERROR` node. The parser also
has progress and nesting limits so malformed or generated input produces a
diagnostic instead of hanging or overflowing the downstream formatter.

## Trivia

Whitespace, newlines, and comments remain in the lexer stream, while parser
events refer to non-trivia tokens. During replay, trivia is emitted outside
syntax-node boundaries and attached to adjacent tokens in `TriviaMap`.

A trivia run between two tokens is split at its first newline, including that
newline. The first portion trails the preceding token; the remainder leads the
following token. Leading file trivia and trailing file trivia are represented
explicitly. Zero-width tokens do not own trivia.

The formatter reads comments and blank-line information from `TriviaMap`; it
does not rediscover them by rescanning the CST or source.

## Formatter

Formatting has four phases, all under `crates/camello-fmt/src`:

1. `build.rs` turns the CST and trivia into `Doc`, making layout and spacing
   decisions.
2. `render.rs` applies indentation and spacing while rendering the document into
   lines.
3. `align.rs` performs vertical alignment over rendered columns.
4. `skip.rs` puts the source's own lines back over the regions perltidy's
   `#<<<` / `#>>>` markers cover.

Explicit `Doc::Space` values control spacing; the renderer does not infer
spaces between arbitrary tokens. Layout groups retain relevant source-newline
decisions so a formatted result is a fixed point on the next pass.

String contents, heredoc bodies, POD, formats, and data sections use raw or
verbatim document atoms. The renderer cannot insert indentation or other text
inside those atoms. Alignment is independent of parsing and only sees rendered
lines and anchors. Format skipping is later still and sees only lines: a marked
region is a run of *lines* the writer settled, so what it replaces is the
formatter's whole answer for them.

`FormatterOptions` currently controls indentation width, minimum spacing before
trailing comments, delimiter spacing, single-line blocks, and the maximum
padding inserted by vertical alignment. Defaults are 4, 4, `Standard`, enabled,
and 64 respectively.

## Public interfaces

The library exports:

- `parse_perl` and `parse_perl_with_trivia` for lossless parsing;
- `format_perl` and `format_perl_with_options` for formatting;
- syntax kinds, syntax nodes and tokens, parse diagnostics, trivia, and
  formatter option types.

`camello lsp` speaks the Language Server Protocol over standard input and
output, so an editor gets the checker's diagnostics as it types, the inferred
type or a sub's signature on hover, the methods a receiver's class declares
after `->`, an outline, go-to-definition, and whole-file formatting. It reads
the same `camello.toml` `[check]` table `camello check` reads. A thin VS Code
client is in `editors/vscode/`; every other editor wants nothing but the
command. The design, and what it deliberately does not do, is in
[lsp.md](lsp.md).

The stable command-line surface is `camello format`. It reads its sources from
paths, `-e`/`-E`, or standard input, and writes each one back over the file it
came from — a file, a directory, or several of either. Directory traversal is
recursive and does not follow symlinks discovered below a requested root. Work
across files is parallelized, but reports remain in input order.

Writing it back is the default because formatting a file is a thing done to the
file, and a tree of them has nowhere else to go. A source with no file behind it
— standard input, `-e` — goes to standard output instead, and `--output` is how
to ask for that of a file too: it names somewhere else to put the result, or `-`
for standard output, for one source at a time. `--write` asks for the default
and does nothing; it stays because it is what the hand types.

Reporting is quiet by default. Formatting one source writes it and says
nothing; `--check` says nothing when the source is already formatted and
otherwise prints its name and exits 1. Over a tree, a run ends in one summary
line, and the files it reformatted are named only under `--check` — which is a
question about which files those are — or under `--list-different`. A file that
could not be read, or that the parser had something to say about and was
therefore left alone, is always named: `--list-different` additionally prints
its diagnostics, which are in any case what `camello format <that file>` shows.

Left alone means left alone however the source was handed over. One path is how
an editor's save hook and a pre-commit hook both ask, so it is the path a
best-effort rewrite of an unparsed file would actually take, and it used to take
it: `camello format one.pl` reported the diagnostics, rewrote the file anyway
and exited 0. It now reports them, writes nothing and exits 1. A source with no
file behind it comes back out as it went in — standard input echoed, `-o -`
echoed — because that is what leaving it alone means when the result was going
to standard output; under `--check` nothing is written at all.

`--encoding` names the encodings a source may be in, in the order they are
tried: the first that decodes a file's bytes without replacing any of them is
the one it is read as, and the one it is written back in. Bytes that no
candidate can read are refused rather than decoded with replacement characters,
so a file in an encoding nobody named is reported and left alone rather than
rewritten. One candidate — utf-8, unless another is named — is the usual case;
several are for a tree whose files are not all in the same encoding, where the
answer belongs to each file rather than to the run. In-place output is encoded
before the original is touched, written and synchronized to a temporary sibling,
and atomically renamed over the target while preserving its permissions.

## The checker

`camello check lib t` reports what the scopes say — undeclared, unused and
shadowed lexicals, and arity against a signature, an `args` list or an `@_`
unpacking — and what the annotations add: a `Str` passed where an
`ArrayRef[Int]` was declared, a method called on a class that declares none
such, a key read off a `Dict` that has no such key, a `Maybe[...]` used with
nothing having checked it. `--disable` leaves any of them unreported.

It prints one diagnostic per line as `path:line:col: severity: message [code]`,
takes `--format json` for tooling, and exits 1 when anything at or above
`--error-on` (default `error`) was reported. It takes `--stubs`, `--inc`,
`--cache-dir` / `--no-cache`, and reads `camello.toml` from the directory it is
run in.

The analysis is two passes, and the split is what makes it cheap. The
**declaration pass** reads no sub body — a body can only *use* a declaration,
never make one another file could see — so the program graph is complete after
it, a dependency contributes its declarations and nothing else, and the result
is cached on disk. The **body pass** is per sub and reads only symbols.

Three rules keep it quiet, and each is the reason a class of false positive is
not there:

- `Unknown` is a top that means "not analysed". It propagates through every
  operation and is never reported against, so a program with no annotations
  gets no type diagnostics at all.
- an element names its container: `$h{k}` reads `%h` and `$a[0]` reads `@a`.
- a class with an unknown ancestor — including one whose file loads XS, whose
  `@ISA` is computed, or which assigns a glob — might have any method, so "no
  such method" is never said of it.

What the checker knows and what it deliberately leaves unknown is specified in
[types.md](types.md), which is to the checker what `formatting.md` is to the
formatter. `docs/typecheck.md` is the design behind it, and carries the
decisions the corpus forced along with where each milestone's corpus bar
landed.

The hidden `camello dev` namespace contains development interfaces. `dump`
prints a CST, and `check` evaluates parser and formatter invariants on files,
directory trees, or standard input. `perl-deparse` takes the same paths and asks the
one question `check` cannot: whether perl reads the output as the program the
input was. It is a command of its own because asking runs perl over the file,
and `perl -c` executes that file's `BEGIN` blocks. These commands may change
independently of the formatter interface.

The layout flags on `format` — `--indent-width` and its four siblings — are
hidden for the same reason. What camello answers is how Perl is written, and a
formatter that answers it five ways depending on its flags has not answered it;
they exist so that a question about the layout can be *asked* — of a fixture, in
a bug report — and they may change with the layout they describe.

## Validation

`src/check` defines six checks in two groups. The group is what a violation
answers first: a parser check is asked of the input alone, so a failure is the
parser's; a formatter check compares an input against its output.

The parser, asked of the input:

- `clean-parse` — parsing reports no diagnostic;
- `normal-form` — the tree's tokens reproduce the source byte for byte, and no
  node's range begins or ends on trivia.

The formatter, asked of input against output:

- `semantics` — the non-trivia token sequence is unchanged;
- `comments` — the comment texts are unchanged, in order;
- `verbatim` — verbatim content is reproduced byte for byte and is present in
  the input;
- `idempotency` — the output is a fixed point of the formatter, and the layout
  seeds read back out of it are the ones the input gave. Seeds and text are one
  check because the seeds are the cause and the text the symptom: camello breaks
  a group from a newline in the input, never from a line's length, so seeds that
  move while the text holds still are a shape that will move on a later edit.
- `perl-deparse` — perl reads the output as the program the input was: both compile
  under `perl -c`, and `B::Deparse` renders them the same. Asked by `dev
  perl-deparse` and by nothing else — not by `dev check`, and not reachable from its
  `--only` — because it is the only check that runs another program: `perl -c`
  executes the `BEGIN` blocks of the file it is pointed at. Opting in is the
  command typed, so no run of `check` can carry it along. It is also the only one that sees what a token stream cannot —
  `${^MATCH}` against `${^ MATCH}` is one token sequence and two variables. The
  deparsed output is normalised before comparison: forward declarations are
  dropped, inlinable constant stubs sorted, stringified addresses masked, since
  `B::Deparse` emits those from a hash walk and their order is not stable across
  runs of the same file. Both texts are put at one temporary path in turn rather
  than at two paths at once, because perl's answer carries the path it read —
  and because that leaves perl running in camello's own working directory,
  where a relative `PERL5LIB` or a `use lib 'lib'` means what the caller meant
  by it.

`Invariant::ALL` is what camello asks of itself and is what the fixture tests
run; `Invariant::OPT_IN` holds the checks that need something outside it.

A check can also go unanswered rather than pass or fail: a file that does not
parse leaves the formatter's questions unanswered, and one perl declines to load
leaves the oracle's. Those are counted in their own column, and the messages
behind them are reported whole, once each: grouped by their text with line
numbers folded together, most files first, a few messages and a few of their
files unless `--verbose` asks for all of them. A corpus checked away from the
tree it was installed in answers `Can't locate ... in @INC` to almost
everything, so the run also names `PERL5LIB`, which the spawned perl inherits
like any other environment variable.

Each check carries a slug, a name, a description, and its group; `dev check
--list-invariants` prints the ones it asks, and names `deparse` as living under
`dev perl-deparse`. A run ends with a table of every check that
was asked, counting the sources that passed it, failed it, and were never asked
it. A parse diagnostic is a prerequisite failure for every formatter check, even
when `dev check --only` selects another invariant; such a source is counted as
unanswered — not passing — in every other row. Parser success fixtures must
have no diagnostics, and error fixtures must have at least one. The invariant
integration test covers formatter fixtures and both parser success-fixture
directories.

Tests and fixtures live beside their implementation:

- lexer and parser unit tests and parser snapshots under
  `crates/camello-syntax/src/lex` and `crates/camello-syntax/src/parse`;
- formatter snapshots and input/expected-output regression pairs under
  `crates/camello-fmt/src/fixtures`;
- cross-component invariant coverage in `tests/invariants.rs`;
- `scripts/perl-check` for compilation and `B::Deparse` comparison over the
  fixtures. Not the same question as `dev perl-deparse`, which asks a bare perl:
  a fixture is a fragment that assumes a context, so this one supplies one —
  `CamelloOracle` stubs the modules it uses, undeclared list operators are
  predeclared from perl's own complaint, and each file is tried under both
  dialects. Whether a fragment is Perl depends on which features are on, and
  the answer flips: without `signatures`, `sub f ($x = 1)` is a prototype whose
  text `B::Deparse` echoes verbatim, so respacing it reads as a changed program.
  Asking a bare perl leaves 24 of the fixtures unanswered and calls that one a
  violation; asking this way answers 151 of 155;
- checker fixtures under `crates/camello-sema/src/fixtures`, whose expected
  diagnostics are `#~` comments on the line they belong to, and whose harness
  requires the two sets to be *equal* — so a fixture with no `#~` is how "the
  checker stays silent here" is written down. A multi-file fixture is a
  directory with a `roots` marker naming which directories are checked and
  which only declare;
- language-server fixtures under `crates/camello-lsp/src/fixtures`, where a
  fixture is a directory that is the whole workspace, its expected hovers,
  completions and definitions are `#^` markers pointing at the line above, and
  its diagnostics are `#~` markers in the checker's own grammar. An `X.pl.edit`
  beside an `X.pl` is the buffer after an edit, which is how a mid-edit state —
  the dangling `->`, the diagnostic that has to survive beside it — is written
  down;
- `scripts/lsp-bar` for the language server's corpus bars: indexing all of
  `@INC` prints the files, the wall time and the peak resident size, and
  `--edits N` times the edit loop the debounce is compared against. `camello
  dev index` is the question and the script is the corpus, the same split as
  below;
- `scripts/corpus-check` for selecting a real corpus — the `.pm` files below
  `@INC` — and asking `dev check` and `dev perl-deparse` about it, or the
  checker under `--check`, whose bar is zero errors. The questions
  are the binary's; the script contributes the corpus. Installed code carries
  its own `use feature`, so the dialect search that the fixtures need is not
  wanted here.

The required local verification sequence before pushing is:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -q
```

When lexer or formatter behavior changes, `scripts/perl-check` and a targeted
corpus check provide stronger semantic coverage than snapshots alone.
