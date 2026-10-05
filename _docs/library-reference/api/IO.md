# IO

## `IO`

```tsuzuri
record IO<'a> {
  work: (unit -> 'a)
}
```

## `Error`

```tsuzuri
union Error =
  | ReadFailed
  | WriteFailed
  | InvalidEncoding
```

## `pure`

```tsuzuri
def pure :: Capture<'a> => 'a -> IO<'a>
```

## `bind`

```tsuzuri
def bind :: IO<'a> -> ('a -> IO<'b>) -> IO<'b>
```

## `map`

```tsuzuri
def map :: ('a -> 'b) -> IO<'a> -> IO<'b>
```

## `Return`

```tsuzuri
def Return :: Capture<'a> => 'a -> IO<'a>
```

## `ReturnFrom`

```tsuzuri
def ReturnFrom :: IO<'a> -> IO<'a>
```

## `Bind`

```tsuzuri
def Bind :: IO<'a> -> ('a -> IO<'b>) -> IO<'b>
```

## `Using`

```tsuzuri
def Using :: (('a -> 'b) -> 'c) -> ('a -> IO<'b>) -> IO<'c>
```

## `Zero`

```tsuzuri
def Zero :: IO<unit>
```

## `Delay`

```tsuzuri
def Delay :: (unit -> IO<'a>) -> IO<'a>
```

## `Combine`

```tsuzuri
def Combine :: IO<unit> -> IO<'a> -> IO<'a>
```

## `For`

```tsuzuri
def For :: (Copy<'a>, Capture<'a>) => ['a] -> ('a -> IO<unit>) -> IO<unit>
```

## `While`

```tsuzuri
def While :: (unit -> bool) -> IO<unit> -> IO<unit>
```

## `MergeSources`

```tsuzuri
def MergeSources :: IO<'a> -> IO<'b> -> IO<('a * 'b)>
```

## `BindReturn`

```tsuzuri
def BindReturn :: IO<'a> -> ('a -> 'b) -> IO<'b>
```

## `try_read_line`

```tsuzuri
def try_read_line :: unit -> IO<Result<Option<string>, Error>>
```

## `read_line`

```tsuzuri
def read_line :: unit -> IO<Option<string>>
```

## `try_write`

```tsuzuri
def try_write :: (Display<'a>, Capture<'a>) => 'a -> IO<Result<unit, Error>>
```

## `try_write_line`

```tsuzuri
def try_write_line :: (Display<'a>, Capture<'a>) => 'a -> IO<Result<unit, Error>>
```

## `try_write_error`

```tsuzuri
def try_write_error :: (Display<'a>, Capture<'a>) => 'a -> IO<Result<unit, Error>>
```

## `try_write_error_line`

```tsuzuri
def try_write_error_line :: (Display<'a>, Capture<'a>) => 'a -> IO<Result<unit, Error>>
```

## `write`

```tsuzuri
def write :: (Display<'a>, Capture<'a>) => 'a -> IO<unit>
```

## `write_line`

```tsuzuri
def write_line :: (Display<'a>, Capture<'a>) => 'a -> IO<unit>
```

## `writeln`

```tsuzuri
def writeln :: (Display<'a>, Capture<'a>) => 'a -> IO<unit>
```

The same action as `write_line`.

## `write_error`

```tsuzuri
def write_error :: (Display<'a>, Capture<'a>) => 'a -> IO<unit>
```

## `write_error_line`

```tsuzuri
def write_error_line :: (Display<'a>, Capture<'a>) => 'a -> IO<unit>
```

