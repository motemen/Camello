# Camello 再設計評価と新アーキテクチャ方針

**日付**: 2026-07-28
**目的**: ソースコード全体（約14,500行）の精読に基づき、現行アーキテクチャの構造的問題を診断し、再設計の方針を定める。
**根拠**: 本文中の `file:line` は執筆時点の HEAD (741e365) に対する参照。検証済みバグは `target/debug/camello` に対して再現確認済み。

---

## 1. 結論

**再設計は妥当であり、推奨される。** ただし「全部捨ててゼロから」ではなく、以下の資産を残して3層のアーキテクチャを入れ替える。

- 残すもの: `formatting.md`（仕様として高品質。新実装の受け入れ基準になる）、fixture + insta スナップショット体制、rowan / red-green tree、SyntaxKind の語彙、CLI 構成、そして **gofmt 的な改行保持ポリシーそのもの**。
- 入れ替えるもの: lexer の文脈受け渡しモデル、parser の CST 直接構築、formatter の直接文字列出力。

判断根拠は「バグの蓄積」ではなく「バグの生成構造」にある。991 コミット中 365 件が `fix:`、formatter 配下だけで全コミットの 39%（うち 100 件が `fix:`）。直近の履歴と open issue（#338, #339, #341, #342, #344, #345, #347, #368 など）はほぼすべて「アライメント × コメント × ネスト」の相互作用バグであり、これは後述する「IR 不在の直接出力」というアーキテクチャの署名である。点修正では収束しない。

---

## 2. 根本原因の診断

構造的な根本原因は 3 つ。派生する問題の大半はこの 3 つに帰着する。

### 2.1 根本原因1: `LexContext` が「呼び出しごとの引数」であること（lexer↔parser）

Perl の字句解析が構文文脈（値期待か演算子期待か）を必要とするのは本質的であり、`LexContext` の導入自体は正しい。問題はその所有権で、文脈が lexer の状態ではなく **約73箇所の呼び出しサイトが個別に指定する引数**になっている。

帰結:

- **`bump()` ≡ `bump_value()`**。`next_token()` は `Default::default()` = `LexContext::Value` に委譲する（`lexer/mod.rs:430-432`, `lexer/types.rs:8-20`）ため、両者はバイト単位で同一の操作。約40箇所の `bump_value()` は実質無意味で、doc コメント（`parser/mod.rs:219-222`）は実装と食い違っている。ADR-0001（parser 駆動 lexing）の移行が途中で止まった化石。
- **`at()` / `current_kind()` は中立に見えて Value 文脈固定**（`lookahead.rs:124-129`）。「副作用なしに覗きたい」ニーズから第3の文脈 `AmbiguousValueLookahead` が発明され、その過小認識を `adjust_ambiguous_next_kind_for_builtin`（`parser/expression/call.rs:609-659`）が組み込み関数ごとに巻き戻す二重補正になっている。同一位置を Value / Ambiguous の2文脈でlexし手で照合するコードもある（`call.rs:310-317`, `:350-357`）。
- **先読みが lexer クローンで実装されている**（`lookahead.rs:81-120`。トークン1つ先読みするごとに logos lexer + VecDeque×2 のクローン）ため、parser が起動する `begin_quote_like` によるモード変更を先読みは構造的に見られない。`#` デリミタだけ ad-hoc にパッチされ（`lookahead.rs:96-106`、`iter_non_trivia_from` にも重複 `:229-235`）、それ以外のデリミタでは虚構のトークン列を返す。
- **文脈切り替えのたびにキャッシュ全消去**（`lookahead.rs:70-72`, `:83-85`）。Pratt ループは Operator/Value の peek を交互に行うため、先読みは実質再lexの繰り返しになる（issue #206, #248 の背景）。
- **未終端 quote-like で lexer モードが復帰しない**。`try_consume_quote_like_string_content` は閉じデリミタ不在時に `None` を返すが `self.mode` は `QuoteLike` のまま（`quote.rs:343` 付近、エラーパスは `quoted.rs:16-20`, `:76-79`, `:168-172` の3箇所）。以降のファイル全体の解釈が壊れる（付録 A の D2）。

