# F# に慣れた利用者のための Tsuzuri

[ドキュメントのトップ](../README.md)

関数の空白適用、パイプと関数合成、パターンマッチ、コンピュテーション式などの考え方は似ていますが、Tsuzuri は F# の方言や .NET のフロントエンドではありません。コードを移すときは、表記だけでなく評価時点と所有権を見直します。

## 構文の対応

| 意図 | F# の代表的な表記 | Tsuzuri |
| --- | --- | --- |
| 名前付き関数 | `let add left right = ...` | `def add :: 型 = \left right -> ...` |
| 匿名関数 | `fun value -> ...` | `\value -> ...` |
| 可変ローカル | `let mutable count = 0` | `let mut count = 0` |
| 代入 | `<-` | `=` |
| 等値・不等値 | `=` / `<>` | `==` / `!=` |
| 配列 | `[\|1; 2\|]` | `[1, 2]` |
| リスト | `[1; 2]` | `[\|1, 2\|]` |
| 判別共用体 | type に case を列挙 | union に case を列挙 |
| レコード | type とフィールド | record と明示的な型 |
| モジュール | module / namespace / open | ファイルとディレクトリ、修飾名 |
| 文字列補間 | `$"hello {name}, {count + 1}"` | 同じ形の `$"hello {name}, {count + 1}"`。穴の値は借用され、書式は `{x:.2}` のように穴の中へ書く（`%d` 形式ではない） |
| 関数合成 | `>>` / `<<` | 同じ。`f >> g` は `\x -> g (f x)` |
| ビット演算 | `&&&` / `\|\|\|` / `^^^` / `~~~` / `<<<` / `>>>` | 同じ。`>>>` は符号付きなら算術、符号なしなら論理シフト |
| 否定・値の破棄 | `not` / `ignore` | 同じ修飾なしの関数。`!` も使える |
| 累乗 | `**` | 同じ。浮動小数点に加え整数と bigint にも使える |
| 例外 | `try ... with` / `try ... finally` | `try ... with ... finally`。値は `Result` で、捕捉するのは `@checked` のオーバーフローだけ |
| コレクション関数 | `xs \|> Array.map f` | 同じく関数が先 |
| 数値リテラル | `100y` / `3u` / `5L` / `1I` / `1.5f` | 同じ形の短い接尾辞。ただし `l` は i64、`L` は i128（後述） |

上表は代表的な記法の比較で、機械的な全構文変換表ではありません。

## 型推論と多相性

Tsuzuri の名前付き関数はシグネチャを明示します。ローカル let は単相で、すべての関数値を自動一般化するわけではありません。接尾辞のない整数リテラルは Rust のように使われ方から型を決め、どこからも決まらなければ F# と同じ i32 です。例えば `let values = [3, 5, 8]` を後で i64 の関数へ渡せば `[i64]` になります。i32 の範囲外で型が決まらないリテラルはエラーなので、`l` などの接尾辞を付けます。小数の既定は f64 です。

型クラスで演算を抽象化し、コンパイル時の単相化で具体化します。class というキーワードは .NET オブジェクトの class 宣言ではありません。継承、仮想メソッド、リフレクションを意味しません。

## 所有権を API に表す

F# の GC で管理する値の共有を、Tsuzuri の値渡しへそのまま移すことはできません。string は非 Copy で move し、Copy 配列を複製すれば独立したバッファになります。

```tsuzuri run=10
def size :: ref string -> i64 = \text -> text.length

let text = "hello"
size text + size text
```

ref は F# の参照セルではなく共有借用です。ref mut は排他借用であり、複数箇所から同じ可変状態を自由に変更する許可ではありません。関数の引数型が分かれば借用を省略できても、寿命と競合は検査します。

クロージャーが捕捉する値はスナップショットです。外側の let mut を関数内から変更する一般的なパターンは使えません。必要な状態は引数と返却値で明示的に受け渡します。

## Maybe、Result、例外

Maybe / Result と match は利用できます。F# の `Option` に当たる型は `Maybe` で、case は同じ `None` / `Some` です。match の網羅性不足はコンパイルエラーです。

`try ... with ... finally` も書けますが、F# の例外処理とは意味が異なります。

- 式の型は `Result<'T, 'E>` です。本体の値は `Ok`、ハンドラーの結果は `Error` になり、ハンドラーの結果は `Err` を実装する型です。
- 捕捉できる例外は `@checked` の中の整数オーバーフロー（`OverflowException`）だけです。`exception` 宣言、`raise`、`failwith` はありません。
- 例外は字句的で、同じ関数本体の最も内側の `try` へ移ります。呼び出し先の関数やラムダの中の例外は捕捉せず、巻き戻しもありません。どの `try` も捕捉しなければトラップします。
- get の不一致、assert、整数ゼロ除算、境界違反はトラップであり、`try` でも回復しません。

```tsuzuri run=Arithmetic%20operation%20resulted%20in%20an%20overflow.
def add_checked :: i32 -> i32 -> Result<i32, Exception> = \left right ->
    try
        @checked left + right
    with
    | e is OverflowException -> e

match add_checked 2147483647 1 with
| Ok value -> $"{value}"
| Error e -> e.msg
```

詳しくは[例外処理](../language-reference/error-handling.md)を参照してください。

