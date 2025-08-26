# 空行保持機能の分析と実装方針

**日付**: 2025-08-26
**問題**: ユーザーが書いた空行が保持されず、use文・sub文の前後に一律で空行が挿入される

## 問題の概要

現在のCamelloフォーマッタでは、以下の問題が発生している：

1. **ユーザーの意図的な空行が失われる**
   - 元のソースコードに空行があっても、フォーマット後に消える
   
2. **use文・sub文の前後に一律で空行挿入**
   - 元のソースの空行状況に関係なく、自動的に空行を挿入
   - 結果として、ユーザーの空行の意図が無視される

3. **複数行の空行の扱い**
   - 複数の連続空行を1つに詰めたいが、現在は適切に処理されていない

## 根本原因の分析

### 1. レクサーレベルでの情報損失

**場所**: `src/lexer.rs:198`
```rust
Token::Newline => SyntaxKind::WHITESPACE,
```

- レクサーでは`Newline`と`Whitespace`を別々に認識
- SyntaxKindへの変換時に`WHITESPACE`として統合され、改行情報が失われる

### 2. フォーマッタの単純な空白処理

**場所**: `src/formatter/mod.rs:781-789` (`handle_whitespace`)
```rust
fn handle_whitespace(&mut self, token: &SyntaxToken<crate::PerlLanguage>) {
    let text = token.text();
    
    // 改行を含む場合は改行処理を実行（従来のhandle_multiline_whitespaceの機能）
    if text.contains('\n') {
        self.handle_newline();
    }
    // 将来的にはこの関数でコンテキストを見て空行などを処理する予定
}
```

- 改行を含む空白を単純に`handle_newline()`で処理
- 連続する改行の数（空行情報）を考慮していない

### 3. 一律な空行挿入ロジック

**場所**: `src/formatter/mod.rs:726-756`
- `add_empty_line_before_if_needed`と`add_empty_line_after_if_needed`
- use文・sub文の前後に条件に関係なく空行を挿入
- 元のソースコードの空行情報を無視

## 改善方針

### 基本コンセプト
**「ユーザーの意図的な空行を保持しつつ、複数の連続空行は1つに正規化する」**

### アプローチ1: フォーマッタレベルでの改善（推奨）

**利点**: 
- 既存アーキテクチャを大きく変更せず実装可能
- 後方互換性を保持

**実装内容**:

#### 1. 空行情報の解析機能
- `handle_whitespace`関数を拡張して改行の数をカウント
- 元のソースに空行があった箇所を記録

#### 2. Formatterに空行管理フィールド追加
```rust
pub struct Formatter {
    output: String,
    indent_level: usize,
    indent_string: String,
    prev_token_kind: Option<SyntaxKind>,
    at_line_start: bool,
    pending_empty_lines: usize,  // 新規追加：処理待ちの空行数
}
```

#### 3. インテリジェントな空行処理
- **ユーザーの空行を優先**: 元のソースに空行がある場合はそれを保持
- **複数空行の正規化**: 2行以上の連続空行は1行に正規化
- **条件付き自動挿入**: use文・sub文の前後は、元の空行がない場合のみ自動挿入

#### 4. 改良された空行管理ロジック
```rust
fn handle_whitespace_with_empty_lines(&mut self, token: &SyntaxToken<PerlLanguage>) {
    let text = token.text();
    let newline_count = text.matches('\n').count();
    
    if newline_count > 0 {
        // 空行として扱う（1回の改行 = 通常の改行、2回以上 = 空行）
        if newline_count > 1 {
            self.pending_empty_lines = (newline_count - 1).min(1); // 最大1行の空行
        }
        self.handle_newline();
    }
}
```

### アプローチ2: アーキテクチャレベルでの改善

**内容**: SyntaxKindに`NEWLINE`を追加し、レクサーからの改行情報を保持

**課題**: 
- より大規模な変更が必要
- 既存コードへの影響が大きい
- パーサーの`is_trivia()`定義も変更が必要

## 実装計画

### フェーズ1: 基本的な空行保持機能
1. `handle_whitespace`関数を改良して空行情報を解析
2. `pending_empty_lines`フィールドをFormatterに追加
3. 適切なタイミングで空行を出力する機能

### フェーズ2: use文・sub文の空行処理改善
1. `add_empty_line_before_if_needed`を改良
   - 元のソースに空行がある場合は自動挿入をスキップ
2. `add_empty_line_after_if_needed`を改良
   - 同上の条件で自動挿入を制御

### フェーズ3: テストケース追加と検証
1. 様々な空行パターンをテストする新しいスナップショットテストを追加
2. 既存テストとの互換性を確保
3. エッジケースの検証

## 期待される結果

- **空行の保持**: ユーザーが書いた空行が適切に保持される
- **正規化**: 複数の連続空行は1つに正規化される
- **インテリジェントな自動挿入**: 元の空行がない場合のみuse文・sub文前後に空行を挿入
- **後方互換性**: 既存のテストケースとの互換性を維持

## テストケース例

```perl
# 入力
use strict;


use warnings;

sub foo {
    my $x = 1;
}


my $var = 42;

sub bar {
    return 1;
}

# 期待出力
use strict;

use warnings;

sub foo {
    my $x = 1;
}

my $var = 42;

sub bar {
    return 1;
}
```

## 実装上の注意点

1. **既存テストとの互換性**: `test_empty_lines_before_after_subs`などの既存テストケースを考慮
2. **パフォーマンス**: 大きなファイルでも効率的に動作するよう配慮
3. **エッジケース**: ファイル先頭・末尾、PODブロック、データセクションなどでの適切な処理