### 2.2 根本原因2: イベントバッファ不在の直接 CST 構築（parser）

parser は `GreenNodeBuilder` に直接書き込む。rowan に `abandon_node` がないため（`statement/mod.rs:181-188` に明示的な嘆きのコメントあり）、**投機的にパースして巻き戻すことができない**。

帰結:

- あらゆる曖昧性を「ノードを開く前の無制限先読み」で解決するしかない。精読で **21 個の独立したヒューリスティック**を確認（付録 B）。代表例: hash-ref vs block 判定のためのブレース本体全走査（`call.rs:150-177`）、`<` の I/O 判定のための **lexer 丸ごとクローンによる投機パース**（`expression/mod.rs:87-91`）、signature vs prototype 判定の手書きミニパーサ（`subroutine.rs:146-190`）。
- **未文書の危険な不変条件**:「primary は非トリビアトークン上で終わる」。`handle_binary_operator` は非トリビアを peek した後 `bump_op()` するが（`expression/mod.rs:124-126`, `:238`）、`bump_op()` はキャッシュ先頭（トリビアかもしれない）を pop する。不変条件が破れると**エラーなしで間違った木**ができる。パッチは `postfix.rs:97-100` の1箇所のみで、他の同型箇所は primary の末尾の `skip_whitespace_and_newlines()` に偶然依存している。`hash_ref` はスペースのみスキップ・`array_ref` は改行もスキップという非対称もある（`primary.rs:21` vs `:40`）。
- **`error()` がデフォルトでトークンを1つ消費する**（`parser/mod.rs:305-323`。非消費版の使用は 5 箇所のみ）。エラーがカスケードする: 2つのミスから6エラー（`errors/direct_subscription_after_call`）、6つのミスから11エラー（`statements/errors/sub_signature_invalid`）。逆に `use A use X;` では2つ目の `use` が ERROR トークンとして黙って飲み込まれ、エラー1件で残りが別の文として解釈される。
- **CLAUDE.md と ADR-0003 が謳う「セミコロン/ブレースへの同期回復」は実装に存在しない**。実在するのは「エラー→1トークン消費→再試行」のみ。
- CST 形状の不整合: `STMT` ラップの有無が文種別でまちまち、`FUNCTION_CALL_EXPR` が6種類の異なる由来を持つ、`EXPR_LIST` はカンマがある時だけ生成される、など（下流の formatter がこの不整合を吸収させられている）。
- ロジック重複: block parser 3実装、括弧引数 parser 4実装、変数パース4エントリポイント、複合代入検出3実装、キーワード→識別子強制8箇所。

### 2.3 根本原因3: ドキュメント IR のない直接文字列出力（formatter）

最大の問題。formatter は CST を歩きながら `Writer`（`Vec<Line>` + 文字列 append）へ直接出力する（`writer.rs:81-117`）。Wadler/Oppen 型の document IR、group/break/indent の代数、幅の概念は存在しない。

帰結:

