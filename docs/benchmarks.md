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

- `benchmarks/Mix.tz` をネイティブ・オブジェクト化した関数。
- `benchmarks/native.c` に書いた同じ計算の C 関数。
- 同じ `.tz` を WASM にした関数。

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

`Task.parallel` はネイティブの bounded fork/join を実装していますが、上の既存比較は
タスクの速度比較ではありません。`examples/tasks` とタスクのテストも、同時実行・結果・
資源回収を示すもので、速度向上の証拠には使いません。
並列処理を比較する場合は同じ仕事・分割・コピー条件で、タスクの生成、捕捉の複製、
結果領域の確保、スレッドの起動、同期、全 join／解放を含む wall time を測ります。
CPU time は複数スレッドの時間を合計するため、逐次版との経過時間比較には使えません。
常駐ワーカープールは未実装で、小さい仕事は起動費用に負ける可能性があります。
WASM のタスクは逐次フォールバックであり、マルチコア性能として報告しません。

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
| `integer_mix` | 20,000,000 反復 | 既存の `Mix.tz` と同じ64-bit整数ミキサー |
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

## コンピュテーション式の比較

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
| `std_option` | 2,000,000 反復 | 標準 Option の成功／失敗と、手書きの同じ union match |
| `std_result` | 2,000,000 反復 | 標準 Result の二段の error 伝播と、手書きの同じ union match |
| `std_option_owned` | 8,192 反復 | 所有文字列の連結・成功／失敗・解放を Option と手書き match で比較 |

`std_*` は B01 で追加したため、この三種目を含む現在のソースとの `--baseline` 比較には Option／Result 対応版が必要です。
`std_option_owned` の C++ は既知の文字列長を直接計算する最適化済みの参照であり、所有文字列のコスト比較は
Tsuzuri の builder／手書き版の間で行います。これらの追加自体を高速化の実測結果とは扱いません。

### 標準 Option／Result の測定（2026-09-25）

`be5b27a` に P0 の未コミット差分を適用したコンパイラ（SHA-256
`7e59f9bb41669b89639e1e1ffc83af94f8fd78d409d98ebe24b074816b7c661a`）を使用しました。
Apple M1 Max（arm64、10 logical CPU）、macOS／Darwin 27.0.0、Homebrew Clang 23.1.1、Node 20.19.6。
`generic`・`-O3`・fast-math／LTO なし、12 サンプルの中央値です。native は CPU 時間、
WASM は warm-up 後の wall time で、起動時間は含みません。

| 種目 | native C++ (ms) | native 手書き (ms) | native builder (ms) | WASM 手書き (ms) | WASM builder (ms) |
|---|---:|---:|---:|---:|---:|
| `std_option` | 4.666600 | 5.290500 | 5.273125 | 0.001854 | 0.002004 |
| `std_result` | 5.168375 | 5.307625 | 5.257875 | 0.002101 | 0.002133 |
| `std_option_owned` | 0.016241 | 0.173719 | 0.177957 | 0.014989 | 0.016846 |

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
計算結果を定数に置換したり、Tsuzuri にだけ不利なゼロ初期化を C++ に追加したりしません。
配列種目には実際の確保・初期化・解放を含めます。`owned_capture` の C++ は固定長配列をスタックに置く
手書き版であり、Tsuzuri に合わせて不要なヒープ・クロージャーを追加しません。
基準版で大きな再帰的コールバックがスタックを使い切ったため、時間比較には両版が完走するサイズを使います。
別の実行テストで、改善版の 262,144 回のコールバックを native／WASM `-O3` で照合します。

### 条件・出力・確保数

両言語に同じ Clang、`-O3`、PIC、`-fno-fast-math`、`-ffp-contract=off` を使い、LTO はしません。
`--cpu native` の場合だけ両言語に同じ CPU フラグを付けます。
native は `clock()` の CPU time、WASM は Node.js でウォームアップした後の wall time です。
WASM は全種目 1,024 要素／反復で、手書き Tsuzuri・改善前・改善後を比較します。
WASM の C++ 標準ライブラリは要求せず、native と WASM の時間を同じ比率にまとめません。

