use tsuzuri::formatter::{ast_fingerprint, format_source};
use tsuzuri::syntax::{Edition, SourceKind};
use tsuzuri::{
    analyze, analyze_modules, analyze_modules_with_edition, analyze_modules_with_semantics,
    analyze_with_edition, llvm, parser,
};

fn rejects(source: &str, code: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.into_data().message
}

fn emits(source: &str) -> [String; 2] {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!("{source}\n{}: {}", error.code, error.message);
    });
    [false, true].map(|wasm| {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap(),
            "{source}: IR is deterministic"
        );
        ir
    })
}

/// `Rc.<name>` in an Rc program and the `Arc` spelling of the same program.
fn both(source: &str) -> [String; 2] {
    [source.to_owned(), source.replace("Rc.", "Arc.")]
}

const ALL_OPERATIONS: &str = "let first = Rc.new \"shared\"\n\
let second = Rc.share (ref first)\n\
let weak = Rc.downgrade (ref first)\n\
let counts = Rc.strong_count (ref first) * 10 + Rc.weak_count (ref second)\n\
let same = if Rc.ptr_eq (ref first) (ref second) then 1 else 0\n\
let text = Rc.get (ref second)\n\
let length = text.length\n\
let alive = match Rc.upgrade (ref weak) with\n    | Maybe.Some strong -> Rc.strong_count (ref strong)\n    | Maybe.None -> 0\n\
Owned.drop second\n\
let unwrapped = match Rc.try_unwrap first with\n    | Result.Ok value -> value.length\n    | Result.Error kept -> Rc.strong_count (ref kept)\n\
counts + same + length + alive + unwrapped";

#[test]
fn shared_pointer_programs_type_check_and_emit() {
    for source in both(ALL_OPERATIONS) {
        emits(&source);
    }
    for source in [
        "let shared: std::Rc<i64> = Rc.new 5\nlet weak: std::Rc.Weak<i64> = std::Rc.downgrade (ref shared)\nlet back: Maybe<Rc<i64>> = Rc.upgrade (ref weak)\nRc.strong_count (ref shared)",
        "let shared: Arc<[i64]> = Arc.new [1, 2, 3]\nlet weak: Arc.Weak<[i64]> = Arc.downgrade (ref shared)\nArray.sum (Arc.get (ref shared)) + Arc.weak_count (ref shared)",
        "def wrap :: 'a -> Rc<'a> = \\value -> Rc.new value\nlet number = wrap 5\nlet text = wrap \"x\"\nRc.strong_count (ref number) + (Rc.get (ref text)).length",
        "record Pair { left: Rc<string>, right: Rc<string> }\nlet shared = Rc.new \"x\"\nlet pair = Pair { left: Rc.share (ref shared), right: shared }\nRc.strong_count (ref pair.left)",
        "let values = Vec.push (Rc.new 2) (Vec.push (Rc.new 1) (Vec.empty()))\nlet copied = Rc.share (ref values[0])\nRc.strong_count (ref copied)",
    ] {
        emits(source);
    }
    for name in ["Rc", "Arc"] {
        let error = analyze_modules(&[(name, "")]).unwrap_err();
        assert_eq!(error.code, "E1011", "{name}: {}", error.message);
    }
}

#[test]
fn shared_pointers_are_owned_and_never_copied() {
    for source in both("let shared = Rc.new 5\nlet moved = shared\nRc.strong_count (ref shared)") {
        let message = rejects(&source, "E1012");
        assert!(message.contains("'shared'"), "{message}");
    }
    for source in both("def dup :: Copy<'a> => 'a -> ('a * 'a) = \\x -> (x, x)\ndup (Rc.new 1)") {
        let message = rejects(&source, "E1005");
        assert!(message.contains("no instance for Copy"), "{message}");
    }
    for source in both("let values = Vec.push (Rc.new 1) (Vec.empty())\nVec.get 0 (ref values)") {
        rejects(&source, "E1005");
    }
    for source in both("let first = Rc.new 1\nlet second = Rc.new 1\nfirst == second") {
        let message = rejects(&source, "E1005");
        assert!(message.contains("no instance for Eq"), "{message}");
    }
    rejects(
        "let shared = Rc.new 5\nlet weak = Arc.downgrade (ref shared)\n0",
        "E1003",
    );
    rejects(
        "let shared = Rc.new 5\nlet weak = Rc.downgrade (ref shared)\nlet strong: Maybe<Arc<i32>> = Rc.upgrade (ref weak)\n0",
        "E1003",
    );
}

#[test]
fn rc_stays_on_its_tzroutine_and_arc_moves_when_its_value_is_send() {
    for source in [
        "let shared = Rc.new 5\ntzroutine {\n    let _value = deref (Rc.get (ref shared))\n}\n0",
        "let shared = Rc.new 5\nlet weak = Rc.downgrade (ref shared)\ntzroutine {\n    let _alive = match Rc.upgrade (ref weak) with | Maybe.Some _strong -> 1 | Maybe.None -> 0\n}\n0",
        "let shared = Arc.new (Rc.new 5)\ntzroutine {\n    let _count = Arc.strong_count (ref shared)\n}\n0",
        "let shared = Rc.new 5\nlet keep = Owned.function (\\() -> Rc.strong_count (ref shared))\nOwned.call (ref keep) ()",
    ] {
        let message = rejects(source, "E1013");
        assert!(
            message.contains("holds an Rc or Rc.Weak, whose counts are not atomic"),
            "{message}"
        );
    }
    for source in [
        "let shared = Rc.new 5\nlet read = \\() -> deref (Rc.get (ref shared))\nread ()",
        "def add :: Rc<i64> -> i64 -> i64\nfn add shared value = deref (Rc.get (ref shared)) + value\nlet partial = add (Rc.new 5)\npartial 1",
        "let shared = Arc.new (Rc.new 5)\nlet read = \\() -> Arc.strong_count (ref shared)\nread ()",
    ] {
        let message = rejects(source, "E1005");
        assert!(
            message.contains("function values may move to other tzroutines or threads"),
            "{message}"
        );
    }
    let message = rejects(
        "let text = \"x\"\nlet shared = Arc.new (ref text)\ntzroutine {\n    let _count = Arc.strong_count (ref shared)\n}\n0",
        "E1013",
    );
    assert!(message.contains("contains a reference"), "{message}");
    for source in [
        "let shared = Arc.new 21\ntzroutine {\n    let _value = deref (Arc.get (ref shared))\n}\n0",
        "let shared = Arc.new 21\nlet weak = Arc.downgrade (ref shared)\ntzroutine {\n    let _count = match Arc.upgrade (ref weak) with | Maybe.Some strong -> Arc.strong_count (ref strong) | Maybe.None -> 0\n}\n0",
        "let shared = Arc.new [1, 2]\nlet read = \\() -> Array.sum (Arc.get (ref shared))\nlet copy = read\nread () + copy ()",
        "union AList = ANil | ACons of (i64 * Arc<AList>)\nlet list = Arc.new (ACons (1, Arc.new ANil))\ntzroutine {\n    let _count = Arc.strong_count (ref list)\n}\n0",
        // A function that spawns a tzroutine with the share it was given, once for each of four shares.
        "def part :: Arc<[i64]> -> unit\nfn part values =\n    tzroutine {\n        let _sum = Array.sum (Arc.get (ref values))\n    }\nlet shared = Arc.new [1, 2, 3]\nfor _index in 0 ..< 4 do\n    part (Arc.share (ref shared))\nArc.strong_count (ref shared)",
    ] {
        emits(source);
    }
}

#[test]
fn shared_values_follow_the_borrow_rules() {
    for source in
        both("def bad :: ref i64\nfn bad =\n    let shared = Rc.new 5\n    Rc.get (ref shared)\n0")
    {
        let message = rejects(&source, "E1013");
        assert!(message.contains("does not live long enough"), "{message}");
    }
    for source in both(
        "let shared = Rc.new \"x\"\nlet text = Rc.get (ref shared)\nlet moved = Rc.try_unwrap shared\ntext.length",
    ) {
        let message = rejects(&source, "E1014");
        assert!(
            message.starts_with("access conflicts with a live borrow"),
            "{message}"
        );
    }
    for source in both(
        "let shared = Rc.new \"x\"\nlet text = Rc.get (ref shared)\nOwned.drop shared\ntext.length",
    ) {
        rejects(&source, "E1014");
    }
    rejects(
        "def bad :: Rc<ref string>\nfn bad =\n    let text = \"x\"\n    Rc.new (ref text)\n0",
        "E1013",
    );
    emits(
        "let text = \"borrowed\"\nlet shared = Rc.new (ref text)\nlet other = Rc.share (ref shared)\nlet value = Rc.get (ref other)\nvalue.length",
    );
    for source in both("let mut x = 5\nlet shared = Rc.new (ref mut x)\n0") {
        let message = rejects(&source, "E1005");
        assert!(
            message.contains("cannot hold a mutable reference"),
            "{message}"
        );
    }
}

fn rejects_with_kind(source: &str, code: &str, kind: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
}

