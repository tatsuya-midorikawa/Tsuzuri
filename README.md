# Tsuzuri

**AI と人間が、少ない暗黙ルールで堅牢なプログラムを書けることを目指す、関数型の式と所有権モデルを備えた言語。**
拡張子は `.tzr`。Rust 製のフロントエンドで型検査し、LLVM によりネイティブコードと
WebAssembly を生成します。

現在は **0.1.0 — 計算カーネルを実行できる初版** です。コンソール実行、C ABI、
ネイティブのデスクトップ・ホスト、ブラウザーのゲーム例を含みます。
C/C++ を上回る性能や C#/F# 以上の書きやすさは設計目標であり、現時点の達成保証ではありません。
動的配列、クロージャー、ジェネリクス、
パッケージ管理、GUI/OS の標準ライブラリは未実装です。
メモリは GC ではなく、Rust と同様に所有権の移動・借用・スコープ終了時の解放で管理します。

`Main.tzr`:

```text
fn sum(n: i64, total: i64) -> i64 {
    if n <= 0 {
        total
    } else {
        sum(n - 1, total + n)
    }
}

export fn answer() -> i64 {
    sum(100, 0)
}

fn main() -> i64 {
    answer()
}
```

## 設計と実装済みの範囲

| 項目 | 初版の実装 |
|---|---|
| 状態 | `let` は不変。`let mut` と排他的な `&mut T` でローカル値を置換できる。共有可変状態・I/O・外部関数インポートなし |
| 型 | `bool`、`unit`、`i8`～`i128`／`i8u`～`i128u`、`f16`／`f32`／`f64`／`f128`、`d32`／`d64`／`d128`、`byte`／`ubyte`、UTF-8 `string`、不変レコード・配列、静的関数参照 |
| 書きやすさ | ローカル型推論、式としての `if`／ブロック、`|>`、高階関数、前方参照 |
| モジュール | 1 ファイル = 1 モジュール。複数ファイルの名前解決と `Main.tzr` エントリー |
| メモリ | 所有権、move、`&T`／`&mut T` の借用検査。文字列を自動解放。GC・参照カウント・手動解放なし |
| 最適化 | 既定で LLVM `-O3`。直接の自己末尾再帰は `-O0` でもループ化 |
| 安全性 | 整数除算・配列アクセスを検査。LLVM の未定義動作に依存しない数値仕様 |
| ホスト連携 | 64-bit 以下の整数、f32／f64、bool の C ABI と WASM エクスポート。UI／I/O はホストの責務 |
| AI 向け | 明示的な関数シグネチャ、暗黙の数値変換なし、位置付き JSON 診断、決定的な IR |

