# 匿名共用体

匿名共用体は、要素の型そのものがケースを表す共用体です。`union` を宣言せずに `(i64 | string)` と書き、「`i64` か `string` のどちらか」を表します。期待される型が匿名共用体の位置では、要素の型の値をそのまま書けます。取り出すときは型テストのパターン `:? i64 as n` を使い、`match` が網羅性を検査します。要素の少ない匿名共用体の値は、その要素をすべて持つ匿名共用体の位置へそのまま渡せ、`:? (i64 | f64) as x` で一部の要素へ絞り込めます。F# の提案 FS-1092（Anonymous Type-tagged Unions）に当たる機能です。

## この記事のポイント

- 型は括弧で囲んで `(i64 | string)` と書きます。要素は 2〜32 個です。
- 要素の順序と入れ子は意味を持ちません。`(string | i64)` と `(i64 | string)` は同じ型で、`(i64 | (string | bool))` は `(i64 | string | bool)` です。
- 期待される型が匿名共用体の位置では、要素の型の値がそのまま匿名共用体の値になります（注入）。構築用の関数はありません。
- 取り出しは型テストのパターン `:? T`・`:? T as x` です。`match` の網羅性を検査し、足りなければ `E1021`（`missing: :? bool`）です。
- 要素の少ない匿名共用体の値は、その要素をすべて持つ匿名共用体が期待される位置へそのまま渡せます（拡大。`(i64 | string)` を `(f64 | i64 | string)` へ）。逆向きは自動では行わず、`:? (i64 | f64) as x` で絞り込みます。
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
- **それ以外の式**は、期待される型なしで検査し、得た型で決めます。合いうる要素が 1 つなら包み、1 つもなければ `E1003`（kind `anonymous_union_not_member`、`expected (i64 | string), found bool; bool is not a member of the anonymous union`。help が要素を並べます）、2 つ以上なら `E1041`（kind `anonymous_union_ambiguous_member`。`(Maybe<i64> | Maybe<string>)` に `None` を渡したときなど）です。式の型が匿名共用体そのものなら包みません。要素の数が期待される型より少なければ拡大し（[共用体どうしの受け渡し（拡大）](#共用体どうしの受け渡し拡大)）、同じ数なら同じ型であることを求めます。型の決まらない式（総称の関数の結果など）は、匿名共用体そのものになります。
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

## 共用体どうしの受け渡し（拡大）

期待される型が匿名共用体 `U` の位置で、式の型が別の匿名共用体 `V` のとき、`V` の要素がすべて `U` の要素なら、`V` の値がそのまま `U` の値になります（拡大）。値は変えず、同じ型の要素のタグへ付け替えるだけです。位置は注入と同じで、型注釈のある `let`、関数の結果、引数とパイプ、フィールド、要素、`if`・`match`・ブロックの腕、代入です。

```tsuzuri run=12px%20auto%2016.5pt
def parse_size :: string -> (i64 | string) = \text ->
    if text.length == 2 then 12 else "auto"

def font_size :: own (f64 | i64 | string) -> string = \size ->
    match size with
    | :? f64 as points -> $"{points}pt"
    | :? i64 as pixels -> $"{pixels}px"
    | :? string as text -> text

let fixed: (i64 | string) = parse_size "12"
let wide: (f64 | i64 | string) = fixed
let half: (f64 | i64) = 16.5
$"{font_size wide} {"none" |> parse_size |> font_size} {font_size half}"
```

実行結果:

```text
12px auto 16.5pt
```

`parse_size` の結果 `(i64 | string)` と `half` の `(f64 | i64)` は、どちらも `(f64 | i64 | string)` の要素だけを持つので、`font_size` へそのまま渡せます。

| 式の型 `V` と期待される型 `U` | 結果 |
| --- | --- |
| `V` の要素がすべて `U` にあり、`V` のほうが要素が少ない | 拡大します |
| `V` に `U` にない要素がある、または `V` のほうが要素が多い | `E1003`（kind `anonymous_union_not_subset`） |
| 要素の数が同じで、要素が違う | `E1003`（kind `type_mismatch`。同じ型であることを求めます） |
| まだ決まっていない要素が、`U` の 2 つ以上の要素と合いうる | `E1041`（kind `anonymous_union_ambiguous_member`。型を注釈した束縛を渡します） |

`let a: (i64 | bool) = true` を `(f64 | i64 | string)` の位置へ書くと、次の診断になります。help が `U` の要素を並べ、`:?` で取り出して渡すよう案内します。

```text
expected (i64 | f64 | string), found (i64 | bool); bool is not a member of (i64 | f64 | string)
```

拡大は値の使用です。注入と同じ規則に従います。

- 値は 1 回だけ評価します。Copy でない要素を持つ共用体（`(i64 | string)`）は move され、拡大の後では使えません（`E1012`）。Copy の要素だけの共用体（`(i64 | bool)`）は複製され、後でも使えます。拡大した後でもう一度使うと、注記が `value moved here into a wider anonymous union` で最初の拡大を指し、help が拡大した値を 1 回だけ束縛するよう案内します（`a widening moves 'w'; bind the wider value once with 'let wide: (i64 | f64 | string) = w' and borrow 'wide', or move it last`）。`ref w` は拡大しないので、`ref` は案内しません。レコードのフィールドを拡大したとき（`take r.u`）は、注記が `part of the value moved here into a wider anonymous union` で、help がレコードではなくフィールドを名指しします（`a widening moves 'r.u' out of 'r'; bind the wider value once with 'let wide: (i64 | f64 | string) = r.u' and borrow 'wide', or move it last`）。ループの中の拡大が前の反復で move した値をもう一度使うときは、注記の終わりに `, in the previous iteration of the loop` が付きます。
- 部分適用（`let g = three w`、`5 |> three w`）の読み取りの引数は、できた関数の値が生きている間、引数を借用します。拡大した値は一時値なので、変数を渡しても借用できず `E1013`（kind `borrow_of_temporary`）です。メッセージは `borrow requires a local place; 'w' converted into (i64 | f64 | [i64]) is a temporary value`、help は `bind the converted value first, as in 'let wide: (i64 | f64 | [i64]) = w', and pass 'wide'; or take the parameter with 'own'` で、注入した値（`three k`）も同じです。拡大した値を型注釈のある `let` で束縛してから渡すか、引数を `own` にします。
- 参照 `ref V` の値は拡大しません（`E1003`、help `an anonymous union is widened from an owned value`）。edition 2027 の読み取りの引数は参照なので、`def f :: (i64 | string) -> (f64 | i64 | string) = \v -> v` は `E1003` で、メッセージが `own (i64 | string)` と書くよう案内します。`(i64 | bool)` のように Copy で drop の要らない共用体の引数は、edition 2027 でも値なので拡大できます。
- 総称の読み取りの引数（edition 2027 の `def keep :: 'a -> (Maybe<'a> | i64) -> (Maybe<'a> | i64 | bool) = \_x v -> v` の `v`）は、共用体なしで `\_x v -> v` と書いたときと同じく、本体が値をビューから取り出すので、具体化した型が Copy であることを求めます。`keep 1 (Some 2)` は通り、`keep "a" (Some "bc")` は呼び出しの位置で `E1005`（`no instance for Copy<string>; ...`）です。`own (Maybe<'a> | i64)` と書けば、どの型でも渡せます。
- 関数型の要素は、純粋かどうかも同じ要素にだけ対応します（拡大はペイロードをそのまま移し、関数の値を調整しません）。`(i64 | pure (i64 -> i64))` を `(i64 | bool | (i64 -> i64))` の位置へ書くと `E1003`（kind `anonymous_union_not_subset`、`pure (i64 -> i64) is not a member of ...`）です。純粋な関数の値そのものは、注入で純粋でない要素へ調整されるので、要素ごとに `:?` で取り出して渡します。
- 借用の先やコレクションの要素にある値を拡大すると、Copy の要素は複製し、Copy でない要素は move できないので `E1012` です（`Some item` と同じです）。
- 入れ物や関数の中は拡大しません。`[(i64 | string)]` を `[(f64 | i64 | string)]` へ、`Maybe<(i64 | string)>` を `Maybe<(f64 | i64 | string)>` へ渡すと `E1003` です。
- 比較などの演算子は、左の被演算子を先に検査し、その型を右の被演算子の期待される型にします（左が接尾辞のない数のリテラルなら、右を先に検査してその型をリテラルに渡します）。もう片方の型を期待される型として受け取る被演算子は拡大しません。`v == w`（`v` が `(f64 | i64 | string)`、`w` が `(i64 | string)`）は `E1003` です。拡大すると、片方の被演算子だけを move することになるためです。その被演算子の値になる位置（`if` の枝、`match`・`select` の腕、ブロックの結果、`@checked` と計算式の中身）も同じで、`v == (if c then w else u)` は枝の `w` に、`v == (if c then v else w)` は `else` の `w` に `E1003` です。左の被演算子の型がまだ決まっていなくても（`\b -> b == (if c then v else w)`）同じです。被演算子の中の呼び出しの引数と型注釈のある `let` は、これまでどおり拡大します（`v == (if c then wide w else v)` の `w`、`v == { let y: (f64 | i64 | string) = w; y }`）。
- 先に検査する被演算子は、もう片方の型を受け取らず、ひとりで検査します。その中の `if`・`match`・`select` は次の項の並んだ式の規則に従うので、`(if c then v else w) == x`（`x` も `(f64 | i64 | string)`）と `1 == (if c then v else w)` は `else` の `w` を拡大します（`let t = if c then v else w` と同じです）。`if` は枝の値から新しい値を作るので、枝に書いた変数は拡大しなくても move されます（`v` も同じです）。
- 並んだ式は、注入と同じく前の式の型を期待される型にします。期待される型のない `[v, w]` は `w` を拡大しますが、`[w, v]` は `E1003` です。型を注釈すれば順序に関係なく書けます。
- 要素の多い共用体から少ない共用体へは、自動では変えません。[`:? (A | B)` で絞る](#-a--b-で絞る)を使います。

拡大は確保をしません。タグを付け替え、ペイロードをそのまま移します。総称の関数の中で、型変数を含む要素と並ぶ配列の要素（`(Maybe<'a> | [i64])` から `(bool | Maybe<'a> | [i64])` へ）も、どの型引数で具体化しても同じです。edition 2027 の読み取りの引数へ渡すときも同じで、呼び出しは拡大した一時値を借用し、所有する値の配列の要素はその一時値へ move されます。引数に書いた `if`・`match`・ブロックの枝で拡大するとき（`weigh (if n > 0 then w else 1.5)`）も同じです。ただし次の場合は、名前付きの共用体を `match` で包み直すときと同じ複製や確保があります。

- 配列・リストの要素（Copy で、要素の領域を持つ型）は、拡大した値を後でもう一度使うとき、拡大がループの本体の中にあるか、同じ関数のその後ろにループがあるとき、パイプの左辺を拡大するときに 1 回複製されます。`let w = v` と同じ暗黙の複製で、`--warn implicit-copy` の `W1006` と `@no_copy` の `E1031` が出ます。位置は、後でもう一度使う値とパイプの左辺では拡大した値、ループによる複製では拡大した値の直後（幅 0 の位置。値が行の最後なら行末の次の列）です。
- 型の循環に入った匿名共用体（`union Tree = Leaf | Node of (i64 | Tree)` の `(i64 | Tree)`）はヒープのノードなので、その値を作る拡大はノードを確保します。

## `:?` で取り出す

```text
pattern-atom = ... | ":?" type-product [ "as" binder ]
```

- `:?` は `:` と `?` を空けずに書きます（`: ?` は `E0002 expected a pattern`）。
- `:?` の後の型は、タプルなら括弧なしで書けます。関数型と匿名共用体は括弧で囲みます（`:? (i64 -> i64) as f`）。
- `as x` は型テストの一部で、`x` の型はテストした要素の型です。共用体の値全体は `(:? i64 as n) as whole` で束縛します。
- 照合する値の型は、パターンを検査する時点で匿名共用体に決まっていなければなりません。決まっていなければ `E1041`（kind `type_test_unknown_scrutinee`）で、引数や束縛に型を注釈します。匿名共用体でない値は `E1041`（kind `type_test_not_anonymous_union`）です。名前付きの共用体はケース名で照合します。
- `:?` の型は本体の型注釈と同じ規則で解決します。署名の型変数を書けます（`:? Maybe<'a> as found`）。要素でない型は `E1041`（kind `type_test_not_member`。help が要素を並べます）です。型テストは型を調整しません。要素が `pure (i64 -> i64)` なら `:? pure (i64 -> i64)` と書き、`:? (i64 -> i64)` は要素でない型です。匿名共用体を書くと、その要素のどれかであることをテストします（[`:? (A | B)` で絞る](#-a--b-で絞る)）。
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

## `:? (A | B)` で絞る

`:?` の後に匿名共用体を書くと、照合する値がその要素のどれかであることをテストします。`as x` を付けると、`x` はその匿名共用体の値です（絞り込み）。要素の多い共用体から、一部の要素だけを受け取る関数へ値を渡せます。

```tsuzuri run=8%203%20auto
def double :: (i64 | f64) -> f64 = \number ->
    match number with
    | :? i64 as n -> 2.0 * (n as f64)
    | :? f64 as x -> 2.0 * x

def describe :: own (f64 | i64 | string) -> string = \size ->
    match size with
    | :? (i64 | f64) as number -> $"{double number}"
    | :? string as text -> text

$"{describe 4} {describe 1.5} {describe "auto"}"
```

実行結果:

```text
8 3 auto
```

- `:?` の後の匿名共用体の要素は、どれも照合する値の型の要素でなければなりません。要素でない型があれば `E1041`（kind `type_test_not_member`）で、正準順序で最初のものを名指しします（`(i64 | string)` の値に `:? (i64 | bool)` と書くと `bool is not a member of (i64 | string)`）。要素は `:? T` と同じく型を調整せずに探すので、関数型の要素は純粋かどうかまで同じでなければなりません（要素が `(i64 -> i64)` の値に `:? (i64 | pure (i64 -> i64))` は `pure (i64 -> i64) is not a member of ...`）。
- 網羅性は、要素ごとの型テストを OR でつないだ `:? A | :? B` と同じです。上の `match` は 3 つの要素をすべて覆います。`:? (i64 | f64)` の腕だけなら `E1021`（`missing: :? string`）で、すでに覆われた要素だけの絞り込みは `W1003`（到達不能）です。
- `as` のない `:? (A | B)` は、タグを比べるだけです。借用した値にもガードのある腕にも書けます。
- 照合する値の要素をすべて並べた `:? (f64 | i64 | string) as whole` は、何もテストせず、値そのものを束縛します（`whole` だけを書いたパターンと同じ規則です）。
- OR パターンの両側で同じ名前を束縛できるのは、両側が同じ型の値を同じ `:?` で絞るときだけです（`(:? (i64 | f64) as n, :? string) | (:? string, :? (i64 | f64) as n)`）。片側だけが絞り込みのとき、または絞る前の型が違うときは `E1020` です。
- 絞り込みは、要素の数だけ OR の代替を作ります。多くの絞り込みを 1 つのパターンに組み合わせると、代替の上限（1,024 個。`E1017`）に早く届きます。同じ値を束縛する代替（絞り込みの代替と、何も束縛しない `:? A | :? B`）では、所有権の検査は腕の本体を 1 回だけ調べるので、腕の中に `match` を入れ子にしても、検査の時間は代替の数の積になりません。

`as x` で要素を減らすと、`x` は新しく作った値です。所有権の規則は、絞った先の要素が Copy かどうかで決まります。

| 照合する値と腕 | 絞った先の要素がすべて Copy | Copy でない要素がある |
| --- | --- | --- |
| 所有する値、ガードのない腕 | ペイロードを複製します。照合した値は腕の中でも後でも使え、腕の中で move や代入もできます | ペイロードを move します。照合した値は使えなくなります（`:? string as s` で取り出したときと同じです） |
| 借用の先、コレクションの要素、edition 2027 の読み取りの引数 | ペイロードを複製します | `E1041`（kind `type_test_narrow_borrowed`） |
| ガード（`when`）のある腕 | ペイロードを複製します | `E1041`（kind `type_test_narrow_borrowed`） |

借用した値からは、Copy でないペイロードを取り出して新しい値を作れません。ガードは腕を選ぶ前に束縛を読むので、その時点ではまだ何も move できません。診断は次のとおりで、位置は `:?` です（`Some (:? (i64 | string) as x)` のように括弧で囲んだ型テストでも同じです）。

```text
cannot narrow a borrowed value into (i64 | string) because string is not Copy
  help: match each member with its own ':?' arm, or match an owned value

cannot narrow a value into (i64 | string) in an arm with a guard because string is not Copy
  help: match each member with its own ':?' arm, or test the condition in the arm's body
```

型変数を含む要素（`Maybe<'a>`）は、Copy と決まらないので Copy でない要素として扱います。

絞り込みと同じ値をほかの束縛が受け取るパターン（`(:? (i64 | f64) as n) as whole`）は、絞った先の要素がすべて Copy なら書けます。`n` は複製で、`whole` が値を受け取ります。Copy でない要素があれば、同じ値を 2 回 move することになるので `E1012` です（`(:? string as s) as whole` と同じです）。照合した値が変数なら、メッセージはその変数を名指しします（`use of moved or partially moved value 'v'`）。関数の結果などの一時値なら `use of moved or partially moved matched value: two bindings of the pattern take the same part of it` で、注記（`value moved here`）が先に move した束縛の名前を指し、help は `bind the whole value with 'as' or bind its parts, not both, and match the bound value again in the arm` です。

所有する値からは、Copy でない要素もそのまま絞り込めます。

```tsuzuri run=abc%203
def keep_text :: own (i64 | string | [i64]) -> (i64 | string) = \value ->
    match value with
    | :? (i64 | string) as kept -> kept
    | :? [i64] as numbers -> numbers.length

def show :: own (i64 | string) -> string = \value ->
    match value with
    | :? i64 as n -> $"{n}"
    | :? string as s -> s

$"{show (keep_text "abc")} {show (keep_text [1, 2, 3])}"
```

絞り込みも、再帰しない匿名共用体では確保をしません。ただし、配列・リストの要素（Copy で、要素の領域を持つ型）を持つ値へ絞ると、その要素は 1 回複製されます（`match` を入れ子にして包み直すときと同じです）。照合する共用体の要素がすべて Copy のとき（`(i64 | f64 | [i64])` を `:? (i64 | [i64]) as x` で絞る）も、借用の先や後でもう一度使う値から絞るときも、複製されるのは絞った値の要素だけで、照合した値全体は複製しません。ガードのある腕、同じパターンのほかの束縛が照合する値を move する腕、照合する共用体が Copy でないときに本体が照合した変数（またはそれを借用した参照）を使う腕では、パターンが値を作るので、パターンが作る値と束縛が受け取る値の 2 回です。ただし、照合する共用体が Copy で、ほかの束縛が move するのが絞る部分と別の部分なら（`(s, :? (i64 | [i64]) as x)` の `s`）、パターンは値を作らず、照合した値をその後使わず、case のペイロードの中（`Some (s, :? (i64 | [i64]) as x)`）でもなければ 1 回です（そうでなければ照合した部分全体を複製するので 2 回です）。照合する共用体が Copy でなく、関数型の要素のように借用を持ちうる要素を持つとき（`(i64 | f64 | string | (i64 -> i64))` を `:? (i64 | f64) as n` で絞る）と、型変数を含むとき（`(i64 | f64 | Maybe<'a>)`。型引数が借用を持つ型でありうるので、どの型引数で呼んでも）も、パターンが値を作ります。`--warn implicit-copy` の `W1006` と `@no_copy` の `E1031` は、パターンと束縛の名前の位置に出ます。

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

注入は `Some x` と同じ値の使用です。Copy の値は複製し、非 Copy の値は共用体の中へ move します。注入する式は 1 回だけ評価し、左から右の評価順も変わりません。拡大も同じ値の使用で、Copy でない要素を持つ共用体の値は、拡大した先へ move されます。

```text
let name = "abc"
let v: (i64 | string) = name
name.length          // E1012: use of moved or partially moved value 'name'
```

edition 2027 の読み取りの引数は参照なので、非 Copy の型の読み取りの引数を、そのまま匿名共用体へ渡すことはできません（`E1012`。help が `own` を案内します）。引数を `own` にするか、値を複製して渡します。型テストの束縛は共用体のケースのペイロードと同じ規則で、所有する値からは move、参照やコレクションの要素からは読み取り専用のビューです。`:? (A | B) as x` の `x` は新しく作った値なので、ビューにはなりません。参照やコレクションの要素から絞れるのは、要素がすべて Copy の共用体だけです（[`:? (A | B)` で絞る](#-a--b-で絞る)）。

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
- 拡大するのは所有する値だけで、入れ物・関数・参照の中の匿名共用体は拡大しません。要素の多い共用体から少ない共用体へは、`:? (A | B) as x` で絞ります（自動では変えません）。名前付きの `union` と匿名共用体の間の変換もありません。
- 部分適用の読み取りの引数へ、拡大または注入する変数をそのまま渡せません（`E1013`）。変換した値を型注釈のある `let` で束縛してから渡すか、引数を `own` にします。
- 読み取りの引数（`ref U`）へ渡した `if`・`match`・ブロックは、腕に `U` を期待します。腕が匿名共用体への参照（ほかの読み取りの引数など）だと `E1003` です（名前付きの共用体なら借用し直されます）。値をいったん束縛してから渡すか、引数を `own` にします。
- `tsuzuri api-diff` の既定の比較は構文だけを見るので、要素の順序の違いは差分にしませんが、型別名は展開しません（`(Meters | string)` と `(f64 | string)` は別の型として報告されます。`--effective` では同じ型です）。
- デバッガーは、要素の型名でケースを表示します（`i64(42)`。[デバッグ](../compiler/debugging.md)）。

## まとめ

- 匿名共用体 `(A | B)` は、宣言なしで「どれかの型」を表します。順序と入れ子は意味を持ちません。
- 期待される型が匿名共用体の位置で、要素の型の値を包みます。リテラル・ラムダ・`Dyn.of` は形で要素を選び、曖昧なら `E1041` です。
- `:? T as x` で取り出し、`match` が網羅性を検査します。
- 要素の少ない共用体の値は、期待される型の位置で要素の多い共用体へ拡大されます。逆向きは `:? (A | B) as x` で絞ります。借用した値とガードのある腕では、絞った先の要素が Copy でなければなりません（`E1041`）。
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
