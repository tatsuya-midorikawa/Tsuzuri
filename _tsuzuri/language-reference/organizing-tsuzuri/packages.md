# パッケージ

複数のモジュールやビルド設定、外部ライブラリとの連携をまとめる単位がパッケージです。プロジェクトのルートディレクトリにマニフェストファイル `Tsuzuri.toml` を配置することで、パッケージのメタデータ、既定名前空間、およびローカル環境や git リポジトリにある他パッケージへの依存関係を宣言できます。

Tsuzuri のパッケージシステムは、決定論的なビルドと安全性に配慮し、意図的にシンプルな構造を採用しています。ネットワークに触れるのは、依存を取得する `tsuzuri fetch` だけです。

## この記事のポイント

- マニフェスト `Tsuzuri.toml` は、決まったセクションだけを受け付ける TOML です。
- `[package]` には `name`、`version`、省略できる `namespace` を書きます。
- `[dependencies]` は、ローカルの相対パス（`path = "..."`）と、commit を固定した git リポジトリ（`git = "..."`, `rev = "..."`）です。
- `tsuzuri fetch` が git 依存を取得し、内容の SHA-256 を `Tsuzuri.lock` に記録します。ほかのコマンドは `git` もネットワークも使いません。
- `[wasm]` と `[native]` で、WebAssembly のメモリとネイティブのリンク入力を指定できます。
- `tsuzuri new <dir>` で、マニフェスト付きの雛形を作れます。
- 中央レジストリと版の解決は未実装です（[E10](../../../_features/E10-package-registry.md) の Phase 2 で計画中）。

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

