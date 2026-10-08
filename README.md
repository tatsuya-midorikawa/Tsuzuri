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
- **マルチターゲット（Native & WASM）**: 単一のコードベースからネイティブ実行ファイル、C ABI 連携用の共有ライブラリ / オブジェクト、およびブラウザや Node.js で動作する WebAssembly を出力可能です。
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

現状は LLVM の CPU 最適化・自動ベクトル化と `--cpu native` に対応し、`Task.parallel` による明示的な CPU 並列処理も利用可能です。また、実験的な機能として厳密な整数演算に基づく WGSL 生成と WebGPU ホスト試作に対応しています（通常ランタイムへの実 GPU 接続や自動オフロードは未実装です）。

### 設計と実装済みの範囲

| 項目 | 初版の実装状況 |
| --- | --- |
| 状態管理 | 変数は既定で不変（`let`）。ローカル変数の再代入・置換は `let mut` と排他借用 `ref mut T` で行います。標準入出力と OS API は `IO`、外部連携は `extern` で分離し、共有可変状態は持ちません。 |
| 型システム | 基本数値型（`bool`, `unit`, `i8`〜`i128`, `i8u`〜`i128u`, `f16`〜`f128`, `d32`〜`d128`, `byte`/`ubyte`/`sbyte`）、任意精度整数 `bigint`、ECMA-262 準拠の UTF-16 `string`、UTF-8 バイト列 `utf8string`、文字型 `char`/`utf8char`、タプル、不変レコード、共用体（`union`）、配列、連結リスト、環境を捕捉する関数値に対応。 |
| 構文と表現 | `def ... = ラムダ式`、カリー化と部分適用、`\引数 -> 式`、`if…then…else`、`match` とガード式、`for…in`、`for…to`／`downto`、`while…do`、`break`／`continue`、レコード更新、インデント構文、パイプライン `\|>`、関数合成 `>>`／`<<`、組み込み関数 `not`／`ignore`、累乗演算子 `**`、ビット演算（`&&&`／`\|\|\|`／`^^^`／`~~~`／`<<<`／`>>>`）、数値接尾辞、高階関数、明示的再帰（`rec`／`and`）。 |
| 多相性 | `'a` によるパラメトリック多相、ジェネリックなレコード・union、透過的な型エイリアス（`type`）、型クラスおよび具体型インスタンスによるアドホック多相。制約推論と静的単相化。ランク1高階型（HKT）。`dyn C` と `Dyn.of` による vtable を使った動的ディスパッチ。 |
| コンピュテーション式 | `.tc` によるユーザー定義ビルダー。明示的ブロックと型推論による暗黙本体。束縛・短絡・分岐・反復を標準の関数呼び出しへ展開。 |
| タスクと並列処理 | `task { ... }`、`let!`／`return`／`return!`／`do!`。所有値を持つ一回実行の計算を組み合わせ、`Task.parallel` でスレッド数を制限して安全に並列実行。 |
| モジュール構成 | 1 ファイル = 1 モジュール。同一ディレクトリ内の自動解決と `Main.tz` によるエントリーポイント。ローカルパッケージ、commit 固定の git 依存、自前の index による registry 依存と最小版選択（`Tsuzuri.toml`、`tsuzuri fetch`、`tsuzuri publish`、`Tsuzuri.lock`）。 |
| メモリモデル | 所有権の移動（move）と借用検査（`ref T`／`ref mut T`、Rust 互換の `&T`／`&mut T` も可）。明示的なヒープ確保（`new`）とスタック配置の区別。文字列・配列・リスト・環境の自動解放、`instance Drop` による RAII（GC は不使用。共有は明示的な `Rc` / `Arc`）。 |
| 最適化 | 既定で LLVM `-O3`、自動 SIMD 化、基本数値変換の直接 lowering。`--cpu native` によるビルド機向け最適化。直接の自己末尾再帰は `-O0` でもループ化。 |
| 安全性 | ゼロ除算や配列・リスト境界アクセスの実行時検査。LLVM の未定義動作に依存しない数値仕様。`@checked` による整数オーバーフローは `try` で `Result` に変換可能。 |
| ホスト連携 | スカラー・バッファ・レコードの C ABI 連携および WebAssembly（WASM）のエクスポート／インポート。`extern` のリンク名指定・不透明ハンドル・静的コールバック、ネイティブのホストリンク。標準入出力と OS API は `IO`、UI やネットワークはホスト側に委譲。 |
| 開発・AI 支援 | 明示的な関数シグネチャ、暗黙の型変換の排除、位置情報付き JSON 診断、決定的な IR 出力。公式 LSP、フォーマッター、テストランナー。 |

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
- **浮動小数点型**: `f16`、`f32`、`f64`、`f128`。十進浮動小数点型として `d32`、`d64`、`d128` もサポートしています。
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
- **連結リスト (`[|T|]`)**: 単方向の不変連結リストです。リテラルは `[|1, 2, 3|]`、空リストは `[||]` です。要素への添字アクセスは $O(n)$ となります。
- **ベクター (`Vec<T>`)**: 伸縮可能な所有バッファです。
- **順序付きマップ・セット (`Map<K, V>` / `Set<K>`)**: 平衡二分探索木による不変コレクションです。キーの順序を保持し、検索は $O(\log n)$、挿入および削除は $O(n)$ で動作します。
- **ハッシュマップ・ハッシュセット (`HashMap<K, V>` / `HashSet<K>`)**: 平均 $O(1)$ で検索・挿入・削除が可能なコレクションです。挿入順序が保持されます。詳細は [HashMap](_tsuzuri/language-reference/built-in-types-and-modules/hashmap.md) / [HashSet](_tsuzuri/language-reference/built-in-types-and-modules/hashset.md) を参照してください。
- **JSON (`Json`)**: RFC 8259 に厳密な `Json.parse`、決定的な `Json.to_utf8string`、字句を保つ数値 `Json.Numeral`、組み込み型クラス `Encode` / `Decode` と `Json.serialize` / `Json.deserialize` を提供します。入力 64 MiB・入れ子 128 段の上限を超える入力も `Result` のエラーで返します。`@json "名前"` による名前の変更、字下げした出力、木を作らないプル型の解析器と逐次の出力器、同じ値の CBOR（[Cbor](_tsuzuri/language-reference/built-in-types-and-modules/cbor.md)）も提供します。詳細は [Json](_tsuzuri/language-reference/built-in-types-and-modules/json.md) を参照してください。
- **シーケンス (`Seq<T>`)**: 一度だけ消費可能な遅延反復ストリームです。`Seq.unfold`、`Seq.map`、`Seq.filter`、`Seq.to_array` などを提供します。
- **共有ポインタ (`Rc<T>` / `Arc<T>` / `Rc.Weak<T>` / `Arc.Weak<T>`)**: 参照カウントで値を共有します。所有者は `Rc.share` で明示的に増やし、`Arc` は atomic な計数で複数のタスクから読めます。詳細は [Rc と Arc](_tsuzuri/language-reference/built-in-types-and-modules/rc.md) を参照してください。
- **Arena (`Arena<T>` / `Arena.Handle<T>`)**: 値をまとめて所有し、Copy の世代付きハンドルで指すコンテナです。グラフや循環する構造を GC や参照カウントなしで表し、削除済み・別の arena のハンドルを実行時に検出します。詳細は [Arena](_tsuzuri/language-reference/built-in-types-and-modules/arena.md) を参照してください。
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