- **一度出したものは取り消せない**ので、すべての判断を `prev_token_kind` / `at_line_start` 等のバックミラー状態で行う。宣言的 spacing テーブル（2層 + 31 アームの特例表, `spacing.rs:240-368`）を **34 箇所の `writer.write_token()` 直接呼び出しが素通り**する。node kind を token kind スロットに書き込む状態修理もある（`mod.rs:495-498`）。
- **幅の測定 = フォーマッタ全体の再実行**（`statement.rs:414-435`, `delimited.rs:577-584`）。等幅の代入が並ぶとアライメントグループ構築が毎行やり直しになり、**綺麗な O(n²)**: 100行 0.48s → 200行 1.88s → 400行 7.49s → 800行 29.7s（実測、debug build）。issue #266, #273 の根はここ。
- **「ソースに改行があるか」述語が7つに分散**（`mod.rs:570`, `:605`, `:639`, `:714`, `expression.rs:264`, `literal.rs:33`, `:58`）。継続インデントは 14 分岐のヒューリスティック（`mod.rs:812-900`）で、`LineBreakSource::User` にのみ反応するため、将来の自動折り返しには構造的に対応できない（ADR-0002 の想定と矛盾）。
- **コメント出力が2系統に分岐**: `format_token` の COMMENT アーム（ハードコード4スペース, `mod.rs:968`）と `format_expr_list_multiline`（**ソースの空白数をそのままコピー**, `delimited.rs:449-450`）。後者は「アライメント」に見える出力が実は fixture 作者が手で揃えた入力の恒等変換であることを意味する。`min_spaces_before_comment` オプションは片方にしか効かない。
- **アライメントが2つの完全に別個の実装**（文レベル `statement.rs:293-435` / 式リストレベル `delimited.rs:518-688`）で、どちらも「ソースの改行の存在」をグループ境界の必要条件にしている（`statement.rs:355-357`）。
- **冪等性が破壊されており、1件はセマンティクス破壊**（付録 A の F1〜F3）。特に F1（複数行文字列リテラルへのインデント注入・パスごとに増殖）は formatting.md POLICY-1（意味の保存）違反。
- **テストに冪等性検査・意味保存検査が存在しない**（`formatter/tests.rs` は fixture を1回通してスナップショット比較するのみ）。fixture の大半が整形済みコードなので不動点検査として機能していない。`--check` はバイト比較（`cli.rs:326-333`）なのでこれらをすべて見逃す。

---

## 3. 本質的な難しさ vs 偶発的複雑性

### 再設計しても消えないもの（本質）

1. **Perl は構文文脈なしに字句解析できない**: `/` `%` `*` `&` `<` `<<` `x` `-e` の曖昧性。perl 本体も `toke.c` の `PL_expect` で解決している。何らかの文脈チャネルは必須。
2. **quote-like 演算子**（任意デリミタ、対デリミタのネスト、`s{}{} `の2部形式）、**heredoc**（本体が次行から始まる遅延抽出、1行複数キュー）、**POD / `__DATA__`**（行頭依存）。
3. **`{}` の hash-ref vs block** は perl 自身もヒューリスティックで推測する（perlref に明記）。
4. **prototype と間接オブジェクト記法**は BEGIN を実行しない限り決定不能。組み込み関数テーブルによる近似は唯一の現実解（テーブルの規模は設計判断）。
5. **改行保持ポリシーを選んだこと自体のコスト**: 出力が入力レイアウトの関数になるため、正準形フォーマッタより冪等性の担保が本質的に難しい。アライメントも非合成的（ネストした `=>` の整列が外側のエントリ幅を変える）。
6. formatter はロスレスでなければならない（コメント・空白・verbatim 領域の保存）。

### 設計で消えるもの（偶発）

- §2 の根本原因3つと、その派生であるヒューリスティック群の大半。
- 知識の重複: 閉じデリミタ対応表2箇所（`lexer/quote.rs:279-287` / `parser/expression/quoted.rs:217-226`）、regex フラグ集合2箇所（`contextual.rs:174` / `quote.rs:411`）、キーワード表3〜4箇所（`contextual.rs:661-711` / `macros.rs` / `predicates.rs:24-81` / `can_start_expression`）、quote-like 状態機械の lexer 側と parser 側の二重エンコード。
- `SyntaxKind` がトークン種とノード種を1つの flat enum に混在させていること（`builder.token(INFIX_EXPR, …)` を型が止められない）。
- 生テキストへの4つの脱出ハッチ（`consume_one_char_as_ident` / `consume_digit_prefixed_ident` / `consume_balanced_parens` / `consume_data_section`）— トークン抽象が言語に合っていない証拠。
- parser 側の手動 `current_pos` バイトカウンタ（`Lexer::span()` と二重管理、`mod.rs:134-136` で既に desync）。

---

## 4. 新アーキテクチャ方針

### 4.1 Lexer: `expect` 状態を lexer が1つだけ所有する

