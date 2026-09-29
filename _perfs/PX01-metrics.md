# PX01: 性能指標の拡張と記録の継続

| 項目 | 内容 |
| --- | --- |
| ID | PX01 |
| 分類 | 計測基盤 |
| 優先度 | P0 |
| 規模 | M |
| 依存 | – |
| 関連 | PX02, PX03, G17 Phase 0。PX01 形式と確保の計数を使う: PM01〜PM09, PR01〜PR07, PB01〜PB07 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D3（生データのリポジトリ外での長期保管）, D8（x86_64 Linux・AArch64 Linux の計測機の確保）。どちらも Phase 1 の手順 1〜12 には不要 |
| 手本にする既存実装 | 確保の計数: `benchmarks/run-computations.mjs` の tracked 経路（clang `-O3` 後の IR の `@malloc`→`@tracked_alloc` 置換、属性の除去、`-O0` での再コンパイル、`-DTRACKING` の host）と `benchmarks/computations/native.cpp` の `#ifdef TRACKING` 節。`realloc` の扱いは `tests/features.mjs` の `tracked_realloc`。要約: `benchmarks/report.mjs` の `renderTable`・`publishSummary`（一時ファイル＋`renameSync`）と CLI の判定（`process.argv[1]` と `import.meta.url` の比較）。テスト: `tests/benchmark_report.mjs` |
| 主な影響ファイル | 変更: `benchmarks/metrics.mjs`（新規）, `benchmarks/tracked_alloc.c`（新規）, `benchmarks/run-control.mjs`, `benchmarks/control/host.c`, `benchmarks/run-cpp.mjs`, `benchmarks/cpp/native.cpp`, `benchmarks/run-computations.mjs`, `benchmarks/report.mjs`, `src/timings.rs`（新規）, `src/lib.rs`, `src/main.rs`, `src/check.rs`, `src/driver.rs`, `tests/time_passes.rs`（新規）, `tests/benchmark_report.mjs`, `docs/benchmarks.md`, `docs/architecture.md`, `README.md`, `_docs/tools/command-line.md`, `_perfs/README.md`。確認のみ: `benchmarks/run-managed.mjs`（runner の stdout を読む）, `benchmarks/computations/native.cpp`, `.gitignore`（`target`） |
| 計測対象 | `examples/hello` のコンパイル（`check`、`build -O0`／`-O3`、`--emit object`、`--target wasm32`）と実行ファイルの実行。`benchmarks/control`（15 種目）・`benchmarks/cpp`・`benchmarks/computations`（9 種目）の全種目の時間と Tsuzuri 実装の確保 |

## 目的

「超高速・超省メモリ・超高速ビルド」を同じ物差しで判断できるよう、処理時間に加えて CPU 時間・最大 RSS・確保回数と確保量・成果物サイズ・コンパイルの段階別時間を、一つの機械可読な形式（**PX01 形式**: JSON Lines、`schema: 1`）で計測・記録する。
他の性能チケット（PX02・PX03・PR・PM・PB）は before／after をこの形式で `target/perf/<run_id>/` に記録し、`node benchmarks/metrics.mjs report --baseline` の比較で効果と退行を判断する。退行の検出は報告であり、合否の判定ではない。

## 着手条件と停止条件

### 着手条件

- 依存はない。`_perfs/README.md` の一覧で PX01 の状態を `doing` にしてから始める。
- GUIDE §2.3 の手順で `cargo build --release --locked` と `cargo test --locked` が通り、`node tests/benchmark_report.mjs` が終了コード 0 で終わる状態から始める。
- runner は Node 24 以上で動かす。PATH の Node が古い場合は `npx --yes --package=node@24 node ...` を使う（2026-09-29 の計測機の PATH は v20.19.6。Node 20 は重い BigInt の処理で V8 が異常終了することがある）。
- 行うのは Phase 1（手順 1〜12）だけ。Phase 2（設計の「Phase 2」）と手順 13（Linux の計測、D8）は、人間が依頼・承認した場合だけ行う。

### 他チケットへ提供するインターフェース

PX01 はどのチケットにも依存しない。後続のチケットは次を前提にしてよい（名前は固定。変える場合は schema を上げる）。

- **PX01 形式**: 設計「結果レコード」の schema 1 のレコードを、`target/perf/<run_id>/<suite>.jsonl` に 1 行 1 レコードで書いたもの。
- `benchmarks/metrics.mjs`（新規）の export: `SCHEMA`・`METRICS`・`LANGUAGES`・`median`・`summarize`・`captureRun`・`makeRecord`・`validateRecord`・`parseTimeOutput`・`parseTimePasses`・`measureProcess`・`writeRecords`・`readRecords`・`compareRuns`・`renderComparison`・`renderMetricsSummary`・`updateMetricsSummary`。
- runner の `--metrics <dir>`（新規、`run-control.mjs`・`run-cpp.mjs`・`run-computations.mjs`）と、`node benchmarks/metrics.mjs process ...`／`report ...`（新規）。
- 確保の計数: metric `alloc_calls`（回）と `alloc_bytes`（bytes）。PM02 の「欄の名前は PX01 の実装に従う」はこの 2 つを指す。値は stdout ではなく `--metrics` の JSONL に出る。
- コンパイラの段階別時間: 環境変数 `TSUZURI_TIME_PASSES=1`（新規）で、終了時に stderr へ 1 行の JSON を出す。G17 Phase 0 と PX03 はこれを使い、別の仕組みを作らない。

### 停止条件

次の場合は手を止め、コマンド・出力・該当箇所を添えて報告する。症状への個別の手当てで先へ進めない。

1. `TSUZURI_TIME_PASSES=1` の有無で、IR・header・object・実行ファイル・WASM のどれかがバイト単位で異なる。
2. 段階別時間の合計が `total_ms` を超える（段の入れ子による二重計上）。入れ子を許す形に変えて通さない。
3. tracked IR に `@malloc`・`@calloc`・`@realloc`・`@free` 以外の確保・解放関数（`@aligned_alloc`・`@posix_memalign`・`@strdup`・`@strndup` など）の呼び出しがある。数えられない確保を黙って除外しない。
4. tracked 実行で `live != 0`、magic の不一致による `abort`、またはリンクで `tsuzuri_task_*` 以外の未定義の runtime シンボル（`tsuzuri_cpu_*`、`tsuzuri_io_*` など）が出た。
5. runner の stdout の JSON（手順 1 で保存したもの）の欄の名前・意味を変える必要が生じた。`benchmarks/run-managed.mjs` と `benchmarks/report.mjs` が読むため、PX01 は stdout を変えない。
6. Rust の crate・npm パッケージの追加、または `unsafe` が必要になった（`Cargo.toml` の `[lints.rust]` は `unsafe_code = "forbid"`）。
7. Linux で `/usr/bin/time` がない、または GNU time 1.8 未満。独自の計測器（起票時の `benchmarks/measure.c` 案）で代替しない（D4）。
8. 既存テストの期待値を変える必要が生じた。
9. 計測機の追加や生データの外部保管（D3・D8）が必要になった。承認なしに用意しない。

## 現状と計測（HEAD `f8dc655`）

### runner の出力（コードで確認）

| runner | 旗 | 標本と warm-up | 時計 | 確保の計数 |
| --- | --- | --- | --- | --- |
| `benchmarks/run-control.mjs` | `[compiler] --quick --scale --cpu --baseline --artifacts` | 1 種目 12 標本（`benchmarks/control/host.c` の `VARIANTS * 3`。`VARIANTS` は c・cpp・rust・tsuzuri の 4、`--baseline` で before を加えて 5）。variant ごとに 2 回の warm-up、反復回数の較正、順序の回転と反転 | `clock()`（CPU）と `CLOCK_MONOTONIC`（wall）。値は 1 呼び出しあたりの ms | なし。`inspection.array_allocator_calls` は最適化後 IR の `call .*@malloc\(` の静的な個数 |
| `benchmarks/run-cpp.mjs` | `[compiler] --quick --scale --cpu --artifacts` | 12 標本（`assert.equal(result.samples, 12)`）、warm-up、6 通りの測定順 | CPU と wall（`cpp_ms`・`rust_ms`・`tsuzuri_ms` と `*_wall_ms`） | なし |
| `benchmarks/run-computations.mjs` | `[compiler] --quick --scale --cpu --baseline --artifacts` | 12 標本（`(quick ? 1 : 3) * 4`、baseline で `* 5`）。WASM は 3 回の warm-up と較正 | `std::clock`（`<label>_ms`）と `std::chrono::steady_clock`（`<label>_wall_ms`）。label は cpp・rust・direct・computation・before。WASM は `performance.now()`（`work.wasm.raw_ms` の direct・current・before） | あり（下記） |

- 3 つとも stdout に 1 つの JSON を出す。run-control の形（値は省略）:

```text
{ environment: { tsuzuri, compiler_sha256, baseline, baseline_sha256, clang, rustc, node, platform,
                 os_release, architecture, cpu, logical_cpus },
  mode: "benchmark" | "correctness-smoke", cpu, scale, wall_clock, c_cpp_flags, rust_flags, tsuzuri_flags,
  inspection: { source_switch_count, optimized_vector_instructions, array_allocator_calls },
  samples, clock_ticks_per_second,
  workloads: [ { name, size, checks: [ { size, seed, checksum } ],
                 raw: [ { seed, checksum, c_ms, c_wall_ms, cpp_ms, ..., tsuzuri_ms, tsuzuri_wall_ms } ],
                 c_median_ms, c_wall_median_ms, ..., tsuzuri_over_c, tsuzuri_over_cpp, tsuzuri_over_rust } ],
  note }
```

- run-control の種目は 15 個（`while_mix`・`for_mix`・`tail_mix`・`tail_if_mix`・`tail_builtin_mix`・`match_dispatch`・`array_sum`・`array_copy`・`list_sum`・`closure_capture`・`closure_churn`・`record_pipeline`・`integer128_mix`・`float32_mix`・`float64_mix`）。既定の大きさは host.c の表（例: `while_mix` 8,000,000、`list_sum` 100,000）に `--scale` を掛けた値。
- run-cpp は `benchmarks/Mix.tz` と `benchmarks/cpp/Kernels.tz` を `--emit object` で別々の object にし、`benchmarks/cpp/native.cpp` と Rust の staticlib（`benchmarks/cpp/native.rs`）にリンクする。種目の名前は run-cpp の `references` の key と一致する（`assert.deepEqual`）。
- run-computations の種目は 9 個（`bind`・`checked`・`delayed`・`array_for`・`array_bind`・`owned_capture`・`std_option`・`std_result`・`std_option_owned`）。
- `benchmarks/run-managed.mjs` は 3 つの stdout を読み、C#・JavaScript と合わせて `publishSummary`（`benchmarks/report.mjs`）でサマリーを書き換える。
- 次の runner は Phase 1 で変えない: `run.mjs`（`wasm.bytes`・`wasm.imports` を出す）、`run-tasks.mjs`、`run-simd.mjs`（平文の出力）、`run-dispatch.mjs`（1 行 1 JSON）、`run-gpu.mjs`、`run-cache.mjs`（200 モジュールの uncached／warm の中央値）。
- 生データは `target/benchmarks/` に手作業の名前で保存されている（`docs/benchmarks.md` の各節）。形式はそろっておらず、比較は手作業。

### 要約の生成

