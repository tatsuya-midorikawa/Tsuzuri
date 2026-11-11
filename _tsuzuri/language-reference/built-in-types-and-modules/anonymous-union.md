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
- **ラムダ**は、関数型の要素がちょうど 1 つならその型で検査します。引数の型を注釈しなくて構いません。関数型の要素が 2 つ以上あれば、ラムダを期待される型なしで検査し、得た型で決めます（下の「それ以外の式」）。
- **配列のリテラル**は、配列か固定長配列の要素がちょうど 1 つならその型で検査します。2 つ以上あれば、動的配列の要素と、長さがリテラルの要素数と同じ固定長配列の要素のうち、要素の型が合うものを数えます。合うものが 1 つならその要素です。固定長配列の要素とほかの要素の両方が合えば、既定の型を選ばず `E1041`（kind `anonymous_union_ambiguous_member`）です。`let v: ([i64; 3] | [i64]) = [1, 2, 3]` は `this array literal could be [i64] or [i64; 3] in ([i64] | [i64; 3])` で、`let a: [i64; 3] = [1, 2, 3]` のように型を注釈した束縛を渡します。`new [1, 2, 3]` と、名前に束縛した配列は動的配列です。値の中の配列のリテラルも同じです。型を注釈した位置では、リテラルの中のリテラル、タプルの要素、構築子や関数の引数のリテラルにも期待される型が届き、固定長配列になります。そのため、配列のリテラルを含む値が、リテラルを動的配列として読めばある要素に、固定長配列として読めば別の要素に合うときも、同じ `E1041` です。`([[i64; 2]; 2] | [[i64]])` に `[[1, 2], [3, 4]]` を渡すと `this array literal could be [[i64]] or [[i64; 2]; 2] in ...`、`(Maybe<[i64; 2]> | Maybe<[i64]>)` に `Some [1, 2]` を渡すと `this value, which holds an array literal, could be Maybe<[i64]> or Maybe<[i64; 2]> in ...` です。リテラルの中のリテラルとタプルの要素は長さも比べるので、`([[i64; 3]; 2] | [[i64]])` に `[[1, 2], [3, 4]]` を渡すと動的配列の要素です。それ以外の形の値（構築子や関数の引数など）は長さを比べません。
- **リストのリテラル**はリストの要素、**タプル**は同じ長さのタプルの要素がちょうど 1 つなら、その型で検査します。
- **`Dyn.of x`** は、`dyn` 型の要素がちょうど 1 つならその型で検査します。
- **それ以外の式**は、期待される型なしで検査し、得た型で決めます。合いうる要素が 1 つなら包み、1 つもなければ `E1003`（kind `anonymous_union_not_member`、`expected (i64 | string), found bool; bool is not a member of the anonymous union`。help が要素を並べます）、2 つ以上なら `E1041`（kind `anonymous_union_ambiguous_member`。`(Maybe<i64> | Maybe<string>)` に `None` を渡したときなど）です。式の型が匿名共用体そのものなら包まず、同じ型であることを求めます。型の決まらない式（総称の関数の結果など）は、匿名共用体そのものになります。
  - まだ決まっていない型は、どの型とも合いうるものとして数えます。関数の値の結果がまだ決まっていなければ、引数がさらに続く関数型の要素とも合いえます。`((string -> i64) | (string -> i64 -> i64))` に、結果の型が決まらないラムダ `\s -> later ()` を渡すと `E1041` です。引数や結果の型を注釈すれば決まります。
  - 関数の値は、関数型を注釈した位置と同じ調整を受けます。調整するのは値そのものの型だけで、値の中の型（タプルの要素、関数の結果など）は調整しません。純粋な関数の値は、純粋でない関数型の要素になります。結果が純粋な関数型の `i64 -> pure (i64 -> i64)` と、カリー化した `i64 -> i64 -> i64` は別の型なので、`((i64 -> pure (i64 -> i64)) | (i64 -> i64 -> i64))` へはそれぞれの型の値がそれぞれの要素になります。
  - edition 2027 では、どの要素にもそのままでは合わない関数の値を、借用する引数（`ref T`）と値で受け取る引数（`T`）の違いを埋めるアダプターを通して数え直します。`def len :: string -> i64` の型は `ref string -> i64` ですが、`((string -> i64) | i64)` へ渡せます（`let f: string -> i64 = len` と同じです）。そのまま合う要素があれば、それを選びます。
  - 値の中の接尾辞のない数のリテラルは、期待される型なしで検査した時点で整数か浮動小数点に決まっています。そのため、整数のリテラルは整数の要素にだけ、浮動小数点のリテラルは浮動小数点の要素にだけ合います。`(Maybe<i64> | Maybe<f64>)` に `Some 1.5` を渡すと `Maybe<f64>` の要素です。`let c: (f64 | string) = 1 + 2` と `let m: (Maybe<f64> | string) = Some 1` は `E1003`（`anonymous_union_not_member`）です（リテラルそのものを渡す `let v: (f64 | string) = 2` は、上の規則で `f64` になります）。`1.0 + 2.0` と書くか、`let c: f64 = 1 + 2` のように型を注釈した束縛を渡します。
  - 診断は、値の中の接尾辞のない数のリテラルを、何も決めなければ取る型（`i32`・`f64`）で示し（`found Maybe<i32>`）、help の末尾で `a number literal inside the value takes no type from the union, so add a suffix to it or annotate the value` と案内します。2 つ以上の要素に合うときは、型がまだ決まっていないので型を示しません。`let t: ((i32 * i32) | (i64 * i64)) = (1, 2)` は `this value could be (i32 * i32) or (i64 * i64) in ...` です。
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
- `:?` の型は本体の型注釈と同じ規則で解決します。署名の型変数を書けます（`:? Maybe<'a> as found`）。要素でない型は `E1041`（kind `type_test_not_member`。help が要素を並べます）、匿名共用体は `E1041`（kind `type_test_union`）です。型テストは型を調整しません。要素が `pure (i64 -> i64)` なら `:? pure (i64 -> i64)` と書き、`:? (i64 -> i64)` は要素でない型です。
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
- 純粋かどうかは順序を決めません。純粋な関数の値は純粋でない関数型の値としても使えるので、純粋かどうかだけが違う 2 つの要素は、値からどちらの要素か決められません。そのため、型を書いた位置で拒否します（上の表。`([pure (i64 -> i64)] | [(i64 -> i64)])` のように、要素の中の関数型だけが違う場合も同じです）。`(pure (i64 -> i64) | (string -> i64))` や `((pure (i64 -> i64) * i64) | ((i64 -> i64) * string))` のように、ほかの部分でも違えば書け、その違いで並べます。
- 結果が純粋な関数型のとき、引数の列はそこで終わります。`(i64 -> pure (i64 -> i64))` は引数 1 個で結果が関数型、`(i64 -> i64 -> i64)` は引数 2 個の関数型なので、両方を要素にできます。
- 純粋な関数型どうしも、カリー化を平らにして比べます。`pure (i64 -> (i64 -> i64))` と `pure (i64 -> i64 -> i64)` は別の型ですが、診断はどちらも `pure (i64 -> i64 -> i64)` と表示するので、片方を名指しできません。この 2 つを要素にすることも `E1041`（kind `anonymous_union_indistinct_members`、`... differ only in how a pure function type is curried, ...`）です。

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
| 比較・表示・ハッシュ | ありません。`==`・`<`・`$"{v}"`・`Hash.hash` は `E1005`、匿名共用体を持つ型の `deriving (Eq, ...)` は `E1025` です。メッセージは `no instance for Eq<(i64 \| string)>; instances cannot be declared for an anonymous union; match on its members, wrap it in a record, or declare a union with named cases` で、インスタンスの定義は勧めません |
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
