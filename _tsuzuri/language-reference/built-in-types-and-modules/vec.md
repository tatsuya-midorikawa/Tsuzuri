# Vec

`Vec<T>` は、動的に要素を追加・削除できる所有権付きの可変長バッファです。内部に連続したメモリ領域（ヒープ）を持ち、末尾への要素追加を償却 $O(1)$ で行えます。

処理の途中で要素数が動的に変化する場面や、蓄積したデータを最後に [Array](./array.md) へ移送して利用するワークフローに最適です。

## この記事のポイント

- 型は `Vec<T>`。要素型にかかわらず常に非 Copy 型として扱われます。
- 空のバッファ `Vec.empty()` または事前確保 `Vec.with_capacity count` で生成します。
- 更新操作（`push`, `pop`, `set`, `swap` 等）は所有する `Vec` を消費して新しい `Vec` を返します。通常は `let mut` 変数に再代入して使用します。
- 内部容量（capacity）は必要に応じて初期値 4 から自動的に倍増します。あらかじめ要素数が予測できる場合は `with_capacity` や `reserve` で再確保を防げます。
- `pop` は `(残りのVec, Maybe<末尾要素>)` のタプルを返し、非 Copy な要素も所有値として安全に取り出せます。
- `Vec.to_array` と `Vec.of_array` は、所有しているバッファを移します。要素の複製はしません。`Copy` 配列の引数渡しと、スタック配列のヒープ昇格は通常どおりです。
- 要素そのものへの排他借用参照（`ref mut`）はバッファ破損を防ぐため取得できません。

## 基本の書き方と生成

### 生成と要素の蓄積

`Vec.empty()` で空のバッファを作成し、`Vec.push` で末尾に要素を追加します。更新操作は `Vec` の所有権を消費して更新後の `Vec` を返すため、`let mut` で宣言した変数に再束縛するパターンが標準的です。

```tsuzuri run=len%3D3%20cap%3D4
let mut values: Vec<i64> = Vec.empty()
values = Vec.push values 10
values = Vec.push values 20
values = Vec.push values 30
let len = Vec.length (ref values)
let cap = Vec.capacity (ref values)
$"len={len} cap={cap}"
```

実行結果:

```text
len=3 cap=4
```

### 事前確保 `Vec.with_capacity`

追加する個数が分かっているなら、`Vec.with_capacity count` で先に容量を確保すると、途中の再確保を避けられます。

```tsuzuri run=len%3D0%20cap%3D100
let buffer: Vec<string> = Vec.with_capacity 100
let len = Vec.length (ref buffer)
let cap = Vec.capacity (ref buffer)
$"len={len} cap={cap}"
```

実行結果:

```text
len=0 cap=100
```

`count` に負数を指定した場合や、合計バイト数がオーバーフローする場合は安全にトラップします。

## 要素の追加・削除・更新

### 末尾からの取り出し `Vec.pop`

`Vec.pop` は現在の `Vec` を消費し、更新後の `Vec` と末尾要素 `Maybe<T>` のタプル `(Vec<T>, Maybe<T>)` を返します。

```tsuzuri run=popped%3D30%20rem_len%3D2
let mut values: Vec<i64> = Vec.empty()
values = Vec.push values 10
values = Vec.push values 20
values = Vec.push values 30

let popped_val = match Vec.pop values with
| (remaining, Some value) ->
    values = remaining
    to_string value
| (remaining, None) ->
    values = remaining
    "none"

let rem_len = Vec.length (ref values)
$"popped={popped_val} rem_len={rem_len}"
```

実行結果:

```text
popped=30 rem_len=2
```

`pop` は、文字列や非 `Copy` のレコードも所有値として取り出せます。全フィールドが `Copy` のレコードは `Copy` です。

### 既存要素の置換と交換

- `Vec.set values index new_value`: 指定インデックスの要素を置き換えた新しい `Vec` を返します。
- `Vec.swap values i j`: 指定された 2 つのインデックスの要素を安全に入れ替えた `Vec` を返します。

いずれも範囲外のインデックスが指定された場合は安全にトラップします。

```tsuzuri run=result%3D%5B10%2C%2099%2C%2030%5D
let mut values: Vec<i64> = Vec.empty()
values = Vec.push values 10
values = Vec.push values 20
values = Vec.push values 30
values = Vec.set values 1 99
let arr = Vec.to_array values
$"result={arr}"
```

実行結果:

```text
result=[10, 99, 30]
```

## 容量管理と再確保

