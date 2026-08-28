# Design: `camello lsp`

Status: designed 2026-08-28, not implemented. The scope decisions here came
out of an interview rather than out of the corpus, and are marked as such;
once code exists it becomes the authority, the way `docs/typecheck.md`
describes.

This is the design for a Language Server Protocol server over the machinery
camello already has: the lossless CST from `camello-syntax`, the formatter
from `camello-fmt`, and — most of all — the two-phase checker from
`camello-sema`, whose "Incremental reanalysis" section
([typecheck.md](typecheck.md)) left a door open for exactly this. This
document walks through that door.

## What is being built

`camello lsp`: a subcommand that speaks LSP over stdio. One binary, no
separate executable to find or version-match; a client configures the
command `camello lsp` and is done.

The interview set the priorities, and they are not the usual ones. Most
language servers grow diagnostics first and completion last; here the
*wanted* features are **method completion** (`$obj-><C-x>` showing what the
receiver's class actually has) and **type hover** (the checker's inferred
type and the sub's signature under the cursor). Diagnostics and whole-file
formatting are in the first version too, but because they are nearly free —
`Analysis::check` and `format_source` exist and return everything needed —
not because they drive the design. What drives the design is that hover and
completion need something the checker computes today and then throws away:
the type *at a position*.

The deployment target is a large work repository — thousands of files — so
project-wide knowledge cannot be an afterthought. The server indexes the
whole workspace in the background at startup, rust-analyzer style: open
files are useful immediately with single-file analysis, and answers that
need cross-file knowledge (a method defined in a parent class two files
away) light up as the index fills in.

VS Code is the reference client, with a thin extension in this repository
(`editors/vscode/`). The server itself stays a standard stdio LSP with
nothing VS Code-specific in it.

### Non-goals, first version

- **Range formatting.** The Doc IR takes whole files (`file(root)`), and the
  idempotency invariant is stated per file. `textDocument/rangeFormatting`
  needs its own design; it is not a weekend addition to this one.
- **Rename and find-references.** The analysis exists — `scope.rs` resolves
  every lexical reference to its binding today — but only diagnostics come
  out. Exposing the resolution table is on the milestone list because hover
  wants it too; *acting* on it (rename, references) is deferred until the
  table has been exercised read-only.
- **Completion beyond methods.** Package names, imported functions, lexical
  variables, hash keys: all plausible, all later. The interview ranked
  method completion first and the rest unrequested; the design keeps them
  cheap to add but builds none of them.
- **POD in hover.** Type and signature only, for now.
- **Salsa.** Decided against for the first version — see "Incremental
  reanalysis" below. A hand-rolled coarse invalidation is enough for the
  editing loop this design needs, and `typecheck.md`'s rule still holds:
  nothing here may hold a reference that would make a later Salsa migration
  harder.
- **A persistent project index.** The declaration cache on disk
  (`.camello-cache/`) is reused as-is; nothing new is persisted. If cold
  start on the work repository turns out to hurt, that is the measurement
  that justifies a persisted index, and it should be taken first.

## The crate and the runtime

A new workspace member, `crates/camello-lsp`, holding everything but the
`clap` wiring; the root crate's `cli.rs` gains the `lsp` subcommand and
calls into it. The crate depends on `camello-syntax`, `camello-sema`, *and*
`camello-fmt` — the first crate to see both sides. The rule that matters is
unchanged and still Cargo-enforced: nothing under `sema` reaches `fmt`. An
LSP sits above both, so it may see both, the same way the root crate does.

The server is built on **`tower-lsp-server`** (the maintained community fork
of `tower-lsp`, which has been dormant since ~2023 — verified during the
interview). This is a deliberate exception to camello's no-async,
few-dependencies habit: the alternative (`lsp-server` + a hand-rolled main
loop, rust-analyzer style) was considered and the interview chose the trait
— implement `LanguageServer`, get the protocol plumbing for free. The cost
is tokio in the dependency tree of the binary. The discipline that keeps the
cost contained: **tokio exists to shuttle JSON-RPC, and nothing else.** All
parsing and analysis is CPU-bound and runs on blocking threads
(`spawn_blocking` or a small owned pool); no analysis code becomes async; no
async type appears in `camello-sema` or below. If `tower-lsp-server` were
ever swapped out, only `crates/camello-lsp`'s outer layer would know.

### State, snapshots, and `Send`

The one hard constraint is rowan's: `SyntaxNode` is not `Send` (it is
`Rc`-based), but `GreenNode` is `Send + Sync`. `src/report.rs` lives with
this today by parsing every file twice — once in the declaration phase, once
in the body phase — because holding trees across the phase boundary would
mean holding them on one thread. An editor session cannot afford that
compromise for the files it re-analyses on every keystroke, and does not
need to: the document store keeps, per open file,

