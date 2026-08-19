# Camello architecture

Camello is a Rust library and command-line formatter for modern Perl. It aims
to handle real-world code without claiming complete compatibility with every
legacy Perl grammar feature. The implementation is lossless: source text is
first represented as tokens and a concrete syntax tree (CST), and formatting
changes trivia and layout rather than program tokens.

The formatting rules themselves are documented in [formatting.md](formatting.md).

## Data flow

```text
Perl source
  -> lexer token buffer
  -> parser events
  -> rowan CST + TriviaMap
  -> formatter Doc IR
  -> rendered lines
  -> vertical alignment
  -> formatted source
```

`src/lang` defines the shared vocabulary, `src/lex` scans source, `src/parse`
records and replays syntax events, and `src/fmt` builds and renders the output.
`src/check.rs` exposes the invariants used to validate arbitrary source.

## Language vocabulary and CST

The `define_language!` invocation in `src/lang/mod.rs` is the source of truth
for `TokenKind`, `NodeKind`, conversions through `SyntaxKind`, the `T![...]`
macro, keyword lookup, token classification, and diagnostic display names.
Tokens and nodes occupy disjoint ranges of the rowan syntax-kind space.

Only `src/parse/replay.rs` writes to `GreenNodeBuilder`. Its wrapper accepts a
`TokenKind` for tokens and a `NodeKind` for nodes, preventing the two categories
from being mixed while building the CST.

The CST is concrete and lossless: concatenating its token texts reproduces the
input. Non-root node ranges begin and end on non-trivia tokens. Error recovery
may create `ERROR` nodes, but it does not discard source text.

## Lexer

The lexer in `src/lex` is hand-written and parser-directed. Perl tokens such as
`/` and `%` are ambiguous without knowing whether the grammar expects a term or
an operator, so that expectation is stored as lexer state. `peek` and `bump`
therefore observe the same context. Changing the expectation invalidates
buffered lookahead from the current cursor and rescans it.

Quote-like operators, heredoc bodies, POD, formats, and data sections are
scanned as complete token runs in `src/lex/atomic.rs`. No partially open lexical
mode escapes a scanning call. Scanner failures are represented by error tokens
and diagnostics rather than silently dropping input.

The token buffer also makes speculative parsing cheap: a checkpoint records the
cursor and expectation, and rollback restores them without rescanning the
already accepted prefix.

## Parser and error recovery

The parser in `src/parse` emits `Start`, `Token`, `Finish`, and `Error` events.
Grammar code does not write rowan nodes directly. After parsing, the replay pass
combines events with the full token stream to build the green tree and the
`TriviaMap`.

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

Formatting has three phases:

1. `src/fmt/build.rs` turns the CST and trivia into `Doc`, making layout and
   spacing decisions.
2. `src/fmt/render.rs` applies indentation and spacing while rendering the
   document into lines.
3. `src/fmt/align.rs` performs vertical alignment over rendered columns.

Explicit `Doc::Space` values control spacing; the renderer does not infer
spaces between arbitrary tokens. Layout groups retain relevant source-newline
decisions so a formatted result is a fixed point on the next pass.

String contents, heredoc bodies, POD, formats, and data sections use raw or
verbatim document atoms. The renderer cannot insert indentation or other text
inside those atoms. Alignment is independent of parsing and only sees rendered
lines and anchors.

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

The stable command-line surface is `camello format`. It reads one source from a
path, `-e`/`-E`, or standard input. A directory or multiple paths require
`--write` or `--check`; directory traversal is recursive and does not follow
symlinks discovered below a requested root. Work across files is parallelized,
but reports remain in input order.

Input is decoded with the selected encoding and invalid byte sequences are
rejected rather than replaced. In-place output is encoded before the original
is touched, written and synchronized to a temporary sibling, and atomically
renamed over the target while preserving its permissions.

The hidden `camello dev` namespace contains development interfaces. `dump`
prints a CST, and `check` evaluates parser and formatter invariants on files,
directory trees, or standard input; `check --deparse` additionally asks perl
whether the output is the same program. These commands may change independently of the
formatter interface.

## Validation

`src/check.rs` defines six checks in two groups. The group is what a violation
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
- `deparse` — perl reads the output as the program the input was: both compile
  under `perl -c`, and `B::Deparse` renders them the same. Opt-in, via `dev check
  --deparse` or by naming it under `--only`, and it is the only check that runs
  another program: `perl -c` executes the `BEGIN` blocks of the file it is
  pointed at. It is also the only one that sees what a token stream cannot —
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
leaves the oracle's. Those are counted in their own column and reported where
the file is named, carrying what the tool that declined actually said — a
corpus checked out of the tree it was installed in answers `Can't locate ... in
@INC` to almost everything, and the run says so and names `PERL5LIB`, which the
spawned perl inherits like any other environment variable.

Each check carries a slug, a name, a description, and its group; `dev check
--list-invariants` prints them all. A run ends with a table of every check that
was asked, counting the sources that passed it, failed it, and were never asked
it. A parse diagnostic is a prerequisite failure for every formatter check, even
when `dev check --only` selects another invariant; such a source is counted as
unanswered — not passing — in every other row. Parser success fixtures must
have no diagnostics, and error fixtures must have at least one. The invariant
integration test covers formatter fixtures and both parser success-fixture
directories.

Tests and fixtures live beside their implementation:

- lexer and parser unit tests and parser snapshots under `src/lex` and
  `src/parse`;
- formatter snapshots and input/expected-output regression pairs under
  `src/fmt/fixtures`;
- cross-component invariant coverage in `tests/invariants.rs`;
- `scripts/perl-check` for compilation and `B::Deparse` comparison;
- `scripts/corpus-check` for invariant and Perl-oracle checks over real code.

The required local verification sequence before pushing is:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -q
```

When lexer or formatter behavior changes, `scripts/perl-check` and a targeted
corpus check provide stronger semantic coverage than snapshots alone.
