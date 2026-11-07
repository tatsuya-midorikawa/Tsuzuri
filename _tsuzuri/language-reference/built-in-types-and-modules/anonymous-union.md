# 匿名共用体

匿名共用体は、要素の型そのものがケースを表す共用体です。`union` を宣言せずに `(i64 | string)` と書き、「`i64` か `string` のどちらか」を表します。期待される型が匿名共用体の位置では、要素の型の値をそのまま書けます。取り出すときは型テストのパターン `:? i64 as n` を使い、`match` が網羅性を検査します。F# の提案 FS-1092（Anonymous Type-tagged Unions）に当たる機能です。

## この記事のポイント

- 型は括弧で囲んで `(i64 | string)` と書きます。要素は 2〜32 個です。
- 要素の順序と入れ子は意味を持ちません。`(string | i64)` と `(i64 | string)` は同じ型で、`(i64 | (string | bool))` は `(i64 | string | bool)` です。
- 期待される型が匿名共用体の位置では、要素の型の値がそのまま匿名共用体の値になります（注入）。構築用の関数はありません。
- 取り出しは型テストのパターン `:? T`・`:? T as x` です。`match` の網羅性を検査し、足りなければ `E1021`（`missing: :? bool`）です。
- 型変数・区別できない要素・重複した要素は `E1041`、借用を含む要素は `E1013`、33 個以上の要素は `E1017` です。
- 比較・表示・ハッシュはまだありません（`==` や `$"{v}"` は `E1005`）。値は `match` で取り出して使います。
- 名前付きの `union` と同じ表現で、native と WebAssembly で同じ結果です。新しいランタイム関数や import は使いません。

## 基本の書き方

```tsuzuri run=int%2042
def describe :: own (i64 | string | bool) -> string = \value ->
    match value with
    | :? i64 as n -> $"int {n}"
    | :? string as s -> s
    | :? bool as b -> if b then "yes" else "no"

let first: (string | bool | i64) = 42
describe first
```

実行結果:

```text
int 42
```

`first` の型 `(string | bool | i64)` と引数の型 `(i64 | string | bool)` は、要素の順序が違うだけなので同じ型です。`42` は、期待される型の要素のうち整数の `i64` として包まれます。腕を 1 つ消すと、足りない要素を示す `E1021` になります。

```text
match is not exhaustive; missing: :? bool
```

