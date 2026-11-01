# TZ07: `Context`（取り消し・期限）と `ErrGroup`

| 項目 | 内容 |
| --- | --- |
| ID | TZ07 |
| 優先度 | P1 |
| 規模 | M。Phase 1（`Context`）M、Phase 2（`ErrGroup`）S、Phase 3（`Context.after_func`）S |
| 依存 | TZ04, TZ05, TZ06 |
| 後続 | TZ08（Net の待ちを `Context` で止める）、TZ12（`Task.parallel_results` の移行先） |
| 状態 | todo |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61）。決定事項 D1〜D6 は既定案 |
| 手本にする既存実装 | TZ06 の `tsuzuri_tz_handle_*`（参照数のハンドル）と `WaitGroup`、TZ05 の `tzr_timer`（期限）とスコープへのつなぎ、TZ03 のランタイムが作るチャンネル（`Time.after` と同じく `tsuzuri_alloc` で確保し、要素の drop が要らない型）、`std/Time.tz` の `IO` の包み方 |
| 主な影響ファイル | `std/Context.tz`・`std/ErrGroup.tz`（新規）、`src/check.rs`・`src/llvm_sync.rs`・`src/stdlib.rs`・`src/capabilities.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/runtime/tzroutine.c`・`tests/tzroutine_runtime.c`・`tests/context.rs`（新規）・`tests/fixtures/context/Main.tz`（新規）・`tests/tzroutine.mjs`・`tests/std_argument_order.rs`・`tests/std_examples.rs`・`tests/fixtures/capabilities/std-table.txt`・文書 |

## 実装者向け早見

- 範囲: Go の `context`（`Background`・`WithCancel`・`WithTimeout`・`WithDeadline`・`Done`・`Err`・`Deadline`・`AfterFunc`）と `golang.org/x/sync/errgroup`（`Group`・`WithContext`・`Go`・`TryGo`・`SetLimit`・`Wait`）。終わらない tzroutine を止める標準の手段（D-61 の決定 1 で境界が待つため、止め方が要る）。
- 最初に読む: ① 仕様 1・2 ② TZ05 の設計（タイマーの記録とスコープ）③ TZ06 の `tsuzuri_tz_handle_*` と `WaitGroup` ④ TZ04 の `select`。
- 最初に書くテスト: `tests/tzroutine_runtime.c` の `context_tree`（C だけで、親の取り消しが孫まで届き、`done` のチャンネルが閉じる）→ E2E の `stop_a_worker`。
- 触ってはいけない: TZ03〜TZ06 の API の意味。
- 完了の見分け方: 次が `stopped after 3` を出して終わる（境界が待ち続けない）。

```tsuzuri
let root = Context.background ()
let ctx = Context.with_cancel root
let ticks: Channel<i64> = Channel.make 0
let worker_ctx = Context.share ctx
let out = Channel.share ticks
tzroutine {
    let done = Context.done worker_ctx
    let mut n = 0i64
    let mut running = true
    while running do
        select
        | recv _ from done -> running = false
        | send n to out -> n = n + 1
}
let mut seen = 0i64
for _ in 1 ..= 3 do
    match Channel.recv ticks with
    | Maybe.Some _ -> seen = seen + 1
    | Maybe.None -> ()
Context.cancel ctx
$"stopped after {seen}"
```

## 目的

境界は全 tzroutine の終了を待つ（D-61 の決定 1）。止まらない処理（サーバーのループ、周期の処理、遅い呼び出し）を外から止める手段として、Go の `context` を入れる。複数の tzroutine のうち 1 つが失敗したら残りを止めて最初の誤りを返す処理（Go の `errgroup`）は、旧 `Task.parallel_results` の移行先にもなる（TZ12）。

## 着手条件と停止条件

### 着手条件

- TZ04・TZ05・TZ06 が done。

### 停止条件

- **S1** 取り消しの伝播で、ロックの順序（R3）を守れない（親と子のロックを逆の順に取る経路がある）。設計の「全体で 1 つのロック」を変えたくなったら止める。
- **S2** `live != 0`（期限のタイマー・`done` のチャンネル・`ErrGroup` の誤りの値）。2 回直して直らなければ止める。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認）

