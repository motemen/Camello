#[macro_export]
macro_rules! T {
    [$token:tt] => {
        $crate::__syntax_kind_token!($token)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __syntax_kind_token {
    (ident) => {
        $crate::SyntaxKind::IDENT
    };
    (number) => {
        $crate::SyntaxKind::NUMBER
    };
    (string) => {
        $crate::SyntaxKind::STRING
    };
    (regex_literal) => {
        $crate::SyntaxKind::REGEX_LITERAL
    };
    (whitespace) => {
        $crate::SyntaxKind::WHITESPACE
    };
    (newline) => {
        $crate::SyntaxKind::NEWLINE
    };
    (comment) => {
        $crate::SyntaxKind::COMMENT
    };
    (scalar_sigil) => {
        $crate::SyntaxKind::SCALAR_SIGIL
    };
    (array_index_sigil) => {
        $crate::SyntaxKind::ARRAY_INDEX_SIGIL
    };
    (array_sigil) => {
        $crate::SyntaxKind::ARRAY_SIGIL
    };
    (hash_sigil) => {
        $crate::SyntaxKind::HASH_SIGIL
    };
    (reference_sigil) => {
        $crate::SyntaxKind::REFERENCE_SIGIL
    };
    (code_sigil) => {
        $crate::SyntaxKind::CODE_SIGIL
    };
    (typeglob_sigil) => {
        $crate::SyntaxKind::TYPEGLOB_SIGIL
    };
    ('{') => {
        $crate::SyntaxKind::L_BRACE
    };
    ('}') => {
        $crate::SyntaxKind::R_BRACE
    };
    ('(') => {
        $crate::SyntaxKind::L_PAREN
    };
    (')') => {
        $crate::SyntaxKind::R_PAREN
    };
    ('[') => {
        $crate::SyntaxKind::L_BRACKET
    };
    (']') => {
        $crate::SyntaxKind::R_BRACKET
    };
    (;) => {
        $crate::SyntaxKind::SEMICOLON
    };
    (,) => {
        $crate::SyntaxKind::COMMA
    };
    (::) => {
        $crate::SyntaxKind::DOUBLE_COLON
    };
    (?) => {
        $crate::SyntaxKind::QUESTION_MARK
    };
    (:) => {
        $crate::SyntaxKind::COLON
    };
    (.) => {
        $crate::SyntaxKind::DOT
    };
    (..) => {
        $crate::SyntaxKind::RANGE
    };
    (...) => {
        $crate::SyntaxKind::RANGE_EXCLUSIVE
    };
    (ellipsis) => {
        $crate::SyntaxKind::ELLIPSIS
    };
    (->) => {
        $crate::SyntaxKind::ARROW
    };
    (=>) => {
        $crate::SyntaxKind::FAT_COMMA
    };
    (=) => {
        $crate::SyntaxKind::EQ
    };
    (==) => {
        $crate::SyntaxKind::EQ_EQ
    };
    (!=) => {
        $crate::SyntaxKind::NE
    };
    (>=) => {
        $crate::SyntaxKind::GE
    };
    (<=) => {
        $crate::SyntaxKind::LE
    };
    (>) => {
        $crate::SyntaxKind::GT
    };
    (<) => {
        $crate::SyntaxKind::LT
    };
    (+) => {
        $crate::SyntaxKind::PLUS
    };
    (-) => {
        $crate::SyntaxKind::MINUS
    };
    (++) => {
        $crate::SyntaxKind::INCREMENT
    };
    (--) => {
        $crate::SyntaxKind::DECREMENT
    };
    (*) => {
        $crate::SyntaxKind::STAR
    };
    (**) => {
        $crate::SyntaxKind::EXPONENT
    };
    (/) => {
        $crate::SyntaxKind::SLASH
    };
    (%) => {
        $crate::SyntaxKind::MODULO
    };
    (/ /) => {
        $crate::SyntaxKind::DEFINED_OR
    };
    (&&) => {
        $crate::SyntaxKind::LOGICAL_AND
    };
    (||) => {
        $crate::SyntaxKind::LOGICAL_OR
    };
    (!) => {
        $crate::SyntaxKind::LOGICAL_NOT
    };
    (~) => {
        $crate::SyntaxKind::BITWISE_NOT
    };
    (&) => {
        $crate::SyntaxKind::BITWISE_AND
    };
    (|) => {
        $crate::SyntaxKind::BITWISE_OR
    };
    (^) => {
        $crate::SyntaxKind::BITWISE_XOR
    };
    (caret_token) => {
        $crate::SyntaxKind::CARET
    };
    (=~) => {
        $crate::SyntaxKind::REGEX_MATCH
    };
    (!~) => {
        $crate::SyntaxKind::REGEX_NOT_MATCH
    };
    (<=>) => {
        $crate::SyntaxKind::SPACESHIP
    };
    (<<) => {
        $crate::SyntaxKind::SHIFT_LEFT
    };
    (>>) => {
        $crate::SyntaxKind::SHIFT_RIGHT
    };
    (sub) => {
        $crate::SyntaxKind::SUB_KW
    };
    (my) => {
        $crate::SyntaxKind::MY_KW
    };
    (our) => {
        $crate::SyntaxKind::OUR_KW
    };
    (state) => {
        $crate::SyntaxKind::STATE_KW
    };
    (local) => {
        $crate::SyntaxKind::LOCAL_KW
    };
    (BEGIN) => {
        $crate::SyntaxKind::BEGIN_KW
    };
    (begin) => {
        $crate::SyntaxKind::BEGIN_KW
    };
    (END) => {
        $crate::SyntaxKind::END_BLOCK_KW
    };
    (INIT) => {
        $crate::SyntaxKind::INIT_KW
    };
    (CHECK) => {
        $crate::SyntaxKind::CHECK_KW
    };
    (UNITCHECK) => {
        $crate::SyntaxKind::UNITCHECK_KW
    };
    (if) => {
        $crate::SyntaxKind::IF_KW
    };
    (unless) => {
        $crate::SyntaxKind::UNLESS_KW
    };
    (elsif) => {
        $crate::SyntaxKind::ELSIF_KW
    };
    (else) => {
        $crate::SyntaxKind::ELSE_KW
    };
    (for) => {
        $crate::SyntaxKind::FOR_KW
    };
    (foreach) => {
        $crate::SyntaxKind::FOREACH_KW
    };
    (while) => {
        $crate::SyntaxKind::WHILE_KW
    };
    (until) => {
        $crate::SyntaxKind::UNTIL_KW
    };
    (package) => {
        $crate::SyntaxKind::PACKAGE_KW
    };
    (qw) => {
        $crate::SyntaxKind::QW_KW
    };
    (q) => {
        $crate::SyntaxKind::Q_KW
    };
    (qq) => {
        $crate::SyntaxKind::QQ_KW
    };
    (qx) => {
        $crate::SyntaxKind::QX_KW
    };
    (m) => {
        $crate::SyntaxKind::M_KW
    };
    (qr) => {
        $crate::SyntaxKind::QR_KW
    };
    (s) => {
        $crate::SyntaxKind::S_KW
    };
    (tr) => {
        $crate::SyntaxKind::TR_KW
    };
    (y) => {
        $crate::SyntaxKind::Y_KW
    };
    (use) => {
        $crate::SyntaxKind::USE_KW
    };
    (no) => {
        $crate::SyntaxKind::NO_KW
    };
    (require) => {
        $crate::SyntaxKind::REQUIRE_KW
    };
    (return) => {
        $crate::SyntaxKind::RETURN_KW
    };
    (undef) => {
        $crate::SyntaxKind::UNDEF_KW
    };
    (next) => {
        $crate::SyntaxKind::NEXT_KW
    };
    (last) => {
        $crate::SyntaxKind::LAST_KW
    };
    (redo) => {
        $crate::SyntaxKind::REDO_KW
    };
    (not) => {
        $crate::SyntaxKind::NOT_KW
    };
    (and) => {
        $crate::SyntaxKind::AND_KW
    };
    (or) => {
        $crate::SyntaxKind::OR_KW
    };
    (xor) => {
        $crate::SyntaxKind::XOR_KW
    };
    (x) => {
        $crate::SyntaxKind::X
    };
    (__END__) => {
        $crate::SyntaxKind::END_KW
    };
    (__DATA__) => {
        $crate::SyntaxKind::DATA_KW
    };
    (pod_start) => {
        $crate::SyntaxKind::POD_START
    };
    (cut) => {
        $crate::SyntaxKind::CUT_KW
    };
}
