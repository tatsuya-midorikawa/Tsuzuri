# TZ05: 時間（`Time.sleep`・`Time.after`・`Time.tick`・`Timer`・`Ticker`・`Time.after_func`）

| 項目 | 内容 |
| --- | --- |
| ID | TZ05 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | TZ03 |
| 後続 | TZ07（`Context.with_timeout`）、TZ08（netpoller とタイマーの統合）、TZ09（WASM の時計） |
| 状態 | todo |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61）。決定事項 D1〜D7 は既定案 |
| 手本にする既存実装 | `std/Time.tz`（`IO` を返す関数、`Os.__clock`・`Os.__sleep`、`Os.Error`）、`src/runtime/os.c` の `tsuzuri_os_sleep`（837 行）、TZ01 の `tzr_timers`（タイマーのヒープとスレッド。付録 B-1）、TZ03 の `Channel`、TZ02 の `Routine.go` の補助関数 `@tz.tzroutine.owned`、`std/Channel.tz` の不透明な record と `Drop` |
| 主な影響ファイル | `std/Time.tz`・`std/Timer.tz`（新規）・`std/Ticker.tz`（新規）・`src/check.rs`・`src/llvm_io.rs`（または `src/llvm_channel.rs`）・`src/stdlib.rs`・`src/capabilities.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/runtime/tzroutine.c`・`src/runtime/os.c`・`tests/tzroutine_runtime.c`・`tests/timers.rs`（新規）・`tests/fixtures/timers/Main.tz`（新規）・`tests/tzroutine.mjs`・`tests/std_argument_order.rs`・`tests/std_examples.rs`・`tests/fixtures/capabilities/std-table.txt`・文書 |

## 実装者向け早見

- 範囲: Go の `time.Sleep`・`time.After`・`time.Tick`・`time.NewTimer`・`time.NewTicker`・`time.AfterFunc` と時間の定数。待つのは tzroutine だけ（`main` の擬似 G はスレッドを止める）。既存の `Time.sleep_ms` も、tzroutine の中ではスレッドを止めずに待つようにする。
- 最初に読む: ① 仕様 1〜3 ② TZ01 の付録 B-1 のタイマーの節（`tzr_timer`・`tzr_timerproc`・`tsuzuri_tz_sleep`）③ `std/Time.tz`（45 行）④ TZ03 の仕様 4（チャンネルの生成）。
- 最初に書くテスト: `tests/tzroutine_runtime.c` に `timer_channel`（C だけでタイマーのチャンネルが 1 回届く）→ `tests/timers.rs` の型 → E2E の `after_is_a_timeout`。
- 触ってはいけない: `Time.monotonic_ns`・`unix_ns`・`sleep_ms` の型と結果（`sleep_ms` の負の数は `InvalidInput`）。tzroutine を使わないプログラムの IR（`sleep_ms` の IR を含む）。
- 完了の見分け方: 次が `timeout` を出す（`never` に誰も送らない）。

```tsuzuri
let never: Channel<i64> = Channel.make 0
let! deadline = Time.after (50 * Time.millisecond)
let result =
    select
    | recv _ from never -> "value"
    | recv _ from deadline -> "timeout"
do! IO.write_line result
```

## 目的

タイムアウト・周期の処理・遅延した実行は、`select`（TZ04）と組み合わさって初めて Go の書き方になる。時間をチャンネルとして渡す Go の API を、GC なしで正しく解放される形で入れる。

## 着手条件と停止条件

### 着手条件

- TZ03 が done。TZ04 が done なら `select` の E2E も書く（TZ04 がなければ、`select` を使う例とテストは TZ04 の後に足す。手順 6）。
- `grep -n "tsuzuri_alloc\|tsuzuri_free" src/llvm_abi.rs src/runtime/*.c` で、C から Tsuzuri の確保（`live` で数える）を呼ぶ関数の名前と型を確かめる（`async.c` が使っている）。

### 停止条件

