# モジュール

Tsuzuri では 1 つのファイルが 1 つのモジュールです。ファイルの中に `module` を書いたり、1 つのモジュールを複数ファイルに分けたり、モジュールを入れ子にしたりはできません。ファイル名がモジュール名、ディレクトリが名前空間になります。

インポート文は要りません。プロジェクトルート以下のソースを、コンパイラが決まった順で全部読みます。

## この記事のポイント

- 1 ファイル = 1 モジュールであり、インラインのモジュール宣言や分割ファイルはありません。
- モジュール名（拡張子を除いたファイル名）は英大文字で始まります。`point.tz` は `E1011` です。
- 拡張子 `.tz`、`.tt`、`.tc` で、書ける宣言が分かれます。
- モジュールと同名の型（`Point.tz` の `record Point`）はモジュールの完全名そのものです。`Point.Point` と重ねると `E1004` です。
- 予約された 46 個の名前は、ルート直下のファイル名と、パスや名前空間の先頭要素には使えません。サブディレクトリのファイル名には使えます。
- 入口は、選んだルート直下の `Main.tz` だけです。

## 基本の書き方とファイル構成

モジュール名は、拡張子を除いたファイル名です。英大文字で始まる ASCII 識別子にします。`point.tz` は `E1011` で、`Point.tz` への改名が案内されます。`_` だけのファイル名も `E1011` です。ディレクトリ名は小文字でもかまいませんが、ASCII 識別子である必要があります。

次の例では、`Geometry/Point.tz` で定義した `Point` モジュールの関数を、`Main.tz` から修飾参照して呼び出しています。

```tsuzuri project=modules-basic file=Geometry/Point.tz
record Point { x: f64, y: f64 }

def make :: f64 -> f64 -> Point = \x y -> Point { x: x, y: y }
def distance_squared :: Point -> f64 = \p -> p.x * p.x + p.y * p.y
```

```tsuzuri project=modules-basic file=Main.tz run=25
let p = Geometry::Point.make 3.0 4.0
(Geometry::Point.distance_squared p) as i64
```

実行結果:

```text
25
```

別モジュールの関数は、`Geometry::Point.make` のように名前空間とモジュールを `::` で、メンバーを `.` でつなぎます。ルートの `Main.tz` から `Point.make` とは書けません。`Point` は `Geometry` の下にあるためです。同じ名前空間にいるときだけ、モジュール名だけで足ります。解決順は [名前空間](namespaces.md) を見てください。

## 拡張子ごとの役割

拡張子ごとに、そのファイルへ書ける宣言が決まっています。

| 拡張子 | 役割 | 記述できる内容 | 記述できない内容 |
| --- | --- | --- | --- |
| `.tz` | 通常のプログラムコード | `record`、`union`、`type`、`const`、`def` / `fn`、`instance`。`Main.tz` ではトップレベル式も可 | `class` 宣言 |
| `.tt` | 型クラス定義（Trait / Type class） | 複数の `class` 宣言およびデフォルトメソッド | レコード、union、型エイリアス、トップレベル関数、`instance`、トップレベル式 |
| `.tc` | コンピュテーション式ビルダー | ファイル名と同名のビルダー操作関数（`Return`、`Bind` など）、補助関数、定数、レコード、union、型エイリアス、`instance`、ビルダーの別名（`@alias`） | トップレベル式、ファイル名と異なるビルダー |

