# PM08: 成果物サイズの最小化

| 項目 | 内容 |
| --- | --- |
| ID | PM08 |
| 分類 | 成果物サイズ |
| 優先度 | P2 |
| 規模 | M |
| 依存 | PB01 |
| 関連 | PB03, G08, F05 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（Phase 1）。Phase 2 は要承認: D8（外部ツール `wasm-opt` の導入）。リンカーの未使用コード除去は PB01 の D10（要承認）の範囲で、PM08 は計測だけを行う（D1） |
| 手本にする既存実装 | 記号で足すランタイム IR: `src/llvm.rs` の `output.contains("@tz.display.")` → `runtime/display.ll` の分岐。手書きのランタイム IR: `src/runtime/display.ll`・`src/runtime/console.ll`。CLI の検証: `src/main.rs` の `parse` の `-O0`〜`-O3` の分岐と `src/driver.rs` の `BuildOptions::validate`（E2000）。大きさの記録: PX01 の process suite（`benchmarks/metrics.mjs`（PX01 で新規）の `executable_bytes`・`object_bytes`・`wasm_bytes`） |
| 主な影響ファイル | `src/main.rs`（`HELP`・`parse`・単体テスト）, `src/driver.rs`（`BuildOptions`・`BuildOptions::validate`・`build_complete`）, `src/test_runner.rs`（`TestOptions` は変えない。変換だけ）, `src/llvm.rs`（`console_main`・`emit_display`・ランタイム IR を足す分岐）, `src/runtime/integer.ll`（新規）, `tests/display_parse.mjs`, `tests/size.mjs`（新規）, `benchmarks/metrics.mjs`（PX01 で新規。`size` suite と 2 つの metric を足す）, `benchmarks/size/hello.c`・`benchmarks/size/hello.rs`（新規）, `docs/benchmarks.md`, `docs/architecture.md`, `_docs/tools/command-line.md`, `_docs/tools/build-and-cache.md`, `README.md`, `_perfs/README.md` の状態欄 |
| 計測対象 | native の実行ファイル（`-O0`・`-O3`・`-Os`・`-Oz`）と object（`-O3`・`-Oz`）、wasm32 の `.wasm`（`-O0`・`-O3`・`-Oz`）。種目は `examples/` のビルドできる project（「計測済みの事実」）と C・Rust の hello |

## 目的

実行ファイル・object・WASM の大きさを、C・Zig の最小構成と同等にする。
組み込み機器、ブラウザーへの配信、起動時間、命令キャッシュの効率のいずれにも大きさが効く。

Phase 1 は次の 4 つで、どれも言語の意味を変えない。実装者は Phase 1 だけを実装する。Phase 2 は人間が求めた場合だけ着手する。

1. 大きさの計測: section 単位の metric（`text_bytes`（新規）・`wasm_code_bytes`（新規））と、種目ごとの大きさの報告（PX01 形式）。
2. 整数の表示を数値ランタイム（`numeric.ll`）から分ける: 整数だけを表示するプログラムが `@tz_soft_format` を参照しないようにする（D2）。
3. サイズ優先の最適化 `-Os`・`-Oz`（D3）。
4. 実行ファイルの局所シンボルを除く `--strip`（新規、D5）。

リンカーの未使用コード除去（`-Wl,-dead_strip`・`-Wl,--gc-sections`・`-ffunction-sections`）と `math.ll` の分離は PB01 Phase 3（PB01 の D10）が持つ。
PM08 はその効果を計測して PB01 へ渡すだけで、そのリンク引数は足さない（D1）。

## 着手条件と停止条件

### 着手条件

- PB01 Phase 1（事前ビルドした `numeric` の object）が `_perfs/README.md` の状態欄で done であること。確認: `grep -n "PB01\|PX01\|PM08" _perfs/README.md`。
  PB01 が `@tz_soft_` を参照する IR の扱い（`numeric.ll` を IR に足すか object をリンクするか）を変えるので、整数の表示の分離（D2）はその後に行う。
- PX01 Phase 1（`benchmarks/metrics.mjs` と process suite）が done であること。メタデータの依存欄には PX01 がないが、計測（手順 9〜11）は PX01 形式で記録するので開始条件にする。
  PX01 が未完了なら手順 1〜8 だけを行い、計測は「再現」のコマンドで before／after を手で記録して報告する。
- GUIDE §2.3 の基準コマンドが成功すること。GUIDE §14（性能チケットの共通手順）に従う。

### 前提とする他チケットのインターフェース

- PX01: `benchmarks/metrics.mjs` の `METRICS`（metric 名 → 単位）、`makeRecord`・`writeRecords`・`readRecords`・`compareRuns`、
  CLI `node benchmarks/metrics.mjs <suite> ... --out <dir>`、保存先 `target/perf/<run_id>/<suite>.jsonl`、`validateRecord` の規則 7（`*_bytes` は全標本が等しい）。
  PM08 は `METRICS` に `text_bytes: "bytes"` と `wasm_code_bytes: "bytes"` を足し、suite `size`（新規）を足す。どちらも規則 7 の `*_bytes` に入る。
- PB01 Phase 1: `@tz_soft_*` の定義は `numeric` の object（または IR）にあり、IR が `@tz_soft_` を含まなければ `numeric` は成果物に入らない。
  PM08 はこの「含まなければ入らない」だけに依存する。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `@tz.integer.format`（新規）の出力が、いずれかの整数型・値で `tz_soft_format` の出力と 1 文字でも違う。
- `@tz.integer.format` を含む wasm32 のビルドで未定義記号（`__umodti3` など）のリンクエラーが出る、または WASM の import が空でなくなる。
- `-Os`・`-Oz` のどちらかで既存の E2E（数値・トラップ・所有権）が失敗する。fast-math などの指定で回避しない。
- `-Os`・`-Oz` の効果を出すために IR の関数へ `optsize`・`minsize` 属性を付ける必要があると判断した（D3 の範囲外）。
- 同じ入力の 2 回の `--no-cache` ビルドの成果物が `cmp` で異なる（非決定性）。
- `--strip` の実行ファイルが `codesign -v`（macOS）で失敗する、または実行結果が変わる。
- `-Wl,-dead_strip`・`-Wl,--gc-sections`・`-ffunction-sections` を足したくなった（PB01 の D10 の範囲）。
- `wasm-opt`・`bloaty` などの新しい外部ツールが必要になった（D8）。
- 既存テストの期待値（IR の文字列、E2000 のメッセージ、E2E の出力）を変える必要がある。次の 2 つだけは除く（「既存テストへの影響」）:
  `src/main.rs` の単体テストの `options.optimization` の比較を `OptLevel`（新規）へ書き換えること、`tests/display_parse.mjs` の `display-only` の例
  （`to_string 42` の IR が `call i32 @tz_soft_format` を含むことを確かめる）を浮動小数点の値へ移すこと（D2 の直接の結果）。

## 現状と計測（HEAD `f8dc655`）

### ランタイムが成果物に入る経路

- `src/llvm.rs` は生成した IR の文字列を検索してランタイム IR を足す。`output.contains("@tz_soft_")` なら `runtime/numeric.ll`（11,136 行）、
  `@tz_math_` なら `runtime/math.ll`、`@tz.display.` なら `runtime/display.ll`。wasm32 では `src/driver.rs` の `build_complete` が常に `runtime/wasm.ll`
  （128 ビット演算の helper。`__udivti3` を `define weak hidden` で持つ）を足す。
