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

const FILL: &str = "def fill :: ref mut [i64..] -> i64 -> unit = \\values value ->\n    for index in 0i64 .. (values.length - 1) do Array.write values index value\n";
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
    let error = analyze("def f :: ref mut [i64..] -> unit = \\values ->\n    let g = \\x -> Array.write values 0 x\n    g 1\n").expect_err("capture");
    assert!(
        error.message.contains("ref mut [i64..]"),
        "{}",
        error.message
    );
}

#[test]
fn exclusive_slices_borrow_named_mutable_arrays() {
    accepts(
        "let mut values: [i64] = [1, 2, 3, 4]\nlet a = ref mut values[1..3]\nArray.write a 0 9\nlet b = ref mut values[1..]\nArray.write b 0 8\nlet c = &mut values[..2]\nArray.write c 0 7\nlet d = ref mut values[1..values.length - 1]\nd.length\n",
    );
    accepts(
        "let mut values: [i64] = [1, 2, 3, 4]\nlet part = ref mut values[1..3]\nArray.write part 0 9\nlet size = values.length\nlet again = ref mut values[0..1]\nArray.write again 0 5\nsize + again.length\n",
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
        "let mut values: [i64] = [1, 2, 3]\nlet left = ref mut values[0..1]\nlet right = ref mut values[1..3]\nArray.write left 0 5\nright.length\n",
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
        "def tail :: ref mut [i64..] -> ref mut [i64..] = \\values -> ref mut values[1..]\n\ndef whole :: ref mut [i64] -> ref mut [i64..] = \\values -> ref mut values[0..]\n\nlet mut values: [i64] = [1, 2, 3]\nlet rest = tail (whole (ref mut values))\nArray.write rest 0 5\nrest.length\n",
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
            "def f :: ref mut [i64..] -> unit = \\values ->\n    let g = \\x -> Array.write values 0 x\n    g 1\n",
            "E1005",
            "cannot capture ref mut [i64..] in a reusable function",
        ),
        (
            "def f :: ref mut [i64..] -> i64 = \\values ->\n    let work = task { return values.length }\n    Task.run work\n",
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
            "def f :: ref mut [i64..] -> unit = \\values ->\n    for value in values do Array.write values 0 value\n",
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
        "{FILL}def whole :: ref mut [i64] -> unit = \\array -> fill array 1\n\nlet mut values: [i64] = [1, 2, 3, 4, 5]\nfill (ref mut values) 7\nfill values 7\nfill &mut values 7\nwhole (ref mut values)\nlet converted: ref mut [i64..] = ref mut values\nArray.write converted 0 9\nvalues.length\n"
    ));
    accepts(
        "def sum_shared :: ref [i64] -> i64 = \\values -> Array.sum values\n\nlet mut values: [i64] = [1, 2, 3]\nlet part = ref mut values[1..]\nsum_shared part + Array.sum part + Array.length part\n",
    );
    let ir = accepts(
        "def pass :: 'a -> 'a = \\value -> value\n\nlet mut values: [i64] = [1, 2, 3]\nlet whole = pass (ref mut values)\nlet size = whole.length\nlet part = pass (ref mut values[1..])\nArray.write part 0 5\nsize + part.length\n",
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
        "def run :: unit -> i64 = \\_unit ->\n    let mut values: [i64] = [1, 2, 3, 4]\n    let mut texts: [string] = [\"a\", \"b\", \"c\"]\n    let part = ref mut values[0..]\n    let mut index = 0i64\n    while index < 4 do\n        Array.write part index (index * 10)\n        index = index + 1\n    Array.swap_in part 0 3\n    let words = ref mut texts[0..]\n    Array.write words 0 \"xyz\"\n    Array.swap_in words 1 2\n    part[0] + words[0].length\n",
    );
    rejects(
        "let mut values: [i64] = [1, 2, 3]\nlet tail = ref mut values[1..]\nArray.write tail 1 (tail[1] * 10)\ntail.length\n",
        "E1014",
        CONFLICT,
    );
    let ir = accepts(
        "def put :: ref mut [string..] -> i64 -> string -> unit = \\values index text -> Array.write values index text\n",
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
        "def rec negate_all :: ref mut [i64..] -> unit = \\values ->\n    let length = Array.length values\n    if length == 1 then\n        let value = values[0]\n        Array.write values 0 (0 - value)\n    elif length > 1 then\n        match Array.split_at_mut values (length / 2) with\n        | (left, right) ->\n            negate_all left\n            negate_all right\n\ndef alternate :: ref mut [i64..] -> unit = \\values ->\n    match Array.split_at_mut values 2 with\n    | (left, right) ->\n        Array.write left 0 10\n        Array.write right 0 30\n        Array.write left 1 20\n        Array.write right 1 40\n",
    );
    rejects(
        "def f :: ref mut [i64..] -> unit = \\values ->\n    match Array.split_at_mut values 1 with\n    | (left, right) ->\n        Array.write values 0 1\n        Array.write left 0 2\n        Array.write right 0 3\n",
        "E1014",
        CONFLICT,
    );
    rejects(
        "def f :: ref mut [i64..] -> unit = \\values ->\n    match Array.split_at_mut values 1 with\n    | (left, right) ->\n        let inner = ref mut left[0..1]\n        Array.write right 0 1\n        Array.write inner 0 2\n",
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
        "{FILL}def rec negate_all :: ref mut [i64..] -> unit = \\values ->\n    let length = Array.length values\n    if length == 1 then\n        let value = values[0]\n        Array.write values 0 (0 - value)\n    elif length > 1 then\n        match Array.split_at_mut values (length / 2) with\n        | (left, right) ->\n            negate_all left\n            negate_all right\n"
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
        "def scale :: i64 -> ref mut [i64..] -> unit = \\start chunk ->\n    for index in 0i64 .. (chunk.length - 1) do Array.write chunk index (start + index)\n\nlet mut values: [i64] = [1, 2, 3]\nlet offset = 7i64\nParallel.for_each_chunk 2 scale (ref mut values)\nParallel.for_each_chunk 2 (\\start chunk -> Array.write chunk 0 (start + offset)) (ref mut values)\nlet mut words: [string] = [\"b\", \"a\"]\nParallel.for_each_chunk 1 (\\_start chunk -> Array.sort_in_place chunk) (ref mut words)\nvalues[0]\n",
    );
    assert!(ir.contains("@tsuzuri_task_parallel("), "{ir}");
    for (source, code, fragment) in [
        (
            "let mut values: [i64] = [1, 2, 3]\nlet other: [i64] = [5]\nlet shared = ref other\nParallel.for_each_chunk 2 (\\start chunk -> Array.write chunk 0 shared[0]) (ref mut values)\nvalues[0]\n",
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
            "let mut values: [i64] = [1, 2, 3]\nParallel.for_each_chunk 2 (\\start chunk -> Array.write values 0 1) (ref mut values)\n",
            "E1014",
            "",
        ),
    ] {
        rejects(source, code, fragment);
    }
}
