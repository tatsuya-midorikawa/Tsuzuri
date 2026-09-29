# PR04: 数値の表示・解析の高速化

| 項目 | 内容 |
| --- | --- |
| ID | PR04 |
| 分類 | 実行速度 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | PX01 |
| 関連 | D01, D07, D08 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D4（表の追加で数値 runtime を使う全プログラムが約 20 KB 大きくなる。D07 の D9 と同じ扱い）。表を使わない準備の手順は承認不要 |
| 手本にする既存実装 | 表を使わない桁の処理: `src/runtime/numeric.c` の `eight_decimal_digits`（SWAR）と `tz_soft_format` の整数経路（`% 100` で 2 桁ずつ）。表示の配置（指数表記の判定・末尾ゼロの除去）: `format_float`。raw runtime の検証: `tests/display_parse.mjs` の native／WASM × `-O0`／`-O3` のループと `tests/display_parse_runtime.c` の `f`／`p`／`F`／`P` プロトコル。独立参照: `tests/display_parse_reference.py` の `binary_text`・`round_binary`。再生成: `src/runtime/generate.py` と `src/runtime/generate_math.py`（`numeric.ll` の番号の最大値で math の `!N`・`#N` をずらす） |
| 主な影響ファイル | 変更: `src/runtime/numeric.c`, `src/runtime/numeric_tables.h`（新規、`generate.py` が生成）, `src/runtime/generate.py`, `src/runtime/numeric.ll`（再生成）, `src/runtime/math.ll`（再生成。番号のずれだけ）, `tests/display_parse_reference.py`, `tests/display_parse.mjs`, `tests/display_parse_exhaustive.c`（新規）, `tests/display_parse_exhaustive.mjs`（新規、任意の長時間テスト）, `benchmarks/run-numeric.mjs`（新規）, `docs/architecture.md`, `docs/benchmarks.md`, `README.md`, `_perfs/README.md`。確認のみ（変えない）: `tests/display_parse_runtime.c`, `src/llvm.rs`（`numeric.ll` の連結と `FIRST_METADATA`）, `src/llvm_display.rs`, `src/runtime/wasm.ll`（`__multi3`） |
| 計測対象 | F/P プロトコル（`tests/display_parse_runtime.c` と `src/runtime/numeric.ll` を直接リンク）の kind 1（f32）・2（f64）の表示・解析と kind 27（i64u）の表示・解析の 1 回あたりの時間（入力は「計測手順」の固定集合）。PX01 形式の `cpp` suite の `format_parse`。`numeric.ll` の object の大きさ（native・wasm32 × `-O0`・`-O3`） |

## 目的

f32・f64 の最短往復表示と解析を、C++ の `std::to_chars`／`from_chars` と同じ系統のアルゴリズム（表示は Schubfach、解析は Eisel–Lemire と正確な低速経路）に置き換え、1 回あたりの時間を桁違いに縮める。
結果は現在の実装とすべての入力でバイト単位・ビット単位で同一にする（D-11 の規則は変えない）。
JSON・CSV・ログなどのデータ変換で、浮動小数点の入出力がボトルネックにならないようにする。

`cpp/format_parse` は u64 の十進表示と解析だけを測る（`benchmarks/cpp/Kernels.tz` の `format_parse`）。浮動小数点の変更（段 A・B）はこの種目を動かさない。
整数の表示（段 C）は計測で採否を決め、この種目の残りの差（文字列の確保・UTF-16 への拡幅）は PM03・PM06 が扱う。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の状態欄で done であること（`grep -n "PX01" _perfs/README.md`）。計測は PX01 の `benchmarks/metrics.mjs` の `captureRun`・`makeRecord`・`writeRecords`・`readRecords` と `node24 benchmarks/metrics.mjs report` を使う。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（変更前の `numeric.ll`・F/P の時間・大きさ）を `target/perf/PR04-before/` に保存していること。
- 道具: `/usr/bin/clang`（Apple Clang 21.0.0。`numeric.ll` の生成）、`/opt/homebrew/opt/llvm@21/bin/llvm-link`（`math.ll` の生成）、`/opt/homebrew/opt/llvm@23/bin/clang`（WASM の追加検証）、標準ライブラリだけの `python3`、Node 24（`node24() { npx --yes --package=node@24 node "$@"; }`）。
- 段 A・B・C の順に進める。各段は単独で取り込める（段 A だけで止めてもよい）。

### 停止条件

次の場合は即興で回避せず、コマンド・出力・該当箇所を添えて報告する（GUIDE §13）。

1. 新しい経路の出力が現在の実装と 1 件でも異なる（表示の文字列、解析の成否、非 NaN のビット、NaN の分類）。Python 参照（`tests/display_parse_reference.py` の関数）でどちらが D-11 に合うかを調べて報告に書く。現在の実装が誤っている場合も、期待値の変更は承認が要るので止まる。
2. wasm32 の `-O0` または `-O3`（Apple Clang 21・Homebrew Clang 23 のどちらか）で、リンク失敗・trap・結果の不一致が出て、D5 の代替の形（手順 3）でも解消しない。WASM のテストを外す、最適化レベルを変える、Clang の版を固定し直すことで通さない。
3. 再生成で、変更していない関数（`tz_soft_op`・`tz_soft_cast`・`tz_soft_math_unary` など）の本体が `numeric.ll` で変わる、または `math.ll` の差分が `!N`・`#N` の番号のずれ以外を含む。
4. 表の合計が D4 の予算（24,576 bytes）を超える、または D4 の 2 表以外の表（桁の組の表など）が欲しくなった。
5. ホストの浮動小数点（`double` の演算）、libc、`printf`・`strtod`、既定の WASM import、`unsafe`、Rust の crate、npm パッケージが必要になった。
6. `tz_soft_format`・`tz_soft_parse` の引数・戻り値・128 bytes の出力 buffer・4096 bytes の入力上限、または `tests/display_parse_runtime.c` のプロトコルを変える必要が生じた。
7. 既存テストの期待値（`tests/display_parse_reference.py` の出力、`tests/primitives.mjs`・`tests/features.mjs` の期待）を変える必要が生じた。参照の入力を増やすことは期待値の変更ではない。
8. 第三者の実装（OpenJDK の `DoubleToDecimal`、Ryū、fast_float、drachennest など）のソースや表を写したくなった。論文と「アルゴリズム」の擬似コードから書き、表は `generate.py` で計算する（D1・D2）。
9. f16・f128・decimal・整数の表示・解析の F/P の時間が、before と before2 の広がりを超えて悪化した。
10. `cargo test --locked` の trap 情報・DWARF 系のテストが、再生成の後だけ失敗する（`FIRST_METADATA` と metadata 番号の衝突の疑い）。

## 現状と計測（HEAD `f8dc655`）

### 表示の経路

