# TZ03: 旧 `Channel` を `DataChannel` に改名し、Go と同じ双方向の `Channel<T>` を入れる（`for x in ch`・既定の WASM の 1 本の実行を含む）

| 項目 | 内容 |
| --- | --- |
| ID | TZ03 |
| 優先度 | P0 |
| 規模 | L。Phase 1（旧 `Channel` の `DataChannel` への改名、新しい `Channel<T>` の全操作、`for x in ch`、改名の `E1040`、既定の WASM の 1 本の実行）L、Phase 2（向きのある型 `Channel.SendOnly`・`Channel.ReceiveOnly`。人間が求めたときだけ）M |
| 依存 | TZ02 |
| 後続 | TZ04（`select` の腕）、TZ05（`Time.after` などがチャンネルを返す）、TZ06（既定の WASM の同期の型が同じ 1 本の実行の組み方を使う）、TZ07（`Context.done`）、TZ09、TZ12（`DataChannel` の待ちを tzroutine のランタイムへ移す） |
| 状態 | done（2026-11-02。Phase 1・2） |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61 の決定 3・5・6: Go と同じ双方向型を `Channel` の名前で入れ、旧 `Channel` は `DataChannel` に改名して残す）。決定事項 D1〜D10 は既定案 |
| 手本にする既存実装 | 旧 `Channel` の全体（改名する対象であり、新しい `Channel` の書き方の手本でもある）: `std/Channel.tz`（不透明な record と `Drop`）、`src/check.rs` の `Builtin::Channel*`（1634〜1648 行・1971〜1976 行・2211〜2216 行・3321〜3326 行・3456〜3570 行の `sync_scheme`）、`src/llvm_sync.rs` の `channel_builtin`（277 行）・`channel_bounded`（378 行）・`channel_close`（436 行）、`src/ownership.rs` の要素の借用の検査（2731〜2747 行の `ChannelItemHoldsBorrow`、2936〜2945 行の `MustApplyDirectly`）、`src/stdlib.rs` の `OptIn { module: "Channel", … }`。`for` の脱糖: `src/control.rs` の `sequence_loop`。WASM の C の組み立て: `src/driver.rs` の `task-wasm-threads.c` の扱い（4935〜4970 行）。機械的な改名の `fixes`: G24 の `tsuzuri fix` |
| 主な影響ファイル | `std/DataChannel.tz`（新規。旧 `std/Channel.tz` の改名）、`std/Channel.tz`（新しい中身）、`src/check.rs`・`src/control.rs`・`src/ownership.rs`・`src/llvm_sync.rs`（または新規 `src/llvm_channel.rs`）・`src/llvm.rs`・`src/driver.rs`・`src/stdlib.rs`・`src/polymorph.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/kg_capture.rs`・`src/diagram/concurrency.rs`・`src/hints.rs`・`src/diagnostic_kinds.rs`・`src/explain/`・`src/runtime/tzroutine.c`・`tests/tzroutine_runtime.c`・`tests/channel.rs`（新規）・`tests/removed_features.rs`（新規）・`tests/fixtures/data_channel`（旧 `tests/fixtures/channel` の改名）・`tests/fixtures/data_channel_threads`（旧 `channel_threads` の改名）・`tests/fixtures/channel/Main.tz`（新しい中身）・`tests/concurrency.rs`・`tests/tasks.mjs`・`tests/wasm_threads.mjs`・`tests/wasm_threads_stress.mjs`・`tests/trap_return_sync.mjs`・`tests/llvm_handoff.rs`・`tests/diagram.rs`・`tests/golden/diagram/`・`tests/diagnostic_v2.rs`・`tests/param_facts.rs`・`tests/features.mjs`・`tests/std_argument_order.rs`・`tests/std_examples.rs`・`vsc/src/core.ts`・文書 |

## 実装者向け早見

- 範囲: ① 旧 `Channel`（`bounded`・`Sender`／`Receiver`・最後の `Sender` の drop で閉じる・`send` が `Error item` を返す）を、**意味も実装も変えずに** `DataChannel` へ改名する。② 空いた `Channel` の名前で、Go の `chan T` に当たる `Channel<T>` を入れる（`make`（容量 0 はバッファなし）・`send`・`recv`・`try_send`・`try_recv`・`close`・`length`・`capacity`・`share`、`for x in ch do`、要素の借用の検査、drop）。③ 旧の名前（`Channel.bounded` など）を書くと `E1040` で `DataChannel` の名前を示し、`tsuzuri fix` で機械的に直せる。④ 既定の WASM（import なし）で、tzroutine を生成しないプログラムは新しい `Channel` を 1 本の実行として使える。ランタイムは TZ01 の `tsuzuri_tz_chan_*`。`select` は TZ04。
- `DataChannel` の待ちは TZ12 まで今の `task.c`（ネイティブ）・`channel-wasm.ll`（既定の WASM）・`task-wasm-threads.c`（threads）のまま。tzroutine のランタイムへ移すのは TZ12。
- 最初に読む: ① 仕様 1〜9 ② `std/Channel.tz`（15 行）③ `src/llvm_sync.rs` 277〜470 行 ④ `src/ownership.rs` 2725〜2760 行・2925〜2950 行 ⑤ `src/control.rs` 75〜260 行 ⑥ TZ01 の仕様 1.2・1.9 ⑦ 「改名の一覧」。
- 進め方: **改名（手順 2）を先に 1 つのコミットで終え、全テストを改名だけで通してから**、新しい `Channel` を足す。
- 最初に書くテスト: 改名の後の `node tests/features.mjs target/debug/tsuzuri data_channel`（旧の `channel` の suite の改名。期待値は同じ）→ `tests/channel.rs` の `makes_sends_and_receives` → E2E の `ping_pong` → `tests/removed_features.rs` の改名の行。
- 触ってはいけない: `DataChannel` の意味・待ちの規則・生成・ランタイム（名前だけが変わる）。Task（TZ12 まで）。どちらのチャンネルも使わないプログラムの IR。診断の kind（`channel_item_holds_borrow` は両方のチャンネルが使う）。
- 完了の見分け方: 次が `sum=5050` を出し、`live == 0`。旧の `Channel.bounded 3` を書くと `E1040`（kind `renamed_to_data_channel`）で `DataChannel.bounded` を示し、`tsuzuri fix` がそう直す。直したプログラムは、旧と同じ結果を出す。

```tsuzuri
let jobs: Channel<i64> = Channel.make 0
let producer = Channel.share jobs
tzroutine {
    for i in 1 ..= 100 do
        Channel.send producer (i as i64)
    Channel.close producer
}
let mut sum = 0i64
for job in jobs do
    sum = sum + job
$"sum={sum}"
```

## 目的

tzroutine の間で値を受け渡す Go のチャンネルを、Tsuzuri の所有権と合う形で入れる。容量 0 のチャンネルは送り手と受け手が出会うまで両方を待たせ（受け渡しの同期）、容量のあるチャンネルはキューになる。待つのは tzroutine だけなので、旧 `Channel` の「待つタスクの数がスレッド数を超えると完了しない」制約はない。

名前は利用者の追加の指示（D-61 の決定 5・6）で、新しいものを `Channel`、旧 `Channel` を `DataChannel` にする。`DataChannel` は送り手と受け手の端が分かれ、最後の送り手の drop で閉じ、受け手がいなければ `send` が値を返す、という Go にない性質を持つので、両方を残す（使い分けは仕様 9 の表）。

## 着手条件と停止条件

### 着手条件

- TZ02 が done（`grep -n "RoutineGo" src/check.rs` が見つかる。`node tests/tzroutine.mjs target/debug/tsuzuri` が通る）。
- 基準: `cargo nextest run --locked` の件数、`node tests/features.mjs target/debug/tsuzuri channel concurrency` と `node tests/tasks.mjs target/debug/tsuzuri`・`node tests/wasm_threads.mjs target/debug/tsuzuri` の結果。共有の数（`Builtin::ALL.len()`・`RESERVED_MODULES.len()`・`OPT_IN` の数・codec の tag）を今の値から読む。
- 着手の前に `git grep -nE "Channel\.(bounded|send|recv|clone_sender|Sender|Receiver|__close)" -- src std tests examples benchmarks _tsuzuri docs guide scripts vsc README.md` の件数をファイルごとに控える（改名の漏れの確認に使う）。

### 停止条件

- **S1** 改名だけの段階（手順 2）で、旧の suite（`channel` → `data_channel`）・`tests/concurrency.rs`・`tests/tasks.mjs`・`tests/wasm_threads.mjs`・`tests/trap_return_sync.mjs` の期待値を、名前以外の理由で変えないと通らない。
- **S2** `for x in ch` の脱糖で、`break`・`continue` の意味が変わる。選択肢: A `sequence_loop` と同じく、生成した `while` を `normal_loop_depth` の 1 段として数える、B 止めて報告する。
- **S3** 要素の借用の検査の文と位置が、`DataChannel.send` の検査と違う形になる（kind・コードは同じで、関数の名前だけが変わるはず）。
- **S4** 確保の追跡で `live != 0`。2 回直して直らなければ止める。
- **S5**（既定の WASM）`tzroutine.c` を `-DTZR_SINGLE` で wasm32 向けに作ると、import が 1 つでも出る（`WebAssembly.Module.imports` が空でない）。
- **S6** `E1040` を出すために、名前の解決の誤りの経路を大きく変える必要がある（利用者が定義した `Channel.Sender` などを壊す）。
- **S7** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認）

