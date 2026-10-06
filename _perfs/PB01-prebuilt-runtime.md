# PB01: ランタイムの事前ビルドと必要部分だけのリンク

| 項目 | 内容 |
| --- | --- |
| ID | PB01 |
| 分類 | ビルド速度 |
| 優先度 | P0 |
| 規模 | M |
| 依存 | PX03 |
| 関連 | G11, G14, PM08, PR07, PR08 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（Phase 1）。Phase 2 は要承認: D9（配布物への object の同梱。G14 の配布形式に依存）。Phase 3 は要承認: D10（リンカーの未使用コード除去と `math.ll` の分離。全実行ファイルのリンクを変える） |
| 手本にする既存実装 | object の保存・検証・原子的な公開・掃除: `src/cache.rs` の `BuildCache::open`・`load`・`store`・`evict`。鍵とツールの同定: `src/cache.rs` の `build_key`（`tool-path`・`tool-binary`・`tool-version-stdout`・`native-cpu-features`）。ランタイム object を作ってリンクへ渡す経路: `src/driver.rs` の `build_complete` の `native_runtime` 分岐（`task.c` → `task.o` → `-x none`・`clang -r`）。`numeric.ll` を単独の module としてコンパイル・リンクする例: `tests/display_parse.mjs`（C harness と native・WASM × `-O0`・`-O3`） |
| 主な影響ファイル | `src/llvm.rs`（`EmitOptions`・`Instrumentation`・`emit_program`・`numeric_declarations`（新規））、`src/driver.rs`（`build_complete`・`runtime_object`（新規））、`src/cache.rs`（`build_key`・`ToolIdentity`（新規）・`tool_identity`（新規）・`runtime_key`（新規）・`KEY_ENVIRONMENT`（新規））、`EmitOptions` を構築するテスト（`tests/cpu_dispatch.rs`・`tests/debug_info.rs`・`tests/debug_output.rs`・`tests/trap_locations.rs`・`tests/windows.rs`）、`tests/prebuilt_runtime.mjs`（新規）、`benchmarks/run-build.mjs`（PX03 で新規。variant の追加だけ）、`docs/architecture.md`、`docs/benchmarks.md`、`README.md`、`_perfs/README.md` |
| 計測対象 | `examples/hello` の実行ファイル（native `-O0`・`-O3`）、数値の表示だけを使う WASM（`--target wasm32 -O0`・`-O3`）、`numeric.ll` と C ランタイムの両方を使う `examples/tasks`（native `-O0`・`-O3`）。状態は whole-build cache が miss でランタイム object が cache にある状態（variant `build-runtime-warm`（新規））と、`--no-cache`（変わらないことの確認） |

## 目的

ビルドのたびに同じランタイムを Clang でコンパイルし直す処理をなくし、小さなプログラムのビルドを C のコンパイルと同じ程度の時間に近づける。
`examples/hello` の実行ファイルの `-O3` ビルド 0.53 s のうち 0.43 s が数値ランタイム `src/runtime/numeric.ll` のコンパイルで、
現在の最も大きく、最も解消しやすいビルド時間の無駄である。

- 対象は `numeric.ll` とネイティブの C ランタイム（`task.c`・`cpu.c`・`io.c`）。一度コンパイルした object を whole-build cache の
  root の下の専用の場所に置き、target・最適化レベル・CPU 指定・WASM の機能・Clang・ソースが同じなら再利用する。
- 実行結果・トラップ・確保・生成コードの性能は変えない。`numeric.ll` の入口は今でも `weak hidden` で、LLVM は inline しないので、
  別 object にしても最適化の機会は減らない（「現状と計測」）。

実装者は Phase 1 だけを実装する。Phase 2（配布物への同梱、G14）と Phase 3（必要な関数だけのリンク、PM08）は人間が求め、
D9・D10 が承認された場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- PX03 が `_perfs/README.md` の一覧の状態欄で done であること（`benchmarks/run-build.mjs` と PX01 の `benchmarks/metrics.mjs` がある）。
  確認: `grep -n "PX01\|PX03\|PB01" _perfs/README.md` と `ls benchmarks/run-build.mjs benchmarks/metrics.mjs`。
- GUIDE §2.3 の基準コマンドが成功すること。
- 計測機に Apple Clang 21（`clang --version` が `Apple clang version 21.0.0 (clang-2100.3.34.2)`）、`wasm-ld`（`command -v wasm-ld`。
  このマシンは `/opt/homebrew/bin/wasm-ld`）、Node 24、`python3` があること。
- 「再現」の手作業の実験がこのマシンで同じ結果になること（実装手順 1）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. `numeric.ll` の `define` のうち生成 IR から名前で参照される関数が `weak hidden` でない。宣言だけでは別 object の定義に届かず、
   `src/runtime/generate.py` の変更が必要になる。
2. `numeric.ll` が `@llvm.*` 以外の関数を `declare` している（他のランタイム IR の関数を参照し始めた）。
3. `external_numeric`（新規、D5）の IR に `@tz.trap.report` か `%tz.context` が現れた（トラップ計装と組み合わさった）。
4. 事前ビルドの有無で、実行結果・終了コード・トラップの報告・確保の追跡（`live == 0`）・WASM の import のどれかが変わる。
5. 既存テストの期待値（IR、診断、`tests/cache.mjs` の entry 数や Clang の起動回数）を変える必要がある。
   `EmitOptions` の構築に field を足すだけの変更は除く。
6. `--emit llvm`・`--emit header`・`--emit wgsl` の出力、または `--emit object` の object の記号表（`nm -m`）が変わる。
7. cache を `crate::cache::default_root()` の外に書く必要がある、または `--no-cache` でランタイムの cache を読む・書く経路ができた。
8. Windows の経路の変更、`unsafe`、新しい crate、既定の WASM import が必要になった。
9. PX01 の実行時間の比較で `-O3` の種目が悪化した（広がりを超える）。

## 現状と計測（HEAD `f8dc655`）

### ランタイムが成果物に入る経路

- `src/llvm.rs` の `emit_program` は、生成 IR の文字列に特定の名前が現れるかを `output.contains` で調べ、該当するランタイム IR の全文を
  末尾に連結する。`@tz.rec.` → `recursive.ll`、`@tsuzuri_task_parallel(` など → WASM では `task-wasm.ll`（native は宣言だけ）、
  `@tz.debug.write` → `debug.ll`、`@tz.display.` → `display.ll`、`@tz_soft_` → IR から `declare void @llvm.trap()\n` を消してから
  `numeric.ll`、`@tz_math_` → `math.ll` を足した後に全体の `declare` 行を名前で重複除去（`BTreeSet`、`lines()` を `"\n"` で結合し直す）、
  `@tz.closure.` → `closure.ll`、`@tz.character.` → `character.ll`、`@tz.string.`・`@tz.alloc` など → `string.ll`・`utf8string.ll`・
  `heap-native.ll`（WASM は `heap-wasm.ll`）。
- `Globals::default`（`src/llvm.rs`）は生成 IR の metadata 番号を `numeric.ll` と `math.ll` の最大の番号より後から始める（`FIRST_METADATA`）。
- `src/driver.rs` の `build_complete` は、`--target wasm32` のとき IR の末尾に `src/runtime/wasm.ll`（i128 の補助関数、`weak hidden` 9 個）を足す。
  native で IR が `declare void @tsuzuri_task_parallel(`・`declare i64 @tsuzuri_cpu_sum_i64(`・`declare i32 @tsuzuri_io_` を含むと
  （`task_runtime`・`cpu_runtime`・`io_runtime`）、`task_runtime_source()`・`cpu.c`・`io.c` を一つの `task.c` に連結して一時ディレクトリに書き、
  `clang -std=c11 -c <native_compile_args> -O<n> [-pthread] [-g] [-S -emit-llvm] [-mcpu=native] task.c -o task.o` をビルドのたびに実行する。
