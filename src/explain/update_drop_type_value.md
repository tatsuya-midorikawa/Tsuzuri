# Record update of a type with a Drop instance

## Why
A record update `{ value with field = ... }` takes the other fields out of `value`. When the record type has a `Drop` instance, its `drop` needs the value whole, so the update is rejected (`E1012`).

## Bad
```tsuzuri
record Resource { id: i64, name: string }
instance Drop<Resource> { fn drop _value = () }
def rename :: own Resource -> Resource = \value -> { value with name = "c" }
```

## Good
```tsuzuri
record Resource { id: i64, name: string }
instance Drop<Resource> { fn drop _value = () }
def rename :: own Resource -> Resource = \value -> Resource { id: value.id, name: "c" }
```
Construct a new record from the fields that you keep.

## See also
- `use_after_move`
