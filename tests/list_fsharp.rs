use tsuzuri::driver::{Target, TestOptions, run_tests};
use tsuzuri::{analyze, analyze_modules, llvm};

const BEHAVIOR: &str = include_str!("fixtures/list_fsharp/Main.tz");
const TRAPS: &str = include_str!("fixtures/list_fsharp_traps/Main.tz");
const STD_LIST: &str = include_str!("../std/List.tz");

/// Every function of the F# `List` module (FSharp.Core 9) that is not obsolete, spelled in snake_case. `List.nth` is
/// left out: F# marks it obsolete in favour of `List.item`.
const FSHARP_FUNCTIONS: [&str; 117] = [
    "all_pairs",
    "append",
    "average",
    "average_by",
    "choose",
    "chunk_by_size",
    "collect",
    "compare_with",
    "concat",
    "contains",
    "count_by",
    "distinct",
    "distinct_by",
    "empty",
    "exactly_one",
    "except",
    "exists",
    "exists2",
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
    "of_array",
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
    "random_shuffle_with",
    "reduce",
    "reduce_back",
    "remove_at",
    "remove_many_at",
    "replicate",
    "rev",
    "scan",
    "scan_back",
    "singleton",
    "skip",
    "skip_while",
    "sort",
    "sort_by",
    "sort_by_descending",
    "sort_descending",
    "sort_with",
    "split_at",
    "split_into",
    "sum",
    "sum_by",
    "tail",
    "take",
    "take_while",
    "to_array",
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
    "zip",
    "zip3",
];

/// The functions of `List` that are compiler builtins instead of std source: `cons` is Tsuzuri's own, `mapr` and
/// `foldr` are the borrowing forms of `map` and `fold`.
const BUILTINS: [&str; 7] = ["cons", "foldr", "map", "mapr", "rev", "tail", "to_array"];

/// The `r` forms, which borrow each element: they are the Tsuzuri spelling of what other modules call `_ref`.
const BORROWING_FORMS: [&str; 14] = [
    "containsr",
    "existsr",
    "findr",
    "foldr",
    "foldr_back",
    "forallr",
    "iterir",
    "iterr",
    "mapir",
    "mapr",
    "maxr",
    "minr",
    "to_seqr",
    "try_findr",
];

fn count_tests(source: &str) -> usize {
    source
        .lines()
        .filter(|line| line.starts_with("test \""))
        .count()
}

/// One test of a fixture: its name, why it failed (a trap, a timeout, ...) or `None` when it passed, and the end of
/// its standard error.
struct Outcome {
    name: String,
    failure: Option<String>,
    output: String,
}

fn run(source: &str, target: Target, optimization: u8) -> Vec<Outcome> {
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
        .map(|result| Outcome {
            name: result.case.name,
            failure: result.failure,
            output: result.output,
        })
        .collect()
}

fn defined(name: &str) -> bool {
    BUILTINS.contains(&name)
        || STD_LIST
            .lines()
            .any(|line| line.starts_with(&format!("def {name} ::")))
}

#[test]
fn every_function_of_the_fsharp_list_module_is_defined() {
    let missing: Vec<_> = FSHARP_FUNCTIONS
        .iter()
        .filter(|name| !defined(name))
        .collect();
    assert!(missing.is_empty(), "missing from std/List.tz: {missing:?}");
}

