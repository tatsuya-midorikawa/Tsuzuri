# G23 付録: ヒント DB の表（手順 3.x の `src/hints.rs` の `TABLE`）

[G23 本体](G23-diagnostic-kinds-explain-hints.md) の付録 D です。**40 件**を、HEAD の `target/release/tsuzuri check --json` で確かめました（各 `bad` が指定の診断を出し、各 `good` がエラーなしで通ります）。`patterns` は、主スパンの先頭のトークンに当たることまで確かめました（付録 E.6）。

各項目のフィールドは本体の「ヒントの表」の `Hint` と同じです。`patterns` は代替の列（どれか 1 つが合えばよい）で、各要素は 1 トークンの照合（`@` が付いたものが診断の主スパンの先頭のトークン）です。`message_has` は診断のメッセージに全部含まれる部分文字列です。順序が優先順位です（先に合ったものが付きます）。

## D.1 一覧

| # | id | kind | コード | message_has | patterns | fix |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `tuple-dot-index` | `expected_identifier` | E0002 | （なし） | `[[".", "@<int>"]]` | （なし） |
| 2 | `type-ascription` | `expected_token` | E0002 | `closing delimiter` | `[["(", "<any>", "@:"]]` | （なし） |
| 3 | `display-to-string` | `class_has_no_method` | E1002 | `no method 'to_string'` | `[["Display", ".", "@to_string"]]` | （なし） |
| 4 | `new-is-reserved` | `expected_identifier` | E0002 | （なし） | `[["let", "@new"]]` | （なし） |
| 5 | `abs-on-integer` | `type_mismatch` | E1003 | `found f64` | `[["@abs", "<ident>"]]` | （なし） |
| 6 | `return-after-then` | `expected_expression` | E0002 | （なし） | `[["then", "@return"]]` | （なし） |
| 7 | `fn-unit-parameter` | `expected_token` | E0002 | `explicit return type` | `[["(", ")", "@="]]` | （なし） |
| 8 | `union-braces` | `expected_token` | E0002 | `after the union name` | `[["<upper>", "@{"]]` | （なし） |
| 9 | `for-braces` | `expected_token` | E0002 | `after the enumerable expression` | `[["@{"]]` | （なし） |
| 10 | `while-braces` | `expected_token` | E0002 | `after the while condition` | `[["@{"]]` | （なし） |
| 11 | `print-macro` | `unknown_value` | E1002 | （なし） | `[["@println", "!"]]` | （なし） |
| 12 | `compound-assignment` | `expected_expression` | E0002 | （なし） | `[["+", "@="], ["-", "@="], ["*", "@="], ["/", "@="]]` | （なし） |
| 13 | `module-path-call` | `expected_binding_end` | E0002 | （なし） | `[["<upper>", "@::", "<ident>"]]` | （なし） |
| 14 | `method-call` | `field_access_non_record` | E1007 | （なし） | `[[".", "@<ident>", "("]]` | （なし） |
| 15 | `struct-keyword` | `unknown_value` | E1002 | （なし） | `[["@struct"]]` | （なし） |
| 16 | `null-value` | `unknown_value` | E1002 | （なし） | `[["@null"]]` | （なし） |
| 17 | `question-mark` | `unexpected_character` | E0001 | `'?'` | `[["@?"]]` | （なし） |
| 18 | `let-in` | `expected_expression` | E0002 | （なし） | `[["->", "@let"]]` | （なし） |
| 19 | `and-as-boolean` | `and_without_recursive_group` | E1019 | （なし） | `[["<ident>", "and", "@<ident>"]]` | （なし） |
| 20 | `pub-keyword` | `expected_token` | E0002 | `end of the file` | `[["pub", "@def"]]` | （なし） |
| 21 | `use-import` | `expected_token` | E0002 | `end of the file` | `[["use", "<ident>", "@::"]]` | （なし） |
| 22 | `generic-angle-def` | `expected_token` | E0002 | `after the declaration name` | `[["def", "<ident>", "@<"]]` | （なし） |
| 23 | `def-parameter-list` | `expected_token` | E0002 | `after the declaration name` | `[["def", "<ident>", "@("]]` | （なし） |
| 24 | `record-call` | `unknown_value` | E1002 | （なし） | `[["@<upper>", "("]]` | （なし） |
| 25 | `module-len` | `unknown_module_member` | E1002 | （なし） | `[["<upper>", ".", "@len"]]` | （なし） |
| 26 | `len-function` | `unknown_value` | E1002 | （なし） | `[["@len", "<ident>"]]` | （なし） |
| 27 | `option-type` | `unknown_type_name` | E1004 | （なし） | `[["@Option", "<"]]` | `Maybe`（likely） |
| 28 | `integer-type-name` | `unknown_type_name` | E1004 | （なし） | `[["@Int"], ["@int"], ["@integer"]]` | （なし） |
| 29 | `float-type-name` | `unknown_type_name` | E1004 | （なし） | `[["@float"], ["@Float"], ["@double"]]` | `f64`（maybe） |
| 30 | `string-type-name` | `unknown_type_name` | E1004 | （なし） | `[["@str"], ["@Str"]]` | `string`（likely） |
| 31 | `match-without-with` | `expected_token` | E0002 | `after the matched expression` | （なし） | （なし） |
| 32 | `match-fat-arrow` | `expected_token` | E0002 | `after the pattern` | `[["@=>"]]` | `->`（likely） |
| 33 | `match-arm-bar` | `match_without_arms` | E0002 | （なし） | （なし） | （なし） |
| 34 | `true-false-case` | `unknown_value` | E1002 | （なし） | `[["@True"]]` | `true`（likely） |
| 35 | `false-case` | `unknown_value` | E1002 | （なし） | `[["@False"]]` | `false`（likely） |
| 36 | `void-value` | `unknown_value` | E1002 | （なし） | `[["@void"]]` | `()`（maybe） |
| 37 | `char-double-quotes` | `type_mismatch` | E1003 | `expected char, found string` | `[["@<string>"]]` | （なし） |
| 38 | `string-single-quotes` | `char_literal_not_one_unit` | E0001 | `char literal` | `[["@<any>"]]` | （なし） |
| 39 | `lambda-pipes` | `expected_expression` | E0002 | （なし） | `[["(", "@\|"]]` | （なし） |
| 40 | `lambda-fat-arrow` | `expected_token` | E0002 | `closing delimiter` | `[["<ident>", "@=>"]]` | （なし） |

