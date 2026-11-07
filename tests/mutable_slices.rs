use tsuzuri::syntax::Edition;
use tsuzuri::{analyze, analyze_all, analyze_with_edition, llvm};

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

/// A28 review: `Array.write`, and the std functions that write through it, store a function that
/// borrows `text` in `arr`, and `stash` returns it, so `text` stays lent while `leak`'s result
/// lives. Without that, `leak` freed `text` and the call `f ()` read freed memory. A28 gave a body
/// that lends an exclusive borrow to a call no facts; since A31 the store facts put what the write
/// stores in `arr`'s loans, and the facts of `stash` keep `text` in its result. The A30 tip rejects
/// each program at the same place with the same kind.
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

/// The 1-based line and column of the byte `offset` of `source`.
fn position(source: &str, offset: usize) -> (usize, usize) {
    let before = &source[..offset];
    (
        before.matches('\n').count() + 1,
        before.len() - before.rfind('\n').map_or(0, |at| at + 1) + 1,
    )
}

/// A31: the first error of `source`, in both editions, has `code` and `kind` and starts at
/// `line:column` of `source`.
fn rejects_at(source: &str, code: &str, kind: &str, at: (usize, usize)) {
    for edition in [Edition::E2026, Edition::E2027] {
        let error = analyze_with_edition(source, edition).expect_err(source);
        assert_eq!(
            (error.code, error.kind),
            (code, Some(kind)),
            "edition {edition}\n{source}\n{}",
            error.message
        );
        assert_eq!(
            position(source, error.span.start),
            at,
            "edition {edition}\n{source}\n{}",
            error.message
        );
    }
}

/// A31: `source` is accepted in both editions, with deterministic IR.
fn accepts_in_both(source: &str) {
    accepts(source);
    let module = analyze_with_edition(source, Edition::E2027)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit_target(&module, llvm::Entry::Library, false).unwrap();
    assert_eq!(
        ir,
        llvm::emit_target(&module, llvm::Entry::Library, false).unwrap(),
        "{source}"
    );
}

/// A31: `Array.write`, and `Array.set`, `Array.fill` and a slice of the array that write through
/// it, keep the borrows of the written value in the owner of the target, as `arr[0] = v` does: the
/// array that holds a function borrowing `text` cannot leave the block that frees `text`. Each
/// program is the reproduction of the ticket (`h1` to `h4`, `h7`), which main accepts and which
/// then reads freed memory; the diagnostic is the one of the assignment `arr[0] = ...` in the same
/// place (`h0`, `Main.tz:7:5`).
#[test]
fn an_in_place_write_keeps_the_written_borrow_in_the_owner() {
    for write in [
        "Array.write 0 (\\() -> String.length borrowed) (ref mut arr)",
        "Array.set 0 (\\() -> String.length borrowed) (ref mut arr)",
        "Array.fill 0 1 (\\() -> String.length borrowed) (ref mut arr)",
        "Array.write 0 (\\() -> String.length borrowed) (ref mut arr[0..1])",
    ] {
        rejects_at(
            &format!(
                "def leak :: i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    let borrowed = ref text\n    let mut arr = [\\() -> 1]\n    {write}\n    arr[0]\nlet f = leak 123456789\nf ()"
            ),
            "E1013",
            "borrow_outlives_block",
            (7, 5),
        );
    }
    // h7: the array itself leaves the block.
    rejects_at(
        "def leak :: i64 -> [(unit -> i64)]\nfn leak n =\n    let text = to_string n\n    let borrowed = ref text\n    let mut arr = [\\() -> 1]\n    Array.write 0 (\\() -> String.length borrowed) (ref mut arr)\n    arr\nlet f = leak 123456789\nf[0] ()",
        "E1013",
        "borrow_outlives_block",
        (7, 5),
    );
    // h9: a reference element that `Array.set` writes outlives the block of its owner, as with
    // `arr[0] = ref local` (`Main.tz:4:19`).
    rejects_at(
        "def leak :: i64 -> ref string -> i64\nfn leak n other =\n    let mut arr = [other]\n    if n > 0 then {\n        let local = to_string n;\n        Array.set 0 (ref local) (ref mut arr)\n    } else ()\n    arr[0].length\nlet o = \"ab\"\nleak 123456789 (ref o)",
        "E1013",
        "assignment_extends_borrow",
        (4, 19),
    );
}

