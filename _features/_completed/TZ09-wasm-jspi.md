# TZ09: WebAssembly（JSPI）の tzroutine

| 項目 | 内容 |
| --- | --- |
| ID | TZ09 |
| 優先度 | P2 |
| 規模 | L（`--wasm-feature jspi` の tzroutine）。既定の WASM の 1 本の実行（tzroutine を生成しないプログラムの `Channel`・同期の型）は TZ03 が入れた |
| 依存 | TZ04, TZ05, TZ06 |
| 後続 | TZ13（Async の JSPI の経路を消すときの残り） |
| 状態 | done（2026-11-04） |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61 の決定 2: tzroutine は `--wasm-feature jspi` を必須にする）。決定事項 D1〜D7 は既定案 |
| 手本にする既存実装 | `--wasm-feature jspi`（`src/driver.rs` 577〜593 行の検査、`BuildOptions.wasm_jspi`）、`src/bindings.rs` の `JsFlavor::Jspi` と `src/runtime/bindings-core.mjs` の `asyncImports`（631〜655 行。`WebAssembly.Suspending`）と export の包み（762〜791 行。`WebAssembly.promising`、呼び出しの直列化）、`src/runtime/task-wasm-threads.c` の WASM 向けの C の組み立て（`--target=wasm32-unknown-unknown -std=c11 -ffreestanding -fno-stack-protector`、`driver.rs` 4935〜4970 行）、`src/llvm.rs` の `with_stack_checks`（300 行。関数ごとのスタックの検査）、TZ03 の `TZR_SINGLE`（`tzroutine.c` を WASM 向けに組む分岐） |
| 主な影響ファイル | `src/runtime/tzroutine.c`（`TZR_JSPI` の分岐）、`src/runtime/tzroutine-jspi.ll`（新規。スタックの保存と復元）、`src/driver.rs`・`src/llvm.rs`・`src/bindings.rs`・`src/runtime/bindings-core.mjs`・`src/runtime/bindings.mjs`・`src/test_runner.rs`・`.gitignore`・`scripts/check-runtime-includes.sh`・`tests/tzroutine_wasm.mjs`（新規）・`tests/features.mjs`・`tests/tzroutine.mjs`・文書 |

## 実装者向け早見

- 範囲: `--wasm-feature jspi` で tzroutine を動かす（tzroutine を生成しないプログラムの既定の WASM の 1 本の実行は TZ03 で済んでいる）: tzroutine ごとに JSPI のスタック（エンジンが持つ wasm のスタック）と線形メモリのスタック（shadow stack）を持ち、JavaScript の 1 本のスレッドの上で交代に走らせる（M:1）。
- 最初に読む: ① 仕様の 3〜5 ② `src/runtime/bindings-core.mjs` 600〜900 行 ③ TZ01 の付録 D-1（park／ready）④ `src/llvm.rs` 300〜390 行。
- 最初に書くテスト: `tests/tzroutine_wasm.mjs` の `ping_pong`（Node 24 の JSPI）。
- 触ってはいけない: 既定の WASM の import なし（D-18）。`--wasm-feature jspi` の既存の用途（Net・`Async.block_on`）の import と挙動。
- 完了の見分け方: TZ03 の早見の例（`sum=5050`）が `tsuzuri build --target wasm32 --wasm-feature jspi --emit bindings-js` のグルーで、Node で同じ出力を出す。

## 目的

D-61 の決定 2 のとおり、wasm32 の tzroutine を JSPI で動かす。WebAssembly には実行中のスタックを切り替える命令がない（stack switching の提案は未採用）が、JSPI は wasm の呼び出しを Promise で中断・再開でき、中断したスタックを複数持てる。これを tzroutine の切り替えに使う。既定の WASM（import なし）では tzroutine を作れない（同期の型の 1 本の実行は TZ03）。

## 着手条件と停止条件

### 着手条件

- TZ04・TZ05・TZ06 が done。手元の `node --version` が 24 以上（`WebAssembly.Suspending` と `WebAssembly.promising` がある）。

### 停止条件

