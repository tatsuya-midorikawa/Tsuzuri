# Tsuzuri guide: language

## Modules, namespaces, packages
- One file is one module, named after the file and starting uppercase (`point.tz` is E1011): `.tz` code, `.tt` type classes, `.tc` one computation builder. Every file below the root is loaded; the root `Main.tz` is the entry.
- A file's namespace is its first line `namespace A::B`, else the package namespace plus its folders. `::` joins namespaces and modules, `.` reaches members: `Geometry::Shape.area`; `using Geometry` allows `Shape.area`. `union Shape` in `Shape.tz` is the module's type: `Shape.Rect`.
- Declarations are public; `private` keeps one in its module. `export` is for C and WASM hosts. Std module names (`Array`, `IO`...) cannot name a root file.
- `Tsuzuri.toml`: `[package]` has `name`, `version`, `edition`, `namespace`. `[dependencies]` takes `geo = { path = "../geo" }`, `{ git = "URL", rev = "COMMIT" }` or `{ version = "1.2.3" }`; `tsuzuri fetch .` writes `Tsuzuri.lock`. Call a dependency by its namespace.
```tsuzuri project=geo file=Geometry/Shape.tz
union Shape =
    | Circle of i64
    | Rect of i64 * i64

def area :: ref Shape -> i64 = \s ->
    match s with
    | Circle r -> 3 * r * r
    | Rect (w, h) -> scale * w * h

private const scale: i64 = 1
```
```tsuzuri project=geo file=Main.tz run=15
using Geometry

def main :: unit -> i32 = \() ->
    let r = Shape.Rect (3, 4)
    do! IO.writeln (Shape.area r + Geometry::Shape.area (Shape.Circle 1))
    0
```

## Declarations
- `record Point { x: i64, y: i64 }`: build `Point { x: 1, y: 2 }`, read `p.x`, update `{ p with y = 5 }`. Generic: `record Pair<'a, 'b> { first: 'a, second: 'b }`.
- `union Color = | Red | Rgb of i64 * i64 * i64`: one payload per case (a tuple for several); unions may be recursive.
- `type Id = i64` is an alias, the same type. `const limit: i64 = 100` is computed at compile time.
- `deriving (Eq, Ord, Display, Hash, Default)` follows a record or union; `Ord` needs `Eq`.
- `Maybe<'a>` is `Some x` or `None`, `Result<'a, 'e>` is `Ok x` or `Error e`; the value comes last: `Maybe.map f m`, `Maybe.bind f m`, `Maybe.default_value d m`, `Result.map_error f r`. `Maybe.get` traps on `None`.
- There is no `let (a, b) = pair`: use `match` or a lambda pattern.

## Control flow and patterns
- `if c then a elif d then b else e` is an expression. Conditions are `bool` only.
- `match v with | p when c -> e | _ -> d` must be exhaustive (E1021). Patterns: literals, `_`, tuples, records `{ x = a }`, cases, lists `[|a|]` and `h :: t`, arrays `[a, b]`, `p | q`, `p as all`.
- Guard clauses right after `->` match the arguments; `where` holds shared values.
- `for x in xs do` walks arrays, lists, `Vec`, strings, ranges and `Seq`; `for i in a .. b do` includes `b`. `for i = a to b do` is E0002 in a body that uses `do!` or `let!`. `while c do`, `break`, `continue`.
- `def rec f` recurses (mutual: `and`). A direct self tail call becomes a loop unless the function takes a borrow (`ref T`) or a function value; other recursion uses the stack.
<!--if edition=2027-->
- Edition 2027: a bare non-Copy parameter is a borrow, so write `own T` where a tail call must loop.
<!--endif-->
```tsuzuri run=Point%20%7B%20x%3A%201%2C%20y%3A%205%20%7D%20true%20big
record Point { x: i64, y: i64 } deriving (Eq, Ord, Display)

def classify :: i64 -> string = \n ->
    | 0 -> "zero"
    | v when v > big -> "big"
    | otherwise -> "small"
    where
        big = 50

def main :: unit -> i32 = \() ->
    let p = Point { x: 1, y: 2 }
    let q = { p with y = 5 }
    do! IO.writeln $"{q} {p < q} {classify 70}"
    0
```