- `benchmarks/report.mjs` は `renderTable(headers, rows, { caption, timingColumns, unrankedRows })`・`renderSummary(results)`・`updateSummary(document, results)`・`publishSummary(results, path)` を export する。`renderTable` は汎用の HTML 表で、`timingColumns` を渡した列だけに最小値の強調と「Tsuzuri 相対性能」列を付ける（渡さなければ普通の表）。
- `renderSummary` は run-managed の結果専用（`mode === "benchmark"`、tsuzuri・cpp・rust・csharp・javascript の variant 必須、`median_wall_ms`）。`docs/benchmarks.md` の `<!-- benchmark-summary:start -->`／`<!-- benchmark-summary:end -->` の間を書き換える。
- `publishSummary` は `${path}.${process.pid}.tmp` を `flag: "wx"` で書いて `renameSync` し、失敗時は一時ファイルを消す。
- `tests/benchmark_report.mjs` がこれらを検査する（位置、同率、単位、エスケープ、冪等性、quick の拒否、NaN の拒否、`std_option_owned` の順位除外）。

### 確保の計数

- run-computations: clang `-O3 -S -emit-llvm` の出力（`<label>.opt.ll`）の `@malloc`／`@free` を `@tracked_alloc`／`@tracked_free` に置き換え、`attributes #N = ...` の行と ` #N` を消して `-O0` で object にし、`-DTRACKING` でビルドした `benchmarks/computations/native.cpp` とリンクする。host は各種目の variant（direct・computation・before。cpp と rust は除く）を `kernels[variant](64, 42)` で 1 回呼び、`{calls, bytes}` を出し、`live != 0` なら終了コード 1。runner は `workload.allocations_at_size_64` に入れる。
- E2E（`tests/features.mjs`・`tests/io.mjs`・`tests/host_imports.mjs`）は `@realloc` も `@tracked_realloc` に置き換える。追跡関数は 16 bytes のヘッダー（大きさと magic `0x51a110ca7e`）を前に付ける。
- control と cpp には確保の計数がない。

### 最大 RSS・成果物サイズ・ビルド時間

- 最大 RSS を測る runner はない。成果物サイズは `run.mjs` と run-computations の WASM のバイト数だけ。ビルド時間は run-cache の end-to-end の wall time だけ。
- コンパイラに段階別の時間を出す仕組みはない。`std::time::Instant` を使うのは `src/lsp.rs` と `src/test_runner.rs` だけ。Rust から `getrusage` を呼ぶには `unsafe` か crate の追加が要る（`unsafe_code = "forbid"`）。
- コンパイラの処理の段（関数名は確認済み）:
  - 読み込み: `src/main.rs` の `main` が `Project::load_for_tests`／`Project::load_for_docs`／`Project::load`（`src/driver.rs`）を呼ぶ。
  - 構文解析: `Project::analyze_all` → `src/lib.rs` の `analyze_inputs_indexed_all` のループが入力ごとに `parser::parse_with_source_all` を呼ぶ。
  - 型検査以降: `src/check.rs` の `check_modules_indexed_all` が型検査、`crate::ownership::infer_copy_all`、`polymorph::specialize`、`closures::lower`、`crate::ownership::check_all`、`crate::gpu::validate_calls` を順に行う。
  - 生成: `src/main.rs` の `run_action` → `driver::build`／`driver::run_with_diagnostics` → `build_complete`。`let mut text = if options.emit == Emit::Header {` の文が `llvm::header`・`crate::gpu::extract_kernel`・`llvm::emit_*` で文字列を作る。その後 `crate::cache::build_key`・`BuildCache::open`・`load` で cache を引き、外部ツール（`Command::new(tool("TSUZURI_CLANG", "clang"))` など）を `run_tool` で実行する。
  - `src/main.rs`・`src/driver.rs`・`src/lib.rs` に `process::exit` はない（`main` は常に `ExitCode` を返す）。`check` は `--no-cache` を受け付けない（`--no-cache is only valid with build or run`）。

### 2026-09-29 の probe（M1 Max、macOS 27.0）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
/usr/bin/time -l target/release/tsuzuri build examples/hello -O0 --no-cache -o /tmp/tz-work-PX01/hello
/usr/bin/time -l /tmp/tz-work-PX01/hello
/usr/bin/time -l -o /tmp/tz-work-PX01/time-o.txt /bin/sh -c 'echo child-err >&2; exit 3'
```

コンパイラの `/usr/bin/time -l` の出力（抜粋。stderr に出る）:

```text
        0.16 real         0.11 user         0.05 sys
            43253760  maximum resident set size
                 530  page faults
           263042652  instructions retired
            12878280  peak memory footprint
```

- `build -O0` は real 0.16 s、最大 RSS 43,253,760 bytes。値は wait4 の rusage なので、待ち合わせた子プロセス（clang）を含む。`peak memory footprint` は macOS 独自の別の量で、使わない。
- hello の実行ファイルは 100,824 bytes（`_perfs/README.md` の値と一致）。実行すると `5050` を出し、最大 RSS は 1,687,552 bytes。新しくリンクした実行ファイルの初回の実行は real 0.22 s に対して user・sys とも 0.00 s だった（初回起動の検査の分。warm-up で捨てる）。
- `real`・`user`・`sys` は 10 ms 単位（小数 2 桁）で、短い処理の wall time には粗い。
- `-o <file>` を付けると結果はファイルに出て、子の stderr（`child-err`）は分かれたまま、子の終了コード（3）がそのまま返る。
- 道具の版: Apple clang version 21.0.0 (clang-2100.3.34.2)、rustc 1.98.1 (48a229cea 2026-09-01) (Homebrew)、PATH の Node v20.19.6、`sysctl` の CPU は Apple M1 Max・10 コア。

## 目標と指標

目標は計測の範囲と正しさであり、PX01 自身は速度の目標を持たない。CI の合否条件にしない。

- G1: Phase 1 の runner（control・cpp・computations）と process suite が、PX01 形式のレコードを書く。
- G2: 計時する実行ファイルとコンパイラの既定の出力は変わらない。確保の計数は別の実行ファイルで行う。
- G3: 他チケットが before／after を同じ手順で記録し、`report --baseline` で「改善・悪化・差なし」を読める。

| metric | unit | 定義 | 標本と統計 | 出す suite |
| --- | --- | --- | --- | --- |
| `wall_time` | ms | 経過時間。in-process suite は 1 回の kernel 呼び出し（runner の反復回数で割った値）。process suite は 1 プロセス（`process.hrtime.bigint()` で `/usr/bin/time` の起動から終了まで） | warm-up を除いて 9 標本以上。中央値・最小・最大 | control, cpp, computations, process |
| `cpu_time` | ms | CPU 時間。in-process suite は host の `clock()`／`std::clock`（全スレッドの合計）。process suite は user + sys | 同上 | control, cpp, computations, process |
| `user_time` | ms | `/usr/bin/time` の user（分解能 10 ms） | 同上 | process |
| `sys_time` | ms | `/usr/bin/time` の sys（分解能 10 ms） | 同上 | process |
| `peak_rss` | bytes | `/usr/bin/time` の最大 RSS。macOS は `maximum resident set size`（bytes）、Linux は `Maximum resident set size (kbytes)` × 1024。子プロセスのうち最大の 1 つを含み、合計ではない | 同上 | process |
| `alloc_calls` | count | Tsuzuri の IR の `malloc`・`calloc` の回数と、null でも大きさ 0 でもない `realloc` の回数 | 1 標本（決定的） | control, cpp, computations |
| `alloc_bytes` | bytes | その要求バイト数の合計（`calloc` は個数 × 大きさ、`realloc` は新しい大きさ）。累計であり、同時に生きている量の最大ではない | 1 標本 | control, cpp, computations |
| `executable_bytes` | bytes | 実行ファイルの大きさ（`statSync().size`） | 1 標本（9 回のビルドで同じことを確認） | process |
| `object_bytes` | bytes | `--emit object` の出力の大きさ | 同上 | process |
| `wasm_bytes` | bytes | `--target wasm32` の `.wasm` の大きさ | 同上 | process, computations |
| `phase_time` | ms | `TSUZURI_TIME_PASSES=1` の段階別時間。段の名前は `phase` 欄 | 9 標本以上 | process |

- 中央値は既存 runner と同じ計算（昇順に並べ、標本数が偶数なら中央の 2 つの平均）。平均と標準偏差は記録しない。
- warm-up: in-process suite は各 runner の既存の warm-up と較正をそのまま使う（標本数は 12 で 9 以上を満たす）。process suite は測る組（project・variant・opt・target）ごとに 1 回を捨ててから 9 回測る。
- `--quick` は `mode: "quick"` で記録する。process suite の quick は warm-up 0 回・1 標本。quick の値は性能の証拠にせず、要約への公開を拒否する。

## 変えてはいけない意味

- コンパイラの既定の出力（IR・header・object・実行ファイル・WASM・stdout・stderr・終了コード・診断のコードとメッセージ）。`TSUZURI_TIME_PASSES=1` のときも成果物はバイト単位で同じで、stderr の最後に 1 行が増えるだけ。
- IR の決定性。段の計測は IR の生成順・名前に触れない。
- runner の stdout の JSON（欄の名前・値・順序の意味）と、計時に使う実行ファイルのビルド手順。host の変更は `#ifdef TRACKING` の中だけにする。
- benchmark の数値の意味（checksum の照合、整数の折り返し、浮動小数点の丸め）。既存の参照値との照合をそのまま通す。
- WASM の import がないこと（run-computations の `assert.deepEqual(WebAssembly.Module.imports(module), [])`）。
- 共有 CI に速度・メモリの閾値を入れない（AGENTS.md）。比較の結果で終了コードを変えない。

## 設計

### 結果レコード（PX01 形式、schema 1）

1 行 1 レコードの JSON Lines。各行は自己完結し（host を行ごとに複製する）、複数の suite・run を連結しても解釈できる。欄は次の 22 個で固定し、`makeRecord` がこの順序で出す。欄の追加・改名・意味の変更は schema 2 とし、`validateRecord` は schema 1 だけを受け付ける。

| 欄 | 型 | 内容 |
| --- | --- | --- |
| `schema` | number | 常に `1` |
| `run_id` | string | 出力ディレクトリの basename。`^[A-Za-z0-9._-]{1,80}$` |
| `commit` | string | `git rev-parse HEAD`（40 桁の 16 進、小文字） |
| `dirty` | boolean | `git status --porcelain` の出力が空でなければ `true` |
| `date` | string | run の開始時刻。`new Date().toISOString()`（UTC、`Z` 終わり）。1 回の `captureRun` で 1 つ |
| `mode` | string | `benchmark` または `quick` |
| `host` | object | 下の表の 9 欄 |
| `suite` | string | `control`・`cpp`・`computations`・`process`。PX02・PX03 が追加する。`^[a-z0-9-]+$` |
| `workload` | string | 種目名（runner の `name`）。process suite はリポジトリ根からの project の相対パス（`examples/hello`）。module 全体の値は `module` |
| `size` | number or null | その測定の入力の大きさ（`--scale` 適用後の整数）。ない場合は `null` |
| `language` | string | `LANGUAGES` のどれか |
| `variant` | string | 実装の区別。既定は `default`。`before`（`--baseline` のコンパイラ）、`direct`・`computation`（computations）、process suite では操作名。`^[a-z0-9-]+$` |
| `target` | string | `native` または `wasm32` |
| `opt` | string | `O0`・`O3`・`none`（`check` など最適化のない操作） |
| `cpu_mode` | string | `generic` または `native`（runner の `--cpu`。process suite は `generic`） |
| `metric` | string | 「目標と指標」の表の名前 |
| `phase` | string or null | `metric` が `phase_time` のときだけ段の名前（`^(load\|parse\|check\|specialize\|closures\|ownership\|emit\|cache\|total\|tool\.[A-Za-z0-9._+-]+)$`）。それ以外は `null` |
| `unit` | string | `METRICS[metric]`（`ms`・`bytes`・`count`） |
| `samples` | number[] | 生の標本（warm-up を除く、測定順） |
| `median`, `min`, `max` | number | `samples` から計算した値 |

