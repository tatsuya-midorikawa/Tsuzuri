use tsuzuri::check::CheckedModule;
use tsuzuri::driver::{Target, TestOptions, run_tests};
use tsuzuri::{analyze, analyze_modules, llvm};

const BEHAVIOR: &str = include_str!("fixtures/vec_dotnet/Main.tz");
const TRAPS: &str = include_str!("fixtures/vec_dotnet_traps/Main.tz");
const STD_VEC: &str = include_str!("../std/Vec.tz");

/// The functions of `Vec` that are compiler builtins instead of std source (`Vec.push` and the rest of the module that
/// existed before the members of `List<T>`).
const BUILTINS: [&str; 17] = [
    "at",
    "capacity",
    "clear",
    "clone",
    "empty",
    "get",
    "is_empty",
    "length",
    "of_array",
    "pop",
    "push",
    "reserve",
    "set",
    "swap",
    "to_array",
    "truncate",
    "with_capacity",
];

/// Every public member of `List<T>` (.NET 10), with the functions of `Vec` that stand for it. An overload that is a
/// separate function has its own row; the second name of a row is the form that borrows (`r`) or reads another source.
const MEMBERS: [(&str, &[&str]); 59] = [
    ("List<T>()", &["empty"]),
    ("List<T>(Int32)", &["with_capacity"]),
    (
        "List<T>(IEnumerable<T>)",
        &["of_array", "of_list", "of_seq", "clone"],
    ),
    ("Capacity (get)", &["capacity"]),
    ("Capacity (set)", &["set_capacity"]),
    ("Count", &["length"]),
    ("Item[Int32] (get)", &["at", "get"]),
    ("Item[Int32] (set)", &["set"]),
    ("Add", &["add"]),
    ("AddRange", &["add_range", "add_range_seq", "append"]),
    ("BinarySearch(T)", &["binary_search", "binary_searchr"]),
    (
        "BinarySearch(T, IComparer<T>)",
        &["binary_search_with", "binary_search_withr"],
    ),
    (
        "BinarySearch(Int32, Int32, T, IComparer<T>)",
        &["binary_search_range"],
    ),
    ("Clear", &["clear"]),
    ("Contains", &["contains", "containsr"]),
    ("ConvertAll", &["convert_all", "convert_allr"]),
    ("CopyTo(T[])", &["copy_to"]),
    ("CopyTo(T[], Int32)", &["copy_to_at"]),
    ("CopyTo(Int32, T[], Int32, Int32)", &["copy_to_range"]),
    ("EnsureCapacity", &["ensure_capacity"]),
    ("Exists", &["exists", "existsr"]),
    ("Find", &["find", "findr"]),
    ("FindAll", &["find_all"]),
    ("FindIndex(Predicate<T>)", &["find_index", "find_indexr"]),
    ("FindIndex(Int32, Predicate<T>)", &["find_index_from"]),
    (
        "FindIndex(Int32, Int32, Predicate<T>)",
        &["find_index_range"],
    ),
    ("FindLast", &["find_last", "find_lastr"]),
    (
        "FindLastIndex(Predicate<T>)",
        &["find_last_index", "find_last_indexr"],
    ),
    (
        "FindLastIndex(Int32, Predicate<T>)",
        &["find_last_index_from"],
    ),
    (
        "FindLastIndex(Int32, Int32, Predicate<T>)",
        &["find_last_index_range"],
    ),
    ("ForEach", &["for_each", "for_eachr"]),
    ("GetEnumerator", &["iter"]),
    ("GetRange", &["get_range"]),
    ("IndexOf(T)", &["index_of", "index_ofr"]),
    ("IndexOf(T, Int32)", &["index_of_from"]),
    ("IndexOf(T, Int32, Int32)", &["index_of_range"]),
    ("Insert", &["insert"]),
    ("InsertRange", &["insert_range", "insert_range_seq"]),
    ("LastIndexOf(T)", &["last_index_of", "last_index_ofr"]),
    ("LastIndexOf(T, Int32)", &["last_index_of_from"]),
    ("LastIndexOf(T, Int32, Int32)", &["last_index_of_range"]),
    ("Remove", &["remove", "remover"]),
    ("RemoveAll", &["remove_all", "remove_allr"]),
    ("RemoveAt", &["remove_at"]),
    ("RemoveRange", &["remove_range"]),
    ("Reverse()", &["reverse"]),
    ("Reverse(Int32, Int32)", &["reverse_range"]),
    ("Slice", &["slice"]),
    ("Sort()", &["sort"]),
    ("Sort(Comparison<T>)", &["sort_with", "sort_withr"]),
    ("Sort(IComparer<T>)", &["sort_with", "sort_withr"]),
    (
        "Sort(Int32, Int32, IComparer<T>)",
        &["sort_range", "sort_range_with", "sort_range_withr"],
    ),
    ("ToArray", &["to_array"]),
    ("TrimExcess", &["trim_excess"]),
    ("TrueForAll", &["true_for_all", "true_for_allr"]),
    ("(not in .NET) pop", &["pop"]),
    ("(not in .NET) push", &["push"]),
    ("(not in .NET) reserve", &["reserve"]),
    (
        "(not in .NET) swap, truncate, is_empty",
        &["swap", "truncate", "is_empty"],
    ),
];

