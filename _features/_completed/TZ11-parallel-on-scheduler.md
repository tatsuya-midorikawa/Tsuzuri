# TZ11: `Parallel` と `Routine.scope` を tzroutine のスケジューラーへ移す

| 項目 | 内容 |
| --- | --- |
| ID | TZ11 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | TZ06 |
| 後続 | TZ12（`task.c` のプールを消す前提） |
| 状態 | done（2026-11-03。native の macOS・Linux・Windows。下の「実装と検証」） |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61）。決定事項 D1〜D5 は既定案 |
| 手本にする既存実装 | `src/llvm_parallel.rs` の `parallel_launch`（97〜238 行。文脈を呼び出し元の `alloca` に置き、`@tsuzuri_task_parallel(ptr cb, ptr ctx, i64 chunks)` を呼ぶ）・`parallel_scope_child`（283 行）・`Task.scope` の生成（579〜641 行）、`src/runtime/task.c` の `tz_task_submit`（421 行。呼んだスレッドも仕事をする）、`src/check.rs` の `Builtin::TaskScope`（1604 行・`sync_scheme` 3492 行）と `is_parallel`・`parallel_callback`（3429〜3448 行）、`src/ownership.rs` の `E::Parallel` の腕（3106〜3172 行） |
| 主な影響ファイル | `src/runtime/tzroutine.c`・`src/llvm_parallel.rs`・`src/llvm.rs`・`src/check.rs`・`src/ownership.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/kg_capture.rs`・`src/diagram/concurrency.rs`・`tests/tzroutine_runtime.c`・`tests/parallel_scheduler.rs`（新規）・`tests/fixtures/parallel_scheduler/Main.tz`（新規）・`tests/tzroutine.mjs`・`tests/features.mjs`・文書 |

## 着手時の再検討（2026-11-03、TZ07 の上）

TZ06・TZ07 の実装と照合し、次のとおり直してから実装した。以下が本文より優先する。

1. **トラップの境界の中の `Parallel` はプールに残す**: トラップの枠（`src/runtime/trap.c` の `tz_frame`）はスレッドのもの（`_Thread_local`）。`Domain.run`（純粋な本体）の中の `Parallel.*` が tzroutine の上で待つと、その M で走るほかの G（手伝いを含む）が同じ枠を見て、トラップで別のスタックへ `longjmp` しうる。プールは項目ごとに境界の下で走らせ、項目のトラップを呼び出し元で投げ直す（B10）。そこで `tsuzuri_tz_parallel` は、呼んだスレッドがトラップの境界の中なら（`tsuzuri_trap_hooks.owner()` が `NULL` でない。`task.c` と同じ弱いフック）何もせず 3 を返し、生成コードはそのときだけ今の `tsuzuri_task_parallel` を呼ぶ（`with_blocking_calls`・`with_task_accounting` もそのまま掛かる）。境界の外では、ticket の設計どおり呼んだ G も待ち（parking lot）、手伝いの G の枠は常に空。テストとベンチマークの実行器は境界を使わない（子のプロセス）ので、境界は `Domain.run` だけ（`--trap-mode return` は tzroutine のプログラムでは `E2000`）。TZ12 でプールを消すときは、枠を G ごとにするか、同じ判断をし直す。
2. **WebAssembly は対象外**: 分岐は `globals.tzroutine && !globals.wasm` のときだけ（既定の WebAssembly の tzroutine のプログラムは `TZR_SINGLE` で、`tsuzuri_tz_parallel` を持たない）。
3. **`Routine.scope` のスコープは境界でない**: `tsuzuri_tz_scope_run` は、呼んだ G のスコープの `boundary` を受け継ぐ非境界のスコープを作る（`tsuzuri_tz_scope_enter` は境界を作るので使わない）。子が作るタイマーは呼び出し元の境界のもの。子と、子が起動した tzroutine を待つ。手伝いの tzroutine（`Parallel`）は呼んだ G のスコープで数える。
4. **`Routine.scope` の呼び出しは並列の操作の式**: 完全に適用した呼び出しは `TypedExprKind::Parallel` なので、`llvm::spawns`・`uses_tzroutine`・`waits` は `Function` の参照に加えて `Parallel(op, _)` の `op.spawns()` を見る。`Builtin::is_routine` は `Routine.scope` を除く（`Routine` の生成へ行かない）。`Builtin::is_scope`（`Task.scope`・`Routine.scope`）を足した。