- 入口は `src/runtime/numeric.c` の `tz_soft_format(char *out, const unsigned char *input, int kind)`（`weak hidden`）。`kind` は `format` の switch で、0〜3 が f16・f32・f64・f128、4〜6 が decimal32・64・128、16〜20 が i8〜i128、24〜28 が i8u〜i128u。
- 整数は `tz_soft_format` の中で完結する。u128 の上位を `% 100`、u64 に収まった後を `% 100` で 2 桁ずつ逆順に `digits[40]` へ書き、符号を付けて反転する。表は使わない。
- binary のゼロは `tz_soft_format` が `0`／`-0` を直接書く。それ以外は `format_float`（`noinline`）へ進む。
- `format_float` は `decode` で `tzrt_number`（`tzrt_big coefficient` を含む）を作り、NaN は符号なしの `nan`、無限大は `inf`／`-inf` を書く。binary は `shortest` で最短の `coefficient`・`exponent` を求め、`divide_small(&value.coefficient, 10)` で 1 桁ずつ逆順に取り出し、末尾のゼロを除いて配置する。
- 配置: 有効桁数 `significant`、十進指数 `exponent = value.exponent + significant - 1` として、`exponent >= -6 && exponent < significant + 6` なら通常表記（`0.001`、`1000000`）、それ以外は `d.ddde+N`／`d.ddde-N`（指数は `unsigned_text`）。
- `shortest` は多倍長の Dragon4 である。`numerator`・`denominator`・`lower`・`upper` の 4 つの `tzrt_big`（`LIMBS` 1400 語、1 個 5,604 bytes）を使い、桁ごとに `tzrt_big gap = denominator;` の構造体の複製と比較を行う。閉区間は仮数が偶数のとき、2 の冪（最小の正規数を除く）は下側の隣との間隔が半分。
- 表示の規則は D01 と D-11 のとおり（最短往復、同じ桁数なら近い方、等距離なら偶数。`docs/architecture.md` の数値表示の節）。

### 解析の経路

- 入口は `tz_soft_parse(unsigned char *out, const char *input, u64 length, int kind)`。空と 4096 bytes 超は失敗（戻り値 0）。先頭の `+`／`-` の後、binary と decimal は `inf`・`nan`（小文字 3 文字の完全一致）を先に判定し、残りを `parse_float`（`noinline`）へ渡す。
- `parse_float` の文法: 整数部の数字列、任意の `.` と小数部、任意の `e`／`E`・符号・1 桁以上の指数。整数部と小数部の少なくとも一方に数字が要る（`1.` と `.5` は受理）。`_` は同じ節の数字の間だけ（`decimal_digits` が判定）。末尾に余りがあれば失敗。
- 仮数の全桁を `tzrt_big` へ 1 桁ずつ `multiply_small` で積み、`power` と `pack` で一度だけ最近接・偶数に丸める。値が大きすぎて無限大になる入力は失敗（戻り値 0）、小さすぎる入力は符号付きゼロ。指数の数字は `decimal_digits` が 100000 付近で飽和させる。
- 整数は `eight_decimal_digits`（8 桁の SWAR）と 1 桁ずつの経路で、`0x`・`0b` と `_` を受理し、範囲外は失敗。

### 生成と連結

- `src/runtime/generate.py` は `TSUZURI_CLANG`（既定 `clang`）に `--target=x86_64-unknown-linux-gnu -std=c11 -O1 -ffreestanding -fno-builtin -fno-stack-protector` を渡して `numeric.c` を IR にし、target の行と属性を消して `numeric.ll` を書く。HEAD の `numeric.ll` は 11,337 行・460,147 bytes で、定数は `@.str`（`nan`）と `@.str.1`（`inf`）だけ。
- `src/runtime/generate_math.py` は `numeric.ll` の `#N`・`!N` の最大値 + 1 だけ `math.ll` の番号をずらす。`numeric.ll` を変えたら `math.ll` も必ず再生成する。
- `src/llvm.rs` は生成 IR が `@tz_soft_` を含むとき `numeric.ll` 全体を連結し（重複する `declare void @llvm.trap()` を消す）、`@tz_math_` を含むとき `math.ll` を連結して `declare` 行を名前で重複除去する。`FIRST_METADATA` は両ファイルの `!N` の最大値 + 1 から利用者 IR の metadata を始める。
- wasm32 では i128 の乗除算が `src/runtime/wasm.ll` の `__multi3`（`optnone noinline`）・`__udivti3` などの呼び出しになる。
- 2026-09 の記録: 桁の組の static な表を `numeric.ll` に加えた試作は、Clang 23 の wasm32 `-O0` で不正なコードを生成したため採用しなかった。現在の整数表示が表を使わないのはこのため。

### 検証の仕組み

- `tests/display_parse.mjs` は `python3 tests/display_parse_reference.py` の出力（全 f16、f32・f64 各 10,000 と f128 2,000 の乱択と境界、decimal、整数全幅）に整数の境界を足し、`tests/display_parse_runtime.c` と `numeric.ll` を native（`TSUZURI_CLANG` の `-O0`・`-O3`）と wasm32（`wasm.ll` を連結、import なし）でリンクして全件を照合する。最後に `features.mjs` の `display_parse` suite、parse-only／display-only のプログラム、コンソール出力を確かめる。
- `TSUZURI_MATH_PYTHON` は `tests/math.mjs` だけが使う。表示・解析の参照は PATH の `python3`（標準ライブラリの `fractions`・`decimal`）で動く。
- `tests/display_parse_runtime.c` のプロトコル: `f <kind> <low-hex> <high-hex>` は表示の文字列、`p <kind> <bytes-hex>`（空は `-`）は `<ok> <32 桁の hex>` を出す。大文字の `F <kind> <repeats> ...`・`P <kind> <repeats> ...` は `clock()` で repeats 回の呼び出しを測り、`<1 回あたりの ms> <checksum>` を出す（stdio は測定区間の外）。`#ifndef __wasm__` の `main` だけが時間を測る。

### 計測済みの事実

- 2026-09-26（M1 Max、`-O3`）: `cpp/format_parse`（5,000 回）の中央値は Tsuzuri 0.555905 ms、C++ 0.294335、Rust 1.176169、C# 0.270814、JavaScript 0.719167（`docs/benchmarks.md`）。`_perfs/README.md` の相対性能は 0.751（最速は C#）。
- 2026-09-30 の probe（この機械、Apple Clang 21.0.0、native `-O3`、各 1 回。中央値ではない）の 1 回あたりの時間:

| kind | 入力 | 表示 | 解析 |
| --- | --- | --- | --- |
| 1（f32） | `0.1`（`3dcccccd`） | 843 ns | 985 ns |
| 1（f32） | 最大値（`7f7fffff`） | 1,728 ns | 未計測 |
| 2（f64） | `0.1`（`3fb999999999999a`） | 845 ns | 1,109 ns |
| 2（f64） | π（`400921fb54442d18`、`3.141592653589793`） | 2,507 ns | 1,299 ns |
| 2（f64） | 最小の非正規化数（`1`） | 5,071 ns | 未計測 |
| 2（f64） | 最大値（`7fefffffffffffff`、`1.7976931348623157e+308`） | 8,193 ns | 3,711 ns |
| 27（i64u） | `18446744073709551615` | 23.3 ns | 20.4 ns |
| 27（i64u） | `42` | 9.3 ns | 未計測 |

- 同じ probe で `numeric.ll` を native `-O3` の object にすると `size` の text は 35,036 bytes。
- 読み取り: f32・f64 の表示・解析は整数の 40〜350 倍遅い。`format_parse` の 1 反復は約 111 ns で、そのうち整数の表示と解析は約 40 ns。

### 再現（2026-09-30 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-work-PR04
/usr/bin/clang -O3 -Wno-override-module tests/display_parse_runtime.c src/runtime/numeric.ll -o /tmp/tz-work-PR04/fp-O3
printf '%s\n' 'F 1 200000 3dcccccd 0' 'F 2 200000 400921fb54442d18 0' 'F 27 2000000 ffffffffffffffff 0' \
  'P 2 200000 332e313431353932363533353839373933' 'P 27 2000000 3138343436373434303733373039353531363135' \
  | /tmp/tz-work-PR04/fp-O3
