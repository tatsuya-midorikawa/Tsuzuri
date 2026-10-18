# PR07: プロファイル誘導最適化（PGO）

| 項目 | 内容 |
| --- | --- |
| ID | PR07 |
| 分類 | 実行速度 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | PX01, (PB01) |
| 関連 | PR08, G11 |
| 状態 | done |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`）。2026-10-17 に HEAD `9dd0fbbc` で再検討（「着手時の再検討」） |
| 承認 | 2026-10-17 の全フェーズ実装依頼で、D1〜D10（旗 `--profile-generate`・`--profile-use` と `TSUZURI_LLVM_PROFDATA`、D6 の範囲、D7 の WASM、D8 の C/C++ 参照、D9 の `!prof` を含む）と、再検討で足した D11〜D17 を承認済み（測定で効果を確認できない既定の切り替えは行わない。PGO は profile が要るので既定にならない）。診断は既存の `E2000`・`E2001`・`E2002`・`W2001` だけを使う |
| 手本にする既存実装 | 値を取る build 専用の旗と組み合わせの拒否: `src/main.rs` の `parse_arguments` の `--cpu`・`--wasm-feature`（`"--wasm-feature is only valid with build"` を返す `Err`）と、`src/driver.rs` の `BuildOptions::validate`。外部ツールの起動と失敗の報告: `src/driver.rs` の `tool`・`run_tool`・`collect_message`（`TSUZURI_LLVM_LINK` の `llvm-link` の呼び出し）。cache の鍵への条件付きの追加: `src/cache.rs` の `build_key`（`options.cpu == Cpu::Native` のときだけ足す field）。ツールを差し替える E2E: `tests/cache.mjs` の `cli` と clang の wrapper |
| 主な影響ファイル | `src/main.rs`（`parse_arguments`・`Arguments`・`run_action`・`HELP`・`mod tests`）, `src/driver.rs`（`build`・`build_complete`・`build_profiled`（新規）・`Profile`（新規）・`prepare_profile`（新規）・`find_profdata`（新規）・`summarize_profile_warnings`（新規））, `src/cache.rs`（`build_key`）, `tests/pgo.mjs`（新規）, `tests/fixtures/pgo/`（新規）, `benchmarks/run-control.mjs`（`--pgo`（新規））, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md`, `_docs/tools/command-line.md`, `_docs/tools/build-and-cache.md`, `_perfs/README.md`（状態欄）。2026-10-17 の再検討で `src/profile.rs`（新規）、`_tsuzuri/language-reference/compiler/{option,usage,diagnostics,platforms,native-interop}.md`、`benchmarks/run-control.mjs`、`tests/pgo.mjs` などに更新（「着手時の再検討」の作業パッケージ） |
| 計測対象 | `node benchmarks/run-control.mjs` の Tsuzuri の行（native、`-O3`、`--cpu generic`）。主指標は分岐の多い `match_dispatch`・`while_mix`・`for_mix`・`tail_mix`・`record_pipeline`。成果物サイズとビルド時間は参考として記録する |

## 目的

実行時のプロファイルを使って、分岐の配置・インライン展開・関数の配置を LLVM に最適化させる。
C/C++（Clang の `-fprofile-generate`／`-fprofile-use`）、Rust、Go が持つ PGO を Tsuzuri の native ビルドでも使えるようにし、分岐の多い処理で
C/C++ と同じ手段で競えるようにする。PGO は最適化の判断だけを変え、プログラムの意味は一切変えない。

実装者は Phase 1（native の計装・適用・cache・診断・計測）だけを行う。Phase 2（設計「Phase 分割」）は人間が求めた場合だけ着手する。

## 着手時の再検討（2026-10-17）

2026-10-17 の依頼で、Phase 1 と Phase 2（D6〜D9 を含む）をすべて実装する。以下の「現状と計測（HEAD `f8dc655`）」「実装手順」「決定事項」は起票時の記録で、
この節と食い違う箇所はこの節が優先する（旧 `_docs/tools/*.md` は `_tsuzuri/language-reference/compiler/` の各ページ、GUIDE §8.1）。
着手時の HEAD は main `9dd0fbbc`（第2・第3フェーズ統合後）＋ docs の足場 `a2313d19`。偵察は変更前のコンパイラ（SHA-256 `5e1b9687…`）に、
`TSUZURI_CLANG` の wrapper で旗を注入して行った（製品コードは変えていない）。機械は M1 Max、macOS 27.0.1、Apple clang 21.0.0（clang-2100.3.34.2）、
Homebrew LLVM 21.1.8（clang・llvm-profdata。CI の LLVM と同じ版）、比較用に Homebrew LLVM 22.1.8・23.1.1。共有機で 1 分の load average は 7〜34。
生データと scratch は `target/perf/PR07/`（コミットしない）。

### 起票時の記述と現行のコード

| # | 起票時 | 現行（確認済み） | 影響 |
| --- | --- | --- | --- |
| 1 | `BuildOptions` は `#[derive(Clone, Copy, Debug)]` で 10 field。`-O<n>` は数値 | 22 field（`wasm_jspi`・`wasm_webgpu`・`wasm_net`・`wasm_host`・`wasm_max_memory`・`wasm_stack_size`・`trap_return`・`allocator`・`freestanding`・`strip`・`wasm_opt`・`triple` が増えた）。`Copy`・`Debug` は同じで `PartialEq` はない。`optimization` は `OptLevel`（`-O0`〜`-O3`・`-Os`・`-Oz`、`flag()`） | D4（`Profile` は `BuildOptions` の外）は有効。`-Os`・`-Oz` との組み合わせは D16。他チケットの field 追加で `{options:?}` の鍵は変わるので、条件は「`None` のとき `profile` の field を足さない」 |
| 2 | `driver::build` を `main.rs`・`tests/host_abi.rs`・`tests/trap_locations.rs` が呼ぶ | `main.rs` の `Action::Build` は `driver::build_linked(module, project, output, options, links)` を呼び、`build` は `build_linked(…, &LinkInputs::default())` の薄い wrapper。`build` の呼び出しは `tests/` の 6 ファイル 15 か所（`f16_hardware`・`host_abi`・`fixed_arrays`・`targets`(6)・`trap_locations`(4)・`async`(2)）。`build_complete` の呼び出し元は `build_linked` と `run_process`（`run`・`script`・REPL の `run_captured`）の 2 つ | 新しい入口は `build_profiled(module, project, output, options, links, &Profile)`（起票時の形は `links` がなく `main.rs` から呼べない）。`build`・`build_linked` の signature は不変。`run_process` へは Phase 2（WP8） |
| 3 | 本体は `clang -x ir … -O<n> -lm -x none task.o`。`dwarf_sidecar` と `-r -nostdlib` は別の `Command` | `build_complete` は約 1,230 行（3003〜4237）。cache あり・`-g`/`--trap-info`/trap-return/threads/stack 検査なしの native 実行ファイルと wasm32 `.wasm` は `runtime_objects`（PB01）で、`numeric`・`math` を `compile_runtime_object`（同梱 → `runtime/` cache → Clang）の別 object にする。C ランタイムは `native-c` として cache される。wrapper で数えた clang の呼び出し: 整数だけを表示する program の cold build は本体 1 回（`-x ir -Wno-override-module -O3 -fPIC module.ll -o artifact -lm -Wl,-dead_strip`。`--strip` は `-Wl,-x`、`--cpu native` は `-mcpu=native`（x86 は `-march=native`））と同定の `--version` 1 回、warm は `--version` だけ、float を表示する cold は numeric の object が増えて 2 回、`-g`（macOS）は `-c` と別リンク（`-g -lm`）の 2 回と `dsymutil`、`--emit object` は `-c` の 1 回 | 計装の旗を足す所は、本体の `Command`（`-O<n>` の直後）と `dwarf_sidecar` の別リンクの 2 か所。numeric・math・C ランタイムの object と `-r` のリンクには足さない（D6） |
| 4 | D6: IR ランタイム（`numeric.ll` など）は同じ module に連結されて計装・PGO の対象になり、PB01 の object は対象外 | 既定（cache あり）の native 実行ファイルでは `numeric.ll`（`tz_soft_*`）と `math.ll`（`tz_math_*`）は別 object で、module には `declare hidden` と sqrt/fabs の wrapper だけが入る。IR のまま module に入る（計装・PGO の対象になる）のは、`--no-cache`・`--emit object`・`--trap-info`・`-g` の build と、`trap_info` が既定の `run`、および string・heap・closure・console・display・integer（PM08）などの小さい IR ランタイム。float を表示する program で、cache あり経路の profile を `--no-cache` の build に使った結果は mismatch 0 件 | 範囲が build の経路で変わる。D6 を更新。generate と use は同じ経路（cache 設定・`--trap-info`・`-g`）で行うと docs に書く（D14） |
| 5 | PGO 名は `source_filename = "tsuzuri"`（落とし穴） | `src/llvm.rs:1115` に 1 か所のまま。`llvm-profdata show` で `tsuzuri;tz.integer.digits` の形を確認。IR ランタイムの `.ll` に `source_filename` はない | 変更なし。E2E で `show` の関数名を固定する |
| 6 | `build_key` は `options.cpu == Cpu::Native` のときだけ field を足す | 引数に `clang: Option<&ToolIdentity>` が増えた（`tool_identity("TSUZURI_CLANG", "clang", cpu)`。`native-cpu-features` はこの identity が持つ）。条件付き field の手本は `linker`（`TSUZURI_LINKER` が未設定・`default` 以外のときだけ）。`options` は今も `format!("{options:?}")`、`FORMAT` は `1`、呼び出しは `build_complete` の 1 か所 | `profile: Option<(&str, &[u8])>` を足し、`Some` のときだけ `hash.field`（手本は `linker`）。`TSUZURI_LLVM_PROFDATA`・`LLVM_PROFILE_FILE` は `KEY_ENVIRONMENT` に足さない（merge の結果は indexed profile の SHA-256 で鍵に入り、後者は実行時だけ） |
| 7 | 同梱 toolchain への言及なし | `tsuzuri-clang`（`scripts/toolchain/clang.rs`）は `-x ir` の入力で `-O`・`-W`・`-m`・`-g`・`-fPIC`・`-ffunction-sections`・`-fdata-sections` だけを LLVM clang に渡し、残りを `zig cc` のリンクに渡す。この機械の配布物（`vsc/toolchain`）で、`-fprofile-generate=<dir>` を注入した build は終了コード 0 で計装されない実行ファイル（`__llvm_prf` section 0、`.profraw` なし）を作り、`-fprofile-use=<有効な profile>` を注入した object は plain と byte 同一だった。配布物は `llvm-profdata` も `libclang_rt.profile` も持たない（manifest の 19,640 file に 0 件。Zig の `compiler_rt` は Zig 製） | 旗が黙って無効になる（AGENTS.md「明示した経路が使えなければエラー」に反する）。D11: 起動する Clang が launcher なら両方の旗を `E2002`。`clang.rs`・`bundle.mjs` は変えない |
| 8 | D3: `llvm-profdata` は `TSUZURI_LLVM_PROFDATA`、clang の実体のディレクトリ、macOS の `xcrun --find` の順 | `/usr/bin/clang` は xcode-select の shim で、隣に `llvm-profdata` がない。`clang -print-prog-name=llvm-profdata` は Apple clang で `/Library/Developer/CommandLineTools/usr/bin/llvm-profdata`、Homebrew で Cellar の実体を返す。`-print-resource-dir`（`…/usr/lib/clang/21`）の 3 階層上が toolchain の root。wrapper の clang（`tests/cache.mjs` の形）でも効く | D3 を変更 |
| 9 | profile の版: raw は書いた runtime と同じ系列の `llvm-profdata` で merge | raw version は 10（`INSTR_PROF_RAW_VERSION`。Apple clang 21 と Homebrew 21.1.8 の runtime で同じ）。Apple と Homebrew 21.1.8 の `llvm-profdata` が merge した indexed は byte 同一（SHA-256 `9d980f5b…`）。llvm@22 の merge は成功するが indexed の版が新しく、Apple clang 21 は `unsupported instrumentation profile format version`、llvm@23 は `raw profile version mismatch: Profile uses raw profile format version = 10; expected version = 11` で raw を拒否する。merge は入力の順序に依らない（3 つの `.profraw` を逆順に渡しても同一）が、`--sparse` を付けると出力が変わる | D10 の sort は残す。`--sparse` は使わない（実行されなかった関数が「冷たい」記録を失う）。上の 2 文面は E2002 の hint の実例 |
| 10 | 古い profile の警告 `… (hash mismatch) f Hash = <数> up to 0 count discarded [-Wbackend-plugin]` | Apple clang 21.0.0・Homebrew 21.1.8・22.1.8 で文面が同一（`warning: <module.ll>: function control flow change detected (hash mismatch) main Hash = … up to 166676 count discarded [-Wbackend-plugin]` と `1 warning generated.`）。Apple clang で作った profile を Homebrew clang で使うと、同じ IR でも 1 関数（`tz_closure_churn`）が mismatch した | D5 の文面と数え方は不変。profile は生成した clang と同じ clang で使う（D14） |
| 11 | `--profile-use <path>` の directory は Tsuzuri が `.profraw` を merge する | clang に directory を渡すと `<dir>/default.profdata` を読む（`Error in reading profile <dir>/default.profdata: No such file or directory`）。raw・存在しない path の失敗文面は起票時と同じ | `prepare_profile` は必ず file を clang に渡す。実装は変えない |
| 12 | 訓練中に trap する場合は未記載 | 計装版が trap すると `.profraw` は書かれない（runtime が起動時に空の dir を作る）。stderr・`.trap.json`・終了コードは通常版と同一（SIGTRAP） | 訓練の入力は trap しないこと（docs に書く） |
| 13 | `--emit` は exe・object 以外を拒否（`llvm`・`header`・`wgsl` を列挙） | `Emit` は 14 種（`shared`・`spirv`・`bindings-*` が増えた）。`--emit shared` の最終リンクは別の `link_shared`、`--emit object` はユーザーが最後に link する（`___llvm_profile_runtime` が未定義）。PR08 が `Emit::Bitcode` を足す | 拒否を whitelist（`Emit::Executable`・`Emit::Object` だけ許す）にして、新しい出力が黙って通らないようにする。検査は `src/profile.rs`（D13）に閉じ、PR08 は 1 行で許せる |
| 14 | 最適化は `-O0`〜`-O3`。`-Os`・`-Oz`・`--cpu native`・`--allocator small`・`--strip` との関係は未記載 | 計装版が `-O0`〜`-Oz`・`--cpu native`・`--allocator small`・`--strip`・`--trap-info` のすべてで `3699964` を出し `.profraw` を 1 つ書く。examples の 9 種（`tasks`・`io` の C ランタイム経路を含む）で stdout・終了コードが通常版と同一。`-O0` の `-fprofile-use` は何も変えない | E2000 にしない（D16） |
| 15 | WASM: 「compile は通る」。Phase 2 は host 側の仕組みが要る | wasm32 の計装 object は作れるが、`.wasm` の link は `wasm-ld: error: … undefined symbol: __llvm_profile_runtime` で失敗する。`int __llvm_profile_runtime = 0;` だけを定義した object を足すと link でき、`--export=__start___llvm_prf_{cnts,data,names}`・`__stop_…` を export でき、module の import は 0 のまま。Node で `tz_total(1000n)` を実行すると counters が `1000, 1, 333, 334, 0, …`（loop・entry・match の各 arm）と読めた | D7 を更新。Phase 1 は E2000 のまま |
| 16 | runner: `run-control.mjs` は `--emit object` の `control.o` と C・C++・Rust の参照を 1 つの host に link し、`--quick` は大きさ 1024・1 標本 | 変更なし。Tsuzuri は `--emit object` なので、IR ランタイムは全部 module に入り、numeric・math は使わない。PX01 の record の `variant` は `^[a-z0-9-]+$` の自由な文字列（`allocatorVariants` が `small` を使う） | PGO の行は schema を変えず `variant` で表せる（D8） |
| 17 | Linux CI の LLVM と compiler-rt は未確認 | macOS: `brew install llvm@21 lld`、Linux: apt の `clang-21 lld-21 llvm-21 libclang-rt-21-dev`、全 job で `TSUZURI_CLANG=$LLVM_PREFIX/bin/clang`。`libclang-rt-21-dev` は `libclang_rt.profile-x86_64.a`・`-i386.a` を含み（Debian の file list で確認。arm64 版は aarch64 用。Linux では実行していない）、`llvm-21` が `llvm-profdata` を持つ。Windows は `windows.yml`（LLVM 21.1.8）で、ここでは確認できない | `tests/pgo.mjs` は `targets.yml` の 4 host に足す（orchestrator）。Windows は E2000（D12）。ELF の `--gc-sections` と `__llvm_prf_*` の保持は未確認で、Linux CI が最初の確認になる |
| 18 | `driver` の `messages` と `main.rs` の W2001 | 同じ経路（`main.rs` の 4 か所）。`TSUZURI_TIME_PASSES=1` は `run_tool` が `tool.<名前>` を自動で計上するので、merge は `tool.llvm-profdata`、compile の増分は `tool.clang` に出る | 新しい phase 名は要らない |
| 19 | `tsuzuri toolchain info` | `tests/toolchain.rs:87` が行の位置で `TSUZURI_WASM_OPT` を検査する | `toolchain info` に `llvm-profdata` を足さない（D17。既存の期待値を変えない） |
| 20 | 文書は `_docs/tools/command-line.md`・`build-and-cache.md` | 旧 `_docs/tools/` は `_tsuzuri/language-reference/compiler/` の `option.md`（旗・受け付けるサブコマンドの表・組み合わせのエラー表・環境変数）・`usage.md`（cache・ワークフロー）・`diagnostics.md`（`W2001`）・`platforms.md`・`native-interop.md`（`--emit object`）。`check-docs.mjs` が例を検証する | WP7 で更新 |
| 21 | Tsuzuri の IR に `!prof` はない。`math.ll` にはある | `src/*.rs` に `!prof`・`branch_weights`・`llvm.expect` は 0 件。`src/runtime/math.ll` に `!prof` が 31 行 | 変更なし（D9 は下） |

