# パッケージ

複数のモジュールやビルド設定、外部ライブラリとの連携をまとめる単位がパッケージです。プロジェクトのルートディレクトリにマニフェストファイル `Tsuzuri.toml` を配置することで、パッケージのメタデータ、既定名前空間、およびローカル環境にある他パッケージへの依存関係を宣言できます。

Tsuzuri のパッケージシステムは、決定論的なビルドと安全性に配慮し、意図的にシンプルな構造を採用しています。

## この記事のポイント

- マニフェスト `Tsuzuri.toml` は、決まったセクションだけを受け付ける TOML です。
- `[package]` には `name`、`version`、省略できる `namespace` を書きます。
- `[dependencies]` は、ローカルの相対パス（`path = "..."`）だけです。
- `[wasm]` と `[native]` で、WebAssembly のメモリとネイティブのリンク入力を指定できます。
- `tsuzuri new <dir>` で、マニフェスト付きの雛形を作れます。
- Git や中央レジストリからの取得は未実装です（[E10](../../../_features/E10-package-registry.md) で計画中）。

## Tsuzuri.toml の役割と構文規則

ルートの `Tsuzuri.toml` は、そのディレクトリが 1 つのパッケージであることを伝えます。マニフェストが無くても、ディレクトリ名から名前空間を決めてビルドできます。規則は [名前空間](namespaces.md) と同じです。

パーサーが受け付けるのは、次のものだけです。

- `#` の行コメント、空行、CRLF / LF
- ダブルクォートの UTF-8 文字列と、エスケープ `\"`、`\\`、`\n`、`\r`、`\t`
- `[package]` を最初に置き、そのあとに `[dependencies]`、`[wasm]`、`[native]` を任意の順で、それぞれ 1 回まで

複数行の配列、未知のセクションやキー、対応していない TOML は `E0002` です。マニフェストは最大 1 MiB で、超えると `E1017` です。シンボリックリンクのマニフェストは `E1011` です。

## [package] セクション

`[package]` はマニフェストの最初のセクションです。

```toml
[package]
name = "geometry-tools"
version = "0.1.0"
namespace = "GeometryTools"
```

各キーの仕様は次のとおりです。

| キー名 | 必須 / 任意 | 型 | 仕様と制限 |
| --- | --- | --- | --- |
| `name` | 必須 | 文字列 | 小文字の ASCII kebab-case。各要素は英小文字で始まり、続きは英小文字か数字。全体で最大 255 バイト。`app2` は可、`Demo` は `E1011`。 |
| `version` | 必須 | 文字列 | 空でない版の文字列（例: `"0.1.0"`）。パス依存では版の選択には使いません。 |
| `namespace` | 任意 | 文字列 | 既定名前空間。`::` でつないだ ASCII 識別子（最大 16 階層、255 バイト）。 |

`namespace` を省略すると、`name` を PascalCase にした名前が既定になります。`geometry-tools` なら `GeometryTools` です。PascalCase は自動変換の形であり、書いた名前が小文字でも、識別子なら受理されます。

> [!WARNING]
> `Acme.Tools` のようにドットで区切ると `E1011` です。先頭要素に `std`、`Task`、予約モジュール名（`Maybe`、`Array` など）も `E1011` です。どの要素も `_` 単独、`Task`、予約語にはできません。

## [dependencies] セクション（ローカルパス依存）

他のローカルパッケージに依存する場合は、`[dependencies]` セクションに相対パスで指定します。

```toml
[dependencies]
geometry-core = { path = "../geometry-core" }
native-helper = { path = "../native-helper", native = true }
```

- **キー名**: テーブルのキーは、依存先の `name` と一致させます。違うと `E1011` です。同じキーを 2 回書くのも `E1011` です。
- **相対パス**: `path` は空でない相対パスです。`/usr/lib` や `C:\lib` のようなルート付きパスは `E0002` です。途中のシンボリックリンクも `E1011` です。
- **`native`**: 既定は `false` です。`native = true` の依存だけが、自分の `[native]` をルートのビルドへ渡します。`[native]` がある依存を `native = true` にしないと、無視ではなく `E2000` です。
- **依存の制限**:
  - 循環（A が B に依存し、B が A に依存する）は `E1011` です。
  - 同じ正規化パスのパッケージは共有されます。違うパスが同じ名前空間に解決されると `E1011` です。パッケージ名が同じだけでは拒否されません。
  - グラフ全体でパッケージ 1024、深さ 128、ソース 4096 までです。超えると `E1017` です。

## その他のセクション（[wasm] と [native]）

ビルド環境に応じた追加設定を記述できます。

### [wasm] セクション

WebAssembly の build と test で使う既定のメモリです。依存パッケージの `[wasm]` は読みません。

```toml
[wasm]
max-memory = "256MiB"
stack-size = "4MiB"
```

値はクォートした文字列です。バイト数そのもの（`"65536"`）か、`KiB`、`MiB`、`GiB` を付けます。`MB` は `E0002` です。コマンドラインの `--wasm-max-memory` と `--wasm-stack-size` が優先されます。省略時の既定は最大メモリ 16MiB、スタック 1MiB です。

