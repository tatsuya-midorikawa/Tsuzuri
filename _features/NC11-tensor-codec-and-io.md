# NC11: テンソル永続化モジュール `TensorCodec`（`.npy`/`.npz`/Safetensors/CSV）と `TensorIO`（ファイル/`mmap`）

| 項目 | 内容 |
| --- | --- |
| ID | NC11 |
| 優先度 | P2（他ツール・Python/NumPy・機械学習モデルとのデータ交換に必須） |
| 規模 | L（2 Phase: Phase 1 純粋 `[ubyte]` コーデック `TensorCodec`、Phase 2 ファイル読み書き・ネイティブ `mmap` の `TensorIO`） |
| 依存 | [NC01](NC01-complex-and-extended-math.md), [NC02](NC02-tensor-core-views-and-broadcasting.md), ([W11](W11-windows-os-api.md)) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（新規 opt-in std モジュール `TensorCodec`・`TensorIO` の `RESERVED_MODULES` 登録と Phase 2 の `mmap` 専用組み込み。決定事項 D1・D2 が要承認、ほかは既定案） |
| 手本にする既存実装 | `std/Json.tz`・`std/Cbor.tz`（ファイル I/O 非依存の純粋 `encode`/`decode` 設計）、`std/File.tz`（`read_bytes`/`write_bytes`・`fs` 権限）、`src/runtime/os.c`（ネイティブ OS API）、`scripts/check-cli-docs.mjs`（`STD_PAGES`）、`tests/capabilities.rs`・`tests/fixtures/capabilities/std-table.txt` |
| 主な影響ファイル | `std/TensorCodec.tz`（新規）、`std/TensorIO.tz`（新規）、`src/stdlib.rs`、`src/check.rs`（Phase 2 `mmap` スライス合成組み込み）、`src/llvm.rs`、`src/capabilities.rs`、`src/runtime/os.c`（Phase 2 の `mmap` のみ）、`tests/capabilities.rs`、`tests/fixtures/capabilities/std-table.txt`、`tests/tensor_codec.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/`、`scripts/check-cli-docs.mjs`（`STD_PAGES`）、`vsc/src/core.ts`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor-codec.md`（新規）、`_tsuzuri/language-reference/built-in-types-and-modules/tensor-io.md`（新規）、`_tsuzuri/language-reference/index.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: 特定エコシステム名（`Npy` 等）をトップレベルモジュール名にせず、Tsuzuri の `Json`/`Cbor` と同じ「純粋変換とファイル I/O の分離」方針に従い 2 モジュールを提供する:
  - **Phase 1: `std/TensorCodec.tz`（純粋 `[ubyte]` / `utf8string` 変換。権限不要・WASM 動作）**:
    - `.npy` 形式（Format v1.0 / v2.0 / v3.0、リトル/ビッグエンディアン、C/Fortran 順、全主要 dtype: `bool`, `i8`〜`i64`, `i8u`〜`i64u`, `f16`, `f32`, `f64`, `Complex<f32>`, `Complex<f64>`）。
    - `.npz` アーカイブ（複数名前付きテンソルの ZIP store、および純 Tsuzuri の Deflate/Inflate 圧縮対応版）。
    - **Safetensors** 形式（JSON ヘッダ＋64 bit 長さプレフィックス＋生テンソルバイト列。既存の `Json` モジュールを再利用）。
    - **CSV / TSV テキスト表形式**（`encode_csv`/`decode_csv`、区切り文字・ヘッダスキップ・コメント文字指定。NumPy `loadtxt`/`savetxt` 相当）。
  - **Phase 2: `std/TensorIO.tz`（ファイル I/O ラッパー＋読み取り専用 `mmap`）**:
    - `File.read_bytes`/`File.write_bytes` を組み合わせた `save_npy`/`load_npy`/`save_archive`/`load_archive`/`save_safetensors`/`load_safetensors`/`save_csv`/`load_csv`（`fs` 権限）。
    - ネイティブ向け読み取り専用メモリマップ（`TensorIO.mmap_npy` / `mmap_safetensors`、`Drop` で自動 `munmap`/`UnmapViewOfFile`。WASM では未対応エラーを返す）。