/usr/bin/clang -O3 -Wno-override-module -c -x ir src/runtime/numeric.ll -o /tmp/tz-work-PR04/numeric-O3.o
size /tmp/tz-work-PR04/numeric-O3.o
```

期待: 5 行の `<ms> <checksum>`（1 列目は上の表と同じ桁の大きさ）と、text が 35036 の行。

## 目標と指標

目標は CI の合否条件にしない。PX01 の before／after の手順で記録して判断する（GUIDE §14）。

- G1（段 A）: f32・f64 の表示の 1 回あたりの時間を、固定集合の全入力で before の 1/10 以下にする。
- G2（段 B）: 有効数字 19 桁以下の f32・f64 の解析を before の 1/10 以下にする。20 桁以上の入力（低速経路）は悪化させない。
- G3: f16・f128・decimal・整数の表示・解析と、f32・f64 の結果（全入力）を変えない。
- G4: 追加する定数は D4 の予算（24,576 bytes）以内。`numeric.ll` の object の増分を記録する。
- G5（段 C）: i64u の表示を速くする。速くならなければ段 C は採用しない（D7）。

| 指標 | 単位・統計 | 対象 | 期待（見積もり。計測で確かめる） |
| --- | --- | --- | --- |
| M1 表示の時間 | ns/回（9 標本の中央値・最小・最大） | F の固定集合の kind 1・2（「計測手順」） | before の 1/10 以下（Schubfach の乗算 6 回と桁の配置） |
| M2 解析の時間 | ns/回（同上） | P の固定集合の kind 1・2 のうち 19 桁以下 | before の 1/10 以下（128 bit 乗算 1〜2 回と走査） |
| M3 低速経路の解析 | ns/回（同上） | P の固定集合の 20 桁以上の入力 | 変化なし（走査 1 回分だけ増える） |
| M4 他の型 | ns/回（同上） | F／P の kind 0・3・5・27 | 段 A・B で変化なし |
| M5 `cpp/format_parse` | ms（PX01 の cpp suite の中央値） | 5,000 回の u64 の表示と解析 | 段 A・B で変化なし。段 C で減るか、段 C を採用しない |
| M6 runtime の大きさ | bytes（固定値） | `numeric.ll` の object（native・wasm32 × `-O0`・`-O3`）の `size` の合計 | 表の 20,288 bytes と新しい関数の分だけ増える |
| M7 WASM の時間 | ns/回（Node で `format_value`／`parse_value` を繰り返す。9 標本の中央値） | M1・M2 と同じ入力 | before より遅くならない（i128 乗算は `__multi3` の呼び出し） |

## 変えてはいけない意味

- 表示: すべての kind・すべての入力で出力のバイト列を変えない。f32・f64 では D-11 の規則（最短往復、同じ桁数なら真の値に近い方、等距離なら偶数）、仮数が偶数のときだけ区間の両端を含むこと、2 の冪（最小の正規数を除く）の下側の間隔が半分であること、`-0`、符号なしの `nan`、`inf`／`-inf`、指数表記の条件（`exponent < -6 || exponent >= significant + 6`）、`e+N`／`e-N` の形を保つ。
- 解析: 受理・拒否の集合を変えない（`_` の位置、`1.`・`.5`、`e`／`E`、符号、`inf`・`nan` は小文字 3 文字だけ、4096 bytes の上限、空の入力）。丸めは十進から目的の型への一度だけの最近接・偶数。f32 は f64 を経由しない（二重丸めの禁止）。無限大に丸まる入力は失敗、ゼロに丸まる入力は符号付きゼロ、`nan` は canonical quiet NaN（`special(f, 2, 0)`）。
- ホストの浮動小数点を使わない。新しい C のコードに `float`・`double` の型と演算を書かない（生成 IR の新しい関数に `fmul`・`fadd`・`fdiv`・`fptoui`・`sitofp` がないことを確かめる）。fast-math・FMA の縮約は無関係だが、`generate.py` のフラグを変えない。
- trap しない。新しい経路は、どの入力でも除算のゼロ、表の範囲外の読み出し、未定義の shift を起こさない（添字の範囲は「アルゴリズム」の不変条件で保証する）。
- 決定性: 表は `generate.py` が Python の整数だけで計算し、再生成を 2 回行っても `numeric_tables.h`・`numeric.ll`・`math.ll` が同一になる。利用者のコードから生成する IR（runtime の連結より前）は変わらない。
- WASM: import を増やさない（`WebAssembly.Module.imports` が空）。wasm32 の i128 演算は `wasm.ll` の helper だけを使う。新しい関数は `tzrt_big` を stack に置かない（harness の WASM stack は 1 MiB）。
- ABI とプロトコル: `tz_soft_format`・`tz_soft_parse` の引数・戻り値、128 bytes の出力 buffer、`tests/display_parse_runtime.c` のプロトコルを変えない。f64 の最長の出力は `-2.2250738585072014e-308` の 24 bytes。
- 評価順序・所有権・借用・トラップ情報は runtime の外なので変わらない。`to_string`・`Display.display`・コンソール出力・`Parse.parse` は同じ入口を通るので、同じ高速経路を選ぶ（AGENTS.md の「同等の形が別の経路を選ばない」）。

## 設計

### 構成

- 段 A（表示）: kind 1・2 の有限の非ゼロ値を `format_binary_fast`（新規）が扱う。Schubfach で最短の十進 `(f, e)`（値は `f × 10^e`、`f < 10^17`）を求め、`u64` の除算で桁を出し、`format_float` から切り出した `layout_decimal`（新規）で配置する。f16・f128・decimal は `format_float` のまま（配置も同じ関数を通る）。
- 段 B（解析）: kind 1・2 の数値の文字列を `parse_binary_fast`（新規）が 1 回だけ走査する。有効数字が 19 桁以下で文法が単純（`_` なし）なら Eisel–Lemire で丸める。決められない入力（20 桁以上、`_` を含む、文法の誤り、無限大、`q > 308`、Eisel–Lemire の未決定）は既存の `parse_float` に任せる。高速経路は受理だけを返し、拒否は必ず `parse_float` が決める。
- 段 C（整数の表示）: `tz_soft_format` の整数経路の u64 部分を 8 桁の塊に分け、塊の中を u32 の `% 100` で処理する。表は使わない。計測で採否を決める（D7）。
- 表（D4）: `generate.py` の `write_tables`（新規）が `src/runtime/numeric_tables.h`（新規）を書き、`numeric.c` が `#include "numeric_tables.h"` する。

### データ構造

新 API（実装後に有効。未検証）。`numeric_tables.h` は生成物で、先頭に `/* Generated by generate.py. Do not edit. */` を置く。

```c
/* k = -324..292, index 2 * (k + 324): { g >> 63, g & (2^63 - 1) } */
static const u64 tzrt_schubfach_g[1234] = { 0x...ULL, 0x...ULL, /* 1 行 1 項目 */ };
/* q = -342..308, index 2 * (q + 342): { c >> 64, c & (2^64 - 1) } */
static const u64 tzrt_pow5[1302] = { 0x...ULL, 0x...ULL };
```

```c
static int layout_decimal(char *out, int length, const char *digits, int count, int exponent); /* 新規。digits は下位桁から */
static __attribute__((noinline)) int format_binary_fast(char *out, const unsigned char *input, int kind);  /* 新規 */
static u64 schubfach(int q, u64 c, int bits, int *exponent);                                              /* 新規 */
static int eisel_lemire(u64 w, int q, int bits, u64 *raw);                                               /* 新規。1: 決定、0: 未決定 */
static __attribute__((noinline)) int parse_binary_fast(unsigned char *out, const char *input, u64 length,
                                                       u64 index, int negative, int kind);             /* 新規。1 または -1 */
```