- **S1** 既定の WASM の出力に import が 1 つでも増える（`features.mjs` の WASM の import なしの検査）。
- **S2** JSPI で中断した G を再開したあと、線形メモリのスタックのポインター（`__stack_pointer`）が別の G の値のままになる経路を、`tzroutine-jspi.ll` の保存と復元で閉じられない。
- **S3** CI の Node で JSPI が使えない（`WebAssembly.Suspending` がない）。選択肢: A そのジョブのこのテストを飛ばす（理由を出す）、B Node の版を上げる（人間の承認）。
- **S4** 既存の `tests/async.mjs` の JSPI のケースか `tests/net_wasm.mjs` が落ちる。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認）

- TZ02 以降、tzroutine のランタイムの記号を持つプログラムを WASM へ出すと `E2000`（`jspi` なしは `option_requires_option`、ありは `option_conflict` の「未実装」）。
- 既定の WASM の `Channel`・同期の型は、TZ03 の `TZR_SINGLE`（`tzroutine.c` を 1 本の実行に組んだもの。待つべきときに状態 2 を返し、生成コードがトラップ）。
- JSPI のグルーは、export を `WebAssembly.promising` で包んで呼び出しを直列化し（前の呼び出しの Promise を待ってから次）、`tsuzuri_async.wait` を `WebAssembly.Suspending` にしている。Net の wasm（`--wasm-feature net`）も JSPI の Suspending の import を使う。
- WASM のスタックの検査（`with_stack_checks`）は、threads か 2 GiB を超えるメモリのときだけ入り、`__stack_low`／`__stack_high`（または threads の `tsuzuri_stack_base`／`top`）と比べる。

## 変えてはいけない意味

- 既定の WASM の import なし。tzroutine を使わないプログラムの WASM。
- tzroutine の意味（TZ02〜TZ08）: 終了の待ち、デッドロックの判定、チャンネル・`select`・同期・時間。WASM で違うのは、同時に走るのが 1 つだけであること（並列ではなく並行）と、デッドロックの報告の出し方（仕様 6）。

## 仕様

### JSPI の tzroutine

#### 1. 有効にする条件

- `tsuzuri build --target wasm32 --wasm-feature jspi`（`--emit wasm`・`object`・`llvm`・`bindings-js`）。wasm64 は対象外（`E2000`。JSPI のグルーが wasm32 だけのため）。`run`・`test` の WASM の実行器は対象外（`E2000`。既存の Async の扱いと同じ）。
- `--wasm-feature threads` との併用は `E2000`（既存の `jspi` と `threads` の拒否のまま）。
- tzroutine を生成する JSPI のプログラムの `DataChannel`（旧 `Channel`）: TZ12 が done なら、TZ12 が `tzroutine.c` に置いた `tsuzuri_channel_*` を `TZR_JSPI` で組み、G の待ちにする。TZ12 の前なら `E2000`（kind `option_conflict`、`DataChannel cannot be used in a program that spawns tzroutines on WebAssembly yet; use Channel`）。`channel-wasm.ll`（1 本の実行）は、tzroutine を生成しないプログラムだけ。

#### 2. 実行のモデル

- G は 1 つずつ走る。G が待つと、JSPI でその G の wasm のスタックを中断し、JavaScript の実行器（グルー）が次の実行可能な G を再開する（新しい G なら最初から呼ぶ）。
- export の呼び出しは、それ自体が 1 つの G（境界の G）。終了の待ち（TZ02 の境界）は、境界の G が待つ形で同じ。export の呼び出しは Promise を返す（既存の JSPI の export と同じ。呼び出しは直列化される）。
- G の線形メモリのスタックは 64 KiB（D3）。`with_stack_checks` を有効にし、各 G の範囲で検査する（溢れはトラップ。隣の G のスタックを壊さない）。
- 時計: グルーの `performance.now()`（ナノ秒に変換）。タイマー（TZ05）はグルーの `setTimeout` で期限に実行器を回す。
- Net の Suspending の import（`--wasm-feature net`）は、そのまま G の待ちになる（その G のスタックだけが中断する）。

#### 3. import と export（モジュール `tsuzuri_tz`）