- 最初に読む: ① `std/Cbor.tz`・`std/Json.tz` のエラー・encode/decode 設計 ② 仕様 1〜3 ③ `std/File.tz` と `src/capabilities.rs`。
- 最初に書くテスト: `tests/tensor_codec.rs` で `.npy` バイナリ（Python/NumPy が生成した実バイト列フィクスチャ）のデコード・エンコード往復、ビッグエンディアン読み込み、Fortran 順 `.npy` 読み込み、不正マジック/破損ヘッダ/dtype 不一致の `Result.Error`、Safetensors と CSV の往復。
- 触ってはいけない: 既定 WASM のインポートなし（Phase 1 `TensorCodec` は一切の OS/WASM インポートを要しない）。`TensorCodec`/`TensorIO` を名指さないプログラムの IR。
- 完了の見分け方: Python/NumPy の `np.save`・`np.savez`・`np.savetxt` で書き出したファイルを Tsuzuri の `TensorCodec`/`TensorIO` でそのまま読み込め、逆に Tsuzuri で書き出したファイルを Python/NumPy の `np.load` で無修正に読み込める。

## 目的

数値計算や機械学習では、既存の Python/NumPy や外部ツールとのデータ交換（`.npy`/`.npz`/Safetensors/CSV）と、大規模重みファイルの低メモリ読み込み（`mmap`）が不可欠である。ファイル非依存の純粋変換（`TensorCodec`）とファイル I/O（`TensorIO`）を分離することで、WASM・ネットワーク越し転送・ローカルファイルのすべてで安全かつ中立な API を提供する。

## 着手条件と停止条件

### 着手条件

- [NC01](NC01-complex-and-extended-math.md) と [NC02](NC02-tensor-core-views-and-broadcasting.md) が done。
- Phase 2 の Windows 側 `mmap` は [W11](W11-windows-os-api.md) が done（現行 `src/target.rs:397` の `os_apis()` は macOS/Linux のみ対応。macOS/Linux の Phase 2 は W11 なしでも着手可能）。
- 決定事項 D1・D2 の承認。

### 停止条件

- **S1** Phase 1 の `.npz` 圧縮版で純 Tsuzuri Deflate 実装が過大になる場合、Phase 1a を無圧縮 ZIP（ZIP_STORED、`np.savez` 相当）＋`.npy`＋Safetensors＋CSV として先に出荷し、圧縮 `.npz`（ZIP_DEFLATED、`np.savez_compressed` 相当）を Phase 1b へ分離する。
- **S2** Phase 2 の `mmap` で、マップ領域を誤って所有 `['a]` として扱い Drop 時に `@tz.free` が呼ばれる危険や、`MappedTensor` の `Drop`（`munmap`）より `Tensor.View<'a> { r }` が長生きする抜け道が見つかった場合は止めて報告する。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `std/Json.tz` と `std/Cbor.tz` が構造化データの純粋エンコード／デコードを担っているが、多次元数値配列向けのバイナリ／表形式コーデック（`.npy`/`.npz`/Safetensors/CSV）は存在しない。
- `std/File.tz` に `read_bytes`/`write_bytes`/`open`/`read`/`write` はあるが、`mmap` は未実装。

## 変えてはいけない意味

- `Json`/`Cbor`/`File` の既存契約。
- 破損ファイル・悪意あるヘッダ（過大な形状積オーバーフロー、ヘッダ長超過、範囲外オフセット）で未定義動作や OOM クラッシュを起こさず必ず `Result.Error` を返すこと。

## 仕様

### 1. Phase 1: `std/TensorCodec.tz`（opt-in、純粋・権限不要）