- 名前の接頭辞は `tzrt_`。利用者の export は `tz_<名前>` になるので衝突しない（`tz_soft_` と同じ理由）。
- 表は 1 次元の `u64` 配列にし、添字は `int` で計算する（D5）。2 次元配列・構造体・`u128` の要素・`char` の桁の表は使わない。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| A | `src/runtime/generate.py` | `write_tables`（新規） | 「アルゴリズム」の式で `tzrt_schubfach_g` を計算し、範囲を assert して `numeric_tables.h` を書く。clang の呼び出しより前に実行する |
| A | `src/runtime/numeric_tables.h`（新規） | `tzrt_schubfach_g`（新規） | 1,234 語（9,872 bytes） |
| A | `src/runtime/numeric.c` | `format_float` | 桁を出した後の配置（末尾ゼロの除去、通常表記、指数表記）を `layout_decimal` へ移し、呼び出すだけにする。挙動は変えない |
| A | `src/runtime/numeric.c` | `flog10pow2`・`flog10threequarterspow2`・`flog2pow10`・`rop64`・`rop32`・`schubfach`（新規） | 「アルゴリズム」の擬似コードどおり。`static inline`、`long long` と `u128` の整数演算だけ |
| A | `src/runtime/numeric.c` | `format_binary_fast`（新規） | `load` で raw を読み、NaN・無限大は `format_float` と同じ文字列、それ以外は `schubfach` → `u64` の桁 → `layout_decimal` |
| A | `src/runtime/numeric.c` | `tz_soft_format` | binary のゼロの判定の後に `if (kind == 1 \|\| kind == 2) return format_binary_fast(out, input, kind);` |
| A | `src/runtime/numeric.ll`・`src/runtime/math.ll` | 生成物 | 再生成（手順 3）。手で直さない |
| A | `tests/display_parse_reference.py` | 入力の集合 | 境界を足す（テスト計画）。既存の乱数列を変えないよう、追加分は別の `random.Random(20260930)` を使う |
| A | `tests/display_parse_exhaustive.c`・`tests/display_parse_exhaustive.mjs`（新規） | `main`・`run`（新規） | 変更前の `numeric.ll`（`@tz_soft_` を `@tz_base_` に置換）と新しい `numeric.ll` を 1 つの実行ファイルにリンクし、全 f32 と f64 の乱択を比べる |
| B | `src/runtime/generate.py`・`numeric_tables.h` | `write_tables`, `tzrt_pow5`（新規） | 1,302 語（10,416 bytes） |
| B | `src/runtime/numeric.c` | `el_power`・`eisel_lemire`・`parse_binary_fast`（新規） | 擬似コードどおり |
| B | `src/runtime/numeric.c` | `tz_soft_parse` | `inf`・`nan` の判定の後、`parse_float` の前に `if (kind == 1 \|\| kind == 2) { int r = parse_binary_fast(out, input, length, index, negative, kind); if (r > 0) return r; }` |
| B | `tests/display_parse_reference.py` | 解析の入力 | 中点の前後、19・20 桁の境界、`_`、指数の極端（テスト計画） |
| C | `src/runtime/numeric.c` | `tz_soft_format` の整数経路 | u64 部分を `v % 100000000` の塊に分け、塊は u32 で 2 桁ずつ。u128 の上位の処理は変えない |
| 計測 | `benchmarks/run-numeric.mjs`（新規） | `main`（新規） | F/P の固定集合と WASM の繰り返しを 9 標本測り、PX01 形式で `numeric.jsonl` を書く |
| 確認のみ | `src/llvm.rs` | `numeric.ll` の連結、`FIRST_METADATA` | 変更なし。再生成後に metadata の範囲を確かめる（手順 3） |

### Phase 分割

段 A・B・C の順に実装し、各段の終わりで全テストが通る状態にする。段 A だけ、または段 A・B だけで取り込んでよい。段 C は計測の結果で採否を決める（D7）。

### 生成 IR とランタイム

- `numeric.ll` に `@tzrt_schubfach_g = internal unnamed_addr constant [1234 x i64]`（段 A）と `@tzrt_pow5 = internal unnamed_addr constant [1302 x i64]`（段 B）、`internal` の `@format_binary_fast`・`@parse_binary_fast` が増える。ほかの新しい helper は `-O1` の生成で inline されてよい。
- 64×64→128 bit の乗算は `(u128)a * b` で書く。native は `umulh`（AArch64）・`mul`（x86_64）、wasm32 は `__multi3` の呼び出しになる（D6）。
- `numeric.ll` の `declare` の集合を増やさない。`src/llvm.rs` は `numeric.ll` の連結で `declare void @llvm.trap()` 以外の重複を除かないので、`__builtin_clzll`（`llvm.ctlz.i64`）は使わず、既存の `llvm.ctlz.i32` になる `__builtin_clz` を 2 回使う。
- 利用者の IR、`tz_soft_*` の宣言、`FIRST_METADATA` の仕組みは変えない。再生成で `numeric.ll` の `!N` が増え、`generate_math.py` が `math.ll` の番号をずらす。

### アルゴリズム

表示（段 A）。`raw` は符号を除いた値。ゼロ・NaN・無限大は呼び出し側で処理済み。f64 は `P = 53, BIAS = 1023, QMIN = -1074, H = 2`、f32 は `P = 24, BIAS = 127, QMIN = -149, H = 33`。

```text
t = raw & (2^(P-1) - 1);  bq = raw >> (P-1)
if bq != 0:  c = 2^(P-1) | t;  q = bq - BIAS - (P-1)
             if -P < q < 0 and (c & (2^-q - 1)) == 0: return (c >> -q, 0)      # 整数値の近道
else:        c = t;  q = QMIN
out = c & 1                                   # 1 なら区間の両端を含まない
cb = 4c;  cbr = cb + 2
if c != 2^(P-1) or q == QMIN: cbl = cb - 2;  k = flog10pow2(q)
else:                         cbl = cb - 1;  k = flog10threequarterspow2(q)   # 2 の冪の非対称な区間
h = q + flog2pow10(-k) + H                    # f64 は 1..4、f32 は 32..35
f64: vb = rop64(G[k], cb << h)、vbl・vbr も同じ;  f32: g = (G[k] >> 63) + 1, vb = rop32(g, cb << h)
s = vb >> 2
if s >= 10:                                   # 10 未満なら 1 桁で、これより短くできない
    sp = 10 * (s / 10);  tp = sp + 10
    upin = vbl + out <= 4 sp;  wpin = 4 tp + out <= vbr
    if upin != wpin: return (upin ? sp : tp, k)
u = s + 1
uin = vbl + out <= 4 s;  win = 4 u + out <= vbr
if uin != win: return (uin ? s : u, k)
cmp = vb - 2 (s + u)
return (cmp < 0 or (cmp == 0 and s is even) ? s : u, k)       # 値は f × 10^k
```

```text
flog10pow2(q)              = (q * 661971961083) >> 41                 # floor(log10 2^q)
flog10threequarterspow2(q) = (q * 661971961083 - 274743187321) >> 41  # floor(log10 (3/4 · 2^q))
flog2pow10(e)              = (e * 913124641741) >> 38                 # floor(log2 10^e)
  （long long の積と算術 shift。|q| < 1100、|e| < 400 で厳密な値と一致することを確認済み）
rop64(g, cp):  g1 = g >> 63;  g0 = g & (2^63 - 1)            # 63 bit ずつに分ける（64 bit にしない）
  x1 = hi64(g0 · cp);  y = g1 · cp;  z = (lo64(y) >> 1) + x1   # z < 2^64
  return (hi64(y) + (z >> 63)) | (((z & (2^63 - 1)) + (2^63 - 1)) >> 63)
rop32(g, cp):  x1 = hi64(g · cp);  return (x1 >> 31) | (((x1 & 0xFFFFFFFF) + 0xFFFFFFFF) >> 32)
G[k]（k = -324..292）: r = flog2pow10(-k) - 125;  G[k] = floor(10^-k / 2^r) + 1   # 2^125 < G[k] < 2^126
```

