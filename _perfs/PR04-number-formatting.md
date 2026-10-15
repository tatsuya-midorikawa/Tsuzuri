# PR04: 数値の表示・解析の高速化

| 項目 | 内容 |
| --- | --- |
| ID | PR04 |
| 分類 | 実行速度 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | PX01 |
| 関連 | D01, D07, D08 |
| 状態 | in-progress（全段 A/B/C を実装・検証） |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 2026-10-14: 利用者が全設計判断・段 A/B/C・D4 の 20 KB 増分を承認 |
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
- 段 A・B・C の順に進める。利用者の 2026-10-14 の依頼では全段を実装・検証する。段 C の採否は D7 の計測で決める。

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
- 段 B（解析）: kind 1・2 の符号を除く ASCII 表記が 25 bytes 以下なら `parse_binary_fast` が 1 回だけ走査する。有効数字が 19 桁以下で文法が単純（`_` なし）なら Eisel–Lemire で丸める。決められない入力（有効数字 20 桁以上、長い表記、`_` を含む、文法の誤り、無限大、`q > 308`、Eisel–Lemire の未決定）は既存の `parse_float` に任せる。高速経路は受理だけを返し、拒否は必ず `parse_float` が決める。長い表記は投機的な再走査をせず、直接正確な経路へ進む。
- 段 C（整数の表示）: u64 部分を 8 桁の塊と u32 の `% 100` にする候補を実装・検証・計測したが、D7 の noise gate を満たさず不採用。現在は従来の u64 `% 100` を保持する。桁の表は使わない。
- 表（D4）: `generate.py` の `write_tables`（新規）が `src/runtime/numeric_tables.h`（新規）を書き、`numeric.c` が `#include "numeric_tables.h"` する。

### データ構造

検証済みの内部 API。`numeric_tables.h` は生成物で、先頭に `/* Generated by generate.py. Do not edit. */` を置く。

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

- [x] 段 A・B で、全 f32 の表示・解析と f64 の 1 億件の乱択が変更前と完全に一致する（手順 8・10 の出力を完了報告に貼る）。
- [x] `tests/display_parse.mjs` が Apple Clang 21 と Homebrew Clang 23 の両方で、native・WASM × `-O0`・`-O3` を通る。
- [ ] 手順 12 のコマンドがすべて通る。
- [x] 表は `generate.py` の生成物で、合計 24,576 bytes 以下、再生成が決定的。`numeric.ll` の `declare` の集合が変わらない。
- [x] 「生成コードの確認」の期待を満たす（annotation 件数については D11 の整合を適用）。
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

- 決定: 符号を除く ASCII 表記が 25 bytes 以下、有効数字 19 桁以下で `_` を含まない f32・f64 は Eisel–Lemire。原論文の保守的な未決定の判定を残し、未決定・有効数字 20 桁以上・長い表記・無限大・文法の誤りは既存の `parse_float` が決める。Clinger の近道（ホストの `double` の演算）は使わない。
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
- 状態: 2026-10-14 に利用者承認済み（全段と表の増分）

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
- 状態: 2026-10-14 に完了、不採用。kind 27 の max/small は native/WASM とも gate を越えて改善しなかった。候補を保存し、u64 整数表示を基準 C ソースと同一の `% 100` に戻した。詳細は末尾の正式計測を参照。

### D8: 計測の形式

- 決定: `benchmarks/run-numeric.mjs` が suite `numeric` の PX01 形式（`wall_time`・`object_bytes`）で記録する。`cpp/format_parse` は既存の `run-cpp.mjs` で測る。
- 理由: F/P プロトコルは stdio と起動を測定区間の外に置き、型別の 1 回あたりの時間を直接測れる。
- 状態: 既定案（実装者はこの案に従う）

### D9: 全数テストの位置付け

- 決定: `tests/display_parse_exhaustive.mjs` は任意の長時間テストとし、README の既定の一覧と CI に入れない。段 A・B の完了時に 1 回実行して結果を完了報告に貼る。
- 理由: 全 f32 は数十分かかる。共有 CI の時間を使わず、正しさの主張の根拠だけを残す。
- 状態: 既定案（実装者はこの案に従う）

