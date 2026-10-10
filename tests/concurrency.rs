use tsuzuri::{analyze, analyze_modules, llvm};

fn rejects(source: &str, code: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.message
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

const OPERATIONS: &str = "let counter = Atomic.create 0i64\n\
let added = Atomic.fetch_add (ref counter) 5\n\
let subtracted = Atomic.fetch_sub (ref counter) 1\n\
let anded = Atomic.fetch_and (ref counter) 7\n\
let ored = Atomic.fetch_or (ref counter) 8\n\
let xored = Atomic.fetch_xor (ref counter) 1\n\
let swapped = Atomic.swap (ref counter) 3\n\
Atomic.store (ref counter) 4\n\
let exchanged = match Atomic.compare_exchange (ref counter) 4 9 with\n    | Result.Ok previous -> previous\n    | Result.Error current -> current\n\
let last = Atomic.load (ref counter)\n\
added + subtracted + anded + ored + xored + swapped + exchanged + last + Atomic.into_inner counter";

const INTEGERS: [&str; 8] = ["i8", "i16", "i32", "i64", "i8u", "i16u", "i32u", "i64u"];

const NODE: &str = "record Node { value: i64, next: Maybe<Arc<Mutex<Node>>> }\n";

const HANDLE: &str = "extern type Counter\n\
extern \"f10_counter_new\" def counter_new :: i64 -> Counter\n";

const SHAPE: &str = "class Shape<'a> {\n    def area :: ref 'a -> i64\n}\nrecord Square { side: i64 }\ninstance Shape<Square> {\n    fn area s = s.side * s.side\n}\n";

const SYNC_PARAMETER: &str = "def share :: Sync<'a> => ref 'a -> i64 = \\_value -> 1\n";

#[test]
fn atomic_programs_type_check_and_emit() {
    emits(OPERATIONS);
    for ty in INTEGERS {
        emits(&format!(
            "let cell = Atomic.create 1{ty}\nlet previous = Atomic.fetch_add (ref cell) 2{ty}\nlet changed = Atomic.fetch_xor (ref cell) 1{ty}\nif Atomic.load (ref cell) == 2{ty} && previous == 1{ty} && changed == 3{ty} then 1 else 0"
        ));
    }
    emits(
        "let flag = Atomic.create false\nlet was = Atomic.swap (ref flag) true\nlet ok = match Atomic.compare_exchange (ref flag) true false with\n    | Result.Ok _previous -> 1\n    | Result.Error _current -> 0\nif was || Atomic.load (ref flag) then 0 else ok",
    );
    for source in [
        "def bump :: (AtomicValue<'a>, Integer<'a>) => ref Atomic<'a> -> 'a -> 'a = \\cell amount -> Atomic.fetch_add cell amount\nlet cell = Atomic.create 4i32\nbump (ref cell) 3",
        "record Counters { hits: Atomic<i64>, misses: Atomic<i32> }\nlet counters = Counters { hits: Atomic.create 0i64, misses: Atomic.create 0i32 }\nlet _hit = Atomic.fetch_add (ref counters.hits) 1\nAtomic.load (ref counters.hits)",
        "let cells = Vec.push (Vec.push (Vec.empty()) (Atomic.create 1i64)) (Atomic.create 2i64)\nAtomic.load (ref cells[0]) + Atomic.load (ref cells[1])",
        "let cell = Atomic.create 7i64\nTask.run (task { return Atomic.load (ref cell) })",
    ] {
        emits(source);
    }
    for name in ["Atomic", "Mutex"] {
        let error = analyze_modules(&[(name, "")]).unwrap_err();
        assert_eq!(error.code, "E1011", "{name}: {}", error.message);
    }
}

#[test]
fn atomic_values_are_integers_and_bool() {
    let limit =
        "atomic values must be i8, i16, i32, i64, i8u, i16u, i32u, i64u or bool; use Mutex for";
    for (source, ty) in [
        ("let cell = Atomic.create \"text\"\n0", "string"),
        ("let cell = Atomic.create 1.5\n0", "f64"),
        ("let cell = Atomic.create ()\n0", "unit"),
        ("let cell = Atomic.create [1i64]\n0", "[i64]"),
        (
            "record P { x: i64 }\nlet cell = Atomic.create (P { x: 1 })\n0",
            "Main.P",
        ),
        (
            "def bump :: AtomicValue<'a> => 'a -> Atomic<'a> = \\x -> Atomic.create x\nlet fine = Atomic.create 1i64\nlet other = bump \"x\"\n0",
            "string",
        ),
    ] {
        let message = rejects(source, "E1005");
        assert!(message.contains(&format!("{limit} {ty}")), "{message}");
    }
    // Only integers take the arithmetic and bitwise updates.
    for operation in [
        "fetch_add",
        "fetch_sub",
        "fetch_and",
        "fetch_or",
        "fetch_xor",
    ] {
        let message = rejects(
            &format!("let flag = Atomic.create false\nAtomic.{operation} (ref flag) true"),
            "E1005",
        );
        assert!(message.contains("Integer"), "{operation}: {message}");
    }
}

#[test]
fn atomics_and_mutexes_are_owned_and_never_copied() {
    for source in [
        "let first = Atomic.create 0i64\nlet second = first\nAtomic.load (ref first)",
        "let first = Mutex.create 0i64\nlet second = first\nMutex.with_lock (ref first) (\\value -> deref value)",
    ] {
        let message = rejects(source, "E1012");
        assert!(message.contains("'first'"), "{message}");
    }
    for source in [
        "def dup :: Copy<'a> => 'a -> ('a * 'a) = \\x -> (x, x)\ndup (Atomic.create 0i64)",
        "def dup :: Copy<'a> => 'a -> ('a * 'a) = \\x -> (x, x)\ndup (Mutex.create 0i64)",
        "let first = Atomic.create 0i64\nlet second = Atomic.create 0i64\nfirst == second",
    ] {
        let message = rejects(source, "E1005");
        assert!(message.contains("no instance for"), "{message}");
    }
    // A function value may be copied, and a copy would be a second cell.
    for source in [
        "let cell = Atomic.create 0i64\nlet read = \\() -> Atomic.load (ref cell)\nread ()",
        "let cell = Mutex.create 0i64\nlet read = \\() -> Mutex.with_lock (ref cell) (\\value -> deref value)\nread ()",
        "record Counters { hits: Atomic<i64> }\nlet counters = Counters { hits: Atomic.create 0i64 }\nlet read = \\() -> Atomic.load (ref counters.hits)\nread ()",
    ] {
        let message = rejects(source, "E1005");
        assert!(
            message.contains("cannot capture")
                && message.contains("a copy of an Atomic or Mutex would be a separate cell")
                && message.contains(
                    "capture a borrow of it, share it through an Arc, or pass it as an argument"
                ),
            "{message}"
        );
    }
    // The ways the message names are accepted.
    for source in [
        "let cell = Atomic.create 0i64\nlet borrowed = ref cell\nlet read = \\() -> Atomic.load borrowed\nread ()",
        "let cell = Arc.new (Atomic.create 0i64)\nlet read = \\() -> Atomic.load (Arc.get (ref cell))\nread ()",
        "let cell = Mutex.create 5i64\nlet read = \\shared -> Mutex.with_lock shared (\\value -> deref value)\nread (ref cell)",
    ] {
        emits(source);
    }
}

#[test]
fn a_cell_behind_an_arc_is_captured_in_a_recursive_type_as_in_any_other() {
    // The cell is shared through the Arc, so a copy of the function value shares it too, and the
    // type being recursive makes no difference.
    for source in [
        "let node = Node { value: 1, next: Maybe.None }\nlet add = \\x -> x + node.value\nadd 41",
        "let tail = Arc.new (Mutex.create (Node { value: 1, next: Maybe.None }))\nlet head = Node { value: 2, next: Maybe.Some (Arc.share (ref tail)) }\nlet add = \\x -> x + head.value\nadd 40",
    ] {
        emits(&format!("{NODE}{source}"));
    }
    emits(
        "record Link { hits: Arc<Atomic<i64>>, next: Maybe<Arc<Link>> }\nlet link = Link { hits: Arc.new (Atomic.create 0i64), next: Maybe.None }\nlet bump = \\() -> Atomic.fetch_add (Arc.get (ref link.hits)) 1\nbump ()",
    );
    // A cell that the record owns outright would still be copied with the function value.
    let message = rejects(
        "record Chain { hits: Atomic<i64>, next: Maybe<Arc<Chain>> }\nlet chain = Chain { hits: Atomic.create 0i64, next: Maybe.None }\nlet read = \\() -> Atomic.load (ref chain.hits)\nread ()",
        "E1005",
    );
    assert!(
        message.contains("a copy of an Atomic or Mutex would be a separate cell"),
        "{message}"
    );
    // An Rc in a recursive type is still refused, with the message about its counts.
    let message = rejects(
        "record Ring { value: i64, next: Maybe<Rc<Ring>> }\nlet ring = Ring { value: 1, next: Maybe.None }\nlet read = \\() -> ring.value\nread ()",
        "E1005",
    );
    assert!(
        message.contains("Rc counts its owners without atomic operations"),
        "{message}"
    );
}

#[test]
fn sync_marks_what_tasks_may_share() {
    for body in [
        "let values = [1i64, 2i64]\nshare (ref values) + share (ref \"text\") + share (ref 5i64)",
        "let cell = Atomic.create 0i64\nlet lock = Mutex.create 0i64\nshare (ref cell) + share (ref lock)",
        "let shared = Arc.new 1\nshare (ref shared)",
        "let function = \\value -> value + 1\nshare (ref function)",
        "record Counters { hits: Atomic<i64>, names: [string] }\nlet counters = Counters { hits: Atomic.create 0i64, names: [\"a\"] }\nshare (ref counters)",
    ] {
        emits(&format!("{SYNC_PARAMETER}{body}"));
    }
    let not_sync = "tasks can share only Sync values";
    for (body, ty) in [
        ("let counted = Rc.new 1\nshare (ref counted)", "Rc<i32>"),
        (
            "let counted = Arc.new (Rc.new 1)\nshare (ref counted)",
            "Arc<Rc<i32>>",
        ),
        (
            "let pending = task { return 1 }\nshare (ref pending)",
            "Task<i32>",
        ),
        (
            "let mut number = 5i64\nshare (ref mut number)",
            "ref mut i64",
        ),
        (
            "record Holder { counted: Rc<i64> }\nlet holder = Holder { counted: Rc.new 1 }\nshare (ref holder)",
            "Holder",
        ),
    ] {
        let message = rejects(
            &format!("def share :: Sync<'a> => 'a -> i64 = \\_value -> 1\n{body}"),
            "E1013",
        );
        assert!(
            message.contains(not_sync) && message.contains(&format!("{ty} is not Sync")),
            "{message}"
        );
    }
    let message = rejects(
        &format!("{HANDLE}{SYNC_PARAMETER}let counter = counter_new 1\nshare (ref counter)"),
        "E1013",
    );
    assert!(
        message.contains(not_sync) && message.contains("is not Sync"),
        "{message}"
    );
    // A dyn value is Sync only when it is Copy: a shared copy cannot be mutated through.
    let shared = format!("{SHAPE}{SYNC_PARAMETER}");
    emits(&format!(
        "{shared}let shape: dyn (Shape, Copy) = Dyn.of (Square {{ side: 3 }})\nshare (ref shape)"
    ));
    rejects(
        &format!(
            "{shared}let shape: dyn (Shape, Send) = Dyn.of (Square {{ side: 3 }})\nshare (ref shape)"
        ),
        "E1013",
    );
    // `Sync` is a class of the checker, not a mark that a dyn value carries.
    rejects(
        &format!("{SHAPE}let shape: dyn (Shape, Sync) = Dyn.of (Square {{ side: 3 }})\n0"),
        "E1028",
    );
    // The built-in classes cannot be given instances, and their names are taken.
    for (source, code) in [
        ("record P { x: i64 }\ninstance Sync<P> {}\n0", "E1016"),
        ("record P { x: i64 }\ninstance Send<P> {}\n0", "E1016"),
        (
            "record P { x: i64 }\ninstance AtomicValue<P> {}\n0",
            "E1016",
        ),
        ("record Sync { x: i64 }\n0", "E1001"),
        ("record AtomicValue { x: i64 }\n0", "E1001"),
    ] {
        rejects(source, code);
    }
}

#[test]
fn arc_shares_only_sync_values_between_tasks() {
    let sync_send = "Arc<Rc<i32>> holds an Rc or Rc.Weak, whose counts are not atomic";
    for body in [
        "let shared = Arc.new (Atomic.create 0i64)\nlet other = Arc.share (ref shared)\nTask.run (task { return Atomic.fetch_add (Arc.get (ref other)) 1 }) + Atomic.load (Arc.get (ref shared))",
        "let shared = Arc.new (Mutex.create 0i64)\nlet other = Arc.share (ref shared)\nTask.run (task { return Mutex.with_lock (Arc.get (ref other)) (\\value -> deref value) })",
        "let shared = Arc.new (Arc.new (Atomic.create 0i64))\nTask.run (task { return Arc.strong_count (ref shared) })",
        "record Counters { hits: Atomic<i64> }\nlet shared = Arc.new (Counters { hits: Atomic.create 0i64 })\nTask.run (task { return Atomic.load (ref (Arc.get (ref shared)).hits) })",
        "let shared = Arc.new (\\value -> value + 1)\nTask.run (task { return Arc.strong_count (ref shared) })",
        "let shared = Arc.new [1i64, 2i64]\nTask.run (task { return Array.sum (Arc.get (ref shared)) })",
    ] {
        emits(body);
    }
    // The recursive node holds a Mutex that holds the next Arc: cycles are possible, and typed.
    emits(&format!(
        "{NODE}let tail = Arc.new (Mutex.create (Node {{ value: 1, next: Maybe.None }}))\nlet head = Arc.new (Mutex.create (Node {{ value: 2, next: Maybe.Some (Arc.share (ref tail)) }}))\nTask.run (task {{ return Mutex.with_lock (Arc.get (ref head)) (\\node -> (deref node).value) }})"
    ));
    let message = rejects(
        "let shared = Arc.new (Rc.new 1)\nTask.run (task { return Arc.strong_count (ref shared) })",
        "E1013",
    );
    assert!(message.contains(sync_send), "{message}");
    for (source, shape) in [
        (
            "let shared = Arc.new (task { return 1 })\nTask.run (task { return Arc.strong_count (ref shared) })",
            "shares a task, an exclusive reference, a lazy sequence, an Async computation or a GPU handle through an Arc",
        ),
        (
            "let shared = Arc.new (async { return 1 })\nTask.run (task { return Arc.strong_count (ref shared) })",
            "shares a task, an exclusive reference, a lazy sequence, an Async computation or a GPU handle through an Arc",
        ),
    ] {
        let message = rejects(source, "E1013");
        assert!(message.contains(shape), "{message}");
    }
    let message = rejects(
        &format!(
            "{HANDLE}let shared = Arc.new (counter_new 1)\nTask.run (task {{ return Arc.strong_count (ref shared) }})"
        ),
        "E1013",
    );
    assert!(message.contains("shares an extern handle"), "{message}");
    // A task that holds an Arc of an Rc could drop it on another thread; so could a plain Rc.
    rejects(
        "let shared = Rc.new 1\nTask.run (task { return Rc.strong_count (ref shared) })",
        "E1013",
    );
}

// A type variable is not known to be Send, so `Send<'a>` is no answer for the function that is
// generic over it: the constraint stays on the function and is checked at every type that the
// function is used at, as `Sync<'a>` is. A wrapper of `Channel.bounded`, `Mutex.create` or a
// parallel operation, and a constraint that a program writes, must not let an `Rc` through.
#[test]
fn send_is_checked_at_the_types_a_generic_function_is_used_at() {
    let send = "tasks require Send values";
    let programs = |item: &str| {
        [
            // The channel is made by a wrapper, and the item is sent where the type is known.
            format!(
                "def make :: i64 -> (Channel.Sender<'a> * Channel.Receiver<'a>)\nfn make capacity = Channel.bounded capacity\nmatch make 2 with\n| (sender, _receiver) ->\n    let _sent = Channel.send (ref sender) ({item})\n    0"
            ),
            // The wrapper of a wrapper, with the ends kept in a record that tasks may share.
            format!(
                "record Pipe<'a> {{ sender: Channel.Sender<'a>, receiver: Channel.Receiver<'a> }}\ndef make :: i64 -> (Channel.Sender<'a> * Channel.Receiver<'a>)\nfn make capacity = Channel.bounded capacity\ndef open :: i64 -> Pipe<'a>\nfn open capacity =\n    match make capacity with\n    | (sender, receiver) -> Pipe {{ sender: sender, receiver: receiver }}\nlet pipe = open 4\nlet _sent = Channel.send (ref pipe.sender) ({item})\n0"
            ),
            format!(
                "def wrap :: 'a -> Mutex<'a>\nfn wrap value = Mutex.create value\nlet _lock = wrap ({item})\n0"
            ),
            // A constraint that the program writes.
            format!("def need :: Send<'a> => 'a -> i64\nfn need _value = 1\nneed ({item})"),
            // The same through a recursive type, which is judged by what it stores.
            format!(
                "union Tree<'a> = Leaf | Node of Tree<'a> * 'a * Tree<'a>\ndef need :: Send<'a> => 'a -> i64\nfn need _value = 1\ndef need_tree :: Tree<'a> -> i64\nfn need_tree tree = need tree\nneed_tree (Node (Leaf, {item}, Leaf))"
            ),
        ]
    };
    for source in programs("Rc.new 1i64") {
        let message = rejects(&source, "E1013");
        assert!(message.contains(send), "{source}\n{message}");
    }
    for source in programs("1i64") {
        emits(&source);
    }
    // What a constraint that stays generic still lets through, and what it still refuses.
    let need = "def need :: Send<'a> => 'a -> i64\nfn need _value = 1\n";
    for item in [
        "\"text\"",
        "[1i64, 2i64]",
        "(1i64, \"text\")",
        "Maybe.Some 1i64",
        "Arc.new (Atomic.create 0i64)",
        "Arc.new (Mutex.create [1i64])",
        "\\x -> x + 1i64",
        "task { return 1 }",
    ] {
        emits(&format!("{need}need ({item})"));
    }
    for item in [
        "Maybe.Some (Rc.new 1i64)",
        "(1i64, Rc.new 1i64)",
        "[Rc.new 1i64]",
        "Arc.new (Rc.new 1i64)",
    ] {
        let message = rejects(&format!("{need}need ({item})"), "E1013");
        assert!(message.contains(send), "{item}: {message}");
    }
    let message = rejects(
        &format!("{need}need (Arc.new (task {{ return 1 }}))"),
        "E1013",
    );
    assert!(message.contains(send), "{message}");
    let message = rejects(
        &format!("{need}let mut number = 1i64\nneed (ref number)"),
        "E1013",
    );
    assert!(message.contains("tasks require owned values"), "{message}");
    // A parallel operation whose result type the function takes from a class.
    let fill = |ty: &str| {
        format!(
            "class Make<'a> {{\n    def make :: i64 -> 'a\n}}\nrecord Holder {{ counted: Rc<i64> }}\ninstance Make<Holder> {{\n    fn make n = Holder {{ counted: Rc.new n }}\n}}\ninstance Make<i64> {{\n    fn make n = n\n}}\ndef fill :: Make<'a> => i64 -> ['a]\nfn fill count = Parallel.init count (\\index -> Make.make index)\nlet _built: [{ty}] = fill 2\n0"
        )
    };
    let message = rejects(&fill("Holder"), "E1013");
    assert!(message.contains(send), "{message}");
    emits(&fill("i64"));
}

#[test]
fn task_scope_shares_a_sync_borrow() {
    for source in [
        "let counter = Atomic.create 0i64\nlet seen = Task.scope (ref counter) 4 (\\shared index -> Atomic.fetch_add shared index)\nArray.length (ref seen)",
        "let table = [1i64, 2i64, 3i64]\nlet sums = Task.scope (ref table) 3 (\\shared index -> Array.sum shared + index)\nArray.sum (ref sums)",
        "let text = \"shared\"\nlet lengths = Task.scope (ref text) 2 (\\shared index -> shared.length + index)\nArray.sum (ref lengths)",
        "let shared = Arc.new (Atomic.create 0i64)\nlet seen = Task.scope (ref shared) 2 (\\arc index -> Atomic.fetch_add (Arc.get arc) index)\nArray.sum (ref seen)",
        "let lock = Mutex.create 0i64\nlet seen = Task.scope (ref lock) 4 (\\shared index -> Mutex.with_lock shared (\\value -> { deref value = deref value + index; deref value }))\nArray.length (ref seen)",
        "let seen = Task.scope (ref 5i64) 0 (\\shared index -> deref shared + index)\nArray.length (ref seen)",
        "let outer = Task.scope (ref 1i64) 2 (\\shared index -> Array.sum (ref (Task.scope shared 2 (\\inner position -> deref inner + position + index))))\nArray.sum (ref outer)",
    ] {
        emits(source);
    }
    let not_sync = "tasks can share only Sync values";
    for source in [
        "let counted = Rc.new 1\nlet seen = Task.scope (ref counted) 2 (\\_shared index -> index)\n0",
        "let pending = task { return 1 }\nlet seen = Task.scope (ref pending) 2 (\\_shared index -> index)\n0",
    ] {
        let message = rejects(source, "E1013");
        assert!(message.contains(not_sync), "{message}");
    }
    // Each child returns an owned Send value; the shared borrow cannot leave the scope.
    let message = rejects(
        "let counter = Atomic.create 0i64\nlet seen = Task.scope (ref counter) 4 (\\shared _index -> shared)\n0",
        "E1013",
    );
    assert!(message.contains("tasks require owned values"), "{message}");
    let message = rejects(
        "let counter = Atomic.create 0i64\nlet seen = Task.scope (ref counter) 4 (\\_shared index -> Rc.new index)\n0",
        "E1013",
    );
    assert!(message.contains("tasks require Send values"), "{message}");
    // The callback keeps no borrowed environment, as in every parallel operation.
    for source in [
        "let counter = Atomic.create 0i64\nlet other = Atomic.create 0i64\nlet borrowed = ref other\nlet seen = Task.scope (ref counter) 4 (\\_shared _index -> Atomic.load borrowed)\n0",
        "let counter = Atomic.create 0i64\nlet step = 5i64\nlet borrowed = ref step\nlet seen = Task.scope (ref counter) 4 (\\shared _index -> Atomic.fetch_add shared (deref borrowed))\n0",
    ] {
        let message = rejects(source, "E1013");
        assert!(
            message.contains("cannot retain borrowed environments"),
            "{message}"
        );
    }
    // The call is one ownership boundary and cannot be erased into a function value.
    let message = rejects(
        "let counter = Atomic.create 0i64\nlet partial: (ref Atomic<i64> -> i64 -> i64) -> [i64] = Task.scope (ref counter) 4\n0",
        "E1013",
    );
    assert!(
        message.contains("must be fully applied directly"),
        "{message}"
    );
}

// A function value that a scope shares runs in several tasks at once, so what it borrows must be
// shared too. The environment is proven to hold no borrow, and a temporary has to meet the same
// proof as a name does.
const BORROWING_CLOSURE: &str = "\\x -> { let again = Rc.share r; deref (Rc.get (ref again)) + x }";

#[test]
fn a_shared_temporary_must_hold_no_borrow_just_as_a_named_value_must_not() {
    let owned = "task scopes can share only values with proven owned environments";
    let prelude = "let rc = Rc.new 5i64\nlet r = ref rc\n";
    for source in [
        // The closure bound to a name first: refused (and was before).
        format!("{prelude}let f = {BORROWING_CLOSURE}\nlet seen = Task.scope (ref f) 4 (\\shared index -> (deref shared) index)\nArray.sum (ref seen)"),
        // The same closure as a temporary: its loan on the Rc has no parents, and was accepted.
        format!("{prelude}let seen = Task.scope (ref ({BORROWING_CLOSURE})) 4 (\\shared index -> (deref shared) index)\nArray.sum (ref seen)"),
        format!("record Holder {{ f: i64 -> i64 }}\n{prelude}let seen = Task.scope (ref (Holder {{ f: {BORROWING_CLOSURE} }})) 4 (\\shared index -> shared.f index)\nArray.sum (ref seen)"),
        format!("{prelude}let seen = Task.scope (ref [{BORROWING_CLOSURE}]) 4 (\\shared index -> (shared[0]) index)\nArray.sum (ref seen)"),
        format!("{prelude}let seen = Task.scope (ref (Maybe.Some ({BORROWING_CLOSURE}))) 4 (\\shared index -> match shared with | Maybe.Some f -> f index | Maybe.None -> 0)\nArray.sum (ref seen)"),
        // A borrow of a Sync place would be sound to share, but the rule is the one that a name
        // meets, whatever the pointee.
        "let counter = Atomic.create 0i64\nlet b = ref counter\nlet seen = Task.scope (ref (\\x -> Atomic.fetch_add b x)) 4 (\\shared index -> (deref shared) index)\nArray.sum (ref seen)".to_owned(),
    ] {
        let message = rejects(&source, "E1013");
        assert!(message.contains(owned), "{source}\n{message}");
    }
    // The elements that Parallel.map_ref shares between its tasks meet the same rule.
    let message = rejects(
        &format!(
            "{prelude}let seen = Parallel.map_ref (\\f -> (deref f) 1) (ref [{BORROWING_CLOSURE}])\nArray.sum (ref seen)"
        ),
        "E1013",
    );
    assert!(
        message.contains("parallel input elements must have proven owned environments"),
        "{message}"
    );
    // A temporary whose environment owns what it holds is shared as before.
    emits(
        "let base = 10i64\nlet seen = Task.scope (ref (\\x -> x + base)) 4 (\\shared index -> (deref shared) index)\nArray.sum (ref seen)",
    );
    emits(
        "let shared = Arc.new (Atomic.create 0i64)\nlet seen = Task.scope (ref (\\x -> Atomic.fetch_add (Arc.get (ref shared)) x)) 4 (\\f index -> (deref f) index)\nArray.sum (ref seen)",
    );
    analyze("let base = 10i64\nlet seen = Parallel.map_ref (\\f -> (deref f) 1) (ref [\\x -> x + base])\nArray.sum (ref seen)")
        .expect("an owned environment is shared");
}

#[test]
fn a_mutex_result_never_holds_a_borrow_of_the_locked_value() {
    let free = "Mutex.with_lock results must be proven free of borrowed environments";
    let escaping = |result: &str| {
        format!(
            "let lock = Mutex.create 5i64\nlet kept = Mutex.with_lock (ref lock) (\\value -> {{ let view = ref (deref value); {result} }})\n0"
        )
    };
    // A function value, alone or in a container, that reads the locked value after the lock is gone:
    // the value could have been replaced and freed, or be written by another task, by then.
    for result in [
        "\\x -> deref view + x",
        "Maybe.Some (\\x -> deref view + x)",
        "(1, \\x -> deref view + x)",
        "[\\x -> deref view + x]",
        "Reader { read: \\x -> deref view + x }",
        "Seq.unfold (\\n -> if n < 3 then Some (deref view + n, n + 1) else None) 0",
    ] {
        let source = format!("record Reader {{ read: i64 -> i64 }}\n{}", escaping(result));
        let message = rejects(&source, "E1013");
        assert!(message.contains(free), "{result}: {message}");
    }
    // The same through a view of a locked array.
    let message = rejects(
        "let lock = Mutex.create [1i64, 2i64, 3i64]\nlet view = Mutex.with_lock (ref lock) (\\values -> { let items = ref (deref values); \\i -> items[i] })\n0",
        "E1013",
    );
    assert!(message.contains(free), "{message}");
    // A function applied right away runs after the lock is released.
    let message = rejects(
        "let lock = Mutex.create 5i64\nMutex.with_lock (ref lock) (\\value -> { let view = ref (deref value); \\x -> deref view + x }) 1",
        "E1013",
    );
    assert!(message.contains(free), "{message}");
    // A named callback is proven by its body.
    let message = rejects(
        "def peek :: ref mut i64 -> (i64 -> i64)\nfn peek value =\n    let view = ref (deref value)\n    \\x -> deref view + x\nlet lock = Mutex.create 5i64\nlet kept = Mutex.with_lock (ref lock) peek\n0",
        "E1013",
    );
    assert!(message.contains(free), "{message}");
    // A child of a scope keeps the function after its own lock is released.
    let message = rejects(
        "let lock = Mutex.create 5i64\nlet seen = Task.scope (ref lock) 2 (\\shared index -> {\n    let kept = Mutex.with_lock shared (\\value -> { let view = ref (deref value); \\x -> deref view + x });\n    kept index\n})\nArray.sum (ref seen)",
        "E1013",
    );
    assert!(message.contains(free), "{message}");
    // The call has to be direct for its result to be checked, when the result may hold a borrow.
    for source in [
        "let lock = Mutex.create 5i64\nlet locker = Mutex.with_lock (ref lock)\nlet kept = locker (\\value -> { let view = ref (deref value); \\x -> deref view + x })\n0",
        "let lock = Mutex.create 5i64\nlet locker = Mutex.with_lock\nlet kept = locker (ref lock) (\\value -> { let view = ref (deref value); \\x -> deref view + x })\n0",
    ] {
        let message = rejects(source, "E1013");
        assert!(
            message.contains("must be fully applied directly"),
            "{message}"
        );
    }
    // What is owned stays allowed: a copy of the value, in a function or a container; a borrow that
    // the callback did not take from the locked value; a result that holds no borrow at all, however
    // the call is spelled.
    for source in [
        "let lock = Mutex.create 5i64\nlet adder = Mutex.with_lock (ref lock) (\\value -> { let copy = deref value; \\x -> copy + x })\nadder 1",
        "let lock = Mutex.create 5i64\nlet adder = Mutex.with_lock (ref lock) (\\value -> { let copy = deref value; Maybe.Some (\\x -> copy + x) })\nmatch adder with\n| Maybe.Some f -> f 1\n| Maybe.None -> 0",
        "let lock = Mutex.create 5i64\nMutex.with_lock (ref lock) (\\value -> { let copy = deref value; \\x -> copy + x }) 1",
        "let other = 7i64\nlet outside = ref other\nlet lock = Mutex.create 5i64\nlet adder = Mutex.with_lock (ref lock) (\\value -> { let copy = deref value; \\x -> copy + deref outside + x })\nadder 1",
        "let lock = Mutex.create 5i64\nlet locker = Mutex.with_lock (ref lock)\nlocker (\\value -> deref value)",
        "let lock = Mutex.create 5i64\nlet locker = Mutex.with_lock\nlocker (ref lock) (\\value -> deref value + 1)",
        "let lock = Mutex.create [1i64, 2i64, 3i64]\nlet seen = Task.scope (ref lock) 2 (\\shared index -> Mutex.with_lock shared (\\values -> { let items = ref (deref values); items[index] }))\nArray.sum (ref seen)",
    ] {
        emits(source);
    }
}

#[test]
fn mutex_programs_type_check_and_the_closure_result_is_owned() {
    for source in [
        "let lock = Mutex.create [1i64, 2i64]\nMutex.with_lock (ref lock) (\\values -> Array.length (deref values))",
        "let lock = Mutex.create 5i64\nlet seen = Mutex.with_lock (ref lock) (\\value -> { deref value = deref value + 1; deref value })\nseen + Mutex.into_inner lock",
        "record Totals { count: i64, sum: i64 }\nlet lock = Mutex.create (Totals { count: 0, sum: 0 })\nMutex.with_lock (ref lock) (\\totals -> { deref totals = Totals { count: (deref totals).count + 1, sum: (deref totals).sum + 5 }; (deref totals).sum })",
        "let outer = Mutex.create 1i64\nlet inner = Mutex.create 2i64\nlet borrowed = ref inner\nMutex.with_lock (ref outer) (\\_a -> Mutex.with_lock borrowed (\\b -> deref b))",
        "let lock = Mutex.create 0i64\nTask.run (task { return Mutex.with_lock (ref lock) (\\value -> deref value) })",
        "let lock = Mutex.create \"text\"\nMutex.with_lock (ref lock) (\\text -> (deref text).length)",
    ] {
        emits(source);
    }
    // A reference to the protected value would outlive the lock.
    for source in [
        "let lock = Mutex.create 0i64\nMutex.with_lock (ref lock) (\\value -> value)",
        "let lock = Mutex.create [1i64]\nMutex.with_lock (ref lock) (\\values -> ref (deref values))",
    ] {
        let message = rejects(source, "E1013");
        assert!(message.contains("tasks require owned values"), "{message}");
    }
    let message = rejects("let lock = Mutex.create (Rc.new 1)\n0", "E1013");
    assert!(message.contains("tasks require Send values"), "{message}");
    let message = rejects(
        "let lock = Mutex.create 0i64\nMutex.with_lock (ref lock) (\\_value -> Rc.new 1)",
        "E1013",
    );
    assert!(message.contains("tasks require Send values"), "{message}");
}

// An item stays in a channel after the call that sent it, and it is received later, perhaps on
// another thread, perhaps after the data that it borrows is gone. `Sender<T>` stores one integer,
// so the loan analysis cannot see the item type through it: the call that stores the item is what
// is checked, as the result of `Mutex.with_lock` is.
#[test]
fn a_channel_item_never_holds_a_borrow() {
    let held = "a channel item cannot hold a borrow";
    let staged = "Channel.send must be fully applied directly";
    let borrowing = "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let table = [40i64, 41i64]\n    let view = ref table\n";
    let received = "    match Channel.recv (ref receiver) with\n    | Maybe.Some _item -> 1\n    | Maybe.None -> 0";
    let sends = |item: &str| {
        format!(
            "record Reader {{ read: i64 -> i64 }}\n{borrowing}    let _sent = Channel.send (ref sender) ({item})\n{received}"
        )
    };
    // A function, alone or in a container, that reads borrowed data after the data is gone.
    for item in [
        "\\x -> x + view[0]",
        "Maybe.Some (\\x -> x + view[0])",
        "(1i64, \\x -> x + view[0])",
        "[\\x -> x + view[0]]",
        "Reader { read: \\x -> x + view[0] }",
        "Seq.unfold (\\n -> if n < 3 then Some (view[0] + n, n + 1) else None) 0",
    ] {
        let message = rejects(&sends(item), "E1013");
        assert!(message.contains(held), "{item}: {message}");
    }
    // The function bound to a name first, and a borrow of a parameter, which a caller lent.
    let message = rejects(
        &format!(
            "{borrowing}    let f = \\x -> x + view[0]\n    let _sent = Channel.send (ref sender) f\n{received}"
        ),
        "E1013",
    );
    assert!(message.contains(held), "{message}");
    let reaching = "def work :: ref Channel.Sender<i64 -> i64> -> ref [i64] -> i64\nfn work sender data =\n    match Channel.send sender (\\x -> x + data[0]) with\n    | Result.Ok _unit -> 0\n    | Result.Error _item -> 1\n\ndef producer :: ref Channel.Sender<i64 -> i64> -> i64\nfn producer sender =\n    let local = [40, 41]\n    work sender (ref local)\n\ndef clobber :: i64 -> i64\nfn clobber n =\n    let a = [n, n, n, n]\n    let b = [n + 1, n + 1, n + 1, n + 1]\n    a[0] + b[1]\n\nmatch Channel.bounded 2 with\n| (sender, receiver) ->\n    let _produced = producer (ref sender)\n    let _clobbered = clobber 1000\n    match Channel.recv (ref receiver) with\n    | Maybe.Some f -> f 1\n    | Maybe.None -> -1";
    let message = rejects(reaching, "E1013");
    assert!(message.contains(held), "{message}");
    // The function is a child's own, and the record that holds the end looks loan-free.
    let message = rejects(
        "record Pipe { sender: Channel.Sender<i64 -> i64>, receiver: Channel.Receiver<i64 -> i64> }\ndef child :: ref Pipe -> i64 -> i64\nfn child shared index =\n    let local = [index, 1]\n    let view = ref local\n    match Channel.send (ref shared.sender) (\\x -> x + view[0]) with\n    | Result.Ok _unit -> 0\n    | Result.Error _item -> 1\nmatch Channel.bounded 2 with\n| (sender, receiver) ->\n    let pipe = Pipe { sender: sender, receiver: receiver }\n    let seen = Task.scope (ref pipe) 2 child\n    Array.sum (ref seen)",
        "E1013",
    );
    assert!(message.contains(held), "{message}");
    // The call has to be direct, with both arguments, for its item to be checked when the item may
    // hold a borrow.
    let delayed = |statement: &str| format!("{borrowing}{statement}\n    0");
    for source in [
        delayed(
            "    let send = Channel.send (ref sender)\n    let _sent = send (\\x -> x + view[0])",
        ),
        delayed(
            "    let send = Channel.send\n    let _sent = send (ref sender) (\\x -> x + view[0])",
        ),
        delayed("    let _sent = (\\x -> x + view[0]) |> Channel.send (ref sender)"),
    ] {
        let message = rejects(&source, "E1013");
        assert!(message.contains(staged), "{source}\n{message}");
    }
    // A function that forwards the item of its parameter may be given a borrow.
    let push = "def push :: ref Channel.Sender<'a> -> 'a -> bool\nfn push sender item =\n    match Channel.send sender item with\n    | Result.Ok _unit -> true\n    | Result.Error _item -> false\n";
    let message = rejects(
        &format!("{push}{borrowing}    if push (ref sender) (\\x -> x + view[0]) then 1 else 0"),
        "E1013",
    );
    assert!(message.contains(held), "{message}");
    // What holds no borrow still goes through: data, a function that captured copies, one that
    // captures nothing or is named, and one that an Owned.Function owns, however the call is made.
    for source in [
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let k = 41i64\n    let _sent = Channel.send (ref sender) (\\x -> x + k)\n    match Channel.recv (ref receiver) with\n    | Maybe.Some f -> f 1\n    | Maybe.None -> 0",
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) (\\x -> x + 1)\n    match Channel.recv (ref receiver) with\n    | Maybe.Some f -> f 1\n    | Maybe.None -> 0",
        "def add_one :: i64 -> i64\nfn add_one x = x + 1\nmatch Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) add_one\n    match Channel.recv (ref receiver) with\n    | Maybe.Some f -> f 1\n    | Maybe.None -> 0",
        "def offer :: ref Channel.Sender<i64 -> i64> -> i64 -> i64\nfn offer sender k =\n    match Channel.send sender (\\x -> x + k) with\n    | Result.Ok _unit -> 0\n    | Result.Error _item -> 1\nmatch Channel.bounded 2 with\n| (sender, receiver) ->\n    let _offered = offer (ref sender) 41\n    match Channel.recv (ref receiver) with\n    | Maybe.Some f -> f 1\n    | Maybe.None -> 0",
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let k = 41i64\n    let _sent = Channel.send (ref sender) (Owned.function (\\x -> x + k))\n    match Channel.recv (ref receiver) with\n    | Maybe.Some f -> Owned.call (ref f) 1\n    | Maybe.None -> 0",
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) (Maybe.Some \"text\")\n    match Channel.recv (ref receiver) with\n    | Maybe.Some (Maybe.Some text) -> text.length\n    | _ -> 0",
        // A generic function that forwards an item that is not a function.
        &format!(
            "{push}match Channel.bounded 2 with\n| (sender, _receiver) ->\n    if push (ref sender) 5i64 then 1 else 0"
        ),
        // The item of a call through a function value, when it cannot hold a borrow.
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let send = Channel.send (ref sender)\n    let _sent = send 3i64\n    match Channel.recv (ref receiver) with\n    | Maybe.Some value -> value\n    | Maybe.None -> 0",
    ] {
        emits(source);
    }
    // What a channel returns holds no borrow either, so it may be sent on or returned.
    emits(
        "match Channel.bounded 2 with\n| (first, middle) ->\n    match Channel.bounded 2 with\n    | (second, last) ->\n        let k = 41i64\n        let _sent = Channel.send (ref first) (\\x -> x + k)\n        let _moved = match Channel.recv (ref middle) with\n            | Maybe.Some f -> Channel.send (ref second) f\n            | Maybe.None -> Result.Ok ()\n        match Channel.recv (ref last) with\n        | Maybe.Some f -> f 1\n        | Maybe.None -> 0",
    );
    emits(
        "def next :: ref Channel.Receiver<i64 -> i64> -> Maybe<i64 -> i64>\nfn next receiver = Channel.recv receiver\nmatch Channel.bounded 2 with\n| (sender, receiver) ->\n    let k = 41i64\n    let _sent = Channel.send (ref sender) (\\x -> x + k)\n    match next (ref receiver) with\n    | Maybe.Some f -> f 1\n    | Maybe.None -> 0",
    );
    // A lock keeps its value as long as it lives, so a borrow in it stays on the lock, which
    // cannot be returned or shared by a scope, as with any other value that holds a function.
    for source in [
        "def make :: ref [i64] -> Mutex<i64 -> i64>\nfn make data = Mutex.create (\\x -> x + data[0])\ndef leak :: i64 -> Mutex<i64 -> i64>\nfn leak n =\n    let local = [n, n]\n    make (ref local)\nlet _kept = leak 5i64\n0",
        "def keep :: ref [i64] -> i64\nfn keep data =\n    let lock = Mutex.create (\\x -> x + data[0])\n    let seen = Task.scope (ref lock) 2 (\\shared index -> Mutex.with_lock shared (\\f -> (deref f) index))\n    Array.sum (ref seen)\nlet local = [40i64, 41i64]\nkeep (ref local)",
    ] {
        rejects(source, "E1013");
    }
    emits(
        "def keep :: ref [i64] -> i64\nfn keep data =\n    let lock = Mutex.create (\\x -> x + data[0])\n    Mutex.with_lock (ref lock) (\\f -> (deref f) 1)\nlet local = [40i64, 41i64]\nkeep (ref local)",
    );
}

