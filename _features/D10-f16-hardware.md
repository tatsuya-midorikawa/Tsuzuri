# D10: f16 のハードウェア演算経路

| 項目 | 内容 |
| --- | --- |
| ID | D10 |
| 優先度 | P3 |
| 規模 | M |
| 依存 | D03 |
| 後続 | F08, F09 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（Phase 1 は観測できる結果を変えない。Phase 2 は人間が求めた場合だけ着手） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: f16 が常にソフトウェア演算で、C/C++ の `_Float16`（AArch64 FP16 など）より遅い |
| 手本にする既存実装 | target 能力による経路の選択: `src/llvm_math.rs` の `math_builtin` の `MathFma` 分岐（D05。AArch64 native の f32/f64 だけ `llvm.fma`、他は `tz_soft_fma`）。直接命令の変換: `src/llvm.rs` の `FunctionEmitter::cast` の `fpext`／`fptrunc`。intrinsic 宣言の重複排除: `math_intrinsic` の `self.intrinsics`（`BTreeSet<String>`）。builtin 用 `Globals` への設定の引き継ぎ: `emit_typed_builtin` の `wasm: shared_globals.wasm`。CPU 設定: `src/driver.rs` の `Cpu`・`native_cpu_flag`。E2E の IR wrapper 方式: `tests/math.mjs` の `math_probe`・`host.c` |
| 主な影響ファイル | `src/driver.rs`, `src/llvm.rs`, `src/llvm_math.rs`, `src/test_runner.rs`, `tests/f16_hardware.rs`（新規）, `tests/f16_hardware.mjs`（新規）, `benchmarks/f16/Main.tz`（新規）, `benchmarks/f16/f16.c`（新規）, `benchmarks/run-f16.mjs`（新規）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `_docs/language-reference/numbers.md`, `_docs/feature-status.md`, `_features/README.md`。`src/runtime/numeric.c`・`numeric.ll`・`tests/numeric_casts.mjs` は変更しない |

## 目的

binary16 の意味（最近接・偶数丸め、NaN、符号付きゼロ、非正規化数）を保ったまま、FP16 算術命令（Arm の FEAT_FP16）を保証できる
native AArch64 で、f16 の四則演算・比較・`Math.sqrt`・f32／f64 との `as` 変換をハードウェア命令にする。
AGENTS.md の「意味を保つ場合は型付き LLVM 命令を優先する」方針を f16 に適用する。

結果は現在の soft 実装（`src/runtime/numeric.c`）とビット単位で同一にする。NaN の符号と payload も含む（D3）。
言語の意味は変わらず、変わるのは速度と生成コードだけである。

実装者は Phase 1 だけを実装する。Phase 2（FP16 のない AArch64 と x86-64 での binary32 経由・F16C 変換、AVX512-FP16）は設計方針だけを示し、
人間が求めた場合だけ着手する。WASM はどの Phase でも soft のまま（D6）。

## 着手条件と停止条件

### 着手条件

- D03 が `_features/README.md` の状態欄で done であること。確認: `grep -n "| D03 \|| D10 " _features/README.md` で D03 の行が `done`。
- 承認は不要。GUIDE §2.3 の基準コマンドが成功していること。
- 手順 1 で変更前のコンパイラを `/tmp/tz-d10/tsuzuri-before` へ複写していること（計測の soft 側と IR 比較に使う）。
- Phase 1 の hardware 経路を実行して照合できるのは FEAT_FP16 の AArch64 macOS だけである。確認: `sysctl -n hw.optional.arm.FEAT_FP16` が `1`
  （2026-09-30 の作業機で `1` を確認済み）。それ以外の host では E2E は soft 経路と参照の照合だけを行い、hardware 側は skip と表示する（手順 9）。

### 停止条件

次の場合は即興で回避せず、作業を止めて入力ビット列・コマンド・出力を添えて報告する（GUIDE §13）。

- hardware 経路の結果が soft 経路または JS 参照と 1 件でも一致しない（NaN を含む）。参照や期待値を合わせにいかない。
- NaN の補正（D3）だけでは一致させられない入力が見つかった。
- `src/runtime/numeric.c`・`numeric.ll` の変更、`numeric.ll`／`math.ll` の再生成が必要になった（Phase 1 は runtime を変えない）。
- macOS の `-O3` の機械語で f16 の四則演算が `fcvt s…, h…` と単精度演算に下がっている（clang が fullfp16 を有効にしていない）。
- f16 を使わないプログラムの IR が 1 byte でも変わった、または WASM の IR・`.wasm` が変わった。
- WASM の出力に `half`、`__extendhfsf2`・`__truncsfhf2`・`__truncdfhf2`、import が現れた。
- IR に `llvm.fma.f16`・`llvm.fmuladd.f16`、`sitofp`／`uitofp … to half`、`fptosi`／`fptoui half` が現れる設計が必要になった（D5）。
- `unsafe`、新しい crate、新しい CLI option・環境変数・IR コメントが必要になった（D2）。
- 既存テストの期待値を変える必要がある（Rust テストは既定の soft 経路のままなので変わらないはず。「既存テストへの影響」）。
- 計測で hardware 経路が soft 経路より遅い。調整せずに数値を報告する。

## 現状（HEAD `f8dc655` で確認）

### 型と値の表現

- f16 は `Type::Binary(16)`。LLVM の値の型は `i16` で、`src/llvm.rs` に `half` は一度も現れない（`grep -n "\bhalf\b" src/llvm.rs` は 0 件）。
  `tests/math.mjs` の wrapper も f16 を `i16` として受け渡す。関数の ABI は Phase 1 でも `i16` のまま変えない。
- `numeric_kind(&Type::Binary(16))` は `0`（`src/llvm.rs` の `numeric_kind`）。soft runtime の `format(0)` は精度 11、指数の最小 -24。

### 演算・比較・変換の経路

- 四則演算: `FunctionEmitter::binary` は `Type::Decimal(_) | Type::Binary(16 | 128)` のとき両辺を `spill` し、
  `call void @tz_soft_op(ptr out, ptr a, ptr b, i32 kind, i32 op)`（op は `+` 0、`-` 1、`*` 2、`/` 3）を出す。
- 比較: 同じ分岐で `call i32 @tz_soft_cmp(ptr a, ptr b, i32 kind)`（-1／0／1、非順序は 2）と `icmp` を出す。対応は
  `==` が `eq 0`、`!=` が `ne 0`、`<` が `slt 0`、`<=` が `sle 0`、`>` が `eq 1`、`>=` が `ule 1`。
- 単項 `-`: `xor i16 %v, 32768`（ビット演算。変更しない）。
- `as`: `FunctionEmitter::cast` は 8〜64-bit 整数と f32／f64 の間、f32 と f64 の間だけ直接命令・飽和 intrinsic を使い、f16 を含む変換は
  すべて `call void @tz_soft_cast(ptr out, ptr in, i32 from, i32 to)`。
- `Math`: `src/llvm_math.rs` の `math_builtin` で、f16 の `Math.sqrt` は `tz_soft_math_unary`（opcode 0）、`floor`／`ceil`／`trunc`／`round`／
  `round_even` も同じ関数、`min`／`max`／`clamp` は `tz_soft_math_binary`・`tz_soft_cmp`、`fma` は `tz_soft_fma`、`abs`／`copysign`／`is_nan`／
  `is_infinite`／`is_finite` はビット演算。`math_intrinsic` は宣言の型に `self.ty(ty)` を使うので、f16 に使うと `i16 @llvm.sqrt.f16(i16)` という
  誤った宣言になる。