| `host` の欄 | 値 |
| --- | --- |
| `os` | `os.platform()`（`darwin`・`linux`） |
| `os_version` | macOS は `sw_vers -productVersion` の出力、それ以外は `os.release()` |
| `arch` | `os.arch()` |
| `cpu` | `os.cpus()[0].model` |
| `cores` | `os.cpus().length`（論理 CPU 数。既存 runner の `logical_cpus` と同じ） |
| `clang` | `$TSUZURI_CLANG`（既定 `clang`）の `--version` の 1 行目。実行できなければ `null` |
| `rustc` | `$TSUZURI_RUSTC`、`$RUSTC`、`rustc` の順で最初に決まるものの `--version` の 1 行目。実行できなければ `null` |
| `node` | `process.version` |
| `tsuzuri_sha256` | 計測したコンパイラの実行ファイルの SHA-256（既存 runner の `compiler_sha256` と同じ計算） |

- 識別キーは `suite`・`workload`・`size`・`language`・`variant`・`target`・`opt`・`cpu_mode`・`metric`・`phase` の組。1 つの run の中で重複してはならない（`readRecords` が拒否する）。
- 例（2026-09-29 の probe の値で作った quick のレコード。ファイルでは 1 行）:

```json
{
  "schema": 1,
  "run_id": "20260929T101500Z-f8dc655",
  "commit": "<git rev-parse HEAD の 40 桁>",
  "dirty": false,
  "date": "2026-09-29T10:15:00.000Z",
  "mode": "quick",
  "host": {
    "os": "darwin",
    "os_version": "27.0",
    "arch": "arm64",
    "cpu": "Apple M1 Max",
    "cores": 10,
    "clang": "Apple clang version 21.0.0 (clang-2100.3.34.2)",
    "rustc": "rustc 1.98.1 (48a229cea 2026-09-01) (Homebrew)",
    "node": "<process.version>",
    "tsuzuri_sha256": "<コンパイラの SHA-256、64 桁>"
  },
  "suite": "process",
  "workload": "examples/hello",
  "size": null,
  "language": "tsuzuri",
  "variant": "execute",
  "target": "native",
  "opt": "O0",
  "cpu_mode": "generic",
  "metric": "peak_rss",
  "phase": null,
  "unit": "bytes",
  "samples": [1687552],
  "median": 1687552,
  "min": 1687552,
  "max": 1687552
}
```

### 保存先と run_id

- 生データは `target/perf/<run_id>/<suite>.jsonl`。`target` は `.gitignore` 済みで、Git に追加しない。runner の stdout は同じディレクトリに `<suite>.json` として保存する（計測手順）。
- `run_id` は `--out`／`--metrics` に渡したディレクトリの basename。推奨する名前は、通常の計測が `<UTC の YYYYMMDDTHHMMSSZ>-<git rev-parse --short HEAD>`、チケットの比較が `<チケット ID>-before`・`<チケット ID>-after`（GUIDE §14 の「`target/perf/<ID>/`」はこの形で使う）。
- `writeRecords` はファイルを `flag: "wx"` で作る。同じ run のディレクトリに同じ suite を二度書けない（計測ごとに新しい run_id を使う）。異なる suite は同じディレクトリに並べてよい。

### `benchmarks/metrics.mjs`（新規）

新 API（実装後に有効。未検証）。Node の標準モジュール（`node:assert/strict`・`node:child_process`・`node:crypto`・`node:fs`・`node:os`・`node:path`・`node:url`）と `./report.mjs` だけを使い、npm の依存を足さない。

```javascript
export const SCHEMA = 1;
export const METRICS = Object.freeze({
  wall_time: "ms", cpu_time: "ms", user_time: "ms", sys_time: "ms", phase_time: "ms",
  peak_rss: "bytes", alloc_calls: "count", alloc_bytes: "bytes",
  executable_bytes: "bytes", object_bytes: "bytes", wasm_bytes: "bytes",
});
export const LANGUAGES = Object.freeze(["tsuzuri", "c", "cpp", "rust", "zig", "go", "csharp", "javascript"]);
export function median(values) {}                       // 既存 runner と同じ計算
export function summarize(samples) {}                   // { samples, median, min, max }。空・非有限・負は throw
export function captureRun(directory, { compiler, mode, clang, rustc }) {}
                                                        // { schema, run_id, commit, dirty, date, mode, host }
export function makeRecord(context, fields) {}          // 22 欄を固定順で持つ検証済みレコード
export function validateRecord(record) {}               // 規則違反で AssertionError
export function parseTimeOutput(text, system) {}        // { wall_ms, user_ms, sys_ms, peak_rss_bytes }
export function parseTimePasses(stderr) {}              // { phases: [{ name, ms }], total_ms }
export function measureProcess(command, args, options) {}
                                                        // { wall_ms[], user_ms[], sys_ms[], peak_rss_bytes[], stdout[], stderr[] }
export function instrumentAllocations(ir) {}            // tracked IR の文字列
export function writeRecords(directory, suite, records) {} // 書いたファイルのパス
export function readRecords(directory) {}               // 全 *.jsonl のレコード（ファイル名順、行順）
export function compareRuns(baseline, current) {}       // 比較の行の配列
export function renderComparison(rows) {}               // Markdown の表（端末向け）
export function renderMetricsSummary(records) {}        // 1 run の HTML
export function updateMetricsSummary(document, runs) {} // マーカーの間を置き換えた文書
```

- `makeRecord(context, fields)`: `fields` は `suite`・`workload`・`size`（既定 `null`）・`language`・`variant`（既定 `"default"`）・`target`（既定 `"native"`）・`opt`・`cpu_mode`（既定 `"generic"`）・`metric`・`phase`（既定 `null`）・`samples`。`unit` は `METRICS` から、`median`・`min`・`max` は `summarize` から埋める。最後に `validateRecord` を呼ぶ。
- `validateRecord` の規則:
  1. 欄の集合が 22 個と一致する（過不足で throw）。`host` の欄も 9 個と一致する。
  2. `schema === 1`。`run_id`・`commit`（`^[0-9a-f]{40}$`）・`date`（`new Date(date).toISOString() === date`）・`mode` が上の表のとおり。`dirty` は boolean。
  3. `host.cores` は正の整数、`host.tsuzuri_sha256` は `^[0-9a-f]{64}$`、`host.clang`・`host.rustc` は string か `null`。
  4. `language`・`target`・`opt`・`cpu_mode`・`metric` が許された値。`unit === METRICS[metric]`。
  5. `phase` は `metric === "phase_time"` のときだけ段の名前の正規表現に一致し、それ以外は `null`。
  6. `samples` は空でない有限の非負数の配列。`unit` が `bytes`・`count` なら整数。
  7. 標本の数: `mode === "benchmark"` で、`wall_time`・`cpu_time`・`user_time`・`sys_time`・`phase_time`・`peak_rss` は 9 個以上。`alloc_*`・`*_bytes` は全標本が等しい（決定的）。
  8. `median`・`min`・`max` が `samples` からの計算値と `===` で等しい。
- 他チケットによる拡張: `suite` の名前は制限しない（PX02 の `apps`、PX03 の `build`、PR04 の `numeric`、PR05 の `utf`、
  PR06 の `tasks-data`・`tasks-sweep`、PM08 の `size` など）。metric と opt の許可値は、それを使うチケットが `METRICS` と許可値の
  表へ行を足して増やす（PM05 の `wasm_memory_bytes`、PM08 の `text_bytes`・`wasm_code_bytes` と opt の `Os`・`Oz`）。
  欄の追加ではないので schema 1 のままとし、足した値は `tests/benchmark_report.mjs` の `validateRecord` のテストに 1 件ずつ加える。
- `writeRecords(directory, suite, records)`: 全レコードの `suite` が引数と、`run_id` が `basename(resolve(directory))` と等しいことを確かめ、`mkdirSync(directory, { recursive: true })` の後、`<suite>.jsonl` に `records.map(JSON.stringify).join("\n") + "\n"` を `flag: "wx"` で書く。
- `readRecords(directory)`: `.jsonl` のファイルを名前順に読み、空行を除いて `JSON.parse` と `validateRecord` を行い、識別キーの重複・`run_id` の混在を拒否する。
- CLI（`process.argv[1]` が本ファイルのとき。`report.mjs` と同じ判定）:

```text
node benchmarks/metrics.mjs process [compiler] --out <dir> [--project <dir>]... [--quick]
node benchmarks/metrics.mjs report <dir>... [--baseline <dir>] [--publish]
```

  - `process`: compiler の既定は `target/release/tsuzuri`。`--project` は複数回指定でき、既定は `examples/hello` だけ。旗の誤りは runner と同じく `throw new Error(usage)`。
  - `report` の旗なし: 各レコードを 1 行ずつ `suite/workload variant language target opt metric[phase] median unit (min 〜 max, n)` の形で表示する。
  - `report <dir> --baseline <dir>`: `<dir>` は 1 つだけ。`renderComparison(compareRuns(...))` を stdout に出す。終了コードは入力が正しければ常に 0。
  - `report <dir>... --publish`: `updateMetricsSummary` で `docs/benchmarks.md` を書き換え、`Metrics summary updated` を出す。書き込みは `report.mjs` から切り出した `writeAtomically(path, text)`（新規 export）を使う。

### プロセス単位の計測（`measureProcess`・`parseTimeOutput`）

```javascript
// 新 API（実装後に有効。未検証）
export function measureProcess(command, args, { cwd = root, env = process.env, runs = 9, warmups = 1,
    timeoutMs = 600_000, system = platform() } = {}) {
  const flag = { darwin: "-l", linux: "-v" }[system];
  if (!flag) throw new Error(`peak RSS measurement supports macOS and Linux, not ${system}`);
  if (system === "linux") requireGnuTime(); // `/usr/bin/time --version` が "GNU time 1.8" 以上。1 回だけ確認
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-metrics-"));
  const result = { wall_ms: [], user_ms: [], sys_ms: [], peak_rss_bytes: [], stdout: [], stderr: [] };
  try {
    for (let index = 0; index < warmups + runs; ++index) {
      const report = join(directory, `time-${index}.txt`);
      const start = process.hrtime.bigint();
      const child = spawnSync("/usr/bin/time", [flag, "-o", report, command, ...args],
        { cwd, env, encoding: "utf8", timeout: timeoutMs, maxBuffer: 256 * 1024 * 1024 });
      const wall = Number(process.hrtime.bigint() - start) / 1e6;
      if (child.error) throw new Error(`cannot run /usr/bin/time: ${child.error.message}; on Linux install GNU time (apt-get install time)`);
      if (child.status !== 0) throw new Error(`${command} exited with ${child.status}:\n${child.stderr}`);
      if (index < warmups) continue;
      const parsed = parseTimeOutput(readFileSync(report, "utf8"), system);
      result.wall_ms.push(wall);
      result.user_ms.push(parsed.user_ms);
      result.sys_ms.push(parsed.sys_ms);
      result.peak_rss_bytes.push(parsed.peak_rss_bytes);
      result.stdout.push(child.stdout);
      result.stderr.push(child.stderr);
    }
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
  return result;
}
```