- 桁の出力: `f`（f64 は 17 桁以下）を `do { digits[count++] = '0' + f % 10; f /= 10; } while (f);` で逆順に出し、`layout_decimal(out, length, digits, count, k)` を呼ぶ。`layout_decimal` は `format_float` の `first`（末尾ゼロの除去）以降をそのまま移したもの。
- これは Schubfach 論文（R. Giulietti, "The Schubfach way to render doubles"）の構成で、Java の `Double.toString` と違い「2 桁以上」の制約を持たない純粋な最短形である（Java の `s >= 100` と、`c < C_TINY` の非正規化数を 10 倍する処理は使わない。落とし穴）。

解析（段 B）。`parse_binary_fast` の走査と Eisel–Lemire。

```text
scan: w = 0, digits = 0, frac = 0, seen = false
  数字の列（'0'..'9'）: seen = true; w == 0 かつ '0' なら数えない; それ以外は digits == 19 なら未決定, w = 10w + d, digits++
  '.' の後の数字の列: 同じ規則で w に積み、各桁で frac++（先頭のゼロも frac に数える）
  !seen なら未決定;  'e'/'E' の後: 符号, 1 桁以上の数字（なければ未決定）, 値が 100000 以上なら未決定
  index != length なら未決定（'_'・不正な文字・余り）
  q = ±e - frac;  w == 0 なら符号付きゼロを store して 1;  eisel_lemire(w, q) が未決定なら未決定
  store(out, (negative << (width - 1)) | raw, width) して 1
eisel_lemire(w, q):   f64: M = 52, EMIN = -1023, INF = 0x7FF, RTE = [-4, 23];  f32: M = 23, EMIN = -127, INF = 0xFF, RTE = [-17, 10]
  q < -342 なら raw = 0（決定）;  q > 308 なら未決定
  lz = 先頭のゼロの数(w);  w <<= lz;  (TH, TL) = pow5[q]
  (hi, lo) = w · TH;  mask = 2^(64 - M - 3) - 1
  if (hi & mask) == mask:  s = hi64(w · TL);  lo = lo + s mod 2^64;  lo < s なら hi += 1
  if lo == 2^64 - 1 and not (-27 <= q <= 55): 未決定          # 原論文の保守的な判定
  up = hi >> 63;  shift = up + 64 - M - 3;  m = hi >> shift
  p2 = ((217706 q) >> 16) + 63 + up - lz - EMIN
  if p2 <= 0:  1 - p2 >= 64 なら raw = 0;  m >>= 1 - p2;  m += m & 1;  m >>= 1;  raw = m    # m == 2^M は最小の正規数
  else:  if lo <= 1 and RTE[0] <= q <= RTE[1] and (m & 3) == 1 and (m << shift) == hi: m &= ~1   # ちょうど中点は偶数へ
         m += m & 1;  m >>= 1;  if m >= 2^(M+1): m = 2^M, p2 += 1
         m &= ~2^M;  p2 >= INF なら未決定（無限大は parse_float が失敗にする）;  raw = (p2 << M) | m
pow5[q]（q = -342..308, 2^127 <= c < 2^128）:
  q >= 0: c = 5^q を 2 倍し続けるか 2 で割り続けて（切り捨て）範囲に入れる
  -27 <= q < 0: p = 5^-q, z = bitlen(p); c = floor(2^(z+127) / p) + 1
  q < -27:      c = floor(2^(2z+128) / p) + 1 を 2^128 未満になるまで 2 で割る（切り捨て）
```

- 2026-09-30 の Python の試作（`/tmp/tz-work-PR04/proto.py`、リポジトリ外）で、上の擬似コードは次と一致した: f32 の表示 151,672 件と f64 の表示 24,000 件を Fraction の厳密な最短（`tests/display_parse_reference.py` の `binary_text` と同じ探索）、f64 の表示 410,632 件を Python の `repr`、f64 の解析 902,782 件を `float()`、f32 の解析 370,222 件を Fraction の厳密な丸め（`round_binary` と同じ）。不一致 0、Eisel–Lemire の未決定 0、2 回目の乗算は f64 で 67,168 回・f32 で 34,918 回。非正規化数の `t = 1..11` も一致した。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは通る。`cargo test --locked <filter>` は 0 件でも成功するので `running N tests` を見る（GUIDE §3.1）。再生成は常に次の 2 行を続けて行う（以下「再生成」）。

```sh
TSUZURI_CLANG=/usr/bin/clang python3 src/runtime/generate.py
TSUZURI_CLANG=/usr/bin/clang TSUZURI_LLVM_LINK=/opt/homebrew/opt/llvm@21/bin/llvm-link python3 src/runtime/generate_math.py
```

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドの後、比較の元を残す。
- 確認: 次が成功し、`display_parse.mjs` が O0・O3 の 2 行（`display/parse O0: N exact displays, M parse/round-trip cases on native/WASM`）を 2 回とも出す。N・M を記録する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && node24() { npx --yes --package=node@24 node "$@"; }
cargo build --release --locked && mkdir -p /tmp/tz-PR04
cp target/release/tsuzuri /tmp/tz-PR04/tsuzuri-before && cp src/runtime/numeric.ll /tmp/tz-PR04/numeric-before.ll
grep '^declare' src/runtime/numeric.ll > /tmp/tz-PR04/declares-before.txt
node24 tests/display_parse.mjs target/release/tsuzuri
TSUZURI_CLANG=/opt/homebrew/opt/llvm@23/bin/clang node24 tests/display_parse.mjs target/release/tsuzuri
```

### 手順 2: 計測の runner

- 変更: `benchmarks/run-numeric.mjs`（新規）。PX01 の `validateRecord` が suite `numeric` を拒否する場合だけ、`benchmarks/metrics.mjs` の `METRICS` の `wall_time`・`object_bytes` の suite に `numeric` を足す。
- 内容: 「計測手順」の仕様どおり。`--runtime <numeric.ll>`（既定 `src/runtime/numeric.ll`）、`--metrics <dir>`、`--quick`（repeats を 1/100 にして 1 標本）を受ける。
- 確認: `node24 benchmarks/run-numeric.mjs --runtime /tmp/tz-PR04/numeric-before.ll --metrics target/perf/PR04-before` が `numeric.jsonl` を書き、`node24 benchmarks/metrics.mjs report target/perf/PR04-before` が throw しない。f64 の π の表示が 1〜5 µs 程度（現状の表と同じ桁）。

### 手順 3: 表の読み出しの canary（D5）

- 変更: なし（`/tmp/tz-PR04/canary/` だけ）。
- 内容: 「落とし穴」の canary を、D5 の形（1 次元の `static const u64` 配列を変数の添字で直接読む関数）で作る。`generate.py` と同じフラグで x86_64 の IR にし、target の行を除き、`/usr/bin/clang` と `/opt/homebrew/opt/llvm@23/bin/clang` で wasm32 の `-O0`・`-O3` にして Node で値を確かめる。
- 確認: 4 通りすべてで `WebAssembly.Module` が成功し、値が一致する。1 つでも失敗したら停止条件 2。

### 手順 4: 配置の切り出し（挙動は変えない）

- 変更: `src/runtime/numeric.c` の `format_float` と `layout_decimal`（新規）。再生成。
- 内容: `format_float` の `first` の計算から `return length` までを `layout_decimal` に移す。
- 確認: `git diff --stat src/runtime` が `numeric.c`・`numeric.ll`・`math.ll` だけ。`cargo build --release --locked` の後、手順 1 の 2 つの `display_parse.mjs` が同じ N・M で通る。

### 手順 5: 段 A の表

- 変更: `src/runtime/generate.py` の `write_tables`（新規）、`src/runtime/numeric_tables.h`（新規）、`numeric.c` の `#include "numeric_tables.h"`（typedef の直後）。
- 内容: `G[k]` を式どおり計算し、`assert (1 << 125) < g < (1 << 126)`。1 行 1 語の `0x%016xULL,` で書く。表はまだ使わないので `numeric.ll` は変わらない（未使用の static は消える）。
- 確認: `python3 src/runtime/generate.py` を 2 回実行して `git diff --stat` が同じ。`grep -c "ULL," src/runtime/numeric_tables.h` が 1234。

