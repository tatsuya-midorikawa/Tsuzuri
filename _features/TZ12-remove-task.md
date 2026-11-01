# TZ12: `Task`・`task { }` の削除と移行（`DataChannel` の待ちの移設を含む）

| 項目 | 内容 |
| --- | --- |
| ID | TZ12 |
| 優先度 | P1 |
| 規模 | L。Phase 1（ネイティブの `Parallel`・`Mutex`・`DataChannel` を全プログラムで新しいランタイムへ）M、Phase 2（`Task` の構文・型・組み込みの削除と `E1040`）L、Phase 3（テスト・ベンチマーク・文書・CI の整理）L。旧 `Channel` は TZ03 で `DataChannel` に改名済み |
| 依存 | TZ04, TZ07, TZ11 |
| 後続 | TZ13 |
| 状態 | todo |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61 の決定 4: 即削除。非推奨の期間なし）。決定事項 D1〜D8 は既定案 |
| 手本にする既存実装 | 削除の手本はない（初めての言語機能の削除）。診断の追加は G23 の仕様 S1（kind の登録）と D-51（`help`）、規則 ID の退役は `spec/README.md`（番号を再利用せず `retired`）、kind を消さない規則は `src/diagnostic_kinds.rs` の先頭の注記 |
| 主な影響ファイル | 下の「削除と書き換えの一覧」の全ファイル。代表: `src/lexer.rs`・`src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/parser.rs`・`src/parse_control.rs`・`src/check.rs`・`src/closures.rs`・`src/computation.rs`・`src/ownership.rs`・`src/polymorph.rs`・`src/llvm.rs`・`src/llvm_task.rs`・`src/llvm_parallel.rs`・`src/llvm_sync.rs`・`src/driver.rs`・`src/test_runner.rs`・`src/runtime/task.c`・`src/runtime/task-windows.h`・`src/runtime/sync.ll`・`src/runtime/task-wasm.ll`・`src/diagnostic_kinds.rs`・`src/explain/`・`tests/`・`benchmarks/`・`docs/`・`_tsuzuri/language-reference/`・`spec/rules/TSK.md`・`vsc/`・`guide/`・`AGENTS.md`・`README.md` |

## 実装者向け早見

- 範囲: `task { }`・`Task<T>`・`Task.run`・`Task.parallel`・`Task.parallel_results`・`Task.scope` を言語から消し、書かれていれば `E1040` で移行先を示す。`task.c` のプールを消し、ネイティブの `Parallel`・`Mutex`・`DataChannel`（TZ03 で旧 `Channel` を改名したもの）は全プログラムで tzroutine のランタイムを使う。`DataChannel` の API と意味は変えず、待ちだけが「OS スレッドを止める」から「tzroutine（擬似 G）だけが待つ」に変わる。
- **内部の 1 回だけの関数は残す**: `tzroutine { }` は `Type::Task`・`LambdaKind::Task`・`"$task"` の module を使っている（TZ02 の D4）。これらは名前を `Type::Once`・`LambdaKind::Once`・`"$once"` に変えて残す（利用者には見えない。D3）。
- 最初に読む: ① D-61 ② 仕様 1〜3 ③ 「削除と書き換えの一覧」④ TZ02 の設計（tzroutine が流用している部分）。
- 進め方: Phase 1 → 2 → 3 → 4 の順。各 Phase の終わりで `cargo nextest run --locked` と、その Phase の E2E を通す。**1 つの PR**（AGENTS.md）だが、Phase ごとにコミットを分ける。
- 触ってはいけない: `tzroutine { }`・`Channel`・`select`・同期の型・`Parallel.*` の意味。WASM の `--wasm-feature threads` の `Parallel`・`Mutex`・`Atomic`（ワーカーのプールは `task-wasm-threads.c` に残る）。診断の kind（登録簿から消さない）。
- 完了の見分け方: `grep -rnwE "Task\.(run|parallel|parallel_results|scope)|task \{" src std tests examples benchmarks _tsuzuri docs guide README.md` が、`E1040` の検査・移行の文書・履歴（`_features/_completed/`）以外で 0 件。

## 目的

D-61 の決定 4 のとおり、Task を削除する（旧 Channel は TZ03 で `DataChannel` に改名され、このチケットで待ちを新しいランタイムへ移す）。tzroutine・`Channel`・`WaitGroup`・`ErrGroup`・`Routine.scope`・`Parallel` がその役割を引き継いだ（TZ02〜TZ11）。2 つのスレッドの集まり（`task.c` のプールと tzroutine の M）が 1 つになり、文書と実装の両方が単純になる。

## 着手条件と停止条件

### 着手条件

- TZ02〜TZ07・TZ11 が done（`Routine.scope`・`ErrGroup` がある）。TZ08 は要らない。
- TZ01 の Phase 2（Windows）が done（`windows.yml` で `node tests/tzroutine_runtime.mjs` が通っている）。`Parallel`・`Mutex`・`DataChannel` は Windows でも動いているので、Windows のランタイムなしに `task.c` を消せない。
- 基準: `cargo nextest run --locked` の件数と、`node tests/features.mjs target/debug/tsuzuri` の全 suite の結果。削除の前の時点で、`git grep -c` で「削除と書き換えの一覧」の各ファイルの参照の数を控える（漏れの確認に使う）。

### 停止条件

