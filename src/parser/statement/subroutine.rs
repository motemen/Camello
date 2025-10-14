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

    fn looks_like_sub_signature_parens(&self) -> bool {
        if !self.at(T!['(']) {
            return false;
        }

        let mut offset = 1;
        let mut saw_placeholder = false;

        while let Some((kind, _)) =
            self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset)
        {
            match kind {
                T![')'] => return saw_placeholder,
                T![;] => return false,
                SyntaxKind::SCALAR_SIGIL | SyntaxKind::ARRAY_SIGIL | SyntaxKind::HASH_SIGIL => {
                    let Some((next_kind, _)) =
                        self.peek_nth_non_trivia_token_with_context(LexContext::Value, offset + 1)
                    else {
                        return false;
                    };

                    match next_kind {
                        SyntaxKind::IDENT => return true,
                        k if k.is_keyword() => return true,
                        SyntaxKind::NUMBER => return true,
                        T![=] => return true,
                        SyntaxKind::DEFINED_OR | T![||] => {
                            let eq_offset = offset + 2;
                            if self
                                .peek_nth_non_trivia_token_with_context(
                                    LexContext::Operator,
                                    eq_offset,
                                )
                                .is_some_and(|(k, _)| k == T![=])
                            {
                                return true;
                            }
                        }
                        T![,] => {
                            saw_placeholder = true;
                            offset += 2;
                            continue;
                        }
                        T![')'] => return true,
                        T![;] => return false,
                        SyntaxKind::SCALAR_SIGIL
                        | SyntaxKind::ARRAY_SIGIL
                        | SyntaxKind::HASH_SIGIL
                        | SyntaxKind::CODE_SIGIL
                        | SyntaxKind::TYPEGLOB_SIGIL
                        | SyntaxKind::ARRAY_INDEX_SIGIL
                        | T!['[']
                        | T!['('] => return false,
                        _ => return true,
                    }
                }
                _ => return false,
            }
        }

        false
    }

    fn parse_sub_signature(&mut self) {
        self.builder.start_node(SyntaxKind::SUB_SIGNATURE.into());

        self.expect(T!['(']);
        self.skip_whitespace_and_newlines();

        let mut first_param = true;
        while !self.at_end() && !self.at(T![')']) {
            if !first_param {
                if self.at(T![,]) {
                    self.bump();
                    self.skip_whitespace_and_newlines();

                    if self.at(T![')']) {
                        break;
                    }
                } else {
                    self.error_without_consuming("Expected ',' between signature parameters");
                }
            }

            self.parse_signature_param();
            self.skip_whitespace_and_newlines();
            first_param = false;
        }

        self.expect(T![')']);
        self.builder.finish_node();
    }

    fn parse_signature_param(&mut self) {
        self.builder.start_node(SyntaxKind::SIGNATURE_PARAM.into());

        match self.current_kind() {
            Some(SyntaxKind::SCALAR_SIGIL | SyntaxKind::ARRAY_SIGIL | SyntaxKind::HASH_SIGIL) => {
                self.bump();
            }
            _ => {
                self.error("Expected signature parameter");
                self.builder.finish_node();
                return;
            }
        }

        self.skip_whitespace_and_newlines();

        if self.at(SyntaxKind::IDENT) {
            self.bump();
        } else if self.current_kind().is_some_and(SyntaxKind::is_keyword) {
            self.bump_as(SyntaxKind::IDENT);
        } else if self.at(SyntaxKind::NUMBER) {
            self.error("Signature parameter names must start with a letter or underscore");
        } else if self.at(T![-]) {
            self.error("Invalid character in signature parameter name");
            if self.at(SyntaxKind::IDENT) {
                self.bump();
            }
        }

        self.skip_whitespace_and_newlines();
        self.parse_signature_param_default();

        self.builder.finish_node();
    }

    fn parse_signature_param_default(&mut self) {
        let Some((operator_kind, _)) =
            self.peek_non_trivia_token_with_context(LexContext::Operator)
        else {
            return;
        };

        enum DefaultKind {
            Simple,
            DefinedOr,
            LogicalOr,
        }

        let default_kind = match operator_kind {
            T![=] => Some(DefaultKind::Simple),
            SyntaxKind::DEFINED_OR => Some(DefaultKind::DefinedOr),
            T![||] => Some(DefaultKind::LogicalOr),
            _ => None,
        };

        let Some(kind) = default_kind else {
            return;
        };

        self.builder
            .start_node(SyntaxKind::SIGNATURE_DEFAULT.into());

        match kind {
            DefaultKind::Simple => {
                self.expect_op(T![=]);
            }
            DefaultKind::DefinedOr => {
                self.expect_op(SyntaxKind::DEFINED_OR);
                self.expect_op(T![=]);
            }
            DefaultKind::LogicalOr => {
                self.expect_op(T![||]);
                self.expect_op(T![=]);
            }
        }

        self.skip_whitespace_and_newlines();

        if !self.parse_expression_with_precedence(Precedence::ASSIGNMENT) {
            self.error_without_consuming("Expected default value expression");
        }

        self.builder.finish_node();
    }
}
