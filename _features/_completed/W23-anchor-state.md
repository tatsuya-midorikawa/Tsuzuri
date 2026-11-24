# W23: コールバックから届くアプリの状態 `Anchor`

| 項目 | 内容 |
| --- | --- |
| ID | W23 |
| 種別 | 機能 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | [W12](W12-static-callback-lifetime.md)（静的コールバックの寿命の契約） |
| 後続 | [W26](W26-com-objects-in-tsuzuri.md)、[W27](W27-win32-package-from-winmd.md) |
| 状態 | done |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（再入の扱い、D-55）。D1・D2 は要承認、D3〜D10 は既定案。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) |
| 改善する劣位 | 保持されるコールバック（ウィンドウプロシージャ、タイマー、フック）から Tsuzuri の状態に届く方法がない。今の回避策は、`GWLP_USERDATA` などに整数だけを置くことで、Tsuzuri のヒープの値（`string`、`Vec`、record）を持てない |
| 手本にする既存実装 | 公開の組み込みと、record だけを持つ opt-in の std: `Mutex.create`・`Mutex.with_lock`（`src/check.rs` の `Builtin::MutexCreate`・`MutexWith`。`std/Mutex.tz` は `record Mutex<'a>` だけ）。不透明な record と `Drop` から呼ぶ私的な組み込み: `std/Timer.tz`・`std/Channel.tz`（`instance Drop` の `Timer.__free`・`Channel.__release`、[TZ05](_completed/TZ05-timers.md) の「着手時の再検討」2・3）。ランタイムへ確保の関数を渡す形: TZ05 の S1 案 A（生成コードが `@tz.free` を渡す）。借用を持つ結果の拒否: `src/ownership.rs` の `Checker::locked_result`（`Mutex.with_lock`・`OnceValue.get`、kind `mutex_result_retains_borrow`）。コールバックの下げ方: `Mutex.with_lock`（`src/llvm_sync.rs` の `MutexWith`）。G の固定: `src/runtime/tzroutine.c` の `tzr_lockm`・`tzr_unlockm`（`Routine.with_locked_thread`） |
| 主な影響ファイル | `std/Anchor.tz`（新規）・`src/stdlib.rs`（`SOURCES`・`OPT_IN`・`RESERVED_MODULES`・`opaque_record`）・`src/check.rs`（組み込み、`can_send`・`is_unsync_record`）・`src/ownership.rs`・`src/llvm.rs` か新しい `src/llvm_anchor.rs`・`src/runtime/tzroutine.c`（新しい節）・`src/runtime/tzroutine-jspi.h`・`src/driver.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/diagnostic_kinds.rs`・`tests/anchor.rs`（新規）・`tests/fixtures/anchor/Main.tz`（新規）・`tests/features.mjs`・`tests/tzroutine_runtime.c`・`tests/tzroutine.mjs`・`tests/windows.mjs`・`tests/std_argument_order.rs`・`tests/std_examples.rs`・文書 |

## 実装者向け早見

- 範囲: opt-in の std モジュール `Anchor`。`Anchor.create v` が値をランタイムの表に預けて所有者 `Anchor<T>` を返し、`Anchor.token` が整数の鍵（`i64`）を返す。鍵はホストの文脈の欄（`GWLP_USERDATA`、`lpCreateParams`、`dwRefData`、スレッドプールの文脈）に置け、コールバックは鍵から `Anchor.read`・`Anchor.update` で値に届く。鍵は世代付きで、偽の鍵・古い鍵・別の型・別のスレッド・再入はすべて `Error` になり、メモリを壊さない。
- 最初に読む: ① 仕様 1〜4 ② 「設計」の表の手順（create・read・update・release）③ 安全性の穴の表 ④ `Checker::locked_result`。
- 最初に書くテスト: `tests/tzroutine_runtime.c` の `anchor_tokens`（C だけで、作る・読む・解放した後の鍵が `Stale`・別の型の記述子が `WrongType`）。
- 触ってはいけない: `Anchor` を使わないプログラムの IR（opt-in にするのはこのため。TZ05 の再検討 1）。`Mutex.with_lock` の借用の検査の結果。W12 のデッドロックの判定の前提（別のスレッドから値に届かない）。
- 完了の見分け方: 下が native（C のホストが `on_click token` を 3 回呼ぶ。W12 の `tests/fixtures/callback_lifetime`）で 3 を返し、ホストのない同じ流れ（`features.mjs anchor` の `basic`）が native と既定の WebAssembly で 3 と `live == 0` になる。

```tsuzuri
record Counter { clicks: i64, name: string }
@trusted
extern "w_click_three_times" def click_three_times :: (i64 -> i64) -> i64 -> unit
def on_click :: i64 -> i64 = \token ->
    match Anchor.update (\c -> { c with clicks = c.clicks + 1 }) token with
    | Ok _ -> 0
    | Error _ -> 1
def main :: unit -> i32 = \() ->
    let state = Anchor.create (Counter { clicks: 0, name: "main" })
    let token = Anchor.token (ref state)
    click_three_times on_click token          // ホストが on_click token を 3 回呼ぶ
    match Anchor.read (\c -> c.clicks) token with
    | Ok n -> n as i32
    | Error _ -> 0 - 1
```

