# F14 付録: 確保の契約の fixture・std の `Alloc`・計数 heap の全文と、HEAD での実測

本文は [F14-structural-performance-assertions.md](F14-structural-performance-assertions.md)。この付録の A〜C は、**そのまま写して使う**全文（実装者は文面を発明しない）。
D〜G は測定と出典。測定はすべて 2026-10-18、Apple M1 Max（macOS）、HEAD `a2313d19`（作業ツリーは `main` の `9dd0fbbc`。差は `docs/benchmarks.md` だけ）の
`target/release/tsuzuri` と、試作（付録 G）で行った。

## 付録 A: `tests/fixtures/alloc_contracts/Main.tz`（42 件。そのまま置く）

試作の `tsuzuri test <この fixture>` が、native と wasm32 の `-O0`・`-O3` の 4 構成すべてで `42 passed; 0 failed; 0 ignored` になった（付録 D.3）。
各テストの直前の `// Rule:` は、期待値を導いた規則（出典は付録 E の表）で、コンパイラの出力を写したものではない。

```tsuzuri
// Structural allocation contracts of the standard library (F14).
//
// Every expected number comes from a rule of the language or of the library (the rule is written
// above each test), not from a run of the compiler. Each test measures a window with
// Alloc.snapshot and Alloc.since, then checks it with `expect`. The windows hold only the
// operation under test: no Debug.print and no interpolation inside a window. The byte counts hold
// for element types without pointers (i64, i32, UTF-16 text), so they are the same on native and
// wasm32; the programs whose byte counts depend on the size of a pointer use `expect_events`.
// A count that changes on purpose is changed here, in the same change as the code that moved it.

record Point { x: i64, y: i64 }
union Shape = Circle of i64 | Rect of Point
union Tree = Leaf | Node of Tree * i64 * Tree

/// Fails unless the four counters are exactly the expected ones. A failing native run prints
/// them first; the WebAssembly test runner has no console, so the test name is all it shows.
def expect :: string -> Alloc.Stats -> i64 -> i64 -> i64 -> i64 -> unit
fn expect label stats allocs reallocs frees bytes =
    let same = stats.allocs == allocs && stats.reallocs == reallocs && stats.frees == frees && stats.bytes == bytes
    if !same then Debug.print $"{label}: allocs {stats.allocs}/{allocs} reallocs {stats.reallocs}/{reallocs} frees {stats.frees}/{frees} bytes {stats.bytes}/{bytes} (got/want)" else ()
    assert same

/// Like `expect` for the programs whose byte counts depend on the size of a pointer.
def expect_events :: string -> Alloc.Stats -> i64 -> i64 -> i64 -> unit
fn expect_events label stats allocs reallocs frees =
    let same = stats.allocs == allocs && stats.reallocs == reallocs && stats.frees == frees
    if !same then Debug.print $"{label}: allocs {stats.allocs}/{allocs} reallocs {stats.reallocs}/{reallocs} frees {stats.frees}/{frees} (got/want)" else ()
    assert same

def area :: ref Shape -> i64
fn area shape =
    match shape with
    | Circle r -> 3 * r * r
    | Rect p -> p.x * p.y

def rec count_nodes :: ref Tree -> i64
fn rec count_nodes tree =
    match tree with
    | Leaf -> 0
    | Node (left, _, right) -> 1 + count_nodes left + count_nodes right

def make_and_drop :: unit -> i64
fn make_and_drop _unit =
    let xs = new [i64](10, \i -> i)
    Array.length (ref xs)

def build_and_drop_tree :: unit -> i64
fn build_and_drop_tree _unit =
    let tree = Node (Node (Leaf, 2, Leaf), 1, Node (Leaf, 3, Leaf))
    count_nodes (ref tree)

// --- Array ---

// Rule: `new [T](n, f)` takes one block of n elements (array.md). 10 x 8 bytes.
test "new array: one block of n elements" =
    let start = Alloc.snapshot ()
    let xs = new [i64](10, \i -> i)
    let used = Alloc.since start
    expect "new [i64](10)" used 1 0 0 80
    assert (Array.length (ref xs) == 10)

// Rule: an empty array still asks for one byte (`allocate_array` in src/llvm.rs).
test "new array of length 0: one block of one byte" =
    let start = Alloc.snapshot ()
    let xs = new [i64](0, \i -> i)
    let used = Alloc.since start
    expect "new [i64](0)" used 1 0 0 1
    assert (Array.length (ref xs) == 0)

// Rule: a literal without `new` lives on the stack (stack-and-heap.md).
test "array literal without new: nothing on the heap" =
    let start = Alloc.snapshot ()
    let xs = [1i64, 2i64, 3i64]
    let total = xs[0] + xs[1] + xs[2]
    let used = Alloc.since start
    expect "[1, 2, 3]" used 0 0 0 0
    assert (total == 6)

// Rule: `new [..]` puts the elements in one heap block (stack-and-heap.md). 3 x 8 bytes.
test "array literal with new: one block" =
    let start = Alloc.snapshot ()
    let xs = new [1i64, 2i64, 3i64]
    let used = Alloc.since start
    expect "new [1, 2, 3]" used 1 0 0 24
    assert (Array.length (ref xs) == 3)

// Rule: `[]` is a null pointer with length 0; only `new []` allocates, one byte (stack-and-heap.md).
test "empty array: [] has no block, new [] has one byte" =
    let before_empty = Alloc.snapshot ()
    let empty: [i64] = []
    let used_empty = Alloc.since before_empty
    expect "[]" used_empty 0 0 0 0
    let before_new = Alloc.snapshot ()
    let heap_empty: [i64] = new []
    let used_new = Alloc.since before_new
    expect "new []" used_new 1 0 0 1
    assert (Array.length (ref empty) == 0 && Array.length (ref heap_empty) == 0)

// Rule: `Array.map` returns a new array: one block of the result. 10 x 8 bytes.
test "Array.map: one result block" =
    let xs = new [i64](10, \i -> i)
    let start = Alloc.snapshot ()
    let ys = Array.map (\x -> x * 3) (ref xs)
    let used = Alloc.since start
    expect "Array.map" used 1 0 0 80
    assert (ys[9] == 27)

// Rule: `Array.copy` returns a new array of the same length. 10 x 8 bytes.
test "Array.copy: one block of the same size" =
    let xs = new [i64](10, \i -> i)
    let start = Alloc.snapshot ()
    let ys = Array.copy (ref xs)
    let used = Alloc.since start
    expect "Array.copy" used 1 0 0 80
    assert (ys[9] == 9)

// Rule: `Array.append` returns one new array of both lengths. (10 + 5) x 8 bytes.
test "Array.append: one block of both lengths" =
    let a = new [i64](10, \i -> i)
    let b = new [i64](5, \i -> i)
    let start = Alloc.snapshot ()
    let c = Array.append (ref a) (ref b)
    let used = Alloc.since start
    expect "Array.append" used 1 0 0 120
    assert (Array.length (ref c) == 15)

// Rule: `Array.reverse` returns a new array of the same length. 50 x 8 bytes.
test "Array.reverse: one block of the same size" =
    let xs = new [i64](50, \i -> i)
    let start = Alloc.snapshot ()
    let ys = Array.reverse (ref xs)
    let used = Alloc.since start
    expect "Array.reverse" used 1 0 0 400
    assert (ys[0] == 49)

// Rule: shared borrows, indexing, `length`, and `for` read in place (stack-and-heap.md).
test "reading an array allocates nothing" =
    let xs = new [i64](100, \i -> i)
    let start = Alloc.snapshot ()
    let total = Array.sum (ref xs)
    let folded = Array.fold (\acc x -> acc + x) 0 (ref xs)
    let first = xs[0] + Array.length (ref xs)
    let mut looped = 0
    for i in 0i64 .. 99i64 do looped = looped + xs[i]
    let used = Alloc.since start
    expect "reads" used 0 0 0 0
    assert (total == 4950 && folded == 4950 && first == 100 && looped == 4950)

// Rule: a slice `ref xs[a..b]` borrows a range of the array (array.md).
test "a slice borrows: no allocation" =
    let xs = new [i64](100, \i -> i)
    let start = Alloc.snapshot ()
    let part = ref xs[10..20]
    let total = Array.sum part
    let used = Alloc.since start
    expect "slice" used 0 0 0 0
    assert (total == 145)

// Rule: the owner frees its block when it goes out of scope (ownership).
test "dropping an array frees its block" =
    let start = Alloc.snapshot ()
    let length = make_and_drop ()
    let used = Alloc.since start
    expect "make and drop" used 1 0 1 80
    assert (length == 10)

// Rule: at the last use of an owned array, `Array.update_at` reuses the buffer (array.md, ownership.md).
test "Array.update_at on a uniquely owned array: in place" =
    let xs = new [i64](4, \i -> i)
    let start = Alloc.snapshot ()
    let ys = Array.update_at 0 5 xs
    let used = Alloc.since start
    expect "Array.update_at" used 0 0 0 0
    assert (ys[0] == 5)

// Rule: `xs = Array.update_at i v xs` in a loop is a last use too (ownership.md).
test "Array.update_at in a loop that rebinds the array: in place" =
    let mut xs = new [i64](100, \_i -> 0)
    let start = Alloc.snapshot ()
    for i in 0i64 .. 99i64 do
        xs = Array.update_at i (i * 2) xs
    let used = Alloc.since start
    expect "Array.update_at loop" used 0 0 0 0
    let last = xs[99]
    assert (last == 198)

// Rule: a local that is borrowed anywhere in the function is excluded from the last-use move
// (ownership.md), so each of the 10 updates copies 100 x 8 bytes and frees the old array.
test "Array.update_at in a loop on an array that is borrowed elsewhere: one copy per update" =
    let mut xs = new [i64](100, \_i -> 0)
    let start = Alloc.snapshot ()
    for i in 0i64 .. 9i64 do
        xs = Array.update_at i 1 xs
    let used = Alloc.since start
    expect "Array.update_at loop with a borrow" used 10 0 10 8000
    assert (Array.length (ref xs) == 100)

// Rule: binding an array to a second name and using both copies the elements (ownership.md).
test "using an array after binding it elsewhere: one implicit copy" =
    let xs = new [i64](10, \i -> i)
    let start = Alloc.snapshot ()
    let ys = xs
    let total = Array.sum (ref xs) + Array.sum (ref ys)
    let used = Alloc.since start
    expect "implicit copy" used 1 0 0 80
    assert (total == 90)

// Rule: the result of `Array.copy` is a unique temporary, so `Array.update_at` updates it in place.
test "Array.update_at on a fresh copy: the copy only" =
    let xs = new [i64](10, \i -> i)
    let start = Alloc.snapshot ()
    let ys = Array.update_at 0 9 (Array.copy (ref xs))
    let used = Alloc.since start
    expect "Array.update_at on copy" used 1 0 0 80
    assert (ys[0] == 9 && xs[0] == 0)

// Rule: `Array.filter` pushes into a Vec (capacity 4, then doubling) and `Vec.to_array` hands over
// the buffer. 25 kept elements: capacity 4, 8, 16, 32 = one block of 32 bytes and 3 resizes
// to 64, 128, and 256 bytes: 32 + 64 + 128 + 256 = 480.
test "Array.filter: collects through a Vec that doubles from 4" =
    let xs = new [i64](50, \i -> i)
    let start = Alloc.snapshot ()
    let ys = Array.filter (\x -> deref x % 2 == 0) (ref xs)
    let used = Alloc.since start
    expect "Array.filter" used 1 3 0 480
    assert (Array.length (ref ys) == 25)

// Rule: the doc comment of `Array.sort_in_place` promises a sort without allocating.
test "Array.sort_in_place: no allocation" =
    let mut xs = new [i64](100, \i -> 100 - i)
    let start = Alloc.snapshot ()
    Array.sort_in_place (ref mut xs)
    let used = Alloc.since start
    expect "Array.sort_in_place" used 0 0 0 0
    assert (xs[0] == 1 && xs[99] == 100)

// --- Vec ---

// Rule: `Vec.empty()` has no buffer (vec.md).
test "Vec.empty: no block" =
    let start = Alloc.snapshot ()
    let v: Vec<i64> = Vec.empty()
    let used = Alloc.since start
    expect "Vec.empty" used 0 0 0 0
    assert (Vec.length (ref v) == 0)

// Rule: the capacity of a Vec starts at 4 (vec.md). 4 x 8 bytes.
test "Vec.push: the first push allocates four elements" =
    let start = Alloc.snapshot ()
    let mut v: Vec<i64> = Vec.empty()
    for i in 1i64 .. 4i64 do v = Vec.push i v
    let used = Alloc.since start
    expect "4 pushes" used 1 0 0 32
    assert (Vec.length (ref v) == 4)

// Rule: the capacity doubles when it is full (vec.md): the fifth push resizes 32 to 64 bytes.
test "Vec.push: the fifth push doubles the capacity by realloc" =
    let start = Alloc.snapshot ()
    let mut v: Vec<i64> = Vec.empty()
    for i in 1i64 .. 5i64 do v = Vec.push i v
    let used = Alloc.since start
    expect "5 pushes" used 1 1 0 96
    assert (Vec.capacity (ref v) == 8)

// Rule: capacities 4, 8, 16, 32, 64, 128 = one block and 5 resizes: 8 x (4+8+16+32+64+128) = 2016.
test "Vec.push: 100 pushes realloc five times" =
    let start = Alloc.snapshot ()
    let mut v: Vec<i64> = Vec.empty()
    for i in 1i64 .. 100i64 do v = Vec.push i v
    let used = Alloc.since start
    expect "100 pushes" used 1 5 0 2016
    assert (Vec.capacity (ref v) == 128)

// Rule: `Vec.with_capacity n` reserves n elements at once (vec.md). 100 x 8 bytes.
test "Vec.with_capacity: no realloc while the capacity lasts" =
    let start = Alloc.snapshot ()
    let mut v: Vec<i64> = Vec.with_capacity 100
    for i in 1i64 .. 100i64 do v = Vec.push i v
    let used = Alloc.since start
    expect "with_capacity 100" used 1 0 0 800
    assert (Vec.length (ref v) == 100)

// Rule: `Vec.to_array` hands the buffer to the array (the rule that `Array.filter` relies on).
test "Vec.to_array: moves the buffer" =
    let mut v: Vec<i64> = Vec.empty()
    for i in 1i64 .. 100i64 do v = Vec.push i v
    let start = Alloc.snapshot ()
    let xs = Vec.to_array v
    let used = Alloc.since start
    expect "Vec.to_array" used 0 0 0 0
    assert (Array.length (ref xs) == 100)

// Rule: `length` and `capacity` read the metadata (vec.md).
test "Vec.length and Vec.capacity: no allocation" =
    let mut v: Vec<i64> = Vec.empty()
    for i in 1i64 .. 10i64 do v = Vec.push i v
    let start = Alloc.snapshot ()
    let length = Vec.length (ref v)
    let capacity = Vec.capacity (ref v)
    let used = Alloc.since start
    expect "Vec.length" used 0 0 0 0
    assert (length == 10 && capacity == 16)

// Rule: 3 element arrays and the buffer of the Vec (capacity 4 holds 3 pushes) are 4 blocks;
// the size of a Vec element (an array descriptor) depends on the pointer size.
test "Vec of arrays: one block per array and one for the Vec" =
    let start = Alloc.snapshot ()
    let mut v: Vec<[i64]> = Vec.empty()
    for _i in 1i64 .. 3i64 do v = Vec.push (new [i64](2, \k -> k)) v
    let used = Alloc.since start
    expect_events "3 arrays in a Vec" used 4 0 0
    assert (Vec.length (ref v) == 3)

// --- string ---

// Rule: literals are static data; `length`, `==`, and `contains` borrow (stack-and-heap.md, string.md).
test "string literal, length, equality, search: no allocation" =
    let unit_text = "abcdefghij"
    let needle = "jab"
    let a = String.repeat (ref unit_text) 40
    let b = String.repeat (ref unit_text) 40
    let start = Alloc.snapshot ()
    let literal = "hello"
    let length = String.length (ref a) + String.length (ref literal)
    let same = a == b
    let found = String.contains (ref a) (ref needle)
    let used = Alloc.since start
    expect "string reads" used 0 0 0 0
    assert (length == 405 && same && !found)

// Rule: `string` is UTF-16, 2 bytes per code unit; `to_string` returns a new string. 5 digits.
test "to_string: one block of two bytes per digit" =
    let start = Alloc.snapshot ()
    let text = to_string 12345
    let used = Alloc.since start
    expect "to_string 12345" used 1 0 0 10
    assert (String.length (ref text) == 5)

// Rule: `String.repeat` returns a new string. 10 x 40 = 400 code units.
test "String.repeat: one block of two bytes per character" =
    let unit_text = "abcdefghij"
    let start = Alloc.snapshot ()
    let text = String.repeat (ref unit_text) 40
    let used = Alloc.since start
    expect "String.repeat" used 1 0 0 800
    assert (String.length (ref text) == 400)

// Rule: an interpolation builds its result in one block. 400 + 1 + 400 = 801 code units.
test "string interpolation: one block for the whole result" =
    let unit_text = "abcdefghij"
    let a = String.repeat (ref unit_text) 40
    let start = Alloc.snapshot ()
    let text = $"{a}-{a}"
    let used = Alloc.since start
    expect "interpolation" used 1 0 0 1602
    assert (String.length (ref text) == 801)

// Rule: a consumed heap string on the left of `+` is extended with `realloc` (stack-and-heap.md).
// 401, 402, 403 code units: 2 x (401 + 402 + 403) = 2412 bytes in 3 resizes.
test "s = s + text on an owned string: realloc in place, no new block" =
    let unit_text = "abcdefghij"
    let mut s = String.repeat (ref unit_text) 40
    let start = Alloc.snapshot ()
    s = s + "x"
    s = s + "y"
    s = s + "z"
    let used = Alloc.since start
    expect "3 appends" used 0 3 0 2412
    assert (String.length (ref s) == 403)

// Rule: `String.trim` returns a new string. 40 x 14 = 560 code units minus 4 outer spaces = 556.
test "String.trim: one block for the trimmed text" =
    let unit_text = "  abcdefghij  "
    let a = String.repeat (ref unit_text) 40
    let start = Alloc.snapshot ()
    let text = String.trim (ref a)
    let used = Alloc.since start
    expect "String.trim" used 1 0 0 1112
    assert (String.length (ref text) == 556)

// --- records, unions, Rc, closures ---

// Rule: records, tuples, `Maybe`, and non-recursive unions are values (stack-and-heap.md).
test "records, tuples, Maybe, unions with payloads: nothing on the heap" =
    let start = Alloc.snapshot ()
    let p = Point { x: 3, y: 4 }
    let pair = (p.x, p.y)
    let found: Maybe<i64> = Maybe.Some 7
    let shape = Rect p
    let size = area (ref shape)
    let used = Alloc.since start
    expect "values" used 0 0 0 0
    match found with
    | Maybe.Some n -> assert (n == 7)
    | Maybe.None -> assert false
    match pair with
    | (a, b) -> assert (size == 12 && a + b == 7)

// Rule: each constructor of a recursive union that has a recursive child allocates that node
// (stack-and-heap.md): 3 nodes. The bytes of a node depend on the pointer size.
test "recursive union: one block per node, freed with the tree" =
    let start = Alloc.snapshot ()
    let tree = Node (Node (Leaf, 2, Leaf), 1, Node (Leaf, 3, Leaf))
    let built = Alloc.since start
    expect_events "tree built" built 3 0 0
    assert (count_nodes (ref tree) == 3)
    let before_drop = Alloc.snapshot ()
    let nodes = build_and_drop_tree ()
    let used = Alloc.since before_drop
    expect_events "tree built and dropped" used 3 0 3
    assert (nodes == 3)

// Rule: `Maybe` is a value; the array in its payload is the only block. 4 x 8 bytes.
test "Maybe holding an array: the array only" =
    let start = Alloc.snapshot ()
    let found = Maybe.Some (new [i64](4, \i -> i))
    let used = Alloc.since start
    expect "Maybe.Some array" used 1 0 0 32
    match found with
    | Maybe.Some xs -> assert (Array.length (ref xs) == 4)
    | Maybe.None -> assert false

// Rule: `Rc.new` allocates the block of count and value, plus the blocks the value owns;
// `Rc.share` only counts (rc.md). The array and the Rc block are 2 blocks.
test "Rc.new allocates one block, Rc.share none" =
    let start = Alloc.snapshot ()
    let a = Rc.new (new [i64](4, \i -> i))
    let built = Alloc.since start
    expect_events "Rc.new" built 2 0 0
    let before_share = Alloc.snapshot ()
    let b = Rc.share (ref a)
    let shared = Alloc.since before_share
    expect "Rc.share" shared 0 0 0 0
    assert (Array.length (ref (Rc.get (ref b))) == 4)

// Rule: a closure that does not leave its function keeps its environment on the stack
// (stack-and-heap.md). A closure that captures a value and is passed to a library function
// can allocate its environment on wasm32, so this contract does not cover that case.
test "a closure that stays in its function: no allocation" =
    let base = 10
    let start = Alloc.snapshot ()
    let add = \x -> x + base
    let total = add 5 + add 6
    let used = Alloc.since start
    expect "local closure" used 0 0 0 0
    assert (total == 31)

// --- Alloc itself ---

// Rule: reading the counters does not allocate.
test "Alloc.since right after Alloc.snapshot: all zero" =
    let start = Alloc.snapshot ()
    let used = Alloc.since start
    expect "empty window" used 0 0 0 0

// Rule: `Alloc.measure` counts what the closure does; the closure's own environment is built
// before the call, outside the window.
test "Alloc.measure: the events of the closure and its result" =
    let xs = new [i64](10, \i -> i)
    match Alloc.measure (\() -> Array.map (\x -> x + 1) (ref xs)) with
    | (used, ys) ->
        expect "measure" used 1 0 0 80
        assert (ys[9] == 10)

test "Alloc.measure of code that allocates nothing" =
    match Alloc.measure (\() -> 1 + 2) with
    | (used, total) ->
        expect "measure nothing" used 0 0 0 0
        assert (total == 3)

// Rule: the counters are cumulative, so a window contains the windows inside it.
test "an outer window contains the windows inside it" =
    let outer = Alloc.snapshot ()
    let a = new [i64](4, \i -> i)
    let inner = Alloc.snapshot ()
    let b = new [i64](4, \i -> i)
    let inner_used = Alloc.since inner
    let outer_used = Alloc.since outer
    expect "inner" inner_used 1 0 0 32
    expect "outer" outer_used 2 0 0 64
    assert (Array.length (ref a) + Array.length (ref b) == 8)
```