- perl 本体の `toke.c` と同様、Value/Operator 期待を **lexer 内の単一の権威ある状態**にする。parser は明示的 API（例: `set_expect(Expect::Operator)`）でこれを更新する。呼び出しごとの引数は廃止。
  - これで peek/consume の不一致クラスが消え、`AmbiguousValueLookahead` と `adjust_ambiguous_next_kind_for_builtin` は丸ごと不要になる。
  - debug ビルドでは「peek 時と consume 時の expect が一致すること」を assert する（ADR-0001 が提案して未実装だったガード）。
- **先読みはクローンではなく再lex可能なトークンバッファ**で行う。モード変更（quote-like 開始など）はバッファの当該オフセット以降を明示的に無効化して再lexする。これで「先読みが parser 起因のモード変更を見られない」問題が構造的に消える。
- **未終端構文は沈黙しない**: `UNTERMINATED_REGEX` / `UNTERMINATED_QUOTE_LIKE` 等のエラートークンを発行し、モードを必ず `Normal` に復帰する（スコープガード/RAII で強制）。regex 終端探索は現在ファイル全体に及ぶ（改行越え可）が、これを有界にするかは要検討（非局所性: 900行目の編集が5行目のパースを変える）。
- **`TokenKind` / `NodeKind` を分離**し、rowan への変換はオフセットで行う。キーワード表・`T!` マクロ・`is_keyword` は単一ソースからマクロ生成する。
- logos の regex 不備を後から手術する2箇所（`0x7f..` の dot 分割 `mod.rs:532-576`、`x5` 分割 `mod.rs:581-607`）は、スキャナ側で正しくトークン化する。
- 位置情報はトークン自身が `TextRange` として持つ。parser 側の `current_pos` は廃止。
- 生テキスト脱出ハッチは「kind 付き raw span トークン」として一級市民にする。

### 4.2 Parser: イベントバッファ方式（rust-analyzer 型）

- `Vec<Event{Start(kind) | Token | Finish | Abandon}>` に記録し、最後に `GreenNodeBuilder` へ再生する。この1つの変更で:
  - (a) **投機パース + 巻き戻し**が可能になり、21個のヒューリスティックの大半が「試しにパースして駄目なら Abandon」に置換できる（hash-ref vs block の `;` 全走査、`<` の lexer クローン、signature 判定ミニパーサ、`try` の二重チェックポイント等）。
  - (b) **トリビア付与を独立パス**にでき、「primary は非トリビアトークン上で終わる」不変条件が消滅する。`hash_ref`/`array_ref` の非対称も消える。
  - (c) CST 形状（`STMT` ラップ、`FUNCTION_CALL_EXPR` の多義性、`EXPR_LIST` の有無）を再生時に一元的に正規化できる。
- **エラーは非消費をデフォルト**にし（`error()` / `error_and_bump()` を分離）、ADR-0003 の同期セット（`;` `}` 文開始キーワード）を今度こそ実装する。受け入れ基準: `direct_subscription_after_call` が 2 エラー、`sub_signature_invalid` が 6 エラーになること。診断メッセージから内部 enum 名（`Expected R_BRACE, found None`）を排除する。
- **組み込み関数の知識を単一テーブルに**: 現在22個のみ・4箇所に分散している知識を、Perl の実 prototype 文字列（`prototype` 関数の出力相当、~200個）から生成した1テーブルに統合し、1つの解釈ルーチンで処理する。`"sort"` や `"//"` の文字列比較特例を排除。
- 精度の高い部分（`precedence.rs` の Pratt テーブル、quote-like の2部形式パース、heredoc キュー）は移植する。ただし precedence の既知の誤り（bitwise が comparison より下、FILE_TEST の結合）は修正する。

### 4.3 Formatter: 2フェーズ + 垂直アライメント独立パス（本丸）

**フェーズ1 — レイアウト決定（CST → Doc IR）**

