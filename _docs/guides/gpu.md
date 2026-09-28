# GPU カーネルと WebGPU 試作

[ドキュメントのトップ](../README.md)

GPU は実験的な Phase 1 の機能です。カーネル抽出、CPU 参照実行、strict 整数の WGSL 生成、独立した WebGPU ホスト試作に対応します。通常の Tsuzuri ランタイムへ実 GPU を自動接続する機能ではありません。

## 対応範囲を選ぶ

| 機能 | 現在の対応 |
| --- | --- |
| 言語内の Gpu.CpuReference | 対応。通常の CPU と allocator を使う |
| 言語内の WebGpu / Vulkan / Cuda / Metal / Auto | Unavailable を返す |
| WGSL の i32 / i32u カーネル | 対応 |
| WebGPU ホスト試作からの実 adapter 実行 | 対応環境で可能 |
| float / 64-bit GPU、通常 runtime への自動接続 | 未実装 |
| 自動 offload、性能優位の保証 | なし |

Auto も「CPU fallback を選ぶ」という意味ではありません。明示 GPU 要求が unavailable なら Error を返し、CPU に変更して成功扱いにしません。

## CPU 参照 API

```tsuzuri run=18
let device = Result.get (Gpu.request Gpu.CpuReference)
let initial = Gpu.init (ref device) 4 (fx index -> index * index)
let mapped = Gpu.map (ref device) (fx value -> value + 1i32) initial
let values = Gpu.to_array mapped
Array.sum ref values
```

Device と Buffer は非 Copy の不透明な所有型です。Device は共有借用し、map / to_array は Buffer を消費します。CPU 参照の Buffer は GPU 常駐メモリではありません。

| API | 契約 |
| --- | --- |
| `request backend` | `Result<Device, Error>`。CpuReference だけ成功 |
| `backend device` | 借用した Device の Backend を返す |
| `init device count initializer` | index は i32、count は i64 の 0 から 2147483647 |
| `map device transform buffer` | buffer を消費して新しい Buffer を返す |
| `from_array device values` | 共有配列を複製して Buffer を作る |
| `to_array buffer` | buffer を消費して配列へ |

CPU 参照の要素型は i32 / i32u / i64 / i64u / f32 / f64 です。これらがすべて WGSL 出力に対応するわけではありません。

## カーネルの制約

init / map / from_array は直接の完全適用が必要です。init / map の callback は既知の関数または capture のない lambda に限ります。

scalar のローカル値、算術、比較、cast、if、既知の関数呼び出しを使えます。確保、借用、host call、Task、ループ、再帰、assert、未知の関数値は `E1018` です。抽出深さ 128、関数 1,024、式 65,536 の上限は `E1017` です。

## WGSL を生成する

専用プロジェクトの `Kernel.tz`:

```tsuzuri project=wgsl file=Kernel.tz
export def transform :: i32u -> i32u
fn transform value = value * 3i32u + 1i32u
```

```sh
./target/release/tsuzuri build target/gpu-demo/Kernel.tz --emit wgsl -o target/kernel.wgsl
```

一つの export された scalar 一引数関数を入口にします。target / optimization / cpu / debug オプションは付けません。

strict WGSL は i32 / i32u の加減乗算を折り返し、シフト量を下位 5 bit にマスクし、同幅の整数 cast を bit 保持で処理します。除算・剰余はトラップ契約の違いから拒否します。

64-bit 整数と float は WGSL の表現・fusion・再結合・subnormal の契約が一致しないため `E1018` です。CPU 参照側の意味を緩めて対応扱いにしません。

## WebGPU ホスト

[同梱ホスト](../../src/runtime/webgpu.mjs)は createWebGpu で adapter を明示要求します。browser の navigator.gpu、または対応する Node WebGPU binding を利用します。binding や GPU driver は compiler 自身の依存には追加されません。

| ホスト API | 動作 |
| --- | --- |
| `prepare(source)` | WGSL の診断を確認して map / init pipeline を作る |
| `fromArray(values)` | Int32Array / Uint32Array を device へ転送 |
| `init(program, count)` | index カーネルで device 上の Buffer を生成 |
| `map(program, buffer)` | 所有 Buffer を消費し、結果を device に保持 |
| `toArray(buffer)` | Buffer を消費し、同期を待って Uint32Array として読み戻す |
| `close()` | 待機中の処理を完了し、資源を解放する |

toArray の符号付き解釈が必要なら返却 bit 列を Int32Array として読みます。map / toArray の入力ハンドルを二度使えません。device 制限や shader の失敗は例外になり、CPU fallback はありません。

workgroup size は 256、entry は map_main / init_main、binding 0 は入力、1 は出力、2 は length を含む uniform です。init は入力 binding を使いません。

Node の既存例は[GPU 実行スクリプト](../../examples/gpu/run.mjs)です。WebGPU binding を用意した上で `node examples/gpu/run.mjs target/kernel.wgsl` を実行します。既定の任意依存の配置と `TSUZURI_WEBGPU_MODULE` の上書きは同スクリプトを参照してください。

## 測定と検証状況

実装記録には Metal adapter 上での strict 整数参照照合がありますが、全 GPU・driver の動作や速度を保証するものではありません。転送、pipeline 初期化、queue 同期、読み戻しまで含めて測定します。

## 関連項目

- [Gpu のソース宣言](../library-reference/api/Gpu.md)
- [性能測定](performance.md)
- [Parallel](../library-reference/parallel.md)
