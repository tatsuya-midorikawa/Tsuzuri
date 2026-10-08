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
常駐実行時間にはデータ転送と最終読み戻しは含まれませんが、各ディスパッチごとの同期オーバーヘッドは含まれます。表内の CPU 列は WASM スカラーエクスポート関数のホスト呼び出し時間であり、GPU によるバルク一括処理との直接の速度比を示すものではありません。
2026-09-28 には、Dawn Node binding 0.6.1 の Metal バックエンド上で、厳密な `i32` シェーダーの初期化、マッピング、および常駐チェーンの参照照合を実施しました。速度の優位性は主張しておらず、浮動小数点の GPU 実行や自動オフロードは現時点で対象外です。
制約の根拠は [WGSL 仕様](https://gpuweb.github.io/gpuweb/wgsl/#floating-point-accuracy) に基づきます。ハードウェアネイティブな 64-bit 整数型は存在せず、浮動小数点演算においては式の再結合、融合演算、および非正規化数の扱いに差異が許容されているためです。

## 表の読み方

ビルド全体キャッシュ（whole-build cache）の合成ベンチマークは `node benchmarks/run-cache.mjs target/release/tsuzuri` で実行します。200 モジュールのプロジェクトを用いて、コールド状態（uncached）とウォーム状態（warm）における 7 回試行の中央値をミリ秒（ms）単位で計測し、出力バイナリのバイト単位の一致および実行チェックサムを照合します。
`--quick` オプションは 10 モジュール／2 回試行の動作検証のみを行います。2026-09-28 にこの実行経路を確認していますが、短縮された所要時間から一般的なビルド速度の優位性を安易に主張することはありません。
計測時間には、パース、型検査、IR 生成、コンパイラおよび依存ツールのフィンガープリント計算、ファイルコピー、および成果物の公開処理の時間がすべて含まれます。構文木（AST）単位のインクリメンタルパースキャッシュではないため、キャッシュヒット時であってもこれらの基盤処理時間は残存します。

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
<p>最終測定: <code>2026-09-26T12:02:10.992Z</code><br>CPU: Apple M1 Max / arm64 / native<br>Tsuzuri: <code style="overflow-wrap:anywhere">429015c246e6aa57f3e74f156750a671dfde173a4e002702080b9ef850db6bc9</code><br>C#: .NET 10.0.2 / JavaScript: v24.21.0</p>
<div style="overflow-x:auto;max-width:100%">
<table style="border-collapse:collapse;width:100%;font-size:13px">
<caption style="text-align:left;padding:8px 0;font-weight:600">36種目 / native / scale 0.1 / 3回の中央値</caption>
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
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.260250</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.257583</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.260154</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.263000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.510545</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.485708</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9978837532235666" title="最速時間 / Tsuzuri時間 = 0.9978837532235666"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99788</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/for_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.261353</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.260824</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.258500</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 2.011300</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.506107</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.292542</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9977381430892066" title="最速時間 / Tsuzuri時間 = 0.9977381430892066"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99774</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.254824</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.259125</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.257176</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.261824</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.509107</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">37.240042</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_if_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.259176</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.256813</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.257562</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.257625</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.506693</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">36.941292</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9981233759220315" title="最速時間 / Tsuzuri時間 = 0.9981233759220315"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99812</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/tail_builtin_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.259111</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.255437</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.259375</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.258933</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.504907</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">36.971625</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9970820682211495" title="最速時間 / Tsuzuri時間 = 0.9970820682211495"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99708</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/match_dispatch</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.252333</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.254538</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.255125</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.254550</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.475707</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">46.226708</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.166512</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.166639</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.167333</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.165629</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.582006</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.260520</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9946970788892092" title="最速時間 / Tsuzuri時間 = 0.9946970788892092"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99470</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/array_copy</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.134046</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.133827</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.139691</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.138662</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.398535</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.241396</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9983662324873551" title="最速時間 / Tsuzuri時間 = 0.9983662324873551"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99837</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/list_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">10000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.177991</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.180562</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.179646</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.180439</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.238915</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.519678</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.188726</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.188625</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.188880</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.189519</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.193991</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 5.226052</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.999464832614478" title="最速時間 / Tsuzuri時間 = 0.999464832614478"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99946</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/closure_churn</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">100000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.633667</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.627406</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.627212</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.896304</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 2.353489</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">11.757896</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9898132615395784" title="最速時間 / Tsuzuri時間 = 0.9898132615395784"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98981</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/record_pipeline</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.257353</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.257813</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.258000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.256353</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.510843</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">36.653375</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9992046783997812" title="最速時間 / Tsuzuri時間 = 0.9992046783997812"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99920</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/integer128_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.572865</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.510550</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.511195</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.526615</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.698290</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">12.155375</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.8912221902193361" title="最速時間 / Tsuzuri時間 = 0.8912221902193361"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.89122</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float32_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.879583</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.879087</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.878391</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.879708</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.880973</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 2.389458</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9986448123713169" title="最速時間 / Tsuzuri時間 = 0.9986448123713169"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99864</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">control/float64_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">400000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.881583</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.880273</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.885750</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.882783</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.879408</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 3.799417</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9975328471624338" title="最速時間 / Tsuzuri時間 = 0.9975328471624338"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99753</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/integer_mix</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">2000000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 3.136217</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 3.143896</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 3.143360</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 3.148664</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">65.695042</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/mandelbrot</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">243 x 243</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.033563</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.044823</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">12.977657</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15.817850</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.058250</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9957106126697667" title="最速時間 / Tsuzuri時間 = 0.9957106126697667"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99571</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/array_sum</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">800000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.333241</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.333199</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.333425</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.163721</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">27.648938</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9998739650883295" title="最速時間 / Tsuzuri時間 = 0.9998739650883295"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99987</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_scan</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.006505</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.007135</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.007397</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.029312</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.066383</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_compare</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.017064</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.040177</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.019463</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.018979</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.008473</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">JavaScript</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.4965424285044538" title="最速時間 / Tsuzuri時間 = 0.4965424285044538"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.49654</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf16_validate</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.052943</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.054220</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.109140</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.076270</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.352653</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/utf8_roundtrip</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26215</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.135802</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.089322</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.105157</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.109455</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.399460</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.657736999455089" title="最速時間 / Tsuzuri時間 = 0.657736999455089"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.65774</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/format_parse</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">5000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.361152</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.287956</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.153248</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.271101</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.706445</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.7506562333864966" title="最速時間 / Tsuzuri時間 = 0.7506562333864966"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.75066</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/math_intrinsics</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.502429</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.504254</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.504626</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.505729</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.503375</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequence</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.078303</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.078511</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.078520</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.622019</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.832987</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_sequential</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.255049</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.257371</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.256434</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.260312</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">26.283854</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Tsuzuri</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="1" title="最速時間 / Tsuzuri時間 = 1"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 1.00000</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">cpp/task_parallel</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.270801</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.271723</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.282452</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.207901</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">40.328333</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.7677261162255679" title="最速時間 / Tsuzuri時間 = 0.7677261162255679"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.76773</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.315597</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.314338</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.376964</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.314195</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 9.453403</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C#</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9955576257062012" title="最速時間 / Tsuzuri時間 = 0.9955576257062012"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99556</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/checked</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.512117</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.502418</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.517866</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.812968</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">14.448646</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9810609684896224" title="最速時間 / Tsuzuri時間 = 0.9810609684896224"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98106</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/delayed</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">50000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.127529</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.126686</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.145697</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.181289</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 7.499083</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9933897388045071" title="最速時間 / Tsuzuri時間 = 0.9933897388045071"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99339</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_for</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000603</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000603</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000584</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.001198</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.075583</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.96849087893864" title="最速時間 / Tsuzuri時間 = 0.96849087893864"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.96849</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/array_bind</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000601</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000609</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.000590</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.001195</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.074818</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9816971713810317" title="最速時間 / Tsuzuri時間 = 0.9816971713810317"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98170</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/owned_capture</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.002617</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.002600</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.002674</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.003055</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.062440</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9935040122277417" title="最速時間 / Tsuzuri時間 = 0.9935040122277417"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.99350</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.503928</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.440928</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.439684</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.525974</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">13.823125</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">Rust</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.8725135336794145" title="最速時間 / Tsuzuri時間 = 0.8725135336794145"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.87251</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_result</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">200000</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.512261</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right;background-color:#dcfce7;color:#14532d;font-weight:700" title="この行の観測最小値" data-fastest="true"><strong><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.503086</code></strong></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.517902</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.747900</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">15.489666</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">C++</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" data-tsuzuri-performance="0.9820892084308586" title="最速時間 / Tsuzuri時間 = 0.9820892084308586"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.98209</code></td>
</tr>
<tr>
<th scope="row" style="padding:8px 12px;border:1px solid #94a3b8;text-align:left">computations/std_option_owned</th>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">820</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.017409</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">        -</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.001514</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.001808</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.002098</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit"> 0.057878</code></td>
<td style="padding:8px 12px;border:1px solid #94a3b8;white-space:nowrap">条件差あり</td>
<td style="padding:8px 12px;border:1px solid #94a3b8;text-align:right" title="比較条件または測定値のため計算対象外"><code style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0;background:transparent;color:inherit">       -</code></td>
</tr>
</tbody></table>
</div>
<p><code>-</code> は未測定です。すべて wall time（ms/呼び出し）で、起動時間は含みません。C#/JavaScript は別プロセス、管理メモリの回収条件は言語ごとに異なります。<code>std_option_owned</code> は確保条件が異なるため順位を付けません。詳細な比較条件は後述します。</p>
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
- `std_option_owned` 種目における C++、Rust、C#、JavaScript のコードは、文字列長を直接算出する高度に最適化されたリファレンス実装です。所有文字列の扱いに伴うコスト比較は、同一のメモリ管理を行う Tsuzuri の直接記述版とコンピュテーション式版との間で行う必要があります。この結果をもって「言語間に 11 倍の性能差がある」と単純解釈することは誤りです。
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

`std_*` は B01 で追加したため、この三種目を含む現在のソースとの `--baseline` 比較には Maybe／Result 対応版が必要です。
`std_option_owned` の C++ は既知の文字列長を直接計算する最適化済みの参照であり、所有文字列のコスト比較は
Tsuzuri の builder／手書き版の間で行います。これらの追加自体を高速化の実測結果とは扱いません。

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
1,024 から 65,536 にしました。型が大きくなり続ける多相再帰は全体の上限より先に止めます（呼び出しの連鎖に同じ関数がより小さい型で 32 回を超えて現れたとき、
またはそのような特殊化が 1,024 件を超えたとき）。`fn rec f x = f [x]` は 0.06 s、`f (ref x, 1)` と `f (ref x, true)` の分岐は 0.07 s で止まります。
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