- 実行ファイルは `clang -x ir -Wno-override-module -O<n> <native_compile_args> [cpu] module.ll -o artifact -lm -x none task.o [-pthread]` の
  1 回の起動で作る（macOS の `-g` はリンクを分けて `dsymutil`）。`--emit object` は `module.o` と `task.o` を `clang -r -nostdlib`
  （macOS は `-Wl,-keep_private_externs`、Linux は `-no-pie`）で 1 つにする。WASM は `clang -x ir ... --target=wasm32-unknown-unknown
  -mbulk-memory [-matomics] -msimd128|-mno-simd128 -c` の後に `wasm-ld --no-entry --stack-first -z stack-size=1048576
  --max-memory=16777216 [--strip-all] [--export=...] module.o -o artifact`。
- `native_compile_args` は Windows 以外で `-fPIC`、Windows で `--target=<arch>-pc-windows-msvc`。`-ffunction-sections`・`-dead_strip`・
  `--gc-sections` はどこにもない。
- `src/main.rs` は `trap_info: trap_info || action == Action::Run` で、`tsuzuri run` は常にトラップ位置を計装する。`src/llvm_traps.rs` の
  `instrument` は IR の全関数を走査し、外部名（`@tz_<export>`・`@main` など）以外のすべての `define` に `%tz.context` 引数を足し
  （`append_context`）、`@llvm.trap` の呼び出しを `@tz.trap.report` に置き換える。`numeric.ll` の関数も書き換えの対象になる。

### `numeric.ll` と他のランタイム IR の形

- `src/runtime/generate.py` が `numeric.c` から `clang --target=x86_64-unknown-linux-gnu -std=c11 -O1 -ffreestanding -fno-builtin
  -fno-stack-protector -S -emit-llvm` で生成し、target 行と target 属性を除き、`define hidden ` を `define weak hidden ` に置き換える。
  11,337 行、460,147 bytes。
- `define` は 21 個。入口 9 個が `weak hidden`（`@tz_soft_op`・`fma`・`cmp`・`hash_canonical`・`math_unary`・`math_binary`・`cast`・
  `format`・`parse`）、補助 12 個が `internal fastcc`。global は 2 個。`declare` は `@llvm.*` の intrinsic だけ。
- 生成 IR からの参照は `src/llvm.rs`（`tz_soft_cast`・`op`・`cmp`・`format`・`parse`）、`src/llvm_hash.rs`（`hash_canonical`）、
  `src/llvm_math.rs`（`fma`・`cmp`・`math_unary`・`math_binary`）で、すべて入口 9 個のどれか。
- `weak` の定義は interposable なので、LLVM は呼び出し元へ inline せず、関数属性の推論にも使わない。今でも利用者のコードと
  `numeric.ll` の間で関数をまたぐ最適化は起きていない。
- `math.ll`（`generate_math.py`、5,653 行、283,718 bytes）の `@tz_math_*` 57 個はすべて `internal` で、本体 4 行の
  `tz_math_sqrt_f64`・`tz_math_fabs_f64` などは `-O3` で呼び出し元に inline されうる。手書きのランタイム IR（`string.ll`・`utf8string.ll`・
  `heap-*.ll`・`closure.ll`・`character.ll`・`display.ll`・`debug.ll`・`recursive.ll`・`console.ll`・`task-wasm.ll`）も関数は `internal` で小さい。
- `tests/display_parse.mjs` はすでに `numeric.ll` を単独の module として `tests/display_parse_runtime.c` と native・WASM
  （`wasm.ll` と一緒）× `-O0`・`-O3` でリンクして検査している。

### cache

- `src/cache.rs` の `build_key` は `format`・`compiler-version`・`compiler-binary`（コンパイラの SHA-256）・`host-os`・`host-arch`・
  `options`（`BuildOptions` の `Debug` 表示）・`action`・`ir-and-embedded-runtime`（IR の全文）・ソース・環境変数 17 個（`TSUZURI_CLANG`・
  `PATH`・`SDKROOT`・`CPATH` など）・使うツールごとの `tool-path`・`tool-binary`・`tool-version-stdout`・`tool-version-stderr`、
  `--cpu native` なら `native-cpu-features`（`clang -dM -E -x c - -mcpu=native`）を SHA-256 にかける。
- `BuildCache::open` は root に marker（`tsuzuri whole-build cache v1\n`）を作る。`load` は `metadata.json` の size と SHA-256 を検査し、
  不一致なら miss。`store` は `.lock-<key>` を `create_new` で取り（取れなければ何もしない）、一時ディレクトリで組んで `rename` で公開し、
  最後に `evict` する。受け付けるファイル名は `artifact`・`traps`・`dwarf` だけ。1 entry は 256 MiB まで、root 全体は 2 GiB・30 日。
- `evict` は root の直下のうち 64 桁の 16 進・`.lock-`・`.tsuzuri-` で始まる名前だけを扱い、サブディレクトリなどそれ以外は触らない。
  `tests/cache.mjs` の `entries()` も 64 桁の 16 進の名前だけを数える。同じファイルの Clang wrapper のテストは `--emit object` の
  Clang の起動を数え、2 回目のビルドで増えないこと（hit）を確かめる。
- `--no-cache` は `build`・`run` だけが受け付け、cache を読みも書きもしない。cache が使えないときは `build cache disabled: ...` を
  `W2001` の警告として出して続ける。
- whole-build cache は入力が 1 byte でも変われば miss になり、そのたびに `numeric.ll` と C ランタイムをコンパイルし直す。

### 計測済みの事実

2026-09-29（`--no-cache`、各 1 回、Apple M1 Max。元の起票の値）:

- `examples/hello` の実行ファイルは `-O0` 0.33 s、`-O3` 0.53 s。フロントエンド（IR の出力まで）は 0.02 s、C の hello は 0.07 s。
- ライブラリとしての IR（`--emit llvm`、`Entry::Library`）は 3 KB だが、実行ファイル（`Entry::Console`）は結果の表示に
  `@tz_soft_format` を使うので `numeric.ll` 全体を連結して 464 KB になる。その Clang `-O3` だけで 0.43 s、`-O0` で 0.06 s。
- Task を使うプログラムは `task.c` を毎回コンパイルする（`-O3` で 0.08 s）。

2026-09-30 の再計測（同じマシン、`/usr/bin/time -p`、各 1 回）:

| 対象 | `-O0` | `-O3` |
| --- | ---: | ---: |
| `examples/hello` の実行ファイル（`--no-cache`） | 0.14 s | 0.48 s |
| `numeric.ll` だけの `clang -x ir -fPIC -c` | 0.06–0.07 s | 0.41–0.44 s |
| hello の IR から `numeric.ll` を除いた残り（4,124 bytes）の `clang -c` | 0.02 s | 0.02 s |
| 2 つの object のリンク（`clang user.o numeric.o -lm`） | 0.02–0.05 s | 0.02–0.04 s |
| `math.ll`（入口を `hidden` に置き換えた写し）の `clang -c` | 0.05 s | 0.16 s |
| `task.c` だけの `clang -std=c11 -fPIC -pthread -c` | 0.07 s | 0.06 s |
| `numeric.ll` の wasm32 object（`-mbulk-memory -mno-simd128`） | – | 0.43 s |

- 実行ファイルの IR（`module.ll`）は 464,271 bytes で、そのうち `numeric.ll` が 460,147 bytes。
- 手作業の実験（「再現」）: `numeric.ll` を除いた IR に 1 行の宣言を足して別の object とリンクすると、hello の出力 `5050`・終了コード 0、
  実行ファイルの大きさ（`-O0` 100,824 bytes、`-O3` 67,256 bytes）、`nm` の `tz_soft_` の数（9）が今と同じ。object の記号は
  `weak private external _tz_soft_format`。
