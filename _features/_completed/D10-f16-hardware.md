# D10: f16 のハードウェア演算経路

| 項目 | 内容 |
| --- | --- |
| ID | D10 |
| 優先度 | P3 |
| 規模 | M |
| 依存 | D03 |
| 後続 | F08, F09 |
| 状態 | done（Phase 1・2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。旧計画の基準は `f8dc655` |
| 実装の基準 | `053bced8`、2026-10-11 |
| 承認 | 2026-10-11 に、全フェーズと `要承認` の判断への包括承認（旧 D1〜D10 の既定案と、利用者が求めた Phase 2 の着手を含む）。コンパイラが読む新しい CLI option・環境変数、診断、std 名、予約語はない |
| 利用者向け仕様 | [`f16` の計算の経路](../../_tsuzuri/language-reference/types-and-type-inference/basic-types.md#f16-の計算の経路)、[演算子と式](../../_tsuzuri/language-reference/values-and-functions/op-and-expressions.md)、[戦略](../../_tsuzuri/language-reference/languages/strategy.md)、[オプション](../../_tsuzuri/language-reference/compiler/option.md)、[WebAssembly](../../_tsuzuri/language-reference/compiler/webassembly.md)、[ネイティブ連携](../../_tsuzuri/language-reference/compiler/native-interop.md)、[言語仕様](../../docs/language.md#f16-の計算経路)、[性能測定](../../docs/benchmarks.md#f16-のハードウェア経路d10) |

## 目的と実装範囲

f16 の `+ - * /`、6 つの比較、`Math.sqrt`、f32／f64 との `as` を LLVM の `half` 命令にした。FP16 命令（Arm の FEAT_FP16、AVX512-FP16）があればその命令に、なければ演算ごとに `f32` を 1 回挟んだ命令列に下がる。
結果は、ソフトウェア実装（`src/runtime/numeric.c`）と NaN の符号・ペイロードまでビット単位で同一にした（旧 D3）。言語の意味は変わらず、変わるのは速度と生成コードだけである。
経路はコンパイル時に 1 度だけ選び、実行時の切り替えはしない。f16 の保存、ABI、デバッグ情報は経路によらず `i16` のままで、演算の前後でだけ `half` へ `bitcast` する。WASM は常にソフトウェア実装で、`half` も import も出さない。

| フェーズ | 実装 |
| --- | --- |
| Phase 1 | `F16Path` と `EmitOptions.f16`、`FunctionEmitter::binary`・`cast` と `math_builtin` の 3 か所のフック（`half_binary`・`half_cast`・`half_sqrt`）、native AArch64 の `Hardware`、driver と test runner の選択。ticket の Rust テスト 8 件と E2E、ベンチマーク、文書 |
| Phase 2 | FEAT_FP16 のない AArch64（Linux・Windows の `--cpu generic`）への拡張、x86-64 の `--cpu native` の `Conversions`（F16C）と `Hardware`（AVX512-FP16）、clang へのフラグと `@cpu` の版の特徴、複数ターゲットの機械語の検査、決定関数と `CpuFeatures` |

## 確定した仕様

### 経路の選択

`driver::f16_path(wasm, os, arch, cpu, host: CpuFeatures)` が選ぶ純関数で、ホストの `cfg!` も環境も読まない。ホストの機能（`CpuFeatures::detect`）は `--cpu native` のときだけ見る。経路が使えないとき（機能が確認できない、ほかのアーキテクチャ）は `Soft` に戻る。経路を指定する option はなく、`--cpu native` が使えないアーキテクチャの `E2000` は従来どおり。

| ターゲット | `--cpu generic` | `--cpu native` |
| --- | --- | --- |
| WASM | `Soft` | （使えない） |
| AArch64（OS によらない） | `Hardware` | `Hardware` |
| x86-64 | `Soft` | AVX512-FP16 があれば `Hardware`、F16C があれば `Conversions`、なければ `Soft` |
| ほか | `Soft` | `Soft` |

`Hardware` は演算・比較・`Math.sqrt` と 4 つの変換（f16↔f32、f16↔f64）、`Conversions` は f16→f32、f16→f64、f32→f16 だけが `half` の `fpext`／`fptrunc`（演算と f64→f16 は `Soft` のまま）。F16C に f64→f16 の命令はなく、LLVM は `__truncdfhf2` を呼び、f32 を経由すると 2 回丸めになる。`llvm::emit`・`emit_target` とテストの既定は `Soft`。

AArch64 を OS によらず `Hardware` にする規則は定数 `AARCH64_HALF_WITHOUT_FP16` で、`false` にすると macOS と `--cpu native` で `fp16` を検出したホストだけになる（1 行で狭められる）。

### 結果のビット

- `+ - * /` と平方根は、1 回の IEEE の丸め。FEAT_FP16 のない AArch64 では LLVM が演算ごとに `f32` へ広げて 1 回の `f32` 演算をして `half` へ丸める。`f32` の仮数 24 bit が `2 × 11 + 2` 以上なので 2 回丸めにならない。融合（FMA）、fast-math のフラグ、整数との `half` 変換は使わない。
- NaN の結果は、オペランドのビットから作った値に `select` で置き換える。`+ -` は quiet NaN `0x7E00` に左辺の符号、`* /` は両辺の符号の排他的論理和。`Math.sqrt` は NaN の入力をそのまま返し（signaling NaN も）、非 NaN の入力が NaN になるときは `0x7E00`。`as` は目的の型の quiet NaN に入力の符号を付ける（f16 は `0x7E00`、f32 は `0x7FC00000`、f64 は `0x7FF8000000000000`。ペイロードは捨てる）。
- NaN の判定は `(bits & 0x7FFF) > 0x7C00`（f16 のビット）と、f32／f64 入力の `fcmp uno`。
- 次はどの経路でも `half` 命令にしない。同梱のソフトウェア実装: `Math.fma`、`Math.min`・`max`・`clamp`、`Math.floor`・`ceil`・`trunc`・`round`・`round_even`、整数・f128・decimal との `as`、表示と解析。コンパイラ内の評価: 定数評価。もとから整数のビット演算: `Math.abs`・`copysign`・`is_nan`・`is_infinite`・`is_finite`、単項 `-`、`Hash`。

### 機械語と clang

- `--cpu native` の x86-64 は、`clang -x ir` に `-march=native` と、経路の `-mf16c`（`Conversions`）・`-mavx512fp16 -mf16c`（`Hardware`）を渡し、コンパイラと clang の CPU 検出が食い違っても `half` がライブラリ関数（`__extendhfsf2`・`__truncsfhf2`・`__truncdfhf2`）にならないようにする。
- `@cpu` の版の `target-features` はビルドの特徴を置き換えるので、x86-64 の level 1〜3 の版のうち、その版の本体に `half` の命令があるものにだけ、`Conversions` は `+f16c`、`Hardware` は `+avx512fp16,+f16c` を足す。AArch64 の版は足さない（`half` は基本命令に下がる）。
- 経路を選んだ target では機械語にライブラリ関数の呼び出しがないことを、`tests/f16_hardware.rs` が、AArch64（Linux の gnu・musl、Windows の msvc、macOS）と x86-64（`Hardware` は `-mavx512fp16 -mf16c`、`Conversions` は Linux・Windows・macOS の `-mf16c`）の `-O0`・`-O3` で確かめる。実装中には、`-march=haswell`・`-march=sapphirerapids`、FEAT_FP16 を外した AArch64（`-mcpu=generic`）、Windows の gnu、clang 21・22・23 と同梱の clang でも確かめた。フラグなしの x86-64 はライブラリ関数を呼ぶ（負の対照のテスト）。

## 旧計画から外れた判断

| 旧判断 | 確定した判断と理由 |
| --- | --- |
| `EmitOptions.f16_hardware: bool` | 3 値の `F16Path`（`Soft`・`Conversions`・`Hardware`）。F16C は変換だけを `half` にする中間の水準が要り、bool では書けない。`EmitOptions` に必須の項目を足したので、既存の Rust テストの `EmitOptions` の初期化 8 ファイルに `f16: F16Path::Soft` を足した（期待値は変えていない） |
| 選択は host の `cfg!`（`native_fp16`）と `is_aarch64_feature_detected!` | 純関数 `f16_path(wasm, os, arch, cpu, host)` と、それにホストの値を渡す `host_f16_path`。任意の host で全部の組を単体テストでき、G15 のクロスコンパイルがターゲットの OS とアーキテクチャを渡せる（ホストの機能は `--cpu native` のときだけ見る） |
| native AArch64 は macOS か、`--cpu native` で `fp16` を検出したとき | Linux・Windows を含む AArch64 全体。FEAT_FP16 のない `half` は基本命令（`fcvt` と単精度の演算）に下がり、ライブラリ関数を呼ばず、演算ごとに 1 回丸めることを機械語と実行（M1 の `-mcpu=generic`、ベクトル化したループを含む）で確かめた。`AARCH64_HALF_WITHOUT_FP16` で 1 行で戻せる |
| NaN の判定は `fcmp uno half` | f16 のビットの整数の比較。`half` の比較を FEAT_FP16 のない target で変換つきにせず、浮動小数点の領域に依存しない。計測は一貫しなかったので（連鎖は速く、ベクトル化したループは遅い）、単純な形を採った（性能測定） |
| Phase 2 の x86-64 は設計方針だけ | `Conversions`（F16C の変換）と `Hardware`（AVX512-FP16）を実装。F16C の CPU でも、演算は AVX512-FP16 がなければソフトウェアのまま（F16C だけの `half` の演算は演算ごとに `f32` を挟む形で、機械語はライブラリ関数なしだが、有効にしていない。下の既知の限界） |
| clang のフラグは触らない | `--cpu native` の x86-64 は `-mf16c`・`-mavx512fp16` を明示する。ライブラリ関数の呼び出しは、未解決のシンボルか、実行時の依存になる |
| `@cpu` の版は触らない | F08 の版は `target-features` でビルドの特徴を置き換えるので、版に `half` の命令があれば特徴を足す（足さないと、その版だけライブラリ関数になる）。f16 を使わない版には足さない（レビューで修正） |
| E2E は host が hardware でなければ `hardware path skipped` | どの host も、実行した経路（`hardware`／`conversions`／`soft`）を表示する。経路は、オブジェクトの `tz_soft_` の参照を、演算・広げる・f32 から縮める・f64 から縮めるの別のプローブで調べて分類する。環境変数 `TSUZURI_F16_EXPECT`（このテストだけが読む）で期待する経路を決めると、別の経路を通ったときに失敗する |
| E2E の `half` の演算は switch のループだけ | `-O3` でベクトル化される演算ごとの一括ループも実行する。LLVM は f64→f16 を `fcvtxn`（round-to-odd）と `fcvtn` の 2 命令にし、1 回丸めと同じ結果になるはずだが、スカラーのループでは通らない形なので実行して確かめる |
| ticket の Rust テスト 8 件 | 15 件。経路ごとの IR、WASM が全経路で同一、演算子と builtin の経路が同じ、複数ターゲットの機械語、`@cpu` の特徴、`driver::build` の経路を足した |
| 計測は soft・hardware・C | M1 だけで、FEAT_FP16 のない lowering の形を `clang -mcpu=generic` の `promoted` で実行する第 4 の列を足した。ticket の `-ffloat16-excess-precision=none` は arm64 では使わない（Apple clang にない）。x86-64 では付ける |
| 文書は `_docs/` | `_tsuzuri/language-reference/`（GUIDE §8.1）。`_docs/feature-status.md` の D10 行は `languages/strategy.md` の表 |
| 旧 D8: ライブラリの入口 `emit`・`emit_target` は soft 固定で、driver と test runner だけが判定を使う | 方針は同じ。ticket の後に増えた入口（`emit_wasm_build`・`emit_trap_return`・`emit_native_build_for`・`TestRunnerOptions`・`emit_bench_runner`）も `EmitOptions.f16`・`TestRunnerOptions.f16` から経路を受け取り、WASM は `emit_program` が `Soft` にする。`emit_bench_runner` は `Soft` のままで、経路を渡すのは `emit_bench_runner_with` |

### 停止条件の確認

ticket の停止条件はどれも起きなかった。hardware の結果は soft・JS の参照と NaN を含めて一致し、`numeric.c`・`numeric.ll`・`math.ll` は変えず、f16 を使わないプログラムの IR・WASM の IR と `.wasm` は変わらず、WASM に `half`・ライブラリ関数・import はなく、`unsafe`・新しい crate・コンパイラが読む環境変数・IR コメントはなく、既存テストの期待値も変えず、hardware は soft より速かった。

## 受け入れ条件

- Phase 1: macOS arm64 の native で f16 の四則演算・比較・`Math.sqrt`・f32／f64 との変換が `half` の命令になり、機械語に `fmul h`・`fadd h`・`fcvt h, d` が現れる。`tests/f16_hardware.mjs` が native と WASM の `-O0`・`-O3` で NaN を含む全ビット一致（単項は全 65,536 件）を示す。WASM の IR・`.wasm` と、f16 を使わないプログラムの native の IR と object が、基準のコンパイラとバイト単位で同一。WASM の import は空。IR に `llvm.fma.f16`・`fmuladd`・fast-math のフラグ・`to half` の `sitofp`／`uitofp` がない。soft・hardware・C の計測を記録し、計測していない target の速度を主張しない。
- Phase 2: 機械語に `__extendhfsf2`・`__truncsfhf2`・`__truncdfhf2` などのライブラリ関数がない（経路を選んだ target すべて）。経路の決定が純関数で、単体テストが全ての組を覆う。`--cpu native` の x86-64 に必要なフラグが clang に渡る。`@cpu` の版が特徴を持つ。どの host も実行した経路を表示し、期待する経路を環境変数で強制できる。AVX512-FP16 の演算と x86-64・Linux・Windows の実行は、実行した範囲と未実行の範囲を分けて報告する。

## 実装と検証（2026-10-11）

### 実装したファイル

- `src/llvm.rs`（`F16Path`、`EmitOptions.f16`、`Instrumentation`・`Globals`・`TestRunnerOptions` への受け渡し、`binary` と `cast` のフック、`emit_bench_runner_with`）、`src/llvm_math.rs`（`half_binary`・`half_sqrt`・`half_cast`・`half_widen`・`half_narrow`、`math_builtin` のフック）、`src/llvm_cpu.rs`（`f16_features`・`uses_half`、版の `target-features`）。
- `src/driver.rs`（`CpuFeatures`、`AARCH64_HALF_WITHOUT_FP16`、`f16_path`・`f16_path_with`・`host_f16_path`・`f16_cpu_flags`・`native_ir_cpu_args`、`EmitOptions` の 2 か所、単体テスト 3 件）、`src/test_runner.rs`（`tsuzuri test` と `tsuzuri bench` は `--cpu generic` の経路）。
- `tests/f16_hardware.rs`（15 件）、`tests/f16_hardware.mjs`（E2E）、`tests/{allocator,bounds_checks,cpu_dispatch,debug_info,debug_output,multiversion,trap_locations,windows}.rs`（`f16: F16Path::Soft` の追加だけ）。
- `benchmarks/f16/Main.tz`・`f16.c`、`benchmarks/run-f16.mjs`。
- `_tsuzuri/language-reference/` の `types-and-type-inference/basic-types.md`（「`f16` の計算の経路」）、`values-and-functions/op-and-expressions.md`、`languages/strategy.md`、`compiler/option.md`・`webassembly.md`・`native-interop.md`、`built-in-types-and-modules/gpu.md`。`docs/language.md`・`architecture.md`・`benchmarks.md`、`README.md`。

### 検証結果

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`、`sh scripts/check-runtime-includes.sh`、`git diff --check` が成功。`cargo check --all-targets --target x86_64-pc-windows-msvc` と `--target aarch64-pc-windows-msvc`（rustup の stable 1.96.1）が成功し、x86-64 の `CpuFeatures::detect` もここで型検査された。
- 最終の `cargo test --locked --no-fail-fast -- --skip bounds_computation_syntax_and_expansion` は 94 バイナリで **1096 件成功**、失敗 0。`RUST_MIN_STACK=2200000` の `bounds_computation_syntax_and_expansion` と、GUIDE §3.1 の 4 つの回帰テスト（各 1 件）が成功した（パーサとチェッカーは変えていない）。`tests/f16_hardware.rs` は 15 件、driver の単体テストは 3 件（`f16_path_follows_target_cpu_and_host_features`・`detected_cpu_features_are_consistent`・`f16_cpu_flags_name_the_instructions_of_the_path`）。決定関数のテストは、`AARCH64_HALF_WITHOUT_FP16` の両方の値と、WASM・x86-64・AArch64・ほかのアーキテクチャ、`--cpu` の両方、ホストの機能の組を数え上げる。
- 変更後の release ビルドで、`tests/f16_hardware.mjs` が成功。既定の実行で **2,707,485 件**（全 65,536 ビット列の単項、境界値、中点、二重丸めの罠の値、乱数の二項演算と比較と変換）を native（この host は `hardware`）と WASM（`soft`）の `-O0`・`-O3` で、参照と NaN のビットまで照合する。native は、既定と FEAT_FP16 なしの `-mcpu=generic` の両方、switch のループと `-O3` でベクトル化される一括ループの両方で実行した。`--quick` は 21,620 件。`--cpu generic` と `--cpu native` のオブジェクトは、どちらも `hardware` と分類され、エクスポートした f32／f64 の関数 1,799,651 件（`--quick` は 24,315 件）を各 build の `-O0`・`-O3` で照合した。`tsuzuri test` も `-O0`・`-O3`・wasm32 で成功する。
- 同じ E2E を D10 より前のコンパイラ（ソフトウェア経路）に `TSUZURI_F16_EXPECT=soft` を付けて実行しても、2,707,485 件が成功した（新しいコンパイラは、この期待で「`--cpu generic` は hardware の経路」と失敗する）。JS の参照、ソフトウェア経路、`half` 命令、`-mcpu=generic` の昇格経路が、同じビットを返す。丸めを最近接偶数から四捨五入へ変えた変異と、NaN の符号の規則を変えた変異を入れて、この E2E が失敗することを確かめた。
- `TSUZURI_F16_EXPECT`: この host（macOS arm64）で `hardware` と `generic=hardware,native=hardware` は成功、`soft` と `generic=soft,native=conversions` は「期待した経路ではない」で失敗、不正な値は拒否された。
- 既存の E2E が成功: `math`（786,051 件、うち初等関数 26,376 件。`TSUZURI_MATH_PYTHON` で mpmath の環境を指定した）、`primitives`、`display_parse`（正確な表示 88,063 件、解析と往復 111,282 件）、`numeric_casts`（1,585 件）、`e2e`、`features`（13,647 件）、`cpu_dispatch`、`cpu_kernels`、`trap_return`、`trap_return_sync`、`trap_boundary`、`json`（297 件の入力）、`gpu`、`control`、`computations`、`simd`（467 件。`llvm-objdump` が要る）、`integer_intrinsics`、`allocator`、`strings`、`tasks`。native と WASM の `-O0`・`-O3` を、各 suite が実行する範囲で。
- 基準のコンパイラ（`053bced8` の release ビルド）との比較（`--emit llvm`・`--emit object`・`--emit wasm`、native と wasm32、`-O0`・`-O3`）。f16 を含まない 103 プロジェクト（tests/fixtures、examples、benchmarks）の 824 件は、714 件が同一、106 件は両方のコンパイラが同じ理由で失敗、4 件が違い、その 4 件はすべて `tests/fixtures/exceptions` の native で、この fixture は `1.5hf` と `half as f64` で f16 の演算を使う。f16 を使う 8 プロジェクトの 64 件は、36 件が同一、12 件が同じ失敗、16 件が違い、その 16 件は f16 の演算を出す 4 プロジェクト（constants・deriving・fma_reductions・primitives）の native の IR と object だけで、wasm32 の IR と `.wasm` はすべて同一（import は空）。
- 機械語: `tests/f16_hardware.rs` が、経路を選んだ target の `-O0`・`-O3` の機械語にライブラリ関数の呼び出しがないことを確かめる（上の「機械語と clang」）。macOS arm64 の `-O3` の機械語で、`half_chain` が `fmul h`・`fadd h`・`fcvt h, d` に、`half_scale` と `half_roundtrip` がベクトル命令（`fcvtxn`・`fcvtn`・`fmul.4h`）になることを `otool -tV` で確かめた。FEAT_FP16 なし（`-mcpu=generic`）は `fcvt s, h`・`fmul s`・`fcvt h, s` で、`fmul.4s` のベクトル化もされる。
- `--trap-info`・`-g`・`--trap-mode return` の build、`tsuzuri test`（`-O0`・`-O3`・wasm32）が、`half` の IR でも動く。
- `node scripts/check-docs.mjs` が変更した 7 ページで成功（148 リンク、31 例、native の実行 42 回）。

### レビュー後の修正

独立したコードレビューの指摘のうち、Medium 2 件と Low 3 件を、3 つのコミットで直した。コードの修正は、先にテストが失敗することを確かめた。

- `--cpu native` で F16C の host の x86-64 は、f16 を使わないプログラムの `@cpu` の版にも `+f16c`（`Hardware` では `+avx512fp16`）を足していた。f16 を使わないプログラムが旧コンパイラと同じでなくなり、SSE4.2 の版が AVX の命令を持ちうる。版の本体に `half` の命令があるときだけ足すようにし（`uses_half`）、`half_add` という名前だけの関数と、f16 を使わない版が「その水準の特徴だけ」であることを `tests/f16_hardware.rs` で検査した。`uses_half` を外すと、このテストは `+avx2,+f16c` で失敗する。
- 文書が、実在しない CI がこの検査を実行するかのように書いていた。CI の workflow は `tests/f16_hardware.mjs` を実行しない。README・`docs/architecture.md`・`languages/strategy.md` を、その環境で実行して確かめる、に直した。
- `basic-types.md` に「`f16` のハードウェア経路は計画中です」が残っていた。`f16` の計算の経路への参照に直した。
- ベンチマークの C の host が、`assert` の中で `clock_gettime` と C の kernel を呼んでいた（`NDEBUG` で消える）。C の行に Tsuzuri の checksum を出してもいた。検査を `assert` でないものにし、各実装が自分の checksum を出す。
- 速度の比（C に対して 1.4〜4.4 倍）を、生データに合わせて 1.4〜4.5 倍に直した。NaN の置き換えを外した試作の測定は、コミットも生データの保存もしていないので、1 回ずつの測定と書いた。

### 文書と計測

M1 Max（macOS arm64、Apple clang 21、load average 8〜15）の 100 万要素の `f16` のカーネルを 3 回測った（`docs/benchmarks.md`）。1 回の通過の中央値（ms）の 3 回の中央値:

| カーネル | soft | hardware | promoted | C `_Float16` |
| --- | --- | --- | --- | --- |
| `half_chain`（積と和の依存の連鎖） | 3336 | 10.2 | 13.9 | 2.26 |
| `half_roundtrip`（変換だけ） | 1922 | 1.39 | 1.80 | 0.97 |
| `half_scale`（独立な要素） | 3232 | 2.19 | 3.44 | 0.97 |

hardware は soft の約 330〜1,480 倍速く、C の `_Float16` の 1.4〜4.5 倍の時間である。チェックサムは 4 つの実装で 1 ビットも違わない。差の主な原因は、NaN のビットをそろえる置き換えで、取り除いた使い捨ての試作（1 回ずつの測定）は C と同じ時間だった。置き換えの形を変えた試作（FP 領域の `select`、`!prof` つきの分岐、`copysign`）は、連鎖で最大 2 倍速くなる代わりにベクトル化したループが 13〜29% 遅くなり、一貫しなかったので採らなかった。M1 以外、x86-64、Linux・Windows は測っていない。

## 既知の限界

- 実行して確かめたのは macOS arm64 だけ。x86-64 の F16C と AVX512-FP16、Linux と Windows の AArch64 は、機械語の確認までで、その CPU での実行はしていない。FEAT_FP16 のない AArch64 の lowering は、同じ命令列を M1 の `-mcpu=generic` で実行して照合した。AVX512-FP16 の演算はどの host でも実行していない。
- F16C だけの x86-64 の演算は `Soft`。`half` の演算を F16C だけで下げると、演算ごとに `f32` を挟んだライブラリ関数なしの機械語になり、理論上は正確だが、x86-64 で実行して確かめていないので有効にしていない。`f16_path_with` の x86-64 の規則の 1 行で有効にできる。
- NaN のビットをソフトウェア実装とそろえる費用（C の `_Float16` の 1.4〜4.5 倍）は、NaN の符号とペイロードを仕様から外せば消える見込みだが、経路ごとに NaN のビットが変わる。この判断は仕様の側に残してある（旧 D3 のまま）。
- `Math.fma`、`min`・`max`・`clamp`、丸め系、整数・f128・decimal との変換、定数評価は、どの経路でもソフトウェア実装。f16 の SIMD 型はない。
- `Cargo.toml` の `rust-version = "1.85"` は、let 連鎖（1.88）を使う既存のコードですでに実態とずれている。`is_x86_feature_detected!("avx512fp16")` は rustup の stable 1.96.1 で x86-64 向けに型検査できることを確かめたが、それより古い Rust では確かめていない。
