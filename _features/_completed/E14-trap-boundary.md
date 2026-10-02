# E14: 埋め込み時のトラップ境界とスタック枯渇の報告

| 項目 | 内容 |
| --- | --- |
| ID | E14 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | G04, E05, (B06) |
| 後続 | F13 Phase 2 |
| 状態 | done（Phase 1。Phase 2・3 は要承認で未着手） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1 は不要。要承認: D6（Phase 2: native object の setjmp/longjmp 境界と `--trap-mode return`）, D7（Phase 2: 確保 list と worker の境界）, D9（Phase 3: signal handler によるスタック枯渇の報告） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: native ホストに組み込んだ関数のトラップでホストのプロセス全体が終了し、スタック枯渇は理由なしに異常終了する |
| 手本にする既存実装 | 同梱 JS ホスト: `src/runtime/wasm-threads.mjs` の `createThreadPool`（失敗後は `closed` で呼び出しを拒否する）。トラップごとに新しい instance を作る形: `tests/tasks.mjs` の `isolated`、`tests/features.mjs` の `fresh`。trap 表との照合: `tests/trap_locations.rs` の `trap_aware_ir_executes_on_native_and_wasm_at_both_levels`。子プロセスの終了状態の報告: `src/driver.rs` の `run`、`src/test_runner.rs` の `execute_test` |
| 主な影響ファイル | Phase 1: `src/runtime/trap-boundary.mjs`（新規）, `src/driver.rs`（`run`）, `src/test_runner.rs`（`execute_test`）, `tests/trap_locations.rs`, `tests/trap_boundary.mjs`（新規）, `tests/fixtures/trap_boundary/Main.tz`（新規）, `tests/fixtures/trap_boundary_host/Main.tz`（新規）, `README.md`, `docs/language.md`, `docs/architecture.md`, `_docs/tools/debugging.md`, `_docs/guides/webassembly.md`, `_docs/guides/native-interop.md`, `_docs/feature-status.md`, `_features/README.md`。Phase 2・3（要承認）: `src/main.rs`, `src/driver.rs`, `src/llvm.rs`（`emit_with_trap_info`）, `src/llvm_traps.rs`（`instrument`）, `src/llvm_abi.rs`, `src/trap.rs`, `src/runtime/trap.c`（新規）, `src/runtime/task.c`, `tests/trap_boundary_runtime.c`（新規） |

## 目的

C/C++ のサーバー、デスクトップアプリ、Node・ブラウザーへ Tsuzuri の計算を組み込んだとき、一回の呼び出しのトラップでホスト全体を失わない方法を用意する。
Tsuzuri のトラップは巻き戻さない（GUIDE D-10）。したがって境界は「トラップした実行単位を丸ごと捨てる隔離」として作り、トラップはホスト側で値（成功／失敗）として受け取る。
言語の意味（トラップを `Result` に変換しない、drop を走らせない）は変えない。Rust の `catch_unwind`、C# の例外に相当する用途を、例外なしで満たす。
あわせて、スタック枯渇を理由付きで報告する。

- Phase 1（承認不要・実装対象）: WASM の export 呼び出し境界（同梱 JS ホスト `createBoundary`（新規））と、`tsuzuri run`／`tsuzuri test` でのスタック枯渇の報告。コンパイラの出力は変えない。
- Phase 2（要承認）: native object の呼び出し境界（`--trap-mode return` と `tsuzuri_try_<name>`（新規））。
- Phase 3（要承認）: native 実行ファイル自身によるスタック枯渇の報告（signal handler）。

実装者は Phase 1 だけを実装する。Phase 2・3 は承認後に人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- G04・E05・B06 が `_features/README.md` の状態欄で done であること（HEAD では 3 件とも done）。
  確認: `grep -nE "^\| (G04|E05|B06) \|" _features/README.md`。
- Phase 1 は承認不要。Phase 2 は D6・D7、Phase 3 は D9 の承認前に着手しない。
- GUIDE §2.3 の基準コマンドと、手順 1 の既存 suite が成功していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- Phase 1 でコンパイラの出力（IR、object、`.wasm`、header、`.trap.json`）を変える必要が出た。Phase 1 が変えるのは driver の診断 message と JS ホストだけである。
- signal handler（`sigaction`・`sigaltstack`）、`setjmp`／`longjmp`、`fork`、Rust の `unsafe`、新しい crate、既定の WASM import が必要になった（Phase 2・3 の承認事項）。
- 既存テストの期待値を変える必要がある。`E2005` の既存 message は、SIGSEGV／SIGBUS で終了した場合を除いて一字も変えない。
- Node でのスタック枯渇が `RangeError` でも `WebAssembly.RuntimeError` でもない例外になった（D3 の分類が成り立たない）。
- トラップ後の instance を再利用しないと成り立たない利用例・既存テストが見つかった（D2 に反する）。
- トラップ後の instance で `tsuzuri_trap_site` 以外の export を呼ぶ必要が出た。
- signal 番号を得るのに `std::os::unix::process::ExitStatusExt::signal` 以外（`libc` crate など）が要る。

## 現状（HEAD `f8dc655` で確認）

- トラップは `call void @llvm.trap()`。native は AArch64 の `brk`（SIGTRAP）、x86-64 の `ud2`（SIGILL）、WASM は `unreachable` になる。
  `src/trap.rs` の `TrapKind` は 14 種（`TrapKind::ALL`）で、`description` が理由文字列（`"assertion failed"` など）を返す。
- `--trap-info` では `src/llvm.rs` の `emit_with_trap_info` が `src/llvm_traps.rs` の `instrument` を呼び、`@llvm.trap` の直前へ
  `call void @tz.trap.report(i32 site)` を入れる。native の `@tz.trap.report` は site ごとの message を `@tz.trap.write` で stderr へ書く。
  WASM は `@tz.trap.latest` へ保存し、`define i32 @tsuzuri_trap_site()` を `--export=tsuzuri_trap_site`（`src/driver.rs`）で公開する。
  表は `src/trap.rs` が `{"version":1,"sites":[{"id":…,"kind":…,"path":…,"span":{"start","end","line","column","end_line","end_column"}}]}` の形で書く。
- 巻き戻し・`setjmp`／`longjmp`・signal handler はない（`src/`・`tests/` に `sigaction`・`sigaltstack`・`setjmp`・`longjmp` の使用なし）。
- native のタスク: `src/runtime/task.c` の `tz_task_execute` は `run_result` の戻り値 1（`Error`）だけを失敗として扱う。
  callback 内のトラップは worker thread 上で `@llvm.trap` を実行し、signal の既定動作でプロセス全体が終わる（B06「trap / abort」）。
  pthread の失敗は `tz_task_fail` が stderr へ書いて `abort()` する。つまり worker thread ではトラップを隔離できない。
