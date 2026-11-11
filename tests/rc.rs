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

/// A31, review round 1 (findings 1 and 16): a function value that is not called from the local
/// that holds it (an element or a field read through a `ref` parameter, an alias of a function
/// parameter, an element that `Array.iter` hands a lambda) stores what it is given where its
/// environment reaches: the type of the value does not show the cell that the environment
/// borrows, so a place whose type hides an environment is a place to store in when some
/// function value of the module stores into what its environment reaches. Each program lets a
/// borrow of a block's local outlive the block in the cell (the error is at the block), moves the
/// borrowed value while the cell is alive (at the move), or passes a borrow of a callee's local
/// (at the call); main and the tip before this round accept them and read freed memory. The
/// controls keep the borrowed value alive and stay accepted.
#[test]
fn a_function_value_behind_a_reference_or_an_alias_keeps_what_its_call_stores() {
    for (source, code, kind, at) in [
        // k01_array_fn_ref
        (
            "def writers :: own Arc<Mutex<[unit -> i64]>> -> [(unit -> i64) -> unit]\nfn writers a = [\\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)]\ndef call_it :: ref [(unit -> i64) -> unit] -> own (unit -> i64) -> unit\nfn call_it ws f = ws[0] f\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let ws = writers (Arc.share (ref a))\n    { let text0 = to_string n; let text = ref text0; call_it (ref ws) (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // k03_record_fn_ref
        (
            "record Holder { w: (unit -> i64) -> unit, k: i64 }\ndef holder :: own Arc<Mutex<[unit -> i64]>> -> Holder\nfn holder a = Holder { w: \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot), k: 1 }\ndef call_it :: ref Holder -> own (unit -> i64) -> unit\nfn call_it h f = h.w f\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let h = holder (Arc.share (ref a))\n    { let text0 = to_string n; let text = ref text0; call_it (ref h) (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (10, 5),
        ),
        // t04_param_alias
        (
            "def writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef apply2 :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> unit\nfn apply2 f x =\n    let g = f\n    g x\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = writer (Arc.share (ref a))\n    { let text0 = to_string n; let text = ref text0; apply2 w (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (11, 5),
        ),
        // t05_param_alias_if
        (
            "def writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef apply2 :: own ((unit -> i64) -> unit) -> own ((unit -> i64) -> unit) -> own (unit -> i64) -> unit\nfn apply2 f h x = (if true then f else h) x\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = writer (Arc.share (ref a))\n    { let text0 = to_string n; let text = ref text0; apply2 w w (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // t06_param_in_array
        (
            "def writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef apply2 :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> unit\nfn apply2 f x =\n    let fs = [f]\n    fs[0] x\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = writer (Arc.share (ref a))\n    { let text0 = to_string n; let text = ref text0; apply2 w (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (11, 5),
        ),
        // op7
        (
            "record Handler { w: (unit -> i64) -> unit }\ndef run :: own Handler -> own (unit -> i64) -> unit\nfn run h x = h.w x\ndef leak :: ref Mutex<[unit -> i64]> -> i64 -> unit\nfn leak rm n =\n    let text = to_string n\n    let borrowed = ref text\n    run (Handler { w: \\x -> Mutex.with_lock rm (\\s -> Array.write 0 x s) }) (\\() -> String.length borrowed)\nlet m = Mutex.create [\\() -> 0]\nleak (ref m) 123456789\nlet junk = [to_string 987654321, to_string 111111111]\nMutex.with_lock (ref m) (\\slot -> slot[0] ()) + junk.length",
            "E1013",
            "store_local_borrow_through_reference",
            (8, 5),
        ),
        // op8
        (
            "def run :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> unit\nfn run w x =\n    let g = w\n    g x\ndef leak :: ref Mutex<[unit -> i64]> -> i64 -> unit\nfn leak rm n =\n    let text = to_string n\n    let borrowed = ref text\n    run (\\x -> Mutex.with_lock rm (\\s -> Array.write 0 x s)) (\\() -> String.length borrowed)\nlet m = Mutex.create [\\() -> 0]\nleak (ref m) 123456789\nlet junk = [to_string 987654321, to_string 111111111]\nMutex.with_lock (ref m) (\\slot -> slot[0] ()) + junk.length",
            "E1013",
            "store_local_borrow_through_reference",
            (9, 5),
        ),
        // op1
        (
            "record Handler { w: (unit -> i64) -> unit }\ndef run :: own Handler -> own (unit -> i64) -> unit\nfn run h x = h.w x\nlet m = Mutex.create [\\() -> 0]\nlet rm = ref m\nlet text = to_string 123456789\nlet borrowed = ref text\nrun (Handler { w: (\\x -> Mutex.with_lock rm (\\s -> Array.write 0 x s)) }) (\\() -> String.length borrowed)\nlet moved = text\nMutex.with_lock (ref m) (\\slot -> slot[0] ()) * 100 + moved.length",
            "E1014",
            "borrow_conflict",
            (9, 13),
        ),
        // op2
        (
            "def run :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> unit\nfn run w x =\n    let g = w\n    g x\nlet m = Mutex.create [\\() -> 0]\nlet rm = ref m\nlet text = to_string 123456789\nlet borrowed = ref text\nrun (\\x -> Mutex.with_lock rm (\\s -> Array.write 0 x s)) (\\() -> String.length borrowed)\nlet moved = text\nMutex.with_lock (ref m) (\\slot -> slot[0] ()) * 100 + moved.length",
            "E1014",
            "borrow_conflict",
            (10, 13),
        ),
        // op3
        (
            "def run :: own [(unit -> i64) -> unit] -> own (unit -> i64) -> unit\nfn run ws x = ws[0] x\nlet m = Mutex.create [\\() -> 0]\nlet rm = ref m\nlet text = to_string 123456789\nlet borrowed = ref text\nrun [(\\x -> Mutex.with_lock rm (\\s -> Array.write 0 x s))] (\\() -> String.length borrowed)\nlet moved = text\nMutex.with_lock (ref m) (\\slot -> slot[0] ()) * 100 + moved.length",
            "E1014",
            "borrow_conflict",
            (8, 13),
        ),
        // n01_partial_lock
        (
            "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let lk = Mutex.with_lock (ref m)\n    { let text0 = to_string n; let text = ref text0; lk (\\slot -> Array.write 0 (\\() -> String.length text) slot) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (5, 5),
        ),
    ] {
        rejects_at(source, code, kind, at);
    }
    for source in [
        // k05_record_fn_ctl
        "record Holder { w: (unit -> i64) -> unit, k: i64 }\ndef holder :: own Arc<Mutex<[unit -> i64]>> -> Holder\nfn holder a = Holder { w: \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot), k: 1 }\ndef call_it :: ref Holder -> own (unit -> i64) -> unit\nfn call_it h f = h.w f\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let text = ref text0\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let h = holder (Arc.share (ref a))\n    call_it (ref h) (\\() -> String.length text)\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
        // k06_mutex_ref_record
        "record Holder {r} { w: (unit -> i64) -> unit, m: ref {r} Mutex<[unit -> i64]> }\ndef call_it :: ref Holder -> own (unit -> i64) -> unit\nfn call_it h f = h.w f\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let h = Holder { w: \\g -> (), m: mr }\n    { let text0 = to_string n; let text = ref text0; call_it (ref h) (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
    ] {
        emits_in_both(source);
    }
    // A function value applied in stages binds its first arguments: the lock that the first call
    // gives `h.w2` is where the second call stores.
    rejects_at(
        "def put :: ref Mutex<[unit -> i64]> -> own (unit -> i64) -> unit\nfn put m g = Mutex.with_lock m (\\slot -> Array.write 0 g slot)\nrecord Handler { w2: ref Mutex<[unit -> i64]> -> (unit -> i64) -> unit }\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let h = Handler { w2: put }\n    let g = h.w2 (ref m)\n    { let text0 = to_string n; let text = ref text0; g (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
        "E1013",
        "assignment_extends_borrow",
        (9, 5),
    );
    emits_in_both(
        "def put :: ref Mutex<[unit -> i64]> -> own (unit -> i64) -> unit\nfn put m g = Mutex.with_lock m (\\slot -> Array.write 0 g slot)\nrecord Handler { w2: ref Mutex<[unit -> i64]> -> (unit -> i64) -> unit }\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let text = ref text0\n    let m = Mutex.create [\\() -> 0]\n    let h = Handler { w2: put }\n    let g = h.w2 (ref m)\n    g (\\() -> String.length text)\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 123456789",
    );
    // `Array.iter` lends each element to its lambda: edition 2027 took the element for a read
    // parameter, whose environment was not reached.
    let iter = "def writers :: own Arc<Mutex<[unit -> i64]>> -> [(unit -> i64) -> unit]\nfn writers a = [\\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)]\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let ws = writers (Arc.share (ref a))\n    { let text0 = to_string n; let text = ref text0; Array.iter (\\w -> w (\\() -> String.length text)) (ref ws) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789";
    for edition in [Edition::E2026, Edition::E2027] {
        let error = analyze_with_edition(iter, edition).expect_err(iter);
        assert_eq!(
            (error.code, error.kind, position(iter, error.span.start)),
            ("E1013", Some("assignment_extends_borrow"), (7, 5)),
            "edition {edition}: {}",
            error.message
        );
    }
}

/// A31, review round 1 (findings 2 and 13): a function value that owns the cell that it writes
/// (a lambda or a partial application that captured an `Arc<Mutex<...>>` by value, an
/// `Owned.Function` or an `Rc` of one) holds no loan, and a copy of it shares the cell. In a
/// module that makes such a value, a call of a function value that the check does not see keeps
/// what it is given in every local whose type hides an environment or reaches a cell, and in the
/// values bound afterwards from what was evaluated before. `Owned.call` runs the owned function:
/// a named function behind it applies its facts.
#[test]
fn a_function_value_that_owns_its_cell_keeps_what_its_call_stores() {
    for (source, code, kind, at) in [
        // d01_closure_owns_arc
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let first = { let text0 = to_string n; let text = ref text0; w (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + w (\\() -> 1) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (6, 17),
        ),
        // d04_partial_owns_arc
        (
            "def swap_in :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64) -> i64\nfn swap_in a f = Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let w = swap_in (Arc.new (Mutex.create [\\() -> 0]))\n    let first = { let text0 = to_string n; let text = ref text0; w (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + w (\\() -> 1) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (6, 17),
        ),
        // d06_apply_helper
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef apply :: ((unit -> i64) -> i64) -> (unit -> i64) -> i64\nfn apply w f = w f\ndef run :: i64 -> i64\nfn run n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let first = { let text0 = to_string n; let text = ref text0; apply w (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + apply w (\\() -> 1) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (8, 17),
        ),
        // d07_pipe
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let first = { let text0 = to_string n; let text = ref text0; (\\() -> String.length text) |> w }\n    let other = [to_string 777777, to_string 88888888]\n    first + ((\\() -> 1) |> w) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (6, 17),
        ),
        // e04_owned_fn_owns_arc
        (
            "def run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = Owned.function (\\x -> match x with\n        | Maybe.Some g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { Array.write 0 g slot; 0 })\n        | Maybe.None -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()))\n    let first = { let text0 = to_string n; let text = ref text0; Owned.call (ref w) (Maybe.Some (\\() -> String.length text)) }\n    let other = [to_string 777777, to_string 88888888]\n    first + Owned.call (ref w) Maybe.None + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (7, 17),
        ),
        // e05_closure_copy
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let w2 = w\n    let first = { let text0 = to_string n; let text = ref text0; w (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + w2 (\\() -> 1) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (7, 17),
        ),
        // e06_rc_closure
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let w = Rc.new (mk (Arc.new (Mutex.create [\\() -> 0])))\n    let first = { let text0 = to_string n; let text = ref text0; (deref (Rc.get (ref w))) (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + (deref (Rc.get (ref w))) (\\() -> 1) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (6, 17),
        ),
        // x02_callee_local_owned
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef leak :: ((unit -> i64) -> i64) -> i64 -> i64\nfn leak w n =\n    let text0 = to_string n\n    let text = ref text0\n    w (\\() -> String.length text)\ndef run :: i64 -> i64\nfn run n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let first = leak w n\n    let other = [to_string 777777, to_string 88888888]\n    first + w (\\() -> 1) + other.length * 0\nrun 123456789",
            "E1013",
            "store_local_borrow_through_reference",
            (11, 17),
        ),
        // oc3
        (
            "record Job {r} { t: ref mut {r} [(unit -> i64)..], f: unit -> i64 }\ndef store {r} :: own Job {r} -> unit\nfn store job = Array.write 0 job.f job.t\ndef run :: i64 -> (unit -> i64)\nfn run n =\n    let mut arr = [\\() -> 0]\n    let w = Owned.function store\n    let text = to_string n\n    let borrowed = ref text\n    Owned.call (ref w) (Job { t: ref mut arr, f: \\() -> String.length borrowed })\n    arr[0]\nlet f = run 123456789\nlet junk = [to_string 987654321, to_string 111111111]\nf () + junk.length",
            "E1013",
            "borrow_outlives_block",
            (11, 5),
        ),
        // oc2
        (
            "record Job { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ndef store :: own Job -> unit\nfn store job = { let g = job.f; Mutex.with_lock (Arc.get (ref job.cell)) (\\slot -> Array.write 0 g slot) }\ndef run :: i64 -> Arc<Mutex<[unit -> i64]>>\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = Owned.function store\n    let text = to_string n\n    let borrowed = ref text\n    Owned.call (ref w) (Job { cell: Arc.share (ref a), f: \\() -> String.length borrowed })\n    a\nlet a = run 123456789\nlet junk = [to_string 987654321, to_string 111111111]\nMutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + junk.length",
            "E1013",
            "borrow_outlives_block",
            (11, 5),
        ),
        // oc1
        (
            "record Job { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ndef store :: own Job -> unit\nfn store job = { let g = job.f; Mutex.with_lock (Arc.get (ref job.cell)) (\\slot -> Array.write 0 g slot) }\nlet a = Arc.new (Mutex.create [\\() -> 0])\nlet w = Owned.function store\nlet text = to_string 123456789\nlet borrowed = ref text\nOwned.call (ref w) (Job { cell: Arc.share (ref a), f: \\() -> String.length borrowed })\nlet moved = text\nMutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) * 100 + moved.length",
            "E1014",
            "borrow_conflict",
            (9, 13),
        ),
        // n02_mut_local_fn
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let mut w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let first = { let text0 = to_string n; let text = ref text0; w (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + w (\\() -> 1) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (6, 17),
        ),
        // n03_rc_share_before
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let w = Rc.new (mk (Arc.new (Mutex.create [\\() -> 0])))\n    let w2 = Rc.share (ref w)\n    let text0 = to_string n\n    let text = ref text0\n    let first = (deref (Rc.get (ref w))) (\\() -> String.length text)\n    let moved = text0\n    first + (deref (Rc.get (ref w2))) (\\() -> 1) * 100 + moved.length\nrun 123456789",
            "E1014",
            "borrow_conflict",
            (10, 17),
        ),
        // n04_tuple_temp_copy
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let text0 = to_string n\n    let text = ref text0\n    let t = (w, w (\\() -> String.length text))\n    let moved = text0\n    match t with\n    | (w2, first) -> first + w2 (\\() -> 1) * 100 + moved.length\nrun 123456789",
            "E1014",
            "borrow_conflict",
            (9, 17),
        ),
        // n11_wrapper_sink
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef wrap :: own ((unit -> i64) -> i64) -> (ref string -> i64)\nfn wrap sink = \\s -> sink (\\() -> String.length s)\ndef run :: i64 -> i64\nfn run n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let hs = [wrap w]\n    let first = { let text0 = to_string n; hs[0] (ref text0) }\n    let other = [to_string 777777, to_string 88888888]\n    first + w (\\() -> 1) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (9, 17),
        ),
        // n12_stage_result_sink
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Box { get: unit -> ((unit -> i64) -> i64) }\ndef run :: i64 -> i64\nfn run n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let b = Box { get: \\() -> w }\n    let first = { let text0 = to_string n; let text = ref text0; b.get () (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + w (\\() -> 1) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (8, 17),
        ),
        // n13_arg_sink
        (
            "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef feed :: own ((unit -> i64) -> i64) -> own (unit -> i64) -> i64\nfn feed sink f = sink f\ndef run :: i64 -> i64\nfn run n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let fs = [feed]\n    let first = { let text0 = to_string n; let text = ref text0; fs[0] w (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + w (\\() -> 1) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (9, 17),
        ),
    ] {
        rejects_at(source, code, kind, at);
    }
    for source in [
        // d09_ctl_safe
        "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let text = ref text0\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let first = w (\\() -> String.length text)\n    first + w (\\() -> 1)\nrun 123456789",
        // n08_safe_sink_long_lived
        "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let text = ref text0\n    let ws = [mk (Arc.new (Mutex.create [\\() -> 0]))]\n    let first = ws[0] (\\() -> String.length text)\n    first + ws[0] (\\() -> 1) + text.length\nrun 123456789",
        // n09_named_fn_array_safe
        "def add_len :: ref string -> i64 -> i64\nfn add_len s k = String.length s + k\ndef twice :: ref string -> i64 -> i64\nfn twice s k = String.length s * 2 + k\ndef mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let hs = [add_len, twice]\n    let name = to_string n\n    let k = hs[0] (ref name) 1\n    let moved = name\n    k + hs[1] (ref moved) 2 + w (\\() -> 3)\nrun 12345",
    ] {
        emits_in_both(source);
    }
}

/// A31, review round 1: the captures of a tzroutine (and a channel's items, the argument of
/// `Owned.function`, a dyn value without a region) are checked to hold no borrow where they are
/// handed over (A30), and nothing follows them afterwards. A copy of a function value that owns
/// the cell that it writes shares the cell, so once the program hands such a value to code that
/// runs later, no borrow may be stored in such an environment: the tzroutine below would read
/// `text` after `start` has freed it (main accepts it).
#[test]
fn a_function_value_handed_to_later_code_takes_no_borrow_afterwards() {
    rejects_at(
        "def wait_for :: ref Arc<Atomic<i64>> -> i64 -> unit\nfn wait_for counter target =\n    while Atomic.load (Arc.get counter) < target do\n        Routine.yield ()\ndef mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef start :: own i64 -> ref Arc<Atomic<i64>> -> ref Arc<Atomic<i64>> -> ref Arc<Atomic<i64>> -> unit\nfn start n result gate done =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let w2 = w\n    let mine = Arc.share result\n    let go = Arc.share gate\n    let finished = Arc.share done\n    tzroutine {\n        wait_for (ref go) 1\n        let _sum = Atomic.fetch_add (Arc.get (ref mine)) (w2 (\\() -> 1))\n        let _count = Atomic.fetch_add (Arc.get (ref finished)) 1i64\n    }\n    let text = to_string n\n    let borrowed = ref text\n    let _first = w (\\() -> String.length borrowed)\n    ()\nlet result = Arc.new (Atomic.create 0i64)\nlet gate = Arc.new (Atomic.create 0i64)\nlet done = Arc.new (Atomic.create 0i64)\nstart 123456789 (ref result) (ref gate) (ref done)\nlet junk = [to_string 987654321, to_string 111111111]\nlet _g = Atomic.fetch_add (Arc.get (ref gate)) 1i64\nwait_for (ref done) 1\nAtomic.load (Arc.get (ref result)) + junk.length * 0",
        "E1013",
        "task_capture_retains_borrow",
        (21, 18),
    );
    // The same copy in an owned function that `Routine.go` runs later.
    rejects_at(
        "def wait_for :: ref Arc<Atomic<i64>> -> i64 -> unit\nfn wait_for counter target =\n    while Atomic.load (Arc.get counter) < target do\n        Routine.yield ()\ndef mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef start :: own i64 -> ref Arc<Atomic<i64>> -> ref Arc<Atomic<i64>> -> ref Arc<Atomic<i64>> -> unit\nfn start n result gate done =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n    let w2 = w\n    let mine = Arc.share result\n    let go = Arc.share gate\n    let finished = Arc.share done\n    Routine.go (Owned.function (\\() -> { wait_for (ref go) 1; let _sum = Atomic.fetch_add (Arc.get (ref mine)) (w2 (\\() -> 1)); let _count = Atomic.fetch_add (Arc.get (ref finished)) 1i64; () }))\n    let text = to_string n\n    let borrowed = ref text\n    let _first = w (\\() -> String.length borrowed)\n    ()\nlet result = Arc.new (Atomic.create 0i64)\nlet gate = Arc.new (Atomic.create 0i64)\nlet done = Arc.new (Atomic.create 0i64)\nstart 123456789 (ref result) (ref gate) (ref done)\nlet junk = [to_string 987654321, to_string 111111111]\nlet _g = Atomic.fetch_add (Arc.get (ref gate)) 1i64\nwait_for (ref done) 1\nAtomic.load (Arc.get (ref result)) + junk.length * 0",
        "E1013",
        "task_capture_retains_borrow",
        (17, 18),
    );
}

/// A31, review round 1 (finding 11): the error of a callee that passes a borrow of its own local
/// to a function value that the caller cannot see is reported at the caller's call, and names
/// the callee rather than a local value or a reference parameter of the caller.
#[test]
fn a_borrow_of_a_callee_local_handed_to_an_unseen_function_value_names_the_callee() {
    let source = "def writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn writer a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef leak :: ((unit -> i64) -> unit) -> i64 -> unit\nfn leak w n =\n    let text0 = to_string n\n    let text = ref text0\n    w (\\() -> String.length text)\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = writer (Arc.share (ref a))\n    leak w n\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789";
    for edition in [Edition::E2026, Edition::E2027] {
        let error = analyze_with_edition(source, edition).expect_err(source);
        assert_eq!(
            (error.code, error.kind, position(source, error.span.start)),
            (
                "E1013",
                Some("store_local_borrow_through_reference"),
                (12, 5)
            ),
            "edition {edition}: {}",
            error.message
        );
        assert!(
            error
                .message
                .contains("'leak' passes it a borrow of a local value of its own"),
            "{}",
            error.message
        );
        assert!(
            !error.message.contains("through a reference parameter"),
            "{}",
            error.message
        );
    }
}

/// A31, review round 1 (findings 3, 5, 6 and 17): a value whose `Drop.drop` stores what it holds
/// is dropped without a call in the body. The store is a fact of a callee that drops the value
/// (`build a f` binds a `Stash` and returns unit); a temporary that is never bound (the scrutinee
/// of a `match`, a value lent to a call, a record that a field is read from, an element of an
/// array literal) is dropped where the statement ends; and a `Drop.drop` that stores a borrow of
/// the dropped value itself leaves it in the cell after the value is gone.
#[test]
fn a_drop_in_a_callee_or_of_a_temporary_stores_what_the_value_holds() {
    for (source, code, kind, at) in [
        // g17_fn_returns_unit
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef build :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> unit\nfn build a f =\n    let _s = Stash { cell: a, f: f }\n    ()\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    build a (\\() -> String.length borrowed)\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (15, 5),
        ),
        // g38_drop_build_local
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef build :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> unit\nfn build a f =\n    let _s = Stash { cell: a, f: f }\n    ()\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let text0 = to_string n; let text = ref text0; build (Arc.share (ref a)) (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (12, 5),
        ),
        // g37_drop_match_local
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let text0 = to_string n; let text = ref text0; match Stash { cell: Arc.share (ref a), f: \\() -> String.length text } with | _ -> () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (8, 5),
        ),
        // g13_match
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    match Stash { cell: a, f: \\() -> String.length borrowed } with\n    | _ -> ()\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (11, 11),
        ),
        // dr1
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64, k: i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef peek :: ref Stash -> i64\nfn peek s = s.k\ndef make :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> Stash\nfn make a f = Stash { cell: a, f: f, k: 1 }\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    let _k = peek (ref (Stash { cell: a, f: \\() -> String.length borrowed, k: 1 }))\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (15, 14),
        ),
        // dr2
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64, k: i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef peek :: ref Stash -> i64\nfn peek s = s.k\ndef make :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> Stash\nfn make a f = Stash { cell: a, f: f, k: 1 }\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    let _k = (Stash { cell: a, f: \\() -> String.length borrowed, k: 1 }).k\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (15, 9),
        ),
        // dr3
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64, k: i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef peek :: ref Stash -> i64\nfn peek s = s.k\ndef make :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> Stash\nfn make a f = Stash { cell: a, f: f, k: 1 }\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    let _k = peek (ref (make a (\\() -> String.length borrowed)))\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (15, 14),
        ),
        // dr4
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64, k: i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef peek :: ref Stash -> i64\nfn peek s = s.k\ndef make :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> Stash\nfn make a f = Stash { cell: a, f: f, k: 1 }\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    let _k = (make a (\\() -> String.length borrowed)).k\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (15, 9),
        ),
        // dr6
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64, k: i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef peek :: ref Stash -> i64\nfn peek s = s.k\ndef make :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> Stash\nfn make a f = Stash { cell: a, f: f, k: 1 }\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    let _k = if n > 0 then (make a (\\() -> String.length borrowed)).k else 0\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (15, 9),
        ),
        // dr7
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64, k: i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef peek :: ref Stash -> i64\nfn peek s = s.k\ndef make :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> Stash\nfn make a f = Stash { cell: a, f: f, k: 1 }\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    let _k = [make a (\\() -> String.length borrowed)].length\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (15, 9),
        ),
        // g33_drop_self_borrow_param
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, name: string }\ninstance Drop<Stash> {\n    fn drop s = { let nm = ref s.name; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length nm) slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let _s = Stash { cell: a, name: to_string n }\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (9, 9),
        ),
        // g32_drop_self_borrow
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, name: string }\ninstance Drop<Stash> {\n    fn drop s = { let nm = ref s.name; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 (\\() -> String.length nm) slot) }\n}\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let _s = Stash { cell: Arc.share (ref a), name: to_string n }; () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (8, 5),
        ),
        // dr0
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64, k: i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef peek :: ref Stash -> i64\nfn peek s = s.k\ndef make :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> Stash\nfn make a f = Stash { cell: a, f: f, k: 1 }\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    let _s = Stash { cell: a, f: \\() -> String.length borrowed, k: 1 }\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (15, 9),
        ),
        // dr5
        (
            "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64, k: i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\nfn reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef peek :: ref Stash -> i64\nfn peek s = s.k\ndef make :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> Stash\nfn make a f = Stash { cell: a, f: f, k: 1 }\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)\nfn leak a r n =\n    let text = to_string n\n    let borrowed = ref text\n    make a (\\() -> String.length borrowed) |> ignore\n    r\ndef run :: unit -> i64\nfn run u =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a1 = Arc.share (ref a)\n    let a2 = Arc.share (ref a)\n    let r = reader a2\n    let f = leak a1 r 123456789\n    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]\n    f () + junk.length\nrun ()",
            "E1013",
            "store_local_borrow_through_reference",
            (15, 5),
        ),
    ] {
        rejects_at(source, code, kind, at);
    }
    // A temporary lent to a read parameter (edition 2027); edition 2026 passes it by value.
    let peek = "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef peek :: Stash -> i64\nfn peek _s = 1\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let k = { let text0 = to_string n; let text = ref text0; peek (Stash { cell: Arc.share (ref a), f: \\() -> String.length text }) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0 + k * 0\nrun 123456789";
    for edition in [Edition::E2026, Edition::E2027] {
        let error = analyze_with_edition(peek, edition).expect_err(peek);
        assert_eq!(
            (error.code, error.kind, position(peek, error.span.start)),
            ("E1013", Some("assignment_extends_borrow"), (10, 13)),
            "edition {edition}: {}",
            error.message
        );
    }
    for source in [
        // y01_drop_share_own_param
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef build3 :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> unit\nfn build3 a f =\n    let _s = Stash { cell: Arc.share (ref a), f: f }\n    ()\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    build3 (Arc.share (ref a)) (\\() -> 7)\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + n\nrun 1",
        // y03_drop_share_ref_param
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef build3 :: ref Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> unit\nfn build3 a f =\n    let _s = Stash { cell: Arc.share a, f: f }\n    ()\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    build3 (ref a) (\\() -> 7)\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + n\nrun 1",
        // y04_drop_local_everything
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let text = ref text0\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let _s = Stash { cell: Arc.share (ref a), f: \\() -> String.length text }; () }\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 12345",
        // y05_drop_param_fn_safe
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> unit\nfn leak a f =\n    let _s = Stash { cell: a, f: f }\n    ()\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let text = ref text0\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    leak (Arc.share (ref a)) (\\() -> String.length text)\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 12345",
    ] {
        emits_in_both(source);
    }
}

