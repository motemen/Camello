//! The language vocabulary.
//!
//! Everything here is generated from the single `define_language!` invocation
//! below. Semantic predicates that cannot be derived from the declaration order
//! live in the private `predicates` module.

mod macros;
mod predicates;

pub(crate) use macros::define_language;

define_language! {
    // ===== Reserved words and named operators =====
    //
    // A keyword is only a keyword where the grammar expects one; `sub if {}` and
    // `package tr;` are legal Perl. The parser funnels those through a single
    // `name()` routine (the parser contract) rather than special-casing each site.
    keywords {
        "sub"        => SUB_KW,
        "my"         => MY_KW,
        "our"        => OUR_KW,
        "state"      => STATE_KW,
        "local"      => LOCAL_KW,

        "BEGIN"      => BEGIN_KW,
        "END"        => END_BLOCK_KW,
        "INIT"       => INIT_KW,
        "CHECK"      => CHECK_KW,
        "UNITCHECK"  => UNITCHECK_KW,

        "if"         => IF_KW,
        "unless"     => UNLESS_KW,
        "elsif"      => ELSIF_KW,
        "else"       => ELSE_KW,
        "for"        => FOR_KW,
        "foreach"    => FOREACH_KW,
        "while"      => WHILE_KW,
        "until"      => UNTIL_KW,
        "do"         => DO_KW,
        "continue"   => CONTINUE_KW,

        "package"    => PACKAGE_KW,
        // The core class feature (perl 5.38). `class` opens a package-like
        // scope; `field` declares a slot and `method` a subroutine with an
        // implicit `$self`, both of them only inside one.
        "class"      => CLASS_KW,
        "field"      => FIELD_KW,
        "method"     => METHOD_KW,
        "use"        => USE_KW,
        "no"         => NO_KW,
        "require"    => REQUIRE_KW,
        "return"     => RETURN_KW,
        "undef"      => UNDEF_KW,
        "next"       => NEXT_KW,
        "last"       => LAST_KW,
        "redo"       => REDO_KW,
        "goto"       => GOTO_KW,

        "try"        => TRY_KW,
        "catch"      => CATCH_KW,
        "finally"    => FINALLY_KW,
        "given"      => GIVEN_KW,
        "when"       => WHEN_KW,
        "default"    => DEFAULT_KW,

        // Quote-like operators. The lexer turns these into atomic sequences
        // (the lexer contract) rather than leaving a mode switched on.
        "q"          => Q_KW,
        "qq"         => QQ_KW,
        "qx"         => QX_KW,
        "qw"         => QW_KW,
        "m"          => M_KW,
        "qr"         => QR_KW,
        "s"          => S_KW,
        "tr"         => TR_KW,
        "y"          => Y_KW,

        // Low-precedence logical operators.
        "not"        => NOT_KW,
        "and"        => AND_KW,
        "or"         => OR_KW,
        "xor"        => XOR_KW,

        // String comparison operators.
        "eq"         => STR_EQ,
        "ne"         => STR_NE,
        "lt"         => STR_LT,
        "gt"         => STR_GT,
        "le"         => STR_LE,
        "ge"         => STR_GE,
        "cmp"        => STR_CMP,

        // The class instance operator (perl 5.32, stable in 5.36). Its right
        // operand is a bareword class name.
        "isa"        => ISA_KW,

        // Repetition. Only recognised in operator position, so `x5` lexes as a
        // single identifier in term position and needs no re-splitting
        // (the lexer contract).
        "x"          => X_OP,

        "format"     => FORMAT_KW,

        "__END__"    => END_KW,
        "__DATA__"   => DATA_KW,
    }

    // ===== Punctuation and symbolic operators =====
    //
    // Longest-match order is the lexer's business, not this table's; entries are
    // grouped by role for readability.
    punct {
        "{"    => L_BRACE,
        "}"    => R_BRACE,
        "("    => L_PAREN,
        ")"    => R_PAREN,
        "["    => L_BRACKET,
        "]"    => R_BRACKET,
        ";"    => SEMICOLON,
        ","    => COMMA,
        "::"   => DOUBLE_COLON,
        "?"    => QUESTION_MARK,
        ":"    => COLON,

        "$"    => SCALAR_SIGIL,
        "$#"   => ARRAY_INDEX_SIGIL,
        "@"    => ARRAY_SIGIL,
        "\\"   => BACKSLASH,

        "="    => EQ,
        "+"    => PLUS,
        "-"    => MINUS,
        "++"   => INCREMENT,
        "--"   => DECREMENT,
        "."    => DOT,
        "->"   => ARROW,
        "=>"   => FAT_COMMA,
        "**"   => EXPONENT,
        "/"    => SLASH,

        "<<"   => SHIFT_LEFT,
        ">>"   => SHIFT_RIGHT,
        ">"    => GT,
        "<"    => LT,
        ">="   => GE,
        "<="   => LE,
        "=="   => EQ_EQ,
        "!="   => NE,
        "<=>"  => SPACESHIP,
        "~~"   => SMART_MATCH,

        "=~"   => REGEX_MATCH,
        "!~"   => REGEX_NOT_MATCH,

        "&&"   => LOGICAL_AND,
        "||"   => LOGICAL_OR,
        "!"    => LOGICAL_NOT,
        "|"    => BITWISE_OR,
        "^"    => BITWISE_XOR,
        "~"    => BITWISE_NOT,
        "//"   => DEFINED_OR,

        ".."   => RANGE,
        "..."  => ELLIPSIS,

        // Compound assignment is a single token (the lexer contract), which removes the
        // `COMPOUND_ASSIGNMENT` node entirely (the language model).
        "+="   => PLUS_EQ,
        "-="   => MINUS_EQ,
        "*="   => STAR_EQ,
        "/="   => SLASH_EQ,
        "%="   => MODULO_EQ,
        "**="  => EXPONENT_EQ,
        ".="   => DOT_EQ,
        "x="   => X_EQ,
        "//="  => DEFINED_OR_EQ,
        "||="  => LOGICAL_OR_EQ,
        "&&="  => LOGICAL_AND_EQ,
        "|="   => BITWISE_OR_EQ,
        "&="   => BITWISE_AND_EQ,
        "^="   => BITWISE_XOR_EQ,
        "<<="  => SHIFT_LEFT_EQ,
        ">>="  => SHIFT_RIGHT_EQ,

        // Postfix dereference (Perl 5.20+).
        "->@*"  => POSTFIX_DEREF_ARRAY,
        "->%*"  => POSTFIX_DEREF_HASH,
        "->$*"  => POSTFIX_DEREF_SCALAR,
        "->$#*" => POSTFIX_DEREF_ARRAY_LAST_INDEX,
        "->&*"  => POSTFIX_DEREF_CODE,
        "->**"  => POSTFIX_DEREF_GLOB,
    }

    // ===== Spellings that mean two things =====
    //
    // `%`, `*` and `&` are a sigil in term position and an operator in operator
    // position. Which token the lexer emits is decided by its single `expect`
    // state (the lexer contract) — the old "look at the raw characters either side"
    // rule is gone. These get no `T!` key precisely because the spelling does
    // not identify the kind.
    punct_ctx {
        "%"    => HASH_SIGIL,
        "%"    => MODULO,
        "*"    => TYPEGLOB_SIGIL,
        "*"    => STAR,
        "&"    => CODE_SIGIL,
        "&"    => BITWISE_AND,
    }

    // ===== Trivia (the trivia model) =====
    //
    // NEWLINE is exactly one line terminator, so consecutive blank lines survive
    // in the token stream and the formatter never has to re-read the source to
    // count them.
    trivia {
        WHITESPACE : "whitespace",
        NEWLINE    : "newline",
        COMMENT    : "comment",
    }

    // ===== Tokens without a fixed spelling =====
    tokens {
        IDENT               : "identifier",
        NUMBER              : "number",
        VERSION             : "version string",
        STRING              : "string literal",

        // Quote-like operators are emitted as an atomic run of tokens
        // (the lexer contract): keyword, delimiter, content, [delimiter, content,]
        // delimiter, flags.
        DELIMITER           : "delimiter",
        LITERAL_STRING      : "string contents",
        INTERPOLATED_STRING : "interpolated string contents",
        REGEX_PATTERN       : "regex pattern",
        TR_SEARCH_LIST      : "transliteration search list",
        TR_REPLACEMENT_LIST : "transliteration replacement list",
        QW_STRING           : "word list contents",
        REGEX_FLAGS         : "regex flags",

        HEREDOC_START       : "heredoc marker",
        HEREDOC_CONTENT     : "heredoc body",
        HEREDOC_END         : "heredoc terminator",

        POD_CONTENT         : "POD block",
        DATA_CONTENT        : "data section",
        // The picture lines of a `format` declaration, from the line after the
        // `=` to the line holding only `.`. Every character is significant:
        // `@<<<<` is a left-justified field five wide, and `@< << <` is four
        // things that are not.
        FORMAT_CONTENT      : "format picture",

        FILE_TEST_OP        : "file test operator",
        // `<STDIN>`, `<$fh>`, `<>`. Scanned as one token so that the parser
        // never has to re-decide whether `<` opened a readline or a comparison.
        IO_HANDLE           : "readline operator",

        // A span of source carried through verbatim with a kind attached, which
        // replaces the four ad-hoc escape hatches of the old lexer
        // (the language model). Prototype bodies and attribute arguments use this.
        RAW_CONTENT         : "raw text",

        // Failure is never silent (the lexer contract).
        UNTERMINATED_REGEX      : "unterminated regex",
        UNTERMINATED_QUOTE_LIKE : "unterminated quote-like operator",
        UNTERMINATED_HEREDOC    : "unterminated heredoc",
        UNTERMINATED_STRING     : "unterminated string literal",
        ERROR_CHAR              : "unexpected character",
    }

    // ===== Syntax nodes (the parser contract) =====
    nodes {
        ROOT => Root,

        // -- Statements. The generic `STMT` wrapper is gone; an expression
        //    statement is always an EXPR_STMT.
        EXPR_STMT => ExprStmt,
        VAR_DECL_STMT => VarDeclStmt,
        IF_STMT => IfStmt,
        LOOP_STMT => LoopStmt,
        SUB_DEF => SubDef,
        PACKAGE_STMT => PackageStmt,
        USE_STMT => UseStmt,
        NO_STMT => NoStmt,
        TRY_STMT => TryStmt,
        GIVEN_STMT => GivenStmt,
        BLOCK_STMT => BlockStmt,
        LABELED_STMT => LabeledStmt,
        PHASE_BLOCK => PhaseBlock,
        POD => Pod,
        DATA_SECTION => DataSection,
        FORMAT_DECL => FormatDecl,
        EMPTY_STMT => EmptyStmt,
        ERROR => Error,

        // -- Statement parts
        BLOCK => Block,
        LABEL => Label,
        ELSIF_CLAUSE => ElsifClause,
        ELSE_CLAUSE => ElseClause,
        CONTINUE_CLAUSE => ContinueClause,
        CATCH_CLAUSE => CatchClause,
        CATCH_CLASS => CatchClass,
        CATCH_PARAM => CatchParam,
        FINALLY_CLAUSE => FinallyClause,
        WHEN_CLAUSE => WhenClause,
        DEFAULT_CLAUSE => DefaultClause,
        STMT_MODIFIER => StmtModifier,
        LOOP_HEADER => LoopHeader,
        C_STYLE_LOOP_HEADER => CStyleLoopHeader,
        FOREACH_HEADER => ForeachHeader,

        // -- Subroutine parts
        SUB_NAME => SubName,
        SUB_PROTOTYPE => SubPrototype,
        SUB_SIGNATURE => SubSignature,
        SIGNATURE_PARAM => SignatureParam,
        SIGNATURE_DEFAULT => SignatureDefault,
        ATTR => Attr,
        ATTR_ARGS => AttrArgs,

        // -- Declarations
        VAR_DECL => VarDecl,
        DECL_TARGET => DeclTarget,

        // -- Expressions. Operator classes are distinguished so that the
        //    formatter and future lints can branch on them (the parser contract).
        BINARY_EXPR => BinaryExpr,
        ASSIGN_EXPR => AssignExpr,
        TERNARY_EXPR => TernaryExpr,
        PREFIX_EXPR => PrefixExpr,
        POSTFIX_EXPR => PostfixExpr,
        RANGE_EXPR => RangeExpr,
        LIST_EXPR => ListExpr,
        PAREN_EXPR => ParenExpr,

        // -- Calls. The old six-way-ambiguous FUNCTION_CALL_EXPR is split.
        CALL_EXPR => CallExpr,
        LIST_CALL_EXPR => ListCallExpr,
        METHOD_CALL_EXPR => MethodCallExpr,
        BLOCK_CALL_EXPR => BlockCallExpr,
        ARG_LIST => ArgList,
        FILEHANDLE => Filehandle,

        // -- Variables and dereferencing
        SCALAR_VAR => ScalarVar,
        ARRAY_VAR => ArrayVar,
        HASH_VAR => HashVar,
        CODE_VAR => CodeVar,
        TYPEGLOB_VAR => TypeglobVar,
        ARRAY_LAST_INDEX => ArrayLastIndex,
        DEREF_EXPR => DerefExpr,
        BLOCK_DEREF_EXPR => BlockDerefExpr,
        POSTFIX_DEREF_EXPR => PostfixDerefExpr,
        POSTFIX_ARRAY_SLICE_EXPR => PostfixArraySliceExpr,
        POSTFIX_HASH_SLICE_EXPR => PostfixHashSliceExpr,
        REFERENCE_EXPR => ReferenceExpr,

        // -- Subscripting and slicing
        HASH_SUBSCRIPT_EXPR => HashSubscriptExpr,
        ARRAY_SUBSCRIPT_EXPR => ArraySubscriptExpr,
        CODE_CALL_EXPR => CodeCallExpr,
        SLICE_EXPR => SliceExpr,
        SUBSCRIPT => Subscript,

        // -- Literals and quote-likes
        ANON_HASH => AnonHash,
        ANON_ARRAY => AnonArray,
        ANON_SUB_EXPR => AnonSubExpr,
        LITERAL => Literal,
        Q_EXPR => QExpr,
        QQ_EXPR => QqExpr,
        QX_EXPR => QxExpr,
        QW_EXPR => QwExpr,
        M_EXPR => MExpr,
        QR_EXPR => QrExpr,
        S_EXPR => SExpr,
        TR_EXPR => TrExpr,
        HEREDOC_EXPR => HeredocExpr,
        HEREDOC_BODY => HeredocBody,

        // -- Misc expression forms
        MATCH_EXPR => MatchExpr,
        IO_EXPR => IoExpr,
        FILE_TEST_EXPR => FileTestExpr,
        DO_BLOCK_EXPR => DoBlockExpr,
        REQUIRE_EXPR => RequireExpr,
        UNDEF_EXPR => UndefExpr,
    }
}

