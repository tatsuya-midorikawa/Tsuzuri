# TZ10: プリエンプション（長く走る tzroutine の横取り）

| 項目 | 内容 |
| --- | --- |
| ID | TZ10 |
| 優先度 | P2 |
| 規模 | L。Phase 1（呼び出しを含むループと関数の入口の協調的な横取り）M、Phase 2（信号による非同期の横取り。人間が求めたときだけ）L |
| 依存 | TZ08 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61）。決定事項 D1〜D6 は既定案。Phase 2 は **着手前に人間の承認が要る**（D5） |
| 手本にする既存実装 | TZ08 の sysmon（P の `syscall_tick` の見張り）、`src/llvm.rs` の `with_stack_checks`（IR の文字列の後処理で関数の入口に呼び出しを足す）、`src/llvm_overflow.rs` の計装（A23。ループの中に検査を入れる）、Go の `preemptone` と 1.14 の非同期プリエンプション（考え方だけ） |
| 主な影響ファイル | `src/runtime/tzroutine.c`・`src/llvm.rs`（または新規 `src/llvm_preempt.rs`）・`tests/tzroutine_runtime.c`・`tests/preemption.rs`（新規）・`tests/fixtures/preemption/Main.tz`（新規）・`tests/tzroutine.mjs`・`docs/benchmarks.md`・文書 |

## 実装者向け早見

- 範囲: CPU を使い続けて待たない tzroutine が、同じ P の上のほかの tzroutine（起こされた受け手、タイマーで起きた G）を無期限に待たせないようにする。Phase 1 は、sysmon が 10 ms 以上同じ G を走らせている P を見つけたら「横取りの要求」を立て、生成コードの **関数の入口** と **呼び出しを含むループの後ろへの分岐** で要求を見て `yield` する（Go 1.13 までの方式に近い）。呼び出しのない密なループ（ベクトル化されるループ）は Phase 1 では横取りされない（Phase 2）。
- 最初に読む: ① 仕様 1・2 ② TZ08 の sysmon の設計 ③ `src/llvm.rs` の `with_stack_checks` ④ D1・D2（性能の取り決め）。
- 最初に書くテスト: `tests/tzroutine_runtime.c` の `preempt_flag`（C だけで、要求を立てると、検査を呼ぶ G が `yield` する）→ E2E の `busy_does_not_starve`。
- 触ってはいけない: tzroutine を使わないプログラムの IR。呼び出しのないループの IR（ベクトル化を妨げない）。
- 完了の見分け方: `TSUZURI_MAXPROCS=1` で、1 つの tzroutine が 2 秒間 `fib 25` のような呼び出しのある計算を回す間に、別の tzroutine の 10 ms ごとの `Ticker` が少なくとも 50 回受け取れる（順序と回数の下限だけを見る）。

## 目的

TZ01〜TZ08 の tzroutine は、待つか `yield` するまで P を離さない。P の数より多くの tzroutine が計算を続けると、ほかの tzroutine（ネットワークの受け手、タイマー）が遅れる。Go は sysmon が 10 ms で横取りを要求する。同じ保証を、性能を壊さない範囲で入れる。

## 着手条件と停止条件

### 着手条件

- TZ08 の Phase 1 が done（sysmon がある）。
- 基準の性能: `node benchmarks/run-cpp.mjs`（または `docs/benchmarks.md` の手順）で、tzroutine を使う計算の核（Phase 1 で足す `benchmarks/tzroutine/`）の時間を、変更の前に測っておく（D1）。

### 停止条件

- **S1** tzroutine を使わないプログラムの IR が変わる。
- **S2** 呼び出しのないループのベクトル化が失われる（`-O3` の IR で、`vector.body` を持っていたループが持たなくなる）。
- **S3** 計算の核の時間が 3 % を超えて遅くなる（共有 CI の閾値ではなく、手元の計測の取り決め。超えたら止めて、要求の検査を入れる場所の選び方を報告する）。
- **S4**（Phase 2）信号の処理で、生成コードでない場所（ランタイムの C・libc・ホストの関数）を横取りする経路が見つかる。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認）

- TZ08 の sysmon は P の `syscall_tick` を見張る。走っている G の時間は見ない。
- 生成コードには、tzroutine に関する検査はない。

## 変えてはいけない意味

