# G22 付録: 実測・試作・全発行箇所の表

[G22-diagnostic-v2.md](G22-diagnostic-v2.md) の付録。本文の「現状」「仕様」「設計」「テスト計画」が参照する。HEAD `a2313d19`（作業ツリーは `9dd0fbbc`）で、2026-10-18 に測った。試作は `main` の複製で行い、実装には含まれない。

| 付録 | 内容 | 使う手順 |
| --- | --- | --- |
| A | 再現ソース（1 ブロック 1 ファイル）と HEAD の出力 | 手順 1・2・8・14（テストの定数） |
| B | 試作の出力。実装後の出力はこれと一致する（行・列・メッセージまで。34 件を最終の試作の `check` と照合済み） | 手順 8〜19 の確認 |
| C | 互換性の確認手順（変更の前後で追加の情報を持たない診断が 1 byte も変わらない） | 手順 7・13・19 |
| D-1 | Phase 2: 所有権の診断 42 か所の表と、借用の作成位置 | 手順 9〜13 |
| D-2 | Phase 3: 族ごとの全発行箇所と「触らないもの」 | 手順 14〜19 |
| E | stack の測定の手順と値、試作で通ったテスト | 手順 3 |
| F | 試作のコード（F-1〜F-8）。`cargo fmt`・`cargo clippy --all-targets -- -D warnings`・全テストが通ったもの | 手順 3〜21 |
| G | テストの全文（G-1〜G-5）。試作で通ったもの。G-1 は HEAD で通る | 手順 2〜8・14・20・21 |

付録のソースは `/tmp/tz-G22/<名前>/Main.tz` に置いて使う（GUIDE §0.6）。`/tmp/tz-G22*` はリポジトリに置かない。

## 付録 A: 再現ソースと HEAD の出力

各ソースを `/tmp/tz-G22/<名前>/Main.tz` に置き（**1 つのブロックが 1 つのファイル。太字の名前はファイルに含めない**）、`target/release/tsuzuri check /tmp/tz-G22/<名前>` を実行する。**付録 B の各出力の最初の 3 行（主の位置の引用まで）が HEAD の出力と同じ**（55 本のソースで、診断ごとの先頭 3 行が試作と HEAD で一致することを確かめた）。HEAD の `E1012`・`E1014`・`E1013` は 1 診断 3 行だけで、`note`・`help` の行がない（Phase 3 の対象の `E1021`・`E1003` も、主の位置の 3 行だけ）。

### 所有権

**move1**

```text
def consume :: string -> i64 = \s -> s.length

def main_value :: unit -> i64 = \_ ->
    let text = "hello"
    let a = consume text
    let b = consume text
    a + b

main_value ()
```

**loop1**

```text
def consume :: string -> i64 = \s -> s.length

def total :: unit -> i64 = \_ ->
    let text = "hello"
    let mut sum = 0
    for i in 0 .. 2 do
        sum = sum + consume text
    sum

0
```

**loop2**

```text
def consume :: string -> i64 = \s -> s.length

def f :: bool -> i64 =
    \c ->
        let text = "a"
        let mut n = 0
        for i in 0 .. 2 do
            if c then
                n = n + consume text
            else
                n = n + 1
        n

0
```

**branch1**

```text
def consume :: string -> i64 = \s -> s.length

def total :: bool -> i64 = \flag ->
    let text = "sample"
    if flag then
        let a = consume text
        ()
    else
        ()
    text.length

0
```

**branch2**

```text
def consume :: string -> i64 = \s -> s.length

def total :: bool -> i64 = \flag ->
    let text = "sample"
    let n = if flag then consume text else consume text + 1
    consume text + n

0
```

**partial1**

```text
record Package { title: string, body: string, count: i64 }

def take :: string -> i64 = \s -> s.length

def size :: Package -> i64 = \pkg ->
    let t = pkg.title
    let n = take pkg.body
    pkg.count + take pkg.title + t.length + n

0
```

**borrow1**

```text
let mut score = 100
let r1 = ref mut score
let r2 = ref score
deref r1
```

**borrow2**

```text
def bump :: ref mut i64 -> unit = \r -> deref r = deref r + 1

def run :: unit -> i64 = \_ ->
    let mut score = 100
    let view = ref score
    score = 5
    deref view

0
```

**borrow3**

```text
let score = 100
let r = ref mut score
deref r
```

**life1**

```text
def invalid_ref :: unit -> ref string = \_ ->
    let local = "temporary"
    ref local

0
```

**life3**

```text
def f :: string -> ref string = \s -> ref s

0
```

**esc1**（assignment lets a reference outlive its owner）

```text
def f :: unit -> i64 =
    \_ ->
        let x = 1
        let mut r = ref x
        if true then
            let y = 2
            r = ref y
        else
            ()
        deref r

0
```

**esc3**（pattern binding）

```text
union Box = Full of i64 | Empty

def f :: Box -> i64 =
    \b ->
        let zero = 0
        let mut keep = ref zero
        match b with
        | Full n -> keep = ref n
        | Empty -> ()
        deref keep

0
```

### 所有権（`related` を付けない診断。付録 D-1 の「付けない」行から 6 つ）

**no1**（`Drop` の型からフィールドを移す）

```text
extern def drop_log :: i64 -> unit
record Resource { id: i64, name: string }
instance Drop<Resource> { fn drop value = drop_log value.id }
def name_of :: Resource -> string
fn name_of value = value.name

0
```

**no2**（参照から移す）

```text
def take :: string -> i64
fn take text = text.length
let text = "owned"
let r = ref text
take r
```

**no3**（`Mutex.with_lock` の結果）

```text
let lock = Mutex.create 5i64
let kept = Mutex.with_lock (ref lock) (\value -> { let view = ref (deref value); \x -> deref view + x })
0
```

**no4**（チャネルの要素）

```text
match Channel.bounded 2 with
| (sender, receiver) ->
    let table = [40i64, 41i64]
    let view = ref table
    let _sent = Channel.send (ref sender) (\x -> x + view[0])
    match Channel.recv (ref receiver) with
    | Maybe.Some _item -> 1
    | Maybe.None -> 0
```

**no5**（配列の要素を移す）

```text
def take :: string -> i64 = \s -> s.length

let names = ["a", "b"]
take names[0]
```

**no6**（`Drop` の型のレコード更新）

```text
extern def drop_log :: i64 -> unit
record Resource { id: i64, name: string }
instance Drop<Resource> { fn drop value = drop_log value.id }
def rename :: Resource -> Resource
fn rename value = { value with name = "c" }

0
```

HEAD の出力（先頭の 1 行。いずれも `related`・`help` なしで、実装後も同じ）:

```text
no1/Main.tz:5:20: error[E1012]: cannot move a field or payload out of a value whose type implements Drop; borrow it with 'ref' instead
no2/Main.tz:5:6: error[E1012]: cannot move a non-Copy value out of a reference
no3/Main.tz:2:39: error[E1013]: Mutex.with_lock results must be proven free of borrowed environments: ...
no4/Main.tz:5:43: error[E1013]: a channel item cannot hold a borrow, including a borrowed function environment: ...
no5/Main.tz:4:6: error[E1012]: cannot move a non-Copy element out of an array or list; borrow the element instead
no6/Main.tz:5:19: error[E1012]: cannot update a value whose type implements Drop; construct a new value instead
```

### 型・名前・呼び出し

**type1**

```text
def add :: i64 -> i64 = \n -> n + true

def pick :: bool -> i64 = \flag -> if flag then 1 else "two"

let x: i64 = "text"
add 1
```

**call1**

```text
def add :: i64 -> i64 -> i64 = \a b -> a + b

def f :: unit -> i64 = \_ -> add 1

def g :: unit -> i64 = \_ -> add 1 2 3

def h :: unit -> i64 = \_ -> add 1 "two"

def k :: unit -> i64 = \_ -> add true 2

0
```

**name1**

```text
record Point { x: i64, y: i64 }

def length_of :: string -> i64 = \s -> s.length

def f :: i64 -> i64 = \count -> countt + 1

def g :: i64 -> Pointt = \n -> Point { x: n, y: n }

def h :: Point -> i64 = \p -> p.z

def k :: unit -> i64 = \_ -> Array.lenght [1, 2, 3]

def m :: unit -> i64 = \_ -> length_off "abc"

0
```

**field1**

```text
record Point { x: i64, y: i64 }

def f :: Point -> i64 = \p -> p.z

def g :: unit -> Point = \_ -> Point { x: 1, z: 2 }

0
```

**nofn**

```text
def f :: i64 = 1 2

def g :: i64 -> i64 = \n -> n 3

def h :: string -> i64 = \s -> s "x"

0
```

**std2**

```text
def f :: i64 = Array.length 5

0
```

**mismatch1**

```text
def f :: [i64; 2] -> i64 = \a -> 0

def g :: [i64; 3] -> i64 = \a -> f a

0
```

**mismatch2**

```text
def f :: Maybe<i64> -> i64 = \m -> 0

def g :: Maybe<string> -> i64 = \m -> f m

0
```

**multi1**（2 ファイルのプロジェクト。`multi1/Lib.tz` と `multi1/Main.tz`）

```text
def add :: i64 -> i64 -> i64 = \a b -> a + b
```

```text
def total :: unit -> i64 = \_ -> Lib.add 1 "two"

0
```

### 網羅性

**match1**

```text
union Shape = Circle of f64 | Square of f64 | Triangle of f64

def area :: Shape -> f64 = \s ->
    match s with
    | Circle r -> r * r * 3.0
    | Square w -> w * w

def pick :: i64 -> string = \n ->
    match n with
    | 0 -> "zero"
    | 1 -> "one"

0
```

**fix2**（`bool` とタプル。fix あり）

```text
def f :: bool -> i64 = \b ->
    match b with
    | true -> 1

def g :: bool -> bool -> i64 = \a b ->
    match (a, b) with
    | (true, true) -> 1
    | (false, _) -> 2

0
```

**fix3**（レコード・配列・リスト）

```text
record P { x: i64, y: i64 }

def f :: P -> i64 = \p ->
    match p with
    | P { x = 0, y = n } -> n

def g :: [i64] -> i64 = \xs ->
    match xs with
    | [] -> 0

def h :: [|i64|] -> i64 = \ls ->
    match ls with
    | [||] -> 0

0
```

**fix4**（関数ガード）

```text
def classify :: bool -> i64
fn classify flag
    | true -> 1

0
```

`fix2`〜`fix4` の試作の出力は付録 B。

**arms1**〜**arms6**: `match` の腕の形（`E1021` の fix の根拠。HEAD の実測。`arms1`〜`arms3` は文法の観察でテストに使わない。`arms2`〜`arms6` は付録 A の `match1` と同じ `union Shape` を使う）。

| 名前 | 形 | HEAD の結果 |
| --- | --- | --- |
| `arms1` | `def pick :: i64 -> string = \n ->` の本体 `match n with \| 0 -> "zero" \| 1 -> "one" \| _ -> "many"`（1 行）と、複数行の `match` の最後の行 `\| 1 -> "one" \| _ -> "many"` | 通る（行内の腕は文法で許される） |
| `arms2` | 外側の腕 `\| Square w -> match t with \| Circle q -> q \| Square z -> z \| Triangle y -> y` の**次の行**に外側の `\| Triangle h -> h` | 通る（`W1001` の警告だけ。次の行の `\|` は外側の腕。字下げが意味を持つ） |
| `arms3` | `arms2` の内側から `\| Triangle y -> y` を除く | `E1021` が**内側**の `match`（6:19）を指す（`missing: Triangle _`） |
| `arms4` | 下のソース。`arms6` に `E1021` の fix の edit を適用した結果と 1 byte も違わない（実測） | 通る |
| `arms5` | 下のソース。外側の最後の腕が行内の入れ子の `match`（外側は `Triangle` を持たない） | `E1021`（外側。4:5。`missing: Triangle _`）。外側に足す腕は末尾の入れ子の側に付くので、足しても直らない |
| `arms6` | 下のソース。外側の腕の本体が複数行（`let a = w * w` の次の行に `a`、`if` の `else` の次の行に `2.0`） | `E1021` が 2 件（4:5・11:5。`missing: Triangle _`）。どちらも fix あり |

**arms6**（ソース。複数行の本体の `match`。`E1021` が 2 件）

```text
union Shape = Circle of f64 | Square of f64 | Triangle of f64

def area :: Shape -> f64 = \s ->
    match s with
    | Circle r -> r * r * 3.0
    | Square w ->
        let a = w * w
        a

def area2 :: Shape -> f64 = \s ->
    match s with
    | Circle r -> r * r * 3.0
    | Square w ->
        if w > 1.0 then
            w
        else
            2.0

0
```

**arms5**（ソース。`E1021` の fix が直らない形）

```text
union Shape = Circle of f64 | Square of f64 | Triangle of f64

def area :: Shape -> Shape -> f64 = \s t ->
    match s with
    | Circle r -> r
    | Square w -> match t with | Circle q -> q | Square z -> z | Triangle _ -> unreachable ()

0
```

**arms4**（ソース。`arms6` に fix を適用した結果）

```text
union Shape = Circle of f64 | Square of f64 | Triangle of f64

def area :: Shape -> f64 = \s ->
    match s with
    | Circle r -> r * r * 3.0
    | Square w ->
        let a = w * w
        a | Triangle _ -> unreachable ()

def area2 :: Shape -> f64 = \s ->
    match s with
    | Circle r -> r * r * 3.0
    | Square w ->
        if w > 1.0 then
            w
        else
            2.0 | Triangle _ -> unreachable ()

0
```

### 重複

**dup2**

```text
record Point { x: i64, y: i64 }

record Point { a: i64 }

0
```

**dup3**

```text
union Shape = Circle of f64 | Square of f64 | Circle of f64

record Pair { a: i64, a: i64 }

0
```

**dup5**

```text
def twice :: i64 -> i64 = \n -> n * 2

def twice :: i64 -> i64 = \n -> n + n

0
```

**dup6**

```text
def add :: i64 -> i64 -> i64 = \a a -> a + a

record Pair { left: i64, left: i64 }

0
```

**dup7**

```text
def add :: i64 -> i64 -> i64 = \a a -> a + a

0
```

**dup8**（`fn` の実装の重複）

```text
def twice :: i64 -> i64

fn twice n = n * 2

fn twice n = n + n

0
```

**dup9**（`fn` の定義のあとに `def ... =`）

```text
fn twice n = n * 2

def twice :: i64 -> i64 = \n -> n + n

0
```

**dup12**（予約名。1 つ目の宣言がない）

```text
record Array { x: i64 }

0
```

HEAD の `dup2`: `E1001 duplicate or reserved name 'Point'`（3:8）、`dup3`: `E1024 duplicate case 'Circle' in union 'Shape'; give each case a distinct name`（1:47。`Pair` の重複は同じ段階の別の診断で、先の `E1024` で止まる）、`dup5`: `E1001 duplicate function signature`（3:5）、`dup6`: `E1001 duplicate or reserved name 'left'`（3:26。`add` の重複した引数は、record の段階で止まるため出ない）、`dup7`: `E1001 duplicate or reserved name 'a'`（1:35）、`dup8`: `E1001 duplicate function definition`（5:4）、`dup9`: `E1001 duplicate function definition`（3:5）、`dup12`: `E1001 duplicate or reserved name 'Array'`（1:8）。いずれも主の位置の引用だけ。`let` 形（`src/parser.rs` の `let` の分岐）の `duplicate function definition` は、分岐に入る条件が `!defined_names.contains(name)` で、中の検査が同じ集合を引くので、到達できない（試作で到達する入力を作れなかった）。

### HEAD の出力（抜粋）

```text
move1/Main.tz:6:21: error[E1012]: use of moved or partially moved value 'text'
  6 |     let b = consume text
    |                     ^
borrow2/Main.tz:6:5: error[E1014]: access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively
  6 |     score = 5
    |     ^
life1/Main.tz:3:5: error[E1013]: borrowed value does not live long enough to leave this block
  3 |     ref local
    |     ^
```

## 付録 B: 試作の出力（実装後の出力はこれと一致する）

試作は付録 F の骨格を `main` の複製に入れてビルドしたもの（`target/debug/tsuzuri check <名前>`）。以下は Phase 2・3 の期待の出力。診断の間の空行は `main.rs` が出す（既存）。

```text
##### move1
move1/Main.tz:6:21: error[E1012]: use of moved or partially moved value 'text'
  6 |     let b = consume text
    |                     ^
  note: value moved here
    --> move1/Main.tz:5:21
  5 |     let a = consume text
    |                     ^
  help: borrow the value with 'ref text' or move it last
##### loop1
loop1/Main.tz:7:29: error[E1012]: use of moved or partially moved value 'text'
  7 |         sum = sum + consume text
    |                             ^
  note: value moved here, in the previous iteration of the loop
    --> loop1/Main.tz:7:29
  7 |         sum = sum + consume text
    |                             ^
  help: borrow the value with 'ref text' or move it last
##### loop2
loop2/Main.tz:9:33: error[E1012]: use of moved or partially moved value 'text'
  9 |                 n = n + consume text
    |                                 ^
  note: value moved here, in the previous iteration of the loop
    --> loop2/Main.tz:9:33
  9 |                 n = n + consume text
    |                                 ^
  help: borrow the value with 'ref text' or move it last
##### branch1
branch1/Main.tz:10:5: error[E1012]: use of moved or partially moved value 'text'
  10 |     text.length
    |     ^
  note: value moved here
    --> branch1/Main.tz:6:25
  6 |         let a = consume text
    |                         ^
  help: borrow the value with 'ref text' or move it last
##### branch2（2 つの枝が移動する。後ろの枝を指す）
branch2/Main.tz:6:13: error[E1012]: use of moved or partially moved value 'text'
  6 |     consume text + n
    |             ^
  note: value moved here
    --> branch2/Main.tz:5:52
  5 |  if flag then consume text else consume text + 1
    |                                         ^
  help: borrow the value with 'ref text' or move it last
##### partial1
partial1/Main.tz:8:22: error[E1012]: use of moved or partially moved value 'pkg'
  8 |     pkg.count + take pkg.title + t.length + n
    |                      ^
  note: part of the value moved here
    --> partial1/Main.tz:6:13
  6 |     let t = pkg.title
    |             ^
  help: borrow the field with 'ref' instead of moving it out of 'pkg', or move it last
##### borrow1
borrow1/Main.tz:3:10: error[E1014]: access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively
  3 | let r2 = ref score
    |          ^
  note: value mutably borrowed here
    --> borrow1/Main.tz:2:10
  2 | let r1 = ref mut score
    |          ^
  note: the borrow is used here, so it is still alive
    --> borrow1/Main.tz:4:7
  4 | deref r1
    |       ^
  help: end the borrow before this access: stop using 'r1' first, or move this access after its last use
##### borrow2
borrow2/Main.tz:6:5: error[E1014]: access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively
  6 |     score = 5
    |     ^
  note: value borrowed here
    --> borrow2/Main.tz:5:16
  5 |     let view = ref score
    |                ^
  note: the borrow is used here, so it is still alive
    --> borrow2/Main.tz:7:11
  7 |     deref view
    |           ^
  help: end the borrow before this access: stop using 'view' first, or move this access after its last use
##### borrow3
borrow3/Main.tz:2:9: error[E1014]: mutable access requires 'let mut' or an exclusive reference ('ref mut' or '&mut'); record fields, array elements, and list elements are immutable
  2 | let r = ref mut score
    |         ^
  note: 'score' is declared here without 'mut'
    --> borrow3/Main.tz:1:5
  1 | let score = 100
    |     ^
  help: declare 'score' with 'let mut', or take it by 'ref mut'
##### life1
life1/Main.tz:3:5: error[E1013]: borrowed value does not live long enough to leave this block
  3 |     ref local
    |     ^
  note: 'local' is declared here and dropped at the end of this block
    --> life1/Main.tz:2:9
  2 |     let local = "temporary"
    |         ^
  help: return the value itself instead of a reference to it, or declare it in an outer scope
##### life3
life3/Main.tz:1:39: error[E1013]: cannot return a reference to a local value
  1 | def f :: string -> ref string = \s -> ref s
    |                                       ^
  note: 's' is declared here and is dropped when the function returns
    --> life3/Main.tz:1:34
  1 | def f :: string -> ref string = \s -> ref s
    |                                  ^
  help: return the value itself, or return a reference that comes from a reference parameter
##### esc1
esc1/Main.tz:6:13: error[E1013]: assignment lets a reference outlive its owner
  6 |             let y = 2
    |             ^
  note: 'y' is declared here and dropped at the end of this block
    --> esc1/Main.tz:6:17
  6 |             let y = 2
    |                 ^
  note: value borrowed here
    --> esc1/Main.tz:7:17
  7 |             r = ref y
    |                 ^
  help: declare the owner before the variable that keeps the reference, in the same block or an outer one
##### esc3
esc3/Main.tz:7:9: error[E1013]: a borrowed value cannot outlive its pattern or iteration binding
  7 |         match b with
    |         ^
  note: 'n' is bound here and ends with this arm or iteration
    --> esc3/Main.tz:8:16
  8 |         | Full n -> keep = ref n
    |                ^
  note: value borrowed here
    --> esc3/Main.tz:8:28
  8 |         | Full n -> keep = ref n
    |                            ^
  help: move or copy the value out of the pattern or loop instead of keeping a reference to it
```