- `tsuzuri run`（`src/driver.rs` の `run`）は `trap_info: true` で実行ファイルを作り、子プロセスとして起動して stderr を中継する。
  失敗時は stderr に site の message があればその位置、なければ既定の span（1:1）で
  `program terminated with {status}; integer division, indexing, assert, or allocation may have trapped` を `E2005` で報告する。
  スタック枯渇でもこの message になり、理由が分からない（再現参照）。
- `tsuzuri test`（`src/test_runner.rs` の `execute_test`）は signal による終了を `"trapped or terminated by signal"` とだけ報告する。
- WASM: ホストは `WebAssembly.RuntimeError` を捕捉できる。既存テストはトラップごとに新しい instance を作る（`tests/tasks.mjs`、`tests/features.mjs`、
  `tests/strings.mjs` など）。トラップ後の heap・shadow stack（`__stack_pointer`）の整合は保証しない。
  WASM の link は `--stack-first` と `-z stack-size=1048576`（`src/driver.rs`）で、shadow stack の溢れは address 0 の下へ出る。
- WASM threads: `createThreadPool` は worker の失敗で pool を失敗状態にし、以後の `call` を `"WASM thread pool is closed or failed"` で拒否する。
  docs/language.md「タスク」は「trap後のpoolは再利用せずclose」とする。
- extern（E06）の WASM import は module 名 `tsuzuri`（`tests/host_imports.mjs`）。
- D-10: 例外・巻き戻しは導入しない。docs/language.md「タスク」は「失敗時のスタック巻き戻し・捕捉値の解放・兄弟タスクのキャンセルは保証しません」とする。

### 再現（検証済み）

`/tmp/tz-work-E14/deep/Main.tz`（`check` 成功、`deep 10` は 44281）:

```tsuzuri
def rec depth :: i64 -> i64
fn rec depth n = if n == 0 then 0 else depth (n - 1) * 3 + n

export def deep :: i64 -> i64
fn deep n = depth n

def main :: i64 = deep 100000000
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri run /tmp/tz-work-E14/deep -O0   # -O3 でも同じ
target/release/tsuzuri build /tmp/tz-work-E14/deep --target wasm32 --trap-info -O3 -o /tmp/tz-work-E14/deep.wasm
```

```text
error[E2005]: program terminated with signal: 11 (SIGSEGV); integer division, indexing, assert, or allocation may have trapped
```

Node 20.19.6 で `tz_deep(100000000n)` は `-O0`・`-O3` とも `RangeError: Maximum call stack size exceeded` を投げ、`tsuzuri_trap_site()` は 0 を返した。
同じ instance の `tz_deep(10n)` はその後も 44281 を返したが、これは整合の保証ではない（D4）。WASM の import は空だった。

## 仕様

### 前提とする他チケットのインターフェース

- G04（done）: `.trap.json` の `version` 1 の形と、`--trap-info` 付き WASM の export `tsuzuri_trap_site`（最後のトラップの site ID、未発生なら 0）。
- E05（done）: 公開 ABI の out descriptor と `tsuzuri_alloc`／`tsuzuri_free`。Phase 1 は形を変えない。
- B06（done）: `tsuzuri_task_parallel_results` の runtime ABI。Phase 2 の worker 境界（D7）だけが使う。
- 提供（E13）: E13 の生成 glue の `load` は、export 呼び出しを `createBoundary` の `call` に通すか、D2–D4 と同じ規則（捨てる・作り直す・分類）を実装する。
  E13 はトラップを `TsuzuriTrap` として throw する設計のままでよく、その中身は D3 の `trap` の値にする。
- 提供（B08）: ホスト駆動の実行器は、再開中のトラップで instance が捨てられたら、その instance の保留中の Async 値をすべて破棄し、再開しない。
- 提供（F13 Phase 2）: Phase 2 の確保 list（D7）が確保統計の差し込み口になる。

### 構文・型規則

変更しない。言語内でトラップを捕捉する API（例: `Trap.isolate :: (unit -> 'a) -> Result<'a, TrapInfo>`）は作らない（D1）。

### WASM の境界（Phase 1）

新 API（実装後に有効。未検証）。`src/runtime/trap-boundary.mjs`（新規）は `node:` の import を持たない ES module で、次の一つだけを export する。

```javascript
// module: コンパイル済み WebAssembly.Module。imports: WebAssembly.Instance へ渡す import object（extern は namespace "tsuzuri"）。
// sites: `<output>.trap.json` を JSON.parse した値の `sites`（省略可）。
export function createBoundary(module, { imports = {}, sites = [] } = {}) {
  // return { call(name, ...args), get exports() }
}
// call の戻り値:
//   { ok: true, value }
//   { ok: false, trap: { reason: "trap" | "stack", site, kind?, path?, line?, column? } }
```

- 境界は export 呼び出し 1 回である。`call("tz_add", 1n, 2n)` のように、公開シンボル名（`tz_` 付き）と引数をそのまま渡す。
- instance は最初の `call` または `exports` の参照で作る（`new WebAssembly.Instance(module, 包んだ imports)`）。
- 例外が export 呼び出しから出たら、その instance を捨てる（poison、D2）。古い instance へは、分類のための `tsuzuri_trap_site()` の 1 回を除いて二度と触れない。
  次の `call`／`exports` で同じ module と imports から新しい instance を作る。
- `exports` は現在の instance の export を返す。ホストは buffer の確保（`tsuzuri_alloc`）と `memory` の読み書きにこれを使い、`call` の後に取り直す。
- `--wasm-feature threads` の module（import に namespace `tsuzuri_threads` を持つ）は拒否する。threads の境界は既存の `createThreadPool` の pool である（D2）。

### 分類と報告の値

| export 呼び出しから出た例外 | 条件 | `call` の結果 | instance |
| --- | --- | --- | --- |
| import 関数（extern のホスト実装）が投げた例外 | 包んだ import が同じ例外を記録した | 同じ例外を再送出する | 捨てる |
| `WebAssembly.RuntimeError` | 上以外 | `{ ok: false, trap: { reason: "trap", site } }`。`site` は `tsuzuri_trap_site?.() ?? 0` | 捨てる |
| `RangeError`、または `name === "InternalError"` | 上以外 | `{ ok: false, trap: { reason: "stack", site: 0 } }` | 捨てる |
| その他（引数変換の `TypeError` など） | 上以外 | 同じ例外を再送出する | 捨てる |

- `site` が 0 でなく `sites` に同じ `id` があれば、`kind`（`TrapKind::description` の文字列）、`path`、`line`、`column`（`span.line`・`span.column`）を足す。
- `site` 0 の `reason: "trap"` は、`--trap-info` なしの build、または Tsuzuri の site を持たないエンジンのトラップ（shadow stack の溢れによる範囲外アクセスを想定。未検証）を表す。
- 呼び出す前の検査は instance を捨てない: 未知の名前は `TypeError`（`unknown export {name}`）を投げる。

