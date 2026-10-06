# Vec

[ドキュメントのトップ](../README.md)

`Vec<T>` は長さと容量を別々に持つ伸縮可能な所有バッファです。要素型にかかわらず非 Copy で、更新操作は Vec を消費して更新後の Vec を返します。

## 基本例

```tsuzuri run=42
let mut values: Vec<i64> = Vec.with_capacity 2
values = Vec.push values 20
values = Vec.push values 22
let result = Vec.to_array values
Array.sum ref result
```

更新結果を可変ローカルへ再束縛すると、所有権を明示したまま構築を続けられます。`Vec.to_array` は所有バッファを配列へ移します。

## 作成・情報取得

| API | 型・動作 |
| --- | --- |
| `Vec.empty()` | 空の Vec。期待型から要素型を決定 |
| `Vec.with_capacity count` | i64 の容量を確保。長さは 0 |
| `Vec.length values` | `ref Vec<T> -> i64` |
| `Vec.capacity values` | `ref Vec<T> -> i64` |
| `Vec.is_empty values` | `ref Vec<T> -> bool` |

容量は初回 4 から倍増する方針です。負の容量、長さ・容量・サイズ積の overflow、確保失敗はトラップです。初期化済みの長さだけを読み取り・解放し、余った容量を要素として扱いません。

## 更新 API

| API の引数順 | 結果 |
| --- | --- |
| `push values value` | 末尾へ追加した Vec |
| `pop values` | `(残りの Vec, Maybe<T>)`。空なら None |
| `reserve values additional` | 追加個数を収容する容量を持つ Vec |
| `truncate values length` | 指定長を超える要素を解放した Vec |
| `clear values` | 全要素を解放した空の Vec |
| `set values index value` | 既存位置を置換。範囲外はトラップ |
| `swap values first second` | 二つの既存位置を交換。範囲外はトラップ |

すべて Vec モジュールの関数です。reserve の数は総容量ではなく追加個数です。truncate / clear は削除要素を先頭から解放します。容量管理と要素の寿命を混同しないでください。

```tsuzuri run=42
let values = Vec.push (Vec.empty()) "answer"
match Vec.pop values with
| (remaining, Maybe.Some text) ->
    assert (Vec.is_empty ref remaining)
    text.length * 7
| (_, Maybe.None) -> 0
```

pop は非 Copy 要素も所有値として取り出せます。添字読み取りで同じことをするのではありません。

## 借用と複製

| API | 契約 |
| --- | --- |
| `get values index` | 借用した Vec から Copy 要素の `Maybe<T>`。範囲外は None |
| `at values index` | `ref T`。範囲外はトラップ |
| `clone values` | Copy 要素を持つ独立した Vec を作る |
| `iter values` | 要素の共有借用を返す Seq |

添字と直接の for も使えます。非 Copy 要素は借用して扱い、要素の排他借用はできません。at や iter の参照が生きている間、元 Vec を更新・move できません。

## Array との移送

`Vec.of_array :: [T] -> Vec<T>` と `Vec.to_array :: Vec<T> -> [T]` は所有バッファを移し、要素を複製しません。

ただし、Copy 配列の元値を後で使う場合の引数取得や、スタック配列のヒープ移送は通常どおりです。「常に一切のコピーがない」という保証ではありません。空の配列の領域は適切に解放し、空 Vec の形へ正規化します。

クロージャーへ Vec を捕捉した場合、その関数値の複製は独立した内部スナップショットを作ります。Vec 自身が Copy になるわけではありません。

## API と関連項目

- [Vec のソース宣言](api/Vec.md): iter。その他の API はコンパイラ組み込みで、このページに記載
- [Array と List](arrays-and-lists.md)
- [所有権](../language-reference/ownership.md)
