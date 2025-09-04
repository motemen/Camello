#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // ===== Token Level =====

    // Trivia (whitespace, comments)
    WHITESPACE,
    COMMENT,

    // Identifiers / Variables
    IDENT,
    QUALIFIED_IDENT, // Qualified identifier (e.g., Foo::Bar::baz)

    // Sigils (prefixes indicating variable type)
    DOLLAR,    // $
    AT,        // @
    PERCENT,   // %
    CARET,     // ^
    BACKSLASH, // \ (reference operator)
    AMPERSAND, // & (function sigil/reference)
    ASTERISK,  // * (typeglob sigil)

    // Composite variable nodes (used later)
    SCALAR_VAR,
    ARRAY_VAR,
    HASH_VAR,
    TYPEGLOB_VAR,

    // Literals
    NUMBER,
    STRING,
    VERSION,       // v1.23, v5.008_001
    BARE_VERSION,  // 5.24.1, 5.024_001 (contextually determined)
    REGEX_LITERAL, // /pattern/flags

    // Keywords
    SUB_KW,
    MY_KW,
    OUR_KW,
    STATE_KW,
    LOCAL_KW,
    IF_KW,
    UNLESS_KW,
    ELSIF_KW,
    ELSE_KW,
    FOR_KW,     // for keyword
    FOREACH_KW, // foreach keyword (synonym for for)
    WHILE_KW,   // while keyword
    PACKAGE_KW,
    QW_KW,     // qw keyword
    Q_KW,      // q keyword (single-quoted string literal)
    QQ_KW,     // qq keyword (double-quoted string literal)
    QX_KW,     // qx keyword (command execution)
    M_KW,      // m keyword (match operator)
    QR_KW,     // qr keyword (compiled regex)
    S_KW,      // s keyword (substitution operator)
    TR_KW,     // tr keyword (transliteration operator)
    Y_KW,      // y keyword (transliteration operator, alias for tr)
    USE_KW,    // use keyword (for use warnings qw(...) syntax)
    NO_KW,     // no keyword (for no warnings qw(...) syntax)
    RETURN_KW, // return keyword

    // Data section
    END_KW,  // __END__
    DATA_KW, // __DATA__

    // I/O operators

    // POD related
    POD_START, // =pod, =head1, etc. (any =identifier)
    CUT_KW,    // =cut keyword

    // Symbols / Delimiters
    L_BRACE,      // {
    R_BRACE,      // }
    L_PAREN,      // (
    R_PAREN,      // )
    L_BRACKET,    // [
    R_BRACKET,    // ]
    DELIMITER,    // Generic delimiter for quote-like operators (contains actual delimiter text)
    SEMICOLON,    // ;
    COMMA,        // ,
    DOUBLE_COLON, // ::

    // Ternary operator tokens
    QUESTION_MARK, // ?
    COLON,         // :

    // For qw() only
    QW_STRING, // Any text inside qw()

    // Q-string family content
    Q_STRING,            // Any text inside q() (single-quoted string content)
    QQ_STRING,           // Any text inside qq() (double-quoted string content)
    QX_STRING,           // Any text inside qx() (command execution content)
    M_STRING,            // Any text inside m() (match regex content)
    QR_STRING,           // Any text inside qr() (compiled regex content)
    S_PATTERN,           // Pattern part inside s() (substitution pattern)
    S_REPLACEMENT,       // Replacement part inside s() (substitution replacement)
    TR_SEARCH_LIST,      // Search list part inside tr() (characters to translate from)
    TR_REPLACEMENT_LIST, // Replacement list part inside tr() (characters to translate to)

    // Flags for quote-like operators
    S_FLAGS,  // Flags for substitution operator (s///msixpodualngcer)
    TR_FLAGS, // Flags for transliteration operator (tr///cdsr)

    // Operators
    EQ,          // =
    PLUS,        // + (binary addition)
    MINUS,       // - (binary subtraction)
    UNARY_PLUS,  // + (unary plus)
    UNARY_MINUS, // - (unary minus)
    DOT,         // . (string concatenation)
    ARROW,       // ->
    FAT_COMMA,   // =>

    // Multiplicative operators
    STAR,   // *
    SLASH,  // /
    MODULO, // % (modulo operator)
    X,      // x (repetition)

    // Comparison operators
    GT,      // >
    LT,      // <
    GE,      // >=
    LE,      // <=
    EQ_EQ,   // ==
    NE,      // !=
    STR_EQ,  // eq
    STR_NE,  // ne
    STR_GT,  // gt
    STR_LT,  // lt
    STR_GE,  // ge
    STR_LE,  // le
    STR_CMP, // cmp

    // Regex operators
    REGEX_MATCH,     // =~
    REGEX_NOT_MATCH, // !~

    // Logical operators
    LOGICAL_AND, // &&
    LOGICAL_OR,  // ||
    LOGICAL_NOT, // !

    // Low-precedence logical operators
    NOT_KW, // not
    AND_KW, // and
    OR_KW,  // or
    XOR_KW, // xor

    // Defined-or operator
    DEFINED_OR, // //

    // Three-way comparison (spaceship) operator
    SPACESHIP, // <=>

    // File test operators
    FILE_TEST_OP, // -f, -d, etc.

    // Postfix dereference operators (introduced in Perl 5.20)
    POSTFIX_DEREF_ARRAY,  // ->@*
    POSTFIX_DEREF_HASH,   // ->%*
    POSTFIX_DEREF_SCALAR, // ->$*

    // ===== Node Level (composite structures) =====
    ROOT,          // File root
    SUB_DEF,       // Subroutine definition
    SUB_PROTOTYPE, // Subroutine prototype (e.g., (\@@), ($@), etc.)
    BLOCK_STMT,    // Block statement

    // Declarations
    DECLARATION_STMT, // Variable declaration (my, our, state, etc.)
    PACKAGE_STMT,     // Package declaration (package Foo::Bar)
    USE_STMT,         // use statement (use warnings qw(all);)
    NO_STMT,          // no statement (no warnings qw(all);)
    FOR_STMT,         // for statement
    WHILE_STMT,       // while statement
    IF_STMT,          // if statement
    UNLESS_STMT,      // unless statement

    // Data section (content after __END__ / __DATA__)
    DATA_SECTION,
    RAW_STRING,

    // POD related
    POD_BLOCK,   // POD block containing commands and content
    POD_CONTENT, // Content within POD block (verbatim text)

    // Expressions
    INFIX_EXPR,               // Infix expression (binary operation)
    PREFIX_EXPR,              // Prefix expression (unary operation, e.g., !$foo, -$x)
    POSTFIX_EXPR,             // Postfix expression (e.g., $i++, $i--)
    TERNARY_EXPR, // Ternary expression (conditional operator: condition ? true_expr : false_expr)
    METHOD_CALL_EXPR, // Method call expression (e.g., $obj->method())
    HASH_REF_ACCESS_EXPR, // Hash reference access expression (e.g., $hash->{key})
    ARRAY_REF_ACCESS_EXPR, // Array reference access expression (e.g., $arr->[0])
    CODE_REF_CALL_EXPR, // Code reference call expression (e.g., $coderef->(args))
    HASH_SUBSCRIPTION_EXPR, // Direct hash access expression (e.g., $hash{key})
    ARRAY_SUBSCRIPTION_EXPR, // Direct array access expression (e.g., $array[0])
    FUNCTION_CALL_EXPR, // Function call expression (e.g., push @array, $value)
    BLOCK_FUNCTION_CALL_EXPR, // Block function call expression (e.g., eval { ... }, map { ... } @list)
    QW_EXPR,                  // qw() expression (quote word list)
    Q_EXPR,                   // q() expression (single-quoted string literal)
    QQ_EXPR,                  // qq() expression (double-quoted string literal)
    QX_EXPR,                  // qx() expression (command execution)
    M_EXPR,                   // m() expression (match regex literal)
    QR_EXPR,                  // qr() expression (compiled regex literal)
    S_EXPR,                   // s() expression (substitution literal)
    TR_EXPR,                  // tr() expression (transliteration literal)
    DEREF_EXPR,               // Dereference expression (e.g., @$var, %$var, $$var)
    REGEX_EXPR,               // Regex expression (e.g., $str =~ "pattern")
    REFERENCE_EXPR,           // Reference expression (e.g., \$scalar, \@array, \%hash, \&code)
    IO_EXPR,                  // I/O expression (e.g., <STDIN>, <>, <$fh>)
    ANON_SUB_EXPR,            // Anonymous subroutine expression (e.g., sub { ... })
    TYPEGLOB_EXPR,            // Typeglob expression (e.g., *{$name}, *STDIN)
    FILE_TEST_EXPR,           // File test expression (e.g., -f $file)
    POSTFIX_DEREF_EXPR,       // Postfix dereference expression (e.g., $ref->@*, $ref->%*, $ref->$*)

    // Literal references
    HASH_REF,  // Hash reference (anonymous hash)
    ARRAY_REF, // Array reference (anonymous array)

    // Modifiers
    IF_MODIFIER,     // Postfix if modifier (e.g., print "hello" if $debug;)
    UNLESS_MODIFIER, // Postfix unless modifier (e.g., return $x unless $x > $y;)

    // Other statements
    STMT, // General statement

    EXPR_LIST, // Expression list (e.g., $a, $b, $c)

    // ===== Other =====
    ERROR, // Parse error
    EOF,   // End of file
}