## 付録 B: `std/Alloc.tz`（そのまま置く）

試作で同じ本体（コメントだけが違う）が型検査と実行を通った。

```tsuzuri
/// The heap events that a piece of code caused, as the counting allocator counts them: blocks
/// allocated, live blocks resized (`reallocs`), blocks freed, and the bytes that the allocations
/// and the resizes asked for (a resize counts its new size; the 16-byte header of the counting
/// allocator is not counted). `tsuzuri test` links the counting allocator when a test reaches
/// this module; other outputs need `--allocator counting`.
record Stats { allocs: i64, reallocs: i64, frees: i64, bytes: i64 }

/// The counters of the whole program so far. Subtract two snapshots, or use `since`.
def snapshot :: unit -> Stats
fn snapshot _unit =
    Stats { allocs: Alloc.__counter 0, reallocs: Alloc.__counter 1, frees: Alloc.__counter 2, bytes: Alloc.__counter 3 }

/// The events since `start`, which `snapshot` returned earlier.
def since :: Stats -> Stats
fn since start =
    let now = snapshot ()
    Stats { allocs: now.allocs - start.allocs, reallocs: now.reallocs - start.reallocs, frees: now.frees - start.frees, bytes: now.bytes - start.bytes }

/// Runs `run` once and returns the events it caused with its result. A value that `run`
/// captures is not uniquely owned inside it, so measure an operation that consumes a value
/// (`Array.update_at i v xs`) with `snapshot` and `since` instead.
def measure :: (unit -> 'a) -> Stats * 'a
fn measure run =
    let start = snapshot ()
    let result = run ()
    (since start, result)
```