## D.2 各項目（help・bad・good）

### D.2.1 `tuple-dot-index`

- title: Tuple elements are read by pattern matching
- kind: `expected_identifier`（コード E0002）
- message_has: （なし）
- patterns: `[[".", "@<int>"]]`
- help: `read a tuple element by pattern matching, as in 'match pair with | (first, _) -> first'`

```tsuzuri
def first :: (i64 * i64) -> i64 = \pair -> pair.0
```

good:

```tsuzuri
def first :: (i64 * i64) -> i64 = \pair ->
    match pair with
    | (left, _) -> left
```

### D.2.2 `type-ascription`

- title: A value is annotated on its binding
- kind: `expected_token`（コード E0002）
- message_has: `closing delimiter`
- patterns: `[["(", "<any>", "@:"]]`
- help: `annotate the binding instead, as in 'let x: i64 = 5'`

```tsuzuri
let x = (5: i64)
```

good:

```tsuzuri
let x: i64 = 5
```

### D.2.3 `display-to-string`

- title: to_string and Display.display
- kind: `class_has_no_method`（コード E1002）
- message_has: `no method 'to_string'`
- patterns: `[["Display", ".", "@to_string"]]`
- help: `use 'to_string value' to consume a value or 'Display.display value' to borrow it`

```tsuzuri
let s = Display.to_string 5
```

good:

```tsuzuri
let s = to_string 5
```

### D.2.4 `new-is-reserved`

- title: 'new' is a reserved word
- kind: `expected_identifier`（コード E0002）
- message_has: （なし）
- patterns: `[["let", "@new"]]`
- help: `'new' is a reserved word; choose another name such as 'create'`

```tsuzuri
let new = 1
```

good:

```tsuzuri
let create = 1
```

### D.2.5 `abs-on-integer`

- title: 'abs' is for f64; integers use Int.abs
- kind: `type_mismatch`（コード E1003）
- message_has: `found f64`
- patterns: `[["@abs", "<ident>"]]`
- help: `'abs' takes an f64; use 'Int.abs' for a signed integer`

