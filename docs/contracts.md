# The contracts

Five names carry the design of camello, and 166 comments in `src/` refer to them
by name — "the lexer contract", "I3 of the formatter contract". This file is
what they refer to.

The names come from ADRs 0004-0008, written on 2026-07-28 as proposals against
the implementation of the time. That implementation is gone and so are the ADRs;
the vocabulary outlived both. What follows is stated in the present tense, and
the code is the authority: where an ADR and the source disagreed about a rule,
the source was right and the rule is written as the source has it.

`docs/architecture.md` says what is where. `docs/formatting.md` specifies the
output. This file is the set of rules the source is expected to hold to — and,
where a rule rests on evidence too thin to be a rule, the `GUESS:` label
(`docs/architecture.md`) marks the place instead.

## The language model

`src/lang`. Tokens and nodes are different things and are typed as such.

- `TokenKind` and `NodeKind` are separate enums. `SyntaxKind(u16)` is a
  generated conversion layer for rowan: tokens map to `0..TOKEN_COUNT`, nodes
  above it. Discriminants follow the order of the macro, and the tree is never
  persisted between processes, so reordering is allowed.
- One macro, `define_language!` (`src/lang/macros.rs`), is the single source.
  From it come both enums, the `SyntaxKind` conversions, the `T![...]` macro,
  `is_keyword` / `is_punct` / `is_trivia` derived from the section a kind is
  declared in, the keyword lookup the lexer uses, and `display_name` — so a
  diagnostic says `` `}` `` rather than `R_BRACE`.
- Semantic predicates — `can_start_term`, `starts_statement`, `is_sigil` —
  carry grammar knowledge that cannot be derived, and stay hand-written in
  `src/lang/predicates.rs`. They are typed on `TokenKind`, which is what makes
  "a node kind reached a token predicate" unrepresentable.
- Only the replay pass touches `GreenNodeBuilder`, through an API typed on
  `NodeKind` and `TokenKind`.
- Failure has a kind of its own: `UNTERMINATED_REGEX`, `UNTERMINATED_QUOTE_LIKE`,
  `UNTERMINATED_HEREDOC`, `ERROR_CHAR`. Raw spans that are text to everything
  above the lexer — prototype bodies, attribute arguments, `__DATA__` — are
  `RAW_CONTENT` tokens rather than escape hatches that re-read the source.
- Compound assignments (`+=`, `//=`, `**=`) are single tokens. There is no
  `COMPOUND_ASSIGNMENT` node.

## The lexer contract

`src/lex`. The scanner is hand-written and parser-directed. Perl cannot be lexed
without knowing whether a term or an operator is due, so that expectation is
state, and the grammar owns it.

- `Expect` is `Term` or `Operator`, held by the lexer, set by the parser at
  syntactic decision points. `peek` and `bump` do not take it as an argument.
- **The coherence guarantee.** A token is never scanned under one expectation
  and consumed under another. `set_expect` drops the buffer from the cursor
  forward and rescans; so does `rollback`, unconditionally, because a
  speculative attempt may have rescanned tokens *past* the cursor under a
  different expectation and left them there — `foo{sub}` looks fine at the
  cursor and is wrong four tokens later. A debug build asserts the invariant in
  `bump`; a release build would consume the mis-scanned token in silence.
- **The atomicity guarantee.** Quote-like operators, heredoc bodies, POD,
  `__DATA__`, prototypes and attribute arguments are pushed as whole token runs
  by a single scan step. No scanning mode is observable between calls, so
  lookahead cannot see a half-open mode and an unterminated construct cannot
  leave one behind.
- **Failure is not silence.** `peek` returns `None` at end of input and nowhere
  else. An unterminated construct is one error token covering the rest of the
  input, plus one diagnostic — never a fallback to a different reading, which is
  how the presence of a `/` 900 lines down used to change the tree on line 5.
- **The bareword exception.** In term position a quote-like keyword
  (`q qq qw qx m qr s tr y`) is emitted as `IDENT` when the next token past
  horizontal space is `=>` or `}`, and is a quote-like operator otherwise. A
  comma is deliberately not on that list: perl has no such exception either, and
  adding it made `$v =~ m,/\z,,;` lex as a bareword and an unterminated regex.
