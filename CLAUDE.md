# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This project provides a robust toolkit for parsing and formatting modern Perl code. While it does not aim to support every legacy feature of the Perl grammar, its primary goal is to function effectively on real-world, modern Perl codebases.

The long-term vision is to expand beyond formatting and evolve into a comprehensive static analysis tool, incorporating features such as linting and type checking.

## Development Commands

### Building and Testing
```bash
# Basic build check
cargo check

# Run all tests (unit tests, integration tests, doc tests)
cargo test

# Run with optimizations
cargo build --release

# Run a single test module
cargo test lexer::tests
cargo test parser::tests
cargo test formatter::tests

# Run specific test
cargo test test_var_decl_formatting
```

### CLI Usage

```bash
# Format a Perl program (outputs to stdout)
cargo run -- format -e 'my $var=1;'
cargo run -- format input.pl

# Dump the parsed CST for debugging
cargo run -- dump -e 'my $var=1;'
cargo run -- dump input.pl
```

## Architecture Overview

### Data Flow

```
Perl Source [Lexer] -> Tokens [Parser] -> CST [Formatter] -> Formatted Code
```

### Core Components

**SyntaxKind** (`src/syntax_kind.rs`): Central enum defining all Perl syntax elements. Uses `#[repr(u16)]` for efficient Rowan integration.

**Lexer** (`src/lexer/mod.rs`): Logos-based tokenizer that handles Perl-specific tokens. It performs contextual disambiguation for operators like `/` (division vs. regex), `%` (modulo vs. hash sigil), and keywords like `tr`, `y`, `s`. It correctly tokenizes quote-like operators (`q`, `qq`, `qw`, `s`, `tr`, `y`), POD blocks, and `__DATA__` sections.

**Parser** (`src/parser/mod.rs`): Recursive descent Pratt parser using Rowan's GreenNodeBuilder.
- `root()`: Parses an entire file, including statements, POD, and data sections.
- `statement()`: Handles various statement types, including control structures (`if/else/elsif`, `for`, `while`, `unless`), declarations (`package`, `use`, `no`), and subroutine definitions.
- `var_decl()`: Parses variable declarations (`my`, `our`, `state`, `local`).
- `sub_def()`: Parses subroutine definitions, including prototypes.
- `expression()`: Parses complex expressions with correct operator precedence, including infix, prefix, and postfix operators, ternary expressions (`?:`), anonymous subroutines (`sub { ... }`), and typeglobs (`*FOO`, `*{...}`).

**Formatter** (`src/formatter/mod.rs`): Traverses the CST to apply formatting rules.
- Indentation: 4-space indentation for blocks.
- Spacing: Adds spaces around operators (e.g., `$a = $b + $c`), but keeps others compact (e.g., `$obj->method`).
- Braces: Uses K&R brace style (`sub name {`).
- Comments & Whitespace: Preserves comments, newlines, and user-added empty lines between statements.
- Multiline Formatting: Intelligently formats multiline array/hash references and `qw` expressions based on whether they contain newlines in the original source.
- Verbatim Sections: Preserves `__DATA__` and POD sections exactly as they are.

**CLI** (`src/cli.rs`): Clap-based interface supporting `format`, `check`, `dump` subcommands, and input from files, strings (`-e`), or stdin.

### Rowan Integration

The project uses a custom `PerlLanguage` type implementing Rowan's `Language` trait. SyntaxKind conversion is handled via `From<SyntaxKind> for rowan::SyntaxKind`.

### Error Recovery

The parser implements multiple error recovery strategies:
1. **Token-level**: Skips invalid tokens and records them as `ERROR` nodes.
2. **Statement-level**: Attempts to recover to the next semicolon or brace to continue parsing.
3. **Structure-level**: Continues parsing subsequent statements even after encountering a malformed one.

## Key Implementation Notes

### Adding New Syntax Support

1.  Add new variants to the `SyntaxKind` enum in `src/syntax_kind.rs`.
2.  Update the lexer in `src/lexer/mod.rs`. This may involve adding a new `Token` variant, updating regexes, or adding contextual disambiguation logic in `disambiguate()` or `next_token()`.
3.  Add parsing logic to the appropriate parser function in `src/parser/`. For expressions, this may involve adding a new `OperatorInfo` in `src/parser/expression/precedence.rs`.
4.  Update the formatter in `src/formatter/` by adding a new `format_...` function to handle the new syntax node.
5.  Add comprehensive snapshot tests in `src/formatter/tests.rs` to cover various use cases of the new syntax.

### Parser Function Pattern
```rust
fn parse_construct(&mut self) {
    self.builder.start_node(SyntaxKind::CONSTRUCT.into());
    // ... parsing logic ...
    self.builder.finish_node();
}
```

### Testing Strategy

Our testing strategy prioritizes end-to-end formatting correctness and maintainability by focusing on snapshot tests.

-   **Primary: Formatter Snapshot Tests (`src/formatter/tests.rs`)**
    -   These are the most important tests, acting as integration tests that cover the entire process from lexing and parsing to final output.
    -   They verify the formatter's output against stored snapshots using `insta`.
    -   The goal is to have a comprehensive suite of snapshot tests that cover a wide range of valid Perl syntax and formatting edge cases.

-   **Secondary: Lexer and Parser Unit Tests (`src/lexer/tests.rs`, `src/parser/mod.rs`)**
    -   These tests should be limited to cases that are difficult or impossible to cover through formatter tests.
    -   Their primary role is to validate specific **error handling** and **context-sensitive ambiguity resolution** (e.g., distinguishing `/` as division vs. a regex delimiter).
    -   Avoid adding unit tests for simple tokenization or parsing of basic syntax that is already implicitly covered by the formatter snapshot tests. This reduces redundancy and maintenance overhead.

-   **Integration Tests**: Verifies CLI behavior and file I/O.

This approach ensures that our tests are both effective and efficient, focusing developer effort on creating robust, real-world formatting scenarios rather than on redundant, low-level unit tests.
