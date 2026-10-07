# Array、List、スライス

[ドキュメントのトップ](../README.md)

配列は連続領域、List は単方向連結ノードに要素を保持します。どちらも共有されている間は要素が不変です。配列の要素は、排他スライスを通してだけその場で置き換えられます。Tsuzuri の `[1, 2]` は配列、`[|1, 2|]` はリストであり、F# と記号が逆です。

## 作成と読み取り

| 項目 | Array | List |
| --- | --- | --- |
| 型 | `[T]`（`Array<T>` とも書ける） | `[\|T\|]` |
| リテラル | `[1, 2]` | `[\|1, 2\|]` |
| 空 | `[]` | `[\|\|]` |
| 実行時長の生成 | `new [T](count, initializer)` | `new [\|T\|](count, initializer)` |
| length | O(1)、i64 | O(1)、i64 |
| 添字 | O(1) | O(index + 1) |
| 直接の for | 順次走査 | O(n) のノード走査 |

長さは型の一部ではありません。空値には期待型が必要です。添字は i64、負数と範囲外はトラップです。実行時生成は長さと初期化関数を一度ずつ評価し、添字順に初期化します。長さ 0 でも関数を作る式は評価しますが、callback は呼びません。

負の長さ、サイズ積の overflow、確保失敗はトラップです。未初期化の要素は公開しません。束縛したリテラルと new の記憶域の違いは[所有権](../language-reference/ownership.md)を参照してください。

## 共有スライス

```tsuzuri run=42
let values = [10, 20, 22, 100]
let middle = ref values[1..3]
Array.sum middle
```

スライスの終端は含みません。型は `ref [T]` で、要素を複製せず元の配列を借用します。`ref values[start..]` と `ref values[..end]` も使えます。

`0 <= start <= end <= length` を要求し、違反はトラップです。同じ開始・終了の空区間も有効です。再スライスは元の借用を引き継ぎます。