/// A31: while the array holds what a write stored, the borrowed `text` cannot move (`h6`; main
/// accepts it and `arr[0] ()` reads the moved string). The same holds for a generic writer of the
/// program and one without type variables (their bodies write with `Array.write`, and the facts of
/// the call store the value in the caller's array), and for `MatrixView.write` through a window of
/// `data`, which reaches the owner of the window's array. Each diagnostic is the one of
/// `arr[0] = ...` (or `data[0] = ...`) in place of the write.
#[test]
fn a_written_borrow_keeps_its_source_from_moving() {
    for last in ["arr[0] ()", "arr[0] () * 100 + moved.length"] {
        rejects_at(
            &format!(
                "let text = to_string 123456789\nlet borrowed = ref text\nlet mut arr = [\\() -> 1]\nArray.write 0 (\\() -> String.length borrowed) (ref mut arr)\nlet moved = text\n{last}"
            ),
            "E1014",
            "borrow_conflict",
            (5, 13),
        );
    }
    for put in [
        "def put :: own 'a -> ref mut ['a..] -> unit\nfn put x arr = Array.write 0 x arr\n",
        "def put :: own (unit -> i64) -> ref mut [(unit -> i64)..] -> unit\nfn put f target = Array.write 0 f target\n",
    ] {
        rejects_at(
            &format!(
                "{put}let text = to_string 123456789\nlet borrowed = ref text\nlet mut arr = [\\() -> 1]\nput (\\() -> String.length borrowed) (ref mut arr)\nlet moved = text\narr[0] () * 100 + moved.length"
            ),
            "E1014",
            "borrow_conflict",
            (7, 13),
        );
    }
    rejects_at(
        "let text = to_string 123456789\nlet borrowed = ref text\nlet mut data = [\\() -> 1, \\() -> 2]\nlet mut grid = MatrixView.of_array_mut 1 2 (ref mut data)\nMatrixView.write 0 0 (\\() -> String.length borrowed) (ref mut grid)\nlet moved = text\ndata[0] () * 100 + moved.length",
        "E1014",
        "borrow_conflict",
        (6, 13),
    );
}

/// A31: a function that writes a borrow of its own local into the target of its `ref mut`
/// parameter hands the caller a borrow that dies when the function returns. No fact of the call
/// can keep it alive, so the definition is rejected where it writes (main accepts it, and
/// `arr[0] ()` reads the freed `s`). The assignment `target[0] = ...` is rejected there too
/// (`store_region_unnamed`).
#[test]
fn a_write_of_a_local_borrow_through_a_parameter_is_rejected_where_it_is_made() {
    rejects_at(
        "def bad :: ref mut [(unit -> i64)..] -> unit\nfn bad target =\n    let s = to_string 123456789\n    let b = ref s\n    Array.write 0 (\\() -> String.length b) target\nlet mut arr = [\\() -> 1]\nbad (ref mut arr)\narr[0] ()",
        "E1013",
        "store_local_borrow_through_reference",
        (5, 5),
    );
}

