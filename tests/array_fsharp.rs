use tsuzuri::analyze;
use tsuzuri::driver::{Target, TestOptions, run_tests};

const BEHAVIOR: &str = include_str!("fixtures/array_fsharp/Main.tz");
const TRAPS: &str = include_str!("fixtures/array_fsharp_traps/Main.tz");
const STD_ARRAY: &str = include_str!("../std/Array.tz");

/// Every function of the F# `Array` module (FSharp.Core 9), spelled in snake_case. `Array.Parallel` is
/// not part of it: Tsuzuri has its own `Parallel` module.
const FSHARP_FUNCTIONS: [&str; 131] = [
    "all_pairs",
    "append",
    "average",
    "average_by",
    "blit",
    "choose",
    "chunk_by_size",
    "collect",
    "compare_with",
    "concat",
    "contains",
    "copy",
    "count_by",
    "create",
    "distinct",
    "distinct_by",
    "empty",
    "exactly_one",
    "except",
    "exists",
    "exists2",
    "fill",
    "filter",
    "find",
    "find_back",
    "find_index",
    "find_index_back",
    "fold",
    "fold2",
    "fold_back",
    "fold_back2",
    "forall",
    "forall2",
    "get",
    "group_by",
    "head",
    "indexed",
    "init",
    "insert_at",
    "insert_many_at",
    "is_empty",
    "item",
    "iter",
    "iter2",
    "iteri",
    "iteri2",
    "last",
    "length",
    "map",
    "map2",
    "map3",
    "map_fold",
    "map_fold_back",
    "mapi",
    "mapi2",
    "max",
    "max_by",
    "min",
    "min_by",
    "of_list",
    "of_seq",
    "pairwise",
    "partition",
    "partition_with",
    "permute",
    "pick",
    "random_choice",
    "random_choice_by",
    "random_choice_with",
    "random_choices",
    "random_choices_by",
    "random_choices_with",
    "random_sample",
    "random_sample_by",
    "random_sample_with",
    "random_shuffle",
    "random_shuffle_by",
    "random_shuffle_in_place",
    "random_shuffle_in_place_by",
    "random_shuffle_in_place_with",
    "random_shuffle_with",
    "reduce",
    "reduce_back",
    "remove_at",
    "remove_many_at",
    "replicate",
    "rev",
    "scan",
    "scan_back",
    "set",
    "singleton",
    "skip",
    "skip_while",
    "sort",
    "sort_by",
    "sort_by_descending",
    "sort_descending",
    "sort_in_place",
    "sort_in_place_by",
    "sort_in_place_with",
    "sort_with",
    "split_at",
    "split_into",
    "sub",
    "sum",
    "sum_by",
    "tail",
    "take",
    "take_while",
    "to_list",
    "to_seq",
    "transpose",
    "truncate",
    "try_exactly_one",
    "try_find",
    "try_find_back",
    "try_find_index",
    "try_find_index_back",
    "try_head",
    "try_item",
    "try_last",
    "try_pick",
    "unfold",
    "unzip",
    "unzip3",
    "update_at",
    "where",
    "windowed",
    "zero_create",
    "zip",
    "zip3",
];

/// The functions of `Array` that are compiler builtins instead of std source.
const BUILTINS: [&str; 2] = ["concat", "to_list"];

fn count_tests(source: &str) -> usize {
    source
        .lines()
        .filter(|line| line.starts_with("test \""))
        .count()
}

fn run(source: &str, target: Target, optimization: u8) -> Vec<(String, bool)> {
    let module =
        analyze(source).unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    let options = TestOptions {
        target,
        optimization,
        ..Default::default()
    };
    run_tests(&module, &options)
        .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message))
        .results
        .into_iter()
        .map(|result| (result.case.name, result.failure.is_none()))
        .collect()
}

#[test]
fn every_function_of_the_fsharp_array_module_is_defined() {
    let defined = |name: &str| {
        BUILTINS.contains(&name)
            || STD_ARRAY
                .lines()
                .any(|line| line.starts_with(&format!("def {name} ::")))
    };
    let missing: Vec<_> = FSHARP_FUNCTIONS
        .iter()
        .filter(|name| !defined(name))
        .collect();
    assert!(missing.is_empty(), "missing from std/Array.tz: {missing:?}");
}

#[test]
fn the_fsharp_functions_agree_with_their_documentation_on_every_target() {
    let expected = count_tests(BEHAVIOR);
    assert!(expected >= 39, "{expected} tests");
    for target in [Target::Native, Target::Wasm32] {
        for optimization in [0, 3] {
            let results = run(BEHAVIOR, target, optimization);
            assert_eq!(results.len(), expected);
            let failed: Vec<_> = results
                .iter()
                .filter(|(_, passed)| !passed)
                .map(|(name, _)| name)
                .collect();
            assert!(
                failed.is_empty(),
                "{target:?} -O{optimization} failed: {failed:?}"
            );
        }
    }
}

#[test]
fn contract_violations_trap_where_fsharp_raises() {
    let expected = count_tests(TRAPS);
    assert!(expected >= 90, "{expected} tests");
    for (target, optimization) in [(Target::Native, 0), (Target::Wasm32, 3)] {
        let results = run(TRAPS, target, optimization);
        assert_eq!(results.len(), expected);
        let survived: Vec<_> = results
            .iter()
            .filter(|(_, passed)| *passed)
            .map(|(name, _)| name)
            .collect();
        assert!(
            survived.is_empty(),
            "{target:?} -O{optimization} did not trap: {survived:?}"
        );
    }
}

