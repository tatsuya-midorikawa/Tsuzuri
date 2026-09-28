# Seq

## `Seq`

```tsuzuri
record Seq<'a> {
  head: Option.Option<'a>
  step: Option.Option<(unit -> (Seq<'a> * Option.Option<'a>))>
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
def defer :: (unit -> (Seq<'a> * Option.Option<'a>)) -> Seq<'a>
```

## `unfold`

```tsuzuri
def rec unfold :: Capture<'state> => 'state -> ('state -> Option.Option<('a * 'state)>) -> Seq<'a>
```

## `map`

```tsuzuri
def rec map :: Capture<'a> => Seq<'a> -> ('a -> 'b) -> Seq<'b>
```

## `filter`

```tsuzuri
def rec filter :: Capture<'a> => Seq<'a> -> (ref 'a -> bool) -> Seq<'a>
```

## `to_array`

```tsuzuri
def to_array :: Seq<'a> -> ['a]
```