```text
##### type1（1 件目は n + true。内側の不一致なので related なし）
type1/Main.tz:1:35: error[E1003]: expected i64, found bool
  1 | def add :: i64 -> i64 = \n -> n + true
    |                                   ^

type1/Main.tz:3:56: error[E1003]: expected i64, found string
  3 | l -> i64 = \flag -> if flag then 1 else "two"
    |                                         ^
  note: the result type of 'pick' is declared here
    --> type1/Main.tz:3:21
  3 | def pick :: bool -> i64 = \flag -> if flag then 1 else "two"
    |                     ^

type1/Main.tz:5:14: error[E1003]: expected i64, found string
  5 | let x: i64 = "text"
    |              ^
  note: the type annotation declares this type
    --> type1/Main.tz:5:8
  5 | let x: i64 = "text"
    |        ^
##### call1
call1/Main.tz:3:30: error[E1003]: expected i64, found i64 -> i64
  3 | def f :: unit -> i64 = \_ -> add 1
    |                              ^
  note: the result type of 'f' is declared here
    --> call1/Main.tz:3:18
  3 | def f :: unit -> i64 = \_ -> add 1
    |                  ^
  help: a call with too few arguments evaluates to a function; add the missing argument(s) (1 more needed)

call1/Main.tz:5:30: error[E1006]: cannot apply 3 arguments to a function accepting 2 arguments
  5 | def g :: unit -> i64 = \_ -> add 1 2 3
    |                              ^
  help: remove the 1 extra argument(s)

call1/Main.tz:7:36: error[E1003]: expected i64, found string
  7 | def h :: unit -> i64 = \_ -> add 1 "two"
    |                                    ^
  note: parameter 2 of 'add' is declared here
    --> call1/Main.tz:1:19
  1 | def add :: i64 -> i64 -> i64 = \a b -> a + b
    |                   ^

call1/Main.tz:9:34: error[E1003]: expected i64, found bool
  9 | def k :: unit -> i64 = \_ -> add true 2
    |                                  ^
  note: parameter 1 of 'add' is declared here
    --> call1/Main.tz:1:12
  1 | def add :: i64 -> i64 -> i64 = \a b -> a + b
    |            ^
##### name1
name1/Main.tz:5:33: error[E1002]: unknown value 'countt'
  5 | def f :: i64 -> i64 = \count -> countt + 1
    |                                 ^
  help: check the spelling, or define the name with 'let' or 'def'; names in scope: count

name1/Main.tz:7:17: error[E1004]: unknown record, union, or type alias 'Pointt'
  7 | def g :: i64 -> Pointt = \n -> Point { x: n, y: n }
    |                 ^

name1/Main.tz:9:33: error[E1007]: unknown field 'z'
  9 | def h :: Point -> i64 = \p -> p.z
    |                                 ^
  help: the fields of 'Main.Point' are: x, y

name1/Main.tz:11:36: error[E1002]: module 'Array' has no function or union case 'lenght'
  11 | def k :: unit -> i64 = \_ -> Array.lenght [1, 2, 3]
    |                                    ^

name1/Main.tz:13:30: error[E1002]: unknown value 'length_off'
  13 | def m :: unit -> i64 = \_ -> length_off "abc"
    |                              ^
  help: check the spelling, or define the name with 'let' or 'def'
##### nofn
nofn/Main.tz:1:16: error[E1005]: only a function value can be called
  1 | def f :: i64 = 1 2
    |                ^
  help: a number is not a function; remove the argument(s) or call a function

nofn/Main.tz:3:29: error[E1005]: only a function value can be called
  3 | def g :: i64 -> i64 = \n -> n 3
    |                             ^
  help: a value of type i64 is not a function; remove the argument(s) or call a function

nofn/Main.tz:5:32: error[E1005]: only a function value can be called
  5 | def h :: string -> i64 = \s -> s "x"
    |                                ^
  help: a value of type string is not a function; remove the argument(s) or call a function
##### mismatch1
mismatch1/Main.tz:3:36: error[E1003]: expected [i64; 2], found [i64; 3]
  3 | def g :: [i64; 3] -> i64 = \a -> f a
    |                                    ^
  note: parameter 1 of 'f' is declared here
    --> mismatch1/Main.tz:1:10
  1 | def f :: [i64; 2] -> i64 = \a -> 0
    |          ^
##### mismatch2
mismatch2/Main.tz:3:41: error[E1003]: expected Maybe<i64>, found Maybe<string>
  3 | def g :: Maybe<string> -> i64 = \m -> f m
    |                                         ^
  note: parameter 1 of 'f' is declared here
    --> mismatch2/Main.tz:1:10
  1 | def f :: Maybe<i64> -> i64 = \m -> 0
    |          ^
##### multi1
multi1/Main.tz:1:44: error[E1003]: expected i64, found string
  1 |  total :: unit -> i64 = \_ -> Lib.add 1 "two"
    |                                         ^
  note: parameter 2 of 'add' is declared here
    --> multi1/Lib.tz:1:19
  1 | def add :: i64 -> i64 -> i64 = \a b -> a + b
    |                   ^
##### match1
match1/Main.tz:4:5: error[E1021]: match is not exhaustive; missing: Triangle _
  4 |     match s with
    |     ^
  help: add an arm for 'Triangle _', or end the match with a wildcard arm '| _ -> ...'
  fix (maybe): Add the arm '| Triangle _ -> unreachable ()'

match1/Main.tz:9:5: error[E1021]: match is not exhaustive; missing: _
  9 |     match n with
    |     ^
  help: add an arm for '_', or end the match with a wildcard arm '| _ -> ...'
  fix (maybe): Add the arm '| _ -> unreachable ()'
##### fix2
fix2/Main.tz:2:5: error[E1021]: match is not exhaustive; missing: false
  2 |     match b with
    |     ^
  help: add an arm for 'false', or end the match with a wildcard arm '| _ -> ...'
  fix (maybe): Add the arm '| false -> unreachable ()'

fix2/Main.tz:6:5: error[E1021]: match is not exhaustive; missing: (true, false)
  6 |     match (a, b) with
    |     ^
  help: add an arm for '(true, false)', or end the match with a wildcard arm '| _ -> ...'
  fix (maybe): Add the arm '| (true, false) -> unreachable ()'
##### fix3
fix3/Main.tz:4:5: error[E1021]: match is not exhaustive; missing: _
  4 |     match p with
    |     ^
  help: add an arm for '_', or end the match with a wildcard arm '| _ -> ...'

fix3/Main.tz:8:5: error[E1021]: match is not exhaustive; missing: _
  8 |     match xs with
    |     ^
  help: add an arm for '_', or end the match with a wildcard arm '| _ -> ...'
  fix (maybe): Add the arm '| _ -> unreachable ()'

fix3/Main.tz:12:5: error[E1021]: match is not exhaustive; missing: _ :: _
  12 |     match ls with
    |     ^
  help: add an arm for '_ :: _', or end the match with a wildcard arm '| _ -> ...'
##### fix4
fix4/Main.tz:3:5: error[E1021]: match is not exhaustive; missing: false
  3 |     | true -> 1
    |     ^
  help: add an arm for 'false', or end the match with a wildcard arm '| _ -> ...'
##### arms6
arms6/Main.tz:4:5: error[E1021]: match is not exhaustive; missing: Triangle _
  4 |     match s with
    |     ^
  help: add an arm for 'Triangle _', or end the match with a wildcard arm '| _ -> ...'
  fix (maybe): Add the arm '| Triangle _ -> unreachable ()'

arms6/Main.tz:11:5: error[E1021]: match is not exhaustive; missing: Triangle _
  11 |     match s with
    |     ^
  help: add an arm for 'Triangle _', or end the match with a wildcard arm '| _ -> ...'
  fix (maybe): Add the arm '| Triangle _ -> unreachable ()'
##### dup2
dup2/Main.tz:3:8: error[E1001]: duplicate or reserved name 'Point'
  3 | record Point { a: i64 }
    |        ^
  note: first defined here
    --> dup2/Main.tz:1:8
  1 | record Point { x: i64, y: i64 }
    |        ^
##### dup3
dup3/Main.tz:1:47: error[E1024]: duplicate case 'Circle' in union 'Shape'; give each case a distinct name
  1 | Shape = Circle of f64 | Square of f64 | Circle of f64
    |                                         ^
  note: first defined here
    --> dup3/Main.tz:1:15
  1 | union Shape = Circle of f64 | Square of f64 | Circle of f64
    |               ^
##### dup5
dup5/Main.tz:3:5: error[E1001]: duplicate function signature
  3 | def twice :: i64 -> i64 = \n -> n + n
    |     ^
  note: first defined here
    --> dup5/Main.tz:1:5
  1 | def twice :: i64 -> i64 = \n -> n * 2
    |     ^
##### dup6（record のフィールド）
dup6/Main.tz:3:26: error[E1001]: duplicate or reserved name 'left'
  3 | record Pair { left: i64, left: i64 }
    |                          ^
  note: first defined here
    --> dup6/Main.tz:3:15
  3 | record Pair { left: i64, left: i64 }
    |               ^
##### dup7（関数の引数）
dup7/Main.tz:1:35: error[E1001]: duplicate or reserved name 'a'
  1 | def add :: i64 -> i64 -> i64 = \a a -> a + a
    |                                   ^
  note: first defined here
    --> dup7/Main.tz:1:33
  1 | def add :: i64 -> i64 -> i64 = \a a -> a + a
    |                                 ^
##### dup8
dup8/Main.tz:5:4: error[E1001]: duplicate function definition
  5 | fn twice n = n + n
    |    ^
  note: first defined here
    --> dup8/Main.tz:3:4
  3 | fn twice n = n * 2
    |    ^
##### dup9
dup9/Main.tz:3:5: error[E1001]: duplicate function definition
  3 | def twice :: i64 -> i64 = \n -> n + n
    |     ^
  note: first defined here
    --> dup9/Main.tz:1:4
  1 | fn twice n = n * 2
    |    ^
##### dup12（予約名。`related` を付けない。HEAD と同じ出力）
dup12/Main.tz:1:8: error[E1001]: duplicate or reserved name 'Array'
  1 | record Array { x: i64 }
    |        ^
##### std2（note が別のソース std/Array.tz を指す。--json の path は std/Array.tz）
std2/Main.tz:1:29: error[E1003]: expected ref [an undetermined type], found i32
  1 | def f :: i64 = Array.length 5
    |                             ^
  note: parameter 1 of 'length' is declared here
    --> std/Array.tz:1:15
  1 | def length :: ref ['a] -> i64
    |               ^
```


JSON（`check --json`。実際は 1 診断が 1 行）:

```text
{"severity":"error","code":"E1012","message":"use of moved or partially moved value 'text'","path":"move1/Main.tz","span":{"start":153,"end":157,"line":6,"column":21,"end_line":6,"end_column":25},"related":[{"role":"moved_here","message":"value moved here","path":"move1/Main.tz","span":{"start":128,"end":132,"line":5,"column":21,"end_line":5,"end_column":25}}],"help":"borrow the value with 'ref text' or move it last"}
{"severity":"error","code":"E1014","message":"access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively","path":"borrow2/Main.tz","span":{"start":147,"end":156,"line":6,"column":5,"end_line":6,"end_column":14},"related":[{"role":"borrowed_here","message":"value borrowed here","path":"borrow2/Main.tz","span":{"start":133,"end":142,"line":5,"column":16,"end_line":5,"end_column":25}},{"role":"borrow_used_here","message":"the borrow is used here, so it is still alive","path":"borrow2/Main.tz","span":{"start":167,"end":171,"line":7,"column":11,"end_line":7,"end_column":15}}],"help":"end the borrow before this access: stop using 'view' first, or move this access after its last use"}
{"severity":"error","code":"E1021","message":"match is not exhaustive; missing: Triangle _","path":"match1/Main.tz","span":{"start":100,"end":166,"line":4,"column":5,"end_line":6,"end_column":24},"help":"add an arm for 'Triangle _', or end the match with a wildcard arm '| _ -> ...'","fixes":[{"title":"Add the arm '| Triangle _ -> unreachable ()'","applicability":"maybe","edits":[{"path":"match1/Main.tz","span":{"start":166,"end":166,"line":6,"column":24,"end_line":6,"end_column":24},"text":" | Triangle _ -> unreachable ()"}]}]}
{"severity":"error","code":"E1002","message":"unknown value 'countt'","path":"name1/Main.tz","span":{"start":114,"end":120,"line":5,"column":33,"end_line":5,"end_column":39},"help":"check the spelling, or define the name with 'let' or 'def'; names in scope: count","scope":[{"name":"count","type":"i64"}]}
{"severity":"error","code":"E1003","message":"expected ref [an undetermined type], found i32","path":"std2/Main.tz","span":{"start":28,"end":29,"line":1,"column":29,"end_line":1,"end_column":30},"related":[{"role":"expected_because","message":"parameter 1 of 'length' is declared here","path":"std/Array.tz","span":{"start":14,"end":22,"line":1,"column":15,"end_line":1,"end_column":23}}],"expected":"ref [an undetermined type]","found":"i32"}
```

`match1` の fix の edit を適用した結果（Python で `start`・`end`・`text` を後ろから適用して `check`）は終了コード 0: 最後の腕が `| Square w -> w * w | Triangle _ -> unreachable ()` と `| 1 -> "one" | _ -> unreachable ()` になる。

LSP（試作のテスト 3 件が通った。`tests/lsp.rs` に足したものと同じ）: `move1` の `relatedInformation[0]` は `{"location":{"uri":<同じファイルの URI>,"range":{"start":{"line":4,"character":20},"end":{"line":4,"character":24}}},"message":"value moved here"}`（5 行目の `text`。0 始まり）。`E1021` の quickfix は `title: "Add the arm '| Triangle _ -> unreachable ()'"`・`kind: "quickfix"`・`isPreferred: false`・edit `{"range":{"start":{"line":5,"character":19},"end":{"line":5,"character":19}},"newText":" | Triangle _ -> unreachable ()"}`・`diagnostics[0].data.fixes[0].applicability == "maybe"`。`arms5` は quickfix 0 件。

## 付録 C: 互換性の確認手順

追加の情報を持たない診断の出力が変わらないことを、変更の前後で比べる。変更前のコンパイラは `/tmp/tz-G22/tsuzuri-before`（手順 1 で保存）。コーパスは付録 A の全ソースと `tests/fixtures/*/` と `examples/*/`。

```sh
mkdir -p /tmp/tz-G22-out/before /tmp/tz-G22-out/after
run() { # $1 = コンパイラ, $2 = 出力先
  for dir in /tmp/tz-G22/*/ tests/fixtures/*/ examples/*/; do
    [ -f "$dir/Main.tz" ] || continue
    name=$(echo "$dir" | tr '/' '_')
    "$1" check "$dir" --json 2> "$2/$name.jsonl"; echo $? > "$2/$name.code"
    "$1" check "$dir" 2> "$2/$name.txt"
  done
}
run /tmp/tz-G22/tsuzuri-before /tmp/tz-G22-out/before
run target/release/tsuzuri /tmp/tz-G22-out/after
```

- **Phase 1**: `diff -r /tmp/tz-G22-out/before /tmp/tz-G22-out/after` が空（JSON もテキストも終了コードも 1 byte も違わない）。
- **Phase 2 以降**: 追加の情報を取り除いて同じ。次のスクリプトが、JSON から追加の 8 キーを、テキストから各診断の最初の `  note: `・`  help: `・`  fix (` の行以降（その診断の終わりまで）を取り除いて比べる。終了コードのファイルはそのまま比べる。試作で、確認用のソース（付録 A のものを含む）と `tests/fixtures/*/`・`examples/*/` の 157 本（471 ファイル）がすべて等しかった。`tests/fixtures`・`examples` は 1 byte も変わらず、出力が増えたのは付録 A のソースだけだった。

```sh
node -e '
const fs = require("fs"), path = require("path");
const [before, after] = process.argv.slice(1);
const added = ["kind","related","help","expected","found","candidates","scope","fixes"];
const stripJson = (text) => text.split("\n").map((line) => {
  if (!line.startsWith("{")) return line;
  const d = JSON.parse(line);
  for (const key of added) delete d[key];
  return JSON.stringify(d);
}).join("\n");
const stripText = (text) => text.replace(/\n+$/, "").split("\n\n").map((block) => {
  const lines = block.split("\n");
  const cut = lines.findIndex((l) => /^  (note: |help: |fix \()/.test(l));
  return (cut < 0 ? lines : lines.slice(0, cut)).join("\n");
}).join("\n\n");
let bad = 0;
for (const file of fs.readdirSync(before)) {
  const a = fs.readFileSync(path.join(before, file), "utf8");
  const b = fs.readFileSync(path.join(after, file), "utf8");
  const [x, y] = file.endsWith(".jsonl") ? [stripJson(a), stripJson(b)]
    : file.endsWith(".txt") ? [stripText(a), stripText(b)] : [a, b];
  if (x !== y) { bad++; console.log("DIFF", file); }
}
process.exit(bad ? 1 : 0);
' /tmp/tz-G22-out/before /tmp/tz-G22-out/after
```