impl SyntaxKind {
    #[must_use]
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }

    #[must_use]
    pub fn is_whitespace(self) -> bool {
        self == SyntaxKind::WHITESPACE
    }

    #[must_use]
    pub fn is_keyword(self) -> bool {
        matches!(
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
                | SyntaxKind::RETURN_KW
                | SyntaxKind::NOT_KW
                | SyntaxKind::AND_KW
                | SyntaxKind::OR_KW
                | SyntaxKind::XOR_KW
        )
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

    #[must_use]
    pub fn is_sigil(self) -> bool {
        matches!(
            self,
            SyntaxKind::DOLLAR
                | SyntaxKind::AT
                | SyntaxKind::PERCENT
                | SyntaxKind::BACKSLASH
                | SyntaxKind::ASTERISK
        )
    }

    #[must_use]
    pub fn is_operator(self) -> bool {
        matches!(
            self,
            SyntaxKind::EQ
                | SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::UNARY_PLUS
                | SyntaxKind::UNARY_MINUS
                | SyntaxKind::DOT
                | SyntaxKind::ARROW
                | SyntaxKind::FAT_COMMA
                | SyntaxKind::STAR
                | SyntaxKind::SLASH
                | SyntaxKind::MODULO
                | SyntaxKind::X
                | SyntaxKind::LOGICAL_AND
                | SyntaxKind::LOGICAL_OR
                | SyntaxKind::LOGICAL_NOT
                | SyntaxKind::NOT_KW
                | SyntaxKind::AND_KW
                | SyntaxKind::OR_KW
                | SyntaxKind::XOR_KW
                | SyntaxKind::DEFINED_OR
                | SyntaxKind::SPACESHIP
        )
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}