- **エラー型**: `union Error = InvalidMagic | UnsupportedVersion of (i64 * i64) | InvalidHeader of string | DtypeMismatch of (string * string) | UnsupportedDtype of string | TruncatedData | ShapeOverflow | EntryNotFound of string | UnsupportedCompression of i64 | CsvParseError of (i64 * string)`（`deriving (Eq)` + `Display`）。
- **`.npy` 形式（NumPy NPY Format v1.0 / v2.0 / v3.0 準拠）**:
  - `encode_npy_f64 :: ref Tensor.View<f64> -> Result<[ubyte], Error>`（`f32`, `f16`, `i64`, `i32`, `i16`, `i8`, `i64u`, `i32u`, `i16u`, `i8u`, `bool`, `Complex<f32>`, `Complex<f64>` の型別入口、または `NpyElement` クラスによる単一 `encode_npy` / `decode_npy`: D3）
  - `inspect_npy :: ref [ubyte] -> Result<Header, Error>`（`{ dtype: Dtype, shape: [i64], fortran_order: bool, data_offset: i64 }` を先読み）
  - リトルエンディアン `<`・ビッグエンディアン `>`・バイト順序非依存 `|` の全読み込みに対応し、ビッグエンディアンはバイトスワップしてホスト順序へ変換。Fortran 順（`fortran_order: True`）は行優先へ変換（または列優先ビュー解釈）。
- **`.npz` アーカイブ**:
  - `encode_npz :: ref [(string * EncodedTensor)] -> Result<[ubyte], Error>`、`decode_npz_index :: ref [ubyte] -> Result<Archive, Error>`、`get_f64` 等で名前付きテンソルを取り出し。
- **Safetensors 形式**:
  - `encode_safetensors` / `decode_safetensors_index`（JSON メタデータ解析に既存 `std/Json.tz` を使用。バイトオフセットの重複・範囲外・整列違反を厳格に検査）。
- **CSV / TSV**:
  - `encode_csv :: Display<'a> => own CsvOptions -> ref Tensor.View<'a> -> utf8string`（2D 以下）、`decode_csv_f64 :: own CsvOptions -> ref utf8string -> Result<Tensor<f64>, Error>`（`delimiter`, `skip_rows`, `comment` 指定）。

### 2. Phase 2: `std/TensorIO.tz`（opt-in、`fs` 権限）

- ファイル読み書きコンビニエンス API: `save_npy`, `load_npy_*`, `save_npz`, `load_npz`, `save_safetensors`, `load_safetensors`, `save_csv`, `load_csv_*`（内部で `File.read_bytes`/`File.write_bytes` と `TensorCodec` を合成）。
- ネイティブ読み取り専用 `mmap`（POSIX `mmap`/`munmap`、W11 完了後に Windows `CreateFileMappingW`/`MapViewOfFile`/`UnmapViewOfFile`）:
  - **非ジェネリック**不透明ハンドル `record MappedTensor`（Tsuzuri は未使用の型パラメータを持つレコードを `E1024` で拒否するため、`MappedTensor<'a>` のような幽霊型パラメータは使わない）。`Drop` 実装付きで `munmap` / `UnmapViewOfFile` を呼ぶ。内部では所有 `['a]` 配列を持たず、生のマップハンドルと検証済みの `dtype: TensorCodec.Dtype`・`shape`・`strides`・`byte_offset`・`element_count` を保持する。
  - `open_mmap_npy :: own string -> IO<Result<MappedTensor, Error>>`（および期待 dtype も同時に検証する `open_mmap_npy_as :: own TensorCodec.Dtype -> own string -> IO<Result<MappedTensor, Error>>`）。`open` 時点で `.npy` ヘッダ・エンディアン一致・アライメント一致・境界をすべて検証する（不一致や WASM は `Result.Error`）。
  - メタデータ取得: `mapped_dtype :: ref MappedTensor -> TensorCodec.Dtype`、`mapped_shape :: ref MappedTensor -> ref [i64]`。
  - 型別ビュー取得: `view_f64 { r } :: ref { r } MappedTensor -> Tensor.View<f64> { r }`（`view_f32`, `view_i64`, `view_i32`, `view_u8`, `view_complex_f64` 等。dtype が一致しない場合は事前条件違反としてトラップする。非 `Result` なので `regions.rs` の最上位 `{ r }` 契約を満たし、コンパイラ内蔵の非公開組み込み `TensorIO.__mmap_slice` で `MappedTensor` の借用寿命 `{ r }` に結ばれた `ref { r } [T]` スライスのみを合成して返す。所有 `[T]` を一切作らないため、Drop 時に誤って `@tz.free` が呼ばれることはない）。