native は各版を2回、WASM は3回ウォームアップします。
各種目を12サンプル測り、seed と計測順を変えます。各版ごとに繰り返し数を校正して、
約20 ms のバッチを目安にし、一呼び出し当たりの時間へ正規化します。
上限は10,000呼び出しで、非常に短い経路ではバッチも20 ms未満になります。
native の入力読み出し・結果保存は volatile、C++ カーネルは noinline です。
計測内の全呼び出しの checksum と、ゼロ長・小さい入力・負の seed・i64 の MIN/MAX を含む
各25ケースを確認し、JavaScript/BigInt の独立した参照実装とも照合します。
WASM のホスト境界と結果照合は計測に含みますが、コンパイル・インスタンス化・プロセス起動は含めません。

JSON には `native_flags`、`wasm_flags`、各コンパイラの SHA-256、CPU・OS・ツールの情報、
全サンプルの時間・繰り返し数・checksum を残します。
`native.computation_over_cpp`／`computation_over_direct` は1未満なら CE が速い比率、
`native.speedup`／`wasm.speedup` は改善前時間を改善後時間で割った倍率です。
最小・最大も保存するため、単発の僅かな差を速度向上とは判定しないでください。
`--artifacts` は native のソース IR・最適化後 IR・アセンブリ、C++ の IR・アセンブリ、
オブジェクト、実行ファイル、WASM を指定ディレクトリへ保存します。

`allocations_at_size_64` は **時間計測とは別の実行ファイル**で数えます。
まず通常の `-O3` を完了し、その IR に残った `malloc`／`free` だけを追跡関数へ置換します。
追跡による副作用を隠す最適化属性を除き、再最適化せずに実行し、全呼び出し後の未解放バイトが0であることも確認します。
最適化前の IR を追跡関数に置換してから `-O3` にすると、本来消える確保まで残り、
実際のリリースコードの確保数とは異なるため、この測定では使いません。

### 改善の範囲

非 escaping な既知の継続を通常の関数呼び出しとして特殊化し、読むだけの所有する捕捉値を
呼び出し元が生存させたまま内部的に貸し出すことで、間接呼び出し・環境の複製・反復ごとの解放を除去します。
一時的な環境は entry block のスタック領域を使い、一回しか使用しない Copy ローカルの深いコピーも省略できます。
同じ経路を手書きの高階関数、パイプライン、配列・リスト初期化にも適用します。
型検査・所有権検査を省く最適化ではなく、スナップショット・評価順序・trap を維持します。

未知・逃げる関数値、所有する捕捉値の消費や置換、借用を返す場合などは通常の環境を維持します。
追加 worker の予算を超えた場合も正しい通常経路を使います。
したがって、この6種目が手書き版に近づいても、すべての CE が必ずゼロコストになるという意味ではありません。
GPU・複数 CPU コア・専用 SIMD バックエンドによる高速化の測定でもありません。

### 実測（2026-09-22）

Apple M1 Max／arm64／Darwin 27.0.0、Apple Clang 21.0.0
（clang-2100.3.34.2）、Node.js 20.19.6、Tsuzuri 0.1.0。
改善前はコミット `869c4b1` から保存したコンパイラです。
同じ現在のベンチマークソースを両コンパイラに渡し、generic／native を各3回、
各条件の36サンプルをまとめた中央値で比較しました。
反復条件を C++ と揃える前の探索的な測定値は、この表には混ぜていません。

native generic、単位は ms。改善倍率は「改善前 / 改善後」です。

| 種目 | 改善前 CE | 改善後 CE | C++ | 改善倍率 | 改善後 / C++ |
|---|---:|---:|---:|---:|---:|
| `bind` | 3.207643 | 3.208357 | 3.205500 | 1.00 | 1.001 |
| `checked` | 5.145250 | 5.240125 | 5.147625 | 0.98 | 1.018 |
| `delayed` | 1.302906 | 1.295531 | 1.289031 | 1.01 | 1.005 |
| `array_for` | 0.370280 | 0.005649 | 0.005634 | **65.5** | 1.003 |
| `array_bind` | 0.007913 | 0.005616 | 0.005627 | **1.41** | 0.998 |
| `owned_capture` | 1.419150 | 0.026332 | 0.026308 | **53.9** | 1.001 |

