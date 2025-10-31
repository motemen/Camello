mod macros;
mod predicates;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // ===== Token Level =====

    // Trivia (whitespace, newlines, comments)
    WHITESPACE,
    NEWLINE,
    COMMENT,

    // Identifiers / Variables
    IDENT,
    QUALIFIED_IDENT, // Qualified identifier (e.g., Foo::Bar::baz)

    // Sigils (prefixes indicating variable type)
    SCALAR_SIGIL,      // $
    ARRAY_INDEX_SIGIL, // $# (array last index sigil)
    ARRAY_SIGIL,       // @
    HASH_SIGIL,        // %
    CARET,             // ^
    BACKSLASH,         // \ (reference operator)
    CODE_SIGIL,        // & (function sigil/reference)
    TYPEGLOB_SIGIL,    // * (typeglob sigil)

    // Composite variable nodes (used later)
    SCALAR_VAR,
    ARRAY_VAR,
    HASH_VAR,
    TYPEGLOB_VAR,

    // Literals
    NUMBER,
    STRING,
    BACKTICK_STRING, // `command` (command execution literal)
    VERSION,         // v1.23, v5.008_001
    BARE_VERSION,    // 5.24.1, 5.024_001 (contextually determined)
    REGEX_LITERAL,   // /pattern/flags

    // Keywords
    SUB_KW,
    MY_KW,
    OUR_KW,
    STATE_KW,
    LOCAL_KW,
    BEGIN_KW,     // BEGIN block keyword
    END_BLOCK_KW, // END block keyword
    INIT_KW,      // INIT block keyword
    CHECK_KW,     // CHECK block keyword
    UNITCHECK_KW, // UNITCHECK block keyword
    IF_KW,
    UNLESS_KW,
    ELSIF_KW,
    ELSE_KW,
    FOR_KW,     // for keyword
    FOREACH_KW, // foreach keyword (synonym for for)
    WHILE_KW,   // while keyword
    UNTIL_KW,   // until keyword
    PACKAGE_KW,
    QW_KW,      // qw keyword
    Q_KW,       // q keyword (single-quoted string literal)
    QQ_KW,      // qq keyword (double-quoted string literal)
    QX_KW,      // qx keyword (command execution)
    M_KW,       // m keyword (match operator)
    QR_KW,      // qr keyword (compiled regex)
    S_KW,       // s keyword (substitution operator)
    TR_KW,      // tr keyword (transliteration operator)
    Y_KW,       // y keyword (transliteration operator, alias for tr)
    USE_KW,     // use keyword (for use warnings qw(...) syntax)
    NO_KW,      // no keyword (for no warnings qw(...) syntax)
    REQUIRE_KW, // require keyword (for require local::lib syntax)
    RETURN_KW,  // return keyword
    UNDEF_KW,   // undef keyword
    NEXT_KW,    // next keyword
    LAST_KW,    // last keyword
    REDO_KW,    // redo keyword
    TRY_KW,     // try keyword
    CATCH_KW,   // catch keyword
    FINALLY_KW, // finally keyword
    GIVEN_KW,   // given keyword
    WHEN_KW,    // when keyword
    DEFAULT_KW, // default keyword

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

    // Heredoc tokens
    HEREDOC_START,   // <<EOF, <<'EOF', etc.
    HEREDOC_CONTENT, // The content between start and end
    HEREDOC_END,     // The termination marker line

    // Unified string and regex content types
    LITERAL_STRING,      // Content inside q() (non-interpolated string)
    INTERPOLATED_STRING, // Content inside qq(), qx(), and s() replacement (interpolated string)
    REGEX_PATTERN,       // Content inside m(), qr(), and s() pattern (regex pattern)

    // Transliteration-specific (tr/y have unique semantics)
    TR_SEARCH_LIST,      // Search list part inside tr() (characters to translate from)
    TR_REPLACEMENT_LIST, // Replacement list part inside tr() (characters to translate to)

    // Flags for quote-like operators
    M_FLAGS,  // Flags for match operator (m///msixpodualngcer)
    QR_FLAGS, // Flags for compiled regex operator (qr///msixpodualngcer)
    S_FLAGS,  // Flags for substitution operator (s///msixpodualngcer)
    TR_FLAGS, // Flags for transliteration operator (tr///cdsr)

    // Operators
    EQ,                // =
    PLUS,              // + (binary addition)
    MINUS,             // - (binary subtraction)
    UNARY_PLUS,        // + (unary plus)
    UNARY_MINUS,       // - (unary minus)
    INCREMENT,         // ++ (raw increment token)
    DECREMENT,         // -- (raw decrement token)
    PREFIX_INCREMENT,  // ++ (prefix increment)
    PREFIX_DECREMENT,  // -- (prefix decrement)
    POSTFIX_INCREMENT, // ++ (postfix increment)
    POSTFIX_DECREMENT, // -- (postfix decrement)
    DOT,               // . (string concatenation)
    ARROW,             // ->
    FAT_COMMA,         // =>

    // Exponentiation operator
    EXPONENT, // **

    // Multiplicative operators
    STAR,   // *
    SLASH,  // /
    MODULO, // % (modulo operator)
    X,      // x (repetition)

    // Bit shift operators
    SHIFT_LEFT,  // <<
    SHIFT_RIGHT, // >>

    // Comparison operators
    GT,          // >
    LT,          // <
    GE,          // >=
    LE,          // <=
    EQ_EQ,       // ==
    NE,          // !=
    SMART_MATCH, // ~~
    STR_EQ,      // eq
    STR_NE,      // ne
    STR_GT,      // gt
    STR_LT,      // lt
    STR_GE,      // ge
    STR_LE,      // le
    STR_CMP,     // cmp

    // Regex operators
    REGEX_MATCH,     // =~
    REGEX_NOT_MATCH, // !~

    // Logical operators
    LOGICAL_AND, // &&
    LOGICAL_OR,  // ||
    LOGICAL_NOT, // !

    // Bitwise operators
    BITWISE_AND, // &
    BITWISE_OR,  // |
    BITWISE_XOR, // ^
    BITWISE_NOT, // ~

    // Low-precedence logical operators
    NOT_KW, // not
    AND_KW, // and
    OR_KW,  // or
    XOR_KW, // xor

    // Defined-or operator
    DEFINED_OR, // //

    // Three-way comparison (spaceship) operator
    SPACESHIP, // <=>

    // Range operators
    RANGE,           // ..
    RANGE_EXCLUSIVE, // ...
    ELLIPSIS,        // ... (statement placeholder)

    // File test operators
    FILE_TEST_OP, // -f, -d, etc.

    // Postfix dereference operators (introduced in Perl 5.20)
    POSTFIX_DEREF_ARRAY,            // ->@*
    POSTFIX_DEREF_HASH,             // ->%*
    POSTFIX_DEREF_SCALAR,           // ->$*
    POSTFIX_DEREF_ARRAY_LAST_INDEX, // ->$#*
    POSTFIX_DEREF_CODE,             // ->&*
    POSTFIX_DEREF_GLOB,             // ->**

    // ===== Node Level (composite structures) =====
    ROOT,              // File root
    SUB_DEF,           // Subroutine definition
    SUB_PROTOTYPE,     // Subroutine prototype (e.g., (\\@@), ($@), etc.)
    SUB_SIGNATURE,     // Subroutine signature (e.g., ($x, @rest, %opts))
    SIGNATURE_PARAM,   // Parameter inside a subroutine signature
    SIGNATURE_DEFAULT, // Default value expression for a signature parameter
    ATTR,              // Attribute (e.g., :method)
    ATTR_ARGS,         // Attribute arguments (e.g., (1, 2))
    BLOCK_STMT,        // Block statement
    PHASE_BLOCK_STMT,  // Phase block statement (BEGIN/UNITCHECK/CHECK/INIT/END)
    LABELED_STMT,      // Labeled statement
    LABEL,             // Statement label

    // Declarations
    VAR_DECL,        // Variable declaration (my, our, state, etc.)
    PACKAGE_STMT,    // Package declaration (package Foo::Bar)
    USE_STMT,        // use statement (use warnings qw(all);)
    NO_STMT,         // no statement (no warnings qw(all);)
    FOR_STMT,        // for statement (Perl-style)
    WHILE_STMT,      // while statement
    UNTIL_STMT,      // until statement
    IF_STMT,         // if statement
    UNLESS_STMT,     // unless statement
    TRY_STMT,        // try statement
    CATCH_BLOCK,     // catch block (with optional parameter)
    CATCH_PARAM,     // catch parameter binding
    FINALLY_BLOCK,   // finally block
    GIVEN_STATEMENT, // given statement
    WHEN_CLAUSE,     // when clause
    DEFAULT_CLAUSE,  // default clause

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
    BACKTICK_EXPR,            // `command` expression (command execution literal)
    M_EXPR,                   // m() expression (match regex literal)
    QR_EXPR,                  // qr() expression (compiled regex literal)
    S_EXPR,                   // s() expression (substitution literal)
    TR_EXPR,
    COMPOUND_VAR, // Compound variables (e.g., @{expr}, %$var, $#array),               // Dereference expression (e.g., @$var, %$var, $$var)
    REGEX_EXPR,   // Regex expression (e.g., $str =~ "pattern")
    REFERENCE_EXPR, // Reference expression (e.g., \$scalar, \@array, \%hash, \&code)
    FUNCTION_REF, // Function reference (e.g., &function)
    IO_EXPR,      // I/O expression (e.g., <STDIN>, <>, <$fh>)
    ANON_SUB_EXPR, // Anonymous subroutine expression (e.g., sub { ... })
    FILE_TEST_EXPR, // File test expression (e.g., -f $file)
    POSTFIX_DEREF_EXPR, // Postfix dereference expression (e.g., $ref->@*, $ref->%*, $ref->$*, $ref->$#*)
    POSTFIX_ARRAY_SLICE_EXPR, // Postfix array slice (e.g., $ref->@[...], $ref->@{...})
    POSTFIX_HASH_SLICE_EXPR, // Postfix hash slice (e.g., $ref->%[...], $ref->%{...})
    PAREN_EXPR,         // Parenthesized expression (e.g., (1, 2, 3))
    REQUIRE_EXPR,       // Require expression (e.g., require local::lib)

    // Literal references
    HASH_REF,  // Hash reference (anonymous hash)
    ARRAY_REF, // Array reference (anonymous array)

    // Modifiers
    IF_MODIFIER,     // Postfix if modifier (e.g., print "hello" if $debug;)
    UNLESS_MODIFIER, // Postfix unless modifier (e.g., return $x unless $x > $y;)
    WHILE_MODIFIER,  // Postfix while modifier (e.g., print $i++ while $i < 10;)
    UNTIL_MODIFIER,  // Postfix until modifier (e.g., $i++ until $i == 10;)
    FOR_MODIFIER,    // Postfix for modifier (e.g., say for @items;)
    WHEN_MODIFIER,   // Postfix when modifier (e.g., say "Found it!" when $_ ~~ /pattern/;)

    // Other statements
    ELLIPSIS_STMT, // Ellipsis statement placeholder
    EMPTY_STMT,    // Empty statement (bare semicolon)
    STMT,          // General statement

    EXPR_LIST,           // Expression list (e.g., $a, $b, $c)
    COMPOUND_ASSIGNMENT, // Compound assignment (e.g., +=, ||=

    // ===== Other =====
    ERROR, // Parse error
    EOF,   // End of file
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}
