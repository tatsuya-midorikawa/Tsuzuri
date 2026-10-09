# B08: 非同期計算（Async）とホスト駆動の実行

| 項目 | 内容 |
| --- | --- |
| ID | B08 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | B05, B07, (E08), (E13) |
| 後続 | E09 |
| 状態 | done（Phase 1・2・3） |
| 起票 | 2026-09-29。旧計画の基準は `f8dc655` |
| 実装の基準 | `ce7e8a1`、2026-10-09 |
| 承認 | 全フェーズと必要な判断への包括承認。[GUIDE D-42](../GUIDE.md#d-42-b08-の全フェーズと現行仕様の確定) |
| 利用者向け仕様 | [Async 式](../../_tsuzuri/language-reference/async-tasks-and-lazy/async.md)、[言語仕様](../../docs/language.md#非同期計算async) |

## 目的と実装範囲

計算を中断点で合成し、ホストの非同期操作、イベントループ、実時間のタイマーにつなぐ。
`Task` の同期的なフォーク・ジョインとは別の、1 スレッドで協調的に進む cold な計算である。

| フェーズ | 実装 |
| --- | --- |
| Phase 1 | `Async<'a>`、ビルダー、`Async.yield`・`sleep`・`now`、仮想時計の `run`、`all`・`all_results`、中断をまたぐ借用の拒否 |
| Phase 2 | `Operation`、`host`、`start`、ホストの poll / complete、未完了の操作の取消 |
| Phase 3 | native POSIX reactor、他スレッドからの完了、`block_on`、WASM JSPI、Promise の生成グルー、native test / debug-test / bench |

旧計画は Phase 2 を設計だけとし、古い構文・文書配置と未完了の依存を前提にしていた。
着手前に B07・E08・E13・E14 の完了と D-34〜D-41 を確認し、全フェーズの依頼に合わせて再検討した。
現行の契約はこの記録、GUIDE D-42、言語リファレンスを優先する。

## 確定した API と意味

```text
Async.yield       :: unit -> Async<unit>
Async.sleep       :: i64 -> Async<unit>
Async.now         :: unit -> Async<i64>
Async.all         :: Capture<'a> => [Async<'a>] -> Async<['a]>
Async.all_results :: (Capture<'a>, Capture<'e>) => [Async<Result<'a, 'e>>] -> Async<Result<['a], 'e>>
Async.run         :: Async<'a> -> 'a
Async.start       :: Async<unit> -> IO<unit>
Async.host        :: Async.Operation -> Async<i64>
Async.block_on    :: Async<'a> -> IO<'a>
```

- `Async` は予約された opt-in std モジュール。使わないプログラムの IR・import・ABI は変えない。
- `Async<'a>` と内部の `Next<'x,'y>` は不透明・非 Copy。通常の関数値は従来どおり Copy で、既存の捕捉規則を変えない。
- 既存の計算式展開を使い、builder operation は `Return`・`ReturnFrom`・`Bind`・`Delay`・`Zero`・`Combine`・`For`。
  暗黙の Async 本体も既存の規則で選ばれる。`and!`・文の `yield`・`while` など未定義の操作は E1018。
- 中断点の名前は `Async.yield ()`。`yield_now` の別名はない。予約語 `yield` を関数宣言名とメンバー位置で使えるようにし、
  単独の `yield` はコンピュテーション式の文のままにした。formatter・LSP・VS Code も同じ規則。
- `run` は独立した仮想時計を 0 から始める。yield は時計を進めず、全計算が眠ったときだけ最小の起床時刻へ進む。
  `sleep n` は `n <= 0` なら yield、正の加算は i64 MAX で飽和。`now` は中断しない。
- `all` は開始・各 round とも入力順、結果も入力順。`all_results` はスケジュール順で最初の Error で止め、
  残りを現在の中断点で破棄する。`Vec.pop` による move で集約するため、旧計画の結果の Copy 制約は不要。
- `For` の要素には Copy / Capture を要求する。同期完了の反復はループで進め、反復数に比例してネイティブのスタックを深くしない。

### 所有権と借用

`Async<T>` の式は実際の loan を持てない。cold な開始も境界で、借用した callback 環境を record に入れてから渡す場合も検査する。
抽象的な引数の placeholder loan だけを呼び出し側の具体的な環境で検査し、std 本体の検査は免除しない。
所有した捕捉値を中断点の内側で借用することは許す。

```text
error[E1013]: async computations cannot keep borrowed values across 'let!', 'do!', or the start of the computation; move or clone the value into the async block instead
```

### ホストの契約

`Operation` は `{ start: i64 -> unit, cancel: i64 -> unit }`。操作を開始した実行器が ID を割り当てて start に渡す。
`Async.start` は投入して戻り、最初の poll が cold な計算を開始する。
未完了の操作を破棄すると cancel を 1 回呼び、完了・取消済みの ID は再利用しない。

```c
int64_t tsuzuri_async_poll(int64_t now);
void tsuzuri_async_complete(int64_t operation, int64_t value);
/* native reactor に到達する場合だけ */
int32_t tsuzuri_async_post(int64_t operation, int64_t value);
```

- poll は非負の時刻、FIFO の完了、poll の開始時に進められた root を処理する。時刻は戻さない。
  計算なしは -1、すぐ再開できれば現在時刻、タイマーは最小の期限、ホスト完了だけなら INT64_MAX を返す。
- raw complete の未知・二重・取消済み ID、poll の再入、`run` でのホスト操作は trap。
- native はスレッドごとの TLS。操作 ID の bits 39–62 に再利用しないスレッド index、下位 39 bits に操作のカウンターを置く。
  別スレッドは post を使う。post は登録された操作を一度だけ受理して 1、未知・二重・退役済みなら確保せずに 0 を返す。
- reactor は開始時に登録、完了・取消時に退役させ、取消と競合した完了も除去する。最後の操作がなくなると mailbox を解放する。
  mailbox・登録表・完了キューも標準の `tsuzuri_alloc` / `tsuzuri_free` を使い、host/counting allocator と解放追跡の対象にする。
- ホストは開始したスレッドの実行器を完了まで駆動する。ほかのスレッドの poll や、スレッドの終了による自動駆動はない。

### 実時間と WASM

- native `block_on` は POSIX の単調時計と条件変数を使う。timer と外部完了が B08 の範囲で、ソケットは E09。
  kqueue／epoll／IOCP を先に足す旧案は、まだソケット API がない現行仕様では採用しない。
- WASM `block_on` は明示的な `--wasm-feature jspi`。`tsuzuri_async.clock` と `tsuzuri_async.wait` を import し、
  `WebAssembly.Suspending` / `WebAssembly.promising` で待機する。wasm32 / wasm64 の raw JSPI に対応する。
- WASM は `tsuzuri_async_set_epoch(i64)` を export する。操作開始前の、正で 2^24 未満の世代だけを受理する。
  raw ホストは再作成時に別の世代を使い、ID と実行器を対応付ける。
- wasm32 の生成グルーは `bindings.async.complete` / `settled()` で自動駆動する。同じ生成モジュール内では複数の load と再作成を通じて
  世代を再利用せず、別の世代の完了を TypeError で拒否する。complete の引数は厳密な i64 範囲で、切り詰めない。
- JSPI の生成 export はすべて Promise で直列化する。typed array は要求時にスナップショットを作り、withBorrowed は提供しない。
  raw 呼び出しの block_on の重ね実行は trap。失敗は settled を reject し、再作成まで保持する。未観測の background failure は診断する。
- 未対応は明示的に拒否する: native Windows reactor は E2002、JSPI なしの WASM block_on、threads / WASI との JSPI の併用、
  executor と WASM threads / trap-return の併用、WASM の host executor を使うテスト実行器は E2000。
  JSPI 機能のないエンジンでは生成グルーの load が Error。

## 旧計画から外れた判断

| 旧判断 | 確定した判断と理由 |
| --- | --- |
| D1: `yield_now` | `Async.yield`。予約語のメンバー名の前例 `Set.union` を使い、二重の API は作らない |
| D2: LLVM の変更なし | consuming `Next` と std 専用 `Async.__resume`。通常の Copy closure だけでは継続の深い複製が起こる |
| D3: `Done / Yield / Sleep / Now` | `Done / Now / Begin / Suspend`。再開条件は ready、optional timer、操作 ID の集合。i64 MAX の timer と「timer なし」を区別する |
| D6: あらゆる形式的 loan を拒否 | concrete loan は拒否し、引数の placeholder は実際の caller で検査する。std の特例免除はしない |
| D7: 集約結果は Copy | 現在の `Vec.pop` を再利用して move で処理し、string なども受理する |
| D10: Phase 2 は設計だけ | 全フェーズの承認を受け、Operation、取消、ホスト ABI、TLS、post、JSPI と生成グルーまで実装 |
| Phase 3: socket reactor も一括 | B08 の timer / 外部完了には条件変数を採用。socket の readiness は後続 E09 で設計する |
| 旧 `_docs/` と i64 既定の例 | 現在の日本語リファレンス、i32 既定、`def ... :: ... = \...` と名前空間の契約を優先 |

新しい Rust crate、unsafe Rust、fast-math、資源上限・WASM stack の引き上げ、既定の host import は追加しない。
コンパイラ生成の async 状態機械、先取り、stackful coroutine、独自のスレッドプール、取消 token、channel、Async の while / and! は対象外。

## 受け入れ条件

- [x] Phase 1・2・3 の API と実行器を実装し、予約語・不透明性・non-Copy・借用・診断を検査する。
- [x] native / WASM の O0 / O3 で時刻・順序・短絡・飽和・非 Copy の結果を確認する。
- [x] 100 万回の同期 For と末尾 yield を既定の stack / WASM memory で確認する（For の大規模ケースは unit の配列）。
- [x] host と reactor の登録・完了・取消・他スレッド・再入・不正 ID、Task.parallel と独立 TLS を確認する。
- [x] 生成グルーの並行呼び出し、入力のコピー、世代、範囲、失敗保持、TypeScript の宣言を確認する。
- [x] native の test、単独 debug-test、bench と、未対応の WASM runner の診断を確認する。
- [x] ASan で C host だけでなく生成 IR も計装し、mailbox を含む native の正常終了後に live == 0 を確認する。
- [x] Async を使わない 6 組の基準 IR をバイト比較し、決定性・宣言の重複なし・純粋な WASM の import なしを維持する。
- [x] fmt / clippy / Rust 全テストと、既定 stack の深度・特殊化境界の回帰を確認する。
- [x] 日本語リファレンスと README / language / architecture / benchmarks、GUIDE / feature status を更新する。
- [x] 測定値と raw samples を記録し、速度優位や SIMD・並列・GPU 加速は主張しない。

## 実装と検証（2026-10-09）

### 実装したファイル

- `std/Async.tc`。登録と内部 builtin は `src/stdlib.rs`、`src/check.rs`、`src/polymorph.rs`。
- `src/ownership.rs`（借用境界）、`src/parser.rs`（member yield）、`src/llvm.rs`（consuming resume、TLS、host ABI、epoch）。
- `src/runtime/async.c`、`src/driver.rs`、`src/main.rs`、`src/test_runner.rs`（runtime / CLI / runner）。
- `src/bindings.rs`、`src/runtime/bindings-core.mjs`、`src/runtime/bindings.mjs`（JSPI、生成宣言、自動駆動、失敗と世代）。
- `src/lsp.rs`、`tests/formatter.rs`、`tests/lsp.rs`、`vsc/src/core.ts`、`vsc/src/editor.ts`、
  `vsc/syntaxes/tsuzuri.tmLanguage.json`、`vsc/src/test/grammar.test.ts`（editor の keyword / reserved module）。
- `tests/async.rs`、`tests/async.mjs`、`tests/features.mjs`、`tests/bindings.rs`、
  `tests/fixtures/async`、`tests/fixtures/async_host`、`tests/fixtures/async_reactor`。
- `.github/workflows/vscode.yml` に Node 24 の Async E2E を追加（macOS / Linux、x64 / ARM64）。

### 検証結果

- 基準の Rust 全テストは 949 件。最終の `RUST_MIN_STACK=4194304 cargo test --locked` は **965 件成功**。
  この環境変数は変更前にも必要だった全 suite 用で、コンパイラの上限やプログラムの stack は変更していない。
- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings` は成功。
  `tests/async.rs` は 15 件、formatter 17 件、LSP 29 件、bindings 5 件。
- 次の 4 件は RUST_MIN_STACK を設定せず、各 1 件の実行を確認して成功:
  `bounds_type_growing_polymorphic_recursion`、`bounds_recursive_and_flat_expression_depth`、
  `bounds_nested_builder_expansion_not_just_source_syntax`、lib の `honors_the_exact_specialization_limit`。
- `tests/features.mjs` 全 suite は **12,684 ケース成功**。100 万反復の回帰を含め、Async は **29 ケース成功**。
  `tests/computations.mjs` は各最適化で 70 結果 / 3 traps、`tests/tasks.mjs` は 41 結果 / 4 traps。
  いずれも native / WASM の O0 / O3 と解放追跡。
- `tests/async.mjs` は native host / reactor（mailbox も live == 0）、wasm32 / wasm64 JSPI、
  生成グルー、test / debug-test / bench の O0 / O3 が成功。Apple Clang 21 の ASan でも host / reactor と純粋な Async が成功。
  Homebrew LLVM 21.1.8 の ASan は空の C main でも起動が止まる環境問題を再現したため、ASan は Apple Clang 21 を使用。
- 初回 macOS ARM64 CI は、4 個の Task に 4 個のスレッドを仮定したテストで失敗した。既存の pool は CPU 数で制限し worker を再利用するので、
  契約どおり操作 ID の非再利用を検査するよう修正し、1 / 2 CPU を強制する O0 / O3 の回帰も追加した。言語・runtime の挙動変更はない。
- `tests/e2e.mjs`、`tests/wasm64.mjs`、`tests/docgen.mjs`、`tests/bindings.mjs`、
  `tests/bindings_threads.mjs`、`tests/host_bindings.mjs`、`tests/lsp_sessions.mjs` が成功。
  host bindings は Python / C++ / C#（.NET SDK 10.0.102）。Worker glue は Node Web Workers、実 browser は未実行。
- VS Code の `npm run test:unit` は 13 件、`npm run lint` は成功。
- Windows x64 / ARM64 は installed targets を持つ rustup の rustc を明示した `cargo check --locked --all-targets --target ...` が成功。
  Windows の実行確認は GitHub CI の対象で、POSIX reactor を Windows に対応させたとは主張しない。
- 現在の C runtime は GNU/Linux x64 / ARM64 と musl x64 へ `-std=c11 -Wall -Wextra -Werror` で cross-compile が成功。
- 6 組の non-Async IR は基準とバイト一致。runtime include の追跡検査は 35 ファイルで成功。
- `benchmarks/run-computations.mjs --quick --baseline ...` は 9 種目の参照と基準コンパイラの照合が成功。quick の時間は性能値に使わない。
  managed quick は既存の C# runner が `control/array_index_sum` を拒否する問題で停止し、保存した基準コンパイラでも同じ失敗を確認した。
  この B08 と無関係な benchmark の問題は変更していない。

### 文書と計測

更新した日本語リファレンスは Async、Task、computation expressions、keywords、modules、union、
WebAssembly、native interop、CLI options、diagnostics、why / how-about / strategy、index。
`node scripts/check-docs.mjs <変更ページ>` は 14 ページ、377 links、60 examples、104 native O0 / O3 runs が成功。

測定は Apple M1 Max / macOS 27.0.1 / Apple Clang 21、native generic O3、warm-up 1 回と各 9 標本。
末尾 yield 100 万回は中央値 0.18 秒、範囲 0.17–0.23 秒、最大 RSS 1,753,088 bytes。
非末尾の 1000 / 2000 / 4000 段は中央値 0.03 / 0.11 / 0.40 秒。
raw samples、入力、計測スクリプト、machine code は ignored の `target/perf/B08-after/`、PX01 形式は `process.jsonl`。
条件と再現方法は [性能測定](../../docs/benchmarks.md#async-の中断と継続のコストb08)。
代表的な i64 の bind_step は直接の tz.alloc 4 箇所、環境 clone helper は 2 箇所。
非末尾の深い再開は O(n²) で、WASM の stack exhaustion の既知の限界を隠していない。