- `console_main`（`src/llvm.rs`）は `main` の結果が数値型なら `call i32 @tz_soft_format(ptr %buffer, ptr %slot, i32 <numeric_kind>)` を出す。
  `emit_display`（`Builtin::Display`・`Builtin::ToString`）も、`Char`・`Utf8Char`・`String`・`Utf8String`・`Bool`・`Unit` 以外（整数と浮動小数点）で同じ呼び出しを出す。
  `numeric_kind` は整数に `16 + log2(bytes) + (unsigned なら 8)` を返す。
- `tz_soft_format`（`src/runtime/numeric.c`）は `format(kind)` の `integer` なら 25 行ほどの 10 進変換（`u128` の 100 ごとの割り算）で終わり、それ以外は
  `format_float`（`noinline`）へ進む。整数を 1 つ表示するだけで、浮動小数点・decimal の表示とランタイム全体が成果物に入る。
- `numeric.ll` の関数は `define weak hidden`（`src/runtime/generate.py` が `define hidden ` を置き換える）。外部から見える定義なので、LLVM は `-O3` でも
  使われない関数を消せない。起票時の「`-O3` では未使用の internal 関数が除かれる」は誤りで、hello の `-O3` の実行ファイルにも `_tz_soft_op`・`_tz_soft_parse`・
  `_tz_soft_math_unary`・`_tz_soft_math_binary`・`_tz_soft_hash_canonical`・`_tz_soft_cast`・`_tz_soft_cmp`・`_tz_soft_fma`・`_tz_soft_format` が局所シンボルとして残る（`nm`）。

### リンクとシンボル

- native の実行ファイル（`build_complete`）: `clang -x ir -Wno-override-module -O<n>` に `native_compile_args`（Windows 以外は `-fPIC`）、`--cpu native` なら
  `native_cpu_flag`、`-lm`、C ランタイム（`task.c`・`cpu.c`・`io.c` を連結して `-std=c11 -c -O<n>` で作った object）を足して 1 回で作る。
  `-dead_strip`・`--gc-sections`・`-Wl,-x`・`strip` はどこにもない。`nm` で hello の `-O3` は 27 行、`-O0` は 41 行のシンボルがある。
- macOS の `-g` の実行ファイル: object → `clang ... -g -lm` でリンク → `dsymutil --flat` で DWARF sidecar。
- wasm32: `wasm-ld --no-entry --stack-first -z stack-size=1048576 --max-memory=16777216`。`debug_info` でなければ `--strip-all`（名前 section も除く）。
  `wasm-ld` は既定で未使用の section を除く。`--trap-info` は `--export=tsuzuri_trap_site` と `<output>.trap.json`。
- 最適化の指定: `parse`（`src/main.rs`）は `-O0`〜`-O3` だけを受け、`BuildOptions::optimization: u8` に入れる。`BuildOptions::validate` は 3 より大きい値を
  E2000 `optimization level must be 0, 1, 2, or 3` にする。`build_complete` は `format!("-O{}", options.optimization)` を C ランタイム・WASM スレッドのランタイム・
  IR の 3 か所で渡し、`options.optimization != 0` を DWARF の最適化フラグに使う。`TestOptions::optimization: u8`（`src/test_runner.rs`）は別に持つ。
- cache: `build_key`（`src/cache.rs`）は `format!("{options:?}")` を鍵に入れる。`BuildOptions` に足した欄は自動で鍵に入る。
- 決定性: hello の `-O3` を `--no-cache` で 2 回ビルドした実行ファイルは `cmp` で一致した。

### 計測済みの事実

2026-09-30、M1 Max、macOS 27.0、Apple clang 21.0.0（`/usr/bin/clang`）、HEAD `f8dc655`。`file` は `stat -f %z`、`__text` は `size -m` の `Section __text`。
`-Os`・`-Oz`・`-dead_strip`・`-Wl,-x` の行は「再現」の Clang の wrapper で引数を差し替えた計測で、コンパイラはまだその指定を持たない。

| 種目 | 指定 | file（bytes） | `__text`（bytes） |
| --- | --- | --- | --- |
| hello | `-O0` | 100,824 | 72,288 |
| hello | `-O0` + `-Wl,-dead_strip` | 50,360 | 20,532 |
| hello | `-O0` + `-Wl,-x` | 99,768 | 72,288 |
| hello | `-O3` | 67,256 | 33,184 |
| hello | `-Os` | 67,288 | 32,148 |
| hello | `-Oz` | 50,776 | 30,316 |
| hello | `-O3` + `-Wl,-dead_strip` | 33,704 | 10,048 |
| hello | `-Oz` + `-Wl,-dead_strip` | 33,704 | 8,172 |
| hello | `-O3` + `-Wl,-x` | 66,664 | 33,184 |
| io | `-O3` | 35,192 | 6,280 |
| io | `-Os` | 35,528 | 7,000 |
| io | `-Oz` | 35,752 | 5,628 |
| io | `-O3` + `-Wl,-x` | 34,152 | 6,280 |
| tasks | `-O3` | 85,048 | 35,408 |
| tasks | `-Oz` | 85,304 | 32,224 |
| tasks | `-O3` + `-Wl,-dead_strip` | 51,496 | 12,260 |
| tasks | `-O3` + `-Wl,-x` | 83,928 | 35,408 |

- 読み方: macOS arm64 の実行ファイルは segment が 16 KiB 単位なので、file の大きさは `__text` が減っても増えることがある（io の `-Oz`）。比較には `__text` を使う。
- `-dead_strip` だけで hello の `-O3` は 33,704 bytes になり、C の hello（Clang `-O3`、33,424 bytes。起票時の値）とほぼ同じになる。残る `tz_soft` の記号は
  `_tz_soft_format` だけ。これは PB01 Phase 3 の範囲（D1）。
- io（数値を表示しない）は `numeric` を含まず 35,192 bytes。整数の表示を分ければ（D2）、整数だけを表示するプログラムは `-dead_strip` なしでもこの水準に近づく見込み（未計測）。
- `-Wl,-x` は 592〜1,120 bytes（約 1〜3%）減らす。リンク後の `strip` もほぼ同じ（hello の `-O0` 99,752、`-O3` 66,648）。
- 他の example の native `-O3`: computations・control・currying・polymorphism 67,208、point 67,272、functional 67,624。desktop・gpu・native・web は
  `E2001`（この形ではビルドできない。計測対象外）。
- wasm32: hello は `-O3` 59 bytes、`-O0` 208 bytes（CODE section 0x81 = 129 bytes）。`-O3` で point 74、functional 5,778、io 22,327。
  computations・control・currying・polymorphism・tasks は `E2004`（export がない）。
- 起票時の記録（再計測していない）: 合成 1,000 モジュールの `-O0` の実行ファイルは 7.1 MB。
- 参照言語: `rustc` は `/opt/homebrew/bin/rustc` にある。`zig`・`wasm-opt`・`bloaty` はない。`/opt/homebrew/opt/llvm@21/bin` に `llvm-size`・`llvm-objdump`・`llvm-nm`・`llvm-strip` がある。

