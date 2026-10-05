# C11: 多次元配列と数値カーネル

| 項目 | 内容 |
| --- | --- |
| ID | C11 |
| 優先度 | P3 |
| 規模 | L |
| 依存 | A16, F02, F04, (C08), (F08) |
| 後続 | F09 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（Phase 1 は言語の意味を変えない std 追加。std 名 `Matrix` は GUIDE D-30 の仮割り当てを完了時に D-07 へ移す。D1）。Phase 2 は着手前に要承認（D9） |
| 改善する劣位 | C/C++ 比: 最適化済みライブラリの不足（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 手本にする既存実装 | opaque 標準 record: `std/Map.tz`・`std/Set.tz`（C06）と `src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES`・`opaque_record`。常に non-Copy にする登録: `src/check.rs` の `Type::is_noncopy_record`（`Seq.Seq`・`Gpu.Device`・`Gpu.Buffer`）。順序を固定した数値 API: `std/Array.tz` の `dot`（`+0` から左へ、積と和を別々に丸める）、前提条件の `assert`: `Array.sub`・`Array.zip`。Rust テスト: `tests/map_set.rs`。E2E: `tests/features.mjs` の `map_set`・`fma_reductions` suite（`orderedReferences`）。計測: `benchmarks/run-simd.mjs` |
| 主な影響ファイル | `std/Matrix.tz`（新規）, `src/stdlib.rs`, `src/check.rs`（`Type::is_noncopy_record` の 1 行）, `tests/matrix.rs`（新規）, `tests/fixtures/matrix/Main.tz`（新規）, `tests/features.mjs`, `benchmarks/matrix/Main.tz`（新規）, `benchmarks/run-matrix.mjs`（新規）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `_docs/library-reference/matrix.md`（新規）, `_docs/library-reference/README.md`, `_docs/feature-status.md`, `_features/README.md`, `_features/GUIDE.md`（D-07 の表と D-30 の行）, `vsc/resources/completions.json`。Phase 2 のみ: `src/llvm_bulk.rs`, `src/check.rs` の `Builtin` |

## 目的

行列計算を一つの連続した行優先バッファで書けるようにし、構築・要素参照・行の借用・転置・要素ごとの変換と集計・加算・行列積を
std の `Matrix` モジュールとして提供する。C++ の Eigen／BLAS、Rust の ndarray、C# の `System.Numerics.Tensors` に相当する入口を作り、
数値計算での採用障壁を下げる。行列積は各出力要素の演算順序を API 契約として固定し（GUIDE D-14）、どの機械・native と WASM・
`-O0` と `-O3` でも同じビットを返す。

実装者は Phase 1 だけを実装する。Phase 1 は std のソース（`std/Matrix.tz`）と登録 3 か所・non-Copy 登録 1 行で完結し、
新しい構文・`Type` の variant・`Builtin`・ランタイム関数を追加しない。Phase 2（ストライド付きビュー、N 次元、C08 の可変ビュー、
最適化カーネル）は設計方針だけを示し、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- 依存の状態を `_features/README.md` の状態欄で確認する: `grep -n "| A16 \|| F02 \|| F04 \|| C08 \|| F08 " _features/README.md`。
  HEAD では F02・F04 が done、A16・C08・F08 が todo。
- Phase 1 は A16（固定長配列）・C08（可変スライス）・F08（広い SIMD）を使わない。この 3 件は Phase 1 の開始条件にしない。
  メタデータの依存欄は変えず、A16 は Phase 2 の小さい固定サイズ行列、C08 は Phase 2 の可変ビュー、F08 は Phase 2 の最適化カーネルの前提として扱う。
- F02（`Parallel`）と F04（`Simd`）も Phase 1 では呼ばない。Phase 1 の `Matrix.mul` は逐次の std ソースである（D5）。
- 承認は不要。std 名 `Matrix` は GUIDE D-30 の仮割り当てで、完了時に D-07 の表へ移す（D1）。
- GUIDE §2.3 の基準コマンドが成功していること。加えて次の 3 テストが成功することを手順 1 で記録する:
  `cargo test --locked --lib stdlib::tests`、`cargo test --locked --test map_set`、
  `cargo test --locked --test polymorphism honors_the_exact_specialization_limit`。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- Phase 1 の API を std ソースだけで書けず、`Builtin` の追加、`src/llvm.rs`・`src/llvm_bulk.rs`・`src/ownership.rs` の変更、
  新しい構文が必要になった。Phase 1 のコンパイラ変更は `src/stdlib.rs` の 3 か所と `Type::is_noncopy_record` の 1 行だけである。
- `Matrix.mul`・`Matrix.add` の結果が JavaScript 参照と 1 ビットでも違う（NaN の payload を除く）。参照側を実装に合わせて直さない。
- 既存テストの期待値を変える必要がある。例外は `src/stdlib.rs` の `reserves_the_d07_table` の件数 20 → 21 だけ。
- リポジトリの fixture・example・benchmark に `Matrix` という名前の利用者モジュールがある（予約で E1011 になる）。
- `honors_the_exact_specialization_limit` か stack-depth の 3 テスト（GUIDE §3.1）が失敗する。上限や stack サイズを上げたくなった。
- `mul` に i-k-j 順、`Simd`、`Parallel`、`Math.fma`、ブロッキングを使いたくなった。これは Phase 2 の範囲である（D5）。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- 多次元は `[[T]]` だけで、各行が独立したバッファで長さも可変。連続した行列型・ストライド付きビュー・行列積はない。
- `src/stdlib.rs`: `SOURCES` に std 18 ファイル、`RESERVED_MODULES` に 20 名（`Matrix` はない）。
  単体テスト `reserves_the_d07_table` が `RESERVED_MODULES.len() == 20` を検査し、`embedded_sources_use_reserved_flat_modules` が
  `SOURCES` の全モジュールの予約を検査する。
- `src/stdlib.rs` の `opaque_record` は `"Map.Map" | "Map.Entry" | "Set.Set" | "Seq.Seq" | "Gpu.Device" | "Gpu.Buffer" | "IO.IO"` を返す。
  `src/check.rs` の `Record::opaque` がこれを読み、`record_storage` が定義モジュール外の構築・field・pattern・update を E1022
  `the representation of '<name>' is opaque; use its module API` で拒否する。公開 record の field 型検査（`validate_public_type`）も
  opaque な std record では省かれる。
- `src/check.rs` の `Type::is_noncopy_record` は `Seq.Seq`・`Gpu.Device`・`Gpu.Buffer` を常に non-Copy にする。
  `Type::is_copy` と `src/ownership.rs` の `is_copy`・`require_copy` の 3 か所がこれを使う。登録のない generic record は、
  全 field が Copy なら Copy になる（`Type::Record` の arm）。`{ rows: i64, cols: i64, data: ['a] }` は `'a` が Copy なら Copy になってしまう。