さらに、`TSUZURI_THREADS=1` と `TSUZURI_THREADS=8` で `after` を作り、`diff -r` が空であること（診断が並列度に依らない）。


## 付録 D-1: Phase 2 の全発行箇所（所有権の診断 42 か所）

`grep -n '"E101[234]"' src/ownership.rs src/ownership_control.rs` が 41 行と 1 行。関数名は `src/ownership.rs` の `impl Checker` のもの（行番号は書かない）。**付ける 7、付けない 35**。`help` が「なし」の行は、メッセージ自体が修正方法を含む（D13）。

| # | 関数 | メッセージ（HEAD） | `related` | `help` |
| --- | --- | --- | --- | --- |
| 1 | `access` | `use of moved or partially moved value '{name}'` | **付ける**: `MovedHere` | 全体 `borrow the value with 'ref {name}' or move it last`／部分 `borrow the field with 'ref' instead of moving it out of '{name}', or move it last` |
| 2 | `access` | `cannot replace the whole value inside Drop.drop; ...` | 付けない | なし |
| 3 | `access` | `cannot pass the value on as 'ref mut' inside Drop.drop; ...` | 付けない | なし |
| 4 | `read_places` | `cannot move a non-Copy element out of an array or list; borrow the element instead` | 付けない | なし |
| 5 | `read_places` | `cannot move a non-Copy value out of an array or list element; borrow it instead` | 付けない | なし |
| 6 | `read_places` | `cannot move a non-Copy pattern variable bound through a reference or a collection element; ...`／`cannot move a non-Copy value out of a reference` | 付けない | なし |
| 7 | `read_places` | `cannot move a field or payload out of a value whose type implements Drop; ...`（`MOVE_OUT_OF_DROP`） | 付けない | なし |
| 8 | `eval_value`（レコード更新） | `cannot update a value whose type implements Drop; construct a new value instead` | 付けない | なし |
| 9 | `eval_value` | `cannot move a non-Copy array or list element; borrow it instead` | 付けない | なし |
| 10 | `eval_value` | `MOVE_OUT_OF_DROP` | 付けない | なし |
| 11 | `access` | `mutable access requires 'let mut' or an exclusive reference ...` | **付ける（条件付き）**: `DeclaredHere`（`'{name}' is declared here without 'mut'`。immutable な局所変数・`via` が空・フィールドなし・名前が `$` で始まらない・span が空でない） | `declare '{name}' with 'let mut', or take it by 'ref mut'`。条件を満たさないとき `write through a 'let mut' variable that holds the whole value, or take it by 'ref mut'` |
| 12 | `access` | `access conflicts with a live borrow; ...` | **付ける**: `BorrowedHere`・`BorrowUsedHere`（「仕様」の決定表） | 「仕様」の決定表 |
| 13 | `shared_exclusive` | `cannot mutate or exclusively reborrow through a shared reference` | 付けない | なし |
| 14 | `read_places` | `cannot move an exclusive reference while it is reborrowed` | 付けない | なし |
| 15 | `call_contract` | `this function does not keep the region-quantified parameter type; ...`・`pass a named function with matching named regions, ...` | 付けない | なし |
| 16 | `check_result` | `task results cannot retain borrowed values`／`cannot return a reference to a local value` | **後者だけ付ける**: `escape`（`DeclaredHere` `'{name}' is declared here and is dropped when the function returns`、`BorrowedHere`） | `return the value itself, or return a reference that comes from a reference parameter`（前者は付けない） |
| 17 | `check_result` | `returned borrow does not match the declared result region; ...` | 付けない | なし |
| 18 | `place` | `reference has no live owner` | 付けない | なし |
| 19 | `place` | `borrow requires a local place; bind the temporary with 'let' first` | 付けない | なし |
| 20 | `store_through` | `nested borrowed values require explicit lifetime parameters ...; name the target's region ...`（`UNNAMED` を含む形） | 付けない | なし |
| 21 | `store_through` | `cannot store a borrow of a local value through a reference parameter; ...` | 付けない | なし |
| 22 | `store_through` | `the stored borrow does not have the region of the reference's target; ...` | 付けない | なし |
| 23 | `store_through` | `UNNAMED` | 付けない | なし |
| 24 | `store_arguments` | `cannot store a reference inside its own owner` | 付けない | なし |
| 25 | `read_places` | `nested borrowed values require explicit lifetime parameters, which are not supported` | 付けない | なし |
| 26 | `eval` | `async computations cannot keep borrowed values across ...`（`ASYNC_BORROW`） | 付けない | なし |
| 27 | `eval_block` | `borrowed value does not live long enough to leave this block` | **付ける**: `escape`（`DeclaredHere` `'{name}' is declared here and dropped at the end of this block`、`BorrowedHere`） | `return the value itself instead of a reference to it, or declare it in an outer scope` |
| 28 | `eval_block` | `assignment lets a reference outlive its owner` | **付ける**: `escape`（同上の `DeclaredHere`・`BorrowedHere`） | `declare the owner before the variable that keeps the reference, in the same block or an outer one` |
| 29 | `eval_call` | `Mutex.with_lock results must be proven free of borrowed environments: ...` | 付けない | なし |
| 30 | `eval_call` | `a dyn value without a region cannot hold borrowed data; ...` | 付けない | なし |
| 31 | `eval_call` | `a channel item cannot hold a borrow, ...` | 付けない | なし |
| 32 | `capture_loans` | `task captures cannot retain borrowed values, including borrowed function environments` | 付けない | なし |
| 33 | `eval_value` | `parallel operations must be fully applied directly; ...` | 付けない | なし |
| 34 | `eval_value` | `Mutex.with_lock must be fully applied directly ...` | 付けない | なし |
| 35 | `eval_value` | `Channel.send must be fully applied directly ...` | 付けない | なし |
| 36 | `eval_value`（`Assign`） | `cannot store a reference inside its own owner` | 付けない | なし |
| 37 | `eval_value`（`Closure`） | `task captures cannot retain borrowed values, ...` | 付けない | なし |
| 38 | `eval_value` | `task scopes can share only values with proven owned environments` | 付けない | なし |
| 39 | `eval_value` | `parallel input elements must have proven owned environments` | 付けない | なし |
| 40 | `eval_value` | `parallel callbacks and values cannot retain borrowed environments` | 付けない | なし |
| 41 | `eval_value` | `parallel callback results must be proven free of borrowed environments` | 付けない | なし |
| 42 | `finish_control_scope`（`ownership_control.rs`） | `a borrowed value cannot outlive its pattern or iteration binding` | **付ける**: `escape`（`DeclaredHere` `'{name}' is bound here and ends with this arm or iteration`、`BorrowedHere`） | `move or copy the value out of the pattern or loop instead of keeping a reference to it` |

内訳: `E1012` は 1〜10、`E1014` は 11〜14、`E1013` は 15〜42（28）。`escape` を呼ぶのは 16・27・28・42。

**借用の作成位置（`Loan.span`）**: `Checker::loan(place, mutable, parents, span)` の 9 つの呼び出しと、`Loan` を直接作る 1 か所。

| # | 呼び出し | `span` |
| --- | --- | --- |
| 1 | `external_loans`（引数が呼び出し元から借りるもの） | `parameter.span` |
| 2 | `place` の `E::Index`（添字を評価する間の guard） | `value.span` |
| 3 | `exclusive_slice`（guard） | 引数の `span` |
| 4 | `exclusive_slice`（結果） | 引数の `span` |
| 5 | `read_places`（`Use::Read` の借用） | `expression.span` |
| 6 | `eval_call`（結果が静的データだけを借りるときの loan） | `expression.span` |
| 7 | `eval_value` の `E::Slice` | `expression.span` |
| 8 | `eval_value` の `E::Borrow` | `expression.span` |
| 9 | `ownership_control.rs` の `protect` | `local.span` |
| — | `ownership_control.rs` の `view`（`Loan { .. }` を直接 push） | `projection.span` |

## 付録 D-2: Phase 3 の全発行箇所

`Diagnostic::new` は `src/` に 475、`Diagnostic::warning` は 8（`grep -rn "Diagnostic::new" src | wc -l`・`grep -rn "Diagnostic::warning" src | wc -l`）。`src/` の中の `"E10xx"` の出現数（`grep -rn '"E1003"' src | wc -l`。`#[cfg(test)]` の中を含む）: `E1003` 17、`E1002` 8、`E1004` 35、`E1005` 52、`E1006` 9、`E1007` 9、`E1015` 34、`E1021` 2、`E1001` 12、`E1024` 15。AI の修復に効く族を次のとおり選んだ。**ここにない発行箇所は変えない**。

| # | コード | 場所 | 追加する情報 | テスト（`tests/diagnostic_v2.rs`） |
| --- | --- | --- | --- | --- |
| 1 | `E1003` | `src/polymorph.rs` `Inference::unify` の最後の `Err` | `expected`・`found`（`mismatch`） | `mismatch_has_expected_and_found`（`type1`） |
| 2 | `E1003` | 同 `unify` の `FixedArray` の長さの書き換え | 同上 | `mismatch_has_expected_and_found`（`mismatch1`） |
| 3 | `E1003` | 同 `unify` の `Record`／`Union` の型引数の書き換え | 同上 | 同（`mismatch2`） |
| 4 | `E1003` | `src/check.rs` `block_bindings`（`let` の注釈） | `ExpectedBecause` `the type annotation declares this type` | `annotation_mismatch_names_the_annotation`（`type1`） |
| 5 | `E1003` | `check_modules_collect` の関数本体の検査 | `ExpectedBecause` `the result type of '{name}' is declared here` | `result_mismatch_names_the_result_type`（`type1`・`call1`） |
| 6 | `E1003` | `call_expression` の引数のループ | `ExpectedBecause` `parameter {n} of '{name}' is declared here` | `argument_mismatch_names_the_parameter`（`call1`）、`std_parameter_note_points_into_std`（`std2`） |
| 7 | `E1003` | `call_expression` の `self.same(&result, &hint, ..)` | `help`（引数が足りない） | `partial_application_gets_a_help`（`call1`） |
| 8 | `E1002` | `Checker::name` の最後の `Err`（`unknown value`） | `scope`・`help` | `unknown_value_lists_the_scope`（`name1`） |
| 9 | `E1007` | `field_access`・`record_literal`・`record_update` | `help`（フィールドの一覧） | `unknown_field_lists_the_fields`（`field1`） |
| 10 | `E1006` | `src/polymorph.rs` `call_signature` の `cannot apply` | `help` | `call_helps`（`call1`） |
| 11 | `E1005` | `call_signature` の `only a function value can be called` 2 か所 | `help` | `call_helps`（`nofn`） |
| 12 | `E1021` | `src/exhaustiveness.rs` `check_coverage` | `help`・`fix`（`maybe`。条件は「仕様」） | `non_exhaustive_match_offers_a_fix_that_compiles`（`match1`・`fix2`・`arms6`）、`no_fix_for_unwritable_witnesses`（`fix3`・`fix4`）、`nested_inline_match_fix_is_not_verified_here`（`arms5`） |
| 13 | `E1001` | `src/parser.rs` の `duplicate function definition`（`fn` の実装の重複。非宣言の経路） | `FirstDefinedHere` | `duplicates_name_the_first_declaration`（`dup8`） |
| 14 | `E1001` | `parser.rs` の `duplicate function definition`（`def name :: .. = ..` が定義済みの名前） | 同上 | 同（`dup9`） |
| 15 | `E1001` | `parser.rs` の `duplicate function signature` | 同上 | 同（`dup5`） |
| 16 | `E1001` | `check_modules_collect` の record の重複 | 同上 | 同（`dup2`） |
| 17 | `E1001` | 同 record のフィールドの重複 | 同上 | 同（`dup6`） |
| 18 | `E1001` | 同 関数の引数の重複 | 同上 | 同（`dup7`） |
| 19 | `E1024` | `collect_unions` の `duplicate case` | 同上 | 同（`dup3`） |
| — | `E1001` | 予約名（`record Array`）。1 つ目の宣言がない | 付けない | `reserved_names_have_no_first_definition`（`dup12`） |

**触らないもの**（理由つき）: `E1002` の `unknown function`（`qualified_function`）と `module '..' has no function or union case`（`value_expression`）は、候補が G25 の仕事。`E1004`（35 件）は型・record・union・クラスの名前の解決で、候補が G25 の仕事。`E1015`（34 件）は内部エラー・上限・種類の不整合で、メッセージが注釈の書き方を含む。`E1005` の残り（52 件から `call_signature` の 2 件を除く 50 件）は「捕捉できない値」「SIMD 幅」など機能の制限で、メッセージが理由を含む。`E1016`・`E1017`・`E1020`・`E2000` 系は G23 が `kind` と一緒に扱う。`duplicate(` の呼び出しは `src/` に 21 か所（`grep -rn --include=*.rs '[^_a-z]duplicate(' src | grep -v 'fn duplicate' | wc -l`）で、触るのは `check.rs` の record（`record_declarations`）・record のフィールド・関数の引数の 3 か所だけ。残りの 18 か所（`check.rs` の extern・handle・class・alias・関数名・active pattern の case・`collect_unions` の union 名と case 名の衝突 2 か所・`let rec`・record literal・record update の 11 か所、`closures.rs` 1、`control.rs` 2、`polymorph.rs` 4）は、名前の集合の重複判定や別の種類の宣言との衝突で、1 つ目の宣言の位置を取る手段が別に要るので触らない。`parser.rs` の `let` 形の `duplicate function definition` は到達できないので触らない（付録 A の `dup` の節）。

## 付録 E: stack の測定

D1 の根拠（本文の「現状」の表）。2026-10-18、macOS arm64、rustc 1.98.1、debug。

```sh
rsync -a --exclude target --exclude .git --exclude vsc --exclude node_modules ./ /tmp/tz-G22-copy/
cd /tmp/tz-G22-copy && export CARGO_TARGET_DIR=/tmp/tz-G22-target
cargo test --offline --locked --lib bounds_recursive_and_flat_expression_depth          # 既定の stack
cargo test --offline --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --offline --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
# 必要 stack の二分探索（16 KiB 刻み）。テストのバイナリは target/debug/deps/<名前>-<hash>
bisect() { local bin=$1 filter=$2 lo=262144 hi=8388608
  while [ $((hi-lo)) -gt 16384 ]; do mid=$(( (lo+hi)/2 ))
    if RUST_MIN_STACK=$mid ./$bin $filter >/dev/null 2>&1; then hi=$mid; else lo=$mid; fi
  done; echo "$bin $filter ~$hi"; }
```

| 変種 | `Diagnostic` | `Result<(), Diagnostic>` | lib | polymorphism | computations |
| --- | --- | --- | --- | --- | --- |
| HEAD | 80 bytes | 80 bytes | 約 2,087,424 | 約 2,071,552 | 約 2,055,680 |
| A: D-51 のままフィールドを直接置く | 264 bytes | 264 bytes | **溢れる** | **溢れる** | **溢れる** |
| C（D1）: `Diagnostic(Box<DiagnosticData>)`。Phase 1〜4 をすべて入れた試作 | 8 bytes | 8 bytes | 約 1,544,192 | 約 1,609,728 | 約 1,847,296 |

既定の stack は 2,097,152 bytes。Phase 1 だけの試作でも約 1,547,776 / 1,611,264 / 1,849,344 で、Phase 2〜4 の追加（`check.rs` の `block_bindings`・`call_expression` の呼び出しを含む）で frame はほとんど増えなかった。

最終の試作（Phase 1〜4）で `cargo fmt --all -- --check` と `cargo clippy --offline --locked --all-targets -- -D warnings` が緑、`RUST_MIN_STACK=4194304 TSUZURI_THREADS=8 cargo test --offline --locked --no-fail-fast` が 108 のテスト対象・1,490 件すべて通った（`ignore` の 1 件は既存）。**この環境では lib の `tests::the_deepest_accepted_nesting_is_checked_the_same_on_workers` が HEAD でも既定の stack で溢れる**ので、全体は `RUST_MIN_STACK=4194304` で流す。複製には `benchmarks/` が要る（`tests/call_specialization.rs`・`tests/formatter.rs` が読む）。

## 付録 F: 試作の骨格コード（`main` の複製に入れ、`cargo clippy --all-targets -- -D warnings` と全テストが通ったもの）

名前・型・メッセージはこのとおりにする。`cargo fmt` 済み。関数の中身は、ここにあるものは**そのまま使ってよい**。付録 F の範囲に書いていない変更（Phase 1 の 18 か所の書き換えと `State.moved` の map 化）は本文の「設計」の表のとおり。

### F-1: `src/diagnostic.rs` の型とビルダー（Phase 1 の手順 3・4）

`Diagnostic::new`・`warning` は HEAD の同名の関数を、次の形に置き換える。`Severity`・`Span`・`DiagnosticSet`・`Diagnostics` は変えない。

