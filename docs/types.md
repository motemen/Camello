# Perl Type Checking Specification

このドキュメントは、`camello check` が Perl のコードについて
何を知り、何を知らないままにするかを定義します。フォーマッタの仕様は
[formatting.md](formatting.md) に、実装の設計は [typecheck.md](typecheck.md) にあります。

Perl に静的な型はありません。ここに書かれている「型」は、シジル・リテラル・
コンストラクタ・アノテーションといった**証拠から導かれた形（shape）**であって、
実行時に成り立つことを保証するものではありません。

## 1. 基本方針 (POLICY)

### 1.1 分からないときは黙る

- (POLICY-1) **`Unknown` の伝播**: camello が形を決められない値は `Unknown` です。
  `Unknown` に対する操作の結果は `Unknown` であり、**`Unknown` について診断は出ません**。
- (POLICY-2) **両側が既知のときだけ言う**: 型の食い違いは、値の側と受け側の
  両方が既知のときにだけ報告されます。片方でも `Unknown` なら沈黙します。
- (POLICY-3) **アノテーションのないコードは無診断**: アノテーションを一つも持たず、
  camello が認識できるコンストラクタも使っていないプログラムには、型の診断は
  一つも出ません。これは機能の欠落ではなく、正しい動作です。

`Unknown` は「何でもありうる」を意味する `Any` とは別物です。`Any` は
「どんな値でもよい」とアノテーションが述べたもの、`Unknown` は「camello が
調べていない」ものです。

### 1.2 スコープの診断は別扱い

- (POLICY-4) `my` は宣言であり、`use strict` は未宣言の名前をエラーにします。
  したがって未宣言変数・未使用変数・シャドーイングの診断は、型と違って
  **ファイル内で健全**です。これらは型を一切使わずに報告されます。

### 1.3 実行しない

- (POLICY-5) camello はプログラムを実行せず、モジュールを読み込まず、`perl -c` も
  呼びません。結果として、`BEGIN` ブロック、`eval "..."`、`AUTOLOAD`、
  `local *glob = ...`、`$obj->$name` のような文字列メソッド名、`no strict 'refs'` の
  シンボルテーブル操作は、すべて不透明です。これらを通してしか到達できない
  サブルーチンは `Unknown` であり、診断は出ません。

## 2. 型の言語 (TYPE)

アノテーションに書ける型は、Moose の文字列制約と Types::Standard の共通部分です。
**ここにないものは書けません** — アノテーションで表現できない型は、camello に
期待させることのできない型です。

### 2.1 二つの書き方 (TYPE-1)

同じ文法が、文字列としても Perl の式としても書けます。

```perl
has items => (is => 'ro', isa => 'ArrayRef[Str]');   # 文字列
has items => (is => 'ro', isa => ArrayRef[Str]);     # 式 (Types::Standard)
```

camello はどちらも同じように読みます。

### 2.2 型の一覧 (TYPE-2)

| 書き方 | 意味 |
| --- | --- |
| `Any`, `Item` | 何でも |
| `Defined` | `undef` でない何か |
| `Value` | 参照でない値 |
| `Str` | 文字列 |
| `Num` | 数 |
| `Int` | 整数 |
| `Bool` | 真偽値（`Int` とは別物、TYPE-5） |
| `ClassName`, `RoleName` | クラス名・ロール名を表す文字列 |
| `Undef` | `undef` |
| `Ref` | 何かの参照 |
| `ScalarRef[T]` | スカラーへの参照 |
| `ArrayRef[T]`, `ArrayLike[T]` | 配列参照 |
| `Tuple[T, U, ...]` | 長さと各要素の型が決まった配列参照 |
| `HashRef[T]`, `HashLike[T]` | ハッシュ参照 |
| `Dict[k => T, ...]` | キーの決まったハッシュ参照（TYPE-4） |
| `Map[K, V]` | キーと値の型が決まったハッシュ参照 |
| `CodeRef`, `CodeLike` | コード参照 |
| `RegexpRef` | `qr//` |
| `GlobRef`, `FileHandle` | グロブ参照・ファイルハンドル |
| `Object` | ブレスされた何か |
| `InstanceOf['Foo']` | クラス `Foo` のインスタンス |
| `ConsumerOf['Role']` | ロール `Role` を消費するもの |
| `HasMethods[a, b]`, `Overload[...]` | 指定のメソッドを持つもの |
| `Enum[a, b, c]` | 列挙 |
| `Maybe[T]` | `T` または `undef` |
| `Optional[T]` | スロットそのものが無くてよい（`Dict` と引数リストの中だけ） |
| `T \| U` | 合併 |

`Types::Common::Numeric` と `Types::Common::String` の絞り込み型
（`PositiveInt`, `NonEmptyStr`, `StrictNum` など）は、**基底の型として読まれます**。
`PositiveInt` は `Int`、`NonEmptyStr` は `Str` です。制約の構造的な部分だけが
静的に使えるもので、述語は実行時の絞り込みだからです。

### 2.3 知らない名前はクラス名 (TYPE-3)

上の表にない裸の名前や文字列は、**クラス名**として読まれます
（Moose と同じ読み方です）。

```perl
has user => (is => 'ro', isa => 'MyApp::User');   # InstanceOf['MyApp::User']
```

この読み方の代償として、型名の打ち間違いは「そんなクラスのインスタンス」になり、
そのクラスは解決できないので `Unknown` になり、黙って通ります。それを拾うのが
`unknown-type`（DIAG-11）で、`info` として出ます。

### 2.4 `Dict` は閉じている、推論されたハッシュは開いている (TYPE-4)

`Dict[name => Str]` は**閉じた**型です。そこにない鍵を読むと `unknown-key`
（DIAG-6）になります。`slurpy` を書くと開きます。

```perl
Dict[name => Str, age => Optional[Int]]           # name と age だけ
Dict[name => Str, slurpy HashRef[Str]]            # 他の鍵もあってよい
```

一方、`{ a => 1, b => 2 }` のようなリテラルから**推論された**ハッシュは、
`Dict` の形を持ちますが常に開いています（INFER-3）。プログラムが後から鍵を
足さないとは誰も言っていないからです。したがって `unknown-key` が鍵の読み出しで
出るのは、**アノテーションに書かれた `Dict` に対してだけ**です。

- (TYPE-4b) パラメータを書かない `Dict` / `Map` / `Tuple` は、参照の種類しか
  言っていません。`Dict` は `HashRef`、`Tuple` は `ArrayRef` として読みます。
  Type::Tiny のパラメータなし `Dict` は任意のハッシュを受けるからで、これを
  「鍵ゼロの閉じた `Dict`」と読むと、そこから読む鍵がすべて `unknown-key` に
  なります。
- (TYPE-4c) `Map[K, V]` と `HashRef[V]` は、鍵の側を言ったか言わなかったかの
  違いしかない同じ参照です。互いに適合し、比べられるのは値の側だけです。
  `Dict` と `Map` の間は、`Dict` と `HashRef` の間と同じく比べません。
- (TYPE-4d) `Dict` どうしが適合するのは、**宣言側のスロットがすべて値の側に
  あって適合する**ときです。宣言側に無い鍵は矛盾ではありません（値の側は
  TYPE-4 のとおり、そうと言わない限り開いています）。宣言側にあって値の側に
  無い鍵は、そのスロットが `Optional` であるか、値の側が開いているなら
  黙ります。閉じた値がそれを持たないと言っているときだけ、二つの宣言の矛盾
  です。

### 2.5 `Str` と数、`Bool` (TYPE-5)

- (TYPE-5a) Perl の値が実際に持っている部分型関係、`Int <: Num <: Str` を採ります。
  数を `Str` のスロットに渡すのは適合します。
- (TYPE-5b) 数に見える文字列リテラルは数です。`"3"` は `Int`、`"1.5"` は `Num`、
  `"abc"` は `Str` です。したがって `Int` のスロットに `"abc"` を渡すのは
  `type-mismatch` ですが、`"3"` は通ります。
- (TYPE-5c) `Bool` は名前的に別の型です。`Bool` のスロットに `Int` を渡すことも、
  その逆も適合します。camello は**値**を追いません（`2` を `Bool` に渡しても
  何も言いません）。形だけを見ます。
- (TYPE-5d) `undef` は `Bool` です。Moose も Types::Standard も `Bool` の値を
  `0` / `1` / `''` / `undef` の四つとしているので、`Bool` のスロットに `undef`
  を渡すのは適合します。`Bool` 以外のスロットに渡せるのは `Undef` と
  `Maybe[...]` だけです。
- (TYPE-5e) `Enum` の値は文字列です。`Enum[...] <: Str` を採るので、`Enum` は
  `Str` のスロットに入ります。`Enum` どうしは、値の集合が相手の部分集合である
  ときに適合します。
- (TYPE-5f) `RegexpRef` と `InstanceOf['Regexp']` は同じものです。`qr//` は
  `Regexp` に bless された参照で、Type::Tiny も両方の名前で同じ型を指します。
- (TYPE-5g) クラス名がプログラムのどこにも無いとき、そのスロットに参照を渡す
  ことについては何も言いません。読めなかった型ライブラリの構造型は、TYPE-3 に
  よってそこに `InstanceOf['名前']` として届きます。宣言が読めていないのに
  「その形ではない」とは言えません。
- (TYPE-5h) `Defined` は `undef` **以外**のすべて、`Value` は defined な
  **非参照**の値です（Types::Standard）。それぞれが排除するのはその一つだけで、
  ほかについては何も言いません。二つとも `Any` と同じ「頂」として扱っていたので、
  `undef` を `Defined` に、参照を `Value` に渡すのが通っていました。

### 2.6 型の族 (TYPE-6)

いくつかの型は、**値がどういう種類のものか**だけを言っていて、それ以上のことを
言っていません。それぞれが族の頂で、族に属するものはその頂に入ります。