- **S1** `Parallel.*` の結果（F15 の逐次と同じ還元を含む）が、Phase 1 の移設で変わる。
- **S2** tzroutine の型検査・生成が、`Type::Task` などの名前の変更で変わる（TZ02〜TZ11 の E2E が 1 つでも落ちる）。
- **S3** 既存のテストの期待値を、移行の書き換え以外の理由で変えないと通らない（Task を「非 Copy の例の型」として使っていたテスト（一覧の 1c）は、別の非 Copy の型に置き換える。期待値の意味は変えない）。
- **S4** `E1040` を出すために、名前の解決の誤りの経路を大きく変える必要がある（利用者が定義した `Task` という名前のモジュール・型を壊す）。
- **S5** 文書の検査（`check-docs`・`check-spec`・`gen-docs --check`・`check-cli-docs`・`check-guide`・`check-guide-lite`）を、文書の中身の削除だけでは通せない。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認。TZ11 の後に読み直す）

- Task の表現: 字句 `TokenKind::Task`（`src/lexer.rs` 660 行）、AST `ExprKind::Task`・`TaskRun`（`src/syntax.rs` 1153 行。codec の tag 17・18）、`TypeExprKind::Task`（782 行。codec の tag 11）、型 `Type::Task`（`src/check.rs` 168 行）、`BuiltinType::Task`、`TypedExprKind::TaskRun`・`TaskParallel`・`TaskParallelResults`（4060〜4062 行）、`Builtin::TaskRun`・`TaskParallel`・`TaskParallelResults`・`TaskScope`、`higher_kinds::Constructor::Task`、`closures::LambdaKind::Task`、`CheckedFunction.is_task`、module `"$task"`。予約名の扱いは `check.rs` 6435・6936・8754・8836・8909 行、`polymorph.rs` 1089 行、`driver.rs` 2021 行、`package.rs` 540 行（`RESERVED_MODULES` には入っていない）。
- 構文解析の `in_task`・`task`・`task_return`・`task_do`、`binding` の `TaskRun` の包み（`src/parser.rs` 2916〜3038 行）と、計算式の暗黙のビルダー `"task"`（`src/computation.rs` 1187・1227・1457〜1476・1592〜1597・1680〜1681・1845・1949〜2128 行）。
- 生成: `src/llvm_task.rs`（`parallel_results`）、`src/llvm.rs` の `parallel_tasks`（6436〜6482 行）と `TaskRun`、`src/llvm_parallel.rs` の `Task.scope`（579〜641 行）。ランタイム `src/runtime/task.c`・`task-windows.h`・`sync.ll`・`task-wasm.ll`・`channel-wasm.ll`・`task-wasm-threads.c`。
- 旧 Channel は TZ03 で `DataChannel` に改名された。生成コードは今も `tsuzuri_channel_send`・`recv`・`clone_sender`・`close`（状態 0・1・2）と `tsuzuri_mutex_wait_ok` を呼び、ネイティブの待ちは `task.c`（574〜699 行。手伝いと、スレッド数による行き詰まり、判定の文 `deadlock: every task is waiting on a channel`）、既定の WASM は `channel-wasm.ll`、threads は `task-wasm-threads.c`。ブロックの先頭 80 バイトは容量・要素の大きさ・先頭・数・送り手・受け手・所有者・予備（56 バイト目）・待ちの 2 語（64・72 バイト目）で、生成コードが予備と待ちの語を 0 にする（`src/llvm_sync.rs` の `channel_bounded`）。TZ03 の `E1040` と `removed_feature` がある。

## 変えてはいけない意味

- tzroutine の一式（TZ02〜TZ11）と `Parallel.*`・`Atomic`・`Arc`・`Mutex` の API と意味。
- 診断の kind の登録簿（D-51: 消さない、改名しない）。
- 利用者が `Task` という名前のモジュール・型・関数を自分で定義したプログラムは、そのまま動く（削除で名前が空く）。

## Phase 分割

| Phase | 内容 |
| --- | --- |
| 1 | ネイティブの `Parallel.*`・`Mutex` を、全プログラムで tzroutine のランタイムへ（`globals.tzroutine` の分岐を消す） |
| 2 | Task の構文・型・組み込み・生成・ランタイムの削除、`Type::Task` などの改名、`E1040` |
| 3 | テスト・ベンチマーク・文書・ガイド・CI・VS Code の整理と移行の文書 |

## 仕様

### 1. 削除したものを書いたとき（`E1040`）

| 書いたもの（利用者が同じ名前を定義していないとき） | kind | 文（help は D-51 の `help`） |
| --- | --- | --- |
| `task { … }` | `removed_task_expression` | `'task { }' was removed`。help: `run the work directly, or spawn it with 'tzroutine { ... }' and wait with WaitGroup or a Channel` |
| 型 `Task<T>` | `removed_task_type` | `the type 'Task' was removed`。help: `return the value directly; to run work concurrently, use 'tzroutine { ... }'` |
| `Task.run e` | `removed_task_api` | `'Task.run' was removed`。help: `call the work directly` |
| `Task.parallel e` | `removed_task_api` | `'Task.parallel' was removed`。help: `use Parallel.init or Parallel.map for data parallelism, or tzroutines with a WaitGroup` |
| `Task.parallel_results e` | `removed_task_api` | `'Task.parallel_results' was removed`。help: `use ErrGroup: ErrGroup.go for each job and ErrGroup.wait for the first error` |
| `Task.scope s n f` | `removed_task_api` | `'Task.scope' was removed`。help: `use Routine.scope, which takes the same arguments` |

