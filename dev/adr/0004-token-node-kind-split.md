# ADR 0004: TokenKind / NodeKind の分離と単一ソース生成

- Status: Proposed
- Date: 2026-07-28
- Owners: camello core
- 参照: notes/2026-07-28-redesign-assessment.md §4.1, 付録D

## Context

現行の `SyntaxKind` は約300バリアントの flat enum にトークン種とノード種が混在しており、`builder.token(INFIX_EXPR, …)` のような誤用を型が止められない。また、キーワードの知識が4箇所（`contextual.rs:661-711` の keyword map、`macros.rs` の `T!`、`predicates.rs:24-81` の `is_keyword`、parser の `can_start_expression`）に分散しており、キーワード追加のたびに全箇所の同期が必要になっている。`Token` enum には logos が生成できない phantom バリアントが10個ある。

## Decision

### 1. 2つの enum に分離する

```rust
#[repr(u16)] pub enum TokenKind { /* 字句要素のみ */ }
#[repr(u16)] pub enum NodeKind  { /* 構文ノードのみ */ }
```

- rowan 向けの `SyntaxKind(u16)` は生成される単なる変換層とし、トークンは `0..TOKEN_COUNT`、ノードは `TOKEN_COUNT..` にマップする。`From<TokenKind>` / `From<NodeKind>` / `TryFrom<SyntaxKind>` を生成する。
- `GreenNodeBuilder` を直接触るのは event 再生器（ADR 0007）のみとし、その API は `token(TokenKind, …)` / `start_node(NodeKind)` に型付けする。誤用はコンパイルエラーになる。
- discriminant はマクロの記述順で採番する。rowan の木はプロセス間で永続化しないため、並び替えによる値変化は許容する（永続化を導入する場合は別 ADR で安定 ID を定める）。

### 2. 言語定義を単一マクロに集約する

```rust
define_language! {
    keywords  { "if" => IF_KW, "unless" => UNLESS_KW, /* … */ }
    punct     { "+" => PLUS, "=>" => FAT_COMMA, "+=" => PLUS_EQ, /* … */ }
    trivia    { WHITESPACE, NEWLINE, COMMENT }
    tokens    { IDENT, NUMBER, STRING, HEREDOC_CONTENT, POD_CONTENT,
                RAW_CONTENT, UNTERMINATED_REGEX, /* … */ }
    nodes     { EXPR_STMT, IF_STMT, BINARY_EXPR, /* … */ }
}
```

このマクロから以下をすべて生成する（手書きの重複を廃止）:

- `TokenKind` / `NodeKind` / `SyntaxKind` 変換
- `T![...]` マクロ（現行の `T!` / `__syntax_kind_token!` の二重定義・`[=cut]` 重複を解消）
- `TokenKind::is_keyword()` / `is_trivia()` / `is_punct()`（セクション由来で自動導出）
- キーワード文字列 → `TokenKind` の lookup（lexer が使用）
- `Display`（診断メッセージ用の人間可読名。`Expected R_BRACE, found None` のような `{:?}` 出力を廃止するため、各トークンに表示名を持たせる: `R_BRACE` → `` `}` ``）

### 3. 意味的述語は1つの手書きモジュールに残す

`can_start_expression` / `is_operator` のような構文知識を含む述語は自動導出できないため、`syntax_kind/predicates.rs` 相当の単一モジュールに手書きで残す。ただし対象は `TokenKind` に型付けされるため、現行の「`is_literal` にノード種 `IO_EXPR` が混入」のような誤りは表現不能になる。

### 4. 削除するもの

- `Token` enum の phantom バリアント10個（`PodCommand`, `BacktickString`, `RegexLiteral`, `DataSection`, `PostfixDeref*`）。新 lexer（ADR 0005）は logos を使わないため `Token` enum 自体が消える。
- dead バリアント: `SyntaxKind::EOF`, `POD_START`, `QUALIFIED_IDENT`（未生成）。
- 複合代入を表す `COMPOUND_ASSIGNMENT` ノード。`+=` `||=` `//=` 等は lexer が単一トークンとして発行する（ADR 0007 §2）。

### 5. エラートークン・rawトークンを一級市民にする

- `UNTERMINATED_REGEX` / `UNTERMINATED_QUOTE_LIKE` / `UNTERMINATED_HEREDOC` / `ERROR_CHAR` をトークン種として定義する。lexer は失敗を `None` で沈黙させない（ADR 0005 §4）。
- 「kind 付きの生テキスト範囲」を表す `RAW_CONTENT` 系トークンを定義し、現行の4つの脱出ハッチ（`consume_one_char_as_ident` 等）を置き換える。prototype 本体・attribute 引数・`__DATA__` 本体はこれで表現する。

## Consequences

- キーワード追加が1箇所の編集になる（現行4箇所）。
- token/node の取り違えバグがコンパイル時に消える。
- 診断メッセージが人間可読になる。
- typed AST 層（`ast::IfStmt` のようなアクセサ）は将来の lint/型検査で必要になるが、本 ADR のスコープ外とする。マクロの `nodes` セクションに子要素仕様を足せば生成可能な構造にはしておく。
