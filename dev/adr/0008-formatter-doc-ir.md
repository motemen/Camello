# ADR 0008: Formatter — Doc IR 2フェーズ + 垂直アライメント独立パス

- Status: Proposed
- Date: 2026-07-28
- Owners: camello core
- 参照: notes/2026-07-28-redesign-assessment.md §2.3, §4.3, 付録A (F1-F6)。ADR 0002 の継続インデントを包含し置き換える。formatting.md を仕様とする。

## Context

現行 formatter は CST を歩きながら文字列を直接出力する。IR がないため、(a) 幅測定＝全再フォーマットで O(n²)、(b)「ソースに改行があるか」述語が7つに分散、(c) spacing 特例表を34箇所が素通り、(d) 冪等性が破壊されている（文字列リテラルへのインデント注入 F1 はセマンティクス破壊）。改行保持（gofmt 的）ポリシー自体は維持する。

## Decision

### 1. 3フェーズ構成

```
CST + TriviaMap
  → [build]  Doc IR          （レイアウト決定。flat/broken はここで確定）
  → [render] Vec<Line>       （spacing・インデント適用。Line はアライメントアンカー注釈付き）
  → [align]  String          （垂直アライメントのパディング挿入）
```

### 2. Doc IR の代数

```rust
enum Doc {
    Token(SyntaxToken),            // 通常トークン。テキストは CST から
    Raw(SyntaxToken),              // verbatim: heredoc 本体, POD, __DATA__, quote-like 内容,
                                   // 文字列リテラル。レンダラは一切改変しない
    Space,                         // 明示的な1スペース
    Concat(Vec<Doc>),
    Group { broken: bool, body: Box<Doc> },  // broken は build 時に確定（§3）
    Indent(Box<Doc>),              // 中の改行に +1 単位（ブロック・継続インデント共用）
    Line,                          // broken group 内: 改行 / flat group 内: Space
    SoftLine,                      //          〃    : 改行 /      〃     : 何も出さない
    HardLine,                      // 常に改行（文末・ブロック内など）
    UserLine { broken: bool },     // ユーザー改行の保持点。ソース由来で個別に確定（§3）
    BlankLine,                     // 空行1つ（正規化済み）
    Anchor(AnchorClass),           // アライメントアンカー（§5）。出力幅ゼロ
    Comment(SyntaxToken, Placement), // TriviaMap 由来。Placement = OwnLine | Trailing
}
```

重要な性質:

- **spacing は build 時に `Space` として明示的に挿入する。** builder は CST の親ノード・兄弟を見られるので、現行の「(prev, current) の2トークン + 31特例」ではなく「ノード文脈つきの隣接規則」で決められる。レンダラはトークン間に暗黙のスペースを一切入れない（バイパス問題の消滅）。
- **`Raw` はレンダラが行分割・インデント付与をしない。** 複数行文字列リテラルも `Raw` にする。F1（インデント注入によるセマンティクス破壊）が**表現不能**になる。
- インデントはレンダラが行構築時に付ける。`write_str` 内での付与は存在しない。

### 3. broken の決定規則（gofmt 的ポリシーの一元化）

現行7述語を、build 時の2規則に集約する:

1. **Group の broken**: 構文ごとに定義した「判定点」にソースの NEWLINE があるか（ADR 0006 §4 によりノード range が正確なので厳密に判定できる）。
   - デリミタ組（`()` `[]` `{}` リテラル, 引数リスト, qw）: 開きデリミタ直後に NEWLINE → broken（formatting.md INDENT-2）。broken なら要素ごとに `Line`（1行1要素 + 末尾カンマ）。
   - ブロック: 制御構造のブロックは**無条件 broken**（NEWLINE-2）。`map`/`sub`/`do`/`try` 等のブロックは「単文・セミコロンなし・コメントなし・ソース改行なし」なら flat（単文ブロック）、それ以外 broken。現行 `is_simple_block` の7拒否規則はこの1判定に吸収される。
2. **UserLine**: flat な group 内・および文の継続位置（演算子の後/前、`,` の後、後置キーワード前）では、対応するソース位置に NEWLINE があれば `UserLine{broken: true}` にする（POLICY-4 のユーザー改行保持）。broken な `UserLine` は `Indent` 内にあるので継続インデントは自動で付く — ADR 0002 の14分岐は `Indent(...)` の構造に置き換わる。