### 偵察で測った効果（実コンパイラを変えず wrapper で旗を注入。2026-10-17）

- 動作（変更前のコンパイラが clang に渡す `module.ll`）: Apple clang 21.0.0 の計装版が `.profraw`（376 B、magic `81 72 66 6f 72 70 6c ff`、raw version 10）を書き、実行ファイルに `__llvm_prf_cnts`・`_data`・`_names` ほか 7 section ができる。
  `-fprofile-use` の `-S -emit-llvm` は `!prof` 13・`branch_weights` 9・`function_entry_count` 1・`ProfileSummary` 1（plain は 0）、fast-math 系の flag は両方 0。
  起票時の手順 1（C の probe。`branch_weights` 6）と、`--emit llvm` の IR を直接 link すると `_main` が未定義になる点も再現した。停止条件 1 には当たらない。
  同じ `--quick` で訓練し直した PGO の object は byte 同一（SHA-256 `80acb321…`）。
- 効果（control suite の `--emit object -O3 --cpu generic`。Apple clang 21.0.0 は 9 回、Homebrew 21.1.8 は 7 回、plain と PGO の host を交互に実行。訓練は `--quick`。host 内の C・C++ は同一コードの対照で、plain と PGO の比は 0.96〜1.02）:

| 種目 | plain（Apple、ms） | PGO / plain（Apple） | PGO / plain（Homebrew 21.1.8） |
| --- | ---: | ---: | ---: |
| `tail_mix` | 13.02 | 1.183 | 1.213 |
| `tail_if_mix` | 12.87 | 1.200 | 1.195 |
| `tail_builtin_mix` | 12.81 | 1.218 | 1.205 |
| `match_dispatch` | 2.594 | 1.609 | 1.642 |
| `closure_capture` | 1.944 | 0.822 | 0.836 |
| `closure_churn` | 6.484 | 0.873 | 0.879 |
| 他の 10 種目 | 1.7〜13.0 | 0.959〜1.002 | 0.991〜1.016 |

  訓練の大きさに依らない（Apple、5 回: `--scale 0.02` と `--scale 1` で `tail_mix` 1.211・1.215、`match_dispatch` 1.622・1.640、`closure_capture` 0.823・0.842）。
  つまり PGO は常に速くならず、control では遅くなる種目（4 種目、+18〜+64%）のほうが、速くなる種目（2 種目、−12〜−18%）より多い。D15 に従って、そのまま記録して主張に使う。
- 観測した要因（IR・asm・remark で確認）: `closure_*` は profile が間接呼び出しの対象を記録し（`llvm-profdata show --ic-targets` で `tz_closure_capture` の `tsuzuri;tz.apply.$lambda.28.1` 50.45%・`…29.1` 49.55%）、呼び出しが specialize される。
  `match_dispatch` は plain が `loop-unroll: PartialUnrolled（run-time trip count × 8）`、PGO は `Peeled` だけで scalar の loop が残る。PGO の IR では vectorizer の fallback の scalar loop の latch に `branch_weights 119, 119`（期待 2 回）が付き、count が大きいと実際には主な loop になるのに unroll されない。
  `tail_*` は PGO の build で inline の threshold が 75（plain は 250）になり、`tz_tail_mix` の loop の依存鎖が `madd`→`add` と 1 段長い（plain は `remaining + K` が鎖の外）。Apple clang は entry count の小さい関数に `cold minsize` も付ける（Homebrew の upstream clang は `cold` を 5 関数に付けるだけ）のに、結果は同じ +21%。
- 費用（control の module IR、n=9、共有機）: `clang -O3 -c` が plain 89.7 ms、`-fprofile-use` 109.0 ms（+19.3 ms）、`-fprofile-generate` 104.6 ms。`llvm-profdata merge`（`.profraw` 4,960 B → `.profdata` 5,928 B）が 33.6 ms（Apple）・42.1 ms（Homebrew）。`xcrun --find llvm-profdata` 11 ms、`clang -print-prog-name=llvm-profdata` 25 ms。
- D9 の証拠: control の plain の asm（`-O3`）で、16 関数のうち 13 は `brk`（trap の block）が関数の末尾にあり、3（`array_sum`・`array_index_sum`・`array_copy`）は無条件分岐の直後にあって fall-through で到達しない。hot path の間に置かれた trap の block はない。
- 既定の出力は WP1 の `cmp` で確かめる（まだ製品コードを変えていない）。

### 他チケットとの関係

- PR08（`Emit::Bitcode`・LTO）: 同じ `driver.rs`・`main.rs`。PR07 は exe・object の whitelist なので、bitcode と PGO の併用は既定で `E2000`。ThinLTO との相互作用（計装が pre-link で入り最終リンクに `-fprofile-generate` が要ること、`-fprofile-use` が bitcode の compile で `!prof` として注釈されること）は LLVM の設計からの見込みで、ここでは確かめていない。許すかは PR08 が確かめて決め、`src/profile.rs` の 1 行を足す。
- PB05（`--backend` の Cranelift）: 旗は LLVM backend 専用。PB05 が `E2000` の行を足す。PB06（`cli(raw, out, err)`・`watch`・`serve`）: `parse_arguments` の同じ場所を触る。PM02・PM03・PM04（closure・string・list の表現）: IR が変わると既存の profile は stale で `W2001` になる（統合後は profile を作り直す）。上の closure の −12〜−18% などの値は表現変更の前のもので、統合後に取り直す。

### 決定事項の更新

- **D1（変更）**: 旗は起票どおり（`--profile-generate <dir>`・`--profile-use <path>`、排他）。許す出力は native の `--emit exe`・`object` だけ（whitelist）。Phase 2（WP8）で `run` も両方の旗を受ける。
  診断の表の変更: 「`build` 以外」は Phase 1 が `--profile-generate and --profile-use are only valid with build`、WP8 の後は `… are only valid with build or run`。「wasm」は `profile-guided optimization is native-only; remove --target wasm32 or the profile option`（`--target wasm64` のときは `wasm64`。D7 まで）。
  「`--emit`」は `profile-guided optimization needs --emit exe or object; remove the profile option for other output kinds`。新規の `E2000`:
  Windows の host・target は `--profile-generate and --profile-use are not supported on Windows yet; build on macOS or Linux`、別の machine の `--target TRIPLE` は `--profile-generate and --profile-use apply to the machine that builds; remove --target <TRIPLE> for another machine`、
  `--freestanding` と generate は `--profile-generate cannot be combined with --freestanding: the profile runtime needs the C library`、`--profile-generate ""`（空）は `--profile-generate needs a value`。他の行は起票どおり。
- **D2（維持）**: 計装は clang の IR PGO（`-x ir` に `-fprofile-generate=<dir>`・`-fprofile-use=<file>`）。
- **D3（変更）**: `--profile-use` は起票どおり（indexed の file か `.profraw` の directory）。`llvm-profdata` は ①`TSUZURI_LLVM_PROFDATA` ②clang の実体（`cache::executable_path`）のディレクトリ ③`clang -print-resource-dir` の 3 階層上の `bin/`（`xcrun` の代わり。macOS の分岐が要らず、wrapper の clang でも効く）の順で探し、PATH は見ない。版番号は比べない。見つからなければ `E2002`。merge に `--sparse` は付けない。
- **D4（形を変更）**: `Profile` は `BuildOptions` の外。入口は `build_profiled(module, project, output, options, links, &Profile)`。`build`・`build_linked` は不変、`BuildOptions` は `Copy` のまま。
- **D5（維持）**: 文面と数え方は起票どおり（3 つの clang で同一を確認）。
- **D6（変更）**: Phase 1 は module IR だけ。既定（cache あり）の経路の `numeric`・`math` の object と C ランタイムの object は旗を受けない（docs に書く）。Phase 2（WP10）で判断する: (a) control・apps の profile に `numeric`・`math`・C ランタイムの関数が現れるか、(b) PGO の build だけ `runtime_objects` を使わず numeric・math を module に埋め込む案（cache 設定に依らない profile になり、両者も対象になる。-O3 で約 +0.4 s の compile で、whole-build cache は効く）の費用と効果、(c) C ランタイムは `static` 関数の PGO 名が compile した file 名を含み、cache でない経路は一時 path になるので、広げるなら cache 経由の相対名に限る。効果が測れなければ取り下げて記録する。
- **D7（変更）**: Phase 1 は E2000（D1）。Phase 2 は技術的に可能（`wasm-ld` が `__start/__stop___llvm_prf_*` を export でき、import は増えず、Node で counters を読めた）。要るのは raw profile（version 10。wasm32 は 32 bit の magic と pointer）を組み立てる writer で、compiler-rt は同梱せず LLVM 21 の `InstrProfData.inc` の形に従って自作する（module に足すのは export だけ）。
  gate は WP12 の試作: (1) Node で control の instrumented wasm から profile を書き出し、`llvm-profdata merge`・`show` で関数別の counts が native と整合する、(2) `-fprofile-use` の wasm が plain と同じ結果を返し、時間に測れる差が出る。差が測れなければ試作の証拠を記録して E2000 のまま。出すなら opt-in の `--profile-generate`・`--profile-use`（wasm32）。
- **D8（変更）**: `--pgo` は起票どおり、訓練は `--quick`（Apple の 3 通りの大きさで同じ結果）。PGO あり・なしは 1 つの run に `variant` の `default`・`pgo` で残す（plain の host と PGO の host を交互に実行し、PGO の object を使った行を `pgo` にする。`--pgo` を付けない出力と欄は変えない）。
  Phase 2（WP9）: C・C++ の参照も同じ clang で `-fprofile-generate` → `--quick` → `-fprofile-use` にする。Rust は rustc 1.98.1 が LLVM 22.1.8 で（`llvm@22` の `llvm-profdata` が対）、clang 21 の runtime と組めるかを試し、組めなければ「rust（PGO なし）」と表に明記する。