- 取り消しの仕組みは `Task.parallel_results` の「未開始のタスクを始めない」（B06）と、`Async.all_results` の「中断した子を捨てる」（B08）だけ。外から実行中の計算を止める手段はない。
- TZ05 のタイマーは、期限で G を起こす・チャンネルへ送る・関数を tzroutine で呼ぶの 3 種類と、境界での停止を持つ。

## 変えてはいけない意味

- 取り消しは協調的: 取り消された `Context` を見ている tzroutine が自分で止まる。ランタイムは tzroutine を強制的に止めない（D1）。
- 一度取り消された `Context` は取り消しのまま（`err` は最初の理由のまま変わらない）。

## Phase 分割

| Phase | 内容 |
| --- | --- |
| 1 | `Context` の作成・取り消し・期限・`done`・`err`・`deadline` |
| 2 | `ErrGroup`（std の Tsuzuri） |
| 3 | `Context.after_func` |

## 仕様

### Phase 1: `Context`（`std/Context.tz`）

```tsuzuri
record Context { handle: i64 }
instance Drop<Context> { fn drop context = Context.__release context }
/// Why a context ended.
union Error = Canceled | DeadlineExceeded
```

| 関数 | 型 | 意味 | Go |
| --- | --- | --- | --- |
| `background` | `unit -> Context` | 取り消されない根 | `context.Background()` |
| `with_cancel` | `ref Context -> Context` | 子。親が取り消されるか、自分が `cancel` されると取り消される | `WithCancel` |
| `with_timeout` | `ref Context -> own i64 -> IO<Context>` | 今から `d` ナノ秒の期限を持つ子（親の期限が早ければ親の期限） | `WithTimeout` |
| `with_deadline` | `ref Context -> own i64 -> IO<Context>` | 単調時計のナノ秒 `t` の期限を持つ子 | `WithDeadline` |
| `cancel` | `ref Context -> unit` | 自分と子孫を `Canceled` で取り消す（すでに取り消されていれば何もしない） | `cancel()` |
| `done` | `ref Context -> Channel<unit>` | 取り消されると閉じるチャンネル（ハンドルを 1 つ増やす）。`background` のは閉じない | `ctx.Done()` |
| `err` | `ref Context -> Maybe<Error>` | 取り消されていれば理由 | `ctx.Err()` |
| `is_done` | `ref Context -> bool` | `err` が `Some` か | — |
| `deadline` | `ref Context -> Maybe<i64>` | 期限（単調時計のナノ秒） | `ctx.Deadline()` |
| `share` | `ref Context -> Context` | 同じ `Context` のハンドル（tzroutine へ移すため） | — |

- 取り消すのは `cancel` を呼べる誰でも（Go は `cancel` 関数を別に返すが、Tsuzuri はハンドルが取り消しもできる。D2）。取り消しの権利を渡したくない場合は、`with_cancel` で子を作って渡す（子を取り消しても親は取り消されない）。
- 子の `done` は、親が取り消されたときにも閉じる（理由は親の理由。期限なら `DeadlineExceeded`）。
- 期限は TZ05 のタイマー（`CTX` の種類）で、期限で `DeadlineExceeded` の取り消しをする。`cancel` された子は期限のタイマーを止める。
- 境界（TZ02）を出るとき、まだ発火していない期限のタイマーは TZ05 の規則で止められる。
- `Context` は不透明なハンドル（`Send`・`Sync`、非 Copy、`Capture` でない）。資源のハンドルなので先頭（D-59）。
- 権限: `with_timeout`・`with_deadline` は `time`。ほかは権限なし。

### Phase 2: `ErrGroup<'e>`（`std/ErrGroup.tz`。Tsuzuri で書く）

| 関数 | 型 | 意味 | Go |
| --- | --- | --- | --- |
| `create` | `Send<'e> => unit -> ErrGroup<'e>` | — | `var g errgroup.Group` |
| `with_context` | `Send<'e> => ref Context -> ErrGroup<'e>` | 最初の誤りで取り消される子の `Context` を持つ | `errgroup.WithContext(ctx)` |
| `context` | `ref ErrGroup<'e> -> Context` | その `Context` のハンドル（`create` で作ったものは `background` の子） | 戻り値の `ctx` |
| `go` | `ref ErrGroup<'e> -> own Owned.Function<unit, Result<unit, 'e>> -> unit` | 関数を新しい tzroutine で呼ぶ。上限があれば空くまで待つ | `g.Go(f)` |
| `try_go` | `ref ErrGroup<'e> -> own Owned.Function<unit, Result<unit, 'e>> -> Result<unit, Owned.Function<unit, Result<unit, 'e>>>` | 上限で空きがなければ関数を返す | `g.TryGo(f)` |
| `set_limit` | `ref ErrGroup<'e> -> i64 -> unit` | 同時に走る数の上限（負は上限なし。走っている間に変えるとトラップ） | `g.SetLimit(n)` |
| `wait` | `ref ErrGroup<'e> -> Result<unit, 'e>` | 全員の終わりを待ち、最初に起きた `Error`（完了の時刻の順。2 つ目以降は drop）か `Ok` | `g.Wait()` |
| `share` | `ref ErrGroup<'e> -> ErrGroup<'e>` | 同じグループのハンドル | — |

