# TZ06: 同期（`Mutex` の移設・`RwMutex`・`WaitGroup`・`Once`・`OnceValue`・`Cond`・`Semaphore`・`SyncMap`）

| 項目 | 内容 |
| --- | --- |
| ID | TZ06 |
| 優先度 | P1 |
| 規模 | L。Phase 1（`Mutex` の移設・`WaitGroup`・`Once`・`OnceValue`・`RwMutex`）L、Phase 2（`Cond`・`Semaphore`）M、Phase 3（`SyncMap`）M |
| 依存 | TZ02 |
| 後続 | TZ07（`ErrGroup` は `WaitGroup` の上）、TZ09、TZ11（`Routine.scope` の待ち）、TZ12（旧 `Mutex` の経路の削除） |
| 状態 | todo |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61）。決定事項 D1〜D8 は既定案 |
| 手本にする既存実装 | `std/Mutex.tz`・`std/Atomic.tz`（セルを配列に置く不透明な record）、`src/llvm_sync.rs` の `MutexCreate`・`MutexWith`・`MutexIntoInner`（60〜107 行）、`src/ownership.rs` の `Mutex.with_lock` の結果の借用の検査（`MutexResultRetainsBorrow`・`MustApplyDirectly`）、`Builtin::ArcGet` の型（借用を返す組み込み。`SharedOperation::Get`）、TZ01 の parking lot（`tsuzuri_tz_addr_wait`／`addr_wake`）・`tsuzuri_tz_mutex_*`・`tsuzuri_tz_waitgroup_*`、TZ03 の `Channel` のハンドルの作り（参照数・`share`・`Drop`） |
| 主な影響ファイル | `std/WaitGroup.tz`・`std/RwMutex.tz`・`std/Once.tz`・`std/OnceValue.tz`・`std/Cond.tz`・`std/Semaphore.tz`・`std/SyncMap.tz`（新規）、`src/check.rs`・`src/llvm_sync.rs`・`src/llvm.rs`・`src/ownership.rs`・`src/stdlib.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/diagram/concurrency.rs`・`src/runtime/tzroutine.c`・`tests/tzroutine_runtime.c`・`tests/sync_types.rs`（新規）・`tests/fixtures/sync_types/Main.tz`（新規）・`tests/tzroutine.mjs`・`tests/std_argument_order.rs`・`tests/std_examples.rs`・`vsc/src/core.ts`・文書 |

## 実装者向け早見

- 範囲: Go の `sync`（`Mutex`・`RWMutex`・`WaitGroup`・`Once`・`OnceValue`・`Cond`・`Map`）と `x/sync/semaphore`。待つのは tzroutine だけ。値を守るロックは、値を持つ型（`Mutex<T>`・`RwMutex<T>`）で、ロックの中でだけ値に触れる（Go の `sync.Mutex` は値を持たないが、Tsuzuri は型で競合を拒むため）。
- 既存の `Mutex<T>`（F10）は、**tzroutine を使うプログラムでは**新しいランタイムのロック（待つ G だけが止まる）で生成する。使わないプログラムの生成は変えない（TZ12 で旧の経路を消す）。
- 最初に読む: ① 仕様 1・2 ② `src/llvm_sync.rs` 1〜110 行 ③ TZ01 の仕様 1.11 と付録 B-1 の parking lot ④ TZ03 の仕様 4（ハンドルの生成・drop）。
- 最初に書くテスト: `tests/tzroutine_runtime.c` に `rwmutex`・`once`・`cond`・`semaphore`（C だけ）→ `tests/sync_types.rs`（型）→ E2E の `wait_group_sum`。
- 触ってはいけない: tzroutine を使わないプログラムの `Mutex`・`Atomic` の IR と挙動（入れ子のトラップ、`Channel` の操作のトラップ、`--trap-mode return` の解放）。`Atomic` のすべて。
- 完了の見分け方: 次が `sum=4950` を出す（TZ02 の早見の例の、`Atomic` の数えの待ちを `WaitGroup` に置き換えたもの）。