- **D9（変更）**: Phase 1 は IR を変えない。上の証拠で trap の block が hot path に混ざる例はない。WP11 で corpus（control・apps・computations・examples）を同じ基準（`brk` が fall-through で到達するか）で機械走査し、反例がなければ取り下げ、あれば最小の重みを検討する。
- **D10（維持）**: 入力順の sort と複製を残す（順序に依らないことは実測したが、版が変わるときの保険）。
- **D11（新設）**: 起動する Clang が launcher（`bundled_launcher`。file stem が `tsuzuri-clang`）なら、両方の旗を `E2002` にする: `profile options need an LLVM Clang: the bundled Clang launcher (tsuzuri-clang) passes no profile options to LLVM Clang and has no profile runtime or llvm-profdata; set TSUZURI_CLANG to an LLVM 21 Clang`。`build_profiled` の先頭（cache の参照より前）で検査する。
- **D12（新設）**: Windows と別の machine の target は `E2000`（D1）。検証できない経路を黙って通さない（G10 と同じ扱い）。Windows CI で確かめられる段階で再検討する。
- **D13（新設）**: 実装は新しい `src/profile.rs`（`Profile`・`PreparedProfile`・`prepare_profile`・`find_profdata`・`summarize_profile_warnings`・magic・`Profile::validate`）に置き、`driver.rs`・`main.rs` は呼び出しだけにする（共有の hot file で他の 6 チケットと衝突しやすい）。`driver.rs` の `run_tool`・`collect_message`・`driver_error`・`io_error`・`tool` は `pub(crate)` にする。
- **D14（新設）**: profile は IR・clang・option に結び付く。generate と use は、同じコンパイラの binary・ソース・option（cache 設定・`--trap-info`・`-g`・`--cpu`・`--allocator`・`-O` の段・`@cpu` の多版を決める機械の CPU）と同じ clang で行う。違えば hash mismatch の関数が出て `W2001`（build は成功し、その関数は profile なしで最適化される）。`run` の profile は `--trap-info` の IR で、`build` の profile と完全には一致しない（`examples/functional` で 2 関数が mismatch）。
- **D15（新設）**: PGO は opt-in のまま（profile が要るので既定にならない）。docs と `docs/benchmarks.md` には、速くなる例（`closure_*`）と遅くなる例（`tail_*`・`match_dispatch`）の両方を記録し、「PGO を使えば速くなる」とは書かない。使う人は自分の program で before/after を測る。
- **D16（新設）**: `-O0`・`-Os`・`-Oz`・`--cpu native`・`--allocator small`・`--strip`・`--trap-info`・`-g` と併用できる（上の確認）。`-O0` の `--profile-use` は何も変えない。
- **D17（新設）**: `tsuzuri toolchain info` は変えない（`tests/toolchain.rs`）。

### 作業パッケージ（コミット）

| WP | 内容 | 確認 |
| --- | --- | --- |
| WP0 | この節（ticket だけ。`_perfs/README.md` は統合担当が更新する） | `git diff --check` |
| WP1 | `src/profile.rs`（`Profile`・magic・`PreparedProfile` の骨格）、`build_profiled`・`build_complete`・`build_key` の `profile` 引数（`None` だけ。挙動を変えない） | 単体テスト、変更前のコンパイラと `examples/*`・`tests/fixtures/*` の IR・object・実行ファイルを `cmp`（`artifactFingerprint`） |
| WP2 | CLI（`parse_arguments`・`Arguments`・`HELP`・`Profile::validate`・launcher の `E2002`）と単体テスト | `cargo test --bin tsuzuri profile`、拒否の E2E |
| WP3 | `--profile-generate`（本体の `Command`・`-g` の別リンク・hint） | 手順 4 の確認、`-O0`〜`-Oz` の行列 |
| WP4 | `--profile-use`（directory の merge・file の複製・SHA-256・`find_profdata`・`W2001` の要約） | 手順 5・6 の確認 |
| WP5 | `tests/pgo.mjs`（新規。README・`docs/architecture.md` の検証一覧に足す） | `node tests/pgo.mjs target/release/tsuzuri` |
| WP6 | `run-control.mjs --pgo`（Tsuzuri）、生成コードの確認、計測（`docs/benchmarks.md` の `phase4:PR07`） | `--quick --pgo`、`tests/benchmark_report.mjs` |
| WP7 | ドキュメント（言語リファレンス・`docs/*`・README） | `check-docs.mjs` |
| WP8 | Phase 2: `run --profile-generate`・`--profile-use` | E2E |
| WP9 | Phase 2: C・C++（と可能なら Rust）の参照の PGO、同条件の比較の測定 | runner |
| WP10 | Phase 2: D6 の判断と実装または取り下げ | 測定 |
| WP11 | Phase 2: D9 の corpus 走査 | asm の機械走査 |
| WP12 | Phase 2: D7 の WASM の試作、実装または取り下げ | Node |
| WP13 | 全体の確認、「実装と検証」、`_completed/` への移動 | 全 suite |

### 受け入れ条件の変更

- 追加: launcher で両方の旗が `E2002`（旗が黙って無効にならない）。`find_profdata` の 3 段（env・clang の隣・resource dir）と見つからないときの `E2002`。`-O0`・`-Os`・`-Oz`・`--cpu native`・`--allocator small`・`--strip`・`--trap-info`・`-g` との併用。`--emit`・Windows・別の machine・`--freestanding`・wasm の拒否が D1 の表のとおり。Phase 2 の `run`。
- 変更: 「M1〜M3 を記録し、判定できない metric はそう書く」に、悪化した種目を省かないことを足す。G1「PGO ありが短い種目がある」は目標のまま、期待ではなく記録として扱う（偵察では 2 種目が −12〜−18%、4 種目が +18〜+64%）。「PGO なしの成果物が byte 一致」は `artifactFingerprint`（Mach-O の `LC_UUID`・署名を除く）で比べ、`build_key` の条件は「`None` のとき `profile` の field を足さない」。
- 取り下げ: なし。

### 停止条件の更新

- 変更: 5 は `build`・`build_linked` の signature と `BuildOptions` の `Copy`（`build_profiled` で収める）。6 は native の profile runtime が見つからない場合で、`E2002` と hint にして runtime を同梱しない（D7 の writer は runtime の同梱ではなく、wasm module の export と host 側の書き出し）。
- 追加: PGO の旗が無効なまま build が成功する toolchain（launcher の類）。`tests/toolchain.rs` を含む既存テストの期待値を変える必要。同じ入力・同じ profile の build が byte 一致しない。
- 取り下げ: なし。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の状態欄で done であること（runner の `--metrics` と `node benchmarks/metrics.mjs report` を計測手順で使う）。
  確認: `grep -n "PX01\|PB01\|PR07" _perfs/README.md`。
- PB01 は開始条件にしない。PB01 の状態で変わるのは、数値ランタイムが計装・PGO の対象に入るかどうかだけである（D6）。
- `_perfs/README.md` の PR07 の状態を `doing` にしてから始める。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 の probe とベースラインを保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めてコマンド・出力・該当箇所を添えて報告する（GUIDE §13）。

1. 手順 1 の probe で、`clang -x ir -fprofile-generate=<dir>` で作った実行ファイルが `.profraw` を書かない、または `-fprofile-use` の
   `-S -emit-llvm` 出力に `!prof` が 0 件。`opt -passes=pgo-instr-gen` や `-fprofile-instr-generate` へ切り替えない（D2）。
2. PGO の旗を付けない build で、成果物（IR・object・実行ファイル）が変更前のコンパイラの物と 1 byte でも変わる（手順 2・8 の `cmp`）、
   または `build_key` が `Profile::None` のときに field を足している。
3. 計装版・PGO 版の E2E で、stdout・終了コード・trap の報告のどれかが通常版と異なる。旗や最適化段を外して通さない。
4. `llvm-profdata merge` の出力が、同じ入力で byte 単位で一致しない（D10）。
5. `BuildOptions` から `Copy` を外す必要が生じた、または `driver::build` の signature を変える必要が生じた（`tests/host_abi.rs` と
   `tests/trap_locations.rs` が呼んでいる。D4 の `build_profiled` で収める）。
6. 計装版の link で profile runtime（macOS では `libclang_rt.profile_osx.a`）が見つからない。runtime を同梱・自作しない。
7. crate の追加（glob 用など）、`unsafe`、既定の WASM import が必要になった。
8. 既存テストの期待値を変える必要が生じた。`src/main.rs` の `mod tests` の拒否一覧へ行を足すことと、`--help` の文面の追加は除く。

## 現状と計測（HEAD `f8dc655`）

### CLI とビルドの経路

- CLI の最適化指定は `-O0`〜`-O3` だけで、PGO の手段はない。`src/**` に `fprofile`・`profdata` は 0 件。
- `src/main.rs` の `parse_arguments` は旗を 1 つずつ読み、重複（`"CPU tuning specified more than once"` の形）と組み合わせ
  （`"--wasm-feature is only valid with build"` の形）を `Err(String)` で返す。`main` はこれを `Diagnostic::new("E2000", message, Span::default())`
  にして終了コード 2 で終える。最後に `BuildOptions` を組み立てて `options.validate()`（`BuildOptions::validate`）を呼ぶ。
- `src/driver.rs` の `BuildOptions` は `#[derive(Clone, Copy, Debug)]` で、`target`・`emit`・`optimization`・`cpu`・`debug_output`・
  `trap_info`・`debug_info`・`wasm_simd`・`wasm_threads`・`cache` を持つ。path を持つ field はない。
- `driver::build(module, project, output, options)` は `build_complete(module, project, output, options, "build")` を呼ぶだけである。
  `build` は `src/main.rs` の `run_action` のほか `tests/host_abi.rs`（1 か所）と `tests/trap_locations.rs`（4 か所）が呼ぶ。
  `build_complete` のもう一つの呼び出し元は `run` 系（`run_with_diagnostics`）である。
- `build_complete` は IR を一時ディレクトリの `module.ll` に書き、次の順に clang を起動する。
  - C ランタイム（`task_runtime_source()`・`runtime/cpu.c`・`runtime/io.c` を連結した `task.c`）: `runtime` の `Command` で
    `-std=c11 -c -O<n>`（`native_runtime` のときだけ）。
  - 本体: `clang` の `Command` で `-x ir -Wno-override-module -O<n>`、`-g`（debug）、wasm32 の旗または `native_compile_args` と
    `native_cpu_flag`、`options.emit != Emit::Executable || dwarf_sidecar.is_some()` なら `-c`。実行ファイルを直接作るときは同じ
    コマンドで `-lm` と `-x none task.o` を渡して link まで行う。
  - `dwarf_sidecar` があるとき（macOS の `-g` 付き実行ファイル）: 別の `linker` の `Command`（`-g -lm`）で link し、`dsymutil` を起動する。
  - native の `--emit object` で C ランタイムがあるとき: `-r -nostdlib`（macOS は `-Wl,-keep_private_externs`）の再配置可能 link。
- `run_tool` は成功時に stdout と stderr を連結した文字列を返し、`collect_message` が空でなければ `messages` へ積む。`main` は
  `run_action` の `Ok(messages)` を 1 件ずつ stderr へ出し、`--json` では `{"severity":"warning","code":"W2001","message":...}` にする
  （`run_test_action` も同じ）。docs/language.md の「ツールの警告は `W2001` の JSON オブジェクトです。」がこの経路である。
- 生成 IR の分岐の重み: Tsuzuri 自身が出す IR（`src/llvm*.rs`）に `!prof` はない。C から生成した `src/runtime/math.ll` には
  `!prof`（`__builtin_expect` 由来）がある。trap は `call void @llvm.trap()` の後に `unreachable` を置く形で、LLVM はこの block を冷たいと推定する。

### cache

- `src/cache.rs` の `build_key(project, options, action, ir, output)` は `format`・`compiler-version`・`compiler-binary`・`host-os`・`host-arch`・
  `options`（`format!("{options:?}")`）・`action`・`ir-and-embedded-runtime`・全ソース・環境変数の一覧（`TSUZURI_CLANG`・`TSUZURI_LLVM_LINK`・
  `PATH` など）・使うツールごとの `tool-path`・`tool-binary`・`tool-version-stdout`・`tool-version-stderr` を hash する。`--cpu native` の
  ときだけ `native-cpu-features` を足す。呼び出しは `build_complete` の中の 1 か所だけである。
- cache の hit では、保存した `messages`（metadata.json の `"messages"`）を再び出す。したがって build 時の警告は hit でも同じ文面で出る。
- `FORMAT` は `1`。field を条件付きで足すだけなら、PGO を使わない build の鍵は変わらない。

### ツールチェーン（計測機: M1 Max、macOS 27.0）

- `clang` は `/usr/bin/clang`（`Apple clang version 21.0.0 (clang-2100.3.34.2)`）。`/usr/bin/llvm-profdata` は存在しない。
  `xcrun --find llvm-profdata` は `/Library/Developer/CommandLineTools/usr/bin/llvm-profdata`。Homebrew の
  `/opt/homebrew/opt/llvm@21/bin/llvm-profdata` は `Homebrew LLVM version 21.1.8`。
- profile の形式は LLVM の版に結び付く。raw profile（`.profraw`）は書いた runtime と同じ系列の `llvm-profdata` で merge し、indexed profile
  （`.profdata`）は読む clang より新しい版で作らない。Apple clang の版番号は LLVM の版番号と対応しないので、版番号の比較では判定できない。
- 先頭 8 bytes: indexed profile は `ff 6c 70 72 6f 66 69 81`（`\xfflprofi\x81`）、raw profile は `81 72 66 6f 72 70 6c ff`（2026-09-30 に `xxd` で確認）。

### 再現（2026-09-30 に確認）

