use tsuzuri::{analyze, llvm};

fn accepts(source: &str) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    for wasm in [true, false] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap(),
            "{source}"
        );
    }
    llvm::emit_target(&module, llvm::Entry::Library, false).unwrap()
}

fn rejects(source: &str, code: &str, fragment: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert!(
        error.message.contains(fragment),
        "{source}\nexpected '{fragment}' in: {}",
        error.message
    );
}

const FILL: &str = "def fill :: ref mut [i64..] -> i64 -> unit = \\values value ->\n    for index in 0i64 .. (values.length - 1) do Array.write index value values\n";
const CONFLICT: &str = "access conflicts with a live borrow";

#[test]
fn exclusive_slice_types_resolve_only_after_ref_mut() {
    accepts(
        "def total :: ref mut [i64..] -> i64 = \\values ->\n    let mut sum = 0i64\n    for value in values do sum = sum + value\n    sum + values.length + values[0]\n\ndef count :: ref mut ['a..] -> i64 = \\values -> values.length\n\nlet mut values: [i64] = [1, 2]\ntotal (ref mut values[0..]) + count (ref mut values[1..])\n",
    );
    accepts(
        "def count {r} :: ref mut {r} ['a..] -> i64 = \\values -> values.length\n\nlet mut values: [i64] = [1]\ncount (ref mut values[0..])\n",
    );
    for source in [
        "def f :: [i64..] -> i64 = \\values -> 0\n",
        "def f :: ref [i64..] -> i64 = \\values -> 0\n",
        "def f :: Vec<[i64..]> -> i64 = \\values -> 0\n",
        "def f :: [[i64..]] -> i64 = \\values -> 0\n",
        "let values: [i64..] = [1]\n()\n",
    ] {
        rejects(
            source,
            "E1005",
            "'[T..]' is only valid directly after 'ref mut'",
        );
    }
    rejects(
        "def f :: [ref mut [i64..]] -> i64 = \\values -> 0\n",
        "E1005",
        "collections cannot hold exclusive borrows",
    );
    let error = analyze("def f :: ref mut [i64..] -> unit = \\values ->\n    let g = \\x -> Array.write 0 x values\n    g 1\n").expect_err("capture");
    assert!(
        error.message.contains("ref mut [i64..]"),
        "{}",
        error.message
    );
}

#[test]
fn exclusive_slices_borrow_named_mutable_arrays() {
    accepts(
        "let mut values: [i64] = [1, 2, 3, 4]\nlet a = ref mut values[1..3]\nArray.write 0 9 a\nlet b = ref mut values[1..]\nArray.write 0 8 b\nlet c = &mut values[..2]\nArray.write 0 7 c\nlet d = ref mut values[1..values.length - 1]\nd.length\n",
    );
    accepts(
        "let mut values: [i64] = [1, 2, 3, 4]\nlet part = ref mut values[1..3]\nArray.write 0 9 part\nlet size = values.length\nlet again = ref mut values[0..1]\nArray.write 0 5 again\nsize + again.length\n",
    );
    rejects(
        "let mut values: [i64] = [1, 2]\nlet part = ref mut values[..]\npart.length\n",
        "E0002",
        "write 'ref mut xs[0..]'",
    );
    rejects(
        "let values: [i64] = [1, 2]\nlet part = values[1..2]\npart.length\n",
        "E0002",
        "array slices must be borrows",
    );
    rejects(
        "let mut values: [i64] = [1, 2, 3]\nlet left = ref mut values[0..1]\nlet right = ref mut values[1..3]\nArray.write 0 5 left\nright.length\n",
        "E1014",
        CONFLICT,
    );
    rejects(
        "let values: [i64] = [1, 2, 3]\nlet part = ref mut values[0..1]\npart.length\n",
        "E1014",
        "mutable access requires 'let mut'",
    );
    rejects(
        "def f :: ref [i64] -> i64 = \\shared ->\n    let part = ref mut shared[0..1]\n    part.length\n",
        "E1014",
        "cannot mutate or exclusively reborrow through a shared reference",
    );
    rejects(
        "let part = ref mut [1, 2, 3][0..2]\npart.length\n",
        "E1014",
        "an exclusive slice must borrow a named array",
    );
    rejects(
        "let mut values: [i64] = [1, 2, 3]\nlet part = ref mut values[0..2]\nvalues.length + part.length\n",
        "E1014",
        CONFLICT,
    );
    rejects(
        "let mut values: [i64] = [1, 2, 3]\nlet part = ref mut values[{ values = [3]; 0 }..1]\npart.length\n",
        "E1014",
        CONFLICT,
    );
    rejects(
        "let mut values: [i64] = [1, 2, 3]\nlet part = ref mut values[0..]\nlet moved = values\npart.length + moved.length\n",
        "E1014",
        CONFLICT,
    );
}