### 再現（2026-09-30 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
W=/tmp/tz-work-PM08 && mkdir -p $W
for o in 0 3; do
  target/release/tsuzuri build examples/hello --no-cache -O$o -o $W/hello-O$o
  stat -f "%z" $W/hello-O$o
  size -m $W/hello-O$o | grep "Section __text"
  nm $W/hello-O$o | grep -c tz_soft
  target/release/tsuzuri build examples/hello --no-cache --target wasm32 -O$o -o $W/hello-O$o.wasm
  stat -f "%z" $W/hello-O$o.wasm
done
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -h $W/hello-O0.wasm
```

Clang の引数を差し替える wrapper（計測専用。`TZ_OPT` は `-O<n>` の置き換え、`TZ_EXTRA` は末尾に足す引数）:

```sh
cat > $W/clang-wrap <<'EOF'
#!/bin/bash
args=()
for a in "$@"; do if [[ -n "$TZ_OPT" && "$a" == -O[0-3] ]]; then args+=("$TZ_OPT"); else args+=("$a"); fi; done
exec /usr/bin/clang "${args[@]}" $TZ_EXTRA
EOF
chmod +x $W/clang-wrap
TZ_OPT=-Oz TZ_EXTRA="-Wl,-dead_strip" TSUZURI_CLANG=$W/clang-wrap \
  target/release/tsuzuri build examples/hello --no-cache -O3 -o $W/hello-Oz-ds
size -m $W/hello-Oz-ds | grep "Section __text"
```

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する（GUIDE §14）。

- G1: hello の実行ファイルを C の hello と同等（macOS arm64 で 34 KB 前後）にする。Phase 1 の範囲では、整数だけを表示するプログラム
  （hello・point・computations・control・currying・polymorphism）の native の成果物に `numeric` が入らない（`nm` に `tz_soft` がない）ことで近づける。
  残りの差は PB01 Phase 3 の `-dead_strip` が埋める（計測済み: 33,704 bytes）。
- G2: `-Oz` の `text_bytes` が `-O3` 以下。wrapper の計測では hello −8.6%、io −10.4%、tasks −9.0%。
- G3: `--strip` が局所シンボルを除き、実行結果を変えない（計測済み: −592〜−1,120 bytes）。
- G4: 種目ごとの大きさが PX01 形式で記録され、C・Rust の hello と並べて読める。代表的な種目で Rust（`opt-level = "z"`）・Zig（`ReleaseSmall`）と同等以下を長期目標にする（Zig はこの計測機にない）。
- G5: 既定の指定（`-O3`、`--strip` なし）では、整数を表示しないプログラムの IR と成果物がバイト単位で変わらない。

| 指標 | 単位・統計 | 取り方 | 対象 |
| --- | --- | --- | --- |
| `executable_bytes` | bytes。1 標本（2 回のビルドの `cmp` 一致を確認） | `statSync(path).size` | native の実行ファイル |
| `text_bytes`（新規） | bytes。同上 | `llvm-objdump -h` の行のうち名前が `__text`（Mach-O）または `.text`（ELF）の Size（16 進） | native の実行ファイル |
| `object_bytes` | bytes。同上 | `statSync(path).size` | native の `--emit object` |
| `wasm_bytes` | bytes。同上 | `statSync(path).size` | wasm32 の `.wasm` |
| `wasm_code_bytes`（新規） | bytes。同上 | `llvm-objdump -h` の `CODE` 行の Size（16 進） | wasm32 の `.wasm` |

- `llvm-objdump` は `TSUZURI_OBJDUMP`（`tests/wasm_simd.mjs` と同じ変数。既定 `llvm-objdump`）で選ぶ。
- data の section は metric にしない。hello の `__DATA_CONST` は 32〜40 bytes で、file の大きさは segment 単位（16 KiB）でしか動かない。

## 変えてはいけない意味

- 整数の表示: `i8`〜`i128`・`i8u`〜`i128u` の全値で、`@tz.integer.format` の出力は `tz_soft_format` と同じ ASCII（負なら先頭に `-` を 1 つ、先頭の 0 なし、0 は `0`）。
  最小値（`-128`、`-170141183460469231731687303715884105728`）と最大値を含む。`Display`・`to_string`・`main` の結果の表示のすべてで同じ。
- `-Os`・`-Oz`: 整数の折り返し・飽和・溢れのトラップ、丸め、NaN、符号付きゼロ、評価順序、トラップ、所有権・借用、`live == 0` は `-O3` と同じ。
  `-ffast-math`・`-fno-trapping-math`・`-ffp-contract=fast` などを足さない。Clang に渡すのは `-Os`・`-Oz` だけ。
- IR: `--emit llvm` の出力は `-O1`〜`-O3`・`-Os`・`-Oz` で同じ（`-g` の DWARF の最適化フラグも同じ true）。`-O0` と `-O1` 以上の差も今と同じ。
- `--strip`: 外部シンボル（`_main`、`tz_` の export、未定義記号）は残す。`<output>.trap.json`（trap site の id で引く）は変わらない。`-g` とは併用できない（E2000）。
- WASM: import は空のまま（D-18）。`debug_info` でない WASM の `--strip-all` は今と同じ。
- 決定性: 同じ入力・同じ指定の 2 回の `--no-cache` ビルドはバイト単位で一致する。新しいランタイム IR は固定の文字列で、足す位置も固定。
- cache: `-Os`・`-Oz`・`--strip` は `build_key` の `options` に入り、他の指定の成果物と混ざらない。
- 既定の最適化は `-O3` のまま（D4）。

## 設計

### 分担（PB01 との境界）

| 施策 | 担当 | PM08 での扱い |
| --- | --- | --- |
| 大きさの metric と種目ごとの報告 | PM08 | 実装（D6） |
| 整数の表示の分離（`@tz.integer.format`） | PM08 | 実装（D2） |
| `-Os`・`-Oz` | PM08 | 実装（D3・D7） |
| `--strip`（`-Wl,-x`） | PM08 | 実装（D5） |
| `-ffunction-sections -fdata-sections`・`-Wl,-dead_strip`・`-Wl,--gc-sections` | PB01 Phase 3（PB01 の D10） | 計測だけ。wrapper で効果を記録し PB01 へ報告（D1） |
| `math.ll` の分離、使う記号の集合による判断 | PB01 Phase 3 | 対象外 |
| `tz_soft_format` の浮動小数点・decimal ごとの分割 | PM08 Phase 2 | PB01 Phase 3 の後に計測で判断（D9） |
| `wasm-opt` | PM08 Phase 2 | 要承認（D8） |

### データ構造

```rust
// src/driver.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptLevel { O0, O1, O2, O3, Os, Oz }            // 新規

impl OptLevel {
    pub fn flag(self) -> &'static str {                   // 新規。"-O0" ... "-O3", "-Os", "-Oz"
        match self { Self::O0 => "-O0", Self::O1 => "-O1", Self::O2 => "-O2", Self::O3 => "-O3", Self::Os => "-Os", Self::Oz => "-Oz" }
    }
}