/// A31, review round 1 (findings 4 and 14): `==`, `!=` and `<` lend their operands to the user
/// instance, which may keep the reference itself (a function that reads `b.name` later): the
/// operand's own value is what the place holds, so the borrow of the place is added for the
/// facts that store the reference. A structural comparison or display of an array runs the
/// instance of the element type with references to the elements. A temporary operand whose
/// reference a callee keeps is an error: it is dropped when the statement ends.
#[test]
fn an_operator_or_a_structural_operation_applies_the_stores_of_the_instance() {
    for (source, code, kind, at) in [
        // v07_eq_instance
        (
            "record Cell {r} { m: ref {r} Mutex<[unit -> i64]>, name: string }\ninstance Eq<Cell> {\n    fn eq a b = { Mutex.with_lock a.m (\\slot -> Array.write 0 (\\() -> String.length b.name) slot); true }\n    fn ne a b = !(Eq.eq a b)\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let c1 = Cell { m: ref m, name: \"a\" }\n    let e = { let c2 = Cell { m: ref m, name: to_string n }; c1 == c2 }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + (if e then 0 else 0)\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (10, 13),
        ),
        // z04_ord_structural
        (
            "record Cell {r} { m: ref {r} Mutex<[unit -> i64]>, name: string }\ninstance Eq<Cell> {\n    fn eq a b = { Mutex.with_lock a.m (\\slot -> Array.write 0 (\\() -> String.length b.name) slot); true }\n    fn ne a b = !(Eq.eq a b)\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let c1 = Cell { m: ref m, name: \"a\" }\n    let e = { let c2 = Cell { m: ref m, name: to_string n }; c1 != c2 }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + (if e then 0 else 0)\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (10, 13),
        ),
        // v10_ord_instance
        (
            "record Cell {r} { m: ref {r} Mutex<[unit -> i64]>, name: string }\ninstance Eq<Cell> {\n    fn eq a b = true\n    fn ne a b = false\n}\ninstance Ord<Cell> {\n    fn lt a b = { Mutex.with_lock a.m (\\slot -> Array.write 0 (\\() -> String.length b.name) slot); true }\n    fn le a b = true\n    fn gt a b = false\n    fn ge a b = false\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let c1 = Cell { m: ref m, name: \"a\" }\n    let e = { let c2 = Cell { m: ref m, name: to_string n }; c1 < c2 }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + (if e then 0 else 0)\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (16, 13),
        ),
        // z01_eq_explicit
        (
            "record Cell {r} { m: ref {r} Mutex<[unit -> i64]>, name: string }\ninstance Eq<Cell> {\n    fn eq a b = { Mutex.with_lock a.m (\\slot -> Array.write 0 (\\() -> String.length b.name) slot); true }\n    fn ne a b = !(Eq.eq a b)\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let c1 = Cell { m: ref m, name: \"a\" }\n    let e = { let c2 = Cell { m: ref m, name: to_string n }; Eq.eq (ref c1) (ref c2) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + (if e then 0 else 0)\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (10, 13),
        ),
        // disp5
        (
            "record Job { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Display<Job> {\n    fn display job = { let g = job.f; Mutex.with_lock (Arc.get (ref job.cell)) (\\slot -> Array.write 0 g slot); \"job\" }\n}\ndef run :: i64 -> Arc<Mutex<[unit -> i64]>>\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let text = to_string n\n    let borrowed = ref text\n    let js = [Job { cell: Arc.share (ref a), f: \\() -> String.length borrowed }]\n    let s = $\"{js}\"\n    a\nlet a = run 123456789\nlet junk = [to_string 987654321, to_string 111111111]\nMutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + junk.length",
            "E1013",
            "borrow_outlives_block",
            (12, 5),
        ),
        // disp4
        (
            "record Job { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Display<Job> {\n    fn display job = { let g = job.f; Mutex.with_lock (Arc.get (ref job.cell)) (\\slot -> Array.write 0 g slot); \"job\" }\n}\nlet a = Arc.new (Mutex.create [\\() -> 0])\nlet text = to_string 123456789\nlet borrowed = ref text\nlet js = [Job { cell: Arc.share (ref a), f: \\() -> String.length borrowed }]\nlet s = $\"{js}\"\nlet moved = text\nMutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) * 100 + moved.length + s.length",
            "E1014",
            "borrow_conflict",
            (10, 13),
        ),
        // disp6
        (
            "record Job { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Display<Job> {\n    fn display job = { let g = job.f; Mutex.with_lock (Arc.get (ref job.cell)) (\\slot -> Array.write 0 g slot); \"job\" }\n}\nlet a = Arc.new (Mutex.create [\\() -> 0])\nlet text = to_string 123456789\nlet borrowed = ref text\nlet js = [Job { cell: Arc.share (ref a), f: \\() -> String.length borrowed }]\nlet s = to_string js\nlet moved = text\nMutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) * 100 + moved.length + s.length",
            "E1014",
            "borrow_conflict",
            (10, 13),
        ),
        // eq2
        (
            "record Job { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Eq<Job> {\n    fn eq left right = { let g = left.f; Mutex.with_lock (Arc.get (ref right.cell)) (\\slot -> Array.write 0 g slot); true }\n    fn ne left right = false\n}\nlet a = Arc.new (Mutex.create [\\() -> 0])\nlet text = to_string 123456789\nlet borrowed = ref text\nlet js = [Job { cell: Arc.share (ref a), f: \\() -> String.length borrowed }]\nlet ks = [Job { cell: Arc.share (ref a), f: \\() -> 1 }]\nlet same = js == ks\nlet moved = text\nMutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) * 100 + moved.length + (if same then 1 else 0)",
            "E1014",
            "borrow_conflict",
            (12, 13),
        ),
        // v05i
        (
            "record Cell {r} { m: ref {r} Mutex<[unit -> i64]>, name: string }\ninstance Display<Cell> {\n    fn display c = { Mutex.with_lock c.m (\\slot -> Array.write 0 (\\() -> String.length c.name) slot); \"cell\" }\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let k = { let c2 = Cell { m: ref m, name: to_string n }; let s = $\"{c2}\"; String.length s }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + k * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (8, 13),
        ),
    ] {
        rejects_at(source, code, kind, at);
    }
    for source in [
        // v08_eq_instance_ctl
        "record Cell {r} { m: ref {r} Mutex<[unit -> i64]>, name: string }\ninstance Eq<Cell> {\n    fn eq a b = { Mutex.with_lock a.m (\\slot -> Array.write 0 (\\() -> String.length b.name) slot); true }\n    fn ne a b = !(Eq.eq a b)\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let c1 = Cell { m: ref m, name: \"a\" }\n    let c2 = Cell { m: ref m, name: to_string n }\n    let e = c1 == c2\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + (if e then 0 else 0)\nrun 123456789",
        // v09i
        "record Cell {r} { m: ref {r} Mutex<[unit -> i64]>, name: string }\ninstance Display<Cell> {\n    fn display c = { Mutex.with_lock c.m (\\slot -> Array.write 0 (\\() -> String.length c.name) slot); \"cell\" }\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let c2 = Cell { m: ref m, name: to_string n }\n    let s = $\"{c2}\"\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + String.length s * 0\nrun 123456789",
    ] {
        emits_in_both(source);
    }
    // Edition 2027 lends the temporary record to `keep`, which keeps the reference in `arr`;
    // edition 2026 moves the record into `keep`, whose function owns it.
    let temporary = "record Cell {r} { m: ref {r} Mutex<[unit -> i64]>, name: string }\ndef keep :: Cell -> ref mut [(unit -> i64)..] -> unit\nfn keep b slot = Array.write 0 (\\() -> String.length b.name) slot\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mut arr = [\\() -> 0]\n    keep (Cell { m: ref m, name: to_string n }) (ref mut arr)\n    let other = [to_string 777777, to_string 88888888]\n    arr[0] () + other.length * 0\nrun 123456789";
    let error = analyze_with_edition(temporary, Edition::E2027).expect_err(temporary);
    assert_eq!(
        (
            error.code,
            error.kind,
            position(temporary, error.span.start)
        ),
        (
            "E1013",
            Some("store_local_borrow_through_reference"),
            (8, 5)
        ),
        "{}",
        error.message
    );
    assert!(error.message.contains("temporary"), "{}", error.message);
    analyze_with_edition(temporary, Edition::E2026)
        .unwrap_or_else(|error| panic!("{temporary}\n{}: {}", error.code, error.message));
}