- 中身は `record ErrGroup<'e> { group: WaitGroup, first: Arc<Mutex<Maybe<'e>>>, limit: Arc<Mutex<i64>>, slots: Semaphore, context: Context }`（std の外から見えない。`E1022`）。`go` は `WaitGroup.add`・`Semaphore.acquire`（上限があるとき）・各ハンドルの `share` をしてから `tzroutine { … }` で関数を `Owned.call` し、`Error e` なら `first` が空のときだけ入れて `Context.cancel` する。最後に `Semaphore.release`・`WaitGroup.done`。
- 「最初の誤り」は完了の時刻の順（Go と同じ）。旧 `Task.parallel_results` の「入力の添字が最小の誤り」とは違う（TZ12 の移行の文書に書く）。

### Phase 3: `Context.after_func`

- `after_func :: ref Context -> own Owned.Function<unit, unit> -> Context.AfterFunc`。取り消されたら関数を新しい tzroutine で 1 回呼ぶ（すでに取り消されていれば、すぐ）。`Context.stop_after :: ref Context.AfterFunc -> bool`（まだ呼んでいなければ止めて真）。`AfterFunc` は不透明なハンドル（drop で止める）。Go の `context.AfterFunc`。

## 設計

### ランタイム（`src/runtime/tzroutine.c` に `context` の節）

```c
typedef struct tzr_ctx {
    _Atomic uint64_t refs;
    struct tzr_ctx *parent;      /* holds a reference to the parent */
    struct tzr_ctx *children;    /* the live children (no reference), linked through `sibling` */
    struct tzr_ctx *sibling, *prev_sibling;
    tzr_chan *done;              /* capacity 0, element size 0; the context holds one reference */
    int err;                     /* 0 none, 1 canceled, 2 deadline exceeded */
    int64_t deadline;            /* INT64_MAX: none */
    tzr_timer *timer;            /* the deadline timer, if any */
    struct tzr_after *afters;    /* Phase 3 */
} tzr_ctx;
static tzr_lock tzr_ctx_lock;    /* one lock for every context tree (S1: no lock order to get wrong) */
```

| 関数（`TZR_API`） | 中身 |
| --- | --- |
| `void *tsuzuri_tz_ctx_background(void)` | 親なし・期限なし・`done` は閉じないチャンネル（`tsuzuri_alloc(128)`、`tsuzuri_tz_chan_init(c, 0, 0)`） |
| `void *tsuzuri_tz_ctx_child(void *parent, int64_t deadline)` | `deadline` は `INT64_MAX` で期限なし。親の期限より遅ければ親の期限。ロックの下で親の子の一覧へつなぐ。親がもう取り消されていれば、同じ理由で取り消した子を作る。期限があれば TZ05 のタイマー（種類 `CTX`）を作る（期限が過去ならすぐ発火） |
| `void tsuzuri_tz_ctx_cancel(void *ctx, int32_t why)` | `tzr_ctx_lock` の下で、`err == 0` の自分と子孫を幅優先（子の一覧をたどる明示のスタック。再帰しない）に `err = why`、`done` を `tsuzuri_tz_chan_close`（チャンネルのロックを `tzr_ctx_lock` の内側で取ってよい。R3）、期限のタイマーを持つものを手元の一覧に集める。**ロックを外してから**、集めたタイマーを `tsuzuri_tz_timer_stop` し、止められたものはタイマーの分の参照を `ctx_release` で離す（`tzr_ctx_lock` と `tzr_timers.lock` を入れ子にしない。D3） |
| `int32_t tsuzuri_tz_ctx_err(void *)`・`int64_t tsuzuri_tz_ctx_deadline(void *)`・`void *tsuzuri_tz_ctx_done(void *)` | 読むだけ（`err` はロックの下で）。`done` はチャンネルの `share` |
| `void tsuzuri_tz_ctx_share(void *)`・`void tsuzuri_tz_ctx_release(void *)` | 参照数。期限のタイマーは、まだ発火していない間 1 つ参照を持つ（`timer_ref = 1`）。利用者の参照が 0 になったら（残りがタイマーの分だけになったら）、タイマーを `stop` し、止められたらタイマーの分を離す。最後の参照が離れたら、`tzr_ctx_lock` の下で親の子の一覧から外し、`done` の参照を離し（最後なら `tsuzuri_free`）、ロックを外してから親の参照を離し（これも最後なら同じ手順を親へ。再帰せずループで）、自分を `tsuzuri_free` |