/// A rowan kind. Tokens occupy `0..TOKEN_COUNT`, nodes the range above it.
///
/// Constructed only through the `From` impls below, so the two spaces cannot be
/// confused by arithmetic accident.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyntaxKind(pub u16);

/// Prints the kind's name rather than its discriminant, so a dumped tree is
/// readable. rowan's own `Debug` for nodes defers to this.
impl std::fmt::Debug for SyntaxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.as_token(), self.as_node()) {
            (Some(token), _) => write!(f, "{token:?}"),
            (_, Some(node)) => write!(f, "{node:?}"),
            _ => write!(f, "SyntaxKind({})", self.0),
        }
    }
}

impl From<TokenKind> for SyntaxKind {
    fn from(kind: TokenKind) -> Self {
        SyntaxKind(kind as u16)
    }
}

impl From<NodeKind> for SyntaxKind {
    fn from(kind: NodeKind) -> Self {
        SyntaxKind(TOKEN_COUNT + kind as u16)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind.0)
    }
}

impl SyntaxKind {
    /// The token kind this represents, or `None` if it is a node kind.
    #[must_use]
    pub fn as_token(self) -> Option<TokenKind> {
        (self.0 < TOKEN_COUNT).then(|| {
            // Safe: the range check above matches the `#[repr(u16)]` layout, and
            // `SyntaxKind` is only ever built from the `From` impls above.
            unsafe { std::mem::transmute::<u16, TokenKind>(self.0) }
        })
    }

