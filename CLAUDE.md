# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Camello is a Perl code formatter built in Rust using the Rowan library for CST (Concrete Syntax Tree) operations. It provides lossless formatting that preserves comments and whitespace while applying consistent coding styles.

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
# Format a Perl file (outputs to stdout)
cargo run -- format input.pl

# Check if file is already formatted
cargo run -- format --check input.pl

# Format and save to specific output file
cargo run -- format input.pl -o output.pl
```

### Development Tools
```bash
# Update snapshot tests (when output format changes intentionally)
cargo test -- --update-snapshots

# Generate documentation
cargo doc --open
```

## Architecture Overview

### Data Flow
```
Perl Source [Lexer] Tokens [Parser] CST [Formatter] Formatted Code
```

### Core Components

**SyntaxKind** (`src/syntax_kind.rs`): Central enum defining all Perl syntax elements. Uses `#[repr(u16)]` for efficient Rowan integration.

**Lexer** (`src/lexer.rs`): Logos-based tokenizer that handles Perl-specific tokens including variables (`$var`, `@array`, `%hash`), keywords (`sub`, `my`), and contextual keyword resolution.

**Parser** (`src/parser.rs`): Recursive descent parser using Rowan's GreenNodeBuilder. Key functions:
- `root()` - parses entire file
- `statement()` - handles different statement types  
- `var_decl()` - variable declarations (`my $var = 1;`)
- `sub_def()` - subroutine definitions (`sub name { ... }`)
- `expression()` - expression parsing with operator precedence

**Formatter** (`src/formatter.rs`): CST traversal that applies formatting rules:
- 4-space indentation for blocks
- Space around operators (`$a = $b + $c`)
- K&R brace style (`sub name {`)
- Preserves comments and handles trivia appropriately

**CLI** (`src/cli.rs`): Clap-based interface supporting format, check, and output redirection.

### Rowan Integration

The project uses a custom `PerlLanguage` type implementing Rowan's `Language` trait. SyntaxKind conversion is handled via `From<SyntaxKind> for rowan::SyntaxKind`.

### Error Recovery

Parser implements multiple error recovery strategies:
1. **Token-level**: Skip invalid tokens as ERROR nodes
2. **Statement-level**: Recover to semicolons or braces
3. **Structure-level**: Continue parsing after encountering unknown constructs

## Key Implementation Notes

### Adding New Syntax Support

1. Add new variants to `SyntaxKind` enum
2. Update lexer regex patterns in `Token` enum  
3. Add parsing logic in appropriate parser functions
4. Update formatter to handle new syntax elements
5. Add comprehensive tests

### Parser Function Pattern
```rust
fn parse_construct(&mut self) {
    self.builder.start_node(SyntaxKind::CONSTRUCT.into());
    // ... parse logic
    self.builder.finish_node();
}
```

### Testing Strategy

- **Unit tests**: Individual component functionality
- **Snapshot tests**: Complete formatting verification using `insta`
- **Integration tests**: CLI behavior and file I/O
- Compare the whole output when testing format instead of using `.contains()`

Current test coverage includes basic Perl constructs: variable declarations, subroutine definitions, binary expressions, and proper indentation handling.

### Future Development Areas

1. **Extended Perl syntax**: Control structures (if/else, loops), complex expressions
2. **Configuration system**: `.camellorc` support for customizable formatting rules  
3. **Performance optimization**: Large file handling, incremental parsing
4. **Error messaging**: More descriptive parse error reporting

# Notes on development

* Follow conventional commits
* Use English in commit messages
* Use `cargo fmt` before commit
* Use `cargo clippy --fix` before commit
* Use `cargo run -- format -e '...'` to test behavior