内部バッファの容量（capacity）は、要素の追加によって不足した際に初期値 4 から 2 倍ずつ自動的に拡張されます。

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `capacity` | `ref Vec<'a> -> i64` | 現在確保されている最大要素数を返します |
| `reserve` | `Vec<'a> -> i64 -> Vec<'a>` | 追加で `additional` 個の要素を格納できるよう容量を確保します |
| `truncate` | `Vec<'a> -> i64 -> Vec<'a>` | 指定した長さまで縮小し、溢れた要素を先頭側から順に解放します |
| `clear` | `Vec<'a> -> Vec<'a>` | 全要素を解放し、長さ 0 の空の `Vec` にします（容量は維持されます） |

`reserve values additional` は「総容量」ではなく「追加で必要な個数」を指定します。

```tsuzuri run=before%3D3%20after%3D1
let mut values: Vec<string> = Vec.empty()
values = Vec.push values "a"
values = Vec.push values "b"
values = Vec.push values "c"
let before_len = Vec.length (ref values)
values = Vec.truncate values 1
let after_len = Vec.length (ref values)
$"before={before_len} after={after_len}"
```

実行結果:

```text
before=3 after=1
```

## 要素へのアクセスと借用

### 添字アクセスと借用関数

- `values[i]`: 読み取り専用の要素そのものです。`ref` 型ではありません。範囲外はトラップします。非 `Copy` はムーブできず、共有借用は `ref values[i]` で取ります。
- `Vec.at (ref values) index`: `ref T` を返します。範囲外はトラップします。こちらは参照なので `deref` できます。
- `Vec.get (ref values) index`: 要素を `Maybe<T>` で返します。**`Copy<T>` 制約が必要**です。範囲外は `None` を返します。

```tsuzuri run=at%3Dhello%20get%3D42%20missing%3Dnone
let mut words: Vec<string> = Vec.empty()
words = Vec.push words "hello"

let mut numbers: Vec<i64> = Vec.empty()
numbers = Vec.push numbers 42

let word_ref = Vec.at (ref words) 0
let num_val = match Vec.get (ref numbers) 0 with
| Some v -> to_string v
| None -> "none"
let missing_val = match Vec.get (ref numbers) 99 with
| Some v -> to_string v
| None -> "none"

$"at={word_ref} get={num_val} missing={missing_val}"
```

実行結果:

```text
at=hello get=42 missing=none
```

> [!WARNING]
> **要素への排他借用参照は取得できません**
> `ref mut values[0]` のように要素そのものの排他借用は取れません。コンパイルエラーは `E1014` です。
> 要素の書き換えには `Vec.set` や `Vec.swap` を使用してください。また、`Vec.at` で得た共有借用参照が生存している間は、バッファ自体の更新や移動は禁止されます。

## Array とのゼロコピー移送

`Vec` と固定長配列 `[T]` は、相互に所有バッファをそのまま移送できます。

- `Vec.to_array values`: `Vec` を消費し、内部バッファを所有配列 `[T]` として返します。
- `Vec.of_array array`: 所有配列 `[T]` を消費し、そのバッファを持つ `Vec<T>` を返します。

```tsuzuri run=converted%3D%5B1%2C%202%2C%203%2C%204%5D%20sum%3D10
let mut buffer = Vec.of_array [1, 2, 3]
buffer = Vec.push buffer 4
let array = Vec.to_array buffer
$"converted={array} sum={Array.sum (ref array)}"
```

実行結果:

```text
converted=[1, 2, 3, 4] sum=10
```

所有しているバッファはポインタを渡すだけで移せます。ただし、`Copy` な配列を後で使うときの引数渡しや、スタック上の配列をヒープへ上げる処理は、通常の所有権規則どおりコピーします。「常に一切コピーしない」わけではありません。空の配列を渡すと、不要なバッファは解放され、空の `Vec` に正規化されます。

## API リファレンス

すべての関数は `std::Vec` モジュール（または言語組み込み）に属しています。

### 生成・情報取得

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `empty` | `fn() -> Vec<'a>` | 空の `Vec` を作ります。呼び出しは `Vec.empty()` です。`unit ->` ではありません |
| `with_capacity` | `i64 -> Vec<'a>` | 指定容量を事前確保した空の `Vec` を生成します |
| `length` | `ref Vec<'a> -> i64` | 現在格納されている要素数を返します（`.length` と同じ） |
| `capacity` | `ref Vec<'a> -> i64` | 現在確保されている内部容量を返します |
| `is_empty` | `ref Vec<'a> -> bool` | 要素数が 0 かどうかを返します |