#[test]
fn exclusive_slices_reborrow_and_read_like_shared_slices() {
    accepts(&format!(
        "{FILL}let mut values: [i64] = [1, 2, 3]\nlet part = ref mut values[0..]\nfill part 1\nfill part 2\nlet again = ref mut *part\nfill again 3\nlet keyword = ref mut part\nfill keyword 4\nlet shared = ref part[0..1]\nlet copy = deref part\nshared[0] + part.length + copy[0]\n"
    ));
    accepts(
        "def tail :: ref mut [i64..] -> ref mut [i64..] = \\values -> ref mut values[1..]\n\ndef whole :: ref mut [i64] -> ref mut [i64..] = \\values -> ref mut values[0..]\n\nlet mut values: [i64] = [1, 2, 3]\nlet rest = tail (whole (ref mut values))\nArray.write 0 5 rest\nrest.length\n",
    );
    for (source, code, fragment) in [
        (
            "def f :: ref mut [i64..] -> unit = \\values -> deref values = [1]\n",
            "E1014",
            "cannot replace an exclusive slice as a whole",
        ),
        (
            "def f :: ref mut [i64..] -> unit = \\values -> *values = [1]\n",
            "E1014",
            "cannot replace an exclusive slice as a whole",
        ),
        (
            "def f :: ref mut [i64..] -> unit = \\values ->\n    let g = \\x -> Array.write 0 x values\n    g 1\n",
            "E1005",
            "cannot capture ref mut [i64..] in a reusable function",
        ),
        (
            "def f :: ref mut [i64..] -> i64 = \\values ->\n    tzroutine {\n        let _n = values.length\n    }\n    0\n",
            "E1013",
            "ref mut [i64..] contains a reference",
        ),
        (
            "export def f :: ref mut [i64..] -> i64 = \\values -> 0\n",
            "E1008",
            "",
        ),
        (
            "def f :: unit -> ref mut [i64..] = \\_unit ->\n    let mut values: [i64] = [1]\n    ref mut values[0..1]\n",
            "E1013",
            "",
        ),
        (
            "def f :: ref mut [i64..] -> unit = \\values ->\n    for value in values do Array.write 0 value values\n",
            "E1014",
            CONFLICT,
        ),
        (
            "let mut items: Vec<i64> = Vec.empty()\nlet part = ref mut items[0..1]\npart.length\n",
            "E1005",
            "a slice requires an array",
        ),
        (
            "def f :: ref mut [string..] -> i64 = \\values ->\n    let first = values[0]\n    first.length\n",
            "E1012",
            "",
        ),
        (
            "def f :: ref mut [i64..] -> i64 = \\values ->\n    let element = ref mut values[0]\n    deref element\n",
            "E1014",
            "",
        ),
    ] {
        rejects(source, code, fragment);
    }
}

