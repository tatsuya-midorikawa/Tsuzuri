# Array

## `length`

```tsuzuri
def length :: ref ['a] -> i64
```

## `is_empty`

```tsuzuri
def is_empty :: ref ['a] -> bool
```

## `iter`

```tsuzuri
def iter :: ref ['a] -> Seq.Seq<ref 'a>
```

## `get`

```tsuzuri
def get :: Copy<'a> => ref ['a] -> i64 -> Option.Option<'a>
```

## `at`

```tsuzuri
def at :: ref ['a] -> i64 -> ref 'a
```

## `init`

```tsuzuri
def init :: i64 -> (i64 -> 'a) -> ['a]
```

## `sub`

```tsuzuri
def sub :: Copy<'a> => ref ['a] -> i64 -> i64 -> ['a]
```

## `reverse`

```tsuzuri
def reverse :: Copy<'a> => ref ['a] -> ['a]
```

## `copy`

```tsuzuri
def copy :: Copy<'a> => ref ['a] -> ['a]
```

Returns a new array with a copy of every element, the explicit form of an implicit array copy.

## `append`

```tsuzuri
def append :: Copy<'a> => ref ['a] -> ref ['a] -> ['a]
```

## `zip`

```tsuzuri
def zip :: ref ['a] -> ref ['b] -> [('a * 'b)]
```

## `map`

```tsuzuri
def map :: Copy<'a> => ref ['a] -> ('a -> 'b) -> ['b]
```

## `mapi`

```tsuzuri
def mapi :: Copy<'a> => ref ['a] -> (i64 -> 'a -> 'b) -> ['b]
```

## `map_ref`

```tsuzuri
def map_ref :: ref ['a] -> (ref 'a -> 'b) -> ['b]
```

## `mapi_ref`

```tsuzuri
def mapi_ref :: ref ['a] -> (i64 -> ref 'a -> 'b) -> ['b]
```

## `fold`

```tsuzuri
def fold :: Copy<'a> => ref ['a] -> 'state -> ('state -> 'a -> 'state) -> 'state
```

## `fold_ref`

```tsuzuri
def fold_ref :: ref ['a] -> 'state -> ('state -> ref 'a -> 'state) -> 'state
```

## `fold_back`

```tsuzuri
def fold_back :: Copy<'a> => ref ['a] -> 'state -> ('a -> 'state -> 'state) -> 'state
```

## `fold_back_ref`

```tsuzuri
def fold_back_ref :: ref ['a] -> 'state -> (ref 'a -> 'state -> 'state) -> 'state
```

## `reduce`

```tsuzuri
def reduce :: Copy<'a> => ref ['a] -> ('a -> 'a -> 'a) -> Option.Option<'a>
```

## `sum`

```tsuzuri
def sum :: Numeric<'a> => ref ['a] -> 'a
```

## `sum_pairwise`

```tsuzuri
def sum_pairwise :: Float<'a> => ref ['a] -> 'a
```

Sums adjacent pairs in a fixed tree, carrying an odd final element unchanged.
Uses one working copy. Empty input returns positive zero.

## `sum_kahan`

```tsuzuri
def sum_kahan :: Float<'a> => ref ['a] -> 'a
```

Computes the left-to-right Kahan-Babuska-Neumaier compensated sum.
Intermediate operations are not fused or reassociated.

## `dot`

```tsuzuri
def dot :: Float<'a> => ref ['a] -> ref ['a] -> 'a
```

Computes a left-to-right dot product with separate multiply and add rounding.
Unequal lengths trap before reading elements; empty inputs return positive zero.

## `dot_fma`

```tsuzuri
def dot_fma :: Float<'a> => ref ['a] -> ref ['a] -> 'a
```

Computes a left-to-right dot product with one rounding per explicit FMA step.
Unequal lengths trap before reading elements; empty inputs return positive zero.

## `product`

```tsuzuri
def product :: Numeric<'a> => ref ['a] -> 'a
```

## `min`

```tsuzuri
def min :: Ord<'a> => ref ['a] -> Option.Option<ref 'a>
```

## `max`

```tsuzuri
def max :: Ord<'a> => ref ['a] -> Option.Option<ref 'a>
```

## `any`

```tsuzuri
def any :: ref ['a] -> (ref 'a -> bool) -> bool
```

## `all`

```tsuzuri
def all :: ref ['a] -> (ref 'a -> bool) -> bool
```

## `count`

```tsuzuri
def count :: ref ['a] -> (ref 'a -> bool) -> i64
```

## `find`

```tsuzuri
def find :: ref ['a] -> (ref 'a -> bool) -> Option.Option<ref 'a>
```

## `index_of`

```tsuzuri
def index_of :: Eq<'a> => ref ['a] -> ref 'a -> Option.Option<i64>
```

## `contains`

```tsuzuri
def contains :: Eq<'a> => ref ['a] -> ref 'a -> bool
```

## `equal`

```tsuzuri
def equal :: Eq<'a> => ref ['a] -> ref ['a] -> bool
```

## `binary_search`

```tsuzuri
def binary_search :: Ord<'a> => ref ['a] -> ref 'a -> Option.Option<i64>
```

## `sort`

```tsuzuri
def sort :: Ord<'a> => ref ['a] -> ['a]
```

## `filter`

```tsuzuri
def filter :: Copy<'a> => ref ['a] -> (ref 'a -> bool) -> ['a]
```