pub struct BuildOptions {
    // 既存の欄はそのまま
    pub optimization: OptLevel, // u8 から変更。Default は OptLevel::O3
    pub strip: bool,            // 新規。Default は false
}
```

- `TestOptions::optimization: u8` は変えない。`src/main.rs` で `TestOptions` を作る箇所（`optimization: arguments.options.optimization`）は
  `O0`〜`O3` を 0〜3 に変換する `match` にし、`Os`・`Oz` は `unreachable!("parse rejects -Os and -Oz for test")`（`parse` が E2000 で先に拒否する）。

### CLI と診断

- `HELP` の行: `-O0, -O1, -O2, -O3, -Os, -Oz  LLVM optimization level (default: -O3; -Os/-Oz favor size; no fast-math)` と
  `--strip  Remove local symbols from native executables (build only)`。`tsuzuri run` の書式行にも `-Os|-Oz` を足す。
- `-Os`・`-Oz` は `-O0`〜`-O3` と同じ分岐で読み、同じ重複検査（`optimization specified more than once`）と同じ action 制限（check・fmt・doc・WGSL は既存の文で拒否）に従う。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E2000 | `tsuzuri test` に `-Os`・`-Oz` | `test supports -O0 to -O3; use -Os or -Oz with build or run` | CLI（`parse` の `Err`） |
| E2000 | `--strip` を 2 回 | `--strip specified more than once` | CLI |
| E2000 | `build` 以外の action に `--strip` | `--strip is only valid with build` | CLI |
| E2000 | `--strip` と `-g`／`--debug-info` | `--strip cannot be combined with --debug-info; remove one of them` | `BuildOptions::validate` |
| E2000 | `--strip` で `--emit exe` 以外（object・llvm・header・wasm・wgsl） | `--strip requires native executable output; WASM output is already stripped unless --debug-info is used` | `BuildOptions::validate` |

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | `HELP` | 上の 2 行と `run` の書式 |
| CLI | `src/main.rs` | `parse` | `Some("-O0" \| "-O1" \| "-O2" \| "-O3" \| "-Os" \| "-Oz")` から `OptLevel`。`--strip` の読み取りと action の検査。`test` の `-Os`・`-Oz` の拒否 |
| CLI | `src/main.rs` | `TestOptions` を作る箇所、単体テスト | 変換の `match`。`assert_eq!(..., 0)` を `OptLevel::O0` へ |
| driver | `src/driver.rs` | `OptLevel`（新規）、`BuildOptions`、`Default for BuildOptions` | 上のとおり |
| driver | `src/driver.rs` | `BuildOptions::validate` | `self.optimization > 3` の検査を消す（型で表せない）。`--strip` の 2 つの検査 |
| driver | `src/driver.rs` | `build_complete` | `format!("-O{}", options.optimization)` の 3 か所を `options.optimization.flag()`、`options.optimization != 0` の 3 か所を `!= OptLevel::O0`。IR を実行ファイルにする `clang`（`-lm` を足す箇所と同じ条件）に、`options.strip && !cfg!(windows)` なら `-Wl,-x` |
| cache | `src/cache.rs` | `build_key` | 変更なし（`{options:?}` が新しい欄を含む） |
| IR | `src/llvm.rs` | `console_main` | `_ if ty.is_numeric()` の前に `Type::Integer(bits, signed)` の arm |
| IR | `src/llvm.rs` | `emit_display` | `_ =>` の前に `Type::Integer(bits, signed)` の arm（`borrowed` なら先に `load`） |
| IR | `src/llvm.rs` | `emit_program` のランタイム IR を足す分岐 | `@tz.display.` の分岐の直後に `if output.contains("@tz.integer.format") { output.push_str(include_str!("runtime/integer.ll")); }` |
| runtime | `src/runtime/integer.ll`（新規） | `@tz.integer.format` | 下の IR。metadata を持たない（`FIRST_METADATA` に影響しない） |
| 計測 | `benchmarks/metrics.mjs` | `METRICS`、CLI | `text_bytes`・`wasm_code_bytes`、suite `size`（計測手順） |
| 計測 | `benchmarks/size/hello.c`・`hello.rs`（新規） | 参照の hello | `5050` を表示する（C は `printf("%d\n", 5050)`、Rust は `println!("{}", 5050)`） |

### 生成 IR とランタイム

呼び出し側（`emit_display` の `ToString`、`i64` の例）。`signed` の型は `sext`、unsigned は `zext`、128 ビットは拡張しない。buffer は 40 bytes
（`i128u` の最大値は 39 桁、`i128` の最小値は `-` と 39 桁）。`console_main` も同じ形で、`@tz.string.from_ascii` の代わりに `@tz.console.write` を呼ぶ。

```llvm
  %wide = sext i64 %x to i128
  %buffer = alloca [40 x i8], align 1
  %count = call i32 @tz.integer.format(ptr %buffer, i128 %wide, i1 true)
  %length = zext i32 %count to i64
  %r = call %tz.string @tz.string.from_ascii(ptr %buffer, i64 %length)
  ret %tz.string %r
