# Seq

## `Seq`

```tsuzuri
record Seq<'a> {
  head: Option<'a>
  step: Option<(unit -> (Seq<'a> * Option<'a>))>
}
```

## `empty`

```tsuzuri
def empty :: Seq<'a>
```

## `once`

```tsuzuri
def once :: 'a -> Seq<'a>
```

## `defer`

```tsuzuri
def defer :: (unit -> (Seq<'a> * Option<'a>)) -> Seq<'a>
```

## `unfold`

```tsuzuri
def rec unfold :: Capture<'state> => ('state -> Option<('a * 'state)>) -> 'state -> Seq<'a>
```

## `map`

```tsuzuri
def rec map :: Capture<'a> => ('a -> 'b) -> Seq<'a> -> Seq<'b>
```

## `filter`

```tsuzuri
def rec filter :: Capture<'a> => (ref 'a -> bool) -> Seq<'a> -> Seq<'a>
```

## `to_array`

```tsuzuri
def to_array :: Seq<'a> -> ['a]
```