## 付録 C: `src/runtime/heap-counting.ll`（新しい全文）

`tsuzuri_alloc_stats` が書く 4 つの値と順序は変えない（`tests/allocator.mjs` が値を確かめる）。差分は、配列を 6 要素にしたこと、`@tz.heap.change` の引数を 3 つにしたこと、
`@tz.heap.counter` を足したこと、`tsuzuri_alloc_stats` がそれを呼ぶこと。

```llvm
; --allocator counting (F13 Phase 2, F14): the target's allocator, renamed to @tz.alloc.base,
; @tz.free.base, and @tz.realloc.base, behind a 16-byte header that holds the requested size.
; @tz.heap.counts holds six counters: 0 allocations, 1 resizes, 2 frees, 3 requested bytes
; (an allocation counts its size, a resize its new size), 4 live bytes, 5 peak of the live bytes.
@tz.heap.counts = internal global [6 x i64] zeroinitializer, align 8

define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %large = icmp ugt i64 %size, 9223372036854775791
  br i1 %large, label %fail, label %request
request:
  %total = add i64 %size, 16
  %base = call ptr @tz.alloc.base(i64 %total)
  store i64 %size, ptr %base, align 16
  call void @tz.heap.change(i64 0, i64 %size, i64 %size)
  %pointer = getelementptr inbounds i8, ptr %base, i64 16
  ret ptr %pointer
fail:
  call void @llvm.trap()
  unreachable
}

define internal void @tz.free(ptr %pointer) nounwind {
entry:
  %null = icmp eq ptr %pointer, null
  br i1 %null, label %done, label %release
release:
  %base = getelementptr inbounds i8, ptr %pointer, i64 -16
  %size = load i64, ptr %base, align 16
  %delta = sub i64 0, %size
  call void @tz.heap.change(i64 2, i64 %delta, i64 0)
  call void @tz.free.base(ptr %base)
  br label %done
done:
  ret void
}

define internal ptr @tz.realloc(ptr %old, i64 %old_size, i64 %new_size) nounwind {
entry:
  %zero = icmp eq i64 %new_size, 0
  br i1 %zero, label %release, label %check
release:
  call void @tz.free(ptr %old)
  ret ptr null
check:
  %empty = icmp eq ptr %old, null
  br i1 %empty, label %allocate, label %bound
allocate:
  %fresh = call ptr @tz.alloc(i64 %new_size)
  ret ptr %fresh
bound:
  %large = icmp ugt i64 %new_size, 9223372036854775791
  br i1 %large, label %fail, label %resize
resize:
  %base = getelementptr inbounds i8, ptr %old, i64 -16
  %stored = load i64, ptr %base, align 16
  %old_total = add i64 %stored, 16
  %new_total = add i64 %new_size, 16
  %moved = call ptr @tz.realloc.base(ptr %base, i64 %old_total, i64 %new_total)
  store i64 %new_size, ptr %moved, align 16
  %delta = sub i64 %new_size, %stored
  call void @tz.heap.change(i64 1, i64 %delta, i64 %new_size)
  %pointer = getelementptr inbounds i8, ptr %moved, i64 16
  ret ptr %pointer
fail:
  call void @llvm.trap()
  unreachable
}

; %slot (0 allocation, 1 resize, 2 free) counts one event, %requested adds to the requested
; bytes, and %delta moves the live bytes and the peak.
define internal void @tz.heap.change(i64 %slot, i64 %delta, i64 %requested) nounwind {
entry:
  %event_slot = getelementptr inbounds [6 x i64], ptr @tz.heap.counts, i64 0, i64 %slot
  %events = atomicrmw add ptr %event_slot, i64 1 monotonic, align 8
  %bytes_slot = getelementptr inbounds [6 x i64], ptr @tz.heap.counts, i64 0, i64 3
  %bytes = atomicrmw add ptr %bytes_slot, i64 %requested monotonic, align 8
  %live_slot = getelementptr inbounds [6 x i64], ptr @tz.heap.counts, i64 0, i64 4
  %before = atomicrmw add ptr %live_slot, i64 %delta monotonic, align 8
  %after = add i64 %before, %delta
  %peak_slot = getelementptr inbounds [6 x i64], ptr @tz.heap.counts, i64 0, i64 5
  %peak = atomicrmw umax ptr %peak_slot, i64 %after monotonic, align 8
  ret void
}

; Alloc.__counter and tsuzuri_alloc_stats read one counter by slot. A slot above 5 traps.
define internal i64 @tz.heap.counter(i64 %index) nounwind {
entry:
  %valid = icmp ult i64 %index, 6
  br i1 %valid, label %read, label %fail
read:
  %slot = getelementptr inbounds [6 x i64], ptr @tz.heap.counts, i64 0, i64 %index
  %value = load atomic i64, ptr %slot monotonic, align 8
  ret i64 %value
fail:
  call void @llvm.trap()
  unreachable
}

; void tsuzuri_alloc_stats(tsuzuri_allocation_stats *stats)
define void @tsuzuri_alloc_stats(ptr %stats) nounwind {
entry:
  %allocations = call i64 @tz.heap.counter(i64 0)
  store i64 %allocations, ptr %stats, align 8
  %frees = call i64 @tz.heap.counter(i64 2)
  %frees_out = getelementptr inbounds i8, ptr %stats, i64 8
  store i64 %frees, ptr %frees_out, align 8
  %live = call i64 @tz.heap.counter(i64 4)
  %live_out = getelementptr inbounds i8, ptr %stats, i64 16
  store i64 %live, ptr %live_out, align 8
  %peak = call i64 @tz.heap.counter(i64 5)
  %peak_out = getelementptr inbounds i8, ptr %stats, i64 24
  store i64 %peak, ptr %peak_out, align 8
  ret void
}
```