/// A31, review round 1 (finding 7): the callback of `List.foldr` gets its own result back as the
/// state, so what the callback holds is a source of what it stores.
#[test]
fn a_fold_callback_stores_the_state_that_it_returned() {
    for (source, code, kind, at) in [
        // u04_list_foldr_feedback_mutex
        (
            "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let xs = [| 1, 2 |]\n    { let text0 = to_string n; let text = ref text0; let last = List.foldr (\\acc x -> { Mutex.with_lock mr (\\slot -> Array.write 0 acc slot); \\() -> String.length text + deref x }) (\\() -> 0) xs; () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (6, 5),
        ),
        // u02_list_foldr_feedback_inline
        (
            "def run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a2 = Arc.share (ref a)\n    let xs = [| 1, 2 |]\n    { let text0 = to_string n; let text = ref text0; let last = List.foldr (\\acc x -> { Mutex.with_lock (Arc.get (ref a2)) (\\slot -> Array.write 0 acc slot); \\() -> String.length text + deref x }) (\\() -> 0) xs; () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (6, 5),
        ),
        // u03_array_fold_feedback_inline
        (
            "def run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a2 = Arc.share (ref a)\n    let xs = [1, 2]\n    { let text0 = to_string n; let text = ref text0; let last = Array.fold (\\acc x -> { Mutex.with_lock (Arc.get (ref a2)) (\\slot -> Array.write 0 acc slot); \\() -> String.length text + x }) (\\() -> 0) (ref xs); () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (6, 5),
        ),
    ] {
        rejects_at(source, code, kind, at);
    }
    // u05_list_foldr_ctl
    emits_in_both(
        "def run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let text = ref text0\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a2 = Arc.share (ref a)\n    let xs = [| 1, 2 |]\n    let last = List.foldr (\\acc x -> { Mutex.with_lock (Arc.get (ref a2)) (\\slot -> Array.write 0 acc slot); \\() -> String.length text + deref x }) (\\() -> 0) xs\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + last () * 0\nrun 123456789",
    );
}