- WASM（数値の表示だけのプログラム）でも、分けてリンクした成果物は今と同じ 19,512 bytes、import 0、`tz_value()` が `2n`。
- `examples/tasks` の `-O3` の実行ファイルは `clang -std=c11 -c -fPIC -O3 -pthread task.c -o task.o` と、`tz_soft_` を含む IR の
  `clang ... -x none task.o` の 2 回の Clang で作る（C ランタイムと `numeric.ll` の両方を使う）。
- whole-build cache あり（`TSUZURI_CACHE_DIR` を空の一時ディレクトリ）の hello `-O3`: 1 回目（miss）0.56 s、2 回目（hit）0.10 s。
  hit の 0.10 s はフロントエンド・鍵の計算（コンパイラと Clang の SHA-256、`clang --version` の起動）・成果物の復元で、PB01 では減らない下限。

### 再現（2026-09-30 に確認）

実行ファイルの IR は一時ディレクトリとともに消えるので、`TSUZURI_CLANG` に写しを取る wrapper を指定して取り出す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
W=/tmp/tz-pb01 && mkdir -p $W/spy $W/spyw $W/disp
printf '#!/bin/sh\nfor a in "$@"; do case "$a" in *.ll|*.c) cp "$a" "$SPY_DIR/";; esac; done\nexec clang "$@"\n' > $W/clang-spy.sh
chmod +x $W/clang-spy.sh
/usr/bin/time -p target/release/tsuzuri build examples/hello -O3 --no-cache -o $W/ref3
SPY_DIR=$W/spy TSUZURI_CLANG=$W/clang-spy.sh target/release/tsuzuri build examples/hello -O3 --no-cache -o $W/ref3
wc -c $W/spy/module.ll src/runtime/numeric.ll
/usr/bin/time -p clang -x ir -Wno-override-module -O3 -fPIC -c src/runtime/numeric.ll -o $W/numeric3.o
python3 $W/split.py $W/spy/module.ll $W/user3.ll
clang -x ir -Wno-override-module -O3 -fPIC -c $W/user3.ll -o $W/user3.o
clang $W/user3.o $W/numeric3.o -lm -o $W/new3
$W/ref3; echo " $?"; $W/new3; echo " $?"
ls -l $W/ref3 $W/new3
nm -m $W/numeric3.o | grep tz_soft_format
```

期待: 2 つの実行ファイルとも `5050` と `0`、どちらも 67,256 bytes、最後の行は `weak private external _tz_soft_format`。
`$W/split.py` は次の内容（Math を使わないプログラム専用。`math.ll` の宣言の重複除去が `numeric.ll` の宣言行を消すため）。

```python
import re, sys
ir, rt = open(sys.argv[1]).read(), open("src/runtime/numeric.ll").read()
assert rt in ir, "numeric.ll is not verbatim (programs using Math are not supported)"
user = ir.replace(rt, "")
decl = {}
for m in re.finditer(r"^define weak hidden (.*?) (@\w+)\((.*)\)[^()\n]*\{$", rt, re.M):
    params = re.sub(r" %[\w.]+", "", m[3])
    decl[m[2]] = f"declare hidden {m[1]} {m[2]}({params})"
extra = [decl[n] for n in sorted(set(re.findall(r"@tz_soft_\w+", user)))]
if "@llvm.trap" in user and "declare void @llvm.trap()" not in user:
    extra.append("declare void @llvm.trap()")
open(sys.argv[2], "w").write(user + "\n".join(extra) + "\n")
```

WASM（`tests/display_parse.mjs` の display-only と同じソース）:

```sh
printf 'export def value :: i64\nfn value = (to_string 42).length\n' > $W/disp/Main.tz
SPY_DIR=$W/spyw TSUZURI_CLANG=$W/clang-spy.sh target/release/tsuzuri build $W/disp --target wasm32 -O3 --no-cache -o $W/ref.wasm
python3 $W/split.py $W/spyw/module.ll $W/userw.ll
clang --target=wasm32-unknown-unknown -mbulk-memory -mno-simd128 -x ir -Wno-override-module -O3 -c $W/userw.ll -o $W/userw.o
clang --target=wasm32-unknown-unknown -mbulk-memory -mno-simd128 -x ir -Wno-override-module -O3 -c src/runtime/numeric.ll -o $W/numericw.o
wasm-ld --no-entry --stack-first -z stack-size=1048576 --max-memory=16777216 --strip-all --export=tz_value $W/userw.o $W/numericw.o -o $W/new.wasm
node -e 'const fs=require("fs");for(const f of process.argv.slice(1)){const m=new WebAssembly.Module(fs.readFileSync(f));console.log(fs.statSync(f).size,WebAssembly.Module.imports(m).length,new WebAssembly.Instance(m).exports.tz_value())}' $W/ref.wasm $W/new.wasm
```

期待: 2 行とも `19512 0 2n`。WASM の IR では `emit_program` が `declare void @llvm.trap()` を消しているので `split.py` が足し直す
（intrinsic なので宣言がなくても Clang は受け付けるが、二重にすると失敗する。落とし穴 1）。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する（計測手順）。

- G1: whole-build cache が miss でランタイム object が cache にある状態で、hello の実行ファイルのビルドを `-O0`・`-O3` とも 0.1 s 以下
  （C の hello と同程度）に近づける。計算上の見込みは約 0.15 s（hit の下限 0.10 s + 利用者の IR 0.02 s + リンク 0.02–0.04 s）で、
  残りの鍵の計算は PB06 の対象。
- G2: ランタイム object の cache が hit したビルドでは、`numeric.ll` と C ランタイムのための Clang を起動しない。
- G3: `--no-cache` のビルド・`--emit llvm`・`--emit object` の出力・トラップ計装のあるビルド（`run`）は HEAD と同じ（時間も同程度）。
- G4: 実行時間と成果物の大きさは変わらない。

| 指標 | 単位・統計 | 対象 | 期待（計算値。計測で確かめる） |
| --- | --- | --- | --- |
| M1 ビルド時間 | ms（PX01 の `wall_time`、warm-up 1 回を除く 9 標本の中央値・最小・最大） | variant `build-runtime-warm` の hello・`examples/tasks`（native `O0`・`O3`） | hello `O3` 約 560 → 約 150、`O0` 約 200 → 約 150 |
| M2 外部ツールの時間 | ms（PX01 の `phase_time` の `tool.clang`、中央値） | M1 と同じ | `numeric.ll` の分（`O3` 約 430、`O0` 約 65）と `task.c` の分（約 60–70）が消える |
| M3 Clang の起動回数 | 回（固定値） | `tests/prebuilt_runtime.mjs` の wrapper の記録 | hello の実行ファイル: ランタイム miss 2 回、hit 1 回。`examples/tasks`: miss 3 回、hit 1 回 |
| M4 成果物の大きさ | bytes（PX01 の `executable_bytes`・`wasm_bytes`、1 標本） | PX03 の `build`・`build-wasm` | 変わらない（hello `O0` 100,824、`O3` 67,256。WASM の display-only 19,512） |
| M5 `--no-cache` の時間 | ms（PX03 の B1） | PX03 の `build`・`build-wasm` | 変わらない（差が広がりの範囲内） |
| M6 実行時間 | ms（PX01 の既存の種目） | `docs/benchmarks.md` の native `-O3` の種目 | 変わらない（差が広がりの範囲内） |

## 変えてはいけない意味

- 実行結果・stdout・stderr・終了コード・トラップの種類と報告（`--trap-info`、`run`）。事前ビルドの object は、今 IR に連結している
  `numeric.ll`・C ランタイムと同じテキストを、同じ `-O<n>`・target・CPU 指定・WASM の機能でコンパイルしたもの。整数の折り返し、丸め、NaN、
  符号付きゼロ、飽和、評価順序、トラップは同じ機械語から来る。`-ffast-math`・`-ffunction-sections`・LTO は足さない。
- 確保の追跡。`numeric.ll` は確保しない（`declare` が intrinsic だけ）。`@tz.alloc`・`@tz.free` を持つ `heap-native.ll`・`heap-wasm.ll` は IR に残るので、
  `tests/primitives.mjs` などの確保の計数と `live == 0` は変わらない。
- 可視性と ABI。`numeric.ll` の入口は `weak hidden` のまま（`generate.py`・`numeric.ll` を変えない）。生成 IR の宣言は `declare hidden` で、
  実行ファイル・WASM の外へ記号を出さない。`--emit object` の object と記号表、`--emit llvm` の IR は byte 単位で HEAD と同じ。
- WASM。import を足さない。`--strip-all` と明示の `--export` は同じ。
- 決定性。同じ入力の IR は byte 単位で同じ（宣言は `numeric.ll` の `define` の順）。ランタイム object は一時ディレクトリを current dir にして
  相対のファイル名でコンパイルし、object に一時ディレクトリの名前を残さない。cache の有無・hit と miss で実行ファイルの動作は同じ。
- cache。`--no-cache` は HEAD と同じ経路（ランタイムを IR に連結し、C ランタイムを一時ディレクトリでコンパイル）で、cache を読みも書きもしない。
  書く場所は `default_root()` の下だけ。ランタイム object の cache が使えなくてもビルドは成功する（D8）。cache の中身は SHA-256 の検査を通ったものだけを使う
  （`BuildCache::load`）。
- ランタイムのソース。`numeric.c`・`numeric.ll`・`math.ll`・`task.c`・`cpu.c`・`io.c` を変えない（`FIRST_METADATA` の規則も変えない）。

## 設計

### 事前ビルドする範囲

D1 の一覧。Phase 1 で object にするのは 2 種類だけで、それ以外は今の経路のまま。

| ランタイム | Phase 1 | 理由 |
| --- | --- | --- |
| `numeric.ll`（unit `numeric`） | object（native の実行ファイル、WASM の `--emit wasm`） | `-O3` で 0.41–0.44 s。入口が `weak hidden` で、分けても inline の機会を失わない |
| `task.c`・`cpu.c`・`io.c` の連結（unit `native-c`） | object（native の実行ファイルと `--emit object`） | 今も別 object。ソース（連結後のテキスト）と引数が同じなら結果も同じ |
| `math.ll` | IR のまま（Phase 3） | 入口がすべて `internal`。`tz_math_sqrt_f64`・`tz_math_sqrt_f32`・`tz_math_fabs_f64`・`tz_math_fabs_f32`（本体 4 行）などが `-O3` で inline されうる。分けるには linkage の変更と実行時間の計測が要る |
| `wasm.ll` | IR のまま | 6,966 bytes。WASM の `numeric` object の i128 の libcall はこの `weak hidden` 定義で解決する（「再現」で確認） |
| `string.ll`・`utf8string.ll`・`heap-native.ll`・`heap-wasm.ll`・`closure.ll`・`character.ll`・`display.ll`・`debug.ll`・`recursive.ll`・`console.ll`・`task-wasm.ll`・`host_abi::allocator` | IR のまま | 小さく、呼び出しの多い `internal` の関数（文字列の比較・確保・解放など）で、`-O3` で inline される。コンパイルの時間も小さい |
| `task-wasm-threads.c`・`heap-wasm-threads.ll` | 今のまま | `--wasm-feature threads` は Phase 1 の対象外（D3） |

### 使う条件

`numeric` の object を使うのは次のすべてを満たすビルドだけ。満たさなければ HEAD と同じく IR に連結する（D3・D5）。

```rust
let external_numeric = options.cache
    && !cfg!(windows)
    && !options.debug_info
    && !options.trap_info
    && !options.wasm_threads
    && matches!(
        (options.target, options.emit),
        (Target::Native, Emit::Executable) | (Target::Wasm32, Emit::Wasm)
    );