## 目的

Win32 のウィンドウプロシージャはメッセージごとに呼ばれ、アプリの状態（設定、文書、窓のハンドル）に届く必要がある。Tsuzuri にはモジュール共有の可変状態がなく（`docs/language.md` 446 行目）、Tsuzuri の値をホストへ渡す型もない。.NET の `GCHandle` と同じく、状態を表に預けて整数の鍵で引く方法を、型と世代と所有するスレッドと動的な借用の検査で安全にする。再入（`DestroyWindow` が `WM_DESTROY` を送る、モーダルのループが同じプロシージャを呼ぶ）は日常的に起きるので、トラップではなく `Busy` を返す。

## 着手条件と停止条件

### 着手条件

- W12 が done。D1・D2 が承認済み。
- `grep -n "fn locked_result" src/ownership.rs` と `grep -n "fn is_unsync_record\|fn opaque_record" src/check.rs src/stdlib.rs` が「現状」と同じ形であること。

### 停止条件

- **S1** 表の記憶を、生成コードが渡す確保の関数（TZ05 の S1 案 A）で取ると、`live == 0` の検査か、`Anchor` を使わない tzroutine のプログラムの連結が壊れる。
- **S2** 所有者を持つ G をその M に固定する（D4）と、既存の suite（`tzroutine.mjs`、`features.mjs` の `tzroutine`・`channel`・`sync_types`）かデッドロックの判定が変わる。
- **S3** `Anchor.read` の結果の借用の検査を足すために、`Mutex.with_lock`・`OnceValue.get` の検査の結果が変わる。
- **S4** `Anchor` を使わないプログラムの IR・header・診断が 1 byte でも変わる（`src/runtime/tzroutine.c` の文字列が変わるのは想定内）。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- コールバックは捕捉を持てない: `src/check.rs` の `validate_callbacks` が実引数に捕捉のないトップレベルの関数を求める。開発版で確認: `register (\x -> x + base)` は `E1008`（`extern_callback_argument`）。
- 大域の状態はない: `docs/language.md` 446 行目「モジュール共有の可変状態は言語レベルで徹底して排除されています」。`const total: Atomic<i64> = Atomic.create 0` は `E1026`（開発版で確認）。`Atomic<T>` は export と extern の境界で `E1008`（`atomic.md`）。`extern type` のハンドルは Tsuzuri では作れない（E12 D3）。したがって Tsuzuri の値をホストの文脈の欄へ渡す方法がない。
- 再入: 同じ `Mutex` を `Mutex.with_lock` の中でもう一度取るとトラップ（`Tsuzuri runtime: Mutex.with_lock cannot be nested`。`mutex.md`、`src/runtime/tzroutine.c`）。ウィンドウプロシージャの再入に `Mutex` を使うとトラップする。
- 借用を持つ結果: `src/ownership.rs` の `Checker::locked_result` は `MutexWith`・`RwMutexWithRead`・`RwMutexWithWrite`・`OnceValueGet`（とコールバックの位置 3 の `CondWaitWith`）の結果が借用した環境を持たないと証明できなければ `E1013`（`mutex_result_retains_borrow`）にする。
- Send と Sync: `Type::can_send` は record をフィールドで判定するので、`record Anchor<'a> { token: i64 }` はそのままでは Send になる。`is_unsync_record` が std の名前（`Seq.Seq`・`Gpu.Device`・`Gpu.Buffer`・`Owned.Function`）で Sync を外す前例があり、Send を名前で外す前例はない。
- opt-in: `src/stdlib.rs` の `OPT_IN`（`Timer` など）。TZ05 は、常に読み込む std に関数を足すとすべてのプログラムの内部の名前の番号がずれると確かめた。`stdlib::opaque_record` が不透明な std の record の一覧（構築・分解は `E1022`）。
- ランタイム: `src/runtime/tzroutine.c` は IR が `tsuzuri_tz_` の記号を名指すときだけ連結され（`src/driver.rs` の `tzroutine_symbols`）、既定の WebAssembly では 1 本のスレッドの `TZR_SINGLE` の形で連結される（libc・TLS を使わない）。JSPI の形は `tsuzuri_alloc` を `malloc` にする。生成した関数は TZ10 の横取りの対象で、ランタイムの関数は対象外（`tzroutine.md` の「横取り」）。
- 名前: `record Anchor { x: i64 }` は今 `check` を通る（開発版で確認）。

## 仕様

### 1. API（opt-in のモジュール `Anchor`）

