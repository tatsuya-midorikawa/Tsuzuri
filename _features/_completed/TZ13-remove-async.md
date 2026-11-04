# TZ13: `Async` と Net の `_async` の削除と移行

| 項目 | 内容 |
| --- | --- |
| ID | TZ13 |
| 優先度 | P1 |
| 規模 | M。Phase 1（Net の `_async` と Async の非同期の待ちの経路）M、Phase 2（`Async` のモジュール・ビルダー・実行器・ホスト駆動の削除と `E1040`）M、Phase 3（テスト・文書・CI の整理）M |
| 依存 | TZ08, TZ12 |
| 後続 | – |
| 状態 | done（2026-11-04。Phase 1〜3。下の「着手時の再検討」と「実装と検証」） |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61 の決定 4: 即削除）。決定事項 D1〜D6 は既定案。**ホストのイベントループに計算を預ける機能（`Async.start`・`Async.host`・`tsuzuri_async_poll`）には後継がない**（D2）ことを、着手前に人間に再確認する |
| 手本にする既存実装 | TZ12（削除の進め方、`E1040`、kind を消さない規則、規則 ID の退役） |
| 主な影響ファイル | `std/Async.tc`（削除）・`std/Net.tz`・`src/runtime/async.c`（削除）・`src/runtime/net.c`・`src/runtime/bindings-core.mjs`・`src/runtime/bindings-net.mjs`・`src/runtime/bindings.mjs`・`src/bindings.rs`・`src/llvm.rs`・`src/llvm_io.rs`・`src/driver.rs`・`src/test_runner.rs`・`src/check.rs`・`src/polymorph.rs`・`src/ownership.rs`・`src/computation.rs`・`src/parser.rs`・`src/stdlib.rs`・`src/capabilities.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/target.rs`・`src/main.rs`・`src/diagnostic_kinds.rs`・`src/explain/`・`src/llvm_split.rs`・`tests/`・`_tsuzuri/language-reference/`・`docs/`・`guide/`・`vsc/`・`.github/workflows/vscode.yml`・`scripts/check-cli-docs.mjs`・`scripts/toolchain/smoke.mjs`・`README.md` |

## 実装者向け早見

- 範囲: `Async<T>`・`Async { }`・`async { }`・`Async.*`（`yield`・`sleep`・`now`・`all`・`all_results`・`run`・`block_on`・`start`・`host`・`Operation`）、ネイティブの reactor（`async.c`）、ホスト駆動の実行器（`tsuzuri_async_poll`・`complete`・`post`・`set_epoch`）、JSPI の `tsuzuri_async` の import、生成グルーの `bindings.async`、Net の `_async` の 12 関数と、それを支える `net.c` の poll のスレッド（`tsuzuri_net_watch`・`connect`・`unwatch`）を消す。書かれていれば `E1040`。
- 残すもの: `--wasm-feature jspi`（Net の WASM と tzroutine の WASM が使う）、`@alias`（ビルダーの別名の機能。std の利用者はいなくなるが、利用者の `.tc` が使える）、Net の同期の API（TZ08 の Phase 2 で tzroutine の中では G だけの待ち）。
- 最初に読む: ① 仕様 1・2 ② エージェントの調査の一覧（下の「削除と書き換えの一覧」）③ TZ12 の進め方。
- 完了の見分け方: `grep -rnwE "Async|async \{|tsuzuri_async_|_async\b" src std tests _tsuzuri docs guide README.md vsc scripts .github` が、`E1040` の検査・移行の文書・履歴以外で 0 件。

## 目的

D-61 の決定 4 のとおり、Async を削除する。I/O を待ちながら別の計算を進める用途は tzroutine と netpoller（TZ08）、タイムアウトは `select` と `Time.after`（TZ04・TZ05）、`all`・`all_results` は `WaitGroup`・`ErrGroup`（TZ06・TZ07）が引き継ぐ。

## 着手条件と停止条件

### 着手条件

- TZ08 の Phase 2（netpoller）と TZ12 が done。
- 決定事項 D2（ホスト駆動の実行器に後継がない）を人間に再確認した記録（チケットの末尾に日付と回答）。

### 停止条件