- `parseTimeOutput(text, "darwin")`: `^\s*([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys\s*$` と `^\s*(\d+)\s+maximum resident set size\s*$`（どちらも `m` フラグ）。秒は `Math.round(Number(value) * 1000)` で整数の ms にする。RSS は bytes のまま。
- `parseTimeOutput(text, "linux")`: `User time (seconds): ([\d.]+)`、`System time (seconds): ([\d.]+)`、`Elapsed (wall clock) time (h:mm:ss or m:ss): ([\d:.]+)`、`Maximum resident set size (kbytes): (\d+)`。経過時間は `:` で分け、最後を秒、その前を分、さらに前を時として ms にする。RSS は × 1024。
- どちらも見つからない行があれば throw（`unrecognized /usr/bin/time output`）。`darwin`・`linux` 以外は throw。
- `wall_ms` は `/usr/bin/time` の起動を含む（数 ms の一定の上乗せ）。比較は同じ方法の値どうしで行う。`/usr/bin/time` の `real` は 10 ms 単位のため wall には使わず、テストでだけ確かめる。
- `parseTimePasses(stderr)`: `{"time_passes":` で始まる最後の行を `JSON.parse` し、`time_passes === 1` を確かめて `{ phases, total_ms }` を返す。行がなければ throw。

### process suite（`node benchmarks/metrics.mjs process`）

project `P`（`--project` ごと）について、一時ディレクトリ `T`（`mkdtempSync`、最後に削除）へ出力する。コンパイラの操作は環境変数 `TSUZURI_TIME_PASSES=1` を付けて実行し、各標本の stderr を `parseTimePasses` で読む（段の計測のコストは `Instant::now` の数回で、既定の経路と同じコードを通る）。

| variant | コマンド | target | opt | 記録する metric |
| --- | --- | --- | --- | --- |
| `check` | `tsuzuri check P` | native | none | `wall_time`・`cpu_time`・`user_time`・`sys_time`・`peak_rss`・`phase_time` |
| `build` | `tsuzuri build P -O0 --cpu generic --no-cache -o T/app-O0` | native | O0 | 同上 + `executable_bytes` |
| `build` | `tsuzuri build P -O3 --cpu generic --no-cache -o T/app-O3` | native | O3 | 同上 + `executable_bytes` |
| `build-object` | `tsuzuri build P --emit object -O3 --cpu generic --no-cache -o T/app.o` | native | O3 | 同上 + `object_bytes` |
| `build-wasm` | `tsuzuri build P --target wasm32 -O3 --no-cache -o T/app.wasm` | wasm32 | O3 | 同上 + `wasm_bytes` |
| `execute` | `T/app-O0`、`T/app-O3` | native | O0・O3 | `wall_time`・`cpu_time`・`user_time`・`sys_time`・`peak_rss` |

- `phase_time` は段ごとに 1 レコード（`phase` 欄に段の名前）。`total` も段として記録する。9 回の標本で段の名前の集合が同じでなければ throw。
- 大きさの metric は 9 回の出力の大きさがすべて同じことを確かめ、1 標本で記録する（違えば throw。ビルドの非決定性であり、停止条件 1 と同じく報告する）。
- `execute` は 9 回の stdout が同じことを確かめる（hello は `5050`）。
- `build-object` と `build-wasm` の warm-up が 0 以外で終わった project は、その variant を記録せず、stderr に `skip <P> <variant>: <1 行目>` を出して続ける（例: `export def` も `IO<T>` の main もない project の WASM は `E2004`）。
- 順序: variant を表の順に、各組で warm-up 1 回と 9 回の標本を続けて測る（quick は warm-up 0 回・1 標本）。`execute` は対応する `build` の成果物を使う。

### 確保の計数（`benchmarks/tracked_alloc.c`（新規）と `TRACKING`）

tracked の実行ファイルは `--metrics` を指定したときだけ、計時用とは別に作る。計時用の実行ファイルとその手順は変えない。

```c
// benchmarks/tracked_alloc.c（新規）。Tsuzuri の IR の確保だけを数える
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>

#define TRACKED_MAGIC UINT64_C(0x51a110ca7e)
static _Atomic uint64_t tracked_calls, tracked_bytes, tracked_live;

void *tracked_malloc(uint64_t size) {
    uint64_t *header = malloc((size_t)size + 16);
    if (!header) abort();
    header[0] = size;
    header[1] = TRACKED_MAGIC;
    tracked_calls += 1;
    tracked_bytes += size;
    tracked_live += size;
    return header + 2;
}
void *tracked_calloc(uint64_t count, uint64_t size);    // 積の溢れで abort。0 で埋めて tracked_malloc と同じ計上
void tracked_free(void *value);                         // null は無視。magic を確かめて 0 にし、live を減らす
void *tracked_realloc(void *value, uint64_t size);      // null は tracked_malloc、0 は tracked_free して NULL。
                                                        // それ以外は calls += 1、bytes += size、live を差し替え
void tracked_reset(void) { tracked_calls = 0; tracked_bytes = 0; }
void tracked_read(uint64_t *calls, uint64_t *bytes, uint64_t *live) {
    *calls = tracked_calls; *bytes = tracked_bytes; *live = tracked_live;
}
```

- 数え方は `tests/features.mjs` の host の `tracked_alloc`・`tracked_free`・`tracked_realloc` と同じヘッダー（大きさと magic の 16 bytes）を使う。Task のワーカーから呼ばれるため、計数は `_Atomic` にする。
- `instrumentAllocations(ir)`（metrics.mjs）: `@(aligned_alloc|posix_memalign|strdup|strndup|valloc|memalign)\b` が現れたら throw（停止条件 3）。そうでなければ `@(malloc|calloc|realloc|free)\b` を `@tracked_$1` に置き換え、run-computations と同じく `^attributes #\d+ = .*\n` の行と ` #\d+\b` を消す。LLVM は `malloc` と 0 埋めの `memset` を `calloc` に変えることがあるため、`calloc` も数える。
- runner（run-control の例。run-cpp も同じ形）: `inspection` を計算した後、`if (artifacts)` の前に次を行う。
  1. `control.optimized.ll`（既存。clang `-O3` と runner の `flags` の出力）を `instrumentAllocations` で `control.tracked.ll` にし、`generated` に加える（`--artifacts` で保存される）。
  2. `clang -O0 -fPIC -Wno-override-module -c control.tracked.ll -o control.tracked.o`、`clang -std=c11 -O2 -Wall -Wextra -Werror -c benchmarks/tracked_alloc.c -o tracked_alloc.o`。
  3. 計時用と同じリンクで、`control.o` を `control.tracked.o` と `tracked_alloc.o` に替え、`-DTRACKING` を付け、`-DBASELINE` と `before.o` は付けない。IR が `declare void @tsuzuri_task_parallel(` を含むときは `src/runtime/task.c` と `-pthread` を加える（`tests/features.mjs` と同じ判定）。
  4. 計時用と同じ引数（`--quick` または `--scale <s>`）で実行し、stdout の JSON を読む。
- host の `#ifdef TRACKING`（control の `benchmarks/control/host.c`）: 先頭の `printf("{\"samples\":...` の代わりに `{"tracking":1,"workloads":[` を出す。各種目で、`--scale` を適用した `work->size` と seed 42 について、参照（`work->kernels[0]`、C）の結果を先に求め、`tracked_reset()` の後に Tsuzuri（`variants[]` の `"tsuzuri"` の添字 3。`WORKLOAD` マクロの kernel の並びと一致することを確かめる）を 1 回呼び、`tracked_read` で読む。結果が参照と違うか `live != 0` なら stderr に種目名を出して終了コード 1。`{"name":"...","size":N,"calls":C,"bytes":B}` を出して次の種目へ進み（checks と計時は行わない）、最後に `]}` を出す。`tracked_reset`・`tracked_read` の宣言も `#ifdef TRACKING` の中に置く。
- cpp の `benchmarks/cpp/native.cpp` も同じ形にする。Tsuzuri の関数は `tsuzuri_ms` の計測に渡している `tz_` で始まる関数、参照は `cpp_ms` の関数。宣言は `extern "C"` にする。Rust の staticlib は計時用と同じくリンクする。
- runner は tracked の `workloads` の `name` と `size` が計時の結果と同じ順序・値であることを `assert.deepEqual` で確かめる。

### コンパイラの段階別時間（`TSUZURI_TIME_PASSES`）

新 API（実装後に有効。未検証）。

```rust
// src/timings.rs（新規）
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static PHASES: Mutex<Vec<(String, f64)>> = Mutex::new(Vec::new());

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("TSUZURI_TIME_PASSES").is_some_and(|value| value == "1")
    })
}

pub fn start() -> Option<Instant> {
    enabled().then(Instant::now)
}

pub fn finish(name: &str, started: Option<Instant>) {
    let Some(started) = started else { return };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut phases = PHASES.lock().unwrap_or_else(|error| error.into_inner());
    match phases.iter_mut().find(|(phase, _)| phase == name) {
        Some((_, total)) => *total += ms,
        None => phases.push((name.to_owned(), ms)),
    }
}

/// `{"time_passes":1,"phases":[{"name":"parse","ms":1.235}],"total_ms":5.000}`
pub fn render(phases: &[(String, f64)], total_ms: f64) -> String { /* 各 ms は {:.3}、名前は serde_json::to_string */ }

pub struct Report(Option<Instant>);
impl Report {
    pub fn start() -> Self { Self(start()) }
}
impl Drop for Report {
    fn drop(&mut self) { /* 有効なら render(&PHASES, 経過 ms) を eprintln! する */ }
}
```

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 基盤 | `src/timings.rs`（新規） | `enabled`・`start`・`finish`・`render`・`Report`（すべて新規） | 上のコード。段は最初に現れた順に並べ、同じ名前は合算する |
| 公開 | `src/lib.rs` | モジュール宣言 | `pub mod timings;` |
| 全体 | `src/main.rs` | `main` | 最初の文を `let _time_passes = tsuzuri::timings::Report::start();` にする（どの `return` でも `Drop` で 1 行を出す） |
| `load` | `src/main.rs` | `main` | `let loaded = if arguments.action == Action::Test {` の文の前で `start()`、後で `finish("load", ..)` |
| `parse` | `src/lib.rs` | `analyze_inputs_indexed_all` | `for (id, input) in inputs.iter().enumerate()` のループの前後 |
| `check` | `src/check.rs` | `check_modules_indexed_all` | 関数の最初の文で `start()`、`polymorph::specialize(` の文の直前で `finish("check", ..)`（`infer_copy_all` を含む） |
| `specialize` | 同 | 同 | `let module = polymorph::specialize(...)` の `?` を外して結果を変数に受け、`finish` の後に `let module = module?;` |
| `closures` | 同 | 同 | `closures::lower(module)` を同じ形で囲む |
| `ownership` | 同 | 同 | `diagnostics.extend(crate::ownership::check_all(&module));` の前後 |
| `emit` | `src/driver.rs` | `build_complete` | `let mut text = if options.emit == Emit::Header {` の文の前後（Windows ABI の変換と `runtime/wasm.ll` の連結は含めない） |
| `cache` | 同 | 同 | `let cache = if options.cache && options.emit != Emit::Header {` の文の前から `let cache_hit = cached.is_some();` の後まで |
| `tool.<名前>` | 同 | `run_tool` | `command.output()` の前後。名前は `Path::new(&name).file_stem()`（`clang`・`llvm-link`・`wasm-ld`・`dsymutil`）。`started.is_some()` のときだけ `format!` する |

- 段は入れ子にしない（`check` は `specialize` の前で閉じる）。`gpu::validate_calls`、成果物の書き出し、`run` の子プロセスの実行は段に入らず `total` にだけ含まれる。
- `render` の出力（新しい出力。実装後に有効。未検証。数値は形の例）:

```json
{"time_passes":1,"phases":[{"name":"load","ms":0.412},{"name":"parse","ms":1.903},{"name":"check","ms":3.221},{"name":"specialize","ms":0.874},{"name":"closures","ms":0.101},{"name":"ownership","ms":0.655}],"total_ms":7.702}
```

- 値が `1` 以外（未設定、`0`、`true` など）は無効。無効のときは `Instant` を取らず、`PHASES` にも触れない。
- LSP（`tsuzuri lsp`）も環境変数が `1` なら終了時に 1 行を出す。段の名前は固定の有限集合なので、長時間動いても `PHASES` は増え続けない。

### runner の対応（Phase 1）

各 runner に `--metrics <dir>`（新規）を加える。指定しないときの動作・出力・所要時間は今と同じ。指定したときは stdout の JSON を今と同じに出したうえで、`captureRun(dir, { compiler, mode: quick ? "quick" : "benchmark", clang, rustc })` と `makeRecord` でレコードを作り、`writeRecords(dir, <suite>, records)` で書く。旗の解析は run-control・run-cpp が既存の `take`、run-computations が既存の `options` の一覧（`["--cpu", "--baseline", "--artifacts", "--scale"]`）に `"--metrics"` を足す。

| suite | 計時のレコード（各種目・各 variant に `wall_time`・`cpu_time`） | 確保のレコード | その他 |
| --- | --- | --- | --- |
| `control` | raw の `<v>_wall_ms`・`<v>_ms`。`v` が c・cpp・rust・tsuzuri はその言語で variant `default`、before は `tsuzuri`・`before`。`opt: "O3"`、`cpu_mode` は `--cpu` | tracked の各種目: `tsuzuri`・`default` の `alloc_calls`・`alloc_bytes`（size は種目の size） | なし |
| `cpp` | raw の `cpp_*`・`rust_*`・`tsuzuri_*` | 同上 | なし |
| `computations` | native: raw の `<label>_wall_ms`・`<label>_ms`。cpp→`cpp`、rust→`rust`、direct→`tsuzuri`・`direct`、computation→`tsuzuri`・`computation`、before→`tsuzuri`・`before`。WASM: `work.wasm.raw_ms` の direct・current・before を `tsuzuri` の `direct`・`computation`・`before`、`target: "wasm32"`、`wall_time` だけ、size は `work.wasm.size` | 既存の `allocations_at_size_64` の direct・computation・before を `size: 64` で記録 | `wasm_bytes`: `workload: "module"`、`size: null`、current→`computation`、before→`before` |

- レコードの順序: 種目の順、variant の順（上の順）、metric の順（`wall_time`、`cpu_time`、`alloc_calls`、`alloc_bytes`）、最後に module のレコード。
- quick の件数（`--baseline` なし）: control は 15 × 4 × 2 + 15 × 2 = 150、computations は 9 × 4 × 2 + 9 × 2 + 9 × 2 × 2 + 1 = 127、cpp は種目数を N として 8N。
- run-computations の `extensions`（Bind2 の比較）は記録しない。

### 要約と比較

- `renderMetricsSummary(records)`（1 run 分）: 先頭に `<p>` で run_id・日付・commit の先頭 12 桁（`dirty` なら「（未コミットの変更を含む）」）・CPU・arch・コア数・OS と版・Clang・rustc・Node・Tsuzuri の SHA-256 を出す（すべて `report.mjs` と同じエスケープ）。続いて `metric`・`target`・`opt`・`cpu_mode` の組ごとに `renderTable(headers, rows, { caption })` で表を出す（`timingColumns` は渡さず、強調と相対性能の列を付けない）。
  - 列: `種目`、`仕事量`、次に `(language, variant)` の組。言語は `LANGUAGES` の順（Tsuzuri が識別列の次）、同じ言語の中は variant の出現順。見出しは言語の表示名（`Tsuzuri`・`C`・`C++`・`Rust`・`Zig`・`Go`・`C#`・`JavaScript`）に、variant が `default` 以外なら空白と variant を続ける。
  - 行: `suite/workload`（`phase` があれば ` [phase]` を続ける）、仕事量は `size`（`null` なら `-`）。値は中央値で、ms は `toFixed(3)`、bytes と count は整数。ない値は `-`。
  - caption: `${metric} (${unit}) / ${target} / ${opt} / ${cpu_mode} / 中央値`。
- `updateMetricsSummary(document, runs)`: `<!-- perf-metrics:start -->` と `<!-- perf-metrics:end -->` がそれぞれ 1 回ずつ、この順にあることを確かめ（なければ `add the perf-metrics markers to docs/benchmarks.md first` で throw）、間を `"\n" + runs.map(renderMetricsSummary).join("\n") + "\n"` に置き換える。各 run は空でなく、`run_id`・`commit`・`host` が 1 種類で、全レコードが `mode: "benchmark"` であること（quick は throw）。冪等（同じ入力で 2 回適用しても同じ文書）。
- `compareRuns(baseline, current)`: 識別キーで突き合わせ、キーの昇順に行を返す。各行は識別キーの欄、`unit`、`before`・`after`（`{ median, min, max }` または `null`）、`ratio`（after の中央値 / before の中央値。片側なら `null`）、`verdict`。
  - `verdict`: 片側だけなら `片側のみ`。両 run の host の `os`・`os_version`・`arch`・`cpu`・`cores`・`clang`・`rustc`・`node` のどれかが違えば `条件差`（数値は出す）。それ以外は `差 = after.median − before.median`、`広がり = max(before.max − before.min, after.max − after.min)` とし、`|差| ≤ 広がり` なら `差なし`、`差 < 0` なら `改善`、`差 > 0` なら `悪化`。決定的な metric は広がりが 0 なので、値が違えば必ず `改善` か `悪化` になる。
  - `tsuzuri_sha256` と `commit` の違いは比較の前提なので `条件差` にしない。`dirty` はどちらかが `true` なら表の上に 1 行の注意を出す（`renderComparison`）。
- `renderComparison(rows)`: 見出し `| 種目 | 言語 | variant | target | opt | 指標 | before 中央値 | after 中央値 | after/before | 広がり | 判定 |` の Markdown の表（区切りは `| --- |`）。端末に出すだけで、文書には書かない。

### Phase 2（人間が依頼した場合だけ）

- `run-tasks.mjs`（PR06 が使う）、`run-simd.mjs`、`run-dispatch.mjs`、`run.mjs` に `--metrics` を加える。それぞれ既存の標本（run-dispatch と run-tasks は 9 回・7 回。7 回のものは 9 回へ増やす）を `wall_time` として、suite 名は `tasks`・`simd`・`dispatch`・`mix` とする。
- `run-cache.mjs` は PX03（`benchmarks/run-build.mjs`）、run-managed の C#・JavaScript は PX02 が対応する。`run-gpu.mjs` は対象外（GPU の速度を主張しない）。

## 実装手順

1. ベースラインと棚卸し
   - 変更: なし。
   - 内容: 次を実行し、3 つの runner の quick の stdout を保存する。`src/driver.rs` の `Command::new` をすべて列挙し、`run` の子プロセス（`Command::new(&output)`）以外が `run_tool` を通ることを確かめる。

     ```sh
     cd /Users/tmidorikawa/Documents/git/Tsuzuri
     cargo build --release --locked && cargo test --locked
     node tests/benchmark_report.mjs
     mkdir -p target/perf/px01-baseline
     for suite in control cpp computations; do
       npx --yes --package=node@24 node benchmarks/run-$suite.mjs target/release/tsuzuri --quick \
         > target/perf/px01-baseline/$suite.json || break
     done
     grep -n "Command::new" src/driver.rs
     grep -n "run_tool(" src/driver.rs
     grep -n "process::exit" src/main.rs src/driver.rs src/lib.rs
     ```

   - 確認: `cargo test --locked` が成功。`node tests/benchmark_report.mjs` が終了コード 0。3 つの JSON が `node -e 'for (const p of process.argv.slice(1)) JSON.parse(require("fs").readFileSync(p, "utf8"))' target/perf/px01-baseline/*.json` で読める。`process::exit` の grep が空。`run_tool` を通らない外部ツールがあれば、手順 3 でその呼び出しも `tool.<名前>` で囲む対象として記録する。
2. `src/timings.rs` を作る
   - 変更: `src/timings.rs`（新規）、`src/lib.rs`（`pub mod timings;`）。
   - 内容: 設計のコード。`render` は `format!("{{\"name\":{},\"ms\":{:.3}}}", serde_json::to_string(name).unwrap(), ms)` を `,` でつなぎ、`{"time_passes":1,"phases":[...],"total_ms":<{:.3}>}` にする。単体テスト `render_keeps_order_and_three_decimals` を同じファイルの `#[cfg(test)] mod tests` に置く（テスト計画）。
   - 確認: `cargo test --locked --lib timings` が `running 1 test` で成功。`cargo clippy --locked --all-targets` に新しい警告がない。
3. 段の計測を入れる
   - 変更: `src/main.rs` の `main`、`src/lib.rs` の `analyze_inputs_indexed_all`、`src/check.rs` の `check_modules_indexed_all`、`src/driver.rs` の `build_complete` と `run_tool`。
   - 内容: 設計の表のとおり。`use` は各ファイルで `crate::timings`（`src/main.rs` は `tsuzuri::timings`）。
   - 確認:

     ```sh
     cargo build --release --locked
     TSUZURI_TIME_PASSES=1 target/release/tsuzuri check examples/hello
     TSUZURI_TIME_PASSES=1 target/release/tsuzuri build examples/hello -O0 --no-cache -o /tmp/tz-px01/hello
     target/release/tsuzuri check examples/hello; echo "exit=$?"
     ```

     1 つ目は stderr の最後の行が `{"time_passes":1,"phases":[{"name":"load",` で始まり、段が load・parse・check・specialize・closures・ownership の順に並ぶ。2 つ目は emit・cache・`tool.clang` も含む。3 つ目は stderr に何も出さず `exit=0`。`cargo test --locked` が成功。
4. 段階別時間の結合テスト
   - 変更: `tests/time_passes.rs`（新規）。
   - 内容: テスト計画の 4 関数。
   - 確認: `cargo test --locked --test time_passes` が `running 4 tests` で全部成功。
5. `report.mjs` から書き込みを切り出す
   - 変更: `benchmarks/report.mjs`。
   - 内容: `publishSummary` の一時ファイル＋`renameSync` の部分を `export function writeAtomically(path, text)`（新規）に移し、`publishSummary` はそれを呼ぶ。
   - 確認: `node tests/benchmark_report.mjs` が既存の assert を変えずに成功。
6. `metrics.mjs` の記録部分
   - 変更: `benchmarks/metrics.mjs`（新規）、`tests/benchmark_report.mjs`。
   - 内容: `SCHEMA`・`METRICS`・`LANGUAGES`・`median`・`summarize`・`captureRun`・`makeRecord`・`validateRecord`・`parseTimeOutput`・`parseTimePasses`・`instrumentAllocations`・`writeRecords`・`readRecords`。テスト計画の JavaScript テスト 1〜8 を加える。
   - 確認: `node tests/benchmark_report.mjs` が成功。`node -e 'import("./benchmarks/metrics.mjs").then((m) => console.log(Object.keys(m).sort().join(",")))'` が設計の export（この時点で実装したもの）を表示する。