```tsuzuri
def f :: i64 -> i64 = \n -> abs n
```

good:

```tsuzuri
def f :: i64 -> i64 = \n -> Int.abs n
```

### D.2.6 `return-after-then`

- title: A task choice uses braces
- kind: `expected_expression`（コード E0002）
- message_has: （なし）
- patterns: `[["then", "@return"]]`
- help: `write a task choice as 'if c { return a } else { return b }'`

```tsuzuri
def f :: bool -> Task<i64> = \c ->
    task {
        if c then return 1
        return 2
    }
```

good:

```tsuzuri
def f :: bool -> Task<i64> = \c ->
    task {
        if c { return 1 } else { return 2 }
    }
```

### D.2.7 `fn-unit-parameter`

- title: An implementation takes '_unit', not '()'
- kind: `expected_token`（コード E0002）
- message_has: `explicit return type`
- patterns: `[["(", ")", "@="]]`
- help: `write the implementation as 'fn name _unit = ...'; '()' is not a parameter pattern there`

```tsuzuri
def f :: unit -> i64
fn f () = 1
```

good:

```tsuzuri
def f :: unit -> i64
fn f _unit = 1
```

### D.2.8 `union-braces`

- title: A union is written with = and |
- kind: `expected_token`（コード E0002）
- message_has: `after the union name`
- patterns: `[["<upper>", "@{"]]`
- help: `declare a union with '=' and '|', as in 'union Reading = Missing | Value of i64'`

```tsuzuri
union Reading { Missing, Value(i64) }
```

good:

```tsuzuri
union Reading = Missing | Value of i64
```

### D.2.9 `for-braces`

- title: A for loop uses 'do' and an indented body
- kind: `expected_token`（コード E0002）
- message_has: `after the enumerable expression`
- patterns: `[["@{"]]`
- help: `write 'for i in a .. b do' and indent the body; the range includes both ends`

```tsuzuri
def f :: i64 -> i64 = \n ->
    let mut t = 0
    for i in 0 .. n { t = t + i }
    t
```

good:

```tsuzuri
def f :: i64 -> i64 = \n ->
    let mut t = 0
    for i in 0 .. n do
        t = t + i
    t
```

### D.2.10 `while-braces`

- title: A while loop uses 'do' and an indented body
- kind: `expected_token`（コード E0002）
- message_has: `after the while condition`
- patterns: `[["@{"]]`
- help: `write 'while condition do' and indent the body`

```tsuzuri
def f :: i64 -> i64 = \n ->
    let mut i = 0
    while i < n {
        i = i + 1
    }
    i
```

good:

```tsuzuri
def f :: i64 -> i64 = \n ->
    let mut i = 0
    while i < n do
        i = i + 1
    i
```

### D.2.11 `print-macro`

- title: Printing is IO.write_line
- kind: `unknown_value`（コード E1002）
- message_has: （なし）
- patterns: `[["@println", "!"]]`
- help: `write a line with 'IO.write_line' in an IO block, or use 'Debug.print' while debugging`

```tsuzuri
def f :: unit -> unit = \_ -> println!("x")
```

good:

```tsuzuri
IO.write_line "x"
```

### D.2.12 `compound-assignment`

- title: No compound assignment
- kind: `expected_expression`（コード E0002）
- message_has: （なし）
- patterns: `[["+", "@="], ["-", "@="], ["*", "@="], ["/", "@="]]`
- help: `there is no compound assignment; write 'x = x + 1'`

```tsuzuri
def f :: i64 -> i64 = \n ->
    let mut x = n
    x += 1
    x
```

good:

```tsuzuri
def f :: i64 -> i64 = \n ->
    let mut x = n
    x = x + 1
    x
```

### D.2.13 `module-path-call`

- title: A module member is reached with '.'
- kind: `expected_binding_end`（コード E0002）
- message_has: （なし）
- patterns: `[["<upper>", "@::", "<ident>"]]`
- help: `reach a module member with '.', as in 'String.length s'; '::' only joins namespace parts`

```tsuzuri
def f :: unit -> i64 = \_ ->
    let n = String::length "abc"
    n
```

good:

```tsuzuri
def f :: unit -> i64 = \_ ->
    let n = String.length "abc"
    n
```

### D.2.14 `method-call`