```tsuzuri
let wg = WaitGroup.create ()
let total = Arc.new (Mutex.create 0i64)
for i in 0 ..< 100 do
    WaitGroup.add wg 1
    let done = WaitGroup.share wg
    let mine = Arc.share (ref total)
    tzroutine {
        Mutex.with_lock (Arc.get (ref mine)) (\value -> deref value = deref value + (i as i64))
        WaitGroup.done done
    }
WaitGroup.wait wg
$"sum={Mutex.with_lock (Arc.get (ref total)) (\value -> deref value)}"
```

## 目的

共有の状態とライフサイクルの待ち合わせを、tzroutine の待ち（OS スレッドを止めない待ち）で書けるようにする。`WaitGroup` は TZ02 の「数えて `yield` で待つ」形を置き換え、`Once`・`OnceValue` は一度だけの初期化、`RwMutex` は読みの多い共有、`Cond` は条件の待ち、`Semaphore` は同時実行の数の制限、`SyncMap` は並行のキャッシュ。

## 着手条件と停止条件

### 着手条件

- TZ02 が done。基準: `node tests/features.mjs target/debug/tsuzuri concurrency`（旧 `Mutex`・`Atomic`）の結果、`node tests/tasks.mjs`、`cargo nextest run --locked` の件数。

### 停止条件

- **S1** tzroutine を使わないプログラムの `Mutex` の IR か挙動が変わる（`tests/concurrency.rs`・`features.mjs` の `concurrency`・`tests/trap_return_sync.mjs`）。
- **S2** 新しい型の借用を返す組み込み（`OnceValue.get`）を、`Arc.get` と同じ仕組みで型付けできない。選択肢: A 値を返す `OnceValue.get_copy`（`Copy<'a>`）とコールバックの `OnceValue.with` にする、B 止めて報告する。
- **S3** `RwMutex`・`Cond` の C のテストが TSan で報告を出す、または 2 回直して直らない。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認）

- `Mutex<'a>` は `record Mutex<'a> { cell: [(i32u * 'a)] }`（`std/Mutex.tz`）。`with_lock` は `call i32 @tsuzuri_mutex_lock(ptr cell)`（`task.c` の CAS と 1 つの全体の mutex・条件変数による待ち）→ 非 0（入れ子）ならトラップ → コールバックを `ref mut` の値で呼ぶ → `@tsuzuri_mutex_unlock`。入れ子の検査はスレッドローカルの `tz_mutex_held`（`task.c` 508 行）。ロックの中の `Task.parallel` と `Channel` の操作もトラップ（`tsuzuri_mutex_parallel_ok`・`wait_ok`）。
- `Atomic<'a>` は `{ cell: ['a] }` で、LLVM の atomic 命令（`seq_cst`）。ランタイムを使わない。
- `WaitGroup`・`RwMutex`・`Once`・`Cond`・`Semaphore`・並行のマップはない。共有の初期化は `Mutex<Maybe<T>>` で書くしかない。
- TZ01 のランタイムに `tsuzuri_tz_addr_wait`（64 bit の語）・`addr_wake`・`mutex_lock`／`unlock`（64 bit の語）・`waitgroup_add`／`wait` がある。

## 変えてはいけない意味

- tzroutine を使わないプログラムの `Mutex` のすべて（TZ12 まで）。`Atomic` のすべて。
- `Mutex.with_lock` の型と、結果の借用の検査（`E1013`）。
- 値の意味: ロックの中の変更は、次にロックを取った者に見える（acquire／release）。

## Phase 分割

| Phase | 内容 |
| --- | --- |
| 1 | `Mutex` の移設、`WaitGroup`、`Once`、`OnceValue`、`RwMutex` |
| 2 | `Cond`、`Semaphore` |
| 3 | `SyncMap`（std の Tsuzuri で書く） |

## 仕様

### Phase 1

#### 0. ターゲット

- native の macOS・Linux（Windows は TZ01 の Phase 2 の後）。
- 既定の WASM: TZ03 が done なら、tzroutine を生成しないプログラムの新しい同期の型（`WaitGroup`・`Once`・`OnceValue`・`RwMutex`・`Cond`・`Semaphore`・`SyncMap`）は、TZ03 の `TZR_SINGLE`（1 本の実行）で使える。待つべき操作は即座にデッドロックの判定で、呼び出し地点でトラップする。TZ03 がまだなら、WASM は TZ02 の拒否のまま（このチケットで `TZR_SINGLE` を作らない）。`Mutex` は既定の WASM では今までどおり `sync-wasm.ll`（TZ12 まで）。
- `WaitGroup.go` など tzroutine を生成するものは、WASM では TZ02 の拒否（`jspi` は TZ09）。