### [native] セクション

C 言語連携でリンクする入力です。各キーは 1 行の文字列配列です。複数行の配列は `E0002` です。

```toml
[native]
link = ["src/helper.c"]
libraries = ["m"]
search = ["libs"]
```

- `link`: リンクする C ソースやオブジェクトの相対パス。
- `libraries`: システムライブラリ名（`-l` に相当）。`lib` 接頭辞や拡張子は付けません。`libm` は `E0002` です。
- `search`: 探索ディレクトリの相対パス（`-L` に相当）。

ルートの入力が先、`native = true` の依存があとです。`link`、`libraries`、`search` を合わせて 256 個までです。超えると `E2000` です。絶対パスは拒否されます。

## tsuzuri new によるプロジェクト作成

新しいパッケージを作成するときは、`tsuzuri new` コマンドを使用します。

```sh
tsuzuri new my-app
```

空のディレクトリ `my-app` に、次のファイルができます。無いディレクトリは作成されます。

```text
my-app/
  ├── Tsuzuri.toml
  ├── Main.tz
  └── .gitignore
```

`.gitignore` の中身は `.tsuzuri/` です。生成される `Tsuzuri.toml` の例:

```toml
[package]
name = "my-app"
version = "0.1.0"
namespace = "MyApp"
```

生成される `Main.tz` の例:

```tsuzuri
namespace MyApp

def main :: unit -> i32 = \() ->
    do! IO.writeln "Hello, Tsuzuri!"
    0

test "adds numbers" = assert (1 + 2 == 3)
```

ディレクトリが空でなければ `E2000` で止まり、既存ファイルは上書きしません。空の既存ディレクトリは使えます。`--namespace` を省略すると、ディレクトリ名からパッケージ名と名前空間を決めます。`my-app` なら名前空間は `MyApp` です。変えたいときは `--namespace` を付けます。無効な名前も `E2000` です。

```sh
tsuzuri new my-app --namespace Acme::App
```

## 依存パッケージの参照方法

依存関係にあるパッケージのモジュールは、依存先パッケージの既定名前空間で修飾して呼び出します。

例えば、`geometry-core` パッケージ（既定名前空間 `GeometryCore`）に依存している場合、呼び出し側は次のように記述します。

```text
namespace App

// 完全修飾名で呼び出す
let p = GeometryCore::Point.make 3.0 4.0
```

`using` 宣言を組み合わせることで、プレフィックスを省略することも可能です。

```text
namespace App

using GeometryCore

// using により名前空間を省略して呼び出す
let p = Point.make 3.0 4.0
```

> [!NOTE]
> 依存先パッケージの内部に `Main.tz` が含まれていても、ルートパッケージのエントリーポイントとして誤認されることはありません。エントリーポイントは常にルートパッケージ直下の `Main.tz` です。

## 未実装の機能と将来の計画

Tsuzuri 0.1.0 には、次の機能はありません（未実装です）。

- 外部ネットワーク通信によるパッケージのダウンロード
- Git リポジトリ（URL やブランチ指定）からの直接取得
- 中央パッケージレジストリからの検索・取得
- lockfile（依存バージョンの厳密固定ファイル）の自動生成
- ビルドスクリプト（ビルド時に任意のコードを実行する仕組み）

現在利用できる依存関係は、ローカルファイルシステム上の相対パス（`path = "..."`）のみです。外部パッケージを管理する場合は、git submodule やモノレポ構成などを用いてローカルディレクトリに配置してください。

Git やレジストリへの対応は、[E10](../../../_features/E10-package-registry.md) として計画中です。今の構文でそれらを書くと `E0002` になります。

## 他の言語との比較

| 項目 | Tsuzuri | Cargo（Rust） | npm（JavaScript） |
| --- | --- | --- | --- |
| マニフェスト | `Tsuzuri.toml` | `Cargo.toml` | `package.json` |
| 依存の指定 | ローカル相対パスのみ | レジストリ、Git、パス | レジストリ、Git、ファイルパス |
| lockfile | なし | `Cargo.lock` | `package-lock.json` |
| ビルドスクリプト | なし（`[native]` で宣言） | `build.rs` | `preinstall` などの hooks |
| ネットワーク通信 | なし | あり（crates.io） | あり（npm registry） |

## まとめ

- `Tsuzuri.toml` はパッケージの定義ファイルです。`[package]` を最初に置きます。
- パッケージ名は小文字の kebab-case です。`namespace` を省略すると PascalCase の既定名前空間になります。
- 依存はローカルの相対パスだけです。循環と、同じ名前空間の別ルートは拒否されます。
- `native = true` を付けない依存の `[native]` は、無視ではなく `E2000` です。
- `tsuzuri new` で、マニフェストと入口を含む初期構成を作れます。
- Git や中央レジストリからの取得は未実装です（計画中）。

## 関連項目

- [名前空間](namespaces.md)
- [モジュール](modules.md)
- [using 宣言](using-declarations.md)
- [コンパイラの使い方](../compiler/usage.md)
- [ネイティブ連携 (C ABI)](../compiler/native-interop.md)
- [言語リファレンスの目次](../index.md)