- 定数評価（`const`）の f16 はコンパイラ内の別実装で、この変更の対象外。

### soft 実装の NaN と符号の規則

`src/runtime/numeric.c` の `tz_soft_op`・`tz_soft_math_unary`・`tz_soft_cast`・`special` から読み取った規則。hardware 経路はこの結果を
ビット単位で再現する（D3）。f16 の quiet NaN の基本形は `0x7E00`（32256）。

| 演算 | NaN 入力のとき | 新しく NaN を作る入力 | NaN の結果のビット |
| --- | --- | --- | --- |
| `+`／`-` | NaN | `inf + -inf`、`inf - inf` | `0x7E00` に左辺の符号ビットを付けた値 |
| `*`／`/` | NaN | `0 * inf`、`0 / 0`、`inf / inf` | `0x7E00` に「左辺の符号 xor 右辺の符号」を付けた値 |
| `Math.sqrt` | 入力のビットをそのまま返す（signaling NaN も quiet にしない） | 負の有限値、`-inf` | `0x7E00`（正） |
| f16 → f32／f64 | 入力の符号を保った canonical quiet NaN | なし | `0x7FC00000`／`0x7FF8000000000000` に入力の符号ビット。payload は捨てる |
| f32／f64 → f16 | 同上 | なし | `0x7E00` に入力の符号ビット |
| 比較 | 非順序（2） | なし | `!=` だけ true、他は false |

非 NaN の結果は IEEE 754 の最近接・偶数丸めで、`-0 - +0 = -0`、`x - x = +0`、65520 以上への丸めは無限大、非正規化数は保持する。
一方 AArch64 の命令は NaN の payload を伝播し（FPCR.DN = 0 の既定）、無効演算では正の `0x7E00` を返す。LLVM の LangRef も NaN の結果の符号と payload を
非決定的と定める。したがって命令の結果をそのまま使うと NaN のビットが soft と異なり、補正が要る。`tests/math.mjs` は native と WASM の一致を
非 NaN だけで確認している（`"non-NaN results are bit-identical"`）ので、NaN の差は既存テストでは検出されない。

### 経路を選ぶ材料

- `src/llvm.rs` の `EmitOptions { entry, wasm, debug_output }`。`emit_program` が `Globals { wasm, traps, cpu_dispatch, .. }` を作り、
  `emit_typed_builtin` は builtin 用に別の `Globals` を作って `wasm: shared_globals.wasm` だけを引き継ぐ。`emit`／`emit_target` は `EmitOptions` を
  内部で組み立てる（Rust テストはこの経路を使う）。`emit_test_runner(module, selected, wasm)` は `emit_selected` 経由。
- `src/driver.rs` の `Cpu::{Generic, Native}`、`native_cpu_flag`（arm／aarch64 は `-mcpu=native`）、`BuildOptions::validate`
  （`--cpu native` は native の実行形式・object だけ。`--emit llvm` とは併用できない）。driver は `llvm::EmitOptions` を 1 か所で組み立てる。
- `src/test_runner.rs` は `options.target` から `wasm` を決めて `llvm::emit_test_runner` を呼ぶ。
- `src/cache.rs` の cache key は options の `Debug` 表示、host の OS・arch、IR の全文を含む。IR が変われば key も変わる。
- D05 の前例は `cfg!(target_arch = "aarch64") && !self.globals.wasm` で、コンパイラ自身の arch で決めている。

### LLVM の `half` の下げ方（2026-09-30 に確認）

`fadd half`、`llvm.sqrt.f16`、`fptrunc double … to half`、`fpext half … to float`、`llvm.fma.f16` を含む IR を `-O2 -S` で下げた結果。

| target（clang） | `fadd half` | `llvm.sqrt.f16` | `fptrunc double` → `half` | `fpext half` → `float` | `llvm.fma.f16` |
| --- | --- | --- | --- | --- | --- |
| `arm64-apple-macos`（Homebrew clang 21・Apple clang 21、既定 `-target-cpu apple-m1`） | `fadd h0, h0, h1` | `fsqrt h0, h0` | `fcvt h0, d0` | `fcvt s0, h0` | `fmadd h0, h0, h1, h2` |
| `aarch64-linux-gnu`（既定 CPU generic） | `fcvt s`・`fadd s`・`fcvt h`（演算ごとに丸める） | `fcvt s`・`fsqrt s`・`fcvt h` | `fcvt h0, d0` | `fcvt s0, h0` | `fmadd s` の後に `fcvt h`（二重丸め） |
| `wasm32` | `__extendhfsf2`・`__truncsfhf2` の libcall と `f32.add` | libcall と `f32.sqrt` | `__truncdfhf2` | `__extendhfsf2` | `fmaf` の libcall |

### 再現（検証済み）

現在の f16 の下げ方。次の内容を `/tmp/tz-d10/cur/Main.tz` に保存する。`tests/fixtures/primitives/Main.tz` の `half_add` と同じ構文で、`build` が成功する。

```tsuzuri
export fn half_add(x: f32, y: f32) -> f32 { ((x as f16) + (y as f16)) as f32 }
export fn half_less(x: f32, y: f32) -> bool { (x as f16) < (y as f16) }
export fn half_sqrt(x: f32) -> f32 { Math.sqrt (x as f16) as f32 }
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-d10/cur --emit llvm -o /tmp/tz-d10/cur.ll
awk '/^define .*@tz\.(fn|builtin)\./{f=1} f{print} /^}/{f=0}' /tmp/tz-d10/cur.ll > /tmp/tz-d10/user.ll
grep -oE 'call (void|i32) @tz_soft_[a-z_]+' /tmp/tz-d10/user.ll | sort | uniq -c
grep -c '\bhalf\b' /tmp/tz-d10/user.ll
```

結果（生成関数だけ。埋め込まれた `numeric.ll` の内部呼び出しは数えない）: `tz_soft_cmp` 1、`tz_soft_cast` 7、`tz_soft_math_unary` 1、
`tz_soft_op` 1、`half` 0 行。生成関数は `@tz.fn.Main.half_add`・`half_less`・`half_sqrt`・`@tz.fn.$builtin.Math.sqrt.3`・
`@tz.builtin.Math.sqrt.f16`（`i16` を返す）。

LLVM の表は次の `h.ll` を `for t in arm64-apple-macos aarch64-linux-gnu wasm32; do /opt/homebrew/opt/llvm@21/bin/clang --target=$t -O2 -S h.ll -o -; done`
で再現できる。`sysctl -n hw.optional.arm.FEAT_FP16` は作業機で `1`。

```llvm
define half @add(half %a, half %b) {
  %r = fadd half %a, %b
  ret half %r
}
define half @sq(half %a) {
  %r = call half @llvm.sqrt.f16(half %a)
  ret half %r
}
define half @t64(double %a) {
  %r = fptrunc double %a to half
  ret half %r
}
define float @e(half %a) {
  %r = fpext half %a to float
  ret float %r
}
define half @fm(half %a, half %b, half %c) {
  %r = call half @llvm.fma.f16(half %a, half %b, half %c)
  ret half %r
}
declare half @llvm.sqrt.f16(half)
declare half @llvm.fma.f16(half, half, half)
```

### 文書の現状