/// The member of `List<T>` that `Vec` has no function for: a shared borrow `ref values` is the read-only view.
const NOT_PROVIDED: [&str; 1] = ["AsReadOnly"];

/// The `r` forms, which borrow each element or the item to look for.
const BORROWING_FORMS: [&str; 16] = [
    "binary_search_withr",
    "binary_searchr",
    "containsr",
    "convert_allr",
    "existsr",
    "find_indexr",
    "find_last_indexr",
    "find_lastr",
    "findr",
    "for_eachr",
    "index_ofr",
    "last_index_ofr",
    "remove_allr",
    "remover",
    "sort_withr",
    "true_for_allr",
];

fn count_tests(source: &str) -> usize {
    source
        .lines()
        .filter(|line| line.starts_with("test \""))
        .count()
}

/// One test of a fixture: its index, its name, why it failed (a trap, a timeout, ...) or `None` when it passed, and the
/// end of its standard error.
struct Outcome {
    index: usize,
    name: String,
    failure: Option<String>,
    output: String,
}

impl Outcome {
    fn timed_out(&self) -> bool {
        self.failure
            .as_deref()
            .is_some_and(|reason| reason.starts_with("timed out"))
    }

    /// The test ended in a trap: one that passed did not trap, and one that timed out hung.
    fn trapped(&self) -> bool {
        self.failure.is_some() && !self.timed_out()
    }
}

/// How often a test runs before a timeout counts. A loaded CI machine can hold a process that traps in a millisecond for
/// longer than the runner's 30 seconds, while a test that really hangs times out on every attempt.
const ATTEMPTS: usize = 3;

fn execute(module: &CheckedModule, options: &TestOptions) -> Vec<Outcome> {
    run_tests(module, options)
        .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message))
        .results
        .into_iter()
        .map(|result| Outcome {
            index: result.case.index,
            name: result.case.name,
            failure: result.failure,
            output: result.output,
        })
        .collect()
}

fn run(source: &str, target: Target, optimization: u8) -> Vec<Outcome> {
    run_matching(source, target, optimization, None)
}

/// Runs the tests of `source` whose names contain `filter` (all of them without one). A test that timed out runs again,
/// on its own, up to `ATTEMPTS` times in all.
fn run_matching(
    source: &str,
    target: Target,
    optimization: u8,
    filter: Option<&str>,
) -> Vec<Outcome> {
    let module =
        analyze(source).unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    let mut options = TestOptions {
        target,
        optimization,
        filter: filter.map(str::to_owned),
        ..Default::default()
    };
    let mut outcomes = execute(&module, &options);
    for _ in 1..ATTEMPTS {
        options.indices = outcomes
            .iter()
            .filter(|outcome| outcome.timed_out())
            .map(|outcome| outcome.index)
            .collect();
        if options.indices.is_empty() {
            break;
        }
        for again in execute(&module, &options) {
            let slot = outcomes
                .iter_mut()
                .find(|outcome| outcome.index == again.index)
                .expect("a test of the first run");
            *slot = again;
        }
    }
    outcomes
}

/// The names of the functions that std/Vec.tz makes public.
fn public_functions() -> Vec<&'static str> {
    STD_VEC
        .lines()
        .filter_map(|line| line.strip_prefix("def "))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect()
}

