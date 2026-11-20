# Lint Rules Specification

このドキュメントは、`camello check` が型とは関係なく報告する**書き方の誤り**を
定義します。型の診断は [types.md](types.md) に、その設計は
[typecheck.md](typecheck.md) にあります。

## 1. 方針 (LINT-POLICY)

- (LINT-POLICY-1) **書いたとおりに動かない書き方だけを報告します。** 書き手が
  そう書いたときに思い描いた動作と、perl が実際にする動作が食い違う書き方です。
  好みが分かれるスタイルは対象にしません。
- (LINT-POLICY-2) **誤検知はゼロを目指します。** よく使われ、意図どおりに動く
  書き方は、各ルールの「指摘しない」に挙げて対象から外します。
- (LINT-POLICY-3) **設定はほぼありません。** ルールはすべて既定で有効で、オプトインの
  ルールはありません。ルールの判定に関わる設定は、読めない `import` を補う
  `strict-equivalent-modules`（LINT-5h）だけです。止める手段は型の診断と同じく `[check] disable`、
  `--disable`、`## camello-disable:` です（[OFF](types.md#8-診断を止める-off)）。
  加えて、Perl::Critic から移ってくるコードのために `## no critic` も読みます
  （[LINT-OFF](#3-診断を止める-lint-off)）。
- (LINT-POLICY-4) **重大度はすべて `warning` です。** 既定の `--min-severity` で
  印字され、既定の `--error-on` では終了ステータスを変えません。perlcritic の
  ように違反で CI を落とすには `--error-on warning` を指定します。
- (LINT-POLICY-5) **`camello check` の既定の報告です。** 別のサブコマンドはありません。
  `perlcritic lib t` を置き換えるのは `camello check lib t` です。型の診断は
  `--types` を付けたときだけ同じ実行に加わります（[DIAG-0](types.md#7-診断-diag)）。

各ルールには Perl::Critic のポリシー名が対応していて、`## no critic` はその名前で
ルールを止めます。対応が一部だけのものは、そう書いてあります。

| code                                  | Perl::Critic                                         |
| ------------------------------------- | ---------------------------------------------------- |
| `return-in-do-value`                  | ControlStructures::ProhibitReturnInDoBlock           |
| `conditional-declaration`             | Variables::ProhibitConditionalDeclarations           |
| `two-arg-open`                        | InputOutput::ProhibitTwoArgOpen                      |
| `bareword-filehandle`                 | InputOutput::ProhibitBarewordFileHandles             |
| `missing-strict`                      | TestingAndDebugging::RequireUseStrict                |
| `missing-warnings`                    | TestingAndDebugging::RequireUseWarnings              |
| `return-sort`                         | Subroutines::ProhibitReturnSort                      |
| `non-lexical-loop-iterator`           | Variables::RequireLexicalLoopIterators               |
| `package-filename-mismatch`           | Modules::RequireFilenameMatchesPackage               |
| `double-semicolon`                    | ValuesAndExpressions::ProhibitNullStatements         |
| `nested-named-sub`                    | Subroutines::ProhibitNestedSubs                      |
| `augmented-assignment-in-declaration` | Variables::ProhibitAugmentedAssignmentInDeclaration  |
| `mutating-list-function`              | ControlStructures::ProhibitMutatingListFunctions（一部） |

`ProhibitReturnInDoBlock` は同名の CPAN ディストリビューション、
`ProhibitNullStatements` は Perl-Critic-Pulp のポリシーで、Perl::Critic 本体には
含まれません。ほかの 11 個は本体のものです。

Perl::Critic のほかのポリシーは実装しません。`ProhibitLeadingZeros` に当たるものも
ありません。Perl で `0` から始まる数はほぼ必ず意図した 8 進数（`0644`、`umask 022`）で、
誤検知のない線が引けないからです。`ProhibitUnusedVarsStricter` に
当たるものは `unused-variable` です。

## 2. ルール (LINT)

### 2.1 `return-in-do-value` (LINT-1)

`do BLOCK` の中の `return` は `do` からではなく、囲んでいるサブルーチンから
抜けます。`do` の値を使う書き方は `do` が値を返すことを当てにしているので、
その中の `return` は書き手の思い描いた動作をしません。

- (LINT-1a) **指摘する**: `do BLOCK` のすぐ下の文にある `return` で、その `do` の
  値が使われているもの。すぐ下の文とは、`do` のブロックを直接構成する文です。
  `return $x if $c;` のような後置の修飾子つきの文や、`f() or return;` も含みます。
  `if`・`unless`・`else`・`for`・`while` のブロックや、ただのブロック `{ ... }` の
  中の `return` は、その分岐を選んで抜ける書き方なので指摘しません。
  Perl::Critic の ProhibitReturnInDoBlock と同じ範囲です。
  値が使われるとは、`do` が次のどれかであることです。
  - 代入の右辺（`=`、`//=`、`||=` などすべて）
  - 関数やメソッドの引数、リスト・ハッシュ・配列の要素、ほかの式のオペランド
  - `return` の引数
  - サブルーチンの本体の最後の文（その値がサブルーチンの戻り値になる）
  - 値が使われる `do` のブロックの最後の文。値が使われるブロックの最後の文に
    置いた `if`・`else` の分岐や、ただのブロックの最後の文も同じです
  ```perl
  $self->{cache} //= do { return $x if $x; compute() };   # 指摘する
  my $v = do { return unless $ok; compute() };            # 指摘する
  sub f { do { return 1 if $x; 2 } }                      # 指摘する
  my $t = do { 1; do { return 3 } };                      # 指摘する
  sub g { if ($x) { do { return 2 } } }                   # 指摘する
  my $v = do { if ($ok) { 1 } else { return } };          # 指摘しない
  my $w = do { for (@a) { return $_ if $_ } 0 };          # 指摘しない
  ```
- (LINT-1b) **指摘しない**: `do` の値が捨てられるとき。`do` が文としてそのまま
  置かれているとき、`or`・`and`・`xor` の右側にあるとき、および値が捨てられる
  `||`・`&&`・`//` の右側にあるときです。
  ```perl
  open my $fh, '<', $f or do { warn $!; return };         # 指摘しない
  defined $x && do { ok($x) or return };                  # 指摘しない
  ```
- (LINT-1c) `do` の中にある `sub { ... }` や名前付きの `sub`、関数に渡した
  ブロック（`eval { }`、`first { }`、`map { }`）の中の `return` は、`do` の
  すぐ下の文ではないので対象外です。
- (LINT-1d) 報告する位置は `return` です。

### 2.2 `conditional-declaration` (LINT-2)

- (LINT-2a) **指摘する**: `my` の宣言文に後置の `if`・`unless`・`while`・`until`・
  `for`・`foreach` が付いているもの。条件が偽のときの変数の中身は perl の仕様で
  未定義で、前回の呼び出しの値が残ることがあります。
  ```perl
  my $x = compute() if $cond;                             # 指摘する
  ```
- (LINT-2b) **指摘しない**: `our` と `local`。どちらも宣言のたびに新しい変数を
  作るのではないので、条件が偽でも食い違いは起きません。

### 2.3 `two-arg-open` (LINT-3)

- (LINT-3a) **指摘する**: 引数が 2 つの `open`。2 つ目の引数の先頭や末尾の文字が
  モードとして読まれるので、ファイル名によって動作が変わります。
  ```perl
  open(my $fh, $path);                                    # 指摘する
  ```
- (LINT-3b) **指摘しない**: 2 つ目の引数が文字列リテラルの `'-'` または `'>-'`
  （標準入力・標準出力を開く書き方）。

### 2.4 `bareword-filehandle` (LINT-4)

- (LINT-4a) **指摘する**: `open`・`opendir`・`sysopen`・`socket`・`socketpair`・
  `pipe`・`accept` のファイルハンドルの引数が裸の単語であるもの。裸の単語の
  ファイルハンドルはパッケージ全体で共有されるので、同じ名前を使うほかのコードと
  ぶつかります。
- (LINT-4b) **指摘しない**: `STDIN`・`STDOUT`・`STDERR`・`DATA`・`ARGV`・`ARGVOUT`。
- (LINT-4c) `open(FH, "< $path")` は LINT-3 と LINT-4 の両方を報告します。
  直すべきことが 2 つあるからです。

### 2.5 `missing-strict` と `missing-warnings` (LINT-5)

- (LINT-5a) **指摘する**: ファイルの中で、`strict`（`warnings`）が有効でない
  ところにコードの文が現れるとき。有効になる前の文も、有効にしたブロックを
  抜けた後の文も含みます。報告はファイルごとにそれぞれ一度、そのような最初の
  コードの文の位置にします。
- (LINT-5b) **コードの文として数えないもの**: `package`、`use`、`no`、空の文、
  コメント、POD、`__END__` と `__DATA__` 以降。`use` の引数（`use Foo { ... }` の
  `{ ... }` など）は `use` の一部です。ブロックつきの `package Foo { ... }` と、
  ただのブロック `{ ... }` は、ブロックの中の文を数えます。どちらも、ブロックの
  中で有効にしたものはブロックの外には及びません。
  `strict` の有無で動作が変わらない、モジュールの頭と終わりの決まり文句も
  数えません。`require 5.006` のようなバージョンの `require`、`require Exporter`
  のようなモジュール名の `require`、`$VERSION`（`our $VERSION`、`$Foo::VERSION`）
  への代入、そして `1;` のようにリテラルだけの文です。
- (LINT-5c) **コードの文として数えるもの**: それ以外のすべての文。`sub` の定義や
  `BEGIN` ブロックも含みます。
- (LINT-5d) **有効にするもの**:
  1. `use strict` と `use warnings`（引数があってもよい）
  2. `use v5.12` 以降は `strict`、`use v5.36` 以降は `warnings` も
  3. [付録 A](#付録-a-strict-と-warnings-を有効にするモジュール) のモジュール
  4. `sub import` の中で `strict` や `warnings` の `import` を呼んでいるモジュール
     （LINT-5e）
  5. `camello.toml` の `strict-equivalent-modules` に挙げたモジュール（LINT-5h）
  6. shebang の `-w` と `-W`（`warnings` のみ）。`-X` は打ち消します。スイッチは
     順に読み、`-I/home/www/lib` のように引数を取るスイッチの後ろは読みません
- (LINT-5e) プロジェクトや依存先のモジュールは、`sub import` の本体を読んで
  判定します。本体が `->import`（`Import::Into` の `->import::into` を含む）を
  呼んでいて、`strict` や `warnings` を名指ししていれば、そのモジュールを `use`
  することはそれを有効にすることです。`no strict 'refs'` のような `use`・`no` の
  文の中の名前は、名指しに数えません。本体が付録 A のモジュールの `import` を
  呼んでいれば、そのモジュールが有効にするものを有効にします。ほかのモジュールの
  `import` を呼んでいれば（`My::Pragma->import`）、そのモジュールの `import` も
  同じ規則で読み、何段でもたどります。`import` の中の `require t; t->import` の
  ように、ファイルのどこにも `use` されていないモジュールも探して読みます。
  一度たどったモジュールは二度読みません。
  読む `import` は、perl と同じく継承をたどって最初に見つかったものだけです。
  サブクラスの `sub import { 1 }` は、親の `import` が何を有効にしていても何も
  有効にしません。`$class->SUPER::import(@_)`・`$class->next::method(@_)` で親に
  渡すものは、親の `import` が有効にするものも有効にします。
  たどった先が見つからなければ LINT-5f と同じく扱います。
  - `*import = \&Exporter::import` のように、ほかのモジュールの `import` を glob で
    借りているものは、そのモジュールの `import` を読みます。
  - `$^H |= 0x602` のように `$^H` にリテラルを書き込むものは、`use strict` が立てる
    ビット（0x2・0x200・0x400）がすべて立っていれば `strict` を有効にします。
    リテラルでない値の書き込み（`$^H |= $integer::hint_bits`）は、ほかの
    プラグマのビットなので何も有効にしません。
  - 次のものは中身が読めないので LINT-5f と同じく扱います。呼び出し元に届く
    文字列の `eval`（`eval qq{package $pkg; use Test2::V0;}` のように、`package`・
    `import`・`$^H` を含むもの）と、ファイルの一番外側で glob に代入した `import`
    （`*{$prefix . 'import'} = sub { ... }`）。名前の最後の要素が `import` で
    あるものだけで、`*important_thing` は数えません。`eval q{ use Foo; }` のように
    呼び出し元に届かない `eval` は数えません。
- (LINT-5f) どこにも見つからないモジュールは、名前に関わらず `strict` も `warnings` も
  有効にするものとして扱います。見えないものについて誤検知を出さないためです。
  探す場所は `@INC`（`--inc` を付ければその場所）、`[check] lib`、チェック対象です。
  チェック対象に含めず、依存先として読むだけのものも、見つかれば LINT-5e で
  `import` を読みます。小文字だけの名前も同じです。perl のプラグマ（`utf8`、`lib`、
  `integer` など）は perl の `@INC` で必ず見つかり、LINT-5e で何も有効にしないと
  判定されます。小文字の名前で見つからないのは、探す場所に入っていないプロジェクトの
  モジュール（`t`、`t::helper`）だからです。
  `use Foo ()` は `import` を呼ばないので、どのモジュールでも何も有効にしません。
- (LINT-5h) `camello.toml` の `[check] strict-equivalent-modules = ["My::Test"]` に
  挙げたモジュールは、`strict` と `warnings` の両方を有効にします。付録 A に
  足す形で、LINT-5e で読めない `import` のための逃げ道です。Perl::Critic の
  `equivalent_modules` に当たります。
- (LINT-5g) `undeclared-variable` が `strict` の有効な範囲を決めるときも、
  LINT-5d と同じ判定を使います。ただし LINT-5f は逆に倒れます。見つからない
  モジュールは `strict` を有効にしないものとして扱います。どちらも、見えない
  ものについて黙る側です。

### 2.6 `return-sort` (LINT-6)

- (LINT-6a) **指摘する**: `return sort ...`。スカラーコンテキストで呼ばれたときの
  `sort` の値は perl の仕様で未定義です。報告する位置は `sort` です。

### 2.7 `non-lexical-loop-iterator` (LINT-7)

- (LINT-7a) **指摘する**: `for $x (...)`・`foreach $x (...)` のように、ループ変数を
  宣言していないもの。ループ変数は暗黙に `local` されるので、ループの中で代入
  した値はループの後に残りません。外側の `my $x` をループ変数にしても、ループの
  中から呼んだサブルーチンにはその値が見えません。
- (LINT-7b) **指摘しない**: `for my $x`・`for our $x`、および
  ループ変数を書かない（`$_` を使う）ループ。

### 2.8 `package-filename-mismatch` (LINT-8)

- (LINT-8a) **指摘する**: `.pm` ファイルのどの `package` の名前も、ファイルの
  パスと合わないもの。`::` で区切った名前の各部分を、パスの末尾から順に
  ディレクトリ名とファイル名（`.pm` を除く）に突き合わせ、短い方が尽きるまで
  すべて一致すれば合っています。`lib/Foo/Bar.pm` の `package Foo::Bar` も、
  `Bar.pm` の `package Foo::Bar` も合っています。合う `package` がないと、
  `use Foo::Bar` がそのファイルを読んでも `Foo::Bar` は定義されません。補助の
  パッケージが先に来て、パスの名前のパッケージが後に来るファイルは合っています。
- (LINT-8b) **指摘しない**: `package` のないファイル、最初の `package` が
  `package main` のファイル、`.pm` 以外のファイル。報告する位置は最初の
  `package` の名前です。
- (LINT-8c) **指摘しない**: `use` で読まれるつもりのないファイル。
  - `package` と名前を別の行に書いたもの（`package\n    DBI;`）。PAUSE は
    `package` と同じ行の名前しか読まないので、これはインデックスから隠すための
    書き方です。
  - パスの祖先にあたるパッケージのもの。`XML/SAX/PurePerl/DTDDecls.pm` の
    `package XML::SAX::PurePerl` は、そのパッケージの実装を分けたファイルで、
    パスを指定して `require` されます。

### 2.9 `double-semicolon` (LINT-9)

- (LINT-9a) **指摘する**: `;` で終わる文の直後に、何もない文（`;` だけ）が続くもの。
  `;;` を意図して書くことはないので、打ち間違いか編集の残りです。
  ```perl
  foo();;                                                 # 指摘する
  ```
- (LINT-9b) **指摘しない**: `}` で終わる文の直後の `;`。
  ```perl
  { ... };
  sub foo { ... };
  if (...) { ... };
  ```
  ブロックを閉じた後の `;` は、ブロックが式なのか文なのかを書き手が区別しない
  場所で書かれるもので、`;;` とは違って書き手の意図に反するものではありません。
  `for (;;)` は何もない文ではありません。

### 2.10 `nested-named-sub` (LINT-10)

- (LINT-10a) **指摘する**: サブルーチンの本体（名前付きでも無名でも）の中に
  ある名前付きの `sub` で、外側のサブルーチンが呼び出しごとに作る変数を読む
  もの。そうした変数は `my` で宣言したもの、引数、`for my` のループ変数、
  `catch` の変数です。名前付きの `sub` は一度しか作られないので、そうした
  変数は最初の呼び出しのものしか見えません（perl は "Variable will not stay
  shared" と警告します）。報告する位置は内側の `sub` の名前で、メッセージは
  読んでいる変数を一つ名指しします。
- (LINT-10b) **指摘しない**: 外側の変数を読まないもの（`package Foo { sub bar
  { ... } }` や小さな補助関数）。`state` と `our` の変数、ファイルの一番外側の
  変数、内側で宣言し直した同名の変数は、何度呼んでも一つなので数えません。
  `my sub` と `state sub` は呼び出しごとに作られるので対象外です。前方宣言
  （本体のない `sub foo;`）も対象外です。

### 2.11 `augmented-assignment-in-declaration` (LINT-11)

- (LINT-11a) **指摘する**: `my` と `local` の宣言を左辺にした、`=` 以外の代入
  （`.=`、`+=`、`||=`、`//=` など）。`my` の変数は新しく作られて未定義なので、
  演算子が組み合わせる相手がなく、`=` と同じ結果になります。`local` は元の値を
  退避して変数を空にしてから代入するので、元の値は結果に入りません。
  ```perl
  my $err .= "Can't locate $name";                       # 指摘する
  local $Carp::CarpLevel += 1;                           # 指摘する（`= 1` と同じ）
  local $ENV{PATH} .= ":/opt/bin";                       # 指摘する（PATH は ":/opt/bin"）
  ```
- (LINT-11b) **指摘しない**: `our` と `state`。`our` はすでにあるパッケージ変数を
  指し、`state` は前の呼び出しの値を持っているので、演算子が組み合わせる値が
  あります。`state $x //= compute()` は一度だけ初期化する書き方です。
  Perl::Critic はこの 2 つも指摘します。
- (LINT-11c) 報告する位置は代入の式です。

### 2.12 `mutating-list-function` (LINT-12)

`map` などのブロックの中の `$_` は、リストの各要素そのもの（エイリアス）です。
`$_` を書き換えると、作ろうとしている新しいリストではなく、渡した配列の要素が
変わります。

- (LINT-12a) **指摘する**: `map`・`grep`・`first`・`any`・`all`・`none`・`notall` の
  ブロックで、`$_` を書き換えるもの。リストに配列そのもの（`@a`、`@$r`、`@{...}`、
  `$r->@*`）か `values` があるときだけです。`split`・`<$fh>`・`keys` などの
  呼び出しが返す値は新しく作られた値で、書き換えてもほかに影響しません。
  書き換えとは、`$_` に対する次のものです。
  - `/r` のない `s///`（`$_ =~ s///` も）
  - 置き換えるか `/d` のある、`/r` のない `tr///`・`y///`。`tr/a//` は数えるだけです
  - 代入（`=`、`.=` などすべて）、`++`、`--`
  - 引数のない `chomp`・`chop`、または `$_` を渡したもの
  ```perl
  my @out = map { s/^\s+//; $_ } @lines;                 # 指摘する（@lines も変わる）
  my %ok  = map { s/^&//; $_ => 1 } @EXPORT_OK;           # 指摘する
  my @out = map { s/^\s+//r } @lines;                    # 指摘しない
  my @out = grep { chomp; length } <$fh>;                # 指摘しない
  ```
- (LINT-12b) **指摘しない**:
  - 値が捨てられるもの（`map { s/a/b/ } @lines;`）。`for` の代わりに `map` を
    使って、配列を書き換えるために書いたものです。
  - 作ったリストを、読んだのと同じ配列に代入するもの（`@a = map { s/x//; $_ } @a`。
    間に `grep` や `sort` を挟んでも同じ）。配列は結局その値になります。
  - `map` 以外で、書き換えそのものがブロックの値であるもの
    （`scalar grep { s/^>// } @$body`）。書き換えた数を数えながら、その場で
    書き換えるために書いたものです。
- (LINT-12c) ブロックの中で `$_` を別のものにする書き方の中は見ません。入れ子の
  `map` などのブロック、`for` のループと後置の `for`、`given`、`sub` です。
  `local $_` があるブロックは対象外です。
- (LINT-12d) 報告する位置は、最初の書き換えの式です。Perl::Critic は配列に
  限らず、List::MoreUtils の関数も対象にします。

## 3. 診断を止める (LINT-OFF)

`## camello-disable:` と `[check] disable` は、ここのコードにもそのまま効きます。
それに加えて、Perl::Critic の `## no critic` を読みます。

- (LINT-OFF-1) **名前の突き合わせ**: `## no critic (A, B)`・`## no critic (A B)`・
  `## no critic qw(A B)` の各名前を、大文字と小文字を区別せずに、§1 の表の
  Perl::Critic の名前（`Perl::Critic::Policy::` を除いたもの）の部分文字列として
  探します。`(ProhibitTwoArgOpen)` も `(TwoArg)` も `two-arg-open` を止めます。
  `::` を含む名前は、最後の部分だけでも突き合わせます。
- (LINT-OFF-2) **何も合わない名前は何も止めません。** camello が実装していない
  ポリシーの名前は、`## camello-disable:` に知らない名前を書いたときと違って
  行を丸ごと止めません。Perl::Critic のために書かれた注釈の大半はそれだからです。
- (LINT-OFF-3) **名前のない `## no critic`** は §2 のコードをすべて止めます。
  型の診断は止めません。
- (LINT-OFF-4) **範囲**: コードと同じ行にある `## no critic` はその行に効きます。
  自分の行に単独である `## no critic` は、そこから、自分の行に単独である
  `## use critic`、囲んでいるブロックの終わり、ファイルの終わりのうち最も近いもの
  までに効きます。

## 付録 A. `strict` と `warnings` を有効にするモジュール

(`pragma::BY_IMPORT`)

| モジュール | strict | warnings |
| --- | --- | --- |
| `strict` | ✓ | |
| `warnings` | | ✓ |
| `strictures`, `Modern::Perl`, `common::sense` | ✓ | ✓ |
| `Moose`, `Moose::Role`, `Moose::Exporter`, `Moose::Util::TypeConstraints` | ✓ | ✓ |
| `Moo`, `Moo::Role`, `Mouse`, `Mouse::Role`, `Any::Moose`, `Role::Tiny` | ✓ | ✓ |
| `Mojo::Base`, `Mojolicious::Lite`, `Dancer`, `Dancer2` | ✓ | ✓ |
| `Test2::V0`, `Test::Most`, `Test::Class::Moose` | ✓ | ✓ |
| `Class::Accessor::Typed` | ✓ | ✓ |
| `Object::Pad` | ✓ | |
| `Exporter`, `Exporter::Tiny` | | |

網羅ではありえません。LINT-5e が表にないモジュールの `import` を読み、
LINT-5h が表に足せるのはそのためです。`Exporter` の行は、何も有効にしないことを
`import` を読まなくても知っておくためのものです。
