# Camello アーキテクチャ設計

## 全体アーキテクチャ

### データフロー概要
```
Perl Source Code
     ↓
[ Lexer (logos) ]
     ↓
   Token Stream
     ↓
[ Parser (rowan) ]
     ↓
CST (Concrete Syntax Tree)
     ↓
[ Formatter ]
     ↓
Formatted Perl Code
```

## コアコンポーネント

### 1. SyntaxKind (syntax_kind.rs)

Perlの構文要素を表現する中央的な型定義。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // トークンレベル
    WHITESPACE, COMMENT,
    IDENT, SCALAR_VAR, ARRAY_VAR, HASH_VAR,
    NUMBER, STRING,
    SUB_KW, MY_KW, IF_KW, ELSE_KW,
    L_BRACE, R_BRACE, L_PAREN, R_PAREN,
    SEMICOLON, COMMA, EQ, PLUS, MINUS,
    
    // ノードレベル（複合構造）
    ROOT, SUB_DEF, BLOCK_STMT, VAR_DECL, BINARY_EXPR,
    
    ERROR,
}
```

**設計原則**:
- `#[repr(u16)]` でRowanとの効率的な統合
- トークン（葉ノード）とノード（複合構造）を明確に分離
- エラー回復のためのERRORバリアント

### 2. Lexer (lexer.rs)

Logosクレートベースの高性能字句解析器。

**特徴**:
- 正規表現ベースのトークン認識
- ゼロコストな抽象化
- エラー回復メカニズム

**実装例**:
```rust
use logos::Logos;

#[derive(Logos, Debug, PartialEq)]
pub enum Token {
    #[regex(r"\$[a-zA-Z_][a-zA-Z0-9_]*")]
    ScalarVar,
    
    #[token("sub")]
    SubKw,
    
    #[token("my")]
    MyKw,
    
    #[regex(r"[0-9]+(\.[0-9]+)?")]
    Number,
    
    #[regex(r#""([^"\\]|\\.)*""#)]
    String,
    
    #[regex(r"[ \t\f]+", logos::skip)]
    #[regex(r"\r\n|\r|\n")]
    Whitespace,
    
    #[regex(r"#[^\r\n]*")]
    Comment,
}
```

### 3. Parser (parser.rs)

Rowanベースの再帰下降パーサー。

**コア構造**:
```rust
pub struct Parser<'a> {
    lexer: Peekable<logos::Lexer<'a, Token>>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
}

impl<'a> Parser<'a> {
    pub fn parse(&mut self) -> (GreenNode, Vec<ParseError>) {
        self.root();
        let green_node = self.builder.finish();
        let errors = std::mem::take(&mut self.errors);
        (green_node, errors)
    }
}
```

**主要パース関数**:
- `root()` - ファイル全体
- `statement()` - 文レベル
- `var_decl()` - 変数宣言
- `sub_def()` - サブルーチン定義
- `expression()` - 式レベル
- `block()` - ブロック構造

**エラー回復戦略**:
1. **スキップ戦略**: 不明なトークンをERRORノードとして包含
2. **同期戦略**: セミコロンや閉じ括弧まで読み飛ばし
3. **部分パース**: 文の一部が無効でも全体の解析は継続

### 4. Formatter (formatter.rs)

CSTを走査してフォーマット済みコードを生成。

**コア構造**:
```rust
pub struct Formatter {
    indent_level: usize,
    indent_size: usize,
    output: String,
    prev_token: Option<SyntaxKind>,
}

impl Formatter {
    pub fn format(&mut self, node: &SyntaxNode) -> String {
        self.format_node(node);
        std::mem::take(&mut self.output)
    }
    
    fn format_node(&mut self, node: &SyntaxNode) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => self.format_node(&node),
                NodeOrToken::Token(token) => self.format_token(&token),
            }
        }
    }
}
```

**フォーマットルール実装**:

1. **インデント管理**:
```rust
fn handle_indent(&mut self, token_kind: SyntaxKind) {
    match token_kind {
        L_BRACE => {
            self.output.push('{');
            self.indent_level += 1;
        }
        R_BRACE => {
            self.indent_level = self.indent_level.saturating_sub(1);
            self.add_newline_with_indent();
            self.output.push('}');
        }
        _ => {}
    }
}
```

2. **スペース調整**:
```rust
fn needs_space_before(&self, current: SyntaxKind) -> bool {
    match (self.prev_token, current) {
        (Some(EQ), _) | (_, EQ) => true,
        (Some(PLUS), _) | (_, PLUS) => true,
        (Some(COMMA), _) => true,
        (Some(SUB_KW), IDENT) => true,
        (Some(MY_KW), SCALAR_VAR) => true,
        _ => false,
    }
}
```

### 5. CLI (cli.rs)

Clapベースのコマンドラインインターフェース。

**コマンド構造**:
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "camello")]
#[command(about = "A Perl code formatter")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Format {
        /// Path to Perl file or directory
        path: PathBuf,
        
        /// Output to file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        
        /// Check if file is already formatted
        #[arg(long)]
        check: bool,
    },
}
```

## データ構造の詳細

### CST（具象構文木）の特徴

**Lossless（無損失）**:
- 元ソースのすべての文字（空白、コメント含む）を保持
- フォーマット時にコメント位置の適切な調整が可能

**Error Resilient（エラー耐性）**:
- 部分的な構文エラーがあっても全体の解析は継続
- IDEライクなツール開発に適している

**Memory Efficient（メモリ効率）**:
- Rowanのinterning機構により、同一構造の共有
- 大きなファイルでも効率的な処理

### トークン管理

**トークンの分類**:
1. **構文的トークン**: キーワード、演算子、区切り文字
2. **識別子トークン**: 変数名、サブルーチン名
3. **リテラルトークン**: 数値、文字列
4. **トリビアトークン**: 空白、コメント

**トリビア処理**:
```rust
// Rowanでは"trivia"として扱われる
// フォーマット時に適切に配置される
pub fn format_trivia(&mut self, token: &SyntaxToken) {
    match token.kind() {
        WHITESPACE => self.handle_whitespace(token),
        COMMENT => self.preserve_comment(token),
        _ => {}
    }
}
```

## エラーハンドリング戦略

### パースエラー
```rust
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: TextRange,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy)]
pub enum Severity {
    Error,      // 続行不可能
    Warning,    // 続行可能だが問題あり
    Info,       // 情報のみ
}
```

### 回復戦略
1. **ローカル回復**: 単一トークンの修正
2. **句レベル回復**: 文や式の境界まで読み飛ばし
3. **グローバル回復**: 次の既知の構造まで移動

## パフォーマンス考慮事項

### メモリ使用量
- Rowanのinterning による構造共有
- 文字列のSmolStr使用による効率化
- ストリーミング処理によるメモリ使用量抑制

### 処理速度
- Logos による高速字句解析
- Zero-allocation な文字列操作
- 不要なCloneの回避

### スケーラビリティ
- 大きなPerlファイルに対応
- 並列処理の将来対応（複数ファイル）
- インクリメンタル解析の可能性

## テスト戦略

### スナップショットテスト
```rust
#[test]
fn test_basic_formatting() {
    let input = r#"
sub example{
my$x=1;
print$x;
}
    "#;
    
    let formatted = format_perl_code(input).unwrap();
    insta::assert_snapshot!(formatted);
}
```

### プロパティベーステスト
- ランダムなPerlコードの生成
- フォーマット前後での意味保持の検証
- エラー回復の堅牢性テスト

この設計により、堅牢で保守しやすいPerlフォーマッタの実装が可能になります。