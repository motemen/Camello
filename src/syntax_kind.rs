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

    // Composite variable nodes (used later)
    SCALAR_VAR,
    ARRAY_VAR,
    HASH_VAR,

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
    USE_KW,    // use keyword (for use warnings qw(...) syntax)
    RETURN_KW, // return keyword

    // Data section
    END_KW,  // __END__
    DATA_KW, // __DATA__

    // POD related
    POD_COMMAND, // =pod, =head1, =cut, etc.
    CUT_KW,      // =cut keyword

    // Symbols / Delimiters
    L_BRACE,      // {
    R_BRACE,      // }
    L_PAREN,      // (
    R_PAREN,      // )
    L_BRACKET,    // [
    R_BRACKET,    // ]
    SEMICOLON,    // ;
    COMMA,        // ,
    DOUBLE_COLON, // ::

    // Ternary operator tokens
    QUESTION_MARK, // ?
    COLON,         // :

    // For qw() only
    QW_STRING, // Any text inside qw()

    // Q-string family content
    Q_STRING,      // Any text inside q() (single-quoted string content)
    QQ_STRING,     // Any text inside qq() (double-quoted string content)
    QX_STRING,     // Any text inside qx() (command execution content)
    M_STRING,      // Any text inside m() (match regex content)
    QR_STRING,     // Any text inside qr() (compiled regex content)
    S_PATTERN,     // Pattern part inside s() (substitution pattern)
    S_REPLACEMENT, // Replacement part inside s() (substitution replacement)

    // Operators
    EQ,        // =
    PLUS,      // +
    MINUS,     // -
    DOT,       // . (string concatenation)
    ARROW,     // ->
    FAT_COMMA, // =>

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

    // ===== Node Level (composite structures) =====
    ROOT,       // File root
    SUB_DEF,    // Subroutine definition
    BLOCK_STMT, // Block statement

    // Declarations
    DECLARATION_STMT, // Variable declaration (my, our, state, etc.)
    PACKAGE_STMT,     // Package declaration (package Foo::Bar)
    USE_STMT,         // use statement (use warnings qw(all);)
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
    DEREF_EXPR,               // Dereference expression (e.g., @$var, %$var, $$var)
    REGEX_EXPR,               // Regex expression (e.g., $str =~ "pattern")
    REFERENCE_EXPR,           // Reference expression (e.g., \$scalar, \@array, \%hash, \&code)

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
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }

    pub fn is_whitespace(self) -> bool {
        self == SyntaxKind::WHITESPACE
    }

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
                | SyntaxKind::USE_KW
                | SyntaxKind::RETURN_KW
                | SyntaxKind::NOT_KW
                | SyntaxKind::AND_KW
                | SyntaxKind::OR_KW
                | SyntaxKind::XOR_KW
        )
    }

    pub fn is_variable(self) -> bool {
        matches!(
            self,
            SyntaxKind::SCALAR_VAR | SyntaxKind::ARRAY_VAR | SyntaxKind::HASH_VAR
        )
    }

    pub fn is_sigil(self) -> bool {
        matches!(
            self,
            SyntaxKind::DOLLAR | SyntaxKind::AT | SyntaxKind::PERCENT | SyntaxKind::BACKSLASH
        )
    }

    pub fn is_operator(self) -> bool {
        matches!(
            self,
            SyntaxKind::EQ
                | SyntaxKind::PLUS
                | SyntaxKind::MINUS
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

impl std::fmt::Display for SyntaxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            SyntaxKind::WHITESPACE => "WHITESPACE",
            SyntaxKind::COMMENT => "COMMENT",
            SyntaxKind::IDENT => "IDENT",
            SyntaxKind::QUALIFIED_IDENT => "QUALIFIED_IDENT",
            SyntaxKind::DOLLAR => "DOLLAR",
            SyntaxKind::AT => "AT",
            SyntaxKind::PERCENT => "PERCENT",
            SyntaxKind::CARET => "CARET",
            SyntaxKind::BACKSLASH => "BACKSLASH",
            SyntaxKind::AMPERSAND => "AMPERSAND",
            SyntaxKind::SCALAR_VAR => "SCALAR_VAR",
            SyntaxKind::ARRAY_VAR => "ARRAY_VAR",
            SyntaxKind::HASH_VAR => "HASH_VAR",
            SyntaxKind::NUMBER => "NUMBER",
            SyntaxKind::STRING => "STRING",
            SyntaxKind::VERSION => "VERSION",
            SyntaxKind::BARE_VERSION => "BARE_VERSION",
            SyntaxKind::REGEX_LITERAL => "REGEX_LITERAL",
            SyntaxKind::SUB_KW => "SUB_KW",
            SyntaxKind::MY_KW => "MY_KW",
            SyntaxKind::OUR_KW => "OUR_KW",
            SyntaxKind::STATE_KW => "STATE_KW",
            SyntaxKind::LOCAL_KW => "LOCAL_KW",
            SyntaxKind::IF_KW => "IF_KW",
            SyntaxKind::UNLESS_KW => "UNLESS_KW",
            SyntaxKind::ELSIF_KW => "ELSIF_KW",
            SyntaxKind::ELSE_KW => "ELSE_KW",
            SyntaxKind::FOR_KW => "FOR_KW",
            SyntaxKind::FOREACH_KW => "FOREACH_KW",
            SyntaxKind::WHILE_KW => "WHILE_KW",
            SyntaxKind::PACKAGE_KW => "PACKAGE_KW",
            SyntaxKind::QW_KW => "QW_KW",
            SyntaxKind::Q_KW => "Q_KW",
            SyntaxKind::QQ_KW => "QQ_KW",
            SyntaxKind::QX_KW => "QX_KW",
            SyntaxKind::M_KW => "M_KW",
            SyntaxKind::QR_KW => "QR_KW",
            SyntaxKind::S_KW => "S_KW",
            SyntaxKind::USE_KW => "USE_KW",
            SyntaxKind::RETURN_KW => "RETURN_KW",
            SyntaxKind::L_BRACE => "L_BRACE",
            SyntaxKind::R_BRACE => "R_BRACE",
            SyntaxKind::L_PAREN => "L_PAREN",
            SyntaxKind::R_PAREN => "R_PAREN",
            SyntaxKind::L_BRACKET => "L_BRACKET",
            SyntaxKind::R_BRACKET => "R_BRACKET",
            SyntaxKind::QUESTION_MARK => "QUESTION_MARK",
            SyntaxKind::COLON => "COLON",
            SyntaxKind::QW_STRING => "QW_STRING",
            SyntaxKind::Q_STRING => "Q_STRING",
            SyntaxKind::QQ_STRING => "QQ_STRING",
            SyntaxKind::QX_STRING => "QX_STRING",
            SyntaxKind::M_STRING => "M_STRING",
            SyntaxKind::QR_STRING => "QR_STRING",
            SyntaxKind::S_PATTERN => "S_PATTERN",
            SyntaxKind::S_REPLACEMENT => "S_REPLACEMENT",
            SyntaxKind::SEMICOLON => "SEMICOLON",
            SyntaxKind::COMMA => "COMMA",
            SyntaxKind::DOUBLE_COLON => "DOUBLE_COLON",
            SyntaxKind::EQ => "EQ",
            SyntaxKind::PLUS => "PLUS",
            SyntaxKind::MINUS => "MINUS",
            SyntaxKind::DOT => "DOT",
            SyntaxKind::ARROW => "ARROW",
            SyntaxKind::FAT_COMMA => "FAT_COMMA",
            SyntaxKind::STAR => "STAR",
            SyntaxKind::SLASH => "SLASH",
            SyntaxKind::MODULO => "MODULO",
            SyntaxKind::X => "X",
            SyntaxKind::GT => "GT",
            SyntaxKind::LT => "LT",
            SyntaxKind::GE => "GE",
            SyntaxKind::LE => "LE",
            SyntaxKind::EQ_EQ => "EQ_EQ",
            SyntaxKind::NE => "NE",
            SyntaxKind::STR_EQ => "STR_EQ",
            SyntaxKind::STR_NE => "STR_NE",
            SyntaxKind::STR_GT => "STR_GT",
            SyntaxKind::STR_LT => "STR_LT",
            SyntaxKind::STR_GE => "STR_GE",
            SyntaxKind::STR_LE => "STR_LE",
            SyntaxKind::STR_CMP => "STR_CMP",
            SyntaxKind::REGEX_MATCH => "REGEX_MATCH",
            SyntaxKind::REGEX_NOT_MATCH => "REGEX_NOT_MATCH",
            SyntaxKind::LOGICAL_AND => "LOGICAL_AND",
            SyntaxKind::LOGICAL_OR => "LOGICAL_OR",
            SyntaxKind::LOGICAL_NOT => "LOGICAL_NOT",
            SyntaxKind::NOT_KW => "NOT_KW",
            SyntaxKind::AND_KW => "AND_KW",
            SyntaxKind::OR_KW => "OR_KW",
            SyntaxKind::XOR_KW => "XOR_KW",
            SyntaxKind::DEFINED_OR => "DEFINED_OR",
            SyntaxKind::SPACESHIP => "SPACESHIP",
            SyntaxKind::ROOT => "ROOT",
            SyntaxKind::SUB_DEF => "SUB_DEF",
            SyntaxKind::BLOCK_STMT => "BLOCK_STMT",
            SyntaxKind::DECLARATION_STMT => "DECLARATION_STMT",
            SyntaxKind::PACKAGE_STMT => "PACKAGE_STMT",
            SyntaxKind::USE_STMT => "USE_STMT",
            SyntaxKind::FOR_STMT => "FOR_STMT",
            SyntaxKind::WHILE_STMT => "WHILE_STMT",
            SyntaxKind::IF_STMT => "IF_STMT",
            SyntaxKind::UNLESS_STMT => "UNLESS_STMT",
            SyntaxKind::INFIX_EXPR => "INFIX_EXPR",
            SyntaxKind::PREFIX_EXPR => "PREFIX_EXPR",
            SyntaxKind::POSTFIX_EXPR => "POSTFIX_EXPR",
            SyntaxKind::TERNARY_EXPR => "TERNARY_EXPR",
            SyntaxKind::METHOD_CALL_EXPR => "METHOD_CALL_EXPR",
            SyntaxKind::HASH_REF_ACCESS_EXPR => "HASH_REF_ACCESS_EXPR",
            SyntaxKind::ARRAY_REF_ACCESS_EXPR => "ARRAY_REF_ACCESS_EXPR",
            SyntaxKind::CODE_REF_CALL_EXPR => "CODE_REF_CALL_EXPR",
            SyntaxKind::HASH_SUBSCRIPTION_EXPR => "HASH_SUBSCRIPTION_EXPR",
            SyntaxKind::ARRAY_SUBSCRIPTION_EXPR => "ARRAY_SUBSCRIPTION_EXPR",
            SyntaxKind::QW_EXPR => "QW_EXPR",
            SyntaxKind::Q_EXPR => "Q_EXPR",
            SyntaxKind::QQ_EXPR => "QQ_EXPR",
            SyntaxKind::QX_EXPR => "QX_EXPR",
            SyntaxKind::M_EXPR => "M_EXPR",
            SyntaxKind::QR_EXPR => "QR_EXPR",
            SyntaxKind::S_EXPR => "S_EXPR",
            SyntaxKind::DEREF_EXPR => "DEREF_EXPR",
            SyntaxKind::REGEX_EXPR => "REGEX_EXPR",
            SyntaxKind::REFERENCE_EXPR => "REFERENCE_EXPR",
            SyntaxKind::HASH_REF => "HASH_REF",
            SyntaxKind::ARRAY_REF => "ARRAY_REF",
            SyntaxKind::IF_MODIFIER => "IF_MODIFIER",
            SyntaxKind::UNLESS_MODIFIER => "UNLESS_MODIFIER",
            SyntaxKind::STMT => "STMT",
            SyntaxKind::ERROR => "ERROR",
            SyntaxKind::EOF => "EOF",
            SyntaxKind::EXPR_LIST => "EXPR_LIST",
            SyntaxKind::FUNCTION_CALL_EXPR => "FUNCTION_CALL_EXPR",
            SyntaxKind::BLOCK_FUNCTION_CALL_EXPR => "BLOCK_FUNCTION_CALL_EXPR",
            SyntaxKind::DATA_SECTION => "DATA_SECTION",
            SyntaxKind::DATA_KW => "DATA_KW",
            SyntaxKind::END_KW => "END_KW",
            SyntaxKind::RAW_STRING => "RAW_STRING",
            SyntaxKind::POD_COMMAND => "POD_COMMAND",
            SyntaxKind::CUT_KW => "CUT_KW",
            SyntaxKind::POD_BLOCK => "POD_BLOCK",
            SyntaxKind::POD_CONTENT => "POD_CONTENT",
        };
        write!(f, "{}", name)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}