```text
text: Arc<str>            the current buffer
green: rowan::GreenNode   Send + Sync
trivia: TriviaMap
parse_errors: Vec<ParseError>
version: i32              the LSP document version that produced the above
```

all of it `Send + Sync`. Any thread that needs a tree calls
`SyntaxNode::new_root(green.clone())` — cheap, it is an `Rc` allocation, not
a reparse. Open files are parsed once per edit, not twice.

Around that store, the rust-analyzer shape: a `GlobalState` behind a lock,
mutated only by notifications (`didOpen`, `didChange`, config and watched
-file events, index completion); requests clone a **snapshot** — `Arc`s of
the store, the `Program`, the config — and do all their work on it on a
blocking thread. A request never blocks an edit; an edit never invalidates a
request mid-flight, it just makes the result stale, and stale results are
discarded by version check before publishing. `Analysis`, `Program`,
`FileDecls`, and `Diagnostic` already hold no trees, only offsets and
strings, so they cross threads as they are.

## Documents and positions

Text synchronisation is **full**, not incremental
(`TextDocumentSyncKind::FULL`): every `didChange` carries the whole buffer,
which is reparsed from scratch. The parser is fast enough that this is not
the bottleneck for any file a human is editing, and incremental text
patching is the classic first source of silent corruption in a young server.
Revisit only with a measurement in hand.

Positions need real work. camello speaks UTF-8 byte offsets
(`TextRange`/`TextSize`) everywhere; LSP clients speak line + character,
where "character" is, for VS Code, a UTF-16 code unit. `camello-sema`'s
`LineIndex` converts offsets to line + *character count* — which is neither
encoding, and is right for its job (human-readable CLI positions) and wrong
for this one. It stays where it is; `camello-lsp` gets its own `PositionMap`
per document version: a line-start offset table, plus per-line UTF-8 ↔
UTF-16 conversion done by walking the line (lines are short; no per-line
cache until proven needed). The server advertises `positionEncoding`
`"utf-16"`; if a client negotiates `"utf-8"` (LSP 3.17 allows it), the
conversion degenerates to the offset table alone. Every boundary crossing
goes through `PositionMap` — no LSP type below the handler layer, no
`TextRange` above it.

## Diagnostics

Two producers, one publication:

- **Parse errors**, from the `Vec<ParseError>` the store already holds.
  Always published.
- **Checker diagnostics**, from `Analysis::check` over the stored tree.
  `Diagnostic { code, severity, range, message }` maps directly:
  `code` → `Diagnostic.code` (the stable kebab-case names), `severity` →
  LSP severity, `range` through `PositionMap`. `##camello-disable:`
  suppression and the `[check]` table of `camello.toml` (`disable`,
  `min-severity`, `lib`, `stubs`, …) apply exactly as in the CLI — the LSP
  is another consumer of the same configuration, not a new dialect of it.

The deliberate divergence from the CLI: **the checker runs even when the
parse has errors.** `check_one` in `src/report.rs` discards all sema
diagnostics for a file that fails to parse, which is the right call for a
batch tool — a broken file is one error, not fifty — and the wrong one for
an editor, where the buffer is broken *most of the time* and the user still
wants the fifty real answers about the parts they are not touching. The
recovery machinery already guarantees a usable partial tree (skipped tokens
land in `ERROR` nodes; text is never dropped), so the checker can run; the
question is only noise control near the damage. The rule: a sema diagnostic
whose range intersects the **enclosing statement of an `ERROR` node or parse
-error range** is dropped; everything else is published. The half-typed
statement under the cursor produces no cascade, and the rest of the file
keeps its full signal. The statement granularity matches the parser's own
recovery synchronisation points, which is what makes it the natural blast
radius. This policy lives in `camello-lsp`; `check_one` and the CLI keep
their behaviour.

Timing: recompute on `didChange` after a **~300 ms debounce** (interview
decision — feedback while typing, not only on save), on `didOpen`, and on
`didSave` immediately. Each computation runs on a snapshot stamped with the
document version; if the version has moved by publication time, the result
is thrown away and the debounce timer is already running for the newer text.

Cross-file effects are coarse in the first version: when an edit changes a
file's *declarations* (see the decl-diff below), all **open** files are
re-checked and re-published. Closed files are checked when opened. Nobody is
told about a broken caller in a file nobody is looking at — that is
`camello check`'s job in CI, not the editor's.

## The index

At `initialize`, the server spawns a background walk over the workspace
roots plus the configured `lib` / `stubs` paths from `camello.toml`, running
the **declaration pass only** on every Perl file it finds — the same
`decl::declare_in` the CLI runs, through the same disk cache
(`.camello-cache/`, same key: path, size, mtime, content hash, dialect
fingerprint), so a repository that has ever run `camello check` warm-starts
the index. The walk uses the existing `in_parallel` worker pool; it needs no
tokio.

