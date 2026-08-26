# Perl Type Checking Specification

このドキュメントは、`camello lint` と `camello typecheck` が Perl のコードについて
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
  **ファイル内で健全**です。これらは `camello lint` が型を一切使わずに報告します。

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

### 2.5 `Str` と数、`Bool` (TYPE-5)

- (TYPE-5a) Perl の値が実際に持っている部分型関係、`Int <: Num <: Str` を採ります。
  数を `Str` のスロットに渡すのは適合します。
- (TYPE-5b) 数に見える文字列リテラルは数です。`"3"` は `Int`、`"1.5"` は `Num`、
  `"abc"` は `Str` です。したがって `Int` のスロットに `"abc"` を渡すのは
  `type-mismatch` ですが、`"3"` は通ります。
- (TYPE-5c) `Bool` は名前的に別の型です。`Bool` のスロットに `Int` を渡すことも、
  その逆も適合します。camello は**値**を追いません（`2` を `Bool` に渡しても
  何も言いません）。形だけを見ます。

## 3. アノテーションの読み取り (ANNOT)

### 3.1 どれもインポートで裏付けられている (ANNOT-1)

`has` や `args` を認識するのは、**その名前を提供しうる `use` がファイルにあるとき
だけ**です。自作の `sub has` が Moose の `has` と取り違えられることはありません。

| 認識するもの | 必要な `use` |
| --- | --- |
| `has` | `Moose`, `Moo`, `Mouse`, それぞれの `::Role`, `Mojo::Base` ほか |
| `args` / `args_pos` | `Smart::Args`, `Smart::Args::TypeTiny` |
| `rw`/`ro`/... の宣言 | `Class::Accessor::Typed` |
| `declare` / `class_type` / ... | `Type::Library`, `Type::Utils`, `MooseX::Types` |

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
- (ANNOT-2a) `coerce => 1` のスロットは `Unknown` になります。宣言された型は
  **強制変換後**の値の上限であって、変換関数は camello から見えないからです。

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
- 残りはスカラーコンテキストの型、`list: (<type>, ...)` でリストコンテキストの形、
  その両方を `|` で繋いだもの、または「何も返さない」を意味する `()` です。
- 属性 (`sub f :Returns(Str)`) ではなくコメントなのは、どの perl でも、どんな
  属性ハンドラの下でも足せる必要があるからです。`camello format` はコメントを
  一バイトも変えないので（[contracts.md](contracts.md) の `comments` 不変条件）、
  整形で壊れることもありません。
- (ANNOT-7a) `Returns:` とその推論された戻り値が食い違うとき、**アノテーションが
  勝ち**、推論された形の方が `return` の位置で報告されます（DIAG-9）。
- (ANNOT-7b) 読めない `Returns:` は診断になります（DIAG-8）。黙って無視される
  アノテーションは、無いより悪いからです。
- (ANNOT-7c) ただし、型の**形をしていない**ものは散文として扱われ、何も言いません。
  `# Returns:    modified template` のような行はアノテーションではありません。
  「括弧の外に裸の名前が二つ並んでいる」ものは散文です。

### 3.8 型ライブラリ (ANNOT-8)

プロジェクト自身の `Type::Library` から、次の形だけ読みます。

```perl
declare 'PositiveInt', as Int, where { $_ > 0 };   # Int の部分型
declare 'Handle', as InstanceOf['IO::Handle'];
class_type 'User', { class => 'MyApp::User' };     # InstanceOf
role_type 'Loggable';                              # ConsumerOf
enum 'Color', [qw(red green blue)];                # Enum
union 'Id', [Int, Str];                            # Int | Str
```

`as T` が親を与え、`where` は無視されます。`as` を持たない `declare` は `Any` です。

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

### 4.2 コンストラクタ (INFER-2)

- (INFER-2a) `Foo->new(...)` は `InstanceOf['Foo']` です。ただし camello が
  実際に `Foo` の `sub new` を読めたときに限ります。読めなかったクラスは
  `Unknown` のままです。
- (INFER-2b) `Returns:` があればそれが勝ちます。`URI->new` のように自分と違う
  クラスを返すコンストラクタは、そう書けば正しく伝わります。
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
- (INFER-4a) 戻り値の推論は**サブルーチンの境界を越えません**。`Returns:` のない
  サブルーチンの戻り値は `Unknown` です。
- 組み込み関数はスカラーコンテキストで次を返します。ここにないものは `Unknown` です。

  | 組み込み | 型 |
  | --- | --- |
  | `length` `index` `rindex` `ord` `int` `time` `fileno` `system` `scalar` `keys` `values` | `Int` |
  | `abs` `sqrt` `atan2` `sin` `cos` `exp` `log` `rand` | `Num` |
  | `lc` `uc` `lcfirst` `ucfirst` `chr` `sprintf` `join` `substr` `quotemeta` `ref` | `Str` |
  | `defined` `exists` `wantarray` `eof` | `Bool` |

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

