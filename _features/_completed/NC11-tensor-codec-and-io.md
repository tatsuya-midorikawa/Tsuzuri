# NC11: テンソル永続化モジュール `TensorCodec`（`.npy`/`.npz`/Safetensors/CSV）と `TensorIO`（ファイル/`mmap`）

| 項目 | 内容 |
| --- | --- |
| ID | NC11 |
| 優先度 | P2（他ツール・Python/NumPy・機械学習モデルとのデータ交換に必須） |
| 規模 | L（2 Phase: Phase 1 純粋 `[ubyte]` コーデック `TensorCodec`、Phase 2 ファイル読み書き・ネイティブ `mmap` の `TensorIO`） |
| 依存 | [NC01](NC01-complex-and-extended-math.md), [NC02](NC02-tensor-core-views-and-broadcasting.md), ([W11](W11-windows-os-api.md)) |
| 後続 | – |
| 状態 | done（Phase 1・1b・2。2026-11-28） |
| 起票 | 2026-11-24（[GUIDE D-65](../GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 承認済み（D-65 の委任で決定、2026-11-28。D1・D2 と D-55 の登録。「方針の最新化」と決定事項の D4〜D8）。起票時の記述: 要承認（新規 opt-in std モジュール `TensorCodec`・`TensorIO` の `RESERVED_MODULES` 登録と Phase 2 の `mmap` 専用組み込み。決定事項 D1・D2 が要承認、ほかは既定案） |
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

## 方針の最新化（2026-11-28）

起票（2026-11-24、HEAD `27069465`）の後に入った変更（W11・W24・NS07・M15・NC16〜NC21・edition 2028・リファレンスの別リポジトリへの移動）と、HEAD `b72bccd0` の実装を読んで、着手前に方針を改めた。**この節が、下の「仕様」「設計」「実装手順」と食い違う箇所では、この節が正である。** 決定は利用者の委任（「あなたが考える最高の選択肢で実装することを常に許可します」）による。

### 1. HEAD と食い違っていた記述

| 起票時の記述 | HEAD の事実 | 改めた方針 |
| --- | --- | --- |
| 文書は `_tsuzuri/language-reference/built-in-types-and-modules/*.md` | 利用者向けの日本語リファレンスは別リポジトリ `Tsuzuri-Reference` の `docs/stdlib/...`。`scripts/check-cli-docs.mjs` の `STD_PAGES` もその相対パス | ページは `docs/stdlib/encoding/tensor-codec.md`（`TensorElement` も同じページ）・`docs/stdlib/encoding/tensor-io.md`。`STD_PAGES` に同じパスを足す |
| `src/stdlib.rs:843` の件数、`src/target.rs:397`、std-table の 88 関数 | `RESERVED_MODULES` は 83（`src/stdlib.rs:1029`）。行番号と件数はどれも古い | 現在の値から数える（GUIDE-LITE §11） |
| Windows の `mmap` は W11 の後 | W11 は done（`src/runtime/os-windows.h`） | Windows（`CreateFileW`・`CreateFileMappingW`・`MapViewOfFile`・`UnmapViewOfFile`）も Phase 2 に含める。手元では実行できないので CI（`ci:full`）で確かめる |
| S1: Deflate が大きければ Phase 1b へ分ける | – | 分けずに、同じ PR で Phase 1b（Deflate／Inflate）まで実装する |
| スライスや `for` の範囲 | std は edition 2028 で解析する（`Edition::LATEST`） | 範囲は `..<`・`..=` を明示して書く |

### 2. `mmap` の借用スライスの作り方（D2 の再検討）

起票の後に入った W24 の `extern view` と NS07 の `ExternAddr` で、非公開の組み込みが不要になるかを確かめた。

- `extern view def f :: ExternAddr -> R` は、アドレスにある C レイアウトの record を**呼び出しの時点でフィールドごとに複写する**（W24 の仕様 3）。借用 `ref { r } [T]` は作れず、要素数が実行時に決まる領域も写せない。`mmap` の目的（複写しない読み取り）に合わない。
- `ExternAddr` は利用者が外部のアドレスを渡すための型である。`MappedTensor` は不透明な record（構築とフィールド参照は `E1022`）で、アドレスは runtime から受け取った値だけが入るので、内部の表現は `i64` で足りる。

よって D2 のとおり、**std の `TensorIO` だけが呼べる非公開の組み込み `TensorIO.__mmap_slice :: i64 -> i64 -> ref 'owner -> ref ['a]`**（アドレス・要素数・持ち主の借用）を足す。結果は引数の借用をすべて保持する（`Vec.__view` と同じ規則）ので、スライスは持ち主の `MappedTensor` の借用より長く生きられない。所有する `[T]` はどこにも作らない。

マップと解除は、M15 の D13 の形（独立した入口、std だけが呼ぶ内部の組み込み、到達したときだけの `-D`）に合わせる。`Os.__read` などの既存の操作に番号を足すと、OS API に到達するすべてのプログラムのランタイムの object が変わるので採らない。

- `Os.__map :: ref utf8string -> (i64 * [ubyte])`（`tsuzuri_os_map`。成功なら 16 バイト: アドレスと長さの little-endian の `i64`。長さ 0 のファイルはアドレス 0・長さ 0）。権限は `fs`。
- `Os.__unmap :: i64 -> i64 -> i64`（`tsuzuri_os_unmap`。アドレスと長さ）。メモリーだけに触るので権限は要らない。
- どちらも `os.c`・`os-windows.h`・`os-wasi.c` の `#if defined(TZ_OS_MAP)` の中に置く。WASI は `Other`（ENOSYS）を返す。

`MappedTensor` は 2 段にする。資源を持つ非公開の record `Mapping { address, length }` が `Drop`（`Os.__unmap`）を持ち、`MappedTensor` はそれと検証済みのメタデータ（dtype・shape・strides・バイト位置・要素数）を持つ。マップした直後に `Mapping` を作るので、ヘッダーの検証が失敗した経路でも解除は `Drop` が 1 回だけ行う。

### 3. 要素型の入口（D3）と、バイト列との変換

- HEAD には浮動小数点のビット列を取り出す組み込みがない（`std/Cbor.tz` は算術で `f64` のビットを組み立てている）。算術の変換は NaN の payload を保てず、要素ごとのループになる。そこで、**std の `TensorCodec` だけが呼べる非公開の組み込みを 2 つ**足す。どちらも連続した領域の 1 回の `memcpy` である。
  - `TensorCodec.__to_bytes :: Numeric<'a> => ref ['a] -> [ubyte]`
  - `TensorCodec.__of_bytes :: Numeric<'a> => i64 -> i64 -> ref [ubyte] -> ['a]`（バイト位置と要素数。範囲外はトラップで、std の側が先に検証する）
  - 対応する target（x86-64・arm64・wasm32/64）はすべて little-endian なので、メモリーのバイト列がそのまま `<` の並びになる。big-endian のファイルは、複写の前にバイト列を入れ替える。
- 型別の入口（`encode_npy_f64` など）は 14 の dtype × 形式の数だけ関数が増えるので採らない。**`.tt` の型クラス `TensorElement`（`std/TensorElement.tt`）を足し、入口を 1 つにする**（D3 の後者の案。`NpyElement` という名前は D1 と同じ理由で使わない）。instance は `std/TensorCodec.tz` に置く（`bool`、`i8`〜`i64`、`i8u`〜`i64u`、`f16`、`f32`、`f64`、`Complex<f32>`、`Complex<f64>`）。`RESERVED_MODULES` は 3 つ増える（`TensorCodec`・`TensorIO`・`TensorElement`）。
- 型を消した中間の値 `TensorCodec.Raw { dtype, shape, data }`（C 順・little-endian のバイト列）を足す。dtype が実行時に決まる読み込み（`decode_npy_raw`）と、要素型の違うテンソルを 1 つのアーカイブにまとめる書き出し（`encode_npz`・`encode_safetensors` の入力は `[(string * Raw)]`）に使う。起票時の `EncodedTensor` に当たる。

### 4. API の整理

- `.npz` と Safetensors の読み取りは、同じ `TensorCodec.Archive` を返す。`Archive` はバイト列を**所有する**（`decode_npz :: own [ubyte] -> ...`。`File.read_bytes` の結果を複写なしで渡せる。借用にすると `Result` の中の region の制約に当たる）。要素は `archive_get`・`archive_raw` で名前から取り出し、その時点で展開・検証する。
- `Error` に `InvalidArchive of string`（ZIP・Safetensors の構造）、`CorruptData of string`（Deflate の流れ・CRC）、`UnsupportedRank of i64`（CSV の 3 次元以上）を足す。`encode_csv` は `Result` を返す。
- Deflate・Inflate・CRC-32 は公開する（`deflate`・`inflate`・`crc32`）。`.npz` の外でも使え、Python の `zlib` と両方向で照合できる。
- `.npz` の書き出しは決定的にする（時刻は 1980-01-01 00:00:00 の固定）。`np.savez` は現在時刻を書くので、アーカイブ全体のバイト一致は求めない。`.npy`（`np.save`）はバイト単位で一致させる。
- `TensorIO` の型別のビュー（`view_f64` など）は、型クラスで 1 つの `view { r } :: TensorElement<'a> => ref { r } MappedTensor -> Tensor.View<'a> { r }` にする。`bool` のファイルは、0・1 以外のバイトを `bool` として読むと未定義になるので、`open` の時点で全バイトを検査する（`bool` だけ、マップした全体を 1 回読む）。
- `Drop` の値は `let!` の後ろの継続へ捕捉できない（`E1005`）。IO の連鎖の中で使えるように、`with_mmap_npy :: own string -> own (ref MappedTensor -> 'a) -> IO<Result<'a, Error>>` も足す。
- `open_mmap_safetensors :: own string -> own string -> IO<Result<MappedTensor, Error>>`（テンソルの名前とパス）を足す（早見の `mmap_safetensors`）。
- Safetensors の複素数は、公開されている形式の表に確かな対応がないので、書き出しも読み込みも `UnsupportedDtype` にする。

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

### 委任による決定（2026-11-28）

利用者の委任（「あなたが考える最高の選択肢で実装することを常に許可します」）で決めた。GUIDE D-65 の「委任による決定（2026-11-28、NC11）」と D-55 の表に記録した。

- **D1・D2**: 起票の案のとおり承認した。モジュール名は `TensorCodec`・`TensorIO`、pickle の dtype は拒否、`mmap` には非公開の組み込みを足す。
- **D4**: D3 は `.tt` の型クラスの案を採り、名前は `TensorElement`（`std/TensorElement.tt`）にする。`RESERVED_MODULES` は 3 つ増える。型別の入口は 14 の dtype × 形式の数だけ関数が要るので採らない。
- **D5**: `TensorElement` は `TensorCodec.ElementType<'a>`（不透明なレコード）を返すメソッドで封じ、利用者の instance を持てなくする。`TensorIO.view` がマップしたメモリーを要素型として読むので、ポインターを持つ型の instance があると健全でなくなる（S2 の一部）。
- **D6**: 浮動小数点のビットを保つために、非公開の組み込み `TensorCodec.__to_bytes`・`TensorCodec.__of_bytes`（数値の配列とバイト列の間の 1 回の複写）を足す。HEAD にはビット列を取り出す手段がなく、算術の変換は NaN の payload を保てない。
- **D7**: マップと解除は `Os.__map`・`Os.__unmap`（`-DTZ_OS_MAP`）にする。M15 の D13 と同じ形で、既存の `Os.__read` などに操作を足さない。W24 の `extern view` は複写なので使えない。
- **D8**: API を整理する。`.npz` と Safetensors は同じ `Archive`（バイト列を所有する）を返す。型を消した `Raw` を足す。`Error` に `InvalidArchive`・`CorruptData`・`UnsupportedRank` を足す。`deflate`・`inflate`・`crc32` を公開する。`view` は 1 つの総称関数にし、`with_mmap_npy`・`open_mmap_safetensors`・`inspect_safetensors`・`archive_inspect`・`encode_npz_with`（`NpzOptions`）を足す。CSV の読み込みは `Parse` による総称の `decode_csv` 1 つにする。Safetensors の複素数は書かない。

## 実装と検証（2026-11-28）

Phase 1（`.npy`・無圧縮の `.npz`・Safetensors・CSV）、Phase 1b（Deflate／Inflate）、Phase 2（ファイルの読み書きと読み取り専用のマップ。POSIX・Windows・WASI）をすべて実装した。macOS 27.2（arm64）、Apple clang、`target/debug/tsuzuri`（dev profile）、Python 3.14.7 と NumPy 2.5.1 で確かめた。Windows の実行は手元ではできず、CI（`ci:full`）に任せた（下の「確かめていないこと」）。

### 実装した内容

- **コンパイラ**: 非公開の組み込み 5 つ（`Os.__map`・`Os.__unmap`・`TensorCodec.__to_bytes`・`TensorCodec.__of_bytes`・`TensorIO.__mmap_slice`。`Builtin::ALL` は 392 → 397）。`src/check.rs`（宣言・名前・署名）、`src/polymorph.rs`（呼び出し元のモジュールの検査、`E1022`）、`src/llvm.rs`・`src/llvm_io.rs`・`src/llvm_bulk.rs`（生成）、`src/effects.rs`・`src/kg/effects.rs`・`src/pure_rules.rs`・`src/pinned_rules.rs`（分類）、`src/capabilities.rs`（`Os.__map` は `fs`、ほかの 4 つは権限なし）、`src/driver.rs`・`src/test_runner.rs`（`-DTZ_OS_MAP`）、`src/stdlib.rs`（`SOURCES`・`RESERVED_MODULES` 83 → 86・`OPT_IN`・`opaque_record`）。
- **ランタイム**: `src/runtime/os.c`（`mmap`・`munmap`）、`src/runtime/os-windows.h`（`CreateFileMappingW`・`MapViewOfFile`・`UnmapViewOfFile`）、`src/runtime/os-wasi.c`（ENOSYS）。どれも `#if defined(TZ_OS_MAP)` の中。
- **std**: `std/TensorCodec.tz`（2,136 行。公開関数 31）、`std/TensorElement.tt`（型クラス 1、メソッド 3）、`std/TensorIO.tz`（391 行。公開関数 19）。
- **テスト**: `tests/tensor_codec.rs`（13 件）、`tests/fixtures/tensor_codec/generate.py` と `reference.json`（NumPy・zlib・zipfile の出力）、`tests/fixtures/tensor_codec/Main.tz` と `tests/tensor-codec-cases.mjs`（`tests/features.mjs` の suite `tensor_codec`）、`tests/fixtures/tensor_io/Main.tz` と `tests/tensor_io.mjs`。

### 実行したコマンドと結果

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 差分・警告なし。
- `cargo nextest run --locked`: 3813 tests run: 3813 passed (34 slow), 4 skipped。
- `cargo test --locked --test tensor_codec`: 13 passed。中で `tsuzuri test` が動かす Tsuzuri のテストは 162 件（`.npy` 54 ファイル（14 の dtype × 行優先・列優先・big-endian、0〜3 階、空、形式 1.0・2.0・3.0、NaN の payload と符号・`-0.0`・非正規化数、Python 2 の書式）を読み、同じ配列の `np.save` の出力とバイト単位で同じに書く。窓 5 種。不正な `.npy` 34 種と、接頭辞・1 バイトの変更。`np.savez`・`np.savez_compressed`・ZIP64 を強制した zipfile のアーカイブ 3 つ。不正な `.npz` 19 種。zlib の流れ 30 本（無圧縮・固定符号・動的符号）と 400 回の変更。形式の記述から作った Safetensors と不正な 27 種。`np.savetxt` の 7 種）。
- `cargo test --locked --test std_argument_order --test std_examples --test std_tags --test stdlib --test capabilities`: 22・5・3・7・18 passed。`cargo test --locked --lib -- effects:: capabilities:: kg::effects pure_rules pinned_rules stdlib::`: 38 passed。
- `target/debug/tsuzuri test --doc std --filter TensorCodec`／`TensorElement`／`TensorIO`（`-O0` と `-O3`）: 31・1・19 passed。`target/debug/tsuzuri test --doc std`（全モジュール）: 1401 passed, 0 failed。
- `node tests/features.mjs target/debug/tsuzuri tensor_codec`: 52 cases passed（native と wasm32 の `-O0`・`-O3`、`live == 0`、wasm は import なし）。
- `node tests/tensor_io.mjs target/debug/tsuzuri`: 342 checks passed（native と WASI の `-O0`・`-O3`。NumPy が書いたファイルの読み込みとマップ、マップできないファイルの拒否、要素型の違う `view` のトラップ、書いたファイルのバイト比較と独立した ZIP・Safetensors の読み手、NumPy 2.5.1 による `np.load`・`np.loadtxt`・`zlib` での読み込み、4 GiB の疎なファイルを 100,000 回マップして解除する漏れの検査）。
- `node scripts/gen-docs.mjs --check`: 11 blocks checked, 0 stale。`node scripts/check-cli-docs.mjs`: 4 checks, 0 failed。`node scripts/check-guide-lite.mjs`: 87 references checked, 0 failed。`sh scripts/check-runtime-includes.sh`: 66 files。
- Tsuzuri-Reference: `node scripts/check-docs.mjs` を新しい 2 ページと `index.md`・`conventions.md` に実行し、4 pages, 239 links, 13 checked examples, 26 native runs。
- `TensorCodec`・`TensorIO` を名指さないプログラムの IR: fixture と example の 172 個の `--emit llvm -O1`（native と wasm32）の出力 340 個（`.ll` 323、診断 17）が、変更の前後でバイト単位で同じ（増えたのは新しい 2 つの fixture の 4 個だけ）。

### チケットから外れた判断

「方針の最新化」と「委任による決定」のとおり。実装中に足したもの:

- `TensorIO.MappedTensor` は `Mapping`（`Drop` を持つ非公開の record）を包む 2 段にした。ヘッダーの検証が失敗した経路でも、解除は drop glue が 1 回だけ行う。
- `bool` のファイルは、マップするときに全バイトが 0・1 であることを確かめる。`view_bool` を別にする代わりである。
- `.npz` の名前の `.npy` の判定は、`String.ends_with` ではなくバイトの比較にした。`String.matches_at` の `while` に付く展開の指示（`llvm.loop.unroll.enable`）が、インライン化の文脈によって clang の `loop not unrolled` の警告を標準エラーへ出すためである（下の「見つけたこと」）。

### 確かめていないこと

- **Windows の実行**: `os-windows.h` の `tsuzuri_os_map`・`tsuzuri_os_unmap` は、手元にヘッダーも実行環境もないのでコンパイルも実行もしていない。`cargo check --target x86_64-pc-windows-msvc` も、手元の Rust（Homebrew）にその target の標準ライブラリがなく実行できなかった（Rust の側の変更に target ごとの分岐はない。CI の `lint` が確かめる）。`windows.yml` に `node tests/tensor_io.mjs` の手順を足したので、`ci:full` の実行で確かめる。Windows では漏れの検査は 64 KiB のファイルを 2,000 回マップするだけで、漏れを検出できない（NTFS は疎なファイルを割り当てるため）。
- **Linux**: 手元では動かしていない（`targets.yml` と `ci.yml` に手順を足した）。
- **Python の `safetensors` パッケージ**: 入っていないので、Safetensors は公開されている形式の記述から作ったファイルと独立した読み手で確かめた。参照実装とのバイト一致は確かめていない。
- **4 GiB を超える `.npz`**: ZIP64 の読み書きは、zipfile に ZIP64 を強制したアーカイブと `zip64: true` の出力（NumPy が読む）で確かめた。実際に 4 GiB を超えるアーカイブは作っていない。
- **性能**: 速さの主張はしない。`.npy` の読み書きは連続した領域の 1 回の `memcpy`、Deflate の出力の大きさは zlib の水準 6 とほぼ同じ（試した 10 種で ±1%）であることだけを確かめた。

### 見つけたこと（このチケットの外）

- `while` の本体の代入が 1 つで、終了がデータに依る繰り返し（例: `while index < end && is_space bytes[index] do index = index + 1`、`for` の中の `Array.write` と 1 つのカウンター）に `llvm.loop.unroll.enable` が付き、clang が `warning: loop not unrolled` を標準エラーへ出すことがある（利用者のコードでも起きる。`std/String.tz` の `matches_at` も、インライン化の文脈によって出す）。std の 2 つのモジュールでは書き方を変えて避け、`tests/tensor_codec.rs` と `tests/tensor_io.mjs` が警告のないことを検査する。コンパイラの側の修正は別のチケットにする。
- `scripts/check-cli-docs.mjs --reference-dir` は、このチケットと無関係な失敗を `main` でも出す（Tsuzuri-Reference に `Linalg`・`ExternAddr`・`ZUtf8String` のページがない、`Tensor` の 4 関数、`--framework`・`--rpath`、`W1011`）。

### 更新した文書

- このリポジトリ: `docs/language.md`（新しい節「TensorCodec / TensorIO」、予約モジュール名）、`docs/architecture.md`（新しい節「テンソルのファイル（NC11）」、OS API の `-DTZ_OS_MAP`）、`README.md`（標準ライブラリの表）、`_features/GUIDE.md`（D-55 の行、D-65 の委任による決定）、`_features/README.md`、`.github/workflows/ci.yml`・`targets.yml`・`windows.yml`（suite の手順）。
- Tsuzuri-Reference（ブランチ `docs/nc11-tensor-codec-io`）: `docs/stdlib/encoding/tensor-codec.md`・`tensor-io.md`（新規）、`docs/index.md`、`docs/stdlib/conventions.md`、`scripts/check-cli-docs.mjs`（`STD_PAGES`）。