#### 1. `Mutex<T>`（既存の API のまま、tzroutine を使うプログラムでの生成を変える）

- 生成コードがランタイムの `tsuzuri_tz_mutex_lock(ptr word, ptr node)`／`tsuzuri_tz_mutex_unlock(ptr word, ptr node)` を呼ぶ（`node` は生成コードの entry の `alloca` の 16 バイト `{ ptr word, ptr next }`。G ごとの「持っているロック」の連結リストの節）。語は既存のセルの `i32u`（32 bit。TZ01 の 64 bit の関数とは別に 32 bit 版を足す。設計）。
- 同じ `Mutex` を、持っている tzroutine がもう一度 `with_lock` すると、`Tsuzuri runtime: Mutex.with_lock cannot be nested` を書いてトラップ（`assert`。`tsuzuri_tz_report(4)`）。**違う** `Mutex` の入れ子は許す（Go と同じ。順序の誤りによるデッドロックは判定が見つける）。
- ロックを持ったまま `Channel` の待ち・`select`・`WaitGroup.wait` などで待ってよい（Go と同じ。待つのは G で、ロックは OS スレッドに結び付かない）。旧の「ロックの中の `Channel`・`Task.parallel` はトラップ」は、tzroutine を使うプログラムでは使わない（`Task` や `DataChannel`（旧 `Channel`。TZ12 まで OS スレッドを止めて待つ）を同じプログラムで混ぜた場合も、新しいロックでは検査しない。TZ02 の共存の規則）。
- 待ち: 速い経路は compare-exchange、混んでいれば 32 回再試行、そのあと parking lot（理由 `Mutex lock`）。公平さは保証しない（Go の starvation mode は作らない。D5）。

#### 2. `WaitGroup`

| 関数 | 型 | 意味 | Go |
| --- | --- | --- | --- |
| `create` | `unit -> WaitGroup` | 数 0 | `var wg sync.WaitGroup` |
| `add` | `ref WaitGroup -> i64 -> unit` | 数に足す。負になったら `Tsuzuri runtime: negative WaitGroup counter` を書いてトラップ | `wg.Add(n)` |
| `done` | `ref WaitGroup -> unit` | `add wg (-1)` | `wg.Done()` |
| `wait` | `ref WaitGroup -> unit` | 数が 0 になるまで待つ（理由 `WaitGroup.wait`） | `wg.Wait()` |
| `go` | `ref WaitGroup -> own Owned.Function<unit, unit> -> unit` | `add 1` してから、関数を新しい tzroutine で呼び、終わったら `done` | `wg.Go(f)`（Go 1.25） |
| `share` | `ref WaitGroup -> WaitGroup` | 同じ数のハンドルをもう 1 つ（tzroutine へ移すため） | — |

- `WaitGroup` は不透明なハンドル `{ block: i64 }`（ランタイムが `tsuzuri_alloc` で確保する 16 バイト `{ refs, count }`）。最後の drop で解放。`Send`・`Sync`、非 Copy、`Capture` でない。
- `go` の tzroutine は、`go` を呼んだ G の生成スコープで数える（`Builtin::spawns()` が真）。tzroutine の側が `WaitGroup` の参照を 1 つ持ち、終わりで離す。

#### 3. `Once` と `OnceValue<T>`

| 関数 | 型 | 意味 | Go |
| --- | --- | --- | --- |
| `Once.create` | `unit -> Once` | — | `var once sync.Once` |
| `Once.call` | `ref Once -> (unit -> unit) -> unit` | 最初の 1 回だけ関数を呼ぶ。ほかの呼び出しは、最初の呼び出しが終わるまで待つ（理由 `Once.call`） | `once.Do(f)` |
| `Once.done` | `ref Once -> bool` | もう呼ばれ終わったか | — |
| `Once.share` | `ref Once -> Once` | ハンドルの複製 | — |
| `OnceValue.create` | `unit -> OnceValue<'a>` | 空 | — |
| `OnceValue.get` | `(Send<'a>, Sync<'a>) => ref OnceValue<'a> -> (unit -> 'a) -> ref 'a` | 最初の 1 回だけ関数で値を作り、以後はその値を借りて返す（`OnceValue` の借用と同じ寿命） | `sync.OnceValue(f)` |
| `OnceValue.into_inner` | `own OnceValue<'a> -> Maybe<'a>` | 作られていれば値 | — |

