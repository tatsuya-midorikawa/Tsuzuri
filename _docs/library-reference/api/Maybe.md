# Maybe

Namespace: `std`

## `Maybe`

```tsuzuri
union Maybe<'a> =
  | None
  | Some of 'a
```

## `MergeSources`

```tsuzuri
def MergeSources :: Maybe<'a> -> Maybe<'b> -> Maybe<('a * 'b)>
```

## `BindReturn`

```tsuzuri
def BindReturn :: Maybe<'a> -> ('a -> 'b) -> Maybe<'b>
```

## `Bind2`

```tsuzuri
def Bind2 :: Maybe<'a> -> Maybe<'b> -> ('a -> 'b -> 'c) -> Maybe<'c>
```

## `is_some`

```tsuzuri
def is_some :: ref Maybe<'a> -> bool
```

## `is_none`

```tsuzuri
def is_none :: ref Maybe<'a> -> bool
```

## `get`

```tsuzuri
def get :: Maybe<'a> -> 'a
```

## `default_value`

```tsuzuri
def default_value :: 'a -> Maybe<'a> -> 'a
```

## `default_with`

```tsuzuri
def default_with :: (unit -> 'a) -> Maybe<'a> -> 'a
```

## `map`

```tsuzuri
def map :: ('a -> 'b) -> Maybe<'a> -> Maybe<'b>
```

## `map_ref`

```tsuzuri
def map_ref :: (ref 'a -> 'b) -> ref Maybe<'a> -> Maybe<'b>
```

## `bind`

```tsuzuri
def bind :: Maybe<'a> -> ('a -> Maybe<'b>) -> Maybe<'b>
```

## `bind_ref`

```tsuzuri
def bind_ref :: ref Maybe<'a> -> (ref 'a -> Maybe<'b>) -> Maybe<'b>
```

## `filter`

```tsuzuri
def filter :: (ref 'a -> bool) -> Maybe<'a> -> Maybe<'a>
```

## `or_else`

```tsuzuri
def or_else :: Maybe<'a> -> (unit -> Maybe<'a>) -> Maybe<'a>
```

## `to_result`

```tsuzuri
def to_result :: 'e -> Maybe<'a> -> Result<'a, 'e>
```

## `of_result`

```tsuzuri
def of_result :: Result<'a, 'e> -> Maybe<'a>
```

## `Return`

```tsuzuri
def Return :: 'a -> Maybe<'a>
```

## `ReturnFrom`

```tsuzuri
def ReturnFrom :: Maybe<'a> -> Maybe<'a>
```

## `Bind`

```tsuzuri
def Bind :: Maybe<'a> -> ('a -> Maybe<'b>) -> Maybe<'b>
```

## `Zero`

```tsuzuri
def Zero :: Maybe<unit>
```

## `Delay`

```tsuzuri
def Delay :: (unit -> Maybe<'a>) -> (unit -> Maybe<'a>)
```

## `Run`

```tsuzuri
def Run :: (unit -> Maybe<'a>) -> Maybe<'a>
```

## `Combine`

```tsuzuri
def Combine :: Maybe<unit> -> (unit -> Maybe<'a>) -> Maybe<'a>
```

## `For`

```tsuzuri
def For :: Copy<'a> => ['a] -> ('a -> Maybe<unit>) -> Maybe<unit>
```

## `While`

```tsuzuri
def While :: (unit -> bool) -> (unit -> Maybe<unit>) -> Maybe<unit>
```

