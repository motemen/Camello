# パーサー無限ループ問題の調査と解決

**日付**: 2025-01-17  
**問題**: `'sub f { return { } }'` のパーズが終わらない  
**解決**: `primary_expr()` に `L_BRACE` ケースを追加

## 問題の概要

Camelloパーサーで以下のコードを処理すると無限ループが発生していた：

```perl
sub f { return { } }
```

## 調査過程

### 1. 他のトークンとの比較

他のカッコ始まり文字 (`L_PAREN`, `L_BRACKET`) では無限ループが発生しないことを確認：

```perl
# これらは無限ループしない
sub f { return ( }
sub f { return [ }
```

### 2. 処理フローの分析

#### `L_PAREN` (`(`) の場合：
- `primary_expr()` で `error()` が呼ばれる
- トークンが消費される
- 対応する `)` が即座にないため、別の statement として処理される
- 無限ループにならない

#### `L_BRACKET` (`[`) の場合：
- 同様に `error()` で処理されてトークンが消費される
- 対応する `]` が即座にない場合が多い
- 無限ループにならない

#### `L_BRACE` (`{`) の場合（修正前）：
- `error()` でトークンが消費される
- **しかし直後に `}` があるケースで特殊な問題が発生**
- `{}` のペアで何らかの解析ループが起きていた

### 3. 無限ループの真の原因

問題は以下の流れで発生していた：

1. `sub f { return { } }` をパース
2. `block()` 関数で `{` の後の内容を処理
3. `while !self.at(SyntaxKind::R_BRACE)` でループ （parser.rs:153）
4. `statement()` が呼ばれて、`return` が `IDENT` として処理される
5. `return` の後の `{` で `expression_stmt()` → `expression()` → `primary_expr()` が呼ばれる
6. `primary_expr()` で `L_BRACE` がサポートされていないため `error()` が呼ばれる
7. **重要**: エラー処理後、`statement()` は `true` を返す（parser.rs:73の`Some(_)`ケース）
8. `while` ループが続行するが、まだ `R_BRACE` に到達していないため、再び `statement()` が呼ばれる
9. 同じ位置で同じ処理が繰り返される → 無限ループ

### 4. なぜ `L_BRACE` だけが特別だったか

- **ペアの問題**: `{}` は完全なペアとして頻繁に現れる
- **文脈の特殊性**: ブロック内で表現として `{` が現れるケースが特殊
- **他のトークンとの違い**: `(` や `[` は対応する閉じカッコが即座にない場合が多い

## 解決方法

`primary_expr()` に `L_BRACE` ケースを追加：

```rust
Some(SyntaxKind::L_BRACE) => {
    // ハッシュリテラルまたは匿名ハッシュ: {}
    self.builder.start_node(SyntaxKind::STMT.into());
    self.bump(); // {
    self.skip_trivia();
    self.expect(SyntaxKind::R_BRACE); // }
    self.builder.finish_node();
}
```

これにより、`{}` を1つの単位として適切に消費するようになり、無限ループが解決された。

## 学んだこと

1. **エラー処理の重要性**: エラー時にも適切にトークンが消費される必要がある
2. **文脈の考慮**: 同じトークンでも、現れる文脈によって処理が異なる
3. **ペア構造の特殊性**: `{}` のような完全なペア構造は特別な配慮が必要
4. **デバッグツールの価値**: `dump` サブコマンドにより問題の特定が効率的になった

## 関連ファイル

- `src/parser.rs`: `primary_expr()` 関数
- テストケース: `test_hash_literal()`, `test_sub_with_hash_literal()`

## 検証

修正後、以下のコードが正常に処理されることを確認：

```perl
sub f { return { } }
return {}
```

無限ループは完全に解決され、すべてのテストが通過している。