fn accepts(source: &str) {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn callbacks_take_values_and_the_r_forms_borrow() {
    // F# code reads the same: callbacks receive the element, and arguments follow F#'s order.
    for source in [
        "[1, 2, 3] |> Array.filter (\\x -> x > 1) |> Array.map (\\x -> x * 2) |> Array.sum",
        "Array.exists (\\x -> x > 2) [1, 2, 3]",
        "Array.fold (\\total x -> total + x) 0 [1, 2, 3]",
        "Array.fold_back (\\x total -> total + x) [1, 2, 3] 0",
        "Array.contains 2 [1, 2, 3]",
        "Array.sort_by (\\x -> -x) [1, 2, 3]",
        "Array.try_find (\\x -> x > 1) [1, 2, 3]",
        "Array.zip [1, 2] [3, 4]",
        "Array.init 3 (\\i -> i * i)",
        "Array.replicate 3 7",
    ] {
        accepts(source);
    }
    // The r forms borrow each element, so the elements need not be Copy.
    for source in [
        "let words = [\"a\", \"bb\"]\nArray.mapr (\\w -> w.length) words",
        "let words = [\"a\", \"bb\"]\nArray.mapir (\\i w -> i + w.length) words",
        "let words = [\"a\", \"bb\"]\nArray.foldr (\\total w -> total + w.length) 0 words",
        "let words = [\"a\", \"bb\"]\nArray.foldr_back (\\w total -> total + w.length) words 0",
        "let words = [\"a\", \"bb\"]\nArray.iterr (\\w -> assert (w.length > 0)) words",
        "let words = [\"a\", \"bb\"]\nArray.iterir (\\i w -> assert (w.length > i)) words",
        "let words = [\"a\", \"bb\"]\nArray.existsr (\\w -> w.length == 2) words",
        "let words = [\"a\", \"bb\"]\nArray.forallr (\\w -> w.length > 0) words",
        "let words = [\"a\", \"bb\"]\nArray.countr (\\w -> w.length > 1) words",
        "let words = [\"a\", \"bb\"]\n(Array.findr (\\w -> w.length == 2) words).length",
        "let words = [\"a\", \"bb\"]\nmatch Array.try_findr (\\w -> w.length == 2) words with\n| Maybe.Some word -> word.length\n| Maybe.None -> 0",
        "let words = [\"a\", \"bb\"]\n(Array.minr words).length + (Array.maxr words).length",
        "let words = [\"a\", \"bb\"]\nArray.containsr \"a\" words",
        "let words = [\"a\", \"bb\"]\nfor word in Array.to_seqr words do ()",
    ] {
        accepts(source);
    }
    // The value forms copy the elements, so a string array is rejected.
    for source in [
        "Array.map (\\w -> w.length) [\"a\"]",
        "Array.fold (\\total w -> total + w.length) 0 [\"a\"]",
        "Array.exists (\\w -> w.length > 0) [\"a\"]",
        "Array.min [\"a\"]",
        "Array.head [\"a\"]",
        "Array.filter (\\w -> w.length > 0) [\"a\"]",
    ] {
        rejects(source, "E1005");
    }
}

#[test]
fn the_ref_suffix_is_spelled_r_and_the_old_names_are_gone() {
    for name in [
        "map_ref",
        "mapi_ref",
        "fold_ref",
        "fold_back_ref",
        "reverse",
        "any",
        "all",
    ] {
        rejects(&format!("let f = Array.{name}\n0"), "E1002");
    }
    for name in ["mapr", "mapir", "foldr", "foldr_back"] {
        assert!(
            STD_ARRAY
                .lines()
                .any(|line| line.starts_with(&format!("def {name} ::"))),
            "{name}"
        );
    }
}

#[test]
fn set_updates_in_place_and_update_at_consumes_the_array() {
    accepts("let mut values = [1, 2, 3]\nArray.set values 1 9\nvalues[1]");
    accepts("let values = [1, 2, 3]\nlet updated = Array.update_at 1 9 values\nupdated[1]");
    accepts("let mut values = [\"a\", \"b\"]\nArray.set values 0 \"z\"\nvalues.length");
    // `set` needs a mutable array, like `write`.
    rejects("let values = [1, 2, 3]\nArray.set values 1 9\n0", "E1014");
    // The consumed array cannot be read again when it is not Copy.
    rejects(
        "let values = [\"a\", \"b\"]\nlet updated = Array.update_at 0 \"z\" values\nvalues.length",
        "E1012",
    );
    // A statement of another type than unit is rejected, which catches the old functional use of `set`.
    rejects(
        "let mut values = [1, 2, 3]\nvalues = Array.set values 1 9\n0",
        "E1003",
    );
}

#[test]
fn fsharp_idioms_report_their_type_errors() {
    rejects("Array.map (\\x -> x) [1] [2]", "E1006");
    rejects("Array.init 3 (\\i -> i) 4", "E1006");
    rejects("Array.zip [1] [\"a\"] == 3", "E1003");
    // An index is an i64, like every size.
    rejects("Array.take 1i32 [1, 2]", "E1003");
}
