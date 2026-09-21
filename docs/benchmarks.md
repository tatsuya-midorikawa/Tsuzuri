# 性能測定

**C/C++ 以上の性能は目標であり、初版の一般的な性能保証ではありません。**
LLVM の利用だけで C を上回るわけではなく、コード形状、値のコピー、最適化、
WASM エンジン、ホスト呼び出しの粒度に依存します。

整数ミキサーの性能上の土台は、ヒープ確保／GC／参照カウントなし、自己末尾再帰のループ化、
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
上の C／WASM 比較は GC、動的メモリ、文字列、巨大配列、I/O、GUI、ゲーム全体、
C++／F#／C# の包括的比較を測っていません。

## C++20 とのネイティブ比較

```sh
cargo build --release --locked
node benchmarks/run-cpp.mjs target/release/tsuzuri

# 両言語をビルド機の CPU 向けに最適化（生成物の他機への移植性は下がる）
node benchmarks/run-cpp.mjs target/release/tsuzuri --cpu native

# 小さな入力で結果の一致だけを確認（CI 用。速度比較には使わない）
node benchmarks/run-cpp.mjs target/release/tsuzuri --quick
```

`TSUZURI_CLANG`（既定は `clang`）を両言語に使い、C++ は同じ実行ファイルを
`--driver-mode=g++` で起動します。C++20 の標準ライブラリが必要です。
既存の C／WASM 比較とは独立した、次の3種目の測定です。

| 種目 | 既定の仕事量 | 含める処理 |
|---|---|---|
| `integer_mix` | 20,000,000 反復 | 既存の `Mix.tzr` と同じ64-bit整数ミキサー |
| `mandelbrot` | 768 × 768 点、最大256反復/点 | f64 の Mandelbrot escape-time、全点の反復数を集計 |
| `array_sum` | 8,000,000 要素（64 MB） | 64-bit配列の確保、seed に依存する初期化、全要素の和、解放 |

両言語とも `-O3`、ネイティブ・オブジェクトは PIC、LTO・fast-math・CPU 固有の
`-march=native` は使いません。C++ は `-ffp-contract=off` で暗黙の FMA 融合も無効にし、
Tsuzuri と演算順を揃えます。C++ の整数演算は `uint64_t` と `std::bit_cast` で
64-bit折り返しを再現し、signed overflow に依存しません。
配列は Tsuzuri では読み取り借用で集計し、余分な深いコピーを避けます。
C++ は `std::make_unique_for_overwrite` で未初期化領域を確保してから同じ式で埋め、
不要なゼロ初期化を加えません。Tsuzuri の安全検査は無効にしていません。
`--cpu native` を選んだ場合だけ、両言語とも同じ `-march=native`（x86）または
`-mcpu=native`（ARM）を使います。generic／native の結果を混ぜず、JSON の
`cpp_flags` と `tsuzuri_flags` に条件を保存します。

各種目を両言語で2回ずつウォームアップし、seed を変えて10サンプルずつ採取します。
C++ を先に測る回と Tsuzuri を先に測る回は5回ずつです。既存の C 比較と同じ
`clock()` の CPU time を使い、コンパイル時間やプロセス起動時間は含めません。
C++ カーネルは noinline、Tsuzuri は別オブジェクトから呼び、入力読み出しと
結果保存を volatile にして計算の除去・使い回しを防ぎます。
ゼロ長、小さい入力、負の seed、i64 の最小・最大 seed を含む各25ケースを、
両言語および独立した JavaScript/BigInt 参照実装で照合します。
本測定の全サンプルでも両言語の checksum が一致しなければ失敗します。
CI は小さい入力で正しさだけを確認し、速度の合否判定はしません。

JSON は CPU・OS・ツールのバージョン、コンパイル条件、時計の単位、
各種目の `checks`／`raw` を含みます。`cpp_median_ms` と `tsuzuri_median_ms` は
10サンプルの中央値（中央2値の平均）、`tsuzuri_over_cpp` はその比です。
**1 未満なら Tsuzuri が速く、1 より大きければ C++ が速い**ことを表します。
この3種目だけで一般的な言語の優劣は決まりません。配列種目は確保・解放も含むため、
メモリアクセスだけの帯域測定でもありません。小差は実行ごとの変動と合わせて判断してください。

### 改善前の実測例（2026-09-21）