```

`src/runtime/integer.ll`（新規の IR。実装後に有効。未検証）。10 進の桁を逆順に `%digits` へ書き、符号と正順の桁を `%out` へ写す。
2^64 以上の間だけ `udiv i128`（native は compiler-rt／libgcc、wasm32 は `runtime/wasm.ll` の `__udivti3`・`__umodti3`）、以降は `udiv i64`（`-O1` 以上で乗算になる）。

```llvm
define internal i32 @tz.integer.format(ptr %out, i128 %value, i1 %signed) nounwind {
entry:
  %digits = alloca [40 x i8], align 1
  %below = icmp slt i128 %value, 0
  %negative = and i1 %signed, %below
  %negated = sub i128 0, %value
  %magnitude = select i1 %negative, i128 %negated, i128 %value
  br label %wide
wide:
  %w = phi i128 [ %magnitude, %entry ], [ %w.next, %wide.body ]
  %wc = phi i32 [ 0, %entry ], [ %wc.next, %wide.body ]
  %big = icmp ugt i128 %w, 18446744073709551615
  br i1 %big, label %wide.body, label %narrow.entry
wide.body:
  %w.next = udiv i128 %w, 10
  %w.times = mul i128 %w.next, 10
  %w.digit = sub i128 %w, %w.times
  %w.byte = trunc i128 %w.digit to i8
  %w.char = add i8 %w.byte, 48
  %w.slot = getelementptr inbounds [40 x i8], ptr %digits, i32 0, i32 %wc
  store i8 %w.char, ptr %w.slot
  %wc.next = add i32 %wc, 1
  br label %wide
narrow.entry:
  %n0 = trunc i128 %w to i64
  br label %narrow
narrow:
  %n = phi i64 [ %n0, %narrow.entry ], [ %n.next, %narrow ]
  %nc = phi i32 [ %wc, %narrow.entry ], [ %nc.next, %narrow ]
  %n.next = udiv i64 %n, 10
  %n.times = mul i64 %n.next, 10
  %n.digit = sub i64 %n, %n.times
  %n.byte = trunc i64 %n.digit to i8
  %n.char = add i8 %n.byte, 48
  %n.slot = getelementptr inbounds [40 x i8], ptr %digits, i32 0, i32 %nc
  store i8 %n.char, ptr %n.slot
  %nc.next = add i32 %nc, 1
  %more = icmp ne i64 %n.next, 0
  br i1 %more, label %narrow, label %sign
sign:
  %start = zext i1 %negative to i32
  br i1 %negative, label %minus, label %copy
minus:
  store i8 45, ptr %out
  br label %copy
copy:
  %i = phi i32 [ 0, %sign ], [ 0, %minus ], [ %i.next, %copy ]
  %from.end = sub i32 %nc.next, %i
  %from.index = sub i32 %from.end, 1
  %from = getelementptr inbounds [40 x i8], ptr %digits, i32 0, i32 %from.index
  %char = load i8, ptr %from
  %to.index = add i32 %start, %i
  %to = getelementptr inbounds i8, ptr %out, i32 %to.index
  store i8 %char, ptr %to
  %i.next = add i32 %i, 1
  %done = icmp eq i32 %i.next, %nc.next
  br i1 %done, label %exit, label %copy
exit:
  %length = add i32 %start, %nc.next
  ret i32 %length
}
```

- `narrow` は do-while なので 0 は `0` になる。i128 の最小値は `sub i128 0, %value` が同じ値に戻り、unsigned として 2^127 になるので正しい。
- 整数を表示する IR は `@tz_soft_` を含まなくなり、浮動小数点・decimal を使わないプログラムには `numeric` が入らない。`tz_soft_format` と `numeric_kind` は変えない。

### Phase 分割

- Phase 1（このチケット）: 分担の表の「実装」の行すべて。native（macOS・Linux）と wasm32。
- Phase 2（人間が求めた場合だけ）: D8（`wasm-opt`、要承認）、D9（`tz_soft_format` の分割、PB01 Phase 3 の後）、`optsize`・`minsize` 属性の付与（D3 の計測で `-Oz` の効果が足りない場合）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N が 0 でないことを見る（GUIDE §3.1）。
以下 `W=/tmp/tz-work-PM08`、`L=/opt/homebrew/opt/llvm@21/bin`。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」のコマンドと wrapper を実行し、`$W/before-hello.ll`（`build examples/hello --emit llvm -O3`）と
  `$W/before-hello`（`--no-cache -O3` の実行ファイル）を保存する。
- 確認: `cargo test --locked` が成功する。M1 Max では hello の `-O3` が 67,256 bytes・`__text` 33,184 bytes。別の機械なら値を記録して続ける。

### 手順 2: size suite（計測だけ。コンパイラは変えない）

- 変更: `benchmarks/metrics.mjs`（`METRICS` に `text_bytes`・`wasm_code_bytes`、`opt` の許す値に `Os`・`Oz`、suite `size` と CLI）、
  `benchmarks/size/hello.c`・`benchmarks/size/hello.rs`（新規）、PX01 の JavaScript テスト（`tests/benchmark_report.mjs`）。
- 内容: 「計測手順」の size suite。`sectionBytes(file, name)`（新規）は `llvm-objdump -h` の各行を `^\s*\d+\s+(\S+)\s+([0-9a-f]+)\s` で読み、
  名前が一致した行の Size を `parseInt(size, 16)` で返す。見つからなければ `section <name> not found in <file>` で throw。
- 確認: `node tests/benchmark_report.mjs` が成功する（追加: `__text 000081a0` の行から 33,184、`CODE 00000081` から 129、行がなければ throw）。
  `node benchmarks/metrics.mjs size target/release/tsuzuri --out target/perf/PM08-before` が `size.jsonl` を書き、hello の `O3`・`default` の
  `executable_bytes` と `text_bytes` が手順 1 の値と等しい。このコンパイラには `-Oz` がないので `Os`・`Oz`・`strip` の行は `skip` になる。

### 手順 3: `OptLevel`（挙動は変えない）

- 変更: `src/driver.rs` の `OptLevel`・`BuildOptions`・`Default for BuildOptions`・`BuildOptions::validate`・`build_complete`、`src/main.rs` の `parse`
  （まだ `-O0`〜`-O3` だけ）・`TestOptions` を作る箇所・単体テスト。
- 内容: 「段ごとの変更」の driver と CLI の行のうち `-Os`・`-Oz`・`--strip` 以外。
- 確認: `cargo test --locked` が成功する。`cargo build --release --locked` の後、`build examples/hello --emit llvm -O3 -o $W/step3.ll` が
  `cmp $W/before-hello.ll $W/step3.ll` で一致し、`--no-cache -O3` の実行ファイルも `$W/before-hello` と `cmp` で一致する。

### 手順 4: `-Os`・`-Oz`

- 変更: `src/main.rs` の `HELP`・`parse`・単体テスト。
- 内容: `parse` の分岐に `"-Os" | "-Oz"`。`action == Action::Test` で `Os`・`Oz` なら E2000（診断の表）。
- 確認: `cargo test --locked` が成功する。`target/release/tsuzuri run examples/hello -Oz` が `5050`。`build examples/hello --no-cache -Oz` の
  `__text` が M1 Max で 30,316（wrapper の計測と同じ）。`--emit llvm -Oz` の IR が `$W/step3.ll` と `cmp` で一致。
  `target/release/tsuzuri test examples/hello -Oz` が `error[E2000]` と `test supports -O0 to -O3`。

### 手順 5: `--strip`

- 変更: `src/main.rs`（`HELP`・`parse`）、`src/driver.rs`（`BuildOptions::strip`・`validate`・`build_complete`）、単体テスト。
- 内容: 診断の表の 4 行。`-Wl,-x` は IR を実行ファイルにする `clang` だけに足す（`-r` のリンクと `dwarf_sidecar` のリンクには足さない）。
- 確認: `build examples/hello --no-cache -O3 --strip -o $W/strip` が M1 Max で 66,664 bytes、`$W/strip` が `5050`、`nm $W/strip | grep -c " t "` が 0、
  `codesign -v $W/strip` が成功。`build examples/hello --strip -g`・`--strip --target wasm32`・`run ... --strip` が E2000。

### 手順 6: `integer.ll` と足す分岐（まだ使わない）

- 変更: `src/runtime/integer.ll`（新規）、`src/llvm.rs` の `emit_program`。
- 内容: 「生成 IR とランタイム」の IR と分岐。新しいファイルを `git add` する（`scripts/check-runtime-includes.sh` は追跡されたファイルだけを許す）。
- 確認: `sh scripts/check-runtime-includes.sh` が `runtime includes are tracked:` で始まる行を出す。hello の IR はまだ `$W/step3.ll` と一致する。

### 手順 7: 整数の表示を切り替える

- 変更: `src/llvm.rs` の `console_main`・`emit_display`、`tests/display_parse.mjs`。
- 内容: 「段ごとの変更」の IR の行。`display_parse.mjs` の `display-only` を `(to_string 4.5).length`（期待 3、symbol `format`。WASM で 3 を返すことを確認済み）にし、
  `display-integer`（`(to_string 42).length`、期待 2）を足して IR が `call i32 @tz.integer.format` を含み `@tz_soft_` を含まないことを確かめる。
- 確認: `cargo test --locked` が成功する。`node tests/display_parse.mjs target/release/tsuzuri` と `node tests/examples.mjs target/release/tsuzuri` が成功する。
  hello の `-O0`・`-O3` の実行ファイルで `nm <exe> | grep -c tz_soft` が 0。

### 手順 8: テストを足す

- 変更: `src/main.rs`・`src/llvm.rs` の単体テスト、`tests/size.mjs`（新規）、`README.md` のテストの一覧。
- 内容: 「テスト計画」のとおり。
- 確認: `cargo test --locked` が成功する。`node tests/size.mjs target/release/tsuzuri` が最後に `Size:` で始まる行を出す。

### 手順 9: 全体の確認と `-Oz` での既存 E2E

- 変更: なし。
- 内容: README の検証コマンドをすべて実行する（BigInt の重い suite は Node 24）。続けて「再現」の wrapper で全 Clang 呼び出しを `-Oz` にして、
  数値・トラップ・所有権の suite を実行する（テストファイルの `-O0`・`-O3` がすべて `-Oz` になる）。
- 確認: すべて成功する。

```sh
for s in e2e display_parse numeric_casts integer_intrinsics primitives strings tasks control computations io math; do
  TZ_OPT=-Oz TSUZURI_CLANG=$W/clang-wrap npx --yes --package=node@24 node tests/$s.mjs target/release/tsuzuri || echo "FAILED $s"
