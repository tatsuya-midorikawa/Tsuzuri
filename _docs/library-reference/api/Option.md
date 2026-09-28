# Option

## `Option`

```tsuzuri
union Option<'a> =
  | None
  | Some of 'a
```

## `MergeSources`

```tsuzuri
def MergeSources :: Option<'a> -> Option<'b> -> Option<('a * 'b)>
```

## `BindReturn`

```tsuzuri
def BindReturn :: Option<'a> -> ('a -> 'b) -> Option<'b>
```

## `Bind2`

```tsuzuri
def Bind2 :: Option<'a> -> Option<'b> -> ('a -> 'b -> 'c) -> Option<'c>
```

## `is_some`

```tsuzuri
def is_some :: ref Option<'a> -> bool
```

## `is_none`

```tsuzuri
def is_none :: ref Option<'a> -> bool
```

## `get`

```tsuzuri
def get :: Option<'a> -> 'a
```

## `default_value`

```tsuzuri
def default_value :: 'a -> Option<'a> -> 'a
```

## `default_with`

```tsuzuri
def default_with :: (unit -> 'a) -> Option<'a> -> 'a
```

## `map`

```tsuzuri
def map :: ('a -> 'b) -> Option<'a> -> Option<'b>
```

## `map_ref`

```tsuzuri
def map_ref :: (ref 'a -> 'b) -> ref Option<'a> -> Option<'b>
```

## `bind`

```tsuzuri
def bind :: Option<'a> -> ('a -> Option<'b>) -> Option<'b>
```

## `bind_ref`

```tsuzuri
def bind_ref :: ref Option<'a> -> (ref 'a -> Option<'b>) -> Option<'b>
```

## `filter`

```tsuzuri
def filter :: (ref 'a -> bool) -> Option<'a> -> Option<'a>
```

## `or_else`

```tsuzuri
def or_else :: Option<'a> -> (unit -> Option<'a>) -> Option<'a>
```

## `to_result`

```tsuzuri
def to_result :: 'e -> Option<'a> -> Result.Result<'a, 'e>
```

## `of_result`

```tsuzuri
def of_result :: Result.Result<'a, 'e> -> Option<'a>
```

## `Return`

```tsuzuri
def Return :: 'a -> Option<'a>
```

## `ReturnFrom`

```tsuzuri
def ReturnFrom :: Option<'a> -> Option<'a>
```

## `Bind`

```tsuzuri
def Bind :: Option<'a> -> ('a -> Option<'b>) -> Option<'b>
```

## `Zero`

```tsuzuri
def Zero :: Option<unit>
```

## `Delay`

```tsuzuri
def Delay :: (unit -> Option<'a>) -> (unit -> Option<'a>)
```

## `Run`

```tsuzuri
def Run :: (unit -> Option<'a>) -> Option<'a>
```

## `Combine`

```tsuzuri
def Combine :: Option<unit> -> (unit -> Option<'a>) -> Option<'a>
```

## `For`

```tsuzuri
def For :: Copy<'a> => ['a] -> ('a -> Option<unit>) -> Option<unit>
```

## `While`

```tsuzuri
def While :: (unit -> bool) -> (unit -> Option<unit>) -> Option<unit>
```

