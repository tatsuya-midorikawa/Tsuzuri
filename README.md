# Tsuzuri

**AI と人間が、少ない暗黙ルールで堅牢かつ高パフォーマンスなプログラムを書けることを目指す、関数型の式と所有権モデルを備えたプログラミング言語。**

コードは **`.tz`**、型クラス宣言は **`.tt`**、コンピュテーション式のビルダー実装は **`.tc`** に記述します。Rust 製のフロントエンドで厳密な型検査を行い、LLVM をバックエンドとして高効率なネイティブコードおよび WebAssembly を生成します。

- **[公式日本語ドキュメント（言語リファレンス）](_tsuzuri/language-reference/index.md)**: [Tsuzuri の特徴](_tsuzuri/language-reference/languages/how-about-tsuzuri.md) · [なぜ Tsuzuri なのか](_tsuzuri/language-reference/languages/why-tsuzuri.md) · [所有権とムーブ](_tsuzuri/language-reference/ownership-and-memory/ownership.md) · [組み込み型 / 組み込みモジュール](_tsuzuri/language-reference/index.md#組み込み型--組み込みモジュール) · [コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md)
- **[VS Code 拡張機能](vsc/README.md)**: コンパイラ・LLVM・SDK を統合した VSIX パッケージを提供。シンタックスハイライト、コード補完、定義ジャンプ、リファクタリング、ビルド・実行、テスト、ソースデバッグに対応しています（対応アーキテクチャ: x64 / ARM64。詳細は [vsc/Development.md](vsc/Development.md) を参照）。

---

## 主な特徴

- **所有権と借用による安全なメモリ管理**: ガベージコレクション（GC）に頼らず、Rust と同様の所有権移動と借用検査（`ref` / `ref mut`）によってコンパイル時にメモリ安全性を保証します。参照カウントは標準ライブラリの `Rc` / `Arc` を明示的に使ったときだけです。
- **引数規約（edition 2027）**: 通常の所有する引数・裸の型変数・関数引数は既定で読み取り、`own T` は値渡し、`mut T` は排他更新です。可変な所有者・排他参照からの field／要素の代入も検査します。既存 manifest の省略時は 2026 のままで、`fix --edition 2027 --from 2026` が旧契約を保って移行します。
- **直感的で表現力豊かな関数型構文**: カリー化、強力な型推論、代数的データ型（`union`）、網羅性を検証するパターンマッチ、パイプライン演算子（`|>`）、関数合成（`>>` / `<<`）をサポートしています。
- **ゼロコストの型クラス**: 型クラスはコンパイル時に静的に単相化（モノモーフィゼーション）され、動的ディスパッチやランタイム辞書のオーバーヘッドを生じさせません。異なる型の値を一つのコレクションへ入れるときだけ、`dyn` 型で実行時のディスパッチを明示できます。
- **拡張可能なコンピュテーション式**: `.tc` ファイルでビルダーを定義することで、`Result` や `Maybe`、非同期処理などのモナディックな制御フローを言語本来の構文のように扱えます。
- **安全で軽量な並列処理**: 遅延評価・一回実行のタスク（`task { ... }`）と常駐スレッドプールにより、データ競合のない安全な並列計算を提供します。
- **明快な名前空間とモジュール構造**: 1 ファイル = 1 モジュールの原則を維持しつつ、`namespace` と `using` による階層管理、`::` によるパス区切りをサポートしています。
- **LLVM による高い実行性能**: 既定で LLVM `-O3` による最適化を適用し、自動ベクトル化や `--cpu native` によるターゲット最適化に対応しています。
- **マルチターゲット（Native & WASM）**: 単一のコードベースからネイティブ実行ファイル、C ABI 連携用の共有ライブラリ / オブジェクト、およびブラウザや Node.js で動作する WebAssembly を出力可能です。対応環境は Windows x64/Arm64、Linux x64/Arm64、macOS Arm64 の 5 つだけです。`--target` は、Linux の glibc/musl と Windows の MSVC/MinGW を区別した 9 個の native triple を受け付け、同梱ツールチェーンか `TSUZURI_SYSROOT` でクロスリンクします。検証範囲と機能の制限は [対応プラットフォーム](_tsuzuri/language-reference/compiler/platforms.md) にあります。
- **充実した開発支援ツール**: 公式言語サーバー（LSP）、自動フォーマッター、単体テストランナー、プロジェクト生成ツール（`tsuzuri new`。AI エージェント向けの `AGENTS.md`・`llms.txt` も作る）、AI エージェント向けのガイド（`tsuzuri guide`）と MCP サーバー（`tsuzuri mcp`）、Markdown ドキュメント生成ツール、C ヘッダーからの extern 生成ツール（`tsuzuri bindgen`）を標準で同梱しています。

---

## クイックスタート

### プロジェクトの作成 (`tsuzuri new`)

`tsuzuri new` コマンドで、推奨構成のプロジェクト（`Tsuzuri.toml`、`Main.tz`、`.gitignore`）と、AI エージェント向けの `AGENTS.md`・`llms.txt` を新規作成できます（`--no-agent-files` で 2 つを省きます）。AI エージェントには、コンパイラの版と一致する英語のガイド `tsuzuri guide`（例はすべて検証済み）を読ませます。

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
| 最適化 | 既定で LLVM `-O3`、自動 SIMD 化、基本数値変換の直接 lowering。所有権・借用から証明した内部参照属性、scoped alias、限定した scalar TBAA・値・memory effects の情報を付与（public wrapper・slice ABI は維持）。`--cpu native` によるビルド機向け最適化。直接の自己末尾再帰は `-O0` でもループ化し、同じ再帰グループへの末尾呼び出しは native で保証された末尾呼び出し（`musttail`）。 |
| 安全性 | ゼロ除算や配列・リスト境界アクセスの実行時検査。LLVM の未定義動作に依存しない数値仕様。`@checked` による整数オーバーフローは `try` で `Result` に変換可能。 |
| ホスト連携 | スカラー・バッファ・レコードの C ABI 連携および WebAssembly（WASM）のエクスポート／インポート。`extern` のリンク名指定・不透明ハンドル・静的コールバック、ネイティブのホストリンク。標準入出力と OS API は `IO`、協調的な待ちは `Async`、UI やネットワークのイベントはホスト側に委譲。 |
| 開発・AI 支援 | 明示的な関数シグネチャ、暗黙の型変換の排除、位置情報付き JSON 診断（原因の識別子 kind、関連する位置・直し方・修正案を含む）、型つき穴 `?` と型での検索（`tsuzuri find`）、診断の説明（`tsuzuri explain`）とよくある誤りのヒント、決定的な IR 出力。公式 LSP、フォーマッター、テストランナー、REPL（`tsuzuri repl`）、スクリプト実行（`tsuzuri script`、shebang 行）。 |

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

- **配列 (`[T]`)**: 固定長の連続メモリ領域です。`[1, 2, 3]` のように記述し、生成後の長さや要素は不変です。`ref values[start..end]` で終端を含まない共有スライス（型: `ref [T]`）を作成でき、コピーを発生させずに部分列を読み取れます。動的な初期化には `new [i32](count, i -> i as i32)` を使用します。F# の `Array` モジュールの関数は、F# と同じ名前（snake_case）で、配列を最後の引数にして使えます（F# で配列が最後にない `get`・`sub`・`fold_back` などだけ、F# と引数の順が違います）。詳細は [Array](_tsuzuri/language-reference/built-in-types-and-modules/array.md) を参照してください。
- **連結リスト (`[|T|]`)**: 不変のリストです。リテラルは `[|1, 2, 3|]`、空リストは `[||]` です。F# の `List` モジュールの関数（`List.map`・`List.filter`・`List.fold` など）は、F# と同じ名前（snake_case）で、リストを最後の引数にして使えます（F# でリストが最後にない `fold_back` などだけ、F# と引数の順が違います）。要素は連続した領域（chunk）に並び、一度に作ったリスト（`new`・リテラル・複製）は 1 つの chunk、`List.cons` で伸ばしたリストは chunk の連なりです。要素への添字アクセスは、一度に作ったリストなら $O(1)$、`List.cons` で伸ばしたリストなら先行する chunk の数に比例します。要素数が定数の小さなリストを 1 つの関数の中で作って使って捨てる処理は、以前の node の連結（LLVM が確保を全部消して約 1 ns）より遅くなることがあります（`List.map`・`List.rev`・`List.copy`・`List.cons` の連鎖・文字列のリストで 25〜80 ns）。要素数が実行時に決まるリストは速くなります。詳細は [List](_tsuzuri/language-reference/built-in-types-and-modules/list.md) を参照してください。
- **ベクター (`Vec<T>`)**: 伸縮可能な所有バッファです。.NET の `List<T>` のメンバーの関数が、同じ名前（snake_case）で使えます。引数は .NET の順で、`Vec` は最後に渡すので、`values |> Vec.add 3 |> Vec.sort` のようにパイプでつなげられます。容量の増え方と、範囲外などでのトラップ、`-1` と二分探索の補数の戻り値も .NET と同じです。詳細は [Vec](_tsuzuri/language-reference/built-in-types-and-modules/vec.md) を参照してください。
- **順序付きマップ・セット (`Map<K, V>` / `Set<K>`)**: キーの昇順に並べたエントリを、1〜512 エントリのチャンクの列で持つ、所有権を消費して更新するコレクションです（木ではありません）。検索は $O(\log n)$ 回の比較、挿入および削除は $O(\log n)$ 回の比較と 1 チャンク内の移動（最大 511 エントリ）で動作し、チャンクの分割と併合はチャンクの個数に比例する移動を伴います。詳細は [Map](_tsuzuri/language-reference/built-in-types-and-modules/map.md) / [Set](_tsuzuri/language-reference/built-in-types-and-modules/set.md) を参照してください。
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

- **純粋性の契約**: `pure def` と関数型 `pure (T -> U)` は、観測できる副作用がないことをコンパイラが検査する契約です。引数は共有借用か平たいデータの `own` で、generic・純粋 callback・class／instance・`@cpu` にも対応します。停止・トラップしないこと・割り当てがないことは保証しません（[純粋関数](_tsuzuri/language-reference/values-and-functions/functions.md)）。
- **契約**: 関数の本体の先頭に `@requires 条件`・`@ensures \r -> 条件` を書くと、`run`・`test` は入口と出口で検査し（`build` の既定は検査コードなし）、`tsuzuri test --contracts test` は性質テストを生成し、`tsuzuri prove` は z3 などの SMT ソルバーで証明します（[契約](_tsuzuri/language-reference/values-and-functions/contracts.md)）。
- **正準形（edition 2027）**: 範囲は `a ..= b`（終端を含む）と `a ..< b`（含まない）、接尾辞は型名（`86i8u`・`1.5f32`）、借用はキーワード（`ref x`・`deref r`・`ref T`）、ラムダは `\x y -> e` の 1 つの書き方です。edition 2027 は別の書き方を `E0004` で拒否し、edition 2026 では `--warn canonical`（`W1007`）と `tsuzuri fix --edition 2027` が書き換えます（[for...in](_tsuzuri/language-reference/loops-and-conditionals/for-in.md#整数範囲の反復)）。
- **信頼境界と権限**: `extern` の宣言に `@trusted [pure no_alloc ...]` を付けて、人が確かめた外部関数を記録します（印のない使用は `--warn untrusted-extern` と `tsuzuri audit` の `W1010`）。マニフェストの `[capabilities]` と依存の `capabilities = [...]` で、パッケージごとに `fs`・`net`・`env`・`time` など 9 個の権限を許すと、許可の外の静的な使用を `E1036` で拒否します。`tsuzuri audit` はパッケージごとの使用・許可・`extern` の信頼をテキストと JSON Lines（`tz-audit/1`）で報告します。実行時のサンドボックスではありません（[パッケージ: 権限](_tsuzuri/language-reference/organizing-tsuzuri/packages.md#権限capabilities)）。
- **効果の表明**: `@no_alloc`・`@no_copy`・`@no_trap`・`@terminates` を `def` に付けると、コンパイラが関数の効果（確保・暗黙の複製・論理のトラップ・停止しない可能性）を型付き IR から推論し、表明を破れば `E1031` で拒否します。属性は検査だけで、生成コードは変わりません（[属性: 効果の表明](_tsuzuri/language-reference/values-and-functions/attributes.md#効果の表明no_allocno_copyno_trapterminates)）。
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
- **再帰関数**: 再帰関数は `def rec name :: ...`、相互再帰は `and name :: ...` で繋いで記述します。関数の本体やブロックの中では、`let rec go = \n acc -> ...`（相互再帰は `and`）で補助関数を定義できます。外側のローカル値を捕捉したまま再帰でき、文字列や配列などの値は借用して読みます。直接の自己末尾再帰は、`-O0` であっても LLVM レベルでループ構造へと自動変換されます（`def rec` も `let rec` も同じです）。同じ再帰グループのほかの関数への末尾呼び出しは、条件を満たすと保証された末尾呼び出し（`musttail`）で、native では `-O0` でも相互再帰のスタックが増えません（WASM は `--wasm-feature tail-call` を付けたときだけです）。詳細は [言語仕様: 再帰とスタック](docs/language.md#再帰とスタック)、[言語仕様: 制御構文](docs/language.md#制御構文)、[言語仕様: ローカルな `let rec`](docs/language.md#ローカルな-let-rec)、および [制御構文のベンチマーク](docs/benchmarks.md#制御構文の比較) を参照してください。実行例は `tsuzuri run examples/control` で確認できます。

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
| `Domain` | 純粋な計算のトラップを `Result.Error` にする失敗の領域（macOS・Linux のネイティブ） |
| `Channel` | 容量付きの MPMC キュー。`Sender`／`Receiver`、`bounded`・`send`・`recv`・`clone_sender`。最後の `Sender` の drop で閉じ、全スレッドが待ったらデッドロックのトラップ |
| `String`, `Utf8String`, `Char` | UTF-16 / UTF-8 文字列および文字操作 |
| `Regex`, `Unicode` | 線形時間の正規表現、Unicode 17.0.0 の文字データ |
| `Math`, `Int` | 高精度数学関数、浮動小数点超越関数、整数組み込み演算 |
| `Debug`, `Test` | デバッグ出力およびテストフレームワーク |
| `Bench`, `Gen` | ベンチマーク（`tsuzuri bench`）とプロパティテストの生成器 |
| `Alloc` | 確保・再確保・解放と要求バイト数の snapshot・差分。テストは counting heap を必要なときだけ自動で連結します |
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

`edition` は、そのパッケージのソースを読むときの言語版です。対応する値は `"2026"` と `"2027"` で、既存 manifest の省略時は互換性のため `2026` のままです。新規パッケージ・manifest のない CLI 入力・script・REPL・std は最新の `2027` を使います。依存パッケージは自分の `Tsuzuri.toml` の版で読まれ、異なる edition を 1 つのプログラムにまとめられます。2027 は A18 の明示的な例外として、通常の引数を読み取り既定にし、`own T` と `mut T` で所有・更新を表します。2026 の契約を保存して移行するには `tsuzuri fix --edition 2027 --from 2026 app/` を使います。保証と例外は [安定性と互換性の方針](docs/stability.md) にあります。

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

### 修正案の適用と言語版の移行 (`tsuzuri fix`)

`tsuzuri fix PATH` は、診断が持つ修正案（既定は意味を保つ `certain` だけ。`--applicability likely` で `likely` も）を適用し、ラウンドごとに再検査して、診断が増えるなら何も書きません。`--dry-run` は統一差分を出し、`--json` は適用した・しなかった修正案を 1 行 1 オブジェクトで出します。依存パッケージと標準ライブラリは書き換えません。

```sh
tsuzuri fix app/ --dry-run
tsuzuri fix app/
```

値渡しの署名と標準 API を `own`・`_owned` で保存し、対象版で全体を再検査してから atomic に書き換えます。
依存パッケージは変更せず、manifest の edition は最後に更新します（[詳細](_tsuzuri/language-reference/compiler/usage.md#fix)）。

```sh
tsuzuri fix --edition 2027 --from 2026 --dry-run app/
tsuzuri fix --edition 2027 --from 2026 app/
```

### MCP サーバー (`tsuzuri mcp`)

Claude Code・VS Code・Copilot CLI・Cursor などの AI エージェントから使える、標準入出力の Model Context Protocol サーバーです。型検査の診断（`tz_check`）、言語の案内、診断の説明、知識グラフの照会（宣言・呼び出し元・最短の鎖・型での検索）、版の比較、修正の計画、図を、読み取り専用の 12 の型付きツールで返します。ファイルを書かず、子プロセスもネットワークも使いません（[MCP サーバー](_tsuzuri/language-reference/compiler/mcp.md)）。

```sh
claude mcp add --transport stdio tsuzuri -- tsuzuri mcp
```

### 言語サーバー (`tsuzuri lsp`)

VS Code や Neovim など、LSP 対応のエディタから利用可能な標準入出力（stdio）言語サーバーです。

- **主要機能**: 未保存バッファのリアルタイム全量同期、複数診断、型ホバー表示、定義ジャンプ、シンボル検索、参照検索、リネーム、補完、シグネチャヘルプ、セマンティックハイライト、関連する位置の表示、診断の修正案（未使用ローカル変数の `_`、`E1021` の足りない腕、ヒントの字句の置き換えなど）のクイックフィックスと、文書の `certain` の修正案をまとめて適用する `source.fixAll.tsuzuri`、診断の範囲の hover での kind と `tsuzuri explain` の案内。
- **高精度なリファクタリング**: リネームやクイックフィックスは編集後のコードを内部で再解析し、安全性が確認された差分のみを適用します。診断の修正案は、適用するとその診断のコードの件数が減り、どのコードのエラーも増えないものだけを出します（ほかのエラーは残っていてもかまいません）。詳細は [コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md) を参照してください。

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

# /// の中の tsuzuri の例を確かめる（doc-test。std を渡すと標準ライブラリの例）
tsuzuri test --doc src/
```

プロパティテストは標準モジュール `Gen` で書きます。`Gen.for_all` は生成器の値で性質を確かめ、失敗すると反例を最も単純な値まで縮小し、seed・何個目の値か・反例を表示します。

```text
test "加算は可換" = Gen.for_all (Gen.pair (Gen.i64()) (Gen.i64())) (\pair -> match pair with | (a, b) -> Int.wrapping_add a b == Int.wrapping_add b a)
```

`tsuzuri test` は既定で、テストの中で折り返した整数の演算を `integer overflow wrapped at ./Main.tz:2:50 (i32 mul)` のように報告し、そのテストを失敗にします。乱数の整数どうしの足し算のように折り返すことが正しい計算は、`Int.wrapping_add` などで書きます（`--allow-overflow` は報告だけにします）。

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

# 時間とは別の counting 実行器で、1 反復あたりの確保回数と要求バイト数も出す
tsuzuri bench benches/ --allocs --json
```

各ベンチは別プロセスで 1 件ずつ計測されます（ネイティブのみ）。速さの合否の閾値はありません。
`--allocs` は成功行の末尾に `alloc_calls`（確保と再確保の合計）・`alloc_bytes` を加えます。
計数は 2 反復の増分から 1 反復の増分を引くので準備と解放を相殺し、時間の実行器を変えません。
トラップ・時間切れ・負の時間・負の計数の差分は失敗です。`bench` は予約語です。

### ドキュメント生成 (`tsuzuri doc`)

ソースコード内の `///` ドキュメントコメントを抽出し、公開 API の Markdown ドキュメントを自動生成します。

```sh
# 公開 API ドキュメントを生成
tsuzuri doc src/ -o docs/api
```

説明の中の ```` ```tsuzuri ```` のフェンスは例で、`tsuzuri test --doc` が確かめます。

- ```` ```tsuzuri run=3 ```` は実行して標準出力が `3` であること（空白や改行は `%20`・`%0A`）、```` ```tsuzuri test ```` は中の `test` が通ること、```` ```tsuzuri fails=E1003 ```` は型検査が `E1003` で拒否すること、印なしは型検査が通ることを確かめます（`ignore` は確かめません）。
- `# ` で始まる行は隠し行で、例を動かすときだけ使い、`tsuzuri doc` とホバーには出しません。生成する文書は `run=` の出力を `Output:` として添えます。
- 標準ライブラリの公開関数にはすべて例があり、`tsuzuri test --doc std` で確かめられます。

宣言の説明を `@deprecated` で始めると（`/// @deprecated use next instead`）、その宣言は非推奨になります。別のパッケージや標準ライブラリがそれを使うと、警告 `W1005` が出ます。同じパッケージの中の使用には出ません。

### 公開 API の差分 (`tsuzuri api-diff`)

パッケージやプロジェクトの 2 つの版の公開 API を比べ、互換性のない変更（公開宣言の削除、署名の変更、record の field や union の case の追加など）と、互換性を保つ変更（宣言の追加、説明の変更、`@deprecated` の追加）を 1 行ずつ出します。互換性のない変更があると終了コードは 1 です。既定では型検査をせず、宣言の署名だけを比べます。`--effective` は両方を依存込みで検査し、alias の解決後の型・実際の引数規約・`pure` の呼び出し段階・region の契約を比べます。検査や依存の読み込みが失敗した場合は診断で止まり、構文だけの比較へ戻しません。

```sh
# 前の版のチェックアウトと、今のパッケージを比べる
tsuzuri api-diff ../geometry-1.2.0 .
tsuzuri api-diff ../geometry-1.2.0 . --json
tsuzuri api-diff ../geometry-1.2.0 . --effective --json
```

次は、`Shapes.tz` だけを持つ 2 つのディレクトリを比べたときの出力です（`Tsuzuri.toml` のない入力は、名前空間のない名前で出ます）。

```text
breaking: added field Shapes.Point.z: `z: f64` (constructions of the record without it stop compiling)
breaking: changed def Shapes.area: `def area :: f64 -> f64` => `def area :: f64 -> f32`
non-breaking: added def Shapes.double: `def double :: i64 -> i64`
summary: 2 breaking, 1 non-breaking
```

`tsuzuri publish` は `api-diff` を呼びません。公開の前に実行してください。出力とその制限は [コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md#api-diff) にあります。

### 信頼境界と権限 (`tsuzuri audit`)

`tsuzuri audit [PATH]` は、プロジェクトを `check` と同じに検査し、パッケージごとに、コードが使う権限（std の関数と `extern` から導出）、マニフェストが許す権限、`extern` の関数の `@trusted` の印を報告します。ルートの `Tsuzuri.toml` に `[capabilities]` か依存の `capabilities` があると、`check`・`build`・`run`・`test` も許可の外の使用を `E1036` で止めます。宣言がなければ何も検査せず、出力は変わりません（[コンパイラの使い方](_tsuzuri/language-reference/compiler/usage.md#audit)）。

```sh
tsuzuri audit app
tsuzuri audit app --json --deny-warnings
```

### 知識グラフ (`target/tz/kg`、`tsuzuri kg`)

`check`・`build`・`run`・`test` は、プロジェクトの知識グラフ（宣言をノード、呼び出し・型の使用・テストなどの関係を辺にした JSON Lines、関数ごとの効果・トラップ・所有権の事実、型のレイアウト、短い英語の要約カード）を `target/tz/kg/` に書きます。ソースが変わっていなければ書き直さず、`--no-kg` か `TSUZURI_KG=0` で止められます。`tsuzuri kg build|verify|clean|export` で明示的に作る・確かめる・消す・出力でき、`tsuzuri kg show|callers|callees|uses|tests-for|search|find|path|brief` でソースを読まずに宣言・呼び出し・テスト・型での検索・最短の関係の鎖を引けます。`tsuzuri kg impact --git REV` は前の版との差（公開 API・効果・警告）とその差に届くテストを出し、`tsuzuri test --affected REV` はそのテストだけを実行します。`tsuzuri kg diagram module|types|calls|ownership|concurrency|layout|transitions` は、その知識グラフからモジュールの依存・型・呼び出し・所有権の流れ・並行・メモリの配置・状態遷移の図を Mermaid・DOT・Markdown で出し、ID の要らない図は `views/*.mmd` として知識グラフに入ります（[図表の生成](_tsuzuri/language-reference/compiler/diagrams.md)、[知識グラフ](_tsuzuri/language-reference/compiler/kg.md)、[形式](_tsuzuri/language-reference/compiler/kg-format.md)）。

### 型つき穴と型での検索 (`?`、`tsuzuri find`)

まだ書いていない式の位置に `?`（`?名前`）を置くと、`check` はその位置で期待される型・見える局所・その型の値を作れる候補（局所、`Array.length (ref xs)` のような呼び出し、共用体のケースなど）を `E1030` で出します。穴は誤りなので、プログラムは生成しません。期待される型の局所が 1 つだけなら、その名前に置き換える `maybe` の修正案も付きます（[型つき穴](_tsuzuri/language-reference/types-and-type-inference/type-inference.md#型つき穴-)）。未知の名前（`E1002`・`E1004`・`E1007`・`E1020`）には、近い名前と、モジュールを付ければ解決する名前（`length` に `Array.length`）が候補として付きます。

`tsuzuri find` は、型から std と（`--project DIR` で）プロジェクトの宣言を探します。照合は `tsuzuri kg find` と同じです（[find](_tsuzuri/language-reference/compiler/usage.md#find)）。

```sh
tsuzuri find 'ref [i64] -> i64'
tsuzuri find --result bool --takes string
tsuzuri find --project . --module Main 'i64 -> i64'
```

### 診断の説明 (`tsuzuri explain`)

すべての診断は、原因を表す安定した識別子 kind（`use_after_move`、`type_mismatch` など）を持ち、`--json` の `kind` キーと LSP の `data.kind` に出ます。`tsuzuri explain` は、kind の原因と、テストで検査した誤りの例・直した例を出します。コードを渡すと、そのコードの kind の一覧と直し方を出します。説明はコンパイラに埋め込まれた英語です。

```sh
tsuzuri explain use_after_move
tsuzuri explain E1012 --json
tsuzuri explain --catalog
```

よくある誤りの形（`pair.0`、`(5: i64)`、`=>` の腕、`println!`、`Option` など 40 件）には、診断にヒントの `help` が付き、7 件は `likely`／`maybe` の修正案も持ちます。詳細は [診断メッセージとエラーコード](_tsuzuri/language-reference/compiler/diagnostics.md#kind-と-explain) にあります。

### C ヘッダーからの extern 生成 (`tsuzuri bindgen`)

C のヘッダーを Clang で解析し、ヘッダー自身の関数・enum の定数・整数の `#define`・struct・typedef から、リンク名付きの `extern "symbol" def`・`const`・`record`・`extern type`・`type` を持つモジュールを生成します。不透明な struct のポインターはハンドル、関数ポインターの引数は静的コールバックになり、`--buffer 関数:ポインター:長さ` で指定した組は `ref [T]`、`--consume 関数:引数` で指定したハンドルはムーブになります。ABI の一致を確かめられない宣言（可変長引数、注釈のないポインター、値渡しの struct、大域変数など）は、理由付きの `// skipped` 行と警告 `W2002` になります。出力は決定的で、`// Generated by tsuzuri bindgen. Do not edit.` で始まるファイルだけを上書きします。

```sh
tsuzuri bindgen vendor/sample.h -o Sample.tz --include-dir vendor --buffer sample_sum:values:count
```

### デバッグ情報と出力仕様

- **DWARF デバッグ情報**: `build` や `run` に `-g`（`--debug-info`）を付与することで、関数・行番号・変数・型の DWARF 情報を埋め込めます。macOS では `.dwarf` ファイルが生成され、LLDB などのデバッガでシンボルを解決可能です。呼び出し履歴の関数名は `Main.show` のような Tsuzuri の名前で、ステップ実行はランタイムとコンパイラーが生成した補助関数に入りません。LLDB に `scripts/lldb/tsuzuri_lldb.py`（配布物では `share/lldb/tsuzuri_lldb.py`）を `command script import` で読み込むと、文字列・配列・リスト・union・`Map` などを Tsuzuri の値として表示します。VS Code 拡張は自動で読み込みます。Windows で MSVC のリンカーを使う場合は CodeView も出し、実行ファイルの隣に PDB を書いて natvis の表示を埋め込みます（[デバッグ](_tsuzuri/language-reference/compiler/debugging.md)）。
- **実行時トラップ報告**: 配布ビルドでは `--trap-info` を指定することで、トラップ発生時の正確なソース位置情報を出力に含めることができます。WASM では `.trap.json` 表との連動に対応しています。`tsuzuri run` のトラップは、`E2005` の下にトラップした関数から入口までの呼び出しの連鎖（`  at Main.level2` の行、`--json` では `trap`）を出します。`--trap-info` の macOS・Linux の実行ファイルは、環境変数 `TSUZURI_TRAP_REPORT` が指すファイルへ同じ連鎖を書き、関数名は `llvm-symbolizer`（`TSUZURI_LLVM_SYMBOLIZER`）で引きます（[デバッグ](./_tsuzuri/language-reference/compiler/debugging.md#トラップのバックトレース)）。
- **失敗の閉じ込め（`Domain`）**: 純粋な計算を `Domain.run (\() -> ...)` に包むと、中のトラップ（範囲外・ゼロ除算・`assert` など）が `Result.Error` になり、確保した値は解放されてプロセスが続きます。本体の純粋性と、捕捉・結果が平たいデータであることを `E1038` で検査します。macOS・Linux のネイティブだけです（[Domain](./_tsuzuri/language-reference/built-in-types-and-modules/domain.md)）。
- **コンソールの出力とフォーマット**: トップレベルの結果式が評価された場合、その型が `Display` を実装していれば文字列表現が標準出力に表示されます。浮動小数点数は最短で往復可能な十進表現で出力され、ネイティブと WASM で完全に一致します。

---

## 環境構築とビルド

### 必要要件

- **対応 OS / CPU**: Windows x64・Arm64、Linux x64・Arm64、macOS Arm64 のみ（Intel Mac、32-bit、その他の OS / CPU は対象外）
- **Rust**: 1.85 以降（浮動小数点リテラルの厳密な丸めに `rustc_apfloat` を使用）
- **LLVM / Clang**: 17 以降（WebAssembly のリンクには `wasm-ld`（LLD）も必要）
- **Node.js**: 20 以降（WASM テスト・検証用）
- **Python**: 3.9 以降（参照実装テスト・デスクトップ検証用）

### 配布パッケージの利用

ローカルに LLVM / Clang の完全な開発環境を構築せずに利用したい場合は、コンパイラ本体・Clang・LLD・SDK を同梱したプラットフォーム別配布パッケージ（`tsuzuri-<version>-<host>.tar.gz`）を利用できます。詳細は [配布物を使う](_tsuzuri/language-reference/compiler/usage.md#配布物を使う) を参照してください。

### プラットフォーム別のセットアップ

#### macOS Arm64 (Homebrew)

Apple Silicon のネイティブ arm64 環境だけが対象です。Rosetta 上の x64 コンパイラ・拡張ホストは対応しません。CLI・VSIX のツールチェーン作成と再検証では、Clang の既定 target も arm64 であることを確認します。

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
コンパイラは、x64 では `--target=x86_64-pc-windows-msvc`、Arm64 では `--target=aarch64-pc-windows-msvc` を指定してネイティブ実行ファイルを生成します。

```powershell
cargo build --release
```

`clang` や `wasm-ld` のパスは環境変数 `TSUZURI_CLANG`、`TSUZURI_WASM_LD` で明示的に指定することも可能です。`--emit bitcode` が C ランタイムを IR のまま 1 つにまとめる `llvm-link` は、`TSUZURI_LLVM_LINK` で指定します（なければ、配布物の `bin/`、Clang と同じディレクトリ、PATH の順で探します）。`TSUZURI_LINKER=lld` は、ネイティブ実行ファイルと共有ライブラリの最終リンクに Clang へ `-fuse-ld=lld` を渡す opt-in で、未設定と `default` は従来どおり Clang の既定のリンカーです（[option.md](./_tsuzuri/language-reference/compiler/option.md)）。`--profile-use` にディレクトリを渡すときに `.profraw` を merge する `llvm-profdata` は `TSUZURI_LLVM_PROFDATA` で指定します（未設定なら `TSUZURI_CLANG` の Clang の隣、その toolchain の順で探し、PATH は探しません）。

Windows は、CI（`.github/workflows/windows.yml`。x64 は `windows-2022`、Arm64 は `windows-11-arm`。main へのマージのたびと、`ci:full` ラベルを付けた PR で動く）が LLVM 21.1.8、MSVC 14.44、Windows SDK 10.0.26100 の組で、fmt・clippy・release build・`cargo test`・`tests/windows.mjs`・`tests/cache.mjs`・`tests/examples.mjs`・`tests/wasm_threads.mjs` を実行します。Visual Studio は x64 が 2022、Arm64 が 2026 で、後者も同梱の 14.44 toolset を使います。配布物の MinGW は、5 環境の CLI と VSIX を作る `.github/workflows/vscode.yml` が検証します。Windows の共有ライブラリ（`--emit shared`）と OS API は未対応です（[G10](_features/_completed/G10-windows.md)）。

#### クロスコンパイル

`--target TRIPLE` で、対応する 5 環境の別 OS や CPU 向けのオブジェクト・実行ファイルを作れます。実行ファイルのリンクには、その target の C ヘッダー・C ライブラリ・リンカーが要ります。同梱ツールチェーン（Zig）は Linux（glibc・musl）の x64/Arm64、MinGW の Windows x64/Arm64、macOS Arm64 の 7 個をクロスリンクできます。MSVC のリンクは Windows SDK と CRT を使います。システムの Clang には `TSUZURI_SYSROOT` で SDK か sysroot を渡せます。ビルド機で動かせない target の `run`・`test` は、`TSUZURI_RUNNER` に実行器（qemu-user など）を指定したときだけ動きます。何かを取得したり導入したりはしません。

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
tsuzuri new directory [--namespace NAME] [--edition 2026|2027] [--no-agent-files]
tsuzuri guide [--level 0|1] [--topic NAME] [--list] [--json] [--edition YEAR]
tsuzuri guide std [MODULE] | --agents [DIR] | --llms-txt [DIR]
tsuzuri fetch directory [--json]
tsuzuri publish directory --git URL --rev COMMIT [--json]
tsuzuri api-diff old new [--json] [--effective]
tsuzuri find [--project DIR] [--module NAME] [--limit N] [--json] QUERY | [--result TYPE] [--takes TYPE]...
tsuzuri audit [PATH] [--json] [--deny-warnings]
tsuzuri prove [source.tz|directory] [--function NAME]... [--prove-timeout SECONDS] [--json]
tsuzuri build|run|test|check ... [--contracts off|check|test|prove]
tsuzuri check|build|run|test ... [--kg | --no-kg] [--kg-dir DIR]
tsuzuri kg build|verify|clean|export [PATH] [--kg-dir DIR] [--std] [--force] [--nodes|--edges|--facts] [--no-refresh] [--json]
tsuzuri kg brief|show|callers|callees|uses|tests-for|search|find|path ... [--budget N] [--path PATH] [--no-refresh] [--json]
tsuzuri kg path FROM TO [--rel R[,R...]] [--depth N]
tsuzuri kg impact (--git REV | --from PATH [--to PATH]) [--path PATH] [--json]
tsuzuri kg diagram KIND [ID] [--path PATH] [--format mermaid|dot] [--max-nodes N] [--depth N] [--json]
tsuzuri test source.tz|directory --affected BASE
tsuzuri test --doc directory|std [--list] [--filter TEXT] [--index N]... [--json] [-O0..-O3]
tsuzuri fix [--applicability certain|likely|maybe] [--dry-run] [--json] source.tz|directory
tsuzuri fix --edition YEAR [--from YEAR] [--dry-run] [--json] source.tz|directory
tsuzuri explain TARGET [--json]
tsuzuri explain --catalog [--json]
tsuzuri bindgen header.h -o Module.tz [--include-dir DIR]... [--buffer F:P:L]... [--consume F:P]... [--json]
tsuzuri toolchain info
tsuzuri lsp
tsuzuri mcp [--root DIR] [--kg-dir DIR]
```

### 主要オプション一覧

| オプション | 説明 |
| --- | --- |
| `-o`, `--output PATH` | 出力先パスを指定します（親ディレクトリは自動作成されます）。 |
| `--target native\|wasm32\|wasm64\|TRIPLE` | ターゲット環境を指定します（既定: `native`。`wasm64` は 64-bit 線形メモリ。`TRIPLE` は対応する 5 環境の 9 個の native の名前で、`tsuzuri --help` に一覧が出ます）。ビルド機で動かせない `TRIPLE` の `run`・`test` は、`TSUZURI_RUNNER` に実行器を指定したときだけ動きます。 |
| `--emit exe\|object\|llvm\|header\|wasm\|wgsl\|wgsl-relaxed\|spirv\|spirv-relaxed\|shared\|bitcode\|bindings-js\|bindings-cs\|bindings-py\|bindings-cpp` | 出力成果物の種類（既定: native は `exe`、WASM は `wasm`）。`wgsl` は厳密な `i32`／`i32u` の GPU カーネル、`wgsl-relaxed` は緩い `f32` を含むカーネルの WGSL、`spirv` は Vulkan 用の厳密なカーネル（`i32`・`i32u`・`i64`・`i64u`・`f32`）、`spirv-relaxed` は緩い `f32` のカーネルの SPIR-V で、どれも `-O`・`--target`・`--cpu` と併用できません。`bindings-js` は `--target wasm32` で JavaScript のグルー `<name>.mjs` と TypeScript 宣言 `<name>.d.mts` を出します（`--wasm-feature threads` でスレッドプール版）。`shared` は native の共有ライブラリ、`bitcode` は C・C++ のホストが `-flto=thin` で小さな公開関数を inline できる ThinLTO の LLVM bitcode（native と wasm32。同じ Clang でリンクする。C ランタイムを使うプログラムには `llvm-link` が要る）、`bindings-cs`／`bindings-py`／`bindings-cpp` はそれを呼ぶ C#／Python／C++ のバインディングです。 |
| `-O0` ～ `-O3` | 最適化レベル（既定: `-O3`。高速化のために精度を損なう fast-math などは使用しません）。 |
| `-Os`, `-Oz` | サイズ重視の最適化（`build` と `run` だけ。`test`・`bench`・`repl`・`script` は `-O0`〜`-O3` だけ）。生成する LLVM IR は `-O3` と同じで（native の関数値の inline 属性 `alwaysinline` だけは、`-O0`・`-Os`・`-Oz` で外れます）、Clang に渡すレベルだけが変わります。意味・トラップ・`-g` は変わりません。 |
| `--cpu generic\|native` | CPU 命令セットの特化（既定: `generic`。`native` はビルド機の命令セットとスケジューリングに最適化）。 |
| `--deny-warnings` | 警告が存在する場合にコンパイルを失敗させ、コード生成や実行を行わずに停止します。 |
| `--debug-output` | WASM の `Debug` 出力の import を有効にします（native は常に出力を書きます）。 |
| `--version` | コンパイラの版（`tsuzuri 0.1.0` の形）を出して終了します。 |
| `--warn canonical` | edition 2026 のソースの、edition 2027 が拒否する表記（素の `..` の範囲、`86uy` のような短い接尾辞、`&x`・`*r`・`&T`、旧ラムダ `x -> e`）に `W1007` を出し、正準形への書き換えを修正案として示します。 |
| `--edition 2026\|2027` | `Tsuzuri.toml` のないプロジェクトのソースを読む edition（既定は 2027。マニフェストがあると `E2000`）。`check`・`build`・`run`・`test`・`script` と `tsuzuri new` で使えます。 |
| `--warn untrusted-extern` | ルートのパッケージで宣言して使っている、`@trusted` のない `extern` の関数に `W1010` を出します。 |
| `--trap-info` | 配布用ビルドにおいて、実行時トラップの正確なソース位置情報を保持します。macOS・Linux の実行ファイルは、`TSUZURI_TRAP_REPORT` があればトラップの呼び出しの連鎖も書きます。 |
| `--overflow-report` | `build`・`run`（`test` は既定）で、範囲の証明で溢れないと示せない整数の `+ - * ** -x` に検査を足し、折り返した地点を報告します（native は標準エラー、WASM は `tsuzuri_overflow_count`・`tsuzuri_overflow_hit` と `<出力>.overflow.json`）。値は変わりません。診断用で、配布ビルドには使いません。 |
| `--allow-overflow` | `tsuzuri test` で、整数の演算が折り返したテストを失敗にせず、報告だけを出します。 |
| `-g`, `--debug-info` | DWARF デバッグ情報を付与します。`test` では `--index N -g -o PATH` で 1 件のテストをデバッグ用のランナーとしてビルドします（実行しません）。 |
| `--wasm-max-memory SIZE` | WASM の最大線形メモリサイズ（既定: 16MiB、例: `256MiB`）。 |
| `--wasm-stack-size SIZE` | WASM のスタックサイズ（既定: 1MiB、例: `4MiB`）。 |
| `--coverage PATH` | `tsuzuri test` で、成功したテストの行と関数のカバレッジを lcov 形式で `PATH` に書きます（ネイティブだけ）。 |
| `--samples N` | `tsuzuri bench` の標本数（1〜1000、既定 11）。 |
| `--allocs` | `tsuzuri bench` の時間測定後、別の counting 実行器で `alloc_calls`・`alloc_bytes` を報告します。指定しない出力は変わりません。 |
| `--seed N` | `tsuzuri test` のプロパティテスト（`Gen.for_all`）の seed（既定は固定の `11400714819323198485`）。 |
| `--doc` | `tsuzuri test` で、テストの代わりに `///` の中の ```` ```tsuzuri ```` の例を確かめます（`run=`・`test`・`fails=`・`ignore`。記法の誤りは `E2011`）。 |
| `--contracts off\|check\|test\|prove` | 契約の行（`@requires`・`@ensures`）の使い方（既定: `build` は `off`、`run`・`test` は `check`）。`test` は `tsuzuri test` で性質テストを作り、`prove` は SMT ソルバーで証明してから `check` として続けます。 |
| `--prove-timeout SECONDS`, `--function NAME` | `--contracts prove` と `tsuzuri prove` の問いごとの時間（既定 10 秒）と、証明する関数。 |
| `--wasm-host wasi` | wasm32 において、標準入出力および OS API を WASI preview1 のインポートへ接続します。 |
| `--wasm-feature simd128\|threads\|jspi\|webgpu\|net\|tail-call` | WebAssembly の追加機能（128-bit SIMD、Worker スレッド分散、Async の JavaScript Promise Integration、`Gpu.request Gpu.WebGpu` 用の WebGPU import、`Net` のソケットを Node.js 向けのグルーで実装する `net`、同じ再帰グループへの保証された末尾呼び出しを `return_call` にする `tail-call`。`net` には `jspi` が必要）を有効化します。 |
| `--allocator system\|host\|counting\|small` | ヒープ確保の行き先（既定: `system`）。`host` はホストの `tsuzuri_host_alloc`・`tsuzuri_host_free`・`tsuzuri_host_realloc` を呼びます。`counting` は実行ファイル・object・共有ライブラリ・LLVM IR・header・WASM に使え、`Alloc` と従来の 32-byte ABI `tsuzuri_alloc_stats` で読めます。`run` と `test` も `counting` だけを受けます。`small` は native 64-bit のサイズ区分 allocator（512 バイト以下を区分から、それ以上を C ライブラリから確保）で、WASM・`--trap-mode return`・`--freestanding` とは排他です。 |
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
- **codegen unit（PB07、opt-in）**: 環境変数 `TSUZURI_CODEGEN_UNITS=auto|split` を選ぶと、`-O0` の native の実行ファイルと `--emit object`、wasm32 の `--emit wasm`（`--trap-info`・`-g`・`--wasm-feature tail-call` を含む）は、生成した IR を決定的な unit に分け、unit ごとの object を同じ保存先の `units/` に内容で保存し、変わった unit だけを並列にコンパイルして 1 回でリンクします。1 つのモジュールの本体の編集で再コンパイルされるのは、そのモジュールの unit だけで、大きなプログラムでは編集後のビルドが短くなります。初回のビルドは長くなるので、既定は `single`（分けない。未設定と空も同じ）で、`auto` は IR が 16 MB 以上の対象のビルドだけ、`split` は対象のビルドをすべて分けます。ほかの値は `E2000` です。`-O1` 以上、`--backend fast`、`--emit shared`・`bitcode`・`llvm`、PGO、Windows のホストなどは、どの値でも従来どおり 1 つのモジュールです。出力の意味と公開 symbol は変わりません（[詳細](./_tsuzuri/language-reference/compiler/option.md#コード生成の単位codegen-unit)、[測定](./docs/benchmarks.md#pb07-codegen-unit-による増分ビルド)）。
- **整数の範囲証明の旗**: 整数の `+ - *` は常に型の幅で折り返します。`TSUZURI_RANGE_FLAGS` は、コンパイラが範囲を証明できた演算に LLVM の `nsw`／`nuw` を書くかを選びます。`off`（既定）は何も書かず、`proven` は証明のある命令に書き、`audit` は旗の代わりに、証明が誤っていれば trap する検査を書く試験用です。ほかの値は `E2000`、結果の値は変わりません（[変数の表](./_tsuzuri/language-reference/compiler/option.md#環境変数)）。
- **コンパクト文字列（opt-in）**: `TSUZURI_STRINGS=compact` は、全コード単位が 0xFF 以下（ASCII と Latin-1）の `string` を 1 単位 1 byte で保存し、そうでない文字列は UTF-16 のままにします（長さの語の bit 62 が印で、`Maybe<string>` の niche は変わりません）。ASCII・Latin-1 の文字列の確保量は約半分になり、native と `--wasm-feature simd128` の WebAssembly では Latin-1 の走査・比較・変換に 128-bit vector の kernel を使います。長さ・添字・順序・hash・表示・host との ABI（常に UTF-16）は変わりません。既定の `utf16` は PM03 より前と byte 単位で同じ IR を出し、ほかの値は `E2000` です。runtime の IR・小さな WebAssembly・ビルド時間が増え、host へ短い文字列を返す呼び出しが遅くなるので、既定は `utf16` のままです（[測定と判断](./docs/benchmarks.md#pm03-コンパクト文字列)、[使い方](./_tsuzuri/language-reference/compiler/option.md#コンパクト文字列の旗)）。
- **スレッド数**: コンパイラ自身が使うスレッド数の上限は環境変数 `TSUZURI_THREADS`（1 から 32 の整数）で決まります。未設定または空のとき、CLI は使える core 数（最大 8）、ライブラリ API は 1 です。`1` は呼び出したスレッドだけで解析し、範囲外や整数でない値は `E2000`（終了コード 2）です。並列に実行する段は、現在はソースの構文解析（`parse`。frontend cache の照合を含む）、関数ごとの本体の型検査と仕上げ、所有権の検査（`check` の一部と `ownership`）で、ソースや関数が少ない入力は 1 スレッドのままです。診断（順序・重複の除去・上限）、IR、終了コードはスレッド数で変わらず、キャッシュのキーにもスレッド数は入りません（[詳細](./_tsuzuri/language-reference/compiler/option.md#コンパイラのスレッド数)）。
- **デバッグ用のツール**: macOS で、タスクを含むデバッグ用オブジェクトを作る `llvm-link` は環境変数 `TSUZURI_LLVM_LINK`（既定 `llvm-link`）、デバッグ実行ファイルを作る `dsymutil` は `TSUZURI_DSYMUTIL`（既定 `dsymutil`）、`run` のトラップの呼び出しの連鎖に名前を付ける `llvm-symbolizer` は `TSUZURI_LLVM_SYMBOLIZER`（既定 `llvm-symbolizer`。なければアドレスだけ）で指定できます。空の値も「設定済み」で、配布物や `PATH` を探しません（[option.md](./_tsuzuri/language-reference/compiler/option.md#環境変数)）。
- **知識グラフと証明の道具**: 知識グラフの置き場は `--kg-dir`、なければ環境変数 `TSUZURI_KG_DIR`（未設定は `<根>/target/tz/kg`。`tsuzuri mcp` は読みません）です。`tsuzuri prove` と `--contracts prove` の SMT ソルバーは `TSUZURI_SMT`（名前か経路。未設定なら PATH の `z3`、なければ `cvc5`。同梱しません）です。
- **段階別時間**: `TSUZURI_TIME_PASSES=1` のときだけ、終了時に stderr の最後へ時間の JSON を 1 行出します（`watch` は 1 回ごと、`serve` が引き受けた要求は 1 要求ごとに、その回の分だけです）。`emit` は target ごとの ABI・WASM host／runtime の IR 処理も含み、Clang・リンカーは `tool.*` に分けます。成果物・診断・終了コードは変わりません（[段の範囲](./_tsuzuri/language-reference/compiler/option.md#コンパイルの段階別時間)）。

---

## 検証と性能測定

G31 の着手時検証（基準 `41f80bc7`）: 旧付録の shop 全文を未変更のコンパイラで `check`・`test --list` に通し、無診断・16 テストと 5183 字（941/460/770/1394/662/956 字）を再現しました。D-59 の対象最後の順序を確認したところ、既存の `Array.map callback subject` はそのまま適合し、shop と診断コーパスの行・文字を変更する必要はありません。タスク D-02 だけは旧 `Array.get`（`Maybe` を返す）から同じ問いに答える現在の `Array.try_item` に改名します（`get` は現在トラップする取得）。shop の独立したシェル計算による golden は変更せず使います。std・文書の量は現 HEAD から測り直し、ハーネスの実出力を golden に写しません。
同じ基準で B-04 の `Array.swap_in` 呼び出し元は、旧 4 関数に Array の reverse/shuffle/sample と Vec の 6 関数が加わった 15 関数です。ソースの `fn` と参照行を独立した `awk` で照合し、問いは変えず `expect` を更新しました。残る B/F の 11 タスクは旧一覧と一致します。

### C12 の実装前の整合確認

C12 の付録は古い HEAD を基準にしています。実装では現在の D-59（対象は最後）と
`std/Map.tz`・`std/Set.tz`・`Vec` の公開 API を正とし、既存の全シグネチャと引数順を維持します。
付録のアルゴリズム・定数（512／256／384）・期待値は維持し、必要な呼び出しだけ現行 API に合わせます。
照合の結果、付録 A・B の公開シグネチャと `Vec` 呼び出しは現行 API と一致しており、旧順への差し戻しは不要でした。
追加する公開関数は `chunk_count` と `invariants_hold` だけです。古い release 前提や stack 増加は採用せず、
検証は現行の dev（opt-level 1）、性能比較は同じ `-O3` workload と旧実装の checksum で行います。
LLDB の実プロセス起動が使えないホストでもモックだけで完了とせず、Linux CI でも
`tests/debugger.mjs` を実行し、複数チャンクをまたぐ表示と既存の stepping を検証します。
複数 worktree の検証では `TSUZURI_CACHE_DIR` を各作業の `target/scratch/` 配下に分けます。
LLDB とサーバーの検証用 `TMPDIR` は、Unix socket のパス長制限に収まる、
リポジトリ内の短い**絶対パス**にします（相対パスは作業ディレクトリを変える試験では使えません）。
レビューで付録 B の `Set.select` が比較だけのためにもキーを値コピーすることが判明しました。
配列キーを不必要に clone しないよう、比較は借用で行い、選ばれた左キーだけを従来の `key_at` でコピーします。
この補正は公開 API・代表キー・NaN の検査を変えません。先に配列キーの回帰試験を追加し、
既存の WASM 16 MiB 上限で、入力と選択された結果は収まるが比較用の余分な clone は収まらない境界を検証します。

A18 上の統合では、2027 edition の読み取り既定に合わせ、消費する内部引数には `own`、
読み取りには `ref` を保ち、消費する Vec 操作は現行の `Vec.push_owned` に合わせます。
公開 API は積み元 A18 の実効規約を維持し、チャンクのアルゴリズム・定数・比較／drop の順序は変えません。
付録の std・fixture・benchmark の全文も同じ所有規約へ同期します。従来の性能表は記録した版の測定であり、
この所有規約の互換修正について新しい速度向上を主張しません。

### F14 の実装前の整合確認

F14 の確保数の契約は、古い付録より現在の API と D-59（対象は最後）を優先します。
`Alloc.since` の対象は `Stats`（最後の唯一の引数）で、`snapshot` と `measure` には
`Stats` の入力はありません。`std_argument_order` では `Alloc` を `Stats` の対象モジュールとして検査します。
契約 fixture の文字列検索も `String.contains needle text` の順に揃え、期待する確保数は変えません。
付録の `Array.reverse` は現在の `Array.rev` に、`Array.filter` の述語は現在の値引数
（`deref` なし）に読み替えます。42 契約の操作と期待値は維持します。
検索結果の補助表明だけは、現在の順序で `"jab"` が反復境界の `"j" + "ab"` に見つかることを
確認します（旧順の検索を前提にした `!found` は使いません）。確保数の期待値は変更しません。
既存の `tsuzuri_allocation_stats`（32 bytes、4 fields）、`Alloc` を使わない既定の heap の出力、
`--allocs` のない bench 出力を保持し、新しい Builtin は HEAD の一覧へ追加します（古い件数を使いません）。
検証は以下の dev バイナリと共有スロットを使い、性能の主張をしない変更に release ビルドは要求しません。

```sh
cargo test --locked --test alloc --test allocator --test bench --test alloc_contracts
# alloc_contracts は 42 契約を native/wasm32 × O0/O3 で検証し、期待値を壊した変異体が失敗することも確かめます。
```

Tsuzuri は言語仕様の正しさとパフォーマンスを保証するため、網羅的なテストスイートとマルチ言語ベンチマークを備えています。

開発用のリポジトリ KB（G32）は、現在の作業ツリーの件数・規約を索引にします。起票時の件数は固定せず、
Markdown の節は「次の同じ以下のレベルの見出しの直前まで」という仕様に従います
（付録の H1 の終了行の例より、この規則を優先します）。言語・ABI・生成 IR は変更しません。
優先度セルの `P1（D-50 の表）` のような注記も受理し、優先度は `P1` として索引にします。
第3期で増えた Phase ごとの依存は文章だけから強弱を推測せず、
[_features/README.md](_features/README.md) の依存関係図に明示された辺を優先して補います。
セルの原文も索引に残すため、Phase ごとの条件は `deps_raw` で確認できます。
`section <ID> commands` は関連する検証コマンドを、`section <ID> symbols` は関連ファイルから選んだ公開宣言を省略せず表示します。
子プロセス専用のヘルパーと、比較対象の旧ランタイムや明示的なワークロードを必要とする手動比較ランナーには、自動実行コマンドを生成しません。
検証スイートの `cases:` と `get cases()` はどちらも静的に検出し、索引のために JavaScript を実行しません。
8 MiB を超える入力は索引だけでなく、参照検査・公開宣言の照会・brief でも読み込み対象から除外します。必須の GUIDE 入力が過大な場合はエラーで止め、規則を黙って省いた brief は生成しません。
索引と入力ハッシュは同じ読取済み内容・採否を使います。読取後にサイズが 8 MiB 境界をまたいでもハッシュだけを切り替えず、次の鮮度検査で変更を検出します。
`--out` で選んだ出力ディレクトリとロックは入力から除外し、再構築ではプロセスごとに独立した一時ファイルを使います。
構築・鮮度検査・照会の読み取りは、出力の隣の `<out>.repo-kb-lock/` で世代全体を直列化し、異なる入力の索引を混在させません。
公開前に旧 manifest を無効化し、新 manifest を最後に公開するので、書き込み中断後に部分的な索引を fresh と判断しません。
2 分以内にロックを取得できなければエラーになります。強制終了で残ったロックは `owner` に記録された PID の停止を確認してから除去し、再実行してください。
JSONL を外部から直接読む場合は構築完了後に読み、並行する照会にはツールの読み取り API を使います。

### 何を回すか

変更に見合う範囲だけを回します。作業中は T0 と T1 を繰り返し、完了を報告する前に T2 を 1 回回し、T3 は CI に任せます。

開発時の読み替え（D13）: 古いチケットの例より、現在の対象を最後に取る API（D-59）を優先します（`Array.write 0 9 a`）。検証も現在の T0〜T3 に従い、dev バイナリと共有スロットを使い、stack の上限は増やしません。作業用の出力は `target/scratch/` に置きます。

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
| 数値・数学 | `tests/integers.rs`、`tests/integer_wrapping.rs`、`tests/overflow_report.rs`、`tests/math.rs`、`tests/f16_hardware.rs`（f16 の `half` 経路。D10） | `numeric_casts.mjs`、`integer_intrinsics.mjs`、`overflow_report.mjs`、`math.mjs`（重い。Node 24）、`f16_hardware.mjs`（`TSUZURI_F16_EXPECT` でホストが通る経路を固定） |
| コンパイル時評価（`const`・`const def`・静的データ） | `tests/constants.rs` | `features.mjs constants`・`const_functions`・`const_statics` |
| 生成 IR・ランタイム・閉包 | 変更した機能の Rust テスト、IR の決定性の確認 | `primitives.mjs`、`e2e.mjs`、`features.mjs`（全 suite）、`codegen_units.mjs`（codegen unit への分割。PB07。`TSUZURI_CODEGEN_UNITS=split` で、ほかの suite も分けたビルドで確かめられます） |
| スタックのフレーム・末尾呼び出し（PM09） | `tests/stack_frames.rs`（寿命の印と関数ごとの上限）、`tests/tail_recursion.rs`（`musttail`・drop の順序・対象外の形）、`tests/fast_backend.rs`・`tests/fast_backend_ops.rs`（`return_call` への翻訳） | `stack.mjs`（深さ 10,000,000 の相互再帰。native・`--backend fast`・`--wasm-feature tail-call`）、`control.mjs`（`TSUZURI_ASAN=1` も）、`user_drop.mjs` |
| Task・並列・WASM threads・同期 | `tests/tasks.rs`、`tests/parallel.rs`、`tests/concurrency.rs`（`Atomic`・`Mutex`・`Channel`・`Task.scope`） | `tasks.mjs`、`wasm_threads.mjs`、`wasm_threads_stress.mjs`、`trap_return_sync.mjs`、`features.mjs concurrency`・`channel`、`TSUZURI_TSAN=1` の実行 |
| トラップの報告・境界・`Domain`（E14・B10） | `tests/trap_locations.rs`、`tests/trap_backtrace.rs`（バックトレースとクラッシュレポート）、`tests/domain.rs`（`E1038` と IR） | `trap_return.mjs`（C の境界と `tests/domain_runtime.c`）、`domain.mjs`、`trap_boundary.mjs`、`stack_overflow.mjs`、`trap_return_sync.mjs`（`TSUZURI_NO_SANITIZERS=1` で sanitizer の build を外す） |
| SIMD・CPU dispatch・GPU | `tests/simd.rs`、`tests/cpu_dispatch.rs`、`tests/gpu.rs`、`tests/gpu_spirv.rs` | `simd.mjs`、`wasm_simd.mjs`、`cpu_dispatch.mjs`、`gpu.mjs`、`gpu_runtime.mjs`、`gpu_vulkan.mjs` |
| 行列・テンソル | `tests/matrix.rs`、`tests/matrix_views.rs`、`tests/tensor.rs` | `features.mjs matrix`・`matrix_view`・`tensor`、`benchmarks/run-matrix.mjs` |
| ネットワーク | `tests/net_api.rs` | `net.mjs`、`net_wasm.mjs`（Node.js 24） |
| host ABI・import | `tests/host_abi.rs`、`tests/host_imports.rs` | `host_imports.mjs` |
| driver・CLI・cache | `src/main.rs` の test module、`tests/modules.rs`、`tests/targets.rs`（`--target TRIPLE`・クロスの引数・実行器。G15）、`tests/api_diff.rs`（`tsuzuri api-diff`。G19）、`tests/find.rs`（`tsuzuri find`。G25） | `cache.mjs`、`examples.mjs`、`targets.mjs`（triple ごとに executed／linked／object を表示。`TSUZURI_CROSS_LINK=1`、`TSUZURI_CROSS_EXECUTE=1`） |
| 診断・警告 | `tests/diagnostics.rs`、`tests/warnings.rs`（W1005 を含む）、`tests/typed_holes.rs`・`tests/name_suggestions.rs`（型つき穴と未解決名の候補。G25） | — |
| LSP・docgen・debug | `tests/lsp.rs`、`tests/docs.rs`、`tests/doctest.rs`（`tsuzuri test --doc`。G34）、`tests/std_examples.rs`（std の公開関数の例の数と記法）、`tests/debug_info.rs` | `lsp_sessions.mjs`、`docgen.mjs`、`debug_info.mjs` |
| 言語内テスト | `tests/test_runner.rs` | `tsuzuri test` を使う E2E |
| `_tsuzuri/` の文書 | — | `node scripts/check-docs.mjs <変更したページ>`（引数なしで全体） |
| AI 向けガイド（`guide/`、`tsuzuri guide`、`tsuzuri new` の `AGENTS.md`・`llms.txt`） | `tests/guide.rs`、`src/guide.rs`・`src/guide_std.rs` の test module | `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-guide.mjs`（例を全 edition で `check`・`run`・`test`。上限の検査） |

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
target/release/tsuzuri test --doc std -O0 && target/release/tsuzuri test --doc std -O3   # std の全公開関数の例（G34）。std を変えたら、先に cargo build（std は埋め込み）
node tests/primitives.mjs target/release/tsuzuri
node tests/numeric_casts.mjs target/release/tsuzuri   # 全ビット幅の整数・符号と f32／f64 の間の型変換を BigInt の参照と照合
node tests/integer_intrinsics.mjs target/release/tsuzuri
node tests/overflow_report.mjs target/release/tsuzuri   # --overflow-report（A23）: fixture を native（C のホスト）と wasm32 の -O0／-O3 で BigInt と照合し、標準エラーの報告・tsuzuri_overflow_hit・.overflow.json を確かめる。TSUZURI_RANGE_FLAGS=audit でも同じ
node tests/math.mjs target/release/tsuzuri   # 重い。Node 24
npx --yes --package=node@24 node tests/wasm64.mjs target/release/tsuzuri   # wasm64（memory64）。Node 24
node tests/prebuilt_runtime.mjs target/release/tsuzuri   # runtime object の cold／warm・破損 fallback・no-cache・native／WASM O0/O3
node tests/size.mjs target/release/tsuzuri   # -Os／-Oz の出力一致・水準ごとの IR（native の関数値の inline 属性だけが違う）・キャッシュ・WASM の大きさ、埋め込み形式のリンク時 GC、--strip（nm・codesign・トラップ表）、整数の表示・書式指定・Parse（全型の境界値・参照との照合・IR・WASM）、--wasm-opt（偽の wasm-opt での呼び出し・キャッシュ・E2002。TSUZURI_TEST_WASM_OPT に本物があれば実行）とオプション誤り（TSUZURI_TEST_SECTION=名前の一部 で節を選べる）
node tests/integer_format.mjs   # 整数の表示・書式指定・Parse（integer.ll）を Python の独立した参照、8／16 bit の全値、numeric.ll の tz_soft_* と照合（native／WASM、O0／O3／Os／Oz。Clang と Python が必要でコンパイラは不要）
node tests/llvm_handoff.mjs target/release/tsuzuri   # 実行ファイルの到達する関数だけを Clang へ渡す（PB03 D1）。vtable・closure・export・trap 位置・型定義・剪定で通るようになるビルド・`run` の trap 計装
node tests/handoff_tools.mjs target/release/tsuzuri   # WASM threads／WASI／-g の C runtime の cache と失敗の hint、TSUZURI_LINKER（PB03）
node tests/bundle_identity.mjs target/release/tsuzuri   # 同梱ツールチェーンの識別を控える memo の無効化・上書き・cache 保護（PB03 WP-F）
node tests/pgo.mjs target/release/tsuzuri   # PGO（PR07）: 計装した実行ファイル・object・run の raw profile、--profile-use の directory と merge 済みの file、cache の鍵、古い profile の W2001、llvm-profdata の探索、wasm32 の writer（counts を native と照合）。LLVM の Clang と llvm-profdata が無いと skip（LLVM_PREFIX か TSUZURI_PGO_STRICT=1 では失敗）
TSUZURI_OBJDUMP=llvm-objdump node tests/stack.mjs target/release/tsuzuri   # 保証された末尾呼び出し（PM09）: 深さ 10,000,000 の相互再帰を native の -O0／-O3（--trap-info・-g も）、--backend fast（その feature の build。TSUZURI_STACK_FAST=1 で無いと失敗）、wasm32 の --wasm-feature tail-call（V8）で実行し、既定の WASM の出力に musttail・return_call・tail-call の機能がないこと、cache の鍵、拒否を確かめる。llvm-objdump が要る
node tests/codegen_units.mjs target/release/tsuzuri   # codegen unit（PB07）。小さいプログラムも分けて、単一のモジュールとの差分・決定性・cache からの組み立て・編集で変わる unit・公開 symbol・DWARF・wasm32・object、PM09 の保証された末尾呼び出しの unit をまたぐ深さ 10,000,000 の相互再帰、ホストの object の symbol との衝突、相対パスの TSUZURI_CLANG（LLVM の道具が無い検査は LLVM_PREFIX があると失敗になる。Windows は終了）
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
node tests/platforms.mjs   # 配布物と CI マトリクスが対応する 5 環境だけを含むこと
node tests/targets.mjs target/release/tsuzuri   # --target TRIPLE の 9 個を、実行・リンク・オブジェクトのどこまで確かめたか 1 行ずつ出す。TSUZURI_CROSS_LINK=1 と TSUZURI_CROSS_CLANG=<同梱の tsuzuri-clang> で、同梱ツールチェーンのリンクも（無ければ失敗）。TSUZURI_CROSS_EXECUTE=1 で Linux の qemu-user による静的 musl の実行も。CI は .github/workflows/targets.yml
node tests/windows.mjs target/release/tsuzuri.exe   # Windows x64 / Arm64（MSVC）。PowerShell で。native の -O0／-O3、ホストの MSVC triple の build・run・test、Win32 の task と失敗の注入、トラップの報告、UTF-8、hard link の拒否、既存出力の置換、object の ABI、WASM を確かめる。CI は .github/workflows/windows.yml（cache.mjs・examples.mjs・wasm_threads.mjs・cargo test も）
node tests/cpu_kernels.mjs target/release/tsuzuri
node tests/f16_hardware.mjs target/release/tsuzuri   # f16 の経路（ソフトウェア・F16C の変換・half 命令）を JS の参照実装とビット単位で照合し、実行した経路を表示する（D10）。--quick は件数を減らす。TSUZURI_F16_EXPECT=hardware|conversions|soft（既定の CPU と native を分けるなら generic=…,native=…。各値は `|` で複数許可できる）を付けると、期待した経路が実行されなかったときに失敗する。`.github/workflows/targets.yml` が macOS arm64・Linux x86-64・Linux arm64 の runner でこのコマンドを実行し、Linux x86-64 の `native` は runner の機能に応じ `conversions` または `hardware` を許可し、現時点で `conversions` の実行を確認済み（AVX512-FP16 の `hardware` 実行は未確認）。実行した経路は参照と照合するが速度は測らない。Windows の実行も未確認。cargo test --locked の tests/f16_hardware.rs は clang で生成コードを確かめる
node tests/packages.mjs target/release/tsuzuri
node tests/bindgen.mjs target/release/tsuzuri
node tests/debug_info.mjs target/release/tsuzuri   # llvm-dwarfdump が必要（TSUZURI_DWARFDUMP で指定できる）
node tests/debugger.mjs target/release/tsuzuri   # lldb が必要（TSUZURI_LLDB で指定できる）
node tests/debugger_static.mjs target/release/tsuzuri   # PM02: lldb が必要（TSUZURI_LLDB で指定できる）。プロセスを起動せず、-g の実行ファイルの型を lldb に読ませて、関数値と Task（descriptor と environment の 2 メンバー）を formatter の is_function が見分けること、同じ名前のフィールドを持つ record（古い 4 欄の形、descriptor が整数や ref の record）は見分けないこと、descriptor が closure.descriptor（code・clone・drop）を指すことを確かめる
node tests/frontend_cache.mjs target/release/tsuzuri
node tests/parallel_frontend.mjs target/release/tsuzuri   # PB04: TSUZURI_THREADS が 1・2・8・未設定のどれでも、診断（順序・重複の除去・上限）、IR、終了コード、frontend cache の bytes が同じこと（examples・fixtures・合成 200／1,000 module・上限を超える誤り・入れ子の上限）。不正な値は E2000 で終了コード 2。TSUZURI_BASELINE=<PB04 より前の compiler> を付けるとその compiler とも比べる
node tests/ai_cost.mjs target/release/tsuzuri   # G31: AI 作業コストの定義・独立 golden・導出・診断・外部実測・静的サイズ・KB/MCP アダプター。LLM なしで決定的。結果は docs/ai-cost.md
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

# 文書の生成ブロック（診断 kind・標準コレクションの計算量・benchmarks の表）と、文書の欠け・リンクの検査（G35。CI は docs.yml）
node tests/gen_docs.mjs && node scripts/gen-docs.mjs --check   # ずれたら node scripts/gen-docs.mjs で作り直す
node tests/check_cli_docs.mjs && node scripts/check-cli-docs.mjs   # --help の旗・サブコマンド・環境変数、診断コード、版、std の公開関数
node tests/benchmark_render.mjs && node tests/check_links.mjs && node scripts/check-links.mjs   # benchmarks の表と docs/benchmarks.md へのリンク

# 開発リポジトリの索引と GUIDE-LITE の参照検査（Node 20 以降）
node tests/repo_kb.mjs
node scripts/check-guide-lite.mjs
node scripts/repo-kb.mjs build
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
# A18 / A19 実装時の契約の照合（2026-10-23）

- A19 の照合: `own` は積み元の A18 で実装済みなので、通常の `def` でも引き続き受理します。
  D-53 の効果の語彙と純粋性の問い合わせ口を優先し、検査を最適化の属性には変換しません。
  Phase 2 の並行・メモ化・共通部分式の除去は、純粋性だけではトラップ・停止・一意所有を保存できません。
  保証のない呼び出し消去や結果共有は実装せず、必要な追加条件と未実装の範囲を区別します。
- A19 の後続項目のうち診断の強化は A18 と同じ補完マスクを共有します。
  明示の `ref` の診断は変えず、`pure def` の補完による拒否にだけ `own` の案内を付けます。
- A19 の明示した `Task`／`Parallel` の許可は、入力から不透明な関数・Task・dyn を受け取らない
  閉じた根に限定し、生成・参照するコールバックをすべて検査します。外部の問い合わせもこの根の条件を検査します。
  実行器の内部状態だけを例外とし、利用者の Atomic／Rc や非同期の時計・入出力は引き続き拒否します。
  通常の Array 呼び出しを並列化する変更ではありません。明示した並行 API の既存のトラップ・待機の意味を保ち、
  逐次との失敗順の同一性や停止は保証しません。純粋性だけで呼び出しを消す・並べ替える最適化は行いません。
- 利用者は A18 の D1〜D4 と、両チケットの全 Phase（設計だけの項目も含む）への着手を承認しました。
  A18 と A19 は別ブランチ・別 PR とし、A19 は A18 の上に積みます。
- D-54 の「edition は意味を変えない」は A18 D1 と衝突します。今回承認された例外は
  edition 2027 の読み取り・可変引数規約と、その規約を使う Array／List の callback 入口および
  `Vec.push` の排他更新・unit 戻り値です。2026 の所有権・借用・診断・生成 IR は保ちます。
  A22 の正準形の受理・拒否は今回変更しません。
- Phase 1 では `LATEST`・`MANIFEST_DEFAULT`・std を 2026 に保ちます。Phase 2 の既定版の更新は、
  移行ツールと std・fixture・文書の移行の検証後に限ります。古い manifest の既定は変更しません。
- 古いチケットの引数例より D-59（41f80bc7）の対象が最後という規則を優先します。
  `Array.update_at 0 value xs` など、現行 API の順序を使用します。
- `own` は共有構文を一度だけ実装します。純粋性だけを理由に LLVM 属性、未使用呼び出しやトラップの消去、
  所有結果の共有は許可しません。全 Phase の追加実装では、独立した既存のメモリ効果の証明もある整数呼び出しだけを局所的に再利用します。
- 新しい crate、stack・解析上限の引き上げは行いません。未実装の Phase を完了とは記録しません。
- A18 の review で明示 region を持つ record 引数の補完が既存の複数 lifetime 契約と衝突したため、
  明示 region の引数は固定署名として補完しません。region 推論や所有権規則は変えません。
  class method の whole-parameter `own` は、固定署名を変えずに印だけを除いて受理します。
- api-diff の既定は構文比較のまま保ち、edition による引数の既定の差を保守的な breaking change として報告します。
  明示した `--effective` では両方を依存込みで検査し、alias 解決後の型と実際の引数規約、純粋性の呼び出し段階、region の対応を比べます。
  editor は `own` と補完前の引数表記を保ち、無関係な型不一致には所有権の修正を勧めません。
- A18 Phase 2 の移行は、本体から所有権を推測せず、旧署名の値渡しを `own` で保存します。
  型変数・callback も明示し、後続の規約変更から旧 API を守ります。変更後を対象 edition で解析してから、
  通常ファイル・内容一致の保護付きでソースを書き、manifest は最後に更新します。
  単一ファイル入力では他のモジュールを無断で edition 移行しないため manifest を変更しません。
  manifest のない旧入力は `--from 2026` で明示します。G24 の一般の warning fix は今回実装しません。
  std は埋め込みソースと一致する開発用ツリーだけを移行対象にし、依存パッケージは書き換えません。
- 後から main に統合された G31 の固定コーパスは 2026 の測定契約を保ちます。
  コピーした検証用 project にだけ明示 edition の manifest を置き、元の 6 ファイル・行番号・手計算の
  golden は変更しません。現在の std を探すタスクは実際の `own` 署名へ追従させます。
  edition 移行だけの `fix` を G31 の一般の診断修正能力として数えません。
- A18 Phase 3 は型変数そのものを参照型に単一化せず、宣言ごとの読み取り規約を保持して
  引数 ABI にだけ共有借用を補います。generic／関数引数の本体は読み取り view を使い、
  値として取り出す場合の Copy 要件は既存の所有権推論に任せます。明示の `own` は従来の値 ABI です。
  矢印で書いた callback 型と型クラスの契約は既存のまま保ち、所有する callback 引数から
  読み取り callback へ安全に貸す adapter を挿入します。これにより `Array.map` は読み取り callback も受けられ、
  `map_ref` は互換 API として残します。adapter は callback の構築を再評価せず、借用結果の escape も既存の検査に通します。
- 規約は関数の引数リストの metadata として保持し、値の型変数へ押し込みません。
  関数を変数・戻り値・部分適用経由で渡しても、読み取り callback の lambda 束縛が参照値へ変わらないようにします。
  関数の引数リストは box に置き、`Type` の大きさを増やしません。LLVM へは従来と同じ物理引数型を渡します。
  2027 の `Array.map` は読み取り実装を使い、値 callback との変換は Copy と既存の借用検査で検証します。
  古い ticket の `map_ref`／`fold_ref` は現在の API 名 `mapr`／`foldr` と読み替えます。
  std 内部の既存の呼び出し規約は保存し、2027 の利用者側だけで読み取り API の入口を選択します。
- A18 Phase 4 は 2027 の whole-parameter `mut T` を `ref mut T` の ABI と値 view に展開します。
  明示の `ref mut` と 2026 の規約は保存します。2027 の可変な所有者または排他参照からの
  field・collection 要素への代入を許可します（文字列の要素は引き続き不変）。
  RHS、代入先、古い値の drop、store の順を保存し、添字は既存と同じ範囲検査を通します。
  射影先への書き込みは兄弟の loan を消しません。新旧 loan を保守的に合わせ、移動済みの射影は再初期化しません。
  要素単位の排他参照を追加するものではありません。`Vec.push` は 2027 では排他借用して unit を返し、
  従来の消費する操作は `Vec.push_owned` で明示できます。移行ツールは解決済みの builtin 参照だけを変更します。
- Phase 2 の最終切り替えでは `LATEST = 2027` とし、manifest なしの CLI 入力・新規パッケージ・
  script・REPL・文書・埋め込み std を新しい版で読みます。既存 manifest の省略時は引き続き 2026 です。
  Rust の版を受け取らない `analyze*` と単独 checker は既存の利用者の意味を保つ 2026 の互換入口とし、
  明示版の解析入口も提供します。テスト専用の環境変数や隠れた既定変更は使いません。
  `SourceInput`／project の明示 edition はそのまま優先します。
- 読み取り callback と値 callback の入口が異なる Array／List の map・mapi・iter・iteri・fold・
  fold_back は、2027 では読み取り側を既定に統合します。移行時の契約を保存する `_owned` 入口と、
  既存の `mapr`／`foldr` などの明示参照入口を残します。移行は解決された std 定義と builtin だけを対象にし、
  同名の利用者定義は変更しません。`get` と `get_ref` のように返す値の所有権が異なる API は別の契約のままです。
  std の内部呼び出しも消費する入口を明示してから移行し、2026 と同じ ABI と IR を検証します。
- D-54 の A22 向けの「2027 を追加しても LATEST は 2026」という記述は、A18 Phase 1 の足場の
  方針として扱います。全 Phase を承認された今回の既定版切り替えには適用しません。
  未実装の A22 の正準形拒否・一般の `check/build --edition`・`NEW_PROJECT` 定数は先取りしません。
  リリースして凍結した edition にあとから非互換な正準形拒否を追加することも許可しません。
- `Drop.drop` は既存の固定プロトコルを保ち、自己の field・要素への代入も禁止します。
  通常の mutable borrow と異なり destructor の呼び出しは frame をそのまま渡すため、
  新しい射影代入を無条件に許すと呼び出し元の stack buffer を解放し得ることを tracking allocator で確認しました。
  2026 の IR を変える一律の heap 昇格で隠さず、禁止を共有の ownership 検査で守ります。
  通常の可変な所有者の Drop field の置換と、置換後の destructor／field 解放は検証します。
- 既存の型名 `own` は型エイリアスと union payload でも保持します。凍結した 2026 の
  compiler で、これらの宣言と entry 式を同じ行に書く形も有効だと確認しました。
  whole-parameter の所有印を許可しないこの文脈では `own` を型名として読み、
  次の式を印の引数として取り込みません。新しい予約語や 2026 の文の区切りを追加しません。
- extern の返却型 `own` と後続の呼び出しも、2026 では改行の有無によらず有効でした。
  所有印の先読みは token の種類だけで決めず、既存の型 parser で候補の終端まで試し読みします。
  entry 式が続けられる宣言では、矢印などで引数と判定できる候補だけを印として扱います。
  parser の試し読みは token 列を共有し、再字句解析や token 列全体の複製をしません。
- Phase 3 の一時 callback の借用も、既存の「一段で完全適用し、結果が借用を持たない」という条件を保ちます。
  callback を受け取って関数値として返す旧 identity のような消費する契約には `own` を明示します。
  関数を返すからといって temporary operand を直接作り、借用の escape 検査を迂回しません。
- F14 の `Alloc.__counter` は割り当てではなく debug 効果として扱います。独立したブランチを統合するときは
  `Builtin::AllocCounter` の網羅 match に Debug を足し、counter の観測を pure から許可しません。

## A18 の完了（2026-10-24）

上の照合に基づき、A18 の Phase 1〜4（2027 と `own`、checked migration と既定版の更新、
generic／関数の引数別規約、`mut` と射影代入・版別 `Vec.push`）をすべて実装・検証しました。
[チケットと最終の完了報告](_features/_completed/A18-parameter-conventions.md)をアーカイブし、
[PR48](https://github.com/tatsuya-midorikawa/Tsuzuri/pull/48) は未マージのままにします。
最終の実装 head `b3343ed1` は Rust **1772 通過・2 skip**、全 features **19740 ケース**、
旧版と移行後各 **114 入力の byte 同一 IR**、リファレンス **52 頁・702 native 実行**、
CI **14 項目 green**。Copilot は未解決の指摘なしで、人による review は別途必要です。
固定 Drop protocol の自己置換と const evaluator の射影代入は拒否する安全な制限を残し、
新しい速度向上は主張しません。A19 の実装・統合は別担当・別 PR です。

## A19 全 Phase の追加設計の照合（2026-10-23、実装前）

- 利用者の全 Phase 承認に従い、旧 D8・D9・D12 の Phase 1 限定をここで更新します。
  generic、純粋 callback、明示の閉じた並行処理、`@cpu`、class／instance を実装対象とします。
  `const pure` は別の契約なので引き続き拒否します。D-53 の不明な呼び先を拒否する規則と解析上限は変えません。
- `pure (T -> U)` は既存の矢印型に呼び出し時の効果の保証だけを加えます。矢印型の物理的な引数規約は変えません。
  したがって無印の `pure def size :: string -> i64` の実効型は `pure (ref string -> i64)` です。
  通常の矢印型と同様、型の中に `own` は導入しません。純粋な関数値は通常の関数値に弱められますが、
  逆向きは閉じた関数／lambda の本体を検証できる場合だけです。未知の通常 callback の格上げは拒否します。
  関数を返す段階と返された関数を呼ぶ段階の保証を混ぜないため、純粋な戻り値の境界では矢印を平坦化しません。
- generic の型変数はテンプレートの保留条件として扱います。未使用の本体でも既知の外部効果を拒否し、
  実際の特殊化では `own` の plain-data 条件と全呼び先を再検証します。特殊化は元の純粋性の証明根を保持します。
  明示した純粋関数値の本体・捕捉も、未使用の generic template が捨てられる前に検査します。
  外側の通常 factory の効果を禁止したり、純粋性を付けていない戻り関数の実行まで純粋と要求したりはしません。
  未解決の型クラス呼び出しをそのままコード生成することや、未知の実行時呼び先を純粋と仮定することはしません。
- `IO` の構築だけは標準 `IO` の不透明な遅延アクションとして扱い、closure の本体を実行効果と混ぜません。
  捕捉値の評価・所有・解放は通常どおり検査します。`IO.Run` や取り出した未知の thunk の実行は許可しません。
  利用者の同名 record、通常の callback、Task の実行までこの例外を広げません。
- 2026-10-24 のレビュー照合: 純粋な宣言の呼び出し段階と実装の引数数を一致させます。
  `pure def f :: A -> B -> C` の実装は同じ段階で両引数を束縛し、本体の block から残りの関数を返して
  段階を短縮する形は `E1029` です。途中の計算を後ろへ移す eta 展開は行いません。
  途中で計算して関数を返す実装には `A -> (B -> C)`、返る段階も純粋なら `A -> pure (B -> C)` を使います。
  通常の `def` の途中評価、class の factory の戻り lambda、括弧で指定した段階は保持します。
- class の純粋メソッドの契約は既定実装とすべての instance に適用します。
  instance が独自に付ける純粋性も本体を検証します。未知の `dyn` dispatch をこの契約から推測しません。
- std のうち検証できる読み取り／数値 API に明示した純粋契約を付けます。
  通常の callback を受け取れる既存 API を一律に純粋 callback 専用へ狭めません。
- 最適化の共有は、意味上の純粋性と既存の実メモリ効果の証明の両方を使います。
  同じ基本ブロックで既に実行した、外部メモリに依存しない scalar 呼び出しの局所的な再利用だけを対象にし、
  最初の呼び出しを残します。これは停止・トラップの証明を省略した未使用呼び出しの削除ではありません。
  所有コレクションの共有、永続的なグローバル memo cache、投機実行、自動の GPU／並列 offload は行いません。
  明示した既存 backend の検査で同じ問い合わせを再利用し、未対応 backend のエラーと portable path を保ちます。
- API 比較は、既定の構文だけの比較を維持し、明示した `api-diff --effective` で
  解決した型・別名・補完後の引数規約を比較します。既定モードは従来どおり本体の型検査や依存の取得を必要としません。
  `--effective` は doc と同じ依存解決を使って両側を完全に解析し、解析・依存のエラーを構文比較で隠しません。
  純粋性の追加が実効 ABI も変えるときは互換な追加と報告しません。ABI が同じでも、generic template に
  純粋性の特殊化証明を新しく課す追加は保守的に破壊的とします。所有引数の plain-data 条件だけでなく、
  共有入力の不透明な callback／Task や、結果・呼び先を含む特殊化の効果検査もあるためです。
  特殊化義務のない、解決済みの共有入力や平たい所有型だけの追加とは区別します。
  非公開 helper を経由する保留義務も型引数の対応に沿って公開関数・class の既定実装・instance へ伝播し、再帰でも固定点まで求めます。
  通常の generic 関数内の明示的な純粋 callback についても、値の型・capture・実行する本体の型に由来する保留義務を同じ伝播へ含めます。
  具体型の呼び出しは、途中の通常 helper も含め既存の特殊化・効果検査で証明してから充足とし、不正な場合は解析エラーを返します。
  充足した部分を公開側の別の型変数へ誤って転嫁せず、型変数名の変更だけでは差分を出しません。
  純粋な factory の戻り段階、
  明示した領域、制約、型変数の対応と class メソッドの純粋契約を保ち、内部の package 識別子を公開しません。
  制約はソースの明示節だけでなく、型検査済みの inline／alias 経由の class 要求を解決し、
  呼び出し側に残る要求を集合として比較します。表記・順序・重複や、すでに満たされた本体内の具体型の要求では差分を出しません。
  モジュール関数の要求は推論・伝播された署名も含め、公開 alias 自体の制約も、ローカルな使用箇所の有無にかかわらず比較します。
  snapshot は所有権による Copy 推論と class 要求の固定点伝播が完了した後、generic 特殊化の直前に保存します。
  A18 の診断・移行・2026 の固定契約は保存します。
  全 Phase の実装とローカル受入を完了しました。初回は Rust 1,816 件（2 skipped）＋targets 12 件、
  features 20,012 cases（native／WASM × O0／O3）、既存 114 プログラムの IR byte 一致、
  リファレンス 12 ページ／75 例／134 native 実行を確認しました。
  PR #50 の 4 指摘を修正後、全 Rust 1,832 件（targets 12 件を含む、2 skipped）、
  pure 302＋parallel 12 ケース（native／WASM × O0／O3）、関連 E2E・cache・LSP と
  manual 4 ページ／32 例／58 native 実行を再確認しました。
  Windows CI で見つかった標準ライブラリ自身の実効 API 入力も、仮想パスを全 OS 共通の `std/Name.tz` に修正し、
  関連 62 テストと usage の例を確認しました。推論・公開 alias・Copy の要求を含む最終修正後は全 Rust 1,839 件（targets 12 件を含む、2 skipped）、
  Clippy・fmt・build、pure 302＋parallel 12（native／WASM × O0／O3）、E2E・cache・LSP、usage と docgen を再確認しました。
  未使用 generic の純粋 callback 検査の修正後は全 Rust 1,841 件（targets 12 件を含む、2 skipped）、
  Clippy・fmt・build、pure 302＋parallel 12、関連 E2E・cache・LSP・docgen と manual 2 ページ／29 例／52 native 実行を確認しました。
  AI 測定の字句的な callers／impact では宣言接頭辞を参照から除き、関数名 `pure` と `pure def` の修飾子を区別します。
  宣言と同じ行の本体にある通常・修飾付きの呼び出しは保持します。
  generic の純粋性の特殊化義務を実効 API に含めた最終差分で、全 Rust 1,843 件（targets 12 件を含む、2 skipped）、
  Clippy・fmt・build、pure 302＋parallel 12＋Map/Set 33、cache・LSP・docgen、AI 全 suite の Node20／24 各 44 件を確認しました。
  usage の 5 例／4 native 実行／1 test project と guide-lite 85 件も成功しました。
  最終 head の x64／arm64 CI は PR で追跡します。速度向上は未計測です。
  実装範囲・例外・検証コマンドと変更一覧は [A19 の完了記録](_features/_completed/A19-pure-functions.md)、
  最終 head の CI・Copilot レビューと 7 件の統合結果は各 PR で追跡します。
