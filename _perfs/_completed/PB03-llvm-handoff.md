# PB03: LLVM とリンカーへの受け渡しの高速化

| 項目 | 内容 |
| --- | --- |
| ID | PB03 |
| 分類 | ビルド速度 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | PX03 |
| 関連 | G17, PB01, PB05, PM08 |
| 状態 | done |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`）。2026-10-16 に HEAD `6d15b437` で再検討 |
| 承認 | 2026-10-16 の全フェーズ実装依頼で Phase 2 の D6・D7 を承認済み（効果を測定で確認できない既定の切り替えはしない） |
| 手本にする既存実装 | 根を限った到達可能性: `src/llvm.rs` の `reachable_functions`（`roots` が `Some` の経路はテストのビルドが使用中）と `emit_program` の `roots`。外部ツールの起動: `src/driver.rs` の `tool`・`run_tool`・`collect_message`。スレッドでの並行実行: `src/test_runner.rs` と `src/cache.rs` の `std::thread::scope` |
| 主な影響ファイル | `src/llvm.rs`（`Instrumentation::executable_roots`・`emit_native`・`executable_roots`・`emit_program` の型定義）、`src/llvm_traps.rs`（`ir_lines`）、`src/bundle_identity.rs`（新規）、`src/cache.rs`（`tool_identity`・`invoked_path`・`build_key`）、`src/driver.rs`（`compile_runtime_object`・`stage_runtime_object`・`final_link_args`）、`src/lib.rs`・`src/main.rs`（各 1 か所）、`tests/llvm_handoff.rs`・`tests/llvm_handoff.mjs`・`tests/handoff_tools.mjs`・`tests/bundle_identity.mjs`・`tests/fixtures/llvm_handoff/`（新規）、`benchmarks/check-handoff-equivalence.mjs`（新規）、`.github/workflows/targets.yml`、`docs/architecture.md`、`docs/benchmarks.md`、`docs/language.md`、`README.md`、`_tsuzuri/language-reference/compiler/{debugging,native-interop,option,platforms,usage}.md` |
| 計測対象 | PX03 の `benchmarks/run-build.mjs`（`build` の `O0`・`O3`）: 合成 200・1,000 モジュール、`examples/hello`。本チケットの追加の workload: W2（新規。合成 1,000 モジュールのうち 100 だけを `Main.tz` から呼ぶ）、W3（新規。`examples/tasks` の macOS `-g` 実行ファイル）、W4（新規。`tests/fixtures/tasks` の `--target wasm32 --wasm-feature threads`）、W5（新規。`tests/fixtures/tasks` の `--emit object`）。指標は `wall_time`・`phase_time`（`emit`・`tool.clang`）と、Clang に渡した IR の bytes |

## 目的

Tsuzuri が IR を Clang とリンカーへ渡す経路（受け渡し）の時間を、出力の意味と IR の決定性を変えずに減らす。
LLVM の C API／Rust バインディングに結合せず、テキストの IR と Clang／LLD の CLI を境界にする方針（`docs/architecture.md` 冒頭）は変えない。

担当の境界は次のとおり。

- PB01: ランタイム（`numeric.ll`、C ランタイム）の事前ビルドと cache。PB03 はランタイムのコンパイルを減らさない。
- PB07: IR の codegen unit への分割と unit の並列コンパイル。PB03 は IR を分割しない。
- PB02: フロントエンドの確保と、IR の生成中の書式化・文字列検索（`emit_program` の `output.contains(..)` によるランタイムの連結の判定を含む）。
- PB03（本チケット）: Clang に渡す IR の量（実行ファイルで使われない公開関数）、1 回のビルドの外部プロセスの数と並行性、IR 全体の複写を伴う後処理、IR の渡し方・Clang のフラグ・リンカーの選択の判断と記録。

2026-10-16 の依頼で全 Phase（Phase 2 の D6・D7・D9 を含む）を実装する。起票時の「Phase 1 だけ」「承認後だけ」の記述はこの依頼には適用しない。
意味の保持と検証の条件は維持する。現行のコードと食い違う記述は、次の「着手時の再検討」を優先する。

## 着手時の再検討（2026-10-16）

HEAD `6d15b437`（第2フェーズの PR #26 を含む）で、起票時の記述を現行のコードと実測で照合した（Apple clang 21.0.0・Homebrew LLD 23.1.1・
M1 Max。調査中の共有機の load average は 11〜112 で、時間は参考値）。起票は PB01・PB02・PM06・PM07・PR01・PR04・PR05・A17 より前で、前提が大きく変わっている。

| # | 起票時の記述 | 現行（確認済み） | 扱い |
| --- | --- | --- | --- |
| 1 | PB01 は未完。ランタイムは HEAD の `runtime` の `Command` | PB01 完了。`compile_runtime_object`（同梱 → `runtime/` cache → Clang）、`runtime_objects`（`cache` かつ native 実行ファイルまたは wasm32 の `.wasm`、トラップ・debug・threads・stack 検査なし）、numeric・math の object、`-Wl,-dead_strip` | D4 は解決。プロセス表を作り直す |
| 2 | task.c の 0.05〜0.12 s を並行で隠す（D2） | `native-c` は cache され steady state でプロセスがない。常に直接 compile するのは `threads.c`・`wasi.c`・`stack.c`（`-g`）と、cold・`--no-cache` の C ランタイム | 先に cache 化（WP2）。並行は残る分だけ・計測ゲート付き |
| 3 | `tool.clang` が和になり `other` が負になりうる。判断は `wall_time` | `tests/time_passes.rs`・`metrics.mjs` の `parseTimePasses`・`run-build.mjs` の `breakdown` が「段の和 ≤ total」を検査し、option.md は二重計上をしないと定める | 並行する tool は組全体を 1 つの `tool.<名前>` 区間で計上する |
| 4 | Windows では並行実行が起きない | wasm の threads・WASI・numeric／math object は Windows の host でも走る | Windows は CI でしか確認できない。両 Windows target の `cargo check` を保つ |
| 5 | `emit_native_build` に `executable_roots` を足す。既存テストへの影響なし | `tests/multiversion.rs` が `emit_native_build_for(Entry::Console)` を使う。`EmitOptions` の構築は 33 か所 | 剪定は `emit_native_build` からだけ立てる（private な内側の関数）。`_for` は不変 |
| 6 | `reachable_functions(module, None)` | 現行は `(module, roots, benches)`。`program_reach` が drop glue を根に足し、`Bench.now` の規則（G18 D3）と `clock_error(roots)` を持つ | `executable_roots` は export と entry だけ。drop は `program_reach` が足す |
| 7 | 後処理の複写 3 か所と `output.contains` | `contains` は PB01 の `RuntimeSymbols` に置換済み。`replace` は `!runtime_objects` のみ、`lines().join` は `tz_math_` のとき。他に `has_recursion`（IR の 2 走査。どの段にも入らない）、`unique_declarations`、`with_vector_alignment` | D8 の対象と M7 の分母を更新。release は `strip = true` なので sample には非 strip の build が要る |
| 8 | `emit_wasm_threads_build` | `emit_wasm_build(…, threads, memory64, stack_checks)`。`--emit shared`・`trap_return` の経路が増えた | 関数名と D9 の変更箇所を更新 |
| 9 | 失敗の hint を E2E で検査 | 既定（cache あり）の C ランタイムの失敗は `runtime objects require Clang with support for the selected target`。旧 hint は `--no-cache` と `merge_debug_ir` だけ。主の Clang は `cross_hint` で包まれる | E2E の期待文を経路ごとに書く |
| 10 | 既存テストへの影響なし | `prebuilt_runtime.mjs` は warm で Clang が 1 回だけと検査し、`cache.mjs` は compile の数を、`smoke.mjs` は同梱 object の消費を検査する | 検査用の追加プロセス・runtime key と引数の変更は不可 |
| 11 | D6: Linux の既定リンカーを lld、macOS は Apple ld | 配布物の `tsuzuri-clang` は全 OS で LLVM clang `-c` → `zig cc`（内蔵 LLD）。launcher は `-fuse-ld=lld` を捨て、`*.ll` の path を要し、LLVM clang へは限った flag だけ渡す。macOS では ld64.lld が Apple ld より遅い（1,000 module で 84 ms と 109 ms） | opt-in の `TSUZURI_LINKER=lld` だけ。既定は変えない |
| 12 | 計測は PX03 の `build`（`--no-cache`） | `--no-cache` は `runtime_objects=false`（既定でない経路）。`run` は `--trap-info` が既定で runtime object を使わない。既定の経路は `build-runtime-{cold,warm}` | 計測に `build-runtime-warm` と trap-info の variant を足す |
| 13 | M1「hello・合成 1,000 は before と byte 単位で同じ」。数値は 464,271 B・29 MB | 全 public が使われるので成り立つ。数値は変わった: hello の exe IR は `--no-cache` で 740,282 B、runtime object 版で 121,933 B（+ numeric.ll 619,488 B）、`run` で 758,166 B。合成 1,000 は 139,010 行・37.3 MB。`--emit llvm` の hello は 740,290 B（起票時は 3,126 B） | 数値を取り直す |
| 14 | PM08 の `-O{}` は 3 か所 | `format!("-O{}")` は driver に 5 か所（numeric・math、C、WASI、threads、主） | PM08 の機械的な置換は PB03 の後 |

実測（再現手順は「計測手順」と実装記録）: 剪定の上限は -O0 で Clang 246 → 72 ms（3.4 倍）、-O3 で 343 → 303 ms（1.13 倍）。Clang の front end は -O0 で 5〜9%、-O3 で 3%（機械語生成が -O0 の 88%）。
`.ll` を `.bc` にしても Clang の時間は 1〜2% しか変わらず、`llvm-as` は Clang の -O0 の 35%。compile と link の分割は hello で +25 ms。ld64.lld は ld64 より約 30% 遅い。

決定（起票時からの変更）:

- **D1**: 採用。`emit_native_build`（`Entry::Console`）だけ。`emit_native_build_for` と `emit`・`emit_target`・`emit_with_options` は不変。`-g` も剪定する。
- **D2**: 縮小。まず `threads.c`・`wasi.c`・`stack.c`（`-g`）を cache 化して、プロセス自体を消す。並行は残る独立な compile だけで、組全体を 1 区間で計上し、wall の改善が広がりを超えたときだけ入れる。実行ファイルの compile と link の分割はしない。
- **D3・D5**: 変えない。配布物の launcher が `*.ll` の path を要し、LLVM clang へ渡す flag を限っている。
- **D6**: `TSUZURI_LINKER=default|lld` の opt-in だけ。未設定は現行。lld が見つからなければ Clang のエラーを E2002 で出し、黙って戻さない。Linux の既定化は Linux の実測が出るまでしない。
- **D7**: 採用しない。上の数値を記録する。
- **D8**: 計測で決める。見込みは 2% 未満。**D9**: D1 が安定した後、条件付きで実装（最後）。**D10**: 対象外のまま。
- **D11（新規）**: 並行する tool は組全体を 1 つの `tool.<名前>` 区間で計上する。**D12（新規）**: 計測に `build-runtime-warm` と `--trap-info` の variant を含める。
- **WP-F（新規。範囲外の発見の修正）**: 同梱 resource の識別子の費用（下記）を、検証済みの memo で O(1) に近づける。

範囲外の発見（記録。★は本チケットで修正する）:

- ★ 配布物（約 19,600 file・759 MB）と cache を使うと、`bundled_resource_fingerprint` と `bundled-zig-binary`（185 MB）が毎ビルド全件ハッシュされる（`cache` 段が 33〜165 s。cache hit でも）。launcher の `--version`（LLVM clang と `zig cc` の 2 プロセス）も毎ビルド 約 0.25 s。
- `run` は `--trap-info` が既定で runtime object を使わない。hello の IR は 758 KB（`build` は 122 KB）で、Clang が -O3 で 675 ms。計装済みの numeric を別の object にする案（PB01 D5 の「将来の候補」）は別チケットにする。
- whole-build cache の hit の固定費は 約 90 ms（コンパイラ本体 9 MB のハッシュと `--version` の起動）。frontend cache の stat による識別の流用が候補。
- ★ Homebrew の `wasm-ld` は `lld` への symlink で、`tool_identity` が `executable_path`（`canonicalize`）経由で `lld --version` を呼んで失敗し、wasm の whole-build cache が黙って無効になる。`--version` は symlink を辿らない `invoked_path` の名前で呼ぶ（file の識別は従来どおり実体）。

## 着手条件と停止条件

### 着手条件

- PX03 が `_perfs/README.md` の状態欄で done であること（PX03 は PX01 に依存するので、PX01 の `TSUZURI_TIME_PASSES` も使える）。
  確認: `grep -n "PX01\|PX03\|PB03" _perfs/README.md` と、次の最後の行に `"name":"tool.clang"` があること。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
TSUZURI_TIME_PASSES=1 target/release/tsuzuri build examples/hello -O0 --no-cache -o /tmp/tz-pb03/hello 2>&1 | tail -1
```