#[test]
fn atomics_lower_to_sequentially_consistent_instructions() {
    for ir in emits(OPERATIONS) {
        for instruction in [
            "atomicrmw add ptr",
            "atomicrmw sub ptr",
            "atomicrmw and ptr",
            "atomicrmw or ptr",
            "atomicrmw xor ptr",
            "atomicrmw xchg ptr",
            "cmpxchg ptr",
            "load atomic i64, ptr",
            "store atomic i64",
        ] {
            assert!(ir.contains(instruction), "{instruction}\n{ir}");
        }
        for line in ir.lines().filter(|line| {
            ["atomicrmw ", "cmpxchg ", "load atomic ", "store atomic "]
                .iter()
                .any(|operation| line.trim_start().contains(operation))
        }) {
            assert!(
                line.contains(" seq_cst"),
                "every atomic operation is seq_cst: {line}"
            );
        }
        assert!(ir.contains("seq_cst, align 8"), "{ir}");
        assert!(
            !ir.contains("tz.mutex") && !ir.contains("tsuzuri_mutex"),
            "atomics need no lock"
        );
    }
    for ty in ["i8", "i16", "i32", "i64"] {
        let [native, _] = emits(&format!(
            "let cell = Atomic.create 1{ty}\nlet _a = Atomic.fetch_add (ref cell) 1{ty}\nAtomic.load (ref cell) as i64"
        ));
        let bits = &ty[1..];
        assert!(
            native.contains("atomicrmw add ptr")
                && native.contains(&format!("load atomic i{bits}, ptr")),
            "{ty}\n{native}"
        );
    }
    let [native, _] = emits(
        "let flag = Atomic.create true\nlet _was = Atomic.swap (ref flag) false\nAtomic.load (ref flag)",
    );
    assert!(
        native.contains("load atomic i8, ptr") && native.contains("atomicrmw xchg ptr"),
        "a bool is one byte in the cell\n{native}"
    );
}