- POD and `__DATA__` are recognised in column 0 only. A file test is `-` plus a
  character that actually names one (`efdlpSbcugktrwxoRWXOszAMC`, not any
  letter). In term position `<...>` is one `IO_HANDLE` token, which is what
  settles it against a comparison — the expectation decides, so the parser never
  has to try one reading and undo it. An apostrophe is not a package separator:
  every input is read as though under `no feature "apostrophe_as_package_separator"`.
- The expectation never updates itself. Where a name may follow, the grammar
  says so through `take_name` — neither `Expect` state can express "a name goes
  here", and under `Term` a `sub tr {}` would open a substitution.

## The parser contract

`src/parse`. The parser records events; nothing writes the tree as it goes.

- `Event` is `Start { kind, forward_parent }`, `Token`, `Finish`,
  `Error(Diagnostic)`, `Tombstone`. `forward_parent` is how a left-associative
  infix expression wraps an operand it had already emitted; a `Tombstone` is an
  abandoned `Start`, skipped at replay. Replay combines the events with the full
  token stream to build the green tree and the `TriviaMap`.
- **Speculation replaces lookahead.** `checkpoint()` and `rollback()` rewind
  both the event list and the lexer cursor, so an ambiguity is settled by trying
  a reading rather than by scanning ahead for evidence: an anonymous hash before
  a block, a signature before a prototype, a `try` statement before a call to a
  function named `try`.
  Bounded lookahead — is the next token a `=>` — is fine. Unbounded scanning of
  the token stream is not, and no longer exists.
- **Normal form.** The children of a `ROOT` or a `BLOCK` are statement nodes
  from a closed set; there is no generic `STMT` wrapper and an expression
  statement is always an `EXPR_STMT`. Operators build `BINARY_EXPR`,
  `ASSIGN_EXPR`, `TERNARY_EXPR`, `PREFIX_EXPR`, `POSTFIX_EXPR` rather than one
  catch-all. A comma series is always a `LIST_EXPR`, including at zero and one
  element, so the shape does not depend on whether a comma was written. A call
  is one of `CALL_EXPR`, `LIST_CALL_EXPR`, `METHOD_CALL_EXPR`, `BLOCK_CALL_EXPR`,
  and a filehandle is a `FILEHANDLE` child rather than an unexplained first
  argument.
- An `ERROR` node wraps tokens skipped to recover, and only that. A diagnostic
  on its own does not change the tree. `error` does not consume by default;
  recovery synchronises on statement boundaries, closing delimiters and list
  separators.
- **The builtin table** (`grammar/builtins.rs`) answers two questions and only
  two: what shape the arguments take, and what the lexer should expect straight
  after the name. It is generated from perl's own prototypes and **committed**,
  not generated at build time — a crate that cannot be built without a perl
  installation is a worse trade than a table that has to be regenerated when
  perl gains a builtin.
- A name that is not in the table is a list operator (`GUESS:`), and a bareword
  whose declaration is not in sight is read by the rules at
  `grammar/expr.rs` — all of them labelled, because there is no symbol table to
  ask. Such a list operator stops before the next `key => value` pair when it is
  itself the value of one (`GUESS:`): where the list around it is already a
  table of pairs, the next pair is read as the table's rather than as an
  argument. Reading it either way is reading a prototype nobody can see — `($)`
  against none at all — so neither is the faithful answer and the label says
  which was chosen. `getopt \@args, 'a|all' => \$all` is not the value of a
  pair and keeps its whole list.
- A heredoc body is a token that lands where the line its marker is on ends,
  which is between two statements. Anything walking only child *nodes* drops it.

## The trivia model

`src/parse/trivia.rs`, built during replay.

- `WHITESPACE` is horizontal only. `NEWLINE` is exactly one `\r?\n`, so blank
  lines survive as consecutive newline tokens. `COMMENT` runs to just before the
  newline that ends its line.
- The parser never sees trivia. Events carry non-trivia tokens only, and one
  pass at replay puts trivia into the tree.
- **Ownership.** The trivia between two non-trivia tokens splits at the first
  newline: up to and including it belongs to the preceding token as `trailing`,
  everything after it to the following token as `leading`. A comment on the same
  line as the code it follows belongs to that code; an own-line comment belongs
  to what comes next. A run after the last token of the file belongs to nobody —
  the rule gives it to the token that follows, and there is none — so
  `TriviaMap::at_end` names that case rather than leaving it implicit, which is
  how the last line of `feature.pm` used to be dropped.
