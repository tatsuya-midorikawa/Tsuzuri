//! The collection that a function works on is its LAST argument in `Array`, `List` and `Vec`, so that a pipeline
//! (`xs |> Array.f a`) and a composition (`Array.f a >> Array.g b`) pass it on. The rule is checked on the signatures
//! of the std modules, on the functions that are compiler builtins, and on the pipeline and composition forms.

use tsuzuri::analyze;

const STD_ARRAY: &str = include_str!("../std/Array.tz");
const STD_LIST: &str = include_str!("../std/List.tz");
const STD_VEC: &str = include_str!("../std/Vec.tz");

/// The parameter types of the signature in `def name :: [Constraint =>] t1 -> t2 -> result`: every type before the
/// last arrow that is not inside a bracket (a function type in a parameter keeps its own arrows).
fn parameters(signature: &str) -> Vec<String> {
    let ty = signature.split_once("=>").map_or(signature, |(_, ty)| ty);
    let characters: Vec<char> = ty.chars().collect();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '-' && characters.get(index + 1) == Some(&'>') {
            if depth == 0 {
                parts.push(current.trim().to_owned());
                current.clear();
            } else {
                current.push_str("->");
            }
            index += 2;
            continue;
        }
        match character {
            '(' | '[' | '<' => depth += 1,
            ')' | ']' | '>' => depth -= 1,
            _ => {}
        }
        current.push(character);
        index += 1;
    }
    parts.push(current.trim().to_owned());
    parts.pop();
    parts
}

/// The public functions of a module with their parameter types.
fn functions(source: &str) -> Vec<(String, Vec<String>)> {
    source
        .lines()
        .filter_map(|line| line.strip_prefix("def "))
        .filter_map(|line| line.split_once(" :: "))
        .map(|(name, signature)| (name.to_owned(), parameters(signature)))
        .collect()
}

fn strip_reference(ty: &str) -> &str {
    let ty = ty.strip_prefix("ref ").unwrap_or(ty);
    ty.strip_prefix("mut ").unwrap_or(ty)
}

/// Every function that takes a collection of the module's kind takes one as its last parameter.
fn assert_collection_last(module: &str, source: &str, is_collection: fn(&str) -> bool) {
    let mut checked = 0;
    let mut misplaced = Vec::new();
    for (name, parameters) in functions(source) {
        let collections = parameters
            .iter()
            .filter(|ty| is_collection(strip_reference(ty)))
            .count();
        if collections == 0 {
            continue;
        }
        checked += 1;
        if !parameters
            .last()
            .is_some_and(|ty| is_collection(strip_reference(ty)))
        {
            misplaced.push(format!("{module}.{name} :: {}", parameters.join(" -> ")));
        }
    }
    assert!(checked > 40, "{module}: only {checked} functions were read");
    assert!(
        misplaced.is_empty(),
        "the collection must be the last argument:\n{}",
        misplaced.join("\n")
    );
}

#[test]
fn every_array_function_takes_the_array_last() {
    // `[T]` and the exclusive slice `[T..]`, not a list `[|T|]`.
    assert_collection_last("Array", STD_ARRAY, |ty| {
        ty.starts_with('[') && !ty.starts_with("[|")
    });
}

#[test]
fn every_list_function_takes_the_list_last() {
    assert_collection_last("List", STD_LIST, |ty| ty.starts_with("[|"));
}

#[test]
fn every_vec_function_takes_the_vec_last() {
    assert_collection_last("Vec", STD_VEC, |ty| ty.starts_with("Vec<"));
}

#[test]
fn the_signature_reader_splits_at_the_arrows_outside_brackets() {
    assert_eq!(
        parameters("Copy<'a> => ('a -> 'state -> 'state) -> 'state -> ref ['a] -> 'state"),
        ["('a -> 'state -> 'state)", "'state", "ref ['a]"]
    );
    assert_eq!(
        parameters("i64 -> ref mut ['a..] -> (ref mut ['a..] * ref mut ['a..])"),
        ["i64", "ref mut ['a..]"]
    );
    assert_eq!(parameters("['a]"), Vec::<String>::new());
}

fn accepts(source: &str) {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
}

fn rejects(source: &str, codes: &[&str]) {
    let error = analyze(source).expect_err(source);
    assert!(
        codes.contains(&error.code),
        "{source}\n{}: {}",
        error.code,
        error.message
    );
}

