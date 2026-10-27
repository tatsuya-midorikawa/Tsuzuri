# Record has no such field

## Why
A record construction `Point { ... }`, a record update `{ p with ... }`, or a record pattern names a field that the record type does not declare (`E1007`). The help lists the declared fields.

## Bad
```tsuzuri
record Point { x: i64, y: i64 }
def origin :: unit -> Point = \_ -> Point { x: 0, y: 0, z: 0 }
```

## Good
```tsuzuri
record Point { x: i64, y: i64 }
def origin :: unit -> Point = \_ -> Point { x: 0, y: 0 }
```

## See also
- `unknown_field`
- `missing_record_fields`
