# Tsuzuri 入門

[ドキュメントのトップ](README.md)

このページでは、コンパイラをビルドし、型検査、実行、実行ファイルの生成、テストまでを行います。以下のコマンドはリポジトリのルートで実行します。

## 必要な環境

| 用途 | 必要なもの |
| --- | --- |
| コンパイラのビルド | Rust 1.85 以降と Cargo |
| 型検査・LLVM IR・C ヘッダーの出力 | ビルド済みの `tsuzuri`。LLVM の実行環境は不要 |
| ネイティブコードの生成 | LLVM / Clang 17 以降と OS の開発用ツールチェーン |
| WebAssembly の生成 | Clang と `wasm-ld` (LLD) |
| WASM テスト | Node.js 20 以降 |
| リポジトリの総合検証 | 上記に加えて Python 3.9 以降。個別の検証には追加要件がある |

標準ライブラリはコンパイラに埋め込まれています。利用者のプロジェクトに標準ライブラリのソースや検索パスを配置する必要はありません。

macOS で Homebrew を利用する場合:

```sh
brew install llvm lld
export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"
cargo build --release --locked
./target/release/tsuzuri --version
```

Linux ではディストリビューションの Clang / LLD と Rust を用意し、同じ Cargo コマンドでビルドします。異なるツールを使う場合は `TSUZURI_CLANG` と `TSUZURI_WASM_LD` にそれぞれ実行ファイルのパスを指定します。POSIX の並列ランタイムには pthread が必要です。

