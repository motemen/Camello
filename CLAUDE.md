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

# Dump the parsed CST for debugging
cargo run -- dump -e 'my $var=1;'
cargo run -- dump input.pl

# Check if a file is already formatted (exits with non-zero if not)
cargo run -- format --check input.pl
```

Use `-E` instead of `-e` to use character escapes in the input string. e.g. `-E 'sub foo {\n\twarn;\n}'`.

### Testing Strategy

**Invariants first (`tests/invariants.rs`).** Every fixture must parse without a
diagnostic, round-trip losslessly, format to a fixed point
(`format(format(x)) == format(x)`), and preserve its non-trivia token stream.
These are the acceptance bar from ADR 0006 §6 and ADR 0008 §6; they ran
throughout the redesign against a registry of known violations that was only
allowed to shrink.

**Formatter fixtures (`src/fmt/fixtures/`, snapshots via `insta`).** The
spec-by-example: `formatting.md` says what the rules are, these say what they
produce. Add a `.pl` file and run the tests to generate its snapshot.

**Parser fixtures (`src/parse/fixtures/`).** `success/` for valid code (snapshot
is the tree), `errors/` and `statements/errors/` for invalid code (snapshot is
the diagnostics). Prefer adding a case to an existing fixture over writing an
inline test.

**Unit tests.** `src/lex/tests.rs` covers the lexical rules including the seven
reproduced bugs D1-D7; `src/fmt/tests.rs` covers the layout rules including
F1-F6; `src/parse/tests.rs` covers the CST normal form and the error-recovery
acceptance criteria of ADR 0007 §3.

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