- **S1** Net の同期の API（`Net.read`・`write`・`accept`・`connect` など）の結果か、`tests/net.mjs`・`tests/net_wasm.mjs` の同期の部分が変わる。
- **S2** `src/bindings.rs` の `javascript_with_net` が、`bindings-core.mjs` の Async の文字列を目印に Net のグルーを差し込んでいる（`if (namespace === "tsuzuri_async")` などの 3 つの目印。`src/bindings.rs` 342〜375 行と 597〜620 行のテスト）。目印を消すと Net の WASM のグルーが作れなくなる。目印を Net 自身の文字列に付け替える方法が 1 つに決まらなければ止める。
- **S3** `@alias` の機能のテスト（`tests/alias.rs`）を、std の `Async` なしで書き直せない（テストの中の `.tc` で別名を宣言できない）。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認）

- `std/Async.tc`（775 行）は `@alias async`、`Step`・`Next`・`Async`・`Operation` の型、ビルダー、仮想時刻の `run`、ホスト駆動の実行器（`__poll`・`__complete`）、`block_on`（`async.c` の reactor）。組み込みは `Async.__resume`・`__take`・`__put`・`__next_id`・`__retire`・`__clock`・`__wait`・`__posted`（`src/check.rs` 1574〜1600 行、生成は `src/llvm.rs` 7569〜8230 行）。
- 実行器の検出は `host_entries`（`src/llvm.rs` 4002 行）・`reaches_reactor`（3985 行）・`uses_reactor`（3936 行）・`uses_net_async`（3947 行）。ドライバーの検査は `src/driver.rs` 3784〜3805・4161〜4183・4247〜4293 行、テストの実行器は `src/test_runner.rs` 648〜669・832〜837・954〜1047 行。ヘッダーは `header_full`（`src/llvm.rs` 2332〜2364 行）。
- Net の `_async`: `connect_async`・`accept_async`・`read_async`・`write_async`・`recv_from_async`・`send_to_async`（待つ）、`bind_async`・`bind_udp_async`・`close_async`・`close_listener_async`・`close_udp_async`・`shutdown_async`（待たない）。`std/Net.tz` 787〜1068 行、`net.c` の `tsuzuri_net_watch`・`connect`・`unwatch` と poll のスレッド（794〜910 行）、`llvm_io.rs` の `net_async_builtin`（366〜401 行）。`stdlib.rs` の Net の `uses: &["Async"]`。
- JSPI: `BuildOptions.wasm_jspi`、`--wasm-feature net` は `jspi` を要る（`driver.rs` 622〜627 行）。Async の import は `tsuzuri_async.clock`・`wait`。

## 変えてはいけない意味

- Net の同期の API とその WASM（`--wasm-feature jspi --wasm-feature net`）。`--wasm-feature jspi` の受理（Net・tzroutine のため）。
- `@alias` の構文と意味。計算式の機能（`Maybe`・`Result`・`IO`・利用者のビルダー）。
- 診断の kind の登録簿（消さない）。

## 仕様

### 1. 削除したものを書いたとき（`E1040`）

| 書いたもの | kind | help |
| --- | --- | --- |
| `Async { … }`・`async { … }`（利用者のビルダー `async` がないとき） | `removed_async` | `run the work directly; to wait for I/O while other work runs, spawn tzroutines` |
| 型 `Async<T>`・`Async.Operation` | `removed_async` | `return the value directly, or use a Channel to deliver it later` |
| `Async.run`・`block_on` | `removed_async` | `call the work directly (in a tzroutine, I/O waits without stopping other tzroutines)` |
| `Async.all`・`all_results` | `removed_async` | `spawn a tzroutine per job and wait with WaitGroup, or with ErrGroup for the first error` |
| `Async.sleep`・`now`・`yield` | `removed_async` | `use Time.sleep, Time.monotonic_ns, or Routine.yield` |
| `Async.start`・`Async.host` | `removed_async` | `the host-driven executor was removed; call exports from host threads (native) or await the export's Promise (WebAssembly with --wasm-feature jspi)` |
| `Net.connect_async` など 12 関数 | `removed_net_async` | `call Net.connect (and the others without '_async') in a tzroutine; it waits without stopping other tzroutines` |

### 2. ホストとの境界の変化