done
```

### 手順 10: 生成コードの確認

- 変更: なし。内容: 「生成コードの確認」を実行する。
- 確認: そこに書いた期待どおり。`___udivti3` の観察結果を報告に書く。

### 手順 11: 計測と記録

- 変更: なし。内容: 「計測手順」の after と比較、dead_strip の probe。
- 確認: `report --baseline` で hello・point・computations・control・currying・polymorphism の `text_bytes` が減り、io・tasks・functional は「差なし」。

### 手順 12: 文書

- 変更: 「ドキュメント」のファイル。
- 確認: `node scripts/check-docs.mjs _docs/tools/command-line.md _docs/tools/build-and-cache.md` と `git diff --check` が成功する。

## 計測手順

### 環境

PX01 の `captureRun` が commit・CPU・OS・Clang・rustc・Node・Tsuzuri の SHA-256 を記録する。手でも次を保存する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
(git rev-parse HEAD; sw_vers; sysctl -n machdep.cpu.brand_string; clang --version | head -1; rustc --version; node --version) > target/perf/PM08-env.txt
```

### size suite（`node benchmarks/metrics.mjs size`）

```text
node benchmarks/metrics.mjs size [compiler] --out <dir> [--project <dir>]...
```

- 既定の project: `examples/` の hello・io・point・functional・computations・control・currying・polymorphism・tasks。`workload` はディレクトリ名。
- 出力は `mkdtempSync` の一時ディレクトリ。各ビルドは `--no-cache` で 2 回行い、成果物がバイト単位で等しいことを確かめてから 1 標本を記録する
  （違えば `nondeterministic build: <path>` で throw。停止条件）。大きさは決定的なので warm-up と 9 標本は要らない（PX01 の規則 7）。
- 1 回目が失敗した組（`E2004`・`E2001`）は stderr に `skip <P> <variant> <opt> <target>: <1 行目>` を出して続ける。
  `tsuzuri --help` が `-Oz` を含まなければ `Os`・`Oz`・`strip` の組を skip する（before の計測）。

| variant | コマンド | target | opt | metric |
| --- | --- | --- | --- | --- |
| `default` | `tsuzuri build P -O<l> --cpu generic --no-cache -o T/app`（l = 0, 3, s, z） | native | `O0`・`O3`・`Os`・`Oz` | `executable_bytes`・`text_bytes` |
| `strip` | `tsuzuri build P -O3 --strip --cpu generic --no-cache -o T/app` | native | `O3` | `executable_bytes`・`text_bytes` |
| `default` | `tsuzuri build P --emit object -O<l> --cpu generic --no-cache -o T/app.o`（l = 3, z） | native | `O3`・`Oz` | `object_bytes` |
| `default` | `tsuzuri build P --target wasm32 -O<l> --no-cache -o T/app.wasm`（l = 0, 3, z） | wasm32 | `O0`・`O3`・`Oz` | `wasm_bytes`・`wasm_code_bytes` |
| `default`（language `c`） | `$TSUZURI_CLANG -O<l> benchmarks/size/hello.c -o T/c`（l = 3, z） | native | `O3`・`Oz` | `executable_bytes`・`text_bytes` |
| `default`（language `rust`） | `rustc -C opt-level=<l> benchmarks/size/hello.rs -o T/rust`（l = 3, z） | native | `O3`・`Oz` | `executable_bytes`・`text_bytes` |

- C・Rust の `workload` は `hello`。`rustc` がなければ Rust の行を skip する。Zig は計測機にないので Phase 1 では記録しない。
- `text_bytes` は native で `__text`（macOS）か `.text`（Linux）、`wasm_code_bytes` は `CODE`。

### before／after

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
# 手順 2 の後（コンパイラは変更前）
node benchmarks/metrics.mjs size target/release/tsuzuri --out target/perf/PM08-before
# 手順 11（cargo build --release --locked の後）
node benchmarks/metrics.mjs size target/release/tsuzuri --out target/perf/PM08-after
node benchmarks/metrics.mjs report target/perf/PM08-after --baseline target/perf/PM08-before
# PB01 Phase 3 へ渡す probe（公開しない）
TZ_EXTRA=-Wl,-dead_strip TSUZURI_CLANG=$W/clang-wrap \
  node benchmarks/metrics.mjs size target/release/tsuzuri --out target/perf/PM08-dead-strip-probe
```

- probe は wrapper が C の hello にも `-dead_strip` を足す。そのことを報告に書く。
- `report --baseline` の表をそのまま完了報告に貼る。`docs/benchmarks.md` には after の値を載せる（「ドキュメント」）。

## 生成コードの確認

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
T=target/release/tsuzuri
$T build examples/hello --emit llvm -O3 -o $W/after-hello.ll
grep -c "@tz_soft_" $W/after-hello.ll                                  # 0
grep -c "^define internal i32 @tz.integer.format(" $W/after-hello.ll  # 1
$T build examples/hello --emit llvm -Oz -o $W/after-hello-Oz.ll && cmp $W/after-hello.ll $W/after-hello-Oz.ll
$T build examples/hello --no-cache -O3 -o $W/after-hello
nm $W/after-hello | grep -c tz_soft                                    # 0
$L/llvm-objdump -h $W/after-hello | grep __text                        # 減った __text
$L/llvm-objdump -d --no-show-raw-insn $W/after-hello | grep -c ___udivti3   # 観察。報告する
$T build examples/hello --no-cache -O3 --strip -o $W/after-strip
nm $W/after-strip | grep -c " t "                                       # 0
codesign -v $W/after-strip
$T build examples/functional --no-cache --target wasm32 -Oz -o $W/f.wasm
$L/llvm-objdump -h $W/f.wasm | grep CODE
```

- 浮動小数点を表示するプログラム（`display_parse.mjs` の `display-only`）の IR には `call i32 @tz_soft_format` が残る。
- `___udivti3` が i64 の表示に残るのは、LLVM が「符号拡張した i64 の絶対値は 2^64 未満」を証明できない場合。正しさには影響しないので停止しない。

## テスト計画

### Rust テスト