- `Type::exportable` は整数・`f32`/`f64`・`bool` だけを受け、record を export 境界に出すと E1008 になる（`tests/map_set.rs` が `Map` で検査）。
- `std/Array.tz` の `dot` は長さを `assert` し、`0i64 as 'a`（`+0`）から `total = total + (left[index] * right[index])` を左から右へ行う。
  `Array.map`・`Array.fold` は `new ['b](values.length, index -> ...)` と左から右の `for` で書かれている。
- `src/trap.rs` の `TrapKind` と表示: `Assert`（`assertion failed`）、`BoundsCheck`（`index out of bounds`）、
  `AllocationSize`（`allocation size overflow`）、`AllocationFailure`（`allocation failed`）。実行時の表示は
  `trap: <description> at <path>:<line>:<column>`（`TrapSite::message`）。std 関数内の `assert` の trap は利用者の呼び出し位置で表示される
  （`Map.at` に無いキーを渡す 2 行目の呼び出しで `trap: assertion failed at <利用者のファイル>:2:7`。2026-09-30 に `run` で確認）。
- 整数の算術は幅ごとに折り返す（docs/language.md）。`tests/features.mjs` の native 経路は host C を `-O3 -ffp-contract=off` で
  コンパイルし、各ケースで `assert(tz_<name>(...) == <期待値>); assert(live == 0);` を実行する。trap ケースは非 0 終了だけを検査する。
- GUIDE D-14: 浮動小数点の集計は既定で左から右。別の順序は名前で区別した別 API。整数の折り返し演算だけは順序変更可。

### 再現（検証済み）

次の確認を `target/release/tsuzuri` で行った（scratch は `/tmp/tz-work-C11/`）。

- 利用者 record `Mat<'a> { rows: i64, cols: i64, data: ['a] }` と、仕様の Phase 1 API（`of_array`・`init`・`at`・`get`・`row`・
  `as_array`・`to_array`・`map`・`fold`・`transpose`・`add`・`mul`）を利用者モジュールに書いた project が `run` で期待値 `327` を返した。
  std ソースだけで Phase 1 を書けること、借用 record の配列 field から `ref ['a]` の部分参照を返せることを確認した。
- 同じ project で、i-j-k 順の `mul` と、`Array.set` で出力を更新する i-k-j 順の変種が `f64` の 7×7 で `Array.equal` になった。
- `build --emit llvm -O3` の出力に `contract`・`fast`・`fmuladd` は現れない（暗黙の縮約なし）。
- 利用者コードから `Map` の field を読むと E1022、non-Copy の `Seq` を二度 move すると E1012 になる。

std の部分参照と要素参照の書き方（検証済み。上の project の抜粋で、利用者 record `Mat` 上で動いた）:

```tsuzuri
def at :: ref Mat<'a> -> i64 -> i64 -> ref 'a
fn at matrix row col =
    assert (row >= 0 && row < matrix.rows && col >= 0 && col < matrix.cols)
    ref matrix.data[row * matrix.cols + col]

def row :: ref Mat<'a> -> i64 -> ref ['a]
fn row matrix index =
    assert (index >= 0 && index < matrix.rows)
    let start = index * matrix.cols
    ref matrix.data[start..start + matrix.cols]
```

## 仕様

この節の `Matrix` の API と例はすべて新 API（実装後に有効。未検証）である。ただし同じ本体を利用者モジュール `Mat` として
`run` で確認済み（「再現」）。

### 前提とする他チケットのインターフェース

- Phase 1: なし。既存の `new ['a](count, initializer)`（添字 0 から順に一度ずつ初期化）、`Array.map`・`Array.fold`、組み込みクラス
  `Copy<'a>`・`Numeric<'a>`、`0i64 as 'a`、組み込みの `assert` だけを使う。
- Phase 2（着手前に要承認。D9）: C08 の可変スライスと `Array.split_at_mut`、A16 の固定長配列、F08 の広い SIMD、F09 の GPU 浮動小数点カーネル。
  完了したチケットの「仕様」の形を前提にし、Phase 2 の設計時に改めて書く。

### 構文

新しい構文はない。型は `Matrix<'a>` と書く（正規名 `Matrix`。`tests/map_set.rs` の `Map<i64, string>` と同じ書き方）。
関数は `Matrix.<name>` で呼ぶ。

### API

| API | 型 | 意味 | trap の条件（すべて `Assert`） |
| --- | --- | --- | --- |
| `of_array` | `i64 -> i64 -> ['a] -> Matrix<'a>` | 行優先の平らな配列を複製せずに所有する | 次元が負、`rows * cols` の溢れ、`values.length != rows * cols` |
| `init` | `i64 -> i64 -> (i64 -> i64 -> 'a) -> Matrix<'a>` | `initializer row col` を行優先の順に要素ごとに一度呼ぶ | 次元が負、`rows * cols` の溢れ |
| `rows` | `ref Matrix<'a> -> i64` | 行数 | なし |
| `cols` | `ref Matrix<'a> -> i64` | 列数 | なし |
| `at` | `ref Matrix<'a> -> i64 -> i64 -> ref 'a` | 要素の共有借用（`Array.at` に対応） | `row` か `col` が範囲外 |
| `get` | `Copy<'a> => ref Matrix<'a> -> i64 -> i64 -> Option<'a>` | 範囲外は `Option.None`（`Array.get` に対応） | なし |
| `row` | `ref Matrix<'a> -> i64 -> ref ['a]` | 行の部分参照（長さ `cols`、複製なし） | 行番号が範囲外 |
| `as_array` | `ref Matrix<'a> -> ref ['a]` | 全要素の行優先の借用（長さ `rows * cols`） | なし |
| `to_array` | `Matrix<'a> -> ['a]` | 行列を消費し、バッファを複製せずに返す | なし |
| `map` | `Copy<'a> => ('a -> 'b) -> ref Matrix<'a> -> Matrix<'b>` | 同じ形の新しい行列。`transform` を行優先の順に一度ずつ呼ぶ（std の高階関数と同じく関数が先。D-34） | なし |
| `fold` | `Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref Matrix<'a> -> 'state` | 行優先で左から右に集計する | なし |
| `transpose` | `Copy<'a> => ref Matrix<'a> -> Matrix<'a>` | `cols` 行 `rows` 列の新しい行列。出力 `(j, i)` = 入力 `(i, j)` | なし |
| `add` | `Numeric<'a> => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>` | 要素ごとの `left + right` | `rows` か `cols` が異なる |
| `mul` | `Numeric<'a> => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>` | 行列積（順序は下記） | `left.cols != right.rows`、`left.rows * right.cols` の溢れ |