- ネイティブの C のホスト: `tsuzuri_async_poll`・`tsuzuri_async_complete`・`tsuzuri_async_post` と `--emit header` のその宣言がなくなる。ホストは export を呼び、その呼び出しは中の tzroutine が終わるまで戻らない（TZ02 の境界）。ホストのイベントループを止めたくない場合は、ホストが別のスレッドから export を呼ぶ。
- WASM の JSPI のグルー: `bindings.async`（`complete`・`settled`）がなくなる。export は今までどおり Promise を返す（tzroutine の待ちと Net の待ちで中断する）。

## 設計: 削除と書き換えの一覧

### コンパイラ・std・ランタイム

| ファイル | 処理 |
| --- | --- |
| `std/Async.tc` | 消す |
| `std/Net.tz` | `_async` の 12 関数と、その私的な補助（`watch`・`ready`・`deadline_of`・`accepted`・`pending` の判定）を消す |
| `src/runtime/async.c` | 消す |
| `src/runtime/net.c` | `tsuzuri_net_watch`・`tsuzuri_net_connect`・`tsuzuri_net_unwatch`・`tz_net_settle`・`tz_net_post` と poll のスレッド（Async 用）を消す。TZ08 の netpoller のフック（`tz_net_wait`）は残す |
| `src/check.rs` | `Builtin::Async*`、`Async.Async`・`Async.Next` の非 Copy・非 Sync の扱い（679・694〜704 行）、`builder_name` の別名の解決の Async の例（5388 行）を消す（別名の機能そのものは残す） |
| `src/llvm.rs`・`src/llvm_io.rs` | Async の生成（`async_slot_builtin`・`async_thread_index`・`Async.__*` の各腕）、`host_entries`・`reaches_reactor`・`uses_reactor`・`uses_net_async`、`header_full` の Async の宣言、`net_async_builtin` を消す |
| `src/driver.rs`・`src/test_runner.rs`・`src/target.rs` | `async_reactor`、`async.c` の連結、Async の `E2000`・`E2002` の検査（kind は消さない）、`NetAsyncNeedsBlockOn`・`WasmTestRunnerAsync` の検査、`--export=tsuzuri_async_*`、`shared_exports` の Async、`Platform::reactor()`（`net()` と同じなら呼ぶ側を `net()` に） |
| `src/bindings.rs`・`src/runtime/bindings-core.mjs`・`bindings-net.mjs`・`bindings.mjs` | `ASYNC_DECLARATIONS`、表の `async` の欄、`createAsyncHost`、`asyncImports`、世代（epoch）の扱い。`JsFlavor::Jspi` と export の `promising` の包みは残す（Net・tzroutine）。`javascript_with_net` の目印を Net の側の文字列に付け替える（S2） |
| `src/polymorph.rs`・`src/ownership.rs` | `Async.__*` の std 専用の検査（3212〜3229 行）、`__resume` の `Next` の検査（3343 行）、`BorrowAcrossAsyncBoundary` の検査（`is_async`、2462〜2473 行）を消す（kind は残す） |
| `src/parser.rs` | `@alias` の誤りの文の例 `async`（1402〜1417 行）を、別の名前（`'@alias query'` など）に。`yield` をメンバー名として読む規則（1690 行）は `Routine.yield` のために残す |
| `src/stdlib.rs` | `SOURCES`・`OPT_IN` の `Async`、`RESERVED_MODULES` の `Async`、opaque の一覧、Net の `uses: &["Async"]` を消す。件数のテスト |
| `src/capabilities.rs` | `Async.__clock`・`__wait`・`__posted` の `Time`、`PRIVATE_WITHOUT_CAPABILITY` の Async の 5 つ（配列の長さ 22 を減らす）。`tests/fixtures/capabilities/std-table.txt` の `Async.block_on` と `Net.*_async` の行 |
| `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs` | Async の組み込みの行。件数の pin |
| `src/main.rs` | `--help` の `jspi` の説明（Async に触れない文に） |
| `src/llvm_split.rs` | 分割のテストの `tz.async.next_id` の大域変数の期待 |
| `src/diagnostic_kinds.rs`・`src/explain/` | `E1040` に `removed_async`・`removed_net_async`。出なくなった kind（`net_async_needs_block_on`・`wasm_test_runner_async`・`borrow_across_async_boundary`）は残し、説明に「TZ13 で出ない」と書く |

