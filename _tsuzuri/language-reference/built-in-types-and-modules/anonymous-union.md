# 匿名共用体

匿名共用体は、要素の型そのものがケースを表す共用体です。`union` を宣言せずに `(i64 | string)` と書き、「`i64` か `string` のどちらか」を表します。期待される型が匿名共用体の位置では、要素の型の値をそのまま書けます。取り出すときは型テストのパターン `:? i64 as n` を使い、`match` が網羅性を検査します。F# の提案 FS-1092（Anonymous Type-tagged Unions）に当たる機能です。

## この記事のポイント

- 型は括弧で囲んで `(i64 | string)` と書きます。要素は 2〜32 個です。
- 要素の順序と入れ子は意味を持ちません。`(string | i64)` と `(i64 | string)` は同じ型で、`(i64 | (string | bool))` は `(i64 | string | bool)` です。
- 期待される型が匿名共用体の位置では、要素の型の値がそのまま匿名共用体の値になります（注入）。構築用の関数はありません。
- 取り出しは型テストのパターン `:? T`・`:? T as x` です。`match` の網羅性を検査し、足りなければ `E1021`（`missing: :? bool`）です。
- 型変数・区別できない要素・重複した要素は `E1041`、借用を含む要素は `E1013`、33 個以上の要素は `E1017` です。
- 要素がすべて `Eq`・`Ord`・`Hash`・`Display` を持てば、匿名共用体も同じクラスを持ちます（タプルと同じ、組み込みのインスタンスです）。`==`・`<`・`$"{v}"` とリテラルのパターン（`| 1 ->`）を書け、`HashMap`・`HashSet`・`Map`・`Set` のキーにできます。
- JSON では、`Encode` がペイロードの JSON だけを書きます（タグを書きません）。`Decode` は、JSON の値の種類で要素が 1 つに決まる匿名共用体だけが持ちます（`(i64 | string)` は持ち、`(i64 | f64)` は `E1025`）。
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
| 比較演算子のもう一方の被演算子 | `v == 1`、`v < "a"`（`1` と `"a"` を包んでから比べる。[比較・順序・ハッシュ・表示](#比較順序ハッシュ表示)） |

式の種類ごとに、要素の選び方が決まっています。

- **接尾辞のない整数のリテラル**は、整数の要素を選びます。整数の要素がなければ `bigint` の要素、それもなければ浮動小数点の要素です。同じ組に 2 つ以上あれば、既定の型を選ばず `E1041`（kind `anonymous_union_ambiguous_literal`）です。`let v: (i32 | i64 | string) = 16` は `the literal 16 could be i32 or i64 in (i32 | i64 | string)` で、help が `16i32` のように接尾辞を示します。`let v: (f64 | string) = 2` は `f64` の `2.0` です。
- **接尾辞のない浮動小数点のリテラル**は、浮動小数点の要素を選びます。2 つ以上あれば同じ `E1041` です。
- **接尾辞のあるリテラル**は、その型の要素になります。範囲の検査も変わりません（`let v: (i8u | string) = -1` は `E1009`）。
- **ラムダ**は、関数型の要素がちょうど 1 つならその型で検査します。引数の型を注釈しなくて構いません。関数型の要素が 2 つ以上あれば、ラムダを期待される型なしで検査し、得た型で決めます（下の「それ以外の式」）。
- **配列のリテラル**は、配列か固定長配列の要素がちょうど 1 つならその型で検査します。2 つ以上あれば、動的配列の要素と、長さがリテラルの要素数と同じ固定長配列の要素のうち、要素の型が合うものを数えます。合うものが 1 つならその要素です。固定長配列の要素とほかの要素の両方が合えば、既定の型を選ばず `E1041`（kind `anonymous_union_ambiguous_member`）です。`let v: ([i64; 3] | [i64]) = [1, 2, 3]` は `this array literal could be [i64] or [i64; 3] in ([i64] | [i64; 3])` で、`let a: [i64; 3] = [1, 2, 3]` のように型を注釈した束縛を渡します。`new [1, 2, 3]` と、名前に束縛した配列は動的配列です。
- **リストのリテラル**はリストの要素、**タプル**は同じ長さのタプルの要素がちょうど 1 つなら、その型で検査します。
- **`Dyn.of x`** は、`dyn` 型の要素がちょうど 1 つならその型で検査します。
- **それ以外の式**は、期待される型なしで検査し、得た型で決めます。合いうる要素が 1 つなら包み、1 つもなければ `E1003`（kind `anonymous_union_not_member`、`expected (i64 | string), found bool; bool is not a member of the anonymous union`。help が要素を並べます）、2 つ以上なら `E1041`（kind `anonymous_union_ambiguous_member`。`(Maybe<i64> | Maybe<string>)` に `None` を渡したときなど）です。式の型が匿名共用体そのものなら包まず、同じ型であることを求めます。型の決まらない式（総称の関数の結果など）は、匿名共用体そのものになります。
  - まだ決まっていない型は、どの型とも合いうるものとして数えます。関数の値の結果がまだ決まっていなければ、引数がさらに続く関数型の要素とも合いえます。`((string -> i64) | (string -> i64 -> i64))` に、結果の型が決まらないラムダ `\s -> later ()` を渡すと `E1041` です。引数や結果の型を注釈すれば決まります。
  - 診断は、値の中の接尾辞のない数のリテラルを、何も決めなければ取る型（`i32`・`f64`）で示します（`found Maybe<i32>`）。
- **参照 `ref T` の値**は、参照先の `T` として要素を選びます。Copy の値は複製し、非 Copy の値は move できないので `E1012` です。

次は注入しません。どれも既存の `E1003` などのままです。

- 入れ物や関数の中: `[i64]` を `[(i64 | string)]` へ、`i64 -> unit` を `(i64 | string) -> unit` へは変えません。
- 参照の型: `ref i64` を `ref (i64 | string)` へは変えません（表現が違います）。
- 期待される型のない位置: `let xs = [1, "a"]` は `E1003` です。
- 型変数の推論: `'a` を要素の型から匿名共用体へ広げません。

期待される型は、次の 2 つの形でも決まります。どちらも、型を注釈すれば書き方に関係なく決まります。

- **並んだ式は、前の式の型を次の式の期待される型にします**（配列・リストのリテラルの要素、`if` の `then` と `else`、`match` の腕）。そのため、期待される型のない位置では、匿名共用体の値が先にあるときだけ後ろの式を包みます。`let a: (i64 | string) = 1` のとき、`[a, 2]` と `if c then a else 2` は `2` を包みますが、`[3, a]` と `if c then 3 else a` は `E1003` です。`let xs: [(i64 | string)] = [3, a]` のように型を注釈すれば、順序に関係なく書けます。
- **共用体の構築子は、匿名共用体を期待される型として受け取りません**（上の「それ以外の式」です）。構築子の引数は期待される型なしで検査するので、`let v: (Maybe<[|i64|]> | string) = Some [||]` は `E1004`（空のリストの型が決まらない）、`let v: (Maybe<(i64 | string)> | i64) = Some 4` は `E1003`（`found Maybe<i32>`）です。`let m: Maybe<[|i64|]> = Some [||]` のように要素の型を注釈した束縛を先に作り、それを渡します。

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

値そのものと比べるリテラルのパターン（`| 1 ->`、`| "x" ->`）も、同じ `match` に書けます（[リテラルのパターン](#リテラルのパターン)）。

網羅性は要素ごとのケースとして数えます。すべての要素をガードなしでテストすれば網羅的です。足りなければ `E1021` で、足りない要素を `:? T` で示し、`| :? bool -> unreachable ()` を足す修正案（`maybe`）を持ちます。ペイロードの中では `missing: Some (:? string)` のように示します。同じ要素の 2 回目のテストは `W1003`（到達不能）です。

## 要素の規則

| 書いたもの | 結果 |
| --- | --- |
| `('a \| i64)`（型変数そのもの、または頭が型変数の型） | `E1041`（kind `anonymous_union_type_variable`）。`type OrInt<'a> = ('a \| i64)` も同じ |
| `(Maybe<'a> \| Maybe<i64>)`（型引数によっては同じ型） | `E1041`（kind `anonymous_union_indistinct_members`） |
| `((i64 -> 'r) \| (i64 -> i64 -> i64))`（`'r` が `i64 -> i64` なら同じ型） | 同じ `E1041`。`i64 -> 'r and i64 -> i64 -> i64 may be the same type for some type arguments, ...` |
| `((i64 -> i64) \| pure (i64 -> i64))`（純粋かどうかだけが違う） | 同じ `E1041`。`i64 -> i64 and pure (i64 -> i64) differ only in the purity of a function type, and purity does not pick a member, ...` |
| `type Meters = f64` のときの `(Meters \| f64)` | `E1041`（kind `anonymous_union_duplicate_member`。`f64 appears more than once in the anonymous union (f64 \| f64)`） |
| `(ref string \| i64)` | `E1013`（kind `anonymous_union_borrowed_member`、`an anonymous union holds owned values; ref string is borrowed`） |
| 平坦化した後の要素が 33 個以上 | `E1017`（kind `anonymous_union_member_limit`） |

`(Maybe<'a> | i64)` は書けます。頭の `Maybe` と `i64` で区別でき、型引数を置き換えても順序と区別は変わりません。`(Maybe<i64> | Maybe<string>)` も書けます。

関数型の要素は、次の規則で比べます（引数の規約は比べません）。

- 関数型は、カリー化を平らにして比べます。純粋でない `i64 -> (i64 -> i64)` と `i64 -> i64 -> i64` は同じ型なので、両方を要素にすると要素の重複（`E1041`）です。
- 引数を先頭から順に比べ、そこで決まらなければ引数の少ないほうが先、引数の数も同じなら結果で比べます。`((string -> i64) | (i64 -> i64 -> i64))` は `((i64 -> i64 -> i64) | (string -> i64))` と表示します（先頭の引数 `i64` が `string` より先です）。
- 関数型の結果が型変数のときは、型引数が関数型になると引数が増えます。そのため、そこまでの引数が同じで、より引数の多い要素とは区別できません（上の表の `(i64 -> 'r)` と `(i64 -> i64 -> i64)`）。先頭の引数で区別できる `((i64 -> 'r) | (string -> i64))` は書け、`'r` が何であっても順序は変わりません。
- 純粋な関数型は純粋でない関数型の後に並べます。ただし、純粋な関数の値は純粋でない関数型の値としても使えるので、純粋かどうかだけが違う 2 つの要素は、値からどちらの要素か決められません。そのため、型を書いた位置で拒否します（上の表）。`(pure (i64 -> i64) | (string -> i64))` のように、ほかの部分でも違えば書けます。
- 純粋な関数型どうしも、カリー化を平らにして比べます。`pure (i64 -> (i64 -> i64))` と `pure (i64 -> i64 -> i64)` は別の型ですが、診断はどちらも `pure (i64 -> i64 -> i64)` と表示するので、片方を名指しできません。この 2 つを要素にすることも `E1041`（kind `anonymous_union_indistinct_members`、`... differ only in how a pure function type is curried, ...`）です。

## 比較・順序・ハッシュ・表示

匿名共用体は、要素がすべてそのクラスを持てば、`Eq`・`Ord`・`Hash`・`Display` を持ちます。タプルと同じく、要素のインスタンスから決まる組み込みのインスタンスです。匿名共用体には宣言がないので、`deriving` も `instance` も書きません。

| クラス | 規則 |
| --- | --- |
| `Eq`（`==`・`!=`） | 2 つの値が同じ要素で、ペイロードが要素の `==` で等しいとき、等しい値です。別の要素なら、ペイロードを比べずに「等しくない」です |
| `Ord`（`<`・`<=`・`>`・`>=`） | まず要素を正準順序（表示の順）で比べます。同じ要素なら、ペイロードを要素の同じ演算子で比べます |
| `Hash` | 名前付きの共用体の `deriving (Hash)` と同じ列です。タグ `0x55`、正準順序での要素の位置（0 から数える）、ペイロードのハッシュの順に混ぜます。`==` で等しい値は、ハッシュも等しくなります |
| `Display` | ペイロードの表示だけです。要素の型名は書きません。入れ物の中の文字列には、ほかの文字列と同じく引用符が付きます |

### 比較と順序

```tsuzuri run=true%20false%20true%20true%20false%20true%20true%20true
let a: (i64 | string) = 1
let b: (string | i64) = "one"
let c: (i64 | string) = 1
$"{a == c} {a == b} {a != b} {a < b} {b <= a} {a == 1} {b == "one"} {b > 100}"
```

実行結果:

```text
true false true true false true true true
```

- **要素の順は、書いた順ではなく正準順序です。** `i64` は `string` より前なので、`i64` の値はどの `string` の値よりも小さくなります。`b` の型を `(string | i64)` と書いても、`a < b` は `true` です。`b > 100` も、ペイロードを比べずに要素の順で決まります。
- **比較演算子のもう一方の被演算子は、匿名共用体へ注入されます。** `a == 1` は、`1` を `i64` の要素として包んでから比べます。左辺が接尾辞のない数のリテラルの `1 == a` も同じです。左辺がそれ以外の式なら、左辺の型が右辺の期待される型になります。`"one" == b` は `E1003`（`expected string, found (i64 | string)`）なので、匿名共用体を左辺に書きます。
- **注入した被演算子は move されます。** 注入は値の使用なので（[所有権](#所有権)）、比較の被演算子にした非 Copy の変数（`string` など）は匿名共用体の中へ move され、後では使えません（`E1012`）。ほかの型どうしの比較は被演算子を借用するだけなので、ここが違います。`ref s` は注入できません（`E1012`）。後でも使う値は、先に使うか、複製を比べる（`a == clone_string ref s`）か、型テストで取り出してから比べます（`t == s` は `s` を借用するだけです）。`E1012` の help もこの方法を案内します。
- **浮動小数点の要素は、要素の比較のままです。** `(f64 | string)` の NaN は自分自身と等しくありません（`==` は `false`、`!=` は `true`、`<` は `false`）。`0.0` と `-0.0` は等しい値です。
- **クラスを持たない要素があれば、その要素を示す `E1005` です。** `bool` は `Ord` を持たないので、`(i64 | bool)` は `==` を使えますが、`<` は `no instance for Ord<bool>` です。`record Point { x: i64, y: i64 }` に `Eq` がなければ、`(i64 | Point)` の `==` は `no instance for Eq<Main.Point>` です。

注入した被演算子を後でも使う例です。複製を比べるか、型テストで取り出してから比べます。

```tsuzuri run=true%20true%20s1
let a: (i64 | string) = "s1"
let s = "s1"
let copied = a == clone_string ref s
let tested = match a with
    | :? string as t -> t == s
    | _ -> false
$"{copied} {tested} {s}"
```

実行結果:

```text
true true s1
```

### リテラルのパターン

リテラルのパターンは `==` での比較です。匿名共用体が `Eq` を持てば、`match` の腕にリテラルを書けます。リテラルは、比較演算子の被演算子と同じ規則で匿名共用体へ注入してから比べます。

```tsuzuri run=one%20ex%20other%20other%20other
def name :: own (i64 | f64 | string) -> string = \value ->
    match value with
    | 1 -> "one"
    | "x" -> "ex"
    | _ -> "other"

$"{name 1} {name "x"} {name 2} {name 1.0} {name "1"}"
```

実行結果:

```text
one ex other other other
```

- **別の要素の値は、どのリテラルにも一致しません。** `1` は整数の要素 `i64` として包まれるので、`f64` の `1.0` にも、文字列の `"1"` にも一致しません。
- タプルやレコードの中（`| (1, "s") ->`、`| Row { key: 0, n: _ } ->`）、OR パターン（`| 7 | 8 | "y" ->`）、関数ガードにも書けます。型テスト `:? T as x` と同じ `match` に混ぜられます。
- 注入の規則がそのまま働きます。2 つ以上の要素になりうるリテラルは `E1041`（`(i32 | i64)` への `| 1 ->`。`| 1i64 ->` と接尾辞を付けます）、どの要素の型でもないリテラルは `E1003` です。
- 比べるのは匿名共用体の `Eq` なので、すべての要素に `Eq` が要ります。`Eq` を持たない要素があれば、その要素を示す `E1005` です（リテラルのない要素でも同じです）。その場合は、型テストで取り出してから比べます。
- 浮動小数点の要素は、要素の `==` のままです。NaN はどのリテラルにも一致せず、`-0.0` は `0.0` に一致します。
- リテラルのパターンは値を 1 つしか覆わないので、`_` の腕が要ります。`(i64 | bool)` に `true`・`false`・`:? i64` を並べても、`E1021`（`missing: _`）です。

### 表示

単独の値と文字列補間は、要素の `Display` をそのまま使います（文字列に引用符は付きません）。配列・リスト・タプルと、`deriving (Display)` のレコード・共用体の中では、文字列の要素に引用符が付きます。

```tsuzuri run=1%20one%20%5B2%2C%20%22two%22%5D%20%281%2C%20%22one%22%29
let a: (i64 | string) = 1
let b: (i64 | string) = "one"
let items: [(i64 | string)] = [2, "two"]
let alone = $"{a} {b}"
alone + " " + Display.display items + " " + Display.display (a, b)
```

実行結果:

```text
1 one [2, "two"] (1, "one")
```

`(i64 | string)` は Copy ではないので、タプル `(a, b)` を作ると `a` と `b` は move されます。後でも使う値は、タプルにする前に使うか、複製して渡します。

`Display` を持つので、プログラムの結果式（[トップレベル式による形式](../organizing-tsuzuri/modules.md#トップレベル式による形式)の最後の式）が匿名共用体なら、その表示を出力します。`def pick :: i64 -> (i64 | string) = \n -> if n > 0 then n else "none"` の後に `pick 0` と書いたプログラムは `none` を出力します（`Display` を持つ型の値を表示する、結果式の規則のとおりです）。

### コレクションのキーと `deriving`

`HashMap`・`HashSet` のキー（`Eq` と `Hash`）と、`Map`・`Set` のキー（`Ord`）に使えます。`Map` と `Set` は `Ord` の順に並ぶので、`i64` のキーがすべて `string` のキーより前に来ます。

```tsuzuri run=2%20%5B1%5D%5B2%5D%5Ba%5D%5Bb%5D
def key :: own (i64 | string) -> (i64 | string) = \value -> value

let seen: HashSet<(i64 | string)> = HashSet.empty() |> HashSet.insert (key 1) |> HashSet.insert (key "one") |> HashSet.insert (key 1)
let ordered: Set<(i64 | string)> = Set.empty() |> Set.insert (key "b") |> Set.insert (key 2) |> Set.insert (key "a") |> Set.insert (key 1)
let mut text = ""
for item in Set.iter ordered do
    text = text + $"[{item}]"
$"{HashSet.length seen} {text}"
```

実行結果:

```text
2 [1][2][a][b]
```

型引数の推論は、要素の型を匿名共用体へ広げません（[値の作り方](#値の作り方注入)）。`Set.empty() |> Set.insert "b"` と書くと、キーの型は `string` に決まります。キーは、上の `key` のような関数か型注釈のある束縛で、匿名共用体の値にしてから渡します。

匿名共用体をフィールドやペイロードに持つ型には、`deriving (Eq, Ord, Hash, Display)` を付けられます。

```tsuzuri run=false%20Setting%20%7B%20name%3A%20%22depth%22%2C%20value%3A%203%20%7D%20Setting%20%7B%20name%3A%20%22depth%22%2C%20value%3A%20%223%22%20%7D
record Setting { name: string, value: (i64 | string | bool) } deriving (Eq, Display)

let a = Setting { name: "depth", value: 3 }
let b = Setting { name: "depth", value: "3" }
$"{a == b} {a} {b}"
```

実行結果:

```text
false Setting { name: "depth", value: 3 } Setting { name: "depth", value: "3" }
```

ハッシュは、同じ要素を同じ順（正準順序）に並べた名前付きの共用体と同じ値です。

```tsuzuri run=true
union Named = Number of i64 | Text of string deriving (Hash)

let a: (i64 | string) = "one"
Hash.hash a == Hash.hash (Text "one")
```

### 持たないクラスとそのほかの規則

- `Encode`・`Decode` は [JSON（Encode と Decode）](#jsonencode-と-decode) の規則で持ちます。メソッドを持つほかの組み込みクラス（`Default`・`Format`・`Arbitrary`、`Add` などの演算子のクラス）のインスタンスはありません。プログラムが宣言したクラスも、匿名共用体のためのインスタンスは書けません（すべての型に当てはまる `instance Size<'a>` だけは、匿名共用体にも当てはまります）。メソッドのないクラス `Copy`・`Send`・`Sync`・`Capture` は、要素から決まります（[型の性質](#型の性質)。`Copy<'a>` を要る総称の関数に `(i64 | f64)` を渡せます）。`Default` と `Arbitrary` は、どの要素の値を作るかを決められないので持ちません。`Default.default()` や書式指定の補間 `$"{v:x}"` は `E1005`、匿名共用体を持つ型の `deriving (Default)` は `E1025` です。メッセージは `no instance for Default<(i64 | string)>; instances cannot be declared for an anonymous union; match on its members, wrap it in a record, or declare a union with named cases` で、インスタンスの定義は勧めません。幅と寄せだけの補間 `$"{v:>5}"` は `Display` を使うので書けます。
- 匿名共用体を head とする `instance` は書けません（`E1041`）。比較や表示の規則を自分で決めたいときは、レコードで包むか、名前付きの `union` を宣言します。
- `Dyn.of v` で `dyn Display`・`dyn Encode` などに格納できます（タプルと同じです）。表示はペイロードだけです。
- `const def` の中では、タプルや名前付きの共用体と同じく比較演算子を使えません（`E1026`）。リテラルのパターンも同じ比較なので、同じ `E1026`（`an operator on a record, tuple, array, or union is not allowed in a const def; ...`）がパターンの位置に出ます。
- インスタンスは要素のインスタンスを呼ぶ関数です。`pure def` の中で匿名共用体を比べたり表示したりするには、要素のインスタンスも純粋でなければなりません（`E1029`）。
- **インスタンスの解決には上限があります。** 1 つの制約（`a < b` の `Ord` など）から導かれる要件は 128 個までで、超えると `E1017`（kind `instance_resolution_limit`）です。匿名共用体の要素そのものの要件は数えません（要素は 32 個までなので）。要素の中の型（タプルや配列の要素、導出したレコードのフィールドの型）は、異なる型ごと・クラスごとに 1 個で、`Ord` は `Eq` も要るので 2 個です。たとえば、互いに異なる 64 個のレコードを 2 個ずつタプルにした 32 要素の匿名共用体は、`==` を使えますが、`<` は `E1017` です。型の種類を減らすか、要素の一部を、手書きのインスタンスを持つレコードで包みます（文脈のない手書きのインスタンスは、フィールドの型を辿らないので、クラスごとに 1 個です）。数えた要件の半分より多くが匿名共用体の要素の中から来たときは、`E1017` の help がこの数え方と対処を言います（深さ 64 の上限や、要素の外の型で超えたときは、help はありません）。この上限はタプルやレコードにも同じに働きます。
- **型変数を持つ匿名共用体の制約は、要素がすべて満たせるときだけ、書いた位置で要素の制約に分けます。** 総称の関数の中の `Eq<(['a] | string)>` は `Eq<'a>` になり、署名の `Eq<'a> =>` やインスタンスの文脈が満たします。要素の制約のどれかが満たせない（`(['a] | (i64 -> i64))` の `Eq`、`(['a] | bool)` の `Ord`）か、`Decode` で要素の JSON の種類が重なる・決まらないとき（[Decode](#decode)）は、制約を匿名共用体のまま残し、型変数が決まる位置（呼び出し）で調べます。そうした総称の関数は、呼び出さなければ受理され、呼び出すと、呼び出しの位置で `E1005`（`no instance for Eq<i64 -> i64>`）や `E1025` です。型変数を持つ制約を、満たすインスタンスがないあいだ具体化まで残すのは、ほかの型の制約と同じ規則です。タプルは、型変数のない部分が満たせなければ定義の位置で `E1005` なので、ここはタプルと違います。

## JSON（Encode と Decode）

匿名共用体は、[Json](json.md) の型クラス `Encode`・`Decode` の組み込みのインスタンスも持ちます。`deriving (Encode, Decode)` のあるレコードや共用体のフィールド・ペイロードにも使えます。

### Encode

`Encode` は、ペイロードの JSON をそのまま書きます。要素を示すタグや包みは書きません（TypeScript の `number | string` を JSON にしたときと同じ見え方です）。要素がすべて `Encode` を持てば、匿名共用体も持ちます。結果は要素の `Encode` の結果そのもので、誤りも要素のものです（`f64` の NaN は `NonFinite`）。

```tsuzuri run=[1,"two",[3,4]]
let items: [(i64 | string | [i64])] = [1, "two", [3, 4]]
Json.serialize items |> Result.default_value u8"error"
```

### Decode

タグがないので、`Decode` は JSON の値の種類（真偽値・数・文字列・配列・object）で要素を選びます。そのため、**要素が読む JSON の種類が互いに重ならない匿名共用体だけ**が `Decode` を持ちます。重なるときに「最初の要素」のような既定は選びません。

| 要素の型 | 読む JSON の種類 |
| --- | --- |
| `bool` | 真偽値（boolean） |
| 整数（`i8`〜`i128`、`i8u`〜`i128u`）、`f16`〜`f128`、`d32`〜`d128` | 数（number） |
| `string`、`utf8string` | 文字列（string） |
| `[T]`、`[\|T\|]`、`Vec<T>`、2〜4 要素のタプル、`Set<T>`、`HashSet<T>` | 配列（array） |
| `Map<K, V>`、`HashMap<K, V>` | object と配列の両方（キーが JSON の文字列にならないマップは `[キー, 値]` の配列の配列になるため） |
| `deriving (Decode)` のあるレコード | object |
| `Maybe<T>`、名前付きの共用体、`Json.Value`、手書きの `Decode` を持つ型 | 決まりません（どの要素とも重なります） |

- 種類は要素の型の頭（`[T]` なら配列）だけで決まり、型引数では変わりません。総称の関数の中の `(['a] | string)` も `Decode<'a>` があれば読めます。
- 型変数を持つ要素も、種類が型変数で変わらなければ、匿名共用体を書いた位置で決まります。`record Pair<'a> { x: 'a } deriving (Decode)` の `(Pair<'a> | i64)` は、`Pair<'a>` がどの `'a` でも object を読むので、総称の関数の中でも `Decode<'a>` があれば読めます。`Pair<'a>` に、すべての `'a` についての `Decode` のインスタンスがないときと、書いた位置で種類が重なるか決まらないとき（`(['a] | Vec<'a>)`、`(Maybe<'a> | i64)`）は、制約を匿名共用体のまま残し、型変数が決まる位置（呼び出し）で調べます（[持たないクラスとそのほかの規則](#持たないクラスとそのほかの規則)）。`instance Decode<Pair<i64>>` が手書きされていても、`Pair<i64>` は種類が決まらないので、`(Pair<i64> | i64)` で呼び出す位置で `E1025` です。インスタンスがなければ、同じ位置で `E1005`（`no instance for Decode<Main.Pair<i64>>`）です。診断は、総称の関数の本体や std の中ではなく、型を決めた呼び出しを指します。
- `Maybe<T>` は `null` に加えて `T` の種類を、導出した名前付きの共用体は文字列と object を、`Json.Value` はすべての種類を読みます。手書きの `Decode` が何を読むかは、コンパイラには分かりません。これらを要素に持つ匿名共用体は `Decode` を持ちません。
- `Decode` のインスタンスがない要素（`char`、固定長配列、5 要素以上のタプルなど）は、要素の型を言う `E1005`（`no instance for Decode<char>`）です。

読むときは、JSON の値の種類に当たる要素の `Decode` を呼び、その結果を匿名共用体の値にします。選ばれた要素が値を受理しなければ、その要素の誤りをそのまま返します（`(i64 | string)` に `1.5` を読ませると、`i64` の `json: expected integer`）。どの要素の種類でもなければ、`Json.ExpectedType` の誤りです。メッセージは要素の種類を正準順序で `or` でつなぎます（`offset` は `-1`）。

```tsuzuri run=number%207%20|%20text%20seven%20|%20json:%20expected%20number%20or%20string%20|%20json:%20expected%20integer
def describe :: ref utf8string -> string = \text ->
    let decoded: Result<(i64 | string), Json.Error> = Json.deserialize text
    match decoded with
    | Ok value ->
        match value with
        | :? i64 as n -> $"number {n}"
        | :? string as s -> $"text {s}"
    | Error error -> Display.display error

let number = describe u8"7"
let text = describe u8"\"seven\""
let flag = describe u8"true"
let fraction = describe u8"1.5"
$"{number} | {text} | {flag} | {fraction}"
```

要素を選ぶ種類は、要素の `Decode` のインスタンスだけから決まります。`Encode` が何を書くかは調べません。`Json` のインスタンスと、`Encode`・`Decode` の両方を導出した型は、書く種類と読む種類が同じなので、書いた JSON は書いた要素として読み戻されます。`Decode` を導出して `Encode` を手書きしたレコードが object 以外を書くと、その JSON は、その種類を読む別の要素の値になるか、どの要素にも読まれません（そのレコードは、単独でも書いた JSON を読み戻せません）。

```text
record Meters { value: i64 } deriving (Decode)
instance Encode<Meters> {
    fn encode meters = Encode.encode (ref meters.value)    // object ではなく、数を書く
}

let length: (Meters | i64) = Meters { value: 1 }
// Json.serialize は 1 を書き、それを (Meters | i64) として読むと i64 の 1 になります
```

種類が重なる匿名共用体の `Decode` は `E1025`（kind `anonymous_union_decode_overlap`）です。メッセージが、重なる 2 つの要素か、種類が決まらない要素を示します。

```text
let a: Result<(i64 | f64), Json.Error> = Json.deserialize u8"1"
// E1025: no Decode instance for (i64 | f64): the members i64 and f64 both read a JSON number,
//        so the JSON value does not select one member; decode Json.Value and match on it,
//        or use a union with named cases

let b: Result<(Maybe<i64> | string), Json.Error> = Json.deserialize u8"1"
// E1025: no Decode instance for (string | Maybe<i64>): the member Maybe<i64> is not limited to
//        one kind of JSON value (a Maybe, a union with named cases, Json.Value, or a type with
//        a hand-written Decode can read several), so the JSON value does not select one member; ...
```

`(Map<i64, string> | [i64])` も、どちらも配列を読むので `E1025` です。`Encode` にはこの制限がなく、`(i64 | f64)` や `(Maybe<i64> | string)` も JSON に書けます（読み戻すには、`Json.Value` に decode して `match` で分けるか、名前付きの `union` を使います）。

### 導出した型のフィールド

`deriving (Encode, Decode)` のあるレコードと共用体は、匿名共用体のフィールドとペイロードを持てます。種類が重ならなければ、書いた JSON を同じ値に読み戻せます。

```tsuzuri run={"name":"depth","value":3}%20true
record Setting { name: string, value: (i64 | string | bool) } deriving (Eq, Encode, Decode)

let setting = Setting { name: "depth", value: 3 }
let text = Json.serialize (ref setting) |> Result.get
let back: Result<Setting, Json.Error> = Json.deserialize (ref text)
let same = match back with
    | Ok copy -> copy == setting
    | Error _ -> false
let shown = String.from_utf8 (ref text)
$"{shown} {same}"
```

- 導出したレコードは、JSON に無いキーを `null` として decode します。匿名共用体は `null` を読まないので、匿名共用体のフィールドのキーが無ければ `MissingField` です。省略できるフィールドは `Maybe<(i64 | string)>` にします。
- フィールドの匿名共用体の種類が重なると、`deriving (Decode)` の位置で同じ `E1025`（`anonymous_union_decode_overlap`）です。
- 値そのものが往復するかどうかは、要素の `Encode`・`Decode` の性質のままです（[Json](json.md#encode-と-decode)）。

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
| 比較・順序・ハッシュ・表示 | 要素がすべてそのクラスを持てば、`Eq`・`Ord`・`Hash`・`Display` を持ちます（[比較・順序・ハッシュ・表示](#比較順序ハッシュ表示)。要素の中の型が多いと、インスタンスの解決の上限で `E1017`）。`Default`・`Format`・`Arbitrary` はありません（`E1005`。`deriving` では `E1025`） |
| JSON | 要素がすべて持てば `Encode` を持ちます。`Decode` は、要素が読む JSON の種類が重ならないときだけ持ちます（[JSON（Encode と Decode）](#jsonencode-と-decode)。重なれば `E1025`） |
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
- 比較や表示の規則を自分で決めたい、または `Default` のインスタンスが要る（匿名共用体のインスタンスは組み込みのものだけで、`instance` も `deriving` も書けません）。
- 同じ種類の JSON を読む要素（`i64` と `f64`、2 つのレコード）を JSON から読み分けたい（匿名共用体の JSON にはタグがないので、`Decode` を持てません。名前付きの `union` の導出は `{"Case": payload}` と書きます）。

## 所有権

注入は `Some x` と同じ値の使用です。Copy の値は複製し、非 Copy の値は共用体の中へ move します。注入する式は 1 回だけ評価し、左から右の評価順も変わりません。

```text
let name = "abc"
let v: (i64 | string) = name
name.length          // E1012: use of moved or partially moved value 'name'
```

比較演算子の被演算子（`v == name`）への注入も同じ move です。後で使うときの `E1012` の help は、`ref name` ではなく、先に使うか複製（`clone_string ref name`）を注入するよう案内します（[比較と順序](#比較と順序)）。

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

- `Ord` の順は、書いた順ではなく正準順序（表示の順）です。`(string | i64)` と書いても、`i64` の値が `string` の値より前です。
- コレクションのキーや総称の関数の引数は、匿名共用体の値にしてから渡します（型引数の推論は要素の型を匿名共用体へ広げません）。
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
- 要素がすべて持てば、`Eq`・`Ord`・`Hash`・`Display` を持ちます。順序は正準順序で要素を比べ、表示はペイロードだけです。リテラルのパターンは `==` で比べます。
- `Encode` はペイロードの JSON だけを書き、`Decode` は JSON の値の種類で要素を選びます。種類が重なる匿名共用体の `Decode` は `E1025` です。
- `instance` は書けません。ラベルに意味があるデータは名前付きの `union` で表します。

## 関連項目

- [Union](union.md)
- [Maybe](maybe.md)
- [Tuple](tuple.md)
- [match 式](../pattern-matching/match.md)
- [パターンマッチング](../pattern-matching/pattern-matching.md)
- [型](../types-and-type-inference/types.md)
- [型推論](../types-and-type-inference/type-inference.md)
- [型クラス](../types-and-type-inference/type-classes.md)
- [HashMap](hashmap.md)
- [Map](map.md)
- [Json](json.md)
- [診断](../compiler/diagnostics.md)
- [言語仕様の匿名共用体](../../../docs/language.md#匿名共用体)
- [言語リファレンスの目次](../index.md)
