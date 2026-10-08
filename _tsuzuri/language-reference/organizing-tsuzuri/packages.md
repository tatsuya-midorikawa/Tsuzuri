# パッケージ

複数のモジュールやビルド設定、外部ライブラリとの連携をまとめる単位がパッケージです。プロジェクトのルートディレクトリにマニフェストファイル `Tsuzuri.toml` を配置することで、パッケージのメタデータ、既定名前空間、およびローカル環境、git リポジトリ、registry にある他パッケージへの依存関係を宣言できます。

Tsuzuri のパッケージシステムは、決定論的なビルドと安全性に配慮し、意図的にシンプルな構造を採用しています。ネットワークに触れるのは、依存を取得する `tsuzuri fetch` と、公開する commit を確かめる `tsuzuri publish` だけです。

## この記事のポイント

- マニフェスト `Tsuzuri.toml` は、決まったセクションだけを受け付ける TOML です。
- `[package]` には `name`、`version`、省略できる `namespace` を書きます。
- `[dependencies]` は、ローカルの相対パス（`path = "..."`）、commit を固定した git リポジトリ（`git = "..."`, `rev = "..."`）、registry の版の要求（`version = "1.2.3"`）です。
- 版の要求は、`[registry]` に書いた index（git リポジトリ）から最小版選択で解決します。Tsuzuri は公開の registry を運営しておらず、index は利用者や組織が置きます。
- `tsuzuri fetch` が依存を取得し、内容の SHA-256 と選んだ版を `Tsuzuri.lock` に記録します。ほかのコマンドは `git` もネットワークも使いません。
- `tsuzuri publish` は、パッケージを検査し、公開する commit の index の項目を出力します。
- `[wasm]` と `[native]` で、WebAssembly のメモリとネイティブのリンク入力を指定できます。
- `tsuzuri new <dir>` で、マニフェスト付きの雛形を作れます。

## Tsuzuri.toml の役割と構文規則

ルートの `Tsuzuri.toml` は、そのディレクトリが 1 つのパッケージであることを伝えます。マニフェストが無くても、ディレクトリ名から名前空間を決めてビルドできます。規則は [名前空間](namespaces.md) と同じです。

パーサーが受け付けるのは、次のものだけです。

- `#` の行コメント、空行、CRLF / LF
- ダブルクォートの UTF-8 文字列と、エスケープ `\"`、`\\`、`\n`、`\r`、`\t`
- `[package]` を最初に置き、そのあとに `[dependencies]`、`[wasm]`、`[native]`、`[registry]` を任意の順で、それぞれ 1 回まで

複数行の配列、未知のセクションやキー、対応していない TOML は `E0002` です。マニフェストは最大 1 MiB で、超えると `E1017` です（ソースの上限の 4 MiB を超えるファイルは、読み込みの `E0003` です）。シンボリックリンクのマニフェストは `E1011` です。

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
| `version` | 必須 | 文字列 | 空でない版の文字列（例: `"0.1.0"`）。registry に公開するパッケージは `MAJOR.MINOR.PATCH`（先頭の 0 なし）でなければなりません。パス依存と git 依存では版の選択に使いません。 |
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
  - git 依存と registry 依存のパッケージ名は、グラフ全体で 1 つの取得元（1 つの URL と rev、または registry）を指します。同じ名前を別の URL や rev で要求したり、git と registry で要求したり、同じ名前のパス依存やルートパッケージがあったりすると `E1011` です（`package 'NAME' is required from different sources ...`）。
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

