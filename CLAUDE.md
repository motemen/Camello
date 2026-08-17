# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This project provides a robust toolkit for parsing and formatting modern Perl code. While it does not aim to support every legacy feature of the Perl grammar, its primary goal is to function effectively on real-world, modern Perl codebases.

The long-term vision is to expand beyond formatting and evolve into a comprehensive static analysis tool, incorporating features such as linting and type checking.

## Architecture & Design

### Data Flow

```
Perl Source [lex] -> Tokens [parse] -> Events -> CST + TriviaMap [fmt] -> Doc IR -> Lines -> Formatted Code
```

The design is recorded in `dev/adr/`:

- **ADR 0004** — `TokenKind` / `NodeKind` split, generated from one definition.
- **ADR 0005** — lexer contract: a single `expect` state, a token buffer, atomic runs.
- **ADR 0006** — trivia model: ownership and placement rules.
- **ADR 0007** — event-based parser and the CST normal form.
- **ADR 0008** — three-phase formatter over a document IR.

ADR 0001-0003 describe the implementation these replaced and are superseded.

`notes/2026-07-28-redesign-assessment.md` diagnoses what was wrong with that
implementation; `notes/2026-07-28-redesign-deviation-log.md` records every point
where the built thing differs from the ADRs and why.

### Rowan Integration

`lang::PerlLang` implements rowan's `Language` with `lang::SyntaxKind`, a
newtype over `u16`. Token kinds occupy `0..TOKEN_COUNT` and node kinds the range
above, so `SyntaxKind::as_token` / `as_node` recover the split. Only
`parse::replay` touches `GreenNodeBuilder`, and its API is typed on `TokenKind` /
`NodeKind`, so a node kind cannot be written into a token slot.

### Error Recovery

`p.error(msg)` reports without consuming; `p.error_and_bump` and
`p.error_recover` are explicit. Recovery is panic-mode against a synchronisation
set — `;`, `}` and the statement-starting keywords, plus `,` `)` `]` inside a
list — and everything skipped goes into one `ERROR` node (ADR 0007 §3).

## Key Components

### Core API (`src/lib.rs`)

- `parse_perl(&str) -> (PerlNode, Vec<ParseError>)`
- `parse_perl_with_trivia(&str) -> (PerlNode, TriviaMap, Vec<ParseError>)`
- `format_perl(&str) -> (String, Vec<ParseError>)`
- `format_perl_with_options(&str, &FormatterOptions) -> (String, Vec<ParseError>)`

### `src/lang/` — the language vocabulary (ADR 0004)

One `define_language!` invocation generates `TokenKind`, `NodeKind`, the
`SyntaxKind` conversion layer, the `T![…]` macro, `is_keyword` / `is_punct` /
`is_trivia`, the keyword lookup the lexer uses, and `Display`. Adding a keyword
is one edit. `predicates.rs` holds the hand-written semantic predicates, typed on
`TokenKind`.

### `src/lex/` — the scanner (ADR 0005)

Hand-written. `expect` is lexer state, not an argument, so `peek` and `bump`
cannot disagree; a debug assertion checks it. Lookahead is a token buffer that
`set_expect` invalidates from the cursor forward. Quote-like operators, heredoc
bodies, POD and `__DATA__` are scanned as atomic runs (`atomic.rs`), so no
scanning mode is observable between calls.

### `src/parse/` — the parser (ADR 0006, 0007)

`event.rs` records `Start` / `Token` / `Finish` / `Error`; `replay.rs` turns
those into a green tree and builds the `TriviaMap` in the same pass. Speculative
parsing is `checkpoint()` / `rollback()`. `grammar/` holds the rules:
`precedence.rs` (perlop order), `builtins.rs` (argument shape and the `expect`
after a name), `primary.rs`, `expr.rs`, `mod.rs`.

Fixtures live in `src/parse/fixtures/{success,errors,statements}`.

### `src/fmt/` — the formatter (ADR 0008)

`build.rs` turns the CST and trivia map into `doc::Doc`, deciding every layout
question once. `render.rs` walks the document into `Vec<Line>`, applying spacing
and indentation. `align.rs` is an independent O(n) pass over the rendered lines.
Verbatim content is a `Raw` (or `VerbatimLines`) atom the renderer never writes
inside, which is what makes indentation-into-a-string-literal unrepresentable.

Fixtures live in `src/fmt/fixtures/`.

### CLI (`src/cli.rs`)