- **S1** タイマーのチャンネルのブロックを、C のランタイムが Tsuzuri の allocator で確保・解放できない（`tsuzuri_alloc`・`tsuzuri_free` がない、`--allocator` の種類によって違う関数を呼ぶ必要がある）。選択肢: A 生成コードが確保して C へ渡す、B 止めて報告する。
- **S2** 確保の追跡で `live != 0`（止めなかったタイマー、境界を出た後のタイマー）。2 回直して直らなければ止める。
- **S3** `Time.sleep_ms` を使う既存のプログラム（`tests/os.mjs`・`examples/`）の結果か IR が変わる。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認）

- `std/Time.tz` は `monotonic_ns`・`unix_ns`・`sleep_ms` の 3 つ。すべて `IO<Result<…, Os.Error>>`。`sleep_ms` は `Os.__sleep` → `tsuzuri_os_sleep`（`nanosleep` の繰り返し）で、OS スレッドを止める。権限は `time`（`src/capabilities.rs` 447〜448 行の `Os.__sleep`・`Os.__clock`）。
- TZ01 のランタイムに、眠る G を起こすだけのタイマー（`tzr_timer { when, g }`、1 つのヒープ、1 本のスレッド）がある。止める（ヒープから外す）操作はない。
- `Async.sleep`・`Async.now`（仮想時刻）は Async の中だけ（TZ13 で消える）。

## 変えてはいけない意味

- `Time.monotonic_ns`・`unix_ns`・`sleep_ms` の型・結果・権限。`sleep_ms` は、tzroutine でない場所では今と同じく OS スレッドを止め、tzroutine の中では、ほかの tzroutine を止めずに同じ時間（以上）待つ。
- 時間の単位: 新しい API の長さはナノ秒の `i64`（Go の `time.Duration`）。時刻は `Time.monotonic_ns` と同じ時計のナノ秒。

## 仕様

### 1. API

`std/Time.tz` に足す:

| 名前 | 型 | 意味 | Go |
| --- | --- | --- | --- |
| `nanosecond`・`microsecond`・`millisecond`・`second`・`minute`・`hour` | `const … : i64` | 1・1,000・1,000,000・10⁹・6×10¹⁰・3.6×10¹² | `time.Millisecond` など |
| `sleep` | `own i64 -> IO<unit>` | 少なくとも `d` ナノ秒待つ。`d <= 0` は `Routine.yield` と同じ | `time.Sleep` |
| `after` | `own i64 -> IO<Channel<i64>>` | `d` 後に、そのときの単調時計のナノ秒を 1 つ受け取れる容量 1 のチャンネル | `time.After` |
| `tick` | `own i64 -> IO<Channel<i64>>` | `d` ごとに時刻を送るチャンネル（容量 1。受け手が遅れたら、その回は捨てる）。**返したチャンネルのハンドルがすべて drop されたら止まる**。`d <= 0` はトラップ（`assert`。Go の panic） | `time.Tick`（Go 1.23 以降の、参照がなくなれば回収される挙動） |
| `after_func` | `own i64 -> own Owned.Function<unit, unit> -> IO<Timer>` | `d` 後に、関数を新しい tzroutine で 1 回呼ぶ。返した `Timer` で止められる | `time.AfterFunc` |

新しいモジュール `Timer`（`std/Timer.tz`）と `Ticker`（`std/Ticker.tz`）:

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `Timer.create` | `own i64 -> IO<Timer>` | `d` 後に 1 回だけ時刻を送るタイマー（`time.NewTimer`） |
| `Timer.channel` | `ref Timer -> Channel<i64>` | タイマーのチャンネルのハンドル（`share`。`after_func` のタイマーでは、閉じたままの空のチャンネル） |
| `Timer.stop` | `ref Timer -> bool` | 止める。まだ送っていなければ真（`t.Stop()`）。止めたあとチャンネルに値は来ない（Go 1.23 以降と同じく、止める前に入っていた値も捨てる） |
| `Timer.reset` | `ref Timer -> i64 -> bool` | 今から `d` 後に送り直す。止めていたか送ったあとなら偽、まだ待っていたなら真（`t.Reset(d)`） |
| `Ticker.create` | `own i64 -> IO<Ticker>` | `d` ごとに送るティッカー。`d <= 0` はトラップ（`time.NewTicker`） |
| `Ticker.channel`・`Ticker.stop`・`Ticker.reset` | `ref Ticker -> Channel<i64>`・`ref Ticker -> unit`・`ref Ticker -> i64 -> unit` | 同上（`Reset` は周期を変えて今から数え直す） |

