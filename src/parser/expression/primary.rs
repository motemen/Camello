use crate::SyntaxKind;

use super::super::Parser;

impl Parser<'_> {
    pub fn hash_ref(&mut self) {
        self.builder.start_node(SyntaxKind::HASH_REF.into());

        // Opening '{' of anonymous hash; inside expects values
        self.expect_value(SyntaxKind::L_BRACE);
        self.skip_whitespace_and_newlines();

        // Parse expressions inside braces - could be key => value pairs or a simple expression list
        if !self.at(SyntaxKind::R_BRACE) {
            self.expression_list();
        }

        self.skip_whitespace_and_newlines();
        // After closing '}', expect an operator
        self.expect_op(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    pub fn array_ref(&mut self) {
        self.builder.start_node(SyntaxKind::ARRAY_REF.into());

        // Opening '[' of anonymous array; inside expects values
        self.expect_value(SyntaxKind::L_BRACKET);
        self.skip_whitespace_and_newlines();

        // Parse expression list inside brackets (supports trailing comma)
        if !self.at(SyntaxKind::R_BRACKET) {
            self.expression_list();
        }

        self.skip_whitespace_and_newlines();
        // After closing ']', expect an operator
        self.expect_op(SyntaxKind::R_BRACKET);
        self.builder.finish_node();
    }

    pub fn parse_variable(&mut self) {
        let sigil = self.current_kind().unwrap();

        // Check if this should be a compound variable
        let is_compound = match sigil {
            SyntaxKind::DOLLAR_HASH => true, // $# variables are always compound
            SyntaxKind::DOLLAR | SyntaxKind::AT | SyntaxKind::PERCENT => {
                // Check if followed by brace or valid dereferencing pattern
                match self
                    .peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 1)
                {
                    Some((SyntaxKind::L_BRACE, _)) => true, // @{expr}, %{expr}, ${expr}
                    Some((SyntaxKind::DOLLAR, _)) => {
                        // Peek the third token to ensure a valid variable name follows
                        // to avoid misclassifying special variables like "$$;" as dereferencing
                        let third = self.peek_nth_non_trivia_token_with_context(
                            crate::lexer::LexContext::Value,
                            2,
                        );
                        matches!(
                            third.map(|(k, _)| k),
                            Some(
                                SyntaxKind::IDENT
                                    | SyntaxKind::NUMBER
                                    | SyntaxKind::AT
                                    | SyntaxKind::CARET
                                    | SyntaxKind::L_BRACE
                            )
                        )
                    }
                    _ => false,
                }
            }
            _ => false,
        };

        let var_kind = if is_compound {
            SyntaxKind::COMPOUND_VAR
        } else {
            match sigil {
                SyntaxKind::DOLLAR => SyntaxKind::SCALAR_VAR,
                SyntaxKind::AT => SyntaxKind::ARRAY_VAR,
                SyntaxKind::PERCENT => SyntaxKind::HASH_VAR,
                SyntaxKind::ASTERISK => SyntaxKind::TYPEGLOB_VAR,
                _ => unreachable!(),
            }
        };

        self.builder.start_node(var_kind.into());

        // Consume the sigil
        self.bump();
        self.skip_whitespace_and_newlines();

        // Handle compound variable parsing
        if is_compound {
            match sigil {
                SyntaxKind::DOLLAR_HASH => {
                    // $# variables
                    match self.current_kind() {
                        Some(SyntaxKind::IDENT) => {
                            // $#array_name
                            self.parse_identifier_or_qualified();
                        }
                        Some(SyntaxKind::DOLLAR) => {
                            // $#$var
                            self.parse_variable();
                        }
                        Some(SyntaxKind::L_BRACE) => {
                            // $#{...}
                            self.bump(); // consume {

                            if !self.expression() {
                                self.error("Expected expression in $#{...}");
                            }

                            if self.at(SyntaxKind::R_BRACE) {
                                self.bump(); // consume }
                            } else {
                                self.error("Expected '}' after expression in $#{...}");
                            }
                        }
                        _ => {
                            self.error("Expected array name or variable after $#");
                        }
                    }
                }
                SyntaxKind::DOLLAR | SyntaxKind::AT | SyntaxKind::PERCENT => {
                    // Handle braced variables and dereferencing
                    match self.current_kind() {
                        Some(SyntaxKind::L_BRACE) => {
                            // Braced variable: @{expr}, %{expr}, ${expr}
                            self.bump(); // consume {
                            self.skip_whitespace_and_newlines();

                            // Parse the expression inside braces
                            // For ${^MATCH}, this will parse ^MATCH as a primary expression
                            if !self.expression() {
                                self.error("Expected expression inside braces");
                            }

                            self.skip_whitespace_and_newlines();
                            if self.at(SyntaxKind::R_BRACE) {
                                self.bump(); // consume }
                            } else {
                                self.error("Expected '}' to close braced variable");
                            }
                        }
                        Some(SyntaxKind::DOLLAR) => {
                            // Dereferencing: @$ref, %$ref, $$ref
                            self.parse_variable();
                        }
                        _ => {
                            // This shouldn't happen due to our lookahead check
                            self.error("Expected '{' or '$' after compound variable sigil");
                        }
                    }
                }
                _ => unreachable!(),
            }
        } else {
            // Standard variable parsing for simple variables
            match self.current_kind() {
                Some(SyntaxKind::IDENT) => {
                    // Regular identifier or qualified identifier (including $_, $_foo, etc.)
                    self.parse_identifier_or_qualified();
                }
                Some(SyntaxKind::NUMBER) => {
                    // Number like $1, $2, etc. - treat as regular variable name
                    self.bump();
                }
                Some(SyntaxKind::AT) => {
                    // Special punctuation like $@ - treat as regular variable name
                    self.bump();
                }
                Some(SyntaxKind::CARET) => {
                    // Handle $^ or $^X patterns
                    self.bump(); // consume ^

                    // Check if there's a character after ^
                    if self.at(SyntaxKind::IDENT) {
                        // This is $^X pattern where X is an identifier (single char)
                        self.bump();
                    }
                }
                Some(SyntaxKind::L_BRACE) => {
                    // Handle ${...} syntax (e.g., ${^NAME}) as simple variables
                    self.bump(); // consume {
                    self.skip_whitespace_and_newlines();

                    // Check for ^ inside braces
                    if self.at(SyntaxKind::CARET) {
                        self.bump(); // consume ^
                        self.skip_whitespace_and_newlines();
                    }

                    // Parse identifier inside braces - accept keywords as identifiers
                    if self.at(SyntaxKind::IDENT) {
                        self.bump();
                    } else if self.current_kind().is_some_and(SyntaxKind::is_keyword) {
                        self.bump_as(SyntaxKind::IDENT);
                    } else {
                        self.error("Expected identifier inside braces");
                    }

                    self.skip_whitespace_and_newlines();
                    // Expect closing brace
                    if self.at(SyntaxKind::R_BRACE) {
                        self.bump();
                    } else {
                        self.error("Expected '}' to close variable name");
                    }
                }
                Some(SyntaxKind::DOUBLE_COLON) => {
                    // Allow variables like $::foo (root-qualified names)
                    self.bump(); // consume ::
                    self.skip_whitespace_and_newlines();
                    self.parse_identifier_or_qualified();
                }
                _ => {
                    // Check if it's a keyword that should be treated as an identifier
                    if self.current_kind().is_some_and(SyntaxKind::is_keyword) {
                        self.bump_as(SyntaxKind::IDENT);
                    } else if let Some(text) = self.current_text() {
                        // Accept any ASCII punctuation as a valid special variable name by
                        // consuming exactly one character, regardless of the lexer's tokenization.
                        if text
                            .chars()
                            .next()
                            .is_some_and(|ch| ch.is_ascii_punctuation())
                        {
                            // Manually consume one character from the lexer and emit it as IDENT.
                            if let Some((k, t)) = self.lexer.consume_one_char_as_ident() {
                                self.builder.token(k.into(), t);
                                self.current_pos += t.len();
                            } else {
                                self.error("Unexpected end while reading special variable name");
                            }
                        } else {
                            // Expect an identifier (including qualified identifiers)
                            self.parse_identifier_or_qualified();
                        }
                    } else {
                        self.error("Expected variable name after sigil");
                    }
                }
            }
        }

        self.builder.finish_node();

        self.skip_whitespace_and_newlines();
    }

    /// Parses a variable for 'my'/'state' declarations (qualified identifiers are not allowed).  
    pub fn parse_variable_simple(&mut self) {
        let sigil = self.current_kind().unwrap();
        let var_kind = match sigil {
            SyntaxKind::DOLLAR => SyntaxKind::SCALAR_VAR,
            SyntaxKind::AT => SyntaxKind::ARRAY_VAR,
            SyntaxKind::PERCENT => SyntaxKind::HASH_VAR,
            SyntaxKind::ASTERISK => SyntaxKind::TYPEGLOB_VAR,
            _ => unreachable!(),
        };

        self.builder.start_node(var_kind.into());

        // Consume the sigil
        self.bump();
        self.skip_whitespace_and_newlines();

        // Expect an identifier (only simple identifiers, no qualified allowed)
        if self.at(SyntaxKind::IDENT) {
            self.bump();
        } else if self.current_kind().is_some_and(SyntaxKind::is_keyword) {
            self.bump_as(SyntaxKind::IDENT);
        } else {
            self.error("Expected identifier after sigil");
        }

        // Check for :: after identifier - if found, it's a package-qualified name which is not allowed for my/state
        if self.at(SyntaxKind::DOUBLE_COLON) {
            self.error("Package-qualified variable names are not allowed with 'my' or 'state' declarations");
        }

        self.builder.finish_node();
    }

    /// Parses a variable for 'our'/'local' declarations (qualified identifiers are allowed).
    pub fn parse_variable_qualified(&mut self) {
        let sigil = self.current_kind().unwrap();
        let var_kind = match sigil {
            SyntaxKind::DOLLAR => SyntaxKind::SCALAR_VAR,
            SyntaxKind::AT => SyntaxKind::ARRAY_VAR,
            SyntaxKind::PERCENT => SyntaxKind::HASH_VAR,
            SyntaxKind::ASTERISK => SyntaxKind::TYPEGLOB_VAR,
            _ => unreachable!(),
        };

        self.builder.start_node(var_kind.into());

        // Consume the sigil
        self.bump();
        self.skip_whitespace_and_newlines();

        // Expect an identifier (qualified identifiers allowed)
        self.parse_identifier_or_qualified();

        self.builder.finish_node();
    }

    /// Checks if this is a dereferencing pattern (sigil followed by another sigil or brace).
    #[must_use]
    pub fn is_dereferencing_pattern(&self) -> bool {
        // If the current token is not a sigil, it's not a dereference
        if let Some(current) = self.current_kind() {
            if !current.is_sigil() {
                return false;
            }
            // DOLLAR_HASH ($#) is not a dereferencing sigil, it's for array last index
            if current == SyntaxKind::DOLLAR_HASH {
                return false;
            }
        } else {
            return false;
        }
        // Use token-based lookahead to check if next non-trivia token is a dollar sigil or brace
        // Valid dereference patterns are of the form: @$ref, %$ref, $$ref, @{expr}, %{expr}, ${expr}
        match self.peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 1) {
            Some((SyntaxKind::L_BRACE, _)) => true,
            Some((SyntaxKind::DOLLAR, _)) => {
                // Peek the third token to ensure a valid variable name follows; avoid misclassifying
                // special variables like "$$;" as dereferencing.
                let third =
                    self.peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 2);
                matches!(
                    third.map(|(k, _)| k),
                    Some(
                        SyntaxKind::IDENT
                            | SyntaxKind::NUMBER
                            | SyntaxKind::AT
                            | SyntaxKind::CARET
                            | SyntaxKind::L_BRACE
                    )
                )
            }
            _ => false,
        }
    }

    /// Checks if we are currently inside hash braces (for treating keywords as identifiers).
    #[must_use]
    pub fn is_inside_hash_braces(&self) -> bool {
        // For now, we check if the next non-whitespace token is a closing brace
        // This is a simple heuristic that covers the common case of $h->{keyword}
        // where 'keyword' should be treated as IDENT

        // Check if we have closing brace next (possibly after whitespace/newlines)
        // This suggests we're a single token inside braces, which is typically a hash key
        self.peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 1)
            .is_some_and(|(kind, _)| kind == SyntaxKind::R_BRACE)
    }

    // This function is no longer needed - compound variables are now handled in parse_variable()

    /// Parses a regular identifier or qualified identifier, accepting keywords as identifiers
    /// in identifier-expected positions. Examples: Foo, Foo::Bar, Foo::Bar::Baz, and keywords
    /// like `else` when grammar expects an identifier (e.g., `sub else {}` or `use if`).
    pub fn parse_identifier_or_qualified(&mut self) {
        // Accept IDENT or coerce a keyword into IDENT at identifier positions
        let checkpoint = self.builder.checkpoint();
        if self.at(SyntaxKind::IDENT) {
            self.bump(); // First identifier
        } else if self.current_kind().is_some_and(SyntaxKind::is_keyword) {
            self.bump_as(SyntaxKind::IDENT);
        } else {
            self.error("Expected identifier");
            return;
        }
        self.skip_whitespace_and_newlines();

        // Check for package qualifiers (::)
        if self.at(SyntaxKind::DOUBLE_COLON) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::QUALIFIED_IDENT.into());

            while self.at(SyntaxKind::DOUBLE_COLON) {
                self.bump(); // ::
                self.skip_whitespace_and_newlines();

                if self.at(SyntaxKind::IDENT)
                    || self.current_kind().is_some_and(SyntaxKind::is_keyword)
                {
                    // Coerce subsequent segments to IDENT as needed
                    self.bump_as(SyntaxKind::IDENT);
                } else {
                    // A trailing `::` is valid, so we don't report an error, just stop parsing the qualified name.
                    break;
                }
            }
            self.builder.finish_node();
            self.skip_whitespace_and_newlines(); // Skip trivia after QUALIFIED_IDENT is complete
        }
    }

    /// Parses a typeglob expression (e.g., *{$name}, *STDIN)
    pub fn parse_typeglob_expr(&mut self) {
        self.builder.start_node(SyntaxKind::TYPEGLOB_EXPR.into());

        // Consume the asterisk
        self.expect(SyntaxKind::ASTERISK);
        self.skip_whitespace_and_newlines();

        // Check what comes after the asterisk
        match self.current_kind() {
            Some(SyntaxKind::L_BRACE) => {
                // Handle *{expression} syntax
                self.bump(); // consume {
                self.skip_whitespace_and_newlines();

                if !self.expression() {
                    self.error("Expected expression in typeglob braces");
                }

                if self.at(SyntaxKind::R_BRACE) {
                    self.bump(); // }
                    self.skip_whitespace_and_newlines();
                } else {
                    self.error("Expected '}' after typeglob expression");
                }
            }
            Some(kind) if kind == SyntaxKind::IDENT || SyntaxKind::is_keyword(kind) => {
                // Handle *STDIN syntax (simple identifier), allow keywords as names
                self.parse_identifier_or_qualified();
            }
            _ => {
                self.error("Expected '{' or identifier after '*' in typeglob expression");
            }
        }

        self.builder.finish_node();
    }
}