- 空の行列（`rows == 0` または `cols == 0`）を許す。要素数 0 なら長さ 0 の配列を確保し、callback を呼ばない。
- `mul` の内側の次元（`left.cols`）が 0 のとき、出力の全要素は `+0`（`0i64 as 'a`）。
- 要素を値で読む API は `Copy<'a>`、算術は `Numeric<'a>` を要求する。`Array` の同名 API と同じ制約である。
- Phase 1 の API はこの 14 個だけ（D6）。

### 型規則

- `Matrix<'a>` は opaque（`opaque_record` に `"Matrix.Matrix"`）。`Matrix` モジュールの外での構築・field 参照・pattern・record 更新は E1022。
- 要素型に制約はない（`Matrix<string>` も作れる）。API の制約を満たさない要素型は E1005。
- `Matrix<'a>` は `'a` が Copy でも常に non-Copy（D3）。複製は明示的に書く:
  `Matrix.of_array (Matrix.rows (ref m)) (Matrix.cols (ref m)) (deref (Matrix.as_array (ref m)))`（`Copy<'a>` が要る）。
- record は `Type::exportable` を満たさないので、export 関数の引数・戻り値に使うと E1008。

### 評価順序・所有権・借用

- 引数は通常の呼び出しと同じく左から右に評価する。各 API は前提条件の `assert` をすべて通してから、確保・要素の読み出し・callback の呼び出しをする。
- `init`・`map` の callback と `fold` の folder は行優先（行の昇順、その中で列の昇順）に要素ごとにちょうど一度呼ぶ。
- `of_array`・`to_array` は所有権を移すだけで要素を複製しない。`to_array` の後の元の行列の使用は E1012。
- `at`・`row`・`as_array` の戻り値は引数の行列を共有借用する。借用が生きている間の move・置換は E1014（`Map.at` と同じ）。
- `map`・`transpose`・`add`・`mul` は入力を借用し、出力のバッファを一つだけ確保する。同じ行列を両引数に渡してよい（`Matrix.mul (ref a) (ref a)`）。

### 数値・トラップ・native と WASM の差

- `mul` の出力 `(i, j)` は、`total = +0` から `k = 0, 1, …, left.cols - 1` の順に `total = total + (left(i, k) * right(k, j))` を行った値。
  積と和を別々に丸め、FMA に縮約しない（`Array.dot` と同じ。D5）。
- 整数は幅ごとに折り返す。`add` は要素ごとの `+` 一回。
- NaN・無限大・符号付きゼロ・非正規化数は IEEE 754 のとおり伝播する。`+0` から始めるので、内側の次元が 0 の出力と、項がすべて `-0` の和は `+0` になる。
- native と WASM、`-O0` と `-O3` で結果のビットは同じ（NaN の payload を除く）。std のソースだけなので target による差はない。
- `at` は `data` の添字検査（`BoundsCheck`）に任せず、行と列を別々に assert する。列が範囲外でも平らな添字は範囲内になりうるため（D4）。

| 条件 | API | `TrapKind` | 表示 |
| --- | --- | --- | --- |
| 次元が負、または積が `9223372036854775807` を超える | `of_array`・`init`・`mul` | `Assert` | `trap: assertion failed at <呼び出し位置>` |
| `values.length != rows * cols` | `of_array` | `Assert` | 同上 |
| 添字が範囲外 | `at`・`row` | `Assert` | 同上 |
| 形の不一致 | `add`・`mul` | `Assert` | 同上 |
| 要素の byte 数の溢れ | 確保する API | `AllocationSize`（`allocate_array` の既存の検査） | `trap: allocation size overflow at ...` |
| 確保の失敗 | 確保する API | `AllocationFailure` | `trap: allocation failed at ...` |

### 診断

新しい診断コードはない。すべて既存の経路で出る。E1022・E1014・E1005 の文言と位置は `Map` と `Array.map` で確認した（2026-09-30）。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E1022 | 利用者コードでの `Matrix` の構築・field 参照（`m.data`）・pattern・`{ m with ... }` | `the representation of 'Matrix' is opaque; use its module API` | field 名、構築式 |
| E1012 | move 済みの行列を使う（`to_array` や別の束縛への move の後） | `use of moved or partially moved value '<name>'` | 使用箇所 |
| E1014 | `at`・`row`・`as_array` の借用が生きている間に行列を move・置換する | `access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively` | move する式 |
| E1005 | `Copy<'a>`・`Numeric<'a>` を満たさない要素型 | `no instance for Copy<string>; define an instance or use a supported type`（クラスと型は実際のもの） | 呼び出す関数名 |
| E1008 | export 関数の引数・戻り値に `Matrix<'a>` | 既存の export 型の文言（変えない） | 型の位置 |
| E1011 | 利用者のファイル名が `Matrix.tz`・`Matrix.tt`・`Matrix.tc` | `module name 'Matrix' is reserved for the standard library; rename the file` | ファイル |

### 資源上限

- 新しい上限はない。要素数は `rows * cols <= 9223372036854775807` と確保の上限で決まる。
- std の generic 関数は使われた要素型ごとに単相化され、未使用の関数は IR に出ない（D-07）。特殊化の上限 1,024 の数え方は変えない
  （`honors_the_exact_specialization_limit`）。
- `mul` は再帰しないループで `rows × inner × cols` 回の積和をする。stack の深さは大きさに依存しない。

### 例

受理（新 API（実装後に有効。未検証））。「再現」で `Mat` として `327` を返した project の名前を置き換えたもの。期待値 `327`
（`p = [[14, 32], [32, 77]]`、`109 + 32 + 154 + 32`）。

```tsuzuri
let a = Matrix.of_array 2 3 [1, 2, 3, 4, 5, 6]
let t = Matrix.transpose (ref a)
let p = Matrix.mul (ref a) (ref t)
let r = Matrix.row (ref p) 1
let s = Matrix.add (ref p) (ref p)
let flat = Matrix.to_array s
Array.sum r + deref (Matrix.at (ref p) 0 1) + flat[3] + Option.get (Matrix.get (ref p) 1 0)
```

拒否・trap（新 API（実装後に有効。未検証））。前置き `let m = Matrix.init 2 3 (\i j -> i + j)` の後に続ける。