### 数値・トラップ・native と WASM の差

| 失敗 | WASM（Phase 1） | native `run`／`test`（Phase 1） | native object（Phase 2、要承認） |
| --- | --- | --- | --- |
| `TrapKind` の 15 種（生成 IR、数値 runtime、WASM の stack 検査の `@llvm.trap`） | `reason: "trap"` と site | 変更なし（既存の site の報告） | status 1 と `tsuzuri_trap_info` |
| スタック枯渇 | `reason: "stack"`（V8 は `RangeError`、検証済み） | SIGSEGV／SIGBUS の推定 message（D5） | 対象外（プロセス終了のまま。Phase 3） |
| エンジンのトラップ（`TrapKind::WasmRuntime` の site を持たないもの） | `reason: "trap"`、site 0 | — | — |
| ホスト関数（extern・import）の失敗 | 再送出して instance を捨てる | 対象外 | 対象外（ホストのフレームは跨がない） |
| C runtime の `abort()`（`tz_task_fail` など） | — | 変更なし | 対象外（プロセス終了のまま） |

数値の意味（overflow、NaN、丸め）とトラップの条件は変えない。トラップを値にするのはホスト側であり、Tsuzuri のコードには戻らない。

### 資源と確保の方針

- WASM: トラップした instance の linear memory・heap・`__stack_pointer` はまとめて捨てる。個々の確保は回収も drop もしない。
  ホストが古い instance から得た pointer・`memory.buffer` の view・未解放の所有結果はすべて無効になる。所有結果は次の `call` の前に copy して `tsuzuri_free` する。
  トラップした呼び出しの `live == 0` は検査しない（検査できない）。新しい instance の成功呼び出しが正しい結果を返すことで確認する。
- native `run`／`test`: 子プロセスの終了で OS が回収する（変更なし）。
- native Phase 2（要承認）: 境界内の確保を確保 list（D7）へ載せ、トラップ時に drop を走らせず一括で `free` する。成功時は list を空にする。
  トラップした呼び出しの後も `live == 0` を満たす。借用入力はホストの所有なので触らない。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E2005` | `run` の子プロセスが SIGSEGV／SIGBUS で終わり、stderr に site の message がない | `program terminated with {status}; the stack was probably exhausted by deep recursion; reduce the recursion depth or use a loop` | 既定の span（1:1。既存と同じ） |
| `E2005` | それ以外の異常終了 | 既存の message のまま | 既存のまま |
| （テスト失敗の理由） | `tsuzuri test` の子プロセスが SIGSEGV／SIGBUS で終わった | `terminated by signal {n}; the stack was probably exhausted by deep recursion` | 既存のテスト失敗と同じ |
| JS `Error` | `createBoundary` に threads の module を渡した | `createBoundary does not support modules built with --wasm-feature threads; use createThreadPool` | — |
| JS `TypeError` | `call` の名前が関数の export でない | `unknown export {name}` | — |

`{status}` は既存と同じ `ExitStatus` の表示（例: `signal: 11 (SIGSEGV)`）、`{n}` は signal 番号である。

### 例

新 API（実装後に有効。未検証）。fixture `tests/fixtures/trap_boundary/Main.tz`（新規）を `--trap-info` 付きで build した module を使う。

```javascript
import { readFileSync } from "node:fs";
import { createBoundary } from "../src/runtime/trap-boundary.mjs";
const module = new WebAssembly.Module(readFileSync(wasm));
const { sites } = JSON.parse(readFileSync(`${wasm}.trap.json`, "utf8"));
const boundary = createBoundary(module, { sites });
boundary.call("tz_div", 7n, 2n);      // { ok: true, value: 3n }
boundary.call("tz_div", 7n, 0n);      // { ok: false, trap: { reason: "trap", site: <id>, kind: "integer division by zero", path, line, column } }
boundary.call("tz_deep", 100000000n); // { ok: false, trap: { reason: "stack", site: 0 } }
boundary.call("tz_div", 9n, 3n);      // 新しい instance で { ok: true, value: 3n }
```

### Phase 2（設計方針・要承認）

- CLI: `--trap-mode return`（D6）は native の `build --emit object` と `--emit llvm` だけで受け、`--trap-info` を含意する。それ以外の組み合わせは
  `E2000`（`--trap-mode return is only valid for native object or llvm output`）。
- ABI: 各 `export def name` に `int32_t tsuzuri_try_<name>(tsuzuri_trap_info *trap, <結果の out>, <tz_name と同じ引数>)`（新規）を足す。
  結果の out は、`tz_name` が値を返すなら `T *result`、既に先頭 `out` を持つ buffer／record 結果ならその `out`、`unit` ならなし。
  戻り値は 0（成功）、1（トラップ。`*trap` を書く）、2（同じ thread で境界が入れ子。何も実行しない）。`tz_name` の ABI と挙動は変えない。
- `typedef struct { uint32_t site; uint32_t kind; } tsuzuri_trap_info;`（新規）。`kind` は `TrapKind` の `repr(u32)` の値、`site` は表の `id`。
- 名前を `tz_try_<name>` にしないのは、`export def try_name` の `tz_try_name` と衝突するためである（D10）。

### Phase 3（設計方針・要承認）

- Tsuzuri が作る native 実行ファイル（`build --emit exe`、`run`、`test`）だけで、runtime が `sigaltstack` と SIGSEGV／SIGBUS の handler を登録する（D9）。
  故障アドレスが主 thread・worker の guard 範囲なら `write(2, "stack overflow\n", 15)` の後に `abort()` し、範囲外は既定動作へ戻して再送する。
- 既存の `TrapKind::StackOverflow`（WASM の入口検査で追加済み）を native でも使い、`run` は `E2005` で `stack overflow` を報告する。object・ライブラリ出力はホストの signal 設定を変えない。

## 設計

### データ構造

Phase 1 のコンパイラ側の変更は、終了状態を分類する小さな関数だけである。

```rust
// src/driver.rs（新規）。unix 以外は常に false。
pub(crate) fn probable_stack_exhaustion(status: &std::process::ExitStatus) -> bool;
// SIGSEGV は 11。SIGBUS は Linux で 7、macOS・BSD で 10。libc crate は使わず cfg(target_os) の定数で持つ。
```

JS 側の状態は closure の変数 3 つ（`instance`、`hostError`、包んだ `imports`）だけにする。class は作らない。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| driver | `src/driver.rs` | `probable_stack_exhaustion`（新規） | `#[cfg(unix)]` で `std::os::unix::process::ExitStatusExt::signal` を見る。SIGSEGV と SIGBUS なら true |
| driver | `src/driver.rs` | `run` | 既定 message を作る分岐（`!json \|\| stderr.trim().is_empty()`）で、site が見つからず `probable_stack_exhaustion` なら D5 の message にする。span の求め方・`--json` で stderr を含める分岐は変えない |
| test | `src/test_runner.rs` | `execute_test` | `result.code()` が `None` の分岐で、`probable_stack_exhaustion` なら `terminated by signal {n}; the stack was probably exhausted by deep recursion`、それ以外は既存の `"trapped or terminated by signal"` |
| JS ホスト | `src/runtime/trap-boundary.mjs`（新規） | `createBoundary`（新規） | D2–D4。約 50 行。`wasm-threads.mjs` と同じく `src/runtime/` に置き、Rust からは参照しない（`include_str!` しない） |
| 生成 IR・runtime | `src/llvm*.rs`、`src/runtime/*.c`・`*.ll` | — | Phase 1 では変更なし（手順 6 で IR の不変を確かめる） |
| Phase 2（要承認） | `src/main.rs`、`src/driver.rs` | 引数解析、`BuildOptions` | `--trap-mode return`（新規）。`trap_info` を含意し、組み合わせの検査は `--trap-info` の検査の隣に置く |
| Phase 2（要承認） | `src/llvm_traps.rs` | `instrument` | trap mode では `call void @llvm.trap()` を `call void @tsuzuri_trap_raise(i32 site, i32 kind)` に置き換える（後続の `unreachable` は残す） |
| Phase 2（要承認） | `src/llvm_abi.rs` | header と wrapper の生成 | `tsuzuri_try_<name>` の thunk と header の prototype、`tsuzuri_trap_info` の typedef |
| Phase 2（要承認） | `src/runtime/trap.c`（新規） | `tsuzuri_boundary_run`、`tsuzuri_trap_raise`、確保 list | `setjmp` は C の中だけで行う（D6） |
| Phase 2（要承認） | `src/runtime/task.c` | `tz_task_execute`、`tz_task_submit`、`struct tz_task_group` | worker 境界と group のトラップ記録（D7） |
| Phase 3（要承認） | `src/runtime/`、`src/trap.rs` | signal handler（新規）、既存の `TrapKind::StackOverflow` | D9 |

