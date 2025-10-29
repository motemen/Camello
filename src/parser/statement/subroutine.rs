use crate::parser::expression::precedence::Precedence;
use crate::parser::Parser;
use crate::{lexer::LexContext, SyntaxKind, T};

impl Parser<'_> {
    pub(super) fn looks_like_sub_definition(&self) -> bool {
        match self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1) {
            Some((next, _)) => next == T![::] || next == SyntaxKind::IDENT || next.is_keyword(),
            None => false,
        }
    }

    pub(super) fn looks_like_lexical_sub_definition(&self) -> bool {
        self.peek_nth_non_trivia_token_with_context(LexContext::Value, 1)
            .is_some_and(|(next, _)| next == T![sub])
    }

    pub(super) fn sub_def_with_modifier(&mut self, modifier_kind: Option<SyntaxKind>) {
        self.builder.start_node(SyntaxKind::SUB_DEF.into());

        if let Some(kind) = modifier_kind {
            self.expect(kind);
            self.skip_whitespace_and_newlines();
        }

        self.expect(T![sub]);
        self.skip_whitespace_and_newlines();

        // Subroutine name (qualified identifier also allowed); keywords accepted as identifiers
        self.parse_identifier_or_qualified();
        self.skip_whitespace_and_newlines();

        self.parse_sub_tail();

        self.builder.finish_node();
    }

    pub(crate) fn parse_sub_tail(&mut self) {
        if self.at(T!['(']) {
            if self.looks_like_sub_signature_parens() {
                self.parse_sub_signature();
            } else {
                self.parse_sub_prototype();
            }
            self.skip_whitespace_and_newlines();
        }

        while self.at(T![:]) {
            self.parse_attribute();
            self.skip_whitespace_and_newlines();
        }

        self.skip_whitespace_and_newlines();

        if self.at(T![;]) {
            self.bump();
        } else if self.at(T!['{']) {
            self.block();
        } else {
            self.error("Expected block or ';' after subroutine declaration");
        }
    }

    pub(crate) fn parse_attribute(&mut self) {
        self.builder.start_node(SyntaxKind::ATTR.into());

        self.expect(T![:]);
        self.skip_whitespace_and_newlines();

        loop {
            let can_start_identifier = self.at(SyntaxKind::IDENT)
                || self.current_kind().is_some_and(SyntaxKind::is_keyword);

            if !can_start_identifier {
                break;
            }

            self.parse_identifier_or_qualified();
            self.skip_whitespace_and_newlines();

            if self.at(T!['(']) {
                self.parse_attr_args();
                self.skip_whitespace_and_newlines();
            }
        }

        self.builder.finish_node();
    }

    fn parse_attr_args(&mut self) {
        self.builder.start_node(SyntaxKind::ATTR_ARGS.into());

        self.expect(T!['(']);

        // Attribute arguments only check for balanced parentheses, not expression validity
        // See perlsub: "They may have a parameter list appended, which is only checked
        // for whether its parentheses ('(',')') nest properly."
        // The lexer handles consuming balanced parentheses as a single RAW_STRING token.
        if let Some((kind, text)) = self.lexer.consume_balanced_parens() {
            self.builder.token(kind.into(), text);
            self.current_pos += text.len();
        }

        self.expect(T![')']);

        self.builder.finish_node();
    }

    /// Parse subroutine prototype like (\@@), ($@), (\@$@), etc.
    pub(crate) fn parse_sub_prototype(&mut self) {
        self.builder.start_node(SyntaxKind::SUB_PROTOTYPE.into());

        self.expect(T!['(']);
        self.skip_whitespace_and_newlines();

        while let Some((kind, _)) = self.peek_non_trivia_token_with_context(LexContext::Value) {
            if kind == T![')'] {
                break;
            }
            match kind {
                T!['\\']
                | SyntaxKind::ARRAY_SIGIL
                | SyntaxKind::HASH_SIGIL
                | SyntaxKind::SCALAR_SIGIL
                | SyntaxKind::CODE_SIGIL
                | SyntaxKind::TYPEGLOB_SIGIL
                | T![;]
                | T!['[']
                | T![']']
                | T!['(']
                | T![')'] => {
                    self.bump_with_context(LexContext::Value);
                    self.skip_whitespace_and_newlines();
                }
                _ => {
                    self.error("Invalid character in subroutine prototype");
                    break;
                }
            }
        }

        self.expect(T![')']);
        self.builder.finish_node();
    }

    pub(crate) fn looks_like_sub_signature_parens(&self) -> bool {
        let mut iter = match self.lexer.iter_non_trivia_from(LexContext::Value, 0) {
            Some(iter) => iter.peekable(),
            None => return false,
        };

        match iter.next() {
            Some((T!['('], _)) => {}
            _ => return false,
        }

        while let Some((kind, _text)) = iter.next() {
            match kind {
                T![')'] => break,
                T![,] => continue,
                T![;] | T!['\\'] | T!['['] | T![']'] => return false,
                SyntaxKind::CODE_SIGIL | SyntaxKind::TYPEGLOB_SIGIL => return false,
                SyntaxKind::SCALAR_SIGIL | SyntaxKind::ARRAY_SIGIL | SyntaxKind::HASH_SIGIL => {
                    if let Some(&(next_kind, _next_text)) = iter.peek() {
                        match next_kind {
                            SyntaxKind::IDENT => return true,
                            _ if next_kind.is_keyword() => return true,
                            SyntaxKind::NUMBER => return true,
                            T![,] | T![')'] => return true,
                            T![=] => return true,
                            SyntaxKind::DEFINED_OR | SyntaxKind::LOGICAL_OR => return true,
                            SyntaxKind::SCALAR_SIGIL
                            | SyntaxKind::ARRAY_SIGIL
                            | SyntaxKind::HASH_SIGIL
                            | SyntaxKind::CODE_SIGIL
                            | SyntaxKind::TYPEGLOB_SIGIL
                            | T!['\\'] => return false,
                            T![;] => return false,
                            _ => {}
                        }
                    } else {
                        break;
                    }
                }
                _ => {}
            }
        }

        false
    }

    fn parse_sub_signature(&mut self) {
        self.builder.start_node(SyntaxKind::SUB_SIGNATURE.into());

        self.expect(T!['(']);
        self.skip_whitespace_and_newlines();

        if !self.at(T![')']) {
            loop {
                self.parse_signature_param();
                self.skip_whitespace_and_newlines();

                if self.at(T![,]) {
                    self.bump();
                    self.skip_whitespace_and_newlines();

                    if self.at(T![')']) {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        self.expect(T![')']);
        self.builder.finish_node();
    }

    fn parse_signature_param(&mut self) {
        self.builder.start_node(SyntaxKind::SIGNATURE_PARAM.into());

        if !self.current_kind().is_some_and(|kind| {
            matches!(
                kind,
                SyntaxKind::SCALAR_SIGIL | SyntaxKind::ARRAY_SIGIL | SyntaxKind::HASH_SIGIL
            )
        }) {
            self.error("Expected parameter sigil in subroutine signature");
            while !self.at_end() && !self.at(T![,]) && !self.at(T![')']) {
                self.bump();
            }
            self.builder.finish_node();
            return;
        }

        self.bump();
        self.skip_whitespace_and_newlines();

        if let Some(kind) = self.current_kind() {
            if Self::is_signature_placeholder_boundary(kind) {
                // Placeholder parameter (e.g., $ or @)
            } else if kind == SyntaxKind::NUMBER {
                self.error_without_consuming("Invalid parameter name in signature");
                self.bump();
            } else if kind == T![-] {
                self.error_without_consuming("Invalid parameter name in signature");
                self.bump();
                self.skip_whitespace_and_newlines();
                if self
                    .current_kind()
                    .is_some_and(|next| next == SyntaxKind::IDENT || next.is_keyword())
                {
                    self.bump();
                }
            } else if kind == SyntaxKind::IDENT || kind.is_keyword() {
                let ident_text = self.current_text().unwrap_or("");
                if Self::is_valid_signature_identifier(ident_text) {
                    self.bump();
                } else {
                    self.error_without_consuming("Invalid parameter name in signature");
                    self.bump();
                }
            }
        }

        self.skip_whitespace_and_newlines();

        let mut default_operator = None;

        if let Some((op_kind, _)) = self.peek_non_trivia_token_with_context(LexContext::Operator) {
            if op_kind == T![=] {
                default_operator = Some(SignatureDefaultOperator::Simple);
            } else if op_kind.is_compoundable_operator()
                && self
                    .peek_nth_non_trivia_token_with_context(LexContext::Operator, 1)
                    .is_some_and(|(next_kind, _)| next_kind == T![=])
            {
                default_operator = Some(SignatureDefaultOperator::Compound(op_kind));
            }
        }

        if let Some(SignatureDefaultOperator::Compound(kind)) = default_operator {
            if kind != SyntaxKind::DEFINED_OR && kind != SyntaxKind::LOGICAL_OR {
                self.error_without_consuming(
                    "Only '=', '//=', and '||=' defaults are supported in subroutine signatures",
                );
            }
        }

        if let Some(operator) = default_operator {
            self.parse_signature_default(operator);
        }

        self.skip_whitespace_and_newlines();

        self.builder.finish_node();
    }

    fn parse_signature_default(&mut self, operator: SignatureDefaultOperator) {
        self.builder
            .start_node(SyntaxKind::SIGNATURE_DEFAULT.into());

        match operator {
            SignatureDefaultOperator::Simple => {
                self.bump_with_context(LexContext::Operator);
            }
            SignatureDefaultOperator::Compound(_) => {
                self.bump_with_context(LexContext::Operator);
                self.skip_whitespace_and_newlines();
                self.expect_with_context(T![=], LexContext::Operator);
            }
        }

        self.skip_whitespace_and_newlines();

        if !self.parse_expression_with_precedence(Precedence::ASSIGNMENT) {
            self.error("Expected default value expression in signature parameter");
        }

        self.builder.finish_node();
    }

    fn is_signature_placeholder_boundary(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            T![,] | T![')'] | T![=] | SyntaxKind::DEFINED_OR | SyntaxKind::LOGICAL_OR
        )
    }

    fn is_valid_signature_identifier(name: &str) -> bool {
        let mut chars = name.chars();
        match chars.next() {
            Some(first) if first == '_' || first.is_ascii_alphabetic() => {
                chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
            }
            _ => false,
        }
    }
}

enum SignatureDefaultOperator {
    Simple,
    Compound(SyntaxKind),
}
