use crate::parser::Parser;
use crate::{SyntaxKind, T};

impl Parser<'_> {
    pub(super) fn handle_postfix_and_semicolon(&mut self) {
        self.skip_whitespace_and_newlines();

        // Handle postfix modifiers (if/unless/for/while/until)
        self.parse_optional_postfix_modifier();

        // Check if semicolon is required
        self.expect_optional_semicolon("expression statement");
    }

    fn parse_optional_postfix_modifier(&mut self) {
        if self.at(T![if]) || self.at(T![unless]) || self.at(T![while]) || self.at(T![until]) || self.at(T![when]) {
            self.parse_postfix_conditional();
        } else if self.at(T![for]) || self.at(T![foreach]) {
            self.parse_postfix_for();
        }
    }

    fn parse_postfix_conditional(&mut self) {
        let keyword_kind = self
            .current_kind()
            .expect("Current token should be if/unless/while/until/when keyword");
        let modifier_kind = match keyword_kind {
            T![if] => SyntaxKind::IF_MODIFIER,
            T![unless] => SyntaxKind::UNLESS_MODIFIER,
            T![while] => SyntaxKind::WHILE_MODIFIER,
            T![until] => SyntaxKind::UNTIL_MODIFIER,
            T![when] => SyntaxKind::WHEN_MODIFIER,
            _ => {
                self.error("Unexpected keyword in postfix conditional");
                return;
            }
        };

        self.builder.start_node(modifier_kind.into());

        // Consume the if/unless/while/until/when keyword; next should be a value (condition)
        self.bump_value();
        self.skip_whitespace_and_newlines();

        // Parse the condition expression
        if !self.expression() {
            self.error("Expected condition after postfix if/unless/while/until/when");
        }

        self.builder.finish_node();
    }

    fn parse_postfix_for(&mut self) {
        self.builder.start_node(SyntaxKind::FOR_MODIFIER.into());

        // Consume the for/foreach keyword; next should be a value (list expression)
        self.bump_value();
        self.skip_whitespace_and_newlines();

        if !self.expression_list() {
            self.error("Expected expression list after postfix for");
        }

        self.builder.finish_node();
    }
}
