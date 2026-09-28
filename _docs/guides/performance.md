# 性能、CPU 選択、測定

[ドキュメントのトップ](../README.md)

Tsuzuri は性能を設計要件としますが、ハードウェアの名称や LLVM の採用だけで性能優位を保証しません。同じ意味の仕事を、確保・コピー・起動・転送・同期も含めて比較します。

## 最適化レベルと CPU

```sh
./target/release/tsuzuri build examples/hello -O3 --cpu generic -o target/hello-portable
./target/release/tsuzuri build examples/hello -O3 --cpu native -o target/hello-local
```

既定は `-O3` と generic のターゲット baseline です。`--cpu native` はビルド機の命令セットを有効にするので、その成果物の配布先にも対応命令が必要です。自動的にあらゆる CPU 用の複数版を作るオプションではありません。

LLVM のループ・SLP 自動ベクトル化に加え、明示的な SIMD 値型があります。O0 でも直接の自己末尾再帰をループ化するなど、最適化レベルに依存しない言語処理もあります。

## 実行時 CPU dispatch

native の exe / object に同梱する `Array.sum` の `ref [i64] -> i64` 具体化だけが、現在の実行時 CPU 選択の対象です。

| 環境 | 経路 |
| --- | --- |
| x86 の対応 CPU / OS | SSE4.2 または AVX2 の候補を選ぶ |
| AArch64、未知の環境 | baseline |
| Windows の現行実装 | portable baseline |
| WASM、通常の LLVM IR 出力 | この native runtime dispatch の対象外 |

AVX2 は CPUID の bit だけでなく OSXSAVE / AVX と XCR0 の状態保存能力も確認します。選択結果は atomic に共有します。任意の利用者関数、float、SVE / SVE2 の多重化は未実装です。

この経路は整数の折り返し和を維持します。テスト用 `TSUZURI_CPU_FORCE` で利用できない版を強制した場合は停止し、黙って別版で成功しません。

実装記録では ARM baseline の実行、x86 経路のクロスコンパイルを確認しています。x86 実機の速度や優位を確認済みと読み替えないでください。

## データと所有権

読み取り関数の入力を共有借用にすると、大きい Copy 配列の複製を避けられます。非 Copy 要素は `_ref` API を選びます。所有更新は、一意なヒープバッファを再利用できる場合があります。

return することが分かっているコレクションを new で作ると、スタックからヒープへの移送を避けられる場合があります。クロージャーに大きな所有値を捕捉すると、関数値の独立したスナップショットの複製費用が発生し得ます。

既知で外へ逃げない高階関数やビルダー継続は、直接呼び出しや一時的な貸し出しへ特殊化します。借用検査を緩和する最適化ではなく、観測できる独立所有の意味を保持します。

## 並列化の選択

Task.parallel は仕事単位、Parallel は配列の固定チャンクを明示的に並列化します。通常のループを自動並列化しません。小さい仕事では同期や確保が支配し、逐次より遅い場合もあります。

GPU の自動 offload は未実装です。実験的 WebGPU ホストでは、device 上の連続操作と最後の読み戻しを分けられますが、転送を除外した数字だけで end-to-end の優位は主張できません。

## 数値の意味を保つ

overflow、NaN、符号付きゼロ、丸め、評価順序、トラップ、借用の契約は維持します。fast-math、暗黙 FMA、無許可の再結合、データ競合を性能のために導入しません。

Array.sum、sum_pairwise、sum_kahan、Parallel.sum は順序が異なる API です。比較時は同じ入力だけでなく、許される演算順序と精度の契約も合わせます。

## 再現可能な測定

1. 同じアルゴリズム、入力、出力確認、数値規則をそろえる。
2. コンパイラ、最適化、CPU、OS、ランタイム、機能フラグを記録する。
3. コールド起動と定常状態、コンパイル時間と実行時間を分ける。
4. 確保・コピー・転送・同期を含む範囲を明記する。
5. 複数回測定し、生成 IR と機械語でも経路を確認する。

quick モードのチェックサム一致は機能検証であって、速度優位の根拠ではありません。共有 CI runner に速度の合否閾値を設けません。詳しいコマンドと既知の限界は[性能測定の記録](../../docs/benchmarks.md)にあります。

## 関連項目

- [SIMD](../library-reference/simd.md)
- [Parallel](../library-reference/parallel.md)
- [コンパイラの性能設計](../../docs/architecture.md#性能設計の原則)
