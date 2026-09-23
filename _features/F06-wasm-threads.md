# F06: WASM threads バックエンド
| 項目 | 内容 |
|---|---|
| ID | F06 |
| 優先度 | P3 |
| 規模 | L |
| 依存 | F01 |
| 後続 | なし |
| 状態 | todo |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/llvm.rs`, `src/runtime/task-wasm.ll`, `src/runtime/task-wasm-threads.ll`（新規）, `src/runtime/heap-wasm.ll`, `tests/wasm_threads.mjs`（新規）, `examples/web/*`, `docs/language.md`, `docs/architecture.md`, `README.md`, `docs/benchmarks.md` |

## 目的

- wasm32 でも `Task.parallel` / F02 の chunk 実行を複数 Worker へ分散できる opt-in backend を設計する。
- D-18 に従い、既定 wasm32 は import なし・逐次・SIMD なしのまま維持する。
- shared memory、atomics、bulk memory、host glue の要件を明示し、未対応 host では明示エラーにする。
- F01 の `tsuzuri_task_parallel` callback ABI と group semantics を WASM に写す。

## 現状

- `src/runtime/task-wasm.ll` は import なしの逐次 `@tsuzuri_task_parallel`。
- `src/runtime/heap-wasm.ll` は `@tz.heap.end` と `@tz.heap.free` の global free list を持つが、atomic / lock はない。
- `src/driver.rs::build` は wasm32 clang に `--target=wasm32-unknown-unknown` と `-mbulk-memory` を渡す。
- `src/driver.rs::build` は `wasm-ld` に `--stack-first`, `-z stack-size=1048576`, `--max-memory=16777216` を渡す。
- 現在の WASM module は host import を必要としない。
- `tests/tasks.mjs` は WASM `Task.parallel` が sequential fallback で同じ結果を返し、imports が空であることを確認する。
- F01 後、native runtime は常駐 pool + group + caller participation になる。

## 仕様

### CLI

- F03 の `--wasm-feature` 構文を拡張し、`--wasm-feature threads` を追加する。
- `threads` は `--target wasm32` かつ `--emit wasm` / `--emit object` でだけ有効。
- `--wasm-feature threads` は明示 host import を要求するため、default output は変えない。
- `--wasm-feature threads` 指定時は `--wasm-feature simd128` と併用可能。
- unsupported toolchain / target / emit combination は `E2000` または `E2002` で失敗し、逐次 fallback に黙って戻さない。

### 実行要件

- Web host は `SharedArrayBuffer` を利用できる必要がある。
- Browser では COOP/COEP による cross-origin isolation が必要。
- Node.js では `worker_threads` と shared WebAssembly.Memory が必要。
- Module は shared memory を import または define し、最大 memory を明示する。
- Atomics と bulk-memory が必要。
- Host glue は Worker を生成し、同じ module と shared memory を各 Worker に instantiate する。
- Host glue は worker ごとに linear-memory stack slice を割り当てる。
- Host glue は worker entry export を呼んで待機 loop に入れる。

### 言語意味

- `Task.parallel` の型・構文・結果順序は変えない。
- group 内実行順は未規定。
- `tsuzuri_task_parallel` は全 callback 完了まで戻らない。
- user-visible detach はない。
- trap 時の cleanup / cancellation は保証しない。
- 正常終了時の所有値回収は既存と同じ。
- explicit threads が要求されたのに host glue がない場合、instantiation または runtime init で error にする。

### 前提とする他チケットのインターフェース

- F01: `tsuzuri_task_parallel` callback ABI と group semantics。
- F03: `--wasm-feature` 拡張の CLI framework。F03 未完なら F06 が同じ構文を先に導入してよいが、重複実装しない。

### 他チケットへの提供インターフェース

- F02 は wasm threads 有効時も同じ chunk callback を使える。
- B06 は group cancellation state を `task-wasm-threads.ll` にも追加できる。
- F07 は WebGPU host glue と Worker host glue を混同しない。

## 設計

### module / memory

- `--wasm-feature threads` では clang に `-matomics -mbulk-memory` を渡す。
- wasm-ld に `--shared-memory` を渡す。
- memory は host から import する方式を Phase 1 の既定にする。
- driver は `--import-memory` と `--max-memory=16777216` を併用する。
- shared memory の initial pages は既存 stack + heap を満たす最小値にする。
- 現在の `--stack-first` 1 MiB stack は main instance 用とし、worker stack は host glue が別 slice を与える。
- 各 Worker は同じ shared memory だが、mutable globals は instance ごとに独立する。
- host glue は worker instance の `__stack_pointer` を stack slice top に初期化する必要がある。
- `__stack_pointer` を直接 export/import できない場合は、linker/export option と runtime helperを検討する。

### task runtime

- `src/runtime/task-wasm-threads.ll` を新規追加する。
- `@tsuzuri_task_parallel` は F01 と同じ group fields を linear memory に置く。
- group は stack ではなく `@tz.alloc` で確保するか、呼び出し元 stack に置き、完了まで lifetime を守る。
- Phase 1 は group を stack に置く。worker は group pointer を共有するが、caller が戻る前に completion を待つ。
- work index は `atomicrmw add`。
- remaining count は `atomicrmw sub`。
- wakeup は wasm atomic wait/notify を使う。
- host worker は runtime queue を poll せず、futex wait で眠る。
- caller participation は必須。
- worker が nested group を作る場合も、join loop で他 group を進める。

### host imports / exports

- Module import namespace は `tsuzuri_threads`。
- Phase 1 imports:
  - `tsuzuri_threads.spawn_workers(i32 desired) -> i32 started`
  - `tsuzuri_threads.worker_ready(i32 worker_id) -> void`
  - 必要なら `tsuzuri_threads.abort(i32 code) -> void`
- Phase 1 exports:
  - `tsuzuri_thread_entry(i32 worker_id) -> void`
  - `tsuzuri_thread_stack_size() -> i32`
  - 通常の `tz_*` exports。
- Host glue は `spawn_workers` で Worker を起動し、各 Worker が `tsuzuri_thread_entry` を呼ぶ。
- `spawn_workers` が desired 未満を返す場合、explicit threads では runtime error にする。
- 自動縮退はしない。

### allocator

- `src/runtime/heap-wasm.ll` は現状 thread-safe ではない。
- threads 有効時は `@tz.alloc` / `@tz.free` を mutex で保護する。
- Phase 1 は global heap lock を使う。
- lock は `i32` atomic compare_exchange で取得する。
- contended path は `memory.atomic.wait32` / `memory.atomic.notify` を使う。
- lock 中に user callback を呼ばない。
- allocator lock の導入で semantics は変えない。
- Phase 2 で per-worker arena を検討するが、Phase 1 の対象外。

### Phase 1 の完全仕様

- Phase 1 の対象は Node.js `worker_threads` の E2E。
- Browser glue は docs と examples skeleton のみ。
- `Task.parallel` だけを threads backend へ接続する。
- F02 `Parallel.*` は有効なら同じ `tsuzuri_task_parallel` 経由で動くが、F06 単独テストは task fixture で行う。
- `spawn_workers` は module instantiation 直後、最初の `tsuzuri_task_parallel` で一回だけ呼ぶ。
- worker 数は `min(navigator.hardwareConcurrency or os.cpus().length, 32) - 1` を host glue が渡す。
- runtime は requested count を超えない。
- worker stack は 256 KiB 以上、最大 31 worker なら 8 MiB 未満に収める。
- 既存 16 MiB memory 上限内で stack + heap + data が収まらない場合は instantiation error。

## 実装手順

1. **CLI と validation を追加する。**
   - `--wasm-feature threads` を受理する。
   - native/check/run/header で拒否する。
   - 確認: parser tests。

2. **link flags を試作する。**
   - clang `-matomics -mbulk-memory`。
   - wasm-ld `--shared-memory --import-memory --max-memory=16777216`。
   - 確認: 空に近い export fixture が Node で shared memory import として instantiate できる。

3. **worker stack 初期化の feasibility を検証する。**
   - `__stack_pointer` を worker instance ごとに設定できるか確認する。
   - できない場合は go/no-go で中断する。
   - 確認: worker から alloca を含む Tsuzuri function を呼び、stack が重ならないことを sentinel で検査。

4. **allocator lock を実装する。**
   - threads build 専用の heap runtime を分けるか、`heap-wasm.ll` に feature 分岐を持たせる。
   - 確認: 複数 worker が allocation/free を繰り返して free list が壊れない。

5. **task runtime を実装する。**
   - `task-wasm-threads.ll` を追加する。
   - group index / remaining / wait/notify を実装する。
   - 確認: timing なしの atomic counter / barrier test。

6. **Node host glue test を追加する。**
   - `tests/wasm_threads.mjs` で shared memory と workers を作る。
   - 結果順序、入れ子、trap、imports を検査する。
   - 確認: unsupported host では明示的に skip ではなく expected unsupported diagnostic を出す設計にする。CI で走らせない場合は optional command として docs に置く。

7. **docs を更新する。**
   - Browser の COOP/COEP と Node worker_threads 要件を書く。
   - default は変わらないと明記する。

## go/no-go 基準

- Go: shared memory import + worker per-instance stack が Node 20 以上で安定して動く。
- Go: `@tz.alloc` / `@tz.free` の global lock で data race なく heap reuse test が通る。
- Go: `Task.parallel` nested fixture が timeout なしの barrier で完了する。
- No-go: `__stack_pointer` を安全に worker ごとに分離できない。
- No-go: wasm-ld / clang の freestanding shared-memory link が macOS/Linux CI toolchain で再現しない。
- No-go: host imports が default wasm path に漏れる。
- No-go の場合は F06 を「host glue/runtime ABI 設計」チケットに分割し、default backend は逐次のまま維持する。

## テスト計画

- Parser/driver tests: `--wasm-feature threads` の受理・拒否。
- Node E2E optional: SharedArrayBuffer が使える場合にだけ実行。
- E2E: imports に `tsuzuri_threads.*` と memory import があることを確認する。
- E2E: feature なし build は imports 空のまま。
- E2E: parallel sum/order fixture の checksum。
- E2E: nested group fixture。
- E2E: allocation/free stress。
- E2E: trap は RuntimeError または worker termination として検出する。
- E2E: worker count は host requested count を超えない。
- timing は合否条件にしない。

## ドキュメント

- `docs/language.md` の WASM backend。
- `docs/architecture.md` の WASM / task runtime。
- `README.md` の CLI と Web host 注意。
- `docs/benchmarks.md` に WASM threads 測定条件。wall time と worker startup を含める。
- `examples/web` に COOP/COEP が必要な注記。

## 受け入れ条件

- [ ] default wasm32 output は imports 空・逐次のまま。
- [ ] `--wasm-feature threads` は explicit opt-in。
- [ ] unsupported host / toolchain は明示エラー。
- [ ] shared memory と atomics を使う。
- [ ] `heap-wasm.ll` 相当の allocator が thread-safe。
- [ ] worker ごとに stack が分離される。
- [ ] `tsuzuri_task_parallel` は全 callback 完了まで戻らない。
- [ ] caller participation と nested group が動く。
- [ ] docs に COOP/COEP / SharedArrayBuffer / Worker glue が書かれている。

## 落とし穴

- default wasm に imports を増やすと D-18 違反。
- shared memory で既存 allocator をそのまま使うと free list が壊れる。
- worker instance の `__stack_pointer` を共有すると stack が衝突する。
- host が worker を起動できないのに逐次成功へ黙って fallback してはいけない。
- CPU time と wall time を混ぜて speedup を主張してはいけない。

## 対象外

- Browser production glue の完全実装。
- Cancellation / failure propagation。
- Work-stealing 最適化。
- Per-worker allocator。
- WASI threads。
- GPU。
- speed threshold CI。

## 未決事項

- memory を import するか module 内 define shared memory にするか。
  - 既定案: host Worker が共有するため import memory。
- `spawn_workers` を import にするか、host が先に workers を起動するだけにするか。
  - 既定案: explicit failure を返せる import にする。
- stack size を 256 KiB に固定するか CLI 化するか。
  - 既定案: Phase 1 は固定。CLI は対象外。
- 台帳の見直し提案: なし。D-18 と整合する。