## 設計

- `TensorCodec` は `OPT_IN` へ `uses: &["Json", "Tensor", "Complex"]`、`TensorIO` は `uses: &["TensorCodec", "Tensor"]` として登録し（いずれもソース自身が直接参照する opt-in モジュールのみ。推移閉包は `sources_for` が自動解決する）、`RESERVED_MODULES` に `"TensorCodec"`・`"TensorIO"` を追加して `src/stdlib.rs:843` の件数アサーションを更新する。`TensorCodec` は `File` / `Os` に依存しない純粋モジュールとする。
- `.npy` ヘッダは Python 辞書リテラルサブセット（`{'descr': '<f8', 'fortran_order': False, 'shape': (2, 3), }`）の厳格な専用パーサで解析し、任意コード実行の危険（pickle 相当の `O` dtype）は `UnsupportedDtype` で確実に拒否する。
- ハイフン入りリファレンスページを持つため、**`scripts/check-cli-docs.mjs` の `STD_PAGES`** に `TensorCodec: 'built-in-types-and-modules/tensor-codec.md'` と `TensorIO: 'built-in-types-and-modules/tensor-io.md'` を必ず登録する。
- `tests/std_argument_order.rs` の `MODULES` には免除（`Exempt`）を増やさず、`TensorCodec` を `subject(&["Tensor.View", "[ubyte]", "utf8string", "Archive"])`、`TensorIO` を **`subject(&["Tensor.View", "MappedTensor"])`** として登録する（保存関数 `save_npy path view` は `Tensor.View` が最後、`MappedTensor` の関数は `MappedTensor` が最後、パス文字列のみを取る読み込み関数は対象型を含まないため自動適合する）。
- `TensorIO` の `fs` 権限関数は `src/capabilities.rs`・`tests/capabilities.rs`・`tests/fixtures/capabilities/std-table.txt` を更新し、全公開関数を `tests/fixtures/effects_sweep/` で網羅呼び出しする。

## 前提とする他チケットのインターフェース

- [NC01](NC01-complex-and-extended-math.md): `Complex<f32>`, `Complex<f64>`。
- [NC02](NC02-tensor-core-views-and-broadcasting.md): `Tensor`, `Tensor.View`, `is_f_contiguous`, 64 軸。
- 弱い依存 [W11](W11-windows-os-api.md): Phase 2 の Windows 側 `mmap`。

## 他チケットへ提供するインターフェース

- `TensorCodec.*` / `TensorIO.*` 公開 API。

## 実装手順

1. **変更**: `std/TensorCodec.tz`, `src/stdlib.rs`, `tests/tensor_codec.rs`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`, `scripts/check-cli-docs.mjs`, `vsc/src/core.ts`。**内容**: Phase 1（`.npy` v1/v2/v3・`.npz` 無圧縮・Safetensors・CSV のエンコード／デコードと破損入力検証、`STD_PAGES` 登録）を実装する。**確認**: T0、`tests/tensor_codec.rs`（Python 生成バイト列との実データ相互検証）、`cargo test --locked --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
2. **変更**: `std/TensorCodec.tz`。**内容**: `.npz` の Deflate/Inflate 圧縮対応を追加する（Phase 1b）。**確認**: T0、`np.savez_compressed` 生成物との相互検証。
3. **変更**: `std/TensorIO.tz`, `src/stdlib.rs`, `src/check.rs`, `src/llvm.rs`, `src/capabilities.rs`, `src/runtime/os.c`, `tests/capabilities.rs`, `tests/fixtures/capabilities/std-table.txt`, `tests/fixtures/effects_sweep/`, `scripts/check-cli-docs.mjs`。**内容**: Phase 2（ファイル I/O ラッパーとネイティブ読み取り専用 `mmap`＋専用スライス合成組み込み＋`Drop`）。**確認**: T0、`cargo test --locked --test capabilities --test effects_alloc_conservative`、`sh scripts/check-runtime-includes.sh`、T1。
4. **変更**: `tests/fixtures/`・`tests/features.mjs`、文書。**内容**: native/WASM `-O0`/`-O3` E2E（Phase 1 は WASM でもインポートなしで動くこと）とドキュメント更新。**確認**: T2、`node scripts/check-docs.mjs`、`node scripts/check-cli-docs.mjs`。

