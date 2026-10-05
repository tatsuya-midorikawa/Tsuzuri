# F10: 並行処理プリミティブ（Atomic・Mutex・Channel）

| 項目 | 内容 |
| --- | --- |
| ID | F10 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | F01, B07, (A13), (C10) |
| 後続 | C10 Phase 2 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（std 名 `Atomic`・`Mutex`、組み込みクラス `Sync`・`AtomicValue`、builtin `Task.scope` の確定。`AtomicValue` は GUIDE D-30 に未割り当て）, D10（Phase 2 の `Channel` と待ちの規則） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: C++ の `std::atomic`／`mutex`、Rust の `Mutex`／channel、C# の `Interlocked`／`Channel<T>` に相当する共有状態・パイプラインがない |
| 手本にする既存実装 | 不透明な std record: `std/Gpu.tz` の `Buffer<'a> { values: ['a] }`、`src/stdlib.rs` の `opaque_record`、`src/check.rs` の `Type::is_noncopy_record`。builtin と std ファイルが同居する module: `std/Parallel.tz` と `Builtin::ParallelInit` 等、std 型を含む builtin の型: `Builtin::TaskParallelResults` の `BuiltinType::Std`。fork/join の lowering と所有権: `TypedExprKind::Parallel`、`Builtin::is_parallel`、`src/ownership.rs` の `E::Parallel` の arm、`src/llvm_parallel.rs` の `parallel_apply`・`parallel_launch`。組み込みクラス: `BUILTIN_CLASSES` の `Send`、`Classes::intrinsic`、`Classes::validate_instance` の専用メッセージ、`Type::can_send`。C ランタイムと試験: `src/runtime/task.c` の `tz_task_lock`・`tz_task_wait`・`tz_task_fail`、`tests/task_runtime.c` の `rendezvous` |
| 主な影響ファイル | `std/Atomic.tz`・`std/Mutex.tz`（新規）, `src/stdlib.rs`, `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/llvm_parallel.rs`, `src/llvm_sync.rs`（新規）, `src/driver.rs`, `src/runtime/task.c`, `src/runtime/task-wasm.ll`, `tests/concurrency.rs`（新規）, `tests/sync_runtime.c`（新規）, `tests/fixtures/concurrency/Main.tz`（新規）, `tests/features.mjs`, `tests/tasks.mjs`, `tests/wasm_threads.mjs`, `docs/language.md`, `docs/architecture.md`, `_docs/language-reference/tasks.md`, `_docs/library-reference/parallel.md`, `_docs/library-reference/concurrency.md`（新規）, `_docs/library-reference/README.md`, `_docs/feature-status.md`, `_features/README.md`。Phase 2 だけ: `std/Channel.tz`（新規）, `src/runtime/task-wasm-threads.c` |

## 目的

構造化された fork/join（`Task.parallel`、`Parallel`）だけでは表せない、進捗カウンター・共有キャッシュ・生産者と消費者のパイプラインを、データ競合なしに書けるようにする。
Phase 1 は「呼び出し元の値を共有借用で子へ渡すスコープ付き並列（`Task.scope`）」「lock-free の `Atomic`」「コールバック形式の `Mutex`」を入れる。
Phase 2 は容量付きの `Channel` を入れる。

実装者は Phase 1 だけを実装する。Phase 2 は D10 の承認後に人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- F01 が `_features/README.md` の状態欄で done であること（HEAD で done）。確認: `grep -n "| F01 \|| B07 \|| F10 " _features/README.md`。
- B07 は Phase 1 の着手条件にしない。Phase 1 の型は配列 field を持つ std record なので、解放は既存の record・配列の drop で足りる（D3）。
  Phase 2 の `Channel` は送信側の drop で close するので B07 の done が要る。A13・C10 はどちらの Phase でも不要。
- D1 が承認済みであること。承認前はどの手順にも着手しない。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（IR と stack-depth テスト）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- std の汎用関数に組み込みクラスの制約（`AtomicValue<'a> =>`）を書けない、または std の汎用関数が利用者の特殊化予算（1,024）を先に消費する（`honors_the_exact_specialization_limit` が失敗する）。
- `ref Atomic<'a>` から配列の data pointer を得る途中で配列の中身が複製される（`clone_value` を通る、または data pointer が呼び出しごとに変わる）。
- `Atomic`・`Mutex` の値を `clone_value` で複製する経路が見つかった（D4 は複製を禁止する）。
- `atomicrmw`・`cmpxchg`・`load atomic` を含む IR が既定の wasm32（threads なし）で `-O0` または `-O3` の build に失敗する、または WASM の import が増える。
- 配列の確保（`@tz.alloc`）の align が要素の自然な align（`i64` は 8）を満たさない target がある。
- `src/runtime/task.c` の `_Thread_local` が native の対応 target（macOS、Linux、`src/runtime/task-windows.h` の Windows）のどれかで compile できない。
- TSan が runtime または生成コードの競合を報告し、テスト側の誤りでない。
- 既存テストの期待値を変える必要がある。例外は「既存テストへの影響」に挙げた 3 か所（`RESERVED_MODULES` の数、`BUILTIN_CLASSES` の長さ、`src/runtime/task-wasm.ll` を含む WASM IR の関数数）だけ。
- stack-depth の 3 テスト（`bounds_type_growing_polymorphic_recursion`、`bounds_recursive_and_flat_expression_depth`、`bounds_nested_builder_expansion_not_just_source_syntax`）が失敗する。上限や stack サイズを上げたくなった。
- `unsafe`、新しい crate、既定の WASM import、`--wasm-feature threads` での `Mutex` の対応（Phase 1 は E2000 で拒否する。D8）が必要になった。

## 現状（HEAD `f8dc655` で確認）

- docs/language.md の `### 並列区間と寿命` は「`and!`、detach、スレッド ID、共有状態、外部キャンセルトークン、回復可能なタスク例外は提供しません」と書く。
- Task の捕捉は `Send`（`src/closures.rs` の `if task { "Send" } else { "Capture" }`）。`Type::can_send`（`src/check.rs`）は `Type::Reference` を拒否し、
  関数値は「環境は所有権検査で見る」として true を返す。`src/ownership.rs` は借用を保持する Task の捕捉を
  `task captures cannot retain borrowed values, including borrowed function environments`（E1013）で拒否する。
- 並列 builtin は `Builtin::ParallelInit`・`ParallelMap`・`ParallelMapRef`・`ParallelReduce`（`Builtin::is_parallel`）で、`TypedExprKind::Parallel(Builtin, Vec<TypedExpr>)` に型付けされる。
  `src/ownership.rs` の `E::Parallel` の arm は callback と値が借用を保持すると `parallel callbacks and values cannot retain borrowed environments`（E1013）を出す。
  `Parallel.map_ref` は入力要素の共有借用を fork/join の内側だけで callback へ渡す。呼び出し元の任意の値を子へ共有借用で渡す API はない。