- Wadler 系の Doc IR（`group` / `nest` / `text` / `softline` / `hardline` / `raw`）を構築する。ただし純粋な幅駆動ではなく、**各 group の flat/broken をソースの改行から種付けする**（Prettier がオブジェクトリテラルで採る方式。gofmt 的ポリシーと Doc IR は両立する）。
- これにより:
  - 7つの改行述語 → group 生成時の1判定に collapse。
  - 14分岐の継続インデント → `nest(4, …)` 1ルールに collapse。ユーザー由来/フォーマッタ由来の改行の区別が不要になり、`LineBreakSource` も不要。
  - `is_simple_block`（7つの拒否ルール + メモ化 + それでもリークする `suppress_newlines`）→ `group` が flat になれるかの自然な帰結。
  - 将来の max-width 自動折り返し（formatting.md FUTURE-1）は「flat が幅を超えたら broken に倒す」だけで載る。現アーキテクチャでは事実上実装不可能だった。
- heredoc / POD / `__DATA__` / quote-like 本体は `raw` アトム（測定不能・改変不能）として表現する。
- コメントはトリビアテーブル（`comments/mod.rs` は現行コードで最も健全なモジュール。移植候補）から Doc 構築時に leading/trailing として**一元的に**取り付ける。出力パスを1つにし、コメント前スペースのルールを1箇所にする。

**フェーズ2 — 描画（Doc → 行列）**

- spacing は「同一 group 内の隣接要素間のエッジ属性」として、親ノード文脈つきで決定する。31アームの特例表と34箇所のバイパスの大半が消える。インデントは描画器が行構築時に付与する（文字列 append 時ではなく）。**これにより文字列リテラルへのインデント注入（F1）が表現不能になる。**

**フェーズ3 — 垂直アライメント（行列 → 行列）**

- perltidy の vertical aligner と同型の、**描画済み行ストリームに対する独立パス**にする。各行は「アライメント可能トークン（`=` `=>` 後置 `if` 行末コメント）の列位置」を注釈として持ち、後段が連続行グループを検出してパディングを挿入する。
- これにより:
  - 幅測定のための再フォーマットが不要になり **O(n²) が消える**。
  - アライメントがコメント・ネスト・複数行要素・「フォーマッタが今挿入した改行」と直交し、**2回目のパスで初めて整列するバグ（F3）と open issue の大半のバグクラスが構造的に消える**。
  - 仕様は formatting.md §7 がそのまま使える。文レベル/式リストレベルの2実装は1つになる。

### 4.4 テスト戦略の追加

1. **冪等性**: 全 fixture で `format(format(x)) == format(x)` を強制する。現状 `fixtures/control_flow.pl` が既に破れている（付録 A の F3）。
2. **意味保存**: 入力と出力を re-lex し、トリビア以外のトークン列一致を検証する。F1（文字列増殖）はこれで即検出できる。
3. `--check` をバイト比較からこの基準に載せ替える。
4. 実世界コーパス（`scripts/deperl` の対象など）に対する差分回帰を CI 化する。

---

## 5. 移行計画

ビッグバン書き換えではなく、**契約から順に**進める。同一 crate 内で新旧を並走させ、`scripts/diff` と既存スナップショットで差分レビューしながら切り替える。

1. **契約の定義**: `TokenKind` / `NodeKind` の分離、トリビアモデル（改行・空白・コメントの帰属規則）、CST 形状の正規形（`STMT` ラップ規則、`FUNCTION_CALL_EXPR` の分割）を先に文書化する。→ **完了: ADR 0004（kind分離）/ 0005（lexer契約）/ 0006（トリビアモデル）/ 0007（イベントparser + CST正規形）/ 0008（formatter Doc IR）として決定済み。**
2. **新 lexer**: 単一 `expect` 状態 + トークンバッファ + エラートークン。既存 lexer テストを移植し、7件の検証済みバグ（付録 A の D1〜D7）を修正確認のテストにする。
3. **イベント式 parser**: 既存 parser fixture で差分レビュー。ワークアラウンドを固定化した約35本の fixture は「仕様として維持するか」を1本ずつ判定する（付録 C）。
4. **Doc IR formatter**: 既存スナップショットで差分レビュー。意図的な挙動変更（例: コメント列のソースコピー廃止 → 規則ベースへ）は fixture 更新として明示コミットする。
5. 新旧切り替え後、旧実装を削除。CLAUDE.md の虚偽記述（存在しない `Builder` API、実装されていないエラー回復戦略、dead な「source mapping」）を現実に合わせて更新する。