native CPU 指定でも同様で、配列 `for` は66.1倍、所有配列の捕捉は53.7倍、
配列 bind は1.40倍の改善でした。改善後の C++ 比は generic で0.998〜1.018、
native CPU 指定で0.994〜1.020です。
**手書き最適化版とほぼ同程度に到達しましたが、全種目で C++ を上回った結果ではありません。**
`checked` の native は改善前より約2%遅く、この小差を隠して全面的な高速化とは報告しません。
単純なスカラー CE は改善前から native LLVM `-O3` が環境の確保を消せていました。

WASM は native generic と同時に採取した3回、各36サンプルです。
仕事量は各1,024、単位はマイクロ秒で、native 表とは直接比較しません。

| 種目 | 改善前 CE | 改善後 CE | 手書き Tsuzuri | 改善倍率 |
|---|---:|---:|---:|---:|
| `bind` | 17.167 | 2.031 | 2.013 | **8.45** |
| `checked` | 14.858 | 2.102 | 2.083 | **7.07** |
| `delayed` | 25.389 | 2.781 | 2.796 | **9.13** |
| `array_for` | 27.876 | 0.721 | 0.716 | **38.7** |
| `array_bind` | 5.227 | 0.713 | 0.721 | **7.33** |
| `owned_capture` | 178.551 | 3.998 | 3.988 | **44.7** |

改善前の WASM では、独自 allocator を使う環境の確保・複製が native と同じようには消えていませんでした。
静的な継続の特殊化と初期化関数の直接呼び出しにより、この差も縮まりました。
native CPU 指定の測定に付随する WASM も同じバイナリ条件で、同程度の結果です。

最適化後 native IR に残る確保回数（入力64）:

| 種目 | 改善前 CE | 改善後 CE | 手書き Tsuzuri |
|---|---:|---:|---:|
| スカラー3種目 | 0 | 0 | 0 |
| `array_for` | 130 | **1** | 1 |
| `array_bind` | 3 | **1** | 1 |
| `owned_capture` | 259 | **1** | 1 |

改善後の配列種目の IR／アセンブリには、入力データ用の確保と解放だけが残り、
ホットループから継続の間接呼び出し・環境の確保・深い複製が消えています。
反復の後に環境を解放するために残っていた再帰も、既知の callback では LLVM がループへ変換できます。
一般の未知・逃げる関数値に同じ保証を拡張したわけではありません。

生データと生成コードは `target/benchmarks/ce-final-{generic,native}-{1,2,3}.json` と
同名ディレクトリへ保存しました。JSON の compiler SHA-256 は
改善前 `c161c53723ef4620aed36cf1e8872c5a4efc701adfeb296e54927a9c2d9cc95d`、
改善後 `8e22ddcba72572b0bd8f0dc4dfee6129618a4c7e1c4b08b706e1172646385b0c` です。
baseline バイナリは改善前コミットを別の作業ディレクトリでビルドして保存することで再作成できます。
再測定例:

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

# 保存した変更前のコンパイラと、同じプロセス内でも比較
node benchmarks/run-control.mjs target/release/tsuzuri \
  --baseline target/benchmarks/tsuzuri-recursion-baseline-b3f658e