- title: No methods; call the module function
- kind: `field_access_non_record`（コード E1007）
- message_has: （なし）
- patterns: `[[".", "@<ident>", "("]]`
- help: `there are no methods; call the module function, as in 'Array.length xs' or 'Array.map f xs'`

```tsuzuri
def f :: [i64] -> [i64] = \xs -> xs.map(\x -> x + 1)
```

good:

```tsuzuri
def f :: [i64] -> [i64] = \xs -> Array.map (\x -> x + 1) xs
```

### D.2.15 `struct-keyword`

- title: A struct is a record
- kind: `unknown_value`（コード E1002）
- message_has: （なし）
- patterns: `[["@struct"]]`
- help: `declare a record with 'record Point { x: i64, y: i64 }'`

```tsuzuri
struct Point { x: i64 }
```

good:

```tsuzuri
record Point { x: i64 }
```

### D.2.16 `null-value`

- title: No null; use Maybe
- kind: `unknown_value`（コード E1002）
- message_has: （なし）
- patterns: `[["@null"]]`
- help: `there is no null; use 'Maybe<T>' with 'Maybe.None' and 'Maybe.Some value'`

```tsuzuri
def f :: unit -> i64 = \_ -> null
```

good:

```tsuzuri
def f :: unit -> Maybe<i64> = \_ -> Maybe.None
```

### D.2.17 `question-mark`

- title: '?' is not an operator
- kind: `unexpected_character`（コード E0001）
- message_has: `'?'`
- patterns: `[["@?"]]`
- help: `'?' is not an operator; write 'if c then a else b', or 'match' on a Result or Maybe`

```tsuzuri
def f :: bool -> i64 = \c -> c ? 1 : 2
```

good:

```tsuzuri
def f :: bool -> i64 = \c -> if c then 1 else 2
```

### D.2.18 `let-in`

- title: No 'let ... in'
- kind: `expected_expression`（コード E0002）
- message_has: （なし）
- patterns: `[["->", "@let"]]`
- help: `start an indented body for the bindings: 'let x = 1' on its own line, then the result below`

```tsuzuri
def f :: unit -> i64 = \_ -> let x = 1 in x
```

good:

```tsuzuri
def f :: unit -> i64 = \_ ->
    let x = 1
    x
```

### D.2.19 `and-as-boolean`

- title: Boolean and is '&&'
- kind: `and_without_recursive_group`（コード E1019）
- message_has: （なし）
- patterns: `[["<ident>", "and", "@<ident>"]]`
- help: `'and' continues recursive groups; write '&&' for boolean and, '||' for or`

```tsuzuri
def f :: bool -> bool -> bool = \a b -> a and b
```

good:

```tsuzuri
def f :: bool -> bool -> bool = \a b -> a && b
```

### D.2.20 `pub-keyword`

- title: No 'pub'; declarations are public by default
- kind: `expected_token`（コード E0002）
- message_has: `end of the file`
- patterns: `[["pub", "@def"]]`
- help: `declarations are public by default; write 'private def' to hide one and 'export def' for a C export`

```tsuzuri
pub def f :: i64 -> i64 = \x -> x
```

good:

```tsuzuri
def f :: i64 -> i64 = \x -> x
```

### D.2.21 `use-import`

- title: Imports are 'using Namespace'
- kind: `expected_token`（コード E0002）
- message_has: `end of the file`
- patterns: `[["use", "<ident>", "@::"]]`
- help: `import a namespace with 'using Namespace' at the top of the file`

```tsuzuri
use std::io;
```

good:

```tsuzuri
using std
```

### D.2.22 `generic-angle-def`

- title: Generics are type variables like 'a
- kind: `expected_token`（コード E0002）
- message_has: `after the declaration name`
- patterns: `[["def", "<ident>", "@<"]]`
- help: `write generics as type variables in the signature, as in "def id :: 'a -> 'a = \\x -> x"`

```tsuzuri
def id<T> :: T -> T = \x -> x
```

good:

```tsuzuri
def id :: 'a -> 'a = \x -> x
```

### D.2.23 `def-parameter-list`

- title: A def has a signature and a lambda
- kind: `expected_token`（コード E0002）
- message_has: `after the declaration name`
- patterns: `[["def", "<ident>", "@("]]`
- help: `declare a function as 'def name :: ParamType -> ResultType = \arg -> body'`