### 手順 6: Schubfach と `format_binary_fast`

- 変更: `numeric.c` の `flog10pow2`・`flog10threequarterspow2`・`flog2pow10`・`rop64`・`rop32`・`schubfach`・`format_binary_fast`（新規）と `tz_soft_format` の分岐。再生成。
- 内容: 「アルゴリズム」どおり。表の読み出しは D5 の形だけ。
- 確認: `grep -n "^@tzrt_" src/runtime/numeric.ll` が `[1234 x i64]` の 1 行。`diff /tmp/tz-PR04/declares-before.txt <(grep '^declare' src/runtime/numeric.ll)` が空。`cargo build --release --locked` の後、手順 1 の 2 つの `display_parse.mjs` が同じ N・M で通る。

### 手順 7: 段 A の参照の追加

- 変更: `tests/display_parse_reference.py`。
- 内容: 「テスト計画」の表示の追加分を、別の `random.Random(20260930)` で作る。期待値は既存の `binary_text` が計算する。
- 確認: 手順 1 の 2 つの `display_parse.mjs` が通り、N が追加分だけ増える。

### 手順 8: 全数・乱択の比較（任意の長時間テスト）

- 変更: `tests/display_parse_exhaustive.c`・`tests/display_parse_exhaustive.mjs`（新規）。
- 内容: 「テスト計画」の仕様どおり。変更前の runtime は `git show f8dc655:src/runtime/numeric.ll` の `@tz_soft_` を `@tz_base_` に置き換えて作る。
- 確認: `node24 tests/display_parse_exhaustive.mjs --f32 --f64 100000000` が `f32: 4294967296 values, 0 mismatches` と `f64: 100000000 values, 0 mismatches` を出す。まず `--range 0:1000000` で数秒で終わることを確かめる。

### 手順 9: 段 B の表と Eisel–Lemire

- 変更: `write_tables` と `tzrt_pow5`、`numeric.c` の `eisel_lemire`・`parse_binary_fast`（新規）と `tz_soft_parse` の分岐。再生成。
- 内容: 先頭のゼロは `(w >> 32) ? __builtin_clz((u32)(w >> 32)) : 32 + __builtin_clz((u32)w)`。`parse_binary_fast` は -1 か 1 だけを返す。
- 確認: 手順 6 の `declare` の比較が空、`@tzrt_pow5` が `[1302 x i64]`。手順 1 の 2 つの `display_parse.mjs` が通る。

### 手順 10: 段 B の参照と全数の解析

- 変更: `tests/display_parse_reference.py`（解析の追加分）、`display_parse_exhaustive.c`（解析の比較）。
- 確認: 手順 1 の 2 つの `display_parse.mjs` が通り M が増える。手順 8 のコマンドが解析の不一致 0 も出す。

### 手順 11: 段 C の試行（D7）

- 変更: `tz_soft_format` の整数経路。再生成。
- 内容: 「構成」の段 C。before／after を `run-numeric.mjs` の kind 27・26・25 で比べる。
- 確認: `display_parse.mjs` が通る。中央値が before と before2 の広がりを超えて減った場合だけ残し、それ以外は `git checkout -- src/runtime`（この手順の差分だけ）で戻して結果を記録する。

### 手順 12: 全体の確認

- 確認: 次がすべて成功する。`tests/display_parse.rs` は `6 passed`。

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked
cargo test --locked --test display_parse
cargo build --release --locked
for suite in display_parse primitives numeric_casts e2e; do node24 tests/$suite.mjs target/release/tsuzuri || break; done
node24 tests/features.mjs target/release/tsuzuri display_parse
node24 tests/math.mjs target/release/tsuzuri --quick
```

### 手順 13: 計測・生成コード・文書

- 内容: 「計測手順」「生成コードの確認」を行い、「ドキュメント」を更新する。
- 確認: `target/perf/PR04-before`・`PR04-after`・`PR04-before2` に `numeric.jsonl`・`cpp.jsonl`・`env.txt` がある。`node scripts/check-docs.mjs README.md docs/architecture.md docs/benchmarks.md` が成功する。

## 計測手順

GUIDE §14 と PX01 の「before／after」に従う。環境の記録（`env.txt`）は PX01 の「環境と条件」のコマンドを使う。

- `run-numeric.mjs` の仕様: `tests/display_parse_runtime.c` と `--runtime` の IR を `$TSUZURI_CLANG`（既定 `clang`）の `-O3` で native にリンクし、下の固定集合を 1 行ずつ `F`／`P` で流す。warm-up 1 プロセスの後に 9 プロセスを順に実行し、行ごとの 1 回あたりの ms を標本にする。WASM は `display_parse.mjs` と同じ方法で `-O3` の module を作り、Node で `format_value`／`parse_value` を repeats 回呼ぶ時間を `performance.now()` で 9 標本測る。大きさは `-c -x ir` の object を native・wasm32 × `-O0`・`-O3` で作り、`statSync().size` を 1 標本で記録する。
- レコード: `suite: "numeric"`、`language: "tsuzuri"`、`workload` は下の名前、`size` は repeats、`variant: "default"`、`target` は `native`／`wasm32`、`opt: "O3"`（大きさは `O0`／`O3`）、`cpu_mode: "generic"`、`metric` は `wall_time`（ms）または `object_bytes`。
- 固定集合（repeats は浮動小数点 200,000、整数 2,000,000）:

| workload | 行 |
| --- | --- |
| `f32-format-tenth`・`f32-format-pi`・`f32-format-max`・`f32-format-min` | `F 1 <n> 3dcccccd 0`・`40490fdb`・`7f7fffff`・`1` |
| `f64-format-tenth`・`f64-format-pi`・`f64-format-1e23`・`f64-format-min`・`f64-format-max`・`f64-format-integer` | `F 2 <n> 3fb999999999999a 0`・`400921fb54442d18`・`44b52d02c7e14af6`・`1`・`7fefffffffffffff`・`40fe240000000000` |
| `f32-parse-tenth`・`f32-parse-pi`・`f32-parse-max` | `0.1`・`3.1415927`・`3.4028235e+38` の 16 進 |
| `f64-parse-tenth`・`f64-parse-pi`・`f64-parse-max`・`f64-parse-min` | `0.1`・`3.141592653589793`・`1.7976931348623157e+308`・`5e-324` |
| `f64-parse-long`・`f64-parse-underscore`（低速経路） | `1.000000000000000000000001`・`1_000.5` |
| `f16-format-tenth`・`f128-format-tenth`（M4） | `F 0 <n> 2e66 0`・`F 3 <n> 999999999999999a 3ffb999999999999` |
| `i64u-format-max`・`i64u-format-small`・`i64u-parse-max`・`i64u-parse-small`（M4・段 C） | `F 27 <n> ffffffffffffffff 0`・`2a`、`18446744073709551615`・`42` |

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && node24() { npx --yes --package=node@24 node "$@"; }
for run in before after before2; do
  RUN=target/perf/PR04-$run; mkdir -p "$RUN"
  if [ $run = after ]; then rt=src/runtime/numeric.ll; tz=target/release/tsuzuri
  else rt=/tmp/tz-PR04/numeric-before.ll; tz=/tmp/tz-PR04/tsuzuri-before; fi
  node24 benchmarks/run-numeric.mjs --runtime $rt --metrics "$RUN"
  node24 benchmarks/run-cpp.mjs $tz --metrics "$RUN" > "$RUN/cpp.json"
done
node24 benchmarks/metrics.mjs report target/perf/PR04-before2 --baseline target/perf/PR04-before
node24 benchmarks/metrics.mjs report target/perf/PR04-after --baseline target/perf/PR04-before
```

