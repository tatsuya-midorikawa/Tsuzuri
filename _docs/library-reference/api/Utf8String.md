# Utf8String

## `length`

```tsuzuri
def length :: ref utf8string -> i64
```

## `find`

```tsuzuri
def find :: ref utf8string -> ref utf8string -> Option<i64>
```

## `rfind`

```tsuzuri
def rfind :: ref utf8string -> ref utf8string -> Option<i64>
```

## `contains`

```tsuzuri
def contains :: ref utf8string -> ref utf8string -> bool
```

## `starts_with`

```tsuzuri
def starts_with :: ref utf8string -> ref utf8string -> bool
```

## `ends_with`

```tsuzuri
def ends_with :: ref utf8string -> ref utf8string -> bool
```

## `slice`

```tsuzuri
def slice :: ref utf8string -> i64 -> i64 -> Option<utf8string>
```

## `sub`

```tsuzuri
def sub :: ref utf8string -> i64 -> i64 -> Option<utf8string>
```

## `char_count`

```tsuzuri
def char_count :: ref utf8string -> i64
```

## `chars`

```tsuzuri
def chars :: ref utf8string -> [utf8char]
```

## `join`

```tsuzuri
def join :: ref utf8string -> ref [utf8string] -> utf8string
```

## `concat`

```tsuzuri
def concat :: ref [utf8string] -> utf8string
```

## `split`

```tsuzuri
def split :: ref utf8string -> ref utf8string -> [utf8string]
```

## `trim_start`

```tsuzuri
def trim_start :: ref utf8string -> utf8string
```

## `trim_end`

```tsuzuri
def trim_end :: ref utf8string -> utf8string
```

## `trim`

```tsuzuri
def trim :: ref utf8string -> utf8string
```

## `to_ascii_lower`

```tsuzuri
def to_ascii_lower :: ref utf8string -> utf8string
```

## `to_ascii_upper`

```tsuzuri
def to_ascii_upper :: ref utf8string -> utf8string
```

## `repeat`

```tsuzuri
def repeat :: ref utf8string -> i64 -> utf8string
```

## `replace`

```tsuzuri
def replace :: ref utf8string -> ref utf8string -> ref utf8string -> utf8string
```