/// A28: `share`, `downgrade` and `upgrade` make a new owner of the block, so their result holds
/// what the block holds (the loans that the borrow of `shared` keeps as parents), not a borrow of
/// `shared` itself. `get` still borrows `shared`.
#[test]
fn sharing_a_function_keeps_the_borrows_of_its_environment_only() {
    for source in [
        "let shared = Rc.new (\\v -> v + 1)\nlet mine = Rc.share (ref shared)\nlet moved = shared\n(deref (Rc.get (ref mine))) 1 + (deref (Rc.get (ref moved))) 1",
        "def make :: unit -> Rc<i64 -> i64>\nfn make _unit =\n    let shared = Rc.new (\\v -> v + 1)\n    Rc.share (ref shared)\nlet f = make ()\n(deref (Rc.get (ref f))) 41",
        "let shared = Rc.new (\\v -> v + 1)\nlet weak = Rc.downgrade (ref shared)\nlet moved = shared\nmatch Rc.upgrade (ref weak) with\n    | Maybe.Some strong -> (deref (Rc.get (ref strong))) 1\n    | Maybe.None -> Rc.strong_count (ref moved)",
        "def make :: unit -> Maybe<Rc<i64 -> i64>>\nfn make _unit =\n    let shared = Rc.new (\\v -> v + 1)\n    let weak = Rc.downgrade (ref shared)\n    Rc.upgrade (ref weak)\nmatch make () with\n    | Maybe.Some f -> (deref (Rc.get (ref f))) 41\n    | Maybe.None -> 0",
        // The environment's owner outlives every share.
        "let text = \"abc\"\nlet borrowed = ref text\nlet shared = Rc.new (\\() -> String.length borrowed)\nlet mine = Rc.share (ref shared)\nlet moved = shared\n(deref (Rc.get (ref mine))) () + Rc.strong_count (ref moved)",
    ] {
        for source in [
            source.to_owned(),
            source.replace("Rc.", "Arc.").replace("Rc<", "Arc<"),
        ] {
            emits(&source);
        }
    }
    for (source, code, kind) in [
        (
            "def make :: unit -> Rc<unit -> i64>\nfn make _unit =\n    let text = \"abc\"\n    let borrowed = ref text\n    let shared = Rc.new (\\() -> String.length borrowed)\n    Rc.share (ref shared)\nlet f = make ()\nRc.strong_count (ref f)",
            "E1013",
            "borrow_outlives_block",
        ),
        (
            "def make :: unit -> Rc.Weak<unit -> i64>\nfn make _unit =\n    let text = \"abc\"\n    let borrowed = ref text\n    let shared = Rc.new (\\() -> String.length borrowed)\n    Rc.downgrade (ref shared)\nlet f = make ()\nmatch Rc.upgrade (ref f) with\n    | Maybe.Some strong -> Rc.strong_count (ref strong)\n    | Maybe.None -> 0",
            "E1013",
            "borrow_outlives_block",
        ),
        (
            "def make :: unit -> i64\nfn make _unit =\n    let f = { let shared = Rc.new (\\v -> v + 1); Rc.get (ref shared) }\n    (deref f) 1\nmake ()",
            "E1013",
            "borrow_outlives_block",
        ),
        (
            "let text = \"abc\"\nlet borrowed = ref text\nlet shared = Rc.new (\\() -> String.length borrowed)\nlet mine = Rc.share (ref shared)\nlet moved = text\nRc.strong_count (ref mine) + moved.length",
            "E1014",
            "borrow_conflict",
        ),
        (
            "let shared = Rc.new (\\v -> v + 1)\nlet read = Rc.get (ref shared)\nlet moved = shared\n(deref read) 1 + Rc.strong_count (ref moved)",
            "E1014",
            "borrow_conflict",
        ),
    ] {
        for source in [
            source.to_owned(),
            source.replace("Rc.", "Arc.").replace("Rc<", "Arc<"),
        ] {
            rejects_with_kind(&source, code, kind);
        }
    }
}

/// A28 review: the result of `share`, `downgrade` and `upgrade` holds copies of the loans that the
/// block holds, not the loans themselves, which a live borrow of the owner reborrows: moving the
/// new owner while `Rc.get (ref shared)` or `ref shared` is live is not a move of a reborrowed
/// value. The base accepts these programs and prints 5, 5, 5, 4, 4 and 5.
#[test]
fn a_new_owner_moves_while_the_owner_is_borrowed() {
    for source in [
        "let text = \"abc\"\nlet shared = Rc.new (ref text)\nlet view = Rc.get (ref shared)\nlet mine = Rc.share (ref shared)\nlet moved = mine\nString.length (deref view) + Rc.strong_count (ref moved)",
        "let text = \"abc\"\nlet borrowed = ref text\nlet shared = Rc.new (\\() -> String.length borrowed)\nlet r = ref shared\nlet mine = Rc.share (ref shared)\nlet moved = mine\nRc.strong_count r + (deref (Rc.get (ref moved))) ()",
        "let text = \"abc\"\nlet borrowed = ref text\nlet shared = Rc.new (\\() -> String.length borrowed)\nlet mine = Rc.share (ref shared)\nlet r = ref shared\nlet moved = mine\nRc.strong_count r + (deref (Rc.get (ref moved))) ()",
        "def total :: own Rc<ref string> -> i64\nfn total value = String.length (deref (Rc.get (ref value)))\nlet text = \"abcd\"\nlet shared = Rc.new (ref text)\nlet r = ref shared\nlet mine = Rc.share (ref shared)\ntotal mine + 0 * Rc.strong_count r",
        "let text = \"abc\"\nlet shared = Rc.new (ref text)\nlet view = Rc.get (ref shared)\nlet weak = Rc.downgrade (ref shared)\nlet moved = weak\nString.length (deref view) + Rc.strong_count (ref shared)",
        "let text = \"abc\"\nlet shared = Rc.new (ref text)\nlet view = Rc.get (ref shared)\nlet mine = Rc.share (ref shared)\nlet pair = (mine, 1)\nString.length (deref view) + Rc.strong_count (ref shared)",
        // The pipeline form reads through as the call does.
        "let text = \"abc\"\nlet shared = Rc.new (ref text)\nlet view = Rc.get (ref shared)\nlet mine = ref shared |> Rc.share\nlet moved = mine\nString.length (deref view) + Rc.strong_count (ref moved)",
    ] {
        for source in [
            source.to_owned(),
            source.replace("Rc.", "Arc.").replace("Rc<", "Arc<"),
        ] {
            emits(&source);
        }
    }
    // The copies keep the borrowed data lent, as the originals do.
    for source in [
        "let text = \"abc\"\nlet shared = Rc.new (ref text)\nlet view = Rc.get (ref shared)\nlet mine = Rc.share (ref shared)\nlet moved = mine\nlet gone = text\nString.length (deref view) + Rc.strong_count (ref moved)",
        "def make :: unit -> Rc<ref string>\nfn make _unit =\n    let text = \"abc\"\n    let shared = Rc.new (ref text)\n    let mine = ref shared |> Rc.share\n    mine\nRc.strong_count (ref (make ()))",
    ] {
        let error = analyze(source).expect_err(source);
        assert!(
            error.code == "E1014" || error.code == "E1013",
            "{source}\n{}",
            error.message
        );
    }
}

/// A28 review: `Array.write` stores a function that borrows `text` in `arr`, so a share of
/// `arr[0]` cannot leave `make`, which frees `text`. Before A31 the check did not track that store,
/// so A28 read nothing through in a body that lends an exclusive borrow to a call (the share held
/// the borrow of `arr`); reading through, the share held what `arr` held before the write. Since
/// A31 the store facts put what the write stores in `arr`'s loans, which a read through holds (the
/// abstract bodies of the inference, which apply no store facts, keep A28's rule). The A30 tip
/// rejects both forms here.
#[test]
fn a_share_of_an_element_holds_what_was_written_in() {
    for share in ["Rc.share (ref arr[0])", "ref arr[0] |> Rc.share"] {
        rejects_with_kind(
            &format!(
                "def make :: own i64 -> Rc<unit -> i64>\nfn make n =\n    let text = to_string n\n    let borrowed = ref text\n    let mut arr = [Rc.new (\\() -> 0)]\n    Array.write 0 (Rc.new (\\() -> String.length borrowed)) (ref mut arr)\n    {share}\nlet f = make 123456789\n(deref (Rc.get (ref f))) ()"
            ),
            "E1013",
            "borrow_outlives_block",
        );
    }
    // A lambda that the writing body makes captures `arr` as it was before the write, so the
    // lambda's body reads nothing through it either (`lending_bodies`). Reading through, its share
    // held nothing, `make` returned it, and `g` read the freed `text`. The A30 tip rejects these.
    for share in [
        "\\() -> Arc.share (ref arr[0])",
        "\\() -> ref arr[0] |> Arc.share",
        "\\() -> \\() -> Arc.share (ref arr[0])",
    ] {
        let nested = share.matches("\\()").count() == 2;
        let (result, call) = if nested {
            ("unit -> unit -> Arc<unit -> i64>", "f () ()")
        } else {
            ("unit -> Arc<unit -> i64>", "f ()")
        };
        rejects_with_kind(
            &format!(
                "def make :: own i64 -> ({result})\nfn make n =\n    let text = to_string n\n    let borrowed = ref text\n    let mut arr = [Arc.new (\\() -> 0)]\n    Array.write 0 (Arc.new (\\() -> String.length borrowed)) (ref mut arr)\n    {share}\nlet f = make 123456789\nlet g = {call}\n(deref (Arc.get (ref g))) ()"
            ),
            "E1013",
            "task_result_retains_borrow",
        );
    }
    // Without the write too: `arr` is a capture of the lifted lambda, and a capture's loans stand
    // for what the enclosing body's value holds, so the share keeps the borrow of `arr` (the A30
    // tip's false positive, which A28 keeps; `read_through`).
    rejects_with_kind(
        "def make :: own i64 -> (unit -> Arc<unit -> i64>)\nfn make n =\n    let arr = [Arc.new (\\() -> n)]\n    \\() -> Arc.share (ref arr[0])\nlet f = make 41\nlet g = f ()\n(deref (Arc.get (ref g))) ()",
        "E1013",
        "task_result_retains_borrow",
    );
    // A helper that shares out of its own parameter: an `own` parameter's loans stand for what the
    // caller's value holds, and a caller that wrote a borrowing value into `arr` with
    // `Array.write` passes the loans that `arr` held before the write. Reading through, `make`
    // returned a share of the function that borrows the freed `text`. Each keeps the A30 tip's
    // diagnostic at the share in the helper; the upgrade is below.
    let make = |kind: &str, result: &str, call: &str| {
        format!(
            "def make :: own i64 -> {result}\nfn make n =\n    let text = to_string n\n    let borrowed = ref text\n    let mut arr = [{kind}.new (\\() -> 0)]\n    Array.write 0 ({kind}.new (\\() -> String.length borrowed)) (ref mut arr)\n    {call}\n"
        )
    };
    let arc = "Arc<unit -> i64>";
    let read = "let g = make 123456789\n(deref (Arc.get (ref g))) ()";
    let wrap = "def wrap :: own [Arc<unit -> i64>] -> Arc<unit -> i64>\nfn wrap arr = Arc.share (ref arr[0])\n";
    for (source, (line, column)) in [
        (
            format!("{wrap}{}{read}", make("Arc", arc, "wrap arr")),
            (2, 15),
        ),
        (
            format!(
                "def wrap :: own [Rc<unit -> i64>] -> Rc<unit -> i64>\nfn wrap arr = Rc.share (ref arr[0])\n{}let g = make 123456789\n(deref (Rc.get (ref g))) ()",
                make("Rc", "Rc<unit -> i64>", "wrap arr")
            ),
            (2, 15),
        ),
        (
            format!(
                "def wrap :: own [Arc<unit -> i64>] -> Arc<unit -> i64>\nfn wrap arr = ref arr[0] |> Arc.share\n{}{read}",
                make("Arc", arc, "wrap arr")
            ),
            (2, 15),
        ),
        (
            format!("{wrap}{}{read}", make("Arc", arc, "arr |> wrap")),
            (2, 15),
        ),
        (
            format!(
                "def first_share :: own [Arc<'a>] -> Arc<'a>\nfn first_share arr = Arc.share (ref arr[0])\n{}{read}",
                make("Arc", arc, "first_share arr")
            ),
            (2, 22),
        ),
        (
            format!(
                "record Bag {{ items: [Arc<unit -> i64>] }}\ndef wrap :: own Bag -> Arc<unit -> i64>\nfn wrap bag = Arc.share (ref bag.items[0])\n{}{read}",
                make("Arc", arc, "wrap (Bag { items: arr })")
            ),
            (3, 15),
        ),
        (
            format!(
                "def wrap :: own [Arc<unit -> i64>] -> (unit -> Arc<unit -> i64>)\nfn wrap arr = \\() -> Arc.share (ref arr[0])\n{}let h = make 123456789\nlet g = h ()\n(deref (Arc.get (ref g))) ()",
                make("Arc", "(unit -> Arc<unit -> i64>)", "wrap arr")
            ),
            (2, 16),
        ),
        (
            format!(
                "def wrap :: own Arc<[Arc<unit -> i64>]> -> Arc<[Arc<unit -> i64>]>\nfn wrap shared = Arc.share (ref shared)\n{}let g = make 123456789\nlet items = Arc.get (ref g)\n(deref (Arc.get (ref items[0]))) ()",
                make("Arc", "Arc<[Arc<unit -> i64>]>", "wrap (Arc.new arr)")
            ),
            (2, 18),
        ),
        // Without the write, the A30 tip's false positive stays.
        (
            format!("{wrap}let g = wrap [Arc.new (\\() -> 41)]\n(deref (Arc.get (ref g))) ()"),
            (2, 15),
        ),
    ] {
        let error = analyze(&source).expect_err(&source);
        assert_eq!(error.code, "E1013", "{source}\n{}", error.message);
        assert_eq!(
            error.kind,
            Some("task_result_retains_borrow"),
            "{source}\n{}",
            error.message
        );
        let before = &source[..error.span.start];
        let position = (
            before.matches('\n').count() + 1,
            before.len() - before.rfind('\n').map_or(0, |at| at + 1) + 1,
        );
        assert_eq!(position, (line, column), "{source}\n{}", error.message);
    }
    // The upgrade: the weak pointer is a downgrade of `arr`'s element, whose contents come from the
    // caller, so the upgrade keeps the borrow of `arr`, which `wrap` drops. A31 (step 4): `make`'s
    // store with `Array.write` no longer drops every fact of the program (A28's eighth review
    // rejected the upgrade itself, which then held the borrow of `weak`, at 4:5), so the error is
    // the seventh round's again, at the body of `wrap`.
    let source = format!(
        "def wrap :: own [Arc<unit -> i64>] -> Maybe<Arc<unit -> i64>>\nfn wrap arr =\n    let weak = Arc.downgrade (ref arr[0])\n    Arc.upgrade (ref weak)\n{}match make 123456789 with\n    | Maybe.Some g -> (deref (Arc.get (ref g))) ()\n    | Maybe.None -> 0",
        make("Arc", "Maybe<Arc<unit -> i64>>", "wrap arr")
    );
    let error = analyze(&source).expect_err(&source);
    assert_eq!(error.code, "E1013", "{source}\n{}", error.message);
    assert_eq!(
        error.kind,
        Some("task_result_retains_borrow"),
        "{source}\n{}",
        error.message
    );
    assert_eq!(
        &source[error.span.start..error.span.end],
        "let weak = Arc.downgrade (ref arr[0])\n    Arc.upgrade (ref weak)",
        "{}",
        error.message
    );
    // A local that takes the parameter's value holds the caller's loans too.
    rejects_with_kind(
        &format!(
            "def wrap :: own [Arc<unit -> i64>] -> Arc<unit -> i64>\nfn wrap arr =\n    let a = arr\n    Arc.share (ref a[0])\n{}{read}",
            make("Arc", arc, "wrap arr")
        ),
        "E1013",
        "borrow_outlives_block",
    );
}

