# 再設計実装のレビュー結果 — 実コーパス検証と敵対的レビュー

**日付**: 2026-07-28
**対象**: ADR 0004〜0008 実装（`c60b232`〜`7651307`）
**方法**: (a) 完了条件・逸脱ログ・分類レポートの検証、(b) src/lex・src/parse・src/fmt の敵対的コードレビュー、(c) システムの実Perlコーパス（`@INC` 配下 .pm、サンプル318本＋全759本）に対する format / 冪等性 / `perl -c` / `B::Deparse` 検証。
**結論**: アーキテクチャと fixture ベースの受け入れ基準は本物だが、**ADR 0008 §6 が要求していた実コーパス CI ゲートだけが未実装で、実害はすべてそこに集中していた**。以下の P0 を塞ぎ、コーパスゲートを常設するまで実世界のコードに `-w` で適用してはならない。

## 0. コーパス統計

- 318本サンプル: 診断なしパース 84.0%、panic **0**、**冪等性破壊 7.9%**、**`perl -c` 退行 7.5%**
- 759本全件: 冪等性破壊 55本（7.2%）。診断なしでパースしたのに整形後にコンパイル不能になるもの 6本（`Cwd.pm`, `POSIX.pm`, `File/Spec/Unix.pm`, `HTML/FormatText.pm`, `URI/data.pm` ほか）
- **注意**: `perl -c` は被害を過小評価する。下記 P0-1/P0-2/P0-3 は「コンパイルは通るが意味が変わる」出力を日常的に生む。

---

## 1. P0 — コードを破壊する（修正するまで実運用不可）

### P0-1: flat グループ内の行末コメントが後続コードをコメントアウトする

両レビューが独立に検出した最重要バグ。`delimited()` の broken 判定（`src/fmt/build.rs:561-563`）は開きデリミタ直後の改行しか見ず、`newline_follows`（`build.rs:667-679`）は COMMENT を「改行ではない」と扱う。`Placement::Trailing`（`src/fmt/render.rs:153-168`）はコメントの後に改行を強制しないため、flat グループでは後続トークンがコメント行に連結される。

```
$ printf 'my %h = ( # c\n  a => 1,\n);\n' | camello format
my %h = ( # ca => 1,);        # キー a が消滅。しかも区切り空白なしで連結
```

実被害: `JSON/backportPP.pm`（ハッシュ3エントリ消滅・perl -c は通る）、`Pod/Checker.pm` / `_charnames.pm` / `Net/SMTP.pm` / `Net/Ping.pm` / `Cwd.pm` / `POSIX.pm`（構文エラー化）。

**修正方針**: 「flat グループは hard break を含まない」という構造保証に**コメントを含める**。(a) `newline_follows` が COMMENT をスキップして改行を見る、かつ (b) `Comment(_, Trailing)` 自体を hard break として扱い、flat 判定時に本体内 COMMENT の存在で broken に倒す。連結時の区切り空白欠落も直すこと。

### P0-2: Raw 隣接への Space 注入 → パスごとに literal 内へ空白が増殖（F1 の別経路）

renderer は Raw に書き込めないが、**builder が Raw の隣に `Doc::Space` を置き、次のパスの再lexが literal 内に取り込む**。2つの発生経路:

1. 誤パースで quote-like が `ERROR` ノードに包まれると `is_quote_like_node`（`build.rs:333-335`）が発火せず、quote-like の `DELIMITER` は `T!["("]` ではないので tight 規則（`build.rs:344`）も外れ、`wants_space` が true に落ちる。
   - `use Exporter 5.57 qw( import );` → `qw (  import  )` → `qw (   import   )` …（バージョン付き use の引数で誤パース）
   - `HTTP/Config.pm` の `s/xx\z//;` → `s/xx \ z /  /;` → 置換文字列が毎パス成長（**意味破壊、perl -c は通る**）
2. `}qw(...)`（ブロック直後の qw）: `}` の後は Expect::Operator なので `qw` が IDENT に demote され ERROR 行き。`Dpkg/BuildOptions.pm` 等で再現。