- 記録は `tsuzuri_alloc` で確保する（`live` で数える。`Context` は利用者の値なので D9 の例外にしない）。
- TZ05 のタイマーに種類 `TZR_TIMER_CTX` を足す。タイマーのスレッドは `tzr_timers.lock` の下でヒープから外して `FIRED` にし、**ロックを外してから** `tsuzuri_tz_ctx_cancel(ctx, 2)` と、タイマーの分の `ctx_release` を呼ぶ（タイマーが参照を持つので、その間 `Context` は解放されない）。

### コンパイラ

- `std/Context.tz`: record・`Drop`・`union Error`・公開関数（私的な組み込み `Context.__background` などを包む。`with_timeout` は `Time.monotonic_ns` 相当の時計を組み込みの側で読む）。
- 組み込み（`src/check.rs`）: `ContextBackground`・`ContextChild`・`ContextCancel`・`ContextErr`・`ContextDeadline`・`ContextDone`・`ContextShare`・`ContextRelease`（`__` の名前は std 専用）。生成は C の 1 回の呼び出しと、`err` の整数から `Maybe<Error>` への変換。
- `std/ErrGroup.tz`（Phase 2）: 仕様のとおり Tsuzuri で書く。`go` の `tzroutine { }` が std の中で使えること（`Builtin::spawns` の歩きが std の関数も見ること）を確かめる。
- `src/stdlib.rs`: `SOURCES`・`OPT_IN`（`Context` は `uses: &["Channel"]`、`ErrGroup` は `uses: &["WaitGroup", "Mutex", "Semaphore", "Context"]`）・`RESERVED_MODULES`。
- `tests/std_argument_order.rs`: `("Context", Exempt(BUILTIN_HANDLE))`・`("ErrGroup", Exempt(BUILTIN_HANDLE))`。

## 前提とする他チケットのインターフェース

- TZ03 の `tsuzuri_tz_chan_*`、TZ04 の `select`、TZ05 の `tzr_timer` とスコープの一覧、TZ06 の `WaitGroup`・`Mutex`・`Semaphore`・`tsuzuri_tz_handle_*`。

## 他チケットへ提供するインターフェース

- `Context`（TZ08 の Net の待ちを `Context` で止める `Net.*_with` を足すときの型）。
- `ErrGroup`（TZ12 の `Task.parallel_results` の移行先）。

## 実装手順

1. **変更**: ブランチ `feat/tz07-context-errgroup`、基準。
2. **変更**: ランタイムと C のテスト（`context_tree`: 3 段の木で親を取り消すと全員の `done` が閉じ、`err` が 1。`context_deadline`: 20 ms の期限で `err` が 2、期限より早くない。`context_child_of_canceled`: 取り消された親の子はすぐ取り消し。`context_release`: 子より先に親のハンドルを離しても、子が親の参照を持つので壊れない。`context_deadline_race`（落とし穴）。全部 `tsuzuri_alloc` の数えで解放を確かめる）。**確認**: `TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`。
3. **変更**: `std/Context.tz`・組み込み・生成。**確認**: `tests/context.rs`（型・`E1022`・`E1005`・権限）。
4. **変更**: E2E（下の表）。**確認**: `node tests/tzroutine.mjs target/debug/tsuzuri`。
5. **変更**: Phase 2 の `std/ErrGroup.tz`。**確認**: E2E の `errgroup_*`。
6. **変更**: 文書と規則 ID、T2、CI（`ci:full`）。

## テスト計画

