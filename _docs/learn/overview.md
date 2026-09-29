# Tsuzuri の概要

[ドキュメントのトップ](../README.md) | [なぜ Tsuzuri か](why-tsuzuri.md) | [コードサンプル](code-samples.md)

Tsuzuri は、関数型の式、静的な型検査、所有権と借用を組み合わせたコンパイル言語です。Rust 製のフロントエンドでプログラムを検査し、LLVM を通してネイティブコードと WebAssembly を生成します。

目指しているのは、AI と人間が、少ない暗黙ルールで堅牢なプログラムを書けることです。関数の合成やデータの表現には高水準の記法を使い、値の受け渡しや数値演算の意味は明確にします。

このページは Tsuzuri 0.1.0 の現在の実装を紹介します。環境の準備は[入門](../get-started.md)、採用時の判断材料は[なぜ Tsuzuri か](why-tsuzuri.md)を参照してください。

## 式と関数で組み立てる

関数は型と実装を一緒に記述します。引数の一部を渡して関数を作り、パイプで値を次の処理へ渡せます。

```tsuzuri run=42
def add :: i64 -> i64 -> i64 = \left right -> left + right

let add_ten = add 10
32 |> add_ten
```

`add 10` は、残りの引数を受け取る関数です。最後の式の値は `42` になります。この例のようなトップレベルの式は、入口となる Main.tz に書けます。

`let` は不変の束縛です。必要なローカル状態には `let mut` を使います。関数のシグネチャは明示し、ローカルな値の型は推論します。異なる数値型の間では、暗黙の変換ではなく `as` などによる明示変換を使います。

評価は厳格です。関数を使った記法であることは、引数が自動的に遅延評価されることを意味しません。詳しくは[関数](../language-reference/functions.md)と[式と演算子](../language-reference/expressions-and-operators.md)を参照してください。

## データの状態を型で表す

関連する値はレコードにまとめ、複数の状態は `union` で表します。`match` は、入力のすべての状態を扱っているか検査します。

```tsuzuri run=42
union Reading = Missing | Value of i64

let reading = Reading.Value 42
match reading with
| Reading.Missing -> 0
| Reading.Value value -> value
```

この例では、値がない状態と数値を持つ状態を区別できます。数値の `0` を「値がない」という特別な意味に使う必要はありません。状態を追加すれば、その状態を扱わない `match` を検出できます。

レコードと union は型パラメーターを持てます。構造に沿った比較・表示などは `deriving` で導出でき、独自の操作は型クラスとインスタンスで定義できます。

詳しくは[レコード](../language-reference/records.md)、[union と再帰型](../language-reference/unions.md)、[型クラス](../language-reference/generics-and-typeclasses.md)を参照してください。

## GC ではなく所有権と借用で管理する

所有する値は、移動・借用・スコープ終了時の解放によって管理します。利用者が通常の値に `free` を書く必要はなく、GC による回収も行いません。

読み取りだけなら、所有権を渡さずに借用します。配列の一部も、要素をコピーせずスライスとして渡せます。

```tsuzuri run=13
let values = [3, 5, 8, 13]
let middle = ref values[1..3]
Array.sum middle
```

スライスは開始位置を含み、終了位置を含みません。この例で読むのは `5` と `8` です。借用が生きている間は、所有者を移動・置換できません。変更を伴う操作では、排他的な `ref mut T` を使います。

`string` や `Vec<T>` は非 Copy の所有値です。値として渡せば移動し、元の束縛はそのまま再利用できません。一方、Copy なコレクションの複製には要素数に応じたコストがあり、Copy は「無料」を意味しません。

また、GC がないことと、暗黙のメモリ確保がないことは別です。`new` による生成のほか、フレーム内の値を外へ移す処理や所有値の複製でもヒープ確保が起こり得ます。記憶域とコピーの規則は[所有権と new](../language-reference/ownership.md)を参照してください。

## 失敗を値として合成する

値がない場合は `Option<T>`、失敗の理由も返す場合は `Result<T, E>` を使います。通常のデータ型なので、`match` で処理できます。

複数の処理をつなぐ場合は、コンピュテーション式の `let!` で成功値を取り出せます。

```tsuzuri run=42
let first = "20"
let second = "22"
let total: Option<i64> = Option {
    let! left = Parse.parse ref first
    let! right = Parse.parse ref second
    return left + right
}
Option.default_value 0 total
```

どちらかの解析が `None` なら、以降の計算を行わず `None` になります。最後の行では、その場合の値を `0` に決めています。`return` は任意位置から抜ける文ではなく、ビルダーの結果を作る末尾の操作です。