Clap-based, with `format` and `dump`. `format --check` reports whether a file is
already formatted. Input comes from a file, `-e` / `-E`, or stdin.

## Development Guidelines

### Version Control

- Use Conventional Commits when crafting commit messages (e.g., `feat: ...`, `fix: ...`) to keep history consistent and machine-parseable.
- Always include a descriptive commit body that explains the motivation and high-level changes.

### Pre-commit Checks

- Always run **all** of the following commands, in order, before committing: `cargo fmt` → `cargo clippy --all-targets -- -D warnings` → `cargo test -q`. Skipping any step is not allowed.
- If any command fails, keep iterating on fixes and rerun the full sequence until all commands succeed. Only commit once they complete without errors. If you ultimately cannot resolve a failure, state the reason explicitly in your final message and leave the command undone.
- Report the result of each command in the testing section of your final message, indicating success or failure.

```bash
# Format code
cargo fmt

# Run linter
cargo clippy --all-targets -- -D warnings

# Run all tests
cargo test -q
```

### Building and Testing

```bash
# Basic build check
cargo check

# Run all tests (unit tests, integration tests, doc tests)
cargo test -q

# Run with optimizations
cargo build --release

# Run a single test module
cargo test -q parse::tests
cargo test -q fmt::tests
cargo test -q lex::tests

# Regenerate snapshots after an intended change
INSTA_FORCE_UPDATE=1 cargo test -q --lib

# Run specific test
cargo test -q assignments_align_on_the_first_pass
```

### CLI Usage

```bash
# Format a Perl program (outputs to stdout)
cargo run -- format -e 'my $var=1;'
cargo run -- format input.pl

# Check if a file is already formatted (exits with non-zero if not)
cargo run -- format --check input.pl

# Format a tree in place, one file per core
cargo run -- format --write lib/
cargo run -- format --check lib/ t/       # report what would change
cargo run -- format --write --jobs 4 lib/ # or a worker count of your own
```

One source goes to stdout; a tree has nowhere to print to, so `--write` or
`--check` is required as soon as the arguments name a directory or more than one
file. A file the parser reports a diagnostic on is named and left alone: a
best-effort rewrite is something to ask for one file at a time.

`format` is the interface; the developer tools live under a hidden `dev`
subcommand, in the sense of `go tool`, and may change shape without notice.

```bash
# Dump the parsed CST for debugging
cargo run -- dev dump -e 'my $var=1;'
cargo run -- dev dump input.pl

# Ask the formatter's invariants of arbitrary code (file, directory, or stdin)
cargo run -- dev check input.pl
cargo run -- dev check --list-invariants
cargo run -- dev check --only comments,verbatim ~/some/perl/tree
cargo run -- dev check --jobs 1 ~/some/perl/tree   # serially, for profiling
```

Use `-E` instead of `-e` to use character escapes in the input string. e.g. `-E 'sub foo {\n\twarn;\n}'`.

### Testing Strategy

Two kinds of check, and the difference between them is the design.

**Where the answer is written down, check the answer.** A fixture has an expected
output; checking it against that output is the whole of the job, and subsumes
every property below.

**Where it is not, ask the invariants** (`src/check.rs`, ADR 0006 §6 and ADR 0008
§6): parses without a diagnostic, round-trips losslessly, formats to a fixed
point (`format(format(x)) == format(x)`), preserves its non-trivia token stream,
keeps its comments and its verbatim content unchanged, reaches the same layout
decisions on the second pass as on the first (I2), holds no node whose range
begins or ends on trivia. These are what can be asked of code nobody has written
an expected output for — which is to say, of a corpus. `camello dev check` is the
command; `tests/invariants.rs` runs the same checks over the fixtures, where they
serve as a guard against an expected output that is itself wrong.

**Formatter fixtures (`src/fmt/fixtures/`, snapshots via `insta`).** The
spec-by-example: `formatting.md` says what the rules are, these say what they
produce. Add a `.pl` file and run the tests to generate its snapshot.

**Regression fixtures (`src/fmt/fixtures/regressions/`) are A→B pairs.** The
`.pl` file is A; B is its `.expected.pl` sibling, or A itself when there is none.
A fixture that must come back unchanged is not a special case — it is one whose B
equals its A. These carry no snapshot: the expected output is already the answer,
and a snapshot generated from what the formatter does is exactly what a
regression fixture must not take on trust.

