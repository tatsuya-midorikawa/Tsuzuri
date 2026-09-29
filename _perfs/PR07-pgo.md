# PR07: プロファイル誘導最適化（PGO）

| 項目 | 内容 |
| --- | --- |
| ID | PR07 |
| 分類 | 実行速度 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | PX01, (PB01) |
| 関連 | PR08, G11 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（CLI の旗 `--profile-generate`・`--profile-use` と環境変数 `TSUZURI_LLVM_PROFDATA` は D1・D3 で確定。診断は既存の `E2000`・`E2001`・`E2002`・`W2001` だけを使う） |
| 手本にする既存実装 | 値を取る build 専用の旗と組み合わせの拒否: `src/main.rs` の `parse_arguments` の `--cpu`・`--wasm-feature`（`"--wasm-feature is only valid with build"` を返す `Err`）と、`src/driver.rs` の `BuildOptions::validate`。外部ツールの起動と失敗の報告: `src/driver.rs` の `tool`・`run_tool`・`collect_message`（`TSUZURI_LLVM_LINK` の `llvm-link` の呼び出し）。cache の鍵への条件付きの追加: `src/cache.rs` の `build_key`（`options.cpu == Cpu::Native` のときだけ足す field）。ツールを差し替える E2E: `tests/cache.mjs` の `cli` と clang の wrapper |
| 主な影響ファイル | `src/main.rs`（`parse_arguments`・`Arguments`・`run_action`・`HELP`・`mod tests`）, `src/driver.rs`（`build`・`build_complete`・`build_profiled`（新規）・`Profile`（新規）・`prepare_profile`（新規）・`find_profdata`（新規）・`summarize_profile_warnings`（新規））, `src/cache.rs`（`build_key`）, `tests/pgo.mjs`（新規）, `tests/fixtures/pgo/`（新規）, `benchmarks/run-control.mjs`（`--pgo`（新規））, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md`, `_docs/tools/command-line.md`, `_docs/tools/build-and-cache.md`, `_perfs/README.md`（状態欄） |
| 計測対象 | `node benchmarks/run-control.mjs` の Tsuzuri の行（native、`-O3`、`--cpu generic`）。主指標は分岐の多い `match_dispatch`・`while_mix`・`for_mix`・`tail_mix`・`record_pipeline`。成果物サイズとビルド時間は参考として記録する |

## 目的

実行時のプロファイルを使って、分岐の配置・インライン展開・関数の配置を LLVM に最適化させる。
C/C++（Clang の `-fprofile-generate`／`-fprofile-use`）、Rust、Go が持つ PGO を Tsuzuri の native ビルドでも使えるようにし、分岐の多い処理で
C/C++ と同じ手段で競えるようにする。PGO は最適化の判断だけを変え、プログラムの意味は一切変えない。

実装者は Phase 1（native の計装・適用・cache・診断・計測）だけを行う。Phase 2（設計「Phase 分割」）は人間が求めた場合だけ着手する。

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

- [ ] `build --profile-generate <dir>` の実行ファイルが通常版と同じ結果を返し、`<dir>` に `.profraw` を書く。
- [ ] `build --profile-use <dir>` と `--profile-use <file>` が同じ成果物を作り、結果が通常版と一致する。
- [ ] 設計の診断の表のすべての行に対応するテスト（Rust または E2E）がある。
- [ ] 古い profile で build が成功し、`W2001` の 1 行だけを出す。
- [ ] PGO なしの成果物が変更前と byte 一致する（手順 8）。
- [ ] 同じ profile で成果物が byte 一致し、profile の内容・generate の dir の違いで cache の entry が分かれる。
- [ ] `cargo test --locked`、`node tests/e2e.mjs`、`node tests/cache.mjs`、`node tests/pgo.mjs` が成功する。
- [ ] 「生成コードの確認」の期待が成り立つ。
- [ ] M1〜M3 を PX01 形式と docs/benchmarks.md に記録し、判定できない metric はそう書いている。
- [ ] GUIDE §10 の完了の定義を満たす。

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
- 状態: 既定案（実装者はこの案に従う）

### D2: 計装の方式

- 決定: clang の IR レベルの PGO（`-x ir` に `-fprofile-generate=<dir>`・`-fprofile-use=<file>`）を使う。`-fprofile-instr-generate`、`opt -passes=pgo-instr-gen`、G18 の自前の計装は使わない。
- 理由: 2026-09-30 の probe で Apple clang 21 が `.ll` に対して計装・適用とも行い、`-fprofile-instr-generate` は何もしないことを確かめた。`opt` を
  使うと新しいツールの発見と版合わせが要る。G18 のカバレッジは目的（行の網羅）も出力も違うので共有しない。
- 状態: 既定案（実装者はこの案に従う）

### D3: profile の入力と `llvm-profdata`

- 決定: `--profile-use` は indexed profile の file か `.profraw` の directory を受ける。directory は `llvm-profdata merge` で一時ディレクトリへ merge する。
  `llvm-profdata` は `TSUZURI_LLVM_PROFDATA`、clang の実体と同じディレクトリ、macOS の `xcrun --find llvm-profdata` の順で探し、PATH は探さない。版番号は比べない。
- 理由: 訓練の dir をそのまま渡せる。raw profile は書いた runtime と同じ系列の `llvm-profdata` でしか確実に読めず、Apple clang の版番号は LLVM と
  対応しないので、clang と対になる場所を優先し、不一致は clang の読み込みの失敗（`E2002`）として報告する。
- 状態: 既定案（実装者はこの案に従う）

### D4: 型の置き場所と cache の鍵

- 決定: `Profile` は `BuildOptions` の外に置き、`build_profiled` と `build_complete` の引数で渡す。鍵には generate なら dir の絶対 path、use なら
  一時ディレクトリへ複製・merge した indexed profile の SHA-256 を、`Some` のときだけ足す。
- 理由: `BuildOptions` は `Copy` で、`Debug` 表現が鍵に入る。外に置けば PGO なしの鍵と `driver::build` の signature が変わらない。path ではなく
  内容で鍵を作るので、profile を移動しても hit し、中身を替えれば miss する。
- 状態: 既定案（実装者はこの案に従う）

### D5: 古い profile の警告

- 決定: clang の `(hash mismatch)` の行を数えて除き、`W2001` の既存の経路で 1 行の要約を出す。build は成功させる。新しいコードは割り当てない。
- 理由: docs/language.md が「ツールの警告は `W2001`」と定めており、driver の `messages` は既にこの経路で出る。関数ごとの行をそのまま出すと大きい
  プログラムで数千行になる。`--deny-warnings` は codegen 前の検査の警告だけが対象なので、この警告では失敗しない。
- 状態: 既定案（実装者はこの案に従う）

### D6: ランタイムの範囲

- 決定: Phase 1 で計装・PGO の対象にするのは、clang に渡す 1 つの IR module（利用者のコードと連結された `src/runtime/*.ll`）だけ。C ランタイム
  （`task.c`）と PB01 の事前ビルド object には旗を渡さない。
- 理由: C ランタイムの compile に旗を足すと link と cache の経路が増え、PB01 の object の鍵にも profile が入る。分岐の多い利用者のコードが主な対象である。
- 状態: 既定案（実装者はこの案に従う）

### D7: WASM

- 決定: Phase 1 は `--target wasm32` との組み合わせを `E2000` で拒否する。native で取った profile を WASM の build に使わない。
- 理由: WASM には `.profraw` を書き出す runtime がなく、host 側の仕組みは既定の import を増やす（D-18）。target が違うと inline と layout の判断も
  変わる。Phase 2 で扱う場合は新しい host の機能として別途承認を得る。
- 状態: 既定案（実装者はこの案に従う）

### D8: 計測の方式

- 決定: `benchmarks/run-control.mjs` に `--pgo` を足し、`--quick`（大きさ 1024、seed 42）を訓練の入力にする。PX01 の記録の欄は変えず、run_id で
  PGO の有無を分ける。C/C++ の参照実装への PGO は Phase 2 とする。
- 理由: 訓練と計測の入力の大きさを分けつつ、runner の checks で計装版の正しさも確かめられる。記録の欄を変えると PX01 の schema に触れる。
- 状態: 既定案（実装者はこの案に従う）

### D9: trap の分岐への `!prof`

- 決定: Phase 1 では Tsuzuri の IR に `!prof` を足さない。Phase 2 で、PGO なしの機械語で trap の block が hot path の間に置かれている例を
  「生成コードの確認」の方法で見つけた場合だけ、`llvm.expect` 相当の重みを検討する。
- 理由: trap の block は `llvm.trap` と `unreachable` で終わり、LLVM は既に冷たいと推定する。IR を変えると PGO なしの成果物が変わり、G2 に反する。
- 状態: 既定案（実装者はこの案に従う）

### D10: 決定性

- 決定: directory の `.profraw` はファイル名の順で merge に渡す。file の profile も一時ディレクトリへ複製してから digest と clang に使う。
- 理由: 2026-09-30 に、merge と `-fprofile-use` の出力が同じ入力で byte 一致することを確かめた。順序と複製で、入力の列挙順と読み込み中の差し替えに左右されない。
- 状態: 既定案（実装者はこの案に従う）