### 生成 IR とランタイム

Phase 1 は生成 IR・object・`.wasm`・header・`.trap.json` を 1 byte も変えない。runtime の C と `.ll` も変えない。

Phase 2（要承認）の形は次のとおり。`setjmp` を IR に出さず、C runtime の一か所に閉じ込める（`returns_twice` と `jmp_buf` の大きさを IR へ持ち込まない）。

```c
/* src/runtime/trap.c（新規、Phase 2） */
int32_t tsuzuri_boundary_run(void (*thunk)(void *), void *frame, tsuzuri_trap_info *trap);
_Noreturn void tsuzuri_trap_raise(uint32_t site, uint32_t kind); /* 境界がなければ __builtin_trap() */
```

生成する `tsuzuri_try_<name>` は、引数と結果の置き場所を持つ frame を stack に作り、`tz_name` を呼んで結果を frame へ書く thunk を渡して
`tsuzuri_boundary_run` を呼ぶ。成功経路の追加は境界の出入りだけで、`tz_name` の中身には呼び出しを足さない。

### アルゴリズム

`call(name, ...args)`（Phase 1）:

```text
exports := current instance（なければ new WebAssembly.Instance(module, wrapped)）
if typeof exports[name] != "function": throw TypeError("unknown export " + name)   # instance は捨てない
hostError := null
try: return { ok: true, value: exports[name](...args) }
catch error:
  instance := null                               # 以後の call は新しい instance を作る
  if error === hostError: throw error            # ホストの失敗はホストへ返す
  if error instanceof WebAssembly.RuntimeError:
    site := exports.tsuzuri_trap_site?.() ?? 0   # 古い instance に触れる唯一の操作
    return { ok: false, trap: describe("trap", site, sites) }
  if error instanceof RangeError or error.name == "InternalError":
    return { ok: false, trap: { reason: "stack", site: 0 } }
  throw error
```

包んだ imports: 各 namespace の各関数 `f` を `(...a) => { try { return f(...a); } catch (e) { hostError = e; throw e; } }` にする。関数でない値（`memory` など）はそのまま渡す。
`describe` は `site` が 0 でなければ `sites.find(s => s.id === site)` から `kind`・`path`・`span.line`・`span.column` を写す。

## 実装手順

Phase 1 だけを実装する。各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」の sample を `/tmp/tz-e14/deep/Main.tz` に置き、生成物を保存する（手順 7 で比較する）。
- 確認: 次がすべて成功し、`trap_locations` は `7 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
cargo test --locked --test trap_locations
node tests/tasks.mjs target/release/tsuzuri
node tests/wasm_threads.mjs target/release/tsuzuri
target/release/tsuzuri build /tmp/tz-e14/deep --emit llvm -O3 -o /tmp/tz-e14/before.ll
target/release/tsuzuri build /tmp/tz-e14/deep --emit object -O0 -o /tmp/tz-e14/before.o
target/release/tsuzuri build /tmp/tz-e14/deep --target wasm32 --trap-info -O3 -o /tmp/tz-e14/before.wasm
```

### 手順 2: 終了状態の分類

- 変更: `src/driver.rs` の `probable_stack_exhaustion`（新規）と `mod tests`。
- 内容: 設計「データ構造」の関数を足す。`#[cfg(unix)]` の本体は `status.signal()` が SIGSEGV（11）か、`cfg(target_os = "linux")` で 7、
  `cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))` で 10 のとき true。それ以外の OS は false。
  テスト `probable_stack_exhaustion_matches_segv_and_bus`（新規）を足す（テスト計画）。
- 確認: `cargo test --locked --lib probable_stack_exhaustion` が `1 passed`。

### 手順 3: `tsuzuri run` の message

- 変更: `src/driver.rs` の `run`、`tests/trap_locations.rs`。
- 内容: 既定 message の分岐で、site が見つからず（`span` を `unwrap_or_default` で得た場合と同じ条件）`probable_stack_exhaustion(&status)` なら、診断表の
  message を使う。それ以外の文字列・span・`--json` の分岐は一字も変えない。テスト `run_reports_probable_stack_exhaustion`（新規）を足す。
- 確認: `cargo test --locked --test trap_locations` が `8 passed`。

### 手順 4: `tsuzuri test` の失敗理由

- 変更: `src/test_runner.rs` の `execute_test` と `mod tests`。
- 内容: `result.code()` が `None` の分岐だけを変える。テスト `execute_test_names_stack_exhaustion_signals`（新規）を足す。
- 確認: `cargo test --locked --lib execute_test_names_stack_exhaustion_signals` が `1 passed`。`cargo test --locked --lib test_runner` が成功する。

### 手順 5: JS ホスト `createBoundary`