- ランタイム: native は `src/runtime/task.c` の常駐 pool（`tsuzuri_task_parallel`・`tsuzuri_task_parallel_results`、`TZ_TASK_API` = weak・hidden、pool の `pthread_mutex_t`・`pthread_cond_t`、失敗は `tz_task_fail`）。
  既定の WASM は `src/runtime/task-wasm.ll` の internal な逐次実装。`--wasm-feature threads` は `src/runtime/task-wasm-threads.c`（`lock`・`unlock` は `__builtin_wasm_memory_atomic_wait32`・`notify`）。
  threads の worker は host が `__stack_pointer` だけを設定し（`tests/wasm_threads.mjs`）、TLS（`__wasm_init_tls`）は初期化しない。
- 連結の契機: `src/llvm.rs` は IR が `@tsuzuri_task_parallel(` を含むと、WASM（threads なし）では `task-wasm.ll` を、それ以外では 2 つの `declare` を足す。
  `src/driver.rs`（`task_runtime`）、`src/test_runner.rs`、`tests/features.mjs` は native の IR が `declare void @tsuzuri_task_parallel(` を含むと `task.c` を連結する。
- 組み込みクラスは `src/polymorph.rs` の `BUILTIN_CLASSES: [&str; 25]`。判定は `Classes::intrinsic` の `"Send" => ty.can_send(types)` 等、失敗時の専用メッセージは
  `Classes::validate_instance`（`Capture` は E1005、`Send` は E1013 `tasks require owned values; {} contains a reference`）。
- 不透明な std record は `src/stdlib.rs` の `opaque_record`（`Map.Map`・`Set.Set`・`Seq.Seq`・`Gpu.Device`・`Gpu.Buffer`・`IO.IO` 等）、非 Copy の std record は
  `Type::is_noncopy_record`（`Seq.Seq`・`Gpu.Device`・`Gpu.Buffer`）。std の module 名は `RESERVED_MODULES`（20 個。`src/stdlib.rs` の test が数を assert する）。
- `new` は予約語なので `Atomic.new` は字句・構文の段階で E0002 になる。構築関数の名前は `create` にする（D1）。
- LLVM の表現: `%tz.array = type { ptr, i64 }`、`Type::Reference(..)` は `ptr`。要素 pointer は `FunctionEmitter::element_pointer(element, data, index)`。

### 再現（検証済み）

各ケースを別ディレクトリの `Main.tz` に置き、`target/release/tsuzuri run <dir>` で確認した。

```tsuzuri
let number = 1
let borrowed = ref number
let values = Parallel.init 2 (index -> deref borrowed + index)
values.length
```

結果: `error[E1013]: parallel callbacks and values cannot retain borrowed environments`（callback の位置）。

```tsuzuri
let number = 1
let borrowed = ref number
Task.run (task { return deref borrowed })
```

結果: `error[E1013]: tasks require owned values; ref i64 contains a reference`（`task` の位置）。
`let counter = Atomic.new 0` は `error[E0002]: expected an identifier`（`new` の位置）。利用者の `record Mutex { value: i64 }` は受理される（D1 の後も受理する）。

## 仕様

### 前提とする他チケットのインターフェース

- F01（done）: `tsuzuri_task_parallel(void (*run)(void *, uint64_t), void *context, uint64_t length)`。各 index を一回だけ実行し、join を待つ thread は
  未配布の仕事を手伝う（F01 の「グループと進行保証」）。結果の書き込みは、join が戻る前に呼び出し元から見える（HEAD では
  `remaining` の acq-rel、PR06 の D7 の後はグループの mutex と `helpers == 0` の通知で公開される。保証は同じ）。`Task.scope` はこれに乗る。
- PR06（並行して詳細化中）: F10 は PR06 が `tsuzuri_task_parallel`・`tsuzuri_task_parallel_results` の ABI と「join 中の thread が手伝う」保証を保つと仮定する。
  PR06 が入口を増やす場合、その入口でも D7 の `tz_mutex_held` の検査を行う。
- PR01: F10 は `Type::has_interior_mutability`（新規）を提供する。PR01 は属性の条件に `Type::is_frozen`（PR01 で新規、当初は常に true）を使うので、
  PR01 が先に done なら F10 は `is_frozen` の本体を `!self.has_interior_mutability(types)` に置き換える（PR01 が後なら PR01 がそう実装する）。
  true の型への `ref` に PR01 は `noalias`・`readonly`・`!invariant.load` を付けず、
  その型から届くメモリが呼び出しをまたいで不変だとも仮定しない（`Atomic`・`Mutex` の cell は共有借用中に書き換わる）。
- B07（Phase 2 だけ）: 送信側の drop で `Channel` を close する。

### API（Phase 1）

新 API（実装後に有効。未検証）。型名は std record の修飾名（`std/Gpu.tz` の `Result<Device, Error>` と同じ書き方）。

```tsuzuri
// std/Atomic.tz（新規）。opaque_record。cell の要素は常に 1 個
record Atomic<'a> { cell: ['a] }
// std/Mutex.tz（新規）。opaque_record。cell の要素は常に 1 個で、u32 は lock 語
record Mutex<'a> { cell: [(u32, 'a)] }
```

| builtin（`Builtin` の variant） | 型 |
| --- | --- |
| `Task.scope`（`TaskScope`） | `(Sync<'s>, Send<'a>) => ref 's -> i64 -> (ref 's -> i64 -> 'a) -> ['a]` |
| `Atomic.create`（`AtomicCreate`） | `AtomicValue<'a> => 'a -> Atomic<'a>` |
| `Atomic.load`（`AtomicLoad`） | `AtomicValue<'a> => ref Atomic<'a> -> 'a` |
| `Atomic.store`（`AtomicStore`） | `AtomicValue<'a> => ref Atomic<'a> -> 'a -> unit` |
| `Atomic.swap`（`AtomicSwap`） | `AtomicValue<'a> => ref Atomic<'a> -> 'a -> 'a` |
| `Atomic.compare_exchange`（`AtomicCompareExchange`） | `AtomicValue<'a> => ref Atomic<'a> -> 'a -> 'a -> Result<'a, 'a>` |
| `Atomic.fetch_add`・`fetch_sub`・`fetch_and`・`fetch_or`・`fetch_xor`（`AtomicFetchAdd` 等 5 個） | `(AtomicValue<'a>, Integer<'a>) => ref Atomic<'a> -> 'a -> 'a` |
| `Atomic.into_inner`（`AtomicIntoInner`） | `Atomic<'a> -> 'a` |
| `Mutex.create`（`MutexCreate`） | `Send<'a> => 'a -> Mutex<'a>` |
| `Mutex.with`（`MutexWith`） | `Send<'b> => ref Mutex<'a> -> (ref mut 'a -> 'b) -> 'b` |
| `Mutex.into_inner`（`MutexIntoInner`） | `Mutex<'a> -> 'a` |

`compare_exchange cell expected desired` は現在値が `expected` と等しければ `desired` を書いて `Ok previous`、等しくなければ書かずに `Error current` を返す（strong。見かけの失敗はない）。
`fetch_*` は更新前の値を返す。`Task.scope` は `Parallel` と同じく完全適用だけを許す（`Builtin::is_parallel` に入れる）。ほかの 14 個は既存の builtin と同じく関数値にできる。

### 型規則

