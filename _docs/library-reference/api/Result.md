# Result

Namespace: `std`

## `Result`

```tsuzuri
union Result<'a, 'e> =
  | Ok of 'a
  | Error of 'e
```

## `MergeSources`

```tsuzuri
def MergeSources :: Result<'a, 'e> -> Result<'b, 'e> -> Result<('a * 'b), 'e>
```

## `BindReturn`

```tsuzuri
def BindReturn :: Result<'a, 'e> -> ('a -> 'b) -> Result<'b, 'e>
```

## `Bind2`

```tsuzuri
def Bind2 :: Result<'a, 'e> -> Result<'b, 'e> -> ('a -> 'b -> 'c) -> Result<'c, 'e>
```

## `is_ok`

```tsuzuri
def is_ok :: ref Result<'a, 'e> -> bool
```

## `is_error`

```tsuzuri
def is_error :: ref Result<'a, 'e> -> bool
```

## `get`

```tsuzuri
def get :: Result<'a, 'e> -> 'a
```

## `get_error`

```tsuzuri
def get_error :: Result<'a, 'e> -> 'e
```

## `default_value`

```tsuzuri
def default_value :: 'a -> Result<'a, 'e> -> 'a
```

## `default_with`

```tsuzuri
def default_with :: (unit -> 'a) -> Result<'a, 'e> -> 'a
```

## `map`

```tsuzuri
def map :: ('a -> 'b) -> Result<'a, 'e> -> Result<'b, 'e>
```

## `map_ref`

```tsuzuri
def map_ref :: Copy<'e> => (ref 'a -> 'b) -> ref Result<'a, 'e> -> Result<'b, 'e>
```

## `map_error`

```tsuzuri
def map_error :: ('e -> 'f) -> Result<'a, 'e> -> Result<'a, 'f>
```

## `bind`

```tsuzuri
def bind :: Result<'a, 'e> -> ('a -> Result<'b, 'e>) -> Result<'b, 'e>
```

## `bind_ref`

```tsuzuri
def bind_ref :: Copy<'e> => ref Result<'a, 'e> -> (ref 'a -> Result<'b, 'e>) -> Result<'b, 'e>
```

## `or_else`

```tsuzuri
def or_else :: Result<'a, 'e> -> (unit -> Result<'a, 'e>) -> Result<'a, 'e>
```

## `to_maybe`

```tsuzuri
def to_maybe :: Result<'a, 'e> -> Maybe<'a>
```

## `of_maybe`

```tsuzuri
def of_maybe :: 'e -> Maybe<'a> -> Result<'a, 'e>
```

## `Return`

```tsuzuri
def Return :: 'a -> Result<'a, 'e>
```

## `ReturnFrom`

```tsuzuri
def ReturnFrom :: Result<'a, 'e> -> Result<'a, 'e>
```

## `Bind`

```tsuzuri
def Bind :: Result<'a, 'e> -> ('a -> Result<'b, 'e>) -> Result<'b, 'e>
```

## `Zero`

```tsuzuri
def Zero :: Result<unit, 'e>
```

## `Delay`

```tsuzuri
def Delay :: (unit -> Result<'a, 'e>) -> (unit -> Result<'a, 'e>)
```

## `Run`

```tsuzuri
def Run :: (unit -> Result<'a, 'e>) -> Result<'a, 'e>
```

## `Combine`

```tsuzuri
def Combine :: Result<unit, 'e> -> (unit -> Result<'a, 'e>) -> Result<'a, 'e>
```

## `For`

```tsuzuri
def For :: Copy<'a> => ['a] -> ('a -> Result<unit, 'e>) -> Result<unit, 'e>
```

## `While`

```tsuzuri
def While :: (unit -> bool) -> (unit -> Result<unit, 'e>) -> Result<unit, 'e>
```

