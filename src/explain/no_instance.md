# No instance of a type class for the type

## Why
An operator or function needs an instance of a type class, such as `Eq` for `==`, and the type has none (`E1005`). The message names the class and the type. A record or union gets an instance only from `deriving` or from an `instance` that you write.

## Bad
```tsuzuri
record Point { x: i64, y: i64 }
def same :: Point -> Point -> bool = \a b -> a == b
```

## Good
```tsuzuri
record Point { x: i64, y: i64 } deriving (Eq)
def same :: Point -> Point -> bool = \a b -> a == b
```
`deriving` covers `Eq`, `Ord`, `Display`, `Hash`, `Default`, `Encode` and `Decode`; for another class, write an `instance` with its methods.

## See also
- `instance_missing_method`
- `overlapping_instance`