- `Once` はハンドル（`WaitGroup` と同じ形）。`OnceValue<'a>` は値を持つセル（`Mutex` と同じく `{ cell: [(i32u * Maybe<'a>)] }`。共有は `Arc<OnceValue<'a>>`）。
- 関数（`unit -> unit`・`unit -> 'a`）は普通の関数値で、呼び出しの間だけ使う（借用を捕捉してよい。`Mutex.with_lock` のコールバックと同じ）。
- 状態の語: 0 未実行・1 実行中・2 完了。実行中の関数の中から同じ `Once`・`OnceValue` を呼ぶと自分を待つ（判定が見つける。D6）。
- `OnceValue.get` の結果の借用は、`Arc.get` の結果と同じ規則（引数の借用の寿命を持つ）。

#### 4. `RwMutex<T>`

| 関数 | 型 | 意味 | Go |
| --- | --- | --- | --- |
| `create` | `Send<'a> => own 'a -> RwMutex<'a>` | — | `var mu sync.RWMutex` |
| `with_read` | `ref RwMutex<'a> -> (ref 'a -> 'b) -> 'b` | 読みのロック（複数の読み手が同時に持てる） | `mu.RLock()` |
| `with_write` | `ref RwMutex<'a> -> (ref mut 'a -> 'b) -> 'b` | 書きのロック（1 人だけ。読み手もいない） | `mu.Lock()` |
| `into_inner` | `own RwMutex<'a> -> 'a` | 値を取り出す | — |

- セル `{ cell: [(i64u * 'a)] }`。語のビット: 下位 62 bit が読み手の数、bit 62 が「書き手が待っている」、bit 63 が「書き手が持っている」。書き手が待っている間、新しい読み手は待つ（書き手を飢えさせない。Go と同じ方針）。
- 同じ `RwMutex` の中で `with_read` の中から `with_write`（またはその逆、`with_write` の入れ子）は自分を待つ。`Mutex` と同じく、持っているロックの連結リストで見つけてトラップする（`tsuzuri_tz_report(5)`: `RwMutex cannot be locked again by the tzroutine that holds it`）。`with_read` の中の `with_read` も同じくトラップ（書き手が待っていると自分を待つため。Go の文書の注意と同じ）。
- `with_write` の結果の借用の検査は `Mutex.with_lock` と同じ（`E1013`）。

### Phase 2

#### 5. `Cond`

| 関数 | 型 | 意味 | Go |
| --- | --- | --- | --- |
| `create` | `unit -> Cond` | — | `sync.NewCond(&mu)` |
| `wait_with` | `ref Cond -> ref Mutex<'a> -> (ref 'a -> bool) -> (ref mut 'a -> 'b) -> 'b` | ロックを取り、述語が真になるまで「ロックを外して待ち、起こされたら取り直して述語を見直す」を繰り返し、真になったら本体を呼んでロックを外す | `for !cond() { c.Wait() }` |
| `signal` | `ref Cond -> unit` | 待つ者を 1 人起こす | `c.Signal()` |
| `broadcast` | `ref Cond -> unit` | 全員起こす | `c.Broadcast()` |
| `share` | `ref Cond -> Cond` | ハンドルの複製 | — |

- 語は 64 bit の通し番号。待つ側: ロックの下で述語を見る → 偽なら番号を読み、ロックを外し、`addr_wait(番号)`、ロックを取り直す。起こす側: 番号を 1 増やして `addr_wake(1)`／`addr_wake(-1)`。述語を見直すので、余分な起床は害がない。
- `Cond` はどの `Mutex` とも組める（Go の `c.L` は生成時に固定だが、Tsuzuri は呼び出しごとに渡す）。