```tsuzuri
def f(x: i64): i64 = x
```

good:

```tsuzuri
def f :: i64 -> i64 = \x -> x
```

### D.2.24 `record-call`

- title: A record is built with field names
- kind: `unknown_value`（コード E1002）
- message_has: （なし）
- patterns: `[["@<upper>", "("]]`
- help: `build a record with field names, as in 'Point { x: 1, y: 2 }'`

```tsuzuri
record Point { x: i64, y: i64 }
let p = Point(1, 2)
```

good:

```tsuzuri
record Point { x: i64, y: i64 }
let p = Point { x: 1, y: 2 }
```

### D.2.25 `module-len`

- title: Length is .length or Array.length
- kind: `unknown_module_member`（コード E1002）
- message_has: （なし）
- patterns: `[["<upper>", ".", "@len"]]`
- help: `the length of an array, list, or string is 'xs.length' or 'Array.length xs'`

```tsuzuri
def f :: string -> i64 = \s -> String.len s
```

good:

```tsuzuri
def f :: string -> i64 = \s -> String.length s
```

### D.2.26 `len-function`

- title: Length is .length or Array.length
- kind: `unknown_value`（コード E1002）
- message_has: （なし）
- patterns: `[["@len", "<ident>"]]`
- help: `the length of an array, list, or string is 'xs.length' or 'Array.length xs'`

```tsuzuri
def f :: [i64] -> i64 = \xs -> len xs
```

good:

```tsuzuri
def f :: [i64] -> i64 = \xs -> Array.length xs
```

### D.2.27 `option-type`

- title: Option is Maybe
- kind: `unknown_type_name`（コード E1004）
- message_has: （なし）
- patterns: `[["@Option", "<"]]`
- help: `the optional type is 'Maybe<T>', with the cases 'Maybe.None' and 'Maybe.Some'`
- fix: 主トークンを `Maybe` に置き換える（applicability `likely`。title は `replace with 'Maybe'`）

```tsuzuri
def f :: Option<i64> -> i64 = \o -> 0
```

good:

```tsuzuri
def f :: Maybe<i64> -> i64 = \o -> 0
```

### D.2.28 `integer-type-name`

- title: Integer types are sized
- kind: `unknown_type_name`（コード E1004）
- message_has: （なし）
- patterns: `[["@Int"], ["@int"], ["@integer"]]`
- help: `write a sized integer type such as 'i32' or 'i64'`

```tsuzuri
def f :: int -> int = \x -> x
```

good:

```tsuzuri
def f :: i32 -> i32 = \x -> x
```

### D.2.29 `float-type-name`

- title: Float types are f32 and f64
- kind: `unknown_type_name`（コード E1004）
- message_has: （なし）
- patterns: `[["@float"], ["@Float"], ["@double"]]`
- help: `write a sized floating-point type: 'f64' or 'f32'`
- fix: 主トークンを `f64` に置き換える（applicability `maybe`。title は `replace with 'f64'`）

```tsuzuri
def f :: float -> float = \x -> x
```

good:

```tsuzuri
def f :: f64 -> f64 = \x -> x
```

### D.2.30 `string-type-name`

- title: The string type is 'string'
- kind: `unknown_type_name`（コード E1004）
- message_has: （なし）
- patterns: `[["@str"], ["@Str"]]`
- help: `the string type is written 'string'`
- fix: 主トークンを `string` に置き換える（applicability `likely`。title は `replace with 'string'`）

```tsuzuri
def f :: str -> str = \x -> x
```

good:

```tsuzuri
def f :: string -> string = \x -> x
```

### D.2.31 `match-without-with`

- title: A match needs 'with'
- kind: `expected_token`（コード E0002）
- message_has: `after the matched expression`
- patterns: `[]`（トークンの条件なし）
- help: `write 'match value with' and then one '| pattern -> result' arm per line`

```tsuzuri
def f :: i64 -> i64 = \x -> match x | 1 -> 2 | _ -> 3
```

good:

```tsuzuri
def f :: i64 -> i64 = \x -> match x with | 1 -> 2 | _ -> 3
```

### D.2.32 `match-fat-arrow`