- 旧 `Channel` は `Channel.bounded n`（`n ≥ 1`）が `(Sender<T> * Receiver<T>)` を返し、最後の `Sender` の drop で閉じ、`send` は閉じていれば `Error item` を返す（[channel.md](../_tsuzuri/language-reference/built-in-types-and-modules/channel.md)）。ネイティブの待ちは `src/runtime/task.c` のプールの mutex と OS スレッドの条件変数（574〜699 行。待つスレッドは未着手の仕事を手伝い、全員が待つと `deadlock: every task is waiting on a channel`）。既定の WASM は `channel-wasm.ll`（待つべきときに状態 2）、`--wasm-feature threads` は `task-wasm-threads.c` の Channel の部分。Windows でも動く（`task-windows.h`）。
- 端は `record Sender<'a> { block: i64 }`（`std/Channel.tz`）で、ブロックのアドレスを `i64` で持つ不透明な record。構築・フィールド参照・分解は `E1022`、`export`・`extern` の境界は `E1008`、`const` は `E1026`、関数値への捕捉は `E1005`（`Drop` を持つので `Capture` でない）。`Sync` の扱いは `src/check.rs` の `sync_here`。
- 組み込みは `ChannelBounded`・`ChannelSend`・`ChannelRecv`・`ChannelCloneSender`・`ChannelCloseSender`・`ChannelCloseReceiver`（`is_channel()` は名前が `Channel.` で始まるもの。3456 行）。生成は `src/llvm_sync.rs`。C の記号は `tsuzuri_channel_send`・`recv`・`clone_sender`・`close` と `tsuzuri_mutex_wait_ok`（`Mutex.with_lock` の中の Channel の操作をトラップにする検査）。
- 要素の借用の検査は、`Channel.send` の 2 番目の引数（`index == 1`）について、要素の型が借用を運びうるとき（`carries_loans`）、その値が loan を持てば `E1013`（`ChannelItemHoldsBorrow`）。部分適用・関数値・pipe は `MustApplyDirectly`（`E1013`）。
- `for x in e` は `src/control.rs` で、配列・リスト・`Vec`・文字列・範囲・`Seq` だけ。ほかは `E1005`（`ForSourceNotIterable`）。`Seq` は `sequence_loop` が `while` と `match` に脱糖する。
- TZ01 のランタイムに `tsuzuri_tz_chan_init`・`share`・`release`・`drain`・`send`・`recv`・`close`・`length`・`capacity` がある。

## 変えてはいけない意味

- `DataChannel` のすべて（名前を除く）: 型、`bounded` の容量の規則、`send`・`recv` の結果、閉じ方、待ちの規則、トラップの文、生成コード、ランタイム、対応するターゲット（Windows・既定の WASM・threads を含む）。`DataChannel` を使うプログラムの IR は、型と関数の名前（`Channel.Sender` → `DataChannel.Sender` などの記号の名前）だけが変わる。
- Task のすべて（TZ12 まで）。Task のワーカーの中の新しい `Channel` の待ちは、そのワーカーの OS スレッドを止める（擬似 G。TZ01 の D4）。
- 要素の所有権・FIFO・1 つの要素は 1 人の受け手だけ（両方のチャンネル）。
- 既定の WASM の import なし（S5）。

## Phase 分割

| Phase | 内容 |
| --- | --- |
| 1 | 仕様 1〜9（新しい API、`for`、借用、生成、デッドロック、ターゲット、既定の WASM、改名と `E1040`、使い分けの文書）。**1 つの PR で全部**（名前が入れ替わるので分けられない） |
| 2 | 仕様 10（向きのある型）。**人間が求めたときだけ** |

## 仕様

### Phase 1

#### 1. 型と API（`std/Channel.tz` を新しく書く。旧の中身は `std/DataChannel.tz` へ移る）

```tsuzuri
/// A channel of Go: a queue of `capacity` items that tzroutines send to and receive from. ...
record Channel<'a> { block: i64 }
instance Drop<Channel<'a>> {
    fn drop channel = Channel.__release channel
}
/// What `Channel.try_send` gives back when it cannot send now.
union SendError<'a> = Full of 'a | Closed of 'a
/// What `Channel.try_recv` found.
union Received<'a> = Item of 'a | Empty | Closed
```

| 関数 | 型 | 意味 | Go |
| --- | --- | --- | --- |
| `make` | `Send<'a> => i64 -> Channel<'a>` | 容量 `n` のチャンネル。0 はバッファなし。負はトラップ（`assert`）。`128 + n × 要素の大きさ` が `i64` に入らなければトラップ（`allocation size`） | `make(chan T, n)` |
| `send` | `ref Channel<'a> -> 'a -> unit` | 受け手が待っていれば直接渡し、空きがあれば入れ、なければ待つ。閉じていれば、または待つ間に閉じられたら、`Tsuzuri runtime: send on closed Channel` を書いてトラップ（`assert`） | `ch <- v` |
| `recv` | `ref Channel<'a> -> Maybe<'a>` | 要素があれば `Some`。空なら送り手か `close` を待つ。閉じて空なら `None`（待たない） | `v, ok := <-ch` |
| `try_send` | `ref Channel<'a> -> 'a -> Result<unit, SendError<'a>>` | 待たない。入れられなければ `Error (Full item)`、閉じていれば `Error (Closed item)`（要素を返す。トラップしない） | `select` + `default` |
| `try_recv` | `ref Channel<'a> -> Received<'a>` | 待たない。`Item v`・`Empty`（開いていて空）・`Closed`（閉じて空） | 同上 |
| `close` | `ref Channel<'a> -> unit` | 閉じる。待っている受け手は `None` で、送り手はトラップで起きる。2 回目は `Tsuzuri runtime: close of closed Channel` を書いてトラップ（`assert`） | `close(ch)` |
| `length` | `ref Channel<'a> -> i64` | 入っている要素の数 | `len(ch)` |
| `capacity` | `ref Channel<'a> -> i64` | 容量 | `cap(ch)` |
| `share` | `ref Channel<'a> -> Channel<'a>` | 同じチャンネルのハンドルをもう 1 つ作る（tzroutine へ移すため） | （Go はポインタを共有する） |

- 型は `Channel<i64>` と書く（`Mutex<T>`・`Atomic<T>` と同じく、モジュールと同じ名前の record）。
- 引数の順序（D-59）: 資源のハンドルが先頭（`tests/std_argument_order.rs` の `MODULES` の既存の `("Channel", Exempt(BUILTIN_HANDLE))` のまま）。
- 呼び出しでは `ref` を省ける（引数の型が `ref Channel<'a>` なので、既存の暗黙の借用が働く。例: `Channel.send ch 1`）。
- `Channel<'a>` は非 Copy の所有値で、`Send` かつ `Sync`（`'a` は `make` で `Send` と検査済み）。`Capture` ではない（`Drop` を持つ。関数値に捕捉すると `E1005`。tzroutine と `Owned.function` へは移せる）。
- 不透明: `Channel { block: … }` の構築、`ch.block`、パターン分解、更新構文は `E1022`。`export def`・`extern def` の境界は `E1008`、`const` は `E1026`。`Eq`・`Ord`・`Hash`・`Display` の instance はない。`Channel.__release` を std の外から呼ぶと `E1022`（`DataChannel.__close_sender` と同じ扱い）。
- `make` の型注釈: 要素の型が決まらない `Channel.make 0` は `E1015`（`ambiguous polymorphic use`）。`let ch: Channel<i64> = Channel.make 0` と書く。

#### 2. `for x in ch do body`

- `ch` の型が `Channel<'a>` か `ref Channel<'a>` なら、`x : 'a` で、閉じて空になるまで受け取り続ける（Go の `for v := range ch`）。
- 脱糖（型検査の中で、`sequence_loop` と同じく生成した AST を検査する）:

```text
{
    let $channel = ref <ch>        // ch が一時の値なら、ループの間それを保持する
    let mut $open = true
    while $open do
        match Channel.recv $channel with
        | Maybe.Some x -> body
        | Maybe.None -> $open = false
}
```

- `body` の `break` は `for` を抜け、`continue` は次の受信へ進む。`x` のパターンは `for` の既存の規則（単純な名前か、ほかのパターンは `match` に包む）。
- `for` の要素は受け手のもの（所有値。`string` のような Copy でない要素も受け取れる）。

#### 3. 要素の借用

