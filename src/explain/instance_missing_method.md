# Instance without every method of its class

## Why
An `instance` must define every method of its class that has no default. The message names the first missing one as `Class.method` (`E1016`). `Eq` has no defaults, so an instance needs both `eq` and `ne`.

## Bad
```tsuzuri
record Point { x: i64, y: i64 }
instance Eq<Point> {
    fn eq left right = left.x == right.x && left.y == right.y
}
```

## Good
```tsuzuri
record Point { x: i64, y: i64 }
instance Eq<Point> {
    fn eq left right = left.x == right.x && left.y == right.y
    fn ne left right = !(Eq.eq left right)
}
```
Add the missing method, or replace the instance with `deriving (Eq)` on the record.

## See also
- `instance_unknown_method`
- `overlapping_instance`
- `no_instance`
