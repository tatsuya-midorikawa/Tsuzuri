# 性能測定

**C/C++ 以上の性能は目標であり、初版の一般的な性能保証ではありません。**
LLVM の利用だけで C を上回るわけではなく、コード形状、値のコピー、最適化、
WASM エンジン、ホスト呼び出しの粒度に依存します。

この数値カーネルの性能上の土台は、ヒープ確保／GC／参照カウントなし、自己末尾再帰のループ化、
型付きの LLVM 値、既定の `-O3`、WASM の不要コード削除です。
安全検査や IEEE 754 の意味を捨てて速く見せる最適化は使いません。

## 再現可能な比較

```sh
cargo build --release
node benchmarks/run.mjs target/release/tsuzuri

# コンパイラと反復数を変更できる（最大 100,000,000）
node benchmarks/run.mjs target/release/tsuzuri 10000000
```

必要なのは通常の LLVM ツールチェーンと Node.js 20 以降です。
同じ Clang、`-O3`、同じ入力で以下を測定し、JSON を標準出力に出します。

- `benchmarks/Mix.tzr` をネイティブ・オブジェクト化した関数。
- `benchmarks/native.c` に書いた同じ計算の C 関数。
- 同じ `.tzr` を WASM にした関数。

ワークロードは符号なし右シフト、XOR、64-bit の折り返し乗算／加算を行う
ループ依存の整数ミキサーです。C 側も `uint64_t` を使い、signed overflow に依存しません。
数列の単純な和だけを測り、LLVM の閉形式への変換をループ速度と取り違えることを避けています。

## 測定条件

既定は 5,000,000 反復 × 9 サンプル。サンプルごとに seed を変え、
全サンプルで C／Tsuzuri native／WASM の checksum を照合します。
WASM は追加で独立した BigInt の参照実装、ゼロ回、負の反復数を検証します。
一致しない測定は失敗です。

ネイティブは外部 ABI の関数を呼び、LTO は行いません。
C baseline は noinline、計測内の入力読み出しと結果の保存は volatile とし、
計算の除去や同一結果の再利用を避けます。
CPU time を `clock()` で計測し、C を先に測る回と Tsuzuri を先に測る回を交互にします。
WASM はウォームアップ後の関数呼び出しを `performance.now()` の wall time で測定します。
一回のホスト呼び出し内に全反復を置き、JS↔WASM 境界のコストだけの比較にしません。

JSON の主な項目:

| 項目 | 意味 |
|---|---|
| `environment` | Tsuzuri／Clang／Node のバージョン、OS、CPU アーキテクチャ |
| `native.c_median_ms` | C の中央値 |
| `native.tsuzuri_median_ms` | Tsuzuri native の中央値 |
| `native.tsuzuri_over_c` | 上の二値の比率。1 付近ならこの入力では同程度 |
| `wasm.median_ms` | WASM の wall time 中央値 |
| `wasm.bytes` / `wasm.imports` | モジュールサイズ／インポート数 |
| `raw` / `raw_ms` | 全サンプルの値 |

ネイティブの CPU time と WASM の wall time は同じ時計ではありません。
単発の僅かな差、別々の環境の比、ブラウザー例の FPS を一般的な言語性能差と解釈しないでください。
ベンチマークは GC、動的メモリ、文字列、巨大配列、I/O、GUI、ゲーム全体、
C++／F#／C# の包括的比較を測っていません。

## 現実的な次の指標

目標の達成には、代表的なアプリケーション・カーネル、コンパイル時間、成果物サイズ、
ホスト境界、最大メモリ使用量を継続測定する必要があります。
新しいデータ構造では、コピーや確保のコストも含めて比較対象と条件を揃えてください。
性能の回帰閾値を CI に入れる場合も、安定した専用ハードウェアと複数回の測定が必要です。
共有 CI ランナーに速度の合否閾値は設定していません。
