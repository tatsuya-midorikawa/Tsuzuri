# Tsuzuri

**AI と人間が、少ない暗黙ルールで堅牢なプログラムを書けることを目指す、関数型の式と所有権モデルを備えた言語。**
コードは **`.tz`**、型クラス宣言は **`.tt`**、コンピュテーション式のビルダー実装は **`.tc`**。
Rust 製のフロントエンドで型検査し、LLVM によりネイティブコードと WebAssembly を生成します。

現在は **0.1.0 — 計算カーネルを実行できる初版** です。コンソール実行、C ABI、
ネイティブのデスクトップ・ホスト、ブラウザーのゲーム例を含みます。
C/C++ を上回る性能や C#/F# 以上の書きやすさは設計目標であり、現時点の達成保証ではありません。
**性能を最優先の設計要件の一つとし、CPU 命令・SIMD・並列 CPU・GPU のうち、
意味を保ち実処理が最も速くなる経路を内部で選ぶことを目指します。**
この方針はコンパイラだけでなく、組み込み関数と今後の標準ライブラリにも適用します。
現状は LLVM の CPU 最適化・自動ベクトル化と `--cpu native` に対応し、
`Task.parallel` による明示的な CPU 並列処理も使えます。
GPU バックエンドと自動マルチスレッド化は未実装です。
伸縮可能なコレクション、ジェネリックなレコード型、
パッケージ管理、GUI/OS の標準ライブラリは未実装です。
メモリは GC ではなく、Rust と同様に所有権の移動・借用・スコープ終了時の解放で管理します。
記憶域は C/C++ と同じ方式で、`new` で生成した値はヒープ、`new` を使わずに生成して束縛した値はスタックに置きます。

`Main.tz`:

```text
def rec sum :: i64 -> i64 -> i64
fn rec sum n total =
    if n <= 0 {
        total
    } else {
        sum (n - 1) (total + n)
    }

export def answer :: i64
fn answer = sum 100 0

def main :: i64
fn main = answer()
```

## 設計と実装済みの範囲

| 項目 | 初版の実装 |
|---|---|
| 状態 | `let` は不変。`let mut` と排他的な `&mut T` でローカル値を置換できる。共有可変状態・I/O・外部関数インポートなし |
| 型 | `bool`、`unit`、`i8`～`i128`／`i8u`～`i128u`、`f16`／`f32`／`f64`／`f128`、`d32`／`d64`／`d128`、`byte`／`ubyte`、UTF-8 `string`、タプル、不変レコード・配列・連結リスト、捕捉環境を持つ関数値 |
| 書きやすさ | `def` と `fn`／`let`、カリー化・部分適用、`fx`、`if…then…else`、`match` とガード、`for…in`／`for…to`／`downto`／`while…do`、インデント本体、`|>`、高階関数、明示的な `rec`／`and` |
| 多相性 | `'a` によるパラメトリック多相、型クラス・具体型のインスタンスによるアドホック多相。制約推論と単相化 |
| コンピュテーション式 | `.tc` のユーザー定義ビルダー。`let!`／`do!`、`return`／`yield`、条件分岐・反復を通常の関数呼び出しへ展開 |
| タスク | `task { ... }`、`let!`／`return`／`return!`／`do!`。所有値を持つ一回実行の計算を組み合わせ、`Task.parallel` でスレッド数を制限して並列実行 |
| モジュール | 1 ファイル = 1 モジュール。複数ファイルの名前解決と `Main.tz` エントリー |
| メモリ | 所有権、move、`&T`／`&mut T` の借用検査。`new` はヒープ、`new` なしで束縛したリテラルはスタック。文字列・配列・連結リスト・捕捉環境を自動解放。GC・参照カウント・手動解放なし |
| 最適化 | 既定で LLVM `-O3`、自動 SIMD 化、基本数値変換の直接 lowering。`--cpu native` で実行機向けに最適化。直接の自己末尾再帰は `-O0` でもループ化 |
| 安全性 | 整数除算・配列／リストアクセスを検査。LLVM の未定義動作に依存しない数値仕様 |
| ホスト連携 | 64-bit 以下の整数、f32／f64、bool の C ABI と WASM エクスポート。UI／I/O はホストの責務 |
| AI 向け | 明示的な関数シグネチャ、暗黙の数値変換なし、位置付き JSON 診断、決定的な IR |