Apple M1 Max、arm64、Darwin 25.6.0、Apple Clang 21.0.0
（clang-2100.3.34.2）、Node.js 20.19.6、Tsuzuri 0.1.0。
コンパイラの基準コミットは `b4626bd`。上記の既定サイズで3回連続実行し、
各言語・各種目の計30サンプルをまとめた中央値です。単位は ms、短いほど速い値です。

| 種目 | C++20 | Tsuzuri | Tsuzuri / C++ | 読み方 |
|---|---:|---:|---:|---|
| 整数ミキサー | 31.798 | 31.656 | 0.996 | ほぼ同速 |
| Mandelbrot | 129.276 | 1269.500 | 9.820 | C++ が約9.8倍速い |
| 配列の生成・集計・解放 | 3.710 | 3.745 | 1.009 | ほぼ同速 |

各回の中央値の比は、整数が 0.999〜1.001、Mandelbrot が 9.792〜9.844、
配列が 0.994〜1.030 でした。整数と配列の小差を勝敗とは扱いません。
生データは次のように保存できます（今回の測定もこの3ファイルに保存）。

```sh
mkdir -p target/benchmarks
for trial in 1 2 3; do
  node benchmarks/run-cpp.mjs target/release/tsuzuri > "target/benchmarks/cpp-$trial.json" || break
done
```

この改善前の Mandelbrot の生成コードでは、Tsuzuri は画素ごとの座標の `i64 -> f64` 変換2回に
汎用ソフトウェア変換の `tz_soft_cast` を呼び、C++ は CPU の整数→浮動小数点変換命令を使います。
`src/llvm.rs` の `cast` と `src/runtime/numeric.c` の `tz_soft_cast` が対応する処理です。
escape-time の内側は両者とも浮動小数点命令のループになっており、
**型変換の実装が大きな改善候補**です。ただし、この測定だけで遅延の全量を
型変換に帰属させたり、浮動小数点演算全般が9.8倍遅いと判断したりはできません。
この比較ではコンパイラ本体の最適化は変更していません。

### 直接数値変換への改善後（2026-09-22）

同じ M1 Max・ツールチェーン・入力・仕事量で、`generic` と `native` をそれぞれ
3回連続実行しました。各条件・各言語の30サンプルをまとめた中央値（ms）です。
ネイティブコードの数値仕様、トラップ、安全検査、演算順は緩めていません。

| 種目 | C++ generic | Tsuzuri generic | C++ native | Tsuzuri native |
|---|---:|---:|---:|---:|
| 整数ミキサー | 31.372 | 31.346 | 31.402 | 31.350 |
| Mandelbrot | 128.371 | 129.100 | 128.332 | 128.270 |
| 配列の生成・集計・解放 | 3.610 | 3.723 | 3.583 | 3.605 |

Mandelbrot の Tsuzuri generic は改善前の 1269.500 ms から 129.100 ms へ、
**約9.8倍高速化**しました。C++ との時間比は約9.82から約1.006になっています。
変更点は8〜64-bit整数と f32／f64 の直接変換、浮動小数点→整数の飽和 intrinsic、
f32／f64 間の直接変換です。最適化後の IR には、画素座標の整数→f64 変換命令が残り、
`tz_soft_cast` 呼び出しはなくなりました。配列集計も SIMD の整数加算に下がることを確認しました。

この機械では `native` の追加効果は小さく、CPU 指定だけで必ず速くなるとは限りません。
配列は実行ごとにも数 % 揺れるため、僅かな差を一般的な言語の優劣とは扱いません。
GPU や複数 CPU コアの測定ではなく、単一スレッド CPU の比較です。
正確な広幅／decimal 変換のソフトウェア経路は維持しています。

生データは改善前とは別の `target/benchmarks/cpp-generic-{1,2,3}.json` と
`target/benchmarks/cpp-native-{1,2,3}.json` に保存しました。再実行例:

```sh
mkdir -p target/benchmarks
for cpu in generic native; do
  for trial in 1 2 3; do
    node benchmarks/run-cpp.mjs target/release/tsuzuri --cpu "$cpu" \
      > "target/benchmarks/cpp-$cpu-$trial.json" || exit 1
  done
done
```

## 現実的な次の指標

目標の達成には、代表的なアプリケーション・カーネル、コンパイル時間、成果物サイズ、
ホスト境界、最大メモリ使用量を継続測定する必要があります。
新しいデータ構造では、コピーや確保のコストも含めて比較対象と条件を揃えてください。
性能の回帰閾値を CI に入れる場合も、安定した専用ハードウェアと複数回の測定が必要です。
共有 CI ランナーに速度の合否閾値は設定していません。
