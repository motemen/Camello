# 再設計移行 — 進捗と残作業

**対象**: `notes/2026-07-28-redesign-assessment.md` §5 の移行計画ステップ2〜5
**関連**: ADR 0004 / 0005 / 0006 / 0007 / 0008、`notes/2026-07-28-redesign-deviation-log.md`

この文書は「どこまで終わったか」ではなく「**切り替えに何が残っているか**」を
正確に書くためのものである。残作業はすべて `tests/invariants.rs` の
3つのレジストリに機械可読な形で入っており、この文書はその読み方を与える。

---

> **更新（切り替え完了後）**: 本文の §1〜§3 は移行途中の状況を書いたものである。
> 現在は切り替えが完了しており、旧スタックは削除されている。
> 最新の状況は §6 を参照。差分の分類は
> `notes/2026-07-28-redesign-snapshot-classification.md`。

## 1. 現在の構成

新旧が同一 crate 内で並走している。

| 層 | 旧 | 新 | 状態 |
|---|---|---|---|
| 語彙 | `src/syntax_kind/` | `src/lang/` | 新は ADR 0004 準拠で完成 |
| 字句 | `src/lexer/` | `src/lex/` | 新は ADR 0005 準拠で完成（D1〜D7 回帰通過） |
| 構文 | `src/parser/` | `src/parse/` | 新は ADR 0006/0007 準拠。文法カバレッジに欠落あり |
| 整形 | `src/formatter/` | `src/fmt/` | 新は ADR 0008 準拠（F1〜F6 回帰通過）。上の欠落の下流で一部未通過 |
| CLI | `src/cli.rs` | — | **未着手。旧スタックを呼んでいる** |

旧スタックは削除されていない。したがって移行計画ステップ5（切り替えと削除）は
未完了であり、`cargo test` が緑であることは「新旧どちらも自分のテストに
通っている」以上を意味しない。

---

## 2. 完了した項目

### 検証基盤（先行実装）

`tests/invariants.rs` が全 fixture 76 本に対して次を強制する。

- 冪等性 `format(format(x)) == format(x)`
- 意味保存（入出力を再パースし、トリビア以外のトークン列が一致）

旧スタックについては、既知の違反 1 件（F3: `control_flow.pl`）だけを
レジストリで許容する。新スタックについては §3 のレジストリを持つ。
どちらのレジストリも**エントリの追加を許さず**、「直ったのに登録されたまま」も
失敗にするため、単調に減る台帳として機能する。

### ADR 0004（TokenKind / NodeKind の分離）

`define_language!` 1回の呼び出しから TokenKind / NodeKind / SyntaxKind 変換 /
`T![]` / 述語 / キーワード lookup / Display を生成する。
綴りが文脈で二義になる `%` `*` `&` は `T![]` キーを持たない別セクションに分けた。

### ADR 0005（lexer 契約）

手書きスキャナ。expect は lexer が所有する単一状態で、peek/bump は文脈を
引数に取らない。先読みはトークンバッファ。quote-like / heredoc 本体 / POD /
`__DATA__` は原子的シーケンス。

評価ノート付録 A の **D1〜D7 をすべて回帰テスト化し通過**（`src/lex/tests.rs`）。

### ADR 0006 / 0007（トリビアモデル・イベント parser・CST 正規形）

イベントバッファ + 再生器。投機パースで hash-ref vs block、signature vs
prototype、C形式 for vs foreach を解決。トリビア付与は再生時の単一パスで、
ROOT 以外のすべてのノードの range が先頭にも末尾にもトリビアを含まない
（テストで全ノード検査）。

**ADR 0007 §3 の受け入れ基準を3件とも満たしている**（`src/parse/tests.rs`）:

- `direct_subscription_after_call` → 2 エラー
- `sub_signature_invalid` → 6 エラー
- `use_missing_semicolon` → 1 エラーで、2つ目の `use` が黙殺されない

### ADR 0008（Doc IR formatter）

build / render / align の3フェーズ。評価ノート付録 A の
**F1〜F6 をすべて回帰テスト化し通過**（`src/fmt/tests.rs`）。

---

## 3. 残作業

### 3.1 文法カバレッジの欠落（19 件）

`tests/invariants.rs` の `redesign::PARSE_GAPS` に列挙。各エントリに
原因のコメントが付いている。分類すると:

| 分類 | 件数 | 例 |
|---|---|---|
| quote-like の細部 | 3 | `q\hello\`（バックスラッシュ区切り）、`m[[\]]`（文字クラス内の区切り文字） |
| heredoc の配置 | 3 | 引数リスト内の heredoc、`print {$fh} <<EOF` |
| `<...>` の readline vs 比較 | 2 | 同一ファイル内に両方が出る |
| パッケージ/スタッシュ変数 | 3 | `%::`、`$::{name}`、`::diag()` |
| 組み込み関数の引数形状 | 3 | `keys(%h)`、`sort \&cmp @xs`、`map({...} @xs)` |
| 宣言の左辺 | 1 | `local Module->hash->{key}` |
| signature の細部 | 1 | `@rest` / `%opts` と属性の併用 |
| その他 | 3 | `0o10`、裸ブロックのループ、`grep +$_, @list` |

いずれも**文法の欠落であって、fixture の意味についての見解の相違ではない**。
新設計の構造に起因する問題は1件もない。

### 3.2 formatter の未通過（冪等性 2 / 意味保存 4）

`redesign::IDEMPOTENCY_GAPS` / `redesign::SEMANTIC_GAPS`。
**すべて §3.1 の parse gap の下流**であり、formatter 単独の欠陥ではない。
パースが壊れた木を整形しているため、出力が再パースで別の木になる。

### 3.3 切り替え（移行計画ステップ5）— 未着手

1. `src/cli.rs` と `src/lib.rs` の公開 API を新スタックに向ける。
2. 旧 `src/lexer/` `src/parser/` `src/formatter/` `src/comments/`
   `src/syntax_kind/` を削除する。
3. 旧スタックのスナップショット約 200 本を再生成する。
4. `tests/delimiter_tightness.rs` と `tests/example/` を新 API に移す。
5. `FormatterOptions` の旧オプション（`AlignmentStrategy`、
   `DelimiterTightness` 等）を新 formatter に移植するか、廃止を明示する。
6. CLAUDE.md の虚偽記述（存在しない `Builder` API、実装されていない
   エラー回復戦略、dead な source mapping）を現実に合わせる。

### 3.4 スナップショット差分の分類レポート（完了条件6）— 未着手

§3.3 の 3 が終わるまで着手できない。なお分類の対象について、
移行計画の想定と現実が食い違う点を §4 に記す。

---

## 4. 移行計画と現実の食い違い

### 4.1 parser スナップショットは「差分」にならない

移行計画ステップ3は「既存 parser fixture で差分レビュー」、完了条件6は
差分を「CST 形状変更 / 意図的挙動変更 / バグ修正」に分類せよと言う。

しかし新 parser は**ノード語彙そのものが別**である（旧 `SyntaxKind` の
flat enum と新 `NodeKind`）。`STMT` ラッパーの廃止、`FUNCTION_CALL_EXPR` の
4分割、`LIST_EXPR` の常時生成により、78 本の parser スナップショットは
1つ残らず全面的に変わる。行単位の差分は取れるが、読める情報を持たない。

したがって分類が意味を持つのは **formatter の出力スナップショット**に限られる。
parser 側は「全件が CST 形状変更」という 1 行の事実に潰れる。
これは逸脱ログ L-004 として記録した。

### 4.2 「バグ修正」は分類ではなく列挙になる

formatter 出力の差分のうち「バグ修正」に当たるものは、F1〜F6 と
open issue 群（#338, #339, #341, #342, #344, #345, #347, #368）に対応する。
これらは fixture の入力そのものが旧実装のワークアラウンドを前提に
書かれている場合があり（評価ノート付録 C が言う約35本）、
「出力の差分」ではなく「入力を書き直すべきか」の判断になる。
分類レポートはこの判断を1本ずつ記録する形になる。

---

## 5. 構造的な問題は見つかっていない

念のため明記する。ここまでの実装で、ADR 0004〜0008 の設計が
成り立たないと判断される事象は**1件も出ていない**。

- ADR 0005 の「expect 単一状態」は、名前を読む位置（`sub tr {}` /
  `sub x100 {}`）だけ2状態で表現できず、lexer に `take_name` を足した。
  これは設計の破綻ではなく、ADR 0004 §5 が言う「kind 付き raw span
  トークン」の一種であり、強制は `name()` の1箇所に集約されている。
- ADR 0007 §4 のビット演算子の記述は perlop と矛盾するが、
  同じ文の目的（perlop 準拠）を採ることで解決した（逸脱ログ L-003）。

残っているのは**分量**であって、設計上の障害ではない。


---

## 6. 切り替え完了後の状況（追記）

移行計画のステップ2〜5はすべて実施済みである。

### 完了

| 項目 | 状態 |
|---|---|
| 検証基盤の先行実装 | `tests/invariants.rs`。既知違反レジストリは空になったので削除 |
| ADR 0004 / 0005（D1〜D7） | 実装・通過 |
| ADR 0006 / 0007（§3 の受け入れ基準3件） | 実装・充足 |
| ADR 0008（F1〜F6） | 実装・通過 |
| 旧実装の削除 | `src/lexer/` `src/parser/` `src/formatter/` `src/comments/` `src/syntax_kind/` を削除。14,502 行 → 8,598 行 |
| スナップショット差分の分類 | `notes/2026-07-28-redesign-snapshot-classification.md` |
| 逸脱ログ | L-001 〜 L-008 |
| CLAUDE.md の現実化 | 完了 |
| fixture の監査（評価ノート付録 C） | 完了。§6.2 参照 |

全 fixture 76 本が、診断なしでパースされ、可逆で、冪等で、意味を保存する。
さらに全 76 本が perl の通る Perl であり、整形の前後で `B::Deparse` の出力が
一致する。

### 6.1 検証の三層

不変条件は互いに直交しており、どれか1つでは足りない。

| 見つかるもの | `tests/invariants.rs` | `scripts/snapshot-diff` | `scripts/perl-check` |
|---|---|---|---|
| トークン列が変わる改変 | ○ | ○ | ○ |
| 字句単位の内部に空白が入る | **×** | △ | **○** |
| perl が拒否する出力 | × | △ | **○** |
| 演算子の結合が変わる | × | △ | **○** |
| コメント落ち・見た目の劣化 | × | **○** | × |
| fixture 自体が非 Perl | × | × | **○** |

`tests/invariants.rs` がトリビア以外のトークン列を比較する以上、
**1つの字句単位が複数トークンに割れている箇所では原理的に盲目**である。
`${^MATCH}` と `${^ MATCH}` はトークン列が同一で、変数としては別物だった。
この盲点は実際に不具合を通しており、`scripts/perl-check` を足して塞いだ。

- `scripts/snapshot-diff` — 切り替え直前（`22fbf18~1`）の formatter
  スナップショットと現在の出力を突き合わせる。現状 一致 20 / 差分 27。
- `scripts/perl-check` — perl 自身を正解とする。入力を perl が受け付ける
  ものだけを対象に、出力も受け付けるか、`B::Deparse` の出力が一致するかを
  見る。現状 検査 76 / 非 Perl 0 / 破壊 0 / 意味変化 0。

  方言を2つ試すのは、`signatures` を有効にすると `sub f (&\%)` が不正な
  signature になり、無効なら正しい prototype になるため。
  1つの方言では fixture 全体を判定できない。これは camello が
  （シンボル表を持たない以上）直面している曖昧性そのものである。

### 6.2 fixture の監査（評価ノート付録 C）

perl を通すことで機械的に片付いた。当初 15 本が perl に拒否されていたが、
内訳は半々だった。

**オラクル側の前提不足**（fixture は正しかった）: `use Foo` の解決、
属性ハンドラ、Try::Tiny の export、未宣言サブルーチンの予告宣言、方言の選択。
`scripts/perl-lib/CamelloOracle.pm` が供給する。

**本当に非 Perl だったもの**（fixture を修正した）: `my *glob`、
lexical への `local`、`state (...) = ...`、`undef($a,$b,$c)`、
`($x + $y)++`、`$*`（5.30 で廃止）、2要素のドット付きバージョン、
`sort \&custom LIST`、slurpy が2つある signature `($,@,%)`、
核の try/catch と Try::Tiny 式形式の同居。

最後の2つは単なる誤記ではない。`($,@,%)` は signature として不正なので
perl は prototype として読み、それを signature とみなして整形すると
**prototype の文字列を書き換える＝意味を変える**ことになっていた。

### 6.3 残っている課題

1. **忠実性の差分 4 件**（分類レポート §4）。多行 signature の改行、
   `+{%hash}` の空白規則、`[ +qw(a b) ]` の特例、
   BLANK_LINE-1 の `package` / `use` グループ前後。
2. **`DelimiterTightness` / `AlignmentStrategy`**（逸脱ログ L-006）。
   オプションとしてではなく spacing 規則の分岐として再導入するかの判断。
3. **実世界コーパスへの `scripts/perl-check` 適用**（評価ノート §4.4 の 4）。
   fixture 76 本はこちらが書いたものにすぎず、実世界の Perl のほうが
   必ず意地悪である。`scripts/deperl` の対象が手近な候補。
4. **max-width 自動折り返し**（formatting.md FUTURE-1、ADR 0008 §7）。
   Doc IR がそろったので「flat の測定幅が上限を超えたら broken に倒す」
   1パスを render の前に挟むだけで載る。