- `docs/language.md` の数値の節: 「f16／f128 と decimal は同梱の整数ベース演算で処理し、binary64 で代用しません」「i128、f16／f128、decimal を含む
  変換は正確なソフトウェア処理を維持します」「NaN のペイロード、浮動小数点例外フラグ、丸めモードの変更は公開しません」。
- `_docs/language-reference/numbers.md` の `## decimal`: 「f16、f128、decimal の演算は同梱の整数ベース実装を使い」。
- `docs/architecture.md`: `numeric.c` の役割の表と、`fpext`／`fptrunc` の変換の段落、`tests/math.mjs` の f16 全 65536 パターンの記述。

## 仕様

### 前提とする他チケットのインターフェース

- D03（done）: `Math.sqrt` は `Float<'a>` の builtin で、`src/llvm_math.rs` の `math_builtin` が下げる。D05（done）: `Math.fma` の経路。
  f16 の `Math.fma` は Phase 1 でも `tz_soft_fma` のまま（D5）。todo のチケットには依存しない。
- 後続の F08・F09 には、driver の `f16_hardware`（新規）と `Globals::f16_hardware`（新規）を「f16 の hardware 命令を使ってよいか」の
  唯一の判定として提供する。後続は判定を複製しない。

### 経路の選択（Phase 1）

判定はコンパイル時に一度だけ行い、生成する IR を切り替える。実行時 dispatch はしない（D1）。

```text
f16_hardware(target, cpu) =
    target == native
    && host_arch == aarch64
    && (host_os == macos || (cpu == native && build host has FEAT_FP16))
```

| host と option | f16 の経路 | 理由 |
| --- | --- | --- |
| macOS arm64、`--cpu generic`（既定）と `--cpu native` | hardware | Apple Silicon はすべて FEAT_FP16 を持ち、clang の既定 CPU `apple-m1` が fullfp16 を有効にする（確認済み）。未対応命令にはならない |
| Linux／Windows の AArch64、`--cpu generic` | soft | Cortex-A53・A72 など FEAT_FP16 のない CPU が残る |
| Linux／Windows の AArch64、`--cpu native` | build 機が FEAT_FP16 なら hardware、なければ soft | 判定は `std::arch::is_aarch64_feature_detected!("fp16")`。clang には既存の `-mcpu=native` が渡る |
| x86-64（全 `--cpu`） | soft | F16C は変換命令だけ。算術は AVX512-FP16 が要る（D7。Phase 2） |
| `--target wasm32` | soft | WASM MVP と SIMD128 に binary16 演算はない（D6） |

- `tsuzuri build --emit llvm` は `--cpu generic` なので、macOS arm64 では hardware 経路の IR を出す（IR で経路を確認できる）。
- `tsuzuri test` には `--cpu` がないので `Cpu::Generic` として同じ規則を使う（`TestOptions` は `target` だけを持つ）。
- 新しい CLI option・環境変数・IR コメントは足さない（D2）。経路は IR の `half` の有無で分かる。
- Rust のライブラリ API `llvm::emit`・`llvm::emit_target` は常に soft 経路を出す（D8）。

### 対象の演算（Phase 1）

| 式（`x`・`y` は f16、`s` は f32、`d` は f64） | HEAD の下げ方 | hardware 経路 | NaN の補正（D3） |
| --- | --- | --- | --- |
| `x + y`・`x - y` | `tz_soft_op` op 0／1 | `fadd`／`fsub half` | 結果が NaN なら `0x7E00` に `x` の符号 |
| `x * y`・`x / y` | `tz_soft_op` op 2／3 | `fmul`／`fdiv half` | 結果が NaN なら `0x7E00` に `x` xor `y` の符号 |
| `x == y`・`!=`・`<`・`<=`・`>`・`>=` | `tz_soft_cmp` と `icmp` | `fcmp oeq`／`une`／`olt`／`ole`／`ogt`／`oge half` | 不要（結果は `i1`） |
| `Math.sqrt x` | `tz_soft_math_unary` opcode 0 | `call half @llvm.sqrt.f16` | 入力が NaN なら入力のビット、結果が NaN なら `0x7E00` |
| `x as f32`・`x as f64` | `tz_soft_cast` | `fpext half` | 入力が NaN なら canonical quiet NaN に `x` の符号 |
| `s as f16`・`d as f16` | `tz_soft_cast` | `fptrunc float`／`double … to half` | 入力が NaN なら `0x7E00` に入力の符号 |

- `tz_soft_op` の呼び出しは `FunctionEmitter::binary` の 1 か所、`tz_soft_cast` は `FunctionEmitter::cast` の 1 か所、`tz_soft_math_unary` は
  `math_builtin` の 1 か所だけである（grep で確認済み）。複合代入、`Ord` による整列、std の関数も演算子と同じ `binary`・`cast` を通るので、
  演算子と同等の builtin が別の経路を選ぶことはない（AGENTS.md）。
- soft のまま残すもの（D5）: `Math.fma`、`Math.min`／`max`／`clamp`／`floor`／`ceil`／`trunc`／`round`／`round_even`、整数と f16 の変換、
  f16 と f128・decimal の変換、定数評価、表示・解析、`Hash`。単項 `-` と `abs`／`copysign`／`is_*` は現状どおりビット演算。

### 数値・トラップ・native と WASM の差

- 観測できる結果（非 NaN の値と符号、NaN の符号と payload）は、全 target・`-O0`／`-O3` で HEAD の soft 経路とビット単位で同一。トラップは関係しない。
- 丸め環境は docs/language.md の「ホストの標準の丸め環境を前提とします」に従う。AArch64 の既定の FPCR（最近接・偶数、FZ16 = 0 で非正規化数を保持、
  DN = 0）を前提とし、FPCR を書き換えない。
- WASM の IR と `.wasm` は byte 単位で変わらない。f16 を使わないプログラムの native IR も変わらない。
- fast-math のフラグ（`fast`・`reassoc`・`contract`・`afn`・`nnan`・`ninf`）は付けない。
- 二重丸めの条件と Phase 1 の扱い。四則演算と平方根は binary32 の精度 24 bit が 2 × 11 + 2 以上なので、backend が f32 へ昇格しても
  演算ごとに丸め直せば正しい（上の `aarch64-linux-gnu` の列がこの形）。次の形は二重丸めになりうるので Phase 1 では IR に出さない。

| 形 | 何が起きるか | Phase 1 |
| --- | --- | --- |
| `llvm.fma.f16`・`llvm.fmuladd.f16` を fullfp16 のない backend で下げる | f32 の `fmadd` の後に `fcvt h`（確認済み）。積和の厳密値が 24 bit を超えると二重丸め | 出さない（D5） |
| `fptrunc double` → `float` → `half` の 2 段 | f32 への丸めの後に f16 へ丸める | 出さない。AArch64 の `fptrunc double … to half` は `fcvt h0, d0` の 1 命令（確認済み） |
| 複数の f16 演算を f32 のまま続けて最後に 1 回だけ丸める | 中間の丸めが消える（C の `_Float16` の excess precision と同じ） | 出さない。f16 の演算 1 回ごとに `half` の命令を出す |
| `sitofp`／`uitofp` の `half` への変換 | backend により f32 を経由しうる（未検証） | 出さない（整数との変換は soft） |
| fast-math のフラグ | 再結合・縮約 | 出さない |

### 診断

新しい診断はない。`--cpu native` の既存の `E2000`（`'--cpu native' requires native executable or object output` など）も変えない。

### 資源上限

新しい上限はない。hardware 経路は演算 1 回あたり 7〜9 命令の SSA 値を出し、soft 経路の `alloca`（`spill`・`slot`）を使わない。

