# Camello 実装時の考慮事項

## Perlの特殊性と対処法

### 1. 文脈依存の構文

Perlは同じ記号が文脈によって異なる意味を持ちます：

```perl
# スカラー文脈
my $count = @array;  # 配列の要素数

# リスト文脈  
my @copy = @array;   # 配列のコピー

# ハッシュのスライス vs 配列の参照
my @slice = @hash{@keys};    # ハッシュスライス
my $ref = \@array;           # 配列への参照
```

**対処法**:
- 初期実装では基本的な文脈のみサポート
- 将来的にLookahead/Lookbehindを使った文脈解析
- あいまいな場合はエラー回復でスキップ

### 2. 多様な引用符・文字列リテラル

```perl
my $single = 'simple string';
my $double = "interpolated $var";  
my $qq = qq{custom delimiters};
my $heredoc = <<EOF;
Multi-line
content
EOF
```

**段階的対応**:
1. Phase 2: 基本的なシングル・ダブルクォート
2. Phase 5: q/qq演算子とカスタム区切り文字
3. 将来: ヒアドキュメント

### 3. 正規表現リテラル

```perl
my $pattern = /regex/flags;
my $match = m{alternative}i;
my $substitute = s/from/to/g;
```

**実装戦略**:
- 正規表現は文字列として扱い、内部解析は行わない
- フォーマットは区切り文字の統一のみ

## Rowanライブラリの活用

### CSTの構築パターン

```rust
// パーサーでの典型的なノード構築
fn parse_var_decl(&mut self) -> CompletedMarker {
    let m = self.start();  // ノード開始
    
    self.expect(MY_KW);         // "my" 
    self.expect(SCALAR_VAR);    // "$var"
    
    if self.at(EQ) {
        self.bump(EQ);          // "="
        self.parse_expr();      // 式
    }
    
    self.expect(SEMICOLON);     // ";"
    m.complete(VAR_DECL)        // ノード完了
}
```

### エラー回復のベストプラクティス

```rust
fn recover_to_stmt_boundary(&mut self) {
    // 文の境界（セミコロン、閉じブレース）まで読み進める
    while !self.at_any(&[SEMICOLON, R_BRACE, EOF]) {
        if self.at_any(&[SUB_KW, MY_KW]) {
            // 次の文の開始を発見
            break;
        }
        self.bump_any(); // エラートークンとして消費
    }
}
```

### メモリ効率的なCST操作

```rust
// ❌ 非効率: 不要なClone
fn bad_example(node: SyntaxNode) -> String {
    let children: Vec<_> = node.children().collect(); // 全体をCollect
    // ...
}

// ✅ 効率的: ストリーミング処理
fn good_example(node: &SyntaxNode) -> String {
    let mut result = String::new();
    for child in node.children() { // イテレータを直接使用
        // ...
    }
    result
}
```

## フォーマッタの実装パターン

### 状態管理

```rust
#[derive(Debug, Clone)]
pub struct FormatState {
    // インデント管理
    pub indent_level: usize,
    pub indent_string: String,  // "    " or "\t"
    
    // 行管理
    pub current_line_length: usize,
    pub max_line_length: usize,
    
    // コンテキスト
    pub in_expression: bool,
    pub in_string: bool,
    pub after_comma: bool,
    
    // 前のトークン情報
    pub prev_token_kind: Option<SyntaxKind>,
    pub prev_was_newline: bool,
}
```

### ルールベースのスペーシング

```rust
fn spacing_rule(prev: SyntaxKind, current: SyntaxKind) -> SpaceAction {
    use SyntaxKind::*;
    match (prev, current) {
        // 演算子の前後
        (_, EQ) | (EQ, _) => SpaceAction::RequireSpace,
        (_, PLUS) | (PLUS, _) => SpaceAction::RequireSpace,
        
        // カンマの後
        (COMMA, _) => SpaceAction::RequireSpace,
        
        // 括弧の内側
        (L_PAREN, _) | (_, R_PAREN) => SpaceAction::NoSpace,
        
        // キーワードの後
        (SUB_KW, IDENT) | (MY_KW, SCALAR_VAR) => SpaceAction::RequireSpace,
        
        _ => SpaceAction::Preserve,
    }
}

#[derive(Debug, Clone, Copy)]
enum SpaceAction {
    RequireSpace,   // 必ず1つのスペース
    NoSpace,        // スペースなし
    Preserve,       // 元の空白を保持
    OptionalSpace,  // 好みに応じて
}
```