/// A31: a read parameter of edition 2027 is a shared reference to the caller's value. A function
/// value given to one is called through `deref`, and a value given to one is lent as a borrowed
/// temporary, whose value is what the temporary holds. The facts follow both: `apply_read` calls
/// its function-valued read parameter with its array (a higher-order fact, as for an `own`
/// parameter), and `put_read` stores the value behind its read parameter (the temporary's
/// contents). Each program reads the freed `text` with the stage 3 compiler in edition 2027 (and
/// is rejected in edition 2026, whose parameters are values); the diagnostic is the one of
/// `arr[0] = ...` in place of the write: the array that holds a function borrowing `text` leaves
/// the block that frees it. A function value that writes an owned function, or that only reads
/// what it captured, stays accepted (13 and 6 by hand).
#[test]
fn a_read_parameter_keeps_what_its_writer_stores() {
    const APPLY: &str = "def apply_read :: (ref mut [(unit -> i64)..] -> unit) -> ref mut [(unit -> i64)..] -> unit\nfn apply_read w arr = w arr\n";
    const PUT: &str = "def put_read :: (unit -> i64) -> ref mut [(unit -> i64)..] -> unit\nfn put_read x arr = Array.write 0 x arr\n";
    const PUT_GENERIC: &str =
        "def put_read :: 'a -> ref mut ['a..] -> unit\nfn put_read x arr = Array.write 0 x arr\n";
    for (header, write) in [
        (
            APPLY,
            "apply_read (Array.set 0 (\\() -> String.length borrowed)) (ref mut arr)",
        ),
        (
            APPLY,
            "apply_read (\\a -> Array.write 0 (\\() -> String.length borrowed) a) (ref mut arr)",
        ),
        (
            APPLY,
            "apply_read (Array.write 0 (\\() -> String.length borrowed)) (ref mut arr)",
        ),
        (
            PUT,
            "put_read (\\() -> String.length borrowed) (ref mut arr)",
        ),
        (
            PUT_GENERIC,
            "put_read (\\() -> String.length borrowed) (ref mut arr)",
        ),
    ] {
        rejects_at(
            &format!(
                "{header}def leak :: i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    let borrowed = ref text\n    let mut arr = [\\() -> 1]\n    {write}\n    arr[0]\nlet f = leak 123456789\nf ()"
            ),
            "E1013",
            "borrow_outlives_block",
            (9, 5),
        );
    }
    // A partial application of a named writer over a reference parameter.
    rejects_at(
        &format!(
            "def put :: ref string -> ref mut [(unit -> i64)..] -> unit\nfn put t a = Array.write 0 (\\() -> String.length t) a\n{APPLY}def leak :: i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    let mut arr = [\\() -> 1]\n    apply_read (put (ref text)) (ref mut arr)\n    arr[0]\nlet f = leak 123456789\nf ()"
        ),
        "E1013",
        "borrow_outlives_block",
        (10, 5),
    );
    // The borrowed `text` moves while `arr` lives.
    rejects_at(
        &format!(
            "{APPLY}let text = to_string 123\nlet b = ref text\nlet mut arr = [\\() -> 1]\napply_read (Array.set 0 (\\() -> String.length b)) (ref mut arr)\nlet moved = text\narr[0] () + moved.length"
        ),
        "E1014",
        "borrow_conflict",
        (7, 13),
    );
    for source in [
        format!(
            "{APPLY}let text = to_string 123\nlet b = ref text\nlet mut arr = [\\() -> 1]\napply_read (\\_a -> ()) (ref mut arr)\napply_read (\\a -> Array.write 0 (\\() -> 7) a) (ref mut arr)\nlet n = b.length\nlet moved = text\nn + moved.length + arr[0] ()"
        ),
        format!(
            "{APPLY}let text = to_string 123\nlet b = ref text\nlet mut arr = [\\() -> 1]\napply_read (\\a -> {{ let k = b.length; Array.write 0 (\\() -> k) a }}) (ref mut arr)\nlet moved = text\nmoved.length + arr[0] ()"
        ),
    ] {
        accepts_in_both(&source);
    }
}