- title: A match arm uses '->'
- kind: `expected_token`（コード E0002）
- message_has: `after the pattern`
- patterns: `[["@=>"]]`
- help: `separate a match pattern from its result with '->', not '=>'`
- fix: 主トークンを `->` に置き換える（applicability `likely`。title は `replace with '->'`）

```tsuzuri
def f :: i64 -> i64 = \x -> match x with | 1 => 2 | _ => 3
```

good:

```tsuzuri
def f :: i64 -> i64 = \x -> match x with | 1 -> 2 | _ -> 3
```

### D.2.33 `match-arm-bar`

- title: Every match arm starts with '|'
- kind: `match_without_arms`（コード E0002）
- message_has: （なし）
- patterns: `[]`（トークンの条件なし）
- help: `start every arm with '|', as in 'match x with | 1 -> a | _ -> b'`

```tsuzuri
def f :: i64 -> i64 = \x ->
    match x with
    1 -> 2
    _ -> 3
```

good:

```tsuzuri
def f :: i64 -> i64 = \x ->
    match x with
    | 1 -> 2
    | _ -> 3
```

### D.2.34 `true-false-case`

- title: Booleans are 'true' and 'false'
- kind: `unknown_value`（コード E1002）
- message_has: （なし）
- patterns: `[["@True"]]`
- help: `the boolean literals are 'true' and 'false'`
- fix: 主トークンを `true` に置き換える（applicability `likely`。title は `replace with 'true'`）

```tsuzuri
def f :: unit -> bool = \_ -> True
```

good:

```tsuzuri
def f :: unit -> bool = \_ -> true
```

### D.2.35 `false-case`

- title: Booleans are 'true' and 'false'
- kind: `unknown_value`（コード E1002）
- message_has: （なし）
- patterns: `[["@False"]]`
- help: `the boolean literals are 'true' and 'false'`
- fix: 主トークンを `false` に置き換える（applicability `likely`。title は `replace with 'false'`）

```tsuzuri
def f :: unit -> bool = \_ -> False
```

good:

```tsuzuri
def f :: unit -> bool = \_ -> false
```

### D.2.36 `void-value`

- title: The unit value is '()'
- kind: `unknown_value`（コード E1002）
- message_has: （なし）
- patterns: `[["@void"]]`
- help: `the unit value is written '()'`
- fix: 主トークンを `()` に置き換える（applicability `maybe`。title は `replace with '()'`）

```tsuzuri
def f :: unit -> unit = \_ -> void
```

good:

```tsuzuri
def f :: unit -> unit = \_ -> ()
```

### D.2.37 `char-double-quotes`

- title: A char uses single quotes
- kind: `type_mismatch`（コード E1003）
- message_has: `expected char, found string`
- patterns: `[["@<string>"]]`
- help: `a char literal uses single quotes, as in 'a'`

```tsuzuri
def f :: unit -> char = \_ -> "a"
```

good:

```tsuzuri
def f :: unit -> char = \_ -> 'a'
```

### D.2.38 `string-single-quotes`

- title: A string uses double quotes
- kind: `char_literal_not_one_unit`（コード E0001）
- message_has: `char literal`
- patterns: `[["@<any>"]]`
- help: `a string literal uses double quotes, as in "abc"`

```tsuzuri
def f :: unit -> string = \_ -> 'abc'
```

good:

```tsuzuri
def f :: unit -> string = \_ -> "abc"
```

### D.2.39 `lambda-pipes`

- title: A lambda is '\x -> body'
- kind: `expected_expression`（コード E0002）
- message_has: （なし）
- patterns: `[["(", "@|"]]`
- help: `write a lambda as '\\x -> x + 1'`

```tsuzuri
def f :: [i64] -> [i64] = \xs -> Array.map (|x| x + 1) xs
```

good:

```tsuzuri
def f :: [i64] -> [i64] = \xs -> Array.map (\x -> x + 1) xs
```

### D.2.40 `lambda-fat-arrow`

- title: A lambda is '\x -> body'
- kind: `expected_token`（コード E0002）
- message_has: `closing delimiter`
- patterns: `[["<ident>", "@=>"]]`
- help: `write a lambda as '\\x -> x + 1'`

```tsuzuri
def f :: [i64] -> [i64] = \xs -> Array.map (x => x + 1) xs
```

good:

```tsuzuri
def f :: [i64] -> [i64] = \xs -> Array.map (\x -> x + 1) xs
```