```rust
/// How many related locations, fixes, edits of a fix, candidates, and scope entries a
/// diagnostic keeps; the builders drop the rest.
pub const MAX_RELATED: usize = 16;
pub const MAX_FIXES: usize = 8;
pub const MAX_EDITS: usize = 64;
pub const MAX_CANDIDATES: usize = 10;
pub const MAX_SCOPE: usize = 32;

/// Why a related location is shown. The set is closed; `as_str` is the JSON `role`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelatedRole {
    MovedHere,
    BorrowedHere,
    BorrowUsedHere,
    DeclaredHere,
    ExpectedBecause,
    FirstDefinedHere,
    ViaCall,
    RequiredBy,
    Note,
}

impl RelatedRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MovedHere => "moved_here",
            Self::BorrowedHere => "borrowed_here",
            Self::BorrowUsedHere => "borrow_used_here",
            Self::DeclaredHere => "declared_here",
            Self::ExpectedBecause => "expected_because",
            Self::FirstDefinedHere => "first_defined_here",
            Self::ViaCall => "via_call",
            Self::RequiredBy => "required_by",
            Self::Note => "note",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Related {
    pub role: RelatedRole,
    pub message: String,
    pub span: Span,
}

/// How far a fix can be trusted: `Certain` keeps the meaning, `Likely` is almost always right,
/// `Maybe` is a suggestion that a person or an agent completes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applicability {
    Certain,
    Likely,
    Maybe,
}

impl Applicability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Certain => "certain",
            Self::Likely => "likely",
            Self::Maybe => "maybe",
        }
    }
}

/// Replaces the bytes `span.start..span.end` of a source with `text`; an empty span inserts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    pub title: String,
    pub applicability: Applicability,
    pub edits: Vec<Edit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    pub signature: String,
    pub how: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeEntry {
    pub name: String,
    pub ty: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detail {
    pub expected: Option<String>,
    pub found: Option<String>,
    pub candidates: Vec<Candidate>,
    pub scope: Vec<ScopeEntry>,
}

/// Finds the path and text of the source that a `Span::source` index names.
pub type SourceLookup<'a> = dyn Fn(usize) -> Option<(&'a str, &'a str)> + 'a;

/// A diagnostic. The data is boxed so that the many `Result<_, Diagnostic>` temporaries on the
/// checker's recursive paths stay one pointer wide; the fields are read and written through
/// `Deref`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic(Box<DiagnosticData>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticData {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub kind: Option<&'static str>,
    pub related: Vec<Related>,
    pub help: Option<String>,
    pub fixes: Vec<Fix>,
    pub detail: Detail,
}

impl Diagnostic {
    /// The data of the diagnostic, to move fields out of it.
    pub fn into_data(self) -> DiagnosticData {
        *self.0
    }
}

impl std::ops::Deref for Diagnostic {
    type Target = DiagnosticData;
    fn deref(&self) -> &DiagnosticData {
        &self.0
    }
}

impl std::ops::DerefMut for Diagnostic {
    fn deref_mut(&mut self) -> &mut DiagnosticData {
        &mut self.0
    }
}

impl Diagnostic {
    pub fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self(Box::new(DiagnosticData {
            severity: Severity::Error,
            code,
            message: message.into(),
            span,
            kind: None,
            related: Vec::new(),
            help: None,
            fixes: Vec::new(),
            detail: Detail::default(),
        }))
    }

    pub fn warning(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        let mut diagnostic = Self::new(code, message, span);
        diagnostic.severity = Severity::Warning;
        diagnostic
    }

    pub fn with_kind(mut self, kind: &'static str) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn related(mut self, role: RelatedRole, message: impl Into<String>, span: Span) -> Self {
        self.add_related(role, message, span);
        self
    }

    pub fn add_related(&mut self, role: RelatedRole, message: impl Into<String>, span: Span) {
        if self.related.len() < MAX_RELATED {
            self.related.push(Related {
                role,
                message: message.into(),
                span,
            });
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn fix(
        mut self,
        title: impl Into<String>,
        applicability: Applicability,
        edits: Vec<Edit>,
    ) -> Self {
        if self.fixes.len() < MAX_FIXES && !edits.is_empty() && edits.len() <= MAX_EDITS {
            self.fixes.push(Fix {
                title: title.into(),
                applicability,
                edits,
            });
        }
        self
    }

    pub fn expected_found(mut self, expected: impl Into<String>, found: impl Into<String>) -> Self {
        self.detail.expected = Some(expected.into());
        self.detail.found = Some(found.into());
        self
    }

    pub fn with_candidates(mut self, mut candidates: Vec<Candidate>) -> Self {
        candidates.truncate(MAX_CANDIDATES);
        self.detail.candidates = candidates;
        self
    }

    pub fn with_scope(mut self, mut scope: Vec<ScopeEntry>) -> Self {
        scope.truncate(MAX_SCOPE);
        self.detail.scope = scope;
        self
    }
}
```

### F-2: `src/diagnostic.rs` の出力（Phase 1 の手順 5・6）

HEAD の `render`・`render_with_severity`・`json`・`json_with_severity` の本体を、次の `render_with`・`json_with` に移し、HEAD の 4 つは空の `lookup`（`&|_| None`）で委譲する。`write_quote`・`span_json` は HEAD の `render_with_severity`・`json_with_severity` の本体から切り出した関数（出力は同じ）。

```rust
impl Diagnostic {
    pub fn render(&self, path: &str, source: &str) -> String {
        self.render_with_severity(self.severity.as_str(), path, source)
    }

    /// Renders the diagnostic as `path:line:column: severity[code]: message`.
    pub fn render_with_severity(&self, severity: &str, path: &str, source: &str) -> String {
        self.render_with(severity, path, source, &|_| None)
    }

    /// Calls `use_source` with the path and text of the source that `span` is in: the primary
    /// source for a span without a source or in the primary source, `lookup` for any other.
    /// `None` when `lookup` does not know the source.
    fn with_source<T>(
        &self,
        span: Span,
        path: &str,
        source: &str,
        lookup: &SourceLookup<'_>,
        use_source: impl FnOnce(&str, &str) -> T,
    ) -> Option<T> {
        match span.source {
            None => Some(use_source(path, source)),
            Some(_) if span.source == self.span.source => Some(use_source(path, source)),
            Some(index) => lookup(index).map(|(path, source)| use_source(path, source)),
        }
    }

    /// `render_with_severity` plus the related locations, the help, and the fixes. A related
    /// location in a source that `lookup` does not know, and a fix with an edit in one, are left out.
    pub fn render_with(
        &self,
        severity: &str,
        path: &str,
        source: &str,
        lookup: &SourceLookup<'_>,
    ) -> String {
        let (line, column) = location(source, self.span.start);
        let mut output = format!(
            "{path}:{line}:{column}: {severity}[{}]: {}",
            self.code, self.message
        );
        write_quote(&mut output, source, line, column);
        for related in &self.related {
            let note = self.with_source(related.span, path, source, lookup, |path, source| {
                let (line, column) = location(source, related.span.start);
                let mut note = format!(
                    "\n  note: {}\n    --> {path}:{line}:{column}",
                    related.message
                );
                write_quote(&mut note, source, line, column);
                note
            });
            output.extend(note);
        }
        if let Some(help) = &self.help {
            let _ = write!(output, "\n  help: {help}");
        }
        for fix in &self.fixes {
            if self.fix_resolves(fix, path, source, lookup) {
                let _ = write!(
                    output,
                    "\n  fix ({}): {}",
                    fix.applicability.as_str(),
                    fix.title
                );
            }
        }
        output
    }

    fn fix_resolves(&self, fix: &Fix, path: &str, source: &str, lookup: &SourceLookup<'_>) -> bool {
        fix.edits.iter().all(|edit| {
            self.with_source(edit.span, path, source, lookup, |_, _| ())
                .is_some()
        })
    }

    pub fn json(&self, path: &str, source: &str) -> String {
        self.json_with_severity(self.severity.as_str(), path, source)
    }

    /// Renders the diagnostic as one JSON object with the given severity.
    pub fn json_with_severity(&self, severity: &str, path: &str, source: &str) -> String {
        self.json_with(severity, path, source, &|_| None)
    }

    /// `json_with_severity` plus the keys that the diagnostic has values for, in the order
    /// `kind`, `related`, `help`, `expected`, `found`, `candidates`, `scope`, `fixes`.
    pub fn json_with(
        &self,
        severity: &str,
        path: &str,
        source: &str,
        lookup: &SourceLookup<'_>,
    ) -> String {
        let mut json = format!(
            "{{\"severity\":{},\"code\":{},\"message\":{},\"path\":{},\"span\":{}",
            json_string(severity),
            json_string(self.code),
            json_string(&self.message),
            json_string(path),
            span_json(source, self.span),
        );
        if let Some(kind) = self.kind {
            let _ = write!(json, ",\"kind\":{}", json_string(kind));
        }
        let related: Vec<String> = self
            .related
            .iter()
            .filter_map(|related| {
                self.with_source(related.span, path, source, lookup, |path, source| {
                    format!(
                        "{{\"role\":{},\"message\":{},\"path\":{},\"span\":{}}}",
                        json_string(related.role.as_str()),
                        json_string(&related.message),
                        json_string(path),
                        span_json(source, related.span),
                    )
                })
            })
            .collect();
        if !related.is_empty() {
            let _ = write!(json, ",\"related\":[{}]", related.join(","));
        }
        if let Some(help) = &self.help {
            let _ = write!(json, ",\"help\":{}", json_string(help));
        }
        if let Some(expected) = &self.detail.expected {
            let _ = write!(json, ",\"expected\":{}", json_string(expected));
        }
        if let Some(found) = &self.detail.found {
            let _ = write!(json, ",\"found\":{}", json_string(found));
        }
        if !self.detail.candidates.is_empty() {
            let candidates: Vec<String> = self
                .detail
                .candidates
                .iter()
                .map(|candidate| {
                    format!(
                        "{{\"name\":{},\"signature\":{},\"how\":{}}}",
                        json_string(&candidate.name),
                        json_string(&candidate.signature),
                        json_string(&candidate.how),
                    )
                })
                .collect();
            let _ = write!(json, ",\"candidates\":[{}]", candidates.join(","));
        }
        if !self.detail.scope.is_empty() {
            let scope: Vec<String> = self
                .detail
                .scope
                .iter()
                .map(|entry| {
                    format!(
                        "{{\"name\":{},\"type\":{}}}",
                        json_string(&entry.name),
                        json_string(&entry.ty),
                    )
                })
                .collect();
            let _ = write!(json, ",\"scope\":[{}]", scope.join(","));
        }
        let fixes: Vec<String> = self
            .fixes
            .iter()
            .filter(|fix| self.fix_resolves(fix, path, source, lookup))
            .map(|fix| {
                let edits: Vec<String> = fix
                    .edits
                    .iter()
                    .filter_map(|edit| {
                        self.with_source(edit.span, path, source, lookup, |path, source| {
                            format!(
                                "{{\"path\":{},\"span\":{},\"text\":{}}}",
                                json_string(path),
                                span_json(source, edit.span),
                                json_string(&edit.text),
                            )
                        })
                    })
                    .collect();
                format!(
                    "{{\"title\":{},\"applicability\":{},\"edits\":[{}]}}",
                    json_string(&fix.title),
                    json_string(fix.applicability.as_str()),
                    edits.join(","),
                )
            })
            .collect();
        if !fixes.is_empty() {
            let _ = write!(json, ",\"fixes\":[{}]", fixes.join(","));
        }
        json.push('}');
        json
    }
}

/// The text with `edits` applied. The edits are byte ranges of `text`, in any order; two
/// insertions at one offset keep the order they were given in. `None` when an edit is out of
/// range, starts or ends inside a character, or overlaps another.
pub fn apply_edits(text: &str, edits: &[&Edit]) -> Option<String> {
    let mut sorted: Vec<&Edit> = edits.to_vec();
    sorted.sort_by_key(|edit| (edit.span.start, edit.span.end));
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    for edit in sorted {
        let (start, end) = (edit.span.start, edit.span.end);
        if start < cursor
            || end < start
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return None;
        }
        output.push_str(&text[cursor..start]);
        output.push_str(&edit.text);
        cursor = end;
    }
    output.push_str(&text[cursor..]);
    Some(output)
}

/// Appends the line of `column` and a caret under it, unless the line is empty.
fn write_quote(output: &mut String, source: &str, line: usize, column: usize) {
    let text = source.lines().nth(line - 1).unwrap_or("");
    if !text.is_empty() {
        let start = column.saturating_sub(41);
        let snippet: String = text.chars().skip(start).take(120).collect();
        let indent = " ".repeat(column - 1 - start);
        let _ = write!(output, "\n  {line} | {snippet}\n    | {indent}^");
    }
}

/// The span object that the JSON diagnostics use: byte offsets, then lines and columns.
fn span_json(source: &str, span: Span) -> String {
    let (line, column) = location(source, span.start);
    let (end_line, end_column) = location(source, span.end);
    format!(
        "{{\"start\":{},\"end\":{},\"line\":{line},\"column\":{column},\
         \"end_line\":{end_line},\"end_column\":{end_column}}}",
        span.start, span.end,
    )
}
```

### F-3: `src/ownership.rs` のデータの変更と `E1012`・`E1014` の可変性（Phase 2 の手順 9〜12）

```rust
// Loan: 末尾に追加
    /// The expression that created the loan, for the diagnostics that name it.
    span: Span,

// State: `moved` の型を変える
    /// The places that were moved, with the span of the move that the diagnostics point at.
    moved: BTreeMap<Place, Span>,

// Checker: `module` の次に `body`、末尾に `later_use_budget` を足す
    /// The body that is checked, searched for the later uses of a borrow when a diagnostic names them.
    body: &'a TypedExpr,
    // ...
    /// How many more expressions `later_use` may look at in this function.
    later_use_budget: usize,

/// How many expressions the diagnostics of one function may look at, in all, to find where a
/// borrow is used later; past it they leave `BorrowUsedHere` out.
const LATER_USE_NODES: usize = 1 << 20;

// check_body の `Checker { .. }` に: `body,`（`module,` の次）と `later_use_budget: LATER_USE_NODES,`（末尾）

    fn loan(
        &mut self,
        place: Place,
        mutable: bool,
        mut parents: BTreeSet<usize>,
        span: Span,
    ) -> usize {
        if let Some((_, value)) = self.state.locals.get(&place.root) {
            parents.extend(&value.loans);
        }
        let id = self.loans.len();
        self.loans.push(Loan {
            place,
            mutable,
            parents,
            view: None,
            span,
        });
        id
    }

    fn merge(&mut self, other: &State<'a>) {
        for (place, span) in &other.moved {
            self.state
                .moved
                .entry(place.clone())
                .and_modify(|kept| {
                    if (span.start, span.end) > (kept.start, kept.end) {
                        *kept = *span;
                    }
                })
                .or_insert(*span);
        }
        self.state.generic_moves.extend(other.generic_moves.clone());
        // ... 残りは HEAD のまま
    }
```

`Checker::access` の 3 か所（`fn access(&mut self, place, via, usage, span)` の中。ほかは HEAD のまま）。1 つ目は `usage != Use::Write && self.state.moved.iter().any(..)` の `if` ブロック全体の置き換え:

```rust
        if usage != Use::Write
            && let Some((moved, origin)) = self
                .state
                .moved
                .iter()
                .filter(|(moved, _)| moved.overlaps(place))
                .max_by_key(|(_, origin)| (origin.start, origin.end))
                .map(|(moved, origin)| (moved.clone(), *origin))
        {
            let name = self
                .state
                .locals
                .get(&place.root)
                .map_or("value", |(local, _)| local.name.as_str())
                .to_owned();
            let message = match (
                moved.fields.is_empty(),
                (origin.start, origin.end) >= (span.start, span.end),
            ) {
                (true, false) => "value moved here",
                (false, false) => "part of the value moved here",
                (true, true) => "value moved here, in the previous iteration of the loop",
                (false, true) => {
                    "part of the value moved here, in the previous iteration of the loop"
                }
            };
            let mut diagnostic = error(
                "E1012",
                format!("use of moved or partially moved value '{name}'"),
                span,
            )
            .with_help(if moved.fields.is_empty() {
                format!("borrow the value with 'ref {name}' or move it last")
            } else {
                format!(
                    "borrow the field with 'ref' instead of moving it out of '{name}', or move it last"
                )
            });
            if origin.end > origin.start {
                diagnostic = diagnostic.related(RelatedRole::MovedHere, message, origin);
            }
            return self.report(place, diagnostic);
        }
```

2 つ目は `if !mutable || !unmarked_fields(place).is_empty() {` の中の `return self.report(place, error("E1014", "mutable access requires ...", span));` の置き換え（`error(..)` のメッセージは HEAD のまま）:

```rust
                let mut diagnostic = error(
                    "E1014",
                    "mutable access requires 'let mut' or an exclusive reference ('ref mut' or '&mut'); record fields, array elements, and list elements are immutable",
                    span,
                );
                let declared = self
                    .state
                    .locals
                    .get(&place.root)
                    .map(|(local, _)| *local)
                    .filter(|local| {
                        via.is_empty()
                            && !local.mutable
                            && unmarked_fields(place).is_empty()
                            && local.span.end > local.span.start
                            && !local.name.starts_with('$')
                    });
                diagnostic = match declared {
                    Some(local) => diagnostic
                        .related(
                            RelatedRole::DeclaredHere,
                            format!("'{}' is declared here without 'mut'", local.name),
                            local.span,
                        )
                        .with_help(format!(
                            "declare '{}' with 'let mut', or take it by 'ref mut'",
                            local.name
                        )),
                    None => diagnostic.with_help(
                        "write through a 'let mut' variable that holds the whole value, or take it by 'ref mut'",
                    ),
                };
                return self.report(place, diagnostic);
```

3 つ目は衝突の `E1014`（`for id in self.active() { .. }` の中の `return self.report(place, error("E1014", "access conflicts ..", span));`）の置き換え。**同じ根が報告済みなら診断を作らない**（`conflict` は走査をするので、作ってから捨てると同じ根への衝突が多い関数で遅くなる）:

```rust
                if self.reported.contains(&place.root) {
                    return Ok(());
                }
                let diagnostic = self.conflict(id, span);
                return self.report(place, diagnostic);
```

### F-4: `E1014` の衝突 `conflict`・`holders`・`later_use`（Phase 2 の手順 11）

`access` の次に置く。`find_later_use` は `uses` の前の自由関数。

```rust
    /// The `E1014` of an access at `span` that conflicts with the live loan `loan`: where the loan
    /// was taken, and where the reference that keeps it alive is used later.
    #[cold]
    #[inline(never)]
    fn conflict(&mut self, loan: usize, span: Span) -> Diagnostic {
        let mut diagnostic = error(
            "E1014",
            "access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively",
            span,
        );
        let created = &self.loans[loan];
        if !self.external.contains(&created.place.root)
            && created.span.end > created.span.start
            && created.span != span
        {
            diagnostic = diagnostic.related(
                RelatedRole::BorrowedHere,
                if created.mutable {
                    "value mutably borrowed here"
                } else {
                    "value borrowed here"
                },
                created.span,
            );
        }
        let holders = self.holders(loan);
        let holder = holders
            .iter()
            .filter_map(|id| self.state.locals.get(id))
            .map(|(local, _)| local.name.as_str())
            .find(|name| !name.starts_with('$'));
        if let Some((used, repeated)) = self.later_use(&holders, span) {
            diagnostic = diagnostic.related(
                RelatedRole::BorrowUsedHere,
                if repeated {
                    "the borrow is used here, in the next iteration of the loop, so it is still alive"
                } else {
                    "the borrow is used here, so it is still alive"
                },
                used,
            );
        }
        match holder {
            Some(name) => diagnostic.with_help(format!(
                "end the borrow before this access: stop using '{name}' first, or move this access after its last use"
            )),
            None => diagnostic.with_help(
                "end the borrow before this access: evaluate this access before the borrow is taken, or borrow a copy",
            ),
        }
    }

    /// The locals whose values keep `loan` alive, directly or through the parents of a loan they hold.
    fn holders(&self, loan: usize) -> BTreeSet<usize> {
        let mut holders = BTreeSet::new();
        for (id, (_, value)) in &self.state.locals {
            let mut pending: Vec<usize> = value.loans.iter().copied().collect();
            let mut seen = BTreeSet::new();
            while let Some(current) = pending.pop() {
                if !seen.insert(current) {
                    continue;
                }
                if current == loan {
                    holders.insert(*id);
                    break;
                }
                pending.extend(&self.loans[current].parents);
            }
        }
        holders
    }

    /// The first use of a holder after `after` in source order, else (`true`) the first use before it.
    fn later_use(&mut self, holders: &BTreeSet<usize>, after: Span) -> Option<(Span, bool)> {
        find_later_use(self.body, holders, after, &mut self.later_use_budget)
    }
```