- 横取りは観測できる値を変えない（`yield` の位置が増えるだけ。整数・浮動小数点・評価順序・所有権は同じ）。
- tzroutine を使わないプログラム、呼び出しのないループ。

## Phase 分割

| Phase | 内容 |
| --- | --- |
| 1 | sysmon の検出、大域の要求の旗、関数の入口と呼び出しを含むループの検査 |
| 2 | 信号による非同期の横取り（POSIX は `SIGURG`、Windows は `SuspendThread`）。**人間の承認が要る** |

## 仕様

### Phase 1

1. sysmon は、各 P の `schedtick`（G を走らせるたびに増える）と、それを最初に見た時刻を覚え、同じ `schedtick` のまま 10 ms 以上たった P の G に横取りを要求する（`g->preempt = 1`、大域の `tsuzuri_tz_preempt_pending` を 1）。
2. 生成コードは、tzroutine を使うプログラム（`globals.tzroutine`）でだけ、次の 2 か所で `load atomic i32 @tsuzuri_tz_preempt_pending monotonic` を読み、非 0 なら `call void @tsuzuri_tz_preempt_check()` を呼ぶ:
   - すべての生成した関数の入口（`with_stack_checks` と同じ IR の後処理。`alloca` の後）。
   - **呼び出しを含む** ループの後ろへの分岐（ループの本体の基本ブロックに `call` の命令があるもの）。呼び出しのないループには入れない（D2）。
3. `tsuzuri_tz_preempt_check()`: 現在の G の `preempt` が 1 なら 0 に戻して `tsuzuri_tz_yield()`。擬似 G・要求のない G では何もしない。要求された G がすべて応じたら（sysmon が次の巡回で、要求中の G がないことを見て）大域の旗を 0 に戻す。
4. 検査の費用: 要求のない間は、1 回の atomic の読み（`monotonic`）と、予測の当たる分岐。

### Phase 2（人間の承認が要る）

- sysmon が、Phase 1 で応じない G（要求から 10 ms たっても `schedtick` が変わらない）を走らせている M に `pthread_kill(SIGURG)`。信号の処理は、割り込まれた位置（`ucontext` の PC）が **生成コードの範囲**（生成した関数をすべて専用のセクション `tz_text` に置き、リンカーの開始と終了の記号で範囲を得る）にあり、M がランタイムのロックを持っていない（`m->locks == 0`）ときだけ、PC をスタックに積み（x86-64 は赤い領域の 128 バイトを避ける。macOS の arm64 も 128 バイト）、PC を全レジスタを保存する関数（x86-64 は `xsave` で全ベクトル状態、aarch64 は q0〜q31・FPCR・FPSR・NZCV）へ書き換える。その関数は `tsuzuri_tz_yield()` を呼び、全レジスタを戻して元の PC へ戻る。Windows は `SuspendThread`・`GetThreadContext`・`SetThreadContext`・`ResumeThread`。
- 生成コードの中のどの命令の位置で止めてもよいことの根拠: Tsuzuri は GC を持たず、スタックの上の値を誰も走査しない。ランタイムの関数の中（ロックを持っているかもしれない）と libc の中（`malloc` のロック）では止めない（範囲と `m->locks` で見る）。

## 設計

### ランタイム（Phase 1）

- `tzr_p` に `uint32_t seen_tick` と `int64_t seen_at`（sysmon だけが触る）、`tzr_g` に `_Atomic int preempt`、大域 `TZR_API _Atomic int32_t tsuzuri_tz_preempt_pending`。
- sysmon の巡回（TZ08 の巡回に足す）: P が `RUNNING` で `schedtick == seen_tick` かつ `now - seen_at >= 10 ms` なら、`m->curg->preempt = 1`（`tzr_sched.lock` の下で `m->curg` を読む。M の `curg` は `tzr_execute` が書く）、`tsuzuri_tz_preempt_pending = 1`。違えば `seen_tick = schedtick`・`seen_at = now`。要求中の G がなくなった巡回で旗を 0 に。
- `TZR_API void tsuzuri_tz_preempt_check(void)`。

### 生成（Phase 1）