## 実装者向け早見

- 範囲: tzroutine を使うプログラムで、`Parallel.init`・`map`・`map_ref`・`reduce`・`for_each_chunk`（と `Parallel.sum`・`fold`）を `task.c` のプールではなく tzroutine の上で走らせる（1 つのプロセスに 2 つのスレッドの集まりを持たない）。`Task.scope` の後継の `Routine.scope` を足す（子ごとに 1 つの tzroutine。子は待ってよい）。tzroutine を使わないプログラムは TZ12 まで今のまま。
- 最初に読む: ① 仕様 1・2 ② `src/llvm_parallel.rs` 97〜305 行と 579〜641 行 ③ TZ01 の parking lot ④ TZ02 の `uses_tzroutine`（TZ06 の設計）。
- 最初に書くテスト: `tests/tzroutine_runtime.c` の `parallel_claims`（C だけで、`tsuzuri_tz_parallel` が全添字を 1 回ずつ呼ぶ）。
- 触ってはいけない: `Parallel.*` の結果（添字の順、F15 の逐次と同じ還元）。tzroutine を使わないプログラムの IR。`Task.scope` のすべて（TZ12 まで）。
- 完了の見分け方: `features.mjs` の `parallel`・`parallel_equivalence`・`monoid` の suite を、tzroutine を使う形（fixture に `Routine.yield ()` を 1 つ足した写し）でも同じ期待値で通す。`Routine.scope` の子が `Channel` で互いを待っても、`TSUZURI_MAXPROCS=1` で完了する（旧 `Task.scope` は判定のトラップになった）。

## 目的

tzroutine と `Parallel` を同じプログラムで使うと、TZ10 までは `task.c` のプールの OS スレッド（`min(CPU, 32) - 1`）と、tzroutine の M（`TSUZURI_MAXPROCS`）が同時に CPU を取り合う。`Parallel` を tzroutine の P の上で走らせ、データ並列も並行の処理も 1 つのスケジューラーに載せる。`Task.scope`（借用を子に共有する fork／join）の後継を、子が待てる形で用意する（TZ12 の移行先）。

## 着手条件と停止条件

### 着手条件

- TZ06 が done（`uses_tzroutine` がある）。基準: `node tests/features.mjs target/debug/tsuzuri parallel parallel_equivalence monoid pure_parallel matrix`、`cargo test --test parallel --test parallel_report`。

### 停止条件

- **S1** `Parallel.*` の結果が、tzroutine を使う形と使わない形で違う（とくに `reduce`・`fold` の浮動小数点の値。F15 の還元の順序は生成コードが決めるので、変わらないはず）。
- **S2** tzroutine を使わないプログラムの IR が変わる。
- **S3** `Routine.scope` の型と借用の検査を、`Task.scope` の検査（`TaskScopeSharesBorrowedEnvironment` など）の流用で表せない。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認）

- `Parallel.*` は `parallel_expression`（`src/llvm_parallel.rs` 406〜573 行）が、塊（`min(ceil(len / 4096), 1024)` 個）ごとの関数 `@tz.parallel.chunk.<id>(ptr %context, i64 %chunk)` と文脈を作り、`call void @tsuzuri_task_parallel(ptr cb, ptr ctx, i64 chunks)` を呼ぶ。プールは呼んだスレッドも仕事をし、全塊が終わるまで戻らない。
- `Task.scope shared count callback` は `parallel_launch` を `chunks = count` で使い、子 1 つが 1 つの塊（`parallel_scope_child`）。子は `Channel` で待てるが、待つ子の数がスレッド数を超えると完了しない。

## 変えてはいけない意味

- `Parallel.*` の結果（順序・値・トラップ）。F15 の「逐次と同じ結果」。
- 所有権と借用の規則（`Parallel.*` の入力・コールバック・結果、`Task.scope` の `Sync` の共有）。

## 仕様

### 1. `Parallel.*`（tzroutine を使うプログラム）

- 生成コードは `tsuzuri_task_parallel` の代わりに `tsuzuri_tz_parallel(ptr cb, ptr ctx, i64 n)` を呼ぶ（引数と意味は同じ: `cb(ctx, i)` を `i = 0 .. n-1` で 1 回ずつ、呼び出しは全部が終わるまで戻らない）。
- ランタイム: 呼んだ G（または擬似 G）も添字を取って仕事をし、ほかに `min(n, nprocs) - 1` 個（擬似 G から呼んだときは `min(n, nprocs)` 個。擬似 G は P を持たないため）の手伝いの tzroutine を作る。添字は atomic の加算で配る。呼んだ側は、自分の分が終わったら、残りの数が 0 になるまで parking lot で待つ（理由 `Parallel`）。
- 手伝いの tzroutine はスコープで数える（`Parallel` の呼び出しの中で終わる）。`Routine.count` には一時的に現れる。