- 位置は書いた名前（`task` の語、`Task`）の `Span`。行は TZ03 の `removed_feature` に足す（旧 `Channel` の行は TZ03 が足し済み）。
- 判定は名前の解決の **後**: 名前が利用者の定義に解決しないときだけ `E1040`（S4）。`task` は予約語でなくなる（変数名に使える）。`task { … }` は計算式（ビルダー名 `task`）として読まれ、ビルダー `task` が見つからないときの診断を `E1040` にする（`@alias task` の利用者のビルダーがあれば、それが使われる）。
- `--json` の `help` と `fixes`: `Task.scope` → `Routine.scope` だけは機械的に置き換えられるので、`fixes` に置換を 1 つ付ける（適用性 `certain`。G24 の `tsuzuri fix` が適用できる）。ほかは `help` だけ。

### 2. 実行の変化

- ネイティブの `Parallel.*` は、すべてのプログラムで tzroutine のランタイム（TZ11 の `tsuzuri_tz_parallel`）。`Parallel` だけを使うプログラムの実行ファイルにも `tzroutine.c` が入る（スレッドの数は `TSUZURI_MAXPROCS`、既定は CPU 数。旧のプールの `min(CPU, 32) - 1` の追加ワーカーではない）。
- ネイティブの `Mutex` は、すべてのプログラムで TZ06 のロック（同じロックの入れ子だけがトラップ、違うロックの入れ子とロックの中の待ちを許す）。旧の「ロックの中の `Task.parallel`・`DataChannel` の操作はトラップ」は消える（`tsuzuri_mutex_parallel_ok`・`tsuzuri_mutex_wait_ok` の検査と、生成コードのその呼び出しを消す）。
- `--trap-mode return` と `Mutex` の組み合わせ（F10 の `tsuzuri_sync_hooks.abandon` で、トラップしたときに持っていたロックを外す）は、新しいロックでも保つ: `trap.c` の `abandon` のフックが、現在の G（擬似 G）の `held` の連結リストのロックをすべて外す（TZ06 の `held`）。`--trap-mode return` と tzroutine の生成の組み合わせは今までどおり `E2000`（TZ02）。
- `--trap-mode return` の境界の中（`tsuzuri_boundary_owner()` が非 NULL）の `Parallel.*` は、手伝いの tzroutine を作らず、呼んだスレッドで添字の順に逐次に走る（`tsuzuri_tz_parallel` の先頭で判定する）。別の OS スレッドのトラップを境界へ返せないため。旧のプールは子のトラップを境界へ返して並列に走ったので、`--trap-mode return` の `Parallel` は遅くなる（結果は同じ）。文書（`parallel.md`・`compiler/native-interop.md`）に書く。
- ネイティブの `DataChannel` は、すべてのプログラムで tzroutine のランタイムが待つ: 生成コードはそのまま `tsuzuri_channel_*` を呼び（ABI・状態の値・ブロックの配置は変えない）、その C の関数を `tzroutine.c` に置く（設計の「DataChannel のランタイム」）。待つのは G（tzroutine）か擬似 G（`main`・ホストのスレッド）だけで、「未着手の仕事を手伝う」と「待つ数がスレッド数を超えると完了しない」はなくなる。全員が待ったときの文は TZ01 の `Tsuzuri runtime: all tzroutines are asleep - deadlock! (…)`（待ちの理由 `DataChannel.send`・`DataChannel.recv`）になり、旧の `deadlock: every task is waiting on a channel` は出なくなる。結果（値・順序・`Error item`・`None`）は変わらない。
- WASM: 既定の WASM の `Parallel`（逐次。`task-wasm.ll`）・`Mutex`（`sync-wasm.ll`）・`DataChannel`（`channel-wasm.ll`）、`--wasm-feature threads` の `Parallel`・`Mutex`・`Atomic`・`DataChannel`（`task-wasm-threads.c`）は変えない。`Task.parallel` の分だけを消す。JSPI の tzroutine を生成するプログラムの `DataChannel` は TZ09 の扱い（TZ09 の仕様）。

### 3. 内部の名前

- `Type::Task` → `Type::Once`、`LambdaKind::Task` → `LambdaKind::Once`、`CheckedFunction.is_task` → `is_once`、module `"$task"` → `"$once"`（KG・カバレッジの除外の一覧 `"$lambda" | "$task"` も直す）、`higher_kinds::Constructor::Task` は削除（`Once` は利用者が書けない型なので構築子に出ない）。
- 型の表示（`display`）: `Type::Once` は利用者の診断に出ないはずだが、出たら `tzroutine body` と表示する（`Task<…>` を表示しない）。
- 診断の文の中の `task`: tzroutine の捕捉の検査（`TaskRequiresSend` など、kind はそのまま）の文を、`task` から `tzroutine` に直す（例: `tasks require Send values` → `values moved into a tzroutine must be Send`）。

## 設計: 削除と書き換えの一覧

**全件をこの表のとおり処理する。** 表にないファイルで参照が見つかったら（着手条件の `git grep`）、同じ方針で処理してチケットの末尾に記録する。

### コンパイラ（`src/`）