`create`・`read`・`update` は `Mutex.create`・`Mutex.with_lock` と同じく公開の組み込み（借用の検査が利用者の呼び出し地点で `builtin_called` に当たるため）。`std/Anchor.tz` は record・`Drop`・`Error`・`token` だけを持つ。

| 名前 | 型 | 意味 |
| --- | --- | --- |
| `Anchor<'a>` | 不透明な record（`std/Anchor.tz`。構築・分解は `E1022`） | 所有者。Copy でない。drop で値を解放する（借用中なら遅らせる）。Send でも Sync でもない。関数値に捕捉できない（利用者の `Drop` を持つので `E1005`） |
| `Anchor.Error` | `union Error = Stale \| WrongType \| WrongThread \| Busy` | `Stale`: 鍵が生きている預け先を指さない（0、偽、所有者が drop された、表が作り直された）。`WrongType`: 預けた型と読む型が違う。`WrongThread`: 預けたスレッドと違うスレッド。`Busy`: 読み書きの最中の再入 |
| `Anchor.create` | `own 'a -> Anchor<'a>` | 値を預ける。値は借用した環境を持たないと証明できなければならない（`E1013`、kind 案 `anchor_value_retains_borrow`） |
| `Anchor.token` | `ref Anchor<'a> -> i64` | 鍵。所有者の寿命の間変わらない。0 にはならない |
| `Anchor.read` | `(ref 'a -> 'b) -> i64 -> Result<'b, Anchor.Error>` | 共有借用で読む。読みの入れ子は可。結果 `'b` は借用した環境を持たないと証明できなければならない（`E1013`、kind 案 `anchor_result_retains_borrow`） |
| `Anchor.update` | `('a -> 'a) -> i64 -> Result<unit, Anchor.Error>` | 値を取り出して関数に渡し、返った値を戻す。返す値にも `create` と同じ検査 |

- 引数の順序（D-59 を改めない）: 鍵が最後（`Anchor.read f token`）。`tests/std_argument_order.rs` の `MODULES` に `("Anchor", subject(&["Anchor"]))`。このテストが見るのは `std/Anchor.tz` の `def` の行（`token` だけ）で、組み込みの型（`create`・`read`・`update`）は `Array`・`List`・`Vec`・`String`・`Utf8String` のほかは検査しない。鍵を最後にする順序は `Builtin::signature` で固定し、`anchor.md` の「引数の順序」の節と `tests/anchor.rs` の型のテストで確かめる。
- 所有者から値を直接借りる関数は作らない（外側の読みと内側の書きを、すべて表の借用の数で見つけるため）。
- 組み込み（`create`・`read`・`update` と、`Drop` が呼ぶ私的な `Anchor.__release`）は純粋でない（表は大域の状態）。`pure def`・契約の条件からは呼べず、`Domain.run` の本体（純粋でなければならない、`E1038`）にも書けない。権限は要らない。

### 2. 実行の意味

- **型**: 表は預けた型の記述子を持ち、`read`・`update` の型（関数の型から決まる）と違えば `WrongType`。
- **スレッド**: 表は預けた OS スレッドを記録し、別のスレッドからの `read`・`update` は `WrongThread`。tzroutine の中で作ったときは、所有者が生きている間その G を今の M に固定する（D4）ので、所有者の G が M を移って誤って `WrongThread` になることはない。main スレッドのウィンドウプロシージャ（`main` の擬似 G）と、所有者の G が呼んだ `extern` の中のコールバック（W12 のとおり、その G がその M で走る）は同じスレッドである。
- **借用**: `read` の最中の `read` は可。`read` の最中の `update`、`update` の最中の `read`・`update` は `Busy`（関数を呼ばずに返る。トラップしない）。別の預け先の読み書きは入れ子にできる。
- **確かめる順序**: `Stale` → `WrongThread` → `WrongType` → `Busy`。
- **所有者の drop**: 世代を先に進め（以後その鍵は `Stale`）、借用中でなければ値を drop して預け先を空ける。値の `Drop` の中から同じ鍵を使うと `Stale`。借用中なら、最後の借用が終わったときに解放する。
- **`update` の戻し**: 関数が返った時点で所有者が drop されていれば（再入した処理が drop した）、返った値を drop して `Error Stale` を返す。
- **上限**: 同時に生きている預け先は 16,777,215 個まで（超えると `create` が `Tsuzuri runtime: too many live anchors` でトラップ。`TrapKind` は増やさない）。
- **target**: native と既定の WebAssembly・`--wasm-feature jspi`（1 本のスレッド。`WrongThread` は起きない）。`--wasm-feature threads` との組み合わせは `E2000`（`option_conflict`）。`--trap-mode return` は `E2000`（トラップが借用の数と表を残す）。`--freestanding` は `E2000`（文に `Anchor` と出す）。Windows の `--emit object` は、ランタイムを持つ COFF の object の既存の `E2002`。

### 3. ホストの規律（文書に書く）

