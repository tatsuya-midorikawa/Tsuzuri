# E10: git／registry 依存・lockfile・版解決

| 項目 | 内容 |
| --- | --- |
| ID | E10 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | E04, G11 |
| 後続 | G19, E09 Phase 2 |
| 状態 | Phase 1 done（Phase 2 は実装中） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（`tsuzuri fetch` が外部の `git` CLI を起動すること、`E2007` の確定、GUIDE D-29 の E04 記録「git・lockfile を導入しない」の更新）, D10（Phase 2 の registry の運用主体と index の置き場所） |
| 改善する劣位 | Rust 比: Cargo／crates.io に相当する依存管理がない（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#rust-に対する劣位点)）、C#/F# 比: NuGet |
| 手本にする既存実装 | manifest の厳密な行解析: `src/package.rs` の `parse_manifest` と `Line`（`key`・`expect`・`string`・`finish`・`error`）。グラフの読み込みと E04 の規則: `src/driver.rs` の `load_packages`・`LoadedPackage`・`Project::load_from_root`・`collect_sources`。外部ツールの起動: `src/driver.rs` の `tool`・`run_tool`。キャッシュ root・marker・staging: `src/cache.rs` の `default_root`・`BuildCache::open`・`stage`・`read_regular`。内容ハッシュ: `src/cache.rs` の `Sha256::field`・`Sha256::hex`。JSON 文字列の escape: `src/diagnostic.rs` の `json_string`。一時 project のテスト: `tests/modules.rs` の `loads_and_protects_local_package_graphs`。E2E の環境変数: `tests/cache.mjs`（`TSUZURI_CACHE_DIR` を一時ディレクトリへ向ける） |
| 主な影響ファイル | `src/package.rs`, `src/fetch.rs`（新規）, `src/lib.rs`, `src/driver.rs`, `src/main.rs`, `src/cache.rs`, `tests/packages.rs`（新規）, `tests/packages.mjs`（新規）, `tests/modules.rs`（変更しないことを確認）, `docs/language.md`, `docs/architecture.md`, `README.md`, `_docs/language-reference/modules-and-packages.md`, `_docs/tools/command-line.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

別リポジトリのライブラリを再現可能に取得・固定し、共有できるようにする。
ビルド中にネットワークへ触れず、取得物の同一性を検証できる安全な依存管理を段階的に導入する。

- Phase 1（実装対象）: commit 固定の git 依存、`tsuzuri fetch`、`Tsuzuri.lock`（SHA-256 の内容ハッシュ）、オフラインのビルド。
  サーバーを必要としない（任意の git ホスティング、社内ミラー、ローカルの bare repository で使える）。
- Phase 2（設計方針だけ。D10 の承認後）: semver の版要求、最小版選択、git repository で配る registry index、`tsuzuri publish`。

実装者は Phase 1 だけを実装する。Phase 2 は D10 が承認され、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- E04 と G11 が完了していること。確認: `ls _features/_completed/ | grep -E "^(E04|G11)"` が 2 件を示し、
  `grep -n "E04\|G11" _features/README.md` の状態が done（完了一覧）であること。
- D1 が承認済みであること。承認前はどの手順にも着手しない。
- 開発機と CI に `git` 2.32 以降があること（テストがローカルの repository を作る。`GIT_CONFIG_GLOBAL` は 2.32 から）。
  確認: `git --version`。HEAD 確認時の開発機は `git version 2.55.0`。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースラインを保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- Rust の crate（HTTP・TLS・圧縮・tar・`git2`・`gix` など）を足したくなった。Phase 1 は `std` と既存の `serde_json` だけで作る。
- git に作業木を書かせる必要が出た（`git clone`・`checkout`・`worktree`・`archive` の展開）。D5 の読み出し方（`ls-tree` と
  `cat-file --batch`）で足りない理由を報告する。
- `build`／`check`／`run`／`test`／`doc`／`lsp` の経路から `git` やネットワークに触れる必要が出た。
- 取得とビルドで `load_packages` のグラフ走査を共有できず、走査を 2 つ書く必要が出た（D6）。
- `BuildCache::open` や `BuildCache::evict` の挙動を変えないと `packages/`（D4）と共存できない。
- 既存テスト（`tests/modules.rs`、`tests/cache.mjs`、`src/cache.rs` の単体テスト）の期待値を変える必要がある。
- `unsafe` が必要になった。Windows のパス（`\`、ドライブ文字、予約名）で D5 の規則を超える扱いが必要になった（G10 と調整する）。
- テストにインターネット接続が必要になった。認証（credential helper・SSH 鍵）が必要な依存を扱いたくなった。

## 現状（HEAD `f8dc655` で確認）

- `src/package.rs`: `Manifest { name, version, namespace, dependencies: BTreeMap<String, Dependency> }`、
  `Dependency { path: PathBuf, span: Span }`。`parse_manifest` は `[package]`（`name`・`version`、非空文字列）と
  `[dependencies]` の `key = { path = "..." }` だけを受ける行単位の解析で、`{` の後のキーが `path` でなければ
  `E0002` `only local path dependencies are supported`。manifest が 1 MiB を超えると `E1017`、依存が 1,024 を超えると `E1017`。
  `namespace` は kebab-case を PascalCase にし、違反は `E1011`。単体テストはない。
- `src/driver.rs` の `load_packages(directory)` は `pending`（root、期待する名前、leaving）・`active`・`loaded`
  （`BTreeMap<PathBuf, LoadedPackage>`）・`namespaces` による DFS。循環、manifest の symlink、std と衝突する名前空間、
  一つの名前空間に複数の root（`package name or namespace resolves to multiple roots`）、依存パスの symlink、依存キーと
  package 名の不一致を `E1011`、1,024 package・深さ 128 を `E1017` で拒否する。結果は名前空間の順に並ぶ。
- `Project::load_from_root` は `load_packages` の各 root で `collect_sources(root, &roots)` を呼び、`Project::manifests` に
  各 manifest を `relative_path = "Tsuzuri.toml"` の `SourceFile` として入れる。`manifests` は `cache::build_key` の入力と
  出力保護（`project.sources.iter().chain(&project.manifests)`）に使われる。
- `collect_sources` は `.` で始まる名前を飛ばし、source・ディレクトリへの symlink を `E1011`、1,024 ディレクトリ・16 段・
  4,096 source を `E1017` で拒否し、`/` 区切りのパスで並べる。`read_source_text` はバイト列を正規化せずに UTF-8 として読み、
  1 MiB 超は `E0003` `source exceeds the 1048576-byte limit`。`source_kind` は private。
- `src/cache.rs`: `default_root`（`TSUZURI_CACHE_DIR`、なければ macOS `~/Library/Caches/tsuzuri/build-cache`、Linux
  `$XDG_CACHE_HOME/tsuzuri/build-cache`、Windows `%LOCALAPPDATA%/Tsuzuri/Cache/build-cache`）。`BuildCache::open` は
  marker `.tsuzuri-cache` のない空でないディレクトリを拒否する。`BuildCache::evict` は 64 桁 hex の名前・`.lock-<key>`・
  `.tsuzuri-*` だけを扱い、それ以外の名前（例: `packages`）には触れない。`Sha256` は `update`・`field(tag, bytes)`
  （タグと値をそれぞれ u64 LE の長さ付きで入れる）・`finalize`・`hex` を持つ。
- `src/main.rs`: `enum Action { Lsp, Check, Doc, Build, Run, Fmt, Test }`。`parse_arguments` は先頭語で分岐し、未知の語は
  `Build` の入力として扱う。`Lsp` と `Fmt` は `main` で project を読む前に分岐する。`src/` に `git` を起動するコードはない
  （`Command::new` は clang・llvm-link・dsymutil・wasm-ld・生成物・node だけ）。
- `Cargo.toml` の依存は `rustc_apfloat` と `serde_json` だけ。GUIDE §1 規則 10 により crate 追加はチケットの明記が要る。
- 文書: `docs/language.md` の `#### ローカルパッケージ` と GUIDE D-29 の E04 記録は「git、lockfile、版解決は導入しない」。
  `_docs/language-reference/modules-and-packages.md` の `## ローカルパッケージ` は未実装と書く。`docs/architecture.md` は
  「build script・ネットワーク実行はありません」。完了チケット E04 の `### Git dependencies and lockfile` は SHA-256 のために
  crate 追加を検討するとしていたが、G11 の `Sha256` で不要になった。
- git の挙動（開発機 2.55.0 で確認）: `--depth 1` で広告されていない古い commit を ID で `file://` から取得できる（protocol v2）。
  `-c protocol.allow=never` は `fatal: transport 'file' not allowed` で止まる。`ls-tree -r -z` は symlink を `120000 blob` と示す。

### 再現（検証済み）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-work-E10/app
printf '[package]\nname = "app"\nversion = "0.1.0"\n\n[dependencies]\ngeometry-core = { git = "https://example.org/geometry-core.git", rev = "0123456789abcdef0123456789abcdef01234567" }\n' > /tmp/tz-work-E10/app/Tsuzuri.toml
printf '42\n' > /tmp/tz-work-E10/app/Main.tz
target/release/tsuzuri check /tmp/tz-work-E10/app
target/release/tsuzuri fetch /tmp/tz-work-E10/app
```

```text
/private/tmp/tz-work-E10/app/Tsuzuri.toml:1:1: error[E0002]: only local path dependencies are supported
<command line>:1:1: error[E2000]: pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively
```

終了コードは 1 と 2。`--json` では span が `{"start":57,"end":172,...,"line":1,"column":1}` で、manifest の診断は行・列が常に
`1:1` と表示される（`main` が `print_diagnostic` に空のテキストを渡すため。本チケットでは直さない）。

## 仕様

### 前提とする他チケットのインターフェース

- E04（完了）: `parse_manifest`、`load_packages` の `E1011`／`E1017` の規則、package 名から名前空間を作る規則。本チケットは規則を
  変えず、git package の root を同じ走査に渡す。
- G11（完了）: `Sha256`（`field` の枠付け）、`default_root`、`BuildCache::open` の marker。変更しない。
- G19（todo）: Phase 2 の `tsuzuri publish` が公開前の API 差分検査を使う。Phase 1 は依存しない。
- G14（todo）: `git` は同梱しない。PATH の `git` を使う。

### 構文（manifest）

新構文（実装後に有効。未検証）。`[dependencies]` の値に git の形を足す。`path` の形は変えない。

```text
dependency = package-name "=" "{" source "}"
source     = "path" "=" string
           | "git" "=" string "," "rev" "=" string
```

- キーの順序は `git`、`rev` に固定する。`branch`・`tag`・`version`・その他のキー、`rev` の欠落、順序違いは `E0002`。
  空白と文字列の escape は既存の `Line`（`space`・`string`）と同じで、一つの依存は一行に書く（既存どおり）。
- url: `https://` か `file:///` で始まり、2,048 バイト以下、0x21–0x7E の ASCII だけで、`@`・`?`・`#`・`\` を含まない。
  `@` を拒むのは資格情報を URL に書かせず lockfile へ漏らさないため。`http://`・`ssh://`・`git://`・`git@host:path` は `E0002`。
- rev: 40 文字の小文字 16 進（SHA-1 の commit ID）。大文字、短縮形、64 文字（SHA-256 object format）は `E0002`。

```toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
geometry-core = { git = "https://example.org/geometry-core.git", rev = "0123456789abcdef0123456789abcdef01234567" }
local-util = { path = "../local-util" }
```

### lockfile `Tsuzuri.lock`

- 置き場所は root package の manifest と同じディレクトリだけ。依存 package の `Tsuzuri.lock` は読まない（git からは取り出さない）。
- 書式は JSON。`fetch` が書く正規形は次のとおり（`sha256` の値は例示）。

```json
{
  "format": 1,
  "packages": [
    {
      "name": "geometry-core",
      "git": "https://example.org/geometry-core.git",
      "rev": "0123456789abcdef0123456789abcdef01234567",
      "sha256": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    }
  ]
}
```

- 正規形: 2 空白の字下げ、LF、末尾の改行は 1 つ。`packages` は `name` のバイト順、要素のキーは `name`・`git`・`rev`・`sha256` の順。
  git package がなければ `"packages": []`。文字列は `json_string` で escape する。
- 読み込み（`serde_json`）: キーの順序と空白は問わない。最上位は `format`（数値 1）と `packages`（配列）だけ、要素は 4 キーだけ。
  `name` は `namespace` が受ける名前、`rev` は 40 桁、`sha256` は 64 桁の小文字 16 進、`git` は manifest と同じ URL 規則。
  重複する `name`・未知のキー・型の違いは `E2007`、1 MiB 超は `E1017`。
- 記録するのは git package だけ（path 依存は E04 のとおり記録しない）。path 依存の package が持つ git 依存も root の lockfile に入る。
- `Tsuzuri.lock` があれば、git 依存の有無に関係なく読み込んで検証し、`Project::manifests` に入れる（出力保護と build cache の key）。
- 書くのは `tsuzuri fetch` だけ。`build`／`check`／`run`／`test`／`doc`／`lsp` は書かない。

### 内容ハッシュ

package root からの `/` 区切りの相対パスをキーにした `BTreeMap<String, &[u8]>`（root の `Tsuzuri.toml` と、`collect_sources` が
選ぶ `.tz`／`.tt`／`.tc` のすべて）について、`content_sha256`（新規）で次を計算する。

```rust
let mut hash = Sha256::new();
hash.field("tsuzuri-package", b"1");
for (path, bytes) in files {          // BTreeMap の順 = パスのバイト順
    hash.field("path", path.as_bytes());
    hash.field("bytes", bytes);
}
hash.hex()                             // 64 桁の小文字 16 進
```

取得時は取り出したバイト列、ビルド時は読み込んだ `SourceFile::text` と manifest の text から計算する。`read_source_text` は
正規化しないので両者は一致する。

### コマンド `tsuzuri fetch`

新構文（実装後に有効。未検証）: `tsuzuri fetch <directory> [--json]`。

- 引数はディレクトリ一つと任意の `--json` だけ。ファイル、複数の入力、build option は `E2000`。ディレクトリに `Tsuzuri.toml` が
  なければ `E2000`。
- E04 と同じグラフ走査（D6）で git 依存を解決する。既存の lockfile に同じ `name`・`git`・`rev` の項目があり、store の内容の
  ハッシュがその `sha256` と一致すれば取得しない。そうでなければ D5 の手順で取得する。
- 取得した内容の `sha256` が既存 lockfile の同じ `name`・`git`・`rev` の値と違えば `E2007` で止め、lockfile を上書きしない。
- 走査が全部成功したときだけ lockfile を正規形で書く。内容が同じなら書かない（mtime を変えない）。グラフに現れない項目は消す。
  git 依存も lockfile もなければ作らない。書き込みは同じディレクトリの一時ファイル `.Tsuzuri.lock.<pid>.tmp` から `fs::rename` する。
- 成功時は何も出力せず終了コード 0。診断は既存の `print_diagnostic`（`--json` 対応）で出して 1、`E2000` は 2。

### ビルド時の解決（オフライン）

`build`／`check`／`run`／`test`／`doc`／`lsp` では、`load_packages` が git 依存を次のように解決し、`git` もネットワークも使わない。

1. root の `Tsuzuri.lock` がなければ `E2007`。`name` の項目がないか、`git`・`rev` が manifest と違えば `E2007`。
2. `package_store`（新規）が `None` なら `E2007`。`<store>/git/<sha256>` が実在のディレクトリ（symlink でない）でなければ `E2007`。
3. そのディレクトリを package root として E04 と同じく読み、読み終えた後の内容ハッシュが `sha256` と違えば `E2007`。

git package の manifest にある path 依存は `E1011`（D7）。同じ `name` が異なる url・rev、または path と git で現れると root が
異なるので、既存の `package name or namespace resolves to multiple roots`（`E1011`）になる（一つの名前に一つの版）。

### 取得の安全性

- `git` に作業木を作らせない。bare repository へ `fetch` し、`ls-tree` で一覧、`cat-file --batch` で blob を読み、Tsuzuri が
  自分でファイルを書く（D5）。checkout がないので hooks・filter・LFS・submodule・symlink の作成は起きない。
- 取り出す entry: パスのどの要素も `.` で始まらず、root の `Tsuzuri.toml` か `source_kind` が `Some` のもの（`collect_sources` と同じ
  選択）。それ以外（README、`.github`、入れ子の `Tsuzuri.toml`、`Tsuzuri.lock` など）は無視する。
- 取り出す entry は mode `100644`／`100755` の blob だけ。`120000`（symlink）と `160000`（submodule）は `E2007`。
- パスの要素が空・`.`・`..`・255 バイト超、`\`・`:`・NUL・制御文字を含む、`.git` と大文字小文字を無視して一致、のどれかは `E2007`。
  大文字小文字だけが違う 2 つのパスも `E2007`。ファイルは `create_new(true)` で作り、上書きしない。
- `https://` で取得した package の manifest にある `file:///` 依存は `E2007`。root、path 依存、`file:///` で取得した package は
  `file:///` を使える（D3）。
- 設定の隔離: `GIT_` で始まる環境変数と `SSH_ASKPASS` を消し、`GIT_CONFIG_NOSYSTEM=1`、`GIT_CONFIG_GLOBAL=<一時>/gitconfig`（空）、
  `GIT_TERMINAL_PROMPT=0` を設定する。proxy と証明書の環境変数（`HTTPS_PROXY`・`SSL_CERT_FILE` など）は残す。`-c` は D5 の固定値。

### 診断

メッセージの `NAME`・`URL`・`REV`・`PATH` などは実際の値に置き換える。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E0002` | `{` の後が `path`・`git` 以外 | `expected a path or git dependency`（`only local path dependencies are supported` を置き換える） | 依存の行 |
| `E0002` | git の形の違反 | `git dependencies are written as { git = "https://...", rev = "<40 lowercase hex>" }` | 依存の行 |
| `E0002` | url の違反 | `git urls must start with https:// or file:/// and must not contain credentials, spaces, '?', or '#'` | 依存の行 |
| `E0002` | rev の違反 | `rev must be a full 40-character lowercase hex commit id; branches and tags are not supported` | 依存の行 |
| `E1011` | git package の path 依存 | `git packages cannot have path dependencies; use a git dependency` | その manifest の依存の行 |
| `E1011` | 同じ名前が異なる source | 既存 `package name or namespace resolves to multiple roots` | 後から読んだ manifest |
| `E1011` | 取得した package 名とキーの不一致 | 既存 `dependency key must match the dependency package name` | 宣言した依存の行 |
| `E1017` | lockfile が 1 MiB 超 | `Tsuzuri.lock exceeds 1 MiB` | `Tsuzuri.lock` |
| `E1017` | `ls-tree` の出力が 64 MiB 超 | `git dependency 'NAME' has a tree listing larger than 64 MiB` | 宣言した依存の行 |
| `E1017` | 取り出す source が上限超 | 既存 `module discovery exceeds 4096 source files`、`module discovery exceeds 1024 directories or 16 path segments` | 宣言した依存の行 |
| `E0003` | 取り出す blob が 1 MiB 超 | 既存 `source exceeds the 1048576-byte limit` | 宣言した依存の行 |
| `E2000` | fetch の引数の誤り | `fetch takes one project directory and optionally --json` | `<command line>` |
| `E2000` | manifest がない | `fetch requires a Tsuzuri.toml in the project directory` | `<command line>` |
| `E2001` | store・lockfile の I/O 失敗 | 既存の `io_error` の形 | 対象のパス |
| `E2003` | 出力先が `Tsuzuri.lock` | 既存の出力保護のメッセージ | `<command line>` |
| `E2007` | lockfile がない | `Tsuzuri.lock is missing; run tsuzuri fetch to download git dependencies and record them` | 宣言した依存の行 |
| `E2007` | lockfile に項目がない・古い | `Tsuzuri.lock does not record git dependency 'NAME' at this url and rev; run tsuzuri fetch` | 宣言した依存の行 |
| `E2007` | store がない | `no package store is available; set TSUZURI_CACHE_DIR and run tsuzuri fetch` | 宣言した依存の行 |
| `E2007` | 未取得 | `git dependency 'NAME' is not downloaded; run tsuzuri fetch` | 宣言した依存の行 |
| `E2007` | ビルド時のハッシュ不一致 | `git dependency 'NAME' does not match its sha256 in Tsuzuri.lock; run tsuzuri fetch to restore it` | `Tsuzuri.lock` |
| `E2007` | lockfile の書式違反 | `Tsuzuri.lock is not a valid version 1 lockfile (DETAIL); restore it from version control or delete it and run tsuzuri fetch` | `Tsuzuri.lock` |
| `E2007` | cache root を開けない | `cannot use the cache directory PATH for packages (DETAIL); set TSUZURI_CACHE_DIR to an empty or Tsuzuri cache directory` | `<command line>` |
| `E2007` | `git` がない・古い | `git 2.32 or later is required to fetch git dependencies (found: VERSION)`（見つからなければ `found: none`） | 宣言した依存の行 |
| `E2007` | `git` の失敗 | `git fetch of URL at REV failed: LAST_STDERR_LINE; check the url, the commit id, and network access` | 宣言した依存の行 |
| `E2007` | https の package の `file:///` 依存 | `package 'PARENT' was fetched over https and cannot use the file:// url of 'NAME'` | その manifest の依存の行 |
| `E2007` | symlink・submodule | `git dependency 'NAME' has a symbolic link or submodule at 'PATH'; packages may contain only regular source files` | 宣言した依存の行 |
| `E2007` | 危険なパス・大文字小文字の衝突 | `git dependency 'NAME' has an unsafe path 'PATH'`、`git dependency 'NAME' has paths that differ only in case: 'A' and 'B'` | 宣言した依存の行 |
| `E2007` | root に manifest がない | `git dependency 'NAME' has no Tsuzuri.toml at the repository root of rev REV` | 宣言した依存の行 |
| `E2007` | UTF-8 でない source | `git dependency 'NAME' has a source that is not UTF-8: 'PATH'` | 宣言した依存の行 |
| `E2007` | 取得時の lockfile との不一致 | `git dependency 'NAME' at rev REV has sha256 NEW but Tsuzuri.lock records OLD; remove its entry only if you trust the new content` | `Tsuzuri.lock` |

### 資源上限

git package は E04 の 1,024 package・深さ 128・4,096 source に数える。package ごとの取り出しも `collect_sources` と同じ
4,096 source・1,024 ディレクトリ・16 段・1 file 1 MiB。lockfile 1 MiB、url 2,048 バイト、`ls-tree` の出力 64 MiB。取得の時間と
転送量には上限を設けない（明示的なコマンドで、利用者が中断できる。対象外）。

### 例

新 API（実装後に有効。未検証）。

```sh
tsuzuri fetch app          # 取得して app/Tsuzuri.lock を書く。成功時は出力なし
tsuzuri run app            # ネットワークなし。store から読む
rm app/Tsuzuri.lock && tsuzuri check app   # E2007 Tsuzuri.lock is missing ...
```

拒否: `rev = "main"`・`rev` の短縮形・`git = "http://..."`・`git = "https://user:token@host/r.git"`（`E0002`）。取得した repository の
`Shapes/Circle.tz` が symlink（`E2007`）。store のファイルを書き換えた後の `check`（`E2007`）。

### Phase 2（設計方針。D10 の承認まで着手しない）

- manifest: `name = { version = "1.2.3" }`。版要求は `MAJOR.MINOR.PATCH`（10 進、先頭 0 なし）だけで「その版以上、互換の範囲内」を
  意味する。互換の範囲は `MAJOR >= 1` なら同じ `MAJOR`、`0.x` なら同じ `0.MINOR`。範囲演算子と pre-release は受けない。
  公開する package の `[package] version` は同じ文法でなければならない。
- 版の選択は最小版選択（D8）。一つの名前には一つの版（名前空間の規則）で、互換の範囲が違う要求は `E1011`。
- registry は https の git repository に置く index（`index/<name>.json` に版ごとの `git`・`rev`・`sha256`・依存）で、取得は Phase 1 の
  仕組みをそのまま使う。HTTP client と archive の展開は要らない。lockfile には選んだ版を `version` として足す（`format` 2）。
- `tsuzuri publish` は G19 の API 差分検査を通した後、index の項目を出力するだけにする（index への反映は index repository への変更）。

## 設計

### データ構造

```rust
// src/package.rs
pub enum DependencySource {                 // 新規
    Path(PathBuf),
    Git { url: String, rev: String },
}
pub struct Dependency {
    pub source: DependencySource,           // 変更: `path: PathBuf` を置き換える
    pub span: Span,
}
pub struct LockEntry { pub git: String, pub rev: String, pub sha256: String }   // 新規
pub fn parse_lock(text: &str, source_id: usize) -> Result<BTreeMap<String, LockEntry>, Diagnostic>; // 新規、キーは name
pub fn render_lock(entries: &BTreeMap<String, LockEntry>) -> String;                           // 新規
pub fn content_sha256(files: &BTreeMap<String, &[u8]>) -> String;                              // 新規

// src/cache.rs
pub(crate) fn package_store() -> Option<PathBuf>;   // 新規: default_root().map(|root| root.join("packages"))

// src/driver.rs
pub(crate) struct GitRequest<'a> {          // 新規
    pub name: &'a str,
    pub url: &'a str,
    pub rev: &'a str,
    pub parent_url: Option<&'a str>,        // 宣言した package が git package ならその url
    pub manifest: &'a Path,                 // 宣言した manifest（診断の位置）
    pub span: Span,
}
pub(crate) struct LoadedPackage { /* 既存 3 field */ pub sha256: Option<String> }  // 追加 field、git package だけ Some
pub(crate) fn load_packages(
    directory: &Path,
    git: &mut dyn FnMut(&GitRequest) -> Result<(PathBuf, String), SourceError>, // (store 内の root, sha256)
) -> Result<Vec<LoadedPackage>, SourceError>;
```

`load_packages` の `pending` の要素に 4 つ目 `Option<(String, String)>`（その package の url と sha256）を足す。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| manifest | `src/package.rs` | `Dependency`, `DependencySource`（新規） | `path` を `source` に置き換える |
| manifest | `src/package.rs` | `parse_manifest` | `{` の後で `path` は既存の処理、`git` は url・`,`・`rev` を読み、構文の診断を出す。他は `E0002` |
| lockfile | `src/package.rs` | `LockEntry`, `parse_lock`, `render_lock`, `content_sha256`（新規） | 仕様の書式とハッシュ。`crate::cache::Sha256` を使う |
| store | `src/cache.rs` | `package_store`（新規） | `default_root` の下の `packages`。`BuildCache::open`・`evict` は変更しない |
| グラフ | `src/driver.rs` | `load_packages`, `LoadedPackage` | `pub(crate)` にし、引数 `git` を足す。依存ごとに `DependencySource` で分岐し、git は resolver の返す root を `fs::canonicalize` して積む。git package の path 依存は `E1011` |
| 読み込み | `src/driver.rs` | `Project::load_from_root` | オフラインの resolver（新規 `offline_git`）を渡す。git package の source を読んだ後に `content_sha256` を照合する。`Tsuzuri.lock` があれば読み、`manifests` に `name: "Tsuzuri.lock"` の `SourceFile` を足す |
| 読み込み | `src/driver.rs` | `source_kind` | `pub(crate)` にする（取得時の選択に使う） |
| 変更なし | `src/driver.rs`, `src/cache.rs` | `manifests` を使う 4 か所（source の索引、出力保護の 2 つの chain、`build_key`） | lockfile も同じ扱いで正しい |
| 取得 | `src/fetch.rs`（新規） | `fetch`（pub）, `Fetcher`, `git`, `download`, `parse_tree`, `read_blobs`, `store_sha256` | D5 と「アルゴリズム」 |
| 公開 | `src/lib.rs` | `pub mod fetch;` | 追加 |
| CLI | `src/main.rs` | `Action::Fetch`（新規）, `parse_arguments`, `main`, `HELP` | 先頭語 `fetch`。`Lsp`・`Fmt` と同じく project を読む前に分岐し、`tsuzuri::fetch::fetch` を呼ぶ。`HELP` に `tsuzuri fetch directory [--json]` と `git` の行 |

### 生成 IR とランタイム

変更しない。git package の source は path 依存と同じ `SourceFile` になり、同じ名前空間の module になる。ランタイムの symbol、
WASM import、IR の形は増えない。

### アルゴリズム

オフラインの resolver（`offline_git`）:

```text
lock = root の Tsuzuri.lock を一度だけ読む（なければ None）
resolve(request):
  entry = lock?.get(name)                      なし → E2007（lockfile なし／項目なし）
  entry.git == url && entry.rev == rev         違う → E2007（古い lockfile）
  dir = package_store()?/git/<entry.sha256>    None → E2007（store なし）
  symlink_metadata(dir) が実在のディレクトリ     違う → E2007（未取得）
  return (dir, entry.sha256)
```

取得（`Fetcher::resolve`）:

```text
if parent_url が https:// で url が file:/// → E2007
if old[name] が (url, rev) と一致し store_sha256(<store>/git/<sha>) == sha → new[name] = old[name]; return
check_git_version()（初回だけ。`git --version` の 3 語目の MAJOR.MINOR >= 2.32）
tmp = TemporaryDirectory::new(<store>)         // rename を同じ file system で行う
tmp/hooks（空）と tmp/gitconfig（空）を作る
git init --bare --quiet --template=<tmp/hooks> <tmp/repo.git>
git -C <tmp/repo.git> <固定の -c> fetch --quiet --depth=1 --no-tags --no-recurse-submodules <url> <rev>
listing = git -C <tmp/repo.git> ls-tree -r -z -l --full-tree <rev>   // stdout は 64 MiB まで
entries = parse_tree(listing)                   // 選択・mode・パス・大文字小文字・上限・root manifest を検査
blobs = read_blobs(repo, entries)               // cat-file --batch。大きさを listing と照合、UTF-8 を検査
tmp/package/<path> へ create_new で書く
sha = content_sha256(files)
if old[name] が (url, rev) と一致し old.sha256 != sha → E2007（取得時の不一致）
target = <store>/git/<sha>
target があり store_sha256(target) == sha → tmp を捨てる
target があり一致しない → target を tmp/stale へ rename し、tmp/package を target へ rename
target がない → create_dir_all(<store>/git)、tmp/package を target へ rename（競合で失敗したら上の 2 行へ）
new[name] = LockEntry { git: url, rev, sha256: sha }; return (target, sha)
```

`parse_tree` は NUL 区切りの各レコードを最初の TAB で分け、左側を空白で `[mode, type, oid, size]` に分ける（submodule の size は
`-`）。`read_blobs` は `std::thread::scope` で stdin に oid を書き、stdout から `<oid> blob <size>\n<bytes>\n` を順に読む
（`missing` は `E2007`）。stdin と stdout を同じ thread で扱うと pipe が詰まって止まる。`store_sha256` は `collect_sources(dir, &[])`
と root の manifest を読んで `content_sha256` を計算する。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。Rust テストは `std::env::set_var` を使わない（edition 2024 で `unsafe`）。
環境変数は子プロセス（`env!("CARGO_BIN_EXE_tsuzuri")`、`git`）にだけ渡す。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドと「再現」を実行し、`tests/modules.rs` のテスト数を記録する。
- 確認: 次がすべて成功し、「再現」の 2 つの診断が出る。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
git --version
cargo build --release --locked
cargo test --locked --test modules
node tests/cache.mjs target/release/tsuzuri
node tests/e2e.mjs target/release/tsuzuri
```

### 手順 2: manifest の git 依存

- 変更: `src/package.rs` の `DependencySource`（新規）・`Dependency`・`parse_manifest`、`src/driver.rs` の `load_packages`、
  `tests/packages.rs`（新規）。
- 内容: 「構文（manifest）」の文法と `E0002` の 4 メッセージ。`load_packages` は `DependencySource::Path` で既存の処理を行い、
  `Git` は仮に `E2007` `Tsuzuri.lock is missing; ...` を返す（手順 4 で置き換える）。
- 確認: `cargo test --locked --test packages` が `2 passed`（`manifest_accepts_git_dependencies`、
  `manifest_rejects_invalid_git_dependencies`）。`cargo test --locked --test modules` の N が手順 1 と同じ。「再現」の `check` が
  `E2007` になる。

### 手順 3: lockfile と内容ハッシュ

- 変更: `src/package.rs` の `LockEntry`・`parse_lock`・`render_lock`・`content_sha256`（新規）。
- 内容: 「lockfile」「内容ハッシュ」のとおり。テストの期待値は次の Python（独立の参照）で求めて貼る。

```sh
python3 -c 'import hashlib, struct
def f(h, t, b): h.update(struct.pack("<Q", len(t)) + t + struct.pack("<Q", len(b)) + b)
h = hashlib.sha256(); f(h, b"tsuzuri-package", b"1")
files = {"Tsuzuri.toml": b"[package]\nname = \"geometry-core\"\nversion = \"0.1.0\"\n", "Point.tz": b"def value :: i64\nfn value = 42\n"}
for p, b in sorted(files.items()): f(h, b"path", p.encode()); f(h, b"bytes", b)
print(h.hexdigest())'
```

- 確認: `cargo test --locked --test packages` が `5 passed`（`lock_renders_canonical_json_and_round_trips`、
  `lock_rejects_invalid_files`、`content_sha256_matches_independent_reference` を追加）。

### 手順 4: store とオフラインの解決

- 変更: `src/cache.rs` の `package_store`（新規）、`src/driver.rs` の `GitRequest`（新規）・`LoadedPackage`・`load_packages`・
  `offline_git`（新規）・`Project::load_from_root`・`source_kind`。
- 内容: 「ビルド時の解決」「アルゴリズム」のとおり。`Tsuzuri.lock` を `manifests` に足し、git package の source を読んだ後に
  `content_sha256` を照合する。git package の path 依存は `E1011`。
- 確認: `grep -n "manifests" src/*.rs` が「段ごとの変更」の 4 か所と今回の追加だけを示す（他の使い方があれば停止）。
  `cargo test --locked --test packages` が `7 passed`（`offline_build_reads_store_and_detects_tampering`、
  `git_packages_reject_path_dependencies` を追加）。`cargo test --locked --test modules` の N が不変。

### 手順 5: `tsuzuri fetch`

- 変更: `src/fetch.rs`（新規）の `fetch`・`Fetcher`・`git`・`check_git_version`・`download`・`store_sha256`、`src/lib.rs`、
  `src/main.rs` の `Action::Fetch`・`parse_arguments`・`main`・`HELP`。
- 内容: D4 の store、D5 の呼び出し、「コマンド」の lockfile の書き方。テストの fixture は D5 の注意に従い、`git hash-object -w --stdin`・
  一時 index（`GIT_INDEX_FILE`）への `git update-index --add --cacheinfo`・`git write-tree`・`git commit-tree` で作る
  （作業木を使わない。helper `commit_tree`（新規）をテストファイルに置く）。
- 確認: `cargo test --locked --test packages` が `9 passed`（`fetch_downloads_file_repositories_and_writes_lock`: 終了コード 0・
  出力なし、lockfile が期待した `render_lock` と一致、`check` 成功、2 回目の `fetch` で lockfile のバイト列と mtime が不変。
  `fetch_cli_rejects_bad_arguments`: 2 つの `E2000` と終了コード 2）。

### 手順 6: tree の検査

- 変更: `src/fetch.rs` の `parse_tree`・`read_blobs`・`url_allowed`（新規、pub）。
- 確認: `cargo test --locked --test packages` が `11 passed`。`fetch_rejects_unsafe_trees` は、`L.tz` の symlink・`Sub.tz` の
  submodule・`a.tz` と `A.tz`・root の manifest なし・UTF-8 でない source を `E2007`、1,048,577 バイトの blob を `E0003`、
  4,097 個の source を `E1017` で拒否し、`README.md`・`.hidden/X.tz`・`sub/Tsuzuri.toml`・`vendor` の submodule は無視して
  取得が成功する（store にそれらがない）ことを確かめる。`url_allowed_rejects_file_urls_under_https_packages` は
  `url_allowed(Some("https://a/b.git"), "file:///x")` が false、`url_allowed(None, "file:///x")` と
  `url_allowed(Some("file:///y"), "file:///x")` が true。

### 手順 7: 完全性と再取得

- 確認: `cargo test --locked --test packages` が `12 passed`（`fetch_detects_lock_mismatch_and_repairs_store`: lockfile の
  `sha256` を別の値にした `fetch` が `E2007` で lockfile を変えない。store のファイルを書き換えると `check` が `E2007`、`fetch` が
  直し、`check` が成功する。同じ名前を異なる rev で要求するグラフは `E1011`）。

### 手順 8: オフラインの保証

- 確認: `cargo test --locked --test packages` が `13 passed`（`#[cfg(unix)]` の `builds_never_run_git`: 呼ばれると marker を書く
  `git` script だけを置いたディレクトリを `PATH` にして `check` と `doc` を実行し、成功して marker がない）。

### 手順 9: E2E `tests/packages.mjs`

- 変更: `tests/packages.mjs`（新規）。`tests/cache.mjs` の `cli` と `TSUZURI_CACHE_DIR` の渡し方、`tests/e2e.mjs` の package を
  wasm32 で build して結果を読む箇所を写す。
- 内容: 一時 root に 3 つの repository を作る。`geometry-core`（`tests/modules.rs` と同じ `Point.tz` と `Main.tz`）、`other`
  （同じ `Library.tz`。`geometry-core` へ `file:///` の git 依存）、`app`（同じ `Main.tz`。両方へ git 依存）。`fetch` の後、native と
  wasm32 × `-O0`／`-O3` で結果が 42（JS で `42 + (99 - 99)` と独立に計算）。`--no-cache` でも同じ。`--emit llvm` を 2 回出して
  バイト列が一致。wasm の import は空。lockfile を消した `build` は `E2007`。
- 確認: `cargo build --release --locked && node tests/packages.mjs target/release/tsuzuri` が成功する。

### 手順 10: 文書と最終確認

- 変更: 「ドキュメント」の各ファイル。
- 確認: `cargo test --locked` が成功。`node tests/packages.mjs target/release/tsuzuri`、`node tests/cache.mjs target/release/tsuzuri`、
  `node tests/e2e.mjs target/release/tsuzuri`、`node scripts/check-docs.mjs _docs/language-reference/modules-and-packages.md _docs/tools/command-line.md`
  が成功。GUIDE §10 の完了の定義を満たす。

## テスト計画

### Rust テスト

`tests/packages.rs`（新規）の 13 テスト。library API（`tsuzuri::package::*`、`tsuzuri::fetch::url_allowed`）は直接呼び、取得と
ビルドは `env!("CARGO_BIN_EXE_tsuzuri")` を `TSUZURI_CACHE_DIR` 付きで起動する。各テストは自分の一時 root（プロセス ID と時刻の名前、
`tests/modules.rs` と同じ作り方）を使う。

| テスト | 確かめること |
| --- | --- |
| `manifest_accepts_git_dependencies` | git と path の混在、`Dependency::source` の値、空白の違い |
| `manifest_rejects_invalid_git_dependencies` | `rev` なし・順序違い・`branch`・短縮 rev・大文字 rev・64 桁 rev・`http://`・`ssh://`・`git@h:r`・`@`・`?`・`#`・2,049 バイトの url・未知の source キー。すべて `E0002` とメッセージ |
| `lock_renders_canonical_json_and_round_trips` | 順不同の入力を `parse_lock` → `render_lock` で正規形（`"packages": []` を含む）、2 回目で不変 |
| `lock_rejects_invalid_files` | `format` 2・未知キー・重複 name・短い sha256・不正 url は `E2007`、1 MiB 超は `E1017` |
| `content_sha256_matches_independent_reference` | 手順 3 の Python の値 |
| `offline_build_reads_store_and_detects_tampering` | 手で作った store と lockfile で `check` 成功。改ざん・lockfile なし・項目なし・rev 違い・store なしの `E2007` |
| `git_packages_reject_path_dependencies` | store 内の package の path 依存が `E1011` |
| `fetch_downloads_file_repositories_and_writes_lock` | 手順 5 |
| `fetch_cli_rejects_bad_arguments` | `fetch`・`fetch a b`・`fetch Main.tz`・`fetch dir -O3`・manifest なしが `E2000` |
| `fetch_rejects_unsafe_trees` | 手順 6 |
| `url_allowed_rejects_file_urls_under_https_packages` | 手順 6 |
| `fetch_detects_lock_mismatch_and_repairs_store` | 手順 7 |
| `builds_never_run_git` | 手順 8 |

### E2E

`tests/packages.mjs`（新規、手順 9）。期待値 42 は JS の式で独立に求める。codegen を変えないので `live == 0` の検査は足さない。

### 既存テストへの影響

なし。`only local path dependencies are supported` を assert するテストはない。`tests/modules.rs` と `tests/cache.mjs` は変更しない。

### 性能

ビルド時に git package の source へ SHA-256 を 1 回かける。計測はしておらず、性能の主張はしない。閾値は設けない。

## ドキュメント

- `docs/language.md` の `#### ローカルパッケージ`: 「ネットワーク、git、lockfile、版解決…は導入しません」を、git 依存・`Tsuzuri.lock`・
  `tsuzuri fetch`・オフラインのビルド・`E2007` の説明に置き換える（版解決と build script は引き続きなし）。診断の一覧に `E2007`。
- `docs/architecture.md`: ファイルの責務に `src/fetch.rs`、package 読み込みの段落の「ネットワーク実行はありません」を
  「`fetch` だけが `git` を起動する」に。
- `README.md`: `## ファイルとモジュール` の package の段落と `## ビルド` のコマンド一覧に `fetch`。
- `_docs/language-reference/modules-and-packages.md`: `## ローカルパッケージ` の「未実装」の文を直し、`## git 依存と Tsuzuri.lock`（新規）。
- `_docs/tools/command-line.md`: `## コマンド` に `fetch`、`TSUZURI_CACHE_DIR` の行に `packages/` の説明。
- `_docs/feature-status.md` と `_features/README.md` の E10 の状態（Phase 1 完了、Phase 2 未着手）。
- GUIDE §9（D-29 の E04 記録、D-30 の `E2007`）は人間が D1 の承認時に更新する。実装者は GUIDE を編集しない。

## 受け入れ条件

- [ ] commit 固定の git 依存を `tsuzuri fetch` で取得し、`Tsuzuri.lock` を正規形で書き、オフラインでビルド・実行できる（native と wasm32、`-O0`／`-O3`）。
- [ ] `build`／`check`／`run`／`test`／`doc`／`lsp` が `git` を起動しない（`builds_never_run_git`）。
- [ ] lockfile の欠落・古さ・改ざん・未取得を「診断」の `E2007` で拒否し、`fetch` が store を直す。
- [ ] 安全でない URL・symlink・submodule・危険なパス・大文字小文字の衝突・上限超過を拒否する。
- [ ] Rust の crate を追加していない（`Cargo.toml` の `[dependencies]` が不変）。
- [ ] `tests/packages.rs` の 13 テストと `tests/packages.mjs` が成功し、既存テストの期待値を変えていない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `BuildCache::open` は marker のない空でないディレクトリを拒否する。`fetch` が `BuildCache::open` より先に `packages/` を作ると、
  以後の build が `W2001`（cache disabled）になる。`fetch` は必ず先に `BuildCache::open(root)` を呼ぶ（D4）。
- 利用者の git 設定（`commit.gpgsign`、`init.templateDir`、hooks）がテストの fixture 作成を壊す。fixture の `git` にも空の
  `GIT_CONFIG_GLOBAL`・`GIT_CONFIG_NOSYSTEM=1`・`GIT_AUTHOR_NAME` などを渡す。macOS の file system は大文字小文字を区別しないので、
  `a.tz` と `A.tz` は作業木では作れない。`commit_tree` で作る。
- macOS の `/tmp` は `/private/tmp` への symlink。store の entry の symlink 検査は canonicalize の前に行い、root には canonicalize した
  パスを使う（`load_packages` は canonical な root で package を同一視する）。
- `cat-file --batch` の stdin と stdout を同じ thread で扱うと pipe が詰まって止まる。`ls-tree -l` の size は右寄せで空白が入り、
  submodule は `-`。
- `fetch` から `Project::load` を呼ばない（入口の選択が要る）。`load_packages` を直接使う。部分的に失敗した `fetch` は lockfile を書かない。
- ハッシュの鍵は repository 内の相対パスで、`SourceFile::relative_path`（名前空間付き）ではない。
- LSP の overlay で store 内のファイルを編集すると内容ハッシュが変わり `E2007` になる。正しい挙動なので回避しない。
- `/dev/null` は Windows にないので `GIT_CONFIG_GLOBAL` には一時ディレクトリの空ファイルを使う。

## 対象外

- Phase 2（registry、版要求、最小版選択、`tsuzuri publish`）。tarball・URL archive の依存。repository の下位ディレクトリにある package。
- branch・tag の追跡、`tsuzuri add`・`update`・`--locked`・`--offline`、認証（credential helper・SSH 鍵）、`ssh://`・`http://`。
- SHA-256 object format の repository、store の GC、取得の時間・転送量の上限、build script、ネイティブライブラリの取得。
- Windows の予約名（`CON` など）の検査（G10）。manifest の診断が `1:1` と表示される問題。

## 決定事項

### D1: 取得の手段と診断コード

- 決定: 取得は明示的なコマンド `tsuzuri fetch` だけが行い、PATH の外部 `git`（2.32 以降）を起動する。HTTP・TLS・archive の crate は
  足さない。取得と検証の失敗は `E2007`（GUIDE D-30 の仮割り当て）。GUIDE D-29 の E04 記録「git、lockfile を導入しない」を更新する。
- 理由: git は任意のホスティングとローカルの bare repository で動き、TLS・proxy・protocol v2 を任せられる。crate を足すと供給網と
  ビルド時間が増える（GUIDE §1 規則 10）。ビルドの経路から分離すればオフラインの保証を保てる。
- 状態: 要承認（承認前は Phase 1 のどの手順にも着手しない）

### D2: manifest の構文

- 決定: `{ git = "<url>", rev = "<40 桁の小文字 16 進>" }` をこの順序だけで受ける。branch・tag・短縮 rev は受けない。
- 理由: 固定の順序は既存の `Line` の解析のまま書け、書き方が一つに決まる。完全な commit ID だけが取得物を一意に決める。
- 状態: 既定案（実装者はこの案に従う）

### D3: URL の方針

- 決定: `https://` と `file:///` だけ。`https://` で取得した package は `file:///` 依存を持てない。資格情報・query・fragment を拒む。
  テスト用の環境変数は設けない。
- 理由: 遠隔の package が利用者の機械のローカル repository を読ませる経路を塞ぎつつ、テストと社内ミラーは `file:///` で動く。
- 状態: 既定案（実装者はこの案に従う）

### D4: store の場所と形

- 決定: `default_root()` の下の `packages/git/<sha256>/`。`fetch` は `BuildCache::open(root)` の後で `packages/` を作り、staging は
  `TemporaryDirectory::new(<store>)`、確定は `fs::rename`。`--no-cache` は store に影響しない。新しい環境変数は作らない。
- 理由: `TSUZURI_CACHE_DIR` だけで隔離でき（`tests/cache.mjs` と同じ）、`evict` は `packages` に触れない。内容で鍵を付けると lockfile
  から場所が一意に決まる。
- 状態: 既定案（実装者はこの案に従う）

### D5: git の呼び出し方

- 決定: `git init --bare --quiet --template=<空>`、`fetch --quiet --depth=1 --no-tags --no-recurse-submodules <url> <rev>`、
  `ls-tree -r -z -l --full-tree <rev>`、`cat-file --batch` だけを使う。固定の `-c` は `protocol.allow=never`、
  `protocol.https.allow=always`（`file:///` の依存だけ `protocol.file.allow=always`）、`transfer.fsckObjects=true`、
  `core.hooksPath=<空のディレクトリ>`、`http.followRedirects=false`、`credential.helper=`、`fetch.recurseSubmodules=false`。環境は
  「取得の安全性」の隔離に従う。git の失敗は stderr の最後の空でない行を `E2007` に入れる。
- 理由: 作業木を書かせないので、symlink・hooks・filter・submodule・パスの検査をすべて Tsuzuri 側で一度だけ行える。
- 状態: 既定案（実装者はこの案に従う）

### D6: グラフ走査の共有

- 決定: `load_packages` に resolver（`&mut dyn FnMut(&GitRequest) -> Result<(PathBuf, String), SourceError>`）を渡し、ビルドは
  `offline_git`、取得は `Fetcher` を使う。
- 理由: E04 の循環・上限・名前空間・symlink の規則が取得とビルドで必ず同じになる。
- 状態: 既定案（実装者はこの案に従う）

### D7: git package の範囲

- 決定: package root は repository の root。git package は path 依存を持てない（`E1011`）。
- 理由: path 依存は store の外や repository の外を指せる。下位ディレクトリの package は Phase 2 以降で必要になってから決める。
- 状態: 既定案（実装者はこの案に従う）

### D8: 版の解決

- 決定: Phase 1 は解決しない（完全な commit 固定、一つの名前に一つの source）。Phase 2 は最小版選択にする。以前の既定案
  （Cargo 互換: 互換範囲の最大版 + lockfile）は採らない。
- 理由: 最小版選択は lockfile がなくても決定的で、backtracking が要らず、一つの名前に一つの版という名前空間の規則と両立する。
- 状態: 既定案（実装者はこの案に従う）。Phase 2 の着手は D10 に従う。

### D9: lockfile の形とコマンドの数

- 決定: JSON の `Tsuzuri.lock`（`format` 1、`name`・`git`・`rev`・`sha256`）。`fetch` だけが書き、既存の `sha256` を上書きしない。
  `add`・`update`・`--locked` は作らない。
- 理由: `serde_json` は依存にあり、正規形を手で出せば決定的になる。rev は manifest に固定されるので、`fetch` と既存の `sha256` の
  保護で CI の再現性は足りる。
- 状態: 既定案（実装者はこの案に従う）

### D10: Phase 2 の registry

- 決定: index は https の git repository（`index/<name>.json`）に置き、取得は Phase 1 の仕組みを使う。HTTP の sparse index と
  archive の配布は採らない。運用主体、index の URL、公開の審査を人間が決める。
- 理由: crate も新しい取得経路も要らない。運用には費用と責任が伴う。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D11: 内容ハッシュの定義

- 決定: 「内容ハッシュ」の `content_sha256`（`Sha256::field` の枠付け、パスのバイト順、manifest を含む）。
- 理由: git の SHA-1 と独立に取得物を固定でき、取得時とビルド時で同じ関数を使える。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-07）

利用者が D1・D10 を含む全 `要承認` 項目を承認し、Phase 1 と Phase 2 の実装を求めた（共通指示）。着手時の HEAD は `2ee813f`
（チケットの詳細化は `f8dc655`）。ブランチは `wt/e10`。性能は主張しない（ビルド時のハッシュの計測はしていない）。

### Phase 1: commit 固定の git 依存、`tsuzuri fetch`、`Tsuzuri.lock`、オフラインのビルド

#### 実装

- manifest（`src/package.rs`）: `DependencySource::{Path, Git { url, rev }}` と `Dependency::source`（E12 の `native` は path 依存だけ）。
  `{ git = "<url>", rev = "<40 hex>" }` を `Line::git_dependency` で読み、形・url・rev の違反をチケットの 3 つのメッセージ
  （`GIT_FORM`・`GIT_URL_RULE`・`GIT_REV_RULE`、公開定数）で `E0002`。他のキーは `expected a path or git dependency`。
  `valid_git_url`・`valid_rev`、`LockEntry`、`parse_lock`（`serde_json` で読み、重複キーは自前の走査 `duplicate_json_key` で検出）、
  `render_lock`（正規形）、`content_sha256`（`Sha256::field` の枠付け）。
- store（`src/cache.rs`）: `package_store()`（`default_root()/packages`）。
- グラフ（`src/driver.rs`）: `load_packages(directory, git)` を `pub(crate)` にし、resolver（`GitResolver`、`GitRequest`）を受ける。
  依存ごとに source で分岐し、git は resolver の返す root を `canonicalize` して積む。path 依存の検査は `dependency_directory` に分けた。
  `LoadedPackage::sha256`。git package の path 依存は `E1011`。`Project::load_from_root` は `read_lockfile`（manifest があるときだけ読む。
  symlink・1 MiB 超・UTF-8 でない・形式違反は `E2007`／`E1017`）と `offline_git`（git も network も使わない）を渡し、git package の
  source を読んだ後に `content_sha256` を照合する。`Tsuzuri.lock` は `Project::manifests` に入る（出力保護と `build_key` はそのまま効く）。
  git package の `[native]` は `E2000`（専用のメッセージ）。`collect_sources`・`read_source_text`・`source_kind`・`SourceError::new` を `pub(crate)` にした。
- 取得（`src/fetch.rs`、新規）: `fetch`、`url_allowed`（公開）、`Fetcher`（lockfile と store が一致すれば git を起動しない、取得時の sha256 の不一致は
  `E2007` で lockfile を保持）、`Repository`（隔離した一時 bare repository。`init --bare --template=<空>`、`fetch --depth=1 --no-tags
  --no-recurse-submodules`、`ls-tree -r -z -l --full-tree`（64 MiB で打ち切り）、`cat-file --batch`（書き込みと読み出しを別 thread））、
  `parse_tree`（選択・mode・パス・大文字小文字・上限・root manifest）、`Download::commit`（`<store>/git/<sha256>` へ rename、壊れた entry は退避して置換、競合は再確認）、
  `store_sha256`、`write_lock`（同じ内容なら書かない、`.Tsuzuri.lock.<pid>.tmp` から rename）。
- CLI（`src/main.rs`）: `tsuzuri fetch directory [--json]` を `new` と同じく `parse_arguments` の前で分岐（`fetch_dependencies`）。`HELP` に追記。
- テスト: `tests/packages.rs`（13 件、チケットの表のとおり）、`tests/packages.mjs`（file:/// の 3 repository、native・wasm32 × `-O0`／`-O3`、`--no-cache`、IR の決定性、
  WASM import なし、lockfile 削除後の `E2007`）。`src/cache.rs` の単体テスト `eviction_keeps_the_marker_and_the_package_store`（下の 7）。
- 文書: `_tsuzuri/language-reference/organizing-tsuzuri/packages.md`（`## git 依存と Tsuzuri.lock` ほか）、`compiler/usage.md`（`### fetch`、`packages/`）、
  `compiler/diagnostics.md`（`E2007`）、`languages/strategy.md`、`docs/language.md`（`#### git 依存と Tsuzuri.lock`、診断表）、`docs/architecture.md`、`README.md`。

#### 決定事項への追記（チケットから外れた判断）

1. **名前空間（D-35／D-37）への追従。** 明示の `namespace` で package 名と名前空間が独立したため、「同じ名前が異なる source」を名前空間の衝突に頼れない。
   `load_packages` が依存の辺ごとに名前と取得元（path、または url と rev）の対応を記録し、食い違いを宣言した依存の行で
   `E1011` `package 'NAME' is required from different sources; ...` にする。path 依存どうしは従来どおり（同じ名前でも名前空間が違えば受理）。
   root package の名前も path として登録するので、root と同じ名前の git 依存も `E1011`。オフラインでは、root が新しい rev を要求すると
   その前に lockfile の照合が `E2007`（古い lockfile）になり、`fetch` が `E1011` を報告する。
2. **E12 の `native` への追従。** git 依存の形は `{ git, rev }` だけなので `native = true` は `E0002`。git package の `[native]` は、
   既存の「mark the dependency」を勧める文ではなく `git packages cannot declare [native] link settings; move them to the application manifest`（`E2000`）。
3. **lockfile の書式違反の文。** Phase 2 で `format` 2 を足すため `Tsuzuri.lock is not a valid lockfile (DETAIL); ...` とした（チケットは `version 1 lockfile`）。
   UTF-8 でない lockfile と symlink の lockfile も `E2007`。
4. **git の失敗の文。** stderr の最後の空でない行では、repository がないときに `and the repository exists.` だけになる。
   最初の `fatal:`／`error:` の行（なければ最後の空でない行）を入れる。
5. **git の起動。** すべての呼び出しに `--git-dir=` を明示して repository の探索をさせない（store が利用者の repository の中にあっても安全）。`LC_ALL=C`。
6. **テストの fixture。** `update-index --cacheinfo` は `a:b.tz` や `\` を含むパスを拒むので、`hash-object -w`・`mktree -z`・`commit-tree`・`update-ref` で作る
   （作業木を使わない点は同じ）。一時 root は OS の一時ディレクトリではなく `CARGO_TARGET_TMPDIR`（`target/tmp`）に置く。
7. **関連する既存の不具合を直した。** `BuildCache::evict` は `.tsuzuri-` で始まる名前を staging とみなすため、作成から 30 日を過ぎた
   ownership marker `.tsuzuri-cache` を消していた。以後 `BuildCache::open` が root を拒み、ビルドは cache なしになり、`fetch` は
   `cannot use the cache directory` で止まる（D4 は `fetch` が先に `BuildCache::open` を呼ぶ）。marker を対象から外し、単体テストで確かめた（修正前は失敗）。

#### 確認（Phase 1）

- `cargo test --locked --test packages`: 13 passed。`--test modules`: 21 passed（手順 1 と同じ）。`--lib`: 99 passed。`--bin tsuzuri`: 11 passed。
- `node tests/packages.mjs target/release/tsuzuri`、`node tests/cache.mjs target/release/tsuzuri`、`node tests/e2e.mjs target/release/tsuzuri`: 成功。
- `node scripts/check-docs.mjs`（変更した LR の 4 ページ）: 成功。`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
