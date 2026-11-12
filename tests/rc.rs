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
}
