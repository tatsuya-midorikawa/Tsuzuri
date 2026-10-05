# Format

## `Align`

```tsuzuri
union Align =
  | AlignAuto
  | AlignLeft
  | AlignCenter
  | AlignRight deriving (Eq)
```

How the text of a hole sits inside its width. `AlignAuto` means that the spec names no alignment.

## `Kind`

```tsuzuri
union Kind =
  | KindPlain
  | KindLowerHex
  | KindUpperHex
  | KindOctal
  | KindBinary
  | KindExponent
  | KindFixed deriving (Eq)
```

The `type` letter of a spec: `x`, `X`, `o`, `b`, `e`, `f`, or `KindPlain` when there is none.

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
def parse :: ref string -> Option<Spec>
```

Takes a spec apart; `None` when `text` is not written the way `$"{x:spec}"` allows: a fill the lexer refuses
(`{`, `}`, `"`, `\`, CR, LF, or half of a surrogate pair), a width that starts with 0, a precision with a leading
0 (`.05`), or a number above 4096. It checks the grammar only, so a type letter that does not fit the precision
(`.2x`) is not rejected. The text a `Format` instance receives is always valid.

## `pad`

```tsuzuri
def pad :: ref Spec -> string -> string
```

`text` widened to the width of `spec` with its fill, aligned as it says (`AlignAuto` is left). A text that is
already as wide is returned unchanged.

