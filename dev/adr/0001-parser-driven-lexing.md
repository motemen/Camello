# ADR 0001: Parser-Driven Lexing (No-Cache MVP)

- Status: Proposed
- Date: 2025-09-11
- Owners: camello core
- Author: Codex

## Context

現状、lexer が「次は値か、演算子か」というコンテキスト（`ExpectingValue/ExpectingOperator`）を内部で更新しています。これにより `/`（除算/正規表現）、`%`（modulo/ハッシュシジル）、`^`（XOR/特殊変数）、`x`（繰返し/識別子）等の曖昧性解消を lexer 側が担っており、責務が肥大化しています。

一方、parser からは「次は演算子が欲しい」「次は値が欲しい」といった意図が明確にあり、`biome_parser` のような `bump_with_context` 的な操作で曖昧性を外部指定したいニーズがあります。さらに、`parser.bump()` が次トークンを即座に読み進める設計のため、「`bump_with_context()` のために current_token を再読込みする」問題も発生します。

このADRでは、曖昧性解消を parser 主導に移し、`bump()` の次トークン自動読込みを止める（遅延取得）方向へ段階的に移行する設計を定めます。初期実装ではキャッシュを持たない MVP とし、単純さと安全性を優先します。

## Decision

- 新しい Lex 期待値（Lexical Goal）を導入する:
  - `enum LexExpectation { Value, Operator }`
  - これを lexer の曖昧性解決入力として使う。
- Lexer に期待値付き API を追加する（peek と next を用意）:
  - `next_token_with(expect: LexExpectation) -> Option<(SyntaxKind, &str)>`
  - `peek_non_trivia_with(expect: LexExpectation) -> Option<(SyntaxKind, &str)>`
  - 既存の特殊モード（`QuoteLike`, `SubPrototype`, `RawData`, `VariableName`）は従来通り lexer 側で扱い、`Value/Operator` 期待は関与しない。
- Parser は期待値を明示して先読み・消費する:
  - `at_with(expect, kind)` は `peek_non_trivia_with(expect)` ベースで判定。
  - `bump_with(expect)` は `next_token_with(expect)` で消費（この時点で初めて読み進める）。
  - 利便性のためショートハンド `at_value/at_op/bump_value/bump_op` を用意。
- current_token を段階的に廃止する:
  - `at()` は `peek_*_with` ベースで、その場で判定。
  - `bump()` は「即次を読む」挙動をやめ、呼ばれた時点でのみ読み進める（遅延取得）。
- 初期は cached_token（先読みキャッシュ）を持たない:
  - 実装を簡潔に保ち、整合性問題（異なる期待値での先読み混在）を避ける。
  - 必要になれば後から軽量キャッシュを追加可能。

## Rationale

- 責務分離: 演算子/値の文脈は構文上の判断であり parser の責務。lexer は「テキスト→トークン」へ専念し、曖昧性は外部の期待値で最小限補助する。
- 遅延取得: `bump()` が次を即座に読むと、コンテキスト変更時に再lexが必要になり、複雑化とバグ温床になる。遅延モデルで `peek` と `bump` を分離すればシンプル。
- MVP の単純性: キャッシュ無しで正しさを満たし、後から性能や堅牢性のためにキャッシュを導入できる。

## Expected Parser Rules（期待値の付け方）

- 左辺・リテラル・識別子・右括弧・後置演算子の直後 → 次は `Operator`。
- 演算子直後・式の先頭・`(` の直後・`;` の直後・キーワード直後（引数が値の文脈） → 次は `Value`。
- ビルトイン関数名の直後 → 次は `Value`（`is_builtin_function` を共有化して parser からも参照）。
- 特殊モード（QuoteLike / SubPrototype / RawData / VariableName）では、期待値は使わず専用処理を維持。

## API Sketch

```rust
// lexer側（新規）
pub enum LexExpectation { Value, Operator }

impl<'a> Lexer<'a> {
    pub fn next_token_with(&mut self, expect: LexExpectation)
        -> Option<(SyntaxKind, &'a str)> { /* disambiguate using expect */ }

    pub fn peek_non_trivia_with(&self, expect: LexExpectation)
        -> Option<(SyntaxKind, &'a str)> { self.clone() /* safe lookahead */.find(|(k, _)| !k.is_trivia()) }

    // 便利版（Valueをデフォルト期待にする）
    pub fn next_token_default(&mut self) -> Option<(SyntaxKind, &'a str)> {
        self.next_token_with(LexExpectation::Value)
    }
    pub fn peek_non_trivia(&self) -> Option<(SyntaxKind, &'a str)> {
        self.peek_non_trivia_with(LexExpectation::Value)
    }
}

// parser側（新規）
impl<'a> Parser<'a> {
    fn at_with(&self, expect: LexExpectation, kind: SyntaxKind) -> bool {
        self.lexer.peek_non_trivia_with(expect).is_some_and(|(k, _)| k == kind)
    }
    fn bump_with(&mut self, expect: LexExpectation) {
        if let Some((k, t)) = self.lexer.next_token_with(expect) {
            self.builder.token(k.into(), t);
            self.current_pos += t.len();
        }
    }
    // Helpers（Valueがデフォルトの便利メソッド）
    fn at(&self, k: SyntaxKind) -> bool { self.at_with(LexExpectation::Value, k) }
    fn bump(&mut self) { self.bump_with(LexExpectation::Value) }

    // Operator文脈を明示するメソッド
    fn at_value(&self, k: SyntaxKind) -> bool { self.at_with(LexExpectation::Value, k) }
    fn at_op(&self, k: SyntaxKind) -> bool { self.at_with(LexExpectation::Operator, k) }
    fn bump_value(&mut self) { self.bump_with(LexExpectation::Value) }
    fn bump_op(&mut self) { self.bump_with(LexExpectation::Operator) }
}
```

