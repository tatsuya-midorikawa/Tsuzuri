# Missing record fields

## Why
Building a record with `Type { field: value, ... }` must give every field a value; the message lists the fields left out (`E1007`).

## Bad
```tsuzuri
record Point { x: i64, y: i64 }
def origin :: unit -> Point = \_ -> Point { x: 0 }
```

## Good
```tsuzuri
record Point { x: i64, y: i64 }
def origin :: unit -> Point = \_ -> Point { x: 0, y: 0 }
```
To change some fields of an existing record, use an update: `{ p with x = 1 }`.

## See also
- `record_has_no_field`