#### 6. `Semaphore`（`x/sync/semaphore.Weighted`）

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `create` | `i64 -> Semaphore` | 容量 `n`（0 以上。負はトラップ） |
| `acquire` | `ref Semaphore -> i64 -> unit` | `w` が空くまで待つ（`w` が容量より大きい・負ならトラップ） |
| `try_acquire` | `ref Semaphore -> i64 -> bool` | 待たない |
| `release` | `ref Semaphore -> i64 -> unit` | 返す。容量を超えるとトラップ（`tsuzuri_tz_report(6)`: `Semaphore released more than it acquired`） |
| `share` | `ref Semaphore -> Semaphore` | ハンドルの複製 |

- 語は空いている量。`acquire` は compare-exchange、足りなければ `addr_wait`。`release` は足して `addr_wake(-1)`。**先着順は保証しない**（Go の `Weighted` は先着順。D7）。

### Phase 3

#### 7. `SyncMap<K, V>`（`std/SyncMap.tz`。Tsuzuri で書く）

- `record SyncMap<'k, 'v> { inner: RwMutex<HashMap<'k, 'v>> }`。`create`・`load :: (Copy<'v>) => ref SyncMap<'k,'v> -> ref 'k -> Maybe<'v>`・`with_value :: ref SyncMap<'k,'v> -> ref 'k -> (ref 'v -> 'b) -> Maybe<'b>`・`store :: ref SyncMap<'k,'v> -> own 'k -> own 'v -> unit`・`load_or_store`・`delete :: ref SyncMap<'k,'v> -> ref 'k -> Maybe<'v>`・`length`・`for_each :: ref SyncMap<'k,'v> -> (ref 'k -> ref 'v -> unit) -> unit`（読みのロックの中で呼ぶ。中から同じ `SyncMap` の書き込みをするとトラップ）。`'k` は `HashMap` の制約（`Hash`・`Eq`）と `Send`・`Sync`。
- 書く前に `HashMap` の API（`std/HashMap.tz`）を読み、`with_write` の中で値を更新できる関数（`insert` の消費・再構築の形でよい）を確かめる。G35 の `TAGGED` に `SyncMap` を入れるかは `HashMap` と同じ扱い（`@complexity`）。

## 設計

### ランタイム（`src/runtime/tzroutine.c`）

| 足す関数（`TZR_API`） | 中身 |
| --- | --- |
| `int32_t tsuzuri_tz_addr_wait32(_Atomic uint32_t *addr, uint32_t expected)` | `tzr_addr_wait` の比較を 32 bit にした版（parking lot のバケットは同じ。内部の `tzr_addr_wait` に「幅」の引数を足して 64・32 の両方を扱う） |
| `int32_t tsuzuri_tz_mutex32_lock(_Atomic uint32_t *word, tzr_held *node)`・`void tsuzuri_tz_mutex32_unlock(_Atomic uint32_t *word, tzr_held *node)` | 既存の 64 bit 版と同じ 3 状態。`lock` は、現在の G の `held` の連結リストに `word` があれば 1（生成コードが `report(4)` + トラップ）。なければ取ってから `node = { word, g->held }` を `g->held` の先頭に入れる。`unlock` は `g->held` から `node` を外してから解放。待ちの理由 `Mutex lock` |
| `tsuzuri_tz_rw_read_lock`・`read_unlock`・`write_lock`・`write_unlock`（`_Atomic uint64_t *word, tzr_held *node`） | 仕様 4 のビット。読み: `(s & (W_ACTIVE \| W_WAITING)) == 0` なら `s + 1` の CAS、違えば `addr_wait(word, s)`。読みの解放: `fetch_sub(1)` の後、読み手が 0 で `W_WAITING` なら全員起こす。書き: `(s & (W_ACTIVE \| READERS)) == 0` なら `(s & ~W_WAITING) \| W_ACTIVE` の CAS、違えば `W_WAITING` を立てる CAS をしてから `addr_wait(word, s \| W_WAITING)`。書きの解放: `store(0)`、全員起こす（待っていた書き手は自分で `W_WAITING` を立て直す） |
| `tsuzuri_tz_handle_new(int64_t initial)`・`handle_share(void *)`・`handle_release(void *)`（最後なら 1） | `WaitGroup`・`Once`・`Cond`・`Semaphore` のブロック `{ _Atomic uint64_t refs; _Atomic uint64_t word; }`（`tsuzuri_alloc(16)`、`refs = 1`、`word = initial`）。解放は `tsuzuri_free` |
| `tsuzuri_tz_waitgroup_go(void *wg, void (*run)(void *), void *box)` | `add(+1)`、`share(wg)`、`{ wg, run, box }` を `malloc` して `tsuzuri_tz_go(tzr_wg_entry, that)`。`tzr_wg_entry` は `run(box)`、`waitgroup_add(-1)`、`handle_release(wg)`（最後なら `tsuzuri_free`）、`free(that)` |
| `tsuzuri_tz_once_begin(_Atomic uint32_t *word)` → 1（自分が呼ぶ）/ 0（もう完了）、`tsuzuri_tz_once_end(_Atomic uint32_t *word)` | `begin`: 2 なら 0。0 → 1 の CAS に勝てば 1。負ければ 2 になるまで `addr_wait32(word, 1)`（理由 `Once.call`）して 0。`end`: `store(2)`、全員起こす |
| `tsuzuri_tz_cond_wait(_Atomic uint64_t *cond, _Atomic uint32_t *mutex, tzr_held *node, uint64_t seq)` | `mutex32_unlock` → `addr_wait(cond, seq)` → `mutex32_lock`（理由 `Cond.wait_with`）。`seq` は生成コードがロックの下で読んだ値 |
| `tsuzuri_tz_report` の文 | 5: `RwMutex cannot be locked again by the tzroutine that holds it`、6: `Semaphore released more than it acquired`（TZ03 の表の末尾に足す） |

