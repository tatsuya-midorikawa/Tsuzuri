# モジュール、名前空間、可視性、ローカルパッケージ

[ドキュメントのトップ](../README.md)

一つのソースファイルが一つのモジュールです。モジュールは名前空間に属し、名前空間はファイル先頭の `namespace` 宣言かディレクトリから決まります。モジュール間の関数呼び出しは修飾名で明示し、`using` で名前空間の修飾を省けます。`export` はモジュール公開ではなくホスト ABI への公開です。

## 複数ファイルの例

root から見た `Geometry/Point.tz`:

```tsuzuri project=modules file=Geometry/Point.tz
record Point { horizontal: f64, vertical: f64 }

private def squared_length :: ref Point -> f64 = \point -> point.horizontal * point.horizontal + point.vertical * point.vertical

def distance :: ref Point -> f64 = \point -> sqrt (squared_length point)
```

root 直下の `Main.tz`:

```tsuzuri project=modules file=Main.tz run=5
let point = Geometry::Point { horizontal: 3.0, vertical: 4.0 }
Geometry::Point.distance ref point
```

このモジュールは名前空間 `Geometry` の `Point` で、`Geometry::Point` と書きます。名前空間とモジュールは `::`、モジュールとその中の名前は `.` でつなぎます。モジュールと同じ名前の `record Point` の完全名も `Geometry::Point` です。型も関数も完全修飾できます。別モジュールの関数は修飾必須ですが、レコード、union、case、クラスには自モジュール優先と一意な公開候補による無修飾の解決があります。

ローカル値がモジュールと同名なら `name.field` はローカル値のフィールドアクセスを優先します。名前が曖昧なら修飾を増やして解決します。

モジュール名（拡張子を除いたファイル名）は英大文字で始めます。`point.tz` は `E1011` です。ディレクトリ名は小文字でも構いません。

## 名前空間

ファイルの最初の宣言に `namespace` を書くと、そのファイルのモジュールが属する名前空間を指定できます。モジュールの完全名は名前空間とファイル名を `::` でつないだもので、入れ子の名前空間も `namespace Sample::Codebase` のように `::` でつなぎます。関数やレコードなどの宣言は名前空間ではなく、常にモジュールに属します。

```tsuzuri project=namespaces file=Shape.tz
namespace Sample

union Shape =
    | Circle of f64
    | Rect of f64 * f64

def area :: Shape -> f64 = \shape ->
    match shape with
    | Circle r -> r * r * 3.0
    | Rect (w, h) -> w * h
```

```tsuzuri project=namespaces file=Point.tz
namespace Sample

record Point { x: f64, y: f64 }

def sum :: Point -> f64 = \point -> point.x + point.y
```

```tsuzuri project=namespaces file=Main.tz run=24
namespace Sample

def main :: f64 = \() ->
    let p = Sample::Point { x: 1.0, y: 2.0 }
    let q = Point { x: 3.0, y: 4.0 }
    Sample::Shape.area (Sample::Shape.Rect (3.0, 4.0)) + Shape.area (Rect (1.0, 2.0)) + Sample::Point.sum p + Point.sum q
```

- `Sample::Shape.area` が完全名です。`Sample::Shape.Shape.area` のように名前空間・モジュール・union を重ねることはできません（`E1004`）。
- 同じ名前空間のファイルからは `Sample` を省略できます。参照は、自分の名前空間、その外側の名前空間、グローバルの順に探します。
- モジュール名と同じ名前の record／union の完全名はモジュールの完全名で、`Sample::Point { ... }`・`Sample::Shape.Rect` と書きます。`Sample::Point.Point` のようにモジュール名を重ねると `E1004` で、診断の型の表示も `Sample::Point` です。修飾しない `Point` も同じ順序で探すので、別の名前空間に `Other::Point` があっても曖昧になりません。
- `Sample.Shape.area` のように名前空間を `.` でつなぐことはできません（`E1002`／`E1004`）。診断は `::` を使う書き方を示します。
- `namespace` 宣言のないファイルは、パッケージの既定名前空間に root からのディレクトリを続けた名前空間に属します。既定名前空間は `Tsuzuri.toml` の `namespace`、なければ package 名の PascalCase、manifest がなければ root フォルダー名です。
- `namespace` は単独の行に書く最初の宣言です。キーワードの後やパスの途中では改行できず、パスの要素は空白を挟まない `::` でつなぎます（`using` も同じ）。途中の `namespace` や `.` でつないだパスは `E0002`、同じ完全名のモジュールが 2 つあると `E1011` です。入口は名前空間にかかわらず root 直下の `Main.tz` です。

`using 名前空間` は `namespace` 宣言の後、他の宣言の前に書きます。その名前空間の直下のモジュールを、名前空間を省いて参照できます。

```tsuzuri project=using file=Features/Shape.tz
namespace Sample::Features

union Shape =
    | Circle of f64
    | Rect of f64 * f64

def area :: Shape -> f64 = \shape ->
    match shape with
    | Circle r -> r * r * 3.0
    | Rect (w, h) -> w * h
```

```tsuzuri project=using file=Main.tz run=12
namespace Sample

using Sample::Features

def main :: f64 = \() -> Shape.area (Shape.Rect (3.0, 4.0))
```

- 自分の名前空間に同名のモジュールがあればそちらを優先し、`using` で見つからなければ外側の名前空間を探します。入れ子の名前空間は取り込みません。
- 二つの `using` が同じ名前のモジュールを取り込むと、その名前の使用が `E1004` です。修飾しない型名としての使用も同じです（ファイル自身がその型を宣言している場合を除く）。名前空間で修飾してください。
- 存在しない名前空間、モジュールを指す `using`、重複した `using` は `E1011` です。

