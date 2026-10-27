# Unknown field

## Why
`value.name` reads a field, and the record has no field of that name (`E1007`). The help lists the fields that the record has.

## Bad
```tsuzuri
record Point { x: i64, y: i64 }
def f :: Point -> i64 = \p -> p.z
```

## Good
```tsuzuri
record Point { x: i64, y: i64 }
def f :: Point -> i64 = \p -> p.x
```

## See also
- `record_has_no_field`
- `field_access_non_record`