```

`native-c` の object を使うのは `options.cache && !cfg!(windows) && !options.debug_info && native_runtime` かつ `options.emit` が
`Executable`・`Object` のビルド（トラップ計装は C ランタイムを書き換えないので `run` でも使う）。

### データ構造

```rust
// src/llvm.rs
pub struct EmitOptions {
    pub entry: Entry,
    pub wasm: bool,
    pub debug_output: bool,
    pub external_numeric: bool, // （新規）true なら numeric.ll を連結せず、入口の宣言だけを出す
}
struct Instrumentation<'a> {
    // 既存の traps・debug・cpu_dispatch・wasm_threads に加えて
    external_numeric: bool, // （新規）
}
pub(crate) fn numeric_declarations() -> &'static str; // （新規）OnceLock<String>

// src/cache.rs
const RUNTIME_FORMAT: u64 = 1; // （新規）
pub(crate) const KEY_ENVIRONMENT: [&str; 17] = [/* build_key の環境変数の列をそのまま移す */]; // （新規）
pub(crate) struct ToolIdentity(Vec<(&'static str, Vec<u8>)>); // （新規）build_key が hash にかける (名前, byte 列) の並び
pub(crate) fn tool_identity(variable: &str, fallback: &str, cpu: crate::driver::Cpu) -> io::Result<ToolIdentity>; // （新規）
pub(crate) fn runtime_key(unit: &str, source: &[u8], args: &[OsString], clang: &ToolIdentity) -> String; // （新規）

// src/driver.rs（新規）。cache は (default_root(), Clang の同定)、unit は "numeric" | "native-c"、input は一時ディレクトリ
// （current dir にする）の中の相対名 "numeric.ll" | "task.c"、args は入力と出力を除く Clang の引数
fn runtime_object(cache: Option<(&Path, &ToolIdentity)>, unit: &str, source: &str, input: &str,
    args: &[OsString], directory: &Path, messages: &mut Vec<String>) -> Result<PathBuf, Diagnostic>;
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| LLVM | `src/llvm.rs` | `EmitOptions` | `external_numeric` を足す |
| LLVM | `src/llvm.rs` | `Instrumentation` | `external_numeric` を足す（`Default` は false） |
| LLVM | `src/llvm.rs` | `emit_target`・`emit_test_runner` | `external_numeric: false` |
| LLVM | `src/llvm.rs` | `emit_with_options`・`emit_selected` | `emit_selected` に引数 `external_numeric` を足し、`Instrumentation { external_numeric, ..Default::default() }` で渡す |
| LLVM | `src/llvm.rs` | `emit_with_trap_info`・`emit_with_debug_info`・`emit_native_build`・`emit_wasm_threads_build` | `options.external_numeric` を `Instrumentation` へ渡す。トラップ・debug の計装、または `wasm_threads` と同時に true なら `E2000`（`internal compiler error: the external numeric runtime cannot be combined with trap, debug, or thread instrumentation`）を返す |
| LLVM | `src/llvm.rs` | `emit_program` の `@tz_soft_` の分岐 | `external_numeric` なら `numeric_declarations()` を足すだけ（`declare void @llvm.trap()` は消さない）。それ以外は HEAD のまま |
| LLVM | `src/llvm.rs` | `numeric_declarations`（新規） | `include_str!("runtime/numeric.ll")` の `define weak hidden ` で始まる行ごとに、最後の `)` までを取り、各引数の末尾の ` %<名前>` を除き、`declare hidden ` を前に付けた行を連結する（D6） |
| LLVM | `src/llvm.rs` | `Globals::default` | 変更なし（`FIRST_METADATA` は同じ番号から始まり、IR は有効なまま） |
| cache | `src/cache.rs` | `KEY_ENVIRONMENT`（新規）・`build_key` | 環境変数の列を定数へ移す。ツールの loop の本体を `tool_identity` に移し、`TSUZURI_CLANG` は引数の `clang: Option<&ToolIdentity>` があればそれを使う。hash にかける byte 列は HEAD と同じ |
| cache | `src/cache.rs` | `ToolIdentity`・`tool_identity`（新規） | `tool-path`・`tool-binary`・`tool-version-stdout`・`tool-version-stderr`、`TSUZURI_CLANG` で `Cpu::Native` なら `native-cpu-features`・`native-cpu-diagnostics` を HEAD と同じ順に持つ |
| cache | `src/cache.rs` | `runtime_key`（新規） | D4 の field を SHA-256 にかけ、64 桁の 16 進を返す |
| cache | `src/cache.rs` | `BuildCache` | 変更なし。root を `default_root()` の `runtime` に変えた別の instance として `open`・`load`・`store` を使う（ファイル名は `artifact`） |
| driver | `src/driver.rs` | `build_complete`（`llvm::EmitOptions` の構築） | 「使う条件」の `external_numeric` を渡す |
| driver | `src/driver.rs` | `build_complete`（cache の準備） | `options.cache` で Clang を使う emit なら `tool_identity("TSUZURI_CLANG", "clang", options.cpu)` を 1 回だけ求め、`build_key` とランタイムの鍵で共有する。失敗したら既存の `build cache disabled: ...` と同じ経路 |
| driver | `src/driver.rs` | `runtime_object`（新規） | D7 の手順 |
| driver | `src/driver.rs` | `build_complete`（`native_runtime` の分岐） | 「使う条件」を満たせば、`runtime.arg(...)` の手組みの代わりに `runtime_object(.., "native-c", &source, "task.c", ..)` で `task.o` を得る。満たさなければ HEAD のまま |
| driver | `src/driver.rs` | `build_complete`（実行ファイルのリンク・`wasm-ld`） | `external_numeric` なら `runtime_object(.., "numeric", include_str!("runtime/numeric.ll"), "numeric.ll", ..)` の object を、実行ファイルは `-x none` の後（`task.o` の後）、`wasm-ld` は `module.o` の後に渡す |
| テスト | `tests/cpu_dispatch.rs`・`tests/debug_info.rs`・`tests/debug_output.rs`・`tests/trap_locations.rs`・`tests/windows.rs` | `llvm::EmitOptions { .. }` の構築 | `external_numeric: false` を足すだけ |
| テスト | `tests/prebuilt_runtime.mjs`（新規） | 全体 | テスト計画 |
| 計測 | `benchmarks/run-build.mjs` | `VARIANTS` | `build-runtime-warm`（新規、D11）を足す |
| 検査 | `scripts/check-runtime-includes.sh` | なし | 変更なし（新しい `include_str!` は増えない） |

### 生成 IR とランタイム

`external_numeric` の IR は、`numeric.ll` の全文（460,147 bytes）の代わりに入口 9 個の宣言を末尾に持つ。形（`tz_soft_format` は「再現」で確認済み。
残り 8 行は `numeric.ll` の `define` から同じ規則で作る）:

```llvm
declare hidden i32 @tz_soft_format(ptr noundef writeonly, ptr noundef readonly, i32 noundef)
```

ビルドの Clang・リンカーの起動（`[]` は条件付き。`<numeric args>` は D4）:

```text
# numeric の object（ランタイムの cache が miss のときだけ。current dir は一時ディレクトリ）
clang -x ir -Wno-override-module -O<n> -fPIC [-mcpu=native] -c numeric.ll -o numeric.o
clang -x ir -Wno-override-module -O<n> --target=wasm32-unknown-unknown -mbulk-memory -msimd128|-mno-simd128 -c numeric.ll -o numeric.o
# native-c の object（同じく miss のときだけ）
clang -std=c11 -c -fPIC -O<n> [-pthread] [-mcpu=native] task.c -o task.o
# native の実行ファイル
clang -x ir -Wno-override-module -O<n> -fPIC [-mcpu=native] <dir>/module.ll -o <dir>/artifact -lm -x none [<dir>/task.o] [<dir>/numeric.o] [-pthread]
# WASM
wasm-ld --no-entry --stack-first -z stack-size=1048576 --max-memory=16777216 [--strip-all] [--export=...] <dir>/module.o <dir>/numeric.o -o <dir>/artifact
```

cache の配置:

```text
<default_root()>/            whole-build cache（変更なし。evict は 64 桁の 16 進などの名前しか扱わない）
  <64 桁の 16 進>/           whole-build の entry
  runtime/                   ランタイム object の cache（新規。BuildCache の別の instance。marker・.lock-・evict も独立）
    <64 桁の 16 進>/metadata.json
    <64 桁の 16 進>/artifact  object の本体
```

### アルゴリズム

`runtime_object` の手順（D7）:

```text
object = directory / (input の拡張子を .o に変えた名前)
if cache = Some((root, clang)):
    key = runtime_key(unit, source, args, clang)
    match BuildCache::open(root/"runtime") and then load(key):
        Ok(Some(entry)) => entry.files["artifact"] を object に書いて返す
        Ok(None) => 続ける
        Err(e) => messages へ "runtime object cache read failed; rebuilding: {e}"（open の失敗は "runtime object cache disabled: {e}"）
write directory/input = source
run_tool(clang args... input -o object, current_dir = directory)   // 失敗は今と同じ診断（E2002）
if cache が開けていれば store(key, {"artifact": object}, [])   // 失敗は "runtime object cache write failed: {e}"
return object
```

### Phase 分割

- Phase 1（このチケット）: 上のすべて。macOS と Linux の native の実行ファイル・`--emit object`（`native-c` だけ）、WASM の `--emit wasm`。
- Phase 2（D9 が承認された場合だけ。設計方針）: G14 の配布物に主要な組（native の `O0`・`O3` × `--cpu generic`、wasm32 の `O0`・`O3` × SIMD の有無）の
  object を同梱する。探す順は配布物の読み取り専用ディレクトリ → ランタイムの cache → コンパイル。配布物の object の鍵は G14 が同梱する Clang
  （`vsc/src/toolchain.ts` の `tsuzuri-clang`）の同定を使い、利用者の Clang が違えば使わない。`--no-cache` でも配布物の object は使う
  （cache ではなく toolchain の一部）。
- Phase 3（D10 が承認された場合だけ。設計方針）: ランタイム object を `-ffunction-sections -fdata-sections` で作り、実行ファイルのリンクに
  `-Wl,-dead_strip`（macOS）・`-Wl,--gc-sections`（Linux）を足す（`wasm-ld` は既定で未使用の関数を除く）。PM08 の型ごとの表示の分割と合わせて、
  整数の表示だけのプログラムから浮動小数点・decimal のコードを除く。`math.ll` は本体が大きい関数（`tz_math_pow_f64` など）だけを `hidden` にして
  object へ移し、4 行の `sqrt`・`fabs` は IR に残す。使うランタイムの判断を IR の文字列の検索から IR 生成時の使用記号の集合へ移す（PB02 と共有）。
  元の起票の Phase 2（関数単位のリンク）と Phase 3（検索の置き換え）はここにまとめた。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。`W=/tmp/tz-pb01`。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドと「再現」を実行し、HEAD のコンパイラと出力を保存する。
- 確認: 「再現」の期待どおり。次の 4 ファイルができる（`before-tasks.nm` は空でない）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && W=/tmp/tz-pb01
cargo build --release --locked && cp target/release/tsuzuri $W/tsuzuri-before
target/release/tsuzuri build $W/disp --emit llvm -O3 --no-cache -o $W/before-disp.ll
target/release/tsuzuri build $W/disp --target wasm32 --emit llvm -O3 --no-cache -o $W/before-disp-wasm.ll
target/release/tsuzuri build examples/tasks --emit object -O3 --no-cache -o $W/before-tasks.o && nm -m $W/before-tasks.o > $W/before-tasks.nm
```

### 手順 2: `EmitOptions::external_numeric`（挙動は変えない）

- 変更: `src/llvm.rs` の `EmitOptions`・`Instrumentation`・`emit_*`（段ごとの変更の LLVM の行のうち `numeric_declarations` 以外）、
  `src/driver.rs` の `llvm::EmitOptions` の構築（`false`）、テスト 5 ファイルの構築。
- 内容: field を足して値を運ぶだけ。`emit_program` はまだ読まない。
- 確認: `cargo test --locked` が成功。手順 1 と同じコマンドの出力が `cmp` で一致。

### 手順 3: 宣言の生成と E2000

- 変更: `src/llvm.rs` の `numeric_declarations`（新規）と `emit_program` の分岐、`emit_*` の E2000、`tests/prebuilt_runtime.rs`（新規）。
- 内容: 設計のとおり。テストは「Rust テスト」の 5 つ。
- 確認: `cargo test --locked --test prebuilt_runtime` が `5 passed`。`cargo test --locked` が成功。

### 手順 4: Clang の同定と鍵

- 変更: `src/cache.rs` の `KEY_ENVIRONMENT`・`ToolIdentity`・`tool_identity`・`runtime_key`（新規）、`build_key`、`src/driver.rs` の `build_key` の呼び出し。
- 内容: `build_key` の hash にかける field の名前・順・値は HEAD と同じにする（loop の本体を移すだけ）。`src/cache.rs` にテスト
  `runtime_key_changes_with_every_input`（新規）を足す。
- 確認: `cargo test --locked runtime_key_changes_with_every_input` が `1 passed`。`cargo build --release --locked && node tests/cache.mjs target/release/tsuzuri` が成功。

### 手順 5: `runtime_object` と C ランタイム

- 変更: `src/driver.rs` の `runtime_object`（新規）と `native_runtime` の分岐。
- 内容: 「使う条件」を満たすとき `task.o` を `runtime_object` で得る。`-g` のビルドは HEAD の組み立てのまま。
- 確認: `cargo build --release --locked` の後、次で 2 つの出力が一致し、`runtime` の entry が 1 個（`native-c` の `O3`）。

```sh
rm -rf $W/c && export TSUZURI_CACHE_DIR=$W/c
target/release/tsuzuri build examples/tasks -O3 -o $W/tasks-new && $W/tasks-new > $W/new.txt
$W/tsuzuri-before build examples/tasks -O3 --no-cache -o $W/tasks-old && $W/tasks-old > $W/old.txt
cmp $W/old.txt $W/new.txt && ls $W/c/runtime | grep -cE '^[0-9a-f]{64}$'
unset TSUZURI_CACHE_DIR
```

### 手順 6: native の実行ファイルに `numeric` の object

- 変更: `src/driver.rs` の `build_complete`（`external_numeric` の値とリンク）。
- 内容: 設計の「使う条件」と「生成 IR とランタイム」。
- 確認: `TSUZURI_CACHE_DIR=$W/c` で hello を `-O0`・`-O3` でビルドして `5050` と終了コード 0、大きさ 100,824・67,256 bytes。
  手順 5 の `examples/tasks` の比較をもう一度行い一致する（`runtime` の entry は 2 個）。

### 手順 7: WASM に `numeric` の object

- 変更: `src/driver.rs` の `wasm-ld` の引数。
- 確認: `TSUZURI_CACHE_DIR=$W/c target/release/tsuzuri build $W/disp --target wasm32 -O3 -o $W/new.wasm` の後、「再現」の `node -e` が
  `19512 0 2n`。

### 手順 8: E2E

- 変更: `tests/prebuilt_runtime.mjs`（新規）。
- 確認: `node tests/prebuilt_runtime.mjs target/release/tsuzuri` が成功し、最後の行が `prebuilt runtime: ... passed`。

### 手順 9: 全体の確認

- 確認: 次がすべて成功する（重い BigInt の suite は Node 24）。

```sh
cargo test --locked && cargo build --release --locked && sh scripts/check-runtime-includes.sh
for s in e2e features primitives tasks numeric_casts display_parse cache examples io cpu_dispatch wasm_simd wasm_threads prebuilt_runtime; do
  npx --yes --package=node@24 node tests/$s.mjs target/release/tsuzuri || echo "FAILED $s"
done
npx --yes --package=node@24 node tests/math.mjs target/release/tsuzuri
```

### 手順 10: 計測の variant

- 変更: `benchmarks/run-build.mjs` の `VARIANTS`（D11）。
- 確認: `node benchmarks/run-build.mjs target/release/tsuzuri --out /tmp/tz-pb01/quick --quick` が成功し、`build.jsonl` に
  `"variant":"build-runtime-warm"` の行がある。`node tests/benchmark_report.mjs` が成功。

### 手順 11: 計測・生成コードの確認・文書

- 内容: 「計測手順」「生成コードの確認」「ドキュメント」。
- 確認: `target/perf/PB01-before`・`PB01-before2`・`PB01-after` があり、`node scripts/check-docs.mjs README.md docs/architecture.md docs/benchmarks.md` が成功。

## 計測手順

- 環境と条件は PX01・PX03 の「計測手順」に従う（電源接続、他の重い処理なし、`git status --porcelain` が空）。before のコンパイラは手順 1 の
  `$W/tsuzuri-before`、after は `cargo build --release --locked` の直後の `target/release/tsuzuri`。
- variant `build-runtime-warm`（D11）: warm-up の前に `T/cache` を消し、warm-up 1 回で whole-build と `runtime/` を作る。各標本の前に
  `T/cache` の直下の 64 桁の 16 進の entry だけを消し（`runtime/` は残す）、`tsuzuri build P -O<n> --cpu generic -o T/app-rw` を測る。
  WASM は `--target wasm32 -O<n> -o T/app-rw.wasm`。before のコンパイラでは `runtime/` がないので、毎標本ランタイムをコンパイルする。
- 標本は warm-up 1 回と 9 標本、統計は中央値・最小・最大（PX01）。生データは PX01 形式の JSON Lines で `target/perf/<run_id>/build.jsonl`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
test -z "$(git status --porcelain)" && echo clean
npx --yes --package=node@24 node benchmarks/run-build.mjs /tmp/tz-pb01/tsuzuri-before --out target/perf/PB01-before --sizes 200
npx --yes --package=node@24 node benchmarks/run-build.mjs /tmp/tz-pb01/tsuzuri-before --out target/perf/PB01-before2 --sizes 200
npx --yes --package=node@24 node benchmarks/run-build.mjs target/release/tsuzuri --out target/perf/PB01-after --sizes 200
npx --yes --package=node@24 node benchmarks/metrics.mjs report target/perf/PB01-before2 --baseline target/perf/PB01-before
npx --yes --package=node@24 node benchmarks/metrics.mjs report target/perf/PB01-after --baseline target/perf/PB01-before
```

- `before2` 対 `before` で `改善`・`悪化` が出た metric は揺れが大きいので判断に使わない。M1・M2 は `build-runtime-warm`、M4・M5 は `build`・
  `build-wasm`（`--no-cache`）の行で読む。M6 は PX01 の「before／after（他チケットの共通手順）」で native `-O3` の既存の種目を
  `--out target/perf/PB01-runtime-before`・`PB01-runtime-after` として測る。
- 完了報告には M1〜M6 の中央値と広がり、`clang --version`・`wasm-ld --version`・`node --version`・commit を書く。

## 生成コードの確認

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && W=/tmp/tz-pb01 && rm -rf $W/spy $W/c && mkdir -p $W/spy
export TSUZURI_CACHE_DIR=$W/c
SPY_DIR=$W/spy TSUZURI_CLANG=$W/clang-spy.sh target/release/tsuzuri build examples/hello -O3 -o $W/hello-new
grep -c '^define weak hidden' $W/spy/module.ll; grep -c '^declare hidden .*@tz_soft_' $W/spy/module.ll; wc -c $W/spy/module.ll
nm -m $W/c/runtime/*/artifact | grep -c 'weak private external _tz_soft_'
nm $W/hello-new | grep -c tz_soft_
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn $W/hello-new | grep -E 'bl.*<_tz_soft_format' | head -3
target/release/tsuzuri build $W/disp --emit llvm -O3 -o $W/after-disp.ll && cmp $W/before-disp.ll $W/after-disp.ll
target/release/tsuzuri build examples/tasks --emit object -O3 -o $W/after-tasks.o && nm -m $W/after-tasks.o | diff $W/before-tasks.nm -
unset TSUZURI_CACHE_DIR
```

期待: `0`・`9`・約 5,000 bytes、`9`、`9`、`bl` は `<_tz_soft_format>` への直接の呼び出し（`__stubs` を経由しない。経由したら D6 の宣言が
`hidden` になっていない）、`cmp` と `diff` は差なし。WASM は手順 7 の `node -e` で import 0 と大きさを確かめる。

## テスト計画

### Rust テスト

`tests/prebuilt_runtime.rs`（新規）。module と `sources` の作り方は `tests/cpu_dispatch.rs` を写す。ソースは「再現」の display-only に
トップレベルの `value()` を足したもの。

- `external_numeric_declares_every_weak_entry`: `external_numeric: true` の `emit_native_build` の IR が `declare hidden ` の行を 9 個持ち、
  その名前の集合が `include_str!("../src/runtime/numeric.ll")` の `define weak hidden` の名前の集合と同じ。`define weak hidden` と ` %0` を含まない。
- `external_numeric_keeps_the_trap_declaration`: IR に `declare void @llvm.trap()` が高々 1 行（落とし穴 1）。
- `inline_numeric_is_unchanged`: `external_numeric: false` の IR が `numeric.ll` の全文を含む。
- `external_numeric_is_deterministic`: 2 回の出力が同じ。
- `external_numeric_rejects_instrumentation`: `trap_info = true` または `debug = Some(..)` との組み合わせが `E2000`。

`src/cache.rs` の `runtime_key_changes_with_every_input`（新規）: 同じ入力は同じ 64 桁の 16 進、`unit`・`source`・引数 1 つ・`ToolIdentity` の
field 1 つのどれを変えても鍵が変わる。

### E2E

`tests/prebuilt_runtime.mjs`（新規）。`tests/cache.mjs` の形（`cli`・一時ディレクトリ・`TSUZURI_CACHE_DIR`・Clang の wrapper）を写す。
wrapper は起動ごとに 1 行を log に足す。Windows では `skip` を出して終わる。各ケースは自分の一時ディレクトリにソースを置く。

1. hello（`examples/hello/Main.tz` の写し）を native `O0`・`O3` でビルド: stdout `5050\n`、終了コード 0。空の cache からの `O3` は Clang 2 回、
   `sum 100 0` を `sum 101 0` に変えた再ビルドは Clang 1 回で stdout `5151\n`（1 から 101 の和）。`runtime/` の entry は 2 個（`O0`・`O3`）。
2. display-only・parse-only（`tests/display_parse.mjs` のソースにトップレベルの `value()` を足す）: native `O0`・`O3` の stdout が `2\n`・`42\n`。
   WASM `O0`・`O3` の `tz_value()` が `2n`・`42n`、`WebAssembly.Module.imports` が空。
3. `examples/tasks` の写し: `O0`・`O3` の stdout が `--no-cache` のビルドの stdout と同じ。
4. `--no-cache`: hello を空の cache で `--no-cache` ビルドすると Clang 1 回、cache のディレクトリはできない。
5. `run`（トラップ計装）: hello の `tsuzuri run` が `5050` を出し、`runtime/` に entry を作らない（D5）。
6. 破損: `runtime/<key>/artifact` の 1 byte を書き換え、whole-build miss の再ビルドが成功し正しい出力で、Clang が 2 回（作り直し）。
7. cache が使えない: `TSUZURI_CACHE_DIR` を通常ファイルにしたビルドが成功し、stderr が `cache disabled` を含み、出力が正しい。
8. 同時ビルド: 空の cache で 4 つの異なる hello の変種を同時に `O3` でビルドし、すべて正しい出力、`runtime/` の entry は 1 個。
9. `--emit llvm`: display-only の出力が cache の有無で同じで、`define weak hidden i32 @tz_soft_format(` を含む。

### 既存テストへの影響

`EmitOptions` の構築に `external_numeric: false` を足すだけ。期待値は変えない。`tests/cache.mjs` の wrapper のテストは `--emit object` で
`numeric` を使わず C ランタイムもないので、Clang の起動回数は変わらない。`tests/display_parse.mjs`・`tests/primitives.mjs` などが cache ありで
作る実行ファイル・WASM は新しい経路を通り、そのまま回帰テストになる。

### 性能

M1〜M6。合否の閾値は置かない。

## ドキュメント

- `docs/architecture.md`: 「数値ランタイムの変更時は」で始まる段落の後に、`numeric.ll` と C ランタイムの object の cache（`runtime/`、鍵、使う条件、
  `--no-cache`・`run`・`-g`・`--emit object`・Windows の扱い）を足す。「`src/runtime/numeric.c` / `numeric.ll`」の行に「実行ファイルと WASM では事前ビルドの object」。
- `README.md`: 「既定の保存先は」の文の後に、`runtime/` にランタイムの object を置くことと `--no-cache` では使わないことを足す。
- `docs/benchmarks.md`: PX03 の `## ビルド速度` に PB01 の before／after（M1・M2・M4・M5）と run_id を足す。
- `_perfs/README.md`: PB01 の状態を done にし、「現状」の 0.43 s の行に after の値を足す。

## 受け入れ条件

- [ ] 同じ組のランタイムが 2 回目以降のビルドで再コンパイルされない（E2E 1・3、M3）。
- [ ] `--no-cache`・`run`・`-g`・`--emit llvm`・`--emit object`・Windows の出力と経路が HEAD と同じ（E2E 4・5・9、「生成コードの確認」）。
- [ ] 実行結果・トラップ・確保の追跡・WASM の import が変わらない（手順 9 の全 suite、E2E 2・3）。
- [ ] 破損・使えない cache・同時ビルドでビルドが成功する（E2E 6〜8）。
- [ ] hello の M1・M2 を before／after で実測し、`docs/benchmarks.md` に記録した。M4〜M6 が変わらない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

1. `emit_program` は `numeric.ll` を連結するとき `declare void @llvm.trap()` を消す（`numeric.ll` にも同じ宣言があるため）。宣言だけにする経路では
   消さず、`numeric_declarations` に `@llvm.trap` を入れない。同じ宣言が 2 行あると Clang は `invalid redefinition of function 'llvm.trap'` で
   失敗する（2026-09-30 に確認）。
2. トラップ計装は `numeric.ll` の関数にも `%tz.context` を足す（`append_context`）。計装した IR と事前ビルドの object を混ぜると、同じ名前の
   `weak` 定義が 2 つになり、リンカーはどちらかを黙って選ぶ（引数の数が違うので未定義動作）。D5 の E2000 を外さない。
3. 実行ファイルのリンクで object を渡す前に `-x none` が要る。ないと Clang が `numeric.o` を IR として読んで失敗する。
4. `--emit object` を事前ビルドの経路にするなら、`clang -r` の `-Wl,-keep_private_externs` を保つ。macOS の `ld -r` は外すと hidden の記号を
   local にする。Phase 1 は `numeric` を `--emit object` に使わない（D3）。
5. ランタイムの object を絶対パスでコンパイルすると、ELF の `STT_FILE` などに一時ディレクトリの名前が残る。current dir を一時ディレクトリにし、
   相対名で渡す。
6. `BuildCache::load`・`store` はファイル名 `artifact`・`traps`・`dwarf` しか受け付けない。object は `artifact` として保存する。
7. ランタイムの entry を whole-build の root の直下に置かない。`tests/cache.mjs` の `entries()` が数えて期待値が変わる（停止条件 5）。
8. `--cpu native` の object を CPU の違うマシンと共有すると `SIGILL` になりうる。鍵に `native-cpu-features` を必ず入れる（`ToolIdentity`）。
9. `tool_identity` を 2 回呼ぶと `clang --version` の起動と Clang の SHA-256 が 2 回になり、M1 が数十 ms 悪化する。1 回だけ求めて渡す。
10. `math.ll` を足すと宣言の重複除去が全体の `declare` 行を名前で消す。`numeric_declarations` の名前は `math.ll` にないので影響しないが、
    `math.ll` に `tz_soft_` の宣言が増えたら確かめ直す。

## 対象外

- `math.ll`・手書きのランタイム IR の事前ビルド、`--wasm-feature threads`、Windows、`-g` のビルド、トラップ計装のある `numeric`（D5 の将来の候補）。
- 配布物への同梱（Phase 2）、未使用コード除去と表示の分割（Phase 3、PM08）。
- 鍵の計算の高速化（PB06）、利用者コードの関数単位のキャッシュ（PB07）、フロントエンドの高速化（PB02）。
- 言語間 LTO（PR08）。事前ビルドの object は LTO に参加しないので、PR08 は使わない。

## 決定事項

### D1: 事前ビルドする範囲

- 決定: Phase 1 は `numeric.ll`（unit `numeric`）と、`task.c`・`cpu.c`・`io.c` の連結ソース（unit `native-c`）だけ。他は「事前ビルドする範囲」の表のとおり IR のまま。
- 理由: 時間の 8 割が `numeric.ll`。その入口は `weak hidden` で今も inline されず、分けても最適化の機会を失わない（大きさまで一致を確認）。
  `math.ll` と手書きの IR は `internal` の小さな関数が inline されうるので、分けると `-O3` の実行時間の検証が要る。
- 状態: 既定案（実装者はこの案に従う）

### D2: object の形

- 決定: unit ごとに 1 つの object。`.a` の archive、関数ごとの翻訳単位への分割、`-ffunction-sections` は使わない。
- 理由: `numeric.c` は 9 個の入口が 12 個の `internal` の補助を共有する 1 つの生成物で、分割は `generate.py` と `FIRST_METADATA` の規則を変える。
  archive は member 単位でしか選べず、1 object では効果がない。今も全入口が実行ファイルに残る（`nm` で 9）ので、1 object でも大きさは変わらない。
  必要な関数だけのリンクは Phase 3 で section とリンカーの除去で行う（Mach-O の ld64 は atom 単位、`wasm-ld` は既定で除去）。
- 状態: 既定案（実装者はこの案に従う）

### D3: 使う条件

- 決定: 「使う条件」のコードのとおり。`numeric` は macOS・Linux の native の実行ファイルと `--emit wasm`、`native-c` は native の実行ファイルと
  `--emit object`。`-g`・`--wasm-feature threads`・Windows・`--emit llvm`・`--emit header` は HEAD の経路。
- 理由: `-g` は DWARF にパスが入り、`--emit object` の `numeric` は `clang -r` の記号の扱いを変える。Windows は COFF と MSVC の経路を検証できない（G10）。
- 状態: 既定案（実装者はこの案に従う）

### D4: 鍵と保存場所

- 決定: `default_root()` の `runtime` を root とする別の `BuildCache`。鍵は `runtime_key` が `format`（`RUNTIME_FORMAT` = 1）・`kind`
  （`runtime-object`）・`unit`・`host-os`・`host-arch`・`source`（Clang に渡すテキストの全文）・引数ごとの `arg`（入力と出力を除く）・
  `KEY_ENVIRONMENT` の値・`ToolIdentity` の field をこの順に SHA-256 にかけたもの。コンパイラの SHA-256 は入れない。
- 理由: ソースと引数の全文で、コンパイラを作り直しても object を再利用できる。Clang の同定と環境変数は whole-build と同じ規則で、SDK の違いも拾う。
- 状態: 既定案（実装者はこの案に従う）

### D5: トラップ計装との組み合わせ

- 決定: トラップ計装・debug 計装・`wasm_threads` のあるビルドでは `numeric` を IR に連結する（`run` を含む）。組み合わせは `emit_*` が `E2000` で拒む。
  `native-c` は計装の対象外なので `run` でも使う。
- 理由: `instrument` は `numeric.ll` の関数の引数と `@llvm.trap` を書き換える。
- 状態: 既定案（実装者はこの案に従う）
- 将来の候補: `numeric.ll` の関数は利用者のソースを持たず、トラップの番号が `%tz.context` だけで決まるので、計装済みの `numeric` を別の unit にできる可能性がある。計装の出力がプログラムに依存しないことを確かめてから提案する。

### D6: 宣言・linkage・可視性

- 決定: `numeric.ll` は変えず（入口は `weak hidden`）、生成 IR には `declare hidden` を入口 9 個すべてについて `numeric.ll` の順に出す。
- 理由: `hidden` なら `-fPIC` でも同じ image 内の直接呼び出しになり、記号も外へ出ない。全 9 行を出すと IR の検索の結果に依存せず決定的で、
  使わない宣言は object に記号を作らない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 取得・コンパイル・保存と同時ビルド

- 決定: 「アルゴリズム」のとおり。同時ビルドはどちらもコンパイルし、`BuildCache::store` の lock を先に取った方が公開する（待たない）。
- 理由: 既存の whole-build cache と同じ規則で、lock の待ちによる停止がない。
- 状態: 既定案（実装者はこの案に従う）

### D8: cache が使えないときと `--no-cache`

- 決定: `--no-cache` は HEAD と同じ経路で、ランタイムの cache を読みも書きもしない。`--no-cache` でないのに cache が開けない・読めない・書けないときは、
  object を一時ディレクトリでコンパイルしてリンクし（保存しない）、`runtime object cache ...` のメッセージを既存の `W2001` の経路で出す。
- 理由: `--no-cache` の意味（`build/run artifact cache reads and writes` をしない）を保つ。IR を作り直さずに済む。
- 状態: 既定案（実装者はこの案に従う）
- 注: PX03 の B1（`--no-cache` の `build`）は PB01 で変わらない。PB01 の効果は D11 の variant で測る（PX03 の表の B1 の「使うチケット」とずれる）。

### D9: Phase 2（配布物への同梱）

- 決定: 「Phase 分割」の Phase 2 の方針。着手は G14 の配布形式が決まってから。
- 理由: 配布物の置き場所・同梱する組・Clang の同定の扱いが G14 に依存し、`--no-cache` での利用という CLI の意味の変更を含む。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D10: Phase 3（必要な関数だけのリンクと `math.ll`）

- 決定: 「Phase 分割」の Phase 3 の方針。
- 理由: すべての実行ファイルのリンク（`-dead_strip`・`--gc-sections`）を変え、`math.ll` の linkage を変える。実行時間と大きさの計測が要る。
- 状態: 要承認（承認前は Phase 3 に着手しない）

### D11: 計測の variant

- 決定: PX03 の `benchmarks/run-build.mjs` の `VARIANTS` に `build-runtime-warm`（native と wasm32、`O0`・`O3`）を足す。手順は「計測手順」。
- 理由: whole-build miss でランタイムが warm な状態は、編集して再ビルドする日常の状態で、PX03 の既存の variant では作れない。
- 状態: 既定案（実装者はこの案に従う）