```

`benchmarks/control/Main.tz` の新しい制御構文を、C11、C++20、Rust と比較します。
C／C++ は同じ `reference.c` を各言語としてコンパイルし、Clang と CPU 指定を Tsuzuri に揃えます。
Rust は `reference.rs` を `rustc` で独立した PIC オブジェクトにします。
`TSUZURI_CLANG`／`TSUZURI_RUSTC` でツールを指定できます。
全言語で `-O3` 相当、LTO・fast-math なし、整数は同じ 64-bit 折り返しです。
Rust の slice 走査は通常の安全なコードで、未初期化領域の確保・初期化・解放だけを
同じシステムの malloc/free に揃えています。ゼロ初期化や余分なコピーを比較相手だけに課しません。

| 種目 | 既定の仕事量 | 比較する処理 |
|---|---|---|
| `while_mix` | 8,000,000 反復 | ループ依存の整数ミキサー |
| `for_mix` | 8,000,000 反復 | i32 の `downto` と、対応する C／C++ の for、Rust の逆順 inclusive range |
| `tail_mix` | 8,000,000 反復 | match からの自己末尾再帰と、同じ計算の手書きループ |
| `tail_if_mix` | 8,000,000 反復 | 同じ末尾再帰を if で記述 |
| `tail_builtin_mix` | 8,000,000 反復 | 同じ末尾再帰を `Sub.sub` で記述 |
| `match_dispatch` | 8,000,000 選択 | 16 通りの整数分類と折り返し加算 |
| `array_sum` | 4,000,000 要素（32 MB） | 確保、seed 依存の初期化、for-in 集計、解放の全体 |

各種目・各言語を二回ウォームアップし、12 サンプルを採取します。
`--baseline` 付きでは変更前も五つ目の実装としてリンクし、全実装で順序を均等に巡回できる15サンプルにします。
一サンプルは二回呼び出しの平均 CPU time です。順序は四言語で巡回・反転させ、
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
下表の時間は独立した三回の実行の中央値の中央値、比率も各回の比率の中央値です。
したがって、別々に集計した時間同士の商と比率が厳密に一致するとは限りません。

| CPU 指定 | 種目 | Tsuzuri ms | 対 C | 対 C++ | 対 Rust |
|---|---|---:|---:|---:|---:|
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

この条件では **for は Rust より約 1.60 倍、整数 match は C/C++ より約 1.63 倍高速**でした。
while、配列、その他のほぼ 1.0 の差を性能上の勝利とは解釈しません。
**この時点では match を使う末尾再帰のミキサーが約 20% 遅く**、すべての制御経路が比較相手を上回ったわけではありません。
その後の末尾再帰改善は次節に記録します。速度のために signed overflow を未定義動作に変更することはしません。
Rust と Clang では LLVM の版・最適化パイプラインも異なります。

生成アセンブリでは、for の反復ごとの狭幅拡張が消え、整数 match は静的表と展開したスカラーループ、
配列集計は NEON の `ldp q`／`add.2d`／`addp` になっていることを確認しました。
match 内の小さい入力専用の vector 経路は大きい入力では通らないため、
その命令の存在だけをこの match 測定の SIMD 加速と呼びません。
GPU・自動並列化・全アプリケーション性能・他 CPU での優位性を示す測定でもありません。

生データとコードは `target/benchmarks/control-final-{generic,native}-{1,2,3}.json` と同名ディレクトリです。
測定したコンパイラの SHA-256 は
`94291a712871f12137d8227a7cc52e9772feaacd72245f0750e58fb0fa4b710a`。
共有 CI には速度の閾値を追加していません。

既存の CE 比較で古い `--baseline` を指定する場合、実行器は小さいソースで `rec` 構文を検出します。
未対応のベースラインには、一時コピーから再帰の指定だけを除いて同じアルゴリズムを渡します。
このモードは `baseline_rec_syntax: "legacy-rec-erased"` として記録し、予期した構文エラー以外の
ツール失敗はそのまま失敗にします。現行ソースの再帰検査を無効にする機能ではありません。

### 末尾再帰の改善（2026-09-23）

ループ化自体は既に実装済みなので、while への単純な構文置換ではなく、
元の計算・入力・終了条件を保つ八通りの LLVM IR を比較しました。
分岐／ブロック順の変更、非ゼロの assume、広幅カウンター、正負の経路の分離だけでは
このミキサーの遅れは解消しませんでした。
明示的なループ末尾の比較と、引数の整数演算を後で生成する方法は、ともに C/Rust と同等になりました。
前者に必要なカウンター・停止条件の認識や本体の複製を避けられる、後者を採用しています。

native の自己末尾呼び出しで、整数 `+`／`-` の**オペランドを元の評価順で確定**し、
トラップしない折り返し演算だけを全引数の評価後に配置します。
読み出し・呼び出し・可変状態の更新・トラップ・一時値の解放は元の順序のままで、
新しい非負制約、overflow フラグ、浮動小数点の再結合は追加しません。
本体が一つの演算だけの既知の関数も認識し、`Sub.sub` と `-` を同じ経路にします。
この変更は非末尾再帰や相互再帰のアルゴリズムを変更するものではありません。

同じ M1 Max／Apple Clang 21／Rust 1.98.1 の環境で、変更前後と C/C++/Rust を
**同じプロセス内**にリンクし、generic／native を独立に三回ずつ測定しました。
一回は各実装二回のウォームアップ、15サンプル、各サンプル二回の平均です。
時間・比率はそれぞれ三回の中央値です。

| CPU | 末尾再帰の形式 | 変更前 ms | 変更後 ms | 高速化率 | 対 C | 対 C++ | 対 Rust |
|---|---|---:|---:|---:|---:|---:|---:|
| generic | match / `-` | 14.937 | 12.437 | 1.201 | 1.001 | 1.000 | 1.001 |
| generic | if / `-` | 14.979 | 12.416 | 1.205 | 0.999 | 0.999 | 0.996 |
| generic | match / `Sub.sub` | 14.990 | 12.521 | 1.197 | 0.993 | 0.999 | 0.995 |
| native | match / `-` | 14.912 | 12.441 | 1.199 | 1.000 | 0.998 | 1.001 |
| native | if / `-` | 14.972 | 12.461 | 1.196 | 1.002 | 1.003 | 1.001 |
| native | match / `Sub.sub` | 14.929 | 12.433 | 1.200 | 1.000 | 0.999 | 1.001 |

**約1.20倍、実行時間では約17%の改善で、以前の約20%の遅れを解消しました。**
C/C++/Rust を大幅に上回ったという結果ではなく、このミキサーで同等になったという結果です。
生成アセンブリでは、状態更新の `eor → madd → add` の直列依存が、
カウンター側の加算を独立に行う `eor → madd` へ変わっています。
while／for／整数 match／配列の変更前後比較もほぼ同等でした。
既存の C++ 比較（三種目）と native の CE 比較でも、明確な低下は観測していません。

**WASM にこの命令配置を一律適用する案は採用していません。**
Node/V8 の末尾ミキサーにはほぼ効果がなく、長くウォームアップした CE の delayed 種目では
0.350 ms → 0.371 ms の約6%の低下が二回の測定で再現しました。
最終版は WASM の従来の引数生成順を維持し、CE 比較の WASM 全体が変更前とバイト単位で一致します。
CPU time と WASM wall time を混ぜたり、native の改善率を WASM にも適用したりしません。

`tests/tail_recursion.rs` と control の native/WASM `-O0`／`-O3` 検証では、
負数、8／64／128-bit の折り返し、可変値の読み出し、左右の副作用、関数内の assert、
浮動小数点の加算順序・NaN・符号付きゼロ、所有値・一時配列の解放を検査します。
ASan でも同じ所有権経路を検証し、borrowed／非末尾の通常経路を維持しています。

再測定には、コミット `b3f658e` を別の作業ディレクトリでビルドした変更前バイナリを保存します。
今回は `target/benchmarks/tsuzuri-recursion-baseline-b3f658e` を使用しました。

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

変更前の SHA-256 は `94291a712871f12137d8227a7cc52e9772feaacd72245f0750e58fb0fa4b710a`、
変更後は `66cf308bce9c93498ca93bf27faf7c59217af607d47acd7db98a42894d31187a`。
生データ・IR・アセンブリは上記の各 JSON／同名ディレクトリへ保存しています。
速度の合否閾値は追加せず、他 CPU・LLVM・実行エンジンでの優位性は別途測定する必要があります。

## 現実的な次の指標

目標の達成には、代表的なアプリケーション・カーネル、コンパイル時間、成果物サイズ、
ホスト境界、最大メモリ使用量を継続測定する必要があります。
新しいデータ構造では、コピーや確保のコストも含めて比較対象と条件を揃えてください。
性能の回帰閾値を CI に入れる場合も、安定した専用ハードウェアと複数回の測定が必要です。
共有 CI ランナーに速度の合否閾値は設定していません。