- PB01・PB07 は着手条件にしない。PB01 が先に done なら、C ランタイムの object は PB01 の `runtime_object`（PB01 で新規）が作る。
  手順 6 の並行実行は、その呼び出しを HEAD の `runtime` の `Command` の代わりに包む（D4）。
- GUIDE §2.3 の基準コマンドが成功し、GUIDE §14 の性能チケットの共通手順に従って before のコンパイラを保存していること（手順 1）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. 根を限った実行ファイルのビルドで、Clang が `use of undefined value '@tz.fn.…'` などを出す、またはリンクが未定義の記号で失敗する。
   `reachable_functions` の `references` が辿らない参照の経路があるということなので、根を足して隠さない。`TypedExprKind` の variant と再現を報告する。
2. 既存のテストの期待値を変える必要が出た（Rust テストの IR・トラップの数・DWARF の検査、E2E の出力）。PB03 は `llvm::emit`・
   `emit_target`・`emit_with_options`・`emit_with_trap_info`・`emit_with_debug_info`・`emit_wasm_build`・`emit_native_build_for` の出力を変えない。
3. 並行実行で、`messages` の順序・報告する診断（どちらのツールの失敗を報告するか）が HEAD と変わる、または `unsafe`・新しい crate
   （`rayon`・`tempfile` など）が必要になった。
4. 手順 1 の IR の byte 比較（`--emit llvm` と Rust テストの IR）で差が出た。
5. IR の渡し方（stdin）、Clang のフラグ、既定のリンカーを変えたくなった。どれも既定では変えないと決めてある（D3・D5・D6）。計測の結果を報告する（opt-in の D6 は可）。
6. どれかの workload で `wall_time` か `tool.clang` の中央値が before の広がり（最小〜最大）を超えて悪化した。
7. 計測のために `src/` を変える必要が出た（計測専用の CLI オプション、隠し環境変数など）。IR の採取は `TSUZURI_CLANG` の wrapper（計測手順）で行う。
   ただし並行する tool の計上（D11）は製品の不変条件（段の和 ≤ total）なので `src/` を変えてよい。
8. （2026-10-16 追加）逐次の経路の `tool.*` などの段の名前・計上の仕方が変わる。
9. （2026-10-16 追加）既存の runtime object の source・引数・`KEY_ENVIRONMENT` が変わり、同梱 object が一致しなくなる。
   `prebuilt_runtime.mjs` の「warm の Clang は 1 回」、`cache.mjs` の compile の数、`smoke.mjs` の同梱 object の消費の期待値も変えない。

## 現状と計測（HEAD `f8dc655`）

### 受け渡しの経路（コードで確認）

- `src/driver.rs` の `build_complete` が IR を 1 つの `String` として作る。native の実行ファイルと `--emit object` は
  `llvm::emit_native_build`、`--wasm-feature threads` は `emit_wasm_threads_build`、`-g` は `emit_with_debug_info`、`--trap-info` は
  `emit_with_trap_info`、それ以外は `emit_with_options`。`EmitOptions::entry` は `Emit::Executable` のときだけ `Entry::Console`、
  他は `Entry::Library`（`--emit llvm`・`--emit wasm` を含む）。`tsuzuri test` は `src/test_runner.rs` が `llvm::emit_test_runner`
  （`Entry::TestRunner`）と自前の Clang・`wasm-ld` の起動で作る。
- 根: `emit_program` は `tests` が `Some` のときだけテスト関数を根にし、それ以外は `reachable_functions(module, None)` が
  「利用者のモジュールの公開関数（テスト・入れ子・`TypedExprKind::HostCall` の本体を除く）、`exported` の関数、`module.entry`」を根にする。
  到達は `references` が `TypedExprKind::Function(FunctionRef::User(id))` と `TypedExprKind::Closure(id, _)` を辿る。
  利用者の関数は `define internal` で出るので、実行ファイルでどこからも呼ばれない公開関数は外から見えないのに、`-O0` ではすべてコンパイルされる。
- 後処理: 関数の後、`emit_program` が IR の文字列を検索してランタイム IR を連結する（PB02 の範囲）。IR 全体を複写する箇所は 3 つ:
  `output.contains("@tz_soft_")` のときの `output.replace("declare void @llvm.trap()\n", "")`、`output.contains("@tz_math_")` のときの
  `lines().filter(..).collect::<Vec<_>>().join("\n")`（`declare` の重複除去）、`build_complete` の wasm32 SIMD の
  `text.insert_str(0, "; wasm-feature: simd128; ...")`。Windows だけ `llvm::windows_abi` も複写する。
- 書き出し: `build_complete` が `temporary.path.join("module.ll")` へ `fs::write` し、Clang に path で渡す。`TemporaryDirectory` は出力先の
  親に `.tsuzuri-<pid>-<n>` を作り、`close` か `Drop` で消す。失敗したビルドでも消えるので、Clang のエラーに出る path はビルドの後には存在しない。
- Clang の既定: Apple clang 21 のドライバーは `-x ir` の入力に対し、cc1 へ `-disable-llvm-verifier`・`-discard-value-names`・`-disable-free` を
  すでに渡す（`clang -### -x ir -O0 -c hello.ll` で確認）。
- 外部プロセス: すべて `tool`（環境変数と既定名）と `run_tool`（`Command::output` で完了を待つ）で、1 つずつ順に動く。

### 1 回のビルドの外部プロセス

「C ランタイム」は `native_runtime`（`task_runtime`・`cpu_runtime`・native の `io_runtime` のどれか）。`clang` の IR のコンパイルは
`-x ir -Wno-override-module -O<n>` を持つ。

| 構成 | 外部プロセス（左から順に 1 つずつ） | 独立な組 |
| --- | --- | --- |
| native 実行ファイル（C ランタイムなし、`-g` なし） | `clang module.ll -o exe`（リンクを含む） | なし |
| native 実行ファイル（C ランタイムあり、`-g` なし） | `clang task.c -c` → `clang module.ll task.o -o exe` | なし（リンクが `task.o` を読む） |
| native 実行ファイル、macOS の `-g`（`dwarf_sidecar`） | [`clang task.c -c`] → `clang module.ll -c` → `clang`（リンク）→ `dsymutil --flat` | `task.c` と `module.ll` |
| native `--emit object`（C ランタイムなし） | `clang module.ll -c` | なし |
| native `--emit object`（C ランタイムあり、`merge_debug_ir` でない） | `clang task.c -c` → `clang module.ll -c` → `clang -r -nostdlib` | `task.c` と `module.ll` |
| native `--emit object`（C ランタイムあり、macOS の `-g` = `merge_debug_ir`） | `clang task.c -S -emit-llvm` → `llvm-link` → `clang -c` | なし |
| `--emit wasm` | `clang module.ll -c` → `wasm-ld` | なし |
| `--emit wasm`・`--emit object` の `--wasm-feature threads` | `clang threads.c -c` → `clang module.ll -c` → `wasm-ld`（object は `wasm-ld -r`） | `threads.c` と `module.ll` |
| `--emit llvm`・`header`・`wgsl` | なし | – |

### 計測済みの事実

2026-09-29、合成 1,000 モジュール（124,003 行）、Apple M1 Max、`--no-cache`、各 1 回（`_perfs/README.md` と同じ値。生成器が
リポジトリになく再現できない。PX03 の生成器の値と直接比べない）:

- IR のテキストは 29 MB（1 行あたり約 233 bytes）。実行ファイルのビルドで Clang の処理は `-O0` 3.70 s（全体 約 6.2 s の約 60%）、
  `-O3` 7.62 s（全体 約 10.1 s の約 75%）。`check` 2.12 s、`--emit llvm -O0` 2.58 s なので、IR の生成と書き出しは約 0.46 s。
- PB02 のサンプリングでは、IR 出力の実行のうち `build_complete` が約 25%。書式化（`fmt::write`）と `str::contains` が目立つ。

2026-09-30 の probe（本チケットで確認。Apple clang 21.0.0（clang-2100.3.34.2）、Homebrew LLD 23.1.1、各 1〜3 回）:

- `examples/hello` の実行ファイルの IR（`TSUZURI_CLANG` の wrapper で採取）は 464,271 bytes、`define` 35 個、`tz_soft_` を含む。
  `--emit llvm -O0` の IR は `Entry::Library` なので 3,126 bytes で `main` がなく、リンクすると `_main` が未定義になる。実行ファイルの IR を
  `--emit llvm` で代用しない。
- `clang -x ir - < hello.ll` は受け付けられる。構文エラーは file のとき `bad.ll:2:7: error: value doesn't match function result type 'i32'`、
  stdin のとき `<stdin>:2:7: error: ...` で、終了コードはどちらも 1。