### 例

「再現」の `Main.tz` を macOS arm64 で `--emit llvm` にすると、実装後は生成関数から `tz_soft_op`・`tz_soft_cmp`・`tz_soft_cast`・
`tz_soft_math_unary` が消え、`fadd half`・`fcmp olt half`・`call half @llvm.sqrt.f16`・`fpext half`・`fptrunc float` が現れる（手順 8 で確認）。
`--target wasm32` では HEAD と同じ IR のままである。

## 設計

### データ構造

```rust
// src/llvm.rs
pub struct EmitOptions {
    pub entry: Entry,
    pub wasm: bool,
    pub debug_output: bool,
    pub f16_hardware: bool, // 新規。driver だけが true にする
}

struct Instrumentation<'a> {
    // 既存の traps・debug・cpu_dispatch・wasm_threads に加えて
    f16_hardware: bool, // 新規。#[derive(Default)] で false
}

struct Globals {
    // 既存の field に加えて
    f16_hardware: bool, // 新規。Default は false。wasm のときは常に false
}

// src/driver.rs
pub(crate) fn f16_hardware(target: Target, cpu: Cpu) -> bool {
    target == Target::Native
        && (cfg!(all(target_arch = "aarch64", target_os = "macos"))
            || (cpu == Cpu::Native && native_fp16()))
}

#[cfg(target_arch = "aarch64")]
fn native_fp16() -> bool {
    std::arch::is_aarch64_feature_detected!("fp16")
}

#[cfg(not(target_arch = "aarch64"))]
fn native_fp16() -> bool {
    false
}
```

`f16_hardware`（新規）と `native_fp16`（新規）の形は上のとおりにする。`unsafe` も crate も要らない。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| driver | `src/driver.rs` | `f16_hardware`（新規）、`native_fp16`（新規） | 上の定義。単体テストを driver の `#[cfg(test)]` へ足す（手順 2） |
| driver | `src/driver.rs` | `llvm::EmitOptions { .. }` を組み立てる箇所（1 か所） | `f16_hardware: f16_hardware(options.target, options.cpu)` |
| test runner | `src/test_runner.rs` | `llvm::emit_test_runner` の呼び出し | 第 4 引数 `crate::driver::f16_hardware(options.target, Cpu::Generic)` |
| emit | `src/llvm.rs` | `EmitOptions` | field `f16_hardware` |
| emit | `src/llvm.rs` | `emit_target` | `f16_hardware: false`（D8。`emit` もこれを通る） |
| emit | `src/llvm.rs` | `Instrumentation`、`emit_selected`、`emit_test_runner`、`EmitOptions` を受ける `emit_with_options`・`emit_native_build`・`emit_with_debug_info`・`emit_with_trap_info`・`emit_wasm_threads_build` | `Instrumentation { f16_hardware: options.f16_hardware, .. }` で `emit_program` へ渡す。`emit_test_runner` は引数 `f16_hardware: bool` を足す |
| emit | `src/llvm.rs` | `emit_program` の `Globals { .. }` | `f16_hardware: instrumentation.f16_hardware && !wasm` |
| emit | `src/llvm.rs` | `emit_typed_builtin` の builtin 用 `Globals { .. }` | `f16_hardware: shared_globals.f16_hardware`（忘れると `Math.sqrt` だけ soft に残る） |
| emit | `src/llvm.rs` | `Globals`、`impl Default for Globals` | field と `false` |
| emit | `src/llvm.rs` | `FunctionEmitter::binary` | `let ty = self.ty(&left.ty);` の直後、soft 分岐の前に `if self.globals.f16_hardware && matches!(left.ty, Type::Binary(16)) { return self.half_binary(operator, &lhs, &rhs); }` |
| emit | `src/llvm.rs` | `FunctionEmitter::cast` | `let input = self.spill(..)`（soft の fallback）の直前に、`(Binary(16), Binary(32 \| 64))` と `(Binary(32 \| 64), Binary(16))` で `self.half_cast(&value, &expression.ty, to)` を返す分岐 |
| emit | `src/llvm_math.rs` | `math_builtin` | `tz_soft_math_unary` の fallback の直前に、`operation == MathSqrt && matches!(ty, Type::Binary(16)) && self.globals.f16_hardware` で `self.half_sqrt("%arg0")` を返す |
| emit | `src/llvm_math.rs` | `half_binary`・`half_cast`・`half_sqrt`・`half_nan`（すべて新規、`pub(super)` は前 3 つ） | 次節の IR を出す。`math_intrinsic` は使わない（宣言が `i16` になる） |
| 変更なし | `src/llvm.rs`・`src/llvm_math.rs` | 単項 `-`、`numeric_kind`、`math_minmax`、`MathFma`・`MathClamp`・丸め系、`llvm_type` | f16 は `i16` のまま。soft 経路の分岐は残す |
| 変更なし | `src/runtime/numeric.c`・`numeric.ll`、`src/llvm_debug.rs`、`src/cache.rs` | — | runtime は変えない。DWARF は `i16` の保存形のまま。cache key は IR 全文を含むので自動で変わる |

### 生成 IR とランタイム

値の名前は `self.value` が振る番号で、下の `%1` などは説明用である。f16 の値は `i16`、f32／f64 は `float`／`double` のまま受け渡す。

四則演算（`x + y`）。`-` は `fsub`。`*`・`/` は `fmul`／`fdiv` で、符号は `xor i16 %x, %y` の結果から取る。

```llvm
%1 = bitcast i16 %x to half
%2 = bitcast i16 %y to half
%3 = fadd half %1, %2
%4 = bitcast half %3 to i16
%5 = fcmp uno half %3, %3
%6 = and i16 %x, -32768
%7 = or i16 %6, 32256
%8 = select i1 %5, i16 %7, i16 %4
```

比較（`x < y`）。結果は `i1` で補正は要らない。

```llvm
%1 = bitcast i16 %x to half
%2 = bitcast i16 %y to half
%3 = fcmp olt half %1, %2
```

`Math.sqrt`（builtin 関数 `@tz.builtin.Math.sqrt.f16` の本体）。宣言 `declare half @llvm.sqrt.f16(half)` は `self.intrinsics` へ入れる。

```llvm
%1 = bitcast i16 %arg0 to half
%2 = call half @llvm.sqrt.f16(half %1)
%3 = bitcast half %2 to i16
%4 = fcmp uno half %2, %2
%5 = select i1 %4, i16 32256, i16 %3
%6 = fcmp uno half %1, %1
%7 = select i1 %6, i16 %arg0, i16 %5
```

f16 → f32（f64 は `zext … to i64`、`shl i64 …, 48`、`or i64 …, 9221120237041090560`、`bitcast i64 … to double`）。

```llvm
%1 = bitcast i16 %x to half
%2 = fpext half %1 to float
%3 = fcmp uno half %1, %1
%4 = and i16 %x, -32768
%5 = zext i16 %4 to i32
%6 = shl i32 %5, 16
%7 = or i32 %6, 2143289344
%8 = bitcast i32 %7 to float
%9 = select i1 %3, float %8, float %2
```

f32 → f16（f64 は `bitcast double … to i64`、`lshr i64 …, 48`、`trunc i64 … to i16`）。

