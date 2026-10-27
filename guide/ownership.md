# Tsuzuri guide: ownership

No GC: an owned value has one owner, a binding, and is freed when it leaves scope.

## Copy and non-Copy
- Copy: numbers, `bool`, `char`, `unit`, `ref T`, function values; records, tuples, `[T; N]`, non-recursive unions, `[T]` and `[|T|]` made of Copy parts only.
- Not Copy: `string`, `Vec`, `Map`, `Set`, `Rc`, `Arc`, `Task`, `ref mut T`, recursive unions, `Drop` types, and whatever holds one.
- Copying an array or list is O(n), unless the source is not read again (then it moves).

## Moves
- `let b = a`, returning `a`, storing it (field, collection, `task`) and passing it to `own T` move a non-Copy `a`; a later use of `a` is E1012.
<!--if edition=2026-->
- Edition 2026: a plain parameter `T` also moves. Use `ref T` to only read.
<!--endif-->
<!--if edition=2027-->
- Edition 2027: a plain parameter of a heap type (`string`, array, `Vec`, record that needs freeing), a bare `'a` or a function borrows like `ref T`; numbers pass by value.
<!--endif-->
- A move in one `if`/`match` branch, or in a loop body of an outer value, makes the next use E1012.
- `let t = r.title` moves one field; the Copy fields of `r` stay readable. A non-Copy element cannot move out of `xs[i]`: use `ref xs[i]`. `{ r with f = v }` consumes a non-Copy `r`. A lambda that names a non-Copy local moves it in (a later use is E1012); a `ref mut` borrow or a task cannot be captured (E1005).
- Fix E1012: borrow (`ref T`), copy (`clone_string s`, `Array.copy (ref xs)`), or move last.

## Borrows
- `ref x` reads; many may coexist. `ref mut x` writes, needs `let mut x` (else E1014), and excludes every other use of `x` until its last use (E1014). `deref r` reads, `deref r = v` writes.
- Calls borrow for you: `f x` for a `ref T` parameter, or with `let mut x` for `ref mut T`.
- Slices: `ref xs[a..b]` (half-open; `a..`, `..b`) is `ref [T]`. `ref mut xs[a..b]` (needs `let mut xs`) is `ref mut [T..]`, edited by `Array.write i v`, `Array.swap_in i j`, `Array.sort_in_place`, `Array.split_at_mut k`; passing `xs` gives the whole slice.
- The subject is lent last: `Array.write 0 (xs[1] * 10) xs` works, `xs |> Array.write 0 (xs[1] * 10)` is E1014. So are moving or reassigning a borrowed owner and writing through `ref T`. A lambda cannot capture `ref mut` (E1005).
- Non-Copy elements: `Array.mapr (\s -> s.length) names` and `Array.foldr` pass references.
```tsuzuri run=9%2030%2047
def total :: ref [i64] -> i64 = \xs -> Array.sum xs
def bump :: ref mut i64 -> unit = \r -> deref r = deref r + 1

def main :: unit -> i32 = \() ->
    let mut xs = [5, 3, 9, 1]
    let mut n = total (ref xs[0..2])
    bump n
    Array.sort_in_place (ref mut xs[0..])
    Array.write 0 (xs[1] * 10) xs
    do! IO.writeln $"{n} {xs[0]} {total xs}"
    0
```
<!--if edition=2026-->
- Edition 2026: fields and elements are immutable (`p.x = v`, `xs[i] = v` are E1012): rebind `p = { p with x = v }`, `xs = Array.update_at i v xs`. `Array.map` and `fold` need Copy elements.
<!--endif-->
<!--if edition=2027-->
- Edition 2027: through a `let mut` owner, a `ref mut T`, or a `mut T` parameter (it borrows to write), `p.x = v` and `xs[i] = v` assign in place. `Vec.push x v` mutates `v` and returns unit. `Array`/`List` `map`, `iter`, `fold` pass read views.
```tsuzuri run=42
record Counter { value: i64 }

def bump :: i64 -> mut Counter -> unit = \step c -> c.value = c.value + step

def main :: unit -> i32 = \() ->
    let mut c = Counter { value: 40 }
    bump 2 c
    do! IO.writeln c.value
    0
```
<!--endif-->

## Lifetimes and regions
- Returning `ref local` is E1013: return the value, or own it in an outer scope.
- A returned reference borrows every reference argument unless regions pick one: `def first {r s} :: ref {r} string -> ref {s} string -> ref {r} string`. A body that returns another borrow is E1013.
- Records may hold references: `record View {r} { text: ref {r} string }`. There is no `'static`.

## Sharing and freeing
- Drops run at scope end in reverse binding order, and on reassignment; not after a trap (no unwinding).
- `instance Drop<T> { fn drop v = ... }` adds cleanup to your record or union; `Owned.drop x` drops now.
- `Rc.share (ref rc)` adds an owner to an `Rc.new v`; `Rc.get (ref rc)` reads the value, which stays immutable unless it is an `Atomic` or `Mutex`. Across tasks use `Arc` (`Arc<Mutex<T>>`); `Rc` there is E1013.
- Graphs and cycles: an `Arena<T>` owns the values; Copy `Arena.Handle<T>` values point at them.

## Performance
- `tsuzuri check . --warn implicit-copy` flags implicit array and list copies (W1006): borrow, or copy with `Array.copy (ref xs)`.
- Rebinding reuses the buffer when the old value is not read again: `xs = Array.update_at i v xs`, `v = Vec.push_owned x v`. `Vec.with_capacity n` preallocates.