| 続けるソース | 結果 |
| --- | --- |
| `m.rows` | E1022 |
| `let n = m` の後に `Matrix.rows (ref m)` | E1012 |
| `let r = Matrix.row (ref m) 0` の後に `let n = m` と `r[0]` | E1014 |
| `let w = Matrix.init 1 1 (\i j -> "x")` の後に `Matrix.map (\s -> 1) (ref w)` | E1005 |
| `Matrix.of_array 2 2 [1, 2, 3]` | trap（`Assert`） |
| `Matrix.mul (ref m) (ref m)` | trap（`Assert`。3 != 2） |
| `deref (Matrix.at (ref m) 0 3)` | trap（`Assert`。平らな添字 3 は範囲内だが列が範囲外） |
| `Matrix.init 4294967296 4294967296 (\i j -> 0)` | trap（`Assert`。確保より前） |

### Phase 2（設計方針）

着手前に要承認（D9）。ここは方針だけで、実装者は着手しない。

- ストライド付きの借用ビュー `MatrixView<'a>`（新規。offset・行の stride を持つ）と、その上の `row`・`col`・部分行列。
- N 次元の `Tensor<'a>`（新規。形と stride の配列）。`Matrix` はその 2 次元の特別な場合として残す。
- C08 の可変スライスによる行単位の書き込み、`Parallel` の行チャンク版。
- 最適化カーネルは `src/check.rs` の `Builtin` と `src/llvm_bulk.rs` に置く。`mul` と同じ名前で置き換える版は、各出力要素の `k` の順序を保つ形
  （列方向のベクトル化、行の並列化）に限り、Phase 1 の `mul` とビット一致させる。ブロッキング・FMA・pairwise など順序の違う版は
  `mul_fma`（新規）などの別名にする（GUIDE D-14）。
- F09 の GPU カーネルとの接続。

## 設計

### データ構造

std の record（新 API（実装後に有効。未検証））:

```tsuzuri
record Matrix<'a> { rows: i64, cols: i64, data: ['a] }
```

不変条件: `rows >= 0`、`cols >= 0`、`rows * cols <= 9223372036854775807`、`data.length == rows * cols`、要素 `(i, j)` は
`data[i * cols + j]`。構築は `of_array`・`init`・`map`・`transpose`・`add`・`mul` だけで、どれも不変条件を検査または保存する。
この不変条件があるので、`at`・`row`・`mul` の添字計算 `i * cols + j` は溢れない。コンパイラ側のデータ構造の変更はない。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| std | `std/Matrix.tz`（新規） | `Matrix`、公開 14 関数、`private` の `checked_count`・`element` | 「アルゴリズム」のソース。字下げは 4 空白（`std/Array.tz` と同じ） |
| 登録 | `src/stdlib.rs` | `SOURCES` | 末尾に `("std/Matrix.tz", include_str!("../std/Matrix.tz"))`。既存の 18 行の順序は変えない（D2） |
| 登録 | `src/stdlib.rs` | `RESERVED_MODULES` | 末尾に `"Matrix"` |
| 登録 | `src/stdlib.rs` | `opaque_record` | `"Matrix.Matrix"` を足す |
| 単体テスト | `src/stdlib.rs` | `reserves_the_d07_table` | `20` → `21`。`assert!(is_reserved_module("Matrix"))` を足す |
| 単体テスト | `src/stdlib.rs` | `embedded_sources_use_reserved_flat_modules`, `std_definitions_never_shadow_builtin_functions` | 変更なし（新しい source も自動で検査される） |
| 検査 | `src/check.rs` | `Type::is_noncopy_record` | `"Matrix.Matrix"` を足す（D3） |
| 検査 | `src/check.rs` | `Record::opaque`, `record_storage`, `validate_public_type`, `Type::is_copy`, `Type::exportable` | 変更なし（上の 2 関数を読む、または record を拒否する） |
| 所有権 | `src/ownership.rs` | `is_copy`, `require_copy` | 変更なし（`Type::is_noncopy_record` を読む） |
| 生成 | `src/llvm.rs`, `src/llvm_bulk.rs`, `src/runtime/` | なし | 変更なし（record・配列・`new` の既存の lowering） |
| 文書生成 | `src/docgen.rs` | なし | 変更なし（`tsuzuri doc std` が `Matrix.md` を出す） |
| エディター | `vsc/resources/completions.json` | std API の項目 | 公開 14 関数を既存の std の項目（例: `"module": "Utf8String"`）と同じ形で足す |

### 生成 IR とランタイム

- 新しいランタイム関数・`%tz.*` 型・intrinsic・WASM import はない。行列は既存の record の lowering で下がり、`data` は `%tz.array`
  （`{ ptr, i64 }`）になる。確保は `allocate_array` の既存の検査（`AllocationSize`）を通る。
- 関数は `@tz.fn.Matrix.` で始まる名前で、使われた要素型ごとに単相化される（`map_set` suite の `inspect` が `@tz.fn.Map.` を探すのと同じ）。
- `element` の IR は積と和が別の命令（浮動小数点は `fmul` と `fadd`、整数は `mul` と `add`）で、`contract`・`fast`・`reassoc` の flag と
  `llvm.fmuladd`・`llvm.fma` の呼び出しがない。flag のない浮動小数点の加算を LLVM は並べ替えないので、`-O3` でも `k` 方向の再結合は起きない。
- 列方向のベクトル化などの最適化が起きたかは、計測時に IR で確かめた事実だけを `docs/benchmarks.md` に書く（D8）。

### アルゴリズム

`std/Matrix.tz` の全体（新 API（実装後に有効。未検証）。「再現」の `Mat` の本体から名前と `cols` の追加だけを変えたもの）:

```tsuzuri
record Matrix<'a> { rows: i64, cols: i64, data: ['a] }

private def checked_count :: i64 -> i64 -> i64
fn checked_count rows cols =
    assert (rows >= 0 && cols >= 0 && (cols == 0 || rows <= 9223372036854775807 / cols))
    rows * cols

/// Takes a row-major array without copying; traps unless its length is rows * cols.
def of_array :: i64 -> i64 -> ['a] -> Matrix<'a>
fn of_array rows cols values =
    assert (values.length == checked_count rows cols)
    Matrix { rows: rows, cols: cols, data: values }

def init :: i64 -> i64 -> (i64 -> i64 -> 'a) -> Matrix<'a>
fn init rows cols initializer =
    let count = checked_count rows cols
    Matrix { rows: rows, cols: cols, data: new ['a](count, index -> initializer (index / cols) (index % cols)) }

def rows :: ref Matrix<'a> -> i64
fn rows matrix = matrix.rows

def cols :: ref Matrix<'a> -> i64
fn cols matrix = matrix.cols

def at :: ref Matrix<'a> -> i64 -> i64 -> ref 'a
fn at matrix row col =
    assert (row >= 0 && row < matrix.rows && col >= 0 && col < matrix.cols)
    ref matrix.data[row * matrix.cols + col]

def get :: Copy<'a> => ref Matrix<'a> -> i64 -> i64 -> Option<'a>
fn get matrix row col =
    if row < 0 || row >= matrix.rows || col < 0 || col >= matrix.cols then Option.None
    else Option.Some matrix.data[row * matrix.cols + col]

def row :: ref Matrix<'a> -> i64 -> ref ['a]
fn row matrix index =
    assert (index >= 0 && index < matrix.rows)
    let start = index * matrix.cols
    ref matrix.data[start..start + matrix.cols]

def as_array :: ref Matrix<'a> -> ref ['a]
fn as_array matrix = ref matrix.data

def to_array :: Matrix<'a> -> ['a]
fn to_array matrix = matrix.data

def map :: Copy<'a> => ('a -> 'b) -> ref Matrix<'a> -> Matrix<'b>
fn map transform matrix = Matrix { rows: matrix.rows, cols: matrix.cols, data: Array.map transform (ref matrix.data) }

def fold :: Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref Matrix<'a> -> 'state
fn fold folder initial matrix = Array.fold folder initial (ref matrix.data)

def transpose :: Copy<'a> => ref Matrix<'a> -> Matrix<'a>
fn transpose matrix =
    let out_rows = matrix.cols
    let out_cols = matrix.rows
    Matrix { rows: out_rows, cols: out_cols, data: new ['a](matrix.data.length, index -> matrix.data[(index % out_cols) * out_rows + index / out_cols]) }

def add :: Numeric<'a> => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>
fn add left right =
    assert (left.rows == right.rows && left.cols == right.cols)
    Matrix { rows: left.rows, cols: left.cols, data: new ['a](left.data.length, index -> left.data[index] + right.data[index]) }

private def element :: Numeric<'a> => ref Matrix<'a> -> ref Matrix<'a> -> i64 -> i64 -> 'a
fn element left right row col =
    let mut total = 0i64 as 'a
    let mut k = 0
    while k < left.cols do
        total = total + (left.data[row * left.cols + k] * right.data[k * right.cols + col])
        k = k + 1
    total

/// Row-major product. Each element sums k = 0, 1, ... left to right from +0,
/// rounding every multiply and add separately (no FMA contraction).
def mul :: Numeric<'a> => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>
fn mul left right =
    assert (left.cols == right.rows)
    let count = checked_count left.rows right.cols
    let cols = right.cols
    Matrix { rows: left.rows, cols: cols, data: new ['a](count, index -> element left right (index / cols) (index % cols)) }
```

- `transpose`: 出力の添字 `index` の行は `index / out_cols`、列は `index % out_cols`。入力の `(列, 行)` を読む。要素数 0 なら除算は実行されない。
- `init`・`mul` の `index / cols` も、`cols == 0` なら要素数 0 で実行されない。
- `checked_count` は乗算の前に除算で溢れを判定する（`String.tz` の `Int.checked_mul` は `Option` を経由するので使わない）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドと着手条件の 3 テストを実行し、既存 fixture の IR を保存する（手順 3 で byte 単位の不変を確かめる）。
- 確認: すべて成功する。`map_set` は `3 passed`、特殊化上限は `1 passed`、`stdlib::tests` の件数を記録する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
cargo test --locked --lib stdlib::tests
cargo test --locked --test map_set
cargo test --locked --test polymorphism honors_the_exact_specialization_limit
mkdir -p /tmp/tz-c11
for name in map_set array_bulk fma_reductions; do target/release/tsuzuri build tests/fixtures/$name --emit llvm -O3 -o /tmp/tz-c11/before-$name.ll; done
```

### 手順 2: ソースを利用者モジュールとして確かめる

- 変更: なし（scratch だけ）。
- 内容: 「アルゴリズム」のソースを `/tmp/tz-c11/user/Matrix.tz`、「例」の受理例を `/tmp/tz-c11/user/Main.tz` に置く。HEAD では `Matrix` は
  予約されていないので利用者モジュールとして動く。
- 確認: `target/release/tsuzuri run /tmp/tz-c11/user` が `327` を出す。`target/release/tsuzuri fmt --check /tmp/tz-c11/user` が成功する
  （失敗したら `fmt` で整形し、整形後のソースを手順 3 で使う）。

### 手順 3: std へ登録する

- 変更: `std/Matrix.tz`（新規）、`src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES`・`opaque_record`・`reserves_the_d07_table`。
- 内容: 手順 2 のソースを置き、「段ごとの変更」の登録 4 行を入れる。`SOURCES` と `RESERVED_MODULES` は末尾に足す（D2）。
- 確認: `cargo test --locked --lib stdlib::tests` が手順 1 と同じ件数で成功する。`cargo build --release --locked` の後、
  `target/release/tsuzuri check /tmp/tz-c11/user` が E1011 になる。`Matrix.tz` を消した `/tmp/tz-c11/std/Main.tz`（受理例だけ）の `run` が `327`。
  手順 1 の 3 fixture の IR を同じコマンドで `/tmp/tz-c11/after-$name.ll` に出し、`cmp` が 3 つとも一致する。

### 手順 4: non-Copy の登録

- 変更: `src/check.rs` の `Type::is_noncopy_record`。
- 内容: `"Matrix.Matrix"` を足す（D3）。
- 確認: `/tmp/tz-c11/copy/Main.tz` に `let m = Matrix.init 1 1 (\i j -> 1)`・`let n = m`・`Matrix.rows (ref m)` の 3 行を置き、
  `target/release/tsuzuri check /tmp/tz-c11/copy` が手順 3 の build では成功し、この手順の build 後は `error[E1012]` になる。

### 手順 5: Rust テスト

- 変更: `tests/matrix.rs`（新規）。
- 内容: 「テスト計画」の 4 テスト。`tests/map_set.rs` と同じく `use tsuzuri::{analyze, llvm};` で、IR は `llvm::emit_target(&module, llvm::Entry::Library, wasm)`。
- 確認: `cargo test --locked --test matrix` が `4 passed`。`cargo test --locked` が成功する。

### 手順 6: E2E suite `matrix`

- 変更: `tests/fixtures/matrix/Main.tz`（新規）、`tests/features.mjs`（`suites.matrix` と参照関数 `matrixReferences`（新規））。
- 内容: 「E2E」の export・ケース・trap を書く。期待値は `matrixReferences` だけから作る。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri matrix` が成功する。Node 20 が V8 の
  `RepresentationChangerError` で落ちたら `npx --yes --package=node@24 node tests/features.mjs target/release/tsuzuri matrix`。

### 手順 7: 生成コードの確認

