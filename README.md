# Camello

A formatter and static checker for modern Perl, written in Rust.

Camello parses Perl into a lossless concrete syntax tree and rewrites only the
whitespace: every token of the input survives into the output, in the same
order. It aims at real-world code rather than at complete coverage of every
legacy corner of the grammar.

Over the same tree it also checks: `camello check` reports what the scopes say
and what the annotations Perl code already carries — `has ... isa => 'Str'`,
`args my $x => 'Int'`, `Class::Accessor::Typed`, the `mk_accessors` family,
`use constant` — can be made to say.

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
camello check lib t                          # everything, over the tree
camello check --error-on warning             # exit 1 on a warning too, for CI
camello check --min-severity error           # print the errors and nothing else
camello check --format json lib              # one JSON array, for tooling
camello check --disable unknown-method lib   # leave one code unreported
```

It prints one diagnostic per line as `path:line:col: severity: message [code]`
and exits 1 when anything at or above `--error-on` (default `error`) was
reported.

`--error-on` decides the exit status; `--min-severity` decides what is printed,
and what it drops is dropped whole — not counted, and not a reason to fail.

It reports undeclared, unused and shadowed lexicals, and arity against a
signature, a Smart::Args list or an `@_` unpacking. An unread *parameter* is
its own code, `unused-parameter`, reported at `info`: a parameter goes on
saying what the sub takes whether or not the body wants the value. A value held
for its destructor — `my $guard = Scope::Guard->new(...)` — is neither.

What the annotations say adds to that: a value that contradicts a declared
type, a key a class does not declare, a name a call had to pass and did not, a
method a class does not have, a `Maybe[...]` used with nothing having checked
it. `Class::Accessor::Lite` and
the `mk_accessors` family it belongs to declare no types, but they do say which
names are accessors and whether there is a `new`, and that is enough to answer
"no such method". A `# Returns: ArrayRef[Str]`
comment above a `sub` annotates what it gives back, and a name a project's own
type library declares — `type FooBar => as Foo | Bar`, under any of the
`Type::` / `Types::` / `MooseX::Types` family — stands behind every annotation
that writes it.

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
disable = ["unused-parameter"]
error-on = "warning"
min-severity = "warning"
guard-classes = ["My::Lock"]
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

## Edit

```bash
camello lsp        # speaks the Language Server Protocol over stdin/stdout
```

One binary, no separate executable to find or version-match. In an editor it
publishes the same diagnostics `camello check` prints — as you type, 300 ms
after the last keystroke and at once on save — and adds what only an editor can
ask for: hover shows the inferred type of the expression under the cursor or
the signature of the sub, `->` completes the methods the receiver's class
actually declares, in the order perl would find them, and go-to-definition and
an outline come off the same declarations. `textDocument/formatting` is
`camello format` over the buffer.

It keeps its silence discipline in the editor: where the checker knows nothing,
hover shows nothing and completion offers nothing, because an empty list
teaches you what camello can and cannot see, and a list of every sub name in
the repository teaches you to ignore the feature.

The one thing it does that `camello check` will not: it checks a file that does
not parse. A buffer being edited is broken most of the time, and the answers
about the parts you are not touching are still the answers you want — so a
diagnostic near the damage is dropped and the rest of the file keeps its full
signal.

It reads the same `camello.toml` as `camello check`. A thin VS Code client is
in [editors/vscode](editors/vscode); every other editor needs nothing but the
command:

```lua
cmd = { "camello", "lsp" }, filetypes = { "perl" }   -- nvim-lspconfig
```

```elisp
(add-to-list 'eglot-server-programs '(perl-mode . ("camello" "lsp")))
```

[docs/lsp.md](docs/lsp.md) is the design, and says what it deliberately does
not do yet — range formatting, rename, find-references, completion beyond
methods.

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
| `crates/camello-lsp` | documents, the workspace index, the LSP handlers |
| `camello` (root) | the command line, and the invariants that compare the two |

Nothing under `camello-sema` can reach `camello-fmt`: a checker has no use for
the Doc IR, and a build of one should not carry it. `camello-lsp` is the one
crate that sees both, because an editor asks both questions; it sits above
them, the way the root crate does. The rules those layers hold
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
| `scripts/corpus-check` | picks a corpus out of `@INC` and asks the two below about it; `--check` asks the checker instead |
| `scripts/lsp-bar` | the language server's corpus bars: index all of `@INC`, and time an edit loop |
| `scripts/generate-builtins` | regenerate the builtin table from perl's prototypes |
| `camello dev check` | the invariants, asked of arbitrary source |
| `camello dev perl-deparse` | perl's own reading of the input against the output |
| `camello dev index` | build the language server's index over a tree, and say what it cost |

`camello dev` is a hidden subcommand holding the tools used to work on camello
itself. It is not an interface to depend on.
