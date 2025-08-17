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
    
    // Sigils（変数の型を示すプレフィックス）
    DOLLAR,    // $
    AT,        // @
    PERCENT,   // %
    
    // 複合変数ノード（後で使用）
    SCALAR_VAR,
    ARRAY_VAR,
    HASH_VAR,

    // リテラル
    NUMBER,
    STRING,

    // キーワード
    SUB_KW,
    MY_KW,
    IF_KW,
    ELSE_KW,
    PACKAGE_KW,

    // 記号・区切り文字
    L_BRACE,   // {
    R_BRACE,   // }
    L_PAREN,   // (
    R_PAREN,   // )
    SEMICOLON, // ;
    COMMA,     // ,
    DOUBLE_COLON, // ::

    // 演算子
    EQ,    // =
    PLUS,  // +
    MINUS, // -
    ARROW, // =>
    
    // Multiplicative operators
    STAR,    // *
    SLASH,   // /
    MODULO,  // % (modulo operator)
    X,       // x (repetition)

    // ===== ノードレベル（複合構造） =====
    ROOT,             // ファイルのルート
    SUB_DEF,          // サブルーチン定義
    BLOCK_STMT,       // ブロック文
    
    // 宣言文
    DECLARATION_STMT, // 変数宣言（my, our, state など）
    PACKAGE_STMT,     // パッケージ宣言（package Foo::Bar）
    
    // 式
    INFIX_EXPR,       // 中置式（二項演算式）
    PREFIX_EXPR,      // 前置式（単項演算式、例: !$foo, -$x）
    POSTFIX_EXPR,     // 後置式（例: $i++, $i--）
    
    // リテラル・リファレンス
    HASH_REF,         // ハッシュリファレンス（匿名ハッシュ）
    
    // 修飾子
    IF_MODIFIER,      // 後置if修飾子（例: print "hello" if $debug;）
    
    // その他の文
    STMT,             // 一般的な文

    // ===== その他 =====
    ERROR, // パースエラー
    EOF,   // ファイル終端
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }

    pub fn is_keyword(self) -> bool {
        matches!(
            self,
            SyntaxKind::SUB_KW | SyntaxKind::MY_KW | SyntaxKind::IF_KW | SyntaxKind::ELSE_KW | SyntaxKind::PACKAGE_KW
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
            SyntaxKind::EQ | SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::ARROW |
            SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::MODULO | SyntaxKind::X
        )
    }
}

impl std::fmt::Display for SyntaxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            SyntaxKind::WHITESPACE => "WHITESPACE",
            SyntaxKind::COMMENT => "COMMENT",
            SyntaxKind::IDENT => "IDENT",
            SyntaxKind::DOLLAR => "DOLLAR",
            SyntaxKind::AT => "AT",
            SyntaxKind::PERCENT => "PERCENT",
            SyntaxKind::SCALAR_VAR => "SCALAR_VAR",
            SyntaxKind::ARRAY_VAR => "ARRAY_VAR",
            SyntaxKind::HASH_VAR => "HASH_VAR",
            SyntaxKind::NUMBER => "NUMBER",
            SyntaxKind::STRING => "STRING",
            SyntaxKind::SUB_KW => "SUB_KW",
            SyntaxKind::MY_KW => "MY_KW",
            SyntaxKind::IF_KW => "IF_KW",
            SyntaxKind::ELSE_KW => "ELSE_KW",
            SyntaxKind::PACKAGE_KW => "PACKAGE_KW",
            SyntaxKind::L_BRACE => "L_BRACE",
            SyntaxKind::R_BRACE => "R_BRACE",
            SyntaxKind::L_PAREN => "L_PAREN",
            SyntaxKind::R_PAREN => "R_PAREN",
            SyntaxKind::SEMICOLON => "SEMICOLON",
            SyntaxKind::COMMA => "COMMA",
            SyntaxKind::DOUBLE_COLON => "DOUBLE_COLON",
            SyntaxKind::EQ => "EQ",
            SyntaxKind::PLUS => "PLUS",
            SyntaxKind::MINUS => "MINUS",
            SyntaxKind::ARROW => "ARROW",
            SyntaxKind::STAR => "STAR",
            SyntaxKind::SLASH => "SLASH",
            SyntaxKind::MODULO => "MODULO",
            SyntaxKind::X => "X",
            SyntaxKind::ROOT => "ROOT",
            SyntaxKind::SUB_DEF => "SUB_DEF",
            SyntaxKind::BLOCK_STMT => "BLOCK_STMT",
            SyntaxKind::DECLARATION_STMT => "DECLARATION_STMT",
            SyntaxKind::PACKAGE_STMT => "PACKAGE_STMT",
            SyntaxKind::INFIX_EXPR => "INFIX_EXPR",
            SyntaxKind::PREFIX_EXPR => "PREFIX_EXPR",
            SyntaxKind::POSTFIX_EXPR => "POSTFIX_EXPR",
            SyntaxKind::HASH_REF => "HASH_REF",
            SyntaxKind::IF_MODIFIER => "IF_MODIFIER",
            SyntaxKind::STMT => "STMT",
            SyntaxKind::ERROR => "ERROR",
            SyntaxKind::EOF => "EOF",
        };
        write!(f, "{}", name)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}
