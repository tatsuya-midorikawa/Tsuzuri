use tsuzuri::formatter::{ast_fingerprint, format_source};
use tsuzuri::syntax::SourceKind;
use tsuzuri::{analyze, analyze_modules, analyze_modules_with_semantics, llvm, parser};

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

/// A28 review: the check does not track what `Array.write` stores in `arr` (A31), so a body that
/// lends an exclusive borrow to a call reads nothing through a borrowed owner: the share holds the
/// borrow of `arr`, as before A28, and cannot leave `make`. Reading through, it held what `arr`
/// held before the write, not `text`, which `make` frees. The A30 tip rejects both forms here.
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
    // diagnostic at the share in the helper, except the upgrade, which the A30 tip reports at the
    // borrow of `weak` (4:5, `borrow_outlives_block`): the upgrade reads through `weak`, a local of
    // `wrap`, and holds the borrow of `arr` that `weak` kept.
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
        (
            format!(
                "def wrap :: own [Arc<unit -> i64>] -> Maybe<Arc<unit -> i64>>\nfn wrap arr =\n    let weak = Arc.downgrade (ref arr[0])\n    Arc.upgrade (ref weak)\n{}match make 123456789 with\n    | Maybe.Some g -> (deref (Arc.get (ref g))) ()\n    | Maybe.None -> 0",
                make("Arc", "Maybe<Arc<unit -> i64>>", "wrap arr")
            ),
            (3, 5),
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
