# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This project provides a robust toolkit for parsing and formatting modern Perl code. While it does not aim to support every legacy feature of the Perl grammar, its primary goal is to function effectively on real-world, modern Perl codebases.

The long-term vision is to expand beyond formatting and evolve into a comprehensive static analysis tool, incorporating features such as linting and type checking.

## Architecture & Design

### Data Flow

```
Perl Source [Lexer] -> Tokens [Parser] -> CST [Formatter] -> Formatted Code
```

### Rowan Integration

The project uses a custom `PerlLanguage` type implementing Rowan's `Language` trait. SyntaxKind conversion is handled via `From<SyntaxKind> for rowan::SyntaxKind`.

### Error Recovery

The parser implements multiple error recovery strategies:

1. **Token-level**: Skips invalid tokens and records them as `ERROR` nodes.
2. **Statement-level**: Attempts to recover to the next semicolon or brace to continue parsing.
3. **Structure-level**: Continues parsing subsequent statements even after encountering a malformed one.

## Key Components & Functions

### Core API Functions

- **`pub fn format_perl(input: &str) -> (String, Vec<ParseError>)`** (`src/lib.rs`): The primary public API. It takes a string of Perl code, orchestrates the lexing, parsing, and formatting process, and returns the formatted code along with any parse errors.

- **`pub fn parse_perl(input: &str) -> (PerlNode, Vec<ParseError>)`** (`src/lib.rs`): Parses Perl source into a `PerlNode` CST and returns any syntax errors encountered during parsing.

- **`Parser::root(&mut self)`** (`src/parser/mod.rs`): The top-level parsing function that starts the recursive descent process for an entire file. It's the internal entry point called by `parse_perl`.

- **`format_node(node: &SyntaxNode, builder: &mut Builder)`** (in `src/formatter/mod.rs`): The heart of the formatter. This function recursively traverses the CST, applying formatting rules (indentation, spacing, newlines) for each `SyntaxNode` and appending the result to a string builder.

### Core Components

**SyntaxKind** (`src/syntax_kind.rs`): Central enum defining all Perl syntax elements. Uses `#[repr(u16)]` for efficient Rowan integration.

**Lexer** (`src/lexer/mod.rs`): Logos-based tokenizer that handles Perl-specific tokens. It performs contextual disambiguation for operators like `/` (division vs. regex), `%` (modulo vs. hash sigil), and keywords like `tr`, `y`, `s` via a dedicated `disambiguate` method that uses parser-provided `LexContext` hints. It correctly tokenizes quote-like operators (`q`, `qq`, `qw`, `s`, `tr`, `y`), POD blocks, and `__DATA__` sections.

**Parser** (`src/parser/mod.rs`): Recursive descent Pratt parser using Rowan's GreenNodeBuilder.

- `root()`: Parses an entire file, including statements, POD, and data sections.
- `statement()`: Handles various statement types, including control structures (`if/else/elsif`, `for`, `while`, `unless`), declarations (`package`, `use`, `no`), and subroutine definitions.
- `var_decl()`: Parses variable declarations (`my`, `our`, `state`, `local`).
- `sub_def()`: Parses subroutine definitions, including prototypes.
- `expression()`: Parses complex expressions with correct operator precedence, including infix, prefix, and postfix operators, ternary expressions (`?:`), anonymous subroutines (`sub { ... }`), and typeglobs (`*FOO`, `*{...}`).

- **Formatter** (`src/formatter/mod.rs`): Traverses the CST to apply formatting rules.

- Indentation: 4-space indentation for blocks.
- Spacing: Adds spaces around operators (e.g., `$a = $b + $c`), but keeps others compact (e.g., `$obj->method`).
- Braces: Uses K&R brace style (`sub name {`).
- Comments & Whitespace: Preserves comments, newlines, and user-added empty lines between statements.
- Multiline Formatting: Intelligently formats multiline array/hash references and `qw` expressions based on whether they contain newlines in the original source.
- Verbatim Sections: Preserves `__DATA__` and POD sections exactly as they are.
- Token Spans: Tracks the original token span for each line, enabling source mapping and diff generation features.

**CLI** (`src/cli.rs`): Clap-based interface with `format` and `dump` subcommands. The `format` command also supports a `--check` flag to verify that code is already formatted. Input can come from files, strings (`-e`/`-E`), or stdin.