| ファイル | 処理 |
| --- | --- |
| `src/lexer.rs` | `"task" => TokenKind::Task` を消す（`task` は識別子）。字句のテストの `task` の期待を直す |
| `src/syntax.rs` | `TokenKind::Task`・`ExprKind::Task`・`ExprKind::TaskRun`・`TypeExprKind::Task` を消し、`visit`・`visit_mut` の腕を消す |
| `src/syntax_codec.rs` | 上の 3 つの variant の put／get を消す。**tag の番号は詰めない**（tag 11・17・18 は欠番。`get` は欠番を `Err` にする）。`("ExprKind", N)` の件数のテスト |
| `src/frontend_cache.rs` | `FRONTEND_FORMAT` + 1 |
| `src/parser.rs`・`src/parse_control.rs` | `fn task`・`task_return`・`task_do`、`in_task` の field と全ての使用、`binding` の `TaskRun` の包み、`TokenKind::Task` の 3 つの分岐（4334・4067・1490 行）、`Task<…>` の型の構文（2545〜2554 行）を消す。`do!`・`return` をビルダーの外に書いたときの誤り（`DoBangOutsideTask`・`ReturnOutsideTaskEnd`）の文を、`task` に触れない文に直す（`'do!' is only allowed in a computation expression or a function body that runs IO directly` など。kind はそのまま） |
| `src/computation.rs` | 暗黙のビルダー `"task"` の特別扱い（`implicit_task_sequence`・`lower_implicit_task_sequence`・`checked_lambda` の `LambdaKind::Task`・`TaskRun` の包み）を消す。`ImplicitTaskNeedsBuilder` の kind は出なくなる |
| `src/check.rs` | `BuiltinType::Task`、`TypedExprKind::TaskRun`・`TaskParallel`・`TaskParallelResults`、`Builtin::TaskRun`・`TaskParallel`・`TaskParallelResults`・`TaskScope`（`ALL`・`name`・`scheme`・`is_parallel`・`parallel_callback`）、`Task has no function …` の文（11643 行）、予約名の 5 か所（`"Task"` を消す）、`ExprKind::Task` の腕（10741・10925・11443・11803 行）を消す。`Type::Task` を `Type::Once` に改名（仕様 3）。`TaskResultIsReference` の検査は `Once` の結果（常に `unit`）なので残してよい |
| `src/closures.rs` | `LambdaKind::Task` → `Once`、`"$task"` → `"$once"`、`TaskRun`・`TaskParallel*` の関数値の包み（436〜472 行）を消す |
| `src/ownership.rs` | `E::TaskRun \| TaskParallel \| TaskParallelResults` の腕を消す。`TaskScope` を参照する所を `RoutineScope` だけに |
| `src/polymorph.rs` | `Type::Task` の腕を `Type::Once` に（`map_type` 227・`visit` 288・`resolve` 381・`unify` 478・`head_constructor` 2775 など）。`"Task"` のクラス名の予約（1089 行）を消す。`captures`（4094 行）の `Type::Task` を `Once` に。`Task` を例に出す文を直す（`Async` を例に出す文は TZ13） |
| `src/higher_kinds.rs` | `Constructor::Task` を消す |
| `src/regions.rs`・`src/contracts.rs`・`src/api_diff.rs`・`src/docgen.rs`・`src/semantic.rs`・`src/formatter.rs`・`src/warnings.rs`・`src/type_arena.rs`・`src/guide_std.rs`・`src/hints.rs`・`src/kg/ids.rs`・`src/kg/layout.rs`・`src/kg/sites.rs`・`src/llvm_debug.rs`・`src/llvm_layout.rs`・`src/const_static.rs`・`src/constants.rs`・`src/call_specialization.rs`・`src/gpu.rs`・`src/coverage.rs`・`src/kg/facts.rs` | `Task` の腕を消すか `Once` に改名する（エージェントの調査で見つかった行。`grep -rnw "Task" src/` で全件） |
| `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/kg_capture.rs`・`src/diagram/concurrency.rs`・`src/diagram/calls.rs`・`src/parallel_report.rs`・`src/const_rules.rs`・`src/const_machine.rs`（`enum Task` は無関係。触らない）・`src/prove_smt.rs` | `Task*` の組み込みと `TaskRun*` の腕を消す。件数の pin を今の値から減らす（`Builtin::ALL.len()`、`by(Effect::Task)`、不純の数、`IMPURE_BUILTINS`）。`Effect::Task` の名前 `"task"` は効果の語彙（D-53）なので **残す**（`tzroutine`・`Parallel` の効果の名前として使い続ける） |
| `src/llvm.rs`・`src/llvm_task.rs`・`src/llvm_parallel.rs`・`src/llvm_sync.rs`・`src/llvm_traps.rs` | `parallel_tasks`・`llvm_task.rs` の全体（ファイルを消し、`mod` の宣言も）・`Task.scope` の生成・`tsuzuri_task_parallel*` と `tsuzuri_mutex_*` の宣言と、`@tz.mutex.parallel` の文字列の置換（1948〜1957 行）を消す。`globals.tzroutine` で分けていた `Parallel`・`Mutex` の生成を、ネイティブでは常に新しい方へ（Phase 1）。WASM の生成は変えない（仕様 2） |
| `src/driver.rs`・`src/test_runner.rs`・`src/runtime_cache.rs` | `task_runtime`・`task_runtime_source_for`・`task.c` の連結・`TZ_STACK_GUARD` の `task.c` の分・`--freestanding` の一覧の `Mutex`・`parallel tasks` の行（`tzroutine` の行に統合）を消す。翻訳単位の 9 つの部分の書式を変えるなら、事前ビルドのランタイム（`scripts/toolchain/runtime.mjs` の `task` の mask）も同時に直す（S5 の一部として `tests/prebuilt_runtime.mjs`） |
| `src/main.rs` | `--help` の文の `Task` の言及（`--wasm-feature threads` の説明など） |
| `src/lsp.rs` | `KEYWORDS` から `"task"`（配列の長さ） |
| `src/diagnostic_kinds.rs`・`src/explain/` | `E1040`（TZ03 が登録済み）に 3 つの kind を足す（`removed_task_expression`・`removed_task_type`・`removed_task_api`）。`src/explain/codes/E1040.md` に足す。出なくなった kind（`task_*`・`implicit_task_needs_builder`・`channel_item_holds_borrow` は Channel が使うので残る）は消さず、説明の文に「TZ12 で `Task` を削除したので出ない」と書く |
| `src/stdlib.rs` | 変更なし（`Task` は `RESERVED_MODULES` に入っていない。`is_reserved_module("Task")` が偽のテスト（669 行）はそのまま） |