Windows 向けには x86_64 MSVC ABI の実装がありますが、現時点では Windows 上の実行検証が完了していません。Windows SDK、Visual Studio Build Tools、LLVM が必要です。詳細な条件は[リポジトリのビルド手順](../README.md#ビルド)を参照してください。

### 配布物を使う

LLVM を導入しない場合は、コンパイラ・Clang・LLD・リンク用の SDK/libc をまとめたホスト別の配布物 `tsuzuri-<version>-<host>.tar.gz`（Windows は `.zip`）と、そのチェックサム `.sha256` を使えます。VS Code 拡張機能の VSIX と同じツールチェーンです。

macOS・Linux では、archive と `.sha256` を同じディレクトリへ取得してから次を実行します。導入先は例です。

```sh
v=0.1.0 host=darwin-arm64
shasum -a 256 -c "tsuzuri-$v-$host.tar.gz.sha256"   # Linux は sha256sum -c
mkdir -p "$HOME/.local/share/tsuzuri" "$HOME/.local/bin"
tar -xzf "tsuzuri-$v-$host.tar.gz" -C "$HOME/.local/share/tsuzuri"
ln -sf "$HOME/.local/share/tsuzuri/tsuzuri-$v-$host/bin/tsuzuri" "$HOME/.local/bin/tsuzuri"
tsuzuri toolchain info
```

- `PATH` に置くのは `tsuzuri` の symlink だけにします。配布物の `bin/` を `PATH` へ足すと、同梱の `clang`・`wasm-ld` がシステムのツールを隠します。
- コンパイラは各ツールを環境変数（`TSUZURI_CLANG` など）→ 配布物の `bin/` → `PATH` の順に探します。`tsuzuri toolchain info` は選ばれたツールと配布物の識別子（manifest の `id`）を表示します。
- `.sha256` は破損を検出するだけで、配布元の改ざんは検出しません。配布物はまだ署名・公証していません。
- macOS: ブラウザーで取得したファイルには `com.apple.quarantine` が付き、公証していない実行ファイルは Gatekeeper が止めます。`curl -fLO` で取得するか、チェックサムの確認後に `xattr -dr com.apple.quarantine "$HOME/.local/share/tsuzuri/tsuzuri-$v-$host"` を実行します。
- Linux: 同梱の LLVM は作成した環境（Ubuntu 24.04）の glibc 以上を要求します。生成した実行ファイルは Zig の musl（MIT）を静的に含むので、再配布するときは配布物の `licenses/` の表示を添えてください。
- Windows（未検証）: `Get-FileHash -Algorithm SHA256` の値を `.sha256` と比べ、`Expand-Archive` で `%LOCALAPPDATA%\Tsuzuri\` へ展開して `bin` を利用者の `PATH` に足します。同梱の `clang` などが `PATH` の他のツールを隠す点に注意してください。
- WASM の言語内テストには別途 Node.js が必要です。配布物に Node.js は含まれません。
- 削除は symlink（Windows は `PATH` の項目）と展開したディレクトリを消します。必要なら[ビルドキャッシュ](tools/build-and-cache.md)も消します。複数の版は別ディレクトリに並べ、symlink を張り替えて切り替えます。

検証状況: darwin-arm64 は実機で、配布物だけの環境の検査（native と WASM の `-O0`／`-O3`）を通しています。darwin-x64・linux-x64・linux-arm64 は CI の同じ検査で確認します。win32-x64・win32-arm64 の配布物は作成しますが未検証です。

## 最初のプログラム

独立した作業ディレクトリの `Main.tz` に、次のようなプログラムを置きます。

```tsuzuri run=42
def add :: i64 -> i64 -> i64 = \left right -> left + right

def main :: i64 = add 20 22
```

`def` の型の後に `= \引数 -> 本体` を書きます。引数なしの関数では本体を直接書きます。`i64` は符号付き 64-bit 整数です。関数呼び出しの引数は空白で区切り、`main` の最後の式がプログラムの結果になります。

ここではそのディレクトリを `target/first-program` とすると、次のように検査・実行できます。

```sh
./target/release/tsuzuri check target/first-program
./target/release/tsuzuri run target/first-program
```

`check` は実行せず、構文・型・所有権などを検査します。`run` はネイティブコードを生成して実行し、標準出力へ `42` と改行を出します。これはコンソール用ホストが返却値を表示する動作です。通常の関数が暗黙に出力するわけではありません。

次の形も独立した `Main.tz` として実行できます。

```tsuzuri run=42
let base = 40
base + 2
```

トップレベル実行コードは root 直下の `Main.tz` にだけ置けます。`main` とトップレベル実行コードを同じ入口として併用しないでください。結果の型は `IO<unit>`、数値、`bool`、`unit`、`string`、`utf8string`、`char`、`utf8char` のいずれかです。`unit` の結果は表示されません。`IO<unit>` はアクションを実行し、追加の結果表示はしません。`IO<i32>` はアクションを実行し、その値を表示せずプロセスの終了コードにします（[終了コード](library-reference/io.md#終了コード)）。

標準入出力は IO 計算式で明示できます。

```tsuzuri run=Hello
IO { do! IO.write_line "Hello" }
```

入力には `let! line = IO.read_line ()` を使います。[IO のガイド](library-reference/io.md)と[対話サンプル](../examples/io/Main.tz)に、EOF・失敗の処理を含む使い方があります。

## プロジェクトの作成

`tsuzuri new` は、空のフォルダーに `Tsuzuri.toml`・`Main.tz`・`.gitignore` を作ります。

```sh
./target/release/tsuzuri new target/my-app --namespace Acme.MyApp
./target/release/tsuzuri run target/my-app
```

`Tsuzuri.toml` にはパッケージの既定の名前空間（`namespace`）を書き、`Main.tz` は最初の行で同じ名前空間を宣言します。`--namespace` を省くと、フォルダー名から名前空間を決めます。VS Code 拡張機能の **Tsuzuri: New Project** も同じファイルを作ります。名前空間の規則は[モジュールと名前空間](language-reference/modules-and-packages.md#名前空間)を参照してください。

## 実行ファイルの生成

```sh
./target/release/tsuzuri build target/first-program -o target/first-program-app
./target/first-program-app
```

既定の最適化レベルは `-O3` です。デバッグ用に `-O0`、行や変数のデバッグ情報に `-g` を指定できます。`--cpu native` はビルド機の CPU 命令を利用できる反面、その命令に対応しない別の機械への配布には適しません。

## 最初のテスト

次はテストを含む独立したプログラムです。

```tsuzuri run=42
def add :: i64 -> i64 -> i64 = \left right -> left + right

test "adds two integers" = assert (add 20 22 == 42)

def main :: i64 = add 20 22
```

```sh
./target/release/tsuzuri test target/first-program
./target/release/tsuzuri test target/first-program --target wasm32
./target/release/tsuzuri fmt --check target/first-program
```

上記の `test` は対象ディレクトリにテスト宣言がある場合に実行します。WASM テストには Node.js が必要です。テストは通常ビルドから除外されますが、その構文と型は通常の検査でも確認されます。`fmt --check` は整形差分を確認するだけで、ファイルを書き換えません。

## サンプルを試す

自分のプロジェクトを作る前に、同梱の例も実行できます。

```sh
./target/release/tsuzuri run examples/hello
./target/release/tsuzuri run examples/functional
./target/release/tsuzuri run examples/currying
./target/release/tsuzuri run examples/polymorphism
```

最初の例は `5050`、次の例は `42` を返します。ブラウザーの例は[Web ホスト](../examples/web/README.md)、ネイティブとの接続は [C ホストの実装](../examples/native/main.c)を参照してください。

## よくあるつまずき

| 状況 | 確認すること |
| --- | --- |
| `clang` / `wasm-ld` を起動できない | PATH または対応する `TSUZURI_*` 環境変数 |
| 関係のないソースのエラーが出る | root 以下の全ソースが検査対象。例を別ディレクトリに分ける |
| 他ファイルの関数が見つからない | `Module.function` のように修飾する。`open` はない |
| 所有値を後から使えない | 値渡しは move になり得る。読み取りだけなら `ref` を使う |
| F# の例が通らない | `fn`、`let mut`、`==`、配列 `[1, 2]` など Tsuzuri の構文に合わせる |
| WASM だけ画面や出力がない | IO は tsuzuri_io ホストと tsuzuri_main の呼び出しが必要。画面はホスト側に実装する |
