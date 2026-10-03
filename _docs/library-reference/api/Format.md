# Format

## `Align`

```tsuzuri
union Align =
  | Auto
  | Left
  | Center
  | Right deriving (Eq)
```

How the text of a hole sits inside its width. `Auto` means that the spec names no alignment.

## `Kind`

```tsuzuri
union Kind =
  | Plain
  | LowerHex
  | UpperHex
  | Octal
  | Binary
  | Exponent
  | Fixed deriving (Eq)
```

The `type` letter of a spec: `x`, `X`, `o`, `b`, `e`, `f`, or `Plain` when there is none.

## `Spec`

```tsuzuri
record Spec {
  fill: string
  align: Align
  plus: bool
  width: i64
  precision: i64
  kind: Kind
}
```

A format spec `[[fill]align][+][width][.precision][type]` taken apart. `fill` is one Unicode scalar, `width`
is 0 when the spec has none, and `precision` is -1 when it has none.

## `parse`

```tsuzuri
def parse :: ref string -> Option.Option<Spec>
```

Takes a spec apart; `None` when `text` is not a spec of the grammar `$"{x:spec}"` accepts. The text a
`Format` instance receives is always valid.

## `pad`

```tsuzuri
def pad :: ref Spec -> string -> string
```

`text` widened to the width of `spec` with its fill, aligned as it says (`Auto` is left). A text that is
already as wide is returned unchanged.