- `Timer`・`Ticker` は非 Copy の所有値（不透明な record `{ handle: i64 }`。構築・分解は `E1022`）。`Drop` で止めてランタイムの記録を解放する。`Send`・`Sync`。
- 引数の順序（D-59）: ハンドルが先頭（`MODULES` に `("Timer", Exempt(BUILTIN_HANDLE))`・`("Ticker", Exempt(BUILTIN_HANDLE))`）。`Time.after_func d f` は対象のない関数（`Time` の既存の分類のまま）。
- 権限: 新しい関数はすべて `time`（`after_func` の関数の中の権限は、その関数の本体の分）。
- `Time.after` は `select` の腕に直接書ける値ではない（`IO` を返す）。`let! deadline = Time.after d` で受けてから腕に書く（早見の例）。

### 2. 実行の意味

- 時計は `CLOCK_MONOTONIC`（Windows は `QueryPerformanceCounter`）。期限は「今 + d」（`INT64_MAX` で飽和）。早く起きることはない（期限を過ぎてから送る／起こす）。
- タイマーのチャンネルの値は、送った時点の単調時計のナノ秒（Go は送った時刻の `time.Time`）。
- `tick`・`Ticker`: 期限 `when += d`。送るときに受け手が受け取っていなければ（容量 1 が埋まっていれば）その回を捨てる（Go と同じ）。処理が遅れて期限を過ぎていたら、次の期限は「今 + d」。
- `after_func`: 期限で、`Routine.go f` と同じく新しい tzroutine を作る。tzroutine は、`after_func` を呼んだ tzroutine と同じスコープで数える（境界は、まだ発火していない `after_func` を **待たない**。下）。
- `Time.sleep_ms`（既存）: tzroutine の中なら `Time.sleep` と同じ待ち（スレッドを止めない）。それ以外では今の `nanosleep`。

### 3. 境界と解放

- 境界（`main`・`export`・`test`・`bench`。TZ02）を出るとき、その境界の中で作られ、まだ発火していないタイマーはすべて止められ、ランタイムの記録とチャンネルの分の参照が解放される（`after_func` の関数は呼ばれずに drop される）。境界はタイマーを待たない（Go の `main` が終わればタイマーは捨てられるのと同じ）。
- `Time.after`・`Time.tick` のチャンネル: ランタイムが 1 つ参照を持つ。`after` は送ったあと、`tick` は「参照がランタイムの分だけになった」ことを送る時点で見て、参照を離す。最後の参照が離れたらブロックを解放する（要素は `i64` なので drop は要らない）。
- 確保の追跡（`live`）: タイマーのチャンネルのブロックは Tsuzuri の allocator で確保する（`live` で数える）。ランタイムの記録（`tzr_timer`）は `malloc`（TZ01 の D9）。

## 設計

### ランタイム（`src/runtime/tzroutine.c` のタイマーの節を広げる）

```c
enum { TZR_TIMER_WAKE, TZR_TIMER_SEND, TZR_TIMER_TICK, TZR_TIMER_FUNC };
enum { TZR_TIMER_PENDING, TZR_TIMER_FIRED, TZR_TIMER_STOPPED };

typedef struct tzr_timer {
    int64_t when;          /* the deadline (monotonic ns) */
    int64_t period;        /* TICK only */
    int kind;
    int state;             /* under tzr_timers.lock */
    size_t index;          /* its position in the heap while PENDING (for removal) */
    tzr_g *g;              /* WAKE: the sleeper */
    tzr_chan *chan;        /* SEND, TICK: the channel (the timer holds one reference) */
    void (*run)(void *);   /* FUNC: @tz.tzroutine.owned */
    void (*drop)(void *);  /* FUNC: @tz.tzroutine.owned.drop (drops the function without calling it) */
    void *box;             /* FUNC: the Owned.Function */
    tzr_scope *scope;      /* the boundary that made it (section 3) */
    struct tzr_timer *scope_next; /* the boundary's list of its timers */
} tzr_timer;
```