### ランタイム（`src/runtime/`）

| ファイル | 処理 |
| --- | --- |
| `tzroutine.c`（DataChannel のランタイム） | `tsuzuri_channel_send(ptr block, ptr item)`・`tsuzuri_channel_recv(ptr block, ptr out)`・`tsuzuri_channel_clone_sender(ptr block)`・`tsuzuri_channel_close(ptr block, i32 kind)` を足す（`task.c` と同じ型と状態の値）。ブロックの 56 バイト目の予備の語を `tzr_lock`、64・72 バイト目の待ちの 2 語を受け手と送り手の待ち行列（`tzr_sudog *` の単方向リストの先頭）に使う（生成コードが 0 にしているので、そのまま空の行列）。`send`: ロック → 受け手が 0 なら 1 → 待つ受け手がいれば直接渡して起こす → 空きがあれば入れる → なければ送り手の行列で待つ（`tzr_park`。起きたら最初から見直す）。`recv`: 要素があれば取り、待つ送り手がいれば 1 人の要素をリングの末尾へ入れて起こす → 送り手が 0 なら 1 → なければ受け手の行列で待つ。`close`（kind 0 送り手・1 受け手）: その数を減らし、送り手が 0 になったら受け手を全員、受け手が 0 になったら送り手を全員起こす（それぞれ 1 を返す側へ）。所有者の数を減らし、0 なら 1（生成コードが残りを drop して解放）。擬似 G がデッドロックの判定で起こされたら 2。C のテスト: `tests/channel_runtime.c` を、`task.c` の代わりに `tzroutine.c` を含めて同じケースを検査する形に書き直す（手伝いとスレッド数の検査のケースは、新しい規則の検査（`TSUZURI_MAXPROCS=1` で完了する）に置き換える） |
| `task.c`・`task-windows.h` | 消す。`trap.c` が `task.c` の hook（`tsuzuri_trap_hooks.item`・`resume`）を前提にしている部分（`tsuzuri_boundary_item`・`tsuzuri_boundary_resume`）は、呼ぶ者がいなくなるので消す。`tsuzuri_sync_hooks.abandon` は仕様 2 の新しいロックの解放に付け替える |
| `sync.ll` | 消す（`@tz.mutex.parallel` の検査） |
| `task-wasm-threads.c` | `Task.parallel`・`Task.scope` の部分を消す。`Parallel`・`Mutex`・`Atomic` の部分は残す |
| `task-wasm.ll` | 残す（既定の WASM の逐次の `Parallel`）。`Task.parallel` の分だけ消す |
| `.gitignore`・`scripts/check-runtime-includes.sh` | 消したファイルの行 |

### std

| ファイル | 処理 |
| --- | --- |
| `std/Mutex.tz`・`std/Atomic.tz` の `///` | `Task.scope` の言及を `Routine.scope` と tzroutine に |

### テスト（エージェントの調査の 1a〜1f）