```llvm
%1 = fptrunc float %s to half
%2 = bitcast half %1 to i16
%3 = fcmp uno float %s, %s
%4 = bitcast float %s to i32
%5 = lshr i32 %4, 16
%6 = trunc i32 %5 to i16
%7 = and i16 %6, -32768
%8 = or i16 %7, 32256
%9 = select i1 %3, i16 %8, i16 %2
```

定数は 10 進で書く（`32256` = `0x7E00`、`2143289344` = `0x7FC00000`、`9221120237041090560` = `0x7FF8000000000000`）。`half` の 16 進 literal
（`0xH…`）は使わない。runtime（`numeric.ll`）は変えず、soft 経路の関数は他の形のために従来どおり埋め込まれる。

### アルゴリズム

```text
half_nan(nan: i1, sign: i16, result: i16) -> i16:       // 補正の共通部分
    canonical = or(and(sign, -32768), 32256)
    return select(nan, canonical, result)

half_binary(op, x, y):
    a, b = bitcast x, y to half
    if op is comparison: return fcmp <pred> half a, b
    r = f<op> half a, b
    sign = x                  if op in {+, -}
           xor(x, y)          if op in {*, /}
    return half_nan(fcmp uno r, r, sign, bitcast r to i16)

half_cast(v, from, to):
    f16 -> f32/f64: fpext と、入力が NaN なら符号付き canonical quiet NaN の select
    f32/f64 -> f16: fptrunc と、half_nan(fcmp uno v, v, 入力の上位 16 bit, 結果)

half_sqrt(v): 上の IR（入力 NaN を最優先し、次に結果 NaN を 0x7E00 にする）
```

### Phase 2（設計方針。人間が求めた場合だけ着手）

- FP16 のない AArch64（Linux／Windows の `--cpu generic`）: Phase 1 と同じ IR が演算ごとの昇格で正しく下がる（確認済みの形）。`fcvt` は ARMv8 の基本命令。
  Linux AArch64 の実機で E2E を通すことを条件に、判定を「native AArch64 すべて」へ広げる。FMA は含めない。
- x86-64: `--cpu native` と `is_x86_feature_detected!("f16c")` のとき f16 → f32／f64 と f32 → f16 だけを `vcvtph2ps`／`vcvtps2ph` にする。
  f64 → f16 は直接命令がなく f32 経由は二重丸めなので soft のまま。算術は `avx512fp16` のときだけ `half` の命令にする。x86 実機での E2E が条件（D7）。
- F16C のない x86-64 と WASM での binary32 経由（`fpext` → 演算 → `fptrunc`）は、変換が `__extendhfsf2`・`__truncsfhf2` の libcall になるので採らない（D6）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。作業用の path は `/tmp/tz-d10/` に置く。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、旧コンパイラを複写する。「再現」の `Main.tz` を `/tmp/tz-d10/cur/` に、f16 を使わない
  `export fn twice(x: f32) -> f32 { x + x }`（`build` 成功を確認済み）を `/tmp/tz-d10/plain/Main.tz` に置く。
- 確認: 次がすべて成功し、最後の行が `0` を出す（HEAD の `-O3` object は soft 呼び出しの `bl` だけ）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && cargo test --locked
cp target/release/tsuzuri /tmp/tz-d10/tsuzuri-before
for p in cur plain; do
  target/release/tsuzuri build /tmp/tz-d10/$p --emit llvm -o /tmp/tz-d10/before-$p.ll
  target/release/tsuzuri build /tmp/tz-d10/$p --target wasm32 --emit llvm -o /tmp/tz-d10/before-$p-wasm.ll
done
target/release/tsuzuri build /tmp/tz-d10/cur --emit object -O3 -o /tmp/tz-d10/before-cur.o
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d /tmp/tz-d10/before-cur.o | grep -cE '\s(fadd|fsqrt|fcvt)\s'
```

### 手順 2: 判定と受け渡し（IR はまだ変えない）

- 変更: `src/driver.rs` の `f16_hardware`・`native_fp16`（新規）と `llvm::EmitOptions` の組み立て、`src/llvm.rs` の `EmitOptions`・
  `Instrumentation`・`Globals`・`emit_target`・`emit_selected`・`emit_test_runner`・`EmitOptions` を受ける `emit_*`・`emit_program`・
  `emit_typed_builtin`、`src/test_runner.rs`。driver の `#[cfg(test)]` に `f16_hardware_follows_target_and_host`（新規）。
- 内容: 「段ごとの変更」の driver・test runner・emit の行のうち `binary`・`cast`・`math_builtin` 以外。構築箇所は
  `grep -rn "EmitOptions {" src tests benchmarks` と `grep -n "emit_selected(\|emit_program(\|Instrumentation {\|Globals {" src/llvm.rs` で洗い出す。
  テストは wasm32 では両 `Cpu` で false、native・`Cpu::Generic` では `cfg!(all(target_arch = "aarch64", target_os = "macos"))` と等しい、
  aarch64 以外の native・`Cpu::Native` では false、を確かめる。
- 確認: `cargo test --locked --lib f16_hardware_follows_target_and_host` が `1 passed`。`cargo build --release --locked` の後、
  `target/release/tsuzuri build /tmp/tz-d10/cur --emit llvm -o /tmp/tz-d10/step2.ll && cmp /tmp/tz-d10/before-cur.ll /tmp/tz-d10/step2.ll`
  が差分なし。`cargo test --locked` が成功する。

### 手順 3: Rust テストの骨組み

- 変更: `tests/f16_hardware.rs`（新規）。
- 内容: `tests/types_ownership.rs` と同じ import で `analyze` と `llvm` を使う。helper `emit_f16(source, f16_hardware, wasm) -> String`（新規。
  `llvm::EmitOptions { entry: llvm::Entry::Library, wasm, debug_output: false, f16_hardware }` で `llvm::emit_with_options`）と
  `body(ir, name) -> &str`（新規。`@tz.fn.Main.{name}(` を含む `define` 行から次の `\n}` まで）を置き、「Rust テスト」の 1〜3 を書く。
- 確認: `cargo test --locked --test f16_hardware` が `3 passed`。

### 手順 4: 四則演算と比較

- 変更: `src/llvm_math.rs` の `half_binary`・`half_nan`（新規）、`src/llvm.rs` の `FunctionEmitter::binary`。
- 内容: 「生成 IR とランタイム」の形を出す。分岐は両辺の `self.expression` の後に置き、左から右の評価順を保つ。比較の述語は
  `Equal` → `oeq`、`NotEqual` → `une`、`Less` → `olt`、`LessEqual` → `ole`、`Greater` → `ogt`、`GreaterEqual` → `oge`。Rust テスト 4・5 を足す。
- 確認: `cargo test --locked --test f16_hardware` が `5 passed`。`cargo test --locked --test types_ownership` と `--test math` が成功する（既定は soft）。

### 手順 5: `Math.sqrt`

- 変更: `src/llvm_math.rs` の `half_sqrt`（新規）と `math_builtin`。
- 内容: 宣言は `self.intrinsics.insert("declare half @llvm.sqrt.f16(half)".into())`。入力 NaN の判定を最後の `select` に置く。Rust テスト 6 を足す。
- 確認: `cargo test --locked --test f16_hardware` が `6 passed`。

### 手順 6: f32／f64 との変換

- 変更: `src/llvm_math.rs` の `half_cast`（新規）、`src/llvm.rs` の `FunctionEmitter::cast`。
- 内容: 4 方向の IR を出す。f64 → f16 は `fptrunc double … to half` の 1 命令だけで、`float` を経由しない。Rust テスト 7 を足す。
- 確認: `cargo test --locked --test f16_hardware` が `7 passed`。

