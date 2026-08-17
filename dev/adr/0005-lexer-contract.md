# ADR 0005: Lexer 契約 — 単一 expect 状態・トークンバッファ・原子的シーケンス

- Status: Proposed
- Date: 2026-07-28
- Owners: camello core
- 参照: notes/2026-07-28-redesign-assessment.md §2.1, §4.1, 付録A (D1-D7)。ADR 0001 を置き換える。

## Context

現行 lexer の構造的問題（詳細は再設計評価ノート）:

1. `LexContext` が約73箇所の呼び出しサイトごとの引数であり、`bump()` ≡ `bump_value()`、`at()` の暗黙 Value 固定、第3文脈 `AmbiguousValueLookahead` とその補正テーブルという迷宮を生んでいる。
2. 先読みが lexer クローンで実装されており、parser 起因のモード変更（`begin_quote_like`）を構造的に見られない。
3. 未終端構文で `None` を返してモードが復帰せず、以降のファイル解釈が壊れる。
4. logos の regex 不備を後から手術する箇所が2つあり、トークンの半分近くは手書きスキャナが処理している。

## Decision

### 1. logos を廃止し、手書きスキャナにする

logos が実際に担っているのは単純トークンのみで、quote-like・heredoc・POD・regex・数値の dot 分割・`x5` 分割はすべて手書きコードが処理している。手書きスキャナに一本化することで、後付けのトークン手術（`mod.rs:532-576`, `:581-607`）が消え、行・桁の追跡とエラートークン発行が自然に書ける。キーワード認識は ADR 0004 の生成 lookup を使う。

### 2. expect は lexer が所有する単一状態にする

```rust
pub enum Expect { Term, Operator }

impl Lexer {
    pub fn set_expect(&mut self, e: Expect);   // 変更時、カーソル以降のバッファを無効化
    pub fn peek(&mut self, n: usize) -> &LexedToken;  // 現在の expect で lex
    pub fn bump(&mut self) -> LexedToken;
}
```

- perl 本体の `PL_expect` と同型。**文脈は呼び出しごとの引数ではなく lexer の状態**であり、parser は構文上の判断点（primary を読み終えた、演算子を消費した、`(` に入った等）で `set_expect` を呼ぶ。
- peek / bump は expect を引数に取らない。**「peek したときと consume するときで文脈が違う」ことが API 上表現不能になる。** debug ビルドでは `bump` 時に「そのトークンを lex した時点の expect == 現在の expect」を assert する（ADR 0001 が提案して未実装だったガード）。
- `AmbiguousValueLookahead` は廃止する。従来これが担っていた「副作用なしの先読み」は、§3 の原子的シーケンスにより不要になる（先読みしても lexer 状態は変わらない。バッファに積まれるだけ）。
- expect の自動更新は行わない（lexer は parser の指示に従うのみ）。`foo / 2` の除算/regex 判定は「bareword の後に parser が expect をどちらに設定するか」という parser 側ポリシーに一元化される（組み込み関数テーブル ADR 0007 §6 が入力になる）。

### 3. 先読みはクローンではなくトークンバッファで行う

```rust
struct LexedToken { kind: TokenKind, range: TextRange, expect_at_lex: Expect }
```

- lexer は `Vec<LexedToken>` + カーソルを持ち、`peek(n)` は必要分だけ前方を lex してバッファに積む。クローンは廃止。
- `set_expect` はカーソル以降のバッファを破棄して再 lex させる（無効化は位置ベースで安価。現行のような「文脈切り替えごとに全キャッシュ破棄 + スナップショット再構築」は起きない）。
- **原子的シーケンス**: quote-like 一式（キーワード・デリミタ・内容・フラグ）、heredoc（マーカー、および行頭到達時の本体+終端）、POD、`__DATA__`、prototype、attribute 引数は、**1回の lex 呼び出しでトークン列としてまとめてバッファに積む**。内部のモード遷移は呼び出し内で完結し、呼び出し後に観測可能なモード状態は残らない。
  - これにより D2（未終端 quote-like でモードが残り以降が壊れる）と D3（先読みが quote-like モードを見られず虚構のトークン列を返す）が**構造的に**消える。`#` デリミタだけの ad-hoc パッチ（`lookahead.rs:96-106`）も不要になる。
  - parser の `begin_quote_like` API は廃止。quote-like の開始判定は lexer 内部で行う（§5）。