| 種類 | ファイル | 処理 |
| --- | --- | --- |
| DataChannel の C のテスト | `tests/channel_runtime.c`（`tests/tasks.mjs` が組み立てていた） | 書き直して（`tzroutine.c` を含める。設計の「DataChannel のランタイム」）、`tests/tzroutine_runtime.mjs` から組み立てて走らせる（`-O0`・`-O3`、P 1・2・4、TSan・ASan） |
| DataChannel の E2E | `features.mjs` の `data_channel`・`tests/fixtures/data_channel`（TZ03） | 値の期待はそのまま。デッドロックの判定の文と、スレッド数で完了しなかった形（あれば）の期待だけを、新しい規則に直す |
| Task 専用（消す） | `tests/tasks.rs`・`tests/tasks.mjs`・`tests/task_runtime.c`・`tests/task_stress.c`・`tests/sync_runtime.c`・`tests/trap_boundary_runtime.c` の Task の部分・`tests/fixtures/tasks`・`tests/conformance/TSK/TSK-001`〜`010`（旧 Channel のテストは TZ03 で処理済み） | 消す。ただし `tests/tasks.mjs` が検査していた「関数値と捕捉の深いケース」（`function_captures`・`partial_move`・`snapshot` など）は、`tzroutine { }` の形に写して `tests/tzroutine.mjs` へ移す（同じ期待値） |
| 書き換える（中身は残す） | `tests/concurrency.rs`（Atomic・Mutex・Arc・Send／Sync の部分は `Routine.scope` と tzroutine の形に）、`tests/rc.rs`（Arc を Task へ渡す例 → tzroutine へ）、`tests/computations.rs`（計算式の例の `task { }` → `Maybe`・`Result`・`IO` のビルダー）、`tests/diagram.rs` と `tests/golden/diagram/*`（並行の図の fixture の `Task.parallel`・`Task.scope` を tzroutine と `Routine.scope` に。golden を作り直す）、`tests/mutable_slices.rs`・`tests/matrix.rs`（`@tsuzuri_task_parallel` の IR の期待を `@tsuzuri_tz_parallel` に）、`tests/param_facts.rs`・`tests/pure_functions.rs`・`tests/local_recursion.rs`・`tests/diagnostic_v2.rs`・`tests/kg.rs`・`tests/lsp.rs`（`Task<i64>` の補完 → 別の型）・`tests/debugger.mjs`・`tests/debugger_static.mjs`（LLDB の `Task<i64>` の formatter の検査を消す。`scripts/lldb/tsuzuri_lldb.py` の `Task<` の formatter も）・`tests/trap_return.mjs`・`tests/trap_locations.rs`・`tests/trap_boundary.mjs`・`tests/stack_overflow.mjs`（`Task.parallel` のワーカーの溢れ → tzroutine の溢れ）・`tests/windows.rs`・`tests/windows_runtime.c`・`tests/lto.rs`・`tests/explain.rs`（`task_capture_retains_borrow` の kind の説明の検査は残る kind なので残す。文が変わったら直す） | 書き換え |
| 例の型として使っている（1c） | `tests/alias.rs`・`allocator.*`・`api_diff.rs`・`arena.rs`・`bindings.*`・`borrow_syntax.rs`・`closure_representation.rs`・`constants.rs`・`control.rs`・`coverage.rs`・`debug_info.rs`・`deriving.rs`・`diagnostics.rs`・`dyn_dispatch.rs`・`fast_backend.mjs`・`fixed_arrays.rs`・`generic_records.rs`・`gpu.rs`・`hash_map.rs`・`iteration_protocol.rs`・`json.rs`・`map_set.rs`・`matrix_views.rs`・`maybe_result.rs`・`modules.rs`（`std::Task`）・`os.mjs`・`parameter_conventions.rs`・`pgo.mjs`・`recursive_types.rs`・`regex.rs`・`size.mjs`・`slices.rs`・`stdlib.rs`・`storage.rs`・`tail_recursion.rs`・`targets.rs`・`test_runner.rs`・`union_types.rs`・`user_drop.rs`・`wasm64.mjs`・`tests/diagnostic_baseline/return-after-then/Main.tz` | `Task<T>` を「非 Copy の値」「Send の値」として使っている箇所を、同じ性質の別の型（非 Copy なら `Owned.Function<unit, i64>` か `Drop` を持つ利用者の record、`Send` で非 `Sync` なら `Owned.Function`、遅延の値なら関数値）に置き換える。期待（コード・数）は変えない（S3）。`return-after-then` のヒントの例（`src/hints.rs` 240〜258 行）は `Maybe` のビルダーの例へ |
| fixture を使う runner（1d） | `targets.mjs`・`windows.mjs`・`handoff_tools.mjs`・`audit.rs`（`examples/tasks`）・`small_allocator.mjs`・`bindings.mjs`・`examples.mjs`（期待 `21325334000`）・`prebuilt_runtime.mjs`・`size.mjs`（`PROJECTS` の `tasks`）・`compact_strings_modes.mjs`・`range_flags_audit.mjs`・`allocator.mjs`（freestanding の `tasks`） | `tests/fixtures/tasks` と `examples/tasks` を、同じ計算を tzroutine と `Parallel` で書いた `examples/tzroutines`（出力 `21325334000` を保つ）と `tests/fixtures/tzroutine` に置き換え、runner の参照を直す |
| `tests/features.mjs` | suite `concurrency`・`channel` を書き換え（Atomic・Mutex は tzroutine と `Routine.scope` の形）か削除、`parallel` などは期待値のまま。runner の `task.c` の連結（3015 行）を消す。`constStaticsWasmThreadsChecks` などの threads の検査は残す |

### ベンチマーク

| ファイル | 処理 |
| --- | --- |
| `benchmarks/tasks/Main.tz`・`benchmarks/run-tasks.mjs`・`benchmarks/run-tasks-wasm.mjs` | `Task.parallel` の部分を `Routine.scope` と `Parallel` に書き換える（同じ仕事の量）。指標の名前（`tasks`・`tasks-data`・`tasks-sweep`・`tasks-gap`）は残し、`docs/benchmarks.md` に「TZ12 から tzroutine のスケジューラーで測る」と書く |
| `benchmarks/cpp/Kernels.tz`（127〜144 行）と `run-cpp.mjs`・`run-control.mjs` | `cpp/task_*` の Tsuzuri の側を `Routine.scope` に。C++・Rust・C#・JS の側は変えない。`task.c` の連結を消す |
| `docs/benchmarks.md`・`docs/benchmarks-data/*.json` | Task プールの節（645 行・3210 行・4234 行）は、削除した実装の測定として「履歴」の見出しの下へ移すか消す（実装と違う数を現在のものとして書かない）。表の生成は `node scripts/gen-docs.mjs` |

### 文書・ガイド・ツール