- `AtomicValue<T>`（新規の組み込みクラス）: `Type::Integer(8 | 16 | 32 | 64, _)` と `Type::Bool`。`i128`・`u128`・浮動小数・文字は満たさない。
- `Sync<T>`（新規の組み込みクラス。`Type::can_sync`（新規））: 共有借用を複数の thread へ同時に渡してよい型。
  - false: `Type::Function(..)`（環境の型が見えない）、`Type::Task(_)`、`Type::Reference(_, true)`、`Type::is_noncopy_record` の `Seq.Seq`・`Gpu.Device`・`Gpu.Buffer`。
  - `Atomic<T>` は true。`Mutex<T>` は `T.can_send(types)`（中身は一度に一つの thread だけが触る）。
  - `Type::Reference(inner, false)`・`Array`・`List`・`Vec`・`Tuple`・`Record`・`Union` は構成要素がすべて `Sync` なら true。再帰型は
    `types.stored_all` で `Function`・`Task`・`Reference(_, true)` を含まないこと。スカラーと文字列は true（共有中は不変）。
- `Send`・`Copy`・`Capture`: 2 つの record は `Type::is_noncopy_record` に足すので `Copy` ではない。`Type::can_capture` は 2 つの record（それらを含む型も）で false にする
  （関数値の複製が環境を複製すると、独立した別のカウンターになるため）。`Send` は record の構造規則のまま（`Atomic` は常に、`Mutex<T>` は `T` が `Send` なら true）。
  Task の捕捉は `Send` で見るので、`task { ... }` へ move することはできる。
- `Type::has_interior_mutability`（新規）: 2 つの record を構造的に（`Reference` の内側も）含むと true。
- `Task.scope shared count callback`: `shared` は `ref 's`（`Sync<'s>`）。`callback` と `count` は借用を保持してはならない（`Parallel.init` の callback と同じ規則）。
  結果の `'a` は `Send`（借用を結果へ持ち出せない）。
- `Mutex.with m f`: `f` は同期して一回だけ呼ぶので、`f` の環境は借用を捕捉してよい。結果の `'b` は `Send`（`ref mut 'a` を持ち出せない）。

### 評価順序・所有権・借用

- 引数は左から右に評価する。`Task.scope` は `count < 0` なら trap し、結果配列を確保し、index `0..count-1` の子を各一回だけ
  `callback shared index` として実行し、全子の完了後に戻る。子の実行順序・同時実行の有無は未規定で、結果の添字は index に対応する。
  `shared` の借用は呼び出しの間だけ続く。戻った後に子が走ることはない。
- `callback` の環境は `Parallel.init` と同じく子ごとの複製（`parallel_launch` の snapshot）か、直接の借用呼び出しで使う。
- `Mutex.with`: lock を取り、cell の値への `ref mut` を `f` に渡し、`f` の戻りの後に lock を解き、結果を返す。`f` の中の trap は既存の trap と同じく abort で、unlock しない。
- `Atomic.*` の操作は一つの atomic 命令で、すべて SeqCst（D5）。`create` は値を消費し、`into_inner` は容器を消費する。
- drop: 2 つの record は配列 field の既存の drop で解放される（`Mutex` は要素の値を drop してから解放）。

### 数値・トラップ・native と WASM の差

- `fetch_add`・`fetch_sub` は 2 の補数で wrap する（LLVM の `atomicrmw add`・`sub`）。`bool` の cell は `i8` の 0/1 として読み書きする（atomic 命令は `i1` を扱えない）。
- 既定の WASM（threads なし）: wasm backend は atomics 機能がないとき atomic 命令を通常の load・store に下げる。実行は単一 thread で、
  `Task.scope` の子は index 順に一つずつ走る。結果は native の任意のスケジュールの一つと一致する。
- `--wasm-feature threads`: `Atomic` と `Task.scope` は動く。`Mutex` は E2000（D8。TLS の初期化がないため）。
- 実行時の trap（全 backend。native は stderr に `Tsuzuri runtime: <message>` を書いてから trap、既定の WASM は `llvm.trap` だけ）:

| 条件 | native のメッセージ |
| --- | --- |
| `Mutex.with` の実行中に（同じか別の）`Mutex.with` を呼ぶ | `Mutex.with cannot be nested; release the outer mutex first` |
| `Mutex.with` の実行中に `Task.parallel`・`Task.parallel_results`・`Task.scope`・`Parallel.*` を始める | `parallel work cannot start inside Mutex.with; move it outside the critical section` |
| `Task.scope` の `count < 0` | 結果配列の確保の既存 trap（`Parallel.init` の負の長さと同じ） |

- 決定性: IR は決定的。`Task.scope` の結果配列と、可換な更新（`fetch_add` の合計、`Mutex.with` での加算）の最終値はスケジュールによらない。
  個々の `fetch_*` の戻り値と critical section の順序は native・threads では未規定、既定の WASM では index 順。テストはスケジュールによらない値だけを assert する。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E1013 | `Sync` を満たさない（`Task.scope` の共有値） | `task scopes can share only Sync values; {T} is not Sync, so move it into the callback or remove functions, tasks and exclusive references from it` | 制約の span（共有値の引数） |
| E1013 | `Task.scope` の callback・count が借用を保持する | 既存の `parallel callbacks and values cannot retain borrowed environments` | その引数 |
| E1013 | `Task.scope` の結果・`Mutex.with` の結果が参照を含む | 既存の `tasks require owned values; {T} contains a reference`（文言は変えない） | 制約の span |
| E1005 | `AtomicValue` を満たさない | `atomic values must be i8, i16, i32, i64, u8, u16, u32, u64 or bool; use Mutex for {T}` | 制約の span |
| E1005 | `fetch_*` の要素が整数でない（`bool` 等） | `Integer` の既存メッセージ | 制約の span |
| E1013 | `Task.scope` を完全適用しない | `let operation = Parallel.init` と同じ既存の診断 | 式 |
| E2000 | `--wasm-feature threads` の build の IR が `@tsuzuri_mutex_lock(` を含む | `Mutex is not supported with --wasm-feature threads yet; build without threads or use Atomic` | なし（driver） |

### 資源上限

新しい上限は足さない。`Task.scope` の子の数は結果配列の確保（WASM は 16 MiB のヒープ）と pool（native は最大 31 の追加 thread）で決まる。
`Mutex` の待ちは全 Mutex で一つの parking 用 mutex と condvar を使う（D7。競合が計測で問題になったら address で分割する）。

### 例

新 API（実装後に有効。未検証）。期待値は手計算（28 + 140 = 168、10 + 6 = 16）。

```tsuzuri
let counter = Atomic.create 0
let squares = Task.scope (ref counter) 8 (shared -> index -> {
    let _previous = Atomic.fetch_add shared index;
    index * index
})
Atomic.load (ref counter) + Array.sum (ref squares)
```

```tsuzuri
let total = Mutex.create 0
let indices = Task.scope (ref total) 4 (shared -> index -> Mutex.with shared (value -> {
    deref value = deref value + index + 1;
    index
}))
Mutex.into_inner total + Array.sum (ref indices)
```