```rust
/// The first use of one of `holders` after `after` in source order, else (`true`) the first use
/// before it. Looks at no more than `*budget` expressions, and `None` once it runs out.
fn find_later_use(
    body: &TypedExpr,
    holders: &BTreeSet<usize>,
    after: Span,
    budget: &mut usize,
) -> Option<(Span, bool)> {
    if holders.is_empty() {
        return None;
    }
    let (mut later, mut earlier): (Option<Span>, Option<Span>) = (None, None);
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let used = match &expression.kind {
            E::Local(id) => holders.contains(id),
            E::Lambda { captures, .. } => captures.iter().any(|local| holders.contains(&local.id)),
            _ => false,
        };
        let span = expression.span;
        if used && span.source == after.source && span.end > span.start {
            let slot = if span.start >= after.end {
                &mut later
            } else if span.end <= after.start {
                &mut earlier
            } else {
                &mut None
            };
            if slot.is_none_or(|kept| span.start < kept.start) {
                *slot = Some(span);
            }
        }
        if !used {
            expression
                .try_for_each_child(&mut |child| {
                    pending.push(child);
                    Ok(())
                })
                .expect("searching uses cannot fail");
        }
    }
    later
        .map(|span| (span, false))
        .or(earlier.map(|span| (span, true)))
}
```

単体テスト（`src/ownership.rs` の末尾。既存の `allocation_tests` の次。`uses` は HEAD の関数）:

```rust
#[cfg(test)]
mod later_use_tests {
    use super::*;

    #[test]
    fn finds_the_use_after_and_before_a_span_and_honors_the_budget() {
        let source = "def f :: i64 -> i64 = \\x -> x + x";
        let module = crate::analyze(source).unwrap();
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "f")
            .unwrap();
        let mut holders = BTreeSet::new();
        uses(&function.body, &mut holders);
        let at = |start: usize, end: usize| Span::new(start, end).in_source(0);
        let first = source.find("x + x").unwrap();
        let find = |after: Span, holders: &BTreeSet<usize>, mut budget: usize| {
            find_later_use(&function.body, holders, after, &mut budget)
        };
        let end = source.len();
        assert_eq!(
            find(at(0, 0), &holders, LATER_USE_NODES),
            Some((at(first, first + 1), false))
        );
        assert_eq!(
            find(at(end, end), &holders, LATER_USE_NODES),
            Some((at(first, first + 1), true))
        );
        assert_eq!(find(at(0, 0), &holders, 0), None);
        assert_eq!(find(at(0, 0), &BTreeSet::new(), LATER_USE_NODES), None);
    }
}
```

### F-5: `E1013` の `escape` と 4 か所の呼び出し（Phase 2 の手順 12）

`escape` は `conflict` の前に置く。`owner_message` の `{name}` を所有者の名前に置き換える。

```rust
    /// Names where the owner of `loan` is declared (`owner_message`, with `{name}` for its name)
    /// and where the loan was taken, in an `E1013` that says the borrow outlives its owner.
    #[cold]
    #[inline(never)]
    fn escape(
        &self,
        loan: usize,
        primary: Span,
        owner_message: &str,
        mut diagnostic: Diagnostic,
    ) -> Diagnostic {
        let loan = &self.loans[loan];
        if let Some(owner) = self.declarations.get(&loan.place.root)
            && owner.span.end > owner.span.start
            && !owner.name.starts_with('$')
        {
            diagnostic.add_related(
                RelatedRole::DeclaredHere,
                owner_message.replace("{name}", &owner.name),
                owner.span,
            );
        }
        if loan.span.end > loan.span.start && loan.span != primary {
            diagnostic.add_related(RelatedRole::BorrowedHere, "value borrowed here", loan.span);
        }
        diagnostic
    }
```

呼び出しの 4 か所（HEAD の `return Err(error("E1013", .., ..))` を置き換える）。1. `check_result`（`task` でないとき）:

```rust
                let diagnostic = error(
                    "E1013",
                    if task {
                        "task results cannot retain borrowed values"
                    } else {
                        "cannot return a reference to a local value"
                    },
                    body.span,
                );
                return Err(if task {
                    diagnostic
                } else {
                    self.escape(
                        *id,
                        body.span,
                        "'{name}' is declared here and is dropped when the function returns",
                        diagnostic,
                    )
                    .with_help(
                        "return the value itself, or return a reference that comes from a reference parameter",
                    )
                });
```

2. `eval_block`（ブロックの結果が借用を持ち出す。`for id in &result.loans { if ids.contains(..) && !self.reported.contains(..) { .. } }`）:

```rust
                return Err(self
                    .escape(
                        *id,
                        tail.span,
                        "'{name}' is declared here and dropped at the end of this block",
                        error(
                            "E1013",
                            "borrowed value does not live long enough to leave this block",
                            tail.span,
                        ),
                    )
                    .with_help(
                        "return the value itself instead of a reference to it, or declare it in an outer scope",
                    ));
```

3. `eval_block`（外の変数が借用を持つ。`for (id, (_, value)) in &self.state.locals { .. }` の条件を `any` から `find` に変える）:

```rust
        for (id, (_, value)) in &self.state.locals {
            if !ids.contains(id)
                && let Some(loan) = value.loans.iter().find(|loan| {
                    ids.contains(&self.loans[**loan].place.root)
                        && !self.reported.contains(&self.loans[**loan].place.root)
                })
            {
                return Err(self
                    .escape(
                        *loan,
                        expression.span,
                        "'{name}' is declared here and dropped at the end of this block",
                        error(
                            "E1013",
                            "assignment lets a reference outlive its owner",
                            expression.span,
                        ),
                    )
                    .with_help(
                        "declare the owner before the variable that keeps the reference, in the same block or an outer one",
                    ));
            }
        }
```

4. `src/ownership_control.rs` の `finish_control_scope`（`escaping` は `any` を `find` に変えたもの）:

```rust
        let escaping = value
            .loans
            .iter()
            .find(|id| ids.contains(&self.loans[**id].place.root))
            .or_else(|| {
                self.state
                    .locals
                    .iter()
                    .filter(|(id, _)| !ids.contains(id))
                    .flat_map(|(_, (_, value))| value.loans.iter())
                    .find(|loan| ids.contains(&self.loans[**loan].place.root))
            });
        if let Some(loan) = escaping {
            return Err(self
                .escape(
                    *loan,
                    span,
                    "'{name}' is bound here and ends with this arm or iteration",
                    error(
                        "E1013",
                        "a borrowed value cannot outlive its pattern or iteration binding",
                        span,
                    ),
                )
                .with_help(
                    "move or copy the value out of the pattern or loop instead of keeping a reference to it",
                ));
        }
```

`ownership_control.rs` のそのほかの変更: `loop_summary` は `self.state.moved.keys().cloned().collect()`（場所の集合だけ。span を含めない）、`view` は `Loan { .., span: projection.span }`、`protect` は `self.loan(place, false, via, local.span)`、`retain(|place| ..)` 5 か所は `retain(|place, _| ..)`。

### F-6: 型・名前・呼び出し（Phase 3 の手順 15〜17）

`src/polymorph.rs`。`mismatch` は `impl Inference` の前に置く。`unify` の 3 か所（`FixedArray` の長さの書き換え、`Record`／`Union` の型引数の書き換え、最後の `Err`）は `mismatch` を通す。書き換えのクロージャは `mut error` が要らなくなる（`|error|` にする。残すと `-D warnings` で失敗する）。

```rust
/// The `E1003` of a type mismatch; `expected` and `found` are the displayed types.
fn mismatch(expected: String, found: String, span: Span) -> Diagnostic {
    Diagnostic::new("E1003", format!("expected {expected}, found {found}"), span)
        .expected_found(expected, found)
}
```

```rust
            (Type::FixedArray(a, n), Type::FixedArray(b, m)) => {
                self.unify(n, m, types, span).map_err(|error| {
                    if error.code == "E1003" {
                        return mismatch(
                            self.resolve(&expected).display(types),
                            self.resolve(&actual).display(types),
                            error.span,
                        );
                    }
                    error
                })?;
                return self.unify(a, b, types, span);
            }
```

```rust
                for (a, b) in a.iter().zip(b) {
                    self.unify(a, b, types, span).map_err(|error| {
                        // Report the whole type when both sides were known before unification;
                        // partially inferred arguments would otherwise show intermediate bindings.
                        if error.code == "E1003" && !inferred(&actual) && !inferred(&expected) {
                            return mismatch(
                                expected.display(types),
                                actual.display(types),
                                error.span,
                            );
                        }
                        error
                    })?;
                }
```

```rust
        Err(mismatch(
            expected.display(types),
            actual.display(types),
            span,
        ))
```

`call_signature`（`src/polymorph.rs`。`impl Checker` の中）の 3 つの `Err`:

```rust
        if self.inference.is_numeric_literal(ty) {
            return Err(
                Diagnostic::new("E1005", "only a function value can be called", span).with_help(
                    "a number is not a function; remove the argument(s) or call a function",
                ),
            );
        }
```

```rust
            Type::Function(parameters, _) => {
                let error = Diagnostic::new(
                    "E1006",
                    format!(
                        "cannot apply {count} arguments to a function accepting {} arguments",
                        parameters.len()
                    ),
                    span,
                );
                // A call with no arguments reaches here too, and has nothing to remove.
                Err(if count > parameters.len() {
                    error.with_help(format!(
                        "remove the {} extra argument(s)",
                        count - parameters.len()
                    ))
                } else {
                    error
                })
            }
```

```rust
            other => Err(Diagnostic::new(
                "E1005",
                "only a function value can be called",
                span,
            )
            .with_help(format!(
                "a value of type {} is not a function; remove the argument(s) or call a function",
                other.display(&self.types)
            ))),
```

`src/check.rs`。型（`Names` の前）と `Names` の先頭のフィールド:

```rust
/// Where a function is declared: its name and the spans of its parameters' types, for the
/// notes of diagnostics that point at the signature.
#[derive(Clone, Default)]
struct DeclaredFunction {
    name: String,
    parameters: Vec<Span>,
}

// Names の先頭のフィールド
    /// By function id; the declarations of tests and benches stay empty.
    declared: Vec<DeclaredFunction>,
```

`check_modules_collect` の関数の id ごとのループ（`let mut signatures = Vec::new();` の次の行と、ループの中の `if test_functions.contains_key(&id) || bench_functions.contains_key(&id) { continue; }` の次）:

```rust
    names.declared = vec![DeclaredFunction::default(); function_declarations.len()];
```

```rust
        names.declared[id] = DeclaredFunction {
            name: function.name.text.clone(),
            parameters: function
                .parameters
                .iter()
                .map(|parameter| parameter.ty.span)
                .collect(),
        };
```

`impl<'a> Checker<'a>` に足す 4 つの関数（`same` の前）:

```rust
    /// Adds `ExpectedBecause` to the `E1003` mismatches that checking one expression reported, in
    /// `result` and in `self.recovered[from..]`, when they are at one of `tails`: the expression
    /// itself or a position that its expected type is passed down to.
    fn expected_because<T>(
        &mut self,
        result: Result<T, Diagnostic>,
        from: usize,
        tails: &[Span],
        message: &str,
        because: Span,
    ) -> Result<T, Diagnostic> {
        let note = |diagnostic: &mut Diagnostic| {
            if diagnostic.code == "E1003"
                && diagnostic.detail.expected.is_some()
                && diagnostic.related.is_empty()
                && because.end > because.start
                && tails.contains(&diagnostic.span)
            {
                diagnostic.add_related(RelatedRole::ExpectedBecause, message, because);
            }
        };
        for diagnostic in self.recovered.iter_mut().skip(from) {
            note(diagnostic);
        }
        result.map_err(|mut diagnostic| {
            note(&mut diagnostic);
            diagnostic
        })
    }

    /// `parameter {n} of '{name}'` and the span of that parameter's declared type, when `callee`
    /// is a named function.
    fn declared_parameter(&self, callee: &TypedExpr, index: usize) -> Option<(String, Span)> {
        let (TypedExprKind::Function(FunctionRef::User(id))
        | TypedExprKind::GenericFunction(id, _)) = &callee.kind
        else {
            return None;
        };
        let declared = self.names.declared.get(*id)?;
        let span = *declared.parameters.get(index)?;
        (!declared.name.starts_with('$')).then(|| {
            (
                format!("parameter {} of '{}'", index + 1, declared.name),
                span,
            )
        })
    }

    /// An `E1003` for a call whose value is a function because it left out trailing arguments.
    #[cold]
    #[inline(never)]
    fn partial_application(&self, mut error: Diagnostic, result: &Type, hint: &Type) -> Diagnostic {
        if error.code == "E1003"
            && let Type::Function(missing, _) = self.inference.resolve(result)
            && !matches!(
                self.inference.resolve(hint),
                Type::Function(..) | Type::Infer(_) | Type::Error
            )
        {
            error.help = Some(format!(
                "a call with too few arguments evaluates to a function; add the missing argument(s) ({} more needed)",
                missing.len()
            ));
        }
        error
    }

    /// The locals in scope, innermost first, at most `MAX_SCOPE`.
    fn scope_entries(&self) -> Vec<ScopeEntry> {
        let mut seen = BTreeSet::new();
        let mut entries = Vec::new();
        for scope in self.scopes.iter().rev() {
            for (name, local) in scope {
                if name == "_"
                    || name.starts_with('$')
                    || local.provenance != Provenance::User
                    || !seen.insert(name.clone())
                {
                    continue;
                }
                entries.push(ScopeEntry {
                    name: name.clone(),
                    ty: self.inference.resolve(&local.ty).display(&self.types),
                });
            }
        }
        entries.truncate(crate::diagnostic::MAX_SCOPE);
        entries
    }
```

自由関数（`duplicate_of` の次に置く）:

```rust
/// The spans of the positions that an expected type is passed down to: the expression, and the
/// result of a block, the branches of an `if`, and the arms of a `match` in it, recursively.
fn tail_spans(expression: &Expr) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        spans.push(expression.span);
        match &expression.kind {
            ExprKind::Block { result, .. } => pending.push(result),
            ExprKind::If {
                then_branch,
                else_branch,
                ..
            } => pending.extend([&**then_branch, &**else_branch]),
            ExprKind::Match { arms, .. } => pending.extend(arms.iter().map(|arm| &arm.body)),
            _ => {}
        }
    }
    spans
}

/// `help` of an unknown field: the fields that the record has, at most 16.
fn field_names_help(record: &str, fields: &[(String, Type)]) -> String {
    let shown: Vec<&str> = fields
        .iter()
        .take(16)
        .map(|(name, _)| name.as_str())
        .collect();
    let more = if fields.len() > 16 { ", ..." } else { "" };
    format!("the fields of '{record}' are: {}{more}", shown.join(", "))
}
```

呼び出し側（5 か所）。1. `block_bindings`（`let` の注釈。`let value = self.expression(&binding.value, annotation.as_ref())?;` の置き換え）:

```rust
            let from = self.recovered.len();
            let value = self.expression(&binding.value, annotation.as_ref());
            let value = match &binding.annotation {
                Some(written) => self.expected_because(
                    value,
                    from,
                    &tail_spans(&binding.value),
                    "the type annotation declares this type",
                    written.span,
                )?,
                None => value?,
            };
```

2. `check_modules_collect` の関数本体の検査（`checker.expression(&function.body, Some(&signature.result))?` の置き換え）:

```rust
                    let from = checker.recovered.len();
                    let body = checker.expression(&function.body, Some(&signature.result));
                    let tails = tail_spans(&function.body);
                    checker.expected_because(
                        body,
                        from,
                        &tails,
                        &format!(
                            "the result type of '{}' is declared here",
                            function.name.text
                        ),
                        function.result.span,
                    )?
```

3. `call_expression` の引数のループ（`_ => self.argument(argument, parameter)?,` の置き換え）:

```rust
                    _ => {
                        let from = self.recovered.len();
                        let checked = self.argument(argument, parameter);
                        match self.declared_parameter(&callee, index) {
                            Some((message, span)) => {
                                let message = format!("{message} is declared here");
                                self.expected_because(
                                    checked,
                                    from,
                                    &tail_spans(argument),
                                    &message,
                                    span,
                                )?
                            }
                            None => checked?,
                        }
                    }
```

4. `call_expression` の `self.same(&result, &hint, expression.span)?;`（部分適用）の置き換え:

```rust
            self.same(&result, &hint, expression.span)
                .map_err(|error| self.partial_application(error, &result, &hint))?;
```

5. `Checker::name` の最後の `Err(Diagnostic::new("E1002", ..))` の置き換えと、`E1007` の 3 か所（`field_access`・`record_literal`・`record_update`。`Diagnostic::new("E1007", ..)` の直後に `.with_help(..)` を足す。`record_literal`・`record_update` は `field_names_help(&record.name, &record.fields)`、`field_access` は次の形）:

```rust
        let scope = self.scope_entries();
        let names: Vec<&str> = scope
            .iter()
            .take(8)
            .map(|entry| entry.name.as_str())
            .collect();
        let help = if names.is_empty() {
            "check the spelling, or define the name with 'let' or 'def'".to_owned()
        } else {
            format!(
                "check the spelling, or define the name with 'let' or 'def'; names in scope: {}",
                names.join(", ")
            )
        };
        Err(
            Diagnostic::new("E1002", format!("unknown value '{}'", name.text), name.span)
                .with_help(help)
                .with_scope(scope),
        )
```

```rust
                        Diagnostic::new(
                            "E1007",
                            format!("unknown field '{}'", field.text),
                            field.span,
                        )
                        .with_help(field_names_help(
                            &self.types.records[*id].name,
                            &self.types.records[*id].fields,
                        ))
```

### F-7: `E1021` の help と fix、重複の `FirstDefinedHere`（Phase 3 の手順 18・19）

`src/exhaustiveness.rs`。`MatchCoverage` に `explicit` を足し、`check_coverage` の `return Err(Diagnostic::new("E1021", ..))` を置き換え、`writable` を足す（`witness` の隣）。`exhaustiveness.rs` は `use` を持たず親の `src/check.rs` の `use` を受けるので、`Applicability`・`Edit` は `src/check.rs` の `use crate::diagnostic::{..}` に足す（F-6 の `RelatedRole`・`ScopeEntry` も同じ行。`src/ownership.rs` と `src/parser.rs` は `RelatedRole` を足す）。

```rust
/// The arms of one explicit match or function guard.
pub(super) struct MatchCoverage {
    pub(super) span: Span,
    pub(super) arms: Vec<CoverageArm>,
    /// An explicit `match ... with`, whose end can take one more arm.
    pub(super) explicit: bool,
}
```

