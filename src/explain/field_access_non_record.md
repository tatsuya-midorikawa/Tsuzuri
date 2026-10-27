# Field access on a value that is not a record

## Why
`value.name` reads a field, so the value must be a record. The only other form is `.length` on an array, list, string, or utf8string (`E1007`). Tsuzuri has no methods: call the module function instead, as in `Array.map f xs`.

## Bad
```tsuzuri
def f :: [i64] -> [i64] = \xs -> xs.map(\x -> x + 1)
```

## Good
```tsuzuri
def f :: [i64] -> [i64] = \xs -> Array.map (\x -> x + 1) xs
```

## See also
- `unknown_field`
- `unknown_module_member`