### D10: 現在のコードとの整合（2026-10-14）

- D07 の `tz_soft_format_spec` が既にある。書式なし・幅のみの表示は `tz_soft_format` を共有するため高速経路も共有する。明示の小数桁・指数書式は従来の正確な処理を保つ。
- PX01 の `validateRecord` は任意の suite 名を受け入れるため metrics.mjs の変更は不要。
- 基準コンパイラは `target/perf/phase2-baseline/tsuzuri-before`、基準 runtime は着手時の `target/perf/PR04/numeric-before.ll` に保存する。作業・生成・計測はすべて repository の target 以下とし、旧い `/tmp` の例は使用しない。
- D4 は利用者承認済み。全段を完遂し、段 C は D7 の実測により採否を決める。共通文書と compiler の変更・最終公開は統合担当が行う。

### D11: 宣言と同等経路の保持

- 定数表の生成は Python の整数だけで行い、2,536 語（20,288 bytes）を生成する。`q` の非正規化数補正は符号なしの借りを使い、Clang が `llvm.umax.i32` という新しい宣言を導入しない形にした。
- `tz_soft_format_spec` の十進整数も `tz_soft_format` を共有する。D07 が追加した汎用 `u128` の基数処理をこの場合だけ避け、桁・符号・`+` の意味を保持する。C 棄却後も通常の表示と同じ従来の u64 `% 100` 経路を選ぶ。2/8/16 進書式は従来どおり。
- `tests/display_parse.mjs --runtime-only` は compiler の統合待ちでも生 runtime の native/WASM × O0/O3 を検証する。通常の呼び出しは既存の compiler E2E もすべて実行する。
- 表の直接読み出しは実 runtime を用いて Apple Clang 21・Homebrew Clang 23 の native/WASM × O0/O3 全組で検証した。分離した canary より強い確認として扱い、基底 volatile ポインターの代替は不要だった。
- `math.ll` は metadata/attribute 番号の変更だけ。`numeric.ll` は番号を正規化すると `tz_soft_format`・`format_float`・`tz_soft_format_spec`・`tz_soft_parse` の既存本体だけが変わり、算術・cast・hash・正確な解析の本体は変わらない。attribute group の実内容まで展開した比較でも同じ結果で、math の全文は番号だけを正規化すると一致する。新旧の宣言集合は番号を正規化すると一致する。
- 新しい 3 helper（`format_binary_fast`・`layout_decimal`・`parse_binary_fast`）と 2 表（`tzrt_pow5`・`tzrt_schubfach_g`）を `src/abi.rs` の `RESERVED_HOST_SYMBOLS` に登録し、extern の LLVM 記号衝突を防ぐ。網羅テストの過大な `tz` 前方一致も実際の `RESERVED_SYMBOL_PREFIXES` に合わせ、表が未予約なら検出する。native-interop の既存の「runtime が使う名前は E1008」という説明に含まれるため、同ページの変更は不要。
- native の raw numeric テストは `TSUZURI_ASAN=1` で C ソースを ASan/UBSan 付きでビルドできる。Windows のテストには既存の i128 helper をリンクし、C の数値プロトコルだけ CRLF を正規化し、生成する C host の assert を `NDEBUG` で消さない形にした。CLI の文字列出力はそのまま比較する（Windows の実行はこの機械では未検証）。
- compiler E2E の build-cache は各実行の target 内へ隔離する。共有の巨大な default cache で `run` がファイル走査に長時間かかる状態が PB01 の sample で観測されたためであり、runtime 経路や最適化レベルは変更しない。
- 「新しい構文の件数が旧版と同一」という旧い確認条件は、本体が増えるため適切ではない。実測は `samesign` が 30→32、後処理で取り除く `zext nneg`・`trunc nuw`・`getelementptr nuw`・`or disjoint` は双方 0。生成フラグ・後処理は変更していない。

## 初回の実装と検証（2026-10-14、C 候補の記録）

