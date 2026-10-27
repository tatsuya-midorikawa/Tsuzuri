# Indexing a value that is not a collection

## Why
`value[i]` works on arrays, lists, `Vec`, fixed-size arrays, `string` and `utf8string` (`E1005`). Tuples, records, numbers and other values have no index. A tuple is taken apart with `match`; `pair.0` and `pair[0]` do not exist.

## Bad
```tsuzuri
def first :: (i64 * i64) -> i64 = \pair -> pair[0]
```

## Good
```tsuzuri
def first :: (i64 * i64) -> i64 = \pair -> match pair with | (a, _) -> a
```
Read a record's field with `value.field`.

## See also
- `field_access_non_record`
- `type_mismatch`