他のローカルパッケージに依存する場合は、`[dependencies]` セクションに相対パスで指定します。git リポジトリの依存は [git 依存と Tsuzuri.lock](#git-依存と-tsuzurilock) で説明します。

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
  - 同じ正規化パスのパッケージは共有されます。違うパスが同じ名前空間に解決されると `E1011` です。パス依存どうしでは、パッケージ名が同じだけでは拒否されません。
  - git 依存のパッケージ名は、グラフ全体で 1 つの取得元（1 つの URL と rev）を指します。同じ名前を別の URL や rev で要求したり、同じ名前のパス依存やルートパッケージがあったりすると `E1011` です（`package 'NAME' is required from different sources ...`）。
  - グラフ全体でパッケージ 1024、深さ 128、ソース 4096 までです。超えると `E1017` です。

## git 依存と Tsuzuri.lock

別のリポジトリで開発されているパッケージは、commit を固定した git 依存として書けます。任意の git ホスティング、社内のミラー、ローカルの bare リポジトリが使えます。

```toml
[dependencies]
geometry-core = { git = "https://example.org/geometry-core.git", rev = "0123456789abcdef0123456789abcdef01234567" }
local-util = { path = "../local-util" }
```

- **形**: `{ git = "...", rev = "..." }` をこの順序で 1 行に書きます。`rev` の欠落、順序違い、ほかのキー（`branch`、`tag`、`native` など）は `E0002` です。
- **`git`**: `https://` か `file:///` で始まる、2,048 バイト以下の URL です。空白、`@`（資格情報）、`?`、`#`、`\` を含められません。`http://`、`ssh://`、`git://`、`git@host:path` は `E0002` です。
- **`rev`**: 40 文字の小文字 16 進の commit ID です。ブランチ名、タグ、短縮形、64 文字の ID は `E0002` です。
- **パッケージの範囲**: リポジトリのルートが 1 つのパッケージで、ルートに `Tsuzuri.toml` が必要です。取り出すのはルートの `Tsuzuri.toml` と、`.` で始まる要素を含まないパスの `.tz`、`.tt`、`.tc` だけです。
- **git のパッケージの制限**: path 依存は持てません（`E1011`）。`[native]` も書けません（`E2000`）。`file:///` の git 依存を持てるのは、そのパッケージ自体を `file:///` から取得したときだけで、`https://` から取得したパッケージでは `E2007` です。

### tsuzuri fetch

git 依存は `tsuzuri fetch <ディレクトリ>` で取得します。成功すると何も出力せず、終了コード 0 です。

```sh
tsuzuri fetch app
```

`fetch` は依存グラフをたどり、各 git 依存の commit を PATH にある `git`（2.32 以降）で取得します。作業木は作らず、一時的な bare リポジトリへの `fetch` と、`ls-tree`、`cat-file` だけを使い、ファイルは Tsuzuri 自身が書きます。そのため hooks、フィルター、シンボリックリンク、submodule は動きません。利用者とシステムの git 設定、`GIT_` で始まる環境変数、資格情報の問い合わせも使いません（プロキシと証明書の環境変数は使います）。

取り出したパッケージは、ビルドキャッシュの保存先（`TSUZURI_CACHE_DIR`）の下の `packages/git/<sha256>/` に置きます。`<sha256>` はパッケージの内容のハッシュで、同じ内容は一度だけ置かれます。キャッシュの掃除はこのディレクトリに触れません。

グラフ全体が解決できたときだけ、ルートの `Tsuzuri.toml` の隣に `Tsuzuri.lock` を書きます。

- `Tsuzuri.lock` に同じ名前、URL、rev の項目があり、ストアの内容がその SHA-256 と一致すれば、取得しません（`git` も起動しません）。ストアの内容が壊れていれば、取得し直して置き換えます。
- 取得した内容の SHA-256 が `Tsuzuri.lock` の値と違うときは `E2007` で止め、`Tsuzuri.lock` を書き換えません。同じ commit が別の内容になることはないため、改ざんか取り違えを疑ってください。新しい内容を信用するときだけ、その項目を消してから `fetch` します。
- git の失敗（リポジトリや commit が見つからない、ネットワークの不通など）は、git のメッセージを含めて `E2007` です。`git` が見つからないか 2.32 より古いときも `E2007` です。

### Tsuzuri.lock の形式

```json
{
  "format": 1,
  "packages": [
    {
      "name": "geometry-core",
      "git": "https://example.org/geometry-core.git",
      "rev": "0123456789abcdef0123456789abcdef01234567",
      "sha256": "7948540d0d22602ebe527bbb228b364d248048395ffb2f5559224265d85ea6ab"
    }
  ]
}
```

- `fetch` は、2 空白の字下げ、LF、末尾の改行 1 つ、パッケージ名の順の正規形で書きます。内容が変わらなければ書き直しません（更新時刻も変わりません）。グラフから消えた依存の項目は消し、git 依存も既存の lockfile もなければ作りません。
- 記録するのは git のパッケージだけです。path 依存のパッケージが持つ git 依存も、ルートの `Tsuzuri.lock` に入ります。依存パッケージの `Tsuzuri.lock` は読みません。
- 読むときは、キーの順序と空白は問いません。未知のキー、重複したキーやパッケージ名、形式の違反は `E2007`、1 MiB を超えると `E1017` です。
- `sha256` は、パッケージのルートからの `/` 区切りの相対パスと内容を、パスのバイト順に SHA-256 でまとめた値です（`Tsuzuri.toml` を含みます）。
- バージョン管理に含めてください。`Tsuzuri.lock` も出力保護の対象で、ビルドキャッシュのキーに入ります。

### オフラインのビルド

`check`、`build`、`run`、`test`、`doc`、`lsp` は `git` を起動せず、ネットワークにも触れません。git 依存は `Tsuzuri.lock` とストアから読み、読み込んだ内容が `sha256` と一致することを毎回確かめます。

| 状況 | 診断 |
| --- | --- |
| `Tsuzuri.lock` がない | `E2007` `Tsuzuri.lock is missing; run tsuzuri fetch to download git dependencies and record them` |
| 依存の URL や rev が `Tsuzuri.lock` にない | `E2007` `Tsuzuri.lock does not record git dependency 'NAME' at this url and rev; run tsuzuri fetch` |
| ストアにない | `E2007` `git dependency 'NAME' is not downloaded; run tsuzuri fetch` |
| ストアの内容が変わった | `E2007` `git dependency 'NAME' does not match its sha256 in Tsuzuri.lock; run tsuzuri fetch to restore it` |
| キャッシュの場所が決まらない（`TSUZURI_CACHE_DIR` が空など） | `E2007` `no package store is available; set TSUZURI_CACHE_DIR and run tsuzuri fetch` |

言語サーバーで、ストアにある依存のファイルを編集した場合も、内容が変わるので `E2007` になります。

### 取得するリポジトリの検査

`fetch` は、取り出すファイルについて次を拒否します。README など取り出さないファイルは調べません。

- シンボリックリンクと submodule（`E2007`）
- 空、`.`、`..`、255 バイト超の要素や、`\`、`:`、制御文字を含むパス（`E2007`）
- 大文字と小文字だけが違うパス。大文字小文字を区別しないファイルシステムで重なるためです（`E2007`）
- ルートの `Tsuzuri.toml` がないこと、UTF-8 でないファイル（`E2007`）
- 1 MiB を超えるファイル（`E0003`）、4,096 を超えるソース、1,024 を超えるディレクトリ、16 段を超えるパス、64 MiB を超える `ls-tree` の一覧（`E1017`）

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

- 中央パッケージレジストリと、版の要求（`version = "1.2.3"`）による解決（[E10](../../../_features/E10-package-registry.md) の Phase 2 で計画中です。今の構文で書くと `E0002` です）
- ブランチやタグの追跡、`ssh://` や資格情報が必要なリポジトリ、リポジトリの下位ディレクトリにあるパッケージ
- ビルドスクリプト（ビルド時に任意のコードを実行する仕組み）

## 他の言語との比較

| 項目 | Tsuzuri | Cargo（Rust） | npm（JavaScript） |
| --- | --- | --- | --- |
| マニフェスト | `Tsuzuri.toml` | `Cargo.toml` | `package.json` |
| 依存の指定 | ローカル相対パス、commit 固定の git | レジストリ、Git、パス | レジストリ、Git、ファイルパス |
| lockfile | `Tsuzuri.lock`（git 依存の内容の SHA-256） | `Cargo.lock` | `package-lock.json` |
| ビルドスクリプト | なし（`[native]` で宣言） | `build.rs` | `preinstall` などの hooks |
| ネットワーク通信 | `tsuzuri fetch` だけ（`git` を起動） | あり（crates.io） | あり（npm registry） |

## まとめ

- `Tsuzuri.toml` はパッケージの定義ファイルです。`[package]` を最初に置きます。
- パッケージ名は小文字の kebab-case です。`namespace` を省略すると PascalCase の既定名前空間になります。
- 依存はローカルの相対パスか、commit を固定した git リポジトリです。循環と、同じ名前空間の別ルートは拒否されます。
- `tsuzuri fetch` だけが `git` で依存を取得し、`Tsuzuri.lock` に内容の SHA-256 を記録します。ビルドは lockfile とストアだけを読み、内容の一致を確かめます。
- `native = true` を付けない依存の `[native]` は、無視ではなく `E2000` です。
- `tsuzuri new` で、マニフェストと入口を含む初期構成を作れます。

## 関連項目

- [名前空間](namespaces.md)
- [モジュール](modules.md)
- [using 宣言](using-declarations.md)
- [コンパイラの使い方](../compiler/usage.md)
- [ネイティブ連携 (C ABI)](../compiler/native-interop.md)
- [言語リファレンスの目次](../index.md)

