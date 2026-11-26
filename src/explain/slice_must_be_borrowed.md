# Array slice that is not borrowed

## Why
A slice `xs[a..<b]` does not copy the elements: it borrows them. Write `ref xs[a..<b]` for a read-only slice or `ref mut xs[a..<b]` for an exclusive one (`E0002`). `..<` excludes `b` in every edition (`..=` includes `b`; bare `..` excludes `b` in editions 2026–2027 and includes `b` from edition 2028).

## Bad
```tsuzuri
def f :: [i64] -> i64 = \xs ->
    let part = xs[1..<3]
    part.length
```

## Good
```tsuzuri
def f :: [i64] -> i64 = \xs ->
    let part = ref xs[1..<3]
    part.length
```

## See also
- `slice_without_bounds`
