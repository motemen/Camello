use crate::parser::Parser;
use crate::{SyntaxKind, T};

impl Parser<'_> {
    pub(super) fn labeled_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::LABELED_STMT.into());

        // Label node: IDENT ':'
        self.builder.start_node(SyntaxKind::LABEL.into());
        self.expect(SyntaxKind::IDENT);
        self.skip_whitespace_and_newlines();
        self.expect(T![:]);
        self.builder.finish_node();

        self.skip_whitespace_and_newlines();

        if !self.statement() {
            self.error("Expected statement after label");
        }

        self.builder.finish_node();
    }

    pub(super) fn phase_block_stmt(&mut self, keyword_kind: SyntaxKind) {
        let name = match keyword_kind {
            T![BEGIN] => "BEGIN",
            T![END] => "END",
            T![INIT] => "INIT",
            T![CHECK] => "CHECK",
            T![UNITCHECK] => "UNITCHECK",
            _ => unreachable!("invalid phase block keyword"),
        };

        self.builder.start_node(SyntaxKind::PHASE_BLOCK_STMT.into());

        self.expect(keyword_kind);
        self.skip_whitespace_and_newlines();

        if self.at(T!['{']) {
            self.block();
        } else {
            self.error(&format!("Expected block after {name}"));
        }

        self.builder.finish_node();
    }

    pub(super) fn package_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::PACKAGE_STMT.into());

        // "package"
        self.expect(T![package]);
        self.skip_whitespace_and_newlines();

        // Package name (qualified identifier); allow keywords as identifiers
        self.parse_identifier_or_qualified();
        self.skip_whitespace_and_newlines();

        // After the package name, parse an optional version
        if self.at_any(&[
            SyntaxKind::VERSION,
            SyntaxKind::BARE_VERSION,
            SyntaxKind::NUMBER,
        ]) {
            self.bump();
            self.skip_whitespace_and_newlines();
        }

        // After the package name and optional version, allow either a terminating semicolon
        // or a block to introduce a scoped package
        if self.at(T![;]) {
            self.bump();
        } else if self.at(T!['{']) {
            // package Foo::Bar { ... }
            self.block();
        } else {
            // Neither a semicolon nor a block – report an error but continue
            self.error("Expected ';' or block after package declaration");
        }

        self.builder.finish_node();
    }

    pub(super) fn use_or_no_stmt(&mut self, is_use: bool) {
        let (keyword_kind, stmt_kind) = if is_use {
            (T![use], SyntaxKind::USE_STMT)
        } else {
            (T![no], SyntaxKind::NO_STMT)
        };

        self.builder.start_node(stmt_kind.into());

        // "use" or "no"
        self.expect(keyword_kind);
        self.skip_whitespace_and_newlines();

        // VERSION literal or module name (qualified identifier)
        if self.at(SyntaxKind::VERSION) {
            // Version literal (e.g., use v5.42; or no v5.42;)
            self.bump();
        } else if self.at(SyntaxKind::BARE_VERSION) {
            // Bare version literal (e.g., use 5.24.1; or no 5.24.1;)
            self.bump();
        } else if self.at(SyntaxKind::NUMBER) {
            // Simple version number (e.g., use 5; or no 5;)
            self.bump();
        } else {
            // Module name (qualified identifier); allow keywords as identifiers
            self.parse_identifier_or_qualified();
            self.skip_whitespace_and_newlines();

            // Check for optional version after module name
            if self.at(SyntaxKind::VERSION)
                || self.at(SyntaxKind::BARE_VERSION)
                || self.at(SyntaxKind::NUMBER)
            {
                self.bump();
            }
        }
        self.skip_whitespace_and_newlines();

        // Option: import list (e.g., qw()) or comma-separated expressions (x => 1, y => 2)
        if self.is_at_start_of_expression() {
            self.expression_list();
        }

        // Check if semicolon is required
        self.expect_optional_semicolon("use/no statement");

        self.builder.finish_node();
    }

    pub(super) fn ellipsis_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::ELLIPSIS_STMT.into());
        self.bump(); // consume '...'
        self.skip_whitespace_and_newlines();

        self.expect_optional_semicolon("ellipsis statement");

        self.builder.finish_node();
    }

    pub(super) fn empty_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::EMPTY_STMT.into());
        self.bump(); // consume ';'
        self.builder.finish_node();
    }
}
