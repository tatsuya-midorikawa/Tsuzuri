# F03: WASM SIMD128
| 項目 | 内容 |
|---|---|
| ID | F03 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | なし |
| 後続 | F04 |
| 状態 | todo |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/llvm.rs`, `tests/e2e.mjs`, `tests/numeric_casts.mjs`, `tests/wasm_simd.mjs`（新規）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md` |

## 目的

- wasm32 出力で SIMD128 を明示的に有効化できるようにする。
- D-18 に従い、既定の wasm32 出力は import なし・逐次・SIMD なしのまま維持する。
- `--cpu native` を wasm32 に流用せず、WASM feature と CPU tuning を分離する。
- relaxed SIMD は非決定性の余地があるため導入しない。
- F04 の明示ベクトル型と、LLVM の自動 vectorization の両方が使える opt-in backend 条件を整える。

## 現状

- `src/main.rs` の HELP は `--cpu generic|native` を native build/run 専用として説明している。
- `src/main.rs` の `parse_arguments` は `--target`, `--emit`, `-O0..-O3`, `--cpu` を解析する。
- `src/main.rs` の `rejects_ambiguous_or_unused_arguments` は wasm32 と `--cpu native` の組み合わせを拒否している。
- `src/driver.rs` の `BuildOptions` は `target`, `emit`, `optimization`, `cpu` を持つ。
- `src/driver.rs` の `BuildOptions::validate` は `--cpu native` を native executable/object 以外で `E2000` にする。
- `src/driver.rs` の `build` は wasm32 clang に `--target=wasm32-unknown-unknown` と `-mbulk-memory` を渡す。
- `src/driver.rs` の `build` は `wasm-ld` に `--no-entry`, `--strip-all`, `--stack-first`, `-z stack-size=1048576`, `--max-memory=16777216` を渡す。
- `src/runtime/task-wasm.ll` は SIMD に依存しない逐次 fallback。
- `tests/*.mjs` は wasm32 module の import が空であることを何度も検査している。
- `README.md` は WASM が bulk-memory 対応の現在のブラウザー／Node.js を対象にすることを説明している。

## 仕様

### CLI

- 新オプション `--wasm-feature FEATURE` を追加する。
- 最初に受理する `FEATURE` は `simd128` のみ。
- 同じ feature の重複指定は `E2000`。
- 未知 feature は `E2000`。
- `--wasm-feature simd128` は `--target wasm32` のときだけ有効。
- `--target native --wasm-feature simd128` は `E2000`。
- `check` に `--wasm-feature` を渡すと `E2000`。
- `run` に `--wasm-feature` を渡すと `E2000`。現状 `run` は native executable のみであるため。
- `--emit header` に `--wasm-feature` を渡すと `E2000`。
- `--emit llvm --target wasm32 --wasm-feature simd128` は受理してよいが、Clang を通らないため feature 有効化は IR コメントと metadata に限定する。
- 既定は feature なし。
- `--cpu` と `--wasm-feature` は別概念。`--cpu native` は wasm32 では引き続き拒否する。

### 意味

- `simd128` 指定時だけ clang に `-msimd128` を渡す。
- 既定出力に `v128` 命令を要求する feature を入れない。
- `relaxed-simd` は受理しない。
- fast-math、reassociation、暗黙 FMA は有効にしない。
- 浮動小数点の NaN、符号付きゼロ、丸め順序は既存仕様を維持する。
- LLVM が integer loop を SIMD 化しても、overflow / trap / bounds check の意味を変えてはならない。
- 明示的に `simd128` を要求した wasm module は SIMD128 をサポートしない engine では validation / instantiation が失敗してよい。
- 明示的に要求していない module は従来どおり SIMD128 非対応 engine でも動作対象に残す。

### 前提とする他チケットのインターフェース

- なし。
- F03 は `--wasm-feature` の CLI と driver 経路を先に作り、F04 が同じ feature を利用する。

### 他チケットへの提供インターフェース

- F04 は `BuildOptions::wasm_features` に `WasmFeature::Simd128` がある場合、wasm32 で LLVM vector lowering の v128 codegen を期待してよい。
- F06 は同じ `--wasm-feature` 構文に `threads` を追加できる。
- F07 の WebGPU/WASM host glue は、SIMD128 の有無を GPU 判定と混同してはならない。

## 設計

### データ構造

- `src/driver.rs` に `pub enum WasmFeature { Simd128 }` を追加する。
- `BuildOptions` に `pub wasm_features: BTreeSet<WasmFeature>` を追加する。
- `Default` は空集合。
- `WasmFeature` は `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`, `PartialOrd`, `Ord` を実装する。
- `BuildOptions::validate` は `wasm_features` が空でない場合、`target == Target::Wasm32` かつ `emit != Emit::Header` を要求する。
- `src/main.rs::parse_arguments` は `--wasm-feature` を複数回受け、重複を拒否する。
- HELP と README の CLI 表に追加する。

### driver

- wasm32 clang invocation に、`WasmFeature::Simd128` があれば `-msimd128` を追加する。
- `-mrelaxed-simd` は追加しない。
- `wasm-ld` には原則 feature flag を追加しない。object の target feature section に従わせる。
- もし LLD の版で明示 feature が必要なら、検出せず `E2002` に失敗させ、docs にツール要件を書く。
- `--emit llvm` の場合、`llvm::emit_target` に feature を渡す拡張を行うか、IR 冒頭に `; wasm-feature: simd128` コメントを出す。
- 既定では `llvm::emit_target` の public API を壊さないため、まず driver 側の clang flag だけで実装する。