拒否（新 API。未検証）: 共有値が関数（`Task.scope (ref callback) 2 (...)`）は E1013、callback が `shared` を返す・`Mutex.with (ref m) (value -> value)` は E1013、
callback が外側の借用を捕捉すると E1013、`Atomic.create 1.5` は E1005、`Atomic.fetch_add (ref flag) true` は E1005。
実行時 trap: `Mutex.with` の入れ子、`Mutex.with` の中の `Task.parallel`、`Task.scope (ref total) (0 - 1) (...)`。

### Phase 2（設計方針。D10 の承認後）

- `std/Channel.tz`（新規）: 不透明な `Sender<'a>`・`Receiver<'a>`。`Channel.bounded : Send<'a> => i64 -> (Channel.Sender<'a>, Channel.Receiver<'a>)`（容量 1 以上、未満は trap）、
  `Channel.send : ref Channel.Sender<'a> -> 'a -> unit`（満杯なら待つ）、`Channel.recv : ref Channel.Receiver<'a> -> Option<'a>`（空で開いていれば待ち、全 Sender の drop 後は `None`）、
  `Channel.clone_sender : ref Channel.Sender<'a> -> Channel.Sender<'a>`。両端とも `Sync`（runtime の lock で守る MPMC）。未受信の要素は close 後の最後の drop で解放する。
- 待ちの規則: 待つ thread はまず pool の未配布の仕事を手伝い、それでも進めなければ park する。全 thread が channel・join で park し、未配布の仕事がないとき
  `deadlock: every task is waiting on a channel` で trap する。既定の WASM は子を index 順に走らせ、すぐに満たせない待ちは同じ trap。`Mutex.with` の中の待ちは D7 の trap。
- `--wasm-feature threads` の `Mutex`・`Channel`: worker ごとの TLS（`__wasm_init_tls`）の初期化を F06 の worker 起動へ足してから有効にする。

## 設計

### データ構造

```rust
// src/check.rs: enum Builtin に足す（Builtin::ALL・Builtin::name・Builtin::scheme にも同じ順で）
TaskScope,
AtomicCreate, AtomicLoad, AtomicStore, AtomicSwap, AtomicCompareExchange,
AtomicFetchAdd, AtomicFetchSub, AtomicFetchAnd, AtomicFetchOr, AtomicFetchXor, AtomicIntoInner,
MutexCreate, MutexWith, MutexIntoInner,

// src/check.rs: impl Type
pub(crate) fn can_sync(&self, types: &TypeContext<'_>) -> bool { /* 型規則 */ }
pub(crate) fn has_interior_mutability(&self, types: &TypeContext<'_>) -> bool { /* 型規則 */ }
fn is_shared_cell(&self, types: &TypeContext<'_>) -> bool {
    matches!(self, Self::Record(id, _) if types.records[*id].origin == ModuleOrigin::Std
        && matches!(types.records[*id].name.as_str(), "Atomic.Atomic" | "Mutex.Mutex"))
}
```

`Type` に variant は足さない（D3）。`Task.scope` は `TypedExprKind::Parallel(Builtin::TaskScope, [shared, count, callback])` に型付けされる。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| std | `std/Atomic.tz`・`std/Mutex.tz`（新規） | `record Atomic<'a>`・`record Mutex<'a>` | record 宣言だけ（操作はすべて builtin） |
| std 登録 | `src/stdlib.rs` | std ファイルの表（`("std/Parallel.tz", include_str!(...))` の並び）, `RESERVED_MODULES`, `opaque_record` | 2 ファイル（名前順）、`"Atomic"`・`"Mutex"`、`"Atomic.Atomic"`・`"Mutex.Mutex"` |
| 型性質 | `src/check.rs` | `Type::is_noncopy_record` | 名前の列に `"Atomic.Atomic"`・`"Mutex.Mutex"` |
| 型性質 | `src/check.rs` | `Type::can_capture` | 先頭で `self.is_shared_cell(types)` なら false。再帰型の `stored_all` の述語にも足す |
| 型性質 | `src/check.rs` | `Type::can_sync`・`has_interior_mutability`・`is_shared_cell`（新規） | 型規則のとおり |
| 型性質 | `src/check.rs` | `Type::can_send`・`is_copy`・`needs_drop`・`carries_loans`・`contains_reference` | 変更なし（record の構造規則で正しい） |
| builtin | `src/check.rs` | `Builtin`, `Builtin::ALL`, `Builtin::name`, `Builtin::scheme`, `Builtin::is_parallel` | 15 variant。型は API の表。std 型は `BuiltinType::Std { module: "Atomic", name: "Atomic", .. }`、制約は `BuiltinConstraint` |
| クラス | `src/polymorph.rs` | `BUILTIN_CLASSES` | `[&str; 27]`、末尾に `"Sync"`・`"AtomicValue"` |
| クラス | `src/polymorph.rs` | `Classes::intrinsic` | `"Sync" => ty.can_sync(types)`、`"AtomicValue" => matches!(ty, Type::Integer(8 \| 16 \| 32 \| 64, _) \| Type::Bool)`。`Type::Simd` の arm の `"SimdVector" \| "Copy" \| "Capture" \| "Send"` に `"Sync"` |
| クラス | `src/polymorph.rs` | `Classes::validate_instance` | `Sync` は E1013、`AtomicValue` は E1005 の専用メッセージ（診断の表） |
| 所有権 | `src/ownership.rs` | `E::Parallel` の arm | `TaskScope`: callback は index 2、index 0（共有借用）は loans を許す、index 1・2 は既存の「loans があれば E1013」 |
| LLVM | `src/llvm.rs` | `#[path = "llvm_task.rs"] mod task;` の並び | `#[path = "llvm_sync.rs"] mod sync;` |
| LLVM | `src/llvm_parallel.rs` | `parallel_expression`, `parallel_launch`, `Kernel` の引数を作る `if kernel.operation == Builtin::ParallelInit` | `TaskScope` は `ParallelInit` と同じ経路で、chunk 数 = `count`（`parallel_count` を使わない）、callback の引数は `(ref 's, shared)` と `(i64, index)` |
| LLVM | `src/llvm_sync.rs`（新規） | `atomic_operation`・`mutex_with`・`mutex_into_inner`・`shared_cell`（新規） | 「生成 IR とランタイム」 |
| LLVM | `src/llvm.rs` | builtin 呼び出しの match（`Builtin::StringIsWellFormed` の arm を持つ関数） | 14 個の F10 builtin を `llvm_sync.rs` の関数へ |
| LLVM | `src/llvm.rs` | header の `output.contains("@tsuzuri_task_parallel(")` の分岐 | 条件に `output.contains("@tsuzuri_mutex_")` を足す。native と threads の declare に `declare void @tsuzuri_mutex_lock(ptr)`・`declare void @tsuzuri_mutex_unlock(ptr)` を足す |
| runtime | `src/runtime/task.c` | `tz_task_submit`, `tsuzuri_mutex_lock`・`tsuzuri_mutex_unlock`・`tz_mutex_held`・`tz_mutex_park`・`tz_mutex_wake`・`tz_sync_trap`（新規） | D7 |
| runtime | `src/runtime/task-wasm.ll` | `@tsuzuri_task_parallel`, `@tsuzuri_task_parallel_results`, `@tsuzuri.mutex.held`・`@tsuzuri_mutex_lock`・`@tsuzuri_mutex_unlock`（新規 internal） | 入口で held なら `call void @llvm.trap()`。lock は held を立て、既に立っていれば trap |
| driver | `src/driver.rs` | `options.wasm_threads` の runtime を書く分岐（`include_str!("runtime/task-wasm-threads.c")`）の前 | IR が `@tsuzuri_mutex_lock(` を含めば E2000 |

