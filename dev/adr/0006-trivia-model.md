# ADR 0006: トリビアモデル — 帰属規則と木への配置規則

- Status: Proposed
- Date: 2026-07-28
- Owners: camello core
- 参照: notes/2026-07-28-redesign-assessment.md §2.2（trivia 整合不変条件）, §2.3（コメント出力2系統）

## Context

現行実装ではトリビア（空白・改行・コメント）の扱いが3つの問題を抱える:

1. parser がトリビアを手動でスキップ/消費するため、「primary は非トリビアトークン上で終わる」という未文書の不変条件が生まれ、破れると黙って間違った木ができる。`hash_ref` はスペースのみスキップ・`array_ref` は改行もスキップという非対称もある。
2. トリビアが「そのとき開いていたノード」に入るため、ノードの TextRange が先頭/末尾のトリビアを含むかどうかが場所により異なり、formatter の判定（`node_spans_multiple_lines` 等）が不安定になる。
3. `TriviaTable`（`comments/mod.rs`）は判定オラクルとしてだけ使われ、コメントの実出力は2系統の別コードが行っている。また木の全再走査で構築されるため性能問題がある（issue #266）。

## Decision

### 1. トークン種

- `WHITESPACE`: 水平空白のみ（`\n` を含まない）。
- `NEWLINE`: `\r?\n` ちょうど1つ。連続する空行は連続する NEWLINE トークンになる。
- `COMMENT`: `#` から行末の直前まで（NEWLINE を含まない）。

現行の「改行を含む WHITESPACE」は廃止する。空行の情報が lexer レベルで保存される（notes/2025-08-26 のアプローチ2を採用することに相当）。

### 2. parser はトリビアを一切見ない

- parser のイベントストリーム（ADR 0007）は非トリビアトークンのみを扱う。`skip_trivia` / `skip_whitespace_and_newlines` 系の呼び出しは全廃する。
- トリビアの木への挿入は event 再生時の**トリビア付与パス**が一元的に行う。

これにより「primary は非トリビアトークン上で終わる」不変条件と、その破れによるサイレント誤パースのクラスが消滅する。

### 3. 帰属規則（ownership）

非トリビアトークン A と B の間にあるトリビア列は、**最初の NEWLINE で分割**する:

- 最初の NEWLINE まで（NEWLINE 自身を含む）→ **A の trailing**。
- それ以降 → **B の leading**。

つまり「A と同じ行にあるコメントは A に付く。それ以外（独立行コメント・空行）は次のトークンに付く」。現行 `comments/mod.rs` の規則を踏襲する（このモジュールの設計は健全であり、規則ごと引き継ぐ）。

ファイル末尾のトリビアは EOF に leading として付く。

### 4. 木への配置規則（placement）

トリビアトークンは **A で終わるすべてのノードの Finish の後、B で始まるすべてのノードの Start の前**（= A と B の最小共通祖先の位置）に置く。

- 帰結: **すべてのノードの TextRange は先頭にも末尾にもトリビアを含まない**。「ノードの range = そのコードの range」が全ノードで成立し、`node_spans_multiple_lines` 等の判定が正確・一様になる。
- ブロックの中身は `L_BRACE, NEWLINE, <stmt>, NEWLINE, COMMENT, NEWLINE, <stmt>, …, R_BRACE` のような形になり、文ノード自体はトリビアを縁に持たない。
- ノード内部（トークン間）のトリビアも同じ規則で、そのノード内の最小共通祖先位置に置く。

### 5. TriviaMap を再生時に構築し、formatter の単一の情報源にする

- event 再生パスがトリビアを木に挿入すると同時に `TriviaMap`（非トリビアトークン → leading/trailing トリビア列）を構築する。木の再走査は行わない（issue #266 の解消）。
- formatter はコメント・空行の**判定も出力も** TriviaMap 経由で行う。コメント出力パスは1つになり、「片方はハードコード4スペース・片方はソースの空白数コピー」という現行の分岐（`mod.rs:968` vs `delimited.rs:449-450`)は消える。コメント前スペースの規則は formatter 側の1箇所で定義する（ADR 0008 §5）。

### 6. 空行の意味論

- CST は空行を忠実に保存する（連続 NEWLINE）。正規化（連続空行→1行、formatting.md BLANK_LINE-3）は formatter のポリシーであり、CST では行わない。
- `sub` / `package` / phase block 前後の自動空行挿入（BLANK_LINE-1）も formatter のポリシー。判定は TriviaMap の leading NEWLINE 数を見る（現行のような writer 状態とソース再走査の二重チェックはしない）。

## Consequences

- parser からトリビア処理が完全に消え、コード量と不変条件が減る。
- ノード range が正確になり、formatter の「ソースに改行があるか」判定（ADR 0008 の seed 計算）が厳密に定義できる。
- lossless性は維持される: 木の全トークン（トリビア含む）を連結すると元のソースに一致する。この性質は property test で常時検証する。