- 変更: なし。
- 内容: `-O3` の IR で積和が分かれたままかを見る。`inspect` は `-O0` の IR しか見ないので、この確認は手で行う。
- 確認: 次の出力が `0`。`@tz.fn.Matrix.element` の定義が f32・f64・i64 の 3 つあることも目で確かめる。

```sh
target/release/tsuzuri build tests/fixtures/matrix --emit llvm -O3 -o /tmp/tz-c11/matrix-O3.ll
grep -cE 'fmuladd|llvm\.fma\.|(fmul|fadd) [a-z ]*(fast|contract|reassoc)' /tmp/tz-c11/matrix-O3.ll
target/release/tsuzuri build tests/fixtures/matrix --target wasm32 --emit llvm -O3 -o /tmp/tz-c11/matrix-wasm-O3.ll
grep -cE 'fmuladd|llvm\.fma\.|(fmul|fadd) [a-z ]*(fast|contract|reassoc)' /tmp/tz-c11/matrix-wasm-O3.ll
```

### 手順 8: 性能の記録

- 変更: `benchmarks/matrix/Main.tz`（新規）、`benchmarks/run-matrix.mjs`（新規）、`docs/benchmarks.md`。
- 内容: `benchmarks/run-simd.mjs` の形を写す。`build benchmarks/matrix/Main.tz --emit object -O3` の object と、同じ i-j-k 順の素朴な C を
  `-O3 -std=c11 -ffp-contract=off -fno-lto` で一つの実行ファイルにする。export `matmul_checksum :: i64 -> i64 -> f64`（`n`、繰り返し回数）は
  `n × n` の f64 行列 2 つを `init` で作り、`mul` の結果を `fold` で左から右に足す。C 側も同じ入力と順序で計算し、checksum の一致を
  時間の表示より前に検査する。`n` は 64 と 256、`--quick` は 16 で、9 回の中央値・最小・最大を表示する。
- 確認: `node benchmarks/run-matrix.mjs target/release/tsuzuri --quick` が exit 0。本計測の結果は環境（CPU、OS、clang・node の版、commit）と
  ともに `docs/benchmarks.md` に書き、合否の閾値は置かない（D8）。

### 手順 9: ドキュメント

- 変更: 「ドキュメント」の各ファイル。
- 内容: `target/release/tsuzuri doc std -o _docs/library-reference/api` で `api/Matrix.md` を生成する。
- 確認: `node scripts/check-docs.mjs _docs/library-reference/matrix.md _docs/library-reference/README.md _docs/feature-status.md` が成功する。

### 手順 10: 台帳と最終確認

- 変更: `_features/GUIDE.md`（D-07 の表に `Matrix` の行を足し、D-30 の `std | Matrix | C11` 行を消す）、`_features/README.md`。
- 内容: GUIDE §10 の完了の定義を満たし、チケットを `_features/_completed/` へ移す。
- 確認: 次がすべて成功する。

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo build --release --locked && node tests/features.mjs target/release/tsuzuri
node benchmarks/run-matrix.mjs target/release/tsuzuri --quick
```

## テスト計画

### Rust テスト

`tests/matrix.rs`（新規）。ソースは「例」の前置き `let m = Matrix.init 2 3 (\i j -> i + j)` を使う。

| テスト | 検査すること |
| --- | --- |
| `matrix_is_an_opaque_noncopy_std_record` | 「例」の受理例が `analyze` を通り、native と WASM の IR を 2 回出して一致する。`m.rows`、`Matrix { rows: 1, cols: 1, data: [1] }`、`{ m with rows = 2 }`、`match m with \| Matrix { data = d } -> d.length` が E1022。`i64` と `f64` の行列の二重 move が E1012。`export def bad :: Matrix<i64>` と `fn bad = Matrix.init 1 1 (\i j -> 0)` が E1008 |
| `matrix_borrows_follow_array_rules` | `row` の借用を使いながら `add (ref p) (ref p)` を呼べる。`to_array` の結果を `of_array` に渡せる。`row` の借用中の `let n = m` が E1014。`to_array m` の後の `Matrix.rows (ref m)` が E1012 |
| `matrix_constraints_match_array_apis` | `Matrix<string>` の `init`・`at`・`row`・`to_array` は受理。同じ行列の `map`・`fold`・`transpose`・`get` と、`add`・`mul` が E1005。`Matrix<bool>` の `mul` が E1005 |
| `matrix_ir_keeps_multiply_and_add_separate` | f32 と f64 の `mul` を使う source の IR（native と WASM）で、`@tz.fn.Matrix.element` の本体に `fmul` と `fadd` があり、`fmuladd`・`llvm.fma`・`fast`・`contract`・`reassoc` がない。使っていない `@tz.fn.Matrix.transpose` の定義がない |

表の `\|` は Markdown の表のための escape で、Rust の文字列には `|` を書く。

### E2E

`tests/fixtures/matrix/Main.tz`（新規）と `tests/features.mjs` の suite `matrix`。ハーネスが native と WASM × `-O0`・`-O3`、各ケース後の
`live == 0`、WASM import なし、IR の決定性と宣言の重複なしを検査する。期待値はすべて `matrixReferences`（新規。`orderedReferences` の隣）が
JavaScript だけで計算し、コンパイラの出力から作らない。

入力: `pool = [1e16, 1, -1e16, 3, -3, 0.5, -0.25, 1e-38, tiny, -0]`。f64 は `tiny = 5e-324`、f32 は `tiny = 1e-45` で、各値を `Math.fround`
（Tsuzuri 側は `as f32`）で丸める。`a(i, k) = pool[(i * 37 + k * 11 + seed) % 10]`、`b(k, j) = pool[(k * 13 + j * 29 + seed + 3) % 10]`、
形は `A: n × (n + 1)`、`B: (n + 1) × (n + 2)`（非正方で転置の誤りを検出する）。Tsuzuri 側の浮動小数点の書き方は
`tests/fixtures/fma_reductions/Main.tz` の入力表に合わせる。

| export | 型 | 内容 | ケース |
| --- | --- | --- | --- |
| `int_mul` | `i64 -> i64 -> i64` | `a = (i * 31 + k * 17 + seed) * 6364136223846793005`（折り返す）、`b` は `seed + 1` で同じ式。積の `fold` を `total * 31 + x` で返す | `n` ∈ {0, 1, 2, 7, 16} × `seed` ∈ {0, 5}。参照は `BigInt.asIntN(64, …)` |
| `f64_entry` | `i64 -> i64 -> i64 -> f64` | `mul` の出力の平らな添字 `index` の要素 | `n` ∈ {1, 2, 3} は全要素、{7, 16} は先頭・中央・末尾。`seed` ∈ {0, 1, 7} |
| `f32_entry` | `i64 -> i64 -> i64 -> f64` | 同じ計算を f32 で行い `as f64` で返す | 同上 |
| `zero_signs` | `i64`（引数なし） | 符号付きゼロ 4 件をビットにして返す: 内側 0 の `mul`、`[[-0]] × [[1]]`、`add` の `-0 + -0`、`transpose` した `-0`。負のゼロなら 1（`1.0 / x < 0.0`） | 1 ケース。参照は `Object.is(x, -0)` |
| `nan_count` | `i64`（引数なし） | NaN を含む `mul`、`inf × 0` を含む `mul`、`add` の `inf + -inf` の結果で `Math.is_nan` の数 | 期待値 3 |
| `shapes` | `i64 -> i64` | `transpose`・`row`・`get`（範囲外の `None` を含む）・`map`・`fold`・`as_array`・`to_array`・`of_array` の往復を一つの checksum にする | `n` ∈ {0, 1, 3, 16} |
| `owned_elements` | `i64 -> i64` | `Matrix<[i64]>`（要素ごとに `new [i64](i + j, …)`）の `at`・`row`・`transpose`・`to_array` と長さの合計 | `n` ∈ {0, 1, 5} |

参照関数の核（`orderedReferences` と同じく各演算の後で丸める）:

```javascript
function matrixProduct(n, seed, bits) {
  const round = bits === 32 ? Math.fround : (value) => value;
  const pool = [1e16, 1, -1e16, 3, -3, 0.5, -0.25, 1e-38, bits === 32 ? 1e-45 : 5e-324, -0].map(round);
  const out = [];
  for (let i = 0; i < n; i++) for (let j = 0; j < n + 2; j++) {
    let total = 0;
    for (let k = 0; k < n + 1; k++) total = round(total + round(pool[(i * 37 + k * 11 + seed) % 10] * pool[(k * 13 + j * 29 + seed + 3) % 10]));
    out.push(total);
  }
  return out;
}
```

trap（`traps`。native は非 0 終了、WASM は `RuntimeError`）: `of_array_length`（2×2 に 3 要素）、`add_shape`、`mul_shape`、
`at_col [3]`・`at_col [-1]`（2×3 の行 0）、`row_index [2]`・`row_index [-1]`、`negative_dims`、`count_overflow`（`init 4294967296 4294967296`）、
`mul_count_overflow`（`4294967296 × 0` と `0 × 4294967296` の積）、`alloc_size`（`init 2147483648 2147483648` の f64。`AllocationSize`）。

`inspect(ir)`: `@tz.fn.Matrix.` の定義があり、どの定義にも `fmuladd`・`llvm.fma`・`fast`・`contract`・`reassoc` がない。`%tz.matrix` が現れない。

### 既存テストへの影響

- `src/stdlib.rs` の `reserves_the_d07_table` の件数 20 → 21 だけ。
- 既存 fixture の IR は byte 単位で変わらない（手順 3 の `cmp`）。`SOURCES` の末尾に足すので既存 std の source の番号は変わらない。

### 性能

手順 8 の記録だけ。閾値は置かない。Phase 1 は逐次の std ソースなので、SIMD・並列・BLAS 並みの速度は主張しない。

## ドキュメント

- `docs/language.md`: `### Map / Set` の後に `### Matrix`（新規）。表現と不変条件、API 一覧、`mul` の演算順序、trap、non-Copy と opaque、
  計算量（`mul` は O(rows × inner × cols)、`map`・`transpose`・`add` は O(rows × cols)、`at`・`row`・`as_array` は確保なしの O(1)）。