7. 要約と比較
   - 変更: `benchmarks/metrics.mjs`、`tests/benchmark_report.mjs`。
   - 内容: `compareRuns`・`renderComparison`・`renderMetricsSummary`・`updateMetricsSummary`。テスト 9〜12 を加える。
   - 確認: `node tests/benchmark_report.mjs` が成功。
8. `measureProcess` と CLI
   - 変更: `benchmarks/metrics.mjs`。
   - 内容: `measureProcess`、process suite、CLI の `process` と `report`（`--publish` は手順 12 まで使わない）。
   - 確認:

     ```sh
     node benchmarks/metrics.mjs process target/release/tsuzuri --quick --out /tmp/tz-px01/p1
     node benchmarks/metrics.mjs report /tmp/tz-px01/p1
     grep -c '"variant":"check"' /tmp/tz-px01/p1/process.jsonl
     node benchmarks/metrics.mjs process target/release/tsuzuri --quick --out /tmp/tz-px01/p1; echo "exit=$?"
     ```

     `check` のレコードは 12 行（時間系 5 と段 7: load・parse・check・specialize・closures・ownership・total）。`executable_bytes` の O0 は同じコンパイラで `stat -f %z` した hello の大きさと等しい（2026-09-29 の M1 Max では 100,824）。`execute` の `peak_rss` は 1〜3 MB の範囲（probe は 1,687,552）。4 つ目は `process.jsonl` が既にあるため EEXIST で失敗し、`exit=1`。
9. 確保の計数と run-control
   - 変更: `benchmarks/tracked_alloc.c`（新規）、`benchmarks/control/host.c`、`benchmarks/run-control.mjs`。
   - 内容: 設計「確保の計数」と「runner の対応」。
   - 確認:

     ```sh
     npx --yes --package=node@24 node benchmarks/run-control.mjs target/release/tsuzuri --quick \
       --metrics /tmp/tz-px01/q1 --artifacts /tmp/tz-px01/control-art > /tmp/tz-px01/q1-control.json
     wc -l /tmp/tz-px01/q1/control.jsonl
     node -e 'const [a, b] = process.argv.slice(1).map((p) => JSON.parse(require("fs").readFileSync(p, "utf8"))); const k = (o) => JSON.stringify(Object.keys(o).sort()); if (k(a) !== k(b) || k(a.workloads[0]) !== k(b.workloads[0])) process.exit(1)' \
       target/perf/px01-baseline/control.json /tmp/tz-px01/q1-control.json && echo same-keys
     unifdef -UTRACKING benchmarks/control/host.c > /tmp/tz-px01/host-plain.c
     git show HEAD:benchmarks/control/host.c | diff /tmp/tz-px01/host-plain.c - && echo timed-host-unchanged
     ```

     `control.jsonl` が 150 行、`same-keys`、`timed-host-unchanged` が出る。`generate` の確認は「生成コードの確認」の 2。`--metrics` なしの実行が手順 1 と同じ所要時間の範囲で終わる（tracked のビルドをしない）。
10. run-cpp
    - 変更: `benchmarks/cpp/native.cpp`、`benchmarks/run-cpp.mjs`。
    - 内容: 手順 9 と同じ形。Tsuzuri の IR は `benchmarks/Mix.tz` と `benchmarks/cpp/Kernels.tz` の 2 つを `--emit llvm` で出し、それぞれ clang `-O3 -S -emit-llvm`（runner の `flags`）→ `instrumentAllocations` → `-O0 -c` にする。
    - 確認: 手順 9 と同じコマンドを `cpp` で実行し、`cpp.jsonl` が 8N 行（N は `Object.keys(references).length`）、`same-keys`、`unifdef -UTRACKING benchmarks/cpp/native.cpp` と `git show HEAD:benchmarks/cpp/native.cpp` の差が空。`task_parallel` の `alloc_calls` が quick で 2 回続けて同じ値。
11. run-computations
    - 変更: `benchmarks/run-computations.mjs`。
    - 内容: 設計の表のとおり。既存の tracked の経路と host は変えない。
    - 確認: quick の `--metrics` で `computations.jsonl` が 127 行、`same-keys`。
12. 文書と macOS の記録
    - 変更: `docs/benchmarks.md`、`docs/architecture.md`、`README.md`、`_docs/tools/command-line.md`、`_perfs/README.md`。
    - 内容: 「ドキュメント」の節。マーカーを置いた後、「計測手順」の本計測を 1 回行い、`node benchmarks/metrics.mjs report "$RUN" --publish` で要約を書く。
    - 確認: `node scripts/check-docs.mjs _docs/tools/command-line.md` が成功。`git diff --stat docs/benchmarks.md` がマーカーの間と新しい節・表の読み方の行だけを示す。`node benchmarks/metrics.mjs report "$RUN" --publish` を 2 回続けても 2 回目で差分が増えない。`node tests/benchmark_report.mjs` と `cargo test --locked` が成功。
13. Linux の計測（D8 の承認後、人間が依頼した場合だけ）
    - 変更: `docs/benchmarks.md`（記録だけ）。
    - 内容: x86_64 Linux と AArch64 Linux で「計測手順」を行い、`/usr/bin/time --version` と、x86_64 では `grep -owE 'avx2|avx512f' /proc/cpuinfo | sort -u` の出力を記録の本文に書く。3 台の run を `report <mac> <x86> <arm> --publish` で並べる。
    - 確認: 各 run の `host.os` が `linux`、`peak_rss` が kbytes × 1024 の値（`/usr/bin/time -v` の表示と手で照合）。

## 計測手順

PX01 の本計測（手順 12）と、PX01 形式を使う他チケットの before／after の共通手順。GUIDE §14 の PX01 形式での具体化である。

### 環境と条件

- AC 電源につなぎ、他の build・test・benchmark（端末を共有する他のエージェントのものを含む）を止める。`uptime` の 1 分の load average が論理 CPU 数の 1 割未満になってから始める。
- host の 9 欄は `captureRun` がレコードに入れる。レコードに入らない電源・メモリ量・負荷は run のディレクトリの `env.txt` に残す。Linux では下の `sw_vers`・`sysctl`・`pmset` の代わりに `uname -a`、`lscpu`、`/usr/bin/time --version`、`cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor`（あれば）を残す。
- zsh では `NODE="npx ..."` のような変数は単語に分かれないため、Node 24 は関数で包む。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node24() { npx --yes --package=node@24 node "$@"; }
RUN=target/perf/$(date -u +%Y%m%dT%H%M%SZ)-$(git rev-parse --short HEAD)
mkdir -p "$RUN"
{ git rev-parse HEAD; git status --porcelain | wc -l; sw_vers; uname -m
  sysctl -n machdep.cpu.brand_string hw.ncpu hw.memsize; pmset -g ps | head -1; uptime
  clang --version | head -1; rustc --version; node24 --version; } > "$RUN/env.txt"
```

### 本計測（1 run）

```sh
cargo build --release --locked
for suite in control cpp computations; do
  node24 benchmarks/run-$suite.mjs target/release/tsuzuri --metrics "$RUN" > "$RUN/$suite.json" || break
done
node24 benchmarks/metrics.mjs process target/release/tsuzuri --out "$RUN"
node24 benchmarks/metrics.mjs report "$RUN" > "$RUN/report.txt"
ls "$RUN"
```

- 期待: `control.json`・`control.jsonl`・`cpp.json`・`cpp.jsonl`・`computations.json`・`computations.jsonl`・`process.jsonl`・`env.txt`・`report.txt` がそろい、`report` が throw しない（`readRecords` の検証が全行で通る）。
- warm-up と標本: in-process suite は各 runner の既存の warm-up・較正・12 標本（`--baseline` 付きは 15）。process suite は組ごとに warm-up 1 回と 9 標本。どちらも変えない。
- suite は上の順に 1 つずつ実行し、並列にしない。`--cpu native` は別の run_id で測る（同じ suite を同じディレクトリに二度書けない）。

### before／after（他チケットの共通手順）

1. 変更に着手する前に `cargo build --release --locked && mkdir -p /tmp/tz-<ID> && cp target/release/tsuzuri /tmp/tz-<ID>/tsuzuri-before` で変更前のコンパイラを残す。
2. 本計測と同じコマンドで、`RUN=target/perf/<ID>-before`（compiler は `/tmp/tz-<ID>/tsuzuri-before`）、`RUN=target/perf/<ID>-after`（`target/release/tsuzuri`）、`RUN=target/perf/<ID>-before2`（再び before）の順に測る。suite はチケットの `計測対象` に当たるものだけでよいが、3 つの run で suite と旗をそろえる。
3. `node24 benchmarks/metrics.mjs report target/perf/<ID>-before2 --baseline target/perf/<ID>-before` で時間系の metric に `改善`・`悪化` が出た行は、環境の揺れが広がりを超えている。その metric は「判定できない」と報告し、改善を主張しない。
4. `node24 benchmarks/metrics.mjs report target/perf/<ID>-after --baseline target/perf/<ID>-before` の表を完了報告に貼る。判定は `verdict` をそのまま使い、`条件差` の行から結論を出さない。
5. control と computations では、after の run に `--baseline /tmp/tz-<ID>/tsuzuri-before` を付けてもよい。同じプロセス内で交互に測った `tsuzuri before` の行が同じ run に入り、`report` の表で `tsuzuri default` と並べて読める（run-cpp には `--baseline` がない）。

### PX01 自身の確認

- 手順 1 の直後に `mkdir -p /tmp/tz-px01 && cp target/release/tsuzuri /tmp/tz-px01/tsuzuri-head` で HEAD のコンパイラを残し、手順 12 で無効時の上乗せを確かめる。中央値の差が両者の広がり以内なら「差なし」と記録する。超えた場合は報告し、`timings::enabled` の判定より前に既定の経路で `Instant::now` や `format!` を呼んでいないか確かめる。

```sh
node24 -e 'import("./benchmarks/metrics.mjs").then(({ measureProcess, summarize }) => {
  for (const compiler of process.argv.slice(1))
    console.log(compiler, JSON.stringify(summarize(measureProcess(compiler, ["check", "examples/hello"]).wall_ms)));
})' /tmp/tz-px01/tsuzuri-head target/release/tsuzuri
```

### 記録と公開

- 生データは `target/perf/` に残し、Git に加えない（D2）。リポジトリ外の長期保管は D3 の承認まで行わない。
- `docs/benchmarks.md` へ公開するのは `mode: "benchmark"` の run だけ（`node24 benchmarks/metrics.mjs report "$RUN" --publish`）。公開した run_id と commit を完了報告に書く。

## 生成コードの確認

PX01 は Tsuzuri の生成コードを変えない。確かめるのは、段の計測が成果物に触れないことと、tracked のビルドが Tsuzuri の IR の確保を漏れなく置き換え、計時用のビルドに影響しないことの 2 つ。

### 1. `TSUZURI_TIME_PASSES` の有無（停止条件 1）

出力保護（`E2003`）に当たらないよう、毎回新しいディレクトリに出す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
G=$(mktemp -d /tmp/tz-px01-gen.XXXXXX)
for emit in llvm header object; do
  target/release/tsuzuri build examples/hello --emit $emit -O3 --no-cache -o $G/a.$emit
  target/release/tsuzuri build examples/hello --emit $emit -O3 --no-cache -o $G/a2.$emit
  TSUZURI_TIME_PASSES=1 target/release/tsuzuri build examples/hello --emit $emit -O3 --no-cache -o $G/b.$emit 2> $G/b.$emit.err
  cmp $G/a.$emit $G/a2.$emit && cmp $G/a.$emit $G/b.$emit && echo "same $emit"
done
for kind in exe wasm; do
  extra=(); [[ $kind == wasm ]] && extra=(--target wasm32)
  target/release/tsuzuri build examples/hello "${extra[@]}" -O3 --no-cache -o $G/a.$kind
  target/release/tsuzuri build examples/hello "${extra[@]}" -O3 --no-cache -o $G/a2.$kind
  TSUZURI_TIME_PASSES=1 target/release/tsuzuri build examples/hello "${extra[@]}" -O3 --no-cache -o $G/b.$kind 2> /dev/null
  cmp $G/a.$kind $G/a2.$kind && cmp $G/a.$kind $G/b.$kind && echo "same $kind"
done
```