Memory is the reason this scales to thousands of files: the index retains
**`FileDecls` only** — packages, subs with name-ranges, imports, facts;
serde-sized data — never trees, never source text. Trees exist for open
files (in the document store) and transiently inside a body-pass worker.

When the walk completes, the `Program` graph is built and linked
(`resolve_dependencies` + `link`), and a flag flips: before it, requests are
answered from single-file analysis (the `check_source`-shaped path — lexical
diagnostics are exact, cross-file answers are absent); after it, the full
graph answers. Open files are never queued behind the walk. Files changed
outside the editor arrive via `workspace/didChangeWatchedFiles`
(`**/*.pl`, `**/*.pm`, `camello.toml` — the last reloads config and
relinks); each event re-runs the declaration pass for that file and applies
the same decl-diff as an edit.

## Incremental reanalysis

`typecheck.md` sketched the edit loop and deferred the engine choice; the
interview settled it: **hand-rolled, coarse**, no Salsa, no dependency
recording yet. On each (debounced) edit:

1. Reparse the file (already done by the document store).
2. Rerun its declaration pass.
3. Diff the new `FileDecls` against the indexed one.
4. **Unchanged declarations** — the overwhelmingly common case: a body was
   edited. Re-run the body pass for this file alone; republish its
   diagnostics; touch nothing else.
5. **Changed declarations**: install the new `FileDecls`, relink the
   `Program` (cheap relative to body passes), re-run the body pass for every
   *open* file, republish each.

Step 5 over-invalidates — an edited signature re-checks open files that
never call it — and that is the accepted cost of coarse. The refinement
`typecheck.md` promised (record, during the body pass, which symbols each
sub read; invalidate only dependent subs) slots into step 5 without moving
any other piece, which is the test that the coarse version holds no
reference that makes the fine one harder. Build it when an open-files sweep
is measurably slow on the work repository, not before.

## What sema must newly expose

The honest core of this project. The LSP handlers are thin; the work is
three additions to `camello-sema`, all on the *output* side — no analysis
logic changes, only results that today die in a local variable getting a
way out.

1. **The type side-table.** `flow.rs` infers a type for every expression it
   visits and keeps none of them. Add a recording mode — off for the CLI,
   on for a per-file LSP body pass — that captures `(TextRange, Type)` for
   the expressions the pass already types, and, at each `->` call site, the
   resolved receiver class and method. Cached per document version; this
   single table backs both hover and completion.
2. **The method surface.** `Program` can already answer
   `resolve_method_from` and `linearise`; what completion needs is the
   closed set: *all* methods (and generated accessors, from `attributes`)
   visible on class `C`, in MRO order, each with its `SubDecl` so the
   signature can be shown. A `Program::methods_of(&self, class) -> Vec<…>`
   returning data, not diagnostics.
3. **The scope table.** `scope.rs` resolves every lexical reference today
   and exports `ScopeReport { diagnostics }` — nothing else; `Binding` is
   private. Export the resolution: bindings (range, name, sigil, kind) and
   a references → binding map. Hover on a lexical wants it now;
   definition, references, and rename are then API additions to the LSP,
   not to sema.

## Hover

On a typed expression or binding: the inferred type from the side-table,
spelled the way [types.md](types.md) spells types. On a sub name (definition
or call): the signature from `SubDecl` — package, name, params, `Returns:`.
Where the checker knows nothing, hover shows nothing: `Unknown` produces an
empty response, not a shrug-string. The checker's silence discipline
("silent when it does not know") is a *feature* surfaced in the editor, not
a gap papered over.

## Completion

Triggered on `>` (as part of `->`) and by explicit request. The receiver's
type comes from the type side-table at the offset left of the arrow; if it
names a known class, the items are `Program::methods_of` in MRO order —
label the method name, detail the signature, sorted so the class's own
methods precede inherited ones. If the receiver's type is `Unknown`:
**no items** — an empty list, deliberately. The interview chose precision
over recall: a flood of every sub name in a thousand-file workspace teaches
the user to ignore the feature; an empty list teaches them what the checker
can and cannot see, which is the same thing `camello check`'s silence
teaches.

One mechanical consequence of completing while typing: `$obj->` with
nothing after it is a parse error, and the side-table is keyed by the ranges
of the *last successful* analysis. The handler therefore finds the receiver
by walking tokens left from the cursor (skipping the `->`, taking the
primary expression before it) and looks *that* range up in the current
version's table — falling back to the previous version's table when the
current parse put the receiver inside an `ERROR` node. This is the one
place the LSP reads tokens rather than the tree, and it is confined to the
completion handler.