**修正方針**: (a) `TokenKind::DELIMITER` を wants_space で常に tight にする。(b) より根本的には「Raw アトムを含むノードの子の間には `Doc::Space` を置かない」を builder の不変条件にする。(c) ERROR ノード内のトークン列は verbatim（ソースの空白を保存）で出力する — 誤パース時に「壊さない」ことが最後の砦になる。(d) 経路2の誤パース自体も直す（`}` の後の quote-like キーワードの扱い）。

### P0-3: ゼロ幅トークンへのトリビア重複付与 → s/// の置換文字列にコメントが混入

`TriviaMap::at` は開始オフセットの exact-match（`src/parse/trivia.rs:77-79`）で、空の置換文字列（`INTERPOLATED_STRING@10..10 ""`）と直後のデリミタが同一キーを共有する。builder は quote-like 内部のトークンにも無条件にコメントを付与する（`build.rs:371-392`）。

```
$ printf '$x =~ s/a//   # c\n  || 1;\n' | camello format
$x =~ s/a/ # c/ # c        # 「aを削除」が「aを ' # c' に置換」に変わる
```

実被害: `Math/BigInt.pm:5821`。**修正方針**: (a) TriviaMap のキーを一意化（ゼロ幅トークンには付与しない）、(b) quote-like / verbatim ノード内部のトークンにはトリビアを付与しない。

### P0-4: `eval` / `local` が組み込みテーブルに無く、直後の heredoc が破壊される

`builtins.rs::lookup` のミスで `bareword_call` が `expect_operator()` に落ち、`<<EOT` が SHIFT_LEFT + bareword になる。`eval <<EOT` → `eval << EOT`（perl≥5.28 で構文エラー）になり **heredoc 本体がコードに昇格する**。実被害: `URI/data.pm:69`。

あわせて: ADR 0007 §6 の「実 prototype からのビルド時生成（約200関数）」は実装されておらず、手書き約140エントリの match になっている（**逸脱ログ未記載**）。`wantarray` の Shape も誤り（nullary のはず）。**修正方針**: テーブルを ADR どおり生成に置き換えるか、最低限 `eval` `local` ほか欠落を補い、逸脱ログに L エントリを追加。

### P0-5: 1行に2つの heredoc があると2つ目の本体に前の終端行の `\n` が混入する

`find_heredoc_end`（`src/lex/atomic.rs:358`）が終端行の `\n` を終端トークンに含めないため、次の本体が `\n` から始まる。`foo(<<A, <<B)` で B の内容が `"\ntwo\n"` になる（perl 実行で差異を確認済み）。トークン列としては lossless なので invariants は盲目。**修正方針**: 終端トークンに `\n` を含める。

### P0-6: 非ASCII識別子が分断される（`use utf8` コードを壊す）

`ident_len_at` / `scan_word` が ASCII 限定で、`$café` → `$caf é`（構文エラー化）。**修正方針**: 識別子の継続文字を Unicode（XID_Continue 相当か、最低限「非ASCIIは識別子継続」）に広げる。

---

## 2. P1 — 堅牢性（panic / 爆発）

| # | 内容 | 根拠 | 修正方針 |
|---|---|---|---|
| P1-1 | `block_can_be_flat` が指数時間。`sub { `×20 のネスト（40文字）で36秒 | `src/fmt/build.rs:471-542`（子ブロックへの再帰×全走査、`node.text().to_string()` 割り当てつき） | メモ化（旧実装にはあった）。`text().to_string()` を廃止 |
| P1-2 | `STEP_LIMIT` が**release でも panic!**。開き括弧1100個で abort | `src/parse/mod.rs:126-134`。ネスト解消時の巻き戻りが O(深さ) の同位置検査になる | panic ではなく診断+ERROR ノードに。`bump_raw_parens` がカウンタをリセットしない件も併修 |
| P1-3 | formatter が深さ約3500でスタックオーバーフロー（parser は生存） | `fmt::build` / `render::walk` の再帰 | 明示スタック化か深さ上限で診断 |
| P1-4 | `Lexer::rollback` が expect 差異時にバッファを無効化せず、ADR 0005 の coherence assert が**発火する**（`foo{sub}` / `sub(@y^` / `t{,**t` で debug panic、release では黙って誤分類トークンを消費） | `src/lex/mod.rs:211-220` | `mark.expect != self.expect` なら `invalidate_from_cursor()` |
| P1-5 | Raw の行末空白が `finish_line` で trim される（I1違反）。`__DATA__` 末尾の空行も pop される（`while (<DATA>)` の行数が変わる） | `src/fmt/render.rs:228-241`, `:63-65`。`write_raw` が verbatim フラグを立てない | `Doc::Raw` にも `verbatim = true`。末尾空行 pop を verbatim 領域に適用しない |
| P1-6 | ブロック内の `__END__` がインデントされ、出力が同じ意味で再パースされない | ADR 0005 §5（桁0限定）と formatter 出力の不整合 | verbatim 系は常に桁0で出力 |
| P1-7 | `format STDOUT = ... .`（フォーマット宣言）が式としてパースされ、picture 行 `@<<<<` が `@< << <` に改変される。診断も出ない | 文法未対応 | 最低限「認識して verbatim 保存 or 診断を出す」。黙って壊すのが最悪 |

