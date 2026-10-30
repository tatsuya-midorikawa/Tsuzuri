# for...in 式

<!-- rule: CTL-006 --> <!-- rule: CTL-007 --> <!-- rule: CTL-009 --> <!-- rule: CTL-022 --> <!-- rule: STD-010 -->
`for...in` 式は、配列、リスト、`Vec`、文字列、整数範囲、遅延シーケンス（`Seq`）の要素を順番に走査するループです。要素のパターン分解や `break` / `continue` も使え、結果は `()`（`unit`）です。

## この記事のポイント

<!-- rule: CTL-004 --> <!-- rule: CTL-005 -->
- 構文は `for パターン in コレクション do 本体` です。本体も式全体も `unit` です。
- 配列、リスト、`Vec`、`string`、`utf8string`、整数範囲、`Seq` を直接反復できます。整数範囲は終端を含む `a ..= b` と含まない `a ..< b` です。
- `Map`、`Set`、`HashMap`、`HashSet` やユーザー定義型は、`Module.iter` で `Seq` にしてから反復します。
- 配列、リスト、`Vec`、文字列の走査中はコレクションが共有借用されます。要素のための複製はありません。
- `string` は UTF-16 コード単位（`i16u`）、`utf8string` は UTF-8 バイト（`ubyte`）です。
- `for` のパターンは網羅性検査をしません。不一致は実行時トラップです。
- `break` で抜け、`continue` で次の要素へ進みます。

## 基本の書き方

```text
for pattern in source do
    body
```

`source` 式はループ開始時に 1 回だけ評価されます。各反復で要素が `pattern` に照合され、本体ブロックが実行されます。

```tsuzuri run=60
let numbers = [10, 20, 30]
let mut total = 0

for n in numbers do
    total = total + n

total
```

実行結果:

```text
60
```

ループ全体の評価値、および本体ブロックの型は `unit`（`()`）です。値を集計したい場合は、外側で `let mut` による可変変数を宣言して更新します。

```mermaid
flowchart TD
    start["ループ開始 (source を 1 回評価)"] --> check{"要素がまだあるか?"}
    check -- あり --> bind["要素をパターンに束縛"]
    bind --> body["本体を実行"]
    body --> continueNode{"continue または末尾"}
    continueNode --> check
    body -- break --> finish["ループ終了 (unit を返す)"]
    check -- なし --> finish
```

## 反復できるコレクション

Tsuzuri の `for...in` が直接受理するコレクションは以下の通りです。

| コレクション | 列挙される要素 | 要素の型 | 走査の単位 |
| --- | --- | --- | --- |
| 配列 `[T]` | メモリ格納順の要素 | 要素型 `T` | 0 番目から末尾まで |
| 連結リスト `[\|T\|]` | 先頭からの要素 | 要素型 `T` | 先頭の chunk から順 |
| `Vec<T>` | インデックス昇順の要素 | 要素型 `T` | 0 番目から末尾まで |
| 整数範囲 `a ..= b`・`a ..< b` | 昇順または降順の整数 | その整数型 | 刻みごと |
| `string` | UTF-16 コード単位 | `i16u` | 16 bit（`.length` と同じ単位） |
| `utf8string` | UTF-8 バイト | `ubyte` | 8 bit のバイト |
| `Seq<'a>` | 遅延列挙される要素 | `'a` | `Seq.next` で 1 つずつ |

### 整数範囲の反復

`start ..= finish`（終端を含む）と `start ..< finish`（終端を含まない）、ステップ幅を指定する `start .. step ..= finish`・`start .. step ..< finish` を使って、整数範囲を走査できます。配列の添字を順に回すときは `0 ..< xs.length` と書きます。

```tsuzuri run=30
let mut sum = 0
for i in 1 ..= 4 do
    sum = sum + i * 3
sum
```

実行結果:

```text
30
```

刻みを省くと `1` です。負の刻みにすると降順になります。

```tsuzuri run=28
let mut result = 0
for n in 10 .. -2 ..= 4 do
    result = result + n
result
```

実行結果:

```text
28
```

```tsuzuri run=123
let words = ["a", "bb", "ccc"]
let mut total = 0
for i in 0 ..< words.length do
    total = total * 10 + words[i].length
total
```

実行結果:

```text
123
```

対応するのは 8 bit から 128 bit までの符号付き・符号なし整数です。開始、刻み、終了は同じ型で、この順に 1 回ずつ評価されます。刻みが `0` だと、範囲が空でも実行時に言語トラップします（`range step is zero`）。`..=` は終端を越えない値まで、`..<` は終端に届く手前の値までを列挙し（正の刻みなら `v < finish`、負の刻みなら `v > finish`）、次の値が型の範囲を外れるとその場で止まります。`125i8 .. 2i8 ..= 127i8` は `125` と `127`、`125i8 .. 2i8 ..< 127i8` は `125` だけです。`3 ..< 3` は空です。

