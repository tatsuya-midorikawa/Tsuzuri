# ドキュメント コメント

`///` は、直後の宣言へ説明を付ける構文です。言語サーバー（LSP）のホバーと、`tsuzuri doc` が生成する Markdown がこの説明を使います。

型検査や生成されるバイナリには影響しません。ただし、`@deprecated` で始まる行は、別のパッケージからの使用を警告 `W1005` にします（[非推奨の印](#非推奨の印)）。

## この記事のポイント

- 直後の宣言に対して `///` でドキュメントコメント（doc comment）を記述します。
- スラッシュ直後の空白 1 文字が自動トリミングされ、複数行は改行で連結されます。
- `def`、`record`、`union`、`type`、`const`、`class`、型クラス内のメソッド `def` に付与できます。
- `fn` や `let` 実装、`instance`、フィールド、ローカル束縛への配置は構文エラー `E0002` です。
- 説明の行を `@deprecated` で始めると宣言が非推奨になり、別のパッケージ（標準ライブラリを含む）からの使用が警告 `W1005` になります。
- `@complexity`・`@since`・`@see` の行で、計算量・版・関連する宣言を書けます（構造化タグ）。標準コレクションの計算量の表は、このタグから生成されます。
- 説明の中の ```` ```tsuzuri ```` のコード例は、`tsuzuri test --doc` が型検査・実行して確かめます（doc-test）。`run=出力`・`test`・`fails=コード`・`ignore` で確かめ方を選び、`# ` で始まる行は隠し行です。
- `tsuzuri doc <input> -o <outdir>` コマンドで公開 API の Markdown ドキュメントを出力します。
- 出力先ディレクトリは `.tsuzuri-docs` マーカーファイルによる上書き保護機構を備えています。

## 基本の書き方

```text
/// 関数の概要を記述します。
/// Markdown 記法やコードブロックを含められます。
def 関数名 :: 型 = 実装
```

`///` を付与した直後の宣言がドキュメント化の対象になります。コメント行は連続して複数行記述でき、改行で連結されます。

```tsuzuri project=docs-example file=Arithmetic/Calculator.tz
/// 2 つの数値を加算します。
///
/// 整数演算のオーバーフロー検査が行われます。
def add :: i64 -> i64 -> i64 = \a b -> a + b

/// 計算精度の既定値です。
const DefaultPrecision: i64 = 64
```

```tsuzuri project=docs-example file=Main.tz run=30
let sum = Arithmetic::Calculator.add 10 20
sum
```

実行結果:

```text
30
```

### 整形のルール

- **先頭空白のトリミング**: `///` の直後にある半角スペース 1 文字は自動的に除去されます（`/// テキスト` は `テキスト` として解釈されます）。
- **4 連スラッシュ**: `////` のようにスラッシュを 4 つ以上重ねた場合、トリミング後の本文は `/` で始まります。
- **通常のコメントとの違い**: 通常の行コメント `//` やブロックコメント `/* ... */` はドキュメント情報として保持されず、生成ドキュメントや LSP ホバーには現れません。
- **Markdown の保持**: コメント内に記述された見出し、箇条書き、コードフェンス（```` ```tsuzuri ```` など）、HTML タグはそのまま維持されます。

## コメントを付与できる宣言と禁止される場所

付けられるのは、次の宣言の直前だけです。`private` にも付けられますが、生成される文書には出ません。

| 対象の宣言 | 可否 | 説明 / 違反時の挙動 |
| --- | --- | --- |
| `def`（通常 / export / private / extern） | ○ | 関数シグネチャの説明 |
| `record` | ○ | レコード型自体の説明 |
| `union` | ○ | 共用体型自体の説明 |
| `type` | ○ | 型エイリアスの説明 |
| `const` | ○ | 定数の説明（生成文書では型のみ表示） |
| `class` | ○ | 型クラスの説明 |
| 型クラス内のメソッド `def` | ○ | 型クラスメソッドの説明 |
| `fn` / `let` 実装ブロック | ✗ | 構文エラー `E0002`。先行する `def` に付けます |
| `instance` | ✗ | 構文エラー `E0002`。インスタンス実装は文書化されません |
| レコードのフィールド定義 | ✗ | 構文エラー `E0002` |
| ローカル束縛（関数内の `let` 等） | ✗ | 構文エラー `E0002` |
| `test` ブロック | ✗ | 構文エラー `E0002` |
| コンピュテーション式の文 | ✗ | 構文エラー `E0002` |
| エントリーポイントの式・ファイル末尾 | ✗ | 構文エラー `E0002` |

次は断片です。

```text
def calculate :: i64 -> i64
/// 誤った位置: 実装の fn の前に置いている
fn calculate x = x * 2
```

上記のように `fn` の直前に `///` を配置すると、構文解析時にエラー `E0002` になります。ドキュメントコメントは必ず `def` の直前に記述してください。

## 非推奨の印

説明の行を `@deprecated` で始めると、その宣言は非推奨になります。残りの文が、使う側への案内（移行先など）です。新しい構文ではなく、説明の本文の約束です。

```text
/// @deprecated use LegacyLib::Old.next instead
def old :: i64 -> i64 = \x -> x + 1
```

- **印の行**: 各行の先頭の空白を除いて `@deprecated` で始まり、その直後が行末か空白（スペースかタブ）である最初の 1 行です。`@deprecatedX` は印ではありません。2 つ目以降の `@deprecated` の行は、通常の説明です。コードフェンスの中の行も区別しません。
- **案内**: 印の行の残りから前後の空白を除いたものです。空でもかまいません。
- **表示**: `tsuzuri doc` と LSP のホバーは、この行も説明として出します。
- **対象**: `def`（`extern def` を除く）、`record`、`union` です。`const`、`type`、`class`、メソッド、`extern def` の印は、説明として表示されるだけで、警告にはなりません。

### 警告 W1005

別のパッケージ、または標準ライブラリが宣言した非推奨の宣言を、自分のソースが使うと `W1005` が出ます。同じパッケージの中の使用には出ません。`Tsuzuri.toml` のないプロジェクトは、全体が 1 つのパッケージです。非推奨にした側が、移行の期間に自分の `--deny-warnings` のビルドを壊さずに済みます。

パッケージ `legacy-lib` の `Old.tz` と、それに依存する `app` の `Main.tz` です。

```text
legacy-lib/Old.tz
    /// @deprecated use LegacyLib::Old.next instead
    def old :: i64 -> i64 = \x -> x + 1
    def next :: i64 -> i64 = \x -> x + 1
    /// @deprecated use a tuple
    record Pair { left: i64, right: i64 }
    /// @deprecated use bool
    union Light = Red | Green
    def inside :: i64 -> i64 = \x -> old x

app/Main.tz
    let pair = LegacyLib::Old.Pair { left: 1, right: 2 }
    let light = LegacyLib::Old.Red
    let bonus = match light with
        | LegacyLib::Old.Red -> 0
        | LegacyLib::Old.Green -> 1
    LegacyLib::Old.old 39 + pair.right + bonus
```

`tsuzuri check app` は、名前を書いた 5 か所を警告します。終了コードは 0 です。

```text
app/Main.tz:1:27: warning[W1005]: 'LegacyLib::Old.Pair' is deprecated: use a tuple
  1 | let pair = LegacyLib::Old.Pair { left: 1, right: 2 }
    |                           ^

app/Main.tz:2:28: warning[W1005]: 'LegacyLib::Old.Light' is deprecated: use bool
  2 | let light = LegacyLib::Old.Red
    |                            ^

app/Main.tz:4:22: warning[W1005]: 'LegacyLib::Old.Light' is deprecated: use bool
  4 |     | LegacyLib::Old.Red -> 0
    |                      ^

app/Main.tz:5:22: warning[W1005]: 'LegacyLib::Old.Light' is deprecated: use bool
  5 |     | LegacyLib::Old.Green -> 1
    |                      ^

app/Main.tz:6:16: warning[W1005]: 'LegacyLib::Old.old' is deprecated: use LegacyLib::Old.next instead
  6 | LegacyLib::Old.old 39 + pair.right + bonus
    |                ^
```

`legacy-lib` の `inside` が `old` を呼ぶ行は、同じパッケージなので警告しません。

- **警告する場所**: 宣言の名前を書いた場所です。関数の参照、`record` の構築、型の注釈（`let` の束縛、キャスト、`def` のシグネチャ、フィールドの型、型エイリアスの定義）、`union` の case の構築とパターンです。`union` の case を使うと、`union` の印が使われ、`Light.Red` のように型名を付けた書き方は case の位置に 1 件です。型エイリアスを経由した使用は、エイリアスの定義の 1 件だけです。パターンの中の型注釈（`| q: LegacyLib::Old.Pair ->`、`\(p: LegacyLib::Old.Pair) ->`、`for` のパターン）は警告しません。
- **メッセージ**: `'<完全修飾名>' is deprecated: <案内>` です。案内が空なら `'<完全修飾名>' is deprecated; see its documentation for a replacement` です。案内は 200 文字までで、超えると `...` で切ります。
- **位置**: 参照の最後の名前（上の例の `Pair`、`old`）です。同じ位置は 1 回だけです。
- **コマンド**: `check`、`build`、`run`、`test`、`bench`、`doc`、言語サーバーが、ほかの警告と同じ形で出します。既定で有効で、`--deny-warnings` を付けると終了コード 1 です。この例の `run app --deny-warnings` は何も実行しません。
- **コード生成**: 生成されるコードは、`@deprecated` の有無で変わりません。別のパッケージが使える非推奨の宣言がないプロジェクトでは、警告のための追加の解析も行いません。

非推奨にした宣言を取り除く手順と、警告が互換性の保証の対象外であることは、[安定性と互換性の方針](../../../docs/stability.md#非推奨の手順) にあります。

## 構造化タグ

説明の行を `@complexity`・`@since`・`@see` で始めると、宣言に計算量・版・関連する宣言を付けられます。新しい構文ではなく、`@deprecated`（[非推奨の印](#非推奨の印)）と同じ、説明の本文の約束です。

```text
/// Returns a new array with a copy of every element.
///
/// @complexity O(n)
/// @see Array.append
def copy :: Copy<'a> => ref ['a] -> ['a]
```

- **タグの行**: 各行の先頭の空白を除いた最初の語がタグそのもので、直後が行末か空白（スペースかタブ）の行です。`@complexityX` はタグではありません。値は、タグの後ろから行末までの前後の空白を除いたものです。説明の本文との間に `///` だけの行を 1 行置くと、`tsuzuri doc` で別の段落になります。順序は「説明、`@complexity`、`@since`、`@see`、例（[例を書く](#例を書くdoc-test)）」で、タグの行と例の間にも `///` だけの行を置きます。
- **`@complexity`**: 最初の 1 行だけが有効です。値は `O(式)`、または `amortized O(式)`・`expected O(式)` に `, worst O(式)` を続けたものです。式は `1`、変数 `n`・`m`・`k`、`log`・`log^2`、`+`・`*` で書きます（`n log n`、`n + m`、`n * m`）。`n` は主な入力の要素数、`m` はもう 1 つの入力の要素数、`k` は新しいコレクションを作る関数の結果の要素数（結果が入れ子のコレクションなら入れ子の要素もすべて数えます。表を走査する `longest_probe` では表の大きさ）です。1 回の呼び出しの最悪の時間で、コールバックの呼び出し・`Eq.eq`・`Ord.lt`・`Hash.hash`・要素 1 つの `Copy` の複製は `O(1)` と数えます（入れ子のコレクションを持つ要素の複製は、その大きさの分かかります）。`Seq` を返す関数は `Seq` を返すまでの時間です。`expected` と `amortized` は、キーのハッシュが一様に分散するときの値です。
- **`@since`**: 最初の 1 行だけが有効です。値は `0.2.0` のような版で、その宣言が最初に入ったコンパイラの版です。新しい公開関数から付けます。
- **`@see`**: 何行でも書けます。値は `Array.sum_kahan` のような `Module.name`（または `Module`）で、1 行に 1 つです。
- **検査**: コンパイラは値を検査せず、診断も出しません。標準ライブラリの Array・Vec・Map・Set・HashMap・HashSet・List・String の全公開関数に有効な `@complexity` が要り、各ページの「計算量」の表は `node scripts/gen-docs.mjs` がこのタグから作ります。
- **互換性**: タグの追加と変更は説明の変更で、`tsuzuri api-diff` は `non-breaking` と報告します。IR は変わりません。

## 例を書く（doc-test）

説明の中に置いた ```` ```tsuzuri ```` のコードフェンスは、その宣言の**例**です。`tsuzuri test --doc` は例を 1 つずつ独立したプログラムとして型検査し、印に従って実行して確かめます。説明に書いた例が今のコンパイラで動くことを、テストと同じ手順で保てます。

```tsuzuri project=doctest-example file=Geometry.tz
/// Returns the area of a `width` by `height` rectangle.
///
/// ```tsuzuri run=12
/// Geometry.area 3 4
/// ```
///
/// A line that starts with `# ` runs but is not shown.
///
/// ```tsuzuri run=50
/// # let side = 5
/// Geometry.area side 10
/// ```
def area :: i64 -> i64 -> i64
fn area width height = width * height

/// Returns the perimeter of a `width` by `height` rectangle.
///
/// ```tsuzuri test
/// test "a square" = Test.equal (ref (Geometry.perimeter 2 2)) (ref 8)
/// ```
///
/// ```tsuzuri fails=E1003
/// Geometry.perimeter "2" 2
/// ```
def perimeter :: i64 -> i64 -> i64
fn perimeter width height = 2 * (width + height)
```

```tsuzuri project=doctest-example file=Main.tz run=12
Geometry.area 3 4
```

```sh
tsuzuri test --doc .
```

```text
ok 1 - Geometry area#1
ok 2 - Geometry area#2
ok 3 - Geometry perimeter#1
ok 4 - Geometry perimeter#2

4 passed; 0 failed; 0 ignored
```

### 例の記法

| 項目 | 規則 |
| --- | --- |
| 開きの行 | `///` の本文の 0 桁目から `` ` `` が 3 個以上並び、続く情報文字列の最初の語が `tsuzuri` の行です。`text`・`sh`・言語なしなど、ほかの言語のフェンスは例ではありません（その中の行も例として探しません）。`~~~` のフェンスも例ではありません |
| 閉じの行 | 開きより後ろで、`` ` `` だけが開きと同じ個数以上並ぶ最初の行です |
| 本文 | 開きと閉じの間の行です。そのまま 1 つの `Main.tz` になります（包みも補完もしません）。トップレベルの式が入口で、`let`・`def`・`test` も書けます。宣言は、ほかのモジュールからと同じように `Geometry.area` と修飾して呼びます |
| 対象 | `tsuzuri doc` が出す宣言の説明だけです。`private` の宣言と `instance` の説明の例は集めません |

`tsuzuri` の後ろに、空白で区切って確かめ方を 1 つまで書けます。

| 印 | 確かめること |
| --- | --- |
| （なし） | 型検査が通る。実行しない（ファイル・ネットワーク・時間・乱数に触れる例に使います） |
| `run=出力` | 型検査が通り、ネイティブで実行した標準出力が `出力` と同じ。`run=`（空）は何も出力しないこと |
| `test` | 本文の `test` がすべて通る（1 つ以上必要） |
| `fails=コード` | 型検査が拒否し、最初のエラーのコードが `コード`（`E` と数字 4 桁） |
| `ignore` | 何も確かめない。`--list` には出ます |

- **出力の符号化**: `run=` の値はパーセント符号化です。`%` と 16 進 2 桁が 1 バイトで、全体を UTF-8 として読みます（JavaScript の `decodeURIComponent` と同じ。`+` は `+` のまま）。`%`→`%25`、空白→`%20`、タブ→`%09`、改行→`%0A`、CR→`%0D`、`` ` ``→`%60` は必ず符号化し、ほかの文字は書いたままです。2 行の出力 `a b` と `c` は `run=a%20b%0Ac` です。
- **出力の比べ方**: 実際の出力と期待の両方で `\r\n` を `\n` にし、末尾の空白と改行を除いて完全一致で比べます。先頭の空白は意味を持ちます。標準エラーは比べません。
- **値は表示されるものだけ**: `Maybe`・`Result`・レコード・共用体を最後の式にしても何も出力されません。`match` や `Maybe.default_value` で表示できる値にするか、`test` にします。
- **隠し行**: 本文の行が `#` だけ、または `# ` で始まる行は隠し行です。例を動かすときは行頭の `#` と直後の空白 1 文字を取り（`# let side = 5` → `let side = 5`）、`tsuzuri doc` とホバーには出しません。準備のための行を読者に見せずに済みます。`#x` のような行は隠し行ではありません。
- **独立した実行**: 例はそれぞれ別のプログラムです。前の例の `let` は見えません。

### tsuzuri test --doc

```text
tsuzuri test --doc PATH [--list] [--filter TEXT] [--index N]... [--json] [-O0|-O1|-O2|-O3] [--deny-warnings]
```

`PATH` はパッケージのディレクトリ、または `std` です。パッケージでは、根のパッケージのソースの例だけを集め（依存パッケージの例は走らせません）、各例をパッケージの `Main.tz` の代わりに置いて走らせます。パッケージの宣言・`Tsuzuri.toml`・依存はそのまま使えます。`std` を渡すと標準ライブラリの例を走らせます。

- **選択**: `--filter TEXT` は `Geometry.area#1` のようなラベルの部分一致、`--index N` は `--list` の番号で、`tsuzuri test` と同じです。外れた例と `ignore` の例は `ignored` に数えます。
- **最適化**: 既定は `-O0` です。`-O3` でも同じ結果になるはずの例を書きます。
- **出力**: 結果は標準出力に `ok N - モジュール 名前#番号` か `not ok N - …` で出し、失敗の下に理由（`failure:`）と場所（`at:`）を付けます。最後に `passed; failed; ignored` の件数です。`--json` は 1 例 1 行の `{"type":"doctest",...}` と、最後の `{"type":"summary",...}` です。`--list --json` の行のキーは `index`・`kind`・`line`・`module`・`name`・`path`・`type` です。
- **終了コード**: すべて成功（または 0 件）で 0、失敗・記法の誤り・型検査や読み込みの失敗で 1、引数の誤りで 2 です。失敗があると、最初に失敗した例の開きの行へ `E2006`（`N doc tests failed`）を出します。

`area` の 1 つ目の例を `run=13` に変えると、次のように失敗します。

```text
not ok 1 - Geometry area#1
  failure: expected output "13", found "12"
  at: ./Geometry.tz:3
ok 2 - Geometry area#2
ok 3 - Geometry perimeter#1
ok 4 - Geometry perimeter#2

3 passed; 1 failed; 0 ignored
./Geometry.tz:3:1: error[E2006]: 1 doc test failed
```

| 失敗 | `failure:` の文 |
| --- | --- |
| 型検査の失敗（`run=`・`test`・印なし） | `コード: メッセージ (example line L, column C)`（位置は隠し行の印を取った後の例の行と桁） |
| 実行時のトラップや 30 秒の時間切れ | `E2005: …` |
| 出力の不一致 | `expected output "期待", found "実際"` |
| `test` が 0 件・失敗 | `the example declares no test`／`test "名前" failed: 理由` |
| `fails=` の不一致 | `expected E1003, but the example was accepted`／`expected E1003, found コード: メッセージ` |

- **`Main.tz` の例は走らない**: `Main.tz` の宣言の例は、`Main.tz` を置き換えると自分の宣言が見えなくなるので実行しません。`ignored` に数え、標準エラーに `N doc example(s) in Main.tz not run; move the documented declaration to another module` を 1 回出します（`--json` では警告 `W2001` の行）。例を付けたい宣言は別のモジュールへ移します。
- **ネイティブだけ**: 例はこのマシンのネイティブで走ります。`--target`・`-g`・`-o`・`--coverage`・`--seed`・`--link`/`-l`/`-L`・`--contracts` とは組み合わせられません（`E2000`）。契約は `run` と同じく検査します（`check`）。
- **警告**: 例の警告は、ふだんは無視します。`--deny-warnings` を付けると、警告のある例は最初の警告で失敗します（`--warn implicit-copy` などで有効にした警告も数えます）。ネイティブのライブラリは `Tsuzuri.toml` の `[native]` に書きます。
- **上限**: 1 つのパッケージの例は 10,000 個までです。超えると `E1017` です。

### 記法の誤り（E2011）

`tsuzuri test --doc` は、例を 1 つでも走らせる前に記法を確かめます。誤りがあれば `E2011` をすべて（最大 50 件）出し、何も実行せず終了コード 1 です。

| 誤り | メッセージ |
| --- | --- |
| 未知の印（`fail=E1003` など） | `unknown option 'fail=E1003' in a tsuzuri code fence; use run=OUTPUT, test, ignore, or fails=CODE` |
| 同じ印を 2 回／2 種類の印 | `option 'test' is given twice; write it once`／`a tsuzuri code fence takes one of run=, test, ignore, and fails=; found 'test' and 'ignore'` |
| `fails=` の値 | `fails= needs an error code such as E1012; found 'fails=1003'` |
| `run=` の `%` の後ろが 16 進 2 桁でない／復号が UTF-8 でない | `'%' in run= must be followed by two hexadecimal digits; write a literal percent sign as %25`／`run= is not valid UTF-8 after decoding; encode the output as UTF-8` |
| 字下げしたフェンス・閉じがない・空の本文 | `a tsuzuri code fence must start at column 1 of the comment; remove the indentation`／`the tsuzuri code fence is not closed; end it with a line of three backticks`／`a tsuzuri example needs at least one line of code` |

`tsuzuri check`・`tsuzuri doc`・言語サーバーは `E2011` を出しません。記法の誤ったフェンスは、そのままの本文として表示します。

### tsuzuri doc とホバーでの表示

`tsuzuri doc` と言語サーバーのホバーは、例を読者向けに整えて表示します。

- 印や隠し行のある例は、開きの行を ```` ```tsuzuri ```` にし、隠し行を除きます。
- `run=出力` の例（出力が空でないもの）の後ろに `Output:` と出力の ```` ```text ```` ブロックを、`fails=コード` の例の後ろに ``Compile error: `コード`.`` を足します。
- 印も隠し行もない例と、ほかの言語のフェンスは、1 バイトも変えません。

上の `area` の 2 つ目の例は、隠し行を除いた 1 行 `Geometry.area side 10` の ```` ```tsuzuri ```` ブロックになり、その後ろに次が続きます。

````text
Output:

```text
50
```
````

### 標準ライブラリの例

標準ライブラリ（`std`）の公開関数には、すべてこの形式の例があります。`tsuzuri test --doc std` で確かめられ、`tsuzuri doc std` が生成するページや、ホバーで読めます。

## tsuzuri doc によるドキュメント生成

`tsuzuri doc` は、公開（public）宣言だけを Markdown の API リファレンスにします。

```sh
tsuzuri doc src -o docs/api
```

### 生成の仕様

- **型検査のあと**: 入力したソースの型検査が終わってから出力します。型エラーがあると生成せず、既存の出力も書き換えません。
- **入口は不要**: ライブラリ用なので、入力ディレクトリに `Main.tz` が無くても実行できます。
- **ファイル名**:
  - 一覧は `index.md` です。
  - 階層モジュールは `Geometry.Point.md` です。見出しと一覧では `Geometry::Point` と書きます。
  - ファイルに `namespace` があると、見出しの下に Namespace 行が出ます（例: Namespace: `Physics`）。ディレクトリから決めた名前空間だけでは、この行は出ません。
- **並び**: ページは名前順です。ページ内の宣言は、ソースに出てきた順です。型変数、制約、region、アクティブパターンの宣言名はそのまま残します。
- **中身**:
  - `const` は型だけです。初期化式は出しません。
  - 型クラスはメソッドを含めて出します。`instance` は出しません。
  - `private` は出しません。`///` を付けてあっても同じです。
  - コンパイラ内部の組み込み関数は一覧にしません。ソースにある標準ライブラリは、`std` ディレクトリを入力すれば生成できます。`Maybe.tc` のように型とビルダー操作が同じファイルにあれば、1 ページに並びます。

```sh
tsuzuri doc std -o build/std-api
```

## 出力ディレクトリの保護機構（.tsuzuri-docs）

出力先を間違えて既存ファイルを消さないよう、`tsuzuri doc` は次の規則で守ります。

1. **新規ディレクトリ**: 出力先が無ければ、親も含めて作ります。
2. **マーカー**: 既存ディレクトリを指定するときは、直下に通常ファイル `.tsuzuri-docs` が要ります。4096 バイト以内で、中に `generated by tsuzuri doc` が含まれている必要があります。
3. **置換**: いったん一時ディレクトリへ全部書き、成功したら既存出力と入れ替えます。公開に失敗したときは旧出力の復元を試みます。復元できないときは、退避先のパスを診断に出します。
4. **拒否（`E2003`）**: 次のときは何も書き換えません。
   - マーカーの無い既存ディレクトリ
   - 出力先やマーカーがシンボリックリンク
   - ソース（`.tz`、`.tt`、`.tc`）を含むディレクトリ
   - ページ名が `index.md` と衝突する（モジュール `Index` など）
   - 大文字小文字だけが違うページ名が衝突する

> [!WARNING]
> 正しいマーカーがあるディレクトリは、ツールが管理する出力先です。全体が置き換わるので、手書きのメモを混ぜないでください。

## オプションと制限

- `--json`: コンパイル診断メッセージを標準エラー出力へ JSON lines 形式で出力します。ドキュメント本文を JSON 化するオプションではありません。
- **ビルド専用オプションの拒否（`E2000`）**: `--target`、`--emit`、`-O`、`--cpu`、`-g` などのコード生成関連フラグを `tsuzuri doc` に渡すと引数エラー `E2000` になります。
- **例の確認は `tsuzuri test --doc`**: `tsuzuri doc` はコメント内のコード例を実行しません。例を確かめるのは [`tsuzuri test --doc`](#tsuzuri-test---doc) です。
- **サニタイズ**: コメント内の Markdown や HTML はそのまま出力されます。第三者のドキュメントをブラウザ等でレンダリングする場合は、表示側で適切なサニタイズを実施してください。

## LSP ホバーとの関係

Tsuzuri の組み込み言語サーバー（`tsuzuri lsp`）は、ソースコード内の宣言に付与された `///` ドキュメントコメントを自動的に解析します。

LSP 対応のエディタで名前へカーソルを合わせると、定義位置の `///` が型シグネチャとともにホバーに出ます。例は `tsuzuri doc` と同じく、隠し行を除き、`run=` の出力と `fails=` のコードを添えて表示します（[tsuzuri doc とホバーでの表示](#tsuzuri-doc-とホバーでの表示)）。

## 他の言語との比較

| 項目 | Tsuzuri | Rust | F# | C# |
| --- | --- | --- | --- | --- |
| コメント構文 | `///` | `///` または `//!` | `///`（XML ドキュメント） | `///`（XML ドキュメント） |
| ドキュメント生成ツール | `tsuzuri doc` | `cargo doc` | `fsdocs` | `dotnet build` / DocFX |
| 書式形式 | Markdown | Markdown | XML / Markdown | XML |
| 誤配置のエラー | コンパイルエラー `E0002` | 警告（unused doc comment） | 無視または警告 | 警告（CS1587 等） |
| 出力先保護 | `.tsuzuri-docs` マーカー | `target/doc` に上書き | 指定フォルダに出力 | 指定フォルダに出力 |
| コード例の実行 | `tsuzuri test --doc`（`run=`・`test`・`fails=`・`ignore`、`# ` の隠し行） | `cargo test --doc`（`should_panic`・`compile_fail`・`ignore`、`# ` の隠し行） | なし | なし |

## まとめ

- `///` は直後の宣言へ説明を付けます。型検査やバイナリには影響しません。
- 先頭の空白 1 文字は除かれ、複数行は改行でつながります。Markdown もそのまま出せます。
- `def`、`record`、`union`、`type`、`const`、`class` に付けられます。実装や式へ付けると `E0002` です。
- 説明の行を `@deprecated` で始めると、別のパッケージや標準ライブラリからの使用が `W1005` の警告になります。同じパッケージの中では出ません。
- 説明の ```` ```tsuzuri ```` の例は `tsuzuri test --doc` が確かめます。`run=出力`・`test`・`fails=コード`・`ignore` で確かめ方を選び、記法の誤りは `E2011`、失敗は `E2006` です。
- `tsuzuri doc` は公開宣言だけを Markdown にします。`private` は出しません。
- 出力先は `.tsuzuri-docs` マーカーで保護されます。`Index` というモジュール名は `index.md` と衝突して `E2003` です。
- LSP のホバーは、定義に付いた `///` を表示します。

## 関連項目

- [アクセス制御](access-control.md)
- [モジュール](modules.md)
- [コンパイラの使い方](../compiler/usage.md)
- [診断メッセージとエラーコード](../compiler/diagnostics.md)
- [安定性と互換性の方針](../../../docs/stability.md)
- [言語リファレンスの目次](../index.md)

