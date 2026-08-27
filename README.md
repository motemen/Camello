# Camello

A formatter for modern Perl, written in Rust.

Camello parses Perl into a lossless concrete syntax tree and rewrites only the
whitespace: every token of the input survives into the output, in the same
order. It aims at real-world code rather than at complete coverage of every
legacy corner of the grammar.

> **Status: early.** The output is not yet stable across versions, and the
> layout options below are hidden from `--help` because their names and
> defaults may still change. There is no release yet; build it from source.
> What moved between tags, and what it does to a file already formatted, is in
> [CHANGELOG.md](CHANGELOG.md).

## Build

Rust 1.91 (pinned in `rust-toolchain.toml`).

```bash
cargo build --release      # target/release/camello
cargo install --path .     # or put it on your PATH
```

`scripts/build-linux` cross-compiles a static Linux binary from macOS with
nothing beyond rustup.

## Use

```bash
camello format path/to/script.pl        # formats the file in place
camello format lib t                    # walks directories (.pl .pm .t .psgi)
camello format script.pl -o -           # writes to standard output instead
camello format -e 'my %h=(a=>1);'       # formats an argument
camello format --check lib              # names what would change; exits 1 if any
```

`--check` and `-l/--list-different` print one path per line, which makes them
the shape a CI step or a pre-commit hook wants. A file the parser reports a
diagnostic on is left alone and the run exits 1 — whether it was named on its
own, named beside others, or found under a directory. A source read from
standard input is handed back the way it arrived, so a filter in an editor's
save hook is safe to wire up without a wrapper around it.

Other flags worth knowing: `-j/--jobs` for the worker count, `--extensions` for
which files a directory walk picks up, and `--encoding` for sources that are not
UTF-8 (the file is written back in the encoding it was read in). `camello
format --help` lists them all.

## What it does to your code

The rules — indentation, line breaking, spacing, blank lines, comment handling,
and vertical alignment — are written down in
[docs/formatting.md](docs/formatting.md), with the options that adjust them.

The short version: your line breaks are kept. Camello does not reflow code to a
target width; it decides indentation, spacing, and alignment, and leaves the
choice of where a long expression breaks to whoever wrote it.

A region between perltidy's `#<<<` and `#>>>` markers comes back exactly as it
was written — the escape hatch for a table whose columns were lined up by hand.

## Develop

[docs/architecture.md](docs/architecture.md) describes the layers — `src/lang`
(vocabulary), `src/lex` (parser-directed scanner), `src/parse` (event-recording
parser), `src/fmt` (Doc IR and renderer). The rules those layers hold to, which
their comments refer to by name, are in
[docs/contracts.md](docs/contracts.md).

Before pushing:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -q
```

Snapshot and fixture tests live beside the code they cover. Beyond them:

| | |
| --- | --- |
| `scripts/diff` | the diff for one file or snippet, while iterating |
| `scripts/perl-check` | `perl -c` and `B::Deparse` over the fixtures |
| `scripts/corpus-check` | picks a corpus out of `@INC` and asks the two below about it |
| `scripts/generate-builtins` | regenerate the builtin table from perl's prototypes |
| `camello dev check` | the invariants, asked of arbitrary source |
| `camello dev perl-deparse` | perl's own reading of the input against the output |

`camello dev` is a hidden subcommand holding the tools used to work on camello
itself. It is not an interface to depend on.