`0 ..< xs.length` で `xs[i]` を読むループは、添字の境界検査を省けることをコンパイラが証明します（`0 ..= xs.length - 1` と同じです）。

範囲は `for...in` 専用の列挙構文です。変数に保存すると `E1005`（`a range is an enumerable expression for 'for...in', not a stored value`）になります。浮動小数点の範囲もありません。ステップは 1 つだけで、`0 ..= 3 .. 6` のように 2 つ目の演算子が `..=`・`..<` だと `E0002`（`a range takes at most one step; write 'start .. step ..= finish'`）です。

> [!NOTE]
> 演算子のない素の `..`（`for i in 1 .. 4`）は `..=` と同じ意味ですが、edition 2027 では `E0004` です（edition 2026 では受理し、`--warn canonical` で `W1007` を出します）。`tsuzuri fix --edition 2027` が `..=` に書き換えます。スライスの `xs[a..b]` の `..` は半開のままで、どの edition でも使えます。固定長配列 `[T; N]` も直接は反復できず、`E1005`（`a fixed-length array is not enumerable`）になります。添字の範囲で回して要素を読んでください。

### 文字列の走査（string と utf8string）

文字列の走査は、Unicode スカラー値（文字）単位ではなく、それぞれの型の内部表現単位で行われます。

```tsuzuri run=198
let text = "ABC"
let mut code_sum: i64 = 0

for code in text do
    code_sum = code_sum + code as i64

code_sum
```

実行結果:

```text
198
```

- `string` の走査は **UTF-16 コード単位（`i16u`）** です。
- `utf8string` の走査は **UTF-8 バイト（`ubyte`）** です。

単位は `.length` や添字アクセスと同じです。文字（Unicode スカラー値）単位ではありません。絵文字 `😀` は UTF-16 ではサロゲートペアなので、`string` の反復は 2 回です。

```tsuzuri run=2
let text = "😀"
let mut units = 0

for _code in text do
    units = units + 1

units
```

実行結果:

```text
2
```

`utf8string` は `u8"..."` で書けます。要素は `ubyte` です。

```tsuzuri run=65
let text = u8"A"
let mut total: i64 = 0

for code in text do
    total = total + code as i64

total
```

実行結果:

```text
65
```

## 直接反復と Seq

コンパイラがユーザー定義型の `iter` を暗黙に探すことはありません。直接渡せるのは、前の表の型だけです。`Vec` もその 1 つで、配列と同じように書けます。

```tsuzuri run=60
let values = Vec.push_owned 40 (Vec.push_owned 20 (Vec.empty()))
let mut total = 0

for item in values do
    total = total + item

total
```

実行結果:

```text
60
```

`Map`、`Set`、`HashMap`、`HashSet` やレコードなどのユーザー定義型は、`Module.iter` で `Seq` を作ってから渡します。`Vec.iter` も使えますが、こちらは要素そのものではなく `ref 'a` の列です。非 Copy の要素を読み取るだけなら、直接の `for` の方が単純です。

```tsuzuri run=60
let values = Vec.push_owned 40 (Vec.push_owned 20 (Vec.empty()))
let mut total = 0

for item in Vec.iter (ref values) do
    total = total + deref item

total
```

実行結果:

```text
60
```

`Map` や `Set` も同じ形です。`HashMap` と `HashSet` にも `iter` はありますが、`singleton` はないので `empty` と `insert` で作ります。

```tsuzuri run=42
let m = Map.singleton "answer" 42
let mut result = 0

for (key, value) in Map.iter (ref m) do
    if deref key == "answer" then
        result = deref value

result
```

実行結果:

```text
42
```

`iter` を介さず `Map` などを渡すと `E1005` になります。

```text
// コンパイルエラー E1005:
// for...in expects an array, list, string, utf8string, integer range, or Seq;
// call Module.iter explicitly for user-defined types
let scores = Map.singleton "answer" 42
for (key, value) in scores do ()
```

> [!NOTE]
> `Seq<'a>` は non-Copy 型のワンショットな値です。`for...in` で反復すると `Seq` 自身は消費（ムーブ）され、ループ終了後に再利用することはできません（再利用すると `E1012` になります）。

## タプルのパターン分解

ループの `pattern` 部分には、タプルやレコードなどの分解パターンを指定できます。

```tsuzuri run=14
let pairs = [(1, 2), (3, 4)]
let mut total = 0

for (left, right) in pairs do
    total = total + left * right

total
```

実行結果:

```text
14
```

> [!WARNING]
> `for` 式のパターン分解では、コンパイル時の網羅性検査は行われません。もしコレクション内にパターンと合致しない要素が存在した場合、要素をスキップするのではなく実行時に安全にトラップします。条件に一致するものだけを走査したい場合は、本体内で `match` や `if` を使ってください。