#[test]
fn mutex_lowering_adds_no_runtime_to_other_programs() {
    let plain = "let values = Task.run (Task.parallel [task { return 1 }, task { return 2 }])\nArray.sum (ref values)";
    for ir in emits(plain) {
        assert!(
            !ir.contains("tz.mutex") && !ir.contains("tsuzuri_mutex"),
            "no Mutex, no lock\n{ir}"
        );
    }
    let locked = "let lock = Mutex.create 0i64\nlet held = Mutex.with_lock (ref lock) (\\value -> deref value)\n";
    let [native, wasm] = emits(&format!("{locked}held"));
    assert!(
        native.contains("declare i32 @tsuzuri_mutex_lock(ptr)")
            && native.contains("declare void @tsuzuri_mutex_unlock(ptr)"),
        "{native}"
    );
    assert!(
        wasm.contains("define internal i32 @tsuzuri_mutex_lock(ptr %cell)")
            && !wasm.contains("declare i32 @tsuzuri_mutex_lock"),
        "standalone WASM takes the lock in IR\n{wasm}"
    );
    assert!(
        !wasm.contains("tsuzuri_task_parallel") && !wasm.contains("tz.mutex.parallel"),
        "no parallel work, no wrapper\n{wasm}"
    );
    // With parallel work in the program, a start inside a lock is refused by the wrappers.
    let with_work = format!(
        "{locked}let values = Task.run (Task.parallel [task {{ return 1 }}, task {{ return 2 }}])\nheld + Array.sum (ref values)"
    );
    for ir in emits(&with_work) {
        assert!(
            ir.contains("call void @tz.mutex.parallel(")
                || ir.contains("call i64 @tz.mutex.parallel_results("),
            "{ir}"
        );
        // The one direct call of each entry is inside its wrapper.
        assert!(
            ir.matches("call void @tsuzuri_task_parallel(").count() <= 1,
            "calls go through the wrappers\n{ir}"
        );
        assert!(
            ir.matches("call i64 @tsuzuri_task_parallel_results(")
                .count()
                <= 1,
            "calls go through the wrappers\n{ir}"
        );
    }
}