- `fetch` は、2 空白の字下げ、LF、末尾の改行 1 つ、パッケージ名の順の正規形で書きます。内容が変わらなければ書き直しません（更新時刻も変わりません）。グラフから消えた依存の項目は消し、git・registry 依存も既存の lockfile もなければ作りません。
- 記録するのは git と registry のパッケージだけです。path 依存のパッケージが持つ依存も、ルートの `Tsuzuri.lock` に入ります。依存パッケージの `Tsuzuri.lock` は読みません。
- registry のパッケージがあると `"format": 2` になり、その項目は `name` の次に選んだ版 `version` を持ちます（[registry 依存と版の解決](#registry-依存と版の解決)）。git 依存だけなら `"format": 1` のままです。どちらの形式も読めます。
- 読むときは、キーの順序と空白は問いません。未知のキー、重複したキーやパッケージ名、形式の違反は `E2007`、1 MiB を超えると `E1017` です。
- `sha256` は、パッケージのルートからの `/` 区切りの相対パスと内容を、パスのバイト順に SHA-256 でまとめた値です（`Tsuzuri.toml` を含みます）。
- バージョン管理に含めてください。`Tsuzuri.lock` も出力保護の対象で、ビルドキャッシュのキーに入ります。

### オフラインのビルド

`check`、`build`、`run`、`test`、`bench`、`doc`、`lsp` は `git` を起動せず、ネットワークにも触れません。git 依存は `Tsuzuri.lock` とストアから読み、読み込んだ内容が `sha256` と一致することを毎回確かめます。

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
- 4 MiB を超えるファイル（`E0003`）、4,096 を超えるソース、1,024 を超えるディレクトリ、16 段を超えるパス、64 MiB を超える `ls-tree` の一覧（`E1017`）。`Tsuzuri.toml` は 1 MiB までです（`E1017`）

## registry 依存と版の解決

版の要求で依存を書くと、`tsuzuri fetch` が registry の index から版を選びます。

```toml
[dependencies]
geometry-core = { version = "1.2.0" }

[registry]
index = "https://example.org/tsuzuri-index.git"
rev = "0123456789abcdef0123456789abcdef01234567"
```

- **版の要求**: `{ version = "MAJOR.MINOR.PATCH" }` です。各部は 10 進で先頭に 0 を付けません。範囲の演算子（`^`、`~`、`>=`）、ワイルドカード、pre-release（`1.2.3-beta`）は `E0002` です。
- **意味**: `version = "1.2.0"` は「1.2.0 以上で、互換の範囲にある版」です。互換の範囲は、1.0.0 以降は同じ `MAJOR`（`1.x.y`）、それより前は同じ `0.MINOR`（`0.3.x`）です。
- **`[registry]`**: `index` は index リポジトリの URL で、git 依存と同じ規則（`https://` か `file:///`）です。`rev` は省略でき、書くと index のその commit で解決します。省略すると index リポジトリの `HEAD` を使います。読むのはルートパッケージの `[registry]` だけで、依存パッケージの `[registry]` は無視します。
- **registry のパッケージの制限**: 依存は版の要求だけです（path・git 依存は `E1011`）。`[native]` も書けません（`E2000`）。

### 最小版選択

`fetch` は、グラフのマニフェスト（ルート、path 依存、git 依存のパッケージ）の版の要求から始めて、index に書かれた各版の依存をたどります。各パッケージの互換の範囲（`1.x`、`0.2.x` など）ごとに、たどった要求の最小版のうち最も新しいものを選びます。たとえば `app` が `shapes 1.0.0`、`tiles 1.0.0` を要求し、`tiles 1.0.0` が `shapes 1.1.0` を要求すると、index に `shapes 1.2.0` があっても `shapes 1.1.0` が選ばれます。

- 選ばれる版は、要求された版のどれかです。その版が index になければ `E2007` です（近い版で代わりにしません）。新しい版が公開されても、要求を変えるまで選ばれる版は変わりません。
- 選ばれなかった版の要求もたどります（Go の最小版選択と同じです）。そのため、後から上書きされた要求が、同じ互換の範囲の版を押し上げることがあります。取得して `Tsuzuri.lock` に入れるのは、ルートの要求から選んだ版の依存だけをたどって届くパッケージです。
- 1 つのパッケージ名には 1 つの版です。選んだ版の依存をたどって届く要求に互換の範囲が違うもの（`1.1.0` と `2.0.0`、`0.1.0` と `0.2.0`）があれば `E1011` で、両方の要求元を示します。選ばれなかった版だけが述べる要求は衝突になりません（`delta` が `beta 1.0.0`、`app` が `beta 1.1.0` を要求し、`beta 1.0.0` だけが `aaa 1.0.0`、`beta 1.1.0` が `aaa 2.0.0` を要求するなら、`beta 1.1.0` と `aaa 2.0.0` を選びます）。
- lockfile は選択に使いません。同じ index と要求からは、lockfile がなくても同じ版が選ばれます。

選んだ各版について、`fetch` は index の `git` と `rev` の commit を git 依存と同じ方法で取得し、内容の SHA-256 が index の `sha256` と一致すること、取得したマニフェストの `name`、`version`、依存が index の項目と一致することを確かめます。`Tsuzuri.lock` に同じ版の項目があり、index の `sha256` がその値と違うときは、公開済みの版が書き換えられたとみなして `E2007` で止めます。

### オフラインのビルドと registry

ほかのコマンドは index を読みません。`Tsuzuri.lock` の項目の版が、マニフェストの要求を満たすこと（互換の範囲にあり、要求以上であること）を確かめ、ストアの内容を `sha256` と照合します。要求を変えて記録の版が満たさなくなると、`fetch` まで `E2007` です。記録の版が満たす限り、要求を下げても版は変わりません（下げた要求で選び直すには `fetch` します）。

| 状況 | 診断 |
| --- | --- |
| ルートに `[registry]` がない（`fetch`） | `E2007` `registry dependency 'NAME' needs a [registry] index in the root package's Tsuzuri.toml` |
| index にパッケージや要求された版がない | `E2007` `the registry index has no package 'NAME'`、`the registry index has no version V of 'NAME' (required by P)` |
| 互換の範囲が違う要求 | `E1011` `package 'NAME' is required at incompatible versions A (by P) and B (by Q); one package name has one version` |
| 取得した内容が index の `sha256` と違う | `E2007` `registry package 'NAME' V at REV has sha256 ACTUAL but the registry index records EXPECTED` |
| 取得したマニフェストが index の項目と違う | `E2007` `registry package 'NAME' V does not match its registry index entry (...)` |
| index の `sha256` が `Tsuzuri.lock` と違う | `E2007` `registry package 'NAME' V has sha256 NEW in the registry index but Tsuzuri.lock records OLD; ...` |
| index のファイルの形式違反 | `E2007` `the registry index file for 'NAME' is invalid (...)`。1 MiB 超は `E1017` |
| `Tsuzuri.lock` の版が要求を満たさない（ビルド） | `E2007` `Tsuzuri.lock does not record a version of 'NAME' that satisfies V; run tsuzuri fetch` |

## registry の運用と tsuzuri publish

Tsuzuri は公開の registry を運営していません。registry は、index のファイルを置いた git リポジトリです。組織やコミュニティが、任意の git ホスティング（https）や社内のミラーに置き、利用者は `[registry]` でそれを指します。

### index の形式

index リポジトリのルートの `index/` に、パッケージごとに `index/<name>.json` を置きます。

```json
{
  "name": "geometry-core",
  "versions": [
    {
      "version": "1.2.0",
      "git": "https://example.org/geometry-core.git",
      "rev": "0123456789abcdef0123456789abcdef01234567",
      "sha256": "7948540d0d22602ebe527bbb228b364d248048395ffb2f5559224265d85ea6ab",
      "dependencies": {
        "shapes": "1.1.0"
      }
    }
  ]
}
```

- 最上位のキーは `name`（ファイル名と同じ）と `versions` だけです。各版のキーは `version`、`git`、`rev`、`sha256`、`dependencies` のすべてで、ほかのキー、重複したキーや版は誤りです。
- `git` と `rev` はパッケージの commit、`sha256` はその内容のハッシュ（`Tsuzuri.lock` と同じ定義）、`dependencies` はそのマニフェストの版の要求です。
- https の index の項目は、`file:///` の `git` を使えません（`E2007`）。
- 1 ファイルは 1 MiB まで、1 回の解決でたどるのは 1,024 パッケージ、16,384 版までです（`E1017`）。

運用する側は次を守ってください。

- 公開した版の項目は、書き換えたり消したりしません。書き換えは利用者の `Tsuzuri.lock` との不一致（`E2007`）になり、削除はその版を要求するパッケージの解決を止めます。
- 項目を足すときは、`tsuzuri publish` の出力を使い、commit と依存を確かめてからマージします。

### tsuzuri publish

```sh
tsuzuri publish geometry-core --git https://example.org/geometry-core.git --rev 0123456789abcdef0123456789abcdef01234567
```

`publish` は、ディレクトリのパッケージを公開できるか確かめ、成功すると index の項目を標準出力に出します。index リポジトリへの書き込みや push はしません。出力を `index/<name>.json` の `versions` に足し、index リポジトリへ commit してください。

1. パッケージを `check` と同じように検査します（入口は要りません）。registry の依存は、事前に `tsuzuri fetch` しておきます。
2. `[package] version` が `MAJOR.MINOR.PATCH` であること（`E1011`）、依存が版の要求だけであること（`E1011`）、`[native]` がないこと（`E2000`）を確かめます。
3. `--git` と `--rev` の commit を `fetch` と同じ方法で取得し、その内容のハッシュが、ディレクトリのパッケージのファイル（`Tsuzuri.toml` と、`.` で始まらないパスの `.tz`、`.tt`、`.tc`）のハッシュと一致することを確かめます。一致しなければ `E2007` です。検査した内容と公開する commit が同じになります。

引数の誤り（`--git` や `--rev` の欠落、URL や commit ID の形、`Tsuzuri.toml` がない）は `E2000` で、終了コード 2 です。

> [!NOTE]
> 版の間で公開 API が互換かどうかの検査（[G19](../../../_features/G19-editions-compatibility.md)）はまだありません。`publish` は、`1.2.0` から `1.3.0` で関数を消したような互換性のない変更を検出しないので、版を上げる側が確認してください。

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

- 公開の中央 registry（Tsuzuri の運営する index）。index は自分で置きます
- 公開 API の互換性の検査（[G19](../../../_features/G19-editions-compatibility.md)）、版の取り下げ（yank）、範囲の演算子と pre-release の版
- ブランチやタグの追跡、`ssh://` や資格情報が必要なリポジトリ、リポジトリの下位ディレクトリにあるパッケージ
- ビルドスクリプト（ビルド時に任意のコードを実行する仕組み）

## 他の言語との比較

| 項目 | Tsuzuri | Cargo（Rust） | npm（JavaScript） |
| --- | --- | --- | --- |
| マニフェスト | `Tsuzuri.toml` | `Cargo.toml` | `package.json` |
| 依存の指定 | ローカル相対パス、commit 固定の git、registry の版 | レジストリ、Git、パス | レジストリ、Git、ファイルパス |
| 版の選択 | 最小版選択（lockfile なしでも同じ結果） | 互換範囲の最新版と lockfile | 範囲の最新版と lockfile |
| registry | 自分で置く git の index（公開の運営はなし） | crates.io | npm registry |
| lockfile | `Tsuzuri.lock`（内容の SHA-256 と選んだ版） | `Cargo.lock` | `package-lock.json` |
| ビルドスクリプト | なし（`[native]` で宣言） | `build.rs` | `preinstall` などの hooks |
| ネットワーク通信 | `tsuzuri fetch` と `tsuzuri publish` だけ（`git` を起動） | あり（crates.io） | あり（npm registry） |

## まとめ

- `Tsuzuri.toml` はパッケージの定義ファイルです。`[package]` を最初に置きます。
- パッケージ名は小文字の kebab-case です。`namespace` を省略すると PascalCase の既定名前空間になります。
- 依存はローカルの相対パス、commit を固定した git リポジトリ、registry の版の要求です。循環と、同じ名前空間の別ルートは拒否されます。
- 版の要求は、自分で置いた index から最小版選択で解決します。1 つのパッケージ名には 1 つの版です。
- `tsuzuri fetch` が `git` で依存を取得し、`Tsuzuri.lock` に内容の SHA-256 と選んだ版を記録します。ビルドは lockfile とストアだけを読み、内容の一致を確かめます。
- `tsuzuri publish` は、検査した内容と同じ commit の index の項目を出力します。index への反映は index リポジトリへの commit です。
- `native = true` を付けない依存の `[native]` は、無視ではなく `E2000` です。
- `tsuzuri new` で、マニフェストと入口を含む初期構成を作れます。

## 関連項目

- [名前空間](namespaces.md)
- [モジュール](modules.md)
- [using 宣言](using-declarations.md)
- [コンパイラの使い方](../compiler/usage.md)
- [ネイティブ連携 (C ABI)](../compiler/native-interop.md)
- [言語リファレンスの目次](../index.md)

