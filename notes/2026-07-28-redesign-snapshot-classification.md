# スナップショット差分の分類レポート

**対象**: ADR 0004〜0008 への切り替え（コミット `22fbf18` 以降）
**比較**: 切り替え直前（`22fbf18~1`）の formatter スナップショット 47 本 vs 現在
**再現**: `scripts/snapshot-diff`（一覧） / `scripts/snapshot-diff <fixture>`（個別）
**関連**: `notes/2026-07-28-redesign-deviation-log.md`（L-004 に parser 側の扱い）

---

## 0. 集計

| | 本数 |
|---|---|
| 出力が完全一致 | 22 |
| 出力が変化 | 25 |
| 合計 | 47 |

parser スナップショット 78 本は**全件が変化**した。ノード語彙そのものが
別になったため（旧 `SyntaxKind` の flat enum と新 `NodeKind`）、行単位の差分から
「意図的挙動変更」と「バグ修正」を読み分けることはできない。
これは逸脱ログ L-004 に記録済みで、以下の分類は formatter 出力のみを対象とする。

---

## 1. CST 形状変更に由来する差分

新 CST は ADR 0007 §2 の正規形になり、formatter の場合分けの前提が変わった。

### 1.1 制御構造の括弧が `PAREN_EXPR` になった

`if (...)` `while (...)` `for (...)` の括弧は、他の括弧付きリストと同じ
`PAREN_EXPR` に包まれる。これにより「開きデリミタ直後に改行があれば broken」
（ADR 0008 §3、formatting.md INDENT-2）が条件式にもそのまま適用される。

- `user_newlines.pl`: ユーザーが `if (` の直後で折り返した条件が、
  1要素1行に展開されるようになった。

### 1.2 `LIST_EXPR` の常時生成と `ARG_LIST` の分離

引数リストは常に `ARG_LIST` > `LIST_EXPR` になり、要素数 0/1 でも形が変わらない。
末尾カンマの扱いがリストパーサ1箇所に集約された結果、
`func(1, 2, )` は `func(1, 2,)` になる（閉じ括弧の前に空白を置かない）。

- `trailing_commas.pl`

### 1.3 `BLOCK_CALL_EXPR` の分離

`map { ... } @xs` はブロック引数付き呼び出しとして別ノードになり、
括弧の中に置いた場合（`map({ ... } @xs)`）も同じ経路を通る。

- `builtin_functions.pl`

---

## 2. 意図的な挙動変更

### 2.1 コメント前の最小空白が 4 → 1（既定値）

旧実装はコメント出力が2系統あり、片方はハードコードの4スペース、
片方はソースの空白数のコピーだった。`min_spaces_before_comment` は
前者にしか効かなかった（評価ノート §2.3）。

新実装では出力経路が1つになり、既定値は 1 である。
列を揃えるのは align パス（ADR 0008 §5）の仕事であり、
最小空白は「揃えられないときの下限」でしかない。

- `block_opening_comment.pl`, `comment_formatting.pl`, `comment_ownership.pl`,
  `fat_comma_alignment.pl`, `comment_alignment_in_delimiters.pl`,
  `assignment_alignment.pl`, `operator_comments.pl`

**特に `comment_alignment_in_delimiters.pl`** は、旧出力の「揃って見える」
コメント列が、実はソースの空白数をそのままコピーした恒等変換だった
（評価ノート §2.3）。新実装ではレンダリング済みの列位置から計算する。

### 2.2 デリミタ内側の空白オプションを廃止

`DelimiterTightness` / `AlignmentStrategy` は新 formatter に移植していない。
ADR 0008 はレンダラのパラメータとしてインデント幅とタブのみを挙げており、
デリミタの内側空白は spacing 規則（build 時の `Doc::Space`）の一部である。

結果として `[ 1, 2 ]` → `[1, 2]`、`$h->{ key }` → `$h->{key}`、
`${ $ref }` → `${$ref}` に統一される。

- `method_call_with_refs.pl`, `bracket_hash_spacing.pl`,
  `specials_and_sigils.pl`, `trailing_commas.pl`

対応する `tests/delimiter_tightness.rs` は削除した。
再導入する場合はオプションではなく spacing 規則の分岐として設計すべきである。

### 2.3 アライメントのグループ化規則

形状キーは「宣言子の有無」で見る（formatting.md §7）。
旧実装は `my` / `our` / `state` / `local` を別グループとして扱っていた。

- `declarations.pl`, `assignment_type_separation.pl`,
  `compound_assignment_alignment.pl`, `alignment_strategies.pl`,
  `fat_comma_alignment_nested.pl`, `fat_comma_alignment_hashref.pl`