- A（Schubfach）・B（Eisel–Lemire + exact fallback）を実装し、C（8 桁塊の整数表示）候補を実装。C の正式採否は idle window での matched 計測待ち。
- `TMPDIR="$PWD/target/pr04-work" npx --yes --package=node@24 node tests/display_parse.mjs target/perf/phase2-baseline/tsuzuri-before --runtime-only`: 各 O0/O3 に 109,803 件の厳密な表示、147,420 件の解析・往復を native/WASM で照合、成功。`TSUZURI_CLANG=/opt/homebrew/opt/llvm@23/bin/clang` でも同じ 2 行で成功。新しい参照は全指数欄の仮数境界、非正規化数 1..64、中点の前後、19/20 桁、各バイト位置の不正文字、整数の 10^8/10^16/2^64 境界と全整数幅の乱択を含む。
- `... node tests/display_parse_exhaustive.mjs --range 0:1000000 --f64 1000000 --jobs 2`: f32/f64 各 1,000,000 件、表示・解析・往復・canonical NaN の不一致 0。
- CI 用の bounded smoke は `... node tests/display_parse_exhaustive.mjs --f32-samples 100000 --f64 100000 --jobs 2 --baseline-runtime target/numeric-ci/before.ll`。f32 は固定の zero/inf/NaN/subnormal 境界と、全ビット域の seed 固定乱択を使う。新旧双方で表示・解析・往復・canonical NaN の不一致 0 を確認した。baseline の numeric.ll は `a621fada69bf27f425bfbb012cc7d0a699721db6` の blob を明示的に用意する（shallow checkout ではこの commit も fetch する）。全 f32 のオプトイン・ローカル実行と混同せず、速度閾値は入れない。
- 任意 harness へ意図的に誤った formatter を指定する fault injection は終了コード 1、不一致を検出し、他の子プロセスを終了した。小さい負の inf/NaN 境界範囲の追加 smoke も成功。
- `TMPDIR="$PWD/target/pr04-work" ... node tests/display_parse_exhaustive.mjs --f32 --f64 100000000 --jobs 6` は終了コード 0。全 f32 4,294,967,296 件と f64 100,000,000 件が旧 runtime と完全一致した。stdout は `target/perf/PR04/exhaustive.log`:

```text
f32: 4294967296 values, 0 mismatches (display/parse/round-trip/canonical NaN)
f64: 100000000 values, 0 mismatches (display/parse/round-trip/canonical NaN)
```