#[test]
fn call_arguments_convert_to_exclusive_slices() {
    accepts(&format!(
        "{FILL}def whole :: ref mut [i64] -> unit = \\array -> fill array 1\n\nlet mut values: [i64] = [1, 2, 3, 4, 5]\nfill (ref mut values) 7\nfill values 7\nfill &mut values 7\nwhole (ref mut values)\nlet converted: ref mut [i64..] = ref mut values\nArray.write 0 9 converted\nvalues.length\n"
    ));
    accepts(
        "def sum_shared :: ref [i64] -> i64 = \\values -> Array.sum values\n\nlet mut values: [i64] = [1, 2, 3]\nlet part = ref mut values[1..]\nsum_shared part + Array.sum part + Array.length part\n",
    );
    let ir = accepts(
        "def pass :: 'a -> 'a = \\value -> value\n\nlet mut values: [i64] = [1, 2, 3]\nlet whole = pass (ref mut values)\nlet size = whole.length\nlet part = pass (ref mut values[1..])\nArray.write 0 5 part\nsize + part.length\n",
    );
    let definitions = ir
        .lines()
        .filter(|line| line.starts_with("define internal") && line.contains("@tz.fn.Main.pass."))
        .count();
    assert_eq!(definitions, 2, "{ir}");
    let replace = "def replace :: ref mut [i64] -> unit = \\values -> deref values = [7]\n\n";
    for call in [
        "replace values",
        "replace (ref mut *values)",
        "replace (ref mut values)",
    ] {
        rejects(
            &format!("{replace}def f :: ref mut [i64..] -> unit = \\values -> {call}\n"),
            "E1005",
            "an exclusive slice cannot be passed as 'ref mut [T]'",
        );
    }
    rejects(
        "def touch :: ref mut 'a -> unit = \\value -> ()\n\ndef f :: ref mut [i64..] -> unit = \\values -> touch values\n",
        "E1005",
        "an exclusive slice cannot be passed as 'ref mut [T]'",
    );
    rejects(
        &format!("{FILL}def f :: ref [i64] -> unit = \\shared -> fill shared 1\n"),
        "E1014",
        "cannot mutate or exclusively reborrow through a shared reference",
    );
}

#[test]
fn in_place_writes_check_bounds_before_drop_and_store() {
    accepts(
        "def run :: unit -> i64 = \\_unit ->\n    let mut values: [i64] = [1, 2, 3, 4]\n    let mut texts: [string] = [\"a\", \"b\", \"c\"]\n    let part = ref mut values[0..]\n    let mut index = 0i64\n    while index < 4 do\n        Array.write index (index * 10) part\n        index = index + 1\n    Array.swap_in 0 3 part\n    let words = ref mut texts[0..]\n    Array.write 0 \"xyz\" words\n    Array.swap_in 1 2 words\n    part[0] + words[0].length\n",
    );
    // The slice is the last argument, so the value reads it before it is lent to the write.
    accepts(
        "let mut values: [i64] = [1, 2, 3]\nlet tail = ref mut values[1..]\nArray.write 1 (tail[1] * 10) tail\ntail.length\n",
    );
    rejects(
        "def put :: ref mut [i64..] -> i64 -> i64 -> unit = \\values index value -> Array.write index value values\n\nlet mut values: [i64] = [1, 2, 3]\nlet tail = ref mut values[1..]\nput tail 1 (tail[1] * 10)\ntail.length\n",
        "E1014",
        CONFLICT,
    );
    let ir = accepts(
        "def put :: ref mut [string..] -> i64 -> string -> unit = \\values index text -> Array.write index text values\n",
    );
    let write = ir
        .split("\ndefine ")
        .find(|function| function.starts_with("internal i8 @tz.builtin.Array.write."))
        .unwrap_or_else(|| panic!("{ir}"));
    let check = write.find("icmp ult i64").expect("bounds check");
    let free = write.find("call void @tz.free").expect("old element drop");
    let store = write.rfind("store %tz.string").expect("store");
    assert!(check < free && free < store, "{write}");
}