`a.ll` は `@f`（ループと `urem` の分岐）と、`atoi` した引数で `@f` を呼び `printf` で出す `@main` だけの IR。`b.ll` は `@f` の先頭に分岐を 1 つ足したもの。
どちらも `/tmp/tz-work-PR07/` にある（`probe.sh` が下の全項目を実行する）。

```sh
cd /tmp/tz-work-PR07 && mkdir -p out
clang -x ir -Wno-override-module -O2 -fprofile-generate=$PWD/raw a.ll -o out/a-gen && ./out/a-gen 50 && ls raw
xcrun llvm-profdata merge -o out/a.profdata raw/*.profraw
clang -x ir -Wno-override-module -O2 -fprofile-use=$PWD/out/a.profdata a.ll -S -emit-llvm -o out/a-use.ll && grep -c '!prof' out/a-use.ll
clang -x ir -Wno-override-module -O2 -fprofile-use=$PWD/out/a.profdata b.ll -c -o out/b.o
clang -x ir -Wno-override-module -O2 -fprofile-instr-generate a.ll -o out/a-instr && LLVM_PROFILE_FILE=$PWD/out/instr.profraw ./out/a-instr 5
clang -x ir -Wno-override-module -O2 -fprofile-generate=$PWD/raw2 -c a.ll -o out/a.o && clang out/a.o -o out/a-nolink
```

| 確認 | 結果 |
| --- | --- |
| `-x ir` に `-fprofile-generate=<dir>` | 成功。実行で `<dir>/default_<数字>_0.profraw` ができる。`<dir>` は事前に消しておいても実行時に作られる |
| merge | `xcrun llvm-profdata` と llvm@21 の `llvm-profdata` のどちらも成功。同じ入力を 2 回 merge した出力は byte 一致 |
| `-fprofile-use=<file>` | 成功。`-S -emit-llvm` の出力に `!prof` が 12 件。同じ入力から作った object 2 つは byte 一致 |
| llvm@21 で merge した profile を Apple clang で使う | 成功（この組み合わせでは。一般の保証ではない） |
| 古い profile（`b.ll`） | 終了コード 0。stderr に関数ごとに 1 行 `warning: b.ll: function control flow change detected (hash mismatch) f Hash = <数> up to 0 count discarded [-Wbackend-plugin]`（変えていない `main` も `@f` を inline した結果として 1 行）と、最後に `2 warnings generated.` |
| 存在しない profile | 終了コード 1。`clang: error: Error in reading profile <path>: No such file or directory` |
| `.profraw` をそのまま `-fprofile-use` に渡す | 終了コード 1。`invalid instrumentation profile data (bad magic)` |
| `-x ir` に `-fprofile-instr-generate`（frontend の計装） | 成功するが計装されない。`.profraw` は 128 bytes でカウンターがない（G18 の「再現」と同じ結論） |
| `LLVM_PROFILE_FILE=$PWD/out/custom-%p.profraw` | 埋め込んだ `<dir>` より優先され、`custom-<pid>.profraw` ができる |
| `-c` で作った計装 object を旗なしで link | 失敗: `Undefined symbols ... "___llvm_profile_runtime"`。link にも `-fprofile-generate` を渡すと成功 |
| wasm32 で `-fprofile-generate -c`／`-fprofile-use -c` | どちらも終了コード 0（compile は通るが、WASM には `.profraw` を書き出す runtime がない） |

`target/release/tsuzuri build examples/hello --emit llvm -O3` の IR を clang で直接実行ファイルにすると `_main` が未定義で link に失敗した
（2026-09-30。`--emit llvm` は `Entry::Library` で出す）。PGO の確認は `tsuzuri build` の成果物で行い、`--emit llvm` の IR を手で link しない。

### 既存の計測

PGO に関する計測はない。基準は PX01 形式の control suite（`target/perf/PR07-before/`、手順 10）である。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する（GUIDE §14、PX01 の「before／after」）。

- G1: `build --profile-use` で、分岐の多い control の種目の時間を PGO なしより短くする（期待。計測で確かめ、長くなった種目も報告する）。
- G2: PGO の旗を付けない build の成果物を 1 byte も変えず、cache の鍵にも field を足さない。
- G3: 計装版・PGO 版の stdout・終了コード・trap の報告を通常版と一致させる。
- G4: `--profile-use` によるビルド時間の増分（merge と profile の読み込み）を記録する（目標値は置かない）。

起票時の「同じ PGO を使う C/C++ と同等以上」は、C/C++ の参照実装にも PGO を掛ける runner の変更が要るので Phase 2 とする（D8）。

| 指標 | 単位・統計 | 対象 | 期待 |
| --- | --- | --- | --- |
| M1 時間 | ms（PX01 の in-process 標本の中央値・最小・最大） | control suite の Tsuzuri の行（native、`-O3`、`--cpu generic`）。主に `match_dispatch`・`while_mix`・`for_mix`・`tail_mix`・`record_pipeline` | PGO ありが PGO なしより短い種目がある。差が `before` と `before2` の揺れ以内なら「判定できない」と書く |
| M2 object の大きさ | bytes（固定値） | `target/release/tsuzuri build benchmarks/control --emit object -O3` の成果物。PGO なし、`--profile-use` あり | 記録だけ |
| M3 ビルド時間 | ms（9 回の中央値・最小・最大、`--no-cache`） | M2 と同じ build。PGO なし、`--profile-use <dir>`、`--profile-use <file>` | 記録だけ |
| M4 意味の一致 | 一致・不一致 | `tests/pgo.mjs` の全ケース、control の `--quick` の checks（runner が参照と比べる） | すべて一致 |

## 変えてはいけない意味

- PGO は最適化の判断（分岐と block の配置、inline、関数の配置、ループ変形の選択）だけを変える。演算結果、overflow、rounding、NaN の
  payload、符号付きゼロ、評価順序、trap の有無と位置、所有権・借用は変えない。LLVM の PGO は fast-math や reassociation を有効にしない。
  Tsuzuri は PGO のために `-ffast-math` などの旗を足さない。
- 計装版は通常版と同じ stdout・終了コード・trap を返す。増えるのは `.profraw` のファイルだけで、stdout・stderr には何も足さない。
- 古い・一致しない profile でも build は成功し、正しい結果の成果物を作る（性能だけが変わる）。警告は `W2001` の経路で 1 行だけ出す（D5）。
- PGO の旗を付けない build: IR・header・object・実行ファイル・WASM が byte 単位で不変。`build_key` は `Profile::None` のとき
  field を足さず、`BuildOptions` の `Debug` 表現も変えない（`Profile` は `BuildOptions` の外に置く、D4）。
- 関数の PGO 名: `src/llvm.rs` のヘッダーの `source_filename = "tsuzuri"` を変えない（落とし穴の 1 項目め）。
- 決定性: 同じ IR・同じ indexed profile の bytes・同じツールなら、成果物は byte 一致する（Apple clang について 2026-09-30 に確認）。
  directory を merge するときはファイル名の順で渡す（D10）。
- `--profile-generate` の成果物は `<dir>` の絶対 path を埋め込む。この path は cache の鍵に入る。
- trap の報告: `--trap-info` の sidecar は `build_complete` が IR の生成時に得た `trap_sites` から `crate::trap::side_table` で作るので、PGO の
  有無で変わらない。実行時の trap の報告（stderr と終了コード）も変わらないことを E2E で確かめる。
- WASM: Phase 1 は `--target wasm32` との組み合わせを `E2000` で拒否する。既定の WASM import を増やさない。
- 公開 ABI: `tz_*` の記号と signature を変えない。`--profile-generate` の object は未定義記号 `___llvm_profile_runtime`（Mach-O の表記）を持ち、
  利用者は最終の link に `-fprofile-generate` を渡す（D1。docs に書く）。

## 設計

### CLI と診断

新 API（実装後に有効。未検証）: 次の 2 つの旗を `build` だけに足す。

```text
tsuzuri build <input> --profile-generate <dir> [-O0..-O3] [--cpu generic|native] [--emit exe|object] [-o <path>]
tsuzuri build <input> --profile-use <path>     [-O0..-O3] [--cpu generic|native] [--emit exe|object] [-o <path>]
```

- `--profile-generate <dir>`: 計装版を作る。`<dir>` は `std::path::absolute` で絶対 path にし、存在しなくてよい（実行時に profile runtime が作る）。
- `--profile-use <path>`: `<path>` が directory なら中の `*.profraw` を merge して使い、file なら indexed profile（`.profdata`）としてそのまま使う（D3）。
- 訓練の実行は利用者が行う。計装版の実行ファイルを普段どおり実行すると `<dir>/default_<数字>_0.profraw` ができる。`LLVM_PROFILE_FILE` を
  設定するとそちらが優先される（`%p`・`%m` などの展開は LLVM の規則）。
- `-O0` の `--profile-use` は受け付ける（clang が最適化しないので効果はない）。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E2000` | 同じ旗を 2 回 | `profile generation specified more than once` ／ `profile use specified more than once` | `<command line>` |
| `E2000` | 値がない | `--profile-generate needs a value` ／ `--profile-use needs a value`（既存の `next_value`） | 同上 |
| `E2000` | 両方を指定 | `--profile-generate and --profile-use cannot be combined; build once with each` | 同上 |
| `E2000` | `build` 以外 | `--profile-generate and --profile-use are only valid with build` | 同上 |
| `E2000` | `--target wasm32` | `profile-guided optimization is native-only; remove --target wasm32 or the profile option` | 同上 |
| `E2000` | `--emit llvm`・`header`・`wgsl` | `profile-guided optimization needs --emit exe or object; remove the profile option for llvm, header, or wgsl output` | 同上 |
| `E2000` | `--profile-generate` の絶対 path が `%` を含む | `--profile-generate directory must not contain '%'; choose another directory` | 同上 |
| `E2000` | `--profile-use` の file が raw profile | `'<path>' is a raw profile; pass its directory to --profile-use or merge it with llvm-profdata` | 空の span（`driver_error`） |
| `E2000` | file が indexed でも raw でもない | `--profile-use expects an indexed .profdata file or a directory of .profraw files; '<path>' is neither` | 同上 |
| `E2000` | directory に `.profraw` がない | `--profile-use directory '<path>' has no .profraw files; run the program built with --profile-generate first` | 同上 |
| `E2001` | profile の読み込み・複製の失敗 | 既存の `io_error("read profile", path, error)` の形 | 同上 |
| `E2002` | `llvm-profdata` が見つからない | `cannot find llvm-profdata; set TSUZURI_LLVM_PROFDATA to the llvm-profdata that belongs to the Clang in TSUZURI_CLANG` | 同上 |
| `E2002` | merge の失敗 | `run_tool` の既存の形。hint は `llvm-profdata must come from the same LLVM as the Clang that built the instrumented program; set TSUZURI_LLVM_PROFDATA` | 同上 |
| `E2002` | clang が profile を読めない（版が新しい、壊れている） | `run_tool` の既存の形。`--profile-use` のときだけ hint を `the profile must be merged by the llvm-profdata that matches this Clang; regenerate it with --profile-generate` にする | 同上 |
| `W2001` | clang が hash mismatch を報告した | `profile data does not match <N> function(s) in this build; they are optimized without profile data. Rebuild with --profile-generate and rerun the training workload` | stderr（`--json` では既存の W2001 の JSON） |

CLI の組み合わせ（表の前半 7 行）は `parse_arguments` で、`target`・`emit` を決めた後に検査する。file と directory の検査（後半）は driver で行う。

### データ構造

```rust
// src/driver.rs
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Profile {                     // （新規）
    #[default]
    None,
    Generate(PathBuf),                 // 絶対 path（parse_arguments で std::path::absolute 済み）
    Use(PathBuf),                      // 利用者が渡した path（file または directory）
}

