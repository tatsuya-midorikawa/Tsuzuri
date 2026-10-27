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