- `Channel.send` と `Channel.try_send` の要素（2 番目の引数）は、`DataChannel.send`（旧 `Channel.send`）と同じ規則で検査する: 要素の型が借用を運びうるとき（関数値を含む）、値が loan を持てば `E1013`（kind `channel_item_holds_borrow`、文は今の `a channel item cannot hold a borrow, …`）。その型の `Channel.send` を部分適用・関数値・pipe にすると `E1013`（kind `must_apply_directly`、文は今の `Channel.send must be fully applied directly when its item may hold a borrow, …` を、`try_send` では `Channel.try_send …` にする）。
- 受け取った要素は loan を持たない。

#### 4. 生成コードとランタイム

| 操作 | 生成 |
| --- | --- |
| `make n` | `n < 0` → トラップ（`assert`）。`n × size` の上限（`(i64::MAX - 128) / max(size, 1)`）→ トラップ（`allocation size`）。`@tz.alloc(128 + n × size)`、`call void @tsuzuri_tz_chan_init(ptr %block, i64 size, i64 n)`、`{ i64 ptrtoint }` の record（`mark_live`） |
| `send ch v` | `v` を entry の `alloca` に store、`%s = call i32 @tsuzuri_tz_chan_send(ptr %block, ptr %slot, i32 1)`。`%s == 1`（閉じている）→ `call void @tsuzuri_tz_report(i32 1)` + トラップ（`assert`）。`%s == 2`（デッドロック）→ トラップ（`assert`。文はランタイムが出した） |
| `recv ch` | 要素の型の `alloca`、`%s = call i32 @tsuzuri_tz_chan_recv(ptr %block, ptr %slot, i32 1)`。0 → `Some (load %slot)`、1 → `None`、2 → トラップ |
| `try_send ch v` | `%s = call … send(…, i32 0)`。0 → `Ok ()`、3 → `Error (Full (load %slot))`、1 → `Error (Closed (load %slot))` |
| `try_recv ch` | `%s = call … recv(…, i32 0)`。0 → `Item (load %slot)`、3 → `Empty`、1 → `Closed` |
| `close ch` | `%s = call i32 @tsuzuri_tz_chan_close(ptr %block)`。1 → `report(2)` + トラップ |
| `length`・`capacity` | `call i64 @tsuzuri_tz_chan_length(ptr)`・`capacity(ptr)` |
| `share ch` | `call void @tsuzuri_tz_chan_share(ptr)`、同じ `block` の新しい record（`mark_live`） |
| drop（`__release`） | `%last = call i32 @tsuzuri_tz_chan_release(ptr)`。非 0 なら、`drain` が 0 を返すまで `alloca` に取り出して `drop_value`、そのあと `@tz.free(block)` |

- ランタイムに足す（`src/runtime/tzroutine.c`。TZ01 の仕様 1.2 の表にも足す）:

```c
/* The message of a failure that the generated code traps for (TZ03, TZ06). Writes nothing on a
 * target without standard error. */
TZR_API void tsuzuri_tz_report(int32_t what) {
    static const char *const messages[] = {
        "",
        "send on closed Channel",
        "close of closed Channel",
        "negative WaitGroup counter",
        "Mutex.with_lock cannot be nested",
    };
    if (what > 0 && what < (int32_t)(sizeof messages / sizeof messages[0]))
        fprintf(stderr, "Tsuzuri runtime: %s\n", messages[what]);
}
```

  番号は固定（TZ06 が 3〜6 を使う。新しい文は末尾に足す）。`tests/tzroutine_runtime.c` に `report` のケース（標準エラーに 4 つの文が出る。子プロセスで検査）を足す。
- 要素の大きさ `size` は `ptrtoint (ptr getelementptr (T, ptr null, i32 1) to i64)`。0 でもよい（`unit` のチャンネル。ランタイムは 0 バイトを写す）。
- 宣言は `intrinsics` へ（重複しない）。`Channel` を使うプログラムは IR に `tsuzuri_tz_` の記号を持つので、TZ02 の `tzroutine_runtime` でランタイムが連結される（`DataChannel`・Task を使うプログラムでは、TZ12 まで `task.c` も連結される）。

#### 5. デッドロック

- `main` がチャンネルを待ち、ほかに誰もいないと、TZ01 の判定が `Tsuzuri runtime: all tzroutines are asleep - deadlock! (1 waiting: Channel.recv)` を書き、`recv` が状態 2 を返し、生成コードが `recv` の呼び出し地点でトラップする（`--trap-info` の `tsuzuri run` は、その行を `E2005` の位置にする）。
- Task のワーカー（擬似 G）が `Channel` で待つ場合も同じ判定に入る。`DataChannel` の待ちは TZ12 まで `task.c` の判定（`deadlock: every task is waiting on a channel`）のまま。

#### 6. ターゲット

| 構成 | 新しい `Channel` | `DataChannel` |
| --- | --- | --- |
| native の macOS・Linux | 動く | 動く（今と同じ） |
| native の Windows | TZ01 の Phase 2 が済むまで `E2002`（TZ02 の表） | 動く（今と同じ） |
| 既定の wasm32・wasm64（import なし） | tzroutine を生成しないプログラムは 1 本の実行として使える（仕様 7）。生成するプログラムは TZ02 の `E2000` | 動く（`channel-wasm.ll`。今と同じ） |
| `--wasm-feature threads` | `E2000`（kind `option_conflict`、`Channel cannot be used with --wasm-feature threads; use DataChannel, which runs on the threads backend`） | 動く（`task-wasm-threads.c`。今と同じ） |
| `--wasm-feature jspi` で tzroutine を生成する | TZ09 まで `E2000`（TZ02 の表） | TZ09 の表 |
| `--freestanding` | `E2000`（TZ02 の一覧の `tsuzuri_tz_` の行が当たる） | `E2000`（今と同じ。文の機能名を `DataChannel` に） |

#### 7. 既定の WASM の 1 本の実行（`TZR_SINGLE`）

- 既定の WASM で、tzroutine を生成しないプログラムの新しい `Channel` は、`src/runtime/tzroutine.c` を `-DTZR_SINGLE` で wasm32（wasm64）向けに作ったものを連結する。`TZR_SINGLE` では、スレッド・`mmap`・TLS・信号・標準エラーを使わない: G は 1 つ（`main` の擬似 G に当たる静的な G）、ロックは何もしない、`tzr_park` は解放の関数を呼んでから即座に 1（デッドロック）を返す（起こせる者がいないため）、報告の文は出さない。チャンネル・`select`・parking lot の関数はそのまま使う。待つべき操作は状態 2 を返し、生成コードがトラップする（`DataChannel` の `channel-wasm.ll` と同じ振る舞い）。

#### 8. 旧の名前の改名（`DataChannel`）と `E1040`

| 旧 | 新 |
| --- | --- |
| モジュール `Channel`（旧 API） | `DataChannel` |
| `Channel.bounded n` | `DataChannel.bounded n` |
| `Channel.send (ref sender) item`（`Sender` の端） | `DataChannel.send (ref sender) item` |
| `Channel.recv (ref receiver)`（`Receiver` の端） | `DataChannel.recv (ref receiver)` |
| `Channel.clone_sender (ref sender)` | `DataChannel.clone_sender (ref sender)` |
| 型 `Channel.Sender<T>`・`Channel.Receiver<T>` | `DataChannel.Sender<T>`・`DataChannel.Receiver<T>` |
| std 専用の `Channel.__close_sender`・`__close_receiver` | `DataChannel.__close_sender`・`__close_receiver` |

- 旧の名前のうち、新しい `Channel` にないもの（`Channel.bounded`・`Channel.clone_sender`・型 `Channel.Sender`・`Channel.Receiver`）を書くと `E1040`（kind `renamed_to_data_channel`）。文は `'Channel.bounded' is now 'DataChannel.bounded'`（書いた名前ごと）、help は `the channel with separate Sender and Receiver ends was renamed to DataChannel; the name Channel is now Go's two-way channel (Channel.make)`。`--json` の `fixes` に、名前の置換を 1 つ付ける（適用性 `certain`。`tsuzuri fix` が適用する）。
- 新しい `Channel` にもある名前（`Channel.send`・`Channel.recv`）に、`DataChannel.Sender`・`Receiver` を渡すと、型の不一致の `E1003` になる。G23 のヒント DB に、`expected Channel<…>, found DataChannel.Sender<…>`（`Receiver` も）のときの help `use DataChannel.send for a DataChannel.Sender`（`recv` も）と、名前の置換の `fixes`（適用性 `certain`）を足す。
- 判定は名前の解決の **後**: 新しい `Channel` にも利用者の定義にも解決しないときだけ `E1040`（S6）。

#### 9. どちらを使うか（文書に載せる表）

| | `Channel<T>`（新） | `DataChannel`（旧 `Channel`） |
| --- | --- | --- |
| 形 | 1 つのハンドルで送受信（Go の `chan T`）。`share` で複製 | 送り手 `Sender` と受け手 `Receiver` に分かれる。`clone_sender` |
| 閉じ方 | `close` だけ（drop では閉じない） | 最後の `Sender` の drop で閉じる |
| 閉じた後の `send` | トラップ（Go の panic）。`try_send` は要素を返す | `Error item`（要素を返す） |
| 受け手がいないときの `send` | 待つ（受け手を数えない） | `Error item` |
| バッファなし | 容量 0 | なし（容量は 1 以上） |
| `select`・`for` | 使える（TZ04・仕様 2） | 使えない |
| 待ち | tzroutine だけが待つ | TZ12 まで OS スレッドを止める（Task のプールの規則）。TZ12 から tzroutine だけが待つ |
| `--wasm-feature threads` | 使えない | 使える |

