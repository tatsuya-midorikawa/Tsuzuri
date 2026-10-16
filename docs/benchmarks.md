# 性能測定

**C/C++ と同等以上の実行性能を達成することは Tsuzuri の重要な開発目標ですが、初版においてあらゆるケースでの性能優位を一般的に保証するものではありません。**
LLVM バックエンドを採用していること自体が直ちに C を上回る性能を意味するわけではなく、生成されるコードの形状、値の不要なコピーの有無、最適化パスの適合度、WebAssembly 実行エンジンの特性、ならびにホスト境界呼び出しの粒度に強く依存します。

整数ミキサーなどの演算処理における性能上の基盤は、ヒープ動的確保／GC／参照カウントの完全な排除、自己末尾再帰の確実なループ化、型付き LLVM 値の直接利用、デフォルトの `-O3` 最適化、および WebAssembly における不要コードの削除です。
安全性検査を無効化したり、IEEE 754 の厳密な浮動小数点意味論を犠牲にしたりして、見かけ上の速度を稼ぐような不健全な最適化は行いません。

WebAssembly の SIMD128 機能は明示的なオプトイン（opt-in）制となっています。2026-09-27 実施の `tests/wasm_simd.mjs` では、LLVM 21 の逆アセンブル解析を通じて `-O3` 出力にベクトル命令が正しく生成されていること、デフォルト出力にはベクトル命令が含まれないこと、および演算結果の一致を確認しました。
ただし、これはコード生成の正しさを検証したものであり、速度向上の厳密な測定ではありません。SIMD による高速化を報告する場合は、機能フラグの有無、同一の入力データ／配列長／最適化レベル、実行エンジンおよびバージョンの条件を揃えた中央値と、生成された機械語命令を併記し、共有 CI に恣意的な速度閾値を設けることはしません。

WASM のマルチスレッド（threads）機能については、2026-09-28 に Node.js の Worker におけるアトミックバリアを用いた並行実行への参加、およびヒープメモリの正常な回収を確認しました（速度の優位性自体は未測定です）。
性能比較を行う際は同一の Task／Parallel 入力データを用い、WASM 逐次実行と threads の実経過時間（wall time）、スレッドプールの初回起動時間を含むケース、およびプール再利用時のケースを明確に分離して評価します。
ワーカー数、Node.js バージョン、CPU アーキテクチャ、最適化レベル、SIMD 指定、ならびにメモリ／スタック容量を記録し、生成された WASM 内のアトミックな wait/notify 経路を確認します。
なお、グローバルなヒープおよびキューのロックは競合時の性能上限要因となり得ます。ワーカーごとの独立アロケータやワークスティーリング機構は現時点で未実装であり、実機での厳密な計測なしに並列による高速化を主張することはありません。