```rust
            if let Some(witness) = search.run(rows, push(wild, None), true)? {
                let missing = self.witness(&witness, coverage.span);
                let mut error = Diagnostic::new(
                    "E1021",
                    format!("match is not exhaustive; missing: {missing}"),
                    coverage.span,
                )
                .with_help(format!(
                    "add an arm for '{missing}', or end the match with a wildcard arm '| _ -> ...'"
                ));
                if coverage.explicit && Self::writable(&witness) {
                    let end = Span {
                        start: coverage.span.end,
                        ..coverage.span
                    };
                    error = error.fix(
                        format!("Add the arm '| {missing} -> unreachable ()'"),
                        Applicability::Maybe,
                        vec![Edit {
                            span: end,
                            text: format!(" | {missing} -> unreachable ()"),
                        }],
                    );
                }
                return Err(error);
            }
```

```rust
    /// Whether a witness is printed as a pattern that an arm accepts as written: wildcards,
    /// booleans, `()`, union cases, and tuples of those.
    fn writable(witness: &Witness) -> bool {
        match witness {
            Witness::Wild => true,
            Witness::Ctor(Constructor::Bool(_) | Constructor::Unit, _) => true,
            Witness::Ctor(Constructor::Union { .. } | Constructor::Tuple(_), arguments) => {
                arguments.iter().all(Self::writable)
            }
            Witness::Ctor(
                Constructor::Record { .. }
                | Constructor::ArrayLen(_)
                | Constructor::ListNil
                | Constructor::ListCons
                | Constructor::Literal(_),
                _,
            ) => false,
        }
    }
```

`src/control.rs`（`match_value` の引数 `checked: bool` を `coverage: Option<MatchOrigin>` に。呼び出しは 2 か所と `src/exceptions.rs` の 1 か所）:

```rust
        coverage: Option<MatchOrigin>,
    ) -> Result<TypedExpr, Diagnostic> {
        if matched.ty.contains_error() {
            return Ok(TypedExpr::error(span));
        }
        let slot = coverage.map(|origin| {
            self.coverage.push(MatchCoverage {
                span,
                arms: Vec::new(),
                explicit: origin == MatchOrigin::Explicit,
            });
            self.coverage.len() - 1
        });
```

呼び出し側: `control.rs` の 1 つ目（`while`／`for` の内部で `false` を渡していた）は `None`、2 つ目（`origin.checks_coverage()` を渡していた）は `origin.checks_coverage().then_some(*origin)`、`exceptions.rs` の `self.match_value(caught, &handled.arms, error_hint, span, false)?` は `None`。

重複の `FirstDefinedHere`。`src/parser.rs`（`use crate::diagnostic::{..}` に `RelatedRole` を足す。`let mut definitions = Vec::new();` は `let mut definitions: Vec<Definition> = Vec::new();`）:

```rust
/// `E1001` for a second definition or signature of a function, with the first one when the
/// parser has seen it.
fn duplicate_function(message: &str, name: &Ident, first: Option<Span>) -> Diagnostic {
    let error = Diagnostic::new("E1001", message, name.span);
    match first {
        Some(first) => error.related(RelatedRole::FirstDefinedHere, "first defined here", first),
        None => error,
    }
}
```

```rust
                        let first = signatures
                            .get(&name.text)
                            .or_else(|| program.externs.iter().find(|external| external.name.text == name.text))
                            .map(|signature| signature.name.span);
                        return Err(duplicate_function("duplicate function signature", &name, first));
```

```rust
                            let first = definitions.iter().find(|definition| definition.name.text == name.text);
                            return Err(duplicate_function("duplicate function definition", &name, first.map(|definition| definition.name.span)));
```

3 つ目の `duplicate function definition`（`fn` の実装。`defined_names.contains(&name.text)` の次）も同じ形で:

```rust
                    let first = definitions.iter().find(|definition| definition.name.text == name.text);
                    return Err(duplicate_function(
                        "duplicate function definition",
                        &name,
                        first.map(|definition| definition.name.span),
                    ));
```

`src/check.rs`（`duplicate_of` は `duplicate` の前）:

```rust
/// `duplicate` with the first declaration of the name, when it is known.
fn duplicate_of(name: &Ident, first: Option<Span>) -> Diagnostic {
    let error = duplicate(name);
    match first {
        Some(first) => error.related(RelatedRole::FirstDefinedHere, "first defined here", first),
        None => error,
    }
}
```

```rust
            let first = record_declarations[..id]
                .iter()
                .find(|(other_module, other)| {
                    other_module == module && other.name.text == record.name.text
                })
                .map(|(_, other)| other.name.span);
            diagnostics.push(duplicate_of(&record.name, first));
```

```rust
                    let first = record.fields.iter().find(|other| {
                        other.name.text == field.name.text && other.name.span != field.name.span
                    });
                    return Err(duplicate_of(
                        &field.name,
                        first.map(|other| other.name.span),
                    ));
```

```rust
                    let first = function
                        .parameters
                        .iter()
                        .find(|other| other.name.text == parameter.name.text);
                    return Err(duplicate_of(
                        &parameter.name,
                        first.map(|other| other.name.span),
                    ));
```

`collect_unions` の `duplicate case`（`E1024`）。`any` を `find` にして、`Diagnostic::new("E1024", ..)` の直後に `.related(..)` を足す:

```rust
                if let Some(first) = union.cases[..case]
                    .iter()
                    .find(|other| other.name.text == name.text)
                {
                    return Err(Diagnostic::new(
                        "E1024",
                        format!(
                            "duplicate case '{}' in union '{}'; give each case a distinct name",
                            name.text, union.name.text
                        ),
                        name.span,
                    )
                    .related(
                        RelatedRole::FirstDefinedHere,
                        "first defined here",
                        first.name.span,
                    ));
                }
```

### F-8: LSP・CLI・REPL（Phase 1 の手順 7 と Phase 4 の手順 20・21）

`src/main.rs` の `print_diagnostics`（先頭の 3 行と、診断 1 件の出力の部分。`omission_note` 以降は HEAD のまま。`print_diagnostic`（ソースのない約 40 か所が使う）は変えない）:

```rust
fn print_diagnostics(errors: &tsuzuri::diagnostic::DiagnosticSet, project: &Project, json: bool) {
    let files: Vec<(String, &str)> = project
        .sources
        .iter()
        .chain(&project.manifests)
        .map(|file| (file.path.to_string_lossy().into_owned(), file.text.as_str()))
        .collect();
    let lookup = |index: usize| files.get(index).map(|(path, text)| (path.as_str(), *text));
    for (index, error) in errors.diagnostics.iter().enumerate() {
        if index != 0 && !json {
            eprintln!();
        }
        let source = project.source_for(error);
        let path = source.path.to_string_lossy();
        let severity = error.severity.as_str();
        eprintln!(
            "{}",
            if json {
                error.json_with(severity, &path, &source.text, &lookup)
            } else {
                error.render_with(severity, &path, &source.text, &lookup)
            }
        );
    }
    // ... `if let Some(note) = errors.omission_note() { .. }` 以降は HEAD のまま
```

`src/repl.rs` の `report`（`Diagnostic { span: .., ..diagnostic.clone() }` の置き換え。REPL は生成した別のプログラムの位置を持つので、`related`・`fixes` を出さない）:

```rust
            .map(|diagnostic| {
                let (path, text, offset) =
                    generated.locate(diagnostic.span, project, input, &listing);
                let mut located = diagnostic.clone();
                located.span = Span::new(offset, offset);
                located.related.clear();
                located.fixes.clear();
                (located, path, text)
            })
```

`src/lsp.rs`。`use crate::diagnostic::{Applicability, Diagnostic, Edit, Fix, Severity, Span, apply_edits};`。`refresh` の `.filter_map(|diagnostic| diagnostic_json(&mappers[id], &source.text, diagnostic))` を `.filter_map(|diagnostic| self.diagnostic_value(&project, &mappers, id, diagnostic))` に。`code_actions` の `W1001` のループのあとに次を足す（`W1001` のループ、`diagnostic_json` は変えない）:

```rust
        for diagnostic in &state.diagnostics {
            let span = diagnostic.span;
            if diagnostic.fixes.is_empty()
                || span.source != Some(source_id)
                || span.start > end
                || start > span.end
            {
                continue;
            }
            for fix in &diagnostic.fixes {
                let Ok(edit) = self.fix_edit(directory, state, diagnostic, fix) else {
                    continue;
                };
                actions.push(json!({
                    "title": fix.title,
                    "kind": "quickfix",
                    "diagnostics": [self.diagnostic_value(&state.project, &state.mappers, source_id, diagnostic)],
                    "isPreferred": fix.applicability == Applicability::Certain,
                    "edit": edit,
                }));
            }
        }
```

`impl Session` に足す 2 つの関数と、自由関数 `diagnostic_data`:

```rust
    /// `diagnostic_json` plus `relatedInformation` (the related locations in the project's own
    /// files) and `data` (the kind and the fixes), each only when there is something to say.
    fn diagnostic_value(
        &self,
        project: &Project,
        mappers: &[PositionMapper],
        source_id: usize,
        diagnostic: &Diagnostic,
    ) -> Option<Value> {
        let source = &project.sources[source_id];
        let mut value = diagnostic_json(&mappers[source_id], &source.text, diagnostic)?;
        let related: Vec<Value> = diagnostic
            .related
            .iter()
            .filter_map(|related| {
                let id = related.span.source.unwrap_or(project.root);
                let file = project
                    .sources
                    .get(id)
                    .filter(|file| file.origin == ModuleOrigin::User)?;
                Some(json!({
                    "location": {
                        "uri": self.uri(&file.path)?,
                        "range": mappers[id].range(&file.text, related.span)?,
                    },
                    "message": related.message,
                }))
            })
            .collect();
        if !related.is_empty() {
            value["relatedInformation"] = json!(related);
        }
        if let Some(data) = diagnostic_data(diagnostic) {
            value["data"] = data;
        }
        Some(value)
    }

    /// The workspace edit of `fix`, when applying it to the project's files and analyzing again
    /// removes `diagnostic` (see `fix_removes`).
    fn fix_edit(
        &self,
        directory: &Path,
        state: &ProjectState,
        diagnostic: &Diagnostic,
        fix: &Fix,
    ) -> Result<Value, String> {
        let mut edited: BTreeMap<usize, Vec<&Edit>> = BTreeMap::new();
        for edit in &fix.edits {
            let id = edit.span.source.unwrap_or(state.project.root);
            if state
                .project
                .sources
                .get(id)
                .is_none_or(|file| file.origin != ModuleOrigin::User)
            {
                return Err("an edit is outside the project's own files".into());
            }
            edited.entry(id).or_default().push(edit);
        }
        let mut overlays: BTreeMap<PathBuf, String> = self
            .buffers
            .iter()
            .map(|(path, buffer)| (path.clone(), buffer.text.clone()))
            .collect();
        for (id, edits) in &edited {
            let file = &state.project.sources[*id];
            let text =
                apply_edits(&file.text, edits).ok_or("the edits overlap or are out of range")?;
            overlays.insert(file.path.clone(), text);
        }
        let project = Project::load_with_overlays(directory, &overlays)
            .map_err(|error| format!("{}: {}", error.diagnostic.code, error.diagnostic.message))?;
        let (_, after) = analyze(&project);
        fix_removes(diagnostic, &state.diagnostics, &after)?;
        let mut changes = serde_json::Map::new();
        for (id, edits) in &edited {
            let file = &state.project.sources[*id];
            let uri = self
                .uri(&file.path)
                .ok_or("cannot address an edited file")?;
            let list: Option<Vec<Value>> = edits
                .iter()
                .map(|edit| {
                    state.mappers[*id]
                        .range(&file.text, edit.span)
                        .map(|range| json!({"range": range, "newText": edit.text}))
                })
                .collect();
            changes.insert(uri, json!(list.ok_or("cannot address an edit")?));
        }
        Ok(json!({"changes": changes}))
    }
```

```rust
/// The `data` of a published diagnostic: its kind and the titles of its fixes.
fn diagnostic_data(diagnostic: &Diagnostic) -> Option<Value> {
    let mut data = serde_json::Map::new();
    if let Some(kind) = diagnostic.kind {
        data.insert("kind".into(), json!(kind));
    }
    if !diagnostic.fixes.is_empty() {
        let fixes: Vec<Value> = diagnostic
            .fixes
            .iter()
            .map(|fix| json!({"title": fix.title, "applicability": fix.applicability.as_str()}))
            .collect();
        data.insert("fixes".into(), json!(fixes));
    }
    (!data.is_empty()).then_some(Value::Object(data))
}

/// Whether the diagnostics `after` a fix of `diagnostic` have one fewer with its code than
/// `before` and none more of any code. Errors always count; warnings count only when
/// `diagnostic` is a warning, because the passes that follow a fixed error may report warnings
/// that were not reached before.
fn fix_removes(
    diagnostic: &Diagnostic,
    before: &[Diagnostic],
    after: &[Diagnostic],
) -> Result<(), String> {
    let counts = |list: &[Diagnostic]| {
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        for item in list.iter().filter(|item| {
            item.severity == Severity::Error || diagnostic.severity == Severity::Warning
        }) {
            *counts.entry(item.code).or_default() += 1;
        }
        counts
    };
    let (before, after) = (counts(before), counts(after));
    let count =
        |counts: &BTreeMap<&'static str, usize>, code| counts.get(code).copied().unwrap_or(0);
    if count(&after, diagnostic.code) >= count(&before, diagnostic.code) {
        return Err("the diagnostic would remain".into());
    }
    if let Some((code, _)) = after.iter().find(|(code, n)| **n > count(&before, code)) {
        return Err(format!("{code} would appear"));
    }
    Ok(())
}
```

`src/lsp.rs` の単体テストを 1 件足す（既存の `mod tests` の `framing_roundtrips_and_rejects_invalid_inputs` の次。`use super::*;` で `Diagnostic`・`Span` が見える）:

```rust
    #[test]
    fn a_fix_must_remove_its_diagnostic_and_add_nothing() {
        let error = |code: &'static str| Diagnostic::new(code, "m", Span::new(0, 0));
        let warning = |code: &'static str| Diagnostic::warning(code, "m", Span::new(0, 0));
        // A fixed error may uncover warnings of later passes.
        assert!(fix_removes(&error("E1021"), &[error("E1021")], &[warning("W1001")]).is_ok());
        assert!(fix_removes(&error("E1021"), &[error("E1021")], &[error("E1021")]).is_err());
        assert!(fix_removes(&error("E1021"), &[error("E1021")], &[error("E1003")]).is_err());
        assert!(fix_removes(&warning("W1001"), &[warning("W1001")], &[]).is_ok());
        assert!(fix_removes(&warning("W1001"), &[warning("W1001")], &[warning("W1001")]).is_err());
        assert!(fix_removes(&warning("W1001"), &[warning("W1001")], &[warning("W1005")]).is_err());
        assert!(fix_removes(&warning("W1001"), &[warning("W1001")], &[error("E1003")]).is_err());
    }
```

LSP の新しいテスト 3 件（`tests/lsp.rs` の末尾。`session`・`scripted`・`range` は同じファイルの既存の補助）:

```rust
const MOVE_TWICE: &str = "def consume :: string -> i64 = \\s -> s.length\n\ndef main_value :: unit -> i64 = \\_ ->\n    let text = \"hello\"\n    let a = consume text\n    let b = consume text\n    a + b\n";
const MISSING_ARM: &str = "union Shape = Circle of f64 | Square of f64 | Triangle of f64\n\ndef area :: Shape -> f64 = \\s ->\n    match s with\n    | Circle r -> r\n    | Square w -> w\n";
const NESTED_MISSING_ARM: &str = "union Shape = Circle of f64 | Square of f64 | Triangle of f64\n\ndef area :: Shape -> Shape -> f64 = \\s t ->\n    match s with\n    | Circle r -> r\n    | Square w -> match t with | Circle q -> q | Square z -> z | Triangle _ -> unreachable ()\n";

#[test]
fn publishes_related_information_and_data() {
    use serde_json::json;
    let (messages, _) = session(&[("Main.tz", MOVE_TWICE)], "utf-16", |uri| {
        vec![
            json!({"id": 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": uri("Main.tz")}, "position": {"line": 0, "character": 4}}}),
        ]
    });
    let published = messages
        .iter()
        .rfind(|message| message["method"] == "textDocument/publishDiagnostics")
        .unwrap();
    let diagnostics = published["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic["code"], "E1012");
    assert_eq!(diagnostic["range"], range(5, 20, 24));
    let related = diagnostic["relatedInformation"].as_array().unwrap();
    assert_eq!(related.len(), 1);
    assert_eq!(related[0]["message"], "value moved here");
    assert_eq!(related[0]["location"]["uri"], published["params"]["uri"]);
    assert_eq!(related[0]["location"]["range"], range(4, 20, 24));
    assert!(diagnostic.get("data").is_none(), "{diagnostic}");
}

#[test]
fn code_action_offers_a_verified_fix() {
    use serde_json::json;
    let responses = scripted(&[("Main.tz", MISSING_ARM)], "utf-16", |uri| {
        vec![
            json!({"id": 1, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": uri("Main.tz")}, "range": range(3, 4, 5), "context": {"diagnostics": [], "only": ["quickfix"]}}}),
        ]
    });
    let actions = responses[0]["result"].as_array().unwrap();
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(
        actions[0]["title"],
        "Add the arm '| Triangle _ -> unreachable ()'"
    );
    assert_eq!(actions[0]["kind"], "quickfix");
    assert_eq!(actions[0]["isPreferred"], false);
    let changes = actions[0]["edit"]["changes"].as_object().unwrap();
    assert_eq!(changes.len(), 1);
    let edits = changes.values().next().unwrap().as_array().unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["newText"], " | Triangle _ -> unreachable ()");
    assert_eq!(
        edits[0]["range"],
        json!({"start": {"line": 5, "character": 19}, "end": {"line": 5, "character": 19}})
    );
    assert_eq!(
        actions[0]["diagnostics"][0]["data"]["fixes"][0]["applicability"],
        "maybe"
    );
}

#[test]
fn code_action_omits_a_fix_that_does_not_verify() {
    use serde_json::json;
    let responses = scripted(&[("Main.tz", NESTED_MISSING_ARM)], "utf-16", |uri| {
        vec![
            json!({"id": 1, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": uri("Main.tz")}, "range": range(3, 4, 5), "context": {"diagnostics": [], "only": ["quickfix"]}}}),
        ]
    });
    assert_eq!(responses[0]["result"], json!([]));
}
```

## 付録 G: `tests/diagnostic_v2.rs`（試作で通った全文。定数の本文は付録 A）

テストの期待値は、このチケットに書いたメッセージの雛形と付録 B の出力から作ったもので、試作に対して 28 件すべて通る。**ファイルを段階ごとに作る**（実装の手順 2・4・8・14）。定数（例: `MOVE1`）は、付録 A の同名の見出し（小文字）のソースの本文そのまま。長いものは `r#"…"#`、1 行で書けるものは `"…\n"` の文字列にしてよい（改行と `\` の扱いを間違えない）。本文に `"#` を含むものはない。パスは比較せず、`ends_with("Main.tz")` と `replace('\\', "/")` で比べる（Windows の区切り）。