## Formatting

`textDocument/formatting` is `camello_fmt::format(root, trivia, options)`
over the stored tree, options from `camello.toml`, returned as **one
whole-document `TextEdit`**. Minimal-diff edit splitting is cosmetic — the
idempotency invariant means a second application changes nothing — and can
come later if cursor-jumping annoys. A file whose parse has errors is not
formatted (the request returns null), matching the CLI's refusal to format
what it cannot fully parse. On-save formatting is the client's choice
(`editor.formatOnSave`); the server takes no position.

## The VS Code extension

`editors/vscode/`: TypeScript, `vscode-languageclient`, and as little else
as possible — find the server (a `camello.path` setting, else `camello` on
`PATH`), spawn `camello lsp`, declare the `perl` language activation, done.
No bundled binary, no marketplace publication yet; installation is `vsce
package` + install-from-VSIX, or F5 from the extension folder during
development. The extension surfaces the server's version from `initialize`'s
`serverInfo` so a mismatch is visible. Anything the extension configures
must be a pass-through of server/CLI configuration, so eglot and
nvim-lspconfig users get the identical server by pointing at `camello lsp`
themselves.

## Testing

The fixture habit continues (`docs/contracts.md`, "Tests and fixtures"), one
level up:

- **`PositionMap`** unit tests over the nasty cases: multi-byte UTF-8,
  astral-plane characters (two UTF-16 units), `\r\n`, a final line without
  newline. This is where LSP servers classically go wrong; it is also the
  easiest layer to pin down exhaustively.
- **Handler tests without a transport.** `tower-lsp-server`'s
  `LanguageServer` is a trait on a plain struct; tests construct the server,
  feed it `didOpen`/`didChange` with fixture text, and call handlers
  directly. Fixtures are Perl files with cursor markers in comments
  (`#^ hover`, `#^ complete` on the line below the position they point at),
  harness-checked the way `#~` diagnostics are: the marker set must equal
  the response set. insta snapshots for response shapes.
- **The broken-buffer suite.** Fixtures that are *deliberately* mid-edit —
  the unclosed brace, the dangling `->` — asserting both that sema
  diagnostics survive outside the blast radius and that completion works on
  the dangling arrow. No existing fixture class covers this; it is the
  LSP's own corpus.
- **Corpus bars**, in the `scripts/corpus-check` tradition: index all of
  `@INC` — the walk completes, memory stays within a stated budget, and the
  wall time is printed so regressions are visible. A scripted
  edit-loop benchmark (N edits to one file of the corpus, decl-diff clean)
  gives the debounce a number to be compared against.

## Milestones

Each shippable, each proving the layer the next one stands on:

1. **Skeleton.** `camello lsp` + `crates/camello-lsp`; initialize;
   full-sync document store with green trees; parse-error diagnostics;
   whole-file formatting; the VS Code extension. Proves the plumbing,
   `PositionMap`, and the snapshot shape end to end with almost no new
   analysis surface.
2. **Single-file checking.** Debounced sema diagnostics on the open file
   (single-file mode), the partial-tree policy and its blast-radius
   suppression; `textDocument/documentSymbol` from `FileDecls` (subs and
   packages — the ranges are already there).
3. **The index.** Background walk, `FileDecls` residency, `Program`
   construction, decl-diff invalidation, watched files, cross-file
   diagnostics for open files. Definition for bareword calls and method
   calls rides along nearly free (`resolve_call` / `resolve_method_from`
   exist; this milestone is when a `Program` exists to ask).
4. **The type side-table** in sema; hover for types and signatures.
5. **The method surface** in sema; method completion, including the
   dangling-arrow token walk.
6. **The scope table** in sema; hover and definition for lexicals;
   references and rename become possible and are scheduled on demand.

## Open questions

Decided provisionally; written down so the decision is visible.

- **Full sync forever?** Full for now. If profiling shows reparse-per-
  keystroke matters on the largest real files, incremental sync plus
  rowan-level incremental reparsing is the escape hatch — in that order,
  and neither before a measurement.
- **Memory bar for the index.** `FileDecls`-only residency is *assumed* to
  fit the work repository comfortably; the corpus bar makes it a number. If
  the assumption fails, the decl cache already on disk makes drop-and-
  recache the obvious spill strategy.
- **Where does the blast radius stop?** "Enclosing statement" is the first
  answer for diagnostic suppression near parse errors. If real editing
  shows cascades leaking past it (a broken `sub` header poisoning the whole
  body), widening to the enclosing block is the next answer. The
  broken-buffer suite is where the evidence accumulates.
- **`camello lsp` visibility.** Visible in `--help`, unlike `dev`: it is a
  user-facing entry point, hidden discoverability helps nobody.