| 関数（`TZR_API`） | 意味 |
| --- | --- |
| `void *tsuzuri_tz_timer_channel(int64_t ns, int64_t period, int32_t tick)` | 容量 1・要素 8 バイトのチャンネルを `tsuzuri_alloc(128 + 8)` で作り、参照 2（呼んだ側とタイマー）、タイマーを `SEND`（`tick == 0`）か `TICK` で登録して、チャンネルを返す（`Time.after`・`tick`）。`Timer.create`・`Ticker.create` は記録も返す下の関数を使う |
| `void *tsuzuri_tz_timer_create(int64_t ns, int64_t period, int32_t tick)` | 同じチャンネルを作り、記録（`tzr_timer *`）を返す（`Timer`・`Ticker` のハンドル。記録が `chan` を持つ）。`Timer.channel` は `tsuzuri_tz_timer_chan(timer)` で `share` したチャンネル |
| `void *tsuzuri_tz_timer_func(int64_t ns, void (*run)(void *), void (*drop)(void *), void *box)` | `FUNC` の記録を返す（`after_func`） |
| `int32_t tsuzuri_tz_timer_stop(void *timer)` | `PENDING` ならヒープから外して `STOPPED`、1 を返す（`FUNC` は `drop(box)`）。`SEND` は、チャンネルに残っている値を 1 つ捨てる（`tzr_chan` の `count` を 0 に。要素は `i64` なので drop は要らない）。そうでなければ 0 |
| `int32_t tsuzuri_tz_timer_reset(void *timer, int64_t ns)` | 戻り値は `stop` と同じ規則で求め、そのあと期限を「今 + ns」（`TICK` は周期も `ns`）にして `PENDING` で入れ直す |
| `void tsuzuri_tz_timer_free(void *timer)` | `stop` してから、記録が持つチャンネルの参照を離し（最後なら `tsuzuri_free`）、記録を `free`（`Timer`・`Ticker` の `Drop`） |
| `int32_t tsuzuri_tz_sleep(int64_t ns)` | 既存（`WAKE`）。`index` と `state` を使う形に直す |