- **Placement.** Trivia goes after the `Finish` of every node ending at the
  preceding token and before the `Start` of every node beginning at the next —
  the position of their lowest common ancestor. The consequence is the rule
  everything else relies on: **no node's range begins or ends on trivia**, so a
  node's range is the range of its code, and `dev check` asserts it as part of
  the tree's normal form.
- `TriviaMap` is the formatter's only source for comments and blank lines. It is
  built once, at replay, and never by re-walking the tree. Each `Trivia` carries
  its text, because placement puts a leading comment *outside* the node it
  belongs to and a consumer holding the node cannot find the token again.
- The tree preserves blank lines faithfully. Normalising them is the formatter's
  policy, not the CST's.

## The formatter contract

`src/fmt`, in three phases: **build** (CST + `TriviaMap` → `Doc`, where every
flat/broken decision is made), **render** (`Doc` → `Vec<Line>`, applying spacing
and indentation), **align** (inserting padding). A fourth, **skip**, is the one
place the source's own text is put back over the answer: the lines a `#<<<` /
`#>>>` pair covers (`docs/formatting.md` VERBATIM-2). It runs last and over
lines, because a marked region is a run of lines and what it overrides is
indentation, spacing and alignment together. `docs/formatting.md` is the
specification of what comes out; these are the rules of how.

- **Spacing is decided at build time** and emitted as `Doc::Space`. The renderer
  never puts a space between two tokens on its own, so there is no path that
  bypasses the rule.
- **Verbatim content is never touched.** `Raw` and `VerbatimLines` atoms are
  reproduced as they are, and the renderer neither breaks nor indents inside
  them.
- **Two rules decide breaking.** A group is broken when the source put a newline
  at that construct's seed point — straight after an opening delimiter — or when
  it holds a comment, which is a hard line break by nature. Everything else the
  user wrote is a `UserLine`, preserved individually. Blocks are their own case
  and `docs/formatting.md` NEWLINE-2 states it. There are no suppression flags:
  a flat group cannot contain a `HardLine`, and that is guaranteed by how
  builders nest rather than by a flag that travels.
- **Indentation is structural.** `Indent` is one level, `Continuation` is the
  extent of a wrap (INDENT-3), `Hanging` starts continuation lines at a computed
  column, and `Rooted` places a construct from the line it begins on rather than
  from its statement's level (INDENT-4).
- **A group carries two decisions, not one.** `broken` is whether the writer
  seeded a break after the opening delimiter, and settles what may take a
  `Doc::Line`. `anchored` is whether the construct occupies more than one line,
  and settles whether the anchors written directly inside it are recorded —
  alignment is a relation between lines, and a construct with one line has no
  second line to agree with. They are usually the same answer and part company
  where the writer put something after the bracket and broke the line anyway:
  `f($o,` seeds nothing and is still a table.
- **Align reads columns, never the source.** The pass runs over rendered lines.
  `AnchorClass` is `Assign`, `FatComma(depth)`, `Fallback`, `PostfixKeyword`,
  `TrailingComment`; an `Anchor` carries the width of what must end at the
  agreed column, so `=` and `-=` line up on their `=`. `Shape` declares where
  one group of comparable statements ends — every statement declares one, since
  a statement that declared nothing would inherit the last one's shape and align
  with it. Source spacing and source newlines are not inputs, which is what
  keeps a comment from disturbing a table.

### The invariants

- **I1, verbatim preservation.** The content of a `Raw` atom survives byte for
  byte.
- **I2, seed stability.** A newline appears in the output only where a broken
  `Line`, `UserLine` or `HardLine` put it — so re-reading the output gives every
  group the same broken decision. Any new seed rule owes this round trip: "a
  newline after the opening delimiter" qualifies because a broken group's own
  output has one there.
- **I3, align is its own fixed point.** Alignment is computed from rendered
  columns and pads with spaces, which create no new anchor. So
  `align ∘ align = align`.

These are asked rather than assumed. `camello dev check` puts six invariants to
arbitrary source (`src/check`), and I1-I3 are among them: verbatim preservation
is I1, and idempotency covers I2 and I3 both. The rest are a clean parse, the
tree's normal form — the trivia model's placement rule, asserted — semantic
preservation (input and output re-lex to the same non-trivia tokens), and
comment preservation. A seventh, the deparse oracle, asks perl itself and is
opt-in: running a perl over a file runs that file's `BEGIN` blocks, which is not
something a checker may do to somebody's corpus unless told to.