| ファイル | 処理 |
| --- | --- |
| `_tsuzuri/language-reference/async-tasks-and-lazy/task.md` | 消す。`index.md` の目次からも |
| `built-in-types-and-modules/datachannel.md` | 「待ちの規則」の節を書き直す（手伝い・スレッド数・CPU 1 の表・`deadlock: every task is waiting on a channel` を消し、tzroutine だけが待つこと、TZ01 のデッドロックの文、ロックの中で待ってよいこと）。`Task.parallel` の例を tzroutine と `WaitGroup` に。使い分けの表（TZ03 の仕様 9）の「待ち」の行を直す |
| `async-tasks-and-lazy/tzroutine.md` | 「Task からの移行」の節（仕様 1 の表を、書き換えの例つきで）。Task との違いの表を消す |
| そのほかの言語リファレンス（エージェントの調査の表: `mutex.md`・`rc.md`・`atomic.md`・`parallel.md`・`webassembly.md`・`computation-expressions.md`・`option.md`・`native-interop.md`・`constraints.md`・`lazy.md`・`usage.md`・`result.md`・`functions.md`・`monoid.md`・`why-tsuzuri.md`・`strategy.md`・`lambda-expressions.md`・`keywords.md`・`statements.md`・`drop.md`・`ownership.md`・`modules.md`・`how-about-tsuzuri.md`・`types.md`・`matrix.md`・`attributes.md`・`stack-and-heap.md`・`diagrams.md`・`diagnostics.md`・`gpu.md` ほか） | `Task`・`task { }` の説明と例を、tzroutine・`Channel`・`Routine.scope` に書き換えるか消す。`<!-- rule: TSK-00x -->` の印は退役した規則なので消す |
| `docs/language.md`・`docs/architecture.md` | 「タスク」「並列区間と寿命」「共有状態と Task.scope」「実行バックエンドと失敗」の節（エージェントの調査の行）を、tzroutine の節へ統合する |
| `spec/rules/TSK.md` | `TSK-001`〜`010` の状態を `retired`（番号は再利用しない）。題を `Tasks and tzroutines` のまま |
| `guide/language.md`・`guide/ownership.md`・`guide/card.md`、`src/guide_std.rs` | `Task`・`Task.scope` の説明を tzroutine に。`node scripts/check-guide.mjs` |
| `_features/GUIDE-LITE.md` | §4 の手本の例 `Task` を `Routine` に。`node scripts/check-guide-lite.mjs` |
| `AGENTS.md` | 資源のハンドルの例外の一覧から `Task` を消し、`WaitGroup`・`Context` などを足す（`Channel` は同じ名前で残る） |
| `README.md` | 「タスクと並列処理」の節（341 行）を tzroutine に、予約モジュール名（493 行）、検証の表（`tests/tasks.mjs` の行） |
| `vsc/syntaxes/tsuzuri.tmLanguage.json`・`vsc/src/core.ts`・`vsc/src/editor.ts`・`vsc/src/workflow.ts`・`vsc/src/test/core.test.ts` | `task` の予約語、`Task` の型、文（「Task 以外」）を直す |
| `scripts/check-cli-docs.mjs`・`scripts/toolchain/smoke.mjs` | `Task.parallel` の煙の検査（108〜148 行）を tzroutine に |
| `.github/workflows/targets.yml` | 「Function values and tasks (PM02)」の手順を `node tests/tzroutine.mjs` と `node tests/tzroutine_runtime.mjs` に置き換え |

## 前提とする他チケットのインターフェース

- TZ02 の `tzroutine { }`、TZ03 の `Channel`、TZ06 の新しい `Mutex` と `held`、TZ07 の `ErrGroup`、TZ11 の `tsuzuri_tz_parallel` と `Routine.scope`。

## 他チケットへ提供するインターフェース

- TZ03 の `removed_feature` に Task の行を足した（TZ13 が `Async` の行を足す）。`tests/removed_features.rs` に Task の行。

## 実装手順

### Phase 1

1. **変更**: ブランチ `feat/tz12-remove-task`。基準と参照の数の控え。
2. **変更**: `Parallel.*`・`Mutex` の生成の分岐を、ネイティブでは常に新しい方へ。`trap.c` の `abandon` のフックを新しいロックへ。`DataChannel` の C の関数を `tzroutine.c` に置き、ネイティブの `DataChannel` のプログラムが `task.c` を連結しないようにする（ドライバーの `channel` の判定を `tzroutine_runtime` へ）。生成コードの `tsuzuri_mutex_wait_ok` の呼び出しを消す。**確認**: `node tests/features.mjs target/debug/tsuzuri parallel parallel_equivalence monoid pure_parallel matrix concurrency data_channel`（値の期待は変えない。変えてよいのは `data_channel` の「デッドロックの判定の文」と「スレッド数で完了しなかった形が完了する」ことだけ。S1）、`node tests/trap_return_sync.mjs`（Mutex と DataChannel の部分）、`node tests/tzroutine_runtime.mjs`、書き直した `tests/channel_runtime.c`（TSan・ASan も）。

### Phase 2