#[test]
fn split_halves_share_one_loan() {
    accepts(
        "def rec negate_all :: ref mut [i64..] -> unit = \\values ->\n    let length = Array.length values\n    if length == 1 then\n        let value = values[0]\n        Array.write 0 (0 - value) values\n    elif length > 1 then\n        match Array.split_at_mut (length / 2) values with\n        | (left, right) ->\n            negate_all left\n            negate_all right\n\ndef alternate :: ref mut [i64..] -> unit = \\values ->\n    match Array.split_at_mut 2 values with\n    | (left, right) ->\n        Array.write 0 10 left\n        Array.write 0 30 right\n        Array.write 1 20 left\n        Array.write 1 40 right\n",
    );
    rejects(
        "def f :: ref mut [i64..] -> unit = \\values ->\n    match Array.split_at_mut 1 values with\n    | (left, right) ->\n        Array.write 0 1 values\n        Array.write 0 2 left\n        Array.write 0 3 right\n",
        "E1014",
        CONFLICT,
    );
    rejects(
        "def f :: ref mut [i64..] -> unit = \\values ->\n    match Array.split_at_mut 1 values with\n    | (left, right) ->\n        let inner = ref mut left[0..1]\n        Array.write 0 1 right\n        Array.write 0 2 inner\n",
        "E1014",
        CONFLICT,
    );
}

#[test]
fn sort_in_place_specializes_only_when_used() {
    let ir = accepts(
        "let mut numbers: [i64] = [3, 1, 2]\nlet mut words: [string] = [\"b\", \"a\"]\nArray.sort_in_place (ref mut numbers)\nArray.sort_in_place (ref mut words)\nnumbers[0] + words[0].length\n",
    );
    assert!(ir.contains("sort_in_place"), "{ir}");
    let ir = accepts("let values: [i64] = [3, 1, 2]\nvalues[0]\n");
    assert!(!ir.contains("sort_in_place"));
}

#[test]
fn exclusive_slice_ir_is_deterministic_and_allocation_free() {
    let ir = accepts(&format!(
        "{FILL}def rec negate_all :: ref mut [i64..] -> unit = \\values ->\n    let length = Array.length values\n    if length == 1 then\n        let value = values[0]\n        Array.write 0 (0 - value) values\n    elif length > 1 then\n        match Array.split_at_mut (length / 2) values with\n        | (left, right) ->\n            negate_all left\n            negate_all right\n"
    ));
    for name in ["fill", "negate_all"] {
        let function = ir
            .split("\ndefine ")
            .find(|function| function.contains(&format!("@tz.fn.Main.{name}(%tz.array")))
            .unwrap_or_else(|| panic!("{name}\n{ir}"));
        assert!(!function.contains("@tz.alloc"), "{function}");
    }
}

#[test]
fn parallel_chunks_lend_disjoint_exclusive_slices() {
    let ir = accepts(
        "def scale :: i64 -> ref mut [i64..] -> unit = \\start chunk ->\n    for index in 0i64 .. (chunk.length - 1) do Array.write index (start + index) chunk\n\nlet mut values: [i64] = [1, 2, 3]\nlet offset = 7i64\nParallel.for_each_chunk 2 scale (ref mut values)\nParallel.for_each_chunk 2 (\\start chunk -> Array.write 0 (start + offset) chunk) (ref mut values)\nlet mut words: [string] = [\"b\", \"a\"]\nParallel.for_each_chunk 1 (\\_start chunk -> Array.sort_in_place chunk) (ref mut words)\nvalues[0]\n",
    );
    assert!(ir.contains("@tsuzuri_tz_parallel("), "{ir}");
    for (source, code, fragment) in [
        (
            "let mut values: [i64] = [1, 2, 3]\nlet other: [i64] = [5]\nlet shared = ref other\nParallel.for_each_chunk 2 (\\start chunk -> Array.write 0 shared[0] chunk) (ref mut values)\nvalues[0]\n",
            "E1013",
            "parallel callbacks and values cannot retain borrowed environments",
        ),
        (
            "let mut values: [i64] = [1, 2, 3]\nlet run = Parallel.for_each_chunk 2\nrun (\\start chunk -> ()) (ref mut values)\n",
            "E1013",
            "must be fully applied directly",
        ),
        (
            "let text = \"a\"\nlet mut values: [ref string] = [ref text]\nParallel.for_each_chunk 1 (\\start chunk -> ()) (ref mut values)\n",
            "E1013",
            "ref string contains a reference",
        ),
        (
            "let mut values: [i64] = [1, 2, 3]\nlet shared = ref values\nParallel.for_each_chunk 2 (\\start chunk -> ()) (ref mut values)\nshared.length\n",
            "E1014",
            CONFLICT,
        ),
        (
            "def f :: ref [i64] -> unit = \\values -> Parallel.for_each_chunk 2 (\\start chunk -> ()) values\n",
            "E1014",
            "cannot mutate or exclusively reborrow through a shared reference",
        ),
        (
            "let mut values: [i64] = [1, 2, 3]\nParallel.for_each_chunk 2 (\\start chunk -> Array.write 0 1 values) (ref mut values)\n",
            "E1014",
            "",
        ),
    ] {
        rejects(source, code, fragment);
    }
}

