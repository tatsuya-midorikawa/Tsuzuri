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