Web 向けの小さな計算モジュールという方向性は
[fsw のネイティブコンパイラ](https://github.com/tatsuya-midorikawa/fsw/tree/feat/fsw-native-compiler)
を参考にしています。fsw の直接 WASM 出力とは異なり、Tsuzuri は **LLVM を共通基盤** にして
ネイティブ／WASM の両方へ出力します。生成 WASM は JavaScript ランタイムのインポートを必要としません。

## ファイルとモジュール

**モジュール名は拡張子を除いたファイル名で決まり、1 ファイルに 1 モジュールを強制します。**
`module` 宣言、入れ子のモジュール、複数ファイルへの同一モジュールの分割はできません。
同じディレクトリの `.tzr` ファイルを自動で読み込みます。インポート宣言やファイルの列挙は不要です。

`Point.tzr`:

```text
record Point { x: f64, y: f64 }

fn distance(point: Point) -> f64 {
    sqrt(point.x * point.x + point.y * point.y)
}

export fn hypotenuse(x: f64, y: f64) -> f64 {
    distance(Point { x: x, y: y })
}
```

同じディレクトリの `Main.tzr`:

```text
let p = Point { x: 10.0, y: 20.5 }
let d = Point.distance(p)
d
```

通常の `fn` も別ファイルから `モジュール名.関数名` で呼べます。
`export` はモジュール間の可視性ではなく、C／WASM ホストへの公開指定です。
レコード名は一意なら `Point`、明示する場合は `Point.Point` と書けます。
同名のレコードが複数モジュールにある場合、他モジュールからは修飾名で区別します。

アプリケーションは **`Main.tzr`** から開始します。
トップレベルの `let` と最後の結果式、または従来の `fn main` のどちらかを使います。
上の例では最後の `d` を表示します。結果式を省略すると `unit` になり、何も表示しません。
実行例は `./target/release/tsuzuri run examples/point` です。

## ビルド

- Rust 1.85 以降。浮動小数点リテラルの正確な丸めに `rustc_apfloat` を使います。
- LLVM/Clang 17 以降。WASM のリンクには `wasm-ld`（LLD）も必要です。
- 検証には Node.js 20 以降と Python 3.9 以降。
- macOS と Linux を CI 対象にしています。Windows のネイティブ・ツールチェーンは未検証です。

macOS の例（Rust は rustup 管理を推奨）:

```sh
brew install llvm lld
export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"
cargo build --release
```

Homebrew 管理の古い Rust は LLVM の更新で動的ライブラリの不整合が起こる場合があります。
その場合は Rust と LLVM の依存を整合させてください。

Ubuntu 24.04 の例（Rust は導入済みとします）:

```sh
sudo apt-get update
sudo apt-get install clang lld
cargo build --release
```

`clang` と `wasm-ld` はそれぞれ `TSUZURI_CLANG`、`TSUZURI_WASM_LD` で実行ファイルの
パスを指定できます。`check`／`--emit llvm`／`--emit header` には LLVM の実行環境は不要です。
コンパイラはシェルを経由せずツールを起動し、失敗したツールの診断を報告します。

## コンソール

```sh
./target/release/tsuzuri check examples/hello/Main.tzr
./target/release/tsuzuri run examples/hello/Main.tzr
# 5050

./target/release/tsuzuri run examples/functional
# 42

./target/release/tsuzuri build examples/hello/Main.tzr -o target/hello
./target/hello
```

`Main.tzr` のトップレベルの結果、または引数なしの `main` の返却型は
数値型／`bool`／`unit`／`string` です。ネイティブ用ホスト・ラッパーが
結果を表示し、成功時は終了コード 0 を返します。`unit` は何も表示しません。
言語内に出力の副作用を持ち込む仕組みではありません。

## Web／ゲーム

```sh
./target/release/tsuzuri build examples/web/Physics.tzr --target wasm32 -o examples/web/physics.wasm
python3 -m http.server 8000 --bind 127.0.0.1 --directory examples/web
```

`http://127.0.0.1:8000` を開くと、WASM の純粋な物理関数でボールが反射するデモが動きます。
描画、入力、フレーム時刻は JavaScript 側で扱います。文字列を使う WASM には
解放領域を再利用・結合する allocator を同梱し、GC や JavaScript ランタイムは使いません。
FPS 表示は描画とブラウザーのスケジューリングを含み、コンパイラ性能の指標ではありません。

Node.js でも通常の WebAssembly API から呼び出せます:

```sh
./target/release/tsuzuri build examples/functional/Main.tzr --target wasm32 -o target/functional.wasm
node --input-type=module -e '
import { readFileSync } from "node:fs";
const { instance } = await WebAssembly.instantiate(readFileSync("target/functional.wasm"));
console.log(instance.exports.tz_transform(1n, 2n, 3n, 4n)); // 42n
'
```

公開名には **`tz_`** が付きます。64-bit 整数の受け渡しは `BigInt`、`f32`／`f64` は `Number`、
`bool` は i32（0 = false、非 0 = true、返却値は 0/1）です。
WASM は bulk-memory 対応の現在のブラウザー／Node.js を対象にします。

## 所有権と借用

```text
fn length(text: &string) -> i64 { text.length }
fn replace(text: &mut string) -> unit { *text = "updated"; }

fn main() -> string {
    let mut text = "こんにちは";
    let size = length(&text); // 借用後も所有者を使える
    replace(&mut text);      // この呼び出し中は排他的に借用
    let result = text;      // 所有権を移動。以降の text の使用はエラー
    result
}
```

数値・bool・unit・静的関数参照・共有参照は Copy です。
レコードと配列も全要素が Copy なら Copy、それ以外は move します。
`byte` は `i8`、`ubyte` は `i8u` の別名です。旧 `Int`／`Float`／`Bool`／`Unit` は廃止しました。
型注釈や `1i32`／`0.1d128` のような接尾辞で型を選べます。
所有者より長生きする参照、借用中の move、共有借用と排他借用の競合をコンパイル時に拒否します。
Rust の全機能を実装するものではなく、名前付きライフタイム、借用を含むレコード、
一時値からの直接の借用にはまだ対応していません。詳しくは [言語仕様](docs/language.md) を参照してください。

## ネイティブ・ホスト／デスクトップ

同じ `Physics.tzr` を C/C++、ゲームエンジン、GUI ホストへ組み込めます:

```sh
./target/release/tsuzuri build examples/web/Physics.tzr --emit object -o target/examples/physics.o
./target/release/tsuzuri build examples/web/Physics.tzr --emit header -o target/examples/physics.h
clang -O3 examples/native/main.c target/examples/physics.o -I target/examples -lm -o target/examples/native
./target/examples/native
```

Python/Tk のデスクトップ GUI 例もあります。Python はホストにだけ必要です:

```sh
# macOS
clang -dynamiclib target/examples/physics.o -lm -o target/examples/physics.dylib
python3 examples/desktop/app.py target/examples/physics.dylib

# Linux
clang -shared target/examples/physics.o -lm -o target/examples/physics.so
python3 examples/desktop/app.py target/examples/physics.so
```

GUI には Tk とデスクトップ画面が必要です。`--headless` を付けると Tk をロードせず、
同じネイティブ ABI で 600 ステップを計算します。GUI ツールキット自体は言語に含めていません。

## CLI

```text
tsuzuri check source.tzr|directory [--json]
tsuzuri [build] source.tzr|directory [options]
tsuzuri run Main.tzr|directory [-O0|-O1|-O2|-O3] [--json]
```

| オプション | 内容 |
|---|---|
| `-o`, `--output PATH` | 出力先。親ディレクトリは作成される |
| `--target native\|wasm32` | 既定は native |
| `--emit exe\|object\|llvm\|header\|wasm` | 既定は native なら exe、wasm32 なら wasm |
| `-O0` ～ `-O3` | 既定は `-O3`。fast-math は使わない |
| `--json` | 標準エラーに機械可読の診断を出力 |
| `--` | 以降をパスとして解釈 |
| `--help`, `--version` | ヘルプ／バージョン |

入力はファイルまたはディレクトリを一つ指定します。ディレクトリ指定はその直下の `Main.tzr` を選びます。
どちらも同じディレクトリ直下の全 `.tzr` を名前順に読み込み、未参照のモジュールも検査します。
サブディレクトリは探索しません。ファイル名は大文字小文字を区別する ASCII 識別子で、
`_` 単独や予約語は使えません。
`run`／`--emit exe` の入力は `Main.tzr` またはそのディレクトリに限ります。
`check`／ライブラリ出力では別の `.tzr` を指定でき、`Main.tzr` は不要です。

出力先省略時は選択した入力ファイルの拡張子を変更します。ディレクトリ指定なら `Main.ll` などになります。
`--emit llvm` はライブラリ用 IR で、コンソールのエントリー・ラッパーは付けません。
WASM は少なくとも一つの `export fn` が必要です。
ホスト向けの公開名 `tz_name` は維持するため、`export fn` の名前はプロジェクト全体で一意にします。
`--emit object --target wasm32` はリンク前の WASM オブジェクトも生成できます。
コンパイル／リンク失敗では既存出力を変更せず、成功した成果物だけを同じファイルシステム上で置換します。
読み込んだいずれのソース自身やその別名、シンボリックリンクへの出力も拒否します。

## 検証と性能測定

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/examples.mjs target/release/tsuzuri
node benchmarks/run.mjs target/release/tsuzuri
```

境界値・NaN・短絡評価・高階関数・レコード・配列・100 万回の末尾再帰を、
実際のネイティブコードと WASM の `-O0`／`-O3` で照合します。
失敗時の出力保護、JSON 診断、再現性、決定的なソース変異、LLVM が生成する 128-bit 演算補助も検証します。
追加のプリミティブ、decimal の参照演算との照合、move／借用の失敗例、
文字列の二重解放・解放漏れと、WASM ヒープ使用量の上限も検証します。
`TSUZURI_BROWSER` に Chrome の実行ファイルを設定すると実ブラウザーの起動確認も追加できます。

[言語仕様と ABI](docs/language.md) · [コンパイラ構成と開発指針](docs/architecture.md) ·
[ベンチマークの条件と読み方](docs/benchmarks.md)