- 期待: `same llvm`・`same header`・`same object`・`same exe`・`same wasm`。各 `b.*.err` はちょうど 1 行で、`{"time_passes":1,` で始まる。
- `a` と `a2` が違う成果物は既存の非決定性であり PX01 の原因ではない。比較から外して報告に書く。停止条件 1 は `a`＝`a2` かつ `a`≠`b` のとき。hello の WASM が `E2004` で作れない場合は `wasm` を外す（process suite も同じく skip する）。

### 2. tracked IR

手順 9 の `--metrics --artifacts /tmp/tz-px01/control-art` の後に行う。tracked の IR は `*.tracked.ll` の名前で `generated` に加える（run-cpp も同じ名前の規則）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
A=/tmp/tz-px01/control-art; B=$(mktemp -d /tmp/tz-px01-plain.XXXXXX)
npx --yes --package=node@24 node benchmarks/run-control.mjs target/release/tsuzuri --quick --artifacts $B > /dev/null
grep -cE '@(malloc|calloc|realloc|free)\(' $A/*.tracked.ll
grep -cE 'call .*@tracked_(malloc|calloc|realloc)\(' $A/*.tracked.ll
grep -c '^attributes #' $A/*.tracked.ll
grep -nE '@(aligned_alloc|posix_memalign|strdup|strndup|valloc|memalign)\(' $A/*.optimized.ll
for f in control.ll control.optimized.ll control.s; do cmp $A/$f $B/$f || echo "differs $f"; done
```

- 期待: 1 つ目は `0`（`declare` を含めて元の名前が残らない）。2 つ目は 1 以上（stdout の `inspection.array_allocator_calls` が 0 でない限り）。3 つ目は `0`。4 つ目は空（空でなければ停止条件 3）。5 つ目は何も出さない（`--metrics` の有無で計時用の IR と asm が同じ）。
- 計時用の host が `#ifdef TRACKING` の外で変わらないことは手順 9・10 の `unifdef` で、`live == 0` は tracked 実行が終了コード 0 で終わることで確かめる。

## テスト計画

### Rust テスト

- `src/timings.rs` の `#[cfg(test)] mod tests` の `render_keeps_order_and_three_decimals`: `render(&[("parse".into(), 1.23456), ("check".into(), 2.0)], 5.0)` が `{"time_passes":1,"phases":[{"name":"parse","ms":1.235},{"name":"check","ms":2.000}],"total_ms":5.000}` と等しい。
- `tests/time_passes.rs`（新規）。コンパイラは `env!("CARGO_BIN_EXE_tsuzuri")` を `std::process::Command` で起動し、入力は `concat!(env!("CARGO_MANIFEST_DIR"), "/examples/hello")`、出力は `std::env::temp_dir()` の下に test 名と `std::process::id()` を含む新しいディレクトリ。環境変数は子にだけ `.env(..)`／`.env_remove(..)` で渡す。段の合計の比較は丸めを考え、`sum(ms) <= total_ms + 0.0005 * (段の数 + 1)` とする。
  1. `time_passes_is_off_by_default`: 未設定・`"0"`・`"true"` の `check` は成功し、stderr に `time_passes` を含まない。存在しない project（temp の下の `missing`）の `check` は、`"1"` のとき終了コードが既定と同じで、stderr が既定の stderr に `{"time_passes":1,` で始まる 1 行を足したものと等しい。
  2. `check_reports_frontend_phases_in_order`: 最後の行を `serde_json` で読み、段の名前が `["load", "parse", "check", "specialize", "closures", "ownership"]` と一致し、各 `ms >= 0`、合計の規則を満たす。
  3. `build_reports_backend_phases_and_tools`: `build <hello> -O0 --no-cache -o <temp>/hello` の段が `emit`・`cache`・`tool.clang` を含み、全名前が設計の正規表現に一致し、重複せず、合計の規則を満たす。
  4. `time_passes_does_not_change_artifacts`: `--emit llvm` と `--emit object` を有無で出し、`std::fs::read` の内容と stdout が等しい。

### JavaScript テスト（`tests/benchmark_report.mjs` に追加）

期待値は手計算か既知の値で、実装の出力から作らない。テスト 1〜8 は手順 6、9〜12 は手順 7、13 は手順 8、14 は手順 9 で加える（手順 8・9 の `変更:` に `tests/benchmark_report.mjs` を加えて読む）。

1. `median([3, 1, 2]) === 2`、`median([4, 1, 3, 2]) === 2.5`。`summarize([5, 1, 3])` が `{ samples: [5, 1, 3], median: 3, min: 1, max: 5 }`（標本の順を保つ）。`[]`・`[NaN]`・`[-1]` は throw。
2. `parseTimeOutput` の darwin: 「現状と計測」の probe の出力（`0.16 real 0.11 user 0.05 sys`、`43253760  maximum resident set size`）が `{ wall_ms: 160, user_ms: 110, sys_ms: 50, peak_rss_bytes: 43253760 }`。RSS の行がなければ `unrecognized /usr/bin/time output`、`system: "win32"` も throw。
3. `parseTimeOutput` の linux: `Elapsed (wall clock) time (h:mm:ss or m:ss): 1:02:03.45` が `wall_ms: 3723450`、`0:00.16` が 160、`Maximum resident set size (kbytes): 42240` が `43253760`（42,240 × 1,024）、`User time (seconds): 0.11` が 110。
4. `parseTimePasses`: 他の行と `{"time_passes":1,...}` の行が 2 つある stderr で最後の行を返す。行がない、または `"time_passes":2` なら throw。
5. `makeRecord`・`validateRecord`: 設計の例と同じ値（標本だけ `mode: "benchmark"` 用に 9 個）から作ったレコードの `Object.keys` が 22 欄の順と一致する。次をそれぞれ 1 つだけ壊すと throw: 欄の追加、`host.node` の欠落、39 桁の `commit`、`date: "2026-09-29T10:15:00Z"`（`toISOString` と一致しない）、`unit: "ms"` の `peak_rss`、`phase: "parse"` の `wall_time`、`phase: "link"` の `phase_time`、`samples: [1.5]` の `alloc_calls`、benchmark で 8 標本の `wall_time`、`samples: [1, 2]` の `alloc_bytes`、`median` の改ざん。
6. `captureRun`: temp の `px01-test` ディレクトリで `run_id === "px01-test"`、`commit` が 40 桁、`date` が ISO、`host` が 9 欄。compiler に内容 `abc` のファイルを渡すと `tsuzuri_sha256` が `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`。ディレクトリ名 `bad name` は throw。
7. `writeRecords`・`readRecords`: 2 つの suite を書いて名前順・行順に読み戻す。同じ suite の 2 回目は `EEXIST`。識別キーの重複、`run_id` の混在、`suite` の不一致は throw。
8. `instrumentAllocations`: `declare ptr @malloc(i64) #1`・`call ptr @calloc(i64 4, i64 8)`・`@realloc`・`@free`・`attributes #1 = { nounwind }` を含む IR が手で書いた期待の文字列と一致する。`@malloc_usable_size` は置き換えない。`@strdup`・`@posix_memalign` を含めば throw。
9. `compareRuns`: before `[10, 11, 12]`・after `[8, 9, 9]` は広がり 2、差 −2 で `差なし`、`ratio` は 9 / 11。before `[10, 10, 11]`・after `[13, 13, 14]` は `悪化`。`alloc_calls` の 5→4 は `改善`。片側だけのキーは `片側のみ`、`host.cpu` が違えば `条件差`。行は識別キーの昇順。
10. `renderComparison`: 見出し行が設計の 11 列と一致し、区切りは `| --- |` が 11 個、片側の値は `-`。
11. `renderMetricsSummary`: caption `peak_rss (bytes) / native / O0 / generic / 中央値` を含む。variant `execute` の列見出しは `Tsuzuri execute` で、`C` の列より前。`size: null` は `-`、ms は小数 3 桁。workload の `<` は `&lt;`。
12. `updateMetricsSummary`: マーカーがなければ `add the perf-metrics markers to docs/benchmarks.md first`。2 回適用しても同じ文書、マーカーの外は不変、quick の run は throw、2 つの run は両方出る。
13. `measureProcess`: `process.execPath` の `-e "const b = Buffer.alloc(64 * 1024 * 1024, 1); console.log(b[12345])"` を `{ runs: 2, warmups: 1 }` で測り、配列の長さが 2、stdout が `1\n`、`peak_rss_bytes` が 67,108,864 以上。`/bin/sh -c "echo err >&2; exit 3"` は `exited with 3` と `err` を含んで throw。`system: "win32"` は throw。Linux で `/usr/bin/time` がなければ `skip measureProcess: /usr/bin/time not found` を出して飛ばす。
14. `benchmarks/tracked_alloc.c`: テストが temp に書く C の main と `$TSUZURI_CLANG`（既定 `clang`）の `-std=c11 -O2 -Wall -Wextra -Werror` でリンクする。`p = tracked_malloc(10)`、`q = tracked_calloc(4, 8)`（32 bytes が 0）、`p = tracked_realloc(p, 100)`、`r = tracked_realloc(NULL, 16)`、`tracked_realloc(r, 0)` が `NULL`、`p`・`q`・`NULL` を free した後の `tracked_read` が calls 4、bytes 158（10 + 32 + 100 + 16）、live 0。`tracked_reset` の後は calls 0・bytes 0。別の main の `tracked_calloc(UINT64_MAX, 2)` は `SIGABRT`。

### E2E と意味の保持

- 新しい E2E suite は作らない。runner の quick の `--metrics` 実行（手順 9〜11）が E2E に当たり、各 runner の既存の checksum と参照値の照合、`live != 0` の拒否、WASM の import が空であることの assert がそのまま働く。`src/driver.rs`・`src/check.rs`・`src/lib.rs`・`src/main.rs` の変更に対して、GUIDE §3.1 のテスト選択表の suite を最後に 1 回実行する。生成コードは変えないため native／WASM × `-O0`／`-O3` の追加の確認は「生成コードの確認」の 1 で足りる。

### 既存テストへの影響

なし。`tests/benchmark_report.mjs` の既存の assert は変えない（手順 5）。runner の stdout の欄（`run-managed.mjs` と `report.mjs` が読む）も変えない。性能の閾値は置かない。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/benchmarks.md` | `## 表の読み方` | 性能指標の表は中央値で、最小値の強調と相対性能の列がないこと。`alloc_*` は Tsuzuri の IR の確保の累計、`peak_rss` は子プロセスを含む最大の 1 つ、`quick` は公開しないこと |
| `docs/benchmarks.md` | `## 性能指標の記録`（新規。`## ベンチマーク サマリー` の直後） | 「計測手順」の本計測と before／after のコマンド、metric の一覧（単位と定義）、`target/perf/<run_id>/` の置き方、`<!-- perf-metrics:start -->` と `<!-- perf-metrics:end -->` のマーカー |
| `docs/architecture.md` | `## 開発と検証` | `TSUZURI_TIME_PASSES=1` の出力形式、段の名前と範囲（入れ子にしない、`total` だけに入る処理）、`src/timings.rs` の役割 |
| `README.md` | ベンチマークのコマンドを並べた箇所（`grep -n "run-control.mjs" README.md` で探す） | `--metrics <dir>` と `node benchmarks/metrics.mjs process`／`report` を 1 行ずつ |
| `_docs/tools/command-line.md` | `## ツールと環境変数` | 表に `TSUZURI_TIME_PASSES` の行（`1` のとき終了時に段階別時間の JSON を stderr へ 1 行出す。成果物は変わらない） |
| `_perfs/README.md` | `## 一覧`、`## 運用ルール` | PX01 の状態を `done`。性能チケットは before／after を PX01 形式で `target/perf/<ID>-before`・`<ID>-after` に記録し、`report --baseline` の表を完了報告に貼る規則 |

- `_docs/feature-status.md` と `_features/README.md` は変えない（性能チケットは載っていない）。確認: `node scripts/check-docs.mjs _docs/tools/command-line.md` と `git diff --check` が成功する。

## 受け入れ条件

- [ ] 手順 1〜12 の `確認:` がすべて期待どおり。
- [ ] `--metrics` なしの 3 runner の stdout の欄が手順 1 の保存と同じ（`same-keys`）で、計時用の host が `#ifdef TRACKING` の外で `f8dc655` と同じ（`timed-host-unchanged`）。
- [ ] quick の `--metrics` で control 150 行・computations 127 行・cpp 8N 行が書かれ、`report` が全行を検証して表示する。
- [ ] process suite が表の variant の時間・RSS・段・大きさを記録し、同じディレクトリへの 2 回目は `EEXIST` で終了コード 1。
- [ ] control・cpp・computations の Tsuzuri 実装に `alloc_calls`・`alloc_bytes` が記録され、tracked 実行が `live == 0` で終わり、tracked IR に元の確保関数が残らない（生成コードの確認 2）。
- [ ] `TSUZURI_TIME_PASSES=1` の有無で成果物がバイト単位で同じで（生成コードの確認 1）、既定の stderr・終了コードが変わらず、段の合計が `total_ms` を超えない。
- [ ] `cargo test --locked`（`timings` 1 件と `time_passes` 4 件を含む）、`cargo clippy --locked --all-targets`（新しい警告なし）、`cargo fmt --check`、`node tests/benchmark_report.mjs`（テスト 1〜14）が成功。
- [ ] `Cargo.toml`・`Cargo.lock` に差分がなく、npm の依存を足しておらず、`unsafe` がない。
- [ ] macOS の benchmark の run を 1 つ `docs/benchmarks.md` のマーカーの間に公開し、再公開で差分が増えない。生データは Git に入っていない。
- [ ] 「ドキュメント」の全項目を更新し、その確認コマンドが成功。`_perfs/README.md` の PX01 が `done`。
- [ ] GUIDE §10 の完了の定義を満たす。PX01 自身は速度の改善を主張しない。Linux の記録（手順 13、D8）は Phase 1 の完了条件に含めない。

## 落とし穴

- `time` は zsh の予約語。常に `/usr/bin/time` を絶対パスで呼ぶ（`measureProcess` も同じ）。macOS の最大 RSS は bytes、Linux は kbytes。macOS の値に 1024 を掛けない（テスト 2・3 が守る）。`peak memory footprint` は別の量で使わない。
- `build` の `peak_rss` は待ち合わせた子プロセス（clang）を含む最大の 1 つ。コンパイラ単体のメモリは `check` の値で読む。
- 新しくリンクした実行ファイルの初回の実行は、user・sys が 0 でも real が大きい（probe で 0.22 s）。benchmark の warm-up を 0 にしない。逆に `user`・`sys` は 10 ms 単位で、hello の `check` では 0 が普通に出る。0 を異常として捨てない。
- `writeRecords` は `wx` で書く。同じディレクトリで測り直すと `EEXIST` になるので、古い run を消さずに新しい run_id を使う。
- `median`・`min`・`max` は `summarize` が生の標本から計算する。標本を先に丸めると `validateRecord` の規則 8 が失敗する。
- tracked の IR は属性の除去と `-O0` の両方が要る。どちらかを欠くと LLVM が確保を消したり動かしたりし、数が変わる。tracked のリンクに `-DBASELINE`・`before.o` を入れない（host の添字 3 が Tsuzuri でなくなる）。
- IR が `declare void @tsuzuri_task_parallel(` を含むのに `src/runtime/task.c` と `-pthread` を足さないと、`tsuzuri_task_*` が未定義になる。計数は `_Atomic` にする（`task_parallel` の値が揺れたら計数の競合を疑う）。
- `alloc_bytes` は累計で、同時に生きている量の最大ではない。PM チケットが最大の生存量を要する場合は schema 2 の提案として報告する。
- `TSUZURI_TIME_PASSES` は `OnceLock` に最初の値が残る。テストは子プロセスに `.env()` で渡し、`std::env::set_var` を使わない（edition 2024 では `unsafe` でもある）。
- 段の値は小数 3 桁に丸めて出るため、合計が `total_ms` をわずかに超えうる。許容は値ごとに 0.0005 までとし、それを超えるのは入れ子による二重計上（停止条件 2）。
- `unifdef` は行を削除すると終了コード 1 を返す。手順 9・10 の確認で `&&` につながない。PX01 の途中でコミットした場合、`git show HEAD:...` の `HEAD` は `f8dc655` に替える。
- 同じ run でも suite ごとに `captureRun` の `date` が違うのは正常で、要約には最も早い `date` を出す。未コミットの変更で測ると `dirty: true` になり、要約に注意が出る。そのまま公開せず、人間に確認してから公開する。

## 対象外

- 性能の合否判定の自動化、共有 CI への速度・メモリの閾値、クラウドの計測サービスの運用。
- コンパイラの段階別のメモリ（D6）、同時に生きている確保量の最大、ハードウェアカウンター（`instructions retired`・cache miss）、統計的検定（D7）、C・C++・Rust 実装の確保の計数（D5）。
- Phase 2 の runner（run-tasks・run-simd・run-dispatch・run.mjs）、run-cache（PX03）、run-managed の C#・JavaScript（PX02）、run-gpu。
- Windows の計測（G10）。Linux の計測（D8 の承認後）と生データの長期保管（D3 の承認後）。

## 決定事項

### D1: 記録の形式

- 決定: JSON Lines、`schema: 1`、設計の 22 欄を固定順で持ち、host を行ごとに複製する。欄の追加・改名・意味の変更は schema 2 とし、`validateRecord` は schema 1 だけを受け付ける。
- 理由: 連結・grep・追記ができ、suite や run をまたいでも 1 行で解釈できる。後続の約 20 チケットが同じ読み手（`readRecords`・`compareRuns`）を使える。
- 状態: 既定案（実装者はこの案に従う）

### D2: 生データの置き場所と公開

- 決定: 生データと runner の stdout は `target/perf/<run_id>/` に置き、Git に加えない。`docs/benchmarks.md` には `mode: "benchmark"` の run の要約だけを perf-metrics のマーカーの間に置く。他チケットは before／after の表を完了報告に貼り、要約の再公開は人間が依頼した場合だけ行う。
- 理由: 起票時の未決事項「生データの保存先」のうち、リポジトリ内で決められる部分。生データは機械ごとに異なり大きいため Git に向かない。要約の書き換えを限ると文書の差分が追いやすい。
- 状態: 既定案（実装者はこの案に従う）

### D3: 生データのリポジトリ外での長期保管

- 決定: Phase 1 では行わない。承認された場合の案は、公開した run ごとに `tar -czf <run_id>.tar.gz -C target/perf <run_id>` を作り、人間が指定する保管先に置き、`docs/benchmarks.md` の要約の run_id から辿れるようにする。
- 理由: 起票時の既定案（リポジトリ外の保存場所）は保管先・費用・アクセス権の選定を伴い、実装者が決められない。
- 状態: 要承認（承認前は外部へ保存しない。Phase 1 の手順 1〜12 には影響しない）

### D4: 最大 RSS と CPU 時間の取り方

- 決定: macOS は `/usr/bin/time -l`、Linux は GNU time 1.8 以上の `/usr/bin/time -v` を `-o <file>` 付きで使う。起票時の `benchmarks/measure.c` 案と、Rust からの `getrusage` は作らない。
- 理由: Rust からは `unsafe`（`unsafe_code = "forbid"`）か crate の追加が要る。OS 付属の `/usr/bin/time` は wait4 の rusage と同じ値を出し、`-o` で子の stderr・終了コードと分けられる（probe で確認）。独自の計測器は保守と検証の対象が増える。
- 状態: 既定案（実装者はこの案に従う）

### D5: 確保の計数

- 決定: `--metrics` のときだけ、Tsuzuri の IR の `malloc`・`calloc`・`realloc`・`free` を `tracked_*` に置き換えた別の実行ファイルを作り、`benchmarks/tracked_alloc.c`（新規）で数える。`alloc_calls` は null でも 0 でもない `realloc` を含む回数、`alloc_bytes` は要求バイト数の累計。C・C++・Rust の実装は数えない。
- 理由: 計時用の実行ファイルを乱さない。run-computations と E2E の既存の方法（16 bytes のヘッダーと magic）と同じで、既存の値と比べられる。他言語の allocator を置き換えると比較の意味が変わる。
- 状態: 既定案（実装者はこの案に従う）

### D6: コンパイラの段階別時間

- 決定: 環境変数 `TSUZURI_TIME_PASSES=1` で、終了時に stderr へ 1 行の JSON を出す。段は入れ子にせず、同じ名前は合算する。段階別のメモリは出さず、プロセス全体の `peak_rss`（process suite）で代える。
- 理由: CLI の旗にしないことで引数の解析・既定の出力を変えず、LSP やテストの子プロセスにも同じ方法で渡せる。起票時の受け入れ条件「段階別時間とメモリ」のうちメモリは、D4 と同じ理由で Rust から取れない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 比較の判定

- 決定: 設計の `compareRuns` の規則（中央値の差と、最小〜最大の広がりの大きい方の比較）だけを使う。統計的検定はしない。比較の結果で終了コードを変えない。
- 理由: 標本 9〜15 では検定の前提が弱い。広がりとの比較は保守的で、改善を誤って主張しにくい。AGENTS.md は共有 CI の速度の合否条件を禁じている。
- 状態: 既定案（実装者はこの案に従う）

### D8: 計測機の確保

- 決定: Phase 1 は macOS（Apple M1 Max）1 台で記録する。承認された場合は x86_64 Linux の実機 1 台（F05 の x86 経路の実機検証も兼ねる）と AArch64 Linux 1 台を加え、手順 13 を行う。仮想機と共有 CI の runner は使わない。
- 理由: 起票時の既定案（x86_64 Linux の実機の追加）と受け入れ条件「3 種類の計測機」を引き継ぐ。計測機の調達と維持は費用と運用を伴う。仮想機は揺れが大きい。
- 状態: 要承認（承認前は手順 13 に着手しない）

### D9: Phase 1 の範囲

- 決定: PX01 形式を書くのは run-control・run-cpp・run-computations と process suite だけ。他の runner は設計の「Phase 2」、run-cache は PX03、run-managed の C#・JavaScript は PX02 が扱う。
- 理由: run-managed が読む 3 runner と、ビルド時間・RSS・成果物サイズの基準（hello）で、後続チケットの before／after に足りる。起票時の「既存のベンチマークすべて」は Phase 2 と PX02・PX03 に分けて満たす。
- 状態: 既定案（実装者はこの案に従う）