| 名前 | 向き | 型 | 意味 |
| --- | --- | --- | --- |
| `tsuzuri_tz.suspend` | import（`WebAssembly.Suspending`） | `() -> ()` | 今の G を中断する。実行器がこの G を再開すると戻る |
| `tsuzuri_tz.spawn` | import | `(i32 g) -> ()` | 新しい G ができた（実行器の待ち行列へ） |
| `tsuzuri_tz.ready` | import | `(i32 g) -> ()` | G が実行可能になった |
| `tsuzuri_tz.clock` | import | `() -> i64` | 単調時計のナノ秒 |
| `tsuzuri_tz.timer` | import | `(i64 deadline) -> ()` | いちばん早いタイマーの期限が変わった（`-1` はなし） |
| `tsuzuri_tz.deadlock` | import | `(i32 waiting) -> ()` | デッドロックの判定（グルーが `RuntimeError` で export の Promise を拒否する） |
| `tsuzuri_tz_entry` | export | `(i32 g) -> ()` | G の最初の実行（グルーが `WebAssembly.promising` で呼ぶ） |
| `tsuzuri_tz_fire` | export | `(i64 now) -> ()` | 期限を過ぎたタイマーを発火させる（G を実行可能にする） |

- これらの import は、tzroutine を使う `jspi` のモジュールだけが持つ（既存の `tsuzuri_async`・`tsuzuri_net` と並ぶ）。

#### 4. 線形メモリのスタックの切り替え（`src/runtime/tzroutine-jspi.ll`）

- `define void @tzr_jspi_suspend(ptr %g)`: `%sp = call ptr @llvm.stacksave.p0()` を G の記録に保存 → `call void @tsuzuri_tz.suspend()`（import）→ 戻ったら `call void @llvm.stackrestore.p0(ptr <保存した sp>)`、スタックの検査の範囲の大域変数をこの G の範囲に戻す。この関数は `alloca` を持たない（自分の frame を線形メモリに置かない）。
- `define void @tsuzuri_tz_entry(i32 %g)`: 新しい G のスタックの上端へ `stackrestore`、範囲の大域変数を設定、`code(env)` を呼ぶ、終了の処理（TZ01 の `tzr_after_exit` に当たる: スコープの数を減らし、待つ G を `ready`）、戻る（JSPI の Promise が解決する）。`alloca` を持たない。
- WebAssembly の関数は、入口で `__stack_pointer` を局所変数に取り、出口で書き戻すので、`@tzr_jspi_suspend` から戻ったあとに呼ぶ関数が正しいスタックを使うには、上の `stackrestore` が要る（S2）。

#### 5. グルーの実行器（`src/runtime/bindings-core.mjs`）

```text
queue = []                          // 実行可能な G の番号
pending = new Map()                 // 中断した G → その再開の関数（Suspending の Promise の resolve）
run():                               // 実行器を回す。export の呼び出しの後、ready・spawn の後、タイマーで
  while queue が空でない:
    g = queue.shift()
    if pending.has(g): pending.get(g)(); pending.delete(g)   // 再開（次のマイクロタスクで wasm が続く）
    else: promising(tsuzuri_tz_entry)(g).catch(fail)        // 新しい G
    await その G が次に中断するか終わるまで（suspend の import の中で通知を受ける）
  if タイマーがある: setTimeout(期限) → tsuzuri_tz_fire(now) → run()
```

- `suspend` の import は `new WebAssembly.Suspending(() => new Promise(resolve => { pending.set(current, resolve); notify(); }))`（`notify` は `run` の待ちを解く）。
- 1 回の `run` の中で、G を 1 つずつ順に進める（同時に 2 つの wasm の実行を走らせない）。
- デッドロック: `queue` が空、タイマーがない、Net などのホストの待ちがない、それでも `pending` に G がある → `tsuzuri_tz.deadlock` と同じ扱いで、待っている export の Promise を `RuntimeError("all tzroutines are asleep - deadlock!")` で拒否し、インスタンスを捨てる（既存の JSPI のグルーの失敗の扱い）。

#### 6. 報告

- WASM には標準エラーがないので、ランタイムの文（`send on closed Channel` など）は出さない。トラップは `RuntimeError: unreachable`（既存の WASM のトラップと同じ）。デッドロックだけは、グルーが文付きの `RuntimeError` にする。

## 設計