/// A31, review round 1 (finding 15): a borrow of an `Rc` or `Arc` owner stored in the block that
/// the owner shares is a pointer to the owner's storage, which dangles when the owner moves; only
/// the loan that stands for a share of the block (`Arc.share (ref a)`) is no such pointer.
/// Finding 18: a local that stays borrowed keeps what a later store put in it, so the source of
/// a borrow written into a cell that is reached only through another cell cannot move.
#[test]
fn a_borrow_kept_in_a_cell_holds_its_owner_and_its_source() {
    for (source, code, kind, at) in [
        // arcself
        (
            "record Holder { n: i64, m: Mutex<[unit -> i64]> }\ndef make :: i64 -> Arc<Holder>\nfn make n =\n    let a = Arc.new (Holder { n: n, m: Mutex.create [\\() -> 0] })\n    let ra = ref a\n    Mutex.with_lock (ref (Arc.get ra).m) (\\slot -> Array.write 0 (\\() -> (Arc.get ra).n) slot)\n    a\nlet a = make 123456789\nlet junk = [to_string 987654321, to_string 111111111]\nMutex.with_lock (ref (Arc.get (ref a)).m) (\\slot -> slot[0] ()) + junk.length",
            "E1014",
            "borrow_conflict",
            (7, 5),
        ),
        // rcself
        (
            "record Holder { n: i64, m: Mutex<[unit -> i64]> }\ndef make :: i64 -> Rc<Holder>\nfn make n =\n    let a = Rc.new (Holder { n: n, m: Mutex.create [\\() -> 0] })\n    let ra = ref a\n    Mutex.with_lock (ref (Rc.get ra).m) (\\slot -> Array.write 0 (\\() -> (Rc.get ra).n) slot)\n    a\nlet a = make 123456789\nlet junk = [to_string 987654321, to_string 111111111]\nMutex.with_lock (ref (Rc.get (ref a)).m) (\\slot -> slot[0] ()) + junk.length",
            "E1014",
            "borrow_conflict",
            (7, 5),
        ),
        // arcself2
        (
            "record Holder { n: i64, m: Mutex<[unit -> i64]> }\nlet a = Arc.new (Holder { n: 7, m: Mutex.create [\\() -> 0] })\nlet ra = ref a\nMutex.with_lock (ref (Arc.get ra).m) (\\slot -> Array.write 0 (\\() -> (Arc.get ra).n) slot)\nlet b = a\nlet junk = [to_string 987654321, to_string 111111111]\nMutex.with_lock (ref (Arc.get (ref b)).m) (\\slot -> slot[0] ()) + junk.length",
            "E1014",
            "borrow_conflict",
            (5, 9),
        ),
        // holderself
        (
            "record Holder { n: i64, m: Mutex<[unit -> i64]> }\ndef make :: i64 -> Holder\nfn make n =\n    let a = Holder { n: n, m: Mutex.create [\\() -> 0] }\n    let ra = ref a\n    Mutex.with_lock (ref ra.m) (\\slot -> Array.write 0 (\\() -> ra.n) slot)\n    a\nlet a = make 123456789\nlet junk = [to_string 987654321, to_string 111111111]\nMutex.with_lock (ref a.m) (\\slot -> slot[0] ()) + junk.length",
            "E1014",
            "borrow_conflict",
            (7, 5),
        ),
        // chain3
        (
            "let text = to_string 123456789\nlet borrowed = ref text\nlet k = Mutex.create [\\() -> 0]\nlet m = Mutex.create [\\() -> 0]\nlet rm = ref m\nlet rk = ref k\nMutex.with_lock (ref m) (\\slot -> Array.write 0 (\\() -> Mutex.with_lock rk (\\s -> s[0] ())) slot)\nMutex.with_lock (ref k) (\\slot -> Array.write 0 (\\() -> String.length borrowed) slot)\nlet moved = text\nMutex.with_lock rm (\\slot -> slot[0] ()) * 100 + moved.length",
            "E1014",
            "borrow_conflict",
            (9, 13),
        ),
        // chain3_order
        (
            "let text = to_string 123456789\nlet borrowed = ref text\nlet k = Mutex.create [\\() -> 0]\nlet m = Mutex.create [\\() -> 0]\nlet rm = ref m\nlet rk = ref k\nMutex.with_lock (ref k) (\\slot -> Array.write 0 (\\() -> String.length borrowed) slot)\nMutex.with_lock (ref m) (\\slot -> Array.write 0 (\\() -> Mutex.with_lock rk (\\s -> s[0] ())) slot)\nlet moved = text\nMutex.with_lock rm (\\slot -> slot[0] ()) * 100 + moved.length",
            "E1014",
            "borrow_conflict",
            (9, 13),
        ),
        // chain3_arc
        (
            "def run :: i64 -> i64\nfn run n =\n    let text = to_string n\n    let borrowed = ref text\n    let k = Arc.new (Mutex.create [\\() -> 0])\n    let m = Arc.new (Mutex.create [\\() -> 0])\n    let rm = ref m\n    let rk = ref k\n    Mutex.with_lock (Arc.get (ref m)) (\\slot -> Array.write 0 (\\() -> Mutex.with_lock (Arc.get rk) (\\s -> s[0] ())) slot)\n    Mutex.with_lock (Arc.get (ref k)) (\\slot -> Array.write 0 (\\() -> String.length borrowed) slot)\n    let moved = text\n    Mutex.with_lock (Arc.get rm) (\\slot -> slot[0] ()) * 100 + moved.length\nrun 123456789",
            "E1014",
            "borrow_conflict",
            (11, 17),
        ),
        // chain3_mlive
        (
            "let text = to_string 123456789\nlet borrowed = ref text\nlet k = Mutex.create [\\() -> 0]\nlet m = Mutex.create [\\() -> 0]\nlet rm = ref m\nlet rk = ref k\nMutex.with_lock (ref m) (\\slot -> Array.write 0 (\\() -> Mutex.with_lock rk (\\s -> s[0] ())) slot)\nMutex.with_lock (ref k) (\\slot -> Array.write 0 (\\() -> String.length borrowed) slot)\nlet moved = text\nMutex.with_lock rm (\\slot -> slot[0] ()) * 100 + moved.length + Mutex.with_lock (ref m) (\\slot -> slot.length)",
            "E1014",
            "borrow_conflict",
            (9, 13),
        ),
        // chain2
        (
            "let text = to_string 123456789\nlet borrowed = ref text\nlet k = Mutex.create [\\() -> 0]\nlet rk = ref k\nMutex.with_lock (ref k) (\\slot -> Array.write 0 (\\() -> String.length borrowed) slot)\nlet moved = text\nMutex.with_lock rk (\\slot -> slot[0] ()) * 100 + moved.length",
            "E1014",
            "borrow_conflict",
            (6, 13),
        ),
    ] {
        rejects_at(source, code, kind, at);
    }
    // chain3_ctl
    emits_in_both(
        "let text = to_string 123456789\nlet borrowed = ref text\nlet k = Mutex.create [\\() -> 0]\nlet m = Mutex.create [\\() -> 0]\nlet rm = ref m\nlet rk = ref k\nMutex.with_lock (ref m) (\\slot -> Array.write 0 (\\() -> Mutex.with_lock rk (\\s -> s[0] ())) slot)\nMutex.with_lock (ref k) (\\slot -> Array.write 0 (\\() -> String.length borrowed) slot)\nMutex.with_lock rm (\\slot -> slot[0] ()) * 100 + text.length",
    );
}