### 4. 失敗は沈黙しない

- 未終端構文は `UNTERMINATED_REGEX` / `UNTERMINATED_QUOTE_LIKE` / `UNTERMINATED_HEREDOC` トークンを発行し、内容全体（EOF まで）を1トークンで覆う。診断は1件。現行のような「`None` を返して除算にフォールバック」はしない。
- **Term 文脈の `/` は常に regex 開始としてコミットする**（perl 本体と同じ）。終端が見つからなければ上記エラートークン。これにより「900行後の `/` の有無で5行目の木が変わる」非局所性（D4）が消える。
- 不明文字は `ERROR_CHAR` トークン。lexer は `Option` を返すのは EOF のみ。

### 5. 個別の字句規則の決定

- **quote-like の bareword 例外**: Term 文脈で quote-like キーワード（`q qq qw qx m qr s tr y`）を認識したとき、直後（水平空白スキップ後）が `=>` または `}` なら IDENT として発行する（`(s => 1)` と `$h{q}` のため）。それ以外は quote-like としてコミットする。直前トークンが `->` の場合はメソッド名なので parser が expect=Operator にしているため自然に IDENT になる。
- **`q` とコメント**: デリミタ探索はキーワード直後から始めるが、**空白を挟んだ `#` はコメントとして扱う**（perl 準拠。現行 D5 の修正）。デリミタは「最初の非空白・非コメント文字」。
- **POD は桁0のみ**: 桁を lexer が追跡し、`=pod` 等は行頭（桁0）でのみ POD 開始とする（D1 の修正）。`at_line_start` フラグの文字列検査による維持は廃止。
- **heredoc**: マーカーは Term 文脈でのみ認識（現行同様）。本体は次の行頭で原子的シーケンスとして emit。EOF まで終端が無ければ `UNTERMINATED_HEREDOC`。
- **file test 演算子**: `-` + 実際の file test 文字集合（`efdlpSbcugktrwxoRWXOszAMC`）のみを `FILE_TEST_OP` とする（現行は任意の英字1文字）。
- **数値**: `0x7f..` の dot 問題はスキャナが最初から正しく切る（`0x7f` の直後で数値を閉じ、`..` を演算子として読む）。`"abc"x5` は Operator 文脈で `x` + `5` として読む（再分割ではなく最初からそう lex する）。
- **複合代入演算子**（`+=` `-=` `//=` `||=` `**=` 等）は単一トークン。
- **アポストロフィはパッケージ区切りではない**（2026-08-17 追記）: perl 5.42 は
  `'` を `::` の代わりに使う記法を `apostrophe_as_package_separator` フィーチャとして
  **無効化できる**ようにした（削除ではない）。camello はすべての入力を
  `no feature "apostrophe_as_package_separator"` の下にあるものとして読む。
  名前は `'` で終わる。両方は選べない曖昧性であり——`STDERR'text'` は
  「`STDERR::text` という名前」と「バレワードに続く文字列」のどちらとも読める——
  現代の Perl を対象にする（CLAUDE.md「Project Overview」）以上、
  後者を取る。結果として `Carp::Assert` の `sub shouldn't ($$)`
  （＝`sub shouldn::t`）は診断になる。

### 6. 位置情報

- 各トークンは `TextRange` を持つ。parser 側の手動 `current_pos` カウンタは廃止し、診断の span はトークンの range から取る。

## Consequences

- 3値の `LexContext`、`bump_value` 系 API 群、`adjust_ambiguous_next_kind_for_builtin`、lexer クローン先読み、`begin_quote_like`、parser 側の quote-like 状態機械ミラー（`quoted.rs:141-214`）がすべて不要になる。
- peek と consume の不一致、モードの取り残し、先読みの虚構トークン列が API 上表現不能になる。
- 検証済みバグ D1〜D5 が設計レベルで解消。D6（prototype 文字）は prototype を `RAW_CONTENT` として lexer が原子的に読むことで解消。D7（`foo %h` vs `foo % h`）は本質的曖昧性であり、「Term 文脈ではシジル優先」というルールを仕様として明文化する（空白による生文字検査は廃止し、expect のみで決める。挙動変更があれば fixture 更新で明示する）。
- 手書きスキャナ化により logos 依存が消える。トークン規則の網羅テスト（現行 lexer テストの移植 + D1〜D7 の回帰）を必須とする。