### 手順 7: Phase 1 の外が soft のままであること

- 変更: `tests/f16_hardware.rs` だけ。
- 内容: Rust テスト 8 を足す。
- 確認: `cargo test --locked --test f16_hardware` が `8 passed`。`cargo test --locked` が成功する。

### 手順 8: 生成コードの確認（macOS arm64）

- 変更: なし。
- 確認: 1 行目の数が `0`、2 行目が `5` 以上、`cmp` 3 回が差分なし、最後の出力に `fadd h`、`fcmp h`、`fsqrt h`、`fcvt h…, s…`、`fcvt s…, h…` がある。

```sh
cargo build --release --locked
target/release/tsuzuri build /tmp/tz-d10/cur --emit llvm -o /tmp/tz-d10/after-cur.ll
awk '/^define .*@tz\.(fn|builtin)\./{f=1} f{print} /^}/{f=0}' /tmp/tz-d10/after-cur.ll | grep -cE 'call (void|i32) @tz_soft_'
grep -cE 'fadd half|fcmp olt half|call half @llvm.sqrt.f16|fpext half|fptrunc float' /tmp/tz-d10/after-cur.ll
target/release/tsuzuri build /tmp/tz-d10/plain --emit llvm -o /tmp/tz-d10/after-plain.ll
cmp /tmp/tz-d10/before-plain.ll /tmp/tz-d10/after-plain.ll
for p in cur plain; do
  target/release/tsuzuri build /tmp/tz-d10/$p --target wasm32 --emit llvm -o /tmp/tz-d10/after-$p-wasm.ll
  cmp /tmp/tz-d10/before-$p-wasm.ll /tmp/tz-d10/after-$p-wasm.ll
done
target/release/tsuzuri build /tmp/tz-d10/cur --emit object -O3 -o /tmp/tz-d10/after-cur.o
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d /tmp/tz-d10/after-cur.o | grep -E '\s(fadd|fsqrt|fcmp|fcvt)\s+[hs]'
```

### 手順 9: E2E `tests/f16_hardware.mjs`

- 変更: `tests/f16_hardware.mjs`（新規）。
- 内容: `tests/math.mjs` の構造（定義の生成、`--emit llvm` の native・wasm32 IR、`math_probe` 型の IR wrapper、`host.c`、WASM の実行、
  native／WASM × `-O0`／`-O3`）を写し、「E2E」の全ケースを流す。関数の定義は次の形で生成する（`check` 成功を確認済み）。

```tsuzuri
def op_add :: f16 -> f16 -> f16
fn op_add value0 value1 = value0 + value1
def op_lt :: f16 -> f16 -> bool
fn op_lt value0 value1 = value0 < value1
def op_sqrt :: f16 -> f16
fn op_sqrt value0 = Math.sqrt value0
def op_wide :: f16 -> f64
fn op_wide value0 = value0 as f64
def op_narrow :: f64 -> f16
fn op_narrow value0 = value0 as f16
```

- 確認: `node tests/f16_hardware.mjs target/release/tsuzuri` が成功し、
  `f16_hardware: <N> cases, native hardware|soft and WASM soft, O0/O3, bit identity including NaN` を出す（macOS arm64 では `hardware`）。
  N は 1,000,000 以上。`--quick` は 30,000 未満で終わる。

### 手順 10: 既存の E2E と計測

- 変更: `benchmarks/f16/Main.tz`・`benchmarks/f16/f16.c`・`benchmarks/run-f16.mjs`（新規）。
- 内容: 既存の `node tests/math.mjs target/release/tsuzuri`、`node tests/primitives.mjs target/release/tsuzuri`、
  `node tests/display_parse.mjs target/release/tsuzuri`、`node tests/features.mjs target/release/tsuzuri` を README.md の手順どおり流す。
  その後「性能」の計測をする。
- 確認: 4 つの E2E が HEAD と同じ出力行で成功する。`node benchmarks/run-f16.mjs /tmp/tz-d10/tsuzuri-before target/release/tsuzuri` が
  3 行（soft、hardware、C）と同じ checksum を出す。