/// A28 review (5th round): the lock builtins write into a body's own `Mutex` or `RwMutex` through a
/// shared reference, in a callback that is a function value (`put text`, a lambda given to a
/// helper) or in a helper that takes the lock by `ref`. A28 gave a body that passes a mutable cell
/// whose contents may hold a borrow, or a function whose type reaches an exclusive reference or
/// such a cell (`Type::lends_mutable`), no facts and no read-through. Since A31 the store facts of
/// the callback put what it stores in the lock's owner (the abstract bodies of the inference, which
/// apply no store facts, keep A28's rule where the body may store). Each program keeps the A30
/// tip's diagnostic at its position. Accepted by the 4th round, `m_partial` printed 2 at `-O0`
/// (it reads the freed `text`; 11 is right) and `rt_partial` a pointer. `m_named_owned` writes an
/// owned closure and is the A30 tip's false positive, which A28 keeps. A lambda cannot capture an
/// exclusive borrow (`E1005`), so the capture form of a writer is rejected before the ownership
/// check.
#[test]
fn a_lock_callback_lends_a_way_to_write() {
    for (source, code, kind, (line, column)) in [
        // m_partial
        (
            "def put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn put text slot = Array.write 0 (\\() -> String.length text) slot\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let m = Mutex.create [\\() -> 0]\n    Mutex.with_lock (ref m) (put text)\n    let arr = Mutex.into_inner m\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (13, 5),
        ),
        // rw_partial
        (
            "def put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn put text slot = Array.write 0 (\\() -> String.length text) slot\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let m = RwMutex.create [\\() -> 0]\n    RwMutex.with_write (ref m) (put text)\n    let arr = RwMutex.into_inner m\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (13, 5),
        ),
        // m_partial_set
        (
            "def put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn put text slot = Array.set 0 (\\() -> String.length text) slot\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let m = Mutex.create [\\() -> 0]\n    Mutex.with_lock (ref m) (put text)\n    let arr = Mutex.into_inner m\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (13, 5),
        ),
        // m_partial_pipe
        (
            "def put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn put text slot = Array.write 0 (\\() -> String.length text) slot\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let m = Mutex.create [\\() -> 0]\n    Mutex.with_lock (ref m) (put text)\n    let arr = Mutex.into_inner m\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    keep |> stash (ref text)\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (13, 5),
        ),
        // m_helper_fill
        (
            "def put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn put text slot = Array.write 0 (\\() -> String.length text) slot\ndef fill_lock :: ref string -> ref Mutex<[(unit -> i64)]> -> unit\nfn fill_lock text m = Mutex.with_lock m (put text)\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let m = Mutex.create [\\() -> 0]\n    fill_lock text (ref m)\n    let arr = Mutex.into_inner m\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (15, 5),
        ),
        // cond_partial
        (
            "def put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn put text slot = Array.write 0 (\\() -> String.length text) slot\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let m = Mutex.create [\\() -> 0]\n    let c = Cond.create ()\n    Cond.wait_with (ref c) (ref m) (\\_ -> true) (put text)\n    let arr = Mutex.into_inner m\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (14, 5),
        ),
        // record_mutex_helper
        (
            "record Holder { m: Mutex<[(unit -> i64)]>, n: i64 }\ndef put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn put text slot = Array.write 0 (\\() -> String.length text) slot\ndef fill :: ref string -> ref Holder -> unit\nfn fill text holder = Mutex.with_lock (ref holder.m) (put text)\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let holder = Holder { m: Mutex.create [\\() -> 0], n: 1 }\n    fill text (ref holder)\n    let arr = Mutex.into_inner holder.m\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (16, 5),
        ),
        // arc_mutex_helper
        (
            "def put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn put text slot = Array.write 0 (\\() -> String.length text) slot\ndef fill :: ref string -> ref Arc<Mutex<[(unit -> i64)]>> -> unit\nfn fill text am = Mutex.with_lock (Arc.get am) (put text)\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let am = Arc.new (Mutex.create [\\() -> 0])\n    fill text (ref am)\n    match Arc.try_unwrap am with\n    | Ok m ->\n        let arr = Mutex.into_inner m\n        let f = arr[0]\n        \\() -> f () + String.length other\n    | Error _ -> \\() -> 0\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (18, 5),
        ),
        // closure_param_writer
        (
            "def run :: ref Mutex<[(unit -> i64)]> -> own (ref mut [(unit -> i64)] -> unit) -> unit\nfn run m g = Mutex.with_lock m g\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let m = Mutex.create [\\() -> 0]\n    run (ref m) (\\slot -> Array.write 0 (\\() -> String.length text) slot)\n    let arr = Mutex.into_inner m\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (13, 5),
        ),
        // lambda_lock_lifted
        (
            "def apply :: ref Mutex<[(unit -> i64)]> -> own (ref Mutex<[(unit -> i64)]> -> unit) -> unit\nfn apply m g = g m\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let m = Mutex.create [\\() -> 0]\n    apply (ref m) (\\mm -> Mutex.with_lock mm (\\slot -> Array.write 0 (\\() -> String.length text) slot))\n    let arr = Mutex.into_inner m\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (13, 5),
        ),
        // arc_mutex_capture
        (
            "def run :: own (unit -> unit) -> unit\nfn run g = g ()\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let am = Arc.new (Mutex.create [\\() -> 0])\n    let mine = Arc.share (ref am)\n    run (\\() -> Mutex.with_lock (Arc.get (ref mine)) (\\slot -> Array.write 0 (\\() -> String.length text) slot))\n    match Arc.try_unwrap am with\n    | Ok m ->\n        let arr = Mutex.into_inner m\n        let f = arr[0]\n        \\() -> f () + String.length other\n    | Error _ -> \\() -> 0\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (17, 5),
        ),
        // m_named_owned
        (
            "def writer :: ref mut [(unit -> i64)] -> unit\nfn writer slot = Array.write 0 (\\() -> 5) slot\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let m = Mutex.create [\\() -> 0]\n    Mutex.with_lock (ref m) writer\n    let arr = Mutex.into_inner m\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1013",
            "borrow_outlives_block",
            (13, 5),
        ),
        // rt_partial
        (
            "def put :: ref string -> ref mut [Arc<unit -> i64>] -> unit\nfn put text slot = Array.write 0 (Arc.new (\\() -> String.length text)) slot\ndef make :: own i64 -> Arc<unit -> i64>\nfn make n =\n    let text = to_string n\n    let m = Mutex.create [Arc.new (\\() -> 0)]\n    Mutex.with_lock (ref m) (put (ref text))\n    let arr = Mutex.into_inner m\n    Arc.share (ref arr[0])\nlet g = make 123456789\n(deref (Arc.get (ref g))) ()",
            "E1013",
            "borrow_outlives_block",
            (9, 5),
        ),
        // rt_helper
        (
            "def put :: ref string -> ref mut [Arc<unit -> i64>] -> unit\nfn put text slot = Array.write 0 (Arc.new (\\() -> String.length text)) slot\ndef fill_lock :: ref string -> ref Mutex<[Arc<unit -> i64>]> -> unit\nfn fill_lock text m = Mutex.with_lock m (put text)\ndef make :: own i64 -> Arc<unit -> i64>\nfn make n =\n    let text = to_string n\n    let m = Mutex.create [Arc.new (\\() -> 0)]\n    fill_lock (ref text) (ref m)\n    let arr = Mutex.into_inner m\n    Arc.share (ref arr[0])\nlet g = make 123456789\n(deref (Arc.get (ref g))) ()",
            "E1013",
            "borrow_outlives_block",
            (11, 5),
        ),
        // cond_rt
        (
            "def put :: ref string -> ref mut [Arc<unit -> i64>] -> unit\nfn put text slot = Array.write 0 (Arc.new (\\() -> String.length text)) slot\ndef make :: own i64 -> Arc<unit -> i64>\nfn make n =\n    let text = to_string n\n    let m = Mutex.create [Arc.new (\\() -> 0)]\n    let c = Cond.create ()\n    Cond.wait_with (ref c) (ref m) (\\_ -> true) (put (ref text))\n    let arr = Mutex.into_inner m\n    Arc.share (ref arr[0])\nlet g = make 123456789\n(deref (Arc.get (ref g))) ()",
            "E1013",
            "borrow_outlives_block",
            (10, 5),
        ),
        // rt_tz
        (
            "def put :: ref string -> ref mut [Arc<unit -> i64>] -> unit\nfn put text slot = Array.write 0 (Arc.new (\\() -> String.length text)) slot\ndef wait_for :: ref Arc<Atomic<i64>> -> i64 -> unit\nfn wait_for counter target =\n    while Atomic.load (Arc.get counter) < target do\n        Routine.yield ()\ndef start :: own i64 -> ref Arc<Atomic<i64>> -> ref Arc<Atomic<i64>> -> unit\nfn start n result done =\n    let text = to_string n\n    let m = Mutex.create [Arc.new (\\() -> 0)]\n    Mutex.with_lock (ref m) (put (ref text))\n    let arr = Mutex.into_inner m\n    let s = Arc.share (ref arr[0])\n    let mine = Arc.share result\n    let finished = Arc.share done\n    tzroutine {\n        let _sum = Atomic.fetch_add (Arc.get (ref mine)) ((deref (Arc.get (ref s))) ())\n        let _count = Atomic.fetch_add (Arc.get (ref finished)) 1i64\n    }\nlet result = Arc.new (Atomic.create 0i64)\nlet done = Arc.new (Atomic.create 0i64)\nstart 123456789 (ref result) (ref done)\nwait_for (ref done) 1\nAtomic.load (Arc.get (ref result))",
            "E1013",
            "task_capture_retains_borrow",
            (16, 5),
        ),
        // closure_capture_refmut
        (
            "def run :: own (unit -> unit) -> unit\nfn run g = g ()\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let mut arr = [\\() -> 0]\n    let slot = ref mut arr\n    run (\\() -> Array.write 0 (\\() -> String.length text) slot)\n    let f = arr[0]\n    \\() -> f () + String.length other\ndef leak :: ref string -> own i64 -> (unit -> i64)\nfn leak keep n =\n    let text = to_string n\n    stash (ref text) keep\nlet k = \"ab\"\nlet f = leak (ref k) 123456789\nf ()",
            "E1005",
            "capture_exclusive_in_reusable_function",
            (7, 9),
        ),
    ] {
        let error = analyze(source).expect_err(source);
        assert_eq!(error.code, code, "{source}\n{}", error.message);
        assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
        let before = &source[..error.span.start];
        let position = (
            before.matches('\n').count() + 1,
            before.len() - before.rfind('\n').map_or(0, |at| at + 1) + 1,
        );
        assert_eq!(position, (line, column), "{source}\n{}", error.message);
    }
    // The controls: the same writes while `text` is alive (they print 11 and 9), a counter whose
    // `Atomic<i64>` holds no borrow (the A30 tip rejects it: the share of `shared` keeps its
    // borrow), and a writer that is passed but never called.
    for source in [
        "def put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn put text slot = Array.write 0 (\\() -> String.length text) slot\ndef stash :: ref string -> ref string -> (unit -> i64)\nfn stash text other =\n    let m = Mutex.create [\\() -> 0]\n    Mutex.with_lock (ref m) (put text)\n    let arr = Mutex.into_inner m\n    let f = arr[0]\n    \\() -> f () + String.length other\nlet k = \"ab\"\nlet t = to_string 123456789\nlet f = stash (ref t) (ref k)\nf ()",
        "def put :: ref string -> ref mut [Arc<unit -> i64>] -> unit\nfn put text slot = Array.write 0 (Arc.new (\\() -> String.length text)) slot\nlet text = to_string 123456789\nlet m = Mutex.create [Arc.new (\\() -> 0)]\nMutex.with_lock (ref m) (put (ref text))\nlet arr = Mutex.into_inner m\nlet g = Arc.share (ref arr[0])\n(deref (Arc.get (ref g))) ()",
        "let counter = Arc.new (Atomic.create 0i64)\nlet mine = Arc.share (ref counter)\nlet shared = Arc.new (\\v -> v + 1)\nlet f = Arc.share (ref shared)\ntzroutine {\n    let _x = Atomic.fetch_add (Arc.get (ref mine)) ((deref (Arc.get (ref f))) 1)\n}\n0",
        "def put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn put text slot = Array.write 0 (\\() -> String.length text) slot\ndef via :: own (ref mut [(unit -> i64)] -> unit) -> own (unit -> i64) -> (unit -> i64)\nfn via _g f = f\ndef wrap :: ref string -> own (unit -> i64) -> (unit -> i64)\nfn wrap text f = via (put text) f\nlet t = \"abc\"\nlet g = wrap (ref t) (\\() -> 42)\ng ()",
    ] {
        emits(source);
    }
}