---

## 3. P2 — テスト・プロセスの穴（バグを通した原因）

1. **実コーパス CI ゲートの欠如**（ADR 0008 §6 の「+ 実コーパス」が未実装）。上記の実害はすべてこれで検出できた。`scripts/perl-check` を `@INC` コーパス（または vendored コーパス）に向ける形で常設すること。冪等性・`perl -c` 退行・意味変化の3指標。
2. **コメント保存が無検査**。invariants はトリビアを除外して比較するため、コメント落ち・コメント混入に構造的に盲目。「入力と出力で COMMENT トークン列（テキスト）が一致」を invariants に追加。
3. **Raw のバイト一致が無検査**（I1 は主張のみ）。「全 Raw トークンのテキストが入力に部分文字列として不変で存在」程度の直接検査を足す。
4. **I2（seed 安定性）が無検査**。fixture 冪等性の間接検証のみ。
5. `max_alignment_padding`（DoS 対策キャップ）のテストが0件。CLI から FormatterOptions を渡す配線も無い。
6. トリビア配置の property test が fixture 全体に及んでいない（`node_ranges_never_include_trivia` はハードコード1文字列のみ。ADR 0006 は property test を約束している）。
7. **逸脱ログ未記載の逸脱**: 手書き builtins テーブル（P0-4）、`take_caret_name` / `take_sigil`（2呼び出しサイト）/ `peek_raw_paren_body` の追加 lexer 脱出ハッチ、`balanced_paren_body_len` の無制限走査（doc コメントが「クォートをスキップする」と主張するが未実装 — `sub f("(") {}` で再現）。
8. **ドキュメント矛盾**: 分類レポート §5「fixture は1バイトも変更していない」は直し漏れ（その後のコミットで12本変更。L-009 が正）。

## 4. 良かった点（維持すべきもの）

- panic 0 / 非ゼロ終了 0（構築した深ネスト以外）で全1270モジュールを通過。align パスは設計どおり線形・ソース非依存・不動点。
- 原子的シーケンス（quote-like / heredoc / POD / DATA）はモード漏れなし。`s{a}{b}ge`、`tr###`、`<<~EOF`、`q #c#`、`$h{q}`、prototype 各種、`*glob{CODE}` 等の拷問ケースはすべて正しい。
- parser の全ループが前進保証を持ち、回復セットはカスケードを実際に抑止している。トリビア「ノード縁に無し」は構築により成立（コーパス742本で violation 0）。
- `take_name`（L-005）は宣言どおり1箇所に封じ込められている。逸脱ログと各レポートの誠実さは高い。

## 5. 対処順序の提案

1. P0-1（コメントによるコード破壊）— 今日壊れる
2. P0-2〜P0-3（Raw 隣接 Space / トリビア重複）— 意味破壊で perl -c にも見えない
3. P2-1〜P2-3（コーパスゲート・コメント保存・Raw バイト一致を**先に**テスト化してから修正に入る — 再発防止と検証を同時に得る）
4. P0-4〜P0-6、P1 群
5. P2-7〜P2-8（逸脱ログ・ドキュメント整合）