### Phase 2: 向きのある型（人間が求めたときだけ）

#### 10. `Channel.SendOnly<'a>`・`Channel.ReceiveOnly<'a>`

- Go の `chan<- T`・`<-chan T`。名前は、`DataChannel.Sender`・`Receiver` と混ざらないよう別にする（D9）。`Channel.send_only :: ref Channel<'a> -> SendOnly<'a>`・`Channel.receive_only :: ref Channel<'a> -> ReceiveOnly<'a>`（どちらも同じブロックのハンドルを 1 つ増やす。双方向へは戻せない）。
- 操作: `send`・`try_send`・`close` は `Channel` と `SendOnly`、`recv`・`try_recv`・`for` は `Channel` と `ReceiveOnly`、`length`・`capacity`・`share` は 3 つとも。
- 型付け: 組み込みのクラス `ChannelSend<'h>`・`ChannelRecv<'h>`（`'h : * -> *`。A10 の HKT）を足し、`Channel.send :: ChannelSend<'h> => ref 'h<'a> -> 'a -> unit` のようにする。instance は `ChannelSend` が `Channel` と `Channel.SendOnly`、`ChannelRecv` が `Channel` と `Channel.ReceiveOnly`（`Classes::intrinsic` で決める。利用者の instance は `E1016`）。向きの違う操作は `E1005`（`no instance for ChannelRecv<Channel.SendOnly>`）。
- **停止条件**: A10 の HKT の組み込みのクラスが、std の record を引数に取れない、または Phase 1 の型の表示・診断（`expected Channel<i64>`）が変わる。その場合は止めて、別名の関数で作るかを人間に尋ねる。

## 設計

### Phase 1: 改名（手順 2。新しい `Channel` の前に、これだけで全テストを通す）

| ファイル | 変更 |
| --- | --- |
| `std/DataChannel.tz`（新規） | 旧 `std/Channel.tz` の中身を写し、`///` の文の `Channel` を `DataChannel` に、`Channel.__close_sender` などを `DataChannel.__close_sender` に |
| `std/Channel.tz` | この段では消す（手順 3 で新しい中身を書く） |
| `src/stdlib.rs` | `SOURCES` に `DataChannel.tz`、`OPT_IN` に `OptIn { module: "DataChannel", names: &["DataChannel"], aliases: &[], uses: &[] }`、`RESERVED_MODULES` に `"DataChannel"`（件数のテスト + 1）、opaque の一覧の `Channel.Sender`・`Channel.Receiver` を `DataChannel.Sender`・`DataChannel.Receiver` に。手順 3 まで `Channel` の `OPT_IN` は空の中身を指せないので、手順 2 の間は `Channel` の行を外し、手順 3 で戻す |
| `src/check.rs` | `ChannelBounded`・`ChannelSend`・`ChannelRecv`・`ChannelCloneSender`・`ChannelCloseSender`・`ChannelCloseReceiver` を `DataChannelBounded`・`DataChannelSend`・`DataChannelRecv`・`DataChannelCloneSender`・`DataChannelCloseSender`・`DataChannelCloseReceiver` に改名し、名前を `"DataChannel.bounded"` などに、`sync_scheme` の型の `module: "Channel"` を `"DataChannel"` に、`is_channel()` を `is_data_channel()`（`DataChannel.` で始まる）に。13897 行のテストの `Channel.Sender` を `DataChannel.Sender` に |
| `src/check.rs`（Sync）・`src/polymorph.rs`・`src/ownership.rs` | `Channel.Sender`・`Receiver` の名前の比較（`sync_here`、`sent_item`、`PrivateStdPrimitive`、`MustApplyDirectly` の文の `Channel.send`）を `DataChannel` に |
| `src/llvm_sync.rs`・`src/llvm.rs`・`src/driver.rs` | 組み込みの参照の改名だけ。C の記号（`tsuzuri_channel_*`・`tsuzuri_mutex_wait_ok`）、ブロックの配置（`CHANNEL_*`）、`channel-wasm.ll`、`RuntimeSymbols` の `channel` の判定、`--freestanding` の一覧（文の機能名を `DataChannel` に）はそのまま |
| `src/runtime/task.c`・`task-wasm-threads.c`・`channel-wasm.ll` | 変更なし（コメントの `Channel` を `DataChannel` にしてよい） |
| `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/kg_capture.rs`・`src/diagram/concurrency.rs`・`src/hints.rs` | 組み込みの名前の改名（`CELL_EVIDENCE`・`CREATORS` の `Channel.bounded` → `DataChannel.bounded`）。件数は変わらない。`tests/fixtures/capabilities/std-table.txt` の名前 |
| `tests/std_argument_order.rs`・`tests/std_examples.rs` | `MODULES` の `("Channel", Exempt(BUILTIN_HANDLE))` を `("DataChannel", …)` に改名（手順 3 で `Channel` の行を足し直す）、`FLOORS` の `("Channel", 0)` を `("DataChannel", 0)` に |
| `vsc/src/core.ts` | 予約モジュールの集合に `DataChannel` |
| テスト・fixture・文書 | 「改名の一覧」 |

### Phase 1: 新しい `Channel`（手順 3 以降）

| ファイル | 変更 |
| --- | --- |
| `std/Channel.tz` | 新しく書く（仕様 1 の record・`Drop`・2 つの union）。各公開の名前に `///` の文書と G34 の例。`@since 0.1.0` |
| `src/stdlib.rs` | `OPT_IN` に `Channel` を戻す（`names: &["Channel"]`）。`RESERVED_MODULES` の `Channel` はそのまま。opaque の一覧に `Channel.Channel` |
| `src/check.rs` | `ChannelMake`・`ChannelSend`・`ChannelRecv`・`ChannelTrySend`・`ChannelTryRecv`・`ChannelClose`・`ChannelLength`・`ChannelCapacity`・`ChannelShare`・`ChannelRelease` を足す（`ALL`・`name`（`"Channel.make"` … `"Channel.__release"`）・`scheme`（`make` は `Send<'a>` の制約）・`is_channel()`（新しい組み込みだけ））。`__release` は std 専用（`E1022`）。`Channel.Channel` は `Sync` |
| `src/control.rs` | `for` の型の分岐に、`source.ty` が std の `Channel.Channel`（または その `ref`）のとき `channel_loop(source, pattern, body, span)` を足す。`ForSourceNotIterable` の文に `Channel` を足す（`for...in expects an array, list, string, utf8string, integer range, Seq, or Channel; …`） |
| `src/ownership.rs` | `sent_item` が `ChannelSend`・`ChannelTrySend` も返すようにする（`DataChannelSend` と並べる）。`MustApplyDirectly` の文を組み込みの名前から作る（`format!("{} must be fully applied directly …", builtin.name())`。`DataChannel.send` の文は改名の後のまま） |
| `src/llvm_sync.rs`（または新規 `src/llvm_channel.rs`） | 仕様 4 の生成。`DataChannel` の生成とは別の関数にする |
| `src/driver.rs` | 既定の WASM で `tzroutine_runtime` かつ tzroutine を生成しない（`!spawns`）とき、`tzroutine.c` を `-DTZR_SINGLE` で `task-wasm-threads.c` と同じ旗（`--target=wasm32-unknown-unknown -std=c11 -ffreestanding -fno-stack-protector -mbulk-memory -c -O*`。wasm64 は `--target=wasm64-unknown-unknown`）で作り、キャッシュの単位 `"wasm-tzroutine-single"` で連結する。TZ02 の WASM の拒否を、tzroutine を生成するプログラムだけに狭める。`--wasm-feature threads` と新しい `Channel` の組み合わせを `E2000`（仕様 6） |
| `src/runtime/tzroutine.c` | `tsuzuri_tz_report`。`#if defined(TZR_SINGLE)` の分岐（仕様 7）: `<pthread.h>`・`<sys/mman.h>`・`<signal.h>`・`<stdio.h>` を含めない、`tzr_lock` と `tzr_note` は空、`tzr_current()` は静的な擬似 G、`tzr_park` は `unlock(arg)` の後に 1、`tzr_ready` は何もしない、`tsuzuri_tz_go`・`sleep`・タイマー・スタック・M・P の部分は作らない（呼ぶプログラムは拒否済み）、`tsuzuri_tz_report` は空。`memcpy`・`memset` は `__builtin_memcpy`・`__builtin_memset`（freestanding） |
| `src/diagnostic_kinds.rs`・`src/explain/` | `E1040` を新しいコードとして登録し、kind `renamed_to_data_channel` を足す（G23 の仕様 S1）。`src/explain/codes/E1040.md` を書く（TZ12・TZ13 が kind を足す） |
| `src/check.rs`（名前の解決） | `removed_feature(module, name) -> Option<(DiagnosticKind, String /* message */, &'static str /* help */, Option<String> /* replacement */)>` を足し、解決できない修飾名・型名のときに呼んで `E1040` にする（TZ12・TZ13 が行を足す。他チケットへ提供するインターフェース） |
| `src/hints.rs` | 仕様 8 の `E1003` のヒント（G23 のヒント DB の形） |
| `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs` | 新しい組み込みの行（`DataChannel*` と同じ分類。`make`・`share` は `alloc`）。件数の pin |
| `src/capabilities.rs` | 新しい組み込みは権限なし。`tests/fixtures/capabilities/std-table.txt` の行 |
| `src/kg_capture.rs`・`src/diagram/concurrency.rs` | `CELL_EVIDENCE`・`CREATORS` に `Channel.make` を足す |
| `tests/std_argument_order.rs`・`tests/std_examples.rs` | `MODULES` に `("Channel", Exempt(BUILTIN_HANDLE))` を足し直す。`FLOORS` に `("Channel", <例の数>)` |