`read`・`update` の関数の中で、同じ窓へのメッセージを回す `extern`（`MessageBoxW`・`DialogBoxParamW`・`DestroyWindow`・自分の窓への `SendMessageW`・`TrackPopupMenu` など）を呼ぶと、再入したプロシージャの `update` は `Busy` になる。状態から要るものを取り出して借用を終えてから、その `extern` を呼び、戻ったら `update` し直す。`Busy` を受けたプロシージャは既定の処理（`DefWindowProcW`）に回すか、`PostMessageW` で後に回す。

### 4. 鍵の形

鍵は `i64`: ビット 0〜23 が預け先の番号 + 1、24〜47 が世代（1 から）、48〜62 が表の版（1 から）、63 は 0。ホストは不透明な整数として扱う（ポインター幅の欄に入る。wasm32 の JS ホストでは BigInt）。

## 設計

### 安全性の穴と塞ぎ方

| 穴 | 塞ぎ方 | 確かめるテスト |
| --- | --- | --- |
| (a) 再入した処理が所有者を drop し、実行中の `read`・`update` が解放済みの値を指す | 所有者の drop は世代を先に進め、借用中なら解放を最後の借用の終わりまで遅らせる。値の箱は表の外に別に確保し、表が育っても動かない | `nested_drop_during_read`: 親の値が子の所有者を持ち、子の `read` の関数の中で親を `update` して子の所有者を drop する。子の関数はその後も値を読める、`Drop` は借用の終わりに 1 回 |
| (b) 値の `Drop` の最中の再入（RAII の包みが `DestroyWindow` を呼び、`WM_DESTROY` が来る） | 世代を進めてから値の `Drop` を呼ぶ。`Drop` の間、預け先は空き一覧に戻さない | `drop_reenters`: 値の `Drop` が自分の鍵で `read` すると `Error Stale` |
| (c) `update` が値を戻すとき、預け先が再利用されていて別の型の値を上書きする | 借用中の預け先は再利用しない。戻す前に世代を確かめ、所有者が drop されていれば返った値を drop して `Stale` | `update_after_owner_drop`: 子の `update` の関数の中で親を通して子の所有者を drop する。`Error Stale`、各値の `Drop` が 1 回 |
| (d) `read` の最中の `update` | 書きは読みの数が 0 のときだけ | `busy_matrix`: 4 通りの入れ子 |
| 型の取り違え（正しい鍵を別の型で読む） | 型の記述子の番地を比べる | `wrong_type` |
| `read` の結果が値の借用を持ち出す | `Checker::locked_result` に `Anchor.read`・`update`（関数の位置 0）を足す。`create` の値と `update` が返す値は `OnceValue.get` の初期化子と同じ「保持される値」の検査 | `tests/anchor.rs` の `E1013` |
| 偽の鍵・別のスレッドの鍵の照会が、表の成長と競合する | すべての操作を表のロック（`tzr_lock`）の下で行う。預け先の塊は動かさず、塊の一覧だけを伸ばす | `anchor_threads`（TSan） |
| 世代の折り返しによる古い鍵の一致 | 世代が上限に届いた預け先は引退させ、再利用しない。表を作り直すときは版を進める（版が上限なら作り直さない） | `anchor_generation_retire`（テスト用の小さい世代の幅で） |
| 所有者を別の tzroutine へ移して解放する | `Anchor<T>` は Send でない（`can_send` に名前の例外）。Sync でもない（`is_unsync_record`） | `tests/anchor.rs` の `E1013`（Send） |

### ランタイム（`src/runtime/tzroutine.c` の新しい節「anchors」）

```c
typedef struct tzr_anchor_slot {
    uint32_t generation;        /* 1.. ; a retired slot is never reused */
    uint32_t state;             /* FREE, LIVE, DYING (owner dropped while borrowed or during its Drop) */
    uint64_t owner;             /* the creating thread's anchor id */
    const void *type;           /* the descriptor of T */
    void *box;                  /* the value, allocated by the generated code */
    void (*drop)(void *);       /* drops the value in the box and frees the box */
    uint32_t readers;
    uint32_t writer;
    uint32_t next_free;
} tzr_anchor_slot;
```

| 関数（`TZR_API`） | 意味 |
| --- | --- |
| `int64_t tsuzuri_tz_anchor_create(void *box, const void *type, void (*drop)(void *), void *(*alloc)(int64_t), void (*release)(void *))` | 空きか新しい預け先に `LIVE` で入れて鍵を返す。表の塊は `alloc`・`release`（生成コードの `@tz.alloc`・`@tz.free`。TZ05 の S1 案 A）で取る。tzroutine の G なら `tzr_lockm` |
| `int32_t tsuzuri_tz_anchor_begin(int64_t token, const void *type, int32_t exclusive, void **box)` | 0 = 成功（`readers++` か `writer = 1`）、1 = `Stale`、2 = `WrongType`、3 = `WrongThread`、4 = `Busy` |
| `int32_t tsuzuri_tz_anchor_end(int64_t token, int32_t exclusive)` | 借用を終える。共有で `DYING` かつ最後なら `drop(box)` を呼んで空ける（0 を返す）。排他で `DYING` なら箱を切り離して空け（値は取り出し済み）、0 を返す。生きていれば 1 |
| `void tsuzuri_tz_anchor_release(int64_t token)` | 所有者の drop。世代を進め、借用中なら `DYING` にして戻る。そうでなければ `DYING` のまま `drop(box)` を呼び、終わってから空き一覧へ。tzroutine の G なら `tzr_unlockm`。生きている預け先が 0 になれば塊をすべて `release` し、版を進める |

