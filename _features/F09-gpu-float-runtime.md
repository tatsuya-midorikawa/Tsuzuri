# F09: GPU の浮動小数点・64-bit カーネルと実行時接続

| 項目 | 内容 |
|---|---|
| ID | F09 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | F07, (C11) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C/C++ 比: GPU は実験段階で、成熟した GPU 開発基盤の代替にならない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 主な影響ファイル | `src/gpu.rs`, `std/Gpu.tz`, `src/runtime/webgpu.mjs`, `src/runtime/`（新規 GPU ランタイム）, `src/driver.rs`, `docs/language.md`, `tests/gpu.mjs`, `benchmarks/run-gpu.mjs` |

## 目的

CUDA／Metal／Vulkan compute を C/C++ から使う場合に近い GPU 活用を、数値の意味を明示したうえで可能にする。
浮動小数点と 64-bit 整数のカーネル、通常の Tsuzuri 実行からの実 GPU 利用、測定に基づく自動選択を段階的に追加する。

## 現状

- F07 Phase 1: 型付きカーネルの抽出・CPU 参照・strict WGSL。`Gpu.request Gpu.CpuReference` だけが成功し、WebGpu／Vulkan／Cuda／Metal／Auto は `Unavailable`。
- WGSL の strict 経路は i32/i32u だけ。「WGSLには具体的な64-bit整数型がなく、floatのfusion/reassociation/subnormal差を許すため、64-bit/floatのshader生成はE1018です」。除算・剰余も拒否。
- WebGPU のホスト試作 `src/runtime/webgpu.mjs` は別経路で、通常の実行へ自動リンクしない。実 Metal adapter で 783 参照が一致、速度の主張なし。

## 仕様

### Phase 1: strict な f32 と 64-bit 整数（実装対象）

- IEEE の丸め・非正規化数・符号付きゼロ・NaN の保持を保証できる backend だけで浮動小数点カーネルを許す。
  候補は Vulkan の `VK_KHR_shader_float_controls`（RTE 丸め、DenormPreserve、SignedZeroInfNanPreserve）と SPIR-V の `NoContraction` 装飾。
  能力がない device での明示要求は `Error` を返し、CPU や緩い経路へ黙って切り替えない。
- i64 は `shaderInt64` の能力がある device だけ。除算・剰余は CPU と同じトラップ契約を GPU 側で表せるまで引き続き拒否する。
- 緩い浮動小数点（fusion・再結合を許す）は、名前で区別した別 API（例: `Gpu.map_relaxed`）でだけ提供し、CPU 参照との比較は許容誤差で行うことを文書化する。

### Phase 2: 実行時接続（設計方針）

- `Gpu.request Gpu.Vulkan` などを言語の runtime で実装する。GPU ドライバーへのリンク時依存は作らず、実行時に動的に読み込み、ない場合は `Unavailable`。
- device 常駐の buffer と明示的な転送、完了待ちの所有権。

### Phase 3: 自動選択（設計方針）

- `Gpu.Auto` は確保・コンパイル・転送・同期を含めて測った閾値でだけ GPU を選ぶ（docs/architecture.md 性能設計の原則）。選択経路は観測可能にする。

## 設計

- SPIR-V の生成は LLVM の SPIR-V target を使うか独自 emitter にするかを、float controls の対応状況で決める。WGSL 経路は現状のまま残す。
- カーネル抽出（`src/gpu.rs`）の型規則を f32／i64 へ広げ、backend 能力に応じて検査する。
- runtime は C で書き、到達時だけ連結する。native の task runtime と同じ連結経路を使う。

## 実装手順

1. **能力モデル**: backend ごとの数値能力の表と検査。確認: 能力不足の明示エラー。
2. **SPIR-V 生成**: i32 から始め、f32（strict）、i64 へ。確認: `spirv-val` 等による検証と CPU 参照とのビット一致。
3. **Vulkan ホスト試作**: Node または C ホストで実 device 実行。確認: `tests/gpu.mjs` を拡張（実 device がない環境ではスキップではなく明示的に未実行と報告）。
4. **Phase 2 の設計レビュー**: 動的読み込みと所有権。
5. **計測**: 転送を含む時間と常駐時間を分けて `docs/benchmarks.md` に記録する。

## テスト計画

- CPU 参照とのビット一致（strict）、許容誤差の検証（relaxed）、境界値（NaN・±0・非正規化数・overflow）。
- WASM・native の CPU 参照の結果が変わらないこと。

## ドキュメント

- `docs/language.md` の GPU Kernel、`_docs/guides/gpu.md`、`docs/benchmarks.md`、`_docs/feature-status.md`。

## 受け入れ条件

- [ ] strict な f32 カーネルが対応 device で CPU 参照とビット一致する。
- [ ] 能力のない device・backend での明示要求を黙って置き換えない。

## 落とし穴

- GPU の f32 は既定で非正規化数を flush する実装が多い。能力の確認なしに strict と称さない。
- 転送を除いた時間だけで速度を主張しない。

## 対象外

- CUDA の PTX 生成、GPU 上のヒープ確保、GPU 上の再帰・トラップの完全な再現。

## 未決事項

- **SPIR-V 生成の方式**: 既定案は LLVM SPIR-V target を評価し、float controls を出力できなければ独自 emitter にする。
- **relaxed API の名前と許容誤差の定義**: 既定案は ulp 単位の上限を API ごとに文書化する。