### Phase 1: `channel_loop`

- `sequence_loop`（`src/control.rs`）を読み、同じ形で生成する AST（`ExprKind::While`・`ExprKind::Match`・`ExprKind::Name`・`ExprKind::Call` の `Channel.recv`）を組み立てて `self.expression` で検査する。`$channel`・`$open` の名前は `Provenance::Generated`（`$iteration{start}` と同じ作り方で、位置を付けて一意にする）。
- 深さの上限: `sequence_loop` の `depth` の計算と同じ式で `MAX_NESTING` を検査する。
- `ch` が場所（ローカル・フィールド）なら `ref ch` を束縛し、一時の値なら値を `$owned` に束縛してから `ref $owned`（ループのあとで drop）。
- `DataChannel.Receiver` は `for` の対象にしない（今と同じく `E1005`）。

### 改名の一覧（テスト・ベンチマーク・文書。手順 2）

**期待値は変えない。名前だけを直す。** 着手条件の `git grep` で見つかったファイルを全部処理する。

| 種類 | ファイル | 処理 |
| --- | --- | --- |
| C の単体テスト | `tests/channel_runtime.c` | そのまま（`task.c` の C の記号は変えない）。ファイル名はそのままでよい |
| fixture | `tests/fixtures/channel` | `tests/fixtures/data_channel` へ移し、`Channel.` を `DataChannel.` に（`Channel.Sender` の型も）。手順 3 で `tests/fixtures/channel` に新しい中身を作る |
| fixture | `tests/fixtures/channel_threads` | `tests/fixtures/data_channel_threads` へ移し、同じく改名 |
| suite | `tests/features.mjs` の `channel` | suite の名前を `data_channel`、fixture を `data_channel` に。期待値はそのまま。手順 3 で新しい `channel` の suite を作る |
| runner | `tests/tasks.mjs`・`tests/wasm_threads.mjs`・`tests/wasm_threads_stress.mjs` | fixture の名前（`channel_threads` → `data_channel_threads`）と、ソースの中の `Channel.` |
| 型のテスト | `tests/concurrency.rs` | 例の `Channel.` を `DataChannel.` に（コード・kind の期待は同じ） |
| trap-mode return | `tests/trap_return_sync.mjs`・`tests/fixtures/trap_return_sync` | 改名 |
| IR の受け渡し | `tests/llvm_handoff.rs` の `channel_std`・`channel_user` の fixture | 改名（期待が IR の記号の名前を含むなら、その名前も） |
| 図 | `tests/diagram.rs`・`tests/fixtures/diagram/src/flow/{Main,Worker}.tz`・`kg/flow/*.jsonl`・`tests/golden/diagram/*` | 改名し、golden を作り直す（`tsuzuri kg diagram` の出力。節点の名前が `DataChannel.bounded` に変わる以外の差がないことを `git diff` で確かめてから保存する） |
| 診断・facts・ヒント | `tests/diagnostic_v2.rs`・`tests/param_facts.rs`・`src/hints.rs` の例 | 改名 |
| ベンチマーク | `benchmarks/` の `Channel.`（`grep -rn "Channel\." benchmarks`） | 改名 |
| 言語リファレンス | `built-in-types-and-modules/channel.md` | `datachannel.md`（`check-cli-docs` の `stdPage` の名前）へ移し、`Channel.` を `DataChannel.` に、題を「DataChannel」に、先頭に「旧 `Channel`。TZ03 で改名した。Go の双方向のチャンネルは [Channel](./channel.md)」の注記。手順 3 で `channel.md` に新しい中身を書く |
| 言語リファレンス | `async-tasks-and-lazy/task.md`（「共有状態と Task.scope」の Channel の表と箇条）、`mutex.md`（ロックの中の Channel のトラップ）、`constraints.md`（`Channel.bounded` を包む `make` の例）、`compiler/webassembly.md`（threads の Channel）、`compiler/native-interop.md`（371 行の Mutex・Channel のトラップ）、`compiler/diagnostics.md`（`E1022` の不透明な型の行）、`types-and-type-inference/constraints.md` の `Sync` の表、`index.md` | 改名（`DataChannel` へのリンクに） |
| 仕様 | `docs/language.md`（`### Channel`、3674 行）・`docs/architecture.md`（`### Channel（F10 Phase 2）`、1099 行） | `### DataChannel` に改名し、手順 3 で新しい `### Channel` の節を足す |
| ガイド | `guide/` の Channel の言及 | 改名（`node scripts/check-guide.mjs`） |
| ツール | `scripts/check-cli-docs.mjs`（`STD_PAGES` に特別な行があれば）・`vsc/` の補完の型の一覧 | 改名 |

## 前提とする他チケットのインターフェース

- TZ01 の仕様 1.2（`tsuzuri_tz_chan_*`）と構造体の大きさ（ヘッダー 128 バイト）。TZ02 の `tzroutine_runtime`・`spawns`・`tests/tzroutine.mjs`。

## 他チケットへ提供するインターフェース

- 組み込み `Builtin::Channel*` と std の型 `Channel.Channel`（TZ04 の `select` の腕の型の検査、TZ05・TZ07 が返す型）。
- 組み込み `Builtin::DataChannel*` と、C の記号 `tsuzuri_channel_send`・`recv`・`clone_sender`・`close` の ABI（状態 0・1・2）とブロックの配置（`CHANNEL_*`。先頭 80 バイト）。TZ12 が、この ABI のまま `DataChannel` の待ちを tzroutine のランタイムへ移す。
- `tsuzuri_tz_report` の番号（1・2 は TZ03、3〜6 は TZ06）。
- 要素の借用の検査の入口（`sent_item` が返す組み込みの集合。TZ04 が `select` の送信の腕に使う）。
- `TZR_SINGLE` の組み方（TZ06 の同期の型が既定の WASM で同じ経路を使う。TZ09 の `TZR_JSPI` はこの分岐の隣に置く）。
- `E1040` と `removed_feature`（TZ12・TZ13 が行を足す）。`tests/removed_features.rs`。

## 実装手順

### Phase 1

