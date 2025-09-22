# ADR 0003: Parser Error Recovery Strategy

- Status: Accepted
- Date: 2025-09-22
- Owners: camello core

## Context

The current parser has poor resilience to syntax errors. When a parse error occurs, it often leads to a cascade of subsequent, unrelated errors, making it difficult to diagnose the actual issues in a file.

The root cause is the lack of a synchronization mechanism. While the parser correctly identifies an initial error and records it as a `SyntaxKind::ERROR` node (as confirmed by analyzing `src/parser/mod.rs` and `src/syntax_kind/mod.rs`), it only consumes a single token and then attempts to continue parsing in a state that is no longer valid.

Modern tools like `biomejs` employ robust error recovery strategies to handle such situations gracefully, allowing them to parse the rest of the file and provide accurate diagnostics for multiple, independent errors.

## Decision

To improve the parser's resilience, we will implement an error recovery strategy based on two main pillars:

1.  **Error Node Recording (Existing Mechanism):**
    *   Continue to use the existing mechanism where invalid tokens or syntax constructs are recorded as `SyntaxKind::ERROR` nodes in the Concrete Syntax Tree (CST). This provides a clear "tombstone" for later processing by formatters or linters.

2.  **Synchronization via Recovery Sets (New Mechanism):**
    *   When the parser encounters an error and enters a "panic mode," it will not immediately resume parsing. Instead, it will skip forward until it finds a token that is part of a predefined "recovery set."
    *   Upon finding a recovery token, the parser will exit panic mode and attempt to resume parsing from that point.
    *   **Initial Implementation:** The first implementation will focus on statement-level recovery. The recovery set will include tokens that reliably mark the end of one statement or the beginning of another.
        *   `semicolon (';')`
        *   `right_brace ('}')`
    *   **Future Enhancements:** This strategy can be extended to be more context-aware. For example, when parsing an expression, the recovery set could include `')'`, `']'`, or `,`. Different parsing functions (`if_stmt`, `sub_def`, etc.) can use different recovery sets tailored to their specific syntax.

## Rationale

- **Robustness:** This prevents a single syntax error from derailing the entire parsing process.
- **Accurate Diagnostics:** The parser will be able to identify and report multiple, independent errors within the same file.
- **Improved Tooling:** A more resilient CST allows downstream tools like the formatter and a future linter to function correctly on the well-formed parts of the code, even if other parts contain errors.
- **Declarative Potential:** While the initial implementation will be imperative within the parsing functions, defining recovery sets as static arrays or passing them as arguments paves the way for a more declarative approach in the future, as discussed.

## Implementation Plan

1.  Modify the main statement parsing logic in `src/parser/statement.rs`.
2.  When a parsing function (like `statement()`) fails and an error is reported, enter a "panic mode."
3.  In panic mode, loop and consume tokens, adding them as `ERROR` nodes, until a token from the statement-level recovery set (`;` or `}`) is encountered.
4.  Once a recovery token is found, exit panic mode and resume normal parsing.
5.  Add test cases with specific, isolated syntax errors to verify that the parser recovers and correctly parses subsequent valid statements.

## Consequences

- **Positive:**
    - The parser becomes significantly more robust and fault-tolerant.
    - The quality of diagnostics for files with multiple errors improves dramatically.
    - Enables better partial processing of incomplete or incorrect code.
- **Negative:**
    - The parser logic becomes slightly more complex.
    - A poorly chosen recovery set could potentially lead to misleading secondary errors, but this risk is far outweighed by the benefits of not failing completely.
