# F09: GPU の浮動小数点・64-bit カーネルと実行時接続

| 項目 | 内容 |
| --- | --- |
| ID | F09 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | F07, (C11) |
| 後続 | – |
| 状態 | done（Phase 1・2・3） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。同日、実装者向けに詳細化（HEAD `f8dc655`） |
| 実装の基準 | `96d7cbf`、2026-10-10。Phase 1・2 は `impl/f09-gpu`、Phase 3 は Phase 1・2 を取り込んだ `impl/f09-vulkan` |
| 承認 | 全フェーズと必要な判断への包括承認（`D1`・`D9`・`D10` を含む `要承認` のすべて） |
| 改善する劣位 | C/C++ 比: GPU は実験段階で、成熟した GPU 開発基盤の代替にならない（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 利用者向け仕様 | [Gpu](../../_tsuzuri/language-reference/built-in-types-and-modules/gpu.md)、[言語仕様の GPU Kernel](../../docs/language.md#gpu-kernel実験的)、[アーキテクチャ](../../docs/architecture.md)、[性能測定](../../docs/benchmarks.md#vulkan-と-gpuautof09-phase-3) |

## 目的と実装範囲

CUDA／Metal／Vulkan compute を C/C++ から使う場合に近い GPU 活用を、数値の意味を明示したうえで可能にする。
GPU の浮動小数点は、言語の strict な float 契約（再結合・暗黙 FMA なし、subnormal・NaN・符号付きゼロの保持）を、WGSL では満たせない。
そこで緩い意味を持つ別名の API と出力種別を用意し、利用者が名前で選んだときだけ使う。黙って strict から切り替える経路は作らない。
Vulkan と SPIR-V は、float controls を報告して適合プローブを通るデバイスにだけ、strict な `f32` を許す。

| Phase | 実装 |
| --- | --- |
| 1 | relaxed f32 カーネル: `Gpu.map_relaxed`・`Gpu.init_relaxed`（CPU 参照）、`--emit wgsl-relaxed`、`webgpu.mjs` の f32 buffer、GPU なしの検証と実 adapter の検証、転送を含む計測 |
| 2 | 言語ランタイムからの WebGPU 接続（`Gpu.request Gpu.WebGpu`）、native の動的読み込み、WASM の opt-in の import、`shader-f16` による f16 |
| 3 | SPIR-V の生成（`--emit spirv`・`spirv-relaxed`）、Vulkan 上の実行（`Gpu.request Gpu.Vulkan`。strict の `i32`・`i32u`・`i64`・`i64u`、条件つきの strict `f32`）、測ったコストで選ぶ `Gpu.Auto`、`Gpu.last_backend` |

旧計画は、Phase 1 だけを実装者に任せ、Phase 2・3 を人間の承認後とし、LLVM の SPIR-V ターゲットを先に評価するよう求めていた。
全フェーズの依頼に合わせて、Phase 3 まで実装した。現行の契約はこの記録、[Gpu](../../_tsuzuri/language-reference/built-in-types-and-modules/gpu.md)、[docs/language.md](../../docs/language.md#gpu-kernel実験的) を優先する。

## 確定した API と意味

```text
Gpu.request      :: Backend -> Result<Device, Error>
Gpu.backend      :: ref Device -> Backend
Gpu.last_backend :: unit -> Backend                                        // Phase 3
Gpu.init         :: ref Device -> i64 -> (i32 -> 'a) -> Buffer<'a>          // strict
Gpu.map          :: Copy<'a> => ref Device -> ('a -> 'b) -> Buffer<'a> -> Buffer<'b>
Gpu.init_relaxed / Gpu.map_relaxed                                          // 緩い f32・f16（Phase 1）
tsuzuri build Kernel.tz --emit wgsl | wgsl-relaxed | spirv | spirv-relaxed
```

環境変数は `TSUZURI_WEBGPU_LIBRARY`、`TSUZURI_VULKAN_LIBRARY`（どちらも、設定すればその 1 つだけを試し、空なら無効）、`TSUZURI_GPU_DEBUG`、`TSUZURI_GPU_AUTO_MIN_WORK` で、コンパイル時のオプションでもキャッシュのキーでもない。

- `Gpu.request`: `CpuReference` と `Auto` は常に `Ok`。`WebGpu` と `Vulkan` は、ランタイムがデバイスを開けて、プログラムのカーネルが要る機能がそろうときだけ `Ok`、そうでなければ `Result.Error Gpu.Unavailable`。
  `Cuda` と `Metal` は常に `Unavailable`。明示的に要求したバックエンドを、CPU や緩い意味へ黙って替えることはない。
- strict な呼び出し: WebGPU は `i32`・`i32u` だけを GPU で動かし、Vulkan は `i32`・`i32u`・`i64`・`i64u` と、条件つきの `f32` を動かす。どれも CPU 参照とビット単位で一致する。
  GPU のカーネルがない strict な呼び出しは、デバイスで動かすと理由を標準エラーに出してトラップする（コンパイル時には拒否しない。デバイスが実行時の値で、同じ呼び出しが CPU 参照では動くため）。
- 緩い呼び出し（`_relaxed`）は、WGSL と SPIR-V の浮動小数点規則に従い、結果は厳密ではない。CPU 参照は厳密に評価する。
- `Gpu.Auto`: 呼び出しごとに、CPU 参照か Vulkan を、転送・同期・初回の費用を含めて測ったコストの規則で選ぶ。WebGPU は候補にしない。定数は 1 台のマシンの経験則。
- WebAssembly: WebGPU は `--wasm-feature webgpu` のときだけ 2 つの import を足し、Vulkan は常に `Unavailable`、`Auto` は常に CPU 参照。既定の出力は import を持たない。
- バッファは、どのバックエンドでもホストの配列で、呼び出しごとに複製・アップロード・実行・完了待ち・読み戻しをする。GPU 常駐のバッファはない。

## 決定事項

- D1（Phase 1）: 緩い float は `Gpu.init_relaxed`・`Gpu.map_relaxed` と `--emit wgsl-relaxed`（Phase 3 で `spirv-relaxed`）の中だけで許す。契約は、縮約・再結合・subnormal の flush・WGSL の除算精度・非有限時の未規定値を許し、trap しない。
  理由: WGSL は fusion・再結合・subnormal の差を許し、NaN・無限大を未規定にするので、strict な契約を満たせない。名前で区別すれば strict の意味を変えずに GPU の float を使える。
- D2: CPU 参照は strict で評価する。理由: strict な結果は relaxed で許される結果の一つで、決定性の既存の検証をそのまま使える。CPU で FMA などを使うと、黙って fast-math を有効にしないという方針に反する。
- D3: relaxed の選び方は出力種別で表す（`Emit::WgslRelaxed`・`Emit::SpirvRelaxed`）。`BuildOptions` に field を足さない。理由: 複数のテストが struct literal で作るため。
- D4: relaxed kernel の制約は、lane が `f32`・`i32`・`i32u`、`f64`・`i64` と `f32` から整数へのキャスト・`bool` の lane は拒否（E1018）。理由: WGSL に 64 bit がなく、飽和・NaN の規則を保証できない。
- D5: relaxed の WGSL は 1 行目に `// tsuzuri-gpu float=relaxed input=<t> output=<t>` を出し、ホストの `prepare` は `{ float: "relaxed" }` がなければ拒否する。ホストの境界でも暗黙の選択をなくす。
- D6: 検証の許容誤差は、relaxed の GPU 結果だけに、float 演算ごとの重みの和（`+`・`-`・`*` は 2 ulp、`/` は 3 ulp、整数から `f32` は 1 ulp）で適用する。入力は相殺のない `[2^-8, 2^8]` の正規数に限る。strict は完全一致。
- D7: GPU のない環境では、WGSL の完全一致、CLI の決定性と診断、CPU 参照のビット一致を常に実行し、実デバイスの検証は明示したときだけ。実行しなかった部分は、skip と混同せず、要約行に出す。
- D8: 計測は、転送・dispatch・完了待ち・読み戻しを含む時間を主要な値とし、デバイスの起動、パイプライン作成、常駐を別の列にする。9 回の中央値と最小・最大を記録し、速度の主張と合否の閾値は作らない。
- D9（Phase 2）: `Gpu.request Gpu.WebGpu` を実装する。WASM は `--wasm-feature webgpu` を指定したときだけ import を足し、native は WebGPU 実装を実行時に動的に読み込む（リンク時の依存なし）。
  読み込めない、必要な feature（`shader-f16` など）がないときの明示要求は `Unavailable` で、CPU へ置き換えない。
- D10（Phase 3）: strict な `f32` は、Vulkan の float controls（`SignedZeroInfNanPreserve`・`DenormPreserve`・`RoundingModeRTE`）と SPIR-V の `NoContraction` で作り、デバイスが能力を報告し、適合プローブを通るときだけ許す。
  `i64` は `shaderInt64` のデバイスだけ。SPIR-V は LLVM のターゲットを先に評価し、`NoContraction` を出せないため、独自の emitter にした。`Gpu.Auto` は転送を含む測定の規則でだけ GPU を選び、選択を観測可能にした。
  能力の確認なしに strict と称さない。

## 実装状況（2026-10-10、base `96d7cbf`）

利用者の包括承認（`D1`・`D9`・`D10` を含む `要承認` のすべて）に基づく。実装は worktree `impl/f09-gpu` の Phase 1 と Phase 2、worktree `impl/f09-vulkan` の Phase 3。

### Phase 1（done）

- 実装: `Gpu.init_relaxed`・`Gpu.map_relaxed`（`std/Gpu.tz`）、`Emit::WgslRelaxed`（`--emit wgsl-relaxed`。`BuildOptions` に field なし。D3）、`GpuKernel::wgsl_relaxed`、
  1 行目の宣言 `// tsuzuri-gpu float=relaxed input=<t> output=<t>`、`bitcast<f32>(<bits>u)` のリテラル、`(-x)`・`/`・`f32(x)`、
  `webgpu.mjs` の f32 buffer（`Float32Array`、要素種別 `kind`、`prepare(source, { float: "relaxed" })`）。
- ticket から外れた点: ① `src/cache.rs` の `Emit::Llvm | Header | Wgsl` も直した（ticket の表にない）。② 現行の `--emit` の一覧（`shared`・`bindings-*`）に合わせてメッセージを
  `… wgsl, wgsl-relaxed, shared, …` にした（`src/main.rs` の既存テストの文面。ticket が許す変更）。③ f32 の `%` は言語にない（`Rem` は整数と `BigInt` だけ）ので、ticket の
  「float の剰余」の拒否は到達しない。専用の arm も診断も作っていない。④ bool lane は専用の文面（`relaxed WebGPU buffer lanes must be f32, i32, or i32u; bool lanes are unavailable`）。
  ⑤ ticket の「例」の WGSL は簡略化されていて、実際の出力は全式を `let value_N` に束縛する（strict と同じ生成規則）。テストは実際の出力を完全一致で比べる。
  ⑥ `fromArray` の buffer に `COPY_SRC` を足した（`toArray` が upload 直後の buffer でも検証を通る。以前は未使用の経路）。
- 検証: `cargo test --locked --test gpu` 7 passed、`--emit wgsl` の strict 出力は base と byte 一致（`cmp`）、`node tests/gpu.mjs`（CPU のみ）は 783 の整数参照と 1,315 の緩い f32 参照が成功、
  `TSUZURI_WEBGPU=1 node tests/gpu.mjs`（Apple M1 Max、Dawn 0.6.1 の Metal）は成功。最大誤差は poly 1・horner 1・ratio 2・index 0・threshold 0 ulp（許容 4・12・9・5・0）。
  性能は `docs/benchmarks.md`（9 回の中央値。転送を含む値と常駐の値を分け、速度の優位は主張しない）。

### Phase 2（done）

- 実装: `Gpu.request Gpu.WebGpu` は、言語ランタイムがデバイスを開けたときだけ `Result.Ok`、ほかは `Result.Error Gpu.Unavailable`（CPU への置き換えなし）。
  `Gpu.WebGpu` を構築するユーザー関数があるプログラムだけが「デバイス対応」で、そのときだけ `request`・`init`・`map`・`init_relaxed`・`map_relaxed` を `std/Gpu.tz` の
  `request_on`・`init_on`・`map_on`（内部）へ付け替え、呼び出し箇所ごとの WGSL を `CheckedModule.gpu` の表（`src/gpu_devices.rs`）と LLVM の定数（`src/llvm_gpu.rs`）に埋め込む。
  そうでないプログラムの IR・import・ABI は変わらない（base との IR 比較は「検証」）。バッファは呼び出しごとに転送し、ホスト配列のまま（GPU 常駐は作っていない）。
  ランタイムの境界は `tsuzuri_gpu_open(backend, features)` と `tsuzuri_gpu_run(backend, mode, flags, lanes, wgsl, wgsl_len, spirv, spirv_len, input, count, output)`。
  native は `src/runtime/gpu.c`（wgpu-native 29 を `dlopen`／`LoadLibrary`、`webgpu.h` の必要な部分を自前で宣言、`wgpuGetVersion` の主バージョン 29 を確認、リンク時の依存なし）。
  WebAssembly は `--wasm-feature webgpu`（`BuildOptions.wasm_webgpu`）のときだけ 2 つの import（`tsuzuri_gpu.open`・`tsuzuri_gpu.run`）を足し、ホストは `src/runtime/webgpu.mjs` の
  `createGpuImports`（JSPI）。既定の WASM は import なしのまま。`f16` は緩い API の lane・局所値・引数・結果で、`enable f16;` と `shader-f16`（プログラム全体の要求。アダプタが持たなければ `Unavailable`）。
- 決めた点（ticket が「Phase 2 の着手前レビューで決める」とした点を含む）:
  ① 厳密な `Gpu.map`・`Gpu.init` を WebGpu device で動かすとき、GPU のカーネルがあるのは `i32`・`i32u` の lane だけ（CPU 参照とビット単位で一致）。ほかの厳密な呼び出し（`f32`・`f16`・64 bit・`bool`、
  `f32` などの局所値を持つコールバック）はカーネルがなく、理由を標準エラーに出してトラップする。コンパイル時に拒否しないのは、device が実行時の値で、同じプログラムが CPU 参照の device で同じ呼び出しを使えるため。
  CPU や緩い意味に黙って替えることはない。浮動小数点の GPU 実行は緩い名前だけ。② `request` が成功したあとの失敗は、理由を出してトラップする。③ f16 は緩い API だけで入れた
  （`D10-f16-hardware` は CPU 側の経路の話で、`f16` 型は既に soft で言語にある）。`f16` は `export` できない（E1008）ので `--emit wgsl-relaxed` の根にはならず、`f32` の根の内部では使える。
  ④ `--emit bindings-js` と `webgpu` の組み合わせは E2000（E13 のグルーに `tsuzuri_gpu` は実装していない）。⑤ ソースビルドの wgpu-native（Homebrew の 29.0.1.1 は `wgpuGetVersion` が 0）は、
  `TSUZURI_WEBGPU_LIBRARY` で名指ししたときだけ受け入れる。
- ticket から外れた点: ① `BuildOptions` に field を 1 つ足した（`wasm_webgpu`。struct リテラルのテストは `..Default::default()` を使うので影響なし。`EmitOptions` は変えていない）。
  ② `src/main.rs` の既存テストの文面（`supported WASM features are …`）に `'webgpu'` を足した。③ `tests/gpu.mjs` の `unavailable` は、ホストの実 GPU 環境で結果が変わらないよう、`TSUZURI_WEBGPU_LIBRARY=""` で動かし、リンクに `src/runtime/gpu.c` を足した。
  ④ 完了待ちは、最初の 5 ms をブロックしない poll で回す（wgpu-native 29 の Metal で、ブロックする poll が 1 呼び出し 1.6 ms 前後かかったため。0.46 ms になった）。
  ⑤ `webgpu.mjs` の `fromArray`・`toArray` に `Uint16Array`（f16 の bit 列）を足した。⑥ IR の連番（`$intrinsic.<c>.<m>.<id>`、`$instance.<id>.`）は std のテンプレート数を数えるので、std に関数を足すと、
  使わないプログラムでも 144 ファイル中 24 でこの連番だけが変わる（正規化した比較は差 0）。ticket の「byte 一致」は、この連番を除いて成り立つ。
- 検証（Apple M1 Max、macOS 27.0.1、wgpu-native 29.0.1.1（Homebrew）、Dawn の `webgpu` 0.6.1、Node.js v20.19.6 と v24.21.0）: `cargo test --locked --test gpu` 11 passed、
  `node tests/gpu_runtime.mjs`（GPU なし）、`TSUZURI_WEBGPU=1 TSUZURI_WEBGPU_LIBRARY=… TSUZURI_WEBGPU_MODULE=… node tests/gpu_runtime.mjs`（Node 24）で、native の wgpu-native と WebAssembly の Dawn（JSPI）が
  strict の `i32`・`i32u` を CPU 参照とビット単位で、緩い `f32`・`f16` を許容誤差（`f32` は 1e-5、`f16` は 4 ulp）で一致し、デバイスでの実行回数（`TSUZURI_GPU_DEBUG` の出力）が期待どおり
  （`i32` の掃引は 64 回、`f16` は 48 回）、native と WASM の `-O0`・`-O3`。`src/runtime/gpu.c` を ASan と UBSan で実 wgpu-native に対して実行（lane 数 1〜70,000、`f16` を含む）して指摘 0。
  性能は `docs/benchmarks.md`（9 回の中央値、転送を含む）。
- 確認していないこと: Windows と Linux の実行（`cargo check --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc` の型検査と、`gpu.c` の両分岐の構文検査だけ）、x86-64、Apple 以外の GPU、
  Dawn の native ライブラリ（`libwebgpu_dawn`。`webgpu.h` の版が違うので `Unavailable` になる設計）、ブラウザの `navigator.gpu`、wgpu-native 29 以外。
- Phase 3 への接続点: `std/Gpu.tz` の `request_on`・`init_on`・`map_on` の `Vulkan` と `Auto` の arm（今は `Unavailable` と `unreachable ()`）、`src/gpu_devices.rs` の `blob`（`spirv` と、`FEATURE_*` の bit、64 bit の lane 種別）、
  `src/runtime/gpu.c` の `tsuzuri_gpu_open`・`tsuzuri_gpu_run` の `backend == 2` の枝（Vulkan のコードを別のファイルにするなら、`src/driver.rs`・`src/test_runner.rs`・`tests/gpu.mjs` の `gpu.c` を連結・コンパイルする箇所）。

### Phase 1・2 のレビュー後の修正（WGSL 生成の 2 件）

独立したコードレビューが、F07 から受け継いだ WGSL 生成器の欠陥を 2 件見つけた。どちらも `--emit wgsl-relaxed` と `Gpu.map_relaxed`（と、Phase 2 の埋め込みカーネル）から届き、
コンパイルは通って、Tint が shader module の作成（`prepare()` や WebGpu デバイスでの実行時）で初めて拒否していた。

- `mut` の引数: WGSL の関数の引数は代入できず、`local_0 = …;` を出す関数を Tint が `cannot assign to parameter` で拒否した。`mut` の引数だけを `param_N` と改名し、本体の先頭で
  `var local_N: T = param_N;` と複製する。`mut` の引数がない関数の出力は、修正前と byte 一致（検証用の 24 本で `cmp`）。
- `else if` の連鎖: Tint の文の入れ子の上限は 127 で、`if` の文とその各ブロックが 1 段ずつなので、入れ子の `if`・`else if` の各枝・`&&`／`||` の右辺が 2 段を使い、`else if` は 62 本まで通って
  63 本目で拒否された（Tint で実測。WGSL の `else if` を平らに出しても 124 本が上限なので、平坦化はしない）。生成器が段数を構造から数え（テキストの走査ではない）、超える式で `E1017`
  （`GPU kernel exceeds 127 levels of WGSL statement nesting; …`、上限の数を含む）を出す。関数ごとに数え直す。厳密な呼び出しは、CPU 参照では制限を受けず、WebGpu デバイスでは GPU のカーネルがない（実行時の文面は
  lane の型だけを挙げる）。緩い呼び出しは `validate_calls` でコンパイル時に `E1017` になる。
- テスト: `tests/gpu.rs` の 3 本（修正前は失敗）、`tests/gpu.mjs` の 9 形状（CPU と実 Dawn でビット一致）と拒否 5 件、`tests/gpu_runtime.mjs` の native（wgpu-native）と WASM（Dawn、JSPI）の `shapes`・`deep63`。

### Phase 2 のレビュー後の修正（7 件）

独立したコードレビューが、Phase 2 の実装（`3d60e5c`）の欠陥を 7 件見つけた（wgpu-native の ABI の宣言そのものは、実際のヘッダーと 23 の構造体・enum・40 の関数で一致を確認済み）。

1. **利用者が `Gpu.map_on` を呼べた（高）**: `request_on`・`init_on`・`map_on` は公開の std 関数で、利用者が書いたカーネル番号が `Gpu.__run` に届き、別のカーネルの lane 幅でホストが配列の外を読み書きした。
   3 つを `private` にした（利用者の呼び出しは `E1022`。コンパイラの付け替えは影響を受けない）。さらに `Gpu.__run` の lowering が、番号のカーネルの lane の種類（入力・出力）を呼び出しの要素型の種類と比べ、合わなければソースを渡さない
   （「カーネルなし」。ランタイムが理由を出してトラップする）。
2. **上限がアダプタのもので、失敗したオブジェクトを送信していた（中）**: デバイスは上限を求めずに作るのに、`max_buffer`・`max_groups` はアダプタから読んでいた。作成の失敗（uncaptured-error コールバック）は、map の待ちのあと（送信のあと）に初めて見ていたので、
   wgpu-native が送信で panic し、プロセスが SIGABRT で落ちた。上限を `wgpuDeviceGetLimits` で読み（超える呼び出しは何も作らずに状態 3）、バッファ・レイアウト・バインドグループ・エンコーダー・パス・コマンドバッファを作るたびに、
   エラーの有無を確かめ、失敗は送信せずに状態 4 にする。
3. **失敗したシェーダー・パイプラインがキャッシュに残った（中）**: 代入のあとにエラーを見ていたので、次の呼び出しが無効なハンドルを見つけて送信し、abort した。失敗したオブジェクトは解放し、キャッシュに入れない。
4. **WASM のホストが `i32` のアドレスを符号付きで読んだ（中）**: 2 GiB 以上のアドレスは負の数で届き、`Uint8Array` が RangeError を出し（状態 3 と報告）、`slice` は末尾から読んだ。`>>> 0` で符号なしにし、メモリの外のバッファは状態 4、状態 3 は本物の上限超過
   （`cause: "limit"` の RangeError）だけにした。
5. **既定のライブラリ探索が作業ディレクトリを探した（中、セキュリティ）**: macOS の `dlopen("libwgpu_native.dylib")` は作業ディレクトリを探し、置かれたライブラリの初期化処理が `wgpuGetVersion` の前に動いた。既定の候補を、システムの場所の絶対パスだけにした
   （macOS は `/opt/homebrew/lib`・`/usr/local/lib`、Linux は `/usr/local/lib`・`/usr/lib`・`/usr/lib64`・multiarch、Windows は `LoadLibraryExA` の `LOAD_LIBRARY_SEARCH_SYSTEM32`）。`TSUZURI_WEBGPU_LIBRARY` は書いたとおり。
6. **打ち切った待ちの状態がスタックに残った（中）**: タイムアウト後も `userdata1 = &wait` のコールバックが登録されたままで、後から動くと死んだフレームに書いた。ブロックする `wgpuDevicePoll` は 60 秒の打ち切りが効かなかった。
   待ちの状態を静的な 1 つにして世代番号を持たせ（古い世代のコールバックは何も書かず、遅れて来たアダプタ・デバイスは解放する）、poll は常にブロックせず、5 ms 続けて回したあとは sleep を挟む。読み戻しが終わらなかったデバイスは使わず、終了時に解放もしない。
7. **C のホストの `assert` が NDEBUG で消えた（中、テストの正しさ）**: Windows の `zig cc` は -O1 から NDEBUG を定義し、`assert(...)` の中の呼び出しが消える（hermetic な `unavailable` の検査も）。C のホストは `#undef NDEBUG` で始め、
   NDEBUG を定義してビルドし、`selftest` 引数で assert が効いていることを確かめる。

- テスト: `tests/gpu_runtime_host.c`（ランタイムを直接呼ぶ C のホスト）と `tests/gpu_runtime_fake.c`（偽の wgpu-native。無効なオブジェクトを返し、それを送信すると abort、ブロックする poll は応答しないデバイスで戻らない）で、
  バッファ 4 か所・バインドグループ・パイプライン・`finish` の失敗、デバイスの喪失、応答しない map とアダプタ、デバイスの上限を注入し、どれも状態で終わる（修正前は SIGABRT・SIGSEGV・hang）。実機の wgpu-native でも、壊れたシェーダー・
  入口のないパイプライン・レイアウトに合わないバインドグループ・上限の境界を同じホストで動かす。`tests/gpu_runtime_mock.c` の `-DMOCK_MARKER` で、作業ディレクトリに置いたライブラリが読み込まれないことを確かめる。WASM のホストは
  `tests/gpu_runtime_provider.mjs` の偽のプロバイダで、2 GiB 以上のアドレス、メモリ外、本物の上限、RangeError の別種を確かめ、メモリが十分あれば 2.4 GB の配列を持つモジュールでも動かす。`tests/gpu.rs` は、`_on` の `private`（E1022）と、
  `Gpu.__run` の lane の種類の比較を確かめる。`tests/gpu_runtime.mjs` は、descriptor の lane を書き換えた IR が「カーネルなし」でトラップすることを確かめる（修正前はそのまま動いて 4 を出力した）。`TSUZURI_SANITIZE=1` で C 側を ASan と UBSan で動かす。
- 確認していないこと: Windows の LoadLibraryExA と Sleep の分岐（スタブの `windows.h` での構文検査だけ）。この修正は `impl/f09-vulkan` の `gpu-vulkan.c` に触れていない。そこには、`libvulkan.1.dylib`・`libMoltenVK.dylib`・`vulkan-1.dll` のように、
  ディレクトリのない名前を `dlopen`／`LoadLibraryA` へ渡す候補が先頭にあり、macOS と Windows で上の 5 と同じ探索になっていた（Phase 3 のレビュー後の修正の 1 で直した）。

### Phase 3（done）

- 実装:
  - SPIR-V の生成: `src/gpu/spirv.rs`（`GpuKernel::spirv`・`spirv_relaxed`。独自の emitter で、LLVM の SPIR-V ターゲットも外部のツールも使わない。出力はバイトまで決定的）。SPIR-V 1.3（Vulkan 1.1 が基準）、`Shader`（64 bit の型があるときだけ `Int64`）、
    `GLCompute` の `map_main`（入力が 32 bit 整数のときだけ `init_main` も）、`LocalSize 256 1 1`、set 0 の binding 0（入力、`NonWritable`）・binding 1（出力）、push constant `{ u32 length; u32 base }`
    （1 次元のワークグループ数の上限を超える lane 数を、何回かの dispatch に分ける）。整数の `+`・`-`・`*` は折り返し、シフト量は型の幅 - 1 でマスクする。除算と剰余は CPU でトラップするので出さない。
    strict な `f32` のモジュールは、すべての浮動小数点演算の結果に `NoContraction` を付け、`SPV_KHR_float_controls` の `SignedZeroInfNanPreserve`・`DenormPreserve`・`RoundingModeRTE`（幅 32）を宣言し、
    `f32` から整数へのキャストを、範囲の比較と `OpIsNan` の選択に展開して CPU の飽和と NaN の規則を再現する。`f32` の `/` は `OpFDiv` の誤差（2.5 ULP）のため E1018。
    緩い（`spirv_relaxed`）は lane が `f32`・`i32`・`i32u`、float の装飾なし、`/` は使える。`spirv-val --target-env vulkan1.1`・`vulkan1.2` を通る。
  - CLI: `--emit spirv`・`--emit spirv-relaxed`（`Emit::Spirv`・`Emit::SpirvRelaxed`。拡張子 `.spv`。`BuildOptions` に field なし。`wgsl` と同じ検査で E2000・E2004・E1018。ビルドのキャッシュは使わない）。
  - デバイス対応: `Gpu.Vulkan` か `Gpu.Auto` を構築するプログラムだけが SPIR-V を埋め込み、`Gpu.WebGpu` を構築するプログラムだけが WGSL を埋め込む（`Sources::of`）。記述子は
    `{flags, lanes, features, wgsl, wgsl_len, spirv, spirv_len, weight}`（`features` は bit 0 が `shader-f16`、bit 1 が `i64`、bit 2 が strict な `f32`。lane の種類 4 は 64 bit 整数。`weight` は 1 lane が実行する命令の数の下限で、`if` は条件と安いほうの枝、`&&`・`||` は左の項だけ、呼ぶ関数はその重みを同じ規則で数える。分岐のないカーネルでは命令の総数）。
    `std/Gpu.tz` は `request_on` に `Vulkan` と `Auto` の arm、内部の組み込み関数 `Gpu.__select`・`Gpu.__last`（std 専用。E1022）、公開の `Gpu.last_backend`。`Gpu.request Gpu.Vulkan` はプログラム全体の機能の和で確かめ、`Gpu.Auto` はカーネルごとに確かめる。
  - ランタイム: `src/runtime/gpu-vulkan.c`（Vulkan の C API を必要な分だけ自前で宣言し、`dlopen`／`LoadLibrary`、`TSUZURI_VULKAN_LIBRARY`、portability 列挙、能力の明示的な確認、遅延でスレッド安全な初期化、全エラー経路の解放）。
    `gpu.c` は `backend == 2` をここへ回し、3 つ目の境界の関数 `tsuzuri_gpu_select(mode, lanes, features, spirv, spirv_len, weight, count)` を持つ（native だけ。WebAssembly では 0 を返し、import を足さない）。
    IR のマーカー `; tsuzuri-gpu: vulkan` があるときだけ、リンクに `gpu-vulkan.c` が入る（`llvm::gpu_runtime_source`）。ステータスは 0 成功、1 利用不可、2 機能不足またはカーネルなし、3 上限超過、4 実行時エラー。
  - 適合プローブ: float controls のプロパティは申告で、証明ではない（MoltenVK は `SignedZeroInfNanPreserve` を報告して `-(x * 0.0)` の符号を失い、SwiftShader は 2^31 以上の `i32u` から `f32` への変換を丸め違えた。独立したレビューが見つけた）。
    ランタイムは、プロパティを報告するデバイスに、strict な `f32` が要る最初の要求で、27 lane の SPIR-V（9 つの演算 × 3 組の入力。`tests/gpu_vulkan_probe.spvasm` を組み立てた語を `gpu-vulkan.c` に埋め込む。9 つ目は 2^31 を超える `i32u` から `f32` への丸めの境目で、切り捨てと、同点を上へ丸める変換を、最近接偶数と区別する）を 1 回動かし、
    CPU 参照のビット列と比べる。1 lane でも違うか、動かせなければ、そのプロセスでは strict な `f32` を許さず（`Unavailable`。`Gpu.Auto` はそのカーネルを CPU 参照で動かす）、理由は `TSUZURI_GPU_DEBUG` に出る。
    他のカーネルには影響しない。プローブは標本で、通ることは一致の証明ではない。
  - `Gpu.Auto`: `tz_vulkan_auto` が、呼び出しごとに「使えるか」（SPIR-V、能力、測った種類のデバイス = メモリを共有する統合 GPU、上限、パイプライン。デバイスが報告する内容だけで、論理デバイスを作る前に判断し、合うデバイスがなければ何も作らない）、「割に合うか」（CPU 参照の見積り `n × w × 0.03 ns` と、Vulkan の見積り
    `270 µs + n × b × 0.13 ns + n × w × 0.001 ns` の比較）、「初回の費用（デバイスを開く 30 ms、パイプライン 7 ms）を、候補が CPU で動いたときの節約の貯金で払えるか」を判断する。候補は Vulkan だけで、WebGPU には測った規則がないので選ばない。
    準備の失敗は CPU 参照で動かし、呼び出しの途中の失敗は明示的なデバイスと同じようにトラップする。`TSUZURI_GPU_AUTO_MIN_WORK=<n>` は規則を「lane 数 × 重みが n 以上」に置き換えるが、能力・上限・種類の確認は省かない。
    定数は `benchmarks/run-gpu-vulkan.mjs` の測定から作った、1 台のマシンの経験則（[性能測定](../../docs/benchmarks.md#vulkan-と-gpuautof09-phase-3)）。
- 決めた点（D10 に沿う）:
  ① SPIR-V は独自の emitter。LLVM 21〜23 の SPIR-V ターゲットは float controls の execution mode は出すが `NoContraction` を出さず（`contract` フラグの有無に関わらず 0 件）、Vulkan の仕様は `NoContraction` のない演算の積和と再結合を許すので、strict を満たせない。
  ② strict の `f32` の `/` は E1018（`OpFDiv` は 2.5 ULP）。`f32` から整数は範囲の比較と `OpIsNan` の選択で厳密に再現する。i64 の除算・剰余は strict の WGSL と同じ理由で出さない。
  ③ `Gpu.request Gpu.Vulkan` は、プログラムの SPIR-V カーネル全部が要る機能の和で確かめる（1 つでも欠ければ `Unavailable`）。`Auto` はカーネルごと。
  ④ `Gpu.Auto` の候補は Vulkan だけで、統合 GPU（メモリを共有し、転送がコピーなし）に限る。離散 GPU とソフトウェアの実装（SwiftShader）は、測った規則がないので選ばない。明示的な `Gpu.Vulkan` はそのデバイスでも動かす。
  ⑤ 選択の観測は、公開の `Gpu.last_backend ()`（プロセスで 1 つの atomic の値）と `TSUZURI_GPU_DEBUG` の行。
  ⑥ WebAssembly の `Gpu.request Gpu.Vulkan` は常に `Unavailable`、`Auto` は CPU 参照で、import は増えない。
  ⑦ 適合プローブは、ticket にはなく、レビューを受けて加えた（プロパティの報告だけでは strict と称さない、という D10 の方針の実装）。
- ticket から外れた点: ① `Gpu.Auto` は「閾値」ではなく、呼び出しの見積りと、初回の費用の貯金の規則にした（1 回では初回の費用に届かない呼び出しが、繰り返しで元を取れるため。閾値は環境変数で置き換えられる）。
  ② 境界の関数を 1 つ足した（`tsuzuri_gpu_select`）。std 専用の組み込み関数 `Gpu.__select`・`Gpu.__last` を `src/check.rs`・`src/polymorph.rs`・`src/llvm.rs` に登録した。
  ③ 記述子の欄を足した（`spirv`、`spirv_len`、`weight`）ので、デバイス対応のプログラムの IR は Phase 2 から変わる（デバイス対応でないプログラムは、std のテンプレートの連番を除いて同じ）。
  ④ `scripts/check-runtime-includes.sh` は、`gpu-vulkan.c` を `include_str!` する `src/llvm_gpu.rs` も走査する（37 ファイルを数える）。⑤ `tests/gpu.mjs` の `unavailable` は、`Auto` が `Ok` であることを期待する。
  ⑥ 公開の `Gpu.last_backend` を足した（ticket の「選択を観測可能にする」の実現）。
- 検証（Apple M1 Max、macOS、MoltenVK 1.4.2（Homebrew の Vulkan ローダー）と、Chrome 同梱の SwiftShader（LLVM 10、`i64` なし）の 2 つの実デバイス、Apple clang 21、SPIRV-Tools v2026.4）:
  - `cargo test --locked`: 全体 1,016 件が成功（86 の実行ファイル。デバイスのテストも実行）。`tests/gpu_spirv.rs` 16 件（構造、決定性、`spirv-val` の 54 組、拒否、2 つの実デバイスでの CPU 参照との照合、
    プローブの語と表、`mut` の引数と 100 本の `else if` の連鎖、重みの下限と上限）、`tests/gpu_vulkan_lowering.rs` 10 件（lane の種類の検査が Vulkan と `Auto` の腕に効くこと、`__*`・`_on` の `E1022`、記述子の重み）、`tests/gpu.rs` 16 件。
  - `node tests/gpu_vulkan.mjs` 51 件（宣言した ABI と実際のヘッダーの照合、能力と機能の表、デバイスの順位と機能の優先、ローダーの失敗と探索（作業ディレクトリのライブラリを読み込まない）、上限、`Gpu.Auto` の規則と貯金、ハイブリッド構成、デバイスを作らない `Auto`、
    N 番目の Vulkan 呼び出しを失敗させる掃引と、オブジェクト・割り当ての漏れの有無、デバイスの喪失、パイプラインの表の溢れ、実行中の待ちと `Auto` の判断（厳密な `f32` のプローブを含む）、明示的な要求の待ち、`TSUZURI_GPU_DEBUG`、ASan・UBSan・TSan、プローブの判定。合成した Vulkan ライブラリとハーネス）、
    `node tests/gpu_vulkan_language.mjs` 48 件（言語としての実行を CPU 参照と照合、トラッキングしたヒープで漏れ 0、ドライバのビルド、重みの下限、lane の種類の検査、WebAssembly の import なし。2 つの実デバイスで）。
  - 厳密な整数（`i64` を含む）の Vulkan 実行は、MoltenVK で CPU 参照とビット単位で一致した（direct と staged の転送の両方、lane 数は 1 から 100,003）。緩い `f32` は許容誤差の範囲。
  - strict な `f32`: MoltenVK は `DenormPreserve` と独立性を報告せず、SwiftShader は `RoundingModeRTE` を報告しないので、どちらも `Unavailable`（実際の結果）。プローブを強制すると、MoltenVK は 27 lane 中 7（`-(x * 0.0)` の符号 2、非正規化数 5）、
    SwiftShader は 1（`i32u` から `f32`）が違った。
  - 決まった入力の再現: `--emit spirv` の出力は同じソースでバイトまで一致、Vulkan も `Gpu.Auto` も構築しないプログラムの IR は、Phase 2 の版（`3d60e5c`）と、std のテンプレートの連番を除いて一致、`--freestanding` は `Gpu.Vulkan`・`Gpu.Auto`・`Gpu.WebGpu` を E2000 で拒否する。
  - `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`sh scripts/check-runtime-includes.sh`、`git diff --check`、GUIDE §3.1 の 4 つの回帰テスト、`node scripts/check-docs.mjs`（変更したページ）、
    Windows の型検査（`cargo check --all-targets --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc`）が成功。
- 確認していないこと: float controls をすべて報告して適合プローブも通るデバイスでの、strict な `f32` の CPU 参照との一致（そのデバイスがなく、モックと、報告しない実機での `Unavailable` までを確かめた）、
  Linux と Windows での実行（Windows の `gpu-vulkan.c` は、レビュー後の修正のあとに、`gpu.c` と一緒にした 1 つの翻訳単位を、clang の MSVC ターゲット（x86-64・aarch64）で SDK のヘッダーに対して `-fsyntax-only`（-O0 と -O2 -DNDEBUG、警告なし）、
  `zig cc`（Zig 0.16.0 の mingw-w64 ヘッダー）で windows-gnu と Linux の x86-64・aarch64 向けにコンパイルできるところまで。Rust は msvc 2 ターゲットの型検査まで）、NVIDIA・AMD・Intel の GPU と離散 GPU の転送
  （staged の経路は、MoltenVK の private なメモリ型で動かした）、Node.js 24 がない環境での WebAssembly の JSPI の実行（Phase 2 と同じ。Vulkan は WebAssembly で使えない）、適合プローブの費用、
  `Gpu.Auto` の定数の他のマシンでの当たり。MoltenVK は Metal の変換層で、その結果は Vulkan のドライバ一般のものではない。

### Phase 3 のレビュー後の修正（6 件）

独立したコードレビュー（`f0a0d1f` を起点）が、Phase 3 の Stage 2 の実装の欠陥を 6 件見つけた。2 つの emitter の数値（約 100 本の乱数プログラムを `spirv-val` と MoltenVK・SwiftShader で CPU 参照と照合）と、`f32` から整数への境界、C のランタイムの数値の経路に欠陥は見つからなかった。
`Gpu.__select`・`Gpu.__last` が利用者のコードから `E1022` になること、カーネル番号の範囲検査が符号なし比較であることも確認された。

1. **Vulkan のローダーを名前だけで読み込んだ（高、セキュリティ）**: `libvulkan.1.dylib`・`libMoltenVK.dylib`・`vulkan-1.dll` を、ディレクトリのない名前で `dlopen`／`LoadLibraryA` に渡していた。macOS の `dlopen` は作業ディレクトリを先に探すので、置かれたライブラリの初期化処理が、どの確認よりも前に動き、
   本物のローダーの代わりになった（レビューが再現。その後の init は、そのプロセスの間、状態 2 で失敗した）。既定の候補を、macOS は `/opt/homebrew/lib`・`/usr/local/lib` の絶対パスだけ、Windows は `LoadLibraryExA` の `LOAD_LIBRARY_SEARCH_SYSTEM32`、
   Linux は動的リンカーが作業ディレクトリを探さない soname（理由はコメントに書いた）にした。`vkGetInstanceProcAddr` を持たないライブラリは、`dlclose` して次の候補へ進む。`TSUZURI_VULKAN_LIBRARY` は書いたとおりに読み込む。C のホストは `#undef NDEBUG` で始める。
   テスト: 作業ディレクトリに置いた合成ライブラリ（コンストラクターがマーカーのファイルを作る）が読み込まれないこと（修正前のランタイムでは、マーカーができて、デバイス名が合成ライブラリのものになった）、裸の名前・存在しないファイル・エントリーのないライブラリ・本物の順の探索。
2. **`Gpu.Auto` の判断が実行中のカーネルの完了を待った（中）**: `tz_vulkan_run` が状態のロックを `vkWaitForFences` の間も持ち、CPU 参照と答える呼び出しの `tz_vulkan_auto` も同じロックを取った（レビューの測定: GPU の実行の間が 200 µs だと CPU の判断が 37 倍（最悪 13.7 ms）、間がないと 1 回の判断が 2.5 s）。
   ロックを 2 つに分けた（状態のロック `tz_vk_mutex` はフェンスの完了待ちのあいだ持たず、実行のロック `tz_vk_exec_mutex` は 1 回の実行のあいだ持つ。順は状態、実行）。`tz_vulkan_run` は `tz_vk_prepare`（状態）と `tz_vk_execute`（実行）に分かれ、`tz_vulkan_auto` は、式だけで決まる見積りで CPU と答える呼び出しにはロックを取らず、
   それ以外は状態のロックを待たずに試し、取れなければ CPU 参照にする（例外は、厳密な `f32` のプローブを最初に動かすスレッドの 1 回の入れ子の待ち）。`poisoned` は atomic にし、`tz_vk_execute` が状態とデバイスの喪失を取り直して確かめる。貯金の更新は状態のロックの内側にあり、整合は変わらない。
   テスト: `vkWaitForFences` が応答しない合成ライブラリ（`block_wait=1`）で、あるスレッドの実行がフェンスの待ちに入ったことを確かめてから、別のスレッドが 3 つの `Gpu.Auto` の判断（10 lane の呼び出し、初回の費用を払えない呼び出し、ランタイムの状態が要る大きな呼び出し）を出し、
   3 つとも、実行が終わらないうちに答えること（最後は、状態のロックが空いているときだけ出せる「Vulkan」）を、握手（待ちに入ったスレッドの数、解放の合図）で確かめる。時間の閾値は使わない。TSan でも同じ。状態のロックを実行のあいだ持たせる変更では失敗する。
3. **カーネルの重みが両方の枝の和だった（中）**: `result()` が、出した命令ごとに 1 を足すので、`if` が then と else の合計になった（レビューの例 `if v < 0 then r64 (r64 (r64 (r64 v))) else v + 1` は重み 1,282 で、1 lane が実行するのは 2 個の演算。`Gpu.Auto` は 30 回中 29 回を Vulkan に出し、レビューの測定では、1,000,000 lane で CPU 参照の 3.5 倍、4,000,000 lane で 2.4 倍遅かった。
   遅さは再現しておらず、判断（30 回中 29 回）だけを、合成ライブラリで再現した）。重みを、実行する道の下限にした（`if` は条件と、安いほうの枝、`&&`・`||` は左の項だけ、呼ぶ関数はその重みを同じ規則で）。分岐のない（直線の）カーネルの重みは変わらない。決定的。
   テスト: 重み 2 になること（`tests/gpu_spirv.rs`）、1,280 個の演算を持つ直線のカーネルと並べて、1,000,000 lane の呼び出し 30 回が、分岐のカーネルは 0 回（すべて CPU 参照）、直線のカーネルは規則が出した回数だけ dispatch されること（`tests/gpu_vulkan_language.mjs`。時間は測らない）。
4. **ハイブリッド構成で `Gpu.Auto` が統合 GPU を使えなかった（中、設計）**: 1 つのデバイスを順位（離散が先）と最初の呼び出しの機能で選び、そのあとで `Auto` が統合 GPU かを確かめたので、離散と統合のある機械では、`Auto` が離散 GPU の論理デバイスを作ってから、すべての呼び出しを断った。
   列挙と論理デバイスの作成を分け（`tz_vk_enumerate`・`tz_vk_create_device`）、`Gpu.Auto` の適格（統合 GPU、共有メモリ、動かせる、機能、バッファが上限に収まる）を、`vkCreateDevice` の前に、デバイスが報告する内容だけで判断する（`tz_vk_pick_auto`・`tz_vk_auto_fits`）。合うデバイスがなければ何も作らない。
   明示的な要求は、動かせる、機能を持つ、種類の順で選び、同順位なら先に列挙されたもの（`tz_vk_pick_explicit`）。**デバイスは 1 つのままにし、最初に来た要求が決める**（明示的な要求のデバイスと `Auto` のデバイスの 2 つを開くには、デバイスごとの状態と、実行の境界で 2 種類の呼び出しを区別する印が要り、小さな変更ではないと判断した）。
   統合 GPU を `Auto` が先に開けば、あとの明示的な要求もそれを使い、明示的な要求が先に離散 GPU を開けば、`Auto` は測った種類でないので、すべての呼び出しを CPU 参照で動かす。この規則を `gpu.md`・`docs/language.md`・`gpu-vulkan.c` の冒頭に書いた。
   合成ライブラリは、デバイスごとに種類・名前・機能・制限を設定でき（`d<N>.<key>`）、開いたデバイスと作ったデバイスの数を返す。テスト: 離散 #0 + 統合 #1（`Auto` は #1 を開き、離散は作らない）、統合のみ、離散のみ（`Auto` はデバイスを作らない）、明示的な要求が先なら #0。
5. **適合プローブが、切り捨ての符号なし変換を見逃した（中）**: `OpConvertUToF` の 3 つの lane（`0xFFFFFF7F`・`0x80000001`・`0x01000001`）は、0 への丸めでも最近接偶数でも同じビット列で、符号なしの変換が切り捨てのデバイスでも、符号ありの変換が正しければ通った。
   9 つ目の演算（3 lane）を足した: `0x80000081`（切り捨てと最近接偶数が違う）、`0x80000080`（ちょうど半分で、偶数側）、`0x80000180`（ちょうど半分で、奇数側。最近接偶数は上、切り捨ては下）。期待値は、同じオペランドの Rust の `as f32` と C の変換から求め、識別する組が違うことを検査する。
   プローブは 27 lane × 9 演算（557 語。以前は 24 lane × 8 演算、538 語）。合成ライブラリの `u32=trunc`・`u32=half_up` で、新しいプローブは lane 24・26 と 20・25 で違いを見つけ、古いプローブは切り捨てを通していた（`different=0`）。2 つの実デバイスの違う lane は変わらない（MoltenVK は 3・5・6・7・8・9・10、SwiftShader は 18）。
6. **4 つの実行時の振る舞いに、それを確かめるテストがなかった（中、テスト）**: デバイスの順位の反転、機能の優先の無視、貯金の消費（`tz_vk_auto_credit -= first_use`）の削除、`VK_ERROR_DEVICE_LOST` での `poisoned` の削除は、どれも 29 件が成功した。デバイスごとの合成ライブラリで、
   順位（開いたデバイスの報告）、機能の優先（`shaderInt64` を持つデバイスを、`i64` のカーネルが要るときだけ選ぶ）、2 つのカーネルの貯金（1 つ目のカーネルがデバイスと自分のパイプラインの費用を払うと貯金はその分減り、2 つ目は、自分のパイプラインの費用を自分の節約で払うまで CPU 参照のまま。
   答えは、C のソースから読んだ定数のモデルと一致し、貯金は負にならない）、デバイスの喪失（あとの呼び出しが、喪失したデバイスに Vulkan の呼び出しを 1 回もせずに失敗し、`Auto` は CPU 参照）、パイプラインの表の溢れ（300 個のカーネルを動かしても、表は 256 個のまま、すべて正しく動く）を確かめる。
   **テストを元に戻す変更で失敗することを、1 件ずつ確かめた**: 順位の反転、機能の優先の無視、動かせるかの優先の無視、同順位で最後を選ぶ（`>=`）、
   貯金を消費しない、`poisoned` を立てない、状態のロックを実行のあいだ持つ、`Auto` が順位でデバイスを開く、一時のパイプラインを解放しない。
- `Gpu.__run` の lane の種類の検査: Phase 2 の修正で入ったガードを、Vulkan と `Auto` の腕、`Gpu.__select`、64 bit の lane（`LANE_64`）まで試験した（`tests/gpu_vulkan_lowering.rs` の 2 件。記述子の lane を書き換えた IR が「カーネルなし」でトラップする言語のテスト）。
  `Gpu.__select` は総称でないので lane の種類を比べられず、メモリを守るのは `Gpu.__run` の検査。`__select`・`__run` は、番号を符号なしで比べる。利用者が `__*` の関数と `_on` の関数を呼ぶと、すべて `E1022`。
- 確認していないこと: Linux と Windows のローダーの探索（macOS の絶対パスと、作業ディレクトリのライブラリを読み込まないことだけを動かした。`LoadLibraryExA` は型検査まで）、実際の離散 GPU・ハイブリッドの機械（デバイスの選び方は合成ライブラリだけ）、
  レビューが測った 3.5 倍・2.4 倍の遅さの再測定（判断だけ再現）、`Gpu.Auto` の定数の静かなマシンでの測り直し（定数は負荷の高いマシンでの値）。ランタイムは `VK_*`・ICD の変数を読みも書きもしない（試験だけが設定する）。Phase 2 のレビューの 4 つの種類の欠陥
  （上限をアダプタから読む、失敗したオブジェクトのキャッシュ、スタックの待ちの状態、符号付きのポインタ）は、Vulkan のランタイムでは該当しなかった（上限は開いたデバイスから、失敗した作成は何もキャッシュしない、待ちの状態は呼び出しの間に残らない、ポインタは `uint64_t`・`size_t`）。

### Phase 3 の 2 回目のレビュー後の修正（3 件）

独立したレビュー（`b618b6a..8ef2568` と `c1f974e`）は、ロックの順序、競合、パイプラインの表、デバイスの喪失、デバイスの選択、ローダー、プローブの定数、lane の種類の検査、`E1022` に欠陥がないことを確かめたうえで、2 件の欠陥と 1 件の不整合を見つけた。

1. **最初に厳密な `f32` のプローブを要する `Gpu.Auto` の判断が、別のスレッドのカーネルの完了を待った（中）**: `tz_vk_run_nested` が、状態のロックを持ったまま実行のロックを待っていた。実行のロックは、カーネルのフェンスの完了待ち（時間切れがない）のあいだ、動いているスレッドが持つので、その判断は、カーネルが終わるまで答えず、
   そのあいだ状態のロックも持つので、ほかの `Auto` は CPU 参照を答え、`Gpu.request`・明示的な実行・`tz_vulkan_describe` は状態のロックで待った（レビューの再現: 合成ライブラリ `strict=1,block_wait=1` で、カーネルを 1 つ動かしたまま、別のスレッドが厳密な `f32` のカーネルを `Gpu.Auto` に尋ねると、700 ms 経っても答えず、
   カーネルが解放されてからプローブが動いた）。`tz_vk_run_nested` に `wait` を足し、`Gpu.Auto` の判断（`tz_vk_quiet`）は実行のロックを `TZ_VK_EXEC_TRY_LOCK` で試すだけにした。取れなければ、内部の状態 `TZ_VK_BUSY`（どの入口の関数も返さない）を返し、プローブは**結果を記録せず**（`strict_probe` は `not-run` のまま、
   報告された `strict_f32` も変えない）、判断は CPU 参照を答え、貯金からコンパイルの分として引いた額を返す（コンパイルは行われていない）。カーネルが終わったあとの判断が、プローブを動かし、結果を 1 回だけ記録する。`tz_vk_strict_ok` は「デバイスが報告し、プローブが通った」ときだけ 1 を返し、
   先送りは `tz_vk_probe_pending` で、拒否と区別する。明示的な要求（`Gpu.request Gpu.Vulkan`、明示的なデバイスでの実行）は、これまでどおり待つ（状態のロックを持ったまま、動いているカーネルの完了を待ってから、プローブを 1 回動かして結果を記録する）。
   テスト: ハーネスの新しいモード `blockedprobe`（1 つのスレッドのカーネルが待ちに入ったことを確かめてから、別のスレッドが `Auto` に 2 回尋ねる。両方が、カーネルがまだ待っているうちに答え、プローブは動かず、結果もなく、カーネルのあとの判断がプローブを 1 回だけ動かして `1` を答える。
   プローブが失敗するデバイスでも同じで、結果は 1 回だけ記録される）、貯金の返却（保存した定数から、1 回の節約がコンパイルの 0.6 になる lane 数を求め、返却があれば、カーネルのあとの最初の判断で成立し、なければ 1 つ遅れる）、`blockedopen`（明示的な要求が待ちの中にいて、カーネルが終わるまで戻らず、そのあいだ `Auto` は待たずに CPU 参照を答え、
   カーネルのあとでプローブを 1 回動かして、通ったときも失敗したときも結果を記録する）、ThreadSanitizer の 1 件。待ちの確認は握手で行い、時間の閾値は使わない（`blockedopen` の戻りの有無だけは、不具合が見えるように 200 ms の猶予を置くが、正しいランタイムはそれに左右されない）。
   変異: 判断が実行のロックを待つ（レビューの不具合。`decided_while_running=0`）、先送りで結果を残す、貯金を返さない、明示的な要求が待たない（`open=5`、内部の状態が入口に漏れる）で、それぞれのテストが失敗することを確かめた。
2. **2^31 以上のカーネルの重みが 1 と読まれた（低）**: 重みは `u32` の飽和加算で、記述子の欄は `i32` なので、`i32` を超える重みは負の数（`4294967295` は -1）として書かれ、ランタイムは `weight > 0 ? weight : 1` で重み 1 と数え、とても重いカーネルを一度もデバイスに出さなかった
   （結果は正しい。33 個の関数が次の関数を 2 回ずつ呼ぶ連鎖で、`--emit llvm` が `i32 4294967295` を出した）。`SpirvKernel::weight` を 1 から `i32::MAX`（`MAX_WEIGHT`）に収める。テスト: `tests/gpu_spirv.rs`（深さ 1・2・30 は正確な重み 2・4・2^30、31・32・33・40 は 2,147,483,647）、
   `tests/gpu_vulkan_lowering.rs`（IR の記述子の重みが 2,147,483,647 で、`4294967295` がなく、ほかのカーネルの重みは変わらない）、`tests/gpu_vulkan.mjs`（C 側が 2,147,483,647 を最大の重さとして扱う）。クランプを外すと、Rust の 2 件が `4294967295` で失敗する。
3. **`TSUZURI_GPU_DEBUG` の読み方が 2 つのランタイムで違った（不整合）**: `gpu-vulkan.c` は `"0"` をオフとし、`gpu.c` と文書は、空でない値をすべてオンとしていた。`gpu-vulkan.c` を文書に合わせた（空でなければオン。`"0"`・`"false"` もオン）。
   テスト: `tests/gpu_vulkan.mjs`（`1`・`0`・`false`・`off` で、ローダーの理由と `Auto` の理由が出て、空と未設定では何も出ない）、`tests/gpu_runtime.mjs`（WebGPU のランタイムが `0`・`false` でオンで、空でオフ）。`"0"` をオフに戻すと、Vulkan のテストが失敗する。
- 確認していないこと: 前の節と同じ（Linux と Windows の実行、離散 GPU とハイブリッドの実機、全部の float controls を報告する実機での厳密な `f32` の一致）。先送りの競合は合成ライブラリでだけ確かめた（実機の MoltenVK・SwiftShader は厳密な `f32` を報告せず、プローブに入らない）。
  Windows の `TryAcquireSRWLockExclusive`（実行のロック）は、clang（MSVC の SDK のヘッダー）の `-fsyntax-only` と `zig cc` でのコンパイルまでで、動かしていない。