## 付録 D: HEAD での実測

### D.1 現状の出力（`target/release/tsuzuri`。リポジトリの外の一時ディレクトリで実行）

```text
$ tsuzuri build <exe 用の Main.tz> --allocator counting -o <out>
<command line>:1:1: error[E2000]: --allocator counting requires object, LLVM IR, header, or WebAssembly output; the host reads the counts with tsuzuri_alloc_stats   (終了コード 2)
$ tsuzuri test <dir> --allocator counting
<command line>:1:1: error[E2000]: --allocator is only valid with build   (終了コード 2)
$ tsuzuri run <dir> --allocator counting
<command line>:1:1: error[E2000]: --allocator is only valid with build   (終了コード 2)
$ tsuzuri check <Alloc を使うソース>
error[E1004]: unknown record, union, or type alias 'Alloc.Stats'
error[E1002]: unknown value 'Alloc'
```

HEAD の `--emit llvm --allocator counting` の IR は `@tz.heap.counts = internal global [4 x i64]`（`tests/fixtures/last_use`、`-O0`、native で 246,293 byte）。
試作は 246,599 byte（306 byte 増。配列が 6 要素になり、`@tz.heap.counter` が増えた分）。

### D.2 既存のカウンター（`tsuzuri_alloc_stats`）で見える確保数（wasm32、`--allocator counting`、`-O0` と `-O3` は同じ）