/// A31: writes that store nothing new in the owner stay accepted (main accepts them; the values
/// are checked by the `in_place_writes` suite of `tests/features.mjs`): reference elements written
/// with std writers, partial applications of writers, `MatrixView` wrappers, write-backs of an
/// element read from the same target (`put_last_first`, `bump`, two `rotate`s before the array
/// moves), `Array.blit` before its source is written (the source's element, not the borrow of the
/// source, goes into `dst`), and a function value that only reads what it captured.
#[test]
fn writes_that_store_nothing_new_stay_accepted() {
    for source in [
        "let a = \"a\"\nlet b = \"abc\"\nlet mut arr = [ref a, ref a]\nArray.set 1 (ref b) (ref mut arr)\narr[1].length",
        "let a = 1\nlet b = 2\nlet c = 3\nlet mut arr = [ref a, ref b, ref a]\nref mut arr |> Array.set 0 (ref c)\nderef arr[0] * 100 + deref arr[1] * 10 + deref arr[2] + 2",
        "let a = 1\nlet b = 2\nlet c = 3\nlet mut arr = [ref a, ref b, ref b]\nArray.fill 0 1 (ref c) (ref mut arr)\nderef arr[0] * 100 + deref arr[1] * 10 + deref arr[2]",
        "let mut a = [\\() -> 2]\nlet w = Array.set 0 (\\() -> 5)\nw (ref mut a)\na[0] ()",
        "def put :: own 'a -> ref mut ['a..] -> unit\nfn put x arr = Array.write 0 x arr\nlet f = put (\\() -> 1)\nlet mut a = [\\() -> 2]\nf (ref mut a)\na[0] ()",
        "let mut data = [\\() -> 1]\nlet mut grid = MatrixView.of_array_mut 1 1 (ref mut data)\nlet w = MatrixView.write 0 0 (\\() -> 2)\nw (ref mut grid)\ndata[0] ()",
        "def g :: own 'a -> ref mut MatrixView.Mut<'a> -> unit\nfn g x m = MatrixView.write 0 0 x m\nlet base = 4\nlet mut data = [\\() -> 1, \\() -> 2]\nlet mut grid = MatrixView.of_array_mut 1 2 (ref mut data)\ng (\\() -> base) (ref mut grid)\ndata[0] ()",
        "def g {r} :: Copy<'a> => own 'a -> ref mut MatrixView.Mut<'a> {r} -> unit\nfn g x m = MatrixView.fill x m\nlet base = 4\nlet mut data = [\\() -> 1, \\() -> 2]\nlet mut grid = MatrixView.of_array_mut 1 2 (ref mut data)\ng (\\() -> base) (ref mut grid)\nlet a = 1\nlet b = 2\nlet mut cells = [ref a, ref a]\nlet mut row = MatrixView.of_array_mut 1 2 (ref mut cells)\ng (ref b) (ref mut row)\ndata[0] () + deref cells[0]",
        "def put_last_first :: ref mut [(unit -> i64)..] -> unit\nfn put_last_first arr =\n    let last = arr[arr.length - 1]\n    Array.write 0 last arr\nlet mut a = [\\() -> 1, \\() -> 2]\nput_last_first (ref mut a)\na[0] ()",
        "def bump :: ref mut [(unit -> i64)..] -> unit\nfn bump arr =\n    for i in 0 ..= (arr.length - 1) do\n        let f = arr[i]\n        Array.write i (\\() -> f () + 1) arr\nlet mut a = [\\() -> 1, \\() -> 2]\nbump (ref mut a)\na[0] () + a[1] ()",
        "def rotate :: ref mut [(unit -> i64)..] -> unit\nfn rotate arr =\n    let second = arr[1]\n    Array.write 0 second arr\nlet mut a = [\\() -> 1, \\() -> 2]\nrotate (ref mut a)\nrotate (ref mut a)\nlet b = a\nb[0] () + b[1] ()",
        "let a = 1\nlet b = 2\nlet mut src = [ref a]\nlet mut dst = [ref b]\nArray.blit 0 0 1 (ref src) (ref mut dst)\nArray.set 0 (ref b) (ref mut src)\nderef dst[0] * 10 + deref src[0]",
        "def apply :: own (ref mut [(unit -> i64)..] -> i64) -> ref mut [(unit -> i64)..] -> i64\nfn apply g arr = g arr\nlet text = to_string 123456789\nlet borrowed = ref text\nlet mut arr = [\\() -> 3]\nlet n = apply (\\a -> a[0] () + String.length borrowed) (ref mut arr)\nlet moved = text\narr[0] () + n + moved.length",
    ] {
        accepts_in_both(source);
    }
}

