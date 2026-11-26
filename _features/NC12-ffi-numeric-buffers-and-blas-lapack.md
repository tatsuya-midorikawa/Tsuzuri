# NC12: C FFI 数値可変バッファ拡張と外部 BLAS / LAPACK 明示バックエンド

| 項目 | 内容 |
| --- | --- |
| ID | NC12 |
| 優先度 | P2（大規模行列積・分解でネイティブ最高速を得るための外部数値ライブラリ接続基盤） |
| 規模 | L（`abi.rs` の数値バッファ拡張＋`Linalg.*_blas` の明示オプトイン接続） |
| 依存 | [W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md), [NC01](_completed/NC01-complex-and-extended-math.md), [NC08](_completed/NC08-linalg-core-and-einsum.md) |
| 後続 | [NC14](NC14-python-buffer-protocol-and-dlpack-bindings.md) |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（C ABI バッファ要素型の拡張と、外部 BLAS/LAPACK 経路の API 契約・能力検査。決定事項 D1・D2 が要承認、ほかは既定案） |
| 手本にする既存実装 | [W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md)（`ref mut [T..]` と `@count`）、`src/abi.rs`（`Buffer`）、`src/llvm_imports.rs`（`host_call`）、`std/Matrix.tz`（`mul` vs `mul_fma` の明示分離） |
| 主な影響ファイル | `src/abi.rs`、`src/llvm_abi.rs`、`src/llvm_imports.rs`、`src/bindings.rs`（WASM バインディングの `Buffer` 網羅マッチ）、`src/bindings_native.rs`、`std/Linalg.tz`、`tests/ffi_numeric_buffers.rs`（新規）、`tests/ffi_extensions.mjs`、`tests/host_bindings.mjs`、`spec/rules/ABI.md`、`docs/language.md`、`docs/benchmarks.md`、`_tsuzuri/language-reference/compiler/native-interop.md`、`_tsuzuri/language-reference/built-in-types-and-modules/linalg.md` |

## 実装者向け早見

- 範囲:
  1. **C FFI 数値バッファ型の拡充**: [W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md) が `ref mut [T..]` へ追加する `i64`/`f64`/`ubyte`/`i16u`/`i32`/`i32u` に加えて、`f32`・`i8`・`i16`・`i64u`・`Complex<f32>`・`Complex<f64>` を `ref [T]` および `ref mut [T..]` の C FFI バッファ（コピーなしポインタ渡し、W13 の `@count` 対応）として受理するようにする。これにより CBLAS (`cblas_sgemm`, `cblas_dgemm`, `cblas_cgemm`, `cblas_zgemm`)・LAPACKE (`LAPACKE_dgesdd`, `LAPACKE_dgesv` 等)・FFTW を Tsuzuri の `@trusted extern` からゼロコピーで直接呼べるようになる。
  2. **明示的 BLAS / LAPACK バックエンドの提供**: 決定論的な純 Tsuzuri 経路（`Linalg.matmul`, `Linalg.svd`, `Linalg.solve` 等）とは明確に名前を分けたオプトイン経路（または外部パッケージ／ビルド時リンク構成による `Linalg.matmul_blas` 等）を設計・提供し、未リンク・未検出時に黙って純 Tsuzuri へフォールバックせず明示エラーを返す（AGENTS.md「明示要求した利用不可バックエンドはエラー報告」）。
- 最初に読む: ① [W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md) 全文 ② `src/abi.rs` の `Buffer` ③ 仕様 1〜2。
- 最初に書くテスト: `tests/ffi_numeric_buffers.rs`（`ref [f32]`・`ref mut [f32..]`・`ref [Complex<f64>]`・`ref mut [Complex<f64>..]` の IR がコピーなしで `ptr` を渡すこと、WASM では `abi_native_only` `E1008` になること）。
- 触ってはいけない: 既定のビルドに BLAS/LAPACK リンク依存を混入させること（既定バイナリ・WASM の依存ゼロを厳守）。純 Tsuzuri の `Matrix.mul` / `Linalg.matmul` の決定論ビット契約。
- 完了の見分け方: Tsuzuri から CBLAS の `cblas_dgemm` / `cblas_sgemm` / `cblas_zgemm` を `@trusted extern` でゼロコピー呼び出しでき、参照 C 実装と同等の GFLOP/s かつ `Linalg.matmul`（純 Tsuzuri）と許容誤差内で一致する。

## 目的

現状の [abi.rs](../src/abi.rs) は `Buffer` に `I64`・`F64`・`UByte`・`String`・`Utf8String` の読み取り専用のみを持ち、`f32` や `Complex` の配列・可変出力バッファを C 関数へ渡せないため、OpenBLAS・Apple Accelerate・Intel MKL・FFTW といった既存の高度最適化 C ライブラリと連携できない。C FFI の数値バッファを拡張することで、Tsuzuri 本体の依存ゼロ性を損なわずに、大規模線形代数で BLAS/LAPACK 級の性能を引き出せるようにする。