| ファイル | 変更 |
| --- | --- |
| `src/runtime/tzroutine.c` | TZ03 の `TZR_SINGLE` の分岐の隣に `#if defined(TZR_JSPI)`: pthread・`mmap`・TLS・信号を使わない（`TZR_SINGLE` と共有）。M は 1 つ・P は 1 つ、`tzr_switch` の代わりに `tzr_jspi_suspend`、新しい G はグルーへ `tsuzuri_tz.spawn`。スタックは `tsuzuri_alloc` の 64 KiB。`fprintf` の報告は空 |
| `src/runtime/tzroutine-jspi.ll`（新規） | 仕様 4 の 2 つの関数 |
| `src/driver.rs` | WASM で tzroutine を生成し `jspi` のとき、`tzroutine.c` を `TZR_JSPI` で作り（TZ03 の `TZR_SINGLE` と同じ旗。キャッシュの単位 `"wasm-tzroutine-jspi"`）、`tzroutine-jspi.ll` を足し、wasm-ld に `--export=tsuzuri_tz_entry,tsuzuri_tz_fire`。TZ02 の `jspi` の「未実装」の `E2000` を消し、仕様 1 の拒否に置き換える。`with_stack_checks` を `TZR_JSPI` のときも有効にする |
| `src/llvm.rs` | `with_stack_checks` の範囲の大域変数を、`TZR_JSPI` では G ごとに切り替える（`tsuzuri_stack_base`／`top` の通常の大域変数版。threads の addrspace(1) の版と同じ比較） |
| `src/bindings.rs`・`src/runtime/bindings-core.mjs` | 表の `async` の欄の隣に `tzroutine: true`（モジュールが `tsuzuri_tz` を import するとき）。仕様 5 の実行器。`inspect` の検査（`tsuzuri_tz` の import があるのに JSPI がない環境は `Error`） |
| `src/test_runner.rs` | WASM のテストの実行器は `TZR_SINGLE` の場合だけ受け付ける（`jspi` の tzroutine は `E2000`） |
| `.gitignore`・`scripts/check-runtime-includes.sh` | `!src/runtime/tzroutine-jspi.ll` |

## 前提とする他チケットのインターフェース

- TZ01 のランタイムの構造（OS の呼び出しを小さな関数に閉じ込めた形）。TZ02 の `spawns`・境界。TZ05 のタイマー。

## 他チケットへ提供するインターフェース

- `TZR_JSPI` の組み方（TZ13 で `Async` の JSPI の経路を消したあとも、`--wasm-feature jspi` は Net と tzroutine に残る）。

## 実装手順

1. **変更**: ブランチ `feat/tz09-wasm-jspi`。`TZR_JSPI` の分岐、`tzroutine-jspi.ll`、ドライバー、スタックの検査。**確認**: `tsuzuri build --target wasm32 --wasm-feature jspi` が TZ02 の fixture で成功し、`wasm-objdump -x`（または `WebAssembly.Module.imports`）で `tsuzuri_tz` の import が仕様 3 のとおり。
2. **変更**: グルーの実行器。**確認**: `tests/tzroutine_wasm.mjs`（新規）で `ping_pong`・`pipeline`・`select`・`wait_group_sum`・`timeout`・`deadlock`（`RuntimeError` の文）・`stack_overflow`（64 KiB を超える再帰がトラップし、ほかの G のスタックを壊さない）。
3. **変更**: 既存の JSPI のテスト（S4）、文書、CI（`targets.yml` の WASM の手順に `node tests/tzroutine_wasm.mjs`）。

## テスト計画

- `tests/tzroutine_wasm.mjs`（上）。期待値は native の E2E と同じ値（独立した計算。native の出力を写さない）。

## ドキュメント

- `compiler/webassembly.md`: 「tzroutine と同期の型」（TZ03 が書いた 1 本の実行の節に、JSPI の tzroutine、import の表、64 KiB のスタック、並行であって並列でないこと、デッドロックの `RuntimeError`）。
- `async-tasks-and-lazy/tzroutine.md` のターゲットの表、`compiler/option.md`（`--wasm-feature jspi` の説明に tzroutine）、`compiler/platforms.md`。
- `docs/language.md`・`docs/architecture.md`。

## 受け入れ条件

- [ ] 既定の WASM の import なしが変わらない。
- [ ] JSPI で tzroutine の E2E が native と同じ値を返す。既存の JSPI のテストが通る。
- [ ] 文書を更新した。

## 落とし穴

