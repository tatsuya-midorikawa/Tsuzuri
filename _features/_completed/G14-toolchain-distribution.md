# G14: 自己完結ツールチェーンの配布

| 項目 | 内容 |
| --- | --- |
| ID | G14 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | (G10) |
| 後続 | G15, G16 |
| 状態 | done（Phase 1） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1 は不要。要承認: D9（macOS の公証・Windows の Authenticode 署名）, D10（配布物の署名・来歴証明と GitHub Releases への自動公開）, D11（Node.js の同梱）。いずれも Phase 2 |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: VS Code 拡張以外では LLVM・Clang・LLD・Node.js を利用者が別途導入する必要がある（rustup や .NET SDK に相当する一括導入がない） |
| 手本にする既存実装 | 同梱・再配置・manifest: `vsc/scripts/toolchain.mjs` の `bundle`・`native`・`licenses`・`entries`・`verify`・`download`。再配置後の smoke: `vsc/scripts/smoke.mjs`。archive と manifest の照合と `.sha256` の書式: `vsc/scripts/package.mjs` の `verifyArchive`。PATH 探索: `src/cache.rs` の `executable_path`。CLI の早期処理: `src/main.rs` の `--version`。実行ファイルを使う Rust テスト: `tests/test_runner.rs` の `env!("CARGO_BIN_EXE_tsuzuri")` |
| 主な影響ファイル | `scripts/toolchain/bundle.mjs`（新規。`vsc/scripts/toolchain.mjs` から移す）, `scripts/toolchain/archive.mjs`（新規）, `scripts/toolchain/smoke.mjs`（`vsc/scripts/smoke.mjs` から移す）, `scripts/toolchain/clang.rs`（`vsc/scripts/clang.rs` から移す）, `vsc/scripts/toolchain.mjs`, `vsc/scripts/package.mjs`, `vsc/src/workflow.ts`, `vsc/package.json`, `src/driver.rs`, `src/cache.rs`, `src/main.rs`, `tests/toolchain.rs`（新規）, `.github/workflows/vscode.yml`, `README.md`, `_docs/get-started.md`, `_docs/tools/build-and-cache.md`, `_docs/tools/command-line.md`, `vsc/README.md`, `vsc/Publishing.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

コンパイラ・Clang・LLD・リンク用の SDK/libc を一つの配布物にまとめ、VS Code を使わない CI・サーバー・他のエディターでも、
VSIX と同じツールチェーンを LLVM の別途導入なしで使えるようにする。

Phase 1 の「配布」は次の 3 点に限る。

- ホストごとの archive `tsuzuri-<version>-<host>.tar.gz`（Windows は `.zip`）と `.sha256` を、VSIX と共通の同梱処理から作る。
- コンパイラが archive 内の同梱ツールを実行ファイルからの相対位置で見つける（環境変数 → 同梱 → `PATH`）。`tsuzuri toolchain info` で確かめられる。
- 手動の導入・削除手順（チェックサムの検証を含む）を文書にする。CI は archive を artifact として作り、smoke を通す。

署名・公証・Release への自動公開・導入スクリプト・Node.js の同梱は Phase 2（要承認）である。実装者は Phase 1 だけを実装する。

## 着手条件と停止条件

### 着手条件

- 依存は `(G10)` だけで、着手条件にしない。G10 が blocked の間、Windows の archive は CI で作るが「未検証」と明記する（D8）。
  確認: `grep -n "| G10 \|| G14 " _features/README.md`。
- 承認は Phase 1 に不要。D9〜D11 は Phase 2 の条件であり、Phase 1 では該当コードを書かない。
- GUIDE §2.3 の基準コマンドが成功すること。加えて macOS arm64 で VSIX の同梱が通ること（手順 1 のベースライン）。必要なもの:
  `brew install llvm@21 lld`、Node 24、Rust stable。Linux で試すときは `patchelf` と `apt.llvm.org` の `clang-21 lld-21 llvm-21`（`.github/workflows/vscode.yml` と同じ）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- script の移動（手順 2）で、`vsc/toolchain/manifest.json` の `files` が「`codelldb.vsix` が抜ける」「`licenses/` に D6 のファイルが増える」以外に変わる。
- 同梱ツールの探索で、`target/release/tsuzuri` などの開発用ビルドが同梱段を選ぶ（manifest がないのに検出される）。
- 既存テストの期待値を変える必要がある（`tests/e2e.mjs` の `--version`、`tests/cache.mjs` の wrapper clang、`vsc` の試験）。
- `unsafe`、新しい crate（tar・zip・hash など）、`TSUZURI_*` の意味の変更（空文字列の扱いを含む）が必要になった。
- 同梱ファイルのライセンスの出典が見つからない（HEAD の `licenses` は Linux で `Missing redistribution license` を投げる。例外を足して黙って進めない）。
- shim（`clang.rs`）の target の扱いを変えたくなった。target の写像は G15 の手順 10 の担当である。
- `vsc/toolchain` の配置を、`codelldb.vsix` を外へ出す以外に変える必要が出た。
- push・tag・Release の作成、署名用の証明書や秘密鍵が必要になった（D9・D10）。
- `tar` が実行権限を落とす、macOS で `._*` を足すなどを、手順 5 の環境変数・引数で防げない。

## 現状（HEAD `f8dc655` で確認）

- README の「ビルド」は、利用者が LLVM/Clang 17 以降・`wasm-ld`・（検証に）Node.js 20 以降を導入し、`TSUZURI_CLANG`・`TSUZURI_WASM_LD` で指定する手順だけを書く。
  CLI 単体の配布物・Release はない。
- `src/driver.rs` の `tool(variable, fallback)` は `env::var_os(variable).unwrap_or_else(|| fallback.into())` だけで、同梱の段はない。
  呼び出し元は `src/driver.rs`（`TSUZURI_CLANG`・`TSUZURI_LLVM_LINK`・`TSUZURI_DSYMUTIL`・`TSUZURI_WASM_LD`）と `src/test_runner.rs`（`use super::*` で同じ `tool` を使う。
  `TSUZURI_CLANG`・`TSUZURI_WASM_LD`）。失敗は `run_tool(command, hint)` が E2002 にする（hint 例: `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable`）。
- `src/cache.rs` の cache key は、`TSUZURI_CLANG` などの環境変数と `PATH` を hash し、emit に要るツールを `tool` と同じ式
  （`std::env::var_os(variable).unwrap_or_else(|| fallback.into())`。重複）で決め、`executable_path`（`PATH` を探して `fs::canonicalize`）・
  `file_digest`・`--version` の出力を hash する。コンパイラ自身も `current_exe` の digest を入れる。
- `src/main.rs` の `--version` は `tsuzuri {CARGO_PKG_VERSION}`（`tsuzuri 0.1.0`）を出す。`tests/e2e.mjs` が `/^tsuzuri 0\.1\.0/` を確かめる。
  `HELP` の `Toolchain:` 欄は `TSUZURI_CLANG` と `TSUZURI_WASM_LD` だけ。
- WASM の言語内テストは `src/test_runner.rs` が `PATH` の `node` を起動する（`Command::new("node")`）。対応する環境変数はない。
- ランタイムはコンパイラに埋め込まれている（`src/driver.rs` の `include_str!("runtime/task.c")`・`cpu.c`・`io.c`・`wasm.ll` など）。
  配布物にランタイムのファイルは要らない。`src/runtime/musl/` は musl 1.2.5 の数学関数（MIT。`COPYRIGHT`）で、生成済みの `math.ll` を経てコンパイラと生成物に入る。
- VSIX の同梱は `vsc/scripts/toolchain.mjs` の `bundle` が行う。
  - `LLVM_PREFIX` の `clang --version` が `clang version 21.` であることを要求する。wasm-ld だけは `TSUZURI_WASM_LD` があればそれを使う。
  - 取得物と固定 SHA-256: Zig 0.16.0（`platforms`。win32-arm64 以外）、llvm-mingw 20251216（`mingwChecksums`。win32-arm64）、CodeLLDB 1.12.3（`debuggerChecksums`。win32-arm64 以外）。
    cache は `target/vsc-downloads/`。
  - `llvmTools`: `clang`・`wasm-ld`、macOS は `dsymutil`・`llvm-link`、win32-arm64 は `ld.lld`。`native` が依存 dylib/so を `lib/` へ写し、
    macOS は `install_name_tool` と ad-hoc 署名（`codesign --force --sign -`）、Linux は `patchelf --set-rpath`（`bin/` は `$ORIGIN/../lib`）。
  - ライセンス: `licenses` が各ファイルの親 4 段の `LICENSE`・`COPYING`・`NOTICE` を写し、Linux は `dpkg-query` の copyright を要求する。
    ほかに `Zig.txt`・`llvm-mingw.txt`・`mingw-w64-*`・`Tsuzuri.txt`。musl の `COPYRIGHT` と Rust crate のライセンスは入らない。
  - コンパイラは `cargo build --release --locked`（Windows は `+crt-static`）、shim は `rustc --edition=2024 -O vsc/scripts/clang.rs`。
  - manifest: `{ version: 1, platform, arch, compiler, zig, mingw, llvm: 21, debugger, id, files }`。`files` は `entries` が作る
    `{ path, size, sha256 }` の名前順の列（symlink は拒否）、`id` は `JSON.stringify(files)` の SHA-256。`verify` は host・`files`・`id`・必須ツールの実行権限・
    `tsuzuri --version`・`codelldb.vsix` の digest を確かめる。
- 配置（`vsc/toolchain/`）: `bin/`（`tsuzuri`・`tsuzuri-clang`・`clang`・`wasm-ld`・`llvm-link`・`dsymutil`）、`lib/`、`zig/`、`licenses/`、`codelldb.vsix`、`manifest.json`。
  2026-09-30 の darwin-arm64 の手元の同梱物は 802 MiB・19,569 files（`bin` 5.5 MiB、`lib` 348 MiB、`zig` 402 MiB）。`licenses/` は 8 files で、
  LLVM 21.1.8 と並んで 23.1.1 の `LICENSE.TXT` がある（`TSUZURI_WASM_LD` に Homebrew の最新 `lld` を渡したため。manifest は `llvm: 21` しか記録しない）。
  `otool -l vsc/toolchain/bin/clang` の `minos` は 27.0（Homebrew の bottle はビルド機の macOS 向け）。
- shim `vsc/scripts/clang.rs` の `main` は、`current_exe()` の 2 段上を root とし、`TSUZURI_ZIG` か `<root>/zig/zig`、`TSUZURI_LLVM_CLANG` か `<root>/bin/clang` を使う。
  `--target=wasm32-unknown-unknown`・`--target=x86_64-pc-windows-msvc`・`--target=aarch64-pc-windows-msvc` を捨て、`zig cc` の target を
  ビルド機から決める（`<arch>-macos.12.0`・`<arch>-linux-musl`・`<arch>-windows-gnu`）。Linux の生成物は Zig の musl で静的に link される。
- `vsc/src/toolchain.ts` の `toolchain` は `tsuzuri.toolchainPath`（既定は拡張の `toolchain`）の manifest の `platform`・`arch`・`id` を確かめ、
  `TSUZURI_CLANG=<root>/bin/tsuzuri-clang`・`TSUZURI_LLVM_CLANG`・`TSUZURI_WASM_LD`・`TSUZURI_LLVM_LINK`・`TSUZURI_DSYMUTIL`・`TSUZURI_ZIG`・`TSUZURI_CACHE_DIR` を明示する。
  `vsc/src/workflow.ts` は `path.join(tools.root, 'codelldb.vsix')` を読む。
- `vsc/scripts/smoke.mjs` は第 1 引数の toolchain を空白入りの一時パスへ写し、`PATH`・`TSUZURI_*`・`ZIG_*`・`DYLD_*`・`LD_LIBRARY_PATH` を消し、
  `SDKROOT`・`DEVELOPER_DIR` を存在しないパスにし、`PATH` を同梱の `bin` だけにし、4 つの `TSUZURI_*` を同梱ツールへ明示して、native の `-O0`/`-O3`（`-g`、実行、
  言語内テストの成功と失敗、Task）、WASM の `-O0`/`-O3`（build と instantiate）、`fmt` を試す。
- `vsc/scripts/package.mjs` は VSIX の各 entry を manifest の SHA-256 と照合し、`<vsix>.sha256`（`<hex>  <name>` と改行）を書く。secret scan は `toolchain/` と画像を除く。
- `.github/workflows/vscode.yml` の job `package` は 6 host（`macos-15`・`macos-15-intel`・`ubuntu-24.04`・`ubuntu-24.04-arm`・`windows-2022`・`windows-11-arm`）で
  LLVM 21 を入れ（Windows は 21.1.8 の installer を SHA-256 で確認）、`npm run toolchain`・`npm run test:toolchain`・VSIX の作成と試験を行い、`vsc/dist/*.vsix`・`*.sha256` を
  upload する。trigger は `workflow_dispatch`・PR の paths・tag `vsc-v*`。`permissions: contents: read` で、Release は作らない。
- 旧版のこのチケットの「Node 24 を同梱」は誤り。`vsc/scripts/toolchain.mjs` に Node の取得はない。
- 検証状況: VSIX は macOS arm64 だけ実機で確認済み（`vsc/README.md`）。他の host は CI の結果だけで、Windows は G10 が blocked。

### 再現（2026-09-30 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri --version
# tsuzuri 0.1.0
mkdir -p /tmp/tz-work-G14/toolchain && cd /tmp/tz-work-G14
/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri toolchain info
# <command line>:1:1: error[E2000]: pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively
# 終了コード 2
```

## 仕様

### 前提とする他チケットのインターフェース

- G10（blocked のまま前提にしない。D8）: Windows の archive は同じ script で CI が作るだけで、MSVC target の扱い（`src/driver.rs`）は変えない。
- G15（後続）: 本書の探索順（環境変数 → 同梱 → `PATH`）、shim の場所 `scripts/toolchain/clang.rs`、配置（D1）を前提にする。
  shim の `--target` の写像は G15 の手順 10 が変える。G14 は shim の中身を変えない。
- G16（後続）: 配布物へ足す実行ファイルは `bin/`、ライセンスは `licenses/` に置く。`entries` が manifest へ自動で載せる。
- E10: `git` は同梱しない（E10 の「前提とする他チケットのインターフェース」と一致）。PB01: 事前ビルドの runtime object は Phase 1 に含めない。
  足すときは同じ tree の下に置き、manifest が自動で覆う。

### 配布物

- 名前: `tsuzuri-<version>-<host>.tar.gz`（`darwin-*`・`linux-*`）、`tsuzuri-<version>-<host>.zip`（`win32-*`）。`<version>` は manifest の `compiler`
  （`tsuzuri 0.1.0`）から `tsuzuri ` を除いた値で、`Cargo.toml` の `version` と同じ。`<host>` は Node の `${process.platform}-${process.arch}`
  （`darwin-arm64`・`darwin-x64`・`linux-x64`・`linux-arm64`・`win32-x64`・`win32-arm64`。VSIX の名前と同じ）。
- 中身: 最上位のディレクトリ `tsuzuri-<version>-<host>/` が一つだけあり、その下は D3 の後の `vsc/toolchain/` と同一（D1）。

```text
tsuzuri-0.1.0-darwin-arm64/
  manifest.json
  bin/      tsuzuri  tsuzuri-clang  clang  wasm-ld  llvm-link  dsymutil   （Linux は llvm-link・dsymutil なし。win32-arm64 は ld.lld あり）
  lib/      <hash12>-<name>.dylib …（Linux は .so、Windows は bin/ に .dll）
  zig/      zig  lib/…（win32-arm64 は mingw/）
  licenses/ Tsuzuri.txt  Zig.txt  musl-COPYRIGHT.txt  rust-crates.txt  crate-* …
```

- entry は通常ファイルとディレクトリだけ。symlink・`._*`・拡張属性の PAX header を含めない。実行ファイルの mode（`0755`）を保つ。
- チェックサム: `<archive>.sha256` に `<64 桁の小文字 hex>  <archive のファイル名>` と改行を書く（`vsc/scripts/package.mjs` と同じ書式。
  `shasum -a 256 -c` と `sha256sum -c` がそのまま読む）。
- manifest: HEAD の項目から `debugger` を除き（D3）、`toolVersions`（新規）を足す。`toolVersions` は `llvmTools` の各名前から、
  その `--version` の出力のうち `version` を含む最初の行（なければ最初の空でない行）の trim への object。`version` は 1 のまま
  （`vsc/src/toolchain.ts` は `platform`・`arch`・`id` だけを読む）。

### ツールの探索（コンパイラ）

`src/driver.rs` の `tool` を通るすべての起動（`src/test_runner.rs` と `src/cache.rs` の key を含む）で、次の順に決める（D4）。

| 変数 | `PATH` の名前 | 同梱の名前（`<root>/bin/` の下。Windows は `.exe` を付ける） |
| --- | --- | --- |
| `TSUZURI_CLANG` | `clang` | `tsuzuri-clang` |
| `TSUZURI_WASM_LD` | `wasm-ld` | `wasm-ld` |
| `TSUZURI_LLVM_LINK` | `llvm-link` | `llvm-link` |
| `TSUZURI_DSYMUTIL` | `dsymutil` | `dsymutil` |

1. 環境変数が設定されていれば（空文字列を含む）、その値をそのまま使う。HEAD と同じ。
2. `fs::canonicalize(env::current_exe())` の 2 段上を `<root>` とし、`<root>/manifest.json` がファイルで、かつ同梱の名前のファイルがあれば、その絶対パスを使う。
3. それ以外は `PATH` の名前（`Command` が `PATH` を探す）。HEAD と同じ。

`node`（WASM の言語内テスト）は探索の対象外で、HEAD どおり `PATH` の `node` を使う（D11）。shim の `TSUZURI_LLVM_CLANG`・`TSUZURI_ZIG` と、
その既定値（shim からの相対位置）は変えない。

### CLI

新構文（実装後に有効。未検証）。引数が `toolchain info` とちょうど一致するときだけ、選ばれたツールを標準出力に書いて終了コード 0 で終わる。
`tsuzuri toolchain` 単独や `toolchain info --json` など、それ以外の形は HEAD と同じ解析に進む（`toolchain` という名前のディレクトリの build を壊さない）。

```sh
tsuzuri toolchain info
```

出力は次の行をこの順に書く。`<source>` は `env`・`bundled`・`path` のいずれか。パスは `src/cache.rs` の `executable_path` で解決した正規のパス。

```text
tsuzuri 0.1.0
distribution: /Users/me/.local/share/tsuzuri/tsuzuri-0.1.0-darwin-arm64 (id 1a2b3c4d5e6f)
TSUZURI_CLANG: bundled /Users/me/.local/share/tsuzuri/tsuzuri-0.1.0-darwin-arm64/bin/tsuzuri-clang (<version line>)
TSUZURI_WASM_LD: bundled /Users/me/.local/share/tsuzuri/tsuzuri-0.1.0-darwin-arm64/bin/wasm-ld (<version line>)
TSUZURI_LLVM_LINK: bundled …/bin/llvm-link (<version line>)
TSUZURI_DSYMUTIL: bundled …/bin/dsymutil (<version line>)
node: path /opt/homebrew/bin/node (v24.x.y)
```

- `distribution:` は `<root>` と manifest の `id` の先頭 12 文字。同梱がなければ `distribution: none`。`id` を読めなければ `(id unavailable)`。
- 解決できないツールは `TSUZURI_LLVM_LINK: path llvm-link (not found)` のように、探索した値と `(not found)` を書き、起動しない。
- `<version line>` は `--version` の標準出力（空なら標準エラー）のうち `version` を含む最初の行（大文字小文字を区別しない。なければ最初の空でない行）の trim。
  起動の失敗か終了コードが 0 以外なら `unavailable`。
- `--version` の出力は `tsuzuri 0.1.0` のまま変えない（D5）。

### 導入と削除（文書に書く手順）

macOS・Linux。archive と `.sha256` を同じディレクトリへ取得した後に行う。導入先は例で、どこでもよい。

```sh
v=0.1.0 host=darwin-arm64
shasum -a 256 -c "tsuzuri-$v-$host.tar.gz.sha256"   # Linux は sha256sum -c
mkdir -p "$HOME/.local/share/tsuzuri" "$HOME/.local/bin"
tar -xzf "tsuzuri-$v-$host.tar.gz" -C "$HOME/.local/share/tsuzuri"
ln -sf "$HOME/.local/share/tsuzuri/tsuzuri-$v-$host/bin/tsuzuri" "$HOME/.local/bin/tsuzuri"
tsuzuri toolchain info
```

- `PATH` に入れるのは symlink の `tsuzuri` だけ。`bin/` を `PATH` へ足すと同梱の `clang`・`wasm-ld` がシステムのものを遮蔽する（D2）。
- macOS: ブラウザーで取得したファイルには `com.apple.quarantine` が付き、公証していない（D9）実行ファイルは Gatekeeper が止める。`curl -fLO` で取得するか、
  チェックサムの確認後に `xattr -dr com.apple.quarantine "$HOME/.local/share/tsuzuri/tsuzuri-$v-$host"` を実行する。
- 同じ配布元の `.sha256` は破損を検出するだけで、配布元の改ざんは検出しない（真正性は D10）。
- Linux: 同梱の LLVM は build した runner（Ubuntu 24.04）の glibc 以上を要求する。生成物は musl で静的に link される。
- Windows（未検証。D8）: `Get-FileHash -Algorithm SHA256` の値を `.sha256` と比べ、`Expand-Archive` で `%LOCALAPPDATA%\Tsuzuri\` へ展開し、
  `bin` を利用者の `PATH` へ足す（symlink は権限が要るため。遮蔽の注意を併記）。
- 削除: symlink（Windows は `PATH` の項目）と展開したディレクトリを消す。任意で build cache（`_docs/tools/build-and-cache.md` の「保存先」）を消す。
  複数の版は別ディレクトリに並べ、symlink の張り替えで切り替える。

### 診断

新しい診断コードはない。既存の経路とメッセージを変えない。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E2000 | `toolchain info` に余分な引数がある（`toolchain info --json` など） | HEAD の `parse_arguments` のメッセージ（例: `pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively`） | `<command line>:1:1` |
| E2002 | 探索で選んだツール（同梱を含む）の起動・実行に失敗した | HEAD の `run_tool` の hint（例: `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable`） | HEAD と同じ |

### 資源上限

言語・コンパイラの上限は変えない。archive の大きさは上限にしないが、手順 8 で圧縮前後の大きさを記録する（darwin-arm64 の tree は 802 MiB）。

### 例

| 状況 | 結果 |
| --- | --- |
| 展開した配布物の `bin/tsuzuri`（または symlink）で `toolchain info` | 4 行とも `bundled`（Linux は `TSUZURI_LLVM_LINK`・`TSUZURI_DSYMUTIL` が `path … (not found)` のことがある） |
| 同じく `TSUZURI_CLANG=/usr/bin/clang tsuzuri toolchain info` | `TSUZURI_CLANG: env /usr/bin/clang (…)`、ほかは `bundled` |
| `target/release/tsuzuri toolchain info` | `distribution: none`、4 行とも `path` か `env` |
| manifest を消した配布物 | `distribution: none`、4 行とも `path` |
| `tsuzuri toolchain info --json` | E2000、終了コード 2（HEAD と同じ） |
| 配布物だけ・`PATH` に LLVM なしで `build`・`run`・`test`、`build --target wasm32` | 成功（手順 7 の smoke） |

## 設計

### データ構造

`src/driver.rs`（新規の型と関数）。`tool` の引数と呼び出し元は変えない。

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSource {
    Env,
    Bundled,
    Path,
}

pub fn distribution_root() -> Option<PathBuf> {
    let executable = fs::canonicalize(env::current_exe().ok()?).ok()?;
    let root = executable.parent()?.parent()?;
    root.join("manifest.json").is_file().then(|| root.to_path_buf())
}

pub fn resolve_tool(variable: &str, fallback: &str) -> (ToolSource, OsString) {
    if let Some(value) = env::var_os(variable) {
        return (ToolSource::Env, value);
    }
    let name = if variable == "TSUZURI_CLANG" { "tsuzuri-clang" } else { fallback };
    if let Some(root) = distribution_root() {
        let path = root.join("bin").join(format!("{name}{}", env::consts::EXE_SUFFIX));
        if path.is_file() {
            return (ToolSource::Bundled, path.into_os_string());
        }
    }
    (ToolSource::Path, fallback.into())
}

fn tool(variable: &str, fallback: &str) -> OsString {
    resolve_tool(variable, fallback).1
}
```

manifest（`scripts/toolchain/bundle.mjs` が書く JSON）。

```json
{
  "version": 1,
  "platform": "darwin",
  "arch": "arm64",
  "compiler": "tsuzuri 0.1.0",
  "zig": "0.16.0",
  "mingw": null,
  "llvm": 21,
  "toolVersions": { "clang": "Homebrew clang version 21.1.8", "wasm-ld": "…", "dsymutil": "…", "llvm-link": "…" },
  "id": "<SHA-256 of JSON.stringify(files)>",
  "files": [{ "path": "bin/clang", "size": 0, "sha256": "…" }]
}
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 同梱 | `scripts/toolchain/bundle.mjs`（新規） | `bundle(destination)`・`verify(destination)`・`native`・`licenses`・`entries`・`download`・`digest`・`exists`・`run`・`extractArchive`、定数 `zigVersion`・`platforms`・`mingwVersion`・`mingwChecksums`・`llvmTools` | `vsc/scripts/toolchain.mjs` から移して export する。destination を引数にし、stage と backup は `path.dirname(destination)` に作る。CodeLLDB を除く。shim は `scripts/toolchain/clang.rs` から build。`toolVersions` と D6 のライセンスを足す。直接実行は `node scripts/toolchain/bundle.mjs <destination> [--verify]` |
| 同梱 | `scripts/toolchain/clang.rs` | `main` | `git mv vsc/scripts/clang.rs scripts/toolchain/clang.rs`。内容は変えない |
| VSIX | `vsc/scripts/toolchain.mjs` | `resources`、CodeLLDB の取得 | `resources` だけを残し、`bundle(path.join(extension, 'toolchain'))` を呼ぶ。CodeLLDB は `debuggerVersion`・`debuggerChecksums` とともにここへ残し、`resources/codelldb.vsix` へ置く。`--verify` は共有の `verify` と CodeLLDB の digest |
| VSIX | `vsc/src/workflow.ts` | CodeLLDB の導入 | `path.join(tools.root, 'codelldb.vsix')` を拡張の `resources/codelldb.vsix`（`context.extensionUri` から作る。関数に `context` がなければ引数で渡す）へ |
| VSIX | `vsc/scripts/package.mjs` | secret scan の対象の filter | 除外の正規表現に `vsix` を足す |
| VSIX | `vsc/package.json` | `scripts.test:toolchain` | `node ../scripts/toolchain/smoke.mjs toolchain` |
| archive | `scripts/toolchain/archive.mjs`（新規） | 全体 | 「アルゴリズム」の archive の手順 |
| smoke | `scripts/toolchain/smoke.mjs` | 全体 | `git mv vsc/scripts/smoke.mjs scripts/toolchain/smoke.mjs`。第 1 引数を必須にし、archive も受ける。`--discover`（新規） |
| driver | `src/driver.rs` | `tool`、`ToolSource`（新規）、`distribution_root`（新規）、`resolve_tool`（新規） | 「データ構造」のとおり |
| cache | `src/cache.rs` | cache key の `tools` の loop、`executable_path` | `std::env::var_os(variable).unwrap_or_else(…)` を `crate::driver::resolve_tool(variable, fallback).1` に。`executable_path` を `pub` に |
| CLI | `src/main.rs` | `main`（`--version` の直後）、`HELP`、`toolchain_info`（新規） | `raw` が `["toolchain", "info"]` なら `print!("{}", toolchain_info())` で `ExitCode::SUCCESS`。`HELP` の Usage に `tsuzuri toolchain info`、`Toolchain:` に `TSUZURI_LLVM_LINK`・`TSUZURI_DSYMUTIL` と探索順の 1 行 |
| テスト | `tests/toolchain.rs`（新規） | — | 「テスト計画」 |
| CI | `.github/workflows/vscode.yml` | job `package` | PR の `paths` に `scripts/toolchain/**`。`npm run test:toolchain` の後に archive・smoke・upload の 3 step（D14） |

### 生成 IR とランタイム

変更なし。IR はツールに依存しない（`--emit llvm` はツールを起動しない）。同じ入力と同じツールなら同じ IR・成果物であり、同梱か `PATH` かで
選ばれるツールが変わると成果物も変わりうるが、cache key は解決後のツールのパス・digest・`--version` を HEAD どおり含むので取り違えない。

### アルゴリズム

`toolchain_info`（`src/main.rs`）:

```text
out = "tsuzuri {CARGO_PKG_VERSION}\n"
root = driver::distribution_root()
out += root ? "distribution: {root} (id {manifest.id[..12] or 'unavailable'})\n" : "distribution: none\n"
for (variable, fallback) in [(TSUZURI_CLANG, clang), (TSUZURI_WASM_LD, wasm-ld), (TSUZURI_LLVM_LINK, llvm-link), (TSUZURI_DSYMUTIL, dsymutil)]:
    (source, tool) = driver::resolve_tool(variable, fallback)
    out += line(variable, source, tool)
out += line("node", Path, "node")
line: executable_path(tool) が Ok(p) なら "{name}: {source} {p} ({version_line(p)})"、Err なら "{name}: {source} {tool} (not found)"
```

manifest の `id` は `serde_json::from_str::<serde_json::Value>` で読む（既存の依存）。

`scripts/toolchain/archive.mjs <toolchain> <out>`:

```text
1. await verify(toolchain)                          # 共有の verify。失敗なら終了コード 1
2. version = manifest.compiler から "tsuzuri " を除く。/^\d+\.\d+\.\d+/ でなければ失敗
3. name = `tsuzuri-${version}-${manifest.platform}-${manifest.arch}`
4. staging = mkdtemp(<out>/.archive-)、cp(toolchain, staging/name, { recursive: true })
5. darwin/linux: COPYFILE_DISABLE=1 tar --no-xattrs -czf <out>/<name>.tar.gz -C staging name
   win32:       %SystemRoot%\System32\tar.exe -a -cf <out>\<name>.zip -C staging name
6. <out>/<archive>.sha256 = `${await digest(archive)}  ${basename(archive)}\n`
7. check = mkdtemp(<out>/.check-)、extractArchive(archive, check)
   readdir(check) が [name] と一致し、await verify(check/name) が成功すること
8. finally: staging と check を消す。archive のパス・大きさ・manifest.id を表示
```

`scripts/toolchain/smoke.mjs <toolchain|archive> [--discover]`:

```text
引数が .tar.gz か .zip なら一時ディレクトリへ展開し、その <name> を tools とする（cp しない）。ディレクトリなら HEAD どおり cp する。
環境は HEAD どおり（PATH・TSUZURI_* などを消す）。
--discover なし: HEAD どおり PATH = tools/bin、4 つの TSUZURI_* を明示する。
--discover あり: TSUZURI_* を設定しない。<tmp>/path-only に symlink tsuzuri → tools/bin/tsuzuri を作り（Windows は作らず tools/bin/tsuzuri.exe を直接使う）、
  PATH = <tmp>/path-only、compiler = その symlink。最初に toolchain info を実行し、TSUZURI_CLANG と TSUZURI_WASM_LD の行が
  `bundled ${await realpath(tools)}/bin/…` であることを確かめる。最後に TSUZURI_WASM_LD=<tmp>/not-a-linker で build --target wasm32 が
  0 以外で終わり stderr に E2002 を含むこと（環境変数が同梱に勝つ）を確かめる。
両方: HEAD の native・WASM・fmt の試験をそのまま行う。
```

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N を必ず見る
（GUIDE §3.1）。`npm run toolchain` は Zig・CodeLLDB を取得する（初回だけ network。cache は `target/vsc-downloads/`）。以下の `LLVM_ENV` は
`LLVM_PREFIX="$(brew --prefix llvm@21)" TSUZURI_WASM_LD="$(brew --prefix lld)/bin/wasm-ld"` の略記である。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、VSIX の同梱と smoke を通して manifest を保存する。
- 確認: `npm run toolchain` が `Verified darwin-arm64: <N> files, <id>`、`npm run test:toolchain` が
  `Relocated toolchain passed without system compiler, SDK, or PATH tools.` で終わる。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-work-G14
cd vsc && npm ci
LLVM_PREFIX="$(brew --prefix llvm@21)" TSUZURI_WASM_LD="$(brew --prefix lld)/bin/wasm-ld" npm run toolchain
npm run test:toolchain
cp toolchain/manifest.json /tmp/tz-work-G14/manifest-before.json
```

### 手順 2: 同梱処理を `scripts/toolchain/` へ移す

- 変更: 「段ごとの変更」の同梱・VSIX・smoke の行（`git mv` 2 件、`scripts/toolchain/bundle.mjs`（新規）、`vsc/scripts/toolchain.mjs`、`vsc/src/workflow.ts`、
  `vsc/scripts/package.mjs`、`vsc/package.json`）。smoke の `--discover` はまだ足さない。
- 内容: `bundle.mjs` の `repository` は `path.resolve(<scripts/toolchain>, '..', '..')`、`run` の cwd は `repository` のまま。CodeLLDB を除く以外の処理・順序・
  エラーメッセージを変えない。manifest から `debugger` を除く。`vsc/src/workflow.ts` は `registerWorkflow` の `context` から
  `vscode.Uri.joinPath(context.extensionUri, 'resources', 'codelldb.vsix').fsPath` を作る。
- 確認: 次の `node -e` が `[["codelldb.vsix"],[]]` を出す。`npm test` が成功する。VSIX に `extension/resources/codelldb.vsix` があり、
  `extension/toolchain/codelldb.vsix` がない。`vsc/scripts/toolchain.mjs` に `install_name_tool`・`patchelf` が残らない（`grep -c` が 0）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri/vsc
LLVM_PREFIX="$(brew --prefix llvm@21)" TSUZURI_WASM_LD="$(brew --prefix lld)/bin/wasm-ld" npm run toolchain && npm run test:toolchain
node -e 'const a=require("/tmp/tz-work-G14/manifest-before.json").files.map(f=>f.path),b=require("./toolchain/manifest.json").files.map(f=>f.path);console.log(JSON.stringify([a.filter(p=>!b.includes(p)),b.filter(p=>!a.includes(p))]))'
npm test && npm run vsix
unzip -l dist/tsuzuri-0.1.0-darwin-arm64.vsix | grep codelldb.vsix
grep -c "install_name_tool\|patchelf" scripts/toolchain.mjs
```

### 手順 3: ライセンスと `toolVersions`

- 変更: `scripts/toolchain/bundle.mjs` の `bundle`。
- 内容: (a) `src/runtime/musl/COPYRIGHT` を `licenses/musl-COPYRIGHT.txt` へ写す。(b) `cargo metadata --format-version 1 --locked --filter-platform <rustHost>`
  （`bundle` が既に求める `rustHost`）の `resolve` を `resolve.root` から、`dep_kinds` に `kind: null` を含む `deps` だけたどる。各 package を名前・版の順に
  `licenses/rust-crates.txt` へ `<name> <version> <license>` の行で書き、1 行目は `Rust standard library: MIT OR Apache-2.0 (Apache-2.0 text: Tsuzuri.txt)` とする。
  各 package の `path.dirname(manifest_path)` から `/^(license|copying|notice)/i` のファイルを `licenses/crate-<name>-<version>-<file>` へ写し、一つもなければ
  `Missing license file for crate <name>` を投げる。(c) manifest に `toolVersions`（仕様の「配布物」）を書く。
- 確認: 手順 2 の `npm run toolchain` と `node -e` を再実行し、増えた path が `licenses/musl-COPYRIGHT.txt`・`licenses/rust-crates.txt`・`licenses/crate-*` だけ。
  `rust-crates.txt` に `rustc_apfloat` と `serde_json` があり、`same-file`（Windows 専用）がない。
  `node -e 'console.log(require("./toolchain/manifest.json").toolVersions)'` の `clang` が `clang version 21.` を含む。

### 手順 4: 探索順（driver と cache key）

- 変更: `src/driver.rs` の `ToolSource`（新規）・`distribution_root`（新規）・`resolve_tool`（新規）・`tool`、`src/cache.rs` の key の `tools` loop と `executable_path`。
- 内容: 「データ構造」のとおり。`tool` の呼び出し元（`src/driver.rs`・`src/test_runner.rs`）は変えない。
- 確認: `cargo test --locked` が成功する。`cargo build --release --locked && node tests/cache.mjs target/release/tsuzuri && node tests/e2e.mjs target/release/tsuzuri` が成功する
  （開発用ビルドは `target/manifest.json` がないので HEAD と同じツールを使う）。

### 手順 5: `tsuzuri toolchain info`

- 変更: `src/main.rs` の `main`・`HELP`・`toolchain_info`（新規）。
- 内容: 仕様の「CLI」と「アルゴリズム」のとおり。`--help` と `--version` の処理より後、`parse_arguments` より前に置く。
- 確認: 次が `distribution: none` と `path` の 4 行を出して `rc=0`、`--json` 付きが E2000 で `rc=2`、`--version` が `tsuzuri 0.1.0`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && cargo build --release --locked
cd /tmp/tz-work-G14 && /Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri toolchain info; echo "rc=$?"
/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri toolchain info --json; echo "rc=$?"
```

### 手順 6: Rust テスト

- 変更: `tests/toolchain.rs`（新規）。
- 内容: 「テスト計画」の 6 テスト。
- 確認: `cargo test --locked --test toolchain` が `running 6 tests` で `6 passed`（Windows は 5）。

### 手順 7: smoke の `--discover` と archive 入力

- 変更: `scripts/toolchain/smoke.mjs`。
- 内容: 「アルゴリズム」の smoke。`vsc/toolchain` を作り直してから試す（コンパイラが変わったため）。
- 確認: 次の 2 つがともに最終行 `Relocated toolchain passed without system compiler, SDK, or PATH tools.` を出す。`vsc/toolchain/bin/tsuzuri toolchain info` が 4 行とも `bundled`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri/vsc
LLVM_PREFIX="$(brew --prefix llvm@21)" TSUZURI_WASM_LD="$(brew --prefix lld)/bin/wasm-ld" npm run toolchain
cd .. && node scripts/toolchain/smoke.mjs vsc/toolchain && node scripts/toolchain/smoke.mjs vsc/toolchain --discover
```

### 手順 8: archive

- 変更: `scripts/toolchain/archive.mjs`（新規）。
- 内容: 「アルゴリズム」の archive。
- 確認: `shasum` が `OK`、最上位は `tsuzuri-0.1.0-darwin-arm64` だけ、`._` の entry は 0、archive 内の manifest の `id` が `vsc/toolchain/manifest.json` と一致する。
  圧縮後の大きさを記録する（合否にしない）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node scripts/toolchain/archive.mjs vsc/toolchain target/dist
cd target/dist && shasum -a 256 -c tsuzuri-0.1.0-darwin-arm64.tar.gz.sha256
tar -tzf tsuzuri-0.1.0-darwin-arm64.tar.gz | cut -d/ -f1 | sort -u
tar -tzf tsuzuri-0.1.0-darwin-arm64.tar.gz | grep -c '/\._'
ls -l tsuzuri-0.1.0-darwin-arm64.tar.gz
```

### 手順 9: 配布物だけでの smoke

- 変更: なし。
- 内容: archive を展開し、`TSUZURI_*` なし・`PATH` に symlink だけの環境で native と WASM を試す。
- 確認: `node scripts/toolchain/smoke.mjs target/dist/tsuzuri-0.1.0-darwin-arm64.tar.gz --discover` が最終行を出す。

### 手順 10: CI

- 変更: `.github/workflows/vscode.yml`（D14）。
- 内容: PR の `paths` に `scripts/toolchain/**` を足し、`- run: npm run test:toolchain` の直後に次の 3 step を足す（fence は YAML）。

```text
      - run: node ../scripts/toolchain/archive.mjs toolchain ../target/dist
      - name: Standalone archive smoke
        run: |
          shopt -s nullglob
          set -- ../target/dist/tsuzuri-*-${{ matrix.target }}.tar.gz ../target/dist/tsuzuri-*-${{ matrix.target }}.zip
          node ../scripts/toolchain/smoke.mjs "$1" --discover
      - uses: actions/upload-artifact@v4
        with:
          name: tsuzuri-cli-${{ matrix.target }}
          path: |
            target/dist/tsuzuri-*.tar.gz
            target/dist/tsuzuri-*.zip
            target/dist/tsuzuri-*.sha256
          if-no-files-found: error
```

- 確認: `git diff --stat` で workflow の変更がこの 1 ファイルだけ。実行は人間が push 後に `workflow_dispatch` で行い、6 個の artifact `tsuzuri-cli-<target>` を確かめる
  （実装者は push しない）。

### 手順 11: 文書

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/get-started.md _docs/tools/build-and-cache.md _docs/tools/command-line.md _docs/feature-status.md` が成功する。

### 手順 12: 最終確認

- 確認: 次がすべて成功する。手順 8・9 を最終のコンパイラで再実行する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cd vsc && npm run test:unit && npm run test:toolchain && npm test && npm run vsix && npm run test:installed
```

## テスト計画

### Rust テスト

`tests/toolchain.rs`（新規）。helper `install(root, manifest: bool) -> PathBuf` は `env!("CARGO_BIN_EXE_tsuzuri")` を `root/bin/tsuzuri{EXE_SUFFIX}` へ `fs::copy` し、
`manifest` なら `root/manifest.json` に `{"id":"0123456789abcdef…"}`（64 桁）を書き、空のファイル `root/bin/tsuzuri-clang{EXE_SUFFIX}`・`root/bin/wasm-ld{EXE_SUFFIX}` を作る。
helper `info(compiler, extra_env) -> (i32, String)` は `toolchain info` を、4 つの `TSUZURI_*` を `env_remove` し `PATH` を一時の空ディレクトリにして実行する。
一時ディレクトリは `std::env::temp_dir()` の下に `tsuzuri-toolchain-<pid>-<test 名>` で作って最後に消す。期待するパスは `fs::canonicalize` した root から作る。

| テスト | 検査すること |
| --- | --- |
| `reports_path_tools_without_distribution` | `CARGO_BIN_EXE_tsuzuri` そのもので終了コード 0、1 行目 `tsuzuri {CARGO_PKG_VERSION}`、`distribution: none`、`TSUZURI_CLANG: path clang (not found)`、`node: path node (not found)` |
| `prefers_bundled_tools_over_path` | `PATH` のディレクトリに空の `clang` を置いても `TSUZURI_CLANG: bundled <root>/bin/tsuzuri-clang (unavailable)`。`distribution: <root> (id 0123456789ab)`。`TSUZURI_LLVM_LINK: path llvm-link (not found)` |
| `environment_overrides_bundled_tools` | `TSUZURI_CLANG=<tmp>/fake-clang`（空ファイル）で `TSUZURI_CLANG: env <canonical fake> (unavailable)`、`TSUZURI_WASM_LD` は `bundled` のまま |
| `ignores_bundle_without_manifest` | manifest なしの root で `distribution: none`、`TSUZURI_CLANG: path clang (not found)` |
| `follows_symlinked_compiler`（`#[cfg(unix)]`） | `std::os::unix::fs::symlink` で別ディレクトリに張った `tsuzuri` から実行して `bundled <root>/bin/tsuzuri-clang` |
| `keeps_other_toolchain_arguments_unchanged` | `toolchain info --json` と `toolchain info extra` が終了コード 2 で stderr に `E2000` |

### E2E

- `scripts/toolchain/smoke.mjs`: 明示モード（HEAD と同じ内容。`npm run test:toolchain`）と `--discover`（ディレクトリと archive）。native の `-O0`/`-O3`（`-g`、実行結果 `42`、
  言語内テストの `passed 2, failed 0, ignored 1` と失敗、Task）、WASM の `-O0`/`-O3`（`tz_answer() == 42n`）、`fmt`。期待値は HEAD の smoke と同じ固定値。
- `scripts/toolchain/archive.mjs` の自己検査（展開して `verify`）。
- VS Code: `npm test`・`npm run test:installed`（CodeLLDB の移動の回帰）。

### 既存テストへの影響

なし。`tests/e2e.mjs` の `--version`、`tests/cache.mjs`（`TSUZURI_CLANG` の wrapper は環境変数なので同梱より優先）は変わらない。`vsc/package.json` の `test:toolchain` は
script の場所だけが変わる。

### 性能

`distribution_root` はツールの起動ごとに `canonicalize` と `stat` 2 回を足す。計測しておらず、効果・劣化を主張しない。

## ドキュメント

- `README.md` の「ビルド」: 冒頭に「LLVM を導入せずに使う場合は配布物（`_docs/get-started.md`）」の 1 段落と、探索順の 1 行。
- `_docs/get-started.md` の「必要な環境」: 配布物の導入・確認（`tsuzuri toolchain info`）・削除、macOS の quarantine、Linux の glibc、Windows は未検証、
  Linux の生成物が musl（MIT）を静的に含むこと（再配布時は `licenses/` の表示を添える）。
- `_docs/tools/build-and-cache.md` の「プラットフォーム」と「キーと検証」: 探索順と、同梱か `PATH` かで cache key が変わること。
- `_docs/tools/command-line.md` の「コマンド」と「ツールと環境変数」: `tsuzuri toolchain info`、`TSUZURI_LLVM_LINK`・`TSUZURI_DSYMUTIL`、探索順。
- `vsc/README.md` の「配布対象」、`vsc/Publishing.md` の「5. VSIX の作成と検証」と「6. CI で全プラットフォームを作成する」: 共有の同梱処理、CLI archive と artifact 名。
- `docs/architecture.md`: `grep -n "TSUZURI_CLANG" docs/architecture.md` の箇所のうち、ツールの決め方を述べる文に探索順を足す。
- `_docs/feature-status.md` の G14 の行と `_features/README.md` の状態欄。

## 受け入れ条件

- [ ] macOS arm64 で `node scripts/toolchain/smoke.mjs target/dist/tsuzuri-0.1.0-darwin-arm64.tar.gz --discover` が成功する（`TSUZURI_*` なし、`PATH` に LLVM なし）。
- [ ] archive と `.sha256` を `scripts/toolchain/archive.mjs` が作り、`shasum -a 256 -c` が `OK`、最上位が一つ、`._` の entry がない。
- [ ] 同じ host の VSIX の `toolchain/manifest.json` と CLI archive の `manifest.json` の `id` が一致する（同じ同梱処理）。`vsc/scripts/` に同梱処理が残らない。
- [ ] 探索順が環境変数 → 同梱 → `PATH` で、開発用ビルドの挙動が HEAD と同じ。`cargo test --locked --test toolchain` が 6 件成功する。
- [ ] `tsuzuri toolchain info` が仕様の形式で出力し、`--version` が `tsuzuri 0.1.0` のまま。
- [ ] `licenses/` に musl・Rust crate・LLVM・Zig・Tsuzuri の表示がある。
- [ ] CI の 3 step を足し、Windows の archive を文書で「未検証」と明記した。署名・公証・Release・Node の同梱をしていない（D9〜D11）。
- [ ] 「ドキュメント」の各ファイルを更新した。GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `env::current_exe` は symlink 経由の起動で symlink のパスを返す OS がある。必ず `fs::canonicalize` してから 2 段上を取る（`follows_symlinked_compiler` が検出する）。
- macOS の一時ディレクトリは `/var/…` と `/private/var/…` の 2 通りに見える。テストと smoke の期待値は `fs::canonicalize`・`realpath` した root から作る。
- macOS の `tar` は `._*`（AppleDouble）と拡張属性を入れる。`COPYFILE_DISABLE=1` と `--no-xattrs` を付け、手順 8 の `grep -c '/\._'` で 0 を確かめる。
- Windows の runner の bash では `tar` が Git の GNU tar になり zip を書けない。`%SystemRoot%\System32\tar.exe` を明示する。
- shim を `scripts/toolchain/` から build すると、panic メッセージの source path が変わり `bin/tsuzuri-clang` の hash が変わる。手順 2 は path の集合で比べる。
- Homebrew の bottle はビルド機の macOS を `minos` にする（手元は 27.0）。配布用の archive は CI（`macos-15`）の物だけを使い、開発機で作った archive を配らない。
- `TSUZURI_WASM_LD` に Homebrew の `lld` を渡すと LLVM 21 と異なる版（手元は 23.1.1）が入る。`toolVersions` に記録されるので、release の前に確かめる。
- Linux の同梱物は runner（Ubuntu 24.04）の glibc 以上を要求する。古い distro での失敗は不具合ではなく制約として文書にある。
- 環境変数が空文字列のとき HEAD は空のパスを起動して E2002 になる。同梱へ落とすと意味が変わるので、そのまま（停止条件）。
- `verify` は `entries` で file を全部 hash するので、archive の自己検査は数十秒かかる。省略しない（実行権限・余分な entry の検出はここだけ）。
- 同梱ツールの E2002 の hint は `TSUZURI_CLANG` を案内する。メッセージは既存の期待値があるので変えない。

## 対象外

- 自動更新（`tsuzuri self update`）、OS のパッケージマネージャー（Homebrew tap・winget・apt）への登録。
- 公証・Authenticode（D9）、配布物の署名・来歴証明・Release への自動公開（D10）、Node.js の同梱（D11）、導入スクリプト（D12）。
- `git` の同梱（E10）、事前ビルドの runtime object（PB01）、shim の target 写像と cross-compile（G15）。
- universal binary、32-bit、musl を host とする Linux（Alpine）、Ubuntu 24.04 より古い glibc。
- archive の bit 単位の再現（mtime・owner の正規化）。

## 決定事項

### D1: 配布物の配置

- 決定: archive の tree は D3 の後の `vsc/toolchain/` と同一（`bin/` に `tsuzuri` と同梱ツール、`lib/`・`zig/`・`licenses/`・`manifest.json`）。旧版の `lib/tsuzuri/` 案は採らない。
- 理由: shim は `<root>/bin/clang`・`<root>/zig/zig`、`native` の rpath は `bin/` → `$ORIGIN/../lib` を前提にしており、配置を変えると shim・rpath・`vsc/src/toolchain.ts` の変更が要る。
  同一の tree なら manifest の `id` の一致で共有を機械的に確かめられる。旧案の目的（`PATH` の遮蔽を避ける）は D2 で満たす。
- 状態: 既定案（実装者はこの案に従う）

### D2: `PATH` への登録

- 決定: macOS・Linux は `bin/tsuzuri` への symlink だけを `PATH` 上に置く手順にする。Windows（未検証）は `bin` を利用者の `PATH` へ足し、遮蔽を注記する。
- 理由: `bin/` には `clang`・`wasm-ld` があり、そのまま `PATH` へ足すとシステムのツールを遮蔽する。symlink は D4 の `canonicalize` で同梱を見つけられる。Windows の symlink は権限が要る。
- 状態: 既定案（実装者はこの案に従う）

### D3: 同梱処理の共有と CodeLLDB

- 決定: 取得・再配置・ライセンス・manifest・検証を `scripts/toolchain/bundle.mjs` に移し、VSIX と archive の両方がこれを使う。CodeLLDB は VSIX 専用として
  `vsc/resources/codelldb.vsix` に置き、toolchain の tree と manifest から外す。
- 理由: 二重保守をなくす。CodeLLDB を tree から外すと、CLI archive と VSIX の tree が同一になる。`resources/` は `.vscodeignore` で VSIX に入り、`vsc/.gitignore` で commit されない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 探索順と同梱の検出

- 決定: 環境変数（空文字列を含む）→ `canonicalize(current_exe)` の 2 段上に `manifest.json` があり同梱の名前のファイルがあればそれ → `PATH`。clang の同梱の名前は shim の `tsuzuri-clang`。
  manifest の中身は検出に使わない。`node` は対象外。
- 理由: 環境変数の優先は VS Code 拡張と `tests/cache.mjs` の前提で、HEAD の意味を保つ。manifest の存在は開発用ビルドとの区別に十分で、起動ごとの JSON 解析を避ける。
- 状態: 既定案（実装者はこの案に従う）

### D5: 版の表示

- 決定: `--version` は `tsuzuri <CARGO_PKG_VERSION>` のまま。配布物の識別は `tsuzuri toolchain info` の `distribution:` 行（root と manifest の `id`）で行う。archive の `<version>` は同じ値。
- 理由: `tests/e2e.mjs`・`verify`・manifest の `compiler` が 1 行の形に依存する。commit hash の埋め込みは build script が要り、再現性の説明を増やす。
- 状態: 既定案（実装者はこの案に従う）

### D6: ライセンス

- 決定: HEAD の LLVM・Zig・llvm-mingw・mingw-w64・Tsuzuri（Apache-2.0）に加え、musl の `COPYRIGHT` と Rust crate（通常の依存だけ）のライセンスを `licenses/` に入れる。
  Rust の標準ライブラリは Apache-2.0 を選び、その本文は `Tsuzuri.txt` とする。CodeLLDB（MIT）は VSIX だけに入る。出典が見つからなければ bundle を失敗させる。
- 理由: musl 由来の `math.ll` と crate はコンパイラの binary に入り、MIT は表示を要求する。黙った欠落を防ぐ。
- 状態: 既定案（実装者はこの案に従う）

### D7: 再現性

- 決定: 取得物は HEAD の固定 SHA-256 で検証し、LLVM は major 21 の確認に加えて `toolVersions` に実際の版を記録する。配布物の同一性は manifest の `id`。
  archive の bit 単位の再現は主張しない。配る archive は CI の物だけ。
- 理由: Homebrew・apt の LLVM は patch 版を固定できない。mtime・owner の正規化は tar の実装差が大きく、Phase 1 の効果に見合わない。
- 状態: 既定案（実装者はこの案に従う）

### D8: host の検証段階

- 決定: darwin-arm64 は実装者が実機で手順 8・9 を確かめる。darwin-x64・linux-x64・linux-arm64 は CI の smoke の結果で「CI で検証」と書く。win32-x64・win32-arm64 は
  archive を作るが、G10 が done になるまで文書で「未検証」と明記する。
- 理由: 実機で確かめていない host を検証済みと書かない（AGENTS.md）。CI の実行は push が要り、人間が行う。
- 状態: 既定案（実装者はこの案に従う）

### D9: macOS の公証と Windows の Authenticode

- 決定: Phase 1 は HEAD の ad-hoc 署名だけ。公証（Developer ID 署名と hardened runtime を含む）と Authenticode は Phase 2 とする。
- 理由: Apple Developer Program と code signing 証明書の費用、CI への秘密情報の登録が要る。未公証の間は quarantine の手順を文書にする。
- 状態: 要承認（承認前は公証・Authenticode のコードと CI の secret に着手しない）

### D10: 配布物の署名・来歴証明と Release への公開

- 決定: Phase 1 はチェックサムを必須とし、archive は CI の artifact として作るだけで、Release へは人間が手で載せる。候補は GitHub の artifact attestation
  （`actions/attest-build-provenance`。`id-token: write`・`attestations: write` が要る）、minisign、sigstore と、tag による Release の自動作成（`contents: write`）。
- 理由: 旧版の既定案（チェックサム必須、署名方式は人間が判断）を保つ。権限の拡大と鍵の管理は人間の判断が要る。
- 状態: 要承認（承認前は Phase 2 の署名・公開の step に着手しない）

### D11: Node.js の同梱

- 決定: Phase 1 は同梱しない。WASM の言語内テストは HEAD どおり `PATH` の `node` を使い、`toolchain info` が `node` の行で状態を示す。承認後の候補は旧版の既定案どおり
  任意の別 component（`tsuzuri-node-<version>-<host>`）とする。
- 理由: Node は数十 MiB あり、版の更新とライセンスの保守が増える。native の build・run・test と WASM の build には要らない。
- 状態: 要承認（承認前は Node の取得・同梱に着手しない）

### D12: 導入スクリプト

- 決定: Phase 1 は作らない。文書の手動手順（検証・展開・symlink の 5 行）だけにし、`curl | sh` の形は推奨しない。
- 理由: 署名のない script は新しい信頼の起点になる（D10 が先）。手順が短く、script の利益が小さい。
- 状態: 既定案（実装者はこの案に従う）

### D13: archive の作り方

- 決定: Node から OS の `tar` を `execFileSync` で呼ぶ（darwin・linux は gzip の `.tar.gz`、win32 は `System32\tar.exe -a` の `.zip`）。npm・Rust の依存を足さない。
- 理由: `extractArchive` と同じく OS の標準の道具で足りる。zip は Windows の標準の展開手段で開ける。
- 状態: 既定案（実装者はこの案に従う）

### D14: CI

- 決定: 新しい workflow を作らず、`.github/workflows/vscode.yml` の job `package` に 3 step と `paths` の 1 行を足す。artifact 名は `tsuzuri-cli-<target>`。
- 理由: 同じ job が既に toolchain を作るので、追加は archive と smoke の数分だけで済む。trigger・権限は変えない。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-01）

Phase 1（手順 1–12）を実装した。D9（公証・Authenticode）、D10（署名・来歴証明・Release への公開）、D11（Node.js の同梱）、D12（導入スクリプト）は
実装していない。着手時の HEAD は `1e4c2ba`。

### 実装

- `scripts/toolchain/bundle.mjs`（新規）: `vsc/scripts/toolchain.mjs` の同梱処理を移して export。destination を引数にし、stage・backup は
  `path.dirname(destination)` に作る。CodeLLDB を除き、`licenses/musl-COPYRIGHT.txt`・`licenses/rust-crates.txt`・`licenses/crate-*`（D6）と
  manifest の `toolVersions` を足した。manifest から `debugger` を除いた。
- `scripts/toolchain/clang.rs`・`smoke.mjs`: `git mv`。shim の内容は変えていない。smoke は第 1 引数を必須にし、`.tar.gz`／`.zip` を展開して試す。
  `--discover` は `TSUZURI_*` を設定せず、`PATH` に compiler の symlink だけを置き、`toolchain info` の `bundled` と、変数が同梱に勝つこと（E2002）を確かめる。
- `scripts/toolchain/archive.mjs`（新規）: verify → archive → `.sha256` → 展開して再 verify と manifest の `id` の一致。
- `vsc/scripts/toolchain.mjs`: `resources` と CodeLLDB（`resources/codelldb.vsix`）だけを残し、`bundle(<vsc>/toolchain)` を呼ぶ。
  `vsc/src/workflow.ts` は `context.extensionUri` の `resources/codelldb.vsix` を使う（`ensureDebugger` の未使用の引数を除いた）。
  `vsc/scripts/package.mjs` の secret scan は `.vsix` を除外し、代わりに VSIX の検証で `extension/resources/codelldb.vsix` を
  `debuggerChecksums`（`vsc/scripts/toolchain.mjs` から export）の固定 SHA-256 と照合する（対応 host だけ。PR #3 のレビュー対応）。
  `vsc/scripts/toolchain.mjs` は直接実行されたときだけ処理を走らせる。`vsc/package.json` の `test:toolchain` は共有の smoke を使う。
- `src/driver.rs`: `ToolSource`・`distribution_root`・`resolve_tool`。`src/cache.rs` の key は `resolve_tool` を使い、`executable_path` を `pub` にした。
- `src/main.rs`: `tsuzuri toolchain info`（引数がちょうど 2 つのときだけ）と `HELP`。
- `tests/toolchain.rs`（新規）: 6 件。`.github/workflows/vscode.yml`: `paths` と 3 step。

### 決定事項への追記（チケットから外れた判断）

- `toolchain info` の `--version` は、空の一時ディレクトリを作業ディレクトリにして実行し、終わったら消す。同梱の `tsuzuri-clang --version` は
  `zig cc --version` まで進み、作業ディレクトリに `a.o` を残すため（shim の不具合で、G14 では shim を変えない）。cache key の `--version` は従来どおり。
- `tests/toolchain.rs` の helper `info` は `PATH` を引数で受け取る（`prefers_bundled_tools_over_path` が `PATH` に空の `clang` を置くため）。

### 確認（darwin-arm64、Node 20.17.0）

- `npm run toolchain`（`LLVM_PREFIX=llvm@21`、`TSUZURI_WASM_LD=lld`）: `Verified darwin-arm64: 19585 files, c7ea54c0…`。手順 2・3 の比較で、
  消えた path は `codelldb.vsix` だけ、増えた path は `licenses/musl-COPYRIGHT.txt`・`licenses/rust-crates.txt`・`licenses/crate-*`（15 files）だけ。
  `toolVersions` は clang・dsymutil・llvm-link が 21.1.8、wasm-ld が Homebrew LLD 23.1.1（落とし穴の記載どおり。配る archive は CI の物）。
  `rust-crates.txt` に `rustc_apfloat`・`serde_json` があり、`same-file` はない。
- `npm run test:toolchain`（明示）、`node scripts/toolchain/smoke.mjs vsc/toolchain --discover`、
  `node scripts/toolchain/smoke.mjs target/dist/tsuzuri-0.1.0-darwin-arm64.tar.gz --discover`: いずれも native `-O0`/`-O3`（`-g`・IO・言語内テスト・Task）、
  WASM `-O0`/`-O3`、`fmt` が成功。`vsc/toolchain/bin/tsuzuri toolchain info` は 4 行とも `bundled`。
- archive: 199,032,269 bytes（tree は約 802 MiB）。`shasum -a 256 -c` が OK、最上位は `tsuzuri-0.1.0-darwin-arm64` だけ、`._` の entry は 0。
- `cargo test --locked --test toolchain`: 6 passed。`cargo test --locked` 全体（G12 と合わせた最終状態で 57 suite）、fmt・clippy が成功。
  `node tests/cache.mjs target/release/tsuzuri`・`node tests/e2e.mjs target/release/tsuzuri`: 成功（開発用ビルドは HEAD と同じツールを使う）。
- `vsc`: `npm run test:unit` が 5/5、`npm run vsix`（Node 24。型検査・lint・secret scan・archive 検証）が成功。VSIX に
  `extension/resources/codelldb.vsix` があり、`extension/toolchain/codelldb.vsix` はない。VSIX の `toolchain/manifest.json` と CLI archive の
  manifest は同じ `id`（`c7ea54c0…`）。`npm test` は編集支援の段の後、`vscode.executeCompletionItemProvider` で止まった。HEAD の拡張機能でも
  同じ段で止まるため作業機の状態によると判断し（G12 の「残作業」）、`npm test` と `npm run test:installed` は未完了。CodeLLDB の導入（デバッグの段）は
  このため実機で確かめていない。
- レビュー対応後の `npm run vsix:verify`（Node 24）: 既存の VSIX で成功。`extension/resources/codelldb.vsix` を `zip -d` で抜いた複製では
  `Incomplete VSIX: extension/resources/codelldb.vsix` で失敗した（確認後に元の VSIX へ戻し、`.sha256` の照合が OK）。
- CI（手順 10）は push が要るため未実行。人間が `workflow_dispatch` で 6 個の `tsuzuri-cli-<target>` artifact を確かめる。

### 残作業

- CI の実行と、darwin-x64・linux-x64・linux-arm64 の「CI で検証」の確認。win32 の archive は G10 が done になるまで未検証。
- 負荷の低い環境で `vsc` の `npm test`・`npm run test:installed` を再実行し、`resources/codelldb.vsix` からの CodeLLDB の導入を確かめる。
- D9〜D11 の承認と Phase 2。コミットは作っていない。