3. **変更**: 内部の改名（仕様 3）だけを先にする。**確認**: `cargo nextest run --locked`、`node tests/tzroutine.mjs`（S2）。
4. **変更**: 構文・型・組み込み・生成・ランタイムの Task の削除（表のコンパイラとランタイムの行）。**確認**: `cargo build`。
5. **変更**: `E1040` の `removed_task_*`（仕様 1）。**確認**: `tests/removed_features.rs`（TZ03 が作った）に、表の各行の入力が `E1040` と kind と help を出し、利用者が `Task` という名前のモジュール・型を定義したプログラムは通る（S4）。`task` を変数名に使える。
6. **変更**: 表の「例の型として使っている」テストと、`Task` を前提にしたテストの書き換え。**確認**: `cargo nextest run --locked`。

### Phase 3

7. **変更**: テストの runner・fixture・examples・ベンチマーク・文書・ガイド・VS Code・CI（表のとおり）。**確認**: `node tests/features.mjs target/debug/tsuzuri`（全部）、`node tests/examples.mjs target/debug/tsuzuri`、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/check-spec.mjs --require-markers`、`node scripts/conformance.mjs`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`、`node scripts/check-guide.mjs`、`node scripts/check-guide-lite.mjs`、`node scripts/repo-kb.mjs build`（警告が増えない）、`cd vsc && npm test`（`vscode.yml` の手順）。
8. **変更**: 早見の「完了の見分け方」の `grep`。T2。PR に `ci:full`。`gh run watch` で全ワークフロー（`ci.yml`・`targets.yml`・`windows.yml`・`vscode.yml`・`reference.yml`・`docs.yml`・`spec.yml`）が緑。

## テスト計画

- `tests/removed_features.rs`（TZ03 が作った）に: 仕様 1 の 6 行 × 「利用者の定義なし → `E1040`」「利用者の定義あり → 通る」、`--json` の `help` と `Task.scope` の `fixes`、`tsuzuri fix` が `Task.scope` を `Routine.scope` に直す（G24 の手順）。
- 既存の全 suite が、書き換えた fixture と期待値（意味の同じもの）で通る。

## ドキュメント

- 設計の表のとおり。移行の文書（`tzroutine.md` の節）には、Task の 4 つの使い方の書き換えの例を載せる: ① `Task.run (task { … })` → その場で実行、② `Task.parallel` の結果の配列 → `Parallel.init` か、tzroutine + `WaitGroup` + `Mutex<Vec<T>>`、③ `Task.parallel_results` → `ErrGroup`（最初の誤りの選び方が「最小の添字」から「完了の順」に変わる）、④ `Task.scope` → `Routine.scope`（引数は同じ。子が待てる）。旧 `Channel` からの移行は TZ03 が `channel.md` に書いた。
- `compiler/diagnostics.md`（`E1040` の行。生成の表は `gen-docs`）、`docs/language.md`（診断の表）。

## 受け入れ条件

- [ ] 早見の `grep` が 0 件（例外の場所を除く）。
- [ ] 仕様 1〜3 のとおり。`E1040` のテストが通る。
- [ ] 全ワークフローが緑。`Parallel.*` の結果が変わらない。
- [ ] 文書・ガイド・規則 ID・VS Code を更新した。

## 落とし穴

- **kind を消さない**: `TaskRequiresSend` などの kind は tzroutine と `Channel` が使い続ける。消すと G23 の登録簿のテストが落ちる（D-51）。
- **codec の欠番**: tag を詰めると、古い frontend のキャッシュ（`FRONTEND_FORMAT` を上げるので読まれないが）だけでなく、tag の表のテストの意味が変わる。詰めない。
- **`Effect::Task` は残す**: 効果の語彙 `task`（D-53）は KG・`@no_*`・`pure` の検査の共通の名前。Task を消しても、`tzroutine`・`Parallel` の効果として残す。
- **`$task` の改名と KG の golden**: `kg` の golden に `$task` が出ていれば作り直す（`$once`）。出ていなければ変わらない。
- **`examples/tasks` の出力**: 期待 `21325334000` は計算の結果なので、tzroutine と `Parallel` で同じ計算をすれば同じ値になる。値を写し直さない。

## 対象外

- `Async` の削除（TZ13）。WASM threads のワーカーのプールの tzroutine への移行（WASM の tzroutine は JSPI。TZ09）。
- `tsuzuri fix` による Task の自動の書き換え（`Task.scope` の置換だけを付ける。ほかは意味が変わるので手で直す。D5）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 即削除し、`E1040` で移行先を示す | D-61 の決定 4 | 承認済み |
| D2 | `task` は予約語でなくなる（識別子として使える） | 予約語を減らす。`task { }` を書いた場合は `E1040` | 既定案 |
| D3 | 1 回だけの関数の内部の型は `Type::Once` として残す | tzroutine の捕捉の規則の実装を保つ | 既定案 |
| D4 | ネイティブの `Parallel`・`Mutex` は全プログラムで tzroutine のランタイム | 1 つのプロセスに 1 つのスレッドの集まり | 既定案 |
| D5 | `tsuzuri fix` の置換は `Task.scope` → `Routine.scope` だけ | ほかは意味が変わる（結果の順・誤りの選び方・閉じ方） | 既定案 |
| D6 | 規則 `TSK-001`〜`010` は `retired` | `spec/README.md` の規則 | 既定案 |
| D7 | ベンチマークの指標の名前は残し、実装を tzroutine にする | 時系列の比較を保つ（`docs/benchmarks.md` に切り替わりを書く） | 既定案 |
| D8 | WASM の `Parallel`・`Mutex`・`Atomic` の実装は変えない | WASM の tzroutine は JSPI だけで、threads のワーカーとは別 | 既定案 |