## 着手条件と停止条件

### 着手条件

- [W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md)（`ref mut [T..]` と `@count`）、[NC01](_completed/NC01-complex-and-extended-math.md)（`Complex`）、[NC08](_completed/NC08-linalg-core-and-einsum.md)（`Linalg`）が done。
- 決定事項 D1・D2 の承認。

### 停止条件

- **S1** `Complex<f32>` / `Complex<f64>` のメモリ配置（2 要素連続配置 `[re, im]`）が LLVM レコード並べ替えやプラットフォーム ABI と食い違う場合、C ABI 境界での配置保証（`docs/architecture.md` の std レコード配置不変条件）を確認し、食い違えば止めて報告する。
- **S2** 既定ビルド（BLAS をリンクしない通常の `tsuzuri build`）に未解決シンボルや外部ライブラリ依存が漏れる場合。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `src/abi.rs`: `Buffer` は `I64`, `F64`, `UByte`, `String`, `Utf8String` の 5 種のみ。
- `W13`（todo）が `ref mut [T..]` の `i64`/`f64`/`ubyte`/`i16u`/`i32`/`i32u` と `@count` を導入する計画だが、`f32`・`i8`・`i16`・`i64u`・`Complex<f32>`・`Complex<f64>` は対象外。
- `tsuzuri build` には既存の `--link`・`-l`・`-L` オプション（E12）がある。

## 変えてはいけない意味

- 既存の `ref [i64]`/`ref [f64]`/`ref [ubyte]` の C ABI と生成ヘッダ。
- 既定の WASM 出力（インポートなし）および既定ネイティブビルドの外部 BLAS 非依存。
- `Linalg.matmul` 等の決定論的純 Tsuzuri 経路のビット一致契約。

## 仕様

### 1. C FFI 数値バッファ型の拡充（`src/abi.rs`・`src/llvm_abi.rs`・`src/llvm_imports.rs`）

- [W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md) がすでに対応する `i64`/`f64`/`ubyte`/`i16u`/`i32`/`i32u` に加えて、`ref [T]`（読み取り専用）および `ref mut [T..]`（W13 の可変バッファ、native のみ）で追加受理する要素型 `T`:
  - `f32`（C 型 `const float *` / `float *`）
  - `i8`（`int8_t *`）, `i16`（`int16_t *`）, `i64u`（`uint64_t *`）
  - `Complex<f32>`（C 型 `const float *` / `float *` または `void *` の 2 要素連続配置。CBLAS の `const void *` / C99 `float _Complex *` と ABI 互換）
  - `Complex<f64>`（同様に `double` 2 要素連続配置）
- W13 の `@count(pos, none)` と組み合わせることで、`cblas_dgemm(order, transA, transB, M, N, K, alpha, A, lda, B, ldb, beta, C, ldc)` のように長さ引数を伴わない標準 CBLAS / LAPACKE / FFTW シグネチャへ追加コピー 0 で直結できる。
- `export def` と生成バインディング（C#・Python・C++）でも `ref [f32]`・`ref [i32]` 等を安全に扱えるよう対応を揃える（`NC14` の土台）。

### 2. BLAS / LAPACK バックエンドの接続方針（D2）

- **明示オプトインと能力検査**:
  - 外部ライブラリ（Apple Accelerate / OpenBLAS / Intel MKL / BLIS）のリンクは明示的なパッケージ依存（例: 公式の薄い `blas` パッケージでの `@link(framework = "Accelerate")` / `@link("openblas")`）またはビルドフラグ（`-l openblas` / `--framework Accelerate` / `[native] frameworks` 等）で選択する。
  - 利用時に BLAS が利用不可な環境で明示的に BLAS 経路を呼んだ場合は、黙って純 Tsuzuri に落とさずエラーを返す。自動選択（`Auto`）経路を設ける場合のみ、能力検査の上で文書化されたフォールバック（純 Tsuzuri 経路）を許す（AGENTS.md の方針）。
  - 数値セマンティクス（FMA 縮約・スレッド数依存の丸め順序緩和）は純 Tsuzuri 決定論経路と異なるため、API 名で明確に分離する。

## 設計

- `src/abi.rs` の `Buffer` 列挙型に新しい数値要素型を追加し、`c_parameters` と `host_call` のポインタ渡しロジックを共通化する。
- `Complex<f32>` / `Complex<f64>` のメモリ配置は `std/Complex.tz` の `record Complex<'a> { re: 'a, im: 'a }`（std レコードは並べ替えなし: `docs/architecture.md`）により `re` がオフセット 0、`im` がオフセット `sizeof('a)` であることが保証される。単体テストで `llvm_layout` のサイズ・アライメント・オフセットを明示検証する。

## 前提とする他チケットのインターフェース