- ヒープの入れ替えでは `index` も入れ替える（`tzr_heap_up`・`tzr_heap_down` で `timers.heap[i]->index = i`）。外すのは `heap[index]` を末尾と入れ替えて上下に直す。
- タイマーのスレッドの発火（`tzr_timers.lock` の下でヒープから外し、`state = FIRED`）: `WAKE` → `tzr_ready(g)`。`SEND` → `tsuzuri_tz_chan_send(chan, &now, 0)` の結果にかかわらず、タイマーの参照を離す（`release` が最後なら `tsuzuri_free(chan)`）。`TICK` → 参照数が 1（タイマーだけ）なら止めて離す。そうでなければ `send(…, 0)`（満杯なら捨てる）、`when = max(when + period, now + 1)`、`PENDING` で入れ直す。`FUNC` → `tsuzuri_tz_go(run, box)`（ロックを外してから。`go` は `tzr_sched.lock` を取る。R3 では `timers.lock` を持ったままでもよいが、`go` はスタックの確保もするので外す）。
- チャンネルへの送信と `ready` は `timers.lock` を持ったまま呼んでよい（R3）。ただし `FUNC` の `go` と `tsuzuri_free` は外してから（確保の関数がロックを取る可能性を避ける）。
- `ntimers` は `PENDING` の数（`WAKE` 以外も含む）。発火・停止で減らす（TZ01 の判定の条件は変えない。`after` のタイマーが待つ者を起こしうる間は、デッドロックではない）。
- 境界との対応: `tsuzuri_tz_scope_enter` が作るスコープに `timers` の一覧の頭を足し、タイマーの生成は「生成した G の生成スコープ」の一覧へつなぐ（`WAKE` はつながない）。`tsuzuri_tz_boundary_leave`（TZ02）は、`scope_leave` の後で、一覧の `PENDING` をすべて `stop` し、`SEND`・`TICK` の参照を離し、記録を解放する（`Timer`・`Ticker` のハンドルはこの時点で drop 済み: 境界の関数が戻った後なので）。
- `Time.sleep_ms` のフック（`src/runtime/os.c`）: `tsuzuri_os_sleep` の先頭で、弱い定義のフック変数 `int32_t (*volatile tsuzuri_os_sleep_hook)(int64_t ns)` が非 0 なら呼び、1 が返ったら（tzroutine の中で待った）0 を返す。`tzroutine.c` は `tzr_init` でフックを `tzr_sleep_if_routine`（現在の G が tzroutine なら `tsuzuri_tz_sleep` して 1、擬似 G なら 0）にする。フック変数は両方の C で `__attribute__((weak, visibility("hidden")))` の同じ定義（`= 0`）にする（片方だけ連結されてもよい）。`os.c` の変更は 1 か所で、tzroutine を使わないプログラムの IR は変わらない（C のランタイムの文字列だけが変わる）。

### コンパイラ

| ファイル | 変更 |
| --- | --- |
| `std/Time.tz` | 定数 6 つ（`const`）、`sleep`・`after`・`tick`・`after_func`（それぞれ私的な組み込み `Time.__sleep_ns`・`Time.__after`・`Time.__tick`・`Time.__after_func` を `IO { work: … }` で包む。既存の `reading`・`pause` と同じ書き方）。`///` と G34 の例 |
| `std/Timer.tz`・`std/Ticker.tz`（新規） | `record Timer { handle: i64 }` と `instance Drop<Timer>`（`Timer.__free`）。`create`・`channel`・`stop`・`reset`（私的な組み込みを呼ぶ）。`Ticker` も同じ |
| `src/check.rs` | 組み込み `TimeSleepNs`・`TimeAfter`・`TimeTick`・`TimeAfterFunc`・`TimerCreate`・`TimerChannel`・`TimerStop`・`TimerReset`・`TimerFree`（`Ticker` は同じ組み込みを `tick = 1` で使うか、別に足す。std の側で呼び分けられるなら同じでよい）。`__` の名前は std 専用（`E1022`）。`TimeAfterFunc` は `Builtin::spawns()` が真 |
| 生成 | 各組み込みは C の関数を 1 回呼ぶ。チャンネルを返すものは `{ i64 ptrtoint }` の `Channel` の record（`mark_live`）。`after_func` は TZ02 の `@tz.tzroutine.owned` と、drop だけをする `@tz.tzroutine.owned.drop`（`@tz.closure.drop` と `@tz.free`）の 2 つの補助関数を `intrinsics` へ |
| `src/stdlib.rs` | `SOURCES`・`OPT_IN`（`Timer`・`Ticker`。`Time` は既存）、`RESERVED_MODULES` に 2 つ |
| `src/capabilities.rs` | 新しい組み込みを `time` に（`Os.__sleep` と同じ所）。`tests/fixtures/capabilities/std-table.txt` に新しい公開関数の行（`Time.sleep: time` など） |
| `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs` | `Os.__sleep` と同じ分類（`io`）、生成するものは `alloc`、`after_func` は `task` も |

## 前提とする他チケットのインターフェース

- TZ01 の `tzr_timers`・`tsuzuri_tz_sleep`・`tsuzuri_tz_chan_*`。TZ02 の `@tz.tzroutine.owned`・`Builtin::spawns`・`tsuzuri_tz_boundary_leave`。TZ03 の `Channel` の record の形。