- ロックの下で `drop` や生成コードを呼ばない（値の `Drop` は再入する）。
- スレッドの識別: スレッドごとの `_Thread_local uint64_t` を、初めて使うときに大域の原子的な数から振る（OS のスレッド ID と違い、スレッドが終わっても再利用しない）。`TZR_SINGLE` と JSPI では定数。
- `TZR_SINGLE` はロックも TLS も持たず、記憶は渡された `alloc`・`release` だけを使う（libc を使わない約束を保つ）。
- 生成した関数は横取りされうるので、表をランタイムの外（IR の手書きの `.ll`）に置かない。

### コンパイラ

| ファイル | 変更 |
| --- | --- |
| `std/Anchor.tz`（新規） | `record Anchor<'a> { token: i64 }`、`instance Drop<Anchor<'a>>`（`Anchor.__release`）、`union Error` と `instance Display<Error>`（`std/Json.tz` の `instance Display<Error>` と同じ形）、`def token :: ref Anchor<'a> -> i64`（フィールドを読む）。`create`・`read`・`update` の `def` は置かない（std の `def` で包むと、借用の検査が std の中の型変数の引数に一度だけ走り、利用者の呼び出し地点では走らない）。`///` と G34 の例は `token` と、組み込みの説明の頁（`anchor.md`）に |
| `src/stdlib.rs` | `SOURCES`、`OPT_IN`（`module: "Anchor"`。名前 `Anchor` を書いたプログラムだけが読み込み、組み込み `Anchor.create` などの型が `Anchor<'a>`・`Anchor.Error` を名指せるようにする）、`RESERVED_MODULES` に `Anchor`、`opaque_record` に `Anchor.Anchor` |
| `src/check.rs` | 公開の組み込み `AnchorCreate`（`Anchor.create`）・`AnchorRead`（`Anchor.read`）・`AnchorUpdate`（`Anchor.update`）と私的な `AnchorRelease`（`Anchor.__release`。std 専用、`E1022`）。`Builtin::ALL`・`name`・`signature`（鍵が最後）・生成・効果の表。組み込みの型が std の型を名指す形は `Mutex.create`（`Mutex<'a>`）にそろえる。`can_send` と `is_unsync_record` に `Anchor.Anchor` |
| `src/ownership.rs` | `Checker::locked_result` に `AnchorRead`・`AnchorUpdate`、保持される値の検査に `AnchorCreate` と `AnchorUpdate` の結果 |
| 生成（`src/llvm.rs` か `src/llvm_anchor.rs`） | 型ごとに記述子 `@tz.anchor.type.<型の決定的な名前> = internal constant i8 0`（`unnamed_addr` を付けない: `src/llvm_split.rs` は番地に意味のない定数を単位ごとに複製する）と `@tz.anchor.drop.<型>`（箱の値を drop して `@tz.free`）。`create`: `@tz.alloc(max(size, 1))` に値を入れて `create` を呼ぶ。`read`: `begin(…, 0, …)` → 成功なら `Mutex.with_lock` と同じ渡し方で `ref 'a` を関数に渡す → `end` → `Ok`、失敗なら状態から `Error` の場合を作る。`update`: `begin(…, 1, …)` → 箱から値を取り出す → 関数 → `end` が 1 なら箱へ書いて `Ok ()`、0 なら返った値を drop して `@tz.free(box)` → `Error Stale` |
| `src/driver.rs` | `--trap-mode return`・`--wasm-feature threads` の `E2000`（`llvm::uses_tzroutine` と同じ形の `uses_anchor`）、`--freestanding` の印の一覧に `@tsuzuri_tz_anchor_`（`@tsuzuri_tz_` より前） |
| 効果・純粋・KG | `Mutex.create`・`Mutex.with_lock` と同じ分類（純粋でない、`alloc`） |

### 採らなかった案