- `tzr_g` に `tzr_held *held` を足す（擬似 G も持つ）。`tzr_held` は `{ void *word; struct tzr_held *next; }`（16 バイト。生成コードの `alloca`）。
- 64 bit の `tsuzuri_tz_mutex_lock`（TZ01）はそのまま残す（使わなくてよい）。

### コンパイラ

| ファイル | 変更 |
| --- | --- |
| `src/llvm.rs` | `uses_tzroutine(module)`（TZ02 の `spawns` と同じ歩き方で、`Routine*`・`Channel*`・この表の組み込み・`RoutineGo`・`Select` のどれかがあるか）を `emit_with_options` の最初に求め、`globals.tzroutine` に置く |
| `src/llvm_sync.rs` | `MutexWith`: `globals.tzroutine` なら `%node = alloca { ptr, ptr }`（entry）、`call i32 @tsuzuri_tz_mutex32_lock(ptr cell, ptr %node)`、非 0 なら `report(4)` + トラップ、コールバック、`call void @tsuzuri_tz_mutex32_unlock(ptr cell, ptr %node)`。偽なら今の生成のまま（S1） |
| `src/llvm_sync.rs` | `RwMutex*`・`WaitGroup*`・`Once*`・`OnceValue*`・`Cond*`・`Semaphore*` の生成（`Mutex` の生成と同じ形。ハンドルの型は TZ03 の `Channel` と同じく `{ i64 }` の record と `Drop`） |
| `src/check.rs` | 組み込みの宣言・名前・型（仕様の表）。`OnceValue.get` は `Builtin::ArcGet` と同じ借用の結果（`SharedOperation::Get` の分類を共有するか、同じ型の書き方）。新しい record を `Sync` に（`is_unsync_record` に入れない）、`Capture` でない（`Drop` を持つ） |
| `src/ownership.rs` | `RwMutex.with_write`・`Cond.wait_with` の結果の借用の検査を `Mutex.with_lock` の検査（`locked_result`）に足す。`with_read` は読みの借用だけを渡すので、結果が借用を持てば同じく `E1013` |
| `src/stdlib.rs` | 新しい std のファイル・`OPT_IN`・`RESERVED_MODULES` |
| `tests/std_argument_order.rs` | 新しいモジュールを `Exempt(BUILTIN_HANDLE)`（`SyncMap` も、ハンドルが先頭の形なので同じ） |
| `src/effects.rs` ほか | 旧 `Mutex*` と同じ分類。`WaitGroup.go` は `Builtin::spawns()` が真、`task` |
| `src/diagram/concurrency.rs` | `CELL_EVIDENCE`・`CREATORS` に `RwMutex.create`・`WaitGroup.create`・`Once.create`・`OnceValue.create`・`Cond.create`・`Semaphore.create`・`SyncMap.create` |

## 前提とする他チケットのインターフェース

