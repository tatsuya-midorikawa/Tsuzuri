# B06: タスクのキャンセルと失敗の伝播
| 項目 | 内容 |
|---|---|
| ID | B06 |
| 優先度 | P3 |
| 規模 | L |
| 依存 | B01, F01 |
| 後続 | – |
| 状態 | todo |
| 主な影響ファイル | `src/check.rs`, `src/closures.rs`, `src/ownership.rs`, `src/polymorph.rs`, `src/llvm.rs`, `src/runtime/task.c`, `src/runtime/task-wasm.ll`, `src/driver.rs`, `tests/tasks.rs`, `tests/fixtures/tasks/`, `tests/tasks.mjs`, `tests/task_runtime.c`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

`Task.parallel` はすべてのタスクを実行し、trap 以外の失敗概念を持たない。B06 では、例外・共有可変状態・プリエンプティブ中断を導入せず、`Result` を返すタスク配列に対して協調的な「これ以上の新規開始を止める」API を追加する。

提案 API:

```text
Task.parallel_results : [Task<Result<'a, 'e>>] -> Task<Result<['a], 'e>>
```

意味:

- 各タスクは `Result<'a, 'e>` を返す。
- すべて `Ok` なら入力順の `['a]` を `Ok` で返す。
- どれかが `Error e` なら、決定的に選ばれた error を `Error e` で返す。
- error 検出後、まだ開始していない index の新規実行を止め、未開始タスクの捕捉値を drop する。
- すでに開始したタスクは最後まで join する。強制停止・stack unwind・非同期例外はない。

## 現状

- `docs/language.md` の「タスク」は cancellation と回復可能なタスク例外を未提供と説明している。
- `src/check.rs::Builtin` は `TaskRun`, `TaskParallel` を持つ。
  - `Task.parallel : [Task<'a>] -> Task<['a]>`
- `src/closures.rs` は `task { ... }` を clone 不可の `%tz.closure` として下げる。
- `src/ownership.rs` は `Task<T>` を非 Copy とし、二重実行や task の再利用可能 closure 捕捉を拒否する。
- `src/llvm.rs::FunctionEmitter::parallel_tasks`:
  - tasks 配列を評価し、source pointer/length を取り出す。
  - result array を確保する。
  - result 型ごとの callback `@tz.task.item.<ty>` を `intrinsics` 集合に定義する。
  - callback は task closure を読み、code/env を呼び、result slot に store する。
  - `@tsuzuri_task_parallel(callback, context, length)` を呼ぶ。
  - source buffer を `@tz.free` する。
- `src/runtime/task.c`:
  - `tsuzuri_task_parallel(void (*run)(void *, uint64_t), void *context, uint64_t length)`
  - atomic `next` を `fetch_add` し、bounded pthread fork/join。
  - caller thread も work を実行。
  - pthread 失敗は stderr + abort。
- `src/runtime/task-wasm.ll`:
  - import なし。
  - index 0..length-1 を順に callback。
- `tests/tasks.mjs`:
  - native/WASM × `-O0`/`-O3`。
  - `tests/task_runtime.c` で pthread create/join 失敗と bounded parallelism を検査。
  - heap tracking `live == 0`。

### 前提とする他チケットのインターフェース

- **B01**: `Result<'a, 'e> = Ok of 'a | Error of 'e` が std で提供され、union layout/clone/drop は A02 により実装済み。
- **F01**: 常駐ワーカープール後も、index を単調増加で配布し、すべての開始済み work を join してから返る同期 API を提供する。F01 が `tsuzuri_task_parallel` の ABI を変える場合、B06 の runtime API は F01 の scheduler 上に同じ意味で実装する。

### 他チケットへの提供インターフェース

- `Task.parallel_results` は B01 の `Result` を task 失敗伝播の標準 API として使う。
- B05 の `and!` は task を自動並列化しない。複数 task の失敗伝播は `Task.parallel_results` を明示的に使う。

## 仕様

### API

```text
Task.parallel_results : [Task<Result<'a, 'e>>] -> Task<Result<['a], 'e>>
```

例:

```text
def work :: i64 -> Task<Result<i64, string>>
fn work n = task {
    if n < 0 then return Error "negative"
    else return Ok (n * n)
}

let result = Task.run (Task.parallel_results [
    work 2,
    work 3,
    work -1,
    work 4
])

match result with
| Ok values -> values[0] + values[1]
| Error _ -> -1
```

### 成功

- 入力配列が空なら `Ok []`。
- すべての開始済みタスクが `Ok value` で、failure がなければ全 index を実行し、結果 array を入力順に作る。
- 結果 array の要素は各 `Ok` payload から move する。payload が所有値なら独立した所有権を持つ。

### failure / cancellation

- failure は `Task<Result<'a, 'e>>` が `Error error` を正常に返すこと。trap ではない。
- `Error` を返したタスクを検出したら、runtime は **まだ開始していない index >= cancellation_index** の新規開始を止める。
- すでに開始したタスクは最後まで実行し、join する。
- 未開始タスクは実行せず、捕捉値を drop する。
- 返す error は **入力 index が最小の `Error`**。これはスレッドの完了順や CPU 数に依存してはならない。
- 実行順が異なっても、同じ入力で返る `Error` payload は同じ index のもの。
- 複数の error が開始済みなら、最小 index の error payload だけを move して返し、それ以外の error payload は drop する。
- `Error` より小さい index の `Ok` payload は、全体が `Error` を返すため drop する。

### trap / abort

- task body が trap した場合は従来どおりプロセス終了 / WASM trap。`Error` へ変換しない。
- pthread/worker pool の実行基盤失敗は従来どおり明示診断 + abort。成功形の `Error` へ変換しない。
- memory allocation failure は trap。

### WASM

- WASM は import なしの逐次実行。
- index 0 から順に実行し、最初の `Error` で後続 index を開始しない。
- native と同じ結果を返す。native で並列実行しても返る error は最小 index なので、WASM 逐次と一致する。

### 所有権・借用

- 入力 `[Task<Result<'a, 'e>>]` は `Task.parallel_results` が消費する。
- `Task<T>` の既存不変条件を維持:
  - task は非 Copy。
  - 捕捉値と結果は `Send`。
  - 参照を含む結果は `E1013`。
  - task を二度実行できない。
- 未開始 task の closure environment は drop する。
- 開始済み task の environment は callback 実行で消費済み。二重 drop しない。
- 成功時:
  - temporary `Result` slots から `Ok` payload を final array へ move。
  - temporary slots を zero して二重 drop を防ぐ。
  - temporary result buffer と started bitmap を free。
- failure 時:
  - chosen error payload を return union へ move。
  - それ以外の initialized result slots を drop。
  - not-started task closures を drop。
  - input task buffer、temporary result buffer、started bitmap を free。

## 設計

### Builtin

`src/check.rs::Builtin`:

```rust
pub enum Builtin {
    ...
    TaskParallelResults,
}
```

`Builtin::ALL` に追加し、`name()` は `"Task.parallel_results"`。

`signature()` は B01/A02 の `Result` union 型 ID が必要になる。現状 `Builtin::signature` は records なしで `Type` を返すため、次のどちらかを実装する。

**既定案 A（推奨）:** E02/A02 後に `Builtin::signature(self, names: &Names)` へ拡張し、std `Result` union ID を名前表から取得する。

```rust
Task.parallel_results :
    [Task<Result<'a, 'e>>] -> Task<Result<['a], 'e>>
```

内部 `Type`:

```rust
let a = Type::Variable("a".into());
let e = Type::Variable("e".into());
let result_a_e = Type::Union(result_id, vec![a.clone(), e.clone()]);
let result_array_e = Type::Union(
    result_id,
    vec![Type::Array(Box::new(a.clone())), e.clone()],
);
(
    Type::Array(Box::new(Type::Task(Box::new(result_a_e)))),
    Type::Task(Box::new(result_array_e)),
)
```

**代替案 B:** `Task.parallel_results` を std `Task.tc` 風に実装する。ただし `Task` は予約名前空間で、現状 `Task.run`/`Task.parallel` は `Builtin` なので、API の一貫性から A を採る。

### Typed IR

`src/check.rs::TypedExprKind`:

```rust
TaskParallelResults(Box<TypedExpr>),
```

`TypedExpr::children`/`children_mut` に `TaskParallelResults` を追加。

`src/check.rs::Checker::value_expression`:

- `ExprKind::Field(Name("Task"), field)` の builtin 解決で `parallel_results` を許可。
- `Builtin::TaskParallelResults` の関数値も通常の first-class builtin として扱う。

`src/polymorph.rs`, `ownership.rs`, `closures.rs`, `recursion.rs`, `call_specialization.rs` は `children` 経由の走査漏れを確認する。

### Runtime ABI

既存 ABI は残す。

```c
void tsuzuri_task_parallel(void (*run)(void *, uint64_t), void *context, uint64_t length);
```

B06 で追加:

```c
#define TSUZURI_TASK_NO_FAILURE UINT64_MAX

uint64_t tsuzuri_task_parallel_results(
    uint32_t (*run)(void *, uint64_t),
    void *context,
    uint64_t length
);
```

- callback は index の task を実行し、result slot に `Result<'a, 'e>` を store し、`1` if `Error`, `0` if `Ok` を返す。
- runtime は `uint64_t` で最小 failure index を返す。failure なしなら `UINT64_MAX`。
- callback の C ABI は aggregate payload を C に露出しない。`void *context, uint64_t index` だけは既存方針を維持する。

`src/runtime/task.c` の group:

```c
struct tz_task_result_group {
    uint32_t (*run)(void *, uint64_t);
    void *context;
    uint64_t length;
    _Atomic uint64_t next;
    _Atomic uint64_t cancel_after;
};
```

worker loop:

```c
for (;;) {
    uint64_t index = atomic_fetch_add_explicit(&group->next, 1, memory_order_relaxed);
    uint64_t cancel = atomic_load_explicit(&group->cancel_after, memory_order_acquire);
    if (index >= group->length || index >= cancel) return NULL;
    uint32_t failed = group->run(group->context, index);
    if (failed) {
        uint64_t current = atomic_load_explicit(&group->cancel_after, memory_order_relaxed);
        while (index < current &&
               !atomic_compare_exchange_weak_explicit(
                   &group->cancel_after, &current, index,
                   memory_order_release, memory_order_relaxed)) {}
    }
}
```

決定性:

- index は `fetch_add` で 0 から単調に開始される。
- ある failure index `k` が設定される時点で、`k` 未満の index はすでに開始済み。
- join 後、`cancel_after` は開始済み error の最小 index。
- したがって native の返す error は「全入力のうち最小 index の Error」と一致する。`k` より大きい未開始 index は、最小 error になり得ないため実行不要。

WASM `task-wasm.ll`:

```llvm
define internal i64 @tsuzuri_task_parallel_results(ptr %run, ptr %context, i64 %length) nounwind {
entry:
  br label %loop
loop:
  %index = phi i64 [ 0, %entry ], [ %next, %ok ]
  %done = icmp eq i64 %index, %length
  br i1 %done, label %success, label %body
body:
  %failed = call i32 %run(ptr %context, i64 %index)
  %is_failed = icmp ne i32 %failed, 0
  br i1 %is_failed, label %failure, label %ok
ok:
  %next = add i64 %index, 1
  br label %loop
failure:
  ret i64 %index
success:
  ret i64 -1
}
```

### LLVM lowering

`src/llvm.rs::FunctionEmitter` に `parallel_result_tasks` を追加する。

入力型:

```rust
tasks: [Task<Result<'a, 'e>>]
result: Result<['a], 'e>
```

主要 steps:

1. `tasks` を評価し、`source` pointer と `length` を取得。
2. temporary result buffer を `length * sizeof(Result<'a, 'e>)` 確保する。長さ 0 でも 1 byte 以上の既存 allocator 規則に合わせる。
3. started bitmap `[i8]` を確保し、0 で初期化する。
4. callback context `{ ptr source, ptr temp_results, ptr started }` を entry alloca に置く。
5. result 型ごとの callback を決定的名で生成する。

callback 擬似 IR:

```llvm
define internal i32 @tz.task.result.item.<mangled>(ptr %context, i64 %index) nounwind {
entry:
  %source = ...
  %results = ...
  %started = ...
  %slot = getelementptr %tz.closure, ptr %source, i64 %index
  %task = load %tz.closure, ptr %slot
  %code = extractvalue %tz.closure %task, 0
  %env = extractvalue %tz.closure %task, 1
  %result = call <ResultTy> %code(ptr %env)
  %destination = getelementptr <ResultTy>, ptr %results, i64 %index
  store <ResultTy> %result, ptr %destination
  %started_slot = getelementptr i8, ptr %started, i64 %index
  store i8 1, ptr %started_slot
  %tag = extractvalue <ResultTy> %result, 0
  %failed = icmp eq i32 %tag, 1 ; Result.Error tag from A02
  %failed_i32 = zext i1 %failed to i32
  ret i32 %failed_i32
}
```

6. `failed_index = call i64 @tsuzuri_task_parallel_results(callback, context, length)`。
7. `failed_index == UINT64_MAX`:
   - final array を確保。
   - for each index 0..length:
     - `started[index]` は 1 のはず。debug assert は生成しないがテストで検査。
     - temp result の tag が `Ok` であることを前提に payload を move。
     - final array slot に store。
     - temp result slot を zero。
   - input source buffer を free（task env は callback が消費済み）。
   - temp buffer、started bitmap を free。
   - `Ok final_array` を返す。
8. failure:
   - chosen result slot `failed_index` から `Error` payload を move。
   - loop 0..length:
     - if `started[index] == 1` and index != failed_index: temp result を drop。
     - if `started[index] == 0`: source task closure を drop（未実行）。
   - chosen slot を zero。
   - source buffer、temp buffer、started bitmap を free。
   - `Error error_payload` を返す。

drop helper:

- 未開始 task closure は `%tz.closure` の drop pointer を呼ぶ。task closure は clone pointer null だが drop pointer は環境解放用に存在する。
- 開始済み task closure は task code が env を消費するため drop しない。
- temp `Result` drop は A02 の union drop を使う。

### Driver / runtime linking

`src/llvm.rs::emit_target`:

- 既存の `output.contains("@tsuzuri_task_parallel(")` に加え、`@tsuzuri_task_parallel_results(` を検出して runtime を連結する。
- WASM は `task-wasm.ll` に両方の関数を含める。
- native IR には `declare i64 @tsuzuri_task_parallel_results(ptr, ptr, i64)` を出す。
- `driver.rs` は native object/exe で `src/runtime/task.c` を既存と同じ条件でコンパイルする。条件は `@tsuzuri_task_parallel` または `@tsuzuri_task_parallel_results` のいずれか。

## 実装手順

### Phase 1（完全指定）: `Task.parallel_results`

1. **Builtin と型検査**
   - `Builtin::TaskParallelResults` を追加。
   - A02/E02 後の `Result` union ID を使って signature を作る。
   - `Task.parallel_results` のフィールド解決を追加。
   - 確認: `cargo test --locked --test tasks task_parallel_results_typechecks`（`running N tests` の N が 0 でないことを確認する。GUIDE §3）。
2. **TypedExpr と走査**
   - `TypedExprKind::TaskParallelResults` を追加。
   - `children`/`children_mut` に追加。
   - `polymorph`, `ownership`, `closures`, `recursion`, `call_specialization` の漏れを grep で確認。
   - 確認: `cargo test --locked --test tasks task_parallel_results_polymorphic`（`running N tests` の N が 0 でないことを確認する。GUIDE §3）。
3. **runtime ABI**
   - `src/runtime/task.c` に `tsuzuri_task_parallel_results` を追加。
   - 既存 `tsuzuri_task_parallel` は変更せず互換維持。
   - `tests/task_runtime.c` に result API の単体テストを追加:
     - failure なし -> `UINT64_MAX`
     - index 3 failure -> 3
     - index 7 が早く失敗しても index 2 も失敗するなら返り値 2
     - cancellation 後に index > failure が開始されないことを counter で検査（時間閾値なし）
   - 確認: `clang -std=c11 -pthread tests/task_runtime.c src/runtime/task.c ...` は既存 harness に合わせる。