- [W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md): `abi::extern_parameter`, `ref mut [T..]`, `@count`, `abi_native_only`。
- [NC01](_completed/NC01-complex-and-extended-math.md): `Complex<'a>`。
- [NC08](_completed/NC08-linalg-core-and-einsum.md): `Linalg` の基準実装（精度照合用）。

## 他チケットへ提供するインターフェース

- `f32`/`i32`/`i16`/`i8`/`i64u`/`Complex<f32>`/`Complex<f64>` の C ABI バッファ（[NC14](NC14-python-buffer-protocol-and-dlpack-bindings.md) の Python/NumPy バインディングが利用）。
- CBLAS/LAPACKE ゼロコピー接続の公式パターンとテスト済みバインディング。

## 実装手順

1. **変更**: `src/abi.rs`, `src/llvm_abi.rs`, `src/llvm_imports.rs`, `src/bindings.rs`, `src/bindings_native.rs`。**内容**: W13 で追加済みの `i64`/`f64`/`ubyte`/`i16u`/`i32`/`i32u` に加えて、`f32`・`i8`・`i16`・`i64u`・`Complex<f32>`・`Complex<f64>` の `ref [T]` / `ref mut [T..]` バッファ対応（および WASM `bindings.rs` の `Buffer` 網羅マッチ更新・WASM 側の `abi_native_only` 検査整合）を追加する。**確認**: T0、`tests/ffi_numeric_buffers.rs`（IR 検査＋`Complex` レイアウト検証）、T1。
2. **変更**: `tests/ffi_extensions.mjs`・`tests/ffi_extensions_host.c`・`tests/host_bindings.mjs`（モック CBLAS 風 C 関数とのリンク実行テスト）。**内容**: `cblas_sgemm` / `cblas_dgemm` / `cblas_zgemm` と同形の C 関数へ `@trusted @count(..., none) extern` でゼロコピー呼び出しし、結果と `live == 0` を確認する。**確認**: `node tests/ffi_extensions.mjs target/debug/tsuzuri`、`node tests/host_bindings.mjs target/debug/tsuzuri`。
3. **変更**: 公式 BLAS 連携パターン（`Linalg` またはサンプル/ベンチマークでの実測確認、`docs/benchmarks.md` 記録）、`spec/rules/ABI.md`、`docs/language.md`、`_tsuzuri/language-reference/compiler/native-interop.md`。**内容**: 実機（Accelerate / OpenBLAS）での性能計測・検証とドキュメント更新。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **Rust テスト（`tests/ffi_numeric_buffers.rs`）**: 新規全バッファ型の `extern` / `export` IR、コピーなしポインタ渡し、WASM での `E1008` `abi_native_only`、`Complex<f32>`（8 bytes）/ `Complex<f64>`（16 bytes）レイアウト検証。
- **E2E（`tests/ffi_extensions.mjs`）**: C 側で直接書き換えた `f32`/`f64`/`Complex` 出力バッファが Tsuzuri 側で正しく読めること、確保リークなし。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`
- `_tsuzuri/language-reference/built-in-types-and-modules/linalg.md`
- `docs/language.md`, `docs/benchmarks.md`, `spec/rules/ABI.md`

## 受け入れ条件

- `f32`/`f64`/`i32`/`Complex<f32>`/`Complex<f64>` の入出力バッファが CBLAS/LAPACKE 互換の C 関数へゼロコピーで渡せること。
- 既定のネイティブ/WASM ビルドに BLAS 依存が入らないこと。
- T2・`node scripts/check-docs.mjs` が通ること。

## 落とし穴

- `src/abi.rs` の `Buffer` にバリアントを追加すると、`src/bindings.rs`（WASM JS glue）と `src/bindings_native.rs`（C#/Python/C++）の双方に網羅マッチ箇所がある。WASM 側で `Float32Array`/`Int32Array` 対応とするか `abi_native_only` にするか統一して抜けをなくすこと。
- CBLAS の複素数引数（`const void *alpha` 等）は `Complex<f64>` の単一スカラーもポインタで受け取る。単一 `ref Complex<f64>` 引数の C ABI（スカラーレコード）とも整合させること。

## 対象外

- Tsuzuri ツールチェーンへの OpenBLAS / LAPACK バイナリ同梱。
- 暗黙に純 Tsuzuri 経路を BLAS へ差し替える自動切替（決定論契約が変わるため明示 API 分離を徹底する）。

## 決定事項

- **D1（要承認）**: C ABI の `Buffer` 要素型に（W13 で追加済みの `i64`/`f64`/`ubyte`/`i16u`/`i32`/`i32u` に加えて）`f32`・`i8`・`i16`・`i64u`・`Complex<f32>`・`Complex<f64>` を追加する（`ref mut [T..]` は W13 と同じく native 限定）。
- **D2（要承認）**: BLAS/LAPACK 連携は Tsuzuri 本体へハードコードせず、外部リンク可能な薄いバインディング（または明示オプトイン API）として提供し、決定論的純 Tsuzuri 経路とは名前・契約を明確に分離する。