### テスト

| ファイル | 処理 |
| --- | --- |
| `tests/async.rs`・`tests/async.mjs`・`tests/fixtures/async`・`async_host`・`async_reactor`・`net_async` | 消す。`tests/async.mjs` の JSPI の export の直列化の検査は、TZ09 の `tests/tzroutine_wasm.mjs` に同じ性質の検査があることを確かめる |
| `tests/net_api.rs`・`tests/net.mjs` | `_async` の部分を消す。`net.mjs` の `async` の段（`Async.block_on`）は、同じ通信を tzroutine で行う段に書き換える（TZ08 の Phase 2 の `echo_server` と重なるなら消す） |
| `tests/net_wasm.mjs`・`tests/fixtures/net_wasm` | `_async` と `tsuzuri_async` の部分を消す。同期の Net の JSPI は残す |
| `tests/alias.rs` | `@alias async` の例を、テストの中で `.tc` のビルダーに別名を付ける例に（S3） |
| `tests/lto.rs`・`tests/capabilities.rs`・`tests/docs.rs`・`tests/docgen.mjs`・`tests/formatter.rs`・`tests/lsp.rs`・`tests/concurrency.rs`・`tests/targets.rs`・`tests/windows.rs`・`tests/parameter_conventions.rs`・`tests/local_recursion.rs`・`tests/std_examples.rs`（`FLOORS`・`BUILDER_FLOORS` の `Async`）・`tests/std_argument_order.rs`（`MODULES` の `Async`）・`tests/compact_strings_modes.mjs`・`tests/range_flags_audit.mjs`（`"async"` の project）・`tests/fixtures/effects_sweep` | Async の部分を消すか、同じ性質の別の例に書き換える |
| `tests/removed_features.rs`（TZ03 が作り、TZ12 が足した） | 仕様 1 の行を足す |
| `vsc/src/test/grammar.test.ts` | `Async.yield`・`@alias async` の例を、`Routine.yield` と別の別名に |

### 文書・ツール・CI

| ファイル | 処理 |
| --- | --- |
| `_tsuzuri/language-reference/async-tasks-and-lazy/async.md` | 消す。`index.md` から |
| `async-tasks-and-lazy/tzroutine.md` | 「Async からの移行」の節（仕様 1 の表の書き換えの例つき。`Async.run` の仮想時刻は後継がない（時刻を関数の引数で渡す）こと、ホスト駆動の後継がないこと） |
| `compiler/webassembly.md`（「非同期計算と JSPI」452 行）・`compiler/option.md`・`compiler/native-interop.md`（460 行の `tsuzuri_async_*`）・`compiler/platforms.md`・`built-in-types-and-modules/net.md`（`_async` の節）・`computation-expressions/computation-expressions.md`（`Async` の例を別のビルダーに）・`values-and-functions/keywords.md`・`compiler/diagnostics.md`・`index.md` | Async と `_async` の説明を消すか、tzroutine に書き換える |
| `docs/language.md`（2656〜2705 行ほか）・`docs/architecture.md`（335〜343・621 行ほか）・`docs/benchmarks.md`（175 行の「Async の中断と継続のコスト」は履歴へ） | 同上 |
| `guide/language.md`（102・140 行） | Async の説明を消す。`node scripts/check-guide.mjs` |
| `spec/rules/TSK.md` | Async の規則があれば `retired`（調査の時点では TSK に Async の規則はない。確かめる） |
| `scripts/check-cli-docs.mjs` | `STD_PAGES` の `Async` の行 |
| `scripts/toolchain/smoke.mjs` | `Async.block_on` の煙の検査（108〜148 行）を、tzroutine と `Time.sleep` の検査に |
| `.github/workflows/vscode.yml` | 「Async compiler contracts」「Async executors and JSPI」とその Windows・MSVC の手順を消し、「Net sockets, async operations, and WebAssembly」の文から async を消す |
| `README.md` | 「非同期計算 (Async)」の節（373 行）を消し、機能一覧と検証の表を直す |

## 前提とする他チケットのインターフェース