/// A31, review round 1 (finding 12): a dyn method's dispatch runs one of the slot functions of
/// the vtables, so a call of the method applies their facts, and a store through the receiver
/// reaches what the dyn value holds (`dyn Sinks.Sink {r}` over a `ref Mutex`). A method that
/// stores nothing keeps the other arguments free (`r11`: main accepts it).
#[test]
fn a_dyn_method_applies_the_stores_of_the_instances() {
    let sinks = "class Sink<'value> {\n    def put :: ref 'value -> own (unit -> i64) -> unit\n    def read :: ref 'value -> i64\n}";
    for (main, at) in [
        // v24_dyn_bad
        (
            "record Cell {r} { m: ref {r} Mutex<[unit -> i64]> }\ninstance Sinks.Sink<Cell> {\n    fn put cell g = Mutex.with_lock cell.m (\\slot -> Array.write 0 g slot)\n    fn read _cell = 0\n}\ndef make {r} :: ref {r} Mutex<[unit -> i64]> -> dyn Sinks.Sink {r}\nfn make m = Dyn.of (Cell { m: m })\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let w = make (ref m)\n    { let text0 = to_string n; let text = ref text0; Sinks.Sink.put (ref w) (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            (12, 5),
        ),
        // v02_dyn_region
        (
            "record Cell {r} { m: ref {r} Mutex<[unit -> i64]> }\ninstance Sinks.Sink<Cell> {\n    fn put cell g = Mutex.with_lock cell.m (\\slot -> Array.write 0 g slot)\n    fn read cell = Mutex.with_lock cell.m (\\slot -> slot[0] ())\n}\ndef make {r} :: ref {r} Mutex<[unit -> i64]> -> dyn Sinks.Sink {r}\nfn make m = Dyn.of (Cell { m: m })\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let w = make (ref m)\n    { let text0 = to_string n; let text = ref text0; Sinks.Sink.put (ref w) (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    Sinks.Sink.read (ref w) + other.length * 0\nrun 123456789",
            (12, 5),
        ),
    ] {
        for edition in [Edition::E2026, Edition::E2027] {
            let error =
                analyze_modules_with_edition(&[("Sinks.tt", sinks), ("Main.tz", main)], edition)
                    .expect_err(main);
            assert_eq!(
                (error.code, error.kind, position(main, error.span.start)),
                ("E1013", Some("assignment_extends_borrow"), at),
                "edition {edition}\n{main}\n{}",
                error.message
            );
        }
    }
    for main in [
        // v20_dyn_plain
        "record Cell {r} { m: ref {r} Mutex<[unit -> i64]> }\ninstance Sinks.Sink<Cell> {\n    fn put cell g = Mutex.with_lock cell.m (\\slot -> Array.write 0 g slot)\n    fn read _cell = 0\n}\ndef make {r} :: ref {r} Mutex<[unit -> i64]> -> dyn Sinks.Sink {r}\nfn make m = Dyn.of (Cell { m: m })\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let w = make (ref m)\n    Sinks.Sink.put (ref w) (\\() -> 7)\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + n\nrun 1",
        // v03_dyn_region_ctl
        "record Cell {r} { m: ref {r} Mutex<[unit -> i64]> }\ninstance Sinks.Sink<Cell> {\n    fn put cell g = Mutex.with_lock cell.m (\\slot -> Array.write 0 g slot)\n    fn read cell = Mutex.with_lock cell.m (\\slot -> slot[0] ())\n}\ndef make {r} :: ref {r} Mutex<[unit -> i64]> -> dyn Sinks.Sink {r}\nfn make m = Dyn.of (Cell { m: m })\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let text = ref text0\n    let m = Mutex.create [\\() -> 0]\n    let w = make (ref m)\n    Sinks.Sink.put (ref w) (\\() -> String.length text)\n    Sinks.Sink.read (ref w)\nrun 123456789",
    ] {
        for edition in [Edition::E2026, Edition::E2027] {
            analyze_modules_with_edition(&[("Sinks.tt", sinks), ("Main.tz", main)], edition)
                .unwrap_or_else(|error| panic!("{main}\n{}: {}", error.code, error.message));
        }
    }
    // r11_dyn_visit (edition 2027: the instance assigns a field through its receiver)
    let visits = "class Visitor<'value> {\n    def visit :: ref mut 'value -> ref string -> unit\n    def total :: ref 'value -> i64\n}";
    let main = "record Counter { total: i64 }\ninstance Visits.Visitor<Counter> {\n    fn visit c s = c.total = c.total + String.length s\n    fn total c = c.total\n}\ndef keep :: ref mut [(unit -> i64)..] -> unit\nfn keep arr = Array.write 0 (\\() -> 1) arr\ndef run :: i64 -> i64\nfn run n =\n    let mut fs = [\\() -> 0]\n    keep (ref mut fs)\n    let mut v: dyn Visits.Visitor = Dyn.of (Counter { total: 0 })\n    let name = to_string n\n    Visits.Visitor.visit (ref mut v) (ref name)\n    let moved = name\n    Visits.Visitor.total (ref v) + String.length moved + fs[0] ()\nrun 12345";
    analyze_modules_with_edition(&[("Visits.tt", visits), ("Main.tz", main)], Edition::E2027)
        .unwrap_or_else(|error| panic!("{main}\n{}: {}", error.code, error.message));
}