fn defined(name: &str) -> bool {
    BUILTINS.contains(&name) || public_functions().contains(&name)
}

#[test]
fn every_member_of_list_t_has_a_function_of_vec() {
    let mut missing = Vec::new();
    for (member, names) in MEMBERS {
        for name in names {
            if !defined(name) {
                missing.push(format!("{member}: {name}"));
            }
        }
    }
    assert!(missing.is_empty(), "missing from Vec: {missing:?}");
    // The other way round: no public function is left out of the table, so the documentation cannot lose one.
    let named: Vec<&str> = MEMBERS
        .iter()
        .flat_map(|(_, names)| names.iter().copied())
        .collect();
    let unlisted: Vec<_> = public_functions()
        .into_iter()
        .filter(|name| !named.contains(name))
        .collect();
    assert!(unlisted.is_empty(), "not in the table: {unlisted:?}");
    assert_eq!(NOT_PROVIDED, ["AsReadOnly"]);
}

#[test]
fn the_members_agree_with_dotnet_on_every_target() {
    let expected = count_tests(BEHAVIOR);
    assert!(expected >= 45, "{expected} tests");
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
fn contract_violations_trap_where_dotnet_throws() {
    let expected = count_tests(TRAPS);
    assert!(expected >= 75, "{expected} tests");
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
        // A test that hangs is killed by the runner and reported as a failure too: that is not a trap. A timeout that
        // survived every attempt is a hang.
        let hung: Vec<_> = results
            .iter()
            .filter(|outcome| outcome.timed_out())
            .map(|outcome| &outcome.name)
            .collect();
        assert!(
            hung.is_empty(),
            "{target:?} -O{optimization} timed out instead of trapping: {hung:?}"
        );
    }
}

/// The tests of the traps fixture that print what their callbacks see, with what .NET's `List<T>` shows.
const ORDERS: [(&str, &[i64]); 11] = [
    ("remove_all calls", &[5, 1, 4, 2, 3]),
    ("remove_allr calls", &[5, 1, 4, 2, 3]),
    ("exists stops", &[1, 2, 3]),
    ("true_for_all stops", &[1, 2, 3]),
    ("find_last looks", &[4, 3, 2]),
    ("find_index_from starts", &[3, 4]),
    ("find_last_index_range looks", &[4, 3]),
    ("for_each visits", &[1, 2, 3]),
    ("convert_all converts", &[1, 2, 3]),
    ("find_all tests", &[1, 2, 3]),
    ("binary_search_with probes", &[30, 40]),
];