fn rejects_with_kind(source: &str, code: &str, kind: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
}

/// A28 review: a write through an exclusive slice keeps what it stores in the slice's owner, as
/// an index assignment does. `Array.write`, and the std functions that write through it (`set`,
/// `fill`, `blit`, `Vec.copy_to`...), stored a function that borrows `text` or a `ref string`
/// without the owner keeping the borrow, so `text` could move or be freed while the array held
/// it.
#[test]
fn in_place_writes_keep_the_borrows_that_they_store() {
    const BORROW: &str =
        "let text = \"abc\"\nlet borrowed = ref text\nlet mut arr = [\\() -> 0, \\() -> 0]\n";
    const AFTER: &str = "let moved = text\narr[0] ()\n";
    for write in [
        "Array.write 0 (\\() -> String.length borrowed) (ref mut arr)",
        "Array.write 0 (\\() -> String.length borrowed) (ref mut arr[0..1])",
        "Array.set 0 (\\() -> String.length borrowed) (ref mut arr)",
        "Array.fill 0 2 (\\() -> String.length borrowed) (ref mut arr)",
        "Array.blit 0 0 1 [\\() -> String.length borrowed] (ref mut arr)",
        "Vec.copy_to (ref mut arr) (ref (Vec.of_array [\\() -> String.length borrowed]))",
        "Vec.copy_to_at (ref mut arr) 1 (ref (Vec.of_array [\\() -> String.length borrowed]))",
        "ref mut arr |> Array.set 0 (\\() -> String.length borrowed)",
        "let view = ref mut arr\nArray.write 1 (\\() -> String.length borrowed) view",
    ] {
        rejects_with_kind(
            &format!("{BORROW}{write}\n{AFTER}"),
            "E1014",
            "borrow_conflict",
        );
    }
    // A slice of references, and a function that writes into its own local array.
    rejects_with_kind(
        "let text = \"abc\"\nlet other = \"de\"\nlet mut refs = [ref other]\nArray.write 0 (ref text) (ref mut refs)\nlet moved = text\nrefs[0].length\n",
        "E1014",
        "borrow_conflict",
    );
    for (write, kind) in [
        (
            "Array.write 0 (\\() -> String.length text) (ref mut arr)",
            "borrow_outlives_block",
        ),
        (
            "Array.set 0 (\\() -> String.length text) (ref mut arr)",
            "borrow_outlives_block",
        ),
        (
            "Array.fill 0 1 (\\() -> String.length text) (ref mut arr)",
            "borrow_outlives_block",
        ),
        (
            "Array.write 0 (\\() -> String.length text) (ref mut arr[0..1])",
            "borrow_outlives_block",
        ),
    ] {
        // A28: the facts of `stash` follow the write, so `text` stays lent by its result.
        let source = format!(
            "def stash :: ref string -> own (unit -> i64) -> (unit -> i64)\nfn stash text g =\n    let mut arr = [g]\n    {write}\n    arr[0]\ndef leak :: own i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    stash (ref text) (\\() -> 1)\nlet f = leak 123456789\nf ()\n"
        );
        rejects_with_kind(&source, "E1013", kind);
        // The same write in one function, of a function that borrows the local `text`.
        let single = format!(
            "def leak :: own i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    let borrowed = ref text\n    let mut arr = [\\() -> 1]\n    {}\n    arr[0]\nlet f = leak 123456789\nf ()\n",
            write.replace("String.length text", "String.length borrowed")
        );
        rejects_with_kind(&single, "E1013", kind);
    }
    // A function that writes a value with borrows through its own exclusive reference needs named
    // regions, as an index assignment does; a function type cannot name one.
    rejects_with_kind(
        "def put :: own (unit -> i64) -> ref mut [(unit -> i64)..] -> unit\nfn put f target = Array.write 0 f target\n0\n",
        "E1013",
        "store_region_unnamed",
    );
    // The std writers on functions that hold no borrow, a borrow that outlives the array, and the
    // pipeline form.
    accepts(
        "def run :: unit -> i64\nfn run _unit =\n    let base = 1000\n    let mut arr = [\\() -> 1, \\() -> 2, \\() -> 3]\n    Array.set 0 (\\() -> 10) (ref mut arr)\n    Array.fill 1 1 (\\() -> 20) (ref mut arr)\n    let source = [\\() -> 100, \\() -> 200]\n    Array.blit 0 1 2 (ref source) (ref mut arr)\n    Array.random_shuffle_in_place_by (\\() -> 0.5) (ref mut arr)\n    let v = Vec.of_array [\\() -> 7, \\() -> 8]\n    Vec.copy_to_at (ref mut arr) 1 (ref v)\n    ref mut arr |> Array.set 2 (\\() -> base)\n    Array.write 0 (\\() -> base + 1) (ref mut arr)\n    arr[0] () + arr[1] () + arr[2] ()\n",
    );
    // A write of a value without borrows through a parameter's exclusive slice.
    accepts(
        "def reset :: ref mut [(unit -> i64)..] -> unit\nfn reset target = Array.write 0 (\\() -> 0) target\n",
    );
}