- `src/main.rs`: `parses_size_options`（新規）。`build A.tz -Os`・`build A.tz -Oz`・`run Main.tz -Oz` が `OptLevel::Os`・`OptLevel::Oz`、
  `build A.tz --strip` が `strip == true`、指定なしが `OptLevel::O3` と `strip == false`。`rejects_ambiguous_or_unused_arguments` の一覧に
  `["test", "Main.tz", "-Oz"]`・`["build", "Main.tz", "-O3", "-Oz"]`・`["check", "A.tz", "-Os"]`・`["build", "Main.tz", "--strip", "--strip"]`・
  `["run", "Main.tz", "--strip"]`・`["build", "Main.tz", "--strip", "-g"]`・`["build", "Main.tz", "--strip", "--emit", "object"]`・
  `["build", "Main.tz", "--strip", "--target", "wasm32"]` を足す。メッセージは診断の表と完全一致で確かめる（新しい 5 文）。
- `src/llvm.rs` の `mod tests`: `integer_display_avoids_numeric_runtime`（新規）。`emits_explicit_tail_loop_and_checked_arithmetic` と同じ
  `analyze`・`emit(&module, Entry::Console)` で、`export fn main() -> i64 { 5050 }` の IR が `define internal i32 @tz.integer.format(` を 1 回含み
  `@tz_soft_` を含まない。`-> f64 { 1.5 }` の IR は `@tz_soft_format` を含み `@tz.integer.format` を含まない。2 回の emit が一致する。

### E2E（`tests/size.mjs`（新規））

各ケースは自分の一時 root に project を作る（E03 の再帰的な読み込み）。期待値は JavaScript の `BigInt` で計算する（コンパイラの出力を使わない）。

1. 整数の境界（WASM）: 幅 8〜128 × 符号の有無の 10 型それぞれで、値は最小・最小 + 1・−1（符号付き）・0・1・9・10・最大 − 1・最大。
   1 型 1 module に `def v<k> :: <T> = <値>` と `export def c<k> :: bool = to_string (v<k>()) == "<BigInt の文字列>"` を並べ、
   `-O0`・`-O3`・`-Oz` で全 export が `1`、import が空。この形は `i8` で確認済み（native の `run` が `-128`、WASM の `tz_check` が `1`、import 0）。
2. 同じ module を `--emit llvm` にし、`display_parse.mjs` の host と同じ形の C host（`extern int tz_c<k>(void);` を全部 `assert`）と `-O0`・`-O3`・`-Oz` でリンクして実行する。
3. console: `i8`・`i64`・`i128`・`i8u`・`i64u`・`i128u` の最小と最大を `def main :: <T> = <値>` で `run -O0`・`run -Oz` し、stdout が `<BigInt>\n`
   （`def main :: i8 = -128`、`i128` の最小値、`i128u` の最大値は確認済み）。
4. 数値ランタイムの有無: hello の `-O0`・`-O3` の実行ファイルの `nm` に `tz_soft` がない。`def main :: f64 = 1.5`（`1.5` を表示することを確認済み）の実行ファイルには `tz_soft_format` がある。
5. `-Os`・`-Oz`: hello・point・tasks の `run` の出力が `-O3` と同じ。`--emit llvm` の IR が `-O3` と `-Oz` で一致。
6. `--strip`: hello が `5050`、`nm` に ` t ` の行がない、macOS では `codesign -v` が成功。`--trap-info` の `.trap.json` が `--strip` の有無でバイト一致。
7. 決定性と cache: `-Oz --no-cache` の 2 回がバイト一致。一時の `TSUZURI_CACHE_DIR` で `-O3` と `-Oz` を順にビルドすると成果物が異なる。
8. E2000: 診断の表の 5 行を CLI で出し、stderr が `error[E2000]` とメッセージを含む。

最後に `Size: integer display, -Os/-Oz, --strip, determinism and E2000 passed` を出す。大きさの大小関係は assert しない（CI の閾値にしない）。

### 既存テストへの影響

- `src/main.rs` の `assert_eq!(tests.options.optimization, 0)` と `assert_eq!(arguments.options.optimization, 0)` は `OptLevel::O0` になる（型の変更）。
- `tests/display_parse.mjs` の `display-only`: 整数の表示が `@tz_soft_format` を呼ばなくなるので浮動小数点へ移す（D2）。それ以外の期待値は変えない。
- cache の鍵の文字列が変わる（`optimization: 3` → `optimization: O3`、`strip: false` の追加）ので、更新後の最初のビルドは miss になる。`tests/cache.mjs` は変えない。

## ドキュメント

- `docs/benchmarks.md`: `## 成果物サイズ`（新規）。条件（機械・commit・ツールの版）、size suite の after の表（種目 × 指定、`executable_bytes`・`text_bytes`・`wasm_bytes`・
  `wasm_code_bytes`）、C・Rust の hello、file の大きさが 16 KiB 単位で動くこと、再現コマンド。`-dead_strip` の値は「PB01 Phase 3 の候補」として分けて書く。
- `docs/architecture.md`: ランタイム IR を足す条件に `@tz.integer.format` → `integer.ll` を足す。`-Os`・`-Oz` は Clang へそのまま渡し IR は `-O3` と同じだと書く。
- `_docs/tools/command-line.md`: 表の `-O0` から `-O3` の行の後に `-Os`・`-Oz`（build・run。test は不可）と `--strip`（build の native 実行ファイルだけ。WASM は既に除く）。
- `_docs/tools/build-and-cache.md`: cache の鍵に最適化と `--strip` が入ること。`README.md`: 検証コマンドに `node tests/size.mjs target/release/tsuzuri`。
- `_perfs/README.md` の PM08 の状態欄（Phase 1 を満たしたら done）。

## 受け入れ条件

- [ ] hello・point・computations・control・currying・polymorphism の native の実行ファイル（`-O0`・`-O3`）の `nm` に `tz_soft` がない。
- [ ] 整数 10 型の境界値の表示が BigInt の参照と一致する（native・WASM × `-O0`・`-O3`・`-Oz`、WASM の import は空）。
- [ ] `-Os`・`-Oz` が build・run で使え、`--emit llvm` の IR が `-O3` と同じ。test では E2000。
- [ ] 手順 9 の `-Oz` の wrapper で数値・トラップ・所有権の suite がすべて成功した。
- [ ] `--strip` が局所シンボルを除き、実行結果と `.trap.json` を変えない。macOS で署名が有効。診断の表の E2000 が出る。
- [ ] 整数を表示しないプログラムの IR と成果物が既定の指定で変わらない（io を `cmp`）。2 回のビルドがバイト一致する。
- [ ] `target/perf/PM08-before`・`PM08-after`・`PM08-dead-strip-probe` があり、比較の表と dead_strip の値を完了報告と PB01 へ届けた。
- [ ] `docs/benchmarks.md` に成果物サイズの節があり、C・Rust の hello と並んでいる。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

1. macOS arm64 の file の大きさは segment（16 KiB）単位で動き、`-Os`・`-Oz` で `__text` が減っても増えることがある（io）。改善の判断は `text_bytes` で行う。
2. `llvm-objdump -h` の Size は 16 進。`Number(size)` で読むと誤る。
3. InstCombine は `x - (x / 10) * 10` を `urem` に戻しうる。wasm32 の `urem i128` は `__umodti3` になり、`runtime/wasm.ll` が定義するので問題ない。
   別の libcall（`__divti3` など）が未定義になったら停止する。`wasm.ll` の helper を消したり import で補ったりしない。
