use crate::parser::expression::precedence::Precedence;
use crate::parser::Parser;
use crate::{lexer::LexContext, SyntaxKind, T};

enum SubParenKind {
    Prototype,
    Signature,
}

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
        let mut parsed_signature = false;

        if self.at(T!['(']) {
            match self.classify_sub_paren_contents() {
                SubParenKind::Prototype => {
                    self.parse_sub_prototype();
                    self.skip_whitespace_and_newlines();
                }
                SubParenKind::Signature => {
                    self.parse_sub_signature();
                    self.skip_whitespace_and_newlines();
                    parsed_signature = true;
                }
            }
        }

        while self.at(T![:]) {
            self.parse_sub_attribute();
            self.skip_whitespace_and_newlines();
        }

        self.skip_whitespace_and_newlines();

        if !parsed_signature && self.at(T!['(']) {
            self.parse_sub_signature();
            self.skip_whitespace_and_newlines();
        }

        if self.at(T![;]) {
            self.bump();
        } else if self.at(T!['{']) {
            self.block();
        } else {
            self.error("Expected block or ';' after subroutine declaration");
        }
    }

    pub(crate) fn parse_sub_attribute(&mut self) {
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
        self.skip_whitespace_and_newlines();

        if !self.at(T![')']) {
            if !self.expression_list() {
                self.error("Expected expression list in attribute arguments");
            }
            self.skip_whitespace_and_newlines();
        }

        self.expect_op(T![')']);

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

    fn classify_sub_paren_contents(&self) -> SubParenKind {
        if self.looks_like_sub_signature_parens() {
            SubParenKind::Signature
        } else {
            SubParenKind::Prototype
        }
    }

    fn looks_like_sub_signature_parens(&self) -> bool {
        let mut index = 1;
        let mut depth = 0;
        let mut prev_was_sigil = false;
        let mut saw_signature_token = false;

        while let Some((kind, _)) =
            self.peek_nth_non_trivia_token_with_context(LexContext::Value, index)
        {
            match kind {
                T!['('] => {
                    depth += 1;
                    prev_was_sigil = false;
                }
                T![')'] => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    prev_was_sigil = false;
                }
                SyntaxKind::SCALAR_SIGIL | SyntaxKind::ARRAY_SIGIL | SyntaxKind::HASH_SIGIL => {
                    if depth == 0 {
                        prev_was_sigil = true;
                    }
                }
                SyntaxKind::IDENT => {
                    if depth == 0 && prev_was_sigil {
                        saw_signature_token = true;
                    }
                    prev_was_sigil = false;
                }
                _ if kind.is_keyword() => {
                    if depth == 0 && prev_was_sigil {
                        saw_signature_token = true;
                    }
                    prev_was_sigil = false;
                }
                T![=] | T![,] | T![=>] => {
                    if depth == 0 {
                        saw_signature_token = true;
                    }
                    prev_was_sigil = false;
                }
                SyntaxKind::NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::REGEX_LITERAL
                | SyntaxKind::VERSION
                | SyntaxKind::BARE_VERSION => {
                    if depth == 0 && prev_was_sigil {
                        saw_signature_token = true;
                    }
                    prev_was_sigil = false;
                }
                _ => {
                    prev_was_sigil = false;
                }
            }

            index += 1;
        }

        saw_signature_token
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
                    self.bump_value();
                    self.skip_whitespace_and_newlines();

                    if self.at(T![')']) {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        self.expect_op(T![')']);

        self.builder.finish_node();
    }

    fn parse_signature_param(&mut self) {
        self.builder.start_node(SyntaxKind::SIGNATURE_PARAM.into());

        if self.at_any(&[
            SyntaxKind::SCALAR_SIGIL,
            SyntaxKind::ARRAY_SIGIL,
            SyntaxKind::HASH_SIGIL,
        ]) {
            self.bump_value();
        } else {
            self.error("Expected parameter sigil in signature");
            self.recover_signature_param();
            self.builder.finish_node();
            return;
        }

        self.skip_whitespace_and_newlines();

        if self.maybe_signature_identifier() {
            self.skip_whitespace_and_newlines();
        }

        if self.looks_like_signature_default() {
            self.parse_signature_param_default();
        }

        self.builder.finish_node();
    }

    fn recover_signature_param(&mut self) {
        while !self.at_end() && !self.at_any(&[T![,], T![')']]) {
            self.bump();
        }
    }

    fn looks_like_signature_default(&self) -> bool {
        let Some((next_kind, _)) = self.peek_non_trivia_token_with_context(LexContext::Operator)
        else {
            return false;
        };

        if next_kind == T![=] {
            return true;
        }

        if !next_kind.is_compoundable_operator() {
            return false;
        }

        self.peek_nth_non_trivia_token_with_context(LexContext::Operator, 1)
            .is_some_and(|(kind, _)| kind == T![=])
    }

    fn parse_signature_param_default(&mut self) {
        self.builder
            .start_node(SyntaxKind::SIGNATURE_PARAM_DEFAULT.into());

        let Some((next_kind, _)) = self.peek_non_trivia_token_with_context(LexContext::Operator)
        else {
            self.error("Expected assignment operator in signature parameter default");
            self.builder.finish_node();
            return;
        };

        if next_kind == T![=] {
            self.bump_op();
        } else if next_kind.is_compoundable_operator()
            && self
                .peek_nth_non_trivia_token_with_context(LexContext::Operator, 1)
                .is_some_and(|(kind, _)| kind == T![=])
        {
            let checkpoint = self.builder.checkpoint();
            self.bump_op();
            self.builder
                .start_node_at(checkpoint, SyntaxKind::COMPOUND_ASSIGNMENT.into());
            self.bump_op();
            self.builder.finish_node();
        } else {
            self.error("Expected '=' or compound assignment in signature parameter default");
            self.builder.finish_node();
            return;
        }

        self.skip_whitespace_and_newlines();

        if !self.parse_expression_with_precedence(Precedence::ASSIGNMENT) {
            self.error("Expected expression for signature parameter default value");
        }

        self.builder.finish_node();
    }

    fn maybe_signature_identifier(&mut self) -> bool {
        if self.at(SyntaxKind::IDENT) {
            self.bump();
            true
        } else if self
            .current_kind()
            .is_some_and(SyntaxKind::is_keyword)
        {
            self.bump_as(SyntaxKind::IDENT);
            true
        } else if self.try_bump_digit_prefixed_ident() {
            true
        } else if self.at(SyntaxKind::NUMBER) {
            self.bump_as(SyntaxKind::IDENT);
            true
        } else {
            false
        }
    }
}