### 改行の制御

```rust
fn newline_rule(token: SyntaxKind, context: &FormatState) -> NewlineAction {
    match token {
        SEMICOLON => NewlineAction::ForceAfter,
        L_BRACE => {
            if context.indent_level > 0 {
                NewlineAction::ForceAfter  // ブロック内では改行
            } else {
                NewlineAction::OptionalAfter // トップレベルでは柔軟
            }
        }
        R_BRACE => NewlineAction::ForceBefore,
        _ => NewlineAction::Preserve,
    }
}
```

## テスト実装のガイドライン

### スナップショットテストの構造

```
tests/
├── snapshots/
│   ├── basic_formatting.snap
│   ├── error_recovery.snap
│   └── edge_cases.snap
├── fixtures/
│   ├── basic/
│   │   ├── input.pl
│   │   └── expected.pl
│   └── complex/
│       ├── real_world_script.pl
│       └── expected.pl
└── integration_test.rs
```

### テストケースの分類

**1. Golden Path Tests（正常系）**:
```rust
#[test]
fn format_variable_declaration() {
    let input = "my$var=1;";
    let expected = "my $var = 1;\n";
    assert_eq!(format_code(input), expected);
}
```

**2. Error Recovery Tests（エラー回復）**:
```rust
#[test] 
fn handle_missing_semicolon() {
    let input = "my $var = 1\nmy $other = 2;";
    let formatted = format_code(input);
    // エラーはあるが、次の文は正常にフォーマットされる
    assert!(formatted.contains("my $other = 2;"));
}
```

**3. Edge Case Tests（境界条件）**:
```rust
#[test]
fn empty_file() {
    assert_eq!(format_code(""), "");
}

#[test]
fn only_comments() {
    let input = "# This is a comment\n# Another comment";
    let formatted = format_code(input);
    assert!(formatted.starts_with("# This is a comment"));
}
```

## パフォーマンス最適化

### プロファイリングポイント

1. **字句解析**: Logos のパフォーマンス測定
2. **構文解析**: パーサーのボトルネック特定  
3. **フォーマッタ**: CST走査の効率性
4. **メモリ使用量**: 大きなファイルでのメモリプロファイル

### 最適化戦略

```rust
// ✅ 事前にString容量を確保
let mut output = String::with_capacity(estimated_size);

// ✅ 文字列結合の最適化
use std::fmt::Write;
write!(output, "{}", formatted_token)?;

// ✅ アロケーション削減
let token_str = token.text(); // &str を直接使用（Cloneしない）
```

## デバッグとトラブルシューティング

### CSTの可視化

```rust
pub fn debug_print_cst(node: &SyntaxNode, indent: usize) {
    let indent_str = "  ".repeat(indent);
    println!("{}Node: {:?}", indent_str, node.kind());
    
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(node) => debug_print_cst(&node, indent + 1),
            NodeOrToken::Token(token) => {
                println!("{}Token: {:?} '{}'", 
                    "  ".repeat(indent + 1), token.kind(), token.text());
            }
        }
    }
}
```

### よくある問題と解決法

**1. パースが途中で止まる**
- 原因: 無限ループまたは予期しないトークン
- 解決: `panic!` ではなく `expect()` と適切なエラー回復

**2. フォーマット結果が元と大きく異なる**  
- 原因: トリビア（空白・コメント）の不適切な処理
- 解決: CST内のすべてのトークンを確実に処理

**3. メモリリーク**
- 原因: CSTノードの循環参照
- 解決: Rowanは循環参照安全だが、カスタム構造に注意

## 将来の拡張性

### 設定システムの準備

```rust
#[derive(Debug, Clone)]  
pub struct FormatConfig {
    pub indent_size: usize,
    pub max_line_length: usize,
    pub brace_style: BraceStyle,
    pub space_around_operators: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum BraceStyle {
    KernighanRitchie, // if (condition) {
    Allman,           // if (condition)
                     //  {
}
```

### プラグインシステムの可能性

```rust
pub trait FormattingRule {
    fn applies_to(&self, context: &FormatContext) -> bool;
    fn apply(&self, token: &SyntaxToken, output: &mut String);
}
```

### LSP（Language Server Protocol）対応

将来的にエディタ統合のため：
- インクリメンタルフォーマット
- リアルタイム構文チェック  
- 部分フォーマット（選択範囲のみ）

これらの考慮事項により、保守性が高く拡張可能なフォーマッタを実装できます。