4. **WASM runtime**
   - `src/runtime/task-wasm.ll` に `@tsuzuri_task_parallel_results` を追加。
   - import なし。
   - 確認: `llvm::emit_target(..., wasm=true)` の IR に declare が残らない。
5. **LLVM lowering**
   - `FunctionEmitter::parallel_result_tasks` を実装。
   - callback 名は result LLVM 型から決定的に作る。`BTreeSet`/`intrinsics` に入れて重複排除。
   - temp result buffer、started bitmap、context alloca は entry block に置く。
   - success/failure の drop/move を実装。
   - 確認: `cargo test --locked --test tasks emits_parallel_results_runtime`（`running N tests` の N が 0 でないことを確認する。GUIDE §3）。
6. **所有権・二重実行検査**
   - `Task.parallel_results [work, work]` は既存 `E1012`。
   - `Task.parallel_results` 後に元 tasks を使うと `E1012`。
   - 未開始 task の drop を E2E heap tracking で確認。
7. **E2E**
   - `tests/fixtures/tasks/Main.tz` に export 関数を追加。
   - `tests/tasks.mjs` に native/WASM × `-O0`/`-O3` の結果・traps・heap tracking を追加。
   - 確認: `node tests/tasks.mjs target/release/tsuzuri`。

### Phase 2: F01 worker pool との統合確認

- F01 が runtime scheduler を置き換えている場合、`cancel_after` を worker pool queue に伝える。
- queue は index 順の work item を配布するか、少なくとも「index < current_min_failure は開始済みまたは優先」を保証する。
- 返る error は引き続き最小 index。

### Phase 3: 高水準 helper

- 必要なら std `Task` 風文書に `Task.sequence_results` などを追加するが、`Task` は予約 namespace なので B06 では `Builtin` だけ。

## テスト計画

### Rust 受理

```text
let jobs = [
    task { return Ok 20 },
    task { return Ok 22 }
]
match Task.run (Task.parallel_results jobs) with
| Ok values -> values[0] + values[1]
| Error _ -> -1
```

```text
let jobs = [
    task { return Ok 20 },
    task { return Error 42 },
    task { return Ok (1 / 0) }
]
match Task.run (Task.parallel_results jobs) with
| Ok _ -> -1
| Error error -> error
```

```text
def make :: 'a -> Task<Result<'a, string>>
fn make value = task { return Ok value }
let run: [Task<Result<i64, string>>] -> Task<Result<[i64], string>> = Task.parallel_results
match Task.run (run [make 42]) with
| Ok values -> values[0]
| Error _ -> -1
```

### Rust 拒否

| プログラム | 期待 |
|---|---|
| `Task.parallel_results [task { return 1 }]` | `E1003` |
| `let work = task { return Ok 1 }; Task.run (Task.parallel_results [work, work])` | `E1012` |
| `export def bad :: Task<Result<i64, string>>` | `E1008` |
| `def bad :: Task<Result<&i64, string>> ...` | `E1013` |
| `Task.parallel_results [task { return Error "a" }, task { return Error 1 }]` | `E1003` |

### E2E fixture

追加 export:

- `parallel_results_ok(count)`:
  - `new [Task<Result<i64, i64>>](count, i -> task { return Ok (i * i) })`
  - `Ok values` なら sum。
  - 期待値 JS: `sum i^2`。
- `parallel_results_error()`:
  - index 0 ok, index 1 error 42, index 2 ok/trap-like expensive but should not start if cancellation prevents。
  - 期待 `42`。
- `parallel_results_lowest_error()`:
  - index 1 slow error 11, index 2 fast error 22。native timingに依存せず 11。
  - 時間 sleep は言語にないため、計算量差で順序を揺らす。ただし期待は index 最小のみ。
- `parallel_results_owned_error(count)`:
  - error payload が string を含む Result を内部で扱い、最終的に length を返す。
  - chosen 以外の payload drop を heap tracking で確認。
- `parallel_results_empty()` -> `Ok []` length 0。
- `parallel_results_not_started_drop(count)`:
  - 早い index で `Error`、後続 tasks は大きい owned captures を持つ。
  - 未開始 captures が drop され、`live == 0`。