配列型は `[i32]` のように要素型だけを指定します。
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

Web 向けの小さな計算モジュールという方向性は
[fsw のネイティブコンパイラ](https://github.com/tatsuya-midorikawa/fsw/tree/feat/fsw-native-compiler)
を参考にしています。fsw の直接 WASM 出力とは異なり、Tsuzuri は **LLVM を共通基盤** にして
ネイティブ／WASM の両方へ出力します。生成 WASM は JavaScript ランタイムのインポートを必要としません。

## 関数と型クラス

型を `def` で宣言し、`fn` または `let` と匿名関数で実装します。引数は半角スペースで区切ります。
同じ型変数は同じ型を表し、呼び出しごとに引数・返却値の文脈から具体型を決めます。

```text
def add :: Add 'a -> 'a -> 'a
let add = x -> y -> x + y

def identity :: 'a -> 'a
fn identity x = x

let add20 = add 20i32
let integer = add20 22
let decimal: d128 = add 0.1d128 0.2d128
let text = identity "こんにちは"
integer
```

`Add 'a` は `Add` 制約を持つ `'a` です。`def add :: 'a -> 'a -> 'a` と書いて本体から
制約を推論することも、`Add 'a => 'a -> 'a -> 'a` と明記することもできます。
すべての関数はカリー化され、`add 20 22` と `(add 20) 22` は同じ適用です。
型クラスによるメソッド選択はコンパイル時に完了し、辞書や型クラスの
動的ディスパッチを実行時に持ち込みません。利用した型の組み合わせごとにコードを生成します。

匿名関数は `x -> x + offset` のように外側の値を捕捉できます。
捕捉した所有値は関数値が管理し、関数値のコピーでは捕捉環境を独立したスナップショットにします。
文字列などの捕捉にはコピーコストがありますが、GC・参照カウントは不要です。
共有借用の寿命も検査し、排他借用を再利用可能な関数値へ保存することは拒否します。
完全適用された既知の関数は、環境を確保せず直接呼び出します。
実行例は `tsuzuri run examples/currying` です。

型クラスは `Classes.tt` に記述します。一つのファイルに複数のクラスを宣言できます。

```text
class Score 'a {
    def score :: &'a -> i32
}

class Size 'a {
    def size :: &'a -> i64
}
```

レコードとインスタンス実装は `Main.tz` などのコードファイルに置きます。

```text
record Point { x: i32, y: i32 }

instance Add Point {
    fn add left right = Point { x: left.x + right.x, y: left.y + right.y }
}

instance Classes.Score Point {
    fn score point = point.x + point.y
}
```

上の `add` を `Point` にも使え、`Classes.Score.score (&point)` で独自クラスのメソッドを呼べます。
例は `tsuzuri run examples/polymorphism`。型クラス名でメソッドを明示することで、
同名関数の探索やインスタンスの選択順に依存しない記述にしています。
同じクラス・型のインスタンス重複や、組み込みインスタンスの上書きはエラーです。

初版はランク1の関数多相です。高階型・条件付きの汎用インスタンスは未対応です。
公開する関数も言語内ではカリー化され、C／WASM 境界では全引数を渡す既存ABIを維持します。
旧 `fn name :: ...` は `def name :: ...` へ置き換えます。
旧形式 `fn add(x: i32, y: i32) -> i32 { x + y }` と `add(20, 22)` は互換用に受理します。
詳細と制約一覧は [言語仕様](docs/language.md#多相関数と型クラス) を参照してください。

## 制御構文とパターン

```text
let mut total = 0
for (x, y) in [(1, 2), (3, 4)] do
    total = total + x + y
for i = 1 to 5 do
    total = total + i as i64
while total < 40 do
    total = total + 1
let add = fx x y -> x + y
match add total 2 with
| 0 -> 0
| answer when answer > 0 -> answer
| otherwise -> -1
```

`for…to`／`downto` は F# と同じく **i32 の両端を含む反復**です。
`for…in` は配列・連結リスト・整数範囲 `start .. [step ..] finish` に対応し、
文字列では既存の索引仕様と同じ UTF-8 バイトを列挙します。
ループとその本体は `unit`、条件は `bool` です。
`if condition then value else other`、`elif`、unit を返す `else` 省略も使えます。
従来の `{ ... }` ブロック、`if condition { ... } else { ... }`、`x -> ...` も維持します。

パターンには定数・変数・`_`／`otherwise`、タプル、レコード、配列、リスト、
`head :: tail`、OR／AND、`as`、型注釈を使えます。
単一ケースの全域アクティブパターンと、bool を返す部分アクティブパターンもあります。
対象は **Tsuzuri の型と所有権モデル**であり、.NET の型テスト・null・判別共用体や
`IEnumerable` 全般との互換を意味しません。コレクションの記号は従来どおり、
配列が `[ ... ]`、連結リストが `[| ... |]` です。

再帰関数には `def rec` と `fn rec` が必要です。相互再帰は先頭を `rec`、
続く宣言を `def and`、実装を `and` で記述できます。既存コードも明示指定へ移行してください。
ループは LLVM の直接分岐、配列は連続走査、リストは一方向走査へ下げます。
`match` 内の直接自己末尾再帰も `-O0` からループ化します。
速度の実測と未達の比較は [制御構文のベンチマーク](docs/benchmarks.md#制御構文の比較)、
詳しい契約は [言語仕様](docs/language.md#制御構文)、
実行例は `tsuzuri run examples/control` を参照してください。

## ユーザー定義のコンピュテーション式

F# のように、`Bind`・`Return` などを実装して計算の組み合わせ方を定義できます。
**一つの `.tc` ファイルが一つのビルダー**で、ビルダー名はファイル名です。
型クラスの実装や新しい構文の登録は不要です。例えば `Checked.tc`:

```text
record Result { ok: bool, value: i64 }

def Return :: i64 -> Result
fn Return value = Result { ok: true, value: value }

def Bind :: Result -> (i64 -> Result) -> Result
fn Bind result next =
    if result.ok { next result.value } else { result }
```

同じディレクトリの `Main.tz`:

```text
let answer = Checked {
    let! first = Checked.Return 20
    let! second = Checked.Return 22
    return first + second
}
answer.value
```

`let!` は `Checked.Bind(value, continuation)`、`return` は `Checked.Return(value)` に相当します。
上の `Bind` は失敗値なら続きを呼ばないため、ビルダー自身で短絡を実装できます。
`ReturnFrom`、`Yield`／`YieldFrom`、`Zero`、`Combine`、`For`／`While` も必要に応じて定義でき、
`Delay`／`Run` があれば本体を包んで遅延・実行の仕方を制御します。
使用した構文の操作が未実装ならコンパイルエラーで、暗黙の既定実装はありません。

展開後も通常の型推論・所有権・借用検査を通ります。継続は通常の関数値なので、
外側の可変状態の共有や、一回実行のタスクの捕捉を勝手に許可することはありません。
呼び先が分かり外へ逃げない継続は特殊化し、読み取り専用の捕捉環境の複製・間接呼び出しを省きます。
同じ最適化を通常の高階関数・パイプライン・コレクション初期化にも適用します。
未知・逃げる継続は従来の所有する関数値を使います。
性能の条件と手書き Tsuzuri／C++ との実測は [コンピュテーション式の比較](docs/benchmarks.md#コンピュテーション式の比較) を参照してください。
F# の全機能互換ではなく、`and!`、例外処理、`use`、カスタム演算は未対応です。
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

`task { ... }` は `Task T` 型の **遅延・一回実行の計算** を作ります。
F# の通常の `task` と異なり、作成しただけでは開始しません。
`let!` で前の計算の結果を受け取り、`return` で結果を返します。
独立した計算は `Task.parallel` に配列で渡し、入力順の結果配列を受け取ります。
`Task.run` は計算を消費して実行し、すべての子処理とスレッドの回収が完了してから戻ります。
スレッドの開始・join・detach をユーザーが管理する必要はありません。

捕捉する値は Copy／move でタスク自身が所有します。参照や、借用を保持した関数値の
持ち込みを拒否するため、所有者の寿命や共有可変状態を気にせず処理を分離できます。
同じタスクの二重実行はコンパイルエラーです。未実行のままスコープを出たタスクは、
本体を実行せず捕捉値を解放します。計算を繰り返す場合は、タスクを返す関数を呼び直します。

ネイティブの並列区間は POSIX threads を使い、利用可能 CPU 数と最大32実行スレッドを目安に、
ランタイム全体で追加スレッド数を制限します。入れ子で枠が埋まっても呼び出し元で処理を進めます。
WASM はインポート不要の **逐次フォールバック** です。WASM threads、非同期 I/O、
キャンセル、常駐ワーカープールは未対応で、`Task.run` は呼び出し元をブロックします。
小さい仕事では確保・コピー・スレッド起動の方が高くつくため、ある程度まとまった計算を渡してください。
実行例は `tsuzuri run examples/tasks`、詳細は [タスクの仕様](docs/language.md#タスク) を参照してください。

## ファイルとモジュール

**モジュール名は拡張子を除いたファイル名で決まり、1 ファイルに 1 モジュールを強制します。**
`module` 宣言、入れ子のモジュール、複数ファイルへの同一モジュールの分割はできません。
同じディレクトリの `.tz`・`.tt`・`.tc` ファイルを自動で読み込みます。
インポート宣言やファイルの列挙は不要です。

| 拡張子 | 内容 |
|---|---|
| `.tz` | レコード・関数・型クラスのインスタンス実装。`Main.tz` だけはトップレベルの実行コードも可 |
| `.tt` | 複数の型クラスの宣言。関数本体・レコード・インスタンスは置かない |
| `.tc` | 一つのビルダーの操作と補助関数・レコード・インスタンス。トップレベル実行は不可 |

拡張子が違っても同名のモジュールにはできません。例えば `Checked.tz` と `Checked.tc` の併存はエラーです。
旧 `.tzr` は入力として受理しません。コードを `.tz` へ改名し、`class` 宣言を `.tt` に分離してください。

`Point.tz`:

```text
record Point { x: f64, y: f64 }

def distance :: Point -> f64
fn distance point = sqrt (point.x * point.x + point.y * point.y)

export def hypotenuse :: f64 -> f64 -> f64
fn hypotenuse x y = distance (Point { x: x, y: y })
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

アプリケーションは **`Main.tz`** から開始します。
トップレベルの `let` と最後の結果式、または従来の `fn main` のどちらかを使います。
上の例では最後の `d` を表示します。結果式を省略すると `unit` になり、何も表示しません。
実行例は `./target/release/tsuzuri run examples/point` です。

## ビルド

- Rust 1.85 以降。浮動小数点リテラルの正確な丸めに `rustc_apfloat` を使います。
- LLVM/Clang 17 以降。WASM のリンクには `wasm-ld`（LLD）も必要です。
- 検証には Node.js 20 以降と Python 3.9 以降。
- macOS と Linux を CI 対象にしています。Windows のネイティブ・ツールチェーンは未検証です。
- ネイティブの `Task.parallel` は POSIX pthread ヘッダーとライブラリが必要です。
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
コンパイラはシェルを経由せずツールを起動し、失敗したツールの診断を報告します。

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
数値型／`bool`／`unit`／`string` です。ネイティブ用ホスト・ラッパーが
結果を表示し、成功時は終了コード 0 を返します。`unit` は何も表示しません。
言語内に出力の副作用を持ち込む仕組みではありません。

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
def length :: &string -> i64
fn length text = text.length
def replace :: &mut string -> unit
fn replace text = { *text = "updated"; }

def main :: string
fn main = {
    let mut text = "こんにちは";
    let size = length (&text); // 借用後も所有者を使える
    replace (&mut text);       // この呼び出し中は排他的に借用
    let result = text;      // 所有権を移動。以降の text の使用はエラー
    result
}
```

数値・bool・unit・関数値・共有参照は Copy です。関数値のコピーは捕捉環境の複製を伴う場合があります。
レコードと配列も全要素が Copy なら Copy、それ以外は move します。
`byte` は `i8`、`ubyte` は `i8u` の別名です。旧 `Int`／`Float`／`Bool`／`Unit` は廃止しました。
型注釈や `1i32`／`0.1d128` のような接尾辞で型を選べます。
所有者より長生きする参照、借用中の move、共有借用と排他借用の競合をコンパイル時に拒否します。
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

## CLI

```text
tsuzuri check source.tz|source.tt|source.tc|directory [--json]
tsuzuri [build] source.tz|source.tt|source.tc|directory [options]
tsuzuri run Main.tz|directory [-O0|-O1|-O2|-O3] [--cpu generic|native] [--json]
```

| オプション | 内容 |
|---|---|
| `-o`, `--output PATH` | 出力先。親ディレクトリは作成される |
| `--target native\|wasm32` | 既定は native |
| `--emit exe\|object\|llvm\|header\|wasm` | 既定は native なら exe、wasm32 なら wasm |
| `-O0` ～ `-O3` | 既定は `-O3`。fast-math は使わない |
| `--cpu generic\|native` | 既定は `generic`（Clang のターゲット既定）。`native` はビルド機の命令セットとスケジューリングに最適化 |
| `--json` | 標準エラーに機械可読の診断を出力 |
| `--` | 以降をパスとして解釈 |
| `--help`, `--version` | ヘルプ／バージョン |

ローカル実行や実行機が固定された配備では、`tsuzuri run examples/point --cpu native`、
または `tsuzuri build examples/point --cpu native -o target/point` を使えます。
`native` は x86/x86-64 では `-march=native`、ARM/AArch64 では `-mcpu=native` を
Clang に渡します。SIMD 化は演算と依存関係が許す範囲で LLVM が判断し、全処理の SIMD 化や
全 CPU コアの利用を保証する指定ではありません。生成物は古い CPU で動かない場合があります。
他機への配布では `generic` を使い、OS・アーキテクチャ・ABI の互換性も確認してください。
`--cpu native` はネイティブの実行ファイル／オブジェクトと `run` 専用です。
`check` への CPU 指定、WASM／LLVM IR／ヘッダーへの `native` 指定はエラーにします。

入力はファイルまたはディレクトリを一つ指定します。ディレクトリ指定はその直下の `Main.tz` を選びます。
どちらも同じディレクトリ直下の全 `.tz`・`.tt`・`.tc` をファイル名順に読み込み、
未参照のモジュール・ビルダーも検査します。
サブディレクトリは探索しません。ファイル名は大文字小文字を区別する ASCII 識別子で、
`_` 単独や予約語は使えません。
`run`／`--emit exe` の入力は `Main.tz` またはそのディレクトリに限ります。
`check`／ライブラリ出力では `.tz`・`.tt`・`.tc` を指定でき、`Main.tz` は不要です。

出力先省略時は選択した入力ファイルの拡張子を変更します。ディレクトリ指定なら `Main.ll` などになります。
`--emit llvm` はライブラリ用 IR で、コンソールのエントリー・ラッパーは付けません。
ネイティブの並列タスクを含む生の IR を直接リンクする場合は、
`clang kernel.ll src/runtime/task.c -pthread -lm ...` のようにタスクランタイムも渡します。
`--emit object` にはランタイム本体が含まれ、別途 C ソースを渡す必要はありません。
WASM は少なくとも一つの `export def` が必要です。
ホスト向けの公開名 `tz_name` は維持するため、エクスポート名はプロジェクト全体で一意にします。
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
node tests/tasks.mjs target/release/tsuzuri
node tests/computations.mjs target/release/tsuzuri
node tests/control.mjs target/release/tsuzuri
node tests/numeric_casts.mjs target/release/tsuzuri
node tests/examples.mjs target/release/tsuzuri
node benchmarks/run.mjs target/release/tsuzuri
node benchmarks/run-cpp.mjs target/release/tsuzuri
node benchmarks/run-computations.mjs target/release/tsuzuri
node benchmarks/run-control.mjs target/release/tsuzuri
```

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