/// A28 review (6th round): a function type, a dyn type and an `Owned.Function` do not show their
/// environments, so a body that hands a borrowing value to such code (a function-valued parameter,
/// a field, a partial application, `Owned.call`, a dyn method, a builtin that runs its callback
/// such as `Array.iter`, `Array.map` and `Array.fold`) may have it stored in a cell that the
/// code captured, where another input (a reader closure over the same `Arc<Mutex<...>>`, the dyn
/// value) reads it back. Such a body has no facts and reads nothing through, and a call of a
/// function whose body is one counts as one when it passes a value that can run code. A closure
/// called with `()` stores what it captured (`thunk_env`), so a call that only the callee's
/// environment makes opaque drops the facts too. Each program keeps the A30 tip's diagnostic at its
/// position; accepted by the 5th round, they printed garbage at `-O0` (`String.length` of the
/// freed `text`; 11 is right) and the dyn form aborted.
#[test]
fn an_opaque_call_hands_a_borrow_to_unseen_code() {
    let at = |source: &str, error: &tsuzuri::diagnostic::Diagnostic| {
        let before = &source[..error.span.start];
        (
            before.matches('\n').count() + 1,
            before.len() - before.rfind('\n').map_or(0, |at| at + 1) + 1,
        )
    };
    for (source, (line, column)) in [
        // mx_leak
        (
            "def writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> ref string -> (unit -> i64)\nfn pass w r text other =\n    w (\\() -> String.length text)\n    \\() -> r () + String.length other\ndef leak :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> own i64 -> (unit -> i64)\nfn leak w r keep n =\n    let text = to_string n\n    pass w r (ref text) keep\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let k = \"ab\"\n    let w = writer a1\n    let r = reader a2\n    let f = leak w r (ref k) 123456789\n    f ()\nrun ()",
            (12, 5),
        ),
        // rw_leak
        (
            "def writer :: own Arc<RwMutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> RwMutex.with_write (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<RwMutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> RwMutex.with_write (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> ref string -> (unit -> i64)\nfn pass w r text other =\n    w (\\() -> String.length text)\n    \\() -> r () + String.length other\ndef leak :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> own i64 -> (unit -> i64)\nfn leak w r keep n =\n    let text = to_string n\n    pass w r (ref text) keep\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (RwMutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let k = \"ab\"\n    let w = writer a1\n    let r = reader a2\n    let f = leak w r (ref k) 123456789\n    f ()\nrun ()",
            (12, 5),
        ),
        // mxi_leak
        (
            "def pass :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> ref string -> (unit -> i64)\nfn pass w r text other =\n    w (\\() -> String.length text)\n    \\() -> r () + String.length other\ndef leak :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> own i64 -> (unit -> i64)\nfn leak w r keep n =\n    let text = to_string n\n    pass w r (ref text) keep\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let k = \"ab\"\n    let w = \\g -> Mutex.with_lock (Arc.get (ref a1)) (\\slot -> Array.write 0 g slot)\n    let r = \\() -> Mutex.with_lock (Arc.get (ref a2)) (\\slot -> slot[0] ())\n    let f = leak w r (ref k) 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (8, 5),
        ),
        // owned_call
        (
            "def pass :: ref Owned.Function<Maybe<(unit -> i64)>, i64> -> ref string -> ref string -> (unit -> i64)\nfn pass w text other =\n    let _stored = Owned.call w (Maybe.Some (\\() -> String.length text))\n    \\() -> Owned.call w Maybe.None + String.length other\ndef leak :: ref Owned.Function<Maybe<(unit -> i64)>, i64> -> ref string -> own i64 -> (unit -> i64)\nfn leak w keep n =\n    let text = to_string n\n    pass w (ref text) keep\ndef run :: unit -> i64\nfn run u =\n    let k = \"ab\"\n    let m = Mutex.create [\\() -> 0]\n    let w = Owned.function (\\x -> match x with\n        | Maybe.Some g -> Mutex.with_lock (ref m) (\\slot -> { Array.write 0 g slot; 0 })\n        | Maybe.None -> Mutex.with_lock (ref m) (\\slot -> slot[0] ()))\n    let f = leak (ref w) (ref k) 123456789\n    f ()\nrun ()",
            (8, 5),
        ),
        // array_iter
        (
            "def writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> ref string -> (unit -> i64)\nfn pass w r text other =\n    let arr = [\\() -> String.length text]\n    Array.iter w (ref arr)\n    \\() -> r () + String.length other\ndef leak :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> own i64 -> (unit -> i64)\nfn leak w r keep n =\n    let text = to_string n\n    pass w r (ref text) keep\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let k = \"ab\"\n    let w = writer a1\n    let r = reader a2\n    let f = leak w r (ref k) 123456789\n    f ()\nrun ()",
            (13, 5),
        ),
        // array_map
        (
            "def writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> ref string -> (unit -> i64)\nfn pass w r text other =\n    let arr = [\\() -> String.length text]\n    let _units = Array.map w (ref arr)\n    \\() -> r () + String.length other\ndef leak :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> own i64 -> (unit -> i64)\nfn leak w r keep n =\n    let text = to_string n\n    pass w r (ref text) keep\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let k = \"ab\"\n    let w = writer a1\n    let r = reader a2\n    let f = leak w r (ref k) 123456789\n    f ()\nrun ()",
            (13, 5),
        ),
        // array_fold
        (
            "def writer :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)\nfn writer a = \\n g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { Array.write 0 g slot; n + 1 })\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own (i64 -> (unit -> i64) -> i64) -> own (unit -> i64) -> ref string -> ref string -> (unit -> i64)\nfn pass w r text other =\n    let arr = [\\() -> String.length text]\n    let _count = Array.fold w 0 (ref arr)\n    \\() -> r () + String.length other\ndef leak :: own (i64 -> (unit -> i64) -> i64) -> own (unit -> i64) -> ref string -> own i64 -> (unit -> i64)\nfn leak w r keep n =\n    let text = to_string n\n    pass w r (ref text) keep\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let k = \"ab\"\n    let w = writer a1\n    let r = reader a2\n    let f = leak w r (ref k) 123456789\n    f ()\nrun ()",
            (13, 5),
        ),
        // field_partial
        (
            "record Sink { put: (unit -> i64) -> unit }\ndef store :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> unit\nfn store a g = Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: ref Sink -> own (unit -> i64) -> ref string -> ref string -> (unit -> i64)\nfn pass s r text other =\n    s.put (\\() -> String.length text)\n    \\() -> r () + String.length other\ndef leak :: ref Sink -> own (unit -> i64) -> ref string -> own i64 -> (unit -> i64)\nfn leak s r keep n =\n    let text = to_string n\n    pass s r (ref text) keep\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let k = \"ab\"\n    let s = Sink { put: store a1 }\n    let r = reader a2\n    let f = leak (ref s) r (ref k) 123456789\n    f ()\nrun ()",
            (13, 5),
        ),
        // thunk_env
        (
            "def writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own (unit -> unit) -> own (unit -> i64) -> (unit -> i64)\nfn pass t r =\n    t ()\n    \\() -> r ()\ndef leak :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak w r n =\n    let text = to_string n\n    let borrowed = ref text\n    pass (\\() -> w (\\() -> String.length borrowed)) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let w = writer a1\n    let r = reader a2\n    let f = leak w r 123456789\n    f ()\nrun ()",
            (13, 5),
        ),
    ] {
        let error = analyze(source).expect_err(source);
        assert_eq!(
            error.code, "E1013",
            "{source}
{}",
            error.message
        );
        assert_eq!(
            error.kind,
            Some("borrow_outlives_block"),
            "{source}
{}",
            error.message
        );
        assert_eq!(
            at(source, &error),
            (line, column),
            "{source}
{}",
            error.message
        );
    }
    // dyn_generic: a generic `pass` specialized at `dyn Sinks.Sink {r}` calls the vtable.
    let sinks = "class Sink<'value> {\n    def put :: ref 'value -> own (unit -> i64) -> unit\n    def read :: ref 'value -> i64\n}";
    let main = "record Cell {r} { m: ref {r} Mutex<[unit -> i64]> }\ninstance Sinks.Sink<Cell> {\n    fn put cell g = Mutex.with_lock cell.m (\\slot -> Array.write 0 g slot)\n    fn read cell = Mutex.with_lock cell.m (\\slot -> slot[0] ())\n}\ndef make {r} :: ref {r} Mutex<[unit -> i64]> -> dyn Sinks.Sink {r}\nfn make m = Dyn.of (Cell { m: m })\ndef pass :: Sinks.Sink<'a> => ref 'a -> ref string -> ref string -> (unit -> i64)\nfn pass w text other =\n    Sinks.Sink.put w (\\() -> String.length text)\n    \\() -> Sinks.Sink.read w + String.length other\ndef leak :: Sinks.Sink<'a> => ref 'a -> ref string -> own i64 -> (unit -> i64)\nfn leak w keep n =\n    let text = to_string n\n    pass w (ref text) keep\ndef run :: unit -> i64\nfn run u =\n    let k = \"ab\"\n    let m = Mutex.create [\\() -> 0]\n    let w = make (ref m)\n    let f = leak (ref w) (ref k) 123456789\n    f ()\nrun ()";
    let error = analyze_modules(&[("Sinks.tt", sinks), ("Main.tz", main)]).expect_err(main);
    assert_eq!(error.code, "E1013", "{}", error.message);
    assert_eq!(
        error.kind,
        Some("borrow_outlives_block"),
        "{}",
        error.message
    );
    assert_eq!(at(main, &error), (15, 5), "{}", error.message);
    // The controls: the same programs while `text` is alive (they print 11).
    for source in [
        "def writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> ref string -> (unit -> i64)\nfn pass w r text other =\n    w (\\() -> String.length text)\n    \\() -> r () + String.length other\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let k = \"ab\"\n    let t = to_string 123456789\n    let w = writer a1\n    let r = reader a2\n    let f = pass w r (ref t) (ref k)\n    f ()\nrun ()",
        "def pass :: ref Owned.Function<Maybe<(unit -> i64)>, i64> -> ref string -> ref string -> (unit -> i64)\nfn pass w text other =\n    let _stored = Owned.call w (Maybe.Some (\\() -> String.length text))\n    \\() -> Owned.call w Maybe.None + String.length other\ndef run :: unit -> i64\nfn run u =\n    let k = \"ab\"\n    let t = to_string 123456789\n    let m = Mutex.create [\\() -> 0]\n    let w = Owned.function (\\x -> match x with\n        | Maybe.Some g -> Mutex.with_lock (ref m) (\\slot -> { Array.write 0 g slot; 0 })\n        | Maybe.None -> Mutex.with_lock (ref m) (\\slot -> slot[0] ()))\n    let f = pass (ref w) (ref t) (ref k)\n    f ()\nrun ()",
    ] {
        emits(source);
    }
}

