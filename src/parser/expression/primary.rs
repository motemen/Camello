use crate::SyntaxKind;

use super::super::Parser;

impl<'a> Parser<'a> {
    pub fn hash_ref(&mut self) {
        self.builder.start_node(SyntaxKind::HASH_REF.into());

        self.expect(SyntaxKind::L_BRACE);
        self.skip_trivia();

        // Parse expressions inside braces - could be key => value pairs or a simple expression list
        if !self.at(SyntaxKind::R_BRACE) {
            self.expression_list();
        }

        self.skip_trivia();
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    pub fn array_ref(&mut self) {
        self.builder.start_node(SyntaxKind::ARRAY_REF.into());

        self.expect(SyntaxKind::L_BRACKET);
        self.skip_trivia();

        // Parse expression list inside brackets (supports trailing comma)
        if !self.at(SyntaxKind::R_BRACKET) {
            self.expression_list();
        }

        self.skip_trivia();
        self.expect(SyntaxKind::R_BRACKET);
        self.builder.finish_node();
    }

    pub fn parse_variable(&mut self) {
        let sigil = self.current_kind().unwrap();
        let var_kind = match sigil {
            SyntaxKind::DOLLAR => SyntaxKind::SCALAR_VAR,
            SyntaxKind::AT => SyntaxKind::ARRAY_VAR,
            SyntaxKind::PERCENT => SyntaxKind::HASH_VAR,
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

    /// 'my'/'state' 宣言用の変数をパースする（修飾識別子は使わない）。  
    pub fn parse_variable_simple(&mut self) {
        let sigil = self.current_kind().unwrap();
        let var_kind = match sigil {
            SyntaxKind::DOLLAR => SyntaxKind::SCALAR_VAR,
            SyntaxKind::AT => SyntaxKind::ARRAY_VAR,
            SyntaxKind::PERCENT => SyntaxKind::HASH_VAR,
            _ => unreachable!(),
        };

        self.builder.start_node(var_kind.into());

        // Consume the sigil
        self.bump();
        self.skip_trivia();

        // Expect an identifier (only simple identifiers, no qualified allowed)
        if self.at(SyntaxKind::IDENT) {
            self.bump();

            // Check for :: after identifier - if found, it's a package-qualified name which is not allowed for my/state
            if self.at(SyntaxKind::DOUBLE_COLON) {
                self.error("Package-qualified variable names are not allowed with 'my' or 'state' declarations");
            }
        } else {
            self.error("Expected identifier after sigil");
        }

        self.builder.finish_node();
    }

    /// our/local 宣言用の変数をパースする（修飾識別子は許可される）
    pub fn parse_variable_qualified(&mut self) {
        let sigil = self.current_kind().unwrap();
        let var_kind = match sigil {
            SyntaxKind::DOLLAR => SyntaxKind::SCALAR_VAR,
            SyntaxKind::AT => SyntaxKind::ARRAY_VAR,
            SyntaxKind::PERCENT => SyntaxKind::HASH_VAR,
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

    /// これはデリファレンスパターンかどうかをチェックする（シジルの後にシジルが続く場合）
    pub fn is_dereferencing_pattern(&self) -> bool {
        // If the current token is not a sigil, it's not a dereference
        if let Some(current) = self.current_kind() {
            if !current.is_sigil() {
                return false;
            }
        } else {
            return false;
        }

        // Look ahead to the next token (simple implementation)
        // From the current position, check if the first non-trivia token is a sigil
        let current_text = self.current_text().unwrap_or("");
        let remaining_source = &self.source[self.current_pos + current_text.len()..];

        // Skip whitespace
        let trimmed = remaining_source.trim_start();

        // Valid dereference patterns: @$ref, %$ref, $$ref (sigil followed by $)
        // Only $ sigil can be dereferenced, so we check if next token is $
        trimmed.starts_with('$')
    }

    /// デリファレンス式をパースする（例: @$var, %$var, $$var）
    pub fn parse_dereferencing(&mut self) {
        self.builder.start_node(SyntaxKind::DEREF_EXPR.into());

        // Consume the first sigil (dereference operator)
        self.bump();
        self.skip_trivia();

        // Parse the next sigil and the following variable
        if let Some(kind) = self.current_kind() {
            if kind.is_sigil() {
                self.parse_variable();
            } else {
                self.error("Expected variable after dereference sigil");
            }
        } else {
            self.error("Expected variable after dereference sigil");
        }

        self.builder.finish_node();
    }

    /// 通常の識別子または修飾識別子をパースする
    /// 例: "Foo", "Foo::Bar", "Foo::Bar::Baz"
    pub fn parse_identifier_or_qualified(&mut self) {
        // Expect an identifier
        if !self.at(SyntaxKind::IDENT) {
            self.error("Expected identifier");
            return;
        }

        self.bump(); // First identifier
        self.skip_trivia();

        // Check for package qualifiers (::)
        while self.at(SyntaxKind::DOUBLE_COLON) {
            self.bump(); // ::
            self.skip_trivia();

            if self.at(SyntaxKind::IDENT) {
                self.bump(); // Next identifier
                self.skip_trivia();
            } else {
                self.error("Expected identifier after '::'");
                break;
            }
        }
    }
}