- 変更: `src/runtime/trap-boundary.mjs`（新規）。
- 内容: 仕様「WASM の境界」「分類と報告の値」と設計「アルゴリズム」のとおりに書く。約 40 行。`node:` の import、class、非同期 API は使わない。
  古い instance への参照は `call` の中の局所変数 `exports` だけに持ち、catch の後で `tsuzuri_trap_site` 以外に触れない。
- 確認: `node --check src/runtime/trap-boundary.mjs` が成功し、`grep -n "node:" src/runtime/trap-boundary.mjs` が何も出さない。

### 手順 6: E2E suite

- 変更: `tests/fixtures/trap_boundary/Main.tz`（新規）、`tests/fixtures/trap_boundary_host/Main.tz`（新規）、`tests/trap_boundary.mjs`（新規）。
- 内容: fixture は下の検証済みソースをそのまま使う。harness は `tests/host_imports.mjs` の形（`execute`、`mkdtempSync`、`finally` で `rmSync`）を写し、
  テスト計画「E2E」の全 case を `-O0`・`-O3` で実行する。期待値は harness 内の BigInt 計算（`depth` の漸化式、`/` の切り捨て）と fixture の行・列から求める。
- 確認: `node tests/trap_boundary.mjs target/release/tsuzuri` が成功し、`Trap boundary -O0: 17 cases` と `Trap boundary -O3: 17 cases` を出す。

`tests/fixtures/trap_boundary/Main.tz`（`/tmp/tz-work-E14/boundary` で `check` 成功）:

```tsuzuri
export def div :: i64 -> i64 -> i64
fn div a b = a / b

def rec depth :: i64 -> i64
fn rec depth n = if n == 0 then 0 else depth (n - 1) * 3 + n

export def deep :: i64 -> i64
fn deep n = depth n
```

`tests/fixtures/trap_boundary_host/Main.tz`（`/tmp/tz-work-E14/host` で `check` 成功。WASM の import は `tsuzuri`／`Main.fail_host`）:

```tsuzuri
extern def fail_host :: i64 -> i64

export def call_host :: i64 -> i64
fn call_host n = fail_host n + 1
```

### 手順 7: 出力の不変

- 変更: なし。
- 内容: release を build し直し、手順 1 と同じ 3 つを `after.*` として出して比較する。`.trap.json` も比較する。
- 確認: 次の `cmp` がすべて何も出さずに成功する。

```sh
cargo build --release --locked
target/release/tsuzuri build /tmp/tz-e14/deep --emit llvm -O3 -o /tmp/tz-e14/after.ll
target/release/tsuzuri build /tmp/tz-e14/deep --emit object -O0 -o /tmp/tz-e14/after.o
target/release/tsuzuri build /tmp/tz-e14/deep --target wasm32 --trap-info -O3 -o /tmp/tz-e14/after.wasm
cmp /tmp/tz-e14/before.ll /tmp/tz-e14/after.ll && cmp /tmp/tz-e14/before.o /tmp/tz-e14/after.o
cmp /tmp/tz-e14/before.wasm /tmp/tz-e14/after.wasm && cmp /tmp/tz-e14/before.wasm.trap.json /tmp/tz-e14/after.wasm.trap.json
```

### 手順 8: 文書

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md docs/architecture.md _docs/tools/debugging.md _docs/guides/webassembly.md _docs/guides/native-interop.md _docs/feature-status.md` と
  `git diff --check` が成功する。

### 手順 9: 最終確認

- 変更: なし。
- 確認: `cargo fmt --check`、`cargo test --locked`、`node tests/trap_boundary.mjs target/release/tsuzuri`、`node tests/tasks.mjs target/release/tsuzuri`、
  `node tests/wasm_threads.mjs target/release/tsuzuri`、`node tests/host_imports.mjs target/release/tsuzuri` がすべて成功する。GUIDE §10 を満たす。

## テスト計画

### Rust テスト

- `src/driver.rs` の `tests::probable_stack_exhaustion_matches_segv_and_bus`（新規、`#[cfg(unix)]`）: `ExitStatusExt::from_raw` の raw wait status で、
  `from_raw(11)` と SIGBUS（Linux 7、macOS 10）は true。`from_raw(5)`（SIGTRAP）、`from_raw(4)`（SIGILL）、`from_raw(6)`（SIGABRT）、`from_raw(0)`、
  `from_raw(1 << 8)`（exit code 1）は false。
- `src/test_runner.rs` の `tests::execute_test_names_stack_exhaustion_signals`（新規、`#[cfg(unix)]`）: `Runner { program: "/bin/sh", arguments: ["-c", "kill -SEGV $$"] }`
  の `execute_test(&runner, 0, Duration::from_secs(5))` が `Ok(Some("terminated by signal 11; the stack was probably exhausted by deep recursion"))`。
  `kill -TRAP $$` では既存の `"trapped or terminated by signal"`、`exit 1` では既存の `"trapped or exited with code 1"`。
- `tests/trap_locations.rs` の `run_reports_probable_stack_exhaustion`（新規）: 既存テストと同じく `std::env::temp_dir()` の下に専用の root を作り、「再現」の sample を置く。
  `tsuzuri run <root> -O0` と `-O3` がどちらも status 1 で、stderr に `E2005` と `the stack was probably exhausted by deep recursion` を含む。
  `run --json` の stderr は `"code":"E2005"` と同じ文を含む。assert のトラップの既存の報告は `cli_reports_json_locations_and_preserves_failed_build_artifacts` が守る。

言語の受理・拒否は変えないので、拒否 case と新しい診断コードはない。

### E2E

`tests/trap_boundary.mjs`（新規、`node tests/trap_boundary.mjs target/release/tsuzuri`）。`-O0`・`-O3` の各々で次の 17 case を確かめる。
fixture の `a / b` は 2 行 14 列（`fn div a b = ` の 13 文字の直後）で、`kind` の文字列は `TrapKind::description` と同じである。