- TZ08 の netpoller（Net の待ちの後継）、TZ03 の `E1040` の判定の関数 `removed_feature`（TZ12 が Task の行を足した）と `tests/removed_features.rs`。

## 他チケットへ提供するインターフェース

- なし（第4期の最後）。

## 実装手順

### Phase 1

1. **変更**: ブランチ `feat/tz13-remove-async`。基準（`node tests/net.mjs`・`net_wasm.mjs`・`async.mjs` の結果）。D2 の再確認の記録。
2. **変更**: Net の `_async` と `net.c` の Async 用の経路の削除、`removed_net_async`。**確認**: `node tests/net.mjs target/debug/tsuzuri`・`node tests/net_wasm.mjs target/debug/tsuzuri`（同期の部分。S1）。

### Phase 2

3. **変更**: `Async` の std・組み込み・生成・実行器・ランタイム・グルー・ドライバーの削除、`removed_async`、`javascript_with_net` の目印（S2）。**確認**: `cargo nextest run --locked`、`tests/removed_features.rs`、`node tests/net_wasm.mjs`、`node tests/tzroutine_wasm.mjs`（TZ09）。

### Phase 3

4. **変更**: テスト・文書・ツール・CI（一覧）。**確認**: TZ12 の手順 8 と同じ検査の一式、`gh run watch` で全ワークフローが緑。

## テスト計画

- `tests/removed_features.rs` に仕様 1 の 7 行（利用者の定義なし → `E1040`、利用者のビルダー `async`（`@alias async` の自作）あり → 通る）。
- Net の同期の API の既存のテストが変わらず通る（S1）。

## ドキュメント

- 一覧のとおり。移行の文書の例: ① `Async.run (Async.all [a, b])` → 2 つの tzroutine と `WaitGroup`（結果は `Mutex` か `Channel` で集める）、② `Async.sleep` → `Time.sleep`、③ タイムアウト → `select` と `Time.after`、④ `all_results` → `ErrGroup`、⑤ `Net.read_async` → tzroutine の中の `Net.read`、⑥ `Async.start` + `tsuzuri_async_poll` → 後継なし（ホストのスレッドから export を呼ぶ）。

## 受け入れ条件

- [ ] 早見の `grep` が 0 件（例外の場所を除く）。
- [ ] 仕様 1・2 のとおり。Net の同期の API と JSPI の Net が変わらない。
- [ ] 全ワークフローが緑。文書・ガイド・VS Code を更新した。

## 落とし穴

- **`javascript_with_net` の目印**（S2）: Async の文字列を消す前に、Net のグルーを差し込む位置の目印を Net の側のコメントに付け替え、`src/bindings.rs` のテスト（597〜620 行）で確かめる。
- **`@alias` の機能を消さない**: std の唯一の利用者は Async だが、機能は利用者の `.tc` のためにある。
- **`yield` のメンバー名**: `Async.yield` のために入れた「`yield` をドットの後で名前として読む」規則は、`Routine.yield` が使う。消さない。
- **kind を消さない**: `net_async_needs_block_on` などは出なくなるだけ。

## 対象外

- ホストのイベントループに計算を預ける新しい仕組み（D2。必要になったら、tzroutine の上の別チケット）。
- `Async.run` の仮想時刻の後継（決定的なシミュレーションの時計。D3）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 即削除し、`E1040` で移行先を示す | D-61 の決定 4 | 承認済み |
| D2 | ホスト駆動の実行器（`Async.start`・`Async.host`・`tsuzuri_async_poll`・`complete`・`post`）の後継は作らない | tzroutine は境界の中で終わる（D-61 の決定 1）。ホストのイベントループへ計算を預ける形は、その決定と合わない | 承認済み（2026-11-04、委任） |
| D3 | `Async.run` の仮想時刻の後継は作らない | 時刻に依存する処理は、時刻を引数か `Channel` で渡せば決定的に試せる | 既定案 |
| D4 | `--wasm-feature jspi` は残す | Net の WASM と tzroutine の WASM が使う | 既定案 |
| D5 | `@alias` の機能は残す | 利用者のビルダーのため | 既定案 |
| D6 | Net の `_async` は `removed_net_async` で同期の名前を示す | 同期の API は tzroutine の中で G だけの待ちになる（TZ08） | 既定案 |