### 2. `Routine.scope`

```text
Routine.scope :: (Sync<'s>, Send<'a>) => ref 's -> i64 -> (ref 's -> i64 -> 'a) -> ['a]
```

- `Task.scope` と同じ型・同じ借用の規則（`shared` は `Sync`、関数値の環境の証明、コールバックは借用した環境を持たない、結果は `Send` の所有値、直接の呼び出しだけ）。診断も同じコードと kind。
- 子ごとに 1 つの tzroutine（`callback shared index`）。子は `Channel`・`select`・同期の型で待ってよい（待つ子の数に上限はない）。全員が終わるまで戻らない（呼んだ側の待ちの理由 `Routine.scope`）。結果は `index` の順。
- `count == 0` は空の配列。負は、結果の確保でトラップ（`allocation size overflow`。`Task.scope` と同じ）。
- `Routine.scope` は tzroutine を使うプログラムを作る（`Builtin::spawns()` が真）。

## 設計

### ランタイム（`src/runtime/tzroutine.c`）

```c
typedef struct tzr_parallel {
    void (*cb)(void *, uint64_t);
    void *ctx;
    uint64_t n;
    _Atomic uint64_t next;       /* the next index to claim */
    _Atomic uint64_t remaining;  /* workers that have not finished, the caller included */
} tzr_parallel;

static void tzr_parallel_work(tzr_parallel *job) {
    for (uint64_t i; (i = atomic_fetch_add(&job->next, 1)) < job->n;) job->cb(job->ctx, i);
}
static void tzr_parallel_helper(void *arg) {
    tzr_parallel *job = arg;
    tzr_parallel_work(job);
    /* after this the job (on the caller's stack) may be gone: only its address is used */
    if (atomic_fetch_sub(&job->remaining, 1) == 1) tzr_addr_wake(&job->remaining, 1);
}
TZR_API int32_t tsuzuri_tz_parallel(void (*cb)(void *, uint64_t), void *ctx, uint64_t n) {
    if (n == 0) return 0;
    tzr_start();
    tzr_g *g = tzr_current();
    uint64_t procs = (uint64_t)tzr_sched.nprocs;
    uint64_t helpers = (n < procs ? n : procs) - (g->kind == TZR_KIND_ROUTINE ? 1 : 0);
    tzr_parallel job = {cb, ctx, n, 0, helpers + 1};
    for (uint64_t k = 0; k < helpers; k++) tsuzuri_tz_go(tzr_parallel_helper, &job);
    tzr_parallel_work(&job);
    if (atomic_fetch_sub(&job.remaining, 1) != 1) {
        for (uint64_t r; (r = atomic_load(&job.remaining)) != 0;)
            if (tzr_addr_wait(&job.remaining, r, "Parallel")) return 2;
    }
    return 0;
}
```

- `tzr_addr_wake` は `addr` を鍵に使うだけで中身を読まない（手伝いの最後の 1 つが減らした後、呼んだ側がスタックを離れても安全。TZ01 の parking lot の実装を確かめる）。
- `TZR_API int32_t tsuzuri_tz_scope_run(void (*cb)(void *, uint64_t), void *ctx, uint64_t n)`: `scope = tsuzuri_tz_scope_enter()`、`i` ごとに `{ cb, ctx, i }` を `malloc` して `tsuzuri_tz_go(tzr_scope_child, that)`（`tzr_scope_child` は `cb(ctx, i)` と `free`）、`return tsuzuri_tz_scope_leave(scope) ? 2 : 0`。
- 戻り値 2 は、呼んだ擬似 G がデッドロックの判定で起こされたとき（生成コードがトラップ）。

### 生成