formatter-first の順序も検討したが、parser の CST 形状不整合とトリビアの扱いが formatter に漏れているため、契約 → lexer → parser → formatter の順が結局早いと判断する。

---

## 付録 A: 検証済みバグ（再現確認済み）

### Lexer/Parser 起因

| # | 入力 | 現象 | 根本原因 |
|---|---|---|---|
| D1 | `sub f {`␤`    =head1 x` … | インデントされた `=head1` が POD 扱いになり以降を飲み込む | `at_line_start` が空白トークンで維持される（`mod.rs:679-687`）。実 Perl は桁0のみ |
| D2 | `q{unterminated;` の次行以降 | lexer モードが `QuoteLike` のまま復帰せず後続コードが破壊、エラー4連鎖 | エラーパスでモード未リセット（`quote.rs:343`, `quoted.rs:76-79` 等） |
| D3 | `{ qq{x} => 1 };` | ブロック誤判定、`;` が孤立した出力 | 先読みが `begin_quote_like` を見られない（`#` デリミタのみパッチ済み） |
| D4 | `total / 2 + count / 3` vs `total / 2` | 同一構文で `total` の木が変わる | regex 終端探索が非局所（ファイル全体） |
| D5 | `q #hello#` | `#` がデリミタ扱い（実 Perl ではコメント） | `begin_quote_like` を trivia スキップ前に呼ぶ設計 |
| D6 | `sub f(_) {}` / `sub f(+) {}` | prototype エラー（実 Perl では合法） | prototype を通常トークンとして再lex |
| D7 | `foo %h` vs `foo % h` | 空白の有無でハッシュ引数/剰余が変わる | `ambiguous_remainder_starts_sigil_target` の生文字判定（本質的曖昧性だが規則が空白依存） |

### Formatter 起因

| # | 入力 | 現象 | 根本原因 |
|---|---|---|---|
| F1 | ブロック内の `"line1\nline2"` | **文字列の中身にインデントが注入され、パスごとに増殖**（意味破壊・非有界の非冪等） | `write_str` が行頭 content token に `add_indent`（`writer.rs:99-101` × `predicates.rs:245-253`） |
| F2 | `sub f{my $x=shift;return $x+1}` | pass1 で桁0の壊れた行、pass2 で別の（同じく誤った）不動点 | `suppress_newlines` が `handle_spacing_after_with_token` の強制改行（`mod.rs:1133`）に届かない |
| F3 | `my $x=1;my $yy=2;my $zzz=3;` | **2回目のパスで初めて整列**（checked-in の `control_flow.pl` も非冪等） | アライメントグループがソースの NEWLINE を必要条件にする（`statement.rs:355-357`） |
| F4 | 等幅代入が N 行 | O(n²)。800行で29.7秒 | 幅測定 = 全再フォーマット + グループ毎行再構築（`statement.rs:402-404`, `:414-435`） |
| F5 | 複数行デリミタ先頭のコメント | 余分な空行が挿入される | コメント出力2系統 × NEWLINE 二重発行 |
| F6 | `if (...) # comment` `{` | K&R 違反の brace 落ち + 空行 | parser 側 FIXME（`control_flow.rs:192`）と formatter の合わせ技 |

## 付録 B: 曖昧性ヒューリスティック目録（要約）

lexer/parser にまたがる主要な手書きヒューリスティック。再設計時に「投機パースに置換」「expect 状態で解決」「テーブル駆動化」「仕様として維持」のいずれかに分類すること。