## 他チケットへ提供するインターフェース

- `tsuzuri_tz_timer_*`（TZ07 の `Context.with_timeout` が `FUNC` か専用の種類で期限を作る）。
- タイマーの一覧のスコープへのつなぎ（TZ07 の `Context` の期限も同じ規則で解放する）。
- TZ08: タイマーのスレッドを netpoller の待ち（`kevent`／`epoll_wait` の時間切れ）にまとめるときの入口は `tzr_timerproc` の 1 か所。

## 実装手順

1. **変更**: ブランチ `feat/tz05-timers`、基準、着手条件の `grep`。
2. **変更**: ランタイム（設計の C）と `tests/tzroutine_runtime.c` のケース: `timer_channel`（`after` の値が 1 回届き、期限より早くない）、`timer_stop`（止めたタイマーは送らない。`stop` の戻り値）、`timer_reset`、`ticker`（5 回受けて止める。受けないと捨てられる）、`ticker_drop`（参照を離すと止まり、ブロックが解放される。`tsuzuri_alloc` をテスト用に数える差し替えで確かめる）、`after_func`（呼ばれる・止めれば呼ばれずに drop される）、`boundary_timers`（境界で止められる）。**確認**: `TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`。
3. **変更**: `os.c` のフック。**確認**: `node tests/os.mjs target/debug/tsuzuri`（既存）が通る。C のテスト `sleep_ms_hook`（tzroutine の中の `tsuzuri_os_sleep(20)` が 2,000 個同時に 20 ms 前後で終わる。スレッドを止めていれば数秒かかる）。
4. **変更**: std と組み込みと生成。**確認**: `tests/timers.rs`（型、`E1022`、権限の表、`IO` であること、`Timer` の関数値への捕捉は `E1005`）。
5. **変更**: E2E（`tests/fixtures/timers/Main.tz`）。**確認**: `node tests/tzroutine.mjs target/debug/tsuzuri`。
6. **変更**: TZ04 が done なら `select` を使う E2E（早見の例、`ticker` と `quit` の `select`）。
7. **変更**: 文書と規則 ID。T2 と CI（`ci:full`）。

## テスト計画

| E2E | 内容 | 期待 |
| --- | --- | --- |
| `sleep_order` | 3 つの tzroutine が 30・10・20 ms 眠ってから同じチャンネルへ自分の番号を送る | 受け取る順が `2, 3, 1`（10 ms ずつの差は、混んだ CI でも順が入れ替わらない大きさ。それでも入れ替わるなら 50 ms ずつにする。速度の閾値ではなく順序の検査） |
| `sleep_does_not_block_others` | `TSUZURI_MAXPROCS=1` で、1 つの tzroutine が `Time.sleep (200 * Time.millisecond)` の間に、ほかの tzroutine が 1,000 回の往復を終える | 往復が先に終わる（受け取る順で確かめる） |
| `sleep_ms_in_routine` | 同じ検査を `Time.sleep_ms 200` で | 同じ |
| `after_is_a_timeout` | 早見の例 | `timeout` |
| `after_value` | `after` の値が `monotonic_ns` の前後の間 | 1 |
| `timer_stop_reset` | `Timer.stop` の真偽、`reset` 後に 1 回届く | 期待の符号化 |
| `ticker_five` | `Ticker` を 5 回受けて `stop`、そのあと 50 ms 待っても来ない（`try_recv` が `Empty`） | 5 |
| `tick_dropped` | `Time.tick` のチャンネルを drop したあと、境界で `live == 0` | `live == 0` |
| `after_func_runs` | `after_func` の関数が別の tzroutine で 1 回呼ばれる（チャンネルで知らせる） | 1 |
| `after_func_stopped` | 止めた `after_func` の関数の捕捉が drop される（`Drop` で `Atomic` を数える） | 1 |
| `pending_at_exit` | `main` が 1 時間後のタイマーを作って終わる | 待たずに終わる、`live == 0` |

