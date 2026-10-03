# Tsuzuri

**AI と人間が、少ない暗黙ルールで堅牢なプログラムを書けることを目指す、関数型の式と所有権モデルを備えた言語。**
コードは **`.tz`**、型クラス宣言は **`.tt`**、コンピュテーション式のビルダー実装は **`.tc`**。
Rust 製のフロントエンドで型検査し、LLVM によりネイティブコードと WebAssembly を生成します。

**[日本語ドキュメント](_docs/README.md)**: [入門](_docs/get-started.md)・[言語リファレンス](_docs/language-reference/README.md)・[標準ライブラリ](_docs/library-reference/README.md)・[実践ガイド](_docs/guides/README.md)・[開発ツール](_docs/tools/README.md)・[対応状況](_docs/feature-status.md)。

**[VS Code 拡張機能](vsc/README.md)**: コンパイラ・LLVM・SDK を含む OS／CPU 別 VSIX、編集支援、ビルド、実行、テスト、ソースデバッグを提供します。
x64／ARM64 が対象で、Windows ARM64 はデバッグだけ未対応です。使い方は拡張機能の README、開発方法と各環境の検証状況は [vsc/Development.md](vsc/Development.md) を参照してください。

未使用の束縛・private 宣言・到達不能な match 節は警告します。意図的な未使用ローカルは `_name` で明示できます。
`check`／`build`／`run` の `--deny-warnings` は警告だけでも失敗させ、コード生成・実行前に停止します。

`tsuzuri fmt [--check] <file|directory> [--json]` でソースを保守的に整形できます。
`--check` は書き換えず差分があれば終了コード 1。ディレクトリは直下の `.tz`／`.tt`／`.tc` だけを対象にし、LLVM は不要です。

`tsuzuri lsp` はエディターから起動するstdio言語サーバーです。未保存の全量同期、複数診断、型のhover、定義ジャンプ、シンボル一覧に加え、参照検索・rename・workspace symbol・補完・signature help・semantic tokens・未使用ローカルのquick fix・LSP経由の整形に対応します。
renameとquick fixは編集後のプロジェクトを再解析し、意味が変わらない編集だけを返します。UTF-8位置を交渉できないクライアントにはUTF-16位置を返します。詳細は[フォーマッターと LSP](_docs/tools/editor-tools.md)。

`test "adds numbers" = assert (1 + 2 == 3)` のようにテストを書き、`tsuzuri test <file|directory>` で実行できます。
`--filter TEXT`、`--json`、`-O0`～`-O3`、`--target native|wasm32` に対応します。WASM 実行には Node.js が必要です。
テストは常に型検査しますが通常ビルドには含めず、実行時は別プロセスで隔離します。Main.tz は不要です。

アクティブパターンは bool／Option を返す部分形式と、宣言した union に対応する複数ケース形式を使えます。
`def (|Parsed|_|) :: ref string -> Option<i64> = \text -> Parse.parse text` により、`Parsed value` で解析結果を照合できます。

`const Answer: i64 = 40 + 2` で型付きのコンパイル時定数を宣言できます。前方参照・`private const`・他モジュールからの修飾参照に対応します。
整数とbinary浮動小数点の演算、文字列・配列・タプル・レコードを扱い、利用ごとに通常の所有値を生成します。関数呼び出しとdecimal演算は定数式では拒否します。

