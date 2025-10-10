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
            self.parse_sub_prototype();
            self.skip_whitespace_and_newlines();
        }

        while self.at(T![:]) {
            self.parse_sub_attribute();
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
}
