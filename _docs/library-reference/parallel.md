# Parallel

[ドキュメントのトップ](../README.md)

Parallel は配列の生成・変換・集計を固定チャンクで行う同期 API です。native では Task と同じ常駐プールを使い、既定の WASM では同じチャンク構成の逐次経路を使います。

## 配列の生成

```tsuzuri run=14
let values = Parallel.init 4 (index -> index * index)
Parallel.sum ref values
```

戻るときには全 worker が完了し、結果配列は添字順です。callback の開始・完了順序は規定しません。負の長さ、サイズ overflow、確保失敗はトラップです。

## API と引数順

| API | 型・制約 |
| --- | --- |
| `init count initializer` | `Send<T> => i64 -> (i64 -> T) -> [T]` |
| `map transform values` | `(Copy<T>, Send<T>, Send<U>) => (T -> U) -> ref [T] -> [U]` |
| `map_ref transform values` | `(Send<T>, Send<U>) => (ref T -> U) -> ref [T] -> [U]` |
| `reduce identity reducer values` | `(Copy<T>, Send<T>) => T -> (T -> T -> T) -> ref [T] -> T` |
| `sum values` | 数値型の正のゼロと加算による reduce |

Array.map と異なり Parallel.map は関数が先です。map は各入力要素を複製して渡すため、Copy でも大きい配列や関数環境の複製費用があります。非 Copy 要素には map_ref を使います。

```tsuzuri run=8
let texts = ["red", "green"]
let lengths = Parallel.map_ref String.length (ref texts)
Array.sum ref lengths
```

## callback の制約

callback が借用を捕捉すること、結果が借用を保持することは許可しません。共有入力の借用は fork/join の範囲に閉じています。所有環境を証明できない未知の callback や関数要素は `E1013` です。

init / map / map_ref / reduce 自体は直接の完全適用が必要です。これらの API を関数値にして保存する、部分適用してから呼ぶ、といった利用は未対応です。一方、callback として渡す値には、所有環境を証明できる通常の関数値・部分適用・lambda を使えます。

引数は記載順に一度だけ評価し、後続引数の処理で先に取得した callback のスナップショットを変更しません。

## チャンク境界

入力長を n とすると、空入力は 0 チャンク、それ以外は `min(1024, ceil(n / 4096))` チャンクです。実装は overflow を避けた整数演算で計算します。

チャンク番号 c、チャンク数 k の開始位置は `(n / k) * c + ((n % k) * c) / k`、終了位置は c を c + 1 に置き換えた位置です。除算は整数除算で、終端は含みません。

CPU 数、SIMD 幅、WASM backend で境界を変えません。小さい入力が一チャンクになれば、並列 API を呼んでも追加 worker の効果がない場合があります。

## reduce の順序

各チャンクは identity の複製から添字昇順で fold します。全完了後、呼び出し元が identity からチャンク番号順で部分結果を結合します。空入力の結果は identity です。

identity は各チャンクと最終結合で使われます。非結合演算や浮動小数点では、Array.reduce / Array.sum の逐次結果と異なる場合があります。同じ結果が必要なら演算順序が一致する API を選びます。

再結合、fast-math、暗黙 FMA を許可するスイッチではありません。順序の違いはこの API の明示的な契約です。

## 実行と失敗

native のスレッド上限、遅延起動、入れ子の進行は[Task](../language-reference/tasks.md)と共通です。WASM threads を明示して適切なホストを用意すると Worker 経路を使用できます。既定の WASM は import-free の逐次処理です。

トラップ時の部分結果解放・キャンセルは保証しません。一般の自動並列化や GPU offload は行いません。性能評価には、入力規模、確保、コピー、初期起動、同期、結果回収を含めます。

## API と関連項目

- [Parallel.sum のソース宣言](api/Parallel.md): 他の4操作は組み込み
- [逐次 Array とスライス](arrays-and-lists.md)
- [固定順の浮動小数点集計](math.md)