`.tz` や `.tc` の関数や型の個数、`.tt` の型クラスの個数に上限はありません。`.tc` にはビルダー操作（`Return` や `Bind` など）が 1 つ以上要ります。無いと `E1018` です。`.tz` に `class` を書くのも `E1018` で、`.tt` へ分けるよう案内されます。`@alias 名前` は `.tc` のビルダーに小文字の別名を足す宣言で、`.tz` や `.tt` に書くと `E1018` です（[ビルダーの別名](../computation-expressions/computation-expressions.md#ビルダーの別名)）。

> [!WARNING]
> 同じディレクトリに、拡張子だけが違うファイル（`Point.tz` と `Point.tt`、`Point.tc`）は置けません（`E1011`）。別ディレクトリの同名ファイルは別モジュールです。旧拡張子 `.tzr` は入力として拒否され、`E2000` で `.tz` への改名が案内されます。

## モジュールと同名の型

ファイル名と同じ名前の型（`record`、`union`、`type`、`extern type`）を定義した場合、その型の完全名はモジュールの完全名そのものと一致します。

例えば、`Drawing/Point.tz` の中に `record Point` を定義した場合、型の完全名は `Drawing::Point` になります。

```tsuzuri project=modules-samename file=Drawing/Point.tz
record Point { x: i64, y: i64 }

def scale :: Point -> i64 -> Point = \p factor ->
    Point { x: p.x * factor, y: p.y * factor }
```

```tsuzuri project=modules-samename file=Main.tz run=35
// Drawing::Point.Point ではなく Drawing::Point { ... } と書く
let p = Drawing::Point { x: 2, y: 3 }
let scaled = Drawing::Point.scale p 7
scaled.x + scaled.y
```

実行結果:

```text
35
```

このとき、`Drawing::Point.Point { x: 2, y: 3 }` や `Point.Point` のようにモジュール名と型名を重ねて記述すると、エラー `E1004` になります。

union の場合も同様です。`Shape.tz` 内で `union Shape = Circle of f64 | Rect of f64 * f64` を定義した場合、case コンストラクタは `Shape.Circle` や `Shape.Rect` と記述します（`Shape.Shape.Circle` は `E1004` です）。

コンパイラの診断メッセージでも、冗長さを避けるため `Point` や `Drawing::Point` として表示されます。

## 修飾参照とローカル変数によるシャドーイング

外部モジュールの関数や定数は、`Module.function` または名前空間を含めた完全修飾名 `Namespace::Module.function` で呼び出します。

ただし、スコープ内にモジュールと同名のローカル変数が存在する場合、`name.field` という記法は**ローカル変数のフィールドアクセス**として優先的に解釈されます。

```tsuzuri run=42
record Box { value: i64 }

// ローカル変数 Box がモジュール参照をシャドーイングする
let Box = Box { value: 42 }
Box.value
```

実行結果:

```text
42
```

モジュール側を参照したいときは、変数名を変えるか、モジュールの完全名で修飾します。既定名前空間が `App` なら `App::Main.Box` です。`Main::Box` だけでは届かないことがあります。

## 予約モジュール名

標準ライブラリ（std）用に、以下の 47 個のモジュール名が予約されています。

```text
Maybe       Result      Array       List        Vec         String
Utf8String  Char        Utf8Char    Math        Int         Debug
Parallel    Simd        Map         Set         HashMap     HashSet
Seq         Test        Gpu         IO          Owned       File
Dir         Path        Env         Time        Random      Os
Process     Net         Format      Exception   BigInt      FixedArray
Dyn         Arena       Rc          Arc         Regex       Unicode
Json        Cbor        Bench       Gen         Async
```

`FixedArray`、`Dyn`、`Rc`、`Arc` は、対応するソースが無くても予約です（組み込みの関数と型だけを持ちます）。次の位置に使うと `E1011` です。

- ルート直下のファイル名（`Maybe.tz`）
- ディレクトリパスの先頭要素（`Maybe/Foo.tz`）
- 名前空間の先頭要素（`namespace Maybe` や、`Tsuzuri.toml` の既定名前空間）

サブディレクトリのファイル名（`Tools/Maybe.tz`）や、先頭以外の名前空間要素（`namespace App::Maybe`）には使えます。関数名やレコード名にも使えます。自ファイルで `record Result` を宣言すると、そのファイルではローカル宣言が標準ライブラリより優先されます。標準側は `std::Result` と書きます。

`Task` は上の一覧にはありませんが、組み込みの型と名前空間です。モジュール名にも、パスのどの要素にも使えません。`_` 単独と予約語（ディレクトリ名 `match` など）も、パスの各要素では `E1011` です。

## 探索の規則

ルート以下の `.tz`、`.tt`、`.tc` を再帰的に読み、正規化した相対パスのバイト順で検査します。参照されていなくても対象です。ドットで始まるファイルとディレクトリは無視します。ソースやディレクトリのシンボリックリンクは `E1011` です。

パスは最大 16 要素、名前は 255 バイト、ソースは 4096 件、ディレクトリは 1024 個までです。超えると `E1017` です。

入力がディレクトリなら、そのディレクトリがルートです。単一ファイルなら親ディレクトリがルートで、さらに上のプロジェクトは推測しません。隣接した別の例を、同じルートに混ぜないでください。

## Main.tz とエントリーポイント

`tsuzuri run` と実行ファイルのビルドでは、**選んだルート直下の `Main.tz`** だけが入口です。親プロジェクトから見た `App/Main.tz` は `App::Main` であり、親の入口にはなりません。`App` ディレクトリ自体を渡すと、そちらがルートになり、その直下の `Main.tz` が入口です。他ファイルの `main` は普通の関数です。`namespace` の有無は入口の選定を変えません。

`check` やライブラリ出力では `Main.tz` は必須ではありません。

`Main.tz` の入口は、次のどちらか一方です。両方書くと `E2004` です。`Main.tz` 以外のモジュールでトップレベルの式を実行しようとしても `E2004` です（`def` の実装を書く `let` / `fn` は宣言の一部なので書けます）。

### def main による形式

終了コードを返す形式です。戻り値はコンソールに出ません。

```tsuzuri run=Hello%2C%20Tsuzuri!
def main :: unit -> i32 = \() ->
    do! IO.write_line "Hello, Tsuzuri!"
    0
```

実行結果:

```text
Hello, Tsuzuri!
```

コマンドライン引数を受け取る場合は、引数を `Array<string>`（または `[string]`）で受け取ります。

```text
def main :: Array<string> -> i32 = \args ->
    do! IO.write_line ("arguments count: " + to_string args.length)
    0
```

- `args` にプログラム名（`argv[0]`）は入りません。引数が無ければ空配列です。
- `"..."` で囲んだ空白は 1 つの引数になります。囲んでいた引用符は外れます。
- シグネチャがこの 2 つ以外（`i32`、`unit -> i64`、`IO<unit>` など）だと `E2004` です。
- `def` の無い `fn main` は構文エラー `E0002` です。
- `tsuzuri run` は子プロセスへ追加引数を渡しません。`main` には空配列が渡ります。引数を試すときは、`tsuzuri build` したバイナリを直接実行します。
- 終了コードが 0 以外だと、`tsuzuri run` は `E2005`（`program exited with code N`）を出して、自分は終了コード 1 で終わります。

### トップレベル式による形式

`Main.tz` では、宣言のあとに `let` や式を直接書けます。末尾の結果式は、型によって表示が変わります。

```tsuzuri run=42
let a = 20
let b = 22
a + b
```

実行結果:

```text
42
```

数値、`bool`、`string`、`utf8string`、`char`、`utf8char` は表示されます。`unit` は何も出ません。それ以外は `Display` があればその文字列を出し、無ければ値を捨てて終了します（エラーにはなりません）。

末尾が `IO<T>` なら、そのアクションを 1 回実行し、値は表示しません。`IO<i32>` の結果は終了コードになります。それ以外の `IO<T>` は終了コード 0 です。

## 他の言語との比較

| 項目 | Tsuzuri | Rust | F# | Go |
| --- | --- | --- | --- | --- |
| モジュール定義 | 1 ファイル = 1 モジュール | `mod foo;` や `mod.rs` で明示 | `module Foo` 宣言 | ディレクトリ単位（package） |
| 同名の型参照 | `Point`（`Point.Point` は禁止） | `Point`（`point::Point`） | `Point`（`Point.Point`） | `point.Point` |
| 特殊ファイルの分離 | `.tz` / `.tt` / `.tc` で分離 | 1 つの `.rs` に混在可能 | 1 つの `.fs` に混在可能 | 1 つの `.go` に混在可能 |
| インラインモジュール | なし | `mod foo { ... }` あり | `module Foo = begin ... end` あり | なし |

## まとめ

- Tsuzuri では 1 つのファイルが 1 つのモジュールを定義し、ファイル名が大文字始まりのモジュール名になります。
- 通常コードは `.tz`、型クラスは `.tt`、コンピュテーション式ビルダーは `.tc` に記述します。
- モジュールと同名のレコードや共用体はモジュール完全名そのもので参照し、モジュール名を重ねて書くことはできません。
- 予約された 46 個の名前は、ルートのファイル名とパスや名前空間の先頭には使えません。サブディレクトリのファイル名には使えます。型名にも使えますが、組み込みの型と同じ名前の `Array`・`Vec`・`Rc`・`Arc` は `E1001` です。
- 入口は選んだルート直下の `Main.tz` です。`def main` かトップレベル式のどちらか一方で書きます。

## 関連項目

- [名前空間](namespaces.md)
- [using 宣言](using-declarations.md)
- [パッケージ](packages.md)
- [アクセス制御](access-control.md)
- [コンパイラの使い方](../compiler/usage.md)
- [言語リファレンスの目次](../index.md)
