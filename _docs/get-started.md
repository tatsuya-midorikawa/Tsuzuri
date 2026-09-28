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

## 最初のプログラム

独立した作業ディレクトリの `Main.tz` に、次のようなプログラムを置きます。

```tsuzuri run=42
def add :: i64 -> i64 -> i64
fn add left right = left + right

def main :: i64
fn main = add 20 22
```

`def` は引数と返却値の型、`fn` は実装です。`i64` は符号付き 64-bit 整数です。関数呼び出しの引数は空白で区切り、`main` の最後の式がプログラムの結果になります。

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

トップレベル実行コードは root 直下の `Main.tz` にだけ置けます。`main` とトップレベル実行コードを同じ入口として併用しないでください。結果の型は数値、`bool`、`unit`、`string`、`utf8string`、`char`、`utf8char` のいずれかです。`unit` の結果は表示されません。

## 実行ファイルの生成

```sh
./target/release/tsuzuri build target/first-program -o target/first-program-app
./target/first-program-app
```

既定の最適化レベルは `-O3` です。デバッグ用に `-O0`、行や変数のデバッグ情報に `-g` を指定できます。`--cpu native` はビルド機の CPU 命令を利用できる反面、その命令に対応しない別の機械への配布には適しません。

## 最初のテスト

次はテストを含む独立したプログラムです。

```tsuzuri run=42
def add :: i64 -> i64 -> i64
fn add left right = left + right

test "adds two integers" = assert (add 20 22 == 42)

def main :: i64
fn main = add 20 22
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
| WASM だけ画面や出力がない | WASM は計算モジュール。画面や通常の I/O はホスト側に実装する |