/// The functions whose array came first, called in that old order. Each is a type error now, whichever argument
/// clashes first.
#[test]
fn the_old_order_of_the_functions_that_moved_the_array_is_rejected() {
    let calls = [
        "Array.get (ref xs) 0",
        "Array.at (ref xs) 0",
        "Array.sub (ref xs) 0 1",
        "Array.set (ref mut xs) 0 1",
        "Array.fill (ref mut xs) 0 1 7",
        "Array.blit (ref xs) 0 (ref mut ys) 0 1",
        "Array.fold_back (\\x acc -> acc + x) (ref xs) 0",
        "Array.foldr_back (\\x acc -> acc + deref x) (ref xs) 0",
        "Array.fold_back2 (\\x y acc -> acc + x + y) (ref xs) (ref xs) 0",
        "Array.scan_back (\\x acc -> acc + x) (ref xs) 0",
        "Array.map_fold_back (\\x acc -> (x, acc)) (ref xs) 0",
        "Array.index_of (ref xs) (ref 1)",
        "Array.binary_search (ref xs) (ref 1)",
        "Array.update xs 0 (\\x -> x + 1)",
        "Array.swap xs 0 1",
        "Array.write (ref mut xs) 0 1",
        "Array.swap_in (ref mut xs) 0 1",
        "Array.split_at_mut (ref mut xs) 1",
        "List.fold_back (\\x acc -> acc + x) (ref zs) 0",
        "List.foldr_back (\\x acc -> acc + deref x) (ref zs) 0",
        "List.fold_back2 (\\x y acc -> acc + x + y) (ref zs) (ref zs) 0",
        "List.scan_back (\\x acc -> acc + x) (ref zs) 0",
        "List.map_fold_back (\\x acc -> (x, acc)) (ref zs) 0",
    ];
    for call in calls {
        let source = format!(
            "let mut xs = [1, 2, 3]\nlet mut ys = [0, 0, 0]\nlet zs = [|1, 2, 3|]\nlet result = {call}\n0"
        );
        let error = analyze(&source).expect_err(call);
        assert!(
            matches!(error.code, "E1003" | "E1005" | "E1014" | "E1015"),
            "{call}\n{}: {}",
            error.code,
            error.message
        );
    }
}

#[test]
fn the_new_order_type_checks() {
    let calls = [
        "Array.get 0 (ref xs)",
        "Array.at 0 (ref xs)",
        "Array.sub 0 1 (ref xs)",
        "Array.set 0 1 xs",
        "Array.fill 0 1 7 xs",
        "Array.blit 0 0 1 (ref xs) ys",
        "Array.fold_back (\\x acc -> acc + x) 0 (ref xs)",
        "Array.foldr_back (\\x acc -> acc + deref x) 0 (ref xs)",
        "Array.fold_back2 (\\x y acc -> acc + x + y) 0 (ref xs) (ref xs)",
        "Array.scan_back (\\x acc -> acc + x) 0 (ref xs)",
        "Array.map_fold_back (\\x acc -> (x, acc)) 0 (ref xs)",
        "Array.index_of (ref 1) (ref xs)",
        "Array.binary_search (ref 1) (ref xs)",
        "Array.write 0 1 xs",
        "Array.swap_in 0 1 xs",
        "Array.split_at_mut 1 xs",
        "List.fold_back (\\x acc -> acc + x) 0 (ref zs)",
        "List.foldr_back (\\x acc -> acc + deref x) 0 (ref zs)",
        "List.fold_back2 (\\x y acc -> acc + x + y) 0 (ref zs) (ref zs)",
        "List.scan_back (\\x acc -> acc + x) 0 (ref zs)",
        "List.map_fold_back (\\x acc -> (x, acc)) 0 (ref zs)",
    ];
    for call in calls {
        accepts(&format!(
            "let mut xs = [1, 2, 3]\nlet mut ys = [0, 0, 0]\nlet zs = [|1, 2, 3|]\nlet result = {call}\n0"
        ));
    }
    accepts("let result = Array.update 0 (\\x -> x + 1) [1, 2]\nresult[0]");
    accepts("let result = Array.swap 0 1 [1, 2]\nresult[0]");
}

#[test]
fn pipelines_pass_the_collection_on() {
    for source in [
        "let xs = [3, 1, 2]\nlet top = xs |> Array.sort |> Array.rev |> Array.take 2\ntop[0]",
        "let xs = [1, 2, 3]\nlet middle = xs |> Array.sub 1 1\nmiddle[0]",
        "let xs = [1, 2, 3]\nxs |> Array.fold_back (\\x acc -> acc * 10 + x) 0",
        "let xs = [1, 2, 3]\nlet states = xs |> Array.scan_back (\\x acc -> acc + x) 0\nstates[0]",
        "let xs = [1, 2, 3]\nxs |> Array.get 1",
        "let result = [1, 2] |> Array.update 0 (\\x -> x + 1) |> Array.swap 0 1\nresult[0]",
        "let zs = [|1, 2, 3|]\nzs |> List.fold_back (\\x acc -> acc * 10 + x) 0",
        "let zs = [|3, 1, 2|]\nlet top = zs |> List.sort |> List.rev |> List.take 2\ntop.length",
        "let vs = Vec.empty() |> Vec.add 3 |> Vec.add 1 |> Vec.sort\nvs[0]",
    ] {
        accepts(source);
    }
}