struct PreparedProfile {               // （新規）build_complete の中だけで使う
    flag: OsString,                    // "-fprofile-generate=<dir>" または "-fprofile-use=<一時ディレクトリ>/profile.profdata"
    key: (&'static str, Vec<u8>),      // ("profile-generate-dir", path の bytes) または ("profile-use-sha256", digest)
    generate: bool,
}

pub fn build_profiled(module: &CheckedModule, project: &Project, output: &Path,
    options: BuildOptions, profile: &Profile) -> Result<Vec<String>, Diagnostic>;   // （新規）
// build は build_profiled(module, project, output, options, &Profile::None) を呼ぶだけにする（signature は不変）
```

- `src/main.rs` の `Arguments` に `profile: Profile` を足す（`lsp` の早期 return の構築では `Profile::None`）。
- `build_complete` に引数 `profile: &Profile` を足す。`run_with_diagnostics` からの呼び出しは `&Profile::None`。
- `src/cache.rs` の `build_key` に引数 `profile: Option<(&str, &[u8])>` を足し、`Some((tag, bytes))` のときだけ `hash.field(tag, bytes)` を
  `ir-and-embedded-runtime` の直後で呼ぶ。`None` の鍵は現在と同じになる。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | `HELP` | Build options に 2 行、Toolchain に `TSUZURI_LLVM_PROFDATA` の 1 行 |
| CLI | `src/main.rs` | `parse_arguments` | `Some("--profile-generate")`・`Some("--profile-use")` の arm（`next_value`、重複の検査）。末尾で組み合わせを検査し、`Profile` を作る |
| CLI | `src/main.rs` | `Arguments`、`run_action` | `profile` field。`Action::Build` の `driver::build(...)` を `driver::build_profiled(..., &arguments.profile)` に替える |
| CLI | `src/main.rs` | `mod tests` | 受理・拒否の parse テスト（テスト計画） |
| 駆動 | `src/driver.rs` | `Profile`、`PreparedProfile`、`build_profiled` | 新規（上の形） |
| 駆動 | `src/driver.rs` | `build`、`build_complete`、`run_with_diagnostics` | `profile` 引数の追加と受け渡しだけ |
| 駆動 | `src/driver.rs` | `prepare_profile`（新規） | `messages` を作った直後、cache の準備より前に呼ぶ。アルゴリズムの「profile の準備」 |
| 駆動 | `src/driver.rs` | `find_profdata`（新規） | アルゴリズムの「`llvm-profdata` の発見」 |
| 駆動 | `src/driver.rs` | `build_complete` の本体の `clang` | `-O<n>` の直後に `prepared.flag` を渡す。`--profile-use` のとき、`run_tool` の結果を `summarize_profile_warnings` に通してから `collect_message` |
| 駆動 | `src/driver.rs` | `build_complete` の `dwarf_sidecar` の `linker` | `prepared.generate` のとき `-fprofile-generate` を足す（無いと `___llvm_profile_runtime` が未定義） |
| 駆動 | `src/driver.rs` | C ランタイムの `runtime`、`-r -nostdlib` の `linker`、WASM threads の経路 | 変更なし（D6。wasm32 は CLI で拒否済み） |
| 駆動 | `src/driver.rs` | `summarize_profile_warnings`（新規） | アルゴリズムの「古い profile の警告」 |
| cache | `src/cache.rs` | `build_key` | `profile` 引数（上の形） |
| cache | `src/cache.rs` | `executable_path` | `pub(crate)` にする（`find_profdata` が clang の実体の path を得る） |
| 試験 | `tests/pgo.mjs`（新規）、`tests/fixtures/pgo/branch/Main.tz`・`tests/fixtures/pgo/trap/Main.tz`（新規） | — | テスト計画の E2E |
| 計測 | `benchmarks/run-control.mjs` | 引数の解析、Tsuzuri の object を作る箇所 | `--pgo`（新規、D8） |

### 生成 IR とランタイム

- Tsuzuri が出す IR は変えない（`!prof` を足さない。trap の分岐への重みは Phase 2 の D9）。
- 計装は clang の IR レベルの PGO が行う。計装 object には `__llvm_prf_cnts`・`__llvm_prf_data`・`__llvm_prf_names` の section ができる
  （Mach-O。2026-09-30 に `llvm-objdump -h` で確認）。profile runtime は clang の driver が `-fprofile-generate` の link で足す。
- 最適化後の IR には `!{!"branch_weights", ...}`・`!{!"function_entry_count", i64 <n>}`・`ProfileSummary` が付く（probe で 9・2・1 件）。
- 同じ module に連結される IR ランタイム（`numeric.ll`・`string.ll` などの `src/runtime/*.ll`）は計装・PGO の対象に入る。C ランタイム
  （`task.c` にまとめる `task`・`cpu`・`io`）と PB01 の事前ビルド object は対象外（D6）。

### アルゴリズム

profile の準備（`prepare_profile(profile, temporary: &Path, messages: &mut Vec<String>) -> Result<Option<PreparedProfile>, Diagnostic>`）:

```text
Profile::None        -> None
Profile::Generate(d) -> flag = "-fprofile-generate=" + d, key = ("profile-generate-dir", d の encoded bytes), generate = true
Profile::Use(p):
  p が directory:
    files = read_dir(p) のうち拡張子が "profraw" の通常ファイル。file_name で sort
    files が空 -> E2000（directory に .profraw がない）
    tool = find_profdata()?
    run_tool(tool merge -o <temporary>/profile.profdata files..., hint)? を collect_message
  p が file:
    先頭 8 bytes が raw の magic -> E2000（raw profile）
    indexed の magic でない -> E2000（どちらでもない）
    fs::copy(p, <temporary>/profile.profdata)（読み込み中の差し替えと鍵のずれを防ぐ）
  bytes = <temporary>/profile.profdata の先頭 8 bytes。indexed の magic でなければ E2000（どちらでもない）
  flag = "-fprofile-use=" + <temporary>/profile.profdata
  key = ("profile-use-sha256", crate::cache::file_digest(<temporary>/profile.profdata)?)
  generate = false
```

magic は `const INDEXED_PROFILE_MAGIC: [u8; 8] = *b"\xfflprofi\x81";`（新規）と `const RAW_PROFILE_MAGIC: [u8; 8] = *b"\x81rforpl\xff";`（新規）。
flag は `OsString` で組み、path を `to_str` しない（非 UTF-8 の path を壊さない）。

`llvm-profdata` の発見（`find_profdata() -> Result<OsString, Diagnostic>`）:

```text
1. TSUZURI_LLVM_PROFDATA があればそれを返す（存在しなければ merge の run_tool が E2002 を出す）
2. crate::cache::executable_path(tool("TSUZURI_CLANG", "clang")) の親ディレクトリの llvm-profdata（Windows は .exe）が file ならそれ
3. macOS なら xcrun --find llvm-profdata を実行し、成功して stdout（trim）が file ならそれ
4. E2002（cannot find llvm-profdata ...）
PATH 上の llvm-profdata は使わない（D3）
```

古い profile の警告（`summarize_profile_warnings(text: String) -> String`）:

```text
n = text の行のうち "(hash mismatch)" を含む行の数
n == 0 なら text をそのまま返す
rest = text の行から、"(hash mismatch)" を含む行と、正規表現 ^[0-9]+ warnings? generated\.$ に当たる行を除いたもの
rest（空でなければ改行で連結し末尾に改行）+ W2001 の文面（N = n）を返す
```

### Phase 分割

- Phase 1（本チケットで実装する）: CLI、clang の旗、merge、cache の鍵、診断、`tests/pgo.mjs`、`run-control.mjs --pgo`、計測、文書。
- Phase 2（人間が求めた場合だけ）: C/C++ の参照実装にも PGO を掛けた比較（D8）、`run --profile-generate`、C ランタイムと PB01 の事前ビルド
  object の計装（D6）、trap の分岐への `!prof`（D9）、WASM の PGO（D7。profile を書き出す host 側の仕組みが要るので要承認）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N を必ず見る。
作業用のファイルは `/tmp/tz-PR07/` に置く。

### 手順 1: ベースラインと probe

- 変更: `tests/fixtures/pgo/branch/Main.tz`・`tests/fixtures/pgo/trap/Main.tz`（新規。内容はテスト計画）だけ。
- 内容: GUIDE §2.3 の基準コマンドの後、変更前のコンパイラと PGO なしの成果物を保存し、clang の IR PGO を C から作った IR で確かめる（停止条件 1）。
- 確認: すべて成功し、`ls raw` に `default_*.profraw` が 1 つ、最後の `grep -c` が 1 以上。同じコンパイラで 2 回作った成果物が `cmp` で一致することも確かめる（一致しなければ手順 2 の比較は使えないので報告する）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && mkdir -p /tmp/tz-PR07/base && cp target/release/tsuzuri /tmp/tz-PR07/tsuzuri-before
target/release/tsuzuri build tests/fixtures/pgo/branch -O3 --no-cache -o /tmp/tz-PR07/base/branch
target/release/tsuzuri build tests/fixtures/pgo/branch --emit llvm -O3 -o /tmp/tz-PR07/base/branch.ll
target/release/tsuzuri build benchmarks/control --emit object -O3 --no-cache -o /tmp/tz-PR07/base/control.o
target/release/tsuzuri build benchmarks/control --emit llvm -O3 -o /tmp/tz-PR07/base/control.ll
cd /tmp/tz-PR07
printf 'int main(int c, char **v) { int s = 0; for (int i = 0; i < c * 100000; i++) s += i %% 7 ? 1 : 3; return s & 1; }\n' > p.c
clang -O1 -S -emit-llvm p.c -o p.ll
clang -x ir -Wno-override-module -O2 -fprofile-generate=$PWD/raw p.ll -o p-gen && ./p-gen; ls raw
xcrun llvm-profdata merge -o p.profdata raw/*.profraw
clang -x ir -Wno-override-module -O2 -fprofile-use=$PWD/p.profdata p.ll -S -emit-llvm -o p-use.ll && grep -c branch_weights p-use.ll
```

### 手順 2: `Profile` と `build_profiled`（挙動は変えない）

- 変更: `src/driver.rs` の `Profile`・`build_profiled`（新規）・`build`・`build_complete`・`run_with_diagnostics`、`src/cache.rs` の `build_key`（引数 `profile`。呼び出しは `None`）。
- 内容: 設計「データ構造」の形にする。`driver::build` の signature は変えない。
- 確認: `cargo test --locked` が成功。`cargo build --release --locked` の後、手順 1 の 4 成果物を同じコマンドで `/tmp/tz-PR07/now/` に作り、4 つとも `cmp` が一致。

### 手順 3: CLI

- 変更: `src/main.rs` の `parse_arguments`・`Arguments`・`HELP`・`run_action`・`mod tests`。
- 内容: 設計「CLI と診断」の表の前半 7 行。`run_action` は `build_profiled` を呼ぶ（driver はまだ profile を使わない）。テスト計画の parse テスト 2 つを足す。
- 確認: `cargo test --locked --bin tsuzuri profile` が `running 2 tests` で `2 passed`。`cargo build --release --locked` の後、
  `target/release/tsuzuri run tests/fixtures/pgo/branch --profile-use x; echo $?` の stderr に `E2000` と `are only valid with build`、終了コード `2`。

### 手順 4: 計装（`--profile-generate`）

- 変更: `src/driver.rs` の `PreparedProfile`・`prepare_profile`（`Generate` だけ）・`build_complete`（本体の `clang`、`dwarf_sidecar` の `linker`、`build_key` への `prepared.key`）。
- 内容: `prepare_profile` を `let mut messages = Vec::new();` の直後、cache の準備より前に呼ぶ。flag は本体の `clang` の `-O<n>` の直後に渡す。
- 確認: 次の出力が `3699964`、`ls` に `default_*.profraw` が 1 つ、`grep -c` が 3 以上、`-g` の build も `3699964`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && cargo build --release --locked
rm -rf /tmp/tz-PR07/prof && target/release/tsuzuri build tests/fixtures/pgo/branch -O3 --profile-generate /tmp/tz-PR07/prof -o /tmp/tz-PR07/gen
/tmp/tz-PR07/gen && ls /tmp/tz-PR07/prof
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -h /tmp/tz-PR07/gen | grep -c __llvm_prf
target/release/tsuzuri build tests/fixtures/pgo/branch -O3 -g --profile-generate /tmp/tz-PR07/prof-g -o /tmp/tz-PR07/gen-g && /tmp/tz-PR07/gen-g
```

### 手順 5: `--profile-use` と `llvm-profdata`

- 変更: `prepare_profile`（`Use`）、`find_profdata`、`INDEXED_PROFILE_MAGIC`・`RAW_PROFILE_MAGIC`（新規）、`src/cache.rs` の `executable_path` を `pub(crate)` に、driver のテスト `prepares_profiles_and_rejects_raw_or_unknown_files`。
- 内容: 設計「アルゴリズム」の「profile の準備」と「`llvm-profdata` の発見」。
- 確認: `cargo test --locked --lib prepares_profiles` が `1 passed`。次の 2 つの実行が `3699964`、`cmp` が一致、最後が `E2002` と終了コード `1`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && cargo build --release --locked
target/release/tsuzuri build tests/fixtures/pgo/branch -O3 --no-cache --profile-use /tmp/tz-PR07/prof -o /tmp/tz-PR07/use && /tmp/tz-PR07/use
xcrun llvm-profdata merge -o /tmp/tz-PR07/branch.profdata /tmp/tz-PR07/prof/*.profraw
target/release/tsuzuri build tests/fixtures/pgo/branch -O3 --no-cache --profile-use /tmp/tz-PR07/branch.profdata -o /tmp/tz-PR07/use-file && /tmp/tz-PR07/use-file
cmp /tmp/tz-PR07/use /tmp/tz-PR07/use-file
TSUZURI_LLVM_PROFDATA=/tmp/tz-PR07/missing target/release/tsuzuri build tests/fixtures/pgo/branch --no-cache --profile-use /tmp/tz-PR07/prof -o /tmp/tz-PR07/x; echo $?
```

### 手順 6: 古い profile の警告

- 変更: `summarize_profile_warnings`（新規）、本体の `clang` の `run_tool` の hint と結果の扱い、driver のテスト `summarizes_profile_hash_mismatch_warnings`。
- 確認: `cargo test --locked --lib summarizes_profile` が `1 passed`。branch を `/tmp/tz-PR07/stale/` へ複製し、`| _ -> 100` を `| 2 -> 100` と `| _ -> 1000` の
  2 行に替えて `--profile-use /tmp/tz-PR07/prof` で build すると、成功し、stderr の `profile data does not match` の行がちょうど 1 つで `hash mismatch` を含まず、実行結果は `3699964`。

### 手順 7: E2E `tests/pgo.mjs`

- 変更: `tests/pgo.mjs`（新規）。
- 内容: テスト計画の E2E の全ケース。
- 確認: `node tests/pgo.mjs target/release/tsuzuri` が終了コード 0 で、最後に `PGO: ... passed` の 1 行を出す。

### 手順 8: 全体の確認

- 変更: なし。
- 確認: `cargo test --locked` が成功。`node tests/e2e.mjs target/release/tsuzuri`・`node tests/cache.mjs target/release/tsuzuri`・`node tests/pgo.mjs target/release/tsuzuri` が終了コード 0。
  手順 2 と同じ 4 成果物の `cmp` が `/tmp/tz-PR07/base/` と一致（停止条件 2）。

### 手順 9: runner の `--pgo`

- 変更: `benchmarks/run-control.mjs`。
- 内容: D8。引数の検査（`positional.some((arg) => arg.startsWith("-"))`）に `--pgo` を通す。`control.o` を作る前に、`--profile-generate <temporary>/profile` の
  `control.gen.o` と、`native` と同じ link に `-fprofile-generate` を足した `benchmark-train` を作り、`LLVM_PROFILE_FILE` を消した env で `--quick` を 1 回実行する。
  その後 `control.o` を `--profile-use <temporary>/profile` で作る。`--artifacts` があれば `profile/` も複製する。記録の欄（`variant` など）は変えない。
- 確認: `node benchmarks/run-control.mjs target/release/tsuzuri --quick --pgo` が終了コード 0（runner の checks が参照と一致）。`--pgo` なしの出力の欄は変わらない。

### 手順 10: 生成コードの確認と計測

- 変更: なし（生データは `target/perf/` と `/tmp/tz-PR07/`）。
- 確認: 「生成コードの確認」の期待がすべて成り立ち、「計測手順」の 3 run と M2・M3 の記録がそろう。

### 手順 11: 文書

- 変更: 「ドキュメント」の表のファイル。
- 確認: `node scripts/check-docs.mjs _docs/tools/command-line.md _docs/tools/build-and-cache.md` が終了コード 0。`git diff --check` が空。

## 計測手順

GUIDE §14 と PX01 の「計測手順」に従う。before と after は同じコンパイラ（本チケットの `target/release/tsuzuri`）で、`--pgo` の有無だけを変える
（PGO なしの成果物が変わらないことは手順 8 で確かめる）。

### 環境

PX01 の「環境と条件」のコマンドで `env.txt` を 3 つの run のディレクトリに残す。AC 電源、他の build・test・benchmark を止め、`uptime` の load average を確かめる。

### 時間（M1）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node24() { npx --yes --package=node@24 node "$@"; }
mkdir -p target/perf/PR07-before target/perf/PR07-after target/perf/PR07-before2
node24 benchmarks/run-control.mjs target/release/tsuzuri --metrics target/perf/PR07-before > target/perf/PR07-before/control.json
node24 benchmarks/run-control.mjs target/release/tsuzuri --pgo --artifacts target/perf/PR07-after/artifacts --metrics target/perf/PR07-after > target/perf/PR07-after/control.json
node24 benchmarks/run-control.mjs target/release/tsuzuri --metrics target/perf/PR07-before2 > target/perf/PR07-before2/control.json
node24 benchmarks/metrics.mjs report target/perf/PR07-before2 --baseline target/perf/PR07-before
node24 benchmarks/metrics.mjs report target/perf/PR07-after --baseline target/perf/PR07-before
```

標本は runner の既存の warm-up・較正・12 標本（PX01）。`before2` と `before` の比較で `改善`・`悪化` が出た metric は「判定できない」とする。
C・C++・Rust の行は 3 run で同じ物なので、環境の揺れの目安として読む。

### 大きさとビルド時間（M2・M3）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
A=target/perf/PR07-after/artifacts/profile M=/tmp/tz-PR07/m && mkdir -p $M
xcrun llvm-profdata merge -o $M/control.profdata $A/*.profraw
for trial in 1 2 3 4 5 6 7 8 9; do
  /usr/bin/time -p target/release/tsuzuri build benchmarks/control --emit object -O3 --no-cache -o $M/plain.o 2>> $M/time-plain.txt
  /usr/bin/time -p target/release/tsuzuri build benchmarks/control --emit object -O3 --no-cache --profile-use $A -o $M/dir.o 2>> $M/time-dir.txt
  /usr/bin/time -p target/release/tsuzuri build benchmarks/control --emit object -O3 --no-cache --profile-use $M/control.profdata -o $M/file.o 2>> $M/time-file.txt
done
wc -c $M/plain.o $M/file.o && cmp $M/dir.o $M/file.o
```

M3 は各ファイルの `real` の 9 個から中央値・最小・最大を取る。`cmp` が一致しない場合は D10 に反するので停止条件 4 として報告する。

### 記録

docs/benchmarks.md の `## PGO（PR07）`（新規）に、日付、計測機、commit、Clang・`llvm-profdata`・Node の版、訓練の入力（`--quick`）、M1 の report の表、
M2・M3 の表、生データの場所（`target/perf/PR07-*`、コミットしない）、そろえられなかった条件を書く。C/C++ の PGO との比較は書かない（Phase 2）。

## 生成コードの確認

`--emit llvm` は最適化前の IR なので PGO の効果は見えない。clang に渡す `module.ll` と profile を wrapper で取り出し、同じ旗で最適化後の IR を作って見る。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
G=/tmp/tz-PR07/code && rm -rf $G && mkdir -p $G
printf '#!/bin/sh\nfor a in "$@"; do case "$a" in */module.ll) cp "$a" %s/module.ll ;; -fprofile-use=*) cp "${a#-fprofile-use=}" %s/profile.profdata ;; esac; done\nexec /usr/bin/clang "$@"\n' $G $G > $G/clang-save && chmod +x $G/clang-save
target/release/tsuzuri build tests/fixtures/pgo/branch -O3 --no-cache --profile-generate $G/prof -o $G/gen && $G/gen
TSUZURI_CLANG=$G/clang-save target/release/tsuzuri build tests/fixtures/pgo/branch -O3 --no-cache --profile-use $G/prof -o $G/use
clang -x ir -Wno-override-module -O3 -fprofile-use=$G/profile.profdata $G/module.ll -S -emit-llvm -o $G/pgo.ll
clang -x ir -Wno-override-module -O3 $G/module.ll -S -emit-llvm -o $G/plain.ll
grep -c function_entry_count $G/pgo.ll; grep -c branch_weights $G/pgo.ll
grep -cE ' (fast|reassoc|nnan|ninf|nsz|arcp|contract|afn) ' $G/pgo.ll $G/plain.ll
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -h $G/gen | grep -c __llvm_prf; /opt/homebrew/opt/llvm@21/bin/llvm-objdump -h $G/use | grep -c __llvm_prf
```

- 期待: `function_entry_count` が 1 以上（Tsuzuri の `internal` 関数に profile が当たっている）、`branch_weights` が 1 以上、fast-math 系の flag の数が
  `pgo.ll` と `plain.ll` で等しい、計装版の `__llvm_prf` が 3 以上で PGO 版が 0。
- 機械語: `llvm-objdump -d --no-show-raw-insn` で `gen`・`use` と PGO なしの実行ファイルを逆アセンブルし、`classify` を含む関数の block の順序の違いを
  記録する。決まった形は期待しない（主張は計測で行う）。

## テスト計画

### Rust テスト

| ファイル | テスト（新規） | 確かめること |
| --- | --- | --- |
| `src/main.rs` の `mod tests` | `parses_profile_options` | `build Main.tz --profile-generate prof` が `Profile::Generate(std::path::absolute("prof"))`、`build Main.tz --profile-use p.profdata --emit object -O2` が `Profile::Use("p.profdata")`、旗なしが `Profile::None` |
| 同上 | `rejects_invalid_profile_combinations` | 設計の表の前半 7 行がそれぞれ `Err` で、メッセージが表の文面を含む（`run`・`test`・`check`・`fmt`・`doc` を全部試す） |
| `src/driver.rs` の `mod tests` | `prepares_profiles_and_rejects_raw_or_unknown_files` | 一時ディレクトリで: `Generate` の flag が `-fprofile-generate=` で始まり key の tag が `profile-generate-dir`。raw の magic の file は `E2000`（`is a raw profile`）、乱数 8 bytes は `E2000`（`is neither`）、空の directory は `E2000`（`has no .profraw files`）、indexed の magic で始まる file は成功し key が複製の `file_digest` と等しい。`llvm-profdata` を起動しない |
| 同上 | `summarizes_profile_hash_mismatch_warnings` | mismatch 2 行と `2 warnings generated.` だけの入力が W2001 の文面（N = 2）だけになる。無関係の警告行は残る。mismatch のない入力はそのまま |

環境変数を書き換えるテストは作らない（`std::env::set_var` は `unsafe` で、`Cargo.toml` の `unsafe_code = "forbid"` に反する）。`TSUZURI_LLVM_PROFDATA` は E2E で子プロセスの env として試す。

### E2E（`tests/pgo.mjs`、新規）

`tests/cache.mjs` の形（`compiler = resolve(process.argv[2] ?? "target/release/tsuzuri")`、`mkdtempSync`、`cli`）に従う。fixture は一時ディレクトリの別々の root へ
`cpSync` で複製する（再帰的なソース探索が兄弟を拾わないように）。env は `TSUZURI_CACHE_DIR` を一時ディレクトリにし、`LLVM_PROFILE_FILE` を消す。
file 形式のケースの `llvm-profdata` は `TSUZURI_LLVM_PROFDATA`、なければ macOS では `xcrun --find llvm-profdata` で得る。

fixture（検証済み。2026-09-30 に `target/release/tsuzuri run` と `build -O3` で確認）:

```tsuzuri
def classify :: i64 -> i64 = \n ->
    match n % 3 with
    | 0 -> 1
    | 1 -> 10
    | _ -> 100

