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

/// A28 review: the check does not track what `Array.write`, and the std functions that write
/// through it, store in the owner of the target (A31). A function whose body lends an exclusive
/// borrow to a call therefore gets no facts: its result keeps every argument's loans, as before
/// A28, so `text` stays lent while `leak`'s result lives. Without that, `stash` stored a function
/// that borrows `text`, which `leak` frees, and the call `f ()` read freed memory. The A30 tip
/// rejects each program at the same place with the same kind.
#[test]
fn a_function_that_writes_in_place_keeps_its_arguments_loans() {
    for write in [
        "Array.write 0 (\\() -> String.length text) (ref mut arr)",
        "Array.set 0 (\\() -> String.length text) (ref mut arr)",
        "Array.fill 0 1 (\\() -> String.length text) (ref mut arr)",
        "Array.write 0 (\\() -> String.length text) (ref mut arr[0..1])",
        "ref mut arr |> Array.write 0 (\\() -> String.length text)",
        "Array.blit 0 0 1 [\\() -> String.length text] (ref mut arr)",
    ] {
        let stash = format!(
            "def stash :: ref string -> own (unit -> i64) -> (unit -> i64)\nfn stash text g =\n    let mut arr = [g]\n    {write}\n    arr[0]\n"
        );
        rejects_with_kind(
            &format!(
                "{stash}def leak :: own i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    stash (ref text) (\\() -> 1)\nlet f = leak 123456789\nf ()\n"
            ),
            "E1013",
            "borrow_outlives_block",
        );
        // While `text` lives, the result may hold its borrow.
        accepts(&format!(
            "{stash}let text = to_string 123456789\nlet f = stash (ref text) (\\() -> 1)\nf ()\n"
        ));
    }
    // A function that only passes the borrow to a writer that keeps it lends it too.
    rejects_with_kind(
        "def helper :: ref string -> (unit -> i64)\nfn helper text =\n    let mut a = [\\() -> 0]\n    Array.write 0 (\\() -> String.length text) (ref mut a)\n    a[0]\ndef stash :: ref string -> own (unit -> i64) -> (unit -> i64)\nfn stash text _g = helper text\ndef leak :: own i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    stash (ref text) (\\() -> 1)\nlet f = leak 123456789\nf ()\n",
        "E1013",
        "borrow_outlives_block",
    );
    // A callback that writes into the exclusive borrow that its caller lends it.
    rejects_with_kind(
        "def with_local :: own (ref mut [(unit -> i64)..] -> unit) -> (unit -> i64)\nfn with_local w =\n    let mut a = [\\() -> 0]\n    w (ref mut a)\n    a[0]\ndef stash :: ref string -> (unit -> i64)\nfn stash text = with_local (\\slot -> Array.write 0 (\\() -> String.length text) slot)\ndef leak :: own i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    stash (ref text)\nlet f = leak 123456789\nf ()\n",
        "E1013",
        "borrow_outlives_block",
    );
}

/// A28 review: programs that main accepts and that the attempt to track the in-place writes on
/// this branch (now A31) rejected: std writers on reference elements, generic writers of the
/// program (also with a declared region, or `ref 'a` elements), writers without type variables,
/// `MatrixView`'s writers and wrappers of them, and partial applications. A28 leaves them as main
/// has them.
#[test]
fn in_place_writes_that_main_accepts_stay_accepted() {
    for source in [
        "let a = 1\nlet b = 2\nlet mut arr = [ref a]\nArray.set 0 (ref b) (ref mut arr)\nderef arr[0]\n",
        "let a = 1\nlet b = 2\nlet c = 3\nlet mut arr = [ref a, ref b]\nArray.fill 0 1 (ref c) (ref mut arr)\nref mut arr |> Array.set 1 (ref c)\nderef arr[0] + deref arr[1]\n",
        "def pick :: ref i64 -> ref i64 -> i64\nfn pick a b =\n    let mut arr = [a]\n    Array.set 0 b (ref mut arr)\n    deref arr[0]\nlet x = 1\nlet y = 7\npick (ref x) (ref y)\n",
        "def put :: own 'a -> ref mut ['a..] -> unit\nfn put x arr = Array.write 0 x arr\nlet base = 5\nlet mut a = [\\() -> 1]\nput (\\() -> 9) (ref mut a)\nref mut a |> put (\\() -> base)\nlet p = 1\nlet q = 2\nlet mut refs = [ref p]\nput (ref q) (ref mut refs)\na[0] () + deref refs[0]\n",
        "def g {r} :: own 'a -> ref mut {r} ['a..] -> unit\nfn g x d = Array.write 0 x d\nlet base = 4\nlet mut data = [\\() -> 1]\ng (\\() -> base) (ref mut data)\ndata[0] ()\n",
        "def put :: ref 'a -> ref mut [ref 'a..] -> unit\nfn put x arr = Array.write 0 x arr\nlet p = 1\nlet q = 2\nlet mut a = [ref p]\nput (ref q) (ref mut a)\nderef a[0]\n",
        "def store {s} :: ref {s} string -> ref mut {s} [ref {s} string..] -> unit = \\x arr -> Array.write 0 x arr\nlet a = \"ab\"\nlet b = \"cde\"\nlet mut arr = [ref a]\nstore (ref b) (ref mut arr)\narr[0].length\n",
        "def put :: own (unit -> i64) -> ref mut [(unit -> i64)..] -> unit\nfn put f target = Array.write 0 f target\nlet base = 4\nlet mut arr = [\\() -> 1]\nput (\\() -> base) (ref mut arr)\narr[0] ()\n",
        "def rotate :: ref mut [(unit -> i64)..] -> unit\nfn rotate arr =\n    let second = arr[1]\n    Array.write 0 second arr\nlet mut a = [\\() -> 1, \\() -> 2]\nrotate (ref mut a)\na[0] ()\n",
        "def g {r} :: own 'a -> ref mut MatrixView.Mut<'a> {r} -> unit\nfn g x m = MatrixView.write 0 0 x m\nlet a = 1\nlet b = 2\nlet base = 4\nlet mut data = [\\() -> 1, \\() -> 2]\nlet mut grid = MatrixView.of_array_mut 1 2 (ref mut data)\ng (\\() -> base) (ref mut grid)\nlet mut cells = [ref a, ref a]\nlet mut row = MatrixView.of_array_mut 1 2 (ref mut cells)\ng (ref b) (ref mut row)\ndata[0] () + deref cells[0]\n",
        "let base = 1000\nlet source = [\\() -> 7, \\() -> 8]\nlet mut data = [\\() -> 1, \\() -> 2]\nlet mut grid = MatrixView.of_array_mut 1 2 (ref mut data)\nMatrixView.write 0 1 (\\() -> 20) (ref mut grid)\nMatrixView.fill (\\() -> base) (ref mut grid)\nMatrixView.copy_from (MatrixView.of_array 1 2 source) (ref mut grid)\nMatrixView.map_in_place (\\f -> \\() -> f () + 1) (ref mut grid)\nref mut grid |> MatrixView.write 0 0 (\\() -> base)\ndata[0] () + data[1] ()\n",
        "def put :: own 'a -> ref mut ['a..] -> unit\nfn put x arr = Array.write 0 x arr\nlet f = put (\\() -> 1)\nlet mut a = [\\() -> 2]\nf (ref mut a)\nlet w = Array.set 0 (\\() -> 5)\nw (ref mut a)\na[0] ()\n",
    ] {
        accepts(source);
    }
}