## D2 の再確認（2026-11-04）

- 利用者は、TZ01〜TZ13 の判断をすべて実装者に委ねた（2026-11-04 の指示「あなたが考える最高の選択肢で実装することを常に許可します」）。着手条件の「D2 を人間に再確認した記録」は、この委任で満たす。
- その委任に基づき、D2（ホスト駆動の実行器 `Async.start`・`Async.host`・`tsuzuri_async_poll`・`complete`・`post` の後継は作らない）を採用した。理由は決定事項の表のとおり（tzroutine は境界の中で終わる。D-61 の決定 1）。ホストのイベントループを止めたくない場合は、ホストが別のスレッドから export を呼ぶ（仕様 2）。

## 着手時の再検討（2026-11-04、HEAD `16b71ba7`。TZ12 の上）

起票（HEAD `9a76d8ac`）のあとに入った TZ08〜TZ12 と照らして、次のとおり合わせた。

1. **`Net.__names`・`Net.__send` も消した**: 表にないが、使うのは `_async` の 12 関数だけだった（C の `tsuzuri_net_names`・`tsuzuri_net_send`、表の `names` の欄、グルーの対応する関数も）。
2. **時間制限 0 の「1 回だけ試す」経路は残した**: `std/Net.tz` は時間制限 0 で呼ばなくなったが、WASM の `try_accept`・`try_read` の import と `waiting_call` の分岐、`net.c` の `TZ_NET_ATTEMPT`／`TZ_NET_E_PENDING` を消すと、同期の Net の WASM モジュールの IR と import が変わる（S1）。コメントに残す理由を書いた。後で整理できる。
3. **`E1040` の範囲**: 表の行に加えて、ビルダーの操作（`Async.Return`・`Bind`・`Delay` など）と型 `Async.Step`・`Async.Next` も `removed_async`（ビルダーの help）。利用者の `Async` モジュール・ビルダーや `@alias async` があれば、それが先に解決する。どの行も `fixes` を持たない（同期版は型が違う）。
4. **件数**: 表の `PRIVATE_WITHOUT_CAPABILITY` の 22 は古く、実際は 49 → 44。組み込みの件数の pin は、消した 13 の組み込みから手で数えた（`ALL` 353 → 340 など）。
5. **S2 のグルーの目印**: `bindings-core.mjs` の 3 か所に目印のコメント（`inspect`・`importObject`・`bind`）を置き、`javascript_with_net` は `NET_INSPECT_ANCHOR` などの定数でその後ろへ差し込む（配置は前と同じ）。目印はソケットのないグルーの検査の語（`tsuzuri_net`・`netImports`・`NODE_NET`・`node:net`）を含まない。表の `"async"` は消え、JSPI の flavor だけ `"jspi": true` を持つ。
6. **ランタイムの単位の 3 番目の部分**（`async.c`）は、事前ビルドのランタイムが一致するように空の文字列として残した（TZ12 の 6 番目と同じ）。`Platform::reactor` は呼ぶ側がすべて Async だったので消した。
7. **`tests/net.mjs` の段 `async` は `portable` に書き換えた**: `vscode.yml` は POSIX の記述子を要る段 `sockets` を Windows で走らせないので、`async` の段は Windows で blocking のソケットの振る舞い（accept の前のリセットの読み飛ばし、閉じたポートへの datagram の報告の抑止）を通しで確かめる唯一の段だった。同じ case の同期版を `tests/fixtures/net_portable` に移し、期待はそのまま。`crowd`・`waits`・`cancel`・`close_wait` は消した（tzroutine の段が 100 のクライアントと close をまたぐ read を持つ。取り消しは後継がない）。
8. **JSPI の export の直列化の検査**: TZ09 の `tests/tzroutine_wasm.mjs` はまだないので、`tests/net_wasm.mjs` の `jspi_queue`（中断する呼び出しの前後を報告する export `echo_bytes` を 3 回同時に呼び、1 つずつ順に走ることと、呼んだ後に変えた型付き配列の引数が前の値のままであること）へ移した。キューを飛ばしたグルーでは順序が `+,+,+,-,-,-` になり、検査が失敗することを確かめた。wasm64 で中断する JSPI のモジュールと、WebAssembly の上の同時の待ち（`crowd` など）は、TZ09 の tzroutine の WASM で確かめる（TZ09 に引き継ぐ）。
9. **`docs/language.md` の見出し「非同期計算（Async）」は残した**: `_features/_completed/B08-async.md` などが link するので、中身を削除の記録（`E1040` の表、D2・D3、ホストの今のやり方）にした。

