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

    // 記号・区切り文字
    L_BRACE,   // {
    R_BRACE,   // }
    L_PAREN,   // (
    R_PAREN,   // )
    SEMICOLON, // ;
    COMMA,     // ,

    // 演算子
    EQ,    // =
    PLUS,  // +
    MINUS, // -
    ARROW, // =>

    // ===== ノードレベル（複合構造） =====
    ROOT,        // ファイルのルート
    SUB_DEF,     // サブルーチン定義
    BLOCK_STMT,  // ブロック文
    VAR_DECL,    // 変数宣言
    BINARY_EXPR, // 二項演算式
    HASH_REF,    // ハッシュリファレンス（匿名ハッシュ）
    STMT,        // 文

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
            SyntaxKind::SUB_KW | SyntaxKind::MY_KW | SyntaxKind::IF_KW | SyntaxKind::ELSE_KW
        )
    }

    pub fn is_variable(self) -> bool {
        matches!(
            self,
            SyntaxKind::SCALAR_VAR | SyntaxKind::ARRAY_VAR | SyntaxKind::HASH_VAR
        )
    }

    pub fn is_operator(self) -> bool {
        matches!(self, SyntaxKind::EQ | SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::ARROW)
    }
}

impl std::fmt::Display for SyntaxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            SyntaxKind::WHITESPACE => "WHITESPACE",
            SyntaxKind::COMMENT => "COMMENT",
            SyntaxKind::IDENT => "IDENT",
            SyntaxKind::SCALAR_VAR => "SCALAR_VAR",
            SyntaxKind::ARRAY_VAR => "ARRAY_VAR",
            SyntaxKind::HASH_VAR => "HASH_VAR",
            SyntaxKind::NUMBER => "NUMBER",
            SyntaxKind::STRING => "STRING",
            SyntaxKind::SUB_KW => "SUB_KW",
            SyntaxKind::MY_KW => "MY_KW",
            SyntaxKind::IF_KW => "IF_KW",
            SyntaxKind::ELSE_KW => "ELSE_KW",
            SyntaxKind::L_BRACE => "L_BRACE",
            SyntaxKind::R_BRACE => "R_BRACE",
            SyntaxKind::L_PAREN => "L_PAREN",
            SyntaxKind::R_PAREN => "R_PAREN",
            SyntaxKind::SEMICOLON => "SEMICOLON",
            SyntaxKind::COMMA => "COMMA",
            SyntaxKind::EQ => "EQ",
            SyntaxKind::PLUS => "PLUS",
            SyntaxKind::MINUS => "MINUS",
            SyntaxKind::ARROW => "ARROW",
            SyntaxKind::ROOT => "ROOT",
            SyntaxKind::SUB_DEF => "SUB_DEF",
            SyntaxKind::BLOCK_STMT => "BLOCK_STMT",
            SyntaxKind::VAR_DECL => "VAR_DECL",
            SyntaxKind::BINARY_EXPR => "BINARY_EXPR",
            SyntaxKind::HASH_REF => "HASH_REF",
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