#[test]
fn the_fsharp_functions_agree_with_their_documentation_on_every_target() {
    let expected = count_tests(BEHAVIOR);
    assert!(expected >= 44, "{expected} tests");
    for target in [Target::Native, Target::Wasm32] {
        for optimization in [0, 3] {
            let results = run(BEHAVIOR, target, optimization);
            assert_eq!(results.len(), expected);
            let failed: Vec<_> = results
                .iter()
                .filter_map(|outcome| {
                    outcome
                        .failure
                        .as_ref()
                        .map(|reason| format!("{}: {reason}", outcome.name))
                })
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
    assert!(expected >= 100, "{expected} tests");
    for (target, optimization) in [(Target::Native, 0), (Target::Wasm32, 3)] {
        let results = run(TRAPS, target, optimization);
        assert_eq!(results.len(), expected);
        let survived: Vec<_> = results
            .iter()
            .filter(|outcome| outcome.failure.is_none())
            .map(|outcome| &outcome.name)
            .collect();
        assert!(
            survived.is_empty(),
            "{target:?} -O{optimization} did not trap: {survived:?}"
        );
        // A test that hangs is killed by the runner and reported as a failure too: that is not a trap.
        let hung: Vec<_> = results
            .iter()
            .filter(|outcome| {
                outcome
                    .failure
                    .as_deref()
                    .is_some_and(|reason| reason.starts_with("timed out"))
            })
            .map(|outcome| &outcome.name)
            .collect();
        assert!(
            hung.is_empty(),
            "{target:?} -O{optimization} timed out instead of trapping: {hung:?}"
        );
    }
}

#[test]
fn functions_of_two_lists_visit_the_shared_pairs_before_they_trap() {
    // The callbacks of these tests print the first element of each pair they get: 1 and 2 are the pairs that
    // `[1, 2, 3]` and `[1, 2]` share, so 3 never comes, and the trap of the lengths follows.
    for optimization in [0, 3] {
        let results = run(TRAPS, Target::Native, optimization);
        let ordered: Vec<_> = results
            .iter()
            .filter(|outcome| outcome.name.contains("on the shared"))
            .collect();
        assert_eq!(ordered.len(), 8, "{:?}", ordered.len());
        for outcome in ordered {
            assert!(outcome.failure.is_some(), "{} did not trap", outcome.name);
            assert!(
                outcome.output.contains("pair 1") && outcome.output.contains("pair 2"),
                "-O{optimization} {} did not call the callback on the shared pairs: {:?}",
                outcome.name,
                outcome.output
            );
            assert!(
                !outcome.output.contains("pair 3"),
                "-O{optimization} {} went past the shorter list: {:?}",
                outcome.name,
                outcome.output
            );
        }
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
        "[|1, 2, 3|] |> List.filter (\\x -> x > 1) |> List.map (\\x -> x * 2) |> List.sum",
        "List.exists (\\x -> x > 2) [|1, 2, 3|]",
        "List.fold (\\total x -> total + x) 0 [|1, 2, 3|]",
        "List.fold_back (\\x total -> total + x) [|1, 2, 3|] 0",
        "List.contains 2 [|1, 2, 3|]",
        "List.sort_by (\\x -> -x) [|1, 2, 3|]",
        "List.try_find (\\x -> x > 1) [|1, 2, 3|]",
        "List.zip [|1, 2|] [|3, 4|]",
        "List.init 3 (\\i -> i * i)",
        "List.replicate 3 7",
        "List.cons 1 [|2, 3|]",
    ] {
        accepts(source);
    }
    // The r forms borrow each element, so the elements need not be Copy.
    for source in [
        "let words = [|\"a\", \"bb\"|]\nList.mapr (\\w -> w.length) words",
        "let words = [|\"a\", \"bb\"|]\nList.mapir (\\i w -> i + w.length) words",
        "let words = [|\"a\", \"bb\"|]\nList.foldr (\\total w -> total + w.length) 0 words",
        "let words = [|\"a\", \"bb\"|]\nList.foldr_back (\\w total -> total + w.length) words 0",
        "let words = [|\"a\", \"bb\"|]\nList.iterr (\\w -> assert (w.length > 0)) words",
        "let words = [|\"a\", \"bb\"|]\nList.iterir (\\i w -> assert (w.length > i)) words",
        "let words = [|\"a\", \"bb\"|]\nList.existsr (\\w -> w.length == 2) words",
        "let words = [|\"a\", \"bb\"|]\nList.forallr (\\w -> w.length > 0) words",
        "let words = [|\"a\", \"bb\"|]\n(List.findr (\\w -> w.length == 2) words).length",
        "let words = [|\"a\", \"bb\"|]\nmatch List.try_findr (\\w -> w.length == 2) words with\n| Maybe.Some word -> word.length\n| Maybe.None -> 0",
        "let words = [|\"a\", \"bb\"|]\n(List.minr words).length + (List.maxr words).length",
        "let words = [|\"a\", \"bb\"|]\nList.containsr \"a\" words",
        "let words = [|\"a\", \"bb\"|]\nfor word in List.to_seqr words do ()",
    ] {
        accepts(source);
    }
    // The value forms copy the elements, so a string list is rejected.
    for source in [
        "List.map (\\w -> w.length) [|\"a\"|]",
        "List.mapi (\\i w -> i + w.length) [|\"a\"|]",
        "List.fold (\\total w -> total + w.length) 0 [|\"a\"|]",
        "List.exists (\\w -> w.length > 0) [|\"a\"|]",
        "List.min [|\"a\"|]",
        "List.head [|\"a\"|]",
        "List.filter (\\w -> w.length > 0) [|\"a\"|]",
        "List.rev [|\"a\"|]",
    ] {
        rejects(source, "E1005");
    }
}

#[test]
fn list_literals_are_borrowed_like_array_literals() {
    // `Array.length [1, 2]` has always borrowed its temporary; a list literal does the same.
    for source in [
        "List.length [|1, 2, 3|]",
        "List.length [|\"a\", \"b\"|]",
        "List.map (\\x -> x + 1) [|1, 2|]",
        "List.rev [|1, 2|]",
        "List.fold (\\total x -> total + x) 0 [|1, 2|]",
        "List.zip [|1, 2|] [|3, 4|]",
        "[|1, 2|] |> List.length",
        "List.length (List.cons 1 [|2|])",
    ] {
        accepts(source);
    }
    // A result that borrows from the argument still needs a name for it to borrow from.
    rejects("let smallest = List.minr [|1, 2|]\n*smallest", "E1013");
}

#[test]
fn the_ref_suffix_is_spelled_r_and_the_old_names_are_gone() {
    for name in [
        "map_ref",
        "fold_ref",
        "reverse",
        "mapi_ref",
        "fold_back_ref",
    ] {
        rejects(&format!("let f = List.{name}\n0"), "E1002");
    }
    for name in BORROWING_FORMS {
        assert!(defined(name), "{name}");
    }
    // `iter` is F#'s `iter` now: it runs an action. The sequence of references it used to return is `to_seqr`.
    accepts("List.iter (\\x -> assert (x > 0)) [|1, 2, 3|]");
    accepts("let xs = [|1, 2, 3|]\nfor x in List.to_seqr xs do ()");
}

#[test]
fn fsharp_idioms_report_their_type_errors() {
    rejects("List.map (\\x -> x) [|1|] [|2|]", "E1006");
    rejects("List.init 3 (\\i -> i) 4", "E1006");
    rejects("List.zip [|1|] [|\"a\"|] == 3", "E1003");
    // An index is an i64, like every size.
    rejects("List.take 1i32 [|1, 2|]", "E1003");
}

/// The IR of `source` as a library, for the native target or for wasm.
fn library_ir(source: &str, wasm: bool) -> String {
    let module = analyze_modules(&[("Main.tz", source)])
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
}

#[test]
fn only_the_unsuffixed_random_functions_reach_the_operating_system() {
    // The `_by` and `_with` forms take their randomness from the caller, so they need no runtime on any target.
    let given = "export def shuffled :: i64\nfn shuffled =\n    let values = List.random_shuffle_by (\\() -> 0.5) [|1, 2, 3|]\n    values[0]\n\nexport def sampled :: i64\nfn sampled =\n    match List.random_sample_with (Random.pcg 1i64u 2i64u) 2 [|1, 2, 3|] with\n    | (values, _) -> values[0]\n";
    // Each of the four that seed a generator from the operating system calls the OS random primitive.
    let seeded = [
        "let! picked = List.random_shuffle [|1, 2, 3|]\n    do! IO.write_line (Result.is_ok (&picked))",
        "let! picked = List.random_choice [|1, 2, 3|]\n    do! IO.write_line (Result.is_ok (&picked))",
        "let! picked = List.random_choices 2 [|1, 2, 3|]\n    do! IO.write_line (Result.is_ok (&picked))",
        "let! picked = List.random_sample 2 [|1, 2, 3|]\n    do! IO.write_line (Result.is_ok (&picked))",
    ];
    for wasm in [false, true] {
        let ir = library_ir(given, wasm);
        assert!(!ir.contains("tsuzuri_os_"), "{ir}");
        for body in seeded {
            let source = format!("def main :: unit -> i32 = \\() ->\n    {body}\n    0\n");
            let ir = library_ir(&source, wasm);
            assert!(ir.contains("@tsuzuri_os_random"), "{source}\n{ir}");
        }
    }
}
