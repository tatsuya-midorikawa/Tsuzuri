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

native の exe / object では、同梱 std の一括演算 kernel と、`@cpu` を付けた利用者関数が実行時に命令セットを選びます。選択は最初の呼び出しで一度だけ行い、結果を atomic に共有します。

| 環境 | 選べる版 |
| --- | --- |
| x86-64（Windows を除く） | SSE4.2、AVX2、AVX-512（F・BW・CD・DQ・VL）と baseline |
| AArch64 Linux | SVE、SVE2 と baseline |
| macOS の AArch64、Windows、その他 | baseline だけ |
| WASM、`--emit llvm`、`--freestanding` | 対象外。build 時の命令セットで固定 |

AVX2 と AVX-512 は CPUID の bit だけでなく OSXSAVE と XCR0 の状態保存能力も確認します。SVE / SVE2 は Linux の auxiliary vector から読みます。テスト用 `TSUZURI_CPU_FORCE`（`baseline`、`sse4.2`、`avx2`、`avx512`、`sve`、`sve2`）で利用できない版や未知の名前を指定すると停止し、黙って別の版で成功しません。

### 同梱 kernel

整数 8 型（`i8`〜`i64`、`i8u`〜`i64u`）の `Array.sum`、`Array.min`、`Array.max` は kernel を呼びます。どの版も同じ結果（lane ごとの折り返し和、最初に現れる最小・最大の位置）を返します。自動選択は AVX2 までで、AVX-512 と SVE の版は `TSUZURI_CPU_FORCE` で指定したときだけ使います。周波数低下を含む効果をまだ実機で測定していないためです。浮動小数点の和は左から右の順序が契約なので kernel にしません。

### 利用者関数の多版化（`@cpu`）

```tsuzuri run=285
@cpu ["avx2", "avx512", "sve"]
def dot :: ref [f64] -> ref [f64] -> f64
fn dot left right =
    let mut total: f64x4 = Simd.splat 0.0
    let mut index = 0
    while index + 4 <= left.length do
        let first: f64x4 = Simd.load left index
        let second: f64x4 = Simd.load right index
        total = total + first * second
        index = index + 4
    let mut sum = Simd.sum_lanes total
    while index < left.length do
        sum = sum + left[index] * right[index]
        index = index + 1
    sum

let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
(dot (ref values) (ref values)) as i64
```

`def` の前の `@cpu [...]` は、関数を名前の命令セットごとにもう一度 compile し、元の名前の関数を版を選ぶ stub に置き換えます。名前は `"sse4.2"`、`"avx2"`、`"avx512"`、`"sve"`、`"sve2"` です。build 先で選べない名前は無視するので、一つのソースに x86 と AArch64 の名前を並べられます。

- 版の選択: 指定した版のうち CPU が対応する最新のもの。`TSUZURI_CPU_FORCE` があれば、その水準以下の同じ系統の版だけを使います。明示した `"avx512"` は自動で選びます。
- 意味: どの版も同じ型付き IR から作るので、整数の折り返し、浮動小数点の順序、NaN、トラップは変わりません。fast-math や再結合は有効にしません。
- 呼び出し先: 256-bit ベクトルを渡して直接呼ぶ関数（`Simd.*` を含む）は同じ命令セットで compile します。それ以外の呼び出し先は portable 版のままなので、重い helper にも `@cpu` を付けます。
- 制約: 引数と結果は 256-bit ベクトルを持てず（レコード、union、タプル、固定長配列の中も含む）、256-bit ベクトルを渡す関数値も呼べません（`E1005`）。版ごとに 256-bit ベクトルを渡すレジスタが異なるためです。
- debug 情報: portable 版と stub が持ち、命令セットごとの版は持ちません。

実装記録では、ARM baseline の実行、x86-64 と AArch64 Linux への cross-compile（AVX2 版の `ymm` 命令、SVE 版の生成）を確認しています。x86 と SVE の実機での実行や速度の優位は未確認です。

## データと所有権

読み取り関数の入力を共有借用にすると、大きい Copy 配列の複製を避けられます。非 Copy 要素は `_ref` API を選びます。所有更新は、一意なヒープバッファを再利用できる場合があります。

配列・リストの暗黙の複製は `tsuzuri check --warn implicit-copy` の `W1006` とエディターの inlay hint で位置を確認できます。複製が不要なら `ref` で借用し、必要なら `Array.copy`／`List.copy` で明示します。詳しくは[暗黙の複製を見つける](../language-reference/ownership.md#暗黙の複製を見つける)を参照してください。

return することが分かっているコレクションを new で作ると、スタックからヒープへの移送を避けられる場合があります。クロージャーに大きな所有値を捕捉すると、関数値の独立したスナップショットの複製費用が発生し得ます。

既知で外へ逃げない高階関数やビルダー継続は、直接呼び出しや一時的な貸し出しへ特殊化します。借用検査を緩和する最適化ではなく、観測できる独立所有の意味を保持します。

配列の添字 `values[i]` は、範囲外ならトラップするため検査します。コンパイラが「必ず範囲内」と証明できた添字だけ、この検査の分岐を省きます。省略してもトラップの有無・位置・順序は変わりません。

```tsuzuri run=36
def sum_all :: ref [i64] -> i64 = \values ->
    let mut total = 0
    for i in 0 .. values.length - 1 do
        total = total + values[i]
    total

let values = [10, 11, 15]
sum_all (ref values)
```

この例の `values[i]` は検査を省略します。省略するのは次の形だけです。

- `for i in 0 .. values.length - 1` と `for i in values.length - 1 .. -1 .. 0` の本体の `values[i]`。`Array.length values - 1` も同じ扱いです。
- 長さが分かる配列（`[10, 20, 30]` や `new [i64](3, f)` を `let` で束縛したもの）への、範囲内の定数の添字と、定数の範囲のループ。
- `if i >= 0 && i < values.length then values[i] else 0` の then 側。

次の形は検査を残します。`while` と可変の添字、上限が長さそのものの `0 .. values.length`、別の配列の長さで回すループ、ループの中で配列を置き換える書き方、`ref mut [T]` を経由する配列、`Vec`・リスト・文字列です。残した検査のトラップの種類と位置は変わりません。

`-O3` では LLVM が単純なループの検査を既に消すことが多く、この省略は主に `-O0` の IR と trap 表に現れます。時間の改善を言うときは、[性能測定の記録](../../docs/benchmarks.md)のように測って確かめてください。

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