1. **変更**: ブランチ `feat/tz03-channel`、基準と参照の件数の控え（着手条件）。
2. **変更**: 改名（設計の「改名」の表と「改名の一覧」）。新しい `Channel` はまだ足さない。**確認**: `cargo nextest run --locked`、`node tests/features.mjs target/debug/tsuzuri data_channel concurrency`、`node tests/tasks.mjs target/debug/tsuzuri`、`node tests/wasm_threads.mjs target/debug/tsuzuri`、`node tests/trap_return_sync.mjs target/debug/tsuzuri`、`node tests/diagram.mjs` があれば（または `cargo test --test diagram`）。**名前以外の期待値は 1 つも変えない**（S1）。着手条件の `git grep` の件数（`Channel.bounded` など）が、`_features/` の履歴を除いて 0。ここで 1 つのコミットにする。
3. **変更**: `std/Channel.tz` の新しい中身、`stdlib.rs` の `Channel` の `OPT_IN`、新しい組み込みの宣言と型（`check.rs`）。生成はまだ（仮の拒否）。**確認**: `tests/channel.rs` の型の検査: 受理 `let ch: Channel<i64> = Channel.make 0`、`Channel.send ch 1`、`match Channel.recv ch with | Maybe.Some v -> v | Maybe.None -> 0`、`try_send`・`try_recv`・`close`・`length`・`capacity`・`share`。拒否: `Channel.make 0` だけ（`E1015`）、`Channel<Rc<i64>>` の `make`（`E1013`）、`Channel { block: 0 }`（`E1022`）、`ch.block`（`E1022`）、関数値への捕捉（`E1005`）、`Channel.__release ch`（`E1022`）、`export def f :: Channel<i64> -> i64`（`E1008`）。同じプログラムで `Channel` と `DataChannel` を両方使える。
4. **変更**: 生成（仕様 4）と `tsuzuri_tz_report`。**確認**: IR が決定的で、`tsuzuri_tz_chan_*` の宣言が 1 つずつ。`node tests/tzroutine_runtime.mjs`（`report` を含む）。
5. **変更**: `for x in ch`（`channel_loop`）。**確認**: 型の検査（`x` の型、`break`・`continue` の受理、`for x in 5` の文が変わったこと、`DataChannel.Receiver` の `for` は今と同じく `E1005`）。
6. **変更**: 要素の借用（`ownership.rs`）。**確認**: 拒否 `Channel.send ch (\x -> x + data[0])`（`data` が引数の `ref [i64]`）→ `E1013`（`channel_item_holds_borrow`）、部分適用 → `E1013`（`must_apply_directly`）。受理: コピーを捕捉した関数、`Owned.function`。`DataChannel.send` の同じ検査の文と位置が手順 2 の後と同じ（S3）。
7. **変更**: `E1040`（`renamed_to_data_channel`、`removed_feature`）と `E1003` のヒント。**確認**: `tests/removed_features.rs`（仕様 8 の各行が `E1040` と help と `fixes` を出す、利用者が `Channel.Sender` という型を自分のモジュールに定義したプログラムは通る、`tsuzuri fix` が旧のプログラムを `DataChannel` の名前に直し、直したプログラムが旧と同じ結果を出す（`tests/fixtures/data_channel` を旧の名前に戻した写しで確かめる））。
8. **変更**: 既定の WASM の `TZR_SINGLE`（`tzroutine.c` とドライバー）。**確認**: `clang --target=wasm32-unknown-unknown -std=c11 -ffreestanding -fno-stack-protector -DTZR_SINGLE -Wall -Wextra -Werror -c src/runtime/tzroutine.c`。新しい `channel` の suite の WASM のケースで import なし（S5）。
9. **変更**: 効果・事実・権限・図（新しい組み込みの行）と、新しい E2E（`tests/fixtures/channel`・`features.mjs` の `channel`）。**確認**: `cargo nextest run --locked`、`node tests/features.mjs target/debug/tsuzuri`（全部）、`node tests/tzroutine.mjs target/debug/tsuzuri`。
10. **変更**: 文書と規則 ID。**確認**: `check-docs`・`check-spec`・`conformance`・`gen-docs --check`・`check-cli-docs`（`Channel`・`DataChannel` の全公開関数が、それぞれ `channel.md`・`datachannel.md` に書かれていること）・`check-guide`。
11. **変更**: T2。PR に `ci:full`。

### Phase 2

1. 仕様 10。先に A10 の HKT のクラスで組み込みの instance が作れるかを、小さなテスト（`ChannelSend<Channel>` を `Classes::intrinsic` で真にする）で確かめる。だめなら仕様 10 の停止条件。

## テスト計画

### `DataChannel`（手順 2）

- 旧 `channel` の suite を `data_channel` に改名したものが、同じ期待値で通る（native・既定の WASM・threads）。`tests/channel_runtime.c`・`tests/tasks.mjs`・`tests/wasm_threads.mjs` の Channel の部分も同じ。

### E2E（新しい `tests/fixtures/channel/Main.tz`。native の `-O0`・`-O3`、`TSUZURI_MAXPROCS` 1 と 4）

| export／プログラム | 内容 | 期待（独立に計算した値） |
| --- | --- | --- |
| `ping_pong n` | バッファなしの往復 `n` 回（tzroutine が 1 足して返す） | `n`（`n` = 0・1・10000） |
| `pipeline n` | 生成 → 2 倍 → 合計の 3 段（各段が tzroutine、チャンネルは容量 0）。旧 `Channel`（`DataChannel`）で CPU 1 では完了しなかった形 | `n(n+1)`（`n` = 1000）。`TSUZURI_MAXPROCS=1` でも完了する |
| `fan_in` | 8 個の送り手が 1 つのチャンネルへ 1000 個ずつ。各送り手は終わりに別の容量 8 のチャンネルへ 1 を送り、受け手側の tzroutine が 8 個受けたら `close` | 合計 `8 × 1000 × 1001 / 2` |
| `fan_out` | 1 つの容量 16 のチャンネルを 4 個の受け手が `for` で読む | 受けた数の合計 1000、要素の合計が一致 |
| `buffered_order` | 容量 3 に 1・2・3 を送り、1 つ受け、4 を送り、残りを受ける | `1,2,3,4` の順（1 つの整数に符号化） |
| `closed_drain` | 容量 4 に 2 つ送って `close`、`recv` を 4 回 | `Some 1`・`Some 2`・`None`・`None` |
| `try_ops` | 容量 1: `try_send` 成功・`Full`、`try_recv` の `Item`・`Empty`、`close` 後の `try_send` は `Closed item`（要素が戻る）・`try_recv` は `Closed` | 符号化した整数 |
| `owned_items` | `string` と `[i64]` の要素を送り、半分を受けずにチャンネルを drop | `live == 0`、受けた分の長さの和 |
| `for_break` | `for x in ch` の中の `break`・`continue` | 期待の和 |
| `unit_channel` | `Channel<unit>` で 100 回の合図 | 100 |
| `length_capacity` | `length`・`capacity` | 期待値 |
| `both_kinds` | 同じプログラムで `Channel` と `DataChannel` を使い、値を `Channel` から `DataChannel` へ中継する（tzroutine と Task のタスクの両方） | 期待の和 |
| `task_worker`（TZ12 まで） | `Task.parallel` の 2 つのタスクが新しい `Channel` で受け渡す（容量 0。擬似 G の待ち） | 期待の和（P と無関係に完了する） |

- 既定の WASM（-O0・-O3）: `buffered_order`・`closed_drain`・`try_ops`・`length_capacity`・`unit_channel` を同じ期待値で。待つ操作（空の容量 0 の `recv`）は `traps` の欄でトラップ。import なし。
- トラップ（`traps` の欄）: 閉じたチャンネルへの `send`、2 回目の `close`、`make (-1)`。native では標準エラーの文（`send on closed Channel`・`close of closed Channel`）も照合する。
- デッドロック（実行ファイル）: `let ch: Channel<i64> = Channel.make 0` と `Channel.recv ch` → 標準エラーに `all tzroutines are asleep - deadlock! (1 waiting: Channel.recv)`、終了は異常。`tsuzuri run --json` の `E2005` の位置が `recv` の行。

### Rust（`tests/channel.rs`・`tests/removed_features.rs`）

- 手順 3・5・6・7 の受理と拒否、IR の決定性、どちらのチャンネルも使わないプログラムの IR が変わらないこと。

## ドキュメント

- `built-in-types-and-modules/datachannel.md`（手順 2 で旧 `channel.md` から移す）: 先頭に改名の注記と、仕様 9 の使い分けの表。待ちの規則（手伝い・スレッド数）は TZ12 まで今のまま書き、TZ12 で直す。
- `built-in-types-and-modules/channel.md` を新しく書く: この記事のポイント、基本（早見の `sum=5050`）、バッファなしとありの違い（図: 送り手と受け手の出会い）、閉じる（`close`・`None`・閉じた後の `send` のトラップ。drop では閉じない）、`for`、`try_send`・`try_recv`、`share` と所有権、要素と借用（`E1013` の例）、デッドロックの文、既定の WASM の 1 本の実行、`--wasm-feature threads` では `DataChannel` を使うこと、表現と性能（ヘッダー 128 バイト + 要素、要素のバイト列の写し、1 チャンネル 1 ロック）、API の表、Go との対応表（`make`・`<-`・`close`・`len`・`cap`・`range`）、**`DataChannel` との使い分け**（仕様 9 の表）と、旧 `Channel` を書いていたプログラムの直し方（`tsuzuri fix` で `DataChannel` の名前になる）。
- `async-tasks-and-lazy/tzroutine.md`（TZ02）に、チャンネルでの待ち合わせの例を足す。
- `loops-and-conditionals/for-in.md`（反復できるコレクションの表に `Channel<T>`）。
- 「改名の一覧」の言語リファレンス・`docs/language.md`・`docs/architecture.md`。`compiler/diagnostics.md`（`E1040` の行。生成の表は `gen-docs`）。
- `spec/rules/TSK.md`: `TSK-016`（`Channel.recv` MUST return `None` only after the channel is closed and every item sent before the close has been received.）、`TSK-017`（A `send` on a closed `Channel` MUST trap.）、`TSK-018`（Items of one `Channel` MUST be received in the order their sends completed.）、`TSK-019`（`for x in ch` MUST end when `ch` is closed and empty.）。適合性テストを足す。旧 Channel の規則が `spec/rules/` にあれば、文の `Channel` を `DataChannel` に（規則の意味は同じなので `retired` にしない）。

## 受け入れ条件

- [ ] 手順 2 の改名だけで、旧の全テストが名前以外の期待値を変えずに通った（コミットが分かれている）。
- [ ] 仕様 1〜9 のとおりに動き、拒否は表のコードと kind で出る。
- [ ] 新しい `Channel` の E2E が native の `-O0`・`-O3`、P 1 と 4、既定の WASM で通り、`live == 0`。`pipeline` が P 1 で完了する。
- [ ] `tsuzuri fix` が旧の名前を `DataChannel` に直し、結果が変わらない。
- [ ] 文書・規則 ID・診断の kind を更新し、検査が通る。