### G-1: 手順 2 で作る（HEAD でコンパイルでき、通る。HEAD で確かめ済み）

使う定数: `TYPE1`・`CALL1`・`NAME1`・`MATCH1`・`DUP3`・`DUP5`・`BORROW3`・`LIFE1`・`MOVE1`・`NO1`〜`NO6`・`DUP12`。

```rust
use tsuzuri::analyze_all;
use tsuzuri::diagnostic::{Diagnostic, json_string, location};

// const TYPE1: &str = r#"…"#; ほか（付録 A）

fn errors(source: &str) -> Vec<Diagnostic> {
    analyze_all(source).expect_err(source).diagnostics
}

fn has_extras(_: &Diagnostic) -> bool {
    false
}

fn has_text_extras(_: &Diagnostic) -> bool {
    false
}

/// The text form that HEAD printed, written out here as the reference.
fn old_text(diagnostic: &Diagnostic, path: &str, source: &str) -> String {
    let (line, column) = location(source, diagnostic.span.start);
    let mut output = format!(
        "{path}:{line}:{column}: {}[{}]: {}",
        diagnostic.severity.as_str(),
        diagnostic.code,
        diagnostic.message
    );
    let text = source.lines().nth(line - 1).unwrap_or("");
    if !text.is_empty() {
        let skip = column.saturating_sub(41);
        let snippet: String = text.chars().skip(skip).take(120).collect();
        output.push_str(&format!(
            "\n  {line} | {snippet}\n    | {}^",
            " ".repeat(column - 1 - skip)
        ));
    }
    output
}

/// The JSON form that HEAD printed, written out here as the reference.
fn old_json(diagnostic: &Diagnostic, path: &str, source: &str) -> String {
    let (line, column) = location(source, diagnostic.span.start);
    let (end_line, end_column) = location(source, diagnostic.span.end);
    format!(
        "{{\"severity\":{},\"code\":{},\"message\":{},\"path\":{},\"span\":{{\"start\":{},\"end\":{},\"line\":{line},\"column\":{column},\"end_line\":{end_line},\"end_column\":{end_column}}}}}",
        json_string(diagnostic.severity.as_str()),
        json_string(diagnostic.code),
        json_string(&diagnostic.message),
        json_string(path),
        diagnostic.span.start,
        diagnostic.span.end,
    )
}

#[test]
fn json_without_extras_is_byte_identical() {
    let sources = [
        TYPE1, CALL1, NAME1, MATCH1, DUP3, DUP5, BORROW3, LIFE1, MOVE1, NO1, NO2, NO3, NO4, NO5,
        NO6, DUP12,
    ];
    let mut plain = 0;
    for source in sources {
        for diagnostic in errors(source) {
            let (old_text, old_json) = (
                old_text(&diagnostic, "Main.tz", source),
                old_json(&diagnostic, "Main.tz", source),
            );
            let (text, json) = (
                diagnostic.render("Main.tz", source),
                diagnostic.json("Main.tz", source),
            );
            if has_text_extras(&diagnostic) {
                assert!(text.starts_with(&old_text) && text.len() > old_text.len());
            } else {
                assert_eq!(text, old_text);
            }
            if has_extras(&diagnostic) {
                assert!(json.starts_with(&old_json[..old_json.len() - 1]));
                assert!(json.len() > old_json.len());
            } else {
                plain += 1;
                assert_eq!(json, old_json);
            }
        }
    }
    assert!(plain >= 8, "{plain}");
}
```

### G-2: 手順 4 で入れ替える（型を足したあと）

`use tsuzuri::diagnostic::{Detail, Diagnostic, json_string, location};` にして、`has_extras`・`has_text_extras` を次にする。

```rust
fn has_extras(diagnostic: &Diagnostic) -> bool {
    diagnostic.kind.is_some()
        || has_text_extras(diagnostic)
        || diagnostic.detail != Detail::default()
}

/// The extras that the text form prints: `expected` and `found` are in the JSON only.
fn has_text_extras(diagnostic: &Diagnostic) -> bool {
    !diagnostic.related.is_empty() || diagnostic.help.is_some() || !diagnostic.fixes.is_empty()
}
```

### G-3: 手順 8（Phase 2）で足す

`use` に `std::fs`・`std::process::Command`・`std::time::{SystemTime, UNIX_EPOCH}`、`tsuzuri::diagnostic::{RelatedRole, Span}` を足す。使う定数: `MOVE1`・`LOOP1`・`LOOP2`・`BRANCH1`・`BRANCH2`・`PARTIAL1`・`BORROW1`・`BORROW2`・`BORROW3`・`LIFE1`・`LIFE3`・`ESC1`・`ESC3`・`NO1`〜`NO6`。

```rust
fn first(source: &str, code: &str) -> Diagnostic {
    errors(source)
        .into_iter()
        .find(|diagnostic| diagnostic.code == code)
        .unwrap_or_else(|| panic!("no {code} in {source}"))
}

/// The diagnostic with `code` whose primary span starts on `line`.
fn at_line(source: &str, code: &str, line: usize) -> Diagnostic {
    errors(source)
        .into_iter()
        .find(|diagnostic| diagnostic.code == code && line_of(source, diagnostic.span) == line)
        .unwrap_or_else(|| panic!("no {code} on line {line} in {source}"))
}

fn text_at(source: &str, span: Span) -> &str {
    &source[span.start..span.end]
}

fn line_of(source: &str, span: Span) -> usize {
    source[..span.start].matches('\n').count() + 1
}

/// Runs `tsuzuri check [--json]` on a project made of `files`; returns the exit code and stderr.
fn check_cli(files: &[(&str, &str)], json: bool) -> (i32, String) {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-diagnostic-v2-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    for (name, text) in files {
        fs::write(root.join(name), text).unwrap();
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    command.arg("check").arg(&root);
    if json {
        command.arg("--json");
    }
    let output = command.output().unwrap();
    fs::remove_dir_all(&root).unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn json_lines(stderr: &str) -> Vec<serde_json::Value> {
    stderr
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("{line}")))
        .collect()
}

#[test]
fn cli_json_keeps_old_keys_first() {
    let (code, stderr) = check_cli(&[("Main.tz", MOVE1)], true);
    assert_eq!(code, 1);
    let lines = json_lines(&stderr);
    assert_eq!(lines.len(), 1, "{stderr}");
    let line = stderr.lines().next().unwrap();
    assert!(line.starts_with(
        "{\"severity\":\"error\",\"code\":\"E1012\",\"message\":\"use of moved or partially moved value 'text'\",\"path\":"
    ));
    let keys: Vec<&str> = lines[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        [
            "code", "help", "message", "path", "related", "severity", "span"
        ]
    );
    assert_eq!(lines[0]["related"][0]["role"], "moved_here");
    assert_eq!(lines[0]["related"][0]["span"]["line"], 5);
    assert_eq!(lines[0]["related"][0]["span"]["end_column"], 25);
    assert_eq!(lines[0]["related"][0]["span"].as_object().unwrap().len(), 6);
    // The old keys are first, in the old order, then `related`, then `help`.
    let order = |key: &str| line.find(&format!("\"{key}\":")).unwrap();
    assert!(order("severity") < order("code") && order("code") < order("message"));
    assert!(order("message") < order("path") && order("path") < order("span"));
    assert!(order("span") < order("related") && order("related") < order("help"));
}

#[test]
fn moved_value_names_the_move() {
    let diagnostic = first(MOVE1, "E1012");
    assert_eq!(diagnostic.related.len(), 1);
    let related = &diagnostic.related[0];
    assert_eq!(related.role, RelatedRole::MovedHere);
    assert_eq!(related.message, "value moved here");
    assert_eq!(text_at(MOVE1, related.span), "text");
    assert_eq!(line_of(MOVE1, related.span), 5);
    assert_eq!(line_of(MOVE1, diagnostic.span), 6);
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("borrow the value with 'ref text' or move it last")
    );
}

#[test]
fn moved_in_a_loop_says_previous_iteration() {
    for source in [LOOP1, LOOP2] {
        let diagnostic = first(source, "E1012");
        assert_eq!(diagnostic.related.len(), 1);
        assert_eq!(diagnostic.related[0].role, RelatedRole::MovedHere);
        assert_eq!(
            diagnostic.related[0].message,
            "value moved here, in the previous iteration of the loop"
        );
        assert_eq!(diagnostic.related[0].span, diagnostic.span);
    }
}

#[test]
fn moved_in_branches_names_the_later_move() {
    let diagnostic = first(BRANCH1, "E1012");
    assert_eq!(text_at(BRANCH1, diagnostic.related[0].span), "text");
    assert_eq!(line_of(BRANCH1, diagnostic.related[0].span), 6);
    let diagnostic = first(BRANCH2, "E1012");
    let later = BRANCH2.find("else consume text").unwrap() + "else consume ".len();
    assert_eq!(diagnostic.related[0].span.start, later);
    assert_eq!(text_at(BRANCH2, diagnostic.related[0].span), "text");
}

#[test]
fn partial_move_says_part_of_the_value() {
    let diagnostic = first(PARTIAL1, "E1012");
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(
        diagnostic.related[0].message,
        "part of the value moved here"
    );
    assert_eq!(text_at(PARTIAL1, diagnostic.related[0].span), "pkg.title");
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("borrow the field with 'ref' instead of moving it out of 'pkg', or move it last")
    );
}

#[test]
fn borrow_conflict_names_borrow_and_use() {
    let diagnostic = first(BORROW1, "E1014");
    let roles: Vec<_> = diagnostic
        .related
        .iter()
        .map(|related| related.role)
        .collect();
    assert_eq!(
        roles,
        [RelatedRole::BorrowedHere, RelatedRole::BorrowUsedHere]
    );
    assert_eq!(diagnostic.related[0].message, "value mutably borrowed here");
    assert_eq!(
        text_at(BORROW1, diagnostic.related[0].span),
        "ref mut score"
    );
    assert_eq!(
        diagnostic.related[1].message,
        "the borrow is used here, so it is still alive"
    );
    assert_eq!(text_at(BORROW1, diagnostic.related[1].span), "r1");
    assert_eq!(line_of(BORROW1, diagnostic.related[1].span), 4);
    assert_eq!(
        diagnostic.help.as_deref(),
        Some(
            "end the borrow before this access: stop using 'r1' first, or move this access after its last use"
        )
    );
    let diagnostic = first(BORROW2, "E1014");
    assert_eq!(diagnostic.related[0].message, "value borrowed here");
    assert_eq!(text_at(BORROW2, diagnostic.related[0].span), "ref score");
    assert_eq!(text_at(BORROW2, diagnostic.related[1].span), "view");
    assert_eq!(line_of(BORROW2, diagnostic.related[1].span), 7);
}

#[test]
fn immutable_access_names_the_declaration() {
    let diagnostic = first(BORROW3, "E1014");
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].role, RelatedRole::DeclaredHere);
    assert_eq!(
        diagnostic.related[0].message,
        "'score' is declared here without 'mut'"
    );
    assert_eq!(text_at(BORROW3, diagnostic.related[0].span), "score");
    assert_eq!(line_of(BORROW3, diagnostic.related[0].span), 1);
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("declare 'score' with 'let mut', or take it by 'ref mut'")
    );
}

#[test]
fn escaping_borrow_names_owner_and_borrow() {
    let diagnostic = first(LIFE1, "E1013");
    assert_eq!(
        diagnostic.related.len(),
        1,
        "the borrow is the primary span"
    );
    assert_eq!(diagnostic.related[0].role, RelatedRole::DeclaredHere);
    assert_eq!(
        diagnostic.related[0].message,
        "'local' is declared here and dropped at the end of this block"
    );
    assert_eq!(text_at(LIFE1, diagnostic.related[0].span), "local");
    assert_eq!(
        diagnostic.help.as_deref(),
        Some(
            "return the value itself instead of a reference to it, or declare it in an outer scope"
        )
    );
    let diagnostic = first(LIFE3, "E1013");
    assert_eq!(
        diagnostic.related[0].message,
        "'s' is declared here and is dropped when the function returns"
    );
    assert_eq!(text_at(LIFE3, diagnostic.related[0].span), "s");
    let diagnostic = first(ESC1, "E1013");
    let roles: Vec<_> = diagnostic
        .related
        .iter()
        .map(|related| related.role)
        .collect();
    assert_eq!(
        roles,
        [RelatedRole::DeclaredHere, RelatedRole::BorrowedHere]
    );
    assert_eq!(text_at(ESC1, diagnostic.related[0].span), "y");
    assert_eq!(text_at(ESC1, diagnostic.related[1].span), "ref y");
    let diagnostic = first(ESC3, "E1013");
    assert_eq!(
        diagnostic.related[0].message,
        "'n' is bound here and ends with this arm or iteration"
    );
    assert_eq!(text_at(ESC3, diagnostic.related[1].span), "ref n");
}

#[test]
fn moves_in_loops_still_reach_the_fixpoint() {
    let codes: Vec<_> = errors(LOOP2)
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(codes, ["E1012"]);
}

#[test]
fn ownership_without_related_is_unchanged() {
    let cases = [
        (
            NO1,
            "E1012",
            "cannot move a field or payload out of a value whose type implements Drop; borrow it with 'ref' instead",
        ),
        (
            NO2,
            "E1012",
            "cannot move a non-Copy value out of a reference",
        ),
        (
            NO3,
            "E1013",
            "Mutex.with_lock results must be proven free of borrowed environments",
        ),
        (NO4, "E1013", "a channel item cannot hold a borrow"),
        (
            NO5,
            "E1012",
            "cannot move a non-Copy element out of an array or list; borrow the element instead",
        ),
        (
            NO6,
            "E1012",
            "cannot update a value whose type implements Drop; construct a new value instead",
        ),
    ];
    for (source, code, message) in cases {
        let diagnostic = first(source, code);
        assert!(
            diagnostic.message.starts_with(message),
            "{}",
            diagnostic.message
        );
        assert!(diagnostic.related.is_empty(), "{source}");
        assert!(diagnostic.help.is_none(), "{source}");
    }
}
```

### G-4: 手順 14（Phase 3）で足す

`use` に `tsuzuri::diagnostic::{Edit, ScopeEntry, apply_edits}` を足す。使う定数: `TYPE1`・`CALL1`・`NAME1`・`FIELD1`・`NOFN`・`STD2`・`MISMATCH1`・`MISMATCH2`・`MATCH1`・`FIX2`・`FIX3`・`FIX4`・`ARMS5`・`ARMS6`・`DUP2`・`DUP3`・`DUP5`〜`DUP9`・`DUP12`。2 ファイルの `related_in_another_file_is_rendered` は定数を使わず、中に文字列を書く（付録 A の `multi1`）。