4. `emit_display` の `Builtin::Display` は値を `ptr` で受ける（`borrowed`）。拡張の前に `load` する。`ToString` は値を直接受ける。
5. `-Wl,-x` を `-r -nostdlib` の relocatable リンクや `dwarf_sidecar` のリンクへ渡さない。`strip` コマンドをリンク後に走らせる方式にしない（macOS の署名を作り直す必要が出る）。
6. `TestOptions::optimization` は `u8` のまま。`src/test_runner.rs` の `format!("-O{}", options.optimization)` を `OptLevel` に変えない。
7. `BuildOptions::validate` の `optimization level must be 0, 1, 2, or 3` を消すとき、同じ文を期待するテストがないことを
   `grep -rn "optimization level must be" src tests` で確かめる（HEAD では `src/driver.rs` の 1 か所だけ）。
8. `scripts/check-runtime-includes.sh` は Git に追跡されていない `include_str!` の対象を拒否する。`integer.ll` を `git add` し忘れると CI だけが失敗する。
9. 手順 9 の wrapper は `/usr/bin/clang` を固定で呼ぶ。Linux ではパスを変える。`TZ_OPT` は `-O0` も `-Oz` に置き換えるので、`-O0` 固有のテストがあればその差だけを調べて報告する。
10. BigInt の重い suite は Node 20 で V8 が落ちることがある。Node 24（`npx --yes --package=node@24 node ...`）で実行する。

## 対象外

- 実行ファイルの圧縮、動的ライブラリへの分割、LTO。
- `-ffunction-sections`・`-fdata-sections`・`-Wl,-dead_strip`・`-Wl,--gc-sections`、`math.ll` の分離、使う記号の集合による判断（PB01 Phase 3）。
- IR への `optsize`・`minsize` 属性の付与、`wasm-opt`、`tz_soft_format` の浮動小数点・decimal ごとの分割（Phase 2）。
- Windows（G10）の計測。`--strip` は Windows では何もしない（MSVC link の PE はシンボル表を持たない）。
- `tsuzuri test` の `-Os`・`-Oz`、WASM の名前 section の指定（`debug_info` でなければ既に `--strip-all`）。

## 決定事項

### D1: PB01 との分担

- 決定: リンカーの未使用コード除去と関数ごとの section、`math.ll` の分離は PB01 Phase 3 が持つ。PM08 は大きさの計測、サイズ優先の指定、
  整数の表示の分離、`--strip` を持ち、`-dead_strip` の効果は wrapper で計測して PB01 へ渡すだけにする。
- 理由: PB01 の D10 が同じ変更を要承認で計画している。全実行ファイルのリンクを 2 つのチケットで変えると検証が二重になる。
- 状態: 既定案（実装者はこの案に従う）

### D2: 整数の表示を数値ランタイムから分ける

- 決定: 手書きの `src/runtime/integer.ll` の `@tz.integer.format(ptr, i128, i1)` を、IR がその名前を含むときだけ足す。`console_main` と
  `emit_display` は `Type::Integer` でこれを呼ぶ。`numeric.c`・`numeric.ll`・`tz_soft_format` は変えない。
- 理由: 整数の変換は 25 行ほどだが、`tz_soft_format` 経由だと `numeric` 全体（hello の `-O3` の `__text` 33,184 bytes の大半）が入る。`numeric.c` に関数を足しても
  `@tz_soft_` の検索で全体が入る。手書き IR は `display.ll` と同じ扱いで、`generate.py` と `FIRST_METADATA` の規則に触れない。
- 状態: 既定案（実装者はこの案に従う）

### D3: `-Os`・`-Oz` の表現と渡し方

- 決定: `OptLevel` を `BuildOptions::optimization` に使い、Clang（IR・C ランタイム・WASM スレッドのランタイム）へ `-Os`・`-Oz` をそのまま渡す。
  IR は変えず、`optsize`・`minsize` 属性も付けない。build と run だけで受け、test は E2000。
- 理由: wrapper の計測で IR 入力のまま `-Oz` が `__text` を 8.6〜10.4% 減らした。属性の付与は全 `define` と手書き IR に触る。
  `TestOptions` は別の型で、test の成果物の大きさに利用者の利益がない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 既定の最適化

- 決定: 既定は `-O3` のまま（test は `-O0` のまま）。サイズ優先は明示指定だけ。
- 理由: 起票時の既定案。実行速度の既定を変えない。
- 状態: 既定案（実装者はこの案に従う）

### D5: `--strip`

- 決定: 新しい指定 `--strip` は build の native 実行ファイルだけで受け、Windows 以外でリンクに `-Wl,-x`（局所シンボルの除去）を足す。
  `-g` との併用、実行ファイル以外の出力は E2000。既定では除かない。
- 理由: `-Wl,-x` は ld64・GNU ld・lld が共通で受け、リンカーが署名するので macOS の署名が有効なまま（`codesign -v` を確認済み）。
  外部シンボルは残る。削減は 592〜1,120 bytes と小さく、プロファイラーが内部関数の名前を失うので既定にはしない。
- 状態: 既定案（実装者はこの案に従う）

### D6: 大きさの指標と size suite

- 決定: PX01 の `METRICS` に `text_bytes`・`wasm_code_bytes` を、`opt` に `Os`・`Oz` を足し、suite `size` を `benchmarks/metrics.mjs` に置く。
  section の大きさは `llvm-objdump -h`（`TSUZURI_OBJDUMP`）だけで読む。大きさは 2 回のビルドの一致を確かめた 1 標本。
- 理由: file の大きさは 16 KiB 単位でしか動かず改善を隠す。`llvm-objdump` は Mach-O・ELF・WASM を同じ形で読め、既にテストが使っている。
  size suite を別の script にすると記録・比較のコードが重複する。
- 状態: 既定案（実装者はこの案に従う）

### D7: C ランタイムと CPU dispatch（F05）

- 決定: `task.c`・`cpu.c`・`io.c`・`task-wasm-threads.c` も同じ `-Os`・`-Oz` でコンパイルする。CPU dispatch の版の数は変えない。
- 理由: `build_complete` の 3 か所が同じ値を使う今の形を保てる。dispatch の版は `Array.sum` の数関数で、減らすと実行時の選択（F05）の意味が変わる。
- 状態: 既定案（実装者はこの案に従う）

### D8: `wasm-opt`

- 決定: Phase 2 で `wasm-opt -Oz` を opt-in の後処理として検討する。導入する場合は `TSUZURI_WASM_OPT` で選び、なければ E2002。
- 理由: 新しい外部ツールで、配布物（G14）・cache の鍵・CI の環境に影響する。計測機にもない。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D9: `tz_soft_format` の分割

- 決定: 浮動小数点・decimal ごとの分割は PB01 Phase 3 の後に、size suite で効果を測ってから判断する。Phase 1 では行わない。
- 理由: `-dead_strip` なしでは `weak hidden` の定義がすべて残るので分割の効果が出ない。`numeric.c` の変更は 2 つの生成物の再生成を伴う。
- 状態: 既定案（実装者はこの案に従う）
