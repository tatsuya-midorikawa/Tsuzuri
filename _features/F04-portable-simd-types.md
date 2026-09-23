# F04: 移植可能な SIMD ベクトル型
| 項目 | 内容 |
|---|---|
| ID | F04 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | E02, (F03) |
| 後続 | F05 |
| 状態 | todo |
| 主な影響ファイル | `std/Simd.tz`（新規）, `src/numeric.rs`, `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `src/call_specialization.rs`, `tests/simd.rs`（新規）, `tests/simd.mjs`（新規）, `docs/language.md`, `docs/architecture.md`, `README.md`, `docs/benchmarks.md` |

## 目的

- 固定幅ベクトル型を言語の値型として扱い、LLVM vector 型へ直接下げる。
- LLVM の scalar fallback / legalization により、SIMD 命令がない target でも正しい scalar code を残す。
- native `--cpu generic` / `--cpu native`、WASM feature なし / F03 `simd128` ありの挙動を明確にする。
- 一括 API と operator が別々の性能経路を選ばないよう、型付き LLVM 命令と既存 `binary` lowering を共有する。
- fast-math、reassociation、未サポート命令の無条件使用を避ける。

## 現状

- `src/numeric.rs::primitive` は `i8`〜`i128`, `f16`〜`f128`, decimal, bool, string などの型名を解決している。
- `src/check.rs::Type` には SIMD 型がない。
- `src/check.rs::layout_size` は関数値 / task を 32 bytes、string / array / list を 16 bytes として見積もる。
- `src/check.rs::Type::exportable` は scalar ABI だけを許可する。
- `src/llvm.rs::llvm_type` は scalar numeric を `iN` / `float` / `double`、array を `%tz.array`、function/task を `%tz.closure` に下げる。
- `src/llvm.rs::FunctionEmitter::binary` は integer / float scalar 演算に fast-math flag を付けない。
- `src/llvm.rs::FunctionEmitter::cast` は 8〜64-bit integer と f32/f64 の直接変換、広幅・decimal の software fallback を分けている。
- `src/llvm_frame.rs` は stack literal のサイズ見積もりと relocate を担当する。
- F03 は wasm32 の `--wasm-feature simd128` を提供する予定。
- D-07 は `Simd` モジュールを標準ライブラリ名として予約している。

## 仕様

### 型

- Phase 1 で追加する 128-bit vector 型は次の通り。
- `f32x4`
- `f64x2`
- `i32x4`
- `i64x2`
- `i16x8`
- `i8x16`
- `i32ux4`
- `i64ux2`
- `i16ux8`
- `i8ux16`
- mask 型は lane 幅ごとに次を追加する。
- `mask32x4`
- `mask64x2`
- `mask16x8`
- `mask8x16`
- 256-bit 型 `f32x8`, `f64x4`, `i32x8`, `i64x4` は Phase 2 候補とし、Phase 1 では予約しない。
- SIMD 型は Copy。
- SIMD 型は `needs_drop == false`。
- SIMD 型は `Send` / `Capture` 可能。
- SIMD 型は `exportable == false`。
- SIMD 型はレコード、タプル、配列、リスト、関数引数、戻り値に使える。
- SIMD 型は公開 C/WASM ABI には出さない。

### 構築と lane access

- `Simd.splat : T -> V` を提供する。
- 例: `Simd.splat 1.0f32 : f32x4`。
- `Simd.of_lanes : T -> ... -> V` を lane 数ごとの組み込みとして提供する。
- Phase 1 では専用 literal 構文を導入しない。
- lane access は `Simd.extract : V -> i64 -> T`。
- `Simd.replace : V -> i64 -> T -> V` は指定 lane だけ置換した新しい vector を返す。
- lane index が負または lane 数以上なら trap。
- lane index が compile-time constant の場合は `extractelement` / `insertelement` の immediate index を使う。
- dynamic lane index は bounds check 後に `extractelement` / `insertelement` を使う。

### load / store

- `Simd.load : &[T] -> i64 -> V` を提供する。
- `Simd.load input index` は `input[index + lane]` を lane 昇順で読む。
- `index < 0` または `index + lanes > input.length` は要素 load 前に trap。
- `index + lanes` の overflow も trap。
- `Simd.store : &mut [T] -> i64 -> V -> unit` は C03 が mutable slice を提供する場合に有効化する。
- C03 が mutable slice を提供しない段階では `Simd.store` は std に公開しない。
- `Simd.store` は lane 昇順で書き込むが、配列要素 mutation の言語規則と衝突する場合は C01/C03 の consuming update API へ延期する。
- Phase 1 の既定は `load` のみ必須。

### 演算

- `Add`, `Sub`, `Mul` は integer / float vector に組み込み instance を持つ。
- `Div` は float vector に組み込み instance を持つ。
- integer vector の `Div` / `Rem` は Phase 1 では導入しない。導入する場合は lane ごとのゼロ除算と MIN/-1 を事前検査して trap する。
- `Bits` は integer vector と mask vector に組み込み instance を持つ。
- `Neg` は signed integer vector と float vector に組み込み instance を持つ。
- `Eq` は vector 同士の lane 比較を行い、mask vector を返す。bool ではない。
- `Ord` は numeric vector 同士の lane 比較を行い、mask vector を返す。
- 既存 `Eq` の method signature は bool を返すため、そのままでは使えない。
- 既定案: vector 比較は `Simd.eq`, `Simd.ne`, `Simd.lt`, `Simd.le`, `Simd.gt`, `Simd.ge` として提供し、`Eq` / `Ord` instance は Phase 1 では追加しない。
- `Simd.select : Mask -> V -> V -> V` は mask lane が true のとき第一 vector lane、false のとき第二 vector lane を選ぶ。
- mask の bit 表現は LLVM vector `<N x i1>` とする。

### horizontal reduction

- `Simd.sum_lanes : V -> T` を numeric vector に提供する。
- lane order は 0, 1, 2, ... の左 fold。
- float の `sum_lanes` はその順序で `fadd` し、reassociation しない。
- `Simd.min_lanes` / `max_lanes` は Phase 2。

### LLVM lowering

- `f32x4` は `<4 x float>`。
- `f64x2` は `<2 x double>`。
- `i32x4` / `i32ux4` は `<4 x i32>`。
- `i64x2` / `i64ux2` は `<2 x i64>`。
- `i16x8` / `i16ux8` は `<8 x i16>`。
- `i8x16` / `i8ux16` は `<16 x i8>`。
- mask は `<N x i1>`。
- `Simd.splat` は `insertelement` + `shufflevector` または LLVM `poison` からの splat pattern。
- `of_lanes` は lane 順に `insertelement`。
- `extract` は `extractelement`。
- `replace` は `insertelement`。
- arithmetic は `add/sub/mul` または `fadd/fsub/fmul/fdiv`。
- float vector 演算に fast-math flag を付けない。
- integer add/sub/mul に `nsw` / `nuw` を付けない。
- integer shift を追加する場合は lane ごとに shift count mask を適用する。
- unsupported target では LLVM legalization に任せ、correct scalar code を生成する。
- wasm32 without F03 `simd128` では v128 を要求しないことを検証する。必要なら scalar expansion fallback を emitter 側で実装する。

### layout

- 128-bit vector の `layout_size` は 16 bytes。
- mask128 も 16 bytes と見積もる。
- Phase 2 の 256-bit vector は 32 bytes。
- alignment は LLVM IR の alloca/store/load で 16 bytes を基本とする。
- 256-bit 導入時は 32 bytes align を検討するが、ABI 非公開のため target が下げられない場合は 16 align fallback を許す。
- `llvm_frame.rs::stack_size` と `layout_size` を一致させる。

### 前提とする他チケットのインターフェース

- E02: `std/Simd.tz` を標準モジュールとして予約し、組み込み wrapper を置ける。
- F03: wasm32 で実 v128 を要求する場合は `--wasm-feature simd128` を使う。F03 が未完なら wasm32 は scalar fallback のみを合格条件にする。
- C03: `Simd.load` の `&[T]` はスライス仕様に従う。C03 未完なら配列全体の共有参照だけを対象にして Phase 1 を狭める。

### 他チケットへの提供インターフェース

- F05 は `Simd` の std kernels を ISA multiversioning の対象にできる。
- F02 は `Parallel.sum` の chunk 内処理を vector 化する候補として `Simd.load` / `sum_lanes` を使える。
- F07 は GPU kernel subset で SIMD 型を直接サポートしない。GPU lane 並列とは別物として扱う。

## 設計

### フロントエンド

- `src/check.rs::Type` に `Simd(SimdType)` と `Mask(SimdMask)` を追加する。
- `SimdType` は `element: SimdElement`, `lanes: u8`, `signed: Option<bool>` を持つ。
- `SimdElement` は `I8`, `I16`, `I32`, `I64`, `F32`, `F64`。
- 型名解決は `src/numeric.rs::primitive` に追加するか、新 helper `simd::primitive` を作る。
- `Type::display`, `is_copy`, `needs_drop`, `contains_reference`, `can_capture`, `can_send`, `exportable` を更新する。
- `polymorph.rs` の `map_type`, `substitute`, `bounded_type`, `Inference::resolve`, `Inference::unify`, `variables`, `type_expression` を更新する。
- `layout_size`, `validate_size`, `llvm_frame.rs::stack_size` を更新する。
- `Classes::intrinsic` は Phase 1 では arithmetic class だけを追加し、Eq/Ord は `Simd.*` 関数にする。

### std / builtin

- D-07 に従い `Simd` モジュールに組み込み関数を置く。
- `Builtin` は複数引数・多相制約を扱えるよう E02 の方式に合わせる。
- `Simd.splat`, `of_lanes`, `extract`, `replace`, `load`, `sum_lanes`, `select`, comparison functions を追加する。
- `of_lanes` は名前衝突を避けるため `Simd.of_lanes4`, `of_lanes2`, `of_lanes8`, `of_lanes16` のように lane 数を含めてもよい。
- 既定案: `Simd.of_lanes` は overload なしでは表現しにくいため、Phase 1 は `Simd.splat` と `Simd.load` を必須、`of_lanesN` を提供する。

### codegen

- `llvm.rs::llvm_type` に vector 型を追加する。
- `FunctionEmitter::binary` で SIMD arithmetic を処理する。
- `FunctionEmitter::emit_builtin` または専用 helper で `Simd.*` を処理する。
- vector constants は `zeroinitializer` と `insertelement` で作る。
- mask select は LLVM `select <N x i1>` を使う。
- load は bounds check 後、lane ごとの scalar load + insertelement で始める。
- target が安全に vector load を扱えることを確認した後、連続 load `<N x T>` に最適化してよい。
- scalar load + insertelement は fallback correctness を優先する Phase 1 実装。
- store は Phase 1 optional。入れる場合は lane ごとの scalar store。

## 実装手順

1. **型だけを追加する。**
   - `Type` と型名解決、display、layout を実装する。
   - 確認: `tests/simd.rs` で `let x: f32x4 = ...` の型名が認識され、未実装構築は安定エラー。

2. **`Simd.splat` と `Simd.extract` を実装する。**
   - LLVM vector を出力する。
   - lane bounds check を入れる。
   - 確認: native/WASM `--emit llvm` の deterministic IR。

3. **arithmetic と comparison を実装する。**
   - Add/Sub/Mul/float Div、Bits、Neg、`Simd.eq` などを追加する。
   - 確認: NaN、符号付きゼロ、integer wrap、mask select の参照テスト。

4. **`Simd.load` と `sum_lanes` を実装する。**
   - bounds check を要素 load 前に行う。
   - horizontal sum は lane order を固定する。
   - 確認: 範囲外 trap、empty/small arrays、WASM fallback。

5. **F03 連携を検証する。**
   - wasm32 feature なしで動く。
   - `--wasm-feature simd128` で v128 が現れることを検査する。
   - 確認: `tests/simd.mjs` の native/WASM `-O0` / `-O3`。

6. **docs と benchmarks を追加する。**
   - vector 型一覧、layout、fallback、WASM feature を文書化する。
   - `docs/benchmarks.md` に matched scalar/SIMD workload を追加する。

## テスト計画

- Rust test: 型名、layout size、exportable false、Copy/Send/Capture を確認する。
- Rust test: `llvm::emit` が `<4 x float>` などを含むことを確認する。
- Rust test: intrinsic 宣言が deterministic で重複しないことを確認する。
- E2E: native `-O0` / `-O3` で arithmetic checksum を検査する。
- E2E: wasm32 `-O0` / `-O3` feature なしで同じ checksum。
- E2E: wasm32 `--wasm-feature simd128 -O3` で同じ checksum。
- E2E: `Simd.extract` / `load` の範囲外 trap を子プロセス / isolated WASM instance で確認する。
- E2E: f32/f64 の NaN、+0、-0 を lane ごとに検査する。
- E2E: integer wrap を lane ごとに検査する。
- benchmark: scalar loop、explicit `Simd.load` loop、C/C++ intrinsic なしの matched baseline を比較する。
- 速度 threshold は CI に入れない。

## ドキュメント

- `docs/language.md` に SIMD 型一覧、演算、mask、layout、ABI 非公開を書く。
- `docs/architecture.md` に LLVM vector lowering と fallback 方針を書く。
- `README.md` に小さい例を追加する。
- `docs/benchmarks.md` に測定条件と限界を書く。
- F03 docs から `--wasm-feature simd128` を cross-link する。

## 受け入れ条件

- [ ] 128-bit SIMD 型が型検査できる。
- [ ] LLVM IR が vector 型を使う。
- [ ] public ABI に SIMD 型を出せない。
- [ ] layout size が 16 bytes。
- [ ] `Simd.splat`, `extract`, arithmetic, mask comparison, `select`, `load`, `sum_lanes` が動く。
- [ ] bounds check は要素 access 前に trap する。
- [ ] float に fast-math flag が付いていない。
- [ ] integer overflow flags が付いていない。
- [ ] native generic/native と wasm feature なし/ありの正しさを検査している。
- [ ] docs が実装済みと計画を分けている。

## 落とし穴

- mask を `bool` vector ではなく scalar bool と混同しない。
- `Eq` / `Ord` の既存シグネチャは bool 返却なので、vector mask 返却にそのまま使えない。
- wasm32 without simd128 で v128 を必須にすると D-18 に違反する。
- lane load の bounds check を lane ごと load の後に置くと未定義 access になる。
- integer vector division を guard なしで LLVM に渡すと UB / poison の危険がある。
- f32/f64 horizontal sum を tree reduction に変えると D-14 に反する。

## 対象外

- 256-bit 型の実装。
- relaxed SIMD。
- gather/scatter。
- saturating arithmetic。
- horizontal min/max。
- public C/WASM ABI。
- 自動 vectorization の保証。

## 未決事項

- 256-bit 型を同時に入れるか。
  - 既定案: Phase 1 は 128-bit のみ。256-bit は F05 の runtime dispatch と合わせて再判断する。
- `Simd.store` を Phase 1 に含めるか。
  - 既定案: mutable slice の言語仕様が確定するまで対象外。
- Eq/Ord typeclass instance を拡張するか。
  - 既定案: Phase 1 は `Simd.eq` 等の明示関数にする。
- 台帳の見直し提案: なし。D-07, D-14, D-18 と整合する。