- **`__stack_pointer` は 1 つの大域変数**: JSPI はエンジンのスタック（呼び出しの列）を中断するが、線形メモリのスタックのポインターは中断しない。G を再開するたびに戻す（仕様 4）。
- **`tzr_jspi_suspend` に `alloca` を置かない**: 自分の frame が線形メモリにあると、再開の後のエピローグが別の G の値で `__stack_pointer` を書き戻す。`.ll` で書くのはこのため（C だと最適化の段で frame ができうる）。
- **export の直列化**: 既存の JSPI のグルーは export を 1 つずつ呼ぶ。境界の G が待っている間に次の export を呼ばない（Promise を待つ）。

## 対象外

- wasm64 の tzroutine、`--wasm-feature threads` との併用（Worker の上の M:N）。WebAssembly の stack switching の提案（採用されたら別チケットで JSPI を置き換える）。
- ブラウザの main thread での長い同期の実行の分割（tzroutine は待つ点でしか譲らない。TZ10 のプリエンプションは WASM では無効）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | JSPI で G のスタックを中断し、グルーの実行器が交代させる（M:1） | WebAssembly に stack switching がない。JSPI は既に Net と `Async.block_on` で使っている | 承認済み（D-61 の決定 2） |
| D2 | 既定の WASM の 1 本の実行は TZ03 の `TZR_SINGLE`。このチケットは JSPI だけ | 旧 `Channel` を置き換える TZ03 で、既定の WASM の `Channel` を途切れさせないため | 既定案 |
| D3 | WASM の G のスタックは 64 KiB の固定 | 既定の線形メモリは 16 MiB（F11 で増やせる）。検査付きで溢れはトラップ | 既定案 |
| D4 | タイマーはグルーの `setTimeout` | ホストのイベントループを止めない | 既定案 |
| D5 | デッドロックは `RuntimeError` の文で export の Promise を拒否する | WASM に標準エラーがない | 既定案 |
| D6 | `run`・`test` の WASM の実行器では JSPI の tzroutine を拒否する | 既存の Async の扱いと同じ（実行器が JSPI を駆動しない） | 既定案 |
| D7 | wasm64 は対象外 | JSPI のグルーが wasm32 だけ | 既定案 |

## 着手時の再検討（2026-11-04、HEAD `16bae89f`。TZ08・TZ12・TZ13 の上）

起票（HEAD `9a76d8ac`）のあとに入った TZ08・TZ12・TZ13 と照らして、次のとおり合わせた。

