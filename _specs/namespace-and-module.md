# namespace と module

## namespace

名前空間を使用すると、Tsuzuri プログラム要素のグループに名前を割り当てることができるため、コードを関連する機能領域ごとに整理することができます。通常、名前空間は Tsuzuri ファイルにおける最上位の要素です。
また、`Tsuzuri.toml` ファイルにプロジェクトの既定 `namespace` を宣言できます。`Tsuzuri.toml` 内に宣言がない場合は、プロジェクト名 (= ルート フォルダー名) となります。また、VSCode 拡張機能やコマンドラインで作成したプロジェクトには、必ず `Tsuzuri.toml` 内に `namespace` が設定されます。

構文:

```tz
namespace [parent-namespaces.]identifier
```

例1:

```Main.tz
namespace Sample

def main :: i32 = \() ->
  do! IO.writeln "Hello, Tsuzuri!!" |> ignore
  0
```

例2:
```Foo.tz
namespace Sample.Codebase

def greet :: string -> unit = \text ->
  do! IO.writeln text |> ignore
```

### remarks

コードを名前空間に配置したい場合、ファイル内の最初の宣言で名前空間を宣言する必要があります。
名前空間はあくまでもモジュールがどの名前空間に属するかを決めるものであるため、`namespace`　を設定したとしても、ファイル内で宣言した値や関数などは、すべてそのファイル (= モジュール) に属します。

### using 宣言

`namespace` 宣言を除く、ファイルの先頭に `using {namespace}` を宣言することで、namespace 付きの毎回完全修飾での参照をしなくてもよくなります。

```Shape.tz
namespace Sample.Features

union Shape =
    | Circle of f64
    | Rect of f64 * f64
    | Empty

union Maybe<'a> = None | Some of 'a

def area :: Shape -> f64 = \shape ->
    match shape with
    | Circle r -> r * r * 3.141592653589793
    | Rect (w, h) -> w * h
    | Empty -> 0.0

```

```Main.tz
namespace Sample

using Sample.Features // using は namespace 宣言よりも後に記述する必要がある

def main :: i32 = \() ->
  Shape.area (Shape.Rect (3.0, 4.0)) // using で宣言した `Sample.Features` が省略できるため、`Sample.Features.Shape.area` を `Shape.area` などのように記述できる
  |> ignore

  0
```

## module

モジュール名は拡張子を除いたファイル名で決まり、1 ファイルに 1 モジュールを強制します。
`module` 宣言、入れ子のモジュール、複数ファイルへの同一モジュールの分割はできません。

ファイル内にモジュール名と一致する `record` または `union` が定義されている場合、それらの完全名は `namespace 名 + record 名` または `namespace 名 + union 名` となります。例えば、以下のようになります。

```Shape.tz
namespace Sample

union Shape =
    | Circle of f64
    | Rect of f64 * f64
    | Empty

union Maybe<'a> = None | Some of 'a

def area :: Shape -> f64 = \shape ->
    match shape with
    | Circle r -> r * r * 3.141592653589793
    | Rect (w, h) -> w * h
    | Empty -> 0.0

```

```Point.tz
namespace Sample

record Point { x: f64, y: f64 }

def distance :: Point -> f64 = \point -> 
  sqrt (point.x * point.x + point.y * point.y)
```

```Main.tz
namespace Sample

def main :: i32 = \() ->
  Sample.Shape.area (Sample.Shape.Rect (3.0, 4.0)) // Sample.Shape.Shape.area などのように `namespace + module + union` とはならない
  |> ignore

  // Shape.area (Rect (3.0, 4.0)) // 同じ namespace に所属しているため、`Sample` を省略しても良い
  // |> ignore

  let p = Sample.Point { x: 10.0, y: 20.5 } // Sample.Point.Point {} などのように `namespace + module + record` とはならない
  Sample.Point.distance p |> ignore

  // let p = Point { x: 10.0, y: 20.5 } // 同じ namespace に所属しているため、`Sample` を省略しても良い
  // Point.distance p |> ignore

  0
```

Module 名は基本的に大文字始まりになるようにファイル名をつける必要があります。

