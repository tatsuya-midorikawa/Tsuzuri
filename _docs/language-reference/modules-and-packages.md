# モジュール、可視性、ローカルパッケージ

[ドキュメントのトップ](../README.md)

一つのソースファイルが一つのモジュールです。階層はディレクトリから決まり、モジュール間の関数呼び出しは修飾名で明示します。`export` はモジュール公開ではなくホスト ABI への公開です。

## 複数ファイルの例

root から見た `Geometry/Point.tz`:

```tsuzuri project=modules file=Geometry/Point.tz
record Point { horizontal: f64, vertical: f64 }

private def squared_length :: ref Point -> f64
fn squared_length point = point.horizontal * point.horizontal + point.vertical * point.vertical

def distance :: ref Point -> f64
fn distance point = sqrt (squared_length point)
```

root 直下の `Main.tz`:

```tsuzuri project=modules file=Main.tz run=5
let point = Geometry.Point.Point { horizontal: 3.0, vertical: 4.0 }
Geometry.Point.distance ref point
```

モジュール名は `Geometry.Point` です。型も関数も完全修飾できます。別モジュールの関数は修飾必須ですが、レコード、union、case、クラスには自モジュール優先と一意な公開候補による無修飾の解決があります。

ローカル値がモジュールと同名なら `name.field` はローカル値のフィールドアクセスを優先します。名前が曖昧なら修飾を増やして解決します。

## root の決定と探索

| 入力 | root と入口 |
| --- | --- |
| ディレクトリ | そのディレクトリが root。入口は直下の Main.tz |
| ソースファイル | 親ディレクトリが root。上位のプロジェクトを推測しない |
| ネストした App/Main.tz | 外側 root からは App.Main。外側プロジェクトの入口ではない |

root 配下の全 `.tz` / `.tt` / `.tc` を再帰探索し、相対パス順に検査します。未参照のファイルも対象です。隣接する独立例を同じ root へ混在させないでください。

dot で始まるファイル・ディレクトリは無視します。ソースやディレクトリの symlink は拒否します。パス要素は ASCII 識別子で、予約語、`_` 単独、Task は使えません。

同じ相対パスでは拡張子が違っても同じファイル名本体を併存させられません。別ディレクトリの同名ファイルは別モジュールです。利用者コードには module / namespace / open / import 宣言、任意の検索パス、同一モジュールのファイル分割はありません。

## public と private

宣言は既定で public です。`private def`, `private record`, `private union`, `private type`, `private const` は宣言モジュール内だけで参照できます。private union の case も private です。

fn / let の実装は def の可視性を引き継ぎます。`private fn` とは書きません。public の関数・型・制約から同じモジュールの private 型を漏らすこともできません。違反は `E1022` です。

型クラスとインスタンスは常に public です。ビルダー操作は private にできませんが、補助関数は private にできます。可視性は名前解決の規則で、ランタイム上のアクセス制御ではありません。

## 標準ライブラリ

標準ライブラリはコンパイラに埋め込まれ、利用者のソースの後に読み込まれます。未使用でも型検査しますが、到達しない標準関数などは生成 IR から除去します。

利用者の宣言は同名の標準宣言より優先しますが、標準ライブラリ内部から利用者の宣言を探索することはありません。std の private 関数にはアクセスできません。

以下は先頭の名前空間として予約されています。名前が予約されていることと、同名のソースファイルや API がすべて存在することは同義ではありません。

```text
Option Result Array List Vec String Utf8String Char Utf8Char Math Int
Debug Parallel Simd Map Set Seq Test Gpu
```

## ローカルパッケージ

root の `Tsuzuri.toml` に path 依存を指定できます。

```toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
geometry-core = { path = "../geometry-core" }
```

依存先にも name / version を持つ manifest が必要で、依存キーは実際の name と一致させます。name は小文字 ASCII kebab-case、各要素は英字始まりで最大 255 byte です。version は非空文字列ですが、版解決には使いません。

`geometry-core` の名前空間は `GeometryCore` となり、依存の `Point.tz` は `GeometryCore.Point` です。依存内部からもその完全修飾名を使います。root 自身のモジュールには package prefix を付けず、依存の Main は入口になりません。

同じ正規化 root は共有し、循環、同名の別 root、名前空間の衝突、symlink は拒否します。ネストした依存 root を親パッケージのソースとして二重に読みません。

## manifest の文法と制限

上の section と key だけを認める限定 TOML です。コメント、空行、CRLF、引用符付き UTF-8 文字列と対応 escape を使えますが、任意の TOML 構文を受け付けるわけではありません。未知の key・構文は `E0002` です。

registry、git 依存、ネットワーク取得、lockfile、版解決、build script は未実装です。manifest がなければ従来の探索規則を使います。

探索の上限は、モジュール名 16 要素 / 255 byte、4,096 ソース、1,024 ディレクトリです。依存グラフは 1,024 パッケージ、深さ 128、全体で 4,096 ソースまでです。超過は `E1017` です。manifest も生成物による上書き保護の対象です。

## 関連項目

- [ファイルの種類](lexical-and-layout.md)
- [型クラスのモジュール関数制約](generics-and-typeclasses.md)
- [ホスト関数の仕様](../../docs/language.md#ホスト関数のインポート)
