# Tsuzuri

**AI と人間が、少ない暗黙ルールで堅牢かつ高パフォーマンスなプログラムを書けることを目指す、関数型の式と所有権モデルを備えたプログラミング言語。**

コードは **`.tz`**、型クラス宣言は **`.tt`**、コンピュテーション式のビルダー実装は **`.tc`** に記述します。Rust 製のフロントエンドで厳密な型検査を行い、LLVM をバックエンドとして高効率なネイティブコードおよび WebAssembly を生成します。

- **[公式日本語ドキュメント（言語リファレンス）](_tsuzuri/language-reference/index.md)**: [Tsuzuri の特徴](_tsuzuri/language-reference/languages/how-about-tsuzuri.md) · [なぜ Tsuzuri なのか](_tsuzuri/language-reference/languages/why-tsuzuri.md) · [所有権とムーブ](_tsuzuri/language-reference/ownership-and-memory/ownership.md) · [組み込み型 / 組み込みモジュール](_tsuzuri/language-reference/index.md#組み込み型--組み込みモジュール) · [コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md)
- **[VS Code 拡張機能](vsc/README.md)**: コンパイラ・LLVM・SDK を統合した VSIX パッケージを提供。シンタックスハイライト、コード補完、定義ジャンプ、リファクタリング、ビルド・実行、テスト、ソースデバッグに対応しています（対応アーキテクチャ: x64 / ARM64。詳細は [vsc/Development.md](vsc/Development.md) を参照）。

---

## 主な特徴

- **所有権と借用による安全なメモリ管理**: ガベージコレクション（GC）に頼らず、Rust と同様の所有権移動と借用検査（`ref` / `ref mut`）によってコンパイル時にメモリ安全性を保証します。参照カウントは標準ライブラリの `Rc` / `Arc` を明示的に使ったときだけです。
- **直感的で表現力豊かな関数型構文**: カリー化、強力な型推論、代数的データ型（`union`）、網羅性を検証するパターンマッチ、パイプライン演算子（`|>`）、関数合成（`>>` / `<<`）をサポートしています。
- **ゼロコストの型クラス**: 型クラスはコンパイル時に静的に単相化（モノモーフィゼーション）され、動的ディスパッチやランタイム辞書のオーバーヘッドを生じさせません。異なる型の値を一つのコレクションへ入れるときだけ、`dyn` 型で実行時のディスパッチを明示できます。
- **拡張可能なコンピュテーション式**: `.tc` ファイルでビルダーを定義することで、`Result` や `Maybe`、非同期処理などのモナディックな制御フローを言語本来の構文のように扱えます。
- **安全で軽量な並列処理**: 遅延評価・一回実行のタスク（`task { ... }`）と常駐スレッドプールにより、データ競合のない安全な並列計算を提供します。
- **明快な名前空間とモジュール構造**: 1 ファイル = 1 モジュールの原則を維持しつつ、`namespace` と `using` による階層管理、`::` によるパス区切りをサポートしています。
- **LLVM による高い実行性能**: 既定で LLVM `-O3` による最適化を適用し、自動ベクトル化や `--cpu native` によるターゲット最適化に対応しています。
- **マルチターゲット（Native & WASM）**: 単一のコードベースからネイティブ実行ファイル、C ABI 連携用の共有ライブラリ / オブジェクト、およびブラウザや Node.js で動作する WebAssembly を出力可能です。`--target` に `aarch64-unknown-linux-musl` のような名前を渡すと、macOS・Linux・Windows・FreeBSD・Android・iOS・OS を持たない環境の 64-bit の target（22 個）向けにクロスコンパイルできます（同梱ツールチェーンか `TSUZURI_SYSROOT` でリンク）。CI（`.github/workflows/targets.yml` と `windows.yml`）が実行まで確かめる target は一部で、範囲は [対応プラットフォーム](_tsuzuri/language-reference/compiler/platforms.md) にあります。
- **充実した開発支援ツール**: 公式言語サーバー（LSP）、自動フォーマッター、単体テストランナー、プロジェクト生成ツール（`tsuzuri new`）、Markdown ドキュメント生成ツール、C ヘッダーからの extern 生成ツール（`tsuzuri bindgen`）を標準で同梱しています。

---

## クイックスタート

### プロジェクトの作成 (`tsuzuri new`)

`tsuzuri new` コマンドで、推奨構成のプロジェクト（`Tsuzuri.toml`、`Main.tz`、`.gitignore`）を新規作成できます。

```sh
tsuzuri new my-project --namespace MyCompany::MyProject
cd my-project
```

### コード例 (`Main.tz`)

アプリケーションは `Main.tz` から開始します。`def main` は `unit -> i32` または `Array<string> -> i32` のシグネチャを持ち、プロセスの終了コードを返します。

```text
def rec sum :: i64 -> i64 -> i64 = \n total ->
    if n <= 0 {
        total
    } else {
        sum (n - 1) (total + n)
    }

export def answer :: i64 = sum 100 0

def main :: unit -> i32 = \() ->
    do! IO.write_line (answer ())
    0
```

### ビルドと実行

```sh
# 型検査を実行
./target/release/tsuzuri check Main.tz

# ネイティブ環境で直接実行
./target/release/tsuzuri run Main.tz
# 出力: 5050

# ネイティブ実行ファイルとしてビルド
./target/release/tsuzuri build Main.tz -o target/hello
./target/hello

# WebAssembly (WASM) としてビルド
./target/release/tsuzuri build Main.tz --target wasm32 -o target/hello.wasm
```

---

## 設計方針と現在の対応状況

現在は **0.1.0 — 計算カーネルを実行できる初版** です。コンソール実行、C ABI、ネイティブのデスクトップ・ホスト、ブラウザでのゲーム実行例を含みます。
C/C++ を上回る性能や C#/F# 以上の書きやすさは設計目標であり、現時点での完全な達成を保証するものではありません。

**Tsuzuri は性能を最優先の設計要件の一つとしており、CPU 命令・SIMD・並列 CPU・GPU のうち、プログラムの意味を保ちつつ実処理が最も速くなる経路をコンパイラ内部で選択することを目指しています。**
この方針はコンパイラ本体だけでなく、組み込み関数や標準ライブラリの設計にも一貫して適用されます。

現状は LLVM の CPU 最適化・自動ベクトル化と `--cpu native` に対応し、`Task.parallel` による明示的な CPU 並列処理も利用可能です。また、実験的な機能として厳密な整数演算に基づく WGSL 生成、名前で選ぶ緩い `f32`・`f16` の WGSL 生成（`Gpu.map_relaxed`・`--emit wgsl-relaxed`）、`Gpu.request Gpu.WebGpu` による WebGPU 上の実行（native は wgpu-native を実行時に読み込み、WebAssembly は `--wasm-feature webgpu`）、WebGPU ホスト試作、SPIR-V の生成（`--emit spirv`。厳密な `i32`・`i32u`・`i64`・`i64u`・`f32`）、`Gpu.request Gpu.Vulkan` による Vulkan 上の実行（native だけ。厳密な `f32` は、デバイスが float controls を報告して適合プローブを通るときだけ）、呼び出しごとに測ったコストで CPU 参照と Vulkan を選ぶ `Gpu.Auto` に対応しています（WebGPU での厳密な浮動小数点と 64 bit 整数、GPU 常駐バッファ、自動オフロードは未実装です。確かめたのは Apple M1 Max だけです）。

### 設計と実装済みの範囲

| 項目 | 初版の実装状況 |
| --- | --- |
| 状態管理 | 変数は既定で不変（`let`）。ローカル変数の再代入・置換は `let mut` と排他借用 `ref mut T` で行います。標準入出力と OS API は `IO`、外部連携は `extern` で分離します。タスクの間で状態を共有して書き換えるときは、共有借用から更新できる `Atomic` と `Mutex` に入れます。 |
| 型システム | 基本数値型（`bool`, `unit`, `i8`〜`i128`, `i8u`〜`i128u`, `f16`〜`f128`, `d32`〜`d128`, `byte`/`ubyte`/`sbyte`）、任意精度整数 `bigint`、ECMA-262 準拠の UTF-16 `string`、UTF-8 バイト列 `utf8string`、文字型 `char`/`utf8char`、タプル、不変レコード、共用体（`union`）、配列、連結リスト、環境を捕捉する関数値に対応。 |
| 構文と表現 | `def ... = ラムダ式`、カリー化と部分適用、`\引数 -> 式`、`if…then…else`、`match` とガード式、`for…in`、`for…to`／`downto`、`while…do`、`break`／`continue`、レコード更新、インデント構文、パイプライン `\|>`、関数合成 `>>`／`<<`、組み込み関数 `not`／`ignore`、累乗演算子 `**`、ビット演算（`&&&`／`\|\|\|`／`^^^`／`~~~`／`<<<`／`>>>`）、数値接尾辞、高階関数、明示的再帰（`rec`／`and`）、関数の中の `let rec`。 |
| 多相性 | `'a` によるパラメトリック多相、ジェネリックなレコード・union、透過的な型エイリアス（`type`）、型クラスおよび具体型インスタンスによるアドホック多相。制約推論と静的単相化。ランク1高階型（HKT）。`dyn C` と `Dyn.of` による vtable を使った動的ディスパッチ。 |
| コンピュテーション式 | `.tc` によるユーザー定義ビルダー（`@alias` による小文字の別名を含む）。明示的ブロックと型推論による暗黙本体。束縛・短絡・分岐・反復を標準の関数呼び出しへ展開。 |
| タスクと並列処理 | `task { ... }`、`let!`／`return`／`return!`／`do!`。所有値を持つ一回実行の計算を組み合わせ、`Task.parallel` でスレッド数を制限して安全に並列実行。 |
| モジュール構成 | 1 ファイル = 1 モジュール。同一ディレクトリ内の自動解決と `Main.tz` によるエントリーポイント。ローカルパッケージ、commit 固定の git 依存、自前の index による registry 依存と最小版選択（`Tsuzuri.toml`、`tsuzuri fetch`、`tsuzuri publish`、`Tsuzuri.lock`）。言語版（`Tsuzuri.toml` の `edition`）、非推奨の印（`@deprecated`、警告 `W1005`）、公開 API の差分（`tsuzuri api-diff`）と互換性の方針（[docs/stability.md](docs/stability.md)）。 |
| メモリモデル | 所有権の移動（move）と借用検査（`ref T`／`ref mut T`、Rust 互換の `&T`／`&mut T` も可）。最後の使用と非重複の match 束縛でのコピー省略。明示的なヒープ確保（`new`）とスタック配置の区別。文字列・配列・リスト・環境の自動解放、`instance Drop` による RAII（GC は不使用。共有は明示的な `Rc` / `Arc`）。 |
| 最適化 | 既定で LLVM `-O3`、自動 SIMD 化、基本数値変換の直接 lowering。所有権・借用から証明した内部参照属性、scoped alias、限定した scalar TBAA・値・memory effects の情報を付与（public wrapper・slice ABI は維持）。`--cpu native` によるビルド機向け最適化。直接の自己末尾再帰は `-O0` でもループ化。 |
| 安全性 | ゼロ除算や配列・リスト境界アクセスの実行時検査。LLVM の未定義動作に依存しない数値仕様。`@checked` による整数オーバーフローは `try` で `Result` に変換可能。 |
| ホスト連携 | スカラー・バッファ・レコードの C ABI 連携および WebAssembly（WASM）のエクスポート／インポート。`extern` のリンク名指定・不透明ハンドル・静的コールバック、ネイティブのホストリンク。標準入出力と OS API は `IO`、協調的な待ちは `Async`、UI やネットワークのイベントはホスト側に委譲。 |
| 開発・AI 支援 | 明示的な関数シグネチャ、暗黙の型変換の排除、位置情報付き JSON 診断、決定的な IR 出力。公式 LSP、フォーマッター、テストランナー、REPL（`tsuzuri repl`）、スクリプト実行（`tsuzuri script`、shebang 行）。 |

---

## 言語機能

### 所有権と借用

Tsuzuri はガベージコレクション（GC）に依存せず、Rust と同様の所有権システムと借用検査によって安全にメモリを管理します。複数の所有者で共有する値は `Rc` / `Arc` で明示的に共有します。

```text
def length :: ref string -> i64 = \text -> text.length
def replace :: ref mut string -> unit = \text -> { deref text = "updated"; }

def main :: string = {
    let mut text = "こんにちは";
    let size = length ref text; // 借用中も所有者は保持される
    replace ref mut text;       // 呼び出し中は排他的に借用
    let result = text;          // 所有権が移動（move）。以降の text の参照はコンパイルエラー
    result
}
```

- **借用と参照外し**: 共有借用は `ref x`、排他借用は `ref mut x`、参照先へのアクセスは `deref r` と記述します。Rust との互換性のため、`&x`／`&mut x`／`*r` や `&T`／`&mut T` という表記も同等に解釈されます。参照に対して再度 `ref` や `ref mut` を適用した場合は一段だけ再借用するため、所有値か参照かによって書き分ける必要はありません。
- **Copy と Move**: 数値、真偽値（`bool`）、`unit`、共有参照、関数値は自動的に Copy となります（関数値のコピーには捕捉環境の複製が伴う場合があります）。レコードや配列も、全要素が Copy であれば Copy、非 Copy の要素を含む場合は所有権が移動（move）します。
- **スタックとヒープの記憶域**: `new` を使って生成した値はヒープ、`new` なしで生成して束縛した値はスタック（関数のスタックフレーム）に配置されます。スタック上の値を戻り値やクロージャの捕捉などで関数外へ移動させる際は、その時点で自動的に要素がヒープへ昇格されるため、安全性や意味論が変わることはありません。関数外へ返すことが確定している値は、最初から `new` で生成することでこのコピーを省けます。
  文字列リテラルの連結と長さだけを使う連結の確保は省き、外へ逃げない小さな連結・局所の捕捉環境は上限付きのスタック領域を使います。ただし native Windows では、束縛の初期化に使うこの追加領域を安全性のためヒープへ確保します。
  ヒープの所有文字列は連結で左辺のバッファを再利用します。詳細と fallback は [スタックとヒープ](_tsuzuri/language-reference/ownership-and-memory/stack-and-heap.md#確保の省略とバッファ再利用) を参照してください。
- **フロントエンドの確保削減**: 不要な本体・Local の複製と子 Vec 確保を除き、型述語には private TypeId arena を使います。公開 AST・型・診断順・構文と資源上限は維持します。改善率は [性能測定](docs/benchmarks.md) の同条件の記録で確認してください。
- **ランタイムの再利用**: 通常の native／wasm32 ビルドでは numeric／math と native C runtime の object を再利用します。同梱 object → 内容とツールが一致する runtime cache → 埋め込みソースからのコンパイルの順で選び、破損や未対応の組は source fallback になります。`--no-cache` は同梱 object と cache の両方を使わず、自己完結した LLVM／object 出力や debug／trap 計装の契約は維持します。
- **数値表示・解析**: f32／f64 の最短表示は整数 Schubfach、通常の十進解析は Eisel–Lemire（符号を除き最大 25 ASCII 文字・19 有効桁）と正確な fallback を使い、最短往復・ties-to-even・NaN・符号付きゼロ・文法を維持します。詳細な検証と比較条件は [性能測定](docs/benchmarks.md) を参照してください。
- **UTF 処理**: UTF 変換・検証・比較は scalar 参照と SIMD 経路を持ち、所有権・文字列表現・符号なしコード単位の比較順序・置換・trap の意味を維持します。
  - native の CPU 経路は能力を検査して fallback し（Windows の CPU runtime は portable baseline）、WASM は `--wasm-feature simd128` の明示指定時だけ SIMD を要求します。UTF-8 復号は 16 bytes 未満なら inline scalar、native の 16–31 bytes は portable LLVM、32 bytes 以上は CPU 経路です。順序比較は UTF-16 の 16 単位未満／UTF-8 の 32 bytes 未満で従来の 64-bit/scalar 経路を使います。
  - UTF-16 符号化は AArch64 など non-x86 では portable LLVM、native x86 の 32 単位以上では CPU による検証・計数と LLVM writer を組み合わせます。portable 符号化は 16 単位未満や初窓にサロゲートがある入力では元の scalar 変換を使い、長い一括経路は別の内部関数へ分離しています。計数は 1／2／3-byte 幅の BMP が混在する窓も扱いますが、writer は均一幅の prefix だけを一括処理します。
  - SIMD 経路の UTF-16 の検証・修復はサロゲートを含まない prefix の後、残りを一度だけ scalar へ渡します。変換・計数・UTF-8 検証も一括経路が使えない窓からは scalar のまま進み、失敗した probe を繰り返しません。新しい import は不要です。詳細は [String の UTF 処理の実装](_tsuzuri/language-reference/built-in-types-and-modules/string.md#utf-処理の実装) を参照してください。

```text
let local = [1, 2, 3]            // 要素はスタック（関数のフレーム）に配置
let text = "hello"               // 静的領域を参照（確保は発生しない）
let heap = new [1, 2, 3]         // 最初からヒープに確保
let sized = new [i64](4, i -> i) // 実行時に長さを決定してヒープに確保
```

- **ライフタイムとリソース解放 (RAII)**: 所有者より長生きする参照や、借用中の移動、共有借用と排他借用の競合はコンパイル時に拒否されます。レコードには region を付けた共有・排他の借用フィールド（`ref {r} T`／`ref mut {r} T`）を置けます。レコードや共用体に `instance Drop<T> { fn drop value = ... }` を実装すると、値がスコープを抜ける際や再代入時に自動的に `drop` が一度だけ呼び出されます。ファイルやホストハンドルの確実なクローズに活用できます。`use` 束縛は、対象の値が `Drop` を実装していることを静的に検査します。
- **一時値の借用**: 関数呼び出しの中間結果など、呼び出し後に借用が残らない一時値は、安全に共有借用として渡せます（例: `Array.sum (Array.map f (ref xs))`）。

### 基本データ型とコレクション

#### 数値型とリテラル

- **整数型**: `i8`〜`i128`、符号なしの `i8u`〜`i128u`。別名として `byte`／`ubyte`（`i8u`）、`sbyte`（`i8`）が利用可能です。
- **浮動小数点型**: `f16`、`f32`、`f64`、`f128`。十進浮動小数点型として `d32`、`d64`、`d128` もサポートしています。`f16` の演算と変換は、AArch64 では LLVM の `half` 命令、x86-64 の `--cpu native` では F16C と AVX512-FP16 の命令、それ以外（WASM を含む）では同梱のソフトウェア実装で計算し、結果のビット（NaN を含む）はどれも同じです。詳細は [`f16` の計算の経路](_tsuzuri/language-reference/types-and-type-inference/basic-types.md#f16-の計算の経路) を参照してください。
- **任意精度整数 (`bigint`)**: `123I` のように `I` 接尾辞で記述し、加減乗除（`+`, `-`, `*`, `/`, `%`）、累乗（`**`）、比較演算、文字列変換に対応します。詳細は [BigInt](_tsuzuri/language-reference/built-in-types-and-modules/bigint.md) を参照してください。
- **リテラルの接尾辞**: `1i32` や `0.1d128` などの標準的な接尾辞に加え、F# 互換の短い接尾辞（`1y`, `1uy`, `1s`, `1us`, `1u`, `1l`, `1ul`, `1L`, `1UL`, `1.5hf`, `1.5f`, `1.5F`, `0.1hm`, `0.1m`, `0.1M`）が利用できます。バイトリテラルは `'a'B`（`byte`）、`"text"B`（`[byte]`）です。接尾辞のない整数は文脈から推論され、未確定時は `i32`、小数は `f64` に決定されます。

#### 文字列と文字

- **既定の `string` (UTF-16)**: [ECMA-262 の String 値モデル](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html#sec-ecmascript-language-types-string-type) に従う UTF-16 コード単位列です。文字型は UTF-16 コード単位を表す `char`（`'A'`）です。
- **バイト列 `utf8string` (UTF-8)**: 従来の UTF-8 文字列は `utf8string` 型および `u8"..."` リテラルで扱います。文字型は Unicode スカラー値を表す `utf8char`（`u8'😀'`）です。
- **文字列補間**: `$"x = {x}, y = {y:.2}"`（UTF-8 は `u8$"..."`）と記述し、波括弧自体は `{{`／`}}` でエスケープします。書式指定は `[[fill]align][+][width][.precision][type]` に対応し、ネイティブと WASM で完全に一致する丸め処理を行います。
- **正規表現 (`Regex`)**: `Regex.compile (ref "\\d+")` でパターンをコンパイルし、`is_match`・`find`・`captures`・`find_all`・`replace_all`・`split`（UTF-8 は `_utf8` 版）で使います。後戻りしない Pike VM なので、1 回の探索は入力の長さに線形です。`\w`・`\p{Lu}`・大文字小文字を区別しない照合は Unicode 17.0.0 の表（`Unicode` モジュール）に従います。詳細は [Regex](_tsuzuri/language-reference/built-in-types-and-modules/regex.md) を参照してください。
- **Unicode のテキスト処理 (`Unicode`)**: 一般カテゴリー、正規化（`Unicode.normalize Unicode.Nfc (ref text)` など）、書記素クラスターと単語の境界（UAX #29）、完全な大文字小文字の変換（`to_upper`・`to_lower`・`to_title`・`case_fold`）を提供します。詳細は [Unicode](_tsuzuri/language-reference/built-in-types-and-modules/unicode.md) を参照してください。
- 詳細は [文字列の仕様](docs/language.md#string-と-utf8string) を参照してください。

#### 配列・リスト・コレクション

- **配列 (`[T]`)**: 固定長の連続メモリ領域です。`[1, 2, 3]` のように記述し、生成後の長さや要素は不変です。`ref values[start..end]` で終端を含まない共有スライス（型: `ref [T]`）を作成でき、コピーを発生させずに部分列を読み取れます。動的な初期化には `new [i32](count, i -> i as i32)` を使用します。
- **連結リスト (`[|T|]`)**: 不変のリストです。リテラルは `[|1, 2, 3|]`、空リストは `[||]` です。要素は連続した領域（chunk）に並び、一度に作ったリスト（`new`・リテラル・複製）は 1 つの chunk、`List.cons` で伸ばしたリストは chunk の連なりです。要素への添字アクセスは、一度に作ったリストなら $O(1)$、`List.cons` で伸ばしたリストなら先行する chunk の数に比例します。要素数が定数の小さなリストを 1 つの関数の中で作って使って捨てる処理は、以前の node の連結（LLVM が確保を全部消して約 1 ns）より遅くなることがあります（`List.map`・`List.reverse`・`List.copy`・`List.cons` の連鎖・文字列のリストで 25〜80 ns）。要素数が実行時に決まるリストは速くなります。
- **ベクター (`Vec<T>`)**: 伸縮可能な所有バッファです。
- **順序付きマップ・セット (`Map<K, V>` / `Set<K>`)**: 平衡二分探索木による不変コレクションです。キーの順序を保持し、検索は $O(\log n)$、挿入および削除は $O(n)$ で動作します。
- **ハッシュマップ・ハッシュセット (`HashMap<K, V>` / `HashSet<K>`)**: 平均 $O(1)$ で検索・挿入・削除が可能なコレクションです。挿入順序が保持されます。詳細は [HashMap](_tsuzuri/language-reference/built-in-types-and-modules/hashmap.md) / [HashSet](_tsuzuri/language-reference/built-in-types-and-modules/hashset.md) を参照してください。
- **JSON (`Json`)**: RFC 8259 に厳密な `Json.parse`、決定的な `Json.to_utf8string`、字句を保つ数値 `Json.Numeral`、組み込み型クラス `Encode` / `Decode` と `Json.serialize` / `Json.deserialize` を提供します。入力 64 MiB・入れ子 128 段の上限を超える入力も `Result` のエラーで返します。`@json "名前"` による名前の変更、字下げした出力、木を作らないプル型の解析器と逐次の出力器、同じ値の CBOR（[Cbor](_tsuzuri/language-reference/built-in-types-and-modules/cbor.md)）も提供します。詳細は [Json](_tsuzuri/language-reference/built-in-types-and-modules/json.md) を参照してください。
- **シーケンス (`Seq<T>`)**: 一度だけ消費可能な遅延反復ストリームです。`Seq.unfold`、`Seq.map`、`Seq.filter`、`Seq.to_array` などを提供します。
- **共有ポインタ (`Rc<T>` / `Arc<T>` / `Rc.Weak<T>` / `Arc.Weak<T>`)**: 参照カウントで値を共有します。所有者は `Rc.share` で明示的に増やし、`Arc` は atomic な計数で、`Send` かつ `Sync` な値を複数のタスクで共有できます。共有した値を書き換えるときは `Atomic` か `Mutex` を入れます（`Arc` と `Mutex` では循環を作れるので、一方は `Arc.Weak` で持ちます）。詳細は [Rc と Arc](_tsuzuri/language-reference/built-in-types-and-modules/rc.md)、[Atomic](_tsuzuri/language-reference/built-in-types-and-modules/atomic.md)、[Mutex](_tsuzuri/language-reference/built-in-types-and-modules/mutex.md) を参照してください。
- **Arena (`Arena<T>` / `Arena.Handle<T>`)**: 値をまとめて所有し、Copy の世代付きハンドルで指すコンテナです。グラフや循環する構造を GC や参照カウントなしで表し、削除済み・別の arena のハンドルを実行時に検出します。詳細は [Arena](_tsuzuri/language-reference/built-in-types-and-modules/arena.md) を参照してください。
- **行列と N 次元配列 (`Matrix<T>`・`MatrixView<T>`・`Tensor<T>`)**: `Matrix` は行優先の 1 本の連続バッファに持つ行列です。構築・要素の参照と置き換え・行の借用・転置・要素ごとの変換と集計・加算・行列積を提供します。行列積の各出力要素は `+0` から `k` の昇順に、積と和を別々に丸めて足すので、native と WASM、`-O0` と `-O3` で同じビットを返します。実装は行ごとに更新する形で、ベクトル化できます。FMA を使う `mul_fma` と、行を形だけで決まるチャンクに分けて並列に計算する `mul_parallel`・`mul_fma_parallel` は別名の API です。`Math.fma` がハードウェアの命令になるのは AArch64 向けにビルドしたコンパイラの native だけで、それ以外の native と wasm32 では積和ごとにソフトウェアのルーチンを呼ぶので、`mul_fma` は大幅に遅くなります（ビットは同じです）。常に非 Copy で、内部は不透明です。`MatrixView` は行列を複製せずに転置・部分行列・行・列として見る借用の窓（排他スライスで書き込める窓もあります）、`Tensor` は形と行優先のバッファを持つ N 次元の配列と、軸の並べ替えなどをする窓です。行列積の GPU カーネルはありません。詳細は [Matrix](_tsuzuri/language-reference/built-in-types-and-modules/matrix.md)・[MatrixView](_tsuzuri/language-reference/built-in-types-and-modules/matrix-view.md)・[Tensor](_tsuzuri/language-reference/built-in-types-and-modules/tensor.md) を参照してください。
- **SIMD ベクトル**: 128-bit 幅の `f32x4`、`f64x2` と 256-bit 幅の `f32x8`、`f64x4`、整数ベクトル型をサポートします。`Simd.splat`、`Simd.load`、`Simd.store`、`Simd.extract`、`Simd.sum_lanes` などの高効率な組み込み演算を提供します。関数に `@cpu ["avx2", "sve"]` を付けると、native の成果物が実行時に CPU の命令セットごとの版を選びます。

### 関数と型クラス

#### 関数の宣言と型シグネチャ

関数は `def name :: 型 = \引数 -> 本体` の形式で、型宣言と実装を一体として記述します。引数は半角スペースで区切ります（従来の `def` と `fn`／`let` を分離する形式も互換性のため受理されます）。
関数名には `calculate_total` のようなスネークケース（`snake_case`）を推奨します。

```text
def add :: Add<'a> -> 'a -> 'a = \x -> \y -> x + y

def identity :: 'a -> 'a = \x -> x

let add20 = add 20i32
let integer = add20 22
let decimal: d128 = add 0.1d128 0.2d128
let text = identity "こんにちは"
integer
```

#### 型制約とモジュール関数制約

`Add<'a>` は `Add` 型クラスの制約を持つ型変数 `'a` を表します。`def add :: 'a -> 'a -> 'a` のように記述して本体の実装から制約を推論させることも、`Add<'a> => 'a -> 'a -> 'a` と明記することも可能です。
複数の制約を課す場合は、`def` の次の行にインデントして型変数ごとに宣言できます。

```text
def increment :: 'a -> 'a
    @'a : Add, Integer = \value -> value + 1

def distance_of :: 'T -> 'U
    @'T : Copy, #distance = \value -> 'T.distance value
```

`#distance` は、指定された具体的な型（レコードや共用体）の定義元モジュールに存在する関数を要求する「モジュール関数制約」です。例えば `'T` が `Point` 型であれば `'T.distance` は自動的に `Point.distance` を参照し、戻り値の型も厳密に照合・推論されます。詳細は [モジュール関数の制約](docs/language.md#モジュール関数の制約) を参照してください。

#### カリー化・演算子・クロージャ

- **カリー化と静的単相化**: すべての関数は自動的にカリー化されており、部分適用を自然に行えます。型クラスによるメソッド解決はコンパイル時に完全に完了し、動的ディスパッチやランタイム辞書渡しのコストを一切発生させません（明示した [`dyn` 型](_tsuzuri/language-reference/types-and-type-inference/type-classes.md) を除きます）。呼び出しに使われた具体的な型の組み合わせごとに最適化されたコードを生成します。
- **演算子**: パイプライン演算子 `|>`、関数合成演算子 `>>`（順方向: `\x -> g (f x)`）および `<<`（逆方向: `\x -> f (g x)`）、累乗演算子 `**`（右結合）、組み込みの `not` や `ignore` を提供します。ビット演算には F# と同様の `&&&`、`|||`、`^^^`、`~~~`、`<<<`、`>>>` を使用します（`>>>` は符号付き整数なら算術シフト、符号なし整数なら論理シフト）。詳細は [演算子と式](_tsuzuri/language-reference/values-and-functions/op-and-expressions.md) を参照してください。
- **クロージャと環境捕捉**: ラムダ式（`\x -> ...`）は外側のスコープの値を安全に捕捉できます。捕捉された所有値は関数値自身が管理し、関数値のコピー時には捕捉環境の独立したスナップショットが作成されます。完全適用された既知の関数呼び出しは、環境オブジェクトの確保を行わず直接呼び出しへと最適化されます。

#### 型クラスの定義とインスタンス化

型クラスは **`.tt`** ファイルに宣言します。1 つのファイル内に複数の型クラスを定義できます。

```text
class Score<'a> {
    def score :: ref 'a -> i32
}

class Size<'a> {
    def size :: ref 'a -> i64
}
```

型クラスのインスタンス実装は、レコード定義と同じく `.tz` ファイルに記述します。

```text
record Point { x: i32, y: i32 }

instance Add<Point> {
    fn add left right = Point { x: left.x + right.x, y: left.y + right.y }
}

instance Classes.Score<Point> {
    fn score point = point.x + point.y
}
```

呼び出し時は `Classes.Score.score ref point` のように型クラス名を明示してメソッドを呼び出すことも可能です。これにより、名前の競合やインスタンスの解決順序に依存しない予測可能な記述が保証されます。

#### レコードと共用体 (union)

- **ジェネリックレコード**: フィールド型に型パラメーターを持つレコードを宣言できます。型パラメーターおよび型引数は常に `Name<'a, 'b>` や `Name<i64, string>` のように `<...>` で括って記述します。詳細は [ジェネリックなレコード](docs/language.md#ジェネリックなレコード) を参照してください。レコードの値は、宣言の順に並べるよりも小さくなるときだけ、フィールドをアライメントの大きい順に格納します（たとえば `{ bool, i64, i8, f64 }` は 32 バイトでなく 24 バイト）。観測できる動作は宣言の順のままです。詳細は [メモリ上の大きさ](_tsuzuri/language-reference/built-in-types-and-modules/record.md#メモリ上の大きさ) を参照してください。

```text
record Pair<'a, 'b> { first: 'a, second: 'b }

def swap :: Pair<'a, 'b> -> Pair<'b, 'a> = \pair ->
    Pair { first: pair.second, second: pair.first }

let pair: Pair<string, i64> = swap (Pair { first: 42, second: "answer" })
pair.second
```

- **代数的データ型 (`union`)**: 各ケース（variant）は 0 個または 1 個のペイロードを持ちます。ペイロードを持つケースは 1 引数のコンストラクタ関数値としても扱えます。値はタグ（ケースの数に応じて 1・2・4 バイト）と、最大のペイロードのアライメントに揃えた共有領域で、たとえば下の `Shape` は 24 バイトです。`Maybe<string>` のように、ペイロードなしのケースと文字列・配列のペイロードだけを持つ共用体はタグを持たず、ペイロードそのもの（16 バイト）です。ネイティブのターゲットでは、`ref` のペイロードも null ポインターを `None` にして 8 バイトです。関数値と `Task` のペイロードも、どのターゲットでも null の記述子を `None` にして、関数値と同じ大きさ（ネイティブは 16 バイト）です。詳細は [メモリ上の大きさ](_tsuzuri/language-reference/built-in-types-and-modules/union.md#メモリ上の大きさ) を参照してください。

```text
union Shape =
    | Circle of f64
    | Rect of f64 * f64
    | Empty

union Reply<'a> = Pending | Ready of 'a

def area :: Shape -> f64 = \shape ->
    match shape with
    | Circle r -> r * r * 3.141592653589793
    | Rect (w, h) -> w * h
    | Empty -> 0.0

area (Rect (3.0, 4.0))
```

- **自動導出 (`deriving`)**: レコードや共用体の宣言末尾に `deriving (Eq, Ord, Display, Hash, Default)` を指定することで、構造的なインスタンス実装を自動生成できます。`deriving (Encode, Decode)` はレコードを JSON の object、共用体を `"Case"` / `{"Case": payload}` と相互変換します。
- **再帰的データ型**: `union Tree<'a> = Leaf | Node of Tree<'a> * 'a * Tree<'a>` のように木構造や構文木を定義できます。再帰ケースは自動的にヒープへ配置され、解放や環境の複製時にスタックオーバーフローを起こさない工夫が施されています。詳細は [共用体 (union)](docs/language.md#共用体union) を参照してください。
- **高階型 (HKT)**: カインドを明示したランク 1 の高階型をサポートしています（例: `class Functor<'f: * -> *> { def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b> }`）。詳細は [HKT仕様](docs/language.md#高階型hkt) を参照してください。

### 制御構文とパターンマッチ

```text
let mut total = 0
for (x, y) in [(1, 2), (3, 4)] do
    total = total + x + y
for i = 1 to 5 do
    total = total + i as i64
while total < 40 do
    total = total + 1

let add = \x y -> x + y
match add total 2 with
| 0 -> 0
| answer when answer > 0 -> answer
| otherwise -> -1
```

- **反復構文**:
  - `for…in`: 配列、連結リスト、整数範囲（`start .. [step ..] finish`）を走査します。文字列の反復では、`string` は UTF-16 コード単位（`i16u`）、`utf8string` は UTF-8 バイト（`ubyte`）を列挙します。
  - `for i = start to end do` / `downto`: 両端を含む整数の範囲反復を行います（F# と同様の仕様です）。
  - `while condition do`: 条件を満たす間ループを実行します。ループ全体の評価値は常に `unit` です。
- **条件分岐**: `if condition then expr1 else expr2`、`elif`、および `unit` を返す `else` 省略形をサポートします。従来の `{ ... }` ブロック表記も利用可能です。
- **パターンマッチと網羅性検査**: 定数、変数、ワイルドカード（`_` / `otherwise`）、タプル、レコード、共用体ケース、配列、リスト、コンス（`head :: tail`）、OR / AND パターン、型注釈、`as` パターンに対応しています。コンパイラは `match` の網羅性を静的に検証し、漏れがある場合は不足しているパターンの具体例とともに `E1021` エラーを報告します。また、到達不能な重複節は `W1003` 警告を発行します（`when` ガード付きの節は網羅性の計算には含まれません）。
- **アクティブパターン**: 入力値を任意の論理的ケースに変換・分解するアクティブパターンに対応しています。`Maybe` や `bool` を返す部分アクティブパターン（例: `def (|Parsed|_|) :: ref string -> Maybe<i64> = \text -> Parse.parse text`）や、共用体と連動する完全アクティブパターンが利用可能です。
- **再帰関数**: 再帰関数は `def rec name :: ...`、相互再帰は `and name :: ...` で繋いで記述します。関数の本体やブロックの中では、`let rec go = \n acc -> ...`（相互再帰は `and`）で補助関数を定義できます。外側のローカル値を捕捉したまま再帰でき、文字列や配列などの値は借用して読みます。直接の自己末尾再帰は、`-O0` であっても LLVM レベルでループ構造へと自動変換されます（`def rec` も `let rec` も同じです）。詳細は [言語仕様: 制御構文](docs/language.md#制御構文)、[言語仕様: ローカルな `let rec`](docs/language.md#ローカルな-let-rec)、および [制御構文のベンチマーク](docs/benchmarks.md#制御構文の比較) を参照してください。実行例は `tsuzuri run examples/control` で確認できます。

### ユーザー定義のコンピュテーション式

F# のように、計算の組み合わせや制御フローの動作をカスタマイズできるコンピュテーション式をサポートしています。
**1 つの `.tc` ファイルが 1 つのビルダー** に対応し、ファイル名がそのままビルダー名となります。言語への特別な構文登録は不要で、標準ライブラリの `Result` や `Maybe` もこの仕組みで実装されています。

```text
let answer: Result<i64, string> = Result {
    let! first = Ok 20
    let! second = Ok 22
    return first + second
}
match answer with
| Ok value -> value
| Error _ -> -1
```

- **構文要素**: `let!` は `Builder.Bind`、`return` は `Builder.Return` に展開されます。その他、必要に応じて `ReturnFrom`、`Yield`／`YieldFrom`、`Zero`、`Combine`、`For`／`While`、`Delay`／`Run` を実装できます。使用した構文に対応する操作がビルダーに未定義の場合は、明確なコンパイルエラーとなります。
- **ビルダーの別名**: `.tc` に `@alias async` のように書くと、`async { ... }` と小文字でも同じビルダーを呼べます（`Async { ... }` と同じ展開）。標準の `Async` は `@alias async` を宣言済みです。詳細は [ビルダーの別名](_tsuzuri/language-reference/computation-expressions/computation-expressions.md#ビルダーの別名) を参照してください。
- **高度な演算**: 複数ソースの並行的な合成を行う `and!` や `match!`、およびビルダーが提供する `MergeSources` / `BindReturn` / `Bind2` に対応しています（`and!` の各項は左から右へ順に一度ずつ評価され、勝手な自動並列化は行われません）。
- **暗黙のコンピュテーション式**: 通常の関数や `main` の本体において、明示的なビルダーブロックを省略して `let!` や `do!` を直接記述することも可能です。コンパイラが戻り値の型に基づいて適切なビルダーを自動合成します。詳細は [暗黙の計算式](docs/language.md#ビルダー名を省略した本体) を参照してください。
- **最適化**: スコープを脱出しないローカルな継続はコンパイラによって特殊化・インライン化され、不要なクロージャのメモリ確保や間接関数呼び出しが完全に消去されます。特殊化した関数（worker）の数には 1,024 個の予算があり、使い切る program では、予算で完成できる連鎖を完成させ、残りは通常の経路になります（[測定](docs/benchmarks.md#抽象化コストの除去pr032026-10-16)）。詳細は [ビルダーの仕様](docs/language.md#コンピュテーション式) および [コンピュテーション式の比較](docs/benchmarks.md#コンピュテーション式の比較) を参照してください。実行例は `tsuzuri run examples/computations` で確認できます。

### タスクと並列処理

```text
let computation = task {
    let! values = Task.parallel [
        task { return 20 },
        task { return 22 }
    ]
    return values[0] + values[1]
}

Task.run computation
// 出力: 42
```

- **遅延・一回実行のタスク**: `task { ... }` は `Task<T>` 型の値を生成します。定義時点では実行されず、`Task.run` を呼び出した時点で初めて実行が開始されます。二重実行はコンパイルエラーとして検出されます。また、未実行のままスコープを抜けたタスクは本体を実行せずに捕捉リソースを安全に解放します。
- **安全なスレッド分離**: タスクが捕捉する値は所有権の移動（move）または Copy に限定され、参照と `Rc` の持ち込みはコンパイル時に拒否されます。これにより、共有可変状態によるデータ競合の発生を根本から防ぎます。共有して書き換える状態は、`Atomic`（整数と `bool`）と `Mutex`（ほかの値）に限られ、`Arc<Atomic<T>>`、`Arc<Mutex<T>>`、または次の `Task.scope` の共有借用でタスク間に渡します。値の受け渡しは、容量付きの `Channel`（最後の `Sender` の drop で閉じ、全スレッドが待ったらデッドロックのトラップ）です。`Channel` の要素は借用を持てず（関数値は、環境が借用を持たないものだけ）、`Send` の検査は、ジェネリック関数を通しても、使う型ごとに行われます。`Sync` でない値（`Rc`、ホストのハンドルなど）は共有できません。
- **借用を共有する並列区間 (`Task.scope`)**: `Task.scope (ref shared) count callback` は、`Sync` な値の共有借用を `count` 個の子どもに貸し、`callback shared index` の結果を添字順に返します。すべての子どもが終わってから戻るので、外の値を持ち込まずに共有できます。
- **並列実行とスレッドプール**: `Task.parallel` はタスクの配列を受け取り、入力順と同一の結果配列を返します。ネイティブ環境では POSIX threads を基盤とした常駐スレッドプール（最大 32 スレッド）をオンデマンドで起動し、効率よくタスクを分散します。
- **データ並列 API**: タスクオブジェクトの生成オーバーヘッドを抑えたい大量のデータ処理には、`Parallel.init`、`Parallel.map`、`Parallel.map_ref`、`Parallel.reduce`、`Parallel.sum` を使用します。配列を固定チャンクに分割し、最小限の同期コストで高速に処理します。詳細は [データ並列 API](docs/language.md#データ並列-api) を参照してください。実行例は `tsuzuri run examples/tasks` で確認できます。
- **エラー短絡 (`Task.parallel_results`)**: 複数の `Task<Result<T, E>>` を並列実行し、いずれかが失敗した時点で未開始のタスクを即座にキャンセルして最小インデックスのエラーを返します。

```text
let jobs: [Task<Result<i64, string>>] = [
    task { Result.Ok 20 },
    task { Result.Error "failed" }
]
match Task.run (Task.parallel_results jobs) with
| Result.Ok values -> Array.sum (&values)
| Result.Error _ -> -1
```

### 非同期計算 (`Async`)

待ち時間のあいだに同じスレッドで別の計算を進めるには、標準の `Async` を使います。`Async.run` は仮想時刻、`Async.block_on` は実時間、`Async.start` はホストの poll で実行します。CPU のコアを使う並列処理は、上の `Task.parallel` と分担します。

```text
let work = Async {
    do! Async.sleep 10
    let! now = Async.now ()
    return now + 32
}

Async.run work
// 出力: 42
```

ビルダーの `.tc` に `@alias` を書くと、小文字の別名でも同じビルダーを呼べます。標準の `Async` は `@alias async` を宣言しているので、`async { ... }` は `Async { ... }` と同じ計算です。別名の規則は [コンピュテーション式](_tsuzuri/language-reference/computation-expressions/computation-expressions.md#ビルダーの別名) を参照してください。

`Async.all` は子を入力順に協調スケジュールし、`Async.all_results` は最初に時刻順で完了した `Error` で残りを取り消します。`Async` の値はコールドで非 Copy です。中断をまたぐ借用は `E1013` で拒否されます。詳しくは [Async 式](_tsuzuri/language-reference/async-tasks-and-lazy/async.md) を参照してください。

### 入出力・OS 連携・定数

#### 入出力 (`IO<T>`)

副作用を伴う入出力処理は、遅延アクションを表す `IO<T>` 型によって純粋なコードから分離されます。

```text
def main :: unit -> i32 = \() ->
    do! IO.write_line "お名前を入力してください:"
    let! name = IO.read_line ()
    match name with
    | Some n -> do! IO.write_line ($"こんにちは、{n} さん！")
    | None -> do! IO.write_line "入力が終了しました。"
    0
```

- `IO.write_line`（別名 `IO.writeln`）や `IO.read_line` などの基本関数を提供します。
- 失敗の可能性がある操作には `IO.try_*` 系統の関数を使用し、結果を `Result` 型として安全に処理できます。詳細は [IO](_tsuzuri/language-reference/built-in-types-and-modules/io.md) を参照してください。

#### OS API

ファイルシステム、プロセス、環境変数、時刻などの OS 機能は、標準モジュール `File`、`Dir`、`Path`、`Env`、`Time`、`Random`、`Process`、`Os` で提供されます（macOS および Linux 対応）。
すべての OS 操作は `IO<Result<T, Os.Error>>` などの一貫した遅延アクションとして抽象化されています（`Path` および擬似乱数 `Random.Pcg` は純粋関数です）。WASM 環境では `--wasm-host wasi` を指定することで WASI preview1 に接続可能です。

#### ネットワーク API

TCP と UDP のソケット、IP アドレスの解析と表示、名前解決は、opt-in の標準モジュール `Net` で提供されます（ソケットは macOS・Linux・Windows の native と、`--wasm-feature jspi --wasm-feature net` を付けた Node.js 向けの wasm32。アドレスの解析と表示はどの target でも import なしで動きます）。
操作は `IO<Result<T, Os.Error>>` で、`Async.block_on` の中では `_async` の双子（`connect_async` など）を使えます。ハンドルは `Copy` の不透明な値です（`close` か `with_connection` などで閉じます）。通信は暗号化されません。API と例は [Net](_tsuzuri/language-reference/built-in-types-and-modules/net.md) を参照してください。

#### エントリーポイント (`Main.tz`)

アプリケーションは `Main.tz` から開始します。
エントリーポイントとして、`def main :: unit -> i32` またはコマンドライン引数を受け取る `def main :: Array<string> -> i32` を定義します。`main` が返す `i32` の値がプロセスの終了コードとなり、コンソールに自動表示されることはありません（終了コードが 0 以外の場合は `tsuzuri run` が `E2005` で報告します）。コマンドライン引数は、`tsuzuri script` でファイルより後ろに書くか、ビルドした実行ファイルに渡します。
トップレベルの `let` 式および最後の結果式で記述されたプログラムも引き続き実行可能で、結果値が `Display` を実装していれば標準出力に出力されます。

#### コンパイル時定数 (`const`)

`const Answer: i64 = 40 + 2` のように、型注釈付きのコンパイル時定数を宣言できます。
整数および浮動小数点の演算、文字列、配列、タプル、レコードリテラルを扱うことができ、モジュールを超えた修飾参照や前方参照にも対応しています。
`const def` を付けた関数は、`const` の初期化式から呼べます。ループや再帰を含む計算（表の生成やチェックサムなど）をコンパイル時に評価し、評価した値は他の定数と同じく値として使う箇所へ展開されます。`const def` は通常の関数でもあり、実行時にも呼べます。
値が平たいデータ（数値、タプル、レコード、固定長配列、配列など）だけの定数は、`Table[i]`・`ref Table` のような読み出しと借用で、バイナリに置いた読み取り専用の 1 つの表を直接指します（ループのたびに作り直しません）。その借用は関数から返したり、レコードへ格納したりできます。

```text
const def square :: i64 -> i64
fn square x = x * x

const Area: i64 = square 12
```

#### デバッグ出力と例外処理

- **デバッグ出力**: `Debug.print value` は値を借用して標準エラー出力に表示し、`Debug.trace value` は表示を行ったうえで同じ所有値をそのまま返します（パイプラインの途中での値確認に便利です）。
- **チェック付き演算と例外**: `@checked x + y` は演算オーバーフロー時に `OverflowException` を送出します。この例外は関数境界やラムダ式を越えず、同一関数内の最も内側の `try ... with ... finally` 式によって `Result` 型の値へと安全に変換されます。詳細は [例外処理](_tsuzuri/language-reference/exception-handling/exception-handling.md) を参照してください。

### 名前空間・モジュール・パッケージ

Tsuzuri は **1 つのファイルが厳格に 1 つのモジュールを構成する** 原則を採用しています。モジュール名はファイル名に基づき、英大文字のアスキー文字から始めます。

#### 名前空間宣言 (`namespace`) とインポート (`using`)

- **名前空間宣言**: ファイルの先頭行に `namespace Sample::Features` と宣言することで、モジュールが所属する名前空間を決定します。
- **インポート宣言**: `using Sample::Features` と記述することで、その名前空間に属するモジュールをプレフィックスなしで直接参照できるようになります。
- **区切り記号の規則**: 名前空間どうし、および名前空間とモジュール名は **`::`** で接続します。モジュール名とその中のメンバー（関数・型・ケースなど）は **`.`** で接続します（例: `Sample::Features::Shape.area`、`Sample::Point { ... }`）。
- **モジュールと同名の型**: モジュール名と一致するレコードや共用体の完全名はモジュールの完全名そのものとなり、`Point.Point` のようにモジュール名を重複して記述する必要はありません（`Point.Point` は `E1004` エラーとなります）。

`Point.tz`:

```text
namespace Geometry

record Point { x: f64, y: f64 }

def distance :: Point -> f64 = \point -> sqrt (point.x * point.x + point.y * point.y)

export def hypotenuse :: f64 -> f64 -> f64 = \x y -> distance (Point { x: x, y: y })
```

同一ディレクトリの `Main.tz`:

```text
namespace Geometry

using Geometry

def main :: unit -> i32 = \() ->
    let p = Point { x: 10.0, y: 20.5 }
    let d = Point.distance p
    do! IO.write_line d
    0
```

#### ファイル拡張子の役割

| 拡張子 | 役割と記述内容 |
| --- | --- |
| `.tz` | レコード、共用体、関数、型クラスのインスタンス実装。`Main.tz` のみトップレベル実行コードを記述可能。 |
| `.tt` | 型クラスのシグネチャ宣言専用。関数本体やレコード定義、インスタンス実装は配置不可。 |
| `.tc` | コンピュテーション式のビルダー定義、関連補助関数、ビルダーの別名（`@alias`）。トップレベル実行コードは配置不可。 |

拡張子が異なっていても同名のモジュールを同一ディレクトリに共存させることはできません（例: `Checked.tz` と `Checked.tc` の併存はエラー）。

#### モジュールの可視性と予約モジュール名

- **可視性**: 宣言は既定で外部モジュールへ公開されます（public）。モジュール内でのみ使用する補助関数やデータ構造には `private` キーワードを付与します。ホスト言語（C や WebAssembly）へシンボルを公開する場合は `export` を指定します。
- **標準ライブラリの名前空間 `std`**: 標準ライブラリは名前空間 `std` に属しており、`std::Maybe` や `std::Task.run` のように完全修飾することも可能です。以下の名前は予約されており、ユーザーが同一名のモジュールを作成することはできません（`E1011` エラー）。

| 予約名 | 用途 |
| --- | --- |
| `Maybe`, `Result` | 成功・失敗および値の存在・欠落を表現する基本データ型 |
| `Array`, `List`, `Vec`, `Map`, `Set`, `HashMap`, `HashSet`, `Arena`, `Matrix`, `MatrixView`, `Tensor` | 各種コレクションおよびデータ構造 |
| `Rc`, `Arc` | 参照カウントによる共有所有 |
| `Atomic`, `Mutex` | 共有借用から更新できる整数・`bool` のセルと、ロックで守る値（`Task.scope`、`Arc` と組み合わせる） |
| `Channel` | 容量付きの MPMC キュー。`Sender`／`Receiver`、`bounded`・`send`・`recv`・`clone_sender`。最後の `Sender` の drop で閉じ、全スレッドが待ったらデッドロックのトラップ |
| `String`, `Utf8String`, `Char` | UTF-16 / UTF-8 文字列および文字操作 |
| `Regex`, `Unicode` | 線形時間の正規表現、Unicode 17.0.0 の文字データ |
| `Math`, `Int` | 高精度数学関数、浮動小数点超越関数、整数組み込み演算 |
| `Debug`, `Test` | デバッグ出力およびテストフレームワーク |
| `Bench`, `Gen` | ベンチマーク（`tsuzuri bench`）とプロパティテストの生成器 |
| `Parallel`, `Simd`, `Gpu` | データ並列処理、128-bit・256-bit SIMD 演算、GPU カーネル連携 |
| `Async` | 協調的な非同期計算、仮想時刻、native reactor、ホストの再開 |
| `File`, `Dir`, `Path`, `Env`, `Time`, `Random`, `Os`, `Process` | ファイル、環境変数、システム時刻、プロセス管理などの OS API |
| `Net` | TCP・UDP のソケット、IP アドレスの解析と表示、名前解決（opt-in。ソケットは macOS・Linux・Windows の native と、`--wasm-feature jspi --wasm-feature net` の Node.js 向け wasm32。`Async.block_on` の中では `_async` の双子） |
| `Format` | 文字列補間およびカスタムフォーマット用ヘルパー |
| `Json` | JSON の解析・出力と `Encode` / `Decode` による値の変換 |
| `Cbor` | `Json.Value` の CBOR（RFC 8949）の読み書き |

#### パッケージ管理 (`Tsuzuri.toml`)

プロジェクトルートに `Tsuzuri.toml` を配置することで、パッケージ名、言語版、既定の名前空間、およびローカルや git リポジトリの依存パッケージを定義できます。

```toml
[package]
name = "app"
version = "0.1.0"
edition = "2026"
namespace = "Acme::App"

[dependencies]
geometry-core = { path = "../geometry-core" }
shapes = { git = "https://example.org/shapes.git", rev = "0123456789abcdef0123456789abcdef01234567" }
tiles = { version = "1.2.0" }

[registry]
index = "https://example.org/tsuzuri-index.git"
```

依存パッケージのモジュールは `GeometryCore::Point`（依存先が `namespace` を持てばその名前空間）のように完全修飾名で参照します。git 依存は commit を固定し、registry 依存（`version`）は `[registry]` の git の index から最小版選択で版を決めます。`tsuzuri fetch app` が `git` で取得して内容の SHA-256 と選んだ版を `Tsuzuri.lock` に記録します。index は利用者や組織が置き（Tsuzuri は公開の registry を運営しません）、`tsuzuri publish` が index に足す項目を出力します。ネットワークに触れるのは `fetch` と `publish` だけで、`check`・`build`・`run` などは `Tsuzuri.lock` とキャッシュ内のストアだけを読み、内容の一致を検証します。詳細は [ローカルパッケージの仕様](docs/language.md#ローカルパッケージ) と [パッケージ](_tsuzuri/language-reference/organizing-tsuzuri/packages.md) を参照してください。

`edition` は、そのパッケージのソースを読むときの予約語の版です。対応する値は `"2026"` だけで、省略しても `2026` です。依存パッケージは自分の `Tsuzuri.toml` の `edition` で読まれ、edition の違うパッケージを 1 つのプログラムにまとめられます。edition が変えるのは予約語と警告の既定だけで、生成されるコードは変わりません。何を保証するか、非推奨にした宣言をどう取り除くかは [安定性と互換性の方針](docs/stability.md) にあります。

---

## 外部ターゲットとホスト連携

### C ABI とネイティブホスト組み込み

Tsuzuri で記述した関数は、C/C++ や他の言語ランタイム、GUI ホストから直接呼び出すことができます。

```sh
# オブジェクトファイルおよび C ヘッダーを生成
./target/release/tsuzuri build examples/web/Physics.tz --emit object -o target/examples/physics.o
./target/release/tsuzuri build examples/web/Physics.tz --emit header -o target/examples/physics.h

# C ホストコードとリンクして実行
clang -O3 examples/native/main.c target/examples/physics.o -I target/examples -lm -o target/examples/native
./target/examples/native
```

- **ホスト関数の宣言 (`extern`)**: `extern def now :: unit -> i64` のように宣言することで同期ホスト関数を呼び出せます。ネイティブ環境では `tsuzuri_host_Main_now`、WASM 環境では `tsuzuri` モジュールの `Main.now` に自動接続されます。既存の C 関数と直接接続したい場合は `extern "sqrt" def c_sqrt :: f64 -> f64` のようにリンク名を明示します。
- **C ヘッダーからの生成 (`tsuzuri bindgen`)**: `tsuzuri bindgen zlib.h -o Zlib.tz` は、C ヘッダー自身の宣言と整数の `#define` のうち ABI が一致すると確かめられるものを、リンク名付きの `extern`・`const`・`record`・`extern type`（不透明な struct）・型エイリアスとして書き出します（64-bit の Linux と macOS）。関数ポインターの引数はコールバックに、`--buffer` で指定したポインターと長さの組は `ref [T]` になります。変換できない宣言は推測せず、理由付きの `// skipped` 行と警告 `W2002` にします。
- **不透明ハンドルとコールバック**: `extern type Counter` でホスト側のポインタを安全な不透明ハンドルとして扱えます。また、環境キャプチャを持たないトップレベル関数は関数ポインタとしてホストへ渡せます。
- **ホストライブラリのリンク**: ネイティブ実行ファイルのビルド時には、`--link PATH`、`-l NAME`、`-L DIR` や `Tsuzuri.toml` の `[native]` セクションを通じて、外部の C/C++ ライブラリやオブジェクトを直接リンクできます。
- **共有ライブラリと各言語のバインディング**: `--emit shared` は公開 C ABI（`tz_*`、`tsuzuri_alloc`、`tsuzuri_free` など）だけを export する共有ライブラリ（macOS は `.dylib`、Linux は `.so`。Windows は未対応で `E2000`）を出します。`--emit bindings-cs`、`--emit bindings-py`、`--emit bindings-cpp` は、それを呼ぶ C#（`[LibraryImport]` と `SafeHandle`）、Python（`ctypes`）、C++20（C ヘッダーの上の RAII）のバインディングを同じソースから生成します。`--trap-mode return` を付けると、トラップが各言語の例外になります。
- **言語間 LTO（bitcode）**: `--emit bitcode` は、プログラムと、それが使う C ランタイムを 1 つにまとめた ThinLTO の LLVM bitcode（`.bc`）を出します。C・C++ のホストが、bitcode を書いた Clang と同じ Clang の `-flto=thin`（Linux は `-fuse-ld=lld`）でリンクすると、`tz_add` のような小さな公開関数をホストのループへ inline できます。`--target wasm32` は `wasm-ld` の LTO で同じことができます。C ランタイムを使うプログラムには、Clang と同じ LLVM の `llvm-link`（`TSUZURI_LLVM_LINK`）が要ります。Windows と `wasm64` は `E2000` です。10 億回のループの測定（M1 Max、macOS、共有機の目安）では、bitcode のリンクは `--emit object` のリンクの約 0.5〜0.6 倍の時間で、C で書いた同じ関数と区別できませんでした。使い方と確かめた組み合わせは [ネイティブ連携](_tsuzuri/language-reference/compiler/native-interop.md#言語間-lto-と-bitcode)、測定は [docs/benchmarks.md](docs/benchmarks.md#pr08-言語間-lto-と-bitcode-出力) にあります。
- **デスクトップ GUI 連携の例**: Python/Tkinter などのデスクトップ GUI から Tsuzuri のネイティブ共有ライブラリを呼び出すことも可能です（詳細は `examples/desktop` を参照してください）。

### WebAssembly (ブラウザ / Node.js)

Tsuzuri はブラウザおよび Node.js 向けにスタンドアロンの WebAssembly（wasm32 / wasm64）を出力できます。入出力を使用しない純粋な計算モジュールは、外部の JavaScript ランタイムやグルーコードを一切必要とせず直接インスタンス化できます。

```sh
# WebAssembly モジュールをビルド
./target/release/tsuzuri build examples/functional/Main.tz --target wasm32 -o target/functional.wasm
```

Node.js からの呼び出し例:

```javascript
import { readFileSync } from "node:fs";

const wasmBytes = readFileSync("target/functional.wasm");
const { instance } = await WebAssembly.instantiate(wasmBytes);

// 公開関数には接頭辞 `tz_` が付与されます
console.log(instance.exports.tz_transform(1n, 2n, 3n, 4n)); // 42n
```

- **型付きのグルー生成**: `tsuzuri build examples/web/Physics.tz --target wasm32 --emit bindings-js -o physics.mjs` は、`.wasm` を型付きの関数として呼ぶ JavaScript モジュール `physics.mjs` と TypeScript 宣言 `physics.d.mts` を出します。引数の検査、バッファの複製と `tsuzuri_free`、記述子の読み書き、型付きの import、トラップを `TsuzuriTrap` にしてインスタンスを作り直す処理を行います。`--wasm-feature threads` を足すと、COOP / COEP 付きのページで Web Worker のスレッドプールを作るグルーになります（満たさないページでは `Error` で、逐次実行には切り替えません）。

  ```javascript
  import { load } from "./physics.mjs";
  const api = await load(await (await fetch("physics.wasm")).arrayBuffer());
  console.log(api.exports.next_positions(Float64Array.of(9, 1), Float64Array.of(3, -3), 1, 10)); // Float64Array [8, 2]
  ```
- **型変換の規則**: 64-bit 整数（`i64` / `i64u`）は JavaScript の `BigInt`、`f32` / `f64` は `Number`、`bool` は `i32`（0 = false, 1 = true）に対応します（生成したグルーは `boolean` に直します）。
- **メモリとスタックのカスタマイズ**: `--wasm-max-memory SIZE`（既定 16MiB、最大 4GiB-64KiB / wasm64 は 16GiB）や `--wasm-stack-size SIZE`（既定 1MiB）で線形メモリの上限やメインスタックサイズを調整できます。これらは `Tsuzuri.toml` の `[wasm]` セクションでも設定可能です。
- **マルチスレッド (`threads`)**: `--wasm-feature threads` を指定することで、Task や Parallel による並列計算を Web Worker や Node.js の Worker Threads に分散できます。`Atomic`、`Mutex`、`Channel`、`Task.scope` も Worker で動きます（ロックとチャンネルの待ちは共有メモリの `memory.atomic.wait32` で、スレッドごとの状態はインスタンスごとのグローバルです）。詳細は [Webホスト要件](examples/web/README.md) を参照してください。

### GPU カーネル連携（実験的）

単一の `export` された `i32 -> i32` または `i32u -> i32u` カーネルを含むプロジェクトから、`tsuzuri build Kernel.tz --emit wgsl -o kernel.wgsl` により WebGPU 向けの WGSL シェーダーを生成できます。
これは厳密な整数演算に基づく出力で、自動オフロードや速度優位を保証するものではありません。`f32` と `f16` は GPU では厳密にできないため、名前で緩い意味を選びます。`--emit wgsl-relaxed`（`f32`・`i32`・`i32u` のカーネルで、内部で `f16` も使えます）と、`Gpu.init_relaxed`・`Gpu.map_relaxed` です。CPU 上の評価は厳密なままで、GPU 上の結果だけが WGSL の浮動小数点規則に従います。

言語ランタイムから WebGPU で動かすには、`Gpu.request Gpu.WebGpu` を使います。デバイスが開けたときだけ `Result.Ok` で、ライブラリやアダプタがなければ `Result.Error Gpu.Unavailable` です（CPU への置き換えはしません）。native は、リンク時の依存なしに wgpu-native 29 を実行時に読み込みます（`TSUZURI_WEBGPU_LIBRARY` で名指しするか、システムの場所の絶対パスにあるものだけで、作業ディレクトリは探しません。`TSUZURI_GPU_DEBUG=1` で理由を表示）。WebAssembly は `--wasm-feature webgpu` を付けると `tsuzuri_gpu.open` と `tsuzuri_gpu.run` を import し、`src/runtime/webgpu.mjs` の `createGpuImports` が JSPI で実装します。厳密な `Gpu.init`・`Gpu.map` が WebGPU で動くのは `i32`・`i32u` だけで、`f32`・`f16` は緩い名前の API だけです。

Vulkan では、`tsuzuri build Kernel.tz --emit spirv -o kernel.spv` が SPIR-V（厳密な `i32`・`i32u`・`i64`・`i64u`・`f32`。除算は出せません）を、`--emit spirv-relaxed` が緩い `f32` のモジュールを書きます。`Gpu.request Gpu.Vulkan` は、native のランタイムが Vulkan のデバイスを開けて、プログラムのカーネルが要る機能（`i64` は `shaderInt64`、厳密な `f32` は float controls）をそのデバイスが持つときだけ `Result.Ok` です（ローダーは `TSUZURI_VULKAN_LIBRARY` で名指しするか、macOS はシステムの場所の絶対パス、Windows は System32 だけで、作業ディレクトリは探しません。WebAssembly では常に `Unavailable`）。**デバイスが報告する float controls は申告で、厳密さの証明ではありません。** ランタイムは、組み込みの適合プローブを通ったデバイスにだけ、厳密な `f32` を許します（MoltenVK は `-(x * 0.0)` の符号と非正規化数、SwiftShader は 2^31 以上の `i32u` から `f32` への変換で、報告と違う結果を出しました）。`Gpu.request Gpu.Auto` は常に成功し、呼び出しごとに、転送・同期・初回の費用を含めて測ったコストの規則で CPU 参照か Vulkan を選びます（`Gpu.last_backend ()` で見られます）。規則の定数は 1 台のマシンの経験則です。確かめたのは Apple M1 Max（WebGPU は wgpu-native と Dawn、Vulkan は MoltenVK と SwiftShader）だけで、float controls をすべて報告して適合プローブを通る実機、離散 GPU、離散 GPU と統合 GPU が両方ある機械（デバイスの選び方はモックだけで確認）、Linux、Windows、NVIDIA・AMD・Intel の GPU では確かめていません。詳細は [GPU 仕様](docs/language.md#gpu-kernel実験的) と [Gpu](_tsuzuri/language-reference/built-in-types-and-modules/gpu.md) を参照してください。

---

## 開発ツールとエコシステム

### プロジェクト作成 (`tsuzuri new`)

テンプレートから素早く新規プロジェクトを立ち上げられます。

```sh
tsuzuri new <directory> [--namespace <Namespace>]
```

### フォーマッター (`tsuzuri fmt`)

ソースコードを保守的に自動整形します。AST の意味を変えない安全なフォーマットを行います。

```sh
# ファイルまたはディレクトリ内のコードを整形
tsuzuri fmt src/

# 整形の差分があるか検証（CI 用、変更があれば終了コード 1）
tsuzuri fmt --check src/
```

### 言語サーバー (`tsuzuri lsp`)

VS Code や Neovim など、LSP 対応のエディタから利用可能な標準入出力（stdio）言語サーバーです。

- **主要機能**: 未保存バッファのリアルタイム全量同期、複数診断、型ホバー表示、定義ジャンプ、シンボル検索、参照検索、リネーム、補完、シグネチャヘルプ、セマンティックハイライト、未使用ローカル変数のクイックフィックス。
- **高精度なリファクタリング**: リネームやクイックフィックスは編集後のコードを内部で再解析し、安全性が確認された差分のみを適用します。詳細は [コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md) を参照してください。

### 変更の監視 (`tsuzuri watch`)

`check`・`build`・`test` を実行し、そのコマンドが読むファイルが変わるたびにもう一度実行します。各回の出力は単独のコマンドと同じで、回の終わりに `watch: run N finished with exit code C` を標準エラーへ書きます（`--json` では `{"type":"watch","run":N,"exit":C}`）。

```sh
tsuzuri watch check app/
tsuzuri watch test app/ --filter parse
```

変更は `stat` で見つけ（100 ms ごと。ファイルの追加と削除は 1 秒ごとにディレクトリを探し直します）、本文が同じ保存や `touch` では実行しません。1 回ごとの仕事は新しいコマンドと同じですが、読んだソースの本文と内容ハッシュを `stat` つきで覚えているので、変わっていないファイルは開き直しません。起動したコンパイラのファイルが差し替えられると、次の回を始める前に（待っているあいだも）止まります。Ctrl-C で止めます。詳細は [コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md#watch) を参照してください。

### 常駐ビルドサーバー (`tsuzuri serve`)

1 つのプロジェクトのビルドサーバーを前面で動かします（macOS と Linux）。同じコンパイラ・作業ディレクトリ・環境変数で起動した `check` と `build` は、サーバーが実行して出力を返し、サーバーがなければ（断られても）黙って自分で実行します。出力（標準出力・標準エラー・終了コード・成果物）は、サーバーがあってもなくても同じです。

```sh
tsuzuri serve app/ &
tsuzuri check app/
tsuzuri build app/ -O0 -o app/hello --no-server   # サーバーを使わない
tsuzuri serve --status app/
tsuzuri serve --stop app/
```

サーバーは読んだソースの本文と内容ハッシュを覚えます。要求は 1 つずつ実行し、`--idle-timeout`（既定 1800 秒）のあいだ要求がなければ終わります。`serve --status` は要求の実行中でもすぐに答え、`serve --stop` は実行中の要求を終えてから止めます（待っているあいだに Ctrl-C で止められたクライアントの要求は実行しません）。同じプロジェクトのサーバーは、同時に始めても 1 つです。ログは標準エラーに書き、標準エラーが詰まっても（読まれないパイプなど）要求・`serve --status`・`serve --stop` は止まりません（書けなかった行は捨てて、`status` の `log_dropped` に数えます）。ソケットは `TSUZURI_SERVER_DIR`（既定は macOS が `~/Library/Caches/tsuzuri/server`、Linux が `$XDG_RUNTIME_DIR/tsuzuri`）の中の `0700` のディレクトリに作り、同じ利用者だけを信頼します。空の `TSUZURI_SERVER_DIR=` はサーバーを使いません。詳細は [コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md#serve) を参照してください。

### REPL (`tsuzuri repl`)

宣言・トップレベルの `let`・式を 1 つずつ入力し、型と値を確かめる対話環境です。JIT は使わず、入力ごとにそれまでに受け付けた宣言と `let` を含む `Main.tz` を作り直し、`tsuzuri run` と同じ経路で検査・実行します（既定 `-O0`）。

```text
$ tsuzuri repl
Tsuzuri 0.1.0 REPL; enter :quit to exit
> def square :: i64 -> i64 = \x -> x * x
> square 6
it: i64 = 36
> :quit
```

1 行で完結しない入力は空行で終えます。同じ名前の再定義は元の位置で置き換え、セッション全体を検査し直します。実行中の値は持ち越さず、評価のたびに受け付けた `let` を実行し直します。コマンドは `:type`、`:load`、`:list`、`:reset`、`:quit` です。詳細は [コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md#repl) を参照してください。

### スクリプト実行 (`tsuzuri script`)

名前を問わない 1 つのファイルを、ほかのファイルも依存もないプロジェクトの `Main.tz` として `tsuzuri run` と同じように実行し、ファイルより後ろの引数を `def main :: Array<string> -> i32` へ渡します。ファイルの先頭の `#!` 行は行コメントとして読み飛ばされるので、実行権限を付けたファイルをそのまま実行できます。

```text
#!/usr/bin/env -S tsuzuri script
def main :: Array<string> -> i32 = \args ->
    do! IO.write_line ("hi " + String.join (ref " ") (ref args))
    0
```

`chmod +x greet` の後の `./greet x y` は `hi x y` を出します（macOS は `#!/usr/bin/env tsuzuri script` でも動きます）。詳細は [コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md#script) を参照してください。

### テストランナー (`tsuzuri test`)

言語組み込みの軽量テストフレームワークです。

```text
test "加算の検証" = assert (1 + 2 == 3)
```

```sh
# テストを実行
tsuzuri test tests/

# フィルタリング実行や最適化レベルの指定
tsuzuri test tests/ --filter "加算" -O3 --target native

# 成功したテストの行と関数のカバレッジを lcov で書く（ネイティブだけ）
tsuzuri test tests/ --coverage coverage.info

# プロパティテストの seed を変える（既定は固定の 11400714819323198485）
tsuzuri test tests/ --seed 42
```

プロパティテストは標準モジュール `Gen` で書きます。`Gen.for_all` は生成器の値で性質を確かめ、失敗すると反例を最も単純な値まで縮小し、seed・何個目の値か・反例を表示します。

```text
test "加算は可換" = Gen.for_all (Gen.pair (Gen.i64()) (Gen.i64())) (\pair -> match pair with | (a, b) -> a + b == b + a)
```

テストコードは型検査されますが、通常の実行可能バイナリには含まれません。また、テスト実行時は別プロセスで隔離されるため、安全に並行テストを行えます。`tsuzuri test tests/ --index 1 -g -o runner` は 1 件のテストをデバッグ情報付きのランナーとしてビルドし、デバッガーで `runner 0` として起動できます（VS Code の Testing ビューの **Debug** も同じ仕組みです）。`--coverage PATH` はコンパイラ自身の計装で、関数・`if` の枝・`match` の節・ループの本体ごとのカウンターを数え、lcov のファイルとテスト要約の後の 1 行の要約を出します。

### ベンチマーク (`tsuzuri bench`)

`bench "名前" = 本体` は、反復回数を受け取って準備を除いた経過ナノ秒を返す `i64 -> i64` の宣言です。標準モジュール `Bench` の `Bench.of` と `Bench.with_input` で書くのが普通で、計算結果は最適化で消えないよう `Bench.consume` の障壁を通ります。

```text
bench "合計" = Bench.with_input (\_ -> new [i64](1000000, \i -> i)) (\values -> Array.sum values)
```

```sh
# 既定の -O3 で、1 標本 10 ms を目標に反復回数を較正し、11 標本の中央値・最小・最大を出す
tsuzuri bench benches/

# 標本数と JSON Lines 出力（PX01 の欄名: workload, target, opt, metric, unit, samples, median, min, max）
tsuzuri bench benches/ --samples 21 --json
```

各ベンチは別プロセスで 1 件ずつ計測されます（ネイティブのみ）。速さの合否の閾値はなく、失敗はトラップ・時間切れ・負の時間だけです。`bench` は予約語です。

### ドキュメント生成 (`tsuzuri doc`)

ソースコード内の `///` ドキュメントコメントを抽出し、公開 API の Markdown ドキュメントを自動生成します。

```sh
# 公開 API ドキュメントを生成
tsuzuri doc src/ -o docs/api
```

宣言の説明を `@deprecated` で始めると（`/// @deprecated use next instead`）、その宣言は非推奨になります。別のパッケージや標準ライブラリがそれを使うと、警告 `W1005` が出ます。同じパッケージの中の使用には出ません。

### 公開 API の差分 (`tsuzuri api-diff`)

パッケージやプロジェクトの 2 つの版の公開 API を比べ、互換性のない変更（公開宣言の削除、署名の変更、record の field や union の case の追加など）と、互換性を保つ変更（宣言の追加、説明の変更、`@deprecated` の追加）を 1 行ずつ出します。互換性のない変更があると終了コードは 1 です。型検査はせず、宣言の署名だけを比べます。

```sh
# 前の版のチェックアウトと、今のパッケージを比べる
tsuzuri api-diff ../geometry-1.2.0 .
tsuzuri api-diff ../geometry-1.2.0 . --json
```

次は、`Shapes.tz` だけを持つ 2 つのディレクトリを比べたときの出力です（`Tsuzuri.toml` のない入力は、名前空間のない名前で出ます）。

```text
breaking: added field Shapes.Point.z: `z: f64` (constructions of the record without it stop compiling)
breaking: changed def Shapes.area: `def area :: f64 -> f64` => `def area :: f64 -> f32`
non-breaking: added def Shapes.double: `def double :: i64 -> i64`
summary: 2 breaking, 1 non-breaking
```

`tsuzuri publish` は `api-diff` を呼びません。公開の前に実行してください。出力とその制限は [コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md#api-diff) にあります。

### C ヘッダーからの extern 生成 (`tsuzuri bindgen`)

C のヘッダーを Clang で解析し、ヘッダー自身の関数・enum の定数・整数の `#define`・struct・typedef から、リンク名付きの `extern "symbol" def`・`const`・`record`・`extern type`・`type` を持つモジュールを生成します。不透明な struct のポインターはハンドル、関数ポインターの引数は静的コールバックになり、`--buffer 関数:ポインター:長さ` で指定した組は `ref [T]`、`--consume 関数:引数` で指定したハンドルはムーブになります。ABI の一致を確かめられない宣言（可変長引数、注釈のないポインター、値渡しの struct、大域変数など）は、理由付きの `// skipped` 行と警告 `W2002` になります。出力は決定的で、`// Generated by tsuzuri bindgen. Do not edit.` で始まるファイルだけを上書きします。

```sh
tsuzuri bindgen vendor/sample.h -o Sample.tz --include-dir vendor --buffer sample_sum:values:count
```

### デバッグ情報と出力仕様

- **DWARF デバッグ情報**: `build` や `run` に `-g`（`--debug-info`）を付与することで、関数・行番号・変数・型の DWARF 情報を埋め込めます。macOS では `.dwarf` ファイルが生成され、LLDB などのデバッガでシンボルを解決可能です。呼び出し履歴の関数名は `Main.show` のような Tsuzuri の名前で、ステップ実行はランタイムとコンパイラーが生成した補助関数に入りません。LLDB に `scripts/lldb/tsuzuri_lldb.py`（配布物では `share/lldb/tsuzuri_lldb.py`）を `command script import` で読み込むと、文字列・配列・リスト・union・`Map` などを Tsuzuri の値として表示します。VS Code 拡張は自動で読み込みます。Windows で MSVC のリンカーを使う場合は CodeView も出し、実行ファイルの隣に PDB を書いて natvis の表示を埋め込みます（[デバッグ](_tsuzuri/language-reference/compiler/debugging.md)）。
- **実行時トラップ報告**: 配布ビルドでは `--trap-info` を指定することで、トラップ発生時の正確なソース位置情報を出力に含めることができます。WASM では `.trap.json` 表との連動に対応しています。
- **コンソールの出力とフォーマット**: トップレベルの結果式が評価された場合、その型が `Display` を実装していれば文字列表現が標準出力に表示されます。浮動小数点数は最短で往復可能な十進表現で出力され、ネイティブと WASM で完全に一致します。

---

## 環境構築とビルド

### 必要要件

- **Rust**: 1.85 以降（浮動小数点リテラルの厳密な丸めに `rustc_apfloat` を使用）
- **LLVM / Clang**: 17 以降（WebAssembly のリンクには `wasm-ld`（LLD）も必要）
- **Node.js**: 20 以降（WASM テスト・検証用）
- **Python**: 3.9 以降（参照実装テスト・デスクトップ検証用）

### 配布パッケージの利用

ローカルに LLVM / Clang の完全な開発環境を構築せずに利用したい場合は、コンパイラ本体・Clang・LLD・SDK を同梱したプラットフォーム別配布パッケージ（`tsuzuri-<version>-<host>.tar.gz`）を利用できます。詳細は [配布物を使う](_tsuzuri/language-reference/compiler/usage.md#配布物を使う) を参照してください。

### プラットフォーム別のセットアップ

#### macOS (Homebrew)

```sh
brew install llvm lld
export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"
cargo build --release
```

#### Ubuntu / Debian (apt)

```sh
sudo apt-get update
sudo apt-get install clang lld
cargo build --release
```

#### Windows (MSVC)

LLVM の `clang`、Windows SDK、および Visual Studio Build Tools（MSVC / C++ ツールセット）を用意し、Developer PowerShell 上でビルドを行います。Git Bash では、Git 付属の `link` が MSVC の `link.exe` より先に見つかってリンクが失敗することがあるので、PowerShell を使います。リポジトリは `git clone -c core.autocrlf=false` で取得してください（既定の `core.autocrlf=true` は、fixture とランタイムの IR を CRLF にします）。
コンパイラは `--target=x86_64-pc-windows-msvc` を指定してネイティブ実行ファイルを生成します。

```powershell
cargo build --release
```

`clang` や `wasm-ld` のパスは環境変数 `TSUZURI_CLANG`、`TSUZURI_WASM_LD` で明示的に指定することも可能です。`--emit bitcode` が C ランタイムを IR のまま 1 つにまとめる `llvm-link` は、`TSUZURI_LLVM_LINK` で指定します（なければ、配布物の `bin/`、Clang と同じディレクトリ、PATH の順で探します）。`TSUZURI_LINKER=lld` は、ネイティブ実行ファイルと共有ライブラリの最終リンクに Clang へ `-fuse-ld=lld` を渡す opt-in で、未設定と `default` は従来どおり Clang の既定のリンカーです（[option.md](./_tsuzuri/language-reference/compiler/option.md)）。`--profile-use` にディレクトリを渡すときに `.profraw` を merge する `llvm-profdata` は `TSUZURI_LLVM_PROFDATA` で指定します（未設定なら `TSUZURI_CLANG` の Clang の隣、その toolchain の順で探し、PATH は探しません）。

x64 の Windows は、CI（`.github/workflows/windows.yml`、`windows-2022`。main へのマージのたびと、`ci:full` ラベルを付けた PR で動く）が LLVM 21.1.8、Visual Studio 2022 の MSVC 14.44、Windows SDK 10.0.26100 の組で、fmt・clippy・release build・`cargo test`・`tests/windows.mjs`・`tests/cache.mjs`・`tests/examples.mjs`・`tests/wasm_threads.mjs` を実行して確かめています。Windows ARM64 と MinGW のホストの実行は確かめていません。Windows の共有ライブラリ（`--emit shared`）と OS API は未対応です（[G10](_features/_completed/G10-windows.md)）。

#### クロスコンパイル

`--target TRIPLE` でビルド機とは別の OS や CPU 向けのオブジェクト・実行ファイルを作れます。実行ファイルのリンクには、その target の C ヘッダー・C ライブラリ・リンカーが要ります。配布パッケージの同梱ツールチェーン（Zig）は Linux（glibc・musl）・RISC-V Linux・MinGW の Windows・FreeBSD・macOS の 12 個をリンクでき、ほかは環境変数 `TSUZURI_SYSROOT` に SDK か sysroot を渡します。ビルド機で動かせない target の `run`・`test` は、`TSUZURI_RUNNER` に実行器（qemu-user など）を指定したときだけ動きます。何かを取得したり導入したりはしません。

```sh
tsuzuri build app --target aarch64-unknown-linux-musl -o app-linux-arm64
TSUZURI_RUNNER=qemu-aarch64 tsuzuri run app --target aarch64-unknown-linux-musl
```

target の一覧、CI が確かめている範囲（tier）、制限は [対応プラットフォームとクロスコンパイル](_tsuzuri/language-reference/compiler/platforms.md) にあります。32-bit の native target は受け付けません。

---

## CLI リファレンス

### コマンド書式

```text
tsuzuri check source.tz|source.tt|source.tc|directory [--json]
tsuzuri [build] source.tz|source.tt|source.tc|directory [options]
tsuzuri run Main.tz|directory [-O0|-O1|-O2|-O3|-Os|-Oz] [--cpu generic|native] [--target native|TRIPLE] [--json]
tsuzuri repl [-O0|-O1|-O2|-O3] [--cpu generic|native] [--no-cache] [--timeout SECONDS]
tsuzuri script [-O0|-O1|-O2|-O3] [--cpu generic|native] [--no-cache] [--json] FILE [arguments...]
tsuzuri test source.tz|directory [options]
tsuzuri bench source.tz|directory [--list] [--filter TEXT] [--index N] [--json] [--samples N] [-O0..-O3]
tsuzuri watch check|build|test source.tz|directory [options of that command]
tsuzuri serve directory [--idle-timeout SECONDS]
tsuzuri serve --status directory | --stop directory
tsuzuri check|build ... [--no-server]
tsuzuri fmt source.tz|directory [--check] [--json]
tsuzuri doc source.tz|source.tt|source.tc|directory -o outdir [--json]
tsuzuri new directory [--namespace NAME]
tsuzuri fetch directory [--json]
tsuzuri publish directory --git URL --rev COMMIT [--json]
tsuzuri api-diff old new [--json]
tsuzuri bindgen header.h -o Module.tz [--include-dir DIR]... [--buffer F:P:L]... [--consume F:P]... [--json]
tsuzuri lsp
```

### 主要オプション一覧

| オプション | 説明 |
| --- | --- |
| `-o`, `--output PATH` | 出力先パスを指定します（親ディレクトリは自動作成されます）。 |
| `--target native\|wasm32\|wasm64\|TRIPLE` | ターゲット環境を指定します（既定: `native`。`wasm64` は 64-bit 線形メモリ。`TRIPLE` は `aarch64-apple-darwin` や `x86_64-unknown-linux-musl` など 22 個の native の名前で、`tsuzuri --help` に一覧が出ます。32-bit は対象外）。ビルド機で動かせない `TRIPLE` の `run`・`test` は、`TSUZURI_RUNNER` に実行器を指定したときだけ動きます。 |
| `--emit exe\|object\|llvm\|header\|wasm\|wgsl\|wgsl-relaxed\|spirv\|spirv-relaxed\|shared\|bitcode\|bindings-js\|bindings-cs\|bindings-py\|bindings-cpp` | 出力成果物の種類（既定: native は `exe`、WASM は `wasm`）。`wgsl` は厳密な `i32`／`i32u` の GPU カーネル、`wgsl-relaxed` は緩い `f32` を含むカーネルの WGSL、`spirv` は Vulkan 用の厳密なカーネル（`i32`・`i32u`・`i64`・`i64u`・`f32`）、`spirv-relaxed` は緩い `f32` のカーネルの SPIR-V で、どれも `-O`・`--target`・`--cpu` と併用できません。`bindings-js` は `--target wasm32` で JavaScript のグルー `<name>.mjs` と TypeScript 宣言 `<name>.d.mts` を出します（`--wasm-feature threads` でスレッドプール版）。`shared` は native の共有ライブラリ、`bitcode` は C・C++ のホストが `-flto=thin` で小さな公開関数を inline できる ThinLTO の LLVM bitcode（native と wasm32。同じ Clang でリンクする。C ランタイムを使うプログラムには `llvm-link` が要る）、`bindings-cs`／`bindings-py`／`bindings-cpp` はそれを呼ぶ C#／Python／C++ のバインディングです。 |
| `-O0` ～ `-O3` | 最適化レベル（既定: `-O3`。高速化のために精度を損なう fast-math などは使用しません）。 |
| `-Os`, `-Oz` | サイズ重視の最適化（`build` と `run` だけ。`test`・`bench`・`repl`・`script` は `-O0`〜`-O3` だけ）。生成する LLVM IR は `-O3` と同じで（native の関数値の inline 属性 `alwaysinline` だけは、`-O0`・`-Os`・`-Oz` で外れます）、Clang に渡すレベルだけが変わります。意味・トラップ・`-g` は変わりません。 |
| `--cpu generic\|native` | CPU 命令セットの特化（既定: `generic`。`native` はビルド機の命令セットとスケジューリングに最適化）。 |
| `--deny-warnings` | 警告が存在する場合にコンパイルを失敗させ、コード生成や実行を行わずに停止します。 |
| `--trap-info` | 配布用ビルドにおいて、実行時トラップの正確なソース位置情報を保持します。 |
| `-g`, `--debug-info` | DWARF デバッグ情報を付与します。`test` では `--index N -g -o PATH` で 1 件のテストをデバッグ用のランナーとしてビルドします（実行しません）。 |
| `--wasm-max-memory SIZE` | WASM の最大線形メモリサイズ（既定: 16MiB、例: `256MiB`）。 |
| `--wasm-stack-size SIZE` | WASM のスタックサイズ（既定: 1MiB、例: `4MiB`）。 |
| `--coverage PATH` | `tsuzuri test` で、成功したテストの行と関数のカバレッジを lcov 形式で `PATH` に書きます（ネイティブだけ）。 |
| `--samples N` | `tsuzuri bench` の標本数（1〜1000、既定 11）。 |
| `--seed N` | `tsuzuri test` のプロパティテスト（`Gen.for_all`）の seed（既定は固定の `11400714819323198485`）。 |
| `--wasm-host wasi` | wasm32 において、標準入出力および OS API を WASI preview1 のインポートへ接続します。 |
| `--wasm-feature simd128\|threads\|jspi\|webgpu\|net` | WebAssembly の追加機能（128-bit SIMD、Worker スレッド分散、Async の JavaScript Promise Integration、`Gpu.request Gpu.WebGpu` 用の WebGPU import、`Net` のソケットを Node.js 向けのグルーで実装する `net`。`net` には `jspi` が必要）を有効化します。 |
| `--allocator system\|host\|counting\|small` | ヒープ確保の行き先（既定: `system`）。`host` はホストが定義する `tsuzuri_host_alloc`・`tsuzuri_host_free`・`tsuzuri_host_realloc` を呼び、`counting` は確保の数を `tsuzuri_alloc_stats` で返します（object・LLVM IR・header・WASM 出力のみ）。`small`（PM05）は native の 64-bit のサイズ区分 allocator で、512 バイト以下を 16 バイト刻みの 32 区分から返し、それより大きい確保は C ライブラリに任せます（実行ファイル・object・共有ライブラリ・LLVM IR・header。WASM・`--trap-mode return`・`--freestanding` とは排他）。 |
| `--freestanding` | C ライブラリに依存しない native の object・LLVM IR・header を出力します（`--allocator host` が必須）。 |
| `--backend llvm\|fast` | コード生成器（既定: `llvm`）。`fast` は `-O0` の `build`・`run`・`test` で、Tsuzuri の出した LLVM IR を Clang に渡さずにコンパイラ自身が機械語にします（native は Cranelift、wasm32 は WebAssembly の命令を直接書きます。`run` と `test` は native だけです）。`cargo build --release --features fast-backend` でビルドした `tsuzuri` が要り、`-O1` 以上、`--cpu native`、`--emit exe\|object`（wasm32 は `wasm\|object`）以外、`--profile-*`、`--target TRIPLE`、Windows のホストは `E2000` です（[コード生成器](_tsuzuri/language-reference/compiler/option.md#コード生成器)）。 |
| `--strip` | native の実行ファイルのリンクでローカルシンボルを除きます（`build` の `--emit exe` だけ。`-g`・WASM とは組み合わせられず、Windows のリンクは変わりません）。 |
| `--wasm-opt` | wasm32 の `.wasm` のリンク後に Binaryen の `wasm-opt -Oz` を実行します（`build` の `--emit wasm` だけ。`-g`・threads・wasm64 とは組み合わせられません）。Binaryen は同梱せず、`--wasm-opt` を付けたときだけ使います。見つからない・失敗したときは `E2002` です。場所は環境変数 `TSUZURI_WASM_OPT`（この変数だけでは何も実行しません）。 |
| `--profile-generate DIR` / `--profile-use PATH` | プロファイル誘導最適化（`build` と `run`）。`--profile-generate` は、native の実行ファイルとオブジェクトを計装し、正常に終了したときに LLVM の raw profile（`.profraw`）を `DIR` に書きます。`--profile-use` は、merge 済みの `.profdata` か `.profraw` のディレクトリ（`llvm-profdata` で merge）で最適化します。wasm32 の module も使え、その host は module の隣の `app.wasm.profile.mjs` の `writeProfile(instance)` で書きます。LLVM の Clang が要り（同梱の `tsuzuri-clang` は `E2002`）、Windows と別の機械の `--target` は `E2000` です。速くなる種目も遅くなる種目もあります（[詳細](./_tsuzuri/language-reference/compiler/option.md#プロファイル誘導最適化)）。 |
| `--no-cache` | `build`・`run`・`script` で、ビルド成果物キャッシュと構文解析の結果のキャッシュ（frontend cache）の読み書きをやめます。`repl` ではビルド成果物キャッシュをやめます。 |
| `--json` | 診断情報やテスト結果を 1 行 1 JSON オブジェクト形式で標準エラー出力へ返します。 |

### 入力とエラー報告の仕様

- **入力の解決**: ファイルまたはディレクトリを 1 つ指定します。ディレクトリを指定した場合は、直下の `Main.tz` が自動的にエントリーポイントとして選ばれます。ファイル指定時はその親ディレクトリをルートとし、配下の全 `.tz`・`.tt`・`.tc` を相対パス順に再帰的に探索して読み込みます。
- **一括エラー報告**: コンパイラは独立した複数の型エラーや構文エラーを収集し、ファイル名およびソース位置順にまとめて報告します（最大 50 件まで表示、残りは件数のみ通知）。二次エラーは抑制され、エラーが存在する限りコード生成や実行は行われません。
- **ビルドキャッシュ**: ビルドおよび実行時のアーティファクトキャッシュは既定で有効です。ソース、コンパイラ、ツールチェイン、設定内容の SHA-256 ハッシュをキーとして管理し、変更のないモジュールの再コンパイルを回避します。キャッシュ保存先は環境変数 `TSUZURI_CACHE_DIR` でカスタマイズでき、`--no-cache` で無効化できます。同じ保存先の `packages/` には `tsuzuri fetch` が取得した git と registry のパッケージが置かれ、キャッシュの掃除の対象外です。同梱ツールチェーンの識別（配布物の全ファイルの SHA-256 など）は最初のビルドで計算して `bundle/` に控え、配布物の署名が変わるまで読み直しません（信頼の範囲は `_tsuzuri/language-reference/compiler/usage.md` の「ランタイム object」）。
- **frontend cache**: `check`・`build`・`run`・`script`・`test`・`bench`・`doc` は、構文解析の結果を同じ保存先の `frontend/` にプロジェクトごとに保存し、内容の変わらないソースを構文解析し直しません。出力（IR、診断とその順序、警告、終了コード）はキャッシュの有無で変わりません。`--no-cache` は `build`・`run`・`script`・`repl` だけで受け付け、ほかのコマンドでは `TSUZURI_CACHE_DIR=`（空）にすると両方のキャッシュを使いません。
- **codegen unit（PB07、opt-in）**: 環境変数 `TSUZURI_CODEGEN_UNITS=auto|split` を選ぶと、`-O0` の native の実行ファイルと `--emit object`、wasm32 の `--emit wasm`（`--trap-info`・`-g` を含む）は、生成した IR を決定的な unit に分け、unit ごとの object を同じ保存先の `units/` に内容で保存し、変わった unit だけを並列にコンパイルして 1 回でリンクします。1 つのモジュールの本体の編集で再コンパイルされるのは、そのモジュールの unit だけで、大きなプログラムでは編集後のビルドが短くなります。初回のビルドは長くなるので、既定は `single`（分けない。未設定と空も同じ）で、`auto` は IR が 16 MB 以上の対象のビルドだけ、`split` は対象のビルドをすべて分けます。ほかの値は `E2000` です。`-O1` 以上、`--backend fast`、`--emit shared`・`bitcode`・`llvm`、PGO、Windows のホストなどは、どの値でも従来どおり 1 つのモジュールです。出力の意味と公開 symbol は変わりません（[詳細](./_tsuzuri/language-reference/compiler/option.md#コード生成の単位codegen-unit)、[測定](./docs/benchmarks.md#pb07-codegen-unit-による増分ビルド)）。
- **整数の範囲証明の旗**: 整数の `+ - *` は常に型の幅で折り返します。`TSUZURI_RANGE_FLAGS` は、コンパイラが範囲を証明できた演算に LLVM の `nsw`／`nuw` を書くかを選びます。`off`（既定）は何も書かず、`proven` は証明のある命令に書き、`audit` は旗の代わりに、証明が誤っていれば trap する検査を書く試験用です。ほかの値は `E2000`、結果の値は変わりません（[変数の表](./_tsuzuri/language-reference/compiler/option.md#環境変数)）。
- **コンパクト文字列（opt-in）**: `TSUZURI_STRINGS=compact` は、全コード単位が 0xFF 以下（ASCII と Latin-1）の `string` を 1 単位 1 byte で保存し、そうでない文字列は UTF-16 のままにします（長さの語の bit 62 が印で、`Maybe<string>` の niche は変わりません）。ASCII・Latin-1 の文字列の確保量は約半分になり、native と `--wasm-feature simd128` の WebAssembly では Latin-1 の走査・比較・変換に 128-bit vector の kernel を使います。長さ・添字・順序・hash・表示・host との ABI（常に UTF-16）は変わりません。既定の `utf16` は PM03 より前と byte 単位で同じ IR を出し、ほかの値は `E2000` です。runtime の IR・小さな WebAssembly・ビルド時間が増え、host へ短い文字列を返す呼び出しが遅くなるので、既定は `utf16` のままです（[測定と判断](./docs/benchmarks.md#pm03-コンパクト文字列)、[使い方](./_tsuzuri/language-reference/compiler/option.md#コンパクト文字列の旗)）。
- **スレッド数**: コンパイラ自身が使うスレッド数の上限は環境変数 `TSUZURI_THREADS`（1 から 32 の整数）で決まります。未設定または空のとき、CLI は使える core 数（最大 8）、ライブラリ API は 1 です。`1` は呼び出したスレッドだけで解析し、範囲外や整数でない値は `E2000`（終了コード 2）です。並列に実行する段は、現在はソースの構文解析（`parse`。frontend cache の照合を含む）、関数ごとの本体の型検査と仕上げ、所有権の検査（`check` の一部と `ownership`）で、ソースや関数が少ない入力は 1 スレッドのままです。診断（順序・重複の除去・上限）、IR、終了コードはスレッド数で変わらず、キャッシュのキーにもスレッド数は入りません（[詳細](./_tsuzuri/language-reference/compiler/option.md#コンパイラのスレッド数)）。
- **段階別時間**: `TSUZURI_TIME_PASSES=1` のときだけ、終了時に stderr の最後へ時間の JSON を 1 行出します（`watch` は 1 回ごと、`serve` が引き受けた要求は 1 要求ごとに、その回の分だけです）。`emit` は target ごとの ABI・WASM host／runtime の IR 処理も含み、Clang・リンカーは `tool.*` に分けます。成果物・診断・終了コードは変わりません（[段の範囲](./_tsuzuri/language-reference/compiler/option.md#コンパイルの段階別時間)）。

---

## 検証と性能測定

Tsuzuri は言語仕様の正しさとパフォーマンスを保証するため、網羅的なテストスイートとマルチ言語ベンチマークを備えています。

### 何を回すか

変更に見合う範囲だけを回します。作業中は T0 と T1 を繰り返し、完了を報告する前に T2 を 1 回回し、T3 は CI に任せます。

| 段 | 回すもの | 目安 |
| --- | --- | --- |
| T0（編集のたび） | `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、下の表の「Rust テスト」のうち該当するもの | 数秒 |
| T1（手順の 1 段ごと） | `cargo nextest run --locked`（導入は `cargo install --locked cargo-nextest`。無ければ `cargo test --locked`） | 2〜6 分 |
| T2（完了の報告の前） | T1、下の表の「E2E」を dev バイナリ（`cargo build` が作る `target/debug/tsuzuri`）で、変更した `_tsuzuri/` のページの `node scripts/check-docs.mjs <ページ>`、`src/runtime/` を変えたときの `sh scripts/check-runtime-includes.sh` | 10〜20 分 |
| T3（CI） | release ビルド、全 suite、全 platform、fuzz、sanitizer、ベンチマーク。PR は `ci.yml`（`_tsuzuri/` を変えたら `reference.yml` も）、main へのマージ後と `ci:full` ラベルの PR は `targets.yml`・`windows.yml`・`vscode.yml` | CI |

- dev profile は `opt-level = 1`・行表だけのデバッグ情報です（[Cargo.toml](Cargo.toml)）。Rust のテストと dev バイナリは速く、Node の E2E も dev バイナリで足ります（時間は clang が決めるので release と同程度です。e2e・primitives・examples・control・tasks・computations・strings・json で確かめました）。release ビルドは、性能の主張、ベンチマーク、大きさや性能を見る検査に使います。lldb で Rust のコンパイラを追うときは `cargo build --profile debugging`（完全なデバッグ情報）を使います。
- 実測（Apple M1 Max、ほかの作業と並行）: `cargo test --locked` は opt-level 0 の 22 分 19 秒から、opt-level 1 で 6 分 26 秒（再ビルド込み）、`cargo nextest run --locked` で 2〜3 分です。変異 fuzz の 2 本（`tests/frontend.rs`・`tests/computations.rs`）は、opt-level 1 と、入力を変えずに解析をスレッドへ分ける変更で、binary 全体が 377 秒と 138 秒から 18 秒と 9 秒になりました。
- 複数のセッション（worktree）が重いコマンドを同時に走らせると、CPU を奪い合って全員が遅くなります。`node scripts/slot.mjs -- <コマンド>` は、共有のスロット（既定はコア数の 1/3、`TSUZURI_SLOTS` で変える）が空くまで待ってから実行します。
- ウイルス対策のリアルタイム保護は、ビルドとテストが作る大量の小さなファイルを検査して CPU を 1〜2 コア分使うことがあります。リポジトリ、各 worktree の `target/`、`$TMPDIR`、`~/.cargo` を除外すると軽くなります（Microsoft Defender for Endpoint の macOS: `mdatp exclusion folder add --path <パス>`。管理された端末では管理者の承認が要ります）。古い成果物は `cargo sweep --time 7`（`cargo install cargo-sweep`）で消せます。

#### 変更の種類別

`cargo test --locked <pattern>` は、一致するテストが 0 件でも成功します（`cargo nextest run` は 0 件を失敗にします）。**出力の `running N tests` の N が 0 でなく、期待した数であることを毎回見ます。** どの変更でも、次の回帰テストは通ります（上限や stack の大きさを上げて通してはいけません）。T1 に含まれます。

```sh
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

| 変更の種類 | Rust テスト（手本のファイル） | E2E（`node tests/<名前>.mjs target/debug/tsuzuri`） |
| --- | --- | --- |
| 字句・構文・予約語 | `tests/frontend.rs`、`tests/diagnostics.rs`、`src/parser.rs` の test module、`tests/formatter.rs`、`tests/editions.rs`（edition ごとの予約語・manifest の `edition`） | `examples.mjs`、`features.mjs`（関係する suite） |
| 型・型クラス・多相 | `tests/polymorphism.rs`、`tests/higher_kinds.rs`、`tests/deriving.rs`、`tests/union_types.rs` | `features.mjs typeclasses`・`deriving`・`higher_kinds` |
| 所有権・借用・region | `tests/types_ownership.rs`、`tests/borrowed_records.rs`、`tests/slices.rs` | `features.mjs borrowed_records`、`control.mjs` |
| 制御構文・パターン | `tests/control.rs`、`tests/match_exhaustiveness.rs` | `control.mjs`、`features.mjs` |
| 計算式・IO | `tests/computations.rs` | `computations.mjs`、`io.mjs` |
| コレクション・std | `tests/stdlib.rs`、`tests/map_set.rs`、`tests/vec.rs`、`tests/iteration_protocol.rs` | `features.mjs map_set`・`iteration_protocol`・`vec` |
| 文字列・文字・表示と解析 | `tests/strings.rs`、`tests/chars.rs`、`tests/display_parse.rs` | `strings.mjs`、`display_parse.mjs` |
| 数値・数学 | `tests/integers.rs`、`tests/math.rs`、`tests/f16_hardware.rs`（f16 の `half` 経路。D10） | `numeric_casts.mjs`、`integer_intrinsics.mjs`、`math.mjs`（重い。Node 24）、`f16_hardware.mjs`（`TSUZURI_F16_EXPECT` でホストが通る経路を固定） |
| コンパイル時評価（`const`・`const def`・静的データ） | `tests/constants.rs` | `features.mjs constants`・`const_functions`・`const_statics` |
| 生成 IR・ランタイム・閉包 | 変更した機能の Rust テスト、IR の決定性の確認 | `primitives.mjs`、`e2e.mjs`、`features.mjs`（全 suite） |
| Task・並列・WASM threads・同期 | `tests/tasks.rs`、`tests/parallel.rs`、`tests/concurrency.rs`（`Atomic`・`Mutex`・`Channel`・`Task.scope`） | `tasks.mjs`、`wasm_threads.mjs`、`wasm_threads_stress.mjs`、`trap_return_sync.mjs`、`features.mjs concurrency`・`channel`、`TSUZURI_TSAN=1` の実行 |
| SIMD・CPU dispatch・GPU | `tests/simd.rs`、`tests/cpu_dispatch.rs`、`tests/gpu.rs`、`tests/gpu_spirv.rs` | `simd.mjs`、`wasm_simd.mjs`、`cpu_dispatch.mjs`、`gpu.mjs`、`gpu_runtime.mjs`、`gpu_vulkan.mjs` |
| 行列・テンソル | `tests/matrix.rs`、`tests/matrix_views.rs`、`tests/tensor.rs` | `features.mjs matrix`・`matrix_view`・`tensor`、`benchmarks/run-matrix.mjs` |
| ネットワーク | `tests/net_api.rs` | `net.mjs`、`net_wasm.mjs`（Node.js 24） |
| host ABI・import | `tests/host_abi.rs`、`tests/host_imports.rs` | `host_imports.mjs` |
| driver・CLI・cache | `src/main.rs` の test module、`tests/modules.rs`、`tests/targets.rs`（`--target TRIPLE`・クロスの引数・実行器。G15）、`tests/api_diff.rs`（`tsuzuri api-diff`。G19） | `cache.mjs`、`examples.mjs`、`targets.mjs`（triple ごとに executed／linked／object を表示。`TSUZURI_CROSS_LINK=1`、`TSUZURI_CROSS_EXECUTE=1`） |
| 診断・警告 | `tests/diagnostics.rs`、`tests/warnings.rs`（W1005 を含む） | — |
| LSP・docgen・debug | `tests/lsp.rs`、`tests/docs.rs`、`tests/debug_info.rs` | `lsp_sessions.mjs`、`docgen.mjs`、`debug_info.mjs` |
| 言語内テスト | `tests/test_runner.rs` | `tsuzuri test` を使う E2E |
| `_tsuzuri/` の文書 | — | `node scripts/check-docs.mjs <変更したページ>`（引数なしで全体） |

表にない組み合わせは、変更した関数名を `grep -rn "<関数名>" tests/` して、それを使うテストを全部実行します。

### テストスイートの実行

```sh
# 基本的なコードベース検証
sh scripts/check-runtime-includes.sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo nextest run --locked   # 速い。無ければ cargo test --locked。CI は RUST_MIN_STACK=4194304（4 MiB のテストスレッド）を付けて toolchain に依存しないようにしている。opt-level 1 の dev profile では、macOS arm64 は付けなくても全件通る。深さの上限のテストが stack の溢れで binary ごと落ちる host では付ける（PowerShell は先に $env:RUST_MIN_STACK='4194304'）
TSUZURI_THREADS=8 cargo nextest run --locked   # フロントエンドの並列（src/parallel.rs とその呼び出し）を変えたときだけ、全テストをこの経路で通す（診断と出力は同じでなければならない）。ライブラリの既定は 1 スレッド。ほかの変更では不要で、CI は tests/parallel_frontend.mjs で確かめる
TSUZURI_STRINGS=compact cargo nextest run --locked   # IR を出すテスト全体をコンパクト文字列で。IR の形そのものを固定するテストは、形式を自分で選ぶ（PM03）
cargo build --release --locked   # CI と同じバイナリ。内側のループは cargo build（target/debug/tsuzuri）で足りる

# 高速デバッグバックエンド（PB05。`--backend fast`）。feature なしのビルドと検証は上のとおりで、変わらない
cargo clippy --all-targets --features fast-backend -- -D warnings
RUST_MIN_STACK=4194304 cargo test --locked --features fast-backend --test fast_backend --test fast_backend_ops   # 翻訳器: layout を Clang と照合、E2002 の表、IR の翻訳と決定性、test の runner、約 780 の演算を Clang の object と native では bit 単位で、wasm32 では Node で（NaN の payload を除いて）比較
cargo build --release --locked --features fast-backend
node tests/fast_backend.mjs target/release/tsuzuri   # option の表（E2000）、hello・trap・--trap-info・run、thread 数の決定性、export する symbol、cache の鍵、test、-g の行表、旗なしの IR の不変
node tests/fast_backend.mjs programs target/release/tsuzuri   # examples と fixtures を llvm と fast の両方で動かして出力と終了状態を比べる（library は export する symbol）。FAST_DEBUG=1 で -g も付ける
node tests/fast_backend.mjs debug target/release/tsuzuri   # 同じ projects を -g で作り、fast の DWARF を llvm-dwarfdump --verify で検証し、関数ごとの行を llvm のものと比べる
node tests/fast_backend.mjs wasm target/release/tsuzuri   # wasm32: 同じ projects を llvm と fast で作り、fast の module が validate を通ること、import・export が同じことを確かめ、すべての export を同じ入力で Node から呼んで結果を比べる。FAST_WASM_ARGS="--allocator small" などで option を足せる
node tests/fast_backend.mjs parity e2e strings tasks features   # 既存の suite を、tsuzuri と clang の wrapper の下で fast に流す（wasm32 の build も）。翻訳した件数と fall back の理由を suite ごとに表示。FAST_FORCE=1 で最適化つきの build も -O0 の fast にする。clang しか使わない suite は wasm_heap=wasm32 のように引数を渡す
node benchmarks/run-backend.mjs --out target/perf/backend   # llvm と fast の build 時間・CPU 時間・メモリ（合成 200・1,000 モジュール。docs/benchmarks.md の PB05）。--target wasm32 で wasm32
node benchmarks/run-backend-kernels.mjs --out target/perf/kernels   # benchmarks/control の 16 種目を、llvm の -O0、fast の -O0、llvm の -O3 で作って実行時間を比べる（checksum が同じことも確かめる）。--target wasm32 で Node の中で動かす
TSUZURI_FAST_OPS_EXPORT=DIR TSUZURI_FAST_OPS_TARGET=x86_64-unknown-linux-gnu cargo test --locked --features fast-backend --test fast_backend_ops operations_agree_with_clang   # 比較の材料（host.c、IR、別の機械向けの object）を DIR に書いて止まる。Linux のコンテナで clang と比べる（x86-64 の Linux など、この機械で動かせない target の確認）

# 統合 E2E テスト・言語機能テスト（target/release/tsuzuri は、内側のループでは target/debug/tsuzuri に読み替えてよい）
node tests/slot.mjs   # scripts/slot.mjs：スロットの数を超えて同時に走らないこと、死んだ持ち主のスロットの引き継ぎ、終了コードの引き渡し（コンパイラは不要）
node tests/e2e.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/numeric_casts.mjs target/release/tsuzuri   # 全ビット幅の整数・符号と f32／f64 の間の型変換を BigInt の参照と照合
node tests/integer_intrinsics.mjs target/release/tsuzuri
node tests/math.mjs target/release/tsuzuri   # 重い。Node 24
npx --yes --package=node@24 node tests/wasm64.mjs target/release/tsuzuri   # wasm64（memory64）。Node 24
node tests/prebuilt_runtime.mjs target/release/tsuzuri   # runtime object の cold／warm・破損 fallback・no-cache・native／WASM O0/O3
node tests/size.mjs target/release/tsuzuri   # -Os／-Oz の出力一致・水準ごとの IR（native の関数値の inline 属性だけが違う）・キャッシュ・WASM の大きさ、埋め込み形式のリンク時 GC、--strip（nm・codesign・トラップ表）、整数の表示・書式指定・Parse（全型の境界値・参照との照合・IR・WASM）、--wasm-opt（偽の wasm-opt での呼び出し・キャッシュ・E2002。TSUZURI_TEST_WASM_OPT に本物があれば実行）とオプション誤り（TSUZURI_TEST_SECTION=名前の一部 で節を選べる）
node tests/integer_format.mjs   # 整数の表示・書式指定・Parse（integer.ll）を Python の独立した参照、8／16 bit の全値、numeric.ll の tz_soft_* と照合（native／WASM、O0／O3／Os／Oz。Clang と Python が必要でコンパイラは不要）
node tests/llvm_handoff.mjs target/release/tsuzuri   # 実行ファイルの到達する関数だけを Clang へ渡す（PB03 D1）。vtable・closure・export・trap 位置・型定義・剪定で通るようになるビルド・`run` の trap 計装
node tests/handoff_tools.mjs target/release/tsuzuri   # WASM threads／WASI／-g の C runtime の cache と失敗の hint、TSUZURI_LINKER（PB03）
node tests/bundle_identity.mjs target/release/tsuzuri   # 同梱ツールチェーンの識別を控える memo の無効化・上書き・cache 保護（PB03 WP-F）
node tests/pgo.mjs target/release/tsuzuri   # PGO（PR07）: 計装した実行ファイル・object・run の raw profile、--profile-use の directory と merge 済みの file、cache の鍵、古い profile の W2001、llvm-profdata の探索、wasm32 の writer（counts を native と照合）。LLVM の Clang と llvm-profdata が無いと skip（LLVM_PREFIX か TSUZURI_PGO_STRICT=1 では失敗）
node tests/display_parse.mjs target/release/tsuzuri   # Python の独立参照と native／WASM O0/O3 の表示・解析を照合
node tests/strings.mjs target/release/tsuzuri
node tests/utf_simd.mjs --quick   # scalar／SIMD の参照・境界・guard page・命令生成、Latin-1 の kernel（PM03）と生成物の再生成の一致。--asan／--exhaustive は拡張検証
node tests/compact_strings.mjs target/release/tsuzuri --quick   # コンパクト文字列（PM03）。fixture を TSUZURI_STRINGS=utf16／compact × native／wasm32（simd128 も）× O0／O3 で作り、すべての出力が同じで、呼び出しごとに確保が残らないことを確かめる。host の import へ渡す UTF-16 の複製が、あとの引数の例外のあとにも残らないことも確かめる
node tests/compact_strings_raw.mjs target/release/tsuzuri --quick   # compact の string の runtime を生の長さの語（bit 62）で直接呼び、JavaScript の参照と照合する（wasm32 の scalar／simd128 × O0／O3）。vector 命令の有無の検査に llvm-objdump が要る（TSUZURI_OBJDUMP。無いと失敗し、CI でなければ TSUZURI_SKIP_OBJDUMP=1 でその検査だけを省ける）
node tests/compact_strings_modes.mjs target/release/tsuzuri quick compact utf16   # 既存の文字列に関わる node の suite を TSUZURI_STRINGS=compact と utf16 の両方で実行する（full は到達しうる suite すべて）
python3 tests/lldb_strings.py   # LLDB の formatter（scripts/lldb/tsuzuri_lldb.py）が UTF-16 と Latin-1 の string を読むことを、LLDB のセッションなしで確かめる
python3 tests/lldb_lists.py   # PM04: 同じ formatter の `ListProvider` が、`head` から `next` の連鎖をたどって各要素の address を返すこと（先頭側の空き slot、短い length の view、alignment 16 の要素の slot の offset 32）、chunk を二分探索で見つけて読んだ chunk を覚えていること、壊れたメモリ（範囲外の head、循環する `next`、合わない header、不正な長さ）で例外にならないことを、LLDB のセッションなしで確かめる（python3 だけで動く）
node tests/tasks.mjs target/release/tsuzuri   # Task・Channel・Mutex の native／WASM と、pool の C 検査（task_runtime.c の lock・signal の数と、最初の添字・worker の完了の数え方の順序、task_stress.c の複数 host の stress）。TSUZURI_TSAN=1・TSUZURI_ASAN=1 で sanitizer も
node tests/async.mjs target/release/tsuzuri   # JSPI の検証には Node.js 24 以降
node tests/features.mjs target/release/tsuzuri
ASAN_OPTIONS=detect_stack_use_after_return=1 TSUZURI_ASAN=1 node tests/features.mjs target/release/tsuzuri storage   # 連結のスタック領域・捕捉環境・移送・再利用
node tests/features.mjs target/release/tsuzuri list_chunks   # リストの chunk（PM04）。chunk の境界（i64 は 1〜4093 個、16 bytes の slot は 1〜2046 個、unit は 8〜32744 個）の前後で、cons・添字（範囲外は trap）・tail・パターン・比較・hash・display・複製・配列との変換・スタックのリテラルの移送・user Drop を持つ要素の drop の順序・heap のリテラルの途中の例外で何も漏れないこと（host が live == 0 を確かめる）を、native／WASM の -O0／-O3 で照合する。TSUZURI_ASAN=1・TSUZURI_TEST_ALLOCATOR=small・TSUZURI_RANGE_FLAGS=audit でも回す
node tests/benchmark_list_memory.mjs target/release/tsuzuri   # PM04 の list-memory benchmark の参照式・引数と、quick の実行（checksum・確保の回数・wasm32 の heap）
TSUZURI_TEST_ALLOCATOR=small node tests/features.mjs target/release/tsuzuri   # 同じスイートを --allocator small で。追跡する malloc の代わりに tz_small_stats（切り出したブロックが全部キャッシュか中央に戻ったか）で解放漏れを検査する（PM05）
node tests/small_allocator.mjs target/release/tsuzuri   # --allocator small（PM05）。heap-small.c の C のユニットテスト（通常・ASan+UBSan・TSan）、拒否、IR、LLVM が確保と解放の組を取り除くこと、object と C のホストでの解放漏れ・複数スレッド・複数オブジェクト・共有ライブラリの検査。--unit は compiler なしでユニットテストだけ。TSUZURI_NO_SANITIZERS=1 で ASan+UBSan と TSan のユニットテストを省く（空のプログラムが起動しない clang、Homebrew の LLVM 21 の macOS でも、自動で省いて理由を出す。通常と -O3 は必ず走る。macOS の CI はこれを付ける）
node tests/local_recursion_fuzz.mjs target/release/tsuzuri 100   # ローカルな let rec の差分テスト（B09）。乱数で作った入れ子のグループ・クロージャ・借用の捕捉のプログラムを、独立した JS の評価器と照合する。引数は [コンパイラ] [件数] [最初の seed] [深さ]。リフト（src/closures_rec.rs）を変えたときに回す
node tests/last_use_fuzz.mjs target/release/tsuzuri 300 1000 4   # 最後の使用の move（A17 Phase 1・2）を独立した AST 評価器と native O0/O3 で照合。引数は [コンパイラ] [件数] [最初の seed] [深さ]
node tests/integer_ranges_fuzz.mjs target/release/tsuzuri 300 1 4   # 整数の範囲証明（PR02）の差分テスト。型の端に定数を合わせた乱数のプログラムを、独立した BigInt の評価器と、audit（-O0）・proven・off（-O3）の wasm32 で照合する。TSUZURI_FUZZ_NATIVE=1 で native も。引数は [コンパイラ] [件数] [最初の seed] [深さ]。型付き IR や emitter を変えた後に回す
node tests/range_flags_audit.mjs target/release/tsuzuri quick   # 既存の node の suite を TSUZURI_RANGE_FLAGS=audit と proven で実行し、fuzz も回す（quick は整数の suite だけで、負荷のある機械で 20 分ほど。full はすべての suite で 1 時間以上）。Rust のテストは TSUZURI_RANGE_FLAGS=audit cargo test --locked
node benchmarks/run-range-flags.mjs target/release/tsuzuri   # 旗が asm を変えるか: 12 の corpus を off と proven で arm64・x86_64（-O1〜-O3）と wasm32 に出して比べる（PR02 Gate A。asm はコメントだけを除いて行の順序のまま、即値も含めて比べる）
node tests/json.mjs target/release/tsuzuri
node tests/computations.mjs target/release/tsuzuri
node tests/control.mjs target/release/tsuzuri
node tests/lsp_sessions.mjs target/release/tsuzuri
node tests/io.mjs target/release/tsuzuri
node tests/repl.mjs target/release/tsuzuri
node tests/script.mjs target/release/tsuzuri
node tests/os.mjs target/release/tsuzuri
node tests/net.mjs target/release/tsuzuri   # アドレス・名前解決・ソケット・非同期・監視スレッド・確保の追跡。127.0.0.1 の ephemeral port だけを使い、どのディレクトリからでも動く（CI は vsc/ から）。TSUZURI_TSAN=1 で ThreadSanitizer も。TSUZURI_NO_SANITIZERS=1 で ASan／UBSan を使わない（Homebrew の LLVM 21 は macOS でサニタイザー付きの実行ファイルが起動で止まるため、macOS の CI はこれを付ける）
node tests/net_wasm.mjs target/release/tsuzuri   # wasm32 のソケット（--wasm-feature jspi --wasm-feature net）を Node.js 24 の JSPI で。Node.js 24 未満なら省く。2 GiB を超えるアドレスの case はメモリ 8 GiB 以上で動き、TSUZURI_NET_WASM_HIGH=0 で省ける
node tests/targets.mjs target/release/tsuzuri   # --target TRIPLE の 22 個を、実行・リンク・オブジェクトのどこまで確かめたか 1 行ずつ出す。TSUZURI_CROSS_LINK=1 と TSUZURI_CROSS_CLANG=<同梱の tsuzuri-clang> で、同梱ツールチェーンのリンクも（無ければ失敗）。TSUZURI_CROSS_EXECUTE=1 で Linux の qemu-user による静的 musl の実行も。CI は .github/workflows/targets.yml
node tests/windows.mjs target/release/tsuzuri.exe   # Windows x64（MSVC）だけ。PowerShell で。native の -O0／-O3、名前を付けた x86_64-pc-windows-msvc の build・run・test、Win32 の task と失敗の注入、トラップの報告、UTF-8、hard link の拒否、既存出力の置換、object の ABI、WASM を確かめる。CI は .github/workflows/windows.yml（cache.mjs・examples.mjs・wasm_threads.mjs・cargo test も）
node tests/cpu_kernels.mjs target/release/tsuzuri
node tests/f16_hardware.mjs target/release/tsuzuri   # f16 の経路（ソフトウェア・F16C の変換・half 命令）を JS の参照実装とビット単位で照合し、実行した経路を表示する（D10）。--quick は件数を減らす。TSUZURI_F16_EXPECT=hardware|conversions|soft（既定の CPU と native を分けるなら generic=…,native=…。各値は `|` で複数許可できる）を付けると、期待した経路が実行されなかったときに失敗する。`.github/workflows/targets.yml` が macOS arm64・Intel Mac・Linux x86-64・Linux arm64 の runner でこのコマンドを実行し、Linux x86-64 の `native` は runner の機能に応じ `conversions` または `hardware` を許可し、現時点で `conversions` の実行を確認済み（AVX512-FP16 の `hardware` 実行は未確認）。実行した経路は参照と照合するが速度は測らない。Windows の実行も未確認。cargo test --locked の tests/f16_hardware.rs は clang で生成コードを確かめる
node tests/packages.mjs target/release/tsuzuri
node tests/bindgen.mjs target/release/tsuzuri
node tests/debug_info.mjs target/release/tsuzuri   # llvm-dwarfdump が必要（TSUZURI_DWARFDUMP で指定できる）
node tests/debugger.mjs target/release/tsuzuri   # lldb が必要（TSUZURI_LLDB で指定できる）
node tests/debugger_static.mjs target/release/tsuzuri   # PM02: lldb が必要（TSUZURI_LLDB で指定できる）。プロセスを起動せず、-g の実行ファイルの型を lldb に読ませて、関数値と Task（descriptor と environment の 2 メンバー）を formatter の is_function が見分けること、同じ名前のフィールドを持つ record（古い 4 欄の形、descriptor が整数や ref の record）は見分けないこと、descriptor が closure.descriptor（code・clone・drop）を指すことを確かめる
node tests/frontend_cache.mjs target/release/tsuzuri
node tests/parallel_frontend.mjs target/release/tsuzuri   # PB04: TSUZURI_THREADS が 1・2・8・未設定のどれでも、診断（順序・重複の除去・上限）、IR、終了コード、frontend cache の bytes が同じこと（examples・fixtures・合成 200／1,000 module・上限を超える誤り・入れ子の上限）。不正な値は E2000 で終了コード 2。TSUZURI_BASELINE=<PB04 より前の compiler> を付けるとその compiler とも比べる
node tests/watch.mjs target/release/tsuzuri   # PB06: tsuzuri watch の各回の出力が単独のコマンドと byte 単位で同じこと、同じ長さ・同じ更新時刻の書き換え・追加・削除・新しいディレクトリの検出、touch で実行しないこと、--json の状態行、build・test、コマンド名のない引数の E2000、差し替えたコンパイラで次の回を始めずに止まること（待っているあいだも。Windows は実行中のファイルを差し替えられないので除く）、TSUZURI_TIME_PASSES が回ごとの報告であること（Windows でも動く）
node tests/server.mjs target/release/tsuzuri   # PB06: tsuzuri serve（Unix だけ。Windows では serve が E2000 であることの確認）。サーバーが実行した check・build の出力と成果物が --no-server と同じこと（ソースの状態ごと、覚えたあとの編集を含む）、環境・作業ディレクトリ・コンパイラ・ルート・未対応のコマンド・protocol の断り、並行する要求、サーバーでないもの・残った socket・共有されたディレクトリでの黙った fallback、二重起動、実行中の status・--stop と去ったクライアントの要求の扱い（Clang を止めて確かめる）、idle・差し替えたコンパイラでの終了、TSUZURI_TIME_PASSES、読まれない標準エラーの pipe に何千もの断りを流しても答えと --stop が止まらないこと、serve の引数の E2000。ソケットのパス長のため TMPDIR は短くする（macOS で 103 byte まで）

# WebAssembly & GPU テスト
node tests/wasm_threads.mjs target/release/tsuzuri
node tests/wasm_threads_stress.mjs target/release/tsuzuri   # WASM threads の pool を 1〜7 ワーカーで、グループ・入れ子・Task.scope・pipeline の厳密な値と照合する（PR06）。引数は [コンパイラ] [rounds（既定 12）]。止まったら watchdog が終了させる
node tests/wasm_memory.mjs target/release/tsuzuri
node tests/wasm_heap.mjs   # WASM の heap だけを wasm32・wasm64 で検査（compiler は不要。wasm64 は Node.js 24 以降。PM05）。手で導いたアドレス、不変条件の検査、穴を複数残す確保（大きな一時配列を解放したあとに小さなブロックが生き残る列）、取っておく穴の上限（最近の最大の要求の 2 倍まで、1/64 ずつ忘れる）、生きているデータが一定の乱数列や、数十 KB のブロックの無作為な置き換えで heap が上限まで伸びないこと
node tests/bindings.mjs target/release/tsuzuri   # 生成グルー（TSUZURI_TSC で TypeScript の bin/tsc を指定できる）
node tests/bindings_threads.mjs target/release/tsuzuri   # スレッドのグルー（TSUZURI_BROWSER か TSUZURI_PLAYWRIGHT で実ブラウザも）
node tests/host_bindings.mjs target/release/tsuzuri   # 共有ライブラリと C#・Python・C++ のバインディング（dotnet が無ければ C# を飛ばす）
node tests/gpu.mjs target/release/tsuzuri   # TSUZURI_WEBGPU=1 と TSUZURI_WEBGPU_MODULE で実アダプタ（Dawn）も
node tests/gpu_runtime.mjs target/release/tsuzuri   # TSUZURI_WEBGPU=1 と TSUZURI_WEBGPU_LIBRARY（wgpu-native 29）で実機。WASM の JSPI は Node.js 24 以降。TSUZURI_SANITIZE=1 で C 側に ASan と UBSan
node tests/gpu_vulkan.mjs   # Vulkan のランタイム（合成ライブラリ、ASan・UBSan・TSan、実デバイス）。clang が要り、spirv-as が無ければ実デバイスの検査を飛ばす。TSUZURI_VULKAN_TEST_ICDS で他のドライバも
node tests/gpu_vulkan_language.mjs target/release/tsuzuri   # 言語としての Vulkan と Gpu.Auto（CPU 参照との照合、WebAssembly の確認）

# 言語リファレンス（_tsuzuri/）のリンクと例の検証（ページを指定すると、そのページだけ）
node scripts/check-docs.mjs
```

機能を追加・変更・修正・削除したときは、同じ変更で [言語リファレンス](_tsuzuri/language-reference/index.md) の関連ページも最新にします（手順は [_features/GUIDE.md §8](_features/GUIDE.md#8-ドキュメント更新規約)）。

### ベンチマーク測定

C/C++、Rust、C#、JavaScript との 38 種目にわたる詳細な性能比較スクリプトを提供しています。

PX01 → PX03 → PX02 の計測基盤を全実装フェーズまで実装しました。現行の frontend cache と成果物 cache を区別し、
従来の stdout を保った JSON Lines の記録、決定的なビルド入力、同じ仕事を行う実アプリ型の比較を提供します。
Go・Zig の比較を含む全実装フェーズの方針は [_perfs/README.md](_perfs/README.md#px01px03px02-の実装方針2026-10-13) にあります。
共有 CI に性能の合否閾値は置かず、未実測のホストでの性能は主張しません。
macOS の実行ファイルの決定性は、既存のリンカーが毎回変える UUID と署名だけを除いて検査します。
native Windows の標準 COFF object は Clang が書く `TimeDateStamp`（header の offset 4、4 bytes）だけを除き、
コード・データ・ほかの header とファイルサイズを厳密に比較します。
コンパイラの既定の出力は変更しません。
大規模な合成 WASM `-O3` には LLVM 21 の最適化時間の制限があるため、標準のビルド比較は 200 module まで、
1,000 module のその組は `--large-wasm` で明示します。native と WASM `-O0` の 1,000 module は標準で検査します。
大規模 native `-O0` の既存の Mach-O import-stub 順序の変動は warning と補助 manifest に残し、
ソースの参照値・IR・サイズの検査と区別します。

```sh
# 基本ベンチマークの実行
node benchmarks/run.mjs target/release/tsuzuri
node benchmarks/run-cpp.mjs target/release/tsuzuri
node benchmarks/run-control.mjs target/release/tsuzuri
node benchmarks/run-control.mjs target/release/tsuzuri --pgo   # PGO（PR07）: plain と PGO の host を交互に測り、variant pgo の行も記録する（C・C++ も同じ profile。--pgo-rust-llvm DIR で Rust も）
node benchmarks/run-trap-layout.mjs target/release/tsuzuri   # trap の block が hot path に置かれるかの asm の走査（PR07 D9）
node benchmarks/run-computations.mjs target/release/tsuzuri
node benchmarks/run-control.mjs target/release/tsuzuri --metrics target/perf/PX01-example
node benchmarks/metrics.mjs process target/release/tsuzuri --out target/perf/PX01-example
node benchmarks/metrics.mjs report target/perf/PX01-after --baseline target/perf/PX01-before
node benchmarks/run-build.mjs target/release/tsuzuri --out target/perf/PX03-example
node benchmarks/run-build.mjs target/release/tsuzuri --quick --languages tsuzuri   # ビルド比較の正しさだけ
node benchmarks/run-threads.mjs target/release/tsuzuri target/perf/PB04/s1000 check 11 --before <PB04 より前の compiler> --out target/perf/PB04-after before 1 2 4 8 unset   # TSUZURI_THREADS ごとの check／emit-llvm を交互に測る（PB04）。--cache cold|warm で frontend cache も
node benchmarks/run-apps.mjs target/release/tsuzuri --metrics target/perf/PX02-apps
node benchmarks/run-apps.mjs target/release/tsuzuri --allocator small   # 時間を測る object だけ --allocator small（run-control.mjs も同じ。追跡する IR は system のまま。--metrics の records は、Tsuzuri の時間と peak RSS の variant が small、確保の数（alloc_calls・alloc_bytes）の variant が system で、既定の run とは比較で対応しません。PM05）
node benchmarks/run-small.mjs target/release/tsuzuri --rounds 5 --rss-runs 9   # PX02 の app の --allocator system と small を同じコンパイラで交互に走らせ、CPU 時間と peak RSS を比べる（Tsuzuri だけ。macOS と Linux。PM05）
node benchmarks/run-lto.mjs target/release/tsuzuri --metrics target/perf/PR08-after/lto   # C のループが Tsuzuri の tz_add を呼ぶ時間を、--emit object のリンク・--emit bitcode の ThinLTO のリンク（同じ Clang）・C で書いた add の 3 通りで比べる（9 回、順序を回す。--quick は正しさだけ。PR08）
node benchmarks/run-heap.mjs target/release/tsuzuri --baseline OLDER_COMPILER --capacity   # WASM の heap（PM05）。確保の多い kernel と PX02 の app を wasm32 で走らせ、時間とメモリの大きさ、上限までに収まる最大のサイズを比べる。--quick は動作確認だけ
node benchmarks/run-list-memory.mjs target/release/tsuzuri --baseline OLDER_COMPILER --wasm   # リスト（PM04）。12 の kernel の確保の回数と要求 bytes（決定的）、時間、peak RSS、wasm32 の heap の高水位を、2 つのコンパイラを交互に測って比べる。--opt O0 は -O0 の build、--only で kernel を選ぶ
node benchmarks/run-managed.mjs target/release/tsuzuri --scale 0.1
node benchmarks/run-tasks.mjs target/release/tsuzuri --quick   # Task.parallel の native の時間。--runtime・--baseline-runtime で pool の C ランタイムだけを差し替え、--sweep・--gap-us・--data-parallel で粒度・起床の遅延・Array との比較を測る（PR06）
node benchmarks/run-tasks-wasm.mjs target/release/tsuzuri --quick   # 同じ kernel を WASM threads の pool（Node の worker_threads）で測る。2 つのコンパイラを比べて 2 つの pool を比べる（PR06）
node benchmarks/run-repl.mjs target/release/tsuzuri   # REPL の 1 入力の待ち時間と内訳（G13）
node benchmarks/run-matrix.mjs target/release/tsuzuri   # Matrix の積（mul・mul_fma・mul_parallel）と C の行列積の同条件比較。WASM も実行する（C11。--quick は動作確認だけ。Math.fma がソフトウェアの環境では mul_fma を n = 64 までしか測らず、--fused-all で全サイズを測る）
node benchmarks/run-gpu-vulkan.mjs target/release/tsuzuri   # Gpu.Auto の規則の定数（CPU 参照と Vulkan の転送込みの時間。Vulkan のデバイスが要る）
node benchmarks/run-f16.mjs BASELINE_COMPILER target/release/tsuzuri   # f16 のソフトウェア経路・half 命令・C の _Float16 の同条件比較（D10。BASELINE_COMPILER は D10 より前のコンパイラ。--quick は動作確認だけ。測ったのは macOS arm64 だけ）
```

測定条件の詳細、対応範囲、比較対象の言語との差異、再現手順については [docs/benchmarks.md](docs/benchmarks.md) を参照してください。

---

## 関連ドキュメント

- [言語仕様と ABI](docs/language.md) — 言語の完全な構文仕様、メモリモデル、ホスト連携規約
- [コンパイラ構成と開発指針](docs/architecture.md) — パイプライン設計、IR 生成、コントリビューション基準
- [安定性と互換性の方針](docs/stability.md) — 保証すること・しないこと、言語版（edition）、非推奨の手順
- [ベンチマークの条件と読み方](docs/benchmarks.md) — 各種目における測定環境と性能データ