#[test]
fn callbacks_run_in_the_order_of_dotnet_and_stop_where_it_stops() {
    for optimization in [0, 3] {
        let results = run_matching(TRAPS, Target::Native, optimization, Some("in order"));
        assert_eq!(results.len(), ORDERS.len());
        for (name, sequence) in ORDERS {
            let outcome = results
                .iter()
                .find(|outcome| outcome.name.starts_with(name))
                .unwrap_or_else(|| panic!("no test named {name}"));
            assert!(
                outcome.trapped(),
                "-O{optimization} {} did not fail on purpose: {:?}",
                outcome.name,
                outcome.failure
            );
            let seen: Vec<i64> = outcome
                .output
                .lines()
                .filter_map(|line| line.trim().strip_prefix("seen "))
                .map(|number| number.parse().expect("a number"))
                .collect();
            assert_eq!(
                seen, sequence,
                "-O{optimization} {}: {:?}",
                outcome.name, outcome.output
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
fn value_forms_need_copy_elements_and_r_forms_borrow() {
    // The value forms copy an element to give it to the callback, so a Vec of strings is rejected.
    for call in [
        "Vec.exists (\\w -> w.length > 0) words",
        "Vec.true_for_all (\\w -> w.length > 0) words",
        "Vec.find (\\w -> w.length > 0) words",
        "Vec.find_all (\\w -> w.length > 0) words",
        "Vec.find_index (\\w -> w.length > 0) words",
        "Vec.find_last_index (\\w -> w.length > 0) words",
        "Vec.for_each (\\w -> assert (w.length > 0)) words",
        "Vec.convert_all (\\w -> w.length) words",
        "Vec.sort_with (\\a b -> a.length - b.length) words",
        "Vec.get_range 0 1 words",
        "Vec.find_index_range 0 1 (\\w -> w.length > 0) words",
    ] {
        rejects(
            &format!("let words = Vec.of_array [\"a\"]\n{call}"),
            "E1005",
        );
    }
    // The r forms borrow each element, so they accept any element type.
    for call in [
        "Vec.existsr (\\w -> w.length > 0) words",
        "Vec.true_for_allr (\\w -> w.length > 0) words",
        "Vec.find_indexr (\\w -> w.length > 0) words",
        "Vec.find_last_indexr (\\w -> w.length > 0) words",
        "Vec.for_eachr (\\w -> assert (w.length > 0)) words",
        "Vec.convert_allr (\\w -> w.length) words",
        "match Vec.remove_allr (\\w -> w.length > 5) words with | (rest, removed) -> removed",
        "(Vec.sort_withr (\\a b -> a.length - b.length) words).length",
        "match Vec.findr (\\w -> w.length == 1) words with | Maybe.Some word -> word.length | Maybe.None -> 0",
        "match Vec.find_lastr (\\w -> w.length == 1) words with | Maybe.Some word -> word.length | Maybe.None -> 0",
        "Vec.containsr (ref target) words",
        "Vec.index_ofr (ref target) words",
        "Vec.last_index_ofr (ref target) words",
        "match Vec.remover (ref target) words with | (rest, found) -> found",
        "Vec.binary_searchr (ref target) words",
        "Vec.binary_search_withr (ref target) (\\a b -> a.length - b.length) words",
    ] {
        accepts(&format!(
            "let words = Vec.of_array [\"a\"]\nlet target = \"a\"\n{call}"
        ));
    }
    // What only compares or moves elements works for any element type, and the item is consumed.
    for call in [
        "Vec.contains \"a\" words",
        "Vec.index_of \"a\" words",
        "Vec.last_index_of \"a\" words",
        "Vec.index_of_from \"a\" 0 words",
        "Vec.index_of_range \"a\" 0 1 words",
        "Vec.last_index_of_from \"a\" 0 words",
        "Vec.last_index_of_range \"a\" 0 1 words",
        "Vec.binary_search \"a\" words",
        "(Vec.add \"b\" words).length",
        "(Vec.insert 0 \"b\" words).length",
        "(Vec.remove_at 0 words).length",
        "(Vec.remove_range 0 1 words).length",
        "(Vec.reverse words).length",
        "(Vec.reverse_range 0 1 words).length",
        "(Vec.sort words).length",
        "(Vec.sort_range 0 1 words).length",
        "(Vec.append (Vec.empty()) words).length",
        "(Vec.set_capacity 5 words).length",
        "(Vec.ensure_capacity 5 words).length",
        "(Vec.trim_excess words).length",
        "match Vec.remove \"a\" words with | (rest, found) -> found",
    ] {
        accepts(&format!("let words = Vec.of_array [\"a\"]\n{call}"));
    }
}

#[test]
fn updates_consume_the_vec_and_loans_block_them() {
    // The changed Vec is the result, so the old binding is gone.
    for update in [
        "Vec.add 3 values",
        "Vec.insert 0 3 values",
        "Vec.remove_at 0 values",
        "Vec.remove_range 0 1 values",
        "Vec.reverse values",
        "Vec.sort values",
        "Vec.set_capacity 9 values",
        "Vec.trim_excess values",
    ] {
        rejects(
            &format!("let values = Vec.of_array [1, 2]\nlet changed = {update}\nvalues.length"),
            "E1012",
        );
    }
    // A borrow that is alive keeps the Vec from being changed.
    for update in [
        "Vec.add 3 values",
        "Vec.insert 0 3 values",
        "Vec.remove_at 0 values",
        "Vec.sort values",
        "Vec.reverse values",
    ] {
        rejects(
            &format!(
                "let values = Vec.of_array [1, 2]\nlet borrowed = Vec.at 0 (ref values)\nlet changed = {update}\nderef borrowed"
            ),
            "E1014",
        );
    }
    // The searches only read: the Vec is still there.
    accepts(
        "let values = Vec.of_array [1, 2]\nlet found = Vec.find_index (\\x -> x > 1) values\nvalues.length + found",
    );
    accepts(
        "let values = Vec.of_array [1, 2]\nlet found = Vec.index_of 2 values\nvalues.length + found",
    );
    // The array that `copy_to` writes into is exclusive, so it must be `let mut`.
    rejects(
        "let values = Vec.of_array [1]\nlet target = [0]\nVec.copy_to target values\n0",
        "E1014",
    );
    accepts(
        "let values = Vec.of_array [1]\nlet mut target = [0]\nVec.copy_to target values\ntarget[0]",
    );
    // The primitives of std/Vec.tz (a mutable view of the elements, the capacity setter, and the two bulk moves) are
    // private to that module: a user module reaches the same work through the public API.
    for call in [
        "Vec.__view (ref mut values)",
        "Vec.__set_capacity 4 values",
        "Vec.__insert 0 3 values",
        "Vec.__splice 0 0 [] values",
    ] {
        rejects(
            &format!("let values = Vec.of_array [2, 1]\nlet result = {call}\n0"),
            "E1022",
        );
    }
    accepts(
        "let mut values = Vec.of_array [2, 1]\nvalues = Vec.sort values\nvalues = Vec.insert 1 9 values\nvalues = Vec.remove_range 0 1 values\nvalues = Vec.set_capacity 8 values\nvalues[0]",
    );
}

#[test]
fn the_vec_is_the_last_argument_so_pipelines_and_compositions_work() {
    // The receiver-first order of the first version no longer type-checks, whichever way the types clash.
    for call in [
        "Vec.push values 3",
        "Vec.add values 3",
        "Vec.set values 0 3",
        "Vec.swap values 0 1",
        "Vec.reserve values 4",
        "Vec.truncate values 1",
        "Vec.get (ref values) 0",
        "Vec.at (ref values) 0",
        "Vec.insert values 0 3",
        "Vec.remove_at values 0",
        "Vec.remove_range values 0 1",
        "Vec.index_of (ref values) 1",
        "Vec.find_index (ref values) (\\x -> x > 1)",
        "Vec.sort_with values (\\a b -> a - b)",
        "Vec.get_range (ref values) 0 1",
    ] {
        let error = analyze(&format!("let values = Vec.of_array [1, 2]\n{call}")).expect_err(call);
        assert!(
            matches!(error.code, "E1003" | "E1015"),
            "{call}\n{}: {}",
            error.code,
            error.message
        );
    }
    // A pipeline passes the Vec on, and a partial application is a function of the Vec.
    accepts(
        "let values = Vec.empty() |> Vec.add 3 |> Vec.add 1 |> Vec.sort |> Vec.insert 1 10\nvalues[1]",
    );
    accepts(
        "let tidy = Vec.sort >> Vec.reverse >> Vec.truncate 2\nlet top = Vec.of_array [5, 1, 9] |> tidy\ntop[0]",
    );
    accepts(
        "let words = Vec.of_array [\"b\", \"a\"] |> Vec.sort_withr (\\x y -> x.length - y.length) |> Vec.add \"c\"\nwords.length",
    );
    // An argument before the Vec may borrow it, because the Vec itself is moved last.
    accepts(
        "let mut values = Vec.of_array [1, 2]\nvalues = Vec.insert (Vec.length (ref values)) 3 values\nvalues[2]",
    );
    // The call consumes the Vec, so the old binding cannot be used after it.
    rejects(
        "let values = Vec.of_array [1, 2]\nlet more = Vec.insert (Vec.length (ref values)) 3 values\nvalues.length",
        "E1012",
    );
}

#[test]
fn the_r_suffix_names_the_borrowing_forms() {
    for name in BORROWING_FORMS {
        assert!(defined(name), "{name}");
    }
    // The members that existed before keep their names and their types, except that the Vec is now the last argument.
    accepts(
        "let mut values: Vec<i64> = Vec.empty()\nvalues = Vec.push 1 values\nvalues = Vec.set 0 2 values\nvalues = Vec.swap 0 0 values\nlet size = Vec.length (ref values)\nlet space = Vec.capacity (ref values)\nmatch Vec.pop values with | (rest, last) -> Vec.is_empty (ref rest)",
    );
    accepts(
        "let values = Vec.of_array [1, 2]\nlet copy = Vec.clone (ref values)\nlet items = Vec.to_array copy\nfor word in Vec.iter (ref values) do ()\nitems[0]",
    );
}

/// The IR of `source` as a library, for the native target or for wasm.
fn library_ir(source: &str, wasm: bool) -> String {
    let module = analyze_modules(&[("Main.tz", source)])
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
}

#[test]
fn shifts_lower_to_memmove_and_copies_to_memcpy() {
    let shifts = "export def shifted :: i64\nfn shifted =\n    let values = Vec.of_array [1, 2, 4]\n    let values = Vec.insert 2 3 values\n    let values = Vec.remove_at 0 values\n    values[0]\n";
    let copies = "export def copied :: i64\nfn copied =\n    let values = Vec.of_array [1, 2, 4]\n    let values = Vec.add_range (ref [5, 6]) values\n    values[0]\n";
    let strings = "export def owned :: i64\nfn owned =\n    let values = Vec.of_array [\"a\" + \"b\", \"c\" + \"d\"]\n    let values = Vec.insert 1 (\"e\" + \"f\") values\n    let values = Vec.remove_range 0 2 values\n    (Vec.at 0 (ref values)).length\n";
    for wasm in [false, true] {
        let width = if wasm { "i32" } else { "i64" };
        for source in [shifts, copies, strings] {
            let ir = library_ir(source, wasm);
            // One declaration per width, whatever the number of shifts.
            assert_eq!(
                ir.matches("declare void @llvm.memmove.p0.p0.").count(),
                1,
                "{ir}"
            );
            assert!(
                ir.contains(&format!("declare void @llvm.memmove.p0.p0.{width}(")),
                "{ir}"
            );
            assert_eq!(ir, library_ir(source, wasm));
        }
        let ir = library_ir(copies, wasm);
        assert!(
            ir.contains(&format!("declare void @llvm.memcpy.p0.p0.{width}(")),
            "{ir}"
        );
    }
    // Reversing and sorting work on the buffer through a view: they move nothing else.
    let views = "export def viewed :: i64\nfn viewed =\n    let values = Vec.of_array [3, 1, 2]\n    let values = Vec.sort values\n    let values = Vec.reverse values\n    values[0]\n";
    for wasm in [false, true] {
        let ir = library_ir(views, wasm);
        assert!(!ir.contains("@llvm.memmove"), "{ir}");
    }
}

#[test]
fn wasm64_moves_with_64_bit_lengths_without_trap_information() {
    // The test runner of a wasm build has no trap instrumentation, so the builtins are emitted separately; they must
    // still know that the memory is 64-bit, or a transfer of 4 GiB or more would pass the wrong length. The copies of
    // the elements of a `List` read the same flag.
    let vec = "test \"moves\" =\n    let values = Vec.of_array [1, 2, 4]\n    let values = Vec.insert 2 3 values\n    let values = Vec.add_range (ref [5, 6]) values\n    let values = Vec.remove_at 0 values\n    Test.is_true (values[0] == 2)\n";
    let list = "test \"copies\" =\n    let numbers = new [|i64|](8, \\index -> index)\n    let array = List.to_array numbers\n    Test.is_true (array[7] == 7)\n";
    for (source, intrinsics) in [(vec, &["memmove", "memcpy"][..]), (list, &["memcpy"][..])] {
        let module = analyze_modules(&[("Main.tz", source)])
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
        for (memory64, width, other) in [(false, "i32", "i64"), (true, "i64", "i32")] {
            let ir = llvm::emit_test_runner_with(
                &module,
                &[0],
                llvm::TestRunnerOptions {
                    wasm: true,
                    memory64,
                    ..llvm::TestRunnerOptions::default()
                },
            )
            .unwrap();
            for intrinsic in intrinsics {
                assert!(
                    ir.contains(&format!("declare void @llvm.{intrinsic}.p0.p0.{width}(")),
                    "memory64 {memory64}, {intrinsic}: {ir}"
                );
                assert!(
                    !ir.contains(&format!("@llvm.{intrinsic}.p0.p0.{other}")),
                    "memory64 {memory64}, {intrinsic}: {ir}"
                );
            }
        }
    }
}