| 頂 | 族 |
| --- | --- |
| `Ref` | すべての参照（`ArrayRef` `HashRef` `Dict` `Map` `Tuple` `CodeRef` `RegexpRef` `GlobRef` `ScalarRef` `Object` `InstanceOf` …） |
| `Value` | defined な非参照（`Str` `Num` `Int` `Bool` `Enum` `ClassName` `RoleName`） |
| `Defined` | `undef` 以外のすべて |
| `Object` | `InstanceOf` / `ConsumerOf` / `HasMethods` |
| `GlobRef` | `FileHandle` |
| `Str` ⊇ `Num` ⊇ `Int` | 数と文字列の連鎖（TYPE-5a）。`Enum` `ClassName` `RoleName` も `Str` の族 |

- (TYPE-6a) **種類の族**（`Ref` `Value` `Defined` `Object` `GlobRef`）は
  **どちらの向きでも矛盾しません**。`ArrayRef` は `Ref` ですし、`Ref` としか
  分かっていない値は `ArrayRef` でありえます。これを繋いでいなかったときは、
  `[1]` も `{a=>1}` も `Ref` のスロットで `type-mismatch` になっていました。
- (TYPE-6b) **数と文字列の連鎖は向きがあります**。`Int` は `Str` のスロットに
  入りますが、`Str` の値は `Int` のスロットに入りません。数に見えるリテラルは
  すでに `Int` なので（TYPE-5b）、残った `Str` は数ではない文字列だからです。

### 2.7 二つの関係 (TYPE-7)

型どうしの問いは二つあり、camello はその区別を持っています。

- **`compatible(値, スロット)`** ——「この値がこのスロットに入りうるか」。
  診断が出るのは**これが否と言ったときだけ**です（POLICY 1.1）。向きのある
  関係で（TYPE-6b）、合併は**どれか一つの枝が入りうるなら**入りうると読みます。
- **`is_assignable(値, スロット)`** —— 集合の包含、つまり値のとりうる値が
  すべてスロットのものであるか。合併は**すべての枝**が入る必要があり、`Bool` は
  `undef` を含むので `Value` でも `Defined` でもありません。

- (TYPE-7a) 報告に使うのは `compatible` の方です。動的な Perl に対して包含を
  包含を報告関係にすると、`Bool` と `Enum` のスロットなど、TYPE-5c で
  「値は追わない」と決めた分まで鳴ります。
- (TYPE-7b) 別のまとまった形は「`undef` を含みうる値を、`undef` を許さない
  スロットへ渡している」もので、これは一つの診断として切り出す価値があります。
  `maybe-deref` がデリファレンスについて言っていることの、引数版です。
- (TYPE-7c) `is_assignable` は**まだどこからも報告されません**。厳格な読みを
  入れるときの土台であり、二つの関係を互いに突き合わせるテスト
  （`assignable ⇒ compatible`）の片方です。

## 3. アノテーションの読み取り (ANNOT)

### 3.1 どれもインポートで裏付けられている (ANNOT-1)

`has` や `args` を認識するのは、**その名前を提供しうる `use` がそのパッケージに
あるときだけ**です。自作の `sub has` が Moose の `has` と取り違えられることは
ありません。

- (ANNOT-1a) 単位は**ファイルではなくパッケージ**です。`use Moose` は書かれた
  パッケージに `has` をインポートするので、同じファイルの別のパッケージが持つ
  `sub has` はそれとは別のものです。`package Foo { ... }` はブロックまで、
  `package Foo;` は次の `package` までがその範囲で、これは perl がインポートを
  効かせる範囲そのものです。パッケージの中でなら `use Moose` が `has` より下に
  あってもかまいません。ファイル単位で読んでいたときは、`use Moose` と一度も
  書いていないパッケージが Moose の属性とコンストラクタを貰い、`Plain->new(...)`
  が存在しないコンストラクタに対する `unknown-key` になっていました。

| 認識するもの | 必要な `use` |
| --- | --- |
| `has` | `Moose`, `Moo`, `Mouse`, それぞれの `::Role`, `Mojo::Base` ほか |
| `args` / `args_pos` | `Smart::Args`, `Smart::Args::TypeTiny` |
| `rw`/`ro`/... の宣言 | `Class::Accessor::Typed` |
| `mk_accessors` 一族 | `Class::Accessor::Lite`, 同 `::Lazy`, `Class::Accessor`, 同 `::Fast`, 同 `::Faster` |
| 型 DSL (`type` / `declare` / ...) | `Type::` / `Types::` / `MooseX::Types` の各一族、`*::Util::TypeConstraints` |

型 DSL だけは、一覧ではなく**一族**で裏付けます（ANNOT-8d）。理由はそこに書きます。

### 3.2 `has` (ANNOT-2)

```perl
has name  => (is => 'ro', isa => 'Str', required => 1);
has items => (is => 'rw', isa => ArrayRef[Item], default => sub { [] });
has [qw(a b)] => (is => 'ro', isa => 'Int');       # まとめて宣言
has '+name'   => (default => 'x');                 # 上書き（型は親のもの）
```

- `isa` が型を与えます。無いとき、また値がリテラルでも型式でもないときは `Unknown` です。
- `does => 'Role'` は `ConsumerOf['Role']` を与えます。
- `required` と `default` / `builder` / `lazy` が、`new` でそのスロットを
  省略できるかを決めます。
- `reader` / `writer` / `accessor` / `predicate` / `clearer` は、それぞれ
  名付けられたメソッドを生やします。
- `handles` は `[qw(a b)]` と `{ local => 'remote' }` の形だけ読みます。
  正規表現やロール名を渡した場合、委譲されるメソッドの集合は不明になり、
  そのクラスについて `unknown-method` は言わなくなります。
- (ANNOT-2a) `coerce => 1` が広げるのは**入力側だけ**です。宣言された型は
  **強制変換後**の値の上限で、変換関数は camello から見えないので、スロットに
  入れられるものは何でもよいことになります。一方 reader が返すのは宣言した型の
  ままです。入出力を一つの型で持って、入力に合わせて `Unknown` に潰していた
  ときは、出力の型まで失っていました。

### 3.3 `Smart::Args` (ANNOT-3)

```perl
sub greet {
    args my $self,
         my $who   => 'Str',
         my $times => { isa => 'Int', default => 1 },
         my $loud  => { isa => Bool, optional => 1 };
    ...
}
sub at { args_pos my $self, my $i => 'Int'; ... }
```

- **本体の最初の文**が `args` / `args_pos` の呼び出しであることが、これを
  引数リストにします。他の場所の `args` はただの呼び出しで、何も宣言しません。
- `args` は名前つき、`args_pos` は位置引数です。
- 規則は型の文字列、型式、または `isa` / `optional` / `default` を持つハッシュ参照です。
- 最初の項目が `$self` または `$class` ならそれが invocant で、そのサブルーチンは
  メソッドとして扱われます。`$class` は `ClassName`、`$self` は
  `InstanceOf[そのパッケージ]` です。
- 規則のない `my $x` は `Any` であって `Unknown` ではありません。Smart::Args は
  それを必須引数として扱い、camello も個数についてはそう扱います。
- (ANNOT-3a) `optional`（および `default`）のある引数には、**明示的に `undef` を
  渡せます**。Smart::Args は型より先に規則を読み、値が未定義ならそのまま返して
  型検査に到達しないからです。`f(x => undef)` は
  `my $x => { isa => 'Str', optional => 1 }` に対して通ります。
  省略できない引数に `undef` を渡すのは `type-mismatch` のままです。
- `args` は**レキシカルも宣言**します。これがなければ `args` を使うすべての
  サブルーチンで全パラメータが未宣言と報告されてしまいます。

### 3.4 `Class::Accessor::Typed` (ANNOT-4)

```perl
use Class::Accessor::Typed (
    rw      => { name => 'Str', tags => 'ArrayRef[Str]' },
    ro      => { id => { isa => 'Int' } },
    ro_lazy => { conn => { isa => 'DBI::db', builder => 'build_conn' } },
    new     => 1,
);
```

引数リストがそのまま宣言です。`rw` / `ro` / `wo` とその `_lazy` 版が `has` と
同じように属性になります。`new => 0` は生成されるコンストラクタを消します。