- `docs/architecture.md`: `std/Gpu.tz` の opaque と non-Copy の段落の後に、`std/Matrix.tz` が通常の std ソースで builtin・ランタイムを持たないこと。
- `docs/benchmarks.md`: 手順 8 の条件・コマンド・結果・IR で確かめた事実。
- `_docs/library-reference/matrix.md`（新規）: `map-set.md` と同じ構成（例、API、演算順序と数値、計算量、所有権と不透明性、API と関連項目）。
- `_docs/library-reference/README.md`: 分野別リファレンスに `| [Matrix](matrix.md) | 行優先の行列と行列積 |`、ソース宣言の API 一覧に
  `| Matrix | [Matrix](api/Matrix.md) | [Matrix](matrix.md) |`。`api/` は手順 9 で生成する。
- `_docs/feature-status.md` の C11 行を完了の記述にし、リンクを `library-reference/matrix.md` と `_completed/` のチケットにする。
- `_features/README.md` の C11 行を `done` と `_completed/` へのリンクにする。`_features/GUIDE.md` は手順 10。
- `vsc/resources/completions.json` に公開 14 関数（「段ごとの変更」）。`README.md` に std モジュールの一覧があれば `Matrix` を足す。

## 受け入れ条件

- [ ] `std/Matrix.tz` の公開 14 関数が「API」の型・意味・trap のとおりに動き、`Matrix<'a>` は opaque で常に non-Copy である。
- [ ] `mul` の結果が f32・f64・i64 で JavaScript の独立した参照とビット単位で一致する（native と WASM × `-O0`・`-O3`）。
- [ ] `-O3` の IR に `fmuladd`・`llvm.fma`・`fast`・`contract`・`reassoc` がない（手順 7）。
- [ ] コンパイラの変更が `src/stdlib.rs` の 3 か所と単体テスト 1 件、`Type::is_noncopy_record` の 1 行だけである。
- [ ] 既存 fixture の IR が変わらず、既存テストの期待値の変更は `reserves_the_d07_table` の件数だけである。
- [ ] `tests/matrix.rs` の 4 テストと suite `matrix` が成功し、`live == 0` と WASM import なしを満たす。
- [ ] 性能の記録に閾値や未計測の主張がない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `SOURCES` の途中に挿入すると、後ろの std source の番号がずれて既存の IR が変わりうる。末尾に足し、手順 3 の `cmp` で確かめる。
- `Type::is_noncopy_record` を忘れると `Matrix<i64>` が Copy になり、`let n = m` が長さに比例する暗黙の複製になる。手順 4 と E1012 のテストで検出する。
- `opaque_record` を忘れると `m.data` が通り、不変条件を利用者が壊せる。E1022 のテストで検出する。
- `at` を `data` の添字検査だけで済ませると、2×3 の `(0, 3)` が `(1, 0)` を返す。trap `at_col` で検出する。
- `rows * cols` を掛けてから比較すると、折り返した値で判定してしまう。`checked_count` の除算で先に判定する。`mul` は入力が有効でも
  出力の次元の積が溢れうる（`mul_count_overflow`）。
