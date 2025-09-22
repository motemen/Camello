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
        self.skip_whitespace(); // 改行ではなくスペースのみをスキップ
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
        self.skip_whitespace_and_newlines();
        self.builder.finish_node();
    }

    pub fn parse_variable(&mut self) {
        let sigil = self.current_kind().unwrap();

        // Check if this should be a compound variable
        let is_compound = match sigil {
            SyntaxKind::ARRAY_INDEX_SIGIL => true, // $# variables are always compound
            SyntaxKind::SCALAR_SIGIL | SyntaxKind::ARRAY_SIGIL | SyntaxKind::HASH_SIGIL => {
                // Check if followed by brace or valid dereferencing pattern
                match self
                    .peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 1)
                {
                    Some((SyntaxKind::L_BRACE, _)) => true, // @{expr}, %{expr}, ${expr}
                    Some((SyntaxKind::SCALAR_SIGIL, _)) => {
                        // Peek the third token to ensure a valid variable name follows
                        // to avoid misclassifying special variables like "$$;" as dereferencing
                        let third = self.peek_nth_non_trivia_token_with_context(
                            crate::lexer::LexContext::Value,
                            2,
                        );

                        Self::is_compound_dereference_target(third.map(|(k, _)| k))
                    }
                    _ => false,
                }
            }
            SyntaxKind::TYPEGLOB_SIGIL => {
                // Check if followed by brace or another sigil for compound typeglob variables
                match self
                    .peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 1)
                {
                    Some((SyntaxKind::L_BRACE, _)) => true,     // *{expr}
                    Some((kind, _)) if kind.is_sigil() => true, // *$name, *@name, etc.
                    _ => false,
                }
            }
            _ => false,
        };

        let var_kind = if is_compound {
            SyntaxKind::COMPOUND_VAR
        } else {
            match sigil {
                SyntaxKind::SCALAR_SIGIL => SyntaxKind::SCALAR_VAR,
                SyntaxKind::ARRAY_SIGIL => SyntaxKind::ARRAY_VAR,
                SyntaxKind::HASH_SIGIL => SyntaxKind::HASH_VAR,
                SyntaxKind::TYPEGLOB_SIGIL => SyntaxKind::TYPEGLOB_VAR,
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
                SyntaxKind::ARRAY_INDEX_SIGIL => {
                    // $# variables
                    match self.current_kind() {
                        Some(SyntaxKind::IDENT) => {
                            // $#array_name
                            self.parse_identifier_or_qualified();
                        }
                        Some(SyntaxKind::SCALAR_SIGIL) => {
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
                SyntaxKind::SCALAR_SIGIL | SyntaxKind::ARRAY_SIGIL | SyntaxKind::HASH_SIGIL => {
                    // Handle braced variables and dereferencing
                    match self.current_kind() {
                        Some(SyntaxKind::L_BRACE) => {
                            if self.should_parse_braced_block_in_compound_var() {
                                self.block();
                                self.skip_whitespace_and_newlines();
                            } else {
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
                        }
                        Some(SyntaxKind::SCALAR_SIGIL) => {
                            // Dereferencing: @$ref, %$ref, $$ref
                            self.parse_variable();
                        }
                        _ => {
                            // This shouldn't happen due to our lookahead check
                            self.error("Expected '{' or '$' after compound variable sigil");
                        }
                    }
                }
                SyntaxKind::TYPEGLOB_SIGIL => {
                    // Handle compound typeglob variables: *{expr}, *$name, *@name, etc.
                    match self.current_kind() {
                        Some(SyntaxKind::L_BRACE) => {
                            if self.should_parse_braced_block_in_compound_var() {
                                self.block();
                                self.skip_whitespace_and_newlines();
                            } else {
                                // Braced typeglob: *{expr}
                                self.bump(); // consume {
                                self.skip_whitespace_and_newlines();

                                if !self.expression() {
                                    self.error("Expected expression inside braces after *");
                                }

                                self.skip_whitespace_and_newlines();
                                if self.at(SyntaxKind::R_BRACE) {
                                    self.bump(); // consume }
                                } else {
                                    self.error("Expected '}' to close typeglob braces");
                                }
                            }
                        }
                        Some(kind) if kind.is_sigil() => {
                            // Typeglob with sigil: *$name, *@name, *%name, *&name
                            self.parse_variable();
                        }
                        _ => {
                            // This shouldn't happen due to our lookahead check
                            self.error("Expected '{' or sigil after typeglob '*'");
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
                Some(SyntaxKind::ARRAY_SIGIL) => {
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
            SyntaxKind::SCALAR_SIGIL => SyntaxKind::SCALAR_VAR,
            SyntaxKind::ARRAY_SIGIL => SyntaxKind::ARRAY_VAR,
            SyntaxKind::HASH_SIGIL => SyntaxKind::HASH_VAR,
            SyntaxKind::TYPEGLOB_SIGIL => SyntaxKind::TYPEGLOB_VAR,
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
            SyntaxKind::SCALAR_SIGIL => SyntaxKind::SCALAR_VAR,
            SyntaxKind::ARRAY_SIGIL => SyntaxKind::ARRAY_VAR,
            SyntaxKind::HASH_SIGIL => SyntaxKind::HASH_VAR,
            SyntaxKind::TYPEGLOB_SIGIL => SyntaxKind::TYPEGLOB_VAR,
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
            if current == SyntaxKind::ARRAY_INDEX_SIGIL {
                return false;
            }
        } else {
            return false;
        }
        // Use token-based lookahead to check if next non-trivia token is a dollar sigil or brace
        // Valid dereference patterns are of the form: @$ref, %$ref, $$ref, @{expr}, %{expr}, ${expr}
        match self.peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 1) {
            Some((SyntaxKind::L_BRACE, _)) => true,
            Some((SyntaxKind::SCALAR_SIGIL, _)) => {
                // Peek the third token to ensure a valid variable name follows; avoid misclassifying
                // special variables like "$$;" as dereferencing.
                let third =
                    self.peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 2);
                Self::is_compound_dereference_target(third.map(|(k, _)| k))
            }
            _ => false,
        }
    }

    fn is_compound_dereference_target(lookahead: Option<SyntaxKind>) -> bool {
        // Special variables (e.g. $^FOO) should not be implicitly dereferenced. Only permit
        // tokens that can legally start a normal variable or a braced expression.
        matches!(
            lookahead,
            Some(
                SyntaxKind::IDENT
                    | SyntaxKind::NUMBER
                    | SyntaxKind::ARRAY_SIGIL
                    | SyntaxKind::L_BRACE
            )
        )
    }

    fn should_parse_braced_block_in_compound_var(&self) -> bool {
        if !self.at(SyntaxKind::L_BRACE) {
            return false;
        }

        match self
            .peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 1)
            .map(|(kind, _)| kind)
        {
            Some(SyntaxKind::CARET) => false,
            Some(_) => true,
            None => false,
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
    /// Also supports package names with digits after :: (e.g., Foo::123, Foo::123ABC).
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
                } else if self.try_bump_digit_prefixed_ident() {
                    // Successfully consumed a digit-prefixed identifier (e.g., 123ABC)
                    // Nothing more to do here
                } else if self.at(SyntaxKind::NUMBER) {
                    // Allow pure numbers after :: (e.g., Foo::123) as fallback
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
        self.expect(SyntaxKind::TYPEGLOB_SIGIL);
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