/// The length operand of each call that starts a group of items in the IR of a program.
fn group_lengths(ir: &str) -> Vec<&str> {
    ir.lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("call void @tsuzuri_task_parallel("))
        .map(|line| {
            let (_, length) = line.rsplit_once("i64 ").expect(line);
            length.strip_suffix(')').expect(line)
        })
        .collect()
}

#[test]
fn task_scope_is_one_group_of_children() {
    let source = "let counter = Atomic.create 0i64\nlet seen = Task.scope (ref counter) 4 (\\shared index -> Atomic.fetch_add shared index)\nArray.length (ref seen)";
    let [native, wasm] = emits(source);
    for ir in [&native, &wasm] {
        assert!(ir.contains("@tsuzuri_task_parallel("), "{ir}");
        assert!(!ir.contains("tz.mutex"), "{ir}");
    }
    assert!(
        native.contains("declare void @tsuzuri_task_parallel("),
        "{native}"
    );
    assert!(
        wasm.contains("define internal void @tsuzuri_task_parallel("),
        "standalone WASM runs the children in index order\n{wasm}"
    );
    // One item per child, so that any thread may take any child and the pool hands them out one
    // at a time: the group is as long as the count, never a number of chunks of it.
    for ir in [&native, &wasm] {
        assert_eq!(group_lengths(ir), ["4"], "{ir}");
    }
    let [native, wasm] = emits(
        "let counter = Atomic.create 0i64\nlet few = Task.scope (ref counter) 3 (\\shared index -> Atomic.fetch_add shared index)\nlet many = Task.scope (ref counter) 5000 (\\shared index -> Atomic.fetch_add shared index)\nArray.length (ref few) + Array.length (ref many)",
    );
    for ir in [&native, &wasm] {
        assert_eq!(group_lengths(ir), ["3", "5000"], "{ir}");
    }
    // A count that is not known is the same value that sizes the array of results: one slot, one child.
    let [native, wasm] = emits(
        "def spread :: i64 -> i64\nfn spread n =\n    let counter = Atomic.create 0i64\n    let seen = Task.scope (ref counter) n (\\shared index -> Atomic.fetch_add shared index)\n    Array.length (ref seen)\nspread 6",
    );
    for ir in [&native, &wasm] {
        let lengths = group_lengths(ir);
        assert_eq!(lengths.len(), 1, "{ir}");
        assert!(
            ir.lines().any(|line| {
                line.contains("insertvalue %tz.array")
                    && line.ends_with(&format!(", i64 {}, 1", lengths[0]))
            }),
            "the length of the group is the length of the results ({})\n{ir}",
            lengths[0]
        );
    }
}