## Generics and type classes
- `'a` is a type variable. A local `let` function is not generic: use a `def`.
- Constraints: `def add :: Add<'a> => own 'a -> own 'a -> 'a = \x y -> x + y`; several: `(Add<'a>, Copy<'a>) =>`. A call without the instance is E1005.
<!--if edition=2027-->
- Edition 2027: a bare `'a` parameter borrows; write `own 'a` to consume or return it.
<!--endif-->
- Built-in classes: `Add Sub Mul Div Rem Neg Pow`, `Eq Ord`, `Display` (`to_string x`), `Hash`, `Default`, `Parse` (`let n: Maybe<i64> = Parse.parse (ref "42")`), `Drop`; markers `Copy Send Sync`.
- A class goes in a `.tt` file, instances in `.tz`; call a method via its module: `Shapes.Shape.area x`. `instance Add<P> { fn add a b = ... }` gives `+`; `instance Eq<'a> => Eq<Box<'a>>` is conditional. One instance per class and type.
- `dyn C` holds any value with a `C` instance (`Dyn.of v`).
```tsuzuri project=classes file=Shapes.tt
class Shape<'a> {
    def area :: ref 'a -> i64
}
```
```tsuzuri project=classes file=Main.tz run=10%2014
record Sq { side: i64 }
record Rect { w: i64, h: i64 }

instance Shapes.Shape<Sq> { fn area s = s.side * s.side }
instance Shapes.Shape<Rect> { fn area r = r.w * r.h }

def total :: Shapes.Shape<'a> => ref ['a] -> i64 = \xs ->
    let mut sum = 0
    for x in xs do
        sum = sum + Shapes.Shape.area x
    sum

def main :: unit -> i32 = \() ->
    let squares = [Sq { side: 1 }, Sq { side: 3 }]
    let mixed: [dyn Shapes.Shape] = [Dyn.of (Sq { side: 2 }), Dyn.of (Rect { w: 2, h: 5 })]
    do! IO.writeln $"{total squares} {total mixed}"
    0
```

## Computation expressions and effects
- Failures are values; a trap (`assert`, bounds, division by zero) stops the program. The one exception is the `OverflowException` of `@checked` arithmetic: `try @checked x + 1 with | e -> e` turns it into a `Result` in the same function; nothing unwinds across functions.
- `IO<T>` is an action that runs only when bound. In `main`, `do! a` runs `a` and `let! x = a` binds its result. `IO { ... }` builds an action to return.
- In a body returning `Maybe` or `Result` (or in `Maybe { }`, `Result { }`), `let! x = m` unwraps, the first `None` or `Error` skips the rest, and `return v` wraps.
- `return` only ends a builder block; it is not an early exit. `and!` runs left to right. `match! m with` unwraps, then matches.
- A `.tc` file is one builder (`Bind`, `Return`...). `Async { }` interleaves waits: `Async.run`, `Async.sleep`.

## Tasks and parallelism
- `task { ... }` is a lazy `Task<T>`; `Task.run t` waits and returns its result. A task runs once (again: E1012).
- `Task.parallel tasks` runs an array of tasks on the thread pool; results keep the input order.
- A task takes owned (moved) and Copy values; a captured reference is E1013. Share data with `Arc.share`, borrow with `Task.scope`, mutate through `Atomic` or `Mutex`.
- `Parallel.init n f`, `Parallel.map f xs`, `Parallel.reduce z f xs`, `Parallel.sum xs` split arrays into fixed chunks; call them with every argument.
```tsuzuri run=7%20is%20odd
def half :: i64 -> Result<i64, string> = \n ->
    if n % 2 == 0 then Ok (n / 2) else Error $"{n} is odd"

def quarter :: i64 -> Result<i64, string> = \n ->
    let! h = half n
    let! q = half h
    return q

def main :: unit -> i32 = \() ->
    let jobs = new [Task<i64>](4, \i -> task { return i * i })
    let squares = Task.run (Task.parallel jobs)
    match quarter (Array.sum squares) with
    | Ok q -> do! IO.writeln q
    | Error e -> do! IO.writeln e
    0
```

## Strings, characters, numbers
- `string` is UTF-16: `s.length` and `s[i]` (an `i16u`) count code units. `u8"..."` is a `utf8string` (bytes). `char` (`'a'`) is one UTF-16 unit, `utf8char` one scalar. Copy a string with `clone_string s`.
- `$"x = {x}, {f:.2}, {n:>4}"` interpolates (holes borrow; `{{` is a brace). `to_string v` uses `Display`.
- An integer literal is `i32` unless the context or a suffix decides: `5l` is `i64`, `5ul` `i64u`, `5uy` `i8u`, `1.5f` `f32`, `1.5` `f64`, `5I` `bigint`.
- `as` converts numbers only: narrowing wraps, float to integer truncates and saturates (NaN gives 0). For `char` use `Char.to_u16`.

## Std map
Call std functions qualified, subject last: `Map.insert k v m`. Signatures: `tsuzuri guide std Array`.
- `Array` (`[T]`), `List` (`[|T|]`): `init`, `map`, `filter`, `fold`, `sum`, `sort`, `rev`. `Vec`: `push_owned`, `add`, `pop`, `to_array`.
- `String`, `Utf8String`: `split`, `join`, `find`, `replace`, `sub`, `trim`. `Char`, `Unicode`.
- `Maybe`, `Result`, `Seq` (lazy). `Map`, `Set` (ordered), `HashMap`, `HashSet`: `insert`, `get` or `contains`, `remove`, `iter`.
- `Int` (`abs`, `min`, `checked_add`), `Math` (`sqrt`, `pi`), `BigInt`, `Random`.
- `IO`, `File` (`read_text`, `write_text`), `Dir`, `Path`, `Env` (`args`), `Process`, `Time`, `Json`, `Regex`, `Format`, `Debug`.
- `Task`, `Parallel`, `Async`, `Atomic`, `Mutex`, `Channel`, `Rc`, `Arc`, `Arena`, `Owned`; `Test`, `Gen`, `Bench`.
