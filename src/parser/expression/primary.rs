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

        let next_token_kind = self
            .peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 1)
            .map(|(kind, _)| kind);

        // Check if this should be a compound variable
        let is_compound = match sigil {
            SyntaxKind::ARRAY_INDEX_SIGIL => true, // $# variables are always compound
            SyntaxKind::SCALAR_SIGIL | SyntaxKind::ARRAY_SIGIL | SyntaxKind::HASH_SIGIL => {
                match next_token_kind {
                    Some(SyntaxKind::L_BRACE) => true, // @{expr}, %{expr}, ${expr}
                    Some(SyntaxKind::SCALAR_SIGIL) => {
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
            SyntaxKind::TYPEGLOB_SIGIL => match next_token_kind {
                Some(SyntaxKind::L_BRACE) => true,     // *{expr}
                Some(kind) if kind.is_sigil() => true, // *$name, *@name, etc.
                _ => false,
            },
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

        if is_compound {
            self.parse_compound_variable_body(sigil);
        } else {
            self.parse_simple_variable_body();
        }

        self.builder.finish_node();

        self.skip_whitespace_and_newlines();
    }

    /// Parses the body of a simple variable, after the sigil has been consumed.
    ///
    /// This handles various forms of simple variables, including:
    /// - `$foo`, `$_`, `$Foo::bar` (identifiers, qualified identifiers)
    /// - `$1`, `$2` (numeric captures)
    /// - `$^A`, `$^T` (special variables with a caret)
    /// - `${...}` (braced variable names, e.g., `${^GLOBAL_VAR}`)
    /// - `$::foo` (root-qualified variables)
    /// - `$;`, `$/` (punctuation-based special variables)
    fn parse_simple_variable_body(&mut self) {
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
                self.parse_keyword_or_special_variable();
            }
        }
    }

    /// Parses a keyword used as a variable name, or a special punctuation-based variable.
    fn parse_keyword_or_special_variable(&mut self) {
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

    /// Parses the body of a compound variable, after the initial sigil has been consumed.
    ///
    /// This is responsible for parsing complex dereferencing structures. The `sigil`
    /// parameter is the initial sigil that was already consumed by the caller.
    ///
    /// This handles syntax such as:
    /// - `$#array`, `$#$ref`, `$#{...}` (array last index)
    /// - `$$ref`, `@$ref`, `%$ref`, `*$ref` (scalar, array, hash, and typeglob dereferencing)
    /// - `${...}`, `@{...}`, `%{...}`, `*{...}` (braced expression dereferencing)
    /// - `*foo{BAR}` (typeglob with bareword key, parsed as a block)
    fn parse_compound_variable_body(&mut self, sigil: SyntaxKind) {
        // Handle compound variable parsing
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
            SyntaxKind::SCALAR_SIGIL
            | SyntaxKind::ARRAY_SIGIL
            | SyntaxKind::HASH_SIGIL
            | SyntaxKind::TYPEGLOB_SIGIL => {
                // Handle braced variables and dereferencing
                match self.current_kind() {
                    Some(SyntaxKind::L_BRACE) => {
                        if self.should_parse_braced_block_in_compound_var() {
                            self.block();
                            self.skip_whitespace_and_newlines();
                        } else {
                            // Braced dereference like @{expr}, %{expr}, ${expr}, *{expr}
                            let (expr_error, brace_error) = if sigil == SyntaxKind::TYPEGLOB_SIGIL {
                                (
                                    "Expected expression inside braces after *",
                                    "Expected '}' to close typeglob braces",
                                )
                            } else {
                                (
                                    "Expected expression inside braces",
                                    "Expected '}' to close braced variable",
                                )
                            };
                            self.parse_braced_expression(expr_error, brace_error);
                        }
                    }
                    Some(kind) if Self::should_recurse_into_compound_var(sigil, kind) => {
                        // Dereferencing like @$ref, %$ref, $$ref or typeglob variants
                        self.parse_variable();
                    }
                    _ => {
                        // This shouldn't happen due to our lookahead check
                        let message = if sigil == SyntaxKind::TYPEGLOB_SIGIL {
                            "Expected '{' or sigil after typeglob '*'"
                        } else {
                            "Expected '{' or '$' after compound variable sigil"
                        };
                        self.error(message);
                    }
                }
            }
            _ => unreachable!(),
        }
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
        match lookahead {
            Some(kind) => {
                kind == SyntaxKind::IDENT
                    || kind == SyntaxKind::NUMBER
                    || kind == SyntaxKind::L_BRACE
                    || kind == SyntaxKind::DOUBLE_COLON  // for qualified names like $::foo
                    || kind.is_keyword() // keywords can be used as variable names
            }
            None => false,
        }
    }

    fn should_recurse_into_compound_var(current_sigil: SyntaxKind, next_kind: SyntaxKind) -> bool {
        match current_sigil {
            SyntaxKind::TYPEGLOB_SIGIL => next_kind.is_sigil(),
            SyntaxKind::SCALAR_SIGIL | SyntaxKind::ARRAY_SIGIL | SyntaxKind::HASH_SIGIL => {
                next_kind == SyntaxKind::SCALAR_SIGIL
            }
            _ => false,
        }
    }

    fn should_parse_braced_block_in_compound_var(&self) -> bool {
        match self
            .peek_nth_non_trivia_token_with_context(crate::lexer::LexContext::Value, 1)
            .map(|(kind, _)| kind)
        {
            Some(SyntaxKind::CARET) => false,
            Some(kind) if kind.is_literal() => false,
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

    /// Helper function to parse braced expressions like {expr}
    fn parse_braced_expression(&mut self, expr_error: &str, brace_error: &str) {
        self.bump(); // consume {
        self.skip_whitespace_and_newlines();

        if !self.expression() {
            self.error(expr_error);
        }

        self.skip_whitespace_and_newlines();
        if self.at(SyntaxKind::R_BRACE) {
            self.bump(); // consume }
        } else {
            self.error(brace_error);
        }
    }
}