**A defect enters the tree on the day it is found, not the day it is fixed.**
Minimise it, add it under `regressions/` with its expected output, and add one
line to `src/fmt/fixtures/regressions/known-broken.txt`. The ledger is monotone:
an entry may be removed and never added, a listed fixture that starts producing
its expected output fails the test as loudly as an unlisted one that stops, and a
listed fixture is skipped by the invariant sweeps — its output is already known
to be wrong, and one fact belongs in one place. The fix that lands is what
deletes the line.

**Parser fixtures (`src/parse/fixtures/`).** `success/` for valid code (snapshot
is the tree), `errors/` and `statements/errors/` for invalid code (snapshot is
the diagnostics). Prefer adding a case to an existing fixture over writing an
inline test.

**Unit tests.** `src/lex/tests.rs` covers the lexical rules including the seven
reproduced bugs D1-D7; `src/fmt/tests.rs` covers the layout rules including
F1-F6; `src/parse/tests.rs` covers the CST normal form and the error-recovery
acceptance criteria of ADR 0007 §3.

**Three checks that live outside `cargo test`.** All three are worth running
when touching the lexer or the formatter; none is fast enough to want on every
build.

```bash
cargo run -- dev check <path>... # the invariants, over anything at all
./scripts/corpus-check            # run over every .pm below @INC
./scripts/corpus-check --limit 60 # ... a sample of it, for a quick answer
./scripts/perl-check              # ask perl whether formatting changed the meaning
./scripts/snapshot-diff           # compare output against the pre-redesign snapshots
./scripts/snapshot-diff <fixture> # ... for one fixture
```

`scripts/corpus-check` is the "+ real corpus" half of ADR 0008 §6. It formats
every `.pm` below `@INC` and asks three questions: what does `camello dev check` say;
did an input perl compiles turn into an output it rejects; do the two deparse the
same. Files camello reports a diagnostic on, and files that are not UTF-8, are
counted and set aside — the parser not covering a construct is a different
question from the formatter damaging code it did parse. **Every defect the
2026-07-28 review found was invisible to `cargo test` and obvious here**, because
a fixture is code someone wrote to exercise a rule and a corpus is code someone
wrote to get a job done.

The workflow that follows from this: `camello dev check` (or `corpus-check`) on real
code finds a violation → minimise it into a `regressions/` fixture with its
expected output → one line in the ledger → fix it later, deleting that line.

`scripts/perl-check` compiles each fixture, formats it, compiles the output, and
compares `B::Deparse` on both. It exists because `tests/invariants.rs` compares
non-trivia token streams and is therefore **blind wherever one lexical unit is
split into several tokens** — `${^MATCH}` and `${^ MATCH}` are the same token
stream and different variables. It also keeps every fixture honest: a file that
is not Perl cannot be a specification for how to format Perl.

`scripts/snapshot-diff` catches what perl cannot: dropped comments, and layout
that is worse without being wrong.

**Generated source.** `src/parse/grammar/builtins.rs`'s lookup table comes from
perl's own prototypes via `scripts/generate-builtins`; regenerate and `cargo fmt`
rather than editing the `match` by hand (deviation L-011).

### Adding New Syntax Support

1. Add the token and node kinds to the `define_language!` invocation in
   `src/lang/mod.rs`. Keywords go in the `keywords` section and nothing else
   needs updating — `is_keyword`, the lookup and `T![…]` are all generated.
2. Teach `src/lex/` to scan it. A construct that switches scanning mode belongs
   in `atomic.rs` as a single run.
3. Add the rule to `src/parse/grammar/`. Prefer speculative parsing
   (`checkpoint` / `rollback`) over unbounded lookahead. Keyword-as-name
   coercion goes through `grammar::name`, and nowhere else.
4. Add the layout rule to `src/fmt/build.rs`. Spacing is an explicit
   `Doc::Space`; the renderer inserts nothing on its own.
5. Add a fixture under `src/parse/fixtures/` and `src/fmt/fixtures/`, and run
   the tests to generate snapshots. If the syntax uses a keyword, add a case
   showing it also works as a name (`sub tr {}`, `package q;`).

### Parser Rule Pattern

```rust
fn construct(parser: &mut Parser<'_>) {
    let marker = parser.start();
    // ... rules ...
    parser.complete(marker, NodeKind::CONSTRUCT);
}
```

A `Marker` must be completed or abandoned; dropping one is a debug assertion
failure. To try a reading and back out:

```rust
let checkpoint = parser.checkpoint();
let marker = parser.start();
// ... speculative parse ...
if !worked_out {
    parser.abandon(marker);
    parser.rollback(checkpoint);
}
```
