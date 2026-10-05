# Array、List、共有スライス

[ドキュメントのトップ](../README.md)

配列は連続領域、List は単方向連結ノードに要素を保持します。どちらも生成後の要素は不変です。Tsuzuri の `[1, 2]` は配列、`[|1, 2|]` はリストであり、F# と記号が逆です。

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

裸の `values[start..end]`、可変スライス、`ref values[..]` は未対応です。全体の借用には `ref values` を使います。List や文字列にもこの構文を流用しません。

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

直接の `values[index] = value` や、要素の排他借用はできません。let mut でも値全体を置換するだけです。

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

```tsuzuri run=42
let values = List.cons 20 [|22|]
List.fold (\total value -> total + value) 0 (ref values)
```

List は `length`, `is_empty`, `copy`, `map`, `map_ref`, `fold`, `fold_ref`, `reverse`, `to_array`, `iter` を提供します。map と fold の引数順は Array と同様に callback が先、リストが最後です。値を取り出す callback と所有する複製結果には Copy、借用版には不要です。要素は O(n) の直接走査で処理します。`List.copy (ref values)` は `Array.copy` と同じく、暗黙の複製を明示する形です。

## API と関連項目

- [Array のソース宣言](api/Array.md)、[List のソース宣言](api/List.md)
- [Vec](vec.md)
- [Seq と iter](sequences.md)