// F10 Phase 2: Channel.

const CHANNEL_ROUNDTRIP: &str = "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) 7i64\n    match Channel.recv (ref receiver) with\n    | Maybe.Some value -> value\n    | Maybe.None -> 0";

#[test]
fn channel_programs_type_check_and_emit() {
    for source in [
        CHANNEL_ROUNDTRIP,
        // A sender that is cloned, and a send that is refused hands the item back.
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let clone = Channel.clone_sender (ref sender)\n    let _sent = Channel.send (ref clone) 7i64\n    match Channel.send (ref sender) 8i64 with\n    | Result.Ok _unit -> 1\n    | Result.Error item -> item",
        // Items that own memory, are unit, or are tuples.
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) \"text\"\n    match Channel.recv (ref receiver) with\n    | Maybe.Some text -> text.length\n    | Maybe.None -> 0",
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) ()\n    match Channel.recv (ref receiver) with\n    | Maybe.Some _signal -> 1\n    | Maybe.None -> 0",
        "match Channel.bounded 3 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) (1i64, \"text\")\n    match Channel.recv (ref receiver) with\n    | Maybe.Some _pair -> 1\n    | Maybe.None -> 0",
        // The ends in a record, and a task that owns an end.
        "record Ends { sender: Channel.Sender<i64>, receiver: Channel.Receiver<i64> }\nmatch Channel.bounded 2 with\n| (sender, receiver) ->\n    let ends = Ends { sender: sender, receiver: receiver }\n    let _sent = Channel.send (ref ends.sender) 3i64\n    match Channel.recv (ref ends.receiver) with\n    | Maybe.Some value -> value\n    | Maybe.None -> 0",
        "def produce :: Channel.Sender<i64> -> Task<i64>\nfn produce sender = task {\n    let _sent = Channel.send (ref sender) 5i64\n    return 1\n}\nmatch Channel.bounded 2 with\n| (sender, receiver) ->\n    let results = Task.run (Task.parallel [produce sender])\n    match Channel.recv (ref receiver) with\n    | Maybe.Some value -> value + results[0]\n    | Maybe.None -> 0",
        // Both ends are Sync: a scope borrows them, and an Arc of an end goes to tasks.
        "match Channel.bounded 4 with\n| (sender, receiver) ->\n    let seen = Task.scope (ref sender) 2 (\\shared index -> match Channel.send shared index with | Result.Ok _unit -> 1 | Result.Error _item -> 0)\n    let _first = Channel.recv (ref receiver)\n    Array.sum (ref seen)",
        "def count :: Arc<Channel.Receiver<i64>> -> Task<i64>\nfn count receiver = task {\n    match Channel.recv (Arc.get (ref receiver)) with\n    | Maybe.Some value -> value\n    | Maybe.None -> 0\n}\nmatch Channel.bounded 2 with\n| (sender, receiver) ->\n    let shared = Arc.new receiver\n    let _sent = Channel.send (ref sender) 4i64\n    let results = Task.run (Task.parallel [count (Arc.share (ref shared))])\n    results[0]",
        // A channel operation inside a lock type-checks, and traps when it runs: a critical section never waits.
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let lock = Mutex.create 0i64\n    let borrowed = ref sender\n    let _seen = Mutex.with_lock (ref lock) (\\_value -> match Channel.send borrowed 1i64 with | Result.Ok _unit -> 1 | Result.Error _item -> 0)\n    match Channel.recv (ref receiver) with\n    | Maybe.Some value -> value\n    | Maybe.None -> 0",
    ] {
        emits(source);
    }
}