### 生成 IR とランタイム

cell pointer（`shared_cell`）: `ref` は record を指す `ptr`。record の第 0 field は `%tz.array`（`{ ptr, i64 }`）で、その data pointer が cell。

```llvm
%cell.array = load %tz.array, ptr %ref
%cell = extractvalue %tz.array %cell.array, 0
%old = atomicrmw add ptr %cell, i64 %delta seq_cst, align 8
%value = load atomic i64, ptr %cell seq_cst, align 8
store atomic i64 %value, ptr %cell seq_cst, align 8
%pair = cmpxchg ptr %cell, i64 %expected, i64 %desired seq_cst seq_cst, align 8
```

align は要素の大きさ（1・2・4・8）。`bool` は `i8` で操作し、`zext`・`trunc` で `i1` と変換する。`compare_exchange` の `Result` は
`parallel_result_tasks` が `Ok`・`Error` を作る方法を写す。`create` は `allocate_array(element, "1")` で確保して要素 0 に格納し、record を組む。
`into_inner` は要素 0 を読み、要素を drop せずに `@tz.free` で buffer だけを解放する。

`Mutex.with`: `call void @tsuzuri_mutex_lock(ptr %cell)`、`getelementptr inbounds { i32, T }, ptr %cell, i32 0, i32 1` を `ref mut T` として
`apply_value(callback, ty, Some((ref mut T, pointer)), false)` で一回呼び、`call void @tsuzuri_mutex_unlock(ptr %cell)`。`{ i32, T }` は tuple `(u32, T)` の `llvm_type`。

native の runtime（`src/runtime/task.c`、D7）:

```c
static _Thread_local unsigned tz_mutex_held;
static pthread_mutex_t tz_mutex_park = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t tz_mutex_wake = PTHREAD_COND_INITIALIZER;
TZ_TASK_API void tsuzuri_mutex_lock(void *cell);   /* held なら tz_sync_trap。CAS 0->1、失敗なら park 下で exchange(2) != 0 の間 wait */
TZ_TASK_API void tsuzuri_mutex_unlock(void *cell); /* held = 0。exchange(0) が 2 なら park 下で broadcast */
```

`tz_task_submit` の先頭（`pthread_once` より前）で `tz_mutex_held` なら `tz_sync_trap`。lock 語の CAS・exchange は acquire、unlock の exchange は release。
park・wake の lock と wait は既存の macro（`TZ_TASK_MUTEX_LOCK`・`TZ_TASK_COND_WAIT`・`TZ_TASK_COND_BROADCAST`）と `tz_task_check` を通す。

### アルゴリズム