/// A28, seventh review: dropping a value runs its `Drop.drop` without a call in the body, and the
/// instance may store what the value holds in a cell inside it that the caller reads through
/// another value (`r`). The parameters whose loans a dropped value holds are retained, as the A30
/// tip has it: an unused or moved-in parameter, a local made from a borrowed parameter, a
/// reassigned local, a container of such values, `Owned.drop`, and a named callee that drops its
/// argument and returns `unit`. Only a program with a `Drop.drop` that makes a store that the check
/// does not track (through a cell, or by calling a function value) is affected. From the eighth
/// review until A31, a program that stored a borrowing function with `Array.write` had no facts at
/// all (`untracked_loan_store`), which every rejected form here does (in `Drop.drop` or in
/// `writer`); since A31 tracks that store, the local guard (`Lending::drops_store`, A28's
/// `dropped`) and A31's own drop hook reject them, at the same positions.
#[test]
fn an_implicit_drop_keeps_what_the_dropped_value_holds() {
    let at = |source: &str, error: &tsuzuri::diagnostic::Diagnostic| {
        let before = &source[..error.span.start];
        (
            before.matches('\n').count() + 1,
            before.len() - before.rfind('\n').map_or(0, |at| at + 1) + 1,
        )
    };
    for (source, (line, column)) in [
        // drop_unused
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Stash -> own (unit -> i64) -> (unit -> i64)\nfn pass _s r = r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    pass (Stash { cell: a, f: \\() -> String.length borrowed }) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (13, 5),
        ),
        // drop_param
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Stash -> own (unit -> i64) -> (unit -> i64)\nfn pass s r =\n    let _held = s\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    pass (Stash { cell: a, f: \\() -> String.length borrowed }) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (15, 5),
        ),
        // drop_local
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a text r =\n    let _held = Stash { cell: a, f: \\() -> String.length text }\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    pass a (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (14, 5),
        ),
        // drop_reassign
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Stash -> own Stash -> own (unit -> i64) -> (unit -> i64)\nfn pass s t r =\n    let mut held = s\n    held = t\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a b r n =\n    let text = to_string n\n    let borrowed = ref text\n    pass (Stash { cell: a, f: \\() -> String.length borrowed }) (Stash { cell: b, f: \\() -> 0 }) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let a3 = Arc.new (Mutex.create [\\() -> 0])\n    let r = reader a2\n    let f = leak a1 a3 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (16, 5),
        ),
        // drop_nested_array
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\nrecord Outer { items: [Stash] }\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Outer -> own (unit -> i64) -> (unit -> i64)\nfn pass _o r = r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    pass (Outer { items: [Stash { cell: a, f: \\() -> String.length borrowed }] }) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (14, 5),
        ),
        // drop_rc_repro
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef pass :: own Stash -> own Arc<Mutex<[unit -> i64]>> -> Arc<Mutex<[unit -> i64]>>\nfn pass _s a = a\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own Arc<Mutex<[unit -> i64]>> -> own i64 -> Arc<Mutex<[unit -> i64]>>\nfn leak a b n =\n    let text = to_string n\n    let borrowed = ref text\n    pass (Stash { cell: a, f: \\() -> String.length borrowed }) b\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let got = leak a1 a2 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    Mutex.with_lock (Arc.get (ref got)) (\\slot -> slot[0] ()) + junk.length\nrun ()",
            (11, 5),
        ),
        // drop_opaque_field
        (
            "record Stash { w: (unit -> i64) -> unit, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let w = s.w; let g = s.f; w g }\n}\ndef writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Stash -> own (unit -> i64) -> (unit -> i64)\nfn pass _s r = r\ndef leak :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak w r n =\n    let text = to_string n\n    let borrowed = ref text\n    pass (Stash { w: w, f: \\() -> String.length borrowed }) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let w = writer a1\n    let r = reader a2\n    let f = leak w r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (15, 5),
        ),
        // owned_drop_opaque
        (
            "record Stash { w: (unit -> i64) -> unit, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let w = s.w; let g = s.f; w g }\n}\ndef writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Stash -> own (unit -> i64) -> (unit -> i64)\nfn pass s r =\n    Owned.drop s\n    r\ndef leak :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak w r n =\n    let text = to_string n\n    let borrowed = ref text\n    pass (Stash { w: w, f: \\() -> String.length borrowed }) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let w = writer a1\n    let r = reader a2\n    let f = leak w r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (17, 5),
        ),
        // drop_consume
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef consume :: own Stash -> unit\nfn consume _s = ()\ndef pass :: own Stash -> own (unit -> i64) -> (unit -> i64)\nfn pass s r =\n    consume s\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    pass (Stash { cell: a, f: \\() -> String.length borrowed }) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (17, 5),
        ),
        // drop_consume_opaque: the same with a `Drop.drop` that calls a function value it holds
        (
            "record Stash { w: (unit -> i64) -> unit, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let w = s.w; let g = s.f; w g }\n}\ndef writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef consume :: own Stash -> unit\nfn consume _s = ()\ndef pass :: own Stash -> own (unit -> i64) -> (unit -> i64)\nfn pass s r =\n    consume s\n    r\ndef leak :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak w r n =\n    let text = to_string n\n    let borrowed = ref text\n    pass (Stash { w: w, f: \\() -> String.length borrowed }) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let w = writer a1\n    let r = reader a2\n    let f = leak w r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (19, 5),
        ),
    ] {
        let error = analyze(source).expect_err(source);
        assert_eq!(error.code, "E1013", "{source}\n{}", error.message);
        assert_eq!(
            error.kind,
            Some("borrow_outlives_block"),
            "{source}\n{}",
            error.message
        );
        assert_eq!(
            at(source, &error),
            (line, column),
            "{source}\n{}",
            error.message
        );
    }
    // With such a `Drop.drop` in the program, a function value that holds a borrow may run it when
    // it is dropped (its environment does not show), so nothing is read through it: the share of
    // `shared` is its borrow, as on the A30 tip, and moving `shared` conflicts with it.
    let source = "record Stash { w: (unit -> i64) -> unit, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let w = s.w; let g = s.f; w g }\n}\nlet text = \"abc\"\nlet borrowed = ref text\nlet shared = Rc.new (\\() -> String.length borrowed)\nlet mine = Rc.share (ref shared)\nlet moved = shared\n(deref (Rc.get (ref mine))) () + Rc.strong_count (ref moved)";
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, "E1014", "{}", error.message);
    assert_eq!(error.kind, Some("borrow_conflict"), "{}", error.message);
    assert_eq!(at(source, &error), (9, 13), "{}", error.message);
    // The controls: the same `pass` while `text` is alive (12), and `HashMap.remove` and a `pass`
    // that drops only a borrow in a program whose `Drop.drop` calls a function value. Last, a
    // dropped value that holds no loan (`ctl_drop_no_loan`, which prints 5): the 7th round gave
    // `pass` its facts, so the result of `wrap` holds no borrow of `key`. A31 (step 4): its
    // `Drop.drop` stores a function with `Array.write`, which A28's eighth review answered by
    // dropping every fact of the program (`E1013` `task_result_retains_borrow` at 12:19); the store
    // facts track that store, so the facts apply again.
    for source in [
        "record Stash { w: (unit -> i64) -> unit, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let w = s.w; let g = s.f; w g }\n}\nlet map = HashMap.insert 1 (\\() -> 42) (HashMap.empty())\nlet map = HashMap.remove 1 map\nHashMap.length (ref map)",
        "record Stash { w: (unit -> i64) -> unit, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let w = s.w; let g = s.f; w g }\n}\ndef pass :: ref i64 -> own (unit -> i64) -> (unit -> i64)\nfn pass _key f = f\ndef wrap :: i64 -> own (unit -> i64) -> (unit -> i64)\nfn wrap key f = pass (ref key) f\nwrap 1 (\\() -> 42) ()",
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Stash -> own (unit -> i64) -> (unit -> i64)\nfn pass s r =\n    let _held = s\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let text = to_string 123456789\n    let borrowed = ref text\n    let f = pass (Stash { cell: a1, f: \\() -> String.length borrowed }) r\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: ref string -> own Stash -> own (unit -> i64) -> (unit -> i64)\nfn pass _key s r =\n    let _held = s\n    r\ndef wrap :: own string -> own Stash -> own (unit -> i64) -> (unit -> i64)\nfn wrap key s r = pass (ref key) s r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let r = reader (Arc.share (ref a))\n    let f = wrap \"abc\" (Stash { cell: Arc.share (ref a), f: \\() -> 5 }) r\n    f ()\nrun ()",
    ] {
        emits(source);
    }
}