IDisposable に当たるのは、record・union に書く `instance Drop<T>` と lexical な解放です。Drop を持つ値は scope の終わりや置き換えのときに一度だけ `drop` が呼ばれ、`Dispose` のように明示的に呼ぶことはできません（[利用者定義の解放](../language-reference/ownership.md#利用者定義の解放drop)）。F# と同じく `use` / `use!` で束縛でき、値の型が Drop を持つことを検査します。早く解放するには `Owned.drop value` を呼びます。Drop を持つ値を捕捉するクロージャーは `Owned.function` で作り、`Owned.call` で呼びます。計算式の `let!` より後ろは継続の関数なので、その前に `use` した値を後ろで使えません。`finally` は正常終了と例外のどちらでも実行する後始末ですが、.NET の IDisposable そのものは未対応です。回復可能な失敗には Result を使います。トラップしたときは `drop` も走りません。

## コレクション関数とパイプライン

標準の高階関数は F# と同じく関数を先、配列・リスト・Seq・Map などを最後に受け取ります（`Array.map f xs`、`Array.fold f state xs`、`Array.fold_back f xs state`、`Map.fold f state map`）。F# と同じ形のパイプラインと関数合成を書けます。

```tsuzuri run=15%2024%201024%20true
def double :: i64 -> i64 = \x -> x * 2
def increment :: i64 -> i64 = \x -> x + 1

let values = [1, 2, 3]
let total = values |> Array.map (double >> increment) |> Array.sum
let flags = (0b1100 &&& 0b1010) ||| (1 <<< 4)
$"{total} {flags} {2 ** 10} {not (total > 20)}"
```

`values |>` は values を借用して渡し、途中の配列は次の呼び出しの間だけ借用してから解放します。非 Copy の要素は、要素を借用する `map_ref` / `fold_ref` などで扱います。Maybe / Result の `bind` / `bind_ref` は F# と異なり計算値が先です。

## 計算式とタスク

ビルダーはオブジェクトではなく .tc のモジュールです。Return / Bind などはカリー化された通常の関数で、Zero は引数なしです。Maybe / Result の For は Copy 要素の配列を受けます。

and! は右辺を順に評価して結合し、自動並列起動しません。task 内の and! は提供しません。並列化には Task.parallel を明示します。

Tsuzuri の task は cold・一回実行・非 Copy です。Task.run は同期的で、F# の通常の hot task や .NET Task の scheduler / awaiter との互換はありません。

Seq も一回消費です。`seq { ... }` という組み込みビルダーや再列挙可能な IEnumerable の代わりと決めつけず、Seq.unfold / defer や明示 iter を使います。`Seq.unfold generator state`、`Seq.map f seq`、`Seq.filter p seq` の引数順は F# と同じです。

## 数値と文字列

通常の整数演算は F# と同じく折り返します。F# の `Checked` 演算子に当たるのは `@checked` で、付けた式や文に直接書いた `+`、`-`、`*`、`**` と単項 `-` だけを検査し、呼び出し先の関数の演算は検査しません。浮動小数点から整数への変換は飽和と NaN から 0 です。整数同士の cast は bit 保持・切り詰め・拡張です。F# / .NET の変換と同一視しません。

数値リテラルの短い接尾辞は F# と同じ形です。`y`、`uy`、`s`、`us`、`u`、`I`（bigint）、`f`（f32）、`'A'B`（byte）、`"text"B`（`[byte]`）は F# と同じ型です。`l` は i64、`ul` は i64u、`L` / `UL` は i128 / i128u、`F` は f128、`m` / `M` は d64 / d128 で、F#（`l` は int32、`L` / `UL` は 64-bit、`F` は float32、`m` / `M` は decimal）とは異なります。

```tsuzuri run=100%203%205000000000%2024691357802469135780%2065
let small = 100y
let count = 3u
let wide = 5000000000l
let huge = 12345678901234567890I
let letter = 'A'B
$"{small} {count} {wide} {huge * 2I} {letter}"
```

任意精度の `bigint` は [BigInt](../library-reference/bigint.md) を参照してください。

string は UTF-16 で、添字の返却型は i16u です。utf8string は別型で、索引はバイトです。

F# と同じ形の `$"..."` 補間が使えます。ただし穴の値は消費せずに借用するので、同じ string を何度でも埋め込めます。書式は .NET の書式文字列（`N2` など）や printf 風の `%d` ではなく、穴の中に `{x:.2}`、`{x:>8}` のように書きます（[書式指定](../library-reference/formatting-and-parsing.md#書式指定)）。

```tsuzuri run=Ada%20x2%2C%20Ada%20x3
def greet :: ref string -> i64 -> string
fn greet name count = $"{name} x{count}"

let who = "Ada"
let first = greet (ref who) 2
$"{first}, {greet (ref who) 3}"
```

printf 風の書式文字列、.NET String の全メソッド、文化圏依存比較はありません。

## 提供しない F# / .NET 機能

オブジェクト指向の class / interface / 継承、型プロバイダー、単位付き型、コードクォート、`[<...>]` の属性（`@checked` と `@literal` は専用の構文）、`exception` 宣言と `raise`、reflection、LINQ query、null、F# Interactive を前提とするコードは直接移植できません。

Tsuzuri が担当するのは型付きの計算と所有する状態です。外部機能は C / WASM ホストへ置き、export / extern の ABI を通して接続します。

## 関連項目

- [所有権](../language-reference/ownership.md)
- [API 設計とスタイル](style-and-design.md)
- [対応状況](../feature-status.md)