1. `/` regex vs 除算（Value 文脈 + ファイル全体の終端探索、`contextual.rs:40-189`）
2. 非組み込み関数後の `/` は除算（`call.rs:359-364`）
3. `//` defined-or vs 空 regex（**トークンテキスト `"//"` の文字列比較**、2箇所）
4. `%` `*` `&` `^` sigil vs 演算子（3値文脈 + 前後の生文字検査 `contextual.rs:610-658`）
5. `<...>` I/O vs 比較（**lexer クローンで投機消費**、`expression/mod.rs:66-106`。成立時に Pratt ループを打ち切る欠陥あり）
6. `<<` heredoc vs シフト（+ filehandle 救済の後付けパッチ3箇所）
7. `-x` file test（第3文字が非英数、実 file-test 文字集合との照合なし）
8. `x` 繰り返し vs 識別子（`x5` を nested lexer で再分割）
9. hash-ref vs block（先頭トークン4分類 + **ブレース本体の `;` 全走査**、`call.rs:96-180`。`{ $k => 1 }` は block になる等の穴）
10. bareword filehandle（後続が演算子でなければ filehandle、`call.rs:588-599`）
11. quote-like キーワード vs bareword（次が `=>` でなければ quote-like、`expression/mod.rs:58-64`）
12. 組み込み関数 prototype テーブル（**22個のみ**。`push` `die` `defined` `open` 等が欠落、`call.rs:58-89`）
13. `sort` の名前文字列比較による専用パーサ（`call.rs:257-262`）
14. signature vs prototype の手書きミニパーサ（`subroutine.rs:146-190`）
15. `try` 文 vs 関数（二重チェックポイントで遡及判定、`try_block.rs`）
16. keyword vs 修飾名プレフィックス（生 remainder の `::` 検査 + 後方文字走査）
17. `is_inside_hash_braces`（名前に反して「次が `}` か」だけを見る、`primary.rs:434-443`）
18. `$$x` デリファレンス vs `$$`（PID）（第3トークン分類、2箇所に重複）
19. `*foo{BAR}` の block vs braced-deref（`primary.rs:420-430`）
20. hex/binary literal + `..` の再分割（`mod.rs:532-576`）
21. 後置デリファレンス `->@*` 等の生文字列マッチ（`contextual.rs:412` の FIXME 付き）

## 付録 C: fixture 監査

parser fixture 78本中約35本、formatter の `regressions/` 6本は文法ではなく**現実装のワークアラウンド挙動**を固定化している。再設計時に各 fixture を以下に分類する:

- **仕様として維持**（例: `io_operator_disambiguation.pl` の10ケース、`package_cases.pl` のキーワード識別子）
- **新設計で不要になる**（例: `lt_with_comment.pl`、`quote_like_hash_cases.pl` の `#` デリミタパッチ群、`infix_newline_cases.pl` の trivia 整合ピン）
- **挙動を意図的に変更**（例: `comment_alignment_in_delimiters.pl` のソース空白コピー、エラー系 fixture のカスケードエラー数）

また現状のカバレッジ欠落（新設計で fixture 追加すべきもの）: インデントされた POD、`q #x#`、行跨ぎ regex の非局所性、prototype 文字 `_` `+` `\[$@]`、未終端 quote-like の後続行、`{}` 形式の infix 改行ケース、冪等性全般。

## 付録 D: その他の整理対象（安価な改善）

- dead code: `SyntaxKind::EOF` / `POD_START` / `QUALIFIED_IDENT`、`PrimaryRole::Variable`、`Token` の phantom variant 10個、`peek_for_any`、`TokenSpan.end_byte`（「source mapping」は未実装 — CLAUDE.md の記述は虚偽）、`unowned_trivia`。
- `parser/mod.rs` の inline test 群（CLAUDE.md 自身のルール違反、`test_digit_prefixed_ident_lexer` が2回定義）。
- `lexer/mod.rs` 冒頭の314行のテスト（private フィールド直接参照）。
- 日英混在コメントの統一。
- panic 系: `types.rs:54` / `quote.rs:194` の `panic!`、12箇所の `unreachable!`、`.unwrap()` 群 — フォーマッタとしては診断付きエラーに落とすべき。
