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

/// A31, review round 3 (finding 1): `Seq.next` runs the step function that the sequence holds, a
/// function value of the module: what that function stores (a borrow written into a `Mutex` that
/// it captured, or that an `Arc` it captured shares) stays in the cell, whether `Seq.next` is
/// called directly, by `Seq.to_array`, `Seq.map`, `Seq.unfold`, a `for` loop or a helper that
/// gets the sequence, and whether a lambda or a named function builds it. The twins whose text
/// outlives the cell stay accepted (values 9 in the `in_place_writes` suite).
#[test]
fn a_step_function_that_seq_next_runs_keeps_what_it_stores() {
    for (main, code, kind, at) in [
        // b02_seq_next_arc
        (
            "def run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let text0 = to_string n; let t = ref text0; let b = Arc.share (ref a); let s = Seq.defer (\\() -> { Mutex.with_lock (Arc.get (ref b)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); (Seq.empty(), Maybe.Some 1) }); let r = Seq.next s; () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (4, 5),
        ),
        // b04_seq_to_array_arc
        (
            "def run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let text0 = to_string n; let t = ref text0; let b = Arc.share (ref a); let s = Seq.defer (\\() -> { Mutex.with_lock (Arc.get (ref b)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); (Seq.empty(), Maybe.Some 1) }); let r = Seq.to_array s; () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (4, 5),
        ),
        // b05_seq_unfold_arc
        (
            "def run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let text0 = to_string n; let t = ref text0; let b = Arc.share (ref a); let s = Seq.unfold (\\k -> { Mutex.with_lock (Arc.get (ref b)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); if k < 1 then Maybe.Some (k, k + 1) else Maybe.None }) 0; let r = Seq.to_array s; () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (4, 5),
        ),
        // b06_seq_map_arc
        (
            "def run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let nums = [1, 2]\n    { let text0 = to_string n; let t = ref text0; let b = Arc.share (ref a); let s = Array.to_seq (ref nums) |> Seq.map (\\x -> { Mutex.with_lock (Arc.get (ref b)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); 1 }); let r = Seq.to_array s; () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (5, 5),
        ),
        // b07_seq_foreach
        (
            "def run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let mut total = 0\n    { let text0 = to_string n; let t = ref text0; let b = Arc.share (ref a); let s = Seq.defer (\\() -> { Mutex.with_lock (Arc.get (ref b)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); (Seq.empty(), Maybe.Some 1) }); for x in s do total = total + x }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0 + total * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (5, 5),
        ),
        // b08_seq_next_local_mutex
        (
            "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    { let text0 = to_string n; let t = ref text0; let s = Seq.defer (\\() -> { Mutex.with_lock mr (\\slot -> Array.write 0 (\\() -> String.length t) slot); (Seq.empty(), Maybe.Some 1) }); let r = Seq.next s; () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (5, 5),
        ),
        // c05_seq_next_fnparam
        (
            "def pump :: own Seq<i64> -> i64\nfn pump s =\n    match Seq.next s with\n    | (_, Maybe.Some v) -> v\n    | (_, Maybe.None) -> 0\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let text0 = to_string n; let t = ref text0; let b = Arc.share (ref a); let k = pump (Seq.defer (\\() -> { Mutex.with_lock (Arc.get (ref b)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); (Seq.empty(), Maybe.Some 1) })); () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // c06_seq_defer_named
        (
            "def stash :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> Seq<i64>\nfn stash b f = Seq.defer (\\() -> { Mutex.with_lock (Arc.get (ref b)) (\\slot -> Array.write 0 f slot); (Seq.empty(), Maybe.Some 1) })\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let text0 = to_string n; let t = ref text0; let s = stash (Arc.share (ref a)) (\\() -> String.length t); let r = Seq.next s; () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "E1013",
            "assignment_extends_borrow",
            (6, 5),
        ),
        // m07_seq_move_after (review round 1)
        (
            "def first :: own Seq<i64> -> i64\nfn first seq =\n    match Seq.next seq with\n    | (_, Maybe.Some v) -> v\n    | (_, Maybe.None) -> 0\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let text0 = to_string n\n    let text = ref text0\n    let mr = ref m\n    let seq = Seq.defer (\\() -> { Mutex.with_lock mr (\\slot -> Array.write 0 (\\() -> String.length text) slot); (Seq.empty(), Maybe.Some 1) })\n    let k = first seq\n    let moved = text0\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0 + k * 0 + moved.length * 0\nrun 123456789",
            "E1014",
            "borrow_conflict",
            (14, 17),
        ),
    ] {
        rejects_at(main, code, kind, at);
    }
    for main in [
        // b02_twin
        "def run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let text0 = to_string n\n    { let t = ref text0; let b = Arc.share (ref a); let s = Seq.defer (\\() -> { Mutex.with_lock (Arc.get (ref b)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); (Seq.empty(), Maybe.Some 1) }); let r = Seq.next s; () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
        // b07_twin
        "def run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let mut total = 0\n    let text0 = to_string n\n    { let t = ref text0; let b = Arc.share (ref a); let s = Seq.defer (\\() -> { Mutex.with_lock (Arc.get (ref b)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); (Seq.empty(), Maybe.Some 1) }); for x in s do total = total + x }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0 + total * 0\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 3 (finding 2): a lambda that hands a captured function value to a call of
/// another (`\() -> h g` in `twice`) stores nothing where its environment reaches: the function
/// value is stored into only by its own code. A module that writes a closure in place keeps such
/// combinators accepted in edition 2027, where the lambdas' parameters are read parameters.
#[test]
fn a_lambda_that_hands_a_captured_function_value_on_is_no_sink() {
    for main in [
        // f01_twice_no_owner
        "def twice :: ((unit -> i64) -> i64) -> (unit -> i64) -> i64\nfn twice h g = h (\\() -> h g) + h g\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    Array.write 0 (\\() -> 7) (ref mut arr)\n    let s = to_string n\n    let b = ref s\n    twice (\\a3 -> twice (\\a2 -> twice (\\a1 -> twice (\\a0 -> twice (\\x -> x ()) a0) a1) a2) a3) (\\() -> String.length b) + arr[0] ()\nrun 3",
        // f03_twice_two_levels
        "def twice :: ((unit -> i64) -> i64) -> (unit -> i64) -> i64\nfn twice h g = h (\\() -> h g) + h g\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    Array.write 0 (\\() -> 7) (ref mut arr)\n    let s = to_string n\n    let b = ref s\n    twice (\\a0 -> twice (\\x -> x ()) a0) (\\() -> String.length b) + arr[0] ()\nrun 3",
        // f06_min_1
        "def twice :: ((unit -> i64) -> i64) -> (unit -> i64) -> i64\nfn twice h g = h (\\() -> h g)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    Array.write 0 (\\() -> 7) (ref mut arr)\n    twice (\\a0 -> twice (\\x -> x ()) a0) (\\() -> n) + arr[0] ()\nrun 3",
        // f07_d
        "def bracket :: ((unit -> i64) -> i64) -> (unit -> i64) -> i64\nfn bracket body g = body (\\() -> body g)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    Array.write 0 (\\() -> 7) (ref mut arr)\n    let total = Array.map (\\i -> bracket (\\k -> bracket (\\x -> x () + i) k) (\\() -> i)) [1, 2, 3]\n    Array.sum (ref total) + arr[0] ()\nrun 3",
        // perf_twice_4 (review round 2)
        "def mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\nfn mk a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\ndef twice :: ((unit -> i64) -> i64) -> (unit -> i64) -> i64\nfn twice h g = h (\\() -> h g) + h g\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let w = mk (Arc.share (ref a))\n    w (\\() -> 7)\n    let s = to_string n\n    let b = ref s\n    twice (\\a3 -> twice (\\a2 -> twice (\\a1 -> twice (\\a0 -> twice (\\x -> x ()) a0) a1) a2) a3) (\\() -> String.length b)\nrun 3",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 3 (finding 3): with a `Drop.drop` that writes in the module, a record literal
/// of a reference and a lambda over one runs no code when it is dropped, whether the caller drops
/// it or the callee drops a part of it (a copy of `c.cb` that it calls, a drop-only store fact);
/// so does an immutable local bound to one. A `Stash` that the callee builds from the parts and
/// drops still stores (edition 2026: the callee writes through the record's `ref mut` field).
#[test]
fn dropping_a_literal_runs_only_what_its_parts_drop() {
    for main in [
        // g22_drop_hook_closure_buf
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\nrecord Ctx {r} { buf: ref mut {r} [(unit -> i64)..], cb: unit -> i64 }\ndef fill_with :: Ctx -> unit\nfn fill_with c = { let v = c.cb (); Array.write 0 (\\() -> v) c.buf }\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let u = { let _s = Stash { cell: Arc.share (ref a), f: \\() -> 4 }; 0 }\n    let name = to_string n\n    let nr = ref name\n    let mut arr = [\\() -> 0, \\() -> 0]\n    fill_with (Ctx { buf: ref mut arr, cb: \\() -> String.length nr })\n    let moved = name\n    arr[0] () + String.length moved + u + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 12345",
        // d03_let_bound_ctx
        "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\nrecord Ctx {r} { buf: ref mut {r} [(unit -> i64)..], cb: unit -> i64 }\ndef fill_with :: Ctx -> unit\nfn fill_with c = { let v = c.cb (); Array.write 0 (\\() -> v) c.buf }\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let u = { let _s = Stash { cell: Arc.share (ref a), f: \\() -> 4 }; 0 }\n    let name = to_string n\n    let nr = ref name\n    let mut arr = [\\() -> 0, \\() -> 0]\n    let c = Ctx { buf: ref mut arr, cb: \\() -> String.length nr }\n    fill_with c\n    let moved = name\n    arr[0] () + String.length moved + u + Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 12345",
    ] {
        analyze_with_edition(main, Edition::E2026)
            .unwrap_or_else(|error| panic!("{main}\n{}: {}", error.code, error.message));
    }
    // d01_callee_local_stash
    let main = "record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\\slot -> Array.write 0 g slot) }\n}\nrecord Parts { cell: Arc<Mutex<[unit -> i64]>>, g: unit -> i64 }\ndef build :: Parts -> i64\nfn build p = { let _s = Stash { cell: p.cell, f: p.g }; 0 }\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let text = to_string n; let t = ref text; let k = build (Parts { cell: Arc.share (ref a), g: \\() -> String.length t }); () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789";
    let error = analyze_with_edition(main, Edition::E2026).expect_err(main);
    assert_eq!(
        (error.code, error.kind, position(main, error.span.start)),
        ("E1013", Some("assignment_extends_borrow"), (11, 5)),
        "{main}\n{}",
        error.message
    );
}

/// A31, review round 3 (findings 5 and 6): with a function value that owns its cell in the
/// module (a registry, `make_registry`), a call reaches the hidden cell only through a value that
/// may share it: the handlers and thunks that a function returns, that `Array.map` makes or that a
/// `for` loop takes from such an array, a local bound to a named function or to a lambda over a
/// `ref string`, and an unrelated `Arc<Mutex<...>>` are none of them (`OwnerScan`). A copy of the
/// registry in an array, a record, a `Maybe`, a tuple, an `Owned.function`, a partial application,
/// a lambda or the result of a function that returns its argument still is.
#[test]
fn a_value_that_cannot_hold_an_owner_reaches_no_hidden_cell() {
    for main in [
        // s01_registry_handlers_result
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 0 f slot; 1 })\ndef handlers :: unit -> [(unit -> i64) -> i64]\nfn handlers _u = [\\f -> f () + 1, \\f -> f () * 2]\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let r = reg (\\() -> 5)\n    let hs = handlers ()\n    let text = to_string n\n    let tr = ref text\n    let a = hs[0] (\\() -> String.length tr)\n    let moved = text\n    a + moved.length + r + hs[1] (\\() -> 1)\nrun 123456789",
        // s02_registry_mapped_handlers
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 0 f slot; 1 })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let r = reg (\\() -> 5)\n    let ks = [1, 2, 3]\n    let hs = Array.map (\\k -> \\f -> f () + k) ks\n    let text = to_string n\n    let tr = ref text\n    let a = hs[0] (\\() -> String.length tr)\n    let moved = text\n    a + moved.length + r + hs[1] (\\() -> 1)\nrun 123456789",
        // s05_handler_record_result
        "record Handlers { on_text: (unit -> i64) -> i64 }\ndef make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 0 f slot; 1 })\ndef handlers :: unit -> Handlers\nfn handlers _u = Handlers { on_text: \\f -> f () + 1 }\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let r = reg (\\() -> 5)\n    let h = handlers ()\n    let text = to_string n\n    let tr = ref text\n    let a = h.on_text (\\() -> String.length tr)\n    let moved = text\n    a + moved.length + r + h.on_text (\\() -> 1)\nrun 123456789",
        // s06_iter_handlers
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 0 f slot; 1 })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let r = reg (\\() -> 5)\n    let ks = [1, 2, 3]\n    let hs = Array.map (\\k -> \\f -> f () + k) ks\n    let text = to_string n\n    let tr = ref text\n    let mut total = 0\n    for h in hs do\n        total = total + h (\\() -> String.length tr)\n    let moved = text\n    total + moved.length + r + hs[1] (\\() -> 1)\nrun 123456789",
        // s07_holder_thunks
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 0 f slot; 1 })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let notify = \\() -> reg (\\() -> 1)\n    let text = to_string n\n    let tr = ref text\n    let ks = [1, 2]\n    let thunks = Array.map (\\k -> \\() -> String.length tr + k) ks\n    let a = thunks[0] ()\n    let moved = text\n    a + moved.length + notify ()\nrun 123456789",
        // s09_listeners_vec
        "def make_bus :: unit -> ((unit -> i64) -> i64)\nfn make_bus _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0, \\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 1 f slot; slot[0] () })\ndef run :: i64 -> i64\nfn run n =\n    let subscribe = make_bus ()\n    let r = subscribe (\\() -> 5)\n    let ks = [1, 2, 3]\n    let hs = Array.map (\\k -> \\f -> f () + k) ks\n    let text = to_string n\n    let tr = ref text\n    let mut total = 0\n    for i in 0 ..= 2 do\n        total = total + hs[i] (\\() -> String.length tr)\n    let moved = text\n    total + moved.length + r + hs[0] (\\() -> 0)\nrun 123456789",
        // s13_unrelated_function
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 0 f slot; 1 })\ndef notifier :: ((unit -> i64) -> i64) -> (unit -> i64)\nfn notifier reg = \\() -> reg (\\() -> 1)\ndef lengths :: i64 -> i64\nfn lengths n =\n    let base = \\() -> 7\n    let text = to_string n\n    let tr = ref text\n    let ks = [1, 2]\n    let thunks = Array.map (\\k -> \\() -> String.length tr + k) ks\n    let a = thunks[0] ()\n    let moved = text\n    a + moved.length + base ()\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let notify = notifier reg\n    lengths n + notify ()\nrun 123456789",
        // s15_named_base
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 0 f slot; 1 })\ndef notifier :: ((unit -> i64) -> i64) -> (unit -> i64)\nfn notifier reg = \\() -> reg (\\() -> 1)\ndef seven :: unit -> i64\nfn seven _u = 7\ndef lengths :: i64 -> i64\nfn lengths n =\n    let base = seven\n    let text = to_string n\n    let tr = ref text\n    let ks = [1, 2]\n    let thunks = Array.map (\\k -> \\() -> String.length tr + k) ks\n    let a = thunks[0] ()\n    let moved = text\n    a + moved.length + base ()\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let notify = notifier reg\n    lengths n + notify ()\nrun 123456789",
        // s16_cell_local
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 0 f slot; 1 })\ndef notifier :: ((unit -> i64) -> i64) -> (unit -> i64)\nfn notifier reg = \\() -> reg (\\() -> 1)\ndef lengths :: i64 -> i64\nfn lengths n =\n    let counter = Arc.new (Mutex.create [\\() -> 7])\n    let text = to_string n\n    let tr = ref text\n    let ks = [1, 2]\n    let thunks = Array.map (\\k -> \\() -> String.length tr + k) ks\n    let a = thunks[0] ()\n    let moved = text\n    a + moved.length + Mutex.with_lock (Arc.get (ref counter)) (\\slot -> slot[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let notify = notifier reg\n    lengths n + notify ()\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    for (main, at) in [
        // p02_array_map_const
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let ks = [1, 2]\n    let hs = Array.map (\\k -> reg) (ref ks)\n    let first = { let text0 = to_string n; let text = ref text0; hs[0] (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (10, 17),
        ),
        // p03_identity_fn
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef id_fn :: ((unit -> i64) -> i64) -> ((unit -> i64) -> i64)\nfn id_fn x = x\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h = id_fn reg\n    let first = { let text0 = to_string n; let text = ref text0; h (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p04_record_fn
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Handlers { on_text: (unit -> i64) -> i64 }\ndef wrap :: ((unit -> i64) -> i64) -> Handlers\nfn wrap r = Handlers { on_text: r }\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let r = wrap reg\n    let first = { let text0 = to_string n; let text = ref text0; r.on_text (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 17),
        ),
        // p07_maybe_match
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let m = Maybe.Some reg\n    let first = { let text0 = to_string n; let text = ref text0; match m with\n        | Maybe.Some h -> h (\\() -> String.length text)\n        | Maybe.None -> 0 }\n    let other = [to_string 777777, to_string 88888888]\n    first + reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (9, 17),
        ),
        // q02_array_map_const
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let ks = [1, 2]\n    let hs = Array.map (\\k -> reg) (ref ks)\n    let first = { let text0 = to_string n; let text = ref text0; reg (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + hs[0] (\\() -> 1) + other.length * 0\nrun 123456789",
            (10, 17),
        ),
        // q13_holder_thunk
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let notify = \\() -> reg (\\() -> 1)\n    let first = { let text0 = to_string n; let text = ref text0; reg (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + notify () + other.length * 0\nrun 123456789",
            (9, 17),
        ),
        // q16_tuple
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let pair = (reg, 1)\n    let first = { let text0 = to_string n; let text = ref text0; reg (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + (match pair with\n        | (h, _) -> h (\\() -> 1)) + other.length * 0\nrun 123456789",
            (9, 17),
        ),
        // q17_owned_function
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let w = Owned.function reg\n    let first = { let text0 = to_string n; let text = ref text0; reg (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + Owned.call (ref w) (\\() -> 1) + other.length * 0\nrun 123456789",
            (9, 17),
        ),
        // q18_partial_apply
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef call_with :: ((unit -> i64) -> i64) -> (unit -> i64) -> i64\nfn call_with r f = r f\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h = call_with reg\n    let first = { let text0 = to_string n; let text = ref text0; reg (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + h (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // q20_mapi_branch
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let ks = [1, 2]\n    let hs = Array.mapi (\\i k -> if i == 0 then reg else (\\f -> k)) (ref ks)\n    let first = { let text0 = to_string n; let text = ref text0; reg (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + hs[0] (\\() -> 1) + other.length * 0\nrun 123456789",
            (10, 17),
        ),
    ] {
        rejects_at(main, "E1013", "assignment_extends_borrow", at);
    }
}

/// A31, review round 4 (finding 1): an implicit drop runs `Drop.drop` with the dropped value, so
/// it applies what that body stores in the environments that the check cannot name
/// (`StoreFacts::hidden`): a `Drop.drop` that calls `s.out`, a copy of a function value that owns
/// the cell that it writes (`make_registry`), with what the value holds puts that into the cell
/// that every other copy (`reg`) reaches, and one that hands it a borrow of the value itself
/// (`ref s.name`) puts a borrow of the dropped value there. Each program below reads the stored
/// closure through `reg` after its borrowed string is gone (the controls, which keep the string
/// alive, give 9); main and the previous rounds accepted them. The bound, temporary, by-value,
/// `Owned.Function`, assigned-over and overwritten forms, a borrow that enters the value's own
/// cell before the drop, and one that is there from the start are all rejected where the string
/// is dropped. The values that cannot share such a cell, and the stores of owned values, stay
/// accepted.
#[test]
fn an_implicit_drop_applies_what_its_drop_stores_in_hidden_environments() {
    for (main, at) in [
        // d12_drop_calls_owned_sink
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let _s = Stash { f: \\() -> String.length t, out: reg }; () }\n    let other = [to_string 777777, to_string 88888888]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 5),
        ),
        // d13_drop_owned_sink_temp
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef size :: ref Stash -> i64\nfn size s = 1\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let k = { let text0 = to_string n; let t = ref text0; size (ref (Stash { f: \\() -> String.length t, out: reg })) }\n    let other = [to_string 777777, to_string 88888888]\n    reg (\\() -> 1) + other.length * 0 + k * 0\nrun 123456789",
            (14, 13),
        ),
        // d14_drop_owned_sink_param
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef consume :: Stash -> i64\nfn consume s = 1\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let k = { let text0 = to_string n; let t = ref text0; consume (Stash { f: \\() -> String.length t, out: reg }) }\n    let other = [to_string 777777, to_string 88888888]\n    reg (\\() -> 1) + other.length * 0 + k * 0\nrun 123456789",
            (14, 13),
        ),
        // d15_drop_owned_function
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: Owned.Function<(unit -> i64), i64> }\ninstance Drop<Stash> {\n    fn drop s = { let _k = Owned.call (ref s.out) s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let _s = Stash { f: \\() -> String.length t, out: Owned.function reg }; () }\n    let other = [to_string 777777, to_string 88888888]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 5),
        ),
        // d21_drop_assign_over
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let mut s = Stash { f: \\() -> 0, out: reg }\n    { let text0 = to_string n; let t = ref text0; s = Stash { f: \\() -> String.length t, out: reg }; s = Stash { f: \\() -> 1, out: reg }; () }\n    let other = [to_string 777777, to_string 88888888]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (13, 5),
        ),
        // d09_grow_owned_writer
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { inbox: Arc<Mutex<[unit -> i64]>>, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = Mutex.with_lock (Arc.get (ref s.inbox)) (\\src -> { let g = src[0]; o g }); () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let s = Stash { inbox: Arc.new (Mutex.create [\\() -> 0]), out: reg }; Mutex.with_lock (Arc.get (ref s.inbox)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); () }\n    let other = [to_string 777777, to_string 88888888]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 5),
        ),
        // d11_nogrow_owned_writer
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { inbox: Arc<Mutex<[unit -> i64]>>, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = Mutex.with_lock (Arc.get (ref s.inbox)) (\\src -> { let g = src[0]; o g }); () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let s = Stash { inbox: Arc.new (Mutex.create [\\() -> String.length t]), out: reg }; () }\n    let other = [to_string 777777, to_string 88888888]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 5),
        ),
        // d22_drop_array_write_old
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let mut arr = [Stash { f: \\() -> String.length t, out: reg }]; Array.write 0 (Stash { f: \\() -> 1, out: reg }) (ref mut arr); () }\n    let other = [to_string 777777, to_string 88888888]\n    reg (\\() -> 3) + other.length * 0\nrun 123456789",
            (12, 5),
        ),
        // d23_drop_self_borrow_owned
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Named { name: string, out: (unit -> i64) -> i64 }\ninstance Drop<Named> {\n    fn drop s = { let nm = ref s.name; let o = s.out; let _k = o (\\() -> String.length nm); () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let _s = Named { name: to_string n, out: reg }; () }\n    let other = [to_string 12345678, to_string 87654321, to_string 1234567]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 5),
        ),
    ] {
        rejects_at(main, "E1013", "assignment_extends_borrow", at);
    }
    // d20_drop_owned_via_wrapper_fn: in edition 2027 the lambda lent to the read parameter of
    // `wrap` is a borrowed temporary that the record keeps (E1013 `borrow_of_temporary`, as main).
    let wrapper = "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef wrap :: ((unit -> i64) -> i64) -> (unit -> i64) -> Stash\nfn wrap reg f = Stash { f: f, out: reg }\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let _s = wrap reg (\\() -> String.length t); () }\n    let other = [to_string 777777, to_string 88888888]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789";
    let error = analyze_with_edition(wrapper, Edition::E2026).expect_err(wrapper);
    assert_eq!(
        (error.code, error.kind, position(wrapper, error.span.start)),
        ("E1013", Some("assignment_extends_borrow"), (14, 5)),
        "{}",
        error.message
    );
    // s02_drop_self_temp: a temporary whose drop hands a borrow of itself to the registry.
    rejects_at(
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Named { name: string, out: (unit -> i64) -> i64 }\ninstance Drop<Named> {\n    fn drop s = { let nm = ref s.name; let o = s.out; let _k = o (\\() -> String.length nm); () }\n}\ndef consume :: Named -> i64\nfn consume s = 1\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let k = consume (Named { name: to_string n, out: reg })\n    let other = [to_string 12345678, to_string 87654321]\n    k + reg (\\() -> 1) + other.length * 0\nrun 123456789",
        "E1013",
        "store_local_borrow_through_reference",
        (14, 13),
    );
    for main in [
        // d12_ctl
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let text0 = to_string n\n    { let t = ref text0; let _s = Stash { f: \\() -> String.length t, out: reg }; () }\n    reg (\\() -> 1)\nrun 123456789",
        // d10_grow_owned_writer_ctl
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { inbox: Arc<Mutex<[unit -> i64]>>, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = Mutex.with_lock (Arc.get (ref s.inbox)) (\\src -> { let g = src[0]; o g }); () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let text0 = to_string n\n    { let t = ref text0; let s = Stash { inbox: Arc.new (Mutex.create [\\() -> 0]), out: reg }; Mutex.with_lock (Arc.get (ref s.inbox)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); () }\n    reg (\\() -> 1)\nrun 123456789",
        // s01_drop_owned_temp
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef consume :: Stash -> i64\nfn consume s = 1\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let k = consume (Stash { f: \\() -> 7, out: reg })\n    k + reg (\\() -> 1)\nrun 5",
        // s03_drop_self_noowner
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Named { name: string, out: (unit -> i64) -> i64 }\ninstance Drop<Named> {\n    fn drop s = { let nm = ref s.name; let o = s.out; let _k = o (\\() -> String.length nm); () }\n}\ndef consume :: Named -> i64\nfn consume s = 1\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let k = consume (Named { name: to_string n, out: \\f -> f () })\n    { let _s = Named { name: to_string 42, out: \\f -> f () + 1 }; () }\n    k + reg (\\() -> 1)\nrun 123456789",
        // s04_drop_owned_bound
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let _s = Stash { f: \\() -> 7, out: reg }; () }\n    reg (\\() -> 1)\nrun 5",
        // s05_drop_self_bound_outer
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Named { name: string, out: (unit -> i64) -> i64 }\ninstance Drop<Named> {\n    fn drop s = { let nm = ref s.name; let o = s.out; let _k = o (\\() -> String.length nm); () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let s = Named { name: to_string n, out: make_registry () }\n    3\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 4 (finding 4): inside `Drop.drop`, `Arc.share (ref s.out)` is an owner of the
/// block that `s.out` shares, no borrow of the dropped value: storing what `s.inbox` holds into
/// that block through the share stores what the value holds, so the bound value keeps no borrow of
/// itself in the block (`drop_borrows_self`), and the program, whose string outlives the read,
/// is accepted (9) as main accepts it. The same drop written through a helper was accepted
/// before; the form whose string dies first stays rejected. The values of the accepted forms are
/// checked by the `in_place_writes` suite of `tests/features.mjs` (natively: WebAssembly without
/// the tzroutine runtime traps on the drop's second lock).
#[test]
fn a_share_of_a_block_of_the_dropped_value_is_no_borrow_of_it() {
    for main in [
        // d06_inbox_grow_drop_ctl
        "record Stash { inbox: Arc<Mutex<[unit -> i64]>>, out: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Stash> {\n    fn drop s = { let out = Arc.share (ref s.out); Mutex.with_lock (Arc.get (ref s.inbox)) (\\src -> { let g = src[0]; Mutex.with_lock (Arc.get (ref out)) (\\dst -> Array.write 0 g dst) }) }\n}\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let text0 = to_string n\n    { let t = ref text0; let s = Stash { inbox: Arc.new (Mutex.create [\\() -> 0]), out: Arc.share (ref a) }; Mutex.with_lock (Arc.get (ref s.inbox)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); () }\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
        // d08_inbox_transfer_helper_ctl
        "record Stash { inbox: Arc<Mutex<[unit -> i64]>>, out: Arc<Mutex<[unit -> i64]>> }\ndef transfer :: ref Arc<Mutex<[unit -> i64]>> -> ref Arc<Mutex<[unit -> i64]>> -> unit\nfn transfer src dst = Mutex.with_lock (Arc.get src) (\\a -> { let g = a[0]; Mutex.with_lock (Arc.get dst) (\\b -> Array.write 0 g b) })\ninstance Drop<Stash> {\n    fn drop s = transfer (ref s.inbox) (ref s.out)\n}\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let text0 = to_string n\n    { let t = ref text0; let s = Stash { inbox: Arc.new (Mutex.create [\\() -> 0]), out: Arc.share (ref a) }; Mutex.with_lock (Arc.get (ref s.inbox)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); () }\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 123456789",
        // s06_rc_share_drop
        "record Stash { inbox: Rc<[unit -> i64]>, out: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Stash> {\n    fn drop s = { let keep = Rc.share (ref s.inbox); let out = Arc.share (ref s.out); Mutex.with_lock (Arc.get (ref out)) (\\dst -> Array.write 0 (\\() -> 5) dst) }\n}\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let s = Stash { inbox: Rc.new [\\() -> 1], out: Arc.share (ref a) }; () }\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    // d05_inbox_grow_drop
    rejects_at(
        "record Stash { inbox: Arc<Mutex<[unit -> i64]>>, out: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Stash> {\n    fn drop s = { let out = Arc.share (ref s.out); Mutex.with_lock (Arc.get (ref s.inbox)) (\\src -> { let g = src[0]; Mutex.with_lock (Arc.get (ref out)) (\\dst -> Array.write 0 g dst) }) }\n}\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    { let text0 = to_string n; let t = ref text0; let s = Stash { inbox: Arc.new (Mutex.create [\\() -> 0]), out: Arc.share (ref a) }; Mutex.with_lock (Arc.get (ref s.inbox)) (\\slot -> Array.write 0 (\\() -> String.length t) slot); () }\n    let other = [to_string 777777, to_string 88888888]\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
        "E1013",
        "assignment_extends_borrow",
        (8, 5),
    );
}

/// A31, review round 4 (findings 3 and 5): a mutable parameter (`fn renew mut a`, `\mut a ->`)
/// may be assigned anything in the body, so it holds what its last assignment holds, not the
/// caller's argument: a registry returned through one is a copy of the registry, and the borrow
/// that a call of it stores stays in the registry's cell (main and the previous round accepted
/// these; the round before rejected them where they are rejected here). The builtins of `Rc`
/// give what their argument holds (`Rc.get (ref (Rc.new h))` is `h`), so a handler read from an
/// `Rc` that holds a capture-less lambda shares no cell (14, as main; the `in_place_writes` suite
/// checks the value of the same form).
#[test]
fn a_mutable_parameter_holds_what_the_body_assigns() {
    for (main, at) in [
        // m03_mut_param_fresh
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef renew :: own ((unit -> i64) -> i64) -> ((unit -> i64) -> i64)\nfn renew mut a = { a = make_registry (); a }\ndef run :: i64 -> i64\nfn run n =\n    let h = renew (\\_f -> 1)\n    let first = { let text0 = to_string n; let text = ref text0; h (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + h (\\() -> 1) + other.length * 0\nrun 123456789",
            (10, 17),
        ),
        // m02_mut_param_own
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef pick :: own ((unit -> i64) -> i64) -> own ((unit -> i64) -> i64) -> ((unit -> i64) -> i64)\nfn pick mut a b = { a = b; a }\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h = pick (\\f -> 1) reg\n    let first = { let text0 = to_string n; let text = ref text0; h (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // m04_mut_lambda_param
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef get :: own (((unit -> i64) -> i64) -> ((unit -> i64) -> i64)) -> ((unit -> i64) -> i64)\nfn get f = f (\\_g -> 1)\ndef run :: i64 -> i64\nfn run n =\n    let h = get (\\mut a -> { a = make_registry (); a })\n    let first = { let text0 = to_string n; let text = ref text0; h (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + h (\\() -> 1) + other.length * 0\nrun 123456789",
            (10, 17),
        ),
    ] {
        rejects_at(main, "E1013", "assignment_extends_borrow", at);
    }
    // m01_mut_param: edition 2027 rejects the lambda lent to the read parameter first.
    let pick = "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef pick :: ((unit -> i64) -> i64) -> ((unit -> i64) -> i64) -> ((unit -> i64) -> i64)\nfn pick mut a b = { a = b; a }\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h = pick (\\f -> 1) reg\n    let first = { let text0 = to_string n; let text = ref text0; h (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + reg (\\() -> 1) + other.length * 0\nrun 123456789";
    let error = analyze_with_edition(pick, Edition::E2026).expect_err(pick);
    assert_eq!(
        (error.code, error.kind, position(pick, error.span.start)),
        ("E1013", Some("assignment_extends_borrow"), (11, 17)),
        "{}",
        error.message
    );
    // u_rc_get
    accepts_in_both_editions(&[(
        "Main.tz",
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let r = Rc.new (\\f -> f () + 1)\n    let h = deref (Rc.get (ref r))\n    let text = to_string n\n    let first = { let t = ref text; h (\\() -> String.length t) }\n    let moved = text\n    first + moved.length + reg (\\() -> 3) + reg (\\() -> 1)\nrun 12345",
    )]);
}

/// A31, review round 5 (finding 3): a local that a pattern binds holds what the projection of the
/// matched value holds (the provenance of the cell-owning function values, `OwnerScan`): a handler
/// taken out of an `Rc` with `Rc.try_unwrap` and a `match`, out of a `Maybe`, a tuple, a record, a
/// list, an array element, a nested pattern, an OR pattern, behind a guard or a `ref`, or through
/// an active pattern that returns its argument, is the capture-less lambda that was put in, and
/// shares no cell with the registry (main accepts these with the values 9, 14 and 10; the branch
/// counted every pattern local as an owner before). The same shapes that take the registry out
/// (or a registry that an active pattern or a named function makes, or one of the alternatives
/// of an OR pattern) are rejected where the borrowed string is dropped (main accepts them).
#[test]
fn a_local_that_a_pattern_binds_holds_what_the_matched_value_holds() {
    for main in [
        // ok01_rc_try_unwrap (review round 5)
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let r = Rc.new h\n    let c = match Rc.try_unwrap r with | Result.Ok v -> v | Result.Error _ -> \\f -> 0\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + reg (\\() -> 1) + other.length * 0\nrun 123456789",
        // u_maybe_map (review round 4)
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let m = Maybe.map (\\k -> \\f -> f () + k) (Maybe.Some 1)\n    let h = match m with | Maybe.Some g -> g | Maybe.None -> \\f -> 0\n    let text = to_string n\n    let first = { let t = ref text; h (\\() -> String.length t) }\n    let moved = text\n    first + moved.length + reg (\\() -> 3) + reg (\\() -> 1)\nrun 12345",
        // s01_maybe_some
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let m = Maybe.Some h\n    let c = match m with | Maybe.Some s -> s | Maybe.None -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
        // s02_tuple
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let pair = (h, h)\n    let c = match pair with | (a, b) -> b\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
        // s03_or_pattern
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let x: Result<(unit -> i64) -> i64, (unit -> i64) -> i64> = Result.Error h\n    let c = match x with | Result.Ok g | Result.Error g -> g\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
        // s05_record
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Two { f: (unit -> i64) -> i64, g: (unit -> i64) -> i64 }\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let b = Two { f: h, g: h }\n    let c = match b with | { g = x } -> x\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
        // s06_cons
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let l = [|h, h|]\n    let c = match l with | a :: rest -> (match rest with | b :: _ -> b | _ -> a) | _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
        // s07_try_unwrap
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let r = Rc.new h\n    let c = match Rc.try_unwrap r with | Result.Ok v -> v | Result.Error _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
        // s09_nested
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let m = Maybe.Some (Maybe.Some h)\n    let c = match m with | Maybe.Some (Maybe.Some s) -> s | _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
        // s10_guard
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let m = Maybe.Some h\n    let c = match m with | Maybe.Some s when n > 0 -> s | _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
        // s11_ref_scrutinee
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let m = Maybe.Some h\n    let c = match ref m with | Maybe.Some s -> s | Maybe.None -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
        // s12_array_elem_tuple
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let pairs = [(h, h)]\n    let c = match pairs[0] with | (a, b) -> b\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
        // s13_active_payload
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef (|Wrap|_|) :: own ((unit -> i64) -> i64) -> Maybe<(unit -> i64) -> i64>\nfn (|Wrap|_|) g = Maybe.Some g\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let src = h\n    let c = match src with | Wrap s -> s | _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    for (main, at) in [
        // p01_maybe_some
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let m = Maybe.Some reg\n    let c = match m with | Maybe.Some s -> s | Maybe.None -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p02_tuple
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let pair = (h, reg)\n    let c = match pair with | (a, b) -> b\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p03_or_pattern
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let x: Result<(unit -> i64) -> i64, (unit -> i64) -> i64> = Result.Error reg\n    let c = match x with | Result.Ok g | Result.Error g -> g\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p04_or_pattern_first
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let x: Result<(unit -> i64) -> i64, (unit -> i64) -> i64> = Result.Ok reg\n    let c = match x with | Result.Ok g | Result.Error g -> g\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p05_record
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Two { f: (unit -> i64) -> i64, g: (unit -> i64) -> i64 }\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let b = Two { f: h, g: reg }\n    let c = match b with | { g = x } -> x\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 17),
        ),
        // p06_cons
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let l = [|h, reg|]\n    let c = match l with | a :: rest -> (match rest with | b :: _ -> b | _ -> a) | _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p07_try_unwrap
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let r = Rc.new reg\n    let c = match Rc.try_unwrap r with | Result.Ok v -> v | Result.Error _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p08_upgrade
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let r = Rc.new reg\n    let w = Rc.downgrade (ref r)\n    let c = match Rc.upgrade (ref w) with | Maybe.Some s -> deref (Rc.get (ref s)) | Maybe.None -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 17),
        ),
        // p09_nested
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let m = Maybe.Some (Maybe.Some reg)\n    let c = match m with | Maybe.Some (Maybe.Some s) -> s | _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p10_guard
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let m = Maybe.Some reg\n    let c = match m with | Maybe.Some s when n > 0 -> s | _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p11_ref_scrutinee
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let m = Maybe.Some reg\n    let c = match ref m with | Maybe.Some s -> s | Maybe.None -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p12_array_elem_tuple
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let pairs = [(h, reg)]\n    let c = match pairs[0] with | (a, b) -> b\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
        ),
        // p13_active_payload
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef (|Wrap|_|) :: own ((unit -> i64) -> i64) -> Maybe<(unit -> i64) -> i64>\nfn (|Wrap|_|) g = Maybe.Some g\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let src = reg\n    let c = match src with | Wrap s -> s | _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (13, 17),
        ),
        // p14_match_in_lambda_result
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let m = Maybe.Some reg\n    let pick = \\(u: i64) -> match m with | Maybe.Some s -> s | Maybe.None -> h\n    let c = pick 0\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 17),
        ),
        // p15_active_makes_registry
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef (|Reg|_|) :: own i64 -> Maybe<(unit -> i64) -> i64>\nfn (|Reg|_|) _k = Maybe.Some (make_registry ())\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let c = match 0 with | Reg r -> r | _ -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 17),
        ),
        // p16_named_result_registry
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef wrap :: unit -> Maybe<(unit -> i64) -> i64>\nfn wrap _u = Maybe.Some (make_registry ())\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let c = match wrap () with | Maybe.Some s -> s | Maybe.None -> h\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (12, 17),
        ),
        // p17_or_pattern_mixed
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let h: (unit -> i64) -> i64 = \\f -> f ()\n    let x: Result<(unit -> i64) -> i64, (unit -> i64) -> i64> = if n > 0 then Result.Error reg else Result.Ok h\n    let c = match x with | Result.Ok g | Result.Error g -> g\n    let first = { let text0 = to_string n; let text = ref text0; c (\\() -> String.length text) }\n    let other = [to_string 777777, to_string 88888888]\n    first + c (\\() -> 1) + other.length * 0\nrun 123456789",
            (11, 17),
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

/// A31, review round 6 (finding 1): a share of the block of a local `Rc` or `Arc` owner that
/// goes to the caller's data, or to a hidden environment, takes what the owner holds then and what
/// the body writes into the block's cell later, through the owner, another share, a borrow of the
/// contents, an upgraded weak pointer or the value that the owner moved into (`export_block`,
/// `store_exported`). Before, the later writes stayed in the local owner: the caller's array kept
/// a borrow of a callee local (`sh01`, freed memory at `-O0`) or of a caller value that dies first
/// (`sh11`, `0` in place of `9`).
#[test]
fn a_share_of_a_local_block_takes_what_the_body_writes_into_it_later() {
    for (source, kind, at) in [
        // sh01_share_then_cell_write_callee
        (
            "def put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    let s2 = Rc.share (ref lr)\n    Array.write 0 s2 slot\n    let local = to_string 123456789\n    let lt = ref local\n    Mutex.with_lock (Rc.get (ref lr)) (\\cell -> Array.write 0 (\\() -> String.length lt) cell)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (8, 5),
        ),
        // sh03_share_then_cell_write_callee_arc
        (
            "def put :: ref mut [Arc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let lr = Arc.new (Mutex.create [\\() -> 0])\n    let s2 = Arc.share (ref lr)\n    Array.write 0 s2 slot\n    let local = to_string 123456789\n    let lt = ref local\n    Mutex.with_lock (Arc.get (ref lr)) (\\cell -> Array.write 0 (\\() -> String.length lt) cell)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Arc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Arc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (8, 5),
        ),
        // sh09_lock_param
        (
            "def put :: ref Mutex<[Arc<Mutex<[unit -> i64]>>]> -> unit\nfn put m =\n    let lr = Arc.new (Mutex.create [\\() -> 0])\n    let s2 = Arc.share (ref lr)\n    Mutex.with_lock m (\\slot -> Array.write 0 s2 slot)\n    let local = to_string 123456789\n    let lt = ref local\n    Mutex.with_lock (Arc.get (ref lr)) (\\cell -> Array.write 0 (\\() -> String.length lt) cell)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [Arc.new (Mutex.create [\\() -> 0])]\n    put (ref m)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (ref m) (\\slot -> Mutex.with_lock (Arc.get (ref slot[0])) (\\cell -> cell[0] ())) + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (8, 5),
        ),
        // sh11_param_borrow_into_shared_cell
        (
            "def put :: ref string -> ref mut [Arc<Mutex<[unit -> i64]>>..] -> unit\nfn put text slot =\n    let lr = Arc.new (Mutex.create [\\() -> 0])\n    Array.write 0 (Arc.share (ref lr)) slot\n    Mutex.with_lock (Arc.get (ref lr)) (\\cell -> Array.write 0 (\\() -> String.length text) cell)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Arc.new (Mutex.create [\\() -> 0])]\n    { let text0 = to_string n; put (ref text0) (ref mut arr) }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Arc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // sh20_generic_set
        (
            "def put :: ref string -> ref mut [Arc<Mutex<[unit -> i64]>>..] -> unit\nfn put text slot =\n    let lr = Arc.new (Mutex.create [\\() -> 0])\n    Array.set 0 (Arc.share (ref lr)) slot\n    Mutex.with_lock (Arc.get (ref lr)) (\\cell -> Array.set 0 (\\() -> String.length text) cell)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Arc.new (Mutex.create [\\() -> 0])]\n    { let text0 = to_string n; put (ref text0) (ref mut arr) }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Arc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // sh22_lambda_capture_lock
        (
            "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [Arc.new (Mutex.create [\\() -> 0])]\n    let mr = ref m\n    let put = \\(text: ref string) -> { let lr = Arc.new (Mutex.create [\\() -> 0]); let lrs = Arc.share (ref lr); Mutex.with_lock mr (\\s -> Array.write 0 lrs s); Mutex.with_lock (Arc.get (ref lr)) (\\c -> Array.write 0 (\\() -> String.length text) c) }\n    { let text0 = to_string n; put (ref text0) }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (ref m) (\\s -> Mutex.with_lock (Arc.get (ref s[0])) (\\c -> c[0] ())) + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (6, 5),
        ),
        // v02_export_then_move_then_write
        (
            "def put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    Array.write 0 (Rc.share (ref lr)) slot\n    let lr2 = lr\n    let local = to_string 123456789\n    let lt = ref local\n    Mutex.with_lock (Rc.get (ref lr2)) (\\cell -> Array.write 0 (\\() -> String.length lt) cell)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (8, 5),
        ),
        // v03_share_made_before_written_after
        (
            "def put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    let s3 = Rc.share (ref lr)\n    Array.write 0 (Rc.share (ref lr)) slot\n    let local = to_string 123456789\n    let lt = ref local\n    Mutex.with_lock (Rc.get (ref s3)) (\\cell -> Array.write 0 (\\() -> String.length lt) cell)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (8, 5),
        ),
        // v07_export_then_write_via_get_alias
        (
            "def put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    let c = Rc.get (ref lr)\n    Array.write 0 (Rc.share (ref lr)) slot\n    let local = to_string 123456789\n    let lt = ref local\n    Mutex.with_lock c (\\cell -> Array.write 0 (\\() -> String.length lt) cell)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (8, 5),
        ),
        // v08_export_weak_then_upgrade_write
        (
            "def put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    let w = Rc.downgrade (ref lr)\n    Array.write 0 (Rc.share (ref lr)) slot\n    let local = to_string 123456789\n    let lt = ref local\n    match Rc.upgrade (ref w) with\n    | Maybe.Some r -> Mutex.with_lock (Rc.get (ref r)) (\\cell -> Array.write 0 (\\() -> String.length lt) cell)\n    | Maybe.None -> ()\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (9, 23),
        ),
        // v09_export_in_record_then_write
        (
            "record H { r: Rc<Mutex<[unit -> i64]>> }\ndef put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let h = H { r: Rc.new (Mutex.create [\\() -> 0]) }\n    Array.write 0 (Rc.share (ref h.r)) slot\n    let local = to_string 123456789\n    let lt = ref local\n    Mutex.with_lock (Rc.get (ref h.r)) (\\cell -> Array.write 0 (\\() -> String.length lt) cell)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (8, 5),
        ),
    ] {
        rejects_at(source, "E1013", kind, at);
    }
    for main in [
        // sh21_alive_ctl
        "def put :: ref string -> ref mut [Arc<Mutex<[unit -> i64]>>..] -> unit\nfn put text slot =\n    let lr = Arc.new (Mutex.create [\\() -> 0])\n    Array.write 0 (Arc.share (ref lr)) slot\n    Mutex.with_lock (Arc.get (ref lr)) (\\cell -> Array.write 0 (\\() -> String.length text) cell)\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let mut arr = [Arc.new (Mutex.create [\\() -> 0])]\n    put (ref text0) (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Arc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0\nrun 123456789",
        // v05_safe_export_owned_write
        "def put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    Array.write 0 (Rc.share (ref lr)) slot\n    Mutex.with_lock (Rc.get (ref lr)) (\\cell -> Array.write 0 (\\() -> 5) cell)\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\cell -> cell[0] ()) + n\nrun 2",
        // v06_safe_export_only
        "def put :: ref mut [Arc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let lr = Arc.new (Mutex.create [\\() -> 3])\n    Array.write 0 (Arc.share (ref lr)) slot\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Arc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    Mutex.with_lock (Arc.get (ref arr[0])) (\\cell -> cell[0] ()) + n\nrun 4",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 6 (finding 5): the share or the weak pointer of a block that a body drops
/// hands what the block holds to the hidden environments, not a borrow of the local owner
/// (`expand_shares`), and a weak pointer runs no `Drop.drop` of the block's value
/// (`Type::drops_some`): the registry outlives the inner block that declares the owner (`s13`,
/// rejected before, accepted by main), and the borrowed string still outlives the registry where
/// it must (`sd01`, `u01` to `u04`).
#[test]
fn a_dropped_share_or_weak_pointer_keeps_no_borrow_of_its_owner() {
    for main in [
        // sd02_ctl
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { cell: Rc<Mutex<[unit -> i64]>>, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; Mutex.with_lock (Rc.get (ref s.cell)) (\\c -> { let _k = o c[0]; () }) }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let r = Rc.new (Mutex.create [\\() -> 0]); let _s = Stash { cell: Rc.share (ref r), out: reg }; Mutex.with_lock (Rc.get (ref r)) (\\c -> Array.write 0 (\\() -> 9) c) }\n    let other = [to_string 12345678, to_string 87654321, to_string 1234567]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
        // s03_rc_safe
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let reg = make_registry ()\n    { let r = Rc.new (Stash { f: \\() -> String.length t, out: reg }); let _r2 = Rc.share (ref r); () }\n    reg (\\() -> 1)\nrun 123456789",
        // s08_rc_share_noreg_after
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let reg = make_registry ()\n    { let r = Rc.new (Stash { f: \\() -> String.length t, out: reg }); let _r2 = Rc.share (ref r); () }\n    7\nrun 123456789",
        // s10_rc_downgrade_safe
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let reg = make_registry ()\n    { let r = Rc.new (Stash { f: \\() -> String.length t, out: reg }); let _w = Rc.downgrade (ref r); () }\n    reg (\\() -> 1)\nrun 123456789",
        // s11_arc_share_borrow_safe
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let reg = make_registry ()\n    { let r = Arc.new (Stash { f: \\() -> String.length t, out: reg }); let _r2 = Arc.share (ref r); () }\n    reg (\\() -> 1)\nrun 123456789",
        // s12_rc_share_used_safe
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let reg = make_registry ()\n    let k = { let r = Rc.new (Stash { f: \\() -> String.length t, out: reg }); let r2 = Rc.share (ref r); Rc.strong_count (ref r2) }\n    reg (\\() -> 1) + k\nrun 123456789",
        // s13_weak_min
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let reg = make_registry ()\n    { let r = Rc.new (Stash { f: \\() -> String.length t, out: reg }); let _w = Rc.downgrade (ref r); () }\n    7\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    for (source, kind, at) in [
        // sd01_stash_shared_cell_written_later
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { cell: Rc<Mutex<[unit -> i64]>>, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; Mutex.with_lock (Rc.get (ref s.cell)) (\\c -> { let _k = o c[0]; () }) }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let r = Rc.new (Mutex.create [\\() -> 0]); let _s = Stash { cell: Rc.share (ref r), out: reg }; Mutex.with_lock (Rc.get (ref r)) (\\c -> Array.write 0 (\\() -> String.length t) c) }\n    let other = [to_string 12345678, to_string 87654321, to_string 1234567]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (12, 5),
        ),
        // u01_share_text_inner
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let r = Rc.new (Stash { f: \\() -> String.length t, out: reg }); let _r2 = Rc.share (ref r); () }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (12, 5),
        ),
        // u02_weak_text_inner
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let r = Rc.new (Stash { f: \\() -> String.length t, out: reg }); let _w = Rc.downgrade (ref r); () }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (12, 5),
        ),
        // u03_arc_share_text_inner
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    { let text0 = to_string n; let t = ref text0; let r = Arc.new (Stash { f: \\() -> String.length t, out: reg }); let _r2 = Arc.share (ref r); () }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (12, 5),
        ),
        // u04_share_outlives_text
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Stash { f: unit -> i64, out: (unit -> i64) -> i64 }\ninstance Drop<Stash> {\n    fn drop s = { let o = s.out; let _k = o s.f; () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let mut keep = [Rc.new (Stash { f: \\() -> 0, out: reg })]\n    { let text0 = to_string n; let t = ref text0; let r = Rc.new (Stash { f: \\() -> String.length t, out: reg }); Array.write 0 (Rc.share (ref r)) (ref mut keep) }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0 + keep.length\nrun 123456789",
            "assignment_extends_borrow",
            (13, 5),
        ),
    ] {
        rejects_at(source, "E1013", kind, at);
    }
}

/// A31, review round 6 (finding 2): the annotated or destructured parameter of a lambda is a local
/// that a pattern binds to the parameter (`\(use: T) -> ...` is `\p -> match p with use -> ...`),
/// and so is a variable of an explicit `match` on a parameter: a call of it is a call of the
/// parameter, or of the part of it at the pattern's fields and payloads (`HigherOrderCall::path`),
/// which the caller substitutes. Before, the body stored the borrow of its local into the
/// environment of the function value at its definition, whatever the callers passed (`lk01`).
#[test]
fn a_parameter_that_a_pattern_binds_is_called_as_the_parameter() {
    for main in [
        // lk01_lambda_safe_caller
        "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let lk = \\(use: (unit -> i64) -> unit) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); 0 }\n    let k = lk (\\f -> { let _x = f (); () })\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
        // lk06_lambda_tuple_param
        "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let lk = \\(use, z) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); z }\n    let k = lk ((\\f -> { let _x = f (); () }), 0)\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
        // n05_named_match_tuple_safe
        "def h :: (((unit -> i64) -> unit) * i64) -> i64\nfn h p = match p with | (use, z) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); z }\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let k = h ((\\f -> { let _x = f (); () }), 1)\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
        // n07_record_field_safe
        "record H { use: (unit -> i64) -> unit, z: i64 }\ndef h :: H -> i64\nfn h p = { let s = to_string 123456789; let b = ref s; p.use (\\() -> String.length b); p.z }\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let k = h (H { use: \\f -> { let _x = f (); () }, z: 1 })\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
        // n10_tuple_via_local_safe
        "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let lk = \\(use, z) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); z }\n    let pair = ((\\f -> { let _x = f (); () }), 1)\n    let k = lk pair\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
        // n12_nested_pass_safe
        "def h :: (((unit -> i64) -> unit) * i64) -> i64\nfn h p = match p with | (use, z) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); z }\ndef g :: (((unit -> i64) -> unit) * i64) -> i64\nfn g q = h q + 1\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let k = g ((\\f -> { let _x = f (); () }), 1)\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
        // n15_param_borrow_safe_ctl
        "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let lk = \\(use: (unit -> i64) -> unit) (t: ref string) -> { use (\\() -> String.length t); 0 }\n    let text0 = to_string 123456789\n    let k = lk put (ref text0)\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
        // n17_or_pattern_safe
        "union Pick = | A of ((unit -> i64) -> unit) | B of ((unit -> i64) -> unit)\ndef h :: Pick -> i64\nfn h p = match p with | Pick.A use | Pick.B use -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); 2 }\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let k = h (Pick.B (\\f -> { let _x = f (); () }))\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
        // n18_maybe_ref_param_safe
        "def h :: ref Maybe<(unit -> i64) -> unit> -> i64\nfn h p = match p with | Maybe.Some use -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); 3 } | Maybe.None -> 0\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let g: Maybe<(unit -> i64) -> unit> = Maybe.Some (\\f -> { let _x = f (); () })\n    let k = h (ref g)\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
        // n21_maybe_literal_safe
        "def h :: Maybe<(unit -> i64) -> unit> -> i64\nfn h p = match p with | Maybe.Some use -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); 3 } | Maybe.None -> 0\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let k = h (Maybe.Some (\\f -> { let _x = f (); () }))\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    for (source, kind, at) in [
        // n02_named_annot_lambda
        (
            "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let lk = \\(use: (unit -> i64) -> unit) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); 0 }\n    let k = lk put\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
            "store_local_borrow_through_reference",
            (8, 13),
        ),
        // n04_tuple_unsafe
        (
            "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let lk = \\(use, z) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); z }\n    let k = lk (put, 0)\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 1",
            "store_local_borrow_through_reference",
            (8, 13),
        ),
        // n06_named_match_tuple_unsafe
        (
            "def h :: (((unit -> i64) -> unit) * i64) -> i64\nfn h p = match p with | (use, z) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); z }\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let k = h (put, 1)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 1",
            "store_local_borrow_through_reference",
            (9, 13),
        ),
        // n08_record_field_unsafe
        (
            "record H { use: (unit -> i64) -> unit, z: i64 }\ndef h :: H -> i64\nfn h p = { let s = to_string 123456789; let b = ref s; p.use (\\() -> String.length b); p.z }\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let k = h (H { use: put, z: 1 })\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 1",
            "store_local_borrow_through_reference",
            (10, 13),
        ),
        // n09_tuple_via_local_unsafe
        (
            "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let lk = \\(use, z) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); z }\n    let pair = (put, 0)\n    let k = lk pair\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 1",
            "store_local_borrow_through_reference",
            (9, 13),
        ),
        // n11_nested_pass_unsafe
        (
            "def h :: (((unit -> i64) -> unit) * i64) -> i64\nfn h p = match p with | (use, z) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); z }\ndef g :: (((unit -> i64) -> unit) * i64) -> i64\nfn g q = h q + 1\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let k = g (put, 1)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 1",
            "store_local_borrow_through_reference",
            (11, 13),
        ),
        // n13_or_pattern_unsafe
        (
            "union Pick = | A of ((unit -> i64) -> unit) | B of ((unit -> i64) -> unit)\ndef h :: Pick -> i64\nfn h p = match p with | Pick.A use | Pick.B use -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let k = h (Pick.A put)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 1",
            "store_local_borrow_through_reference",
            (10, 13),
        ),
        // n16_param_borrow_unsafe
        (
            "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let lk = \\(use: (unit -> i64) -> unit) (t: ref string) -> { use (\\() -> String.length t); 0 }\n    let k = { let text0 = to_string 123456789; lk put (ref text0) }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 1",
            "assignment_extends_borrow",
            (8, 13),
        ),
        // n19_maybe_ref_param_unsafe
        (
            "def h :: ref Maybe<(unit -> i64) -> unit> -> i64\nfn h p = match p with | Maybe.Some use -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); 3 } | Maybe.None -> 0\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let g = Maybe.Some put\n    let k = h (ref g)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 1",
            "store_local_borrow_through_reference",
            (10, 13),
        ),
        // n20_maybe_literal_unsafe
        (
            "def h :: Maybe<(unit -> i64) -> unit> -> i64\nfn h p = match p with | Maybe.Some use -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); 3 } | Maybe.None -> 0\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)\n    put (\\() -> n)\n    let k = h (Maybe.Some put)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0\nrun 1",
            "store_local_borrow_through_reference",
            (9, 13),
        ),
    ] {
        rejects_at(source, "E1013", kind, at);
    }
}

/// A31, review round 6 (finding 3): a pattern variable that views the matched storage in place
/// (`TypedMatchArm::views`) is the address of that storage, so a borrow of a view of a reference
/// parameter's target that goes to the caller's data is a borrow of the caller's data, as `ref
/// r.name` is (`expand_views`): the call checks it (`th09`, `th10`). A view of a local value or
/// of a temporary stays a borrow of a local value of the body, and so does a view of the target of
/// a parameter through which the body may write (`m02`: it may replace the matched value after
/// the store). A stored borrow of such a target stays live in the parameter (`m03`).
#[test]
fn a_borrow_of_a_view_of_the_callers_data_is_a_borrow_of_that_data() {
    for main in [
        // rg01_pattern_view_safe
        "def put :: ref Maybe<string> -> ref mut [(unit -> i64)..] -> unit\nfn put m slot = match m with | Maybe.Some s -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | Maybe.None -> ()\ndef run :: i64 -> i64\nfn run n =\n    let v0 = Maybe.Some (to_string n)\n    let mut arr = [\\() -> 0]\n    put (ref v0) (ref mut arr)\n    arr[0] ()\nrun 123456789",
        // rg02_pattern_view_tuple_safe
        "def put :: ref (string * i64) -> ref mut [(unit -> i64)..] -> unit\nfn put p slot = match p with | (s, k) -> { let sr = ref s; Array.write 0 (\\() -> String.length sr + k) slot }\ndef run :: i64 -> i64\nfn run n =\n    let v0 = (to_string n, 1)\n    let mut arr = [\\() -> 0]\n    put (ref v0) (ref mut arr)\n    arr[0] ()\nrun 123456789",
        // rg03_pattern_view_record_safe
        "record Rec { name: string, k: i64 }\ndef put :: ref Rec -> ref mut [(unit -> i64)..] -> unit\nfn put r slot = match r with | Rec { name: s, k: _ } -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot }\ndef run :: i64 -> i64\nfn run n =\n    let v0 = Rec { name: to_string n, k: 1 }\n    let mut arr = [\\() -> 0]\n    put (ref v0) (ref mut arr)\n    arr[0] ()\nrun 123456789",
        // rg05_view_array_set
        "def put :: ref Maybe<string> -> ref mut [(unit -> i64)..] -> unit\nfn put m slot = match m with | Maybe.Some s -> { let sr = ref s; Array.set 0 (\\() -> String.length sr) slot } | Maybe.None -> ()\ndef run :: i64 -> i64\nfn run n =\n    let v0 = Maybe.Some (to_string n)\n    let mut arr = [\\() -> 0]\n    put (ref v0) (ref mut arr)\n    arr[0] ()\nrun 123456789",
        // rg07_view_lock
        "def put :: ref Maybe<string> -> ref Mutex<[unit -> i64]> -> unit\nfn put m lock = match m with | Maybe.Some s -> { let sr = ref s; Mutex.with_lock lock (\\slot -> Array.write 0 (\\() -> String.length sr) slot) } | Maybe.None -> ()\ndef run :: i64 -> i64\nfn run n =\n    let v0 = Maybe.Some (to_string n)\n    let m = Mutex.create [\\() -> 0]\n    put (ref v0) (ref m)\n    Mutex.with_lock (ref m) (\\slot -> slot[0] ())\nrun 123456789",
        // rg09_for_tuple_pattern
        "record Rec { name: string, k: i64 }\ndef put :: ref [(string * i64)] -> ref mut [(unit -> i64)..] -> unit\nfn put xs slot = for (s, _) in xs do { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot }\ndef run :: i64 -> i64\nfn run n =\n    let v0 = [(to_string n, 1)]\n    let mut arr = [\\() -> 0]\n    put (ref v0) (ref mut arr)\n    arr[0] ()\nrun 123456789",
        // rg10_array_pattern
        "record Rec { name: string, k: i64 }\ndef put :: ref [string] -> ref mut [(unit -> i64)..] -> unit\nfn put xs slot = match xs with | [s] -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | _ -> ()\ndef run :: i64 -> i64\nfn run n =\n    let v0 = [to_string n]\n    let mut arr = [\\() -> 0]\n    put (ref v0) (ref mut arr)\n    arr[0] ()\nrun 123456789",
        // rg12_nested_view_field
        "record Rec { name: string, k: i64 }\ndef put :: ref Maybe<Rec> -> ref mut [(unit -> i64)..] -> unit\nfn put m slot = match m with | Maybe.Some r -> { let sr = ref r.name; Array.write 0 (\\() -> String.length sr) slot } | Maybe.None -> ()\ndef run :: i64 -> i64\nfn run n =\n    let v0 = Maybe.Some (Rec { name: to_string n, k: 1 })\n    let mut arr = [\\() -> 0]\n    put (ref v0) (ref mut arr)\n    arr[0] ()\nrun 123456789",
        // v05_view_hidden_registry_safe
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef put :: ref Maybe<string> -> ((unit -> i64) -> i64) -> i64\nfn put m reg = match m with | Maybe.Some s -> { let sr = ref s; reg (\\() -> String.length sr) } | Maybe.None -> 0\ndef run :: i64 -> i64\nfn run n =\n    let v0 = Maybe.Some (to_string n)\n    let reg = make_registry ()\n    let k = put (ref v0) reg\n    k + reg (\\() -> 1)\nrun 123456789",
        // v07_view_param_fn_safe
        "def put :: ref Maybe<string> -> ((unit -> i64) -> unit) -> unit\nfn put m use = match m with | Maybe.Some s -> { let sr = ref s; use (\\() -> String.length sr) } | Maybe.None -> ()\ndef run :: i64 -> i64\nfn run n =\n    let v0 = Maybe.Some (to_string n)\n    let mx = Mutex.create [\\() -> 0]\n    let mr = ref mx\n    put (ref v0) (\\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot))\n    Mutex.with_lock (ref mx) (\\slot -> slot[0] ())\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    // rg08: a read parameter of edition 2027 is a reference; in edition 2026 it is the callee's own
    // value, whose view is a local value.
    let read_parameter = "def put :: Maybe<string> -> ref mut [(unit -> i64)..] -> unit\nfn put m slot = match m with | Maybe.Some s -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | Maybe.None -> ()\ndef run :: i64 -> i64\nfn run n =\n    let v0 = Maybe.Some (to_string n)\n    let mut arr = [\\() -> 0]\n    put v0 (ref mut arr)\n    arr[0] ()\nrun 123456789";
    analyze_modules_with_edition(&[("Main.tz", read_parameter)], Edition::E2027)
        .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    let error = analyze_modules_with_edition(&[("Main.tz", read_parameter)], Edition::E2026)
        .expect_err("a view of the callee's own value");
    assert_eq!(
        (error.code, error.kind),
        ("E1013", Some("store_local_borrow_through_reference"))
    );
    for (source, kind, at) in [
        // v01_view_of_temporary
        (
            "def make :: unit -> [string]\nfn make _u = [to_string 123456789]\ndef put :: ref mut [(unit -> i64)..] -> unit\nfn put slot = match make () with | [s] -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | _ -> ()\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    arr[0] () + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (4, 61),
        ),
        // v02_view_of_local
        (
            "def put :: ref mut [(unit -> i64)..] -> unit\nfn put slot =\n    let local = [to_string 123456789]\n    match ref local with | [s] -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | _ -> ()\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    arr[0] () + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (4, 53),
        ),
        // v03_view_of_owned_param
        (
            "def put :: own Maybe<string> -> ref mut [(unit -> i64)..] -> unit\nfn put m slot = match ref m with | Maybe.Some s -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | Maybe.None -> ()\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    put (Maybe.Some (to_string 123456789)) (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    arr[0] () + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (2, 70),
        ),
        // v04_view_hidden_registry_unsafe
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef put :: ref Maybe<string> -> ((unit -> i64) -> i64) -> i64\nfn put m reg = match m with | Maybe.Some s -> { let sr = ref s; reg (\\() -> String.length sr) } | Maybe.None -> 0\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let k = { let v0 = Maybe.Some (to_string n); put (ref v0) reg }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    k + reg (\\() -> 1) + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (10, 13),
        ),
        // v06_view_param_fn_unsafe
        (
            "def put :: ref Maybe<string> -> ((unit -> i64) -> unit) -> unit\nfn put m use = match m with | Maybe.Some s -> { let sr = ref s; use (\\() -> String.length sr) } | Maybe.None -> ()\ndef run :: i64 -> i64\nfn run n =\n    let mx = Mutex.create [\\() -> 0]\n    let mr = ref mx\n    { let v0 = Maybe.Some (to_string n); put (ref v0) (\\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)) }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (ref mx) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
        // v08_view_vec_param_unsafe
        (
            "def put :: ref [string] -> ref mut [(unit -> i64)..] -> unit\nfn put xs slot = match xs with | [s, _] -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | _ -> ()\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    { let v0 = [to_string n, to_string 5]; put (ref v0) (ref mut arr) }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    arr[0] () + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (6, 5),
        ),
        // th09_tuple_field
        (
            "record Rec { name: string, items: [string] }\ndef put :: ref (string * i64) -> ref mut [(unit -> i64)..] -> unit\nfn put p slot = match p with | (s, _) -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot }\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    { let v0 = (to_string n, 1); put (ref v0) (ref mut arr) }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    arr[0] () + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
        // th10_maybe_some
        (
            "record Rec { name: string, items: [string] }\ndef put :: ref Maybe<string> -> ref mut [(unit -> i64)..] -> unit\nfn put m slot = match m with | Maybe.Some s -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | Maybe.None -> ()\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    { let v0 = Maybe.Some (to_string n); put (ref v0) (ref mut arr) }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    arr[0] () + other.length * 0\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
        // m02_view_then_mutate
        (
            "def put :: ref mut Maybe<string> -> ref mut [(unit -> i64)..] -> unit\nfn put m slot =\n    match m with | Maybe.Some s -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | Maybe.None -> ()\n    deref m = Maybe.None\ndef run :: i64 -> i64\nfn run n =\n    let mut v0 = Maybe.Some (to_string n)\n    let mut arr = [\\() -> 0]\n    put (ref mut v0) (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    arr[0] () + other.length * 0\nrun 123456789",
            "store_local_borrow_through_reference",
            (3, 54),
        ),
    ] {
        rejects_at(source, "E1013", kind, at);
    }
    // A borrow of the target of a parameter through which the body may write, kept in the
    // caller's data, stays live: a later write through the parameter conflicts with it.
    // m03_elem_reborrow_then_push
    let pushed = "def put :: ref mut Vec<string> -> ref mut [(unit -> i64)..] -> unit\nfn put v slot =\n    let sr = ref v[0]\n    Array.write 0 (\\() -> String.length sr) slot\n    let mut i = 0\n    while i < 100 do {\n        Vec.push_mut (to_string i) v\n        i = i + 1\n    }\ndef run :: i64 -> i64\nfn run n =\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    let mut arr = [\\() -> 0]\n    put (ref mut v0) (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    arr[0] () + other.length * 0\nrun 123456789";
    rejects_at(pushed, "E1014", "borrow_conflict", (7, 36));
}

/// A31, review round 6 (finding 4): the locals of a block are dropped in the reverse order of
/// their declarations, so a value whose drop may run a user `Drop.drop` cannot keep a borrow of a
/// local declared after it in the same block, however it got the borrow: an assignment (`do03`), a
/// write in place, a lock's callback, `Vec.push_mut`, or a registry that owns its cell (`dc01`). Its
/// loans stay live until the end of its scope (`dropped_later`), so a parameter that keeps one is
/// caught too (`k04`). A share of a block that such a local holds stands for what the block holds,
/// then and later (`k11`), and not for a borrow of the owner, which may move (`k12`). Before, the
/// drop read the freed value (`drop 0` in place of `drop 9`).
#[test]
fn a_value_whose_drop_runs_code_keeps_no_borrow_of_a_later_local() {
    for (source, kind, at) in [
        // do02_array_write
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Caller { f: \\() -> 0 }]\n    let text0 = to_string n\n    let t = ref text0\n    Array.write 0 (Caller { f: \\() -> String.length t }) (ref mut arr)\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
        // do03_reassign
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> 0 }\n    let text0 = to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> String.length t }\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
        // do04_mutex_local
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [Caller { f: \\() -> 0 }]\n    let text0 = to_string n\n    let t = ref text0\n    Mutex.with_lock (ref m) (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
        // do06_array_set
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Caller { f: \\() -> 0 }]\n    let text0 = to_string n\n    let t = ref text0\n    Array.set 0 (Caller { f: \\() -> String.length t }) (ref mut arr)\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
        // do07_vec_push_mut
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut v = Vec.empty()\n    let text0 = to_string n\n    let t = ref text0\n    Vec.push_mut (Caller { f: \\() -> String.length t }) (ref mut v)\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
        // dc01_cell_drop_calls
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 0 (Caller { f: f }) slot; 0 })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let text0 = to_string n\n    let text = ref text0\n    reg (\\() -> String.length text)\nrun 123456789",
            "assignment_extends_borrow",
            (11, 5),
        ),
        // k04_owned_param_holder_unsafe
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef keep :: own Caller -> i64 -> i64\nfn keep holder n =\n    let mut h = holder\n    let text0 = to_string n\n    let t = ref text0\n    h = Caller { f: \\() -> String.length t }\n    0\ndef run :: i64 -> i64\nfn run n = keep (Caller { f: \\() -> 0 }) n\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
        // k10_mutex_registry_unsafe
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [Caller { f: \\() -> 0 }]\n    let text0 = to_string n\n    let t = ref text0\n    Mutex.with_lock (ref m) (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
        // k11_shared_block_written_later_unsafe
        (
            "record Reader { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Reader> {\n    fn drop r = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (Arc.get (ref r.cell)) (\\slot -> slot[0] ())))\n}\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let s = Reader { cell: Arc.share (ref a) }\n    let text0 = to_string n\n    let t = ref text0\n    Mutex.with_lock (Arc.get (ref a)) (\\c -> Array.write 0 (\\() -> String.length t) c)\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (7, 5),
        ),
    ] {
        rejects_at(source, "E1013", kind, at);
        let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
        assert!(
            error
                .message
                .contains("may run a Drop.drop that uses the borrow"),
            "{source}\n{}",
            error.message
        );
    }
    // do01: the same with an index assignment (edition 2027 only).
    let index_assign = "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Caller { f: \\() -> 0 }]\n    let text0 = to_string n\n    let t = ref text0\n    arr[0] = Caller { f: \\() -> String.length t }\n    0\nrun 123456789";
    let error = analyze_with_edition(index_assign, Edition::E2027).expect_err(index_assign);
    assert_eq!(
        (error.code, error.kind),
        ("E1013", Some("assignment_extends_borrow"))
    );
    assert_eq!(position(index_assign, error.span.start), (7, 5));
    for main in [
        // do03_ctl
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> 0 }\n    let text0 = to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> 9 }\n    0\nrun 123456789",
        // dc02_cell_drop_calls_ctl
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { Array.write 0 (Caller { f: f }) slot; 0 })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let text0 = to_string n\n    let text = ref text0\n    reg (\\() -> 9)\nrun 123456789",
        // k01_holder_after_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let mut holder = Caller { f: \\() -> 0 }\n    holder = Caller { f: \\() -> String.length t }\n    0\nrun 123456789",
        // k02_plain_closures_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [\\() -> 0]\n    let text0 = to_string n\n    let t = ref text0\n    Array.write 0 (\\() -> String.length t) (ref mut arr)\n    let c = Caller { f: \\() -> 3 }\n    arr[0] () + c.f ()\nrun 123456789",
        // k06_moved_holder_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = c.f ()\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> 0 }\n    let text0 = to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> String.length t }\n    drop_it holder\nrun 123456789",
        // k07_param_borrow_holder_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef mk :: ref string -> i64\nfn mk t =\n    let mut holder = Caller { f: \\() -> 0 }\n    holder = Caller { f: \\() -> String.length t }\n    1\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    mk (ref text0)\nrun 123456789",
        // k09_no_drop_module_safe
        "record Caller { f: unit -> i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> 0 }\n    let text0 = to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> String.length t }\n    holder.f ()\nrun 123456789",
        // k12_move_owner_after_share_safe
        "record Reader { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Reader> {\n    fn drop r = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (Arc.get (ref r.cell)) (\\slot -> slot[0] ())))\n}\ndef take :: own Arc<Mutex<[unit -> i64]>> -> i64\nfn take a = Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 3])\n    let s = Reader { cell: Arc.share (ref a) }\n    let k = take a\n    k + n\nrun 4",
        // k13_shared_block_dropped_first_safe
        "record Reader { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Reader> {\n    fn drop r = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (Arc.get (ref r.cell)) (\\slot -> slot[0] ())))\n}\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let text0 = to_string n\n    let t = ref text0\n    Mutex.with_lock (Arc.get (ref a)) (\\c -> Array.write 0 (\\() -> String.length t) c)\n    { let s = Reader { cell: Arc.share (ref a) }; () }\n    let u = Reader { cell: Arc.share (ref a) }\n    0\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 7 (findings 1, 2 and 6): a value whose drop may run a user `Drop.drop` is
/// dropped at the end of its scope unless every path moved it away, so one that only a branch moves
/// is checked like one that no path moves, and keeps its borrows to the end of the scope; and a
/// `break`, a `continue` or an exception that a `try` handles drops the locals of the scopes that it
/// leaves in the reverse order of their declarations there, whatever the code after it puts into
/// them.
#[test]
fn a_value_that_a_branch_moves_or_a_jump_drops_keeps_no_borrow_of_a_later_local() {
    for (source, kind, at) in [
        // do01_cond_move
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = c.f ()\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> 0 }\n    let text0 = to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> String.length t }\n    if n > 1000000000 then drop_it holder else 0\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // do02_cond_move_match
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = c.f ()\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> 0 }\n    let text0 = to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> String.length t }\n    let k = match n with\n        | 0 -> drop_it holder\n        | _ -> 1\n    k\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // do03_while_move
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = c.f ()\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Caller { f: \\() -> 0 }]\n    let text0 = to_string n\n    let t = ref text0\n    Array.write 0 (Caller { f: \\() -> String.length t }) (ref mut arr)\n    let total = if n == 0 then { let a = arr; a.length } else 2\n    total\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // do21_k11_condmove
        (
            "record Reader { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Reader> {\n    fn drop r = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (Arc.get (ref r.cell)) (\\slot -> slot[0] ())))\n}\ndef eat :: own Reader -> i64\nfn eat r = 1\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let s = Reader { cell: Arc.share (ref a) }\n    let text0 = to_string n\n    let t = ref text0\n    Mutex.with_lock (Arc.get (ref a)) (\\c -> Array.write 0 (\\() -> String.length t) c)\n    if n > 1000000000 then eat s else 0\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // do23_k10_condmove
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef eat :: own Mutex<[Caller]> -> i64\nfn eat m = 1\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [Caller { f: \\() -> 0 }]\n    let text0 = to_string n\n    let t = ref text0\n    Mutex.with_lock (ref m) (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\n    if n > 1000000000 then eat m else 0\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // do35_cond_move_content
        (
            "record Caller { f: unit -> string }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop [\" + c.f () + \"]\")\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = 0\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> \"none\" }\n    let text0 = \"secret-\" + to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> Maybe.default_value \"?\" (String.sub 0 16 t) }\n    if n > 1000000000 then drop_it holder else 0\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // m02 (a match arm moves the holder)
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef consume :: own Caller -> unit\nfn consume c = ()\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> 0 }\n    let text0 = to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> String.length t }\n    match n with\n    | 1 -> consume holder\n    | _ -> ()\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // x07: moved on every path, then given a borrow again on one
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = c.f ()\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> 0 }\n    let text0 = to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> String.length t }\n    let k = drop_it holder\n    if n > 5 then holder = Caller { f: \\() -> String.length t } else ()\n    k\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
        // do27_break_then_reassign
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut i = 0\n    while i < 3 do\n        let mut holder = Caller { f: \\() -> 0 }\n        let text0 = to_string n\n        let t = ref text0\n        holder = Caller { f: \\() -> String.length t }\n        if i == 0 then break else ()\n        holder = Caller { f: \\() -> 9 }\n        i = i + 1\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (13, 24),
        ),
        // do28_continue_then_reassign
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut i = 0\n    while i < 2 do\n        i = i + 1\n        let mut holder = Caller { f: \\() -> 0 }\n        let text0 = to_string n\n        let t = ref text0\n        holder = Caller { f: \\() -> String.length t }\n        if i == 1 then continue else ()\n        holder = Caller { f: \\() -> 9 }\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (14, 24),
        ),
        // x02: a break in an inner block
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut i = 0\n    while i < 2 do\n        let mut holder = Caller { f: \\() -> 0 }\n        let k = { let text0 = to_string n; let t = ref text0; holder = Caller { f: \\() -> String.length t }; if i == 0 then break else (); holder = Caller { f: \\() -> 9 }; 1 }\n        i = i + k\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (10, 125),
        ),
        // x04: a break in a for loop
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    for i in 0..=2 do\n        let mut holder = Caller { f: \\() -> 0 }\n        let text0 = to_string n\n        let t = ref text0\n        holder = Caller { f: \\() -> String.length t }\n        if i == 0 then break else ()\n        holder = Caller { f: \\() -> 9 }\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (12, 24),
        ),
        // x08: an exception that the try handles
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> Result<i64, Exception>\nfn run n =\n    try\n        let mut holder = Caller { f: \\() -> 0 }\n        let text0 = to_string n\n        let t = ref text0\n        holder = Caller { f: \\() -> String.length t }\n        let z = @checked n * 9223372036854775807\n        holder = Caller { f: \\() -> 9 }\n        z\n    with\n    | e -> e\nmatch run 123456789 with\n| Ok v -> v\n| Error _ -> 0",
            "assignment_extends_borrow",
            (12, 26),
        ),
    ] {
        rejects_at(source, "E1013", kind, at);
        let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
        assert!(
            error
                .message
                .contains("may run a Drop.drop that uses the borrow"),
            "{source}\n{}",
            error.message
        );
    }
    // do12: the holder that a branch moves keeps its borrow, so the borrowed value cannot move.
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = c.f ()\ndef consume :: own string -> i64\nfn consume s = String.length (ref s)\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let mut holder = Caller { f: \\() -> 0 }\n    holder = Caller { f: \\() -> String.length t }\n    let k = if n > 1000000000 then drop_it holder else 0\n    let other = [to_string 777777, to_string 88888888]\n    k + consume text0 + other.length * 0\nrun 123456789",
        "E1014",
        "borrow_conflict",
        (17, 17),
    );
    for main in [
        // do06_ctl_text_first
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = c.f ()\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let mut holder = Caller { f: \\() -> 0 }\n    holder = Caller { f: \\() -> String.length t }\n    if n > 1000000000 then drop_it holder else 0\nrun 123456789",
        // m01_ctl
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef consume :: own Caller -> unit\nfn consume c = ()\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let mut holder = Caller { f: \\() -> 0 }\n    holder = Caller { f: \\() -> String.length t }\n    if n < 5 then consume holder else ()\n    0\nrun 123456789",
        // do13_uncond_move_then_consume_ctl
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = c.f ()\ndef consume :: own string -> i64\nfn consume s = String.length (ref s)\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let mut holder = Caller { f: \\() -> 0 }\n    holder = Caller { f: \\() -> String.length t }\n    let k = drop_it holder\n    let other = [to_string 777777, to_string 88888888]\n    k + consume text0 + other.length * 0\nrun 123456789",
        // x05: moved on both paths
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = c.f ()\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> 0 }\n    let text0 = to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> String.length t }\n    if n > 5 then drop_it holder else drop_it holder\nrun 123456789",
        // x06: moved on one path, then given a value without a borrow
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef drop_it :: own Caller -> i64\nfn drop_it c = c.f ()\ndef run :: i64 -> i64\nfn run n =\n    let mut holder = Caller { f: \\() -> 0 }\n    let text0 = to_string n\n    let t = ref text0\n    holder = Caller { f: \\() -> String.length t }\n    let k = if n > 5 then drop_it holder else 0\n    holder = Caller { f: \\() -> 3 }\n    k\nrun 123456789",
        // do29_ctl_no_break
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut i = 0\n    while i < 3 do\n        let mut holder = Caller { f: \\() -> 0 }\n        let text0 = to_string n\n        let t = ref text0\n        holder = Caller { f: \\() -> String.length t }\n        holder = Caller { f: \\() -> 9 }\n        i = i + 1\n    0\nrun 123456789",
        // x03: the borrowed value declared first
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut i = 0\n    while i < 3 do\n        let text0 = to_string n\n        let t = ref text0\n        let mut holder = Caller { f: \\() -> String.length t }\n        if i == 0 then break else ()\n        holder = Caller { f: \\() -> 9 }\n        i = i + 1\n    0\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 7 (finding 3): a `Drop.drop` gets only its own value, so the drop of a part
/// whose type cannot hold a borrow (`Guard { id: i64 }`) reads none of the borrows that the parts
/// beside it hold, and does not keep them to the end of the scope or ask their owners to be
/// declared first. A part that may reach a borrow, here through the block of a weak pointer, still
/// counts.
#[test]
fn a_drop_that_cannot_hold_a_borrow_reads_none_of_the_borrows_beside_it() {
    for main in [
        // rd01_guard_sibling_borrow
        "record Guard { id: i64 }\ninstance Drop<Guard> {\n    fn drop g = do! IO.write_line (\"bye \" + to_string g.id)\n}\nrecord Ctx { guard: Guard, label: ref string }\ndef run :: i64 -> i64\nfn run n =\n    let mut text = to_string n\n    let c = Ctx { guard: Guard { id: 1 }, label: ref text }\n    let len = String.length c.label\n    text = to_string 5\n    len + String.length (ref text)\nrun 123456789",
        // rd03_guard_tuple_borrow
        "record Guard { id: i64 }\ninstance Drop<Guard> {\n    fn drop g = do! IO.write_line (\"bye \" + to_string g.id)\n}\ndef second :: own (Guard * ref string) -> i64\nfn second p = 0\ndef run :: i64 -> i64\nfn run n =\n    let mut text = to_string n\n    let c = (Guard { id: 1 }, ref text)\n    let len = match c with | (_, s) -> String.length s\n    text = to_string 5\n    len + String.length (ref text)\nrun 123456789",
        // rd04_guard_sibling_declared_first
        "record Guard { id: i64 }\ninstance Drop<Guard> {\n    fn drop g = do! IO.write_line (\"bye \" + to_string g.id)\n}\nrecord Ctx { guard: Guard, label: ref string }\ndef run :: i64 -> i64\nfn run n =\n    let first = to_string 1\n    let mut c = Ctx { guard: Guard { id: 1 }, label: ref first }\n    let text = to_string n\n    c = Ctx { guard: Guard { id: 2 }, label: ref text }\n    String.length c.label\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    // z01: the guard's drop upgrades a weak pointer to a cell of function values.
    rejects_at(
        "record Guard { w: Rc.Weak<Mutex<[unit -> i64]>> }\ninstance Drop<Guard> {\n    fn drop g = match Rc.upgrade (ref g.w) with\n        | Maybe.Some c -> do! IO.write_line (\"bye \" + to_string (Mutex.with_lock (Rc.get (ref c)) (\\slot -> slot[0] ())))\n        | Maybe.None -> ()\n}\nrecord Ctx { guard: Guard, label: ref string }\ndef run :: i64 -> i64\nfn run n =\n    let cell = Rc.new (Mutex.create [\\() -> 0])\n    let first = to_string 1\n    let mut c = Ctx { guard: Guard { w: Rc.downgrade (ref cell) }, label: ref first }\n    let text = to_string n\n    c = Ctx { guard: Guard { w: Rc.downgrade (ref cell) }, label: ref text }\n    String.length c.label\nrun 123456789",
        "E1013",
        "assignment_extends_borrow",
        (10, 5),
    );
}

/// A31, review round 7 (finding 4): a borrow of what a `ref mut` parameter points at, which the
/// body stored where the caller's data reaches, stays live for the rest of the body by itself, not
/// in the parameter's value: a later write of a disjoint field through the parameter is accepted,
/// and one that may reach the borrow is `E1014` with a help that says why. A caller that passes a
/// fresh `ref mut` borrow there keeps a shared borrow of the value, so it may read the value while
/// the place that took the borrow lives, and still not write or move it.
#[test]
fn a_borrow_stored_from_a_mutable_parameter_keeps_only_what_it_borrows() {
    // wp01_disjoint_field_write (`r.count = 5` assigns through a reference only in edition 2027).
    let disjoint = "record Rec { name: string, count: i64 }\ndef put :: ref mut Rec -> ref mut [(unit -> i64)..] -> unit\nfn put r slot =\n    let sr = ref r.name\n    Array.write 0 (\\() -> String.length sr) slot\n    r.count = 5\ndef run :: i64 -> i64\nfn run n =\n    let mut v0 = Rec { name: to_string n, count: 0 }\n    let mut arr = [\\() -> 0]\n    put (ref mut v0) (ref mut arr)\n    arr[0] () + v0.count\nrun 123456789";
    let module = analyze_with_edition(disjoint, Edition::E2027)
        .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    llvm::emit_target(&module, llvm::Entry::Library, false).unwrap();
    for main in [
        // wq01: the caller reads another field afterwards
        "record Rec { name: string, count: i64 }\ndef put :: ref mut Rec -> ref mut [(unit -> i64)..] -> unit\nfn put r slot =\n    let sr = ref r.name\n    Array.write 0 (\\() -> String.length sr) slot\ndef run :: i64 -> i64\nfn run n =\n    let mut v0 = Rec { name: to_string n, count: 3 }\n    let mut arr = [\\() -> 0]\n    put (ref mut v0) (ref mut arr)\n    arr[0] () + v0.count\nrun 123456789",
        // wp02_read_after_store
        "record Rec { name: string, count: i64 }\ndef put :: ref mut Rec -> ref mut [(unit -> i64)..] -> i64\nfn put r slot =\n    let sr = ref r.name\n    Array.write 0 (\\() -> String.length sr) slot\n    String.length (ref r.name) + r.count\ndef run :: i64 -> i64\nfn run n =\n    let mut v0 = Rec { name: to_string n, count: 1 }\n    let mut arr = [\\() -> 0]\n    let k = put (ref mut v0) (ref mut arr)\n    arr[0] () + k\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    for (source, at) in [
        // wq06: the caller replaces the value
        (
            "record Rec { name: string, count: i64 }\ndef put :: ref mut Rec -> ref mut [(unit -> i64)..] -> unit\nfn put r slot =\n    let sr = ref r.name\n    Array.write 0 (\\() -> String.length sr) slot\ndef run :: i64 -> i64\nfn run n =\n    let mut v0 = Rec { name: to_string n, count: 3 }\n    let mut arr = [\\() -> 0]\n    put (ref mut v0) (ref mut arr)\n    v0 = Rec { name: to_string 5, count: 1 }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    arr[0] () + other.length * 0\nrun 123456789",
            (11, 5),
        ),
        // y05: the body passes the parameter on to a function that pushes
        (
            "def push_all :: ref mut Vec<string> -> unit\nfn push_all v =\n    let mut i = 0\n    while i < 100 do {\n        Vec.push_mut (to_string i) v\n        i = i + 1\n    }\ndef put :: ref mut Vec<string> -> ref mut [(unit -> i64)..] -> unit\nfn put v slot =\n    let sr = ref v[0]\n    Array.write 0 (\\() -> String.length sr) slot\n    push_all v\ndef run :: i64 -> i64\nfn run n =\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    let mut arr = [\\() -> 0]\n    put (ref mut v0) (ref mut arr)\n    arr[0] ()\nrun 123456789",
            (12, 14),
        ),
        // wp03_other_element_write (the view is borrowed whole, as with a local)
        (
            "def put :: ref mut [string..] -> ref mut [(unit -> i64)..] -> unit\nfn put v slot =\n    let sr = ref v[0]\n    Array.write 0 (\\() -> String.length sr) slot\n    Array.write 1 (to_string 5) v\ndef run :: i64 -> i64\nfn run n =\n    let mut words = [to_string n, to_string 7]\n    let mut arr = [\\() -> 0]\n    put (ref mut words) (ref mut arr)\n    arr[0] () + String.length (ref words[1])\nrun 123456789",
            (5, 33),
        ),
    ] {
        rejects_at(source, "E1014", "borrow_conflict", at);
    }
    let error = analyze_with_edition("def push_all :: ref mut Vec<string> -> unit\nfn push_all v =\n    let mut i = 0\n    while i < 100 do {\n        Vec.push_mut (to_string i) v\n        i = i + 1\n    }\ndef put :: ref mut Vec<string> -> ref mut [(unit -> i64)..] -> unit\nfn put v slot =\n    let sr = ref v[0]\n    Array.write 0 (\\() -> String.length sr) slot\n    push_all v\ndef run :: i64 -> i64\nfn run n =\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    let mut arr = [\\() -> 0]\n    put (ref mut v0) (ref mut arr)\n    arr[0] ()\nrun 123456789", Edition::E2027)
        .expect_err("the push through the parameter");
    assert!(
        error
            .help
            .as_deref()
            .is_some_and(|help| help.contains("stored the borrow where its caller's data")),
        "{:?}",
        error.help
    );
}

/// A31, review round 7 (finding 8): a body that hands a borrow of what one of its `ref mut`
/// parameters points at to a function-valued parameter and writes there afterwards (a push that
/// moves the elements, or passing the parameter on) may free what the borrow points at, so the
/// caller rejects a function value that keeps what it is given (`E1013` at the call), and accepts
/// one that only calls it.
#[test]
fn a_borrow_handed_to_a_function_parameter_and_written_over_cannot_be_kept() {
    for (source, at) in [
        // h01 (a registry that stores what it gets)
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef put :: ref mut Vec<string> -> ((unit -> i64) -> i64) -> unit\nfn put v reg =\n    let sr = ref v[0]\n    let _k = reg (\\() -> String.length sr)\n    let mut i = 0\n    while i < 100 do {\n        Vec.push_mut (to_string i) v\n        i = i + 1\n    }\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    put (ref mut v0) reg\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (19, 5),
        ),
        // h03 (a lambda that stores it in a mutex)
        (
            "def put :: ref mut Vec<string> -> ((unit -> i64) -> unit) -> unit\nfn put v use =\n    let sr = ref v[0]\n    use (\\() -> String.length sr)\n    let mut i = 0\n    while i < 100 do {\n        Vec.push_mut (to_string i) v\n        i = i + 1\n    }\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    put (ref mut v0) (\\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot))\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock mr (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            (16, 5),
        ),
        // y02: the body passes the parameter on to a function that pushes
        (
            "def push_all :: ref mut Vec<string> -> unit\nfn push_all v =\n    let mut i = 0\n    while i < 100 do {\n        Vec.push_mut (to_string i) v\n        i = i + 1\n    }\ndef put :: ref mut Vec<string> -> ((unit -> i64) -> unit) -> unit\nfn put v use =\n    let sr = ref v[0]\n    use (\\() -> String.length sr)\n    push_all v\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    put (ref mut v0) (\\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot))\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock mr (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            (19, 5),
        ),
        // y04: a function that passes both on
        (
            "def relay :: ref mut Vec<string> -> ((unit -> i64) -> unit) -> unit\nfn relay v use = put v use\ndef put :: ref mut Vec<string> -> ((unit -> i64) -> unit) -> unit\nfn put v use =\n    let sr = ref v[0]\n    use (\\() -> String.length sr)\n    let mut i = 0\n    while i < 100 do {\n        Vec.push_mut (to_string i) v\n        i = i + 1\n    }\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    relay (ref mut v0) (\\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot))\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock mr (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            (18, 5),
        ),
    ] {
        rejects_at(source, "E1013", "store_local_borrow_through_reference", at);
        let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
        assert!(
            error.message.contains("writes there afterwards"),
            "{source}\n{}",
            error.message
        );
    }
    for main in [
        // h01_ctl (no push)
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef put :: ref mut Vec<string> -> ((unit -> i64) -> i64) -> unit\nfn put v reg =\n    let sr = ref v[0]\n    let _k = reg (\\() -> String.length sr)\n    ()\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    put (ref mut v0) reg\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
        // h03_ctl (no push)
        "def put :: ref mut Vec<string> -> ((unit -> i64) -> unit) -> unit\nfn put v use =\n    let sr = ref v[0]\n    use (\\() -> String.length sr)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    put (ref mut v0) (\\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot))\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock mr (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
        // y01: the function value only calls what it gets
        "def put :: ref mut Vec<string> -> ((unit -> i64) -> i64) -> i64\nfn put v use =\n    let sr = ref v[0]\n    let k = use (\\() -> String.length sr)\n    let mut i = 0\n    while i < 100 do {\n        Vec.push_mut (to_string i) v\n        i = i + 1\n    }\n    k\ndef run :: i64 -> i64\nfn run n =\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    put (ref mut v0) (\\g -> g ())\nrun 123456789",
        // y04c: the same through a function that passes both on
        "def relay :: ref mut Vec<string> -> ((unit -> i64) -> i64) -> i64\nfn relay v use = put v use\ndef put :: ref mut Vec<string> -> ((unit -> i64) -> i64) -> i64\nfn put v use =\n    let sr = ref v[0]\n    let k = use (\\() -> String.length sr)\n    let mut i = 0\n    while i < 100 do {\n        Vec.push_mut (to_string i) v\n        i = i + 1\n    }\n    k\ndef run :: i64 -> i64\nfn run n =\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    relay (ref mut v0) (\\g -> g ())\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 7 (finding 7): a store into the block of a local `Rc` or `Arc` owner that the
/// body moved into a callee, after a share of the block went to the caller's data or to a value
/// whose drop reads it, reaches that place too: the callee's facts, or code that the check does not
/// see, store into the block through the moved owner, which no local root leads to any more.
#[test]
fn a_store_into_an_owner_moved_into_a_callee_reaches_where_its_block_went() {
    for (source, kind, at) in [
        // e06 (the share went to the caller)
        (
            "def put_into :: ref string -> own Rc<Mutex<[unit -> i64]>> -> unit\nfn put_into s a = Mutex.with_lock (Rc.get (ref a)) (\\c -> Array.write 0 (\\() -> String.length s) c)\ndef put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    Array.write 0 (Rc.share (ref lr)) slot\n    let local = to_string 123456789\n    put_into (ref local) lr\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0 + n * 0\nrun 1",
            "store_local_borrow_through_reference",
            (8, 5),
        ),
        // e07: the owner moved into a function value that the check does not see
        (
            "def make_put :: unit -> (ref string -> own Rc<Mutex<[unit -> i64]>> -> unit)\nfn make_put _u = \\s c -> Mutex.with_lock (Rc.get (ref c)) (\\slot -> Array.write 0 (\\() -> String.length s) slot)\ndef put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let f = make_put ()\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    Array.write 0 (Rc.share (ref lr)) slot\n    let local = to_string 123456789\n    f (ref local) lr\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "store_local_borrow_through_reference",
            (9, 5),
        ),
        // e08: the owner given to a function-valued parameter
        (
            "def put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> (ref string -> Rc<Mutex<[unit -> i64]>> -> unit) -> unit\nfn put slot f =\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    Array.write 0 (Rc.share (ref lr)) slot\n    let local = to_string 123456789\n    f (ref local) lr\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr) (\\s c -> Mutex.with_lock (Rc.get (ref c)) (\\slot -> Array.write 0 (\\() -> String.length s) slot))\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\slot -> slot[0] ()) + other.length * 0\nrun 123456789",
            "store_local_borrow_through_reference",
            (6, 5),
        ),
        // e01 (the share went to a value whose drop reads the block)
        (
            "record CellReader { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<CellReader> {\n    fn drop r = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (Arc.get (ref r.cell)) (\\slot -> slot[0] ())))\n}\ndef put_into :: ref string -> own Arc<Mutex<[unit -> i64]>> -> unit\nfn put_into s a = Mutex.with_lock (Arc.get (ref a)) (\\c -> Array.write 0 (\\() -> String.length s) c)\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let reader = CellReader { cell: Arc.share (ref a) }\n    let text0 = to_string n\n    put_into (ref text0) a\n    0\nrun 123456789",
            "assignment_extends_borrow",
            (9, 5),
        ),
    ] {
        rejects_at(source, "E1013", kind, at);
    }
    for main in [
        // e06_ctl
        "def put_into :: ref string -> own Rc<Mutex<[unit -> i64]>> -> unit\nfn put_into s a = Mutex.with_lock (Rc.get (ref a)) (\\c -> Array.write 0 (\\() -> String.length s) c)\ndef put :: ref string -> ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put text slot =\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    Array.write 0 (Rc.share (ref lr)) slot\n    put_into text lr\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string 123456789\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref text0) (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0 + n * 0\nrun 1",
        // e01_ctl
        "record CellReader { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<CellReader> {\n    fn drop r = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (Arc.get (ref r.cell)) (\\slot -> slot[0] ())))\n}\ndef put_into :: ref string -> own Arc<Mutex<[unit -> i64]>> -> unit\nfn put_into s a = Mutex.with_lock (Arc.get (ref a)) (\\c -> Array.write 0 (\\() -> String.length s) c)\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let reader = CellReader { cell: Arc.share (ref a) }\n    put_into (ref text0) a\n    0\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 8 (finding 1): the borrow of a lock's contents that `Mutex.with_lock`,
/// `RwMutex.with_read`, `RwMutex.with_write` and `Cond.wait_with` lend to their callback is valid
/// only while the callback runs: a later write under the lock (a shared reference to the lock is
/// enough) may replace and free what it points at. A callback that keeps the borrow anywhere beyond
/// the call (written in place into another cell, handed to a registry, passed on through a wrapper,
/// a partial application of the lock or `SyncMap.with_value`) is rejected at the call, as a result
/// of the callback that holds it is (F10). Copies of what the contents hold (a function value, an
/// owned value, a new strong pointer) may go anywhere.
#[test]
fn a_borrow_of_a_locked_value_cannot_be_kept_beyond_its_callback() {
    for (source, at) in [
        // m08 (written into another mutex)
        (
            "def grow :: ref mut Vec<string> -> unit\nfn grow v =\n    let mut i = 0\n    while i < 100 do\n        Vec.push_mut (to_string i) v\n        i = i + 1\ndef run :: i64 -> i64\nfn run n =\n    let mut v: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v)\n    let m = Mutex.create v\n    let m2 = Mutex.create [\\() -> 0]\n    let rm2 = ref m2\n    Mutex.with_lock (ref m) (\\a -> { let sr = ref a[0]; Mutex.with_lock rm2 (\\b -> Array.write 0 (\\() -> String.length sr) b) })\n    Mutex.with_lock (ref m) (\\a -> grow a)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock rm2 (\\b -> b[0] ()) + other.length * 0\nrun 123456789",
            (14, 5),
        ),
        // m11 (a read lock's borrow)
        (
            "def grow :: ref mut Vec<string> -> unit\nfn grow v =\n    let mut i = 0\n    while i < 100 do\n        Vec.push_mut (to_string i) v\n        i = i + 1\ndef run :: i64 -> i64\nfn run n =\n    let mut v: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v)\n    let m = RwMutex.create v\n    let m2 = Mutex.create [\\() -> 0]\n    let rm2 = ref m2\n    RwMutex.with_read (ref m) (\\a -> { let sr = ref a[0]; Mutex.with_lock rm2 (\\b -> Array.write 0 (\\() -> String.length sr) b) })\n    RwMutex.with_write (ref m) (\\a -> grow a)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock rm2 (\\b -> b[0] ()) + other.length * 0\nrun 123456789",
            (14, 5),
        ),
        // m13 (handed to a registry)
        (
            "def grow :: ref mut Vec<string> -> unit\nfn grow v =\n    let mut i = 0\n    while i < 100 do\n        Vec.push_mut (to_string i) v\n        i = i + 1\ndef make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let mut v: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v)\n    let m = Mutex.create v\n    let reg = make_registry ()\n    let rreg = ref reg\n    Mutex.with_lock (ref m) (\\a -> { let sr = ref a[0]; let _k = reg (\\() -> String.length sr); () })\n    Mutex.with_lock (ref m) (\\a -> grow a)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            (18, 5),
        ),
        // m30 (`SyncMap.with_value`)
        (
            "def run :: i64 -> i64\nfn run n =\n    let sm: SyncMap<i64, string> = SyncMap.create ()\n    let one = 1\n    SyncMap.store (ref sm) 1 (to_string n)\n    let m2 = Mutex.create [\\() -> 0]\n    let rm2 = ref m2\n    let _r = SyncMap.with_value (ref sm) (ref one) (\\v -> Mutex.with_lock rm2 (\\b -> Array.write 0 (\\() -> String.length v) b))\n    let mut i = 0\n    while i < 50 do\n        SyncMap.store (ref sm) 1 (to_string (i * 1000000007))\n        i = i + 1\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock rm2 (\\b -> b[0] ()) + other.length * 0\nrun 123456789",
            (8, 14),
        ),
        // a04: the body of `Cond.wait_with`
        (
            "def grow :: ref mut Vec<string> -> unit\nfn grow v =\n    let mut i = 0\n    while i < 100 do\n        Vec.push_mut (to_string i) v\n        i = i + 1\ndef run :: i64 -> i64\nfn run n =\n    let mut v: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v)\n    let m = Mutex.create v\n    let c = Cond.create ()\n    let m2 = Mutex.create [\\() -> 0]\n    let rm2 = ref m2\n    Cond.wait_with (ref c) (ref m) (\\a -> true) (\\a -> { let sr = ref a[0]; Mutex.with_lock rm2 (\\b -> Array.write 0 (\\() -> String.length sr) b) })\n    Mutex.with_lock (ref m) (\\a -> grow a)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock rm2 (\\b -> b[0] ()) + other.length * 0\nrun 123456789",
            (15, 5),
        ),
        // a07: a partial application of a named function as the callback
        (
            "def grow :: ref mut Vec<string> -> unit\nfn grow v =\n    let mut i = 0\n    while i < 100 do\n        Vec.push_mut (to_string i) v\n        i = i + 1\ndef put :: ref Mutex<[unit -> i64]> -> ref mut Vec<string> -> unit\nfn put m2 a =\n    let sr = ref a[0]\n    Mutex.with_lock m2 (\\b -> Array.write 0 (\\() -> String.length sr) b)\ndef run :: i64 -> i64\nfn run n =\n    let mut v: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v)\n    let m = Mutex.create v\n    let m2 = Mutex.create [\\() -> 0]\n    Mutex.with_lock (ref m) (put (ref m2))\n    Mutex.with_lock (ref m) (\\a -> grow a)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (ref m2) (\\b -> b[0] ()) + other.length * 0\nrun 123456789",
            (17, 5),
        ),
        // a08: a wrapper that passes the borrow to its function-valued parameter
        (
            "def grow :: ref mut Vec<string> -> unit\nfn grow v =\n    let mut i = 0\n    while i < 100 do\n        Vec.push_mut (to_string i) v\n        i = i + 1\ndef with_first :: ref Mutex<Vec<string>> -> (ref string -> unit) -> unit\nfn with_first m f = Mutex.with_lock m (\\a -> f (ref a[0]))\ndef run :: i64 -> i64\nfn run n =\n    let mut v: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v)\n    let m = Mutex.create v\n    let m2 = Mutex.create [\\() -> 0]\n    let rm2 = ref m2\n    with_first (ref m) (\\s -> Mutex.with_lock rm2 (\\b -> Array.write 0 (\\() -> String.length s) b))\n    Mutex.with_lock (ref m) (\\a -> grow a)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock rm2 (\\b -> b[0] ()) + other.length * 0\nrun 123456789",
            (16, 5),
        ),
        // a09: a partial application of the lock
        (
            "def grow :: ref mut Vec<string> -> unit\nfn grow v =\n    let mut i = 0\n    while i < 100 do\n        Vec.push_mut (to_string i) v\n        i = i + 1\ndef run :: i64 -> i64\nfn run n =\n    let mut v: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v)\n    let m = Mutex.create v\n    let m2 = Mutex.create [\\() -> 0]\n    let rm2 = ref m2\n    let lock = Mutex.with_lock (ref m)\n    lock (\\a -> { let sr = ref a[0]; Mutex.with_lock rm2 (\\b -> Array.write 0 (\\() -> String.length sr) b) })\n    Mutex.with_lock (ref m) (\\a -> grow a)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock rm2 (\\b -> b[0] ()) + other.length * 0\nrun 123456789",
            (15, 5),
        ),
    ] {
        rejects_at(source, "E1013", "mutex_result_retains_borrow", at);
    }
    for main in [
        // a01: a function value copied out of the contents
        "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> n]\n    let m2 = Mutex.create [\\() -> 0]\n    let rm2 = ref m2\n    Mutex.with_lock (ref m) (\\a -> { let f = a[0]; Mutex.with_lock rm2 (\\b -> Array.write 0 f b) })\n    Mutex.with_lock (ref m) (\\a -> Array.write 0 (\\() -> 5) a)\n    Mutex.with_lock rm2 (\\b -> b[0] ())\nrun 9",
        // a02: an owned value computed from the contents
        "def grow :: ref mut Vec<string> -> unit\nfn grow v =\n    let mut i = 0\n    while i < 100 do\n        Vec.push_mut (to_string i) v\n        i = i + 1\ndef run :: i64 -> i64\nfn run n =\n    let mut v: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v)\n    let m = Mutex.create v\n    let m2 = Mutex.create [\\() -> 0]\n    let rm2 = ref m2\n    Mutex.with_lock (ref m) (\\a -> { let len = String.length (ref a[0]); Mutex.with_lock rm2 (\\b -> Array.write 0 (\\() -> len) b) })\n    Mutex.with_lock (ref m) (\\a -> grow a)\n    Mutex.with_lock rm2 (\\b -> b[0] ())\nrun 123456789",
        // a03: a new strong pointer to a block in the contents
        "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [Arc.new (to_string n)]\n    let m2 = Mutex.create [Arc.new \"x\"]\n    let rm2 = ref m2\n    Mutex.with_lock (ref m) (\\a -> { let s = Arc.share (ref a[0]); Mutex.with_lock rm2 (\\b -> Array.write 0 s b) })\n    Mutex.with_lock (ref m) (\\a -> Array.write 0 (Arc.new \"yy\") a)\n    Mutex.with_lock rm2 (\\b -> String.length (Arc.get (ref b[0])))\nrun 123456789",
        // a05: the borrow stays in the callback
        "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [to_string n]\n    let k = Mutex.with_lock (ref m) (\\a -> { let sr = ref a[0]; String.length sr })\n    Mutex.with_lock (ref m) (\\a -> Array.write 0 (to_string 1) a)\n    k + Mutex.with_lock (ref m) (\\a -> String.length (ref a[0]))\nrun 123456789",
        // a06: a borrow of an outer value written into the contents
        "def run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let m = Mutex.create [\\() -> 0]\n    Mutex.with_lock (ref m) (\\a -> Array.write 0 (\\() -> String.length t) a)\n    Mutex.with_lock (ref m) (\\a -> a[0] ())\nrun 123456789",
        // a10: a wrapper whose function value only reads the borrow
        "def with_first :: ref Mutex<Vec<string>> -> (ref string -> i64) -> i64\nfn with_first m f = Mutex.with_lock m (\\a -> f (ref a[0]))\ndef run :: i64 -> i64\nfn run n =\n    let mut v: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v)\n    let m = Mutex.create v\n    with_first (ref m) (\\s -> String.length s)\nrun 123456789",
        // a11: `SyncMap.with_value` with a reader
        "def run :: i64 -> i64\nfn run n =\n    let sm: SyncMap<i64, string> = SyncMap.create ()\n    let one = 1\n    SyncMap.store (ref sm) 1 (to_string n)\n    match SyncMap.with_value (ref sm) (ref one) (\\v -> String.length v) with\n    | Maybe.Some k -> k\n    | Maybe.None -> 0\nrun 123456789",
        // a13: a function value copied out of a read lock
        "def run :: i64 -> i64\nfn run n =\n    let text0 = to_string n\n    let t = ref text0\n    let m = RwMutex.create [\\() -> String.length t]\n    let m2 = Mutex.create [\\() -> 0]\n    let rm2 = ref m2\n    RwMutex.with_read (ref m) (\\a -> { let f = a[0]; Mutex.with_lock rm2 (\\b -> Array.write 0 f b) })\n    Mutex.with_lock rm2 (\\b -> b[0] ())\nrun 123456789",
        // a17: a callback that the check does not see, given a reference to contents that hold no
        // borrow, in a module whose registries own their cells
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Handler { f: ref mut Vec<string> -> unit }\ndef make_handler :: unit -> Handler\nfn make_handler _u = Handler { f: grow }\ndef grow :: ref mut Vec<string> -> unit\nfn grow v =\n    let mut i = 0\n    while i < 10 do\n        Vec.push_mut (to_string i) v\n        i = i + 1\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let mut v: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v)\n    let m = Mutex.create v\n    let h = make_handler ()\n    Mutex.with_lock (ref m) h.f\n    reg (\\() -> 1) + Mutex.with_lock (ref m) (\\a -> a.length)\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 8 (finding 2): a `try` handler with an arm that matches every exception
/// (`| e -> e`) lets none through, so the drops of the scopes that an exception would leave for the
/// outer `try` are not checked at the inner raise. A handler that may let one through (a guard)
/// still is.
#[test]
fn a_handler_that_catches_every_exception_raises_nothing_to_the_outer_try() {
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> Result<i64, Exception>\nfn run n =\n    try\n        let mut holder = Caller { f: \\() -> 0 }\n        let text0 = to_string n\n        let t = ref text0\n        holder = Caller { f: \\() -> String.length t }\n        let r = try\n                    @checked n * 9223372036854775807\n                with\n                | e when n < 0 -> e\n        holder = Caller { f: \\() -> 9 }\n        match r with\n        | Ok v -> v\n        | Error _ -> 1\n    with\n    | e -> e\nmatch run 123456789 with\n| Ok v -> v\n| Error _ -> 0",
        "E1013",
        "assignment_extends_borrow",
        (12, 17),
    );
    for main in [
        // c02
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> Result<i64, Exception>\nfn run n =\n    try\n        let mut holder = Caller { f: \\() -> 0 }\n        let text0 = to_string n\n        let t = ref text0\n        holder = Caller { f: \\() -> String.length t }\n        let r = try\n                    @checked n * 9223372036854775807\n                with\n                | e -> e\n        holder = Caller { f: \\() -> 9 }\n        match r with\n        | Ok v -> v\n        | Error _ -> 1\n    with\n    | e -> e\nmatch run 123456789 with\n| Ok v -> v\n| Error _ -> 0",
        // c11: a guarded arm, then one that catches everything
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> Result<i64, Exception>\nfn run n =\n    try\n        let mut holder = Caller { f: \\() -> 0 }\n        let text0 = to_string n\n        let t = ref text0\n        holder = Caller { f: \\() -> String.length t }\n        let r = try\n                    @checked n * 9223372036854775807\n                with\n                | e when n > 5 -> e\n                | e -> e\n        holder = Caller { f: \\() -> 9 }\n        match r with\n        | Ok v -> v\n        | Error _ -> 1\n    with\n    | e -> e\nmatch run 123456789 with\n| Ok v -> v\n| Error _ -> 0",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 8 (finding 3): the errors that name the callee of a function value that may
/// keep a borrow name a lambda by the local it is bound to, and a method of an instance by the
/// method, not by the numbers of the lifted functions.
#[test]
fn an_error_names_a_lifted_callee_as_the_source_does() {
    for (modules, at, name) in [
        // n01: a lambda bound to a local
        (
            vec![(
                "Main.tz",
                "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let leak = \\(r: (unit -> i64) -> i64) (k: i64) -> { let text0 = to_string k; let t = ref text0; let _x = r (\\() -> String.length t); () }\n    leak reg n\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            )],
            (9, 5),
            "'leak' passes it",
        ),
        // n02: a `let rec` lambda
        (
            vec![(
                "Main.tz",
                "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let rec leak = \\(r: (unit -> i64) -> i64) (k: i64) -> { let text0 = to_string k; let t = ref text0; let _x = r (\\() -> String.length t); () }\n    leak reg n\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            )],
            (9, 5),
            "'leak' passes it",
        ),
        // n04: a method of an instance
        (
            vec![
                (
                    "Leaker.tt",
                    "class Leaker<'value> {\n    def leak :: ref 'value -> ((unit -> i64) -> i64) -> unit\n}",
                ),
                (
                    "Main.tz",
                    "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord L { k: i64 }\ninstance Leaker.Leaker<L> {\n    fn leak l r = { let text0 = to_string l.k; let t = ref text0; let _x = r (\\() -> String.length t); () }\n}\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let l = L { k: n }\n    Leaker.Leaker.leak (ref l) reg\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
                ),
            ],
            (13, 5),
            "the instance method 'leak' passes it",
        ),
        // n05: a lambda that writes after it passes the borrow
        (
            vec![(
                "Main.tz",
                "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord P { v: Vec<string> }\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let mut v0: Vec<string> = Vec.empty()\n    Vec.push_mut (to_string n) (ref mut v0)\n    let put = \\(v: ref mut Vec<string>) (r: (unit -> i64) -> i64) -> { let sr = ref v[0]; let _k = r (\\() -> String.length sr); Vec.push_mut (to_string 1) v }\n    put (ref mut v0) reg\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    reg (\\() -> 1) + other.length * 0\nrun 123456789",
            )],
            (12, 5),
            "'put' passes it",
        ),
    ] {
        for edition in [Edition::E2026, Edition::E2027] {
            let error = analyze_modules_with_edition(&modules, edition).expect_err("rejected");
            let main = modules.last().unwrap().1;
            assert_eq!(
                (error.code, error.kind, position(main, error.span.start)),
                ("E1013", Some("store_local_borrow_through_reference"), at),
                "edition {edition}\n{main}\n{}",
                error.message
            );
            assert!(
                error.message.contains(name),
                "edition {edition}\n{main}\n{}",
                error.message
            );
        }
    }
}

/// A31, review round 8 (finding 4): the parameters of a body are dropped when it ends, in the
/// reverse order of their declarations, so a parameter whose drop may run a user `Drop.drop`
/// cannot keep a borrow of a parameter declared after it (assigned, written in place or pushed),
/// in a function or a lambda. A borrow of a `ref` parameter points into the caller's data.
#[test]
fn a_parameter_whose_drop_runs_code_keeps_no_borrow_of_a_later_parameter() {
    for (source, at) in [
        // p01 (an assignment)
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef keep :: own Caller -> own string -> i64\nfn keep mut c s =\n    let t = ref s\n    c = Caller { f: \\() -> String.length t }\n    0\ndef run :: i64 -> i64\nfn run n = keep (Caller { f: \\() -> n }) (to_string 123456789)\nrun 1",
            (7, 5),
        ),
        // p05 (a lambda)
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let g = \\mut c s -> { let t = ref s; c = Caller { f: \\() -> String.length t }; 0 }\n    g (Caller { f: \\() -> n }) (to_string 123456789)\nrun 1",
            (7, 25),
        ),
        // p06 (a write in place)
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef keep :: own [Caller] -> own string -> i64\nfn keep mut cs s =\n    let t = ref s\n    Array.write 0 (Caller { f: \\() -> String.length t }) (ref mut cs)\n    0\ndef run :: i64 -> i64\nfn run n = keep [Caller { f: \\() -> n }] (to_string 123456789)\nrun 1",
            (7, 5),
        ),
        // p07 (a push)
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef keep :: own Vec<Caller> -> own string -> i64\nfn keep mut cs s =\n    let t = ref s\n    Vec.push_mut (Caller { f: \\() -> String.length t }) (ref mut cs)\n    0\ndef run :: i64 -> i64\nfn run n = keep (Vec.empty()) (to_string 123456789)\nrun 1",
            (7, 5),
        ),
        // d13: the borrowed parameter between two others
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> { fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ())) }\ndef keep :: own string -> own Caller -> own string -> i64\nfn keep a mut c s =\n    let t = ref s\n    c = Caller { f: \\() -> String.length t + String.length a }\n    0\ndef run :: i64 -> i64\nfn run n = keep (to_string 12) (Caller { f: \\() -> n }) (to_string 123456789)\nrun 1",
            (5, 5),
        ),
    ] {
        rejects_at(source, "E1013", "assignment_extends_borrow", at);
    }
    for main in [
        // p09 (the borrowed parameter first)
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef keep :: own string -> own Caller -> i64\nfn keep s mut c =\n    let t = ref s\n    c = Caller { f: \\() -> String.length t }\n    0\ndef run :: i64 -> i64\nfn run n = keep (to_string 123456789) (Caller { f: \\() -> n })\nrun 1",
        // d11: a borrow of a `ref` parameter
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> { fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ())) }\ndef keep :: own Caller -> ref string -> i64\nfn keep mut c s =\n    c = Caller { f: \\() -> String.length s }\n    0\ndef run :: i64 -> i64\nfn run n =\n    let text0 = to_string 123456789\n    keep (Caller { f: \\() -> n }) (ref text0)\nrun 1",
        // d12: the holder after the borrowed parameter, returning a value
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> { fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ())) }\ndef keep :: own string -> own Caller -> i64\nfn keep s mut c =\n    let t = ref s\n    c = Caller { f: \\() -> String.length t }\n    c.f ()\ndef run :: i64 -> i64\nfn run n = keep (to_string 123456789) (Caller { f: \\() -> n })\nrun 1",
        // d14: the holder moved away
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> { fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ())) }\ndef sink :: own Caller -> i64\nfn sink c = 0\ndef keep :: own Caller -> own string -> i64\nfn keep mut c s =\n    let t = ref s\n    c = Caller { f: \\() -> String.length t }\n    let k = c.f ()\n    sink c + k\ndef run :: i64 -> i64\nfn run n = keep (Caller { f: \\() -> n }) (to_string 123456789)\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 8 (finding 5): a function that hands only shared borrows of what its `ref mut`
/// parameter points at to a function value (a lambda that writes them into a cell, a registry, one
/// read from an element) leaves a caller that passed a fresh `ref mut` a shared borrow, as a direct
/// store does: the caller may read the value afterwards, and may not write it while the place that
/// took the borrow lives.
#[test]
fn a_shared_borrow_handed_to_a_function_value_leaves_the_caller_a_shared_borrow() {
    for main in [
        // h07
        "record Rec { name: string, count: i64 }\ndef put :: ref mut Rec -> ((unit -> i64) -> unit) -> unit\nfn put r use =\n    let sr = ref r.name\n    use (\\() -> String.length sr)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let mut v0 = Rec { name: to_string n, count: 3 }\n    put (ref mut v0) (\\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot))\n    Mutex.with_lock mr (\\slot -> slot[0] ()) + v0.count\nrun 123456789",
        // h09 (a registry)
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Rec { name: string, count: i64 }\ndef put :: ref mut Rec -> ((unit -> i64) -> i64) -> unit\nfn put r reg =\n    let sr = ref r.name\n    let _k = reg (\\() -> String.length sr)\n    ()\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let mut v0 = Rec { name: to_string n, count: 3 }\n    put (ref mut v0) reg\n    reg (\\() -> 1) + v0.count\nrun 123456789",
        // h10 (a registry read from an element)
        "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Rec { name: string, count: i64 }\ndef put :: ref mut Rec -> ref [((unit -> i64) -> i64)] -> unit\nfn put r regs =\n    let sr = ref r.name\n    let _k = regs[0] (\\() -> String.length sr)\n    ()\ndef run :: i64 -> i64\nfn run n =\n    let regs = [make_registry ()]\n    let mut v0 = Rec { name: to_string n, count: 3 }\n    put (ref mut v0) (ref regs)\n    regs[0] (\\() -> 1) + v0.count\nrun 123456789",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    // h06 (a write of another field of the parameter afterwards; edition 2027)
    analyze_with_edition(
        "record Rec { name: string, count: i64 }\ndef put :: ref mut Rec -> ((unit -> i64) -> unit) -> unit\nfn put r use =\n    let sr = ref r.name\n    use (\\() -> String.length sr)\n    r.count = 5\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let mut v0 = Rec { name: to_string n, count: 0 }\n    put (ref mut v0) (\\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot))\n    Mutex.with_lock mr (\\slot -> slot[0] ()) + v0.count\nrun 123456789",
        Edition::E2027,
    )
    .expect("h06 is accepted");
    for (source, at) in [
        // b01: the caller writes the borrowed field while the mutex lives
        (
            "record Rec { name: string, count: i64 }\ndef put :: ref mut Rec -> ((unit -> i64) -> unit) -> unit\nfn put r use =\n    let sr = ref r.name\n    use (\\() -> String.length sr)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let mr = ref m\n    let mut v0 = Rec { name: to_string n, count: 3 }\n    put (ref mut v0) (\\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot))\n    v0.name = to_string 1\n    Mutex.with_lock mr (\\slot -> slot[0] ()) + v0.count\nrun 123456789",
            (12, 5),
        ),
        // b02: the same through a registry read from an element
        (
            "def make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\nrecord Rec { name: string, count: i64 }\ndef put :: ref mut Rec -> ref [((unit -> i64) -> i64)] -> unit\nfn put r regs =\n    let sr = ref r.name\n    let _k = regs[0] (\\() -> String.length sr)\n    ()\ndef run :: i64 -> i64\nfn run n =\n    let regs = [make_registry ()]\n    let mut v0 = Rec { name: to_string n, count: 3 }\n    put (ref mut v0) (ref regs)\n    v0.name = to_string 1\n    regs[0] (\\() -> 1) + v0.count\nrun 123456789",
            (16, 5),
        ),
    ] {
        let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
        assert_eq!(
            (error.code, error.kind, position(source, error.span.start)),
            ("E1014", Some("borrow_conflict"), at),
            "{source}\n{}",
            error.message
        );
    }
}

/// A31, review round 8 (finding 6): a share of the block of a local `Arc` owner handed to a
/// function-valued parameter is an owner of the block, not a borrow of the local: the caller may
/// pass a function value that keeps it. The block takes what the body writes into it later, so a
/// later write of a borrow of a local value of the body is rejected there.
#[test]
fn a_share_of_a_local_block_handed_to_a_function_parameter_is_an_owner() {
    for main in [
        // s04
        "def put :: (Arc<Mutex<[unit -> i64]>> -> unit) -> unit\nfn put use =\n    let lr = Arc.new (Mutex.create [\\() -> 9])\n    use (Arc.share (ref lr))\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [Arc.new (Mutex.create [\\() -> 0])]\n    let mr = ref m\n    put (\\r -> Mutex.with_lock mr (\\slot -> Array.write 0 r slot))\n    Mutex.with_lock mr (\\slot -> Mutex.with_lock (Arc.get (ref slot[0])) (\\cell -> cell[0] ())) + n * 0\nrun 1",
        // s02 (a later write of an owned function value)
        "def put :: (Arc<Mutex<[unit -> i64]>> -> unit) -> unit\nfn put use =\n    let lr = Arc.new (Mutex.create [\\() -> 0])\n    use (Arc.share (ref lr))\n    Mutex.with_lock (Arc.get (ref lr)) (\\c -> Array.write 0 (\\() -> 9) c)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [Arc.new (Mutex.create [\\() -> 0])]\n    let mr = ref m\n    put (\\r -> Mutex.with_lock mr (\\slot -> Array.write 0 r slot))\n    Mutex.with_lock mr (\\slot -> Mutex.with_lock (Arc.get (ref slot[0])) (\\cell -> cell[0] ())) + n * 0\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    for (source, at) in [
        // s01 (a later write of a borrow of a local value)
        (
            "def put_into :: ref string -> ref Arc<Mutex<[unit -> i64]>> -> unit\nfn put_into s a = Mutex.with_lock (Arc.get a) (\\c -> Array.write 0 (\\() -> String.length s) c)\ndef put :: (Arc<Mutex<[unit -> i64]>> -> unit) -> unit\nfn put use =\n    let lr = Arc.new (Mutex.create [\\() -> 0])\n    use (Arc.share (ref lr))\n    let local = to_string 123456789\n    put_into (ref local) (ref lr)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [Arc.new (Mutex.create [\\() -> 0])]\n    let mr = ref m\n    put (\\r -> Mutex.with_lock mr (\\slot -> Array.write 0 r slot))\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock mr (\\slot -> Mutex.with_lock (Arc.get (ref slot[0])) (\\cell -> cell[0] ())) + other.length * 0 + n * 0\nrun 1",
            (8, 5),
        ),
        // s03 (the share in a record, the owner moved)
        (
            "record Hold { o: Rc<Mutex<[unit -> i64]>> }\ndef put_into :: ref string -> own Hold -> unit\nfn put_into s h = Mutex.with_lock (Rc.get (ref h.o)) (\\c -> Array.write 0 (\\() -> String.length s) c)\ndef put :: ref mut [Rc<Mutex<[unit -> i64]>>..] -> unit\nfn put slot =\n    let lr = Rc.new (Mutex.create [\\() -> 0])\n    Array.write 0 (Rc.share (ref lr)) slot\n    let local = to_string 123456789\n    put_into (ref local) (Hold { o: lr })\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Rc.new (Mutex.create [\\() -> 0])]\n    put (ref mut arr)\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    Mutex.with_lock (Rc.get (ref arr[0])) (\\cell -> cell[0] ()) + other.length * 0 + n * 0\nrun 1",
            (9, 5),
        ),
    ] {
        rejects_at(source, "E1013", "store_local_borrow_through_reference", at);
    }
    // sh19 (review round 6): the function value is a registry that owns its cell, which takes
    // the share, and the body writes a borrow of its `ref string` parameter into the block later:
    // the caller's string is dropped while the registry still holds it.
    rejects_at(
        "def make_registry :: unit -> (Arc<Mutex<[unit -> i64]>> -> i64)\nfn make_registry _u =\n    let cell: Arc<Mutex<[Arc<Mutex<[unit -> i64]>>]>> = Arc.new (Mutex.create [Arc.new (Mutex.create [\\() -> 0])])\n    \\a -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = Mutex.with_lock (Arc.get (ref slot[0])) (\\c -> c[0] ()); Array.write 0 a slot; old })\ndef give :: (Arc<Mutex<[unit -> i64]>> -> i64) -> ref string -> i64\nfn give reg text =\n    let lr = Arc.new (Mutex.create [\\() -> 0])\n    let k = reg (Arc.share (ref lr))\n    Mutex.with_lock (Arc.get (ref lr)) (\\c -> Array.write 0 (\\() -> String.length text) c)\n    k\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let a = { let text0 = to_string n; give reg (ref text0) }\n    let other = [to_string 777777, to_string 88888888, to_string 12345678]\n    a + reg (Arc.new (Mutex.create [\\() -> 0])) + other.length * 0\nrun 123456789",
        "E1013",
        "assignment_extends_borrow",
        (14, 13),
    );
}
/// A31, review round 9 (findings 1 and 5): the variables of a pattern are dropped where the arm
/// ends, in the reverse order of their bindings (`| (c, s) ->` drops `s`, then `c`). A variable
/// whose drop may run a user `Drop.drop` must not keep a borrow of a variable bound after it, which
/// a lock's callback writes into its cell; one bound before it, or a holder moved away, may.
#[test]
fn a_pattern_variable_whose_drop_runs_code_keeps_no_borrow_of_a_later_one() {
    for (source, at) in [
        // F1 (a tuple pattern)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef run :: i64 -> i64\nfn run n =\n    match (Holder { cell: Mutex.create [\\() -> 0] }, String.repeat 4 (ref \"123456789\")) with\n    | (c, s) ->\n        let t = ref s\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n        0\nrun 1",
            (17, 9),
        ),
        // m01 (a lock that holds the drop values)
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    match (Mutex.create [Caller { f: \\() -> 0 }], to_string n) with\n    | (m, s) ->\n        let t = ref s\n        Mutex.with_lock (ref m) (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\n        0\nrun 123456789",
            (9, 9),
        ),
        // po17 (a union payload)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\nunion Pair = P of Holder * string\ndef run :: i64 -> i64\nfn run n =\n    match P (Holder { cell: Mutex.create [\\() -> 0] }, to_string n) with\n    | P (c, s) ->\n        let t = ref s\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> String.length t) b)\n        0\nrun 123456789",
            (10, 9),
        ),
        // po21 (a tuple inside Maybe.Some)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef run :: i64 -> i64\nfn run n =\n    let pair = Maybe.Some (Holder { cell: Mutex.create [\\() -> 0] }, to_string n)\n    match pair with\n    | Maybe.Some (c, s) ->\n        let t = ref s\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> String.length t) b)\n        1\n    | Maybe.None -> 0\nrun 123456789",
            (10, 9),
        ),
        // po22 (a record pattern)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\nrecord Pair { h: Holder, s: string }\ndef run :: i64 -> i64\nfn run n =\n    let p = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: to_string n }\n    match p with\n    | Pair { h = c, s = s } ->\n        let t = ref s\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> String.length t) b)\n        1\nrun 123456789",
            (11, 9),
        ),
        // a16 (the borrowed variable bound last of three)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef run :: i64 -> i64\nfn run n =\n    match (String.repeat 4 (ref \"abcdefghi\"), Holder { cell: Mutex.create [\\() -> 0] }, String.repeat 4 (ref \"123456789\")) with\n    | (s, c, u) ->\n        let t = ref u\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n        0\nrun 1",
            (17, 9),
        ),
        // F1g (the arms of a function)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef keep :: own Holder -> own string -> i64 = \\c0 s0 ->\n    | (c, s) ->\n        let t = ref s\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n        0\ndef run :: i64 -> i64\nfn run n = keep (Holder { cell: Mutex.create [\\() -> 0] }) (String.repeat 4 (ref \"123456789\"))\nrun 1",
            (15, 9),
        ),
    ] {
        rejects_at(source, "E1013", "assignment_extends_borrow", at);
    }
    // a15: a read parameter of edition 2027 borrows the holder, which stays in the arm
    let source = "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef finish :: Holder -> i64\nfn finish h = 0\ndef run :: i64 -> i64\nfn run n =\n    match (Holder { cell: Mutex.create [\\() -> 0] }, String.repeat 4 (ref \"123456789\")) with\n    | (c, s) ->\n        let t = ref s\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n        finish c\nrun 1";
    let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
    assert_eq!(
        (error.code, error.kind, position(source, error.span.start)),
        ("E1013", Some("assignment_extends_borrow"), (19, 9)),
        "{source}\n{}",
        error.message
    );
    // a15: a by-value parameter of edition 2026 moves the holder away
    analyze_with_edition("record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef finish :: Holder -> i64\nfn finish h = 0\ndef run :: i64 -> i64\nfn run n =\n    match (Holder { cell: Mutex.create [\\() -> 0] }, String.repeat 4 (ref \"123456789\")) with\n    | (c, s) ->\n        let t = ref s\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n        finish c\nrun 1", Edition::E2026).expect("a15_arm_holder_moved is accepted in edition 2026");
    for main in [
        // F1c (the string bound first)
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef run :: i64 -> i64\nfn run n =\n    match (String.repeat 4 (ref \"123456789\"), Holder { cell: Mutex.create [\\() -> 0] }) with\n    | (s, c) ->\n        let t = ref s\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n        0\nrun 1",
        // m02
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    match (to_string n, Mutex.create [Caller { f: \\() -> 0 }]) with\n    | (s, m) ->\n        let t = ref s\n        Mutex.with_lock (ref m) (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\n        0\nrun 123456789",
        // po18
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef run :: i64 -> i64\nfn run n =\n    match (to_string n, Holder { cell: Mutex.create [\\() -> 0] }) with\n    | (s, c) ->\n        let t = ref s\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> String.length t) b)\n        0\nrun 123456789",
        // a16c (the borrowed variable bound first of three)
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef run :: i64 -> i64\nfn run n =\n    match (String.repeat 4 (ref \"123456789\"), Holder { cell: Mutex.create [\\() -> 0] }, String.repeat 4 (ref \"abcdefghi\")) with\n    | (u, c, s) ->\n        let t = ref u\n        Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n        0\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 9 (findings 2 and 6): the temporaries of a statement are dropped where it
/// ends in the order in which they were made (from left to right). A callee that keeps a borrow of
/// one temporary argument in another made after it (or in itself) whose drop may run a user
/// `Drop.drop` leaves that drop reading a temporary that is gone; one made before it may keep it.
#[test]
fn a_temporary_whose_drop_runs_code_keeps_no_borrow_of_one_made_before_it() {
    for (source, at) in [
        // F2 (explicit borrows)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef keep :: ref string -> ref Holder -> i64\nfn keep s c =\n    Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look s) b)\n    0\ndef run :: i64 -> i64\nfn run n = keep (ref (String.repeat 4 (ref \"123456789\"))) (ref (Holder { cell: Mutex.create [\\() -> 0] }))\nrun 1",
            (18, 12),
        ),
        // t03
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"caller drop reads \" + to_string (c.f ()))\n}\nrecord Noisy { text: string }\ninstance Drop<Noisy> {\n    fn drop c = do! IO.write_line (\"noisy dropped\")\n}\ndef put :: ref Noisy -> ref Mutex<[Caller]> -> i64\nfn put s m =\n    Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> if s.text == \"123456789\" then 1 else 0 }) slot)\n    0\ndef run :: i64 -> i64\nfn run n = put (ref (Noisy { text: to_string n })) (ref (Mutex.create [Caller { f: \\() -> 7 }]))\nrun 123456789",
            (14, 12),
        ),
        // pa06
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef keep :: ref string -> ref Holder -> i64\nfn keep s c =\n    Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> String.length s) b)\n    0\ndef run :: i64 -> i64\nfn run n = keep (ref (to_string n)) (ref (Holder { cell: Mutex.create [\\() -> 0] }))\nrun 123456789",
            (10, 12),
        ),
        // a09 (the operands of a user Eq)
        (
            "record Holder { cell: Mutex<[unit -> i64]>, text: string }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ninstance Eq<Holder> {\n    fn eq a b = {\n        Mutex.with_lock (ref b.cell) (\\x -> Array.write 0 (\\() -> String.length a.text) x);\n        true\n    }\n    fn ne a b = false\n}\ndef run :: i64 -> i64\nfn run n = if (Holder { cell: Mutex.create [\\() -> 0], text: to_string n }) == (Holder { cell: Mutex.create [\\() -> 0], text: to_string 7 }) then 1 else 0\nrun 123456789",
            (13, 15),
        ),
        // a06t (a callee that keeps a borrow of the whole temporary in itself)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef keep :: ref Pair -> i64\nfn keep p =\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look (ref p.s)) b)\n    0\ndef run :: i64 -> i64\nfn run n = keep (ref (Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }))\nrun 1",
            (19, 12),
        ),
        // a21u (a callee that keeps a borrow of a part dropped first)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef keep :: ref Pair -> i64\nfn keep p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef run :: i64 -> i64\nfn run n = keep (ref (Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }))\nrun 1",
            (20, 12),
        ),
    ] {
        rejects_at(source, "E1013", "assignment_extends_borrow", at);
    }
    // F2r: read parameters of edition 2027 borrow the temporaries
    let source = "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef keep :: string -> Holder -> i64\nfn keep s c =\n    let t = ref s\n    Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef run :: i64 -> i64\nfn run n = keep (String.repeat 4 (ref \"123456789\")) (Holder { cell: Mutex.create [\\() -> 0] })\nrun 1";
    let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
    assert_eq!(
        (error.code, error.kind, position(source, error.span.start)),
        ("E1013", Some("assignment_extends_borrow"), (19, 12)),
        "{source}\n{}",
        error.message
    );
    for main in [
        // F2c (the holder made first)
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef keep :: ref Holder -> ref string -> i64\nfn keep c s =\n    Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look s) b)\n    0\ndef run :: i64 -> i64\nfn run n = keep (ref (Holder { cell: Mutex.create [\\() -> 0] })) (ref (String.repeat 4 (ref \"123456789\")))\nrun 1",
        // t01
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef put :: ref Mutex<[Caller]> -> ref string -> i64\nfn put m s =\n    Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length s }) slot)\n    0\ndef run :: i64 -> i64\nfn run n = put (ref (Mutex.create [Caller { f: \\() -> 0 }])) (ref (to_string n))\nrun 123456789",
        // a21 (a callee that keeps a borrow of a part dropped after the holder)
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef keep :: ref Pair -> i64\nfn keep p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef run :: i64 -> i64\nfn run n = keep (ref (Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }))\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// The prelude of `an_early_exit_before_the_call_keeps_the_order_of_its_borrowed_temporaries`:
/// `keep3` keeps a borrow of its string in its holder's cell, `keep3r` the same with the string
/// first; both take a third argument after the two temporaries.
const EXIT_PRELUDE: &str = r#"record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = {
        let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy")];
        do! IO.write_line ("drop holder reads " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())) + " junk " + to_string junk.length)
    }
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def keep3 :: ref Holder -> ref string -> i64 -> i64
fn keep3 c s n =
    Mutex.with_lock (ref c.cell) (\b -> Array.write 0 (\() -> look s) b)
    n
def keep3r :: ref string -> ref Holder -> i64 -> i64
fn keep3r s c n =
    Mutex.with_lock (ref c.cell) (\b -> Array.write 0 (\() -> look s) b)
    n
def k :: i64 -> i64 -> i64
fn k a b = a + b
"#;

fn exit_program(body: &str) -> String {
    format!("{EXIT_PRELUDE}{body}do! IO.write_line (to_string (run 3))\n")
}

/// A31, Copilot's review of PR #105: the lowering drops the temporaries that a call borrows right
/// after it returns, in the order in which they were made, but a `break`, `continue` or an
/// exception that a `try` of the same body handles drops the pending ones in the reverse order
/// before the scopes (`emit_loop_jump`, `raise`). Such an exit can only happen while the call's
/// arguments are evaluated, before the call, so no temporary holds a borrow of another yet: the
/// order after the call is the only one that the drop-order rule needs. A holder made first stays
/// accepted with an exit in a later argument, or in a later part of the statement after the call
/// (the native and ASan runs print `drop holder reads 8` after a call and `reads 0` for a holder
/// left before its call), and a holder made after the string stays rejected.
#[test]
fn an_early_exit_before_the_call_keeps_the_order_of_its_borrowed_temporaries() {
    const LOOP: &str = "def run :: i64 -> i64\nfn run n =\n    let mut i = 0\n    let mut total = 0\n    while i < n do\n        i = i + 1\n";
    const TRY: &str = "def run :: i64 -> i64\nfn run n =\n    let outcome =\n        try\n";
    const HANDLE: &str = "        with\n        | e -> e\n    match outcome with\n    | Ok v -> v\n    | Error _ -> -1\n";
    const HOLDER: &str = "ref (Holder { cell: Mutex.create [\\() -> 0] })";
    const TEXT: &str = "ref (String.repeat 4 (ref \"123456789\"))";
    for body in [
        // a break in the third argument, after both temporaries are made (t1)
        format!(
            "{LOOP}        total = total + keep3 ({HOLDER}) ({TEXT}) ({{ if i == 2 then break else (); i }})\n    total\n"
        ),
        // a continue there (t2)
        format!(
            "{LOOP}        total = total + keep3 ({HOLDER}) ({TEXT}) ({{ if i == 2 then continue else (); i }})\n    total\n"
        ),
        // an exception raised there and handled by the enclosing try (t3)
        format!("{TRY}            keep3 ({HOLDER}) ({TEXT}) (@checked n * n)\n{HANDLE}"),
        // a break after the call, in a later argument of an outer call (t4)
        format!(
            "{LOOP}        total = total + k (keep3 ({HOLDER}) ({TEXT}) 1) ({{ if i == 2 then break else (); i }})\n    total\n"
        ),
        // an exception raised after the call (t5)
        format!("{TRY}            k (keep3 ({HOLDER}) ({TEXT}) 1) (@checked n * n)\n{HANDLE}"),
        // a break on the right side of a pipeline whose left side is the call (t7)
        format!(
            "{LOOP}        total = total + (keep3 ({HOLDER}) ({TEXT}) 1 |> k ({{ if i == 2 then break else (); i }}))\n    total\n"
        ),
    ] {
        accepts_in_both_editions(&[("Main.tz", &exit_program(&body))]);
    }
    // The holder made after the string: the call drops the string first and the holder's drop
    // reads it (main prints `drop holder reads -1`), whatever the exits do (u1, u2).
    for (body, at) in [
        (
            format!(
                "{LOOP}        total = total + keep3r ({TEXT}) ({HOLDER}) ({{ if i == 2 then break else (); i }})\n    total\n"
            ),
            (29, 25),
        ),
        (
            format!(
                "{LOOP}        total = total + keep3r ({TEXT}) ({HOLDER}) ({{ if i == 2 then continue else (); i }})\n    total\n"
            ),
            (29, 25),
        ),
        (
            format!("{TRY}            keep3r ({TEXT}) ({HOLDER}) (@checked n * n)\n{HANDLE}"),
            (27, 13),
        ),
    ] {
        rejects_at(
            &exit_program(&body),
            "E1013",
            "assignment_extends_borrow",
            at,
        );
    }
}

/// A31, review round 9 (findings 3 and 7): the fields of a record, the elements of a tuple and of
/// a collection are dropped in their order. A value that keeps a borrow of its own part (a lock's
/// callback wrote a borrow of `p.s` into the cell of `p.h`) must not drop that part before a part
/// whose drop may run a user `Drop.drop`, at the end of a block, of a body, or of an arm.
#[test]
fn a_value_whose_parts_run_drop_code_keeps_no_borrow_of_a_part_dropped_before_them() {
    for (source, at) in [
        // sf05
        (
            "record Txt { text: string }\ninstance Drop<Txt> {\n    fn drop t = do! IO.write_line (\"drop txt\")\n}\nrecord Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\nrecord Pair { s: Txt, h: Holder }\ndef run :: i64 -> i64\nfn run n =\n    let p = Pair { s: Txt { text: String.repeat 4 (ref \"123456789\") }, h: Holder { cell: Mutex.create [\\() -> 0] } }\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> match String.find (ref \"9\") (ref t.text) with\n                                                              | Maybe.Some i -> i\n                                                              | Maybe.None -> -1) b)\n    0\nrun 123456789",
            (15, 5),
        ),
        // s01
        (
            "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"caller drop reads \" + to_string (c.f ()))\n}\nrecord Noisy { id: i64, text: string }\ninstance Drop<Noisy> {\n    fn drop c = do! IO.write_line (\"noisy dropped \" + to_string c.id)\n}\nrecord Pair { s: Noisy, m: Mutex<[Caller]> }\ndef run :: i64 -> i64\nfn run n =\n    let r = Pair { s: Noisy { id: n, text: to_string n }, m: Mutex.create [Caller { f: \\() -> 0 }] }\n    let t = ref r.s\n    Mutex.with_lock (ref r.m) (\\slot -> Array.write 0 (Caller { f: \\() -> if t.text == \"123456789\" then 1 else 0 }) slot)\n    0\nrun 123456789",
            (12, 5),
        ),
        // sf10 (elements of an array)
        (
            "record Node { s: string, cell: Mutex<[unit -> i64]> }\ninstance Drop<Node> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop node reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef run :: i64 -> i64\nfn run n =\n    let arr = [Node { s: String.repeat 4 (ref \"123456789\"), cell: Mutex.create [\\() -> 0] }, Node { s: String.repeat 4 (ref \"abcdefghi\"), cell: Mutex.create [\\() -> 0] }]\n    let t = ref arr[0].s\n    Mutex.with_lock (ref arr[1].cell) (\\b -> Array.write 0 (\\() -> match String.find (ref \"9\") t with\n                                                                 | Maybe.Some i -> i\n                                                                 | Maybe.None -> -1) b)\n    0\nrun 123456789",
            (10, 5),
        ),
        // a05 (a nested record)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Wrap { s: string, h: Holder }\nrecord Outer { w: Wrap }\ndef run :: i64 -> i64\nfn run n =\n    let o = Outer { w: Wrap { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } } }\n    let t = ref o.w.s\n    Mutex.with_lock (ref o.w.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
            (17, 5),
        ),
        // a08 (an owned parameter)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef keep :: own Pair -> i64\nfn keep p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef run :: i64 -> i64\nfn run n = keep (Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } })\nrun 1",
            (16, 5),
        ),
        // a18 (a pattern variable)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef run :: i64 -> i64\nfn run n =\n    match Maybe.Some (Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }) with\n    | Maybe.Some p ->\n        let t = ref p.s\n        Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n        0\n    | Maybe.None -> 1\nrun 1",
            (18, 9),
        ),
        // a21u (a callee that keeps a borrow of a part dropped first)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef keep :: ref Pair -> i64\nfn keep p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef run :: i64 -> i64\nfn run n =\n    let p = Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }\n    keep (ref p)\nrun 1",
            (21, 5),
        ),
        // a06u (a callee that keeps a borrow of the whole value in it)
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef keep :: ref Pair -> i64\nfn keep p =\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look (ref p.s)) b)\n    0\ndef run :: i64 -> i64\nfn run n =\n    let p = Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }\n    keep (ref p)\nrun 1",
            (20, 5),
        ),
    ] {
        rejects_at(source, "E1013", "self_referential_store", at);
    }
    for main in [
        // sf06 (the holder declared first)
        "record Txt { text: string }\ninstance Drop<Txt> {\n    fn drop t = do! IO.write_line (\"drop txt\")\n}\nrecord Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\nrecord Pair { h: Holder, s: Txt }\ndef run :: i64 -> i64\nfn run n =\n    let p = Pair { s: Txt { text: String.repeat 4 (ref \"123456789\") }, h: Holder { cell: Mutex.create [\\() -> 0] } }\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> match String.find (ref \"9\") (ref t.text) with\n                                                              | Maybe.Some i -> i\n                                                              | Maybe.None -> -1) b)\n    0\nrun 123456789",
        // s01b
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"caller drop reads \" + to_string (c.f ()))\n}\nrecord Noisy { id: i64, text: string }\ninstance Drop<Noisy> {\n    fn drop c = do! IO.write_line (\"noisy dropped \" + to_string c.id)\n}\nrecord Pair { m: Mutex<[Caller]>, s: Noisy }\ndef run :: i64 -> i64\nfn run n =\n    let r = Pair { s: Noisy { id: n, text: to_string n }, m: Mutex.create [Caller { f: \\() -> 0 }] }\n    let t = ref r.s\n    Mutex.with_lock (ref r.m) (\\slot -> Array.write 0 (Caller { f: \\() -> if t.text == \"123456789\" then 1 else 0 }) slot)\n    0\nrun 123456789",
        // a05c
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Wrap { s: string }\nrecord Outer { h: Holder, w: Wrap }\ndef run :: i64 -> i64\nfn run n =\n    let o = Outer { h: Holder { cell: Mutex.create [\\() -> 0] }, w: Wrap { s: String.repeat 4 (ref \"123456789\") } }\n    let t = ref o.w.s\n    Mutex.with_lock (ref o.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        // a08c
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef keep :: own Pair -> i64\nfn keep p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef run :: i64 -> i64\nfn run n = keep (Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") })\nrun 1",
        // a18c
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef run :: i64 -> i64\nfn run n =\n    match Maybe.Some (Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }) with\n    | Maybe.Some p ->\n        let t = ref p.s\n        Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n        0\n    | Maybe.None -> 1\nrun 1",
        // a21 (a callee that keeps a borrow of a part dropped after the holder)
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef keep :: ref Pair -> i64\nfn keep p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef run :: i64 -> i64\nfn run n =\n    let p = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    keep (ref p)\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 9 (finding 4): an owned parameter (`own m`, or a by-value parameter of
/// edition 2026) that stores a cell owns it: a store into it reaches no data of the caller, and the
/// drop order of the parameters decides. A cell behind a shared pointer may be the caller's too.
#[test]
fn an_owned_parameter_owns_the_cells_that_it_stores() {
    // rg07 (the holder parameter first)
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef keep :: own Mutex<[Caller]> -> own string -> i64\nfn keep m s =\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (Caller { f: \\() -> String.length t }) b)\n    0\nkeep (Mutex.create [Caller { f: \\() -> 0 }]) (to_string 123456789)",
        "E1013",
        "assignment_extends_borrow",
        (7, 5),
    );
    // F2r: by-value parameters of edition 2026
    analyze_with_edition("record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef keep :: string -> Holder -> i64\nfn keep s c =\n    let t = ref s\n    Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef run :: i64 -> i64\nfn run n = keep (String.repeat 4 (ref \"123456789\")) (Holder { cell: Mutex.create [\\() -> 0] })\nrun 1", Edition::E2026).expect("F2r_read_params_2027 is accepted in edition 2026");
    // a02c: the same through a pipeline
    analyze_with_edition("record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef keep :: string -> Holder -> i64\nfn keep s c =\n    let t = ref s\n    Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef run :: i64 -> i64\nfn run n = Holder { cell: Mutex.create [\\() -> 0] } |> keep (String.repeat 4 (ref \"123456789\"))\nrun 1", Edition::E2026).expect("a02c_pipe_safe_2027 is accepted in edition 2026");
    for main in [
        // rg02
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef keep :: own string -> own Mutex<[Caller]> -> i64\nfn keep s m =\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (Caller { f: \\() -> String.length t }) b)\n    0\nkeep (to_string 123456789) (Mutex.create [Caller { f: \\() -> 0 }])",
        // a25: the same in a module with a function value that owns the cell that it writes
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef make_registry :: unit -> ((unit -> i64) -> i64)\nfn make_registry _u =\n    let cell = Arc.new (Mutex.create [\\() -> 0])\n    \\f -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () })\ndef keep :: own string -> own Mutex<[Caller]> -> i64\nfn keep s m =\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (Caller { f: \\() -> String.length t }) b)\n    0\ndef run :: i64 -> i64\nfn run n =\n    let reg = make_registry ()\n    let k = reg (\\() -> 5)\n    k + keep (to_string 123456789) (Mutex.create [Caller { f: \\() -> 0 }])\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    // rg06: an `Arc` that the caller may share
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef keep :: own string -> own Arc<Mutex<[Caller]>> -> i64\nfn keep s m =\n    let t = ref s\n    Mutex.with_lock (Arc.get (ref m)) (\\b -> Array.write 0 (Caller { f: \\() -> String.length t }) b)\n    0\nkeep (to_string 123456789) (Arc.new (Mutex.create [Caller { f: \\() -> 0 }]))",
        "E1013",
        "store_local_borrow_through_reference",
        (8, 5),
    );
}

/// A31, after review round 9 (the merge into A32): a borrow that a callee keeps of a part of what
/// a `ref mut` parameter points at, in that same value (`ref p.s` written into the cell of
/// `p.h`), is a borrow that the caller's value keeps of a part of itself, however the exclusive
/// borrow reaches the callee: a fresh `ref mut p0`, a `ref mut` variable, another function that
/// passes its parameter on, a pipeline, a function-valued parameter, a lambda, a partial
/// application or a function value that the check does not see. Its parts are dropped in the
/// order of their declarations (`Pair { s, h }` drops `s` first), and a later write of the
/// borrowed part conflicts with it. The safe order (`Pair { h, s }`) stays accepted.
#[test]
fn a_borrow_of_a_part_kept_through_an_exclusive_reference_keeps_the_drop_order() {
    const HOLDER: &str = "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\n";
    const UNSAFE: &str = "record Pair { s: string, h: Holder }\n";
    const SAFE: &str = "record Pair { h: Holder, s: string }\n";
    const TARGET: &str = "def target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\n";
    const P0: &str = "Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }";
    // (name, the code after the header and the record, the line and column of the error)
    let spellings: Vec<(&str, String, (usize, usize))> = vec![
        (
            "fn",
            format!(
                "{TARGET}def run :: i64 -> i64\nfn run n =\n    let mut p0 = {P0}\n    target (ref mut p0)\nrun 1"
            ),
            (15, 5),
        ),
        (
            "var",
            format!(
                "{TARGET}def run :: i64 -> i64\nfn run n =\n    let mut p0 = {P0}\n    let r = ref mut p0\n    target r\nrun 1"
            ),
            (15, 5),
        ),
        (
            "relay",
            format!(
                "def inner :: ref mut Pair -> i64\nfn inner p = {{ let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }}\ndef target :: ref mut Pair -> i64\nfn target p = inner p\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = {P0}\n    target (ref mut p0)\nrun 1"
            ),
            (17, 5),
        ),
        (
            "parameter",
            format!(
                "{TARGET}def run :: ref mut Pair -> i64\nfn run p = target p\ndef go :: i64 -> i64\nfn go n =\n    let mut p0 = {P0}\n    run (ref mut p0)\ngo 1"
            ),
            (17, 5),
        ),
        (
            "pipe",
            format!(
                "{TARGET}def run :: i64 -> i64\nfn run n =\n    let mut p0 = {P0}\n    ref mut p0 |> target\nrun 1"
            ),
            (15, 5),
        ),
        (
            "apply",
            format!(
                "{TARGET}def apply :: (ref mut Pair -> i64) -> ref mut Pair -> i64\nfn apply f p = f p\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = {P0}\n    apply target (ref mut p0)\nrun 1"
            ),
            (17, 5),
        ),
        (
            "lambda",
            format!(
                "{TARGET}def run :: i64 -> i64\nfn run n =\n    let k = \\(q: ref mut Pair) -> target q\n    let mut p0 = {P0}\n    k (ref mut p0)\nrun 1"
            ),
            (15, 5),
        ),
        (
            "partial",
            format!(
                "{TARGET}def target2 :: i64 -> ref mut Pair -> i64\nfn target2 k p = {{ let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t + k) b); 0 }}\ndef run :: i64 -> i64\nfn run n =\n    let f = target2 1\n    let mut p0 = {P0}\n    f (ref mut p0)\nrun 1"
            ),
            (17, 5),
        ),
        (
            "unseen",
            format!(
                "{TARGET}def pick :: i64 -> (ref mut Pair -> i64)\nfn pick n = if n > 100 then (\\(q: ref mut Pair) -> 0) else target\ndef run :: i64 -> i64\nfn run n =\n    let f = pick n\n    let mut p0 = {P0}\n    f (ref mut p0)\nrun 1"
            ),
            (17, 5),
        ),
        (
            "owned",
            format!(
                "{TARGET}def run :: own Pair -> i64\nfn run p = {{ let mut q = p; target (ref mut q) }}\nrun ({P0})"
            ),
            (14, 12),
        ),
        (
            "arm",
            format!(
                "{TARGET}def run :: i64 -> i64\nfn run n =\n    match Maybe.Some ({P0}) with\n    | Maybe.Some p -> {{ let mut q = p; target (ref mut q) }}\n    | Maybe.None -> 0\nrun 1"
            ),
            (16, 23),
        ),
        (
            "loop",
            format!(
                "{TARGET}def run :: i64 -> i64\nfn run n =\n    let mut total = 0\n    for i in 0 ..= 1 do\n        let mut p0 = {P0}\n        total = total + target (ref mut p0)\n    total\nrun 1"
            ),
            (17, 9),
        ),
    ];
    for (name, code, at) in &spellings {
        let unsafe_order = format!("{HOLDER}{UNSAFE}{code}");
        rejects_at(&unsafe_order, "E1013", "self_referential_store", *at);
        let safe_order = format!("{HOLDER}{SAFE}{code}");
        for edition in [Edition::E2026, Edition::E2027] {
            analyze_with_edition(&safe_order, edition).unwrap_or_else(|error| {
                panic!(
                    "{name}, edition {edition}\n{safe_order}\n{}: {}",
                    error.code, error.message
                )
            });
        }
    }
    // Edition 2027: the conventions spelling, a function value in a vector that the check does
    // not see, and a write of the borrowed part after the call (in either order of the fields).
    let edition_2027: Vec<(String, &str, (usize, usize))> = vec![
        (
            format!(
                "{HOLDER}{UNSAFE}def target :: mut Pair -> i64 = \\p -> {{ let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }}\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = {P0}\n    target (ref mut p0)\nrun 1"
            ),
            "self_referential_store",
            (14, 5),
        ),
        (
            format!(
                "{HOLDER}{UNSAFE}{TARGET}def run :: i64 -> i64\nfn run n =\n    let mut fs: Vec<ref mut Pair -> i64> = Vec.empty()\n    Vec.push target fs\n    let f = fs[0]\n    let mut p0 = {P0}\n    f (ref mut p0)\nrun 1"
            ),
            "self_referential_store",
            (15, 5),
        ),
        (
            format!(
                "{HOLDER}{UNSAFE}{TARGET}def run :: i64 -> i64\nfn run n =\n    let mut p0 = {P0}\n    let _x = target (ref mut p0)\n    p0.s = String.repeat 2 (ref \"abc\")\n    0\nrun 1"
            ),
            "borrow_conflict",
            (17, 5),
        ),
        (
            format!(
                "{HOLDER}{SAFE}{TARGET}def run :: i64 -> i64\nfn run n =\n    let mut p0 = {P0}\n    let _x = target (ref mut p0)\n    p0.s = String.repeat 2 (ref \"abc\")\n    0\nrun 1"
            ),
            "borrow_conflict",
            (17, 5),
        ),
    ];
    for (source, kind, at) in &edition_2027 {
        let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
        let code = if *kind == "borrow_conflict" {
            "E1014"
        } else {
            "E1013"
        };
        assert_eq!(
            (error.code, error.kind, position(source, error.span.start)),
            (code, Some(*kind), *at),
            "{source}\n{}",
            error.message
        );
    }
    for source in [
        format!(
            "{HOLDER}{SAFE}def target :: mut Pair -> i64 = \\p -> {{ let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }}\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = {P0}\n    target (ref mut p0)\nrun 1"
        ),
        format!(
            "{HOLDER}{SAFE}{TARGET}def run :: i64 -> i64\nfn run n =\n    let mut fs: Vec<ref mut Pair -> i64> = Vec.empty()\n    Vec.push target fs\n    let f = fs[0]\n    let mut p0 = {P0}\n    f (ref mut p0)\nrun 1"
        ),
    ] {
        analyze_with_edition(&source, Edition::E2027)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    }
}

/// A31, after review round 9 (the merge into A32): the value that a callee passes on to a
/// function value (the element that `Array.map`'s lambda hands to its callback) is no temporary
/// of the caller's statement whose drop may read a borrow: the drop-order rule asks what a drop
/// of its type may read (the callee's view of the type, `Arg::ty`), not whether the module has a
/// `Drop` instance. Round 9 rejected `Array.map ({ calls = calls + 1; owned_inc }) [1, 2, 3]` in
/// a module with a `Drop` instance and a vector of lambdas that capture a value (the
/// `conventions` fixture of A32).
#[test]
fn a_value_that_a_callee_passes_on_reads_what_its_type_reads_when_dropped() {
    for main in [
        "record Item { n: i64 }\ninstance Drop<Item> {\n    fn drop item = do! IO.write_line (\"drop \" + to_string item.n)\n}\n\ndef owned_inc :: own i64 -> i64 = \\value -> value + 1\n\ndef callback_constructed_once :: i64 -> i64 = \\n ->\n    let mut calls = 0\n    let values = Array.map ({ calls = calls + 1; owned_inc }) [1, 2, 3]\n    calls * 100 + Array.sum values + n\n\ndef readers_of :: i64 -> i64 = \\n ->\n    let text = \"hello\"\n    let mut readers = Vec.empty()\n    Vec.push (\\(t: string) -> t.length * 10000 + n) readers\n    let first = readers[0]\n    first text\n\ncallback_constructed_once 0 + readers_of 1",
        "record Item { n: i64 }\ninstance Drop<Item> {\n    fn drop item = do! IO.write_line (\"drop \" + to_string item.n)\n}\n\ndef owned_inc :: own i64 -> i64 = \\value -> value + 1\ndef c :: i64 -> i64 = \\n ->\n    let values = Array.map owned_inc [1, 2, 3]\n    Array.sum values + n\ndef readers_of :: i64 -> i64 = \\n ->\n    let text = \"hello\"\n    let mut readers = Vec.empty()\n    Vec.push (\\(t: string) -> t.length * 10000 + n) readers\n    let first = readers[0]\n    first text\nc 0 + readers_of 1",
    ] {
        analyze_with_edition(main, Edition::E2027)
            .unwrap_or_else(|error| panic!("{main}\n{}: {}", error.code, error.message));
    }
}

/// A31, review round 10 (findings 1 and 4): the captures of a lifted lambda belong to the function
/// value's environment, not to a call: each call works on copies of them in its own frame, and the
/// environment, which outlives the call, drops them. So a cell that an `Owned.function` lambda
/// captured by value is the caller's data (a store into it is a store fact, and the call of the
/// owned function keeps what it is given where the environment reaches), and a borrow of a capture
/// is one of the call's frame. Round 9 took the captures for owned parameters and accepted these
/// programs, which read freed memory. A safe order stays accepted, and the reference that the call
/// goes through (`Owned.call (ref w) x`) is no borrow that the function value's code keeps.
#[test]
fn a_function_value_owns_its_captures_and_its_code_works_on_copies_of_them() {
    // b06m_minimal
    rejects_at(
        "def look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let of = Owned.function (\\(f: unit -> i64) -> Mutex.with_lock (ref m) (\\b -> { let old = b[0]; Array.write 0 f b; old () }))\n    let a = { let s = String.repeat 4 (ref \"123456789\"); let t = ref s; Owned.call (ref of) (\\() -> look t) }\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    a + Owned.call (ref of) (\\() -> 0) + junk.length * 0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (10, 13),
    );
    // b14_owned_function_drop_holder_reads
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef run :: i64 -> i64\nfn run n =\n    let h = Holder { cell: Mutex.create [\\() -> 0] }\n    let of = Owned.function (\\(f: unit -> i64) -> Mutex.with_lock (ref h.cell) (\\b -> { Array.write 0 f b; 0 }))\n    let a = { let s = String.repeat 4 (ref \"123456789\"); let t = ref s; Owned.call (ref of) (\\() -> look t) }\n    a\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (17, 13),
    );
    // b10_owned_function_helper_ref_param
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef put_in :: ref Owned.Function<unit -> i64, i64> -> i64\nfn put_in w =\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    Owned.call w (\\() -> look t)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let of = Owned.function (\\(f: unit -> i64) -> Mutex.with_lock (ref m) (\\b -> { let old = b[0]; Array.write 0 f b; old () }))\n    let a = put_in (ref of)\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    a + Owned.call (ref of) (\\() -> 0) + junk.length * 0\nrun 1",
        "E1013",
        "store_local_borrow_through_reference",
        (22, 13),
    );
    // of_outer_unsafe
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [Caller { f: \\() -> 0 }]\n    let w = Owned.function (\\(g: unit -> i64) -> Mutex.with_lock (ref m) (\\slot -> Array.write 0 (Caller { f: g }) slot))\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    Owned.call (ref w) (\\() -> String.length t)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (7, 5),
    );
    // of_local_into_capture
    rejects_at(
        "def run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let w = Owned.function (\\(k: i64) -> if k == 1 then { let s2 = String.repeat 4 (ref \"123456789\"); let t = ref s2; Mutex.with_lock (ref m) (\\slot -> Array.write 0 (\\() -> String.length t) slot); 0 } else Mutex.with_lock (ref m) (\\slot -> slot[0] ()))\n    let a = Owned.call (ref w) 1\n    let junk = String.repeat 8 (ref \"abcdefghi\")\n    let b = Owned.call (ref w) 2\n    a + b + junk.length * 0\nrun 123456789",
        "E1013",
        "store_local_borrow_through_reference",
        (4, 119),
    );
    // x1_owned_fn_safe
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let s = to_string n\n    let m = Mutex.create [Caller { f: \\() -> 0 }]\n    let w = Owned.function (\\(k: i64) -> { let t = ref s; Mutex.with_lock (ref m) (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot); k })\n    Owned.call (ref w) 1\nrun 123456789",
        "E1013",
        "store_local_borrow_through_reference",
        (9, 59),
    );
    for main in [
        // b06c_owned_function_mutex_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef run :: i64 -> i64\nfn run n =\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    let m = Mutex.create [\\() -> 0]\n    let of = Owned.function (\\(f: unit -> i64) -> Mutex.with_lock (ref m) (\\b -> { let old = b[0]; Array.write 0 f b; old () }))\n    let a = Owned.call (ref of) (\\() -> look t)\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    a + Owned.call (ref of) (\\() -> 0) + junk.length * 0\nrun 1",
        // b05c_owned_function_call_stores_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef run :: i64 -> i64\nfn run n =\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    let h = Holder { cell: Mutex.create [\\() -> 0] }\n    let of = Owned.function (\\(f: unit -> i64) -> Mutex.with_lock (ref h.cell) (\\b -> { Array.write 0 f b; 0 }))\n    Owned.call (ref of) (\\() -> look t)\nrun 1",
        // of_outer_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef run :: i64 -> i64\nfn run n =\n    let s = String.repeat 4 (ref \"123456789\")\n    let m = Mutex.create [Caller { f: \\() -> 0 }]\n    let w = Owned.function (\\(g: unit -> i64) -> Mutex.with_lock (ref m) (\\slot -> Array.write 0 (Caller { f: g }) slot))\n    let t = ref s\n    Owned.call (ref w) (\\() -> String.length t)\n    0\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 10 (finding 5): a value that a callee passes on to a function value is, for
/// the drop-order rule of temporaries, the temporary that it points at when it is a reference (or a
/// view): `apply target (ref (Pair {...}))` with `fn apply f p = f p` lends the temporary `Pair`,
/// whose drop reads. A parameter that the callee passes on unchanged is the caller's argument, so
/// the order of two temporaries is that of their expressions (`apply2 put2 (ref m) (ref s)` makes
/// the cell first, which is dropped first and may read the string). Round 10 judged the reference
/// type itself, whose drop reads nothing, and accepted the unsafe order.
#[test]
fn a_reference_that_a_callee_passes_on_points_at_the_temporary_whose_drop_may_read() {
    // tmp_ho_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef apply :: own (ref Pair -> i64) -> ref Pair -> i64\nfn apply f p = f p\ndef target :: ref Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    apply target (ref (Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }))\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (17, 5),
    );
    // tmp2_ho_unsafe
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef put :: ref string -> ref Mutex<[Caller]> -> unit\nfn put t m = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\ndef put2 :: ref Mutex<[Caller]> -> ref string -> unit\nfn put2 m t = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\ndef apply2 :: own (ref string -> ref Mutex<[Caller]> -> unit) -> ref string -> ref Mutex<[Caller]> -> unit\nfn apply2 f a b = f a b\napply2 put (ref (String.repeat 4 (ref \"123456789\"))) (ref (Mutex.create [Caller { f: \\() -> 0 }]))",
        "E1013",
        "assignment_extends_borrow",
        (11, 1),
    );
    // tmp_ho_elem_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef apply :: own (ref Pair -> i64) -> ref [Pair] -> i64\nfn apply f ps = f (ref ps[0])\ndef target :: ref Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    apply target (ref [Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }])\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (17, 5),
    );
    // iterir_tmp_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef run :: i64 -> i64\nfn run n =\n    Array.iterir (\\i (p: ref Pair) -> { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b) }) (ref [Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }])\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (13, 5),
    );
    // t02_relay2_tmp_unsafe
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef put :: ref string -> ref Mutex<[Caller]> -> unit\nfn put t m = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\ndef apply2 :: own (ref string -> ref Mutex<[Caller]> -> unit) -> ref string -> ref Mutex<[Caller]> -> unit\nfn apply2 f a b = f a b\ndef relay :: own (ref string -> ref Mutex<[Caller]> -> unit) -> ref string -> ref Mutex<[Caller]> -> unit\nfn relay f a b = apply2 f a b\nrelay put (ref (String.repeat 4 (ref \"123456789\"))) (ref (Mutex.create [Caller { f: \\() -> 0 }]))",
        "E1013",
        "assignment_extends_borrow",
        (11, 1),
    );
    // t03_swap_args_unsafe
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef put2 :: ref Mutex<[Caller]> -> ref string -> unit\nfn put2 m t = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\ndef flip :: own (ref Mutex<[Caller]> -> ref string -> unit) -> ref string -> ref Mutex<[Caller]> -> unit\nfn flip f b a = f a b\nflip put2 (ref (String.repeat 4 (ref \"123456789\"))) (ref (Mutex.create [Caller { f: \\() -> 0 }]))",
        "E1013",
        "assignment_extends_borrow",
        (9, 1),
    );
    for main in [
        // tmp2_ho_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef put :: ref string -> ref Mutex<[Caller]> -> unit\nfn put t m = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\ndef put2 :: ref Mutex<[Caller]> -> ref string -> unit\nfn put2 m t = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\ndef apply2 :: own (ref Mutex<[Caller]> -> ref string -> unit) -> ref Mutex<[Caller]> -> ref string -> unit\nfn apply2 f a b = f a b\napply2 put2 (ref (Mutex.create [Caller { f: \\() -> 0 }])) (ref (String.repeat 4 (ref \"123456789\")))",
        // t01_relay2_tmp_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef put2 :: ref Mutex<[Caller]> -> ref string -> unit\nfn put2 m t = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\ndef apply2 :: own (ref Mutex<[Caller]> -> ref string -> unit) -> ref Mutex<[Caller]> -> ref string -> unit\nfn apply2 f a b = f a b\ndef relay :: own (ref Mutex<[Caller]> -> ref string -> unit) -> ref Mutex<[Caller]> -> ref string -> unit\nfn relay f a b = apply2 f a b\nrelay put2 (ref (Mutex.create [Caller { f: \\() -> 0 }])) (ref (String.repeat 4 (ref \"123456789\")))",
        // t04_swap_args_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef put2 :: ref Mutex<[Caller]> -> ref string -> unit\nfn put2 m t = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\ndef flip :: own (ref Mutex<[Caller]> -> ref string -> unit) -> ref Mutex<[Caller]> -> ref string -> unit\nfn flip f a b = f a b\nflip put2 (ref (Mutex.create [Caller { f: \\() -> 0 }])) (ref (String.repeat 4 (ref \"123456789\")))",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 10 (finding 6): the target of a `ref mut [Pair..]` parameter is a view, which
/// drops nothing itself; its elements are those of the array behind it, which that array drops. A
/// callee that keeps a borrow of `ps[0].s` in the cell of `ps[0].h` (`Pair { s, h }` drops `s`
/// first) proves no drop order, whichever part of the array the caller lends.
#[test]
fn a_view_parameter_borrows_the_parts_of_the_array_behind_it() {
    // slice_va
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef target :: ref mut [Pair..] -> i64\nfn target ps = { let t = ref ps[0].s; Mutex.with_lock (ref ps[0].h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }]\n    target (ref mut arr)\nrun 1",
        "E1013",
        "self_referential_store",
        (15, 5),
    );
    // slice_self_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef target :: ref mut [Pair..] -> i64\nfn target ps = { let t = ref ps[0].s; Mutex.with_lock (ref ps[0].h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut arr = [Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }]\n    target (ref mut arr[0..<1])\nrun 1",
        "E1013",
        "self_referential_store",
        (15, 5),
    );
}

/// A31, review round 10 (finding 7): the block of an `Rc` holds one value, whose fields the places
/// through the owner name: a borrow of `(Rc.get (ref rc)).s` kept in the cell of the same block's
/// `h` is read while `s` is still there when `h` is declared first, whichever owner of the block
/// drops it last. Round 9 took the step into the block for an unordered one.
#[test]
fn a_borrow_of_a_part_of_a_shared_block_kept_in_the_block_keeps_the_order_of_its_value() {
    // rc_parts_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } })\n    let p = Rc.get (ref rc)\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        "E1013",
        "self_referential_store",
        (13, 5),
    );
    // rc_share_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } })\n    let rc2 = Rc.share (ref rc)\n    let t = ref (Rc.get (ref rc)).s\n    Mutex.with_lock (ref (Rc.get (ref rc2)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        "E1013",
        "self_referential_store",
        (13, 5),
    );
    for main in [
        // rc_parts_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } })\n    let p = Rc.get (ref rc)\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        // rc_share_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } })\n    let rc2 = Rc.share (ref rc)\n    let t = ref (Rc.get (ref rc)).s\n    Mutex.with_lock (ref (Rc.get (ref rc2)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 10 (finding 3): `p1 = mkp 7` makes the new value, then drops the old one: the
/// borrows of the old value's own parts that it keeps (a lock's callback, or a callee, put a
/// borrow of `p1.s` into the cell of `p1.h`) are read by that drop while the parts are there when
/// the fields keep the order, and end with it, so they do not conflict with the write. The unsafe
/// order is rejected where the old value is dropped, and a borrow that something else still holds
/// conflicts. A callee keeps a borrow of the part that it borrowed (`p.s`), so a later write of
/// another field (`p1.n = 9`) does not conflict with it, and one of the borrowed field does.
#[test]
fn replacing_a_whole_value_ends_the_borrows_of_its_own_parts_that_it_keeps() {
    // r01_reassign_unsafe_order
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder, n: i64 }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let t = ref p1.s\n    Mutex.with_lock (ref p1.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    p1 = mkp 7\n    p1.n\nrun 1",
        "E1013",
        "self_referential_store",
        (21, 5),
    );
    // r03_target_unsafe_reassign
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder, n: i64 }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let r = target (ref mut p1)\n    p1 = mkp 7\n    p1.n + r\nrun 1",
        "E1013",
        "self_referential_store",
        (22, 5),
    );
    // r02_reassign_live_borrow
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let t = ref p1.s\n    Mutex.with_lock (ref p1.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    p1 = mkp 7\n    p1.n + look t\nrun 1",
        "E1014",
        "borrow_conflict",
        (21, 5),
    );
    for main in [
        // k12_lock_selfborrow_then_reassign
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef bump :: ref mut Pair -> i64\nfn bump p = { let k = p.n; deref p = Pair { h: Holder { cell: Mutex.create [\\() -> k] }, s: String.repeat 2 (ref \"abc9\"), n: k + 1 }; k }\ndef peek :: ref mut Pair -> i64\nfn peek p = p.n\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let t = ref p1.s\n    Mutex.with_lock (ref p1.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    p1 = mkp 7\n    p1.n\nrun 1",
        // k15_lock_selfborrow_in_loop_reassign
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef bump :: ref mut Pair -> i64\nfn bump p = { let k = p.n; deref p = Pair { h: Holder { cell: Mutex.create [\\() -> k] }, s: String.repeat 2 (ref \"abc9\"), n: k + 1 }; k }\ndef peek :: ref mut Pair -> i64\nfn peek p = p.n\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let mut acc = 0\n    let mut i = 0\n    while i < 3 do\n        let t = ref p1.s\n        Mutex.with_lock (ref p1.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n        acc = acc + p1.n\n        p1 = mkp (i + 2)\n        i = i + 1\n    acc\nrun 1",
        // k10_target_then_reassign
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef bump :: ref mut Pair -> i64\nfn bump p = { let k = p.n; deref p = Pair { h: Holder { cell: Mutex.create [\\() -> k] }, s: String.repeat 2 (ref \"abc9\"), n: k + 1 }; k }\ndef peek :: ref mut Pair -> i64\nfn peek p = p.n\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let a = target (ref mut p1)\n    p1 = mkp 7\n    a + p1.n\nrun 1",
        // r04_reassign_in_branch_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let r = target (ref mut p1)\n    if n > 0 then { p1 = mkp 5 } else {}\n    p1.n + r\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    // Edition 2027 only (the other edition does not write the program).
    for (source, code, kind, at) in [
        // r05_target_then_write_borrowed
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let r = target (ref mut p1)\n    p1.s = String.repeat 2 (ref \"abc\")\n    p1.n + r\nrun 1",
            "E1014",
            "borrow_conflict",
            (22, 5),
        ),
        // r07_keepr_then_write_borrowed
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef keepr :: ref Pair -> i64\nfn keepr p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let r = keepr (ref p1)\n    p1.s = String.repeat 2 (ref \"abc\")\n    p1.n + r\nrun 1",
            "E1014",
            "borrow_conflict",
            (22, 5),
        ),
        // r08_opaque_then_write_borrowed
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef keepr :: ref Pair -> i64\nfn keepr p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef pick :: i64 -> (ref Pair -> i64)\nfn pick k = if k > 0 then keepr else \\p -> 0\ndef run :: i64 -> i64\nfn run n =\n    let f = pick n\n    let mut p1 = mkp 1\n    let r = f (ref p1)\n    p1.s = String.repeat 2 (ref \"abc\")\n    p1.n + r\nrun 1",
            "E1014",
            "borrow_conflict",
            (25, 5),
        ),
    ] {
        let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
        assert_eq!(
            (error.code, error.kind, position(source, error.span.start)),
            (code, Some(kind), at),
            "{source}\n{}",
            error.message
        );
    }
    for source in [
        // k11_target_then_mutate_other_field
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef bump :: ref mut Pair -> i64\nfn bump p = { let k = p.n; deref p = Pair { h: Holder { cell: Mutex.create [\\() -> k] }, s: String.repeat 2 (ref \"abc9\"), n: k + 1 }; k }\ndef peek :: ref mut Pair -> i64\nfn peek p = p.n\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let a = target (ref mut p1)\n    p1.n = 9\n    a + p1.n\nrun 1",
        // kv02_opaque_handler_then_reassign
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef bump :: ref mut Pair -> i64\nfn bump p = { let k = p.n; deref p = Pair { h: Holder { cell: Mutex.create [\\() -> k] }, s: String.repeat 2 (ref \"abc9\"), n: k + 1 }; k }\ndef peek :: ref mut Pair -> i64\nfn peek p = p.n\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef run :: i64 -> i64\nfn run n =\n    let mut other = mkp 5\n    let z = target (ref mut other)\n    let mut handlers: Vec<ref mut Pair -> i64> = Vec.empty()\n    Vec.push bump handlers\n    Vec.push peek handlers\n    let mut p1 = mkp 1\n    let a = handlers[0] (ref mut p1)\n    p1 = mkp 7\n    a + p1.n + z\nrun 1",
        // kv06_opaque_then_part_mutation
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef bump :: ref mut Pair -> i64\nfn bump p = { let k = p.n; deref p = Pair { h: Holder { cell: Mutex.create [\\() -> k] }, s: String.repeat 2 (ref \"abc9\"), n: k + 1 }; k }\ndef peek :: ref mut Pair -> i64\nfn peek p = p.n\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef run :: i64 -> i64\nfn run n =\n    let mut other = mkp 5\n    let z = target (ref mut other)\n    let mut handlers: Vec<ref mut Pair -> i64> = Vec.empty()\n    Vec.push bump handlers\n    Vec.push peek handlers\n    let mut p1 = mkp 1\n    let a = handlers[0] (ref mut p1)\n    p1.n = 9\n    let b = p1.n\n    a + b + z\nrun 1",
        // r06_keepr_then_write_other
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string, n: i64 }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\"), n: k }\ndef keepr :: ref Pair -> i64\nfn keepr p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p1 = mkp 1\n    let r = keepr (ref p1)\n    p1.n = 9\n    p1.n + r\nrun 1",
    ] {
        analyze_with_edition(source, Edition::E2027)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    }
}

/// A31, review round 10 (findings 2 and 8): code that the check does not see keeps a borrow of a
/// part of what a reference argument (`ref T` or `ref mut T`) points at in that same value only as
/// some function of the module does through a reference parameter to a `T`, at the parts that it
/// borrowed and in the order that it kept (`CallFacts::self_borrow_types`). Round 9 stored the
/// whole borrow of a shared reference argument in itself, which rejected the safe order through a
/// function chosen at run time and a shared borrow of a local cell handed to a function-valued
/// parameter. A function that keeps the whole value, or the unsafe order, keeps them rejected.
#[test]
fn unseen_code_keeps_a_borrow_of_a_part_of_a_reference_argument_only_as_a_function_of_the_module_does()
 {
    // q02_opaque_shared_ref_self_unsafe_dynvec
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef keepr :: ref Pair -> i64\nfn keepr p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef pick :: i64 -> (ref Pair -> i64)\nfn pick k = if k > 0 then keepr else \\p -> 0\ndef run :: i64 -> i64\nfn run n =\n    let f = pick n\n    let p0 = Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }\n    f (ref p0)\nrun 1",
        "E1013",
        "self_referential_store",
        (20, 5),
    );
    // o01_opaque_whole_capture_safe_order
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef keepw :: ref Pair -> i64\nfn keepw p = { Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look (ref p.s)) b); 0 }\ndef pick :: i64 -> (ref Pair -> i64)\nfn pick k = if k > 0 then keepw else \\p -> 0\ndef run :: i64 -> i64\nfn run n =\n    let f = pick n\n    let p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    f (ref p0)\nrun 1",
        "E1013",
        "self_referential_store",
        (20, 5),
    );
    // o02_opaque_mutex_self_capture
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef peek :: ref Mutex<[Caller]> -> i64\nfn peek m = 1\ndef keepm :: ref Mutex<[Caller]> -> unit\nfn keepm m = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> peek m }) slot)\ndef pick :: i64 -> (ref Mutex<[Caller]> -> unit)\nfn pick k = if k > 0 then keepm else \\m -> ()\ndef run :: i64 -> i64\nfn run n =\n    let f = pick n\n    let m = Mutex.create [Caller { f: \\() -> 0 }]\n    f (ref m)\n    0\nrun 1",
        "E1013",
        "self_referential_store",
        (25, 5),
    );
    // ho_unsafe
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef put :: ref string -> ref Mutex<[Caller]> -> unit\nfn put t m = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\ndef apply2 :: (ref string -> ref Mutex<[Caller]> -> unit) -> i64\nfn apply2 f =\n    let m = Mutex.create [Caller { f: \\() -> 0 }]\n    let s = String.repeat 4 (ref \"123456789\")\n    f (ref s) (ref m)\n    0\napply2 put",
        "E1013",
        "assignment_extends_borrow",
        (9, 5),
    );
    for main in [
        // q05_fnparam_shared_ref_self_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef keepr :: ref Pair -> i64\nfn keepr p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef pick :: i64 -> (ref Pair -> i64)\nfn pick k = if k > 0 then keepr else \\p -> 0\ndef run :: i64 -> i64\nfn run n =\n    let f = pick n\n    let p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    f (ref p0)\nrun 1",
        // ho_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}\ndef put :: ref string -> ref Mutex<[Caller]> -> unit\nfn put t m = Mutex.with_lock m (\\slot -> Array.write 0 (Caller { f: \\() -> String.length t }) slot)\ndef apply2 :: (ref string -> ref Mutex<[Caller]> -> unit) -> i64\nfn apply2 f =\n    let s = String.repeat 4 (ref \"123456789\")\n    let m = Mutex.create [Caller { f: \\() -> 0 }]\n    f (ref s) (ref m)\n    0\napply2 put",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    // Edition 2027 only (the other edition does not write the program).
    // q01_opaque_shared_ref_self_unsafe
    let source = "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef keepr :: ref Pair -> i64\nfn keepr p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut fs: Vec<ref Pair -> i64> = Vec.empty()\n    Vec.push keepr fs\n    let f = fs[0]\n    let p0 = Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }\n    f (ref p0)\nrun 1";
    let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
    assert_eq!(
        (error.code, error.kind, position(source, error.span.start)),
        ("E1013", Some("self_referential_store"), (18, 5)),
        "{source}\n{}",
        error.message
    );
    for source in [
        // q01c_opaque_shared_ref_self_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef keepr :: ref Pair -> i64\nfn keepr p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut fs: Vec<ref Pair -> i64> = Vec.empty()\n    Vec.push keepr fs\n    let f = fs[0]\n    let p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    f (ref p0)\nrun 1",
        // q04_opaque_refmut_self_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef keepm :: ref mut Pair -> i64\nfn keepm p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut fs: Vec<ref mut Pair -> i64> = Vec.empty()\n    Vec.push keepm fs\n    let f = fs[0]\n    let mut p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    f (ref mut p0)\nrun 1",
    ] {
        analyze_with_edition(source, Edition::E2027)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    }
}

#[test]
fn a_value_that_reads_through_a_borrow_reads_what_the_borrowed_value_holds_when_dropped() {
    // A31, review 11, finding 1: a value whose drop may read (a record with a Drop instance and a
    // function value) holds a borrow of a cell, and a lock callback, a helper or a borrowed record
    // puts a borrow of a local declared after the reader into that cell: the reader reads it
    // through its borrow when it is dropped, after the local is gone.
    // r08_min_unsafe
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n        do! IO.write_line (\"drop reads length \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef read_cell :: ref Mutex<[unit -> i64]> -> unit -> i64\nfn read_cell m u = Mutex.with_lock m (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let c = Caller { f: read_cell (ref m) }\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (\\() -> String.length t) b)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (12, 5),
    );
    // r06_caller_partial_reads_cell
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop caller reads \" + to_string (c.f ()) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef read_cell :: ref Mutex<[unit -> i64]> -> unit -> i64\nfn read_cell m u = Mutex.with_lock m (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let c = Caller { f: read_cell (ref m) }\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (17, 5),
    );
    // r01_watcher_ref_to_cell
    rejects_at(
        "record Watcher {r} { target: ref {r} Mutex<[unit -> i64]> }\ninstance Drop<Watcher> {\n    fn drop w = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop watcher reads \" + to_string (Mutex.with_lock w.target (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let w = Watcher { target: ref m }\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (15, 5),
    );
    // r09_partial_helper_put
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop caller reads \" + to_string (c.f ()) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef read_cell :: ref Mutex<[unit -> i64]> -> unit -> i64\nfn read_cell m u = Mutex.with_lock m (\\b -> b[0] ())\ndef put :: ref Mutex<[unit -> i64]> -> (unit -> i64) -> unit\nfn put m f = Mutex.with_lock m (\\b -> Array.write 0 f b)\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let c = Caller { f: read_cell (ref m) }\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    put (ref m) (\\() -> look t)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (19, 5),
    );
    // r10_partial_rwmutex
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop caller reads \" + to_string (c.f ()) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef read_cell :: ref RwMutex<[unit -> i64]> -> unit -> i64\nfn read_cell m u = RwMutex.with_read m (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let m = RwMutex.create [\\() -> 0]\n    let c = Caller { f: read_cell (ref m) }\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    RwMutex.with_write (ref m) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (17, 5),
    );
    // r12_partial_record_cell
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop caller reads \" + to_string (c.f ()) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Box { cell: Mutex<[unit -> i64]>, k: i64 }\ndef read_box :: ref Box -> unit -> i64\nfn read_box bx u = Mutex.with_lock (ref bx.cell) (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let bx = Box { cell: Mutex.create [\\() -> 0], k: 1 }\n    let c = Caller { f: read_box (ref bx) }\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    Mutex.with_lock (ref bx.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (18, 5),
    );
    // r13_partial_in_vec_holder
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop caller reads \" + to_string (c.f ()) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef read_cell :: ref Mutex<[unit -> i64]> -> unit -> i64\nfn read_cell m u = Mutex.with_lock m (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let cs = [Caller { f: read_cell (ref m) }]\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (\\() -> look t) b)\n    cs.length\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (17, 5),
    );
    // r16_partial_ref_rc
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop caller reads \" + to_string (c.f ()) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef read_rc :: ref Rc<Mutex<[unit -> i64]>> -> unit -> i64\nfn read_rc r u = Mutex.with_lock (Rc.get r) (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let r = Rc.new (Mutex.create [\\() -> 0])\n    let c = Caller { f: read_rc (ref r) }\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    Mutex.with_lock (Rc.get (ref r)) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (17, 5),
    );
    // r17_regioned_record_ref_cell_no_drop_on_cell
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop caller reads \" + to_string (c.f ()) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord View {r} { m: ref {r} Mutex<[unit -> i64]> }\nrecord Reader {r} { v: View {r}, k: i64 }\ninstance Drop<Reader> {\n    fn drop x = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop reader reads \" + to_string (Mutex.with_lock x.v.m (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let rd = Reader { v: View { m: ref m }, k: 1 }\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (23, 5),
    );
    // r22_watcher_ref_box_with_rc
    rejects_at(
        "record Box { r: Rc<Mutex<[unit -> i64]>>, k: i64 }\nrecord Watcher {r} { b: ref {r} Box }\ninstance Drop<Watcher> {\n    fn drop x = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop watcher reads \" + to_string (Mutex.with_lock (Rc.get (ref x.b.r)) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef run :: i64 -> i64\nfn run n =\n    let bx = Box { r: Rc.new (Mutex.create [\\() -> 0]), k: 1 }\n    let wt = Watcher { b: ref bx }\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    let r2 = Rc.share (ref bx.r)\n    Mutex.with_lock (Rc.get (ref r2)) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (16, 5),
    );
    // h03_read_through_two_levels
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n        do! IO.write_line (\"drop reads length \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef read_cell :: ref Mutex<[unit -> i64]> -> unit -> i64\nfn read_cell m u = Mutex.with_lock m (\\b -> b[0] ())\nrecord Wrap {r} { m: ref {r} Mutex<[unit -> i64]> }\ndef read_wrap :: ref Wrap -> unit -> i64\nfn read_wrap w u = Mutex.with_lock w.m (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let m = Mutex.create [\\() -> 0]\n    let w = Wrap { m: ref m }\n    let c = Caller { f: read_wrap (ref w) }\n    let s = String.repeat 4 (ref \"123456789\")\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (\\() -> String.length t) b)\n    0\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (15, 5),
    );
    // h01_read_through_then_move_source
    rejects_at(
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n        do! IO.write_line (\"drop reads length \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef read_cell :: ref Mutex<[unit -> i64]> -> unit -> i64\nfn read_cell m u = Mutex.with_lock m (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let s = String.repeat 4 (ref \"123456789\")\n    let m = Mutex.create [\\() -> 0]\n    let c = Caller { f: read_cell (ref m) }\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (\\() -> String.length t) b)\n    let s2 = s\n    String.length (ref s2)\nrun 1",
        "E1014",
        "borrow_conflict",
        (17, 14),
    );
    for main in [
        // r08_min_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n        do! IO.write_line (\"drop reads length \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef read_cell :: ref Mutex<[unit -> i64]> -> unit -> i64\nfn read_cell m u = Mutex.with_lock m (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let s = String.repeat 4 (ref \"123456789\")\n    let m = Mutex.create [\\() -> 0]\n    let c = Caller { f: read_cell (ref m) }\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (\\() -> String.length t) b)\n    0\nrun 1",
        // r06c_caller_partial_reads_cell_safe
        "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop caller reads \" + to_string (c.f ()) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef read_cell :: ref Mutex<[unit -> i64]> -> unit -> i64\nfn read_cell m u = Mutex.with_lock m (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let s = String.repeat 4 (ref \"123456789\")\n    let m = Mutex.create [\\() -> 0]\n    let c = Caller { f: read_cell (ref m) }\n    let t = ref s\n    Mutex.with_lock (ref m) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

#[test]
fn a_local_replaced_whole_stays_usable_after_the_borrows_of_its_old_parts_end() {
    // A31, review 11, finding 2: the drop of the old value of a whole assignment stored what it
    // held where its own parts reach, so the new value kept the borrow of an old part and every
    // later move, exclusive borrow or write of the local conflicted with it.
    // w02_callee_stores_then_replaces_local
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef run :: i64 -> i64\nfn run n =\n    let o = Holder { cell: Mutex.create [\\() -> 0] }\n    let mut p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    let t = ref p0.s\n    Mutex.with_lock (ref o.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    p0 = mkp 7\n    0\nrun 1",
        "E1014",
        "borrow_conflict",
        (22, 5),
    );
    // d01_old_drop_exports_self
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]>, out: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let out = Arc.share (ref h.out);\n        Mutex.with_lock (ref h.cell) (\\b -> copy_out b (Arc.get (ref out)))\n    }\n}\ndef copy_out :: ref [unit -> i64] -> ref Mutex<[unit -> i64]> -> unit\nfn copy_out b out = Mutex.with_lock out (\\o -> Array.write 0 b[0] o)\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord P { h: Holder, s: string }\ndef mkp :: ref Arc<Mutex<[unit -> i64]>> -> P\nfn mkp a = P { h: Holder { cell: Mutex.create [\\() -> 0], out: Arc.share a }, s: String.repeat 3 (ref \"x9\") }\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let mut q = P { h: Holder { cell: Mutex.create [\\() -> 0], out: Arc.share (ref a) }, s: String.repeat 4 (ref \"123456789\") }\n    let t = ref q.s\n    Mutex.with_lock (ref q.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    q = mkp (ref a)\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    Mutex.with_lock (Arc.get (ref a)) (\\o -> o[0] ()) + junk.length\nrun 1",
        "E1014",
        "borrow_conflict",
        (24, 5),
    );
    for main in [
        // z01_replace_then_use
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord P { h: Holder, s: string }\ndef mkp :: i64 -> P\nfn mkp k = P { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat k (ref \"x9\") }\ndef sink :: own P -> i64\nfn sink p = 1\ndef touch :: ref mut P -> i64\nfn touch p = 2\ndef run :: i64 -> i64\nfn run n =\n    let mut q = P { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    let t = ref q.s\n    Mutex.with_lock (ref q.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    q = mkp 3\n    sink q\nrun 1",
        // z03_replace_then_use
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord P { h: Holder, s: string }\ndef mkp :: i64 -> P\nfn mkp k = P { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat k (ref \"x9\") }\ndef sink :: own P -> i64\nfn sink p = 1\ndef touch :: ref mut P -> i64\nfn touch p = 2\ndef run :: i64 -> i64\nfn run n =\n    let mut q = P { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    let t = ref q.s\n    Mutex.with_lock (ref q.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    q = mkp 3\n    touch (ref mut q)\nrun 1",
        // z04_replace_then_use
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord P { h: Holder, s: string }\ndef mkp :: i64 -> P\nfn mkp k = P { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat k (ref \"x9\") }\ndef sink :: own P -> i64\nfn sink p = 1\ndef touch :: ref mut P -> i64\nfn touch p = 2\ndef run :: i64 -> i64\nfn run n =\n    let mut q = P { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    let t = ref q.s\n    Mutex.with_lock (ref q.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    q = mkp 3\n    { let r = ref mut q; 0 }\nrun 1",
        // z05_keep_replace_then_move_own
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef keep :: ref mut Pair -> i64\nfn keep p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef sink :: own Pair -> i64\nfn sink p = 1\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkp 1\n    let a = keep (ref mut p0)\n    p0 = mkp 7\n    sink p0 + a\nrun 1",
        // w09_self_borrow_shared_param_then_local_replace_in_callee
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef renew :: own Pair -> Pair\nfn renew p =\n    let mut q = p\n    let t = ref q.s\n    Mutex.with_lock (ref q.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    q = mkp 7\n    q\ndef run :: i64 -> i64\nfn run n =\n    let p0 = renew (Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") })\n    0\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    for source in [
        // z02_replace_then_use
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord P { h: Holder, s: string }\ndef mkp :: i64 -> P\nfn mkp k = P { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat k (ref \"x9\") }\ndef sink :: own P -> i64\nfn sink p = 1\ndef touch :: ref mut P -> i64\nfn touch p = 2\ndef run :: i64 -> i64\nfn run n =\n    let mut q = P { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    let t = ref q.s\n    Mutex.with_lock (ref q.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    q = mkp 3\n    { q.s = String.repeat 2 (ref \"9\"); 0 }\nrun 1",
        // w13_local_self_borrow_replace_then_write_field
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef run :: i64 -> i64\nfn run n =\n    let mut q = mkp 1\n    let t = ref q.s\n    Mutex.with_lock (ref q.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    q = mkp 7\n    q.s = \"zz\"\n    0\nrun 1",
    ] {
        analyze_with_edition(source, Edition::E2027)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    }
}

#[test]
fn replacing_the_target_of_a_reference_ends_the_borrows_of_its_own_parts() {
    // A31, review 11, finding 3: `deref p = v` drops the old value of the caller with the borrows
    // of its own parts that the body kept in it; one kept elsewhere too still conflicts.
    // d02_deref_unsafe_order
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { s: string, h: Holder }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { s: String.repeat 4 (ref \"abcdefgh\"), h: Holder { cell: Mutex.create [\\() -> 0] } }\ndef renew :: ref mut Pair -> i64\nfn renew p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = mkp 7\n    0\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }\n    renew (ref mut p0)\nrun 1",
        "E1013",
        "self_referential_store",
        (20, 5),
    );
    // d04_deref_also_elsewhere
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef renew :: ref mut Pair -> ref Holder -> i64\nfn renew p o =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    Mutex.with_lock (ref o.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = mkp 7\n    0\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    let o = Holder { cell: Mutex.create [\\() -> 0] }\n    renew (ref mut p0) (ref o)\nrun 1",
        "E1014",
        "borrow_conflict",
        (21, 5),
    );
    // w01_callee_stores_then_replaces
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef leak :: ref mut Pair -> ref Holder -> i64\nfn leak p o =\n    let t = ref p.s\n    Mutex.with_lock (ref o.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = mkp 7\n    0\ndef run :: i64 -> i64\nfn run n =\n    let o = Holder { cell: Mutex.create [\\() -> 0] }\n    let mut p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    leak (ref mut p0) (ref o)\nrun 1",
        "E1014",
        "borrow_conflict",
        (20, 5),
    );
    // d03_deref_then_use_borrow
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef renew :: ref mut Pair -> i64\nfn renew p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = mkp 7\n    look t\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    renew (ref mut p0)\nrun 1",
        "E1014",
        "borrow_conflict",
        (20, 5),
    );
    for main in [
        // w04_self_borrow_then_deref_replace_same_fn
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef renew :: ref mut Pair -> i64\nfn renew p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = mkp 7\n    0\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    renew (ref mut p0)\nrun 1",
        // w03_callee_stores_self_then_replaces
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef keep :: ref mut Pair -> i64\nfn keep p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef renew :: ref mut Pair -> i64\nfn renew p =\n    let a = keep p\n    deref p = mkp 7\n    a\ndef run :: i64 -> i64\nfn run n =\n    let o = Holder { cell: Mutex.create [\\() -> 0] }\n    let mut p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    renew (ref mut p0)\nrun 1",
        // d06_deref_in_branch
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef renew :: ref mut Pair -> i64\nfn renew p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    if String.length p.s > 3 then { deref p = mkp 7; 0 } else 1\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    renew (ref mut p0)\nrun 1",
        // d07_deref_twice_relock
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop holder reads \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" junk \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord Pair { h: Holder, s: string }\ndef mkp :: i64 -> Pair\nfn mkp k = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"abcdefgh\") }\ndef renew :: ref mut Pair -> i64\nfn renew p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = mkp 7\n    let u = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look u) b)\n    deref p = mkp 8\n    0\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }\n    renew (ref mut p0)\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

#[test]
fn a_part_that_holds_all_that_its_local_holds_is_replaced_like_the_local() {
    // A31, review 11, findings 9 and 10: an assignment of a field whose siblings on the way hold no
    // borrow replaces what the local holds, after the drop order of the old part's parts.
    // Edition 2027 only (the other edition does not write the program).
    for (source, code, kind, at) in [
        // rp3_field_unsafe
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { p: Pair, n: i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { p: mkp (), n: 0 }\n    let t = ref bx.p.s\n    Mutex.with_lock (ref bx.p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    bx.p = mkp ()\n    n\nrun 1",
            "E1013",
            "self_referential_store",
            (38, 5),
        ),
        // f09_field_old_holds_later_then_drop_new
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { h: mkh (), n: 0 }\n    let s = mks ()\n    let t = ref s\n    bx.h = Holder { cell: Mutex.create [\\() -> look t] }\n    let s3 = mks ()\n    let t3 = ref s3\n    bx.h = Holder { cell: Mutex.create [\\() -> look t3] }\n    n\nrun 1",
            "E1013",
            "assignment_extends_borrow",
            (35, 5),
        ),
        // f02_sibling_holds_loans
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, g: Holder }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { h: mkh (), g: mkh () }\n    let s = mks ()\n    let t = ref s\n    bx.h = Holder { cell: Mutex.create [\\() -> look t] }\n    bx.h = mkh ()\n    n\nrun 1",
            "E1013",
            "assignment_extends_borrow",
            (35, 5),
        ),
        // f03_field_replace_then_use
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { p: PairOk, n: i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { p: mkpo (), n: 0 }\n    let t = ref bx.p.s\n    Mutex.with_lock (ref bx.p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    bx.p = mkpo ()\n    look t\nrun 1",
            "E1014",
            "borrow_conflict",
            (38, 5),
        ),
        // f08_nested_unsafe_order
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { p: Pair, n: i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { p: mkp (), n: 0 }\n    let t = ref bx.p.s\n    Mutex.with_lock (ref bx.p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    bx.p.s = mks ()\n    n\nrun 1",
            "E1014",
            "borrow_conflict",
            (38, 5),
        ),
    ] {
        let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
        assert_eq!(
            (error.code, error.kind, position(source, error.span.start)),
            (code, Some(kind), at),
            "{source}\n{}",
            error.message
        );
    }
    for source in [
        // rp4_field_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { p: PairOk, n: i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { p: mkpo (), n: 0 }\n    let t = ref bx.p.s\n    Mutex.with_lock (ref bx.p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    bx.p = mkpo ()\n    n\nrun 1",
        // fo1_plain_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { h: mkh (), n: 0 }\n    let s = mks ()\n    let t = ref s\n    bx.h = Holder { cell: Mutex.create [\\() -> look t] }\n    bx.h = mkh ()\n    n\nrun 1",
        // fo2_lock_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { h: mkh (), n: 0 }\n    let s = mks ()\n    let t = ref s\n    Mutex.with_lock (ref bx.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    bx.h = mkh ()\n    n\nrun 1",
        // f04_field_replace_then_move_source
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { h: mkh (), n: 0 }\n    let s = mks ()\n    let t = ref s\n    bx.h = Holder { cell: Mutex.create [\\() -> look t] }\n    bx.h = mkh ()\n    let s2 = s\n    String.length (ref s2)\nrun 1",
        // f07_nested_field_replace
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { p: PairOk, n: i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { p: mkpo (), n: 0 }\n    let t = ref bx.p.s\n    Mutex.with_lock (ref bx.p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    bx.p.h = mkh ()\n    n\nrun 1",
        // f10_field_lock_then_replace_outer_value
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { p: PairOk, n: i64 }\ndef run :: i64 -> i64\nfn run n =\n    let mut bx = Box { p: mkpo (), n: 0 }\n    let t = ref bx.p.s\n    Mutex.with_lock (ref bx.p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    bx.p = mkpo ()\n    bx.p.s = mks ()\n    let r = ref mut bx\n    r.n = 3\n    bx.n\nrun 1",
    ] {
        analyze_with_edition(source, Edition::E2027)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    }
}

#[test]
fn a_reference_variable_or_a_pipeline_leaves_a_shared_borrow_of_the_borrowed_part_only() {
    // A31, review 11, findings 5 and 6: the copy of what a callee kept of its reference argument
    // kept the parents of the reborrow (the variable's or the pipeline's borrow of the whole
    // value), which stayed live with the copy.
    // v04_var_then_target_var_again
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let r = ref mut p0\n    let a = target r\n    let b = target r\n    a + b + n\nrun 1",
        "E1014",
        "borrow_conflict",
        (67, 20),
    );
    // v07_var_then_move_p0
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef sinkpn :: own PN -> i64\nfn sinkpn p = 5\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = { let r = ref mut p0; target r }\n    sinkpn p0 + a + n\nrun 1",
        "E1014",
        "borrow_conflict",
        (68, 12),
    );
    for main in [
        // pp7_direct_var_read
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = { let r = ref mut p0; target r }\n    a + p0.n + n\nrun 1",
        // pp6_pipe_var_read
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = { let r = ref mut p0; r |> target }\n    a + p0.n + n\nrun 1",
        // pp1_pipe_then_read
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = ref mut p0 |> target\n    a + p0.n + n\nrun 1",
        // pp2_pipe_then_read_s
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = ref mut p0 |> target\n    a + look (ref p0.s) + n\nrun 1",
        // pp3_pipe_twice
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef peek :: ref PN -> i64\nfn peek p = p.n\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = ref mut p0 |> target\n    let b = ref p0 |> peek\n    a + b + n\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
    // Edition 2027 only (the other edition does not write the program).
    for (source, code, kind, at) in [
        // v01_var_then_touch_var
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef touch :: ref mut PN -> i64\nfn touch p = { p.s = mks (); 1 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let r = ref mut p0\n    let a = target r\n    let b = touch r\n    a + b + n\nrun 1",
            "E1014",
            "borrow_conflict",
            (69, 19),
        ),
        // v02_var_then_write_s_through_var
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let r = ref mut p0\n    let a = target r\n    r.s = mks ()\n    a + n\nrun 1",
            "E1014",
            "borrow_conflict",
            (67, 5),
        ),
        // v05_pipe_then_touch_var
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef touch :: ref mut PN -> i64\nfn touch p = { p.s = mks (); 1 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let r = ref mut p0\n    let a = r |> target\n    let b = touch r\n    a + b + n\nrun 1",
            "E1014",
            "borrow_conflict",
            (69, 19),
        ),
        // v08_var_then_write_s_direct
        (
            "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = { let r = ref mut p0; target r }\n    p0.s = mks ()\n    a + n\nrun 1",
            "E1014",
            "borrow_conflict",
            (66, 5),
        ),
    ] {
        let error = analyze_with_edition(source, Edition::E2027).expect_err(source);
        assert_eq!(
            (error.code, error.kind, position(source, error.span.start)),
            (code, Some(kind), at),
            "{source}\n{}",
            error.message
        );
    }
    for source in [
        // pp10_var_then_write_other
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = { let r = ref mut p0; target r }\n    p0.n = 9\n    a + n\nrun 1",
        // fa4_pipe_other_part
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = ref mut p0 |> target\n    p0.n = 9\n    a + p0.n + n\nrun 1",
        // fa9_pipe_partial_other
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target2 :: i64 -> ref mut PN -> i64\nfn target2 k p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); k }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = ref mut p0 |> target2 1\n    p0.n = 9\n    a + p0.n + n\nrun 1",
        // pp12_refvar_write_other
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = { let r = ref p0; target r }\n    p0.n = 9\n    a + n\nrun 1",
        // fa6_pipe_shared_other_part
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let a = ref p0 |> target\n    p0.n = 9\n    a + p0.n + n\nrun 1",
        // v03_var_then_write_n_through_var
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\ndef target :: ref mut PN -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpn ()\n    let r = ref mut p0\n    let a = target r\n    r.n = 4\n    a + n + p0.n\nrun 1",
    ] {
        analyze_with_edition(source, Edition::E2027)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    }
}

#[test]
fn a_borrow_through_another_owner_of_a_block_borrows_a_part_of_the_block() {
    // A31, review 11, finding 7: `inner = Rc.share (ref rc)` owns the block of `rc`, so a borrow of
    // a part of the block taken through `inner` is one of a part of `rc` itself, whose drop order
    // decides; a separate block of the same type, or a value that is a share on one path only, is
    // not.
    // rs3_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkp ())\n    let inner = Rc.share (ref rc)\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref inner)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\nrun 1",
        "E1013",
        "self_referential_store",
        (57, 5),
    );
    // rs11u_blk_p
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkp ())\n    let x = { let inner = Rc.share (ref rc); let p = Rc.get (ref inner); put (ref p.s) (ref p.h); 0 }\n    n + x\nrun 1",
        "E1013",
        "self_referential_store",
        (58, 13),
    );
    // g04_separate_block_same_type
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let inner = Rc.new (mkpo ())\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (57, 5),
    );
    // g05_share_in_branch
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let inner = if n > 0 then Rc.share (ref rc) else Rc.new (mkpo ())\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (57, 5),
    );
    // g06_share_reassigned_before
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let mut inner = Rc.share (ref rc)\n    inner = Rc.new (mkpo ())\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (57, 5),
    );
    // g01_separate_block_inner
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let x = { let other = Rc.new (mkpo ()); let t = ref (Rc.get (ref other)).s; Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\n    n + x\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (58, 13),
    );
    // rsx1_reassign_share_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkh ())\n    let mut inner = Rc.share (ref rc)\n    let s = mks ()\n    put (ref s) (Rc.get (ref inner))\n    inner = Rc.new (mkh ())\n    n\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (59, 5),
    );
    for main in [
        // rs1_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let inner = Rc.share (ref rc)\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\nrun 1",
        // rs2_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let inner = Rc.share (ref rc)\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref inner)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\nrun 1",
        // rs8_safe_arc
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord APair { h: Holder2, s: string }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Arc.new (APair { h: mkh2 (), s: mks () })\n    let inner = Arc.share (ref rc)\n    let t = ref (Arc.get (ref inner)).s\n    Mutex.with_lock (Arc.get (ref (Arc.get (ref inner)).h.cell)) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\nrun 1",
        // rs9_safe_put_direct
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let inner = Rc.share (ref rc)\n    put (ref (Rc.get (ref inner)).s) (ref (Rc.get (ref inner)).h)\n    n\nrun 1",
        // rs5_safe_inner_block
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let x = { let inner = Rc.share (ref rc); let p = Rc.get (ref inner); put (ref p.s) (ref p.h); 0 }\n    n + x\nrun 1",
        // rs12s_blk_p
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let x = { let inner = Rc.share (ref rc); let p = Rc.get (ref inner); put (ref p.s) (ref p.h); 0 }\n    n + x\nrun 1",
        // g07_share_of_share
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let mid = Rc.share (ref rc)\n    let inner = Rc.share (ref mid)\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

#[test]
fn parts_of_a_reference_handed_to_a_function_value_stay_parts() {
    // A31, review 11, finding 8: `f (ref p.h) (ref p.s)` hands the function value borrows of two
    // parts of what `p` points at; the caller keeps a borrow of the part, not of the whole value.
    // ho1_parts_tmp_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\ndef apply :: (ref Holder -> ref string -> unit) -> ref Pair -> i64\nfn apply f p = { f (ref p.h) (ref p.s); 0 }\ndef run :: i64 -> i64\nfn run n = { apply put2 (ref (mkp ())); n }\nrun 1",
        "E1013",
        "assignment_extends_borrow",
        (60, 14),
    );
    // ho3_parts_local_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\ndef apply :: (ref Holder -> ref string -> unit) -> ref Pair -> i64\nfn apply f p = { f (ref p.h) (ref p.s); 0 }\ndef run :: i64 -> i64\nfn run n = { let p0 = mkp (); apply put2 (ref p0); n }\nrun 1",
        "E1013",
        "self_referential_store",
        (60, 12),
    );
    // ho5_parts_opaque_unsafe
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\ndef apply :: (ref Holder -> ref string -> unit) -> ref Pair -> i64\nfn apply f p = { f (ref p.h) (ref p.s); 0 }\ndef pick :: i64 -> (ref Holder -> ref string -> unit)\nfn pick k = if k > 0 then put2 else \\h t -> ()\ndef run :: i64 -> i64\nfn run n = { let p0 = mkp (); apply (pick n) (ref p0); n }\nrun 1",
        "E1013",
        "self_referential_store",
        (62, 12),
    );
    for main in [
        // ho2_parts_tmp_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\ndef apply :: (ref Holder -> ref string -> unit) -> ref PairOk -> i64\nfn apply f p = { f (ref p.h) (ref p.s); 0 }\ndef run :: i64 -> i64\nfn run n = { apply put2 (ref (mkpo ())); n }\nrun 1",
        // ho4_parts_local_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\ndef apply :: (ref Holder -> ref string -> unit) -> ref PairOk -> i64\nfn apply f p = { f (ref p.h) (ref p.s); 0 }\ndef run :: i64 -> i64\nfn run n = { let p0 = mkpo (); apply put2 (ref p0); n }\nrun 1",
        // ho7_lambda_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\ndef apply :: (ref Holder -> ref string -> unit) -> ref PairOk -> i64\nfn apply f p = { f (ref p.h) (ref p.s); 0 }\ndef run :: i64 -> i64\nfn run n = { let p0 = mkpo (); apply (\\h t -> put2 h t) (ref p0); n }\nrun 1",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

#[test]
fn a_reference_to_one_of_several_values_keeps_a_borrow_of_its_own_part_in_each() {
    // A31, review 12, finding 1: a callee that keeps a borrow of a part of what its reference
    // parameter points at in that same value, given a reference that may point at one of several
    // values (a branch, an element of an array of references, the result of a function), keeps
    // it in the one value that it gets: each value keeps a borrow of its own part only, so the
    // order of the values does not matter. The parts of each value still have to be in their
    // drop order, and a store into another argument still crosses the values.
    // d1u1_if_ref_bad_order: the string is dropped before the holder
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let a = mkp ()\n    let b = mkp ()\n    let r = if n > 0 then ref a else ref b\n    target r\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "self_referential_store",
        (36, 5),
    );
    // d1u7_if_refmut_bad_order
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref mut Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut a = mkp ()\n    let mut b = mkp ()\n    let r = if n > 0 then ref mut a else ref mut b\n    target r\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "self_referential_store",
        (36, 5),
    );
    // d1u12_passon_if_bad: a callee chooses the reference
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef via :: ref Pair -> ref Pair -> bool -> i64\nfn via x y c = target (if c then x else y)\ndef run :: i64 -> i64\nfn run n =\n    let a = mkp ()\n    let b = mkp ()\n    via (ref a) (ref b) (n > 0)\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "self_referential_store",
        (38, 5),
    );
    // d1u9_fnvalue_if_ref_bad: through a function value
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref Pair -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef other :: ref Pair -> i64\nfn other p = 0\ndef run :: i64 -> i64\nfn run n =\n    let a = mkp ()\n    let b = mkp ()\n    let f = if n > 0 then target else other\n    let r = if n > 0 then ref a else ref b\n    f r\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "self_referential_store",
        (38, 5),
    );
    // d1u3_two_if_refs_cross: a store into another argument
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target2 :: ref PairOk -> ref PairOk -> i64\nfn target2 p q = { let t = ref p.s; Mutex.with_lock (ref q.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let a = mkpo ()\n    let b = mkpo ()\n    let r1 = if n > 0 then ref b else ref a\n    let r2 = if n > 0 then ref a else ref b\n    target2 r1 r2\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "assignment_extends_borrow",
        (36, 5),
    );
    // d1u6_if_ref_self_and_out: and into a holder declared first
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target3 :: ref PairOk -> ref Holder -> i64\nfn target3 p out =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    Mutex.with_lock (ref out.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    0\ndef run :: i64 -> i64\nfn run n =\n    let out = mkh ()\n    let a = mkpo ()\n    let b = mkpo ()\n    let r = if n > 0 then ref a else ref b\n    target3 r (ref out)\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "assignment_extends_borrow",
        (40, 5),
    );
    for main in [
        // h01_if_refmut_target_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref mut PairOk -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let mut a = mkpo ()\n    let mut b = mkpo ()\n    let r = if n > 0 then ref mut a else ref mut b\n    target r\ndo! IO.write_line (to_string (run 1))",
        // h02_if_ref_target_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref PairOk -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let a = mkpo ()\n    let b = mkpo ()\n    let r = if n > 0 then ref a else ref b\n    target r\ndo! IO.write_line (to_string (run 1))",
        // h03_if_inline_target_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref PairOk -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let a = mkpo ()\n    let b = mkpo ()\n    target (if n > 0 then ref a else ref b)\ndo! IO.write_line (to_string (run 1))",
        // h05_iter_refs_target_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref PairOk -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let a = mkpo ()\n    let b = mkpo ()\n    let rs = [ref a, ref b]\n    target rs[0] + target rs[1]\ndo! IO.write_line (to_string (run 1))",
        // h06_pick_fn_target_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref PairOk -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef pick :: ref PairOk -> ref PairOk -> bool -> ref PairOk\nfn pick x y c = if c then x else y\ndef run :: i64 -> i64\nfn run n =\n    let a = mkpo ()\n    let b = mkpo ()\n    target (pick (ref a) (ref b) (n > 0))\ndo! IO.write_line (to_string (run 1))",
        // d1u11_passon_if
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref PairOk -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef via :: ref PairOk -> ref PairOk -> bool -> i64\nfn via x y c = target (if c then x else y)\ndef run :: i64 -> i64\nfn run n =\n    let a = mkpo ()\n    let b = mkpo ()\n    via (ref a) (ref b) (n > 0)\ndo! IO.write_line (to_string (run 1))",
        // d1u9b_fnvalue_if_ref_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref PairOk -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef other :: ref PairOk -> i64\nfn other p = 0\ndef run :: i64 -> i64\nfn run n =\n    let a = mkpo ()\n    let b = mkpo ()\n    let f = if n > 0 then target else other\n    let r = if n > 0 then ref a else ref b\n    f r\ndo! IO.write_line (to_string (run 1))",
        // d1u4_if_ref_inner_scope_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef target :: ref PairOk -> i64\nfn target p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let a = mkpo ()\n    let k = { let b = mkpo (); let r = if n > 0 then ref a else ref b; target r }\n    k + 1\ndo! IO.write_line (to_string (run 1))",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

#[test]
fn a_share_written_through_an_exclusive_borrow_no_longer_shares_the_block() {
    // A31, review 12, finding 2: `let mut inner = Rc.share (ref rc)` is an owner of rc's block
    // only until code may write it: after an exclusive borrow of it (`renew (ref mut inner)`,
    // `let r = ref mut inner`) it may own another block, and a borrow of a part of that block
    // kept in rc's cell is one of another value, dropped before rc.
    // d02_stale_share_renew
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef renew :: ref mut Rc<PairOk> -> i64\nfn renew r = { deref r = Rc.new (mkpo ()); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let mut inner = Rc.share (ref rc)\n    let k = renew (ref mut inner)\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n + k\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "assignment_extends_borrow",
        (36, 5),
    );
    // d06_stale_share_deref_nl: through a ref mut variable
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let mut inner = Rc.share (ref rc)\n    let r = ref mut inner\n    deref r = Rc.new (mkpo ())\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "assignment_extends_borrow",
        (34, 5),
    );
    // d08_stale_share_arc: Arc
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef renew :: ref mut Arc<PairOk> -> i64\nfn renew r = { deref r = Arc.new (mkpo ()); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Arc.new (mkpo ())\n    let mut inner = Arc.share (ref rc)\n    let k = renew (ref mut inner)\n    let t = ref (Arc.get (ref inner)).s\n    Mutex.with_lock (ref (Arc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n + k\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "assignment_extends_borrow",
        (36, 5),
    );
    // d09_stale_share_swap_param: the write by a helper
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef renew :: ref mut Rc<PairOk> -> i64\nfn renew r = { deref r = Rc.new (mkpo ()); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let mut inner = Rc.share (ref rc)\n    let k = renew (ref mut inner)\n    put (ref (Rc.get (ref inner)).s) (ref (Rc.get (ref rc)).h)\n    n + k\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "assignment_extends_borrow",
        (38, 5),
    );
    // d10_stale_share_inner_block: in an inner block
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef renew :: ref mut Rc<PairOk> -> i64\nfn renew r = { deref r = Rc.new (mkpo ()); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let k = {\n        let mut inner = Rc.share (ref rc);\n        let z = renew (ref mut inner);\n        let t = ref (Rc.get (ref inner)).s;\n        Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b);\n        1 + z\n    }\n    let junk = [mks (), mks ()]\n    n + k + junk.length\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "assignment_extends_borrow",
        (37, 13),
    );
    // d2u1_share_refmut_in_branch: renewed on one branch
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef renew :: ref mut Rc<PairOk> -> i64\nfn renew r = { deref r = Rc.new (mkpo ()); 0 }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let mut inner = Rc.share (ref rc)\n    let k = if n > 0 then renew (ref mut inner) else 0\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n + k\ndo! IO.write_line (to_string (run 1))",
        "E1013",
        "assignment_extends_borrow",
        (36, 5),
    );
    for main in [
        // d03_share_safe_ctl
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let inner = Rc.share (ref rc)\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\ndo! IO.write_line (to_string (run 1))",
        // d11_share_inner_block_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let k = {\n        let inner = Rc.share (ref rc);\n        let t = ref (Rc.get (ref inner)).s;\n        Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b);\n        1\n    }\n    let junk = [mks (), mks ()]\n    n + k + junk.length\ndo! IO.write_line (to_string (run 1))",
        // d2u3_share_of_share_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let mid = Rc.share (ref rc)\n    let inner = Rc.share (ref mid)\n    let t = ref (Rc.get (ref inner)).s\n    Mutex.with_lock (ref (Rc.get (ref rc)).h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    n\ndo! IO.write_line (to_string (run 1))",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

#[test]
fn replacing_the_target_of_a_reference_ends_no_borrow_that_a_copy_or_a_shared_block_keeps() {
    // A31, review 12, findings 3 and 5: `deref p = v` ends the borrows of the old value's own
    // parts only when every copy of them goes with it. The body copied what a cell of the target
    // holds to another parameter's target or to a hidden environment; the cell is in the block of
    // an `Rc` or `Arc` that another owner of the caller's may share; or the old value's `Drop.drop`
    // copies what its cell holds where it reaches: then the borrows stay, and the write conflicts.
    // f14_deref_copy_out_helper: a helper copies the cell out
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord PA { h: Holder, a: [string] }\ndef mkpa :: unit -> PA\nfn mkpa _u = PA { h: mkh (), a: [mks ()] }\ndef copy_out :: ref [unit -> i64] -> ref Mutex<[unit -> i64]> -> unit\nfn copy_out b out = Mutex.with_lock out (\\o -> Array.write 0 b[0] o)\ndef copy_h :: ref Holder -> ref Holder -> unit\nfn copy_h src out = Mutex.with_lock (ref src.cell) (\\b -> copy_out b (ref out.cell))\ndef renew :: ref mut PA -> ref Holder -> i64\nfn renew p out =\n    let t = ref p.a[0]\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    copy_h (ref p.h) out\n    deref p = mkpa ()\n    0\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpa ()\n    let out = mkh ()\n    let k = renew (ref mut p0) (ref out)\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    Mutex.with_lock (ref out.cell) (\\b -> b[0] ()) + k + junk.length * 0\ndo! IO.write_line (to_string (run 1))",
        "E1014",
        "borrow_conflict",
        (44, 5),
    );
    // f09_deref_after_nested_copy_elem: the copy written inline
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord PA { h: Holder, a: [string] }\ndef mkpa :: unit -> PA\nfn mkpa _u = PA { h: mkh (), a: [mks ()] }\ndef copy_out :: ref [unit -> i64] -> ref Mutex<[unit -> i64]> -> unit\nfn copy_out b out = Mutex.with_lock out (\\o -> Array.write 0 b[0] o)\ndef renew :: ref mut PA -> ref Holder -> i64\nfn renew p out =\n    let t = ref p.a[0]\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    Mutex.with_lock (ref p.h.cell) (\\b -> copy_out b (ref out.cell))\n    deref p = mkpa ()\n    0\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpa ()\n    let out = mkh ()\n    let k = renew (ref mut p0) (ref out)\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    Mutex.with_lock (ref out.cell) (\\b -> b[0] ()) + k + junk.length * 0\ndo! IO.write_line (to_string (run 1))",
        "E1014",
        "borrow_conflict",
        (42, 5),
    );
    // f5u3_hidden_env_copy: the copy goes to a hidden environment
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord PA { h: Holder, a: [string] }\ndef mkpa :: unit -> PA\nfn mkpa _u = PA { h: mkh (), a: [mks ()] }\ndef renew :: ref mut PA -> Arc<Mutex<[unit -> i64]>> -> i64\nfn renew p sink =\n    let t = ref p.a[0]\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    let w = \\g -> Mutex.with_lock (Arc.get (ref sink)) (\\o -> Array.write 0 g o)\n    Mutex.with_lock (ref p.h.cell) (\\b -> w b[0])\n    deref p = mkpa ()\n    0\ndef run :: i64 -> i64\nfn run n =\n    let keep = Arc.new (Mutex.create [\\() -> 0])\n    let mut p0 = mkpa ()\n    let k = renew (ref mut p0) (Arc.share (ref keep))\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    Mutex.with_lock (Arc.get (ref keep)) (\\b -> b[0] ()) + k + junk.length * 0\ndo! IO.write_line (to_string (run 1))",
        "E1014",
        "borrow_conflict",
        (41, 5),
    );
    // f1: the cell is in a shared Arc block
    rejects_at(
        "record Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())))\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord PR { c: Holder2, s: string }\ndef renew :: ref mut PR -> i64\nfn renew p =\n    let t = ref p.s\n    Mutex.with_lock (Arc.get (ref p.c.cell)) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = PR { c: Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }, s: String.repeat 4 (ref \"abcdefgh\") }\n    0\ndef run :: i64 -> i64\nfn run n =\n    let keep = Arc.new (Mutex.create [\\() -> 0])\n    let mut p0 = PR { c: Holder2 { cell: Arc.share (ref keep) }, s: String.repeat 4 (ref \"123456789\") }\n    let k = renew (ref mut p0)\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    Mutex.with_lock (Arc.get (ref keep)) (\\b -> b[0] ()) + k + junk.length * 0\nrun 1",
        "E1014",
        "borrow_conflict",
        (15, 5),
    );
    // t05_deref_shared_block_junk
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\nrecord B2 { h: Holder2, n: i64 }\ndef mkb2 :: unit -> B2\nfn mkb2 _u = B2 { h: mkh2 (), n: 0 }\ndef putc :: ref string -> ref Arc<Mutex<[unit -> i64]>> -> unit\nfn putc t c = Mutex.with_lock (Arc.get c) (\\b -> Array.write 0 (\\() -> look t) b)\ndef readc :: ref Arc<Mutex<[unit -> i64]>> -> i64\nfn readc c = Mutex.with_lock (Arc.get c) (\\b -> b[0] ())\nrecord PR { c: Holder2, s: string }\ndef mkpr :: unit -> PR\nfn mkpr _u = PR { c: mkh2 (), s: mks () }\ndef target :: ref mut PR -> i64\nfn target p =\n    let t = ref p.s\n    putc t (ref p.c.cell)\n    deref p = PR { c: mkh2 (), s: String.repeat 4 (ref \"abcdefgh\") }\n    0\ndef run :: i64 -> i64\nfn run n =\n    let keep = Arc.new (Mutex.create [\\() -> 0])\n    let mut p0 = PR { c: Holder2 { cell: Arc.share (ref keep) }, s: mks () }\n    let k = target (ref mut p0)\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    do! IO.write_line (\"read \" + to_string (readc (ref keep)) + \" \" + to_string junk.length)\n    n + k\nrun 1",
        "E1014",
        "borrow_conflict",
        (74, 5),
    );
    // t08_deref_shared_block_local: through a ref mut variable
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\nrecord B2 { h: Holder2, n: i64 }\ndef mkb2 :: unit -> B2\nfn mkb2 _u = B2 { h: mkh2 (), n: 0 }\ndef putc :: ref string -> ref Arc<Mutex<[unit -> i64]>> -> unit\nfn putc t c = Mutex.with_lock (Arc.get c) (\\b -> Array.write 0 (\\() -> look t) b)\ndef readc :: ref Arc<Mutex<[unit -> i64]>> -> i64\nfn readc c = Mutex.with_lock (Arc.get c) (\\b -> b[0] ())\nrecord PR { c: Holder2, s: string }\ndef mkpr :: unit -> PR\nfn mkpr _u = PR { c: mkh2 (), s: mks () }\ndef target :: ref mut PR -> i64\nfn target p =\n    let t = ref p.s\n    putc t (ref p.c.cell)\n    deref p = PR { c: mkh2 (), s: String.repeat 4 (ref \"abcdefgh\") }\n    0\ndef run :: i64 -> i64\nfn run n =\n    let keep = Arc.new (Mutex.create [\\() -> 0])\n    let mut p0 = PR { c: Holder2 { cell: Arc.share (ref keep) }, s: mks () }\n    let r = ref mut p0\n    let k = target r\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    do! IO.write_line (\"read \" + to_string (readc (ref keep)) + \" \" + to_string junk.length)\n    n + k\nrun 1",
        "E1014",
        "borrow_conflict",
        (74, 5),
    );
    // t17_deref_after_callee_copy_shared_block: a callee keeps it
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\nrecord B2 { h: Holder2, n: i64 }\ndef mkb2 :: unit -> B2\nfn mkb2 _u = B2 { h: mkh2 (), n: 0 }\ndef putc :: ref string -> ref Arc<Mutex<[unit -> i64]>> -> unit\nfn putc t c = Mutex.with_lock (Arc.get c) (\\b -> Array.write 0 (\\() -> look t) b)\ndef readc :: ref Arc<Mutex<[unit -> i64]>> -> i64\nfn readc c = Mutex.with_lock (Arc.get c) (\\b -> b[0] ())\nrecord PR { c: Holder2, s: string }\ndef mkpr :: unit -> PR\nfn mkpr _u = PR { c: mkh2 (), s: mks () }\ndef keepself :: ref mut PR -> i64\nfn keepself p =\n    let t = ref p.s\n    putc t (ref p.c.cell)\n    0\ndef junkread :: ref Arc<Mutex<[unit -> i64]>> -> unit\nfn junkread c =\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    do! IO.write_line (\"read \" + to_string (readc c) + \" \" + to_string junk.length)\ndef cross :: ref PairOk -> ref PairOk -> unit\nfn cross a b = put (ref b.s) (ref a.h)\ndef selfbad :: ref Pair -> unit\nfn selfbad p = put (ref p.s) (ref p.h)\ndef nothingp :: ref Pair -> unit\nfn nothingp p = ()\ndef pickit :: i64 -> (ref [Pair] -> unit)\nfn pickit k = if k > 0 then Array.iterr selfbad else Array.iterr nothingp\ndef pickself :: i64 -> (ref Pair -> unit)\nfn pickself k = if k > 0 then selfbad else nothingp\ndef readh :: ref Holder -> i64\nfn readh h = Mutex.with_lock (ref h.cell) (\\b -> b[0] ())\ndef read_cell :: ref Mutex<[unit -> i64]> -> (unit -> i64)\nfn read_cell m = \\() -> Mutex.with_lock m (\\b -> b[0] ())\nrecord Watcher {r} { target: ref {r} Mutex<[unit -> i64]> }\ninstance Drop<Watcher> {\n    fn drop w = do! IO.write_line (\"dropw \" + to_string (Mutex.with_lock w.target (\\b -> b[0] ())))\n}\ndef target :: ref mut PR -> i64\nfn target p =\n    let k = keepself p\n    deref p = PR { c: mkh2 (), s: String.repeat 4 (ref \"abcdefgh\") }\n    k\ndef run :: i64 -> i64\nfn run n =\n    let keep = Arc.new (Mutex.create [\\() -> 0])\n    let mut p0 = PR { c: Holder2 { cell: Arc.share (ref keep) }, s: mks () }\n    let k = target (ref mut p0)\n    junkread (ref keep)\n    n + k\nrun 1",
        "E1014",
        "borrow_conflict",
        (100, 5),
    );
    // t18_deref_lock_inline_shared_block
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\nrecord B2 { h: Holder2, n: i64 }\ndef mkb2 :: unit -> B2\nfn mkb2 _u = B2 { h: mkh2 (), n: 0 }\ndef putc :: ref string -> ref Arc<Mutex<[unit -> i64]>> -> unit\nfn putc t c = Mutex.with_lock (Arc.get c) (\\b -> Array.write 0 (\\() -> look t) b)\ndef readc :: ref Arc<Mutex<[unit -> i64]>> -> i64\nfn readc c = Mutex.with_lock (Arc.get c) (\\b -> b[0] ())\nrecord PR { c: Holder2, s: string }\ndef mkpr :: unit -> PR\nfn mkpr _u = PR { c: mkh2 (), s: mks () }\ndef keepself :: ref mut PR -> i64\nfn keepself p =\n    let t = ref p.s\n    putc t (ref p.c.cell)\n    0\ndef junkread :: ref Arc<Mutex<[unit -> i64]>> -> unit\nfn junkread c =\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    do! IO.write_line (\"read \" + to_string (readc c) + \" \" + to_string junk.length)\ndef cross :: ref PairOk -> ref PairOk -> unit\nfn cross a b = put (ref b.s) (ref a.h)\ndef selfbad :: ref Pair -> unit\nfn selfbad p = put (ref p.s) (ref p.h)\ndef nothingp :: ref Pair -> unit\nfn nothingp p = ()\ndef pickit :: i64 -> (ref [Pair] -> unit)\nfn pickit k = if k > 0 then Array.iterr selfbad else Array.iterr nothingp\ndef pickself :: i64 -> (ref Pair -> unit)\nfn pickself k = if k > 0 then selfbad else nothingp\ndef readh :: ref Holder -> i64\nfn readh h = Mutex.with_lock (ref h.cell) (\\b -> b[0] ())\ndef read_cell :: ref Mutex<[unit -> i64]> -> (unit -> i64)\nfn read_cell m = \\() -> Mutex.with_lock m (\\b -> b[0] ())\nrecord Watcher {r} { target: ref {r} Mutex<[unit -> i64]> }\ninstance Drop<Watcher> {\n    fn drop w = do! IO.write_line (\"dropw \" + to_string (Mutex.with_lock w.target (\\b -> b[0] ())))\n}\ndef target :: ref mut PR -> i64\nfn target p =\n    let t = ref p.s\n    Mutex.with_lock (Arc.get (ref p.c.cell)) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = PR { c: mkh2 (), s: String.repeat 4 (ref \"abcdefgh\") }\n    0\ndef run :: i64 -> i64\nfn run n =\n    let keep = Arc.new (Mutex.create [\\() -> 0])\n    let mut p0 = PR { c: Holder2 { cell: Arc.share (ref keep) }, s: mks () }\n    let k = target (ref mut p0)\n    junkread (ref keep)\n    n + k\nrun 1",
        "E1014",
        "borrow_conflict",
        (101, 5),
    );
    // f1b: the old value's drop copies its cell into an Arc
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]>, out: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let out = Arc.share (ref h.out);\n        Mutex.with_lock (ref h.cell) (\\b -> copy_out b (Arc.get (ref out)))\n    }\n}\ndef copy_out :: ref [unit -> i64] -> ref Mutex<[unit -> i64]> -> unit\nfn copy_out b out = Mutex.with_lock out (\\o -> Array.write 0 b[0] o)\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\nrecord P { h: Holder, s: string }\ndef renew :: ref mut P -> i64\nfn renew p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = P { h: Holder { cell: Mutex.create [\\() -> 0], out: Arc.new (Mutex.create [\\() -> 0]) }, s: String.repeat 3 (ref \"x9\") }\n    0\ndef run :: i64 -> i64\nfn run n =\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let mut q = P { h: Holder { cell: Mutex.create [\\() -> 0], out: Arc.share (ref a) }, s: String.repeat 4 (ref \"123456789\") }\n    let k = renew (ref mut q)\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    Mutex.with_lock (Arc.get (ref a)) (\\o -> o[0] ()) + k + junk.length * 0\nrun 1",
        "E1014",
        "borrow_conflict",
        (20, 5),
    );
    // d01_deref_old_drop_exports
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\nrecord B2 { h: Holder2, n: i64 }\ndef mkb2 :: unit -> B2\nfn mkb2 _u = B2 { h: mkh2 (), n: 0 }\ndef putc :: ref string -> ref Arc<Mutex<[unit -> i64]>> -> unit\nfn putc t c = Mutex.with_lock (Arc.get c) (\\b -> Array.write 0 (\\() -> look t) b)\ndef readc :: ref Arc<Mutex<[unit -> i64]>> -> i64\nfn readc c = Mutex.with_lock (Arc.get c) (\\b -> b[0] ())\nrecord PR { c: Holder2, s: string }\ndef mkpr :: unit -> PR\nfn mkpr _u = PR { c: mkh2 (), s: mks () }\ndef keepself :: ref mut PR -> i64\nfn keepself p =\n    let t = ref p.s\n    putc t (ref p.c.cell)\n    0\ndef junkread :: ref Arc<Mutex<[unit -> i64]>> -> unit\nfn junkread c =\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    do! IO.write_line (\"read \" + to_string (readc c) + \" \" + to_string junk.length)\ndef cross :: ref PairOk -> ref PairOk -> unit\nfn cross a b = put (ref b.s) (ref a.h)\ndef selfbad :: ref Pair -> unit\nfn selfbad p = put (ref p.s) (ref p.h)\ndef nothingp :: ref Pair -> unit\nfn nothingp p = ()\ndef pickit :: i64 -> (ref [Pair] -> unit)\nfn pickit k = if k > 0 then Array.iterr selfbad else Array.iterr nothingp\ndef pickself :: i64 -> (ref Pair -> unit)\nfn pickself k = if k > 0 then selfbad else nothingp\ndef readh :: ref Holder -> i64\nfn readh h = Mutex.with_lock (ref h.cell) (\\b -> b[0] ())\ndef read_cell :: ref Mutex<[unit -> i64]> -> (unit -> i64)\nfn read_cell m = \\() -> Mutex.with_lock m (\\b -> b[0] ())\nrecord Watcher {r} { target: ref {r} Mutex<[unit -> i64]> }\ninstance Drop<Watcher> {\n    fn drop w = do! IO.write_line (\"dropw \" + to_string (Mutex.with_lock w.target (\\b -> b[0] ())))\n}\ndef readh2 :: ref PairOk -> i64\nfn readh2 p = Mutex.with_lock (ref p.h.cell) (\\b -> b[0] ())\ndef putH :: ref Holder -> ref string -> unit\nfn putH h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Holder3 { cell: Mutex<[unit -> i64]>, out: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder3> {\n    fn drop h = {\n        let out = Arc.share (ref h.out);\n        Mutex.with_lock (ref h.cell) (\\b -> copy_out b (Arc.get (ref out)))\n    }\n}\ndef copy_out :: ref [unit -> i64] -> ref Mutex<[unit -> i64]> -> unit\nfn copy_out b out = Mutex.with_lock out (\\o -> Array.write 0 b[0] o)\nrecord Pair3 { h: Holder3, s: string }\ndef renew :: ref mut Pair3 -> i64\nfn renew p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = Pair3 { h: Holder3 { cell: Mutex.create [\\() -> 0], out: Arc.new (Mutex.create [\\() -> 0]) }, s: String.repeat 4 (ref \"abcdefgh\") }\n    0\ndef run :: i64 -> i64\nfn run n =\n    let keep = Arc.new (Mutex.create [\\() -> 0])\n    let mut p0 = Pair3 { h: Holder3 { cell: Mutex.create [\\() -> 0], out: Arc.share (ref keep) }, s: mks () }\n    let k = renew (ref mut p0)\n    junkread (ref keep)\n    n + k\nrun 1",
        "E1014",
        "borrow_conflict",
        (115, 5),
    );
    for main in [
        // f10_deref_after_copy_elem_noout
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord PA { h: Holder, a: [string] }\ndef mkpa :: unit -> PA\nfn mkpa _u = PA { h: mkh (), a: [mks ()] }\ndef renew :: ref mut PA -> i64\nfn renew p =\n    let t = ref p.a[0]\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = mkpa ()\n    0\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpa ()\n    let k = renew (ref mut p0)\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    k + junk.length * 0\ndo! IO.write_line (to_string (run 1))",
        // f3u2_copy_out_after_deref_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord PA { h: Holder, a: [string] }\ndef mkpa :: unit -> PA\nfn mkpa _u = PA { h: mkh (), a: [mks ()] }\ndef copy_out :: ref [unit -> i64] -> ref Mutex<[unit -> i64]> -> unit\nfn copy_out b out = Mutex.with_lock out (\\o -> Array.write 0 b[0] o)\ndef copy_h :: ref Holder -> ref Holder -> unit\nfn copy_h src out = Mutex.with_lock (ref src.cell) (\\b -> copy_out b (ref out.cell))\ndef renew :: ref mut PA -> ref Holder -> i64\nfn renew p out =\n    let t = ref p.a[0]\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = mkpa ()\n    copy_h (ref p.h) out\n    0\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = mkpa ()\n    let out = mkh ()\n    let k = renew (ref mut p0) (ref out)\n    Mutex.with_lock (ref out.cell) (\\b -> b[0] ()) + k\ndo! IO.write_line (to_string (run 1))",
        // f5u2_arc_in_target_not_used_safe
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord PR { h: Holder, c: Holder2, s: string }\ndef renew :: ref mut PR -> i64\nfn renew p =\n    let t = ref p.s\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n    deref p = PR { h: mkh (), c: mkh2 (), s: mks () }\n    0\ndef run :: i64 -> i64\nfn run n =\n    let mut p0 = PR { h: mkh (), c: mkh2 (), s: mks () }\n    renew (ref mut p0)\ndo! IO.write_line (to_string (run 1))",
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

#[test]
fn replacing_an_rc_ends_the_borrows_that_its_block_keeps_through_a_share() {
    // A31, review 12, finding 6: a borrow of a part of an `Rc` block taken through a share of the
    // block is one of a part of the old value when the `Rc` is replaced: after the share is gone,
    // the assignment drops the block with it. While the share lives, the borrow of its block
    // conflicts with the assignment and with a move of the `Rc`, as a live `Rc.get` does (the
    // conservative limit that drop.md states).
    // r06_rs_good_same_scope_reassign: the share is still in scope
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\nrecord B2 { h: Holder2, n: i64 }\ndef mkb2 :: unit -> B2\nfn mkb2 _u = B2 { h: mkh2 (), n: 0 }\ndef putc :: ref string -> ref Arc<Mutex<[unit -> i64]>> -> unit\nfn putc t c = Mutex.with_lock (Arc.get c) (\\b -> Array.write 0 (\\() -> look t) b)\ndef readc :: ref Arc<Mutex<[unit -> i64]>> -> i64\nfn readc c = Mutex.with_lock (Arc.get c) (\\b -> b[0] ())\nrecord PR { c: Holder2, s: string }\ndef mkpr :: unit -> PR\nfn mkpr _u = PR { c: mkh2 (), s: mks () }\ndef keepself :: ref mut PR -> i64\nfn keepself p =\n    let t = ref p.s\n    putc t (ref p.c.cell)\n    0\ndef junkread :: ref Arc<Mutex<[unit -> i64]>> -> unit\nfn junkread c =\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    do! IO.write_line (\"read \" + to_string (readc c) + \" \" + to_string junk.length)\ndef cross :: ref PairOk -> ref PairOk -> unit\nfn cross a b = put (ref b.s) (ref a.h)\ndef selfbad :: ref Pair -> unit\nfn selfbad p = put (ref p.s) (ref p.h)\ndef nothingp :: ref Pair -> unit\nfn nothingp p = ()\ndef pickit :: i64 -> (ref [Pair] -> unit)\nfn pickit k = if k > 0 then Array.iterr selfbad else Array.iterr nothingp\ndef pickself :: i64 -> (ref Pair -> unit)\nfn pickself k = if k > 0 then selfbad else nothingp\ndef readh :: ref Holder -> i64\nfn readh h = Mutex.with_lock (ref h.cell) (\\b -> b[0] ())\ndef read_cell :: ref Mutex<[unit -> i64]> -> (unit -> i64)\nfn read_cell m = \\() -> Mutex.with_lock m (\\b -> b[0] ())\nrecord Watcher {r} { target: ref {r} Mutex<[unit -> i64]> }\ninstance Drop<Watcher> {\n    fn drop w = do! IO.write_line (\"dropw \" + to_string (Mutex.with_lock w.target (\\b -> b[0] ())))\n}\ndef readh2 :: ref PairOk -> i64\nfn readh2 p = Mutex.with_lock (ref p.h.cell) (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let mut rc = Rc.new (mkpo ())\n    let inner = Rc.share (ref rc)\n    let p = Rc.get (ref inner)\n    put (ref p.s) (ref p.h)\n    rc = Rc.new (mkpo ())\n    n\nrun 1",
        "E1014",
        "borrow_conflict",
        (105, 5),
    );
    // r07_rs_good_inner_block_move: a move
    rejects_at(
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\nrecord B2 { h: Holder2, n: i64 }\ndef mkb2 :: unit -> B2\nfn mkb2 _u = B2 { h: mkh2 (), n: 0 }\ndef putc :: ref string -> ref Arc<Mutex<[unit -> i64]>> -> unit\nfn putc t c = Mutex.with_lock (Arc.get c) (\\b -> Array.write 0 (\\() -> look t) b)\ndef readc :: ref Arc<Mutex<[unit -> i64]>> -> i64\nfn readc c = Mutex.with_lock (Arc.get c) (\\b -> b[0] ())\nrecord PR { c: Holder2, s: string }\ndef mkpr :: unit -> PR\nfn mkpr _u = PR { c: mkh2 (), s: mks () }\ndef keepself :: ref mut PR -> i64\nfn keepself p =\n    let t = ref p.s\n    putc t (ref p.c.cell)\n    0\ndef junkread :: ref Arc<Mutex<[unit -> i64]>> -> unit\nfn junkread c =\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    do! IO.write_line (\"read \" + to_string (readc c) + \" \" + to_string junk.length)\ndef cross :: ref PairOk -> ref PairOk -> unit\nfn cross a b = put (ref b.s) (ref a.h)\ndef selfbad :: ref Pair -> unit\nfn selfbad p = put (ref p.s) (ref p.h)\ndef nothingp :: ref Pair -> unit\nfn nothingp p = ()\ndef pickit :: i64 -> (ref [Pair] -> unit)\nfn pickit k = if k > 0 then Array.iterr selfbad else Array.iterr nothingp\ndef pickself :: i64 -> (ref Pair -> unit)\nfn pickself k = if k > 0 then selfbad else nothingp\ndef readh :: ref Holder -> i64\nfn readh h = Mutex.with_lock (ref h.cell) (\\b -> b[0] ())\ndef read_cell :: ref Mutex<[unit -> i64]> -> (unit -> i64)\nfn read_cell m = \\() -> Mutex.with_lock m (\\b -> b[0] ())\nrecord Watcher {r} { target: ref {r} Mutex<[unit -> i64]> }\ninstance Drop<Watcher> {\n    fn drop w = do! IO.write_line (\"dropw \" + to_string (Mutex.with_lock w.target (\\b -> b[0] ())))\n}\ndef readh2 :: ref PairOk -> i64\nfn readh2 p = Mutex.with_lock (ref p.h.cell) (\\b -> b[0] ())\ndef run :: i64 -> i64\nfn run n =\n    let rc = Rc.new (mkpo ())\n    let k = { let inner = Rc.share (ref rc); let p = Rc.get (ref inner); put (ref p.s) (ref p.h); 0 }\n    let moved = rc\n    n + k\nrun 1",
        "E1014",
        "borrow_conflict",
        (103, 17),
    );
    // r03_rs_good_order_inner_block_reassign
    accepts_in_both_editions(&[(
        "Main.tz",
        "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mkh :: unit -> Holder\nfn mkh _u = Holder { cell: Mutex.create [\\() -> 0] }\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Holder2 { cell: Arc<Mutex<[unit -> i64]>> }\ninstance Drop<Holder2> {\n    fn drop h = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"drop2 \" + to_string (Mutex.with_lock (Arc.get (ref h.cell)) (\\b -> b[0] ())) + \" \" + to_string junk.length)\n    }\n}\ndef mkh2 :: unit -> Holder2\nfn mkh2 _u = Holder2 { cell: Arc.new (Mutex.create [\\() -> 0]) }\nrecord Pair { s: string, h: Holder }\nrecord PairOk { h: Holder, s: string }\ndef mkp :: unit -> Pair\nfn mkp _u = Pair { s: mks (), h: mkh () }\ndef mkpo :: unit -> PairOk\nfn mkpo _u = PairOk { h: mkh (), s: mks () }\nrecord Box { h: Holder, n: i64 }\nrecord Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = {\n        let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")];\n        do! IO.write_line (\"dropc \" + to_string (c.f ()) + \" \" + to_string junk.length)\n    }\n}\ndef mkreg :: unit -> ((unit -> i64) -> unit)\nfn mkreg _u =\n    let cell = Arc.new (Mutex.create [Caller { f: \\() -> 0 }])\n    \\g -> Mutex.with_lock (Arc.get (ref cell)) (\\slot -> Array.write 0 (Caller { f: g }) slot)\ndef put :: ref string -> ref Holder -> unit\nfn put t h = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\nrecord Reg { f: (unit -> i64) -> unit }\ndef mkr :: unit -> Reg\nfn mkr _u = Reg { f: mkreg () }\ndef put2 :: ref Holder -> ref string -> unit\nfn put2 h t = Mutex.with_lock (ref h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\ndef consume :: own Holder -> i64\nfn consume h = 0\ndef mkpo2 :: ref string -> PairOk\nfn mkpo2 t = PairOk { h: Holder { cell: Mutex.create [\\() -> look t] }, s: mks () }\nrecord HB { h: Holder }\nrecord Outer4 { r: Rc<HB>, s: string }\nrecord PN { h: Holder, s: string, n: i64 }\ndef mkpn :: unit -> PN\nfn mkpn _u = PN { h: mkh (), s: mks (), n: 0 }\nrecord B2 { h: Holder2, n: i64 }\ndef mkb2 :: unit -> B2\nfn mkb2 _u = B2 { h: mkh2 (), n: 0 }\ndef putc :: ref string -> ref Arc<Mutex<[unit -> i64]>> -> unit\nfn putc t c = Mutex.with_lock (Arc.get c) (\\b -> Array.write 0 (\\() -> look t) b)\ndef readc :: ref Arc<Mutex<[unit -> i64]>> -> i64\nfn readc c = Mutex.with_lock (Arc.get c) (\\b -> b[0] ())\nrecord PR { c: Holder2, s: string }\ndef mkpr :: unit -> PR\nfn mkpr _u = PR { c: mkh2 (), s: mks () }\ndef keepself :: ref mut PR -> i64\nfn keepself p =\n    let t = ref p.s\n    putc t (ref p.c.cell)\n    0\ndef junkread :: ref Arc<Mutex<[unit -> i64]>> -> unit\nfn junkread c =\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\"), String.repeat 4 (ref \"xxxxxxxxx\")]\n    do! IO.write_line (\"read \" + to_string (readc c) + \" \" + to_string junk.length)\ndef cross :: ref PairOk -> ref PairOk -> unit\nfn cross a b = put (ref b.s) (ref a.h)\ndef selfbad :: ref Pair -> unit\nfn selfbad p = put (ref p.s) (ref p.h)\ndef nothingp :: ref Pair -> unit\nfn nothingp p = ()\ndef pickit :: i64 -> (ref [Pair] -> unit)\nfn pickit k = if k > 0 then Array.iterr selfbad else Array.iterr nothingp\ndef pickself :: i64 -> (ref Pair -> unit)\nfn pickself k = if k > 0 then selfbad else nothingp\ndef readh :: ref Holder -> i64\nfn readh h = Mutex.with_lock (ref h.cell) (\\b -> b[0] ())\ndef read_cell :: ref Mutex<[unit -> i64]> -> (unit -> i64)\nfn read_cell m = \\() -> Mutex.with_lock m (\\b -> b[0] ())\nrecord Watcher {r} { target: ref {r} Mutex<[unit -> i64]> }\ninstance Drop<Watcher> {\n    fn drop w = do! IO.write_line (\"dropw \" + to_string (Mutex.with_lock w.target (\\b -> b[0] ())))\n}\ndef run :: i64 -> i64\nfn run n =\n    let mut rc = Rc.new (mkpo ())\n    let k = { let inner = Rc.share (ref rc); let p = Rc.get (ref inner); put (ref p.s) (ref p.h); 0 }\n    rc = Rc.new (mkpo ())\n    n + k\nrun 1",
    )]);
}

/// A31, after review 12 (the fifth merge into A32): the programs of
/// `a_call_that_stores_a_part_of_one_reference_argument_in_another_keeps_a_borrow_of_that_part`.
/// `cross a b` keeps a borrow of `b.s` in the cell of `a.h`; `crossb` does the same for
/// `PairBad`, whose string is dropped before its holder; `crossw a w` keeps a borrow of the string
/// of what the reference field `w.p` points at; `crossm` keeps a shared borrow of the string of
/// what its `ref mut` parameter points at. The body of `run` starts at line 35.
const CROSS_PRELUDE: &str = r#"record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
record PairBad { s: string, h: Holder }
record Two { a: PairOk, b: PairOk }
record TwoBad { b: PairOk, a: PairOk }
record Wrap {r} { p: ref {r} PairOk }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def mkpb :: unit -> PairBad
fn mkpb _u = PairBad { s: String.repeat 4 (ref "123456789"), h: Holder { cell: Mutex.create [\() -> 0] } }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def crossb :: ref PairBad -> ref PairBad -> unit
fn crossb a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def crossw :: ref PairOk -> ref Wrap -> unit
fn crossw a w = { let t = ref w.p.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def apply2 :: (ref PairOk -> ref PairOk -> unit) -> ref PairOk -> ref PairOk -> i64
fn apply2 f a b = { f a b; 0 }
def apply2g :: (ref 'a -> ref 'a -> unit) -> ref 'a -> ref 'a -> i64
fn apply2g f a b = { f a b; 0 }
def outer :: ref PairOk -> ref PairOk -> unit
fn outer a b = cross a b
def crossm :: ref PairOk -> ref mut PairOk -> unit
fn crossm a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
"#;

fn cross_program(body: &str) -> String {
    format!("{CROSS_PRELUDE}{body}do! IO.write_line (to_string (run 1))\n")
}

#[test]
fn a_call_that_stores_a_part_of_one_reference_argument_in_another_keeps_a_borrow_of_that_part() {
    // A31, after review 12: `cross (ref p2) (ref p1)` keeps a borrow of `p1.s` in `p2`, which is
    // declared after `p1` and dropped first, so the drop of its holder reads `p1.s` while `p1` is
    // still there (main prints `drop 8`, `drop 0`, `1`). The call kept a borrow of the whole `p1`
    // in `p2` before, a borrow through which a store may reach `p1`'s cell, so applying the store
    // again went on into `p1`, which then kept a borrow of a part of itself (`E1013`
    // `self_referential_store`). The callee now tells the part (`StoreFacts::kept_paths`): `p2`
    // keeps a borrow of `p1.s`, a string, through which no store reaches anything. Every spelling
    // of the call and a reference to one of two values do the same; when both arguments point at
    // one value, the drop order of that value's parts decides.
    for body in [
        // loc_direct
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    cross (ref p2) (ref p1)\n    n\n",
        // a pipeline
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    ref p1 |> cross (ref p2)\n    n\n",
        // a partial application
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let f = cross (ref p2)\n    f (ref p1)\n    n\n",
        // a function-valued parameter (loc_safe__fn)
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    apply2 cross (ref p2) (ref p1) + n\n",
        // a generic function-valued parameter
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    apply2g cross (ref p2) (ref p1) + n\n",
        // a lambda
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    (\\a b -> cross a b) (ref p2) (ref p1)\n    n\n",
        // a function that passes its reference parameters on
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    outer (ref p2) (ref p1)\n    n\n",
        // a reference to one of two values: `p1.s` or `p2.s`, which `p2` drops after its holder
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    cross (ref p2) (if n > 0 then ref p1 else ref p2)\n    n\n",
        // one value: the holder is dropped before the string (main prints `drop 8`, `1`)
        "    let p = mkpo ()\n    cross (ref p) (ref p)\n    n\n",
        // two parts of one value: `t.a` is dropped before `t.b`
        "    let t = Two { a: mkpo (), b: mkpo () }\n    cross (ref t.a) (ref t.b)\n    n\n",
        // a fresh exclusive borrow of which the callee stores a shared borrow of a part
        "    let mut p1 = mkpo ()\n    let p2 = mkpo ()\n    crossm (ref p2) (ref mut p1)\n    n\n",
    ] {
        accepts_in_both_editions(&[("Main.tz", &cross_program(body))]);
    }
    // The borrow is of `p1.s` only: replacing `p1.h` does not conflict with it (main prints
    // `drop 0`, `drop 8`, `drop 5`, `1`), replacing `p1.s` does (main reads the freed string).
    // A field assignment is edition 2027.
    let write_other = cross_program(
        "    let mut p1 = mkpo ()\n    let p2 = mkpo ()\n    cross (ref p2) (ref p1)\n    p1.h = Holder { cell: Mutex.create [\\() -> 5] }\n    n\n",
    );
    analyze_with_edition(&write_other, Edition::E2027)
        .unwrap_or_else(|error| panic!("{write_other}\n{}: {}", error.code, error.message));
    let write_borrowed = cross_program(
        "    let mut p1 = mkpo ()\n    let p2 = mkpo ()\n    cross (ref p2) (ref p1)\n    p1.s = String.repeat 2 (ref \"abc\")\n    n\n",
    );
    let error = analyze_with_edition(&write_borrowed, Edition::E2027).expect_err(&write_borrowed);
    assert_eq!(
        (
            error.code,
            error.kind,
            position(&write_borrowed, error.span.start)
        ),
        ("E1014", Some("borrow_conflict"), (38, 5)),
        "{}",
        error.message
    );
    for (body, kind) in [
        // The swapped call keeps a borrow of `p2.s` in `p1`, which is dropped after `p2` (main
        // prints `drop 0`, then reads the freed string), in every spelling.
        (
            "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    cross (ref p1) (ref p2)\n    n\n",
            "assignment_extends_borrow",
        ),
        (
            "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    ref p2 |> cross (ref p1)\n    n\n",
            "assignment_extends_borrow",
        ),
        (
            "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    apply2 cross (ref p1) (ref p2) + n\n",
            "assignment_extends_borrow",
        ),
        (
            "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    apply2g cross (ref p1) (ref p2) + n\n",
            "assignment_extends_borrow",
        ),
        (
            "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    outer (ref p1) (ref p2)\n    n\n",
            "assignment_extends_borrow",
        ),
        (
            "    let p1 = mkpo ()\n    let mut p2 = mkpo ()\n    crossm (ref p1) (ref mut p2)\n    n\n",
            "assignment_extends_borrow",
        ),
        // One value whose string is dropped before its holder.
        (
            "    let p = mkpb ()\n    crossb (ref p) (ref p)\n    n\n",
            "self_referential_store",
        ),
        // A reference that may point at `q2` itself.
        (
            "    let q1 = mkpb ()\n    let q2 = mkpb ()\n    crossb (ref q2) (if n > 0 then ref q1 else ref q2)\n    n\n",
            "self_referential_store",
        ),
        // Two parts of one value: `t.b` is dropped before `t.a`, whose holder reads `t.b.s`.
        (
            "    let t = TwoBad { b: mkpo (), a: mkpo () }\n    cross (ref t.a) (ref t.b)\n    n\n",
            "self_referential_store",
        ),
        // The conservative limit that drop.md states: a borrow through a reference that the
        // argument's value holds (`ref w.p.s`) stands for the whole argument, through which the
        // store reaches `p1`, which then keeps a borrow of `w`, declared after it (main prints
        // `drop 8`, `drop 0`, `1`).
        (
            "    let p1 = mkpo ()\n    let w = Wrap { p: ref p1 }\n    let p2 = mkpo ()\n    crossw (ref p2) (ref w)\n    n\n",
            "assignment_extends_borrow",
        ),
    ] {
        rejects_at(&cross_program(body), "E1013", kind, (35, 5));
    }
}

/// A31, the sixth merge into A32 (`fvpart`): an immutable local bound to `cross` (or to a choice
/// of named functions, a lambda, another such local) runs `cross` in a pipeline and through a
/// partial application of it as in a direct call (`apply_higher_once` substitutes the local's
/// expressions after the bound arguments, `known_functions` knows `let g = chosen (ref p2)`, and
/// the partial application runs no code and drops what the local and the bound arguments drop).
/// They used the conservative rule of a function value whose code does not show, which kept a
/// borrow of `p2` in `p1` (main prints `drop 8`, `drop 0`, `1`). The swapped calls keep a borrow of
/// `p2.s` in `p1`, which is dropped after `p2` (main reads the freed string).
#[test]
fn a_local_bound_to_a_named_function_keeps_the_parts_in_a_pipeline_or_partial_application() {
    const THREE: &str =
        "\\(a: ref PairOk) (b: ref PairOk) (c: ref PairOk) -> { cross a b; cross a c }";
    for (binding, safe, swapped) in [
        // the repros: an alias and a choice, in a pipeline and a partial application
        (
            "    let chosen = cross\n",
            "    ref p1 |> chosen (ref p2)\n",
            "    ref p2 |> chosen (ref p1)\n",
        ),
        (
            "    let chosen = cross\n",
            "    let g = chosen (ref p2)\n    g (ref p1)\n",
            "    let g = chosen (ref p1)\n    g (ref p2)\n",
        ),
        (
            "    let chosen = if n < 0 then cross else cross\n",
            "    ref p1 |> chosen (ref p2)\n",
            "    ref p2 |> chosen (ref p1)\n",
        ),
        (
            "    let chosen = if n < 0 then cross else outer\n",
            "    let g = chosen (ref p2)\n    g (ref p1)\n",
            "    let g = chosen (ref p1)\n    g (ref p2)\n",
        ),
        // an alias of an alias, a lambda, a partial application completed by a pipeline
        (
            "    let a = cross\n    let chosen = a\n",
            "    let g = chosen (ref p2)\n    g (ref p1)\n",
            "    let g = chosen (ref p1)\n    g (ref p2)\n",
        ),
        (
            "    let chosen = \\(a: ref PairOk) (b: ref PairOk) -> cross a b\n",
            "    ref p1 |> chosen (ref p2)\n",
            "    ref p2 |> chosen (ref p1)\n",
        ),
        (
            "    let chosen = cross\n",
            "    let g = chosen (ref p2)\n    ref p1 |> g\n",
            "    let g = chosen (ref p1)\n    ref p2 |> g\n",
        ),
    ] {
        let declared = "    let p1 = mkpo ()\n    let p2 = mkpo ()\n";
        accepts_in_both_editions(&[(
            "Main.tz",
            &cross_program(&format!("{declared}{binding}{safe}    n\n")),
        )]);
        rejects_at(
            &cross_program(&format!("{declared}{binding}{swapped}    n\n")),
            "E1013",
            "assignment_extends_borrow",
            (35, 5),
        );
    }
    // The partial application alone runs no code: nothing keeps a borrow (main prints `drop 0`,
    // `drop 0`, `1`).
    accepts_in_both_editions(&[(
        "Main.tz",
        &cross_program(
            "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let chosen = cross\n    let g = chosen (ref p2)\n    n\n",
        ),
    )]);
    // A choice whose taken member stores the other way (n = 1 takes the lambda, which keeps a
    // borrow of `p2.s` in `p1`).
    rejects_at(
        &cross_program(
            "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let chosen = if n < 0 then cross else (\\(a: ref PairOk) (b: ref PairOk) -> cross b a)\n    ref p1 |> chosen (ref p2)\n    n\n",
        ),
        "E1013",
        "assignment_extends_borrow",
        (35, 5),
    );
    // Three values and partial applications of partial applications: `p3`, declared last, keeps
    // borrows of `p1.s` and `p2.s` (main prints `drop 16`, `drop 0`, `drop 0`, `1`).
    let declared = "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n";
    for (safe, swapped) in [
        (
            "    let g = chosen (ref p3) (ref p1)\n    g (ref p2)\n",
            "    let g = chosen (ref p1) (ref p2)\n    g (ref p3)\n",
        ),
        (
            "    let g = chosen (ref p3)\n    let h = g (ref p1)\n    h (ref p2)\n",
            "    let g = chosen (ref p1)\n    let h = g (ref p2)\n    h (ref p3)\n",
        ),
        (
            "    let g = chosen (ref p3)\n    let h = g (ref p1)\n    ref p2 |> h\n",
            "    let g = chosen (ref p1)\n    let h = g (ref p2)\n    ref p3 |> h\n",
        ),
        (
            "    ref p2 |> chosen (ref p3) (ref p1)\n",
            "    ref p3 |> chosen (ref p1) (ref p2)\n",
        ),
    ] {
        let binding = format!("    let chosen = {THREE}\n");
        accepts_in_both_editions(&[(
            "Main.tz",
            &cross_program(&format!("{declared}{binding}{safe}    n\n")),
        )]);
        rejects_at(
            &cross_program(&format!("{declared}{binding}{swapped}    n\n")),
            "E1013",
            "assignment_extends_borrow",
            (35, 5),
        );
    }
    // The conservative limit that drop.md states (A33 `fvparam`): a function-valued parameter that
    // the callee applies partially, or in a pipeline, is there a function value whose code does
    // not show, so the caller lets each argument's borrow go into the others' targets, and `p1`
    // keeps a borrow of `p2` (main prints `drop 8`, `drop 0`, `1`). The helpers come after `run`.
    const APPLY_PARTIAL: &str = "def applyp :: (ref PairOk -> ref PairOk -> unit) -> ref PairOk -> ref PairOk -> i64\nfn applyp f a b = { let g = f a; g b; 0 }\ndef applyq :: (ref PairOk -> ref PairOk -> unit) -> ref PairOk -> ref PairOk -> i64\nfn applyq f a b = { b |> f a; 0 }\n";
    for call in ["applyp", "applyq"] {
        let source = format!(
            "{CROSS_PRELUDE}    let p1 = mkpo ()\n    let p2 = mkpo ()\n    {call} cross (ref p2) (ref p1) + n\n{APPLY_PARTIAL}do! IO.write_line (to_string (run 1))\n"
        );
        rejects_at(&source, "E1013", "assignment_extends_borrow", (35, 5));
    }
}

/// A31, after the review of the fix after review 12: the programs of
/// `a_borrow_through_the_result_of_a_call_is_one_of_the_whole_argument`. `child n` returns a
/// reference to the first child of `n` (`Rc.get (ref n.kids[0])`), which holds `n`'s loan while it
/// points at another `Node`. `crossn a b` keeps a borrow of `b.s` in the cell of `a.h`, `crossq`
/// and `crossv` one of `(child b).s`, `selfn b` one of `b.s` in `b.h`, `selfq` and `selfm` one of
/// `(child b).s` in `b.h`, and `copyh a b` copies the function in `a.h` into `b.h`. The body of
/// `run` starts at line 44.
const NODE_PRELUDE: &str = r#"record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def mkh :: unit -> Holder
fn mkh _u = Holder { cell: Mutex.create [\() -> 0] }
def mks :: unit -> string
fn mks _u = String.repeat 4 (ref "123456789")
record Node { kids: [Rc<Node>], h: Holder, s: string }
record PairOk { h: Holder, s: string }
def leaf :: unit -> Node
fn leaf _u = Node { kids: [], h: mkh (), s: mks () }
def mkn :: unit -> Node
fn mkn _u = Node { kids: [Rc.new (leaf ())], h: mkh (), s: mks () }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: mkh (), s: mks () }
def child :: ref Node -> ref Node
fn child n = Rc.get (ref n.kids[0])
def crossn :: ref Node -> ref Node -> unit
fn crossn a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def crossq :: ref Node -> ref Node -> unit
fn crossq a b = { let q = child b; let t = ref q.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def crossv :: ref mut [unit -> i64] -> ref Node -> unit
fn crossv a b = { let q = child b; let t = ref q.s; Array.write 0 (\() -> look t) a }
def viachild :: ref Node -> ref Node -> unit
fn viachild a b = crossn a (child b)
def selfn :: ref Node -> unit
fn selfn b = { let t = ref b.s; Mutex.with_lock (ref b.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def selfq :: ref Node -> unit
fn selfq b = { let q = child b; let t = ref q.s; Mutex.with_lock (ref b.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def selfm :: ref mut Node -> unit
fn selfm b = { let q = child b; let t = ref q.s; Mutex.with_lock (ref b.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def put_fn :: (unit -> i64) -> ref Holder -> unit
fn put_fn f h = Mutex.with_lock (ref h.cell) (\d -> Array.write 0 f d)
def copyh :: ref Node -> ref PairOk -> unit
fn copyh a b = Mutex.with_lock (ref a.h.cell) (\c -> put_fn c[0] (ref b.h))
def run :: i64 -> i64
fn run n =
"#;

fn node_program(body: &str) -> String {
    format!("{NODE_PRELUDE}{body}do! IO.write_line (to_string (run 1))\n")
}

#[test]
fn a_borrow_through_the_result_of_a_call_is_one_of_the_whole_argument() {
    // A31, after the review of the fix after review 12, findings 1 to 3: the result of `child`
    // holds the loan of its argument `ref p` while it points at the child, another `Node` that
    // `p.kids` holds. A borrow of a part of what it points at is one of a part of the child, not
    // of `p`: `p.h` keeping a borrow of `(child p).s` keeps one of a string that `p.kids` drops
    // before `p.h` (main prints `drop 0`, then reads the freed string). The fix after review 12
    // took the loan's place for what the reference points at, so the callee recorded `[s]` for
    // `let q = child b; ref q.s`, and the caller narrowed `child (ref p)` to `p.s`: every form
    // below was accepted. Now only a reference that points at the place of one of its loans
    // (a borrow of an exact place, a reference parameter, a local bound to one) names parts of
    // that place, and any other borrow is one of the whole argument.
    for body in [
        // n9: the call result is the argument
        "    let p = mkn ()\n    crossn (ref p) (child (ref p))\n    n\n",
        // n10: a function passes `child b` on
        "    let p = mkn ()\n    viachild (ref p) (ref p)\n    n\n",
        // n1: the callee borrows through a local bound to the call result
        "    let p = mkn ()\n    crossq (ref p) (ref p)\n    n\n",
        // b1: a pipeline
        "    let p = mkn ()\n    child (ref p) |> crossn (ref p)\n    n\n",
        // s1 (main accepts it too): a self store through a local bound to the call result
        "    let p = mkn ()\n    selfq (ref p)\n    n\n",
        // s2: the same through an exclusive reference
        "    let mut p = mkn ()\n    selfm (ref mut p)\n    n\n",
    ] {
        rejects_at(
            &node_program(body),
            "E1013",
            "self_referential_store",
            (44, 5),
        );
    }
    // Edition 2027 (a field assignment): after the call, `n1.kids = []` frees the child whose
    // string the other value keeps (main reads the freed string).
    for (body, at) in [
        // n2: the borrow is kept in another value
        (
            "    let mut n1 = mkn ()\n    let p2 = leaf ()\n    crossq (ref p2) (ref n1)\n    n1.kids = []\n    n\n",
            (47, 5),
        ),
        // n3: in an array of functions
        (
            "    let mut n1 = mkn ()\n    let mut arr = [\\() -> 0]\n    crossv (ref mut arr) (ref n1)\n    n1.kids = []\n    arr[0] ()\n",
            (47, 5),
        ),
        // A self store through the call result, which `copyh` copies into `p2` (main accepts it
        // too and prints `drop 8`, `drop -1`: `p2` reads the freed string).
        (
            "    let mut n1 = mkn ()\n    let p2 = mkpo ()\n    selfn (child (ref n1))\n    copyh (child (ref n1)) (ref p2)\n    n1.kids = []\n    n\n",
            (48, 5),
        ),
    ] {
        let program = node_program(body);
        let error = analyze_with_edition(&program, Edition::E2027).expect_err(&program);
        assert_eq!(
            (error.code, error.kind, position(&program, error.span.start)),
            ("E1014", Some("borrow_conflict"), at),
            "{program}\n{}",
            error.message
        );
    }
    // The safe forms stay accepted: a borrow of `n1.s` itself, which a write of `n1.kids` does not
    // free (edition 2027), and a self store through the call result, kept in the child itself or
    // copied into a value that is dropped before `n1`.
    let write_kids = node_program(
        "    let mut n1 = mkn ()\n    let p2 = leaf ()\n    crossn (ref p2) (ref n1)\n    n1.kids = []\n    n\n",
    );
    analyze_with_edition(&write_kids, Edition::E2027)
        .unwrap_or_else(|error| panic!("{write_kids}\n{}: {}", error.code, error.message));
    for body in [
        "    let n1 = mkn ()\n    let p2 = leaf ()\n    crossn (ref p2) (ref n1)\n    n\n",
        "    let n1 = mkn ()\n    selfn (child (ref n1))\n    n\n",
        "    let n1 = mkn ()\n    let p2 = mkpo ()\n    selfn (child (ref n1))\n    copyh (child (ref n1)) (ref p2)\n    n\n",
    ] {
        accepts_in_both_editions(&[("Main.tz", &node_program(body))]);
    }
    // n18 and n20 (main rejects them for another reason): the callee borrows through a local bound
    // to the call result, and the caller keeps a borrow of the whole argument. Without it,
    // `n1.kids = []` freed the string that the cell of `m` reads (n18, edition 2027), and `p`
    // kept a borrow of the string of its child (n20).
    let n18 = "def look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1\ndef mks :: unit -> string\nfn mks _u = String.repeat 4 (ref \"123456789\")\nrecord Node { kids: [Rc<Node>], s: string }\ndef leaf :: unit -> Node\nfn leaf _u = Node { kids: [], s: mks () }\ndef child :: ref Node -> ref Node\nfn child n = Rc.get (ref n.kids[0])\ndef crossx :: ref Mutex<[unit -> i64]> -> ref Node -> unit\nfn crossx a b = { let q = child b; let t = ref q.s; Mutex.with_lock a (\\c -> Array.write 0 (\\() -> look t) c) }\ndef firsts :: ref string -> ref [Rc<Node>] -> ref string\nfn firsts a _b = a\ndef firstf :: ref string -> ref [Rc<Node>] -> (unit -> i64)\nfn firstf a _b =\n    let mut arr = [\\() -> 0]\n    Array.set 0 (\\() -> look a) (ref mut arr)\n    arr[0]\ndef crossy :: ref Mutex<[unit -> i64]> -> ref Node -> unit\nfn crossy a b = { let t = ref b.s; Mutex.with_lock a (\\c -> Array.write 0 (\\() -> look t) c) }\ndef run :: i64 -> i64\nfn run n =\n    let mut n1 = Node { kids: [Rc.new (leaf ())], s: mks () }\n    let m = Mutex.create [\\() -> 0]\n    let other = mks ()\n    crossx (ref m) (ref n1)\n    let k = firstf (ref other) (ref n1.kids)\n    n1.kids = []\n    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    do! IO.write_line (\"read \" + to_string (Mutex.with_lock (ref m) (\\c -> c[0] ())) + \" \" + to_string (k ()) + \" \" + to_string junk.length)\n    n\ndo! IO.write_line (to_string (run 1))\n";
    let error = analyze_with_edition(n18, Edition::E2027).expect_err(n18);
    assert_eq!(
        (error.code, error.kind, position(n18, error.span.start)),
        ("E1014", Some("borrow_conflict"), (31, 5)),
        "{}",
        error.message
    );
    let n20 = |call: &str| {
        format!(
            "{}record Node {{ kids: [Rc<Node>], h: Holder, s: string }}\ndef leaf :: unit -> Node\nfn leaf _u = Node {{ kids: [], h: mkh (), s: mks () }}\ndef child :: ref Node -> ref Node\nfn child n = Rc.get (ref n.kids[0])\ndef crossn :: ref Node -> ref Node -> unit\nfn crossn a b = {{ let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\\c -> Array.write 0 (\\() -> look t) c) }}\ndef firstf :: ref string -> ref [Rc<Node>] -> (unit -> i64)\nfn firstf a _b =\n    let mut arr = [\\() -> 0]\n    Array.set 0 (\\() -> look a) (ref mut arr)\n    arr[0]\ndef crossq :: ref Node -> ref Node -> unit\nfn crossq a b = {{ let q = child b; let t = ref q.s; Mutex.with_lock (ref a.h.cell) (\\c -> Array.write 0 (\\() -> look t) c) }}\ndef run :: ref string -> (unit -> i64)\nfn run keep =\n    let p = Node {{ kids: [Rc.new (leaf ())], h: mkh (), s: mks () }}\n    {call}\n    firstf keep (ref p.kids)\nlet k = \"ab\"\nlet f = run (ref k)\ndo! IO.write_line (to_string (f ()))\n",
            NODE_PRELUDE
                .lines()
                .take(13)
                .map(|line| format!("{line}\n"))
                .collect::<String>()
        )
    };
    for call in ["crossq (ref p) (ref p)", "crossn (ref p) (child (ref p))"] {
        rejects_at(&n20(call), "E1013", "self_referential_store", (30, 5));
    }
}

/// A31, after the review of the fix after review 12: the programs of
/// `a_part_bound_or_passed_to_a_function_value_keeps_a_borrow_of_that_part`. `cross a b` keeps a
/// borrow of `b.s` in the cell of `a.h`, `cross3 a b c` borrows of `b.s` and `c.s`, and `cross5 a b`
/// borrows of the five strings of `b`; `applyp` and `each` pass a part of their argument (a field,
/// an element) to a function value, and `crossall` passes each element to `cross a` through
/// `Array.iterr`; `pick` returns one of its reference arguments; `selfok p` keeps a borrow of `p.s`
/// in `p.h`, and `crossel arr` one of `arr[0].s` in `arr[1].h`. The body of `run` starts at line 41.
const PARTS_PRELUDE: &str = r#"record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
record Outer { inner: PairOk }
record P5 { h: Holder, s1: string, s2: string, s3: string, s4: string, s5: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def mk5 :: unit -> P5
fn mk5 _u = P5 { h: Holder { cell: Mutex.create [\() -> 0] }, s1: to_string 1, s2: to_string 2, s3: to_string 3, s4: to_string 4, s5: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def cross3 :: ref PairOk -> ref PairOk -> ref PairOk -> unit
fn cross3 a b c = { let t = ref b.s; let u = ref c.s; Mutex.with_lock (ref a.h.cell) (\x -> Array.write 0 (\() -> look t + look u) x) }
def cross5 :: ref P5 -> ref P5 -> unit
fn cross5 a b = { let t1 = ref b.s1; let t2 = ref b.s2; let t3 = ref b.s3; let t4 = ref b.s4; let t5 = ref b.s5; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t1 + look t2 + look t3 + look t4 + look t5) c) }
def apply1 :: (ref PairOk -> unit) -> ref PairOk -> i64
fn apply1 f a = { f a; 0 }
def applyp :: (ref PairOk -> unit) -> ref Outer -> i64
fn applyp f o = { f (ref o.inner); 0 }
def each :: (ref PairOk -> unit) -> ref [PairOk] -> i64
fn each f arr = { f (ref arr[0]); 0 }
def crossall :: ref PairOk -> ref [PairOk] -> unit
fn crossall a arr = Array.iterr (cross a) arr
def crossk :: ref PairOk -> ref PairOk -> i64
fn crossk a b = { cross a b; 1 }
def pick :: i64 -> ref PairOk -> ref PairOk -> ref PairOk
fn pick k x y = if k > 0 then x else y
def selfok :: ref PairOk -> unit
fn selfok p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def crossel :: ref [PairOk] -> unit
fn crossel arr = cross (ref arr[1]) (ref arr[0])
def run :: i64 -> i64
fn run n =
"#;

fn parts_program(body: &str) -> String {
    format!("{PARTS_PRELUDE}{body}do! IO.write_line (to_string (run 1))\n")
}

#[test]
fn a_part_bound_or_passed_to_a_function_value_keeps_a_borrow_of_that_part() {
    // A31, after the review of the fix after review 12, findings 4 and 5: the value that keeps
    // the borrows is declared after the borrowed ones, so it is dropped first and its holder reads
    // strings that are still there (main prints `drop 16`, `drop 0`, `drop 0`, `1` for the three
    // values, `drop 8` for the holder of one string). A partial application gave each argument
    // that it bound the loans of the whole function value, so `p3` and `p1` each kept a borrow of
    // the other's string; a part of an argument that a callee passes to a function value
    // (`ref o.inner`, `ref arr[0]`, the elements that `Array.iterr` lends) was a borrow of the
    // whole argument, through which the store went on into it. Now a bound argument that borrows
    // an exact place is the part of the function value's loans that its borrow made, and the part
    // is a borrow of that part of the caller's argument when the argument points at its place. A
    // reference that `pick`, a `match` or an `if` returns points at one of its choices.
    for body in [
        // s_partial2: a partial application of two of the three arguments
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n    let g = cross3 (ref p3) (ref p1)\n    g (ref p2)\n    n\n",
        // s_partial2_pipe: a pipeline
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n    ref p2 |> cross3 (ref p3) (ref p1)\n    n\n",
        // s_partial2_apply: the partial application passed to a function-valued parameter
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n    apply1 (cross3 (ref p3) (ref p1)) (ref p2) + n\n",
        // s_each_field: a field passed to a function value
        "    let o1 = Outer { inner: mkpo () }\n    let p2 = mkpo ()\n    applyp (cross (ref p2)) (ref o1) + n\n",
        // s_each_user: an element passed to a function value
        "    let arr = [mkpo (), mkpo ()]\n    let p2 = mkpo ()\n    each (cross (ref p2)) (ref arr) + n\n",
        // s_iterr_partial: std's `Array.iterr`
        "    let arr = [mkpo (), mkpo ()]\n    let p2 = mkpo ()\n    Array.iterr (cross (ref p2)) (ref arr)\n    n\n",
        // s_iterr_partial_arrfirst_named: the same inside a named function
        "    let arr = [mkpo (), mkpo ()]\n    let p2 = mkpo ()\n    crossall (ref p2) (ref arr)\n    n\n",
        // pick3: a function that returns one of its reference arguments
        "    let p1 = mkpo ()\n    let p3 = mkpo ()\n    let p2 = mkpo ()\n    cross (ref p2) (pick n (ref p1) (ref p3))\n    n\n",
        // s_match3 and s_several3: a choice of references
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n    let p4 = mkpo ()\n    let r = match n with\n        | 0 -> ref p1\n        | 1 -> ref p2\n        | _ -> ref p3\n    cross (ref p4) r\n    n\n",
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n    cross (ref p3) (if n > 0 then ref p1 else ref p2)\n    n\n",
        // iterir_local_safe and its kin: a borrow that `selfok` keeps in the value that its
        // parameter points at, an element here, is one inside that element, whose parts keep the
        // order that `selfok` showed (main prints `drop 8`, `drop 0`, `1`).
        "    let arr = [mkpo (), mkpo ()]\n    selfok (ref arr[0])\n    n\n",
        "    let arr = [mkpo (), mkpo ()]\n    Array.iterr selfok (ref arr)\n    n\n",
        "    let arr = [mkpo ()]\n    Array.iterir (\\i (p: ref PairOk) -> { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b) }) (ref arr)\n    n\n",
    ] {
        accepts_in_both_editions(&[("Main.tz", &parts_program(body))]);
    }
    // A borrow of one element kept in another is not one inside an element: the elements are not
    // told apart, so it is rejected whatever their order.
    for body in [
        "    let arr = [mkpo (), mkpo ()]\n    crossel (ref arr)\n    n\n",
        "    let arr = [mkpo (), mkpo ()]\n    cross (ref arr[1]) (ref arr[0])\n    n\n",
    ] {
        rejects_at(
            &parts_program(body),
            "E1013",
            "self_referential_store",
            (41, 5),
        );
    }
    // The value that keeps the borrows is declared first, and outlives the borrowed strings (main
    // reads freed strings), in each spelling.
    for body in [
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n    let g = cross3 (ref p1) (ref p3)\n    g (ref p2)\n    n\n",
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n    ref p2 |> cross3 (ref p1) (ref p3)\n    n\n",
        "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n    apply1 (cross3 (ref p1) (ref p3)) (ref p2) + n\n",
        "    let p0 = mkpo ()\n    let o1 = Outer { inner: mkpo () }\n    applyp (cross (ref p0)) (ref o1) + n\n",
        "    let p0 = mkpo ()\n    let arr = [mkpo (), mkpo ()]\n    each (cross (ref p0)) (ref arr) + n\n",
        "    let p0 = mkpo ()\n    let arr = [mkpo (), mkpo ()]\n    Array.iterr (cross (ref p0)) (ref arr)\n    n\n",
        "    let p0 = mkpo ()\n    let arr = [mkpo (), mkpo ()]\n    crossall (ref p0) (ref arr)\n    n\n",
        "    let p2 = mkpo ()\n    let p1 = mkpo ()\n    let p3 = mkpo ()\n    cross (ref p2) (pick n (ref p1) (ref p3))\n    n\n",
    ] {
        rejects_at(
            &parts_program(body),
            "E1013",
            "assignment_extends_borrow",
            (41, 5),
        );
    }
    // The conservative limits that drop.md states. `Array.mapr` builds its result with a function
    // whose stores the check does not tell apart, so `arr` is taken to keep the function that holds
    // a borrow of `p2` (main prints `drop 8`, `drop 0`, `drop 0`, `1`); past four borrowed parts,
    // `cross5` keeps a borrow of the whole of `q1` in `q2`, through which the store goes on into
    // `q1` (main prints `drop 4`, `drop 0`, `1`).
    rejects_at(
        &parts_program(
            "    let arr = [mkpo (), mkpo ()]\n    let p2 = mkpo ()\n    let ks = Array.mapr (crossk (ref p2)) (ref arr)\n    n\n",
        ),
        "E1013",
        "assignment_extends_borrow",
        (41, 5),
    );
    rejects_at(
        &parts_program(
            "    let q1 = mk5 ()\n    let q2 = mk5 ()\n    cross5 (ref q2) (ref q1)\n    n\n",
        ),
        "E1013",
        "self_referential_store",
        (41, 5),
    );
}

/// A31, found while checking the fix of `fvpart`: `mk a = \k f -> ...` returns a function value
/// that owns its cell (it captured the `Arc<Mutex<...>>` by value) and writes `f` there. Applying
/// it to its first argument (`w 0`) makes a function value of another type that holds it, so a
/// call of that value may write into the same cell. The scan of the module's function values
/// (`sink_flags`) took such a result for one that holds only the bound arguments, so a borrow of a
/// block-local array that `w2 (\() -> String.length text)` wrote into the cell was not kept, and
/// the next call ran the function after the array was gone (main and base accept; base prints
/// `72` read through the dangling borrow, and AddressSanitizer reports a stack use after scope).
/// The result of a call of a function value, or of the function that a named function returns,
/// now holds that value (`HolderReach`), as a lambda that captured it does.
#[test]
fn a_partial_application_of_a_function_value_that_owns_its_cell_shares_the_cell() {
    for (main, at) in [
        // immutable bindings: the partial application of a known local (this round)
        (
            r#"def mk :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)
fn mk a = \k f -> Mutex.with_lock (Arc.get (ref a)) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k })
def run :: i64 -> i64
fn run n =
    let w = mk (Arc.new (Mutex.create [\() -> 0]))
    let w2 = w 0
    let first = { let arr0 = [String.repeat 8 (ref "123456789")]; let text = ref arr0[0]; w2 (\() -> String.length text) }
    let other = [String.repeat 8 (ref "zzzzzzzzz"), String.repeat 8 (ref "yyyyyyyyy")]
    first + w2 (\() -> 1) + other.length * 0
do! IO.write_line (to_string (run 123456789))
"#,
            (7, 17),
        ),
        // `let mut w2 = w 0`
        (
            r#"def mk :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)
fn mk a = \k f -> Mutex.with_lock (Arc.get (ref a)) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k })
def run :: i64 -> i64
fn run n =
    let w = mk (Arc.new (Mutex.create [\() -> 0]))
    let mut w2 = w 0
    let first = { let arr0 = [String.repeat 8 (ref "123456789")]; let text = ref arr0[0]; w2 (\() -> String.length text) }
    first + w2 (\() -> 1)
do! IO.write_line (to_string (run 123456789))
"#,
            (7, 17),
        ),
        // `let mut w = mk (...)`, then `let w2 = w 0`
        (
            r#"def mk :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)
fn mk a = \k f -> Mutex.with_lock (Arc.get (ref a)) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k })
def run :: i64 -> i64
fn run n =
    let mut w = mk (Arc.new (Mutex.create [\() -> 0]))
    let w2 = w 0
    let first = { let arr0 = [String.repeat 8 (ref "123456789")]; let text = ref arr0[0]; w2 (\() -> String.length text) }
    first + w2 (\() -> 1)
do! IO.write_line (to_string (run 123456789))
"#,
            (7, 17),
        ),
        // a named function whose body returns the lambda after a binding, applied to more arguments than it takes
        (
            r#"def mk :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)
fn mk a =
    let z = 0
    \k f -> Mutex.with_lock (Arc.get (ref a)) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k + z })
def run :: i64 -> i64
fn run n =
    let mut w2 = mk (Arc.new (Mutex.create [\() -> 0])) 0
    let first = { let arr0 = [String.repeat 8 (ref "123456789")]; let text = ref arr0[0]; w2 (\() -> String.length text) }
    first + w2 (\() -> 1)
do! IO.write_line (to_string (run 123456789))
"#,
            (8, 17),
        ),
        // `let mut w2 = 0 |> w`
        (
            r#"def mk :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)
fn mk a = \k f -> Mutex.with_lock (Arc.get (ref a)) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k })
def run :: i64 -> i64
fn run n =
    let w = mk (Arc.new (Mutex.create [\() -> 0]))
    let mut w2 = 0 |> w
    let first = { let arr0 = [String.repeat 8 (ref "123456789")]; let text = ref arr0[0]; w2 (\() -> String.length text) }
    first + w2 (\() -> 1)
do! IO.write_line (to_string (run 123456789))
"#,
            (7, 17),
        ),
    ] {
        rejects_at(main, "E1013", "assignment_extends_borrow", at);
    }
    for main in [
        // the borrowed array declared before the function value (main prints `72`)
        r#"def mk :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)
fn mk a = \k f -> Mutex.with_lock (Arc.get (ref a)) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k })
def run :: i64 -> i64
fn run n =
    let arr0 = [String.repeat 8 (ref "123456789")]
    let text = ref arr0[0]
    let w = mk (Arc.new (Mutex.create [\() -> 0]))
    let mut w2 = w 0
    let first = w2 (\() -> String.length text)
    first + w2 (\() -> 1)
do! IO.write_line (to_string (run 123456789))
"#,
        // an owned function (main prints `7`)
        r#"def mk :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)
fn mk a = \k f -> Mutex.with_lock (Arc.get (ref a)) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k })
def run :: i64 -> i64
fn run n =
    let w = mk (Arc.new (Mutex.create [\() -> 0]))
    let mut w2 = w 0
    let first = w2 (\() -> 7)
    first + w2 (\() -> 1)
do! IO.write_line (to_string (run 123456789))
"#,
        // the borrowed array declared first, through the block-bodied `mk` (main prints `72`)
        r#"def mk :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)
fn mk a =
    let z = 0
    \k f -> Mutex.with_lock (Arc.get (ref a)) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k + z })
def run :: i64 -> i64
fn run n =
    let arr0 = [String.repeat 8 (ref "123456789")]
    let text = ref arr0[0]
    let mut w2 = mk (Arc.new (Mutex.create [\() -> 0])) 0
    let first = w2 (\() -> String.length text)
    first + w2 (\() -> 1)
do! IO.write_line (to_string (run 123456789))
"#,
    ] {
        accepts_in_both_editions(&[("Main.tz", main)]);
    }
}

/// A31, review round 1 after Copilot's review (findings 1 and 2): `applyy` and `applyx` hand
/// their function-valued parameter a reference to a `D` inside the block of an `Rc<D>` that their
/// argument's `e` holds. The recorded place of that borrow was merged into a shorter one (`[a]`
/// for `if k > 0 then ref (...x.a.e[0]...) else ref x.a`) or cut to four fields
/// (`[a, b, c, d]` for `x.a.b.c.d.e[0]`), and still counted as the place that the argument points
/// at, so the caller made an exact copy of it, which `crossd` narrowed to the `s` of the wrong `D`
/// (the type is the same, as `D` holds `[Rc<D>]`). Moving or replacing `e` then freed the borrowed
/// string, which `p2`'s holder read when it was dropped (main and base accept and read the freed
/// string: `drop -1`). A path that stands for a larger part than a borrowed place now names no
/// place (`Source::coarse`): the caller's copy borrows the larger part (`x.a`, `x.a.b.c.d`), not
/// narrowed further, so the move conflicts with it, and a write of `x.z` does not. Without the
/// move, main prints `drop 8` and `1`. The body of `run` starts at line 33.
const COARSE_PRELUDE: &str = r#"record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
record D { s: string, e: [Rc<D>] }
record X { a: D, z: string }
record C { d: D }
record B { c: C }
record A { b: B }
record Y { a: A, z: string }
def mkd :: unit -> D
fn mkd _u = D { s: String.repeat 4 (ref "123456789"), e: [] }
def mkx :: unit -> X
fn mkx _u = X { a: D { s: String.repeat 2 (ref "abc"), e: [Rc.new (mkd ())] }, z: to_string 1 }
def mky :: unit -> Y
fn mky _u = Y { a: A { b: B { c: C { d: D { s: String.repeat 2 (ref "abc"), e: [Rc.new (mkd ())] } } } }, z: to_string 1 }
def applyy :: i64 -> ref X -> (ref D -> unit) -> i64
fn applyy k x f = { (if k > 0 then ref (deref (Rc.get (ref x.a.e[0]))) else ref x.a) |> f; 0 }
def applyx :: ref Y -> (ref D -> unit) -> i64
fn applyx x f = { ref (deref (Rc.get (ref x.a.b.c.d.e[0]))) |> f; 0 }
def crossd :: ref PairOk -> ref D -> unit
fn crossd a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
"#;

#[test]
fn a_borrowed_place_merged_into_a_larger_part_is_no_place_that_the_argument_points_at() {
    let program = |body: &str| {
        format!(
            "{COARSE_PRELUDE}{body}    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    n + z + k * 0 + junk.length * 0\ndo! IO.write_line (to_string (run 1))\n"
        )
    };
    let merged = "    let x = mkx ()\n    let p2 = mkpo ()\n    let z = applyy n (ref x) (crossd (ref p2))\n";
    let cut =
        "    let x = mky ()\n    let p2 = mkpo ()\n    let z = applyx (ref x) (crossd (ref p2))\n";
    // The partial moves conflict with the borrow in both editions.
    for (setup, moved) in [
        (merged, "    let k = { let taken = x.a.e; taken.length }\n"),
        (
            cut,
            "    let k = { let taken = x.a.b.c.d.e; taken.length }\n",
        ),
    ] {
        rejects_at(
            &program(&format!("{setup}{moved}")),
            "E1014",
            "borrow_conflict",
            (36, 27),
        );
    }
    // Replacing the field of a mutable local conflicts the same way in edition 2027 (edition 2026
    // rejects the assignment itself with `E1012`, `assign_to_immutable_place`).
    for (setup, replaced) in [
        (merged, "    x.a.e = []\n    let k = 0\n"),
        (cut, "    x.a.b.c.d.e = []\n    let k = 0\n"),
    ] {
        let setup = setup.replace("let x = ", "let mut x = ");
        let source = program(&format!("{setup}{replaced}"));
        let error = analyze_with_edition(&source, Edition::E2027).expect_err(&source);
        assert_eq!(
            (error.code, error.kind, position(&source, error.span.start)),
            ("E1014", Some("borrow_conflict"), (36, 5)),
            "{source}\n{}",
            error.message
        );
    }
    // Without the move, nothing conflicts (main prints `drop 8` and `1`), nor does a write of a
    // part outside the borrowed one (edition 2027).
    for setup in [merged, cut] {
        accepts_in_both_editions(&[("Main.tz", &program(&format!("{setup}    let k = 0\n")))]);
        let setup = setup.replace("let x = ", "let mut x = ");
        let source = program(&format!("{setup}    x.z = to_string 22\n    let k = 0\n"));
        analyze_with_edition(&source, Edition::E2027)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    }
}

/// A31, review round 1 after Copilot's review (finding 3): the fix of
/// `a_partial_application_of_a_function_value_that_owns_its_cell_shares_the_cell` gave the result
/// of a call of a function value the applied value's environment, by type, so builtins that apply
/// a function argument (`Owned.call (ref w) 0`, `0 |> Owned.call (ref w)`, `new [T](1, w)`,
/// `Array.init 1 w`, `FixedArray.init w`) kept the hole open, and `Array.foldr`, whose callback
/// returns a function that its body makes, made `unit -> i64` the type of an owner in the
/// `in_place_writes` fixture (main accepts it). A function value made at a site that takes `arity`
/// arguments before its code runs now gives its partial applications, whatever applies them, the
/// types of the rest of its parameters (`partial_types`): `w2`, a partial application of
/// `mk`'s lambda, owns the cell, and the function that `Array.foldr`'s callback makes does not.
/// Main accepts the rejected forms and prints `72` read through a dangling borrow; with the array
/// declared first, it prints `72`, and with an owned function, `7`.
#[test]
fn a_partial_application_by_a_builtin_shares_the_cell_of_the_function_value() {
    const MK: &str = "def mk :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)\nfn mk a = \\k f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k })\ndef run :: i64 -> i64\nfn run _n =\n";
    const END: &str = "do! IO.write_line (to_string (run 123456789))\n";
    const OWNED: &str = "    let w = Owned.function (mk (Arc.new (Mutex.create [\\() -> 0])))\n";
    const PLAIN: &str = "    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n";
    const FIXED: &str =
        "    let ws: [(unit -> i64) -> i64; 1] = FixedArray.init w\n    let mut w2 = ws[0]\n";
    let forms: [(&str, &str, usize); 6] = [
        (OWNED, "    let mut w2 = Owned.call (ref w) 0\n", 7),
        (OWNED, "    let mut w2 = 0 |> Owned.call (ref w)\n", 7),
        (
            OWNED,
            "    let g = Owned.call (ref w)\n    let mut w2 = g 0\n",
            8,
        ),
        (
            PLAIN,
            "    let ws = new [(unit -> i64) -> i64](1, w)\n    let mut w2 = ws[0]\n",
            8,
        ),
        (
            PLAIN,
            "    let ws = new [|(unit -> i64) -> i64|](1, w)\n    let mut w2 = ws[0]\n",
            8,
        ),
        (
            PLAIN,
            "    let ws = Array.init 1 w\n    let mut w2 = ws[0]\n",
            8,
        ),
    ];
    let dangling = "    let first = { let arr0 = [String.repeat 8 (ref \"123456789\")]; let text = ref arr0[0]; w2 (\\() -> String.length text) }\n    first + w2 (\\() -> 1)\n";
    let declared_first = |made: &str, applied: &str| {
        format!(
            "{MK}    let arr0 = [String.repeat 8 (ref \"123456789\")]\n    let text = ref arr0[0]\n{made}{applied}    let first = w2 (\\() -> String.length text)\n    first + w2 (\\() -> 1)\n{END}"
        )
    };
    let owned = |made: &str, applied: &str| {
        format!(
            "{MK}{made}{applied}    let first = w2 (\\() -> 7)\n    first + w2 (\\() -> 1)\n{END}"
        )
    };
    for (made, applied, line) in forms {
        rejects_at(
            &format!("{MK}{made}{applied}{dangling}{END}"),
            "E1013",
            "assignment_extends_borrow",
            (line, 17),
        );
        accepts_in_both_editions(&[("Main.tz", &declared_first(made, applied))]);
        accepts_in_both_editions(&[("Main.tz", &owned(made, applied))]);
    }
    // `FixedArray.init w` type-checks in edition 2026 (2027 lends the elements to read parameters).
    let source = format!("{MK}{PLAIN}{FIXED}{dangling}{END}");
    let error = analyze_with_edition(&source, Edition::E2026).expect_err(&source);
    assert_eq!(
        (error.code, error.kind, position(&source, error.span.start)),
        ("E1013", Some("assignment_extends_borrow"), (8, 17)),
        "{source}\n{}",
        error.message
    );
    for source in [declared_first(PLAIN, FIXED), owned(PLAIN, FIXED)] {
        analyze_with_edition(&source, Edition::E2026)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    }
    // A callback that makes the function it returns gives no partial application of itself: the
    // state of the fold is no owner of the cell that the callback writes (the `in_place_writes`
    // suite prints `1011` for both).
    for fold in [
        "List.foldr (\\acc x -> { Mutex.with_lock (Arc.get (ref a2)) (\\slot -> Array.write 0 acc slot); \\() -> String.length text + deref x }) (\\() -> 0) (ref ys)",
        "Array.foldr (\\acc x -> { Mutex.with_lock (Arc.get (ref a2)) (\\slot -> Array.write 0 acc slot); \\() -> String.length text + deref x }) (\\() -> 0) (ref xs)",
    ] {
        let main = format!(
            "record Reading {{ f: unit -> i64 }}\ninstance Drop<Reading> {{\n    fn drop r = do! IO.write_line (to_string (r.f ()))\n}}\ndef folded :: i64 -> i64\nfn folded _n =\n    let text0 = to_string 123456789\n    let text = ref text0\n    let a = Arc.new (Mutex.create [\\() -> 0])\n    let a2 = Arc.share (ref a)\n    let xs = [1, 2]\n    let ys = [|1, 2|]\n    let last = {fold}\n    Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ()) * 100 + last () + xs.length * 0 + List.length ys * 0\ndef held :: i64 -> i64\nfn held _n =\n    let mut holder = Reading {{ f: \\() -> 0 }}\n    let text0 = to_string 123456789\n    let t = ref text0\n    holder = Reading {{ f: \\() -> String.length t }}\n    let k = holder.f ()\n    holder = Reading {{ f: \\() -> 3 }}\n    k\ndo! IO.write_line (to_string (folded 1 + held 1))\n"
        );
        accepts_in_both_editions(&[("Main.tz", &main)]);
    }
}

/// A31, review round 1 after Copilot's review (finding 4): a local bound to a `match` of named
/// functions, to a block (`{ cross }`), or to an `if` or `match` of partial applications
/// (`if n < 0 then cross (ref p2) else outer (ref p2)`, `if n < 0 then cross3 (ref p3) else
/// cross`) gets the facts of the member that it holds, as one bound to an `if` of named functions
/// does: `known_functions` now knows such locals, and dropping one runs what its branches, arms or
/// result run (`value_runs_drop_code`). Dropping a choice of partial applications was taken for
/// running code that may store anything, so `p2` kept a borrow of itself; the others were
/// function values whose code does not show. Main prints `drop 8`, `drop 0` and `1` (three
/// values: `drop 0`, `drop 8`, `drop 0`, `1`, and for `cross3` alone `drop 16`, `drop 0`,
/// `drop 0`, `1`); the swapped calls read a freed string in main.
#[test]
fn a_local_bound_to_a_match_block_or_choice_of_partial_applications_keeps_the_part() {
    let declared = "    let p1 = mkpo ()\n    let p2 = mkpo ()\n";
    for binding in [
        "    let chosen = match n with\n        | 0 -> cross\n        | _ -> cross\n",
        "    let chosen = match n with\n        | k when k > 5 -> cross\n        | _ -> outer\n",
        "    let chosen = { cross }\n",
        "    let chosen = { let unused = 3; outer }\n",
        "    let chosen = if n < 0 then { cross } else { let z = n; outer }\n",
        "    let chosen = match n with\n        | 0 -> if n > 0 then cross else outer\n        | _ -> { cross }\n",
    ] {
        for (safe, swapped) in [
            (
                "    chosen (ref p2) (ref p1)\n",
                "    chosen (ref p1) (ref p2)\n",
            ),
            (
                "    ref p1 |> chosen (ref p2)\n",
                "    ref p2 |> chosen (ref p1)\n",
            ),
            (
                "    let g = chosen (ref p2)\n    g (ref p1)\n",
                "    let g = chosen (ref p1)\n    g (ref p2)\n",
            ),
        ] {
            accepts_in_both_editions(&[(
                "Main.tz",
                &cross_program(&format!("{declared}{binding}{safe}    n\n")),
            )]);
            rejects_at(
                &cross_program(&format!("{declared}{binding}{swapped}    n\n")),
                "E1013",
                "assignment_extends_borrow",
                (35, 5),
            );
        }
    }
    // Choices of partial applications, called, piped and through an alias; alone, they run nothing
    // (main prints `drop 0`, `drop 0`, `1`).
    for choice in [
        "if n < 0 then cross (ref {X}) else cross (ref {X})",
        "if n < 0 then cross (ref {X}) else outer (ref {X})",
        "match n with\n        | 0 -> cross (ref {X})\n        | _ -> outer (ref {X})",
        "{ cross (ref {X}) }",
    ] {
        let safe = format!("    let chosen = {}\n", choice.replace("{X}", "p2"));
        let swapped = format!("    let chosen = {}\n", choice.replace("{X}", "p1"));
        accepts_in_both_editions(&[(
            "Main.tz",
            &cross_program(&format!("{declared}{safe}    n\n")),
        )]);
        for (safe_call, swapped_call) in [
            ("    chosen (ref p1)\n", "    chosen (ref p2)\n"),
            ("    ref p1 |> chosen\n", "    ref p2 |> chosen\n"),
            (
                "    let g = chosen\n    g (ref p1)\n",
                "    let g = chosen\n    g (ref p2)\n",
            ),
        ] {
            accepts_in_both_editions(&[(
                "Main.tz",
                &cross_program(&format!("{declared}{safe}{safe_call}    n\n")),
            )]);
            rejects_at(
                &cross_program(&format!("{declared}{swapped}{swapped_call}    n\n")),
                "E1013",
                "assignment_extends_borrow",
                (35, 5),
            );
        }
    }
    // A choice whose taken member stores the other way (n = 1 takes the lambda, which keeps a
    // borrow of `p2.s` in `p1`).
    for binding in [
        "    let chosen = match n with\n        | 0 -> cross\n        | _ -> (\\(a: ref PairOk) (b: ref PairOk) -> cross b a)\n",
        "    let chosen = if n < 0 then { cross } else { (\\(a: ref PairOk) (b: ref PairOk) -> cross b a) }\n",
    ] {
        rejects_at(
            &cross_program(&format!(
                "{declared}{binding}    chosen (ref p2) (ref p1)\n    n\n"
            )),
            "E1013",
            "assignment_extends_borrow",
            (35, 5),
        );
    }
    // Members of different arities: `three (ref p3)` and `cross` both take two more arguments.
    // With `p2`, `p1`, `p3` declared in that order, n = 1 runs `cross p1 p2`, and n < 0 would run
    // `three p3 p1 p2`; declared `p1`, `p2`, `p3`, `p1` keeps a borrow of `p2.s`; and with `p3`
    // declared first, `three p3 p1 p2` (the only member) keeps borrows of `p1.s` and `p2.s` in it.
    const THREE: &str = "    let three = \\(a: ref PairOk) (b: ref PairOk) (c: ref PairOk) -> { cross a b; cross a c }\n";
    let ordered = "    let p2 = mkpo ()\n    let p1 = mkpo ()\n    let p3 = mkpo ()\n";
    let swapped = "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n";
    let first = "    let p3 = mkpo ()\n    let p2 = mkpo ()\n    let p1 = mkpo ()\n";
    for call in [
        "    chosen (ref p1) (ref p2)\n",
        "    ref p2 |> chosen (ref p1)\n",
        "    let g = chosen (ref p1)\n    g (ref p2)\n",
    ] {
        for (choice, safe_order, unsafe_order) in [
            (
                "    let chosen = if n < 0 then three (ref p3) else cross\n",
                ordered,
                swapped,
            ),
            (
                "    let chosen = match n with\n        | 0 -> three (ref p3)\n        | _ -> cross\n",
                ordered,
                swapped,
            ),
            (
                "    let chosen = if n < 0 then three (ref p3) else three (ref p3)\n",
                ordered,
                first,
            ),
        ] {
            accepts_in_both_editions(&[(
                "Main.tz",
                &cross_program(&format!("{safe_order}{THREE}{choice}{call}    n\n")),
            )]);
            rejects_at(
                &cross_program(&format!("{unsafe_order}{THREE}{choice}{call}    n\n")),
                "E1013",
                "assignment_extends_borrow",
                (35, 5),
            );
        }
    }
}

/// A31, review round 2 after Copilot's review (finding 1): a builtin that applies a function value
/// it gets to fewer arguments than the value takes before its code runs makes a partial
/// application that binds what the builtin passes, and no site counted it: `Owned.call (ref w)
/// cell` with `w` wrapping the named function `wr` of two parameters (or an alias of it, or the
/// partial application `wr3 0`), its pipeline, `List.mapr wrr (ref cells)` and `List.foldr fold
/// zero (ref cells)` (edition 2026) gave a function value that owns or reaches the cell, through
/// which a closure that borrows a block-local string went into the cell. Main and base accept the
/// rejected forms and read the freed string (`-1`). Such a builtin application is now a site as a
/// call of an unseen function value is (`builtin_partial_applications`): with the string declared
/// first the programs print `0 + 8`, and with owned closures only `0 + 7`.
#[test]
fn a_builtin_that_applies_a_function_value_to_fewer_arguments_shares_the_cell() {
    const PRE: &str = r#"def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def wr :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64) -> i64
fn wr a f = Mutex.with_lock (Arc.get (ref a)) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () })
def wr3 :: i64 -> own Arc<Mutex<[unit -> i64]>> -> (unit -> i64) -> i64
fn wr3 k a f = Mutex.with_lock (Arc.get (ref a)) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k * 0 })
def wrr :: ref Arc<Mutex<[unit -> i64]>> -> (unit -> i64) -> i64
fn wrr a f = Mutex.with_lock (Arc.get a) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () })
def fold :: own ((unit -> i64) -> i64) -> ref Arc<Mutex<[unit -> i64]>> -> (unit -> i64) -> i64
fn fold prev a f = Mutex.with_lock (Arc.get a) (\slot -> { let old = slot[0]; Array.write 0 f slot; old () + prev (\() -> 0) * 0 })
def zero :: (unit -> i64) -> i64
fn zero _f = 0
def run :: i64 -> i64
fn run _n =
"#;
    const CELL: &str = "Arc.new (Mutex.create [\\() -> 0])";
    const END: &str = "    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    first + w2 (\\() -> 1) + junk.length * 0\ndo! IO.write_line (to_string (run 123456789))\n";
    let mapr = |piped: bool| {
        format!(
            "    let cells = [|{CELL}|]\n    let ws = {}\n    let mut w2 = match ws with\n        | [|g|] -> g\n        | _ -> \\_f -> 0\n",
            if piped {
                "ref cells |> List.mapr wrr"
            } else {
                "List.mapr wrr (ref cells)"
            }
        )
    };
    // (the function value, the partial application, the line of the error)
    let forms = [
        (
            "    let w = Owned.function wr\n".to_string(),
            format!("    let mut w2 = Owned.call (ref w) ({CELL})\n"),
            20,
        ),
        (
            "    let w = Owned.function wr\n".to_string(),
            format!("    let mut w2 = {CELL} |> Owned.call (ref w)\n"),
            20,
        ),
        (
            "    let named = wr\n    let w = Owned.function named\n".to_string(),
            format!("    let mut w2 = Owned.call (ref w) ({CELL})\n"),
            21,
        ),
        (
            "    let w = Owned.function (wr3 0)\n".to_string(),
            format!("    let mut w2 = Owned.call (ref w) ({CELL})\n"),
            20,
        ),
        (String::new(), mapr(false), 23),
        (String::new(), mapr(true), 23),
    ];
    let dangling = "    let first = { let s0 = String.repeat 4 (ref \"123456789\"); let text = ref s0; w2 (\\() -> look text) }\n";
    let declared_first = |made: &str, applied: &str| {
        format!(
            "{PRE}    let s0 = String.repeat 4 (ref \"123456789\")\n    let text = ref s0\n{made}{applied}    let first = w2 (\\() -> look text)\n{END}"
        )
    };
    let owned = |made: &str, applied: &str| {
        format!("{PRE}{made}{applied}    let first = w2 (\\() -> 7)\n{END}")
    };
    for (made, applied, line) in &forms {
        rejects_at(
            &format!("{PRE}{made}{applied}{dangling}{END}"),
            "E1013",
            "assignment_extends_borrow",
            (*line, 17),
        );
        accepts_in_both_editions(&[("Main.tz", &declared_first(made, applied))]);
        accepts_in_both_editions(&[("Main.tz", &owned(made, applied))]);
    }
    // `List.foldr` binds the state and a reference to the element (edition 2026, where `fold`'s
    // state parameter type-checks).
    let folded =
        format!("    let cells = [|{CELL}|]\n    let mut w2 = List.foldr fold zero (ref cells)\n");
    let source = format!("{PRE}{folded}{dangling}{END}");
    let error = analyze_with_edition(&source, Edition::E2026).expect_err(&source);
    assert_eq!(
        (error.code, error.kind, position(&source, error.span.start)),
        ("E1013", Some("assignment_extends_borrow"), (20, 17)),
        "{source}\n{}",
        error.message
    );
    for source in [declared_first("", &folded), owned("", &folded)] {
        analyze_with_edition(&source, Edition::E2026)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    }
    // A callback whose expression shows that it takes no more arguments than the builtin passes
    // runs its code there, and makes no partial application.
    let lambda = format!(
        "    let cells = [|{CELL}|]\n    let ws = List.mapr (\\(a: ref Arc<Mutex<[unit -> i64]>>) -> \\(f: unit -> i64) -> wrr a f) (ref cells)\n    let mut w2 = match ws with\n        | [|g|] -> g\n        | _ -> \\_f -> 0\n"
    );
    accepts_in_both_editions(&[("Main.tz", &declared_first("", &lambda))]);
}

/// A31, review round 2 after Copilot's review (finding 2): a site that makes a function value
/// taking two or more arguments before its code runs (`mk a = \k f -> ...`) made the types of its
/// partial applications owner types whether or not any partial application was ever made, so
/// `user`, which calls an unrelated `(unit -> i64) -> i64` read out of an array through a reference
/// with a closure that borrows its local, was rejected in a program that only applies `w` to all
/// its arguments (main, base and the previous tip accept it and print `300 + 11 + 6 = 317`). The
/// types now count only when the module may make such a partial application (`partial_results`):
/// a call, a pipeline, `Owned.call`, `new [T]`, `Array.init`, std's `Maybe.map`, a tuple's member.
/// With one of them, the closure goes into the cell and `user` is rejected.
#[test]
fn the_partial_application_types_of_an_owner_count_only_when_the_module_makes_one() {
    const MK: &str = "def mk :: own Arc<Mutex<[unit -> i64]>> -> (i64 -> (unit -> i64) -> i64)\nfn mk a = \\k f -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> { let old = slot[0]; Array.write 0 f slot; old () + k })\ndef apply_thunk :: (unit -> i64) -> i64\nfn apply_thunk f = f () + 1\n";
    // (edition, `user`, the full applications, the line of the error)
    let editions = [
        (
            Edition::E2026,
            "def user :: ref [(unit -> i64) -> i64] -> i64\nfn user fs =\n    let s = to_string 12345\n    let t = ref s\n    fs[0] (\\() -> String.length t)\n",
            "    let a = w 3 (\\() -> 7)\n    let b = w 4 (\\() -> 1)\n",
            (9, 5),
        ),
        (
            Edition::E2027,
            "def user :: ref [ref (unit -> i64) -> i64] -> i64\nfn user fs =\n    let s = to_string 12345\n    let t = ref s\n    let get = \\() -> String.length t\n    fs[0] get\n",
            "    let seven = \\() -> 7\n    let one = \\() -> 1\n    let a = w 3 seven\n    let b = w 4 one\n",
            (10, 5),
        ),
    ];
    for (edition, user, full, at) in editions {
        let program = |made: &str, fs: &str| {
            format!(
                "{MK}{user}def run :: i64 -> i64\nfn run _n =\n    let w = mk (Arc.new (Mutex.create [\\() -> 0]))\n{full}{made}    let fs = {fs}\n    a * 100 + b + user (ref fs)\ndo! IO.write_line (to_string (run 123456789))\n"
            )
        };
        // No partial application of `w`, nor of any value of its partial application's type.
        for made in [
            "",
            "    let p3 = \\(x: i64) (y: i64) (z: i64) -> x + y + z\n    let q = p3 1\n    let r = q 2 3\n",
        ] {
            let source = program(made, "[apply_thunk]");
            analyze_with_edition(&source, edition).unwrap_or_else(|error| {
                panic!(
                    "edition {edition}\n{source}\n{}: {}",
                    error.code, error.message
                )
            });
        }
        let new_array = if edition == Edition::E2027 {
            "new [ref (unit -> i64) -> i64](1, w)"
        } else {
            "new [(unit -> i64) -> i64](1, w)"
        };
        for (made, fs) in [
            ("    let w2 = w 3\n", "[w2]"),
            ("    let w2 = 3 |> w\n", "[w2]"),
            (
                "    let ow = Owned.function w\n    let w2 = Owned.call (ref ow) 3\n",
                "[w2]",
            ),
            ("", "Array.init 1 w"),
            ("", new_array),
            (
                "    let w2 = match Maybe.map w (Maybe.Some 3) with\n        | Maybe.Some g -> g\n        | Maybe.None -> apply_thunk\n",
                "[w2]",
            ),
            (
                "    let pair = (w, 3)\n    let w2 = match pair with\n        | (g, k) -> g k\n",
                "[w2]",
            ),
        ] {
            let source = program(made, fs);
            let error = analyze_with_edition(&source, edition).expect_err(&source);
            assert_eq!(
                (error.code, error.kind, position(&source, error.span.start)),
                ("E1013", Some("store_local_borrow_through_reference"), at),
                "edition {edition}\n{source}\n{}",
                error.message
            );
        }
    }
}

/// A31, review round 2 after Copilot's review (finding 3): the copy that a caller makes of its
/// argument at a path that stands for a larger part than the borrowed place (`Source::coarse`) is
/// not exact, so a body that passed it on to a function value recorded the whole parameter
/// (`self_path`), and the caller of a function that passes its parameters on to `applyy` or
/// `applyx` (`applyy2`, `applyy3` that calls `applyy2`, `applyyr` that reborrows, `applyyw` that
/// passes `ref w.x`, `applyx2`, `applyx3`) kept a borrow of all of `x`: writing or moving `x.z`
/// was rejected (the previous tip, main and base accept it and print `drop 8` and `1`). Such a
/// copy now holds the borrowed part (`holding_loans`), and the body records its place as a coarse
/// path (`holding_path`), as the direct call does. Moving or replacing `x.a.e` stays `E1014`.
#[test]
fn a_function_that_passes_its_parameters_on_keeps_the_coarse_part_of_its_callee() {
    let prelude = COARSE_PRELUDE.replace(
        "def run :: i64 -> i64\n",
        "def applyy2 :: i64 -> ref X -> (ref D -> unit) -> i64\nfn applyy2 k x f = applyy k x f\ndef applyy3 :: i64 -> ref X -> (ref D -> unit) -> i64\nfn applyy3 k x f = applyy2 k x f\ndef applyyr :: i64 -> ref X -> (ref D -> unit) -> i64\nfn applyyr k x f = applyy k (ref (deref x)) f\nrecord W { x: X }\ndef applyyw :: i64 -> ref W -> (ref D -> unit) -> i64\nfn applyyw k w f = applyy k (ref w.x) f\ndef applyx2 :: ref Y -> (ref D -> unit) -> i64\nfn applyx2 x f = applyx x f\ndef applyx3 :: ref Y -> (ref D -> unit) -> i64\nfn applyx3 x f = applyx2 x f\ndef run :: i64 -> i64\n",
    );
    let program = |body: &str| {
        format!(
            "{prelude}{body}    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    n + z + k * 0 + junk.length * 0\ndo! IO.write_line (to_string (run 1))\n"
        )
    };
    // (the declaration of the value, the call, the place of `x` in the value, the borrowed field)
    let forms = [
        (
            "let x = mkx ()",
            "applyy2 n (ref x) (crossd (ref p2))",
            "x",
            "x.a.e",
        ),
        (
            "let x = mkx ()",
            "applyy3 n (ref x) (crossd (ref p2))",
            "x",
            "x.a.e",
        ),
        (
            "let x = mkx ()",
            "applyyr n (ref x) (crossd (ref p2))",
            "x",
            "x.a.e",
        ),
        (
            "let x = mkx ()",
            "crossd (ref p2) |> applyy2 n (ref x)",
            "x",
            "x.a.e",
        ),
        (
            "let w = W { x: mkx () }",
            "applyyw n (ref w) (crossd (ref p2))",
            "w.x",
            "w.x.a.e",
        ),
        (
            "let x = mky ()",
            "applyx2 (ref x) (crossd (ref p2))",
            "x",
            "x.a.b.c.d.e",
        ),
        (
            "let x = mky ()",
            "applyx3 (ref x) (crossd (ref p2))",
            "x",
            "x.a.b.c.d.e",
        ),
    ];
    for (declared, call, value, borrowed) in forms {
        let setup = format!("    {declared}\n    let p2 = mkpo ()\n    let z = {call}\n");
        // A move of the borrowed field conflicts with the borrow in both editions, and so does its
        // replacement in a mutable local (edition 2027).
        rejects_at(
            &program(&format!(
                "{setup}    let k = {{ let taken = {borrowed}; taken.length }}\n"
            )),
            "E1014",
            "borrow_conflict",
            (49, 27),
        );
        let mutable = setup
            .replace("let x = ", "let mut x = ")
            .replace("let w = ", "let mut w = ");
        let source = program(&format!("{mutable}    {borrowed} = []\n    let k = 0\n"));
        let error = analyze_with_edition(&source, Edition::E2027).expect_err(&source);
        assert_eq!(
            (error.code, error.kind, position(&source, error.span.start)),
            ("E1014", Some("borrow_conflict"), (49, 5)),
            "{source}\n{}",
            error.message
        );
        // Moving `z` out of the value, or nothing, conflicts with nothing in both editions, and
        // writing `z` neither in edition 2027 (main prints `drop 8` and `1`).
        accepts_in_both_editions(&[(
            "Main.tz",
            &program(&format!(
                "{setup}    let k = {{ let taken = {value}.z; String.length (ref taken) }}\n"
            )),
        )]);
        accepts_in_both_editions(&[("Main.tz", &program(&format!("{setup}    let k = 0\n")))]);
        let source = program(&format!(
            "{mutable}    {value}.z = to_string 22\n    let k = 0\n"
        ));
        analyze_with_edition(&source, Edition::E2027)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    }
}

/// A31, review round 3 after Copilot (findings 1 and 2): a function that passes its parameters
/// on through a partial application (`applyyq k x f = { let g = applyy k x; g f }`, and the same
/// with a reborrow) or a lambda that captures them (`applyyc k x f = (\(_u: unit) -> applyy k x
/// f) ()`, a lambda bound and then called) kept no part: the caller borrowed all of `x`, so
/// writing or moving `x.z` was `E1014`, and a lambda over a function-valued parameter even kept
/// `crossd (ref p2)`'s borrow in `p2` (`E1013`). The previous tips reject them; main and base
/// accept them and print `drop 8` and `1`. The argument that a partial application bound was
/// narrowed (`bound_value`) only when it was exact in the state where the function value runs, in
/// which the parameter that it read is no longer live and holds nothing; the evaluation of the
/// borrow now records whether it was exact (`inexact_borrows`). The captures of a lambda were not
/// narrowed at all; they are now, by what each captured local held where the lambda was made
/// (`captured_loans`). A lambda that captures a function-valued parameter in edition 2026 owns
/// it, and its drop counts as code that may store what it holds into that function value, so
/// those forms stay `E1013` there (drop.md, A33 `fvlam`). The body of `run` starts at line 45.
#[test]
fn a_partial_application_or_a_lambda_that_passes_parameters_on_keeps_the_part() {
    let prelude = COARSE_PRELUDE.replace(
        "def run :: i64 -> i64\n",
        "def applyp :: ref X -> (ref D -> unit) -> i64\nfn applyp x f = { ref (deref (Rc.get (ref x.a.e[0]))) |> f; 0 }\ndef applyyq :: i64 -> ref X -> (ref D -> unit) -> i64\nfn applyyq k x f = { let g = applyy k x; g f }\ndef applypq :: ref X -> (ref D -> unit) -> i64\nfn applypq x f = { let g = applyp x; g f }\ndef applypr :: ref X -> (ref D -> unit) -> i64\nfn applypr x f = { let g = applyp (ref (deref x)); g f }\ndef applyyc :: i64 -> ref X -> (ref D -> unit) -> i64\nfn applyyc k x f = (\\(_u: unit) -> applyy k x f) ()\ndef applypn :: ref X -> (ref D -> unit) -> i64\nfn applypn x f = { let g = \\(_u: unit) -> { ref (deref (Rc.get (ref x.a.e[0]))) |> f; 0 }; g () }\ndef run :: i64 -> i64\n",
    );
    let program = |body: &str| {
        format!(
            "{prelude}{body}    let junk = [String.repeat 4 (ref \"zzzzzzzzz\"), String.repeat 4 (ref \"yyyyyyyyy\")]\n    n + z + k * 0 + junk.length * 0\ndo! IO.write_line (to_string (run 1))\n"
        )
    };
    // (the call, whether edition 2026 keeps it whole: a lambda that owns the function value)
    let forms = [
        ("applyyq n (ref x) (crossd (ref p2))", false),
        ("applypq (ref x) (crossd (ref p2))", false),
        ("applypr (ref x) (crossd (ref p2))", false),
        ("applyyc n (ref x) (crossd (ref p2))", true),
        ("applypn (ref x) (crossd (ref p2))", true),
    ];
    for (call, owned) in forms {
        let setup = format!("    let x = mkx ()\n    let p2 = mkpo ()\n    let z = {call}\n");
        let mutable = setup.replace("let x = ", "let mut x = ");
        let moved = program(&format!(
            "{setup}    let k = {{ let taken = x.a.e; taken.length }}\n"
        ));
        let replaced = program(&format!("{mutable}    x.a.e = []\n    let k = 0\n"));
        let outside = program(&format!(
            "{setup}    let k = {{ let taken = x.z; String.length (ref taken) }}\n"
        ));
        let nothing = program(&format!("{setup}    let k = 0\n"));
        let written = program(&format!("{mutable}    x.z = to_string 22\n    let k = 0\n"));
        // A move or a replacement of the borrowed field conflicts with the borrow (edition 2027
        // for the replacement in a mutable local).
        let error = analyze_with_edition(&replaced, Edition::E2027).expect_err(&replaced);
        assert_eq!(
            (
                error.code,
                error.kind,
                position(&replaced, error.span.start)
            ),
            ("E1014", Some("borrow_conflict"), (48, 5)),
            "{replaced}\n{}",
            error.message
        );
        analyze_with_edition(&written, Edition::E2027)
            .unwrap_or_else(|error| panic!("{written}\n{}: {}", error.code, error.message));
        if owned {
            // Edition 2027 keeps the part; edition 2026 keeps `p2`'s borrow in itself.
            let error = analyze_with_edition(&moved, Edition::E2027).expect_err(&moved);
            assert_eq!(
                (error.code, error.kind, position(&moved, error.span.start)),
                ("E1014", Some("borrow_conflict"), (48, 27)),
                "{moved}\n{}",
                error.message
            );
            for source in [&outside, &nothing] {
                analyze_with_edition(source, Edition::E2027)
                    .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
                let error = analyze_with_edition(source, Edition::E2026).expect_err(source);
                assert_eq!(
                    (error.code, error.kind, position(source, error.span.start)),
                    ("E1013", Some("self_referential_store"), (45, 5)),
                    "{source}\n{}",
                    error.message
                );
            }
        } else {
            rejects_at(&moved, "E1014", "borrow_conflict", (48, 27));
            accepts_in_both_editions(&[("Main.tz", &outside)]);
            accepts_in_both_editions(&[("Main.tz", &nothing)]);
        }
    }
}

/// A31, review round 3 after Copilot (finding 2, and round 2's `bindref`): a lambda that captures
/// two reference parameters (`outerl a b = (\(_u: unit) -> cross a b) ()`, or bound and then
/// called) gave each capture the loans of both, so `outerl (ref p2) (ref p1)` was taken to keep
/// `p1`'s borrow in `p1` too (`E1013`), and so did a partial application that binds parameters
/// (`via3 a b c = { let g = cross3 a b; g c }`). Main and base accept them: `p2` reads `p1.s`
/// when it is dropped first (`drop 8`, `drop 0`, `1`), and `via3 (ref p2) (ref p3) (ref p1)`
/// keeps `p1.s` in `p2` and `p3` (`drop 8`, `drop 8`, `drop 0`, `1`). The reversed calls keep a
/// borrow of a value dropped first and stay `E1013` (`assignment_extends_borrow`), and a capture
/// that may be either parameter (`let q = if k > 0 then a else b`) keeps a borrow in both. The
/// body of `run` starts at line 45.
#[test]
fn a_lambda_or_a_partial_application_over_reference_parameters_keeps_each_part() {
    let prelude = CROSS_PRELUDE.replace(
        "def run :: i64 -> i64\n",
        "def cross3 :: ref PairOk -> ref PairOk -> ref PairOk -> unit\nfn cross3 a b c = { cross a c; cross b c }\ndef outerl :: ref PairOk -> ref PairOk -> unit\nfn outerl a b = (\\(_u: unit) -> cross a b) ()\ndef outerg :: ref PairOk -> ref PairOk -> unit\nfn outerg a b = { let g = \\(_u: unit) -> cross a b; g () }\ndef via3 :: ref PairOk -> ref PairOk -> ref PairOk -> unit\nfn via3 a b c = { let g = cross3 a b; g c }\ndef mixl :: i64 -> ref PairOk -> ref PairOk -> unit\nfn mixl k a b = { let q = if k > 0 then a else b; let g = \\(_u: unit) -> cross q b; g () }\ndef run :: i64 -> i64\n",
    );
    let program =
        |body: &str| format!("{prelude}{body}    n\ndo! IO.write_line (to_string (run 1))\n");
    let two = "    let p1 = mkpo ()\n    let p2 = mkpo ()\n";
    let three = "    let p1 = mkpo ()\n    let p2 = mkpo ()\n    let p3 = mkpo ()\n";
    for safe in [
        format!("{two}    outerl (ref p2) (ref p1)\n"),
        format!("{two}    outerg (ref p2) (ref p1)\n"),
        format!("{three}    via3 (ref p2) (ref p3) (ref p1)\n"),
        format!("{two}    mixl 1 (ref p2) (ref p1)\n"),
    ] {
        accepts_in_both_editions(&[("Main.tz", &program(&safe))]);
    }
    // `p1` keeps a borrow of `p2` (`p3` of `p2` for `via3`), which is dropped before it.
    for (unsafe_call, at) in [
        (format!("{two}    outerl (ref p1) (ref p2)\n"), (45, 5)),
        (format!("{two}    outerg (ref p1) (ref p2)\n"), (45, 5)),
        (
            format!("{three}    via3 (ref p1) (ref p3) (ref p2)\n"),
            (45, 5),
        ),
        (format!("{two}    mixl 1 (ref p1) (ref p2)\n"), (45, 5)),
    ] {
        rejects_at(
            &program(&unsafe_call),
            "E1013",
            "assignment_extends_borrow",
            at,
        );
    }
}

/// A31, review round 4 after Copilot (findings 1 and 2): a store into the block of an `Rc` owner
/// reaches the other owners of the block. `let r = Rc.share (ref r2)` holds a share of `r2`'s block
/// (`Loan::share`), which a store into `r` (through `let a = Rc.get (ref r)`) followed to `r2`.
/// Once `r` is no longer used, its value stands for what the block holds (`keep_for_drop`), and
/// the store reached `r` alone: `r2`, declared before `p`, frees the block after `p`, and the
/// block's holder reads `p.s` after it was freed (main prints `drop -1`, ASan reports a
/// heap-use-after-free with the string in a heap block). The narrowed capture of a lambda (review
/// round 3) took the same way. Every form keeps the borrow in `r2` now (`E1013`,
/// `assignment_extends_borrow`), and so does the block moved into an older local after the store
/// (`hold = r2`). A store of the block's own `s` into its `h` through another owner of the block
/// is a store into the block itself: `PairBad` drops `s` before `h` (`self_referential_store`),
/// `PairOk` drops `h` first. With `p` declared first, `p` outlives the block (`drop 8`, `drop 0`,
/// `1`). The body of `run` starts at line 42.
#[test]
fn a_store_through_one_owner_of_a_block_reaches_the_other_owners() {
    let prelude = CROSS_PRELUDE.replace(
        "def run :: i64 -> i64\n",
        "record RH { rc: Rc<PairOk> }\ndef outerg :: ref PairOk -> ref PairOk -> unit\nfn outerg a b = { let g = \\(_u: unit) -> cross a b; g () }\ndef outerl :: ref PairOk -> ref PairOk -> unit\nfn outerl a b = (\\(_u: unit) -> cross a b) ()\ndef outerq :: ref PairOk -> ref PairOk -> unit\nfn outerq a b = { let g = cross a; g b }\ndef run :: i64 -> i64\n",
    );
    let program =
        |body: &str| format!("{prelude}{body}    n\ndo! IO.write_line (to_string (run 1))\n");
    let lambda = "    let b = ref p\n    let g = \\(_u: unit) -> cross a b\n    g ()\n";
    let direct = "    cross a (ref p)\n";
    let partial = "    let g = cross a\n    g (ref p)\n";
    let mut forms: Vec<String> = Vec::new();
    for using in [
        lambda,
        "    let b = ref p\n    (\\(_u: unit) -> cross a b) ()\n",
        direct,
        "    let t = ref p.s\n    Mutex.with_lock (ref a.h.cell) (\\c -> Array.write 0 (\\() -> look t) c)\n",
        partial,
        "    ref p |> cross a\n",
        "    outer a (ref p)\n",
        "    outerg a (ref p)\n",
        "    outerl a (ref p)\n",
        "    outerq a (ref p)\n",
    ] {
        forms.push(format!(
            "    let r = Rc.share (ref r2)\n    let a = Rc.get (ref r)\n{using}"
        ));
    }
    for (share, get) in [
        ("    let r = ref r2 |> Rc.share\n", "Rc.get (ref r)"),
        (
            "    let h = RH { rc: Rc.share (ref r2) }\n",
            "Rc.get (ref h.rc)",
        ),
        ("    let rs = [Rc.share (ref r2)]\n", "Rc.get (ref rs[0])"),
    ] {
        for using in [lambda, direct, partial] {
            forms.push(format!("{share}    let a = {get}\n{using}"));
        }
    }
    for form in &forms {
        let safe = format!("    let p = mkpo ()\n    let r2 = Rc.new (mkpo ())\n{form}");
        accepts_in_both_editions(&[("Main.tz", &program(&safe))]);
        let unsafe_order = format!("    let r2 = Rc.new (mkpo ())\n    let p = mkpo ()\n{form}");
        rejects_at(
            &program(&unsafe_order),
            "E1013",
            "assignment_extends_borrow",
            (42, 5),
        );
    }
    // The block moved into `hold`, declared before `p`, after the store.
    let moved = "    let r2 = Rc.new (mkpo ())\n    let r = Rc.share (ref r2)\n    let a = Rc.get (ref r)\n    cross a (ref p)\n    hold = r2\n";
    accepts_in_both_editions(&[(
        "Main.tz",
        &program(&format!(
            "    let p = mkpo ()\n    let mut hold = Rc.new (mkpo ())\n{moved}"
        )),
    )]);
    rejects_at(
        &program(&format!(
            "    let mut hold = Rc.new (mkpo ())\n    let p = mkpo ()\n{moved}"
        )),
        "E1013",
        "assignment_extends_borrow",
        (42, 5),
    );
    // The block's own part, through another owner of the block.
    for (share, get) in [
        ("    let r2 = ref r |> Rc.share\n", "Rc.get (ref r2)"),
        ("    let rs = [Rc.share (ref r)]\n", "Rc.get (ref rs[0])"),
    ] {
        for using in [
            "    crossX a b\n",
            "    let g = \\(_u: unit) -> crossX a b\n    g ()\n",
            "    let g = crossX a\n    g b\n",
        ] {
            let body = |make: &str, writer: &str| {
                format!(
                    "    let r = Rc.new ({make} ())\n{share}    let a = {get}\n    let b = Rc.get (ref r)\n{}",
                    using.replace("crossX", writer)
                )
            };
            accepts_in_both_editions(&[("Main.tz", &program(&body("mkpo", "cross")))]);
            rejects_at(
                &program(&body("mkpb", "crossb")),
                "E1013",
                "self_referential_store",
                (42, 5),
            );
        }
    }
}

/// A31: the store into a value whose share of another owner's block `keep_for_drop` expanded
/// reaches that owner by the types of the blocks that both may hold (the shared block and the
/// blocks inside it), not by every block type of the value's type: a store into an unrelated
/// `Rc<Other>` after the share's last use does not reach `r2` (review round 5 after Copilot),
/// while a store into a block that the other owner may free does (`o2`, which shares the `Other`
/// block, and `r2`, whose tree holds the leaf).
#[test]
fn a_store_through_an_expanded_share_reaches_the_owners_that_may_free_the_block() {
    // 15 lines before `def run`, which is then line 48: the body starts at line 50.
    let prelude = CROSS_PRELUDE.replace(
        "def run :: i64 -> i64\n",
        r#"record Other { h: Holder, n: i64 }
def mkother :: i64 -> Other
fn mkother k = Other { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossother :: ref Other -> ref PairOk -> unit
fn crossother w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record RH2 { rc: Rc<PairOk>, other: Rc<Other> }
record Leaf { h: Holder, n: i64 }
def mkleaf :: i64 -> Leaf
fn mkleaf k = Leaf { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossleaf :: ref Leaf -> ref PairOk -> unit
fn crossleaf w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Tree2 { h: Holder, s: string, child: Rc<Leaf> }
record W { h: Holder, keep: Maybe<Rc<PairOk>> }
def crossw2 :: ref W -> ref PairOk -> unit
fn crossw2 w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
"#,
    );
    let program = |body: &str| format!("{prelude}{body}do! IO.write_line (to_string (run 1))\n");
    // The store goes into an `Rc<Other>` that no owner of `r2`'s block holds: `r2` cannot free
    // it, whether `p` is declared before or after `h`.
    accepts_in_both_editions(&[(
        "Main.tz",
        &program(
            "    let r2 = Rc.new (mkpo ())\n    let p = mkpo ()\n    let h = RH2 { rc: Rc.share (ref r2), other: Rc.new (mkother 1) }\n    let a = Rc.get (ref h.rc)\n    let o = Rc.new (mkother 2)\n    crossother (Rc.get (ref o)) (ref p)\n    look (ref a.s) * 0 + n\n",
        ),
    )]);
    accepts_in_both_editions(&[(
        "Main.tz",
        &program(
            "    let r2 = Rc.new (mkpo ())\n    let h = RH2 { rc: Rc.share (ref r2), other: Rc.new (mkother 1) }\n    let a = Rc.get (ref h.rc)\n    let x = look (ref a.s)\n    let p = mkpo ()\n    let o = Rc.new (mkother 2)\n    crossother (Rc.get (ref o)) (ref p)\n    x * 0 + n\n",
        ),
    )]);
    // The store goes into the `Other` block that `h` shares with `o2`, declared before `p`.
    let two = |order: &str| {
        program(&format!(
            "{order}    let r2 = Rc.new (mkpo ())\n    let h = RH2 {{ rc: Rc.share (ref r2), other: Rc.share (ref o2) }}\n    let a = Rc.get (ref h.other)\n    crossother a (ref p)\n    n\n"
        ))
    };
    accepts_in_both_editions(&[(
        "Main.tz",
        &two("    let p = mkpo ()\n    let o2 = Rc.new (mkother 1)\n"),
    )]);
    rejects_at(
        &two("    let o2 = Rc.new (mkother 1)\n    let p = mkpo ()\n"),
        "E1013",
        "assignment_extends_borrow",
        (50, 5),
    );
    // The store goes into the leaf that `r2`'s tree holds, through a share of the leaf made from
    // the share `r` of the tree.
    let leaf = |order: &str| {
        let tree = "    let r2 = Rc.new (Tree2 { h: Holder { cell: Mutex.create [\\() -> 0] }, s: \"ab\", child: Rc.new (mkleaf 1) })\n";
        program(&format!(
            "{}    let r = Rc.share (ref r2)\n    let t = Rc.get (ref r)\n    let l = Rc.share (ref t.child)\n    let a = Rc.get (ref l)\n    crossleaf a (ref p)\n    n\n",
            order.replace("TREE", tree)
        ))
    };
    accepts_in_both_editions(&[("Main.tz", &leaf("    let p = mkpo ()\nTREE"))]);
    rejects_at(
        &leaf("TREE    let p = mkpo ()\n"),
        "E1013",
        "assignment_extends_borrow",
        (50, 5),
    );
    // A31 limit (`sharetype`, A34): a store into a value whose type reaches the shared block's
    // type goes to `r2` although no block is involved (`w.keep` is `Maybe.None`).
    rejects_at(
        &program(
            "    let r2 = Rc.new (mkpo ())\n    let p = mkpo ()\n    let r = Rc.share (ref r2)\n    let a = Rc.get (ref r)\n    let w = W { h: Holder { cell: Mutex.create [\\() -> 0] }, keep: Maybe.None }\n    crossw2 (ref w) (ref p)\n    look (ref a.s) * 0 + n\n",
        ),
        "E1013",
        "assignment_extends_borrow",
        (50, 5),
    );
}

/// A31, review round 6 after Copilot: a dyn value's method can hand out a place inside the dyn's
/// data (`fn leaf b = ref b.l`), which the walk by types does not see. A share of a block that a
/// dyn value of `r2` owns (`Rc.share (Cls.LeafOf.leaf (ref (Rc.get (ref r2)).d))`) is a share of a
/// block that `r2`'s drop may free, although `r2`'s type does not show it, so a store into it
/// after the share's last use reaches `r2` (the round-5 narrowing of `export_shared_block`
/// accepted it; `drop -1` is a read of freed memory). The same holds when the holder keeps the
/// share inside a dyn value (`H { d: Dyn.of (Box1 { l: Rc.share (ref r2) }) }`), whose type shows
/// no block, and when the store goes into a block inside the dyn of the shared block (pre-existing:
/// main accepts both). With the owner of the string declared before the owner of the block, each
/// program is accepted.
#[test]
fn a_store_through_a_share_behind_a_dyn_value_reaches_the_owner_that_frees_the_block() {
    let defs = r#"record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record Leaf { h: Holder, n: i64 }
def mkleaf :: i64 -> Leaf
fn mkleaf k = Leaf { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def crossleaf :: ref Leaf -> ref PairOk -> unit
fn crossleaf w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
"#;
    let cls = "class LeafOf<'value> {\n    def leaf :: ref 'value -> ref Rc<Defs.Leaf>\n}\n";
    let arc_cls = "class LeafOf<'value> {\n    def leaf :: ref 'value -> ref Arc<Defs.Leaf>\n}\n";
    // Lines 1 to 7 come before the body, which starts at line 8.
    let main = |pointer: &str, record: &str, body: &str| {
        format!(
            "record Box1 {{ l: {pointer}<Defs.Leaf> }}\ninstance Cls.LeafOf<Box1> {{\n    fn leaf b = ref b.l\n}}\n{record}\ndef run :: i64 -> i64\nfn run n =\n{body}do! IO.write_line (to_string (run 1))\n"
        )
    };
    let with_cell = "record B {r} { h: Defs.Holder, d: dyn Cls.LeafOf {r} }";
    let no_cell = "record B {r} { n: i64, d: dyn Cls.LeafOf {r} }";
    let holder = "record H {r} { d: dyn Cls.LeafOf {r} }";
    let plain = "record S {r} { h: Defs.Holder, d: dyn Cls.LeafOf {r} }";
    let b_cell = "    let r2 = Rc.new (B { h: Defs.Holder { cell: Mutex.create [\\() -> 0] }, d: Dyn.of (Box1 { l: Rc.new (Defs.mkleaf 1) }) })\n";
    let b_none =
        "    let r2 = Rc.new (B { n: 0, d: Dyn.of (Box1 { l: Rc.new (Defs.mkleaf 1) }) })\n";
    let b_arc = "    let r2 = Arc.new (B { h: Defs.Holder { cell: Mutex.create [\\() -> 0] }, d: Dyn.of (Box1 { l: Arc.new (Defs.mkleaf 1) }) })\n";
    let s_plain = "    let r2 = S { h: Defs.Holder { cell: Mutex.create [\\() -> 0] }, d: Dyn.of (Box1 { l: Rc.new (Defs.mkleaf 1) }) }\n";
    let leaf = "    let r2 = Rc.new (Defs.mkleaf 1)\n";
    let p = "    let rp = Rc.new (Defs.mkpo ())\n";
    let store = "    Defs.crossleaf a (Rc.get (ref rp))\n    n\n";
    // (pointer, record, the owner of the block, the rest of the body before the store)
    let cases = [
        // d1: the share of the leaf that `r2`'s dyn field owns
        (
            "Rc",
            with_cell,
            b_cell,
            "    let h = Rc.share (Cls.LeafOf.leaf (ref (Rc.get (ref r2)).d))\n    let a = Rc.get (ref h)\n",
        ),
        // d2_nocell: the same, `B` without a cell
        (
            "Rc",
            no_cell,
            b_none,
            "    let h = Rc.share (Cls.LeafOf.leaf (ref (Rc.get (ref r2)).d))\n    let a = Rc.get (ref h)\n",
        ),
        // d6_plain: a record owner in place of an `Rc`
        (
            "Rc",
            plain,
            s_plain,
            "    let h = Rc.share (Cls.LeafOf.leaf (ref r2.d))\n    let a = Rc.get (ref h)\n",
        ),
        // d7_arc
        (
            "Arc",
            with_cell,
            b_arc,
            "    let h = Arc.share (Cls.LeafOf.leaf (ref (Arc.get (ref r2)).d))\n    let a = Arc.get (ref h)\n",
        ),
        // the share of `r2`'s whole block, and the store into the leaf in its dyn field
        (
            "Rc",
            no_cell,
            b_none,
            "    let h = Rc.share (ref r2)\n    let a = Rc.get (Cls.LeafOf.leaf (ref (Rc.get (ref h)).d))\n",
        ),
        // the holder keeps the share in a dyn value
        (
            "Rc",
            holder,
            leaf,
            "    let h = H { d: Dyn.of (Box1 { l: Rc.share (ref r2) }) }\n    let a = Rc.get (Cls.LeafOf.leaf (ref h.d))\n",
        ),
        // the holder keeps, in a dyn value, a share of a share of `r2`
        (
            "Rc",
            holder,
            leaf,
            "    let h = Rc.share (ref r2)\n    let g = H { d: Dyn.of (Box1 { l: Rc.share (ref h) }) }\n    let a = Rc.get (Cls.LeafOf.leaf (ref g.d))\n",
        ),
    ];
    for (pointer, record, owner, rest) in cases {
        let class = if pointer == "Arc" { arc_cls } else { cls };
        let unsafe_main = main(pointer, record, &format!("{owner}{p}{rest}{store}"));
        for edition in [Edition::E2026, Edition::E2027] {
            let modules = [
                ("Defs.tz", defs),
                ("Cls.tt", class),
                ("Main.tz", unsafe_main.as_str()),
            ];
            let error = analyze_modules_with_edition(&modules, edition).expect_err(&unsafe_main);
            assert_eq!(
                (
                    error.code,
                    error.kind,
                    position(&unsafe_main, error.span.start)
                ),
                ("E1013", Some("assignment_extends_borrow"), (8, 5)),
                "edition {edition}\n{unsafe_main}\n{}",
                error.message
            );
        }
        let safe_main = main(pointer, record, &format!("{p}{owner}{rest}{store}"));
        accepts_in_both_editions(&[
            ("Defs.tz", defs),
            ("Cls.tt", class),
            ("Main.tz", &safe_main),
        ]);
    }
    // The share of `r2`'s block also went to the caller's array, which keeps the leaf after the
    // call returns: the store through the holder's dyn value is a store into the caller's data.
    let put = |order: &str| {
        format!(
            "record Box1 {{ l: Rc<Defs.Leaf> }}\ninstance Cls.LeafOf<Box1> {{\n    fn leaf b = ref b.l\n}}\n{holder}\ndef put :: ref mut [Rc<Defs.Leaf>] -> i64 -> i64\nfn put out n =\n{order}    let g = H {{ d: Dyn.of (Box1 {{ l: Rc.share (ref r2) }}) }}\n    let a = Rc.get (Cls.LeafOf.leaf (ref g.d))\n{store}def run :: i64 -> i64\nfn run n =\n    let mut v = [Rc.new (Defs.mkleaf 0)]\n    put (ref mut v) n\ndo! IO.write_line (to_string (run 1))\n"
        )
    };
    let shared = "    Array.write 0 (Rc.share (ref r2)) out\n";
    for (order, expected) in [
        (
            format!("{leaf}{shared}{p}"),
            ("E1013", Some("assignment_extends_borrow"), (8, 5)),
        ),
        (
            format!("{p}{leaf}{shared}"),
            (
                "E1013",
                Some("store_local_borrow_through_reference"),
                (13, 5),
            ),
        ),
    ] {
        let program = put(&order);
        for edition in [Edition::E2026, Edition::E2027] {
            let modules = [
                ("Defs.tz", defs),
                ("Cls.tt", cls),
                ("Main.tz", program.as_str()),
            ];
            let error = analyze_modules_with_edition(&modules, edition).expect_err(&program);
            assert_eq!(
                (error.code, error.kind, position(&program, error.span.start)),
                expected,
                "edition {edition}\n{program}\n{}",
                error.message
            );
        }
    }
    // The source owner still moves after the store through a holder that keeps the share in a dyn
    // value (`keep_for_drop` expands the share).
    let moved = main(
        "Rc",
        holder,
        &format!(
            "{p}{leaf}    let h = H {{ d: Dyn.of (Box1 {{ l: Rc.share (ref r2) }}) }}\n    let a = Rc.get (Cls.LeafOf.leaf (ref h.d))\n    Defs.crossleaf a (Rc.get (ref rp))\n    let _r3 = r2\n    n\n"
        ),
    );
    accepts_in_both_editions(&[("Defs.tz", defs), ("Cls.tt", cls), ("Main.tz", &moved)]);
    // The other direction (pre-existing: main accepts both): `hd` shares `r2`'s block, whose type
    // shows no cell (`B` without one), and the store goes into the leaf in its dyn field through
    // `r2` after `hd`'s last use; and the block whose share went to the caller's array. Both
    // owners hold a block that may hold a cell behind the dyn value (`export_block`).
    let reverse = |order: &str| {
        main(
            "Rc",
            no_cell,
            &format!(
                "{order}    let lf = Rc.share (Cls.LeafOf.leaf (ref (Rc.get (ref r2)).d))\n    Defs.crossleaf (Rc.get (ref lf)) (Rc.get (ref rp))\n    let _r3 = r2\n    n\n"
            ),
        )
    };
    let hd = "    let hd = Rc.share (ref r2)\n";
    let unsafe_reverse = reverse(&format!("{b_none}{hd}{p}"));
    for edition in [Edition::E2026, Edition::E2027] {
        let modules = [
            ("Defs.tz", defs),
            ("Cls.tt", cls),
            ("Main.tz", unsafe_reverse.as_str()),
        ];
        let error = analyze_modules_with_edition(&modules, edition).expect_err(&unsafe_reverse);
        assert_eq!(
            (
                error.code,
                error.kind,
                position(&unsafe_reverse, error.span.start)
            ),
            ("E1013", Some("assignment_extends_borrow"), (8, 5)),
            "edition {edition}\n{unsafe_reverse}\n{}",
            error.message
        );
    }
    let safe_reverse = reverse(&format!("{p}{b_none}{hd}"));
    accepts_in_both_editions(&[
        ("Defs.tz", defs),
        ("Cls.tt", cls),
        ("Main.tz", &safe_reverse),
    ]);
    // Lines 1 to 7 come before the body of `put`, whose store is at line 12.
    let to_caller = format!(
        "record Box1 {{ l: Rc<Defs.Leaf> }}\ninstance Cls.LeafOf<Box1> {{\n    fn leaf b = ref b.l\n}}\n{no_cell}\ndef put :: ref mut [Rc<B>] -> i64 -> i64\nfn put out n =\n{p}{b_none}    Array.write 0 (Rc.share (ref r2)) out\n    let lf = Rc.share (Cls.LeafOf.leaf (ref (Rc.get (ref r2)).d))\n    Defs.crossleaf (Rc.get (ref lf)) (Rc.get (ref rp))\n    n\ndef run :: i64 -> i64\nfn run n =\n    let mut v = [Rc.new (B {{ n: 1, d: Dyn.of (Box1 {{ l: Rc.new (Defs.mkleaf 2) }}) }})]\n    put (ref mut v) n\ndo! IO.write_line (to_string (run 1))\n"
    );
    for edition in [Edition::E2026, Edition::E2027] {
        let modules = [
            ("Defs.tz", defs),
            ("Cls.tt", cls),
            ("Main.tz", to_caller.as_str()),
        ];
        let error = analyze_modules_with_edition(&modules, edition).expect_err(&to_caller);
        assert_eq!(
            (
                error.code,
                error.kind,
                position(&to_caller, error.span.start)
            ),
            (
                "E1013",
                Some("store_local_borrow_through_reference"),
                (12, 5)
            ),
            "edition {edition}\n{to_caller}\n{}",
            error.message
        );
    }
}

/// A31, Copilot's review 2 of PR #105: the fixed point of a loop compares a summary of the state
/// where an iteration starts, and the check goes on from the exit state of the iteration whose
/// summary did not change, which starts from what the iterations before it left. The summary had
/// neither `State::contents_elsewhere` nor `State::kept_elsewhere`: a body that only copied what
/// the cell of `p`'s target holds to `out`'s target, or stored the borrow of `p.a[0]` that the cell
/// held already in `out`'s cell, changed no fact of it, so the loop ended after one iteration with
/// the state from before the body, and `deref p = mkpa ()` after the loop ended the borrow that the
/// copy keeps (the caller then read the freed string through `out`: `drop -1 2` and `-1` in place
/// of `drop 8 2` and `8`, a heap use after free under ASan). Each loop form is rejected where the
/// replacement is, as the program without a loop is; with the replacement before the loop, the
/// copies borrow the new value and the program is accepted. The flags of a loan were not in the
/// summary either: `x` held what it read through `p` (`Value::through`) before a loop that binds
/// it to a lambda that captures `p` itself, so the store of `x` into `out` after the loop was a
/// copy of `p`'s contents and not a borrow of `p`'s target (`drop -1 2` when the caller's `out`
/// outlives `p1`); it is rejected at the call, as without a loop.
#[test]
fn a_loop_keeps_what_its_body_stored_elsewhere() {
    const PRELUDE: &str = r#"record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = {
        let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy")];
        do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())) + " " + to_string junk.length)
    }
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def mkh :: unit -> Holder
fn mkh _u = Holder { cell: Mutex.create [\() -> 0] }
def mks :: unit -> string
fn mks _u = String.repeat 4 (ref "123456789")
record PA { h: Holder, a: [string] }
def mkpa :: unit -> PA
fn mkpa _u = PA { h: mkh (), a: [mks ()] }
def copy_out :: ref [unit -> i64] -> ref Mutex<[unit -> i64]> -> unit
fn copy_out b out = Mutex.with_lock out (\o -> Array.write 0 b[0] o)
def renew :: ref mut PA -> ref Holder -> i64 -> i64
fn renew p out n =
"#;
    const RUN: &str = r#"    0
def run :: i64 -> i64
fn run n =
    let mut p0 = mkpa ()
    let out = mkh ()
    let k = renew (ref mut p0) (ref out) n
    let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy"), String.repeat 4 (ref "xxxxxxxxx")]
    Mutex.with_lock (ref out.cell) (\b -> b[0] ()) + k + junk.length * 0
do! IO.write_line (to_string (run 1))
"#;
    const KEEP: &str = "    let t = ref p.a[0]\n    Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b)\n";
    const REPLACE: &str = "    deref p = mkpa ()\n";
    let bodies = [
        // the copy of what the cell holds (`contents_elsewhere`)
        "Mutex.with_lock (ref p.h.cell) (\\b -> copy_out b (ref out.cell))",
        // the borrow that the cell holds, stored in another target (`kept_elsewhere`)
        "Mutex.with_lock (ref out.cell) (\\b -> Array.write 0 (\\() -> look t) b)",
    ];
    let loops: [&[&str]; 9] = [
        &[
            "let mut i = 0",
            "while i < n do",
            "    BODY",
            "    i = i + 1",
        ],
        &["for i in 0 ..= (n - 1) do", "    BODY"],
        &["let xs = [1, 2]", "for x in xs do", "    BODY"],
        &[
            "let mut i = 0",
            "while true do",
            "    if i >= n then break else ()",
            "    BODY",
            "    i = i + 1",
        ],
        &[
            "let mut i = 0",
            "while i < n do",
            "    i = i + 1",
            "    BODY",
            "    if i > 0 then continue else ()",
        ],
        &[
            "let mut i = 0",
            "while i < n do",
            "    let mut j = 0",
            "    while j < n do",
            "        BODY",
            "        j = j + 1",
            "    i = i + 1",
        ],
        &[
            "let mut i = 0",
            "while i < n do",
            "    let r = try",
            "                BODY",
            "                @checked i * 2",
            "            with",
            "            | e -> e",
            "    i = i + 1",
        ],
        &[
            "let mut i = 0",
            "while i < n do",
            "    match i with",
            "    | 0 -> BODY",
            "    | _ -> ()",
            "    i = i + 1",
        ],
        &[
            "let mut i = 0",
            "while i < n do",
            "    if i == 0 then BODY else ()",
            "    i = i + 1",
        ],
    ];
    for body in bodies {
        for lines in loops {
            let mut looped = String::new();
            for line in lines {
                looped.push_str("    ");
                looped.push_str(&line.replace("BODY", body));
                looped.push('\n');
            }
            let unsafe_source = format!("{PRELUDE}{KEEP}{looped}{REPLACE}{RUN}");
            let at = position(
                &unsafe_source,
                unsafe_source.find("deref p = mkpa ()").unwrap(),
            );
            assert_eq!(at.1, 5);
            rejects_at(&unsafe_source, "E1014", "borrow_conflict", at);
            let safe = format!("{PRELUDE}{REPLACE}{KEEP}{looped}{RUN}");
            accepts_in_both_editions(&[("Main.tz", &safe)]);
        }
    }
    // A value read through `p` before the loop, a lambda that captures `p` in it.
    let through = |assigned: &str| {
        format!(
            r#"record Holder {{ cell: Mutex<[unit -> i64]> }}
instance Drop<Holder> {{
    fn drop h = {{
        let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy")];
        do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())) + " " + to_string junk.length)
    }}
}}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def mkh :: unit -> Holder
fn mkh _u = Holder {{ cell: Mutex.create [\() -> 0] }}
record P {{ s: string, f: unit -> i64 }}
def stash :: ref P -> ref Holder -> i64 -> i64
fn stash p out n =
    let mut x = p.f
    let mut i = 0
    while i < n do
        x = {assigned}
        i = i + 1
    Mutex.with_lock (ref out.cell) (\b -> Array.write 0 x b)
    0
def run :: i64 -> i64
fn run n =
    let out = mkh ()
    let p1 = P {{ s: String.repeat 4 (ref "123456789"), f: \() -> 5 }}
    stash (ref p1) (ref out) n
do! IO.write_line (to_string (run 1))
"#
        )
    };
    rejects_at(
        &through("\\() -> look (ref p.s)"),
        "E1013",
        "assignment_extends_borrow",
        (27, 5),
    );
    accepts_in_both_editions(&[("Main.tz", &through("p.f"))]);
}

/// A31, the review of the fix of Copilot's review 2 (`loopplace` in
/// `_features/A34-store-check-safe-rejections.md`): a loop that passes a borrow of the target of
/// a reference parameter whose type has a cell (the cell's contents `b[0]`, a function that
/// captures `ref p.s`) to a function value that owns the cell it writes (a lambda that captures an
/// `own` parameter, an edition 2026 value parameter or a local `Arc.share`) never settled: the
/// store into the function value added the borrows of the target to `p`, whose dereference made
/// the places of the next iteration one field deeper (`p.cell.cell...`), and the loans that the
/// locals hold doubled with each iteration, so that the check of `15d84aaf` did not end (killed
/// after 120 s, 600 s for the first program; `main` accepts the safe forms within a second). The
/// check now stops with `E1017` (`loop_ownership_limit`) at the start of the loop's body, where
/// the limit of the iterations reports it, with a help that says how many loans the iteration
/// made and names the change that lets the loop settle. Iteration `k` (from 0) of these loops
/// makes `2^k + 2` loans, and iteration 9 (514) is the first that makes more than 500 beyond the
/// first iteration's 3 (`LOOP_LOAN_GROWTH`). The unsafe twin (the caller's `keep` gets the borrow
/// of a string that dies first) stops the same way. With that change (the lambda captures
/// `Arc.get (ref sink)`), the safe form is accepted and the twin is rejected where its block drops
/// the string (`E1013`). A chain of 12 references, each to a field of the target of the next,
/// borrows deeper in each of its first 12 iterations and settles in the 13th, as on `15d84aaf` and
/// `main`: iteration `k` makes `k (k - 1) / 2 + k (12 - k)` loans more than the first, at most 66.
#[test]
fn a_loop_whose_borrows_keep_growing_stops_with_e1017() {
    const OWN: &str = r#"record Box { cell: Mutex<[unit -> i64]> }
def run2 :: ref mut Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64
fn run2 p sink n =
    let w = \g -> Mutex.with_lock (Arc.get (ref sink)) (\o -> Array.write 0 g o)
    let mut i = 0
    while i < n do
        Mutex.with_lock (ref p.cell) (\b -> w b[0])
        i = i + 1
    0
def run :: i64 -> i64
fn run n =
    let keep = Arc.new (Mutex.create [\() -> 0])
    let mut b = Box { cell: Mutex.create [\() -> 3] }
    let k = run2 (ref mut b) (Arc.share (ref keep)) n
    Mutex.with_lock (Arc.get (ref keep)) (\c -> c[0] ()) + k
do! IO.write_line (to_string (run 1))
"#;
    const FIELD: &str = r#"record Box { cell: Mutex<[unit -> i64]>, s: string }
def run2 :: ref mut Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64
fn run2 p sink n =
    let w = \g -> Mutex.with_lock (Arc.get (ref sink)) (\o -> Array.write 0 g o)
    let mut i = 0
    while i < n do
        let t = ref p.s
        w (\() -> String.length t)
        i = i + 1
    0
def run :: i64 -> i64
fn run n =
    let mut b = Box { cell: Mutex.create [\() -> 3], s: "abcd" }
    let keep = Arc.new (Mutex.create [\() -> 0])
    let k = run2 (ref mut b) (Arc.share (ref keep)) n
    Mutex.with_lock (Arc.get (ref keep)) (\c -> c[0] ()) + k
do! IO.write_line (to_string (run 1))
"#;
    const LOCAL: &str = r#"record Box { cell: Mutex<[unit -> i64]> }
def run2 :: ref mut Box -> i64 -> i64
fn run2 p n =
    let sink = Arc.new (Mutex.create [\() -> 0])
    let sink2 = Arc.share (ref sink)
    let w = \g -> Mutex.with_lock (Arc.get (ref sink2)) (\o -> Array.write 0 g o)
    let mut i = 0
    while i < n do
        Mutex.with_lock (ref p.cell) (\b -> w b[0])
        i = i + 1
    Mutex.with_lock (Arc.get (ref sink)) (\c -> c[0] ())
def run :: i64 -> i64
fn run n =
    let mut b = Box { cell: Mutex.create [\() -> 3] }
    run2 (ref mut b) n
do! IO.write_line (to_string (run 1))
"#;
    const TWIN: &str = r#"record Box { cell: Mutex<[unit -> i64]> }
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def run2 :: ref mut Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64
fn run2 p sink n =
    let w = \g -> Mutex.with_lock (Arc.get (ref sink)) (\o -> Array.write 0 g o)
    let mut i = 0
    while i < n do
        Mutex.with_lock (ref p.cell) (\b -> w b[0])
        i = i + 1
    0
def run :: i64 -> i64
fn run n =
    let keep = Arc.new (Mutex.create [\() -> 0])
    let k = {
        let s = String.repeat 4 (ref "123456789");
        let t = ref s;
        let mut b = Box { cell: Mutex.create [\() -> look t] };
        run2 (ref mut b) (Arc.share (ref keep)) n
    }
    let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy")]
    Mutex.with_lock (Arc.get (ref keep)) (\c -> c[0] ()) + k + junk.length * 0
do! IO.write_line (to_string (run 1))
"#;
    const LAMBDA: &str =
        "    let w = \\g -> Mutex.with_lock (Arc.get (ref sink)) (\\o -> Array.write 0 g o)\n";
    const SETTLES: &str = "    let cell = Arc.get (ref sink)\n    let w = \\g -> Mutex.with_lock cell (\\o -> Array.write 0 g o)\n";
    let stops = |source: &str, edition: Edition, at: (usize, usize)| {
        let error = analyze_with_edition(source, edition).expect_err(source);
        assert_eq!(
            (error.code, error.kind),
            ("E1017", Some("loop_ownership_limit")),
            "edition {edition}\n{source}\n{}",
            error.message
        );
        assert_eq!(
            position(source, error.span.start),
            at,
            "edition {edition}\n{source}"
        );
        assert!(
            error
                .help
                .as_deref()
                .is_some_and(|help| help.contains("`let cell = Arc.get (ref sink)`")),
            "edition {edition}\n{source}\n{:?}",
            error.help
        );
    };
    for edition in [Edition::E2026, Edition::E2027] {
        stops(OWN, edition, (7, 9));
        let help = analyze_with_edition(OWN, edition)
            .expect_err(OWN)
            .help
            .clone();
        assert!(
            help.as_deref().is_some_and(|help| help.contains(
                "one iteration made 514 borrows, more than 500 beyond the 3 of the first"
            )),
            "edition {edition}: {help:?}"
        );
        stops(FIELD, edition, (7, 9));
        stops(LOCAL, edition, (9, 9));
        stops(TWIN, edition, (12, 9));
    }
    // An edition 2026 value parameter owns the `Arc`; in edition 2027 it is a read parameter, and
    // a lambda that captures it owns no cell.
    let value = OWN.replace("-> own Arc", "-> Arc");
    stops(&value, Edition::E2026, (7, 9));
    analyze_with_edition(&value, Edition::E2027)
        .unwrap_or_else(|error| panic!("{value}\n{}: {}", error.code, error.message));
    assert!(OWN.contains(LAMBDA) && TWIN.contains(LAMBDA));
    accepts_in_both_editions(&[("Main.tz", &OWN.replace(LAMBDA, SETTLES))]);
    rejects_at(
        &TWIN.replace(LAMBDA, SETTLES),
        "E1013",
        "assignment_extends_borrow",
        (19, 13),
    );
    // `q0 = ref (deref q1).a`, ..., `q11 = ref (deref q12).a`, each `qk` a `ref Rk` that starts
    // at its own `yk`: the iteration `j` (from 0) makes `q0` borrow `y(j+1).a.a...` (`j + 1`
    // fields), up to `y12` and 12 fields in the 12th iteration, and the 13th changes nothing.
    let mut chain = String::from("record R0 { v: i64 }\n");
    let mut literals = vec![String::from("R0 { v: 1 }")];
    for k in 1..=12 {
        chain.push_str(&format!("record R{k} {{ a: R{}, v: i64 }}\n", k - 1));
        literals.push(format!("R{k} {{ a: {}, v: {} }}", literals[k - 1], k + 1));
    }
    chain.push_str("def run :: i64 -> i64\nfn run n =\n");
    for (k, literal) in literals.iter().enumerate() {
        chain.push_str(&format!("    let y{k} = {literal}\n"));
    }
    for k in 0..=12 {
        chain.push_str(&format!("    let mut q{k} = ref y{k}\n"));
    }
    chain.push_str("    let mut i = 0\n    while i < n do\n");
    for k in 0..12 {
        chain.push_str(&format!("        q{k} = ref (deref q{}).a\n", k + 1));
    }
    chain.push_str("        i = i + 1\n    q0.v + q1.v\ndo! IO.write_line (to_string (run 3))\n");
    accepts_in_both_editions(&[("Main.tz", &chain)]);
}

/// The records `R0 { v }` and `Rk { a: R(k-1), v }` up to `R{links}`, and the literal of each.
fn chain_records(links: usize) -> (String, Vec<String>) {
    let mut records = String::from("record R0 { v: i64 }\n");
    let mut literals = vec![String::from("R0 { v: 1 }")];
    for k in 1..=links {
        records.push_str(&format!("record R{k} {{ a: R{}, v: i64 }}\n", k - 1));
        literals.push(format!("R{k} {{ a: {}, v: {} }}", literals[k - 1], k + 1));
    }
    (records, literals)
}

/// The loop `q0 = ref (deref q1).a`, ..., `q{links-1} = ref (deref q{links}).a`, `n` times, and
/// the result `q0.v + q1.v`.
fn chain_loop(links: usize) -> String {
    let mut code = String::from("    let mut i = 0\n    while i < n do\n");
    for k in 0..links {
        code.push_str(&format!("        q{k} = ref (deref q{}).a\n", k + 1));
    }
    code.push_str("        i = i + 1\n    q0.v + q1.v\n");
    code
}

/// A `loopplace` loop in which `t{k-1}`, ..., `t0` carry the borrow of `p.s` for `k` iterations
/// before the store into the function value that owns a cell.
fn late_hand_on(k: usize) -> String {
    let mut source = String::from(
        "record Box { cell: Mutex<[unit -> i64]>, s: string }\ndef run2 :: ref mut Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64\nfn run2 p sink n =\n",
    );
    source.push_str(
        "    let w = \\g -> Mutex.with_lock (Arc.get (ref sink)) (\\o -> Array.write 0 g o)\n",
    );
    source.push_str("    let mut i = 0\n");
    for j in 0..k {
        source.push_str(&format!("    let mut t{j} = ref p.s\n"));
    }
    source.push_str(&format!(
        "    while i < n do\n        let t = t{}\n        w (\\() -> String.length t)\n",
        k - 1
    ));
    for j in (1..k).rev() {
        source.push_str(&format!("        t{j} = t{}\n", j - 1));
    }
    source.push_str("        t0 = ref p.s\n        i = i + 1\n    0\ndef run :: i64 -> i64\nfn run n =\n    let mut b = Box { cell: Mutex.create [\\() -> 3], s: \"abcd\" }\n    let keep = Arc.new (Mutex.create [\\() -> 0])\n    let k = run2 (ref mut b) (Arc.share (ref keep)) n\n    Mutex.with_lock (Arc.get (ref keep)) (\\c -> c[0] ()) + k\ndo! IO.write_line (to_string (run 1))\n");
    source
}

/// A31, the review of the first two `E1017` stops of `loopplace`, which guessed from the shapes of
/// the places that a loop borrows. The first (`773ea1c4`) counted the iterations that borrowed a
/// place deeper than before whose fields left the type that they read from the root's
/// declaration. The place of a loan that the result of a call holds is its argument's
/// (`top (ref y)` returns `ref y.r` with the loan of `y`), the external loan of a record parameter
/// with a region stands for what its references point at, and so does a loan read from a
/// reference that a parameter holds: their fields name fields of another value, so a chain of 10
/// or more references, each to a field of the target of the next, that starts at such places got
/// `E1017`, although `15d84aaf`, `main` and the base accept it (each prints 3). The check now
/// counts the loans that each iteration makes (`LOOP_LOAN_GROWTH`): iteration `k` of the chains
/// below, of 12 links, makes `k (k - 1) / 2 + k (12 - k)` loans more than the first, at most 66,
/// and twice as many in `each`, whose links start with two places each (a field of `p` and the
/// same field of the borrow of `p.r12` that the store before the loop added to `p`), at most 132.
/// They are accepted in both editions.
///
/// Two forms that the first stop missed stop at the start of the loop's body. A `dyn Display`
/// field (`ref p.s`), whose loans double as in the other `loopplace` forms and whose unsafe twin
/// `main` accepts and runs into a use after free; with `let cell = Arc.get (ref sink)` the safe
/// form is accepted and the twin is rejected where its block drops the string (`E1013`). And a
/// loop that hands the borrow to the function value through `t1 = t0` (one or two locals before
/// the store, `late_hand_on`): its loans grow by about 1.6 or 1.5 times per iteration instead of
/// doubling, and its iteration 12 (612 loans, 4 in the first) or 15 (597) is the first beyond the
/// limit.
#[test]
fn a_loop_stops_by_the_loans_of_its_iterations_not_by_the_shapes_of_its_places() {
    let (records, literals) = chain_records(12);
    // Call results: `qk = topk (ref yk)` holds the loan of `yk` while it points at `yk.r`.
    let mut call = records.clone();
    for k in 0..=12 {
        call.push_str(&format!(
            "record Y{k} {{ pad: i64, r: R{k} }}\ndef top{k} :: ref Y{k} -> ref R{k}\nfn top{k} y = ref y.r\n"
        ));
    }
    call.push_str("def run :: i64 -> i64\nfn run n =\n");
    for (k, literal) in literals.iter().enumerate() {
        call.push_str(&format!("    let y{k} = Y{k} {{ pad: 0, r: {literal} }}\n"));
    }
    for k in 0..=12 {
        call.push_str(&format!("    let mut q{k} = top{k} (ref y{k})\n"));
    }
    call.push_str(&chain_loop(12));
    call.push_str("do! IO.write_line (to_string (run 3))\n");
    // A record parameter with one region: `qk = big.fk` holds the external loan of `big`.
    let mut big = records.clone();
    let fields: Vec<String> = (0..=12).map(|k| format!("f{k}: ref {{r}} R{k}")).collect();
    big.push_str(&format!("record Big {{r}} {{ {} }}\n", fields.join(", ")));
    big.push_str("def walk {r} :: Big {r} -> i64 -> i64\nfn walk big n =\n");
    for k in 0..=12 {
        big.push_str(&format!("    let mut q{k} = big.f{k}\n"));
    }
    big.push_str(&chain_loop(12));
    big.push_str("def run :: i64 -> i64\nfn run n =\n");
    for (k, literal) in literals.iter().enumerate() {
        big.push_str(&format!("    let y{k} = {literal}\n"));
    }
    let lent: Vec<String> = (0..=12).map(|k| format!("f{k}: ref y{k}")).collect();
    big.push_str(&format!(
        "    let big = Big {{ {} }}\n    walk big n\ndo! IO.write_line (to_string (run 3))\n",
        lent.join(", ")
    ));
    // A reference that a reference parameter holds: `q12 = w.r`.
    let mut held = records.clone();
    held.push_str("record W {r} { r: ref {r} R12 }\ndef go :: ref W -> i64 -> i64\nfn go w n =\n");
    for (k, literal) in literals.iter().enumerate().take(12) {
        held.push_str(&format!("    let y{k} = {literal}\n"));
    }
    for k in 0..12 {
        held.push_str(&format!("    let mut q{k} = ref y{k}\n"));
    }
    held.push_str("    let mut q12 = w.r\n");
    held.push_str(&chain_loop(12));
    held.push_str(&format!(
        "def run :: i64 -> i64\nfn run n =\n    let yl = {}\n    let w = W {{ r: ref yl }}\n    go (ref w) n\ndo! IO.write_line (to_string (run 3))\n",
        literals[12]
    ));
    // Each `qk = ref p.rk` holds a field of the borrow of `p.r12` that the store before the loop
    // added to `p`, a place that no value has; the loop stores nothing.
    let mut each = records.clone();
    let fields: Vec<String> = (0..=12).map(|k| format!("r{k}: R{k}")).collect();
    each.push_str(&format!(
        "record Box {{ cell: Mutex<[unit -> i64]>, {} }}\n",
        fields.join(", ")
    ));
    each.push_str("def run2 :: ref mut Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64\nfn run2 p sink n =\n");
    each.push_str(
        "    let w = \\g -> Mutex.with_lock (Arc.get (ref sink)) (\\o -> Array.write 0 g o)\n",
    );
    each.push_str("    let t = ref p.r12\n    w (\\() -> t.v)\n");
    for k in 0..=12 {
        each.push_str(&format!("    let mut q{k} = ref p.r{k}\n"));
    }
    each.push_str(&chain_loop(12));
    let values: Vec<String> = literals
        .iter()
        .enumerate()
        .map(|(k, literal)| format!("r{k}: {literal}"))
        .collect();
    each.push_str(&format!(
        "def run :: i64 -> i64\nfn run n =\n    let mut b = Box {{ cell: Mutex.create [\\() -> 3], {} }}\n    let keep = Arc.new (Mutex.create [\\() -> 0])\n    let k = run2 (ref mut b) (Arc.share (ref keep)) n\n    Mutex.with_lock (Arc.get (ref keep)) (\\c -> c[0] ()) + k\ndo! IO.write_line (to_string (run 3))\n",
        values.join(", ")
    ));
    for source in [&call, &big, &held, &each] {
        accepts_in_both_editions(&[("Main.tz", source)]);
    }

    const DYN: &str = r#"record Box { cell: Mutex<[unit -> i64]>, s: dyn Display }
def run2 :: ref mut Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64
fn run2 p sink n =
    let w = \g -> Mutex.with_lock (Arc.get (ref sink)) (\o -> Array.write 0 g o)
    let mut i = 0
    while i < n do
        let t = ref p.s
        w (\() -> String.length (Display.display t))
        i = i + 1
    0
def run :: i64 -> i64
fn run n =
    let mut b = Box { cell: Mutex.create [\() -> 3], s: Dyn.of "abcd" }
    let keep = Arc.new (Mutex.create [\() -> 0])
    let k = run2 (ref mut b) (Arc.share (ref keep)) n
    Mutex.with_lock (Arc.get (ref keep)) (\c -> c[0] ()) + k
do! IO.write_line (to_string (run 1))
"#;
    const DYN_TWIN: &str = r#"record Box { cell: Mutex<[unit -> i64]>, s: dyn Display }
def run2 :: ref mut Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64
fn run2 p sink n =
    let w = \g -> Mutex.with_lock (Arc.get (ref sink)) (\o -> Array.write 0 g o)
    let mut i = 0
    while i < n do
        let t = ref p.s
        w (\() -> String.length (Display.display t))
        i = i + 1
    0
def run :: i64 -> i64
fn run n =
    let keep = Arc.new (Mutex.create [\() -> 0])
    let k = {
        let mut b = Box { cell: Mutex.create [\() -> 3], s: Dyn.of (String.repeat 4 (ref "123456789")) };
        run2 (ref mut b) (Arc.share (ref keep)) n
    }
    let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy")]
    Mutex.with_lock (Arc.get (ref keep)) (\c -> c[0] ()) + k + junk.length * 0
do! IO.write_line (to_string (run 1))
"#;
    const LAMBDA: &str =
        "    let w = \\g -> Mutex.with_lock (Arc.get (ref sink)) (\\o -> Array.write 0 g o)\n";
    const SETTLES: &str = "    let cell = Arc.get (ref sink)\n    let w = \\g -> Mutex.with_lock cell (\\o -> Array.write 0 g o)\n";
    let (two, three) = (late_hand_on(2), late_hand_on(3));
    for (source, at) in [
        (DYN, (7, 9)),
        (DYN_TWIN, (7, 9)),
        (&two, (9, 9)),
        (&three, (10, 9)),
    ] {
        for edition in [Edition::E2026, Edition::E2027] {
            let error = analyze_with_edition(source, edition).expect_err(source);
            assert_eq!(
                (error.code, error.kind),
                ("E1017", Some("loop_ownership_limit")),
                "edition {edition}\n{source}\n{}",
                error.message
            );
            assert_eq!(
                position(source, error.span.start),
                at,
                "edition {edition}\n{source}"
            );
            assert!(
                error
                    .help
                    .as_deref()
                    .is_some_and(|help| help.contains("`let cell = Arc.get (ref sink)`")),
                "edition {edition}\n{source}\n{:?}",
                error.help
            );
        }
    }
    assert!(DYN.contains(LAMBDA) && DYN_TWIN.contains(LAMBDA) && two.contains(LAMBDA));
    accepts_in_both_editions(&[("Main.tz", &DYN.replace(LAMBDA, SETTLES))]);
    accepts_in_both_editions(&[("Main.tz", &two.replace(LAMBDA, SETTLES))]);
    rejects_at(
        &DYN_TWIN.replace(LAMBDA, SETTLES),
        "E1013",
        "assignment_extends_borrow",
        (15, 13),
    );
}

/// The `each` chain of the review of the first `loopplace` stops: `qk = ref p.rk` for `k` up to
/// `links`, after a store of the borrow of `p.r{links}` before the loop, then the loop of
/// `chain_loop` with `extra` at the end of its body and `result` after it. The function value that
/// owns the cell captures the `own` parameter `sink`, or a local `Arc.share` (`local`).
fn each_chain(links: usize, extra: &str, result: &str, local: bool) -> String {
    let (mut source, literals) = chain_records(links);
    let fields: Vec<String> = (0..=links).map(|k| format!("r{k}: R{k}")).collect();
    source.push_str(&format!(
        "record Box {{ cell: Mutex<[unit -> i64]>, {} }}\n",
        fields.join(", ")
    ));
    if local {
        source.push_str("def run2 :: ref mut Box -> i64 -> i64\nfn run2 p n =\n    let sink = Arc.new (Mutex.create [\\() -> 0])\n    let sink2 = Arc.share (ref sink)\n    let w = \\g -> Mutex.with_lock (Arc.get (ref sink2)) (\\o -> Array.write 0 g o)\n");
    } else {
        source.push_str("def run2 :: ref mut Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64\nfn run2 p sink n =\n    let w = \\g -> Mutex.with_lock (Arc.get (ref sink)) (\\o -> Array.write 0 g o)\n");
    }
    source.push_str(&format!(
        "    let t = ref p.r{links}\n    w (\\() -> t.v)\n"
    ));
    for k in 0..=links {
        source.push_str(&format!("    let mut q{k} = ref p.r{k}\n"));
    }
    source.push_str(&chain_loop(links).replace("        i = i + 1\n    q0.v + q1.v\n", ""));
    source.push_str(extra);
    source.push_str("        i = i + 1\n");
    source.push_str(result);
    let values: Vec<String> = literals
        .iter()
        .enumerate()
        .map(|(k, literal)| format!("r{k}: {literal}"))
        .collect();
    source.push_str(&format!(
        "def run :: i64 -> i64\nfn run n =\n    let mut b = Box {{ cell: Mutex.create [\\() -> 3], {} }}\n",
        values.join(", ")
    ));
    if local {
        source.push_str("    run2 (ref mut b) n\n");
    } else {
        source.push_str("    let keep = Arc.new (Mutex.create [\\() -> 0])\n    let k = run2 (ref mut b) (Arc.share (ref keep)) n\n    Mutex.with_lock (Arc.get (ref keep)) (\\c -> c[0] ()) + k\n");
    }
    source.push_str("do! IO.write_line (to_string (run 3))\n");
    source
}

/// A31, the review of the second `E1017` stop of `loopplace` (`5f3c2a0b`), which counted the
/// iterations whose locals held deeper places that no value has, once such places had been fed
/// back to the local whose dereference made them. It stopped converging chains whose places a
/// store in the loop fed back to the parameter while the loop never dereferenced the parameter
/// again: `each9_store0_live` (9 links, the loop stores `\() -> q0.v`), `es_live_10` (10 links,
/// `let u = q0` before the store) and `esl_local` (9 links, a local `Arc.share` sink), which
/// `15d84aaf`, `main` and the base accept (`main` prints 5, 5 and 5). Their iteration `k` makes
/// `2 (k (k - 1) / 2 + k (L - k))` loans more than the first (their links start with two places
/// each), at most 72, 90 and 72; a chain of 16 references to locals makes at most 120 more. All
/// four are accepted in both editions.
///
/// It never stopped the `loopplace` forms whose dereferenced reference is a local of the loop body
/// (`let r = p; let t = ref r.s`, `e_copy2`, or `let q = p`, `bl_copy`), or the result of an `if`
/// (`gl_if`), and a choice between reborrows of a `ref mut` parameter (`rm_ifref`) triples the
/// loans in each iteration, so that it took more than 10 minutes. Those stop now at the start of
/// the loop's body: their iteration 9 makes 514 loans (3 in the first; 515 and 4 for `gl_if`), the
/// iteration 5 of `rm_ifref` 975 (7). So does a loop that passes the borrow through four locals
/// before the store (`late_hand_on(4)`, iteration 18: 659 loans, 4 in the first).
///
/// `LOOP_LOAN_GROWTH` (500) counts every loan of the body: a loop that advances 17 chains of 8
/// references each makes at most 17 times 28 = 476 more loans than in its first iteration and is
/// accepted, and with 18 chains its iteration 7 makes 504 more, and the loop stops at the start of
/// its body. So does a single chain of 33 references (`L (L - 1) / 2` = 528 more at most; its
/// iteration 26 is the first beyond the limit), where the documents put the limit; a chain of 32
/// (496 at most) is accepted, but takes a release build about 13 seconds, so the test does not
/// check it.
#[test]
fn a_loop_stops_when_an_iteration_makes_500_loans_more_than_the_first() {
    let live = "    q0.v + q1.v + p.r0.v\n";
    // A chain of `links` references, each `qk` starting at its own local `yk`.
    let single = |links: usize| {
        let (mut source, literals) = chain_records(links);
        source.push_str("def run :: i64 -> i64\nfn run n =\n");
        for (k, literal) in literals.iter().enumerate() {
            source.push_str(&format!("    let y{k} = {literal}\n"));
        }
        for k in 0..=links {
            source.push_str(&format!("    let mut q{k} = ref y{k}\n"));
        }
        source.push_str(&chain_loop(links));
        source.push_str("do! IO.write_line (to_string (run 3))\n");
        source
    };
    let chain = single(16);
    for source in [
        each_chain(9, "        w (\\() -> q0.v)\n", live, false),
        each_chain(
            10,
            "        let u = q0\n        w (\\() -> u.v)\n",
            live,
            false,
        ),
        each_chain(
            9,
            "        let u = q0\n        w (\\() -> u.v)\n",
            "    q0.v + q1.v + p.r0.v + Mutex.with_lock (Arc.get (ref sink)) (\\c -> c[0] ())\n",
            true,
        ),
        chain,
    ] {
        accepts_in_both_editions(&[("Main.tz", &source)]);
    }

    const COPY: &str = r#"record Box { cell: Mutex<[unit -> i64]>, s: string }
def run2 :: ref Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64
fn run2 p sink n =
    let w = \g -> Mutex.with_lock (Arc.get (ref sink)) (\o -> Array.write 0 g o)
    let mut i = 0
    while i < n do
        let r = p
        let t = ref r.s
        w (\() -> String.length t)
        i = i + 1
    0
def run :: i64 -> i64
fn run n =
    let b = Box { cell: Mutex.create [\() -> 3], s: "abcd" }
    let keep = Arc.new (Mutex.create [\() -> 0])
    let k = run2 (ref b) (Arc.share (ref keep)) n
    Mutex.with_lock (Arc.get (ref keep)) (\c -> c[0] ()) + k
do! IO.write_line (to_string (run 1))
"#;
    let local = COPY.replace(
        "        let r = p\n        let t = ref r.s\n",
        "        let q = p\n        let t = ref q.s\n",
    );
    let choice = COPY.replace(
        "        let r = p\n        let t = ref r.s\n",
        "        let t = ref (if i >= 0 then p else p).s\n",
    );
    let reborrow = COPY
        .replace("ref Box ->", "ref mut Box ->")
        .replace("    let b = Box", "    let mut b = Box")
        .replace("run2 (ref b)", "run2 (ref mut b)")
        .replace(
            "        let r = p\n        let t = ref r.s\n",
            "        let t = ref (deref (if i >= 0 then ref p else ref p)).s\n",
        );
    let late = late_hand_on(4);
    for (source, at) in [
        (COPY, (7, 9)),
        (&local, (7, 9)),
        (&choice, (7, 9)),
        (&reborrow, (7, 9)),
        (&late, (11, 9)),
    ] {
        for edition in [Edition::E2026, Edition::E2027] {
            let error = analyze_with_edition(source, edition).expect_err(source);
            assert_eq!(
                (error.code, error.kind),
                ("E1017", Some("loop_ownership_limit")),
                "edition {edition}\n{source}\n{}",
                error.message
            );
            assert_eq!(
                position(source, error.span.start),
                at,
                "edition {edition}\n{source}"
            );
            assert!(
                error
                    .help
                    .as_deref()
                    .is_some_and(|help| help.contains("`let cell = Arc.get (ref sink)`")),
                "edition {edition}\n{source}\n{:?}",
                error.help
            );
        }
    }

    // `q<c>_<k> = ref (deref q<c>_<k+1>).a` for `chains` chains of 8 links, which share the records.
    let chains = |chains: usize| {
        let (mut source, literals) = chain_records(8);
        source.push_str("def run :: i64 -> i64\nfn run n =\n");
        for c in 0..chains {
            for (k, literal) in literals.iter().enumerate() {
                source.push_str(&format!("    let y{c}_{k} = {literal}\n"));
            }
        }
        for c in 0..chains {
            for k in 0..=8 {
                source.push_str(&format!("    let mut q{c}_{k} = ref y{c}_{k}\n"));
            }
        }
        source.push_str("    let mut i = 0\n    while i < n do\n");
        for c in 0..chains {
            for k in 0..8 {
                source.push_str(&format!(
                    "        q{c}_{k} = ref (deref q{c}_{}).a\n",
                    k + 1
                ));
            }
        }
        source.push_str(
            "        i = i + 1\n    q0_0.v + q0_1.v\ndo! IO.write_line (to_string (run 3))\n",
        );
        source
    };
    accepts_in_both_editions(&[("Main.tz", &chains(17))]);
    // 18 chains of 8 links (iteration 7: 504 more), and one chain of 33 links (iteration 26:
    // 26 * 25 / 2 + 26 * 7 = 507 more; iteration 25 makes 500 more).
    for (over, first) in [
        (chains(18), "        q0_0 = "),
        (single(33), "        q0 = "),
    ] {
        let body = over.find(first).unwrap();
        for edition in [Edition::E2026, Edition::E2027] {
            let error = analyze_with_edition(&over, edition).expect_err(&over);
            assert_eq!(
                (error.code, error.kind),
                ("E1017", Some("loop_ownership_limit")),
                "edition {edition}: {}",
                error.message
            );
            assert_eq!(error.span.start, body + 8, "edition {edition}");
            assert!(
                error
                    .help
                    .as_deref()
                    .is_some_and(|help| help.contains("more than 500 beyond")),
                "edition {edition}: {:?}",
                error.help
            );
        }
    }
}