新しいプロジェクトは `tsuzuri new <directory> [--namespace NAME]` で作れます。`Tsuzuri.toml` に `namespace` を書き、`Main.tz` は同じ名前空間を宣言します。

## root の決定と探索

| 入力 | root と入口 |
| --- | --- |
| ディレクトリ | そのディレクトリが root。入口は直下の Main.tz |
| ソースファイル | 親ディレクトリが root。上位のプロジェクトを推測しない |
| ネストした App/Main.tz | 外側 root からは App::Main。外側プロジェクトの入口ではない |

root 配下の全 `.tz` / `.tt` / `.tc` を再帰探索し、相対パス順に検査します。未参照のファイルも対象です。隣接する独立例を同じ root へ混在させないでください。

dot で始まるファイル・ディレクトリは無視します。ソースやディレクトリの symlink は拒否します。パス要素は ASCII 識別子で、予約語、`_` 単独、Task は使えません。

同じ相対パスでは拡張子が違っても同じファイル名本体を併存させられません。別ディレクトリの同名ファイルは別モジュールです。名前空間の指定は `namespace` と `using` だけで、module / open / import 宣言、任意の検索パス、同一モジュールのファイル分割はありません。

## public と private

宣言は既定で public です。`private def`, `private record`, `private union`, `private type`, `private const` は宣言モジュール内だけで参照できます。private union の case も private です。

分離形式の fn / let の実装も def の可視性を引き継ぎます。`private fn` とは書きません。public の関数・型・制約から同じモジュールの private 型を漏らすこともできません。違反は `E1022` です。

型クラスとインスタンスは常に public です。ビルダー操作は private にできませんが、補助関数は private にできます。可視性は名前解決の規則で、ランタイム上のアクセス制御ではありません。

## 標準ライブラリ

標準ライブラリはコンパイラに埋め込まれ、利用者のソースの後に読み込まれます。未使用でも型検査しますが、到達しない標準関数などは生成 IR から除去します。

利用者の宣言は同名の標準宣言より優先しますが、標準ライブラリ内部から利用者の宣言を探索することはありません。std の private 関数にはアクセスできません。

以下は標準ライブラリのモジュール名として予約されており、利用者のモジュールのパスの先頭要素（既定名前空間の直下の最初の要素）には使えません。名前が予約されていることと、同名のソースファイルや API がすべて存在することは同義ではありません。

```text
Option Result Array List Vec String Utf8String Char Utf8Char Math Int
Debug Parallel Simd Map Set HashMap HashSet Seq Test Gpu IO Owned
File Dir Path Env Time Random Os Process Format
```

利用者のソースがこれらの名前（たとえば `Path.tz` や `Format.tz`）をモジュール名にすると `E1011`（`reserved for the standard library`）で拒否します。

## ローカルパッケージ

root の `Tsuzuri.toml` に path 依存を指定できます。

```toml
[package]
name = "app"
version = "0.1.0"
namespace = "Acme::App"

[dependencies]
geometry-core = { path = "../geometry-core" }
```

依存先にも name / version を持つ manifest が必要で、依存キーは実際の name と一致させます。name は小文字 ASCII kebab-case、各要素は英字始まりで最大 255 byte です。version は非空文字列ですが、版解決には使いません。

省略できる `namespace` はパッケージの既定名前空間で、`Acme::Tools` のように識別子を `::` でつないだものです（`.` 区切りは `E1011`）。省略すると name の PascalCase になり、`geometry-core` の名前空間は `GeometryCore`、依存の `Point.tz` は `GeometryCore::Point` です。依存内部からもその完全修飾名を使います。root 自身のモジュールには package prefix を付けず、依存の Main は入口になりません。

同じ正規化 root は共有し、循環、同名の別 root、名前空間の衝突、symlink は拒否します。ネストした依存 root を親パッケージのソースとして二重に読みません。

root の manifest には WASM の build・test の既定値を書けます。値はコマンドラインの `--wasm-max-memory`・`--wasm-stack-size` と同じ書式の文字列で、コマンドラインの指定が優先します。依存パッケージの `[wasm]` は読みません。

```toml
[wasm]
max-memory = "256MiB"
stack-size = "4MiB"
```

## manifest の文法と制限

上の section と key だけを認める限定 TOML です。`[package]` の key は `name`・`version`・`namespace` です。`[package]` を最初に置き、`[dependencies]` と `[wasm]` はその後にそれぞれ一度まで、順序は問いません。コメント、空行、CRLF、引用符付き UTF-8 文字列と対応 escape を使えますが、任意の TOML 構文を受け付けるわけではありません。未知の key・構文は `E0002` です。

registry、git 依存、ネットワーク取得、lockfile、版解決、build script は未実装です。manifest がなければ従来の探索規則を使います。

探索の上限は、モジュール名 16 要素 / 255 byte、4,096 ソース、1,024 ディレクトリです。依存グラフは 1,024 パッケージ、深さ 128、全体で 4,096 ソースまでです。超過は `E1017` です。manifest も生成物による上書き保護の対象です。

## 関連項目

- [ファイルの種類](lexical-and-layout.md)
- [型クラスのモジュール関数制約](generics-and-typeclasses.md)
- [ホスト関数の仕様](../../docs/language.md#ホスト関数のインポート)