- `with_preemption_checks(ir) -> String`（新規。`with_stack_checks` と同じ形の後処理）: 関数の入口の検査と、ループの後ろへの分岐の検査。ループの判定は IR の文字列ではなく、生成の段で分かる（`while`・`for` の生成が後ろへの分岐を出す所）ので、`FunctionEmitter` に「このループの本体は `call` を出したか」を覚えさせ、出していれば後ろへの分岐の前に検査を出す。関数の入口は後処理でよい。
- 宣言 `@tsuzuri_tz_preempt_pending = external global i32` と `declare void @tsuzuri_tz_preempt_check()` は `intrinsics`。

## 前提とする他チケットのインターフェース

- TZ08 の sysmon、TZ06 の `uses_tzroutine`（`globals.tzroutine`）。

## 他チケットへ提供するインターフェース

- なし。

## 実装手順

### Phase 1

1. **変更**: ブランチ `feat/tz10-preemption`。基準の計測（着手条件）。
2. **変更**: ランタイムと C のテスト `preempt_flag`・`preempt_sysmon`（`TSUZURI_MAXPROCS=1` で、`tsuzuri_tz_preempt_check` を呼びながら回る G と、`Time.sleep` の 10 ms の繰り返しで時刻を記録する G。後者が 1 秒で 50 回以上起きる）。**確認**: `TSUZURI_TSAN=1 node tests/tzroutine_runtime.mjs`。
3. **変更**: 生成。**確認**: `tests/preemption.rs`（tzroutine を使わないプログラムの IR が変わらない、使うプログラムの関数の入口に 1 つ、呼び出しを含むループに 1 つ、呼び出しのないループに 0）。`-O3` の IR で、呼び出しのないループの `vector.body` が残る（S2）。
4. **変更**: E2E（下）と計測（`benchmarks/tzroutine/` に計算の核と、`docs/benchmarks.md` の新しい節。S3）。
5. **変更**: 文書、T2、CI。

### Phase 2

- 承認のあとで、別の PR。

## テスト計画

| E2E | 内容 | 期待 |
| --- | --- | --- |
| `busy_does_not_starve` | 早見の例 | 50 回以上（下限だけ） |
| `values_unchanged` | 横取りのある計算（再帰の `fib`、配列の総和）の結果が、tzroutine なしの同じ計算と一致 | 一致 |
| `tight_loop_not_checked` | 呼び出しのないループの IR に検査がない | IR の検査（Rust） |

## ドキュメント

- `async-tasks-and-lazy/tzroutine.md`: 「横取り」の節（10 ms、検査の位置、呼び出しのない密なループは横取りされないこと（Phase 1）、長い計算の途中で `Routine.yield` を呼ぶ書き方）。`docs/architecture.md`、`docs/benchmarks.md`（計測の結果と条件）。

## 受け入れ条件

- [ ] Phase 1 の仕様のとおりに動き、S1〜S3 を満たす。
- [ ] 文書と計測の記録を更新した。

## 落とし穴

- **`m->curg` の読み**: sysmon は別のスレッド。`tzr_execute` の `m->curg` の書き込みを `atomic` にするか、`tzr_sched.lock` の下で読む（TSan で確かめる）。
- **要求の旗の戻し忘れ**: 戻さないと、すべての検査が遅い経路を通り続ける（正しいが遅い）。テストで旗が 0 に戻ることを確かめる。
- **関数の入口の検査と再帰**: 深い再帰の入口ごとに読みが 1 回増える。S3 の計測に再帰の核を入れる。

## 対象外

- Phase 2（承認まで）。WASM（TZ09 の JSPI では横取りしない）。優先度の付いたスケジューリング。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 計算の核で 3 % を超えて遅くしない（手元の計測の取り決め。CI に閾値は入れない） | AGENTS.md の性能の方針 | 既定案 |
| D2 | 呼び出しのないループには検査を入れない | 検査の分岐がベクトル化を止める | 既定案 |
| D3 | 要求は 10 ms（Go と同じ） | — | 既定案 |
| D4 | 大域の 1 つの旗（M ごとの TLS にしない） | TLS のアドレスを関数の中で使い回す問題（TZ01 の R2）を避ける | 既定案 |
| D5 | 信号による非同期の横取り（Phase 2）は人間の承認のあと | アーキテクチャごとの全レジスタの保存と、横取りしてよい位置の判定は誤ると値を壊す | 要承認 |
| D6 | 横取りは `yield`（全体の run queue の末尾へ） | Go の `Gosched` と同じ | 既定案 |