- 6 個の全数 worker は終了済み。これは native O3 の全数・乱択照合であり、WASM の全数ではない（WASM は上の独立参照と compiler E2E の集合で O0/O3 を検証）。
- `... node benchmarks/run-numeric.mjs --compiler target/perf/phase2-baseline/tsuzuri-before --quick --metrics target/perf/PR04-smoke` は 78 レコードを作り、`benchmarks/metrics.mjs report` は成功（quick の速度は成果として主張しない）。
- 生成コード: `format_binary_fast` は ARM64 の `umulh` 11 箇所、call は `layout_decimal` だけ。`parse_binary_fast` は `umulh` 2 箇所、call なし。新経路はホスト浮動小数点命令 0。表示の alloca は 24-byte の桁バッファだけ、解析の alloca はなし。
- IR の metadata 最大は numeric 222 / math 290。compiler 側の metadata 開始位置との整合は統合担当が検証する。`cargo test --locked --test display_parse` は 6 件すべて成功。
- `TMPDIR="$PWD/target/pr04-work" TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 UBSAN_OPTIONS=halt_on_error=1 ... node tests/display_parse.mjs ... --runtime-only`: native C の ASan/UBSan と通常の WASM を両 O0/O3 で照合し、109,803 表示・147,420 解析が成功。未定義動作・stack-use-after-return・範囲外アクセスの検出なし。
- `TMPDIR="$PWD/target/pr04-work" TSUZURI_CLANG=/usr/bin/clang python3 src/runtime/generate.py` と `TMPDIR="$PWD/target/pr04-work" TSUZURI_CLANG=/usr/bin/clang TSUZURI_LLVM_LINK=/opt/homebrew/opt/llvm@21/bin/llvm-link python3 src/runtime/generate_math.py` の組を 2 回行い、表・numeric.ll・math.ll の SHA-256 がすべて一致。
- 最終の numeric.ll の SHA-256 は `be3c0efea152434c47b43d248a23b7f83042cc995f3e0184ca11ca9d9daa8b83`、math.ll は `d09fd9cbe02dd4d7a4d7a67d1ee95ed67c6c615c220654a841a13b448b84fe6f`。
- `... node tests/display_parse.mjs target/release/tsuzuri` は両方の Clang で、上の raw 検証に加えて共有コンソール出力・custom instances・owned cleanup・standalone links の終了行まで成功。巨大な default build-cache の走査で長時間待った初回を中止し、専用 cache で再実行した（runtime・最適化レベルは同じ）。
- `TMPDIR="$PWD/target/pr04-work" TSUZURI_CACHE_DIR="$PWD/target/pr04-work/interpolation-cache" ... node tests/features.mjs target/release/tsuzuri string_interpolation`: 82 件が native/WASM O0/O3 で成功。
- `TMPDIR="$PWD/target/pr04-work" TSUZURI_CACHE_DIR="$PWD/target/pr04-work/math-cache" ... node tests/math.mjs target/release/tsuzuri --quick`: 17,091 reference cases（5,856 elementary）が native/WASM O0/O3、non-NaN bit identity、trap-info・curried API で成功。
- ABI 修正後の `cargo test --lib abi::tests::reserved_symbols_cover_the_runtime_names` は 1 件、`cargo test --test display_parse` は 6 件、すべて成功（`--lib display_parse` は対象がないため、integration target を指定する）。
- 最後のテスト用 CRLF 処理の限定後は JavaScript 3 script の構文検査、所有差分の whitespace 検査、native O3 の 3 表示（f32/f64 0.1・u64 最大）と 2 解析を再確認し成功。runtime の SHA-256 は上の最終値から変更なし。
- 言語リファレンスは `types-and-type-inference/cast.md` を更新。`... node scripts/check-docs.mjs _tsuzuri/language-reference/types-and-type-inference/cast.md`: 1 page / 10 links / 8 checked examples / 16 native O0/O3 runs、成功。共通文書・全 Rust 回帰・正式計測は統合確認で追記する。

### C 候補の静的 object サイズ（正式速度計測とは別）

C 候補 IR と保存した基準 IR を同じ Clang でコンパイルしたサイズ。表そのものは 20,288 bytes で D4 の 24,576-byte 予算内。object の増分は新しいコードも含むため表のサイズより大きい。数値は `target/perf/PR04/object-sizes.json` に保存した。C 棄却後の最終値は末尾に記録する。

| target | opt | before (bytes) | after (bytes) | 増分 (bytes) |
|--------|-----|----------------|---------------|--------------|
| native | O0 | 84,152 | 109,920 | 25,768 |
| native | O3 | 50,800 | 73,368 | 22,568 |
| wasm32 | O0 | 83,958 | 108,987 | 25,029 |
| wasm32 | O3 | 69,915 | 93,029 | 23,114 |

### 初回 freeze 時の残りの受け入れ条件（下記で更新）

- 実装・参照・全数検証・生成の決定性は完了。正式な before/after/before2 の各 9 標本は他 stream との競合を避けるため未実行。
- D7 の段 C 採否、M1–M7 と G1/G2/G5 の実測判定は未完了。`--quick` の試験値や上の静的サイズを速度改善の根拠にしない。
- 全数 worker はすべて停止済み。統合担当が正式計測・中央文書・全体 gate・CI・公開を行うまで、この ticket を完了へ移動しない。

### 中央文書への統合用文言

