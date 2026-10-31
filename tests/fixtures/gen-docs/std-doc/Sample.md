# Sample

Namespace: `std`

## `Sample`

```tsuzuri
record Sample {
  x: i64
}
```

## `a`

```tsuzuri
def a :: i64
```

@complexity O(1)

## `b`

```tsuzuri
def b :: i64 -> i64
```

Text.

@complexity amortized O(1), worst O(n)
@since 0.2.0
@see Sample.a