| # | 対象 | 操作 | 期待（独立な根拠） |
| --- | --- | --- | --- |
| 1 | main（`--trap-info`） | `WebAssembly.Module.imports` | `[]`（D-18） |
| 2 | main | `call("tz_div", 7n, 2n)` | `{ ok: true, value: 3n }`（BigInt `7n / 2n`） |
| 3 | main | `call("tz_div", -7n, 2n)` | `value: -3n`（0 方向への切り捨て、BigInt `-7n / 2n`） |
| 4 | main | 呼び出し前後の `exports.memory` | 成功では同じ object |
| 5 | main | `call("tz_div", 7n, 0n)` | `reason: "trap"`、`site` が 0 でなく `sites` にある、`kind: "integer division by zero"`、`line: 2`、`column: 14` |
| 6 | main | 5 の前後の `exports.memory` | 異なる object（instance を作り直した） |
| 7 | main | `call("tz_div", -9223372036854775808n, -1n)` | `kind: "integer division overflow"`、`site` は 5 と異なる |
| 8 | main | `call("tz_deep", 100000000n)` | `{ ok: false, trap: { reason: "stack", site: 0 } }` |
| 9 | main | 8 の後の `call("tz_div", 9n, 3n)` | `value: 3n` |
| 10 | main | `call("tz_deep", 10n)` | `value: 44281n`（harness で `d = 3d + k` を k = 1..10 で計算） |
| 11 | main | `call("tz_nope")` | `TypeError`、message `unknown export tz_nope`。前後の `exports.memory` は同じ object |
| 12 | main（`--trap-info` なし） | `call("tz_div", 7n, 0n)` | `{ reason: "trap", site: 0 }` だけ |
| 13 | host | `WebAssembly.Module.imports` | `[{ module: "tsuzuri", name: "Main.fail_host", kind: "function" }]` |
| 14 | host | import が `n * 2n` を返す、`call("tz_call_host", 20n)` | `value: 41n` |
| 15 | host | import が `new Error("host failed")` を投げる | `call` が同じ object を再送出する（`assert.throws` で同一性を確かめる）。`exports.memory` は前と異なる |
| 16 | host | 15 の後に import を戻して `call("tz_call_host", 1n)` | `value: 3n` |
| 17 | threads | `tests/wasm_threads.mjs` と同じく `tests/fixtures/tasks/Main.tz` を `--target wasm32 --wasm-feature threads` で build | 前提として import に namespace `tsuzuri_threads` がある。`createBoundary` が診断表の `Error` を投げる |

- native の `-O0`・`-O3` は Rust の `run_reports_probable_stack_exhaustion` が受け持つ。Phase 1 は生成物を変えないので、`live == 0` の新しい検査はない（D4）。
  既存の `live == 0` の suite（`tests/tasks.mjs`、`tests/host_imports.mjs`）は手順 9 で成功を確かめる。
- Phase 2（承認後）: C ホスト（`tests/tasks.mjs` の `tracked_alloc` 形式）で `tsuzuri_try_*` を成功・トラップ交互に 10,000 回呼び、各呼び出しの後に `live == 0`。
  入れ子の status 2、`Task.parallel_results` の worker 内トラップ、native `-O0`・`-O3`、ASan・UBSan・TSan。

### 既存テストへの影響

なし。`E2005` の既存 message を検査するテストはない（`may have trapped` は `src/driver.rs` だけにある）。SIGSEGV／SIGBUS 以外の終了の message は変えない。

### 性能

Phase 1 は生成コードを変えないので計測しない。`createBoundary` の `call` は try/catch と引数の転送だけを足す。作り直しの費用は計測していないので主張しない。

## ドキュメント

- `docs/language.md`「トラップ位置」: WASM の境界（D2・D3）、トラップ後の instance を再利用しない規則、`run` のスタック枯渇 message。「公開 ABI」は変更なしと一文で書く。
- `docs/architecture.md`: `src/runtime/trap-boundary.mjs` の役割（同梱 JS ホスト、コンパイラは参照しない）と `probable_stack_exhaustion`。`wasm-threads.mjs` を説明する行の隣に置く。
- `_docs/tools/debugging.md`「トラップの理由と位置」: `createBoundary` と `sites` の照合例、スタック枯渇の報告。
- `_docs/guides/webassembly.md`「Node.js から呼ぶ」: `createBoundary` の最小例と、所有結果を次の `call` の前に copy・解放する規則。
- `_docs/guides/native-interop.md`「公開名と呼び出し」: native ではトラップがプロセスを終えること。隔離が要る場合は子プロセスで実行すること（`tsuzuri run` と同じ方式）。
- `README.md` のテスト一覧へ `node tests/trap_boundary.mjs target/release/tsuzuri` を足す。
- `_docs/feature-status.md` と `_features/README.md` の E14 の状態を GUIDE §10 に従って更新し、Phase 2・3 が未着手（要承認）であることを備考に書く。

## 受け入れ条件

- [ ] `tsuzuri run` がスタック枯渇（SIGSEGV／SIGBUS）を `E2005` と理由付きで報告する（native `-O0`・`-O3`）。
- [ ] `tsuzuri test` の失敗理由が signal 番号とスタック枯渇の推定を含む。
- [ ] `createBoundary` がトラップ・スタック枯渇を値として返し、トラップした instance を二度と使わず、次の呼び出しを新しい instance で正しく実行する（`-O0`・`-O3`）。
- [ ] ホストの import の例外は同じ object のまま再送出され、instance は捨てられる。
- [ ] threads の module を拒否する。WASM の import は既定で空のまま。
- [ ] 生成 IR・object・`.wasm`・`.trap.json` が変わらない（手順 7）。
- [ ] Phase 2・3 に着手していない（`setjmp`・`sigaction` などの使用がない）。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `tsuzuri_trap_site` は `--trap-info` の build にしかない。`?.()` で呼び、なければ 0 にする（case 12）。
- site ID は呼び出しごとにリセットされない（docs/language.md）。古い ID が残るので、トラップした instance を使い続けると次の報告が誤る。必ず捨てる。
- 再現では同じ instance がトラップ後も正しい値を返した。これを根拠に再利用するテストや文書を書かない。`__stack_pointer` と heap は途中の状態のままである。
- catch の中で `this.exports` を読むと新しい instance を作ってしまう。古い instance の `exports` は `call` の局所変数から読む。
- 分類の順序を守る。ホストの import が `RangeError` を投げた場合も、先に `hostError` との同一性を見て再送出する。例外を包み直さない（stack trace を保つ）。
- 所有結果の pointer は instance ごとに違う。古い instance の pointer を新しい instance の `tsuzuri_free` へ渡すと別の block を壊す。
- `new WebAssembly.Instance` は同期である。ブラウザーの main thread は大きな module の同期 instantiate を拒むことがある。Phase 1 の対象は Node と Worker で、ブラウザー glue は E13 Phase 2 に任せる。
- SIGBUS の番号は OS で違う（Linux 7、macOS 10）。macOS の 7 は SIGEMT なので 7 を一律に SIGBUS として扱わない。
- 非末尾再帰の fixture は `depth (n - 1) * 3 + n` のままにする。`1 + depth (n - 1)` は LLVM の累積値変換でループになり、`-O3` で枯渇しないことがある。
- SIGSEGV は extern のホスト実装やコンパイラの誤りでも起こる。message は "probably" のままにし、Phase 1 で `TrapKind` を足さない。
- Node 20.19.6 の V8 が BigInt の多い suite で `RepresentationChangerError` を出したら `npx --yes --package=node@24 node tests/trap_boundary.mjs target/release/tsuzuri` で実行する。

## 対象外