- architecture / language: f32/f64 の最短十進表示は整数だけの Schubfach、符号を除く ASCII 表記が 25 bytes 以下で区切りなしの通常の十進解析は最大 19 有効桁の Eisel–Lemire acceptance-only 経路を使う。あいまいな丸め・長い係数や表記・区切り・overflow・不正な文法は従来の正確な経路へ戻す。先頭のゼロは有効桁に数えず、最初の非ゼロ以降の末尾ゼロは数える。f32 は f64 を経由せず直接丸める。
- architecture / language: f16/f128/decimal、明示の小数桁・指数書式は従来の正確な経路。NaN・inf・signed zero・ties-to-even・overflow と公開 ABI は保持する。生成する不変表は計 20,288 bytes で、ネイティブも WASM も同じ整数実装を使う。
- README / index: Display/Parse の通常 gate に加え、基準 IR を指定した bounded differential smoke と、任意の全 f32 / 大量 f64 の native O3 検証がある。全数実行を既定 CI に入れない。変更した manual は `types-and-type-inference/cast.md` だけ。
- benchmarks: 新しい `benchmarks/run-numeric.mjs` は native/WASM の F/P 1 回あたりの時間と O0/O3 object サイズを PX01 に記録する。起動・stdio は時間区間外だが、WASM は API 呼び出し費用を含む。正式な比率は同一環境の before/after/before2 の各 9 標本と drift を確認した後だけ記す。整数 8 桁経路は D7 採否確定前に成果として記述しない。

正式計測は統合担当の idle window で次を順番に実行する（after は最終 source の release compiler を用意してから）。`cpp/format_parse` は既存 runner の matched 計測と合わせる。

```sh
TMPDIR="$PWD/target/pr04-work" npx --yes --package=node@24 node benchmarks/run-numeric.mjs \
  --runtime target/perf/PR04/numeric-before.ll \
  --compiler target/perf/phase2-baseline/tsuzuri-before --metrics target/perf/PR04-before
TMPDIR="$PWD/target/pr04-work" npx --yes --package=node@24 node benchmarks/run-numeric.mjs \
  --compiler target/release/tsuzuri --metrics target/perf/PR04-after
TMPDIR="$PWD/target/pr04-work" npx --yes --package=node@24 node benchmarks/run-numeric.mjs \
  --runtime target/perf/PR04/numeric-before.ll \
  --compiler target/perf/phase2-baseline/tsuzuri-before --metrics target/perf/PR04-before2
```

### bounded CI gate の実行コマンド

前提は既存 gate の Node 24、`python3`、LLVM 21、wasm-ld、release compiler。Windows は既存 MSVC 環境と `TSUZURI_CLANG` / `TSUZURI_WASM_LD` を使う。全数 differential は native O3 だけであり、通常の `display_parse.mjs` が独立参照と compiler E2E を native/WASM O0/O3 で実行する。各 OS で次の 2 本を実行する。

shallow checkout の基準 commit を取得し、Node で blob の bytes を書く（PowerShell の redirect encoding を避ける）。

```sh
git fetch --no-tags origin a621fada69bf27f425bfbb012cc7d0a699721db6
node -e "const fs=require('node:fs'); fs.mkdirSync('target/numeric-ci',{recursive:true}); fs.writeFileSync('target/numeric-ci/before.ll',require('node:child_process').execFileSync('git',['show','a621fada69bf27f425bfbb012cc7d0a699721db6:src/runtime/numeric.ll'],{maxBuffer:64*1024*1024}));"
```

Linux / macOS（既存 targets gate の bash と LLVM_PREFIX）:

```sh
export TSUZURI_CLANG="$LLVM_PREFIX/bin/clang"
export TSUZURI_WASM_LD="${TSUZURI_WASM_LD:-$LLVM_PREFIX/bin/wasm-ld}"
export TMPDIR="$PWD/target/numeric-ci"
node tests/display_parse.mjs target/release/tsuzuri
node tests/display_parse_exhaustive.mjs --f32-samples 100000 --f64 100000 --jobs 2 --baseline-runtime target/numeric-ci/before.ll
```

Windows（既存 Windows gate の PowerShell、各失敗を確実に step failure にする）:

```powershell
$env:TEMP = Join-Path $PWD 'target\numeric-ci'
$env:TMP = $env:TEMP
node tests/display_parse.mjs target/release/tsuzuri.exe
if ($LASTEXITCODE -ne 0) { throw "Display/Parse gate failed: $LASTEXITCODE" }
node tests/display_parse_exhaustive.mjs --f32-samples 100000 --f64 100000 --jobs 2 --baseline-runtime target/numeric-ci/before.ll
if ($LASTEXITCODE -ne 0) { throw "Numeric differential gate failed: $LASTEXITCODE" }
```

