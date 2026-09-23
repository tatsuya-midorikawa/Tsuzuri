# F07: GPU バックエンド
| 項目 | 内容 |
|---|---|
| ID | F07 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | F02, E05, B01（`Gpu.request` が `Result` を返すため） |
| 後続 | なし |
| 状態 | todo |
| 主な影響ファイル | `std/Gpu.tz`（新規）, `src/gpu.rs`（新規）, `src/check.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/driver.rs`, `src/main.rs`, `src/runtime/gpu-*`（新規候補）, `tests/gpu.rs`（新規）, `tests/gpu.mjs`（新規）, `benchmarks/run-gpu.mjs`（新規）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md` |

## 目的

- 連続データの `init` / `map` を GPU に offload できる長期設計を固める。
- F02 の `Parallel.init` / `Parallel.map` と E05 の buffer ABI を土台に、CPU fallback と同じ意味を保つ。
- 自動選択は measured end-to-end cost に基づく場合だけ許可し、明示要求された GPU が使えない場合は error にする。
- GPU 特有の float contraction、reassociation、非決定的 reduction を既存 API に混ぜない。
- GPU なしの CI でも kernel 抽出・CPU reference・診断を検査できる段階的な計画にする。

## 現状

- `docs/architecture.md` は GPU backend を未実装とし、能力検出・所有権を保つ buffer・転送・同期・CPU 経路を設計する必要があると書いている。
- `README.md` も GPU backend と自動マルチスレッド化を未実装としている。
- D-07 は `Gpu` モジュール名を標準ライブラリとして予約している。
- F02 は deterministic chunking の `Parallel.*` を提供する予定。
- E05 は host ABI buffer / scalar record を拡張する予定。
- 現在の Tsuzuri 公開 ABI は 64-bit 以下の scalar と f32/f64/bool/unit に限定される。
- `src/llvm.rs` は native / wasm32 の LLVM IR 生成だけを持つ。
- 既存 tests は GPU runner を前提にしていない。

## 仕様

### 対象 backend 候補

- Phase 1 優先候補は WebGPU / WGSL。
- 理由: Web と native host の両方で同じ shader 言語に近く、GPU がない CI で shader text validation を分離しやすい。
- Phase 2 候補は SPIR-V / Vulkan。
- Phase 3 候補は NVPTX / CUDA。
- Metal は SPIR-V Cross / MSL 経由を候補にするが、Phase 1 では直接対象外。
- LLVM SPIR-V backend の成熟度は go/no-go で評価する。
- backend 名は `Gpu.Backend.webgpu`, `Gpu.Backend.vulkan`, `Gpu.Backend.cuda`, `Gpu.Backend.metal`, `Gpu.Backend.auto`。

### API

- D-07 に従い `Gpu` モジュールに置く。
- Phase 1 の利用者 API 既定案:
  - `Gpu.request : Gpu.Backend -> Gpu.Device`
  - `Gpu.init : Gpu.Device -> i64 -> (i64 -> 'a) -> Gpu.Buffer 'a`
  - `Gpu.map : Gpu.Device -> ('a -> 'b) -> Gpu.Buffer 'a -> Gpu.Buffer 'b`
  - `Gpu.from_array : Gpu.Device -> &['a] -> Gpu.Buffer 'a`
  - `Gpu.to_array : Gpu.Buffer 'a -> ['a]`
  - `Gpu.release : Gpu.Buffer 'a -> unit` は ownership と衝突するため導入しない。
- `Gpu.Buffer 'a` は所有値。drop 時に device buffer を解放する。
- `Gpu.Device` は opaque handle。Copy ではない。
- `Gpu.request` は明示要求 backend が利用不可なら error。
- B01 が完了している場合の望ましい API:
  - `Gpu.request : Gpu.Backend -> Result Gpu.Device Gpu.Error`
- B01 が未完の Phase 1 では、利用不可を runtime trap / host diagnostic として扱う。
- `Gpu.Backend.auto` は CPU fallback を含む自動選択ではなく、利用可能 GPU backend の選択だけを意味する。
- CPU fallback を自動で選ぶ API は Phase 1 では提供しない。
- 自動 CPU/GPU 選択は measured thresholds が揃った後に別 API `Gpu.auto_map` などで検討する。

### kernel subset

- `Gpu.init` / `Gpu.map` の関数は GPU kernel subset に制限する。
- 許可する型は bool、i32/i64/i32u/i64u、f32/f64、固定サイズ scalar record、tuple、array element。
- Phase 1 は i32/i64/f32/f64 の scalar だけ。
- 関数値、closure capture、string、list、task、allocation、recursion、match with heap payload、host call は禁止。
- Capture は Copy scalar / small scalar record のみ。
- Borrow は input buffer / scalar capture の読み取りだけ。
- Mutable state は kernel local scalar だけ。
- `assert` は kernel 内では Phase 1 禁止。境界検査は host side / generated guard で行う。
- ループは affine index loop に限定し、Phase 1 では element function 本体に loop を許可しない。
- trap の代わりに invalid kernel は compile-time `E1005` / `E1018` 系で拒否する。

### determinism / float

- GPU kernel は CPU reference と bit-identical を目標にする。
- f32/f64 は fast-math 無効。
- FMA contraction は無効。
- reassociation は無効。
- denormal flush-to-zero を強制する backend は strict API では使わない。
- GPU backend が strict float を保証できない場合、その backend は `Gpu.request` で unavailable とする。
- reductions は Phase 1 対象外。
- 将来の GPU reduction は F02 の deterministic chunk tree または別名 API に限定する。

### data residency

- `Gpu.Buffer 'a` は device-resident。
- `Gpu.from_array` は host array から device buffer へ転送する。
- `Gpu.to_array` は device buffer から host array へ同期転送する。
- `Gpu.map` / `Gpu.init` は device buffer を返し、host へ戻さない。
- 連続した GPU pipeline では `to_array` まで host transfer を避ける。
- E05 の buffer ABI を host glue が使い、Tsuzuri heap pointer を GPU に直接公開しない。
- Device unavailable / allocation failure / shader compilation failure は明示 error。

### 前提とする他チケットのインターフェース

- F02: `Parallel.init` / `Parallel.map` の element function 制約と CPU reference を再利用する。
- E05: host ABI buffer で byte span / scalar record を安全に渡す。
- B01: D-10 に従う `Result` 型が利用可能なら `Gpu.request` の失敗表現に使う。未完なら Phase 1 API は trap-based で始め、B01 後に移行する。

### 他チケットへの提供インターフェース

- F02 は `Parallel.*` の将来 backend として `Gpu` を参照できるが、暗黙 offload はしない。
- F05 は CPU fallback variant と GPU backend の benchmark 比較に同じ JSON schema を使える。
- E05 は `Gpu.Buffer` の host layout を公開 ABI として安定化する前に F07 の要件を確認する。

## 設計

### compiler IR

- `src/gpu.rs` を新設する。
- `GpuKernel` は `name`, `parameters`, `captures`, `result`, `body`, `work_items` を持つ。
- `GpuType` は scalar と buffer element type を持つ。
- `GpuOp` は Phase 1 で arithmetic, comparison, select, cast, load input, store output だけ。
- `check.rs` から typed expression を渡し、`gpu::extract_kernel` が subset を検査する。
- subset 失敗は stable diagnostic にする。新診断コード未割り当てなので、既存 `E1005` / `E1018` を使う。
- kernel extraction は deterministic に `BTreeMap` / `BTreeSet` を使う。

### WGSL codegen Phase 1

- `Gpu.init device n f` は WGSL compute shader `@compute @workgroup_size(256)` を生成する。
- Global invocation id `i` が `n` 未満なら `output[i] = f(i)`。
- `Gpu.map device f buffer` は `input[i]` を読み `output[i] = f(input[i])`。
- Workgroup size は 256 固定。性能 threshold ではなく code shape の初期値。
- 256 が不適切な device は host glue が pipeline creation で error にする。自動調整は Phase 2。
- WGSL は strict float のため `@diagnostic` や backend option で contraction 無効を確認する。WebGPU 仕様上の挙動を調査し docs に記録する。
- WGSL validation は optional tool ではなく、host test で WebGPU adapter がある場合に行う。
- GPU なし CI では WGSL text snapshot と CPU reference を検査する。

### host runtime

- Phase 1 は compiler 本体に GPU driver dependency を入れない。
- 生成物は host glue に kernel metadata を渡す。
- Native object に GPU runtime を自動 link しない。
- `Gpu` API を使う program は host runtime が必要なので、default CLI `run` では Phase 1 対象外にしてもよい。
- `tsuzuri build --emit gpu-ir` のような新 emit は Phase 1 の go/no-go 後に決める。
- 既定案: 最初は `--emit llvm` と別に sidecar `*.gpu.json` を出す build option を検討するが、このチケット内では設計まで。

### CPU reference

- 全 `Gpu.init/map` は CPU reference lowering を持つ。
- GPU が明示要求されていない test では CPU reference で正しさを確認する。
- CPU reference は F02 の sequential chunk order を使う。
- GPU result は `Gpu.to_array` 後に CPU reference と照合できる。

### Phase 1 の完全仕様

- Phase 1 の実装対象は compiler-internal kernel extraction + CPU reference + WGSL text generation。
- 実 GPU 実行は optional runner。
- `Gpu.request` は GPU runtime が link されていない `tsuzuri run` では `E2004` または runtime diagnostic にする。
- `tests/gpu.rs` は GPU subset の accept/reject と WGSL deterministic snapshot を検査する。
- `tests/gpu.mjs` は GPU が使える環境変数 `TSUZURI_WEBGPU=1` のときだけ optional に実行する。
- CI 必須は GPU なしで通る。

## 実装手順

1. **`Gpu` 型と API 名だけを予約する。**
   - E02 の std module に `Gpu` を追加する。
   - `Gpu.Buffer` / `Gpu.Device` opaque 型を compiler internal として扱う設計を入れる。
   - 確認: ユーザーモジュール `Gpu.tz` が `E1011`。

2. **kernel subset checker を実装する。**
   - `src/gpu.rs::extract_kernel` を追加する。
   - `Gpu.init/map` の lambda / known function だけを受け付ける。
   - 不許可構文を安定エラーで拒否する。
   - 確認: `tests/gpu.rs` の拒否 matrix。

3. **CPU reference lowering を実装する。**
   - GPU runtime なしでも `Gpu.init/map |> Gpu.to_array` 相当を CPU で検査できる内部 path を作る。
   - 確認: native/WASM の scalar checksum。

4. **WGSL generation を追加する。**
   - `GpuKernel` から deterministic WGSL を出力する。
   - arithmetic/cast/select をサポートする。
   - 確認: snapshot test と text validation optional。

5. **host glue prototype を作る。**
   - Node/WebGPU または browser example を `examples/gpu` に置く。
   - E05 buffer ABI を使い、host array を GPU buffer に転送する。
   - 確認: optional GPU runner で CPU reference と一致。

6. **benchmark skeleton を追加する。**
   - `benchmarks/run-gpu.mjs` は CPU reference、GPU transfer included、GPU resident chain を分けて記録する。
   - quick mode は GPU なしで CPU reference だけ。

## go/no-go 基準

- Go: kernel subset checker が F02 の代表的 `init/map` を拒否しすぎず、GPU 禁止構文を安定診断で拒否できる。
- Go: WGSL generation が deterministic で、optional WebGPU runner で CPU reference と一致する。
- Go: transfer included workload と resident workload を分けて測定できる。
- Go: strict float 条件を WebGPU backend で文書化できる。
- No-go: strict float / no contraction を backend が保証できない。
- No-go: E05 buffer ABI なしでは安全な data transfer を表現できない。
- No-go: GPU なし CI で meaningful な compiler test が作れない。
- No-go の場合、F07 は `Gpu` API 実装ではなく kernel extraction / host ABI 調査チケットに分割する。

## テスト計画

- Rust test: kernel subset accept。
- Rust test: allocation/string/closure/task/recursion を拒否。
- Rust test: generated WGSL が deterministic。
- Rust test: CPU reference と Tsuzuri result が一致。
- Node optional: WebGPU adapter がある場合に `Gpu.init/map` を実行。
- Node optional: unavailable backend を明示要求して error。
- Node optional: transfer included time と resident chain time を JSON に記録。
- No GPU CI: optional test は既定で実行しないが、skip success にしない。別コマンドとして docs に書く。
- 浮動小数点: NaN、signed zero、丸め差を CPU reference と照合。
- speed threshold は CI に入れない。

## ドキュメント

- `docs/language.md` に `Gpu` API の実装済み範囲と制限。
- `docs/architecture.md` に backend candidates、kernel subset、data residency、fallback。
- `README.md` に experimental / optional の表示。
- `docs/benchmarks.md` に transfer included と resident workload の分離。
- F02 docs から「暗黙 GPU offload はしない」と cross-link。

## 受け入れ条件

- [ ] `Gpu` は標準モジュールとして予約される。
- [ ] `Gpu.init/map` の Phase 1 subset が明確に検査される。
- [ ] CPU reference path がある。
- [ ] WGSL output が deterministic。
- [ ] GPU unavailable は明示 error。
- [ ] 自動 CPU fallback は explicit request では行わない。
- [ ] float contraction / fast-math を使わない。
- [ ] GPU なし CI で subset / WGSL / CPU reference を検査できる。
- [ ] benchmarks は transfer / dispatch / synchronization / startup を含めて記録する。
- [ ] docs が実装済みと計画を分けている。

## 落とし穴

- GPU を使えば速いと仮定しない。転送・dispatch・pipeline creation を含める。
- explicit GPU request が失敗したとき CPU 成功に黙って変えてはいけない。
- WebGPU / CUDA / Vulkan の float semantics 差を隠してはいけない。
- GPU reduction を既存 `sum` に混ぜると D-14 に反する。
- Tsuzuri heap pointer を device に直接渡すと ownership / lifetime を壊す。
- GPU なし CI で optional test を skip success として実装完了扱いにしない。

## 対象外

- Phase 1 での reductions。
- Closure / string / allocation を含む kernel。
- Automatic `Parallel.map` offload。
- CUDA / Vulkan / Metal の本実装。
- Persistent GPU cache。
- Dynamic scheduling。
- speed threshold CI。

## 未決事項

- `Gpu.request` の失敗を `Result` にするため B01 を正式依存に追加するか。
  - 既定案: D-10 に従い、利用者向け API 実装時には B01 を依存に追加する。B01 未完の Phase 1 は trap/diagnostic の実験 API に限定する。
- Phase 1 の主 backend を WebGPU/WGSL に固定するか。
  - 既定案: WebGPU/WGSL。Vulkan/CUDA/Metal は設計比較だけ。
- compiler が sidecar GPU IR を出す CLI を追加するか。
  - 既定案: go/no-go 後に別チケット化する。
- 反映済み: `Gpu.request` の利用不可は D-10 に従い `Result` で返すため、B01 を正式な依存に追加した。