let mut total = 0
let mut i = 0
while i < 100000 do
    total = total + classify i
    i = i + 1
total
```

```tsuzuri
def divide :: i64 -> i64 -> i64 = \a b -> a / b

let mut i = 3
while i > 0 do
    i = i - 1
divide 10 i
```

| ケース | 内容 | 期待（独立の根拠） |
| --- | --- | --- |
| 1 | branch を `-O0`・`-O3` × {通常、`--profile-generate`、`--profile-use <dir>`、`--profile-use <file>`}（`--no-cache`） | stdout `3699964\n`、終了コード 0（0〜99,999 の剰余 0・1・2 は 33,334・33,333・33,333 個なので 33,334 + 333,330 + 3,333,300） |
| 2 | generate の実行後の dir | `default_` で始まり `.profraw` で終わる file が 1 つ以上 |
| 3 | trap を `-O3 --trap-info` × {通常、generate、use} | signal `SIGTRAP`、stderr が `trap: integer division by zero at` と `Main.tz:1:43`（`a / b` の開始列を数えた値）を含み、3 つの `.trap.json` が byte 一致 |
| 4 | 古い profile（手順 6 の変更を複製に加える） | build 成功、`profile data does not match` の行がちょうど 1 つ、`hash mismatch` なし、`--json` では `"code":"W2001"` の行、実行結果 `3699964` |
| 5 | 決定性 | 同じ `--profile-use <file>` の `--no-cache` build 2 回の実行ファイルが byte 一致 |
| 6 | cache | 同じ file の use を 2 回で entry は 1 つだけ増える。branch の計装版をもう 1 回実行して内容の変わった dir、generate の dir A と B、profile なしの build はそれぞれ別の entry |
| 7 | 拒否 | `run --profile-use x` と `--target wasm32`・`--emit llvm` との組み合わせは終了コード 2 で `E2000`。raw の file は終了コード 1 で `is a raw profile`、空の dir は `has no .profraw files`、`TSUZURI_LLVM_PROFDATA` が存在しない path なら `E2002` |
| 8 | macOS の `-g` と generate | build 成功、実行結果 `3699964`（`dwarf_sidecar` の link に `-fprofile-generate` が要る） |

WASM は Phase 1 で拒否するのでケース 7 だけで扱う。Tsuzuri の IR は変わらないので、`live == 0` の確認は既存の suite の結果が変わらないことで足りる。

### 既存テストへの影響

なし。既存の期待値は変えない。`src/main.rs` のテストは新しい関数として足す。

### 性能

速度の合否条件は置かない（AGENTS.md）。性能は「計測手順」だけで扱う。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/language.md` | `--json` と build 専用オプションの段落、`## 診断` の「ツールの警告は `W2001` の JSON オブジェクトです。」の段落 | 2 つの旗、`E2000` の条件、古い profile の `W2001` |
| `docs/architecture.md` | `## 性能設計の原則` | clang の IR PGO を使うこと、関数名が `source_filename = "tsuzuri"` に依存すること、計装の範囲（D6）、cache の鍵 |
| `docs/benchmarks.md` | `## 現実的な次の指標` の前の `## PGO（PR07）`（新規） | 計測手順の「記録」 |
| `README.md` | `## ビルド` | generate → 訓練 → use の 3 行の例と `TSUZURI_LLVM_PROFDATA` |
| `_docs/tools/command-line.md` | `## build と run の主なオプション`、`## ツールと環境変数` | 旗の意味と組み合わせ、`TSUZURI_LLVM_PROFDATA`、`LLVM_PROFILE_FILE`、`--emit object` の最終 link には `-fprofile-generate` が要ること |
| `_docs/tools/build-and-cache.md` | `## キーと検証` | generate の dir と use の profile の SHA-256 が鍵に入ること |
| `_perfs/README.md` | 一覧の PR07 の状態欄 | `done` |

## 受け入れ条件

- [x] `build --profile-generate <dir>` の実行ファイルが通常版と同じ結果を返し、`<dir>` に `.profraw` を書く。（`tests/pgo.mjs` の「an instrumented program writes a raw profile when it exits」と、`-O0`〜`-Oz` など 12 通りの併用）
- [x] `build --profile-use <dir>` と `--profile-use <file>` が同じ成果物を作り、結果が通常版と一致する。（「a directory of raw profiles and its merge build the same program」。object は byte 同一）
- [x] 設計の診断の表のすべての行に対応するテスト（Rust または E2E）がある。（`src/profile.rs`・`src/main.rs` の単体テストと `tests/pgo.mjs` の「option errors need no tool」・「a profile that Clang cannot read …」・「llvm-profdata is found …」）
- [x] 古い profile で build が成功し、`W2001` の 1 行だけを出す。（「a profile of another program is summarized in one warning」）
- [x] PGO なしの成果物が変更前と byte 一致する（手順 8）。（`compare-defaults.mjs` で 798 件、不一致 0。統合後の木でも）
- [x] 同じ profile で成果物が byte 一致し、profile の内容・generate の dir の違いで cache の entry が分かれる。（「the cache keys follow the profile」）
- [x] `cargo test --locked`、`node tests/e2e.mjs`、`node tests/cache.mjs`、`node tests/pgo.mjs` が成功する。（「実装と検証」の「検証」）
- [x] 「生成コードの確認」の期待が成り立つ。（`docs/benchmarks.md` の「生成コードの確認」。entry count と `branch_weights` が付き、fast-math の flag は 0 件）
- [x] M1〜M3 を PX01 形式と docs/benchmarks.md に記録し、判定できない metric はそう書いている。（遅くなった種目も記録。Linux・x86-64・Windows は「測っていないもの」に書いた）
- [x] GUIDE §10 の完了の定義を満たす。（fmt・clippy 1.99・Windows の型検査・回帰テスト 4 つ・文書の検査）

