# C11: 多次元配列と数値カーネル

| 項目 | 内容 |
| --- | --- |
| ID | C11 |
| 優先度 | P3 |
| 規模 | L |
| 依存 | A16, F02, F04, (C08), (F08) |
| 後続 | F09 |
| 状態 | done（Phase 1・2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。旧計画の基準は `f8dc655` |
| 実装の基準 | `96d7cbf`、2026-10-10 |
| 承認 | 全フェーズと必要な判断への包括承認（旧 D1 の std 名 `Matrix`、旧 D9 の Phase 2 を含む）。モジュールの予約名は 46 個から 49 個になった |
| 利用者向け仕様 | [Matrix](../../_tsuzuri/language-reference/built-in-types-and-modules/matrix.md)、[MatrixView](../../_tsuzuri/language-reference/built-in-types-and-modules/matrix-view.md)、[Tensor](../../_tsuzuri/language-reference/built-in-types-and-modules/tensor.md)、[言語仕様](../../docs/language.md#matrix)、[性能測定](../../docs/benchmarks.md#行列積c11) |

## 目的と実装範囲

行列計算を連続した行優先バッファで書けるようにし、行列積の各出力要素の演算順序を API の約束として固定した（GUIDE D-14）。
同じ入力は、どの機械・native と WASM・`-O0` と `-O3` でも同じビットになる。順序の違う積は別名にした。
実装はすべて std のソース（`std/Matrix.tz`・`std/MatrixView.tz`・`std/Tensor.tz`）で、新しい構文・`Type` の variant・`Builtin`・ランタイム関数・WASM の import はない。

| フェーズ | 実装 |
| --- | --- |
| Phase 1 | `Matrix<'a>`（14 関数）。opt-in std モジュール、常に非 Copy、不透明。`mul` の演算順序の固定。テスト、ベンチマーク、文書 |
| Phase 2 | `Matrix.set`、行ごとの更新（i-k-j）に書き換えた `mul`（ビットは Phase 1 と同一）、`mul_fma`、`mul_parallel`、`mul_fma_parallel`。`MatrixView`（ストライド付きの借用の窓と、排他スライスで書き込める窓 `Mut`）。N 次元の `Tensor` と `Tensor.View`。ベンチマークの拡張（C との同条件比較、`f32`・`i64`・wasm32）。GPU との連携は公開 API の組み合わせだけ |

## 確定した API と意味

### モジュールと登録

- `Matrix`・`MatrixView`・`Tensor` は別々の opt-in std モジュール（D-40）。名前を書いたプログラムだけが読み込み、`MatrixView` は `Matrix`、`Tensor` は `Matrix` と `MatrixView` も連れてくる（`stdlib::OPT_IN` の `uses`）。名前を書かないプログラムの IR は追加前とバイト一致（fixture・例・ベンチマーク 342 ファイル、native と wasm32、`-O0` と `-O3`）。
- 登録は `src/stdlib.rs` の `SOURCES`（末尾）・`RESERVED_MODULES`（46 → 49。`reserves_the_d07_table`）・`opaque_record`（`Matrix.Matrix`・`MatrixView.MatrixView`・`MatrixView.Mut`・`Tensor.Tensor`・`Tensor.View`）・`OPT_IN` と、`src/check.rs` の `Type::is_noncopy_record`（`Matrix.Matrix`・`Tensor.Tensor`・`Tensor.View`）、`vsc/src/core.ts` の `libraryModules`（予約名と一致する検査がある）。`vsc/resources/completions.json` は追跡されず、std の `def` 行から生成される。
- 内部は不透明（`E1022`）、公開 C ABI への export は `E1008`、`Matrix.tz`・`MatrixView.tz`・`Tensor.tz` の名前のファイルは `E1011`。

### Matrix

- `of_array init rows cols at get row as_array to_array set map fold transpose add mul mul_fma mul_parallel mul_fma_parallel`。row-major の `record Matrix<'a> { rows, cols, data }`。不変条件は `rows >= 0`、`cols >= 0`、`rows * cols` が `i64` に収まる、`data.length == rows * cols`。前提条件は確保や callback の前に `assert` でトラップする。
- `mul` の出力 `(i, j)` は `+0` から `k` の昇順に `total + (left * right)` を行った値で、積と和は別々に丸める。実装は出力の行ごとに `k`・`j` で更新する i-k-j 順で、各出力要素の演算の列は i-j-k の素朴な和と同じ。ビットは Phase 1 と同一（JavaScript の参照、独立した i-j-k のオラクル、IR の検査、契約の probe で確認）。
- `mul_fma`（`Float<'a>`）は `total = fma(a, b, total)` を `+0` から `k` の昇順に行う（`Array.dot_fma` と同じ順序）。`mul` とビットが違うので別名。暗黙の FMA 化はない。
- `mul_parallel`・`mul_fma_parallel`（`Send<'a>` も要る）は、出力の行を `Parallel.for_each_chunk` で分ける。1 チャンクは約 2^20 回の積和になる行数（`max(1, 1048576 / (inner * cols))` 行）で、形だけで決まる。結果は `mul`・`mul_fma` とビットが一致する。1 チャンクの積は呼び出しスレッドで直接計算し、2 チャンク以上では入力を `Arc` に 1 回複製して共有する。
- `set` は行列を消費して 1 要素をその場で置き換える。

### MatrixView

- `record MatrixView<'a> { r } { data: ref { r } ['a], offset, rows, cols, row_stride, col_stride }`（Copy）と `record Mut<'a> { r } { data: ref mut { r } ['a..], ... }`（非 Copy）。要素 `(row, col)` は `data[offset + row * row_stride + col * col_stride]`。ストライドは負にならず、窓の全要素は `data` の中にある（`strided` が `extent` で検査し、操作が保つ）。
- `of_matrix of_array strided rows cols offset row_stride col_stride data at get transpose sub row col is_contiguous to_matrix fold map mul`（`transpose`・`sub`・`row`・`col` は O(1)）。`mul` は窓を行優先に複製して `Matrix.mul` を呼ぶ。
- `Mut`: `of_array_mut write row_mut sub_mut transpose_mut fill copy_from map_in_place freeze`。配置は行優先（`col_stride` が 1）かその転置に限り、異なる要素が同じ場所を指さない。
- 診断: 局所の所有者の窓を返すと `E1013`（`does not live long enough`）、窓が生きている間の所有者の move・置換・`to_array`・`set`、`Mut` が生きている間の元の配列の読み書きと 2 つ目の窓は `E1014`、`Mut` の move 後の使用は `E1012`、タスクでの使用は `E1013`、フィールド参照などは `E1022`。

### Tensor

- `record Tensor<'a> { shape: [i64], data: ['a] }`（非 Copy）と `record View<'a> { r } { data: ref { r } ['a], offset, shape: [i64], strides: [i64] }`（非 Copy。API は `ref View`）。軸は 16 本まで、長さは 0 以上、長さが 0 の軸を除いた積は `i64` に収まる。長さが 0 の軸があれば要素は 0 個、軸が 0 本なら 1 個。
- 所有者: `of_array init of_matrix to_matrix as_array to_array reshape view borrow`。窓: `rank shape count strides offset data at get permute index_axis narrow is_contiguous reshape_view fold to_tensor map as_matrix_view of_matrix_view`。`reshape_view` は連続な窓だけで、連続でなければトラップ。
- `Matrix` との変換は複製なし（`of_matrix`・`to_matrix`）。2 軸の窓は `MatrixView` として見られる。

### GPU との連携

提供するのは公開 API の組み合わせだけ: `Matrix.as_array`・`Matrix.of_array` と `Gpu.from_array`・`Gpu.map`・`Gpu.to_array`（CPU 参照バッファ。文書の例と `tests/matrix.rs` が検査）。行列積の GPU カーネルは提供しない。今の GPU カーネルの部分集合は 2 次元の添字と総和を書けず、`src/gpu.rs`・`std/Gpu.tz`・`src/runtime/webgpu.mjs` は F09 の担当である。

## 旧計画から外れた判断

| 旧判断 | 確定した判断と理由 |
| --- | --- |
| Phase 2 は `Builtin` と `src/llvm_bulk.rs` の最適化カーネル | 不要。`right` の行を内側のループにした i-k-j を std ソースで書くと、LLVM が自動ベクトル化し、同じ演算順序の C の i-k-j と同じ速さ（M1 Max、`f64` 約 11〜12 GFLOP/s）。手書きの `f64x2` のブロックは約 7.5 で遅かったので採らなかった。`Simd` は具体型でしか書けず、`@cpu` は x86-64・AArch64 の Linux だけなので、どちらも使っていない |
| `mul` の内側の添字は `left[base + k]` | `left` の行を先にスライスにする。out-of-line の関数では、配列の長さから作られた重なりの検査が配置に依存し、約 5.4〜8 GFLOP/s に落ちた。スライスの長さが `inner` だと分かる形では約 11.9 |
| ブロッキング・pairwise の積 | 提供しない。測定上の必要がなく、順序が違うので別名が要る |
| `Matrix` だけの opt-in | 窓と N 次元も別モジュールにして、読み込みを必要な範囲に限る |
| 並列は `Parallel` に任せる | コールバックは借用を捕捉できない（`E1013`）ので、入力を `Arc` に複製する。1 チャンクの積は複製せず直接計算する |
| 一時の窓を式の途中で借りる | `E1013`（`borrow requires a local place`）。一時の窓は `let` で束縛する。添字の配列リテラルも値で渡す |
| `_docs/library-reference/matrix.md` | `_tsuzuri/language-reference/built-in-types-and-modules/` の `matrix.md`・`matrix-view.md`・`tensor.md` |
| 旧文書の整数リテラルの例 | 既定が `i32` の現在の言語と `i64` の `def` に合わせて書き直した |

## 受け入れ条件

- Phase 1: 14 関数と、トラップ・借用・不透明・non-Copy・登録の検査。`mul` の演算順序が JavaScript の参照と一致（NaN・無限大・符号付きゼロ・非正規化数・`f32`・`f64`・`i64`）。native と wasm32 の `-O0`・`-O3`。`live == 0`。既定の wasm32 に import なし。IR に `fmuladd`・`fast`・`contract`・`reassoc` なし。文書と `check-docs`。
- Phase 2: 窓・N 次元・書き込める窓の accept / reject テストと、窓の独立なモデルによる E2E。同名の `mul` は Phase 1 とビット同一、順序の違う積は別名。並列の分割は形だけで決まる。ASan と TSan。ベンチマークは C と同条件で、計時の前にチェックサムのビット一致を検査し、速度の合否の閾値を置かない。

## 実装と検証（2026-10-10）

### 実装したファイル

- `std/Matrix.tz`、`std/MatrixView.tz`、`std/Tensor.tz`（新規）。登録は `src/stdlib.rs`、`src/check.rs`、`vsc/src/core.ts`。
- `tests/matrix.rs`、`tests/matrix_views.rs`、`tests/tensor.rs`、`tests/lsp.rs`（補完と hover）。
- `tests/matrix-cases.mjs`・`tests/matrix-view-cases.mjs`・`tests/tensor-cases.mjs`（独立な参照）、`tests/fixtures/matrix`・`matrix_view`・`tensor`、`tests/features.mjs`（suite `matrix`・`matrix_view`・`tensor`。`matrix` は CPU 数 1・4 の native と WASM threads の並列の積も検査）。
- `benchmarks/matrix/Main.tz`、`benchmarks/run-matrix.mjs`。
- `_tsuzuri/language-reference/` の `matrix.md`・`matrix-view.md`・`tensor.md`（新規）、`index.md`、`array.md`、`parallel.md`、`simd.md`、`gpu.md`、`languages/strategy.md`・`why-tsuzuri.md`、`organizing-tsuzuri/modules.md`。`docs/language.md`・`architecture.md`・`benchmarks.md`、`README.md`。

### 検証結果

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`sh scripts/check-runtime-includes.sh`、`git diff --check` が成功。GUIDE §3.1 の 4 つの回帰テストは各 1 件で成功。最終の `RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast` は 89 バイナリで **999 件成功**（Phase 1 の後は 984 件）。
- `cargo test --locked --test matrix`（8 件）、`--test matrix_views`（7 件）、`--test tensor`（4 件）、`--lib stdlib`（9 件）、`--test lsp opt_in`（2 件）が成功。
- suite `matrix`（529 ケース）、`matrix_view`（302）、`tensor`（33）が native と WASM の `-O0`・`-O3` で成功。`live == 0`、WASM の import は空。`TSUZURI_ASAN=1`、`TSUZURI_TSAN=1`、`TSUZURI_TEST_WASM_SIMD=1`、`TSUZURI_TEST_CPU=native`、`TSUZURI_TEST_WASM_TARGET=wasm64`、`TSUZURI_TEST_ALLOCATOR=host` でも成功。
- 窓と N 次元の suite は、`strides` を取り違えた変異（`narrow` が stride を無視、`transpose` が stride を入れ替えない、`copy_from` が stride を取り違える）で失敗することを確認した。
- `node scripts/check-docs.mjs` は変更した 11 ページで 44 例・86 回の native 実行が成功。`tsuzuri doc std` が 3 モジュールの API 文書を出す。

### 文書と計測

測定は Apple M1 Max（10 コア）、macOS 27.0.0、Apple clang 21.0.0、Node v20.19.6。`node benchmarks/run-matrix.mjs target/release/tsuzuri` の 3 回分（生データは ignored の `target/perf/C11-phase2/`）。条件・表・生成コードの確認は [性能測定](../../docs/benchmarks.md#行列積c11)。

## 既知の限界

- x86-64、AVX・SVE のような広いベクトル、wasm32 の threads の速度、`n = 1024` 以上、ブロッキングする BLAS との比較は未測定。測定は 1 台の arm64 で、負荷の揺れが大きい（2 回目の並列の値が落ちた原因は特定していない）。
- wasm32 には 1 命令のスカラー fma がなく、`Math.fma`（`mul_fma`）は `tz_soft_fma` を呼ぶので、`n = 64` の積が約 460 ms（`mul` の約 4,000 倍）。
- 窓は負のストライドを持てない。書き込める窓は行優先とその転置だけ。N 次元の書き込める窓、ブロードキャスト、軸に沿った集計はない。
- 行列積の GPU カーネルはない。GPU の実機での実行は F09。