```text
map_count,10  => allocs 2 frees 2 live 0 peak 192        （new [i64](10) と Array.map の 2 回）
vec_push,100  => allocs 1 frees 1 live 0 peak 1056       （100 回の push が確保 1 回に見える。再確保は数えない）
```

`Vec.push` を 100 回しても `allocs` は 1 で、容量の倍増（再確保 5 回）が見えない。**これが `Stats` に `reallocs` を足す理由**（決定事項 D2）。

### D.3 契約 42 件の 4 構成の結果（試作。付録 A の fixture）

```text
$ tsuzuri test <fixture> -O0                   → 42 passed; 0 failed; 0 ignored
$ tsuzuri test <fixture> -O3                   → 42 passed; 0 failed; 0 ignored
$ tsuzuri test <fixture> --target wasm32 -O0   → 42 passed; 0 failed; 0 ignored
$ tsuzuri test <fixture> --target wasm32 -O3   → 42 passed; 0 failed; 0 ignored   （clang の警告 1 行つき。D.6）
```

最初の版では 41 件のうち 1 件（捕捉つきクロージャを `Array.map` に渡す形）が wasm32 でだけ失敗し、その形を契約から外した（D.5）。**期待値は 1 つも書き換えていない**。

失敗の見え方（native。期待値を 1 つ変えた変異体、`Vec.push` 100 回の `reallocs` を 6 と書いたもの）:

```text
not ok 1 - Main 100 pushes realloc five times (the mutant expects six)
  failure: trapped or terminated by signal
  100 pushes: allocs 1/1 reallocs 5/6 frees 0/0 bytes 2016/2016 (got/want)
```

### D.4 規則から決めた値と実測の突き合わせ（`tsuzuri run` の `Debug.print`、native `-O0`／`-O3`）

| 操作 | 規則から決めた値（allocs, reallocs, frees, bytes） | 実測 |
| --- | --- | --- |
| `new [i64](10, f)` | 1, 0, 0, 80 | 同じ |
| `new [i64](0, f)` | 1, 0, 0, 1（空は 1 byte） | 同じ |
| `new [1, 2, 3]`（整数の既定は i32） | 1, 0, 0, 12 | 同じ（付録 A は `i64` を明示して 24 にした） |
| `Array.map`／`copy` n=10 | 1, 0, 0, 80 | 同じ |
| `Array.append` 10+5 | 1, 0, 0, 120 | 同じ |
| `Array.filter` 50 件から 25 件 | 1, 3, 0, 480（容量 4→8→16→32） | 同じ |
| `Vec.push` ×5 | 1, 1, 0, 96 | 同じ |
| `Vec.push` ×100 | 1, 5, 0, 2016 | 同じ |
| `Vec.with_capacity 100` + 100 push | 1, 0, 0, 800 | 同じ |
| `Vec.to_array`（100 要素） | 0, 0, 0, 0 | 同じ |
| `to_string 12345` | 1, 0, 0, 10 | 同じ |
| `String.repeat` 400 字 | 1, 0, 0, 800 | 同じ |
| `s = s + "x"` を 3 回（所有する文字列） | 0, 3, 0, 2412 | 同じ |
| `Array.update_at i v xs`（最後の使用）／ループの再束縛 | 0, 0, 0, 0 | 同じ |
| ループの再束縛で `xs` が別の場所で借用される | 10, 0, 10, 8000（10 回の複製） | 同じ |
| 3 ノードの再帰 union | 3 回確保（byte は pointer の大きさ次第。native で 168） | 3 回 |
| `Rc.new (new [i64](4, f))` | 2 回確保（native の byte は 64） | 2 回 |
| `Array.sort_in_place` 1,000 要素 | 0 | 0 |

### D.5 規則と食い違った所（契約に入れず、本文の「現状」と「落とし穴」に書いたもの）

```text
Array.sort n=1   => allocs 2   frees 1   bytes 16
Array.sort n=2   => allocs 3   frees 2   bytes 40
Array.sort n=10  => allocs 17  frees 16  bytes 280
Array.sort n=20  => allocs 42  frees 41  bytes 640
Array.sort n=50  => allocs 135 frees 134 bytes 1864
Array.sort n=100 => allocs 318 frees 317 bytes 4128      （n に比例して増える。`Array.sort_in_place` は 0）
```

`Array.sort` の確保の回数に規則はない（std の `Array.sort` は `Array.sort_by` を呼ぶ）。契約には入れない。F14 は直さない。

```text
Array.map (\x -> x * factor) (ref xs)   （factor は捕捉する i64、xs は 10 要素）
  native -O0/-O3 : allocs 1 frees 0 bytes 80     （捕捉は即値で閉包に入る）
  wasm32 -O0/-O3 : allocs 2 frees 1 bytes 88     （環境の 8 byte を確保して解放する。pointer が 4 byte で即値が入らない）
```

捕捉つきクロージャの確保は target に依る。契約には入れない（付録 A の `a closure that stays in its function` は両方で 0）。

```text
let mut xs = new [i64](100, \i -> 0)
for i in 0i64 .. 99i64 do xs = Array.update_at i (i * 2) xs
  後ろで xs を読まない／xs[99] を let で読む  : allocs 0  frees 0  bytes 0
  同じ関数のどこかで Array.length (ref xs) か $"{xs[99]}" を書く : allocs 100 frees 100 bytes 80000
```

`ownership.md`「借用・スライスの元となる局所…は、この最後の使用の最適化から除外されます」のとおり（規則どおり）。契約に入れた（付録 A の `…borrowed elsewhere…`）。
補間 `$"…{xs[99]}…"` の穴は借用なので同じになる。

### D.6 `-O3` の wasm32 のテスト実行器の clang の警告（F14 と無関係）

文字列を使うテストを含む wasm32 の `-O3` の実行器は、**HEAD のコンパイラでも** clang の警告を 1 件出す（終了コードは 0）:

```text
warning: <unknown>:0:0: loop not unrolled: the optimizer was unable to perform the requested transformation; ... [-Wpass-failed=transform-warning]
```

`Alloc` を使わない 3 件のテストで HEAD の `target/release/tsuzuri test <dir> --target wasm32 -O3` に再現した。ハーネスは `TestReport::messages` を失敗にしない。

### D.7 そのほかの確認

| 確認 | 結果 |
| --- | --- |
| `tsuzuri_alloc_stats` の値（`map_count,10`・`vec_push,100`。wasm32 `-O0`） | HEAD と試作で同じ（allocs 2/1、frees 2/1、live 0、peak 192/1056） |
| `Alloc` を使わないプログラムの IR（`tests/fixtures/last_use`・`map_set`・`channel`、`examples/point`。native と wasm32、`-O0`、`--emit llvm --no-cache`） | HEAD と試作で **byte 単位で同じ**（240,870／228,116／823,993／805,875／531,909／515,809／5,287／67,737 byte） |
| 試作で `tsuzuri run <dir> -O0\|-O3 --allocator counting`、`tsuzuri build … --allocator counting` で作った実行ファイル | どちらも `90` を出す（`used.bytes + Array.length`＝80 + 10） |
| 試作で `tsuzuri test <dir> --allocator counting` | 通る |
| 4 つの Task が 1,000 回ずつ確保・解放（試作、`-O0`/`-O3` とも） | `allocs=4007 frees=4006`（4,000 + Task の実行器の確保 7 回。差 1 は結果の配列）。更新は失われない |
| `Array.update_at` を閉包の中に入れた `Alloc.measure (\() -> Array.update_at 0 5 xs)` | `allocs=1 bytes=32`（閉包の中の `xs` は一意でなく複製される）。`snapshot`／`since` なら 0。これが `snapshot`／`since` を足す理由（決定事項 D1） |