- 29 MB の書き出し（`dd if=/dev/zero bs=1048576 count=29`）は 0.01〜0.03 s。6.2 s の 0.5% 未満。
- C ランタイムのコンパイル（`clang -std=c11 -c -O0 -g -pthread src/runtime/task.c`）は 0.05〜0.12 s。
- `ld64.lld` は `-r` を実装していない（`clang -fuse-ld=lld -r -nostdlib f.o -o r.o` が `Option '-r' is not yet implemented` を出して失敗）。
- hello の IR の `clang -O0 -c` は 0.02 s。`-ftime-report` は `Clang time report`・`Pass execution timing report` などの表を出す。
- W3（`examples/tasks -O0 -g`）は 0.25〜0.47 s。2 回のビルドの実行ファイルは byte 列が異なる（HEAD のまま。一時ディレクトリの path が
  debug map に入るためと推定。未確認）。W4（`tests/fixtures/tasks --target wasm32 --wasm-feature threads -O0`）は 0.26〜0.46 s、W5
  （`tests/fixtures/tasks --emit object -O0`）は 0.24〜0.25 s で、どちらも 2 回のビルドで同一。`task-wasm-threads.c` のコンパイルは 0.03 s。

### 再現（2026-09-30 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-pb03/ir
printf '#!/bin/sh\nfor a in "$@"; do case "$a" in *.ll) cp "$a" "/tmp/tz-pb03/ir/$$-$(basename "$a")";; esac; done\nexec /usr/bin/clang "$@"\n' > /tmp/tz-pb03/clang-save.sh
chmod +x /tmp/tz-pb03/clang-save.sh
TSUZURI_CLANG=/tmp/tz-pb03/clang-save.sh target/release/tsuzuri build examples/hello -O0 --no-cache -o /tmp/tz-pb03/hello
/tmp/tz-pb03/hello                      # 5050
wc -c /tmp/tz-pb03/ir/*-module.ll       # 464271
clang -### -x ir -O0 -c /tmp/tz-pb03/ir/*-module.ll -o /dev/null 2>&1 | tr ' ' '\n' | grep -E 'disable-llvm-verifier|discard-value-names'
```

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する（GUIDE §14）。

- G1: native の実行ファイルの IR に、entry と `export def` から到達しない関数を出さない。未使用の公開関数を多く含むプロジェクト（W2）で
  `tool.clang` を減らす。
- G2: 独立な 2 つの外部プロセス（「1 回のビルドの外部プロセス」の「独立な組」）を並行に動かし、W3・W4・W5 の `wall_time` を短い方の
  プロセスの時間だけ減らす。
- G3: それ以外の workload（hello、合成 200・1,000。未使用の公開関数も独立な組もない）を悪化させない。
- G4: Clang の内訳（解析・最適化・コード生成）、リンクの時間、IR の後処理の割合を記録し、D3・D5〜D8 の判断を数値で残す。
- 長期目標（旧版から維持）: 124K 行の `-O0` ビルドで LLVM とリンクの時間を 1 s 以下にする。PB01・PB07 と合わせた目標で、
  PB03 単独では合成 1,000（未使用の公開関数がない）に効かない。

| 指標 | 単位・統計 | 対象 | 期待（計測で確かめる） |
| --- | --- | --- | --- |
| M1 IR の大きさ | bytes と `^define` の行数（1 標本。決定的） | Clang に渡した `module.ll`（wrapper で採取）: hello、合成 1,000、W2 | hello・合成 1,000 は before と byte 単位で同じ。W2 は `define` が減る |
| M2 Clang の時間 | `tool.clang` の ms（warm-up 1 回の後の 9 標本の中央値・最小・最大） | `build` の `O0`・`O3`: hello、合成 200・1,000、W2 | W2 で減る。他は広がりの範囲内 |
| M3 ビルドの時間 | `wall_time` の ms（同） | M2 の対象と W3・W4・W5 | W2〜W5 で減る。他は広がりの範囲内 |
| M4 IR の生成 | `emit` の ms（同） | M2 の対象 | 悪化しない（W2 は減る） |
| M5 Clang の内訳 | `-ftime-report` の `Clang time report` と `Pass execution timing report` の上位 10 行の wall 秒（3 標本の中央値） | 合成 1,000 の実行ファイルの `module.ll`、`-O0`・`-O3` | 記録だけ（D5・D7） |
| M6 リンクの時間 | ms（9 標本の中央値・最小・最大） | 合成 1,000 の `module.o` から実行ファイルへのリンク（Apple ld。参考に `-fuse-ld=lld`） | 記録だけ（D6） |
| M7 後処理の割合 | `sample` の samples の割合（%） | 合成 1,000 の `--emit llvm -O0` | 記録し、D8 の判定に使う |

- M1 の「before と byte 単位で同じ」は、合成と hello の公開関数（合成の `run`、`main`）がすべて使われていることからの予想。差が出たら、
  消えた `define` が本当にどこからも呼ばれない公開関数かを確かめる。それ以外の差は停止条件 1。

## 変えてはいけない意味

- 実行ファイルの振る舞い: stdout・stderr・終了コード、トラップの種類と報告する位置、整数の overflow・丸め・NaN・符号付きゼロ・評価順序、
  所有権と解放（`live == 0`）。根を限っても、到達する関数の本文の生成規則は変わらない。`-O3` のインライン展開の判断は呼び出し元の数で
  変わりうる（機械語は変わるが、意味は変わらない）。
- 変えない出力: `--emit llvm`・`--emit object`・`--emit wasm`・`--emit header`・`--emit wgsl`、`tsuzuri test`、Rust テストが使う
  `llvm::emit`・`emit_target`・`emit_with_options` などの IR（byte 単位）。`check`・LSP の診断（未使用の公開関数の型エラーは今までどおり報告する）。
- 実行ファイルの `export def`: 根に含めるので `tz_<name>` の定義は残る。
- IR の決定性: 根は関数 id の昇順、到達集合は `BTreeSet`。同じ入力から同じ IR を出す。
- 実行ファイルの DWARF とトラップ表: 出力しない関数の項目はなくなる。出力する関数の file・行・列は変わらない。
- 並行実行: 起動するコマンドと引数、一時ファイル名、成果物は HEAD と同じ。`messages` の順序（ランタイム、主の Clang の順）と、失敗の報告
  （両方が失敗したらランタイム側の `E2002`）も同じ。
- 既定の WASM import、環境変数、CLI オプション、外部ツールの要件（Clang 17+・`wasm-ld`・`llvm-link`・`dsymutil`）を増やさない。
  `build_key` の形式は変えない（IR が変わる実行ファイルは鍵が変わり、初回だけ cache miss になる）。
- `unsafe` を使わない。crate を足さない。

## 設計

### 根を限る（D1）

新 API（実装後に有効。未検証）:

```rust
// src/llvm.rs: Instrumentation（#[derive(Default)]）に field を足す。既定は false
executable_roots: bool,

// emit_native_build の Instrumentation のリテラルに足す（emit_wasm_threads_build・emit_with_debug_info・
// emit_with_trap_info・emit_test_runner は足さない。..Instrumentation::default() の箇所は false のまま）
executable_roots: options.entry == Entry::Console,

// emit_program の roots
let roots = tests
    .map(|selected| {
        selected
            .iter()
            .map(|index| module.tests[*index].function)
            .collect::<Vec<_>>()
    })
    .or_else(|| instrumentation.executable_roots.then(|| executable_roots(module)));

/// Entry and exported functions: the only code a native executable exposes.
fn executable_roots(module: &CheckedModule) -> Vec<usize> {
    module
        .functions
        .iter()
        .enumerate()
        .filter(|(id, function)| function.exported || module.entry == Some(*id))
        .map(|(id, _)| id)
        .collect()
}
```

- `Entry::Console` は `build_complete` が `Emit::Executable` のときだけ選ぶので、対象は native の `build`・`run` の実行ファイルだけ。
  `emit_program` は `Entry::Console` のとき `validate_main` を通すので、`module.entry` は必ず `Some`。
- `reachable_functions` は変えない。`roots` が `Some` の経路は、テストのビルドで HEAD から使われている。
- `named_types(module, &emitted)` は `emitted` から型を集めるので、消えた関数だけが使う型の定義も IR から消える（起票時の想定。**誤りだった**: drop の builtin は、出力する本体に現れない型を読む。
  剪定した実行ファイルは、剪定しないプログラムが定義する型をすべて定義する。「レビューでの指摘と修正」の指摘 1）。

### 並行実行（D2）

新 API（実装後に有効。未検証）:

```rust
// src/driver.rs
/// Runs two independent tools concurrently; reports the first one's failure first, as the sequential order did.
fn run_tool_pair(
    first: (&mut Command, &str),
    second: (&mut Command, &str),
) -> Result<(String, String), Diagnostic> {
    std::thread::scope(|scope| {
        let (command, hint) = first;
        let first = scope.spawn(move || run_tool(command, hint));
        let second = run_tool(second.0, second.1);
        let first = first
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
        Ok((first?, second?))
    })
}
```

`build_complete` の変更:

- `overlap_runtime`（新規の局所変数）= `native_runtime && !merge_debug_ir && (options.emit == Emit::Object || dwarf_sidecar.is_some())`。
  このとき主の Clang は `-c` で `module.o` を出し、`task.o` を読まない（「1 回のビルドの外部プロセス」の表）。
- `overlap_runtime` なら C ランタイムの `runtime` の `Command` をその場で `run_tool` せず、`deferred: Option<(Command, &'static str)>`
  （新規の局所変数）に置く。`options.wasm_threads` の `threads.c` の `Command` は常に `deferred` に置く（両者は native と wasm32 で排他）。
- 主の Clang の起動で、`deferred` が `Some` なら `run_tool_pair((&mut runtime, hint), (&mut clang, CLANG_HINT))` を呼び、戻り値の 2 つを
  この順に `collect_message` する。`None` なら HEAD と同じ `run_tool(&mut clang, ..)`。hint の文字列は HEAD と同じ
  （`native runtime requires Clang and the platform C/OS SDK headers`、`WASM threads require Clang atomics and bulk-memory support`、
  `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable`）。
- `task.c`・`threads.c` の書き出し（`fs::write`）は HEAD の位置のまま、起動の前に行う。同時に動くのは最大 2 プロセスで、スレッドは 1 つだけ増える。
- ランタイムが失敗しても主の Clang は最後まで動く（HEAD では起動しなかった）。報告する診断は同じで、途中の object は
  `TemporaryDirectory` の `Drop` が消す。

### IR の後処理（D8。手順 8 の計測で判定）

M7 で `emit_program` の全体の複写（`str::replace`・`join`）の samples が `--emit llvm` の全 samples の 2% 以上のときだけ、次の 2 つを入れる。
出力は byte 単位で HEAD と同じにする。

- `@tz_soft_` の分岐: `output.replace(TRAP, "")` を、`output.find(TRAP)` の位置を `replace_range` で消すループにする
  （`TRAP` = `"declare void @llvm.trap()\n"`。行全体を消すので、消した後に新しい一致は生じない）。
- `@tz_math_` の分岐: `lines().filter(..).collect::<Vec<_>>().join("\n")` と直後の `push('\n')` を、`String::with_capacity(output.len())` へ
  残す行と `'\n'` を 1 回の走査で書く形にする。`declare` の名前の取り出し（`split_once('@')` と `split('(')`）は HEAD と同じ式を使う。
- `build_complete` の `text.insert_str(0, ..)`（wasm32 SIMD）と `llvm::windows_abi` は変えない（頻度が低い。Windows は計測機がない）。

### 計測の仕組み（`src/` を変えない）

- 段の時間は PX01 の `TSUZURI_TIME_PASSES=1`、workload の実行は PX03 の `benchmarks/run-build.mjs` と、PX01 の `measureProcess`・
  `makeRecord`・`writeRecords`（W2〜W5 用の scratch の script。計測手順）。
- Clang に渡した IR は、`TSUZURI_CLANG` に置いた wrapper（`/tmp/tz-pb03/clang-save.sh`。「再現」）が `*.ll` の引数を複写して採取する。
  wrapper を使うと段の名前が `tool.clang-save` になるので、時間の計測には使わない。
- M5・M6 は採取した `module.ll` に Clang・リンカーを手で当てる。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| IR 生成 | `src/llvm.rs` | `Instrumentation` | `executable_roots: bool`（新規） |
| IR 生成 | `src/llvm.rs` | `emit_native_build` | リテラルに `executable_roots: options.entry == Entry::Console` |
| IR 生成 | `src/llvm.rs` | `emit_program` | `roots` に `.or_else(..)` を足す。D8 が採用されたら `@tz_soft_`・`@tz_math_` の分岐 |
| IR 生成 | `src/llvm.rs` | `executable_roots`（新規） | 上のコード |
| IR 生成 | `src/llvm.rs` | `reachable_functions`・`named_types`・`emit_wasm_threads_build`・`emit_with_debug_info`・`emit_with_trap_info`・`emit_test_runner`・`emit_with_options` | 変更なし |
| 駆動 | `src/driver.rs` | `run_tool_pair`（新規） | 上のコード |
| 駆動 | `src/driver.rs` | `build_complete` | `overlap_runtime`・`deferred` と主の Clang の起動 |
| 駆動 | `src/driver.rs` | `tool`・`run_tool`・`collect_message`・`TemporaryDirectory` | 変更なし |
| テスト実行 | `src/test_runner.rs` | なし | 変えない（対象外） |
| テスト | `tests/llvm_handoff.rs`（新規）、`tests/llvm_handoff.mjs`（新規）、`tests/fixtures/llvm_handoff/`（新規） | テスト計画 | 根と並行実行の検査 |
| 文書 | `docs/architecture.md`・`docs/benchmarks.md`・`_perfs/README.md` | ドキュメント | 経路と計測の記録 |

### Phase 分割

- Phase 1（実装対象）: D1・D2、D8 の判定（採用なら実装）、D3・D5・D6・D7 の判断材料（M5・M6・M7）の記録。
- Phase 2（設計方針。人間が求めた場合だけ）: `--emit wasm`・`--emit object` の根を限る（D9）、Linux の既定リンカー（D6、要承認）、
  bitcode での受け渡し（D7、要承認）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。重い BigInt の suite は Node 24 で動かす
（`npx --yes --package=node@24 node tests/<suite>.mjs target/release/tsuzuri`）。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、before のコンパイラと `--emit llvm` の IR を保存する。
- 確認: 次が成功し、`/tmp/tz-pb03/before/` に 1 つ以上の `.ll` ができる（`Main.tz` のないディレクトリは `skip` と出る）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && cargo test --locked
mkdir -p /tmp/tz-pb03/before /tmp/tz-pb03/after
cp target/release/tsuzuri /tmp/tz-pb03/tsuzuri-before
for d in examples/*/ tests/fixtures/*/; do
  n=$(echo "$d" | tr '/' '_')
  target/release/tsuzuri build "$d" --emit llvm -O0 --no-cache -o "/tmp/tz-pb03/before/$n.ll" 2>/dev/null || echo "skip $d"
done
```

### 手順 2: workload と before の計測

- 変更: なし（scratch だけ）。
- 内容: 「計測手順」の W2 を作り、before のコンパイラで M1〜M7 を測る。
- 確認: W2 の実行ファイルの stdout が `expectedOutput(100, "12345")` と一致する。`target/perf/PB03-before-<日付>/build.jsonl` ができる。

### 手順 3: 根を限る

- 変更: `src/llvm.rs` の `Instrumentation`・`emit_native_build`・`emit_program`・`executable_roots`（新規）、`tests/llvm_handoff.rs`（新規）。
- 内容: 設計の「根を限る」のとおり。Rust テストは「テスト計画」の 3 つ。
- 確認: `cargo test --locked --test llvm_handoff` が `3 passed`。`cargo test --locked --test cpu_dispatch` と `cargo test --locked` が成功する。

### 手順 4: 変えない出力の byte 比較

- 変更: なし。
- 内容: `cargo build --release --locked` の後、手順 1 のループを出力先 `/tmp/tz-pb03/after/` で繰り返す。
- 確認: `diff -r /tmp/tz-pb03/before /tmp/tz-pb03/after` が何も出さない（停止条件 4）。

### 手順 5: E2E（根）

- 変更: `tests/llvm_handoff.mjs`（新規）、`tests/fixtures/llvm_handoff/prune/Main.tz`・`Lib.tz`（新規）。
- 内容: 「テスト計画」の E2E の 1〜5。
- 確認: `node tests/llvm_handoff.mjs target/release/tsuzuri` が終了コード 0 で、最後に `llvm handoff: roots ok` を出す。

### 手順 6: 並行実行

- 変更: `src/driver.rs` の `run_tool_pair`（新規）と `build_complete`。
- 内容: 設計の「並行実行」のとおり。PB01 が done なら D4 に従う。
- 確認: `cargo test --locked --lib` が成功する。release を build し、`node tests/tasks.mjs target/release/tsuzuri`・
  `node tests/wasm_threads.mjs target/release/tsuzuri`・`node tests/debug_info.mjs target/release/tsuzuri` が成功する。
  W4・W5 の成果物が before のコンパイラの成果物と `cmp` で一致し、W3 の実行ファイルの stdout が before と一致する。

### 手順 7: E2E（並行実行の失敗の報告）

- 変更: `tests/llvm_handoff.mjs`。
- 内容: 「テスト計画」の E2E の 6〜7。
- 確認: `node tests/llvm_handoff.mjs target/release/tsuzuri` が最後に `llvm handoff: roots ok` と `llvm handoff: tool pairs ok` を出す。

### 手順 8: IR の後処理の判定（D8）

- 変更: 判定が「採用」のときだけ `src/llvm.rs` の `emit_program`。
- 内容: 「計測手順」の M7 を測り、D8 の基準で判定する。採用なら設計の「IR の後処理」を入れ、手順 4 の比較をやり直す。
- 確認: `/tmp/tz-pb03/emit.sample.txt` と割合の計算を残す。採用のとき `diff -r` が何も出さず、`cargo test --locked` が成功する。

### 手順 9: 全体の確認

- 変更: なし。
- 確認: `cargo test --locked` が成功する。次の suite がすべて成功する（各 suite は内部で native と WASM、`-O0` と `-O3` を回す）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && cargo build --release --locked
for s in e2e features examples tasks wasm_threads debug_info cache cpu_dispatch io computations control primitives \
  strings math display_parse numeric_casts integer_intrinsics simd wasm_simd host_imports gpu llvm_handoff; do
  npx --yes --package=node@24 node "tests/$s.mjs" target/release/tsuzuri || echo "FAILED $s"
done
```

### 手順 10: 生成コードの確認

- 変更: なし。
- 確認: 「生成コードの確認」の各パターンが期待どおり。

### 手順 11: after の計測

- 変更: なし。
- 確認: `target/perf/PB03-after-<日付>/` に before と同じ種類のファイルができる。停止条件 6 に当たらない。

### 手順 12: 判断材料と文書

- 変更: `docs/architecture.md`・`docs/benchmarks.md`・`_perfs/README.md`。
- 内容: M5・M6・M7 と D3・D5〜D8 の結論を記録し、「ドキュメント」の箇所を更新する。
- 確認: `node scripts/check-docs.mjs docs/architecture.md docs/benchmarks.md` が成功し、`git diff --check` が何も出さない。

## 計測手順

### 環境と条件

- 計測機は PX01 と同じ（Apple M1 Max、AC 電源、他の重い処理なし）。`captureRun` が CPU・OS・Clang・rustc・Node・commit・dirty を記録する。
  scratch の記録にも `sysctl -n machdep.cpu.brand_string`、`sw_vers -productVersion`、`clang --version | head -1`、`git rev-parse HEAD` を残す。
- before は `/tmp/tz-pb03/tsuzuri-before`、after は `target/release/tsuzuri`。すべて `--no-cache`、`TSUZURI_CACHE_DIR` は一時ディレクトリ。
- 時間は warm-up 1 回の後の 9 標本の中央値・最小・最大（PX01 形式、`target/perf/<run_id>/build.jsonl`）。

### workload

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node benchmarks/build/generate.mjs --modules 1000 --out /tmp/tz-pb03/w1
node benchmarks/build/generate.mjs --modules 1000 --out /tmp/tz-pb03/w2
node --input-type=module -e 'import { mainSource } from "./benchmarks/build/generate.mjs"; import { writeFileSync } from "node:fs"; writeFileSync("/tmp/tz-pb03/w2/Main.tz", mainSource(100));'
```

| workload | コマンド（`C` はコンパイラ、`T` は一時ディレクトリ） | 記録の `workload`・`variant` |
| --- | --- | --- |
| hello・合成 200・1,000 | `node benchmarks/run-build.mjs C --out target/perf/PB03-<before\|after>-<日付>`（PX03） | PX03 のまま |
| W2 | `C build /tmp/tz-pb03/w2 -O0\|-O3 --cpu generic --no-cache -o T/w2` | `pb03-w2`・`build` |
| W3（macOS だけ） | `C build examples/tasks -O0 -g --cpu generic --no-cache -o T/w3` | `pb03-w3-debug`・`build` |
| W4 | `C build tests/fixtures/tasks --target wasm32 --wasm-feature threads -O0 --no-cache -o T/w4.wasm` | `pb03-w4-threads`・`build-wasm` |
| W5 | `C build tests/fixtures/tasks --emit object -O0 --cpu generic --no-cache -o T/w5.o` | `pb03-w5-object`・`build` |

W2〜W5 は scratch の script（`/tmp/tz-pb03/measure.mjs`。リポジトリに入れない）が PX01 の `captureRun`・`measureProcess`
（`{ runs: 9, warmups: 1, timeoutMs: 600000 }`、環境に `TSUZURI_TIME_PASSES=1`）・`parseTimePasses`・`makeRecord`・`writeRecords` で
`wall_time`・`phase_time` を PX03 と同じ run のディレクトリの `build.jsonl` に足す。W2 の正しさは、stdin `12345` での stdout を
`expectedOutput(100, "12345")` と比べて確かめる。

### IR の大きさ（M1）

wrapper を `.o` と引数の記録に広げ、hello・W1・W2 の実行ファイルの IR を採取する。

```sh
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> /tmp/tz-pb03/argv.log\nfor a in "$@"; do case "$a" in *.ll|*.o) cp "$a" "/tmp/tz-pb03/ir/$$-$(basename "$a")";; esac; done\nexec /usr/bin/clang "$@"\n' > /tmp/tz-pb03/clang-save.sh
for p in examples/hello /tmp/tz-pb03/w1 /tmp/tz-pb03/w2; do
  rm -f /tmp/tz-pb03/ir/*
  TSUZURI_CLANG=/tmp/tz-pb03/clang-save.sh target/release/tsuzuri build "$p" -O0 --no-cache -o /tmp/tz-pb03/m1-exe
  wc -c /tmp/tz-pb03/ir/*-module.ll; grep -c '^define' /tmp/tz-pb03/ir/*-module.ll
done
```

結果（bytes と `define` の数）は `target/perf/<run_id>/pb03-ir.json`（補助の記録）に書く。W1 の `module.ll` と `task.o` は M5・M6 のために
`/tmp/tz-pb03/w1-module.ll`・`/tmp/tz-pb03/w1-task.o` へ残す。

### Clang の内訳（M5）とリンク（M6）

```sh
for o in 0 3; do for i in 1 2 3; do
  clang -x ir -Wno-override-module -O$o -c /tmp/tz-pb03/w1-module.ll -o /dev/null -ftime-report 2> /tmp/tz-pb03/ftime-O$o-$i.txt
done; done
clang -x ir -Wno-override-module -O0 -c /tmp/tz-pb03/w1-module.ll -o /tmp/tz-pb03/w1.o
for i in 1 2 3 4 5 6 7 8 9 10; do /usr/bin/time -p clang /tmp/tz-pb03/w1.o /tmp/tz-pb03/w1-task.o -lm -o /tmp/tz-pb03/w1-ld 2>&1 | grep real; done
for i in 1 2 3 4 5 6 7 8 9 10; do /usr/bin/time -p clang -fuse-ld=lld /tmp/tz-pb03/w1.o /tmp/tz-pb03/w1-task.o -lm -o /tmp/tz-pb03/w1-lld 2>&1 | grep real; done
echo 12345 | /tmp/tz-pb03/w1-ld; echo 12345 | /tmp/tz-pb03/w1-lld
```

- リンクの 1 回目は warm-up として捨てる。2 つの実行ファイルの stdout は `expectedOutput(1000, "12345")` と一致すること。
- `-ftime-report` の出力と時間の列は `target/perf/<run_id>/` に `ftime-O0-1.txt` などの名前で複写する。

### IR の後処理（M7）

```sh
target/release/tsuzuri build /tmp/tz-pb03/w1 --emit llvm -O0 --no-cache -o /tmp/tz-pb03/w1-lib.ll &
sample $! 10 1 -file /tmp/tz-pb03/emit.sample.txt; wait
```

- 割合 = `emit_program` の下の `replace`・`join_generic_copy`・`Lines` を含む行の数の和 ÷ main thread の総数。関数名が出ない場合は
  `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only cargo build --release --locked` の binary で測り直す。

## 生成コードの確認

- 根（fixture `prune`、after、M1 の wrapper）: 実行ファイルの `module.ll` に `@tz.fn.Lib.used`・`@tz.fn.Lib.exported`・`define i64 @tz_exported(`・
  `define i32 @main()` があり、`Lib.unused` が 0 件。before は `@tz.fn.Lib.unused` と `@tz.apply.Lib.unused.0` の 2 件（2026-09-30 に確認）。
- W2: `grep -cE '^define internal i64 @tz\.fn\.Module[0-9]+\.run\(' <module.ll>` が before 1000、after 100。
- `--emit llvm`: 手順 4 の `diff -r` が空。
- 起動する引数: W3・W4・W5 の `argv.log` を before と after で取り、`sed -E 's#\.tsuzuri-[0-9]+-[0-9]+#TMP#g'` で一時ディレクトリ名を
  そろえて `sort` した結果が一致する（並行実行は順序だけを変え、引数を変えない）。

## テスト計画

### Rust テスト（`tests/llvm_handoff.rs`（新規））

`tests/cpu_dispatch.rs` の `sources`（`TrapSource` と `tsuzuri::stdlib::SOURCES`）の組み立てを写す。源は次の 1 モジュール
（`prune` fixture を 1 ファイルにした形。構文は 2026-09-30 に確認した fixture と同じ）。

```tsuzuri
def used :: i64 -> i64 = \x -> x + 1

def unused :: i64 -> i64 = \x -> x * 3

export def exported :: i64 -> i64 = \x -> x - 1

used 41
```

- `native_executables_emit_only_entry_and_export_reachable_functions`: `emit_native_build`（`Entry::Console`）の IR が `@tz.fn.Main.used`・
  `@tz_exported`・`define i32 @main()` を含み、`@tz.fn.Main.unused` を含まない。2 回の出力が等しい。
- `library_and_plain_emission_keep_public_functions`: `emit_native_build`（`Entry::Library`）、`llvm::emit(&module, Entry::Console)`、
  `llvm::emit(&module, Entry::Library)` の IR がどれも `@tz.fn.Main.unused` を含む。
- `unused_public_functions_are_still_checked`: `unused` の本体を `\x -> true` にした源で `analyze` が `E1003`（`expected i64, found bool`。
  2026-09-30 に `check` で確認）を返し、その span が `true` の位置を含む。

### E2E（`tests/llvm_handoff.mjs`（新規））

各ケースは fixture を `mkdtempSync` の下へ複写して使う（E03 の再帰的なソース探索のため）。wrapper は suite が一時ディレクトリに書く
（`/tmp/tz-pb03` を使わない）。期待値は源から手で計算した値（41 + 1 = 42、5 − 1 = 4）。

1. `prune` の `run`（native、`-O0`・`-O3`）の stdout が `42\n`。
2. `prune` の実行ファイルの IR（wrapper で採取、`-O0`・`-O3`）が `@tz.fn.Lib.used` と `@tz_exported` を含み、`Lib.unused` を含まない。
3. `prune` の `--emit llvm` と、`--emit object` の IR（wrapper で採取）が `@tz.fn.Lib.unused` を含む。
4. `prune` の `--target wasm32 --emit wasm`（`-O0`・`-O3`）を Node で instantiate し、`WebAssembly.Module.imports` が空、`tz_exported(5n)` が `4n`。
5. 同じ入力の 2 回の実行ファイルのビルドで IR（wrapper で採取）が byte 単位で同じ。
6. 並行実行の失敗（Windows では skip）: `tests/fixtures/tasks --emit object` を、引数に `task.c` があれば `fake runtime failure` を出して
   1 で終わる wrapper で build すると、終了コードが 0 以外、stderr が `E2002` と `native runtime requires Clang and the platform C/OS SDK headers`
   と `fake runtime failure` を含み、出力 file がない。`module.ll` で失敗する wrapper では `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable`。
   両方で失敗する wrapper では runtime の hint だけを含む。`--target wasm32 --wasm-feature threads` で `threads.c` に失敗する wrapper では
   `WASM threads require Clang atomics and bulk-memory support`。
7. 並行実行の成功: W4・W5 の形の build を 2 回行い、成果物が byte 単位で同じ。

### 既存テストへの影響

なし。`llvm::emit`・`emit_target` を `Entry::Console` で呼ぶ Rust テスト（`tests/visibility.rs`・`tests/modules.rs` など）は
`emit_native_build` を通らないので IR は変わらない。実行ファイルの内部の記号・DWARF で未使用の公開関数を探すテストが見つかったら停止条件 2。

### 性能

閾値は設けない。「計測手順」の before／after を `docs/benchmarks.md` に記録するだけ。

## ドキュメント

- `docs/architecture.md`: 冒頭のパイプライン図の `-> llvm -> reachability pruning ...` の行に「native の実行ファイルは entry と export から
  到達する関数だけ」を足す。`src/driver.rs` の責務の説明に「独立な 2 つの外部ツール（C ランタイムと IR、WASM thread ランタイムと IR）を並行に起動」を足す。
- `docs/benchmarks.md`: 節「LLVM への受け渡し（PB03）」（新規）。環境、M1〜M7 の before／after の表、D3・D5〜D8 の結論と数値。
- `_perfs/README.md`: PB03 の状態を done にし、「ビルド速度」に計測結果の 1 行を足す。
- `README.md`: 変更なし（環境変数・CLI を増やさない）。

## 受け入れ条件

- [ ] native の実行ファイルの IR に、entry と `export def` から到達しない関数がない（E2E 2、Rust テスト）。
- [ ] `--emit llvm` の IR が全 example・fixture で before と byte 単位で同じ（手順 4）。
- [ ] 独立な組が並行に動き、失敗の報告と成果物が HEAD と同じ（E2E 6・7、手順 6）。
- [ ] 手順 9 の Rust テストと E2E がすべて成功する。
- [ ] M1〜M7 の before／after が `target/perf/` と `docs/benchmarks.md` に記録され、D3・D5〜D8 の結論が数値つきで書かれている。
- [ ] どの workload も `wall_time` の中央値が before の広がりを超えて悪化していない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `--emit llvm` の IR は `Entry::Library` で、実行ファイルの IR ではない（`main` がなく、未使用の公開関数を含む）。M1 と E2E の IR は
  `TSUZURI_CLANG` の wrapper で採取する。wrapper を付けた実行は段の名前が `tool.clang-save` になるので、時間の計測に使わない。
- 根の切り替えを `emit_program` の `entry == Entry::Console` だけで決めると、`llvm::emit(.., Entry::Console)` を使う多数の Rust テストの IR が
  変わる。`Instrumentation::executable_roots` は `emit_native_build` だけが立てる。
- `run_tool_pair` では `Command::spawn` と pipe の手動の読み取りを書かない（stderr が大きいと deadlock する）。`run_tool`（`Command::output`）を
  別スレッドで呼ぶ。`Diagnostic` が `Send` でなく compile できない場合は停止条件 3。
- `merge_debug_ir`（macOS の `-g` の object）は `llvm-link` が両方を読むので並行にしない。Windows では object と C ランタイムの組が
  `E2002` で先に拒まれ、`dwarf_sidecar` も macOS だけなので、Windows では並行実行が起きない。
- PX01 の `tool.clang` は並行に動いた 2 つの Clang の和で、PX03 の要約の `other`（`total` − 各段の和）が負になりうる。判断は `wall_time` で行う。
- W3 の実行ファイルは HEAD でもビルドごとに byte 列が違う。成果物の比較は W4・W5 だけで行い、W3 は stdout と `tests/debug_info.mjs` で確かめる。
- `tests/fixtures/tasks` は `main` を持たないので実行ファイルにできない（`E2004`）。W3 は `examples/tasks` を使う。
- PB01・PB07 と `build_complete` の同じ範囲を変える。後から入れる側が、先に入った変更を残したまま D2・D4 を当てる。
- `sample` は関数名が要る。symbol が出ない binary の結果で D8 を判定しない。

## 対象外

- ランタイムの事前ビルドと cache（PB01）、IR の分割と並列コンパイル（PB07、G17）、フロントエンドの確保と IR 生成中の書式化・文字列検索（PB02）、
  LLVM を使わないバックエンド（PB05）。
- `tsuzuri test` のビルド（`src/test_runner.rs`）の外部プロセス。
- LLVM の C API／Rust バインディングへの結合（`docs/architecture.md` の方針）。
- 長いグローバル名の短縮と、冗長な IR の形の除去（D10）。

## 決定事項

### D1: 根を限る範囲と実装の位置

- 決定: native の実行ファイル（`emit_native_build` に `Entry::Console` が来る `build`・`run`）だけ、根を `exported` の関数と `module.entry` に
  限る。`Instrumentation::executable_roots`（新規）で `emit_program` に伝え、`reachable_functions` は変えない。`--emit llvm`・`--emit object`・
  `--emit wasm`・`tsuzuri test`・Rust テストの `llvm::emit` 系は HEAD のまま。
- 理由: 利用者の関数は `define internal` なので、実行ファイルで使われない公開関数は外から呼べない。`--emit llvm` と Library の IR が全公開関数を
  出し続けるので、未使用の公開関数のコード生成の不具合は Rust テストと object のビルドで見つかる（旧版のリスクへの対処）。
- 状態: 既定案（実装者はこの案に従う）

### D2: 並行実行する組

- 決定: 「C ランタイムの `task.c` と IR」（`native_runtime && !merge_debug_ir` で、主の Clang が `-c` のとき）と「`threads.c` と IR」
  （`--wasm-feature threads`）だけを `run_tool_pair` で並行に動かす。同時に 2 プロセスまで。`messages` はランタイム、主の Clang の順。
  両方が失敗したらランタイム側の診断を返す。
- 理由: それ以外の外部プロセスは前のプロセスの出力を読むので独立でない（表）。C ランタイムのコンパイルは 0.05〜0.12 s、W4 の `threads.c` は
  0.03 s で、並行にすれば短い方の時間が消える。順序の規則は HEAD の逐次実行の観測結果と同じになる。
- 状態: 既定案（実装者はこの案に従う）

### D3: IR の渡し方

- 決定: `module.ll` の一時ファイルを続け、stdin（`clang -x ir -`）にしない。
- 理由: Clang は stdin を受け付け、エラーも `<stdin>:L:C` で出る（確認済み）が、29 MB の書き出しは 0.01〜0.03 s（6.2 s の 0.5% 未満）で、
  `merge_debug_ir` の `llvm-link` は path が要る。stdin にすると書き込み用のスレッドが要り、利点がない。
- 状態: 既定案（実装者はこの案に従う）

### D4: PB01・PB07 との関係

- 決定: PB01 が先に done なら、C ランタイムの object を作る PB01 の `runtime_object` の呼び出しを、D2 と同じ規則で主の Clang と並行にする
  （cache hit ならプロセスがないので並行にしない）。PB07 の分割経路（`-O0`、`-g` なし、native の実行ファイル）では D2 の組が現れないので何もしない。
  D1 の根は分割の前に効き、PB07 の入力の IR も小さくなる。
- 理由: 担当の境界（目的）を保ち、どの順で入っても同じ振る舞いにする。
- 状態: 既定案（実装者はこの案に従う）

### D5: Clang のフラグ

- 決定: Phase 1 では Clang のフラグを足さない。`-Xclang -disable-llvm-verifier` と `-fno-discard-value-names` は使わない。
  成果物を変えるフラグ（`-fno-asynchronous-unwind-tables`、`-fno-addrsig` など）は PB03 で扱わない。
- 理由: Apple clang 21 のドライバーは `-x ir` に `-disable-llvm-verifier` と `-discard-value-names` をすでに渡す（確認済み）ので前者は無意味で、
  後者は値の名前を残して遅くする。IR の構文・型の誤りは解析の段で報告される（`value doesn't match function result type` で確認済み）。
  unwind 表などを消すと debugger・profiler の結果が変わる。
- 状態: 既定案（実装者はこの案に従う）

### D6: リンカー

- 決定: Phase 1 ではリンカーを変えない。macOS は Apple ld のまま。Phase 2 で、Linux の実行ファイルに限り、`clang -fuse-ld=lld -Wl,--version` が
  成功するときだけ `-fuse-ld=lld` を付け、失敗したら HEAD の既定に戻る案を M6 と Linux の計測で評価する。
- 理由: `ld64.lld` は `-r` を実装しておらず（確認済み）、`--emit object` の `clang -r -nostdlib -Wl,-keep_private_externs` を置き換えられない。
  Linux の既定の変更は全実行ファイルの byte 列とツールの要件を変える。Linux の計測機は PX01 D8 の承認待ち。
- 状態: 要承認（承認前は Phase 2 のリンカーの変更に着手しない）

### D7: bitcode での受け渡し

- 決定: M5 で `-O0` の Clang の時間のうち IR の解析（`Clang time report` の front end に当たる行。名前は手順 12 の出力で確かめる）が 20% を
  超える場合だけ、Phase 2 で bitcode を評価する。候補は Rust での bitcode の書き出し（新しい基盤）と、外部の `llvm-as` の並列実行（Clang と
  版をそろえる要件が増える）。LLVM の C API には結合しない。
- 理由: 旧版の既定案（20%）を保つ。どちらの候補も費用が大きく、ツールの要件か保守の範囲を変える。
- 状態: 要承認（承認前は Phase 2 の bitcode に着手しない）

### D8: IR の後処理の複写

- 決定: M7 の割合が 2% 以上なら、設計の「IR の後処理」の 2 つ（`@tz_soft_`・`@tz_math_` の分岐）を入れる。2% 未満なら入れず、値を
  `docs/benchmarks.md` に記録する。`output.contains(..)` の検索は PB02 の範囲なので触らない。
- 理由: 複写 1 回は 29 MB で数 ms と見積もられ、効果が小さい可能性が高い。計測で決め、出力は byte 単位で同じに保つ。
- 状態: 既定案（実装者はこの案に従う）

### D9: `--emit wasm`・`--emit object` の根（Phase 2 の設計方針）

- 決定: Phase 2 で、`EmitOptions` に field を足して `--emit wasm` と `--emit object` にも D1 の根を使う。`--emit llvm` は Library のまま。
- 理由: `--emit wasm` は `wasm-ld` が未使用の関数を捨てるので成果物への効果は Clang の時間だけ。`EmitOptions` を構築する Rust テスト
  （`tests/cpu_dispatch.rs` など）の変更が要り、PB01 の `EmitOptions::external_numeric`（PB01 で新規）と同じ箇所を触るので、PB01 の後に回す。
- 状態: 既定案（実装者はこの案に従う）

### D10: 長いグローバル名と冗長な IR

- 決定: PB03 では扱わない。
- 理由: 記号名は debugger・profiler・トラップ表で利用者に見え、PB07 は名前から安定名と unit を決める。直後に分解する集約値や繰り返しの
  `extractvalue` は `-O0` のコードの質の問題で、生成規則の変更（PM07・PR03 の範囲）になる。
- 見直し提案: 旧版の Phase 1 にあった「冗長な IR の削減」は、M5 で `-O0` の命令選択が支配的と分かった場合に PM07 か PR03 へ移して起票する。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-16）

ブランチ `perf/phase3/PB03`（基点 `c6864cf5` = main `6d15b437` + 計画の記録）。コミット: `d0850940`（WP0）、`f2e3dc3f`（WP1）、`aa53b74f`・`bd2f9c74`（WP-F）、`74babda1`（WP2）、`993e113d`（WP5）、
それに続く文書の記録（WP7・WP9）。

### 実装した範囲と、しなかったもの

| WP | 内容 | 結果 |
| --- | --- | --- |
| WP0 | 起票時の記述を現行のコードと実測で照合し、決定を改め、`benchmarks/check-handoff-equivalence.mjs`（二つのコンパイラの差分検査）を作った | 実装 |
| WP1（D1） | `emit_native_build`（`Entry::Console`）だけ、entry・export・ユーザーの drop から到達する関数だけを出力する（`Instrumentation::executable_roots`、`emit_native`、`executable_roots`）。`emit_native_build_for`・`llvm::emit*`・wasm・library・object・shared は不変 | 実装。W2 の CPU 時間は `-O0` で −65%、`-O3` で −36% |
| WP-F（新規） | 同梱ツールチェーンの識別（全ファイルの fingerprint、Clang・Zig の SHA-256、ランチャーの `--version`）を並列に 1 回だけ計算し、`<cache>/bundle/` に控える（`src/bundle_identity.rs`）。署名が変わらない間は再利用。Homebrew の `wasm-ld`（`lld` への symlink）の `--version` も直した | 実装。毎ビルド約 8 s の CPU → 0.12 s |
| WP2（D2 の前半） | `threads.c`・`wasi.c`・`-g` の `stack.c` を `compile_runtime_object` に載せた（単位 `wasm-threads`・`wasi`・`stack-debug`） | 実装。再ビルドで Clang の 1 プロセスを削る（−23〜−113 ms の CPU） |
| WP3・WP4（D2 の後半・D4・D11） | 独立な Clang プロセスの並行実行と、組全体を 1 区間で計上する helper | **入れない**。cold の最初の 1 回だけで、理想の上限が 35〜259 ms（load の広がりの内側）。計測を `docs/benchmarks.md` に記録 |
| WP5（D6） | `TSUZURI_LINKER=default|lld` の opt-in。最終リンク（実行ファイル・`-g` の別リンク・`--emit shared`）だけに `-fuse-ld=lld`。未設定のキー・引数は不変。lld が無ければ E2002（hint 付き）、不正な値と Windows は E2000 | 実装。既定は変えない |
| WP6（D8） | IR の後処理の複写を非 strip の release と `sample` で測った | 採用しない。複写は上位に出ず、試作の改善は 1.4%（基準 2% 未満） |
| WP7 | `docs/benchmarks.md` に節を 1 つ足した | 実装 |
| WP8（D9） | `--emit wasm` の剪定 | 採用しない。成果物が同じで、効果は Clang の時間だけ。根の整合の費用に見合わない |
| D3・D5・D7 | 変えない（launcher の制約・bitcode の得が 1〜2%） | 数値を記録 |
| D10 | 対象外のまま | — |

### 確認

- 全体: `RUST_MIN_STACK=4194304 cargo test --locked`（`with-lock.sh heavy`）が成功（104 の結果グループ、1,326 件通過、失敗 0。`tests/time_passes.rs`・`tests/llvm_handoff.rs` を含む）。
  レビュー後の修正を入れた最終の tree でも成功（104 グループ、1,332 件通過、失敗 0、無視 1。増えた 6 件は型定義の 2 件、`%tz.context` の 2 件、全プロジェクトの 1 件、`stage_runtime_object` の 1 件）。
  最終の tree で、通常の release の `tests/llvm_handoff.mjs`・`tests/handoff_tools.mjs`・`tests/bundle_identity.mjs` と、変更した 5 ページの `node scripts/check-docs.mjs`（118 リンク・13 例・native 12 回）も成功。
  `cargo fmt --all -- --check`、`cargo clippy --all-targets --locked -- -D warnings`、`git diff --check` は成功。
  Windows: `PATH=$HOME/.cargo/bin:$PATH cargo +stable check --all-targets --locked --target {x86_64,aarch64}-pc-windows-msvc` が成功（Windows の挙動は Windows の CI でしか確認できない）。
- 単体: `--test llvm_handoff`（13 件。D1 の根・剪定・型検査・drop・trap 位置、レビュー後の型定義・`%tz.context`・全プロジェクトの IR）、`--lib bundle_identity`（11 件）、`--lib cache`（25 件。既存の `bundled_resource_identity_*` は変更なし）、
  `--lib the_linker_choice`（1 件）、`--lib a_runtime_object_that_is_staged`（1 件）。
- 通常の release（`cargo build --release --locked`）に対する node の suite（`validate-all.sh --skip-cargo`、55 手順）: `e2e`・`primitives`・`prebuilt_runtime`・`display_parse`・`strings`・`tasks`・`async`・
  `features`・`json`・`computations`・`control`・`lsp_sessions`・`repl`・`script`・`targets`・`f16_hardware`・`packages`・`bindgen`・`debug_info`・`frontend_cache`・`wasm_threads`・`wasm_memory`・`wasm_simd`・
  `numeric_casts`・`integer_intrinsics`・`user_drop`・`trap_boundary`・`trap_return_sync`・`stack_overflow`（20 件）・`allocator`・`cache`・`examples`・`docgen`・`host_imports`・`ffi_extensions`・`bindings`・
  `bindings_threads`・`host_bindings`・`gpu`・`strings_bridges`・`simd`・`benchmark_report`・`utf_simd`・`local_recursion_fuzz`・`last_use_fuzz` が通った。最初の sweep で失敗した 7 件は環境が原因で、
  直して再実行し通った: `io`・`os`・`trap_return`（ASan の実行ファイルが load で 180 s の timeout。load 14〜26 で 92 s・182 s・24 s）、`cpu_kernels`・`cpu_dispatch`（`validate-all.sh` が Homebrew の LLVM を PATH の先頭に置き、
  x86 向けの `clang -target x86_64-apple-macos11 cpu.c` に SDK がない。基準のコンパイラでも同じで、Apple の clang では通る）、`wasm64`・`benchmark_apps`（Node 24 が要る）。
  `node scripts/check-docs.mjs`（全ページ: 111 ページ・1,591 リンク・560 例・native 1,066 回の `-O0`／`-O3`）が成功。
- E2E（新規）: `tests/llvm_handoff.mjs`（vtable・closure・export・trap 位置、`-O0`・`-O3`、決定性）、`tests/handoff_tools.mjs`（C ランタイムの cold 1／warm 0／`--no-cache` 1、cached と fresh の byte 一致、
  失敗の hint と保存されないこと、`-g` の guard の overflow、`TSUZURI_LINKER` の引数・E2002・E2000・キー・実行ファイルと共有ライブラリ以外への無付与）、`tests/bundle_identity.mjs`（偽の配布物と spy の launcher で、
  memo の作成・再利用・無効化（ファイルの追加・ツールの入れ替え・manifest）・上書き・cache でないディレクトリへの無書き込み）。3 つとも通常の release で通り、基準のコンパイラでは失敗する。
- 差分検査（`benchmarks/check-handoff-equivalence.mjs`、基準 `6d15b437` の release との比較、通常の release）: 例・フィクスチャ・ベンチマークの 124 プロジェクト。ただしこの検査は剪定を、実行ファイルを作れる 24 件でしか通さなかった（広げた検査は「レビューでの指摘と修正」）。
  header・llvm・object・wasm・shared・wasmt（threads）・wasi の 868 比較は差 0（720 が byte 一致、148 は同じ失敗）。全 11 モードの代表 7 プロジェクト 77 比較も差 0。
  開発中の非 LTO の build では、WP1 の 8 モード 992 比較（521 が byte 一致、実行ファイル 12 比較が剪定で関数を減らし挙動は同じ）と、cache あり 3 モード 372 比較も差 0。
  threads・WASI の `--emit object` は `-O0`・`-O3` で SHA-256 が一致。`TSUZURI_LINKER` 未設定の Clang の引数列は 8 つのビルドで基準と完全に一致。
- 同梱ツールチェーン: 実物の開発用配布物（759 MB）の複製に `scripts/toolchain/runtime.mjs` の `bundleRuntime` で `share/runtime` を作り、新しいコンパイラで同梱 object が使われる（runtime cache が空）こと、
  `lib/` にファイルを足すと拒否されることを確かめた。runtime cache のキー（2 件）は旧コンパイラと同じ。`scripts/toolchain/smoke.mjs` は開発用配布物に `share/lldb`・`share/natvis` がないので実行していない（CI が担当）。

### レビューでの指摘と修正（2026-10-17）

統合ブランチへの最初のマージの後の独立レビューが、剪定の穴を 3 件指摘した。元の差分検査は剪定を、実行ファイルを作れる 24 プロジェクトでしか通さなかった。
残りの 100 件ほどは入口がなく `E2004` で止まるので、ライブラリの形のコードを剪定したビルドを、一度も見ていなかった。

**指摘 1（HIGH）: 以前通ったビルドが通らない。**

- 症状: `Main.tz` が std だけで、`Channel.Sender<Maybe<i64>>`・`Channel.Receiver<Maybe<i64>>` を使う未使用の公開関数と、トップレベルの `"ok"` を持つと、基準のコンパイラは `ok` を出すが、
  Clang が `load operand must be a pointer to a first class type` で IR を拒否する（`tests/fixtures/llvm_handoff/channel_std`。`tests/fixtures/channel` も同じ）。
- 根本原因: `named_types` は、出力する関数が名指す型だけを定義し、record・union の型引数を辿らない。`Drop` は全実行ファイルの根で、`Drop<Channel.Sender<'a>>` の builtin は
  `'a`（型引数にしか現れない `Maybe<i64>`）を読む。その型を名指すのが未使用の公開関数だけだと、定義が関数と一緒に消えた。
- 修正（`8faf9bb9`）: 剪定した実行ファイルは、剪定しないプログラムが定義する型をすべて定義する（`emit_program`。根が決めるのは出力する関数だけ）。定義は 1 行で、剪定の前に通ったどのプログラムも持っていた集合である。
- 確認: 修正の前に `tests/llvm_handoff.rs`（名指す型がすべて定義されること、定義が 1 つも消えないこと）と `tests/llvm_handoff.mjs`（`channel_std`・`channel_user` の `-O0`・`-O3` のビルドと実行）を足し、前者は修正前に失敗する。
- 費用: 剪定しない場合の到達と型の収集をもう一度行うので、W2（1,000 モジュール）の `emit` 段は 188 → 252 ms（+64 ms。機械占有なし、load average 25〜40、`-O0`、7 標本の最小値。基準は 1,057 ms）。Clang の時間は変わらず、W2 の `-O0` 全体の約 2%。
  WP9 の W2 の表（`emit` 294 ms など）は修正前の値で、静かな機械での再計測で取り直す。

**指摘 2: `wasi.o` の rename。**

- 根本原因: `compile_runtime_object` は `<unit>.o` に書き、WASI のランタイムの staging 名も `wasi.o` なので、`fs::rename` が path を自分自身へ rename していた。Unix では no-op で、Windows は CI でしか分からない。
- 修正（`16a8ab9b`）: `stage_runtime_object` が path が同じなら何もしない。native C・WASI・WASM threads の 3 か所が使う。単体テストは存在しない path を渡す。rename すれば失敗するので、skip したときだけ通る（skip を外すと失敗することを確かめた）。

**指摘 3: 失敗していたビルドが通る。** OS API（`x86_64-pc-windows-gnu` の `File.read_text` は `E2002` だった）、どのライブラリにもない `extern` のシンボル（未定義のシンボルでリンクが失敗していた）、C のランタイムを、未使用の公開関数だけが使うビルドは、
実行ファイルがその関数を出力しないので通る。入口・export・`Drop` から到達すれば今までどおり失敗し、型検査は未使用の公開関数にも行う（`6475ed95`）。
文書: `option.md`（`--emit exe` の中身と、通るようになる 3 種）、`usage.md`、`platforms.md`（`E2002` の表の後）、`native-interop.md`（`extern` の例。`check-docs` が実行し、基準のコンパイラでは未定義のシンボルで失敗する）、`debugging.md`、`docs/language.md`、`docs/architecture.md`。

**指摘 4（広げた差分検査が見つけた。レビューにはなかった）: `tsuzuri run` が、以前通ったプログラムで失敗する。**

- 症状: `tests/fixtures/llvm_handoff/tree_lock`（再帰的な union、表示するリスト、`Mutex` を使う未使用の公開関数）は、基準のコンパイラの `tsuzuri run`（`--trap-info` が既定）が `[1, 2, 3] 1` を出す。
  PB03 の剪定は `use of undefined value '%tz.context'`（`E2002`）で失敗した。`tree`（`tree_lock` から未使用の関数を除いたもの）は、基準のコンパイラも同じ理由で失敗する（既存の不具合）。
- 根本原因: ランタイムの `.ll` のうち `recursive.ll`・`display.ll`・`debug.ll`・`character.ll` は末尾に改行がなく、エミッタは次の部分を直後に付け足すので、次が `define` なら前の関数の閉じ括弧と同じ行（`}define ...`）になる。
  LLVM は読めるが、`llvm_traps::instrument` は行ごとに `define` を探すので、その関数を登録せず、本体を前の関数の一部として `%tz.context` を使わせ、署名には引数を足さなかった。
  `Mutex` と並列タスクの `sync.ll` のような別の部分が間に入るプログラムでは行が分かれていて、剪定でその部分が消えると共有が起きた。
- 修正（`6cc70aa8`）: `instrument` は、閉じ括弧とその残りを別の行として読む（`ir_lines`）。出力が変わるのは `--trap-info` で行を共有する場合だけで、そこは以前、壊れていたか、関数が計装されていなかった。
- 確認: 修正の前に `tests/llvm_handoff.rs`（`%tz.context` を使う関数が引数に取ること。`tree` は剪定の有無にかかわらず、`tree_lock` は剪定したときだけ失敗した）と `tests/llvm_handoff.mjs`（`run` の `-O0`・`-O3`）を足し、修正前の実行ファイルで失敗することを確かめた。

**広げた差分検査**（`benchmarks/check-handoff-equivalence.mjs` の新しいモード、`684028fa`）。入口のない 98 プロジェクト（`Main.tz` のないものを含む）に、次の入口を足して実行ファイルにし、入口のある 34 プロジェクトには export を plain な公開関数にした複製を作る。

- `keep`: 定数の入口（`"ok"`）。export はそのまま（実行ファイルの根）。`none`: 定数の入口と、plain にした export（drop のほかは何も到達しない。最大の剪定）。
- `even`・`odd`: export を plain にして、引数をスカラーと文字列で書ける関数の偶数番目・奇数番目を入口から呼び、結果を表示する。`unexport`: 入口のある 34 件の複製。
- ビルドは `lib0`（`-O0 --no-cache`）、`lib3`（`-O3 --no-cache`）、`libg`（`-O0 -g`）、`libt`（`-O3 --trap-info`。`tsuzuri run` が作るもの）、`libc`（`-O3`、cache あり。`tsuzuri build` の既定）。
- 基準のコンパイラとの比較で要求するのは、プログラムの終了状態・stdout・stderr が同じこと、IR が関数と宣言を失うだけで record・union の型定義が同じこと。
  失敗していたビルドが通ってよいのは、リンクが、剪定されたコードだけが名指すシンボルで失敗していた場合と、基準の trap 計装の失敗（指摘 4）の場合だけで、リンクが失敗のままなら未定義のシンボルが減る場合だけ許す。ほかは差として数える。

結果（通常の release `eb1d78f2…`、基準 `6d15b437` の release `da78ba18…`、`--jobs 5`）: 132 プロジェクトの全 16 モード、**3,332 比較で差 0**。

| 群 | 比較 | 内訳 |
| --- | ---: | --- |
| `--emit header\|llvm\|object\|wasm\|shared`、threads・WASI の wasm | 924 | 745 が byte 一致、179 が同じ失敗 |
| 元のプロジェクトの `exe0`・`exe3`・`exeg`・`exegc` | 528 | 121 が同じ挙動（88 は IR も同じ、33 は剪定で関数が減る）、3 は剪定でリンクが通る、404 は同じ失敗（入口なしの `E2004` など） |
| 新しい `lib0`・`lib3`・`libg`・`libt`・`libc` | 1,880 | 1,672 が両方で成功（1,482 は終了コード 0 で同じ出力、190 は同じ trap で終わる）、うち 1,352 は IR が剪定され（626,595 関数のうち 284,133 が消える）、26 は剪定で通るようになった（リンク 22、trap 計装 4）、22 はリンクが失敗のまま未定義のシンボルが減る、160 は基準も新も作れない |

作れない 160 は、`bindgen` の `expected.tz`（module 名が小文字で `E1011`。25 件）と、host import の `extern` のシンボルを入口・export・`Drop` が使う fixture（`async_host`・`bindings`・`host_imports`・`user_drop` など。リンクの `E2002`。135 件）である。`-O3` では基準の LLVM が未使用の内部関数を捨てるので、未定義のシンボルで失敗するのは `-O0`・`-g` だけで、剪定で通るようになるリンクの 22 はすべてそこに出る。
限界: 入口が呼ぶのは、引数を整数 `3`・浮動小数点 `0.5`・`true`・`"ab"` で書ける関数だけで、前提を assert する関数は trap する（190 比較が同じ trap で終わり、その後の呼び出しは実行されない。コンパイルと IR は検査済み）。Linux と Windows では実行していない。

Rust のテスト `pruning_keeps_the_ir_of_every_project_valid`（`tests/llvm_handoff.rs`、`5a0bdfb1`）は、同じ変換（入口なしは `keep`・`none`、入口ありは原形と `unexport`）の 231 プログラム・693 ビルド（cache あり・なしの `build` と `run`）を、
Clang と基準のコンパイラなしで検査する。剪定した IR の関数が剪定しない IR の部分集合であること、プログラムの型定義が同じこと、定義のない型を名指さないこと、`%tz.context` を使う関数が引数に取ること。CI で動く。
修正の前のコードに戻すと、型の不具合（`channel` の `Cased[i64]` など）と trap 計装の不具合（`tree_lock`）で失敗する。

### 計測

条件・結果・再現は `docs/benchmarks.md` の「LLVM への受け渡し（PB03）」にある。機械は共有で load average 25〜225 だったので、結論は CPU 時間・プロセスごとの時間・標本数だけに基づく。
scratch の道具（リポジトリには入れない）: W2 の生成は「計測手順」のとおり。交互 9 標本は `os.wait4` の `ru_utime + ru_stime` を CPU 時間とし、`TSUZURI_TIME_PASSES=1` の JSON から `emit` と `tool.*` を足す。
Clang の 1 プロセスごとの時間は、引数と経過時間を記録して本物の Clang を呼ぶ Rust の wrapper を `TSUZURI_CLANG` に置いて取った。`sample` は
`CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=line-tables-only` の別 target dir のビルドに `sample <pid> 60 5` を当てた。
静かな機械での正式な before／after（PX03 の `build`・`build-runtime-{cold,warm}`）は未取得で、`_perfs/README.md` の手順で取り直す。

### 決定事項への追記と見つけた問題

- 決定の結果: D1 採用、D2 は C ランタイムの cache 化だけ採用（並行は不採用）、D3・D5・D7 変更なし、D6 は opt-in、D8・D9 不採用、D10 対象外、WP-F 採用。
- ★ 修正した範囲外の問題: 同梱 resource の全件ハッシュ（WP-F）、Homebrew の `wasm-ld` の `--version`（whole-build cache が黙って無効だった）。
- 記録だけ: `run` は `--trap-info` が既定で runtime object を使わず、hello の IR が 758 KB（`build` は 122 KB）になる（計装済みの numeric を別 object にする案は別チケット）。
  whole-build cache の hit の固定費は約 90 ms（コンパイラ本体 9 MB の SHA-256 と `--version`）。`build_key` は `ToolIdentity::hash_into` の戻り値を見ないので、symlink を含む配布物の
  whole-build のキーは resource の識別を欠く（実際の配布物は symlink を持たない）。`tool_line`（`toolchain info`）は Homebrew の `wasm-ld` を `(unavailable)` と表示する（同じ symlink の問題）。
  同梱ツールチェーン（Zig 0.16.0 の `zig cc`）で、`--wasm-host wasi -O0` と `--wasm-feature threads -O0` が `wasm-ld: undefined symbol: __ubsan_handle_*`（`wasi.o`・`threads.o`）で失敗する（基準のコンパイラでも同じ。`-O1` 以上は通る）。`zig cc` が `-O0` で UBSan を既定で有効にし、wasm には その runtime がないため。wasm の C ランタイムの compile に `-fno-sanitize=undefined` を足すのが候補で、本チケットでは直していない（LLVM の Clang を使う CI の `wasm_threads.mjs` には出ない）。
  配布物の `zig cc` のリンクは約 230 ms の固定費で、Apple の ld64 の約 60 ms より重い。ソフトウェアの SHA-256 は `unsafe` を使えず、IR（1,000 モジュールで約 37 MB）とコンパイラのハッシュが毎ビルドの費用になる。
- 後続のチケットへ: PM08 は `build_complete` の `-O{}` 5 か所と `-Wl,-x` を触る（この変更は `final_link_args` と runtime object の 3 か所を足しただけで、衝突は小さい）。
  PM05・PR06 が runtime object やタスクの runtime を変えるときは、`tests/handoff_tools.mjs` の単位名（`wasm-threads`・`wasi`・`stack-debug`）と D1 の差分検査を再実行する。
  ランタイムの部分を足す・並べ替えるチケット（PM01・PR03・PR02・PM05）は、剪定でどの部分が入るかを変えるので、`pruning_keeps_the_ir_of_every_project_valid` と広げた差分検査（`lib0`〜`libc`）を再実行する。
- ★ レビュー後に直した、基準にもあった不具合: `tsuzuri run`（trap 計装）が、再帰的な union と表示するリストを持つ単純なプログラムで `use of undefined value '%tz.context'` になる（指摘 4。`tree` fixture）。
  `}define` の行をまたぐ runtime の部分を `llvm_traps::instrument` が読めなかったのが原因で、剪定が行の共有を新たに起こしうるので、同じ変更で直した。
- 記録だけ（レビュー後）: 入口が呼ぶ関数の引数が固定なので、広げた差分検査の 190 比較は同じ trap で終わる。関数ごとに別の実行にする（引数で呼ぶ関数を選ぶ入口）と、trap の後の呼び出しも比べられる。

### 更新した文書

`_tsuzuri/language-reference/compiler/usage.md`（ランタイム object: C ランタイムの cache と、同梱ツールチェーンの識別の信頼の範囲。実行ファイルに入る関数）、`option.md`（`TSUZURI_LINKER`、組み合わせのエラー。`--emit exe` に入る関数と、通るようになるビルド）、
`platforms.md`（`E2002` の表の後に、実行ファイルでは到達する関数が使うときだけ）、`native-interop.md`（`extern` のシンボルが未定義でもリンクできる例）、`debugging.md`（実行ファイルに入る関数）、`docs/architecture.md`（剪定、型定義、trap 計装の行）、
`docs/language.md`（実行ファイルに入る関数と、通るようになるビルド）、`docs/benchmarks.md`、`README.md`（キャッシュ、`TSUZURI_LINKER`、テストの一覧）、`src/main.rs` の HELP。

### 変更したファイル

`git diff --stat c6864cf5..HEAD` を参照（`src/` は `bundle_identity.rs`（新規）・`cache.rs`・`driver.rs`・`llvm.rs`・`llvm_traps.rs`・`lib.rs`・`main.rs`、テストは `tests/llvm_handoff.{rs,mjs}`・`handoff_tools.mjs`・`bundle_identity.mjs`・`fixtures/llvm_handoff/`、
差分検査は `benchmarks/check-handoff-equivalence.mjs`、CI は `.github/workflows/targets.yml`）。
