# ADR 0002: Continuation Indent for Manual Line Breaks

- Status: Accepted
- Date: 2025-09-16
- Owners: camello core
- Author: ChatGPT

## Context

The formatter currently indents new lines only by block level. When users insert manual line breaks in positions where Perl normally continues a statement (e.g. before postfix control keywords or binary operators), the subsequent line should receive an extra "continuation" indent. The design must allow future configuration and automatic line wrapping to share the same rules.

## Decision

- Apply a continuation indent equal to one indent unit (4 spaces) to every line that follows a user-supplied newline. The formatter now
  distinguishes between line breaks that come from the original source and those it inserts itself (e.g. after semicolons or braces).
- Track the origin of the most recent line break in the formatter state. When writing the next token at the start of a line, add the
  continuation indent if and only if the previous break came from user input and the previous token does not represent a new block/start
  of file. Control-flow keywords such as `else`/`elsif` after a closing brace continue to align with their block by suppressing the
  continuation indent in those contexts.
- Keep the indent width hardcoded for now but structure the code so it can be made configurable later.

## Consequences

- Manual breaks at any non-structural position now produce consistent indentation without enumerating individual syntax patterns:
  ```perl
  warn 1
      if $err;
  my $x = 1
      + 2;
  my $x = 1 # comment
      + 2;
  ```
- Future auto-wrapping can reuse the same continuation indent logic.
- Block-leading lines and keywords such as `else` remain aligned with their enclosing indentation level, preserving readability for
  structural constructs.

## Status

This ADR records the initial implementation. Further refinements and configurability may be introduced in later ADRs.