#[test]
fn channel_ends_are_opaque_owned_and_sync() {
    // The representation is not the program's: a handle cannot be made, read or updated, and the
    // primitive that closes a channel is the Channel module's alone.
    for source in [
        "let sender = Channel.Sender { block: 1 }\n0",
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) 1i64\n    let _taken = Channel.recv (ref receiver)\n    sender.block",
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) 1i64\n    let _taken = Channel.recv (ref receiver)\n    let other = { sender with block: 3 }\n    0",
    ] {
        let message = rejects(source, "E1022");
        assert!(
            message.contains("is opaque; use its module API"),
            "{message}"
        );
    }
    let message = rejects(
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) 1i64\n    let _taken = Channel.recv (ref receiver)\n    let _closed = Channel.__close_sender (ref sender)\n    0",
        "E1022",
    );
    assert!(
        message.contains("the channel close primitive is private to the standard Channel module")
            && message.contains("closes when it is dropped"),
        "{message}"
    );
    // A user Drop instance for a type of the standard library stays refused: the Channel module
    // alone closes the ends it declares.
    let message = rejects(
        "instance Drop<Channel.Sender<i64>> {\n    fn drop sender = ()\n}\n0",
        "E1016",
    );
    assert!(
        message.contains("only records and unions declared in this program can implement Drop"),
        "{message}"
    );
    // An end is owned: a copy would close the channel twice, so it moves, and a function value
    // (which may be copied) holds a borrow of it, never the end.
    rejects(
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let other = sender\n    let _sent = Channel.send (ref sender) 1i64\n    let _taken = Channel.recv (ref receiver)\n    0",
        "E1012",
    );
    for end in ["sender", "receiver"] {
        let source = format!(
            "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) 1i64\n    let _taken = Channel.recv (ref receiver)\n    let use = \\_x -> {end}\n    0"
        );
        let message = rejects(&source, "E1005");
        assert!(
            message.contains("cannot capture Channel."),
            "{end}: {message}"
        );
    }
    emits(
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let borrowed = ref sender\n    let send = \\x -> Channel.send borrowed x\n    let _taken = Channel.recv (ref receiver)\n    match send 1i64 with\n    | Result.Ok _unit -> 1\n    | Result.Error _item -> 0",
    );
    // Only what may move between tasks goes in: an Rc has no atomic count.
    let message = rejects(
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) (Rc.new 1i64)\n    0",
        "E1013",
    );
    assert!(message.contains("tasks require Send values"), "{message}");
    // Sync: both ends may be shared by tasks.
    emits(&format!(
        "{SYNC_PARAMETER}match Channel.bounded 3 with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) 1i64\n    let _taken = Channel.recv (ref receiver)\n    share (ref sender) + share (ref receiver)"
    ));
    // The name is taken only by the module: a program may declare its own Channel record.
    emits("record Channel { x: i64 }\nlet channel = Channel { x: 1 }\nchannel.x");
}

