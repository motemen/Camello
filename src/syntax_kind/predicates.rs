use super::SyntaxKind;
use crate::T;

impl SyntaxKind {
    #[must_use]
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::COMMENT
        )
    }

    #[must_use]
    pub fn is_whitespace(self) -> bool {
        self == SyntaxKind::WHITESPACE
    }

    #[must_use]
    pub fn is_newline(self) -> bool {
        self == SyntaxKind::NEWLINE
    }

    #[must_use]
    pub fn is_keyword(self) -> bool {
        self.is_phase_block_kw()
            || matches!(
                self,
                SyntaxKind::SUB_KW
                    | SyntaxKind::MY_KW
                    | SyntaxKind::OUR_KW
                    | SyntaxKind::STATE_KW
                    | SyntaxKind::LOCAL_KW
                    | SyntaxKind::IF_KW
                    | SyntaxKind::UNLESS_KW
                    | SyntaxKind::ELSIF_KW
                    | SyntaxKind::ELSE_KW
                    | SyntaxKind::FOR_KW
                    | SyntaxKind::FOREACH_KW
                    | SyntaxKind::WHILE_KW
                    | SyntaxKind::UNTIL_KW
                    | SyntaxKind::PACKAGE_KW
                    | SyntaxKind::QW_KW
                    | SyntaxKind::Q_KW
                    | SyntaxKind::QQ_KW
                    | SyntaxKind::QX_KW
                    | SyntaxKind::M_KW
                    | SyntaxKind::QR_KW
                    | SyntaxKind::S_KW
                    | SyntaxKind::TR_KW
                    | SyntaxKind::Y_KW
                    | SyntaxKind::USE_KW
                    | SyntaxKind::NO_KW
                    | SyntaxKind::REQUIRE_KW
                    | SyntaxKind::RETURN_KW
                    | SyntaxKind::NEXT_KW
                    | SyntaxKind::LAST_KW
                    | SyntaxKind::REDO_KW
                    | SyntaxKind::TRY_KW
                    | SyntaxKind::CATCH_KW
                    | SyntaxKind::FINALLY_KW
                    | SyntaxKind::NOT_KW
                    | SyntaxKind::AND_KW
                    | SyntaxKind::OR_KW
                    | SyntaxKind::XOR_KW
                    | T![eq]
                    | T![ne]
                    | T![gt]
                    | T![lt]
                    | T![ge]
                    | T![le]
                    | T![cmp]
                    | SyntaxKind::X
                    | SyntaxKind::UNDEF_KW
                    | SyntaxKind::END_KW
                    | SyntaxKind::DATA_KW
                    | SyntaxKind::CUT_KW
            )
    }

    #[must_use]
    pub fn is_phase_block_kw(self) -> bool {
        matches!(
            self,
            SyntaxKind::BEGIN_KW
                | SyntaxKind::END_BLOCK_KW
                | SyntaxKind::INIT_KW
                | SyntaxKind::CHECK_KW
                | SyntaxKind::UNITCHECK_KW
        )
    }

    #[must_use]
    pub fn is_phase_block_stmt(self) -> bool {
        self == SyntaxKind::PHASE_BLOCK_STMT
    }

    #[must_use]
    pub fn is_variable(self) -> bool {
        matches!(
            self,
            SyntaxKind::SCALAR_VAR
                | SyntaxKind::ARRAY_VAR
                | SyntaxKind::HASH_VAR
                | SyntaxKind::TYPEGLOB_VAR
        )
    }

    pub fn is_sigil(self) -> bool {
        matches!(
            self,
            SyntaxKind::SCALAR_SIGIL
                | SyntaxKind::ARRAY_INDEX_SIGIL
                | SyntaxKind::ARRAY_SIGIL
                | SyntaxKind::HASH_SIGIL
                | SyntaxKind::BACKSLASH
                | SyntaxKind::TYPEGLOB_SIGIL
        )
    }

    #[must_use]
    pub fn is_operator(self) -> bool {
        matches!(
            self,
            // Assignment and arithmetic
            SyntaxKind::EQ
                | SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::UNARY_PLUS
                | SyntaxKind::UNARY_MINUS
                | SyntaxKind::INCREMENT
                | SyntaxKind::DECREMENT
                | SyntaxKind::PREFIX_INCREMENT
                | SyntaxKind::PREFIX_DECREMENT
                | SyntaxKind::POSTFIX_INCREMENT
                | SyntaxKind::POSTFIX_DECREMENT
                | SyntaxKind::DOT
                | SyntaxKind::ARROW
                | SyntaxKind::FAT_COMMA
                | SyntaxKind::EXPONENT
                | SyntaxKind::STAR
                | SyntaxKind::SLASH
                | SyntaxKind::MODULO
                | SyntaxKind::X
                | SyntaxKind::SHIFT_LEFT
                | SyntaxKind::SHIFT_RIGHT
                | SyntaxKind::RANGE
                | SyntaxKind::RANGE_EXCLUSIVE
                // Comparisons
                | SyntaxKind::GT
                | SyntaxKind::LT
                | SyntaxKind::GE
                | SyntaxKind::LE
                | SyntaxKind::EQ_EQ
                | SyntaxKind::NE
                | SyntaxKind::SMART_MATCH
                | T![eq]
                | T![ne]
                | T![gt]
                | T![lt]
                | T![ge]
                | T![le]
                | T![cmp]
                | SyntaxKind::LOGICAL_AND
                | SyntaxKind::LOGICAL_OR
                | SyntaxKind::LOGICAL_NOT
                | SyntaxKind::NOT_KW
                | SyntaxKind::AND_KW
                | SyntaxKind::OR_KW
                | SyntaxKind::XOR_KW
                | SyntaxKind::DEFINED_OR
                | SyntaxKind::SPACESHIP
                | SyntaxKind::BITWISE_AND
                | SyntaxKind::BITWISE_OR
                | SyntaxKind::BITWISE_XOR
                | SyntaxKind::BITWISE_NOT
                // Regex and file-test
                | SyntaxKind::FILE_TEST_OP
                | SyntaxKind::REGEX_MATCH
                | SyntaxKind::REGEX_NOT_MATCH
                // Misc punctuation used as operators in contexts
                | SyntaxKind::COMMA
                | SyntaxKind::BACKSLASH
                // Postfix deref
                | SyntaxKind::POSTFIX_DEREF_ARRAY
                | SyntaxKind::POSTFIX_DEREF_HASH
                | SyntaxKind::POSTFIX_DEREF_SCALAR
        )
    }

    #[must_use]
    pub fn is_literal(self) -> bool {
        matches!(
            self,
            SyntaxKind::NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::VERSION
                | SyntaxKind::BARE_VERSION
                | SyntaxKind::REGEX_LITERAL
                | SyntaxKind::IO_EXPR
                | SyntaxKind::LITERAL_STRING
                | SyntaxKind::INTERPOLATED_STRING
                | SyntaxKind::REGEX_PATTERN
                | SyntaxKind::QW_STRING
        )
    }

    /// Checks if the token is part of the operator group (e.g., `||`, `&&`, etc.)
    /// Returns true if this token kind can be used as a compound assignment operator (e.g., ||=, &&=, .=, +=, etc.)
    #[must_use]
    pub fn is_compoundable_operator(self) -> bool {
        matches!(
            self,
            SyntaxKind::LOGICAL_AND
                | SyntaxKind::LOGICAL_OR
                | SyntaxKind::DEFINED_OR
                | SyntaxKind::DOT
                | SyntaxKind::BITWISE_AND
                | SyntaxKind::BITWISE_OR
                | SyntaxKind::BITWISE_XOR
                | SyntaxKind::EXPONENT
                | SyntaxKind::SHIFT_LEFT
                | SyntaxKind::SHIFT_RIGHT
                | SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::STAR
                | SyntaxKind::SLASH
                | SyntaxKind::MODULO
                | SyntaxKind::X
        )
    }

    /// Returns true if this token kind contains content that should be indented
    /// when spanning multiple lines (heredocs, strings, etc.)
    #[must_use]
    pub fn is_content_token(self) -> bool {
        matches!(
            self,
            SyntaxKind::HEREDOC_CONTENT
                | SyntaxKind::STRING
                | SyntaxKind::LITERAL_STRING
                | SyntaxKind::INTERPOLATED_STRING
        )
    }

    /// Check if the given SyntaxKind is a quote-like keyword
    #[must_use]
    pub fn is_quote_like_keyword(self) -> bool {
        matches!(
            self,
            SyntaxKind::Q_KW
                | SyntaxKind::QQ_KW
                | SyntaxKind::QX_KW
                | SyntaxKind::QW_KW
                | SyntaxKind::QR_KW
                | SyntaxKind::M_KW
                | SyntaxKind::S_KW
                | SyntaxKind::TR_KW
                | SyntaxKind::Y_KW
        )
    }
}