/// A28, eighth review: a field assignment after construction puts a parameter's loan into a value
/// with a user `Drop` (directly, through a nested record, an element, an alias, a branch or a
/// loop, a `Maybe` field, a wrapper record), and dropping the value at the end of `pass` runs a
/// `Drop.drop` that stores a function over the loan in a cell that `r` reads. Each `Drop.drop`
/// stores a borrowing function with `Array.write`. The eighth review rejected them by dropping
/// every fact of such a program (`untracked_loan_store`); since A31 tracks the store, the value
/// that a field assignment writes into counts as dropped (A28's `dropped`), which keeps the A30
/// tip's diagnostic at its position. Accepted by the seventh round, they printed garbage at `-O0`
/// (`String.length` of the freed `text`; 12 is right).
#[test]
fn a_field_assignment_into_a_dropped_value_keeps_the_rule_from_before_a28() {
    let at = |source: &str, error: &tsuzuri::diagnostic::Diagnostic| {
        let before = &source[..error.span.start];
        (
            before.matches('\n').count() + 1,
            before.len() - before.rfind('\n').map_or(0, |at| at + 1) + 1,
        )
    };
    let check =
        |error: tsuzuri::diagnostic::Diagnostic, source: &str, line: usize, column: usize| {
            assert_eq!(error.code, "E1013", "{source}\n{}", error.message);
            assert_eq!(
                error.kind,
                Some("borrow_outlives_block"),
                "{source}\n{}",
                error.message
            );
            assert_eq!(
                at(source, &error),
                (line, column),
                "{source}\n{}",
                error.message
            );
        };
    for (source, (line, column)) in [
        // fa_ref
        (
            "record Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, t: ref {r} string }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut held = Stash { cell: a, t: u }\n    held.t = t\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass a u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (15, 5),
        ),
        // fa_param_move
        (
            "record Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, t: ref {r} string }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Stash -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass s t r =\n    let mut held = s\n    held.t = t\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass (Stash { cell: a, t: u }) (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (15, 5),
        ),
        // fa_nested
        (
            "record Inner {r} { t: ref {r} string }\nrecord Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, inner: Inner {r} }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.inner.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut held = Stash { cell: a, inner: Inner { t: u } }\n    held.inner.t = t\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass a u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (16, 5),
        ),
        // fa_whole_inner
        (
            "record Inner {r} { t: ref {r} string }\nrecord Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, inner: Inner {r} }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.inner.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut held = Stash { cell: a, inner: Inner { t: u } }\n    held.inner = Inner { t: t }\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass a u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (16, 5),
        ),
        // fa_wrapper
        (
            "record Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, t: ref {r} string }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrecord Box {r} { s: Stash {r}, n: i64 }\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut bx = Box { s: Stash { cell: a, t: u }, n: 1 }\n    bx.s.t = t\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass a u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (16, 5),
        ),
        // fa_array_elem
        (
            "record Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, t: ref {r} string }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut hs = [Stash { cell: a, t: u }]\n    hs[0].t = t\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass a u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (15, 5),
        ),
        // fa_field_array
        (
            "record Inner {r} { t: ref {r} string }\nrecord Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, items: [Inner {r}] }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.items[0].t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut held = Stash { cell: a, items: [Inner { t: u }] }\n    held.items[0] = Inner { t: t }\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass a u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (16, 5),
        ),
        // fa_alias
        (
            "record Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, t: ref {r} string }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut held = Stash { cell: a, t: u }\n    let p = ref mut held\n    p.t = t\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass a u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (16, 5),
        ),
        // fa_if
        (
            "record Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, t: ref {r} string }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut held = Stash { cell: a, t: u }\n    if String.length u > 0 then held.t = t else ()\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass a u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (15, 5),
        ),
        // fa_while
        (
            "record Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, t: ref {r} string }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut held = Stash { cell: a, t: u }\n    let mut i = 0\n    while i < 1 do\n        held.t = t\n        i = i + 1\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass a u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (18, 5),
        ),
        // fa_maybe_ref_pass
        (
            "record Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, t: Maybe<ref {r} string> }\ninstance Drop<Stash> {\n    fn drop s = match s.t with\n        | Maybe.Some t -> Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot)\n        | Maybe.None -> ()\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a t r =\n    let mut held = Stash { cell: a, t: Maybe.None }\n    held.t = Maybe.Some t\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    pass a (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (17, 5),
        ),
        // fa_opaque
        (
            "record Stash {r} { w: (unit -> i64) -> unit, t: ref {r} string }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.t; let w = s.w; w (\\() -> String.length t) }\n}\ndef writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own ((unit -> i64) -> unit) -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass w u t r =\n    let mut held = Stash { w: w, t: u }\n    held.t = t\n    r\ndef leak :: own ((unit -> i64) -> unit) -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak w u r n =\n    let text = to_string n\n    pass w u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = writer (Arc.share (ref a))\n    let r = reader (Arc.share (ref a))\n    let f = leak w (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            (17, 5),
        ),
    ] {
        check(
            analyze_with_edition(source, Edition::E2027).expect_err(source),
            source,
            line,
            column,
        );
    }
    // xmod_fa: the `Drop` record in a module of its own.
    let lib = "record Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, t: ref {r} string }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}";
    let main = "def reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut held = Lib.Stash { cell: a, t: u }\n    held.t = t\n    r\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> ref string -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a u r n =\n    let text = to_string n\n    pass a u (ref text) r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 (ref keep) r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()";
    let error = analyze_modules_with_edition(&[("Lib.tz", lib), ("Main.tz", main)], Edition::E2027)
        .expect_err(main);
    check(error, main, 11, 5);
    // The control: the same `pass` while `text` is alive (12).
    let control = "record Stash {r} { cell: Arc<Mutex<[unit -> i64]>>, t: ref {r} string }\ninstance Drop<Stash> {\n    fn drop s = { let t = s.t; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length t) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef pass :: own Arc<Mutex<[unit -> i64]>> -> ref string -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn pass a u t r =\n    let mut held = Stash { cell: a, t: u }\n    held.t = t\n    r\ndef run :: unit -> i64\nfn run u =\n    let keep = \"keep\"\n    let text = to_string 123456789\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = pass a1 (ref keep) (ref text) r\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()";
    let module = analyze_with_edition(control, Edition::E2027)
        .unwrap_or_else(|error| panic!("{control}\n{}: {}", error.code, error.message));
    llvm::emit_target(&module, llvm::Entry::Library, false).unwrap();
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
fn emits_in_both(source: &str) {
    emits(source);
    let module = analyze_with_edition(source, Edition::E2027)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit_target(&module, llvm::Entry::Library, false).unwrap();
    assert_eq!(
        ir,
        llvm::emit_target(&module, llvm::Entry::Library, false).unwrap(),
        "{source}"
    );
}

/// A31: what `Array.write` stores in an array of shared pointers stays in the array's owner, so
/// `Rc.get (ref arr[0])` keeps the borrow of `text` (`h5`), and a tzroutine that captures the
/// array captures that borrow (`tz_capture`, which reads `text` after `start` has freed it on
/// main). The diagnostics are those of `arr[0] = ...` in place of the write.
#[test]
fn a_shared_pointer_written_in_place_keeps_its_borrow() {
    rejects_at(
        "def leak :: i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    let borrowed = ref text\n    let mut arr = [Rc.new (\\() -> 0)]\n    Array.write 0 (Rc.new (\\() -> String.length borrowed)) (ref mut arr)\n    deref (Rc.get (ref arr[0]))\nlet f = leak 123456789\nf ()",
        "E1013",
        "borrow_outlives_block",
        (7, 5),
    );
    rejects_at(
        "def wait_for :: ref Arc<Atomic<i64>> -> i64 -> unit\nfn wait_for counter target =\n    while Atomic.load (Arc.get counter) < target do\n        Routine.yield ()\ndef start :: own i64 -> ref Arc<Atomic<i64>> -> ref Arc<Atomic<i64>> -> unit\nfn start n result done =\n    let text = to_string n\n    let borrowed = ref text\n    let mut arr = [Arc.new (\\() -> 0)]\n    Array.write 0 (Arc.new (\\() -> String.length borrowed)) (ref mut arr)\n    let mine = Arc.share result\n    let finished = Arc.share done\n    tzroutine {\n        let s = Arc.share (ref arr[0])\n        let _sum = Atomic.fetch_add (Arc.get (ref mine)) ((deref (Arc.get (ref s))) ())\n        let _count = Atomic.fetch_add (Arc.get (ref finished)) 1i64\n    }\nlet result = Arc.new (Atomic.create 0i64)\nlet done = Arc.new (Atomic.create 0i64)\nstart 123456789 (ref result) (ref done)\nwait_for (ref done) 1\nAtomic.load (Arc.get (ref result))",
        "E1013",
        "task_capture_retains_borrow",
        (13, 5),
    );
}

/// A31: a lock callback that writes a borrow of `text` into the `ref mut` that the lock lends
/// stores it in the lock's owner `m`, although `m` is not `let mut`: `stash`'s result, read out of
/// `m`, holds the borrow of `text`, and `leak` cannot return it (main accepts these, because
/// `closed_returns` takes `m` for a value that holds no borrow, and `f ()` reads the freed
/// `text`). The diagnostic is the one that `let mut m` gets on main, at the call of `stash`.
#[test]
fn a_lock_callback_write_stays_in_the_lock() {
    let leak = "def leak :: own i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    stash (ref text)\nlet f = leak 123456789\nf ()";
    for (lock, write, into_inner) in [
        (
            "Mutex.create",
            "Mutex.with_lock (ref m)",
            "Mutex.into_inner",
        ),
        (
            "RwMutex.create",
            "RwMutex.with_write (ref m)",
            "RwMutex.into_inner",
        ),
    ] {
        // m_lock, rw_write
        rejects_at(
            &format!(
                "def stash :: ref string -> (unit -> i64)\nfn stash text =\n    let m = {lock} [\\() -> 0]\n    {write} (\\slot -> Array.write 0 (\\() -> String.length text) slot)\n    let arr = {into_inner} m\n    arr[0]\n{leak}"
            ),
            "E1013",
            "borrow_outlives_block",
            (10, 5),
        );
        // m_lock_return, rw_write_return
        let cell = lock.trim_end_matches(".create");
        rejects_at(
            &format!(
                "def stash :: ref string -> {cell}<[(unit -> i64)]>\nfn stash text =\n    let m = {lock} [\\() -> 0]\n    {write} (\\slot -> Array.write 0 (\\() -> String.length text) slot)\n    m\ndef leak :: own i64 -> {cell}<[(unit -> i64)]>\nfn leak n =\n    let text = to_string n\n    stash (ref text)\nlet arr = {into_inner} (leak 123456789)\narr[0] ()"
            ),
            "E1013",
            "borrow_outlives_block",
            (9, 5),
        );
    }
    // cond_lock
    rejects_at(
        &format!(
            "def stash :: ref string -> (unit -> i64)\nfn stash text =\n    let m = Mutex.create [\\() -> 0]\n    let c = Cond.create ()\n    Cond.wait_with (ref c) (ref m) (\\_ -> true) (\\slot -> Array.write 0 (\\() -> String.length text) slot)\n    let arr = Mutex.into_inner m\n    arr[0]\n{leak}"
        ),
        "E1013",
        "borrow_outlives_block",
        (11, 5),
    );
}

/// A31: a lock callback that only reads keeps nothing in the lock (main accepts these; the
/// values are checked by the `in_place_writes` suite of `tests/features.mjs`): the borrowed `text`
/// moves after the lock is used, by a lambda or by a partial application, and a function whose
/// `let m` is only read through the lock returns what `m` held, which borrows nothing.
#[test]
fn a_lock_callback_that_only_reads_stores_nothing() {
    for source in [
        "let m = Mutex.create [\\() -> 3]\nlet text = to_string 123456789\nlet borrowed = ref text\nlet n = Mutex.with_lock (ref m) (\\slot -> slot[0] () + String.length borrowed)\nlet moved = text\nlet arr = Mutex.into_inner m\narr[0] () + n + moved.length",
        "def peek :: ref string -> ref mut [(unit -> i64)] -> i64\nfn peek t a = a[0] () + String.length t\nlet text = to_string 123456789\nlet m = Mutex.create [\\() -> 3]\nlet n = Mutex.with_lock (ref m) (peek (ref text))\nlet moved = text\nlet arr = Mutex.into_inner m\narr[0] () + n + moved.length",
        "def make :: ref string -> (unit -> i64)\nfn make text =\n    let m = Mutex.create [\\() -> 7]\n    let n = Mutex.with_lock (ref m) (\\slot -> String.length text)\n    let arr = Mutex.into_inner m\n    let f = arr[0]\n    f\ndef leak :: own i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    make (ref text)\nlet f = leak 123456789\nf ()",
    ] {
        emits_in_both(source);
    }
}

/// A31: shares of a cell are not read through, so a write into the cell through the original
/// after the share was made keeps the share borrowing the owner (`N_arc_cell_write_alias`, which a
/// read-through would accept while the cell holds a function that borrows the freed `text`); a
/// `SyncMap` that stores a borrowing function keeps it, with no diagnostic inside std.
#[test]
fn a_cell_written_after_it_was_shared_keeps_the_rule() {
    rejects_at(
        "def leak :: own i64 -> Arc<Mutex<[unit -> i64]>>\nfn leak n =\n    let text = to_string n\n    let borrowed = ref text\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a2 = Arc.share (ref a)\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 (\\() -> String.length borrowed) slot)\n    a2\nlet a = leak 123456789\nMutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())",
        "E1013",
        "borrow_outlives_block",
        (8, 5),
    );
    rejects_at(
        "def leak :: own i64 -> (unit -> i64)\nfn leak n =\n    let text = to_string n\n    let borrowed = ref text\n    let sm = SyncMap.create ()\n    SyncMap.store (ref sm) 0 (\\() -> String.length borrowed)\n    let key = 0\n    match SyncMap.load (ref sm) (ref key) with\n    | Maybe.Some f -> f\n    | Maybe.None -> \\() -> 0\nlet f = leak 123456789\nf ()",
        "E1013",
        "borrow_outlives_block",
        (8, 5),
    );
}

/// A31: a lock callback that stores a borrow of the locked value in a cell of its own (`let m`)
/// and returns what it reads back holds that borrow, although `m` is not `let mut`: the check of
/// the lifted lambda returns the borrow, so the closed result that `closed_returns` proved for it is
/// revoked, and the callback's result is not proven free of borrowed environments (F10). The same
/// callback storing a function that borrows nothing stays accepted.
#[test]
fn a_lock_callback_that_returns_what_it_stored_in_its_own_cell_keeps_the_rule() {
    let source = "def make :: ref Mutex<[(unit -> i64)]> -> (unit -> i64)\nfn make outer =\n    Mutex.with_lock outer (\\slot -> { let view = ref slot[0]; let m = Mutex.create [\\() -> 0]; Mutex.with_lock (ref m) (\\s -> Array.write 0 (\\() -> (deref view) () + 1) s); let a = Mutex.into_inner m; a[0] })\ndef run :: own i64 -> i64\nfn run n =\n    let outer = Mutex.create [\\() -> n]\n    let f = make (ref outer)\n    Mutex.with_lock (ref outer) (\\slot -> Array.write 0 (\\() -> 100) slot)\n    f ()\nrun 5";
    rejects_at(source, "E1013", "mutex_result_retains_borrow", (3, 27));
    let control = source.replace("(\\() -> (deref view) () + 1)", "(\\() -> 7)");
    assert_ne!(control, source);
    emits_in_both(&control);
}

/// A31: the implicit drop of `_s` at the end of `leak` runs `Drop.drop`, which writes the function
/// that borrows `text` into the cell that `a` shares with `run`'s `a2`, and `r` reads it after
/// `leak` has freed `text` (`drop_inleak`; main and A28 accept it). The write stores a borrow of a
/// local of `leak` through a value that the caller shares, so it is rejected where the dropped
/// value is declared.
#[test]
fn an_implicit_drop_cannot_store_a_local_borrow_in_a_shared_cell() {
    rejects_at(
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    let _s = Stash { cell: a, f: \\() -> String.length borrowed }\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
        "E1013",
        "store_local_borrow_through_reference",
        (11, 9),
    );
}

#[test]
fn shared_types_have_reserved_names_and_no_abi() {
    for source in [
        "record Rc<'a> { value: 'a }\n0",
        "union Arc = Arc of i64\n0",
        "type Rc = i64\n0",
    ] {
        let message = rejects(source, "E1001");
        assert!(message.contains("reserved"), "{message}");
    }
    for source in [
        "let shared: Rc = Rc.new 5\n0",
        "let shared: Arc<i64, i64> = Arc.new 5\n0",
    ] {
        let message = rejects(source, "E1004");
        assert!(
            message.contains("expects exactly one value type"),
            "{message}"
        );
    }
    rejects("export def bad :: Rc<i64>\nfn bad = Rc.new 5", "E1008");
    rejects("extern def host :: Arc<i64> -> i64\n0", "E1008");
    rejects("const bad: Rc<i64> = Rc.new 5\n0", "E1026");
    rejects("record Loop { next: Rc<Loop> }\n0", "E1010");
    let message = rejects("record Bad { next: Vec<Bad> }\n0", "E1010");
    assert!(message.contains("pass through a union"), "{message}");
}

#[test]
fn only_arc_counts_atomically() {
    for ir in emits(ALL_OPERATIONS) {
        for atomic in ["atomicrmw", "cmpxchg", "fence", "load atomic"] {
            assert!(!ir.contains(atomic), "Rc uses {atomic}");
        }
    }
    let arc = ALL_OPERATIONS.replace("Rc.", "Arc.");
    for ir in emits(&arc) {
        assert!(ir.contains("atomicrmw add ptr"), "{ir}");
        assert!(ir.contains(", i64 1 monotonic, align 8"));
        assert!(ir.contains("atomicrmw sub ptr"));
        assert!(ir.contains(", i64 1 release, align 8"));
        assert!(ir.contains("fence acquire"));
        assert!(ir.contains("cmpxchg ptr"));
        assert!(ir.contains("acquire monotonic"));
        assert!(ir.contains("load atomic i64"));
        assert_eq!(ir.matches("declare void @llvm.trap()").count(), 1);
    }
    for ir in emits("let values = [1, 2, 3]\nArray.sum (ref values)") {
        assert!(!ir.contains("tz.shared") && !ir.contains("atomicrmw"));
    }
}

#[test]
fn recursive_shared_values_drop_through_the_pending_list() {
    let tree = "record Node { value: i64, children: Vec<Rc<Node>> }\n\
        def leaf :: i64 -> Rc<Node>\nfn leaf value = Rc.new (Node { value: value, children: Vec.empty() })\n\
        let bottom = leaf 4\n\
        let shared = Rc.share (ref bottom)\n\
        let top = Rc.new (Node { value: 1, children: Vec.push bottom (Vec.push shared (Vec.empty())) })\n\
        Vec.length (ref (Rc.get (ref top)).children)";
    for ir in emits(tree) {
        assert!(ir.contains(
            "define internal void @\"tz.shared.drop.rc.Main.Node\"(ptr %node, ptr %pending)"
        ));
        assert!(ir.contains("call void @tz.rec.drop(ptr"));
        assert!(ir.contains("call void @tz.rec.enqueue(ptr"));
    }
    let list = "union List = Nil | Cons of (i64 * Arc<List>)\nlet list = Arc.new (Cons (1, Arc.new (Cons (2, Arc.new Nil))))\nArc.strong_count (ref list)";
    for ir in emits(list) {
        assert!(ir.contains("@\"tz.shared.drop.arc.Main.List\""));
    }
    // A shared value that holds no recursive type drops in place, without the drop loop.
    for ir in emits("let shared = Rc.new [\"a\", \"b\"]\nRc.strong_count (ref shared)") {
        assert!(!ir.contains("tz.shared.drop") && !ir.contains("@tz.rec."));
    }
}

#[test]
fn tools_keep_shared_types_and_new_members() {
    let source = "def read :: ref Rc<string> -> i64\nfn read shared =\n    let copy = Rc.share shared\n    (Rc.get (ref copy)).length\nlet value = Rc.new \"text\"\nlet strong = Arc.new 1\nlet weak: Arc.Weak<i64> = Arc.downgrade (ref strong)\nread (ref value) + Arc.weak_count (ref strong)";
    let formatted = format_source("Main.tz", source, SourceKind::Code).unwrap();
    assert!(formatted.formatted.contains("Rc.new \"text\""));
    assert!(formatted.formatted.contains("Arc.Weak<i64>"));
    assert_eq!(
        ast_fingerprint(parser::parse(source).unwrap()),
        ast_fingerprint(parser::parse(&formatted.formatted).unwrap())
    );
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", source)]).unwrap();
    let copy = source.find("copy =").unwrap();
    let entry = index.at(0, copy).unwrap();
    assert!(entry.detail.contains("Rc<string>"), "{}", entry.detail);
}

/// Every quoted named type that `ir` uses has a definition, which clang needs to size it.
fn defines_named_types(ir: &str) {
    for (start, _) in ir.match_indices("%\"tz.") {
        let quoted = &ir[start + 1..];
        let name = &quoted[..quoted[1..].find('"').unwrap() + 2];
        assert!(
            ir.contains(&format!("{name} = type ")),
            "{name} has no definition"
        );
    }
}

#[test]
fn named_types_reached_only_through_shared_pointers_are_defined() {
    for source in [
        "let items: Vec<Rc<Maybe<string>>> = Vec.empty()\nVec.length (ref items)",
        "record Pair<'a> { left: 'a, right: 'a }\nlet pair: Maybe<Rc<Pair<i64>>> = Maybe.Some (Rc.new (Pair { left: 3, right: 5 }))\nmatch pair with\n| Maybe.Some shared -> Rc.strong_count (ref shared)\n| Maybe.None -> 0",
        "let nested: Maybe<Arc<(i64 * Maybe<Arc<i64>>)>> = Maybe.Some (Arc.new (7, Maybe.Some (Arc.new 11)))\nmatch nested with\n| Maybe.Some outer -> Arc.strong_count (ref outer)\n| Maybe.None -> 0",
        "let weak: Vec<Arc.Weak<Maybe<string>>> = Vec.empty()\nVec.length (ref weak)",
    ] {
        for ir in emits(source) {
            defines_named_types(&ir);
        }
    }
}

#[test]
fn weak_back_links_are_not_polymorphic_recursion() {
    for source in [
        "record TreeNode { value: i64, parent: Maybe<Rc.Weak<TreeNode>>, children: Vec<Rc<TreeNode>> }\nlet found: Maybe<Rc<TreeNode>> = Maybe.None\nmatch found with\n| Maybe.Some _node -> 1\n| Maybe.None -> 0",
        "record TreeNode { value: i64, parent: Maybe<Rc.Weak<TreeNode>>, children: Vec<Rc<TreeNode>> }\nlet root = Rc.new (TreeNode { value: 7, parent: Maybe.None, children: Vec.empty() })\nlet child = TreeNode { value: 2, parent: Maybe.Some (Rc.downgrade (ref root)), children: Vec.empty() }\nmatch child.parent with\n| Maybe.Some weak -> match Rc.upgrade weak with\n    | Maybe.Some parent -> (Rc.get (ref parent)).value\n    | Maybe.None -> 0\n| Maybe.None -> 0",
        "record Node { link: Maybe<Rc<Node>> }\nlet first = Rc.new (Node { link: Maybe.None })\nlet weak = Maybe.Some (Rc.downgrade (ref first))\nmatch weak with\n| Maybe.Some _weak -> 1\n| Maybe.None -> 0",
        "record Node { link: Maybe<Arc.Weak<Node>>, next: Maybe<Arc<Node>> }\nlet first = Arc.new (Node { link: Maybe.None, next: Maybe.None })\nlet second = Arc.new (Node { link: Maybe.Some (Arc.downgrade (ref first)), next: Maybe.Some (Arc.share (ref first)) })\nArc.strong_count (ref first) + Arc.strong_count (ref second)",
        "record Node<'t> { value: 't, link: Maybe<Rc.Weak<Node<'t>>>, next: Maybe<Rc<Node<'t>>> }\nlet first = Rc.new (Node { value: 1, link: Maybe.None, next: Maybe.None })\nlet second = Rc.new (Node { value: 2, link: Maybe.Some (Rc.downgrade (ref first)), next: Maybe.Some (Rc.share (ref first)) })\nRc.strong_count (ref first) + (Rc.get (ref second)).value",
    ] {
        emits(source);
    }
}

const HANDLE: &str = "extern type Counter\n\
extern \"c10_counter_new\" def counter_new :: i64 -> Counter\n\
extern \"c10_counter_peek\" def counter_peek :: ref Counter -> i64 -> i64\n";

const SHAPE: &str = "class Shape<'a> {\n    def area :: ref 'a -> i64\n}\nrecord Square { side: i64 }\ninstance Shape<Square> {\n    fn area s = s.side * s.side\n}\n";

#[test]
fn arc_does_not_share_host_handles_between_tzroutines() {
    let sharing = "shares an extern handle, a dyn value that is not Copy, or an Owned.Function through an Arc";
    for body in [
        "let shared = Arc.new (counter_new 1)\ntzroutine {\n    let _value = counter_peek (Arc.get (ref shared)) 0\n}\n0",
        "let shared = Arc.new (counter_new 1)\nlet weak = Arc.downgrade (ref shared)\ntzroutine {\n    let _count = match Arc.upgrade (ref weak) with | Maybe.Some strong -> Arc.strong_count (ref strong) | Maybe.None -> 0\n}\n0",
        "let shared = Arc.new (Arc.new (counter_new 1))\ntzroutine {\n    let _count = Arc.strong_count (ref shared)\n}\n0",
        "record Holder { counter: Arc<Counter> }\nlet holder = Holder { counter: Arc.new (counter_new 1) }\ntzroutine {\n    let _count = Arc.strong_count (ref holder.counter)\n}\n0",
        "union Counters = Done | More of (Arc<Counter> * Counters)\nlet counters = More (Arc.new (counter_new 1), Done)\ntzroutine {\n    let _more = match counters with | More (_, _) -> 1 | Done -> 0\n}\n0",
        "def peek :: Arc<Counter> -> unit\nfn peek shared =\n    tzroutine {\n        let _value = counter_peek (Arc.get (ref shared)) 0\n    }\nlet shared = Arc.new (counter_new 1)\npeek (Arc.share (ref shared))\ncounter_peek (Arc.get (ref shared)) 0",
        "let shared = Arc.new (counter_new 1)\nlet keep = Owned.function (\\() -> counter_peek (Arc.get (ref shared)) 0)\nOwned.call (ref keep) ()",
        "let counter = counter_new 1\nlet shared = Arc.new (Owned.function (\\() -> counter_peek (ref counter) 0))\ntzroutine {\n    let _value = Owned.call (Arc.get (ref shared)) ()\n}\n0",
    ] {
        let source = format!("{HANDLE}{body}");
        let message = rejects(&source, "E1013");
        assert!(message.contains(sharing), "{message}");
    }
    let dynamic = format!(
        "{SHAPE}let shape: dyn (Shape, Send) = Dyn.of (Square {{ side: 3 }})\nlet shared = Arc.new shape\ntzroutine {{\n    let _area = Shape.area (Arc.get (ref shared))\n}}\n0"
    );
    let message = rejects(&dynamic, "E1013");
    assert!(message.contains(sharing), "{message}");
    for source in [
        format!(
            "{HANDLE}let shared = Arc.new (counter_new 1)\nlet read = \\() -> counter_peek (Arc.get (ref shared)) 0\nread ()"
        ),
        format!(
            "{SHAPE}let shape: dyn (Shape, Send) = Dyn.of (Square {{ side: 3 }})\nlet shared = Arc.new shape\nlet read = \\() -> Shape.area (Arc.get (ref shared))\nread ()"
        ),
    ] {
        let message = rejects(&source, "E1005");
        assert!(
            message.contains("function values may move to other tzroutines or threads")
                && message.contains(sharing),
            "{message}"
        );
    }
    // One tzroutine may share a handle with itself, and a handle may move to one other tzroutine.
    for source in [
        format!(
            "{HANDLE}let shared = Arc.new (counter_new 1)\nlet other = Arc.share (ref shared)\ncounter_peek (Arc.get (ref other)) 0 + Arc.strong_count (ref shared)"
        ),
        format!(
            "{HANDLE}let counter = counter_new 1\ntzroutine {{\n    let _value = counter_peek (ref counter) 0\n}}\n0"
        ),
        format!(
            "{SHAPE}let shape: dyn (Shape, Copy) = Dyn.of (Square {{ side: 3 }})\nlet shared = Arc.new shape\nlet read = \\() -> Shape.area (Arc.get (ref shared))\nread ()"
        ),
        "def apply :: Arc<i64 -> i64> -> i64\nfn apply shared =\n    let function = deref (Arc.get (ref shared))\n    function 41\nlet shared = Arc.new (\\value -> value + 1)\ntzroutine {\n    let _value = apply (Arc.share (ref shared))\n}\n0".to_owned(),
    ] {
        emits(&source);
    }
}