### 手順 11: 文書と最終確認

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/language-reference/numbers.md _docs/feature-status.md` と `cargo test --locked` が成功し、
  GUIDE §10 の完了の定義を満たす。

## テスト計画

### Rust テスト

`tests/f16_hardware.rs`（新規）。source は `def f :: f16 -> f16 -> f16` と `fn f x y = x + y` の形で作る。

| # | テスト名 | 確かめること |
| --- | --- | --- |
| 1 | `default_emission_and_wasm_stay_soft` | `llvm::emit`、`emit_f16(.., false, false)`、`emit_f16(.., true, true)` の IR に `call void @tz_soft_op` があり、`half` がない |
| 2 | `f16_free_programs_are_unchanged` | f16 を使わない source の IR が `f16_hardware` の true と false で一致する |
| 3 | `emission_is_deterministic` | `emit_f16(.., true, false)` を 2 回出して一致する |
| 4 | `emits_half_arithmetic_when_enabled` | 4 演算の関数本体に `fadd`／`fsub`／`fmul`／`fdiv half`、`fcmp uno half`、`32256` があり、`tz_soft_op` がない。`*`・`/` の本体に `xor i16` がある |
| 5 | `emits_half_comparisons_with_ordered_predicates` | 6 比較の本体に `fcmp oeq`／`une`／`olt`／`ole`／`ogt`／`oge half` がそれぞれあり、`tz_soft_cmp` がない |
| 6 | `emits_half_sqrt_and_declares_intrinsic_once` | 二つの関数で `Math.sqrt` を使い、`declare half @llvm.sqrt.f16(half)` がちょうど 1 行、builtin 本体に `tz_soft_math_unary` がない |
| 7 | `emits_half_conversions_with_nan_canonicalization` | `fpext half … to float`／`double`、`fptrunc float`／`double … to half`、`2143289344`、`9221120237041090560` があり、f64 → f16 の本体に `to float` がない |
| 8 | `keeps_software_paths_outside_phase_one` | true でも f16 の `Math.fma` は `tz_soft_fma`、`Math.min` は `tz_soft_math_binary`、`x as i32` と `n as f16`（n は i64）と f16 ↔ f128 は `tz_soft_cast`。IR に `llvm.fma.f16`、`fmuladd`、`to half` の `sitofp`／`uitofp`、fast-math のフラグ（`contract`・`fast`・`reassoc`）がない |

driver の単体テスト `f16_hardware_follows_target_and_host`（新規）は手順 2 のとおり。拒否ケースと新しい診断はない。

### E2E

`tests/f16_hardware.mjs`（新規。dedicated suite で `features.mjs` には足さない）。期待値は JS の独立な参照で作り、コンパイラの出力から作らない。

- 参照: `numberFromF16`（新規）と `f16FromNumber`（新規。double から最近接・偶数で f16 のビットへ。2 の冪での拡大縮小と `Math.floor` だけで
  正確に計算する）。f16 の和・差・積は double で正確、商と平方根は double へ丸めてから f16 へ丸めても 53 ≥ 2 × 11 + 2 なので正しい。
  NaN の期待値は「soft 実装の NaN と符号の規則」の表から作る。Node 20.19.6 には `Math.f16round` がない（確認済み）ので使わない。
- 単項（全数）: `Math.sqrt`、`as f32`、`as f64` を f16 の全 65,536 ビット列で。
- f32 → f16: 全 f16 値を広げた 65,536 件、隣り合う正の有限 f16 の中点とその ±1 f32 ulp、`65504`・`65519.99…`・`65520`・`65536`・最大 f32、
  `2^-25` とその ±1 ulp、`0x7FC00001`・`0x7F800001`・`0xFFFFFFFF`・`0xFF800001`、±inf、乱数 65,536 件。
- f64 → f16: 同じ集合に加え、中点 m に m の ulp の `2^-30` 倍を足し引きした「二重丸めの罠」。f32 へ丸めると m になるので、
  `f16FromNumber(Math.fround(d))` と直接の参照が 1,000 件以上で異なることもテスト内で確かめる（罠が本物である証拠）。
- 二項: 境界 64 値（大きさ `0x0000`・`0x0001`・`0x0002`・`0x0200`・`0x03FF`・`0x0400`・`0x0401`・`0x07FF`・`0x1400`・`0x1401`・`0x3800`・
  `0x3BFF`・`0x3C00`・`0x3C01`・`0x3C02`・`0x3E00`・`0x4000`・`0x4200`・`0x4C00`・`0x5BFF`・`0x5C00`・`0x7800`・`0x7BFE`・`0x7BFF`・`0x7C00`・
  `0x7C01`・`0x7D00`・`0x7DFF`・`0x7E00`・`0x7E01`・`0x7F00`・`0x7FFF` と両符号）の全 4,096 組を 10 演算で。乱数は四則演算ごとに 262,144 組、
  比較ごとに 65,536 組（`tests/math.mjs` と同じ LCG）。
- 照合: native と WASM の各 `-O0`／`-O3` の結果が参照と NaN を含めて全ビット一致する。WASM module の import は空。WASM の IR に `half` がない。
  macOS arm64（`process.platform === "darwin" && process.arch === "arm64"`）では native IR に `fadd half`・`fsub half`・`fmul half`・`fdiv half`・
  `@llvm.sqrt.f16`・`fpext half`・`fptrunc double` があることを確かめ、他の host では `hardware path skipped` を表示する。
  `declare` 行の重複なし、fast-math のフラグなし、生成した演算の本体に `@tz.alloc(` なし。

### 既存テストへの影響

- Rust テスト: なし。`llvm::emit`・`emit_target` は soft 固定（D8）なので、`tests/types_ownership.rs` の
  `retains_exact_software_conversions_for_wide_and_decimal_types`（`("f16", "f32")`・`("f64", "f16")` を含む）と `tests/math.rs` はそのまま通る。
- E2E: 期待値の変更なし。macOS arm64 では `tests/math.mjs` の f16 `sqrt`、`tests/primitives.mjs` の `half_add`・`half_divide`、
  `tests/fixtures/constants/Main.tz` の `float_reference` などが hardware 経路で実行されるようになる。変更が要るなら停止条件。

### 性能

- workload: 次の kernel を `benchmarks/f16/Main.tz`（新規）に置き、要素 1,000,000 の `f64` 配列で呼ぶ。`ref [f32]` は export できず
  `E1008` になる（確認済み）ので `ref [f64]` を使う。次の形は `check` 成功を確認済み。

```tsuzuri
export def half_chain :: ref [f64] -> f64
fn half_chain values =
    let mut total = 0.0f16
    for value in values do total = total * 0.9990234375f16 + (value as f16)
    total as f64
```

- 比較: 旧コンパイラ（soft）、新コンパイラ（hardware）、`benchmarks/f16/f16.c`（新規）の `_Float16` 版を `/usr/bin/clang -O3 -ffp-contract=off`
  で。`-ffp-contract=off` で C 側が `fmul h`・`fadd h` になり `fmadd` にならないことを確認済み。`-ffloat16-excess-precision=none` は Apple clang 21 の
  driver option ではない（確認済み）ので使わない。
- 手順: `benchmarks/run-dispatch.mjs` の形を写す。native `-O3`、1 回の warm-up の後 9 回、中央値・最小・最大の ms を出し、結果のビット列
  （checksum）が 3 者で一致しなければ失敗にする。生データは `target/perf/D10/<run_id>/results.jsonl` に JSON Lines で保存し、CPU・OS・
  clang・node・commit を記録する。速度の閾値は置かない。

## ドキュメント

- `docs/language.md` の数値の節: 「f16／f128 と decimal は同梱の整数ベース演算で処理し」の文に、FEAT_FP16 を保証できる native AArch64
  （macOS arm64、または `--cpu native` で FP16 を持つ build 機）では f16 の四則演算・比較・`Math.sqrt`・f32／f64 との変換を hardware 命令にし、
  NaN を含め同梱実装とビット単位で同じ結果を返すことを足す。「i128、f16／f128、decimal を含む変換は正確なソフトウェア処理を維持します」を
  f16 と f32／f64 の変換の例外に合わせて直す。
- `_docs/language-reference/numbers.md` の `## decimal` の同じ文。
- `docs/architecture.md`: `numeric.c` の役割の表、`fpext`／`fptrunc` の変換の段落、`tests/math.mjs` の段落の近くに `tests/f16_hardware.mjs` の説明、
  `## 性能設計の原則` に「f16 の hardware 経路は静的判定、NaN はビット補正」の 1 行。
- `docs/benchmarks.md`: 新しい節 `## f16 のハードウェア経路`（計測条件、表、IR と機械語の抜粋）。
- `README.md` の検証コマンドの一覧に `node tests/f16_hardware.mjs target/release/tsuzuri`。
- `_docs/feature-status.md` の D10 行と `_features/README.md` の状態欄（Phase 1 完了として GUIDE §8 の書式で）。

## 受け入れ条件

- [ ] macOS arm64 の native で f16 の四則演算・比較・`Math.sqrt`・f32／f64 との変換が `half` の命令になり、機械語に `fadd h` などが現れる。
- [ ] `tests/f16_hardware.mjs` が native／WASM × `-O0`／`-O3` で NaN を含む全ビット一致を示し、単項は全 65,536 件を含む。
- [ ] WASM の IR・`.wasm` と、f16 を使わないプログラムの native IR が HEAD と byte 単位で同一。WASM の import は空。
- [ ] `tests/f16_hardware.rs` の 8 件と driver の単体テストが成功し、既存の Rust テスト・E2E の期待値を変えていない。
- [ ] IR に `llvm.fma.f16`・`fmuladd`・fast-math のフラグ・`to half` の `sitofp`／`uitofp` がない。
- [ ] soft・hardware・C の計測を `docs/benchmarks.md` に記録し、計測していない target（Linux AArch64、x86-64）の速度を主張していない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `math_intrinsic` は宣言に `self.ty(ty)` を使うので f16 では `i16 @llvm.sqrt.f16(i16)` になり、LLVM が IR を拒否する。f16 は専用の宣言を使う。
- `emit_typed_builtin` の builtin 用 `Globals` に `f16_hardware` を引き継がないと、演算子は hardware、`Math.sqrt` は soft と経路が分かれる
  （D05 が `wasm` で同じ問題を扱った）。Rust テスト 6 と E2E の IR 検査で検出する。
- LLVM は NaN の結果の符号と payload を非決定的とするので、補正の符号は必ず入力のビット（`%x`、`%x xor %y`、入力の上位 16 bit）から取る。
  hardware の結果のビットから取ると `-O3` の定数畳み込みで変わりうる。
- `Math.sqrt` は入力 NaN を最優先で返す。結果 NaN の補正を後に置くと signaling NaN の入力が `0x7E00` になり soft と一致しない。
- Apple の backend は `llvm.fma.f16` を `fmadd h` に下げるが、fullfp16 のない backend では f32 の `fmadd` の後に丸めて二重丸めになる（確認済み）。
  IR の正しさを backend の feature に依存させないため、f16 の FMA は出さない（D5）。
- `--cpu native` は `--emit llvm` と併用できないので、Linux AArch64 の `--cpu native` の IR は CLI で見られない。Rust テストの `f16_hardware: true` で確かめる。
- `std::arch::is_aarch64_feature_detected!` は aarch64 でしか compile できない。`native_fp16` を `#[cfg]` の 2 版にする。
- E2E で host を判定するとき、Rosetta の x64 版 Node は arm64 の Mac でも `process.arch === "x64"` を返すので hardware 側は skip になる。
  その場合は arm64 版 Node で流し直す。
- C の比較で `-ffp-contract=off` を忘れると `_Float16` の積和が融合されうる。計測の checksum 一致で検出する。

## 対象外

- f128・decimal の hardware 経路（主要 target にない）、bfloat16 型、f16 の SIMD 型（`f16x8` など）。
- f16 の `Math.fma`・`min`／`max`／`clamp`・丸め系、整数と f16 の変換、f16 と f128・decimal の変換の hardware 化（D5）。
- FP16 のない AArch64、x86-64（F16C・AVX512-FP16）、binary32 経由の経路（Phase 2。D7）。WASM（D6）。実行時 dispatch（D1）。
- f16 の全 2^32 組の二項演算の全数照合（乱数と境界で代える。D9）。

## 決定事項

### D1: 経路は静的に選び、実行時 dispatch はしない

- 決定: 判定はコンパイル時に `f16_hardware`（新規）で一度だけ行い、生成する IR を切り替える。F05 の実行時 dispatch は再利用しない。
- 理由: f16 の演算は inline の 1〜数命令で、演算ごとの分岐や間接呼び出しは演算より高くつき、最適化も妨げる。F05 の dispatch は `Array.sum` の
  ような kernel 単位の選択である。主な対象の macOS arm64 は静的に FEAT_FP16 を保証できる。
- 状態: 既定案（実装者はこの案に従う）

### D2: 選択の規則（旧未決事項「AArch64 の既定 CPU」の回答）

- 決定: native かつ host が aarch64 で、host が macOS なら `--cpu` によらず hardware。他の OS は `--cpu native` かつ
  `is_aarch64_feature_detected!("fp16")` のときだけ hardware。新しい CLI option・環境変数・IR コメントは足さない。
- 理由: Apple Silicon はすべて FEAT_FP16 を持ち、clang の既定 CPU `apple-m1` が fullfp16 を有効にする（確認済み）ので、`--cpu generic` の配布物でも
  未対応命令にならない。native target は host と同じなので host の cfg で決まる。経路は IR の `half` で見え、コメントを足すと f16 を使わない
  プログラムの IR も変わる。
- 状態: 既定案（実装者はこの案に従う）

### D3: NaN を含むビット一致

- 決定: hardware の結果が NaN のときは `select` で soft の規則の NaN に置き換え、NaN の符号と payload まで soft と一致させる。
- 理由: docs は NaN の payload を公開しないと定めるが、`Math.copysign` で符号は観測できる。完全一致なら言語の意味を変えず（承認不要）、
  E2E は全ビットを単純比較できる。費用は演算あたり `fcmp`・`and`・`or`・`select` 程度で、計測で確かめる。
- 状態: 既定案（実装者はこの案に従う）

### D4: Phase 1 の対象

- 決定: f16 の `+ - * /`、6 つの比較、`Math.sqrt`、f16 と f32／f64 の `as` 変換。
- 理由: いずれも IEEE の正しい丸めの 1 操作で、f32 へ昇格する backend でも演算ごとに丸めれば正しい（24 ≥ 2 × 11 + 2）。呼び出し元が
  `binary`・`cast`・`math_builtin` の各 1 か所なので、演算子と builtin の経路が揃う。
- 状態: 既定案（実装者はこの案に従う）

### D5: FMA・整数変換・その他の Math は soft のまま

- 決定: f16 の `Math.fma`、`min`／`max`／`clamp`、`floor`／`ceil`／`trunc`／`round`／`round_even`、整数と f16 の変換、f128・decimal との変換は
  Phase 1 で変えない。
- 理由: FMA は fullfp16 のない backend で二重丸めになり（確認済み）、正しさが backend の feature に依存する。整数 → f16 は f32 経由の下げ方が
  未検証、f16 → 整数は飽和の意味を別に検証する必要がある。min／max 以下は NaN と符号付きゼロの規則が別で、補正の設計が要る。
- 状態: 既定案（実装者はこの案に従う）

### D6: WASM は soft のまま

- 決定: `--target wasm32` はどの Phase でも soft 経路で、IR に `half` を出さない。旧仕様の「経路 2 を WASM で使う」は採らない。
- 理由: WASM MVP と SIMD128 に binary16 演算はなく、`half` は `__extendhfsf2`・`__truncsfhf2`・`__truncdfhf2` の libcall（未解決 import）になる
  （確認済み）。既定の WASM に import を足さない（D-18）。
- 状態: 既定案（実装者はこの案に従う）

### D7: x86-64 と FP16 のない AArch64 は Phase 2

- 決定: Phase 1 ではどちらも soft。Phase 2 で、x86-64 は F16C を変換だけ（f64 → f16 は soft）、算術は AVX512-FP16 のときだけ、
  FP16 のない AArch64 は Phase 1 と同じ IR（昇格による演算ごとの丸め）を使う。どちらも実機での E2E を条件にする。
- 理由: 作業機は arm64 macOS だけで、x86-64 と Linux AArch64 の結果を実行して照合できない。AGENTS.md は実際の対応と計画を分けて報告することを求める。
- 状態: 既定案（実装者はこの案に従う）

### D8: ライブラリ API は soft 固定

- 決定: `llvm::emit`・`llvm::emit_target` は `f16_hardware: false`。driver と test runner だけが `f16_hardware` の判定を使う。
- 理由: 生成 IR を引数だけの関数に保ち、host で結果が変わらないようにする。既存の Rust テストの IR 期待値を変えずに済み、新しいテストは
  `EmitOptions` で両経路を任意の host で確かめられる。
- 状態: 既定案（実装者はこの案に従う）

### D9: 検証の規模と参照

- 決定: 単項は全数、二項は境界 4,096 組と乱数（四則 262,144 組、比較 65,536 組）。参照は JS の double 経由の独立実装で、WASM の soft 経路とも
  3 者で照合する。全 2^32 組の照合はしない。
- 理由: 二項の全数は JS 参照で数時間かかる。四則演算と平方根は丸め境界・特殊値の組が誤りの大半を占め、境界集合がそれを網羅する。
  soft 経路は HEAD で既存テストを通った実装なので、第二の参照になる。
- 状態: 既定案（実装者はこの案に従う）

### D10: helper の置き場所

- 決定: `half_binary`・`half_cast`・`half_sqrt`・`half_nan`（新規）は `src/llvm_math.rs` の `impl FunctionEmitter` に置き、新しいファイルは作らない。
- 理由: soft float の分岐と `math_builtin` が既にこのファイルにあり、`src/llvm.rs` をこれ以上大きくしない。
- 状態: 既定案（実装者はこの案に従う）