ゼロ除算や範囲外アクセスによるトラップは、回復可能な失敗値とは別です。`Option` や `Result` で囲んでも、自動的に失敗値へ変換されません。

詳しくは [Option / Result](../library-reference/option-result.md) と[コンピュテーション式](../language-reference/computation-expressions.md)を参照してください。

## 意味を保ったまま機械語へ近づける

ジェネリック関数と型クラスは、利用する具体型に合わせてコンパイルします。型クラスの実装選択はコンパイル時に完了し、実行時の辞書による選択は必要ありません。ただし、一般の関数値まで常に直接呼び出しになるという保証ではありません。

LLVM の最適化、自動ベクトル化、明示的な SIMD 型を利用できます。数値演算の意味を変えて速くする方針ではなく、整数の折り返し、浮動小数点の丸め、NaN、符号付きゼロ、評価順序を保ちます。通常の整数加減乗算は型の幅で折り返し、オーバーフローを検出・飽和したい場合は `Int` の別 API を選びます。

CPU 並列処理には `Task.parallel` や `Parallel` を使います。`Task<T>` は作成時には実行されない、一回消費の計算です。native の `Task.parallel` は常駐ワーカープールを使い、結果を入力順に返します。UI をノンブロッキングにする非同期 I/O の仕組みではありません。

最適化や並列化による効果は処理内容と実行環境に依存します。小さい仕事では起動・確保・同期のコストが上回ることもあります。高速化は、生成コードと実測で確かめます。[性能ガイド](../guides/performance.md)と[ベンチマークの条件](../../docs/benchmarks.md)で、実装済みの経路と測定方法を確認してください。

## 計算とホストの役割を分ける

同じ言語から native と WASM を生成し、`export def` でホストへ関数を公開できます。ホストを呼ぶときは `extern def` で同期関数を宣言します。境界で渡せる型は、対応するスカラー・バッファ・スカラーレコードなどに限定されます。

標準入出力は `IO<T>` で扱います。IO アクションを入口の結果にすると実行され、native では標準ストリーム、WASM では明示的な IO ホストへ接続します。UI、DOM、ファイル、ネットワークなどはホスト側に置きます。

入出力や外部関数を使わない既定の計算用 WASM は、JavaScript ランタイムの import を必要としません。ただし、WASM の生成だけでブラウザーの画面やイベント処理までできるわけではありません。

WASM の SIMD128 と threads は明示的な opt-in です。既定のタスク実行は逐次で、threads を使う場合は共有メモリと対応する Worker ホストが必要です。詳しくは [C 連携](../guides/native-interop.md)、[WebAssembly](../guides/webassembly.md)、[WASM threads](../guides/wasm-threads.md)を参照してください。

## 開発に必要な道具をそろえる

CLI は型検査・ビルド・実行に加えて、テスト、整形、診断、デバッグ情報、ドキュメント生成を提供します。

- `check`: コード生成前に型と所有権を検査する。
- `test`: ソース内の `test` 宣言を実行する。
- `fmt`: AST の意味を変えずに空白とインデントを整える。
- `lsp`: 診断、型の hover、定義への移動などをエディターへ提供する。
- `doc`: 公開宣言とドキュメントコメントから API 文書を生成する。

[VS Code 拡張機能](../../vsc/README.md)には、環境別のコンパイラ・LLVM・SDK と、編集・実行・テストの支援があります。ソースデバッグの対応は OS と CPU により異なります。CLI の詳細は[開発ツール](../tools/README.md)を参照してください。

## 現在の範囲を知る

Tsuzuri は、計算部分を切り出して実行・組み込みできる初版です。既存の汎用言語のエコシステム全体を置き換えるものではありません。

- パッケージはローカル path 依存に対応します。registry、git 依存、版解決は未対応です。
- GPU は CPU 参照、strict 整数 WGSL 生成、WebGPU ホスト試作の段階です。通常ランタイムへの実 GPU 接続と自動 offload は未実装です。
- Windows 向けの実装はありますが、Windows 上の実行検証は未完了です。
- REPL と標準の GUI・ネットワーク API は提供しません。

詳細と更新状況は[対応状況](../feature-status.md)を確認してください。

## 次に読む

- [なぜ Tsuzuri か](why-tsuzuri.md): 設計の意図、使いどころ、採用前の確認事項。
- [コードサンプル](code-samples.md): 目的別の完成コードと期待する結果。
- [入門](../get-started.md): 環境の準備と最初の実行。
- [Tsuzuri のツアー](../tour.md): 基本概念を順番に学ぶ。

## 参考

構成の参考: [Zig Overview](https://ziglang.org/learn/overview/)。