## 付録 E: 契約 42 件の出典

`docs` は `_tsuzuri/language-reference/` の下のページ。「コード」は HEAD のソース。

| # | テスト名（付録 A） | 値 | 出典 |
| --- | --- | --- | --- |
| 1 | new array: one block of n elements | 1, 0, 0, 80 | `built-in-types-and-modules/array.md`「`new [T](length, initializer)` は…ヒープに一括確保」 |
| 2 | new array of length 0 | 1, 0, 0, 1 | コード: `allocate_array`（`src/llvm.rs`）が長さ 0 のとき 1 を渡す。`ownership-and-memory/stack-and-heap.md`「`new []`…1 バイト以上」 |
| 3 | array literal without new | 0, 0, 0, 0 | `stack-and-heap.md`「記憶域の決定」の表 |
| 4 | array literal with new | 1, 0, 0, 24 | 同上（`new [1, 2, 3]` はヒープ上に要素領域を一括確保） |
| 5 | empty array | `[]` は 0、`new []` は 1, 0, 0, 1 | `stack-and-heap.md`「文字列リテラルと空コレクション」 |
| 6 | Array.map | 1, 0, 0, 80 | `std/Array.tz` の署名 `Array.map :: … -> ref ['a] -> ['b]`（新しい配列を返す） |
| 7 | Array.copy | 1, 0, 0, 80 | 同上（`ref ['a] -> ['a]`） |
| 8 | Array.append | 1, 0, 0, 120 | 同上 |
| 9 | Array.reverse | 1, 0, 0, 400 | 同上 |
| 10 | reading an array | 0, 0, 0, 0 | `stack-and-heap.md`「共有借用…は…ゼロコスト」 |
| 11 | a slice borrows | 0, 0, 0, 0 | `array.md`「スライス式…コピーせずに参照」 |
| 12 | dropping an array | 1, 0, 1, 80 | 所有権（スコープの終わりで解放）。`ownership-and-memory/ownership.md` |
| 13 | Array.update_at on a uniquely owned array | 0, 0, 0, 0 | `array.md` の計算量表「最後の使用を証明できればバッファを再利用」 |
| 14 | Array.update_at in a loop that rebinds | 0, 0, 0, 0 | `ownership.md`「ループ内で `xs = Array.update_at i value xs` と再束縛する値も対象」 |
| 15 | …borrowed elsewhere | 10, 0, 10, 8000 | `ownership.md`「借用・スライスの元となる局所…は…除外」。複製は 100 × 8 byte |
| 16 | implicit copy | 1, 0, 0, 80 | `ownership.md`「`second` へ代入したあとも `first` を使っているので、要素全体が新しいバッファへコピーされます」 |
| 17 | Array.update_at on a fresh copy | 1, 0, 0, 80 | `Array.copy` の結果は一時の所有値（13 と同じ規則） |
| 18 | Array.filter | 1, 3, 0, 480 | コード: `std/Array.tz` の `filter`（`Vec.push` と `Vec.to_array`）、`vec.md`（容量 4 から倍） |
| 19 | Array.sort_in_place | 0, 0, 0, 0 | コード: `std/Array.tz` の `sort_in_place` の doc コメント「without allocating」、`array.md`「作業領域を確保しない」 |
| 20 | Vec.empty | 0, 0, 0, 0 | `vec.md`「空のバッファ `Vec.empty()`」 |
| 21 | Vec.push ×4 | 1, 0, 0, 32 | `vec.md`「初期値 4」。コード: `reserve_vector`（`src/llvm_bulk.rs`） |
| 22 | Vec.push ×5 | 1, 1, 0, 96 | 同上（倍増） |
| 23 | Vec.push ×100 | 1, 5, 0, 2016 | 同上。4, 8, 16, 32, 64, 128 の 6 容量 |
| 24 | Vec.with_capacity | 1, 0, 0, 800 | `vec.md`「`with_capacity`…途中の再確保を避けられます」 |
| 25 | Vec.to_array | 0, 0, 0, 0 | コード: 18 の規則が頼る（バッファを渡す） |
| 26 | Vec.length / capacity | 0, 0, 0, 0 | `vec.md`「長さ・容量の取得は O(1)（メタデータ参照）」 |
| 27 | Vec of arrays | 確保 4（byte は pointer の大きさ次第） | 20〜23 と 1 の合成 |
| 28 | string literal, length, equality, search | 0, 0, 0, 0 | `stack-and-heap.md`（リテラルは静的領域）、`string.md`（借用の読み） |
| 29 | to_string | 1, 0, 0, 10 | `string` は UTF-16（1 code unit 2 byte）。`to_string` は新しい文字列を返す |
| 30 | String.repeat | 1, 0, 0, 800 | 同上 |
| 31 | string interpolation | 1, 0, 0, 1602 | 補間は 1 つの文字列を作る |
| 32 | s = s + text | 0, 3, 0, 2412 | `stack-and-heap.md`「連結で消費する左辺がヒープの所有バッファなら、`realloc` で伸ばして再利用します」 |
| 33 | String.trim | 1, 0, 0, 1112 | `std/String.tz` の署名 `trim :: ref string -> string` |
| 34 | records, tuples, Maybe, unions | 0, 0, 0, 0 | `stack-and-heap.md`「レコードやタプルは値型としてスタック上にインライン配置」 |
| 35 | recursive union | 確保 3、解放 3 | `stack-and-heap.md`「再帰的共用体のヒープ配置」 |
| 36 | Maybe holding an array | 1, 0, 0, 32 | 34 と 1 の合成 |
| 37 | Rc.new / Rc.share | 確保 2、`share` は 0 | `built-in-types-and-modules/rc.md`、`stack-and-heap.md`（`Rc.new` は計数と値のブロックを確保） |
| 38 | a closure that stays in its function | 0, 0, 0, 0 | `stack-and-heap.md`「外へ逃げない局所の関数値の捕捉環境も…入口のスタックに置きます」 |
| 39 | Alloc.since right after Alloc.snapshot | 0, 0, 0, 0 | 本チケットの仕様（読み出しは確保しない） |
| 40 | Alloc.measure | 1, 0, 0, 80 | 本チケットの仕様と 6 の規則 |
| 41 | Alloc.measure of code that allocates nothing | 0, 0, 0, 0 | 同上 |
| 42 | an outer window contains the windows inside it | 内側 1, 0, 0, 32／外側 2, 0, 0, 64 | 本チケットの仕様（計数は累積） |

## 付録 F: 出典（外部。2026-10-18 に検索で確認）