- 時間に依存する検査は、順序と「早くない」だけを見る（「何 ms 以内」の上限を入れない。AGENTS.md の速度の閾値の禁止）。

## ドキュメント

- `built-in-types-and-modules/time.md`: 定数・`sleep`・`after`・`tick`・`after_func` の節、Go との対応表、`sleep_ms` の tzroutine の中の挙動。
- 新規 `built-in-types-and-modules/timer.md`・`ticker.md`（`check-cli-docs` の `stdPage`）。
- `async-tasks-and-lazy/select.md`（TZ04）にタイムアウトの例、`tzroutine.md` に周期の処理の例。
- `docs/language.md`（Time の節）、`docs/architecture.md`（TZ01 の節のタイマー）。
- `spec/rules/TSK.md`: `TSK-023`（`Time.sleep` in a tzroutine MUST NOT stop other tzroutines.）、`TSK-024`（A stopped `Timer` MUST NOT send.）、`TSK-025`（A boundary MUST NOT wait for pending timers, and MUST release them.）。

## 受け入れ条件

- [ ] 仕様 1〜3 のとおりに動く。`live == 0`。
- [ ] `Time.sleep_ms` の既存の検査（`tests/os.mjs`）と IR が変わらない。
- [ ] ランタイムの C のテストが TSan・ASan で通る。
- [ ] 文書・規則 ID・権限の表を更新した。

## 落とし穴

- **タイマーの記録を G のスタックに置かない**（`sleep` 以外）: `Timer` のハンドルは G より長く生きうる。`malloc` する。`sleep` の記録だけは、眠る G のスタックに置ける（発火したら外れる）。
- **`stop` と発火の競合**: どちらも `tzr_timers.lock` の下で `state` を見る。発火の処理は、ロックの下で `FIRED` にしてから送る。
- **`TICK` の参照の数え方**: ランタイムの分の 1 と比べる。`Ticker` のハンドル（記録）は、チャンネルの参照を別に持たない（記録が持つ 1 つだけ）。`Ticker.channel` で利用者が取ったハンドルが参照を増やす。`Ticker` は `stop` されるか drop されるまで止まらない（`Time.tick` だけが「参照がなくなれば止まる」）。
- **`tsuzuri_free` は確保した allocator で**: `--allocator counting`・`small` でも同じ関数を呼ぶ（S1）。
- **境界の解放と `Timer` のハンドル**: 境界を出る前に `Timer` は drop されている（関数が戻った）。`tsuzuri_tz_timer_free` と境界の解放の両方が同じ記録を解放しないよう、`Drop` で一覧から外す。

## 対象外

- 壁時計の時刻の型（Go の `time.Time`・`Format`・時差）。時刻はナノ秒の `i64`。
- 単調時計でない時計でのタイマー、タイマーの分解能の保証。
- P ごとのタイマーのヒープ（TZ01 の D7。計測で必要と分かったら別チケット）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 長さはナノ秒の `i64` と `Time.millisecond` などの定数 | Go の `time.Duration` と同じ使い方（`50 * Time.millisecond`） | 既定案 |
| D2 | 新しい関数は `IO` を返す | 時計は OS の効果で、`Time` の既存の関数と権限（`time`）の扱いをそろえる | 既定案 |
| D3 | `Time.tick` はチャンネルの参照がランタイムの分だけになったら止まる | GC なしで Go 1.23 の「参照されなくなれば回収」と同じ結果にする | 既定案 |
| D4 | 境界はタイマーを待たず、止めて解放する | Go の `main` の終了と同じ。止まらない周期で境界が終わらなくならない | 既定案 |
| D5 | 止めたタイマーのチャンネルに残った値は捨てる | Go 1.23 以降と同じ（古い値を受け取る誤りを防ぐ） | 既定案 |
| D6 | `Time.sleep_ms` は tzroutine の中では待つ G だけを止める（フック） | 既存の API を使う tzroutine が P を占めない | 既定案 |
| D7 | `Ticker.create` の周期 0 以下はトラップ | Go の panic と同じ | 既定案 |
