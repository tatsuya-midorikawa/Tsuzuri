# Tsuzuri

**AI と人間が、少ない暗黙ルールで堅牢かつ高パフォーマンスなプログラムを書けることを目指す、関数型の式と所有権モデルを備えたプログラミング言語。**

コードは **`.tz`**、型クラス宣言は **`.tt`**、コンピュテーション式のビルダー実装は **`.tc`** に記述します。Rust 製のフロントエンドで厳密な型検査を行い、LLVM をバックエンドとして高効率なネイティブコードおよび WebAssembly を生成します。

- **[公式日本語ドキュメント](_docs/README.md)**: [入門ガイド](_docs/get-started.md) · [言語リファレンス](_docs/language-reference/README.md) · [標準ライブラリ](_docs/library-reference/README.md) · [実践ガイド](_docs/guides/README.md) · [開発ツール](_docs/tools/README.md) · [機能対応状況](_docs/feature-status.md)
- **[VS Code 拡張機能](vsc/README.md)**: コンパイラ・LLVM・SDK を統合した VSIX パッケージを提供。シンタックスハイライト、コード補完、定義ジャンプ、リファクタリング、ビルド・実行、テスト、ソースデバッグに対応しています（対応アーキテクチャ: x64 / ARM64。詳細は [vsc/Development.md](vsc/Development.md) を参照）。

---

## 主な特徴

- **所有権と借用による安全なメモリ管理**: ガベージコレクション（GC）や参照カウントに頼らず、Rust と同様の所有権移動と借用検査（`ref` / `ref mut`）によってコンパイル時にメモリ安全性を保証します。
- **直感的で表現力豊かな関数型構文**: カリー化、強力な型推論、代数的データ型（`union`）、網羅性を検証するパターンマッチ、パイプライン演算子（`|>`）、関数合成（`>>` / `<<`）をサポートしています。
- **ゼロコストの型クラス**: 型クラスはコンパイル時に静的に単相化（モノモーフィゼーション）され、動的ディスパッチやランタイム辞書のオーバーヘッドを生じさせません。
- **拡張可能なコンピュテーション式**: `.tc` ファイルでビルダーを定義することで、`Result` や `Option`、非同期処理などのモナディックな制御フローを言語本来の構文のように扱えます。
- **安全で軽量な並列処理**: 遅延評価・一回実行のタスク（`task { ... }`）と常駐スレッドプールにより、データ競合のない安全な並列計算を提供します。
- **LLVM による高い実行性能**: 既定で LLVM `-O3` による最適化を適用し、自動ベクトル化や `--cpu native` によるターゲット最適化に対応しています。
- **マルチターゲット（Native & WASM）**: 単一のコードベースからネイティブ実行ファイル、C ABI 連携用の共有ライブラリ / オブジェクト、およびブラウザや Node.js で動作する WebAssembly を出力可能です。
- **充実した開発支援ツール**: 公式言語サーバー（LSP）、自動フォーマッター、単体テストランナー、Markdown ドキュメント生成ツールを標準で同梱しています。

---

アクティブパターンは bool／Option を返す部分形式と、宣言した union に対応する複数ケース形式を使えます。
`def (|Parsed|_|) :: ref string -> Option<i64> = \text -> Parse.parse text` により、`Parsed value` で解析結果を照合できます。

### コード例 (`Main.tz`)