フォーマッタが自分で挿入する改行（`;` の後、`{` の後、`}` の後）は `HardLine`。suppress 系のフラグ伝播は存在しない（flat group の中に `HardLine` を置かないことは builder の構造で保証され、F2 のリーク類が消える）。

### 4. render

- Doc を深さ優先で歩き、`Vec<Line>` を構築する。`Line` は `text` と `Vec<(AnchorClass, column)>` を持つ。
- 空行は `BlankLine` からのみ生じる（≤1 に正規化済み。BLANK_LINE-2/3）。自動空行挿入（sub 前後等, BLANK_LINE-1）は build 時に `BlankLine` を置くことで表現し、判定は TriviaMap による（writer 状態の再検査はしない）。
- コメント: `Comment(_, Trailing)` は行末に「最小スペース数（オプション `min_spaces_before_comment`、既定1）+ Anchor(TrailingComment)」で出力。`OwnLine` は現在のインデントで独立行に出力。**出力パスはこの1箇所のみ**（現行2系統の統一。ハードコード4スペースとソース空白コピーは廃止し、既定値の変更は fixture 更新として明示する）。

### 5. align — 垂直アライメント独立パス

perltidy の vertical aligner と同型の、**レンダリング済み行列に対する O(n) パス**にする。

- `AnchorClass = Assign | FatComma(depth) | PostfixKeyword | TrailingComment`。build 時に `=`（複合代入含む）・`=>`・後置 `if/unless/...`・行末コメントの直前に `Anchor` を置く。
- グループ化規則（formatting.md §7 をそのまま実装）: 連続する行で、(a) 空行で切れる、(b) 同クラスのアンカーを持たない行で切れる、(c) 文の形状キー（文ノード種 + `my/our/state` の有無 + リスト代入か）が変わったら切れる。ネストした `=>` は `FatComma(depth)` でクラスが分かれる。
- 各グループでアンカー列の最大値を取り、差分をアンカー位置に空白として挿入する。パディング幅には上限を設ける（オプション。issue #273 の DoS 対策）。
- **測定のための再フォーマットは存在しない**（O(n²) の解消、F4）。**ソースの空白数・ソースの改行は入力にならない**（レンダリング済みの列位置のみ）ので、「2回目のパスで初めて整列する」（F3）と「コメントが整列を壊す」クラス（issue #338, #339, #341, #342 等）が構造的に消える。

### 6. 冪等性・意味保存の不変条件（実装の受け入れ基準）

- **I1 (Raw 保存)**: `Raw` アトムの内容はビット単位で保存される。
- **I2 (seed 安定性)**: 出力の改行は broken な `Line`/`UserLine`/`HardLine` の位置にのみ現れ、出力を再パースすると同じ group が同じ broken 判定を受ける。構文ごとの seed 規則（§3）を定義する際、この往復安定性を満たすことを義務とする（例:「開きデリミタ直後の改行」は broken 時の出力自身が必ず持つ形なので安定）。
- **I3 (align の不動点)**: アライメントはレンダリング済み列のみから計算され、挿入されるパディングは新たなアンカーを作らない。よって align∘align = align。
- 検証: 全 fixture + 実コーパスに対し (a) `format(format(x)) == format(x)`、(b) 入力と出力を re-lex してトリビア以外のトークン列が一致（意味保存。F1 検出）を CI で強制する。`--check` も (a)(b) 基準に載せ替える。

### 7. 将来拡張のフック

- **max-width 自動折り返し**（FUTURE-1）: render 前に「flat group の測定幅 > 上限なら broken に倒す」1パスを挟むだけで載る。Doc の測定は `Token`/`Space` の長さの和で O(n)（`Raw` は測定不能として常に親を broken にする）。本 ADR では実装しない。
- インデント幅・タブ等のオプションはレンダラのパラメータ。
- ソースマップ（出力範囲→入力トークン）は `Line` が保持するトークン参照から自然に得られる。必要になるまで API 化しない（現行の dead な `TokenSpan` は作らない）。

## Consequences

- 検証済みバグ F1〜F6 が設計レベルで解消。open issue のアライメント系バグ群は align パスの単一実装に置き換わる。
- formatting.md が「Doc 構築規則（§3, §4）+ align 規則（§5）」への直接のマッピングを持つようになり、仕様と実装の対応が1対1になる。
- 挙動が意図的に変わる点（コメント前スペースの既定、align のグループ化細部、エラー入力時の出力）は fixture 更新として明示的にコミットする。