1. **`DataChannel`（仕様 1）**: TZ12 が済んでいるので、spawn する wasm のプログラムの `tsuzuri_channel_*` は `TZR_JSPI` で組んだ `tzroutine.c` のもの（G の待ち）にした。`channel-wasm.ll` と F10 の `tsuzuri_mutex_wait_ok` の検査は使わない（ネイティブと同じく、ロックを持ったまま待てる）。spawn しないプログラムは変わらない（S1）。
2. **`Parallel` と `Mutex`**: spawn する wasm のプログラムの `Parallel.*` は `task-wasm.ll`（呼んだ G の上の添字順の逐次）のままにした。大域の状態を持たないので、G が何個あっても正しい。コールバックが待てばその G が中断し（ほかの G は走る）、コールバックの spawn は呼んだ G の spawn の scope に数える（ネイティブの手伝いと同じ scope）。`Mutex` は `uses_tzroutine` で G のロック（`tsuzuri_tz_mutex32_lock`）になるので、JSPI の中断をまたいで持てる（`mutex_across_wait`、`sync_types` の `mutex_hold_across_wait`）。F10 の旗（`sync-wasm.ll`）は spawn するプログラムでは使われない。
3. **TZ13**: `tests/async.mjs` はない。S4 は `tests/net_wasm.mjs`（`jspi_queue` を含む）と `tests/bindings.mjs`。TZ13 の引き継ぎの「中断する export の 2 回の呼び出しの直列化」は `serialized`（3 回を同時に呼び、報告の順が `1,-1,2,-2,3,-3`）、「WebAssembly の上の同時の待ち」は `tests/fixtures/tzroutine_net_wasm`（1 つのモジュールのサーバーと 50 のクライアント、後の write を待つ read、ピアの close と競う write、close をまたぐ read と、ほかが走る間の read の時間切れ、ソケットを待つ export の 2 回の呼び出し）で確かめた。
4. **TZ08**: `tsuzuri_tz_blocking_enter`・`exit`・`callback_enter`・`exit`・`lock_thread`・`unlock_thread` は空（1 本のスレッド）。`with_blocking_externs` は native だけのまま。Net の待ちは wasm では JSPI の import そのものなので、その G だけが中断するが、`__stack_pointer` と G の範囲と現在の G を戻さないと S2 が開く: `llvm::with_switching_net_imports` が `tsuzuri_net` の import を `noinline` の関数で包み、保存と復元をする（これが無いと、ソケットから戻った G がほかの G の記録で park し、`tsuzuri_tz_entry` が死んだ G を呼ぶ不具合が出た）。
5. **スタックの切り替え（仕様 4、S2）**: `llvm.stacksave`／`stackrestore` は使えない。実験で、frame のない関数の `stackrestore` は `__stack_pointer` へ書かれず（`tsuzuri_tz_entry` では load だけが残った）、`-O3` の InstCombine は `ret` の前の stackrestore を消した。`@__stack_pointer` を `addrspace(1)` の外部の大域変数として宣言して volatile で読み書きすると、バックエンドのスタックポインターと同じ記号になり、`-O0`・`-O3` とも `global.get`／`global.set` が出る（`llvm-objdump` で確認）。これで S2 は閉じた。中断の間はスケジューラーのスタック（16 KiB）を置き、G が走っていない間のグルーの同期の呼び出し（`tsuzuri_tz_fire`、ソケットの結果の `tsuzuri_alloc`）はそこで走る。
6. **`TZR_JSPI` の置き場**: `tzroutine.c` の中の分岐ではなく、Windows と同じく `src/runtime/tzroutine-jspi.h`（`driver::tzroutine_jspi_source` が展開する）にした。TZ10 が `tzroutine.c` のスケジューラーを並行して変えるので、共有部分の変更を最小（`#if` の数か所、`DataChannel` と `tzr_scase` の wasm32 の配置、タイマーの発火を `tzr_timers_run` に切り出してタイマーのスレッドと共有）にするため。
7. **仕様 3 との違い**: デッドロックはグルーが見つける（ホストの待ちを知るのはグルーだけ）ので、import `tsuzuri_tz.deadlock` は作らなかった。`suspend` は `(i32 g)`（グルーが再開を G で引く）、`tsuzuri_tz_fire` は引数なし（ランタイムがタイマーのスレッドと同じく `clock` を読む）。import はプログラムが到達したものだけ（タイマーのないプログラムに `clock`・`timer` はない）。
8. **仕様にない追加**: (a) G のスタックは最初に走るときに取り、終わった G のものを 16 個まで使い回す（spawn の時に取ると 1,023 個の木の `nested` が 16 MiB を超え、解放した 64 KiB のブロックは小さい確保に切られてヒープが伸び続けた）。(b) グルーは 4 ms ごとにホストに番を渡し（`setImmediate`）、期限の過ぎたタイマーは `setTimeout` を待たずに発火させる（`Routine.yield` で回る G がマイクロタスクを埋め、`setTimeout` が永遠に来なかった）。
9. **拒否**: wasm64 と `jspi`（D7）、`webgpu` と spawn（WebGPU の import は `webgpu.mjs` のホストのもので、tzroutine の実行器を持たない。最も保守的に `E2000`）、`--emit bindings-js` の `jspi` なし（ビルドと同じ `option_requires_option`）。`tsuzuri test` は `--wasm-feature` を受けないので、テストの実行器の文を「`--wasm-feature jspi` が要る」から「テストの実行器は tzroutine を走らせられない」に直し、kind を `option_conflict` にした（D6）。
10. **`Time.sleep`**: spawn する wasm のプログラムではスケジューラーのタイマー（`tsuzuri_tz_sleep`）にした（それ以外は今までどおり `sleep_ms`）。
11. **既知の制限（D3 を保つ）**: `-O0` の `f128`・`d128` のソフトウェア演算（`numeric.ll` の `tz_soft_op` 28 KiB と `pack` 33 KiB の frame）は 64 KiB のスタックに入らず、tzroutine の中ではスタックの溢れのトラップになる（`-O3` では動く）。`tzroutine_values` の `wide_numbers` を `-O0` でトラップ、`-O3` で値として検査し、文書に書いた。1,000 個の G を同時に待たせる `yields 1000` は `--wasm-max-memory 128MiB` で走らせる。

## 実装と検証（2026-11-04）