- f32 の参照で `Math.fround` を最後に一度だけ呼ぶと二重丸めで値が変わる。積と和の各演算の後で丸める。
- `cValue` は `NaN`・`Infinity` を C のリテラルにできず、C の `==` と `assert.equal` は `-0` と `+0` を区別しない。値で比べるケースの期待値は
  有限にし、NaN は `nan_count`、符号付きゼロは `zero_signs` で見る。
- 各ケースの WASM メモリは 16 MiB 以下という検査がある。値で比べるケースは `n ≤ 16` にする。
- std のソースでは他の std モジュールを常に修飾して書く（`Array.map`、`Option`。GUIDE D-07）。字下げは 4 空白で、タブを混ぜない。
- `vsc/dist/` は生成物なので編集しない。`completions.json` に生成手順があるかは
  `grep -rln "completions.json" scripts vsc --include=*.mjs --include=*.ts` で確かめ、あればそれで更新する。

## 対象外

- 自動微分、スパース行列、外部 BLAS の自動リンク（E12 の extern で利用者が接続する）。
- `[[T]]` からの変換、`reshape`、部分行列、`scale`・`sub`、`Display`。利用者は `Array.fold` と `of_array` で書ける。
- 順序の違う積（`mul_fma` など）、SIMD・並列・GPU のカーネル、ストライド付きビュー、N 次元、可変ビュー（Phase 2。D9）。
- export 境界・ホスト ABI への行列の受け渡し（E05・E13 の範囲）。

## 決定事項

### D1: 配置と名前

- 決定: std のモジュール `Matrix`（`std/Matrix.tz`）と型 `Matrix<'a>`（正規名 `Matrix`）。GUIDE D-30 の仮割り当てを使い、完了時に
  D-07 の表へ移して D-30 の行を消す。外部パッケージ（E10）にはしない。
- 理由: C06 の `Map`・`Set` と同じ配置で、言語の意味を変えずに std ソースだけで書ける。E10 は未実装で、依存にすると Phase 1 を出せない。
- 状態: 既定案（実装者はこの案に従う）

### D2: 表現と登録の位置

- 決定: `record Matrix<'a> { rows: i64, cols: i64, data: ['a] }` の opaque record。行優先・連続で、不変条件は「データ構造」のとおり。
  `SOURCES` と `RESERVED_MODULES` には末尾に足す。
- 理由: 一つの `%tz.array` に載るので新しい IR 型が要らない。末尾に足すと既存 std source の番号と既存の IR が変わらない。
- 状態: 既定案（実装者はこの案に従う）

### D3: 常に non-Copy

- 決定: `Type::is_noncopy_record` に `"Matrix.Matrix"` を足し、要素型によらず non-Copy にする。要素の読み方は配列の規則に合わせ、値で読む API は
  `Copy<'a>`、借用を返す API は `ref` を返す。複製は `deref (Matrix.as_array (ref m))` を使って明示的に書く。
- 理由: 登録しないと `'a` が Copy のとき record 全体が Copy になり、大きなバッファの暗黙の複製が `let` に隠れる（A15 が警告しようとしている種類）。
  `Map` と同じく所有する容器として扱う。
- 状態: 既定案（実装者はこの案に従う）

### D4: 前提条件の検査と trap

- 決定: 次元・添字・形の検査は std の `assert`（`TrapKind::Assert`）で、読み出し・確保・callback の前に行う。`at`・`row` は行と列を別々に検査する。
  `rows * cols` の溢れは乗算の前に除算で判定する。要素の byte 数の溢れと確保の失敗は `new` の既存の検査に任せる。
- 理由: std の既存 API（`Array.sub`・`Array.zip`・`Map.at`）と同じ方式で、新しい `TrapKind` やランタイムが要らない。平らな添字の検査だけでは
  列の範囲外を見逃す。
- 状態: 既定案（実装者はこの案に従う）

### D5: `mul` の演算順序と実装

- 決定: 各出力要素は `+0` から `k` の昇順に `total = total + (a * b)` で、積と和を別々に丸める。出力は行優先の順に計算する。Phase 1 は逐次の
  std ソース（i-j-k 順の `element`）で、`Simd`・`Parallel`・`Math.fma`・i-k-j 順・ブロッキングは使わない。
- 理由: GUIDE D-14 の既定（左から右）と `Array.dot` の契約に一致し、native・WASM・最適化レベルに依存しない。i-k-j 順は「再現」でビット一致を
  確認したが、`Array.set` の更新が要り利点が計測されていない。最適化は Phase 2 で計測とともに行う。
- 状態: 既定案（実装者はこの案に従う）

### D6: Phase 1 の API の範囲

- 決定: 公開 API は「API」の 14 関数だけ。`of_array` と `to_array` は複製しない所有権の移動、`as_array` は全体の借用。
- 理由: 旧仕様の 9 関数に、構築（`of_array`）・寸法（`rows`・`cols`）・範囲外で `None` を返す読み出し（`get`）・複製なしの出入り（`as_array`・
  `to_array`）を足した最小の組で、どれも「再現」で動作を確認した形である。
- 状態: 既定案（実装者はこの案に従う）

### D7: E2E の参照と比較の方法

- 決定: 期待値は `tests/features.mjs` の `matrixReferences`（新規）が JavaScript の number（f32 は演算ごとの `Math.fround`）と BigInt で計算する。
  浮動小数点は要素を f64 で返して値で比べ、NaN は `Math.is_nan` の数、符号付きゼロは符号のビットで比べる。
- 理由: JavaScript の算術は FMA に縮約せず、f32 の積和は倍精度で計算してから丸めても正しく丸まる（`orderedReferences` と同じ根拠）。
  ハーネスの C 生成と `assert.equal` は NaN と `-0` を値で区別できない。
- 状態: 既定案（実装者はこの案に従う）

### D8: 性能の記録

- 決定: `benchmarks/run-matrix.mjs`（新規）で同じ順序の素朴な C と比べ、結果と IR で確かめた事実だけを `docs/benchmarks.md` に書く。閾値は置かない。
- 理由: 順序の違う BLAS との比較は意味が違う（旧「落とし穴」）。AGENTS.md は計測と生成コードの確認なしの加速の主張と共有 CI の速度閾値を禁じる。
- 状態: 既定案（実装者はこの案に従う）

### D9: Phase 2

- 決定: ストライド付きビュー、N 次元 `Tensor<'a>`、C08 による可変ビュー、`Builtin` による最適化カーネル、`mul_fma` などの別順序 API、
  `Parallel`・GPU との接続は、人間の承認の後に別の設計として着手する。
- 理由: 新しい `Builtin`・型・std 名（GUIDE D-30 にない名前）を足し、C08 は D-13 の変更として承認待ちである。
- 状態: 要承認（承認前は Phase 2 に着手しない）
