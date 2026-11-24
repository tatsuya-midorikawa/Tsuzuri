# NC14: Python / NumPy ゼロコピー相互運用（`--emit bindings-py` Buffer Protocol・`__array_interface__`・DLPack）

| 項目 | 内容 |
| --- | --- |
| ID | NC14 |
| 優先度 | P3（Tsuzuri 製数値カーネルを Python / NumPy / PyTorch からコピーなしで直接呼び出す） |
| 規模 | M（`abi.rs` へのストライドテンソル記述子＋`bindings_native.rs` の Python バインディング拡張） |
| 依存 | [NC02](NC02-tensor-core-views-and-broadcasting.md), [NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（C ABI へのストライドテンソル記述子 `tsuzuri_tensor_view` / `tsuzuri_tensor_buffer` 追加と Python バインディング契約。決定事項 D1・D2 が要承認、ほかは既定案） |
| 手本にする既存実装 | `src/bindings_native.rs`（`--emit bindings-py` の `ctypes` ラッパー生成）、`src/abi.rs`、`src/llvm_abi.rs`、`tests/host_bindings.mjs` |
| 主な影響ファイル | `src/abi.rs`、`src/llvm_abi.rs`、`src/llvm.rs`、`src/bindings_native.rs`、`tests/python_tensor_interop.rs`（新規）、`tests/host_bindings.mjs`、`spec/rules/ABI.md`、`docs/language.md`、`_tsuzuri/language-reference/compiler/native-interop.md` |

## 実装者向け早見

- 範囲:
  1. `export def` で `ref Tensor.View<T>`（入力）および `Tensor<T>`（所有返却、`T` は `i8`〜`i64`, `i8u`〜`i64u`, `f32`, `f64`, `bool`, `Complex<f32>`, `Complex<f64>`）を C ABI（`--emit header`・`--emit shared`）に公開できるようにする（フラットな C 構造体 `tsuzuri_tensor_view_<T>`：`data`, `rank`, `shape`, `strides` のバイト／要素ストライド）。
  2. `--emit bindings-py`（`src/bindings_native.rs`）が、`numpy.ndarray`（あるいは `__array_interface__` / Python Buffer Protocol `memoryview` を持つ任意のオブジェクト）を受け取った際に、**要素コピーなしでポインタ・形状・ストライドをそのまま渡す**コードを生成する。返却された `Tensor<T>` は、`tsuzuri_free` 付きファイナライザを所有するゼロコピー NumPy 配列ビュー（または明示 `.copy()` 可能なオブジェクト）として返す。
  3. Phase 2 として、PyTorch・JAX・CuPy・NumPy 間で標準化された **DLPack**（`DLManagedTensor` 互換カプセル）エクスポート／インポートに対応する。
- 最初に読む: ① `src/bindings_native.rs` の Python 生成部 ② `src/abi.rs` と `src/llvm_abi.rs` ③ 仕様 1〜3。
- 最初に書くテスト: `tests/python_tensor_interop.rs`（`export def` の C ヘッダに `tsuzuri_tensor_view_f64` が出ること、非対応要素型が `E1008` になること）と、`tests/host_bindings.mjs`（Python から NumPy 配列または `memoryview` を渡し、コピーなしで計算結果を受け取り、解放漏れがないこと）。
- 触ってはいけない: 既存の `export def` スカラー・1D 配列・レコードの C ABI と生成バインディング。Python バインディング実行時の NumPy 必須化（NumPy 未導入環境でも標準 `memoryview` / `array.array` で動くフォールバックを保つ: D2）。
- 完了の見分け方: Tsuzuri で `export def normalize_rows :: ref Tensor.View<f64> -> Tensor<f64>` を `--emit shared --emit bindings-py` でビルドし、Python から 2D 非連続（転置済み）NumPy `ndarray` をゼロコピー渡しして結果が一致し、メモリリークがない。

## 目的

Tsuzuri で書いた安全かつ高速な数値計算関数を、既存の Python / NumPy / PyTorch エコシステムからネイティブ拡張として即座に活用できるようにする。現行の `--emit bindings-py` は 1D の `i64`/`f64`/`ubyte` 配列を `ctypes` 経由でコピー渡しするだけのため、大規模多次元配列のやり取りで大きなオーバーヘッドが発生している。

## 着手条件と停止条件

### 着手条件

- [NC02](NC02-tensor-core-views-and-broadcasting.md)（負/0 ストライド対応の `Tensor.View`）と [NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md)（全数値型バッファの ABI 基盤）が done。
- 決定事項 D1・D2 の承認。

### 停止条件

- **S1** 入力された NumPy 配列のストライドが要素サイズの整数倍でない（パディング付き構造化配列等）場合や、読み取り専用バッファへの不正書き込みが起きる場合、コピーでごまかさず Python 側バインディングで明示的に `ValueError` / `TypeError` を送出する。
- **S2** `Tensor.View` の要素型が C ABI 非対応（例: `Tensor.View<string>` や非スカラーレコード）の `export def` が誤って通ってしまう場合（必ず `E1008` で拒否する）。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `src/bindings_native.rs`: `--emit bindings-py` は `ctypes` ベースで、スカラー・スカラーレコード・1D 配列（`i64`/`f64`/`ubyte`）・文字列・不透明ハンドルに対応。`Tensor` / `Matrix` の export は `E1008`。
- `std/Tensor.tz` の `View` は要素単位ストライド（`i64`）を持つ（NumPy はバイト単位ストライドなので境界で `byte_stride / itemsize` に変換・整除検証する）。

## 変えてはいけない意味

- 既存の C / C# / Python / C++ バインディングの型・関数名・メモリ解放規約。
- `Tensor` / `Tensor.View` を使わない `export def` の C ヘッダおよび生成コード。

## 仕様

### 1. C ABI 記述子（`--emit header`・`--emit shared`）

- 入力 `ref Tensor.View<T>`:
  ```c
  typedef struct {
      const T *data;
      int64_t data_len;     /* 境界検証用の元バッファ要素数 */
      int64_t offset;
      int64_t rank;
      const int64_t *shape;
      const int64_t *strides; /* 要素単位ストライド */
  } tsuzuri_tensor_view_<t>;
  ```
  Tsuzuri 入口ラッパーは、`NC02` の `Tensor.strided` と同一のオーバーフロー安全境界検査を必ず実行してから本体関数へ渡す（ホスト側の不正ストライドによる範囲外読み取りを防ぐ）。
- 出力（所有返却）`Tensor<T>`:
  ```c
  typedef struct {
      T *data;
      int64_t data_len;
      int64_t rank;
      int64_t *shape;
  } tsuzuri_tensor_buffer_<t>;
  ```
  ホストは専用解放関数 `tsuzuri_free_tensor_<t>(tsuzuri_tensor_buffer_<t> *)` で `data` と `shape` を 1 回だけ解放する。

### 2. Python バインディング（`--emit bindings-py`）

- **入力（負ストライド正規化・読み取り専用バッファ対応）**:
  - NumPy の `__array_interface__['data'][0]` および Python Buffer Protocol の `Py_buffer.buf` は、バッファ先頭ではなく**要素 `(0, 0, ...)` のアドレス**を指す。そのため負ストライドを含む配列（例: `arr[::-1]`）を `offset = 0` のまま渡すと `offset + sum(min_d) < 0` で NC02 の境界検査に引っかかりトラップする。
  - したがってバインディング側で以下を計算してから `tsuzuri_tensor_view_<t>` を構築する:
    - 空配列（いずれかの `shape[d] == 0`）: `base_ptr = data_ptr`（または null 許容空長 0）、`offset = 0`、`data_len = 0`。
    - 非空配列: バイトストライドが `itemsize` で割り切れることを検証し、`elem_strides[d] = byte_strides[d] // itemsize` を算出。各軸の最小・最大相対オフセット `min_rel = sum(min(0, (shape[d] - 1) * elem_strides[d])) <= 0`、`max_rel = sum(max(0, (shape[d] - 1) * elem_strides[d])) >= 0` を求め、
      - `base_ptr = data_ptr + min_rel * itemsize`
      - `offset = -min_rel`（`>= 0`）
      - `data_len = max_rel - min_rel + 1`
      として渡す。これにより全要素が `[0, data_len)` に厳密に収まり、NC02 の境界検査をそのまま通過する。
  - **読み取り専用バッファの取得**: Python の `ctypes.from_buffer` は読み取り専用 `memoryview` や `arr.flags.writeable = False` の NumPy 配列で `TypeError` を送出するため、`__array_interface__` があればその `data[0]` ポインタを直接使い、ない場合は `ctypes.pythonapi.PyObject_GetBuffer`（`PyBUF_STRIDED_RO`、呼び出し後に必ず `PyBuffer_Release`）を用いて読み取り専用バッファもコピーなしで取得する。型・エンディアン・非整除ストライドの不一致は `TypeError` / `ValueError` を送出（暗黙の型変換・暗黙コピーはしない）。
- **出力**:
  - NumPy が import 可能な環境では、`__array_interface__` を公開する所有ラッパー（`__del__` で `tsuzuri_free_tensor_<t>` を正確に 1 回呼ぶ）を `base` として `np.asarray(owner)` を返し、コピーなしで NumPy `ndarray` としてそのまま使えるようにする。NumPy がない環境では標準 `memoryview`＋`.shape` 属性付きラッパーを返す。

### 3. Phase 2: DLPack 対応

- 生成 Python バインディングに `__dlpack__` / `__dlpack_device__` および `from_dlpack` 入力対応を追加し、PyTorch・JAX・NumPy 2.x とゼロコピー交換できるようにする。

## 設計

- C ABI 層での境界検査を Tsuzuri 側入口ラッパーで必ず行うことで、C / Python ホストから誤った `shape`/`strides` を渡されても未定義動作を防ぎ、既存のトラップ境界（E14 `--trap-mode return`）と協調して Python 例外へ安全に変換する。
- 生成コードは外部 Python パッケージ（Cython/pybind11/nanobind 等）を一切必要とせず、Python 標準の `ctypes` のみで動作する（NumPy が入っていれば自動でゼロコピー連携）。

## 前提とする他チケットのインターフェース

- [NC02](NC02-tensor-core-views-and-broadcasting.md): 負/0 ストライド `Tensor.View` と境界検査関数。
- [NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md): `f32`/`i32`/`Complex` 等の C ABI 型マッピング。

## 他チケットへ提供するインターフェース

- `export def` の `Tensor`/`Tensor.View` ABI とゼロコピー Python/NumPy バインディング。

## 実装手順

1. **変更**: `src/abi.rs`, `src/llvm_abi.rs`, `src/llvm.rs`。**内容**: `export def` での `ref Tensor.View<T>` と所有返却 `Tensor<T>` の C ABI 構造体・入口境界検証・解放関数生成を実装する。**確認**: T0、`tests/python_tensor_interop.rs`（ヘッダと IR 検証）。
2. **変更**: `src/bindings_native.rs`。**内容**: `--emit bindings-py` の `__array_interface__` / `PyObject_GetBuffer(PyBUF_STRIDED_RO)` ゼロコピー入出力（負ストライド正規化付き）とファイナライザを生成する。**確認**: T0、`tests/host_bindings.mjs`（Python 実行: C 連続・転置・負ストライド・読み取り専用配列のゼロコピー渡し、dtype 不一致エラー、解放確認）。
3. **変更**: `src/bindings_native.rs`（Phase 2: DLPack）、`spec/rules/ABI.md`、`docs/language.md`、`_tsuzuri/language-reference/compiler/native-interop.md`。**内容**: DLPack 対応とドキュメント更新。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **Rust テスト（`tests/python_tensor_interop.rs`）**: 全対応要素型の `export def` ヘッダ・IR 決定性、非対応要素型（`string` 等）の `E1008`、C ABI 既存テスト不変。
- **Python 実行テスト（`tests/host_bindings.mjs`）**: `memoryview`（NumPy 非依存・読み取り専用含む）および NumPy 導入環境での 0D/1D/2D/3D・転置・負ストライド（`arr[::-1]`）・部分スライス・`writeable=False` 入力のゼロコピー確認（入力とラッパーのアドレス一致）、不正 dtype/ストライドでの `ValueError`/`TypeError`、大量往復でのメモリリークなし確認。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`
- `docs/language.md`, `spec/rules/ABI.md`

## 受け入れ条件

- 2D/3D・転置・負ストライド・読み取り専用の NumPy `ndarray`（および標準 `memoryview`）をコピーなしで Tsuzuri 共有ライブラリへ渡し、結果をゼロコピーで受け取れること。
- 不正な `shape`/`strides`/dtype が安全にエラー報告され、解放漏れ・二重解放がないこと。
- T2 と `node scripts/check-docs.mjs` が通ること。

## 落とし穴

- NumPy の `data[0]` は要素 `(0, 0, ...)` のアドレスであり、`arr[::-1]` の先頭アドレスではない。`base_ptr` / `offset` / `data_len` の負ストライド正規化を省くと全負ストライド配列が境界検査でトラップする。
- `ctypes.from_buffer` は読み取り専用 `memoryview` や `writeable=False` 配列で `TypeError` を投げるため使ってはならない。必ず `__array_interface__` または `PyObject_GetBuffer(PyBUF_STRIDED_RO)`＋`PyBuffer_Release` を使うこと。
- Python 側 `ctypes` で渡した元オブジェクトを C 呼び出しの間ずっと生存参照保持しないと、一時配列が GC されてダングリングポインタになり得る。

## 対象外

- CPython C-API 拡張モジュール（`.so`/`.pyd` 直接生成）や Cython/pybind11 依存コード生成。
- 非連続な GPU デバイスメモリ上の DLPack バッファの自動ホスト転送（Phase 2 の DLPack はまず CPU デバイスから対応する）。

## 決定事項

- **D1（要承認）**: `export def` の C ABI に `ref Tensor.View<T>`（入力）と `Tensor<T>`（所有返却）を追加し、要素単位ストライドの記述子構造体と入口境界検査を標準化する。
- **D2（要承認）**: 生成される Python バインディングは NumPy を実行時必須依存とせず、NumPy 導入時は `__array_interface__`（Phase 2 で DLPack）でゼロコピー `ndarray` 連携、未導入時は標準 `memoryview` で動作する設計とする。
- **D3（既定案）**: 暗黙の dtype 変換や暗黙コピーは行わない（型・エンディアン・ストライド整除の不一致は明示エラー）。