| E2E | 内容 | 期待 |
| --- | --- | --- |
| `stop_a_worker` | 早見の例 | `stopped after 3`、境界が待ち続けない |
| `cancel_tree` | 親を取り消すと 10 個の孫の tzroutine が全員止まる | 10 |
| `timeout` | `with_timeout` 30 ms の `done` を `select` で待つ | `DeadlineExceeded` |
| `parent_deadline_wins` | 親の期限 20 ms、子の期限 1 時間 | 子も 20 ms 前後で `DeadlineExceeded`（「早くない」だけを見る） |
| `cancel_idempotent` | `cancel` を 2 回、`err` は最初の理由 | `Canceled` |
| `errgroup_ok` | 100 個の `go` が全部 `Ok` | `Ok ()` |
| `errgroup_first_error` | 3 番目が `Error 3`、ほかは `Context.done` を待つ | `Error 3`、ほかは取り消しで止まる |
| `errgroup_limit` | `set_limit 2` で 20 個、同時の数の最大を記録 | 最大 ≤ 2 |
| `errgroup_try_go` | 上限 1 で走っている間の `try_go` は関数を返す | `Error f`（返った関数を呼べる） |
| `release_order` | ハンドルをいろいろな順で drop | `live == 0` |

## ドキュメント

- 新規 `built-in-types-and-modules/context.md`・`errgroup.md`。
- `async-tasks-and-lazy/tzroutine.md`: 「終わらない tzroutine を止める」節（TZ02 が書いた注意を、`Context` の例に置き換える）。`select.md` に `done` の腕の例。
- `docs/language.md`（Context・ErrGroup）、`index.md`。
- `spec/rules/TSK.md`: `TSK-029`（Canceling a `Context` MUST close the `done` channel of it and of every descendant.）、`TSK-030`（`ErrGroup.wait` MUST return only after every function given to `go` has returned.）。

## 受け入れ条件

- [ ] 各 Phase の仕様のとおりに動き、`live == 0`。
- [ ] C のテストが TSan・ASan で通る。
- [ ] 文書・規則 ID・権限の表を更新した。

## 落とし穴

- **取り消しは再帰しない**: 深い `Context` の木（`with_cancel` を 10 万段）でもスタックを使わない（明示のスタックか、子の一覧のたどり）。テストに 10,000 段を入れる。
- **`done` の `share` は毎回参照を増やす**: ループの中で `Context.done ctx` を呼ぶと、ハンドルが増えて drop で減る（漏れはしないが費用がある）。文書に「ループの前に 1 回取る」と書く（早見の例のとおり）。
- **期限のタイマーと解放の競合**: `tzr_ctx_lock` と `tzr_timers.lock` は決して入れ子にしない（取り消しが子孫のタイマーを止めるので、入れ子にすると同じロックを取り直すか、順序が逆になる）。解放の後に触らないことは、発火していない期限のタイマーが `Context` の参照を 1 つ持つことで保つ（設計の表）。C のテスト `context_deadline_race`（期限 1 ms の子を 10,000 個作ってすぐ離す）を TSan で回す。

## 対象外

- `context.WithValue`（D-61: 型のない値の袋。値は引数で渡す）、`WithoutCancel`、`WithCancelCause`・`Cause`（理由は `Error` の 2 種類だけ。独自の理由は `ErrGroup` の `'e` で返す）。
- 強制的な停止（D1）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 取り消しは協調的 | 巻き戻しも強制終了もない（D-10）。所有値の drop を必ず走らせる | 既定案 |
| D2 | ハンドルが取り消しもできる（`cancel` を別の値にしない） | 所有権のある値を 2 つに分けると、片方の drop の扱いが増える。渡したくなければ子を作る | 既定案 |
| D3 | 全体で 1 つのロック（`tzr_ctx_lock`）。タイマーのロックとは入れ子にしない | 木の上り下りのロックの順序の誤りを構造的になくす。`Context` の操作は頻繁でない | 既定案 |
| D4 | `ErrGroup` は std の Tsuzuri で書く | TZ06 までの部品で書け、ランタイムを増やさない | 既定案 |
| D5 | `ErrGroup.wait` は完了の時刻で最初の誤り | Go と同じ | 既定案 |
| D6 | `with_timeout`・`with_deadline` は `IO` | 時計を読む OS の効果（TZ05 の D2 と同じ） | 既定案 |
