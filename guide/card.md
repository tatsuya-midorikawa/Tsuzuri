# Tsuzuri guide, level 0

Tsuzuri is statically typed, uses ownership instead of a GC, and compiles through LLVM to native code or WASM. One file is one module: `Main.tz` is the entry, `Shapes.tz` is module `Shapes` (call `Shapes.area`). A project from `tsuzuri new` starts `Main.tz` with `namespace Name`; keep that line first. The programs below and every error code named here are checked against this compiler. More: `tsuzuri guide --list`.

## Commands
- `tsuzuri check .` type-checks. With `--json` each diagnostic is one JSON line on stderr: `severity`, `code`, `message`, `path`, `span` (`line`, `column`, `end_line`, `end_column`), `kind` (the rule), and often `help`. Exit 1 on errors.
- `tsuzuri run .` builds and runs; `tsuzuri test .` runs `test "name" = assert (...)`; `tsuzuri fmt .` formats in place.

## A program
```tsuzuri run=24%2055%207
record Point { x: i64, y: i64 } deriving (Eq)

union Shape =
    | Circle of i64
    | Rect of i64 * i64

def area :: Shape -> i64 = \shape ->
    match shape with
    | Circle r -> 3 * r * r
    | Rect (w, h) -> w * h

def sum_to :: i64 -> i64 = \n ->
    let mut total = 0
    for i in 1 .. n do
        total = total + i
    total

def norm1 :: ref Point -> i64 = \p -> Int.abs p.x + Int.abs p.y

def main :: unit -> i32 = \() ->
    let p = Point { x: -3, y: 4 }
    let mut sum = 0
    for s in [Circle 2, Rect (3, 4)] do
        sum = sum + area s
    do! IO.writeln $"{sum} {sum_to 10} {norm1 p}"
    0

test "area" = assert (area (Circle 2) == 12)
```
- `def f :: A -> B -> C = \a b -> body`; call `f x y`. The last expression is the value (no `return`). Blocks are indented. Comments are `//`.
- `do! action` runs an effect statement; `let! line = IO.read_line ()` binds its result. `main :: unit -> i32` returns the exit code.
- `if c then a else b`, several lines: `elif`. `match v with | P -> e | _ -> d`. Lambda: `\x -> x + 1`. `let mut x = 0`, then `x = x + 1`.
- `for x in xs do`, `for i in a .. b do` (includes `b`), `while c do`, `break`, `continue`.
- Strings: `a + b`, `$"n = {n}"`, `s.length`, `to_string 5`. Arrays: `[1, 2]`, `xs[i]`, `xs.length`, `Array.map f xs`, slice `ref xs[1..3]` (half-open). Growable: `let mut v: Vec<i64> = Vec.empty()`, `v = Vec.push_owned 1 v`.
- Errors are values: `Maybe<T>` (`Some x`, `None`), `Result<T, E>` (`Ok x`, `Error e`). No `?` and no unwinding: `try ... with` only turns the `OverflowException` of `@checked` arithmetic in the same function into a `Result`.
- Std names are qualified (`Array.sum xs`, `Int.abs n`, `Math.sqrt x`). A def without parameters is called `f()`.

## Types and numbers
- `i8 i16 i32 i64 i128`, unsigned `i8u i16u i32u i64u i128u` (not `u8`), `f32 f64`, `bool char string unit`, `[T]`, `(A * B)`, `Maybe<T>`.
- A bare integer literal is `i32` and a float literal `f64`, unless the context says otherwise. No implicit conversion: `n as f64`. A literal beyond `i32` needs a type: `let big: i64 = 3000000000` (else E1009).
- Integer `+ - *` wrap around. `/` truncates toward zero. `/` or `%` by zero and `MIN / -1` trap. Floats are IEEE 754.
- `==` on a record or union needs `deriving (Eq)`.

## Ownership
```tsuzuri run=5
def len :: ref string -> i64 = \s -> s.length
def eat :: own string -> i64 = \s -> s.length
def bump :: ref mut i64 -> unit = \r -> deref r = deref r + 1

def main :: unit -> i32 = \() ->
    let s = "hi"
    let mut n = len s
    bump n
    n = n + eat s
    do! IO.writeln n
    0
```
1. A `string`, `Vec`, or record/union holding one has one owner. `let b = a`, passing `a` to an `own T` parameter, or returning it moves it; any later use of `a` is E1012. Numbers, `bool`, `char`, `ref T` and arrays of Copy values are Copy (an array copy is O(n)). Copy a string on purpose with `clone_string s`.
<!--if edition=2026-->
   Edition 2026: a plain parameter `T` also takes ownership (like `own T`).
<!--endif-->
<!--if edition=2027-->
   Edition 2027: a plain parameter `T` of such a type borrows it to read (like `ref T`); `mut T` borrows it to write.
<!--endif-->
2. To keep using a value, borrow it: parameter `ref T` reads, `ref mut T` writes. `f x` borrows automatically; `ref x` and `ref mut x` also work. `deref r` reads, `deref r = v` assigns.
3. Assignment and `ref mut` need `let mut` (else E1014). While a `ref mut` lives, any other use of the value is E1014. Many `ref` may coexist.
4. A reference cannot outlive its owner: returning `ref local` is E1013. Return the owned value.
5. Values are freed once at scope end. There is no free or delete; sharing is explicit (`Rc`, `Arc`).

## Pitfalls
- `pair.0` is E0002: `match pair with | (a, _) -> a`.
- `x += 1` is E0002: `x = x + 1`.
<!--if edition=2026-->
- `p.x = 1` and `xs[i] = v` are E1012 (fields and elements are immutable): `{ p with x = 1 }`, `xs = Array.update_at i v xs`.
<!--endif-->
<!--if edition=2027-->
- `p.x = 1` and `xs[i] = v` need a `let mut` owner or a `mut T` parameter (else E1014). `{ p with x = 1 }` works in every edition.
<!--endif-->
- `IO.writeln s` as a statement is E1003 (`expected unit, found IO<unit>`): `do! IO.writeln s`.
- `for i in 0 .. xs.length` compiles, then traps: `..` includes the end. Use `0 .. xs.length - 1`.
- `else if` on a new line is E0002: `elif`.
- `n + 1.5` and `"a" + 1` are E1003: `n as f64 + 1.5`, `$"a{n}"`.
- `new type to with test do in` are reserved words (E0002): rename.
- `length xs`, `abs n` are E1002/E1003: `xs.length`, `Int.abs n` (`abs` is f64 only).
- `Display.to_string x` is E1002: `to_string x`, or `Display.display x`.
- `u8`, `usize`, `String`, `Option`, `True` are E1004/E1002: `i8u`, `i64`, `string`, `Maybe`, `true`.
- `Vec.empty ()` is E1006: `Vec.empty()`. `main` must return `i32` (E1003).

## Workflow
1. Write a little, then `tsuzuri check . --json`. Do not write 100 lines blind.
2. Fix the first diagnostic by `code`, `span.line` and `message`, then check again; later ones are often consequences.
3. Add a `test` for each function; `tsuzuri test .` must pass. Run `tsuzuri fmt .` last.
4. Unsure of a rule: `tsuzuri guide --level 1` or `--topic NAME`. Std API: `tsuzuri guide std`, then `tsuzuri guide std Array`.
5. Unknown code or kind: `tsuzuri explain E1012` or `tsuzuri explain use_after_move`.
6. Mechanical fixes: `tsuzuri fix .` (`--dry-run` shows the diff first).