- `src/llvm_parallel.rs` の `parallel_launch`: `globals.tzroutine` なら `@tsuzuri_tz_parallel` を呼び、戻り値が 2 ならトラップ（`assert`）。偽なら今のまま（S2）。
- `Builtin::RoutineScope`（`src/check.rs`）: 名前 `"Routine.scope"`、型は `TaskScope` と同じ（`sync_scheme` の同じ腕）。`is_parallel()` と `parallel_callback()` で `TaskScope` と同じ扱い（2 引数のコールバック）。`ownership.rs` の `E::Parallel` の腕の `scope = op == TaskScope` を `matches!(op, TaskScope | RoutineScope)` にする。生成は `Task.scope` の生成（579〜641 行）と同じ子の関数と文脈で、呼ぶ関数だけ `@tsuzuri_tz_scope_run`。
- 効果・事実・図: `RoutineScope` を `TaskScope` と同じ所に（`effects.rs` 561〜563 行、`pure_rules.rs`、`kg/effects.rs` 28 行、`kg_capture.rs` 1058 行、`diagram/concurrency.rs` 100〜108 行）。件数の pin。

## 前提とする他チケットのインターフェース

- TZ01 の `tsuzuri_tz_go`・スコープ・parking lot。TZ06 の `uses_tzroutine`。

## 他チケットへ提供するインターフェース

- `tsuzuri_tz_parallel`（TZ12 が、すべてのネイティブのプログラムの `Parallel.*` をこれにする）、`Routine.scope`（`Task.scope` の移行先）。

## 実装手順

1. **変更**: ブランチ `feat/tz11-parallel-on-scheduler`。基準。
2. **変更**: ランタイムと C のテスト（`parallel_claims`: `n` = 0・1・7・10,000 で全添字が 1 回、P 1・4。`parallel_nested`: コールバックの中の `tsuzuri_tz_parallel`。`scope_run_blocking`: 子が隣の子へチャンネルで送る輪を 64 個、P 1）。**確認**: `TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`。
3. **変更**: `parallel_launch` の分岐。**確認**: tzroutine を使わない `Parallel` の IR が変わらない。
4. **変更**: `Routine.scope`（型・所有権・生成・効果）。**確認**: `tests/parallel_scheduler.rs`（`Task.scope` の受理と拒否の例をすべて `Routine.scope` に写した版が同じコード）。
5. **変更**: E2E（`tests/fixtures/parallel_scheduler`: `parallel` の fixture の写しに `Routine.yield ()` を足したもの、`scope_ring`）。**確認**: `node tests/tzroutine.mjs`、`features.mjs parallel_scheduler`。
6. **変更**: 文書、T2、CI（`ci:full`）。

## テスト計画

- `parallel_scheduler` の suite: `features.mjs` の `parallel` の各ケースの期待値（独立に計算済みの値）を、そのまま使う。`parallel_equivalence` の `reduce` の浮動小数点の値も同じ。
- `scope_ring`: 64 個の子が輪のチャンネルで 1 周ずつ値を回す。`TSUZURI_MAXPROCS=1` でも完了し、結果が `index` の順。
- `scope_shares_borrow`: `Routine.scope (ref numbers) 4 (\shared part -> …)` の部分和（`task.md` の例の写し）。

## ドキュメント

- `built-in-types-and-modules/parallel.md`: 実行の節（tzroutine を使うプログラムでは tzroutine の P の上、使わないプログラムは TZ12 まで従来のプール）。
- `built-in-types-and-modules/routine.md`: `Routine.scope`（`Task.scope` との違い: 子が待てる、スレッド数の制約がない）。
- `docs/language.md`、`docs/architecture.md`。

## 受け入れ条件

- [ ] 仕様 1・2 のとおりに動き、`Parallel.*` の結果が変わらない。
- [ ] tzroutine を使わないプログラムの IR が変わらない。
- [ ] `Routine.scope` の子が待てる（P 1 で `scope_ring` が完了する）。

## 落とし穴

- **呼んだ側のスタックの上の `job`**: 手伝いが `remaining` を減らした後に `job` を読むと、呼んだ側が戻った後の領域を読む。減らす操作を最後にする（参照実装の順序）。
- **擬似 G は P を持たない**: 擬似 G から呼ぶと、手伝いを `nprocs` 個にしないと 1 つ分の CPU が余る。
- **入れ子の `Parallel`**: コールバックの中の `Parallel` は、手伝いの tzroutine を作って自分も働く。デッドロックはしない（呼んだ G が自分の分を進める）。

## 対象外

