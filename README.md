# Camello

A formatter and static checker for modern Perl, written in Rust.

Camello parses Perl into a lossless concrete syntax tree and rewrites only the
whitespace: every token of the input survives into the output, in the same
order. It aims at real-world code rather than at complete coverage of every
legacy corner of the grammar.

Over the same tree it also checks: `camello lint` for what needs no types, and
`camello typecheck` for what the annotations Perl code already carries — `has
... isa => 'Str'`, `args my $x => 'Int'`, `Class::Accessor::Typed` — can be made
to say.

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

## Check

```bash
camello lint lib t                      # scopes and arity; no type lattice
camello typecheck lib t                 # everything lint says, plus the types
camello typecheck --error-on warning    # exit 1 on a warning too, for CI
camello typecheck --format json lib     # one JSON array, for tooling
```

Both print one diagnostic per line as `path:line:col: severity: message [code]`
and exit 1 when anything at or above `--error-on` (default `error`) was
reported.

`lint` reports undeclared, unused and shadowed lexicals, and arity against a
signature, a Smart::Args list or an `@_` unpacking. `typecheck` adds what the
annotations say: a value that contradicts a declared type, a key a class does
not declare, a method a class does not have, a `Maybe[...]` used with nothing
having checked it. A `# Returns: ArrayRef[Str]` comment above a `sub` annotates
what it gives back.

The rule underneath is that **the checker is silent when it does not know**. A
program with no annotations and no recognisable constructors gets no type
diagnostics at all, and that is correct behaviour rather than a gap.

Turn one off for a line with a comment, or for a project in `camello.toml`:

```perl
my $thing = $legacy->whatever;   ## camello-disable: unknown-method
```

```toml
[check]
lib = ["lib", "t"]
stubs = ["stubs"]
disable = ["unused-variable"]
error-on = "warning"
strict-annotations = true
```

A `stubs/` directory holds ordinary `.pm` files that declare a dependency's
subs with signatures and `Returns:` and no bodies — the `.pyi` idea with no new
syntax, and how a project types the corner of a module no recogniser can read.

[docs/types.md](docs/types.md) is the specification: which types you can write,
where camello reads them from, what it infers, and what each diagnostic means.
[docs/typecheck.md](docs/typecheck.md) is the design behind it, and records the
decisions the real-world corpus forced on it.

Other flags worth knowing: `-j/--jobs` for the worker count, `--extensions` for
which files a directory walk picks up, and `--encoding` for sources that are not
UTF-8 (the file is written back in the encoding it was read in). `camello
format --help` lists them all.

## What it does to your code

The rules — indentation, line breaking, spacing, blank lines, comment handling,
and vertical alignment — are written down in
[docs/formatting.md](docs/formatting.md), with the options that adjust them.
The checker has its own specification in [docs/types.md](docs/types.md).

The short version: your line breaks are kept. Camello does not reflow code to a
target width; it decides indentation, spacing, and alignment, and leaves the
choice of where a long expression breaks to whoever wrote it.

A region between perltidy's `#<<<` and `#>>>` markers comes back exactly as it
was written — the escape hatch for a table whose columns were lined up by hand.

## Develop

[docs/architecture.md](docs/architecture.md) describes the layers. They are
crates, and the split is enforced by Cargo rather than by review:

| | |
| --- | --- |
| `crates/camello-syntax` | vocabulary, scanner, event-recording parser, AST views |
| `crates/camello-fmt` | the Doc IR and its renderer |
| `crates/camello-sema` | symbols, types, flow — the checker |
| `camello` (root) | the command line, and the invariants that compare the two |

Nothing under `camello-sema` can reach `camello-fmt`: a checker has no use for
the Doc IR, and a build of one should not carry it. The rules those layers hold
to, which their comments refer to by name, are in
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
| `scripts/corpus-check` | picks a corpus out of `@INC` and asks the two below about it; `--lint` and `--typecheck` ask the checker instead |
| `scripts/generate-builtins` | regenerate the builtin table from perl's prototypes |
| `camello dev check` | the invariants, asked of arbitrary source |
| `camello dev perl-deparse` | perl's own reading of the input against the output |

`camello dev` is a hidden subcommand holding the tools used to work on camello
itself. It is not an interface to depend on.