- before2 と before の差が広がりを超えた metric は「判定できない」と報告する。改善は after と before の `report` の `verdict` だけで述べ、`cpp/format_parse` の変化を浮動小数点の成果として書かない。

## 生成コードの確認

```sh
grep -n '^@tzrt_' src/runtime/numeric.ll
diff /tmp/tz-PR04/declares-before.txt <(grep '^declare' src/runtime/numeric.ll)
awk '/^define internal .*@(format_binary_fast|parse_binary_fast)\(/,/^}/' src/runtime/numeric.ll \
  | grep -cE 'fmul|fadd|fsub|fdiv|fptoui|fptosi|uitofp|sitofp|double|float'
grep -cE 'samesign|zext nneg|trunc nuw|getelementptr nuw|or disjoint' src/runtime/numeric.ll
/usr/bin/clang -O3 -S -Wno-override-module -x ir src/runtime/numeric.ll -o /tmp/tz-PR04/numeric.s
awk '/^_format_binary_fast:/,/^; -- End function/' /tmp/tz-PR04/numeric.s | grep -cE 'umulh'
awk '/^_format_binary_fast:/,/^; -- End function/' /tmp/tz-PR04/numeric.s | grep -E '\bbl\b'
```

期待: `@tzrt_` は 2 行（`[1234 x i64]`・`[1302 x i64]`）、`diff` は空、浮動小数点の命令は 0。新しい構文の件数は同じ `grep` を `/tmp/tz-PR04/numeric-before.ll` にかけた件数と同じ（generate.py の後処理が HEAD と同じに扱う）。`_format_binary_fast` に `umulh` が 6 回以上あり、`bl` は `_layout_decimal` だけ（inline されていればなし）。`tzrt_big` を使う関数（`_decode`・`_shortest`）を呼ばない。

## テスト計画

### Rust テスト

新しい Rust テストはない（runtime だけの変更）。`tests/display_parse.rs` の 6 件（`provides_display_for_primitives_and_parse_for_numbers_and_bool`、`numeric_and_boolean_console_output_uses_the_shared_buffered_path` など。`@printf`・`@strtod` がないことを含む）と `cargo test --locked` 全体が通ること。

### E2E

- `tests/display_parse.mjs` を `TSUZURI_CLANG` 未設定（PATH の `clang`、この機械では Apple Clang 21）と `/opt/homebrew/opt/llvm@23/bin/clang` の 2 回、native・wasm32 × `-O0`・`-O3` で実行する。WASM の import は空、memory は 16 MiB 以下（既存の assert）。
- `tests/display_parse_reference.py` に足す表示の入力（期待値は `binary_text`）: f32・f64 の全指数欄 `e` について仮数欄 `0, 1, 2, max-1, max`（f32 1,275 件、f64 10,235 件。有限値だけ）、非正規化数の raw `1..64`、`2^53-1`・`2^53`・`2^53+2`、`1e22`・`1e23`・`5e-324`・`2.2250738585072014e-308` の raw、f32 の `2^24` 前後。
- 足す解析の入力（期待値は `parse`）: 乱択の raw の中点を 19 桁に丸めた値とその ±1（f32・f64 各 2,000 件）、19 桁以下で表せる正確な中点 `(2m+1)·2^j`（f64 は `j = -2..11`、f32 は `j = -12..39`、各 j で 20 件）、`9999999999999999999` と `10000000000000000000`（19 桁と 20 桁）、それぞれの `_` 入り、`1e-343`・`1e-342`・`1e308`・`1e309`（失敗）、`2.4703282292062327e-324`・`2.4703282292062328e-324`、`7.006492321624085e-46`・`3.4028235677973366e38`（f32）、`-0.0`・`0e999999`・`.5`・`1.`・`1e`・`1_e5`（拒否）。
- `tests/display_parse_exhaustive.mjs`（任意。README の既定の一覧に入れない）: native `-O3` の 1 つの実行ファイルに新旧の runtime をリンクし、範囲を `os.availableParallelism()` 個の子プロセスに分ける。f32 は全 raw について、新旧の表示のバイト列、その文字列の新旧の解析（成否とビット）、非 NaN の往復（`parse(format(x)) == x`）、NaN の canonical を比べる。f64 は seed 固定の乱択 raw で同じことを行う。不一致は最初の 10 件を 16 進で出して終了コード 1。見積もり（現状の probe から）: 旧経路が 1 値あたり約 3 µs、全 f32 で単一スレッド約 3.6 時間、10 プロセスで約 25 分。

### 既存テストへの影響

なし。期待値は変わらず、`numeric.ll`・`math.ll` が再生成されるだけ。IR 全体を固定値で比べるテストが見つかった場合は停止条件 7。

### 性能

「計測手順」だけで判断する。速度の合否をテストに入れない。

## ドキュメント

- `docs/architecture.md`: runtime の表の `src/runtime/numeric.c` / `numeric.ll` の行、「binary は既存 `tzrt_big` による Dragon4 の正確な境界区間から最短の桁を生成します。」の段落（f32・f64 は Schubfach と Eisel–Lemire、表は `generate.py` の生成物、それ以外は Dragon4 と正確な経路）、`tests/display_parse.mjs` の説明の段落（追加の集合と任意の全数テスト）。
- `docs/benchmarks.md`: `## C/C++・Rust・C#・JavaScript の横断比較` の `Display/Parse` の行（「全数値型の表示・解析速度は未測定」）を更新し、`### 数値の表示・解析（PR04、日付）`（新規）に before／after の表（M1〜M7、run_id と commit）を足す。
- `README.md`: テストの一覧（`node tests/display_parse.mjs target/release/tsuzuri` の近く）に任意の全数テストと `benchmarks/run-numeric.mjs` を足す。
- `_perfs/README.md`: PR04 の状態。`_docs/` は変えない（利用者に見える挙動は同じ）。

## 受け入れ条件