- 手伝いの tzroutine を作らずに、ほかの P の空きへ仕事を配る仕組み（Go にもない。計測で必要と分かったら別チケット）。
- WASM の `Parallel`（既定は逐次、threads は従来のまま）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 呼び出しごとに手伝いの tzroutine を作る（常駐のワーカーを持たない） | 生成の費用は 1 つ 0.5 µs 程度（TZ01 の付録 A）。P の上で空いた M が拾う | 既定案 |
| D2 | `Parallel` の塊の分け方は変えない | 結果と F15 の保証を変えない | 既定案 |
| D3 | `Routine.scope` は子ごとに 1 つの tzroutine | 子が待てることが目的。塊の取り合いにすると、待つ子が後ろの子を止める | 既定案 |
| D4 | 移すのは tzroutine を使うプログラムだけ（TZ12 で全部） | 既存の性能の記録（`docs/benchmarks.md` の Task プール）を TZ12 まで保つ | 既定案 |
| D5 | `Routine.scope` の型と検査は `Task.scope` と同じ | 検査済みの規則を移す | 既定案 |

## 実装と検証（2026-11-03）

- **範囲**: 仕様 1・2 をすべて実装した（1 つの PR。TZ07 の PR のブランチの上）。着手時の再検討（上の節）のとおり合わせた。
- **ランタイム**（`src/runtime/tzroutine.c` の Parallel と Routine.scope の節）: `tsuzuri_tz_parallel`（呼び出し元のスタックの `tzr_parallel`、手伝いの tzroutine、parking lot の待ち。トラップの境界の中では 3）、`tsuzuri_tz_scope_run`（非境界のスコープに子ごとの tzroutine）。`trap.c` の弱いフック `tsuzuri_trap_hooks` を `task.c` と同じ印で宣言した。
- **コンパイラ**: 組み込み `Routine.scope`（`Builtin::RoutineScope`。型は `Task.scope` と同じ `sync_scheme` の腕、`is_parallel`・`parallel_callback`・`is_scope`、`spawns()` が真、`is_routine` からは外す）。`ownership.rs` の並列の操作の腕は `is_scope`。`llvm::spawns`・`uses_tzroutine`・`waits` は `TypedExprKind::Parallel` も見る。`pure_rules.rs` は `Routine.scope` を純粋でない（`Routine.go` と同じ）とする。`src/llvm_parallel.rs` の `parallel_call` が、`Routine.scope` は `tsuzuri_tz_scope_run`、tzroutine を使うネイティブのプログラムの `Parallel.*` は `tsuzuri_tz_parallel`（3 なら従来の `tsuzuri_task_parallel`）、それ以外と `Task.scope` は従来のまま、を呼ぶ。効果・純粋・KG・図の分類と件数の pin（`Builtin::ALL` 355 → 356、不純 180 → 181、`Task` 18 → 19、`alloc` 78 → 79、`callbacks` 28 → 29、KG 179 → 180）。
- **検証**: 下の PR の本文（`cargo nextest run --locked`、`TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`（新しい C のケース `parallel`・`scope_run`）、`features.mjs parallel_scheduler`（`parallel` の全ケースを同じ期待値で、`scope_ring`・`scope_shares_borrow`・`scope_shares_an_atomic`・`scope_empty`、`parallel` のトラップ。native、`-O0`／`-O3`、P 1・4）と既存の `parallel`、`tests/parallel_scheduler.rs`（`Task.scope` の受理と拒否の例を写した版が同じ結果・同じコードと kind）、`node tests/tzroutine.mjs`（`Domain.run` の中の `Parallel` の値とトラップ、P 1・4。`Routine.scope` の WebAssembly の `E2000`）、変更したページの `check-docs`、`gen-docs --check`、`check-cli-docs`、tzroutine を使わないプログラムの IR が `main` と同じこと）。
- **文書**: `built-in-types-and-modules/routine.md`（`Routine.scope`）・`parallel.md`（どこで走るか）、`docs/language.md`、`docs/architecture.md`、`README.md`。
- **変更したファイル**: `src/runtime/tzroutine.c`、`src/check.rs`・`src/llvm.rs`・`src/llvm_parallel.rs`・`src/ownership.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/diagram/concurrency.rs`、`tests/parallel_scheduler.rs`（新規）・`tests/fixtures/parallel_scheduler/Main.tz`（新規）・`tests/features.mjs`・`tests/tzroutine.mjs`・`tests/tzroutine_runtime.c`・`tests/tzroutine_runtime.mjs`、`.github/workflows/ci.yml`・`targets.yml`、上の文書。