## テスト計画

- **相互運用テスト（`tests/tensor_codec.rs`）**: 全 dtype（`bool`/`i8`〜`i64u`/`f16`/`f32`/`f64`/`Complex<f32>`/`Complex<f64>`）・0D/1D/3D・空配列・Fortran 順・ビッグエンディアン・v1/v2 ヘッダ・Safetensors・CSV の往復検証と、悪意ある入力（`object` dtype・形状オーバーフロー・切り詰め・範囲外オフセット）の安全な `Result.Error` 拒否。
- **E2E（`tests/features.mjs`）**: Phase 1 の WASM インポートなしと `live == 0`、Phase 2 の `MappedTensor` の `Drop`（`munmap`、ヒープ解放と衝突しないこと）および `view_f64` 等の借用寿命拒否（`E1013`）・dtype 不一致トラップ確認。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/tensor-codec.md`（新規）・`tensor-io.md`（新規）・`index.md`
- `docs/language.md`

## 受け入れ条件

- Python/NumPy の `np.save` / `np.load` / `np.savez` / `np.savetxt` および Safetensors とバイト互換で相互読み書きできること。
- Phase 1（`TensorCodec`）がファイル権限なし・WASM インポートなしで動作すること。
- `capabilities`・`effects_alloc_conservative`・`check-cli-docs`・`sh scripts/check-runtime-includes.sh`・T2・`node scripts/check-docs.mjs` がすべて通ること。

## 落とし穴

- レコードのどのフィールドでも使われない型パラメータは `E1024`（`src/check.rs:8759`）で拒否される。`MappedTensor` は非ジェネリックにして内部に `dtype` タグを持ち、`view_f64` 等の型別入口で取り出すこと。
- `mmap` のマップ先アドレスを所有 `['a]` フィールドへ入れてはならない（自動生成 Drop が `@tz.free` を呼んでヒープ破壊になる）。必ず専用組み込みで借用スライス `ref { r } [T]` だけを作る。
- `view_f64` 等を `Result<Tensor.View<f64> { r }, Error>` にすると `regions.rs` の `ResultRegionMissing`（`E1013`）になるため、ヘッダ・アライメント検証は必ず `open` 時に済ませ `view_*` は非 `Result` にする。
- `OPT_IN.uses` はソース内で直接名指しする opt-in モジュールとの完全一致検査である（推移閉包を書くとテスト失敗）。また `src/stdlib.rs:843` の `RESERVED_MODULES.len()` アサーション更新も必須。
- `TensorCodec` と `TensorIO` は `module.toLowerCase()` が `tensorcodec.md` / `tensorio.md` になるため、`scripts/check-cli-docs.mjs` の `STD_PAGES` へ登録しないと `docs.yml` が落ちる。

## 対象外

- 書き込み可能 `mmap`（外部プロセスによる同時変更でエイリアシング・データ競合が起きるため読み取り専用のみ）。
- HDF5 / Parquet / Arrow IPC 等の重量級フォーマット（外部パッケージ領域）。
- `.npy` の `object`（pickle）dtype および構造化 dtype。

## 決定事項

- **D1（要承認）**: モジュール名は NumPy 固有の `Npy` とせず、中立な **`TensorCodec`**（純粋バイト変換）と **`TensorIO`**（ファイル I/O＋読み取り専用 `mmap`）を採用し、`RESERVED_MODULES`（および `D-55`）へ登録する。
- **D2（要承認）**: `.npy` の Python `object`（pickle）dtype はセキュリティ上絶対にサポートせず `UnsupportedDtype` で拒否する。また Phase 2 の読み取り専用 `mmap` 用に非公開の借用スライス合成組み込みを追加する。
- **D3（既定案）**: 要素型ディスパッチは組み込み型クラスを増やさず、`TensorCodec.encode_npy_f64` 等の型付き入口または `std/TensorCodec.tz` 内の `.tt` 型クラス（`NpyElement`）で提供する。