- (ANNOT-4a) **必須かどうかの既定が Moose と逆です。** ここではスロットは
  `optional` と書くか、`default` を与えるか、lazy であるかしない限り**必須**です。
  生成される `new` が `missing mandatory parameter named '$x'` で死ぬからで、
  これは推測ではなく規則です（[DIAG-13](#73-missing-argument-について)）。
- (ANNOT-4b) `Frameworks` はファイル単位なので、同じファイルに `use Mouse` が
  あっても、`new => 0` と書いたパッケージのコンストラクタは消えたままです。
  自分で決めたパッケージには一括処理が触りません。

### 3.5 シグネチャ (ANNOT-5)

```perl
sub greet ($self, $who, $times = 1, @rest) { ... }
```

型はすべて `Any` です。**個数だけ**が正確に分かります。最小は最初のデフォルト値の
前までの数、最大はスラーピーがあれば無限、なければ総数です。perl 自身が実行時に
個数違いで die するので、これを静的に言うのに誤検知はありません。

- (ANNOT-5a) 引数に名前のない `()` や `($)` や `($$;@)` は**プロトタイプ**であって
  シグネチャではありません。`sub f()` については、本体が `@_` を読んでいることを
  証拠にプロトタイプと判定します。
- (ANNOT-5b) `method f { ... }`（`class` 機能）には、宣言のどこにも書かれていない
  invocant が一つあります。perl がそれを渡し、`@_` からは外すからです。camello は
  引数リストの先頭にそれを補います。`method f()` は「invocant を含めて 1 引数」で
  あって 0 ではないので、`$obj->f` は個数違いになりません。`method` に `()` が
  あればそれは常にシグネチャです —— プロトタイプは `class` 機能の下に存在しない
  ので、(ANNOT-5a) の推測は `method` には効きません。

### 3.6 `@_` の展開 (ANNOT-6)

アノテーションのないサブルーチンでも、最初の文が

```perl
my ($self, $x, %opts) = @_;      # または
my $x = shift;                   # の連続（`shift || 'default'` も可）
```

であれば、位置引数のリストが読み取れます。ただし：

- (ANNOT-6a) 型は分かりません。すべて `Any` です。
- (ANNOT-6b) **最小個数は課しません**。perl は足りない分を `undef` で埋めるので、
  四つ宣言して二つで呼ぶのは正しいプログラムです。最大だけが上限になります。
- (ANNOT-6c) 本体が他の場所で `@_` に触れていたら（`$_[0]`、`scalar @_`、
  裸の `shift` / `pop`、`goto &sub`）、引数リストは `Unknown` になり、
  個数について何も言いません。
- (ANNOT-6d) ここで束縛された名前は**引数**であって、値を持つために選ばれた
  ローカル変数ではありません。読まれていないときの扱いも別です
  ([DIAG-12](#7-診断-diag))。

### 3.7 `Returns:` コメント (ANNOT-7)

camello が導入する唯一のアノテーションです。

```perl
# Returns: ArrayRef[Item]
sub items { ... }

# Returns: Maybe[Str] | list: (Str, Int)
sub pair { ... }

# Returns: ()
sub notify { ... }
```

- **文法**: `sub` の直前のコメントブロック（ブロックと `sub` の間に空行があっても
  よく、ブロックの**中**に空行があってはいけません）の中で、`#` と空白の後が
  `Returns:` で始まる行。
- 残りは次の四つのどれかです。スカラーコンテキストの型、**先頭から末尾まで
  丸括弧で囲まれた**リストコンテキストの形（中のトップレベルのコンマがスロットの
  区切り、型ひとつに `...` を付けたものが「その型が任意個」）、または
  「何も返さない」を意味する `()`。

  ```perl
  # Returns: Str               スカラーコンテキストで Str
  # Returns: (Str, Int)        リストコンテキストでちょうど二つ、Str と Int
  # Returns: (Row ...)         リストコンテキストで Row が任意個
  # Returns: ()                何も返さない
  ```

  括弧をグループ化ではなくリストの形に使うのは、スカラー型全体を囲むグループ化に
  `Str | Undef` で足りない用途がなく、また `()` が「ゼロ個のリスト」である以上
  `(Str)` は「一個のリスト」でなければ辻褄が合わないからです。スロットの**中**の
  括弧は今までどおりグループ化なので、`(Str | Undef, Int)` は二スロットです。
- 両方書くときは **`Returns:` の行を二つ**書きます（順序は自由）。コメント
  ブロックの中のすべての `Returns:` 行が読まれ、同じ種類が二つあれば
  `bad-annotation` です。片方だけを書いたサブルーチンは、もう片方について
  **何も言いません**。コンマ演算子に従えば `Returns: (A, B)` はスカラー
  コンテキストで `B`、`(Row ...)` は個数になりますが、この二つの規則は互いに
  食い違うので、スカラー型が欲しいサブルーチンはそれを書きます。
- 属性 (`sub f :Returns(Str)`) ではなくコメントなのは、どの perl でも、どんな
  属性ハンドラの下でも足せる必要があるからです。`camello format` はコメントを
  一バイトも変えないので（[contracts.md](contracts.md) の `comments` 不変条件）、
  整形で壊れることもありません。
- (ANNOT-7a) `Returns:` とその推論された戻り値が食い違うとき、**アノテーションが
  勝ち**、推論された形の方が `return` の位置で報告されます（DIAG-9）。
  逆向きはありません。本体から読み取られた戻り値（INFER-4a）は、書かれた
  アノテーションが無いときにだけ生まれるので、矛盾する相手がいません。
  リスト側も同じように照合されます。**個数が合わなければ `error`** です
  —— アノテーションが「二つ」と書き、`return` が三つ書いている、という
  両方書かれている食い違いだからです。スロットの型が合わないときは
  スカラー側と同じ規則（リテラルなら `error`、推論された値なら `warning`）で、
  `return @rows` を `(Row ...)` に対して照合するのは要素単位です。
- (ANNOT-7b) 読めない `Returns:` は診断になります（DIAG-8）。黙って無視される
  アノテーションは、無いより悪いからです。
- (ANNOT-7d) 旧来の `| list: (...)` の形は**廃止されました**。書かれていれば
  `bad-annotation` で、新しい書き方を示します。
- (ANNOT-7c) ただし、型の**形をしていない**ものは散文として扱われ、何も言いません。
  `# Returns:    modified template` のような行はアノテーションではありません。
  「括弧の外に裸の名前が二つ並んでいる」ものは散文です。

### 3.8 型ライブラリ (ANNOT-8)

プロジェクト自身の `Type::Library` から、次の形だけ読みます。

```perl
declare 'PositiveInt', as Int, where { $_ > 0 };   # Int の部分型
declare 'Handle', as InstanceOf['IO::Handle'];
subtype Name => as Str;                            # `subtype` も `type` も同じ
type FooBar  => as Foo | Bar;                      # 宣言済みの名前どうしの合併
intersection 'Both', [Foo, Bar];                   # 格子に交差はないので Unknown
class_type 'User', { class => 'MyApp::User' };     # InstanceOf
role_type 'Loggable';                              # ConsumerOf
enum 'Color', [qw(red green blue)];                # Enum
union 'Id', [Int, Str];                            # Int | Str
```

`as T` が親を与え、`where` は無視されます。`as` を持たない `declare` は `Any` です。

- (ANNOT-8a) **宣言された名前は、それを書いたすべてのアノテーションの後ろに立ちます。**
  型の位置にある裸の名前は、どこも宣言していなければクラス名として読まれます
  ([TYPE-3](#23-知らない名前はクラス名-type-3))。宣言があればその形に置き換わり、
  `args my $n => Count` は `Count` の中身で検査されます。置き換えは宣言の中でも
  起こるので、`as ArrayRef[Count]` や `as Foo | Bar` のように名前を重ねられます。
- (ANNOT-8b) 名前はパッケージごとではなく**実行全体で一つ**です。型ライブラリは
  インポートされるために存在し、インポートした側は裸の名前を書くからです。
  同じ名前が二度宣言されていたら、最初のものが答えです。
- (ANNOT-8c) 自分自身に解決する名前（`class_type 'DateTime'`）はクラス名のままです。
  互いを指し合う名前（`type A => as B; type B => as A;`）は型ではないので `Unknown`
  になります。スタックが溢れることはありません。
- (ANNOT-8d) 認識されるのは、それを供給しうる `use` がある場合だけです
  ([ANNOT-1](#31-どれもインポートで裏付けられている-annot-1))。ただしここだけは
  一覧ではなく**一族**で判定します。`Type::*`、`Types::*`、`MooseX::Types*`、
  `MouseX::Types*`、および `*::Util::TypeConstraints` のいずれかを `use` していれば、
  このファイルは型 DSL を書いている、と読みます。

  一覧にしないのは、この語彙（`declare` / `type` / `as` / `enum` / `class_type`）を
  供給するディストリビューションが複数あり、しかも**どれが供給したかを言い当てるのが
  難しい**からです。`Type::Utils` は `type` を `-all` のときしか出さず、
  `Type::Library -base` は再エクスポートし、`MooseX::Types` は自前のものを持ち、
  実際のファイルは定数の出どころである `Types::Standard` しか書いていないことが
  よくあります。外すと**ライブラリ一つ分のアノテーションが丸ごと死にます**。
  逆に緩めて外した場合の代償は、`Types::` を `use` しているファイルの裸の `enum` が
  宣言として読まれることですが、それは何にも解決しないので黙ったままです。

### 3.9 スタブ (ANNOT-9)

XS で書かれている、`AUTOLOAD` で生えている、あるいは単に camello の認識器が
カバーしない書き方をしている依存モジュールには、スタブを与えられます。
`camello typecheck --stubs stubs/` あるいは設定ファイルの `stubs` で
指定したディレクトリに置いた `.pm` です。

```perl
package DBI::db;
# Returns: Maybe[DBI::st]
sub prepare ($self, $sql) {}
# Returns: Maybe[HashRef]
sub selectrow_hashref ($self, $sql, $attr = undef, @bind) {}
```

- スタブはただの Perl で、通常の宣言パスを通ります。新しい構文はありません。
- あるパッケージにスタブがあれば、**実物の宣言を丸ごと置き換えます**。
- スタブ自身に対して診断が出ることはありません。

### 3.10 `Class::Accessor::Lite` 一族 (ANNOT-10)

同じ「アクセサを生やす」でも、こちらは**型をほとんど持ちません**。読めるのは
名前とアクセスの向き、そして `new` があるかどうかです。属性の型は `Any` ではなく
`Unknown` です — モジュールが何も言っていないので、こちらも何も言いません。
例外は lazy なスロットで、そこには builder という書かれた出どころがあります
（ANNOT-10f）。

書き方は二つあります。`use` の引数リストが宣言であるもの、

```perl
use Class::Accessor::Lite (
    new => 1,
    rw  => [ qw(foo bar) ],
    ro  => [ qw(baz) ],
    wo  => [ qw(hoge) ],
);

use Class::Accessor::Lite::Lazy (
    ro_lazy => [ 'hoge', { poyo => \&make_poyo, poe => 'make_poe' } ],
    rw_lazy => { baz => 'make_baz' },
);
```

と、クラスメソッドを呼ぶものです。

```perl
use base 'Class::Accessor';
__PACKAGE__->follow_best_practice;
__PACKAGE__->mk_accessors(qw(name role));

use Class::Accessor::Lite;
Class::Accessor::Lite->mk_new_and_accessors(qw(foo bar));
```

- (ANNOT-10a) `mk_accessors` / `mk_ro_accessors` / `mk_wo_accessors` /
  `mk_lazy_accessors` / `mk_ro_lazy_accessors` / `mk_new` /
  `mk_new_and_accessors` を読みます。アクセサが生えるのは、**その文が書かれている
  パッケージ**です。`Class::Accessor::Lite->mk_accessors` は `caller` に生やし、
  `Class::Accessor` のサブクラスは自分自身に対して呼ぶので、どちらも同じ答えに
  なります。`Foo->mk_accessors(...)` のように別のクラス名を書いてあればそちらです。
  invocant が変数（`$class->mk_accessors(...)`）ならどのパッケージか分からないので、
  何もしません。
- (ANNOT-10b) `follow_best_practice` はそれ**以降**の `mk_*` に効き、アクセサが
  `get_x` / `set_x` になります。属性の名前は `x` のままです — ハッシュの鍵であり、
  `new` に渡す名前でもあるからです。なお `x` 自体はもうメソッドではありませんが、
  camello はそこまでは言いません（言わない方の間違いです）。
- (ANNOT-10c) **`new` は名乗り出たときだけあります。** `new => 1` も `mk_new` も
  `mk_new_and_accessors` もないクラスに `new` はなく、`Foo->new` は
  `unknown-method` です。`use base 'Class::Accessor'` の場合は親が `new` を
  持っているので、通常のメソッド解決でそちらに解決します。
- (ANNOT-10d) この `new` は**渡されたハッシュをそのまま bless します**
  ([INFER-2g](#42-コンストラクタ-infer-2))。アクセサのない鍵も通り、
  `$self->{key}` として読めるので、`unknown-key` は `error` ではなく `warning`
  です（[DIAG-6a](#71-重大度が動くもの)）。必須の鍵もありません — 渡されたものを
  見ないので、足りないと気づきようがないからです。
- (ANNOT-10e) `use Class::Accessor 'antlers'`（または `'moose-like'`）は `has` を
  export する唯一の綴りなので、そのファイルは Moose 系として読まれます。そちらは
  型を持つので、`unknown-key` も型検査も普通に効きます。
- (ANNOT-10f) **lazy なスロットだけは型を持ちます。builder が言っているからです。**
  `Class::Accessor::Lite::Lazy` の `ro_lazy` / `rw_lazy` / `mk_lazy_accessors` /
  `mk_ro_lazy_accessors` が生やすアクセサは `$self->{name} //= $self->$builder`
  なので、builder の戻り値がそのままアクセサの戻り値です。この一族で唯一、型の
  出どころがあります。builder の名前は、書いてなければ `_build_$name`、
  `{ poyo => 'make_poyo' }` や `{ poyo => \&make_poyo }` と書いてあればそれです。
  `{ yyy => sub {...} }` は名前がないので `Unknown` のままです。

  builder は**メソッドとして**呼ばれるので、解決は invocant のクラスの MRO を
  辿ります — サブクラスが builder を上書きしていれば、そちらの戻り値です。
  そして解決は**呼ばれた時点で**行います。builder の戻り値は
  [推論されるもの](return-inference.md)であり、宣言を読んだ時点では
  まだ決まっていないからです。

  これは `isa` のような**保証ではありません**。ANNOT-10d の通り `new` は開いて
  いるので `Foo->new(lazy_slot => $anything)` は通りますし、`rw_lazy` には setter
  もあります。それでも builder は「このスロットには何が入るか」について作者が
  書いた唯一の記述なので、INFER-2g が `Foo->new` を `InstanceOf['Foo']` と読むのと
  同じ実際主義で受け取ります。

### 3.11 `use constant` (ANNOT-11)

```perl
use constant PI       => 3.14159;
use constant WEEKDAYS => qw(Mon Tue);
use constant { E => 2.71, PHI => 1.61 };
```

- (ANNOT-11a) 宣言される名前を読みます。定数はサブルーチンなので、`Foo->NAME` は
  ふつうのメソッド呼び出しであり、定数の見えないパッケージはそのすべてに
  `unknown-method` を返していました。
- (ANNOT-11b) **値は読みません。** 定数が返すのは後ろの式を評価した結果で、
  camello は評価しません（POLICY-5）。型は `Unknown` です。
- (ANNOT-11c) 引数の個数も見ません。定数は引数を取りませんが、`Foo->NAME` は
  invocant を渡し、perl はそれを咎めません。数えるものがないので数えません。

### 3.12 自作のラッパーモジュール (ANNOT-12)

認識は「その名前を提供しうる `use` があること」で裏付けられます（ANNOT-1）。
`Class::Accessor::Typed` を自作のモジュールで包んでいるプロジェクトは、その
裏付けを失っています —— どのファイルも `use My::Accessors` としか書いておらず、
ラッパー自身のファイルにあるのは実行時の `sub import` だけで、そこから読み取れる
宣言はありません。

そこで、プロジェクトが `camello.toml` でそれを一度だけ言います
（[OFF-2](#82-プロジェクトごと-off-2)）。

```toml
[check.read-as]
"My::Accessors" = "Class::Accessor::Typed"
"My::Args"      = "Smart::Args::TypeTiny"
```

- (ANNOT-12a) 読み替えが効くのは**認識器に対してだけ**です。`use` が何を指すかは
  書かれたとおりで、依存の解決はラッパー自身のパスを探しますし、インポートされる
  名前もラッパーのものです。
- (ANNOT-12b) 読み替えられる先は、camello がすでに知っているモジュール名です。
  Moose 系・`Smart::Args`・`Class::Accessor::Typed`・`Class::Accessor::Lite` 一族・
  型ライブラリ・XS ローダーのいずれも指せます。
- (ANNOT-12c) 宣言のキャッシュ（DEPS-6）はこの設定込みで鍵付けされます。同じ
  バイトでも、読み替えの下で読んだ宣言は別の宣言だからです。

## 4. 推論 (INFER)

推論は、アノテーションのある部分に照合する相手を与えるために存在します。
局所的で、前向きで、**早めに諦めます**。

### 4.1 リテラルと構築子 (INFER-1)

| 式 | 型 |
| --- | --- |
| `42` | `Int` |
| `1.5` | `Num` |
| `"x"` | `Str` |
| `"3"` | `Int`（数に見える文字列は数、TYPE-5b） |
| `"$a $b"`, `qq{...}`, ヒアドキュメント | `Str` |
| `[ ... ]` | `ArrayRef[要素型の合併]` |
| `{ k => v, ... }` | 鍵がすべてリテラルなら開いた `Dict`、そうでなければ `HashRef[合併]` |
| `sub { ... }` | `CodeRef` |
| `qr//` | `RegexpRef` |
| `\$x` | `ScalarRef[$x の型]` |
| `\@a` | `ArrayRef[@a の要素型]` |
| `\%h` | `HashRef[Unknown]` |
| `undef` | `Undef` |

- (INFER-1a) 算術のうち `+` `-` `*` は、両辺が整数なら `Int`、そうでなければ
  `Num` です。`%` は両辺を整数に切り詰めてから割るので、何を渡されても `Int`
  です。`/` と `**` は常に `Num` で、`2 / 4` も `2 ** -1` も整数ではないから
  です。`Bool` と `undef` は整数として数えます（`0` / `1` / `''` / `undef` は
  どれも整数に数値化されます）。
- (INFER-1b) どちらかの辺が `Unknown` なら、答えも `Unknown` です。`Num` は
  主張であり、その主張がぶつかる先は `Int` のスロットだからです。

### 4.2 コンストラクタ (INFER-2)

- (INFER-2a) `Foo->new(...)` は `InstanceOf['Foo']` です。ただし camello が
  実際に `Foo` の `sub new` を読めたときに限ります。読めなかったクラスは
  `Unknown` のままです。
- (INFER-2b) `Returns:` があればそれが勝ちます。`URI->new` のように自分と違う
  クラスを返すコンストラクタは、そう書けば正しく伝わります。
- (INFER-2g) 手書きの `sub new` について (INFER-2a) が効くのは、**本体が「返す
  ものは自分のクラスのものだ」と言っているとき**だけです。根拠は本体にある
  `bless`（クラスは何であれ。`bless $self, $class` は継承のために書かれた
  コンストラクタが自分のクラスを綴る書き方です）か、`SUPER::`（親の
  コンストラクタを借りて、親が bless したものを受け取るもの）です。どちらも
  無い `new` はファクトリでもありえます。`URI::new` は
  `return $impclass->_init(...)` で終わって `URI::http` を返すので、それを
  `URI` と読むと以降のメソッドがすべて `unknown-method` になります。
  宣言だけを読むこのパスが本体を見る二つ目の場所で、引数リスト（ANNOT-3）と
  同じく**そのサブルーチンについての事実**であり、プログラムについての事実では
  ありません。本体が**空**のときは、宣言だけがあるときと同じく何も言っていない
  ものとして扱います。スタブ（ANNOT-9）は `sub new ($class, $fields = undef) {}`
  と書くもので、空の本体は規約であって証拠ではないからです。
- (INFER-2c) フレームワーク（Moose 系、`Class::Accessor::Typed`）が生成する
  `new` は、そのクラスの属性からなる `Dict` を受け取り、そのクラスのインスタンスを
  返します。宣言されていない鍵は `unknown-key` です。クラスかその祖先に
  `BUILDARGS` があると、この引数検査は止まります。
- (INFER-2d) `bless EXPR, 'Foo'` は `InstanceOf['Foo']`。`bless EXPR, $class` は、
  `$class` がそのサブルーチンの invocant なら、その `bless` が書かれている
  パッケージのインスタンスです。
- (INFER-2e) `bless` は**第一引数の変数の型を書き換えます**。クラスが読めない
  `bless` の後は、その変数は `Unknown` になります（もう誰にも分からないからです）。
  親のコンストラクタを借りてから自分のクラスに bless し直す書き方は、これで
  正しく追えます。
- (INFER-2f) 必須のスロットを渡していない呼び出しは `missing-argument` です。
  どの規則で「必須」かはフレームワークごとに違います
  （[DIAG-13](#73-missing-argument-について)）。
- (INFER-2g) `Class::Accessor::Lite` 一族の `new` は**開いています**。渡された
  ハッシュをそのまま bless するだけなので、アクセサのない鍵も
  `$self->{key}` として読める正しいプログラムでありえます。インスタンスの型は
  分かり、知らない鍵は `warning` として言いますが、足りない鍵は言いません
  （[ANNOT-10d](#310-classaccessorlite-一族-annot-10)）。

### 4.3 変数 (INFER-3)

- レキシカルの型は、その使用に到達するすべての代入の合併です。文の順に計算し、
  分岐は合併され、ループは本体の合併に広げられます。
- 再代入で型が完全に変わるのは構いません。Perl のコードは実際にそうします。
- パスに関する感度はありません（NARROW を除く）。片方の分岐で `$x = undef` して
  もう片方でメソッドを呼ぶコードは、たとえ実行時に安全でも `maybe-deref` になります。
- (INFER-3a) `my ($self, %args) = @_;` や `my $x = shift;` は引数リストが
  読み取られた文そのものなので、そこで束縛済みの型を上書きしません。

### 4.4 呼び出し (INFER-4)

- `Returns:` を持つサブルーチンの呼び出しは、その型を返します。
- `Unknown` なサブルーチン、解決できない裸名、`&$code`、動的なメソッド名は
  `Unknown` を返します。
- (INFER-4a) `Returns:` のないサブルーチンの戻り値は、**本体から読み取られます**
  （[return-inference.md](return-inference.md)）。読み取られた型は書かれた型と
  まったく同じように使われ、違うのは二点だけです。hover が `-> Str (inferred)` と
  出どころを添えること、そして `--strict-annotations` を満足しないこと
  ——- あのオプションは「書かれていること」を求めるためにあります。
  リストコンテキストの側は [INFER-6b](#46-コンテキスト-infer-6) を参照。
- 組み込み関数はスカラーコンテキストで次を返します。ここにないものは `Unknown` です。

  | 組み込み | 型 |
  | --- | --- |
  | `length` `index` `rindex` `ord` `int` `time` `fileno` `system` | `Int` |
  | `abs` `sqrt` `atan2` `sin` `cos` `exp` `log` `rand` | `Num` |
  | `lc` `uc` `lcfirst` `ucfirst` `chr` `sprintf` `join` `substr` `quotemeta` `ref` | `Str` |
  | `defined` `exists` `wantarray` `eof` | `Bool` |

- (INFER-4b) `scalar` は**引数を見ます**。配列・ハッシュ・スライスとそれらへの
  デリファレンス（`@$x` / `%$x` / `${...}` / `$x->@*`）に対しては個数、つまり
  `Int` です。それ以外に対しては、その式をスカラーコンテキストで見た型
  ——- ここにあるすべての型がすでにそれです —— をそのまま返します。
  `scalar $sth->bind` は `Int` ではありません。
- (INFER-4c) `keys` と `values` は**コンテキストで答えが変わる**ので、この表には
  ありません。スカラーコンテキストでは `Unknown` です。リストリテラルの要素
  としてだけは答えが決まっていて（[INFER-6c](#46-コンテキスト-infer-6)）、
  `[ keys %$h ]` は `HashRef` なら `ArrayRef[Str]`、`Map[K, V]` なら
  `ArrayRef[K]`、`[ values %$h ]` はそれぞれ `ArrayRef[V]` です。
  素のハッシュ `%h` の要素型は追わないので（INFER-5a）`Unknown` になります。

- (INFER-4d) **裸名の呼び出しが組み込みの名前なら、それは組み込みです。**
  同じパッケージに `sub delete { ... }` があっても `delete $h->{k}` は perl の
  `delete` であって、そのサブルーチンではありません。perl はパッケージより先に
  組み込みに行き着き、パッケージが自分のために書いたものはそれを変えません。
  組み込みを上書きする方法として perlsub が挙げているのは**インポート**だけなので、
  インポートされた名前なら今までどおりそのサブルーチンに解決します
  （それを許さない組み込みもありますが、それを決めるのは perl です）。
  `Foo::delete(...)` のような修飾名と `$obj->delete(...)` は、どちらも
  最初から組み込みではありません。

- (INFER-4e) 読み取りの単位は**サイト**、つまり値がサブルーチンから出ていく場所です。
  戻り値は**全サイトの join で、一つでも `Unknown` なサイトがあれば全体が
  `Unknown`** です。これは精度の選択ではありません。「型が付いた二つのサイトだけ
  から `Str`、三つ目は無視」は**プログラムが持っていない型**であり、それが
  すべての呼び出し位置で報告されることになります。

  スカラーとリストの二つの側は**独立に** join されます。`return @x` はスカラー側
  だけを沈め、リスト側は沈めません。逆に、スカラー型はあるがリストの形が無い
  呼び先を返す `return $obj->maybe` はリスト側だけを沈めます。

  | サイト | スカラーの型 | リストの形 |
  | --- | --- | --- |
  | `return EXPR` | `EXPR` をスカラーコンテキストで見た型 | `EXPR` の形（INFER-6b） |
  | `return;` / `return ()` | `Undef` | 長さ 0 |
  | `return undef` | `Undef` | 長さ 1 |
  | `return (A, B)` `return @x` `return %h` | `Unknown`（個数を `Int` と言えば `my $rows = $self->rows` を `Int` と保証することになり、それは型ではなくバグです） | `(A, B)` / `(T ...)` / `Unknown` |
  | `return wantarray ? A : B` | `B`、つまりスカラー側の枝 | `A`、つまりリスト側の枝 |
  | `die` `croak` `confess` `throw` `exit` | サイトではありません（bottom）。どちらの join にも何も足しません | 同じ |
  | **末尾**: 本体の最後の文が式文であるとき | その式の型。`sub name { $_[0]->{name} }` がアクセサの書き方の半分だからです。文修飾子 (`... if $ok`) が付いた文は、条件が成り立たないときの値が条件自身の値なので `Unknown` | 同じ式の形 |
  | 末尾が `if`/`unless`/`elsif`/`else` の連鎖 | 各枝の末尾の join。`else` のない連鎖は `Unknown`（偽の `if` の値は条件の値だから） | 同じ |
  | 末尾がループ・素のブロック・`package`・入れ子の `sub`、空の本体 | `Unknown` | `Unknown` |
  | 本体のどこかに `goto` | サブルーチン全体が `Unknown` | 同じ |

  サイトは `sub` ごとに集められ、無名サブルーチンに入ると集め直されます。
  コールバックの中の `return` はコールバックのものです。

  スロットがすべて `Unknown` の形は `Unknown` に畳まれます。`(Unknown)` は
  「値が一つ」しか言っていないのに、二層の推論は「`Unknown` でない形」を
  最終回答として扱うので、最初の周で全スロットが `Unknown` だったサブルーチンが
  二度と見られなくなってしまいます —— 後の周のほうが多くを知っているというのが
  第二層の存在理由です。長さ 0 はこれに当たりません（0 という長さは情報です）。
- (INFER-4f) **`$self` を返すサブルーチンは、書かれたクラスではなく呼ばれた
  クラスを返します。** `Base::set_x` が `return $self` で終わるとき、
  `Child->new->set_x(1)->extra` の `extra` は `unknown-method` になってはいけません
  —— これは連鎖するメソッドのもっともありふれた形です。サイトの式が invocant
  そのもの（`$self`、メソッドの第一引数の名前、`@_` を展開しないサブルーチンでの
  `$_[0]`）であるとき、あるいは `bless {...}, $class` であるとき、その型は
  `InstanceOf[自分のパッケージ]` という**目印**として記録され、呼び出し位置で
  レシーバのクラスに置き換えられます。`return $ok ? $self : undef` のように
  他のものと join されたサイトでは、`InstanceOf` の要素だけが置き換わり
  `Undef` は残ります。
  手書きの `sub new` はこの推論の対象外で、[INFER-2g](#42-コンストラクタ-infer-2)
  がそのまま答えます。
- (INFER-4g) 読み取りは**二層**です。宣言パスの中で走る**ファイル単位**の層は、
  そのファイルだけで分かること（リテラル、コンストラクタ、`bless`、自分の
  パッケージの属性、自分のサブルーチン、invocant の目印）を埋め、宣言キャッシュに
  一緒に入ります。他のファイルへの呼び出しは `Unknown` のままで、それを埋めるのが
  全ファイルが揃った後に走る**プログラム単位**の層です。どちらも単調で、
  「もう変わらなくなるまで」の繰り返し回数には上限があります。上限で切られたものは
  `Unknown` のまま、つまり黙ります。再帰・相互再帰は、どの経路も再帰を通るなら
  毎回同じ `Unknown` が返るので、呼び出しグラフを作らずに `Unknown` に落ちます。
- (INFER-4h) 書かれた `Returns:` が勝ちます（ANNOT-7a）。`Returns: ()` は
  **決して推論されません**。「何も返さない、値を使うな」は意図についての表明で、
  アノテーションだけがそれを言えます。すべてのサイトが `return;` である
  サブルーチンは、スカラーコンテキストで `Undef` を返すサブルーチンです。
  推論された戻り値が `return-mismatch` の対象になることもありません（ANNOT-7a）——
  矛盾する相手がいないからです。

### 4.5 添字 (INFER-5)

- `$x->{k}` は、`Dict` ならそのスロットの型、鍵が閉じた `Dict` に無ければ
  `unknown-key`（DIAG-6）。`HashRef[T]` なら `Maybe[T]`。`Unknown` なら `Unknown`。
- `$x->[0]` は、`Tuple` ならそのスロット、`ArrayRef[T]` なら `Maybe[T]`。
- (INFER-5a) **要素はその容れ物を名指します**。`$h{k}` が読むのは `%h` であって
  `$h` ではなく、`$a[0]` が読むのは `@a` です。矢印がある `$h->{k}` だけが
  `$h` を読みます。camello は素のハッシュ・配列の要素型を追わないので、
  `$h{k}` の型は `Unknown` です。
- (INFER-5b) **代入の左辺の添字は決して診断になりません**（オートビビフィケーション）。

### 4.6 コンテキスト (INFER-6)

- (INFER-6a) 式の型は既定で**スカラーコンテキスト**で計算されます。
- (INFER-6b) **コンテキストが書かれている場所では、リストコンテキストで読まれます。**
  その場所は四つだけで、いずれも左辺や構文がコンテキストを課しているものです。

  | 場所 | 読まれるもの |
  | --- | --- |
  | `my ($a, $b) = EXPR` / `my @a = EXPR` / `my %h = EXPR` | 右辺の**形** |
  | `[ EXPR, ... ]` の要素 | 各要素の形を平坦化したもの |
  | `foreach my $x (EXPR)` | 形の要素型 |
  | `return EXPR` | サブルーチンのリスト側の戻り値（INFER-4e） |

  形（`ListShape`）は「長さが分かっているスロットの並び」`(A, B)`、
  「任意個のひとつの型」`(T ...)`、「何もない」`()`、`Unknown` のどれかです。
  式の読み方は次のとおりで、ここに無いものはすべて「一つの値、つまり長さ 1 の
  リスト」です。

  | 式 | 形 |
  | --- | --- |
  | `(A, B)`、裸の `A, B` | スロットごとに `A`, `B`。要素自身が複数のもの（`@x`・呼び出し・`map`）が混じると、長さが分からなくなるので全体が `(join ...)` |
  | `()` | 長さ 0 |
  | `@a` `@$ref` `$ref->@*` | 要素型が分かっていれば `(T ...)`、でなければ `Unknown` |
  | `%h` `%$ref` | `Unknown`（リストコンテキストのハッシュは鍵と値の対で、それを列として欲しいものはこの先にありません） |
  | `map BLOCK LIST` | ブロックが式ひとつなら、`$_` を要素に束縛して読んだその型の `(T ...)`。でなければ `(Unknown ...)` |
  | `grep` `sort` `reverse` | `LIST` の形を `(T ...)` に広げたもの |
  | `keys` `values` | INFER-4c の答えの `(T ...)` |
  | `f(...)` `$obj->m(...)` | 呼び先の `Returns:` のリスト側。スカラー側しか書かれていない呼び先はここでは `Unknown` です —— スカラーで一つ返すことは、リストで何を返すかについて何も言いません |
  | `wantarray ? A : B` | `A`、つまりリスト側の枝 |
  | `A ? B : C` | 両者の join |

  束縛のしかた: 長さが分かっているならスロット `i` が対象 `i` に、長さを超えた
  対象は `Undef`（perl がそこに置くもの）。`(T ...)` は長さが分からないので、
  スカラーの対象は `Maybe[T]` です。末尾の `@rest` はその位置から先すべて、
  `%opts` は `Unknown` です。
- (INFER-6c) **アリティはこの読み方の消費者ではありません。** `g(f())` で `f` が
  二つ返すなら perl では引数二つですが、アリティのパスは引数を構文的に数え続けます。
  呼び出しを引数リストに平坦化するところに偽陽性が住むので、それは作りません。

### 4.7 クラスメソッドの `$class` (INFER-9)

```perl
package Base;
sub build {
    my $class = shift;          # ClassName['Base']
    my $self  = $class->new;    # InstanceOf['Base'] ——「Self」
    return $self;
}
package Child;
our @ISA = ('Base');
Child->build->extra;            # extra は Child のもの。通る。
```

`$self` は `InstanceOf[自分のパッケージ]` に束縛される（INFER-3a）のに、`$class`
は長らく「どこかのクラスの名前」でしかありませんでした。`$class->` が何も解決
できないので、`my $self = $class->new` の右辺も `Unknown` になり、手書きの
コンストラクタとクラスメソッドの本体が丸ごと見えていませんでした。

- (INFER-9a) **`$class` は `ClassName['自分のパッケージ']` です。** これは仮定
  です —— `$class` が実際に何であるかは呼び出し側が決めます。置いている前提は
  「`$class` は `__PACKAGE__` かそのサブクラスである」で、クラスメソッドの呼び
  出し規約がそうなっている以上、破れているならそれはコードの側の間違いです。
  `$class->m` は自分のパッケージの MRO で解決し、見つかった型を使います。

  **見つからなければ `unknown-method` です。** サブクラスが足したメソッドを基底
  から呼ぶ形（テンプレートメソッド）はこの前提の外にあり、誤検知になります。
  それを承知で報告する側を選んでいます。pyright / mypy が classmethod の `cls` を
  `type[Self]` として同じ扱いをするのと同じ判断で、@INC のコーパス 2564 ファイル
  で実際に誤検知になったのは 1 件でした（`TheSchwartz::Worker` の `$class->work`）。

  `ClassName['Foo']` はアノテーションにも書けます。パラメータのない `ClassName`
  は従来どおり「どこかのクラスの名前」で、`ClassName['Child']` はその部分型です。

- (INFER-9b) **`$class` から作った値は Self です。** INFER-4f が invocant その
  ものについて言っていることを、invocant の *クラス* から作られた値まで広げます。
  `$class->new`、`$self->clone` のように、レシーバが invocant で、返る値が
  レシーバのクラスのインスタンスであるとき、その値には invocant の目印が付き、
  呼び出し位置でレシーバのクラスに置き換わります。`Child->build` は `Child` です。

  目印は**一度だけレキシカルを渡ります**。`my $self = $class->new; ...; return
  $self` は手書きのコンストラクタの標準形で、目印が式にしか付かないなら代入で
  失われるからです。同じレキシカルへの別の代入は目印を外します。値の型が
  レシーバのクラスでないもの（`$class->config` が `Config` を返す）は対象外です。

## 5. 絞り込み (NARROW)

`Maybe[T]` を値として使うと `maybe-deref`（DIAG-14）になります。それを消すのが
絞り込みです。**定理ではなく一覧**であり、下にあるものだけが効きます。

- (NARROW-1) `defined $x` / `blessed $x` / `ref $x` / `exists $h{k}` を条件に
  持つ `if` の中では、その引数から `undef` が外れます。`defined $x->{k}` は
  `$x` も絞り込みます。
- (NARROW-2) `if ($x)` — 真偽の検査そのものも `undef` を外します。
- (NARROW-3) `$x->isa('Foo')` は `$x` を `InstanceOf['Foo']` にします。
- (NARROW-4) `$x // $default` と `$x || $default` の結果からは `undef` が外れます。
- (NARROW-5) **ガード文**: `return` / `die` / `croak` / `confess` / `next` / `last`
  を含む文が `unless COND` / `COND or LEAVE` と書かれていればそれ以降で COND が、
  `if COND` と書かれていればそれ以降で COND の**否定**が成り立ちます。読むのは
  条件の部分だけで、文の残りではありません。

```perl
my $row = find_row();       # Maybe[InstanceOf['Row']]

return unless defined $row; # NARROW-5
print $row->id;             # 診断なし
```

### 5.1 条件は木として読む (NARROW-6)

上の一覧は条件式の**構造に沿って**適用されます。条件は木であり、`!` も `||` も
「中身を読まなかった呼び出し」も、その部分が言っていることを変えるからです。
以前は条件の全ノードを平坦に舐めて、**そこに現れた変数を無条件に絞り込んで**
いました。`if (!$x) { $x->foo }` のように、実行すれば必ず死ぬコードが安全と
判定されていました。

- (NARROW-6a) `!E` は真側と偽側を入れ替えます。したがって `!$x` の中では
  何も絞り込まれません。
- (NARROW-6b) `A && B` の真側では両方が成り立ちます。偽側はどちらが偽だったか
  分からないので何も言いません。`A || B` はその逆で、真側が何も言いません。
- (NARROW-6c) 一覧に無い呼び出しは**何も言いません**。`validate($x)` が
  defined 検査なのかその逆なのかは、このパスが開いていない本体の話です。
- (NARROW-6d) **評価されただけで分かること**は、条件の真偽に関わらず成り立ちます。
  `!$x->name` は `!` が否定する前に呼び出しを済ませているので、`$x` は
  どちらの枝でも `undef` ではありません。
- (NARROW-6e) perl は短絡するので、右辺は**左辺が言ったことの下で**読まれます。
  `$x && $x->foo` の呼び出しは `$x` があるところでしか走らず、
  `!$x || $x->foo` も同じです。これを読まずに左から順に型を付けていたときは、
  この書き方そのものが `maybe-deref` になっていました。
- (NARROW-6f) `unless` はその条件が成り立た**なかった**ところで本体を走らせるので、
  条件が言うことは `else` のものです。`elsif` は自分の条件で自分のブロックを
  絞り込みます。
- (NARROW-6g) 比較のように**両辺とも評価される**形は、どちらが言うことも通します。
  `ref $x eq 'HASH'` がこの一覧にある理由の半分はこれです。厳密には
  `ref $x eq ''` が真でも `$x` は `undef` でありえますが、ここは定理ではなく
  一覧なので、実際に書かれる形を採ります。

## 6. メソッドの解決 (METHOD)

- (METHOD-1) `isa`（`use parent` / `use base` / `extends` / `our @ISA`）と
  ロール（`with`）を畳み込んだ深さ優先で探します。
- (METHOD-2) `UNIVERSAL` のメソッド — `isa` `can` `DOES` `VERSION` — と
  `import` / `unimport` / `DESTROY` は、どのクラスにもあるものとして扱われます。
- (METHOD-3) `$self->SUPER::foo(...)` は、**その行が書かれているパッケージ**の
  親から探します。invocant が何であったかとは無関係です。
  `$obj->Other::foo(...)` は `Other` から探します。
- (METHOD-4) 属性のアクセサと、`reader` / `writer` / `predicate` / `clearer` で
  名付けられたメソッドも見つかります。
- (METHOD-4a) 属性が答える名前のすべてが属性の型を返すわけではありません。
  アクセサ本体と `reader`、`writer` は属性の型を返しますが、`predicate` は
  スロットが埋まっているかどうかなので `Bool`、`clearer` と `handles` の委譲先は
  ここからは読めないので `Unknown` です。全部を属性の型として返していたときは、
  `ArrayRef[Int]` の属性に対する `$obj->has_items` が `ArrayRef[Int]` になり、
  文字列のスロットに渡すたびに `type-mismatch` が出ていました。
- (METHOD-4c) 属性が生やすメソッドは**引数リストを持つ普通の呼び出し**として
  検査されます。アクセサと `reader` / `predicate` / `clearer` は invocant だけ、
  `writer` と `wo` のアクセサは値を一つ取り、`rw` のアクセサはそれを省略できます。
  `handles` の委譲先は他のクラスのものなので `Unknown` です。値の型は
  そのスロットの型（[ANNOT-2a](#32-has-annot-2) の入力側）です。属性を
  「型」としてだけ持っていたときは、`$obj->set_count([1, 2])` を `Int` の
  スロットに対して照合する材料がそもそも無く、同じ内容を普通のサブルーチンで
  書いたときだけ検査されていました。`ro` の属性に値を渡す、`wo` の属性を読む、
  といったことも同じ経路で分かります。
- (METHOD-4b) そのパッケージが `use` したモジュールの `@EXPORT` にある名前も
  見つかります（[METHOD-6](#62-インポートされたサブルーチン-method-6)）。
  クラス自身の宣言を先に見てから探します。

### 6.1 不透明なクラス (METHOD-5)

次のいずれかに当てはまるクラスは、**どんなメソッドを持っていてもおかしくない**ため、
`unknown-method` を言いません。

- (METHOD-5a) camello が一度も見ていないパッケージ、またはその祖先に一つでも
  そういうパッケージがあるもの。
- (METHOD-5b) `AUTOLOAD` を持つもの。
- (METHOD-5c) `handles` に正規表現やロール名を渡している属性を持つもの、
  または知らないロールを消費しているもの。
- (METHOD-5d) そのパッケージを宣言しているファイルが XS を読み込んでいるか
  （`XSLoader` / `DynaLoader` / `bootstrap`）、グロブに代入しているか、
  `@ISA` に実行時に決まる値を代入しているもの。ファイル単位で効きます。
- (METHOD-5e) 名前が上位のパッケージが (METHOD-5d) に当てはまるもの。XS は
  ディストリビューションの名前空間にメソッドを登録し、名前空間とは名前の
  前置部分だからです（`Net::DBus` が `XSLoader::load` を呼び、メソッドは
  `Net::DBus::Binding::Iterator` に生えます）。
- (METHOD-5f) サブルーチンも属性も一つも宣言していないもの。
- (METHOD-5g) **コード生成器を呼んでいるもの**。グロブ代入をする、あるいは
  名前のリストが読めないアクセサ生成子（`mk_accessors` 一族）を呼ぶ本体を
  持つファイルは (METHOD-5d) により「メソッドを読めない手段で作る」ファイル
  ですが、そこで作られるメソッドが生えるのは**呼び出した側**のパッケージです。
  そこでファイルスコープで書かれたメソッド呼び出し（`__PACKAGE__->mk_fields`、
  `Some::Util->ro_datetime([...])`）の解決先がそういうファイルのサブルーチン
  であるとき、呼び出した側のパッケージも不透明になります。生成器はロード時に
  走らなければならないので、見るのはファイルスコープの呼び出しだけです。

### 6.2 インポートされたサブルーチン (METHOD-6)

`use Exporter 'import'` と `our @EXPORT` は、名前を**インポートした側の
パッケージ**に置きます。置かれた名前はそのパッケージのサブルーチンなので、
`$obj->name` はそれを見つけます。

- (METHOD-6a) `@EXPORT` の値が読めないとき — `our @EXPORT = get_public_functions;`
  のように式であるとき —、そのモジュールを `use` したパッケージは、名前を
  列挙できないメソッドを持つことになるので不透明です（METHOD-5）。
- (METHOD-6b) 逆に、`@EXPORT` を持つパッケージ**自身**も不透明です。自分の
  サブルーチンを他所へ配っているミックスインであり、その中の `$self` は
  インポートした側のクラスであって、このパッケージのインスタンスではないから
  です。
- (METHOD-6c) `&name` はサブルーチンを長く書いたものです。`$name` / `@name` /
  `%name` は変数で、メソッドにはならないので読み飛ばされます。リストがそれら
  以外の式を含んでいるときだけ (METHOD-6a) になります。

## 7. 診断 (DIAG)

すべての診断は安定したコードを持ちます。出力は
`path:line:col: severity: message [code]` の一行で、`--format json` も選べます。

重大度の区別は、`error` が**宣言された二つのものの矛盾**、`warning` が片側が
**推論**であるか絞り込みに依存しているもの、`info` がユーザーが求めたときに
知らされるものです。`--error-on` に達したものがあれば終了ステータスは 1 です。

| コード | 既定 | 意味 |
| --- | --- | --- |
| (DIAG-1) `undeclared-variable` | error | `strict` の下で、どの宣言も届かない名前 |
| (DIAG-2) `unused-variable` | warning | 宣言されて一度も読まれないレキシカル |
| (DIAG-3) `shadowed-variable` | warning | 外側のスコープが既に束縛している名前 |
| (DIAG-4) `arity` | error / warning | 引数の個数が引数リストを満たせない |
| (DIAG-5) `type-mismatch` | error / warning | 値の形が、入る先の宣言された型と矛盾する |
| (DIAG-6) `unknown-key` | error | 閉じた `Dict` に無い鍵、宣言のない属性 |
| (DIAG-7) `unknown-method` | warning | そのクラスが宣言していないメソッド |
| (DIAG-8) `bad-annotation` | info | 読めないアノテーション |
| (DIAG-9) `return-mismatch` | error / warning | `Returns:` と食い違う `return` |
| (DIAG-10) `missing-annotation` | info | 公開サブルーチンに何のアノテーションもない |
| (DIAG-11) `unknown-type` | info | どこも宣言していない型名・クラス名 |
| (DIAG-12) `unused-parameter` | info | 本体が一度も読まない引数 |
| (DIAG-13) `missing-argument` | error | 必須の名前付き引数を渡していない呼び出し |
| (DIAG-14) `maybe-deref` | info | 絞り込みを経ずに使われた `Maybe[...]` |
| (DIAG-15) `ignored-prototype` | info | `()` と宣言されたサブルーチンへのメソッド呼び出し |

### 7.1 重大度が動くもの

- (DIAG-4a) `arity` は、シグネチャと `args` に対しては `error` です。perl と
  Smart::Args が実行時に die するからで、静的に言っても誤検知になりません。
  `@_` の展開から読んだリストに対しては `warning` です。そちらは規則ではなく
  形であり、プログラムは動くからです。属性が生やしたメソッド（METHOD-4c）に
  対しても `warning` です。形はフレームワークのものであって書き手のものではなく、
  要らない引数をどう扱うかもフレームワーク次第だからです（Moose の reader は
  黙って無視し、`Class::Accessor::Lite` のものは croak します）。裸の `()` を
  持つサブルーチンへの**メソッド呼び出し**だけは例外で、`arity` ではなく
  DIAG-15 になります。
- (DIAG-5a) `type-mismatch` は、値がリテラルのとき `error` です（両側が
  書かれているからです）。推論された値のときは `warning` です。
- (DIAG-6a) `unknown-key` は、コンストラクタが**開いている**とき `warning` です。
  `Class::Accessor::Lite` の `new` は渡されたハッシュをそのまま bless するので、
  アクセサの無い鍵も `$self->{key}` として読める正しいプログラムでありえます
  ([ANNOT-10d](#310-classaccessorlite-一族-annot-10))。鍵を拒否するコンストラクタに
  対しては、これは宣言された二つのものの矛盾なので `error` のままです。
- (DIAG-9a) `return-mismatch` も同じ規則に従います。無名 sub の中の `return` は
  その無名 sub のもので、無名 sub には注釈を書く手立てがないので、書かれている
  サブルーチンの `Returns:` は何も言いません。
- (DIAG-14a) `maybe-deref` は `info` です。`$ary->[0]` も `$h->{k}` も
  構造的に `Maybe` であり、この診断が指すコードは**イディオムそのもの**で、
  その大半は正しく動いています。求められたときに知らされるもので、読む人に
  押しつけるものではありません。絞り込みの一覧（NARROW）は定理ではないので、
  一覧に無い書き方で守られているコードも必ずここに出ます。
- (DIAG-15a) `ignored-prototype` は `info` です。perlsub の "Prototypes" は
  「メソッド呼び出しはプロトタイプの影響を受けない。呼ばれる関数がコンパイル時に
  決まらないからだ」と言っています。`sub foo() {}` の `()` は、signatures 機能が
  無効なら**プロトタイプ**、有効なら**シグネチャ**で、camello はどちらかを判別
  できません。プロトタイプだったなら `Foo->foo` は何事もなく通り（invocant が
  `$_[0]` に入るだけ）、シグネチャだったなら実行時に die します。どちらだと
  言うのも推測なので、`arity` の error ではなく、呼び出しにつき一度 `info` で
  知らせるだけにします。
- (DIAG-15b) 裸の名前による呼び出し（`Foo::foo(1)`）はプロトタイプの影響を
  **受ける**ので、そちらは今までどおり `arity` です。`()` を持ちながら本体が
  `@_` を読むサブルーチンは、シグネチャではありえない（perl が本体を到達不能に
  する）のでプロトタイプだと確定でき、何も言いません。
- (DIAG-x) パーサの推測（[architecture.md](architecture.md) の `GUESS:`）に
  依存する診断は一段下げて報告されます。

### 7.2 `unused-variable` と `unused-parameter`

読まれない名前が二つのコードに分かれているのは、止めたい理由が別だからです。

- (DIAG-2a) `class` 機能の `field` は、そのクラスのすべての `method` から見える
  宣言です。属性の付いた `field` は本体の外に名前を渡しています（`:param` は
  コンストラクタへ、`:reader` は perl が生成するアクセサへ）ので、本体が読まな
  くても何も言いません。属性の無い `field` は普通のレキシカルと同じです。
- (DIAG-12a) **引数はシグネチャです。** `sub f ($self, $format, $indent)` の
  `$indent` を本体が読まなくても、その名前は呼び出し側に何を渡すかを言い続けます。
  消せば呼び出しが壊れるので、「宣言されて読まれない」とは別のことです。既定が
  `info` なのはそのためで、既定の `--error-on error` では CI を落としません。
  うるさければ `disable = ["unused-parameter"]` で丸ごと止められます。
- (DIAG-12b) 引数とみなされるのは、シグネチャの引数、`args` / `args_pos` の項目、
  そして `my (...) = @_` と `my $x = shift` / `my $x = shift @_` で束縛された名前です
  ([ANNOT-6](#36-_-の展開-annot-6))。`my $x = shift @list` はリスト操作であって
  引数ではありません。
- (DIAG-12c) `catch ($e)` は構文が束縛するもので、本体が要求したものではないので、
  どちらのコードでも報告されません。`foreach my $x` は普通のレキシカルです。
- (DIAG-12d) **デストラクタのために持たれている値**は、どちらでも報告されません。

```perl
my $guard = Scope::Guard->new(sub { $lock->release });   # 読まれなくて当然
```

  判断の根拠は名前ではなく**何が作ったか**です。`Scope::Guard` と `Guard` の
  コンストラクタ、および `guard` / `scope_guard` / `SCOPE_GUARD` という名前の
  呼び出し（`Guard::guard { ... }` のような修飾付きも含む）がそれにあたります。
  プロジェクト自身のガードクラスは `camello.toml` の `guard-classes` に書きます。

### 7.3 `missing-argument` について

呼び出しが必須の名前を渡していないというものです。名前付きの引数リストにだけ
効きます（位置引数の個数は `arity` の仕事です）。

- (DIAG-13a) 必須かどうかは**フレームワークごとに違い**、camello はそれぞれの
  規則に従います。Moose 系は `required => 1` と書いたときだけ必須、
  `Class::Accessor::Typed` は逆で、`optional` でも `default` でも lazy でもない
  スロットが必須です（[ANNOT-4a](#34-classaccessortyped-annot-4)）。
  `Smart::Args` は `optional` / `default` / `builder` が無ければ必須です。
- (DIAG-13b) `error` なのは、どれも**実行時に die する**からです。Moose も
  Smart::Args も `Class::Accessor::Typed` も、足りない引数でコンストラクタや
  サブルーチンを呼ぶとそこで止まります。
- (DIAG-13c) 報告は**呼び出しにつき一回**で、足りない名前を並べます。直すべきものが
  引数リストという一箇所だからです。
- (DIAG-13d) 引数リストが**読めないときは何も言いません**。`Foo->new(%args)`、
  `Foo->new($args)`、`Foo->new({ ... })` のように書かれた鍵が一つも無いものは、
  数えられるリストではありません。`BUILDARGS` があるクラスと、祖先に未知の
  パッケージがあるクラスも同じく対象外です。
- (DIAG-13e) ある名前が必須なのは、その名前の**すべての宣言**が必須と言っている
  ときだけです。`has '+name' => (default => 'x')` は継承した属性を埋め直すもので、
  親の `required => 1` はもう最後の言葉ではありません。
- (DIAG-13f) 開いたコンストラクタ（[ANNOT-10d](#310-classaccessorlite-一族-annot-10)）は
  何も必須にしません。渡されたものを見ないので、足りないと気づきようがないからです。

### 7.4 `unknown-method` について

これは `warning` です。クラスの側が正しくて、その値がそのクラスだという
camello の判断の方が間違っている可能性が常にあるからです。基底クラスが
サブクラスの実装するメソッドを呼ぶ書き方（テンプレートメソッド）では、
`$self` は「このクラスまたはその任意のサブクラス」であり、サブクラスは
何でも定義できます。この場合の報告は現状では誤検知です。

## 8. 診断を止める (OFF)

### 8.1 行ごと (OFF-1)

```perl
my $thing = $legacy->whatever;   ## camello-disable: unknown-method
```

マーカーが指すのは**一行**であって、文でもサブルーチンでもありません。
診断が報告されている行に届く位置に置いてください。

- コードと同じ行にあるマーカーはその行についてのものです。
- **自分の行に単独で**あるマーカーは、**その真下の一行**についてのものです。
  長い行や、診断の位置がコメントそのものである場合（読めない `Returns:` など）は
  こちらを使います。`sub` の上に置いても、本体の中で報告される診断
  （`return-mismatch` など）には届きません。

```perl
# Returns: Int
sub f {
    ## camello-disable: return-mismatch
    return 'nope';                       # 直上のマーカーが届く
}

# Returns: Int
sub g {
    return 'nope';  ## camello-disable: return-mismatch
}
```

- カンマ区切りで複数のコードを書けます。
- コードを一つも書かない `## camello-disable:` は、その行のすべてを止めます。
- `#` ではなく `##` なのは、`## no critic` と衝突しないためです。コメントなので
  `camello format` が壊すこともありません。

### 8.2 プロジェクトごと (OFF-2)

コマンドを実行したディレクトリの `camello.toml` を読みます。

```toml
[check]
lib = ["lib", "t"]           # パスを指定せずに実行したときの対象
stubs = ["stubs"]            # スタブのディレクトリ
disable = ["unused-variable"]
error-on = "warning"         # これ以上の重大度があれば終了ステータスは 1
min-severity = "warning"     # これ未満の重大度は印字しない
guard-classes = ["My::Lock"] # デストラクタのために持たれる値を作るクラス
strict-annotations = true

[check.read-as]                          # 自作ラッパーが何の代わりか (ANNOT-12)
"My::Accessors" = "Class::Accessor::Typed"
```

`min-severity` が落としたものは**丸ごと**落ちます。集計にも数えられず、終了
ステータスも決めません。誰にも見せていない診断で実行を失敗させることはないからです。
`--min-severity error` はエラーだけを印字します。

コマンドラインのフラグが設定ファイルより優先されます
（ファイルはプロジェクトが何であるかを、フラグはこの実行が何であるかを言います）。
読めない設定ファイルはエラーです。黙って無視される設定は、誰も頼んでいない
規則でプロジェクトを検査することになるからです。

## 9. 依存モジュール (DEPS)

`use Foo::Bar` は次の順で解決されます。

1. (DEPS-1) **プロジェクトのルート** — コマンドが指されたディレクトリ。全体が解析され、
   診断の対象になります。
2. (DEPS-2) **スタブのルート** — `--stubs`。宣言だけを与え、下のすべてを覆います。
3. (DEPS-3) **`PERL5LIB` と `PATH` 上の perl の `@INC`** — 宣言だけ。この一覧は
   一回だけ読まれます（これが camello が perl を呼ぶ唯一の場所で、プロジェクトを
   実行するわけではありません）。`--inc` で置き換えられます。
4. (DEPS-4) **どこにもない** — そのパッケージは `Unknown` になり、それを使う
   すべての場所で沈黙します。

- (DEPS-5) **ルートの外のファイルに対して診断が出ることは決してありません。**
- (DEPS-6) 依存モジュールについての宣言はディスクにキャッシュされます
  （既定で `.camello-cache/`、`--no-cache` で無効）。パス・サイズ・更新時刻・
  内容のハッシュで鍵付けされます。
- (DEPS-7) 小文字で始まるモジュール名（perl のプラグマの慣習）は解決されません。
- (DEPS-8) `require Foo::Bar` も `use` と同じように追跡されます。

## 10. 意図的にやらないこと (LIMIT)

- (LIMIT-1) **健全性**。camello は分からないときに黙ります。見逃しはあります。
- (LIMIT-2) **実行**（POLICY-5）。
- (LIMIT-3) **強制変換**（ANNOT-2a）。
- (LIMIT-4) **依存モジュールの検査**（DEPS-5）。
- (LIMIT-5) **パス感度**（INFER-3）。
- (LIMIT-6) **`return @rows` や `return (A, B)` のスカラー側**（INFER-4e）。
  個数でも最後の要素でもなく `Unknown` です。リスト側は読まれます。
- (LIMIT-7) **リストコンテキストのうち読まないもの**（INFER-6b）。
  ハッシュを鍵と値の対として読むこと、スライスの要素型、`$a[0]` のような
  素の配列の要素（INFER-5a）、そしてアリティ（INFER-6c）。

---

このドキュメントは実装とフィクスチャに基づいています。ここに書かれていない
振る舞いを見つけた場合、あるいはここに書かれていることと実装が食い違う場合は、
GitHub Issues で報告してください。設計上の判断とその根拠は
[typecheck.md](typecheck.md) にあります。