### 要素操作・容量制御

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `push` | `Vec<'a> -> 'a -> Vec<'a>` | 末尾に要素を追加した新しい `Vec` を返します |
| `pop` | `Vec<'a> -> (Vec<'a> * Maybe<'a>)` | 末尾要素を取り出した結果と残りの `Vec` を返します |
| `set` | `Vec<'a> -> i64 -> 'a -> Vec<'a>` | 指定インデックスの要素を置換します（範囲外はトラップ） |
| `swap` | `Vec<'a> -> i64 -> i64 -> Vec<'a>` | 2 つの位置の要素を入れ替えます（範囲外はトラップ） |
| `reserve` | `Vec<'a> -> i64 -> Vec<'a>` | 追加要素数分の容量を確保します |
| `truncate` | `Vec<'a> -> i64 -> Vec<'a>` | 指定した長さまで縮小します |
| `clear` | `Vec<'a> -> Vec<'a>` | 全要素を解放して空にします |

### 借用・複製・変換

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `at` | `ref Vec<'a> -> i64 -> ref 'a` | 要素への共有参照を返します（範囲外はトラップ） |
| `get` | `Copy<'a> => ref Vec<'a> -> i64 -> Maybe<'a>` | 要素を `Some` で返します（範囲外は `None`） |
| `clone` | `Copy<'a> => ref Vec<'a> -> Vec<'a>` | `Copy` 要素を持つ独立した `Vec` を複製します |
| `to_array` | `Vec<'a> -> ['a]` | 内部バッファをそのまま配列 `['a]` へ移送します |
| `of_array` | `['a] -> Vec<'a>` | 配列 `['a]` を内部バッファとして取り込みます |
| `iter` | `ref Vec<'a> -> Seq<ref 'a>` | 各要素への共有参照を順次列挙する `Seq` を返します |

## 計算量

| 操作 | 計算量 | 備考 |
| --- | --- | --- |
| 末尾への追加 `Vec.push` | 償却 $O(1)$ | 容量超過時のみメモリ再確保とコピー（倍増） |
| 末尾からの削除 `Vec.pop` | $O(1)$ | バッファ末尾の要素を解放 |
| 添字アクセス `xs[i]` / `at` | $O(1)$ | 連続メモリへの直接アクセス |
| 要素の置換 `set` / `swap` | $O(1)$ | インデックス指定の即時更新 |
| 長さ・容量の取得 `length` / `capacity` | $O(1)$ | メタデータ参照 |
| 配列との相互変換 `to_array` / `of_array` | $O(1)$ | 所有バッファの移送。引数渡しのコピーとは別 |
| 全削除 `clear` / 短縮 `truncate` | $O(k)$ | 削除対象の $k$ 要素のデストラクタ実行 |
| 複製 `clone` | $O(n)$ | 要素のディープコピー |

## 反復の仕方

### `for ... in` ループ

`Vec` は `for ... in` で直接走査できます。`Vec.iter` は要りません。ループ変数は `ref` ではなく、読み取り専用の要素そのものです。`.length` のように読めますが、非 `Copy` を関数へ渡してムーブすると `E1012` です。共有借用が要るときは `ref text` と書きます。`Copy` の要素は、値として使うときに複製されます。

```tsuzuri run=total%3D60
let mut values: Vec<i64> = Vec.empty()
values = Vec.push values 10
values = Vec.push values 20
values = Vec.push values 30

let mut total = 0
for x in values do
    total = total + x
$"total={total}"
```

実行結果:

```text
total=60
```

```tsuzuri run=letters%3D3
let mut words: Vec<string> = Vec.empty()
words = Vec.push words "ab"
words = Vec.push words "c"
let mut letters = 0
for text in words do
    letters = letters + text.length
$"letters={letters}"
```

実行結果:

```text
letters=3
```

```text
for text in words do
    consume text
```

この断片は `E1012` です。`text` は `ref string` ではないので、`*text` も書けません。

### `Vec.iter` による遅延走査

遅延列として扱いたいときは `Vec.iter (ref values)` を呼びます。こちらは `Seq<ref 'a>` なので、ループ変数は `ref` です。直接の `for x in values` とは型が違います。

```tsuzuri run=total%3D2
let mut letters: Vec<string> = Vec.empty()
letters = Vec.push letters "a"
letters = Vec.push letters "b"

let mut total = 0
for letter in Vec.iter (ref letters) do
    total = total + letter.length
$"total={total}"
```

実行結果:

```text
total=2
```

## まとめ

- `Vec<T>` は常に非 `Copy` です。更新は `Vec` を消費して新しい `Vec` を返すので、`let mut` へ再代入します。
- 容量は初回 4 から倍増します。負の容量やサイズのオーバーフローはトラップします。
- 要素の排他借用は `E1014` です。書き換えは `Vec.set` と `Vec.swap` です。
- `to_array` / `of_array` は所有バッファを移します。引数渡しのコピーとは別です。

## 関連項目

- [Array](./array.md) — 固定長の連続メモリ配列
- [List](./list.md) — 不変な単方向連結リスト
- [Seq](./seq.md) — 遅延シーケンス
- [所有権とムーブ](../ownership-and-memory/ownership.md)
- [言語リファレンスの目次](../index.md)