## 落とし穴

- **改名と新設を混ぜない**: 手順 2 で改名だけをして全テストを通してから、新しい `Channel` を足す。同時にすると、旧の期待値が変わったのか新しい実装の誤りなのかを区別できない。
- **組み込みの名前の衝突**: 旧の `ChannelSend`・`ChannelRecv` を手順 2 で `DataChannelSend`・`DataChannelRecv` に改名してから、新しい `ChannelSend`・`ChannelRecv` を足す。`grep -rn "Builtin::Channel" src/` を手順 2 の後で 0 件にしておく（効果・純粋・権限・KG の表の分類を取り違えないため）。
- **要素の `alloca` は entry の block に**: ループの中の `send` が毎回 `alloca` を足すとスタックが伸びる（旧の `channel_builtin` は `self.allocas` に入れる。同じ所に）。
- **`try_send` の失敗で要素を drop しない**: 失敗のとき要素は `alloca` の中に残っていて、`Error` に入れて返す。成功のときは `alloca` を drop しない（チャンネルへ移った）。
- **`recv` の `None` で `alloca` を読まない**: 中身は未初期化。
- **`__release` の drain はロックなしで呼ぶ**: 最後のハンドルなので、ほかに誰もいない。
- **`for` の脱糖で `ch` を消費しない**: `for x in ch` のあとも `ch` は使える。一時の値だけ、ループの後で drop する。
- **`TZR_SINGLE` の `tzr_park`**: 待ち行列に sudog を入れた後に 1 を返すので、sudog はスタックの上に残る。生成コードは状態 2 で必ずトラップするので問題ない（native の `doomed` と同じ）。トラップしない経路を作らない。
- **`FLOORS` の数**: `std/Channel.tz` の例の数と一致させる。`DataChannel` は旧と同じ 0。

## 対象外

- `select` の `DataChannel`（`select` は `Channel` だけ。TZ04）。nil チャンネル（`select` の腕の `when`。TZ04）。
- `DataChannel` の待ちを tzroutine のランタイムへ移すこと（TZ12）。
- チャンネルの比較（`==`）。
- `--wasm-feature threads` の上の新しい `Channel`（D8）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 双方向の 1 つの型 `Channel<'a>`（D-61 の決定 3） | 利用者の決定 | 承認済み |
| D2 | 新しいものの名前は `Channel`、旧 `Channel` は `DataChannel` に改名して残す。旧の名前は `E1040` と `tsuzuri fix` で移す | 利用者の追加の指示（D-61 の決定 5・6）。送受信の端が分かれ drop で閉じる性質は Go にないので、両方を持つ | 承認済み |
| D3 | 新しい `Channel` のハンドルは参照数（`share`）で、最後の drop で解放。drop では閉じない（Go と同じく `close` だけが閉じる） | Go と同じ意味。参照数は所有権と GC なしの解放のため | 既定案 |
| D4 | 閉じたチャンネルへの `send` と 2 回目の `close` はトラップ（Go の panic）。`try_send` は要素を返す | Go と同じ。値を失わない待たない経路を別に持つ | 既定案 |
| D5 | `recv` は `Maybe<'a>`（Go の `v, ok`） | 既定値（ゼロ値）を作らない（GUIDE §1 の 2） | 既定案 |
| D6 | 要素はバイト列として写し、型ごとの drop は生成コードが持つ（`DataChannel` と同じ） | ランタイムを型から独立に保つ | 既定案 |
| D7 | `for x in ch` を直接書ける | `Seq` の閉包は借用を返せず、所有値の要素を受け取る形が Go の `range` に合う | 既定案 |
| D8 | `--wasm-feature threads` の新しい `Channel` は `E2000`。threads では `DataChannel` を使う | 新しいランタイムは WebAssembly の Workers を持たない。WASM の tzroutine は JSPI（D-61 の決定 2） | 既定案 |
| D9 | 向きのある型（Phase 2）の名前は `SendOnly`・`ReceiveOnly` | `DataChannel.Sender`・`Receiver` と混ざらないように | 既定案 |
| D10 | `DataChannel` の実装（待ちの規則・ランタイム）は TZ03 では変えず、TZ12 で tzroutine のランタイムへ移す | 改名と意味の変更を同じ PR に混ぜない。TZ12 で `task.c` を消すときに移す | 既定案 |

## 実装と検証（2026-11-02）

- **範囲**: Phase 1（仕様 1〜9）と Phase 2（仕様 10。利用者の指示で、すべてのフェーズを実装した）を 1 つの PR で実装した（TZ02 の PR #80 のブランチの上）。手順 2 の改名は先に 1 つのコミット（`bbdb2c39`）で終え、名前以外の期待値を変えずに全テストを通した。
- **改名（手順 2）**: `std/DataChannel.tz`、`Builtin::DataChannel*`（名前 `DataChannel.*`、`is_data_channel`）、`RESERVED_MODULES` に `DataChannel`、fixture の `data_channel`・`data_channel_threads`、suite の `data_channel`、文書の `datachannel.md`、図の golden。`task.c` の文は変えていない。
- **新しい `Channel`**: `std/Channel.tz`（record `Channel<'a> { block: i64 }`、`Drop` は `Channel.__release`、`SendError`・`Received`）、`Builtin::Channel{Make, Send, Recv, TrySend, TryRecv, Close, Length, Capacity, Share, Release}`（`is_channel`、`channel_scheme`）、生成は新しい `src/llvm_channel.rs`（`go_channel_builtin`）。`for x in ch` は `control.rs` の `channel_loop`。要素の借用は `ownership.rs` の `sent_item` と受け取りの集合。効果・純粋・KG の分類と pin、権限なし、図の `CELL_EVIDENCE`・`CREATORS`。
- **既定の WebAssembly**: `tzroutine.c` の `TZR_SINGLE`（スケジューラーなし、静的な 1 つの G、空のロック、`tzr_park` は 1 を返す、`Routine` の問い合わせは 1）を、ドライバーと `tsuzuri test` の WebAssembly の実行器が `wasm-tzroutine-single` として連結する（import なし。`clang -DTZR_SINGLE -Wall -Wextra -Werror` を wasm32・wasm64 の `-O0`・`-O3` で確かめた）。spawn するプログラムの WebAssembly は TZ02 のとおり `E2000`、`--wasm-feature threads` と `Channel` は `E2000`。
- **`E1040`**: kind `renamed_to_data_channel`、`check::removed_feature`、`src/explain/codes/E1040.md`。値と型の旧の名前は名前の解決の失敗の後でだけ聞き（S6: 利用者の定義が勝つ。`union Channel = Sender | Receiver` と `record Sender` で確かめた）、置き換えの名前で検査を続ける。`DataChannel` の端を `Channel.send`・`recv` に渡した `E1003` も `E1040` にする。`tsuzuri fix` は、旧の名前に戻した `tests/fixtures/data_channel` を 1 回の実行（112 件の修正）で元の fixture と byte 単位で同じに戻す。
- **チケットから外れた判断**:
  1. **`Received` の閉じた case は `Disconnected`**: 1 つのモジュールの中で union の case の名前は一意で（`E1001`）、`SendError.Closed` と `Received.Closed` は両立しない。Rust の `TryRecvError::Disconnected` に合わせ、`Received<'a> = Item of 'a | Empty | Disconnected` にした（`SendError` は仕様のまま `Full | Closed`）。
  2. **`E1003` のヒントではなく `E1040`**: 仕様 8 の 2 つ目（G23 のヒント DB の `certain` の `fixes`）は、ヒント DB の設計（ヒントは発見的なので `certain` にしない、置き換えるのは診断の先頭の字句＝引数）と合わない。`DataChannel` の端の型が旧の API の証拠になるので、型を確かめたうえで `E1003` を `E1040`（`'Channel.send' of a DataChannel.Sender is now 'DataChannel.send'`、`certain`）に置き換える（`hints::renamed_channel_calls`）。
  3. **`tsuzuri fix` の検証**: 旧の名前を直すと、それを使う後ろの箇所が検査されて同じコードの診断が増え、G24 の「`(path, code)` ごとに増えたら `E2008`」で止まっていた（`MAX_ROUNDS` 5 では 10 ラウンド要る例もあった）。(a) 旧の名前を置き換えの名前で検査し続けて 1 回の解析で全部を見つけ、(b) 次のラウンドで適用できる修正案を持つ診断は増えても数えず、終わりに最初の解析と比べる、ように直した（`usage.md` に書いた）。編集で新しく名前が現れた opt-in の std は、ラウンドの後に足す（`require_named_std`）。
  4. **`Channel` は `DataChannel` も読み込む**: 置き換えの名前を解決するため、`DataChannel` の `OptIn` の `names` に `Channel` を足した（`uses` は std のファイルの参照と一致させる規則なので使わない）。
  5. **`Mutex.with_lock` の中の待ち**: 仕様にはないが、tzroutine を起動するか `Channel` を使うプログラム（`llvm::waits`）では、ロックの中の `send`・`recv`・`for` をトラップにした（ロックの持ち主は OS スレッド。TZ02 の印と文 5 を使う）。`try_*`・`close`・`length`・`capacity` は待たないので許す。
  6. **Task のワーカーと判定**: TZ01 の判定は、まだ呼び込んでいない OS スレッドを知らないので、`Task.parallel` の 2 つのタスクが `Channel` で受け渡すと、相手のタスクが始まる前に誤ってデッドロックと判定した。ネイティブで `tsuzuri_tz_` を使うプログラムでは、項目を渡す呼び出しと各ワーカーの関数が、項目を知らせる（`with_task_accounting`、`tzr_batch`）。最後の項目が終わった瞬間に待つスレッドを走っているものへ戻す（`routine_tasks` で見つけた隙間）。
  7. **`DataChannel` の待ちと tzroutine**: `DataChannel` の待ちは TZ12 まで `task.c` の判定のままで、tzroutine を数えない。tzroutine が送るのを `DataChannel.recv` で待つと誤って行き詰まりと判定しうるので、`both_kinds` は `DataChannel` が待たない形で両方を使い、`datachannel.md` に制限として書いた。
  8. **unroll の印**: `Channel` の操作を本体に持つループには `hint_loop` が印を付けない（回数が受信で決まり、LLVM が `loop not unrolled` を警告した）。`DataChannel` のループの同じ警告は今もある（生成コードを TZ12 まで変えない）。
  9. **Phase 2 は別名の関数**: 仕様 10 の高カインドの組み込みのクラス（`ChannelSend<'h>`）は、組み込みの型の式に型変数の適用がなく、入れると `Channel.send` の型の誤りが制約の誤りになって Phase 1 の診断（`expected Channel<i64>`）と `E1040` の置き換えが崩れる（停止条件）。停止条件の別案（別名の関数）を選び、`Channel.send_only`・`receive_only` が返す `SendOnly<'a>`・`ReceiveOnly<'a>`（std のモジュール `SendOnly`・`ReceiveOnly`。予約名 61 → 63）と、その操作 `SendOnly.send`・`try_send`・`close`・`length`・`capacity`・`share`、`ReceiveOnly.recv`・`try_recv`・`length`・`capacity`・`share` と `for` にした。型の名前は仕様の `Channel.SendOnly` でなく `SendOnly`（モジュールと同じ名前の record。`Channel<T>`・`Mutex<T>` と同じ）。生成と検査は `Builtin::channel_operation` で `Channel` と共有する。