- TZ01 の parking lot・`tsuzuri_tz_go`・スコープ。TZ02 の `Builtin::spawns`・`@tz.tzroutine.owned`。TZ03 の `tsuzuri_tz_report` とハンドルの作り。

## 他チケットへ提供するインターフェース

- `WaitGroup`（TZ07 の `ErrGroup` が使う）、`tsuzuri_tz_handle_*`（TZ07 の `Context` のハンドルにも使う）、`uses_tzroutine`（TZ12 が旧の経路を消すときの分岐点）。

## 実装手順

### Phase 1

1. **変更**: ブランチ `feat/tz06-sync`、基準。
2. **変更**: ランタイム（Phase 1 の行）と C のテスト（`mutex32_nested`・`rwmutex`（読み手 8・書き手 2 で、書きの間に読み手がいないことを `Atomic` で検査）・`once`（16 個が同時に呼んで関数は 1 回）・`waitgroup_go`・`handle`）。**確認**: `TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`。
3. **変更**: `uses_tzroutine` と `Mutex` の移設。**確認**: tzroutine を使わない `Mutex` のプログラムの IR が変わらない（`tests/concurrency.rs`、`features.mjs concurrency`、`trap_return_sync.mjs`）。tzroutine を使うプログラムの IR が `tsuzuri_tz_mutex32_lock` を含む。
4. **変更**: `WaitGroup`・`Once`・`OnceValue`・`RwMutex` の std・組み込み・生成。**確認**: `tests/sync_types.rs`（型、`E1022`・`E1005`・`E1013`）。
5. **変更**: E2E（下の表）。**確認**: `node tests/tzroutine.mjs target/debug/tsuzuri`、`features.mjs sync_types`。
6. **変更**: 文書と規則 ID、T2、CI（`ci:full`）。

### Phase 2・3

- 同じ順（ランタイム → C のテスト → std・組み込み → E2E → 文書）。

## テスト計画

| E2E | 内容 | 期待 |
| --- | --- | --- |
| `wait_group_sum` | 早見の例（`n` = 0・1・100・10000） | `n(n-1)/2` |
| `wait_group_go` | `WaitGroup.go` で 1,000 個 | 合計、`live == 0` |
| `negative_counter` | `done` を 1 回多く | トラップと文 `negative WaitGroup counter` |
| `mutex_hold_across_wait` | `with_lock` の中で `Channel.recv`（ほかの tzroutine が送る） | 完了する（旧の `Mutex` はトラップした） |
| `mutex_same_nested` | 同じ `Mutex` の入れ子 | トラップと文 |
| `mutex_different_nested` | 2 つの `Mutex` を同じ順で入れ子 | 完了 |
| `mutex_order_deadlock` | 2 つの tzroutine が逆の順で 2 つの `Mutex` を取る（チャンネルで同時に取らせる） | デッドロックの判定（`Mutex lock`） |
| `once_once` | 32 個の tzroutine が `Once.call` | 関数は 1 回、全員が完了後の状態を見る |
| `once_value` | `OnceValue.get` の値を全員が読む | 同じ値、関数は 1 回 |
| `rw_readers` | 読み手 8・書き手 2、各 1,000 回 | 書きの間に読み手が 0（`Atomic` で数える）、最終の値 |
| `cond_queue`（Phase 2） | `Cond.wait_with` で空でないのを待つキュー（`Mutex<Vec<i64>>`） | 全要素を受ける |
| `semaphore_limit`（Phase 2） | 容量 3 で 20 個の tzroutine が同時の数を `Atomic` の最大で記録 | 最大 ≤ 3 |
| `sync_map`（Phase 3） | 8 個が `store`・`load` | 期待の内容 |

- すべて native の `-O0`・`-O3`、P 1 と 4、`live == 0`。TZ03 が done なら、tzroutine を生成しないケース（`once_once` を 1 本にしたもの、`semaphore` の `try_acquire`、`OnceValue.get`）を既定の WASM の `-O0`・`-O3` でも回す（import なし）。

## ドキュメント