lock 語は 0（解放）・1（保持）・2（保持かつ待ちあり）。待ち手は park mutex の下で `exchange(state, 2)` し、0 でなければ condvar で待つ。解放側は `exchange(state, 0)` が 2 のとき
park mutex の下で broadcast する。待ち手の exchange と wait は park mutex の下で原子的なので wakeup を失わない。critical section は待たない（D7）ので、
待ち手は保持者の完了で必ず進む。`Task.scope` の join は F01 の「join 中は手伝う」規則で進む。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。重い suite は Node 24（`npx --yes --package=node@24 node ...`）で動かす。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、「再現」の 2 ケースと `/tmp/tz-work-F10/init/Main.tz`（`Parallel.init 8` の平方和）の IR を保存する。
- 確認: 次がすべて成功する。`run` は `140`、stack-depth の 3 テストと特殊化上限のテストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
target/release/tsuzuri run /tmp/tz-work-F10/init
target/release/tsuzuri build /tmp/tz-work-F10/init --emit llvm -o /tmp/tz-work-F10/before.ll
target/release/tsuzuri build /tmp/tz-work-F10/init --target wasm32 --emit llvm -o /tmp/tz-work-F10/before-wasm.ll
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
node tests/tasks.mjs target/release/tsuzuri
node tests/features.mjs target/release/tsuzuri parallel
```

### 手順 2: std record と登録

- 変更: `std/Atomic.tz`・`std/Mutex.tz`（新規）、`src/stdlib.rs`（std ファイルの表、`RESERVED_MODULES`、`opaque_record`、test `reserves_the_d07_table` の数 20 → 22）、
  `src/check.rs` の `Type::is_noncopy_record`・`Type::can_capture`・`Type::is_shared_cell`（新規）、`tests/concurrency.rs`（新規）。
- 内容: 「段ごとの変更」の std・std 登録・型性質の行。テストファイルに `tests/parallel.rs` の形の `accepts(source) -> String`（両 target の IR を 2 回出して一致）と
  `rejects(source, code)` を置く。
- 確認: `cargo test --locked --lib stdlib` が成功する。`cargo test --locked --test concurrency` が `1 passed`（`atomic_and_mutex_cells_are_opaque_non_copy_std_records`）。

### 手順 3: 組み込みクラス `Sync`・`AtomicValue`

- 変更: `src/polymorph.rs` の `BUILTIN_CLASSES`・`Classes::intrinsic`・`Classes::validate_instance`、`src/check.rs` の `Type::can_sync`・`Type::has_interior_mutability`（新規）。
- 内容: 型規則と診断の表のとおり。利用者の `instance Sync<T>`・`instance AtomicValue<T>` は `instance Send<T>` と同じ扱いにする（HEAD の挙動を `Send` で確かめ、同じ診断コードになること）。
- 確認: `cargo test --locked --test concurrency` が `2 passed`。`cargo test --locked --test polymorphism` と `cargo test --locked --test tasks` が成功する。

### 手順 4: Atomic の型付け

- 変更: `src/check.rs` の `Builtin`・`Builtin::ALL`・`Builtin::name`・`Builtin::scheme`（`AtomicCreate` 〜 `AtomicIntoInner` の 12 個）。
- 内容: API の表の型。`AtomicValue` は全操作に、`Integer` は `fetch_*` だけに付ける。LLVM はまだ `unreachable!` にしない（手順 5 で入れる）ので、この手順のテストは `analyze` だけを使う。
- 確認: `cargo test --locked --test concurrency` が `3 passed`（`atomic_value_limits_element_types`）。

### 手順 5: Atomic の lowering

- 変更: `src/llvm_sync.rs`（新規）、`src/llvm.rs` の `mod` 宣言と builtin 呼び出しの match。
- 内容: 「生成 IR とランタイム」の cell pointer と atomic 命令。`into_inner` は要素を drop しない。手順 4 の受理ケースを `accepts`（IR を出す）へ切り替える。
- 確認: `cargo test --locked --test concurrency` が `4 passed`（`atomic_codegen_uses_seq_cst_llvm_atomics_on_both_targets`）。
  既定の wasm32 で `-O0`・`-O3` の build が成功し、import が増えない（手順 11 の suite で見る。ここでは `export def` だけの scratch を
  `target/release/tsuzuri build <dir> --target wasm32 -O0 -o x.wasm` で手で 1 回。top-level の program は wasm32 に build できず E2004 になる）。

### 手順 6: `Task.scope` の型付けと所有権

- 変更: `src/check.rs` の `Builtin::TaskScope`・`Builtin::is_parallel`、`src/ownership.rs` の `E::Parallel` の arm。
- 内容: 引数 0 の共有借用は loans を許し、引数 1・2 は既存の E1013。`TypedExprKind::Parallel` を作る箇所が引数の数を `scheme().parameters.len()` で見ていることを確かめる。
  lowering は手順 7 なので、受理ケースは `analyze` だけで確かめる。
- 確認: `cargo test --locked --test concurrency` が `5 passed`（`task_scope_types_shares_and_keeps_the_ownership_boundary`）。`cargo test --locked --test parallel` が `2 passed`。

### 手順 7: `Task.scope` の lowering

- 変更: `src/llvm_parallel.rs` の `parallel_expression`・`parallel_launch`・`Kernel` の引数分岐。
- 内容: `ParallelInit` の経路を使い、chunk 数を `count` にする（子ごとに一つの group item）。callback には `(ref 's, shared)` と `(i64, index)` を順に `parallel_apply` で渡す。
- 確認: 手順 6 の受理ケースを `accepts` に切り替え、native の IR が `@tsuzuri_task_parallel(` を含む assert を足して `cargo test --locked --test concurrency` が `5 passed` のまま。
  `cargo test --locked --test parallel` が成功する。

### 手順 8: Mutex の型付けと lowering

- 変更: `src/check.rs`（`MutexCreate`・`MutexWith`・`MutexIntoInner`）、`src/llvm_sync.rs` の `mutex_with`・`mutex_into_inner`、`src/llvm.rs` の header の分岐。
- 内容: 「生成 IR とランタイム」の `Mutex.with`。header は `@tsuzuri_mutex_` を見たら native で task の 2 declare と mutex の 2 declare を出す（task.c の連結の契機を既存のまま使う）。
- 確認: `cargo test --locked --test concurrency` が `7 passed`（`mutex_with_keeps_the_exclusive_borrow_inside_the_callback`、`mutex_and_scope_codegen_use_the_task_runtime`）。

### 手順 9: native の runtime

- 変更: `src/runtime/task.c`、`tests/sync_runtime.c`（新規）、`tests/tasks.mjs`（`tests/task_runtime.c` を compile・実行する loop の隣）。
- 内容: D7 の lock 語と park、`tz_task_submit` の先頭の検査、`tz_sync_trap`。試験は `tests/task_runtime.c` と同じく `#include` 前に macro（`TZ_TASK_COND_WAIT` 等）を差し替える。
- 確認: `node tests/tasks.mjs target/release/tsuzuri` が成功する。単体では次が成功し、最後の 2 つは非 0 で終わって stderr にメッセージを出す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
clang -std=c11 -Wall -Wextra -Werror -O3 -pthread tests/sync_runtime.c -o /tmp/tz-work-F10/sync && /tmp/tz-work-F10/sync
clang -std=c11 -O1 -g -fsanitize=thread -pthread tests/sync_runtime.c -o /tmp/tz-work-F10/sync-tsan && /tmp/tz-work-F10/sync-tsan
/tmp/tz-work-F10/sync nested; /tmp/tz-work-F10/sync parallel_inside
```

### 手順 10: 既定の WASM の runtime

- 変更: `src/runtime/task-wasm.ll`、`src/llvm.rs` の header の分岐（WASM は `@tsuzuri_mutex_` でも `task-wasm.ll` を足す）。
- 内容: `@tsuzuri.mutex.held`、2 つの internal 関数、2 つの入口の検査。`@llvm.trap` は header の既存の `declare` を使い、`task-wasm.ll` に `declare` を書かない。
- 確認: `cargo test --locked --lib` が成功する（`declare void @llvm.trap()` が 1 個という既存の assert を含む）。`cargo test --locked --test tasks` が成功する。

### 手順 11: E2E suite `concurrency` と threads

- 変更: `tests/fixtures/concurrency/Main.tz`（新規）、`tests/features.mjs`（suite `concurrency`、GUIDE §7.4）、`src/driver.rs`（E2000）、`tests/wasm_threads.mjs`。
- 内容: 「テスト計画」の E2E。
- 確認: 次が成功する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
node tests/features.mjs target/release/tsuzuri concurrency
TSUZURI_TSAN=1 node tests/features.mjs target/release/tsuzuri concurrency
TSUZURI_ASAN=1 node tests/features.mjs target/release/tsuzuri concurrency
node tests/wasm_threads.mjs target/release/tsuzuri
```

### 手順 12: 回帰と生成コード

- 変更: なし。
- 内容: 手順 1 のコマンドを再実行し、`/tmp/tz-work-F10/init` の native IR が byte 単位で同じであること、WASM IR の差が `task-wasm.ll` の追加分だけであることを見る。
  `concurrency` の `atomic_counter` を `--emit object -O3` で作り、`/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d` で `ldaddal`（arm64 LSE）か `ldaxr`・`stlxr` の loop を確認する。
- 確認: `target/release/tsuzuri build /tmp/tz-work-F10/init --emit llvm -o /tmp/tz-work-F10/after.ll && diff /tmp/tz-work-F10/before.ll /tmp/tz-work-F10/after.ll` が空。
  `cargo test --locked` と `node tests/features.mjs target/release/tsuzuri` が成功する。

### 手順 13: 文書

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md _docs/language-reference/tasks.md _docs/library-reference/concurrency.md _docs/library-reference/parallel.md _docs/library-reference/README.md _docs/feature-status.md` が成功する。

## テスト計画

### Rust テスト

`tests/concurrency.rs`（新規）。受理ケースは native と wasm32 の IR を 2 回ずつ出して一致を確認する。

| テスト関数 | 内容 |
| --- | --- |
| `atomic_and_mutex_cells_are_opaque_non_copy_std_records` | `cell` の読み出しは E1022、`let copy = counter` の後の `counter` の使用は E1012、`Atomic` を捕捉する再利用可能な関数は E1005 |
| `sync_accepts_immutable_data_and_rejects_functions_tasks_and_exclusive_references` | `def share :: Sync<'a> => ref 'a -> i64` を `[i64]`・`string`・record・`Atomic<i64>`・`Mutex<[i64]>` で受理、`i64 -> i64`・`Task<i64>`・`Gpu.Device` で E1013 |
| `atomic_value_limits_element_types` | 8 種の整数と `bool` を受理、`i128`・`f64`・`char`・`string` の `Atomic.create` は E1005、`bool` の `fetch_add` は E1005 |
| `task_scope_types_shares_and_keeps_the_ownership_boundary` | 例の 2 本を受理。関数の共有は E1013、`shared` を返すと E1013、外側の借用の捕捉は E1013、`let scope = Task.scope` は E1013 |
| `mutex_with_keeps_the_exclusive_borrow_inside_the_callback` | 例を受理。`value -> value` は E1013、callback の借用の捕捉は受理 |
| `atomic_codegen_uses_seq_cst_llvm_atomics_on_both_targets` | IR に `atomicrmw add ptr`・`seq_cst, align 8`・`cmpxchg ptr`・`load atomic i8`（bool）があり、関数呼び出しの runtime 記号がない |
| `mutex_and_scope_codegen_use_the_task_runtime` | native の IR に `declare void @tsuzuri_mutex_lock(ptr)` と `declare void @tsuzuri_task_parallel(ptr, ptr, i64)`、wasm32 の IR に `@tsuzuri.mutex.held` と `define internal void @tsuzuri_mutex_lock` |

### E2E

`tests/fixtures/concurrency/Main.tz`（新規）と `tests/features.mjs` の suite `concurrency`。native・既定 WASM × `-O0`・`-O3`、`live == 0`、WASM の import なし（suite の既定の検査）。
期待値はすべて閉じた式で、スケジュールによらない。

| 関数 | 引数 | 期待値 |
| --- | --- | --- |
| `atomic_counter`（子 `index` が `fetch_add (index + 1)`） | `0, 1, 4, 64, 257` | `n(n+1)/2` |
| `atomic_compare_exchange`（`create 5`、`5 -> 9` は Ok、`5 -> 1` は Error、最後に `load`） | なし | `5 * 10000 + 9 * 100 + 9 = 50909` |
| `atomic_wrapping_i32`（`create (2147483647 as i32)` に `fetch_add 1`、`load` を `i64` へ） | なし | `-2147483648` |
| `atomic_bool_flag`（`create false`、`swap true` が false、`compare_exchange true false` が Ok、`load` が false なら 1） | なし | `1` |
| `mutex_total`（子 `index` が lock 下で `index + 1` を足し、`index` を返す。`into_inner` と結果の和） | `0, 1, 4, 64, 257` | `n(n+1)/2 + n(n-1)/2 = n²` |
| `mutex_owned_vector`（`Mutex<Vec<i64>>` に各子が push、`into_inner` の長さ） | `0, 1, 257` | `n` |
| `scope_shared_array`（`[1..n]` を共有し、各子が要素の 2 倍を返す。結果の和） | `0, 1, 4097` | `n(n+1)` |
| `scope_nested_parallel`（各子が `Parallel.init 4097 (i -> i)` の和を返す。子は 4 個） | なし | `4 * 4096 * 4097 / 2 = 33562624` |

trap: `mutex_nested`、`mutex_parallel_inside`（`Mutex.with` の中で `Task.scope`）、`scope_negative`（`count = -1`）。native の stderr は D7 のメッセージを含む。
`tests/wasm_threads.mjs`: `Atomic` と `Task.scope` だけの小さな source を threads で build して `atomic_counter` と同じ値を確かめ、fixture（`Mutex` を含む）の threads build が E2000 で失敗することを確かめる。
`tests/sync_runtime.c`（新規。`tests/tasks.mjs` から `-O0`・`-O3` で実行）:

- `exclusion`: 8 thread が 10,000 回ずつ lock・非 atomic のカウンター加算・unlock。結果 80,000。TSan build でも同じ。
- `handshake`: A が lock を持ったまま、B の lock を開始させる。差し替えた `TZ_TASK_COND_WAIT` が B の park を観測用の condvar で知らせ、main はそれを待ってから
  B が未取得であることを assert し、A を unlock させる。B の取得と join の後に取得回数 1 を assert する（時間・sleep は使わない）。
- `nested`・`parallel_inside`: 非 0 で終わり、stderr に D7 のメッセージを出す。

### 既存テストへの影響

- `src/stdlib.rs` の `reserves_the_d07_table` の `RESERVED_MODULES.len()` が 20 から 22 になる（std module の追加による正当な変更）。
- `BUILTIN_CLASSES` の長さ（型 `[&str; 25]`）が 27 になる。クラス一覧を固定で assert するテストがあれば、同じ理由で 2 名を足す。
- 既定の WASM で `Task.parallel`・`Parallel` を使う program の IR に `@tsuzuri.mutex.held` と 2 つの internal 関数が増える。native の IR と、並列を使わない program の IR は変わらない。
  ほかの期待値が変わる場合は停止条件。

### 性能

合否の閾値は設けない。手順 12 で `fetch_add` が一つの `atomicrmw`（arm64 `-O3` で `ldaddal` か LL/SC の loop）になることだけを確かめる。
`Mutex` の競合時の性能（単一の park mutex）と `Task.scope` の起動費用は計測していない。主張する前に docs/benchmarks.md の手順で計測する。

## ドキュメント

- `docs/language.md`: `## タスク` の `### 並列区間と寿命` の「共有状態…は提供しません」を「`Atomic`・`Mutex` 以外の共有状態…」に直し、直後に
  `### スコープ付き並列と共有状態`（新規）を足す（`Task.scope`、`Sync`、`Atomic`・`Mutex` の表、SeqCst、D7 の trap、既定の WASM と threads の差、決定性）。
  組み込みクラスの一覧に `Sync`・`AtomicValue`、std module の一覧に `Atomic`・`Mutex`、`E2000` の説明に threads の `Mutex` を足す。
- `docs/architecture.md`: runtime の表の `src/runtime/task.c / task-wasm.ll / task-wasm-threads.c` の行に `Mutex` の lock 語と park、LLVM の段に `src/llvm_sync.rs` を足す。
- `_docs/language-reference/tasks.md`: `Task.scope` の節と例。`_docs/library-reference/concurrency.md`（新規）: `Atomic`・`Mutex` の API 表と例。
  `_docs/library-reference/README.md` から link する。`_docs/library-reference/parallel.md`: 呼び出し元の値を共有するときは `Task.scope` を使う旨の 1 文。
- `_docs/feature-status.md` と `_features/README.md` の F10 の状態（Phase 1 完了なら「Phase 1 done」と書く既存の書き方に合わせる）。
- GUIDE D-30 から D-07・台帳への移動は承認した人間が行う。実装者は GUIDE を編集しない。

## 受け入れ条件

- [ ] D1 が承認されている。
- [ ] `tests/concurrency.rs` の 7 テストが成功し、受理ケースの IR が両 target で決定的である。
- [ ] suite `concurrency` が native・既定 WASM × `-O0`・`-O3` で成功し、`live == 0`、WASM の import がない。
- [ ] `TSUZURI_TSAN=1` と `TSUZURI_ASAN=1` の suite `concurrency`、`tests/sync_runtime.c` の TSan build が報告なしで成功する。
- [ ] `tests/sync_runtime.c` が時間に依存せず排他と park を証明し、`nested`・`parallel_inside` が D7 のメッセージで失敗する。
- [ ] `--wasm-feature threads` で `Atomic`・`Task.scope` が動き、`Mutex` は E2000 になる。
- [ ] `Mutex.with` の入れ子、`Mutex.with` の中の並列、負の `count` がすべての backend で trap する。
- [ ] F10 を使わない program の native IR が変わらない（手順 12）。stack-depth の 3 テストと `honors_the_exact_specialization_limit` が成功する。
- [ ] 文書が更新され、`node scripts/check-docs.mjs` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `new` は予約語。`Atomic.new` と書くと E0002 になる。構築は `create`。
- `Type::can_send` の fallback は関数値に true を返す。`can_sync` はこれを写さず、`Function`・`Task`・`Reference(_, true)` を明示的に false にする。
- `Type::is_noncopy_record` だけでは関数値の捕捉を止められない。`Type::can_capture` も直さないと、関数値の複製で cell が別物になる。
- atomic 命令は `i1` を扱えない。`bool` は `i8` で操作する。align は要素の大きさにし、足りない場合は停止条件。
- cell pointer を得るために record を `clone_value` してはならない。`ref` の指す record から `%tz.array` を load して data pointer を取る。
- `tz_task_submit` は CPU が 1 個のとき pool を使わずにその場で実行する。`tz_mutex_held` の検査はこの分岐より前（関数の先頭）に置かないと、1 CPU の機械でだけ trap しない。
- park の wait は必ず while で条件を再確認する（spurious wakeup）。unlock の broadcast は park mutex の下で行う。`tz_mutex_held` は lock の成功後に立て、unlock の最初に下ろす。
- `task-wasm.ll` に `declare void @llvm.trap()` を書くと重複定義になる。header の既存の declare を使い、`src/llvm.rs` が declare を消す分岐（`output.replace("declare void @llvm.trap()\n", "")`）の条件で
  `task-wasm.ll` の trap が壊れないかを手順 10 で確かめる。
- `Task.scope` に `parallel_count` を使うと、4,096 個までの子が一つの chunk で順に走る。Phase 1 の結果は同じだが、子ごとの item という契約（Phase 2 の待ちの前提）を壊す。
- テストの期待値をスケジュールに依存させない（個々の `fetch_add` の戻り値、critical section の順序を assert しない）。
- 重い suite は Node 20 で V8 が落ちることがある。Node 24 で実行する。

## 対象外

- 明示的な memory order（acquire・release・relaxed）、`RwLock`、条件変数・`try_lock` の公開、`Arc`（C10）、非同期（B08）との接続。
- `Atomic<f64>` などの浮動小数、`i128`、inline（ヒープでない）atomic 表現、ロックフリーのデータ構造の公開。
- detach されたスレッド、スレッド ID、関数値を `Sync` にすること。
- Phase 2（`Channel`、threads での `Mutex`・`Channel`）。

## 決定事項

### D1: 名前の確定

- 決定: std module `Atomic`・`Mutex`（record `Atomic<'a>`・`Mutex<'a>`）、組み込みクラス `Sync` と `AtomicValue`、builtin `Task.scope` と API の表の 15 個。構築は `create`。
- 理由: `Atomic`・`Mutex`・`Sync` は GUIDE D-30 の仮割り当て。`AtomicValue` は要素型の制約を std の汎用関数と builtin の両方で静的に表す最小の手段で、未割り当て。`new` は予約語。
- 状態: 要承認（承認前は Phase 1 のどの手順にも着手しない）

### D2: `Task.scope` の形

- 決定: `ref 's -> i64 -> (ref 's -> i64 -> 'a) -> ['a]` の同期 builtin で、完全適用だけを許す。`Task.parallel` の拡張や `Task` を返す形にはしない。
- 理由: cold な `Task` は保存でき、借用を保持させると寿命の追跡が要る。同期の呼び出しなら借用は呼び出しの間に閉じ、`Parallel.init` の型付け・所有権・lowering をそのまま使える。
- 状態: 既定案（実装者はこの案に従う）

### D3: 表現

- 決定: `Type` に variant を足さず、要素 1 個の配列 field を持つ不透明な std record にする。cell は配列の buffer で、アドレスは値の移動で変わらない。
- 理由: `Type` の全 match の変更が要らず、drop は既存の配列の drop、field の隠蔽は `opaque_record`（E1022）で済む。B07 なしで Phase 1 を出せる。
- 状態: 既定案（実装者はこの案に従う）

### D4: 複製と捕捉

- 決定: 2 つの record は `Copy` でも `Capture` でもない。`clone_value` で複製しない。Task への move（`Send`）は許す。
- 理由: 複製は独立した cell を作り、共有の意味を黙って失う。
- 状態: 既定案（実装者はこの案に従う）

### D5: memory order

- 決定: Phase 1 の全操作は SeqCst（`seq_cst`、`cmpxchg` の失敗側も `seq_cst`）。
- 理由: 利用者に C++ のメモリモデルを持ち込まずに正しい。arm64 では `ldaddal`・`ldar`・`stlr` で追加費用が小さい。明示的な order は計測で必要になってから足す。
- 状態: 既定案（実装者はこの案に従う）

### D6: `Sync` の規則

- 決定: 型規則のとおり。関数値・Task・排他参照・GPU handle・遅延列は `Sync` でない。`Mutex<T>` は `T` が `Send` なら `Sync`。
- 理由: 関数値の環境の型は見えず、GPU handle の thread 安全性は保証されていない。保守的な規則は後で緩められる。
- 状態: 既定案（実装者はこの案に従う）

### D7: `Mutex` の実装と待たない critical section

- 決定: lock 語（0・1・2）と全 Mutex 共通の park mutex・condvar。`Mutex.with` の callback の中では、別の `Mutex.with` と並列の開始を trap にする
  （native は `_Thread_local` の `tz_mutex_held`、既定の WASM は `@tsuzuri.mutex.held`）。記号は `tsuzuri_mutex_lock`・`tsuzuri_mutex_unlock`（`TZ_TASK_API`）。
- 理由: 保持者が待たなければ待ち手は必ず進み、lock 順序の deadlock も入れ子の自己 deadlock も起きない。lock 語は platform ごとの大きさの差も drop の hook も要らない。
  単一の park は競合時に余分に起こす（ponytail: 計測で問題になったら address で分割する）。
- 状態: 既定案（実装者はこの案に従う）

### D8: `--wasm-feature threads` の `Mutex`

- 決定: Phase 1 は build 時に E2000 で拒否する。`Atomic` と `Task.scope` は対応する。
- 理由: threads の worker は TLS を初期化しないので D7 の検査ができない。明示的に要求された未対応の backend は黙って劣化させずに報告する（AGENTS.md）。
- 状態: 既定案（実装者はこの案に従う）

### D9: 既定の WASM

- 決定: LLVM の atomic 命令をそのまま出し、wasm backend が非 atomic に下げる。`Task.scope` の子は index 順。import は増やさない。
- 理由: 単一 thread では非 atomic の操作が同じ意味を持ち、target ごとの分岐が要らない。
- 状態: 既定案（実装者はこの案に従う）

### D10: Phase 2 の `Channel` と待ちの規則

- 決定: 「Phase 2（設計方針）」のとおり（容量付き、待つ thread は手伝ってから park、全 thread が park したら deadlock trap、既定の WASM は index 順で即 trap、threads は TLS の初期化後）。
- 理由: 固定の pool では子ごとの専用 thread を保証できない。検出できない停止より明示的な trap を選ぶ。ただし完了するかが thread 数に依存するので、人間の判断が要る。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D11: 依存 B07

- 決定: Phase 1 は B07 を待たない。metadata の依存欄は変えない。
- 理由: D3 により Phase 1 は利用者 drop を使わない。
- 見直し提案: 依存欄を `F01, (B07: Phase 2)` にする。
- 状態: 既定案（実装者はこの案に従う）
