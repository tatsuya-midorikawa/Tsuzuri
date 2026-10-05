# String

## `length`

```tsuzuri
def length :: ref string -> i64
```

## `find`

```tsuzuri
def find :: ref string -> ref string -> Option<i64>
```

## `rfind`

```tsuzuri
def rfind :: ref string -> ref string -> Option<i64>
```

## `contains`

```tsuzuri
def contains :: ref string -> ref string -> bool
```

## `starts_with`

```tsuzuri
def starts_with :: ref string -> ref string -> bool
```

## `ends_with`

```tsuzuri
def ends_with :: ref string -> ref string -> bool
```

## `slice`

```tsuzuri
def slice :: ref string -> i64 -> i64 -> Option<string>
```

## `sub`

```tsuzuri
def sub :: ref string -> i64 -> i64 -> Option<string>
```

## `decode_at`

```tsuzuri
def decode_at :: ref string -> i64 -> (char * i64)
```

## `char_count`

```tsuzuri
def char_count :: ref string -> i64
```

## `chars`

```tsuzuri
def chars :: ref string -> [char]
```

## `join`

```tsuzuri
def join :: ref string -> ref [string] -> string
```

## `concat`

```tsuzuri
def concat :: ref [string] -> string
```

## `split`

```tsuzuri
def split :: ref string -> ref string -> [string]
```

## `trim_start`

```tsuzuri
def trim_start :: ref string -> string
```

## `trim_end`

```tsuzuri
def trim_end :: ref string -> string
```

## `trim`

```tsuzuri
def trim :: ref string -> string
```

## `to_ascii_lower`

```tsuzuri
def to_ascii_lower :: ref string -> string
```

## `to_ascii_upper`

```tsuzuri
def to_ascii_upper :: ref string -> string
```

## `repeat`

```tsuzuri
def repeat :: ref string -> i64 -> string
```

## `replace`

```tsuzuri
def replace :: ref string -> ref string -> ref string -> string
```