- 新規 `built-in-types-and-modules/waitgroup.md`・`rwmutex.md`・`once.md`・`oncevalue.md`・`cond.md`・`semaphore.md`・`syncmap.md`（`stdPage` の名前）。各ページに Go との対応表と、値を持つロックである理由（Phase ごと）。
- `mutex.md`: tzroutine を使うプログラムでのロック（待つのは G だけ、ロックを持ったまま待てる、同じロックの入れ子だけがトラップ）。旧の規則は「tzroutine を使わないプログラム」として残す（TZ12 で消す）。
- `async-tasks-and-lazy/tzroutine.md`: 終わりの待ち合わせを `WaitGroup` の例に置き換える（TZ02 の `Atomic` の数えの例は消す）。
- `types-and-type-inference/constraints.md`（`Sync` の表に新しい型）、`index.md`、`docs/language.md`、`docs/architecture.md`。
- `spec/rules/TSK.md`: `TSK-026`（`WaitGroup.wait` MUST return only after the counter is zero.）、`TSK-027`（`Once.call` MUST run its function at most once.）、`TSK-028`（A tzroutine MAY wait while it holds a `Mutex`.）など。

## 受け入れ条件

- [ ] 各 Phase の仕様のとおりに動き、`live == 0`。
- [ ] tzroutine を使わないプログラムの `Mutex` の IR・挙動が変わらない（S1）。
- [ ] C のテストが TSan・ASan で通る。
- [ ] 文書・規則 ID を更新した。

## 落とし穴

- **`held` の節は生成コードの `alloca`**: G のスタックの上にあるので、`with_lock` の範囲より長く `g->held` に残してはいけない（`unlock` で必ず外す。コールバックがトラップしてもプロセスは終わるので、残っても害はない）。
- **擬似 G の `held`**: `main` の擬似 G も `Mutex` を使う。`tzr_current()` は擬似 G を返すので、同じ規則が働く。
- **`OnceValue` の値の可視性**: 値を書いてから状態 2 を `release` で、読む側は `acquire` で読む（生成コードの load を `load atomic … acquire` にする。普通の load にしない）。
- **`RwMutex` の全員起こし**: 書きの解放で全員を起こす（読み手と書き手の両方）。1 人だけ起こすと、起きた読み手が待つ書き手を起こさず止まることがある。
- **`WaitGroup.add` を `wait` の後に**: Go と同じく、`wait` が戻る前に数を 0 から増やす使い方は誤り（`wait` の待ち手が戻らないことがある）。文書に書く（実行時の検査はしない。D8）。

## 対象外

- `sync.Pool`（D-61: GC がない）、`atomic.Value`・`atomic.Pointer`（`Mutex<T>`・`Arc<T>`）。
- `Mutex` の公平さ（starvation mode）、`Semaphore` の先着順（D5・D7）。
- ロックの所有者を変える操作（Go の別の goroutine からの `Unlock`。Tsuzuri はコールバックの範囲で必ず同じ G が解放する）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | ロックは値を持つ（`Mutex<T>`・`RwMutex<T>`）。値を持たない Go の `sync.Mutex` は作らない | 共有の変更は型で守る（`Sync`）。値のないロックは何も守れない | 既定案 |
| D2 | `WaitGroup`・`Once`・`Cond`・`Semaphore` は参照数のハンドル（`share`）。値を持つものは `Arc` で共有 | 待ち合わせの道具は多くの tzroutine へ配るので、`Arc` を毎回書かなくてよい形にする（`Channel` と同じ） | 既定案 |
| D3 | `Mutex` の移設は tzroutine を使うプログラムだけ（TZ12 で全部に） | 既存のプログラムの生成と挙動を TZ12 まで変えない | 既定案 |
| D4 | 同じロックの入れ子だけをトラップし、違うロックの入れ子は許す | Go と同じ。自分を待つ確実な誤りだけを早く見つける | 既定案 |
| D5 | `Mutex` の公平さは保証しない | 実装を小さく保つ。必要なら計測付きの別チケット | 既定案 |
| D6 | `Once` の関数の中からの同じ `Once` の呼び出しは検査しない（自分を待つ） | Go と同じ。判定が見つける | 既定案 |
| D7 | `Semaphore` の先着順は保証しない | parking lot の全員起こしで足りる。Go の `Weighted` の先着順は別チケット | 既定案 |
| D8 | `WaitGroup` の誤用（`wait` 中の 0 からの `add`）は検査しない | Go も実行時に一部しか見つけない | 既定案 |