**Crate Structure** (`src/main.rs`, `src/lib.rs`): The project is a mixed binary/library crate.

- `src/lib.rs`: The library root, containing the core parsing and formatting logic and exposing public APIs like `format_perl`.
- `src/main.rs`: The binary entry point, which parses command-line arguments via `src/cli.rs` and calls the library functions.

### Module Structure

#### Parser Structure (`src/parser/`)

- `mod.rs`: The main parser module, defining the `Parser` struct and core parsing loop.
- `statement.rs`: Handles statement-level parsing (e.g., `if`, `while`, `sub`).
- `expression/mod.rs`: Manages expression parsing using a Pratt parser.
- `expression/precedence.rs`: Defines operator precedence and associativity.
- `expression/primary.rs`: Parses primary expressions like variables, literals, and parenthesized expressions.
- `expression/quoted.rs`: Handles complex quote-like operators.

#### Formatter Structure (`src/formatter/`)

- `mod.rs`: The main formatter module, responsible for traversing the CST.
- `expression.rs`, `literal.rs`: Handle formatting for specific syntax node types.
- `spacing.rs`, `whitespace.rs`: Manage whitespace and spacing rules.
- `verbatim.rs`: Preserves sections that should not be formatted.

## Development Guidelines

### Pre-commit Checks

```bash
# Format code
cargo fmt

# Run linter
cargo clippy

# Run all tests
cargo test -q
```

### Building and Testing

```bash
# Basic build check
cargo check

# Run all tests (unit tests, integration tests, doc tests)
cargo test -q

# Run with optimizations
cargo build --release

# Run a single test module
cargo test -q parser::tests
cargo test -q formatter::tests

# Run specific test
cargo test -q test_var_decl_formatting
```

### CLI Usage

```bash
# Format a Perl program (outputs to stdout)
cargo run -- format -e 'my $var=1;'
cargo run -- format input.pl

# Dump the parsed CST for debugging
cargo run -- dump -e 'my $var=1;'
cargo run -- dump input.pl

# Check if a file is already formatted (exits with non-zero if not)
cargo run -- format --check input.pl
```

Use `-E` instead of `-e` to use character escapes in the input string. e.g. `-E 'sub foo {\n\twarn;\n}'`.

### Testing Strategy

Our testing strategy prioritizes end-to-end formatting correctness and maintainability by focusing on snapshot tests.

- **Primary: Formatter Snapshot Tests (`src/formatter/tests.rs`)**

  - These are the most important tests, acting as integration tests that cover the entire process from lexing and parsing to final output.
  - They verify the formatter's output against stored snapshots using `insta`.
  - The goal is to have a comprehensive suite of snapshot tests that cover a wide range of valid Perl syntax and formatting edge cases.

- **Secondary: Parser Unit Tests (`src/parser/mod.rs`)**

  - Dedicated lexer tests have been removed as the lexer now contains little custom logic.
  - Any tricky lexical edge cases should be covered through parser tests or formatter snapshots.
  - Keep parser unit tests focused on scenarios that are difficult to express as formatter snapshots.

- **Integration Tests (`tests/` directory)**: Verifies CLI behavior, file I/O, and end-to-end functionality using real-world or complex examples.

This approach ensures that our tests are both effective and efficient, focusing developer effort on creating robust, real-world formatting scenarios rather than on redundant, low-level unit tests.

### Adding New Syntax Support

1. Add new variants to the `SyntaxKind` enum in `src/syntax_kind.rs`.
2. Update the lexer in `src/lexer/mod.rs`. This may involve adding a new `Token` variant, updating regexes, or extending `Lexer::disambiguate` with new contextual rules.
3. Add parsing logic to the appropriate parser function in `src/parser/`. For expressions, this may involve adding a new `OperatorInfo` in `src/parser/expression/precedence.rs`.
4. Update the formatter in `src/formatter/` by adding a new `format_...` function to handle the new syntax node.
5. Add comprehensive snapshot tests in `src/formatter/tests.rs` to cover various use cases of the new syntax.

### Parser Function Pattern

```rust
fn parse_construct(&mut self) {
    self.builder.start_node(SyntaxKind::CONSTRUCT.into());
    // ... parsing logic ...
    self.builder.finish_node();
}
```