`Debug.print value` は借用して表示し、`Debug.trace value` は表示して同じ所有値を返します。native は stderr、WASM は既定で no-op です。
WASM の `--debug-output` を使う場合は、[Debug のホスト契約](docs/language.md#デバッグ出力) に従って `tsuzuri_debug.write` を提供します。

標準入出力は `IO<T>` の遅延アクションで扱います。`IO { do! IO.write_line "Hello" }` を Main.tz の入口にすると実行し、`let! line = IO.read_line ()` で EOF を区別して読み取れます。`IO.writeln` は `IO.write_line` の別名です。
ビルダーブロックを省略して通常の関数・匿名関数・main の本体へ `let!`／`do!` を直接書くこともでき、IO と Option／Result／独自ビルダーを型に基づいて合成します。[暗黙の計算式](docs/language.md#ビルダー名を省略した本体)を参照してください。
結果型がビルダーを持たない本体（`def main :: i32` など）と `try` の中では、IO の `let!`／`do!` をその場で実行します。`do! a |> f` は `a` の結果を `f` へ渡します。
`IO.try_*` は入出力・符号化の失敗を Result で返します。[IO の使い方](_docs/library-reference/io.md)と[対話サンプル](examples/io/Main.tz)を参照してください。native は標準ストリーム、WASM は明示的な tsuzuri_io ホストへ接続します。

ファイル・ディレクトリ・環境変数・コマンドライン引数・時刻・乱数・子プロセスは、std の `File`／`Dir`／`Path`／`Env`／`Time`／`Random`／`Process`／`Os` で扱います（macOS／Linux）。
`File.read_text "note.txt"` は `IO<Result<string, Os.Error>>` で、OS に触れる操作はすべて同じ形の遅延アクションです（`Path` と `Random.Pcg` は純粋）。
入口が `IO<i32>` ならその値がプロセスの終了コードになり、`tsuzuri run` は 0 以外を `E2005` で報告します。
既定の wasm32 は OS API を `E2000` で拒否し、`--wasm-host wasi` を付けた wasm32 は標準入出力と OS API を WASI preview1 へ接続します（`Process.run` は未対応、preview2 は未実装）。Windows は `E2002` で未対応です。

`run` はトラップの理由とソース位置を報告します。配布用の `build` は既定で位置を含めず、`--trap-info` で明示的に追加できます。
WASM では import なしの `tsuzuri_trap_site()` と、隣接する `.trap.json` の表を使います。

`build`／`run` の `-g`（`--debug-info`）で関数・行・ローカル変数・型のDWARF情報を追加できます。WASMではdebug custom sectionを保持します。
macOSの実行ファイルは隣接する `.dwarf` を保持し、LLDBの `target symbols add <output>.dwarf` で読み込めます。`-O3`では変数が最適化で消える場合があります。
macOSのタスクを含むdebug objectにはClangと対応する `llvm-link`（`TSUZURI_LLVM_LINK`）、実行ファイルには `dsymutil`（`TSUZURI_DSYMUTIL`）が必要です。

ホスト ABI は借用配列・UTF-16／UTF-8 入力、所有バッファ結果、スカラーのみのレコードに対応します。
`extern def now :: unit -> i64`で同期ホスト関数を宣言できます。nativeは`tsuzuri_host_Main_now`、WASMは`tsuzuri`モジュールの`Main.now`へ接続します。
未使用externはimportを増やしません。nativeは生成object/headerをホストと連結し、WASMは`{ tsuzuri: { "Main.now": () => 42n } }`を渡します。
C header を生成して pointer／length と out pointer を使い、返却バッファは `tsuzuri_free` で解放します。
`extern "sqrt" def c_sqrt :: f64 -> f64` のようにリンク名を付けると既存の C 関数を shim なしで呼べ、`extern type Counter` で不透明ハンドルを、関数型の引数で捕捉のないトップレベル関数をホストへ渡せます。
native の実行ファイルは `--link PATH`・`-l NAME`・`-L DIR` や `Tsuzuri.toml` の `[native]` でホストの object・library を直接リンクできます。
[C の使用例](examples/native/main.c) と [WASM のバッファ移転例](examples/web/simulation.mjs) に往復処理があります。

現在は **0.1.0 — 計算カーネルを実行できる初版** です。コンソール実行、C ABI、
ネイティブのデスクトップ・ホスト、ブラウザーのゲーム例を含みます。
C/C++ を上回る性能や C#/F# 以上の書きやすさは設計目標であり、現時点の達成保証ではありません。
**性能を最優先の設計要件の一つとし、CPU 命令・SIMD・並列 CPU・GPU のうち、
意味を保ち実処理が最も速くなる経路を内部で選ぶことを目指します。**
この方針はコンパイラだけでなく、組み込み関数と今後の標準ライブラリにも適用します。
現状は LLVM の CPU 最適化・自動ベクトル化と `--cpu native` に対応し、
`Task.parallel` による明示的な CPU 並列処理も使えます。
GPUは実験的なstrict整数WGSL生成・WebGPUホスト試作と明示CPU参照に対応します。通常runtimeへの実GPU接続と自動offloadは未実装です。
整数の checked／saturating 演算、popcount、rotate などは `Int` モジュールで利用できます。
`@checked x + y` はオーバーフローで `OverflowException` を送出し、`try ... with ... finally` がそれを `Result` の値として受け取ります。例外は同じ関数本体の最も内側の `try` へ字句的に移り、関数呼び出しやラムダを越えず、捕捉されなければトラップします（[例外処理](_docs/language-reference/error-handling.md)）。
任意精度の整数 `bigint` は `123I` のリテラルと `+`・`-`・`*`・`/`・`%`・`**`・比較・Display／Parse に対応します（[BigInt](_docs/library-reference/bigint.md)）。
伸縮可能な所有バッファ `Vec<T>` と配列・リストの標準 API を利用できます。
高階関数は F# と同じく関数を先に受け取り（`Array.map f xs`、`Array.fold f state xs`、`Map.fold f state map`、`Seq.unfold generator state`）、`values |> Array.map double |> Array.sum` のように途中の一時値を借用してつなげられます。
順序付きの不透明型 `Map<K, V>`／`Set<K>` も使えます。`Map.insert (Map.empty()) 1 "value"` は所有値を消費して更新し、`Map.at (&map) 1` で値を借用します。
検索はO(log n)、挿入・削除はO(n)です。`Set.union`／`intersect`／`difference`と借用foldに対応し、キー順に列挙します。
ハッシュ表の `HashMap<K, V>`／`HashSet<K>` は、検索・挿入・削除が平均 O(1) です。`HashMap.insert (HashMap.empty()) 1 "value"` は所有値を消費して更新し、`HashMap.at (&map) 1` で値を借用します。
列挙は挿入順で、`remove` は最後の要素を空きへ移すため、順序は挿入と削除の列だけで決まります。
既定の表は HashDoS への耐性を持ちません。`HashMap.with_seed`／`HashMap.randomized ()` でキー付きの hash にしても部分的な緩和なので、信頼できないキーには `Map` を使ってください。詳細は[HashMap / HashSet](_docs/library-reference/hash-map.md)を参照してください。
一回消費の `Seq<T>` を `for value in Seq.once 42 do ...` のように反復できます。`Seq.unfold`・`map`・借用述語の`filter`・`to_array`を提供します。
128-bitの `f32x4`・`f64x2`・整数vectorとlane maskを使えます。`let values: i32x4 = Simd.splat 1i32`、`Simd.load`・`extract`・`select`・順序付き`sum_lanes`を提供します。
nativeはLLVMの対応命令、WASMは既定でscalar fallback、`--wasm-feature simd128`でv128へ下げます。高速化の保証ではありません。
ユーザー型は `Module.iter` を明示してSeqを返します。Array/List/Vec/Map/Setの`iter`は要素を借用し、通常の直接for反復は従来経路のままです。
ローカルpathパッケージに対応します。git・registry・版解決とGUI・ネットワークの標準ライブラリは未実装です。
メモリは GC ではなく、Rust と同様に所有権の移動・借用・スコープ終了時の解放で管理します。
レコードに共有借用を格納でき、`def first {r s} :: ref {r} string -> ref {s} string -> ref {r} string`で返却元の入力を指定できます。
名前付き契約は直接の完全適用に反映し、関数値経由は保守的に全入力の寿命を保持します。レコードは `record Pair {r s}` のように独立した複数のregionを持て、
名前付き関数の引数には `({s} ref {s} string -> ref {s} string)` のようにregionで量化した関数型を書けます。排他借用フィールドは未対応です。
記憶域は C/C++ と同じ方式で、`new` で生成した値はヒープ、`new` を使わずに生成して束縛した値はスタックに置きます。

`Main.tz`:

```text
def rec sum :: i64 -> i64 -> i64 = \n total ->
    if n <= 0 {
        total
    } else {
        sum (n - 1) (total + n)
    }

export def answer :: i64 = sum 100 0

def main :: i64 = answer()
```

### ビルドと実行

| 項目 | 初版の実装 |
|---|---|
| 状態 | `let` は不変。`let mut` と排他的な `ref mut T` でローカル値を置換できる。標準入出力と OS API は IO、外部機能は extern。共有可変状態はなし |
| 型 | `bool`、`unit`、`i8`～`i128`／`i8u`～`i128u`、`f16`／`f32`／`f64`／`f128`、`d32`／`d64`／`d128`、`byte`／`ubyte`（`i8u`）／`sbyte`（`i8`）、任意精度の `bigint`、ECMA-262 の UTF-16 `string`、従来の UTF-8 `utf8string`、タプル、不変レコード・共用体（`union`）・配列・連結リスト、捕捉環境を持つ関数値 |
| 書きやすさ | `def ... = ラムダ式`、カリー化・部分適用、`\引数 -> 式`、`if…then…else`、`match` とガード、`for…in`／`for…to`／`downto`／`while…do`、`break`／`continue`、レコード更新、インデント本体、`\|>`、関数合成 `>>`／`<<`、`not`／`ignore`、累乗 `**`、F# と同じビット演算 `&&&`／`\|\|\|`／`^^^`／`~~~`／`<<<`／`>>>` と数値リテラルの短い接尾辞、関数を先に受け取る高階関数、明示的な `rec`／`and` |
| 多相性 | `'a` によるパラメトリック多相、ジェネリックなレコード・union、透過的な型別名（`type`）、型クラス・具体型のインスタンスによるアドホック多相。制約推論と単相化 |
| コンピュテーション式 | `.tc` のユーザー定義ビルダー。明示ブロックと型で解決する暗黙本体。束縛・短絡・分岐・反復を通常の関数呼び出しへ展開 |
| タスク | `task { ... }`、`let!`／`return`／`return!`／`do!`。所有値を持つ一回実行の計算を組み合わせ、`Task.parallel` でスレッド数を制限して並列実行 |
| モジュール | 1 ファイル = 1 モジュール。複数ファイルの名前解決と `Main.tz` エントリー |
| メモリ | 所有権、move、`ref T`／`ref mut T`（Rust 互換の `&T`／`&mut T` も可）の借用検査。`new` はヒープ、`new` なしで束縛したリテラルはスタック。文字列・配列・連結リスト・捕捉環境を自動解放し、record・union の `instance Drop` でメモリ以外の資源も一度だけ解放。GC・参照カウント・手動解放なし |
| 最適化 | 既定で LLVM `-O3`、自動 SIMD 化、基本数値変換の直接 lowering。`--cpu native` で実行機向けに最適化。直接の自己末尾再帰は `-O0` でもループ化 |
| 安全性 | 整数除算・配列／リストアクセスを検査。LLVM の未定義動作に依存しない数値仕様。`@checked` の整数オーバーフローは `try` で `Result` に変換 |
| ホスト連携 | スカラー・バッファ・レコードの C ABI と WASM エクスポート／インポート。extern のリンク名・不透明ハンドル・静的コールバック、native のホストリンク指定。標準入出力と OS API（ファイル・環境・時刻・乱数・子プロセス）は IO、UI・ネットワークはホストの責務 |
| AI 向け | 明示的な関数シグネチャ、暗黙の数値変換なし、位置付き JSON 診断、決定的な IR |

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
| 多相性 | `'a` によるパラメトリック多相、ジェネリックなレコード・union、透過的な型エイリアス（`type`）、型クラスおよび具体型インスタンスによるアドホック多相。制約推論と静的単相化。ランク1高階型（HKT）。 |
| コンピュテーション式 | `.tc` によるユーザー定義ビルダー。明示的ブロックと型推論による暗黙本体。束縛・短絡・分岐・反復を標準の関数呼び出しへ展開。 |
| タスクと並列処理 | `task { ... }`、`let!`／`return`／`return!`／`do!`。所有値を持つ一回実行の計算を組み合わせ、`Task.parallel` でスレッド数を制限して安全に並列実行。 |
| モジュール構成 | 1 ファイル = 1 モジュール。同一ディレクトリ内の自動解決と `Main.tz` によるエントリーポイント。ローカルパッケージ（`Tsuzuri.toml`）。 |
| メモリモデル | 所有権の移動（move）と借用検査（`ref T`／`ref mut T`、Rust 互換の `&T`／`&mut T` も可）。明示的なヒープ確保（`new`）とスタック配置の区別。文字列・配列・リスト・環境の自動解放、`instance Drop` による RAII（GC や参照カウントは不使用）。 |
| 最適化 | 既定で LLVM `-O3`、自動 SIMD 化、基本数値変換の直接 lowering。`--cpu native` によるビルド機向け最適化。直接の自己末尾再帰は `-O0` でもループ化。 |
| 安全性 | ゼロ除算や配列・リスト境界アクセスの実行時検査。LLVM の未定義動作に依存しない数値仕様。`@checked` による整数オーバーフローは `try` で `Result` に変換可能。 |
| ホスト連携 | スカラー・バッファ・レコードの C ABI 連携および WebAssembly（WASM）のエクスポート／インポート。`extern` のリンク名指定・不透明ハンドル・静的コールバック、ネイティブのホストリンク。標準入出力と OS API は `IO`、UI やネットワークはホスト側に委譲。 |
| 開発・AI 支援 | 明示的な関数シグネチャ、暗黙の型変換の排除、位置情報付き JSON 診断、決定的な IR 出力。公式 LSP、フォーマッター、テストランナー。 |

---

## 言語機能

### 所有権と借用

Tsuzuri はガベージコレクション（GC）や参照カウントに依存せず、Rust と同様の所有権システムと借用検査によって安全にメモリを管理します。

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

- **ライフタイムとリソース解放 (RAII)**: 所有者より長生きする参照や、借用中の移動、共有借用と排他借用の競合はコンパイル時に拒否されます。レコードや共用体に `instance Drop<T> { fn drop value = ... }` を実装すると、値がスコープを抜ける際や再代入時に自動的に `drop` が一度だけ呼び出されます。ファイルやホストハンドルの確実なクローズに活用できます。`use` 束縛は、対象の値が `Drop` を実装していることを静的に検査します。
- **一時値の借用**: 関数呼び出しの中間結果など、呼び出し後に借用が残らない一時値は、安全に共有借用として渡せます（例: `Array.sum (Array.map f (ref xs))`）。

### 基本データ型とコレクション

#### 数値型とリテラル

- **整数型**: `i8`〜`i128`、符号なしの `i8u`〜`i128u`。別名として `byte`／`ubyte`（`i8u`）、`sbyte`（`i8`）が利用可能です。
- **浮動小数点型**: `f16`、`f32`、`f64`、`f128`。十進浮動小数点型として `d32`、`d64`、`d128` もサポートしています。
- **任意精度整数 (`bigint`)**: `123I` のように `I` 接尾辞で記述し、加減乗除（`+`, `-`, `*`, `/`, `%`）、累乗（`**`）、比較演算、文字列変換に対応します。詳細は [BigInt](_docs/library-reference/bigint.md) を参照してください。
- **リテラルの接尾辞**: `1i32` や `0.1d128` などの標準的な接尾辞に加え、F# 互換の短い接尾辞（`1y`, `1uy`, `1s`, `1us`, `1u`, `1l`, `1ul`, `1L`, `1UL`, `1.5hf`, `1.5f`, `1.5F`, `0.1hm`, `0.1m`, `0.1M`）が利用できます。バイトリテラルは `'a'B`（`byte`）、`"text"B`（`[byte]`）です。接尾辞のない整数は文脈から推論され、未確定時は `i32`、小数は `f64` に決定されます。

#### 文字列と文字

- **既定の `string` (UTF-16)**: [ECMA-262 の String 値モデル](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html#sec-ecmascript-language-types-string-type) に従う UTF-16 コード単位列です。文字型は UTF-16 コード単位を表す `char`（`'A'`）です。
- **バイト列 `utf8string` (UTF-8)**: 従来の UTF-8 文字列は `utf8string` 型および `u8"..."` リテラルで扱います。文字型は Unicode スカラー値を表す `utf8char`（`u8'😀'`）です。
- **文字列補間**: `$"x = {x}, y = {y:.2}"`（UTF-8 は `u8$"..."`）と記述し、波括弧自体は `{{`／`}}` でエスケープします。書式指定は `[[fill]align][+][width][.precision][type]` に対応し、ネイティブと WASM で完全に一致する丸め処理を行います。
- 詳細は [文字列の仕様](docs/language.md#string-と-utf8string) を参照してください。

#### 配列・リスト・コレクション

- **配列 (`[T]`)**: 固定長の連続メモリ領域です。`[1, 2, 3]` のように記述し、生成後の長さや要素は不変です。`ref values[start..end]` で終端を含まない共有スライス（型: `ref [T]`）を作成でき、コピーを発生させずに部分列を読み取れます。動的な初期化には `new [i32](count, i -> i as i32)` を使用します。
- **連結リスト (`[|T|]`)**: 単方向の不変連結リストです。リテラルは `[|1, 2, 3|]`、空リストは `[||]` です。要素への添字アクセスは $O(n)$ となります。
- **ベクター (`Vec<T>`)**: 伸縮可能な所有バッファです。
- **順序付きマップ・セット (`Map<K, V>` / `Set<K>`)**: 平衡二分探索木による不変コレクションです。キーの順序を保持し、検索は $O(\log n)$、挿入および削除は $O(n)$ で動作します。
- **ハッシュマップ・ハッシュセット (`HashMap<K, V>` / `HashSet<K>`)**: 平均 $O(1)$ で検索・挿入・削除が可能なコレクションです。挿入順序が保持されます。詳細は [HashMap / HashSet](_docs/library-reference/hash-map.md) を参照してください。
- **シーケンス (`Seq<T>`)**: 一度だけ消費可能な遅延反復ストリームです。`Seq.unfold`、`Seq.map`、`Seq.filter`、`Seq.to_array` などを提供します。
- **SIMD ベクトル**: 128-bit 幅の `f32x4`、`f64x2`、整数ベクトル型をサポートします。`Simd.splat`、`Simd.load`、`Simd.extract`、`Simd.sum_lanes` などの高効率な組み込み演算を提供します。

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

`#distance` は具体的なレコード・union の定義元モジュールにある関数を要求します。
例えば `'T` が `Point.Point` なら `'T.distance` は `Point.distance` を選び、返却型も照合・推論します。
通常の関数値・部分適用に対応し、`private` を迂回しません。
詳しくは [モジュール関数の制約](docs/language.md#モジュール関数の制約) を参照してください。

#### カリー化・演算子・クロージャ

- **カリー化と静的単相化**: すべての関数は自動的にカリー化されており、部分適用を自然に行えます。型クラスによるメソッド解決はコンパイル時に完全に完了し、動的ディスパッチやランタイム辞書渡しのコストを一切発生させません。呼び出しに使われた具体的な型の組み合わせごとに最適化されたコードを生成します。
- **演算子**: パイプライン演算子 `|>`、関数合成演算子 `>>`（順方向: `\x -> g (f x)`）および `<<`（逆方向: `\x -> f (g x)`）、累乗演算子 `**`（右結合）、組み込みの `not` や `ignore` を提供します。ビット演算には F# と同様の `&&&`、`|||`、`^^^`、`~~~`、`<<<`、`>>>` を使用します（`>>>` は符号付き整数なら算術シフト、符号なし整数なら論理シフト）。詳細は [演算子](_docs/language-reference/expressions-and-operators.md) を参照してください。
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

union Maybe<'a> = None | Some of 'a

def area :: Shape -> f64 = \shape ->
    match shape with
    | Circle r -> r * r * 3.141592653589793
    | Rect (w, h) -> w * h
    | Empty -> 0.0

area (Rect (3.0, 4.0))
```

match は tag の `switch` に下げ、payload の move・解放・複製も case ごとに行います。
標準の `Option<'a>`／`Result<'a, 'e>` と、変換・借用・失敗伝播の関数も同梱しています。
case が不足する match は `E1021` のコンパイルエラーです。
record／union宣言の後に `deriving (Eq, Ord, Display, Hash, Default)` を指定して構造的な実装を生成できます。
`union Tree<'a> = Leaf | Node of Tree<'a> * 'a * Tree<'a>` のような木・ASTも使えます。
再帰型は非 Copy の所有ヒープノードで、解放と捕捉環境の複製は深さに比例するスタックを使いません。
詳細は [言語仕様](docs/language.md#共用体union) を参照してください。

### 制御構文とパターンマッチ

kindを明示したrank-1高階型にも対応します。`class Functor<'f: * -> *> { def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b> }`を定義し、OptionやResult等のconstructorごとにinstanceを実装できます。
`Result<string>`の部分適用は末尾のエラー型を固定します。辞書/boxingを追加せず通常の単相化へ下げます。kind省略推論や標準Functorは未導入です。
例と制約は[HKT仕様](docs/language.md#高階型hkt)を参照してください。

## 制御構文とパターン

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
- **アクティブパターン**: 入力値を任意の論理的ケースに変換・分解するアクティブパターンに対応しています。`Option` や `bool` を返す部分アクティブパターン（例: `def (|Parsed|_|) :: ref string -> Option<i64> = \text -> Parse.parse text`）や、共用体と連動する完全アクティブパターンが利用可能です。
- **再帰関数**: 再帰関数は `def rec name :: ...`、相互再帰は `and name :: ...` で繋いで記述します。直接の自己末尾再帰は、`-O0` であっても LLVM レベルでループ構造へと自動変換されます。詳細は [言語仕様: 制御構文](docs/language.md#制御構文) および [制御構文のベンチマーク](docs/benchmarks.md#制御構文の比較) を参照してください。実行例は `tsuzuri run examples/control` で確認できます。

### ユーザー定義のコンピュテーション式

F# のように、計算の組み合わせや制御フローの動作をカスタマイズできるコンピュテーション式をサポートしています。
**1 つの `.tc` ファイルが 1 つのビルダー** に対応し、ファイル名がそのままビルダー名となります。言語への特別な構文登録は不要で、標準ライブラリの `Result` や `Option` もこの仕組みで実装されています。

## ユーザー定義のコンピュテーション式

F# のように、`Bind`・`Return` などを実装して計算の組み合わせ方を定義できます。
**一つの `.tc` ファイルが一つのビルダー**で、ビルダー名はファイル名です。
型クラスの実装や新しい構文の登録は不要です。標準の `Option`／`Result` もこの仕組みで実装され、
追加ファイルなしで使えます。

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

`let!` は `Result.Bind value continuation`、`return` は `Result.Return value` に相当します。
`Error`／`None` なら続きを呼ばず、error 型の暗黙変換はしません。
`return` 自体は関数脱出ではなく成功値の生成です。失敗前の通常の `let`・式は実行し、トラップは失敗値に変換しません。
`Option.map_ref` などは所有する payload を借用して扱い、`get` は失敗値に対してトラップします。
`ReturnFrom`、`Yield`／`YieldFrom`、`Zero`、`Combine`、`For`／`While` も必要に応じて定義でき、
`Delay`／`Run` があれば本体を包んで遅延・実行の仕方を制御します。
使用した構文の操作が未実装ならコンパイルエラーで、暗黙の既定実装はありません。

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
- **安全なスレッド分離**: タスクが捕捉する値は所有権の移動（move）または Copy に限定され、参照の持ち込みはコンパイル時に拒否されます。これにより、共有可変状態によるデータ競合の発生を根本から防ぎます。
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

**モジュール名は拡張子を除いたファイル名で決まり、1 ファイルに 1 モジュールを強制します。**
`module` 宣言、入れ子のモジュール、複数ファイルへの同一モジュールの分割はできません。
同じディレクトリの `.tz`・`.tt`・`.tc` ファイルを自動で読み込みます。
インポート宣言やファイルの列挙は不要です。

副作用を伴う入出力処理は、遅延アクションを表す `IO<T>` 型によって純粋なコードから分離されます。

```text
def main :: IO<unit> = IO {
    do! IO.write_line "お名前を入力してください:"
    let! name = IO.read_line ()
    match name with
    | Some n -> do! IO.write_line ($"こんにちは、{n} さん！")
    | None -> do! IO.write_line "入力が終了しました。"
}
```

- `IO.write_line`（別名 `IO.writeln`）や `IO.read_line` などの基本関数を提供します。
- 失敗の可能性がある操作には `IO.try_*` 系統の関数を使用し、結果を `Result` 型として安全に処理できます。詳細は [IO の使い方](_docs/library-reference/io.md) を参照してください。
- アプリケーションのエントリーポイントが `IO<i32>` を返す場合、その値がプロセスの終了コードとなります（0 以外は `E2005` で報告されます）。

#### OS API

ファイルシステム、プロセス、環境変数、時刻などの OS 機能は、標準モジュール `File`、`Dir`、`Path`、`Env`、`Time`、`Random`、`Process`、`Os` で提供されます（macOS および Linux 対応）。
すべての OS 操作は `IO<Result<T, Os.Error>>` などの一貫した遅延アクションとして抽象化されています（`Path` および擬似乱数 `Random.Pcg` は純粋関数です）。WASM 環境では `--wasm-host wasi` を指定することで WASI preview1 に接続可能です。

#### コンパイル時定数 (`const`)

`const Answer: i64 = 40 + 2` のように、型注釈付きのコンパイル時定数を宣言できます。
整数および浮動小数点の演算、文字列、配列、タプル、レコードリテラルを扱うことができ、モジュールを超えた修飾参照や前方参照にも対応しています。

#### デバッグ出力と例外処理

- **デバッグ出力**: `Debug.print value` は値を借用して標準エラー出力に表示し、`Debug.trace value` は表示を行ったうえで同じ所有値をそのまま返します（パイプラインの途中での値確認に便利です）。
- **チェック付き演算と例外**: `@checked x + y` は演算オーバーフロー時に `OverflowException` を送出します。この例外は関数境界やラムダ式を越えず、同一関数内の最も内側の `try ... with ... finally` 式によって `Result` 型の値へと安全に変換されます。詳細は [例外処理](_docs/language-reference/error-handling.md) を参照してください。

### ファイルとモジュール構成

Tsuzuri は **1 つのファイルが厳格に 1 つのモジュールを構成する** 原則を採用しています。明示的な `module` 宣言や、複数ファイルへの同一モジュール分割は許可されません。
同一ディレクトリ内に配置されたファイルは自動的に認識・読み込まれるため、手動でファイルを列挙したり `import` を記述したりする必要はありません。

#### ファイル拡張子の役割

| 拡張子 | 役割と記述内容 |
| --- | --- |
| `.tz` | レコード、共用体、関数、型クラスのインスタンス実装。`Main.tz` のみトップレベル実行コードを記述可能。 |
| `.tt` | 型クラスのシグネチャ宣言専用。関数本体やレコード定義、インスタンス実装は配置不可。 |
| `.tc` | コンピュテーション式のビルダー定義および関連補助関数。トップレベル実行コードは配置不可。 |

拡張子が異なっていても同名のモジュールを同一ディレクトリに共存させることはできません（例: `Checked.tz` と `Checked.tc` の併存はエラー）。

`Point.tz`:

```text
record Point { x: f64, y: f64 }

def distance :: Point -> f64 = \point -> sqrt (point.x * point.x + point.y * point.y)

export def hypotenuse :: f64 -> f64 -> f64 = \x y -> distance (Point { x: x, y: y })
```

同一ディレクトリの `Main.tz`:

```text
let p = Point { x: 10.0, y: 20.5 }
let d = Point.distance p
d
```

通常の `fn` も別ファイルから `モジュール名.関数名` で呼べます。
`export` はモジュール間の可視性ではなく、C／WASM ホストへの公開指定です。
レコード名は一意なら `Point`、明示する場合は `Point.Point` と書けます。
同名のレコードが複数モジュールにある場合、他モジュールからは修飾名で区別します。

別ファイルの関数や型は、`モジュール名.識別子名`（例: `Point.distance` や `Geometry.Point.Point`）で参照します。
宣言は既定で外部モジュールへ公開されます（public）。モジュール内でのみ使用する補助関数やデータ構造には `private` キーワードを付与します。
なお、ホスト言語（C や WebAssembly）へシンボルを公開する場合は `export` を指定します。

標準ライブラリ（std）はコンパイラに埋め込まれ、すべてのプロジェクトで自動的に読み込まれます。
std の関数も `Math.zero()` のように修飾して呼び、使わない std のコードは生成物に含まれません。
次のモジュール名は std 用に予約しており、利用者のファイル名には使えません（`E1011`）。

| 予約モジュール | 用途 |
|---|---|
| `Option`、`Result` | 省略可能な値と失敗 |
| `Array`、`List`、`Vec`、`Map`、`Set`、`HashMap`、`HashSet` | コレクション |
| `String`、`Utf8String`、`Char` | UTF-16／UTF-8 文字列と文字 |
| `Math`、`Int` | 数学関数と整数演算 |
| `Debug`、`Test` | デバッグ出力とテスト |
| `Parallel`、`Simd`、`Gpu` | データ並列・SIMD・GPU |
| `File`、`Dir`、`Path`、`Env`、`Time`、`Random`、`Os`、`Process` | OS API（`Path` と `Random.Pcg` を除き `IO` の遅延アクション） |
| `Format` | 書式指定（`Format` クラスの instance 用の `parse`／`pad`） |

OS API・ハッシュコンテナ・書式指定の追加で、これらの名前のファイルは `E1011` で拒否されます。使っていた既存のプロジェクトは改名が必要です。
stdは`Option`・`Result`、コレクション・文字列・文字・整数・並列処理・数学・OS・書式指定のAPIを持ちます。
`Math.sqrt 4.0f32`のように全float型の基本演算を使え、超越関数はf32／f64に対応します。`Math.pi()`などの定数も型を保持します。
`Math.fma 2.0 3.0 4.0`は積和を一度だけ丸めます。`Array.sum_pairwise`は固定ペア木、`Array.sum_kahan`はNeumaier補償和、`Array.dot_fma`は順次FMA内積です。通常の`a * b + c`、`Array.sum`、`Array.dot`は融合・再結合しません。
無修飾の型・case・クラス名は利用者の宣言を std より優先します。

#### パッケージ管理 (`Tsuzuri.toml`)

プロジェクトルートに `Tsuzuri.toml` を配置することで、ローカルの依存パッケージを同一の `check` / `build` / `run` コマンドでシームレスに利用できます。

アプリケーションは **`Main.tz`** から開始します。
トップレベルの `let` と最後の結果式、または従来の `fn main` のどちらかを使います。
上の例では最後の `d` を表示します。結果式を省略すると `unit` になり、何も表示しません。
実行例は `./target/release/tsuzuri run examples/point` です。

rootに`Tsuzuri.toml`を置くと、ローカル依存を同じ`check`／`build`／`run`コマンドで利用できます。

```toml
[package]
name = "app"
version = "0.1.0"
[dependencies]
geometry-core = { path = "../geometry-core" }
```

依存側にもname/versionを持つmanifestを置きます。依存の`Point.tz`は`GeometryCore.Point`で参照し、依存内でも完全修飾します。
限定TOML、相対pathだけに対応し、ネットワークやbuild scriptは実行しません。詳細は[言語仕様](docs/language.md#ローカルパッケージ)を参照してください。

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
- **不透明ハンドルとコールバック**: `extern type Counter` でホスト側のポインタを安全な不透明ハンドルとして扱えます。また、環境キャプチャを持たないトップレベル関数は関数ポインタとしてホストへ渡せます。
- **ホストライブラリのリンク**: ネイティブ実行ファイルのビルド時には、`--link PATH`、`-l NAME`、`-L DIR` や `Tsuzuri.toml` の `[native]` セクションを通じて、外部の C/C++ ライブラリやオブジェクトを直接リンクできます。
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

- **型変換の規則**: 64-bit 整数（`i64` / `i64u`）は JavaScript の `BigInt`、`f32` / `f64` は `Number`、`bool` は `i32`（0 = false, 1 = true）に対応します。
- **メモリとスタックのカスタマイズ**: `--wasm-max-memory SIZE`（既定 16MiB、最大 4GiB-64KiB / wasm64 は 16GiB）や `--wasm-stack-size SIZE`（既定 1MiB）で線形メモリの上限やメインスタックサイズを調整できます。これらは `Tsuzuri.toml` の `[wasm]` セクションでも設定可能です。
- **マルチスレッド (`threads`)**: `--wasm-feature threads` を指定することで、Task や Parallel による並列計算を Web Worker や Node.js の Worker Threads に分散できます。詳細は [Webホスト要件](examples/web/README.md) を参照してください。

### GPU カーネル連携（実験的）

`Main.tz` のトップレベルの結果、または引数なしの `main` の返却型が
数値型／`bool`／`unit`／`string`／`utf8string` なら、ネイティブ用ホスト・ラッパーが
結果を表示し、成功時は終了コード 0 を返します。`unit` は何も表示しません。
トップレベルの結果はそれ以外の型でも `Display` を持てばその表示を出力し、持たなければ表示せずに捨てます（以前は `E2004`）。`main` は従来どおり表示できる型か IO を返します。
言語内に出力の副作用を持ち込む仕組みではありません。
数値は `to_string`／`Display.display` と同じ形式です。二進浮動小数点は最短の往復可能な十進表現
（`0.1` は `0.1`）、負のゼロは `-0` と表示し、native／WASM で共通の実装を使います。
`to_string value` は値を消費し、`Display.display ref value` は借用します。
`let value: Option<f64> = Parse.parse ref text` のように解析でき、不正入力・overflow は `None` です。
独自型にも Display／Parse インスタンスを定義できます。
文字列の出力は UTF-8 です。string の孤立サロゲートは暗黙に置換せずトラップし、
必要なら `String.to_well_formed` で明示的に置換します。

---

## 開発ツールとエコシステム

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
- **高精度なリファクタリング**: リネームやクイックフィックスは編集後のコードを内部で再解析し、安全性が確認された差分のみを適用します。詳細は [フォーマッターと LSP](_docs/tools/editor-tools.md) を参照してください。

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

def main :: string = {
    let mut text = "こんにちは";
    let size = length ref text; // 借用後も所有者を使える
    replace ref mut text;       // この呼び出し中は排他的に借用
    let result = text;      // 所有権を移動。以降の text の使用はエラー
    result
}
```

### デバッグ情報と出力仕様

- **DWARF デバッグ情報**: `build` や `run` に `-g`（`--debug-info`）を付与することで、関数・行番号・変数・型の DWARF情報を埋め込めます。macOS では `.dwarf` ファイルが生成され、LLDB などのデバッガでシンボルを解決可能です。
- **実行時トラップ報告**: 配布ビルドでは `--trap-info` を指定することで、トラップ発生時の正確なソース位置情報を出力に含めることができます。WASM では `.trap.json` 表との連動に対応しています。
- **コンソールの出力とフォーマット**: `Main.tz` のトップレベル結果式や `main` の戻り値が `Display` を実装している場合、その文字列表現が標準出力に表示されます。浮動小数点数は最短で往復可能な十進表現で出力され、ネイティブと WASM で完全に一致します。

---

## 環境構築とビルド

### 必要要件

- **Rust**: 1.85 以降（浮動小数点リテラルの厳密な丸めに `rustc_apfloat` を使用）
- **LLVM / Clang**: 17 以降（WebAssembly のリンクには `wasm-ld`（LLD）も必要）
- **Node.js**: 20 以降（WASM テスト・検証用）
- **Python**: 3.9 以降（参照実装テスト・デスクトップ検証用）

### 配布パッケージの利用

ローカルに LLVM / Clang の完全な開発環境を構築せずに利用したい場合は、コンパイラ本体・Clang・LLD・SDK を同梱したプラットフォーム別配布パッケージ（`tsuzuri-<version>-<host>.tar.gz`）を利用できます。詳細は [配布物を使う](_docs/get-started.md#配布物を使う) を参照してください。

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
tsuzuri lsp
```

### 主要オプション一覧

| オプション | 説明 |
| --- | --- |
| `-o`, `--output PATH` | 出力先パスを指定します（親ディレクトリは自動作成されます）。 |
| `--target native\|wasm32\|wasm64` | ターゲット環境を指定します（既定: `native`。`wasm64` は 64-bit 線形メモリ）。 |
| `--emit exe\|object\|llvm\|header\|wasm\|wgsl` | 出力成果物の種類（既定: native は `exe`、WASM は `wasm`）。 |
| `-O0` ～ `-O3` | 最適化レベル（既定: `-O3`。高速化のために精度を損なう fast-math などは使用しません）。 |
| `--cpu generic\|native` | CPU 命令セットの特化（既定: `generic`。`native` はビルド機の命令セットとスケジューリングに最適化）。 |
| `--deny-warnings` | 警告が存在する場合にコンパイルを失敗させ、コード生成や実行を行わずに停止します。 |
| `--trap-info` | 配布用ビルドにおいて、実行時トラップの正確なソース位置情報を保持します。 |
| `-g`, `--debug-info` | DWARF デバッグ情報を付与します。 |
| `--wasm-max-memory SIZE` | WASM の最大線形メモリサイズ（既定: 16MiB、例: `256MiB`）。 |
| `--wasm-stack-size SIZE` | WASM のスタックサイズ（既定: 1MiB、例: `4MiB`）。 |
| `--wasm-host wasi` | wasm32 において、標準入出力および OS API を WASI preview1 のインポートへ接続します。 |
| `--wasm-feature simd128\|threads` | WebAssembly の追加機能（128-bit SIMD、Worker スレッド分散）を有効化します。 |
| `--no-cache` | ビルド成果物キャッシュを完全に無効化します。 |
| `--json` | 診断情報やテスト結果を 1 行 1 JSON オブジェクト形式で標準エラー出力へ返します。 |

### 入力とエラー報告の仕様

- **入力の解決**: ファイルまたはディレクトリを 1 つ指定します。ディレクトリを指定した場合は、直下の `Main.tz` が自動的にエントリーポイントとして選ばれます。ファイル指定時はその親ディレクトリをルートとし、配下の全 `.tz`・`.tt`・`.tc` を相対パス順に再帰的に探索して読み込みます。
- **一括エラー報告**: コンパイラは独立した複数の型エラーや構文エラーを収集し、ファイル名およびソース位置順にまとめて報告します（最大 50 件まで表示、残りは件数のみ通知）。二次エラーは抑制され、エラーが存在する限りコード生成や実行は行われません。
- **ビルドキャッシュ**: ビルドおよび実行時のアーティファクトキャッシュは既定で有効です。ソース、コンパイラ、ツールチェイン、設定内容の SHA-256 ハッシュをキーとして管理し、変更のないモジュールの再コンパイルを回避します。キャッシュ保存先は環境変数 `TSUZURI_CACHE_DIR` でカスタマイズでき、`--no-cache` で無効化できます。

---

## 検証と性能測定

Tsuzuri は言語仕様の正しさとパフォーマンスを保証するため、網羅的なテストスイートとマルチ言語ベンチマークを備えています。

### テストスイートの実行

build/runの成果物cacheは既定で有効です。`--no-cache`で読み書きを完全に無効化し、`TSUZURI_CACHE_DIR`で保存先を指定できます。check/headerは対象外です。
既定の保存先はmacOSの`$HOME/Library/Caches/tsuzuri/build-cache`、Linuxの`$XDG_CACHE_HOME/tsuzuri/build-cache`（未設定なら`$HOME/.cache`）、Windowsの`%LOCALAPPDATA%\Tsuzuri\Cache\build-cache`です。
コンパイラ・ツール・ソース・設定・runtimeをSHA-256で識別し、hitでも出力保護を通します。破損は再ビルド、cacheのI/O失敗はW2001です。
markerで管理対象を識別し、既存の非cacheディレクトリを転用しません。2GiB/30日を目安に一回最大128件を回収します。明示的に削除する場合はこの専用cacheディレクトリだけを削除してください。
解析とIR生成は毎回行うPhase 1のwhole-build cacheです。macOSのdebug executableはDWARFの出力先依存を保つため出力パスもキーへ含めます。

入力はファイルまたはディレクトリを一つ指定します。ディレクトリ指定はその直下の `Main.tz` を選びます。
ディレクトリ入力はそのディレクトリ、ファイル入力は親ディレクトリをルートにし、配下の全 `.tz`・`.tt`・`.tc` を相対パス順に再帰的に読み込み、
未参照のモジュール・ビルダーも検査します。
`Geometry/Point.tz` は `Geometry.Point` になり、`Geometry.Point.distance` や `Geometry.Point.Point` と完全修飾して参照します。
隠し項目を無視し、ソース・ディレクトリのsymlinkを拒否します。各パス要素は大文字小文字を区別する ASCII 識別子で、
`_` 単独や予約語は使えません。
モジュールは16要素・255バイト、探索は4096ソース・1024ディレクトリまでです。上位のrootは推測せず、階層全体にはrootディレクトリを指定します。
`run`／`--emit exe` の入力は `Main.tz` またはそのディレクトリに限ります。
`check`／ライブラリ出力では `.tz`・`.tt`・`.tc` を指定でき、`Main.tz` は不要です。

`check`・`build`・`run` は独立したエラーをまとめてファイル・位置順で報告します。
例えば二つの関数に型の不一致があれば、`tsuzuri check Main.tz --json` は二行の error オブジェクトを返します。
表示は 50 件までで、残りの件数はコードなしの note です。収集上限 1000 件に達した場合だけ `at least` と表示します。
壊れたシグネチャに由来する二次エラーは抑制し、エラーがある間は生成・実行せず終了コード 1 を返します。

出力先省略時は選択した入力ファイルの拡張子を変更します。ディレクトリ指定なら `Main.ll` などになります。
`--emit llvm` はライブラリ用 IR で、コンソールのエントリー・ラッパーは付けません。
ネイティブの並列タスクを含む生の IR を直接リンクする場合は、
`clang kernel.ll src/runtime/task.c -pthread -lm ...` のようにタスクランタイムも渡します。
`--emit object` にはランタイム本体が含まれ、別途 C ソースを渡す必要はありません。
WASM は `IO<T>` の入口または少なくとも一つの `export def` が必要です。
ホスト向けの公開名 `tz_name` は維持するため、エクスポート名はプロジェクト全体で一意にします。
`--emit object --target wasm32` はリンク前の WASM オブジェクトも生成できます。
コンパイル／リンク失敗では既存出力を変更せず、成功した成果物だけを同じファイルシステム上で置換します。
読み込んだいずれのソース自身やその別名、シンボリックリンクへの出力も拒否します。

## 検証と性能測定

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
node tests/computations.mjs target/release/tsuzuri
node tests/control.mjs target/release/tsuzuri
node tests/lsp_sessions.mjs target/release/tsuzuri
node tests/io.mjs target/release/tsuzuri
node tests/os.mjs target/release/tsuzuri

# WebAssembly & GPU テスト
node tests/wasm_threads.mjs target/release/tsuzuri
node tests/wasm_memory.mjs target/release/tsuzuri
node tests/gpu.mjs target/release/tsuzuri
```

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