```rust
#[test]
fn related_in_another_file_is_rendered() {
    let lib = "def add :: i64 -> i64 -> i64 = \\a b -> a + b\n";
    let main = "def total :: unit -> i64 = \\_ -> Lib.add 1 \"two\"\n\n0\n";
    let files = [("Lib.tz", lib), ("Main.tz", main)];
    let (_, text) = check_cli(&files, false);
    assert!(
        text.contains("Main.tz:1:44: error[E1003]: expected i64, found string"),
        "{text}"
    );
    assert!(
        text.contains("  note: parameter 2 of 'add' is declared here\n    --> "),
        "{text}"
    );
    assert!(text.contains("Lib.tz:1:19\n  1 | def add :: i64 -> i64 -> i64 = \\a b -> a + b\n    |                   ^"), "{text}");
    let (_, stderr) = check_cli(&files, true);
    let diagnostic = &json_lines(&stderr)[0];
    assert!(diagnostic["path"].as_str().unwrap().ends_with("Main.tz"));
    let related = &diagnostic["related"][0];
    assert_eq!(related["role"], "expected_because");
    assert!(related["path"].as_str().unwrap().ends_with("Lib.tz"));
    assert_eq!(related["span"]["line"], 1);
    assert_eq!(related["span"]["column"], 19);
}

#[test]
fn mismatch_has_expected_and_found() {
    let diagnostic = at_line(TYPE1, "E1003", 1);
    assert_eq!(diagnostic.detail.expected.as_deref(), Some("i64"));
    assert_eq!(diagnostic.detail.found.as_deref(), Some("bool"));
    assert!(diagnostic.related.is_empty());
    let diagnostic = first(MISMATCH1, "E1003");
    assert_eq!(diagnostic.detail.expected.as_deref(), Some("[i64; 2]"));
    assert_eq!(diagnostic.detail.found.as_deref(), Some("[i64; 3]"));
    let diagnostic = first(MISMATCH2, "E1003");
    assert_eq!(diagnostic.detail.expected.as_deref(), Some("Maybe<i64>"));
    assert_eq!(diagnostic.detail.found.as_deref(), Some("Maybe<string>"));
    // The message is the same text as before.
    assert_eq!(
        diagnostic.message,
        "expected Maybe<i64>, found Maybe<string>"
    );
}

#[test]
fn annotation_mismatch_names_the_annotation() {
    let diagnostic = at_line(TYPE1, "E1003", 5);
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].role, RelatedRole::ExpectedBecause);
    assert_eq!(
        diagnostic.related[0].message,
        "the type annotation declares this type"
    );
    assert_eq!(text_at(TYPE1, diagnostic.related[0].span), "i64");
    assert_eq!(line_of(TYPE1, diagnostic.related[0].span), 5);
}

#[test]
fn result_mismatch_names_the_result_type() {
    let diagnostic = at_line(TYPE1, "E1003", 3);
    assert_eq!(
        diagnostic.related[0].message,
        "the result type of 'pick' is declared here"
    );
    assert_eq!(text_at(TYPE1, diagnostic.related[0].span), "i64");
    assert_eq!(line_of(TYPE1, diagnostic.related[0].span), 3);
    let diagnostic = at_line(CALL1, "E1003", 3);
    assert_eq!(
        diagnostic.related[0].message,
        "the result type of 'f' is declared here"
    );
    assert_eq!(text_at(CALL1, diagnostic.related[0].span), "i64");
}

#[test]
fn argument_mismatch_names_the_parameter() {
    let diagnostic = at_line(CALL1, "E1003", 7);
    assert_eq!(diagnostic.related[0].role, RelatedRole::ExpectedBecause);
    assert_eq!(
        diagnostic.related[0].message,
        "parameter 2 of 'add' is declared here"
    );
    assert_eq!(text_at(CALL1, diagnostic.related[0].span), "i64");
    assert_eq!(line_of(CALL1, diagnostic.related[0].span), 1);
    let diagnostic = at_line(CALL1, "E1003", 9);
    assert_eq!(
        diagnostic.related[0].message,
        "parameter 1 of 'add' is declared here"
    );
    assert_eq!(
        diagnostic.related[0].span.start,
        CALL1.find("i64 -> i64 -> i64").unwrap()
    );
}

#[test]
fn inner_mismatch_has_no_because() {
    assert!(at_line(TYPE1, "E1003", 1).related.is_empty());
}

#[test]
fn std_parameter_note_points_into_std() {
    let (code, stderr) = check_cli(&[("Main.tz", STD2)], true);
    assert_eq!(code, 1);
    let diagnostic = &json_lines(&stderr)[0];
    assert_eq!(diagnostic["code"], "E1003");
    let related = &diagnostic["related"][0];
    assert_eq!(related["role"], "expected_because");
    assert_eq!(
        related["message"],
        "parameter 1 of 'length' is declared here"
    );
    assert_eq!(
        related["path"].as_str().unwrap().replace('\\', "/"),
        "std/Array.tz"
    );
    assert_eq!(related["span"]["line"], 1);
}

#[test]
fn partial_application_gets_a_help() {
    let diagnostic = at_line(CALL1, "E1003", 3);
    assert_eq!(
        diagnostic.help.as_deref(),
        Some(
            "a call with too few arguments evaluates to a function; add the missing argument(s) (1 more needed)"
        )
    );
    assert_eq!(diagnostic.detail.found.as_deref(), Some("i64 -> i64"));
    // A mismatch that is not a partial application has no such help.
    assert!(at_line(CALL1, "E1003", 7).help.is_none());
}

#[test]
fn unknown_value_lists_the_scope() {
    let diagnostic = first(NAME1, "E1002");
    assert_eq!(text_at(NAME1, diagnostic.span), "countt");
    assert_eq!(
        diagnostic.detail.scope,
        [ScopeEntry {
            name: "count".into(),
            ty: "i64".into()
        }]
    );
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("check the spelling, or define the name with 'let' or 'def'; names in scope: count")
    );
    let diagnostic = errors(NAME1)
        .into_iter()
        .find(|diagnostic| diagnostic.message == "unknown value 'length_off'")
        .unwrap();
    assert!(diagnostic.detail.scope.is_empty());
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("check the spelling, or define the name with 'let' or 'def'")
    );
    // The unknown type and the unknown module function are not changed.
    for diagnostic in errors(NAME1) {
        if diagnostic.code == "E1004" || diagnostic.message.starts_with("module 'Array'") {
            assert!(!has_extras(&diagnostic), "{}", diagnostic.message);
        }
    }
}

#[test]
fn unknown_field_lists_the_fields() {
    for diagnostic in errors(FIELD1) {
        assert_eq!(diagnostic.code, "E1007");
        assert_eq!(
            diagnostic.help.as_deref(),
            Some("the fields of 'Main.Point' are: x, y")
        );
    }
    assert_eq!(errors(FIELD1).len(), 2);
}

#[test]
fn call_helps() {
    let diagnostic = first(CALL1, "E1006");
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("remove the 1 extra argument(s)")
    );
    let helps: Vec<_> = errors(NOFN)
        .into_iter()
        .map(|diagnostic| diagnostic.into_data().help.unwrap())
        .collect();
    assert_eq!(
        helps,
        [
            "a number is not a function; remove the argument(s) or call a function",
            "a value of type i64 is not a function; remove the argument(s) or call a function",
            "a value of type string is not a function; remove the argument(s) or call a function",
        ]
    );
}

/// Applies the first fix of every diagnostic in `source` at once.
fn apply_first_fixes(source: &str) -> String {
    let diagnostics = errors(source);
    let edits: Vec<&Edit> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "E1021")
        .flat_map(|diagnostic| &diagnostic.fixes[0].edits)
        .collect();
    apply_edits(source, &edits).unwrap()
}

#[test]
fn non_exhaustive_match_offers_a_fix_that_compiles() {
    for (source, count) in [(MATCH1, 2), (FIX2, 2), (ARMS6, 2)] {
        let diagnostics: Vec<_> = errors(source)
            .into_iter()
            .filter(|diagnostic| diagnostic.code == "E1021")
            .collect();
        assert_eq!(diagnostics.len(), count, "{source}");
        for diagnostic in &diagnostics {
            assert!(
                diagnostic
                    .help
                    .as_deref()
                    .unwrap()
                    .starts_with("add an arm for '")
            );
            assert_eq!(diagnostic.fixes.len(), 1);
            let fix = &diagnostic.fixes[0];
            assert_eq!(fix.applicability.as_str(), "maybe");
            assert_eq!(fix.edits.len(), 1);
            let edit = &fix.edits[0];
            assert_eq!(edit.span.start, edit.span.end);
            assert_eq!(edit.span.start, diagnostic.span.end);
            assert!(edit.text.starts_with(" | ") && edit.text.ends_with(" -> unreachable ()"));
            assert_eq!(
                fix.title,
                format!("Add the arm '{}'", edit.text.trim_start())
            );
        }
        let fixed = apply_first_fixes(source);
        analyze_all(&fixed).unwrap_or_else(|set| panic!("{fixed}\n{:?}", set.diagnostics));
    }
    // The titles and the inserted arms.
    let titles: Vec<_> = errors(FIX2)
        .iter()
        .map(|diagnostic| diagnostic.fixes[0].title.clone())
        .collect();
    assert_eq!(
        titles,
        [
            "Add the arm '| false -> unreachable ()'",
            "Add the arm '| (true, false) -> unreachable ()'"
        ]
    );
    assert_eq!(
        apply_first_fixes(ARMS6),
        ARMS6
            .replace("        a\n", "        a | Triangle _ -> unreachable ()\n")
            .replace(
                "            2.0\n",
                "            2.0 | Triangle _ -> unreachable ()\n"
            )
    );
}

#[test]
fn no_fix_for_unwritable_witnesses() {
    let diagnostics = errors(FIX3);
    assert_eq!(diagnostics.len(), 3);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.help.is_some())
    );
    let fixes: Vec<usize> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.fixes.len())
        .collect();
    assert_eq!(fixes, [0, 1, 0]);
    assert_eq!(
        diagnostics[1].fixes[0].title,
        "Add the arm '| _ -> unreachable ()'"
    );
    let diagnostic = first(FIX4, "E1021");
    assert!(diagnostic.fixes.is_empty());
    assert!(diagnostic.help.is_some());
}

#[test]
fn nested_inline_match_fix_is_not_verified_here() {
    let diagnostic = first(ARMS5, "E1021");
    assert_eq!(diagnostic.fixes.len(), 1);
    let fixed = apply_first_fixes(ARMS5);
    // The new arm attaches to the nested match, so the outer match is still not exhaustive.
    let remaining = analyze_all(&fixed).expect_err(&fixed).diagnostics;
    assert!(
        remaining
            .iter()
            .any(|diagnostic| diagnostic.code == "E1021"),
        "{fixed}"
    );
}

#[test]
fn duplicates_name_the_first_declaration() {
    let cases = [
        (DUP2, "E1001", "Point", 1),
        (DUP3, "E1024", "Circle", 1),
        (DUP5, "E1001", "twice", 1),
        (DUP6, "E1001", "left", 3),
        (DUP7, "E1001", "a", 1),
        (DUP8, "E1001", "twice", 3),
        (DUP9, "E1001", "twice", 1),
    ];
    for (source, code, name, line) in cases {
        let diagnostic = first(source, code);
        assert_eq!(diagnostic.related.len(), 1, "{source}");
        let related = &diagnostic.related[0];
        assert_eq!(related.role, RelatedRole::FirstDefinedHere);
        assert_eq!(related.message, "first defined here");
        assert_eq!(text_at(source, related.span), name, "{source}");
        assert_eq!(line_of(source, related.span), line, "{source}");
        assert!(related.span.start < diagnostic.span.start, "{source}");
    }
}

#[test]
fn reserved_names_have_no_first_definition() {
    let diagnostic = first(DUP12, "E1001");
    assert_eq!(diagnostic.message, "duplicate or reserved name 'Array'");
    assert!(diagnostic.related.is_empty());
}

#[test]
fn cli_json_of_a_fix_has_edits_with_spans() {
    let (_, stderr) = check_cli(&[("Main.tz", FIX2)], true);
    let diagnostic = &json_lines(&stderr)[0];
    assert_eq!(diagnostic["code"], "E1021");
    let fix = &diagnostic["fixes"][0];
    assert_eq!(fix["applicability"], "maybe");
    assert_eq!(fix["title"], "Add the arm '| false -> unreachable ()'");
    let edit = &fix["edits"][0];
    assert_eq!(edit["text"], " | false -> unreachable ()");
    assert_eq!(edit["span"]["start"], edit["span"]["end"]);
    assert!(edit["path"].as_str().unwrap().ends_with("Main.tz"));
    let keys: Vec<&str> = diagnostic
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert!(keys.contains(&"help") && keys.contains(&"fixes"));
}
```

### G-5: `src/diagnostic.rs` の単体テスト（手順 3〜6。`mod tests` の既存の 2 件の次）

`diagnostic_is_one_pointer_wide` は手順 3、`builders_keep_the_limits`・`role_and_applicability_names_are_stable` は手順 4、`without_extras_the_output_is_unchanged`・`json_key_order_and_escaping`・`related_in_another_source_uses_the_lookup`・`unresolved_related_and_fixes_are_left_out` は手順 5、`apply_edits_orders_and_rejects_overlaps` は手順 6 で足す。

```rust
    #[test]
    fn diagnostic_is_one_pointer_wide() {
        let word = std::mem::size_of::<usize>();
        assert_eq!(std::mem::size_of::<Diagnostic>(), word);
        assert_eq!(std::mem::size_of::<Result<(), Diagnostic>>(), word);
        assert_eq!(std::mem::size_of::<Option<Diagnostic>>(), word);
    }

    #[test]
    fn without_extras_the_output_is_unchanged() {
        let source = "a\nbc d";
        let warning = Diagnostic::warning("W1003", "unreachable", Span::new(4, 5));
        assert_eq!(
            warning.render("M.tz", source),
            "M.tz:2:3: warning[W1003]: unreachable\n  2 | bc d\n    |   ^"
        );
        assert_eq!(
            warning.json("M.tz", source),
            "{\"severity\":\"warning\",\"code\":\"W1003\",\"message\":\"unreachable\",\"path\":\"M.tz\",\"span\":{\"start\":4,\"end\":5,\"line\":2,\"column\":3,\"end_line\":2,\"end_column\":4}}"
        );
        let error = Diagnostic::new("E0001", "bad", Span::new(0, 1));
        assert_eq!(
            error.render_with("error", "A.tz", "x", &|_| None),
            "A.tz:1:1: error[E0001]: bad\n  1 | x\n    | ^"
        );
    }

    fn edits(count: usize) -> Vec<Edit> {
        (0..count)
            .map(|_| Edit {
                span: Span::new(0, 0),
                text: String::new(),
            })
            .collect()
    }

    #[test]
    fn builders_keep_the_limits() {
        let span = Span::new(0, 1);
        let mut diagnostic = Diagnostic::new("E0001", "m", span);
        for index in 0..20 {
            diagnostic = diagnostic.related(RelatedRole::Note, format!("n{index}"), span);
        }
        assert_eq!(diagnostic.related.len(), MAX_RELATED);
        assert_eq!(diagnostic.related[MAX_RELATED - 1].message, "n15");
        let mut diagnostic = Diagnostic::new("E0001", "m", span);
        for _ in 0..10 {
            diagnostic = diagnostic.fix("t", Applicability::Maybe, edits(1));
        }
        assert_eq!(diagnostic.fixes.len(), MAX_FIXES);
        let diagnostic = Diagnostic::new("E0001", "m", span)
            .fix("none", Applicability::Maybe, edits(0))
            .fix("too many", Applicability::Maybe, edits(MAX_EDITS + 1))
            .fix("limit", Applicability::Maybe, edits(MAX_EDITS));
        assert_eq!(diagnostic.fixes.len(), 1);
        assert_eq!(diagnostic.fixes[0].title, "limit");
        let candidates = (0..12)
            .map(|index| Candidate {
                name: index.to_string(),
                signature: String::new(),
                how: String::new(),
            })
            .collect();
        let diagnostic = Diagnostic::new("E0001", "m", span).with_candidates(candidates);
        assert_eq!(diagnostic.detail.candidates.len(), MAX_CANDIDATES);
        let scope = (0..40)
            .map(|index| ScopeEntry {
                name: index.to_string(),
                ty: String::new(),
            })
            .collect();
        let diagnostic = Diagnostic::new("E0001", "m", span).with_scope(scope);
        assert_eq!(diagnostic.detail.scope.len(), MAX_SCOPE);
    }

    #[test]
    fn role_and_applicability_names_are_stable() {
        let roles = [
            (RelatedRole::MovedHere, "moved_here"),
            (RelatedRole::BorrowedHere, "borrowed_here"),
            (RelatedRole::BorrowUsedHere, "borrow_used_here"),
            (RelatedRole::DeclaredHere, "declared_here"),
            (RelatedRole::ExpectedBecause, "expected_because"),
            (RelatedRole::FirstDefinedHere, "first_defined_here"),
            (RelatedRole::ViaCall, "via_call"),
            (RelatedRole::RequiredBy, "required_by"),
            (RelatedRole::Note, "note"),
        ];
        for (role, name) in roles {
            assert_eq!(role.as_str(), name);
        }
        assert_eq!(Applicability::Certain.as_str(), "certain");
        assert_eq!(Applicability::Likely.as_str(), "likely");
        assert_eq!(Applicability::Maybe.as_str(), "maybe");
    }

    #[test]
    fn json_key_order_and_escaping() {
        let source = "let x = 1\n";
        let diagnostic = Diagnostic::new("E0001", "a\n\"\\\u{1}", Span::new(4, 5))
            .with_kind("k")
            .related(RelatedRole::Note, "r", Span::new(0, 3))
            .with_help("h")
            .expected_found("E", "F")
            .with_candidates(vec![Candidate {
                name: "n".into(),
                signature: "s".into(),
                how: "w".into(),
            }])
            .with_scope(vec![ScopeEntry {
                name: "x".into(),
                ty: "i64".into(),
            }])
            .fix(
                "t",
                Applicability::Certain,
                vec![Edit {
                    span: Span::new(9, 9),
                    text: "\n".into(),
                }],
            );
        assert_eq!(
            diagnostic.json("M.tz", source),
            r#"{"severity":"error","code":"E0001","message":"a\n\"\\\u0001","path":"M.tz","span":{"start":4,"end":5,"line":1,"column":5,"end_line":1,"end_column":6},"kind":"k","related":[{"role":"note","message":"r","path":"M.tz","span":{"start":0,"end":3,"line":1,"column":1,"end_line":1,"end_column":4}}],"help":"h","expected":"E","found":"F","candidates":[{"name":"n","signature":"s","how":"w"}],"scope":[{"name":"x","type":"i64"}],"fixes":[{"title":"t","applicability":"certain","edits":[{"path":"M.tz","span":{"start":9,"end":9,"line":1,"column":10,"end_line":1,"end_column":10},"text":"\n"}]}]}"#
        );
    }

    #[test]
    fn related_in_another_source_uses_the_lookup() {
        let main = "def first :: i64 = 2\n";
        let other = "\n\ndef first :: i64 = 1\n";
        let diagnostic = Diagnostic::new("E1001", "dup", Span::new(4, 9).in_source(0))
            .related(
                RelatedRole::FirstDefinedHere,
                "first defined here",
                Span::new(6, 11).in_source(1),
            )
            .related(RelatedRole::Note, "same source", Span::new(0, 3));
        let lookup = |index: usize| (index == 1).then_some(("Other.tz", other));
        assert_eq!(
            diagnostic.render_with("error", "Main.tz", main, &lookup),
            "Main.tz:1:5: error[E1001]: dup\n  1 | def first :: i64 = 2\n    |     ^\n  note: first defined here\n    --> Other.tz:3:5\n  3 | def first :: i64 = 1\n    |     ^\n  note: same source\n    --> Main.tz:1:1\n  1 | def first :: i64 = 2\n    | ^"
        );
        let json = diagnostic.json_with("error", "Main.tz", main, &lookup);
        assert!(json.contains(r#""related":[{"role":"first_defined_here","message":"first defined here","path":"Other.tz","span":{"start":6,"end":11,"line":3,"column":5,"end_line":3,"end_column":10}},{"role":"note","message":"same source","path":"Main.tz","span":{"start":0,"end":3,"line":1,"column":1,"end_line":1,"end_column":4}}]"#), "{json}");
    }

    #[test]
    fn unresolved_related_and_fixes_are_left_out() {
        let main = "def first :: i64 = 2\n";
        let elsewhere = Span::new(0, 1).in_source(1);
        let edit = |span| Edit {
            span,
            text: "x".into(),
        };
        let diagnostic = Diagnostic::new("E1001", "dup", Span::new(4, 9).in_source(0))
            .related(RelatedRole::Note, "unknown source", elsewhere)
            .fix("whole", Applicability::Maybe, vec![edit(Span::new(0, 0))])
            .fix(
                "half",
                Applicability::Maybe,
                vec![edit(Span::new(0, 0)), edit(elsewhere)],
            );
        let text = diagnostic.render_with("error", "Main.tz", main, &|_| None);
        assert!(
            !text.contains("note:")
                && text.contains("fix (maybe): whole")
                && !text.contains("half")
        );
        let json = diagnostic.json_with("error", "Main.tz", main, &|_| None);
        assert!(
            !json.contains("\"related\"")
                && json.contains("\"title\":\"whole\"")
                && !json.contains("half")
        );
        // `render` and `json` pass no lookup, so a location in the main source is the only kind they show.
        assert!(
            diagnostic
                .json("Main.tz", main)
                .contains("\"title\":\"whole\"")
        );
    }

    #[test]
    fn apply_edits_orders_and_rejects_overlaps() {
        let edit = |start, end, text: &str| Edit {
            span: Span::new(start, end),
            text: text.into(),
        };
        let text = "abcdef";
        let (a, b, c) = (edit(4, 5, "X"), edit(0, 1, "Y"), edit(2, 2, "Z"));
        assert_eq!(apply_edits(text, &[&a, &b, &c]).as_deref(), Some("YbZcdXf"));
        let (p, q) = (edit(3, 3, "1"), edit(3, 3, "2"));
        assert_eq!(apply_edits(text, &[&p, &q]).as_deref(), Some("abc12def"));
        assert_eq!(apply_edits(text, &[&q, &p]).as_deref(), Some("abc21def"));
        assert_eq!(apply_edits(text, &[&edit(1, 3, ""), &edit(2, 4, "")]), None);
        assert_eq!(apply_edits(text, &[&edit(5, 7, "")]), None);
        assert_eq!(apply_edits("日本", &[&edit(1, 1, "x")]), None);
        assert_eq!(apply_edits(text, &[]).as_deref(), Some(text));
        assert_eq!(
            apply_edits(text, &[&edit(1, 3, "-"), &edit(3, 4, "+")]).as_deref(),
            Some("a-+ef")
        );
    }
```