/// A31: the writes that main rejects stay rejected with the same diagnostics, and tracking the
/// writes adds none: function elements sorted in place (rejected inside std), slices whose element
/// region a type cannot name, stores through a `ref mut` parameter without a named target region
/// (also of values that hold no borrow), and a parallel callback that borrows its environment.
#[test]
fn writes_that_main_rejects_keep_their_diagnostics() {
    let array = include_str!("../std/Array.tz");
    for (source, expected) in [
        (
            "let mut arr = [\\() -> 2, \\() -> 1]\nArray.sort_in_place_with (\\x y -> x () - y ()) (ref mut arr)\narr[0] ()",
            &[
                ("E1013", "task_result_retains_borrow", (2003, 32)),
                ("E1014", "borrow_conflict", (2084, 15)),
            ][..],
        ),
        (
            "let mut arr = [\\() -> 2, \\() -> 1]\nArray.sort_in_place_by (\\x -> x ()) (ref mut arr)\narr[0] ()",
            &[("E1014", "borrow_conflict", (2098, 15))][..],
        ),
    ] {
        let found: Vec<_> = analyze_all(source)
            .expect_err(source)
            .diagnostics
            .iter()
            .map(|error| {
                (
                    error.code,
                    error.kind.unwrap_or(""),
                    position(array, error.span.start),
                )
            })
            .collect();
        assert_eq!(found, expected, "{source}");
        let first = analyze_with_edition(source, Edition::E2027).expect_err(source);
        assert_eq!(
            (
                first.code,
                first.kind.unwrap_or(""),
                position(array, first.span.start)
            ),
            expected[0],
            "edition 2027\n{source}"
        );
    }
    for (source, code, kind, at) in [
        (
            "def store {r s} :: ref {s} string -> ref mut {r} [ref {s} string..] {s} -> unit = \\x arr -> Array.write 0 x arr\n0",
            "E1005",
            "unsized_slice_type_position",
            (1, 50),
        ),
        (
            "def store {r s} :: ref {s} string -> ref mut {r} [ref {s} string..] -> unit = \\x arr -> Array.write 0 x arr\n0",
            "E1013",
            "field_mixes_regions",
            (1, 38),
        ),
        (
            "def push :: ref mut Vec<(unit -> i64)> -> unit\nfn push v = Vec.push_mut (\\() -> 1) v\n0",
            "E1013",
            "store_region_unnamed",
            (2, 13),
        ),
        (
            "def set_ref :: own 'a -> ref mut 'a -> unit\nfn set_ref v r = deref r = v\nlet base = 2\nlet mut x = \\() -> 1\nset_ref (\\() -> base) (ref mut x)\nx ()",
            "E1013",
            "store_region_unnamed",
            (2, 18),
        ),
        (
            "def push :: own 'a -> ref mut Vec<'a> -> unit\nfn push x v = Vec.push_mut x v\nlet base = 2\nlet mut v: Vec<(unit -> i64)> = Vec.empty()\npush (\\() -> base) (ref mut v)\nVec.length (ref v)",
            "E1013",
            "store_region_unnamed",
            (2, 15),
        ),
        (
            "let text = to_string 123456789\nlet borrowed = ref text\nlet mut arr = [\\() -> 1, \\() -> 2]\nParallel.for_each_chunk 1 (\\_start chunk -> Array.write 0 (\\() -> String.length borrowed) chunk) (ref mut arr)\nlet moved = text\narr[0] () * 100 + moved.length",
            "E1013",
            "parallel_callback_borrows_environment",
            (4, 27),
        ),
    ] {
        rejects_at(source, code, kind, at);
    }
}