- **ジェネリックレコード**: フィールド型に型パラメーターを持つレコードを宣言できます。型パラメーターおよび型引数は常に `Name<'a, 'b>` や `Name<i64, string>` のように `<...>` で括って記述します。詳細は [ジェネリックなレコード](docs/language.md#ジェネリックなレコード) を参照してください。

```text
record Pair<'a, 'b> { first: 'a, second: 'b }

def swap :: Pair<'a, 'b> -> Pair<'b, 'a> = \pair ->
    Pair { first: pair.second, second: pair.first }

let pair: Pair<string, i64> = swap (Pair { first: 42, second: "answer" })
pair.second
```

- **代数的データ型 (`union`)**: 各ケース（variant）は 0 個または 1 個のペイロードを持ちます。ペイロードを持つケースは 1 引数のコンストラクタ関数値としても扱えます。

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
- **再帰関数**: 再帰関数は `def rec name :: ...`、相互再帰は `and name :: ...` で繋いで記述します。直接の自己末尾再帰は、`-O0` であっても LLVM レベルでループ構造へと自動変換されます。詳細は [言語仕様: 制御構文](docs/language.md#制御構文) および [制御構文のベンチマーク](docs/benchmarks.md#制御構文の比較) を参照してください。実行例は `tsuzuri run examples/control` で確認できます。

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
- **高度な演算**: 複数ソースの並行的な合成を行う `and!` や `match!`、およびビルダーが提供する `MergeSources` / `BindReturn` / `Bind2` に対応しています（`and!` の各項は左から右へ順に一度ずつ評価され、勝手な自動並列化は行われません）。
- **暗黙のコンピュテーション式**: 通常の関数や `main` の本体において、明示的なビルダーブロックを省略して `let!` や `do!` を直接記述することも可能です。コンパイラが戻り値の型に基づいて適切なビルダーを自動合成します。詳細は [暗黙の計算式](docs/language.md#ビルダー名を省略した本体) を参照してください。
- **最適化**: スコープを脱出しないローカルな継続はコンパイラによって特殊化・インライン化され、不要なクロージャのメモリ確保や間接関数呼び出しが完全に消去されます。詳細は [ビルダーの仕様](docs/language.md#コンピュテーション式) および [コンピュテーション式の比較](docs/benchmarks.md#コンピュテーション式の比較) を参照してください。実行例は `tsuzuri run examples/computations` で確認できます。

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
- **安全なスレッド分離**: タスクが捕捉する値は所有権の移動（move）または Copy に限定され、参照と `Rc` の持ち込みはコンパイル時に拒否されます。これにより、共有可変状態によるデータ競合の発生を根本から防ぎます。読み取り専用のデータは、atomic な計数を持つ `Arc` でタスク間に共有できます。
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

#### エントリーポイント (`Main.tz`)

アプリケーションは `Main.tz` から開始します。
エントリーポイントとして、`def main :: unit -> i32` またはコマンドライン引数を受け取る `def main :: Array<string> -> i32` を定義します。`main` が返す `i32` の値がプロセスの終了コードとなり、コンソールに自動表示されることはありません（終了コードが 0 以外の場合は `tsuzuri run` が `E2005` で報告します）。
トップレベルの `let` 式および最後の結果式で記述されたプログラムも引き続き実行可能で、結果値が `Display` を実装していれば標準出力に出力されます。

#### コンパイル時定数 (`const`)

`const Answer: i64 = 40 + 2` のように、型注釈付きのコンパイル時定数を宣言できます。
整数および浮動小数点の演算、文字列、配列、タプル、レコードリテラルを扱うことができ、モジュールを超えた修飾参照や前方参照にも対応しています。

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
| `.tc` | コンピュテーション式のビルダー定義および関連補助関数。トップレベル実行コードは配置不可。 |

拡張子が異なっていても同名のモジュールを同一ディレクトリに共存させることはできません（例: `Checked.tz` と `Checked.tc` の併存はエラー）。

#### モジュールの可視性と予約モジュール名

- **可視性**: 宣言は既定で外部モジュールへ公開されます（public）。モジュール内でのみ使用する補助関数やデータ構造には `private` キーワードを付与します。ホスト言語（C や WebAssembly）へシンボルを公開する場合は `export` を指定します。
- **標準ライブラリの名前空間 `std`**: 標準ライブラリは名前空間 `std` に属しており、`std::Maybe` や `std::Task.run` のように完全修飾することも可能です。以下の名前は予約されており、ユーザーが同一名のモジュールを作成することはできません（`E1011` エラー）。

| 予約名 | 用途 |
| --- | --- |
| `Maybe`, `Result` | 成功・失敗および値の存在・欠落を表現する基本データ型 |
| `Array`, `List`, `Vec`, `Map`, `Set`, `HashMap`, `HashSet`, `Arena` | 各種コレクションおよびデータ構造 |
| `Rc`, `Arc` | 参照カウントによる共有所有 |
| `String`, `Utf8String`, `Char` | UTF-16 / UTF-8 文字列および文字操作 |
| `Regex`, `Unicode` | 線形時間の正規表現、Unicode 17.0.0 の文字データ |
| `Math`, `Int` | 高精度数学関数、浮動小数点超越関数、整数組み込み演算 |
| `Debug`, `Test` | デバッグ出力およびテストフレームワーク |
| `Parallel`, `Simd`, `Gpu` | データ並列処理、128-bit・256-bit SIMD 演算、GPU カーネル連携 |
| `File`, `Dir`, `Path`, `Env`, `Time`, `Random`, `Os`, `Process` | ファイル、環境変数、システム時刻、プロセス管理などの OS API |
| `Format` | 文字列補間およびカスタムフォーマット用ヘルパー |
| `Json` | JSON の解析・出力と `Encode` / `Decode` による値の変換 |
| `Cbor` | `Json.Value` の CBOR（RFC 8949）の読み書き |

#### パッケージ管理 (`Tsuzuri.toml`)

プロジェクトルートに `Tsuzuri.toml` を配置することで、パッケージ名、既定の名前空間、およびローカルや git リポジトリの依存パッケージを定義できます。

```toml
[package]
name = "app"
version = "0.1.0"
namespace = "Acme::App"

[dependencies]
geometry-core = { path = "../geometry-core" }
shapes = { git = "https://example.org/shapes.git", rev = "0123456789abcdef0123456789abcdef01234567" }
tiles = { version = "1.2.0" }

[registry]
index = "https://example.org/tsuzuri-index.git"
```

依存パッケージのモジュールは `GeometryCore::Point`（依存先が `namespace` を持てばその名前空間）のように完全修飾名で参照します。git 依存は commit を固定し、registry 依存（`version`）は `[registry]` の git の index から最小版選択で版を決めます。`tsuzuri fetch app` が `git` で取得して内容の SHA-256 と選んだ版を `Tsuzuri.lock` に記録します。index は利用者や組織が置き（Tsuzuri は公開の registry を運営しません）、`tsuzuri publish` が index に足す項目を出力します。ネットワークに触れるのは `fetch` と `publish` だけで、`check`・`build`・`run` などは `Tsuzuri.lock` とキャッシュ内のストアだけを読み、内容の一致を検証します。詳細は [ローカルパッケージの仕様](docs/language.md#ローカルパッケージ) と [パッケージ](_tsuzuri/language-reference/organizing-tsuzuri/packages.md) を参照してください。

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
- **共有ライブラリと各言語のバインディング**: `--emit shared` は公開 C ABI（`tz_*`、`tsuzuri_alloc`、`tsuzuri_free` など）だけを export する共有ライブラリ（macOS は `.dylib`、Linux は `.so`。Windows は G10 待ちで `E2000`）を出します。`--emit bindings-cs`、`--emit bindings-py`、`--emit bindings-cpp` は、それを呼ぶ C#（`[LibraryImport]` と `SafeHandle`）、Python（`ctypes`）、C++20（C ヘッダーの上の RAII）のバインディングを同じソースから生成します。`--trap-mode return` を付けると、トラップが各言語の例外になります。
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
- **マルチスレッド (`threads`)**: `--wasm-feature threads` を指定することで、Task や Parallel による並列計算を Web Worker や Node.js の Worker Threads に分散できます。詳細は [Webホスト要件](examples/web/README.md) を参照してください。

### GPU カーネル連携（実験的）

単一の `export` された `i32 -> i32` または `i32u -> i32u` カーネルを含むプロジェクトから、`tsuzuri build Kernel.tz --emit wgsl -o kernel.wgsl` により WebGPU 向けの WGSL シェーダーを生成できます。
厳密な整数演算に基づく Phase 1 実装であり、自動オフロードや速度優位を保証するものではありません。詳細は [GPU 仕様](docs/language.md#gpu-kernel実験的-phase-1) を参照してください。

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
```

テストコードは型検査されますが、通常の実行可能バイナリには含まれません。また、テスト実行時は別プロセスで隔離されるため、安全に並行テストを行えます。

### ドキュメント生成 (`tsuzuri doc`)

ソースコード内の `///` ドキュメントコメントを抽出し、公開 API の Markdown ドキュメントを自動生成します。

```sh
# 公開 API ドキュメントを生成
tsuzuri doc src/ -o docs/api
```

### C ヘッダーからの extern 生成 (`tsuzuri bindgen`)

C のヘッダーを Clang で解析し、ヘッダー自身の関数・enum の定数・整数の `#define`・struct・typedef から、リンク名付きの `extern "symbol" def`・`const`・`record`・`extern type`・`type` を持つモジュールを生成します。不透明な struct のポインターはハンドル、関数ポインターの引数は静的コールバックになり、`--buffer 関数:ポインター:長さ` で指定した組は `ref [T]`、`--consume 関数:引数` で指定したハンドルはムーブになります。ABI の一致を確かめられない宣言（可変長引数、注釈のないポインター、値渡しの struct、大域変数など）は、理由付きの `// skipped` 行と警告 `W2002` になります。出力は決定的で、`// Generated by tsuzuri bindgen. Do not edit.` で始まるファイルだけを上書きします。

```sh
tsuzuri bindgen vendor/sample.h -o Sample.tz --include-dir vendor --buffer sample_sum:values:count
```

### デバッグ情報と出力仕様

- **DWARF デバッグ情報**: `build` や `run` に `-g`（`--debug-info`）を付与することで、関数・行番号・変数・型の DWARF 情報を埋め込めます。macOS では `.dwarf` ファイルが生成され、LLDB などのデバッガでシンボルを解決可能です。
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

LLVM の `clang`、Windows SDK、および Visual Studio Build Tools（MSVC / C++ ツールセット）を用意し、Developer PowerShell 上でビルドを行います。
コンパイラは `--target=x86_64-pc-windows-msvc` を指定してネイティブ実行ファイルを生成します。

```powershell
cargo build --release
```

`clang` や `wasm-ld` のパスは環境変数 `TSUZURI_CLANG`、`TSUZURI_WASM_LD` で明示的に指定することも可能です。

---

## CLI リファレンス

### コマンド書式

```text
tsuzuri check source.tz|source.tt|source.tc|directory [--json]
tsuzuri [build] source.tz|source.tt|source.tc|directory [options]
tsuzuri run Main.tz|directory [-O0|-O1|-O2|-O3] [--cpu generic|native] [--json]
tsuzuri test source.tz|directory [options]
tsuzuri fmt source.tz|directory [--check] [--json]
tsuzuri doc source.tz|source.tt|source.tc|directory -o outdir [--json]
tsuzuri new directory [--namespace NAME]
tsuzuri fetch directory [--json]
tsuzuri publish directory --git URL --rev COMMIT [--json]
tsuzuri bindgen header.h -o Module.tz [--include-dir DIR]... [--buffer F:P:L]... [--consume F:P]... [--json]
tsuzuri lsp
```

### 主要オプション一覧

| オプション | 説明 |
| --- | --- |
| `-o`, `--output PATH` | 出力先パスを指定します（親ディレクトリは自動作成されます）。 |
| `--target native\|wasm32\|wasm64` | ターゲット環境を指定します（既定: `native`。`wasm64` は 64-bit 線形メモリ）。 |
| `--emit exe\|object\|llvm\|header\|wasm\|wgsl\|shared\|bindings-js\|bindings-cs\|bindings-py\|bindings-cpp` | 出力成果物の種類（既定: native は `exe`、WASM は `wasm`）。`bindings-js` は `--target wasm32` で JavaScript のグルー `<name>.mjs` と TypeScript 宣言 `<name>.d.mts` を出します（`--wasm-feature threads` でスレッドプール版）。`shared` は native の共有ライブラリ、`bindings-cs`／`bindings-py`／`bindings-cpp` はそれを呼ぶ C#／Python／C++ のバインディングです。 |
| `-O0` ～ `-O3` | 最適化レベル（既定: `-O3`。高速化のために精度を損なう fast-math などは使用しません）。 |
| `--cpu generic\|native` | CPU 命令セットの特化（既定: `generic`。`native` はビルド機の命令セットとスケジューリングに最適化）。 |
| `--deny-warnings` | 警告が存在する場合にコンパイルを失敗させ、コード生成や実行を行わずに停止します。 |
| `--trap-info` | 配布用ビルドにおいて、実行時トラップの正確なソース位置情報を保持します。 |
| `-g`, `--debug-info` | DWARF デバッグ情報を付与します。 |
| `--wasm-max-memory SIZE` | WASM の最大線形メモリサイズ（既定: 16MiB、例: `256MiB`）。 |
| `--wasm-stack-size SIZE` | WASM のスタックサイズ（既定: 1MiB、例: `4MiB`）。 |
| `--wasm-host wasi` | wasm32 において、標準入出力および OS API を WASI preview1 のインポートへ接続します。 |
| `--wasm-feature simd128\|threads` | WebAssembly の追加機能（128-bit SIMD、Worker スレッド分散）を有効化します。 |
| `--allocator system\|host\|counting` | ヒープ確保の行き先（既定: `system`）。`host` はホストが定義する `tsuzuri_host_alloc`・`tsuzuri_host_free`・`tsuzuri_host_realloc` を呼び、`counting` は確保の数を `tsuzuri_alloc_stats` で返します（object・LLVM IR・header・WASM 出力のみ）。 |
| `--freestanding` | C ライブラリに依存しない native の object・LLVM IR・header を出力します（`--allocator host` が必須）。 |
| `--no-cache` | `build` と `run` で、ビルド成果物キャッシュと構文解析の結果のキャッシュ（frontend cache）の読み書きをやめます。 |
| `--json` | 診断情報やテスト結果を 1 行 1 JSON オブジェクト形式で標準エラー出力へ返します。 |

### 入力とエラー報告の仕様

- **入力の解決**: ファイルまたはディレクトリを 1 つ指定します。ディレクトリを指定した場合は、直下の `Main.tz` が自動的にエントリーポイントとして選ばれます。ファイル指定時はその親ディレクトリをルートとし、配下の全 `.tz`・`.tt`・`.tc` を相対パス順に再帰的に探索して読み込みます。
- **一括エラー報告**: コンパイラは独立した複数の型エラーや構文エラーを収集し、ファイル名およびソース位置順にまとめて報告します（最大 50 件まで表示、残りは件数のみ通知）。二次エラーは抑制され、エラーが存在する限りコード生成や実行は行われません。
- **ビルドキャッシュ**: ビルドおよび実行時のアーティファクトキャッシュは既定で有効です。ソース、コンパイラ、ツールチェイン、設定内容の SHA-256 ハッシュをキーとして管理し、変更のないモジュールの再コンパイルを回避します。キャッシュ保存先は環境変数 `TSUZURI_CACHE_DIR` でカスタマイズでき、`--no-cache` で無効化できます。同じ保存先の `packages/` には `tsuzuri fetch` が取得した git と registry のパッケージが置かれ、キャッシュの掃除の対象外です。
- **frontend cache**: `check`・`build`・`run`・`test`・`doc` は、構文解析の結果を同じ保存先の `frontend/` にプロジェクトごとに保存し、内容の変わらないソースを構文解析し直しません。出力（IR、診断とその順序、警告、終了コード）はキャッシュの有無で変わりません。`--no-cache` は `build` と `run` だけで受け付け、ほかのコマンドでは `TSUZURI_CACHE_DIR=`（空）にすると両方のキャッシュを使いません。

---

## 検証と性能測定

Tsuzuri は言語仕様の正しさとパフォーマンスを保証するため、網羅的なテストスイートとマルチ言語ベンチマークを備えています。

### テストスイートの実行

```sh
# 基本的なコードベース検証
sh scripts/check-runtime-includes.sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked

# 統合 E2E テスト・言語機能テスト
node tests/e2e.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/strings.mjs target/release/tsuzuri
node tests/tasks.mjs target/release/tsuzuri
node tests/features.mjs target/release/tsuzuri
node tests/json.mjs target/release/tsuzuri
node tests/computations.mjs target/release/tsuzuri
node tests/control.mjs target/release/tsuzuri
node tests/lsp_sessions.mjs target/release/tsuzuri
node tests/io.mjs target/release/tsuzuri
node tests/os.mjs target/release/tsuzuri
node tests/cpu_kernels.mjs target/release/tsuzuri
node tests/packages.mjs target/release/tsuzuri
node tests/bindgen.mjs target/release/tsuzuri
node tests/frontend_cache.mjs target/release/tsuzuri

# WebAssembly & GPU テスト
node tests/wasm_threads.mjs target/release/tsuzuri
node tests/wasm_memory.mjs target/release/tsuzuri
node tests/bindings.mjs target/release/tsuzuri   # 生成グルー（TSUZURI_TSC で TypeScript の bin/tsc を指定できる）
node tests/bindings_threads.mjs target/release/tsuzuri   # スレッドのグルー（TSUZURI_BROWSER か TSUZURI_PLAYWRIGHT で実ブラウザも）
node tests/host_bindings.mjs target/release/tsuzuri   # 共有ライブラリと C#・Python・C++ のバインディング（dotnet が無ければ C# を飛ばす）
node tests/gpu.mjs target/release/tsuzuri

# 言語リファレンス（_tsuzuri/）のリンクと例の検証（ページを指定すると、そのページだけ）
node scripts/check-docs.mjs
```

機能を追加・変更・修正・削除したときは、同じ変更で [言語リファレンス](_tsuzuri/language-reference/index.md) の関連ページも最新にします（手順は [_features/GUIDE.md §8](_features/GUIDE.md#8-ドキュメント更新規約)）。

### ベンチマーク測定

C/C++、Rust、C#、JavaScript との 36 種目にわたる詳細な性能比較スクリプトを提供しています。

```sh
# 基本ベンチマークの実行
node benchmarks/run.mjs target/release/tsuzuri
node benchmarks/run-cpp.mjs target/release/tsuzuri
node benchmarks/run-control.mjs target/release/tsuzuri
node benchmarks/run-computations.mjs target/release/tsuzuri
node benchmarks/run-managed.mjs target/release/tsuzuri --scale 0.1
```

測定条件の詳細、対応範囲、比較対象の言語との差異、再現手順については [docs/benchmarks.md](docs/benchmarks.md) を参照してください。

---

## 関連ドキュメント

- [言語仕様と ABI](docs/language.md) — 言語の完全な構文仕様、メモリモデル、ホスト連携規約
- [コンパイラ構成と開発指針](docs/architecture.md) — パイプライン設計、IR 生成、コントリビューション基準
- [ベンチマークの条件と読み方](docs/benchmarks.md) — 各種目における測定環境と性能データ