- [ ] 段 A・B で、全 f32 の表示・解析と f64 の 1 億件の乱択が変更前と完全に一致する（手順 8・10 の出力を完了報告に貼る）。
- [ ] `tests/display_parse.mjs` が Apple Clang 21 と Homebrew Clang 23 の両方で、native・WASM × `-O0`・`-O3` を通る。
- [ ] 手順 12 のコマンドがすべて通る。
- [ ] 表は `generate.py` の生成物で、合計 24,576 bytes 以下、再生成が決定的。`numeric.ll` の `declare` の集合が変わらない。
- [ ] 「生成コードの確認」の期待をすべて満たす。
- [ ] PX01 形式の before・after・before2 があり、M1〜M7 を報告している。段 C の採否と理由を記録している。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- Java の `DoubleToDecimal` の形（`s >= 100`、`c < C_TINY` を 10 倍する処理）は「2 桁以上」を出すので D-11 と合わない。試作では f64 の raw `1`・`2`・`a`〜`14`、f32 の raw `1`〜`4` などで最短より長くなった。`s >= 10` の純粋な形を使う。
- `rop64` の 63 bit の分割を 64 bit の分割に「簡単化」しない。sticky の範囲が変わり、証明の前提が崩れる。
- `flog*` は `int` の積だと溢れる。`long long` で掛け、算術 shift（Clang の実装定義の挙動）で floor にする。
- `pow5` は q の範囲で丸めが違う（`-27 <= q < 0` は +1、`q < -27` は +1 の後に切り捨て）。全部を切り捨てにすると `0.5` のような正確な十進でも 2 回目の乗算に入り、中点の判定も変わる。
- Eisel–Lemire の高速経路は受理だけを返す。拒否・無限大・`_` を高速経路で判断すると、受理集合が変わる（停止条件 1）。
- 19 桁の数え方: 先頭のゼロは数えず、最初の非ゼロ以降のゼロは数える。`frac` は小数部の全桁（先頭のゼロを含む）。
- `display_parse_reference.py` は 1 つの `rng` を共有している。既存の生成の途中に入力を足すと後続の乱数列がすべて変わる。追加分は別の `random.Random(20260930)` で末尾に足す。
- wasm32 の `-O0`: 2026-09 に桁の組の表の試作が Clang 23 の wasm32 `-O0` で壊れた。2026-09-30 の canary（`generate.py` と同じ形の IR）では、64 語の `u64` 表・`[32][2]` の表・桁の組の `char` 表と `% 100` の桁出しを 1 つの module に置くと、Apple Clang 21・Homebrew Clang 22・23 のどれでも wasm32 `-O0` の module が `WebAssembly.Module` で拒否された（`-O1`・`-O3` は正しい）。一方、1 次元の `u64` 表を変数の添字で直接読む関数だけの module は、Apple Clang 21・Clang 23 の `-O0` で正しかった。原因の形は特定していないので D5 の形だけを使い、段ごとに `display_parse.mjs` を両方の Clang で実行する。
- `numeric.ll` を変えたら `math.ll` も再生成する。`math.ll` だけ古いと `!N` が衝突し、trap 情報の metadata が消える（停止条件 10）。Homebrew の Clang で生成すると無関係な IR の差分が大量に出る。
- 重い BigInt の suite は Node 24 で動かす（Node 20.19.6 は V8 の `RepresentationChangerError` で止まることがある）。
- F/P の `clock()` は分解能が粗い。1 行が 50 ms 以上になる repeats にする。
- D07 の段 B（`tz_soft_format_spec`）は PR04 の後なら `schubfach` を共有する。D07 と同時に進める場合は `numeric.c` の競合に注意し、先に取り込んだ側の再生成の後でもう一方を再生成する。

## 対象外

- f16・f128・decimal のアルゴリズム（D3）、整数の解析（既に SWAR）、16 進の浮動小数点の表記。
- 書式指定（D07）の表示規則、locale 依存の表示。D-11 の規則の変更。
- wasm32 向けの i128 乗算の最適化（`__multi3` を避ける 32 bit 分割）。必要なら別チケット。
- `cpp/format_parse` の残りの差（文字列の確保と UTF-16 への拡幅）。PM03・PM06 が扱う。

## 決定事項

### D1: 表示のアルゴリズム

- 決定: f32・f64 は Schubfach の純粋な最短形（「アルゴリズム」）。f32 は f64 の表の上位 63 bit + 1 を使う。論文から実装し、OpenJDK・Ryū・drachennest のソースと表を写さない。
- 理由: 証明があり、f32 と f64 が 1 つの表（9,872 bytes）を共有できる。Ryū は型ごとに 2 表（f64 だけで約 10.7 KB）、Dragonbox は表が同程度でコードが大きい。試作で全比較が一致した。
- 状態: 既定案（実装者はこの案に従う）

### D2: 解析のアルゴリズム

- 決定: 有効数字 19 桁以下で `_` を含まない f32・f64 は Eisel–Lemire。原論文の保守的な未決定の判定を残し、未決定・20 桁以上・無限大・文法の誤りは既存の `parse_float` が決める。Clinger の近道（ホストの `double` の演算）は使わない。
- 理由: 一般的な入力のほぼすべてを 128 bit の乗算 1〜2 回で丸められ、正確な低速経路がそのまま代替になる。ホストの浮動小数点は D-11 と「変えてはいけない意味」に反する。
- 状態: 既定案（実装者はこの案に従う）

### D3: f16・f128・decimal

- 決定: 変えない（`shortest` の Dragon4 と既存の解析）。
- 理由: f128 は 256 bit の表と乗算が要り、f16 と decimal は利用が少ない。同一性の証明がない経路を足さない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 表の生成と大きさ

- 決定: `generate.py` の `write_tables` が Python の整数だけで 2 表を計算し、`src/runtime/numeric_tables.h` に書いてコミットする。合計 20,288 bytes、予算 24,576 bytes。
- 理由: 生成の手順が 1 つのコマンドに収まり、決定的で、手で直す余地がない。PM08 はこの増分を記録する。
  ただし表の追加は数値 runtime を使う全プログラムの成果物を約 20 KB 大きくするので、D07 の D9（`numeric.ll` の増分）と同じく
  人間が速度と大きさの交換を判断する。
- 状態: 要承認（承認前は表を使う手順に着手しない。計測の準備と表を使わない手順は進めてよい）

### D5: 表の埋め込みと読み出し（WASM）

- 決定: 表は 1 次元の `static const u64` 配列とし、`int` で計算した添字で直接読む。2 次元配列・構造体・`u128` の要素・`char` の桁の表は使わない。手順 3 の canary でこの形を wasm32 `-O0`・`-O3` で確かめてから手順 6 へ進み、各段で `display_parse.mjs` を Apple Clang 21 と Homebrew Clang 23 の両方で実行する。
- 理由: 2026-09-30 の canary で、この形は両方の Clang の wasm32 `-O0` で正しく、表を含む別の形の組み合わせは不正な module になった（落とし穴）。`static const u64 *const volatile` の基底ポインターを読んでから添字で読む形も両方で正しかったので、直接の読み出しが実際の runtime で壊れた場合はこの形を 1 回だけ試し、それでも壊れたら停止条件 2。
- 状態: 既定案（実装者はこの案に従う）

### D6: 128 bit の乗算

- 決定: `(u128)a * b` で書き、target ごとの分岐を作らない。
- 理由: `numeric.ll` は target に依存しない 1 つの IR で、native は 1〜2 命令になる。wasm32 の `__multi3` の費用は M7 で記録する。
- 状態: 既定案（実装者はこの案に従う）

### D7: 段 C の採否

- 決定: 段 C（8 桁の塊と u32 の `% 100`）は試行し、`run-numeric.mjs` の kind 27 の表示の中央値が before と before2 の広がりを超えて減った場合だけ採用する。表は使わない。
- 理由: 現状の整数の表示は 23 ns と小さく、効果が計測の揺れに埋もれる可能性がある。採用の根拠を計測に置く。
- 状態: 既定案（実装者はこの案に従う）

### D8: 計測の形式

- 決定: `benchmarks/run-numeric.mjs` が suite `numeric` の PX01 形式（`wall_time`・`object_bytes`）で記録する。`cpp/format_parse` は既存の `run-cpp.mjs` で測る。
- 理由: F/P プロトコルは stdio と起動を測定区間の外に置き、型別の 1 回あたりの時間を直接測れる。
- 状態: 既定案（実装者はこの案に従う）

### D9: 全数テストの位置付け

- 決定: `tests/display_parse_exhaustive.mjs` は任意の長時間テストとし、README の既定の一覧と CI に入れない。段 A・B の完了時に 1 回実行して結果を完了報告に貼る。
- 理由: 全 f32 は数十分かかる。共有 CI の時間を使わず、正しさの主張の根拠だけを残す。
- 状態: 既定案（実装者はこの案に従う）
