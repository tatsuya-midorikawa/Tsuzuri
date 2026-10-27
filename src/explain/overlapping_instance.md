# Two instances of a class for the same type

## Why
A type has at most one instance of each class (`E1016`). A second `instance` for the same type, an `instance` next to a `deriving` of the same class, or an instance that matches one the standard library already provides (such as `Encode<i64>`) overlaps.

## Bad
```tsuzuri
record Point { x: i64, y: i64 } deriving (Eq)
instance Eq<Point> {
    fn eq left right = left.x == right.x
    fn ne left right = left.x != right.x
}
```

## Good
```tsuzuri
record Point { x: i64, y: i64 }
instance Eq<Point> {
    fn eq left right = left.x == right.x
    fn ne left right = left.x != right.x
}
```
Keep one of them: remove the class from `deriving`, or delete the hand-written instance.

## See also
- `instance_missing_method`
- `no_instance`