- Windows の standalone host にも `wasm.ll` の i128 helper を付ける最後のテスト修正を行い、構文・whitespace 検査を再確認した。compiler/runtime は変更していないので release の再ビルドは不要。Windows の実行を local で検証したとは主張しない。
- 除外: `--f32`、`--f64 100000000`、正式 benchmark、速度閾値、共有 runner の before/after 評価。ASan/UBSan は別の opt-in Linux/macOS source gate とし、既定の Windows step へ要求しない。
- `--runtime-only` は compiler の統合待ち用なので、release を用意した通常 CI では付けず E2E も実行する。

## 正式計測と最終採否（2026-10-14）

### D7: C 候補を不採用

統合担当が直列に取得した `target/perf/phase2-formal/numeric-{before,after,before2}` は各 78 レコード、wall-time は各 9 標本。Apple Clang 21.0.0 / Node 24.21.0 / Apple M1 Max の条件と native checksum は一致する。基準 runtime SHA-256 は `1a0599b3f5bae1b9927268e720f2fd14871bff53812dbb597d202df7af518c94`、C 候補は `be3c0efea152434c47b43d248a23b7f83042cc995f3e0184ca11ca9d9daa8b83`。C 候補の C/IR は `target/perf/phase2-formal/numeric-c-candidate/` に保存した。

`R = max - min` とし、基準の安定は `|median(before2) - median(before)| <= max(Rbefore, Rbefore2)`。採用は `gain = median(before) - median(after)` が `max(Rbefore, Rafter)` と `max(Rbefore, Rbefore2)` の**双方を厳密に越える**ことを要求する。max/small を平均しない。

| target / workload | before ns | candidate ns | before2 ns | gain ns | before/after spread ns | baseline spread ns | drift gate | 採用 gate |
|-------------------|-----------|--------------|------------|---------|------------------------|--------------------|------------|-----------|
| native / i64u-format-max | 25.450 | 23.286 | 26.203 | 2.164 | 2.718 | 3.796 | 通過 | 不通過 |
| native / i64u-format-small | 9.278 | 9.588 | 9.161 | -0.310 | 1.975 | 1.975 | 通過 | 不通過 |
| wasm32 / i64u-format-max | 41.171 | 32.370 | 33.435 | 8.801 | 15.858 | 15.781 | 通過 | 不通過 |
| wasm32 / i64u-format-small | 23.863 | 16.860 | 15.772 | 7.003 | 12.628 | 12.628 | 通過 | 不通過 |

**結論: C は不採用。** 正しさは確認できたが、改善は noise gate を越えなかった。u64 の 8 桁塊を除去し、整数の本体を基準 commit の C ソースと文字単位で同一の `% 100` に戻した。u128 の処理は初めから変更していない。`tz_soft_format_spec` の十進整数を通常の formatter と共有する改善は、同等経路の保持のため残す。追加の桁の表はない。

### 低速経路の再走査を抑制

C 棄却後の最初の再計測 `numeric-final-*` では native `f64-parse-long` の baseline drift が gate 外だった。9 標本・各 1,000,000 回の focused triple で再検証すると、before 1371.601 ns、after 1391.625 ns、before2 1369.778 ns。gain -20.024 ns が before/after と baseline の両 spread 12.531 ns を越え、投機的 scanner の再走査による悪化を確認した。

`tz_soft_parse` は符号を除く ASCII 表記が 25 bytes を越える場合、scanner を呼ばず直接 exact parser へ進む。正規化した有効数字 19 桁の科学表記（19 桁 + 小数点 + e + 指数符号 + 3 桁の指数）は 25 bytes に収まり、f32/f64 の最短表示もこの限界内。長いゼロ埋め等の表記は exact path に戻す。受理集合・丸め・4096-byte 上限は変わらない。長い表記についての性能上の条件を manual に追記し、24/25/26-byte・各符号・19/20 有効桁・不正末尾の独立参照を 60 件追加した。