裸の `values[start..end]` と `ref values[..]` は未対応です。全体の借用には `ref values` を使います。List や文字列にもこの構文を流用しません。名前付きの[固定長配列](#固定長配列)も同じ構文で `ref [T]` のスライスにできます。

## 排他スライスとその場の更新

```tsuzuri run=63
let mut values: [i64] = [1, 2, 3, 4]
let tail = ref mut values[1..]
Array.write tail 0 20
Array.swap_in tail 1 2
let scaled = tail[1] * 10
Array.write tail 1 scaled
let mut sum = 0i64
for value in tail do sum = sum + value
sum
```

`ref mut values[start..end]`（`&mut values[start..end]`）は `let mut` の配列の一部を排他的に借ります。型は `ref mut [T..]` で、長さは固定です。範囲の規則とトラップは共有スライスと同じで、少なくとも一方の境界が必要です（全体は `ref mut values[0..]`）。添字、`length`、`for`、再スライス、`ref [T]` を取る関数への受け渡しは共有スライスと同じように使えます。

| API | 動作 |
| --- | --- |
| `Array.write slice index value` | 境界を検査してから旧要素を解放し、value を書き込む |
| `Array.swap_in slice first second` | 二要素をその場で交換。同じ添字なら変更なし |
| `Array.split_at_mut slice middle` | `0 <= middle <= length` を検査し、`[0..middle)` と `[middle..length)` の二つの排他スライスを返す |
| `Array.sort_in_place slice` | Ord による安定整列。作業領域を確保しない |

```tsuzuri run=300
let mut values: [i64] = [1, 2, 3, 4]
match Array.split_at_mut (ref mut values) 2 with
| (left, right) ->
    Array.write left 0 10
    Array.write right 0 30
    Array.write left 1 20
    Array.write right 1 40
values[0] + 2 * values[1] + 3 * values[2] + 4 * values[3]
```

`ref mut [T..]` の引数には、`ref mut values`、`let mut` の配列そのもの、`ref mut [T]` の引数も渡せます。いずれも配列全体の排他スライスになります。逆に、排他スライスを `ref mut [T]` の引数へ渡すと配列全体の置換ができてしまうので拒否します。`deref slice = other` も長さが変わるので拒否します。

```tsuzuri run=13499
def fill :: ref mut [i64..] -> i64 -> unit
fn fill values value =
    for index in 0i64 .. (values.length - 1) do Array.write values index value

let mut digits: [i64] = [3, 1, 4, 2, 0]
fill (ref mut digits[3..]) 9
Array.sort_in_place (ref mut digits)
digits[0] * 10000 + digits[1] * 1000 + digits[2] * 100 + digits[3] * 10 + digits[4]
```

排他スライスが生きている間は、元の配列の読み取り・移動・置換と、重なる別のスライスの作成ができません。`Array.write slice 0 (slice[1] * 10)` のように、同じスライスを書き込みの引数の中で読むことも拒否されるので、先に `let` で読み出します。`split_at_mut` の二つの半分は一つの借用として扱うので、片方を貸し直している間はもう片方も使えません。排他スライスは関数値・task・コレクション・export の境界へ入れられません。

`Array.set` などの所有値を返す更新は配列を消費して新しい値を返します。所有者を保ったまま一部だけを書き換えたいとき、再帰的に分割して処理したいときは排他スライスを使います。互いに素な区間を並列に書き込むには [Parallel.for_each_chunk](parallel.md) を使います。

## 固定長配列

```tsuzuri run=28
let squares: [i64; 4] = FixedArray.init (\i -> i * i)
let middle = ref squares[1..3]
Array.sum squares + Array.sum middle * 2 + squares.length
```

`[T; N]` は長さを型に含む値の配列です（[型](../language-reference/types.md#固定長配列)）。`FixedArray.init initializer` は、期待される型の長さ N について `initializer` を添字 0 から N - 1 の順に一度ずつ呼び、結果を並べた `[T; N]` を返します。期待される型から長さが決まらなければ `E1015`、固定長配列でない型が期待されれば `E1005` です。リテラル `[a, b, c]` も、固定長配列が期待される位置ではその値になります。

固定長配列の名前付きの値（束縛・フィールド・参照外し）は、`ref values[start..end]` で共有スライスにでき、`ref [T]` の引数へはそのまま渡せます（配列全体の共有スライスになります）。そのため `Array.sum` や `Simd.load` などの `ref [T]` を受け取る API をそのまま使えます。関数の戻り値などの一時値は、先に `let` で束縛します。所有する `[T]` との暗黙の変換はなく、`new [T](N, \i -> values[i])` のように明示的に作ります。排他スライス、要素の代入、パターンによる分解、`for` による列挙はありません。添字は `for index in 0 .. values.length - 1` のように使います。

## 所有値を返す更新

```tsuzuri run=42
let original = [1, 2, 3]
let updated = Array.set original 1 42
assert (original[1] == 2)
updated[1]
```

| API | 所有権と動作 |
| --- | --- |
| `Array.set values index value` | 配列を消費。旧要素を解放して置換 |
| `Array.update values index transform` | 配列を消費。旧要素の所有権を callback へ渡す |
| `Array.swap values first second` | 配列を消費。二要素を交換。同じ添字なら変更なし |
| `List.cons value values` | リストを消費し、先頭要素を追加した値を返す |
| `List.tail values` | リストを消費し先頭を解放。空ならトラップ |

Array の更新は引数をすべて左から右に評価した後、要素へ触れる前に境界を検査します。唯一使用の所有ヒープバッファは再利用できますが、上のように Copy の元値を再使用するなら独立コピーを更新します。

直接の `values[index] = value` や、要素の排他借用はできません。その場で書き換えるには、上の排他スライスと `Array.write` を使います。

## Array の取得と構築

以下の読み取り用 values は共有借用です。

| API の引数順 | 結果・境界 |
| --- | --- |
| `length values`, `is_empty values` | i64、bool |
| `get values index` | Copy 要素の `Maybe<T>`。範囲外は None |
| `at values index` | `ref T`。範囲外はトラップ |
| `init count initializer` | 添字から新しい所有配列を生成 |
| `sub values start count` | Copy 要素を複製。不正範囲はトラップ |
| `reverse values` | Copy 要素を逆順に複製 |
| `copy values` | Copy 要素をすべて複製した新しい配列。暗黙の複製の明示形で、`W1006` の対象外 |
| `append left right`, `concat arrays` | Copy 要素を連結した新しい配列 |
| `zip left right` | 各要素を複製したタプル配列。長さ不一致はトラップ |
| `to_list values` | Copy 要素を新しいリストへ |

## Array の変換と fold

高階関数は F# と同じく callback を先、配列を最後に受け取ります。最後の引数を `|>` で渡せるので、変換を左から順に繋げられます。

```tsuzuri run=8
let texts = ["red", "green"]
let total = texts |> Array.map_ref (\text -> text.length) |> Array.sum
assert (texts.length == 2)
total
```

`ref [T]` を受け取る引数へ `|>` で渡した配列は共有借用になり、呼び出し後も使えます。map_ref が返した一時配列も、そのまま Array.sum の借用引数へ渡せます。ラムダ式の引数は他の引数の後で型検査するので、`text` の型は配列から決まります。

| API の引数順 | callback と動作 |
| --- | --- |
| `map transform values`, `mapi transform values` | `T -> U`、`i64 -> T -> U`。T は Copy |
| `map_ref transform values`, `mapi_ref transform values` | `ref T -> U`、`i64 -> ref T -> U` |
| `fold folder initial values` | `State -> T -> State`。T は Copy |
| `fold_ref folder initial values` | `State -> ref T -> State` |
| `fold_back folder values initial` | `T -> State -> State`。末尾から。T は Copy |
| `fold_back_ref folder values initial` | `ref T -> State -> State`。末尾から |
| `reduce folder values` | `T -> T -> T`。先頭を初期値にし、空なら None |
| `filter predicate values` | `ref T -> bool` を一要素一回実行し、採用した Copy 要素を複製 |

通常は先頭から順に処理します。fold_back は F# の foldBack と同じく配列を初期値より先に受け取り、callback の引数順も fold と逆です。`reduce_ref` という API はありません。

```tsuzuri run=123321
let digits = [1, 2, 3]
let forward = Array.fold (\total digit -> total * 10 + digit) 0 (ref digits)
let backward = Array.fold_back (\digit total -> total * 10 + digit) (ref digits) 0
forward * 1000 + backward
```

## Array の検索・比較・整列

| API | 結果・規則 |
| --- | --- |
| `any predicate values`, `all predicate values` | 借用述語で短絡。空なら false / true |
| `count predicate values` | 借用述語に一致した個数 |
| `find predicate values` | 最初の一致の `Maybe<ref T>` |
| `index_of values target`, `contains values target` | target も共有借用。Eq による短絡検索 |
| `equal left right` | 長さと要素を比較。Eq が必要 |
| `min values`, `max values` | `Maybe<ref T>`。空なら None |
| `sort values` | Ord と要素複製が必要な安定整列 |
| `sort_by compare values` | compare は `ref T -> ref T -> i64`。負・ゼロ・正 |
| `binary_search values target` | 同じ順序で整列済みの配列から重複の先頭 index を返す |

述語と compare も先に受け取ります。index_of、contains、binary_search の target は callback ではないので、配列が先です。

sort は bottom-up の安定 merge sort です。f32 / f64 では NaN を数値の後へ配置し、NaN 同士や既定比較の符号付きゼロの入力順を保ちます。min / max は改善時だけ置換し、同値や先頭 NaN の順序を維持します。

## 集計と List API

Array.sum / product は数値を左から右に集計し、空なら 0 / 1 です。浮動小数点用の sum_pairwise、sum_kahan、dot、dot_fma は順序が異なる別 API です。

native の exe / object では、整数 8 型の Array.sum、Array.min、Array.max が CPU の命令セットに合わせた kernel を使います（[実行時 CPU dispatch](../guides/performance.md#実行時-cpu-dispatch)）。整数の和は折り返すので加算順によらず同じ値になり、min / max は最初に現れる位置を返すので、結果は std の本体と変わりません。

```tsuzuri run=42
let values = List.cons 20 [|22|]
List.fold (\total value -> total + value) 0 (ref values)
```

List は `length`, `is_empty`, `copy`, `map`, `map_ref`, `fold`, `fold_ref`, `reverse`, `to_array`, `iter` を提供します。map と fold の引数順は Array と同様に callback が先、リストが最後です。値を取り出す callback と所有する複製結果には Copy、借用版には不要です。要素は O(n) の直接走査で処理します。`List.copy (ref values)` は `Array.copy` と同じく、暗黙の複製を明示する形です。

## API と関連項目

- [Array のソース宣言](api/Array.md)、[List のソース宣言](api/List.md)
- [Vec](vec.md)
- [Seq と iter](sequences.md)