- Go: `testing.AllocsPerRun(runs, f)` は、`f` 1 回あたりの確保の平均回数を返す標準ライブラリの関数。
- .NET: `GC.GetAllocatedBytesForCurrentThread()` は、現在のスレッドが確保したバイト数を返す。
- Rust: 標準には確保の計数がなく、`#[global_allocator]` を差し替える crate（例: `allocation-counter` の `allocation_counter::measure(|| …)`）で数える。
  `info.count_total == 0`（確保なし）や `bytes_total` の範囲を `assert` する使い方が紹介されている。本チケットの `Alloc.snapshot`／`since`／`measure` はこれに当たる（`reallocs` を別に数える点が違う）。

## 付録 G: 試作（リポジトリは変更していない）

- 場所: 作業ツリーの `target/tz-F14-copy`（`Cargo.toml`・`Cargo.lock`・`src`・`std` だけの複製）。ビルド: `cargo build --release`（`CARGO_TARGET_DIR` は別の場所。`CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16` で速くした。60 秒）。その前に `cargo check --release` を 1 回。複製は完了後に消す。
- 試作に入れたもの: 付録 C の `heap-counting.ll`、付録 B の `std/Alloc.tz`（`stdlib.rs` の 3 か所）、`Builtin::AllocCounter`（`check.rs` の 4 か所）と `emit_builtin` の arm、`emit_program_in` の自動の counting（**すべての entry**）と heap を連結する条件、
  `validate_allocator` から `Emit::Executable` の拒否を外す変更、`main.rs` の `run`／`test` への `--allocator counting`。
- 試作に入れていないもの（本文の設計どおりに実装する）: 到達の制限（`E1018`）と根の除外、`E1022`（std の外からの `Alloc.__counter`）、`E2000`（host／small との併用）、`TestRunnerOptions`・`TestOptions` の `allocator`（試作は自動の連結だけで足りた）、`bench --allocs`、テストと文書。

## 付録 H: `bench --allocs` の計数用の IR と C（Phase 2。本文の「bench の計数の流れ」が参照する）

計数用の bench 実行器の IR（`emit_program_in` の `Entry::BenchRunner` の組み立てで、`instrumentation.allocator == Allocator::Counting` のとき、`@tsuzuri_bench_count` の定義の後に足す）。
`@tz.heap.counter` を参照するので、`reads_counters` が真になり、counting の heap が連結される（明示の `Counting` なので `E1018` にならない）。

```llvm
define void @tsuzuri_bench_counts(ptr %out) {
entry:
  %allocs = call i64 @tz.heap.counter(i64 0)
  store i64 %allocs, ptr %out, align 8
  %reallocs = call i64 @tz.heap.counter(i64 1)
  %second = getelementptr inbounds i64, ptr %out, i64 1
  store i64 %reallocs, ptr %second, align 8
  %bytes = call i64 @tz.heap.counter(i64 3)
  %third = getelementptr inbounds i64, ptr %out, i64 2
  store i64 %bytes, ptr %third, align 8
  ret void
}
```

`src/runtime/bench-runner.c` の `-DTSUZURI_BENCH_ALLOCS` 版。既存の `main` は `#else` の側にそのまま残し、`parse` と `tsuzuri_bench_now` は共有する。

```c
#ifdef TSUZURI_BENCH_ALLOCS
extern void tsuzuri_bench_counts(uint64_t *out); /* allocations, resizes, requested bytes */

/* bench-runner INDEX: runs bench INDEX once with 1 iteration (warm-up), then with 1 and with 2
   iterations, and prints "alloc_calls N" and "alloc_bytes N": the allocations plus resizes and
   the requested bytes of the second iteration alone. The setup of Bench.with_input runs in both
   calls and cancels. Exit codes: 2 usage, 3 a negative sample, 4 output error, 5 a negative count. */
int main(int argc, char **argv) {
    unsigned long long index;
    if (argc != 2 || !parse(argv[1], 0, UINT32_MAX, &index) || index >= tsuzuri_bench_count()) return 2;
    uint64_t before[3], middle[3], after[3];
    if (tsuzuri_bench_sample((uint32_t)index, 1) < 0) return 3;
    tsuzuri_bench_counts(before);
    if (tsuzuri_bench_sample((uint32_t)index, 1) < 0) return 3;
    tsuzuri_bench_counts(middle);
    if (tsuzuri_bench_sample((uint32_t)index, 2) < 0) return 3;
    tsuzuri_bench_counts(after);
    int64_t calls = (int64_t)((after[0] - middle[0]) + (after[1] - middle[1]))
                  - (int64_t)((middle[0] - before[0]) + (middle[1] - before[1]));
    int64_t bytes = (int64_t)(after[2] - middle[2]) - (int64_t)(middle[2] - before[2]);
    if (calls < 0 || bytes < 0) return 5;
    if (printf("alloc_calls %lld\nalloc_bytes %lld\n", (long long)calls, (long long)bytes) < 0) return 4;
    return fflush(stdout) == 0 ? 0 : 4;
}
#else
/* the existing main */
#endif
```

## 付録 I: bench の検証用ソース（`bench2`。Phase 2 のテストと手順 2.4 が使う）

HEAD の `tsuzuri bench`（`--allocs` なし）で、5 件とも走る（2026-10-18。中央値は機械で変わる。`five pushes` は 0.18 ns で、時間の実行器では LLVM が確保ごと消している）。

```tsuzuri
def push_five :: unit -> i64
fn push_five _unit =
    let mut v: Vec<i64> = Vec.empty()
    for i in 1i64 .. 5i64 do v = Vec.push i v
    Vec.length (ref v)

def sum_all :: ref [i64] -> i64
fn sum_all values = Array.sum values

bench "nothing allocated" = Bench.of (\_ -> 42)
bench "one array of ten" = Bench.of (\_ -> new [i64](10, \i -> i))
bench "map over a shared input" = Bench.with_input (\_ -> new [i64](10, \i -> i)) (\values -> Array.map (\x -> x + 1) values)
bench "sum over a shared input" = Bench.with_input (\_ -> new [i64](100, \i -> i)) (\values -> sum_all values)
bench "five pushes" = Bench.of (\_ -> push_five ())
```

期待する `--allocs` の値（想定。付録 A の規則から）: 添字 0 `nothing allocated` は `alloc_calls` 0・`alloc_bytes` 0、1 `one array of ten` は 1・80、2 `map over a shared input` は 1・80、3 `sum over a shared input` は 0・0、
4 `five pushes` は 2・96（確保 1 回の 32 byte と再確保 1 回の 64 byte）。