## 所有権と借用ルール

配列、リスト、`Vec`、文字列を走査するために、コレクション全体が複製されることはありません。走査中は共有借用です。

- **コレクションの保護**: ループの途中で元の値をムーブしたり、上書きしたり、排他借用したりすると拒否されます。
- **要素**: Copy 型は値が複製されます。非 Copy な要素は読み取り専用ビューで、`item.length` のような読み取りはできますが、所有値として持ち出せません（`E1012`）。`Vec.iter` の要素は `ref 'a` なので、読むときは `deref` します。
- **文字列のコード単位**: `for unit in text do ...` の `unit`（`i16u`）は、文字列の中の場所ではなく、反復ごとの値です。`ref unit` はその反復の中で使えます。外側の `ref` 変数へ代入したり、関数の戻り値にしたりして反復のあとまで持ち越すと、範囲の `for` の変数と同じ `E1013` です。配列の要素は配列の中の場所なので、配列が生きているあいだは `ref` を持ち越せます。残したいコード単位は `let kept = unit` で値として複製します。この規則は `TSUZURI_STRINGS` の値によって変わりません（`compact` の Latin-1 の文字列には、16 ビットのコード単位の場所がメモリ上にありません）。
- **一時値**: `for x in [1, 2, 3] do ...` のような一時コレクションは、ループが終わるまで生きています。
- **`Seq`**: 共有借用ではなく、所有権を 1 回消費します。ループのあとで同じ変数を使うと `E1012` です。

文字列のコード単位への `ref` は、反復の中で読めます。

```tsuzuri run=195
let text = "ab"
let mut total = 0
for unit in text do
    let seen = ref unit
    total = total + ((deref seen) as i64)
total
```

実行結果:

```text
195
```

反復のあとまで持ち越すと拒否されます。

```text
let text = "ab"
let zero = 0i16u
let mut saved = ref zero
for unit in text do
    if unit == 97i16u then saved = ref unit
```

```text
error[E1013]: a borrowed value cannot outlive its pattern or iteration binding
```

## break と continue

ループの流れを制御するために、`break` と `continue` が使えます。

```tsuzuri run=12
let mut sum = 0

for n in 1 ..= 10 do
    if n % 2 == 1 then
        continue
    if n > 6 then
        break
    sum = sum + n

sum
```

実行結果:

```text
12
```

- `break`: 最も内側のループを直ちに終了します。
- `continue`: 直ちに次の反復ステップへ進みます。整数範囲ループでも終端判定やオーバーフロー検査は正確に維持されます。

`break` と `continue` はともに `unit` 型です。値を返したり、ラベルを指定して外側のループから脱出したりする構文はありません。また、ラムダ式、並行タスク、コンピュテーション式、`finally` を持つ `try` 式の境界を跨いで脱出しようとするとコンパイルエラー `E1023` になります。

## 他の言語との比較

| 機能 | Tsuzuri | F# | Rust | Python |
| --- | --- | --- | --- | --- |
| 構文 | `for x in xs do` | `for x in xs do` | `for x in xs` | `for x in xs:` |
| 戻り値の型 | `unit` | `unit` | `()` | `None` |
| 反復の対象 | 配列・リスト・`Vec`・文字列・範囲・`Seq` は直接。ほかは `Module.iter` | `seq`（`IEnumerable`） | `IntoIterator` | `__iter__` |
| 文字列の列挙 | UTF-16 単位 / UTF-8 バイト | UTF-16 `char` | `chars()` / `bytes()` | Unicode 文字 |
| コレクションの複製 | 配列・リスト・`Vec` は共有借用。`Seq` は消費 | コピーなし | 所有権ムーブまたは借用 | 参照を走査 |

## まとめ

- `for...in` はコレクションを順に走査し、評価値は `unit`（`()`）です。
- 配列、リスト、`Vec`、文字列、整数範囲、`Seq` を直接反復できます。
- `Map`、`Set`、`HashMap`、`HashSet` やユーザー定義型は `Module.iter` で `Seq` にしてから反復します。
- 配列やリスト、`Vec` の走査は共有借用です。`Seq` は消費されます。
- `break` と `continue` で脱出やスキップができます。

## 関連項目

- [for...to 式](for-to.md)
- [while 式](while.md)
- [if 式](if.md)
- [Seq](../built-in-types-and-modules/seq.md)
- [Array](../built-in-types-and-modules/array.md)
- [List](../built-in-types-and-modules/list.md)
- [Vec](../built-in-types-and-modules/vec.md)
- [String](../built-in-types-and-modules/string.md)
- [言語仕様の制御構文](../../../docs/language.md#for--while)
- [言語リファレンスの目次](../index.md)

