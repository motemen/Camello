//! Semantic predicates over [`TokenKind`] (ADR 0004 §3).
//!
//! These encode grammar knowledge and so cannot be derived from the declaration
//! order in `define_language!`. They are hand-written, but because they are
//! typed on `TokenKind` the old class of mistake — a node kind such as `IO_EXPR`
//! turning up in `is_literal` — is not expressible.

use super::{TokenKind, T};

impl TokenKind {
    /// Can this token begin a term (a primary expression)?
    ///
    /// Used to decide whether a list operator has arguments, and to stop error
    /// recovery from swallowing the start of the next expression.
    #[must_use]
    pub fn can_start_term(self) -> bool {
        if self.is_quote_like_keyword() {
            return true;
        }
        matches!(
            self,
            TokenKind::IDENT
                | TokenKind::NUMBER
                | TokenKind::VERSION
                | TokenKind::STRING
                | TokenKind::HEREDOC_START
                // A bare `/.../` has no keyword; its run starts at the opening
                // delimiter.
                | TokenKind::DELIMITER
                | TokenKind::IO_HANDLE
                | TokenKind::FILE_TEST_OP
                | TokenKind::SCALAR_SIGIL
                | TokenKind::ARRAY_SIGIL
                | TokenKind::ARRAY_INDEX_SIGIL
                | TokenKind::HASH_SIGIL
                | TokenKind::CODE_SIGIL
                | TokenKind::TYPEGLOB_SIGIL
        ) || matches!(
            self,
            T!["("]
                | T!["["]
                | T!["{"]
                | T!["\\"]
                | T!["!"]
                | T!["~"]
                | T!["-"]
                | T!["+"]
                | T!["++"]
                | T!["--"]
                | T!["not"]
                | T!["sub"]
                | T!["do"]
                // `return try { ... } catch { ... };`. The statement form is
                // the common one, but `try` in term position is an expression
                // like any other, and `return`'s argument is where it shows.
                | T!["try"]
                | T!["my"]
                | T!["our"]
                | T!["state"]
                | T!["local"]
                | T!["undef"]
                | T!["return"]
                | T!["require"]
                | T!["..."]
        )
    }

    /// One of `q qq qx qw m qr s tr y`.
    ///
    /// These are keywords only when a term is expected and the next character is
    /// not `=>` or `}` (ADR 0005 §5); the lexer owns that decision.
    #[must_use]
    pub fn is_quote_like_keyword(self) -> bool {
        matches!(
            self,
            T!["q"]
                | T!["qq"]
                | T!["qx"]
                | T!["qw"]
                | T!["m"]
                | T!["qr"]
                | T!["s"]
                | T!["tr"]
                | T!["y"]
        )
    }

    /// A heredoc body, which the parser never consumes.
    ///
    /// The marker `<<EOF` is a token in the expression; the body arrives at the
    /// next line start, in the middle of whatever the expression was doing. The
    /// parser skips it exactly as it skips trivia, and the replayer puts it back
    /// where it was found (ADR 0007 §7).
    #[must_use]
    pub fn is_heredoc_body(self) -> bool {
        matches!(
            self,
            TokenKind::HEREDOC_CONTENT | TokenKind::HEREDOC_END | TokenKind::UNTERMINATED_HEREDOC
        )
    }

    /// Tokens the parser does not see at all.
    #[must_use]
    pub fn is_parser_invisible(self) -> bool {
        self.is_trivia() || self.is_heredoc_body()
    }

    /// A sigil that introduces a variable.
    #[must_use]
    pub fn is_sigil(self) -> bool {
        matches!(
            self,
            TokenKind::SCALAR_SIGIL
                | TokenKind::ARRAY_SIGIL
                | TokenKind::HASH_SIGIL
                | TokenKind::CODE_SIGIL
                | TokenKind::TYPEGLOB_SIGIL
                | TokenKind::ARRAY_INDEX_SIGIL
        )
    }

    /// `=` or a compound assignment operator. Compound assignment is a single
    /// token (ADR 0005 §5), so this is a flat check rather than a two-token
    /// pattern match.
    #[must_use]
    pub fn is_assignment_op(self) -> bool {
        matches!(
            self,
            T!["="]
                | T!["+="]
                | T!["-="]
                | T!["*="]
                | T!["/="]
                | T!["%="]
                | T!["**="]
                | T![".="]
                | T!["x="]
                | T!["//="]
                | T!["||="]
                | T!["&&="]
                | T!["|="]
                | T!["&="]
                | T!["^="]
                | T!["<<="]
                | T![">>="]
        )
    }

    /// A token that can only appear where the parser has failed to make sense of
    /// the input. Each one carries its own diagnostic (ADR 0005 §4).
    #[must_use]
    pub fn is_error(self) -> bool {
        matches!(
            self,
            TokenKind::UNTERMINATED_REGEX
                | TokenKind::UNTERMINATED_QUOTE_LIKE
                | TokenKind::UNTERMINATED_HEREDOC
                | TokenKind::UNTERMINATED_STRING
                | TokenKind::ERROR_CHAR
        )
    }

    /// Content the formatter must reproduce byte for byte (ADR 0008 §2 `Raw`).
    #[must_use]
    pub fn is_verbatim(self) -> bool {
        matches!(
            self,
            TokenKind::STRING
                | TokenKind::LITERAL_STRING
                | TokenKind::INTERPOLATED_STRING
                | TokenKind::REGEX_PATTERN
                | TokenKind::TR_SEARCH_LIST
                | TokenKind::TR_REPLACEMENT_LIST
                | TokenKind::HEREDOC_CONTENT
                | TokenKind::HEREDOC_END
                | TokenKind::POD_CONTENT
                | TokenKind::DATA_CONTENT
                | TokenKind::RAW_CONTENT
        ) || self.is_error()
    }

    /// A keyword that can begin a statement. Doubles as the statement-level
    /// synchronisation set for panic-mode recovery (ADR 0007 §3).
    #[must_use]
    pub fn starts_statement(self) -> bool {
        matches!(
            self,
            T!["sub"]
                | T!["my"]
                | T!["our"]
                | T!["state"]
                | T!["local"]
                | T!["if"]
                | T!["unless"]
                | T!["while"]
                | T!["until"]
                | T!["for"]
                | T!["foreach"]
                | T!["package"]
                | T!["use"]
                | T!["no"]
                | T!["require"]
                | T!["return"]
                | T!["next"]
                | T!["last"]
                | T!["redo"]
                | T!["goto"]
                | T!["try"]
                | T!["given"]
                | T!["when"]
                | T!["default"]
                | T!["do"]
                | T!["BEGIN"]
                | T!["END"]
                | T!["INIT"]
                | T!["CHECK"]
                | T!["UNITCHECK"]
        )
    }

    /// Postfix statement modifiers (`... if $x`).
    #[must_use]
    pub fn is_stmt_modifier(self) -> bool {
        matches!(
            self,
            T!["if"]
                | T!["unless"]
                | T!["while"]
                | T!["until"]
                | T!["for"]
                | T!["foreach"]
                // `say $x when /re/;` inside a `given` block.
                | T!["when"]
        )
    }
}