#[test]
fn channel_lowering_calls_the_runtime_and_closes_by_drop() {
    let [native, wasm] = emits(CHANNEL_ROUNDTRIP);
    // Native links the runtime of task.c, so the IR only declares it.
    for declaration in [
        "declare i32 @tsuzuri_channel_send(ptr, ptr)",
        "declare i32 @tsuzuri_channel_recv(ptr, ptr)",
        "declare void @tsuzuri_channel_clone_sender(ptr)",
        "declare i32 @tsuzuri_channel_close(ptr, i32)",
        "declare i32 @tsuzuri_mutex_wait_ok()",
    ] {
        assert_eq!(
            native.lines().filter(|line| *line == declaration).count(),
            1,
            "{declaration}\n{native}"
        );
    }
    // Standalone WASM has one thread: the operations are IR in the module, and a wait that cannot
    // end is a trap.
    for definition in [
        "define internal i32 @tsuzuri_channel_send(",
        "define internal i32 @tsuzuri_channel_recv(",
        "define internal void @tsuzuri_channel_clone_sender(",
        "define internal i32 @tsuzuri_channel_close(",
        "define internal i32 @tsuzuri_mutex_wait_ok(",
    ] {
        assert!(wasm.contains(definition), "{definition}\n{wasm}");
    }
    assert!(!wasm.contains("declare i32 @tsuzuri_channel"), "{wasm}");
    // A sender and a receiver that go out of scope are released with the kind of each, and a send
    // or a receive asks first whether a channel operation may wait here.
    for ir in [&native, &wasm] {
        assert!(
            ir.contains("call i32 @tsuzuri_channel_close(ptr %v2, i32 0)"),
            "{ir}"
        );
        assert!(
            ir.contains("call i32 @tsuzuri_channel_close(ptr %v2, i32 1)"),
            "{ir}"
        );
        assert!(ir.contains("call i32 @tsuzuri_mutex_wait_ok()"), "{ir}");
        assert!(ir.contains("call i32 @tsuzuri_channel_send("), "{ir}");
        assert!(ir.contains("call i32 @tsuzuri_channel_recv("), "{ir}");
    }
    let [native, _] = emits(
        "match Channel.bounded 2 with\n| (sender, receiver) ->\n    let clone = Channel.clone_sender (ref sender)\n    let _sent = Channel.send (ref clone) 7i64\n    match Channel.recv (ref receiver) with\n    | Maybe.Some value -> value\n    | Maybe.None -> 0",
    );
    assert!(
        native.contains("call void @tsuzuri_channel_clone_sender("),
        "{native}"
    );
    // Without a mention of Channel, no runtime and no lowering is added, whatever else is used.
    for source in [
        "let values = Task.run (Task.parallel [task { return 1 }, task { return 2 }])\nArray.sum (ref values)",
        "let lock = Mutex.create 0i64\nMutex.with_lock (ref lock) (\\value -> deref value)",
        "let counter = Atomic.create 0i64\nlet seen = Task.scope (ref counter) 2 (\\shared index -> Atomic.fetch_add shared index)\nArray.length (ref seen)",
    ] {
        for ir in emits(source) {
            assert!(
                !ir.contains("tsuzuri_channel") && !ir.contains("tz.channel"),
                "no Channel, no channel runtime\n{ir}"
            );
        }
    }
}