修正後の focused triple は long 1372.287 → 1373.263 → 1371.655 ns、underscore 1149.364 → 1149.822 → 1148.281 ns で両者とも gate 内。focused probe と最初の full 再計測は途中の診断記録であり、最終結果は以下を使う。

### 最終 matched triple

最終結果は **`target/perf/phase2-formal/numeric-final2-{before,after,before2}`**。3 本を同じ Clang/Node で直列に実行し、各 78 レコード、各 wall-time 9 標本、min/max/median の再計算、全キー・host 条件・runtime/compiler hash・native checksum 一致を検証した。解析の全行は `target/perf/phase2-formal/numeric-final2-analysis.json`。

最終 release compiler SHA-256 は `4fd9a7e1a07776ccf152f193a868ce0a115b3e4995b803cf2a59fbe8dc78f3c3`、numeric は `f34c50951d1737cda8129797f375c4312f6422c8336b293c35326f3c39291a05`、math は `eaef7b69866a230dd238ebf1169bed9ca2b4231275370af788f5bec2cfbf455c`。table hash は変わらない。metadata 最大は numeric 220 / math 288。fast formatter/parser と layout の本体は C 候補と番号を正規化して同一、math 全文は保存した基準と番号だけを正規化して同一。

#### M1 / M2: native の全固定入力

すべて baseline drift gate と改善 gate を通過。G1/G2 の `after/before <= 0.1` も全入力で満たした。

| workload | before ns | after ns | before2 ns | after/before |
|----------|-----------|----------|------------|--------------|
| f32-format-tenth | 805.675 | 21.630 | 805.920 | 0.0268 |
| f32-format-pi | 1463.755 | 24.735 | 1473.025 | 0.0169 |
| f32-format-max | 1657.055 | 24.885 | 1663.145 | 0.0150 |
| f32-format-min | 1063.930 | 14.275 | 1066.465 | 0.0134 |
| f64-format-tenth | 806.055 | 35.850 | 809.495 | 0.0445 |
| f64-format-pi | 2396.940 | 42.065 | 2406.785 | 0.0175 |
| f64-format-1e23 | 899.455 | 39.120 | 904.415 | 0.0435 |
| f64-format-min | 4824.900 | 18.080 | 4848.770 | 0.0037 |
| f64-format-max | 7884.560 | 41.660 | 7892.765 | 0.0053 |
| f64-format-integer | 1425.995 | 14.495 | 1430.785 | 0.0102 |
| f32-parse-tenth | 937.030 | 10.535 | 941.380 | 0.0112 |
| f32-parse-pi | 950.180 | 17.795 | 954.115 | 0.0187 |
| f32-parse-max | 1029.500 | 18.760 | 1037.480 | 0.0182 |
| f64-parse-tenth | 1059.710 | 10.580 | 1062.455 | 0.0100 |
| f64-parse-pi | 1243.265 | 28.515 | 1249.500 | 0.0229 |
| f64-parse-max | 3541.740 | 27.945 | 3559.715 | 0.0079 |
| f64-parse-min | 1432.585 | 10.965 | 1435.860 | 0.0077 |

native 表示は 22.48–266.86 倍、解析は 43.60–130.65 倍（各入力の比率。平均倍率ではない）。

#### M3 / M4 / M7: controls と WASM

| target / workload | before ns | after ns | before2 ns | 判定 |
|-------------------|-----------|----------|------------|------|
| native / f64-parse-long | 1367.795 | 1371.530 | 1374.530 | 差なし |
| native / f64-parse-underscore | 1141.885 | 1146.600 | 1150.810 | 差なし |
| wasm32 / f64-parse-long | 1319.298 | 1319.133 | 1319.546 | 差なし |
| wasm32 / f64-parse-underscore | 1272.293 | 1276.584 | 1270.465 | 差なし |
| native / i64u-format-max | 24.964 | 24.359 | 24.107 | 差なし |
| native / i64u-format-small | 8.872 | 8.873 | 9.108 | 差なし |
| wasm32 / i64u-format-max | 32.891 | 32.784 | 32.792 | 差なし |
| wasm32 / i64u-format-small | 15.690 | 15.747 | 15.593 | 差なし |