- 言語内でのトラップの捕捉（`Trap.isolate` など）、drop を走らせる巻き戻し、`?` 演算子（D-10）。
- トラップした WASM instance の再利用・heap の再初期化。
- Phase 1 での native のプロセス内隔離（Phase 2、要承認）、signal handler（Phase 3、要承認）、`fork` による隔離（多 thread のホストでは fork 後の malloc が未定義動作になる）。
- 無限ループ・時間切れの打ち切り（非停止はトラップではない）。
- ブラウザーの glue と TypeScript の型（E13）、Windows（G10。`STATUS_STACK_OVERFLOW` の分類を含む）。
- threads の境界の変更（既存の `createThreadPool` の失敗・close の規則のまま）。

## 決定事項

### D1: 境界の意味と言語内 API

- 決定: トラップは巻き戻さない。境界はトラップした実行単位（WASM は instance、Phase 2 の native は一回の `tsuzuri_try_*` 呼び出し）を丸ごと捨て、ホスト側で値として報告する。
  言語内の API（`Trap.isolate :: (unit -> 'a) -> Result<'a, TrapInfo>`）は作らない。将来作るなら std 名 `Trap`（要割り当て）・`TrapInfo`（要割り当て）と D-10 の変更が要る。
- 理由: WASM は module 内で `unreachable` を捕捉できない（例外処理の提案もトラップは捕捉しない）。native は Phase 2 の仕組みなしには隔離できない。
  言語内で捕捉するとトラップ後の heap で Tsuzuri のコードが走り続け、D-10 の「例外・巻き戻しは導入しない」に反する。
- 状態: 既定案（実装者はこの案に従う）

### D2: WASM の境界

- 決定: 境界は export 呼び出し 1 回。例外が出たら instance を捨て、次の `call`／`exports` で同じ module と imports から作り直す。threads の module は拒否する。
- 理由: トラップ時の `__stack_pointer`・heap・`@tz.trap.latest` は途中の状態で、整合を保証できない。既存テストもトラップごとに新しい instance を作る。
  threads は pool が単位で、`createThreadPool` が既に失敗後の呼び出しを拒否する。
- 状態: 既定案（実装者はこの案に従う）

### D3: 分類と報告の値

- 決定: 仕様「分類と報告の値」の表のとおり。`reason` は `"trap"` と `"stack"` の 2 つ。ホストの import の例外とその他の例外は再送出する。
- 理由: `WebAssembly.RuntimeError` は Tsuzuri とエンジンのトラップ。スタック枯渇は V8 で `RangeError`（検証済み）、SpiderMonkey で `InternalError`（未検証）。
  ホストの失敗を値にするとホストの誤りを隠す。
- 状態: 既定案（実装者はこの案に従う）

### D4: 破損とみなす状態と確保の方針

- 決定: WASM はトラップした instance 全体を破損とみなし、個々の確保を回収しない。古い pointer・view・所有結果は無効。トラップした呼び出しの `live == 0` は検査しない。
  Phase 2 の native は確保 list で一括解放し、トラップ後も `live == 0` を満たす。
- 理由: instance を捨てれば linear memory ごと回収され、drop を走らせる必要がない。途中の heap を修復するには巻き戻しが要る。
- 状態: 既定案（実装者はこの案に従う）

### D5: native の Phase 1（スタック枯渇の推定）

- 決定: `run`／`test` の親プロセスが子の終了 signal（SIGSEGV／SIGBUS）から推定して報告する。runtime と生成物は変えない。
- 理由: signal handler なしで理由を付けられる唯一の場所である。Tsuzuri のコードはメモリ安全なので、site の報告がない SIGSEGV／SIGBUS の最も多い原因はスタック枯渇である。
- 状態: 既定案（実装者はこの案に従う）

### D6: Phase 2 の native 境界

- 決定: `--trap-mode return`（新規 CLI。承認時に名前を確定）で `@llvm.trap` を `tsuzuri_trap_raise` へ置き換え、`tsuzuri_try_<name>` が C runtime の
  `tsuzuri_boundary_run`（`setjmp`）を通して `tz_name` を呼ぶ。`tsuzuri_trap_raise` は thread-local の境界へ `longjmp` し、境界がなければ `__builtin_trap()`。
  入れ子は status 2。トラップは Tsuzuri のフレームでだけ起こるので、ホストのフレームを跨ぐ `longjmp` はない。E12 Phase 2 のコールバックとの併用は `E2000`。
- 理由: 元の設計を保つ。`setjmp` を C に閉じ込めれば IR に `returns_twice` と `jmp_buf` を持ち込まない。LLVM の invoke／landingpad は D-10 に反する。
  全呼び出しの後で flag を検査して戻る方式は未定義動作がないが、`src/llvm.rs` の全呼び出し箇所と数値 runtime を変える必要がある（承認されない場合の代案）。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D7: Phase 2 の確保 list と worker

- 決定: trap mode の `@tz.alloc`／`@tz.free` は 32 bytes の header（prev、next、境界、大きさ）を付けて境界の list に載せる。トラップ時は list 全体を `free`、成功時は残りを外す。
  `struct tz_task_group` に境界を持たせ、worker は `tz_task_execute` の中の自分の `setjmp` で item のトラップを受けて group へ最小 index と site を記録し、
  以後の item を始めない。全 item の完了後、投入した thread が `tsuzuri_trap_raise` で自分の境界へ戻る。list の変更は trap mode でだけ mutex で守る。
  C runtime 内部の確保（`src/runtime/*.c` の `malloc`）も list に載せる。載せない確保経路が残ればトラップ時に漏れるので、承認時に全経路を列挙する。
- 理由: worker が他 thread の境界へ `longjmp` することはできない。B06 の失敗伝播（最小 index、未開始 item を始めない）と同じ形にすれば WASM の逐次実行とも結果が一致する。
- 状態: 要承認（D6 と同時。承認前は Phase 2 に着手しない）

### D8: E13・B08・F13 との関係

- 決定: 仕様「前提とする他チケットのインターフェース」のとおり。E13 の glue は D2–D4 に従い、`TsuzuriTrap` の中身を D3 の値にする。
- 理由: 境界の規則を一か所に置き、生成 glue と手書きホストで挙動を揃える。
- 状態: 既定案（実装者はこの案に従う）

### D9: Phase 3 の signal handler

- 決定: 元の設計（`sigaltstack` と SIGSEGV／SIGBUS の handler、guard 範囲の判定、`write` と `abort` だけ、`TrapKind::StackOverflow`）を保つ。
  worker の guard 範囲は pthread の属性から求め、得られなければ報告せず既定動作へ戻す。Windows は G10 の完了後に vectored exception handler で同じ報告をする。
- 理由: 実行ファイル自身が理由を出せる唯一の方法だが、プロセス全体の signal 設定を変えるので費用と危険が大きい。
- 状態: 要承認（承認前は Phase 3 に着手しない）

