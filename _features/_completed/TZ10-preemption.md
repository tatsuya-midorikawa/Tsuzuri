# TZ10: プリエンプション（長く走る tzroutine の横取り）

| 項目 | 内容 |
| --- | --- |
| ID | TZ10 |
| 優先度 | P2 |
| 規模 | L。Phase 1（呼び出しを含むループと関数の入口の協調的な横取り）M、Phase 2（信号による非同期の横取り。人間が求めたときだけ）L |
| 依存 | TZ08 |
| 後続 | – |
| 状態 | done（Phase 1: ネイティブのすべてのターゲット。Phase 2: macOS・Linux の実行ファイル） |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61）。決定事項 D1〜D6 は既定案。Phase 2 は 2026-11-04 に委任で承認（[Phase 2 の承認](#phase-2-の承認2026-11-04委任)） |
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

- [x] Phase 1 の仕様のとおりに動き、S1〜S3 を満たす（検査の位置は再検討の 5 のとおりに変えた）。
- [x] 文書と計測の記録を更新した。

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
| D5 | 信号による非同期の横取り（Phase 2）は人間の承認のあと | アーキテクチャごとの全レジスタの保存と、横取りしてよい位置の判定は誤ると値を壊す | 承認済み（2026-11-04、委任。下の節） |
| D6 | 横取りは `yield`（全体の run queue の末尾へ） | Go の `Gosched` と同じ | 既定案 |

## Phase 2 の承認（2026-11-04、委任）

D5 は Phase 2 の着手に人間の承認を求めていました。利用者は 2026-11-04、TZ01〜TZ13 のすべての判断を実装者に委ね（「あなたが考える最高の選択肢で実装することを常に許可します」）、すべての Phase の完了を求めました。これを D5 の承認として Phase 2 に着手しました。ただし、承認は「安全にできる構成」に限ります。生成コードの外（ランタイム・C ライブラリ・ホスト）で止める経路が見つかれば S4 で止め、安全にできないターゲットは Phase 1 のままにして記録する、という条件は変えていません（下の再検討の 11〜14）。

## 着手時の再検討（2026-11-04、HEAD `16bae89f`。TZ08・TZ12・TZ13 の上）

起票（HEAD `9a76d8ac`）のあとに入った TZ08（sysmon・netpoller）・TZ12・TZ13 と、手元の計測に照らして、次のとおり合わせた。

1. **要求は G でなく P に付ける**: 設計の「`tzr_sched.lock` の下で `m->curg` を読み `g->preempt = 1`」は、G が見ている間に終わってキャッシュの外で解放されうる（`tzr_after_exit` は `tzr_sched.lock` を取らない）ので、解放後の書き込みになりうる。P は解放されないので、P に `preempt`・`runm`（走らせている M）・`run_lo`／`run_hi`（その G のスタック）を持たせた。`schedtick` は所有者が書き sysmon が読む relaxed の atomic にした（TSan）。
2. **大域の旗は数**: `tsuzuri_tz_preempt_pending` は「要求中で応じていない P の数」。要求の取り消し（応じた検査、新しい G を走らせる `tzr_execute`、走っていない P を見た sysmon）が数を引くので、sysmon の次の巡回を待たずに 0 へ戻る（落とし穴「旗の戻し忘れ」は C の `preempt_flag`・`preempt_sysmon` が数の 0 を確かめる）。生成コードの読みは「0 でないか」で、仕様のとおり。
3. **待つ仕事があるときだけ求める**: 10 ms 走った G でも、その P の環が空で、全体の列が空か空いた P があり、P を待つ M もなければ求めない。1 人で計算する G は譲る相手がいないので、検査の遅い経路も信号も起きない（Go は求めるが、S3 の計測と、信号が無関係の system call を割り込むのを避けるため）。
4. **対象のプログラムは `spawns`**: 仕様の `globals.tzroutine`（`uses_tzroutine`）は、spawn せずに `Channel` やタイマーを使うだけのプログラムも含むが、そこには P を持つ G がない。TZ12 からネイティブの `Parallel.*` は手伝いの tzroutine を作るが、待つのは P を持たない呼び出し元のスレッドで、手伝いを横取りしても待つ相手がいない。そこで検査は spawn するネイティブのプログラム（`llvm::spawns`）だけにした。spawn しないプログラムの IR は 1 byte も変わらない（S1。`tests/preemption.rs` と、`examples/*` の IR の比較）。
5. **検査の位置（S2・S3 で設計を変えた）**: 仕様どおり「すべての生成した関数の入口」と「`call` を出したループ」に入れると、(a) 小さな関数（`square`）やラムダ（`Array.map (\x -> x * 2)`・`Array.fold`）を呼ぶループが、インライン展開で入口の検査を取り込んで `-O3` の `vector.body` を失い（S2 の趣旨。`tests/preemption.rs` の比較で確かめた）、(b) 計算の核で `fib 35` が +10 %、配列の再帰ソートが +34 % になった（S3 超過。後者は `Array.filter` が検査で大きくなり、インライン展開されなくなって述語が間接呼び出しのまま残ったため）。停止条件に従って配置の選択肢を測り（`docs/benchmarks.md` の「tzroutine の横取りの費用（TZ10）」）、次に決めた。
   - 再帰（IR の直接の呼び出しの閉路）の関数: 入口でなく、閉路の関数を呼ぶブロックの最初の呼び出しの前（基底の道に入れない。`fib` は +9 % → 誤差の範囲）。
   - IR に閉路が見えない関数（ソースで関数とラムダを名指す閉路、`dyn` の dispatch）: 入口。
   - ループ: 重い呼び出し（宣言だけの関数、インラインのアセンブリ、閉路の関数、それらを呼ぶ定義）を含む自然なループの latch の終端の前。確保（`malloc`・`realloc`・`free`、`--allocator host`・トラップの境界の確保）、intrinsic、ポインターを通した呼び出し（定数のクロージャはインライン展開で直接の呼び出しになる）は軽い。
   - ループは生成の段でなく、IR の文字列から支配木と自然なループで見つける（設計の「`FunctionEmitter` に覚えさせる」の代わり）。組み込みの操作が自分で出すループ（`Array.map` の本体など）と自己末尾呼び出しのループを同じ規則で扱い、どの呼び出しがインライン展開で消えるかはモジュール全体の呼び出しの図でしか決まらないため。
   これで呼び出しのないループに加え、軽い呼び出しだけのループも Phase 1 では横取りされない（D2 の範囲を広げた。Phase 2 が止める）。
6. **検査の形**: `call void @tz.preempt.poll()`（`alwaysinline` の 1 行。LLVM が冷たい呼び出しのブロックを分けるので、パスは制御の流れも `phi` も書き換えない）。旗は `external hidden global`（ランタイムが同じ image に hidden で定義するので GOT を通らない）、`tsuzuri_tz_preempt_check(ptr @tsuzuri_tz_preempt_pending)` は `memory(argmem: readwrite, inaccessiblemem: readwrite)`（yield はプログラムの記憶域の同期でない。D-61 でデータ競合はないので、LLVM は値をレジスタに置いたまま呼べる。消されはしない。検査は数を引くので、最初の `memory(inaccessiblemem: readwrite)` はモジュールが宣言する数について誤りで、数を引数で渡すようにした。Copilot の #91 の指摘）。`facts` が `memory(...)` を付けた関数は直線で intrinsic だけを呼ぶので検査の対象にならず、念のため触らない。
7. **トラップの境界では譲らない**: `Domain.run` の枠（`trap.c` の `tz_frame`）はスレッドの TLS なので、境界の中の G を別の M で続けると、トラップが別の G の枠へ跳ぶ。純粋な閉包は協調的には待たないので今までは起きなかったが、横取りは起こしうる。検査と Phase 2 の入口は `tzr_trap_boundary()` なら譲らない（求めは取り消し、10 ms 後にまた来る）。
8. **`errno` を保つ**: 横取りは呼び出しの合間の見えない yield なので、TZ08 の規則に合わせて `errno`（Windows は `GetLastError`）を保つ。
9. **スレッドの記憶域を読む IR の関数は `noinline`**: `--allocator small` の `tz_small_realloc`（`heap-small.ll`）は `noinline` でなく `@llvm.threadlocal.address` を使う。LLVM はこの intrinsic の結果を呼び出しをまたいで使い回せるので、生成した関数へインライン展開されると、検査（や TZ08 以前からの待ち）で M が変わった後に前のスレッドの cache を使いうる。spawn するプログラムでは、`@llvm.threadlocal.address` を使う生成でない関数に `noinline` を付けた（検査より前からあった危険。下の「残したこと」）。
10. **`m->locks` は作らなかった**: 設計の「M がランタイムのロックを持っていない（`m->locks == 0`）」は、ランタイムが生成コードをロックを持ったまま呼ぶ経路があるときに要る。読んだ結果、ランタイムが生成コードを呼ぶのは G の開始（`g->code`）、`Parallel`・`Routine.scope` のコールバック、タイマーと `Context.after_func` の drop と解放の関数、`trap.c` の thunk だけで、どれもロックを外してから呼ぶ（R1）。生成コード自身はランタイムのロックを持たない（`Mutex.with_lock` は G のロックで、その中で譲ってよい。TZ06）。範囲（生成コードのセクション）だけで十分なので、数は持たない。
11. **Phase 2 は実行ファイルだけ**: `stack.c` と同じく、オブジェクト・共有ライブラリ・bitcode はホストのシグナルの設定を変えない（bitcode はホストの LTO がランタイムやホストの関数を生成した関数へインライン展開しうるので、範囲の判定も成り立たない）。ドライバーは spawn するネイティブの実行ファイル（macOS・Linux）でだけ、生成した関数（`@tz.fn.*`・`@tz.specialized.*`、それをインライン展開しうる adapter・tzroutine の入口・チャンク・callback と export の包み、それらを直接呼ぶモジュールのすべての関数。`dyn` の slot や `Drop.drop` を呼ぶ drop は名前の一覧から漏れていたので、呼び出しのグラフで決めるようにした。Copilot の #91 の指摘。スレッドの記憶域を読む関数は除く）を Mach-O の `__TEXT,__tz_text`（`section$start$__TEXT$__tz_text` などの記号は、空のセクションでも ld64 が作る）か ELF の `tz_text`（`__start_tz_text`／`__stop_tz_text`）に置き、ランタイムを `-DTZ_ASYNC_PREEMPT` で作る。COFF の `.text$tz…` は使わない（Windows は 14）。
12. **Phase 2 の戻り方（2 回の信号）**: 仕様の「PC をスタックに積み、全レジスタを保存する関数へ書き換え、戻って元の PC へ」は、AArch64 で成り立たない。全レジスタを戻したあと任意の番地へ飛ぶには空いたレジスタが要るが、LLVM の生成コードは x16・x17 も割り当てる（x18 は Linux で一般のレジスタ、Apple と Windows では使えない）。そこで、1 回目のハンドラーがカーネルの保存した文脈をまるごと G のスタックへ写し、C の `tzr_async_entry(frame)` を呼んだ形にして戻り、それが譲ったあと自分のスレッドへもう一度 `SIGURG` を送り、2 回目のハンドラーが写しを自分の signal frame に書き戻して戻る（カーネルの復帰がすべてのレジスタ・フラグ・浮動小数点とベクトルの状態を、そのとき G がいるスレッドで戻す）。x86-64 も同じ経路にした（`xsave` の手書きのアセンブリを持たないため）。ハンドラーは M の代替スタックで走り、ロックも TLS も使わない（2 回目は、M の一覧の `async_resume` と割り込んだ SP で frame を見つける）。
13. **Phase 2 で止めないもの**: PC がセクションの外、SP が sysmon が印を付けた P の G のスタックの外、赤い領域と保存と `tzr_async_entry` の 32 KiB とガードの上の 16 KiB が入らないとき、文脈の大きさが起動時の probe（sysmon のスレッドが自分へ送る 1 回）と違うとき（Intel の Mac の AVX-512、Linux の AMX）、Linux aarch64 で SME の状態があるとき（ZA の記録はどの frame にもあるので、SME のある CPU では probe で Phase 2 を使わない）、`__reserved` に収まらない生きた SVE（extra の記録。512 ビットまでは収まる）と未知の記録があるとき。生きた SVE のレジスタは保存する: 1 回目に x0〜x30・sp・pc・pstate と記録の並びを写し、2 回目に再開の frame の `__reserved` へ並びをまるごと書く（カーネルは sigreturn でそのとき frame にある記録から SVE のレジスタを読む）。記録の大きさは SVE が生きているかで変わるので、probe と書き戻し先との照合は大きさでなく SVE のベクトル長（初めは生きた SVE を止めなかったが、#91 の CI の linux-arm64 で、C ライブラリの SVE の関数が状態を生かしたスレッドの `dyn_spin` が P 1 つで終わらなかった。再開のスレッドの SVE が生きていれば形が違うとして `abort` する道もあった）。SME の CPU では、`tzroutine_runtime.c` の Phase 2 の case と `tzroutine.mjs` の信号に頼る fixture を飛ばす（ランタイムの probe でなく CPU の機能（`HWCAP2_SME`、`/proc/cpuinfo`）で決めるので、ほかの機械で probe が壊れれば失敗する。Copilot の #91 の指摘）。書き戻す frame の形が違えば `abort`（起きないはずの場合で、値を壊して続けない）。ThreadSanitizer は非同期の信号を遅らせて文脈の写しを渡すので、TSan の build では Phase 2 を作らない（`TZR_TSAN`）。`--backend fast` は Cranelift が関数をセクションに置けないので Phase 1 だけ。
14. **Windows は Phase 1 だけ**: `SuspendThread`・`GetThreadContext`・`SetThreadContext` の経路は、AVX の上位の状態（`CONTEXT_XSTATE`）と戻り方（`RtlRestoreContext`／`NtContinue`）の扱いが要り、手元で動かして確かめられない。確かめていない信号の経路は出さず、Windows は検査（Phase 1）だけにした（文書に書いた）。
15. **D3 の 10 ms**: Phase 2 の信号は、協調的な要求から 10 ms 応じないとき（最初の要求から 20 ms ほど）。仕様のとおり。

## 実装と検証（2026-11-04）

- **範囲**: Phase 1 と Phase 2 を実装した（1 つの PR、TZ08・TZ12・TZ13 の上に重ねる。実装手順の「Phase 2 は承認のあとで別の PR」は、すべての Phase の完了の指示に合わせて同じ PR にした）。Phase 1 はネイティブのすべてのターゲット（macOS・Linux・Windows、aarch64・x86-64）、Phase 2 は macOS（arm64。x86-64 の経路は Rosetta の C の単体テストで確かめた）と Linux（aarch64・x86-64）の実行ファイル。Windows・オブジェクト・共有ライブラリ・bitcode・`--backend fast`・TSan の build・`tsuzuri test`／`bench` の実行器は Phase 1 だけ。WebAssembly（JSPI）は横取りしない（対象外のとおり）。
- **ランタイム**（`src/runtime/tzroutine.c`）: P の `schedtick`（atomic）・`runm`・`run_lo`／`run_hi`・`preempt`・`seen_tick`／`seen_at`・`async_armed`、`tzr_p_running`、`tzr_preempt_clear`、大域の数 `tsuzuri_tz_preempt_pending`、sysmon の `tzr_preempt_scan`（10 ms、待つ仕事があるときだけ、応じなければ 10 ms 後に `tzr_async_kick`）、`tsuzuri_tz_preempt_check`（トラップの境界では譲らない、`errno`・`GetLastError` を保つ）。Phase 2（`TZR_ASYNC`、`-DTZ_ASYNC_PREEMPT` かつ TSan でない macOS・Linux）: M の `thread`・`async_resume`・`all_next` と `tzr_sched.allm`、代替スタックと `SIGURG` の開放（`tzr_async_thread`）、ハンドラーの設置と大きさの probe（`tzr_async_init`、sysmon のスレッド）、1 回目の停止（`tzr_async_stop`: 範囲・印・スタックの余裕・大きさの確認、文脈の保存 `tzr_async_save`、`tzr_async_redirect`）、`tzr_async_entry`（譲って 2 回目を送る。ASan では `__asan_handle_no_return`）、2 回目の書き戻し（`tzr_async_resume`・`tzr_async_load`）、それ以前の `SIGURG` のハンドラーへの受け渡し。Linux aarch64 の記録の magic と `mcontext_t` の並びは、カーネルのヘッダーがあれば `_Static_assert` で照合する。
- **生成**（`src/llvm_preempt.rs`、新規）: `with_preemption_checks`（ブロック・支配木・自然なループ、閉路と重さの分類（Tarjan）、`@tz.preempt.poll`、スレッドの記憶域を読む関数の `noinline`）、`opaque_functions`（ソースの閉路と `dyn` の dispatch）、`with_generated_section`（Phase 2 のセクション）。`src/llvm.rs` の `emit_program_in` が、spawn するネイティブのプログラムで最後に呼ぶ。`src/driver.rs` は spawn するネイティブの実行ファイル（macOS・Linux、`--backend fast` を除く）でセクションを付け、ランタイムを `-DTZ_ASYNC_PREEMPT` で作る。
- **テスト**: `tests/tzroutine_runtime.c` に `preempt_flag`・`preempt_sysmon`（Phase 1。検査のない版はウォッチドッグで止まることを手で確かめた）、`preempt_async`（セクションの中の呼び出しのないループを、P と同じ数だけ走らせてもタイマーの G が 20 回起きる）、`preempt_async_values`（整数 8 個と浮動小数点 8 個の累算をレジスタに持つループを 6 つの G で走らせ、止まった回数が 0 より多く、結果が止められないメインのスレッドの同じ計算とビットまで一致）、`preempt_async_sve`（Linux aarch64 で SVE があれば、SVE の述語とベクトルのレジスタを持ったまま回るセクションのループが止まって、どの値も変わらずに続く）、`preempt_async_runtime`（同じループをランタイムの C に置くと、信号は届くが 1 度も止めない）。`tests/tzroutine_runtime.mjs` はすべての build を `-DTZ_ASYNC_PREEMPT` で作る（Windows と TSan では Phase 2 の 3 つの case は「not built」）。`tests/preemption.rs`（新規、6 件: tzroutine を使わないプログラム・Channel だけ・Parallel だけには検査がない、再帰の呼び出しの前に 1 つで基底の道にない、ランタイムを呼ぶループに 1 つ、葉・呼び出しのないループ・小さな関数とラムダを呼ぶループ・spawn だけの関数には 0、`extern` を呼ぶループは 1 つで、ブロッキングの区間の中にはない、閉包を通した再帰に検査、確保だけのループに 0、`-O3` で plain の版にある `vector.body` がすべて残る）と `src/llvm_preempt.rs` の単体テスト 5 件。`tests/fixtures/preemption/Main.tz` と `tests/tzroutine.mjs` の `busy`（busy_does_not_starve: `fib 25` を回す間に 10 ms の `Ticker` を 50 回）、`tight`（同じことを呼び出しのないループで。Windows では走らせない）、`values`（values_unchanged: 3 つの tzroutine の `fib`・配列の和・鎖が、tzroutine なしの同じ計算と、JavaScript で独立に計算した値に一致）を `-O0`・`-O3` × P 1・4 で。下限と値だけで、時間の閾値はない。`benchmarks/tzroutine/Main.tz` と `benchmarks/run-tzroutine.mjs`（`--quick` は結果の照合だけで、`targets.yml` の benchmark の段で走る）。
- **計測**（S3、D1）: `docs/benchmarks.md` の「tzroutine の横取りの費用（TZ10）」。Apple M3、変更前の HEAD `16bae89f` の release のコンパイラと交互に 15 サンプルを 2 回。採用した位置で 6 つの核すべてが −1.4 %〜+0.7 %（Copilot のレビューを終えた `0a71a376` で測り直すと −1.2 %〜+2.4 %。最大は `fib` の +2.4 %）。仕様どおりの位置（`fib` +10 %、`sort` +34 %、ベクトル化の喪失）とほかの 4 つの選択肢の値も記録した。
- **実行したコマンドと結果**: `cargo fmt --all -- --check`、`cargo clippy --all-targets --locked -- -D warnings`、`cargo nextest run --locked --no-fail-fast`（2927 件すべて成功、4 件 skip）、`TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`（676 runs。-O0・-O3 × P 1・2・4、`stack.c` との build、Rosetta の x86-64、TSan、ASan + UBSan）、`node tests/tzroutine.mjs target/debug/tsuzuri`（131 runs）、`node tests/features.mjs target/debug/tsuzuri`（`tzroutine`・`tzroutine_values`・`channel`・`channel_single`・`select`・`select_single`・`timers`・`sync_types`・`sync_single`・`context`・`parallel_scheduler`）、`--features fast-backend` の `target/fast` で `tests/fast_backend.mjs`（既定・`programs`・`debug` のすべて成功。`--backend fast` の fixture の `busy` も 1 つの P で終わる）、変更したページの `check-docs`、`gen-docs --check`、`check-cli-docs`、`check-spec --require-markers`、`check-runtime-includes.sh`、zig cc での `tests/tzroutine_runtime.c` の compile（`x86_64`・`aarch64` の `linux-gnu`・`linux-musl` を `-DTZ_ASYNC_PREEMPT` あり・なし・`TZ_STACK_GUARD` あり、`windows-gnu` の 2 つ）、Linux（x86-64 glibc・aarch64 musl）への実行ファイルの cross build（`tz_text` と `__start_tz_text`／`__stop_tz_text`、セクションにあるのが生成した関数だけであることを確かめた）、Windows（MSVC の x86-64・aarch64）の IR の compile。`examples/*` の IR は、spawn しない 8 つが変更前と 1 byte も変わらず、`tzroutines` だけに 2 つの検査が入る。
- **CI だけが確かめること**: Linux の Phase 2（`targets.yml` の linux-x64・linux-arm64 の `tzroutine_runtime.mjs` と `tzroutine.mjs` の `tight`、`ci.yml` の `tzroutine.mjs`）。Linux aarch64 の生きた SVE の保存（再検討の 13）: CI の linux-arm64 は SVE のある CPU で、`tests/tzroutine_runtime.mjs` は `preempt_async_sve`（SVE の述語とベクトルのレジスタを持ったセクションのループが止まり、値を保って続く）の行を出力する（SVE のない機械では `no SVE`）。Windows の Phase 1 の runtime の case（`windows.yml`）。
- **残したこと**: Windows の Phase 2（再検討の 14）。Linux aarch64 の SME の状態の保存（SME のある CPU では Phase 2 を使わない）と、`__reserved` に収まらない長さの生きた SVE。`--allocator small` の `tz_small_realloc` の TLS は、検査より前からある tzroutine の待ちでも同じ危険がありえた（spawn するプログラムでは今回 `noinline` にした）。spawn せず `Parallel.*` を呼ぶプログラムは [TZ14](TZ14-thread-storage-across-parallel-waits.md) で塞いだ（2026-11-06。`with_thread_storage_noinline` が同じ `noinline` だけを付ける。検査は入れない。入れ子の `Parallel.init` の前後で `Vec` を伸ばすプログラムが、`-O3` の `--allocator small` で P が 8 のとき SIGSEGV になることを再現して確かめた）。
- **文書**: 言語リファレンス（`async-tasks-and-lazy/tzroutine.md` の「横取り（プリエンプション）」の節と要点、`built-in-types-and-modules/routine.md` の `Routine.yield`）、`docs/language.md`（tzroutine の「横取り（TZ10）」）、`docs/architecture.md`（tzroutine のランタイムの「横取り（TZ10）」）、`docs/benchmarks.md`、`README.md`、`.github/workflows/ci.yml`・`targets.yml`・`windows.yml` の注記と `run-tzroutine.mjs --quick`、`_features/README.md` の行。
- **変更したファイル**: `src/runtime/tzroutine.c`、`src/llvm_preempt.rs`（新規）、`src/llvm.rs`、`src/driver.rs`、`tests/tzroutine_runtime.c`、`tests/tzroutine_runtime.mjs`、`tests/preemption.rs`（新規）、`tests/fixtures/preemption/Main.tz`（新規）、`tests/tzroutine.mjs`、`benchmarks/tzroutine/Main.tz`（新規）、`benchmarks/run-tzroutine.mjs`（新規）、上の文書と workflow。