- traps:
  - index 0 `Ok`, index 1 trap -> native/WASM trap。
  - trap は `Error` に変換されない。

`tests/tasks.mjs`:

- native `-O0`/`-O3` with `src/runtime/task.c -pthread`。
- object link path。
- WASM `-O0`/`-O3` imports empty。
- `live == 0` after each call。
- runtime IR contains exactly one declaration/definition for each used symbol.

### Runtime C tests

`tests/task_runtime.c` に result API tests を追加。

- `result_success`: callback always 0, all indices visited, return `UINT64_MAX`。
- `result_failure_min`: callbacks for 2 and 7 fail, return 2。
- `result_cancellation`: first failure at 3, indices greater than or equal to observed cancel threshold are not started after cancel is visible。
- pthread create/join failure path は既存 `Tsuzuri task runtime: pthread_create failed` / `pthread_join failed` と同様に検査。

時間の速さを合否条件にしない。条件変数や atomic counter で開始有無だけ検査する。

## ドキュメント

- `docs/language.md`:
  - 「タスク」に `Task.parallel_results` を追加。
  - cancellation は「未開始 work の開始停止」であり、開始済み task の強制停止ではないと明記。
  - 返る error は最小 input index と明記。
  - trap は `Error` に変換しない。
- `docs/architecture.md`:
  - runtime ABI `tsuzuri_task_parallel_results`。
  - started bitmap、temp result buffer、drop/move 不変条件。
  - WASM 逐次 semantics。
- `README.md`:
  - タスク節に `Result` 付き parallel example を追加。

## 受け入れ条件

- [ ] `Task.parallel_results : [Task<Result<'a, 'e>>] -> Task<Result<['a], 'e>>` が型検査される。
- [ ] すべて `Ok` の場合、入力順の array を `Ok` で返す。
- [ ] `Error` がある場合、スレッド完了順に依存せず最小 input index の error を返す。
- [ ] failure 検出後、未開始 index の新規開始を止める。
- [ ] 開始済み task はすべて join してから返る。detached worker を残さない。
- [ ] 未開始 task captures、未採用 Ok/Error payload、temporary buffers が drop/free され、heap tracking `live == 0`。
- [ ] trap/runtime abort は `Error` に変換されない。
- [ ] WASM は import なし、逐次で native と同じ結果。
- [ ] native/WASM × `-O0`/`-O3`、object link path、runtime C tests が通る。
- [ ] 既存 `Task.parallel` ABI と挙動を壊さない。

## 落とし穴

- 「最初に完了した Error」を返すと非決定的。必ず最小 index。
- failure 後に開始済み task を止めようとしない。Tsuzuri には unwind/destructor がない。
- 未開始 task closure を drop し忘れると owned captures が漏れる。
- 開始済み task closure を failure cleanup で再度 drop すると二重解放。
- temp result slots は initialized bitmap なしで drop してはいけない。
- success 時も temp `Result` slots を zero/move 済みにしないと payload が二重 drop される。
- WASM の `-1` は `i64` の all ones。JS からは BigInt に見えるが内部 runtime sentinel としてだけ使う。
- `Result` union tag 値は A02 の定義に合わせる。B06 側で tag 0/1 を重複定義しない。A02 の helper/metadata を使う。

## 対象外

- プリエンプティブ cancellation。
- task body への cancellation token 引数。
- shared mutable state、atomic user API、channels。
- `Task.parallel` 自体の挙動変更。
- 例外、unwind、trap の recovery。
- `and!` による自動 task parallel。B05 で非採用。
- GPU/WASM threads 連携。

## 未決事項

- **既定案:** runtime ABI は `uint64_t tsuzuri_task_parallel_results(uint32_t (*run)(void *, uint64_t), void *context, uint64_t length)` とし、callback は aggregate を返さない。
- F01 が worker pool ABI を変更済みの場合、B06 は上記 ABI を外部に保つ wrapper として実装するか、F01 の内部 scheduler に `cancel_after` を追加する。公開 LLVM 側 symbol は `tsuzuri_task_parallel_results` を維持する。
- started bitmap の型は `i8` 配列を既定とする。bitset はメモリを減らせるが、drop loop が複雑になるため phase 1 では採用しない。