## 実装と検証（2026-11-04）

- **範囲**: Phase 1〜3 をすべて実装した（1 つの PR、TZ12 の PR の上に重ねる）。
- **コンパイラ・ランタイム・std**: `std/Async.tc`・`src/runtime/async.c` を削除。`std/Net.tz` の `_async` の 12 関数と私的な補助、`net.c` の Async の poll のスレッド（起こし・待ちの表・`tsuzuri_net_watch`／`connect`／`unwatch`・`tz_net_settle`・`tz_net_post`・`tz_net_cancel_handle`）を削除（TZ08 の netpoller のフックと `tz_net_wait` は残す）。`check.rs` の `Async.*` の 8 個と Net の 5 個の組み込み、Async の非 Copy・非 Sync・`is_async`、`ownership.rs` の `BorrowAcrossAsyncBoundary` の検査、`llvm.rs`・`llvm_io.rs` の生成（slot・thread index・reactor・`host_entries`・ヘッダーの `tsuzuri_async_*`）、ドライバーの Async の `E2000`・`E2002`・`async.c` の連結・`--export=tsuzuri_async_*`、テストの実行器、`capabilities.rs`・`effects.rs`・`pure_rules.rs`・`kg/effects.rs` の行と pin、グルーの `createAsyncHost`・`asyncImports`・世代を削除。kind は消さない（`net_async_needs_block_on`・`wasm_test_runner_async`・`borrow_across_async_boundary` は登録簿に残り、出なくなった）。
- **`E1040`**: `removed_async`・`removed_net_async`（`src/diagnostic_kinds.rs`、`src/explain/codes/E1040.md`）。`tests/removed_features.rs` が表の全行と、利用者の定義が勝つ場合を検査する。
- **テスト**: 上の 7・8 のほか、Async を例に使ったテストを同じ性質の別の構成（`Maybe`／`IO` のビルダー、テストの中の `.tc` の利用者のビルダー、`Routine.yield`、同期の Net）に書き換えた。`tests/async.rs`・`tests/async.mjs` と fixture `async`・`async_host`・`async_reactor`・`net_async`、`features.mjs` の suite `async` を削除。
- **実行したコマンドと結果**: `cargo nextest run --locked`（下の PR の本文に件数）、`cargo test --locked --lib`、`node tests/net.mjs`（全段。`runtime` は ASan＋UBSan と TSan も）、`node tests/net_wasm.mjs`、`node tests/features.mjs`（`regex`・`unicode` 以外の 78 suite。2 つは手元の Node 24.22 の Unicode 18 と std の 17 の差で、TZ12 の前から同じ）、`tests/docgen.mjs`・`tests/bindings.mjs`・`tests/tzroutine.mjs`・`tests/lsp_sessions.mjs`、VS Code の `test:unit`・`check-types`・`lint`、変更したページの `check-docs`、`gen-docs --check`、`check-cli-docs`、`check-guide`、`check-guide-lite`、`check-spec --require-markers`、`check-links`。`examples/*` の IR は Async を使わないものすべてで変わらない（同期の Net のプログラムは、Net が Async を読まなくなったので生成の id の番号だけが変わる）。
- **文書**: 言語リファレンス（`async.md` を削除、`tzroutine.md` に「Async からの移行」、`webassembly.md` の「JSPI」、`native-interop.md` の「ホストのイベントループと export」、`net.md`、`computation-expressions.md`、`keywords.md`、`diagnostics.md` ほか）、`docs/language.md`、`docs/architecture.md`、`docs/benchmarks.md`（B08 の測定を履歴へ）、`guide/`、`README.md`、`scripts/check-cli-docs.mjs`、`vscode.yml`・`windows.yml`・`ci.instructions.md`。
