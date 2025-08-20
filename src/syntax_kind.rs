#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // ===== トークンレベル =====

    // トリビア（空白・コメント）
    WHITESPACE,
    COMMENT,

    // 識別子・変数
    IDENT,
    QUALIFIED_IDENT, // 修飾付き識別子（Foo::Bar::baz）

    // Sigils（変数の型を示すプレフィックス）
    DOLLAR,  // $
    AT,      // @
    PERCENT, // %

    // Special variable components
    UNDERSCORE, // _ for $_
    DIGITS,     // digits for $1, $2, etc.
    CARET,      // ^ for ${^NAME}

    // Special punctuation characters for variables
    EXCLAMATION,  // !
    QUESTION,     // ?
    PIPE,         // |
    AMPERSAND,    // &
    BACKTICK,     // `
    SINGLE_QUOTE, // '
    DOUBLE_QUOTE, // "
    TILDE,        // ~
    COLON,        // :
    BACKSLASH,    // \

    // 複合変数ノード（後で使用）
    SCALAR_VAR,
    ARRAY_VAR,
    HASH_VAR,

    // Special variable types
    SPECIAL_VAR,  // for punctuation-based variables like $!, $?, $$
    CAPTURE_VAR,  // for $1, $2, etc.
    EXTENDED_VAR, // for ${^NAME} syntax
    DEFAULT_VAR,  // for $_

    // リテラル
    NUMBER,
    STRING,
    REGEX_LITERAL, // /pattern/flags

    // キーワード
    SUB_KW,
    MY_KW,
    IF_KW,
    ELSIF_KW,
    ELSE_KW,
    FOR_KW,     // for keyword
    FOREACH_KW, // foreach keyword (synonym for for)
    WHILE_KW,   // while keyword
    PACKAGE_KW,
    QW_KW,  // qw keyword
    USE_KW, // use keyword (for use warnings qw(...) syntax)

    // データセクション
    END_KW,  // __END__
    DATA_KW, // __DATA__

    // 記号・区切り文字
    L_BRACE,      // {
    R_BRACE,      // }
    L_PAREN,      // (
    R_PAREN,      // )
    L_BRACKET,    // [
    R_BRACKET,    // ]
    SEMICOLON,    // ;
    COMMA,        // ,
    DOUBLE_COLON, // ::

    // qw() 専用
    QW_STRING, // qw()内の任意のテキスト

    // 演算子
    EQ,        // =
    PLUS,      // +
    MINUS,     // -
    ARROW,     // ->
    FAT_COMMA, // =>

    // Multiplicative operators
    STAR,   // *
    SLASH,  // /
    MODULO, // % (modulo operator)
    X,      // x (repetition)

    // Comparison operators
    GT,    // >
    LT,    // <
    GE,    // >=
    LE,    // <=
    EQ_EQ, // ==
    NE,    // !=

    // Regex operators
    REGEX_MATCH,     // =~
    REGEX_NOT_MATCH, // !~

    // Logical operators
    LOGICAL_AND, // &&
    LOGICAL_OR,  // ||

    // ===== ノードレベル（複合構造） =====
    ROOT,       // ファイルのルート
    SUB_DEF,    // サブルーチン定義
    BLOCK_STMT, // ブロック文

    // 宣言文
    DECLARATION_STMT, // 変数宣言（my, our, state など）
    PACKAGE_STMT,     // パッケージ宣言（package Foo::Bar）
    USE_STMT,         // use文（use warnings qw(all);）
    FOR_STMT,         // for文
    WHILE_STMT,       // while文
    IF_STMT,          // if文

    // データセクション（__END__ / __DATA__ 以降の内容）
    DATA_SECTION,
    RAW_STRING,

    // 式
    INFIX_EXPR,               // 中置式（二項演算式）
    PREFIX_EXPR,              // 前置式（単項演算式、例: !$foo, -$x）
    POSTFIX_EXPR,             // 後置式（例: $i++, $i--）
    METHOD_CALL_EXPR,         // メソッド呼び出し式（例: $obj->method()）
    FUNCTION_CALL_EXPR,       // 関数呼び出し式（例: push @array, $value）
    BLOCK_FUNCTION_CALL_EXPR, // ブロック関数呼び出し式（例: eval { ... }, map { ... } @list）
    QW_EXPR,                  // qw() 式（クォートワードリスト）
    DEREF_EXPR,               // デリファレンス式（例: @$var, %$var, $$var）
    REGEX_EXPR,               // 正規表現式（例: $str =~ "pattern"）

    // リテラル・リファレンス
    HASH_REF,  // ハッシュリファレンス（匿名ハッシュ）
    ARRAY_REF, // 配列リファレンス（匿名配列）

    // 修飾子
    IF_MODIFIER, // 後置if修飾子（例: print "hello" if $debug;）

    // その他の文
    STMT, // 一般的な文

    EXPR_LIST, // 式のリスト（例: $a, $b, $c）

    // ===== その他 =====
    ERROR, // パースエラー
    EOF,   // ファイル終端
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
                | SyntaxKind::IF_KW
                | SyntaxKind::ELSIF_KW
                | SyntaxKind::ELSE_KW
                | SyntaxKind::FOR_KW
                | SyntaxKind::FOREACH_KW
                | SyntaxKind::WHILE_KW
                | SyntaxKind::PACKAGE_KW
                | SyntaxKind::QW_KW
                | SyntaxKind::USE_KW
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
            SyntaxKind::DOLLAR | SyntaxKind::AT | SyntaxKind::PERCENT
        )
    }

    pub fn is_operator(self) -> bool {
        matches!(
            self,
            SyntaxKind::EQ
                | SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::ARROW
                | SyntaxKind::FAT_COMMA
                | SyntaxKind::STAR
                | SyntaxKind::SLASH
                | SyntaxKind::MODULO
                | SyntaxKind::X
                | SyntaxKind::LOGICAL_AND
                | SyntaxKind::LOGICAL_OR
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
            SyntaxKind::UNDERSCORE => "UNDERSCORE",
            SyntaxKind::DIGITS => "DIGITS",
            SyntaxKind::CARET => "CARET",
            SyntaxKind::EXCLAMATION => "EXCLAMATION",
            SyntaxKind::QUESTION => "QUESTION",
            SyntaxKind::PIPE => "PIPE",
            SyntaxKind::AMPERSAND => "AMPERSAND",
            SyntaxKind::BACKTICK => "BACKTICK",
            SyntaxKind::SINGLE_QUOTE => "SINGLE_QUOTE",
            SyntaxKind::DOUBLE_QUOTE => "DOUBLE_QUOTE",
            SyntaxKind::TILDE => "TILDE",
            SyntaxKind::COLON => "COLON",
            SyntaxKind::BACKSLASH => "BACKSLASH",
            SyntaxKind::SCALAR_VAR => "SCALAR_VAR",
            SyntaxKind::ARRAY_VAR => "ARRAY_VAR",
            SyntaxKind::HASH_VAR => "HASH_VAR",
            SyntaxKind::SPECIAL_VAR => "SPECIAL_VAR",
            SyntaxKind::CAPTURE_VAR => "CAPTURE_VAR",
            SyntaxKind::EXTENDED_VAR => "EXTENDED_VAR",
            SyntaxKind::DEFAULT_VAR => "DEFAULT_VAR",
            SyntaxKind::NUMBER => "NUMBER",
            SyntaxKind::STRING => "STRING",
            SyntaxKind::REGEX_LITERAL => "REGEX_LITERAL",
            SyntaxKind::SUB_KW => "SUB_KW",
            SyntaxKind::MY_KW => "MY_KW",
            SyntaxKind::IF_KW => "IF_KW",
            SyntaxKind::ELSIF_KW => "ELSIF_KW",
            SyntaxKind::ELSE_KW => "ELSE_KW",
            SyntaxKind::FOR_KW => "FOR_KW",
            SyntaxKind::FOREACH_KW => "FOREACH_KW",
            SyntaxKind::WHILE_KW => "WHILE_KW",
            SyntaxKind::PACKAGE_KW => "PACKAGE_KW",
            SyntaxKind::QW_KW => "QW_KW",
            SyntaxKind::USE_KW => "USE_KW",
            SyntaxKind::L_BRACE => "L_BRACE",
            SyntaxKind::R_BRACE => "R_BRACE",
            SyntaxKind::L_PAREN => "L_PAREN",
            SyntaxKind::R_PAREN => "R_PAREN",
            SyntaxKind::L_BRACKET => "L_BRACKET",
            SyntaxKind::R_BRACKET => "R_BRACKET",
            SyntaxKind::QW_STRING => "QW_STRING",
            SyntaxKind::SEMICOLON => "SEMICOLON",
            SyntaxKind::COMMA => "COMMA",
            SyntaxKind::DOUBLE_COLON => "DOUBLE_COLON",
            SyntaxKind::EQ => "EQ",
            SyntaxKind::PLUS => "PLUS",
            SyntaxKind::MINUS => "MINUS",
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
            SyntaxKind::REGEX_MATCH => "REGEX_MATCH",
            SyntaxKind::REGEX_NOT_MATCH => "REGEX_NOT_MATCH",
            SyntaxKind::LOGICAL_AND => "LOGICAL_AND",
            SyntaxKind::LOGICAL_OR => "LOGICAL_OR",
            SyntaxKind::ROOT => "ROOT",
            SyntaxKind::SUB_DEF => "SUB_DEF",
            SyntaxKind::BLOCK_STMT => "BLOCK_STMT",
            SyntaxKind::DECLARATION_STMT => "DECLARATION_STMT",
            SyntaxKind::PACKAGE_STMT => "PACKAGE_STMT",
            SyntaxKind::USE_STMT => "USE_STMT",
            SyntaxKind::FOR_STMT => "FOR_STMT",
            SyntaxKind::WHILE_STMT => "WHILE_STMT",
            SyntaxKind::IF_STMT => "IF_STMT",
            SyntaxKind::INFIX_EXPR => "INFIX_EXPR",
            SyntaxKind::PREFIX_EXPR => "PREFIX_EXPR",
            SyntaxKind::POSTFIX_EXPR => "POSTFIX_EXPR",
            SyntaxKind::METHOD_CALL_EXPR => "METHOD_CALL_EXPR",
            SyntaxKind::QW_EXPR => "QW_EXPR",
            SyntaxKind::DEREF_EXPR => "DEREF_EXPR",
            SyntaxKind::REGEX_EXPR => "REGEX_EXPR",
            SyntaxKind::HASH_REF => "HASH_REF",
            SyntaxKind::ARRAY_REF => "ARRAY_REF",
            SyntaxKind::IF_MODIFIER => "IF_MODIFIER",
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
        };
        write!(f, "{}", name)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}