- **レビュー（PR #81 の 1 回目）**: 4 件を確かめた。(1) `Mutex` を使うプログラムでは、`tz.mutex.parallel*` の呼び出しと、その中の `tsuzuri_task_parallel*` の両方で batch を告げていた（IR で確かめた）。項目は新しい方の batch しか減らさず、`npending` が残って、空の `Channel` を待つタスクがトラップせずに待ち続けた。ランタイムの呼び出しだけを告げる（`the_items_of_a_task_batch_are_announced_once`）。(2)(3) `--trap-mode return` の境界のトラップは、項目の `_leave`・batch の `_end`・`Mutex.with_lock` の印の 0 を飛ばし、`trap.c`（事前ビルドの照合で変えない）では戻せない。spawn するプログラムと同じく、`Channel` を使うプログラムの `--trap-mode return` を `E2000` にした（`trap_mode_return_refuses_a_channel`）。失敗の領域は、TZ02 の 3 回目の直し（ブロッキング呼び出しの深さ・スコープ・印を戻す）を `Channel` を使うプログラムに広げ、さらに、そのスレッドが告げた batch を外し、その項目でトラップしたワーカーを pool へ戻ったものとして数える（`tsuzuri_tz_parallel_mark`・`_reset`、`item_batch`。2 つの項目がトラップする例で、直す前は後の行き詰まりの判定が出ずに待ち続けた: `a_failure_domain_drops_the_batches_that_a_trap_left`）。(4) プールのスレッドがすべて `Channel` で待ち、残りの項目を始められないとき（CPU が 1 つで順に走るときも）、未着手の項目が判定を止めて待ち続ける。プールの待ち行列はランタイムから見えず、正しく数えるには `task.c`（事前ビルド）の中を知る必要があるので、TZ12（Task を tzroutine に置き換える）まで既知の制限とし、`channel.md` と `docs/architecture.md` に書いた（誤って行き詰まりと判定する側には倒さない）。
- **レビュー（PR #81 の 2 回目）**: 4 件を確かめて直した。(1) `Domain.run` の印が batch の並びの先頭の節だったので、それがほかのスレッドの batch で、`_reset` の前に終わると、印が並びから消え、成功した領域でも、呼んだスレッドの外側の batch（`Domain.run` を呼ぶ項目の batch）まで外した（古いランタイムに直接 `_begin`・`_mark`・`_end`・`_reset` を呼ぶ C で再現: 外側の batch が消え `npending` が 0）。batch に告げた順の番号を付け、印は最後の番号にした（`tests/tzroutine_runtime.c` の `batches`）。(2)(3) プールのワーカーが項目の中で入れ子の `Task.parallel` を出し、その項目がトラップすると、ワーカーの batch は `Domain.run` を呼んだスレッドの `_reset` から見えず（持ち主が違う）、入れ子の項目の `_leave` は外側の項目の `item_batch` を消していた。どちらも、後の行き詰まりの判定が出ずに待ち続けた（外側 8 項目・内側 2 項目の例で、直す前は 3 回とも、入れ子の項目のあとでトラップする例は 3 回に 1 回、時間切れ）。トラップを受けるところで、そのスレッドが告げた分を片付けるようにした: ランタイムへ渡すワーカーの関数を守り（`<関数>.guard`）に差し替え、`_enter` は項目の前のスレッドの状態と最後の番号を守りの frame に取り、`_leave` は状態を戻して、番号より後にこのスレッドが告げた batch を外す（`item_batch` はなくした）。`Domain.run` のあるプログラムでは、守りが `trap.c` の `tsuzuri_boundary_item` で項目を呼んでトラップを受け、ブロッキング呼び出しの深さ・生成のスコープ・`Mutex.with_lock` の印を戻し、`_leave` の後に `tsuzuri_boundary_resume` で投げ直す（`task.c` は前と同じトラップを受け取る。`an_item_that_traps_in_a_failure_domain_sets_its_thread_back`、`-O0`・`-O3`・`--trap-info` でも確かめた）。(4) `for x in ch` の脱糖が無修飾の `Channel.recv` を呼んでいたので、名前空間の中に利用者の `Channel.recv` があるとそちらに解決し（`E1003`）、`using` で 2 つの `Channel` が見えると正しいループが `E1004` になった（どちらも再現）。`std::Channel.recv`・`std::ReceiveOnly.recv` を呼ぶ（`a_for_loop_receives_with_the_standard_channel`）。1 回目の (2)(3) の 2 件は、`--trap-mode return` と `Channel` を `E2000` にしたこと（指摘の「組み合わせを拒む」案）で対応済み。
- **TZ02 の PR #80 の 2 回目のレビュー**の直し（ブロッキング呼び出しの P の受け渡し、`noinline` の印、`Domain.run` のスコープ）を取り込み、`with_task_accounting` は `with_blocking_calls` の内側に入る。
- **検証**: `cargo fmt`・`cargo clippy --all-targets -- -D warnings` 成功。`cargo nextest run --locked`（下記の件数）。新しいテスト: `tests/channel.rs` 10 件、`tests/removed_features.rs` 4 件、`fix::tests` 1 件。E2E（dev のバイナリ）: `features.mjs channel`（native、`-O0`／`-O3`、P 1・4、15 件と 3 つのトラップ、`live == 0`）、`channel_single`（native と既定の wasm32、7 件と 5 つのトラップ、import なし）、`data_channel`・`concurrency`・`tzroutine`、`tzroutine.mjs` 45 runs、`tzroutine_runtime.mjs`。適合性 `TSK-016`〜`019`（`conformance.mjs --area TSK` 19 件）、`check-spec --require-markers`、`gen-docs --check`、`check-cli-docs`、`check-guide-lite`、`repo_kb`、`check-runtime-includes.sh`。文書は変更した全ページを `check-docs` で確かめた（`datachannel.md` の 2 つの例の `loop not unrolled` は TZ02 の木でも出る既存の警告）。
- **文書**: 新規 `built-in-types-and-modules/channel.md`。`datachannel.md`（注記と制限）、`for-in.md`、`tzroutine.md`、`routine.md`、`compiler/diagnostics.md`・`usage.md`・`option.md`・`platforms.md`・`webassembly.md`・`conformance.md`（生成）、`organizing-tsuzuri/modules.md`（予約名 63 個）、`index.md`、`docs/language.md`（`### Channel`、予約名、`E1040`）、`docs/architecture.md`（`### Channel（TZ03）`）、`README.md`、`spec/rules/TSK.md`、`vsc/src/core.ts`。
- **新しく予約した std モジュール名**: `DataChannel`、`SendOnly`、`ReceiveOnly`（release notes に載せる）。