- f16/f128/decimal の F/P、i16u/i32u/i64u の F/P、long/underscore は **native/WASM とも全 control の baseline が安定、gate 外の悪化 0**。native f16/f128/d64 の 6 行はすべて差なし。整数解析本体は未変更で、観測した小さい改善を新しい解析アルゴリズムの成果とは主張しない。
- 最終 37 wall-time 行は native が改善 20 / 差なし 17 / 悪化 0 / drift 不明 0、WASM が改善 13 / 差なし 19 / 悪化 0 / drift 不明 5。
- WASM の 7 個の f32/f64 解析は全行で改善。表示は 5 行で改善、`f32-format-pi/max/min`・`f64-format-1e23/min` の 5 行は baseline drift が spread を越え、**判定できない**。この 5 行の見かけの比率を確かな改善として扱わない。M7 を全入力について確定できたとは主張しない。これは C の採否とは別で、C の max/small と全 control は安定している。
- M5 の `cpp/format_parse` と全プログラムの frontend/allocation/runtime 計測は統合担当が最終 compiler で行う。複数 stream の変更を含むため、その差を PR04 に単独帰属させない。

#### M6: 最終 standalone numeric object サイズ

| target | opt | before bytes | after bytes | 増分 bytes |
|--------|-----|--------------|-------------|------------|
| native | O0 | 84,152 | 109,568 | 25,416 |
| native | O3 | 50,800 | 73,016 | 22,216 |
| wasm32 | O0 | 83,958 | 108,611 | 24,653 |
| wasm32 | O3 | 69,915 | 92,736 | 22,821 |

これは指定した numeric IR だけを同じ Clang でコンパイルした **raw standalone object** の全 bytes（text だけではない）。生成表は引き続き 20,288 bytes で予算内。PB01 の math module 分離・dead stripping により最終リンクしたプログラムが縮む場合でも、この表の numeric object 自体は増加している。linked program のサイズ差と混同せず、math の除外による減少を numeric formatter 単独の成果として記さない。

### 最終 source の検証と freeze

- Apple Clang 21 / Homebrew Clang 23 の通常 `tests/display_parse.mjs target/release/tsuzuri`: 各 O0/O3 **109,803 表示 + 147,480 解析・往復**、native/WASM、コンソール・custom instances・owned cleanup・standalone links まで成功。
- ASan/UBSan、stack-use-after-return 有効の native C O0/O3 + 通常 WASM: 同じ集合で成功。
- `--f32-samples 100000 --f64 100000 --jobs 2 --baseline-runtime target/perf/PR04/numeric-before.ll`: 各 100,000 件、不一致 0。
- Native O3 の全 f32 4,294,967,296 + f64 100,000,000 件は初回記録の通り不一致 0。最終変更では同一の fast helper 本体を維持し、全 canonical 最短表記が 25-byte 条件内なので、その照合経路は変わらない。全数を再実行したとは主張しない。
- C 棄却後の interpolation 82 件、math quick 17,091 件（5,856 elementary）、native/WASM O0/O3 と trap-info/curried API は成功。math は guard 追加前後で SHA-256 が同一。
- 最終 `cargo test --test display_parse`: 6 件、ABI reservation: 1 件、成功。通常の test tool はこの Rust integration target を発見しなかったため Cargo で実行した。
- guard 追加後に表/numeric/math の生成を再度行い、SHA-256 の決定性を確認。manual `cast.md` は 1 page / 10 links / 8 examples / 16 native O0/O3 runs、成功。
- 中央文書の追加条件: 「符号を除く ASCII 表記 25 bytes 以下」を Eisel–Lemire の説明へ加える。整数 8 桁経路は出荷しない。最終 compiler hash と上の正式記録を使用し、85b181… / C 候補の `numeric-after` を最終 source の測定と扱わない。
- D7 決定と source は freeze。numeric worker はすべて終了済み。中央文書・全体 gate・M5・CI/公開・ticket の完了移動は統合担当。WASM 表示 5 行の drift 不明は実測の限界として残し、過剰な完了主張をしない。
