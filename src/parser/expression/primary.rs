use crate::SyntaxKind;

use super::super::Parser;

impl Parser<'_> {
    pub fn hash_ref(&mut self) {
        self.builder.start_node(SyntaxKind::HASH_REF.into());

        // Opening '{' of anonymous hash; inside expects values
        self.expect_value(SyntaxKind::L_BRACE);
        self.skip_trivia();

        // Parse expressions inside braces - could be key => value pairs or a simple expression list
        if !self.at(SyntaxKind::R_BRACE) {
            self.expression_list();
        }

        self.skip_trivia();
        // After closing '}', expect an operator
        self.expect_op(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    pub fn array_ref(&mut self) {
        self.builder.start_node(SyntaxKind::ARRAY_REF.into());

        // Opening '[' of anonymous array; inside expects values
        self.expect_value(SyntaxKind::L_BRACKET);
        self.skip_trivia();

        // Parse expression list inside brackets (supports trailing comma)
        if !self.at(SyntaxKind::R_BRACKET) {
            self.expression_list();
        }

        self.skip_trivia();
        // After closing ']', expect an operator
        self.expect_op(SyntaxKind::R_BRACKET);
        self.builder.finish_node();
    }

    pub fn heredoc_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.expect_value(SyntaxKind::HEREDOC_START);
        self.pending_heredocs.push_back(checkpoint);
        self.skip_trivia();
    }

    pub fn parse_variable(&mut self) {
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
        self.skip_trivia();

        // Check what comes after the sigil
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
                // Handle ${...} syntax (e.g., ${^NAME})
                self.bump(); // consume {

                // Check for ^ inside braces
                if self.at(SyntaxKind::CARET) {
                    self.bump(); // consume ^
                }

                // Parse identifier inside braces
                if self.at(SyntaxKind::IDENT) {
                    self.bump();
                }

                // Expect closing brace
                if self.at(SyntaxKind::R_BRACE) {
                    self.bump();
                } else {
                    self.error("Expected '}' to close variable name");
                }
            }
            _ => {
                // Check for other punctuation characters that might be tokenized differently
                let text = self.current_text().unwrap_or("");
                if matches!(
                    text,
                    "!" | "?" | "|" | "&" | "`" | "'" | "\"" | "~" | ":" | "\\" | "$"
                ) {
                    // These are punctuation characters like $!, $?, $$, etc. - treat as regular variable names
                    self.bump();
                } else {
                    // Expect an identifier (including qualified identifiers)
                    self.parse_identifier_or_qualified();
                }
            }
        }

        self.builder.finish_node();

        self.skip_trivia();
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
        self.skip_trivia();

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
        self.skip_trivia();

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
        } else {
            return false;
        }

        // Use token-based lookahead to check if next non-trivia token is a dollar sigil or brace
        // Valid dereference patterns are of the form: @$ref, %$ref, $$ref, @{expr}, %{expr}, ${expr}
        matches!(
            self.peek_second_non_trivia_with(crate::lexer::LexContext::Value),
            Some((SyntaxKind::DOLLAR | SyntaxKind::L_BRACE, _))
        )
    }

    /// Parses a dereferencing expression (e.g., @$var, %$var, $$var, @{expr}, %{expr}, ${expr}).
    pub fn parse_dereferencing(&mut self) {
        self.builder.start_node(SyntaxKind::DEREF_EXPR.into());

        // Consume the first sigil (dereference operator)
        self.bump();
        self.skip_trivia();

        // Parse what comes after the dereference sigil
        match self.current_kind() {
            Some(SyntaxKind::DOLLAR) => {
                // Traditional dereferencing: @$var, %$var, $$var
                self.parse_variable();
            }
            Some(SyntaxKind::L_BRACE) => {
                // Expression dereferencing: @{expr}, %{expr}, ${expr}
                self.bump(); // consume {
                self.skip_trivia();

                if !self.expression() {
                    self.error("Expected expression in dereferencing braces");
                }

                if self.at(SyntaxKind::R_BRACE) {
                    self.bump(); // consume }
                    self.skip_trivia();
                } else {
                    self.error("Expected '}' after dereferencing expression");
                }
            }
            _ => {
                self.error("Expected scalar variable (e.g., $ref) or expression in braces (e.g., {expr}) after dereference sigil");
            }
        }

        self.builder.finish_node();
    }

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
        self.skip_trivia();

        // Check for package qualifiers (::)
        if self.at(SyntaxKind::DOUBLE_COLON) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::QUALIFIED_IDENT.into());

            while self.at(SyntaxKind::DOUBLE_COLON) {
                self.bump(); // ::
                self.skip_trivia();

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
            self.skip_trivia(); // Skip trivia after QUALIFIED_IDENT is complete
        }
    }

    /// Parses a reference expression (e.g., \$scalar, \@array, \%hash, \&code)
    pub fn parse_reference_expr(&mut self) {
        self.builder.start_node(SyntaxKind::REFERENCE_EXPR.into());

        // Consume the backslash
        self.bump(); // \
        self.skip_trivia();

        // Parse what comes after the backslash
        match self.current_kind() {
            Some(
                SyntaxKind::DOLLAR | SyntaxKind::AT | SyntaxKind::PERCENT | SyntaxKind::ASTERISK,
            ) => {
                // Reference to a variable: \$scalar, \@array, \%hash, \*typeglob
                self.parse_variable();
            }
            Some(SyntaxKind::AMPERSAND) => {
                // Reference to a function: \&func
                self.bump(); // consume &
                self.skip_trivia();

                self.parse_identifier_or_qualified();
            }
            Some(kind) if kind == SyntaxKind::IDENT || SyntaxKind::is_keyword(kind) => {
                // Reference to a bareword function: \func (shorthand for \&func)
                // Allow keywords as identifiers
                self.parse_identifier_or_qualified();
            }
            Some(SyntaxKind::L_PAREN) => {
                // Reference to parenthesized expression: \(expr)
                self.bump(); // (
                self.skip_trivia();

                if !self.expression() {
                    self.error("Expected expression in reference");
                }

                if self.at(SyntaxKind::R_PAREN) {
                    self.bump(); // )
                    self.skip_trivia();
                } else {
                    self.error("Expected ')' after reference expression");
                }
            }
            _ => {
                self.error(
                    "Expected variable, function name, or parenthesized expression after '\\'",
                );
            }
        }

        self.builder.finish_node();
    }

    /// Parses a typeglob expression (e.g., *{$name}, *STDIN)
    pub fn parse_typeglob_expr(&mut self) {
        self.builder.start_node(SyntaxKind::TYPEGLOB_EXPR.into());

        // Consume the asterisk
        self.expect(SyntaxKind::ASTERISK);
        self.skip_trivia();

        // Check what comes after the asterisk
        match self.current_kind() {
            Some(SyntaxKind::L_BRACE) => {
                // Handle *{expression} syntax
                self.bump(); // consume {
                self.skip_trivia();

                if !self.expression() {
                    self.error("Expected expression in typeglob braces");
                }

                if self.at(SyntaxKind::R_BRACE) {
                    self.bump(); // }
                    self.skip_trivia();
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