Pratt ループ例（抜粋）:

```rust
// LHS を値期待で読む
if !self.parse_primary_with_postfix() { return false; }

loop {
    // ここでは演算子を期待
    let Some(op_kind) = self.lexer.peek_non_trivia_with(LexExpectation::Operator).map(|(k, _)| k) else { break };
    let Some(op_info) = get_operator_info(op_kind) else { break };
    // precedence/associativity 判定...

    // 演算子トークンを消費（次は値を期待）
    self.bump_with(LexExpectation::Operator);
    self.skip_trivia();

    if !self.parse_expression_with_precedence(op_info.next_min_precedence()) {
        self.error("Expected rhs expression after operator");
    }
}
```

## Migration Plan（段階移行）

1. Lexerに `LexExpectation` と `*_with` API を追加（既存 `next_token` は現状維持）。
2. Parserに `at_with/bump_with` とショートハンドを追加。
3. 式パーサのホットパス（`/`, `%`, `^`, `x` の曖昧性が出る箇所）から新APIへ置換。
4. 式パーサ全体→文→root と段階的に置換。
5. lexer の `ExpectingValue/ExpectingOperator` の自動更新を撤廃（特殊モードは存続）。
6. `current_token` を廃止。`at()` は `peek_*_with` ベースへ、`bump()` は `bump_with` に一本化。
7. 後続最適化（必要なら）：軽量キャッシュ導入、デバッグガード追加。

## Alternatives Considered

- 代替A: これまで通り lexer が文脈を内部管理。
  - 責務過多・複雑化・parser からの制御が困難。
- 代替B: 先読みキャッシュ（`cached_token`）を初回から導入。
  - 正当だが、期待値不一致や無効化の扱いが初期から必要になり、MVPの複雑度が上がる。
- 代替C: 事前に全トークン化（トークンストリーム構築）。
  - 文脈依存の曖昧性解消上、結局期待値や再判定が必要。メモリ/実装コストが高い。

## Risks & Mitigations

- 先読みと消費で異なる期待値を指定するミス:
  - 規約で回避。必要なら開発ビルドで `debug_assert!` により検知を追加。
- パフォーマンス低下（`peek` と `bump` で二度 lex する箇所）:
  - 初期は許容。必要に応じて軽量キャッシュを追加。
- QuoteLike/Prototype/RawData との相互作用:
  - 当面は現行の lexer 専用モードを維持し、移行は別ADRで検討。

## Testing Strategy

- 単体テスト（lexer・parser）:
  - `/`, `%`, `^`, `x` の曖昧性ケース。
  - ビルトイン関数直後に regex/q 系が来るパターン（`split /re/`, `print q(...)`）。
  - Postfix deref/`->` 周辺の回帰。
- スナップショット（formatter）:
  - 既存一式を回して差分確認。
- デバッグガード（任意）:
  - 期待値混在を `debug_assert!` で検知（将来導入）。

## Open Questions

- QuoteLike を parser 主導にするタイミングとAPI（`begin_quote_like(mode)` のような形）の設計。
- `is_builtin_function` の配置（lexer→共有モジュールへ抽出、parser からも参照）。
- `lookahead_for_any` 等の補助関数を期待値付きにどう整理するか。

## Consequences

- 曖昧性解消の責務が parser に移ることで、lexer が単純化し拡張性が増す。
- 遅延取得により `bump()` 時の不意な先読みが消え、`bump_with_context()` 相当の操作が自然に書ける。
- 初期は性能の劣化があり得るが、後から局所的なキャッシュで解消可能。

***

本ADRは「parser 主導の期待値指定＋遅延lex（ノーキャッシュ）」を安全に導入するための最小スコープを定義します。まずは式パーサから段階導入し、回帰を抑えつつ `current_token` 廃止まで到達することを目標とします。
