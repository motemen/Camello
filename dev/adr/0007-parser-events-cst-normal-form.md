# ADR 0007: イベント式 parser と CST 正規形

- Status: Proposed
- Date: 2026-07-28
- Owners: camello core
- 参照: notes/2026-07-28-redesign-assessment.md §2.2, §4.2, 付録B。ADR 0003 のエラー回復方針を具体化し置き換える。

## Context

現行 parser は `GreenNodeBuilder` に直接書き込むため投機パースができず、あらゆる曖昧性を無制限先読みヒューリスティック（21個を確認、評価ノート付録B）で解決している。また CST の形状が場当たり的で（`STMT` ラップの不整合、`FUNCTION_CALL_EXPR` の6重多義、カンマがある時だけ生成される `EXPR_LIST`）、下流の formatter が形状の場合分けを強いられている。エラー回復は「1トークン消費」のみで、ADR 0003 の同期セットは実装されていない。

## Decision

### 1. イベントバッファ方式

```rust
enum Event {
    Start(NodeKind),
    StartAt(Checkpoint, NodeKind),   // 遡及ラップ（Pratt の左結合用）
    Token,                            // 次の非トリビアトークンを消費
    Finish,
    Error(Diagnostic),                // 木を変えない診断
}
```

- parser は `Vec<Event>` に記録し、終了後に再生器が `GreenNodeBuilder` へ変換する。トリビアの挿入と `TriviaMap` 構築は再生時に行う（ADR 0006）。
- **投機パース**: `let mark = p.mark();` … `p.rollback(mark);` でイベント列とlexerカーソルを巻き戻せる。lexer バッファ（ADR 0005）はカーソルを戻すだけなので巻き戻しは O(1)。
- 巻き戻しで置換するヒューリスティック（優先順）:
  - hash-ref vs block（現行: 先頭トークン4分類 + ブレース本体の `;` 全走査）→ hash-ref として試し、失敗したら block として再パース。
  - `<` I/O vs 比較（現行: lexer 丸ごとクローン + Pratt ループ打ち切りの欠陥）→ 比較として試し、失敗したら I/O。
  - signature vs prototype（現行: 手書きミニパーサ）→ signature として試す。
  - `try` 文 vs 関数 → 現行の二重チェックポイントを rollback で単純化。
- 有界の先読み（次トークンが `=>` か、等）はそのまま `peek` で行ってよい。禁止するのは**無制限走査**（`iter_non_trivia_from` 相当）で、これは全廃する。

### 2. CST 正規形

**文**:

- ROOT / BLOCK の子は閉じた文ノード集合のみ: `EXPR_STMT`, `VAR_DECL_STMT`, `IF_STMT`, `LOOP_STMT`(while/until/for/foreach), `SUB_DEF`, `PACKAGE_STMT`, `USE_STMT`, `NO_STMT`, `TRY_STMT`, `GIVEN_STMT`, `BLOCK_STMT`(裸ブロック), `LABELED_STMT`, `PHASE_BLOCK`, `POD`, `DATA_SECTION`, `ERROR`。
- 汎用 `STMT` ラッパーは廃止する。式文は必ず `EXPR_STMT` に包む。「包まれたり包まれなかったり」を無くす。

**式**:

- `BINARY_EXPR`（全2項演算子。演算子はトークンとして保持）/ `ASSIGN_EXPR`（`=` と複合代入。複合代入は単一トークン、ADR 0005 §5）/ `TERNARY_EXPR` / `PREFIX_EXPR` / `POSTFIX_EXPR`（`++`/`--`）。現行の「ほぼ何でも `INFIX_EXPR`」をやめ、formatter・将来の lint が演算子クラスで分岐できるようにする。
- カンマ列は常に `LIST_EXPR` に包む。**要素数 0/1 でもリスト位置なら必ず生成**し、「カンマがある時だけラッパーが現れる」二重形状を廃止する。

**呼び出し**（現行 `FUNCTION_CALL_EXPR` の6重多義を分割）:

- `CALL_EXPR`: `foo(...)`。子は name + `ARG_LIST`（括弧含む）。
- `LIST_CALL_EXPR`: 括弧なしのリスト演算子呼び出し（組み込み・ユーザー関数とも）。`print` 系のファイルハンドルは `FILEHANDLE` 子ノードとして明示する。`sort` の比較関数は第1子の位置で表現し、名前の文字列比較による専用パーサは廃止する。
- `METHOD_CALL_EXPR`: `->method` / `->method(...)`。
- `BLOCK_CALL_EXPR`: `map { } @xs` のような先頭ブロック引数付き呼び出し。ブロックは `BLOCK` 子ノード。

**エラー**:

- `ERROR` ノードは「回復のためにスキップしたトークン列」を包む場合のみ生成する。診断だけの `Event::Error` は木を変えない。

### 3. エラー回復

- `p.error(msg)` は**非消費**がデフォルト。消費が必要な場面は `p.error_and_bump(msg)` を明示的に使う。
- 同期セットによる panic-mode 回復を実装する: 文レベルは `{ ';', '}', 文開始キーワード }`、リスト内は `{ ',', ')', ']', '}' }` を追加。スキップしたトークンは1つの `ERROR` ノードに包む。
- 受け入れ基準（既存スナップショットの改善目標）: `errors/direct_subscription_after_call` は 6 → 2 エラー、`statements/errors/sub_signature_invalid` は 11 → 6 エラー、`errors/use_missing_semicolon` は「2つ目の `use` の黙殺」ではなく `use A` の後に `;` 欠落を指す1エラー。
- 診断は ADR 0004 の `Display` を使い、内部 enum 名を出さない。span はトークンの `TextRange` から取る。

### 4. 演算子優先順位

現行 `precedence.rs` のテーブル構造は維持しつつ、既知の誤りを perlop に合わせて修正する:

- ビット演算子（`& | ^`）を比較演算子より**上**（強く結合）に置く。
- named unary（file test 含む）の優先順位を perlop の named unary operators に合わせる（現行は `PREFIX` を流用しており `-f $x . "y"` の結合が誤り）。
- 未使用の `VAR_DECL` 定数を削除。

### 5. 重複実装の一本化

- block パース: 1実装（現行3）。
- 括弧付き引数リスト: 1実装（現行4）。
- 変数パース・変数宣言: それぞれ1実装（現行4・2）。`for my $x` も同じ `VAR_DECL` ビルダーを使い、宣言の内部構造を式文脈と一致させる。
- キーワード→識別子の強制（`sub if {}` 等）: 単一の `name()` ルーチン経由のみ（現行8箇所）。
- 末尾カンマ許容: リストパーサの単一オプション（現行4実装）。

### 6. 組み込み関数テーブル

- Perl の実 prototype（`prototype "CORE::xxx"` の出力相当、約200関数）から**ビルド時生成**した単一テーブルにする。現行の22エントリ手書きテーブル・`"sort"` / `"//"` の文字列比較特例・`can_start_expression` 内の分散知識を置き換える。
- テーブルの役割は2つ: (a) 引数パース形状（block / filehandle / 単項 / リスト / 0項）、(b) bareword 直後の expect 設定（ADR 0005 §2。例: `split` の後は Term なので `/,/` が regex になる）。
- ユーザー定義関数は従来どおり「宣言不明」として扱う（listop 仮定）。シンボルテーブルを持たない以上ここは近似のままであり、その旨を仕様に明記する。

### 7. heredoc の配置

- heredoc マーカー（`<<EOF`）は式の中のトークン。本体（`HEREDOC_CONTENT` + `HEREDOC_END`）は出現位置（行頭）で ADR 0006 §4 の配置規則に従い最小共通祖先位置に置き、マーカーと本体はトークン range で対応付ける。formatter は本体を Raw アトム（ADR 0008）として扱う。

## Consequences

- ヒューリスティック21個のうち、投機パース置換で約半分、lexer の expect 一元化（ADR 0005)で残りの大半が消える。残るのは本質的近似（組み込みテーブル、間接オブジェクト記法）のみで、それらは生成テーブルと明文化された規則になる。
- CST 形状が閉じた正規形になり、formatter の場合分けが減る。既存 parser スナップショットは大量に変わるため、移行時は「形状変更の差分」と「挙動変更の差分」を分けてレビューする。
- `GreenNodeBuilder` 直接呼び出し・`checkpoint` の生使用・手動 `current_pos` は parser から消える。