/// The slice is lent last, so an argument before it may still read it; passed on by a pipeline, the slice is lent
/// first and the arguments after it cannot.
#[test]
fn an_argument_may_read_the_slice_that_is_lent_last() {
    accepts("let mut values = [1, 2, 3]\nArray.write 0 (values[1] * 10) values\nvalues[0]");
    accepts(
        "let mut values = [1, 2, 3]\nArray.write (values.length - 1) (values[1] * 10) (ref mut values)\nvalues[2]",
    );
    accepts("let mut values = [1, 2, 3]\nArray.swap_in 0 (values.length - 1) values\nvalues[0]");
    rejects(
        "let mut values = [1, 2, 3]\nvalues |> Array.write 0 (values[1] * 10)\nvalues[0]",
        &["E1014"],
    );
}

/// A mutable array passes on as an exclusive slice, also into a partial application that has a lambda.
#[test]
fn a_mutable_array_pipes_into_the_functions_that_write_it() {
    for source in [
        "let mut xs = [1, 2, 3]\nref mut xs |> Array.set 1 9\nxs[1]",
        "let mut xs = [1, 2, 3]\nref mut xs |> Array.write 1 9\nxs[1]",
        "let mut xs = [1, 2, 3]\nref mut xs |> Array.swap_in 0 2\nxs[0]",
        "let mut xs = [1, 2, 3]\nref mut xs |> Array.fill 1 2 7\nxs[1]",
        "let mut xs = [3, 1, 2]\nref mut xs |> Array.sort_in_place_with (\\a b -> a - b)\nxs[0]",
        "let mut xs = [3, 1, 2]\nref mut xs |> Array.sort_in_place_by (\\a -> -a)\nxs[0]",
        "let mut xs = [3, 1, 2]\nxs |> Array.set 0 5\nxs[0]",
    ] {
        accepts(source);
    }
    // The element types must agree.
    rejects(
        "let mut xs = [1, 2, 3]\nref mut xs |> Array.set 1 \"nine\"\n0",
        &["E1003"],
    );
}

/// A composition passes the collection that one function returns to the next, which borrows it. The element type is
/// not known where `Array.sort >> Array.rev` is written, so the borrow is settled once the function is checked.
#[test]
fn partial_applications_compose() {
    for source in [
        "let both = Array.sort >> Array.rev\nlet top = both [3, 1, 2]\ntop[0]",
        "let top = [3, 1, 2] |> (Array.sort >> Array.rev)\ntop[0]",
        "let both = Array.sort >> Array.rev >> Array.truncate 2\nlet top = [3, 1, 2] |> both\ntop[0]",
        "let sorted = Array.sort >> Array.truncate 2\nlet top = [3, 1, 2] |> sorted\ntop[0]",
        "let total = Array.map (\\x -> x * 2) >> Array.sum\n[1, 2, 3] |> total",
        "let both = Array.update_at 0 9 >> Array.swap 0 1\nlet changed = both [1, 2, 3]\nchanged[0]",
        "let both = List.sort >> List.rev\nlet top = both [|3, 1, 2|]\ntop.length",
        "let both = Vec.sort >> Vec.reverse\nlet top = both (Vec.of_array [3, 1, 2])\ntop[0]",
    ] {
        accepts(source);
    }
    // The composition still cannot return a reference into the array that the first function made.
    for source in [
        "let first = Array.sort >> Array.minr\nlet xs = [3, 1, 2]\nlet smallest = first (ref xs)\n0",
        "let first = Array.sort >> Array.findr (\\x -> deref x > 1)\nlet xs = [3, 1, 2]\nlet found = first (ref xs)\n0",
        "let seed = 0\nlet last = Array.sort >> Array.foldr (\\acc x -> x) (ref seed)\nlet xs = [3, 1, 2]\nlet found = last (ref xs)\n0",
        "let first = List.sort >> List.minr\nlet xs = [|3, 1, 2|]\nlet smallest = first (ref xs)\n0",
    ] {
        rejects(source, &["E1013"]);
    }
}
