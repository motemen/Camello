# ADR 0002: Continuation Indent for Manual Line Breaks

- Status: Proposed
- Date: 2025-09-16
- Owners: camello core
- Author: ChatGPT

## Context

The formatter currently indents new lines only by block level. When users insert manual line breaks in positions where Perl normally continues a statement (e.g. before postfix control keywords or binary operators), the subsequent line should receive an extra "continuation" indent. The design must allow future configuration and automatic line wrapping to share the same rules.

## Decision

- Introduce a continuation indent equal to one indent unit (4 spaces) that is applied when a line break occurs:
  - before postfix control keywords (`if`, `unless`, `while`, `until`, `for`, `foreach`),
  - before binary operators,
  - after commas or opening delimiters inside lists/parentheses.
- Implement detection in the formatter using the previous token and current token kind.
- Keep the indent width hardcoded for now but structure the code so it can be made configurable later.

## Consequences

- Manual breaks at supported positions now produce consistent indentation:
  ```perl
  warn 1
      if $err;
  my $x = 1
      + 2;
  ```
- Future auto-wrapping can reuse the same continuation indent logic.
- Other break positions remain unchanged and will be handled in future work.

## Status

This ADR records the initial implementation. Further refinements and configurability may be introduced in later ADRs.
