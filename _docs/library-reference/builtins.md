# 基本組み込み関数、Debug、Owned、Test

[ドキュメントのトップ](../README.md)

組み込み関数にも通常の型・所有権・適用規則があります。無修飾の互換用関数と、新しいモジュール付き API を区別してください。

## 無修飾の組み込み関数

| 名前 | シグネチャと用途 |
| --- | --- |
| `sqrt` | `f64 -> f64`。負数は NaN |
| `floor`, `ceil`, `abs` | `f64 -> f64`。整数用ではない |
| `to_float` | `i64 -> f64`。必要に応じて丸める |
| `to_int` | `f64 -> i64`。切り捨て、飽和、NaN は 0 |
| `clone_string` | `ref string -> string`。UTF-16 の独立複製 |
| `to_string` | `Display<T> => T -> string`。消費する表示 |
| `assert` | `bool -> unit`。false でトラップ |
| `unreachable` | `unit -> T`。必ずトラップ |
| `not` | `bool -> bool`。`!` と同じ論理否定 |
| `ignore` | `T -> unit`。引数を受け取って解放し、何も返さない |

sqrt / floor / ceil / abs は f64 固定の互換名です。型汎用版は Math、整数の絶対値は Int を使います。基本組み込み名をトップレベルの関数名として再定義することはできません。

assert は最適化レベルにかかわらず契約違反を検査する操作で、`-O3` なら無効になるという規則ではありません。unreachable は任意の型を満たせますが、実行すると終了する部分関数です。

not と ignore は `|>` の右辺にも書けます。`do! action |> ignore` は IO の結果を捨てます（[IO の直接形式](io.md#直接形式)）。

```tsuzuri run=true
let finished = false
ignore (Array.sum [1, 2])
assert (not finished)
finished |> not
```

## Debug

```tsuzuri
let answer = 42
Debug.print ref answer
Debug.trace answer
```

`Debug.print :: Display<T> => ref T -> unit` は入力を借用して表示し、`Debug.trace :: Display<T> => T -> T` は入力を消費して表示後に同じ所有値を返します。

native では UTF-8 と改行を stderr へ出します。WASM は既定で no-op であり、デバッグ用 import を増やしません。明示的な debug-output オプションを使う場合にホスト出力へ接続します。

no-op でも引数と Display の評価、および表示用文字列の解放は行います。Display 内のトラップは消えません。接続方法は[Debug のホスト契約](../../docs/language.md#デバッグ出力)を参照してください。

## Owned

```tsuzuri run=4
let text = "abc"
let length = Owned.function (\extra -> String.length (ref text) + extra)
Owned.call (ref length) 1
```

| API | 型と用途 |
| --- | --- |
| `Owned.drop value` | `T -> unit`。値を消費してその場で解放する。`Drop` を持つ型なら `drop` が走る |
| `Owned.function body` | `(A -> B) -> Owned.Function<A, B>`。引数に直接書いたラムダは `Drop` を持つ値も捕捉できる |
| `Owned.call (ref f) x` | `ref Owned.Function<A, B> -> A -> B`。環境を借用したまま呼ぶ |

`Owned.Function` は Copy ではない opaque 型で、通常の関数値に捕捉できません。直接書いたラムダは引数が一つで、所有値だけを捕捉し、本体は捕捉した値を読むか借用するだけです。値の終わりに捕捉した値を一度だけ解放します。詳しくは[所有権](../language-reference/ownership.md#資源を持つ関数値)を参照してください。

## Test

```tsuzuri run=42
test "checks integer equality" = {
    let actual = 40 + 2;
    let expected = 42;
    Test.equal (ref actual) (ref expected)
}

42
```

| API | 型・失敗 |
| --- | --- |
| `Test.equal left right` | `Eq<T> => ref T -> ref T -> unit`。異なれば assert 失敗 |
| `Test.not_equal left right` | 同じなら assert 失敗 |
| `Test.is_true condition` | bool が false なら assert 失敗 |

比較のために値を消費しません。テスト実行は test 宣言と CLI の test コマンドを使います。失敗を例外として捕捉するフレームワークではなく、トラップした実行をランナーが分離して扱います。

## 名前解決

モジュール付きの名前は、ソース定義の関数、同名の組み込み関数、型クラスメソッドの順で解決します。std ソースで同じ修飾名の組み込みを上書きすることはできません。内部実装用の名前は公開 API として呼び出さないでください。

## API と関連項目

- [Debug の宣言](api/Debug.md)、[Test の宣言](api/Test.md)
- [Math](math.md)、[Int](integers.md)
- [表示と解析](formatting-and-parsing.md)