### D10: 名前

- 決定: JS は `createBoundary` と結果の field（`ok`、`value`、`trap`、`reason`、`site`、`kind`、`path`、`line`、`column`）。
  C は `tsuzuri_try_<name>`、`tsuzuri_trap_info`、`tsuzuri_boundary_run`、`tsuzuri_trap_raise`。
- 理由: GUIDE D-30 の割り当て対象（予約語・診断コード・組み込みクラス・std 名）ではない。runtime の `tsuzuri_` 接頭辞は `tz_` の export と衝突しない。
  元の `tz_try_name` は `export def try_name` の `tz_try_name` と衝突する（見直し提案として改名した）。
- 状態: 既定案（実装者はこの案に従う。Phase 2・3 の名前は D6・D9 の承認に含める）

## 実装と検証（2026-10-02、Phase 1）

Phase 1（手順 1–9）を実装した。Phase 2（D6: native object の `setjmp`/`longjmp` 境界と `--trap-mode return`、D7: 確保 list と worker の境界）と
Phase 3（D9: signal handler によるスタック枯渇の報告）は要承認のため着手していない（`setjmp`・`sigaction`・`--trap-mode` を使っていない）。
F12 と同じ作業ツリーで実装した（着手時の HEAD は `30b2d1d`、未コミット）。コミットは作っていない。

### 実装

- `src/driver.rs`: `probable_stack_exhaustion`（unix では `ExitStatusExt::signal` が SIGSEGV と SIGBUS のとき真。SIGBUS は Linux で 7、macOS・BSD で 10。
  unix 以外は常に偽）、`run` の既定の message（site が見つからず推定が真のとき診断表の文。`--json` で stderr がある場合の既存の分岐は変えない）、
  単体テスト `probable_stack_exhaustion_matches_segv_and_bus`。
- `src/test_runner.rs`: `termination_reason`（子プロセスの終了理由。`code()` があれば従来の `trapped or exited with code N`、SIGSEGV／SIGBUS なら
  `terminated by signal N; the stack was probably exhausted by deep recursion`、ほかの signal は従来の `trapped or terminated by signal`）、
  単体テスト `execute_test_names_stack_exhaustion_signals`。
- `src/runtime/trap-boundary.mjs`（新規、57 行）: `createBoundary(module, { imports, sites })`。D2–D4 の規則（例外が出た instance を捨てる、古い instance へは
  `tsuzuri_trap_site` を一度だけ呼ぶ、ホストの例外は同じ object を再送出、threads の module は拒否、未知の export は `TypeError`）。`node:` の import・class・非同期 API は使っていない。
- テスト: `tests/trap_boundary.mjs`（新規、17 case × `-O0`・`-O3`）、`tests/fixtures/trap_boundary/Main.tz`・`tests/fixtures/trap_boundary_host/Main.tz`（新規）、
  `tests/trap_locations.rs` の `run_reports_probable_stack_exhaustion`（新規）。
- 文書: `docs/language.md`、`docs/architecture.md`、`_docs/tools/debugging.md`、`_docs/guides/webassembly.md`、`_docs/guides/native-interop.md`、
  `README.md`（テスト一覧）、`_docs/feature-status.md`、`_features/README.md`。生成 IR・object・`.wasm`・header・`.trap.json`・runtime の C と `.ll` は変えていない。

### 決定事項への追記（チケットから外れた判断）

- 判断なし。仕様の message・分類・名前（D1–D5、D10）のまま実装した。終了理由の組み立ては `execute_test` の分岐ではなく関数 `termination_reason` に出した
  （`code()` の分岐も同じ関数に入る。表示される文字列は変わらない）。
- `tests/trap_locations.rs` の既存 7 テストは変えていない。新しい 1 件を足して 8 件になった。

### 確認（Apple M1 Max、macOS、Apple clang 21.0.0、Homebrew LLD 23.1.1、rustc 1.98.1、Node v20.17.0 と v24.21.0）

- 手順 1: `cargo test --locked --test trap_locations` が 7 passed。tasks・wasm_threads の suite が成功。sample（`/tmp/tz-e14/deep`）の IR・object・wasm・`.trap.json` を `/tmp/tz-e14/before.*` に保存した。
- 手順 2–4: `cargo test --locked --lib probable_stack_exhaustion` が 1 passed、`--test trap_locations` が 8 passed、`--lib execute_test_names_stack_exhaustion_signals` が 1 passed、
  `--lib test_runner` が成功。実機で `tsuzuri run /tmp/tz-e14/deep -O3` が `E2005: program terminated with signal: 11 (SIGSEGV); the stack was probably exhausted by deep recursion; reduce the recursion depth or use a loop`、
  `tsuzuri test /tmp/tz-e14/deeptest` が `failure: terminated by signal 11; the stack was probably exhausted by deep recursion` を報告した。
- 手順 5・6: `node --check src/runtime/trap-boundary.mjs` が成功し、`grep -n "node:"` は何も出さない。`node tests/trap_boundary.mjs target/release/tsuzuri` が Node 20.17.0 と 24.21.0 で
  `Trap boundary -O0: 17 cases` と `Trap boundary -O3: 17 cases` を出して成功（import 空、`site` と行・列の照合、スタック枯渇後の再実行、ホスト例外の同一性、threads の拒否を含む）。
- 手順 7: 最終のコンパイラで sample を同じ 3 つの形に build し直し、`before.ll`・`before.o`・`before.wasm`・`before.wasm.trap.json` と `cmp` で一致した（4 組）。
- 手順 8: `node scripts/check-docs.mjs` が全体で成功した（83 pages・746 links・141 checked examples・246 native runs（O0/O3）・9 test projects）。`git diff --check` が空。
- 手順 9: `node tests/trap_boundary.mjs`（17 case、Node 20.17.0 と 24.21.0）、`tests/tasks.mjs`（41 結果・4 trap）、`tests/wasm_threads.mjs`、`tests/host_imports.mjs` を含む
  29 項目の gate script がすべて成功した（29 PASS・0 FAIL。内訳は E12 の記録と同じ）。
- 全体: `cargo test --locked` は 548 passed・0 failed（F12・E12 を含む作業ツリー全体）。`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` が成功。
  §3.1 の stack-depth 3 テストが成功。`git diff --check` が空。
- 性能: 生成コードを変えていないので計測していない。`createBoundary` の作り直しの費用は計測していないので主張しない。

### 残作業

- Phase 2（native object の境界、`tsuzuri_try_<name>`、確保 list）と Phase 3（signal handler）。いずれも D6・D7・D9 の承認が前提。
- native の `tsuzuri run`／`test` の報告は終了 signal からの推定で、実行ファイル自身は理由を出さない。ホストへ組み込んだ native object のトラップはプロセスを終了させるまま。
