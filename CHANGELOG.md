# Changelog

One line per change. The reasoning is in the commit it came from.

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