/// A28 review, second round: the values that the std writers, the generic writers of the program
/// and `MatrixView`'s writers store get the derived contract when the element type comes from a
/// type variable. A reference that a `own 'a` parameter becomes (`Array.set 0 (ref b) values`) is
/// a value that may be stored, and a value read from the target and written back stores nothing
/// new. Main accepts every program here; the rejected ones moved or dropped what the array kept.
#[test]
fn derived_contracts_store_references_and_functions_of_type_variables() {
    for source in [
        "let a = 1\nlet b = 2\nlet mut arr = [ref a]\nArray.set 0 (ref b) (ref mut arr)\nderef arr[0]\n",
        "let a = 1\nlet b = 2\nlet c = 3\nlet mut arr = [ref a, ref b]\nArray.fill 0 1 (ref c) (ref mut arr)\nderef arr[0]\n",
        "let a = 1\nlet b = 2\nlet c = 3\nlet mut arr = [ref a, ref b]\nref mut arr |> Array.set 1 (ref c)\nderef arr[1]\n",
        "def pick :: ref i64 -> ref i64 -> i64\nfn pick a b =\n    let mut arr = [a]\n    Array.set 0 b (ref mut arr)\n    deref arr[0]\nlet x = 1\nlet y = 7\npick (ref x) (ref y)\n",
        "let a = \"ab\"\nlet b = \"cde\"\nlet mut arr = [ref a, ref a]\nArray.set 1 (ref b) (ref mut arr)\nArray.fill 0 1 (ref b) (ref mut arr)\narr[0].length * 10 + arr[1].length\n",
        // Generic writers of the program, on function, reference and plain elements.
        "def put :: own 'a -> ref mut ['a..] -> unit\nfn put x arr = Array.write 0 x arr\nlet base = 5\nlet mut a = [\\() -> 1]\nput (\\() -> 9) (ref mut a)\nref mut a |> put (\\() -> base)\nlet p = 1\nlet q = 2\nlet mut refs = [ref p]\nput (ref q) (ref mut refs)\nlet mut numbers = [1]\nput 5 (ref mut numbers)\na[0] () + deref refs[0] + numbers[0]\n",
        "def put :: own 'a -> ref mut ['a..] -> unit\nfn put x arr = Array.set 0 x arr\ndef twice :: own 'a -> ref mut ['a..] -> unit\nfn twice x arr = put x arr\nlet mut a = [\\() -> 1]\ntwice (\\() -> 4) (ref mut a)\na[0] ()\n",
        "def put :: own i64 -> ref mut ['a..] -> unit\nfn put index arr =\n    let last = arr[index]\n    Array.write 0 last arr\nlet mut a = [\\() -> 1, \\() -> 2]\nput 1 (ref mut a)\na[0] ()\n",
        "def rotate :: ref mut [(unit -> i64)..] -> unit\nfn rotate arr =\n    let second = arr[1]\n    Array.write 0 second arr\nlet mut a = [\\() -> 1, \\() -> 2]\nrotate (ref mut a)\na[0] ()\n",
        // A generic writer through `ref mut 'a`, which main rejected as `store_region_unnamed`.
        "def set_ref :: own 'a -> ref mut 'a -> unit\nfn set_ref v r = deref r = v\nlet mut f = \\() -> 1\nset_ref (\\() -> 2) (ref mut f)\nf ()\n",
        // MatrixView's writers on function and reference elements.
        "let base = 1000\nlet source = [\\() -> 7, \\() -> 8]\nlet mut data = [\\() -> 1, \\() -> 2]\nlet mut grid = MatrixView.of_array_mut 1 2 (ref mut data)\nMatrixView.write 0 1 (\\() -> 20) (ref mut grid)\nMatrixView.fill (\\() -> base) (ref mut grid)\nMatrixView.copy_from (MatrixView.of_array 1 2 source) (ref mut grid)\nMatrixView.map_in_place (\\f -> \\() -> f () + 1) (ref mut grid)\nref mut grid |> MatrixView.write 0 0 (\\() -> base)\ndata[0] () + data[1] ()\n",
        "let a = 1\nlet b = 2\nlet mut data = [ref a, ref a]\nlet mut grid = MatrixView.of_array_mut 1 2 (ref mut data)\nMatrixView.write 0 1 (ref b) (ref mut grid)\nderef data[1]\n",
    ] {
        accepts(source);
    }
    for (source, code, kind) in [
        (
            "let a = \"ab\"\nlet b = \"cde\"\nlet mut arr = [ref a]\nArray.set 0 (ref b) (ref mut arr)\nlet moved = b\narr[0].length\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "let a = \"ab\"\nlet b = \"cde\"\nlet mut arr = [ref a]\nArray.fill 0 1 (ref b) (ref mut arr)\nlet moved = b\narr[0].length\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "let a = \"ab\"\nlet b = \"cde\"\nlet mut arr = [ref a]\nref mut arr |> Array.set 0 (ref b)\nlet moved = b\narr[0].length\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "let a = 1\nlet mut b = 2\nlet mut arr = [ref a]\nArray.set 0 (ref b) (ref mut arr)\nb = 5\nderef arr[0]\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "let other = \"x\"\nlet mut arr = [ref other]\nif other.length > 0 then\n    let local = \"abc\"\n    Array.set 0 (ref local) (ref mut arr)\narr[0].length\n",
            "E1013",
            "assignment_extends_borrow",
        ),
        (
            "let other = 1\nlet mut arr = [ref other]\nif other > 0 then\n    let local = 5\n    Array.fill 0 1 (ref local) (ref mut arr)\nderef arr[0]\n",
            "E1013",
            "assignment_extends_borrow",
        ),
        (
            "def put :: own 'a -> ref mut ['a..] -> unit\nfn put x arr = Array.write 0 x arr\nlet text = \"abc\"\nlet borrowed = ref text\nlet mut a = [\\() -> 1]\nput (\\() -> String.length borrowed) (ref mut a)\nlet moved = text\na[0] ()\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "def put :: own 'a -> ref mut ['a..] -> unit\nfn put x arr = Array.write 0 x arr\ndef leak :: own i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    let borrowed = ref text\n    let mut a = [\\() -> 1]\n    put (\\() -> String.length borrowed) (ref mut a)\n    a[0]\nlet f = leak 123456789\nf ()\n",
            "E1013",
            "borrow_outlives_block",
        ),
        (
            "def put :: own 'a -> ref mut ['a..] -> unit\nfn put x arr = Array.write 0 x arr\nlet a = \"ab\"\nlet b = \"cde\"\nlet mut refs = [ref a]\nput (ref b) (ref mut refs)\nlet moved = b\nrefs[0].length\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "def set_ref :: own 'a -> ref mut 'a -> unit\nfn set_ref v r = deref r = v\nlet text = \"abc\"\nlet borrowed = ref text\nlet mut f = \\() -> 1\nset_ref (\\() -> String.length borrowed) (ref mut f)\nlet moved = text\nf ()\n",
            "E1014",
            "borrow_conflict",
        ),
        // A partial application loses what the call stores, as for the std writers.
        (
            "def put :: own 'a -> ref mut ['a..] -> unit\nfn put x arr = Array.write 0 x arr\nlet f = put (\\() -> 1)\nlet mut a = [\\() -> 2]\nf (ref mut a)\na[0] ()\n",
            "E1013",
            "region_callback_stores_exclusive_input",
        ),
        // The stores of MatrixView's writers stay in the window's array after the window.
        (
            "let text = \"abc\"\nlet borrowed = ref text\nlet mut data = [\\() -> 1, \\() -> 2]\nlet mut grid = MatrixView.of_array_mut 1 2 (ref mut data)\nMatrixView.write 0 1 (\\() -> String.length borrowed) (ref mut grid)\nlet moved = text\ndata[1] ()\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "let a = \"ab\"\nlet b = \"cde\"\nlet mut data = [ref a, ref a]\nlet mut grid = MatrixView.of_array_mut 1 2 (ref mut data)\nMatrixView.write 0 1 (ref b) (ref mut grid)\nlet moved = b\ndata[1].length\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "let text = \"abc\"\nlet borrowed = ref text\nlet source = [\\() -> String.length borrowed]\nlet mut data = [\\() -> 1]\nlet mut grid = MatrixView.of_array_mut 1 1 (ref mut data)\nMatrixView.copy_from (MatrixView.of_array 1 1 source) (ref mut grid)\nlet moved = text\ndata[0] ()\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "let text = \"abc\"\nlet borrowed = ref text\nlet mut data = [\\() -> 1]\nlet mut grid = MatrixView.of_array_mut 1 1 (ref mut data)\nMatrixView.map_in_place (\\f -> \\() -> f () + String.length borrowed) (ref mut grid)\nlet moved = text\ndata[0] ()\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "let text = \"abc\"\nlet borrowed = ref text\nlet mut data = [\\() -> 1]\nlet mut grid = MatrixView.of_array_mut 1 1 (ref mut data)\nref mut grid |> MatrixView.fill (\\() -> String.length borrowed)\nlet moved = text\ndata[0] ()\n",
            "E1014",
            "borrow_conflict",
        ),
        (
            "def leak :: own i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    let borrowed = ref text\n    let mut data = [\\() -> 1]\n    let mut grid = MatrixView.of_array_mut 1 1 (ref mut data)\n    MatrixView.fill (\\() -> String.length borrowed) (ref mut grid)\n    data[0]\nlet f = leak 123456789\nf ()\n",
            "E1013",
            "borrow_outlives_block",
        ),
        (
            "let mut data = [\\() -> 1]\nlet mut grid = MatrixView.of_array_mut 1 1 (ref mut data)\nlet w = MatrixView.write 0 0 (\\() -> 2)\nw (ref mut grid)\ndata[0] ()\n",
            "E1013",
            "region_callback_stores_exclusive_input",
        ),
    ] {
        rejects_with_kind(source, code, kind);
    }
}