edition 2027 では、非 Copy の型の引数は既定で読み取りの引数（参照）です。`| :? string as s -> s` のように要素を外へ move する関数は、引数を `own` にします（名前付きの共用体と同じ規則です。[所有権](#所有権)）。

## 型の書き方

```text
type-primary = ... | "(" type { "|" type }+ ")"
```

- 括弧は省略できません。`i64 | string` とは書けません（`union` の宣言や `match` の腕と区別するためです）。
- 各要素は型全体です。`->` と `*` は `|` より強く結び付くので、`(i64 -> i64 | string)` は `((i64 -> i64) | string)`、`(i64 * i64 | string)` は `((i64 * i64) | string)` です。読みやすさのため、関数型の要素は括弧で囲むことを勧めます。
- 要素の間に `||` を書くと `E0002`（kind `anonymous_union_double_bar`、`use a single '|' between the members of an anonymous union, as in (i64 | string)`）です。先頭の `|`（`(| i64 | string)`）は書けません。
- 要素が匿名共用体なら、その要素で置き換えます（平坦化）。型別名を経た入れ子も同じです。
- 要素の順序は型を変えません。どのモジュールで書いても同じ型です。
- 型を書ける位置ならどこにでも書けます。束縛の注釈、関数の署名、レコードのフィールド、共用体のペイロード、型引数（`Maybe<(i64 | string)>`）、型別名です。

コンパイラは要素を決まった順（正準順序）に並べ、診断や hover はその順で表示します。順序はまず型の種類で決まり、数（整数はビット幅の順で、同じ幅では符号なしが先。浮動小数点、十進浮動小数点が続く）、SIMD 型、`bool`、`unit`、`char`、`utf8char`、`string`、`utf8string`、レコード、共用体、配列、固定長配列、リスト、`Vec`、タプル、関数、`dyn` 型、`Rc`・`Arc` の順です。同じ種類どうしは、レコードと共用体は宣言の修飾名、配列などは要素、タプルは要素数と要素で比べます。`(string | bool | i64)` は `(i64 | bool | string)` と表示されます。

要素の多い型は、`|` を行頭に揃えて複数行に書けます。`tsuzuri fmt` はこの形を保ちます。

```tsuzuri
type Setting =
    (i64
    | f64
    | string
    | bool)

def width :: own Setting -> i64 = \setting ->
    match setting with
    | :? i64 as n -> n
    | :? f64 -> 0
    | :? string as s -> s.length
    | :? bool -> 1
```

型注釈のパターンで匿名共用体を書くときは、型を括弧で囲んで `(x: (i64 | string))` と書きます。`(x: i64 | string)` は、`(x: i64)` と `string` の OR パターンとして読まれます。

## 値の作り方（注入）

期待される型が匿名共用体 `U` の位置で、式の型が `U` の要素のどれか 1 つに決まるとき、式の値をその要素として包みます。値は変えず、タグを付けるだけです（`Some x` と同じ構築です）。暗黙の変換をしない Tsuzuri の、ただ 1 つの例外です。

| 期待される型が決まる位置 | 例 |
| --- | --- |
| 型注釈のある `let` | `let v: (i64 \| string) = 42` |
| 関数の結果 | `def make :: i64 -> (i64 \| string) = \n -> if n > 0 then n else "none"` |
| 引数とパイプ（`own` の引数も読み取りの引数も） | `width 40`、`"ab" \|> width` |
| レコードのフィールド、共用体のペイロード、ジェネリックな構築子 | `Cell { value: 40 }`、`Node 7`、`let m: Maybe<(i64 \| string)> = Some 1` |
| 配列・リスト・タプルのリテラルの要素 | `let xs: [(i64 \| string)] = [1, "two"]` |
| `if`・`match`・ブロックの腕 | `if flag then 1 else "x"`（腕ごとに包む） |
| 比較演算子の右辺 | `v == 1`（`1` を包む。ただし `==` そのものは `E1005`） |

式の種類ごとに、要素の選び方が決まっています。

- **接尾辞のない整数のリテラル**は、整数の要素を選びます。整数の要素がなければ `bigint` の要素、それもなければ浮動小数点の要素です。同じ組に 2 つ以上あれば、既定の型を選ばず `E1041`（kind `anonymous_union_ambiguous_literal`）です。`let v: (i32 | i64 | string) = 16` は `the literal 16 could be i32 or i64 in (i32 | i64 | string)` で、help が `16i32` のように接尾辞を示します。`let v: (f64 | string) = 2` は `f64` の `2.0` です。
- **接尾辞のない浮動小数点のリテラル**は、浮動小数点の要素を選びます。2 つ以上あれば同じ `E1041` です。
- **接尾辞のあるリテラル**は、その型の要素になります。範囲の検査も変わりません（`let v: (i8u | string) = -1` は `E1009`）。
- **ラムダ**は、関数型の要素がちょうど 1 つならその型で検査します。引数の型を注釈しなくて構いません。
- **`Dyn.of x`** は、`dyn` 型の要素がちょうど 1 つならその型で検査します。
- **それ以外の式**は、期待される型なしで検査し、得た型で決めます。合いうる要素が 1 つなら包み、1 つもなければ `E1003`（kind `anonymous_union_not_member`、`expected (i64 | string), found bool; bool is not a member of the anonymous union`。help が要素を並べます）、2 つ以上なら `E1041`（kind `anonymous_union_ambiguous_member`。`(Maybe<i64> | Maybe<string>)` に `None` を渡したときなど）です。式の型が匿名共用体そのものなら包まず、同じ型であることを求めます。型の決まらない式（総称の関数の結果など）は、匿名共用体そのものになります。
- **参照 `ref T` の値**は、参照先の `T` として要素を選びます。Copy の値は複製し、非 Copy の値は move できないので `E1012` です。

次は注入しません。どれも既存の `E1003` などのままです。

- 入れ物や関数の中: `[i64]` を `[(i64 | string)]` へ、`i64 -> unit` を `(i64 | string) -> unit` へは変えません。
- 参照の型: `ref i64` を `ref (i64 | string)` へは変えません（表現が違います）。
- 期待される型のない位置: `let xs = [1, "a"]` は `E1003` です。
- 型変数の推論: `'a` を要素の型から匿名共用体へ広げません。

F# の提案の例と同じく、「どれかの型」を受け取る関数を、オーバーロードなしで書けます。

```tsuzuri run=12pt%2010px%20small
def font_size :: own (f64 | i64 | string) -> string = \size ->
    match size with
    | :? f64 as points -> $"{points}pt"
    | :? i64 as pixels -> $"{pixels}px"
    | :? string as text -> text

$"{font_size 12.0} {font_size 10} {font_size "small"}"
```

実行結果:

```text
12pt 10px small
```

`10` は整数の要素 `i64`、`12.0` は浮動小数点の要素 `f64` です。異なる型の値を並べたデータも、要素の型を注釈すれば書けます。

```tsuzuri run=id%3D7%3Bratio%3D0.5%3Bname%3D%22tsuzuri%22%3B
let row: [(string * (i64 | f64 | string))] = [("id", 7), ("ratio", 0.5), ("name", "tsuzuri")]
let mut text = ""
for (key, value) in row do
    let shown = match value with
        | :? i64 as n -> $"{n}"
        | :? f64 as x -> $"{x}"
        | :? string as s -> $"\"{s}\""
    text = text + $"{key}={shown};"
text
```

実行結果:

```text
id=7;ratio=0.5;name="tsuzuri";
```

## `:?` で取り出す

```text
pattern-atom = ... | ":?" type-product [ "as" binder ]
```

- `:?` は `:` と `?` を空けずに書きます（`: ?` は `E0002 expected a pattern`）。
- `:?` の後の型は、タプルなら括弧なしで書けます。関数型と匿名共用体は括弧で囲みます（`:? (i64 -> i64) as f`）。
- `as x` は型テストの一部で、`x` の型はテストした要素の型です。共用体の値全体は `(:? i64 as n) as whole` で束縛します。
- 照合する値の型は、パターンを検査する時点で匿名共用体に決まっていなければなりません。決まっていなければ `E1041`（kind `type_test_unknown_scrutinee`）で、引数や束縛に型を注釈します。匿名共用体でない値は `E1041`（kind `type_test_not_anonymous_union`）です。名前付きの共用体はケース名で照合します。
- `:?` の型は本体の型注釈と同じ規則で解決します。署名の型変数を書けます（`:? Maybe<'a> as found`）。要素でない型は `E1041`（kind `type_test_not_member`。help が要素を並べます）、匿名共用体は `E1041`（kind `type_test_union`）です。
- 型テストは静的に決まったタグの比較です。実行時の型情報は使いません。

型テストは、ほかのパターンと同じ位置に書けます。`match` と関数ガードのほか、共用体のペイロード（`Some (:? string as s)`）、タプル（`(:? i64 as n, :? string as s)`）の中にも書けます。

```tsuzuri run=42
def width :: (i64 | string) -> i64 = \value ->
    | :? i64 as n -> n
    | :? string as s -> s.length

def first_width :: Maybe<(i64 | string)> -> i64 = \found ->
    match found with
    | Some (:? i64 as n) -> n
    | Some (:? string as s) -> s.length
    | None -> 0

let chosen: (i64 | string) = if width "ab" > 1 then 40 else "forty"
width chosen + first_width (Some "ab")
```

実行結果:

```text
42
```

ラムダ式の引数と `for...in` にも書けます。網羅性は検査せず、一致しなければ実行時にトラップします（`let` は名前しか取らないので書けません）。

```tsuzuri run=6
let values: [(i64 | string)] = [1, 2, 3]
let mut total = 0
for :? i64 as n in values do total = total + n
total
```

網羅性は要素ごとのケースとして数えます。すべての要素をガードなしでテストすれば網羅的です。足りなければ `E1021` で、足りない要素を `:? T` で示し、`| :? bool -> unreachable ()` を足す修正案（`maybe`）を持ちます。ペイロードの中では `missing: Some (:? string)` のように示します。同じ要素の 2 回目のテストは `W1003`（到達不能）です。

## 要素の規則

| 書いたもの | 結果 |
| --- | --- |
| `('a \| i64)`（型変数そのもの、または頭が型変数の型） | `E1041`（kind `anonymous_union_type_variable`）。`type OrInt<'a> = ('a \| i64)` も同じ |
| `(Maybe<'a> \| Maybe<i64>)`（型引数によっては同じ型） | `E1041`（kind `anonymous_union_indistinct_members`） |
| `type Meters = f64` のときの `(Meters \| f64)` | `E1041`（kind `anonymous_union_duplicate_member`。`f64 appears more than once in the anonymous union (f64 \| f64)`） |
| `(ref string \| i64)` | `E1013`（kind `anonymous_union_borrowed_member`、`an anonymous union holds owned values; ref string is borrowed`） |
| 平坦化した後の要素が 33 個以上 | `E1017`（kind `anonymous_union_member_limit`） |

`(Maybe<'a> | i64)` は書けます。頭の `Maybe` と `i64` で区別でき、型引数を置き換えても順序と区別は変わりません。`(Maybe<i64> | Maybe<string>)` も書けます。関数型の要素は、純粋かどうかと、カリー化を平らにした引数と結果で比べます（引数の規約は比べません）。

## 型の性質

匿名共用体は、要素数ごとにコンパイラが合成した総称の共用体の具体化です。そのため、次はすべて名前付きの共用体の規則がそのまま決めます。

| 性質 | 規則 |
| --- | --- |
| Copy・Send・Sync・drop | 要素から構造的に決まります。`(i64 \| bool)` は Copy、`(i64 \| string)` は Copy ではありません |
| 大きさ | 1 バイトのタグと、最大の要素が入る領域です。どの値もタグを持ちます。すべての要素の表現が同じ（`(i64 \| i64u)`）なら、ペイロードの領域を共有します |
| 再帰 | `union Tree = Leaf \| Node of (i64 \| Tree)` のように名前付きの型の循環に入ると、名前付きの共用体と同じくノードをヒープに置きます |
| 大きさの上限 | 具体型ごとに 64 KiB（`E1010`） |
| `export`・`extern` | 使えません（`E1008`） |
| GPU カーネル | 使えません（`E1018`） |
| 比較・表示・ハッシュ | ありません。`==`・`<`・`$"{v}"`・`Hash.hash` は `E1005`、匿名共用体を持つ型の `deriving (Eq, ...)` は `E1025` です |
| インスタンス | 匿名共用体を head とする `instance` は書けません（`E1041`、kind `anonymous_union_instance`） |

```tsuzuri run=3
union Tree = Leaf | Node of (i64 | Tree)

def rec depth :: Tree -> i64 = \tree ->
    match tree with
    | Leaf -> 0
    | Node (:? i64) -> 1
    | Node (:? Tree as rest) -> 1 + depth rest

depth (Node (Node (Node 7)))
```

## 名前付きの union との使い分け

匿名共用体が向いているのは、「どれかの型」を受け取る関数（オーバーロードの代わり）と、異なる型の値を並べたデータです。次の場合は名前付きの `union` を使います（F# の提案の Guidance と同じ考え方です）。

- ケースの名前に意味がある（`Celsius of f64` と `Fahrenheit of f64`）。
- 同じ型を 2 回持つ（匿名共用体では要素の重複は `E1041` です）。
- 要素が多い、または公開 API として長く保つ。
- 比較・表示・`deriving` が要る。

## 所有権

注入は `Some x` と同じ値の使用です。Copy の値は複製し、非 Copy の値は共用体の中へ move します。注入する式は 1 回だけ評価し、左から右の評価順も変わりません。

```text
let name = "abc"
let v: (i64 | string) = name
name.length          // E1012: use of moved or partially moved value 'name'
```

edition 2027 の読み取りの引数は参照なので、非 Copy の型の読み取りの引数を、そのまま匿名共用体へ渡すことはできません（`E1012`。help が `own` を案内します）。引数を `own` にするか、値を複製して渡します。型テストの束縛は共用体のケースのペイロードと同じ規則で、所有する値からは move、参照やコレクションの要素からは読み取り専用のビューです。

## 他の言語との比較

| | Tsuzuri | F#（提案 FS-1092） | TypeScript | Rust |
| --- | --- | --- | --- | --- |
| 型の書き方 | `(i64 \| string)` | `(int \| string)` | `number \| string` | なし（`enum` を宣言する） |
| 値の作り方 | 期待される型で自動で包む | 期待される型で自動で包む | そのまま代入（構造的な部分型） | `Value::Int(1)` と書く |
| 取り出し | `:? T as x`（網羅性を検査） | `:? T as x` | `typeof` などで絞り込む | `match` でバリアント名 |
| 順序と入れ子 | 可換・平坦化 | 可換・平坦化 | 可換・平坦化 | — |
| 実行時の型情報 | 使わない（静的なタグ） | — | 使う（`typeof`） | 使わない |

## 注意点

- 比較・表示・ハッシュはまだありません。現在は `match` で取り出してから比べたり表示したりします。
- 要素の少ない共用体を、要素の多い共用体の位置へそのまま渡すことはできません（`(i64 | string)` を `(i64 | string | bool)` へ渡すと `E1003`）。`match` で取り出して包み直します。

```text
def widen :: own (i64 | string) -> (i64 | string | bool) = \value ->
    match value with
    | :? i64 as n -> n
    | :? string as s -> s
```

- 読み取りの引数（`ref U`）へ渡した `if`・`match`・ブロックは、腕に `U` を期待します。腕が匿名共用体への参照（ほかの読み取りの引数など）だと `E1003` です（名前付きの共用体なら借用し直されます）。値をいったん束縛してから渡すか、引数を `own` にします。
- `tsuzuri api-diff` の既定の比較は構文だけを見るので、要素の順序の違いは差分にしませんが、型別名は展開しません（`(Meters | string)` と `(f64 | string)` は別の型として報告されます。`--effective` では同じ型です）。
- デバッガーは、要素の型名でケースを表示します（`i64(42)`。[デバッグ](../compiler/debugging.md)）。

## まとめ

- 匿名共用体 `(A | B)` は、宣言なしで「どれかの型」を表します。順序と入れ子は意味を持ちません。
- 期待される型が匿名共用体の位置で、要素の型の値を包みます。リテラル・ラムダ・`Dyn.of` は形で要素を選び、曖昧なら `E1041` です。
- `:? T as x` で取り出し、`match` が網羅性を検査します。
- 型変数・重複・区別できない要素は `E1041`、借用は `E1013`、33 個以上は `E1017` です。
- 比較・表示・インスタンスはまだありません。ラベルに意味があるデータは名前付きの `union` で表します。

## 関連項目

- [Union](union.md)
- [Maybe](maybe.md)
- [Tuple](tuple.md)
- [match 式](../pattern-matching/match.md)
- [パターンマッチング](../pattern-matching/pattern-matching.md)
- [型](../types-and-type-inference/types.md)
- [型推論](../types-and-type-inference/type-inference.md)
- [診断](../compiler/diagnostics.md)
- [言語仕様の匿名共用体](../../../docs/language.md#匿名共用体)
- [言語リファレンスの目次](../index.md)