`Debug.print value` は借用して表示し、`Debug.trace value` は表示して同じ所有値を返します。native は stderr、WASM は既定で no-op です。
WASM の `--debug-output` を使う場合は、[Debug のホスト契約](docs/language.md#デバッグ出力) に従って `tsuzuri_debug.write` を提供します。

標準入出力は `IO<T>` の遅延アクションで扱います。`IO { do! IO.write_line "Hello" }` を Main.tz の入口にすると実行し、`let! line = IO.read_line ()` で EOF を区別して読み取れます。
ビルダーブロックを省略して通常の関数・匿名関数・main の本体へ `let!`／`do!` を直接書くこともでき、IO と Option／Result／独自ビルダーを型に基づいて合成します。[暗黙の計算式](docs/language.md#ビルダー名を省略した本体)を参照してください。
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
伸縮可能な所有バッファ `Vec<T>` と配列・リストの標準 API を利用できます。
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

## 設計と実装済みの範囲

| 項目 | 初版の実装 |
|---|---|
| 状態 | `let` は不変。`let mut` と排他的な `ref mut T` でローカル値を置換できる。標準入出力と OS API は IO、外部機能は extern。共有可変状態はなし |
| 型 | `bool`、`unit`、`i8`～`i128`／`i8u`～`i128u`、`f16`／`f32`／`f64`／`f128`、`d32`／`d64`／`d128`、`byte`／`ubyte`、ECMA-262 の UTF-16 `string`、従来の UTF-8 `utf8string`、タプル、不変レコード・共用体（`union`）・配列・連結リスト、捕捉環境を持つ関数値 |
| 書きやすさ | `def ... = ラムダ式`、カリー化・部分適用、`\引数 -> 式`、`if…then…else`、`match` とガード、`for…in`／`for…to`／`downto`／`while…do`、`break`／`continue`、レコード更新、インデント本体、`|>`、高階関数、明示的な `rec`／`and` |
| 多相性 | `'a` によるパラメトリック多相、ジェネリックなレコード・union、透過的な型別名（`type`）、型クラス・具体型のインスタンスによるアドホック多相。制約推論と単相化 |
| コンピュテーション式 | `.tc` のユーザー定義ビルダー。明示ブロックと型で解決する暗黙本体。束縛・短絡・分岐・反復を通常の関数呼び出しへ展開 |
| タスク | `task { ... }`、`let!`／`return`／`return!`／`do!`。所有値を持つ一回実行の計算を組み合わせ、`Task.parallel` でスレッド数を制限して並列実行 |
| モジュール | 1 ファイル = 1 モジュール。複数ファイルの名前解決と `Main.tz` エントリー |
| メモリ | 所有権、move、`ref T`／`ref mut T`（Rust 互換の `&T`／`&mut T` も可）の借用検査。`new` はヒープ、`new` なしで束縛したリテラルはスタック。文字列・配列・連結リスト・捕捉環境を自動解放し、record・union の `instance Drop` でメモリ以外の資源も一度だけ解放。GC・参照カウント・手動解放なし |
| 最適化 | 既定で LLVM `-O3`、自動 SIMD 化、基本数値変換の直接 lowering。`--cpu native` で実行機向けに最適化。直接の自己末尾再帰は `-O0` でもループ化 |
| 安全性 | 整数除算・配列／リストアクセスを検査。LLVM の未定義動作に依存しない数値仕様 |
| ホスト連携 | スカラー・バッファ・レコードの C ABI と WASM エクスポート／インポート。extern のリンク名・不透明ハンドル・静的コールバック、native のホストリンク指定。標準入出力と OS API（ファイル・環境・時刻・乱数・子プロセス）は IO、UI・ネットワークはホストの責務 |
| AI 向け | 明示的な関数シグネチャ、暗黙の数値変換なし、位置付き JSON 診断、決定的な IR |

配列型は `[i32]` のように要素型だけを指定します。
`ref values[start..end]` で終了位置を含まない共有スライスを作り、コピーなしで読み取れます。型は `ref [T]` です。
`Array.set values index value`／`Array.update`／`Array.swap` は所有値を受け取り更新値を返します。元値を後で使う Copy 配列は独立コピーを保ちます。
`new [i32](count, i -> i as i32)` は実行時の `count: i64` 個の要素を添字順に初期化します。
`[1, 2, 3]` のリテラルも使え、生成後の長さ・要素は不変です。
旧 `[i32; 4]` 形式は `[i32]` へ移行してください。

連結リスト型は `[|i32|]`、リテラルは `[|1, 2, 3|]`、空リストは `[||]` です。
`new [|i32|](count, i -> i as i32)` で実行時にも生成できます。
配列と同じく `.length` と読み取り専用の `xs[index]` を使えますが、リストの添字アクセスは O(n) です。
配列・リストとも入れ子の要素は変更不可で、可変参照を要素に格納することも禁止します。
`let mut` ではコレクション全体を別の値に置換できますが、既存の要素は変更できません。

```text
let local = [1, 2, 3]            // 要素はスタック（関数のフレーム）
let nodes = [|1, 2, 3|]          // ノードもスタック
let text = "hello"               // リテラルの静的領域を参照し、確保しない
let heap = new [1, 2, 3]         // ヒープ
let list = new [|1, 2, 3|]       // ノードごとにヒープ
let sized = new [i64](4, i -> i) // 実行時に長さを決める生成もヒープ
```

型は記憶域によらず同じ `[i64]`／`[|i64|]` です。スタックの値を戻り値・値渡し・捕捉などで束縛から移すときは、
その時点で要素をヒープへ移すため、安全性と値の意味は変わりません。返すことが分かっている値は `new` で作るとこのコピーを省けます。

文字列の既定型 `string` は [ECMA-262 の String 値モデル](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html#sec-ecmascript-language-types-string-type)
に従う UTF-16 コード単位列です。`"😀".length` は 2、索引・`for…in` の要素型は `i16u` で、
`\uD800` のような孤立サロゲートも保持します。JavaScript のオブジェクト機構や String iterator を導入するものではありません。
従来の UTF-8 は `utf8string` と `u8"..."` リテラルで使え、長さ・索引・列挙は引き続きバイト単位です。
長さは `.length` のほか、共有借用の `String.length ref text`／`Utf8String.length ref bytes` でも取得できます。
`String.from_utf8 ref bytes`／`Utf8String.from_string ref text` で明示的に変換し、
UTF-8 にできない孤立サロゲートはトラップします。置換する場合だけ `String.to_well_formed ref text` を使います。
複製はそれぞれ `clone_string`／`Utf8String.clone` です。詳しくは [文字列の仕様](docs/language.md#string-と-utf8string) を参照してください。

文字型は UTF-16 コード単位の `char`（`'A'`）と Unicode スカラーの `utf8char`（`u8'😀'`）です。
`String`／`Utf8String` は検索・分割・連結・置換・切り出し・ASCII 変換を提供します。
`String.split "" "😀"` と `String.chars "😀"` は2要素、UTF-8 版は1要素です。
`Char`／`Utf8Char` の明示的な整数変換を使い、既存の文字列索引・列挙の要素型は変えません。

文字列補間は `$"x = {x}, y = {y:.2}"`（UTF-8 は `u8$"..."`）と書き、波括弧は `{{`／`}}` で表します。
穴の値は借用して `Display` で表示し、結果は一回の確保で組み立てます。
書式は `[[fill]align][+][width][.precision][type]`（type は `x X o b e f`）で、幅は Unicode スカラー数で数えます。数値は runtime が一度だけ最近接・偶数丸めで書式化するため、native と WASM で同じ結果になります。
record・union の穴は `instance Format<T>` が書式文字列を受け取り、幅の調整まで自分で行います（補助関数 `Format.parse`／`Format.pad`）。

Web 向けの小さな計算モジュールという方向性は
[fsw のネイティブコンパイラ](https://github.com/tatsuya-midorikawa/fsw/tree/feat/fsw-native-compiler)
を参考にしています。fsw の直接 WASM 出力とは異なり、Tsuzuri は **LLVM を共通基盤** にして
ネイティブ／WASM の両方へ出力します。入出力を使わない生成 WASM は JavaScript ランタイムのインポートを必要としません。

## 関数と型クラス

`def name :: 型 = \引数 -> 本体` で型と実装をまとめて記述します。引数は半角スペースで区切ります。従来の `def` と `fn`／`let` を分離する形式も受理します。
同じ型変数は同じ型を表し、呼び出しごとに引数・返却値の文脈から具体型を決めます。
関数名は `calculate_total` のようなスネークケース（`snake_case`）を推奨します。構文上の必須条件ではありません。

```text
def add :: Add<'a> -> 'a -> 'a = \x -> \y -> x + y

def identity :: 'a -> 'a = \x -> x

let add20 = add 20i32
let integer = add20 22
let decimal: d128 = add 0.1d128 0.2d128
let text = identity "こんにちは"
integer
```

`Add<'a>` は `Add` 制約を持つ `'a` です。`def add :: 'a -> 'a -> 'a` と書いて本体から
制約を推論することも、`Add<'a> => 'a -> 'a -> 'a` と明記することもできます。
複数の制約は `def` の次の行にインデントして、型変数ごとに指定できます。既存の構文も引き続き使えます。

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

すべての関数はカリー化され、`add 20 22` と `(add 20) 22` は同じ適用です。
型クラスによるメソッド選択はコンパイル時に完了し、辞書や型クラスの
動的ディスパッチを実行時に持ち込みません。利用した型の組み合わせごとにコードを生成します。
比較演算子は値を消費せず、`Eq`／`Ord` のメソッドは共有借用を受け取ります。算術メソッドは従来どおり値を受け取ります。

匿名関数は `\x -> x + offset` のように外側の値を捕捉できます。
`\x -> \y -> \z -> x + y + z` のようにネストすることもできます。
捕捉した所有値は関数値が管理し、関数値のコピーでは捕捉環境を独立したスナップショットにします。
文字列などの捕捉にはコピーコストがありますが、GC・参照カウントは不要です。
共有借用の寿命も検査し、排他借用を再利用可能な関数値へ保存することは拒否します。
完全適用された既知の関数は、環境を確保せず直接呼び出します。
実行例は `tsuzuri run examples/currying` です。

型クラスは `Classes.tt` に記述します。一つのファイルに複数のクラスを宣言できます。

```text
class Score<'a> {
    def score :: ref 'a -> i32
}

class Size<'a> {
    def size :: ref 'a -> i64
}
```

レコードとインスタンス実装は `Main.tz` などのコードファイルに置きます。

```text
record Point { x: i32, y: i32 }

instance Add<Point> {
    fn add left right = Point { x: left.x + right.x, y: left.y + right.y }
}

instance Classes.Score<Point> {
    fn score point = point.x + point.y
}
```

上の `add` を `Point` にも使え、`Classes.Score.score ref point` で独自クラスのメソッドを呼べます。
例は `tsuzuri run examples/polymorphism`。型クラス名でメソッドを明示することで、
同名関数の探索やインスタンスの選択順に依存しない記述にしています。
同じクラス・型のインスタンス重複や、組み込みインスタンスの上書きはエラーです。

型パラメーターと型引数は、レコード・union・型クラス・制約・インスタンス・`Task<T>` のすべてで
`Name<'a, 'b>`／`Name<i64, string>` のように `<...>` 内へカンマ区切りで書きます。
旧来の空白区切りは受理しません。関数の型変数の暗黙の全称量化と、値からの型推論は維持します。
レコードリテラルの型引数はフィールドの値や期待型から推論します。

```text
record Pair<'a, 'b> { first: 'a, second: 'b }

def swap :: Pair<'a, 'b> -> Pair<'b, 'a> = \pair -> Pair { first: pair.second, second: pair.first }

let pair: Pair<string, i64> = swap (Pair { first: 42, second: "answer" })
pair.second
```

具体化ごとに別の LLVM 型を生成し、Copy・move・借用・レイアウトは置換後のフィールド型から決まります。
詳細は [言語仕様](docs/language.md#ジェネリックなレコード) を参照してください。

代数的データ型は `union` で宣言します。case は payload を 0 個か 1 個持ち、
payload のある case は一引数の関数値としても使えます。

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

初版はランク1の関数多相です。条件付きの汎用インスタンス、スーパークラス、デフォルトメソッドに対応します。高階型は未対応です。
配列・リスト・タプルは、要素の `Eq`／`Ord` を使って借用のまま構造比較できます。
公開する関数も言語内ではカリー化され、C／WASM 境界では全引数を渡す既存ABIを維持します。
旧 `fn name :: ...` は `def name :: ...` へ置き換えます。
旧形式 `fn add(x: i32, y: i32) -> i32 { x + y }` と `add(20, 22)` は互換用に受理します。
詳細と制約一覧は [言語仕様](docs/language.md#多相関数と型クラス) を参照してください。

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

`for…to`／`downto` は F# と同じく **i32 の両端を含む反復**です。
`for…in` は配列・連結リスト・整数範囲 `start .. [step ..] finish` に対応し、
string は UTF-16 コード単位（`i16u`）、utf8string は UTF-8 バイト（`ubyte`）を列挙します。
ループとその本体は `unit`、条件は `bool` です。
`if condition then value else other`、`elif`、unit を返す `else` 省略も使えます。
従来の `{ ... }` ブロック、`if condition { ... } else { ... }`、`x -> ...` も維持します。

パターンには定数・変数・`_`／`otherwise`、タプル、レコード、union case、配列、リスト、
`head :: tail`、OR／AND、`as`、型注釈を使えます。
単一ケースの全域アクティブパターンと、bool を返す部分アクティブパターンもあります。
明示の `match` と関数ガードは網羅性をコンパイル時に検査し、一致しない値があれば不足する値の例とともに
`E1021` のエラー、前の節だけで覆われる節は `W1003` の警告です。`when` 付きの節は網羅に数えません。
`for`・ラムダ式の分解パターンは従来どおり、一致しない値で実行時にトラップします。
対象は **Tsuzuri の型と所有権モデル**であり、.NET の型テスト・null や
`IEnumerable` 全般との互換を意味しません。コレクションの記号は従来どおり、
配列が `[ ... ]`、連結リストが `[| ... |]` です。

再帰関数は `def rec name :: 型 = ラムダ式` と書きます。相互再帰は先頭を `def rec`、
後続を `and name :: 型 = ラムダ式` で記述します。分離形式では宣言と実装の再帰指定を対応させます。
ループは LLVM の直接分岐、配列は連続走査、リストは一方向走査へ下げます。
`match` 内の直接自己末尾再帰も `-O0` からループ化します。
速度の実測と未達の比較は [制御構文のベンチマーク](docs/benchmarks.md#制御構文の比較)、
詳しい契約は [言語仕様](docs/language.md#制御構文)、
実行例は `tsuzuri run examples/control` を参照してください。

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

展開後も通常の型推論・所有権・借用検査を通ります。継続は通常の関数値なので、
外側の可変状態の共有や、一回実行のタスクの捕捉を勝手に許可することはありません。
呼び先が分かり外へ逃げない継続は特殊化し、読み取り専用の捕捉環境の複製・間接呼び出しを省きます。
同じ最適化を通常の高階関数・パイプライン・コレクション初期化にも適用します。
未知・逃げる継続は従来の所有する関数値を使います。
性能の条件と手書き Tsuzuri／C++ との実測は [コンピュテーション式の比較](docs/benchmarks.md#コンピュテーション式の比較) を参照してください。
`match!`と`and!`、ビルダーが提供する`MergeSources`／`BindReturn`／`Bind2`に対応します。and!の右辺は左から右に一度ずつ評価し、自動並列化しません。
F#の全機能互換ではなく、例外処理・カスタム演算は未対応です。try構文はE1018で拒否し、所有値のlet/dropとOption/Resultを使います。`use`／`use!`はDropを持つ値の束縛で、letと同じscopeの終わりに解放します。
実行例は `tsuzuri run examples/computations`、
詳細は [ビルダーの仕様](docs/language.md#コンピュテーション式) を参照してください。

## タスクと並列処理

```text
let computation = task {
    let! values = Task.parallel [
        task { return 20 },
        task { return 22 }
    ]
    return values[0] + values[1]
}

Task.run computation
// 42
```

`task { ... }` は `Task<T>` 型の **遅延・一回実行の計算** を作ります。
F# の通常の `task` と異なり、作成しただけでは開始しません。
`let!` で前の計算の結果を受け取り、`return` で結果を返します。
独立した計算は `Task.parallel` に配列で渡し、入力順の結果配列を受け取ります。
`Task.run` は計算を消費して実行し、すべての子処理と結果の公開が完了してから戻ります。常駐スレッド自体は待機して再利用します。
スレッドの開始・join・detach をユーザーが管理する必要はありません。

捕捉する値は Copy／move でタスク自身が所有します。参照や、借用を保持した関数値の
持ち込みを拒否するため、所有者の寿命や共有可変状態を気にせず処理を分離できます。
同じタスクの二重実行はコンパイルエラーです。未実行のままスコープを出たタスクは、
本体を実行せず捕捉値を解放します。計算を繰り返す場合は、タスクを返す関数を呼び直します。

ネイティブの並列区間は POSIX threads を使い、利用可能 CPU 数と最大32実行スレッドを目安に、
ランタイム全体で追加スレッド数を制限した常駐プールを遅延起動します。入れ子でも呼び出し元が自分の仕事を進めます。
WASM の既定はインポート不要の **逐次フォールバック** です。ホストの非同期 I/O、
外部キャンセルトークンは未対応で、`Task.run` は呼び出し元をブロックします。worker はプロセス終了時に join します。
小さい仕事では確保・コピー・同期が支配する場合があり、常駐化だけで常に速くなるわけではありません。
`Parallel.init`／`map`／`map_ref`／`reduce`／`sum` は Task 配列を作らず固定チャンクで処理します。
例えば `let values = Parallel.init 10000 (index -> index * index)` の後、`Parallel.sum (ref values)` で集計できます。
reduce/sum の順序は逐次 Array 版と異なります。詳細は [データ並列 API](docs/language.md#データ並列-api) を参照してください。
実行例は `tsuzuri run examples/tasks`、詳細は [タスクの仕様](docs/language.md#タスク) を参照してください。

Resultを返す仕事には`Task.parallel_results`を使えます。最小入力indexのErrorを返し、未開始分を停止して捕捉値を回収します。開始済みは全件joinし、trapをErrorへ変換しません。

```text
let jobs: [Task<Result<i64, string>>] = [task { Result.Ok 20 }, task { Result.Error "failed" }]
match Task.run (Task.parallel_results jobs) with
| Result.Ok values -> Array.sum (&values)
| Result.Error _ -> -1
```

## ファイルとモジュール

**モジュール名は拡張子を除いたファイル名で決まり、1 ファイルに 1 モジュールを強制します。**
`module` 宣言、入れ子のモジュール、複数ファイルへの同一モジュールの分割はできません。
同じディレクトリの `.tz`・`.tt`・`.tc` ファイルを自動で読み込みます。
インポート宣言やファイルの列挙は不要です。

| 拡張子 | 内容 |
|---|---|
| `.tz` | レコード・union・関数・型クラスのインスタンス実装。`Main.tz` だけはトップレベルの実行コードも可 |
| `.tt` | 複数の型クラスの宣言。関数本体・レコード・union・インスタンスは置かない |
| `.tc` | 一つのビルダーの操作と補助関数・レコード・union・インスタンス。トップレベル実行は不可 |

拡張子が違っても同名のモジュールにはできません。例えば `Checked.tz` と `Checked.tc` の併存はエラーです。
旧 `.tzr` は入力として受理しません。コードを `.tz` へ改名し、`class` 宣言を `.tt` に分離してください。

`Point.tz`:

```text
record Point { x: f64, y: f64 }

def distance :: Point -> f64 = \point -> sqrt (point.x * point.x + point.y * point.y)

export def hypotenuse :: f64 -> f64 -> f64 = \x y -> distance (Point { x: x, y: y })
```

同じディレクトリの `Main.tz`:

```text
let p = Point { x: 10.0, y: 20.5 }
let d = Point.distance p
d
```

通常の `fn` も別ファイルから `モジュール名.関数名` で呼べます。
`export` はモジュール間の可視性ではなく、C／WASM ホストへの公開指定です。
レコード名は一意なら `Point`、明示する場合は `Point.Point` と書けます。
同名のレコードが複数モジュールにある場合、他モジュールからは修飾名で区別します。

宣言は既定で public です。モジュール内だけで使う補助関数・レコード・union には `private` を付けます。
`fn` 実装は `def` の可視性を継承し、他モジュールからの参照は `E1022` です。

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

```text
private def square :: f64 -> f64 = \x -> x * x

def length :: Point -> f64 = \point -> sqrt (square point.x + square point.y)
```

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

## ビルド

LLVM を導入せずに使う場合は、コンパイラ・Clang・LLD・SDK/libc をまとめたホスト別の配布物（`tsuzuri-<version>-<host>.tar.gz`）を使えます。
導入・確認・削除の手順は[はじめに](_docs/get-started.md#配布物を使う)を参照してください。
コンパイラは各ツールを環境変数 → 配布物の `bin/` → `PATH` の順に探します（`tsuzuri toolchain info` で確認できます）。

- Rust 1.85 以降。浮動小数点リテラルの正確な丸めに `rustc_apfloat` を使います。
- LLVM/Clang 17 以降。WASM のリンクには `wasm-ld`（LLD）も必要です。
- 検証には Node.js 20 以降と Python 3.9 以降。
- macOS/Linuxに加えx86_64 Windows MSVC ABIの実装とCIを追加しています。Windowsは実SDKでO0/O3クロスリンク済みですが、Windows runnerでの実行結果は未確認です。OS API（File など）はWindowsでは未対応で、native の exe／object は `E2002` です。
- POSIXの `Task.parallel` は pthread ヘッダーとライブラリが必要です。WindowsではWin32常駐threadを使います。
  `build`／`run` はランタイムを同梱します。オブジェクトを C/C++ ホストへリンクするときは `-pthread` を付けます。

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
標準ライブラリはコンパイラに埋め込まれているため、これらも追加のファイルなしで動きます。
コンパイラはシェルを経由せずツールを起動し、失敗したツールの診断を報告します。

WindowsではLLVMの`clang`、Windows SDK、Visual Studio Build Tools（MSVC/CRT）を用意し、Developer PowerShellでビルドします。clang-cl専用の引数形式は対象外です。
compilerは`--target=x86_64-pc-windows-msvc`を指定し、`-fPIC`/`-pthread`/`-lm`を渡しません。公開wrapperはdllexportで、出力拡張子はexe/objです。
Task/CPU runtimeを埋め込むCOFF object結合はE2002です。exeへ直接ビルドするか、--emit llvmとruntime/task.cを一度だけホストへリンクします。
UTF-8 bytesを出力しコードページを変更しません。対話コンソールはWindows Terminal等のUTF-8対応環境を推奨します。PDB/ARM64/MinGW/DLL import library自動生成は対象外です。

## コンソール

```sh
./target/release/tsuzuri check examples/hello/Main.tz
./target/release/tsuzuri run examples/hello/Main.tz
# 5050

./target/release/tsuzuri run examples/functional
# 42

./target/release/tsuzuri build examples/hello/Main.tz -o target/hello
./target/hello
```

`Main.tz` のトップレベルの結果、または引数なしの `main` の返却型は
数値型／`bool`／`unit`／`string`／`utf8string` です。ネイティブ用ホスト・ラッパーが
結果を表示し、成功時は終了コード 0 を返します。`unit` は何も表示しません。
言語内に出力の副作用を持ち込む仕組みではありません。
数値は `to_string`／`Display.display` と同じ形式です。二進浮動小数点は最短の往復可能な十進表現
（`0.1` は `0.1`）、負のゼロは `-0` と表示し、native／WASM で共通の実装を使います。
`to_string value` は値を消費し、`Display.display ref value` は借用します。
`let value: Option<f64> = Parse.parse ref text` のように解析でき、不正入力・overflow は `None` です。
独自型にも Display／Parse インスタンスを定義できます。
文字列の出力は UTF-8 です。string の孤立サロゲートは暗黙に置換せずトラップし、
必要なら `String.to_well_formed` で明示的に置換します。

## Web／ゲーム

```sh
./target/release/tsuzuri build examples/web/Physics.tz --target wasm32 -o examples/web/physics.wasm
python3 -m http.server 8000 --bind 127.0.0.1 --directory examples/web
```

`http://127.0.0.1:8000` を開くと、WASM の純粋な物理関数でボールが反射するデモが動きます。
描画、入力、フレーム時刻は JavaScript 側で扱います。文字列を使う WASM には
解放領域を再利用・結合する allocator を同梱し、GC や JavaScript ランタイムは使いません。
FPS 表示は描画とブラウザーのスケジューリングを含み、コンパイラ性能の指標ではありません。

Node.js でも通常の WebAssembly API から呼び出せます:

```sh
./target/release/tsuzuri build examples/functional/Main.tz --target wasm32 -o target/functional.wasm
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
def length :: ref string -> i64 = \text -> text.length
def replace :: ref mut string -> unit = \text -> { deref text = "updated"; }

def main :: string = {
    let mut text = "こんにちは";
    let size = length ref text; // 借用後も所有者を使える
    replace ref mut text;       // この呼び出し中は排他的に借用
    let result = text;      // 所有権を移動。以降の text の使用はエラー
    result
}
```

借用は `ref x`（共有）、`ref mut x`（排他）、参照先は `deref r` と書きます。
`ref`／`ref mut` は参照に対して使うと一段だけ貸し直すため、所有値か参照かで書き分ける必要はありません。
Rust との互換のため `&x`／`&mut x`／`*r`／`&mut *r`／`&*r` と `&T`／`&mut T` も受理し、
どちらの表記も同じコードを生成します。キーワードの被演算子は一つの項で、`f (ref mut x) 1` のように
後続の引数があるときは括弧で区切ります。

数値・bool・unit・関数値・共有参照は Copy です。関数値のコピーは捕捉環境の複製を伴う場合があります。
レコードと配列も全要素が Copy なら Copy、それ以外は move します。
`byte` は `i8`、`ubyte` は `i8u` の別名です。旧 `Int`／`Float`／`Bool`／`Unit` は廃止しました。
型注釈や `1i32`／`0.1d128` のような接尾辞で型を選べます。
所有者より長生きする参照、借用中の move、共有借用と排他借用の競合をコンパイル時に拒否します。
record・union に `instance Drop<T> { fn drop value = ... }` を書くと、値が終わる時点（scope の終わり、代入での置き換えなど）で
`drop` を一度だけ呼びます。ファイルやホストのハンドルを閉じる RAII に使えます。
`use` 束縛はその値が `Drop` を持つことを検査し、`Owned.drop` はその場で解放します。
資源を捕捉する関数値は `Owned.function` で作り、`Owned.call` で借用したまま呼びます（複製できない関数値）。
Rust の全機能を実装するものではなく、名前付きライフタイム、借用を含むレコード、
一時値からの直接の借用にはまだ対応していません。詳しくは [言語仕様](docs/language.md) を参照してください。

## ネイティブ・ホスト／デスクトップ

同じ `Physics.tz` を C/C++、ゲームエンジン、GUI ホストへ組み込めます:

```sh
./target/release/tsuzuri build examples/web/Physics.tz --emit object -o target/examples/physics.o
./target/release/tsuzuri build examples/web/Physics.tz --emit header -o target/examples/physics.h
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

## GPU（実験的）

単一のexportされた`i32 -> i32`または`i32u -> i32u` kernelを持つprojectから、`tsuzuri build Kernel.tz --emit wgsl -o kernel.wgsl`でWGSLを生成できます。
Nodeの実行例は`node examples/gpu/run.mjs kernel.wgsl`です。検証用bindingは`npm install --prefix target/webgpu-runtime --no-save --package-lock=false webgpu@0.6.1`で導入でき、コンパイラの配布依存には含みません。
同梱host試作はWebGPU bufferをdevice上に保持し、明示的に読み戻します。GPUなし・shaderエラー・資源上限はエラーで、CPUへ黙って切り替えません。
通常の言語APIでは`Gpu.request Gpu.CpuReference`だけが成功し、他backendの要求はUnavailableです。64-bit/floatはCPU参照のみで、strict WGSLでは拒否します。
詳細は[言語仕様](docs/language.md#gpu-kernel実験的-phase-1)を参照してください。自動GPU選択や速度優位を保証する機能ではありません。

## CLI

```text
tsuzuri check source.tz|source.tt|source.tc|directory [--json]
tsuzuri doc source.tz|source.tt|source.tc|directory -o outdir [--json]
tsuzuri [build] source.tz|source.tt|source.tc|directory [options]
tsuzuri run Main.tz|directory [-O0|-O1|-O2|-O3] [--cpu generic|native] [--json]
```

| オプション | 内容 |
|---|---|
| `-o`, `--output PATH` | 出力先。親ディレクトリは作成される |
| `--target native\|wasm32\|wasm64` | 既定は native。wasm64 は 64-bit の線形メモリ（memory64） |
| `--emit exe\|object\|llvm\|header\|wasm` | 既定は native なら exe、wasm32・wasm64 なら wasm |
| `-O0` ～ `-O3` | 既定は `-O3`。fast-math は使わない |
| `--cpu generic\|native` | 既定は `generic`（Clang のターゲット既定）。`native` はビルド機の命令セットとスケジューリングに最適化 |
| `--wasm-max-memory SIZE` | WASM の build（object・llvm・wasm）と test の線形メモリ上限。既定 16MiB、64KiB の倍数で wasm32 は最大 4GiB-64KiB、wasm64 は最大 16GiB |
| `--wasm-stack-size SIZE` | WASM 出力と test の main stack。既定 1MiB、16 の倍数で 64KiB 以上 |
| `--wasm-host wasi` | wasm32 の object・WASM で標準入出力と OS API を WASI preview1 の import へ下げる（build だけ。LLVM IR と threads とは併用不可）。指定がなく OS API に到達すると `E2000` |
| `--json` | 標準エラーへ 1 行 1 JSON オブジェクトで診断を出力 |
| `--` | 以降をパスとして解釈 |
| `--help`, `--version` | ヘルプ／バージョン |

`///`で宣言に説明を添え、`tsuzuri doc examples/point -o target/api`で公開APIのMarkdownを生成できます。
関数の説明は`fn`ではなく`def`の前に書きます。record/union/type/const/extern/classとclass methodにも対応し、誤配置はE0002です。
`doc`は全ソースを型検査し、Mainのないライブラリディレクトリも受理します。LLVM/Clangは不要です。
出力先は必須で、既存ディレクトリは正しい`.tsuzuri-docs` markerがある場合だけ置換します。説明のMarkdown/HTMLはそのままなので、表示側で安全性を管理してください。
`tsuzuri doc std -o target/std-api`は同梱stdと同じファイル構成のソース宣言を文書化します。生成物はcommitせずrelease artifactにできます。

ローカル実行や実行機が固定された配備では、`tsuzuri run examples/point --cpu native`、
または `tsuzuri build examples/point --cpu native -o target/point` を使えます。
`native` は x86/x86-64 では `-march=native`、ARM/AArch64 では `-mcpu=native` を
Clang に渡します。SIMD 化は演算と依存関係が許す範囲で LLVM が判断し、全処理の SIMD 化や
全 CPU コアの利用を保証する指定ではありません。生成物は古い CPU で動かない場合があります。
他機への配布では `generic` を使い、OS・アーキテクチャ・ABI の互換性も確認してください。
nativeの同梱`Array.sum<i64>`は能力検出後に標準カーネルを選択します。x86はSSE4.2/AVX2、AArch64と未知環境はbaselineです。
`TSUZURI_CPU_FORCE=baseline|sse4.2|avx2`はテスト用です。未対応/未知variantは明示失敗し、環境変数は初回呼び出し前だけ設定します。
`--emit llvm`と従来のLLVM APIは独立した従来経路を出し、native exe/objectだけruntimeを必要時に連結します。`--cpu native`生成物の移植性は従来どおり保証しません。
`--cpu native` はネイティブの実行ファイル／オブジェクトと `run` 専用です。
`check` への CPU 指定、WASM／LLVM IR／ヘッダーへの `native` 指定はエラーにします。

`build --target wasm32 --wasm-feature simd128` はWASM SIMD128を明示的に有効にします。既定はSIMDなしで、relaxed SIMD・fast-mathは有効にしません。

WASMは既定でstack 1MiB・線形メモリ上限16MiBです。`build --target wasm32 --wasm-max-memory 256MiB --wasm-stack-size 4MiB`のように、SIZE（バイト数か`KiB`/`MiB`/`GiB`付きの整数）でwasm32は最大4GiB-64KiB、wasm64は最大16GiBまで変更できます。`test --target wasm32`／`wasm64`も同じ指定を受けます。
既定値の明示と省略は同じ成果物です。`--emit object`/`llvm`はヒープの上限だけを持つため、自分でリンクするときはwasm-ldへ同じ`--max-memory`を渡します。stack指定はWASM出力とtestだけです。
root packageの`Tsuzuri.toml`の`[wasm]`に`max-memory = "256MiB"`・`stack-size = "4MiB"`を書くと既定値になり、コマンドラインの指定が優先します。依存packageの`[wasm]`は読みません。
wasm32で2GiBを超える上限とthreadsでは、各関数がframe確保後にstackの範囲を検査し、溢れはframeを使う前にトラップします（O3の計測で、呼び出しの多い再帰は1.1〜1.4倍、ループ中心の処理は誤差内）。
`--target wasm64`はpointerが64-bitになり、ホストABIのpointerはBigIntです。memory64対応のNode.js 24以降などのエンジンが必要で、threadsはwasm32だけです。

`build --target wasm32 --wasm-feature threads`はTask/ParallelをNode Workerへ分散します。WASM/object専用でsimd128と併用可能です。
同梱のNode.js 20+ホストを使用してください。未対応hostやWorker失敗を逐次成功に置き換えません。

```javascript
import { readFile } from "node:fs/promises";
import { createThreadPool } from "./src/runtime/wasm-threads.mjs";
const pool = await createThreadPool(await readFile("app.wasm"));
try { console.log(pool.call("tz_answer")); }
finally { await pool.close(); }
```

main（既定1MiB）、各Worker 256KiBのstackを含め共有memoryは既定で最大16MiBです。同梱ホストはmoduleが宣言した最大page数（`--wasm-max-memory`）を最初に確保し、各Workerのstack範囲を`tsuzuri_stack_base`・`tsuzuri_stack_top`へ設定します。Browser用本番glueは対象外で、[Webホスト要件](examples/web/README.md)を参照してください。
SIMD128を指定した成果物には対応エンジンが必要です。`--emit llvm`ではSIMD要件をコメントに記録し、そのIRのコンパイルには `-msimd128` を指定します。

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
sh scripts/check-runtime-includes.sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/strings.mjs target/release/tsuzuri
node tests/tasks.mjs target/release/tsuzuri
node tests/wasm_threads.mjs target/release/tsuzuri
node tests/wasm_memory.mjs target/release/tsuzuri
node tests/trap_boundary.mjs target/release/tsuzuri
node tests/trap_return.mjs target/release/tsuzuri # native --trap-mode return, the C trap runtime (asan/ubsan/tsan builds); Linux and macOS
node tests/stack_overflow.mjs target/release/tsuzuri # native stack overflow report; Linux and macOS
npx --yes --package=node@24 node tests/wasm64.mjs target/release/tsuzuri # memory64 requires Node.js 24 or newer
node tests/gpu.mjs target/release/tsuzuri # CPU/reference validation; TSUZURI_WEBGPU=1 enables actual WebGPU tests
node tests/cache.mjs target/release/tsuzuri
node tests/computations.mjs target/release/tsuzuri
node tests/control.mjs target/release/tsuzuri
node tests/numeric_casts.mjs target/release/tsuzuri
node tests/integer_intrinsics.mjs target/release/tsuzuri
node tests/display_parse.mjs target/release/tsuzuri
node tests/examples.mjs target/release/tsuzuri
node tests/features.mjs target/release/tsuzuri # TSUZURI_TEST_WASM_TARGET=wasm64 under Node.js 24 runs its WASM cases on wasm64
node tests/lsp_sessions.mjs target/release/tsuzuri
node tests/docgen.mjs target/release/tsuzuri
node tests/wasm_simd.mjs target/release/tsuzuri # llvm-objdump required; TSUZURI_OBJDUMP overrides it
node tests/simd.mjs target/release/tsuzuri # same llvm-objdump requirement
node tests/cpu_dispatch.mjs target/release/tsuzuri
node tests/host_imports.mjs target/release/tsuzuri
node tests/user_drop.mjs target/release/tsuzuri
node tests/ffi_extensions.mjs target/release/tsuzuri # wasm64 runs under Node.js 24 or newer; older engines only link it
node tests/io.mjs target/release/tsuzuri
node tests/os.mjs target/release/tsuzuri # macOS and Linux; the WASI cases run under node:wasi
node tests/debug_info.mjs target/release/tsuzuri # llvm-dwarfdump required; TSUZURI_DWARFDUMP overrides it
python3 -m venv target/math-reference-env
target/math-reference-env/bin/python -m pip install mpmath==1.3.0
node tests/math.mjs target/release/tsuzuri
node benchmarks/run.mjs target/release/tsuzuri
node benchmarks/run-cpp.mjs target/release/tsuzuri
node benchmarks/run-computations.mjs target/release/tsuzuri
node benchmarks/run-control.mjs target/release/tsuzuri
node benchmarks/run-managed.mjs target/release/tsuzuri --scale 0.1
```

C/C++・Rust・C#・JavaScript の36種目比較には Node.js 24 以降と .NET SDK 10 を使います。
Math参照テストのPythonは`TSUZURI_MATH_PYTHON`で変更できます。`--quick`は境界集合、`--basic-only`／`--elementary-only`は対象を限定します。
ランタイム再生成にはClangとllvm-linkの17〜21系を揃えます。検証済みはApple Clang 21＋llvm-link 21です。
C の基準は15種目、C++・Rust・C#・JavaScript の基準は36種目です。
`run-managed.mjs --quick` は速度を評価せずチェックサムを照合し、通常測定は JIT のウォームアップ後の経過時間と GC 情報を保存します。
対応範囲・比較不能な型・残る性能差・再現条件は [docs/benchmarks.md](docs/benchmarks.md) を参照してください。

コンパイラが `include_str!` で埋め込むランタイム IR（`src/runtime/*.ll`）はリポジトリに同梱されており、
新しい clone でもそのままビルドできます。`scripts/check-runtime-includes.sh` は埋め込み対象が存在し、
Git で追跡され、`.gitignore` に無視されていないことを検査します。

末尾再帰の変更前後も比べる場合は、
`node benchmarks/run-control.mjs target/release/tsuzuri --baseline <変更前のコンパイラ>` を使います。
両コンパイラと C／C++／Rust の同じ計算を、一つのプロセス内で順序を入れ替えて測定します。

境界値・NaN・短絡評価・高階関数・レコード・配列・100 万回の末尾再帰を、
実際のネイティブコードと WASM の `-O0`／`-O3` で照合します。
失敗時の出力保護、JSON 診断、再現性、決定的なソース変異、LLVM が生成する 128-bit 演算補助も検証します。
追加のプリミティブ、decimal の参照演算との照合、move／借用の失敗例、
文字列の二重解放・解放漏れと、WASM ヒープ使用量の上限も検証します。
スタックに置くリテラルと `new` の記憶域は、ネイティブの確保回数・未解放バイト数で照合します。
`TSUZURI_BROWSER` に Chrome の実行ファイルを設定すると実ブラウザーの起動確認も追加できます。

[言語仕様と ABI](docs/language.md) · [コンパイラ構成と開発指針](docs/architecture.md) ·
[ベンチマークの条件と読み方](docs/benchmarks.md)