- **範囲**: 実装手順 1〜3 をすべて実装した（1 つの PR、TZ08・TZ12・TZ13 の上に重ねる）。
- **ランタイム**: `src/runtime/tzroutine-jspi.h`（新規。`TZR_JSPI` の M:1 のスケジューラー: G の記録とスタックの使い回し、`tzr_park`・`tzr_ready`・`tsuzuri_tz_yield`・`tsuzuri_tz_go`、空の blocking・callback・lock_thread、`tsuzuri_tz_report` は空、`Routine` の問い合わせは 1、`malloc` 系は `tsuzuri_alloc` に大きさの前置き）、`src/runtime/tzroutine-jspi.ll`（新規。`tzr_jspi_suspend`・`tsuzuri_tz_entry`・`tsuzuri_tz_fire`、スケジューラーのスタック 16 KiB）、`src/runtime/tzroutine.c`（`TZR_JSPI` の include の切り替え、blocking・sysmon・netpoller を除く `#if`、`DataChannel` の待ちの語と `tzr_scase` の wasm32 の配置、タイマーの発火を `tzr_timers_run` に切り出してタイマーのスレッドと `tzr_jspi_fire` で共有、`tzr_heap_push` が最早の期限を import で知らせる）。
- **コンパイラ**: `src/llvm.rs`（spawn する wasm32 のスタックの検査を G ごとの範囲で、`with_switching_net_imports`、spawn する wasm の `DataChannel` は `tzroutine.c` のもの、`tsuzuri_alloc` を常に定義）、`src/llvm_sync.rs`（`channel_unlocked` を spawn する wasm で省く）、`src/llvm_timer.rs`（`Time.sleep` を spawn する wasm でスケジューラーの待ちに）、`src/driver.rs`（`stack_checks`・`jspi_tzroutines`・`tzroutine_jspi`、`tzroutine.c` を `-DTZR_JSPI` でキャッシュの単位 `wasm-tzroutine-jspi`、`.ll` の追加、`--export=tsuzuri_tz_entry,tsuzuri_tz_fire`、未実装の `E2000` を wasm64・webgpu・`jspi` なしの拒否に、`--emit bindings-js` の拒否、プラットフォームの検査は native だけ、`tzroutine_jspi_source`）、`src/test_runner.rs`（WebAssembly のテストの実行器の拒否の文と kind）、`src/bindings.rs`（表の `"tzroutine": true`）。
- **グルー**: `src/runtime/bindings-core.mjs`（`tzImports` の実行器: 列・中断した G・走っている G・ホストの待ち・最早の期限、4 ms の時間割り、期限の過ぎたタイマーの発火、デッドロックの `RuntimeError`、境界の `tz.call`、`hostSuspending`、`inspect` の `tsuzuri_tz` と export の検査、`fail` がデッドロックの誤りをそのまま渡しタイマーを止める）、`src/runtime/bindings-net.mjs`（ソケットの Suspending を `hostSuspending` で作る。tzroutine のないモジュールでは今までと同じ）。
- **テスト**: `tests/tzroutine_wasm.mjs`（新規）と fixture `tzroutine_wasm`・`tzroutine_net_wasm`。拒否（`jspi` なしの build と bindings、wasm64、threads、webgpu、`tsuzuri test --target wasm32`）、spawn しないモジュールの import なし（`jspi` があってもなくても）、`-O0`・`-O3` の import と export、`sum`（`sum=5050`、完了の見分け方）・`ping_pong`・`pipeline`・`select_two`・`wait_group_sum`・`timeout`・`sleep_lets_others_run`・`deadlock`（`RuntimeError` の文と、次の呼び出しの新しいインスタンス）・`neighbors`（64 KiB の中の再帰で隣の G の値が無事、40 段でトラップ、`--trap-info` で `stack overflow`）・`interleaved`・`serialized`・`data_channel_sum`・`parallel_in_routines`・`mutex_across_wait`・`trap_in_routine`・`counts`、ソケットの 5 つの case と、スケジューラーの fixture（`tzroutine`・`tzroutine_values`・`channel`・`select`・`timers`・`sync_types`・`context`）の native の suite を同じ期待値で（デッドロックの case は `RuntimeError`、ほかのトラップは `TsuzuriTrap`）。`tests/tzroutine.rs` に `jspi_modules_that_spawn_get_the_stack_switch`（IR の決定性、spawn しないモジュールの IR が `jspi` で変わらない）・`jspi_net_imports_keep_the_stack_pointer_of_the_g`・`the_webassembly_test_runner_refuses_tzroutines`・`jspi_bindings_of_a_module_that_spawns_run_its_tzroutines`、`rejects_unsupported_targets` の `jspi` の行を新しい拒否に（TZ02 の「未実装」は TZ09 が置き換えるもの）。`src/driver.rs` に `tzroutine_jspi_source_inlines_the_webassembly_half`。
- **実行したコマンドと結果**: `cargo fmt --all -- --check`、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）、`cargo nextest run --locked --no-fail-fast`（2921 passed、4 skipped）、`node tests/tzroutine_wasm.mjs target/debug/tsuzuri`（Node.js 24.22。refusals と、`-O0`・`-O3` それぞれ 22 の case のまとまり）、`node tests/net_wasm.mjs`（`-O0`・`-O3` passed）、`node tests/bindings.mjs`（passed）、`node tests/tzroutine.mjs`（119 runs）、`node tests/tzroutine_runtime.mjs`（504 runs。`TSUZURI_TSAN=1 TSUZURI_ASAN=1` で 626 runs）、`node tests/features.mjs target/debug/tsuzuri`（`regex`・`unicode` 以外の 78 suite が native と WASM の `-O0`・`-O3` で通り、WASM の import なしの検査も通った。2 つは手元の Node 24.22 の Unicode と std の差で、TZ12・TZ13 の前から同じ）、変更した言語リファレンスの 21 ページの `check-docs`（579 links、107 examples、202 native runs）、`gen-docs --check`（19 blocks、0 stale）、`check-cli-docs`（4 checks）、`check-spec --require-markers`（239 active）、`check-runtime-includes`（60 files）、`repo-kb build`（新しい警告なし。残る 4 件は G13・G16〜G18 の既存のもの）、`check-guide-lite`（88 references）。`tests/tzroutine_runtime.c` を zig cc の `x86_64-linux-gnu`（`-pthread`）と `x86_64-windows-gnu` で `-Wall -Wextra -Werror` でコンパイルし、`tzroutine.c` の `-DTZR_JSPI` を wasm32 の `-O0`・`-O2`・`-Oz` で `-Wall -Wextra -Werror` でコンパイルした。
- **文書**: `async-tasks-and-lazy/tzroutine.md`（ターゲットの表と新しい節「WebAssembly（JSPI）」）、`compiler/webassembly.md`（「JSPI の tzroutine」: 振る舞い、import と export の表、ホストの約束）、`compiler/option.md`（`jspi` の行、説明、拒否の文の表）、`compiler/diagnostics.md`（`E2000` の行）、`compiler/platforms.md`、`built-in-types-and-modules/` の `routine.md`・`channel.md`・`datachannel.md`・`mutex.md`・`parallel.md`・`time.md`・`timer.md`・`ticker.md`・`context.md`・`errgroup.md`・`waitgroup.md`・`result.md`・`net.md`、`async-tasks-and-lazy/select.md`、`languages/strategy.md`・`how-about-tsuzuri.md`、`docs/language.md`（tzroutine のターゲットと WebAssembly、タイマー、同期の型、`Context`、JSPI のグルー）、`docs/architecture.md`（`TZR_JSPI` の項: 線形メモリのスタック（S2）、スタックの検査、生成の違い、グルーの実行器）、`README.md`（機能の一覧、変更の種類の表、検証のコマンド）。
- **CI**: `targets.yml` の tzroutine の手順と `ci.yml` の tzroutine の手順に `node tests/tzroutine_wasm.mjs`（どちらも Node.js 24）。
- **残したこと**: `-O0` の `f128`・`d128` のソフトウェア演算の frame（再検討の 11）は、tzroutine の中ではスタックの溢れになる。`numeric.ll` を `-O0` のプログラムでも最適化して組むか、G のスタックの大きさを選べるようにするのは別のチケット。ブラウザ（`setImmediate` がなく `setTimeout(0)` で番を渡す）と Linux・Windows の Node.js では、`tests/tzroutine_wasm.mjs` を手元では走らせていない（CI の `ci.yml`・`targets.yml` が Linux と macOS で走らせる）。
