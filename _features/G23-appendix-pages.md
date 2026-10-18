# G23 付録: Phase 1 の説明ページ 8 本の全文（`src/explain/<kind>.md`）

[G23 本体](G23-diagnostic-kinds-explain-hints.md) の付録 A です。次の 8 つを `src/explain/<kind>.md` としてそのまま置きます。ファイルの形は本体の「説明ページ」の規則どおりです。各ページの `Bad` が指定の kind を出し、`Good` がエラーなしで通ることを、HEAD の `target/release/tsuzuri check --json` で確かめました（コードは `Bad` の診断のコード）。

| kind | コード |
| --- | --- |
| `use_after_move` | `E1012` |
| `borrow_conflict` | `E1014` |
| `mutate_immutable_value` | `E1014` |
| `type_mismatch` | `E1003` |
| `unknown_value` | `E1002` |
| `unknown_module_member` | `E1002` |
| `non_exhaustive_match` | `E1021` |
| `borrow_outlives_block` | `E1013` |

## A.1 `src/explain/use_after_move.md`

`````markdown
# Use of a moved value

## Why
Passing a value to a function that takes ownership, binding it to another name, or consuming it with `to_string` moves it. The old name no longer owns a value, so reading it afterwards is rejected with `E1012`.

## Bad
```tsuzuri
def total :: unit -> i64 = \_ ->
    let text = "hello"
    let copy = to_string text
    text.length + copy.length
```

## Good
```tsuzuri
def total :: unit -> i64 = \_ ->
    let text = "hello"
    let length = text.length
    let copy = to_string text
    length + copy.length
```
Read what you need before the move, or pass a borrow (`ref text`) where the callee accepts one.

## See also
- `move_out_of_collection_element`
- `borrow_conflict`
`````

## A.2 `src/explain/borrow_conflict.md`

`````markdown
# Access that conflicts with a live borrow

## Why
A `ref` borrow keeps the borrowed place read-only until the borrow's last use. Assigning to the place, moving it, or borrowing it exclusively before that last use is a conflict (`E1014`).

## Bad
```tsuzuri
def f :: unit -> i64 = \_ ->
    let mut n = 1
    let r = ref n
    n = 2
    deref r
```

## Good
```tsuzuri
def f :: unit -> i64 = \_ ->
    let mut n = 1
    n = 2
    let r = ref n
    deref r
```
Finish the write before taking the borrow, or stop using the borrow before the write.

## See also
- `mutate_immutable_value`
- `mutate_through_shared_reference`
- `use_after_move`
`````

## A.3 `src/explain/mutate_immutable_value.md`

`````markdown
# Mutation of a binding that is not mutable

## Why
A `let` binding is immutable. Assigning to it, or borrowing it with `ref mut`, needs `let mut` (or an exclusive reference parameter) (`E1014`).

## Bad
```tsuzuri
def f :: unit -> i64 = \_ ->
    let x = 1
    x = 2
    x
```

## Good
```tsuzuri
def f :: unit -> i64 = \_ ->
    let mut x = 1
    x = 2
    x
```

## See also
- `borrow_conflict`
- `assign_to_immutable_place`
`````

## A.4 `src/explain/type_mismatch.md`

`````markdown
# Type mismatch

## Why
Every expression has one type, and the place that uses it expects another. The message names the expected type and the type that was found (`E1003`). Tsuzuri never converts between types implicitly.

## Bad
```tsuzuri
def add :: i64 -> i64 = \n -> n + true
```

## Good
```tsuzuri
def add :: i64 -> i64 = \n -> n + 1
```
Change the expression to the expected type, or convert explicitly (for example with `as`).

## See also
- `call_non_function`
- `integer_literal_out_of_range`
`````

## A.5 `src/explain/unknown_value.md`

`````markdown
# Unknown value

## Why
The name is not bound in the current scope: it is misspelled, declared later in a block, private to another module, or never declared (`E1002`). A name that lives in another module is written `Module.name`.

## Bad
```tsuzuri
def f :: i64 -> i64 = \n -> n + m
```

## Good
```tsuzuri
def f :: i64 -> i64 = \n ->
    let m = 1
    n + m
```

## See also
- `unknown_module_member`
- `unknown_function`
`````

## A.6 `src/explain/unknown_module_member.md`

`````markdown
# Unknown member of a module

## Why
`Module.name` names a function or union case that the module does not have (`E1002`). The usual causes are a misspelling and a name from another language's library (`String.len` instead of `String.length`).

## Bad
```tsuzuri
def f :: string -> i64 = \s -> String.len s
```

## Good
```tsuzuri
def f :: string -> i64 = \s -> String.length s
```
Check the module's functions with `tsuzuri doc`, or read `std/<Module>.tz`.

## See also
- `unknown_value`
`````

## A.7 `src/explain/non_exhaustive_match.md`

`````markdown
# Match that does not cover every case

## Why
A `match` must handle every value of the matched type. The message lists the cases that no arm covers (`E1021`). A wildcard arm `| _ -> ...` covers the rest, but it also hides cases that are added later.

## Bad
```tsuzuri
union Light = Red | Green
def f :: Light -> i64 = \l -> match l with | Red -> 1
```

## Good
```tsuzuri
union Light = Red | Green
def f :: Light -> i64 = \l -> match l with | Red -> 1 | Green -> 2
```

## See also
- `unreachable_match_arm`
`````

## A.8 `src/explain/borrow_outlives_block.md`

`````markdown
# Borrow that outlives its owner's block

## Why
A `ref` borrow cannot leave the block that owns the borrowed value: the value is dropped at the end of the block, and the borrow would point at nothing (`E1013`).

## Bad
```tsuzuri
def f :: unit -> i64 = \_ ->
    let r = { let x = 1; ref x }
    deref r
```

## Good
```tsuzuri
def f :: unit -> i64 = \_ ->
    let x = 1
    let r = ref x
    deref r
```
Move the owner out of the inner block, or return the value instead of a borrow.

## See also
- `borrow_of_temporary`
- `reference_without_owner`
`````