### テスト用 binary inspection

- Node 20 は SIMD128 をサポートするため、`WebAssembly.validate` / instantiate を実行できる。
- `tests/wasm_simd.mjs` は vectorizable fixture を `--target wasm32 -O3` で二回ビルドする。
- feature なしの module bytes には SIMD prefix `0xfd` が code section に現れないことを検査する。
- `--wasm-feature simd128` の module bytes には同じ fixture で `0xfd` が現れることを検査する。
- `0xfd` scan は code section に限定する。custom section の文字列と混同しない。
- 期待通り vectorize しない LLVM/Clang 版では、テスト fixture を明示的な F04 vector 型導入後に移す。F03 単独では loop-vectorization の presence test を optional にしてよい。
- F03 単独の必須検査は「feature なし bytes と feature あり bytes が別」「feature なしで `0xfd` がない」「feature ありが Node で validate する」。

## 実装手順

1. **CLI parser を拡張する。**
   - `--wasm-feature` を解析し、`simd128` だけ受理する。
   - 重複・未知・check/run での使用を拒否する。
   - 確認: `src/main.rs` の parser tests に拒否例を追加する。

2. **driver option を拡張する。**
   - `WasmFeature` と `BuildOptions::wasm_features` を追加する。
   - `BuildOptions::validate` に target/emit 制約を入れる。
   - 確認: `src/driver.rs` の option tests に native/header/unknown の拒否を追加する。

3. **clang flag を追加する。**
   - wasm32 clang invocation で `-msimd128` を付与する。
   - 既定 path は変更しない。
   - 確認: `TSUZURI_CLANG` を wrapper にした小テストで flag を検査するか、`tests/wasm_simd.mjs` で生成物差分を検査する。

4. **E2E を追加する。**
   - `/tmp` または `tests/fixtures/wasm-simd` に vectorizable program を作る。
   - feature なし / ありを `-O3` でビルドする。
   - Node で both instantiate し結果が同じことを確認する。
   - feature なしで import なしを確認する。
   - feature ありでも import なしを確認する。

5. **docs を更新する。**
   - README CLI 表に `--wasm-feature simd128` を追加する。
   - `docs/language.md` の WASM backend 節に opt-in と engine 要件を書く。
   - `docs/architecture.md` の WASM 節に default no SIMD と simd128 opt-in を書く。
   - `docs/benchmarks.md` に SIMD を主張するには bytes/IR/実測を併記する規則を書く。

## テスト計画

- `cargo test` では parser/validate のみを検査する。
- Node E2E は `tests/wasm_simd.mjs target/release/tsuzuri` を追加する。
- fixture は integer array transform を使い、JS BigInt 参照で checksum を計算する。
- `--target wasm32 -O0 --wasm-feature simd128` も instantiate して結果一致を確認する。
- `--target wasm32 -O3 --wasm-feature simd128` も instantiate して結果一致を確認する。
- `--target wasm32 -O3` feature なしの bytes に code section SIMD prefix がないことを確認する。
- feature ありは Node の SIMD 対応環境で validate する。
- relaxed-simd 文字列は CLI で `E2000`。
- `--cpu native --target wasm32` は引き続き `E2000`。
- 速度は合否条件にしない。

## ドキュメント

- `README.md` の CLI 表。
- `README.md` の WASM 説明。
- `docs/language.md` の「実行バックエンドと失敗」。
- `docs/architecture.md` の WASM と性能設計。
- `docs/benchmarks.md` の「現実的な次の指標」または WASM SIMD の小節。

## 受け入れ条件

- [ ] 既定の wasm32 build は従来どおり SIMD128 を要求しない。
- [ ] `--wasm-feature simd128` が wasm32 build で受理される。
- [ ] native / check / run / header で不正指定が `E2000`。
- [ ] clang に `-msimd128` が渡る。
- [ ] relaxed-simd を受理しない。
- [ ] feature なし / ありの wasm module は同じ結果を返す。
- [ ] feature なし module は import なしで、SIMD prefix を含まない。
- [ ] feature あり module も import なし。
- [ ] docs が engine requirement と default unchanged を明記する。

## 落とし穴

- `--cpu native` を wasm SIMD の意味に転用しない。
- LLVM が自動 vectorize しないことを機能失敗と誤判定しない。
- `0xfd` を custom section 文字列から検出しない。
- relaxed-simd を将来 planning として書く場合も、実装済みと書かない。
- SIMD128 を有効にした結果を、すべての処理の高速化保証として扱わない。
- fast-math を付けると浮動小数点仕様を壊す。

## 対象外

- F04 の明示 SIMD 型。
- relaxed-simd。
- WASM threads。
- CPU feature auto selection。
- native SIMD の CLI。
- 速度 threshold の CI 追加。

## 未決事項

- `--emit llvm --target wasm32 --wasm-feature simd128` を受理するか拒否するか。
  - 既定案: 受理するが、実際の feature 反映は object/wasm build 時の clang flag と docs に明記する。
- code section parser を独自実装するか、LLVM tool に依存するか。
  - 既定案: Node だけで完結する最小 parser を tests に書く。
- 台帳の見直し提案: なし。D-18 と整合する。