/// A31, review round 2: a source in both editions is accepted, with deterministic IR.
fn accepts_in_both_editions(modules: &[(&str, &str)]) {
    for edition in [Edition::E2026, Edition::E2027] {
        let module = analyze_modules_with_edition(modules, edition).unwrap_or_else(|error| {
            panic!(
                "edition {edition}\n{modules:?}\n{}: {}",
                error.code, error.message
            )
        });
        let ir = llvm::emit_target(&module, llvm::Entry::Library, false).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, false).unwrap(),
            "{modules:?}"
        );
    }
}

/// A31, review round 2 (findings 1, 5, 6 and 11): a function value is a sink (it stores into
/// what its environment reaches) when it passes a bound value to a call of a function value only
/// if a store may reach through that value: the lambda of std's `Array.map`
/// (`\index -> transform values[index]`) passes an `i64`, that of `Array.mapr` a `ref i64`, and
/// `Mutex.with_lock (ref m)` on a `Mutex<i64>` lends an `i64`. A module that uses them and writes
/// a closure in place keeps its dispatch tables, handler records and helpers that take a
/// callback as main accepts them (the values are in the `in_place_writes` suite).
#[test]
fn a_bound_value_that_no_store_reaches_through_makes_no_sink() {
    for main in [
        // t06_dispatch_table
        "def setf :: ref mut [(unit -> i64)..] -> unit\nfn setf a = Array.write 0 (\\() -> 1) a\ndef run :: i64 -> i64\nfn run n =\n    let table = [\\(x: i64) -> x + 1, \\(x: i64) -> x * 2]\n    let t = ref table\n    let ops = [\\(s: ref string) -> t[0] (String.length s)]\n    let name = to_string n\n    let k = ops[0] (ref name)\n    let moved = name\n    let sizes = Array.map (\\x -> x + 1) [1, 2]\n    k + table[1] 3 + moved.length + sizes[0]\nrun 12345",
        // t03_reader_record_direct_map
        "record H { f: ref string -> i64 }\ndef setf :: ref mut [(unit -> i64)..] -> unit\nfn setf a = Array.write 0 (\\() -> 1) a\ndef bump :: [i64] -> [i64]\nfn bump xs = Array.map (\\x -> x + 1) xs\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 3]\n    let mr = ref m\n    let h = H { f: \\(s: ref string) -> Mutex.with_lock mr (\\slot -> slot[0] () + String.length s) }\n    let name = to_string n\n    let k = h.f (ref name)\n    let moved = name\n    k + moved.length + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + (bump [1])[0]\nrun 12345",
        // u24_array_mapr
        "def setf :: ref mut [(unit -> i64)..] -> unit\nfn setf a = Array.write 0 (\\() -> 1) a\ndef extra :: [i64] -> i64\nfn extra xs = (Array.mapr (\\(x: ref i64) -> deref x + 1) (ref xs))[0]\ndef run :: i64 -> i64\nfn run n =\n    let table = [\\(x: i64) -> x + 1, \\(x: i64) -> x * 2]\n    let t = ref table\n    let ops = [\\(s: ref string) -> t[0] (String.length s)]\n    let name = to_string n\n    let k = ops[0] (ref name)\n    let moved = name\n    k + table[1] 3 + moved.length + extra [1, 2]\nrun 12345",
        // r17_direct_map_ctl
        "def with_name :: i64 -> ((unit -> i64) -> i64) -> i64\nfn with_name n k =\n    let s = to_string n\n    let b = ref s\n    k (\\() -> String.length b)\ndef run :: i64 -> i64\nfn run n =\n    let xs = [1, 2, 3]\n    let ys = Array.map (\\x -> x + 1) xs\n    let mut arr = [\\() -> 0]\n    let t = to_string 5\n    let bt = ref t\n    Array.set 0 (\\() -> String.length bt) (ref mut arr)\n    with_name n (\\get -> get () * 2) + ys[0] + arr[0] ()\nrun 123456789",
        // r20_Array_mapi
        "def with_name :: i64 -> ((unit -> i64) -> i64) -> i64\nfn with_name n k =\n    let s = to_string n\n    let b = ref s\n    k (\\() -> String.length b)\ndef run :: i64 -> i64\nfn run n =\n    let xs = [1, 2, 3]\n    let ys = Array.mapi (\\i x -> i + x) xs\n    let mut arr = [\\() -> 0]\n    let t = to_string 5\n    let bt = ref t\n    Array.set 0 (\\() -> String.length bt) (ref mut arr)\n    with_name n (\\get -> get () * 2) + ys[0] + arr[0] ()\nrun 123456789",
        // r13j_with_lock_partial_only
        "def with_name :: i64 -> ((unit -> i64) -> i64) -> i64\nfn with_name n k =\n    let s = to_string n\n    let b = ref s\n    k (\\() -> String.length b)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create 0\n    let bump = Mutex.with_lock (ref m)\n    let z = bump (\\c -> 1)\n    let mut arr = [\\() -> 0]\n    let t = to_string 5\n    let bt = ref t\n    Array.write 0 (\\() -> String.length bt) (ref mut arr)\n    with_name n (\\get -> get () * 2) + z + arr[0] ()\nrun 123456789",
        // r14_apply_to_captured
        "def with_name :: i64 -> ((unit -> i64) -> i64) -> i64\nfn with_name n k =\n    let s = to_string n\n    let b = ref s\n    k (\\() -> String.length b)\ndef apply_all :: [i64 -> i64] -> i64 -> [i64]\nfn apply_all fs x = Array.map (\\f -> f x) fs\ndef run :: i64 -> i64\nfn run n =\n    let ys = apply_all [\\x -> x + 1, \\x -> x * 2] 5\n    let mut cbs = [\\() -> 0]\n    let t = to_string 5\n    let bt = ref t\n    Array.set 0 (\\() -> String.length bt) (ref mut cbs)\n    with_name n (\\get -> get () * 2) + ys[0] + cbs[0] ()\nrun 123456789",
        // r18_handler_record_ref_arg
        "record Handler { run: ref string -> i64 }\ndef invoke :: ref Handler -> ref string -> i64\nfn invoke h s = h.run s\ndef run :: i64 -> i64\nfn run n =\n    let h = Handler { run: \\s -> String.length s }\n    let text = to_string n\n    let k = invoke (ref h) (ref text)\n    let moved = text\n    let ys = Array.map (\\x -> x + 1) [1, 2]\n    let mut arr = [\\() -> 0]\n    let t = to_string 5\n    let bt = ref t\n    Array.set 0 (\\() -> String.length bt) (ref mut arr)\n    k + moved.length + ys[0] + arr[0] () + h.run (ref t)\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 2 (finding 5): a callee that passes a closure over its own local to a
/// callback that only calls it (`\get -> get () * 2`, the named `plus_one`) stores the borrow
/// nowhere: a call of a function value with no argument hands it nothing, and the closure's
/// environment holds only the callee's own values. A module with a function value that stores
/// into its environment elsewhere (`remember`, a partial `Mutex.with_lock`, an owner of a cell)
/// keeps such programs accepted.
#[test]
fn a_call_that_hands_nothing_keeps_no_borrow_of_the_callee() {
    for main in [
        // r13b_ownsink_lambda_to_apply
        "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn mk a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef apply :: ((unit -> i64) -> i64) -> i64 -> i64\nfn apply h n =\n    let text0 = to_string n\n    let text = ref text0\n    h (\\() -> String.length text)\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = mk (Arc.share (ref a))\n    w (\\() -> 7)\n    let r = apply (\\f -> f () + 1) n\n    r + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
        // r13c_ownsink_named_to_apply
        "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn mk a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef apply :: ((unit -> i64) -> i64) -> i64 -> i64\nfn apply h n =\n    let text0 = to_string n\n    let text = ref text0\n    h (\\() -> String.length text)\ndef plus_one :: (unit -> i64) -> i64\nfn plus_one f = f () + 1\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = mk (Arc.share (ref a))\n    w (\\() -> 7)\n    let r = apply plus_one n\n    r + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
        // r13g_with_helper_mutex_partial
        "def with_name :: i64 -> ((unit -> i64) -> i64) -> i64\nfn with_name n k =\n    let s = to_string n\n    let b = ref s\n    k (\\() -> String.length b)\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let set = Mutex.with_lock (Arc.get (ref a))\n    set (\\slot -> Array.write 0 (\\() -> 5) slot)\n    let x = with_name n (\\get -> get () * 2)\n    x + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
        // r15_inline_cache_closure_with_helper
        "def with_name :: i64 -> ((unit -> i64) -> i64) -> i64\nfn with_name n k =\n    let s = to_string n\n    let b = ref s\n    k (\\() -> String.length b)\ndef cached :: i64 -> i64\nfn cached n =\n    let cache = Arc.new (Mutex.create [\\() -> 0])\n    let rc = ref cache\n    let remember = \\(f: unit -> i64) -> Mutex.with_lock (Arc.get rc) (\\slot -> Array.write 0 f slot)\n    remember (\\() -> n)\n    Mutex.with_lock (Arc.get (ref cache)) (\\slot -> slot[0] ())\ndef run :: i64 -> i64\nfn run n = cached 4 + with_name n (\\get -> get () * 2)\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 2 (findings 2 and 7): a function value that owns the cell that it writes
/// (an event bus `subscriber`, `mk`) is reached only through values whose types may hold one:
/// a handler of another type, a closure that captured a `ref string`, a named function in an
/// array, and a parameter of a function type that no owner has are none of them. A table of
/// handlers whose type is the owner's (`r4`) is an immutable array literal of lambdas, whose
/// code the check sees.
#[test]
fn a_function_value_of_a_type_that_no_owner_has_reaches_no_hidden_cell() {
    for main in [
        // w06_bus_plus_handlers_other_fn
        "def subscriber :: own Arc<Mutex<[unit -> i64]>> -> (own (unit -> i64) -> unit)\nfn subscriber bus = \\cb -> Mutex.with_lock (Arc.get (ref bus)) (\\slot -> Array.write 0 cb slot)\ndef setup :: i64 -> i64\nfn setup k =\n    let bus = Arc.new (Mutex.create [\\() -> 0])\n    let sub = subscriber (Arc.share (ref bus))\n    sub (\\() -> 5)\n    Mutex.with_lock (Arc.get (ref bus)) (\\slot -> slot[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let hs = [\\(g: unit -> i64) -> g () + 1]\n    let name = to_string n\n    let t = ref name\n    let k = hs[0] (\\() -> String.length t)\n    let moved = name\n    k + moved.length + hs[0] (\\() -> 2)\nrun 12345 + setup 0",
        // w08_bus_plus_sort_callback
        "def subscriber :: own Arc<Mutex<[unit -> i64]>> -> (own (unit -> i64) -> unit)\nfn subscriber bus = \\cb -> Mutex.with_lock (Arc.get (ref bus)) (\\slot -> Array.write 0 cb slot)\ndef setup :: i64 -> i64\nfn setup k =\n    let bus = Arc.new (Mutex.create [\\() -> 0])\n    let sub = subscriber (Arc.share (ref bus))\n    sub (\\() -> 5)\n    Mutex.with_lock (Arc.get (ref bus)) (\\slot -> slot[0] ())\ndef apply_cmp :: (i64 -> i64 -> i64) -> i64\nfn apply_cmp c = c 1 2\ndef run :: i64 -> i64\nfn run n =\n    let fs = [apply_cmp]\n    let name = to_string n\n    let t = ref name\n    let k = fs[0] (\\a b -> a + b + String.length t)\n    let moved = name\n    k + moved.length + fs[0] (\\a b -> a * b)\nrun 12345 + setup 0",
        // w09_bus_plus_fnparam_local
        "def subscriber :: own Arc<Mutex<[unit -> i64]>> -> (own (unit -> i64) -> unit)\nfn subscriber bus = \\cb -> Mutex.with_lock (Arc.get (ref bus)) (\\slot -> Array.write 0 cb slot)\ndef setup :: i64 -> i64\nfn setup k =\n    let bus = Arc.new (Mutex.create [\\() -> 0])\n    let sub = subscriber (Arc.share (ref bus))\n    sub (\\() -> 5)\n    Mutex.with_lock (Arc.get (ref bus)) (\\slot -> slot[0] ())\ndef run_with :: (i64 -> i64) -> i64 -> i64\nfn run_with f n =\n    let hs = [\\(g: unit -> i64) -> g () + 1]\n    let name = to_string n\n    let t = ref name\n    hs[0] (\\() -> String.length t) + f 1\nrun_with (\\x -> x + 1) 12345 + setup 0",
        // w15_bus_handlers_fnparam_after
        "def subscriber :: own Arc<Mutex<[unit -> i64]>> -> (own (unit -> i64) -> unit)\nfn subscriber bus = \\cb -> Mutex.with_lock (Arc.get (ref bus)) (\\slot -> Array.write 0 cb slot)\ndef setup :: i64 -> i64\nfn setup k =\n    let bus = Arc.new (Mutex.create [\\() -> 0])\n    let sub = subscriber (Arc.share (ref bus))\n    sub (\\() -> 5)\n    Mutex.with_lock (Arc.get (ref bus)) (\\slot -> slot[0] ())\ndef run_with :: (i64 -> i64) -> i64 -> i64\nfn run_with f n =\n    let name = to_string n\n    let t = ref name\n    let hs = [\\(g: unit -> i64) -> g () + 1]\n    hs[0] (\\() -> String.length t) + f 1\nrun_with (\\x -> x + 1) 12345 + setup 0",
        // r10_maybe_cb_ownsink
        "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn mk a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = mk (Arc.share (ref a))\n    w (\\() -> 7)\n    let cbs = [\\(f: unit -> i64) -> f () * 3]\n    let text = to_string n\n    let bt = ref text\n    let k = cbs[0] (\\() -> String.length bt)\n    let moved = text\n    k + moved.length + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
        // r3_handlers_ownsink_othertype
        "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn mk a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef run :: i64 -> i64\nfn run n =\n    let handlers = [\\(f: unit -> i64) -> f () + 1, \\f -> f () * 2]\n    let r = { let text0 = to_string n; let text = ref text0; handlers[0] (\\() -> String.length text) }\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = mk (Arc.share (ref a))\n    w (\\() -> 7)\n    r + handlers[1] (\\() -> 1) + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
        // r4_handlers_ownsink_sametype
        "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> i64)\nfn mk a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { Array.write 0 g slot; 0 })\ndef run :: i64 -> i64\nfn run n =\n    let handlers = [\\(f: unit -> i64) -> f () + 1, \\f -> f () * 2]\n    let r = { let text0 = to_string n; let text = ref text0; handlers[0] (\\() -> String.length text) }\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = mk (Arc.share (ref a))\n    let z = w (\\() -> 7)\n    r + z + handlers[1] (\\() -> 1) + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
        // r9_record_handler_ownsink
        "record Handler { name: string, run: (unit -> i64) -> i64 }\ndef mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn mk a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef invoke :: ref Handler -> i64 -> i64\nfn invoke h n =\n    let text0 = to_string n\n    let text = ref text0\n    h.run (\\() -> String.length text)\ndef run :: i64 -> i64\nfn run n =\n    let h = Handler { name: \"x\", run: \\f -> f () + 1 }\n    let r = invoke (ref h) n\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = mk (Arc.share (ref a))\n    w (\\() -> 7)\n    r + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 2 (finding 9): with a `Drop.drop` that writes in the module, a closure that
/// captured only references runs no code when it is dropped, so binding it or passing it by value
/// stores nothing where its references reach.
#[test]
fn dropping_a_closure_runs_only_what_its_captures_drop() {
    for main in [
        // d6_writing_drop_local_closure_move
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef work :: i64 -> i64\nfn work n =\n    let m = Mutex.create [\\() -> 1]\n    let rm = ref m\n    let text = to_string n\n    let bt = ref text\n    let f = \\() -> Mutex.with_lock rm (\\slot -> slot[0] ()) + String.length bt\n    let k = f () + f ()\n    let moved = text\n    k + moved.length + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let s = Stash { cell: Arc.share (ref a), f: \\() -> 5 }; () }\n    work n + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
        // d3_writing_drop_temp_closure
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\ndef call_once :: own (unit -> i64) -> i64\nfn call_once f = f ()\ndef work :: i64 -> i64\nfn work n =\n    let m = Mutex.create [\\() -> 1]\n    let text = to_string n\n    let bt = ref text\n    let rm = ref m\n    let k = call_once (\\() -> Mutex.with_lock rm (\\slot -> slot[0] ()) + String.length bt)\n    let moved = text\n    k + moved.length + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let s = Stash { cell: Arc.share (ref a), f: \\() -> 5 }; () }\n    work n + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 2 (finding 8): a place that holds a dyn value is a place to store into only
/// when a method of its vtables may store (its facts) or gives back a value through which a store
/// may reach (`cell` returns a `ref Mutex`): an array of `dyn Shape` whose `area` stores nothing
/// keeps a borrowed string free (`dyn1`, `dyn2`).
#[test]
fn a_dyn_value_is_a_store_target_only_through_methods_that_store() {
    let shapes = "class Shape<'value> {\n    def area :: ref 'value -> i64\n}";
    for main in [
        // dyn2
        "record Sq { s: i64 }\ninstance Shapes.Shape<Sq> {\n    fn area q = q.s * q.s\n}\ndef run :: i64 -> i64\nfn run n =\n    let shapes: [dyn Shapes.Shape] = [Dyn.of (Sq { s: 3 })]\n    let text = to_string n\n    let reporters = [\\(d: ref [dyn Shapes.Shape]) (s: ref string) -> Shapes.Shape.area (ref d[0]) + String.length s]\n    let k = reporters[0] (ref shapes) (ref text)\n    let moved = text\n    let mut cbs = [\\() -> 0]\n    let t = to_string 5\n    let b2 = ref t\n    Array.set 0 (\\() -> String.length b2) (ref mut cbs)\n    k + moved.length + cbs[0] () + Shapes.Shape.area (ref shapes[0])\nrun 123456789",
        // dyn1
        "record Sq { s: i64 }\ninstance Shapes.Shape<Sq> {\n    fn area q = q.s * q.s\n}\ndef run :: i64 -> i64\nfn run n =\n    let shapes: [dyn Shapes.Shape] = [Dyn.of (Sq { s: 3 })]\n    let text = to_string n\n    let bt = ref text\n    let reporters = [\\(d: ref [dyn Shapes.Shape]) (f: unit -> i64) -> Shapes.Shape.area (ref d[0]) + f ()]\n    let k = reporters[0] (ref shapes) (\\() -> String.length bt)\n    let moved = text\n    let mut cbs = [\\() -> 0]\n    let t = to_string 5\n    let b2 = ref t\n    Array.set 0 (\\() -> String.length b2) (ref mut cbs)\n    k + moved.length + cbs[0] () + Shapes.Shape.area (ref shapes[0])\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Shapes.tt", shapes), ("Main.tz", main)]);
    }
    for (boxes, main, at) in [
        // v04_dyn_cell_method
        (
            "class Holder<'value> {\n    def cell :: ref 'value -> ref Mutex<[unit -> i64]>\n}",
            "record Hold {r} { m: ref {r} Mutex<[unit -> i64]> }\ninstance Boxes.Holder<Hold> {\n    fn cell h = h.m\n}\ndef make {r} :: ref {r} Mutex<[unit -> i64]> -> dyn Boxes.Holder {r}\nfn make m = Dyn.of (Hold { m: m })\ndef wr {r} :: ref (dyn Boxes.Holder {r}) -> ref string -> i64\nfn wr d s = Mutex.with_lock (Boxes.Holder.cell d) (\\slot -> { Array.write 0 (\\() -> String.length s) slot; 1 })\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let d = make (ref m)\n    let mut ws = [wr]\n    let k = { let text0 = to_string n; ws[0] (ref d) (ref text0) }\n    let other = [to_string 777777, to_string 88888888]\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            (14, 13),
        ),
        // v05_dyn_storing_method
        (
            "class Keeper<'value> {\n    def keep :: ref 'value -> ref string -> unit\n    def read :: ref 'value -> i64\n}",
            "record K {r} { m: ref {r} Mutex<[unit -> i64]> }\ninstance Boxes.Keeper<K> {\n    fn keep k s = Mutex.with_lock k.m (\\slot -> Array.write 0 (\\() -> String.length s) slot)\n    fn read k = Mutex.with_lock k.m (\\slot -> slot[0] ())\n}\ndef make {r} :: ref {r} Mutex<[unit -> i64]> -> dyn Boxes.Keeper {r}\nfn make m = Dyn.of (K { m: m })\ndef kp {r} :: ref (dyn Boxes.Keeper {r}) -> ref string -> i64\nfn kp d s = { Boxes.Keeper.keep d s; 1 }\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let d = make (ref m)\n    let mut cs = [kp]\n    let k = { let text0 = to_string n; cs[0] (ref d) (ref text0) }\n    let other = [to_string 777777, to_string 88888888]\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            (15, 13),
        ),
    ] {
        for edition in [Edition::E2026, Edition::E2027] {
            let error =
                analyze_modules_with_edition(&[("Boxes.tt", boxes), ("Main.tz", main)], edition)
                    .expect_err(main);
            assert_eq!(
                (error.code, error.kind, position(main, error.span.start)),
                ("E1013", Some("assignment_extends_borrow"), at),
                "edition {edition}\n{main}\n{}",
                error.message
            );
        }
    }
}

/// A31, review round 2 (finding 4): the error of a callee whose instance is a specialization of
/// a generic function names the declaration (`'leak'`), not the instance (`'leak.$mono.2'`).
#[test]
fn the_error_of_a_generic_callee_names_its_declaration() {
    for (main, at) in [
        // g07_generic_known_writer
        (
            "def leak :: Copy<'b> => ((unit -> i64) -> 'b) -> i64 -> 'b\nfn leak w n =\n    let text = to_string n\n    let b = ref text\n    w (\\() -> String.length b)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    leak (\\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot)) n\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 123456789",
            (10, 5),
        ),
        // g01_generic_callee_name
        (
            "def leak :: Copy<'b> => ((unit -> i64) -> 'b) -> i64 -> 'b\nfn leak w n =\n    let text = to_string n\n    let b = ref text\n    w (\\() -> String.length b)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let ws = [\\(g: unit -> i64) -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot)]\n    leak ws[0] n\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 123456789",
            (11, 5),
        ),
    ] {
        for edition in [Edition::E2026, Edition::E2027] {
            let error = analyze_with_edition(main, edition).expect_err(main);
            assert_eq!(
                (error.code, error.kind, position(main, error.span.start)),
                ("E1013", Some("store_local_borrow_through_reference"), at),
                "edition {edition}: {}",
                error.message
            );
            assert!(
                error.message.contains("'leak' passes it"),
                "{}",
                error.message
            );
            assert!(!error.message.contains("$mono"), "{}", error.message);
        }
    }
}

/// A31, review round 2 (found while answering it): code that the check does not see may store
/// what the function value's own environment holds where that environment reaches: a closure that
/// captured a borrow and a cell (`\() -> { Mutex.with_lock mr (...); 1 }`) puts the borrow into the
/// cell when it runs, through an element of an array, a field, a returned array or an `IO`
/// action. Main and the first round accepted these and read the freed string (they print 0, not
/// 9).
#[test]
fn an_unseen_function_value_stores_what_its_environment_holds_where_it_reaches() {
    for (main, at) in [
        // v01_self_env_store
        (
            "def mkt :: ref Mutex<[unit -> i64]> -> ref string -> [unit -> i64]\nfn mkt mr text = [\\() -> { Mutex.with_lock mr (\\slot -> Array.write 0 (\\() -> String.length text) slot); 1 }]\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let k = { let text0 = to_string n; let text = ref text0; let ts = mkt (ref m) text; ts[0] () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + k * 0\nrun 123456789",
            (6, 13),
        ),
        // v02_site_target
        (
            "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let k = { let text0 = to_string n; let t = ref text0; let f = \\(c: ref Mutex<[unit -> i64]>) -> { Mutex.with_lock c (\\slot -> Array.write 0 (\\() -> String.length t) slot); 1 }; let g = \\() -> f mr; let mut gs = [g]; gs[0] () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + k * 0\nrun 123456789",
            (5, 13),
        ),
        // q01_io_map_writer
        (
            "def go :: ref Mutex<[unit -> i64]> -> ref string -> i64\nfn go mr text =\n    let act = IO.pure 1 |> IO.map (\\x -> { Mutex.with_lock mr (\\slot -> Array.write 0 (\\() -> String.length text) slot); x })\n    let! v = act\n    v\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let k = { let text0 = to_string n; go (ref m) (ref text0) }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + k * 0\nrun 123456789",
            (9, 13),
        ),
        // q11_thunk_array_direct
        (
            "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let k = { let text0 = to_string n; let text = ref text0; let mr = ref m; let ts = [\\() -> { Mutex.with_lock mr (\\slot -> Array.write 0 (\\() -> String.length text) slot); 1 }]; ts[0] () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + k * 0\nrun 123456789",
            (4, 13),
        ),
        // q13_thunk_record_own
        (
            "record Thunk { work: unit -> i64 }\ndef force :: own Thunk -> i64\nfn force t = t.work ()\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let k = { let text0 = to_string n; let text = ref text0; let mr = ref m; let t = Thunk { work: \\() -> { Mutex.with_lock mr (\\slot -> Array.write 0 (\\() -> String.length text) slot); 1 } }; force t }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + k * 0\nrun 123456789",
            (7, 13),
        ),
    ] {
        rejects_at(main, "E1013", "assignment_extends_borrow", at);
    }
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