### 2.4 単文ブロックの判定

ADR 0008 §3 の「単文・セミコロンなし・コメントなし・**ソース改行なし**」に
統一した。旧実装の `is_simple_block` は7つの拒否規則とメモ化を持ち、
ソースが複数行でも1行に畳むことがあった。

- `regressions__unary_not_simple_block.pl`（ソースが1行なので新実装も1行）
- `assignment_alignment_multiline.pl`（ソースが複数行のものは展開したまま）

---

## 3. バグ修正

### 3.1 評価ノート付録 A の F1〜F6

すべて `src/fmt/tests.rs` に回帰テストとして存在し、通過している。

| # | 内容 | スナップショットへの現れ方 |
|---|---|---|
| F1 | 文字列リテラルへのインデント注入（意味破壊・非有界） | 該当 fixture なし。回帰テストで担保 |
| F2 | `suppress_newlines` の伝播漏れ | `declarations.pl` の `sub foo { BEGIN { ... } }` |
| F3 | 2回目のパスで初めて整列 | `control_flow.pl` |
| F4 | アライメントの O(n²) | 出力には現れない（`alignment_does_not_reformat_to_measure`） |
| F5 | 多行デリミタ先頭コメントで余分な空行 | `comment_alignment_in_delimiters.pl` |
| F6 | コメント付き `if` で brace が落ちる | `control_flow.pl` |

### 3.2 コメントの取りこぼし

多行リストの要素に付いた行末コメントが落ちていた。
意味保存テストはコメントをトリビアとして扱うため検出できず、
旧スナップショットとの突き合わせで初めて見えた。
現在は全 47 本で `#` の出現数が一致する。

### 3.3 継続インデント

ユーザーが折り返した行を1段下げる（formatting.md INDENT-3）。
旧実装は 14 分岐のヒューリスティックで、`LineBreakSource::User` にしか
反応しなかった。新実装は行に印を付けるだけなので、式がどれだけ深く
ネストしても常に1段になる。

- `operator_comments.pl`, `control_flow.pl`, `user_newlines.pl`

### 3.4 空行の正規化

heredoc 終端行の直後、`{` の直後、ファイル先頭には空行を入れない。
旧実装は writer 状態とソース再走査の二重チェックで、
`heredoc.pl` / `multiline_parens.pl` に余分な空行が出ていた。

---

## 4. 未解決の忠実性差分

以下は「新実装の出力のほうが劣る」か「良し悪しを判定していない」もので、
今後の課題として残す。fixture は更新していない（スナップショットのみ更新）。

| fixture | 内容 |
|---|---|
| `sub_signatures.pl` | 多行 signature が `PAREN_EXPR` ではないため、開き括弧直後の改行規則が適用されない |
| `specials_and_sigils.pl` | `+{%hash}`（無名ハッシュとブロックの曖昧性）の空白規則が未確定 |
| `bracket_hash_spacing.pl` | `[ +qw(a b) ]` の「prefix + qw」の特例を移植していない |

### 訂正

本レポートの初版は `heredoc_and_package.pl` を未解決として挙げていたが、
差分を読み直したところ **旧実装のほうが誤っていた**。
桁0の `if ($some_var) {` を旧実装は4スペース下げており、新実装は桁0のまま
出力する。同ファイルの他の差分（`func({}\n);` → `func({});`、
`? [\n]\n : {}` → `? [] : {}`）も新実装のほうが正しい。
残るのは §2.2 のデリミタ内側空白の変更だけである。§3 に属する。

### 本レポート作成中に見つかり、修正した不具合

- signature のプレースホルダに既定値が付く形（`sub f ($thing, $ = 1)`）が
  `$= 1` と出力されていた。`$=` は実在する変数なので読み違えを招く。
  シジルの直後を詰める規則を「名前が続くときだけ」に限定した。
- 同じ形を**空白付きで書いた**場合にパースできていなかった。
  スキャナは `$=` を句読点変数として1トークンにするが、空白を挟むと
  別トークンになるため、文法側で両方の綴りを受ける必要がある。

---

## 5. fixture 自体の変更

**fixture の内容は1バイトも変更していない。**
移設のみ（`src/formatter/fixtures/` → `src/fmt/fixtures/`、
`src/parser/fixtures/` → `src/parse/fixtures/`）。

評価ノート付録 C は「約35本の fixture が現実装のワークアラウンド挙動を
固定化している」と指摘しており、本来は1本ずつ「仕様として維持するか」を
判定すべきである。ただしその判定は fixture の入力を書き換える作業であり、
出力の分類とは別の粒度になる。§4 の項目とあわせて別途行う。