    /// The node kind this represents, or `None` if it is a token kind.
    #[must_use]
    pub fn as_node(self) -> Option<NodeKind> {
        let offset = self.0.checked_sub(TOKEN_COUNT)?;
        (offset < NODE_COUNT).then(|| unsafe { std::mem::transmute::<u16, NodeKind>(offset) })
    }

    /// Trivia classification, for callers holding an untyped kind.
    #[must_use]
    pub fn is_trivia(self) -> bool {
        self.as_token().is_some_and(TokenKind::is_trivia)
    }
}

impl std::fmt::Display for SyntaxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.as_token(), self.as_node()) {
            (Some(token), _) => write!(f, "{token}"),
            (_, Some(node)) => write!(f, "{node}"),
            _ => write!(f, "<invalid kind {}>", self.0),
        }
    }
}

/// The rowan `Language` for the redesigned front end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PerlLang;

impl rowan::Language for PerlLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        SyntaxKind(raw.0)
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind.0)
    }
}

/// Reads a token's kind back as a [`TokenKind`].
///
/// rowan hands back the untyped `SyntaxKind`; these put the split of the language model
/// back in place at the point of use, so the formatter matches on token kinds
/// and node kinds rather than on integers.
pub trait TokenExt {
    fn token_kind(&self) -> TokenKind;
}

impl TokenExt for rowan::SyntaxToken<PerlLang> {
    fn token_kind(&self) -> TokenKind {
        self.kind()
            .as_token()
            .expect("a token in the tree always carries a token kind")
    }
}

pub trait NodeExt {
    fn node_kind(&self) -> NodeKind;
}

impl NodeExt for rowan::SyntaxNode<PerlLang> {
    fn node_kind(&self) -> NodeKind {
        self.kind()
            .as_node()
            .expect("a node in the tree always carries a node kind")
    }
}

pub type SyntaxNode = rowan::SyntaxNode<PerlLang>;
pub type SyntaxToken = rowan::SyntaxToken<PerlLang>;
pub type SyntaxElement = rowan::SyntaxElement<PerlLang>;

#[cfg(test)]
mod tests;