## 落とし穴

- `internal` 関数の PGO 名は module の source file 名を含む（`<file>;<fn>`）。2026-09-30 に、同じ IR を別の path（`p1/a.ll` と `p2/a.ll`）から
  compile すると、profile が当たる `branch_weights` が 9 件から 0 件になることを確かめた。Tsuzuri の IR は `src/llvm.rs` のヘッダーで
  `source_filename = "tsuzuri"` を出すので、一時ディレクトリの path に左右されない。この行を消す・変える変更は PGO を黙って無効にする。
- `-fprofile-instr-generate` は `.ll` の入力に対して何もしない（「再現」）。使う旗は `-fprofile-generate` だけである。
- `-c` で作った計装 object の link には `-fprofile-generate` が要る。`dwarf_sidecar` の `linker` への付け忘れは、macOS の `-g` 付き build でだけ
  `___llvm_profile_runtime` の未定義として現れる（E2E ケース 8）。
- `.profraw` の名前は実行ファイルごとに変わる（`%m`）。別の build の計装版を同じ dir で走らせると、merge が古いデータを混ぜて `W2001` が出る。訓練ごとに新しい dir を使う。
- 環境に `LLVM_PROFILE_FILE` があると埋め込んだ dir より優先され、dir に何もできない。テストと runner の子プロセスからは消す。
- Homebrew の `llvm-profdata` が PATH にあっても、Apple clang の raw profile と版が合うとは限らない（G18 の落とし穴）。PATH は探さない（D3）。
- `.profraw` を `--profile-use` の file として渡すと clang が `bad magic` で失敗する。magic を先に見て `E2000` にする。
- cache の hit は保存した `messages` を再生するので、古い profile の `W2001` は hit でも出る（正しい挙動）。
- `-O0` の `--profile-use` は受け付けるが何も変えない。E2E で `-O0` の結果が一致しても PGO の確認にはならない。
- 並列 Task を使うプログラムでは、計装のカウンターの加算が原子的でないので回数は近似になる。結果には影響しない。
- `--emit llvm` の IR は `Entry::Library` で出るので、手で link して実行ファイルにしない（「再現」）。
- `benchmarks/run-control.mjs` は `-` で始まる未知の引数を拒否する。`--pgo` をその検査から外し忘れると usage で終わる。

## 対象外

- サンプリングによるプロファイル（AutoFDO／BOLT）、context-sensitive PGO（`-fcs-profile-generate`）、実行中の最適化。
- 原子的なカウンター（`-fprofile-update=atomic`）、`run`・`test` での PGO、C ランタイムと PB01 の事前ビルド object の計装（Phase 2）。
- C/C++ の参照実装に PGO を掛けた比較（Phase 2、D8）。WASM の PGO（Phase 2、D7）。

## 決定事項

### D1: CLI

- 決定: `build` だけに `--profile-generate <dir>` と `--profile-use <path>` を足す。両者は排他で、native の `--emit exe`・`object` だけで使える。
  組み合わせの誤りは `E2000`（設計の表）。`--emit object` の利用者は最終の link に `-fprofile-generate` を渡す。
- 理由: Clang・GCC と同じ名前で、C/C++ の利用者がそのまま使える。`run` へ広げると計装版の実行ファイルの置き場所と訓練の入力を決める必要がある。
- 状態: 実装済み（2026-10-17 の再検討で変更: `build` に加えて `run` も受け付ける（`run --profile-generate DIR` は計装して実行し DIR に書き、正常に終了したのに書かれなければ `W2001`）。許す出力は native の `exe`・`object` と wasm32 の `wasm`（D7）。診断の文面と許可リストは「着手時の再検討」の D1 のとおり）

### D2: 計装の方式

- 決定: clang の IR レベルの PGO（`-x ir` に `-fprofile-generate=<dir>`・`-fprofile-use=<file>`）を使う。`-fprofile-instr-generate`、`opt -passes=pgo-instr-gen`、G18 の自前の計装は使わない。
- 理由: 2026-09-30 の probe で Apple clang 21 が `.ll` に対して計装・適用とも行い、`-fprofile-instr-generate` は何もしないことを確かめた。`opt` を
  使うと新しいツールの発見と版合わせが要る。G18 のカバレッジは目的（行の網羅）も出力も違うので共有しない。
- 状態: 実装済み（変更なし）

### D3: profile の入力と `llvm-profdata`

- 決定: `--profile-use` は indexed profile の file か `.profraw` の directory を受ける。directory は `llvm-profdata merge` で一時ディレクトリへ merge する。
  `llvm-profdata` は `TSUZURI_LLVM_PROFDATA`、clang の実体と同じディレクトリ、macOS の `xcrun --find llvm-profdata` の順で探し、PATH は探さない。版番号は比べない。
- 理由: 訓練の dir をそのまま渡せる。raw profile は書いた runtime と同じ系列の `llvm-profdata` でしか確実に読めず、Apple clang の版番号は LLVM と
  対応しないので、clang と対になる場所を優先し、不一致は clang の読み込みの失敗（`E2002`）として報告する。
- 状態: 実装済み（2026-10-17 の再検討で変更: `xcrun` ではなく `clang -print-resource-dir` の 3 階層上の `bin/`。`src/profile.rs` の `find_profdata`）

### D4: 型の置き場所と cache の鍵

- 決定: `Profile` は `BuildOptions` の外に置き、`build_profiled` と `build_complete` の引数で渡す。鍵には generate なら dir の絶対 path、use なら
  一時ディレクトリへ複製・merge した indexed profile の SHA-256 を、`Some` のときだけ足す。
- 理由: `BuildOptions` は `Copy` で、`Debug` 表現が鍵に入る。外に置けば PGO なしの鍵と `driver::build` の signature が変わらない。path ではなく
  内容で鍵を作るので、profile を移動しても hit し、中身を替えれば miss する。
- 状態: 実装済み（2026-10-17 の再検討で変更: 入口は `build_profiled`、型は `src/profile.rs`、鍵の field は `ir-and-embedded-runtime` の直後で、`TSUZURI_LLVM_PROFDATA`・`LLVM_PROFILE_FILE` は鍵に入れない）

### D5: 古い profile の警告

- 決定: clang の `(hash mismatch)` の行を数えて除き、`W2001` の既存の経路で 1 行の要約を出す。build は成功させる。新しいコードは割り当てない。
- 理由: docs/language.md が「ツールの警告は `W2001`」と定めており、driver の `messages` は既にこの経路で出る。関数ごとの行をそのまま出すと大きい
  プログラムで数千行になる。`--deny-warnings` は codegen 前の検査の警告だけが対象なので、この警告では失敗しない。
- 状態: 実装済み（変更なし。3 つの Clang で同じ文面を確認）

### D6: ランタイムの範囲

- 決定: Phase 1 で計装・PGO の対象にするのは、clang に渡す 1 つの IR module（利用者のコードと連結された `src/runtime/*.ll`）だけ。C ランタイム
  （`task.c`）と PB01 の事前ビルド object には旗を渡さない。
- 理由: C ランタイムの compile に旗を足すと link と cache の経路が増え、PB01 の object の鍵にも profile が入る。分岐の多い利用者のコードが主な対象である。
- 状態: 実装済み（Phase 1 の範囲のまま取り下げ。2026-10-17 の測定で、数学のランタイムを埋め込んで計装しても差が測れず、PGO のビルドだけ埋め込む案はプロファイルの効果でない差を混ぜるので採らない。docs/benchmarks.md の PR07）

### D7: WASM

- 決定: Phase 1 は `--target wasm32` との組み合わせを `E2000` で拒否する。native で取った profile を WASM の build に使わない。
- 理由: WASM には `.profraw` を書き出す runtime がなく、host 側の仕組みは既定の import を増やす（D-18）。target が違うと inline と layout の判断も
  変わる。Phase 2 で扱う場合は新しい host の機能として別途承認を得る。
- 状態: 変更して実装済み（2026-10-17 の承認: 試作で counts が native と一致し、測れる効果（apps の `csv_aggregate` −14% など）が出たので、opt-in の wasm32 として出した。module の隣の `app.wasm.profile.mjs` の `writeProfile(instance)` が raw profile を書く。wasm64・threads・`--emit wasm` 以外・generate と `--wasm-opt` は `E2000`、Clang 21 だけ、間接呼び出しの値 profile なし。効果は workload と JavaScript エンジンで変わる）

### D8: 計測の方式

- 決定: `benchmarks/run-control.mjs` に `--pgo` を足し、`--quick`（大きさ 1024、seed 42）を訓練の入力にする。PX01 の記録の欄は変えず、run_id で
  PGO の有無を分ける。C/C++ の参照実装への PGO は Phase 2 とする。
- 理由: 訓練と計測の入力の大きさを分けつつ、runner の checks で計装版の正しさも確かめられる。記録の欄を変えると PX01 の schema に触れる。
- 状態: 変更して実装済み（`--pgo` は 1 run に plain と PGO の host を交互に実行し、PGO の列を `variant` の `pgo` で記録する（schema は変えない）。C・C++ も同じ profile で PGO にし、Rust は `--pgo-rust-llvm DIR` を付けたときだけ rustc の LLVM で PGO にする。`benchmarks/run-control.mjs`）

### D9: trap の分岐への `!prof`

- 決定: Phase 1 では Tsuzuri の IR に `!prof` を足さない。Phase 2 で、PGO なしの機械語で trap の block が hot path の間に置かれている例を
  「生成コードの確認」の方法で見つけた場合だけ、`llvm.expect` 相当の重みを検討する。
- 理由: trap の block は `llvm.trap` と `unreachable` で終わり、LLVM は既に冷たいと推定する。IR を変えると PGO なしの成果物が変わり、G2 に反する。
- 状態: 取り下げ（2026-10-17。21 の corpus の `-O3` の asm 177 個の trap のうち、hot path が飛び越える形は失敗の経路の中の 1 つだけで、`control` と `computations` は全部 out of line。`benchmarks/run-trap-layout.mjs`、docs/benchmarks.md の PR07。IR は変えていない）

### D10: 決定性

- 決定: directory の `.profraw` はファイル名の順で merge に渡す。file の profile も一時ディレクトリへ複製してから digest と clang に使う。
- 理由: 2026-09-30 に、merge と `-fprofile-use` の出力が同じ入力で byte 一致することを確かめた。順序と複製で、入力の列挙順と読み込み中の差し替えに左右されない。
- 状態: 実装済み（変更なし。ディレクトリを渡した build と merge 済みの file を渡した build の object は byte 同一、`tests/pgo.mjs` と M3 の `cmp`）

### D11〜D17（2026-10-17 の再検討で新設）の最終状態

- **D11（launcher は `E2002`）**: 実装済み。`src/profile.rs` の `bundled_launcher`（file stem が `tsuzuri-clang`）と `require_llvm_clang`。単体テスト `refuses_the_bundled_launcher`、`tests/pgo.mjs` の「the bundled launcher is refused」（偽の launcher が実行されないことも確かめる）。PR08 が `scripts/toolchain/clang.rs` に足した `-flto*` の転送の後も、配布物の launcher を `rustc` で作って `build`・`run` の両方の旗が `E2002` になることを確かめた（launcher は `-fprofile-*` を転送しないまま）。
- **D12（Windows・別の machine の target は `E2000`）**: 実装済み。Windows の host では `src/main.rs` の `rejects_profile_options_on_windows` が走る（実行は緑の Windows CI で確かめる。ここでは `x86_64`・`aarch64` の `pc-windows-msvc` の型検査だけ）。
- **D13（`src/profile.rs` に置く）**: 実装済み。`driver.rs` は `mod profile; pub use profile::Profile;` と呼び出しだけ（統合ブランチの `pub mod lto;` と並ぶ）。`run_tool`・`collect_message`・`driver_error`・`io_error`・`tool` は `pub(crate)`。
- **D14（profile は IR・clang・option に結び付く）**: 実装済み。`W2001` の文面と `docs` の「同じ条件でそろえます」。`run` の profile は `--trap-info` の IR のもので、`build --trap-info` の profile と一致する（`tests/pgo.mjs` の `run` の節は `run` の profile を `run` で使い、警告が出ない）。
- **D15（PGO は opt-in）**: 実装済み。docs と `docs/benchmarks.md` に、速くなる種目（`closure_*`）と遅くなる種目（`tail_*`・`match_dispatch`）の両方を記録し、「PGO を使えば速くなる」とは書いていない。
- **D16（併用）**: 実装済み。`-O0`〜`-O3`・`-Os`・`-Oz`・`--cpu native`・`--allocator small`・`--strip`・`--trap-info`・`-g`・`--no-cache` の 12 通りで、計装版が通常版と同じ出力を返し `.profraw` を 1 つ書くこと、使う build の出力が同じことを `tests/pgo.mjs` で確かめた。`TSUZURI_LINKER=lld`（PB03）とも組める（Homebrew LLVM 21.1.8 と `lld` で手で確かめた。テストにはしていない）。
- **D17（`toolchain info` は変えない）**: 実装済み（変更なし。`tests/toolchain.rs` が通る）。

## 実装と検証

Phase 1（native の計装・適用・cache・診断・計測）と Phase 2（`run`、C・C++・Rust の参照の PGO、D6・D9 の判断、wasm32）のすべてを、ブランチ `perf/p4-PR07` に実装した。統合ブランチ `perf/phase4-compiler-performance`（PR08 を取り込んだ `532bf725`）をマージし、競合（`src/driver.rs` の 3 か所と `README.md` の 1 か所。どちらも両方を残した）を解決した。

### コミット