- (INFER-6a) 式の型はすべて**スカラーコンテキスト**で計算されます。
- (INFER-6b) `Returns:` の `list:` の部分は構文検査こそされますが、
  **まだ照合には使われていません**。使われるのは `Returns: ()`
  （何も返さない）だけです。

## 5. 絞り込み (NARROW)

`Maybe[T]` を値として使うと `maybe-deref`（DIAG-7）になります。それを消すのが
絞り込みです。**定理ではなく一覧**であり、下にあるものだけが効きます。

- (NARROW-1) `defined $x` / `blessed $x` / `ref $x` / `exists $h{k}` を条件に
  持つ `if` の中では、その引数から `undef` が外れます。`defined $x->{k}` は
  `$x` も絞り込みます。
- (NARROW-2) `if ($x)` — 真偽の検査そのものも `undef` を外します。
- (NARROW-3) `$x->isa('Foo')` は `$x` を `InstanceOf['Foo']` にします。
- (NARROW-4) `$x // $default` と `$x || $default` の結果からは `undef` が外れます。
- (NARROW-5) **ガード文**: `return` / `die` / `croak` / `confess` / `next` / `last`
  を含む文が、`unless` または `or` / `||` と一緒に書かれているとき、その文が
  言及している変数は、それ以降で絞り込まれます。

```perl
my $row = find_row();       # Maybe[InstanceOf['Row']]

return unless defined $row; # NARROW-5
print $row->id;             # 診断なし
```

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

## 7. 診断 (DIAG)

すべての診断は安定したコードを持ちます。出力は
`path:line:col: severity: message [code]` の一行で、`--format json` も選べます。

重大度の区別は、`error` が**宣言された二つのものの矛盾**、`warning` が片側が
**推論**であるか絞り込みに依存しているもの、`info` がユーザーが求めたときに
知らされるものです。`--error-on` に達したものがあれば終了ステータスは 1 です。

| コード | 既定 | `lint` | 意味 |
| --- | --- | --- | --- |
| (DIAG-1) `undeclared-variable` | error | ○ | `strict` の下で、どの宣言も届かない名前 |
| (DIAG-2) `unused-variable` | warning | ○ | 宣言されて一度も読まれないレキシカル |
| (DIAG-3) `shadowed-variable` | warning | ○ | 外側のスコープが既に束縛している名前 |
| (DIAG-4) `arity` | error / warning | ○ | 引数の個数が引数リストを満たせない |
| (DIAG-5) `type-mismatch` | error / warning | | 値の形が、入る先の宣言された型と矛盾する |
| (DIAG-6) `unknown-key` | error | | 閉じた `Dict` に無い鍵、宣言のない属性 |
| (DIAG-7) `unknown-method` | warning | | そのクラスが宣言していないメソッド |
| (DIAG-8) `bad-annotation` | info | | 読めないアノテーション |
| (DIAG-9) `return-mismatch` | error / warning | | `Returns:` と食い違う `return` |
| (DIAG-10) `missing-annotation` | info | | 公開サブルーチンに何のアノテーションもない |
| (DIAG-11) `unknown-type` | info | | どこも宣言していない型名・クラス名 |

### 7.1 重大度が動くもの

- (DIAG-4a) `arity` は、シグネチャと `args` に対しては `error` です。perl と
  Smart::Args が実行時に die するからで、静的に言っても誤検知になりません。
  `@_` の展開から読んだリストに対しては `warning` です。そちらは規則ではなく
  形であり、プログラムは動くからです。
- (DIAG-5a) `type-mismatch` は、値がリテラルのとき `error` です（両側が
  書かれているからです）。推論された値のときは `warning` です。
- (DIAG-9a) `return-mismatch` も同じ規則に従います。
- (DIAG-x) パーサの推測（[architecture.md](architecture.md) の `GUESS:`）に
  依存する診断は一段下げて報告されます。

### 7.2 `unknown-method` について

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
error-on = "warning"
strict-annotations = true
```

テーブルが `[check]` 一つなのは、ここに書けることが `lint` と `typecheck` の
両方について真だからです。コマンドラインのフラグが設定ファイルより優先されます
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
- (DEPS-9) 依存を追うのは `typecheck` だけです。`lint` の問うことはルート自身の
  呼び出しについてなので、`@INC` を読んでも得るものがありません。

## 10. 意図的にやらないこと (LIMIT)

- (LIMIT-1) **健全性**。camello は分からないときに黙ります。見逃しはあります。
- (LIMIT-2) **実行**（POLICY-5）。
- (LIMIT-3) **強制変換**（ANNOT-2a）。
- (LIMIT-4) **依存モジュールの検査**（DEPS-5）。
- (LIMIT-5) **パス感度**（INFER-3）。
- (LIMIT-6) **サブルーチンをまたぐ戻り値の推論**（INFER-4a）。
- (LIMIT-7) **リストコンテキストの照合**（INFER-6b）。

---

このドキュメントは実装とフィクスチャに基づいています。ここに書かれていない
振る舞いを見つけた場合、あるいはここに書かれていることと実装が食い違う場合は、
GitHub Issues で報告してください。設計上の判断とその根拠は
[typecheck.md](typecheck.md) にあります。