| 案 | 採らない理由 |
| --- | --- |
| 所有者を G（tzroutine）で記録する | tzroutine のランタイムのないプログラム（tzroutine を起動しない）では G がない。OS スレッドの記録と G の固定の組み合わせなら、どのプログラムでも同じ意味になる |
| OS スレッドで記録し、G を固定しない | 固定しない tzroutine は M を移るので、所有者の G が同じ値を読んでも時々 `WrongThread` になる（非決定的） |
| `Arc<Mutex<T>>` を鍵にする（どのスレッドからも引ける） | UI の状態は `!Send` のハンドル（W19）を持つ。`Mutex` の再入はトラップ。W12 の判定の前提（別のスレッドの値に届かない）も崩れる |
| 再入をトラップにする | `DestroyWindow`・モーダルのループの再入は正常な Win32 の動きで、トラップはアプリを落とす |
| 表を IR の `.ll` に置く | 生成した関数は TZ10 で横取りされ、ロックを持ったまま止まりうる。TLS とロックはランタイムにすでにある |
| `calloc` で表を取る | `TZR_SINGLE` は libc を使わない。生成コードの確保の関数を渡せば、どの形でも同じ記憶の扱いになる |
| `update` を `(ref mut 'a -> 'b)` にする | [共通の綴り](README.md#共通の綴り起票時に決めた契約)を変えない。取り出しと戻しの手順で穴 (c) を塞げる |

## 前提とする他チケットのインターフェース

- W12: 静的コールバックの契約、デッドロックの判定の前提（入口の外のスレッドは Tsuzuri の値に届かない）。この機能は別のスレッドからの読み書きを `WrongThread` にして前提を保つ。

## 他チケットへ提供するインターフェース

- `Anchor.create`・`token`・`read`・`update` と `Anchor.Error`（W26 の COM オブジェクトの状態、W27 の生成器が窓の状態の例に使う）。
- ランタイムの `tsuzuri_tz_anchor_*`（W26 が vtable の各メソッドから鍵で状態を引く）。
- W19 が先に入っていれば、`Anchor.read`・`Anchor.update` を W19 の `pinned_rules::builtin_position` に `Inline`（呼んだ G の上で戻る前に関数を呼び、保持しない）で分類する。
- 規則: 別のスレッドの `Anchor` へ届く経路を足す機能は、W12 のデッドロックの判定の規則を見直す。

## 実装手順

1. **変更**: なし（ブランチ `feat/w23-anchor`）。**内容**: `node scripts/repo-kb.mjs brief W23`、着手条件。`tests/fixtures/*`・`examples/*` の `--emit llvm`（native と wasm32）を保存。`Builtin::ALL` の長さ、`RESERVED_MODULES` の数、効果の表の件数を控える（GUIDE-LITE §11）。**確認**: `grep -rnw "Anchor" tests examples std` が 0 件。
2. **変更**: `src/runtime/tzroutine.c`（threaded・`TZR_SINGLE`）、`tzroutine-jspi.h`、`tests/tzroutine_runtime.c`。**内容**: 表と 4 つの関数。C のケース `anchor_tokens`・`anchor_nested`・`anchor_dying`・`anchor_reset_epoch`・`anchor_generation_retire`・`anchor_threads`（8 スレッドが作り、ほかのスレッドの鍵で `WrongThread`）・`anchor_pin`（tzroutine の G が作ると固定され、解放で外れる）。**確認**: `TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`。
3. **変更**: `std/Anchor.tz`、`src/stdlib.rs`、`src/check.rs`、`src/ownership.rs`、効果の表。**内容**: 型・公開の組み込み 3 つと私的な `Anchor.__release`・検査（`Mutex.with_lock` と同じく、組み込みの呼び出し地点で `Checker::locked_result` と保持される値の検査が走ること）。**確認**: `cargo test --locked --test anchor`（型と拒否）、`cargo test --locked --test std_argument_order`、`honors_the_exact_specialization_limit`、`cargo nextest run --locked`。
4. **変更**: 生成と `src/driver.rs`。**確認**: `tests/anchor.rs` の IR のテスト、手順 1 の IR と `cmp` で一致（`Anchor` を使わないもの）。
5. **変更**: `tests/fixtures/anchor/Main.tz`、`tests/features.mjs`（`anchor`）。**確認**: `node tests/features.mjs target/debug/tsuzuri anchor`（native と既定の WebAssembly、`-O0`・`-O3`、`live == 0`）。
6. **変更**: `tests/tzroutine.mjs`（スレッドのケース）、W12 の `tests/fixtures/callback_lifetime`（ホストのコールバックから鍵で読む）、`tests/windows.mjs`（W06 の段）。**確認**: 各 suite。PR に `ci:full`。
7. **変更**: 文書と規則 ID。**確認**: T2、`check-docs`、`check-cli-docs`、`gen-docs --check`。

## テスト計画

| 置き場 | ケース | 期待 |
| --- | --- | --- |
| `tests/anchor.rs`（cargo） | 型: 4 つの関数の型（組み込みの `read`・`update` は鍵が最後）、`Anchor<T>` の構築・分解（`E1022`）、`Anchor.__release` の利用者からの呼び出し（`E1022`）、所有者の関数値への捕捉（`E1005`）、`tzroutine { }` への所有者の移動（`E1013` の Send）、`read` が `ref` か借用を捕捉した関数を返す（`E1013`/`anchor_result_retains_borrow`）、借用した環境を持つ値の `create` と `update` の戻り値（`anchor_value_retains_borrow`。利用者の呼び出し地点で出ること）、`pure def` からの `read`（純粋の検査の既存のコード）、`record Anchor` を持つ利用者のモジュール（予約名の既存のコード） | 各コード |
| 同上（CLI） | `--trap-mode return`・`--wasm-feature threads`・`--freestanding` | `E2000`（文に `Anchor`） |
| 同上 | `Anchor` を使わない fixture の IR | 手順 1 と一致。`Anchor` を使う IR は決定的 |
| `tests/fixtures/anchor` + `features.mjs anchor`（native と既定の WebAssembly、`-O0`・`-O3`、`live == 0`） | `basic`（早見の例のホストなし版）、`nested_reads`、`busy_matrix`、`nested_drop_during_read`、`drop_reenters`、`update_after_owner_drop`、`forged`（0・乱数・drop 後・表の作り直し後の鍵が `Stale`）、`wrong_type`、`many`（10,000 個を同時に持ってから drop） | 手計算の期待値。`Drop` の回数を `Atomic` で数える |
| `tests/tzroutine.mjs`（ci の suites） | `wrong_thread`（鍵を `Channel` で別の tzroutine に送る）、`pinned_owner`（P 4 で、所有者を作った tzroutine が 1,000 回 `Routine.yield` しながら読む） | `WrongThread`、すべて `Ok` |
| W12 の `tests/fixtures/callback_lifetime`（POSIX） | ホストが同じスレッドの `extern` の中で呼ぶコールバックが鍵で `update`、別のスレッドから呼ぶと `WrongThread` | 参照値 |
| `tests/windows.mjs`（windows.yml W5。W06 の段） | W06 の shim（`tests/windows_gui_shim.c`）と W12 が足した `tz_gui_register_class` で窓を作り、`SetWindowLongPtrW(GWLP_USERDATA)` に鍵を置き、プロシージャが `update`。値の中の RAII の包みの `Drop` が `DestroyWindow` を呼び、再入したプロシージャが `Stale` を受ける。`update` の中の自分の窓への `SendMessageW` で再入したプロシージャが `Busy` を受ける | 参照どおりの順序と結果 |

## ドキュメント

- 新規 `built-in-types-and-modules/anchor.md`（API、鍵、`Error` の 4 つ、再入と仕様 3 の規律、スレッドと G の固定、上限、target、「引数の順序」の節）。`index.md`、`argument-order.md`。
- `native-interop.md` の「静的コールバック」節の後に「コールバックからアプリの状態に届く」（`GWLP_USERDATA` の例）。
- `docs/language.md`: 446 行目に「`Anchor` は、預けたスレッドからだけ、動的な借用の検査のもとで整数の鍵で届く状態」を添える。「コールバック」節の「グローバルな可変状態を保持しないため」の理由に、`Anchor` の再入は `Busy` で扱うと足す。予約名と診断の表。
- `routine.md`（`Routine.with_locked_thread` の節に、`Anchor` を持つ G も固定されると書く）、`organizing-tsuzuri/modules.md`（予約名の数）、`compiler/diagnostics.md`（kind の表は `<!-- generated:diagnostic-kinds -->` の生成の部分なので手で書かず、`src/diagnostic_kinds.rs` に登録してから `node scripts/gen-docs.mjs`）。
- `docs/architecture.md`（表、鍵、固定、記憶）。`spec/rules/STD.md` か `TSK.md` に着手時の番号の次で「`Anchor.read` from a thread other than the creating one MUST return `WrongThread`.」「A reentrant `Anchor.update` MUST return `Busy` without calling its function.」「A token MUST NOT reach a value after its owner was dropped.」。release notes に予約名 `Anchor`。

## 受け入れ条件

- [ ] 仕様 1〜4 と安全性の穴の表のとおりに動き、「テスト計画」が通る（native と既定の WebAssembly、`-O0`・`-O3`、P 1・4、`live == 0`）。
- [ ] ランタイムの C のテストが TSan・ASan で通る。
- [ ] `Anchor` を使わないプログラムの IR・header・診断が手順 1 と byte 単位で一致する。
- [ ] `tests/std_argument_order.rs` に `("Anchor", subject(&["Anchor"]))` の行があり通る。組み込みの `Anchor.read`・`update` の鍵が最後であることを `tests/anchor.rs` が型で確かめる。std の例（`tsuzuri test --doc std`）が通る。
- [ ] 文書・規則 ID・診断の表を更新し、`check-docs`・`check-cli-docs`・`gen-docs --check` が通る。
- [ ] PR に `ci:full` を付け、`windows.yml` が緑。

## 落とし穴

- **ロックの下で `drop` を呼ばない**: 値の `Drop` は別の `Anchor` を作り・解放し、ウィンドウプロシージャへ再入する。`DYING` にしてからロックを外して呼ぶ。
- **取り出した値は箱にない**: 排他の借用中に所有者が drop されたとき、ランタイムが `drop(box)` を呼ぶと、取り出し済みの値を二重に drop する。排他の `end` は箱を切り離すだけで、値は生成コードが drop する。
- **固定の数**: `tzr_lockm` は数を持つ。`create` と `release` は 1 対 1。コールバックの `callback_enter` の固定と重なってもよい。
- **表の作り直しと版**: 作り直しの後に古い鍵が新しい預け先と一致しないよう、版を鍵に入れる。版が上限に届いたら作り直さない（`live` は残るが、そこまで作り直すプログラムはテストにない）。
- **記述子の同一性**: `internal unnamed_addr` にすると codegen units（`TSUZURI_CODEGEN_UNITS=split`）で単位ごとに別の番地になり、同じ型が `WrongType` になる。`codegen_units.mjs` でも `anchor` の suite を通す。
- **opt-in**: `Anchor` を常に読み込む std に入れない。名前を書いたプログラムだけが読み込む。

## 対象外

- 別のスレッドから引ける預け先（`Send + Sync` の値の共有の鍵）。W12 の判定の規則の見直しが要る。
- 所有者を消費して値を取り戻す `Anchor.take`、値への直接の借用。
- 窓の文脈の thunk（`WM_NCCREATE` の `lpCreateParams` を読むランタイムの C）。`CREATESTRUCTW` を読むのは W24 の `extern view` の範囲。
- 性能の主張（表のロックの費用は計測してから。W29 と同じ手順）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | opt-in の std モジュール `Anchor`（`create`・`token`・`read`・`update`・`Error`）を足し、モジュール名を予約する。鍵は最後の引数 | D-55 の名前の割り当て。D-59 は改めない（鍵が対象） | 要承認（承認前は手順 3 に着手しない） |
| D2 | 読み書きの最中の再入は `Busy` を返し、関数を呼ばない（トラップしない）。`docs/language.md` 446 行目と「コールバック」節の理由の文を、`Anchor` の状態について改める | ウィンドウプロシージャの再入は正常な動きで、`Mutex` のようなトラップはアプリを落とす。言語の「共有の可変状態がない」という説明に例外が入るので明記する | 要承認（承認前は手順 3 に着手しない） |
| D3 | 鍵は世代と表の版を持つ `i64`。0 は無効。世代が上限の預け先は引退させる | 偽の鍵・古い鍵が生きている別の値に届かない。整数なのでホストのどの文脈の欄にも置ける | 既定案（実装者はこの案に従う） |
| D4 | 所有するスレッドを OS スレッドで記録し、tzroutine の中で作ったら所有者が生きている間その G を M に固定する | どのプログラムでも同じ意味で、M:N の移動による誤った `WrongThread` を防ぐ。UI の状態はもともとスレッドに結び付く | 既定案（実装者はこの案に従う） |
| D5 | 所有者の drop は世代を先に進め、借用中なら解放を遅らせる。`update` は戻す前に世代を確かめる | 穴 (a)〜(c) を塞ぐ | 既定案（実装者はこの案に従う） |
| D6 | 型の照合は、型ごとの記述子の番地で行う | 鍵だけでは型が分からない。番地は同じプログラムの中で一意（`unnamed_addr` なし） | 既定案（実装者はこの案に従う） |
| D7 | 表は `src/runtime/tzroutine.c` の節に置き、記憶は生成コードが渡す確保の関数で取る（TZ05 の S1 案 A）。生きている預け先が 0 になれば表を解放する | 使うプログラムだけに連結され（`tsuzuri_tz_` の記号）、ロック・TLS・`TZR_SINGLE` がそろっている。`live == 0` を保つ | 既定案（実装者はこの案に従う） |
| D8 | `Anchor<T>` は Send でも Sync でもない。`T` に Send を求めない | 値は預けたスレッドからだけ触れるので、`Rc` や W19 の `local` のハンドルを持てる | 既定案（実装者はこの案に従う） |
| D9 | `read` の結果、`create` の値、`update` が返す値は、借用した環境を持たないと証明できなければならない | 表は呼び出しより長く値を持ち、`read` の結果は借用の後に使われる（`Mutex.with_lock`・`OnceValue.get` と同じ理由） | 既定案（実装者はこの案に従う） |
| D10 | `--trap-mode return`・`--wasm-feature threads`・`--freestanding` は `E2000`。`Domain.run` の本体は純粋の規則で外れる | トラップが借用の数と表を残す。WebAssembly のスレッドの形の表はない。native のランタイム（`tzroutine.c` の threaded の形）は C ライブラリを要る（`--freestanding` の既存の拒否と同じ理由で、文だけ `Anchor` と出す） | 既定案（実装者はこの案に従う） |