| 作業 | コミット | 内容 |
| --- | --- | --- |
| WP0 | `b13b60f8` | 着手時の再検討（このチケットだけ） |
| WP1〜WP4 | `2b88ad21` | `src/profile.rs`（`Profile`・検査・launcher の `E2002`・`llvm-profdata` の探索と merge・SHA-256・`W2001` の要約）、`build_profiled`、CLI と `--help`、計装と適用、`build_key` の `profile` の field、`cache` の成果物名 `profile`、単体テスト |
| WP5 | `f0eb1181` | `tests/pgo.mjs`（14 節）と `tests/fixtures/pgo/`、利用者が渡したパスの表示 |
| WP6 | `bc3eb9b5` | `benchmarks/run-control.mjs --pgo`（計装した control の訓練、plain と PGO の host の交互実行、生成コードの確認） |
| WP9 | `a9105ace`、`46800137` | C・C++ の参照も同じ手順で PGO にする、`--pgo-rust-llvm DIR`（rustc の LLVM の clang と `llvm-profdata` で Rust も PGO にする。任意） |
| WP8 | `d469f889` | `run --profile-generate`・`--profile-use`（書かれなかった profile の警告を含む） |
| WP11 | `f55f513b` | `benchmarks/run-trap-layout.mjs`（D9 の corpus 走査） |
| WP12 | `c61bcc11` | wasm32 の `--profile-generate`・`--profile-use`（host が書く raw profile、`src/runtime/profile-wasm.mjs`） |
| WP7 | `e556bc14` | 文書（言語リファレンス、`docs/*`、README）、`benchmarks/run-wasm-pgo.mjs`、`docs/benchmarks.md` の PR07 の測定（WP10 の D6 の判断の記録を含む） |
| WP13 | `e9ab1e06` | 決定事項 D1〜D10 の最終状態 |
| 統合 | `e851b6a4`、`a6893724` | `perf/phase4-compiler-performance`（PR08）のマージ、`--emit bitcode` と profile の旗の併用が `E2000` であることのテスト（`profile.rs`・`main.rs`・`tests/pgo.mjs`）、`--help` と文書の文面 |
| 完了 | このコミット | チケットの完了と検証の記録、`_completed/` への移動 |

### 実装したこと

- `--profile-generate DIR`・`--profile-use PATH`（`build` と `run`。排他、どちらも 1 回）。許す出力は native の `--emit exe`・`object`（この機械の target）と `--target wasm32` の `--emit wasm` だけの許可リストで、新しい出力（PR08 の `bitcode`、PB05 の `--backend fast` など）は既定で `E2000`。診断は既存の `E2000`・`E2001`・`E2002`・`W2001` だけ。
- 計装と適用は Clang の IR PGO。Tsuzuri の LLVM IR は変えず、本体の `clang -x ir` の `-O<n>` の直後に `-fprofile-generate=<絶対パス>` か `-fprofile-use=<複製した profile>` を足す（macOS の `-g` の別リンクにも計装の旗を渡す）。旗を付けない build の成果物と cache の鍵は変わらない。
- `--profile-use`: indexed の `.profdata`（magic を確かめて複製し SHA-256 を鍵にする）か、`.profraw` のディレクトリ（`llvm-profdata merge` をファイル名の順に実行）。`llvm-profdata` は `TSUZURI_LLVM_PROFDATA`、Clang の隣、`clang -print-resource-dir` の 3 階層上の順で探し、PATH は見ない。合わない関数は `W2001` の 1 行にまとめ、build は成功する。
- `run --profile-generate`: `--trap-info` の IR で計装して実行し、正常終了なのに profile が書かれなかったときは `W2001`（`LLVM_PROFILE_FILE` があれば出さない）。
- wasm32: module に profile runtime はない。計装した module の隣に `<out>.profile.mjs` を書き、host が `writeProfile(instance)` を呼ぶと Clang 21 の raw profile（version 10、32 bit）が書かれ、`llvm-profdata` が読む。`wasm-ld` が `__start/__stop___llvm_prf_*` を export し、既定の import は増えない。
- 測定: `benchmarks/run-control.mjs --pgo`（Tsuzuri・C・C++・任意で Rust）、`benchmarks/run-wasm-pgo.mjs`、`benchmarks/run-trap-layout.mjs`。PX01 の schema は変えず、PGO の列は `variant` の `pgo`。
- テスト: `src/profile.rs` の単体 11 件、`src/main.rs` の 3 件（拒否の表を含む）、`src/cache.rs` の成果物名、`tests/pgo.mjs`（14 節。Clang の profile runtime か `llvm-profdata` がなければ理由を出して skip し、`LLVM_PREFIX` か `TSUZURI_PGO_STRICT=1` があれば skip を失敗にする）。

### 実装しなかったこと、取り下げたこと

- D6（C ランタイム・PB01 の object・数値ランタイムの計装）と D9（trap の分岐の `!prof`）は、測定で効果の根拠がなく取り下げた（上の D6・D9、`docs/benchmarks.md` の PR07）。
- Windows（`E2000`。Windows CI で確かめてから外す）、別の machine の `--target TRIPLE`、wasm64、wasm の threads、`--freestanding` の計装、`--emit bitcode`（PR08）などの出力は `E2000`。`llvm-profdata` と profile runtime は配布物に入れていない（launcher は `E2002`）。
- 原子的なカウンター、サンプリングの PGO（AutoFDO・BOLT）、context-sensitive PGO、`test`・`bench` での PGO。
- `.github/workflows/*` は触っていない。`tests/pgo.mjs` の CI への配線は統合担当が行う。

### 測定（`benchmarks/control`、native `-O3`、`--cpu generic`、M1 Max、macOS 27.0.1、load average 15〜50 の共有機）

PGO あり／なしの時間比（1 未満は PGO のほうが短い。plain と PGO の host を 5 回ずつ交互に動かした 60 標本の CPU 時間の中央値。`apple-2`。ほかの 2 回の run（Apple clang の `apple-1`、Homebrew LLVM 21.1.8 の `brew-1`）も同じ向き）:

| 種目 | Tsuzuri | C | C++ | Rust（LLVM 22） |
| --- | ---: | ---: | ---: | ---: |
| `closure_capture` | 0.831 | 0.835 | 0.833 | 0.833 |
| `closure_churn` | 0.877 | 0.569 | 0.563 | 0.395 |
| `tail_mix`・`tail_if_mix`・`tail_builtin_mix` | 1.20〜1.21 | 1.01 | 1.00〜1.01 | 1.00〜1.01 |
| `match_dispatch` | 1.639 | 1.004 | 1.006 | 1.011 |
| ほかの 10 種目 | 0.99〜1.06（host の配置の揺れ。C・C++・Rust も同じ向き） | | | |

- PGO は一律には速くならない。速くなる種目は 2 つ（`closure_*`）で、遅くなる種目は 4 つ（`tail_*`、`match_dispatch`）。C・C++・Rust が PGO で変わらない `tail_*`・`match_dispatch` を Tsuzuri だけが悪化させる原因は、Clang の観察（`tail_*` は PGO で inline の threshold が 75 になり、`match_dispatch` は vectorizer の fallback loop の重みで unroll されない）までで、Tsuzuri の IR の形を直す課題として残る（このチケットの範囲外）。全員が PGO を使うと Tsuzuri は `tail_*` で C の約 1.20 倍、`closure_churn` で C の 1.57 倍、`match_dispatch` で Rust の 1.61 倍の時間になる。
- 費用（Apple clang／Homebrew LLVM）: object は 5,392 B → 計装 17,144／16,064 B（約 3 倍）、PGO 5,848／5,808 B（+8%）。ビルド全体の中央値は 140／146 ms → `--profile-use file` 157／179 ms、`--profile-use dir`（merge 込み）193／178 ms。Clang の compile に約 10 ms、merge に約 20 ms。`.profraw` は 6.5 KB。
- wasm32（Node 24、`--no-liftoff`、15 回交互）: apps の `csv_aggregate` 0.865、`tokenize` 0.916、`format_join` 0.945、`tree_transform` 0.947、`closure_pipeline` 1.054、control は ±3.4%。Node 20 では `closure_capture` が 1.52 倍の時間になる（再現）。効果は workload と JavaScript エンジンで変わり、module の大きさは apps で +15.8%。
- 生成コードの確認: PGO の IR に entry count 20 関数・重みの付いた命令 121・`branch_weights` 29。fast-math 系の flag は plain も PGO も 0 件。trap の block が hot path を飛び越える例は 177 個中 1 個（D9）。

詳細と再現の手順は `docs/benchmarks.md` の `phase4:PR07`。x86-64・Linux・Windows は測っていない。

### 検証

| 確認 | 結果 |
| --- | --- |
| `RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast`（`TSUZURI_CLANG` は Homebrew LLVM 21.1.8。統合ブランチのマージ後） | 1487 passed、0 failed、1 ignored（測定用）。107 の test binary（doc-test を含む）、28 分 34 秒。うち lib が 324（`profile` が 11）、bin が 25（profile が 3）、`tests/lto.rs` が 17 |
| GUIDE §3.1 の回帰テスト 4 つ（既定の stack） | それぞれ ok（4 passed） |
| `rustup run 1.99.0 cargo fmt --all -- --check`、`CARGO_TARGET_DIR=target/clippy199 rustup run 1.99.0 cargo clippy --all-targets --locked -- -D warnings` | 問題なし |
| `cargo check --all-targets --locked --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc`（rustc 1.96.1） | 警告 0 |
| `node tests/pgo.mjs target/release/tsuzuri` | Apple clang 21.0.0 で 14 節が成功（skip 0）、Homebrew LLVM 21.1.8（`LLVM_PREFIX`、`TSUZURI_PGO_STRICT=1`）でも 14 節が成功 |
| node の E2E（`target/release/tsuzuri`）: `e2e`、`cache`、`size`、`llvm_handoff`、`prebuilt_runtime`、`handoff_tools`、`benchmark_report`、`wasm_memory`、`bindings`、`trap_boundary`、`targets` | すべて exit 0（`e2e` は 1 回目が load average 60〜70 の下で、`Semantics.tz` の 1 回の build が 90 秒の timeout に当たり、load が下がってからの再実行で成功。baseline の compiler でも同じ build が 8〜20 秒かかった） |
| 既定の出力が変わらない: `target/perf/PR07/compare-defaults.mjs`（baseline の `tsuzuri-before` と、`examples/*`・`tests/fixtures/*` の 114 project × 7 構成〔IR・header・object・実行ファイル（cache なし・あり・`--trap-info`）・wasm32〕を比較） | 798 件のうち 461 件の成果物が完全に一致（Mach-O の `LC_UUID`・署名を除く）、337 件は両方が同じ終了状態と診断で失敗、不一致 0（統合後の木。マージ前の 791 件でも不一致 0） |
| 統合後の確認: `--profile-generate`・`--profile-use` と `--emit bitcode` | native は `needs --emit exe or object`、wasm32 は `needs --emit wasm for WebAssembly` の `E2000`（`profile.rs`・`main.rs`・`tests/pgo.mjs`） |
| 同梱の launcher（`scripts/toolchain/clang.rs` を `rustc` で build） | PR08 の `-flto*` の転送の後も、`build`・`run` の両方の旗が `E2002` |
| `node scripts/check-docs.mjs`（option・usage・diagnostics・platforms・native-interop・webassembly・index）、`sh scripts/check-runtime-includes.sh`、`git diff --check` | 7 pages、298 links、17 examples、12 native runs。runtime の include は 51 files。問題なし |

### 起票・再検討との違いと訂正

- 起票の「`build` だけ」「native だけ」は、承認された Phase 2 で `run` と wasm32 を受けるようになった（D1・D7）。wasm32 は Clang 21 の raw profile（version 10）だけを書き、間接呼び出しの値 profile は取らない（`-mllvm -disable-vp`）。
- 起票の `llvm-profdata` の探索の `xcrun --find` は、`clang -print-resource-dir` の 3 階層上に替えた（D3。macOS の分岐が要らず、wrapper の Clang でも効く）。
- 起票の「分岐の多い処理は PGO で速くなる」は成り立たなかった。分岐の多い `tail_*`・`match_dispatch` は遅くなり、速くなったのは closure の 2 種目だけ（D15）。測定は悪化した種目も省かず記録した。
- `driver::build` と `BuildOptions` の `Copy` は変えず、入口を `build_profiled` にした（D4）。cache の成果物名に `profile`（wasm の書き出し役）を許可リストへ足した（`src/cache.rs`）。`scripts/check-runtime-includes.sh` は `src/profile.rs` の `include_str!` も見る。
- Apple clang は entry が冷たい関数に `cold minsize` を足す（上流の Clang は `cold` だけ）。それでも Homebrew LLVM 21.1.8 の結果は Apple clang と同じ向きだった。profile は Apple clang と Homebrew の Clang の間で移せない（1 関数が hash mismatch）。

### 既知の制限

- Linux（ELF の `--gc-sections` が `__llvm_prf_*` を残すか、`libclang-rt-21-dev` の `libclang_rt.profile`）、x86-64、Windows は実行していない。`tests/pgo.mjs` は CI の LLVM 21（`llvm-21`・`libclang-rt-21-dev`）で動く見込みだが、確かめていない。Windows は `E2000`。
- `--emit bitcode`（ThinLTO）との併用は確かめていないので `E2000`。許すには `Profile::validate` の許可リストに足し、`llvm-link` で併合した C ランタイムを含む module の profile が合うことを host を含めて確かめる必要がある。
- profile は IR・Clang・option に結び付く（D14）。コンパイラの IR を変える変更（PM02〜PM04 など）の後は、訓練をやり直す。
- 並列 Task の計装のカウンターは原子的でなく、回数は近似になる（結果には影響しない）。
- wasm32 の効果は workload と JavaScript エンジンで変わる（Node 20 の `closure_capture` の悪化を含む）ので、opt-in のままにする。
- 同梱の toolchain には `llvm-profdata` と profile runtime がなく、launcher では使えない（`E2002`）。`TSUZURI_CLANG` に LLVM 21 の Clang を指す必要がある。