GPU 実行の測定試作は `node benchmarks/run-gpu.mjs target/release/tsuzuri [--quick]` で実行可能です（`--quick` は CPU 参照実装との等価性検証のみを行います）。
`TSUZURI_WEBGPU=1` 環境変数設定時には実際の GPU アダプタが要求され、デバイス初期化、パイプライン生成、データ転送／ディスパッチ／同期／読み戻し、およびデータ常駐の 3 段階処理の所要時間をそれぞれ分離して JSON へ記録します。
常駐実行時間にはデータ転送と最終読み戻しは含まれませんが、各ディスパッチごとの同期オーバーヘッドは含まれます。表内の CPU 列は WASM スカラーエクスポート関数のホスト呼び出し時間（入力 261〜263 個を 30 回走査した合計。1 回分の 1/30 ではありません）であり、GPU によるバルク一括処理との直接の速度比を示すものではありません。
2026-09-28 には、Dawn Node binding 0.6.1 の Metal バックエンド上で、厳密な `i32` シェーダーの初期化、マッピング、および常駐チェーンの参照照合を実施しました。
制約の根拠は [WGSL 仕様](https://gpuweb.github.io/gpuweb/wgsl/#floating-point-accuracy) に基づきます。ハードウェアネイティブな 64-bit 整数型は存在せず、浮動小数点演算においては式の再結合、融合演算、および非正規化数の扱いに差異が許容されているためです。

### 緩い f32 カーネルの GPU 測定（F09 Phase 1）

2026-10-10 に、`--emit wgsl-relaxed` の 5 つのカーネル（`x * x + x`、3 次の Horner 式、`(x + 1) / (x * x + 2)`、`(i as f32) * 0.5 + 0.25`、`if x * x > 2.0 then 1 else 0`）を 263 個の入力で実行しました。
比較は、native/WASM の厳密な CPU 参照とは別に、JavaScript の `Math.fround` で一演算ごとに丸めた値に対して行い、GPU の結果だけを許容誤差（演算ごとの重みの和。`+`・`-`・`*` は 2 ulp、`/` は 3 ulp、整数から `f32` への変換は 1 ulp。常駐 3 回の連鎖は 3 × 2 倍）で照合します。
入力は相殺のない `[2^-8, 2^8]` の正規数に限り、特殊値（0、-0、非正規化数、NaN、無限大）は結果の長さだけを見ます。JSON の列は、これまでの列に `float`（`"relaxed"`）、`tolerance_ulps`、観測した `max_ulp_error` を足したものです。

条件は次のとおりです。Apple M1 Max（10 コア）、アダプタは Dawn の Metal（`metal-3`、`apple-m1-max`）、Node.js v20.19.6、`webgpu` 0.6.1、native コンパイラは `cargo build --release` の `-O3`、WASM は `-O3`。
測定中、共有の作業機では他のビルドが動いており、ロードアベレージは 4.5〜7.6 でした。値は 9 回の中央値で、括弧は最小〜最大です（ms）。生データは `target/perf/F09-phase1-final/run-1.json`〜`run-9.json`（`target/` は追跡されません）にあります。

| カーネル（入力数） | パイプライン作成 | 転送・dispatch・同期・読み戻し | 常駐 3 回 | CPU WASM（30 走査） | 最大誤差 / 許容 |
| --- | --- | --- | --- | --- | --- |
| 厳密 `mix`（261） | 3.50（2.97〜20.0） | 2.98（2.74〜5.12） | 1.62（1.43〜1.78） | 1.33（1.26〜3.94） | 完全一致 |
| 厳密 `cast`（261） | 2.64（2.25〜2.72） | 1.77（1.49〜2.19） | 1.27（1.09〜1.70） | 0.49（0.48〜0.50） | 完全一致 |
| 厳密 `locals`（261） | 3.43（2.91〜3.67） | 1.93（1.68〜2.65） | 1.31（1.26〜1.60） | 0.50（0.49〜0.52） | 完全一致 |
| 緩い `poly`（263） | 1.47（1.27〜1.60） | 1.91（1.62〜2.83） | — | 0.63（0.60〜1.31） | 1 / 4 ulp |
| 緩い `horner`（263） | 1.31（1.03〜1.90） | 1.13（0.95〜1.27） | — | 0.58（0.54〜0.65） | 1 / 12 ulp |
| 緩い `ratio`（263） | 0.97（0.85〜1.20） | 0.85（0.78〜1.17） | 1.14（1.02〜1.93） | 0.56（0.54〜0.63） | 2 / 9 ulp |
| 緩い `index`（263） | 1.05（0.81〜1.18） | 0.84（0.70〜0.99） | — | 0.38（0.37〜0.59） | 0 / 5 ulp |
| 緩い `threshold`（263） | 1.05（0.90〜1.45） | 0.86（0.75〜0.99） | — | 0.52（0.38〜0.56） | 完全一致 |

デバイスの起動（アダプタとデバイスの取得）は中央値 13.4 ms（12.2〜71.5 ms）です。
この大きさ（260 個前後）では、転送・dispatch・完了待ち・読み戻しの 1 回が 0.7〜3 ms で、CPU の WASM は 30 走査の合計で 0.4〜1.3 ms（1 走査は 1/30）です。固定費が支配するため、**GPU が速いという主張はしません**。バルクで大きな入力を使った、同じ Tsuzuri プログラムの CPU 参照と GPU の比較は、次の節にあります。
これは速度の合否の閾値ではなく、共有 CI にも入れません。

### GPU デバイスのランタイム（F09 Phase 2）

`node benchmarks/run-gpu-device.mjs target/release/tsuzuri [--runs 9] [--json]` は、同じ Tsuzuri プログラムを `Gpu.request Gpu.CpuReference` と `Gpu.request Gpu.WebGpu` のデバイスで動かします。1 回の呼び出しは `Gpu.to_array (Gpu.map_relaxed device kernel (Gpu.from_array device values))` で、ホスト配列の複製、アップロード、実行、完了待ち、読み戻し、配列への複製を、GPU の側に全部含めます。カーネルは、`f32` の 3 次式（lane あたり乗算 3 回、加算 3 回の `light`）と、`x * 1.0001 + 0.0001` を 64 回つないだ `heavy` です。native は wgpu-native 29（`TSUZURI_WEBGPU_LIBRARY`）、WebAssembly は Dawn の Node.js バインディングと JSPI（Node.js 24 以降）で動かします。

2026-10-10、Apple M1 Max（10 コア）、macOS 27.0.1、アダプタは Metal（`metal-3`）、wgpu-native 29.0.1.1（Homebrew）、Node.js v24.21.0、`webgpu` 0.6.1、native は `-O3`、WebAssembly は `-O3` の `wasm32` に `--wasm-feature webgpu --wasm-max-memory 256MiB` です。CPU 参照は 1 スレッドで、WebAssembly は SIMD なしです。共有の作業機では他のビルドが動いており、測定終了時のロードアベレージ（1 分）は 4.9 でした。値は、`--runs 9` の 9 回の中央値です（native は実行のたびに新しいプロセス、WebAssembly は新しいインスタンスと新しいデバイス）。1 回あたりの時間（µs）で、括弧は GPU の最小〜最大です。「最初の呼び出し」はパイプライン作成を含み、「デバイスを開く」は `Gpu.request Gpu.WebGpu` の時間です（native は `dlopen`、インスタンス、アダプタ、デバイスで、WebAssembly は Dawn の読み込みと `create()` を含みません）。生データは `target/perf/F09-phase2/device.json`（`target/` は追跡されません）にあります。

| ターゲット | カーネル | lane 数 | CPU 参照 µs/回 | WebGPU µs/回（最小〜最大） | 最初の呼び出し ms | デバイスを開く ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| native | light | 1,024 | 0.3 | 462（426〜487） | 6.3 | 20.9 |
| native | light | 16,384 | 5.8 | 556（544〜611） | 7.5 | 22.4 |
| native | light | 262,144 | 74.8 | 983（902〜1,109） | 7.7 | 20.5 |
| native | light | 4,194,304 | 1,401 | 7,866（7,700〜8,393） | 13.3 | 20.1 |
| native | heavy | 1,024 | 12.4 | 455（420〜467） | 6.4 | 22.4 |
| native | heavy | 16,384 | 200.6 | 583（506〜595） | 8.2 | 23.3 |
| native | heavy | 262,144 | 3,147 | 997（924〜1,055） | 8.5 | 24.4 |
| native | heavy | 4,194,304 | 50,507 | 7,882（7,468〜8,238） | 14.5 | 22.8 |
| WebAssembly | light | 1,024 | 1.3 | 400（379〜6,578） | 3.6 | 1.3 |
| WebAssembly | light | 16,384 | 20.4 | 500（475〜15,540） | 4.2 | 1.1 |
| WebAssembly | light | 262,144 | 358 | 1,589（1,247〜16,636） | 5.2 | 1.3 |
| WebAssembly | light | 4,194,304 | 5,430 | 21,495（12,376〜24,161） | 16.6 | 1.2 |
| WebAssembly | heavy | 1,024 | 48.9 | 406（385〜8,467） | 7.2 | 1.2 |
| WebAssembly | heavy | 16,384 | 796 | 516（474〜24,018） | 8.3 | 1.2 |
| WebAssembly | heavy | 262,144 | 12,607 | 1,200（1,058〜16,005） | 8.9 | 1.2 |
| WebAssembly | heavy | 4,194,304 | 205,161 | 12,709（12,330〜13,154） | 76.9 | 1.2 |

完了待ちを、ブロックする poll から、ブロックしない poll と sleep に替えたあと（Phase 2 のレビューの修正。ブロックする poll は 60 秒の打ち切りが効かないため）、native の行を測り直しました（同じ機械、9 回の中央値、同じ手順）。light は 1,024 lane が 437 µs（前 462）、16,384 lane が 521 µs（前 556）、262,144 lane が 966 µs（前 983）、4,194,304 lane が 7,565 µs（前 7,866）、heavy の 4,194,304 lane が 7,463 µs（前 7,882）で、遅くなっていません（共有の機械なので、差は雑音の範囲と見ます）。WebAssembly の行は、ホストの JavaScript の待ち方が変わっていないので、測り直していません。

読み方は次のとおりです。

- 呼び出しごとの転送と同期があるので、GPU の側には、lane 数によらない固定の時間があります。native は約 0.45 ms、WebAssembly（Dawn）は約 0.4 ms で、`light` のように 1 lane あたりの計算が小さいカーネルでは、測った全部の lane 数で CPU 参照より遅く、4,194,304 lane でも native は 5.6 倍、WebAssembly は 4.0 倍遅いままでした。
- 1 lane あたりの計算が大きい `heavy` では、lane 数が増えると逆転します。native は 16,384 lane で GPU が 2.9 倍遅く、262,144 lane で 3.2 倍速く、4,194,304 lane で 6.4 倍速くなりました。WebAssembly の CPU 参照は SIMD なしなので、逆転は 16,384 lane で起き、4,194,304 lane で 16 倍です。この比は、この 2 つのカーネルと、この機械の 1 回の測定だけの値で、別のカーネルや GPU には当てはまりません。
- WebAssembly の最大値には、数 ms から 24 ms の外れ値があります（中央値は安定）。ホストが JSPI で待つ間の JavaScript の事象ループの揺らぎによるものと考えられますが、原因は調べていません。
- native の完了待ちは、最初の実装（ブロックする poll だけ）では 1 呼び出しが 1.6〜1.7 ms（1,024 lane の `light`、1 回の測定）で、wgpu-native 29 の Metal での待ちが 1 ms を超えていました。最初の 5 ms をブロックしない poll で回す現在の実装は 0.46 ms です（9 回の中央値）。待ちを短縮した方法は、より長いカーネルでは 5 ms のあと、ブロックする poll に切り替わります。

GPU 常駐バッファがない（呼び出しごとの転送）ことが、小さい lane 数での固定費の大きな部分です。この測定は速度の合否の閾値ではなく、共有 CI にも入れません。

### Vulkan と Gpu.Auto（F09 Phase 3）

`node benchmarks/run-gpu-vulkan.mjs target/release/tsuzuri [--runs 3] [--json]` は、`Gpu.Auto` の規則（`src/runtime/gpu-vulkan.c` の `TZ_VK_AUTO_*`、[`Gpu.Auto`](../_tsuzuri/language-reference/built-in-types-and-modules/gpu.md#gpuauto-で呼び出しごとに選ぶ)）の定数を、同じ Tsuzuri のカーネルを CPU 参照と Vulkan で動かして測ります。

**方法。** カーネルは、厳密な `i32 -> i32` に、乗算・加算・シフト・XOR の 1 ステップを `rounds` 回（1、4、16、64、256）つないだもので、`Gpu.Auto` が使う重み（1 lane の命令数）は 5、20、80、320、1,280 です。lane 数は 1,000 から 4,000,000 までの 5 通りです。1 回の呼び出しは `Gpu.map device kernel (Gpu.from_array device values)` で、`values` は `Bench.with_input` が用意したホスト配列、結果は捨てます。入力の複製、アップロード、実行、完了待ち、読み戻し、出力の確保と解放が、全部時間に入ります。呼び出しの間にデバイスに残るものはありません。`tsuzuri bench` が、ベンチごとに別のプロセスで、ウォームアップのあとに動かして中央値を出します。1 つの cell で 5 つのベンチを動かします。

| ベンチ | 内容 |
| --- | --- |
| `reference` | 明示的な `Gpu.CpuReference` のデバイス（コンパイラがバックエンドを知っているので、ループを特殊化する） |
| `vulkan` | 明示的な `Gpu.Vulkan` のデバイス。ウォームアップで、開いてパイプラインを作ったあと |
| `auto` | `Gpu.Auto` のデバイス（新しいプロセス）。最初の呼び出しは CPU 参照で動き、節約の貯金が初回の費用に届いてから Vulkan を使うので、ベンチの呼び出しの範囲では、まだ CPU 参照のこともあります |
| `fallback` | Vulkan のライブラリを無効にした（`TSUZURI_VULKAN_LIBRARY=""`）`Gpu.Auto`。`Auto` が Vulkan を断ったときの CPU の道 |
| `copy` | 入力の複製だけ。どのデバイスも払う費用で、見積りから引く |

初回の費用（デバイスを開く時間、最初の呼び出しの時間）は、別のプログラムを 5 つのプロセスで動かした中央値です。もう 1 つのプログラムは、温まった `Gpu.Auto` のデバイスに、各 cell でどちらを選ぶかを尋ねます（`warm Auto`）。3 つのデバイスの結果のチェックサムは、全 cell で一致しました。

**環境。** 2026-10-10、Apple M1 Max（10 コア）、macOS（Darwin 27.0.0）、Vulkan は MoltenVK 1.4.2（Homebrew）と Homebrew の Vulkan ローダー（デバイスが報告した API は 1.3.357）、コンパイラは `cargo build --release`、ベンチのプログラムは既定の `-O3` です。**測定中、共有の作業機では他のビルド、試験、エディタが動いていて、ロードアベレージ（1 分、10 コア）は測定の前が 10.2、後が 10.6（5 分と 15 分は 12〜14）でした。** 静かなマシンでの値ではありません。静かな時間を、ロードアベレージが 3 または 5 以下になるまで待つ形で約 1 時間探しましたが、来ませんでした（他の担当者の全試験と、止まらない試験のプログラム 2 つが CPU を使い続けていました）。このため、この表を最終の測定としています。実行ごとの揺れは 1.5 倍前後あります（1,000 lane の Vulkan の呼び出しは、4 回の実行で 0.25〜0.41 ms でした）。

ms、中央値、入力のコピーを含みます。`warm Auto` は、温まった `Gpu.Auto` が選んだ側で、`速い側` は、`fallback` と `vulkan` の速いほうです。

| 重み | lane 数 | CPU 参照 | Vulkan | Auto（新プロセス） | 入力のコピー | warm Auto | 速い側 |
| ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| 5 | 1,000 | 0.000 | 0.255 | 0.000 | 0.000 | CPU | CPU |
| 5 | 10,000 | 0.002 | 0.345 | 0.002 | 0.001 | CPU | CPU |
| 5 | 100,000 | 0.016 | 0.476 | 0.016 | 0.007 | CPU | CPU |
| 5 | 1,000,000 | 0.160 | 1.363 | 0.164 | 0.106 | CPU | CPU |
| 5 | 4,000,000 | 0.871 | 4.881 | 0.869 | 0.415 | CPU | CPU |
| 20 | 1,000 | 0.000 | 0.261 | 0.000 | 0.000 | CPU | CPU |
| 20 | 10,000 | 0.003 | 0.332 | 0.004 | 0.001 | CPU | CPU |
| 20 | 100,000 | 0.033 | 0.465 | 0.033 | 0.007 | CPU | CPU |
| 20 | 1,000,000 | 0.363 | 1.446 | 0.359 | 0.116 | CPU | CPU |
| 20 | 4,000,000 | 1.436 | 4.971 | 1.411 | 0.418 | CPU | CPU |
| 80 | 1,000 | 0.001 | 0.331 | 0.002 | 0.000 | CPU | CPU |
| 80 | 10,000 | 0.014 | 0.303 | 0.015 | 0.001 | CPU | CPU |
| 80 | 100,000 | 0.144 | 0.435 | 0.143 | 0.007 | CPU | CPU |
| 80 | 1,000,000 | 1.441 | 1.397 | 1.533 | 0.116 | Vulkan | Vulkan（2 % 差） |
| 80 | 4,000,000 | 5.845 | 4.902 | 5.266 | 0.415 | Vulkan | Vulkan |
| 320 | 1,000 | 0.012 | 0.376 | 0.012 | 0.000 | CPU | CPU |
| 320 | 10,000 | 0.118 | 0.322 | 0.118 | 0.001 | CPU | CPU |
| 320 | 100,000 | 1.176 | 0.488 | 0.509 | 0.007 | Vulkan | Vulkan |
| 320 | 1,000,000 | 11.832 | 1.563 | 2.067 | 0.123 | Vulkan | Vulkan |
| 320 | 4,000,000 | 47.478 | 6.274 | 6.065 | 0.406 | Vulkan | Vulkan |
| 1,280 | 1,000 | 0.088 | 0.307 | 0.087 | 0.000 | CPU | CPU |
| 1,280 | 10,000 | 0.871 | 0.344 | 0.875 | 0.001 | Vulkan | Vulkan |
| 1,280 | 100,000 | 8.794 | 0.559 | 0.711 | 0.007 | Vulkan | Vulkan |
| 1,280 | 1,000,000 | 87.457 | 2.716 | 2.801 | 0.095 | Vulkan | Vulkan |
| 1,280 | 4,000,000 | 350.041 | 8.472 | 11.223 | 0.407 | Vulkan | Vulkan |

初回の費用は、デバイスを開くのが 25.7〜30.1 ms（5 つのカーネルの、5 つのプロセスの中央値）、最初の呼び出し（パイプラインの作成を含む）が 3.8〜6.5 ms、2 つ目の呼び出しが 0.36〜0.42 ms でした。

**規則の定数。** 見積りは、この表と、前の 3 回の実行（最初の 1 回は、このスクリプトの前の版の、時計を直接読む測り方）から、入力の複製を引いた自分の時間で作りました。

| 定数 | 値 | 測った値（4 回の実行。最後がこの表） |
| --- | --- | --- |
| 呼び出しの固定費 `TZ_VK_AUTO_CALL_NS` | 270 µs | 254 µs、278 µs、409 µs、255 µs（ロードが高い回は大きい） |
| 1 byte あたり `TZ_VK_AUTO_BYTE_NS` | 0.13 ns | 0.146、0.12、0.118、0.134 ns |
| デバイスの 1 命令あたり `TZ_VK_AUTO_GPU_NS_PER_OP` | 0.001 ns | 0.0008、0.0007、0.0007、0.0007 ns（切り上げ） |
| デバイスを開く `TZ_VK_AUTO_OPEN_NS` | 30 ms | 33.4、27.1、27.0、29.1 ms |
| パイプラインの作成 `TZ_VK_AUTO_COMPILE_NS` | 7 ms | 6.9、6.5、5.4、4.1 ms |
| CPU 参照の 1 命令あたり `TZ_VK_AUTO_CPU_NS_PER_OP` | 0.03 ns | 中央値 0.017、0.017、0.019 ns、範囲 0.006〜0.069 ns（`tsuzuri bench` を使った 3 回の実行） |

CPU 参照の 1 命令あたりの時間は、cell ごとに 6〜11 倍ほど違います。同じ重みのカーネルでも、ビルドによって 5〜6 倍違う時間になることがありました（前の回では、`Auto` の CPU の道が、明示的な `CpuReference` のデバイスより、軽いカーネルで最大 5 倍遅かったことがあります。この回は、両者が 3 % 以内で一致しました）。重み 320 のカーネルを CPU 参照で動かす単独のプログラムの機械語（`otool -tV`）には、NEON のベクトル命令が 134 個あり、ループが自動でベクトル化されていました。ベクトル化されるかどうか、コンパイラがバックエンドを知って特殊化するかが、差の一因と考えていますが、前の回の遅い版の機械語は調べておらず、原因は確かめていません。1 つの定数で全部を表せないので、3 つの表（この回と前の 2 回）の全 cell について、規則の選択が「速いほう」より何 ms 余計にかかったか（regret）の合計を、CPU の定数ごとに計算し、合計が最小になる範囲（0.025〜0.04 ns）の真ん中の 0.03 ns を選びました。中央値の 0.017〜0.019 ns を選ぶと、重み 1,280 の 10,000 lane（Vulkan のほうが約 2.5 倍速い）で CPU 参照を選び、余計に 0.52 ms かかります。

**規則の当たり外れ。** この表の 25 の cell すべてで、温まった `Gpu.Auto` が選んだ側は、速い側と一致しました（余計にかかった時間の合計は 0 ms。重み 80、1,000,000 lane は、Vulkan 1.397 ms と CPU 1.429 ms で 2 % の差）。前の回の表では、同じ重み 80、1,000,000 lane の cell で、Auto の選択のほうが 0.141 ms 余計にかかりました。**これは、定数を作ったのと同じ cell での評価で（in-sample）、別のカーネルや別のマシンでの当たりを示すものではありません。** 重み 80 の付近は、規則の境目（約 21 万 lane）より、測った境目（100 万〜400 万 lane。実行ごとに違います）のほうが 5 倍から 20 倍大きく、規則はこの付近で Vulkan を早く選びすぎる傾向があります。

**静かなマシンでの測り直し（2026-10-11）。** ほかの作業が止まってから、同じスクリプト（`--runs 3`）を、統合したブランチの release ビルドで測り直しました（同じ M1 Max と MoltenVK）。ロードアベレージ（1 分、10 コア）は測定の前が 4.4、後が 4.0（5 分と 15 分は 4.4〜5.6）で、上で探していた「3 または 5 以下」の静かさです。**定数は 1 つも変えていません。** 測り直した値は、規則の値と同じ範囲に入りました。

| 定数 | 規則の値 | 測り直した値 |
| --- | --- | --- |
| 呼び出しの固定費 | 270 µs | 286 µs |
| 1 byte あたり | 0.13 ns | 0.137 ns |
| デバイスの 1 命令あたり | 0.001 ns | 0.001 ns（切り上げ） |
| デバイスを開く | 30 ms | 23.6〜26.9 ms（5 つのカーネルの、3 つのプロセスの中央値） |
| パイプラインの作成 | 7 ms | 3.5 ms（最初の呼び出しは 3.7〜6.1 ms。実行は 0.33〜0.42 ms） |
| CPU 参照の 1 命令あたり | 0.03 ns | 中央値 0.017 ns、範囲 0.011〜0.066 ns（スクリプトが示す、後悔が最小になる範囲は 0.025〜0.04 ns） |

25 の cell のうち 24 で、温まった `Gpu.Auto` が選んだ側は、速い側と一致しました。外れたのは、重み 80、1,000,000 lane の 1 つです（CPU 参照 1.406 ms、Vulkan 1.663 ms で、Auto は Vulkan を選び、0.219 ms 余計にかかりました。上の表では、同じ cell が 2 % の差でした）。2 つの測定で、この cell は損益の境目にあり、速い側が入れ替わります（差は 2 % と 18 %）。25 の cell の合計の余計な時間は 0.219 ms でした。重み 1,280 の 4,000,000 lane は、CPU 参照 338.8 ms に対して Vulkan 10.1 ms です。生データは `target/perf/quiet/` に置きました（git の管理外）。この測り直しも 1 台のマシンの値で、他のマシンの保証ではありません。

**読み方と限界。**

- 重みが 20 以下のカーネルは、測った lane 数（4,000,000 まで）の全部で、CPU 参照のほうが速く、`Gpu.Auto` はデバイスを開きません。重み 320 は 10 万 lane から、重み 1,280 は 1 万 lane から、Vulkan のほうが速くなり、4,000,000 lane、重み 1,280 では、CPU 参照 350 ms に対して Vulkan 8.5 ms でした。この比は、この 1 つのカーネルの形、この GPU、この MoltenVK、この負荷でのものです。
- 測ったのは、整数のカーネル 1 種類（`i32`、入力と出力が 4 byte ずつ）の重みを変えたものだけです。別の演算の組み合わせ、別の lane の型（`i64` は 8 byte）、`init` だけの呼び出し、`f32` のカーネルは、測っていません。
- 重みは、1 lane が実行する命令の数の下限です（`if` は条件と安いほうの枝、`&&`・`||` は左の項だけ）。この表のカーネルは分岐がないので、重みは命令の総数のままで、下限の数え方に直しても表は変わりません。分岐のあるカーネルは、重みと測った時間を突き合わせていません。時間を測らずに確かめたのは、重い枝（1,280 個の演算）と安い枝（1 個の演算）を持つカーネルの重みが 2 になることと、その重みで 1,000,000 lane の `Gpu.Auto` の呼び出しが CPU 参照に回ることだけです（`tests/gpu_spirv.rs`、`tests/gpu_vulkan_language.mjs`）。重い枝を通る lane が多いカーネルは、実際の命令数が重みより大きいので、下限の見積りは `Gpu.Auto` が Vulkan へ出すのを遅らせる向きに外れます。
- Vulkan 側の生成コードも見ました。同じカーネル（重み 320）の `--emit spirv` の出力（9,972 byte）は、`spirv-val` を通り、`spirv-dis` では、1 ステップごとに `OpIMul`、`OpIAdd`、`OpBitwiseXor` が 1 つずつ、`Bits.ushr`（シフト量を `& 31` でマスクする std の小さな関数）が `OpFunctionCall` で出ています。エミッタは、この呼び出しを展開しません。重み 1,280 のカーネルで、呼び出しのまま（258 の `OpFunctionCall`）の版と、`>>>` で書いて呼び出しをなくした版（2）を、MoltenVK で 1,000,000 lane、15 回ずつ 3 組を比べると、1 回の中央値は 2.5〜3.2 ms と 2.4〜2.8 ms で、差は揺れの範囲でした（最初の呼び出し、つまりパイプラインの作成を含む時間は 12.8〜13.8 ms と 10.7〜11.9 ms）。この Mac では、呼び出しの有無は実行時間に効いていません。他のドライバは調べていません。
- 統合 GPU（メモリを共有する Apple GPU）の 1 台だけです。離散 GPU の PCIe の転送、NVIDIA・AMD・Intel のドライバ、Linux、Windows は、測っていません。そのため `Gpu.Auto` は、離散 GPU とソフトウェアの実装を選びません。
- 適合プローブの費用（厳密な `f32` の要求で 1 回、小さなパイプラインの作成と 1 回の dispatch）は、測っていません。float controls をすべて報告するデバイスが、この環境にないからです。
- 共有 CI に、速度の合否の閾値は入れません。`tests/gpu_vulkan.mjs` と `tests/gpu_vulkan_language.mjs` は、規則が決めたとおりに選ぶこと、能力・上限・デバイスの種類の確認を飛ばさないこと、失敗したときに CPU 参照へ戻ることを、モックと実機で確かめるだけで、速度は見ません。

## Async の中断と継続のコスト（B08）

2026-10-09、Apple M1 Max（10 論理 CPU）、macOS 27.0.1、Apple Clang 21.0.0、Rust 1.98.1、Node 24.21.0 で測定しました。
コンパイラは `ce7e8a1` に B08 の未コミット変更を加えた release 版、生成プログラムは native / generic / O3 です。
各プログラムを 1 回 warm-up してから `/usr/bin/time -l` で 9 回測りました。時間にはプロセスの開始と終了、継続の確保・再開・解放を含みます。
計時の表示粒度は 0.01 秒で、他言語や旧 Async 実装との速度比、SIMD・並列・GPU による加速を表す値ではありません。

| 入力 | wall time の中央値（最小–最大） | 最大 RSS |
| --- | --- | --- |
| 末尾の `return!` と yield、1,000,000 回 | 0.18 秒（0.17–0.23） | 1,753,088 bytes |
| 非末尾の `let!` と yield、1,000 段 | 0.03 秒（0.03–0.05） | 2,572,288 bytes |
| 同、2,000 段 | 0.11 秒（0.10–0.16） | 3,325,952 bytes |
| 同、4,000 段 | 0.40 秒（0.40–0.57） | 4,882,432 bytes |

入力は [Async fixture](../tests/fixtures/async/Main.tz) の `tail_loop_from` と `deep_from` です。
各関数と、`Async.run` の結果が入力の回数と一致したら終了コード 0 を返す main だけを、別々のプロジェクトに置きます。
末尾のプログラムの main は次のとおりです。非末尾では呼び出しを `deep_from 1000`（または 2000 / 4000）、期待値を同じ数にします。

```tsuzuri
def main :: unit -> i32 = \() ->
    let result = Async.run (tail_loop_from 0 1000000)
    if result == 1000000 then 0 else 1
```

```sh
tsuzuri build <project> --cpu generic -O3 -o <program>
<program>                         # warm-up、終了コード 0 を確認
for sample in 1 2 3 4 5 6 7 8 9; do /usr/bin/time -l <program>; done
```

入力・計測スクリプト・標本ごとの time の出力と machine code は ignored の `target/perf/B08-after/`、
コンパイラの SHA-256、環境、wall / user / sys time、RSS の全標本は PX01 形式の `process.jsonl` に保存しました。
生成 IR の代表的な i64 `bind_step` 特殊化には直接の `tz.alloc` が 4 箇所、環境 clone helper には 2 箇所あります。
consuming `Async.Next` は再開時に環境を move しますが、深い非末尾の継続は各段で包み直すため全体で O(n²) です。
末尾の `return!` や `For` の反復は、反復数に比例してスタックを深くしません。native / WASM の O0 / O3 で 100 万反復を確認しました。
非末尾の深さは別で、WASM はエンジンの stack exhaustion の限界を受けます。既定の stack や資源上限を増やして測定していません。

## 表の読み方

ビルド全体キャッシュ（whole-build cache）の合成ベンチマークは `node benchmarks/run-cache.mjs target/release/tsuzuri` で実行します。200 モジュールのプロジェクトを用いて、コールド状態（uncached）とウォーム状態（warm）における 7 回試行の中央値をミリ秒（ms）単位で計測し、出力バイナリのバイト単位の一致および実行チェックサムを照合します。
`--quick` オプションは 10 モジュール／2 回試行の動作検証のみを行います。2026-09-28 にこの実行経路を確認していますが、短縮された所要時間から一般的なビルド速度の優位性を安易に主張することはありません。
計測時間には、パース、型検査、IR 生成、コンパイラおよび依存ツールのフィンガープリント計算、ファイルコピー、および成果物の公開処理の時間がすべて含まれます。構文木（AST）単位のインクリメンタルパースキャッシュではないため、キャッシュヒット時であってもこれらの基盤処理時間は残存します。

ビルド速度の比較は [ビルド速度](#ビルド速度) の `run-build.mjs` を使います。
指標の記録は [性能指標の記録](#性能指標の記録) の JSON Lines で、中央値・最小・最大を保持し、
指標の表には最小値の強調と相対性能の列を付けません。quick は公開する性能の値ではありません。構文木（AST）単位のインクリメンタルパースキャッシュではないため、キャッシュヒット時であってもこれらの基盤処理時間は残存します。

明示的な SIMD 機能の同条件比較（matched 比較）は、`cargo build --release --locked && node benchmarks/run-simd.mjs target/release/tsuzuri` で実行します。
同一の `i64` 配列、折り返し加算、generic `-O3` 設定の下で、プロセス内反復によって Tsuzuri のスカラー実装、SIMD 実装、および C 言語実装の演算結果を照合し、5 回試行の中央値を ms 単位で表示します（配列の事前生成時間は計測区間外です）。
`--quick` は 4,097 要素・10 反復による正しさの確認のみを目的とし、得られた時間を性能の主張には使用しません。2026-09-27 にチェックサム `-2385` の完全一致を確認しました。
明示的な SIMD 命令の使用が、コンパイラによって自動ベクトル化されたスカラーコードより常に高速であるとは限らず、境界検査やループのプロローグ／エピローグのコストを含めて総合的に判断します。機能チケット F04 においては、232 件の参照ケースがネイティブ generic／native、ならびに WASM スカラー／SIMD の `-O0`／`-O3` で検証済みです。

CPU ディスパッチ（CPU dispatch）の比較は `node benchmarks/run-dispatch.mjs target/release/tsuzuri [--quick]` で行います。同一の `i64` 配列和について、Tsuzuri ディスパッチ版、スカラー版、および C 言語版を同一プロセス内で実行順序をローテーションさせながら測定し、9 回試行の中央値、検出機能フラグ、選択された variant、およびチェックサムを JSON 形式で出力します。
2026-09-27 に Apple M1 Max 環境で検証した際は、ベースライン実装（features=0、variant=0）が選択されることを確認しました。x86 向けの variant はクロスコンパイルおよび CPUID／XCR0 条件の単体テストでのみ確認されており、SSE4.2 や AVX2 の実機における実行速度は未測定です。
`--quick`（4,097 要素／10 反復）ではチェックサム `-2385` の一致を確認しています。ベースライン環境における測定結果から x86 実機での加速を推測して主張することはありません。各 ISA の性能測定は、対応する実機上で同一の入力および同一のコンパイラ最適化条件を揃えて実施します。

2026-10-06（F08 Phase 1。整数 8 型の和・最小・最大の kernel へ一般化した後）に、同じ Apple M1 Max・macOS 27.0.1 で、F08 の前の compiler（`target/perf/F08/baseline-tsuzuri`）と後の compiler を交互に 9 回ずつ実行しました。生データは `target/perf/F08/before/`・`after/` の JSON です。どちらも features=0・variant=0・checksum `-2577` で、arm64 は baseline の版どうしの比較です。

| 実装（1,048,577 要素 × 200 回、9 回の中央値） | F08 前 | F08 後 |
| --- | --- | --- |
| Tsuzuri dispatch | 21.542 ms | 21.557 ms |
| Tsuzuri scalar | 21.526 ms | 21.606 ms |
| C | 21.709 ms | 21.687 ms |

kernel を一つの accumulator で書いた途中の版は、同じ条件の dispatch が約 66 ms でした（ベクトル加算の待ち時間が律速）。4 つの accumulator で 4 ブロックずつ処理する形に改め、F08 前と同等に戻したことを確認しています。x86 の版は cross-compile して、AVX2 の版の `ymm`、AVX-512 の版の `zmm` の命令だけを確認しました。SSE4.2・AVX2・AVX-512・SVE の実機での速度は未測定です。

`Array.sum` などを使う実行ファイルは `cpu.c` の全 kernel を含みます。F08 のチケットの「例」（`Array.sum` の 4 つの呼び出し形、`-O3`）の実行ファイルは 67,432 bytes から 102,600 bytes になりました。`cpu.c` の object の text は arm64 で 605 bytes から 16,457 bytes、x86-64 で 1,920 bytes から 75,649 bytes です。未使用の kernel を除くリンカーの設定は PB01 の D10 の範囲です。

**処理時間は数値が小さいほど高速です（スコア形式ではありません）。** 必ず同一行・同一仕事量の数値を比較してください。
結果表は、ベンチマーク種目や仕事量などの識別列の直後に Tsuzuri の数値を配置し、その右側に比較対象の言語・実装を並べています。

| 指標 | 単位・計算方法 | 評価基準 |
| --- | --- | --- |
| 処理時間 | ms（ミリ秒: $1\text{ ms} = 0.001\text{ 秒}$）または µs（マイクロ秒: $1\text{ µs} = 0.001\text{ ms}$） | **小さいほど高速** |
| Tsuzuri 相対性能 | 最速の実装の処理時間 / Tsuzuri の処理時間（最速を 1.00000 に正規化） | **大きいほど良好**（1.00000 は最速と同等、0.50000 は最速の 50% の性能） |
| 時間比（Tsuzuri / 比較相手） | 単位なし。Tsuzuri の処理時間を比較相手の処理時間で割った値 | **小さいほど有利**（1 未満なら Tsuzuri が高速、1 なら同速、1 超なら低速） |
| 改善倍率（変更前 / 変更後） | 倍。変更前の処理時間を変更後の処理時間で割った値 | **大きいほど改善**（1 超なら高速化、1 未満なら性能低下） |
| メモリ確保回数 | 回。ヒープ領域のメモリ確保（malloc）を実行した回数 | **少ないほど確保負荷が小さい**（実行速度そのものとは異なる） |

例えば、処理時間が 0.5 ms である場合、1.0 ms に対して半分の所要時間となります。この関係は、時間比であれば 0.5、改善倍率であれば 2 倍と表記します。
ベンチマークサマリーに記載されている「相対性能」はこれらとは異なる指標であり、最速値が 1.0 ms で Tsuzuri が 2.0 ms であれば相対性能は `0.50000` となります。
仕事量の列は入力データのサイズを表しており、性能のスコアではありません。表内の `-` は未測定であることを示し、処理時間がゼロであることを意味するものではありません。

## ベンチマーク サマリー

<!-- benchmark-summary:start -->
<p><strong>処理時間は小さいほど高速です。</strong> 緑色・太字は各行の表示値で最速の実装です。同じ表示値は同率扱いです。僅差は一般的な優位性を示しません。</p>
<p><strong>Tsuzuri 相対性能は大きいほど良好です。</strong> 最速を <code>1.00000</code> とし、最速の処理時間を Tsuzuri の処理時間で割ります。<code>0.50000</code> は最速の半分の性能（処理時間は2倍）、<code>0.80000</code> は最速の80%の性能です。小数第5位に丸めるため、ごく僅かな差は <code>1.00000</code> になります。比較できない行は <code>-</code> です。</p>
<p>最終測定: <code>2026-10-13T16:17:59.295Z</code><br>CPU: Apple M1 Max / arm64 / generic<br>Tsuzuri: <code style="overflow-wrap:anywhere">659cf1ad87e8e3dace10ded0d5b82ece98765cd496a155b51f343d55ed5f77dc</code><br>C#: .NET 10.0.2 / JavaScript: v24.21.0</p>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">38種目 / generic / scale 0.1 / 1回の中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri (ms)</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C (ms)</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++ (ms)</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust (ms)</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C# (ms)</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">JavaScript (ms)</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">観測最速</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap" title="この行の最速時間 / Tsuzuriの処理時間。大きいほど高速です">Tsuzuri 相対性能<br><small>最速 = 1.00000</small></th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/while_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.304265</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.297156</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.301656</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.299912</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.522135</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.420292</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9945494205548718" title="最速時間 / Tsuzuri時間 = 0.9945494205548718"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99455</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/for_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.296941</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.292906</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.293412</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 2.066091</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.520627</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.481063</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9968888330309553" title="最速時間 / Tsuzuri時間 = 0.9968888330309553"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99689</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.295676</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.296471</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.301088</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.296219</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.523621</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.358250</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_if_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.293844</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.289031</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.294156</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.289625</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.532079</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.088938</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9962800770417454" title="最速時間 / Tsuzuri時間 = 0.9962800770417454"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99628</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_builtin_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.282156</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.286133</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.281469</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.288344</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.518446</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.141062</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.99946418376547" title="最速時間 / Tsuzuri時間 = 0.99946418376547"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99946</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/match_dispatch</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.254525</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.413710</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.412740</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.255019</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.498443</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">46.159875</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.168405</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.167898</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.168111</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.167399</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.585724</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.010989</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9940263056322556" title="最速時間 / Tsuzuri時間 = 0.9940263056322556"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99403</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_index_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.167339</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.168329</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.169123</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.167975</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.586143</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.757771</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_copy</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.132825</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.133114</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.136928</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.136883</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.405095</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.303771</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/list_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.177414</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.181234</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.181495</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.181871</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.238629</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.546143</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.190829</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.191157</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.190705</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.190288</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.196644</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 5.039344</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9971650011266632" title="最速時間 / Tsuzuri時間 = 0.9971650011266632"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99717</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_churn</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.639636</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.630047</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.635172</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.907217</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 2.342278</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.750188</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9850086611760439" title="最速時間 / Tsuzuri時間 = 0.9850086611760439"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98501</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/record_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.273294</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.270156</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.273750</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.275467</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.526908</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">36.805333</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9975355259665091" title="最速時間 / Tsuzuri時間 = 0.9975355259665091"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99754</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/integer128_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.513073</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.515268</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.510841</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.530436</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.705369</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">12.214396</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9956497418496003" title="最速時間 / Tsuzuri時間 = 0.9956497418496003"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99565</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float32_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.884917</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.885455</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.892370</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.887304</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.884898</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 2.417373</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9999785290597876" title="最速時間 / Tsuzuri時間 = 0.9999785290597876"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99998</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float64_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.889196</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.886348</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.891000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.889326</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.888113</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 3.821212</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9967971065996698" title="最速時間 / Tsuzuri時間 = 0.9967971065996698"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99680</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/integer_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 3.235531</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 3.209047</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 3.240446</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 3.173179</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">65.800626</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9807289746258034" title="最速時間 / Tsuzuri時間 = 0.9807289746258034"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98073</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/mandelbrot</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">243 x 243</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.317229</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.362167</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.388583</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15.518100</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.197739</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.343201</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.346706</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.344674</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.180267</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">27.660854</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_scan</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.006654</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.007205</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.007398</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.030164</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.068435</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_compare</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.017352</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.040798</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.019514</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.019363</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.008560</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">JavaScript</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.49331489165514064" title="最速時間 / Tsuzuri時間 = 0.49331489165514064"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.49331</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_validate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.053580</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.053743</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.110524</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.077807</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.351490</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf8_roundtrip</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.164200</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.089272</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.106033</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.110761</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.404166</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.5436784409257004" title="最速時間 / Tsuzuri時間 = 0.5436784409257004"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.54368</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/format_parse</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.373869</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.325875</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.182429</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.271777</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.706008</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.7269310908366299" title="最速時間 / Tsuzuri時間 = 0.7269310908366299"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.72693</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/math_intrinsics</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.509317</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.507890</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.514595</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.508265</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.509357</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.997198208581296" title="最速時間 / Tsuzuri時間 = 0.997198208581296"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99720</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequence</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.079550</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.080077</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.079247</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.632582</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.835684</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.996191074795726" title="最速時間 / Tsuzuri時間 = 0.996191074795726"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99619</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequential</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.273271</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.270806</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.272396</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.270294</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26.393937</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9976619274294317" title="最速時間 / Tsuzuri時間 = 0.9976619274294317"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99766</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_parallel</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.240655</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.292850</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.300255</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.226232</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">41.392771</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9400677318152542" title="最速時間 / Tsuzuri時間 = 0.9400677318152542"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.94007</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.323081</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.324612</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.388171</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.317519</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 9.425278</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9827845029574627" title="最速時間 / Tsuzuri時間 = 0.9827845029574627"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98278</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/checked</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.523800</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.516428</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.528233</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.816778</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.402646</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9859259259259259" title="最速時間 / Tsuzuri時間 = 0.9859259259259259"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98593</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/delayed</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.130580</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.130538</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.148901</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.182638</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 7.591097</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9996783580946546" title="最速時間 / Tsuzuri時間 = 0.9996783580946546"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99968</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_for</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000621</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000609</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000606</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.001205</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.077196</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9758454106280192" title="最速時間 / Tsuzuri時間 = 0.9758454106280192"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.97585</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000607</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000613</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000589</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.001210</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.076176</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9703459637561779" title="最速時間 / Tsuzuri時間 = 0.9703459637561779"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.97035</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/owned_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.002648</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.002612</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.002704</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.003083</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.063521</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.986404833836858" title="最速時間 / Tsuzuri時間 = 0.986404833836858"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98640</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.515467</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.446133</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.445270</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.548111</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.879719</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.8638186343645665" title="最速時間 / Tsuzuri時間 = 0.8638186343645665"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.86382</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_result</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.515581</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.506539</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.522129</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.753402</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15.560500</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9824625034669625" title="最速時間 / Tsuzuri時間 = 0.9824625034669625"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98246</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_owned</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.017647</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.033147</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.001838</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.017951</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.059832</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.10415368051226838" title="最速時間 / Tsuzuri時間 = 0.10415368051226838"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.10415</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_length</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.391212</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.384712</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.459892</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.574329</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.124146</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9833849677412758" title="最速時間 / Tsuzuri時間 = 0.9833849677412758"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98338</code></td>
</tr>
</tbody></table>
</div>
<p><code>-</code> は未測定です。すべて wall time（ms/呼び出し）で、起動時間は含みません。C#/JavaScript は別プロセス、管理メモリの回収条件は言語ごとに異なります。詳細な比較条件は後述します。</p>
<!-- benchmark-summary:end -->

## C/C++・Rust・C#・JavaScript の横断比較

```sh
cargo build --release --locked
# Node.js 24 以降、.NET SDK 10、Rust、C++20 と WASM 対応の Clang/LLD が必要
node benchmarks/run-managed.mjs target/release/tsuzuri --quick
node benchmarks/run-managed.mjs target/release/tsuzuri --scale 0.1 \
  --artifacts target/benchmarks/managed
node benchmarks/run-managed.mjs target/release/tsuzuri --scale 0.1 --cpu native

# PATH の Node が古い場合。既存 Node やプロジェクト依存は置き換えない
npx --yes --package=node@24 node benchmarks/run-managed.mjs target/release/tsuzuri --quick
```

通常の `run-managed.mjs` の実行が成功すると、冒頭の「ベンチマーク サマリー」がその測定結果によって自動更新されます。
`--quick` オプション実行時はサマリーの更新を行いません。ドキュメントを更新せずに計測のみを行う場合は `--no-report` を指定します。
サマリーに記録されている日時、CPU 指定、仕事量、およびコンパイラ識別子は、その測定が行われた時点の固有条件です。条件が異なる実測結果を混在させることはありません。
保存済みの同一条件の結果をまとめて再表示・集計する場合は、以下のスクリプトを使用します（複数ファイル指定時は各回の中央値の中央値を算出します）。なお、古い形式の JSON に測定日時が記録されていない場合に限り、結果ファイルの更新日時を表示します。

```sh
node benchmarks/report.mjs result-1.json result-2.json result-3.json
node tests/benchmark_report.mjs
```

サマリー表は HTML テーブル形式で記述されています。画面幅に収まらない場合は横スクロールが可能であり、数値データは等幅フォントかつ右揃えで整列表示されます。
表示環境の設定で HTML の背景色や文字色が反映されない場合でも、太字装飾および「観測最速」の列情報によって最速実装を一目で判別できるよう配慮しています。

現在、control 系列 15 種目、cpp 系列 12 種目、computations 系列 9 種目の計 36 種目について、同一の入力データおよびチェックサム検証を用いて比較を行っています。
C 言語は control 系列の 15 種目に対応し、C++、Rust、C#、JavaScript は全 36 種目に対応しています。
**これは実装済みの主要な実行時機能カテゴリを対応付けた技術的検証であり、あらゆる API、型、入力パターンにおいて性能を一般的に証明するものではありません。**
未測定の種目について「同速である」「高速である」あるいは「すでに対応済みである」と安易に解釈しないようご注意ください。

| 実装済み機能 | 対応する測定／残る範囲 |
| --- | --- |
| 整数・ビット演算・末尾再帰・while/for・if/match | control の各 mix、match_dispatch、integer128_mix。狭幅整数の全演算に対する個別速度は未測定 |
| f32/f64・型変換・sqrt/floor/ceil/abs | float32_mix、float64_mix、mandelbrot、math_intrinsics |
| 配列・リスト・所有権・コピー・借用・ローカル更新 | array_sum、array_copy、list_sum。確保・初期化・走査・解放の全行程を含む |
| 関数値・カリー化・捕捉・高階関数・ジェネリックなレコード | closure_capture、closure_churn、record_pipeline、computations。構文差のみの機能は共通 lowering の処理へ対応付ける |
| union・Maybe/Result・型クラス・ユーザービルダー | checked、std_option、std_result、9 種類の computations |
| UTF-16/UTF-8・連結・複製・走査・比較・検査・修復・変換 | utf16_scan、utf16_compare、utf16_validate、utf8_roundtrip |
| Display/Parse | format_parse（u64 の十進表示と解析）。全数値型の表示・解析速度は未測定 |
| cold task・bind・Task.run・Task.parallel | task_sequence、task_sequential、task_parallel |
| f16/f128・decimal32/64/128 の正確な演算 | 正確性の参照検査は実施済み。5 言語で同一の標準型・丸め・表示契約が揃わないため横断速度比較は未実施（C# decimal は Tsuzuri d128 の代替ではない） |
| モジュール・private・型推論・借用検査・網羅性・診断 | 主にコンパイル時の機能であり実行時ディスパッチではない。コンパイル時間は今回未測定 |
| C ABI・WASM・コンソール | 外部 ABI 経由の測定と native/WASM O0/O3 の検証。I/O・GUI 全体の速度比較ではない |
| GPU・常駐 worker pool・自動並列化 | 過去の計測時点では未実装。常駐 pool の後続測定は下の専用節に分離 |

### 条件と読み方

- 統合 JSON の比率計算には、すべての言語・実装において単調時計による **wall time（実経過時間）** のみを採用しています。ネイティブ環境の従来の CPU 時間は参考値として別フィールドに保持します。`Task.parallel` の CPU 時間は全ワーカースレッドの CPU 時間を合算してしまうため、実効的な高速化率の指標としては使用できません。
- 各種目において 25 組の境界入力値を用意し、計測中も乱数シードごとのチェックサムを厳密に照合します。独立した JavaScript／BigInt 参照実装との等価性検証も常時実施します。反復回数は、極端に短い呼び出しを約 20 ms のバッチにまとめて計測できるよう自動調整されます（上限 10,000 回）。プロセスの起動時間やコンパイル時間は計測区間に含みません。
- `--scale` オプションは各ベンチマーク系列の既定仕事量に対する乗数です。統合ランナーの既定値は `0.01`、許容範囲は $0 < \text{scale} \le 5$ です。Mandelbrot 種目では画素数に対応させるため、一辺の解像度にスケール値の平方根を乗じます。`--quick` は正しさの確認のみを目的とし、速度の評価には使用しません。
- ネイティブ実装群は同一プロセス内で実行順序を均等にローテーションさせて測定します。一方、C# と JavaScript は別プロセスとして順番に起動して測定します。このため、OS のスケジューリング、CPU 温度、およびシステム負荷による揺らぎが残存します。僅差の数値を過剰に勝敗として解釈しないよう配慮が必要です。
- C# は .NET 10 の Release JIT（`DOTNET_TieredCompilation=0`、ウォームアップ完了後）で計測しています。NativeAOT による測定ではありません。制御系列の配列・リスト操作では `NativeMemory`、`Span`、およびポインタを用いて明示的なメモリ確保・解放に揃えています。`allocated_bytes` はマネージドヒープの確保量のみを反映し、NativeMemory のバイト数は含みません。スカラー種目への不要なクロージャ環境の混入は回避しています。
- JavaScript の 64/128-bit 整数演算は `BigInt` と明示的なビットマスク（折り返し）を用い、f32 は `Math.fround`、UTF-16 はコード単位として厳密に扱います。安易に `Number` へ置き換えて精度を犠牲にするような測定は行いません。配列は `BigUint64Array`、リストは単方向リンクのオブジェクトで表現しています。`string.slice` は物理的なメモリコピーを保証せず、各 JavaScript エンジン固有のロープ（rope）構造、文字列共有、および短小文字列最適化（SSO）の特性がそのまま現れます。
- C# および JavaScript の計測中に発生した GC（ガベージコレクション）の時間は測定結果に含まれ、発生回数等も記録されます（計測完了後の強制 GC は別枠で記録）。したがって、マネージド言語の測定結果は「全メモリ領域が明示的に解放されるまで」のライフサイクルを完全に再現したものではありません。
- 並列処理種目では、すべての実装で同一の 16 個の独立タスクを実行します。C++ は有界の `std::thread`、Rust は `scoped threads`、Tsuzuri は有界の `pthread fork/join`、C# は有界な `Parallel.For` の常駐プール、JavaScript は有界の `worker_threads` を使用します。スレッドやワーカーの新規起動、全ジョイン・終了待機、および結果の回収がすべて計測時間に含まれます。C# の常駐プールと他言語の都度起動スレッドを同一のスケジューラ特性として混同してはなりません。また JavaScript のメインスレッド側の GC 記録にはワーカー内部の GC は含まれませんが、タスク完了までの実経過時間には適切に反映されます。
- 2026-10-13 の PX02 以後、`std_option_owned` は全実装がソース上で 2 回の所有文字列の連結、長さの読み出し、解放（管理言語は GC）を行います。最適化で確保が消えた場合も、その実装の結果として記録します。以前の定数長の参照は `std_option_length` に分けました。2026-09-26 の旧い owned の値は条件の異なる履歴であり、新しい owned と直接比較しません。
- `--cpu native` オプションは C/C++、Rust、および Tsuzuri に適用されます。C# および V8（JavaScript）は実行マシン向け JIT コンパイラとして動作し、generic な命令セットを強制することはありません。また Clang と rustc 同梱の LLVM のバージョン差も存在します。実際のビルド条件は出力 JSON の `environment` および `native_options` に記録されます。
- `--baseline` オプションは control および computations 系列の同一プロセス比較に適用されます。cpp 系列では保存済みのコンパイラバイナリを個別に指定して比較します。`--artifacts` はネイティブのコンパイル成果物、入力プラン、元のネイティブ JSON、および統合 JSON を指定ディレクトリへ保存します。

環境変数 `TSUZURI_CLANG`、`TSUZURI_RUSTC`、`TSUZURI_DOTNET` によって使用ツールを明示指定可能です。
なお、Node.js 20.19.6 の V8 において、反復した BigInt 参照計算時に `RepresentationChangerError` による内部クラッシュが発生することを確認したため、`run-cpp` および `run-managed` の実行には Node.js 24 以上を必須要件としています（JIT を無効化して不当に遅くした状態での比較は行いません）。

### C# 同等項目の追加改善（2026-09-26）

C# とほぼ同等水準にとどまっていたベンチマーク項目を再分析し、動的関数値（クロージャ）の共通処理パスを最適化しました。
単一の小さな不変スカラー変数を捕捉する場合、関数記述子の環境ポインタ欄へ値を直接インプレース格納（`immediate_capture`）することで、環境メモリの動的確保、クローン、および読み出しの間接参照を完全に排除しました。
さらに、通常の内部アダプタ関数の引数順序を「値引数先頭」に変更し、戻り値レジスタを次回の呼び出しの値引数レジスタとしてそのまま再利用できるようにして、レジスタ間の不要な値移動命令を削減しました。
特定の関数名、特定の入力値、あるいは反復回数に依存した恣意的なハードコーディングは一切行っていません。

同一の Apple M1 Max、Clang 23.1.1、Rust 1.98.1、.NET 10.0.2、Node.js 24.21.0、`--scale 0.1`、generic／native 各 3 回試行の測定結果です。
新旧バージョンの Tsuzuri と C/C++/Rust は同一プロセス内で交互に測定し、C# および JavaScript はウォームアップ完了後の別プロセスとして順次測定しました。
以下の表は各試行の中央値の中央値を示します。**処理時間（ms）は数値が小さいほど高速です。**
関数呼び出しは 100,000 回、クロージャの生成・複製は 100,000 反復（1 反復につき同一関数値を 2 回呼び出し）で計測しています。

| CPU | 種目 | Tsuzuri 今回 (ms) | Tsuzuri 前回 (ms) | C# (ms) | C++ (ms) | Rust (ms) | JavaScript (ms) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| generic | 動的呼び出し（closure_capture） | 0.188876 | 0.208533 | 0.193285 | 0.188593 | 0.188757 | 5.292073 |
| native | 動的呼び出し（closure_capture） | 0.188726 | 0.213011 | 0.193991 | 0.188880 | 0.189519 | 5.226052 |
| generic | 作成・コピー（closure_churn） | 0.633267 | 4.574000 | 2.314744 | 0.627333 | 0.901409 | 11.729500 |
| native | 作成・コピー（closure_churn） | 0.633667 | 4.569800 | 2.353489 | 0.627212 | 0.896304 | 11.757896 |

動的呼び出しは約 9〜11% の所要時間短縮を達成し、作成・コピー処理は前回比で約 7.2 倍の高速化となりました。
C# との呼び出し単体の差は約 2〜3% にとどまるため、これを決定的な言語優位性として扱うことはしません。
新設された作成・コピー種目においては C# 比で約 3.6〜3.7 倍の性能を示しました。
ただし、これは単一の不変スカラー変数を捕捉するワークロードでの結果であり、所有権を持つデータ構造を内包する一般的なクロージャ環境に対して同一の倍率がそのまま適用できるわけではありません。
C# や JavaScript のクロージャコピーは参照のコピーであっても意味論が成立するのに対し、Tsuzuri は独立した値のスナップショットを保証しています（C++ や Rust においても環境をヒープに置くことは強制していません）。

最適化後の両種目の実行ホットパスには `malloc`／`free` や環境の clone／drop 呼び出しが一切存在せず、ARM64 の動的ディスパッチループ内から 2 つの値移動命令が完全に消去されていることをアセンブリレベルで確認しました。
C# の作成・コピー種目では、今回の JIT 条件において 1 反復あたり 88 バイト、100,000 反復で計 8.8 MB のマネージドヒープ確保が発生していました。
計測中の GC 時間は測定値に含まれていますが、マネージドヒープの回収タイミングが Tsuzuri の即時解放と完全に同一のライフサイクルであることを意味するものではありません。
1,000 回および 1,000,000 回の反復検証においても結果の整合性を確認しており、作成・コピー処理は前回比でそれぞれ約 9.7 倍、約 7.2 倍の性能向上を示しました。

その他の C# 同等水準項目についても再測定を実施しました。以下は generic 設定における測定結果です。**処理時間（ms）は数値が小さいほど高速です。**
これらの算術ループのアルゴリズムや浮動小数点の意味論には一切手を加えていません。

| 種目 | 仕事量 | Tsuzuri (ms) | C# (ms) | C++ (ms) | 判断 |
| --- | ---: | ---: | ---: | ---: | --- |
| f32演算（float32_mix） | 400000 | 0.881955 | 0.878929 | 0.880739 | ほぼ同速（変更なし） |
| f64演算（float64_mix） | 400000 | 0.879739 | 0.882617 | 0.880087 | ほぼ同速（変更なし） |
| 数学組み込み（math_intrinsics） | 50000 | 0.502622 | 0.501804 | 0.502320 | LLVM 直接命令を維持 |
| 単純なbind（computations/bind） | 200000 | 0.314065 | 0.318949 | 0.314175 | ほぼ同速（変更なし） |
| 独立計算の逐次実行（task_sequential） | 50000 x 16 | 1.256542 | 1.262647 | 1.255817 | 自動並列化なし |

`sqrt`、`floor`、`ceil`、`abs` は既に LLVM の直接命令として出力されています。依存関係のある演算の安易な再結合、暗黙的な FMA 融合、逐次処理の勝手な並列化は行っていません。すべての C# 同等項目で一方的な大差がついたわけではない点に留意してください。

環境欄への直接即値格納は、Task 以外のクロージャで、整数・bool・f32／f64 のスカラー変数が 1 個のみであり、かつターゲットのポインタ幅以下に収まる場合にのみ適用されます。
環境フィールドはビット列として透過的に扱われ、参照外しや `free` は行われません。所有値、参照型、複数変数の捕捉、広幅値は従来のヒープ環境を使用し、wasm32 における `i64`／`f64` も従来経路で処理されます。WebAssembly 全般に対して同等の高速化倍率を主張することはありません。
関数記述子の 4 フィールド構成、公開 ABI、Task の呼び出し規約、スナップショット意味論、評価順序、および丸め規則は完全に維持されます。
ネイティブおよび WASM の `-O0`／`-O3` において、ゼロ値、極値、NaN、符号付きゼロ、コピー動作、1 捕捉から複数捕捉への部分適用遷移、所有環境へのフォールバック、ならびに引数評価中に関数値が差し替えられるケースを網羅的に検証しました。
Rust の 267 件の単体テスト、Clippy、プリミティブ、Task、ビルダー、制御構文、ABI、および実践的サンプルの実行検証をすべてクリアしています。

基準コミットは `1365c05`。コンパイラ SHA-256 は前回
`d20a3f91440bf169965a6c11105359ad70c7a0322113e9c3f5d524a971a0593b`、今回
`429015c246e6aa57f3e74f156750a671dfde173a4e002702080b9ef850db6bc9` です。
生データは `target/benchmarks/csharp-parity-20260926/final-{generic,native}-{1,2,3}.json` に保存されており、各条件の初回試行と同名のディレクトリに入力データ、生成コード、および元の native／WASM 測定結果を保管しています。
データサイズ別の結果は `size-0.001.json` および `size-1.json` です。再現実行の例は以下のとおりです。

```sh
node benchmarks/run-managed.mjs target/release/tsuzuri --scale 0.1 \
  --baseline target/benchmarks/csharp-parity-20260926/tsuzuri-before \
  --artifacts target/benchmarks/csharp-parity-check
```

### 数値・UTFの追加改善（2026-09-26）

初回の最適化完了後を基準として、数値ランタイムおよび UTF 変換処理に対してさらなる改善を施しました（下掲の初回 35 種目の測定表は開発経緯の履歴として保持します）。
同一の Apple M1 Max、Homebrew Clang 23.1.1、`-O3`、Node.js 24.21.0、`--scale 0.1` の設定下で、変更前後のコンパイラを交互に各 3 回実行しました。
整数表示・解析は 5,000 反復、UTF 往復変換は要求文字長 26,215（内部的には 32,768 UTF-16 コード単位へ展開）で計測しています。
数値は各試行の中央値の中央値です。**処理時間（ms）は小さいほど高速であり、改善倍率（前回 / 今回）は大きいほど性能が向上しています。**
新旧バイナリは別プロセスとして実行しているため、システム負荷やスケジューリングの揺らぎが残存する点は初回と同様です。

| CPU | 種目 | Tsuzuri 今回 (ms) | Tsuzuri 前回 (ms) | 改善倍率（倍） |
| --- | --- | ---: | ---: | ---: |
| generic | 整数表示・解析 | 0.363587 | 0.542210 | 1.491 |
| native | 整数表示・解析 | 0.364516 | 0.542440 | 1.488 |
| generic | UTF往復変換 | 0.136483 | 0.143423 | 1.051 |
| native | UTF往復変換 | 0.137718 | 0.142482 | 1.035 |

整数の表示および解析処理において約 33% の所要時間短縮を達成しました。50 反復および 50,000 反復のテストケースにおいても同様の性能向上を確認しています。
50 反復の初回試行では他言語の実装も含めて全体的に大きな数値の揺らぎが観測され、Tsuzuri でも一時的に遅い値が記録されましたが、追加の 2 回の試行では前回約 0.0054 ms に対し今回約 0.0036 ms と安定した改善を示しました（この初回の揺らぎデータも除外することなく保持しています）。
UTF 変換の短縮効果は数%にとどまるため、この結果のみをもって他環境でも一律に高速であると断定することはしません。ASCII のみおよび混在 Unicode の各入力パターンについて個別に挙動を確認しています。

同一条件下での C++、Rust、C#、JavaScript との 35 種目の横断比較を再実行し、すべてのチェックサムの一致を確認しました。
以下の表は、そのうちの 1 回の実行における中央値を示したものです（上記の 3 回集計とは独立した測定回です）。**処理時間（ms）は数値が小さいほど高速です。**

| 種目 | Tsuzuri (ms) | C++ (ms) | Rust (ms) | C# (ms) | JavaScript (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 整数表示・解析 | 0.363912 | 0.288721 | 1.165644 | 0.270891 | 0.708634 |
| UTF往復変換 | 0.135814 | 0.090114 | 0.105325 | 0.110751 | 0.401954 |

整数表示・解析における C++ との時間比は約 1.9 から約 1.26 へ、C# との時間比は約 2.1 から約 1.34 へと大幅に改善されましたが、依然として先行言語との間には差が残在します。
UTF 往復変換についても、C++、Rust、C# の水準を上回るには至っていません。GPU や自動並列化、スレッドプールへの依存によらない堅実な改善結果です。

今回採用した主な改善点:

- 全ビット幅の整数表示において 2 桁ずつまとめて処理を行い、コストの高い 64-bit および 128-bit の除算回数を削減。binary 浮動小数点の正負ゼロは分岐で直接出力。
- 十進整数の文字列解析において、先頭に数字が続く場合は 8 桁ずつ一括して検証・集約し、基数 1 億の範囲検査を実施。桁区切り文字や他基数の出現時には従来の 1 桁処理へ安全に復帰。
- 数値の読み書き処理を、各型のビット幅に合致した非整列メモリアクセス対応の固定サイズコピーへと最適化。不正な領域外アクセスやポインタのエイリアス仮定は排除。
- 浮動小数点演算用の大きな作業バッファを整数の表示・解析ルーチンから完全に分離。最終的な ARM64 バイナリのエントリースタックサイズは、表示ルーチンで約 39 KiB から 112 バイトへ、解析ルーチンで約 11 KiB から 208 バイトへと劇的に縮小。
- UTF 変換の文字計数フェーズで完了した検証結果を、同一の不変入力に対する書き込みフェーズで安全に再利用。不正な UTF-8 シーケンスや孤立サロゲートの厳格な拒否、および正確なバッファ確保サイズ計算は完全に維持。

`tests/display_parse_runtime.c` に、I/O やプロセス起動のオーバーヘッドを計測区間から除外した純粋な反復測定ハーネスを追加しました。
`F kind repeats low_hex high_hex` は数値表示、`P kind repeats ascii_hex` は数値解析に対応し、CPU 時間（ms/回）とチェックサムを出力します。
入力データの準備および結果の回収は新旧両バージョンで同一の計測区間に含めています。テストにおける合否判定の速度閾値は設けていません。
型ごとに各 3 サンプルを用意し、短小値および整数型は 1,000,000 回、通常の浮動小数点数は 100,000 回ずつ反復測定を実施しました。
通常の浮動小数点数はほぼ同等の処理時間であり、64-bit および 128-bit 整数の最大値の解析では約 2.3〜2.8 倍の高速化が確認されました。
一方で、8-bit 整数のごく短い文字列の解析など一部のケースにおいて、1〜数ナノ秒（$1\text{ ns} = 0.000001\text{ ms}$）程度の僅かな増加が残存しています。
このような微小な差異もすべて記録・開示し、あらゆる入力パターンで一律に高速化されたと誇大に主張することはありません。

ネイティブおよび WASM の `-O0`／`-O3` において、表示処理 88,063 件、解析および往復変換処理 111,282 件、型変換の境界ケース 1,585 件、decimal および binary128 の参照演算、全 Unicode スカラー値の往復、不正入力時のトラップ、ならびに所有メモリの完全回収を厳密に検証しました。
追加した解析テストケースには、8 桁の各位置における全バイトパターン、すべての整数幅・符号、区切り文字、先頭ゼロ、桁数・範囲・文字列長の上限が含まれます。
数値の入出力バッファについては、1 バイトずらした非整列領域および前後に番兵バイトを配置したメモリ上でも検査を行っています。

基準コミットは `74e9667`。コンパイラ SHA-256 は前回
`75500729e5a8f57ee770960a469e79515e8c52c91e5a1bf724cc53eb799c0afb`、今回
`d20a3f91440bf169965a6c11105359ad70c7a0322113e9c3f5d524a971a0593b` です。
生データは `target/benchmarks/followup-20260926/published-{generic,native}-{before,after}-{1,2,3}.json`、言語間比較データは `published-managed.json`、型別の詳細測定は `numeric-isolated-final-code.json` に保管されています。
型別 JSON には再実行用のプロトコル仕様と反復数が記録されており、データサイズ別の小／大サイズ結果は `size-*` の各 JSON に保存されています。
`before` は保存済みの前回コンパイラ、`after` は改善後の今回コンパイラです。通常の `run-cpp.mjs --scale` および `--cpu` コマンドで再測定が可能です。

### 初回の実測（2026-09-26）

測定環境: Apple M1 Max、arm64 アーキテクチャ、10 logical CPUs、Darwin 27.0.0、Homebrew Clang 23.1.1、
Rust 1.98.1／LLVM 22.1.8、.NET SDK 10.0.102／runtime 10.0.2、Node.js 24.21.0／V8 13.6.233.17。
すべて `-O3` 相当の最適化、LTO なし、fast-math なし、暗黙の FMA 融合なし。generic／native 各 3 回試行、`--scale 0.1`。
以下の表は generic 設定における各試行の中央値の中央値を示します。**処理時間（ms、ミリ秒）は数値が小さいほど高速です。**
表内の `-` は、そのベンチマーク系列において C 言語版の測定が対象外であることを意味します。
文字列の size=26,215 は要求文字長であり、実際の内容は 8 単位から倍増を繰り返した 32,768 UTF-16 コード単位です。

| 種目 | 仕事量 | Tsuzuri (ms) | C (ms) | C++ (ms) | Rust (ms) | C# (ms) | JavaScript (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| control/while_mix | 800000 | 1.268500 | 1.270818 | 1.268333 | 1.267643 | 1.517843 | 37.787500 |
| control/for_mix | 800000 | 1.264250 | 1.267688 | 1.265867 | 2.015700 | 1.531107 | 37.882125 |
| control/tail_mix | 800000 | 1.286750 | 1.282063 | 1.281438 | 1.281313 | 1.520264 | 37.685709 |
| control/tail_if_mix | 800000 | 1.273750 | 1.271937 | 1.266750 | 1.268500 | 1.511121 | 37.558958 |
| control/tail_builtin_mix | 800000 | 1.264875 | 1.268188 | 1.266353 | 1.262375 | 1.514057 | 37.602084 |
| control/match_dispatch | 800000 | 0.253889 | 0.257090 | 0.257625 | 0.255275 | 1.486157 | 46.403375 |
| control/array_sum | 400000 | 0.168540 | 0.168661 | 0.167813 | 0.167008 | 0.586429 | 14.135583 |
| control/array_copy | 200000 | 0.135572 | 0.135782 | 0.139727 | 0.140141 | 0.406133 | 11.331771 |
| control/list_sum | 10000 | 0.178797 | 0.182450 | 0.182857 | 0.181758 | 0.239499 | 0.539527 |
| control/closure_capture | 100000 | 0.211652 | 0.189629 | 0.191187 | 0.190792 | 0.199682 | 5.262802 |
| control/record_pipeline | 800000 | 1.273824 | 1.274625 | 1.270471 | 1.266750 | 1.511693 | 37.141708 |
| control/integer128_mix | 200000 | 0.572162 | 0.508878 | 0.508659 | 0.528105 | 0.704955 | 12.225021 |
| control/float32_mix | 400000 | 0.888609 | 0.888864 | 0.892042 | 0.885125 | 0.886646 | 2.402734 |
| control/float64_mix | 400000 | 0.900217 | 0.896130 | 0.893364 | 0.894130 | 0.885208 | 3.836014 |
| cpp/integer_mix | 2000000 | 3.177881 | - | 3.187613 | 3.158774 | 3.174850 | 66.482521 |
| cpp/mandelbrot | 243 x 243 | 13.206615 | - | 13.190511 | 13.085916 | 15.457025 | 14.117323 |
| cpp/array_sum | 800000 | 0.342064 | - | 0.341543 | 0.338788 | 1.171558 | 28.066854 |
| cpp/utf16_scan | 26215 | 0.006683 | - | 0.007364 | 0.007786 | 0.029742 | 0.066700 |
| cpp/utf16_compare | 26215 | 0.017425 | - | 0.040710 | 0.020335 | 0.018905 | 0.008617 |
| cpp/utf16_validate | 26215 | 0.053977 | - | 0.056690 | 0.113791 | 0.077922 | 0.355879 |
| cpp/utf8_roundtrip | 26215 | 0.143708 | - | 0.091412 | 0.106490 | 0.110395 | 0.402196 |
| cpp/format_parse | 5000 | 0.555905 | - | 0.294335 | 1.176169 | 0.270814 | 0.719167 |
| cpp/math_intrinsics | 50000 | 0.512134 | - | 0.509656 | 0.510627 | 0.504729 | 0.506497 |
| cpp/task_sequence | 50000 | 0.079066 | - | 0.079418 | 0.079117 | 0.627942 | 1.839759 |
| cpp/task_sequential | 50000 x 16 | 1.266445 | - | 1.264690 | 1.266917 | 1.268894 | 26.717250 |
| cpp/task_parallel | 50000 x 16 | 0.284257 | - | 0.288814 | 0.296827 | 0.231113 | 45.464000 |
| computations/bind | 200000 | 0.318094 | - | 0.321083 | 0.382412 | 0.317646 | 9.556042 |
| computations/checked | 200000 | 0.518699 | - | 0.511358 | 0.521709 | 0.826508 | 14.474958 |
| computations/delayed | 50000 | 0.128525 | - | 0.127544 | 0.147394 | 0.181978 | 7.587097 |
| computations/array_for | 820 | 0.000616 | - | 0.000607 | 0.000594 | 0.001197 | 0.077031 |
| computations/array_bind | 820 | 0.000603 | - | 0.000601 | 0.000589 | 0.001202 | 0.077140 |
| computations/owned_capture | 820 | 0.002636 | - | 0.002620 | 0.002691 | 0.003070 | 0.062772 |
| computations/std_option | 200000 | 0.509248 | - | 0.445295 | 0.446226 | 0.532832 | 14.032084 |
| computations/std_result | 200000 | 0.517950 | - | 0.508844 | 0.525206 | 0.754050 | 15.628125 |
| computations/std_option_owned | 820 | 0.017724 | - | 0.001539 | 0.001822 | 0.002109 | 0.058372 |

native 設定における Tsuzuri の中央値は、`closure_capture` が 0.211688 ms、`utf16_compare` が 0.017193 ms、
`utf8_roundtrip` が 0.142395 ms、`format_parse` が 0.543789 ms、`task_parallel` が 0.285517 ms でした。
CPU を明示指定したことによる極端な性能変化は見られませんでした。なお、native 第 1 回のタスク計測では一時的な約 2 ms の遅延、generic 第 3 回の文字列比較では同時に計測した C++／Rust／Tsuzuri の全言語で共通の遅延が観測されましたが、これらを生データから恣意的に除外することはしていません。
別プロセスとして動作する C# や JavaScript との僅差の比較においては、このような OS レベルの負荷変動も考慮する必要があります。

### 初回の改善と残る差

1. 動的クロージャのアダプタ関数に、環境借用の内部フラグ（`i1 borrow`）を追加しました。読み取りのみを行う捕捉変数は複製せず、消費的な捕捉時や特殊化予算の上限到達時にのみアダプタ内部で必要な複製を実行します。後続の引数が状態を変更し得る場合は事前のスナップショット保持を継続します。配列やリストの動的初期化にも同様のロジックを適用し、関数記述子の 4 ポインタ表現、公開 ABI、および Task ABI は一切変更していません。同一プロセス内での新旧比較では、generic 設定で 9.51〜10.70 倍、native 設定で 9.88〜10.77 倍の高速化を確認しました。
2. すべての数値型の文字列表現において、ASCII 確定の文字列を直接 UTF-16 バッファへ拡張する高速パスを導入しました。整数の十進変換は値が 64-bit に収まる範囲で 64-bit 除算へと早期に移行させ、狭幅整数の解析における cutoff 計算も 64-bit で行います。128-bit 整数、decimal 型、NaN、符号付きゼロ、および厳密な最近接偶数丸めの意味論は完全に維持されます。
3. UTF-16 および UTF-8 の一致比較において、バッファ境界内の 64-bit 一括読み出しを採用しました。辞書順比較は最初の不一致位置における 16-bit 符号なしコード単位の値で決定します。UTF 変換処理では、検証済みの入出力文字長が等しい場合にのみ ASCII 直接書き込みパスへ進み、非 ASCII 文字を含む場合は従来の復号ループへ安全に分岐します（各位置で ASCII を先読みする試作コードは、混在 Unicode テキストで性能低下を招いたため破棄されました）。
4. `Task.parallel` は、未割り当てのタスクが存在しなくなった時点で不要な追加ワーカースレッドの起動を即座に停止します。固定サイズやベンチマーク名による恣意的な閾値判定は行わず、既に起動したワーカーは確実にジョインして終了します。最大並列度、ネスト実行、同時呼び出し、およびタスクの早期枯渇の各条件下での決定論的動作をテストで確認しました。

同一の計測マシン、`--scale 0.1`、別プロセスによる新旧比較では、UTF-16 比較が 0.029318 ms → 0.017442 ms、UTF 往復変換が 0.164931 ms → 0.144685 ms、整数表示・解析が 1.085425 ms → 0.567830 ms へと短縮されました。
この新旧比較は 1 回ずつの試行であるため、僅かな数値差のみを過大に評価することは避けています。
通常サイズの UTF 変換では、採用された ASCII 高速パスが 0.9275 ms → 0.7125 ms と改善した一方、混在 Unicode は 1.7395 ms → 1.7435 ms と同等水準を維持しました。
生成アセンブリにおいて、ASCII 書き込み時に `<16 x i8>` のベクトル拡張／縮小命令や ARM64 の `ushll`／`xtn`／`uzp1` が正しく活用されていることを確認しています。なお、文字列比較における 64-bit 一括読み出しを SIMD と呼称したり、WASM の専用 SIMD 対応を誇大に主張したりすることはありません。

**初回時点の残る差:** この測定環境において、動的関数呼び出しおよび 128-bit 整数ミキサーで約 1 割、Maybe ビルダーで約 14%、UTF 変換で約 1.3〜1.6 倍、整数表示・解析で約 1.9〜2.1 倍の遅れが比較対象言語との間に依然として存在します。
極小の並列タスクでは C# の常駐スレッドプールが有利であり、文字列比較では JavaScript のロープ（rope）構造や共有バッファ最適化が有利に働きます。
これらの遅れを表面上隠蔽するために、安全性検査の無効化、浮動小数点の再結合、ガベージコレクションの無視、あるいは GPU への無条件な一括移送といった妥協的な手法を導入することはありません。
GPU 実行、全数値型のリファレンス照合、コンパイル時間の短縮、および他アーキテクチャでの検証は今後の継続課題であり、あらゆる入力に対して無条件に高速であるという保証はできません。

生データは `target/benchmarks/performance-20260926/final-{generic,native}-0.1-{1,2,3}.json`、
小規模入力の結果は `final-generic-0.01-1.json`、新旧ペア比較は `cpp-paired-{before,after}.json` に保存されています。
第 1 回試行と同名のディレクトリに生成バイナリおよび実行条件を保存しています（これらはローカルの一時生成物であり、Git リポジトリにはコミットしていません）。
基準コミットは `868a295`、コンパイラ SHA-256 は変更前が
`f2892e5d66b023e9a9545ca982e3e1f768ca89ca1e2b57af41d1764ae0859752`、変更後が
`75500729e5a8f57ee770960a469e79515e8c52c91e5a1bf724cc53eb799c0afb` です。
最終バージョンにおいて、Rust の全テスト、Clippy、ならびに native および WASM の `-O0`／`-O3` における全実行系列を検証しました。
表示処理 88,063 件、解析・往復変換 89,422 件、全 Unicode スカラー値、不正入力時の例外処理、所有メモリの完全回収、タスクの完全ジョインを含みます。合否判定用の恣意的な速度閾値は設定していません。

## 再現可能な比較

手元でベンチマークを再現するための手順は以下のとおりです。

```sh
cargo build --release
node benchmarks/run.mjs target/release/tsuzuri

# コンパイラパスと反復数を任意に変更可能（最大 100,000,000）
node benchmarks/run.mjs target/release/tsuzuri 10000000
```

実行には標準的な LLVM ツールチェーンと Node.js 20 以降が必要です。
同一の Clang、`-O3`、同一の入力データを用いて以下の 3 つの実装を測定し、結果の JSON を標準出力へ出力します。

- `benchmarks/Mix.tz` をネイティブオブジェクトとしてコンパイルした関数
- `benchmarks/native.c` に記述された同一計算を行う C 言語関数
- 同一の `.tz` ソースを WebAssembly（WASM）としてコンパイルした関数

ワークロードは、符号なし右シフト、XOR、および 64-bit 整数の折り返し乗加算を反復するループ依存の整数ミキサーです。C 言語側も `uint64_t` を用いて記述されており、符号付き整数のオーバーフロー未定義動作には依存しません。
単純な等差数列の和のような単純ループでは、LLVM が閉形式（数式解）へとループそのものを消去してしまい、純粋なループ実行性能を正しく測定できないため、このような依存関係を持つミキサーを採用しています。

## 測定条件

既定の測定パラメータは 5,000,000 反復 $\times$ 9 サンプルです。サンプルごとに乱数シードを変更し、すべてのサンプルにおいて C、Tsuzuri ネイティブ、WASM の出力チェックサムが完全に一致することを検証します。
WASM については、独立した `BigInt` 参照実装との照合、反復回数 0 回、および負の反復回数の各エッジケースも追加検証します。チェックサムが 1 回でも不一致となった場合、その測定は失敗として破棄されます。

ネイティブ環境では外部 ABI 経由で関数を呼び出し、リンク時最適化（LTO）は適用しません。
C 言語のベースライン実装には `noinline` 属性を付与し、計測ループ内での入力読み出しおよび結果の保存は `volatile` アクセスとすることで、コンパイラによる計算の完全除去や結果の使い回しを防止します。
CPU 時間は標準の `clock()` を用いて計測し、C 言語を先行して測定する回と Tsuzuri を先行して測定する回を交互に切り替えます。
WASM については、ウォームアップ完了後の関数呼び出しを `performance.now()` による実経過時間（wall time）で測定します。
1 回のホスト関数呼び出しの内部にすべての反復処理を内包させ、JavaScript と WASM 間の境界呼び出しオーバーヘッドのみを測定してしまう事態を避けています。

出力される JSON の主なフィールド:

| 項目 | 意味 |
| --- | --- |
| `environment` | Tsuzuri、Clang、Node.js の各バージョン、OS、CPU アーキテクチャ情報 |
| `native.tsuzuri_median_ms` | Tsuzuri ネイティブ実装の処理時間の中央値（ms、数値が小さいほど高速） |
| `native.c_median_ms` | C 言語実装の処理時間の中央値（ms、数値が小さいほど高速） |
| `native.tsuzuri_over_c` | 時間比（Tsuzuri / C、単位なし。1 未満なら Tsuzuri が高速、1 超なら C が高速） |
| `wasm.median_ms` | WASM 実装の実経過時間の中央値（ms、数値が小さいほど高速） |
| `wasm.bytes` / `wasm.imports` | 生成された WASM モジュールのバイナリサイズおよびインポート数 |
| `raw` / `raw_ms` | 全測定サンプルの生データ配列 |

なお、ネイティブ環境の CPU 時間と WASM 環境の wall time は異なる時計ソースに基づいています。
単発の微小な測定差、測定環境が異なる結果同士の単純比率、あるいはブラウザデモにおける FPS の数値を、一般的なプログラミング言語間の性能差として短絡的に解釈してはなりません。
上記の C／WASM 比較は、ガベージコレクション、動的メモリ確保、文字列処理、巨大配列操作、I/O、GUI、あるいはゲームループ全体を網羅した C++／F#／C# との包括的ベンチマークではありません。

`Task.parallel` はネイティブの常駐スレッドプールによる有界 fork/join を実装していますが、上記の基本比較はタスク並列処理の速度比較を含むものではありません。`examples/tasks` やタスクの単体テストも、並行実行の正確性、計算結果、およびリソース回収を検証するためのものであり、高速化の証拠として提示するものではありません。
並列処理の正当な性能比較を行う場合は、同一のタスク粒度、分割手法、およびメモリコピー条件の下で、タスク生成、クロージャ捕捉の複製、結果メモリの確保、スレッド起動、同期、全ジョイン、およびメモリ解放に至るすべての実経過時間（wall time）を測定する必要があります。
CPU 時間は複数スレッドの実行時間を合算してしまうため、シングルスレッド逐次実行との経過時間比較には使用できません。また、常駐プールの起動および同期オーバーヘッドも入力サイズに依存するため、初回起動時とプール再利用時を明確に分離して評価する必要があります。なお、WASM におけるタスクは現状逐次フォールバックであり、マルチコア並列性能として報告することはありません。
| `wasm.median_ms` | WASM の wall time 中央値（ms、小さいほど高速） |
| `wasm.bytes` / `wasm.imports` | モジュールサイズ／インポート数 |
| `raw` / `raw_ms` | 全サンプルの値 |

ネイティブの CPU time と WASM の wall time は同じ時計ではありません。
単発の僅かな差、別々の環境の比、ブラウザー例の FPS を一般的な言語性能差と解釈しないでください。
上の C／WASM 比較は GC、動的メモリ、文字列、巨大配列、I/O、GUI、ゲーム全体、
C++／F#／C# の包括的比較を測っていません。

`Task.parallel` は現在ネイティブの常駐 pool による bounded fork/join を実装していますが、上の既存比較は
タスクの速度比較ではありません。`examples/tasks` とタスクのテストも、同時実行・結果・
資源回収を示すもので、速度向上の証拠には使いません。
並列処理を比較する場合は同じ仕事・分割・コピー条件で、タスクの生成、捕捉の複製、
結果領域の確保、スレッドの起動、同期、全 join／解放を含む wall time を測ります。
CPU time は複数スレッドの時間を合計するため、逐次版との経過時間比較には使えません。
常駐 pool の起動・同期費用も入力依存であり、初回と再利用時を分けて測る必要があります。
WASM のタスクは逐次フォールバックであり、マルチコア性能として報告しません。

## Task プール

```sh
cargo build --release --locked
node benchmarks/run-tasks.mjs target/release/tsuzuri --quick
node benchmarks/run-tasks.mjs target/release/tsuzuri --baseline-runtime target/f01-task-before.o --artifacts target/benchmarks/f01-measured
```

`--baseline-runtime` オプションには、同一ターゲット、Clang、`-O3`/PIC 設定で事前にビルド・保存した変更前のタスクランタイムオブジェクトを指定します。
同一の LLVM IR を新旧両ランタイムへリンクし、タスク生成、クロージャ捕捉、結果メモリ確保、並行実行、完了待機、およびリソース解放に至る全行程を測定対象に含めます。
コールド測定（cold）は初回呼び出しおよびスレッドプールの遅延起動時間を計測し、ウォーム測定（warm）は同一プロセス内でのプール再利用時の時間を計測します。コンパイル時間、プロセス起動時間、およびプロセス終了時のシャットダウン時間は計測区間外です。
`--quick` オプションは独立した整数参照実装との結果一致を確認するスモークテストであり、速度に関する合否判定閾値は設けていません。
通常の測定では 7 サンプルを採取し、新旧の実行順序を交互にローテーションさせ、動作環境、IR およびランタイムのハッシュ値、ならびに全サンプルの生データを `result.json` へ記録します。

チケット F01 実装時における測定結果（Apple M1 Max、10 logical CPUs、macOS Darwin 27、Apple Clang 21、`-O3 generic`、fast-math なし）:

| 仕事 | 要素数／反復 | 新 pool cold ms | 旧 runtime cold ms | 新 pool warm us/call | 旧 runtime warm us/call |
|---|---|---|---|---|---|
| tiny | 4／0 | 0.079 | 0.070 | 1.319 | 45.147 |
| small | 32／16 | 0.109 | 0.096 | 4.578 | 42.996 |
| bulk | 1024／256 | 0.641 | 0.426 | 423.500 | 309.187 |

所要時間は数値が小さいほど良好です。tiny および small のウォーム実行では大幅な時間短縮を達成した一方、コールド起動時やバルク（bulk）実行時では旧版よりも遅い結果となっており、全般的な高速化と過大に主張することはありません。
要素単位でのタスク割り当て、通知処理、およびミューテックスのロック競合が依然としてオーバーヘッド要因となっています。大量要素の処理に対しては、後述のチケット F02 による固定チャンク分割 API を用いて別途評価を実施します。
生データは `target/benchmarks/f01-measured/result.json`、同一 IR は同ディレクトリの `tasks.ll` に保存されています。
なお、この測定結果を冒頭の言語間ベンチマークサマリーに混入させることはせず、共有 CI に恣意的な速度閾値を設定することもありません。

## データ並列 API

```sh
cargo build --release --locked
node benchmarks/run-tasks.mjs target/release/tsuzuri --data-parallel --quick
node benchmarks/run-tasks.mjs target/release/tsuzuri --data-parallel --artifacts target/benchmarks/f02-measured
```

既存ハーネスのデータ並列比較モードを使用し、Array と Parallel の `init`、`map`、`sum` を同一の入力データおよび整数演算で比較します。
入出力メモリの確保、クロージャ捕捉、並行計算、スレッド間同期、およびリソース解放の全所要時間を計測します。`sum` の計測には入力データの生成時間も含まれるため、純粋な加算カーネル単体の演算速度比較ではない点に留意してください。
整数の加算演算は結合法則により処理順序が異なっても同一の合計値となり、各測定結果は独立した参照リファレンスと厳密に照合されます。WASM 環境においては 18 件の正確性検証のみを実施し、並列による高速化として報告することはありません。
7 サンプルの生データ、実行環境、ハッシュ値、生成 IR、およびアセンブリコードを保存します。アセンブリ解析により、専用のチャンクコールバックおよび `tsuzuri_task_parallel` の呼び出しが正しく生成されていることを確認しました。

チケット F02 実装時における Apple M1 Max（10 logical CPUs）、Apple Clang 21、`-O3 generic` のウォーム中央値（ms/call、数値が小さいほど高速）:

| 要素数／計算反復 | Parallel.init | Array.init | Parallel.map | Array.map | Parallel.sum | Array.sum |
|---|---|---|---|---|---|---|
| 64／8 | 0.000258 | 0.000281 | 0.000297 | 0.000320 | 0.000313 | 0.000266 |
| 8193／16 | 0.068719 | 0.061656 | 0.074875 | 0.062750 | 0.103906 | 0.062188 |
| 262144／32 | 1.244375 | 5.511875 | 1.188750 | 5.566500 | 5.489375 | 5.378500 |

大規模なデータ入力における生成（init）およびマッピング（map）処理では顕著な時間短縮を達成したものの、チャンク境界付近の小規模データや集約処理（sum）においては逐次版の後塵を拝する結果となりました。この限定的な時間差をもって一般的な並列優位性と短絡的に解釈することはありません。
測定結果は `target/benchmarks/f02-measured/result.json` に保存されています。SIMD や GPU による加速を誇大に主張せず、共有 CI に速度閾値を追加することもありません。

## C++20 とのネイティブ比較

```sh
cargo build --release --locked
node benchmarks/run-cpp.mjs target/release/tsuzuri

# 両言語をビルドマシンの CPU 向けに最適化（バイナリの他機への移植性は低下）
node benchmarks/run-cpp.mjs target/release/tsuzuri --cpu native

# 小規模な入力で計算結果の一致のみを確認（CI 用。速度比較には使用しない）
node benchmarks/run-cpp.mjs target/release/tsuzuri --quick
```

環境変数 `TSUZURI_CLANG`（デフォルトは `clang`）を両言語で共通して使用し、C++ は同一の Clang ドライバーを `--driver-mode=g++` で起動してコンパイルします。実行には C++20 標準ライブラリ、Rust、および Node.js 24 以降が必要です。
以下の基本 3 種目に加え、現在は UTF 系列 4 種目、format_parse、math_intrinsics、タスク系列 3 種目を計測しています。追加種目の測定条件は冒頭の横断比較の節を参照してください。以下の測定結果は当時の基本 3 種目の条件に基づくものです。

| 種目 | 既定の仕事量 | 含める処理 |
|---|---|---|
| `integer_mix` | 20,000,000 反復 | 既存の `Mix.tz` と同一の 64-bit 整数ミキサー |
| `mandelbrot` | 768 × 768 点、最大 256 反復/点 | f64 精度による Mandelbrot escape-time 計算、全画素の反復数を集約 |
| `array_sum` | 8,000,000 要素（64 MB） | 64-bit 配列のヒープ確保、乱数シード依存の初期化、全要素の総和計算、メモリ解放 |

両言語ともに `-O3` 最適化を適用し、ネイティブオブジェクトは位置独立コード（PIC）とし、リンク時最適化（LTO）、fast-math フラグ、および CPU 固有の `-march=native` は使用しません。C++ は `-ffp-contract=off` を指定して暗黙の FMA 融合演算を無効化し、Tsuzuri と浮動小数点の演算順序を完全に一致させています。C++ の整数演算は `uint64_t` と `std::bit_cast` を用いて 64-bit 折り返し演算を忠実に再現し、符号付き整数のオーバーフロー未定義動作には依存しません。
配列の集約処理において、Tsuzuri は読み取り借用を用いて無駄なディープコピーを徹底的に排除しています。C++ 側も `std::make_unique_for_overwrite` を用いて未初期化領域を確保したうえで同一の計算式で埋め、不公平なゼロ初期化のオーバーヘッドを課していません。もちろん、Tsuzuri 側の安全性検査を無効化するような不公平な操作も行っていません。
`--cpu native` オプションを選択した場合にのみ、両言語で同一の `-march=native`（x86）または `-mcpu=native`（ARM）が適用されます。generic 設定と native 設定の結果を混在させることはせず、出力 JSON の `cpp_flags` および `tsuzuri_flags` にビルド条件を明確に記録します。

測定では C++、Rust、Tsuzuri の各バイナリを事前にウォームアップし、全 6 通りの実行順序を 2 回ずつ巡回させて計 12 サンプルを採取します。
短時間で完了する処理は反復回数を自動調整し、`clock()` による CPU 時間と `steady_clock` による実経過時間（wall time）をそれぞれ個別に記録します。コンパイル時間やプロセス起動時間は計測対象に含まれません。
C++ カーネルには `noinline` 属性を付与し、Tsuzuri は別オブジェクトから外部関数として呼び出し、入力の読み出しと計算結果の保存を `volatile` アクセスとすることで、コンパイラによる計算の完全除去や結果の使い回しを防止します。
要素数 0、極小入力、負の乱数シード、`i64` の最小値・最大値シードを含む各 25 ケースの境界条件について、両言語および独立した JavaScript／BigInt 参照実装との等価性を照合します。
本測定の全サンプルにおいても両言語のチェックサムが完全に一致しなければテスト失敗となります。CI 上では小規模入力による正しさの検証のみを行い、速度に関する合否判定は行いません。

出力 JSON には、CPU、OS、ツールのバージョン、コンパイル条件、時計の分解能、ならびに各種目の `checks` および `raw` データが含まれます。`cpp_median_ms` と `tsuzuri_median_ms` は 12 サンプルの中央値（中央 2 値の平均値）であり、`tsuzuri_over_cpp` は CPU 時間の比率を示します。
並列処理種目の評価には `tsuzuri_wall_over_cpp` や `tsuzuri_wall_over_rust` を使用してください。
**時間比が 1 未満であれば Tsuzuri が高速であり、1 より大きければ比較対象の言語が高速**であることを意味します。
この 3 種目のみをもって一般的な言語の優劣を決定づけることはできません。配列種目はメモリの確保と解放も含んでいるため、メモリアクセス帯域のみの純粋な測定でもありません。僅かな差異は実行ごとの揺らぎと併せて慎重に判断してください。

### 改善前の実測例（2026-09-21）

測定環境: Apple M1 Max、arm64 アーキテクチャ、Darwin 25.6.0、Apple Clang 21.0.0（clang-2100.3.34.2）、Node.js 20.19.6、Tsuzuri 0.1.0。
コンパイラの基準コミットは `b4626bd`。上記の既定サイズで 3 回連続実行し、各言語・各種目の計 30 サンプルをまとめた中央値です。**処理時間（ms）および時間比ともに、数値が小さいほど有利です。**

| 種目 | Tsuzuri (ms) | C++20 (ms) | Tsuzuri / C++（時間比） | 読み方 |
| --- | ---: | ---: | ---: | --- |
| 整数ミキサー | 31.656 | 31.798 | 0.996 | ほぼ同速 |
| Mandelbrot | 1269.500 | 129.276 | 9.820 | C++ が約 9.8 倍高速 |
| 配列の生成・集計・解放 | 3.745 | 3.710 | 1.009 | ほぼ同速 |

各試行の中央値の時間比は、整数ミキサーが 0.999〜1.001、Mandelbrot が 9.792〜9.844、配列操作が 0.994〜1.030 でした。整数および配列における微小な差異を勝敗として扱うことはしません。
生データは以下のコマンドで保存可能です。

```sh
mkdir -p target/benchmarks
for trial in 1 2 3; do
  node benchmarks/run-cpp.mjs target/release/tsuzuri > "target/benchmarks/cpp-$trial.json" || break
done
```

この最適化前のコード生成において、Tsuzuri は画素座標ごとの `i64 -> f64` 型変換 2 回に対して汎用ソフトウェア変換ルーチン `tz_soft_cast` を呼び出していたのに対し、C++ は CPU ネイティブの整数→浮動小数点変換命令を使用していました。
`src/llvm.rs` の `cast` および `src/runtime/numeric.c` の `tz_soft_cast` が該当する処理です。
Mandelbrot の escape-time 計算の内側ループは双方ともに浮動小数点ハードウェア命令で実行されていたため、**型変換処理の実装が最大のボトルネック**であることが判明しました。ただし、この測定のみをもって浮動小数点演算全般が 9.8 倍遅いと一般化して結論付けることは誤りです。
なお、この比較段階ではコンパイラ本体の最適化パス自体は変更していません。

### 直接数値変換への改善後（2026-09-22）

同一の M1 Max、ツールチェーン、入力データ、および仕事量の下で、`generic` と `native` をそれぞれ 3 回連続実行しました。各条件・各言語の 30 サンプルをまとめた中央値（ms）です。
ネイティブコードにおける数値仕様、トラップ発生条件、安全性検査、および演算順序の厳密性は一切緩めていません。
**処理時間（ms）は数値が小さいほど高速です。同一の CPU 指定列同士を比較してください。**

| 種目 | Tsuzuri generic (ms) | Tsuzuri native (ms) | C++ generic (ms) | C++ native (ms) |
| --- | ---: | ---: | ---: | ---: |
| 整数ミキサー | 31.346 | 31.350 | 31.372 | 31.402 |
| Mandelbrot | 129.100 | 128.270 | 128.371 | 128.332 |
| 配列の生成・集計・解放 | 3.723 | 3.605 | 3.610 | 3.583 |

Mandelbrot 種目の Tsuzuri generic は、改善前の 1269.500 ms から 129.100 ms へと**約 9.8 倍の劇的な高速化**を達成しました。C++ との時間比も約 9.82 から約 1.006 へと縮まり、完全に同等水準に到達しました。
主な改善点は、8〜64-bit 整数と f32／f64 間の直接命令変換、浮動小数点→整数への飽和 intrinsic の適用、ならびに f32 と f64 の直接変換です。最適化後の LLVM IR において、画素座標の整数→f64 変換が直接の機械語命令となり、重い `tz_soft_cast` 呼び出しが完全に消去されていることを確認しました。また配列の集約処理についても、SIMD のベクトル整数加算命令へ正しく lowering されていることを確認しています。

このマシン環境においては `native` 指定による追加の高速化効果は限定的であり、CPU 指定を変更するだけで無条件に大幅高速化するとは限らないことがわかります。
配列処理は実行環境の負荷によって数%の揺らぎが生じるため、微小な差をもって一般的な言語優位性とみなすことは避けています。
これは GPU やマルチコア並列の測定ではなく、シングルスレッド CPU における比較です。正確な広幅整数や decimal 演算のソフトウェア経路は安全に維持されています。

生データは改善前とは独立して `target/benchmarks/cpp-generic-{1,2,3}.json` および `target/benchmarks/cpp-native-{1,2,3}.json` に保存されました。再実行のコマンドは以下のとおりです。

```sh
mkdir -p target/benchmarks
for cpu in generic native; do
  for trial in 1 2 3; do
    node benchmarks/run-cpp.mjs target/release/tsuzuri --cpu "$cpu" \
      > "target/benchmarks/cpp-$cpu-$trial.json" || exit 1
  done
done
```

## コンピュテーション式の比較

B05の`match!`/`and!`追加後も、既存9ワークロードと同じ参照値を確認しています。
`run-computations.mjs`の`extensions`欄はWASM O3のBind2、MergeSources+Bind、手書きBind2を同じ整数mixで比較します。
新構文用の小プロジェクトを一時領域に生成するため、既存の旧compiler比較用sourceは変更しません。
複数サイズ/seedのBigInt参照、回転した測定順、通常9回/quick3回の生時間と中央値を出します。2026-09-27のquick照合は成功し、速度優位性は主張していません。
この追加欄はWASMだけで、nativeや異なるbuilderの意味同値を推測する比較ではありません。

```sh
cargo build --release --locked
node benchmarks/run-computations.mjs target/release/tsuzuri
node benchmarks/run-computations.mjs target/release/tsuzuri --cpu native

# CI 用。速度の合否条件はなく、小さい入力・参照結果・解放を検査する
node benchmarks/run-computations.mjs target/release/tsuzuri --quick

# 変更前のコンパイラも同じ入力でビルドし、同じプロセス内で交互に比較
node benchmarks/run-computations.mjs target/release/tsuzuri \
  --baseline target/benchmarks/tsuzuri-ce-baseline-869c4b1 \
  --artifacts target/benchmarks/ce-inspect
```

比較対象は `.tc` のユーザー定義ビルダー、同じ処理の手書き Tsuzuri、
`benchmarks/computations/native.cpp` の最適化用 C++20 実装です。
**比較した実装の中での到達点を測るもので、理論上の最速実装であることの証明ではありません。**
任意のビルダーのアルゴリズムや、あらゆる CPU・入力での性能を代表する測定でもありません。

| 種目 | native の仕事量 | 比較する処理 |
|---|---:|---|
| `bind` | 2,000,000 反復 | `let!` を2つ持つ整数ミキサー。loop-carried の値に依存 |
| `checked` | 2,000,000 反復 | 成功フラグ付きレコードを `Bind` し、失敗時に続きを呼ばない |
| `delayed` | 500,000 反復 | `Delay`／`Run`／`Combine` と複数 yield を持つミキサー |
| `array_for` | 8,192 要素 | 確保・seed 依存の初期化・捕捉した倍率による変換集計・解放 |
| `array_bind` | 8,192 要素 | 配列を型注釈付き `let!` に渡し、同じ変換集計を行う |
| `owned_capture` | 8,192 反復 | 256 要素の所有配列を捕捉し、前の結果に依存する添字で読み出す |
| `std_option` | 2,000,000 反復 | 標準 Maybe の成功／失敗と、手書きの同じ union match |
| `std_result` | 2,000,000 反復 | 標準 Result の二段の error 伝播と、手書きの同じ union match |
| `std_option_owned` | 8,192 反復 | 所有文字列の連結・成功／失敗・解放を Maybe と手書き match で比較 |
| `std_option_length` | 2,000,000 反復 | 所有文字列を作らず、Maybe で定数の長さ 12 または 0 を返す |

`std_*` は B01 で追加したため、この三種目を含む現在のソースとの `--baseline` 比較には Maybe／Result 対応版が必要です。
現在の `std_option_owned` は C++ の `std::u16string`、Rust の malloc／Drop による `OwnedUtf16`、
C# の `static readonly string`、JavaScript の文字列の parts によって、同じ 2 回の連結をソースに記述します。
定数長は新しい `std_option_length` です。最適化の障壁は入れず、残る確保を実際の生成コードで確認します。

2026-10-14 の Apple Clang 21.0.0／rustc 1.98.1 の `-O3` 後、kernel 本体に残る call／invoke の静的な箇所は次のとおりです。
標準ライブラリの helper 内部の箇所は含めず、実行時の確保回数でもありません。
生の IR は `target/perf/PX02-managed-formal-artifacts/computations/` にあります。

| kernel | malloc | free | operator new | operator delete |
| --- | ---: | ---: | ---: | ---: |
| Tsuzuri direct／computation owned（それぞれ） | 1 | 1 | 0 | 0 |
| Tsuzuri direct／computation length（それぞれ） | 0 | 0 | 0 | 0 |
| C++ owned | 0 | 0 | 2 | 3 |
| C++ length | 0 | 0 | 0 | 0 |
| Rust owned／length（それぞれ） | 0 | 0 | 0 | 0 |
GC、SSO、rope、LLVM による確保の除去の差は残り、それらを同一の解放の方式とみなしません。

### B02 の検証（2026-09-26）

早期伝播の比較には既存の `std_option`／`std_result`／`std_option_owned` と手書き版を再利用します。
Node 24.21.0、Apple Clang 21、Apple M1 Max で全 9 種目の `--quick` 検証を通過しました。
これは `correctness-smoke` モードであり、速度改善や他言語に対する優位性の測定結果ではありません。
速度閾値を合否条件にはしません。JSON は `target/p1-computations.json`、IR・アセンブリなどは
`target/benchmarks/p1-computations/` に保存しています。

```sh
cargo build --release --locked
npx --yes --package=node@24 node benchmarks/run-computations.mjs target/release/tsuzuri \
  --quick --artifacts target/benchmarks/p1-computations > target/p1-computations.json
```

### 標準 Maybe／Result の測定（2026-09-25）

`be5b27a` に P0 の未コミット差分を適用したコンパイラ（SHA-256
`7e59f9bb41669b89639e1e1ffc83af94f8fd78d409d98ebe24b074816b7c661a`）を使用しました。
Apple M1 Max（arm64、10 logical CPU）、macOS／Darwin 27.0.0、Homebrew Clang 23.1.1、Node 20.19.6。
`generic`・`-O3`・fast-math／LTO なし、12 サンプルの中央値です。native は CPU 時間、
WASM は warm-up 後の wall time で、起動時間は含みません。
**処理時間（ms）は小さいほど高速です。native と WASM は時計・仕事量が異なるため直接比較しません。**

| 種目 | Tsuzuri native builder (ms) | Tsuzuri native 手書き (ms) | C++ native (ms) | Tsuzuri WASM builder (ms) | Tsuzuri WASM 手書き (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| `std_option` | 5.273125 | 5.290500 | 4.666600 | 0.002004 | 0.001854 |
| `std_result` | 5.257875 | 5.307625 | 5.168375 | 0.002133 | 0.002101 |
| `std_option_owned` | 0.177957 | 0.173719 | 0.016241 | 0.016846 | 0.014989 |

native の反復数は上表の仕事量、WASM は各 1024 反復です。1 回の実行だけで数 % の差を改善とはみなしません。
特に所有文字列の C++ は確保を除去して長さを直接計算する参照なので、builder の追加コストは
同じ所有値処理をする手書き Tsuzuri と比較してください。
64 反復の別の計測用実行では scalar 2 種目は両版とも 0 確保、所有文字列は両版とも 55 回・605 バイトでした。
最適化後 IR でも scalar は直接ループで確保・間接 callback なし、所有文字列は成功経路の 11 バイト確保と解放が残ります。
SIMD・並列・GPU の加速を示す測定ではありません。

この測定は string が UTF-8 だった時点の結果です。現在の string は UTF-16 なので、
文字列の確保バイト数や時間をそのまま比較できません。旧表現は utf8string として残しています。
符号化・仕事量を揃えて再測定するまでは、この表を文字列変更の性能結果として使わないでください。

生データは `target/benchmarks/p0-computations.json`、IR・アセンブリ・実行ファイルは
`target/benchmarks/p0-computations/` に保存しました。再現コマンド:

```sh
node benchmarks/run-computations.mjs target/release/tsuzuri \
  --artifacts target/benchmarks/p0-computations > target/benchmarks/p0-computations.json
```

C++ は符号なし整数と `std::bit_cast` で i64 の折り返しを再現し、signed overflow に依存しません。
両版の Tsuzuri と C++ は同じ反復継続条件を使い、負の反復数を入口で拒否します。
計算結果をコンパイル時定数に勝手に置換したり、Tsuzuri にのみ不利となる不要なゼロ初期化を C++ 側に課したりすることはありません。
配列種目には実際のメモリ確保、初期化、およびメモリ解放の全行程を含めています。`owned_capture` の C++ 実装は固定長配列をスタック上に配置する手書き最適化版であり、Tsuzuri の条件に合わせて不要なヒープ割り当てやクロージャ呼び出しを追加することはしていません。
基準版のコンパイラにおいて深い再帰コールバックがスタックを使い切る現象が発生したため、時間比較には新旧両版が正常に完走可能なデータサイズを採用しています。
なお、改善版が 262,144 回の深いコールバックをネイティブおよび WASM の `-O3` において正常に処理できることは、独立した実行テストによって確認されています。

### 条件・出力・確保数

両言語で同一の Clang、`-O3`、PIC、`-fno-fast-math`、`-ffp-contract=off` を使用し、リンク時最適化（LTO）は行いません。
`--cpu native` を指定した場合にのみ、両言語に対して同一の CPU 最適化フラグを付与します。
ネイティブは `clock()` による CPU 時間、WASM は Node.js 環境でウォームアップ完了後の実経過時間（wall time）を測定します。
WASM は全種目 1,024 要素／反復で統一し、手書き Tsuzuri 版、改善前版、および改善後版の 3 者を比較します。
WASM 環境に対して C++ 標準ライブラリのリンクを強制することはせず、ネイティブと WASM の所要時間を同一の比率として単純合算することはありません。

ネイティブは各版を 2 回、WASM は 3 回事前にウォームアップします。
各種目について 12 サンプルを採取し、乱数シードと計測順序をローテーションさせます。各版ごとに反復回数を校正し、約 20 ms のバッチを目安として、1 回の呼び出しあたりの所要時間へ正規化します。
測定上限は 10,000 回の呼び出しとし、極めて高速な実行経路ではバッチ所要時間も 20 ms 未満となります。
ネイティブ環境での入力読み出しおよび結果の保存は `volatile` アクセスとし、C++ カーネルには `noinline` 属性を付与します。
計測区間内の全呼び出しにおけるチェックサム検証に加え、要素数 0、極小入力、負のシード、`i64` の最小値・最大値を含む各 25 ケースの境界条件を、JavaScript／BigInt の独立した参照実装と照合します。
WASM のホスト境界呼び出しおよび結果照合は計測区間に含みますが、コンパイル、モジュールインスタンス化、およびプロセス起動時間は含みません。

出力 JSON には、`native_flags`、`wasm_flags`、各コンパイラの SHA-256、CPU・OS・ツールの構成情報、ならびに全サンプルの処理時間、反復回数、チェックサムが記録されます。
`native.computation_over_cpp` および `computation_over_direct` は 1 未満であればコンピュテーション式（CE）が高速であることを示す時間比であり、`native.speedup` および `wasm.speedup` は改善前時間を改善後時間で割った改善倍率を示します。
測定データの最小値と最大値も保存されるため、単発の僅かな数値変動のみをもって速度向上と即断しないよう配慮しています。
`--artifacts` オプションを指定すると、ネイティブのソース IR、最適化後 IR、アセンブリコード、C++ の IR およびアセンブリ、オブジェクトファイル、実行ファイル、WASM バイナリが指定ディレクトリへ出力されます。

メモリ確保回数を示す `allocations_at_size_64` は、**時間計測用のバイナリとは別の専用実行ファイル**を用いて計数されます。
まず通常どおり `-O3` 最適化まで完了させ、その生成 IR 内に残存した `malloc`／`free` 呼び出しのみをメモリ追跡関数へと置き換えます。
追跡処理による副作用をオプティマイザが除去してしまわないよう最適化属性を適切に調整したうえで、再最適化を行わずに実行し、全呼び出し完了後の未解放バイト数が完全に 0 であることも確認します。
最適化前の初期 IR に対して追跡関数を挿入した後に `-O3` を適用すると、本来のリリース最適化であれば消去されるはずのメモリ確保までが不当に残存してしまい、実際の製品バイナリの挙動を正しく反映できないため、この測定方式を採用しています。

### 改善の範囲

スコープ外へエスケープしない既知の継続関数を通常の関数呼び出しとして特殊化し、読み取りのみを行う所有捕捉値を呼び出し元が生存させたまま内部的に借用（貸出）させることで、間接関数呼び出し、クロージャ環境の動的確保、および反復ごとのメモリ解放を完全に消去しました。
一時的なクロージャ環境はエントリーブロックのスタック領域（`alloca`）を使用し、1 回しか参照されない Copy 型ローカル変数の深いコピーも省略されます。
同一の最適化経路は、手書きの高階関数、パイプライン処理、ならびに配列やリストの初期化コードにも等しく適用されます。
これは型検査や所有権検査をバイパスするような危険な最適化ではなく、スナップショット意味論、評価順序、およびトラップ発生条件の厳密性を完全に維持しています。

動的に決定される未知の関数値、スコープ外へ逃げる関数値、所有権を持つ捕捉値の消費や置換、あるいは戻り値として借用参照を返却するケースなどでは、従来のヒープ環境が安全に維持されます。
また、自動生成される特殊化ワーカー関数の予算上限を超過した場合にも、健全な通常経路へと安全にフォールバックします。
したがって、これらのベンチマーク 6 種目が手書き最適化版と同等の性能に到達したからといって、あらゆるコンピュテーション式が無条件にゼロコスト化されると過信してはなりません。
GPU 実行、マルチコア並列処理、あるいは専用 SIMD バックエンドによる高速化を測定したものではない点にも留意してください。

### 実測（2026-09-22）

測定環境: Apple M1 Max（arm64、Darwin 27.0.0）、Apple Clang 21.0.0（clang-2100.3.34.2）、Node.js 20.19.6、Tsuzuri 0.1.0。
改善前のベースラインには、コミット `869c4b1` から保存したコンパイラバイナリを使用しました。
同一のベンチマークソースコードを新旧両コンパイラに引き渡し、generic および native 設定で各 3 回試行、計 36 サンプルの測定結果をまとめた中央値で比較検証しました。
反復条件を C++ と揃える前に行われた初期の探索的測定データは、本測定結果には混入させていません。

以下は native generic 設定における処理時間（ms）です。**時間および時間比は数値が小さいほど有利であり、改善倍率は数値が大きいほど良好です。**
改善倍率は「改善前の処理時間 / 改善後の処理時間」、時間比は「改善後 Tsuzuri の処理時間 / C++ の処理時間」として計算されています。

| 種目 | Tsuzuri 改善後 CE (ms) | Tsuzuri 改善前 CE (ms) | C++ (ms) | 改善倍率（倍） | Tsuzuri 改善後 / C++（時間比） |
| --- | ---: | ---: | ---: | ---: | ---: |
| `bind` | 3.208357 | 3.207643 | 3.205500 | 1.00 | 1.001 |
| `checked` | 5.240125 | 5.145250 | 5.147625 | 0.98 | 1.018 |
| `delayed` | 1.295531 | 1.302906 | 1.289031 | 1.01 | 1.005 |
| `array_for` | 0.005649 | 0.370280 | 0.005634 | **65.5** | 1.003 |
| `array_bind` | 0.005616 | 0.007913 | 0.005627 | **1.41** | 0.998 |
| `owned_capture` | 0.026332 | 1.419150 | 0.026308 | **53.9** | 1.001 |

native CPU 指定の環境においてもほぼ同様の傾向が確認され、配列 `for` で 66.1 倍、所有配列の捕捉で 53.7 倍、配列 bind で 1.40 倍の顕著な改善を示しました。改善後の C++ との時間比は、generic 設定で 0.998〜1.018、native CPU 指定で 0.994〜1.020 の範囲に収まっています。
**手書きの最適化版とほぼ同等の性能水準に到達したものの、すべての種目において C++ を圧倒したわけではない点に客観的な留意が必要です。**
特に `checked` 種目のネイティブ実行では改善前より約 2% の遅延が記録されており、このような微小な低下を隠蔽して全面的な高速化と強弁することはしません。
なお、単純なスカラー型のコンピュテーション式については、今回の改善前からネイティブ LLVM の `-O3` 最適化によってクロージャ環境のメモリ確保がすでに消去されていました。

WASM 環境における測定は、native generic と同一タイミングで採取した 3 試行、計 36 サンプルに基づきます。
仕事量は各 1,024 要素／反復、時間単位はマイクロ秒（µs）であり、ネイティブ表の絶対値とは直接比較できません。
**処理時間（µs）は数値が小さいほど高速であり、改善倍率は数値が大きいほど良好です。**

| 種目 | Tsuzuri 改善後 CE (µs) | Tsuzuri 改善前 CE (µs) | 手書き Tsuzuri (µs) | 改善倍率（倍） |
| --- | ---: | ---: | ---: | ---: |
| `bind` | 2.031 | 17.167 | 2.013 | **8.45** |
| `checked` | 2.102 | 14.858 | 2.083 | **7.07** |
| `delayed` | 2.781 | 25.389 | 2.796 | **9.13** |
| `array_for` | 0.721 | 27.876 | 0.716 | **38.7** |
| `array_bind` | 0.713 | 5.227 | 0.721 | **7.33** |
| `owned_capture` | 3.998 | 178.551 | 3.988 | **44.7** |

改善前の WASM 環境では、独自アロケータを介したクロージャ環境の確保および複製が、ネイティブ環境のようには LLVM によって消去されずに残存していました。
静的な継続の特殊化および要素初期化関数の直接インライン呼び出しを導入したことにより、このオーバーヘッド差も大幅に縮小されました。
native CPU 指定の測定に付随する WASM 実行においても同一のバイナリ条件であり、同等の性能結果が得られています。

最適化後のネイティブ LLVM IR に残存するメモリ確保回数の測定結果（入力サイズ 64）です。**数値が少ないほどメモリ確保負荷が低減されていることを示します（時間の表ではありません）。**

| 種目 | Tsuzuri 改善後 CE (回) | Tsuzuri 改善前 CE (回) | 手書き Tsuzuri (回) |
| --- | ---: | ---: | ---: |
| スカラー3種目 | 0 | 0 | 0 |
| `array_for` | **1** | 130 | 1 |
| `array_bind` | **1** | 3 | 1 |
| `owned_capture` | **1** | 259 | 1 |

改善後の配列種目における IR および生成アセンブリでは、入力データ配列自体のメモリ確保と解放のみが残り、ホットループ内から継続関数の間接呼び出し、環境の動的確保、および深いコピー処理が完全に消去されていることが確認されました。
反復終了後にクロージャ環境を解放するために残存していた再帰呼び出し構造も、既知のコールバックとして認識されることで LLVM がループ構造へと安全に変換できるようになりました。
ただし、これはエスケープしない既知の関数呼び出しに対する最適化であり、一般的な未知の関数値やエスケープするクロージャに対して同一の保証が拡張されたわけではない点にご留意ください。

生データおよび生成されたコード群は、`target/benchmarks/ce-final-{generic,native}-{1,2,3}.json` および同名ディレクトリに保存されています。出力 JSON 内のコンパイラ SHA-256 は、
改善前が `c161c53723ef4620aed36cf1e8872c5a4efc701adfeb296e54927a9c2d9cc95d`、
改善後が `8e22ddcba72572b0bd8f0dc4dfee6129618a4c7e1c4b08b706e1172646385b0c` です。
ベースラインバイナリは、改善前のコミットを別作業ディレクトリでチェックアウト・ビルドして保存することで再作成可能です。再測定のコマンド例は以下のとおりです。

```sh
for cpu in generic native; do
  for trial in 1 2 3; do
    node benchmarks/run-computations.mjs target/release/tsuzuri \
      --cpu "$cpu" --baseline target/benchmarks/tsuzuri-ce-baseline-869c4b1 \
      --artifacts "target/benchmarks/ce-final-$cpu-$trial" \
      > "target/benchmarks/ce-final-$cpu-$trial.json" || exit 1
  done
done
```

## 制御構文の比較

```sh
cargo build --release --locked
node benchmarks/run-control.mjs target/release/tsuzuri
node benchmarks/run-control.mjs target/release/tsuzuri --cpu native
node benchmarks/run-control.mjs target/release/tsuzuri --quick

# 保存した変更前のコンパイラと、同一プロセス内でも比較検証可能
node benchmarks/run-control.mjs target/release/tsuzuri \
  --baseline target/benchmarks/tsuzuri-recursion-baseline-b3f658e
```

`benchmarks/control/Main.tz` に実装された各種の新しい制御構文を、C11、C++20、および Rust の各実装と比較検証します。
C および C++ は同一の `reference.c` をそれぞれの言語モードでコンパイルし、Clang のバージョンおよび CPU 指定を Tsuzuri と完全に統一します。
Rust は `reference.rs` を `rustc` でコンパイルして独立した PIC オブジェクトとしてリンクします。
環境変数 `TSUZURI_CLANG` および `TSUZURI_RUSTC` によって使用ツールを明示指定可能です。
全言語で `-O3` 相当の最適化を適用し、LTO なし、fast-math なし、整数演算は同一の 64-bit 折り返し演算に揃えています。
Rust のスライス走査コードは通常の安全な Rust コードとして記述されており、未初期化バッファの確保・初期化・解放の処理のみを同一システムの `malloc`／`free` に揃えています。比較対象の言語にのみ不当なゼロ初期化や余分なコピーを課すような操作は一切行っていません。

| 種目 | 既定の仕事量 | 比較する処理 |
|---|---|---|
| `while_mix` | 8,000,000 反復 | ループ依存性を持つ整数ミキサー |
| `for_mix` | 8,000,000 反復 | i32 の `downto` と、対応する C／C++ の for、Rust の逆順 inclusive range |
| `tail_mix` | 8,000,000 反復 | match からの自己末尾再帰と、同じ計算の手書きループ |
| `tail_if_mix` | 8,000,000 反復 | 同じ末尾再帰を if で記述 |
| `tail_builtin_mix` | 8,000,000 反復 | 同じ末尾再帰を `Sub.sub` で記述 |
| `match_dispatch` | 8,000,000 選択 | 16 通りの整数分類と折り返し加算 |
| `array_sum` | 4,000,000 要素（32 MB） | 確保、seed 依存の初期化、for-in 集計、解放の全体 |
| `array_index_sum` | 4,000,000 要素（32 MB） | 確保、seed 依存の初期化、添字 `values[i]` による集計、解放の全体（F12 で境界検査が残らない形） |

各種目・各言語を二回ウォームアップし、12 サンプルを採取します。
`--baseline` 付きでは変更前も五つ目の実装としてリンクし、全実装で順序を均等に巡回できる15サンプルにします。
現在の一サンプルは約20 msを目標に反復数を調整した一回当たりの CPU/wall time です。順序は四言語で巡回・反転させ、
volatile の入力と結果保存、別オブジェクト・LTO 無効により計算の削除・再利用を避けます。
25 組の小さい入力を四言語と独立した BigInt 実装で照合し、計測中もすべての checksum を確認します。
`--quick` は正しさだけの小規模実行で、時間を性能の判断には使いません。
WASM は制御構文の native／WASM `-O0`／`-O3` テストで検証し、この表のネイティブ時間と混ぜません。

JSON にツールのバージョン、Rust の LLVM バージョン、コンパイラ SHA-256、
CPU、OS、全フラグ、生サンプル、各中央値、`tsuzuri_over_c`／`cpp`／`rust` を保存します。
変更前を指定した場合は、その SHA-256、`before_median_ms` と `speedup`（変更前／変更後）も記録します。
比率が 1 未満ならその相手より速いことを表します。
`--artifacts directory` は各言語のオブジェクト、LLVM IR、アセンブリを保存します。
自動検出する vector 命令数は調査用で、**それだけを SIMD の実行や高速化の証明にはしません**。

実装中の初回測定では for が C/C++ 比約 1.60、整数 match が Rust 比約 1.62 でした。
32-bit カウンターを毎回拡張する経路を、範囲が証明された広い誘導変数へ変更しました。
密な定数パターンは早い段階で静的表へ下げ、小さい整数 reduction にだけ LLVM の展開ヒントを渡します。
全ループへの一律の展開ヒントはミキサーを遅くしたため採用していません。
整数の overflow フラグや浮動小数点の再結合で意味を緩める変更も行っていません。

再測定例:

```sh
mkdir -p target/benchmarks
for cpu in generic native; do
  for trial in 1 2 3; do
    node benchmarks/run-control.mjs target/release/tsuzuri --cpu "$cpu" \
      --artifacts "target/benchmarks/control-final-$cpu-$trial" \
      > "target/benchmarks/control-final-$cpu-$trial.json" || exit 1
  done
done
```

### 制御構文の実測（2026-09-22）

Apple M1 Max（10 logical CPUs）、arm64 macOS/Darwin 27.0.0、
Apple Clang 21.0.0（clang-2100.3.34.2）、Rust 1.98.1／LLVM 22.1.8、Node.js 20.19.6。
下表の処理時間は独立した 3 回の試行における中央値の中央値であり、時間比も各試行で算出された比率の中央値を示しています。
（個別に集計された時間同士の商と比率の中央値が完全に一致するとは限りません）。
**処理時間（ms）および時間比ともに、数値が小さいほど有利です。時間比が 1 未満であれば Tsuzuri が高速であることを示します。**

| CPU 指定 | 種目 | Tsuzuri (ms) | Tsuzuri / C（時間比） | Tsuzuri / C++（時間比） | Tsuzuri / Rust（時間比） |
| --- | --- | ---: | ---: | ---: | ---: |
| generic | while_mix | 12.469 | 1.002 | 0.998 | 1.002 |
| generic | for_mix | 12.470 | 1.000 | 0.999 | 0.624 |
| generic | tail_mix | 14.989 | 1.196 | 1.200 | 1.201 |
| generic | match_dispatch | 2.489 | 0.614 | 0.613 | 0.993 |
| generic | array_sum | 1.786 | 1.004 | 0.999 | 1.003 |
| native | while_mix | 12.499 | 0.999 | 0.998 | 1.002 |
| native | for_mix | 12.477 | 1.000 | 0.999 | 0.623 |
| native | tail_mix | 15.045 | 1.201 | 1.201 | 1.207 |
| native | match_dispatch | 2.502 | 0.612 | 0.615 | 0.995 |
| native | array_sum | 1.771 | 1.002 | 0.998 | 1.002 |

この測定条件下では、**`for` ループで Rust より約 1.60 倍高速であり、整数の `match` ディスパッチで C/C++ より約 1.63 倍高速**という結果が得られました。
一方で、while ループや配列走査など、時間比がほぼ 1.0 にとどまる種目について一方的な勝利とみなすことは避けています。
**この測定時点では、match を経由する末尾再帰ミキサーが比較相手に対して約 20% 遅延しており**、すべての制御フローにおいて Tsuzuri が上回っていたわけではありません。
（この末尾再帰に関する後続の改善内容は次節に詳述します）。速度向上のために符号付き整数のオーバーフローを未定義動作へ緩和するような妥協は行っていません。
また、Rust と Clang では内包する LLVM のバージョンおよび最適化パイプラインが異なる点にも留意する必要があります。

生成されたアセンブリコードの精査により、`for` ループにおける反復ごとの狭幅整数拡張が消去され、整数 `match` が静的ジャンプテーブルと展開されたスカラーループへ変換され、配列集約処理が NEON の `ldp q`／`add.2d`／`addp` 命令へと自動ベクトル化されていることを確認しました。
なお、match 内の小入力専用ベクトル経路は大入力では実行されないため、その命令の存在のみをもってこの match 測定が SIMD で加速されたと主張することはありません。
GPU 実行、自動並列化、実アプリケーション全体の性能、あるいは他アーキテクチャでの優位性を短絡的に示すものではない点に留意してください。

生データおよびコード生成物は `target/benchmarks/control-final-{generic,native}-{1,2,3}.json` および同名ディレクトリに保存されています。
測定に使用したコンパイラバイナリの SHA-256 は `94291a712871f12137d8227a7cc52e9772feaacd72245f0750e58fb0fa4b710a` です。
共有 CI には恣意的な速度判定閾値を設けていません。

古い `--baseline` コンパイラを用いて既存のコンピュテーション式比較を実行する場合、テストハーネスは小規模ソースを用いて `rec` 構文のサポート状況を自動検出します。
未対応の古いベースラインに対しては、一時コピーから再帰指定の記述のみを除去して同一アルゴリズムを引き渡します。
この実行モードは `baseline_rec_syntax: "legacy-rec-erased"` として JSON に記録され、予期された構文エラー以外のツールエラーは通常の失敗として扱われます（現行ソースの再帰検査ルールを無効化する機能ではありません）。

### 末尾再帰の改善（2026-09-23）

末尾呼び出しのループ化自体はすでに実装されていたため、`while` ループへの単純な構文置換ではなく、元の計算内容、入力値、および終了条件を完全に維持した 8 通りの LLVM IR 構造を比較検討しました。
条件分岐やブロック配置の変更、非ゼロの `llvm.assume`、誘導変数の 64-bit 拡張、あるいは正負経路の事前分離といった試みだけでは、この整数ミキサーにおける遅れを解消できませんでした。
詳細な分析の結果、「明示的なループ末尾の比較」と「引数の整数演算の遅延配置」の 2 つの手法が、ともに C や Rust と同等の性能を達成することが判明しました。
前者はカウンタ認識や停止条件の特殊パターン認識、およびループ本体のコード複製を要するため、コンパイラ構造を肥大化させずシンプルに適用可能な後者の「遅延配置」を採用しました。

ネイティブ環境における自己末尾呼び出しにおいて、整数の `+` および `-` 演算の**被演算子を元の評価順序で確定**させたうえで、トラップの発生しない折り返し加減算命令のみを「全引数の評価完了後」の更新ステップへと遅延配置します。
メモリのロード、関数呼び出し、可変変数の更新、トラップ判定、および一時所有値の解放順序は元のまま維持されるため、後続引数の評価によって可変ローカル変数が書き換えられた場合でも、先行して確定した SSA 値が正しく使用されます。
非負制約の追加、オーバーフローフラグの改変、または浮動小数点演算の再結合などは一切行っていません。
関数本体が 1 つの演算のみで構成される既知の関数呼び出しも認識され、`Sub.sub` と `-` 演算子は同一の最適化経路をたどります。
なお、この最適化は非末尾再帰や相互再帰のアルゴリズムを変更するものではありません。

同一の Apple M1 Max、Apple Clang 21、Rust 1.98.1 環境下で、変更前後のコンパイラと C/C++/Rust を**同一プロセス内**にリンクし、generic および native 設定で独立に各 3 回測定しました。
1 回の試行は各実装 2 回のウォームアップ、15 サンプル（各サンプルは 2 回の平均値）で構成されます。時間および比率はそれぞれ 3 回の中央値です。
**処理時間（ms）および時間比は数値が小さいほど有利であり、改善倍率は数値が大きいほど良好です。**
時間比は「変更後 Tsuzuri の時間 / 比較相手の時間」、改善倍率は「変更前の時間 / 変更後の時間」を示します。

| CPU | 末尾再帰の形式 | Tsuzuri 変更後 (ms) | Tsuzuri 変更前 (ms) | 改善倍率（倍） | Tsuzuri / C（時間比） | Tsuzuri / C++（時間比） | Tsuzuri / Rust（時間比） |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| generic | match / `-` | 12.437 | 14.937 | 1.201 | 1.001 | 1.000 | 1.001 |
| generic | if / `-` | 12.416 | 14.979 | 1.205 | 0.999 | 0.999 | 0.996 |
| generic | match / `Sub.sub` | 12.521 | 14.990 | 1.197 | 0.993 | 0.999 | 0.995 |
| native | match / `-` | 12.441 | 14.912 | 1.199 | 1.000 | 0.998 | 1.001 |
| native | if / `-` | 12.461 | 14.972 | 1.196 | 1.002 | 1.003 | 1.001 |
| native | match / `Sub.sub` | 12.433 | 14.929 | 1.200 | 1.000 | 0.999 | 1.001 |

**約 1.20 倍、実経過時間において約 17% の短縮を達成し、以前の約 20% の遅れを完全に解消しました。**
C/C++/Rust を大幅に凌駕したわけではなく、このミキサー処理において完全に同等水準に並んだという客観的な結果です。
生成アセンブリの解析では、状態更新における `eor → madd → add` の直列データ依存チェーンが、カウンタ側の加算を並行して独立実行する `eor → madd` の構造へと最適化されていることを確認しました。
while、for、整数 match、配列走査の変更前後比較においても、性能退行がないことを確認しています。
既存の C++ 比較（3 種目）およびネイティブのコンピュテーション式比較においても、有意な性能低下は観測されていません。

**この命令遅延配置を WebAssembly（WASM）環境へ一律適用する案は採用を見送りました。**
Node.js／V8 の JIT コンパイラ環境においては末尾ミキサーに対する効果がほとんど認められず、長時間ウォームアップを行ったコンピュテーション式の delayed 種目において、0.350 ms → 0.371 ms（約 6% の性能低下）の再現が 2 回の測定で確認されたためです。
最終的な実装では、WASM 向けには従来のスタック指向の引数生成順序を維持しており、コンピュテーション式比較の WASM バイナリ全体が変更前とバイト単位で一致します。
CPU 時間と WASM の実経過時間を不当に混同したり、ネイティブ環境での改善倍率を WASM にもそのまま適用できると主張したりすることはありません。

`tests/tail_recursion.rs` および control 系列の native／WASM `-O0`／`-O3` テストスイートでは、負数、8／64／128-bit 整数の折り返し、可変値の読み出し、左右オペランドの副作用、関数内アサート、IEEE 754 加算順序・NaN・符号付きゼロ、ならびに所有値および一時配列の安全なメモリ解放を検証します。
AddressSanitizer（ASan）環境下でも同一の所有権管理パスを検証し、借用参照や非末尾呼び出しにおける健全な通常経路が維持されていることを確認しました。

再測定を行う際は、コミット `b3f658e` を別作業ディレクトリでビルドした変更前バイナリを準備します。本測定では `target/benchmarks/tsuzuri-recursion-baseline-b3f658e` を使用しました。

```sh
for cpu in generic native; do
  for trial in 1 2 3; do
    node benchmarks/run-control.mjs target/release/tsuzuri --cpu "$cpu" \
      --baseline target/benchmarks/tsuzuri-recursion-baseline-b3f658e \
      --artifacts "target/benchmarks/recursion-final-$cpu-$trial" \
      > "target/benchmarks/recursion-final-$cpu-$trial.json" || exit 1
  done
done
```

コンパイラの SHA-256 は、変更前が `94291a712871f12137d8227a7cc52e9772feaacd72245f0750e58fb0fa4b710a`、
変更後が `66cf308bce9c93498ca93bf27faf7c59217af607d47acd7db98a42894d31187a` です。
生データ、生成 IR、およびアセンブリコードは上記の各 JSON および同名ディレクトリに保存されています。
速度に関する合否判定閾値は追加しておらず、異なる CPU、LLVM バージョン、実行エンジン環境における優位性については別途継続的な計測が必要です。

### 境界検査の残り方（F12）

チケット F12 の最適化では、コンパイラが型付き IR 上で「添字が必ず配列の有効範囲内に収まる」と論理的に証明できた配列読み出しに限り、境界検査の条件分岐コードの生成を省略します（`src/ranges.rs` の閉じた規則 R1–R5 および P1–P3）。証明できない箇所の検査、`TrapKind::BoundsCheck`、およびトラップの位置情報は変更前と完全に同一です。
メモリアクセス用の `getelementptr inbounds` および `load` はそのまま出力され、`llvm.assume`、`!range`、およびオーバーフロー抑止フラグを不当に生成することはありません。
この静的判定は LLVM 最適化よりも前の段階で行われるため、`-O0` 最適化の IR およびトラップ情報テーブルに直接反映され、ネイティブと WASM の双方で完全に一致する挙動となります。

`tests/fixtures/bounds_checks` の各テスト関数について、`-O0` 最適化の IR（`tsuzuri build --emit llvm` の出力）に残存する `call void @llvm.trap()` 命令の個数を計数しました（なお、このカウントには `new [i64](count, ...)` の確保サイズ検査など、境界検査以外のトラップ命令も含まれます）。
**数値が小さいほど境界検査が省略されていることを示し、変更前と同一の数値であれば検査が安全に残存していることを意味します。**

| 関数 | 添字の形 | `-O0` の trap 変更前 | `-O0` の trap 変更後 |
| --- | --- | ---: | ---: |
| `forward` | `0 .. values.length - 1` のループ | 2 | 1 |
| `backward` | `values.length - 1 .. -1 .. 0` のループ | 2 | 1 |
| `sum_all`（`builtin_length` が呼ぶ） | `0 .. Array.length values - 1` のループ | 1 | 0 |
| `pick` | リテラル配列の `values[2]` と、範囲の `if` の then 側 | 2 | 0 |
| `builtin_length` | 確保と `sum_all` の呼び出しだけ | 1 | 1 |
| `replaced` | ループの中で配列を置き換える | 3 | 3 |
| `other` | 別の配列の長さで回る | 3 | 3 |
| `past_end` | `0 .. values.length`（上限が長さそのもの） | 2 | 2 |
| `while_sum` | `while` と可変の添字 | 2 | 2 |
| `literal_past` | リテラル配列の `values[3]` | 1 | 1 |

検査数が減少した 4 つの関数において、残存した各 1 件はメモリ確保サイズの検査です（`sum_all` と `pick` では 0 件となりました）。
一方、`-O3` 最適化下においては、LLVM が変更前の段階から単純なループの境界検査を自動で消去していたため、エクスポート関数に残存する `llvm.trap` の数は 9 関数すべてで変更前後で完全に一致しました
（`forward`: 2、`backward`: 2、`builtin_length`: 2、`pick`: 0、`replaced`: 4、`other`: 2、`past_end`: 1、`while_sum`: 2、`literal_past`: 1）。
`-O0` および `-O3` におけるトラップ数は `target/perf/F12/shapes-{before,after}.{txt,O0.txt}` に保存されています。
`tests/bounds_checks.rs` は、省略対象となる 7 形式のトラップ情報テーブルから境界検査が消去されること、および検査を残すべき 6 形式の位置情報文字列が変更前と完全に一致することをテストで固定しています。

### 境界検査の省略の実測（2026-10-02）

測定環境: Apple M1 Max（10 logical CPUs）、arm64 macOS Darwin 27.0.0、Apple Clang 21.0.0（clang-2100.3.34.2）、
Rust 1.98.1／LLVM 22.1.8、Node.js v20.17.0、`-O3 --cpu generic`。
F12 最適化を適用する直前（HEAD `30b2d1d`）のコンパイラを `--baseline` に指定し、C および Rust と同一プロセス内で 3 回測定を実施しました。
1 回の試行は各実装 2 回のウォームアップと 15 サンプルで構成されます。表内の処理時間は 3 試行の中央値の中央値、改善倍率は「変更前の処理時間 / 変更後の処理時間」の中央値、範囲は 3 試行の最小値と最大値です。
**処理時間は数値が小さいほど、改善倍率は数値が大きいほど有利です（1.000 付近は有意差なし）。**

| 種目 | 変更後 (ms) | 変更前 (ms) | 改善倍率 | 3 回の範囲 | C (ms) | Rust (ms) |
| --- | ---: | ---: | ---: | --- | ---: | ---: |
| while_mix | 12.684 | 12.656 | 0.998 | 0.996–0.998 | 12.621 | 12.654 |
| for_mix | 12.688 | 12.639 | 0.998 | 0.996–0.999 | 12.668 | 20.238 |
| tail_mix | 12.638 | 12.640 | 1.001 | 1.000–1.002 | 12.659 | 12.639 |
| tail_if_mix | 12.616 | 12.623 | 0.999 | 0.999–1.001 | 12.671 | 12.634 |
| tail_builtin_mix | 12.631 | 12.604 | 0.999 | 0.997–1.004 | 12.630 | 12.601 |
| match_dispatch | 2.523 | 2.523 | 0.996 | 0.996–1.000 | 4.109 | 2.545 |
| array_sum | 1.791 | 1.791 | 1.000 | 1.000–1.007 | 1.788 | 1.786 |
| **array_index_sum** | 1.786 | 1.788 | 0.998 | 0.995–1.001 | 1.784 | 1.782 |
| array_copy | 1.508 | 1.509 | 1.007 | 0.999–1.020 | 1.514 | 1.539 |
| list_sum | 1.769 | 1.770 | 1.001 | 0.999–1.007 | 1.806 | 1.806 |
| closure_capture | 1.898 | 1.902 | 1.002 | 0.993–1.005 | 1.901 | 1.897 |
| closure_churn | 6.354 | 6.359 | 1.001 | 0.998–1.004 | 6.306 | 9.032 |
| record_pipeline | 12.690 | 12.655 | 0.999 | 0.996–1.003 | 12.662 | 12.646 |
| integer128_mix | 4.994 | 4.988 | 0.998 | 0.996–1.001 | 4.979 | 5.309 |
| float32_mix | 8.862 | 8.851 | 0.999 | 0.999–1.006 | 8.854 | 8.840 |
| float64_mix | 8.847 | 8.868 | 1.002 | 0.998–1.005 | 8.849 | 8.857 |

**この測定環境において、F12 による境界検査の省略は `-O3` 最適化下の実行時間を実質的に変化させません。**
静的に検査が消去された `array_index_sum` においても、変更前の 1.788 ms に対し変更後が 1.786 ms であり、改善倍率の 3 回の範囲（0.995〜1.001）は 1.000 を跨いでいます。これは LLVM が変更前の段階から `0 .. values.length - 1` のループ検査を自律的に除去して自動ベクトル化を行っていたためであり、C 言語（1.784 ms）や Rust（1.782 ms）と完全に同等水準のまま推移しています。
他の 15 種目についても生成 IR は変更前と同一であり（差分は `array_index_sum` の本体およびループメタデータの 2 行のみ）、全 16 種目の 3 試行における改善倍率は 0.993〜1.020 の誤差範囲内に収まりました。したがって、この僅かな変動幅をもって F12 による効果とは解釈せず、**実行速度の劇的な改善は主張しません。**
F12 の本質的な効果は、`-O0` 最適化の IR およびトラップ情報テーブルから不要な検査が安全に消去される点にあります（上掲の表を参照）。

なお、初期の試作実装では `array_index_sum` が著しく低速化する現象が発生しました。ループ展開ヒントを抑制しなかったバージョン（SHA-256: `61b15a68…`）での同条件測定では、変更前 1.802 ms に対し変更後が 2.371 ms（改善倍率 0.761）と、**約 32% の大幅な性能低下**を招きました（他の 15 種目は 0.990〜1.009 で変化なし）。
原因を精査したところ、`hint_loop` が小規模な整数リダクションループに対して付与していた `llvm.loop.unroll.enable` ヒントが影響していました。
境界検査が残存しているループではこのヒントの有無にかかわらず `-O3` のコード生成結果は変わりませんでしたが、検査が消去されたループにおいては、LLVM が `LoopVectorize` パスの実行よりも前に 8 回の実行時ループ展開（runtime unroll）を先行して実行してしまい、その結果として効率的な 128-bit ベクトル命令（`ldp q` 4 本）ではなく、1 要素ずつロードする非効率な `ld1.d`（8 命令）のループへと劣化してしまいました（生成アセンブリ行数も 715 行から 753 行へ増大）。ヒントを除去することで変更前と同一の最適なアセンブリが復元されることを `clang -O3` で確認しました。
これを受け、`hint_loop` は「境界検査を省略したメモリアクセスを本体に含むループ」に対してはアンロールヒントを付与しないよう制御ロジックを修正しました。
検査が残存する通常のループや、配列を走査しないリダクションループ（`match_dispatch` など）に対するヒント付与規則は変更していません。
`tests/bounds_checks.rs` の `unroll_hint_is_dropped_only_where_a_guard_was_removed` がこの最適化境界を厳密にテストしています。
**境界検査の省略のような「制御フロー上の分岐を除去する」変更は、LLVM の最適化パスの適用順序や判断を意図せず狂わせるリスクを孕むため、ループ構造に関わる変更は必ず `--baseline` を用いて実測し、生成アセンブリまで確認して検証してください。**

再測定のコマンド例:

```sh
cargo build --release --locked
for trial in 1 2 3; do
  node benchmarks/run-control.mjs target/release/tsuzuri \
    --baseline target/perf/F12/tsuzuri-before \
    --artifacts "target/perf/F12/control-$trial" > "target/perf/F12/control-$trial.json" || exit 1
done
```

変更前のコンパイラは、F12 の着手時点（HEAD `30b2d1d`）をビルドして `target/perf/F12/tsuzuri-before` に保存しました。
SHA-256 は、変更前が `722dab3ccfeccc85c256f746301f4a6d766b13c460b01e57d71bb543b4f5d725`、
上記表を測定した修正後バイナリが `61274fa13e39e860da50e66449d2f7e8259cd8b48d54ef07db56529d74a8bf6f`、
展開ヒントを抑制しなかった試作版が `61b15a68764101f60cf64a309dceb6b19d123c0d4595739dc0d902c16dd7997e` です。
生データ、生成 IR、およびアセンブリは、修正後が `target/perf/F12/control-{1,2,3}.json` と同名ディレクトリ、試作版が `target/perf/F12/control-prefix-{1,2,3}.json` と同名ディレクトリに保管されています。
本測定は Apple M1 Max（arm64 macOS）上でのみ実施しており、x86_64 Linux 環境における測定は未実施です（推測による値は記載しません）。速度に関する合否判定閾値は CI に追加していません。

### 境界検査の Phase 2 の形（F12）

チケット F12 の Phase 2 として検討されていた候補形状（可変上限ループのマルチバージョニング版分け、先行する境界事前検査、`while` ループの帰納変数解析）を実際にコンパイラへ実装すべきかどうかを、実機測定の結果に基づいて判断しました。
`benchmarks/bounds_shapes/Main.tz` に用意した 4 つのテスト関数は、同一の配列合計を算出する異なる構文形式です。

| 関数 | 構文の形式 | F12 Phase 1 の証明 |
| --- | --- | --- |
| `sum_all` | `for i in 0 .. values.length - 1` | あり（規則 R1。検査が消去される） |
| `sum_first` | `for i in 0 .. n - 1`（`n` は関数の引数。長さとの関係は局所コードに現れない） | なし |
| `sum_checked` | `if n <= values.length then for i in 0 .. n - 1` | なし（`n` と長さの比較が添字変数そのものではない） |
| `sum_while` | `let mut i = 0` と `while i < values.length do (…; i = i + 1)` | なし（`while` ループは現在の証明規則の対象外） |

```sh
./target/release/tsuzuri build benchmarks/bounds_shapes --emit header -o m.h
./target/release/tsuzuri build benchmarks/bounds_shapes --emit object -O3 -o m3.o
./target/release/tsuzuri build benchmarks/bounds_shapes --emit object -O0 -o m0.o
clang -O2 -I. benchmarks/bounds_shapes/bench.c m3.o -o bench3 && ./bench3
clang -O2 -I. benchmarks/bounds_shapes/bench.c m0.o -o bench0 && ./bench0
```

テスト配列は 1,048,576 個の `i64` 要素（要素値は `i & 1023`）で構成され、各行は 200 回の呼び出しの合計所要時間を 7 回測定した最小値を示します（Apple M1 Max、Apple Clang 21、`-O3` デフォルト最適化、チェックサムは 4 関数すべてで同一の `750885273600`）。

| 形 | `-O3`（ms） | `sum_all` に対する比 | `-O0`（ms） | `sum_all` に対する比 |
| --- | --- | --- | --- | --- |
| `sum_all`（静的証明あり） | 21.601 | 1.000 | 378.243 | 1.000 |
| `sum_first`（可変上限） | 21.831 | 1.011 | 402.614 | 1.064 |
| `sum_checked`（先行事前検査） | 22.197 | 1.028 | 401.955 | 1.063 |
| `sum_while`（`while` ループ） | 21.767 | 1.008 | 486.269 | 1.286 |

`-O3` 最適化下においては、コンパイラフロントエンドによる静的証明を持たない 3 つの形式であっても、すべて `sum_all` との差が 3% 以内に収まり、4 つの形式すべてが LLVM によって正常に自動ベクトル化されることが確認されました（`clang -O3 -S -emit-llvm` の出力において 4 つすべてに `<N x i64>` ベクトル命令が含まれる）。
これは、LLVM の `IndVarSimplify` 最適化パスが、ループを脱出する境界検査（失敗時の分岐先がトラップ命令）をループ実行前の 1 回の事前比較へと自律的に巻き上げてホイスティングするためです。
したがって、コンパイラフロントエンドが検査省略のためにマルチバージョニング版分けを行うことは、LLVM がすでに行っている最適化をバイナリサイズを倍加させて重複実装するだけにすぎず、**実装しない方針を決定しました**（これは F12 チケットにおける「コードサイズの増大を実測評価してから採否を決定する」という課題に対する明確な結論です）。
同様に、`while` の帰納変数解析や `assert` による支配関係解析についても、`-O3` において完全に同一の性能結果が得られるため実装を見送りました。
なお、`-O0`（Tsuzuri のデフォルトビルドではありません）においては `while` ループが約 29% 低速であり、これを改善できるのは `while` 専用の証明規則のみですが、現在の規則体系でこれを実現するには「ループ変数の増分処理が本体の末尾文であること」を厳密に証明するフロー依存の複雑な解析が必要となり、`-O0` デバッグ時のみの限定的な利益に対してコンパイラ複雑度のコストが見合わないと判断しました。
本測定は Apple M1 Max（arm64 macOS）の 1 台で実施したものであり、x86_64 Linux 環境での測定は未実施です（推測値は記載しません）。

## 動的ディスパッチの比較（A14）

`node benchmarks/run-dyn.mjs target/release/tsuzuri [--quick]` は、同じ図形の面積の和を三つの書き方で求めます。`dyn` は `[dyn Shapes.Shape]` の要素ごとに vtable からメソッドを呼び、`union` は union の `match`、`generic` は型ごとの配列を `Shapes.Shape<'a>` 制約のジェネリック関数（単相化）で合計します。
要素 i は i % 3 で正方形・長方形・三角形のどれかで、各版は 3,000 要素を 2,000 周（600 万回の面積計算）します。配列の構築は各呼び出しに含まれますが、周回に比べて小さい仕事です。
C のホストが `-O3` の object を呼び、実行順を回しながら 9 回測った中央値を出します。チェックサムは三版で一致を確かめます。コードサイズは、その版だけを export した object の `llvm-objdump -d` の出力の大きさです（`TSUZURI_OBJDUMP` で変えられます）。

2026-10-06 に Apple M1 Max（macOS 27.0.1、LLVM 21）で 3 回実行した結果（生データは `target/perf/A14/`）:

| 版 | 中央値（ms、3 回の範囲） | 1 要素あたり | 逆アセンブルの大きさ（bytes） |
| --- | --- | --- | --- |
| `dyn` | 15.34–15.59 | 約 2.6 ns | 10,863 |
| `union` | 3.77–3.85 | 約 0.63 ns | 5,406 |
| `generic` | 2.19–2.26 | 約 0.37 ns | 22,405 |

`dyn` の周回は要素ごとに slot の adapter を間接呼び出しし（object 全体の `blr` は 10 個）、ベクトル化されません。この負荷では union の `match` の約 4 倍、単相化の約 7 倍の時間です。単相化は型ごとの関数とベクトル化したループでコードが最も大きくなります。
`dyn` は格納する型を後から増やせることとコードの大きさの抑制のための選択肢で、速度の改善は主張しません。1 台・1 負荷の測定で、x86_64 と WASM は未測定です。

### integer128_mix の再計測（PX02）

2026-10-14、driver と C／C++ は同じ Apple Clang 21.0.0 で、通常の 2,000,000 反復を 3 run 測りました。
各実装は 12 標本です。括弧は最小〜最大（wall ms／呼び出し）です。
生データは `target/perf/PX02-int128-{a,b,c}/control.jsonl`、IR／assembly は `PX02-int128-artifacts` にあります。

| run | Tsuzuri | C | C++ | Rust |
| --- | --- | --- | --- | --- |
| a | 5.072375（4.9885〜5.2900） | 5.059000（4.9366〜5.2102） | 5.074600（4.9946〜5.1756） | 5.365375（5.30125〜5.54625） |
| b | 5.095625（4.9760〜5.23525） | 5.118625（4.99275〜5.1835） | 5.119750（5.0725〜5.17225） | 5.380125（5.2925〜5.42075） |
| c | 5.021200（4.9402〜9.5120） | 5.015400（4.9162〜5.0762） | 5.030600（4.9544〜6.8494） | 5.353875（5.26575〜13.28925） |

Tsuzuri と C の内側のループは operand も含め、extr・eor・eor・mul・umulh・mul・adds・adc・subs・b.hi の
同じ 10 命令でした。3 run のすべてで Tsuzuri の最小が C の最大を超える条件は満たさないため、
分類は **「計測の揺れ」** です。配置の実験は必要な条件を満たさず行っていません。
大きい外れ値も捨てずに残し、2026-09-26 の異なる Clang の値から性能の後退を推測しません。

## コンパイル時間と規模の上限（G17）

2026-10-08 に Apple M1 Max（10 コア、macOS 27.0.1、rustc 1.98.1、Node v20.19.6）で測りました。ほかのビルドが同時に動いていた機械で、
負荷の平均は 27〜90 でした。時間は 9 回の中央値と最小・最大で、比較する系列は 1 回ずつ交互に走らせています。生データは `target/perf/G17/` です
（`target/` はコミットしません）。速度の合否条件は CI に入れていません。

使ったプロジェクトは次の二つです。

| 名前 | 内容 |
| --- | --- |
| 小さいモジュール 1,000 | `benchmarks/run-cache.mjs` と同じ規則。`Module<i>.tz` は `def value :: i64` と `fn value = <i>` の 2 行で、`Main.tz` が合計する |
| 現実的なモジュール 1,000 | 1 モジュール 138 行（合計 138,002 行）。record、union、`match`、`for`・`while`、ラムダ、文字列への変換と、`Shared.tz` の制約付きジェネリック関数・ジェネリック record の呼び出しを持つ |

### frontend cache（Phase 1）

`base` は変更前のコンパイラ（`1182045`）で、`build` は成果物のキャッシュ（G11）が当たる状態、`check` はキャッシュなしです。`cold` は新しいコンパイラで
`frontend/` を消した直後、`warm` はその次の実行です。出力は 3 系列で同じです。

| プロジェクト | コマンド | base（s） | cold（s） | warm（s） | 最大 RSS（MiB、base → warm） |
| --- | --- | --- | --- | --- | --- |
| 小さいモジュール 1,000 | `build --emit llvm -O0` | 0.52（0.51〜0.61） | 0.53（0.50〜0.67） | 0.52（0.51〜0.77） | 42 → 43 |
| 小さいモジュール 1,000 | `check` | 0.29（0.27〜0.49） | 0.31（0.29〜0.43） | 0.29（0.29〜0.55） | 40 → 42 |
| 現実的なモジュール 1,000 | `build --emit llvm -O0` | 4.18（3.87〜6.09） | 4.46（3.85〜10.73） | 3.98（3.69〜7.01） | 1,034 → 1,042 |
| 現実的なモジュール 1,000 | `check` | 3.38（2.61〜3.86） | 3.09（2.72〜5.23） | 2.92（2.51〜3.66） | 944 → 955 |

構文解析の段だけを計装して測ると、現実的な 1,000 モジュールの構文解析（利用者 1,001 個と std 32 個）は cache なしで約 210 ms、warm で約 120 ms でした。
warm の内訳は、ソースの SHA-256（parse key、3.8 MB）約 22 ms、entry の SHA-256 の検証（9 MB）約 48 ms、復号約 48 ms、パックの読み込み約 3 ms、manifest の
照合約 4〜8 ms です。小さいモジュールでは構文解析そのものが 1 個 10 µs ほどで、warm と cache なしの差は測定のばらつきに収まります（構文解析の段は
cache なし約 10 ms、warm は manifest の照合を含めて約 12〜13 ms）。`examples/hello` の構文解析は 5.3 ms から 3.6 ms になりました。

型検査、特殊化、所有権の検査、IR の生成は毎回すべて行うので、短縮は構文解析の分（現実的な 1,000 モジュールの `check` の約 7%）に限られます。
最初の実装はソース 1 個ごとに entry のファイルを作りましたが、この機械ではコンパイラのプロセスがファイルを 1 個開くのに 0.3〜1.5 ms かかり
（on-access scan。同じ 1,034 ファイルを Node は 35 ms、新しく作った C の実行ファイルは 240〜380 ms で読みます）、warm の `check` が cache なしより
遅くなりました（現実的な 1,000 モジュールで 3.86 s 対 2.44 s）。そのため entry はプロジェクトごとの 1 個のパックにまとめ、warm の解析で開くファイルを
2 個にしています。ファイルを開く費用は環境によって大きく違うので、この数値をほかの機械へそのまま当てはめないでください。

### 型検査の内訳と本体の検査の再利用（Phase 2 の判断）

G17 の Phase 2（変わっていないモジュールの関数本体の型検査を省き、型付きの本体を保存して再利用する）を作るかどうかを、`check_modules_collect`
の段ごとの時間（変更前のコンパイラに一時的な計装を入れたもの）で判断しました。「本体」は関数本体の型検査（`computation::expand` と
`Checker::expression`）、入口の式、`solve_members`、`finish`（型の確定、網羅性、未使用の局所変数の警告）の合計で、Phase 2 が省ける上限です。

| プロジェクト | コマンド | 全体（ms） | 読み込み | 構文解析 | 宣言の収集 | 本体（利用者 + std） | 本体の割合 | 全体の段 | IR の生成 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 現実的なモジュール 200 | `check` | 611 | 10 | 49 | 28 | 131（107 + 17） | 21.4% | 294 | — |
| 現実的なモジュール 200 | `build --emit llvm -O0` | 837 | 15 | 55 | 28 | 134（110 + 18） | 16.0% | 310 | 187 |
| 現実的なモジュール 1,000 | `check` | 3,123 | 593 | 227 | 114 | 569（536 + 17） | 18.2% | 1,356 | — |
| 現実的なモジュール 1,000 | `build --emit llvm -O0` | 4,308 | 401 | 221 | 111 | 572（538 + 18） | 13.3% | 1,457 | 906 |
| `examples/` の 9 個（`desktop`・`gpu`・`native`・`web` を除く） | `check` | 55〜71 | — | 5〜6 | 5〜6 | 15〜21（利用者は 1 未満） | 27〜33% | 20〜26 | — |

「全体の段」は定数の畳み込み、`rec` と未使用の非公開宣言の検査、Copy の推論、特殊化、`closures::lower`、所有権の検査の合計です。現実的な 1,000 モジュールの
`check` では、特殊化 537 ms、Copy の推論 256 ms、所有権 250 ms、`rec` と未使用の検査 219 ms でした。読み込み（ソースのファイルを開いて読む段）は
この機械では 274〜1,329 ms とばらつきます。

1,000 モジュールの `check` で本体の検査を全部省けても 18.2%（解析の段だけを分母にしても 23.9%）で、判断の基準の 25% に届きません。保存した型付きの本体を
読み込む費用もかかるので、実際の短縮はさらに小さくなります。そのため Phase 2 は作らないことにしました（F12 の Phase 2 と同じ扱い）。小さいプログラムでは
std の本体の検査が `check` の約 3 割（15〜21 ms）を占めますが、時間そのものが小さく、std の検査結果をプロセスの間で持つのは常駐サーバー（PB06）の仕事です。
将来実装する場合に必要なものは G17 のチケットの D10 にあります。

### 規模の上限（Phase 3）

1 ファイルの大きさと特殊化の数を変えて、`check` と `build --emit llvm -O0` の時間と最大 RSS を測りました（9 回の中央値と最小・最大、キャッシュなし）。
1 ファイルのソースは、現実的なモジュールの本文を名前に番号を付けて繰り返し、`Main.tz` の入口から全部を呼ぶものです。16 MiB は新しい上限を超えるので、
上限だけを上げた計測用のコンパイラで測りました。特殊化は、`Shared.tz` の `n` 個のジェネリック関数（ループを持つ 6 行の本体）を `m` 個のモジュールの
record で呼び、`m × n` 個の特殊化を作ります。

| ソース | 特殊化 | `check`（s） | 最大 RSS（MiB） | `build --emit llvm -O0`（s） | 最大 RSS（MiB） |
| --- | --- | --- | --- | --- | --- |
| 1 ファイル 1 MiB（1,041,455 bytes） | — | 0.52（0.51〜0.77） | 267 | 0.75（0.71〜0.83） | 284 |
| 1 ファイル 4 MiB（4,189,031 bytes） | — | 2.06（1.98〜2.83） | 983 | 2.74（2.73〜2.99） | 1,034 |
| 1 ファイル 16 MiB（16,773,841 bytes） | — | 9.31（8.23〜10.57） | 3,750 | 12.30（11.14〜16.35） | 3,948 |
| 31 モジュール × 32 関数 | 992 | 0.08（0.08〜0.19） | 45 | 0.11（0.11〜0.22） | 49 |
| 64 × 64 | 4,096 | 0.19（0.19〜0.26） | 107 | 0.30（0.29〜0.31） | 115 |
| 128 × 128 | 16,384 | 0.72（0.63〜1.28） | 343 | 1.05（1.04〜1.10） | 369 |
| 256 × 256 | 65,536 | 2.49（2.34〜4.02） | 1,278 | 4.56（3.99〜7.51） | 1,382 |
| 16 段の連鎖（各段が次を 2 つの大きな型で呼ぶ） | 65,535 | 2.99（2.88〜3.20） | 899 | — | — |
| 現実的なモジュール 1,000（ジェネリック関数をモジュールごとに持つ） | 約 4,000 | 2.55（2.28〜13.85） | 998 | 4.09（3.56〜14.91） | 1,051 |

時間と RSS はソースの大きさと特殊化の数にほぼ比例し、1 MiB あたり約 250 MiB、特殊化 1 件あたり約 20 KiB でした。最後の行のプロジェクトは、変更前の
特殊化の上限（1,024）を超えるので変更前のコンパイラでは `E1017` になりました。

新しい上限は次の基準で選びました。上限ちょうどの入力が、この機械で `check` を約 2.5 s・約 1.3 GiB 以内、IR の出力までを約 5 s 以内で終えることです
（現実的な 1,000 モジュールのプロジェクトと同じ規模）。1 ファイルの上限は 1 MiB から 4 MiB に（16 MiB は約 4 GiB を使うので選ばない）、特殊化の上限は
1,024 から 65,536 にしました。型が大きくなり続ける多相再帰は全体の上限より先に止めます。型引数を包んだ型で自分自身を直接呼ぶ関数は最初の特殊化で拒否し
（`fn rec f x = f [x]` も、`f (ref x, 1)` と `f (ref x, true)` の分岐も 0.07 s）、ほかの関数を経て大きくなる再帰は、呼び出しの連鎖に同じ関数がより小さい型で
32 回を超えて現れたときに止めます（0.08 s）。ほかの関数を経て分岐しながら大きくなる再帰だけは全体の上限まで進み（2.6 s）、そのメッセージが大きくなり続ける
関数を示します。インスタンスの選択で 1 段で終わる成長は、1,025 種類の型で起きても約 4,100 件の特殊化として受け付けます。
構文の深さ 128（`MAX_NESTING`）と型の深さ 128・構成要素 4,096 は、再帰する処理の stack を守る上限なので変えていません（debug ビルドの 2 MiB の stack で、
構文解析は深さ 128 の入力に 1.5 MiB 以上を使います）。

## REPL の 1 入力の待ち時間（G13）

`tsuzuri repl` は JIT を持たず、入力ごとに生成した `Main.tz` を検査し、`run` と同じ経路で実行ファイルを作って実行します。1 入力の待ち時間とその内訳は、次のコマンドで測ります。

```sh
cargo build --release --locked
node benchmarks/run-repl.mjs target/release/tsuzuri [--samples 9] [--out target/perf/<run_id>]
```

各行は 9 回（`--samples`）の中央値と最小・最大（ms）で、先頭の 1 回は捨てます。速度の合否の閾値はありません。前半は、空のセッションで式 `1 + N` を入力したときに REPL が作るプログラム（`let it = (1 + N)` と `Display.display (ref it)`）を CLI で段ごとに測ります。Clang の時間は、`TSUZURI_CLANG` に渡した計時用の小さな C のラッパーがビルドの中の呼び出しごとに記録し、ラッパーが残した IR で「`clang -c`」と「リンク」を同じフラグで別々に再現します。後半は 1 つの REPL セッションの中で、入力を書いてから結果の 1 行を読むまでを測ります。`--out` を付けると、要約と全標本を JSON Lines で書きます（`target/` はコミットしません）。

2026-10-08 の計測: Apple M1 Max（10 コア）、macOS（darwin 27.0.0）、Homebrew clang 21.1.8（PATH の先頭）、Node v20.19.6、release ビルドのコンパイラ。ほかの作業の cargo のビルドとテストが同時に動き、load average は 30〜58 でした。数十 ms の差は、下の交互の計測でだけ比べます。生データは `target/perf/G13-before/`（Phase 2 の `f310d3e`）と `target/perf/G13-after/`（Phase 3）です。

| 測ったもの（Phase 3） | `-O0` | `-O3` |
| --- | --- | --- |
| `tsuzuri --version`（プロセスの起動） | 4（4–25） | 4（3–20） |
| `tsuzuri check`（起動、読み込み、解析） | 48（47–57） | 49（46–80） |
| `build --emit llvm --no-cache`（上に IR の生成） | 50（49–123） | 52（48–142） |
| `build --no-cache`（実行ファイル。cache なし） | 317（302–357） | 860（816–1145） |
| そのビルドの中の Clang（IR のコンパイルとリンク、1 回の呼び出し） | 220（212–245） | 793（748–1035） |
| 再現: IR の `clang -c` | 100（98–112） | 651（627–761） |
| 再現: リンク | 143（111–159） | 146（128–207） |
| `build`、cache あり、miss（鍵の計算と保存を足す） | 418（382–545） | 1011（930–1144） |
| `build`、whole-build cache の hit | 180（165–249） | 198（163–247） |
| その中の `clang --version`（cache の鍵） | 48（44–78） | 47（44–89） |
| プログラムの実行、新しいファイルの初回 | 220（207–241） | 209（206–231） |
| プログラムの実行、2 回目 | 3（2–4） | 3（2–21） |
| 研究用: 同じ IR の `lli`（JIT でコンパイルして実行。リンクも新しいファイルもない） | 272（267–466） | 285（268–457） |

| REPL の入力（セッションの中） | `-O0` Phase 2 | `-O0` Phase 3 | `-O3` Phase 2 | `-O3` Phase 3 |
| --- | --- | --- | --- | --- |
| `:type 1 + N`（解析だけ） | 46（43–145） | 44（42–44） | 44（42–46） | 44（43–45） |
| 新しい式 `N + 1`（cache の miss） | 680（652–863） | 671（602–726） | 1226（1211–1472） | 1256（1220–1604） |
| 同じ式 `1 + 1` の繰り返し（cache の hit） | 281（256–464） | 278（218–509） | 262（259–394） | 308（224–341） |
| 新しい `let v = N`（miss） | 563（538–587） | 546（541–561） | 383（374–412） | 451（401–938） |
| 新しいアクション `do! IO.write_line "N"`（miss） | 658（638–870） | 660（638–761） | 703（686–765） | 907（692–1221） |
| `printf '1 + 1\n' \| tsuzuri repl --no-cache`（プロセス全体） | 385（376–514） | 344（329–377） | 1088（940–1820） | 907（864–1452） |

`-O0` の新しい式（約 0.67 秒）の内訳は、解析が 1 回約 44 ms（Phase 2 は 2 回）、cache の鍵と保存が約 100〜115 ms（`clang --version` の起動が約 47 ms、7.4 MB のコンパイラ自身の SHA-256 と保存が約 69 ms。`--emit llvm` の cache の有無の差で測った）、Clang の IR のコンパイルとリンクが約 220 ms（分けると約 100 ms と約 143 ms）、新しい実行ファイルの初回の起動が約 210〜220 ms、プログラム自体が約 3 ms です。初回の起動の時間は、macOS が新しいファイルを初めて実行するときの検査で、同じファイルの 2 回目は約 3 ms、既に実行した内容の別のファイルは約 70 ms でした（Linux では測っていません）。`-O3` では IR の `clang -c` が約 650 ms になり、その大半は `Display` が連結する埋め込みの数値ランタイムのコンパイルです（PB01 の計測と同じ傾向）。`let` とアクションは `Display` を使わないので、`-O3` でも小さくなります。

Phase 3 では、値を表示するプログラムを先に検査し、`Display` のない式のときだけ型を得るためのプログラムを検査し直すようにして、表示できる式の解析を 2 回から 1 回にしました（`src/repl.rs` の `Repl::expression`。結果は Phase 2 と同じ）。上の表の中央値は負荷の揺れに埋もれるので、2 つのコンパイラの REPL を同じ時間帯に交互に 15 回ずつ測りました（`-O0`、cache あり）。新しい式は 654 → 619 ms（中央値の差 35 ms）、cache の hit する式は 278 → 230 ms（48 ms）、`Display` のない関数値の式（どちらも 2 回解析する）は 595 → 574 ms で、差は 1 回の解析（`:type` の約 44 ms）とおおむね一致します。

残りの大きな部分の担当: 埋め込みのランタイムのコンパイルは PB01（ランタイムの事前ビルド）、`-O0` の Clang のコード生成は PB05（デバッグビルド用の高速バックエンド）、解析と cache の鍵の計算（ツールの `--version` とコンパイラの SHA-256 を毎回繰り返す）はプロセスをまたいで状態を保つ PB06（常駐ビルドサーバー）と G17（解析の cache）の範囲です。新しい実行ファイルの初回の起動の検査は、入力ごとに実行ファイルを作る限りなくならず、どのチケットの範囲でもありません。`lli` の行は JIT の代替案の調査（G13 D13）のための値で、REPL は使いません（`lli` は配布物に含まれません）。`lli` の既定は関数をまとめてコンパイルする ORC で、その `-O` はコード生成の水準だけを変え、Clang の `-O3` のような IR の最適化パスを通さないので、`-O3` の行は Clang の `-O3` と比べられません。呼ばれた関数だけをコンパイルする `--jit-kind=orc-lazy` は、同じ IR（埋め込みのランタイムを含めて 557 KB）で 120〜131 ms でした（5 回の中央値）。JIT を採らない理由と代替案はチケットの D13 にあります。

## 行列積（C11）

`node benchmarks/run-matrix.mjs target/release/tsuzuri [--quick] [--fused-all]` は、`Matrix` の積（`benchmarks/matrix/Main.tz`）を素朴な C の行列積と同じ条件で比べ、同じプログラムを wasm32 でも Node で実行します。`n × n` の入力 2 つ（式は両側で同じ）から `repeats` 回積を作り、積ごとに checksum（`f64` と `f32` は `total * 0.999 + x` を左から右に畳み込んだ値、`i64` は `total * 31 + x`）を足し合わせます。C 側は `-O3 -std=c11 -ffp-contract=off -fno-lto` でコンパイルし、Tsuzuri 側は `build --emit object -O3` の object を、ホストと `src/runtime/task.c` と同じ実行ファイルにリンクします。

比べる実装は次のとおりです。

| 型 | Tsuzuri | C |
| --- | --- | --- |
| `f64`（別々に丸める和） | `Matrix.mul`、i-j-k のループ（Phase 1 の `mul` と同じ順序を公開 API だけで書いたもの）、`Matrix.mul_parallel` | i-j-k、i-k-j、i-k-j を pthread で行ごとに分けたもの（オンライン CPU 数のスレッドを積ごとに作って join する） |
| `f64`（融合） | `Matrix.mul_fma`、`Matrix.mul_fma_parallel` | `fma()` を使う i-k-j |
| `f32`、`i64` | `Matrix.mul`、`Matrix.mul_parallel` | i-j-k、i-k-j |

どの出力要素も同じ順序の演算列なので、同じ型の別々に丸める実装の checksum は 1 ビットの違いもなく一致します（C の i-j-k を基準にします）。融合する実装は、C の `fma()` の版と一致します。runner は計時の前に、各 `n` で全実装のビットの一致を検査し、違えば失敗します（下の `not timed` の実装は、その `n` では検査も計時もしません）。WASM では、計時の前に wasm32 の結果のビットが native の C の基準と一致することも検査します（既定と simd128 の両方）。この入力は積も和もちょうど表現できる値（2^-5 の倍数）なので、融合する版としない版の checksum は同じビットになります。2 つの順序の違いは、このベンチマークではなく、`tests/matrix.rs` と suite `matrix` の probe が検査します。
各実装を実行順を回しながら 9 回ずつ測り、1 回の積あたりの中央値・最小・最大（ms）を出します。速度の合否の閾値はなく、`--quick`（`n = 16`）は動作確認だけで性能の証拠ではありません。

`Math.fma` は、AArch64 向けにビルドしたコンパイラの native の `f32`・`f64` だけがハードウェアの命令で、それ以外の native と wasm32 では、ソフトウェアのルーチン `tz_soft_fma` を積和ごとに呼びます（約 1.6 µs。下の測定）。runner は、`--emit llvm -O3` の IR に `call void @tz_soft_fma(` があるかどうかで、native がソフトウェアのルーチンかを判定し、注意書きを 1 行出します。ソフトウェアのルーチンのとき、Tsuzuri の `mul_fma`・`mul_fma_parallel` は、1 サンプルあたり積 1 回で、`n = 64` までしか測らず、それより大きい `n` は `not timed` と表示します。`n = 512` の積 1 回が分単位になるからです。`--fused-all` を付けると、すべての `n` を測ります（1 積和 1.6 µs とすると、native だけで 1 時間を超える見積もりです。この全体の実行はしていません）。wasm32 は常にソフトウェアのルーチンなので、`mul_fma`・`mul_fma_parallel` を `n = 64` だけ、1 サンプルあたり積 1 回、3 サンプルで測ります（`--fused-all` で `n = 128` と `256` も）。

2026-10-10 に Apple M1 Max（10 コア。性能 8・効率 2）、macOS（darwin 27.0.0）、Apple clang 21.0.0、Node v20.19.6、release ビルドのコンパイラ（`96d7cbf` の上の C11 Phase 2）で 3 回実行した結果です（生データは `target/perf/C11-phase2/`）。ほかの作業のビルドとテストが同時に動き、1 分間の load average は各回の前後で 8.0→6.0、11.2→27.6、26.1→27.2 でした。2 回目は特に並列の版が揺れています。表は、1 回の積の中央値から求めた GFLOP/s（`2n³` を中央値で割った値。積と和 1 組を 2 回と数える）の、3 回分（1 回目 / 2 回目 / 3 回目）です。

**native（arm64、`-O3`）、`f64`**

| 実装 | `n = 64` | `n = 256` | `n = 512` |
| --- | --- | --- | --- |
| Tsuzuri `Matrix.mul` | 11.1 / 9.11 / 10.7 | 11.6 / 11.3 / 11.5 | 11.9 / 11.6 / 11.8 |
| Tsuzuri i-j-k のループ（Phase 1 の順序） | 2.80 / 2.24 / 2.78 | 2.22 / 2.13 / 2.16 | 1.99 / 1.78 / 1.90 |
| Tsuzuri `Matrix.mul_parallel` | 11.2 / 7.44 / 11.2 | 46.6 / 14.8 / 44.8 | 58.0 / 49.7 / 50.6 |
| C i-j-k | 4.06 / 3.41 / 4.06 | 2.56 / 2.34 / 2.55 | 2.11 / 1.73 / 2.03 |
| C i-k-j | 11.6 / 10.0 / 11.6 | 11.8 / 11.0 / 11.7 | 12.2 / 11.9 / 12.2 |
| C i-k-j、pthread | 3.53 / 1.11 / 3.36 | 40.8 / 17.8 / 40.9 | 50.7 / 51.3 / 45.8 |
| Tsuzuri `Matrix.mul_fma` | 11.2 / 10.7 / 11.4 | 11.8 / 11.1 / 11.8 | 11.8 / 11.9 / 11.7 |
| Tsuzuri `Matrix.mul_fma_parallel` | 11.3 / 8.56 / 11.4 | 45.8 / 25.1 / 44.7 | 63.5 / 52.2 / 49.8 |
| C `fma()` i-k-j | 12.0 / 6.41 / 12.1 | 11.9 / 11.4 / 11.9 | 12.2 / 12.1 / 12.1 |

**native、`f32` と `i64`**

| 実装 | `n = 64` | `n = 256` | `n = 512` |
| --- | --- | --- | --- |
| `f32` Tsuzuri `Matrix.mul` | 14.1 / 12.5 / 13.8 | 22.1 / 21.1 / 21.8 | 23.2 / 22.8 / 23.0 |
| `f32` Tsuzuri `Matrix.mul_parallel` | 14.0 / 11.2 / 14.1 | 67.9 / 34.3 / 68.0 | 104.1 / 85.4 / 84.6 |
| `f32` C i-j-k | 4.05 / 2.58 / 4.04 | 2.60 / 2.52 / 2.56 | 2.32 / 2.29 / 2.33 |
| `f32` C i-k-j | 14.4 / 11.6 / 14.2 | 21.9 / 21.6 / 21.8 | 23.8 / 23.4 / 23.7 |
| `i64` Tsuzuri `Matrix.mul` | 5.57 / 4.55 / 5.39 | 5.83 / 5.52 / 5.74 | 5.97 / 5.84 / 5.96 |
| `i64` Tsuzuri `Matrix.mul_parallel` | 5.51 / 4.11 / 5.53 | 33.9 / 12.1 / 25.7 | 36.9 / 27.3 / 30.8 |
| `i64` C i-j-k | 5.74 / 5.06 / 5.56 | 2.95 / 2.74 / 2.89 | 2.33 / 1.91 / 2.18 |
| `i64` C i-k-j | 5.59 / 4.92 / 5.55 | 5.87 / 5.52 / 5.76 | 6.03 / 5.88 / 5.96 |

**wasm32（Node、`f64`）**

| 実装 | `n = 64` | `n = 256` |
| --- | --- | --- |
| 既定 `Matrix.mul` | 4.63 / 4.54 / 4.57 | 4.78 / 4.70 / 4.69 |
| 既定 i-j-k のループ | 2.54 / 2.47 / 2.46 | 2.00 / 1.98 / 2.00 |
| 既定 `Matrix.mul_parallel`（スレッドなしなので逐次） | 4.71 / 4.68 / 4.61 | 4.42 / 4.60 / 4.52 |
| `--wasm-feature simd128` `Matrix.mul` | 6.27 / 6.52 / 6.85 | 7.46 / 7.08 / 7.16 |
| `--wasm-feature simd128` i-j-k のループ | 2.42 / 2.48 / 2.47 | 2.08 / 1.97 / 1.97 |
| `--wasm-feature simd128` `Matrix.mul_parallel`（逐次） | 6.68 / 7.00 / 6.81 | 7.06 / 6.78 / 6.81 |

wasm32 の `Matrix.mul_fma`・`mul_fma_parallel` は、`n = 64` の積 1 回が 459.7 / 472.1 / 465.3 ms（既定）、464.2 / 471.4 / 613.4 ms（simd128）でした。同じ形の `Matrix.mul` は 0.113 ms 前後で、約 4,000 倍です。wasm32 にスカラーの fma 命令はなく、`Math.fma` は正しく丸めるソフトウェアのルーチン（`tz_soft_fma`）を呼ぶためです。

native でも、AArch64 向けにビルドしたコンパイラでなければ同じです。上の native の表（`mul_fma` が `mul` と同じ速さ）は、AArch64 向けにビルドしたコンパイラの値です。それ以外の native を、x86-64 の機械ではなく同じ Apple M1 Max で確かめるために、`src/llvm_math.rs` の `cfg!(target_arch = "aarch64")` を一時的に `x86_64` に変えたコンパイラを別の target ディレクトリへビルドし（変更は戻してあり、コミットしていません）、runner を既定の引数で 1 回実行しました。`n = 64` の `mul_fma` と `mul_fma_parallel`（積和が $2^{18}$ 回で 1 チャンクなので同じ処理）は、1 回の積が 401.8 ms（最小 396.2、最大 412.0）と 402.2 ms（395.7、422.2）で、上の表のハードウェアの FMA（約 11 GFLOP/s、1 回の積が約 0.047 ms）の約 8,600 倍でした。`n = 128` 以降は `not timed` と表示され、checksum は `n = 64` で C の `fma()` の基準と一致しました（runner が検査します）。`n = 128` は、同じ object を積 1 回の時間を測る C のホストにリンクして別に測り（5 回）、1 回の積の中央値が 3.36 s（3.33〜3.40）、1 積和あたり約 1.6 µs でした。ハードウェアの FMA の同じ測定（9 回、各 200 積）は 0.34 ms、同じ形の `Matrix.mul` は 0.35 ms で、ソフトウェアのルーチンは約 1 万倍です。同じ費用で `n = 512` の積を計算すると約 3.5 分になります（計算した値で、測っていません）。測定中の 1 分間の load average は 8〜21 でした。ソフトウェアのルーチンの 1 積和の費用は、x86-64 の機械では測っていません。

測定でわかること:

- `Matrix.mul` は、同じ演算順序の C の i-k-j とほぼ同じ速さです（`f64` 約 11〜12 GFLOP/s、`f32` 約 22〜23、`i64` 約 5.6〜6.0）。Tsuzuri の i-j-k のループの約 4〜6 倍（`n = 64` で約 4 倍、`n = 512` で約 6 倍）、C の i-j-k の約 2.7〜5.6 倍（`n = 64` で小さく、`n = 512` で大きい）です。Phase 1 の `mul` は i-j-k 順で、当時の測定（コミット `4d5da26`、load average 19〜22）は `n = 64` から `512` で 2.9 から 1.35 GFLOP/s でした。
- `mul_fma` は、AArch64 向けにビルドしたコンパイラの native ではハードウェアの FMA で、`mul` と同じ速さです（C の `fma()` の i-k-j とも同じ）。それ以外の native と wasm32 では、`Math.fma` がソフトウェアのルーチンで、上の 2 段落のとおり桁違いに遅くなります。
- `mul_parallel`・`mul_fma_parallel` は、`n = 64`（積和が $2^{18}$ 回で 1 チャンク）では `mul` と同じ速さです。`n = 256` と `512` では、1 回目と 3 回目が `f64` で 45〜64 GFLOP/s（`f32` は最大 104、`i64` は最大 37）で、同じ行分割の C の pthread（41〜51）と同程度でした。2 回目は `n = 256` で 15〜25 に落ちています。load average は 3 回とも 6〜28 で、落ちた原因は特定していません（静かなマシンで測り直した回では落ちませんでした。下）。
- wasm32 の `mul` は、既定で i-j-k のループの約 1.8〜2.4 倍、simd128 で約 2.6〜3.6 倍です。simd128 は既定の `mul` の約 1.4〜1.6 倍でした。

`C i-k-j、pthread` の `n = 64` は、積ごとにスレッドを作って join する費用が支配的です（1〜3.5 GFLOP/s）。Tsuzuri の並列の版は、約 $2^{20}$ 回以下の積和なら 1 チャンクで、スレッドを使いません。

生成コードの確認（2026-10-10、`otool -tV` と、wasm32 の IR を `clang --target=wasm32 -O3 -S` で変換）:

- native arm64 `-O3` の object で、`f64` の `multiply_rows` は `fmul.2d`・`fadd.2d` のループと、末尾のスカラーの `fmul`・`fadd` です。`fmla`・`fmadd`・`fmsub`・`fnmadd` はありません。`f32` は `fmul.4s`・`fadd.4s` のループです。`i64` は NEON に 64 ビット整数の積のベクトル命令がなく、`mul`・`madd` のスカラーだけです。`fma_rows`（`mul_fma`）は `fmla.2d` と `fmadd` です（AArch64 向けにビルドしたコンパイラの場合。それ以外では、IR が `tz_soft_fma` を呼びます）。
- wasm32 `-O3` の既定は、`multiply_rows` が `f64.mul`・`f64.add` のスカラーだけです。`--wasm-feature simd128` を付けると `f64x2.mul`・`f64x2.add` のループが現れます。fma の命令はなく、`Math.fma` は `tz_soft_fma` を呼びます。
- `--emit llvm` の IR に `llvm.fmuladd`・`fast`・`contract`・`reassoc` はなく、`multiply_rows` に `llvm.fma` もありません。`tests/matrix.rs` が IR を、suite `matrix` の `contraction_probe`・`kernel_probe` が最適化後の実行を検査します。probe は、LLVM のバックエンドに積和の融合を許す `llc -fp-contract=fast` でだけ `-2^-60`（binary32 は `-2^-26`）に変わることを確かめてあり、通常の `clang -O3` では `0` です。

`multiply_rows` を out-of-line の関数として呼ぶ形（`matmul_checksum` の `mode` の切り替えで、インライン化されない）では、`left` の添字を `left[base + k]` で検査する実装は約 5.4〜8 GFLOP/s でした。ベクトル化したループに付く重なりの検査が配列の長さに依存し、検査に通らない配置でスカラーの経路を通るためと考えられます。`left` の 1 行を先にスライスにした今の実装は、同じ呼び出しで約 11.9 GFLOP/s です（インライン化された呼び出しは、どちらの実装でも約 12.5）。

**静かなマシンでの測り直し（2026-10-11）。** ほかの作業が止まってから、同じ runner を既定の引数で 1 回、統合したブランチの release ビルドで実行しました（同じ M1 Max、load average は測定の前が 2.3、後が 3.0。生データは `target/perf/quiet/matrix.txt`、git の管理外）。`Matrix.mul` は `f64` が 11.5（`n = 64`）・12.2・11.8・12.0（`n = 512`）GFLOP/s で C の i-k-j（11.7・12.3・11.9・12.2）と同じ、`f32` は 14.1〜23.4、`i64` は 5.6〜5.9 でした。`mul_parallel` は `n = 256` で `f64` 50.3・`f32` 74.0・`i64` 33.5、`n = 512` で 63.3・105.2・38.4 GFLOP/s で、同じ行分割の C の pthread（`f64` が 41.1・55.0）と同程度か上でした。上の 3 回の値の範囲に入り、`n = 256` で 15〜25 に落ちる回はありませんでした。チェックサムは全実装で一致しました。この値も 1 台の arm64 の値です。

この測定は 1 台の arm64 での値です。x86-64 の機械（ソフトウェアの `tz_soft_fma` の 1 積和の費用を含む）、AVX・SVE のような広いベクトル、wasm32 の threads の速度、`n = 1024` 以上、BLAS とのブロッキングを含む比較は未測定で、行列積の GPU カーネルはありません。

## f16 のハードウェア経路（D10）

`node benchmarks/run-f16.mjs BASELINE [COMPILER] [--quick] [--samples N] [--out DIR]` は、`f16`（binary16）のカーネル（`benchmarks/f16/Main.tz`）を、同じ演算を同じ順序で行う C の `_Float16`（`benchmarks/f16/f16.c`）と同じ条件で比べます。`BASELINE` は D10 より前のコンパイラ（`f16` が同梱のソフトウェア経路のもの）、`COMPILER` は測るコンパイラです（省略すると `BASELINE` と同じです）。入力は 100 万要素の `f64` 配列（1/64 の倍数で、およそ −16〜16）です。

| カーネル | 内容 |
| --- | --- |
| `half_chain` | 要素ごとに `f64` から `f16` へ縮め、`total * 0.9990234375 + 要素` を `f16` で畳み込む。前の結果に依存する積と和の連鎖 |
| `half_roundtrip` | 要素を `f16` へ縮めて `f64` へ戻し、`f64` で足す。変換だけ |
| `half_scale` | 要素を `f16` へ縮め、`1.5` を掛け、`f64` へ戻して `f64` で足す。要素どうしは独立で、和の順序は保つので、残りはベクトル化できる |

比べる実装は次のとおりです。

| 実装 | 内容 |
| --- | --- |
| soft | `BASELINE` の `--emit object -O3`。`f16` の演算ごとに `tz_soft_*` を呼ぶ |
| hardware | `COMPILER` の `--emit object -O3`（既定の `--cpu generic`）。macOS arm64 では `half` 命令（FEAT_FP16） |
| promoted | hardware と同じ IR（`--emit llvm -O3`）を `clang -mcpu=generic` で object にしたもの。macOS arm64 の `-mcpu=generic` は FEAT_FP16 の演算を持たないので、LLVM は `half` の演算ごとに `f32` へ広げて 1 回の `f32` 演算をし、丸め直す。FEAT_FP16 のない AArch64 の `--cpu generic`（Linux・Windows）で `half` が下がる形を、この M1 で実行したもの。その CPU の速度ではない |
| C `_Float16` | `clang -O3 -std=c11 -ffp-contract=off -fno-lto`。積と和を融合しない |

どの実装も同じ `f16` の演算を同じ順序で行うので、チェックサム（結果の `f64` のビット）は 4 つとも同じで、runner は計時の前に違いを検査し、違えば失敗します。C と Tsuzuri は、各 build で実行順を回しながら（warm-up のあと）9 回ずつ測り、1 回の通過あたりの中央値・最小・最大（ms）を出します。1 サンプルの通過回数は、Tsuzuri の 1 回の通過が 60 ms 以上かかるときは 1 回、そうでなければ約 60 ms になる回数（最大 2,000 回）です。速度の合否の閾値はなく、`--quick`（4,097 要素、3 サンプル）は動作確認だけで、性能の証拠ではありません。

2026-10-11 に Apple M1 Max（10 コア）、macOS（darwin 27.0.0）、Apple clang 21.0.0、Node v20.19.6、release ビルドのコンパイラ（`e100d5f1` のソース）で 3 回実行した結果です（生データは `target/perf/D10/run-1`〜`run-3` の `results.jsonl`、git の管理外）。ほかの作業のビルドとテストが同時に動き、1 分間の load average は各回の終わりで 8.8、15.5、8.3 でした。表は 1 回の通過（100 万要素）の中央値（ms）の、3 回分（1 回目 / 2 回目 / 3 回目）です。C は、各回の 3 つの build で一緒に測った 3 つの C の中央値です。

| カーネル | soft | hardware | promoted | C `_Float16` |
| --- | --- | --- | --- | --- |
| `half_chain` | 3336 / 3524 / 3311 | 10.22 / 9.98 / 10.18 | 13.90 / 14.03 / 13.75 | 2.257 / 2.274 / 2.251 |
| `half_roundtrip` | 1934 / 1901 / 1922 | 1.388 / 1.379 / 1.400 | 1.815 / 1.799 / 1.789 | 0.986 / 0.972 / 0.970 |
| `half_scale` | 3247 / 3214 / 3232 | 2.179 / 2.221 / 2.186 | 3.435 / 3.435 / 3.419 | 0.970 / 0.967 / 0.968 |

3 回の中央値で、hardware は soft の約 330 倍（`half_chain`）、約 1,380 倍（`half_roundtrip`）、約 1,480 倍（`half_scale`）の速さです。promoted は hardware の約 1.4 倍、1.3 倍、1.6 倍の時間で、soft の約 240 倍、1,070 倍、940 倍の速さです。hardware が C の `_Float16` より遅いのは、それぞれ約 4.5 倍、1.4 倍、2.3 倍です。

生成コードの確認（`--emit llvm -O3` の IR を Apple clang 21 で object にして `otool -tV`）:

- hardware の `half_chain` は、`fmul h`・`fadd h`・`fcvt h, d`（`f64` から `f16` へ 1 命令）のスカラーのループと、NaN の置き換え（`and`・`orr`・`cmp`・`csel`・`fcmp d, d`・`fcsel`）です。promoted は、`half` の演算ごとに `fcvt s, h`・`fmul s`（`fadd s`）・`fcvt h, s` で、ライブラリ関数の呼び出しはありません。
- `half_roundtrip` と `half_scale` は、どちらの build でもベクトル化されます。`f64` から `f16` への縮小は `fcvtxn`（round-to-odd）と `fcvtn` の組、`half_scale` の積は hardware が `fmul.4h`、promoted が `fmul.4s` で、NaN の置き換えは `fcmeq.2d`・`cmhi.2s`・`bif`・`bsl` のベクトル命令です。縮小の 2 段階は、中間の round-to-odd が最終の丸めの結果を変えないので、1 回丸めと同じです。`tests/f16_hardware.mjs` が、二重丸めの罠の値を含む境界の値を、ベクトル化されたループでも照合します。
- soft は `tz_soft_op`・`tz_soft_cast` の呼び出しです。

hardware の `half_chain` のループの中心（`otool -tV` の出力から、NaN の置き換えを省いたもの）:

```text
fmul    h1, h1, h0     ; total * 0.9990234375
fcvt    h2, d1         ; value as f16（1 命令、1 回の丸め）
fadd    h1, h2, h1     ; 積と要素の和
```

**NaN の符号をそろえる費用。** 結果が NaN のとき、`half` 命令の経路は、ハードウェアが返す符号とペイロードを使わず、ソフトウェア経路と同じビットに置き換えます（旧 D3。[D10 の記録](../_features/_completed/D10-f16-hardware.md)、[言語仕様](language.md#f16-の計算経路)）。この置き換え（整数の比較と `select`）が、hardware と C の差の主な原因と見ています。確認のために、`half_binary`・`half_sqrt`・`half_widen`・`half_narrow` の NaN の `select` を取り除いて（結果の NaN のビットはハードウェアのまま）使い捨てのコンパイラをビルドして測ると、`half_chain` は 2.263 ms（C は 2.270）、`half_roundtrip` は 0.968 ms（C は 0.968）、`half_scale` は 0.975 ms（C は 0.975）で、C と同じ時間でした。この試作はコミットしておらず、生データも保存していません（1 回ずつの測定）。置き換えの形を変えた試作の値は次のとおりで、これも 1 回ずつの測定で、同じ load average 8〜24 の下の参考値です。採用したのは、いちばん単純で、E2E で検証した最初の形で、ほかは採用していません。

| 形 | `half_chain`（ms） | `half_scale`（ms） | 結果 |
| --- | --- | --- | --- |
| 整数で判定し、整数の `select`（採用） | 10.0 | 2.19 | 採用 |
| `fcmp uno half` と `half` の `select` | 8.0 | 2.38 | 連鎖は速いが、ベクトル化したループは遅く、不採用 |
| NaN で分岐（`!prof` つき） | 9.5 | — | LLVM が、スカラーの連鎖では `select` に戻す。`half_roundtrip` で速く見えたのは、ベクトル化されなかっただけ |
| `select` に `!prof` | 変化なし | — | 不採用 |
| `copysign`・`fcmp uno`・`select` を浮動小数点の領域で行う | 4.79 | 2.47（+13%） | 連鎖は 2 倍以上速いが、ベクトル化したループが遅くなり、promoted では連鎖 10.8（−22%）、`half_scale` 4.45（+29%）で、一貫しない。不採用 |
| 整数で判定し、`copysign` | 9.96 | — | 変化なし |

NaN のビット（符号とペイロード）を言語の仕様から外せば、置き換えは要らなくなり、上の試作のとおり `f16` は C と同じ時間になる見込みです（この実装では測っていません）。ただし、`f16` の NaN のビットが、経路（WASM のソフトウェア経路と native の `half` 命令）で違うことになります。この費用に値するかは、仕様の判断として残してあり、この実装では変えていません（既存の `f32` と `f64` の NaN のビットは、すでにハードウェア依存です）。

測っていないもの: x86-64（F16C と AVX512-FP16 のどちらも）、Linux と Windows の AArch64、FEAT_FP16 のない CPU の実機（promoted は、その形の命令列を M1 で実行しただけです）、M1 以外の Apple Silicon、ほかの `-O` と `--cpu native`、wasm32（常にソフトウェア経路で、この変更の対象外）。runner は `clock_gettime` のある POSIX の環境なら x86-64 などでも動くはずですが（macOS arm64 以外では実行していません）、hardware は既定の `--cpu generic` の経路（x86-64 ではソフトウェア）で、`--cpu native` の x86-64 の経路は測れません。この節の数値を、それらの機械の性能として扱わないでください。CI の runner（Intel Mac・Linux x86-64・Linux arm64）は `tests/f16_hardware.mjs` で結果のビットを照合するだけで、速度は測りません（共有の runner に速度の合否の閾値は置きません）。

## 静的データ（D11 Phase 2）

`node benchmarks/run-statics.mjs target/release/tsuzuri [--baseline 以前のコンパイラ] [--size バイト数] [--runs 回数] [--quick] [--artifacts ディレクトリ]` は、256 要素の表を引く CRC-32 のループ（`benchmarks/statics/Main.tz`）を、表の読み方の違いで比べます。入力は `(i * 7 + 3) & 255` のバイト列をループの中で作るので、表のほかにメモリは読みません。表は定数 `CrcTable`（256 個の `i32u`。`tests/fixtures/const_functions` の `crc_table()` が計算する値のリテラルで、`const def` のない以前のコンパイラも同じソースを読めます）です。

| 形 | 内容 |
| --- | --- |
| `in_place` | ループで `CrcTable[...]` を直接読む。平たいデータの定数なので、バイナリの静的データを読む |
| `bound` | ループの前に `let table = CrcTable` と束縛し、その複製を読む |
| `copied` | ループの中で毎回 `let table = CrcTable` と束縛し、その複製を読む |

現在のコンパイラの 3 つの形、`--baseline` に渡した静的データのない以前のコンパイラ（`053bced8`）の同じ 3 つ（`before_*`）、同じ表を `static const` に書いた C（`c_static_table`、`-O3 -ffp-contract=off`）を測ります。全変種の checksum が JavaScript の CRC-32 と一致しないと失敗します。1 回の呼び出しの thread CPU 時間と wall-clock 時間を、変種の順を回しながら測り、中央値を出します。速度の合否の閾値はなく、`--quick` は動作確認だけで性能の証拠ではありません。

測定（2026-10-11、M1 Max、macOS、Apple clang 21.0.0、`--size 4194304 --runs 15`、runner を 3 回実行した thread CPU 時間の中央値の範囲）です。

| 変種 | ns/バイト |
| --- | --- |
| `c_static_table`（C） | 2.633〜2.641 |
| `in_place`（現在） | 2.626〜2.630 |
| `bound`（現在） | 2.626〜2.636 |
| `copied`（現在） | 13.269〜13.299 |
| `before_in_place`（以前） | 13.220〜13.277 |
| `before_bound`（以前） | 2.619〜2.639 |
| `before_copied`（以前） | 13.217〜13.292 |

同じソースの `in_place` は、以前のコンパイラの `before_in_place` の 5.03〜5.06 倍の速さで、C の `static const` の表と同じ速さです。wall-clock の中央値も同じ値の ±0.5% でした（測定中のロードアベレージは 27〜34 で、ほかのプロセスが動いていました）。

生成コードを `--artifacts` で保存して確かめました。現在の `crc_in_place` の `-O3` のループは 7 命令で、`l_tz.static.N.data` を `ldr w11, [x10, w11, uxtw #2]` で読みます。添字 `& 0xFF` が長さ 256 に収まることを LLVM が証明するので、境界検査もありません。以前のコンパイラの同じ関数は、反復ごとに 1 KiB の表を作り直します（ループ本体 106 命令のうち、16 バイトのベクトル store が 64 個）。このため `copied`（反復ごとの束縛）も同じ遅さです。ループの前に束縛する `bound` は、どちらのコンパイラでも C と同じ速さです（LLVM が 1 回の複製を定数の表として保つため）。

つまり静的データの利点は、定数を名前で直接読む形で表を作り直さなくなること、`let` で束縛するための書き換えが要らないことです。束縛していた形（`bound`）の速さは変わりません。測ったのは M1 Max の 1 つの負荷（表 256 要素、`i32u`）だけで、x86-64 と Linux、ほかの表の大きさ、wasm32 の実行時間は測っていません。s の速度、`n = 1024` 以上、BLAS とのブロッキングを含む比較は未測定で、行列積の GPU カーネルはありません。

## 性能指標の記録

PX01 形式は `schema: 1` の JSON Lines です。生の標本、中央値、最小、最大、run_id、commit、
未コミットの変更の有無、CPU・OS・Clang・rustc・Node と計測した Tsuzuri の SHA-256 を、各行に記録します。
生データは `target/perf/<run_id>/<suite>.jsonl` に保存し、Git には入れません。
同じ suite の二度目の書き込みは `EEXIST` です。測り直すときは新しい run_id を使います。

```sh
RUN=target/perf/PX01-example
node benchmarks/run-control.mjs target/release/tsuzuri --metrics "$RUN"
node benchmarks/run-cpp.mjs target/release/tsuzuri --metrics "$RUN"
node benchmarks/run-computations.mjs target/release/tsuzuri --metrics "$RUN"
node benchmarks/metrics.mjs process target/release/tsuzuri --out "$RUN"
node benchmarks/metrics.mjs report "$RUN"
node benchmarks/metrics.mjs report target/perf/PX01-after --baseline target/perf/PX01-before
```

`--quick` は正しさの確認だけです。レコードは `mode: "quick"` となり、`report --publish` は拒否します。
本計測は AC 電源で、ほかの重い処理を動かさずに行います。in-process の時計は起動とコンパイルを含まず、
process の時計は `/usr/bin/time` の起動から終了までを含みます。両者を直接比べません。
macOS は `/usr/bin/time -l`、Linux は GNU time 1.8 以上の `/usr/bin/time -v` が必要です。
ディストリビューションの GNU time が版を `UNKNOWN` と表示する場合も、GNU の識別を確認し、
各標本で `-v`・`-o` と必要な時間・RSS の欄を検査します。未知の形式や欠けた欄はエラーです。

| 指標 | 単位 | 読み方 |
| --- | --- | --- |
| `wall_time` | ms | kernel 1 回、または process 全体の経過時間 |
| `cpu_time` | ms | 全スレッドの CPU 時間。process は user + sys |
| `user_time`・`sys_time` | ms | OS の CPU 時間（短い処理では 0 もある） |
| `peak_rss` | bytes | 子プロセスのうち最大の 1 つを含む RSS。合計ではない。コンパイラ単体は check／IR 出力で読む |
| `alloc_calls`・`alloc_bytes` | count・bytes | 最適化後の Tsuzuri IR の確保回数・要求バイト数の累計。別の tracked 実行で測る。allocator 自身の計数用ヘッダーは含まない |
| `executable_bytes`・`object_bytes`・`wasm_bytes` | bytes | 成果物のファイルサイズ。標本間で一致することを検査 |
| `phase_time` | ms | `TSUZURI_TIME_PASSES=1` の段と total。`emit` は target ごとの IR 処理も含み、外部ツールとは分ける。段は入れ子にしない |

通常は warm-up の後で 9 標本以上を記録します。確保と大きさは決定的な値 1 標本です。
中央値は偶数個なら中央の 2 値の平均です。比較は中央値の差が両 run の大きい方の最小〜最大の広がり以内なら
「差なし」、それを超えて小さければ「改善」、大きければ「悪化」です。統計的な有意差の検定ではありません。
CPU・OS・ツール・Node などが違うと「条件差」とし、commit とコンパイラの SHA-256 の違いは比較を妨げません。
どの判定も終了コードを変えず、共有 CI の性能の合否には使いません。

process suite は hello の check、native `-O0`・`-O3`、object、WASM、native の実行を記録します。
cache は使いません。project に対応しない object／WASM の `E2004` は理由付きで skip し、
ツールの不足、実行失敗、標本間の不一致はエラーです。
macOS の実行ファイルは、変更前からリンカーの UUID と署名が毎回変わります。
その 2 領域だけを除いた SHA-256 を照合し、コード・データ・その他のメタデータとファイルサイズは検査します。
IR・header・WASM とその他の object は byte 単位で同じであることを検査します。
native Windows Clang の標準 COFF object は `TimeDateStamp`（offset 4 の 4 bytes）だけを除きます。
2026-10-14 の CI の差分は `0x6aceeee0`／`0x6aceeee2` で、実際の 02:54:24／02:54:26 UTC と
2 秒の待ち時間に一致しました。machine／section header を確認してから比較し、残りの byte とサイズを厳密に保ちます。
この時刻を生成後に書き換えたり、compiler flags やコードの byte を変更したりはしません。

Phase 2 の tasks・simd・dispatch・mix も `--metrics <dir>` を受け付けます。
Task は cold と warm を別 variant にし、SIMD・dispatch の標本は反復回数で割った 1 呼び出しの wall time です。
`run.mjs` の native は wall と CPU、WASM は wall を記録します。
既定の stdout は変えず、9 標本が必要な runner は metrics 指定時だけ標本を増やします。

`node benchmarks/metrics.mjs report <dir>... --publish` は、下のマーカーの間だけに本計測の要約を公開します。
指標の表は中央値で、最小値の強調や言語の相対性能の列は付けません。

2026-10-14 の初回記録は Apple M1 Max・macOS 27.0.1・AC 電源、Apple Clang 21.0.0、
rustc 1.98.1、Node 24.21.0、Go 1.27.2、Zig 0.16.0 です。
run は `PX02-formal-apps`・`PX02-formal-managed`、commit は `dadc6d8b0a7836a702d3b7e033a13030406b0ca9` の
未コミットの実装を含みます。**共有機で開始時の load average が 4.92 あり、専用の無負荷の計測ではありません。**
これは 9 標本以上を持つ記録経路の検証と参照用の値で、一般的な速度の勝敗や改善を主張しません。
実行時の条件は `target/perf/PX02-formal-apps/env.txt`、生データと stdout は各 run のディレクトリです。
下の managed／core 473 レコードは初回を保ち、apps 200・process 88 は `PX03-formal-emit-v4` の再計測に置き換えました。
process は `emit` の target IR の計上漏れ、apps は Zig の必要な compiler runtime のリンクを修正後の値です。
この run は `59a4f1de` の未コミットの修正を含み、同じ toolchain・共有機で開始時の load average は
process／build が 8.85、apps が 7.09 です。条件は `target/perf/px03-formal-emit-v4{,-apps}-env.txt` に残します。
旧 run の `emit` の値とは計上範囲が違うため、速度の比較に使いません。

<!-- perf-metrics:start -->
<p>run: <code>PX02-formal-managed</code> / 2026-10-13T16:16:29.786Z / commit: <code>dadc6d8b0a78</code>（未コミットの変更を含む）<br>CPU: Apple M1 Max / arm64 / 10 cores / darwin 27.0.1<br>Clang: Apple clang version 21.0.0 (clang-2100.3.34.2) / rustc: rustc 1.98.1 (48a229cea 2026-09-01) (Homebrew) / Node: v24.21.0<br>Tsuzuri SHA-256: <code>659cf1ad87e8e3dace10ded0d5b82ece98765cd496a155b51f343d55ed5f77dc</code></p>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">wall_time (ms) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri direct</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri computation</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">JavaScript</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.322</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.323</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.325</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.388</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/checked</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.512</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.524</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.516</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.528</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/delayed</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.131</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.131</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.131</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.149</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_for</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/owned_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.446</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.515</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.446</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.445</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_result</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.518</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.516</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.507</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.522</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_owned</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.017</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.018</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.033</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_length</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.391</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.391</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.385</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.460</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/while_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.304</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.297</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.302</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.300</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/for_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.297</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.293</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.293</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.066</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.296</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.296</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.301</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.296</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_if_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.294</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.289</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.294</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.290</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_builtin_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.282</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.286</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.281</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.288</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/match_dispatch</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.255</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.414</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.413</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.255</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.168</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.168</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.168</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.167</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_index_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.167</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.168</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.169</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.168</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_copy</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.133</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.133</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.137</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.137</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/list_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.177</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.181</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.181</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.182</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.191</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.191</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.191</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.190</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_churn</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.640</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.630</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.635</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.907</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/record_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.273</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.270</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.274</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.275</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/integer128_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.513</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.515</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.511</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.530</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float32_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.885</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.885</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.892</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.887</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float64_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.889</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.886</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.891</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.889</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/integer_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.236</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.209</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.240</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/mandelbrot</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">243</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.317</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.362</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.389</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.343</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.347</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.345</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_scan</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.007</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.007</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.007</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_compare</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.017</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.041</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.020</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_validate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.054</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.054</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.111</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf8_roundtrip</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.164</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.089</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.106</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/format_parse</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.374</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.326</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.182</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/math_intrinsics</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.509</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.508</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.515</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequence</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.080</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.080</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.079</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequential</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.273</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.271</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.272</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_parallel</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.241</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.293</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.300</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-while_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.522</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.420</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-for_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.521</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.481</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-tail_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.524</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.358</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-tail_if_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.532</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.089</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-tail_builtin_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.518</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.141</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-match_dispatch</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.498</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">46.160</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.586</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.011</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-array_index_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.586</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.758</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-array_copy</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.405</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.304</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-list_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.239</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.546</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-closure_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.197</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.039</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-closure_churn</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.342</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.750</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-record_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.527</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">36.805</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-integer128_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.705</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">12.214</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-float32_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.885</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.417</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/control-float64_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.888</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.821</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-integer_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.173</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">65.801</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-mandelbrot</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">243</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15.518</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.198</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.180</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">27.661</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-utf16_scan</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.030</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.068</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-utf16_compare</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.019</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.009</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-utf16_validate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.078</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.351</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-utf8_roundtrip</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.111</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.404</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-format_parse</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.272</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.706</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-math_intrinsics</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.508</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.509</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-task_sequence</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.633</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.836</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-task_sequential</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.270</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26.394</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/cpp-task_parallel</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.226</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">41.393</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/computations-bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.318</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.425</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/computations-checked</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.817</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.403</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/computations-delayed</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.183</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">7.591</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/computations-array_for</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.077</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/computations-array_bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.076</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/computations-owned_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.064</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/computations-std_option</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.548</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.880</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/computations-std_result</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.753</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15.561</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/computations-std_option_owned</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.018</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.060</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">managed/computations-std_option_length</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.574</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.124</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">cpu_time (ms) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri direct</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri computation</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.321</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.322</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.323</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.387</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/checked</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.510</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.521</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.515</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.526</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/delayed</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.130</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.130</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.130</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.148</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_for</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/owned_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.444</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.513</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.445</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.444</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_result</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.516</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.513</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.505</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.521</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_owned</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.017</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.018</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.033</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_length</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.390</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.390</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.384</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.458</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/while_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.296</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.288</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.294</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.294</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/for_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.293</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.286</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.289</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.055</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.293</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.292</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.289</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.291</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_if_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.287</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.283</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.288</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.284</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_builtin_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.279</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.282</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.278</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.286</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/match_dispatch</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.254</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.413</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.411</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.254</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.168</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.168</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.167</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.167</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_index_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.167</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.168</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.168</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.168</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_copy</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.133</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.133</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.137</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.137</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/list_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.177</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.181</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.181</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.181</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.190</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.190</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.190</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.190</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_churn</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.638</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.628</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.634</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.904</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/record_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.268</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.266</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.269</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.270</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/integer128_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.511</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.514</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.509</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.527</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float32_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.882</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.883</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.889</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.884</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float64_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.886</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.884</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.887</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.887</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/integer_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.221</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.199</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.230</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/mandelbrot</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">243</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.270</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.274</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.307</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.342</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.345</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.343</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_scan</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.007</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.007</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.007</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_compare</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.017</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.041</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.019</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_validate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.053</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.054</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.110</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf8_roundtrip</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.164</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.089</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.106</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/format_parse</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.373</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.325</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.180</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/math_intrinsics</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.507</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.506</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.512</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequence</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.079</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.080</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.079</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequential</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.269</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.261</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.268</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_parallel</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.475</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.609</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1.645</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">alloc_calls (count) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri direct</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri computation</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/checked</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/delayed</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_for</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/owned_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_result</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_owned</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">55</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">55</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_length</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/while_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/for_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_if_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_builtin_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/match_dispatch</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_index_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_copy</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/list_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_churn</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/record_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/integer128_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float32_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float64_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/integer_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/mandelbrot</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">243</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_scan</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">24</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_compare</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">27</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_validate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf8_roundtrip</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/format_parse</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/math_intrinsics</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequence</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequential</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_parallel</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">18</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">alloc_bytes (bytes) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri direct</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri computation</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/checked</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/delayed</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_for</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">512</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">512</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">512</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">512</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/owned_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2048</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2048</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_result</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_owned</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1210</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1210</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_length</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">64</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/while_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/for_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_if_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_builtin_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/match_dispatch</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3200000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_index_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3200000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_copy</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3200000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/list_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">160000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_churn</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/record_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/integer128_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float32_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float64_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/integer_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/mandelbrot</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">243</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">6400000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_scan</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">196560</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_compare</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">393172</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_validate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">327636</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf8_roundtrip</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">294864</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/format_parse</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">193948</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/math_intrinsics</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequence</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequential</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_parallel</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1024</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">wall_time (ms) / wasm32 / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri direct</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri computation</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1024</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/checked</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1024</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/delayed</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1024</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_for</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.001</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/owned_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.003</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1024</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_result</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1024</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_owned</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.011</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.012</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_length</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1024</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.002</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">wasm_bytes (bytes) / wasm32 / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri computation</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/module</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10908</code></td>
</tr>
</tbody></table>
</div>
<p>run: <code>PX03-formal-emit-v4</code> / 2026-10-13T23:18:32.615Z / commit: <code>59a4f1de6bb6</code>（未コミットの変更を含む）<br>CPU: Apple M1 Max / arm64 / 10 cores / darwin 27.0.1<br>Clang: Apple clang version 21.0.0 (clang-2100.3.34.2) / rustc: rustc 1.98.1 (48a229cea 2026-09-01) (Homebrew) / Node: v24.21.0<br>Tsuzuri SHA-256: <code>ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc</code></p>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">wall_time (ms) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-object</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri execute</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Zig</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Go</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/tokenize</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.890</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">12.057</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.837</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.447</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15.489</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.142</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/format_join</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.398</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">17.141</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">6.288</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.465</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">22.365</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.212</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/csv_aggregate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">27.367</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">31.764</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.683</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.734</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">33.635</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15.623</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/map_count</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.870</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.980</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.845</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.551</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">7.829</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.322</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/record_sort</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10.548</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.573</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.449</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.426</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.957</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/tree_transform</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">256</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.796</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.236</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">25.811</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.846</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.742</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.528</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/binary_trees</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">25.869</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">39.754</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">56.138</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">43.025</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">40.466</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">21.047</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/closure_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.880</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.409</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.113</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.399</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.033</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.888</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/graph_bfs</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.065</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.741</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.803</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.823</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.221</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.690</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/spectral_norm</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">500</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.388</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.983</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.992</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.093</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.924</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.592</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">714.640</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">735.382</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">59.858</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">cpu_time (ms) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-object</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri execute</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Zig</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Go</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/tokenize</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.779</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.995</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.794</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.344</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15.326</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10.988</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/format_join</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.262</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">17.018</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">6.243</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.412</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">22.229</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.353</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/csv_aggregate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">27.155</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">31.575</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.631</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.608</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">33.444</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">18.447</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/map_count</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.827</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.970</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.819</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.532</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">7.779</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.346</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/record_sort</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10.508</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.814</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.559</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.418</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.380</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">6.203</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/tree_transform</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">256</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.700</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.124</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">25.646</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.768</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19.637</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.798</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/binary_trees</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">25.776</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">39.657</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">55.823</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">42.794</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">40.311</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">24.192</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/closure_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.866</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.393</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.094</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.389</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">7.995</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.132</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/graph_bfs</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.047</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.745</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.786</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3.802</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.168</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.027</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/spectral_norm</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">500</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.328</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.943</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8.939</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.053</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.886</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9.617</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">670.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">610.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">peak_rss (bytes) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-object</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri execute</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Zig</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Go</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/tokenize</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">17055744</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">12582912</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">12517376</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">20774912</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15908864</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14352384</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/format_join</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">18776064</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11845632</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">8617984</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">16924672</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">29753344</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13451264</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/csv_aggregate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9994240</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">7487488</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">7438336</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">12517376</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9224192</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14204928</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/map_count</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1753088</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1753088</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1753088</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1851392</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1851392</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4407296</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/record_sort</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11386880</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11386880</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11403264</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11468800</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11452416</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14614528</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/tree_transform</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">256</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1949696</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1851392</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1884160</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1933312</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1900544</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10780672</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/binary_trees</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1900544</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1802240</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1835008</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1884160</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1867776</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11157504</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/closure_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">17776640</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">17760256</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">17776640</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">17858560</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">17809408</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">21020672</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/graph_bfs</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15351808</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15351808</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15368192</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15433728</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15450112</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">19021824</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/spectral_norm</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">500</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1769472</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1753088</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1753088</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1835008</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1785856</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4521984</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">53215232</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">53313536</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1720320</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">alloc_calls (count) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/tokenize</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400003</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/format_join</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200003</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/csv_aggregate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800005</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/map_count</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">16</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/record_sort</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">3</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/tree_transform</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">256</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">792564</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/binary_trees</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1018000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/closure_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">7</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/graph_bfs</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">7</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/spectral_norm</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">500</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">41</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">alloc_bytes (bytes) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/tokenize</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">12500022</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/format_join</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15038726</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/csv_aggregate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13880746</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/map_count</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">36768</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/record_sort</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">9600000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/tree_transform</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">256</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">38043072</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/binary_trees</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">48864000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/closure_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">16000192</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/graph_bfs</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13600008</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">apps/spectral_norm</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">500</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">164000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">wall_time (ms) / native / none / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri check</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">91.201</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">cpu_time (ms) / native / none / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri check</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">40.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">user_time (ms) / native / none / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri check</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">40.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">sys_time (ms) / native / none / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri check</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">peak_rss (bytes) / native / none / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri check</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26296320</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">phase_time (ms) / native / none / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri check</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [load]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.297</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [cache]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.037</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [parse]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.889</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [check]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">32.490</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [specialize]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.459</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [closures]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.045</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [ownership]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.108</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [total]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">44.210</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">wall_time (ms) / native / O0 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri execute</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">265.296</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">58.704</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">cpu_time (ms) / native / O0 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri execute</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">210.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">user_time (ms) / native / O0 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri execute</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">160.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">sys_time (ms) / native / O0 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri execute</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">peak_rss (bytes) / native / O0 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri execute</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">43728896</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">1703936</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">phase_time (ms) / native / O0 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [load]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.309</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [cache]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [parse]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.878</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [check]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">32.167</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [specialize]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.598</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [closures]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.042</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [ownership]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.110</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [emit]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.631</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [tool.clang]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">172.526</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [total]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">223.443</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">executable_bytes (bytes) / native / O0 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">138008</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">user_time (ms) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-object</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri execute</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">610.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">570.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">sys_time (ms) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-object</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri execute</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">60.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">phase_time (ms) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-object</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [load]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.300</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.312</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [cache]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [parse]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.015</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.988</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [check]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">32.003</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">31.306</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [specialize]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.916</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.765</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [closures]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.044</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.043</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [ownership]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.116</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.120</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [emit]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.654</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.671</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [tool.clang]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">633.582</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">643.894</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [total]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">684.001</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">699.796</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">executable_bytes (bytes) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">84616</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">object_bytes (bytes) / native / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-object</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">48200</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">wall_time (ms) / wasm32 / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-wasm</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">796.216</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">cpu_time (ms) / wasm32 / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-wasm</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">640.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">user_time (ms) / wasm32 / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-wasm</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">610.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">sys_time (ms) / wasm32 / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-wasm</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">30.000</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">peak_rss (bytes) / wasm32 / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-wasm</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">62095360</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">phase_time (ms) / wasm32 / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-wasm</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [load]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.312</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [cache]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [parse]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">4.972</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [check]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">31.717</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [specialize]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5.643</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [closures]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.045</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [ownership]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">0.119</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [emit]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2.497</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [tool.clang]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">635.124</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [tool.wasm-ld]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">49.870</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello [total]</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">760.190</code></td>
</tr>
</tbody></table>
</div>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">wasm_bytes (bytes) / wasm32 / O3 / generic / 中央値</caption>
<thead><tr>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">種目</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">仕事量</th>
<th scope="col" style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri build-wasm</th>
</tr></thead>
<tbody>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">process/examples/hello</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">-</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">24864</code></td>
</tr>
</tbody></table>
</div>
<!-- perf-metrics:end -->

## ビルド速度

`benchmarks/build/generate.mjs` は、20 関数／module の決定的な入力を生成します。
10 種類の処理（while、for、match、record、union、generic、capture、文字列、配列、再帰）を 2 回ずつ持ち、
stdin の 1 行目の UTF-16 の長さに依存する unsigned 64-bit の checksum を出します。
各 native の `-O0`・`-O3` の結果は、空入力・`12345`・`綴り😀` の 3 入力で独立な BigInt 参照と照合します。
1,000 module は 139,010 行です。2026-09-29 の 124,003 行の合成入力とは生成規則が違い、同じ表で直接比べません。

```sh
node benchmarks/run-build.mjs target/release/tsuzuri --out target/perf/PX03-example
node benchmarks/run-build.mjs target/release/tsuzuri --quick --out target/perf/PX03-quick
node benchmarks/run-build.mjs target/release/tsuzuri --sizes 200,1000 --languages tsuzuri --out target/perf/PX03-tsuzuri
node benchmarks/build/generate.mjs --modules 1000 --out /tmp/tsuzuri-synthetic
```

通常は 200・1,000 module と 11 の実プロジェクト、quick は 10 module と hello です。
Tsuzuri は check、LLVM IR、native `-O0`・`-O3`、WASM `-O0`・`-O3`、変更なし、本体の編集、公開 API の編集の 9 組を測ります。
初回の組は両 cache を無効にし、warm と編集の組は専用の一時 cache を使います。
warm は外部ツールが無いこと、編集は外部ツールが走ることを標本ごとに検査します。
main の無い実プロジェクトの実行ファイルなど、`E2004` だけを理由付きで skip し、ほかの失敗はエラーです。

**大規模 WASM `-O3` の既知の制限:** 2026-10-14、1,000 module（41,566,509 bytes の IR）は
Apple Clang 21.0.0 と upstream Clang 21.1.8 の両方で 600 秒の process timeout を超えました。
sample では LLVM の InstCombine／known-bits の計算などに時間があり、Apple の処理は終了前に大きなメモリも使いました。
標準の run は 200 を超える合成 module の WASM `-O3` だけを理由付きで除外し、`projects.json` の
`skipped_variants` に残します。`--large-wasm` を付けた明示要求は試行し、timeout をエラーとして返します。
WASM `-O0`、native と 200 module の全組は除外しません。LLVM の flags や生成する処理を変えて隠しません。
タイムアウトでは `/usr/bin/time` だけでなく、所有する process group のコンパイラ・子ツールも
`SIGKILL` で終了させます。`SIGTERM` を無視する子がいても残しません。

**大規模 Mach-O `-O0` の既存のリンクの変動:** 1,000 module では、計時を足す前の compiler も、
Apple の linker の import-stub の順序によって 2 つの fingerprint になります。
差は memcpy／bzero などの同じ symbol stub の呼び出し先アドレスと stub の順序で、参照値とサイズは一致しました。
この組だけは追加のコードの byte の正規化をせず、warning と `projects.json` の `linker_variation` に
全標本の fingerprint を残します。IR の 9 回の厳密な比較と 3 入力の独立な参照の検査は維持します。
変更前・変更後の実行ファイル、symbol 付きの disassembly は `target/perf/PX03-native-{before-variation,nondeterminism}/` に保存しました。

Phase 2 も実装済みです。C・C++・Rust（single crate と module ごとの crate の workspace）・Go・Zig は、
同じ処理と参照値の入力を生成し、native の初回 `-O0`・`-O3`、warm、本体の編集、API の編集を測ります。
ツールが既定名で見つからなければ理由付きで skip します。明示した `TSUZURI_GO`・`TSUZURI_ZIG` などが使えないときはエラーです。
`--languages` は測る系列を選び、指定した系列がすべて使えないときもエラーです。

| 系列 | 条件 |
| --- | --- |
| Tsuzuri | 1 ファイル／module、全体の検査・IR 生成は毎回。構文解析／復号と成果物の cache を分離 |
| C／C++ | module ごとの source・header、`make -j<logical cores>`、Clang `-O0`／`-O3`。LTO・fast-math なし。編集の `--what-if` と最終成果物の削除で mtime の丸めによる noop を防ぐ |
| Rust | 安全な std の Vec／String、折り返しを明示。dev／release、offline、LTO なし。workspace の分割と単一 crate は別 variant |
| Go | module ごとの package。debug は `-gcflags=all=-N -l`（std も対象）、最適化版は通常の `go build`。GC と GOMAXPROCS は既定または記録した環境 |
| Zig | `Debug`／`ReleaseSafe`、`-mcpu=baseline`、折り返しを明示。C／Zig の closure は native の捕捉構文が無いため型付きの環境で表現 |

schema の `O0`・`O3` は比較する debug／最適化版の分類であり、Go・Rust・Zig が Clang と同じ pass を使う意味ではありません。
実コマンド、版、Go の環境、生成した files／lines／bytes、IR のサイズは `projects.json` に残します。
Go・Zig の版と Go の環境は schema 1 の自動条件判定には入らないので、before／after では補助 manifest も照合します。
参照コンパイラの debug のメタデータには非決定性が残る場合があります。checksum と大きさを検査し、
Tsuzuri の IR・WASM・実行コードはさらに fingerprint を照合します。いずれも性能の合否閾値はありません。

要約の frontend は load・parse・check・specialize・closures・ownership・emit の和、
cache は cache の和、tools は外部ツールの和、other は total との差です。各標本で分けてから中央値を計算します。
小数 3 桁の計時の丸めだけは許容します。参照言語の内訳は分からないので `-` であり、0 ms の意味ではありません。
最大 RSS は子プロセスの最大の 1 つも含むので、コンパイラ単体の値は check と LLVM IR 出力で読みます。
CI は macOS／Linux の x64／ARM64 で quick の checksum・記録を検査し、速度を判定しません。

### ビルド記録（2026-10-14、計上範囲の修正後）

`emit` の target IR の計上漏れを修正して、200・1,000 module と real 11 の全組を再計測しました。
現在の build の本記録は `PX03-formal-emit-v4`（build 1,206、process 88、apps 200、計 1,494 レコード）です。
環境は上の process の再計測と同じ共有機・dirty tree です。開始時の load average は 8.85 でした。
初回の `PX03-formal-a`、repeat 200 の `PX03-formal-b-retry`、repeat 1,000 の
`PX03-formal-large-repeat-verified` と同じキーの比較 `target/perf/PX03-formal-comparison.txt` は履歴として残します。
旧 run 同士の比較は同じ計上範囲ですが、修正後の `emit` と混ぜた速度比較はしません。
括弧なしの値は wall ms の中央値で、9 標本です。元の最小・最大・全標本は JSON Lines に残します。

| module | check | LLVM IR | native O0 | native O3 | WASM O0 | WASM O3 | warm O0 | body edit O0 | API edit O0 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 200 | 450.602 | 610.417 | 1620.294 | 7392.654 | 1831.955 | 65423.082 | 736.460 | 1828.140 | 1880.992 |
| 1,000 | 2242.939 | 2942.584 | 7231.361 | 72748.937 | 8511.871 | 予算超過・明示要求のみ | 3302.139 | 7942.103 | 7789.017 |

200 の user source は 27,810 行・726,844 bytes、native IR は 7,951,476 bytes、
1,000 は 139,010 行・3,648,845 bytes、native IR は 36,851,308 bytes です。std の source の処理は時間には含まれます。
before と再測定で揺れがあるため、これをコンパイラの速度改善として扱いません。

Phase 2 の参照の初回記録は `PX03-reference-formal`（162 レコード、10 module）です。
次は実コマンドの相違を残した、小さい仕事量の参照値であり、「Go を上回る」の判定ではありません。
Zig は cold cache を毎標本空にし、標準ライブラリ・libc の準備も含む条件です。

| 系列／layout | debug | 最適化版 | warm debug | body edit | API edit |
| --- | ---: | ---: | ---: | ---: | ---: |
| C | 284.676 | 288.412 | 62.029 | 222.741 | 193.884 |
| C++ | 860.523 | 894.243 | 62.451 | 578.182 | 595.331 |
| Rust single | 539.112 | 676.946 | 73.508 | 338.680 | 329.523 |
| Rust workspace | 842.515 | 650.295 | 36.878 | 469.381 | 451.018 |
| Go | 3021.450 | 3387.603 | 195.785 | 367.173 | 387.644 |
| Zig | 29228.970 | 33982.517 | 1579.094 | 1601.134 | 1587.275 |

## 実アプリ型の比較

`run-apps.mjs` は、10 種目の Tsuzuri・C・C++・Rust・Zig・Go を独立に実装し、
同じ入力と wrapping u64 の checksum を、別の BigInt／順序を保った Number 参照とも照合します。

```sh
node benchmarks/run-apps.mjs target/release/tsuzuri --quick --metrics target/perf/PX02-quick
node benchmarks/run-apps.mjs target/release/tsuzuri --metrics target/perf/PX02-example --artifacts target/perf/PX02-artifacts
node benchmarks/metrics.mjs report target/perf/PX02-example
```

| 種目 | 通常／quick の大きさ | 処理 |
| --- | --- | --- |
| tokenize | 200,000／1,024 | 語の所有文字列を join・split し、長さを fold |
| format_join | 100,000／1,024 | 所有の十進文字列を連結・分割・解析し、合計 |
| csv_aggregate | 100,000／1,024 | 所有の行・field を作り、16 種の集計 |
| map_count | 200,000／1,024 | 整列 Map／Set、二分探索、隣接交換による挿入 |
| record_sort | 200,000／1,024 | 同じ bottom-up merge。等しい key は左を先に取る安定な整列 |
| tree_transform | 256／2 | 深さ 12 の式の木を左から作り、消費して簡約・評価 |
| binary_trees | 2,000／4 | 深さ 4〜11 の完全二分木の構築・節の数え上げ・解放 |
| closure_pipeline | 1,000,000／1,024 | 4 つの捕捉、新しい map 配列、偶数の fold |
| graph_bfs | 100,000／1,024 | 4n の辺、CSR の構築、0 からの BFS |
| spectral_norm | 500／16 | 10 回の AtA。f64 の加算順は添字の昇順、縮約・再結合なし |

文字列の入力は ASCII です。Tsuzuri は UTF-16、ほかは byte の所有文字列であり、同じ表現ではありません。
C は malloc／realloc／free と snprintf／strtoull、C++ は std::string／vector／unique_ptr／function と
to_chars／from_chars、Rust は安全な String／Vec／Box、Zig は c_allocator の所有 slice、
Go は native の string／slice／node／function と strings.Clone を使い、split の view で所有値を省略しません。
C／C++ の添字は無検査、Rust・Zig・Go・Tsuzuri は通常の検査です。SSO、関数の小さい格納、
文字列バッファの再利用、GC の差は、各実装の結果として残します。
Map／Set は HashMap ではなく全実装が整列した配列です。Tsuzuri は get と insert の 2 回の探索、
参照は見つけた場所の更新という違いを明示します。

**木の物理表現:** Tsuzuri のペイロードを持たない Leaf は null です。参照実装は所有の leaf node を作ります。
binary_trees の quick（size 4、seed 42）は抽象の節が 1,916、Tsuzuri の確保が内部節の 956 回です。
同じ数の物理的な確保をした比較とは主張しません。論理的な木、節の数、構築と解放の処理は同じです。

generic の native `-O3` は 25 境界ケース × 10 種目 × 6 言語（1,500）、
Tsuzuri native `-O0` は 250、WASM `-O0`／`-O3` は各 250 を照合し、WASM の import は空です。
測定する seed と仕事量も独立な参照と照合します。tracked は種目ごとに実際の大きさで 1 回呼び、すべて live 0 を検査します。
f64 の IR 検査は再結合・縮約などの意味を緩める flags を拒否します。Rust 1.99／LLVM 23 が
`1.0 / (u64 as f64)` に推論する `nnan` だけは、同じ関数の SSA の変換元を確かめて許可します。
この式は分母が 0 でも infinity となり、NaN は生じません。ほかの `nnan` や flags は引き続き拒否し、
fast-math を有効にしたわけではありません。
peak RSS は言語ごとの host を `--only` で 1 種目だけ起動し、その checksum も確認します。
全 6 言語なら schema 1 が 200 レコード（wall・CPU・RSS 各 60、確保各 10）、
Go・Zig がないときは理由付きで 140 レコードです。明示した使えないツールはエラーです。

Zig は 0.16.0 の ReleaseSafe と `-fcompiler-rt`、Go は 1.27.2 の通常の GC と自動 GOMAXPROCS で確認しました。
Zig の出力を Clang でリンクするため、x64 の `__zig_probe_stack` などの必要な compiler runtime を含めます。
Linux は `zig build-lib -static` で kernel と compiler runtime を別 member の archive にします。
Zig 0.16.0 の Linux x64 で、runtime を含む単一 object に binary・IR・assembly を同時に出すと compiler が crash するためです。
Darwin／Windows は object のままです。どちらも最適化後の IR と assembly を出し、
ReleaseSafe・baseline CPU・stack の page probe と安全検査を維持します。
Go の cgo の境界と background GC の CPU は計時に含みます。
`--cpu native` は C／C++・Rust・Tsuzuri・Zig の指定を変えますが、Go の指定は変わらず、その差を JSON に出します。
`--cpu native` の比較は別の条件です。ここで確認したのは generic です。
LLVM IR／assembly と実コマンドは `--artifacts`、版・Go の環境は stdout JSON に残します。
CI の quick は正しさと記録だけで、WASM の速度、GPU、並列による加速の主張ではありません。

## 第2フェーズの生成コードと確保（2026-10-14）

PB01・PB02・PR01・PM06・PM07（A17 を含む）・PR04・PR05 の比較の基準は `a621fada` です。
同時に入る変更なので、全体の before／after を個別の最適化だけの速度差として扱いません。
共有 CI には正しさ・境界条件・fallback の検査だけを置き、速度の合否閾値は置きません。

フロントエンドは `run-frontend.mjs <compiler> --sizes 200,1000 --out <dir>` で
hello と 200／1,000 モジュールの check・LLVM 出力、時間と RSS を各 9 標本で測ります。
runtime の cold／warm は `run-build.mjs <compiler> --sizes 10 --languages tsuzuri --out <dir>` の
10 モジュールと既存の実プロジェクト 11 件を使い、同じ条件で before／after／before2 を測ります。
大規模 runtime ビルドの反復はフロントエンド・バックエンドの費用が支配するためこの比較から分離しました。
中断した 200 モジュールの runtime 測定は結果に含めず、200／1,000 モジュールの runtime cache の改善率は主張しません。
warm は frontend cache も保持するため、cold／warm 差を runtime object だけの効果とは扱いません。
既知の大規模 WASM `-O3` 制限は、新しい `build-runtime-cold`／`build-runtime-warm` にも同じ除外規則を適用します。
同梱 object の正しい消費・移設・破損 fallback は配布 smoke で検証し、この cache 比較とは区別します。

UTF の入力別比較は `run-utf.mjs <compiler> --metrics <dir>` を使います。
5 入力種 × 6 操作 × 8／64／65,536 UTF-16 単位、約 2^24 単位の反復、9 標本と独立した checksum の検査を
PX01 suite `utf` に記録します。`utf_compare` は各反復でオペランドを `Bench.consume` のメモリ障壁に通し、
比較が `-O3` でループ外へ移動しないことを最適化済み IR で確認します。`--quick` の値は速度比較に使いません。

### フロントエンドの正式比較（PB02・PR01）

`target/perf/PR01-frontend-final2/{before,after,before2}`、各 9 標本です。
Apple M1 Max、Apple Clang 21.0.0、Rust 1.98.1、Node 24.21.0、cache 無効で直列実行しました。
after の compiler SHA-256 は `00d5d6a434fe6a3830741adc9f86e614f065794aaa426de0152adea6160e07c9`、
before／before2 は `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc` です。
以下は中央値（最小–最大）。統合版と基準の比較であり、PB02 単独の改善率ではありません。

実時間、ms:

| 対象 | before | after | before2 |
| --- | ---: | ---: | ---: |
| hello check | 97.572（58.661–161.132） | 85.348（47.740–122.899） | 88.304（59.337–99.481） |
| hello LLVM 出力 | 89.508（67.863–121.940） | 87.156（57.425–146.910） | 93.979（69.580–121.752） |
| 200 check | 421.530（414.065–469.922） | 352.182（347.980–427.887） | 430.808（417.607–510.015） |
| 200 LLVM 出力 | 584.110（574.613–630.498） | 583.162（564.452–632.132） | 590.072（571.226–643.927） |
| 1,000 check | 2147.114（2128.754–2188.450） | 1872.570（1832.719–1919.954） | 2145.012（2121.358–2189.393） |
| 1,000 LLVM 出力 | 2897.476（2862.491–3790.169） | 2907.470（2870.968–3005.364） | 2910.752（2862.189–2954.357） |

1,000 check は約 12.8% 短縮し、before2 の広がりも超えます。200 check は中央値こそ短縮しましたが
before／after の広がり 79.907 ms 内なので実時間の判定は「差なし」です。hello と LLVM 出力全体も「差なし」です。
CPU 時間は計数の粒度が 10 ms です。hello check は 40（40–40）→30（30–30）→40（40–40）、
200 check は 390（390–400）→330（330–330）→400（390–400）、
1,000 check は 1890（1880–1900）→1600（1590–1630）→1890（1880–1900）ms でした。
LLVM 出力全体の CPU 時間は hello 40→40→40、200 は 550→550→550、1,000 は 2630→2640→2640 ms で、いずれも判定は「差なし」です。

最大 RSS、MiB:

| 対象 | before | after | before2 |
| --- | ---: | ---: | ---: |
| hello check | 25.188（24.891–25.375） | 20.828（20.641–20.891） | 25.203（24.828–25.391） |
| hello LLVM 出力 | 26.672（26.312–26.875） | 23.547（23.125–23.938） | 26.688（26.219–26.922） |
| 200 check | 214.484（212.453–215.172） | 141.094（139.516–142.188） | 214.344（213.391–215.531） |
| 200 LLVM 出力 | 223.734（221.484–224.859） | 154.109（152.953–155.516） | 223.062（221.109–224.922） |
| 1,000 check | 965.781（950.500–967.625） | 615.609（614.906–620.109） | 965.531（950.844–967.406） |
| 1,000 LLVM 出力 | 1006.953（997.641–1008.812） | 662.984（662.453–667.328） | 1002.016（996.469–1008.453） |

RSS は全 6 構成で改善しました。ただし長期目標の 1,000 モジュール 0.5 秒／250 MB は未達です。

出力フェーズ単独、ms:

| 対象 | before | after | before2 |
| --- | ---: | ---: | ---: |
| hello | 2.486（2.422–2.617） | 4.742（4.624–5.121） | 2.523（2.473–2.645） |
| 200 | 139.695（138.153–142.804） | 200.405（198.719–201.981） | 139.540（138.372–142.565） |
| 1,000 | 679.220（676.331–689.820） | 969.691（961.715–1021.099） | 683.645（679.327–698.057） |

このフェーズは全構成で悪化しました。属性・metadata と拡張 runtime の処理費用を含み、前段の改善で全体は相殺されています。
途中版の出力 CPU の約 17.5% の増加は、facts の parse-once／候補限定／遅延証明、metadata を生成しない場合の再走査省略、
UTF の広幅 vector で有効になる名前付き型走査の早期除外で縮めました。最後の修正は 36 の native/WASM LLVM module で byte 同一、
SIMD の 4 回帰テストでも確認しました。残る費用を「退行なし」「出力フェーズも高速化」とは表現しません。

### フロントエンドのプロファイル（PB02）

基準 `a621fada` と統合後を、release・thin LTO・CGU 1 のまま `strip=false` にした別のビルドで調べました。
cache 無効、`LC_ALL=C`、`RUST_MIN_STACK=4194304`、合成 1,000 モジュールの `check` を各 3 回、
`/usr/bin/sample PID 10 1 -mayDie` で採取しました。重い所有プロセスとは重ねていません。
生データと top ten は `target/perf/PB02-profiles-final/{summary.json,summary.tsv,*-final-check-*.sample.txt}` です。
起動直後の exec に sampler が失敗した試行と最初の cold profile は除外し、同じ準備手順で採り直した 6 本だけを使います。

| 指標 | before 中央値（最小–最大） | after 中央値（最小–最大） |
| --- | ---: | ---: |
| allocator と memmove/memcpy/memset の top-stack 標本比率 | 48.406%（46.695–49.150） | 31.627%（30.071–45.385） |
| 名前で malloc/free/memmove だけを数えた比率 | 46.057%（44.893–46.154） | 29.106%（28.714–42.308） |
| `specialize` inclusive 標本数 | 120（120–120） | 62（62–63） |
| `instantiate` inclusive 標本数 | 42（42–50） | 46（39–48） |
| `closures::lower` inclusive 標本数 | 16（13–20） | 13（12–13） |
| `ownership::check_functions` inclusive 標本数 | 156（156–160） | 126（125–127） |
| `children_mut` inclusive 標本数 | 7（6–7） | 未観測 |

比率の分母は profiler が掲載した top-stack 行（5 標本以上）の合計です。
inclusive は最も浅い同名 frame の値で、再帰出現を足しません。未観測は処理がゼロである証明ではありません。
`specialize` のコンパイラ frame 内の順位は 11/11/10 → 15/16/18 で、旧計画の「常に上位 10」は再現しませんでした。
`instantiate` の減少目標も未達です。I/O と標本数の揺れがあり、この比率をそのまま速度の改善率にはしません。
新設 CI の fresh quick equivalence は 22 project・88 CLI 比較・60 IR 出力で差分 0、397.74 秒でした。
CI の 25 分上限と frontend smoke の 5 分上限は速度の合否閾値ではなく、処理全体のタイムアウトです。

### 数値表示・解析の参照検証（PR04）

生成テーブルは整数演算で再現し、追加テーブルは 20,288 bytes です。
表示・解析の文字列とビットは保存した基準ランタイムおよび独立した参照と照合しました。
native/WASM の `-O0`・`-O3` で各 109,803 件の表示と、25 文字境界を含む 147,480 件の解析・往復を検証し、
native `-O3` では f32 の全 4,294,967,296 ビット列と f64 の 100,000,000 件を調べ、不一致は 0 件でした。
NaN は既存の canonical NaN 規則で比較し、符号付きゼロ・非正規化数・境界丸めを含めます。
これは正しさの証拠であって速度の優位性を示す値ではありません。
共有 CI は `tests/display_parse.mjs` と、`display_parse_exhaustive.mjs` の各型 100,000 件の
有界差分検査を行い、全 f32 列挙や 1 億件の f64 は通常の CI には入れません。

### 数値の正式比較と C の採否

最終結果は `target/perf/phase2-formal/numeric-final2-{before,after,before2}`（各 78 レコード、native CPU-time と WASM wall-time は各 9 標本）です。
Apple M1 Max、Apple Clang 21.0.0、Node 24.21.0、native/WASM `-O3`、warmup 後に直列実行しました。
raw runtime の比較であり、生成プログラム全体の速度や他 CPU の速度ではありません。
`tests/display_parse_runtime.c` の POSIX native `clock()` は process CPU time、Windows native の CRT `clock()` は経過時間を測り、WASM は `performance.now()` による wall time です。numeric benchmark の native records は POSIX で `cpu_time`、Windows で `wall_time` として記録します。以前の最終記録では POSIX native の `clock()` 標本を `wall_time` と誤ラベルしていました。保存済みの標本値と判定は変えず、native records の metric metadata を `cpu_time` に訂正しました。
以下は native CPU time の ns/回、中央値（最小–最大）です。全入力と採否の式は [PR04 の記録](../_perfs/_completed/PR04-number-formatting.md#正式計測と最終採否2026-10-14) にあります。

| workload | before | after | before2 |
| --- | ---: | ---: | ---: |
| `f32-format-pi` | 1463.755（1456.595–1469.570） | 24.735（24.530–25.070） | 1473.025（1465.415–1479.660） |
| `f64-format-tenth` | 806.055（802.240–815.655） | 35.850（35.465–36.720） | 809.495（806.235–821.485） |
| `f64-format-min` | 4824.900（4808.765–4863.460） | 18.080（17.940–18.490） | 4848.770（4816.740–4880.070） |
| `f64-parse-pi` | 1243.265（1238.835–1247.440） | 28.515（27.795–28.995） | 1249.500（1244.020–1265.125） |
| `f64-parse-min` | 1432.585（1425.755–1441.780） | 10.965（10.855–11.415） | 1435.860（1431.165–1450.010） |
| `f64-parse-long`（対照） | 1367.795（1365.700–1373.915） | 1371.530（1347.705–1377.085） | 1374.530（1367.015–1432.200） |
| `i64u-format-max`（対照） | 24.964（22.227–26.439） | 24.359（23.310–26.196） | 24.107（23.326–26.027） |

固定 native 入力の表示は 22.48–266.86 倍、解析は 43.60–130.65 倍です。これは入力ごとの比率の範囲で、平均倍率ではありません。
WASM は f32/f64 解析の 7 行と表示の 5 行で改善 gate を通過しましたが、`f32-format-pi/max/min` と
`f64-format-1e23/min` は before／before2 の drift が標本の広がりを超え、改善を確定できませんでした。
f16/f128/decimal・整数・long/underscore の対照は native/WASM とも安定し、gate 外の悪化は 0 でした。

段 C の追加整数 formatter は**不採用**です。候補の native max は 25.450 → 23.286 ns（before2 26.203 ns）でしたが、
2.164 ns の差は before/after の広がり 2.718 ns と baseline の広がり 3.796 ns の双方を超えません。
small は差なし、WASM の max/small も採用 gate を超えず、平均して採用することはしませんでした。
候補を除去して整数の本体を基準と同じ `% 100` に戻し、十進 format-spec と通常 formatter の共通経路だけを残しました。

途中の比較では long input の投機的再走査が 20.024 ns の悪化（広がり 12.531 ns を超過）を起こしました。
先頭符号を除き 25 ASCII 文字を超えた入力は scanner に入れず exact parser へ送り、
24/25/26 文字、符号、19/20 有効桁、不正末尾の参照 60 件を追加して修正しました。
中間の candidate と `numeric-final-*` は診断用の記録であり、上の最終表には混ぜていません。

| standalone numeric object | before bytes | after bytes | 増分 bytes |
| --- | ---: | ---: | ---: |
| native O0 | 84,152 | 109,568 | 25,416 |
| native O3 | 50,800 | 73,016 | 22,216 |
| wasm32 O0 | 83,958 | 108,611 | 24,653 |
| wasm32 O3 | 69,915 | 92,736 | 22,821 |

表は runtime object 全体の増分で、不変表 20,288 bytes だけの値ではありません。
PB01 による math の分離・section GC がリンク後のプログラムを小さくする場合とは区別します。
最終 numeric/math の metadata 最大番号は 220/288、宣言と番号の重複がなく決定的であることも検査しました。

### 文字列と局所環境（PM06・PM07）

`tests/fixtures/storage/Storage.tz` を保存した基準コンパイラと変更後のコンパイラで LLVM IR にし、
`benchmarks/tracked_alloc.c` によって `malloc`・`realloc`・`free` を追跡しました。
Clang の最適化より前に追跡を入れ、native の `-O0` と `-O3` で次の回数・要求 bytes が一致し、
全呼び出しの後で `live == 0` でした。これは**コンパイラが出した確保要求の比較**です。
上のコンピュテーション式の「最適化後の IR に追跡を入れる」計数とは区別してください。

| 呼び出し | before 要求回数 | after 要求回数 | before 要求 bytes | after 要求 bytes |
| --- | ---: | ---: | ---: | ---: |
| `literal_concat_stack()` | 1 | 0 | 22 | 0 |
| `length_only_concat(1)` | 1 | 0 | 10 | 0 |
| `buffer_concat(1)` | 1 | 0 | 10 | 0 |
| `utf8_buffer_concat(1)` | 1 | 0 | 5 | 0 |
| `buffer_escape(1)`（対照） | 1 | 1 | 10 | 10 |
| `boundary_concat(0)`（128 code unit） | 2 | 1 | 510 | 254 |
| `boundary_concat(1)`（129 code unit） | 2 | 2 | 514 | 514 |
| `local_closure(10)` | 1 | 0 | 16 | 0 |
| `field_take()` / `field_take_nested()` / `field_update_reuse()` | 2 | 1 | 48 | 24 |
| `field_take_shared()`（対照） | 2 | 2 | 48 | 48 |
| `concat_stack()` | 2 | 0 | 22 | 0 |
| `capture_stack()` | 4 | 3 | 70 | 38 |
| `string_append_owned()` | 7 | 7 | 52 | 52 |
| `utf8_append_owned()` | 7 | 7 | 26 | 26 |

最後の 2 行は、左辺の再利用が確保 API の要求回数を減らすこととは同義でない例です。
変更後は 4 回の新規確保要求と 3 回の既存バッファへの `realloc` です。
`tests/primitives.mjs` の計数は後者を新規の `malloc` と別に扱いますが、
`tracked_alloc.c` は `realloc` も要求 1 回として数えます。`realloc` は実際には移動することもあり、
物理的な確保回数や繰り返し追記の償却定数時間を保証しません。

Apple Clang 21、LLVM 21 の `opt -O3` と arm64 の逆アセンブルも確認しました。
変更後の `tz_buffer_concat` には 256 bytes の局所領域が残り、`malloc` の呼び出しはありません。
基準版には `malloc` と結果の null 検査があります。
一方、`length_only_concat` と `local_closure` は基準版でも LLVM の `-O3` で確保が消えるため、
それらについて通常の `-O3` 実行が新たに速くなったという主張はしません。
`tests/features.mjs ... storage` は 24 件（`to_string<string>` の identity、局所連結、明示 clone を含む）を native/WASM の `-O0`・`-O3` で照合し、
native では `ASAN_OPTIONS=detect_stack_use_after_return=1 TSUZURI_ASAN=1` でも確認します。
再帰・Task・大きすぎる環境・4 KiB の予算超過時の経路は `tests/storage.rs` で別に検査します。

`benchmarks/allocation/Main.tz` は、長さだけの連結、局所連結、局所環境、外へ逃がす対照を各 100 万回実行します。
期待する出力は独立に計算した `667000000` です。プロセス起動込みの時間と RSS は次の共通 runner で再現できます。

```sh
node benchmarks/metrics.mjs process BEFORE_COMPILER --project benchmarks/allocation --out target/perf/PM06-before
node benchmarks/metrics.mjs process target/release/tsuzuri --project benchmarks/allocation --out target/perf/PM06-after
node benchmarks/metrics.mjs report target/perf/PM06-after --baseline target/perf/PM06-before
```

### 同一入力の複製一覧（PM07・A17）

`target/perf/A17-completion/` の棚卸しは、同じ 122 プロジェクト・172 ソース・47 std ファイルを
基準と統合版の両方で調べました。bindgen の既存の除外も同一で、成功した部分集合だけの比較ではありません。
既存の `tests/copy_inventory.rs` と同じ `CopyCost::Length` の抽出で、次の結果でした。

| 起源 | before | after | 差 |
| --- | ---: | ---: | ---: |
| user | 150 | 62 | −88 |
| std | 138 | 60 | −78 |

新たなソース行位置はありません。これは静的な複製位置の数であり、実行回数・確保回数・bytes・処理時間ではありません。
起票時の user 120／std 138 は 116 プロジェクトの履歴値なので、この matched 比較とは混ぜません。
ガードと Drop の保守的な除外を含む結果で、残る複製をすべて除去したという主張ではありません。

### UTF runtime の最終比較（PR05、2026-10-15）

Apple M1 Max、Homebrew Clang 21.1.8、Node 24.21.0。baseline compiler SHA-256 は `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`、最終 after compiler は `de42dba363e1fa877c0dab5bd51cbad17b60b774bed5b91399d9d996a99d9d08`。各比較は before／after／before2 を使い、M1 は既存4種目の native CPU 時間（ms／call、9 独立 run の中央値）です。

| workload | before | after | before2 | after vs. before / before2 |
| --- | ---: | ---: | ---: | ---: |
| UTF-8 roundtrip | 0.786 | 0.118 | 0.771 | −84.94% / −84.65% |
| UTF-16 compare | 0.139 | 0.084 | 0.135 | −39.74% / −38.23% |
| UTF-16 validate | 0.411 | 0.087 | 0.413 | −78.83% / −78.91% |
| UTF-16 scan | 0.051 | 0.041 | 0.052 | −20.24% / −21.19% |

M2 は 90 workload × 9 wall samples／pass（270 records、2,430 samples）で、各操作・入力・サイズを別々に判定しました。stable adverse は **0**。35 行は両 baseline に対して improved、34 行は両方と no difference、21 行は片方と improved・もう片方と no difference で、regressed 判定はありません。M2 は wall-time のみで CPU metric はありません。`to_well_formed` の計測 kernel は結果の長さしか観測せず、修復内容が dead-code elimination され得るため、処理速度の根拠にはしません。raw hashes と全90行の samples／判定は [`PR05-utf-simd.md`](../_perfs/_completed/PR05-utf-simd.md) に記録しました。

M3 の確保数／bytes は before・after・before2 で完全一致（roundtrip 32／2,359,248、compare 33／3,145,684、validate 32／2,621,396、scan 30／1,572,816）。M4 `-O3` object sizes は native 79,424 → 134,368 bytes、WASM scalar 69,230 → 78,041 bytes、WASM simd128 71,070 → 87,571 bytes（before → after）で、SIMD runtime による成果物の増加も記録します。これらの比較は同じ機械・入力の結果で、他 CPU や共有 CI の速度保証ではありません。

### 第2フェーズ統合の最終比較（2026-10-15）

Apple M1 Max（arm64、AC 電源）、Node 24.21.0、Homebrew Clang 21.1.8 で、clean commit `9ecafb33ff0d666cf2ff9335c9dc0cbde8e917e5` から before／after／before2 を直列に実行しました。各 timed variant は 9 samples、全 run は clean。baseline compiler SHA-256 は `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`、after は `de42dba363e1fa877c0dab5bd51cbad17b60b774bed5b91399d9d996a99d9d08` です。以下は `benchmarks/metrics.mjs report` の spread 判定で、microbenchmark を言語全体の順位や他機種の保証とは解釈しません。

| suite | 比較行 | before→after: 改善 / 悪化 / 差なし | baseline 2 回との一貫した改善 / 悪化 |
| --- | ---: | ---: | ---: |
| allocation | 71 | 13 / 7 / 51 | 10 / 7 |
| control | 160 | 0 / 0 / 160 | 0 / 0 |
| computations | 141 | 3 / 1 / 137 | 3 / 0 |
| C++/UTF | 96 | 1 / 0 / 95 | 0 / 0 |
| build | 2,198 | 281 / 178 / 1,739 | 204 / 113 |

Build suite の before／before2 では 2,198 行中 23 行に baseline drift がありました。従って build の改善・悪化は、after が両 baseline に対して同じ判定を得た行だけを「一貫」と数え、残りは確定的な変化と扱いません。allocation の悪化 7 行は主に no-cache build の emit 時間・生成物サイズで、実行 RSS の O3 は改善しました。control は全行が差なし。computations の単独悪化は対象外の `std_option` native O3 process CPU 行で、両 baseline に対して再現しませんでした。C++/UTF は gate 外の悪化 0 です。

PB01 の runtime reuse を測る `build-runtime-warm` では、`examples/hello` native O3 process CPU median が 960 → 430 ms（before2 1,010 ms）、WASM O3 が 890 → 350 ms（before2 960 ms）でした。native は before と before2 の双方に対して改善、WASM は baseline の広がりを含むため CPU の結論を限定します。wall-time は両者とも spread 内で「差なし」であり、wall-time の改善を主張しません。cold build の初回費用と warm runtime object cache の再利用は別の variant です。

`--no-cache` の通常 build では統合後の cold codegen cost と executable size の増加が残ります。native O3 の `examples/io` は process CPU 380 → 760 ms（before2 430 ms）、executable 50,840 → 69,976 bytes（before2 50,840 bytes）。synthetic 10-module は 1,330 → 1,610 ms（before2 1,410 ms）、101,128 → 153,528 bytes（before2 101,128 bytes）でした。CPU とサイズは両 baseline より悪化しました。これは全 streams を統合した no-cache 初回ビルドの実測であり、PR05 単独への因果帰属はしません。prebuilt/runtime-cache 経路はこの費用を避けるためのもので、cold compiler cost や成果物サイズが無償になるわけではありません。

正式 run の raw data は再現用 `target/perf/phase2-formal/final-verified-9ecafb3/` にあります（通常は git 管理外）。個別チケットの受け入れ範囲・意味論検査は PB01、PB02、PR01、PM06、PM07/A17、PR04、PR05 の各完了記録に記載します。共有 CI に性能の pass/fail 閾値は追加していません。

## 成果物のサイズ

`node benchmarks/metrics.mjs size [compiler] --out DIR [--project DIR]... [--quick]` は、利用者が受け取る成果物の大きさを PX01 の JSONL で記録します（PM08）。サイズは決定的なので 1 点で、各ビルドを 2 回作って同一かを確かめます。合否の基準は、ページ境界に丸められるファイルの大きさではなく section の byte 数です（Mach-O の `__text` と `__const`＋`__cstring`、ELF の `.text` と `.rodata`、wasm の CODE と DATA。`llvm-objdump -h` で読み、`TSUZURI_OBJDUMP` で指定します）。主な値は cache あり（利用者が実行する既定のビルド）で、`--no-cache` の埋め込み形式は別の variant として示します。速度の合否閾値と同じく、共有 CI にサイズの閾値は置きません。

### 整数表示の分割（PM08 D2）

整数の `to_string`・`Display`・補間・式の結果の表示を、`numeric.ll` から手書きの `runtime/integer.ll`（約 0.4〜0.5 KB）へ移しました。整数だけを表示するプログラムは、数値 object とその表（約 10 KB）を連結しなくなります。出力の文字列は変わらず、`tests/integer_format.mjs`（Python の独立した参照 47,029 件と、8／16 bit の全値）と `tests/size.mjs`（コンパイラ経由）で、native と WASM の `-O0`／`-O3`／`-Os`／`-Oz` を照合します。

Apple M1 Max（macOS arm64）、Apple clang 21.0.0、Homebrew LLVM 21 の `llvm-objdump`。before は main `6d15b437` のリリースビルド（SHA-256 `da78ba18…`）、after は branch `perf/phase3/PM08` の整数表示の分割まで入れた非 LTO のリリースビルドです（コンパイラの最適化はその出力に影響しません）。測定時の load average は 50〜150 でしたが、サイズは決定的で load に依存しません。`examples/hello` の `-O3`、cache あり:

| 成果物 | before | after |
| --- | ---: | ---: |
| native 実行ファイル | 50,664 | 33,896 |
| native `__text` | 13,304 | 2,604 |
| native `__const`＋`__cstring` | 10,160 | 64 |
| wasm32 モジュール | 35,714 | 6,357 |
| wasm CODE | 25,564 | 6,142 |
| wasm DATA | 9,882 | 0 |
| `--emit object`（native） | 72,032 | 4,896 |
| native 実行ファイル、`--no-cache` | 117,848 | 50,824 |

`-Os`／`-Oz`（`build` と `run`）は、同じ IR を Clang の別の最適化レベルでコンパイルします。`examples/hello`、cache あり、byte。ここでの before は整数表示の分割の前（`-Os`／`-Oz` を加えた時点）のコンパイラです:

| 成果物 | レベル | before | after |
| --- | --- | ---: | ---: |
| wasm32 モジュール | `-O3` | 35,714 | 6,357 |
| wasm32 モジュール | `-Os` | 31,820 | 2,995 |
| wasm32 モジュール | `-Oz` | 31,428 | 2,587 |
| native `__text` | `-O3` | 13,304 | 2,604 |
| native `__text` | `-Os` | 12,860 | 2,680 |
| native `__text` | `-Oz` | 11,320 | 2,484 |

native の実行ファイルの大きさは macOS のページ境界（16 KiB）と libSystem の stub を含み、`benchmarks/size/hello.c`（5050 を `printf` で出す C のプログラム。`-O3`）も 33,432 byte です。そのため、`-Oz` や `--strip` による section の縮小はファイルの大きさにはほとんど現れません（`--strip` は `examples/hello` で 33,896 から 33,784 byte、`-Oz --strip` は 33,752 byte）。native でサイズを比べるときは section の byte 数を見てください。

`--wasm-opt`（Binaryen の `wasm-opt -Oz` を、リンク後の `.wasm` に通す）は、Binaryen 132（npm の `binaryen` パッケージの `wasm-opt`）で測りました。`wasm32`、cache あり、byte で、`wasm-opt` なしは同じレベルの値です:

| プログラム | `-O3` | `-O3 --wasm-opt` | `-Oz` | `-Oz --wasm-opt` |
| --- | ---: | ---: | ---: | ---: |
| `examples/hello` | 6,357 | 5,196（−18.3%） | 2,587 | 2,156（−16.7%） |
| `examples/io` | 15,593 | 13,807（−11.5%） | 5,734 | 4,583（−20.1%） |
| `examples/functional` | 12,086 | 10,187（−15.7%） | 3,960 | 3,094（−21.9%） |
| `examples/currying` | 6,326 | 5,169（−18.3%） | 2,555 | 2,129（−16.7%） |

どの行も、import、export、メモリの大きさ、`tz_` の export の結果は元のモジュールと同じでした（`tests/size.mjs` は `TSUZURI_TEST_WASM_OPT` に本物の `wasm-opt` があるとき、同じ検査を実行します）。Binaryen は配布物に同梱しません。配布用のアーカイブが Linux と Windows で約 105〜111 MB あり、配布物が大きくなりすぎるためです。

整数の変換の速さは、同じ値の列を `tz_soft_format` と `tz.integer.format` に渡す C の測定（2,000 万回、`-O3`、native）で同等でした。ただし load average が約 95 の共有機での測定で誤差が大きいため、速度の主張には使いません（0〜999 の値は 9.1 から 5.7 ns／回、全幅の `i64` は 26.3 から 24.8 ns／回、`i128` は 60.7 から 54.0 ns／回）。静かな機械での再測定が必要です。

### リンク時の GC（埋め込みのビルド）

ランタイムを成果物キャッシュなしでモジュールに埋め込む形式（`--no-cache`、`--trap-info`。`run` は `--trap-info` が既定）の native 実行ファイルは、これまで未使用の関数をリンク時に除去していませんでした（runtime object を使う既定のビルドだけが `-dead_strip` などを渡していました）。`-g` を除くすべての native 実行ファイルのリンクに同じ除去を適用し、Linux と Windows の GNU 系リンカーでは関数とデータを別々の section に出します。条件は上の整数表示の分割と同じです。section の byte 数（`__text`／`__const`＋`__cstring`）、`-O3`、macOS arm64、変更前は整数表示の分割まで入れたコンパイラです:

| プログラム | variant | `__text` 変更前 → 変更後 | `__const`＋`__cstring` 変更前 → 変更後 |
| --- | --- | ---: | ---: |
| `examples/point` | `--no-cache` | 40,552 → 10,844 | 20,884 → 10,096 |
| `examples/point` | `--trap-info` | 40,288 → 11,020 | 20,996 → 10,080 |
| `examples/io` | `--no-cache` | 27,316 → 9,780 | 329 → 301 |
| `examples/io` | `--trap-info` | 30,132 → 12,596 | 3,241 → 3,220 |
| `examples/tasks` | `--no-cache` | 6,008 → 3,496 | 596 → 314 |
| `examples/tasks` | `--trap-info` | 7,836 → 5,324 | 2,660 → 2,378 |
| `examples/hello` | `--no-cache` | 3,088 → 2,604 | 64 → 64 |
| `examples/hello` | `--trap-info` | 3,932 → 3,400 | 991 → 991 |

どの variant も、変更後の `__text` は cache ありの既定のビルド（`examples/point` は 10,844、`examples/io` は 9,780）と同じか、`--trap-info` の計装のコードの分だけ大きい値です。プログラムの出力と `--trap-info` の位置表（`.trap.json`）は変わらず、`tests/size.mjs` は `point` の埋め込み形式の実行ファイルから、使わない数値の解析（`tz_soft_parse`）が消えていることを `nm` で確かめます。Linux と Windows のリンカーでの効果は、この機械では測っていません（CI のジョブが実行ファイルの出力と動作を確かめます）。

## 現実的な次の指標

性能目標の完全な達成に向けては、代表的な実アプリケーションワークロード、主要アルゴリズムカーネル、コンパイル所要時間、生成バイナリのフットプリント、ホスト境界呼び出しのオーバーヘッド、ならびにピークメモリ消費量（RSS）の継続的なベンチマーク測定が不可欠です。
新しいデータ構造やアルゴリズムを導入する際は、メモリのディープコピーやアロケーションコストも含めて、比較対象言語との前提条件を厳密に揃えて評価してください。
将来的に CI パイプラインへ性能退行の検出閾値を組み込む場合であっても、ノイズの少ない安定した専用ハードウェア環境と、複数回試行に基づく統計的検証が前提となります。
共有 CI ランナーに対して恣意的な速度合否閾値を設けることはしていません。

目標の達成には、代表的なアプリケーション・カーネル、コンパイル時間、成果物サイズ、
ホスト境界、最大メモリ使用量を継続測定する必要があります。
新しいデータ構造では、コピーや確保のコストも含めて比較対象と条件を揃えてください。
性能の回帰閾値を CI に入れる場合も、安定した専用ハードウェアと複数回の測定が必要です。
共有 CI ランナーに速度の合否閾値は設定していません。
